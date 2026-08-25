mod bench;
use bench::{read_source, run_bench};
use manitc::runtime_link;

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

        /// Set a lint to allow (A5). Repeatable: --allow shadowing --allow unused-variable
        #[arg(short = 'A', long = "allow", value_name = "LINT")]
        allow: Vec<String>,

        /// Set a lint to warn (A5). Repeatable.
        #[arg(short = 'W', long = "warn", value_name = "LINT")]
        warn: Vec<String>,

        /// Set a lint to deny — reported and fails the build (A5). Repeatable.
        #[arg(short = 'D', long = "deny", value_name = "LINT")]
        deny: Vec<String>,

        /// Set a lint to forbid — deny that a module cannot lower (A5). Repeatable.
        #[arg(short = 'F', long = "forbid", value_name = "LINT")]
        forbid: Vec<String>,

        /// Print the effective lint levels before compiling (A5).
        #[arg(long)]
        print_lints: bool,

        /// LLVM target triple (e.g. aarch64-unknown-linux-gnu). Defaults to x86_64-pc-linux-gnu.
        #[arg(long)]
        target_triple: Option<String>,

        /// F-1: lift local variables out of memory into SSA values. ON BY
        /// DEFAULT — this flag is now a no-op, kept so existing scripts and
        /// tests that pass it keep working.
        ///
        /// It removes about half of the IR: 42,008 of 79,953 instructions
        /// across the shipped examples are loads and stores of locals, and
        /// 91.5 % of allocas are promotable.
        #[arg(long)]
        mem2reg: bool,

        /// Report how many instructions each optimiser pass removes (F-2).
        ///
        /// Printed to stderr. Every pass reasons about temps, so before
        /// promotion was the default a local variable was invisible to all of
        /// them; this is what says what they are worth now and where the
        /// headroom is.
        #[arg(long)]
        pass_stats: bool,

        /// Run the per-function optimiser passes this many times (F-2).
        ///
        /// The default of 1 is the historical pipeline. Above 1 the passes
        /// repeat until a round changes nothing, so each one sees what its
        /// neighbours produced. Bounded rather than a true fixpoint: a pair
        /// that undid each other would otherwise spin.
        #[arg(long, value_name = "N", default_value_t = 1)]
        rounds: usize,

        /// F-2: the largest single-block callee, in IR instructions, to splice
        /// into its callers.
        ///
        /// 482 of the 1,101 small non-recursive call sites in the shipped
        /// examples have a callee that is ONE block ending in a return, and
        /// those need no control-flow surgery at all — the body is spliced
        /// where the call stood. The most-called of them are forwarding
        /// wrappers with a ONE-instruction body, where inlining removes a call
        /// frame and adds nothing.
        #[arg(long, value_name = "N", default_value_t = manitc::ir::inline::SIZE_LIMIT)]
        inline_limit: usize,

        /// Turn F-2 inlining OFF. Equivalent to `--inline-limit 0`, and the
        /// switch to reach for when dating a defect as pre-existing or when
        /// bisecting a codegen change.
        #[arg(long = "no-inline")]
        no_inline: bool,

        /// Turn P26 block merging OFF, leaving every empty block and every
        /// jump-to-a-single-successor where the lowerer put it.
        ///
        /// It exists so the pass can be measured against itself on one binary:
        /// merging removes no instructions of its own, so what it is worth
        /// shows up only in what the passes AFTER it can then see.
        #[arg(long = "no-merge-blocks")]
        no_merge_blocks: bool,

        /// Turn F-1 promotion OFF, compiling locals as memory the way the
        /// pre-F-1 compiler did.
        ///
        /// This is the switch that reproduces the reference compiler's output
        /// byte for byte, which is how a defect is dated as pre-existing rather
        /// than newly introduced. If both flags are given, this one wins.
        ///
        /// Promotion was off by default until the T3 register allocator was
        /// rewritten (F-3), because the old one did not survive the volume of
        /// phi nodes it produces. It now runs 17/17 examples with 17/17
        /// cross-backend agreement on both language versions.
        #[arg(long = "no-mem2reg")]
        no_mem2reg: bool,

        /// Check the IR against SSA form and report (F-1).
        ///
        /// Reports twice — after lowering and after the optimiser — because
        /// they are different questions: whether the lowerer PRODUCES SSA, and
        /// whether the passes PRESERVE it. Reporting only after the optimiser
        /// would let a pass that breaks SSA hide behind a lowerer that never
        /// established it.
        #[arg(long)]
        verify_ssa: bool,

        /// Language version: v1 (default) or v2 (R2).
        ///
        /// v2 turns on the C4 division semantics — `/` and `%` round to
        /// nearest, ties away from zero — and N5, under which `int` is a
        /// 27-trit word on every backend rather than the target's word.
        /// `--warn division-semantics` lists the sites v2 would change.
        #[arg(long, value_name = "VERSION", default_value = "v1")]
        lang: String,
    },

    /// Type-check a maniT source file without generating code
    Check {
        /// Source file to check
        file: PathBuf,

        /// Treat all warnings as errors
        #[arg(long)]
        warn_as_error: bool,

        /// Set a lint to allow (A5). Repeatable.
        #[arg(short = 'A', long = "allow", value_name = "LINT")]
        allow: Vec<String>,

        /// Set a lint to warn (A5). Repeatable. `--warn undeclared-native`
        /// prints the A1 migration backlog for this program.
        #[arg(short = 'W', long = "warn", value_name = "LINT")]
        warn: Vec<String>,

        /// Set a lint to deny — reported and fails the check (A5). Repeatable.
        #[arg(short = 'D', long = "deny", value_name = "LINT")]
        deny: Vec<String>,

        /// Set a lint to forbid — deny that a module cannot lower (A5). Repeatable.
        #[arg(short = 'F', long = "forbid", value_name = "LINT")]
        forbid: Vec<String>,

        /// Print the effective lint levels (A5).
        #[arg(long)]
        print_lints: bool,

        /// Check availability against this backend (llvm / t3) rather than
        /// backend-agnostically. A1 step 3's input.
        #[arg(long, value_name = "BACKEND")]
        backend: Option<String>,

        /// Language version: v1 (default) or v2 (R2).
        ///
        /// v2 turns on the C4 division semantics — `/` and `%` round to
        /// nearest, ties away from zero — and N5, under which `int` is a
        /// 27-trit word on every backend rather than the target's word.
        /// `--warn division-semantics` lists the sites v2 would change.
        #[arg(long, value_name = "VERSION", default_value = "v1")]
        lang: String,
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

        /// Instruction budget before the run is cut off (exit 71).
        ///
        /// A runaway guard, not a correctness limit. The default is measured:
        /// see DEFAULT_MAX_STEPS. Lower it to bound a suspected infinite loop,
        /// raise it for a benchmark that legitimately runs long.
        #[arg(long, value_name = "N", default_value_t = codegen_t3::DEFAULT_MAX_STEPS)]
        max_steps: usize,

        /// Print the execution profile to stderr when the program stops.
        ///
        /// The emulator has always counted every instruction it executed and
        /// every opcode separately; this is what gets it out (report.txt P31).
        /// It is the right way to measure an optimiser pass: compile the same
        /// program twice and diff the two profiles. Before this existed the
        /// count had to be recovered by bisecting `--max-steps` for the
        /// smallest budget the program completes under — forty runs to read
        /// one number, and no histogram at the end of it.
        ///
        /// Every line is prefixed `[T3ISA]`, which the corpus sweep scripts
        /// already filter, and goes to stderr, so profiling a run cannot
        /// disturb what the program printed.
        #[arg(long)]
        profile: bool,

        /// Language version: v1 (default) or v2 (R2).
        ///
        /// v2 turns on the C4 division semantics — `/` and `%` round to
        /// nearest, ties away from zero — and N5, under which `int` is a
        /// 27-trit word on every backend rather than the target's word.
        /// `--warn division-semantics` lists the sites v2 would change.
        #[arg(long, value_name = "VERSION", default_value = "v1")]
        lang: String,

        /// Arguments passed on to the program, readable with `env::arg(i)`.
        ///
        /// Everything after the binary is the program's, not ours: put
        /// `--debug` or `--max-steps` BEFORE the file, or `--` in front of an
        /// argument that would otherwise look like a flag of ours.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, value_name = "ARGS")]
        args: Vec<String>,
    },

    /// Benchmark: compile to both LLVM and T3ISA, run both, compare metrics
    Bench {
        /// Source file to benchmark
        file: PathBuf,

        /// Number of iterations for timing (default: 1)
        #[arg(long, default_value = "1")]
        iterations: usize,

        /// Language version: v1 (default) or v2 (R2).
        ///
        /// v2 turns on the C4 division semantics — `/` and `%` round to
        /// nearest, ties away from zero — and N5, under which `int` is a
        /// 27-trit word on every backend rather than the target's word.
        /// `--warn division-semantics` lists the sites v2 would change.
        #[arg(long, value_name = "VERSION", default_value = "v1")]
        lang: String,
    },

    /// Start the ManiT Language Server (LSP) over stdio
    Lsp,
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------


