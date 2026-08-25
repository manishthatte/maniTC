//! F-2 — function inlining.
//!
//! The recommendations call this the biggest single win in the performance
//! tier, and the shape of the opportunity was measured before any of it was
//! written. Across the 17 shipped examples, after `mem2reg` and the per-function
//! passes:
//!
//! | | |
//! |---|---|
//! | `Call` instructions | 6,331 |
//! | of those, to a callee whose body is in this module | 1,750 |
//! | non-recursive, with a callee of 16 instructions or fewer | 1,101 |
//! | **of those, callee is ONE block ending in `Return`** | **482 (44 %)** |
//!
//! **The pass does both halves now.** The single-block path came first and is
//! below; a single-block callee needs no control flow at all, its instructions
//! being spliced where the `Call` stood without dividing the caller's block.
//! The CFG path at the end of this file does the other 56 %: it splits the
//! caller's block in two, copies the callee's blocks between the halves,
//! rewrites every `Return` into a jump to the continuation, and joins the
//! returned values with a phi.
//!
//! Doing the easy 44 % first was right, and not only for caution — measuring
//! it is what made the hard 56 % legible. Both halves turned out to be worth
//! about the same, and both turned out to be worth it only after a refusal
//! that measurement found and nothing predicted: the stdlib refusal below for
//! the first, the LOOP refusal for the second (report.txt P30, P36).
//!
//! ## What the CFG path cost to get right
//!
//! Two defects, and NEITHER INSTRUMENT CAUGHT BOTH (report.txt P34, P35).
//! A terminator's OPERANDS were not renamed, only its labels, so every spliced
//! branch tested a temp only the original callee defines — 65 `--verify-ssa`
//! violations across 7 of the 17 examples, and a plausible printed answer on
//! T3. And `IRValue::Void` reached a phi arm from the trailing block of an
//! exhaustive `match` — ZERO `--verify-ssa` violations, a correct printed
//! answer on T3, and a refusal to parse on LLVM.
//!
//! P34 was loud — six of seventeen test targets red — but no failure NAMED it:
//! on T3 the programs ran and printed plausible wrong answers, so every message
//! read "expected X, got Y". P35 was caught by exactly one test, and only
//! because LLVM refuses to parse the module. **The SSA verifier is the first
//! thing to run against a pass that rewrites control flow**, before any test of
//! a program's output — a failing output test says something is wrong, and the
//! verifier says what. It existed throughout and was not run.
//!
//! ## The 482 was mostly an illusion, and measuring it is what said so
//!
//! The paragraph that stood here called the most-called candidates — pure
//! forwarding wrappers, `fmt::align_right` at 91 sites and `fmt::show_t27` at
//! 44, each a ONE-instruction body — the best of the opportunity, on the
//! reasoning that inlining them removes a call frame and adds nothing.
//!
//! **It adds 188 instructions per call.** Those wrappers are the *mixed*
//! standard library: `stdlib_expand` merges a ManiT body for them AND each
//! emitter intercepts the call, so on T3 `fmt::align_right` is `SYSCALL #15`,
//! one instruction, and the ManiT body is compiled and never reached. Splicing
//! it is a correct transformation of a body the backend had already replaced
//! with something an order of magnitude cheaper. Measured, before the refusal
//! in `backend_may_implement` below: **+5.1 % dynamic instructions across the
//! seventeen examples and +159 % on `ternary_calculator`**, with byte-identical
//! output on both backends and a 17/17 parity matrix. See report.txt P30.
//!
//! Of the 318 sites the pass spliced across the examples, **240 (75 %) were
//! stdlib**, and they were the whole of the regression. What is left is 78
//! sites in user code, worth **−727 dynamic instructions, −0.17 %**.
//!
//! So the recommendations' "biggest single win" is, on this corpus, an order of
//! magnitude smaller than the CSE re-scoping that came before it (−1.51 %) —
//! the second time in this phase that measuring a recommendation's premise has
//! contradicted it, after F-1's. The pass is kept because it is a real and
//! uniform improvement, every example neutral or better and none worse, and
//! because it is the prerequisite for the multi-block half. It is not kept
//! because it was predicted to be large.
//!
//! ## And the multi-block half repeated the lesson exactly
//!
//! Correct and SSA-clean, it made the 17 examples **423,782 dynamic
//! instructions against 421,739 with the CFG path off — +2,043, +0.48 %**.
//! The cause is not the splice: it is that substituting a CONSTANT argument
//! into a loop bound turns a register-resident value into a literal the T3
//! backend re-materialises with `TLIT` every iteration, having no
//! loop-invariant code motion (report.txt P36). Register pressure was the
//! obvious hypothesis and was wrong for the second time in this phase; the
//! same callee with an OPAQUE argument is −9 rather than +165.
//!
//! With the loop refusal in `collect`:
//!
//! | | dynamic instructions over the 17 |
//! |---|---|
//! | no inlining at all | 422,466 |
//! | single-block path only | 421,739 |
//! | **both paths** | **420,668 (−1,071, −0.25 %)** |
//! | improved / unchanged / worse | **7 / 10 / 0** |
//!
//! So the multi-block half is worth slightly MORE than the single-block half
//! (−727), and the two together take the pass to −1,798, −0.43 %. The size
//! limit also stops being a performance knob: the spread across limits 8 to 24
//! collapses from 3,103 instructions to 15, which is the sharpest available
//! statement that the refusal addresses the mechanism rather than fitting a
//! constant to seventeen programs.

