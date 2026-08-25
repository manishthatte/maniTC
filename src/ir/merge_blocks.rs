//! P26 — block merging.
//!
//! Measured across the 17 examples before this existed:
//!
//! | | |
//! |---|---|
//! | blocks | 14,857 |
//! | mean block length | **2.78 instructions** |
//! | holding 0 or 1 instruction | 9,769 — 65.8 % |
//! | **empty, terminator a plain `Jump`** | **4,119 — 27.7 %** |
//!
//! That is why the mean block is under three instructions, and a block-scoped
//! pass in an IR shaped like this has nowhere to look — it is the finding that
//! came out of re-scoping CSE to dominance (report.txt P22).
//!
//! ## Why this did not conflict with `split_critical_edges` after all
//!
//! P26 was left open as "a design question, not a patch", because
//! `ssa::split_critical_edges` deliberately INSERTS empty blocks — a phi's
//! value has to be placed on the edge it came from, and a critical edge has
//! nowhere to put it. A pass that merges empty blocks away looks like it would
//! undo exactly that.
//!
//! **It cannot, and the reason is structural rather than a matter of ordering.**
//! The merge condition is: `A` ends in a plain `Jump` to `B`, and `A` is `B`'s
//! only predecessor. Edge splitting inserts `B` on an edge `P → S` where `P`
//! has SEVERAL successors — so `P` ends in a branch, never in a plain `Jump`,
//! and the pair `(P, B)` does not match. The other end, `B → S`, does not match
//! either: `S` has several predecessors, which is what made the edge critical.
//! **The two passes operate on disjoint shapes.**
//!
//! The one case where a split block does get merged is the one where it should:
//! if a later pass folds `P`'s branch to a single target, `P` ends in a `Jump`,
//! the edge is no longer critical, and the block it needed no longer has a job.
//!
//! ## And it preserves the property splitting exists to establish
//!
//! Merging `B` into `A` gives `A` the successors of `B`. If `B` had more than
//! one successor then, splitting having already run, each of those successors
//! has exactly one predecessor — so none of the new edges out of `A` is
//! critical. If `B` had one successor, `A` ends with one successor. Either way
//! no critical edge is created, and every successor's predecessor COUNT is
//! unchanged: `B` is replaced by `A` in the set, not added to it.

use std::collections::HashMap;

use super::types::*;

/// Merge every block into its single predecessor, module-wide.
/// Returns the number of merges performed.
pub fn run(module: &mut IRModule) -> usize {
    let mut n = 0;
    for f in &mut module.functions {
        if !f.is_extern {
            n += run_func(f);
        }
    }
    n
}

/// The same for one function, to a fixpoint.
///
/// A chain `A -> B -> C` collapses in two merges: the first makes `A` end in
/// `B`'s `Jump` to `C`, which is then the same shape again. Each merge removes
/// one block, so this terminates in at most `blocks.len()` steps.
pub fn run_func(func: &mut IRFunction) -> usize {
    let mut merged = 0;
    while let Some((ai, li)) = next_pair(func) {
        merge(func, ai, li);
        merged += 1;
    }
    merged
}

/// The first `(predecessor, successor)` pair that may be merged, in block
/// order so the result does not depend on hash iteration.
fn next_pair(func: &IRFunction) -> Option<(usize, usize)> {
    if func.blocks.len() < 2 {
        return None;
    }
    let index: HashMap<&str, usize> = func
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.label.as_str(), i))
        .collect();

    // Predecessor counts, from terminators. Phi incoming labels are NOT counted
    // here: a phi names the edge a value arrived on, and every such edge is
    // already named by the terminator at the other end of it. Counting both
    // would double-count every phi predecessor and merge nothing.
    let mut preds = vec![0usize; func.blocks.len()];
    for b in &func.blocks {
        for target in successors(&b.term) {
            if let Some(&t) = index.get(target) {
                preds[t] += 1;
            }
        }
    }

    for (ai, a) in func.blocks.iter().enumerate() {
        let IRTerminator::Jump(target) = &a.term else { continue };
        let Some(&li) = index.get(target.as_str()) else { continue };
        // The entry block keeps its position and its role; a self-loop is not a
        // pair of blocks at all.
        if li == 0 || li == ai {
            continue;
        }
        if preds[li] != 1 {
            continue;
        }
        // A phi in a single-predecessor block is a phi with one arm, which is
        // just a copy — but if it says anything else the block is malformed and
        // this pass is not the place to find out.
        let phis_are_sound = func.blocks[li].instrs.iter().all(|i| match i {
            IRInstr::Phi { incoming, .. } => {
                incoming.len() == 1 && incoming[0].1 == a.label
            }
            _ => true,
        });
        if !phis_are_sound {
            continue;
        }
        return Some((ai, li));
    }
    None
}

