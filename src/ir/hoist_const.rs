//! P36 — loop-invariant constant materialisation.
//!
//! © Manish Jagdish Thatte
//!
//! A constant is an OPERAND in this IR, not an instruction, so there is nothing
//! in a loop for an ordinary code-motion pass to lift. That is P40's correction
//! to P36's original premise and it is still true — which is why this pass
//! CREATES the instruction it then hoists, rather than moving one.
//!
//! **What it is for is a fact about T3ISA and not about the IR.** Every
//! data-processing opcode carries a balanced 3-trit immediate in its third
//! operand slot, so a constant in `-13..=13` is free (P40). Anything wider is
//! materialised with a `TLIT`, and the emitter materialises it at each USE — so
//! a loop body containing `acc + 24690` emits `TLIT R7, #24690` on every
//! iteration. Measured over the seventeen examples before this pass existed:
//! 43,191 `TLIT`s executed against 22,291 static sites, so **20,900 of the
//! executions are re-materialisations of a value that never changed, 5.57 % of
//! all instructions executed.** It is concentrated rather than spread —
//! `float_demo` 12.8 %, `crypto_demo` 11.5 %, `fibonacci` 10.8 %, and nine of
//! the seventeen have none at all.
//!
//! **T3-only, and that is a decision rather than an oversight.** On LLVM a
//! constant operand costs nothing and clang would fold the extra temp straight
//! back out, so running it there would move the emitted `.ll` for no gain and
//! break the byte-for-byte comparison `--no-mem2reg` exists to give. The IR the
//! two backends receive therefore differs by this pass alone, and the property
//! that matters — that they compute the same answers — is checked by the parity
//! matrix rather than assumed from a shared input.
//!
//! **Most of that 5.57 % was never available, and finding out why is the
//! result.** Hoisting into every loop is a PESSIMISATION — +4,880 (+1.30 %), 1
//! better and 9 worse — because `regalloc` keeps nothing in a register across a
//! call, so a value spanning one is spilled and each use becomes a frame `LOAD`
//! rather than a `TLIT`: the same instruction, plus the preheader. Refusing
//! call-containing loops gives **-932 (-0.25 %), 2 better and 0 worse**. See
//! the refusal in `run_function` for the numbers and for why register pressure,
//! the obvious explanation, is not the one.

use std::collections::{HashMap, HashSet};

use super::ssa::{Cfg, Dominators};
use super::types::*;

/// Constants below this magnitude ride in the 3-trit immediate field and cost
/// nothing to re-materialise, so hoisting them would spend a register to save
/// nothing. Mirrors `codegen_t3::emitter::t3_imm3`, which is the authority; the
/// two are checked against each other by
/// `hoist_const_tests::the_immediate_bound_matches_the_emitter`.
const IMM3_MAX: i64 = 13;

/// Most values to hoist out of any one loop.
///
/// A safety bound and **not a performance knob**: sweeping it over the examples
/// gives -612 at 1 and -932 at every value from 2 upward, so no loop in the set
/// has more than two hoistable constants and the cap binds on nothing measured.
/// It is kept so that a pathological function cannot lengthen dozens of live
/// ranges at once. (P36's own original finding had the same shape: the
/// `--inline-limit` curve was flat either side of the regression, which is what
/// showed the size was not the cause.)
const PER_LOOP_LIMIT: usize = 16;

pub fn run(module: &mut IRModule) -> usize {
    let mut hoisted = 0;
    for func in &mut module.functions {
        if func.is_extern || func.blocks.len() < 2 {
            continue;
        }
        hoisted += run_function(func);
    }
    hoisted
}

/// A constant worth materialising once, keyed so that two spellings of the same
/// value share a hoist.
#[derive(PartialEq, Eq, Hash, Clone)]
enum Key {
    Int(i64),
    /// Floats are keyed on their BIT PATTERN, not their value: `-0.0` and `0.0`
    /// compare equal and are not interchangeable, and NaN compares unequal to
    /// itself, so `f64` is not a key type.
    Float(u64),
}