use std::collections::{HashMap, HashSet};

use super::types::*;
use crate::semantic::SemanticAnalyzer;

/// Is this callee one the BACKENDS implement themselves?
///
/// **This is the single most important refusal in the pass, and it is not
/// about correctness — it is about not undoing the backend's work.** The
/// standard library modules that `stdlib_expand` merges are *mixed*: `fmt`,
/// `str`, `ternary`, `math`, `env`, `test` and `trit` each ship a ManiT body
/// AND an interception in each emitter. `fmt::align_right` has a three-line
/// ManiT body that forwards to `str::pad_left`, and the T3 emitter never emits
/// it: a call to it becomes `SYSCALL #15`, ONE instruction. The body is
/// compiled and dead.
///
/// Splice that dead body and the call stops being a syscall and becomes the
/// software implementation — measured at **564 → 4,324 instructions** on a
/// twenty-iteration loop, a 7.7× pessimisation with byte-identical output, from
/// inlining a callee whose body is a single instruction. Nothing about the IR
/// says so: the body is a well-formed one-block function and the splice is
/// correct. It just costs 188 instructions per call to save one frame.
///
/// The test is deliberately the MODULE and not a list of intercepted names.
/// The two emitters intercept different sets — `str::pad_left` is a syscall on
/// LLVM (`@str_pad_left`) and compiled ManiT on T3 — and the IR is shared, so a
/// name-level rule would have to be the union of two lists that live in 64
/// match arms across two backends and would rot the first time either moved.
/// `STDLIB_MODULES` is the boundary the rest of the compiler already draws, it
/// is a superset of what either emitter intercepts, and it cannot go stale.
///
/// It is also sharper than "superset" makes it sound. A candidate must have a
/// BODY in this module, and measured over the examples the only bodied
/// functions are `math`, `str`, `fmt` and `ternary` — the mixed modules — plus
/// the user's own. The pure natives never appear: `Vec::push` is `SYSCALL #18`
/// with no ManiT body anywhere, so it could never have been a candidate, and
/// `io::` likewise. So refusing by module refuses exactly the bodied stdlib and
/// nothing else.
///
/// What it gives up is inlining the stdlib, and the measurement says that is
/// not a loss: on the seventeen examples the stdlib candidates were the whole
/// of the regression.
fn backend_may_implement(name: &str) -> bool {
    name.split_once("::")
        .is_some_and(|(prefix, _)| SemanticAnalyzer::STDLIB_MODULES.contains(&prefix))
}

/// The largest callee body, in IR instructions, worth splicing into a caller.
///
/// A size limit is what keeps inlining from being a code-size explosion:
/// splicing an N-instruction body at S sites adds `S * (N - 1)` instructions
/// net of the `Call` it removes, so the cost is linear in the limit and the
/// benefit — one call frame — is not.
pub const SIZE_LIMIT: usize = 16;

