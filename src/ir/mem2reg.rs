//! F-1 — `mem2reg`: lift local variables out of memory into SSA values.
//!
//! ## What F-1 actually is, corrected
//!
//! The recommendation says "the current IR is not SSA". Measured with
//! `ssa::verify` over 18 programs — the 17 examples and thatteOS, 1,806
//! function lowerings, 18,828 blocks, 79,953 instructions — the IR has **zero**
//! SSA violations, before and after the optimiser. Every temp is defined once,
//! every use is dominated by its definition, every phi is well formed. The IR
//! *is* SSA.
//!
//! It is SSA *trivially*, which is the real problem and is what the
//! recommendation was pointing at. Every local variable lives in an `Alloca`
//! and is reached by `Load` and `Store`, so no variable value ever flows
//! through a temp across a statement boundary and there is nothing for an
//! optimiser to see. The evidence F-1 cites — `optimize.rs` propagating
//! constants through a `HashMap` keyed by temp name — is not a workaround for
//! missing SSA; it is a workaround for values living in **memory**.
//!
//! The size of that, measured on the same corpus:
//!
//! | | |
//! |---|---|
//! | allocas | 9,455 |
//! | of those, promotable | 8,660 (91.5 %) |
//! | loads + stores | 42,008 |
//! | share of all IR instructions | **52.5 %** |
//!
//! More than half of the IR is moving locals to and from stack slots they
//! never needed to occupy. This pass removes them.
//!
//! ## The algorithm
//!
//! Cytron, Ferrante, Rosen, Wegman and Zadeck, in the standard two phases:
//!
//!   1. **Phi placement.** For each promotable alloca, put a phi at every
//!      block in the iterated dominance frontier of the blocks that store to
//!      it. That set is exactly where two different stored values can meet.
//!   2. **Renaming.** Walk the dominator tree with a stack of the current
//!      value of each alloca. A store pushes; a load is deleted and every use
//!      of its result is rewritten to the value on top; a phi pushes its own
//!      destination. On the way out of a block, each successor's phis are
//!      given this block's current value on that edge.
//!
//! ## What it deliberately does not do
//!
//! An alloca whose address escapes is left alone — passed to a call, indexed
//! with `GetPtr`, or stored as a value rather than used as a pointer. See
//! `ssa::promotable_allocas`, which is the whole of the safety argument and is
//! tested separately.
//!
//! An uninitialised read becomes a typed zero rather than whatever the stack
//! slot happened to hold. That is a change, and it is an improvement: today
//! such a read is a genuine cross-backend divergence, because the T3 emulator
//! zeroes its memory and an LLVM `alloca` does not. A deterministic zero makes
//! the two agree, and matches what `sanitize_phi_incoming` already does for
//! the same question on a merge edge.
//!
//! © Manish Jagdish Thatte

use std::collections::{HashMap, HashSet};

use super::ssa::{self, Cfg, Dominators};
use super::types::*;

/// What one run of the pass did, for measurement and for tests.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Promoted {
    /// Allocas lifted out of memory.
    pub allocas: usize,
    /// `Load` instructions deleted.
    pub loads: usize,
    /// `Store` instructions deleted.
    pub stores: usize,
    /// Phi nodes inserted.
    pub phis: usize,
}

impl Promoted {
    pub fn add(&mut self, o: &Promoted) {
        self.allocas += o.allocas;
        self.loads += o.loads;
        self.stores += o.stores;
        self.phis += o.phis;
    }

    /// Net instruction change: every load, store and alloca removed, less
    /// every phi added.
    pub fn net_removed(&self) -> i64 {
        (self.allocas + self.loads + self.stores) as i64 - self.phis as i64
    }
}

/// Run `mem2reg` over every non-extern function in a module.
pub fn run(module: &mut IRModule) -> Promoted {
    let mut total = Promoted::default();
    for f in &mut module.functions {
        if f.is_extern {
            continue;
        }
        total.add(&promote_function(f));
    }
    total
}

/// The zero of a type, used for a read of an alloca nothing has stored to on
/// this path.
fn zero_of(ty: &IRType) -> IRValue {
    match ty {
        IRType::F64 => IRValue::Const(IRConst::Float(0.0)),
        IRType::Bool => IRValue::Const(IRConst::Bool(false)),
        IRType::Trit => IRValue::Const(IRConst::Trit(0)),
        _ => IRValue::Const(IRConst::Int(0)),
    }
}