impl Key {
    fn of(v: &IRValue) -> Option<Key> {
        match v {
            IRValue::Const(IRConst::Int(n)) if n.abs() > IMM3_MAX => Some(Key::Int(*n)),
            // A float costs more than a wide integer on this machine: it is a
            // `TLIT` of the sidecar address followed by `SYSCALL #219`, so a
            // loop re-materialising one pays two instructions and a syscall.
            IRValue::Const(IRConst::Float(f)) => Some(Key::Float(f.to_bits())),
            _ => None,
        }
    }
    fn value(&self) -> IRValue {
        match self {
            Key::Int(n) => IRValue::Const(IRConst::Int(*n)),
            Key::Float(b) => IRValue::Const(IRConst::Float(f64::from_bits(*b))),
        }
    }
    fn ty(&self) -> IRType {
        match self {
            Key::Int(_) => IRType::I64,
            Key::Float(_) => IRType::F64,
        }
    }
}

fn run_function(func: &mut IRFunction) -> usize {
    let cfg = Cfg::of(func);
    let dom = Dominators::of(&cfg);
    let n = func.blocks.len();

    // A back edge is one whose target DOMINATES its source; its target is a
    // natural loop's header. Computed from dominance rather than from block
    // order, because the lowerer's `while_body`/`while_exit` naming is a
    // convention and a pass that trusted it would be reading labels for
    // structure.
    let mut headers: Vec<(usize, Vec<usize>)> = Vec::new();
    for b in 0..n {
        for &s in &cfg.succs[b] {
            if dominates(&dom, s, b) {
                match headers.iter_mut().find(|(h, _)| *h == s) {
                    Some((_, tails)) => tails.push(b),
                    None => headers.push((s, vec![b])),
                }
            }
        }
    }
    if headers.is_empty() {
        return 0;
    }

    let mut used: HashSet<String> = HashSet::new();
    for b in &func.blocks {
        for i in &b.instrs {
            if let Some(d) = instr_dst(i) {
                used.insert(d);
            }
        }
    }
    // A fresh name the function does not already define. `mem2reg` has its own
    // prefix for the same reason: a collision silently merges two values.
    let mut counter = 0usize;
    fn fresh(counter: &mut usize, used: &HashSet<String>) -> String {
        loop {
            let name = format!("hc{}", *counter);
            *counter += 1;
            if !used.contains(&name) {
                return name;
            }
        }
    }

    let mut hoisted = 0;
    // Outermost first, so an inner loop sees the outer one's hoist already in
    // place and does not lift the same value twice.
    let mut order: Vec<usize> = (0..headers.len()).collect();
    order.sort_by_key(|&i| dom.rpo_pos.get(headers[i].0).copied().unwrap_or(usize::MAX));

    for oi in order {
        let (header, ref tails) = headers[oi];
        let body = loop_blocks(&cfg, header, tails);

        // The preheader must be the loop's ONLY entry from outside and must
        // reach the header by a plain jump — otherwise there is nowhere to put
        // a definition that runs exactly once. Loops without one are skipped
        // rather than given a synthesised preheader: inserting a block here
        // would have to re-run `split_critical_edges`' reasoning about phi
        // placement, and the measured population that would gain is small.
        let outside: Vec<usize> = cfg.preds[header]
            .iter()
            .copied()
            .filter(|p| !body.contains(p))
            .collect();
        if outside.len() != 1 {
            continue;
        }
        let pre = outside[0];
        if !matches!(func.blocks[pre].term, IRTerminator::Jump(_)) {
            continue;
        }

        // **A LOOP CONTAINING A CALL IS REFUSED, AND THAT REFUSAL IS THE WHOLE
        // DIFFERENCE BETWEEN THIS PASS AND A PESSIMISATION.**
        //
        // `regalloc` keeps nothing in a register across a call — the convention
        // is caller-saved — so a hoisted value that spans one is spilled, and
        // every use inside the loop becomes a frame `LOAD` instead of a `TLIT`.
        // That is the same one instruction it was, plus the preheader's
        // materialisation and store. Measured over the seventeen examples:
        // hoisting into every loop is **+4,880 (+1.30 %), 1 better and 9
        // worse**, and `crypto_demo` alone is +5,218; refusing the
        // call-containing ones turns that into **-932 (-0.25 %), 2 better and
        // 0 worse**, with `crypto_demo` moving from +5,218 to -312.
        //
        // Register pressure was the obvious explanation and it was WRONG for
        // the third time in this campaign (P30, P36's own first diagnosis):
        // sweeping the per-loop limit gives a FLAT curve, +5,159 at one value
        // per loop against +4,880 at eight, so hoisting a single constant is
        // already the whole regression. It is not how many live ranges cross
        // the loop, it is whether they cross a call.
        //
        // It also retires the measurement that motivated the pass. The
        // executed-minus-static `TLIT` count put 20,900 re-materialisations at
        // 5.57 % of all instructions; almost all of them are in loops that
        // call something, where the value must be either spilled or
        // re-materialised and BOTH cost one instruction. **There was no win
        // there to capture** — the 5.57 % was an upper bound on nothing.
        if body.iter().any(|&b| {
            func.blocks[b]
                .instrs
                .iter()
                .any(|i| matches!(i, IRInstr::Call { .. } | IRInstr::CallIndirect { .. }))
        }) {
            continue;
        }

        // Count uses inside the loop. Phi operands are deliberately not
        // counted and not rewritten: a phi's operand belongs to the EDGE it
        // arrives on, and the emitter places those copies itself.
        let mut counts: HashMap<Key, usize> = HashMap::new();
        for &b in &body {
            for instr in &func.blocks[b].instrs {
                if matches!(instr, IRInstr::Phi { .. }) {
                    continue;
                }
                for_each_operand(instr, &mut |v| {
                    if let Some(k) = Key::of(v) {
                        *counts.entry(k).or_insert(0) += 1;
                    }
                });
            }
        }
        if counts.is_empty() {
            continue;
        }

        let mut ranked: Vec<(Key, usize)> = counts.into_iter().collect();
        // Ties broken by the value so the output does not depend on hash order.
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| key_ord(&a.0).cmp(&key_ord(&b.0))));
        ranked.truncate(PER_LOOP_LIMIT);

        let mut defs: Vec<IRInstr> = Vec::new();
        for (k, _) in &ranked {
            let dst = IRTemp(fresh(&mut counter, &used));
            used.insert(dst.0.clone());
            let temp = IRValue::Temp(dst.clone());
            for &b in &body {
                for instr in &mut func.blocks[b].instrs {
                    if matches!(instr, IRInstr::Phi { .. }) {
                        continue;
                    }
                    for_each_operand_mut(instr, &mut |v| {
                        if Key::of(v).as_ref() == Some(k) {
                            *v = temp.clone();
                        }
                    });
                }
            }
            defs.push(IRInstr::Assign { dst, src: k.value(), ty: k.ty() });
            hoisted += 1;
        }
        // Appended at the END of the preheader, after everything already there:
        // the definition must dominate the loop, and nothing in the preheader
        // can depend on it.
        func.blocks[pre].instrs.extend(defs);
    }
    hoisted
}