/// The lint levels requested on the command line (A5).
///
/// Grouped in one struct so `compile` and `check` cannot drift apart on which
/// flags they honour — a `--deny` that worked for one and was ignored by the
/// other would make a check and a build disagree about what passes, which is
/// the failure A5 exists to make impossible.
#[derive(Default, Clone)]
struct LintFlags {
    allow: Vec<String>,
    warn: Vec<String>,
    deny: Vec<String>,
    forbid: Vec<String>,
}

impl LintFlags {
    /// Build the table. Applied in increasing severity so that when the same
    /// lint is named twice, the stricter setting is the one that survives.
    fn table(&self) -> CompileResult<manitc::lint::LintTable> {
        use manitc::lint::{LintLevel, LintTable};
        let mut t = LintTable::new();
        let groups = [
            (&self.allow, LintLevel::Allow),
            (&self.warn, LintLevel::Warn),
            (&self.deny, LintLevel::Deny),
            (&self.forbid, LintLevel::Forbid),
        ];
        for (names, level) in groups {
            for n in names {
                t.set(n, level).map_err(|e| {
                    CompileError::Type(Diagnostic::unknown(e))
                })?;
            }
        }
        Ok(t)
    }
}

/// Resolve a `--lang` argument, reporting an unrecognised one rather than
/// falling back to the default (R2).
///
/// A typo that quietly selected v1 would compile the program under arithmetic
/// its author did not ask for and nothing downstream would say so — the same
/// class of silent no-op that `LintTable::set` refuses for `--deny`.
fn parse_lang(s: &str) -> CompileResult<manitc::lang::LangVersion> {
    manitc::lang::LangVersion::from_name(s).ok_or_else(|| {
        let names: Vec<&str> = manitc::lang::LangVersion::all()
            .iter()
            .map(|v| v.as_str())
            .collect();
        CompileError::Type(Diagnostic::unknown(format!(
            "unknown language version '{}'; known versions: {}",
            s,
            names.join(", ")
        )))
    })
}

