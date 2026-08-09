mod bench;
mod runtime_link;
use bench::{read_source, run_bench};

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use manitc::error::{self, CompileError, CompileResult, Diagnostic};
use manitc::lexer::Lexer;
use manitc::parser::Parser as ManiParser;
use manitc::semantic::SemanticAnalyzer;
use manitc::ir::{self, IRLowerer};
use manitc::codegen_llvm;
use manitc::codegen_t3;
use manitc::lsp;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "manitc",
    about = "The maniT balanced ternary language compiler",
    version = "0.1.0"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a maniT source file
    Compile {
        /// Source file to compile
        file: PathBuf,

        /// Compilation target: llvm or t3
        #[arg(long, default_value = "llvm")]
        target: String,

        /// Output file path
        #[arg(short, long, default_value = "a.out")]
        output: PathBuf,

        /// Print the IR to stdout
        #[arg(long)]
        emit_ir: bool,

        /// Treat all warnings as errors
        #[arg(long)]
        warn_as_error: bool,

        /// LLVM target triple (e.g. aarch64-unknown-linux-gnu). Defaults to x86_64-pc-linux-gnu.
        #[arg(long)]
        target_triple: Option<String>,
    },

    /// Type-check a maniT source file without generating code
    Check {
        /// Source file to check
        file: PathBuf,

        /// Treat all warnings as errors
        #[arg(long)]
        warn_as_error: bool,
    },

    /// Lex a source file and print the token stream (debug)
    Lex {
        /// Source file to lex
        file: PathBuf,
    },

    /// Parse a source file and print the AST (debug)
    Parse {
        /// Source file to parse
        file: PathBuf,
    },

    /// Run a compiled T3ISA binary in the emulator
    RunT3 {
        /// T3ISA binary (.t3b) to run
        file: PathBuf,

        /// Run in interactive debug mode (step, breakpoints, register inspection)
        #[arg(long)]
        debug: bool,
    },

    /// Benchmark: compile to both LLVM and T3ISA, run both, compare metrics
    Bench {
        /// Source file to benchmark
        file: PathBuf,

        /// Number of iterations for timing (default: 1)
        #[arg(long, default_value = "1")]
        iterations: usize,
    },

    /// Start the ManiT Language Server (LSP) over stdio
    Lsp,
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------