fn key_ord(k: &Key) -> (u8, i128) {
    match k {
        Key::Int(n) => (0, *n as i128),
        Key::Float(b) => (1, *b as i128),
    }
}

fn dominates(dom: &Dominators, a: usize, b: usize) -> bool {
    if a == b {
        return true;
    }
    let mut cur = b;
    while let Some(p) = dom.idom.get(cur).copied().flatten() {
        if p == a {
            return true;
        }
        if p == cur {
            return false;
        }
        cur = p;
    }
    false
}

/// The natural loop of `header`: the header itself plus every block that
/// reaches a back-edge tail without passing through the header.
fn loop_blocks(cfg: &Cfg, header: usize, tails: &[usize]) -> HashSet<usize> {
    let mut body: HashSet<usize> = HashSet::new();
    body.insert(header);
    let mut stack: Vec<usize> = tails.to_vec();
    while let Some(b) = stack.pop() {
        if !body.insert(b) {
            continue;
        }
        for &p in &cfg.preds[b] {
            if !body.contains(&p) {
                stack.push(p);
            }
        }
    }
    body
}

fn instr_dst(i: &IRInstr) -> Option<String> {
    match i {
        IRInstr::BinOp { dst, .. }
        | IRInstr::UnOp { dst, .. }
        | IRInstr::Assign { dst, .. }
        | IRInstr::Alloca { dst, .. }
        | IRInstr::Load { dst, .. }
        | IRInstr::GetPtr { dst, .. }
        | IRInstr::TritMin { dst, .. }
        | IRInstr::TritMax { dst, .. }
        | IRInstr::TritNeg { dst, .. }
        | IRInstr::TritSign { dst, .. }
        | IRInstr::TritLane { dst, .. }
        | IRInstr::Phi { dst, .. }
        | IRInstr::Cast { dst, .. } => Some(dst.0.clone()),
        IRInstr::Call { dst, .. } | IRInstr::CallIndirect { dst, .. } => {
            dst.as_ref().map(|d| d.0.clone())
        }
        _ => None,
    }
}

