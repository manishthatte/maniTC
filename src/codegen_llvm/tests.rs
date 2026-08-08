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
    // i64 @math_abs(i64)
    let (params, ret) = sigs.get("math_abs").expect("math_abs missing");
    assert_eq!(ret, "i64");
    assert_eq!(params, &["i64".to_string()]);
    // double @math_pow(double, double)
    let (params, ret) = sigs.get("math_pow").expect("math_pow missing");
    assert_eq!(ret, "double");
    assert_eq!(params, &["double".to_string(), "double".to_string()]);
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