fn run_compile(file: &PathBuf, target: &str, output: &PathBuf, emit_ir: bool, warn_as_error: bool, target_triple: Option<&str>) -> CompileResult<()> {
    let source = read_source(file).map_err(|e| CompileError::Lex(
        Diagnostic::unknown(e),
    ))?;
    let file_str = file.to_string_lossy().to_string();

    // Lex
    let mut lexer = Lexer::with_file(&source, &file_str);
    let tokens = lexer.tokenize()?;

    // Parse
    let mut parser = ManiParser::with_file(tokens, &file_str);
    let program = parser.parse()?;

    // Type-check / semantic analysis
    let mut analyzer = SemanticAnalyzer::with_file(&file_str);
    analyzer.warnings.warn_as_error = warn_as_error;
    let typed_program = analyzer.analyze(&program)?;

    // Emit warnings with source context
    analyzer.warnings.emit_all_rich(&source);
    analyzer.warnings.check_error()?;

    // Borrow / move checking
    manitc::borrow::check_borrows(&typed_program)?;

    // IR lowering + optimization
    let mut ir_module = IRLowerer::lower(&typed_program);
    ir::optimize::run_passes(&mut ir_module);

    if emit_ir {
        println!("; maniT IR for '{}'", file.display());
        println!("; {} functions, {} globals, {} string literals",
            ir_module.functions.len(),
            ir_module.globals.len(),
            ir_module.string_literals.len());
        for func in &ir_module.functions {
            println!("fn {} ({} params, {} blocks)",
                func.name, func.params.len(), func.blocks.len());
            for block in &func.blocks {
                println!("  {}:", block.label);
                for instr in &block.instrs {
                    println!("    {:?}", instr);
                }
                println!("    -> {:?}", block.term);
            }
        }
    }

    // Codegen
    match target {
        "llvm" => {
            let ll_text = codegen_llvm::emit_llvm_ir(&ir_module, target_triple);
            let ll_path = output.with_extension("ll");
            std::fs::write(&ll_path, &ll_text).map_err(|e| {
                CompileError::Codegen(
                    Diagnostic::unknown(e.to_string()))
            })?;
            println!("[LLVM] wrote {}", ll_path.display());

            // Resolve the runtime C source and decide how to build it — full
            // (SDL2 + libcurl) or minimal. See runtime_link.
            let runtime_c_path = runtime_link::resolve_source(Some(&file));
            let link = runtime_link::flags();

            // Compile runtime to object file
            let runtime_obj = runtime_link::object_path("compile");
            let runtime_compiled = std::process::Command::new("clang")
                .args([
                    runtime_c_path.to_str().unwrap(),
                    "-c",
                    "-o", runtime_obj.to_str().unwrap(),
                ])
                .args(&link.cflags)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            // Link .ll + runtime object into final binary
            if runtime_compiled {
                if let Ok(status) = std::process::Command::new("clang")
                    .args([
                        ll_path.to_str().unwrap(),
                        runtime_obj.to_str().unwrap(),
                        "-o", output.to_str().unwrap(),
                        "-lm", "-lpthread",
                    ])
                    .args(&link.libs)
                    .status()
                {
                    if status.success() {
                        println!("[LLVM] binary: {}", output.display());
                    } else {
                        println!("[LLVM] clang link failed (see .ll file)");
                    }
                } else {
                    println!("[LLVM] clang not found — LLVM IR written to {}", ll_path.display());
                }
            } else {
                // No runtime or clang unavailable — try simple link without runtime
                if let Ok(status) = std::process::Command::new("clang")
                    .args([ll_path.to_str().unwrap(), "-o", output.to_str().unwrap(), "-lm", "-lpthread"])
                    .status()
                {
                    if status.success() {
                        println!("[LLVM] binary (no runtime): {}", output.display());
                    } else {
                        println!("[LLVM] clang compilation failed (see .ll file)");
                    }
                } else {
                    println!("[LLVM] clang not found — LLVM IR written to {}", ll_path.display());
                    println!("[LLVM] to compile: clang {} {} -o {} -lm -lpthread",
                        ll_path.display(), runtime_obj.display(), output.display());
                }
            }
        }
        "t3" => {
            let asm_text = codegen_t3::emit_t3_asm(&ir_module);
            let asm_path = output.with_extension("t3s");
            std::fs::write(&asm_path, &asm_text).map_err(|e| {
                CompileError::Codegen(
                    Diagnostic::unknown(e.to_string()))
            })?;
            println!("[T3ISA] wrote assembly: {}", asm_path.display());
            // Assemble to binary
            match codegen_t3::assemble(&asm_text) {
                Ok((words, str_data, float_data)) => {
                    let bin_path = output.with_extension("t3b");
                    codegen_t3::write_t3_binary(&words, bin_path.to_str().unwrap())
                        .map_err(|e| CompileError::Codegen(
                            Diagnostic::unknown(e.to_string())))?;
                    // Write string table alongside binary (.t3s already written above)
                    let str_path = output.with_extension("t3d");
                    let str_json = str_data.iter()
                        .map(|(k, v)| format!("{}:{}", k, v.replace('\n', "\\n")))
                        .collect::<Vec<_>>()
                        .join("\n");
                    let _ = std::fs::write(&str_path, str_json);
                    // Write float sidecar (.t3f)
                    let float_path = output.with_extension("t3f");
                    let float_json = float_data.iter()
                        .map(|(k, v)| format!("{}:{}", k, v))
                        .collect::<Vec<_>>()
                        .join("\n");
                    let _ = std::fs::write(&float_path, float_json);
                    println!("[T3ISA] wrote binary:   {}", bin_path.display());
                    println!("[T3ISA] to run:  manitc run-t3 {}", bin_path.display());
                }
                Err(e) => {
                    eprintln!("[T3ISA] assembler error: {}", e);
                }
            }
        }
        other => {
            eprintln!("unknown target '{}', supported: llvm, t3", other);
            std::process::exit(1);
        }
    }

    Ok(())
}

fn run_check(file: &PathBuf, warn_as_error: bool) -> CompileResult<()> {
    let source = read_source(file).map_err(|e| CompileError::Lex(
        Diagnostic::unknown(e),
    ))?;
    let file_str = file.to_string_lossy().to_string();

    let mut lexer = Lexer::with_file(&source, &file_str);
    let tokens = lexer.tokenize()?;

    let mut parser = ManiParser::with_file(tokens, &file_str);
    let program = parser.parse()?;

    let mut analyzer = SemanticAnalyzer::with_file(&file_str);
    analyzer.warnings.warn_as_error = warn_as_error;
    let typed = analyzer.analyze(&program)?;

    // Emit warnings with source context
    analyzer.warnings.emit_all_rich(&source);
    analyzer.warnings.check_error()?;

    // Borrow / move checking
    manitc::borrow::check_borrows(&typed)?;

    let warning_count = analyzer.warnings.count();
    println!(
        "OK: {} — {} functions, {} structs, {} enums{}",
        file.display(),
        typed.functions.len(),
        typed.structs.len(),
        typed.enums.len(),
        if warning_count > 0 { format!(", {} warning(s)", warning_count) } else { String::new() },
    );
    Ok(())
}

