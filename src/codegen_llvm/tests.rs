use super::*;

fn add_module() -> IRModule {
    let block = IRBlock {
        label: "entry".to_string(),
        instrs: vec![IRInstr::BinOp {
            dst: IRTemp::new("result"),
            op: IRBinOp::Add,
            lhs: IRValue::Temp(IRTemp::new("param_a")),
            rhs: IRValue::Temp(IRTemp::new("param_b")),
            ty: IRType::I64,
        }],
        term: IRTerminator::Return(Some(IRValue::Temp(IRTemp::new("result")))),
    };

    IRModule {
        name: "test".to_string(),
        functions: vec![IRFunction {
            name: "add".to_string(),
            params: vec![
                ("a".to_string(), IRType::I64),
                ("b".to_string(), IRType::I64),
            ],
            ret_ty: IRType::I64,
            blocks: vec![block],
            is_extern: false,
        }],
        globals: vec![],
        string_literals: vec![],
        float_literals: vec![],
        struct_sizes: std::collections::HashMap::new(),
    }
}

#[test]
fn test_basic_function() {
    let m = add_module();
    let ir = emit_llvm_ir(&m, None);
    assert!(ir.contains("define i64 @add(i64 %param_a, i64 %param_b)"));
    assert!(ir.contains("%result = add i64 %param_a, %param_b"));
    assert!(ir.contains("ret i64 %result"));
}

#[test]
fn test_preamble() {
    let ir = emit_llvm_ir(&add_module(), None);
    assert!(ir.contains("target triple = \"x86_64-pc-linux-gnu\""));
    assert!(ir.contains("target datalayout"));
    assert!(ir.contains("declare i32 @printf"));
    assert!(ir.contains("declare ptr @malloc"));
}

#[test]
fn test_extern_function() {
    let m = IRModule {
        name: "ext".to_string(),
        functions: vec![IRFunction {
            name: "ext_fn".to_string(),
            params: vec![("x".to_string(), IRType::I64)],
            ret_ty: IRType::Void,
            blocks: vec![],
            is_extern: true,
        }],
        globals: vec![],
        string_literals: vec![],
        float_literals: vec![],
        struct_sizes: std::collections::HashMap::new(),
    };
    let ir = emit_llvm_ir(&m, None);
    assert!(ir.contains("declare void @ext_fn(i64)"));
}

#[test]
fn test_global_variable() {
    let m = IRModule {
        name: "gtest".to_string(),
        functions: vec![],
        globals: vec![IRGlobal {
            name: "counter".to_string(),
            ty: IRType::I64,
            init: Some(IRValue::Const(IRConst::Int(42))),
        }],
        string_literals: vec![],
        float_literals: vec![],
        struct_sizes: std::collections::HashMap::new(),
    };
    let ir = emit_llvm_ir(&m, None);
    assert!(ir.contains("@counter = global i64 42"));
}

#[test]
fn test_string_literal() {
    let m = IRModule {
        name: "stest".to_string(),
        functions: vec![],
        globals: vec![],
        string_literals: vec![("@str0".to_string(), "hello".to_string())],
        float_literals: vec![],
        struct_sizes: std::collections::HashMap::new(),
    };
    let ir = emit_llvm_ir(&m, None);
    // "hello" has 5 bytes + 1 null = 6
    assert!(ir.contains("@str0 = private unnamed_addr constant [6 x i8] c\"hello\\00\""));
}

#[test]
fn test_llvm_escape_string() {
    assert_eq!(llvm_escape_string("hello"), "hello");
    assert_eq!(llvm_escape_string("\n"), "\\0A");
    assert_eq!(llvm_escape_string("\""), "\\22");
    assert_eq!(llvm_escape_string("\\"), "\\5C");
    assert_eq!(llvm_escape_string("hi\0"), "hi\\00");
}

#[test]
fn test_trit_helpers_present() {
    let ir = emit_llvm_ir(&add_module(), None);
    assert!(ir.contains("define internal void @__manit_print_trit"));
    assert!(ir.contains("define internal void @__manit_print_bool3"));
}

