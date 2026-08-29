// codegen_llvm/helpers.rs — free helper functions for LLVM IR generation.
// Used by LLVMEmitter (mod.rs). Not part of the emitter struct.

use crate::ir::*;
use std::collections::HashMap;

pub(crate) fn llvm_type(ty: &IRType) -> String {
    match ty {
        IRType::I64 => "i64".to_string(),
        IRType::F64 => "double".to_string(),
        IRType::I8 => "i8".to_string(),
        IRType::I16 => "i16".to_string(),
        IRType::I32 => "i32".to_string(),
        IRType::Bool => "i1".to_string(),
        IRType::Trit => "i8".to_string(), // stored as -1, 0, +1
        // P48: a `char` is an unsigned byte 0..=255, CARRIED IN A MACHINE
        // WORD. The byte-ness lives in the cast (`int as char` masks); the
        // width does not, and that is deliberate. Spelled `i8` it needed a
        // correct sign choice at every widening and every comparison, and
        // those are selected from a STRING shadow of the type system in
        // which `char` and `trit` are both "i8" — the same trap this file
        // already records for `i8 signext` (see `strip_abi_attrs`). As a
        // machine word there is nothing to extend, so no site can get the
        // extension wrong. It is also what T3 has always done: the emulator
        // puts the byte in a 64-bit register.
        IRType::Char => "i64".to_string(),
        IRType::Void => "void".to_string(),
        IRType::Ptr(_inner) => "ptr".to_string(), // opaque pointer, LLVM 15+
        IRType::Array(elem, n) => format!("[{} x {}]", n, llvm_type(elem)),
        IRType::Struct(name) => format!("%struct.{}", name),
    }
}

/// The type a value of `ty` has when passed or returned by value in our
/// ABI: aggregates (structs and fixed-size arrays) travel as pointers.
pub(crate) fn llvm_abi_type(ty: &IRType) -> String {
    let s = llvm_type(ty);
    if s.starts_with("%struct.") || s.starts_with('[') {
        "ptr".to_string()
    } else {
        s
    }
}

/// Natural alignment in bytes (as a decimal string literal).
pub(crate) fn llvm_align(ty: &IRType) -> &'static str {
    match ty {
        IRType::I64 | IRType::F64 => "8",
        IRType::I32 => "4",
        IRType::I16 => "2",
        IRType::I8 | IRType::Trit => "1",
        IRType::Char => "8", // a machine word — see `llvm_type`
        IRType::Bool => "1",
        IRType::Ptr(_) => "8",
        _ => "8",
    }
}

/// Zero initialiser string for a type (used when no explicit init is given).
pub(crate) fn llvm_zero_init(ty: &IRType) -> String {
    match ty {
        IRType::F64 => "0.0".to_string(),
        IRType::Bool => "false".to_string(),
        IRType::Ptr(_) => "null".to_string(),
        IRType::Array(_, _) | IRType::Struct(_) => "zeroinitializer".to_string(),
        IRType::Void => "zeroinitializer".to_string(),
        _ => "0".to_string(),
    }
}

// ---------------------------------------------------------------------------
// IRConst → LLVM operand string
// ---------------------------------------------------------------------------

pub(crate) fn irconst_to_string(c: &IRConst, hint_ty: &IRType) -> String {
    match c {
        IRConst::Int(n) => match hint_ty {
            // An int constant used in a double context must be spelled as a
            // float constant (hex form preserves the exact bit pattern).
            IRType::F64 => format!("0x{:016X}", (*n as f64).to_bits()),
            _ => format!("{}", n),
        },
        IRConst::Float(f) => {
            // Emit as LLVM hex double (0x...) to preserve exact bit pattern.
            format!("0x{:016X}", f.to_bits())
        }
        IRConst::Bool(b) => match hint_ty {
            // `true`/`false` are only valid spellings for i1 — in a wider
            // integer context the constant must be numeric.
            IRType::Bool => if *b { "true".to_string() } else { "false".to_string() },
            IRType::F64 => if *b { "1.0".to_string() } else { "0.0".to_string() },
            _ => if *b { "1".to_string() } else { "0".to_string() },
        },
        IRConst::Trit(t) => format!("{}", t),
        IRConst::Str(label) => {
            // The operand for a string literal is the pointer to the global.
            format!("@{}", label.trim_start_matches('@'))
        }
        IRConst::Null => match hint_ty {
            IRType::Ptr(_) => "null".to_string(),
            IRType::F64 => "0.0".to_string(),
            IRType::Bool => "false".to_string(),
            _ => "0".to_string(),
        },
    }
}

/// Convert an IRValue to an LLVM operand string without an emitter context
/// (used only for global-variable initialisers).
pub(crate) fn irvalue_to_operand(val: &IRValue, ty: &IRType) -> String {
    match val {
        IRValue::Temp(t) => format!("%{}", t.0),
        IRValue::Const(c) => irconst_to_string(c, ty),
        IRValue::Global(name) => format!("@{}", mangle_func_name(name)),
        IRValue::Void => llvm_zero_init(ty),
    }
}

// ---------------------------------------------------------------------------
// Integer width helper (for coercion decisions)
// ---------------------------------------------------------------------------

pub(crate) fn int_width(ty_str: &str) -> u32 {
    match ty_str {
        "i1" => 1,
        "i8" => 8,
        "i16" => 16,
        "i32" => 32,
        "i64" => 64,
        _ => 64,
    }
}

/// The widening opcode for an integer coercion written in LLVM type STRINGS.
///
/// `i1` must ZERO-extend: it holds a logical 0/1, and `sext i1 true to i64` is
/// **-1**. Every wider integer type carries a signed value — a `trit` is an i8
/// holding -1/0/+1, and -1 must stay -1 — so those sign-extend.
///
/// This exists because that one rule was written out at three of the seven
/// widening sites in this backend and forgotten at the other four, and the
/// omission is close to invisible: `sext i1 false` is 0, which is the right
/// answer, so only `true` is wrong and only in the paths that skip
/// [`pick_cast_op`]. It surfaced on 23 August 2026 as
/// `io::println_int(5 > 0)` printing **-1** on LLVM and **1** on T3.
///
/// The typed path ([`pick_cast_op`]) has always had this right. Anything that
/// works in LLVM type strings instead of `IRType` must call this rather than
/// reason it out again.
pub(crate) fn widen_op(actual_ty: &str) -> &'static str {
    if actual_ty == "i1" { "zext" } else { "sext" }
}

// ---------------------------------------------------------------------------
// Type-inference helpers
// ---------------------------------------------------------------------------

/// Best-effort type of a value (used where the explicit type is unavailable,
/// e.g. call argument lists or return-value terminators).
pub(crate) fn guess_value_type(val: &IRValue) -> IRType {
    match val {
        IRValue::Const(IRConst::Int(_)) => IRType::I64,
        IRValue::Const(IRConst::Float(_)) => IRType::F64,
        IRValue::Const(IRConst::Bool(_)) => IRType::Bool,
        IRValue::Const(IRConst::Trit(_)) => IRType::I8,
        IRValue::Const(IRConst::Str(_)) => IRType::Ptr(Box::new(IRType::I8)),
        IRValue::Const(IRConst::Null) => IRType::Ptr(Box::new(IRType::I8)),
        IRValue::Global(_) => IRType::Ptr(Box::new(IRType::I8)),
        IRValue::Temp(_) => IRType::I64, // conservative fallback
        IRValue::Void => IRType::Void,
    }
}

/// For comparison BinOps the *result* type is Bool (i1) but the *operand*
/// type must be the original numeric type.  This function recovers that type.
pub(crate) fn operand_type_for_cmp(lhs: &IRValue, rhs: &IRValue, declared_ty: &IRType) -> IRType {
    // If the declared result is already non-Bool, use it directly.
    if !matches!(declared_ty, IRType::Bool) {
        return declared_ty.clone();
    }
    // Try to recover from lhs / rhs constants.
    let lhs_ty = guess_value_type(lhs);
    if !matches!(lhs_ty, IRType::Bool) {
        return lhs_ty;
    }
    let rhs_ty = guess_value_type(rhs);
    if !matches!(rhs_ty, IRType::Bool) {
        return rhs_ty;
    }
    // Both operands look like booleans — comparison is between i1 values.
    IRType::Bool
}

// ---------------------------------------------------------------------------
// Cast instruction selection
// ---------------------------------------------------------------------------