/// The share of a module's own size the pass may ADD, and the floor below
/// which that share is not the binding rule.
///
/// **`SIZE_LIMIT` bounds one splice; it does not bound the pass**, and the
/// difference is the whole of report.txt P38's inlining half. A body of twelve
/// instructions spliced at 597 sites is 597 legal splices and a module three
/// and a half times its original size — measured on
/// `oracle/census/math_agent_work/math_log/sweep0.mt`, where `relerr18` has
/// exactly that shape and the IR goes 4,084 → 12,978 instructions, **+218 %**,
/// and the emitted T3 image 26,245 → 94,473 words. That image no longer fits
/// below the stack at 60,000, so fourteen programs of the 1,147-file corpus
/// stopped working — silently, before the assembler learned to say so.
///
/// The measurement is what sets the numbers. Across the seventeen examples the
/// pass adds between 0.0 % and 2.8 %, except on `patent_classify`, whose whole
/// module is 21 instructions and which grows 71 % by gaining 15 — **which is
/// why a percentage alone is the wrong rule and there is a floor.** 20 % with a
/// floor of 64 leaves every example untouched (the largest absolute growth is
/// +111 on `ternary_calculator`, against a budget of 800) and cuts sweep0's
/// +8,894 to +816.
///
/// **The budget is FIRST-COME, and that is a real cost worth naming.** Sites
/// are charged in module order and then block order, so which callees get
/// spliced once the budget runs low depends on where they appear rather than
/// on what they are worth. It is deterministic — the same module always gives
/// the same answer — but it is not fair, and a program that adds a function
/// near the top can change what gets inlined near the bottom. Ranking
/// candidates by call count or by body size would be better and is a separate
/// change; the budget exists to stop an unbounded explosion, and it does that
/// whatever order it charges in.
const GROWTH_PERCENT: usize = 20;
const GROWTH_FLOOR: usize = 64;

/// A callee this pass is willing to splice.
struct Candidate {
    /// The parameter temp names, in order, AS THE BODY SPELLS THEM. The lowerer
    /// names them `param_<x>` and they are ordinary temps there, so binding an
    /// argument is a substitution rather than an instruction.
    ///
    /// `IRFunction::params` holds the BARE name, so the prefix is added when
    /// this is built — the one place in the pass where the two conventions
    /// meet, and getting it wrong substitutes nothing while looking correct:
    /// the body is spliced with its parameters still free, and the caller then
    /// refers to a temp only the callee ever defined.
    params: Vec<String>,
    /// The whole body. ONE block takes the splice-in-place path below; several
    /// take the CFG path, which splits the caller's block around the call.
    blocks: Vec<IRBlock>,
    /// Its declared return type, compared against the CALL's to decide whether
    /// the result can be renamed in place or has to go through an `Assign`.
    ret_ty: IRType,
    /// Every temp the body defines, across every block — everything that needs
    /// a fresh name at a call site.
    defs: HashSet<String>,
}

impl Candidate {
    /// Does any `Return` in this body carry a value?
    fn returns_a_value(&self) -> bool {
        self.blocks
            .iter()
            .any(|b| matches!(&b.term, IRTerminator::Return(Some(_))))
    }

    /// The one block, for the single-block path.
    fn only_block(&self) -> &IRBlock {
        &self.blocks[0]
    }

    /// The value the single-block body returns, if any.
    fn only_ret(&self) -> Option<&IRValue> {
        match &self.only_block().term {
            IRTerminator::Return(r) => r.as_ref(),
            _ => None,
        }
    }

    /// Instructions across the whole body — what the growth budget is charged in.
    fn size(&self) -> usize {
        self.blocks.iter().map(|b| b.instrs.len()).sum()
    }
}

/// Splice every call to a small single-block function, with the default limit.
pub fn run(module: &mut IRModule) -> usize {
    run_with(module, SIZE_LIMIT)
}