#[test]
fn test_float_operations() {
    let block = IRBlock {
        label: "entry".to_string(),
        instrs: vec![
            IRInstr::BinOp {
                dst: IRTemp::new("r"),
                op: IRBinOp::Add,
                lhs: IRValue::Temp(IRTemp::new("param_x")),
                rhs: IRValue::Const(IRConst::Float(1.5)),
                ty: IRType::F64,
            },
        ],
        term: IRTerminator::Return(Some(IRValue::Temp(IRTemp::new("r")))),
    };
    let m = IRModule {
        name: "ftest".to_string(),
        functions: vec![IRFunction {
            name: "fadd_one".to_string(),
            params: vec![("x".to_string(), IRType::F64)],
            ret_ty: IRType::F64,
            blocks: vec![block],
            is_extern: false,
        }],
        globals: vec![],
        string_literals: vec![],
        float_literals: vec![],
        struct_sizes: std::collections::HashMap::new(),
    };
    let ir = emit_llvm_ir(&m, None);
    assert!(ir.contains("fadd double %param_x,"));
    assert!(ir.contains("ret double"));
}

#[test]
fn test_alloca_and_store_load() {
    let block = IRBlock {
        label: "entry".to_string(),
        instrs: vec![
            IRInstr::Alloca { dst: IRTemp::new("slot"), ty: IRType::I64 },
            IRInstr::Store {
                ptr: IRValue::Temp(IRTemp::new("slot")),
                val: IRValue::Const(IRConst::Int(7)),
                ty: IRType::I64,
            },
            IRInstr::Load {
                dst: IRTemp::new("v"),
                ptr: IRValue::Temp(IRTemp::new("slot")),
                ty: IRType::I64,
            },
        ],
        term: IRTerminator::Return(Some(IRValue::Temp(IRTemp::new("v")))),
    };
    let m = IRModule {
        name: "atest".to_string(),
        functions: vec![IRFunction {
            name: "seven".to_string(),
            params: vec![],
            ret_ty: IRType::I64,
            blocks: vec![block],
            is_extern: false,
        }],
        globals: vec![],
        string_literals: vec![],
        float_literals: vec![],
        struct_sizes: std::collections::HashMap::new(),
    };
    let ir = emit_llvm_ir(&m, None);
    assert!(ir.contains("%slot = alloca i64"));
    assert!(ir.contains("store i64 7, ptr %slot"));
    assert!(ir.contains("%v = load i64, ptr %slot"));
}

// ---------------------------------------------------------------------------
// B9: cast completeness — every scalar type pair must produce legal IR text.
// ---------------------------------------------------------------------------

/// All scalar types a `as` cast can involve at the IR level.
fn scalar_types() -> Vec<IRType> {
    vec![
        IRType::I64,
        IRType::I32,
        IRType::I16,
        IRType::I8,
        IRType::Bool,
        IRType::Trit,
        IRType::F64,
        IRType::Ptr(Box::new(IRType::I8)),
    ]
}

/// A bitcast between differently-spelled first-class types is the illegal
/// size-changing form B9 describes; same-type bitcasts are legal no-ops.
fn assert_no_illegal_bitcast(text: &str) {
    for line in text.lines() {
        if let Some(pos) = line.find(" bitcast ") {
            let rest = &line[pos + " bitcast ".len()..];
            let mut parts = rest.split_whitespace();
            let from = parts.next().unwrap_or("");
            // "<from> <val> to <to>"
            let to = rest.split(" to ").nth(1).unwrap_or("").trim();
            assert_eq!(
                from, to,
                "size-changing bitcast emitted (B9): {}",
                line
            );
        }
    }
}

#[test]
fn test_cast_sequence_all_pairs_legal() {
    for from in scalar_types() {
        for to in scalar_types() {
            let text = cast_sequence("t0", "%src", &from, &to);
            assert!(
                !text.is_empty(),
                "cast {:?} -> {:?} produced no instruction",
                from,
                to
            );
            assert!(
                text.contains("%t0 ="),
                "cast {:?} -> {:?} must define %t0, got: {}",
                from,
                to,
                text
            );
            assert_no_illegal_bitcast(&text);
        }
    }
}