pub(crate) fn pick_cast_op(from: &IRType, to: &IRType) -> &'static str {
    match (from, to) {
        // Sign-extending widening
        (IRType::I8, IRType::I16)
        | (IRType::I8, IRType::I32)
        | (IRType::I8, IRType::I64)
        | (IRType::I16, IRType::I32)
        | (IRType::I16, IRType::I64)
        | (IRType::I32, IRType::I64)
        | (IRType::Trit, IRType::I16)
        | (IRType::Trit, IRType::I32)
        | (IRType::Trit, IRType::I64) => "sext",

        // Char → a NARROWER integer truncates; Char → I64 is the identity and
        // never reaches here (`cast_sequence` takes the same-width branch).
        // A `char` already holds 0..=255, so no extension is involved either
        // way — that is the point of carrying it in a machine word (P48).
        (IRType::Char, IRType::I16) | (IRType::Char, IRType::I32) => "trunc",

        // Bool (i1) → integer: zero-extend (true=1, false=0).
        // Bool → Trit is also zext: true → +1, false → 0.
        (IRType::Bool, IRType::I8)
        | (IRType::Bool, IRType::I16)
        | (IRType::Bool, IRType::I32)
        | (IRType::Bool, IRType::I64)
        | (IRType::Bool, IRType::Char)
        | (IRType::Bool, IRType::Trit) => "zext",

        // Truncation / narrowing
        (IRType::I64, IRType::I32)
        | (IRType::I64, IRType::I16)
        | (IRType::I64, IRType::I8)
        | (IRType::I32, IRType::I16)
        | (IRType::I32, IRType::I8)
        | (IRType::I16, IRType::I8) => "trunc",

        // Integer → float (Trit is i8 holding -1/0/+1 — signed)
        (IRType::I64, IRType::F64)
        | (IRType::I32, IRType::F64)
        | (IRType::I16, IRType::F64)
        | (IRType::I8, IRType::F64)
        | (IRType::Trit, IRType::F64) => "sitofp",

        // Bool → float: false → 0.0, true → 1.0
        // Char → float: a char is 0..=255, so signed and unsigned agree; this
        // is `uitofp` to say which one is meant rather than to change a value.
        (IRType::Bool, IRType::F64) | (IRType::Char, IRType::F64) => "uitofp",

        // Float → integer
        (IRType::F64, IRType::I64)
        | (IRType::F64, IRType::I32)
        | (IRType::F64, IRType::I16)
        | (IRType::F64, IRType::I8) => "fptosi",

        // Pointer ↔ integer (both ops accept any integer width)
        (IRType::Ptr(_), IRType::I64 | IRType::I32 | IRType::I16 | IRType::I8 | IRType::Char) => {
            "ptrtoint"
        }
        (
            IRType::I64 | IRType::I32 | IRType::I16 | IRType::I8 | IRType::Char | IRType::Trit,
            IRType::Ptr(_),
        ) => "inttoptr",

        // Pointer ↔ pointer (same under opaque ptrs, but bitcast is still valid)
        (IRType::Ptr(_), IRType::Ptr(_)) => "bitcast",

        // Default: bitcast. Only correct for same-size reinterpretations;
        // every size-changing pair MUST have an arm above or a multi-
        // instruction lowering in cast_sequence (int→Bool, int→Trit, …).
        _ => "bitcast",
    }
}

/// Full text of the instruction sequence for `%dst = cast %src : from → to`.
/// Multi-line sequences use the emit_block continuation indent ("\n  ").
///
/// This covers the conversions a single LLVM cast opcode cannot express
/// legally or per the language semantics (docs/language-reference.md):
///   * int/trit → bool     — i8→i1 bitcast is illegal; the language meaning
///                           is "nonzero is true", so emit `icmp ne .., 0`.
///   * float → bool        — `fcmp one .., 0.0` (0.0 and NaN are false).
///   * int/float → trit    — `as trit` clamps to {-1, 0, +1} (docs §expr,
///                           "Type cast"), so emit compare + two selects.
///   * everything else     — a single opcode from pick_cast_op.
/// `float as <integer>`, SATURATING, via `llvm.fptosi.sat`.
///
/// Plain `fptosi` is UNDEFINED BEHAVIOUR when the value does not fit — NaN and
/// both infinities included — and on x86 it yields the "integer indefinite"
/// value, `i64::MIN`, for all of them. T3's `ftoi` is Rust's `as`, which
/// saturates: NaN to 0, and out-of-range to the nearest bound. So the two
/// backends disagreed on every out-of-range conversion, and the LLVM answer
/// was not merely different but undefined.
///
/// It was not a theoretical divergence. `exp(nan)` in the math census reaches
/// `n = (q - 0.5) as int`, which became `i64::MIN` on LLVM, and the scaling
/// loop that follows — `while e < 0 { res = res * 0.5; e = e + 1; }` — then
/// had 9.2e18 iterations to run. Three of the corpus's five LLVM hangs were
/// this one cast (report.txt P23).
///
/// `llvm.fptosi.sat` has exactly Rust's semantics, so choosing it makes the
/// backends agree by construction rather than by a second clamp.
fn fptosi_sat(dst: &str, src: &str, to_s: &str) -> String {
    format!(
        "%{} = call {} @llvm.fptosi.sat.{}.f64(double {})",
        dst, to_s, to_s, src
    )
}

/// `<integer> as char`, CLAMPED to 0..=255.
///
/// P48. A `char` is an unsigned byte carried in a machine word, so the
/// byte-ness has to be imposed HERE — it is the one place the width is not
/// implied by the storage. Without it `300 as char` stayed 300 on T3, which
/// never narrowed at all, while LLVM — where a char was an `i8` — truncated to
/// 44.
///
/// **CLAMP AND NOT WRAP, AND THE LANGUAGE CHOSE THAT BEFORE THIS CAST EXISTED.**
/// The two boundary behaviours the reference documents both clamp: `i as trit`
/// "clamps to {-1, 0, +1}", and `float as int` SATURATES (P23 picked
/// `llvm.fptosi.sat` precisely so the backends would agree by construction).
/// Truncating to 44 would be C's answer, not this language's, and a reader
/// predicting from the two documented cases would predict 255. Matching LLVM's
/// old i8 truncation would have been matching an accident of the storage.
fn clamp_to_byte(dst: &str, src: &str, from_s: &str) -> String {
    let w = if from_s == "i64" {
        src.to_string()
    } else {
        // Widen first so the clamp is applied at machine width.
        return format!(
            "%{d}__cw = sext {ft} {s} to i64\n  {rest}",
            d = dst,
            ft = from_s,
            s = src,
            rest = clamp_to_byte(dst, &format!("%{}__cw", dst), "i64")
        );
    };
    format!(
        "%{dst}__lo = icmp slt i64 {w}, 0\n  \
         %{dst}__c1 = select i1 %{dst}__lo, i64 0, i64 {w}\n  \
         %{dst}__hi = icmp sgt i64 %{dst}__c1, 255\n  \
         %{dst} = select i1 %{dst}__hi, i64 255, i64 %{dst}__c1",
        dst = dst,
        w = w
    )
}