/// Visit every operand an instruction READS.
///
/// Written out rather than derived, and the `Alloca`/`BoundsCheck` omissions
/// are the point: an `Alloca` reads nothing, and a `BoundsCheck`'s `len` is a
/// `usize` field rather than an `IRValue`, so neither can carry a constant this
/// pass could hoist. A new `IRInstr` variant leaves this function compiling and
/// no longer complete (P72's hazard), so it is paired with
/// `hoist_const_tests::every_instruction_variant_is_visited`.
fn for_each_operand(i: &IRInstr, f: &mut impl FnMut(&IRValue)) {
    match i {
        IRInstr::BinOp { lhs, rhs, .. } => { f(lhs); f(rhs); }
        IRInstr::UnOp { operand, .. } => f(operand),
        IRInstr::Assign { src, .. } => f(src),
        IRInstr::Store { ptr, val, .. } => { f(ptr); f(val); }
        IRInstr::Load { ptr, .. } => f(ptr),
        IRInstr::Call { args, .. } => args.iter().for_each(f),
        IRInstr::CallIndirect { fn_ptr, args, .. } => { f(fn_ptr); args.iter().for_each(f); }
        IRInstr::GetPtr { ptr, idx, .. } => { f(ptr); f(idx); }
        IRInstr::BoundsCheck { idx, .. } => f(idx),
        IRInstr::TritMin { a, b, .. } | IRInstr::TritMax { a, b, .. } => { f(a); f(b); }
        IRInstr::TritNeg { a, .. } | IRInstr::TritSign { a, .. } => f(a),
        IRInstr::TritLane { a, b, .. } => { f(a); f(b); }
        IRInstr::PrintStr(v) | IRInstr::PrintInt(v) | IRInstr::PrintFloat(v)
        | IRInstr::PrintBool3(v) | IRInstr::PrintTrit(v) => f(v),
        IRInstr::Phi { incoming, .. } => incoming.iter().for_each(|(v, _)| f(v)),
        IRInstr::Cast { src, .. } => f(src),
        IRInstr::Alloca { .. } => {}
    }
}

fn for_each_operand_mut(i: &mut IRInstr, f: &mut impl FnMut(&mut IRValue)) {
    match i {
        IRInstr::BinOp { lhs, rhs, .. } => { f(lhs); f(rhs); }
        IRInstr::UnOp { operand, .. } => f(operand),
        IRInstr::Assign { src, .. } => f(src),
        IRInstr::Store { ptr, val, .. } => { f(ptr); f(val); }
        IRInstr::Load { ptr, .. } => f(ptr),
        IRInstr::Call { args, .. } => args.iter_mut().for_each(f),
        IRInstr::CallIndirect { fn_ptr, args, .. } => { f(fn_ptr); args.iter_mut().for_each(f); }
        IRInstr::GetPtr { ptr, idx, .. } => { f(ptr); f(idx); }
        IRInstr::BoundsCheck { idx, .. } => f(idx),
        IRInstr::TritMin { a, b, .. } | IRInstr::TritMax { a, b, .. } => { f(a); f(b); }
        IRInstr::TritNeg { a, .. } | IRInstr::TritSign { a, .. } => f(a),
        IRInstr::TritLane { a, b, .. } => { f(a); f(b); }
        IRInstr::PrintStr(v) | IRInstr::PrintInt(v) | IRInstr::PrintFloat(v)
        | IRInstr::PrintBool3(v) | IRInstr::PrintTrit(v) => f(v),
        IRInstr::Phi { incoming, .. } => incoming.iter_mut().for_each(|(v, _)| f(v)),
        IRInstr::Cast { src, .. } => f(src),
        IRInstr::Alloca { .. } => {}
    }
}