/// Promote every eligible alloca in one function. Returns what it did.
pub fn promote_function(func: &mut IRFunction) -> Promoted {
    let mut done = Promoted::default();
    if func.blocks.is_empty() {
        return done;
    }

    let cfg = Cfg::of(func);
    let doms = Dominators::of(&cfg);

    // A candidate with any use in an unreachable block is dropped. The walk
    // below only visits reachable blocks, so a load left behind in one would
    // refer to an alloca this pass has deleted — a dangling operand, which is
    // worse than an unpromoted variable. `optimize::dead_block_eliminate`
    // normally leaves none, and this is what makes that not a dependency.
    let unreachable: Vec<usize> = (0..func.blocks.len())
        .filter(|&b| !doms.is_reachable(b))
        .collect();
    let mut in_dead_code: HashSet<String> = HashSet::new();
    for &b in &unreachable {
        for instr in &func.blocks[b].instrs {
            if let Some(d) = ssa::instr_def(instr) {
                in_dead_code.insert(d.to_string());
            }
            for u in ssa::instr_uses(instr) {
                in_dead_code.insert(u.to_string());
            }
        }
    }

    let candidates: Vec<(String, IRType)> = ssa::promotable_allocas(func)
        .into_iter()
        .filter(|(name, _)| !in_dead_code.contains(name))
        .collect();
    if candidates.is_empty() {
        return done;
    }
    done.allocas = candidates.len();

    let ty_of: HashMap<&str, IRType> = candidates
        .iter()
        .map(|(n, t)| (n.as_str(), t.clone()))
        .collect();

    // ---- 1. where do the phis go ------------------------------------------
    let df = doms.frontiers(&cfg);

    // alloca → blocks that store to it. Owned keys: the phi insertion below
    // mutates `func.blocks`, so nothing here may borrow from it.
    let mut store_blocks: HashMap<String, Vec<usize>> = HashMap::new();
    for (bi, block) in func.blocks.iter().enumerate() {
        if !doms.is_reachable(bi) {
            continue;
        }
        for instr in &block.instrs {
            if let IRInstr::Store { ptr: IRValue::Temp(t), .. } = instr {
                if ty_of.contains_key(t.0.as_str()) {
                    let v = store_blocks.entry(t.0.clone()).or_default();
                    if !v.contains(&bi) {
                        v.push(bi);
                    }
                }
            }
        }
    }

    // (block, alloca) → the temp the phi defines there.
    let mut phi_at: HashMap<(usize, String), String> = HashMap::new();
    let mut next_temp = 0usize;
    let prefix = fresh_prefix(func);

    for (name, ty) in &candidates {
        // Iterated dominance frontier of the store blocks, by worklist.
        let mut worklist: Vec<usize> = store_blocks.get(name).cloned().unwrap_or_default();
        let mut placed: HashSet<usize> = HashSet::new();
        let mut ever_on_list: HashSet<usize> = worklist.iter().copied().collect();
        while let Some(b) = worklist.pop() {
            for &d in &df[b] {
                if !placed.insert(d) {
                    continue;
                }
                let dst = format!("{}{}", prefix, next_temp);
                next_temp += 1;
                func.blocks[d].instrs.insert(
                    0,
                    IRInstr::Phi {
                        dst: IRTemp::new(dst.clone()),
                        ty: ty.clone(),
                        // Filled during renaming, one operand per predecessor.
                        incoming: Vec::new(),
                    },
                );
                phi_at.insert((d, name.clone()), dst);
                done.phis += 1;
                // A phi is itself a definition, so the block joins the
                // worklist — this is what makes the frontier *iterated*.
                if ever_on_list.insert(d) {
                    worklist.push(d);
                }
            }
        }
    }

    // ---- 2. renaming -------------------------------------------------------
    //
    // Dominator-tree children, derived from idom.
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); func.blocks.len()];
    for b in 0..func.blocks.len() {
        if let Some(p) = doms.idom[b] {
            children[p].push(b);
        }
    }
    for c in children.iter_mut() {
        c.sort_unstable();
    }

    // alloca → stack of values currently held
    let mut stacks: HashMap<String, Vec<IRValue>> = candidates
        .iter()
        .map(|(n, _)| (n.clone(), Vec::new()))
        .collect();
    // load result temp → the value that replaces it
    let mut renames: HashMap<String, IRValue> = HashMap::new();

    // Explicit stack instead of recursion: a deep dominator tree would
    // otherwise recurse as far as the function nests.
    enum Step {
        Enter(usize),
        Leave(Vec<(String, usize)>),
    }
    let entry = doms.rpo.first().copied().unwrap_or(0);
    let mut work: Vec<Step> = vec![Step::Enter(entry)];

    while let Some(step) = work.pop() {
        match step {
            Step::Leave(pushed) => {
                for (name, n) in pushed {
                    let st = stacks.get_mut(&name).expect("stack for a candidate");
                    st.truncate(st.len() - n);
                }
            }
            Step::Enter(b) => {
                let mut pushed: HashMap<String, usize> = HashMap::new();

                // Phis defined here become the current value.
                for (name, _) in &candidates {
                    if let Some(dst) = phi_at.get(&(b, name.clone())) {
                        stacks
                            .get_mut(name)
                            .expect("stack")
                            .push(IRValue::Temp(IRTemp::new(dst.clone())));
                        *pushed.entry(name.clone()).or_insert(0) += 1;
                    }
                }

                let mut kept: Vec<IRInstr> = Vec::with_capacity(func.blocks[b].instrs.len());
                for instr in std::mem::take(&mut func.blocks[b].instrs) {
                    match instr {
                        // Dropped. `done.allocas` is the candidate count,
                        // recorded before the walk, so nothing to add here.
                        IRInstr::Alloca { ref dst, .. } if ty_of.contains_key(dst.0.as_str()) => {}
                        IRInstr::Store { ptr: IRValue::Temp(ref p), ref val, .. }
                            if ty_of.contains_key(p.0.as_str()) =>
                        {
                            let mut v = val.clone();
                            substitute(&mut v, &renames);
                            let name = p.0.clone();
                            stacks.get_mut(&name).expect("stack").push(v);
                            *pushed.entry(name).or_insert(0) += 1;
                            done.stores += 1;
                        }
                        IRInstr::Load { ref dst, ptr: IRValue::Temp(ref p), .. }
                            if ty_of.contains_key(p.0.as_str()) =>
                        {
                            let cur = stacks
                                .get(p.0.as_str())
                                .and_then(|s| s.last().cloned())
                                .unwrap_or_else(|| zero_of(&ty_of[p.0.as_str()]));
                            renames.insert(dst.0.clone(), cur);
                            done.loads += 1;
                        }
                        mut other => {
                            // Phi operands are NOT rewritten here: they are
                            // used on the incoming edge, whose block may not
                            // have been walked yet. A final pass below does
                            // them, once every rename is known.
                            if !matches!(other, IRInstr::Phi { .. }) {
                                substitute_in_instr(&mut other, &renames);
                            }
                            kept.push(other);
                        }
                    }
                }
                func.blocks[b].instrs = kept;
                substitute_in_term(&mut func.blocks[b].term, &renames);

                // Hand this block's current values to each successor's phis.
                let label = func.blocks[b].label.clone();
                for &s in &cfg.succs[b] {
                    for (name, ty) in &candidates {
                        let Some(dst) = phi_at.get(&(s, name.clone())) else {
                            continue;
                        };
                        let v = stacks
                            .get(name.as_str())
                            .and_then(|st| st.last().cloned())
                            .unwrap_or_else(|| zero_of(ty));
                        for instr in &mut func.blocks[s].instrs {
                            if let IRInstr::Phi { dst: d, incoming, .. } = instr {
                                if d.0 == *dst {
                                    incoming.push((v.clone(), label.clone()));
                                    break;
                                }
                            }
                        }
                    }
                }

                // Children after the leave marker, so the marker runs last.
                work.push(Step::Leave(pushed.into_iter().collect()));
                for &c in children[b].iter().rev() {
                    work.push(Step::Enter(c));
                }
            }
        }
    }

    // ---- 3. phi operands, now that every rename is known -------------------
    for block in &mut func.blocks {
        for instr in &mut block.instrs {
            if let IRInstr::Phi { incoming, .. } = instr {
                for (v, _) in incoming.iter_mut() {
                    substitute(v, &renames);
                }
            }
        }
    }

    done
}