pub(crate) fn cast_sequence(dst: &str, src: &str, from: &IRType, to: &IRType) -> String {
    let from_s = llvm_type(from);
    let to_s = llvm_type(to);

    // `<integer> as char` clamps to a byte. BEFORE the same-width branch below,
    // which would otherwise make `int as char` the identity now that a char is
    // a machine word (P48).
    // `Bool` is excluded because it is ALREADY 0 or 1 and because clamping it
    // would widen it with `sext`, and `sext i1 true` is -1 — which then clamps
    // to 0, so `true as char` came out 0. That is the exact rule `widen_op`
    // documents ten lines up, reproduced by a new caller within the hour.
    if matches!(to, IRType::Char)
        && !matches!(from, IRType::Char | IRType::F64 | IRType::Ptr(_) | IRType::Bool)
    {
        return clamp_to_byte(dst, src, &from_s);
    }

    // Identity at the LLVM level (e.g. Trit → I8 for `trit as bool3`, both
    // i8 with the same {-1,0,+1} encoding — but int-like i8 → Trit clamps).
    if from_s == to_s {
        if matches!(from, IRType::I8 | IRType::I16 | IRType::I32 | IRType::I64 | IRType::Char)
            && matches!(to, IRType::Trit)
        {
            return clamp_to_trit(dst, src, &from_s);
        }
        return match to_s.as_str() {
            "double" => format!("%{} = fadd double {}, 0.0", dst, src),
            "ptr" => format!("%{} = bitcast ptr {} to ptr", dst, src),
            _ => format!("%{} = add {} {}, 0", dst, to_s, src),
        };
    }

    match (from, to) {
        // Anything integer-like → Bool: nonzero is true.
        (
            IRType::I8 | IRType::I16 | IRType::I32 | IRType::I64 | IRType::Char | IRType::Trit,
            IRType::Bool,
        ) => format!("%{} = icmp ne {} {}, 0", dst, from_s, src),
        // Float → Bool: 0.0 (and NaN) → false.
        (IRType::F64, IRType::Bool) => {
            format!("%{} = fcmp one double {}, 0.0", dst, src)
        }
        // Float → integer: saturating, never raw `fptosi`. See `fptosi_sat`.
        (IRType::F64, IRType::I64 | IRType::I32 | IRType::I16 | IRType::I8) => {
            fptosi_sat(dst, src, &to_s)
        }
        // Float → Char: saturate to a machine word the way every other
        // float→integer cast does (P23), then impose the byte (P48).
        (IRType::F64, IRType::Char) => {
            let itmp = format!("{}__ftoi", dst);
            format!(
                "{}\n  {}",
                fptosi_sat(&itmp, src, "i64"),
                clamp_to_byte(dst, &format!("%{}", itmp), "i64")
            )
        }
        // Integer → Trit: clamp to {-1, 0, +1} per the language reference.
        (
            IRType::I8 | IRType::I16 | IRType::I32 | IRType::I64 | IRType::Char,
            IRType::Trit,
        ) => clamp_to_trit(dst, src, &from_s),
        // Float → Trit: truncate to int, then clamp.
        (IRType::F64, IRType::Trit) => {
            let itmp = format!("{}__ftoi", dst);
            format!(
                "{}\n  {}",
                fptosi_sat(&itmp, src, "i64"),
                clamp_to_trit(dst, &format!("%{}", itmp), "i64")
            )
        }
        // Ptr ↔ Bool (rare; keep it legal): nonnull test / select.
        (IRType::Ptr(_), IRType::Bool) => {
            format!("%{} = icmp ne ptr {}, null", dst, src)
        }
        (IRType::Bool, IRType::Ptr(_)) => {
            let itmp = format!("%{}__zext", dst);
            format!(
                "{} = zext i1 {} to i64\n  %{} = inttoptr i64 {} to ptr",
                itmp, src, dst, itmp
            )
        }
        // Ptr ↔ trit / float (nonsense conversions, but must stay legal):
        // go through i64.
        (IRType::Ptr(_), IRType::Trit) => {
            let itmp = format!("%{}__p2i", dst);
            format!(
                "{} = ptrtoint ptr {} to i64\n  {}",
                itmp,
                src,
                clamp_to_trit(dst, &itmp, "i64")
            )
        }
        (IRType::Ptr(_), IRType::F64) => {
            let itmp = format!("%{}__p2i", dst);
            format!(
                "{} = ptrtoint ptr {} to i64\n  %{} = sitofp i64 {} to double",
                itmp, src, dst, itmp
            )
        }
        (IRType::F64, IRType::Ptr(_)) => {
            let itmp = format!("{}__f2i", dst);
            format!(
                "{}\n  %{} = inttoptr i64 %{} to ptr",
                fptosi_sat(&itmp, src, "i64"),
                dst,
                itmp
            )
        }
        // Single-opcode conversions.
        _ => format!(
            "%{} = {} {} {} to {}",
            dst,
            pick_cast_op(from, to),
            from_s,
            src,
            to_s
        ),
    }
}

/// Clamp an integer value of type `src_ty` into a trit (i8 in {-1, 0, +1}):
///   pos → +1, neg → -1, zero → 0.
fn clamp_to_trit(dst: &str, src: &str, src_ty: &str) -> String {
    let isp = format!("%{}__isp", dst);
    let isn = format!("%{}__isn", dst);
    let neg = format!("%{}__neg", dst);
    format!(
        "{isp} = icmp sgt {ty} {src}, 0\n  \
         {isn} = icmp slt {ty} {src}, 0\n  \
         {neg} = select i1 {isn}, i8 -1, i8 0\n  \
         %{dst} = select i1 {isp}, i8 1, i8 {neg}",
        isp = isp,
        isn = isn,
        neg = neg,
        dst = dst,
        ty = src_ty,
        src = src
    )
}

// ---------------------------------------------------------------------------
// Function name mangling
// ---------------------------------------------------------------------------

/// Mangle a maniT function name to a valid LLVM identifier.
/// Replaces `::`, `<`, `>`, `,` and spaces with `_`.
///
/// The user's `main` is renamed to `__manit_main`: the C ABI requires
/// `main` to return i32, but a maniT main is void- or i64-returning, which
/// left the process exit status as whatever happened to be in the return
/// register. emit_module emits a `define i32 @main()` wrapper that calls
/// `__manit_main` and returns a proper status.
pub(crate) fn mangle_func_name(name: &str) -> String {
    if name == "main" {
        return "__manit_main".to_string();
    }
    name.replace("::", "_")
        .replace('<', "_")
        .replace('>', "_")
        .replace(',', "_")
        .replace(' ', "_")
}

// ---------------------------------------------------------------------------
// Declare-line parser for building the function signature map
// ---------------------------------------------------------------------------

/// ABI/parameter attributes that may appear beside a type in a `declare` line.
///
/// These belong to the declaration, not to the type. `i8 signext` is an `i8`
/// that the ABI says is sign-extended by the caller; leaving the word attached
/// makes downstream type logic treat `"i8 signext"` as a distinct type and emit
/// nonsense like `sext i8 %t to i8 signext`.
const LLVM_PARAM_ATTRS: &[&str] = &[
    "signext", "zeroext", "noundef", "inreg", "nonnull", "noalias", "nocapture",
    "readonly", "writeonly", "immarg", "returned", "nofree", "dead_on_unwind",
];

/// Strip ABI attributes from one `declare` type slot, leaving the bare type.
///
/// `"signext i8"` -> `"i8"`, `"i8 signext"` -> `"i8"`, `"ptr"` -> `"ptr"`,
/// `"..."` -> `"..."` (the vararg marker must survive).
pub(crate) fn strip_llvm_attrs(slot: &str) -> String {
    let kept: Vec<&str> = slot
        .split_whitespace()
        .filter(|w| !LLVM_PARAM_ATTRS.contains(w))
        .collect();
    kept.join(" ")
}

/// Parse LLVM `declare` lines to extract function signatures.
/// Returns a map from function name to (param_types, return_type).
/// A trailing `"..."` entry in param_types marks a vararg function — Call
/// emission needs it to print the full callee type
/// (`call ptr (ptr, ...) @fmt_format(...)`), which the x86-64 varargs ABI
/// requires (the AL register carries the vector-register count).
///
/// Types come back **without** ABI attributes. The attributes stay in the
/// emitted `declare` line, where they must match the C runtime's ABI exactly;
/// LLVM's `CallBase::paramHasAttr` falls back to the callee's attribute list,
/// so a direct call needs no attribute of its own to be lowered correctly.
pub(crate) fn parse_declare_sigs(decl_text: &str) -> HashMap<String, (Vec<String>, String)> {
    let mut sigs = HashMap::new();
    for line in decl_text.lines() {
        let line = line.trim();
        if !line.starts_with("declare ") {
            continue;
        }
        // Format: "declare <ret_type> @<name>(<params>)"
        let rest = &line[8..]; // skip "declare "
        // Find @name
        let at_pos = match rest.find('@') {
            Some(p) => p,
            None => continue,
        };
        let ret_str = strip_llvm_attrs(rest[..at_pos].trim());
        let after_at = &rest[at_pos + 1..];
        // Find opening paren
        let paren_pos = match after_at.find('(') {
            Some(p) => p,
            None => continue,
        };
        let name = after_at[..paren_pos].to_string();
        // Extract params between ( and )
        let close_paren = match after_at.find(')') {
            Some(p) => p,
            None => continue,
        };
        let params_str = &after_at[paren_pos + 1..close_paren];
        let params: Vec<String> = if params_str.trim().is_empty() {
            vec![]
        } else {
            params_str
                .split(',')
                .map(|p| strip_llvm_attrs(p.trim()))
                .collect()
        };
        sigs.insert(name, (params, ret_str));
    }
    sigs
}

// ---------------------------------------------------------------------------
// stdlib declare statements (maniT runtime)
// ---------------------------------------------------------------------------

pub(crate) const STDLIB_DECLARES: &str = "\
; ---- runtime fault guards (A7 / A2) ----
declare void @manit_fault(ptr)
declare void @manit_check_divisor(i64)
declare void @manit_check_index(i64, i64)
declare void @manit_check_result_ok(i64)
; ---- N5 (--lang v2): the 27-trit word guards ----
; Called on the OPERANDS, before the arithmetic, so the true result can be
; tested in __int128 rather than after it has wrapped in i64.
declare void @manit_check_t27_add(i64, i64)
declare void @manit_check_t27_sub(i64, i64)
declare void @manit_check_t27_mul(i64, i64)
; ---- T3ISA v1.5 lane-wise ternary logic (C2) ----
; One T3 instruction each; on a binary machine, a 27-iteration loop over
; balanced-ternary digits, so these are runtime calls rather than inline IR.
declare i64 @manit_lane_and(i64, i64)
declare i64 @manit_lane_or(i64, i64)
declare i64 @manit_lane_xor(i64, i64)
declare i64 @manit_lane_imp(i64, i64)
declare i64 @manit_lane_cmp(i64, i64)
declare i64 @manit_lane_popcount(i64, i64)
; ---- io ----
declare void @io_println(ptr)
declare void @io_print(ptr)
declare void @io_newline()
declare void @io_print_int(i64)
declare void @io_println_int(i64)
declare void @io_print_float(double)
declare void @io_println_float(double)
declare void @io_print_char(i64)
declare void @io_print_trit(i8 signext)
declare void @io_print_bool3(i8 signext)
declare void @io_print_tryte(i8 signext)
declare ptr @io_read_line()
declare i64 @io_read_int()