/// The same, with an explicit size limit. `limit == 0` disables the pass.
pub fn run_with(module: &mut IRModule, limit: usize) -> usize {
    if limit == 0 {
        return 0;
    }
    let candidates = collect(module, limit);
    if candidates.is_empty() {
        return 0;
    }

    // The growth budget, charged per splice and shared across the module. See
    // GROWTH_PERCENT above: a per-splice size limit bounds one splice and says
    // nothing about their number.
    let original: usize = module
        .functions
        .iter()
        .filter(|f| !f.is_extern)
        .map(|f| f.blocks.iter().map(|b| b.instrs.len()).sum::<usize>())
        .sum();
    let budget = std::cmp::max(GROWTH_FLOOR, original * GROWTH_PERCENT / 100);
    let mut added = 0usize;

    let mut inlined = 0usize;
    for func in &mut module.functions {
        if func.is_extern {
            continue;
        }
        // A function is never spliced into itself. `collect` already refuses a
        // callee that calls itself, but a candidate can still be the function
        // being rewritten right now — inlining it here would be correct and
        // pointless, and it makes the pass's effect depend on iteration order.
        for bi in 0..func.blocks.len() {
            let mut ii = 0;
            while ii < func.blocks[bi].instrs.len() {
                let Some((callee, args, dst, call_ret_ty)) =
                    call_parts(&func.blocks[bi].instrs[ii])
                else {
                    ii += 1;
                    continue;
                };
                if callee == func.name {
                    ii += 1;
                    continue;
                }
                let Some(c) = candidates.get(&callee) else {
                    ii += 1;
                    continue;
                };
                // The CFG path, below, takes the multi-block callees.
                if c.blocks.len() != 1 {
                    ii += 1;
                    continue;
                }
                if c.params.len() != args.len() {
                    // A mismatch is a malformed module, not something to
                    // paper over by binding what happens to line up.
                    ii += 1;
                    continue;
                }
                if dst.is_some() && c.only_ret().is_none() {
                    ii += 1;
                    continue;
                }

                // The budget is charged the callee's SIZE rather than the
                // spliced length, so the two paths charge the same thing and
                // the reckoning does not depend on which one ran.
                if added + c.size() > budget {
                    ii += 1;
                    continue;
                }

                let body = splice(c, &args, dst.as_deref(), &call_ret_ty, inlined);
                let n = body.len();
                added += c.size();
                func.blocks[bi].instrs.splice(ii..=ii, body);
                ii += n;
                inlined += 1;
            }
        }

        // ------------------------------------------------------------------
        // The CFG path: multi-block callees.
        //
        // One site at a time, re-scanning after each, because the surgery
        // invalidates every block index in the function.
        //
        // TERMINATION, and it is the reason `synthetic` exists. A copied body
        // may itself contain a call to another candidate; re-scanning would
        // then splice that too, and a pair of mutually recursive callees —
        // neither of which `collect` refuses, since neither calls ITSELF —
        // would expand without bound. So the blocks this pass CREATES from a
        // callee body are never scanned again, which makes inlining depth-1
        // exactly as the single-block path's `ii += n` already does.
        //
        // The CONTINUATION block is deliberately not marked: it holds the
        // caller's own instructions, the ones that followed the call, and a
        // call among them is as eligible as it was before the split.
        let mut synthetic: HashSet<String> = HashSet::new();
        while let Some((bi, ii)) = next_cfg_site(func, &candidates, &synthetic, budget - added) {
            let cost = call_parts(&func.blocks[bi].instrs[ii])
                .and_then(|(callee, ..)| candidates.get(&callee).map(|c| c.size()))
                .unwrap_or(0);
            splice_cfg(func, bi, ii, &candidates, inlined, &mut synthetic);
            added += cost;
            inlined += 1;
        }
    }
    inlined
}

/// The first multi-block call site eligible for the CFG path, in block order.
fn next_cfg_site(
    func: &IRFunction,
    candidates: &HashMap<String, Candidate>,
    synthetic: &HashSet<String>,
    remaining: usize,
) -> Option<(usize, usize)> {
    for (bi, block) in func.blocks.iter().enumerate() {
        if synthetic.contains(&block.label) {
            continue;
        }
        for (ii, instr) in block.instrs.iter().enumerate() {
            let Some((callee, args, dst, call_ret_ty)) = call_parts(instr) else {
                continue;
            };
            if callee == func.name {
                continue;
            }
            let Some(c) = candidates.get(&callee) else { continue };
            if c.blocks.len() < 2 || c.params.len() != args.len() {
                continue;
            }
            if c.size() > remaining {
                continue;
            }
            // The result is carried by a phi in the continuation, and a phi has
            // ONE type. Where the call's type and the callee's disagree the
            // single-block path inserts a coercing `Assign`; doing the same
            // here would mean an `Assign` per returning block, so decline
            // instead — it is P13's territory and it is rare.
            if format!("{:?}", c.ret_ty) != format!("{:?}", call_ret_ty) {
                continue;
            }
            if dst.is_some() && !c.returns_a_value() {
                continue;
            }
            return Some((bi, ii));
        }
    }
    None
}

/// `(callee, args, dst, ret_ty)` if this instruction is a direct call.
fn call_parts(instr: &IRInstr) -> Option<(String, Vec<IRValue>, Option<String>, IRType)> {
    match instr {
        IRInstr::Call { dst, func, args, ret_ty } => Some((
            func.clone(),
            args.clone(),
            dst.as_ref().map(|d| d.0.clone()),
            ret_ty.clone(),
        )),
        _ => None,
    }
}