#[test]
fn test_cast_trit_bool_semantics() {
    // trit -> bool: nonzero is true (icmp, never a bitcast).
    let t2b = cast_sequence("t0", "%src", &IRType::Trit, &IRType::Bool);
    assert_eq!(t2b, "%t0 = icmp ne i8 %src, 0");
    // bool -> trit: true -> +1, false -> 0.
    let b2t = cast_sequence("t0", "%src", &IRType::Bool, &IRType::Trit);
    assert_eq!(b2t, "%t0 = zext i1 %src to i8");
    // int -> bool is icmp as well (2 as bool must be true, not trunc parity).
    let i2b = cast_sequence("t0", "%src", &IRType::I64, &IRType::Bool);
    assert_eq!(i2b, "%t0 = icmp ne i64 %src, 0");
}

#[test]
fn test_cast_bool_float_semantics() {
    let b2f = cast_sequence("t0", "%src", &IRType::Bool, &IRType::F64);
    assert_eq!(b2f, "%t0 = uitofp i1 %src to double");
    let f2b = cast_sequence("t0", "%src", &IRType::F64, &IRType::Bool);
    assert_eq!(f2b, "%t0 = fcmp one double %src, 0.0");
}

#[test]
fn test_cast_int_to_trit_clamps() {
    // docs/language-reference.md: `as trit` clamps to {-1, 0, +1}.
    let text = cast_sequence("t0", "%src", &IRType::I64, &IRType::Trit);
    assert!(text.contains("icmp sgt i64 %src, 0"), "missing pos test: {}", text);
    assert!(text.contains("icmp slt i64 %src, 0"), "missing neg test: {}", text);
    assert!(text.contains("select"), "clamp must use select: {}", text);
    // trit widening stays a plain sext
    let widen = cast_sequence("t0", "%src", &IRType::Trit, &IRType::I64);
    assert_eq!(widen, "%t0 = sext i8 %src to i64");
}

// ---------------------------------------------------------------------------
// B10: vararg calls must carry the full vararg function type.
// ---------------------------------------------------------------------------

#[test]
fn test_parse_declare_sigs_keeps_vararg_marker() {
    let sigs = parse_declare_sigs(STDLIB_DECLARES);
    let (params, ret) = sigs.get("fmt_format").expect("fmt_format missing");
    assert_eq!(ret, "ptr");
    assert_eq!(
        params.last().map(String::as_str),
        Some("..."),
        "the vararg marker must survive parsing (B10), got {:?}",
        params
    );
}

