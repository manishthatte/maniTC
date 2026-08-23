use crate::semantic::ManiType;

/// Strip generic parameters from a type name string.
/// e.g. "Vec<int>" → "Vec", "Map<str,int>" → "Map", "int" → "int"
#[allow(dead_code)]
pub(super) fn strip_generics(type_name: &str) -> &str {
    if let Some(idx) = type_name.find('<') {
        &type_name[..idx]
    } else {
        type_name
    }
}

// ---------------------------------------------------------------------------
// IR Module
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IRModule {
    pub name: String,
    pub functions: Vec<IRFunction>,
    pub globals: Vec<IRGlobal>,
    pub string_literals: Vec<(String, String)>, // (label, content)
    pub float_literals: Vec<(String, i64)>,    // (label, f64-bits-as-i64)
    pub static_structs: Vec<IRStaticStruct>,   // payloads for struct-valued globals
    pub struct_sizes: std::collections::HashMap<String, usize>, // struct name → number of fields
}

#[derive(Debug, Clone)]
pub struct IRGlobal {
    pub name: String,
    pub ty: IRType,
    pub init: Option<IRValue>,
}

/// The static storage behind a struct constant.
///
/// A struct VALUE in maniT is a pointer to n_fields consecutive 8-byte slots
/// (`slot_access_ty`), so a module-level `let` of struct type cannot hold the
/// struct — it holds its address, exactly as a `str` global holds the address
/// of its `.data` entry. This is the thing the address points at.
///
/// Emitted once per struct constant. A field that is itself a struct constant
/// gets its own entry and is referenced from its parent as `IRValue::Global`,
/// so nesting needs no extra machinery on either backend.
#[derive(Debug, Clone)]
pub struct IRStaticStruct {
    /// The symbol the payload is emitted under, and what `IRValue::Global`
    /// refers to from the initialiser that points here.
    pub label: String,
    /// The struct this is an instance of — for the comment the backends emit,
    /// and to keep the payload readable in generated assembly.
    pub struct_name: String,
    /// One entry per field IN DECLARATION ORDER (`check_expr` normalises
    /// struct literals to that order, and field access assigns by position).
    ///
    /// The `IRType` is the field's SLOT type — what `slot_access_ty` returns
    /// for it — so the static payload's layout is, by construction, the same
    /// layout the runtime load/store path uses rather than a second guess at
    /// it.
    pub fields: Vec<(IRType, IRValue)>,
}

#[derive(Debug, Clone)]
pub struct IRFunction {
    pub name: String,
    pub params: Vec<(String, IRType)>,
    pub ret_ty: IRType,
    pub blocks: Vec<IRBlock>,
    pub is_extern: bool,
}

#[derive(Debug, Clone)]
pub struct IRBlock {
    pub label: String,
    pub instrs: Vec<IRInstr>,
    pub term: IRTerminator,
}

impl IRBlock {
    pub fn new(label: impl Into<String>) -> Self {
        IRBlock {
            label: label.into(),
            instrs: Vec::new(),
            term: IRTerminator::Unreachable,
        }
    }
}

// ---------------------------------------------------------------------------
// Values and constants
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IRTemp(pub String);

impl IRTemp {
    pub fn new(name: impl Into<String>) -> Self {
        IRTemp(name.into())
    }
}

impl std::fmt::Display for IRTemp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "%{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub enum IRValue {
    Temp(IRTemp),
    Const(IRConst),
    Global(String),
    Void,
}