/// Every function small enough, simple enough and safe enough to splice.
fn collect(module: &IRModule, limit: usize) -> HashMap<String, Candidate> {
    let mut out = HashMap::new();
    for f in &module.functions {
        if f.is_extern || f.blocks.is_empty() {
            continue;
        }
        if backend_may_implement(&f.name) {
            continue;
        }
        // A single-block callee must end in a `Return`; a multi-block one is
        // allowed any mix of terminators, and the `Return`s are what become
        // jumps to the continuation.
        if f.blocks.len() == 1 && !matches!(f.blocks[0].term, IRTerminator::Return(_)) {
            continue;
        }
        // A body with no `Return` anywhere never comes back. Splicing it is
        // correct — the continuation is simply unreachable — but it is a
        // strange thing to do quietly, and the caller's code after the call is
        // dead either way, so leave the call alone and let it diverge.
        if !f.blocks.iter().any(|b| matches!(b.term, IRTerminator::Return(_))) {
            continue;
        }
        let size: usize = f.blocks.iter().map(|b| b.instrs.len()).sum();
        if size > limit {
            continue;
        }

        // **A callee that CONTAINS A LOOP is refused, and the reason is not
        // the loop — it is what a constant argument does to one.**
        //
        // Splicing binds each parameter to the argument VALUE, which is the
        // whole benefit of running before constant folding: a constant
        // argument makes the body foldable. Inside a loop it inverts. A loop
        // bound that was a PARAMETER lives in a register and is compared once
        // per iteration; substituted, it is a LITERAL, and the T3 backend has
        // no loop-invariant code motion, so it re-materialises it with `TLIT`
        // every single iteration. Measured on a three-call repro whose callee
        // is a 60-iteration loop: 2,274 → 2,439 instructions, **+7.3 %**, with
        // `TLIT +180` against 183 iterations and every other opcode moving by
        // exactly the three call frames saved.
        //
        // The emitted loop shows it directly. The condition block of the same
        // `while`, un-spliced and spliced:
        //
        //     accum_while_cond4:         main_il0_while_cond4:
        //                                  TLIT  R8, #60   <- EVERY ITERATION
        //       TCMP  R6, R4, R1           TCMP  R7, R5, R8
        //       TNEG  R6, R6               TNEG  R7, R7
        //       ...                        ...
        //
        // Un-spliced the bound is R1, the PARAMETER, already in a register.
        //
        // The same callee with an OPAQUE argument is −9. So the cost is the
        // constant, not the loop and not register pressure — which is why the
        // limit sweep looked like a size effect and was not one: at limit 12
        // `fibonacci` splices `fib_iterative`, a `while` loop, at 5 sites for
        // +1,098, and the curve is FLAT on either side of that one step.
        //
        // A branch-only callee called 180 times FROM a loop is −13.6 % on the
        // matching repro, so the refusal is deliberately about a loop in the
        // CALLEE and says nothing about the caller.
        //
        // What this gives up is the opaque-argument case, worth about 3
        // instructions per site. The principled repair is loop-invariant
        // literal hoisting in the T3 backend (report.txt P36), which would pay
        // for far more than this pass; until then a shape the optimiser cannot
        // clean up after is a shape not to make.
        let cfg = super::ssa::Cfg::of(f);
        let doms = super::ssa::Dominators::of(&cfg);
        let has_back_edge = (0..f.blocks.len())
            .any(|u| cfg.succs[u].iter().any(|&v| doms.dominates(v, u)));
        if has_back_edge {
            continue;
        }

        // Refused, and each for its own reason:
        //
        // * `Alloca` — the callee's frame cell becomes the CALLER's, and a
        //   call site inside a loop would then allocate once per iteration.
        //   On LLVM that is unbounded stack growth. Hoisting it to the entry
        //   block is the standard answer and it is a separate change.
        // * a call to ITSELF — one pass over the module cannot loop, but a
        //   self-recursive body is not a thing to duplicate on principle.
        //
        // `Phi` is refused only in a SINGLE-block body, where it is malformed
        // to begin with: an entry block has no predecessors. In a multi-block
        // body a phi is ordinary, and copying one is just a matter of renaming
        // the labels its arms name — which is the whole of the delicacy in the
        // CFG path below.
        let refuse = f.blocks.iter().enumerate().any(|(bi, b)| {
            b.instrs.iter().any(|i| {
                matches!(i, IRInstr::Alloca { .. })
                    || (bi == 0
                        && f.blocks.len() == 1
                        && matches!(i, IRInstr::Phi { .. }))
                    || matches!(i, IRInstr::Call { func, .. } if *func == f.name)
            })
        });
        if refuse {
            continue;
        }

        let defs: HashSet<String> = f
            .blocks
            .iter()
            .flat_map(|b| b.instrs.iter())
            .filter_map(super::ssa::instr_def)
            .map(str::to_string)
            .collect();

        out.insert(
            f.name.clone(),
            Candidate {
                params: f.params.iter().map(|(n, _)| format!("param_{}", n)).collect(),
                blocks: f.blocks.clone(),
                ret_ty: f.ret_ty.clone(),
                defs,
            },
        );
    }
    out
}