#[test]
fn test_vararg_call_emits_full_function_type() {
    let block = IRBlock {
        label: "entry".to_string(),
        instrs: vec![IRInstr::Call {
            dst: Some(IRTemp::new("s")),
            func: "fmt_format".to_string(),
            args: vec![
                IRValue::Global("fmt0".to_string()),
                IRValue::Const(IRConst::Int(42)),
            ],
            ret_ty: IRType::Ptr(Box::new(IRType::I8)),
        }],
        term: IRTerminator::Return(None),
    };
    let m = IRModule {
        name: "vtest".to_string(),
        functions: vec![IRFunction {
            name: "use_fmt".to_string(),
            params: vec![],
            ret_ty: IRType::Void,
            blocks: vec![block],
            is_extern: false,
        }],
        globals: vec![],
        string_literals: vec![("fmt0".to_string(), "n = {}".to_string())],
        float_literals: vec![],
        struct_sizes: std::collections::HashMap::new(),
    };
    let ir = emit_llvm_ir(&m, None);
    assert!(
        ir.contains("call ptr (ptr, ...) @fmt_format(ptr @fmt0, i64 42)"),
        "vararg call must spell the full function type (B10), got:\n{}",
        ir.lines()
            .filter(|l| l.contains("fmt_format") && !l.starts_with("declare"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// ---------------------------------------------------------------------------
// C-ABI main wrapper (exit status) and internal runtime helpers.
// ---------------------------------------------------------------------------

#[test]
fn test_main_wrapper_emitted() {
    let block = IRBlock {
        label: "entry".to_string(),
        instrs: vec![],
        term: IRTerminator::Return(None),
    };
    let m = IRModule {
        name: "mwtest".to_string(),
        functions: vec![IRFunction {
            name: "main".to_string(),
            params: vec![],
            ret_ty: IRType::Void,
            blocks: vec![block],
            is_extern: false,
        }],
        globals: vec![],
        string_literals: vec![],
        float_literals: vec![],
        struct_sizes: std::collections::HashMap::new(),
    };
    let ir = emit_llvm_ir(&m, None);
    assert!(ir.contains("define void @__manit_main()"), "user main must be renamed");
    assert!(ir.contains("define i32 @main()"), "C-ABI wrapper must exist");
    assert!(ir.contains("ret i32 0"), "void main must exit 0");
}

#[test]
fn test_internal_runtime_helpers_present() {
    let ir = emit_llvm_ir(&add_module(), None);
    // io println variants the C runtime does not export (K1)
    assert!(ir.contains("define internal void @io_println_bool3(i8"));
    assert!(ir.contains("define internal void @io_println_trit(i8"));
    assert!(ir.contains("define internal void @io_println_bool(i1"));
    // Result constructors with the IR's two-word layout
    assert!(ir.contains("define internal ptr @Err_new(ptr"));
    // @result_unwrap is DELETED, not merely absent: it read word 1 with no tag
    // check, so unwrapping an Err returned the message pointer as the value,
    // and it was LLVM-only. Both backends now share the lowering in
    // ir/lower/lower_result.rs. Pinned so it cannot come back.
    assert!(!ir.contains("@result_unwrap"),
        "@result_unwrap was an unchecked, LLVM-only unwrap and must stay deleted");
    // The unwrap tag guard, whose T3 twin is SYSCALL #561.
    assert!(ir.contains("declare void @manit_check_result_ok(i64)"));
    // ternary array helpers mirroring the T3 emulator
    assert!(ir.contains("define internal ptr @ternary_trits_to_str(ptr"));
    assert!(ir.contains("define internal i64 @ternary_pack_trits(ptr"));
    // no symbol may be both declared and defined
    for name in [
        "@io_println_bool3", "@Err_new", "@Ok_new",
        "@ternary_trits_to_str", "@math_to_balanced_ternary",
    ] {
        assert!(
            !ir.contains(&format!("declare {} ", name))
                && !ir.lines().any(|l| l.starts_with("declare") && l.contains(&format!("{}(", name))),
            "{} must not be declared as well as defined",
            name
        );
    }
}

#[test]
fn test_parse_declare_sigs() {
    let sigs = parse_declare_sigs(STDLIB_DECLARES);
    // void @io_println(ptr)
    let (params, ret) = sigs.get("io_println").expect("io_println missing");
    assert_eq!(ret, "void");
    assert_eq!(params, &["ptr".to_string()]);
    // ptr @fmt_concat(ptr, ptr)
    let (params, ret) = sigs.get("fmt_concat").expect("fmt_concat missing");
    assert_eq!(ret, "ptr");
    assert_eq!(params, &["ptr".to_string(), "ptr".to_string()]);
    // i64 @math_trit_count(i64)
    //
    // This was `math_abs` and `math_pow` until 20 August 2026. Both were deleted
    // when `math::abs` and `math::pow` became ManiT source — the point of this
    // test is that parse_declare_sigs reads an integer signature and a
    // floating-point one correctly, so any surviving pair of that shape serves.
    // math_trit_count is a good long-term choice: it is one of only three
    // math:: functions the T3 backend intercepts natively, so it is the least
    // likely of them to move into ManiT later.
    let (params, ret) = sigs.get("math_trit_count").expect("math_trit_count missing");
    assert_eq!(ret, "i64");
    assert_eq!(params, &["i64".to_string()]);
    // double @math_abs_float(double) — math_sqrt used to be the example here.
    // It is gone: all 34 of math::'s float functions moved into ManiT bodies, so
    // a C definition alongside them would shadow the shared body. Pick a survivor
    // that is not a candidate to move, or this test breaks again on the next
    // migration.
    let (params, ret) = sigs.get("math_abs_float").expect("math_abs_float missing");
    assert_eq!(ret, "double");
    assert_eq!(params, &["double".to_string()]);
}

#[test]
fn test_call_uses_declared_types() {
    // Build a module that calls io_println with a string argument.
    // The Call should emit "call void @io_println(ptr ...)" not
    // "call i64 @io_println(i64 ...)".
    let block = IRBlock {
        label: "entry".to_string(),
        instrs: vec![
            IRInstr::Call {
                dst: None,
                func: "io_println".to_string(),
                args: vec![IRValue::Global("greeting".to_string())],
                ret_ty: IRType::I64, // deliberately wrong — should be overridden
            },
        ],
        term: IRTerminator::Return(None),
    };
    let m = IRModule {
        name: "calltest".to_string(),
        functions: vec![IRFunction {
            name: "main".to_string(),
            params: vec![],
            ret_ty: IRType::Void,
            blocks: vec![block],
            is_extern: false,
        }],
        globals: vec![],
        string_literals: vec![("greeting".to_string(), "hello".to_string())],
        float_literals: vec![],
        struct_sizes: std::collections::HashMap::new(),
    };
    let ir = emit_llvm_ir(&m, None);
    // Must use void return and ptr param, not i64.
    assert!(
        ir.contains("call void @io_println(ptr @greeting)"),
        "Expected 'call void @io_println(ptr @greeting)' in IR, got:\n{}",
        ir.lines()
            .filter(|l| l.contains("io_println"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    // Must NOT contain the wrong types
    assert!(!ir.contains("call i64 @io_println"));
}

// ===========================================================================
// The C-runtime ABI must match, attribute for attribute.
//
// maniTC declared every trit parameter as a bare `i8`, while clang compiles the
// C runtime's `int8_t` as `i8 signext`. At -O0 the mismatch is invisible; from
// -O1 the caller stops zero-filling the register by accident and every `-1`
// trit arrives as 255. Verified by re-linking one example against an -O2
// runtime with only the `signext` attributes removed:
//
//     -  -> int value: -1      becomes      -  -> int value: 255
//     0t--+ = -11              becomes      0t--+ = 245
// ===========================================================================

#[test]
fn runtime_trit_declares_carry_signext() {
    use super::helpers::STDLIB_DECLARES;
    // Every runtime entry point taking or returning a scalar trit/char.
    // `ternary_int_to_trit` and `ternary_trit_median` were on this list until
    // 19 August 2026. They are ManiT source in stdlib/ternary.mt now, so they
    // no longer cross the C boundary at all and have no declare to check —
    // moving them is what fixed their DIVERGENT status.
    //
    // `fmt_show_trit` and `fmt_show_bool3` left for the same reason on
    // 20 August 2026: both are three-armed `tif` expressions in stdlib/fmt.mt
    // now, their C bodies are deleted, and a trit that never reaches C cannot
    // be sign-extended wrongly on the way.
    for name in [
        "ternary_trit_to_int",
        "io_print_trit", "io_print_bool3", "io_print_char", "io_print_tryte",
        "str_char_at",
        "AtomicTrit_new", "AtomicTrit_get", "AtomicTrit_set", "AtomicTrit_swap",
        "AtomicTrit_fetch_and", "AtomicTrit_fetch_or", "AtomicTrit_fetch_neg",
        "AtomicTrit_compare_exchange",
    ] {
        let line = STDLIB_DECLARES
            .lines()
            .find(|l| l.contains(&format!("@{}(", name)))
            .unwrap_or_else(|| panic!("no declare for @{}", name));
        assert!(
            line.contains("signext"),
            "@{} takes or returns a signed i8 in the C runtime, so its declare \
             must say signext, got:\n  {}",
            name, line,
        );
    }
}

#[test]
fn parse_declare_sigs_strips_abi_attributes() {
    // The attribute belongs to the declaration, not the type. Leaving it
    // attached made downstream type logic emit `sext i8 %t to i8 signext`.
    use super::helpers::parse_declare_sigs;
    let sigs = parse_declare_sigs(
        "declare signext i8 @f(ptr, i8 signext, i64 noundef)\n\
         declare ptr @g(ptr, ...)\n",
    );
    let (params, ret) = sigs.get("f").expect("f should parse");
    assert_eq!(ret, "i8", "return type must come back bare");
    assert_eq!(params, &vec!["ptr".to_string(), "i8".to_string(), "i64".to_string()]);
    // The vararg marker is not an attribute and must survive.
    let (gparams, _) = sigs.get("g").expect("g should parse");
    assert_eq!(gparams.last().map(String::as_str), Some("..."));
}

/// The RAVAN check: derive the ABI from the C runtime itself rather than
/// trusting the list above. Skips when clang or the runtime source is absent.
#[test]
fn runtime_declares_match_the_abi_clang_actually_emits() {
    use super::helpers::STDLIB_DECLARES;
    use std::path::PathBuf;
    use std::process::Command;

    // runtime_link lives in the binary crate, so resolve both here.
    let clang = ["clang", "clang-19", "clang-18", "clang-17",
                 "/usr/lib/llvm-19/bin/clang"]
        .into_iter()
        .find(|c| {
            Command::new(c).arg("--version").output()
                .map(|o| o.status.success()).unwrap_or(false)
        });
    let Some(clang) = clang else {
        eprintln!("skipping: no clang on this machine");
        return;
    };
    let rt = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime/manit_runtime.c");
    if !rt.exists() {
        eprintln!("skipping: manit_runtime.c not found");
        return;
    }

    let dir = std::env::temp_dir().join(format!("manitc_abi_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let ll = dir.join("rt.ll");
    let out = Command::new(&clang)
        .args([
            rt.to_str().unwrap(), "-S", "-emit-llvm", "-O0", "-w",
            "-DMANIT_NO_GUI", "-o", ll.to_str().unwrap(),
        ])
        .output()
        .expect("run clang");
    if !out.status.success() {
        eprintln!("skipping: runtime would not compile here");
        return;
    }
    let ir = std::fs::read_to_string(&ll).expect("read runtime IR");

    let mut checked = 0usize;
    for line in ir.lines() {
        let Some(rest) = line.strip_prefix("define ") else { continue };
        let Some(at) = rest.find('@') else { continue };
        let Some(open) = rest[at..].find('(').map(|p| p + at) else { continue };
        let Some(close) = rest[open..].find(')').map(|p| p + open) else { continue };
        let name = &rest[at + 1..open];
        let ret = &rest[..at];
        let params = &rest[open + 1..close];

        // Only the functions where a scalar i8 crosses the boundary. Split the
        // return and the parameter list apart first: in `void @f(i8 noundef %0`
        // there is no space after `(`, so a whitespace scan over the whole
        // signature sees the token `@f(i8` and misses the parameter entirely.
        let is_i8 = |slot: &str| {
            slot.split_whitespace().next().map(|w| w.trim_end_matches(',')) == Some("i8")
        };
        let crosses_i8 = ret.split_whitespace().any(|w| w == "i8")
            || params.split(',').any(is_i8);
        if !crosses_i8 {
            continue;
        }
        let Some(decl) = STDLIB_DECLARES
            .lines()
            .find(|l| l.contains(&format!("@{}(", name)))
        else { continue };
        assert!(
            decl.contains("signext"),
            "@{} crosses the boundary with a scalar i8. clang emits:\n  {}\n\
             maniTC declares:\n  {}\n\
             The declaration must carry signext, or every -1 trit arrives as 255 \
             once the runtime is optimised.",
            name, &rest[..close], decl,
        );
        checked += 1;
    }
    // The floor tracks how many i8-crossing runtime functions actually exist.
    // It was 15 until 20 August 2026, when fmt_show_trit and fmt_show_bool3
    // were deleted from runtime/core.c and reimplemented in ManiT. Lower it
    // only alongside such a removal — a drop with no explanation means the
    // scan silently stopped finding things, which is the failure this guards.
    assert!(checked >= 14, "expected the whole trit runtime surface, checked {}", checked);
}