; ---- fmt ----
declare ptr @fmt_format(ptr, ...)
declare ptr @fmt_concat(ptr, ptr)
declare ptr @fmt_int_to_str(i64)
declare ptr @fmt_show_int(i64)
declare ptr @fmt_show_float(double)
declare ptr @fmt_show_bool(i1)
declare ptr @fmt_pad_zeros(ptr, i64)
; show_trit, show_bool3, align_left and align_right were declared here until
; 20 August 2026. They are ManiT source now (stdlib/fmt.mt) and their C bodies
; are deleted, so a declare would name a symbol that no longer exists. The
; emitter also skips any declare the module defines — see codegen_llvm/mod.rs —
; which is what makes moving the next one a one-file change.
declare ptr @fmt_to_upper(ptr)
declare ptr @fmt_to_lower(ptr)

; ---- math ----
; math_abs, math_min, math_max, math_clamp and math_pow were declared here
; until 20 August 2026 — ManiT source in stdlib/math.mt now, C bodies deleted.
; math_pow's declare was `double(double, double)` against a ManiT declaration
; of `pow(int, int) -> int`, so that call could never have worked.
declare double @math_abs_float(double)
declare i64 @math_trit_count(i64)

; ---- str ----
declare i64 @str_len(ptr)
declare i64 @str_char_at(ptr, i64)
declare i64 @str_char_count(ptr)
declare ptr @str_concat(ptr, ptr)
declare i1 @str_contains(ptr, ptr)
declare i64 @str_find(ptr, ptr)
declare ptr @str_replace(ptr, ptr, ptr)
declare ptr @str_split(ptr, ptr)
declare ptr @str_trim(ptr)
declare ptr @str_from_char(i64)
; str_to_upper / str_to_lower are gone: they are ManiT source in stdlib/str.mt
; as of 19 Aug 2026, and the merged ManiT bodies mangle to those exact symbols,
; so declaring them here would clash at link time.
declare i1 @str_eq(ptr, ptr)

; ---- Vec<T> ----
declare ptr @Vec_new()
declare void @Vec_push(ptr, i64)
declare i64 @Vec_pop(ptr)
declare i64 @Vec_get(ptr, i64)
declare void @Vec_set(ptr, i64, i64)
declare i64 @Vec_len(ptr)
declare i1 @Vec_is_empty(ptr)
declare void @Vec_clear(ptr)
declare i1 @Vec_contains(ptr, i64)
declare i64 @Vec_remove(ptr, i64)
declare void @Vec_sort(ptr)
declare void @Vec_reverse(ptr)
declare i64 @Vec_index_of(ptr, i64)
declare void @Vec_for_each(ptr, ptr)
declare ptr @Vec_map(ptr, ptr)
declare ptr @Vec_filter(ptr, ptr)
declare i64 @Vec_fold(ptr, i64, ptr)
declare ptr @Vec_slice(ptr, i64, i64)
; A Vec<str> compares and orders its elements as TEXT — the plain forms above
; compare the type-erased i64, which for a str is its address.
declare i1 @Vec_contains_str(ptr, i64)
declare i64 @Vec_index_of_str(ptr, i64)
declare void @Vec_sort_str(ptr)

; ---- Map<K,V> ----
declare ptr @Map_new()
declare void @Map_insert(ptr, i64, i64)
declare i64 @Map_get(ptr, i64)
declare i64 @Map_get_or(ptr, i64, i64)
declare i1 @Map_contains_key(ptr, i64)
declare void @Map_remove(ptr, i64)
declare i64 @Map_len(ptr)
declare i1 @Map_is_empty(ptr)
declare ptr @Map_keys(ptr)
declare ptr @Map_values(ptr)
; A Map<str,V> keys on the TEXT: these intern the key so that identity becomes
; equality, which is what the T3 emulator has always done.
declare void @Map_insert_str(ptr, i64, i64)
declare i64 @Map_get_str(ptr, i64)
declare i64 @Map_get_or_str(ptr, i64, i64)
declare i1 @Map_contains_key_str(ptr, i64)
declare void @Map_remove_str(ptr, i64)

; ---- Set<T> ----
declare ptr @Set_new()
declare void @Set_insert(ptr, i64)
declare i1 @Set_contains(ptr, i64)
declare void @Set_remove(ptr, i64)
; A Set<str> holds TEXT: these intern the element, after which the set algebra
; above is correct on its own because every stored element is canonical.
declare void @Set_insert_str(ptr, i64)
declare i1 @Set_contains_str(ptr, i64)
declare void @Set_remove_str(ptr, i64)
declare i64 @Set_len(ptr)
declare void @Set_for_each(ptr, ptr)
declare ptr @Set_intersection(ptr, ptr)
declare ptr @Set_union(ptr, ptr)
declare ptr @Set_difference(ptr, ptr)
declare i1 @Set_is_subset(ptr, ptr)
declare i1 @Set_is_superset(ptr, ptr)
declare i1 @Set_is_disjoint(ptr, ptr)

; ---- Deque<T> ----
declare ptr @Deque_new()
declare void @Deque_push_front(ptr, i64)
declare void @Deque_push_back(ptr, i64)
declare i64 @Deque_pop_front(ptr)
declare i64 @Deque_pop_back(ptr)
declare i64 @Deque_front(ptr)
declare i64 @Deque_back(ptr)
declare i64 @Deque_len(ptr)
declare i1 @Deque_is_empty(ptr)
declare i1 @Deque_contains(ptr, i64)

; ---- TernaryTrie ----
declare ptr @TernaryTrie_new()
declare void @TernaryTrie_insert(ptr, ptr, i64)
declare i64 @TernaryTrie_get(ptr, ptr)
declare i1 @TernaryTrie_contains(ptr, ptr)
declare i64 @TernaryTrie_len(ptr)
declare ptr @TernaryTrie_keys_with_prefix(ptr, ptr)

; ---- sync / concurrency ----
declare ptr @Mutex_new(i64)
declare ptr @Mutex_lock(ptr)
declare void @Mutex_unlock(ptr)
declare i64 @Mutex_get(ptr)
declare void @Mutex_set(ptr, i64)
declare ptr @AtomicTrit_new(i8 signext)
declare signext i8 @AtomicTrit_get(ptr)
declare void @AtomicTrit_set(ptr, i8 signext)
declare signext i8 @AtomicTrit_swap(ptr, i8 signext)
declare i1 @AtomicTrit_compare_exchange(ptr, i8 signext, i8 signext)
declare signext i8 @AtomicTrit_fetch_and(ptr, i8 signext)
declare signext i8 @AtomicTrit_fetch_or(ptr, i8 signext)
declare signext i8 @AtomicTrit_fetch_neg(ptr)
declare ptr @AtomicInt_new(i64)
declare i64 @AtomicInt_load(ptr)
declare void @AtomicInt_store(ptr, i64)
declare i64 @AtomicInt_swap(ptr, i64)
declare i64 @AtomicInt_fetch_add(ptr, i64)
declare i64 @AtomicInt_fetch_sub(ptr, i64)
declare i1 @AtomicInt_compare_exchange(ptr, i64, i64)
declare ptr @channel_new()
declare ptr @channel_bounded(i64)
declare void @channel_send(ptr, i64)
declare i64 @channel_recv(ptr)
declare i64 @channel_len(ptr)
declare i1 @channel_is_empty(ptr)
declare void @channel_close(ptr)
declare i1 @channel_is_closed(ptr)
declare void @async_yield_now()
declare ptr @manit_spawn(ptr, i64)
declare i64 @manit_join(ptr)
declare ptr @Semaphore_new(i64)
declare void @Semaphore_acquire(ptr)
declare void @Semaphore_release(ptr)
declare i1 @Semaphore_try_acquire(ptr)
declare i64 @Semaphore_available(ptr)

; ---- ternary utils ----
declare i64 @ternary_trit_to_int(i8 signext)
declare ptr @ternary_t27_to_str(i64)