/// F-1: print the SSA verifier's findings for one stage of the pipeline.
///
/// Grouped by kind and counted, with a few examples: the interesting number is
/// how many functions fail and in what WAY, not a wall of one line per temp.
/// The counts are on stdout so they can be collected across a corpus; nothing
/// here fails the compilation, because the IR is not SSA yet and making it an
/// error before `mem2reg` lands would mean nothing could be compiled at all.
fn report_ssa(module: &ir::IRModule, stage: &str) {
    use manitc::ir::ssa::Violation;
    let found = manitc::ir::ssa::verify_module(module);
    let total_fns = module.functions.iter().filter(|f| !f.is_extern).count();
    let mut failing: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let (mut multi, mut dom, mut undef, mut phi, mut dup, mut dangle, mut void) =
        (0, 0, 0, 0, 0, 0, 0);
    for (fname, v) in &found {
        failing.insert(fname.as_str());
        match v {
            Violation::MultiplyDefined { .. } => multi += 1,
            Violation::NotDominated { .. } => dom += 1,
            Violation::Undefined { .. } => undef += 1,
            Violation::PhiEdges { .. } => phi += 1,
            Violation::DuplicateLabel { .. } => dup += 1,
            Violation::DanglingTarget { .. } => dangle += 1,
            Violation::VoidPhiArm { .. } => void += 1,
        }
    }
    let st = manitc::ir::ssa::Stats::of_module(module);
    println!(
        "ssa {} — {} of {} functions not in SSA form, {} violations",
        stage,
        failing.len(),
        total_fns,
        found.len()
    );
    println!(
        "ssa {}   blocks={} instrs={} defs={} allocas={} promotable={} \
loads={} stores={} phis={} phi-edges-from-branch={}",
        stage, st.blocks, st.instrs, st.temps, st.allocas, st.promotable,
        st.loads, st.stores, st.phis, st.phi_edges_from_branch
    );
    // **The per-class counts print even when they are all zero**, and that is
    // deliberate. They used to be behind an early return on `found.is_empty()`,
    // so a clean compile showed no counter line at all — and a sweep grepping
    // for one then reported `0 files, 0 violations` and looked like a pass when
    // it had measured nothing. A denominator that disappears when the numerator
    // is zero is not an instrument. It also makes the line the record of WHICH
    // classes were checked, which is what says a new one is live.
    println!(
        "ssa {}   multiply-defined={} not-dominated={} undefined={} phi-edges={}          duplicate-label={} dangling-target={} void-phi-arm={}",
        stage, multi, dom, undef, phi, dup, dangle, void
    );
    for (fname, v) in found.iter().take(5) {
        println!("ssa {}   e.g. {}: {}", stage, fname, v);
    }
}

