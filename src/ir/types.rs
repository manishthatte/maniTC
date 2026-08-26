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
    /// R2: the language version this module was lowered under.
    ///
    /// The lowerer sets it and the backends read it. It rides on the module
    /// rather than being threaded separately into codegen because the two
    /// consumers are the lowerer (which picks `DivNear` over `Div`) and
    /// `codegen_llvm` (which adds N5's range checks), and a second plumbing
    /// path for the second consumer is a second thing to forget.
    pub lang: crate::lang::LangVersion,
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
            ManiType::Struct(name, _) => IRType::Struct(name.clone()),
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
    /// C7: the three-way sign of a WORD — `-1`, `0` or `+1`.
    ///
    /// A separate instruction rather than a composition, for two reasons.
    ///
    /// The first is that it is genuinely one instruction on the target. T3ISA
    /// R0 always reads as zero, so `TCMP Rd, Ra, R0` computes the sign
    /// directly. That is the fact the recommendations single out about this
    /// operation: in two's complement sign is a branch or a shift-and-or; in
    /// balanced ternary it is the leading non-zero trit and the machine reads
    /// it in one step.
    ///
    /// The second is that it could NOT have been composed from `TritMin` and
    /// `TritMax`. Those are trit-width — the LLVM backend types both `i8` —
    /// so `TritMax(TritMin(x, 1), -1)` truncates its operand to 8 bits before
    /// clamping it, and `sign(256)` would be `0` rather than `+1`. This
    /// instruction is word-width on both backends by construction.
    TritSign {
        dst: IRTemp,
        a: IRValue,
    },
    /// C2 / T3ISA v1.5: a lane-wise ternary operation on a whole word.
    ///
    /// Distinct from `TritMin`/`TritMax`, which are NUMERIC min/max on the
    /// word as a magnitude. This treats the same word as 27 independent trit
    /// lanes — the shape a 27-trit register actually has when it holds data
    /// rather than a number. On T3 it is one instruction; on LLVM it is a
    /// runtime call, because a 27-lane balanced-ternary loop is not something
    /// binary hardware expresses inline.
    TritLane {
        dst: IRTemp,
        op: IRLaneOp,
        a: IRValue,
        b: IRValue,
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

/// C2: which lane-wise operation `IRInstr::TritLane` performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IRLaneOp {
    /// Lane-wise min — the lane-wise Lukasiewicz conjunction.
    And,
    /// Lane-wise max — the lane-wise disjunction.
    Or,
    /// Lane-wise balanced sum mod 3. Not an involution: three applications,
    /// not two, recover the original.
    Xor,
    /// Lane-wise Lukasiewicz implication, `min(+1, 1 - a + b)` per lane.
    Imp,
    /// Lane-wise three-way compare, `sign(a_i - b_i)` per lane.
    Cmp,
    /// Count the lanes of `a` equal to the trit `b`. The only member whose
    /// result is a COUNT rather than a word, which is why it is here rather
    /// than in a separate instruction: it shares every operand rule.
    Popcount,
}

#[derive(Debug, Clone)]
pub enum IRBinOp {
    Add,
    Sub,
    Mul,
    /// N5: `int` addition that must stay inside the 27-trit word.
    ///
    /// The three `*T27` variants exist because `int`, `t27` and `trint` all
    /// lower to `IRType::I64` and only the first two are 27 trits wide. The
    /// distinction has to survive to the backends somehow, and an extra
    /// operation is the cheapest carrier: adding an `IRType::T27` would touch
    /// every match on `IRType` in both backends, and a flag on
    /// `IRInstr::BinOp` would touch every construction site.
    ///
    /// On T3 these ARE `Add`/`Sub`/`Mul` — the machine's word is 27 trits and
    /// `checked27` already traps — so they cost nothing there. On LLVM each
    /// gets a guard call. Emitted only under `--lang v2`, and only for `int`
    /// and `t27`; `trint` keeps the unchecked machine word, which is the point
    /// of having it.
    AddT27,
    /// N5: `int` subtraction that must stay inside the 27-trit word.
    SubT27,
    /// N5: `int` multiplication that must stay inside the 27-trit word.
    MulT27,
    /// Truncating division — C's `/`, and what the surface `/` lowers to
    /// under V1.
    ///
    /// C4 does NOT change what this means. It stays truncating because the
    /// compiler's own lowerings use it — `lower_timp` and its neighbours
    /// divide by powers of three to reach a lane, and a lane index that
    /// rounded would be the wrong lane. Under V2 the surface operator lowers
    /// to `DivNear` instead and this one is reached only from internal
    /// lowerings and from `math::div_trunc`.
    Div,
    /// Truncating remainder — the partner of `Div`, unchanged by C4.
    Rem,
    /// C4: division rounded to nearest, ties away from zero.
    ///
    /// Emitted for the surface `/` on an integer type under V2. One
    /// instruction on T3 (`TDIVN`, T3ISA v1.6); a short branchless sequence on
    /// LLVM. The rule itself is `lang::div_nearest`, which is also what the
    /// constant folder and the emulator use, so there is one definition of it
    /// rather than three.
    DivNear,
    /// C4: the balanced remainder, `a - DivNear(a, b) * b`.
    ///
    /// Moves with `DivNear` and not separately: the identity
    /// `(a / b) * b + (a % b) == a` holds in both modes only because the two
    /// operators change together.
    RemNear,
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
    /// F-2: multiply by 3^k — one instruction on a balanced-ternary machine.
    ///
    /// The rhs is the SHIFT AMOUNT k, not the multiplier. T3ISA has `TSHI` for
    /// exactly this and the compiler could not reach it: the IR's `LShift`
    /// maps to `BSHL`, the BINARY shift, and `TSHI`/`TSHR` were emitted only
    /// from an explicit `trit::` intrinsic. Measured before adding these: 118
    /// of 1,708 multiplies across the 17 examples are by a power of three, and
    /// `ternary_sort` emitted 33 `TMUL` and zero `TSHI`.
    ///
    /// UNCHECKED, matching plain `Mul`: `TSHI` traps on 27-trit overflow via
    /// `checked27` exactly as `TMUL` does, and on LLVM both are a wrapping
    /// `mul i64`. `MulT27` is deliberately NOT reduced to this — on LLVM it
    /// emits N5's overflow guard, and dropping that would be a silent
    /// behaviour change under `--lang v2`.
    TShl,
    /// F-2: multiply by 3^k, CHECKED — the N5 partner of `TShl`.
    ///
    /// Same instruction as `TShl` on T3, where `TSHI` already traps on 27-trit
    /// overflow via `checked27`. The pair exists for the OTHER backend: on
    /// LLVM `TShl` is a wrapping `mul i64` and this one carries N5's overflow
    /// guard, exactly as `Mul` and `MulT27` do. Reducing `MulT27` to the
    /// unchecked shift would have silently dropped the check that `--lang v2`
    /// exists to provide, which is why the reduction refused it until this
    /// variant existed.
    TShlT27,
    /// F-2: divide by 3^k, ROUNDING TO NEAREST.
    ///
    /// The rhs is the shift amount k. Dropping k low trits of a balanced
    /// ternary number IS round-to-nearest division by 3^k — ties are
    /// impossible because 3^k is odd — which is why this pairs with `DivNear`
    /// and NOT with `Div`. `Div` truncates, so reducing it to a `TSHR` would
    /// change the answer for every negative operand that does not divide
    /// exactly.
    TShr,
}

#[derive(Debug, Clone)]
pub enum IRUnOp {
    Neg,
    Not,
    FNeg,
}