; ---- fs (filesystem) ----
declare i1 @fs_exists(ptr)
declare i1 @fs_is_file(ptr)
declare i64 @fs_is_dir(ptr)
declare ptr @fs_read_file(ptr)
declare void @fs_write_file(ptr, ptr)
declare void @fs_append_file(ptr, ptr)
declare void @fs_remove_file(ptr)
declare void @fs_copy_file(ptr, ptr)
declare void @fs_rename(ptr, ptr)
declare void @fs_create_dir(ptr)
declare void @fs_create_dir_all(ptr)
declare void @fs_remove_dir(ptr)
declare i64 @fs_file_size(ptr)
declare i64 @fs_list_dir_open(ptr)
declare ptr @fs_list_dir_entry(i64)
declare i64 @fs_copy(ptr, ptr)
declare i64 @fs_move(ptr, ptr)
declare ptr @fs_open(ptr, ptr)
declare ptr @fs_read(ptr, i64)
declare ptr @fs_read_line(ptr)
declare void @fs_write(ptr, ptr)
declare void @fs_close(ptr)
declare void @fs_flush(ptr)
declare void @fs_seek(ptr, i64)
declare i64 @fs_tell(ptr)

; ---- env (environment) ----
declare ptr @env_get_env(ptr)
declare ptr @env_get_env_or(ptr, ptr)
declare void @env_set_env(ptr, ptr)
declare void @env_unset_env(ptr)
declare i1 @env_has_env(ptr)
declare i64 @env_argc()
declare ptr @env_arg(i64)
declare ptr @env_cwd()
declare void @env_set_cwd(ptr)
declare i64 @env_pid()
declare i64 @env_ppid()
declare ptr @env_os_name()
declare ptr @env_arch()
declare i64 @env_cpu_count()
declare ptr @env_home_dir()
declare ptr @env_temp_dir()
declare void @env_exit(i64)
declare void @env_abort(ptr)
declare ptr @env_timestamp()
declare ptr @env_date()
declare ptr @env_time()
declare i64 @process_spawn(ptr)
declare ptr @shell_exec(ptr)

; ---- time ----
declare i64 @time_now_ms()
declare i64 @time_now_nanos()
declare void @time_sleep_ms(i64)
declare i64 @time_unix_secs()
declare ptr @time_format_iso8601(i64)

; ---- path utilities ----
declare ptr @path_join(ptr, ptr)
declare ptr @path_file_name(ptr)
declare ptr @path_extension(ptr)
declare ptr @path_parent(ptr)

; ---- alternate name aliases (IR lowerer may use module::func naming) ----
declare ptr @math_trits_to_str(i64)
declare ptr @int_to_str(i64)
declare ptr @float_to_str(double)
declare i64 @ternary_t27_shift_left(i64, i64)
declare i64 @ternary_t27_shift_right(i64, i64)
declare i64 @ternary_t27_rotate_left(i64, i64)
declare i64 @ternary_t27_rotate_right(i64, i64)
declare ptr @str_slice(ptr, i64, i64)
declare i64 @str_index_of(ptr, ptr)
declare ptr @str_chars(ptr)
; str_join went the same way on 20 August 2026 — declared, never defined, ManiT
; source now.
; str_parse_float, str_from_float, str_from_bool and str_from_trit were declared
; here until 20 August 2026. All four were declarations with no definition — no
; C body ever existed for any of them, on either backend — so every program that
; called one failed at link with an undefined symbol. They are ManiT source in
; stdlib/str.mt now, delegating to fmt::, and a declare would clash with the
; merged body. Their four siblings (from_bool3, from_ternary and the is_*
; predicates) were never even declared here, which is why those failed one step
; earlier, at assembly rather than at link.
declare ptr @str_to_int(ptr)
declare ptr @str_to_float(ptr)
declare void @Channel_send(ptr, i64)
declare i64 @Channel_recv(ptr)
declare i64 @Channel_len(ptr)
declare i1 @Channel_is_empty(ptr)
declare void @Channel_close(ptr)
declare i1 @Channel_is_closed(ptr)
declare ptr @Channel_new()
declare ptr @Channel_bounded(i64)
declare ptr @channel()
declare i1 @TernaryTrie_contains_key(ptr, ptr)
declare void @TernaryTrie_remove(ptr, ptr)
declare ptr @TernaryTrie_keys(ptr)

; ---- terminal control (TUI) ----
declare i64 @terminal_set_raw()
declare i64 @terminal_set_cooked()
declare i64 @terminal_get_rows()
declare i64 @terminal_get_cols()
declare i64 @io_read_char()
declare i64 @io_read_key()
declare i64 @io_clear_screen()
declare i64 @io_move_cursor(i64, i64)
declare i64 @io_set_reverse()
declare i64 @io_reset_attr()
declare i64 @io_set_bold()

; ---- http (libcurl) ----
declare ptr @net_http_get(ptr)

; ---- SDL2 GUI ----
declare i64 @gui_init(i64, i64, ptr)
declare i64 @gui_quit()
declare i64 @gui_clear()
declare i64 @gui_present()
declare i64 @gui_set_color(i64, i64, i64, i64)
declare i64 @gui_fill_rect(i64, i64, i64, i64)
declare i64 @gui_draw_rect(i64, i64, i64, i64)
declare i64 @gui_draw_line(i64, i64, i64, i64)
declare i64 @gui_draw_text(ptr, i64, i64)
declare i64 @gui_draw_text_lg(ptr, i64, i64)
declare i64 @gui_text_width(ptr)
declare i64 @gui_font_height()
declare i64 @gui_window_width()
declare i64 @gui_window_height()
declare i64 @gui_poll_event()
declare i64 @gui_wait_event(i64)
declare i64 @gui_event_type()
declare i64 @gui_event_key()
declare i64 @gui_mouse_x()
declare i64 @gui_mouse_y()
declare i64 @gui_mouse_btn()
declare i64 @gui_event_text_char()
declare i64 @gui_ticks()
declare i64 @gui_delay(i64)
declare i64 @gui_key_return()
declare i64 @gui_key_escape()
declare i64 @gui_key_backspace()
declare i64 @gui_key_delete()
declare i64 @gui_key_up()
declare i64 @gui_key_down()
declare i64 @gui_key_left()
declare i64 @gui_key_right()
declare i64 @gui_key_home()
declare i64 @gui_key_end()
declare i64 @gui_key_pageup()
declare i64 @gui_key_pagedown()
declare i64 @gui_key_f1()
declare i64 @gui_key_f2()
declare i64 @gui_key_f3()
declare i64 @gui_key_f4()
declare i64 @gui_key_f5()
declare i64 @gui_key_f6()
declare i64 @gui_key_f7()
declare i64 @gui_key_f8()
declare i64 @gui_key_f9()
declare i64 @gui_key_f10()
declare i64 @gui_key_f11()
declare i64 @gui_key_f12()
declare i64 @gui_key_tab()
declare i64 @gui_key_space()
declare i64 @gui_key_a()
declare i64 @gui_key_b()
declare i64 @gui_key_c()
declare i64 @gui_key_d()
declare i64 @gui_key_e()
declare i64 @gui_key_f()
declare i64 @gui_key_g()
declare i64 @gui_key_h()
declare i64 @gui_key_i()
declare i64 @gui_key_j()
declare i64 @gui_key_k()
declare i64 @gui_key_l()
declare i64 @gui_key_m()
declare i64 @gui_key_n()
declare i64 @gui_key_o()
declare i64 @gui_key_p()
declare i64 @gui_key_q()
declare i64 @gui_key_r()
declare i64 @gui_key_s()
declare i64 @gui_key_t()
declare i64 @gui_key_u()
declare i64 @gui_key_v()
declare i64 @gui_key_w()
declare i64 @gui_key_x()
declare i64 @gui_key_y()
declare i64 @gui_key_z()
declare i64 @gui_key_mod_ctrl()
declare i64 @gui_key_mod_shift()
declare i64 @gui_key_mod_alt()
declare ptr @gui_event_text_str()
declare i64 @gui_wheel_dy()
declare i64 @fs_mkdir(ptr)
declare ptr @gui_clipboard_get()
declare void @gui_clipboard_set(ptr)
declare i64 @fs_delete(ptr)

";

// ---------------------------------------------------------------------------
// Internal runtime helpers (functions the T3 backend handles with dedicated
// instructions/syscalls but the C runtime does not export). They are emitted
// as `define internal` so they can never clash with a future C runtime
// symbol of the same name. The signature text below is parsed into fn_sigs
// (for call-site type coercion) but NOT printed as `declare` lines — a module
// must not both declare and define the same symbol.
// ---------------------------------------------------------------------------