/// A5: print the effective lint levels.
fn print_lint_manifest(t: &manitc::lint::LintTable) {
    for line in t.manifest_lines() {
        println!("{}", line);
    }
}

fn run_compile(
    file: &PathBuf,
    target: &str,
    output: &PathBuf,
    emit_ir: bool,
    warn_as_error: bool,
    lints: &LintFlags,
    print_lints: bool,
    target_triple: Option<&str>,
    lang: manitc::lang::LangVersion,
    verify_ssa: bool,
    mem2reg: bool,
    pass_stats: bool,
    rounds: usize,
    inline_limit: usize,
    merge_blocks: bool,
) -> CompileResult<()> {
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
    analyzer.warnings.lints = lints.table()?;
    // A1 step 3's input: an `available(...)` clause is checked against the
    // backend actually selected, so the same source can be legal for one
    // target and reported for the other.
    analyzer.backend = Some(target.to_string());
    analyzer.lang = lang;
    if print_lints {
        print_lint_manifest(&analyzer.warnings.lints);
    }
    let typed_program = analyzer.analyze(&program)?;
    // The table the SOURCE ended up with: a `lint` item in the file may have
    // raised or lowered what the command line asked for, and it is the
    // effective set — not the requested one — that belongs in the artifact.
    let lint_manifest = analyzer.warnings.lints.manifest();

    // Emit warnings with source context
    analyzer.warnings.emit_all_rich(&source);
    analyzer.warnings.check_error()?;

    // A8: `compile` produces an executable, so it needs an entry point.
    // Without this the failure surfaced late and differently per backend: the
    // T3 path wrote a .t3b that ran to a silent exit 1, and the LLVM path
    // leaked the raw toolchain error ("undefined reference to `main'").
    if !typed_program.functions.iter().any(|f| f.name == "main") {
        return Err(CompileError::Codegen(Diagnostic::unknown(format!(
            "{}: no `main` function found — a compiled program needs \
             `fn main() {{ … }}` as its entry point",
            file.display(),
        ))));
    }

    // Borrow / move checking
    manitc::borrow::check_borrows(&typed_program)?;

    // IR lowering + optimization
    let mut ir_module = IRLowerer::lower_with(&typed_program, lang);
    if verify_ssa {
        report_ssa(&ir_module, "after lowering");
    }
    ir::optimize::run_passes_with(
        &mut ir_module,
        ir::optimize::PassOptions { mem2reg, pass_stats, rounds, inline_limit, merge_blocks },
    );
    if verify_ssa {
        report_ssa(&ir_module, "after optimisation");
    }

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
            let mut ll_text = codegen_llvm::emit_llvm_ir(&ir_module, target_triple);
            // A5: record the effective lint levels IN the artifact.
            //
            // A comment alone would be lost at link time, so the manifest is
            // also a global constant: it survives into the linked executable
            // and `strings a.out | grep manitc-lints` answers "what was this
            // checked for?" without the compiler, the build log, or a
            // side-channel record of which binary scored the run. That
            // side-channel is precisely what section 54 forced on the
            // model-training campaign, and it is what this removes.
            ll_text = record_lint_manifest_llvm(ll_text, &lint_manifest);
            let ll_path = output.with_extension("ll");
            std::fs::write(&ll_path, &ll_text).map_err(|e| {
                CompileError::Codegen(
                    Diagnostic::unknown(e.to_string()))
            })?;
            println!("[LLVM] wrote {}", ll_path.display());

            // A18: `output.with_extension("ll")` is a no-op when -o already
            // names a .ll file, so ll_path and output are the same path and
            // linking would overwrite the IR we just wrote with the ELF
            // binary. Asking for a .ll output means "emit LLVM IR", so stop
            // here instead: the caller (e.g. thatteos/build.sh) links itself.
            if ll_path == *output {
                println!("[LLVM] IR-only output (-o names a .ll file) — not linking");
                return Ok(());
            }

            // Resolve the runtime C source and decide how to build it — full
            // (SDL2 + libcurl) or minimal. See runtime_link.
            // Removes the runtime object, and the extracted sources if it
            // extracted any, when this scope ends — including on the early
            // returns below.
            let mut scratch = runtime_link::Scratch::new();
            let runtime_c_path = runtime_link::resolve_source(Some(&file), &mut scratch).map_err(|e| {
                CompileError::Codegen(Diagnostic::unknown(format!(
                    "failed to write the embedded C runtime: {}", e)))
            })?;
            let link = runtime_link::flags();

            // K8: a minimal runtime (-DMANIT_NO_GUI) has no gui_*/net_*
            // symbols. If the program actually calls them, fail now with a
            // diagnostic naming the missing packages instead of letting the
            // link die with raw undefined-symbol errors.
            let minimal_runtime = link.cflags.iter().any(|f| f == "-DMANIT_NO_GUI");
            if minimal_runtime {
                let uses = |prefix: &str| {
                    ll_text.lines().any(|l| {
                        let t = l.trim_start();
                        !t.starts_with("declare") && !t.starts_with(';') && t.contains(prefix)
                    })
                };
                let gui_used = uses("@gui_");
                let net_used = uses("@net_");
                if gui_used || net_used {
                    let forced = std::env::var("MANIT_NO_GUI").is_ok();
                    let mut msg = String::new();
                    if gui_used {
                        msg.push_str(
                            "this program uses the `gui` module, but the runtime was built \
                             without GUI support (-DMANIT_NO_GUI).\n  Missing packages: SDL2 and \
                             SDL2_ttf development headers (pkg-config names: sdl2, SDL2_ttf).\n  \
                             On Debian/Ubuntu: apt install libsdl2-dev libsdl2-ttf-dev\n");
                    }
                    if net_used {
                        msg.push_str(
                            "this program uses the `net` module, but the runtime was built \
                             without network support (-DMANIT_NO_GUI).\n  Missing package: \
                             libcurl development headers (pkg-config name: libcurl).\n  \
                             On Debian/Ubuntu: apt install libcurl4-openssl-dev\n");
                    }
                    if forced {
                        msg.push_str(
                            "  (the minimal runtime was forced by MANIT_NO_GUI=1 in the \
                             environment — unset it once the packages are installed)\n");
                    } else {
                        msg.push_str(
                            "  (install the packages above so pkg-config finds them, then \
                             recompile)\n");
                    }
                    return Err(CompileError::Codegen(Diagnostic::unknown(msg)));
                }
            }

            // `clang` may only be installed under a versioned name (clang-19).
            let clang = runtime_link::find_clang();

            // Compile runtime to object file
            let runtime_obj = scratch.add(runtime_link::object_path("compile"));
            let runtime_compiled = clang.as_deref().map(|clang| {
                std::process::Command::new(clang)
                    .args([
                        runtime_c_path.to_str().unwrap(),
                        "-c",
                        "-o", runtime_obj.to_str().unwrap(),
                    ])
                    .args(&link.cflags)
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            }).unwrap_or(false);

            // Link .ll + runtime object into final binary
            if runtime_compiled {
                let clang = clang.as_deref().unwrap();
                if let Ok(out) = std::process::Command::new(clang)
                    .args([
                        ll_path.to_str().unwrap(),
                        runtime_obj.to_str().unwrap(),
                        "-o", output.to_str().unwrap(),
                        "-lm", "-lpthread",
                    ])
                    .args(&link.libs)
                    .output()
                {
                    if out.status.success() {
                        println!("[LLVM] binary: {}", output.display());
                    } else {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        eprint!("{}", stderr);
                        // K8 backstop: explain undefined gui_/net_ symbols.
                        if stderr.contains("gui_") || stderr.contains("net_") {
                            eprintln!(
                                "[LLVM] hint: undefined gui_/net_ symbols mean the runtime was \
                                 built without SDL2/libcurl support; install libsdl2-dev, \
                                 libsdl2-ttf-dev and libcurl4-openssl-dev and recompile");
                        }
                        println!("[LLVM] clang link failed (see .ll file)");
                        return Err(CompileError::Codegen(Diagnostic::unknown(
                            format!("clang failed to link {}", ll_path.display()))));
                    }
                } else {
                    println!("[LLVM] clang not found — LLVM IR written to {}", ll_path.display());
                }
            } else if let Some(clang) = clang.as_deref() {
                // Runtime failed to compile — try simple link without runtime
                if let Ok(status) = std::process::Command::new(clang)
                    .args([ll_path.to_str().unwrap(), "-o", output.to_str().unwrap(), "-lm", "-lpthread"])
                    .status()
                {
                    if status.success() {
                        println!("[LLVM] binary (no runtime): {}", output.display());
                    } else {
                        println!("[LLVM] clang compilation failed (see .ll file)");
                    }
                }
            } else {
                println!("[LLVM] clang not found — LLVM IR written to {}", ll_path.display());
                println!("[LLVM] to compile: clang {} {} -o {} -lm -lpthread",
                    ll_path.display(), runtime_obj.display(), output.display());
            }
        }
        "t3" => {
            let asm_text = codegen_t3::emit_t3_asm(&ir_module)?;
            // A5. The .t3b is a magic word followed by instruction words with
            // no room for metadata, and widening it would break every existing
            // reader — so the manifest goes in the assembly header and in a
            // `.t3l` sidecar, next to the `.t3d` and `.t3f` sidecars the
            // format already uses for exactly this reason.
            let asm_text = format!("; {}\n{}", lint_manifest, asm_text);
            let lint_path = output.with_extension("t3l");
            let _ = std::fs::write(&lint_path, format!("{}\n", lint_manifest));
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
                    // No .t3b was written, so this must not report success:
                    // returning the error renders it and exits non-zero.
                    return Err(CompileError::Codegen(Diagnostic::unknown(
                        format!("[T3ISA] assembler error: {}", e))));
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

/// A5: put the lint manifest into the emitted LLVM IR.
///
/// Both as a leading comment (readable in the .ll) and as a module-level
/// constant (survives into the linked binary). The constant is `@manitc.lints`
/// with external linkage so nothing strips it as dead: a manifest that the
/// optimiser can delete is a manifest you cannot rely on finding.
fn record_lint_manifest_llvm(ll_text: String, manifest: &str) -> String {
    // NUL-terminated, and every byte escaped the way LLVM wants it. The
    // manifest is compiler-generated ASCII, but escaping rather than trusting
    // that keeps a future lint name with a quote in it from emitting broken IR.
    let mut escaped = String::new();
    for b in manifest.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b' ' | b'-' | b'_' | b'.' | b'=' | b',') {
            escaped.push(b as char);
        } else {
            escaped.push_str(&format!("\\{:02X}", b));
        }
    }
    escaped.push_str("\\00");
    let len = manifest.len() + 1;
    let global = format!("@manitc.lints = constant [{len} x i8] c\"{escaped}\"\n");

    // The module header is not optional furniture: LLVM requires `target
    // datalayout` and `target triple` to precede every other top-level entity,
    // so the global goes AFTER them. Putting it first parses as far as line 5
    // and then fails with "expected top-level entity" — which is a link error
    // that looks like a codegen bug, and is exactly the kind of late,
    // misattributed failure A1 is about elsewhere in this compiler.
    let mut out = format!("; {manifest}\n");
    let mut placed = false;
    for line in ll_text.lines() {
        out.push_str(line);
        out.push('\n');
        if !placed && line.starts_with("target triple") {
            out.push_str(&global);
            placed = true;
        }
    }
    if !placed {
        // No triple line to anchor to. End of module is always legal.
        out.push_str(&global);
    }
    out
}