/// The callee's body, renamed for one call site.
///
/// Every temp it defines gets a fresh name and every parameter is replaced by
/// the argument VALUE, so the result is still in SSA form and cannot collide
/// with anything the caller already has — including the caller's own
/// `param_*`, which is why the parameters are substituted rather than assigned.
fn splice(
    c: &Candidate,
    args: &[IRValue],
    dst: Option<&str>,
    call_ret_ty: &IRType,
    site: usize,
) -> Vec<IRInstr> {
    let prefix = format!("il{}_", site);

    let mut renames: HashMap<String, IRValue> = HashMap::new();
    for d in &c.defs {
        renames.insert(d.clone(), IRValue::Temp(IRTemp::new(format!("{}{}", prefix, d))));
    }
    for (p, a) in c.params.iter().zip(args) {
        // A parameter that the body also defines would be two values under one
        // name, which SSA forbids; the argument wins, and `defs` cannot contain
        // a parameter because a parameter is not defined by an instruction.
        renames.insert(p.clone(), a.clone());
    }

    // The RESULT can usually be renamed straight onto the call's destination,
    // which costs nothing. It cannot when the two types differ — the backends
    // coerce on an `Assign` and would not coerce on a rename — and it cannot
    // when what is returned is a constant or an argument rather than something
    // the body computed.
    let ret = c.only_ret().cloned();
    let returns_own_temp = match &ret {
        Some(IRValue::Temp(t)) => c.defs.contains(&t.0),
        _ => false,
    };
    let same_ty = format!("{:?}", c.ret_ty) == format!("{:?}", call_ret_ty);
    let rename_result = matches!((dst, returns_own_temp, same_ty), (Some(_), true, true));
    if rename_result {
        if let (Some(d), Some(IRValue::Temp(t))) = (dst, &ret) {
            renames.insert(t.0.clone(), IRValue::Temp(IRTemp::new(d.to_string())));
        }
    }

    let mut out = Vec::with_capacity(c.only_block().instrs.len() + 1);
    for instr in &c.only_block().instrs {
        let mut i = instr.clone();
        rename_def(&mut i, &renames);
        // `include_phi` is irrelevant on THIS path — `collect` refuses a phi
        // in a single-block body, where it would be malformed anyway — but the
        // walker is shared with the CFG path, where phis are ordinary, and
        // asking for them is what renaming means.
        super::optimize::for_each_operand_mut(&mut i, true, |v| substitute(v, &renames));
        out.push(i);
    }

    if let (Some(d), false) = (dst, rename_result) {
        if let Some(ret) = &ret {
            let mut v = ret.clone();
            substitute(&mut v, &renames);
            out.push(IRInstr::Assign {
                dst: IRTemp::new(d.to_string()),
                src: v,
                ty: call_ret_ty.clone(),
            });
        }
    }
    out
}

/// Rewrite the temp an instruction DEFINES. Operands are handled separately,
/// by the shared walker; this is the other half.
fn rename_def(instr: &mut IRInstr, renames: &HashMap<String, IRValue>) {
    let slot: Option<&mut IRTemp> = match instr {
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
        | IRInstr::Cast { dst, .. } => Some(dst),
        IRInstr::Call { dst, .. } | IRInstr::CallIndirect { dst, .. } => dst.as_mut(),
        IRInstr::Store { .. }
        | IRInstr::BoundsCheck { .. }
        | IRInstr::PrintStr(_)
        | IRInstr::PrintInt(_)
        | IRInstr::PrintFloat(_)
        | IRInstr::PrintBool3(_)
        | IRInstr::PrintTrit(_) => None,
    };
    if let Some(t) = slot {
        if let Some(IRValue::Temp(new)) = renames.get(&t.0) {
            *t = new.clone();
        }
    }
}