pub(crate) const INTERNAL_HELPER_SIGS: &str = "\
declare void @io_println_bool(i1)
declare void @io_println_bool3(i8)
declare void @io_println_trit(i8)
declare void @io_println_ternary(i64)
declare i64 @ternary_int_to_t27(i64)
declare i64 @ternary_t27_to_int(i64)
declare i64 @ternary_int_to_t9(i64)
declare i64 @ternary_t9_to_int(i64)
declare i64 @ternary_int_to_tryte(i64)
declare i64 @ternary_tryte_to_int(i64)
declare i64 @ternary_t27_neg(i64)
declare i64 @ternary_t27_and(i64, i64)
declare i64 @ternary_t27_or(i64, i64)
declare i64 @ternary_trit_shift_left(i64, i64)
declare i64 @ternary_trit_shift_right(i64, i64)
declare i64 @ternary_tryte_from_trits(i64, i64, i64)
declare i64 @ternary_pack_trits(ptr)
declare ptr @ternary_trits_to_str(ptr)
declare ptr @ternary_to_balanced_ternary(i64)
declare ptr @math_to_balanced_ternary(i64)
declare i64 @math_from_balanced_ternary(ptr)
declare ptr @Ok_new(i64)
declare ptr @Err_new(ptr)
declare ptr @Unknown_new(ptr)
declare ptr @Ok(i64)
declare ptr @Err(ptr)
declare ptr @Unknown(ptr)
declare i1 @result_is_ok(ptr)
declare i1 @result_is_err(ptr)
declare i1 @result_is_unknown(ptr)
declare ptr @Channel_try_recv(ptr)
declare void @async_sleep(i64)
declare ptr @async_spawn_task(i64)
declare i64 @Task_join(i64)
declare i64 @async_select(ptr)
declare ptr @_block_on(i64)
declare i8 @AtomicTrit_load(ptr)
declare void @AtomicTrit_store(ptr, i8)
declare i64 @MutexGuard_get(i64)
declare void @MutexGuard_unlock(i64)
declare void @MutexGuard_update(i64, ptr)
declare void @time_sleep(i64)
declare ptr @Barrier_new(i64)
declare i1 @Barrier_wait(ptr)
declare i64 @Barrier_count(ptr)
declare ptr @__lp_from_flat(ptr, i64)
";

/// The definitions backing INTERNAL_HELPER_SIGS.
///
/// Balanced-ternary array conventions mirror the T3 emulator exactly
/// (emulator/syscall_io.rs syscalls 8, 10, 11, 12, 13) so both backends
/// produce identical output:
///   * a `[trit]` value is a length-prefixed i64-slot array:
///     mem[0] = len, mem[1..=len] = trits, least-significant trit first
///   * trits_to_str renders in array order; the empty array renders \"0\"
///   * trit_shift_left/right multiply/round-divide by 3^k (round to
///     nearest — dropping balanced trits is not truncation; ties cannot
///     happen because 3^k is odd)
pub(crate) const INTERNAL_RUNTIME_HELPERS: &str = r#"@__manit_s_true = private unnamed_addr constant [5 x i8] c"true\00", align 1
@__manit_s_false = private unnamed_addr constant [6 x i8] c"false\00", align 1
@__manit_s_unknown = private unnamed_addr constant [8 x i8] c"unknown\00", align 1
@__manit_s_chempty = private unnamed_addr constant [6 x i8] c"empty\00", align 1
@__manit_s_chclosed = private unnamed_addr constant [7 x i8] c"closed\00", align 1

define internal void @io_println_bool(i1 %b) {
entry:
  %s = select i1 %b, ptr @__manit_s_true, ptr @__manit_s_false
  call void @io_println(ptr %s)
  ret void
}

define internal void @io_println_bool3(i8 %t) {
entry:
  %ispos = icmp sgt i8 %t, 0
  %isneg = icmp slt i8 %t, 0
  %nf = select i1 %isneg, ptr @__manit_s_false, ptr @__manit_s_unknown
  %s = select i1 %ispos, ptr @__manit_s_true, ptr %nf
  call void @io_println(ptr %s)
  ret void
}

define internal void @io_println_trit(i8 %t) {
entry:
  call void @io_print_trit(i8 %t)
  call i32 @putchar(i32 10)
  ret void
}

define internal void @io_println_ternary(i64 %n) {
entry:
  %s = call ptr @ternary_t27_to_str(i64 %n)
  call void @io_println(ptr %s)
  ret void
}

define internal i64 @ternary_int_to_t27(i64 %n) {
entry:
  ret i64 %n
}

define internal i64 @ternary_t27_to_int(i64 %n) {
entry:
  ret i64 %n
}

define internal i64 @ternary_int_to_t9(i64 %n) {
entry:
  ret i64 %n
}

define internal i64 @ternary_t9_to_int(i64 %n) {
entry:
  ret i64 %n
}

define internal i64 @ternary_int_to_tryte(i64 %n) {
entry:
  ret i64 %n
}

define internal i64 @ternary_tryte_to_int(i64 %n) {
entry:
  ret i64 %n
}


define internal i64 @ternary_t27_neg(i64 %n) {
entry:
  %r = sub i64 0, %n
  ret i64 %r
}

; 3^k with k clamped to [0, 26] (the T3 TSHI/TSHR clamp)
define internal i64 @__manit_pow3(i64 %k) {
entry:
  %kneg = icmp slt i64 %k, 0
  %k0 = select i1 %kneg, i64 0, i64 %k
  %kbig = icmp sgt i64 %k0, 26
  %kc = select i1 %kbig, i64 26, i64 %k0
  br label %cond
cond:
  %i = phi i64 [ 0, %entry ], [ %inext, %body ]
  %p = phi i64 [ 1, %entry ], [ %pnext, %body ]
  %done = icmp sge i64 %i, %kc
  br i1 %done, label %exit, label %body
body:
  %pnext = mul i64 %p, 3
  %inext = add i64 %i, 1
  br label %cond
exit:
  ret i64 %p
}

define internal i64 @ternary_trit_shift_left(i64 %n, i64 %k) {
entry:
  %p = call i64 @__manit_pow3(i64 %k)
  %r = mul i64 %n, %p
  ret i64 %r
}

define internal i64 @ternary_trit_shift_right(i64 %n, i64 %k) {
entry:
  ; round-to-nearest division by 3^k: floor((n + (3^k - 1)/2) / 3^k)
  %p = call i64 @__manit_pow3(i64 %k)
  %pm1 = sub i64 %p, 1
  %bias = sdiv i64 %pm1, 2
  %t = add i64 %n, %bias
  %q = sdiv i64 %t, %p
  %r = srem i64 %t, %p
  %rneg = icmp slt i64 %r, 0
  %adj = select i1 %rneg, i64 -1, i64 0
  %fq = add i64 %q, %adj
  ret i64 %fq
}

define internal i64 @ternary_tryte_from_trits(i64 %a, i64 %b, i64 %c) {
entry:
  %a9 = mul i64 %a, 9
  %b3 = mul i64 %b, 3
  %s = add i64 %a9, %b3
  %r = add i64 %s, %c
  ret i64 %r
}

define internal i64 @ternary_pack_trits(ptr %arr) {
entry:
  %len = load i64, ptr %arr, align 8
  br label %cond
cond:
  %i = phi i64 [ 0, %entry ], [ %inext, %body ]
  %acc = phi i64 [ 0, %entry ], [ %accnext, %body ]
  %base = phi i64 [ 1, %entry ], [ %basenext, %body ]
  %done = icmp sge i64 %i, %len
  br i1 %done, label %exit, label %body
body:
  %idx = add i64 %i, 1
  %ep = getelementptr i64, ptr %arr, i64 %idx
  %t = load i64, ptr %ep, align 8
  %tm = mul i64 %t, %base
  %accnext = add i64 %acc, %tm
  %basenext = mul i64 %base, 3
  %inext = add i64 %i, 1
  br label %cond
exit:
  ret i64 %acc
}

; Returns a raw [trit; 27] array. Trit array elements are single i8 slots
; in the LLVM lowering (array_value_ty), so the result is 27 bytes.

define internal ptr @ternary_trits_to_str(ptr %arr) {
entry:
  %len = load i64, ptr %arr, align 8
  %isempty = icmp sle i64 %len, 0
  %n = select i1 %isempty, i64 1, i64 %len
  %n1 = add i64 %n, 1
  %s = call ptr @malloc(i64 %n1)
  br i1 %isempty, label %empty, label %cond
empty:
  store i8 48, ptr %s, align 1
  %e1 = getelementptr i8, ptr %s, i64 1
  store i8 0, ptr %e1, align 1
  ret ptr %s
cond:
  %i = phi i64 [ 0, %entry ], [ %inext, %body ]
  %done = icmp sge i64 %i, %len
  br i1 %done, label %exit, label %body
body:
  %idx = add i64 %i, 1
  %ep = getelementptr i64, ptr %arr, i64 %idx
  %t = load i64, ptr %ep, align 8
  %ispos = icmp sgt i64 %t, 0
  %isneg = icmp slt i64 %t, 0
  %cneg = select i1 %isneg, i8 45, i8 48
  %ch = select i1 %ispos, i8 43, i8 %cneg
  %sp = getelementptr i8, ptr %s, i64 %i
  store i8 %ch, ptr %sp, align 1
  %inext = add i64 %i, 1
  br label %cond
exit:
  %endp = getelementptr i8, ptr %s, i64 %len
  store i8 0, ptr %endp, align 1
  ret ptr %s
}