fn run_lex(file: &PathBuf) -> CompileResult<()> {
    let source = read_source(file).map_err(|e| CompileError::Lex(
        Diagnostic::unknown(e),
    ))?;
    let file_str = file.to_string_lossy().to_string();

    let mut lexer = Lexer::with_file(&source, &file_str);
    let tokens = lexer.tokenize()?;

    for tok in &tokens {
        println!("{:?}  ({}:{})", tok.kind, tok.span.line, tok.span.col);
    }
    Ok(())
}

fn run_parse(file: &PathBuf) -> CompileResult<()> {
    let source = read_source(file).map_err(|e| CompileError::Lex(
        Diagnostic::unknown(e),
    ))?;
    let file_str = file.to_string_lossy().to_string();

    let mut lexer = Lexer::with_file(&source, &file_str);
    let tokens = lexer.tokenize()?;

    let mut parser = ManiParser::with_file(tokens, &file_str);
    let program = parser.parse()?;

    println!("Program with {} top-level items:", program.items.len());
    for item in &program.items {
        println!("  {:?}", item);
    }
    Ok(())
}

fn run_t3(file: &PathBuf, debug: bool) -> CompileResult<()> {
    let words = codegen_t3::read_t3_binary(file.to_str().unwrap())
        .map_err(|e| crate::error::CompileError::Codegen(
            crate::error::Diagnostic::unknown(e.to_string())))?;
    // Load string sidecar (.t3d) if present
    let str_data: std::collections::HashMap<usize, String> = {
        let d_path = file.with_extension("t3d");
        if let Ok(text) = std::fs::read_to_string(&d_path) {
            text.lines()
                .filter_map(|line| {
                    let mut parts = line.splitn(2, ':');
                    let addr: usize = parts.next()?.parse().ok()?;
                    let s = parts.next()?.replace("\\n", "\n");
                    Some((addr, s))
                })
                .collect()
        } else {
            std::collections::HashMap::new()
        }
    };
    let float_data: std::collections::HashMap<usize, i64> = {
        let f_path = file.with_extension("t3f");
        if let Ok(text) = std::fs::read_to_string(&f_path) {
            text.lines()
                .filter_map(|line| {
                    let mut parts = line.splitn(2, ':');
                    let addr: usize = parts.next()?.parse().ok()?;
                    let v: i64 = parts.next()?.parse().ok()?;
                    Some((addr, v))
                })
                .collect()
        } else {
            std::collections::HashMap::new()
        }
    };
    println!("[T3ISA] running {} ({} words)", file.display(), words.len());
    let output_lines = if debug {
        codegen_t3::run_emulator_debug(words, str_data, float_data)
    } else {
        codegen_t3::run_emulator(words, str_data, float_data)
    };
    for piece in &output_lines {
        print!("{}", piece);
    }
    println!();
    Ok(())
}

// ---------------------------------------------------------------------------
// Benchmark: compile to both targets, run, compare
// ---------------------------------------------------------------------------


// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let cli = Cli::parse();

    let result: CompileResult<()> = match &cli.command {
        Commands::Compile { file, target, output, emit_ir, warn_as_error, target_triple } => {
            run_compile(file, target, output, *emit_ir, *warn_as_error, target_triple.as_deref())
        }
        Commands::Check { file, warn_as_error } => run_check(file, *warn_as_error),
        Commands::Lex { file } => run_lex(file),
        Commands::Parse { file } => run_parse(file),
        Commands::RunT3 { file, debug } => run_t3(file, *debug),
        Commands::Bench { file, iterations } => run_bench(file, *iterations),
        Commands::Lsp => {
            tokio::runtime::Runtime::new()
                .expect("failed to create tokio runtime")
                .block_on(lsp::run_lsp());
            return;
        }
    };

    if let Err(e) = result {
        // Try to load source for rich error display
        let source_file = match &cli.command {
            Commands::Compile { file, .. } | Commands::Check { file, .. }
            | Commands::Lex { file } | Commands::Parse { file }
            | Commands::Bench { file, .. } => Some(file.clone()),
            _ => None,
        };
        let source = source_file.and_then(|f| std::fs::read_to_string(f).ok());
        eprint!("{}", manitc::error::render_error(&e, source.as_deref()));
        std::process::exit(1);
    }
}