fn substitute(val: &mut IRValue, renames: &HashMap<String, IRValue>) {
    if let IRValue::Temp(t) = val {
        if let Some(r) = renames.get(&t.0) {
            *val = r.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// The CFG path — multi-block callees
// ---------------------------------------------------------------------------

/// Splice a multi-block callee at `func.blocks[bi].instrs[ii]`.
///
/// The shape, and every piece of it is forced:
///
/// ```text
///   before                    after
///   ┌─────────────┐           ┌─────────────┐
///   │ A: ...      │           │ A: ...      │   the instructions before the call
///   │    CALL f   │           │    JUMP f'  │   ← A's terminator is replaced
///   │    ...      │           └─────────────┘
///   │    <term>   │           ┌─────────────┐
///   └─────────────┘           │ f' ... f'n  │   the callee's blocks, renamed
///                             │   RET v ⇒   │   every Return becomes
///                             │   JUMP C    │   a jump to the continuation
///                             └─────────────┘
///                             ┌─────────────┐
///                             │ C: phi ←    │   one arm per returning block
///                             │    ...      │   the instructions after the call
///                             │    <term>   │   ← A's old terminator
///                             └─────────────┘
/// ```
///
/// **The phi is the whole of the difficulty, and it is the shape this project
/// has already got wrong three times** (report.txt P11, P12, P14): a phi's
/// value belongs to the EDGE it arrived on, not to the phi. Each arm is
/// therefore keyed on the RENAMED label of the returning block — not the
/// callee's original label, and not the caller's block — because that is the
/// block the value will actually flow out of.
///
/// **No critical edge is created, so `split_critical_edges` does not need to
/// run again.** Every predecessor of `C` is a former `Return` block, and a
/// `Return` has no successors, so after the rewrite each ends in a plain
/// `Jump` with `C` as its only successor. `A` likewise ends in a plain `Jump`
/// to the entry copy, which has no other predecessor. The callee's own blocks
/// were split before `mem2reg` ran and are copied unchanged.
fn splice_cfg(
    func: &mut IRFunction,
    bi: usize,
    ii: usize,
    candidates: &HashMap<String, Candidate>,
    site: usize,
    synthetic: &mut HashSet<String>,
) {
    let (callee, args, dst, call_ret_ty) = call_parts(&func.blocks[bi].instrs[ii])
        .expect("next_cfg_site returned a non-call");
    let c = &candidates[&callee];
    let prefix = format!("il{}_", site);
    let cont_label = format!("{}cont", prefix);

    // ---- 1. the rename map: temps get fresh names, parameters get arguments
    let mut renames: HashMap<String, IRValue> = HashMap::new();
    for d in &c.defs {
        renames.insert(d.clone(), IRValue::Temp(IRTemp::new(format!("{}{}", prefix, d))));
    }
    for (p, a) in c.params.iter().zip(&args) {
        renames.insert(p.clone(), a.clone());
    }
    let label_of = |l: &str| format!("{}{}", prefix, l);

    // ---- 2. split the caller's block around the call
    let mut tail = func.blocks[bi].instrs.split_off(ii);
    tail.remove(0); // the CALL itself
    let old_term = std::mem::replace(
        &mut func.blocks[bi].term,
        IRTerminator::Jump(label_of(&c.blocks[0].label)),
    );
    let caller_label = func.blocks[bi].label.clone();

    // ---- 3. copy the callee's blocks, rewriting names, labels and returns
    let mut copies: Vec<IRBlock> = Vec::with_capacity(c.blocks.len());
    let mut arms: Vec<(IRValue, String)> = Vec::new();
    for cb in &c.blocks {
        let new_label = label_of(&cb.label);
        let mut instrs = Vec::with_capacity(cb.instrs.len());
        for instr in &cb.instrs {
            let mut i = instr.clone();
            rename_def(&mut i, &renames);
            super::optimize::for_each_operand_mut(&mut i, true, |v| substitute(v, &renames));
            // A phi inside the callee names the callee's OWN labels; they have
            // all just been renamed underneath it.
            if let IRInstr::Phi { incoming, .. } = &mut i {
                for (_, l) in incoming.iter_mut() {
                    *l = label_of(l);
                }
            }
            instrs.push(i);
        }
        let term = match &cb.term {
            IRTerminator::Return(v) => {
                if let Some(v) = v {
                    let mut v = v.clone();
                    substitute(&mut v, &renames);
                    arms.push((v, new_label.clone()));
                }
                IRTerminator::Jump(cont_label.clone())
            }
            // **A terminator has OPERANDS as well as labels**, and they are
            // the callee's temps like any other: `BinBranch` and `TritBranch`
            // each test a value. Renaming only the labels leaves the condition
            // naming a temp that only the ORIGINAL callee defines — and the
            // copy that defines it under its new name is then used by nobody,
            // so dead-code elimination removes it and the branch is left
            // reading a temp nothing writes. report.txt P34, and it is P29's
            // failure mode exactly: the splice is well-formed, the site count
            // is right, and on T3 the free temp gets a register and the
            // program prints something plausible.
            other => {
                let mut t = retarget_term(other, &label_of);
                super::optimize::for_each_term_operand_mut(&mut t, |v| substitute(v, &renames));
                t
            }
        };
        synthetic.insert(new_label.clone());
        copies.push(IRBlock { label: new_label, instrs, term });
    }

    // ---- 4. the continuation, carrying the result and the rest of the caller
    //
    // **`IRValue::Void` is not a value the backends can name**, and a
    // value-returning function reaches `Return(Some(Void))` more often than it
    // sounds: the lowerer gives an exhaustive `match` a trailing
    // `match_nextN` block that no arm matched, and that block returns Void.
    // On a `ret` both backends coerce it — LLVM emits `ret ptr null` — but a
    // PHI ARM gets no such coercion, and LLVM rendered the arm as the empty
    // string: `phi ptr [ %t8, %arm1 ], ..., [ , %match_next8 ]`, which is not
    // parseable. The lowerer has had `sanitize_phi_incoming` for exactly this
    // since it started emitting phis; the inliner is the only OTHER producer
    // of phis in the compiler and did not use it. report.txt P35.
    let arms = super::lower::helpers::sanitize_phi_incoming(arms, &call_ret_ty);

    let mut cont_instrs: Vec<IRInstr> = Vec::with_capacity(tail.len() + 1);
    if let Some(d) = &dst {
        match arms.len() {
            0 => {}
            // One returning block is not a join, and a one-armed phi would be
            // a copy written the hard way — `merge_blocks` would turn it back
            // into this anyway when it absorbs the block.
            1 => cont_instrs.push(IRInstr::Assign {
                dst: IRTemp::new(d.clone()),
                src: arms[0].0.clone(),
                ty: call_ret_ty.clone(),
            }),
            _ => cont_instrs.push(IRInstr::Phi {
                dst: IRTemp::new(d.clone()),
                ty: call_ret_ty.clone(),
                incoming: arms.clone(),
            }),
        }
    }
    cont_instrs.extend(tail);
    let cont = IRBlock { label: cont_label.clone(), instrs: cont_instrs, term: old_term };

    // ---- 5. every phi that named the caller's block on an edge the caller no
    // longer supplies now takes that value from the CONTINUATION.
    //
    // The caller's block used to end in `old_term`; it now ends in a jump to
    // the callee, and `old_term` has moved to the continuation. So a downstream
    // phi that named the caller is naming a block that no longer reaches it.
    // Exactly the same correction `merge_blocks` makes, for the same reason,
    // and forgetting it leaves a phi pointing at a predecessor that is no
    // longer one.
    for b in func.blocks.iter_mut() {
        for instr in b.instrs.iter_mut() {
            if let IRInstr::Phi { incoming, .. } = instr {
                for (_, l) in incoming.iter_mut() {
                    if *l == caller_label {
                        *l = cont_label.clone();
                    }
                }
            }
        }
    }

    // ---- 6. insert, keeping the entry block at index 0
    let mut at = bi + 1;
    for b in copies {
        func.blocks.insert(at, b);
        at += 1;
    }
    func.blocks.insert(at, cont);
}

/// A terminator with every label it names put through `f`.
fn retarget_term(term: &IRTerminator, f: &dyn Fn(&str) -> String) -> IRTerminator {
    match term {
        IRTerminator::Jump(l) => IRTerminator::Jump(f(l)),
        IRTerminator::BinBranch { cond, true_label, false_label } => IRTerminator::BinBranch {
            cond: cond.clone(),
            true_label: f(true_label),
            false_label: f(false_label),
        },
        IRTerminator::TritBranch { cond, pos_label, zero_label, neg_label } => {
            IRTerminator::TritBranch {
                cond: cond.clone(),
                pos_label: f(pos_label),
                zero_label: f(zero_label),
                neg_label: f(neg_label),
            }
        }
        IRTerminator::Return(v) => IRTerminator::Return(v.clone()),
        IRTerminator::Unreachable => IRTerminator::Unreachable,
    }
}