define internal ptr @math_to_balanced_ternary(i64 %n) {
entry:
  %buf = call ptr @malloc(i64 336)
  %iszero = icmp eq i64 %n, 0
  br i1 %iszero, label %zero, label %cond
zero:
  store i64 1, ptr %buf, align 8
  %z1 = getelementptr i64, ptr %buf, i64 1
  store i64 0, ptr %z1, align 8
  ret ptr %buf
cond:
  %i = phi i64 [ 0, %entry ], [ %inext, %body ]
  %val = phi i64 [ %n, %entry ], [ %valnext, %body ]
  %done = icmp eq i64 %val, 0
  br i1 %done, label %exit, label %body
body:
  %r0 = srem i64 %val, 3
  %rneg = icmp slt i64 %r0, 0
  %radd = select i1 %rneg, i64 3, i64 0
  %rem = add i64 %r0, %radd
  %istwo = icmp eq i64 %rem, 2
  %d = select i1 %istwo, i64 -1, i64 %rem
  %idx = add i64 %i, 1
  %ep = getelementptr i64, ptr %buf, i64 %idx
  store i64 %d, ptr %ep, align 8
  %sub = sub i64 %val, %d
  %valnext = sdiv i64 %sub, 3
  %inext = add i64 %i, 1
  br label %cond
exit:
  store i64 %i, ptr %buf, align 8
  ret ptr %buf
}

; ternary_int_to_trits(n, width) -> length-prefixed buffer of exactly `width`
; trits, least-significant first, zero-padded, higher trits discarded.
;
; Added 19 August 2026. It had NO implementation on either backend — LLVM
; emitted a call to an undefined @ternary_int_to_trits and T3 failed to
; assemble — even though it is the example in stdlib/ternary.mt's own header.
; Same digit extraction as @math_to_balanced_ternary above; the only
; differences are the fixed trip count and that val == 0 needs no special case
; (rem 0 -> digit 0 -> val stays 0, which is exactly the zero padding).
define internal ptr @ternary_int_to_trits(i64 %n, i64 %w) {
entry:
  %wneg = icmp slt i64 %w, 0
  %width = select i1 %wneg, i64 0, i64 %w
  %slots = add i64 %width, 1
  %bytes = mul i64 %slots, 8
  %buf = call ptr @malloc(i64 %bytes)
  store i64 %width, ptr %buf, align 8
  br label %cond
cond:
  %i = phi i64 [ 0, %entry ], [ %inext, %body ]
  %val = phi i64 [ %n, %entry ], [ %valnext, %body ]
  %done = icmp sge i64 %i, %width
  br i1 %done, label %exit, label %body
body:
  %r0 = srem i64 %val, 3
  %rneg = icmp slt i64 %r0, 0
  %radd = select i1 %rneg, i64 3, i64 0
  %rem = add i64 %r0, %radd
  %istwo = icmp eq i64 %rem, 2
  %d = select i1 %istwo, i64 -1, i64 %rem
  %idx = add i64 %i, 1
  %ep = getelementptr i64, ptr %buf, i64 %idx
  store i64 %d, ptr %ep, align 8
  %sub = sub i64 %val, %d
  %valnext = sdiv i64 %sub, 3
  %inext = add i64 %i, 1
  br label %cond
exit:
  ret ptr %buf
}

define internal ptr @ternary_to_balanced_ternary(i64 %n) {
entry:
  %r = call ptr @math_to_balanced_ternary(i64 %n)
  ret ptr %r
}

; Trit-wise min / max of two t27 words.
;
; These are single T3ISA instructions on the other backend (TAND / TOR), which
; is the whole point of a ternary ISA, so the T3 intercept stays and LLVM gets
; a loop instead. Two implementations of one function is exactly what made four
; other ternary functions DIVERGENT, so this pair is pinned by a differential
; test that runs both backends over the same inputs and requires agreement.
;
; A balanced digit is `n mod 3` with a residue of 2 rewritten as -1 and carried.
; i64 holds 3^27 (~7.6e12) comfortably, so `place` needs no overflow guard here
; the way the ManiT versions do on T3.
define internal i64 @__t27_zip(i64 %a, i64 %b, i1 %want_max) {
entry:
  br label %cond
cond:
  %i = phi i64 [ 0, %entry ], [ %inext, %body ]
  %va = phi i64 [ %a, %entry ], [ %vanext, %body ]
  %vb = phi i64 [ %b, %entry ], [ %vbnext, %body ]
  %acc = phi i64 [ 0, %entry ], [ %accnext, %body ]
  %place = phi i64 [ 1, %entry ], [ %placenext, %body ]
  %done = icmp sge i64 %i, 27
  br i1 %done, label %exit, label %body
body:
  %ra0 = srem i64 %va, 3
  %raneg = icmp slt i64 %ra0, 0
  %raadj = select i1 %raneg, i64 3, i64 0
  %ra = add i64 %ra0, %raadj
  %raistwo = icmp eq i64 %ra, 2
  %da = select i1 %raistwo, i64 -1, i64 %ra
  %vasub = sub i64 %va, %da
  %vanext = sdiv i64 %vasub, 3
  %rb0 = srem i64 %vb, 3
  %rbneg = icmp slt i64 %rb0, 0
  %rbadj = select i1 %rbneg, i64 3, i64 0
  %rb = add i64 %rb0, %rbadj
  %rbistwo = icmp eq i64 %rb, 2
  %db = select i1 %rbistwo, i64 -1, i64 %rb
  %vbsub = sub i64 %vb, %db
  %vbnext = sdiv i64 %vbsub, 3
  %agt = icmp sgt i64 %da, %db
  %alt = icmp slt i64 %da, %db
  %pick = select i1 %want_max, i1 %agt, i1 %alt
  %d = select i1 %pick, i64 %da, i64 %db
  %term = mul i64 %d, %place
  %accnext = add i64 %acc, %term
  %placenext = mul i64 %place, 3
  %inext = add i64 %i, 1
  br label %cond
exit:
  ret i64 %acc
}

define internal i64 @ternary_t27_and(i64 %a, i64 %b) {
entry:
  %r = call i64 @__t27_zip(i64 %a, i64 %b, i1 false)
  ret i64 %r
}

define internal i64 @ternary_t27_or(i64 %a, i64 %b) {
entry:
  %r = call i64 @__t27_zip(i64 %a, i64 %b, i1 true)
  ret i64 %r
}

define internal i64 @math_from_balanced_ternary(ptr %arr) {
entry:
  %r = call i64 @ternary_pack_trits(ptr %arr)
  ret i64 %r
}

; __lp_from_flat(flat_ptr, len) -> length-prefixed buffer
;
; Bridges the two `[trit]` layouts. A *flat* trit array — which is what an
; unsized `[trit]` function parameter always is — stores element i at slot i
; with the length carried out of band, one byte per trit here. The stdlib
; functions above instead read a length-prefixed buffer: mem[0] = len,
; trits at mem[1..=len], one i64 slot each. Handing a flat array straight to
; one of them made the first trit be read as the length.
;
; The length is only known at run time, so this cannot be an alloca in the
; caller — hence a helper that mallocs, exactly as @math_to_balanced_ternary
; already does. The T3 counterpart is syscall #203; there the flat array is
; word-per-slot, so it copies without widening.
;
; sext, not zext: a trit is a signed i8 and -1 must stay -1 (see §1).
define internal ptr @__lp_from_flat(ptr %p, i64 %n) {
entry:
  %n1 = add i64 %n, 1
  %bytes = mul i64 %n1, 8
  %buf = call ptr @malloc(i64 %bytes)
  store i64 %n, ptr %buf, align 8
  br label %cond
cond:
  %i = phi i64 [ 0, %entry ], [ %inext, %body ]
  %done = icmp sge i64 %i, %n
  br i1 %done, label %exit, label %body
body:
  %sp = getelementptr i8, ptr %p, i64 %i
  %t8 = load i8, ptr %sp, align 1
  %t64 = sext i8 %t8 to i64
  %di = add i64 %i, 1
  %dp = getelementptr i64, ptr %buf, i64 %di
  store i64 %t64, ptr %dp, align 8
  %inext = add i64 %i, 1
  br label %cond
exit:
  ret ptr %buf
}