fn run_check(
    file: &PathBuf,
    warn_as_error: bool,
    lints: &LintFlags,
    print_lints: bool,
    backend: Option<&str>,
    lang: manitc::lang::LangVersion,
) -> CompileResult<()> {
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
    analyzer.warnings.lints = lints.table()?;
    // `check` is backend-agnostic unless asked otherwise: reporting an
    // availability problem for a backend the invocation never named would be
    // an answer to a question nobody asked.
    analyzer.backend = backend.map(|b| b.to_string());
    analyzer.lang = lang;
    if print_lints {
        print_lint_manifest(&analyzer.warnings.lints);
    }
    let typed = analyzer.analyze(&program)?;

    // Emit warnings with source context
    analyzer.warnings.emit_all_rich(&source);

    // A1 step 1: the migration backlog, as a summary rather than a wall of
    // repeated call sites. Printed only when the lint is actually enabled,
    // which is why the default level is `allow` — turning it on is how you ask
    // for the backlog.
    let backlog_wanted = analyzer.warnings.effective_level(
        &manitc::error::WarningKind::UndeclaredNative,
    ) != manitc::lint::LintLevel::Allow;
    if backlog_wanted && !analyzer.undeclared_natives.is_empty() {
        eprintln!(
            "note: {} native(s) called with no `extern` declaration (A1 migration backlog):",
            analyzer.undeclared_natives.len()
        );
        for n in &analyzer.undeclared_natives {
            eprintln!("  {}", n);
        }
    }

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

/// Write to stdout, exiting quietly when the reader has gone away
/// (e.g. `manitc run-t3 prog.t3b | head`) instead of panicking on SIGPIPE.
fn pipe_safe_print(s: &str) {
    use std::io::Write;
    let mut out = std::io::stdout();
    if let Err(e) = out.write_all(s.as_bytes()).and_then(|_| out.flush()) {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
    }
}

/// Compile a .mt source file to T3ISA in memory (no output files), returning
/// (program words, string data, float data) ready for the emulator.
fn compile_t3_in_memory(file: &PathBuf, lang: manitc::lang::LangVersion) -> CompileResult<(
    Vec<i64>,
    std::collections::HashMap<usize, String>,
    std::collections::HashMap<usize, i64>,
)> {
    let source = read_source(file).map_err(|e| CompileError::Lex(
        Diagnostic::unknown(e),
    ))?;
    let file_str = file.to_string_lossy().to_string();

    let mut lexer = Lexer::with_file(&source, &file_str);
    let tokens = lexer.tokenize()?;

    let mut parser = ManiParser::with_file(tokens, &file_str);
    let program = parser.parse()?;

    let mut analyzer = SemanticAnalyzer::with_file(&file_str);
    let typed_program = analyzer.analyze(&program)?;
    analyzer.warnings.emit_all_rich(&source);
    analyzer.warnings.check_error()?;

    manitc::borrow::check_borrows(&typed_program)?;

    let mut ir_module = IRLowerer::lower_with(&typed_program, lang);
    ir::optimize::run_passes(&mut ir_module);

    let asm_text = codegen_t3::emit_t3_asm(&ir_module)?;
    codegen_t3::assemble(&asm_text).map_err(|e| CompileError::Codegen(
        Diagnostic::unknown(format!("T3ISA assembler error: {}", e))))
}

fn run_t3(
    file: &PathBuf,
    debug: bool,
    max_steps: usize,
    profile: bool,
    prog_args: &[String],
    lang: manitc::lang::LangVersion,
) -> CompileResult<()> {
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");

    let (words, str_data, float_data) = if ext == "mt" {
        // A maniT SOURCE file: auto-compile it and run the result, instead of
        // executing the raw source bytes as machine words.
        compile_t3_in_memory(file, lang)?
    } else {
        let (words, has_magic) = codegen_t3::read_t3_binary_with_magic(file.to_str().unwrap())
            .map_err(|e| crate::error::CompileError::Codegen(
                crate::error::Diagnostic::unknown(e.to_string())))?;
        // Legacy .t3b binaries (written before the magic header) are still
        // accepted; anything else without the magic is not a T3 binary.
        if !has_magic && ext != "t3b" {
            return Err(crate::error::CompileError::Codegen(
                crate::error::Diagnostic::unknown(format!(
                    "{} is not a T3ISA binary (missing .t3b magic header); \
                     pass a .t3b produced by `manitc compile --target t3`, or a .mt source file",
                    file.display()))));
        }
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
        (words, str_data, float_data)
    };

    // argv[0] is the file the emulator was handed, mirroring the LLVM backend,
    // where env_argc/env_arg read /proc/self/cmdline and argv[0] is the binary
    // path. A program that asks `argc() > 1` therefore gets the same answer on
    // both backends when it is run with no arguments — which is how the
    // shipped-corpus survey runs everything.
    let argv: Vec<String> = std::iter::once(file.display().to_string())
        .chain(prog_args.iter().cloned())
        .collect();

    pipe_safe_print(&format!("[T3ISA] running {} ({} words)\n", file.display(), words.len()));
    let (output_lines, exit_code, prof) = if debug {
        // The interactive debugger drives the emulator itself and hands back
        // only the output, so there is no profile to report from this path.
        (codegen_t3::run_emulator_debug_argv(words, str_data, float_data, argv), 0, None)
    } else {
        let (out, code, p) = codegen_t3::run_emulator_with_exit_capped_argv_profiled(
            words, str_data, float_data, max_steps, argv,
        );
        (out, code, Some(p))
    };
    for piece in &output_lines {
        pipe_safe_print(piece);
    }
    // AFTER the program's own output, and on stderr: a profile is about the
    // run, not part of it. Printed even when the program trapped or was cut
    // off at the step limit — a profile of a run that did not finish is still
    // the record of what it did before it stopped, and for a suspected
    // infinite loop it is the most useful profile there is.
    if profile {
        if let Some(p) = &prof {
            eprint!("{}", p.report());
        }
    }
    // main's return value becomes the process exit status (low 8 bits).
    if exit_code != 0 {
        std::process::exit((exit_code as i32) & 0xff);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Benchmark: compile to both targets, run, compare
// ---------------------------------------------------------------------------


// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

/// Stack reserved for the compiler thread (A3).
///
/// The constant and the spawning helper now live in the library
/// (`manitc::COMPILER_STACK_BYTES`, `manitc::with_compiler_stack`), because
/// the guarantee belongs to anyone who runs a compiler pass and not only to
/// this binary — see the note there.
use manitc::COMPILER_STACK_BYTES;

fn main() {
    // Run the whole CLI on a thread with a large stack (A3). `Lsp` is handled
    // inside `run` as before; the tokio runtime it builds gets its own threads.
    let child = std::thread::Builder::new()
        .name("manitc".to_string())
        .stack_size(COMPILER_STACK_BYTES)
        .spawn(run)
        .expect("failed to spawn the compiler thread");
    match child.join() {
        Ok(()) => {}
        // The worker already reported the failure (or panicked and printed its
        // own message); propagate a failing status without a second report.
        Err(_) => std::process::exit(1),
    }
}

fn run() {
    let cli = Cli::parse();

    let result: CompileResult<()> = match &cli.command {
        Commands::Compile {
            file, target, output, emit_ir, warn_as_error,
            allow, warn, deny, forbid, print_lints, target_triple, lang, verify_ssa,
            mem2reg, no_mem2reg, pass_stats, rounds, inline_limit, no_inline,
            no_merge_blocks,
        } => {
            let flags = LintFlags {
                allow: allow.clone(), warn: warn.clone(),
                deny: deny.clone(), forbid: forbid.clone(),
            };
            // Promotion is the default; `--mem2reg` is a no-op that asks for it
            // explicitly, and `--no-mem2reg` is the only way to decline it.
            let _ = mem2reg;
            parse_lang(lang).and_then(|lang| run_compile(
                file, target, output, *emit_ir, *warn_as_error,
                &flags, *print_lints, target_triple.as_deref(), lang, *verify_ssa,
                !*no_mem2reg, *pass_stats, *rounds,
                if *no_inline { 0 } else { *inline_limit },
                !*no_merge_blocks,
            ))
        }
        Commands::Check {
            file, warn_as_error, allow, warn, deny, forbid, print_lints, backend, lang,
        } => {
            let flags = LintFlags {
                allow: allow.clone(), warn: warn.clone(),
                deny: deny.clone(), forbid: forbid.clone(),
            };
            parse_lang(lang).and_then(|lang| {
                run_check(file, *warn_as_error, &flags, *print_lints, backend.as_deref(), lang)
            })
        }
        Commands::Lex { file } => run_lex(file),
        Commands::Parse { file } => run_parse(file),
        Commands::RunT3 { file, debug, max_steps, profile, args, lang } => {
            parse_lang(lang)
                .and_then(|lang| run_t3(file, *debug, *max_steps, *profile, args, lang))
        }
        Commands::Bench { file, iterations, lang } => {
            parse_lang(lang).and_then(|lang| run_bench(file, *iterations, lang))
        }
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