#[derive(Debug, Clone)]
pub enum IRConst {
    Int(i64),
    Float(f64),
    Bool(bool),
    Trit(i8),    // -1, 0, +1
    Str(String), // string literal label
    Null,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Arity of a structural tuple type, recovered from its IR type name.
///
/// `IRType::from_mani` encodes it as `<tuple:N>` precisely so the size
/// survives to the backends: tuples are structural, so unlike declared
/// structs and enums they never appear in `IRModule::struct_sizes`. Both
/// backends size a tuple allocation through this — see the `Alloca` arms in
/// codegen_llvm/emit_instr.rs and codegen_t3/emitter/emit_instr.rs.
pub fn tuple_arity_from_name(name: &str) -> Option<usize> {
    name.strip_prefix("<tuple:")?.strip_suffix('>')?.parse().ok()
}

#[derive(Debug, Clone, PartialEq)]
pub enum IRType {
    I64,
    F64,
    I8,
    I16,
    I32,
    Bool,
    Trit, // stored as i8: -1, 0, +1
    Ptr(Box<IRType>),
    Array(Box<IRType>, usize),
    Struct(String),
    Void,
}

impl IRType {
    pub fn from_mani(ty: &ManiType) -> IRType {
        match ty {
            ManiType::Int => IRType::I64,
            ManiType::Float => IRType::F64,
            ManiType::Bool => IRType::Bool,
            ManiType::Bool3 => IRType::I8,
            ManiType::Trit => IRType::Trit,
            ManiType::Tryte => IRType::I16, // 6 trits: max 364, fits I16 (not I8)
            ManiType::T9 => IRType::I32,
            ManiType::T27 => IRType::I64, // 27 trits: max ±3.8×10^12, overflows I32
            ManiType::T54 => IRType::I64,
            // ManiType::Trint is merged into T54 (both map to I64)
            ManiType::Tfloat => IRType::F64,
            ManiType::Str => IRType::Ptr(Box::new(IRType::I8)),
            ManiType::Char => IRType::I8,
            ManiType::Void => IRType::Void,
            ManiType::Array(elem, Some(n)) => {
                IRType::Array(Box::new(IRType::from_mani(elem)), *n)
            }
            ManiType::Array(elem, None) => {
                IRType::Ptr(Box::new(IRType::from_mani(elem)))
            }
            // The arity is part of the name because it is the only place the
            // size survives to the backends. Every tuple used to map to the
            // single name "<tuple>", which is in no struct table, so the LLVM
            // backend's size lookup fell through to its `unwrap_or(1)` default
            // and malloc'd 8 bytes for a tuple of ANY arity — a heap overflow
            // of 8 bytes per extra element, on every tuple construction.
            // See tuple_arity_from_name, just above.
            ManiType::Tuple(elems) => IRType::Struct(format!("<tuple:{}>", elems.len())),
            ManiType::Struct(name) => IRType::Struct(name.clone()),
            ManiType::Enum(name) => IRType::Struct(name.clone()),
            ManiType::Fn(_, _) => IRType::Ptr(Box::new(IRType::I8)),
            ManiType::Generic(_, _) => IRType::Ptr(Box::new(IRType::I8)),
            ManiType::Unknown => IRType::I64,
        }
    }
}

// ---------------------------------------------------------------------------
// Instructions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum IRInstr {
    BinOp {
        dst: IRTemp,
        op: IRBinOp,
        lhs: IRValue,
        rhs: IRValue,
        ty: IRType,
    },
    UnOp {
        dst: IRTemp,
        op: IRUnOp,
        operand: IRValue,
        ty: IRType,
    },
    Assign {
        dst: IRTemp,
        src: IRValue,
        ty: IRType,
    },
    Alloca {
        dst: IRTemp,
        ty: IRType,
    },
    Store {
        ptr: IRValue,
        val: IRValue,
        ty: IRType,
    },
    Load {
        dst: IRTemp,
        ptr: IRValue,
        ty: IRType,
    },
    Call {
        dst: Option<IRTemp>,
        func: String,
        args: Vec<IRValue>,
        ret_ty: IRType,
    },
    CallIndirect {
        dst: Option<IRTemp>,
        fn_ptr: IRValue,
        args: Vec<IRValue>,
        ret_ty: IRType,
    },
    GetPtr {
        dst: IRTemp,
        ptr: IRValue,
        idx: IRValue,
        ty: IRType,
    },
    /// Verify `0 <= idx < len` before a fixed-length array access (A2).
    ///
    /// Emitted only where the element count is statically known, immediately
    /// before the corresponding GetPtr. Unchecked, an out-of-range index
    /// segfaulted on LLVM and read adjacent emulator memory on T3 (a[-1]
    /// returned the array's own length header). A separate instruction rather
    /// than a field on GetPtr, which is also used for struct-field and
    /// slot projections where no bound applies.
    BoundsCheck {
        idx: IRValue,
        len: usize,
    },
    // Ternary operations
    TritMin {
        dst: IRTemp,
        a: IRValue,
        b: IRValue,
    },
    TritMax {
        dst: IRTemp,
        a: IRValue,
        b: IRValue,
    },
    TritNeg {
        dst: IRTemp,
        a: IRValue,
    },
    // Intrinsic print operations
    PrintStr(IRValue),
    PrintInt(IRValue),
    PrintFloat(IRValue),
    PrintBool3(IRValue),
    PrintTrit(IRValue),
    // SSA Phi node
    Phi {
        dst: IRTemp,
        ty: IRType,
        incoming: Vec<(IRValue, String)>,
    },
    // Type cast
    Cast {
        dst: IRTemp,
        src: IRValue,
        from_ty: IRType,
        to_ty: IRType,
    },
}

// ---------------------------------------------------------------------------
// Terminators
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum IRTerminator {
    Return(Option<IRValue>),
    Jump(String),
    BinBranch {
        cond: IRValue,
        true_label: String,
        false_label: String,
    },
    TritBranch {
        cond: IRValue,
        pos_label: String,
        zero_label: String,
        neg_label: String,
    },
    Unreachable,
}

// ---------------------------------------------------------------------------
// Operator enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum IRBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    IEq,
    INe,
    ILt,
    IGt,
    ILe,
    IGe,
    FEq,
    FNe,
    FLt,
    FGt,
    FLe,
    FGe,
    And,
    Or,
    Xor,
    LShift,
    RShift,
    StrEq,
    StrNe,
}

#[derive(Debug, Clone)]
pub enum IRUnOp {
    Neg,
    Not,
    FNeg,
}

