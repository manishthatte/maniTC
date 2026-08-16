// bench.rs — Benchmarking command implementation for the maniT compiler.
use std::path::PathBuf;

use manitc::error::{CompileError, CompileResult, Diagnostic};
use manitc::lexer::Lexer;
use manitc::parser::Parser as ManiParser;
use manitc::semantic::SemanticAnalyzer;
use manitc::ir::{self, IRLowerer};
use manitc::codegen_llvm;
use manitc::codegen_t3;

pub fn read_source(path: &PathBuf) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {}", path.display(), e))
}

pub fn run_bench(file: &PathBuf, iterations: usize) -> CompileResult<()> {
    use std::time::Instant;

    let source = read_source(file).map_err(|e| CompileError::Lex(
        Diagnostic::unknown(e),
    ))?;
    let file_str = file.to_string_lossy().to_string();

    // ---- Compile pipeline (shared) ----
    let mut lexer = Lexer::with_file(&source, &file_str);
    let tokens = lexer.tokenize()?;
    let mut parser = ManiParser::with_file(tokens, &file_str);
    let program = parser.parse()?;
    let mut analyzer = SemanticAnalyzer::with_file(&file_str);
    let typed_program = analyzer.analyze(&program)?;

    // Emit warnings with source context
    analyzer.warnings.emit_all_rich(&source);

    // Borrow / move checking
    manitc::borrow::check_borrows(&typed_program)?;

    let mut ir_module = IRLowerer::lower(&typed_program);
    ir::optimize::run_passes(&mut ir_module);

    println!("===============================================================================");
    println!("  maniT Benchmark: {}", file.display());
    println!("===============================================================================");
    println!();

    // ---- T3ISA target ----
    let t3_asm = codegen_t3::emit_t3_asm(&ir_module);
    let t3_asm_lines = t3_asm.lines().count();

    let (t3_words, t3_str_data, t3_float_data) = match codegen_t3::assemble(&t3_asm) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[T3ISA] assembler error: {}", e);
            return Ok(());
        }
    };
    let t3_program_words = t3_words.len();

    // Run T3ISA with profiling
    let mut t3_total_time = std::time::Duration::ZERO;
    let mut t3_profile = None;
    for _ in 0..iterations {
        let start = Instant::now();
        let (_output, profile) = codegen_t3::run_emulator_profiled(
            t3_words.clone(), t3_str_data.clone(), t3_float_data.clone(),
        );
        t3_total_time += start.elapsed();
        t3_profile = Some(profile);
    }
    let t3_avg_time = t3_total_time / iterations as u32;
    let t3_prof = t3_profile.unwrap();

    // ---- LLVM target ----
    let ll_text = codegen_llvm::emit_llvm_ir(&ir_module, None);
    let ll_lines = ll_text.lines().count();
    let ll_path = std::path::PathBuf::from("/tmp/manitc_bench.ll");
    let bin_path = std::path::PathBuf::from("/tmp/manitc_bench_bin");
    std::fs::write(&ll_path, &ll_text).ok();

    // Resolve runtime
    let runtime_c_path = match manitc::runtime_link::resolve_source(Some(file)) {
        Ok(p) => p,
        Err(e) => {
            return Err(CompileError::Codegen(Diagnostic::unknown(format!(
                "failed to write the embedded C runtime: {}", e))));
        }
    };
    let link = manitc::runtime_link::flags();

    // Compile LLVM binary
    let runtime_obj = manitc::runtime_link::object_path("bench");
    let _ = std::process::Command::new("clang")
        .args([runtime_c_path.to_str().unwrap(), "-c", "-O2",
               "-o", runtime_obj.to_str().unwrap()])
        .args(&link.cflags)
        .status();

    let llvm_compiled = std::process::Command::new("clang")
        .args([ll_path.to_str().unwrap(), runtime_obj.to_str().unwrap(),
               "-O2", "-o", bin_path.to_str().unwrap(), "-lm", "-lpthread"])
        .args(&link.libs)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let mut llvm_avg_time = std::time::Duration::ZERO;
    let mut llvm_binary_size = 0u64;

    if llvm_compiled {
        if let Ok(meta) = std::fs::metadata(&bin_path) {
            llvm_binary_size = meta.len();
        }

        // Run LLVM binary
        let mut llvm_total_time = std::time::Duration::ZERO;
        for _ in 0..iterations {
            let start = Instant::now();
            let _ = std::process::Command::new(bin_path.to_str().unwrap())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            llvm_total_time += start.elapsed();
        }
        llvm_avg_time = llvm_total_time / iterations as u32;
    }

    // Count LLVM IR instructions (approximate: lines with = or store or ret or br)
    let llvm_ir_instrs = ll_text.lines()
        .filter(|l| {
            let t = l.trim();
            t.contains(" = ") || t.starts_with("store ") || t.starts_with("ret ") ||
            t.starts_with("br ") || t.starts_with("call ")
        })
        .count();

    // ---- x86-64 native instruction count via objdump ----
    let x86_instr_count = if llvm_compiled {
        std::process::Command::new("objdump")
            .args(["-d", "--no-show-raw-insn", bin_path.to_str().unwrap()])
            .output()
            .map(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter(|l| {
                        let t = l.trim();
                        // objdump lines with instructions: start with hex address, colon, then mnemonic
                        t.len() > 2 && t.as_bytes()[0].is_ascii_hexdigit() && t.contains(':')
                    })
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };

    // ---- Report ----
    println!("  COMPILATION");
    println!("  ───────────────────────────────────────────────────────────────────");
    println!("  {:30} {:>15} {:>15}", "Metric", "T3ISA (ternary)", "LLVM (binary)");
    println!("  {:30} {:>15} {:>15}", "──────", "───────────────", "─────────────");
    println!("  {:30} {:>15} {:>15}", "Assembly lines",
        t3_asm_lines, ll_lines);
    println!("  {:30} {:>15} {:>15}", "Code size (words/bytes)",
        format!("{} words", t3_program_words),
        if llvm_compiled { format!("{} bytes", llvm_binary_size) } else { "N/A".into() });
    println!("  {:30} {:>15} {:>15}", "Word width",
        "27 trits", "64 bits");
    println!("  {:30} {:>15} {:>15}", "Information per word",
        format!("{:.1} bits", 27.0 * 1.585), // log2(3) = 1.585
        "64.0 bits");
    println!("  {:30} {:>15} {:>15}", "Info density (bits/digit)",
        "1.585", "1.000");
    println!();

    println!("  EXECUTION (avg of {} run{})", iterations, if iterations > 1 { "s" } else { "" });
    println!("  ───────────────────────────────────────────────────────────────────");
    println!("  {:30} {:>15} {:>15}", "Metric", "T3ISA (ternary)", "LLVM (binary)");
    println!("  {:30} {:>15} {:>15}", "──────", "───────────────", "─────────────");
    println!("  {:30} {:>15} {:>15}", "Instructions executed",
        t3_prof.total_instructions,
        if llvm_compiled { format!("~{} (IR)", llvm_ir_instrs) } else { "N/A".into() });
    if x86_instr_count > 0 {
        println!("  {:30} {:>15} {:>15}", "Native instructions (static)",
            format!("{} (T3)", t3_program_words),
            format!("{} (x86)", x86_instr_count));
    }
    println!("  {:30} {:>15} {:>15}", "Wall time",
        format!("{:.3} ms", t3_avg_time.as_secs_f64() * 1000.0),
        if llvm_compiled { format!("{:.3} ms", llvm_avg_time.as_secs_f64() * 1000.0) } else { "N/A".into() });
    println!();

    println!("  T3ISA EXECUTION PROFILE");
    println!("  ───────────────────────────────────────────────────────────────────");
    print!("{}", t3_prof.summary());
    println!();

    // ---- Ternary advantage analysis ----
    println!("  TERNARY vs BINARY ANALYSIS");
    println!("  ───────────────────────────────────────────────────────────────────");
    // Information efficiency
    let t3_info_bits = t3_program_words as f64 * 27.0 * 1.585;
    let binary_bits = llvm_binary_size as f64 * 8.0;
    if llvm_compiled && llvm_binary_size > 0 {
        println!("  T3ISA encodes {:.1} bits of information in {} words", t3_info_bits, t3_program_words);
        println!("  LLVM binary: {} bytes = {:.0} bits", llvm_binary_size, binary_bits);
        println!("  Ternary code density: {:.1}x more information per digit",
            1.585f64);
    }

    // Ternary-native operations
    let ternary_pct = if t3_prof.total_instructions > 0 {
        t3_prof.ternary_native_ops as f64 / t3_prof.total_instructions as f64 * 100.0
    } else { 0.0 };
    println!("  Ternary-native ops: {} ({:.1}% of all ops)", t3_prof.ternary_native_ops, ternary_pct);
    println!("    These ops (TAND/TOR/TNOT/TBRANCH/TMIN/TMAX/TSHI/TSHR) have no");
    println!("    SINGLE-INSTRUCTION binary equivalent — a binary machine emulates");
    println!("    each with a compare-and-select sequence.");

    // Conditional branches executed. NOT a count of three-way branches: TBRANCH
    // is a pseudo-instruction the assembler expands into TBR_POS + TBR_ZERO +
    // JUMP, so a single three-way dispatch contributes two to this total. The
    // previous version of this report called it "three-way branches" and then
    // multiplied by four to estimate a binary equivalent, double-counting twice
    // over. No binary-equivalent estimate is printed now; there is no honest way
    // to derive one from a profile of the ternary side alone.
    let cond_branch_count = t3_prof.opcode_counts[19]  // Tbranch (legacy packed form)
        + t3_prof.opcode_counts[25]  // TbrPos
        + t3_prof.opcode_counts[26]  // TbrZero
        + t3_prof.opcode_counts[27]; // TbrNeg
    if cond_branch_count > 0 {
        println!("  Conditional branch instructions: {}", cond_branch_count);
        println!("    TBRANCH expands to TBR_POS + TBR_ZERO + JUMP, so the number of");
        println!("    three-way dispatches is roughly half this figure.");
    }

    println!();
    println!("===============================================================================");

    // Clean up
    let _ = std::fs::remove_file(&ll_path);
    let _ = std::fs::remove_file(&bin_path);
    let _ = std::fs::remove_file(&runtime_obj);

    Ok(())
}