/// Every label a terminator names.
fn successors(term: &IRTerminator) -> Vec<&str> {
    match term {
        IRTerminator::Jump(l) => vec![l.as_str()],
        IRTerminator::BinBranch { true_label, false_label, .. } => {
            vec![true_label.as_str(), false_label.as_str()]
        }
        IRTerminator::TritBranch { pos_label, zero_label, neg_label, .. } => {
            vec![pos_label.as_str(), zero_label.as_str(), neg_label.as_str()]
        }
        IRTerminator::Return(_) | IRTerminator::Unreachable => Vec::new(),
    }
}

/// Append `li`'s body and terminator to `ai`, and delete `li`.
fn merge(func: &mut IRFunction, ai: usize, li: usize) {
    let l = func.blocks.remove(li);
    // `remove` shifts everything after `li` down by one.
    let ai = if li < ai { ai - 1 } else { ai };
    let a_label = func.blocks[ai].label.clone();

    for instr in l.instrs {
        match instr {
            // One arm, so the phi IS the copy. `next_pair` has already checked
            // that the arm comes from this predecessor.
            IRInstr::Phi { dst, ty, incoming } if incoming.len() == 1 => {
                func.blocks[ai].instrs.push(IRInstr::Assign {
                    dst,
                    src: incoming[0].0.clone(),
                    ty,
                });
            }
            other => func.blocks[ai].instrs.push(other),
        }
    }
    func.blocks[ai].term = l.term;

    // Every phi downstream took its value on an edge FROM the block that has
    // just stopped existing. Missing this is the whole of the danger here: the
    // phi still has a value and still has an arm, so nothing is obviously
    // wrong, and the backend then looks for a predecessor by a name no block
    // carries — the phi-on-its-edge mistake that cost P11, P12 and P14.
    for b in &mut func.blocks {
        for instr in &mut b.instrs {
            if let IRInstr::Phi { incoming, .. } = instr {
                for (_, label) in incoming.iter_mut() {
                    if *label == l.label {
                        *label = a_label.clone();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blk(label: &str, instrs: Vec<IRInstr>, term: IRTerminator) -> IRBlock {
        IRBlock { label: label.to_string(), instrs, term }
    }

    fn func(blocks: Vec<IRBlock>) -> IRFunction {
        IRFunction {
            name: "f".into(),
            params: Vec::new(),
            ret_ty: IRType::I64,
            blocks,
            is_extern: false,
        }
    }

    fn assign(dst: &str) -> IRInstr {
        IRInstr::Assign {
            dst: IRTemp::new(dst.to_string()),
            src: IRValue::Const(IRConst::Int(1)),
            ty: IRType::I64,
        }
    }

    /// A chain collapses to one block, in order.
    #[test]
    fn a_chain_collapses() {
        let mut f = func(vec![
            blk("entry", vec![assign("a")], IRTerminator::Jump("mid".into())),
            blk("mid", vec![assign("b")], IRTerminator::Jump("tail".into())),
            blk("tail", vec![assign("c")], IRTerminator::Return(None)),
        ]);
        assert_eq!(run_func(&mut f), 2);
        assert_eq!(f.blocks.len(), 1);
        assert_eq!(f.blocks[0].label, "entry");
        assert_eq!(f.blocks[0].instrs.len(), 3);
        assert!(matches!(f.blocks[0].term, IRTerminator::Return(None)));
    }

    /// A block with two predecessors is a join and stays one.
    #[test]
    fn a_join_is_not_merged() {
        let mut f = func(vec![
            blk("entry", vec![], IRTerminator::BinBranch {
                cond: IRValue::Const(IRConst::Int(1)),
                true_label: "l".into(),
                false_label: "r".into(),
            }),
            blk("l", vec![assign("a")], IRTerminator::Jump("join".into())),
            blk("r", vec![assign("b")], IRTerminator::Jump("join".into())),
            blk("join", vec![assign("c")], IRTerminator::Return(None)),
        ]);
        assert_eq!(run_func(&mut f), 0);
        assert_eq!(f.blocks.len(), 4);
    }

    /// **The one that matters: a phi downstream is re-pointed at the survivor.**
    /// `mid` disappears into `entry`, and the phi in `join` must stop naming it.
    #[test]
    fn a_downstream_phi_follows_the_merge() {
        let mut f = func(vec![
            blk("entry", vec![], IRTerminator::Jump("mid".into())),
            blk("mid", vec![assign("a")], IRTerminator::BinBranch {
                cond: IRValue::Const(IRConst::Int(1)),
                true_label: "join".into(),
                false_label: "other".into(),
            }),
            blk("other", vec![], IRTerminator::Jump("join".into())),
            blk("join", vec![IRInstr::Phi {
                dst: IRTemp::new("p"),
                ty: IRType::I64,
                incoming: vec![
                    (IRValue::Const(IRConst::Int(1)), "mid".into()),
                    (IRValue::Const(IRConst::Int(2)), "other".into()),
                ],
            }], IRTerminator::Return(None)),
        ]);
        assert_eq!(run_func(&mut f), 1);
        let IRInstr::Phi { incoming, .. } = &f.blocks[2].instrs[0] else {
            panic!("phi gone");
        };
        assert_eq!(incoming[0].1, "entry", "phi still names the merged-away block");
        assert_eq!(incoming[1].1, "other");
    }

    /// A one-armed phi in the merged block becomes the copy it always was.
    #[test]
    fn a_one_armed_phi_becomes_an_assign() {
        let mut f = func(vec![
            blk("entry", vec![], IRTerminator::Jump("mid".into())),
            blk("mid", vec![IRInstr::Phi {
                dst: IRTemp::new("p"),
                ty: IRType::I64,
                incoming: vec![(IRValue::Const(IRConst::Int(7)), "entry".into())],
            }], IRTerminator::Return(None)),
        ]);
        assert_eq!(run_func(&mut f), 1);
        assert_eq!(f.blocks.len(), 1);
        assert!(
            matches!(&f.blocks[0].instrs[0], IRInstr::Assign { dst, .. } if dst.0 == "p"),
            "the phi should have become an Assign: {:?}", f.blocks[0].instrs[0]
        );
    }

    /// **A self-looping block is never merged, and the reason is load-bearing.**
    ///
    /// `loop` has TWO predecessors — `entry` and itself — so the
    /// single-predecessor test refuses it without needing a special case. That
    /// refusal is what keeps the pass sound here: merging `loop` into `entry`
    /// would leave `entry` ending in `Jump("loop")` while no block called
    /// `loop` existed any more, and a terminator naming a label nothing defines
    /// is exactly the `Cfg::dangling` case the rest of the compiler treats as
    /// malformed.
    #[test]
    fn a_self_loop_is_left_alone() {
        let mut f = func(vec![
            blk("entry", vec![], IRTerminator::Jump("loop".into())),
            blk("loop", vec![], IRTerminator::Jump("loop".into())),
        ]);
        assert_eq!(run_func(&mut f), 0, "a self-loop is not a mergeable pair");
        assert_eq!(f.blocks.len(), 2);
    }
}