/// A temp-name prefix no existing temp in this function uses.
///
/// The lowerer names temps `t<n>` and parameters `param_<name>`, so `m2r` is
/// already free — but "already free" is a property of another module that
/// could change, and a name collision here would silently merge two values.
/// Checked rather than assumed.
fn fresh_prefix(func: &IRFunction) -> String {
    let mut used: HashSet<&str> = HashSet::new();
    for b in &func.blocks {
        for i in &b.instrs {
            if let Some(d) = ssa::instr_def(i) {
                used.insert(d);
            }
        }
    }
    let mut prefix = String::from("m2r");
    while used.iter().any(|u| u.starts_with(&prefix)) {
        prefix.push('_');
    }
    prefix
}

fn substitute(val: &mut IRValue, renames: &HashMap<String, IRValue>) {
    if let IRValue::Temp(t) = val {
        if let Some(r) = renames.get(&t.0) {
            *val = r.clone();
        }
    }
}

/// Rewrite every operand of an instruction. Exhaustive on purpose: a variant
/// this does not list would keep a reference to a deleted `Load`'s result.
fn substitute_in_instr(instr: &mut IRInstr, renames: &HashMap<String, IRValue>) {
    match instr {
        IRInstr::BinOp { lhs, rhs, .. } => {
            substitute(lhs, renames);
            substitute(rhs, renames);
        }
        IRInstr::UnOp { operand, .. } => substitute(operand, renames),
        IRInstr::Assign { src, .. } => substitute(src, renames),
        IRInstr::Alloca { .. } => {}
        IRInstr::Store { ptr, val, .. } => {
            substitute(ptr, renames);
            substitute(val, renames);
        }
        IRInstr::Load { ptr, .. } => substitute(ptr, renames),
        IRInstr::Call { args, .. } => {
            for a in args {
                substitute(a, renames);
            }
        }
        IRInstr::CallIndirect { fn_ptr, args, .. } => {
            substitute(fn_ptr, renames);
            for a in args {
                substitute(a, renames);
            }
        }
        IRInstr::GetPtr { ptr, idx, .. } => {
            substitute(ptr, renames);
            substitute(idx, renames);
        }
        IRInstr::BoundsCheck { idx, .. } => substitute(idx, renames),
        IRInstr::TritMin { a, b, .. } | IRInstr::TritMax { a, b, .. } => {
            substitute(a, renames);
            substitute(b, renames);
        }
        IRInstr::TritLane { a, b, .. } => {
            substitute(a, renames);
            substitute(b, renames);
        }
        IRInstr::TritNeg { a, .. } | IRInstr::TritSign { a, .. } => substitute(a, renames),
        IRInstr::PrintStr(v)
        | IRInstr::PrintInt(v)
        | IRInstr::PrintFloat(v)
        | IRInstr::PrintBool3(v)
        | IRInstr::PrintTrit(v) => substitute(v, renames),
        IRInstr::Phi { incoming, .. } => {
            for (v, _) in incoming {
                substitute(v, renames);
            }
        }
        IRInstr::Cast { src, .. } => substitute(src, renames),
    }
}