; --- Result<T, E> ---------------------------------------------------------
; The IR lowers `match` on Result to direct word access: word 0 = tag
; (1 = Ok, 0 = Unknown, -1 = Err), word 1 = payload (Ok value, or the
; Err/Unknown message pointer). The C runtime's ManitResult keeps the
; message in a THIRD field with val = 0, which made every Err/Unknown
; payload print as (null) on LLVM. These internal constructors build the
; two-word layout the IR actually reads.

define internal ptr @Ok_new(i64 %v) {
entry:
  %r = call ptr @malloc(i64 16)
  store i64 1, ptr %r, align 8
  %p1 = getelementptr i64, ptr %r, i64 1
  store i64 %v, ptr %p1, align 8
  ret ptr %r
}

define internal ptr @Err_new(ptr %m) {
entry:
  %r = call ptr @malloc(i64 16)
  store i64 -1, ptr %r, align 8
  %mi = ptrtoint ptr %m to i64
  %p1 = getelementptr i64, ptr %r, i64 1
  store i64 %mi, ptr %p1, align 8
  ret ptr %r
}

define internal ptr @Unknown_new(ptr %m) {
entry:
  %r = call ptr @malloc(i64 16)
  store i64 0, ptr %r, align 8
  %mi = ptrtoint ptr %m to i64
  %p1 = getelementptr i64, ptr %r, i64 1
  store i64 %mi, ptr %p1, align 8
  ret ptr %r
}

define internal ptr @Ok(i64 %v) {
entry:
  %r = call ptr @Ok_new(i64 %v)
  ret ptr %r
}

define internal ptr @Err(ptr %m) {
entry:
  %r = call ptr @Err_new(ptr %m)
  ret ptr %r
}

; Sequential counting barrier, mirroring T3 emulator syscalls 117/118.
; Spawn blocks execute inline, so a real pthread barrier would block the
; single thread forever; instead each wait counts an arrival and the last
; arrival (the leader) resets the cycle. Layout: [0]=needed, [1]=arrived.
define internal ptr @Barrier_new(i64 %n) {
entry:
  %b = call ptr @malloc(i64 16)
  store i64 %n, ptr %b, align 8
  %p1 = getelementptr i64, ptr %b, i64 1
  store i64 0, ptr %p1, align 8
  ret ptr %b
}

define internal i1 @Barrier_wait(ptr %b) {
entry:
  %pn = getelementptr i64, ptr %b, i64 0
  %needed = load i64, ptr %pn, align 8
  %pa = getelementptr i64, ptr %b, i64 1
  %arrived = load i64, ptr %pa, align 8
  %next = add i64 %arrived, 1
  %done = icmp sge i64 %next, %needed
  %reset = select i1 %done, i64 0, i64 %next
  store i64 %reset, ptr %pa, align 8
  ret i1 %done
}

define internal i64 @Barrier_count(ptr %b) {
entry:
  %n = load i64, ptr %b, align 8
  ret i64 %n
}

; Method-name aliases: the language spells these .load()/.store(), the C
; runtime exports AtomicTrit_get/AtomicTrit_set.
define internal i8 @AtomicTrit_load(ptr %a) {
entry:
  %v = call i8 @AtomicTrit_get(ptr %a)
  ret i8 %v
}

define internal void @AtomicTrit_store(ptr %a, i8 %v) {
entry:
  call void @AtomicTrit_set(ptr %a, i8 %v)
  ret void
}

; A MutexGuard is the mutex handle itself (the T3 emulator model: lock is a
; no-op returning the handle in the sequential world; here the mutex was
; already locked by Mutex_lock at the `.lock()` call site).
define internal i64 @MutexGuard_get(i64 %g) {
entry:
  %m = inttoptr i64 %g to ptr
  %v = call i64 @Mutex_get(ptr %m)
  ret i64 %v
}

define internal void @MutexGuard_unlock(i64 %g) {
entry:
  %m = inttoptr i64 %g to ptr
  call void @Mutex_unlock(ptr %m)
  ret void
}

define internal void @MutexGuard_update(i64 %g, ptr %f) {
entry:
  %m = inttoptr i64 %g to ptr
  %v = call i64 @Mutex_get(ptr %m)
  %nv = call i64 %f(i64 %v)
  call void @Mutex_set(ptr %m, i64 %nv)
  ret void
}

define internal void @time_sleep(i64 %ms) {
entry:
  call void @time_sleep_ms(i64 %ms)
  ret void
}

; Eager async model, mirroring the T3 emulator (syscalls 122-126): async fn
; bodies run at the call site, async_spawn_task boxes the finished result,
; Task_join/await unboxes it, and select resolves to the first future in the
; vector (index 0) since everything has already completed.
define internal void @async_sleep(i64 %ms) {
entry:
  %us = mul i64 %ms, 1000
  %us32 = trunc i64 %us to i32
  call i32 @usleep(i32 %us32)
  ret void
}

define internal ptr @async_spawn_task(i64 %result) {
entry:
  %box = call ptr @malloc(i64 8)
  store i64 %result, ptr %box, align 8
  ret ptr %box
}

define internal i64 @Task_join(i64 %handle) {
entry:
  %p = inttoptr i64 %handle to ptr
  %v = load i64, ptr %p, align 8
  ret i64 %v
}

define internal i64 @async_select(ptr %futs) {
entry:
  %val = call i64 @Vec_get(ptr %futs, i64 0)
  %sel = call ptr @malloc(i64 16)
  store i64 0, ptr %sel, align 8
  %p1 = getelementptr i64, ptr %sel, i64 1
  store i64 %val, ptr %p1, align 8
  %r = ptrtoint ptr %sel to i64
  ret i64 %r
}

; select-result .block_on(): the select tuple (winner_idx, winner_val) was
; fully resolved by async_select; just hand the tuple pointer back.
define internal ptr @_block_on(i64 %sel) {
entry:
  %p = inttoptr i64 %sel to ptr
  ret ptr %p
}

; Non-blocking channel receive, mirroring T3 syscall 108: Ok(value) if an
; item is queued, otherwise Err("closed") for a closed channel and
; Err("empty") for an open one. Check emptiness first so a queued item on
; a closed channel is still drained, exactly like the emulator.
define internal ptr @Channel_try_recv(ptr %c) {
entry:
  %empty = call i1 @Channel_is_empty(ptr %c)
  br i1 %empty, label %no_item, label %have_item
have_item:
  %v = call i64 @Channel_recv(ptr %c)
  %ok = call ptr @Ok_new(i64 %v)
  ret ptr %ok
no_item:
  %closed = call i1 @Channel_is_closed(ptr %c)
  %msg = select i1 %closed, ptr @__manit_s_chclosed, ptr @__manit_s_chempty
  %err = call ptr @Err_new(ptr %msg)
  ret ptr %err
}

define internal ptr @Unknown(ptr %m) {
entry:
  %r = call ptr @Unknown_new(ptr %m)
  ret ptr %r
}

define internal i1 @result_is_ok(ptr %r) {
entry:
  %tag = load i64, ptr %r, align 8
  %b = icmp eq i64 %tag, 1
  ret i1 %b
}

define internal i1 @result_is_err(ptr %r) {
entry:
  %tag = load i64, ptr %r, align 8
  %b = icmp eq i64 %tag, -1
  ret i1 %b
}

define internal i1 @result_is_unknown(ptr %r) {
entry:
  %tag = load i64, ptr %r, align 8
  %b = icmp eq i64 %tag, 0
  ret i1 %b
}

; An internal `result_unwrap` USED TO LIVE HERE (spelled with a leading
; sigil, which is why this note is not), and it read word 1 with no tag check at
; all: unwrapping an Err would have handed back the message pointer as if it
; were the value. It was LLVM-only — T3 had no counterpart, which is why
; `.unwrap()` failed at assembly there rather than misbehaving here. Both now
; go through ir/lower/lower_result.rs, which emits the tag guard
; (manit_check_result_ok / SYSCALL #561) and then the same GetPtr + Load that
; `match` uses.
"#;

// ---------------------------------------------------------------------------
// String escaping for LLVM IR constant strings
// ---------------------------------------------------------------------------

/// Escape a Rust string into the LLVM IR `c"..."` constant syntax.
/// Non-printable / non-ASCII bytes become `\XX` (uppercase hex).
pub(crate) fn llvm_escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for byte in s.bytes() {
        match byte {
            // Printable ASCII except '"' (0x22) and '\' (0x5C)
            0x20..=0x21 | 0x23..=0x5B | 0x5D..=0x7E => {
                out.push(byte as char);
            }
            b => {
                out.push('\\');
                out.push(char::from_digit((b >> 4) as u32, 16).unwrap().to_ascii_uppercase());
                out.push(char::from_digit((b & 0xF) as u32, 16).unwrap().to_ascii_uppercase());
            }
        }
    }
    out
}