fn substitute_in_term(term: &mut IRTerminator, renames: &HashMap<String, IRValue>) {
    match term {
        IRTerminator::Return(Some(v)) => substitute(v, renames),
        IRTerminator::BinBranch { cond, .. } | IRTerminator::TritBranch { cond, .. } => {
            substitute(cond, renames)
        }
        IRTerminator::Return(None) | IRTerminator::Jump(_) | IRTerminator::Unreachable => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str) -> IRValue {
        IRValue::Temp(IRTemp::new(name))
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

    fn block(label: &str, instrs: Vec<IRInstr>, term: IRTerminator) -> IRBlock {
        IRBlock { label: label.into(), instrs, term }
    }

    fn alloca(dst: &str) -> IRInstr {
        IRInstr::Alloca { dst: IRTemp::new(dst), ty: IRType::I64 }
    }

    fn store(ptr: &str, v: i64) -> IRInstr {
        IRInstr::Store {
            ptr: t(ptr),
            val: IRValue::Const(IRConst::Int(v)),
            ty: IRType::I64,
        }
    }

    fn store_val(ptr: &str, v: IRValue) -> IRInstr {
        IRInstr::Store { ptr: t(ptr), val: v, ty: IRType::I64 }
    }

    fn load(dst: &str, ptr: &str) -> IRInstr {
        IRInstr::Load { dst: IRTemp::new(dst), ptr: t(ptr), ty: IRType::I64 }
    }

    fn add(dst: &str, a: IRValue, b: IRValue) -> IRInstr {
        IRInstr::BinOp {
            dst: IRTemp::new(dst),
            op: IRBinOp::Add,
            lhs: a,
            rhs: b,
            ty: IRType::I64,
        }
    }

    fn count(f: &IRFunction, p: impl Fn(&IRInstr) -> bool) -> usize {
        f.blocks.iter().flat_map(|b| b.instrs.iter()).filter(|i| p(i)).count()
    }

    /// The value a `Return` hands back, for asserting what a promotion produced.
    fn returned(f: &IRFunction, label: &str) -> IRValue {
        let b = f.blocks.iter().find(|b| b.label == label).expect("block");
        match &b.term {
            IRTerminator::Return(Some(v)) => v.clone(),
            other => panic!("{} does not return a value: {:?}", label, other),
        }
    }

    #[test]
    fn a_store_then_load_becomes_the_stored_value() {
        let mut f = func(vec![block(
            "entry",
            vec![alloca("x"), store("x", 42), load("v", "x")],
            IRTerminator::Return(Some(t("v"))),
        )]);
        let done = promote_function(&mut f);
        assert_eq!(done.allocas, 1);
        assert_eq!(done.loads, 1);
        assert_eq!(done.stores, 1);
        assert_eq!(done.phis, 0, "a single block needs no phi");
        assert_eq!(f.blocks[0].instrs.len(), 0, "nothing should be left: {:?}", f.blocks[0].instrs);
        assert!(
            matches!(returned(&f, "entry"), IRValue::Const(IRConst::Int(42))),
            "{:?}",
            returned(&f, "entry")
        );
        assert_eq!(ssa::verify(&f), Vec::new());
    }

    #[test]
    fn the_last_store_before_a_load_is_the_one_that_wins() {
        let mut f = func(vec![block(
            "entry",
            vec![alloca("x"), store("x", 1), store("x", 2), load("v", "x")],
            IRTerminator::Return(Some(t("v"))),
        )]);
        promote_function(&mut f);
        assert!(matches!(returned(&f, "entry"), IRValue::Const(IRConst::Int(2))));
    }

    #[test]
    fn a_variable_assigned_in_both_arms_gets_one_phi_at_the_join() {
        let mut f = func(vec![
            block(
                "entry",
                vec![alloca("x"), store("x", 0)],
                IRTerminator::BinBranch {
                    cond: IRValue::Const(IRConst::Bool(true)),
                    true_label: "then".into(),
                    false_label: "else".into(),
                },
            ),
            block("then", vec![store("x", 1)], IRTerminator::Jump("join".into())),
            block("else", vec![store("x", 2)], IRTerminator::Jump("join".into())),
            block("join", vec![load("v", "x")], IRTerminator::Return(Some(t("v")))),
        ]);
        let done = promote_function(&mut f);
        assert_eq!(done.phis, 1, "exactly one merge point");
        assert_eq!(count(&f, |i| matches!(i, IRInstr::Load { .. })), 0);
        assert_eq!(count(&f, |i| matches!(i, IRInstr::Store { .. })), 0);
        assert_eq!(count(&f, |i| matches!(i, IRInstr::Alloca { .. })), 0);

        let join = f.blocks.iter().find(|b| b.label == "join").unwrap();
        let IRInstr::Phi { dst, incoming, .. } = &join.instrs[0] else {
            panic!("expected a phi at the join: {:?}", join.instrs);
        };
        assert_eq!(incoming.len(), 2, "{:?}", incoming);
        let mut got: Vec<(i64, &str)> = incoming
            .iter()
            .map(|(v, l)| match v {
                IRValue::Const(IRConst::Int(n)) => (*n, l.as_str()),
                other => panic!("unexpected phi operand {:?}", other),
            })
            .collect();
        got.sort();
        assert_eq!(got, vec![(1, "then"), (2, "else")]);
        assert!(matches!(returned(&f, "join"), IRValue::Temp(ref x) if x.0 == dst.0));
        assert_eq!(ssa::verify(&f), Vec::new(), "the result must still be SSA");
    }

    #[test]
    fn a_variable_assigned_in_only_one_arm_still_merges_the_original() {
        let mut f = func(vec![
            block(
                "entry",
                vec![alloca("x"), store("x", 7)],
                IRTerminator::BinBranch {
                    cond: IRValue::Const(IRConst::Bool(true)),
                    true_label: "then".into(),
                    false_label: "join".into(),
                },
            ),
            block("then", vec![store("x", 9)], IRTerminator::Jump("join".into())),
            block("join", vec![load("v", "x")], IRTerminator::Return(Some(t("v")))),
        ]);
        let done = promote_function(&mut f);
        assert_eq!(done.phis, 1);
        let join = f.blocks.iter().find(|b| b.label == "join").unwrap();
        let IRInstr::Phi { incoming, .. } = &join.instrs[0] else {
            panic!("expected a phi: {:?}", join.instrs)
        };
        let mut got: Vec<(i64, &str)> = incoming
            .iter()
            .map(|(v, l)| match v {
                IRValue::Const(IRConst::Int(n)) => (*n, l.as_str()),
                other => panic!("{:?}", other),
            })
            .collect();
        got.sort();
        assert_eq!(
            got,
            vec![(7, "entry"), (9, "then")],
            "the fall-through edge carries the value from before the branch"
        );
        assert_eq!(ssa::verify(&f), Vec::new());
    }

    #[test]
    fn a_loop_counter_becomes_a_phi_at_the_head() {
        // x = 0; while (c) { x = x + 1 } return x
        let mut f = func(vec![
            block("entry", vec![alloca("x"), store("x", 0)], IRTerminator::Jump("head".into())),
            block(
                "head",
                Vec::new(),
                IRTerminator::BinBranch {
                    cond: IRValue::Const(IRConst::Bool(true)),
                    true_label: "body".into(),
                    false_label: "exit".into(),
                },
            ),
            block(
                "body",
                vec![
                    load("cur", "x"),
                    add("next", t("cur"), IRValue::Const(IRConst::Int(1))),
                    store_val("x", t("next")),
                ],
                IRTerminator::Jump("head".into()),
            ),
            block("exit", vec![load("out", "x")], IRTerminator::Return(Some(t("out")))),
        ]);
        let done = promote_function(&mut f);
        assert_eq!(done.phis, 1, "the loop head is the only merge point");
        assert_eq!(count(&f, |i| matches!(i, IRInstr::Load { .. })), 0);

        let head = f.blocks.iter().find(|b| b.label == "head").unwrap();
        let IRInstr::Phi { dst, incoming, .. } = &head.instrs[0] else {
            panic!("expected a phi at the loop head: {:?}", head.instrs)
        };
        assert_eq!(incoming.len(), 2);
        let from_entry = incoming.iter().find(|(_, l)| l == "entry").expect("entry edge");
        assert!(matches!(from_entry.0, IRValue::Const(IRConst::Int(0))));
        let from_body = incoming.iter().find(|(_, l)| l == "body").expect("body edge");
        assert!(
            matches!(&from_body.0, IRValue::Temp(x) if x.0 == "next"),
            "the back edge carries the incremented value: {:?}",
            from_body
        );

        // The add now reads the phi, not a load.
        let body = f.blocks.iter().find(|b| b.label == "body").unwrap();
        let IRInstr::BinOp { lhs, .. } = &body.instrs[0] else {
            panic!("{:?}", body.instrs)
        };
        assert!(matches!(lhs, IRValue::Temp(x) if x.0 == dst.0), "{:?}", lhs);
        assert_eq!(ssa::verify(&f), Vec::new());
    }

    #[test]
    fn a_read_with_no_store_on_any_path_becomes_a_typed_zero() {
        let mut f = func(vec![block(
            "entry",
            vec![alloca("x"), load("v", "x")],
            IRTerminator::Return(Some(t("v"))),
        )]);
        promote_function(&mut f);
        assert!(
            matches!(returned(&f, "entry"), IRValue::Const(IRConst::Int(0))),
            "an uninitialised read is a deterministic zero, not stack garbage"
        );
    }

    #[test]
    fn an_escaping_alloca_is_left_exactly_as_it_was() {
        let before = vec![
            alloca("x"),
            store("x", 1),
            IRInstr::Call {
                dst: None,
                func: "g".into(),
                args: vec![t("x")],
                ret_ty: IRType::Void,
            },
            load("v", "x"),
        ];
        let mut f = func(vec![block("entry", before.clone(), IRTerminator::Return(Some(t("v"))))]);
        let done = promote_function(&mut f);
        assert_eq!(done, Promoted::default(), "nothing may be promoted");
        assert_eq!(f.blocks[0].instrs.len(), before.len());
        assert_eq!(count(&f, |i| matches!(i, IRInstr::Load { .. })), 1);
    }

    #[test]
    fn a_promotable_and_an_escaping_alloca_in_one_function_are_handled_separately() {
        let mut f = func(vec![block(
            "entry",
            vec![
                alloca("keep"),
                alloca("lift"),
                store("keep", 1),
                store("lift", 2),
                IRInstr::Call {
                    dst: None,
                    func: "g".into(),
                    args: vec![t("keep")],
                    ret_ty: IRType::Void,
                },
                load("a", "keep"),
                load("b", "lift"),
                add("sum", t("a"), t("b")),
            ],
            IRTerminator::Return(Some(t("sum"))),
        )]);
        let done = promote_function(&mut f);
        assert_eq!(done.allocas, 1);
        assert_eq!(count(&f, |i| matches!(i, IRInstr::Alloca { .. })), 1, "keep survives");
        assert_eq!(count(&f, |i| matches!(i, IRInstr::Load { .. })), 1);
        let IRInstr::BinOp { rhs, .. } = f.blocks[0].instrs.last().unwrap() else {
            panic!("{:?}", f.blocks[0].instrs)
        };
        assert!(
            matches!(rhs, IRValue::Const(IRConst::Int(2))),
            "the promoted operand is the stored constant: {:?}",
            rhs
        );
        assert_eq!(ssa::verify(&f), Vec::new());
    }

    #[test]
    fn nested_branches_place_phis_at_every_real_merge_and_no_others() {
        //        entry
        //       /     \
        //     a1       a2        (both store x)
        //    /  \       |
        //   b1  b2      |        (both store x)
        //    \  /       |
        //     m1        |        <- phi here
        //       \      /
        //         join          <- and here
        let mut f = func(vec![
            block(
                "entry",
                vec![alloca("x"), store("x", 0)],
                IRTerminator::BinBranch {
                    cond: IRValue::Const(IRConst::Bool(true)),
                    true_label: "a1".into(),
                    false_label: "a2".into(),
                },
            ),
            block(
                "a1",
                Vec::new(),
                IRTerminator::BinBranch {
                    cond: IRValue::Const(IRConst::Bool(true)),
                    true_label: "b1".into(),
                    false_label: "b2".into(),
                },
            ),
            block("b1", vec![store("x", 1)], IRTerminator::Jump("m1".into())),
            block("b2", vec![store("x", 2)], IRTerminator::Jump("m1".into())),
            block("m1", Vec::new(), IRTerminator::Jump("join".into())),
            block("a2", vec![store("x", 3)], IRTerminator::Jump("join".into())),
            block("join", vec![load("v", "x")], IRTerminator::Return(Some(t("v")))),
        ]);
        let done = promote_function(&mut f);
        assert_eq!(done.phis, 2, "m1 and join, and nowhere else");
        for label in ["m1", "join"] {
            let b = f.blocks.iter().find(|b| b.label == label).unwrap();
            assert!(
                matches!(b.instrs.first(), Some(IRInstr::Phi { .. })),
                "{} should start with a phi: {:?}",
                label,
                b.instrs
            );
        }
        assert_eq!(ssa::verify(&f), Vec::new());
    }

    #[test]
    fn an_existing_phi_operand_is_rewritten_when_its_load_is_promoted() {
        // The join's phi takes a value produced by a load in `then`. Promoting
        // that load must rewrite the phi operand, and the phi's block may be
        // walked before `then` is — which is why the operands are fixed up in
        // a pass of their own.
        let mut f = func(vec![
            block(
                "entry",
                vec![alloca("x"), store("x", 5)],
                IRTerminator::BinBranch {
                    cond: IRValue::Const(IRConst::Bool(true)),
                    true_label: "then".into(),
                    false_label: "else".into(),
                },
            ),
            block("then", vec![load("l", "x")], IRTerminator::Jump("join".into())),
            block(
                "else",
                vec![add("e", IRValue::Const(IRConst::Int(1)), IRValue::Const(IRConst::Int(1)))],
                IRTerminator::Jump("join".into()),
            ),
            block(
                "join",
                vec![IRInstr::Phi {
                    dst: IRTemp::new("p"),
                    ty: IRType::I64,
                    incoming: vec![(t("l"), "then".into()), (t("e"), "else".into())],
                }],
                IRTerminator::Return(Some(t("p"))),
            ),
        ]);
        promote_function(&mut f);
        let join = f.blocks.iter().find(|b| b.label == "join").unwrap();
        let phi = join
            .instrs
            .iter()
            .find_map(|i| match i {
                IRInstr::Phi { dst, incoming, .. } if dst.0 == "p" => Some(incoming),
                _ => None,
            })
            .expect("the original phi survives");
        let from_then = phi.iter().find(|(_, l)| l == "then").unwrap();
        assert!(
            matches!(from_then.0, IRValue::Const(IRConst::Int(5))),
            "the promoted load's result must be substituted into the phi: {:?}",
            from_then
        );
        assert_eq!(ssa::verify(&f), Vec::new());
    }

    #[test]
    fn the_result_of_promoting_is_always_still_ssa() {
        // A shape with a loop, a branch inside it, and two variables.
        let mut f = func(vec![
            block(
                "entry",
                vec![alloca("i"), alloca("acc"), store("i", 0), store("acc", 0)],
                IRTerminator::Jump("head".into()),
            ),
            block(
                "head",
                Vec::new(),
                IRTerminator::BinBranch {
                    cond: IRValue::Const(IRConst::Bool(true)),
                    true_label: "body".into(),
                    false_label: "exit".into(),
                },
            ),
            block(
                "body",
                vec![load("i0", "i")],
                IRTerminator::BinBranch {
                    cond: t("i0"),
                    true_label: "odd".into(),
                    false_label: "even".into(),
                },
            ),
            block(
                "odd",
                vec![
                    load("a0", "acc"),
                    add("a1", t("a0"), IRValue::Const(IRConst::Int(1))),
                    store_val("acc", t("a1")),
                ],
                IRTerminator::Jump("step".into()),
            ),
            block("even", Vec::new(), IRTerminator::Jump("step".into())),
            block(
                "step",
                vec![
                    load("i1", "i"),
                    add("i2", t("i1"), IRValue::Const(IRConst::Int(1))),
                    store_val("i", t("i2")),
                ],
                IRTerminator::Jump("head".into()),
            ),
            block("exit", vec![load("r", "acc")], IRTerminator::Return(Some(t("r")))),
        ]);
        let done = promote_function(&mut f);
        assert_eq!(done.allocas, 2);
        assert_eq!(count(&f, |i| matches!(i, IRInstr::Load { .. })), 0);
        assert_eq!(count(&f, |i| matches!(i, IRInstr::Store { .. })), 0);
        assert_eq!(
            ssa::verify(&f),
            Vec::new(),
            "blocks:\n{}",
            f.blocks
                .iter()
                .map(|b| format!("{}: {:?} -> {:?}", b.label, b.instrs, b.term))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn promotion_is_deterministic() {
        let build = || {
            func(vec![
                block(
                    "entry",
                    vec![alloca("z"), alloca("a"), store("z", 1), store("a", 2)],
                    IRTerminator::BinBranch {
                        cond: IRValue::Const(IRConst::Bool(true)),
                        true_label: "then".into(),
                        false_label: "join".into(),
                    },
                ),
                block("then", vec![store("z", 3), store("a", 4)], IRTerminator::Jump("join".into())),
                block(
                    "join",
                    vec![load("x", "z"), load("y", "a"), add("s", t("x"), t("y"))],
                    IRTerminator::Return(Some(t("s"))),
                ),
            ])
        };
        let render = |f: &IRFunction| {
            f.blocks
                .iter()
                .map(|b| format!("{}: {:?} -> {:?}", b.label, b.instrs, b.term))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let mut a = build();
        let mut b = build();
        promote_function(&mut a);
        promote_function(&mut b);
        assert_eq!(render(&a), render(&b), "phi naming and placement must not vary run to run");
    }
}
