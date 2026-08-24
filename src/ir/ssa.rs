//! F-1 — SSA form: the control-flow graph, dominance, and a verifier.
//!
//! Recommendation F-1 says "the current IR is not SSA" and cites
//! `optimize.rs` propagating constants through a `HashMap` keyed by temp name
//! as the workaround. That is a diagnosis, not a measurement, and F-1 is the
//! prerequisite for the whole performance tier — so the first thing built here
//! is the instrument that turns the diagnosis into a number.
//!
//! **This module answers a question; it does not change the IR.** The verifier
//! reports what is not SSA and where. `mem2reg` (below) is what makes it so,
//! and it is only trustworthy because the verifier came first: a promotion
//! pass that inserts phi nodes and is checked by nothing is a pass that
//! inserts subtly wrong phi nodes.
//!
//! ## What "SSA" means here
//!
//! Three properties, checked separately so a failure names which one:
//!
//!   1. **Single assignment.** Every temp is defined by at most one
//!      instruction. Function parameters count as definitions at entry.
//!   2. **Dominance.** Every use of a temp is dominated by its definition. A
//!      phi operand is used at the END of its incoming block, not at the phi,
//!      which is the one place this differs from the obvious reading.
//!   3. **Well-formed phis.** A phi has exactly one incoming edge per
//!      predecessor of its block, and every incoming label is a predecessor.
//!
//! Unreachable blocks are excluded. They are removed by
//! `optimize::dead_block_eliminate`, but the verifier may run before it, and a
//! use in code that cannot execute is not a dominance failure.
//!
//! © Manish Jagdish Thatte

use std::collections::{HashMap, HashSet};

use super::types::*;

// ---------------------------------------------------------------------------
// Control-flow graph
// ---------------------------------------------------------------------------

/// The CFG of one function, as indices into `func.blocks`.
///
/// Built by label, because that is how the IR refers to blocks: a terminator
/// names its successors as strings. A terminator naming a label that no block
/// carries is recorded in `dangling` rather than dropped — it is a malformed
/// function, and silently ignoring it would let the verifier pass a function
/// the backends cannot emit.
pub struct Cfg {
    /// Successor block indices, per block.
    pub succs: Vec<Vec<usize>>,
    /// Predecessor block indices, per block.
    pub preds: Vec<Vec<usize>>,
    /// Label → block index.
    pub index: HashMap<String, usize>,
    /// Labels named by a terminator that no block defines.
    pub dangling: Vec<(usize, String)>,
}

impl Cfg {
    pub fn of(func: &IRFunction) -> Cfg {
        let mut index = HashMap::new();
        for (i, b) in func.blocks.iter().enumerate() {
            // First definition wins. A duplicate label is itself a defect, and
            // `verify` reports it; resolving to the first keeps the graph
            // deterministic in the meantime.
            index.entry(b.label.clone()).or_insert(i);
        }

        let n = func.blocks.len();
        let mut succs = vec![Vec::new(); n];
        let mut preds = vec![Vec::new(); n];
        let mut dangling = Vec::new();

        for (i, b) in func.blocks.iter().enumerate() {
            for label in terminator_targets(&b.term) {
                match index.get(label) {
                    Some(&j) => {
                        if !succs[i].contains(&j) {
                            succs[i].push(j);
                        }
                        if !preds[j].contains(&i) {
                            preds[j].push(i);
                        }
                    }
                    None => dangling.push((i, label.clone())),
                }
            }
        }

        Cfg { succs, preds, index, dangling }
    }

    pub fn len(&self) -> usize {
        self.succs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.succs.is_empty()
    }
}

/// The labels a terminator can transfer control to.
///
/// Exhaustive on purpose: a new terminator that this does not list would be
/// invisible to the CFG, and every analysis built on the CFG would then be
/// quietly wrong rather than absent. The same reasoning is why
/// `optimize::collect_used_in_instruction` has no catch-all arm.
pub fn terminator_targets(term: &IRTerminator) -> Vec<&String> {
    match term {
        IRTerminator::Jump(l) => vec![l],
        IRTerminator::BinBranch { true_label, false_label, .. } => {
            vec![true_label, false_label]
        }
        IRTerminator::TritBranch { pos_label, zero_label, neg_label, .. } => {
            vec![pos_label, zero_label, neg_label]
        }
        IRTerminator::Return(_) | IRTerminator::Unreachable => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Dominance
// ---------------------------------------------------------------------------

/// Immediate dominators and the reverse-postorder numbering they are computed
/// in, for one function.
///
/// Cooper, Harvey and Kennedy's iterative algorithm ("A Simple, Fast Dominance
/// Algorithm"). Chosen over Lengauer–Tarjan deliberately: it is a page of code
/// instead of several, it is fast enough at the block counts this compiler
/// produces (the largest function in the corpus is well under a hundred
/// blocks), and — the reason that matters here — it is short enough to read
/// and check by eye. A dominance bug produces phi nodes in the wrong places,
/// and a wrong phi is a silently wrong answer.
pub struct Dominators {
    /// Immediate dominator of each block. `None` for the entry and for every
    /// unreachable block.
    pub idom: Vec<Option<usize>>,
    /// Blocks in reverse postorder from the entry. Unreachable blocks are
    /// absent.
    pub rpo: Vec<usize>,
    /// Position of each block in `rpo`, or `usize::MAX` if unreachable.
    pub rpo_pos: Vec<usize>,
}

impl Dominators {
    pub fn of(cfg: &Cfg) -> Dominators {
        let n = cfg.len();
        let mut idom = vec![None; n];
        let mut rpo_pos = vec![usize::MAX; n];
        if n == 0 {
            return Dominators { idom, rpo: Vec::new(), rpo_pos };
        }

        let rpo = reverse_postorder(cfg);
        for (pos, &b) in rpo.iter().enumerate() {
            rpo_pos[b] = pos;
        }

        // The entry dominates itself; the algorithm needs that seeded.
        let entry = rpo[0];
        idom[entry] = Some(entry);

        let mut changed = true;
        while changed {
            changed = false;
            for &b in rpo.iter().skip(1) {
                // Start from the first processed predecessor, then intersect
                // with the rest.
                let mut new_idom: Option<usize> = None;
                for &p in &cfg.preds[b] {
                    if idom[p].is_none() {
                        continue; // not yet processed, or unreachable
                    }
                    new_idom = Some(match new_idom {
                        None => p,
                        Some(cur) => intersect(&idom, &rpo_pos, p, cur),
                    });
                }
                if let Some(ni) = new_idom {
                    if idom[b] != Some(ni) {
                        idom[b] = Some(ni);
                        changed = true;
                    }
                }
            }
        }

        // The entry's idom is itself only as an algorithmic seed; report it as
        // "no dominator" so `dominates` and the frontier computation do not
        // have to special-case a self-loop that is not in the CFG.
        idom[entry] = None;

        Dominators { idom, rpo, rpo_pos }
    }

    /// Whether block `a` dominates block `b`. Every block dominates itself.
    ///
    /// An unreachable `b` is dominated by nothing, including itself: a
    /// question about which definitions reach it has no useful answer.
    pub fn dominates(&self, a: usize, b: usize) -> bool {
        if self.rpo_pos[b] == usize::MAX || self.rpo_pos[a] == usize::MAX {
            return false;
        }
        let mut cur = b;
        loop {
            if cur == a {
                return true;
            }
            match self.idom[cur] {
                Some(p) => cur = p,
                None => return false,
            }
        }
    }

    pub fn is_reachable(&self, b: usize) -> bool {
        self.rpo_pos[b] != usize::MAX
    }

    /// The dominance frontier of every block — where phi nodes go.
    ///
    /// Cytron's formulation: for each join point `b`, walk up from each
    /// predecessor to `idom(b)`, adding `b` to every block passed through.
    pub fn frontiers(&self, cfg: &Cfg) -> Vec<HashSet<usize>> {
        let mut df: Vec<HashSet<usize>> = vec![HashSet::new(); cfg.len()];
        for b in 0..cfg.len() {
            if !self.is_reachable(b) || cfg.preds[b].len() < 2 {
                continue;
            }
            let stop = self.idom[b];
            for &p in &cfg.preds[b] {
                if !self.is_reachable(p) {
                    continue;
                }
                let mut runner = p;
                while Some(runner) != stop {
                    df[runner].insert(b);
                    match self.idom[runner] {
                        Some(next) => runner = next,
                        // Reached the entry without meeting idom(b): the whole
                        // path from the entry is in b's frontier, and there is
                        // nowhere further to walk.
                        None => break,
                    }
                }
            }
        }
        df
    }
}

/// The two-pointer walk up the dominator tree that Cooper–Harvey–Kennedy uses
/// in place of a set intersection.
fn intersect(idom: &[Option<usize>], rpo_pos: &[usize], mut a: usize, mut b: usize) -> usize {
    while a != b {
        while rpo_pos[a] > rpo_pos[b] {
            match idom[a] {
                Some(p) if p != a => a = p,
                _ => return b,
            }
        }
        while rpo_pos[b] > rpo_pos[a] {
            match idom[b] {
                Some(p) if p != b => b = p,
                _ => return a,
            }
        }
    }
    a
}

/// Reverse postorder from block 0. Iterative, because a deeply nested function
/// would otherwise recurse as far as its CFG is deep.
fn reverse_postorder(cfg: &Cfg) -> Vec<usize> {
    let n = cfg.len();
    let mut visited = vec![false; n];
    let mut post = Vec::with_capacity(n);
    // (block, next successor index to consider)
    let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
    visited[0] = true;
    while let Some((b, i)) = stack.pop() {
        if i < cfg.succs[b].len() {
            stack.push((b, i + 1));
            let s = cfg.succs[b][i];
            if !visited[s] {
                visited[s] = true;
                stack.push((s, 0));
            }
        } else {
            post.push(b);
        }
    }
    post.reverse();
    post
}

// ---------------------------------------------------------------------------
// Uses and definitions
// ---------------------------------------------------------------------------

/// The temp an instruction defines, if any.
///
/// Distinct from `optimize::instr_dst_name`, which omits `Call` and
/// `CallIndirect` because dead-code elimination treats them as side-effecting
/// and never needs their destination. A verifier does: `%t = call f()` defines
/// `%t`, and a second definition of the same name is exactly the defect this
/// module exists to find.
pub fn instr_def(instr: &IRInstr) -> Option<&str> {
    match instr {
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
        | IRInstr::Cast { dst, .. } => Some(&dst.0),
        IRInstr::Call { dst, .. } | IRInstr::CallIndirect { dst, .. } => {
            dst.as_ref().map(|t| t.0.as_str())
        }
        IRInstr::Store { .. }
        | IRInstr::BoundsCheck { .. }
        | IRInstr::PrintStr(_)
        | IRInstr::PrintInt(_)
        | IRInstr::PrintFloat(_)
        | IRInstr::PrintBool3(_)
        | IRInstr::PrintTrit(_) => None,
    }
}

/// Every temp an instruction uses, EXCLUDING phi operands.
///
/// Phi operands are deliberately absent: they are used at the end of the
/// incoming block, not where the phi is written, and folding them in here
/// would make every loop-carried phi look like a use before its definition.
/// `phi_uses` returns them with their edges instead.
pub fn instr_uses(instr: &IRInstr) -> Vec<&str> {
    let mut out = Vec::new();
    macro_rules! push {
        ($v:expr) => {
            if let IRValue::Temp(t) = $v {
                out.push(t.0.as_str());
            }
        };
    }
    match instr {
        IRInstr::BinOp { lhs, rhs, .. } => {
            push!(lhs);
            push!(rhs);
        }
        IRInstr::UnOp { operand, .. } => push!(operand),
        IRInstr::Assign { src, .. } => push!(src),
        IRInstr::Alloca { .. } => {}
        IRInstr::Store { ptr, val, .. } => {
            push!(ptr);
            push!(val);
        }
        IRInstr::Load { ptr, .. } => push!(ptr),
        IRInstr::Call { args, .. } => {
            for a in args {
                push!(a);
            }
        }
        IRInstr::CallIndirect { fn_ptr, args, .. } => {
            push!(fn_ptr);
            for a in args {
                push!(a);
            }
        }
        IRInstr::GetPtr { ptr, idx, .. } => {
            push!(ptr);
            push!(idx);
        }
        IRInstr::BoundsCheck { idx, .. } => push!(idx),
        IRInstr::TritMin { a, b, .. } | IRInstr::TritMax { a, b, .. } => {
            push!(a);
            push!(b);
        }
        IRInstr::TritLane { a, b, .. } => {
            push!(a);
            push!(b);
        }
        IRInstr::TritNeg { a, .. } | IRInstr::TritSign { a, .. } => push!(a),
        IRInstr::PrintStr(v)
        | IRInstr::PrintInt(v)
        | IRInstr::PrintFloat(v)
        | IRInstr::PrintBool3(v)
        | IRInstr::PrintTrit(v) => push!(v),
        IRInstr::Phi { .. } => {}
        IRInstr::Cast { src, .. } => push!(src),
    }
    out
}

/// A phi's operands, as (temp, incoming label) pairs. Empty for anything else.
pub fn phi_uses(instr: &IRInstr) -> Vec<(&str, &str)> {
    match instr {
        IRInstr::Phi { incoming, .. } => incoming
            .iter()
            .filter_map(|(v, l)| match v {
                IRValue::Temp(t) => Some((t.0.as_str(), l.as_str())),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Every temp a terminator uses.
pub fn term_uses(term: &IRTerminator) -> Vec<&str> {
    match term {
        IRTerminator::Return(Some(IRValue::Temp(t))) => vec![t.0.as_str()],
        IRTerminator::BinBranch { cond: IRValue::Temp(t), .. }
        | IRTerminator::TritBranch { cond: IRValue::Temp(t), .. } => vec![t.0.as_str()],
        IRTerminator::Return(_)
        | IRTerminator::BinBranch { .. }
        | IRTerminator::TritBranch { .. }
        | IRTerminator::Jump(_)
        | IRTerminator::Unreachable => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Critical-edge splitting
// ---------------------------------------------------------------------------

/// Rewrite the terminator's targets, replacing every occurrence of `from`
/// with `to`.
fn retarget(term: &mut IRTerminator, from: &str, to: &str) {
    let fix = |l: &mut String| {
        if l == from {
            *l = to.to_string();
        }
    };
    match term {
        IRTerminator::Jump(l) => fix(l),
        IRTerminator::BinBranch { true_label, false_label, .. } => {
            fix(true_label);
            fix(false_label);
        }
        IRTerminator::TritBranch { pos_label, zero_label, neg_label, .. } => {
            fix(pos_label);
            fix(zero_label);
            fix(neg_label);
        }
        IRTerminator::Return(_) | IRTerminator::Unreachable => {}
    }
}

/// Split every critical edge — an edge from a block with several successors
/// into a block with several predecessors — by putting an empty block on it.
///
/// Returns the number of edges split.
///
/// **Why this is not optional here.** A phi means "take the value this edge
/// carried", and something has to put that value somewhere on the edge. On a
/// register machine that is a copy, and the copy has to happen where the edge
/// is: at the end of the predecessor if the predecessor has only this
/// successor, and at the start of the successor if the successor has only this
/// predecessor. On a critical edge neither place exists, so the edge needs a
/// block of its own.
///
/// The T3 backend emits phi copies only in its `Jump` arm — a `BinBranch`
/// predecessor writes nothing at all (report.txt P12). After this pass every
/// phi's predecessor ends in a `Jump`, which is exactly the case that backend
/// handles. The LLVM backend does not need it and is unaffected beyond a few
/// extra empty blocks.
///
/// The new block is inserted immediately after its predecessor rather than
/// appended, because the T3 emitter takes a block's canonical register state
/// from the first predecessor to reach it in EMISSION order, and appending
/// would put the split block after every path that reaches it.
pub fn split_critical_edges(func: &mut IRFunction) -> usize {
    if func.blocks.len() < 2 {
        return 0;
    }
    let cfg = Cfg::of(func);
    // (predecessor index, successor label) for each critical edge.
    let mut to_split: Vec<(usize, String)> = Vec::new();
    for b in 0..cfg.len() {
        if cfg.succs[b].len() < 2 {
            continue;
        }
        for &s in &cfg.succs[b] {
            if cfg.preds[s].len() > 1 {
                to_split.push((b, func.blocks[s].label.clone()));
            }
        }
    }
    if to_split.is_empty() {
        return 0;
    }

    // Deterministic: by predecessor, then by successor label.
    to_split.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut existing: HashSet<String> =
        func.blocks.iter().map(|b| b.label.clone()).collect();
    let n = to_split.len();
    // Work from the last predecessor backwards so earlier insertion points
    // stay valid.
    for (pi, succ_label) in to_split.into_iter().rev() {
        let pred_label = func.blocks[pi].label.clone();
        let mut new_label = format!("edge_{}_{}", pred_label, succ_label);
        while existing.contains(&new_label) {
            new_label.push('_');
        }
        existing.insert(new_label.clone());

        retarget(&mut func.blocks[pi].term, &succ_label, &new_label);

        // The successor's phis took their value on this edge from `pred`;
        // now they take it from the new block.
        if let Some(si) = func.blocks.iter().position(|b| b.label == succ_label) {
            for instr in &mut func.blocks[si].instrs {
                if let IRInstr::Phi { incoming, .. } = instr {
                    for (_, l) in incoming.iter_mut() {
                        if *l == pred_label {
                            *l = new_label.clone();
                        }
                    }
                }
            }
        }

        func.blocks.insert(
            pi + 1,
            IRBlock {
                label: new_label,
                instrs: Vec::new(),
                term: IRTerminator::Jump(succ_label),
            },
        );
    }
    n
}

/// Split critical edges in every non-extern function.
pub fn split_critical_edges_module(module: &mut IRModule) -> usize {
    let mut n = 0;
    for f in &mut module.functions {
        if !f.is_extern {
            n += split_critical_edges(f);
        }
    }
    n
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

/// One way in which a function fails to be in SSA form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// A temp is defined more than once. The strings are the block labels the
    /// definitions are in, in order.
    MultiplyDefined { temp: String, at: Vec<String> },
    /// A use whose definition does not dominate it.
    NotDominated { temp: String, used_in: String, defined_in: String },
    /// A use of a temp nothing defines.
    Undefined { temp: String, used_in: String },
    /// A phi whose incoming edges do not match its block's predecessors.
    PhiEdges { block: String, temp: String, expected: Vec<String>, found: Vec<String> },
    /// Two blocks share a label, so a terminator naming it is ambiguous.
    DuplicateLabel { label: String },
    /// A terminator naming a label no block defines.
    DanglingTarget { block: String, label: String },
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::MultiplyDefined { temp, at } => write!(
                f,
                "%{} is defined {} times (in {})",
                temp,
                at.len(),
                at.join(", ")
            ),
            Violation::NotDominated { temp, used_in, defined_in } => write!(
                f,
                "%{} is used in {} but defined in {}, which does not dominate it",
                temp, used_in, defined_in
            ),
            Violation::Undefined { temp, used_in } => {
                write!(f, "%{} is used in {} and defined nowhere", temp, used_in)
            }
            Violation::PhiEdges { block, temp, expected, found } => write!(
                f,
                "phi %{} in {} has incoming [{}] but {} has predecessors [{}]",
                temp,
                block,
                found.join(", "),
                block,
                expected.join(", ")
            ),
            Violation::DuplicateLabel { label } => {
                write!(f, "two blocks are labelled {}", label)
            }
            Violation::DanglingTarget { block, label } => {
                write!(f, "{} branches to {}, which no block defines", block, label)
            }
        }
    }
}

/// Check one function against the three SSA properties.
///
/// An empty result means the function is in SSA form. Extern functions have no
/// blocks and trivially pass.
pub fn verify(func: &IRFunction) -> Vec<Violation> {
    let mut out = Vec::new();
    if func.blocks.is_empty() {
        return out;
    }

    let cfg = Cfg::of(func);
    let doms = Dominators::of(&cfg);

    // Duplicate labels first: everything below resolves labels through
    // `cfg.index`, which silently keeps the first, so an unreported duplicate
    // would make every later message describe the wrong block.
    let mut seen_labels: HashMap<&str, usize> = HashMap::new();
    for b in &func.blocks {
        let n = seen_labels.entry(b.label.as_str()).or_insert(0);
        *n += 1;
    }
    let mut dupes: Vec<&str> = seen_labels
        .iter()
        .filter(|(_, &n)| n > 1)
        .map(|(l, _)| *l)
        .collect();
    dupes.sort_unstable();
    for label in dupes {
        out.push(Violation::DuplicateLabel { label: label.to_string() });
    }

    for (bi, label) in &cfg.dangling {
        out.push(Violation::DanglingTarget {
            block: func.blocks[*bi].label.clone(),
            label: label.clone(),
        });
    }

    // ---- 1. single assignment ------------------------------------------
    //
    // Parameters are definitions at entry. They are named `param_<name>` by
    // the lowerer and have no defining instruction, so nothing else would
    // account for them and every use would be reported as undefined.
    let mut def_block: HashMap<&str, usize> = HashMap::new();
    let mut def_pos: HashMap<&str, usize> = HashMap::new();
    let mut def_sites: HashMap<&str, Vec<String>> = HashMap::new();

    let mut param_names: Vec<String> = Vec::new();
    for (pname, _) in &func.params {
        param_names.push(format!("param_{}", pname));
    }
    for p in &param_names {
        def_block.insert(p.as_str(), 0);
        def_pos.insert(p.as_str(), 0);
    }

    for (bi, block) in func.blocks.iter().enumerate() {
        for (ii, instr) in block.instrs.iter().enumerate() {
            if let Some(d) = instr_def(instr) {
                def_sites.entry(d).or_default().push(block.label.clone());
                // The FIRST definition is the one recorded for dominance.
                // With a multiply-defined temp the dominance answer is
                // meaningless anyway, and that is already reported.
                def_block.entry(d).or_insert(bi);
                // +1 so that "defined at position i" is strictly before "used
                // at position i", which is what a use in the SAME instruction
                // must fail.
                def_pos.entry(d).or_insert(ii + 1);
            }
        }
    }

    let mut multi: Vec<(&str, &Vec<String>)> = def_sites
        .iter()
        .filter(|(_, sites)| sites.len() > 1)
        .map(|(t, s)| (*t, s))
        .collect();
    multi.sort_by_key(|(t, _)| *t);
    for (temp, sites) in multi {
        out.push(Violation::MultiplyDefined {
            temp: temp.to_string(),
            at: sites.clone(),
        });
    }

    // ---- 2 and 3. dominance and phi shape ------------------------------
    for (bi, block) in func.blocks.iter().enumerate() {
        if !doms.is_reachable(bi) {
            continue;
        }

        let mut expected_preds: Vec<String> = cfg.preds[bi]
            .iter()
            .map(|&p| func.blocks[p].label.clone())
            .collect();
        expected_preds.sort();

        for (ii, instr) in block.instrs.iter().enumerate() {
            // Ordinary uses.
            for u in instr_uses(instr) {
                check_use(&mut out, &doms, &def_block, &def_pos, func, u, bi, ii);
            }

            // Phi operands, each checked against its own edge.
            if let IRInstr::Phi { dst, incoming, .. } = instr {
                let mut found: Vec<String> =
                    incoming.iter().map(|(_, l)| l.clone()).collect();
                found.sort();
                if found != expected_preds {
                    out.push(Violation::PhiEdges {
                        block: block.label.clone(),
                        temp: dst.0.clone(),
                        expected: expected_preds.clone(),
                        found,
                    });
                }
                for (temp, label) in phi_uses(instr) {
                    let Some(&edge) = cfg.index.get(label) else {
                        // Already reported as a phi-edge mismatch above.
                        continue;
                    };
                    match def_block.get(temp) {
                        None => out.push(Violation::Undefined {
                            temp: temp.to_string(),
                            used_in: block.label.clone(),
                        }),
                        Some(&db) => {
                            // The operand must be available at the END of the
                            // incoming block — the edge the value travels on.
                            if !doms.dominates(db, edge) {
                                out.push(Violation::NotDominated {
                                    temp: temp.to_string(),
                                    used_in: func.blocks[edge].label.clone(),
                                    defined_in: func.blocks[db].label.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        for u in term_uses(&block.term) {
            check_use(&mut out, &doms, &def_block, &def_pos, func, u, bi, usize::MAX);
        }
    }

    out
}

fn check_use(
    out: &mut Vec<Violation>,
    doms: &Dominators,
    def_block: &HashMap<&str, usize>,
    def_pos: &HashMap<&str, usize>,
    func: &IRFunction,
    temp: &str,
    use_block: usize,
    use_pos: usize,
) {
    let Some(&db) = def_block.get(temp) else {
        out.push(Violation::Undefined {
            temp: temp.to_string(),
            used_in: func.blocks[use_block].label.clone(),
        });
        return;
    };
    let ok = if db == use_block {
        // Same block: the definition must come earlier in it. `usize::MAX`
        // is the terminator, which is after every instruction.
        def_pos.get(temp).copied().unwrap_or(0) <= use_pos
    } else {
        doms.dominates(db, use_block)
    };
    if !ok {
        out.push(Violation::NotDominated {
            temp: temp.to_string(),
            used_in: func.blocks[use_block].label.clone(),
            defined_in: func.blocks[db].label.clone(),
        });
    }
}

// ---------------------------------------------------------------------------
// Promotable allocas — what mem2reg can lift out of memory
// ---------------------------------------------------------------------------

/// Whether a type is a single machine value, and so can live in a temp.
///
/// Aggregates are excluded not because SSA forbids them but because this IR's
/// aggregates are already POINTERS: a struct value is the address of its
/// fields (see `IRType::from_mani`), and the alloca that holds one is reached
/// through `GetPtr`, which is an escaping use and disqualifies it anyway.
pub fn is_scalar(ty: &IRType) -> bool {
    matches!(
        ty,
        IRType::I64
            | IRType::F64
            | IRType::I8
            | IRType::I16
            | IRType::I32
            | IRType::Bool
            | IRType::Trit
            | IRType::Ptr(_)
    )
}

/// The IR type of every temp a function defines, including its parameters.
///
/// Needed because this IR's `Load` and `Store` are **not type-neutral**: they
/// carry a `ty` and the backends coerce to it. `store i64 %v` into an `i8`
/// slot narrows; the matching load widens back. A pass that removes the
/// memory operation removes the coercion with it, so it has to know what it
/// is removing.
pub fn value_types(func: &IRFunction) -> HashMap<String, IRType> {
    let mut out: HashMap<String, IRType> = HashMap::new();
    for (pname, pty) in &func.params {
        out.insert(format!("param_{}", pname), pty.clone());
    }
    for block in &func.blocks {
        for instr in &block.instrs {
            let Some(d) = instr_def(instr) else { continue };
            let ty = match instr {
                IRInstr::BinOp { op, ty, .. } => match op {
                    // A comparison yields a boolean whatever its operands are.
                    IRBinOp::IEq | IRBinOp::INe | IRBinOp::ILt | IRBinOp::IGt
                    | IRBinOp::ILe | IRBinOp::IGe | IRBinOp::FEq | IRBinOp::FNe
                    | IRBinOp::FLt | IRBinOp::FGt | IRBinOp::FLe | IRBinOp::FGe
                    | IRBinOp::StrEq | IRBinOp::StrNe => IRType::Bool,
                    _ => ty.clone(),
                },
                IRInstr::UnOp { ty, .. } => ty.clone(),
                IRInstr::Assign { ty, .. } => ty.clone(),
                IRInstr::Alloca { ty, .. } => IRType::Ptr(Box::new(ty.clone())),
                IRInstr::Load { ty, .. } => ty.clone(),
                IRInstr::GetPtr { ty, .. } => IRType::Ptr(Box::new(ty.clone())),
                IRInstr::Call { ret_ty, .. } | IRInstr::CallIndirect { ret_ty, .. } => {
                    ret_ty.clone()
                }
                // C7: the operand is a word, the result is always a trit.
                IRInstr::TritMin { .. }
                | IRInstr::TritMax { .. }
                | IRInstr::TritNeg { .. }
                | IRInstr::TritSign { .. } => IRType::Trit,
                // Lane-wise results are whole words, and `Popcount` is a count.
                IRInstr::TritLane { .. } => IRType::I64,
                IRInstr::Phi { ty, .. } => ty.clone(),
                IRInstr::Cast { to_ty, .. } => to_ty.clone(),
                IRInstr::Store { .. }
                | IRInstr::BoundsCheck { .. }
                | IRInstr::PrintStr(_)
                | IRInstr::PrintInt(_)
                | IRInstr::PrintFloat(_)
                | IRInstr::PrintBool3(_)
                | IRInstr::PrintTrit(_) => continue,
            };
            out.insert(d.to_string(), ty);
        }
    }
    out
}

/// Temps whose EMITTED width may differ from their IR type.
///
/// The LLVM backend types a call's result from the callee's declared
/// signature, not from the IR's `ret_ty`, and coerces at each use site:
/// `TernaryTrie::get` has `ret_ty: Trit` in the IR and is emitted as
/// `call i64 @TernaryTrie_get`, with a `trunc` wherever the trit is wanted.
/// That is deliberate and it works for every construct whose type comes from
/// its operands.
///
/// A phi is the exception — its type comes from the IR. Promoting a variable
/// fed by such a call therefore produced `phi i8 [ %t94, … ]` with `%t94`
/// defined as `i64`, which clang rejects. Recorded as report.txt P12; the real
/// repair belongs in the backend, and until it lands these are not promoted.
///
/// Only NARROW returns are affected. A call returning `I64`, `F64` or a
/// pointer is emitted at that width on both backends.
pub fn narrow_call_results(func: &IRFunction) -> HashSet<String> {
    let mut out = HashSet::new();
    for block in &func.blocks {
        for instr in &block.instrs {
            let (dst, ret_ty) = match instr {
                IRInstr::Call { dst: Some(d), ret_ty, .. }
                | IRInstr::CallIndirect { dst: Some(d), ret_ty, .. } => (d, ret_ty),
                _ => continue,
            };
            if !matches!(ret_ty, IRType::I64 | IRType::F64 | IRType::Ptr(_)) {
                out.insert(dst.0.clone());
            }
        }
    }
    out
}

/// Whether a value may be stored into a slot of type `slot` without the
/// `Store` performing a conversion that promotion would silently drop.
///
/// A literal is fine at any integer width — the backend materialises it at
/// whatever width it lands in. A TEMP is only fine when its own type already
/// matches, and that is the case this exists for: `store i8 %v` where `%v` is
/// an `i64` call result is a NARROWING, and promoting it produced
/// `phi i8 [ %t94, … ]` with `%t94` defined as `i64` — which clang rejects
/// outright, and which on the other backend would simply have been the wrong
/// value.
fn store_matches_slot(
    val: &IRValue,
    slot: &IRType,
    types: &HashMap<String, IRType>,
    uncertain: &HashSet<String>,
) -> bool {
    match val {
        IRValue::Temp(t) => {
            !uncertain.contains(&t.0) && types.get(&t.0).is_some_and(|vt| vt == slot)
        }
        IRValue::Const(IRConst::Float(_)) => *slot == IRType::F64,
        IRValue::Const(IRConst::Str(_)) | IRValue::Const(IRConst::Null) => {
            matches!(slot, IRType::Ptr(_))
        }
        IRValue::Const(_) => *slot != IRType::F64,
        // The address of a global. Only a pointer-shaped slot can hold one.
        IRValue::Global(_) => matches!(slot, IRType::Ptr(_) | IRType::I64),
        IRValue::Void => false,
    }
}

/// The allocas in a function that `mem2reg` may lift into SSA temps, with the
/// type each holds.
///
/// An alloca qualifies when its address never escapes: every use of it is the
/// `ptr` operand of a `Load` or a `Store`, at the alloca's own type. Anything
/// else — passing it to a call, indexing it with `GetPtr`, storing the pointer
/// itself somewhere — means some other code can reach the memory, and a
/// promoted value would then diverge from what that code sees.
///
/// The type check is not pedantry. This IR does load an `I64` slot through a
/// pointer typed otherwise in places (the uniform 8-byte slot convention), and
/// a promotion that ignored the width would silently change what a `Load`
/// returns.
pub fn promotable_allocas(func: &IRFunction) -> Vec<(String, IRType)> {
    let types = value_types(func);
    let uncertain = narrow_call_results(func);
    let mut candidates: HashMap<&str, IRType> = HashMap::new();
    for block in &func.blocks {
        for instr in &block.instrs {
            if let IRInstr::Alloca { dst, ty } = instr {
                if is_scalar(ty) {
                    candidates.insert(dst.0.as_str(), ty.clone());
                }
            }
        }
    }
    if candidates.is_empty() {
        return Vec::new();
    }

    let mut disqualified: HashSet<&str> = HashSet::new();
    for block in &func.blocks {
        for instr in &block.instrs {
            match instr {
                IRInstr::Load { ptr, ty, .. } => {
                    if let IRValue::Temp(t) = ptr {
                        if candidates.get(t.0.as_str()).is_some_and(|a| a != ty) {
                            disqualified.insert(t.0.as_str());
                        }
                    }
                }
                IRInstr::Store { ptr, val, ty } => {
                    // The POINTER operand is the legitimate use. The VALUE
                    // operand is not: `store %a, %b` puts the address of `%a`
                    // into `%b`'s slot, from which anything may load it.
                    if let IRValue::Temp(t) = ptr {
                        let slot = candidates.get(t.0.as_str());
                        if slot.is_some_and(|a| a != ty) {
                            disqualified.insert(t.0.as_str());
                        } else if let Some(slot) = slot {
                            // The store is at the slot's own width, but the
                            // VALUE may still need converting to reach it, and
                            // that conversion is what the store performs.
                            if !store_matches_slot(val, slot, &types, &uncertain) {
                                disqualified.insert(t.0.as_str());
                            }
                        }
                    }
                    if let IRValue::Temp(t) = val {
                        disqualified.insert(t.0.as_str());
                    }
                }
                other => {
                    for u in instr_uses(other) {
                        disqualified.insert(u);
                    }
                    for (u, _) in phi_uses(other) {
                        disqualified.insert(u);
                    }
                }
            }
        }
        for u in term_uses(&block.term) {
            disqualified.insert(u);
        }
    }

    let mut out: Vec<(String, IRType)> = candidates
        .into_iter()
        .filter(|(name, _)| !disqualified.contains(name))
        .map(|(name, ty)| (name.to_string(), ty))
        .collect();
    // Deterministic order: the pass that consumes this inserts phi nodes, and
    // a HashMap iteration order would make the emitted IR differ run to run.
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Counts that size the work `mem2reg` has to do, for one function.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    pub blocks: usize,
    pub instrs: usize,
    pub temps: usize,
    pub allocas: usize,
    pub promotable: usize,
    pub loads: usize,
    pub stores: usize,
    pub phis: usize,
    /// Edges into a phi block whose predecessor ends in a CONDITIONAL branch.
    ///
    /// Counted because the T3 backend emits phi copies only in its `Jump` arm:
    /// on one of these edges it writes nothing at all, so the merge reads
    /// whatever was left in the phi's home (report.txt P12). Any non-zero
    /// number here is a live miscompile on that backend.
    pub phi_edges_from_branch: usize,
}

impl Stats {
    pub fn of(func: &IRFunction) -> Stats {
        let mut st = Stats { blocks: func.blocks.len(), ..Stats::default() };
        for block in &func.blocks {
            for instr in &block.instrs {
                st.instrs += 1;
                if instr_def(instr).is_some() {
                    st.temps += 1;
                }
                match instr {
                    IRInstr::Alloca { .. } => st.allocas += 1,
                    IRInstr::Load { .. } => st.loads += 1,
                    IRInstr::Store { .. } => st.stores += 1,
                    IRInstr::Phi { .. } => st.phis += 1,
                    _ => {}
                }
            }
        }
        st.promotable = promotable_allocas(func).len();

        let cfg = Cfg::of(func);
        for (bi, block) in func.blocks.iter().enumerate() {
            if !block.instrs.iter().any(|i| matches!(i, IRInstr::Phi { .. })) {
                continue;
            }
            for &p in &cfg.preds[bi] {
                if !matches!(func.blocks[p].term, IRTerminator::Jump(_)) {
                    st.phi_edges_from_branch += 1;
                }
            }
        }
        st
    }

    pub fn add(&mut self, o: &Stats) {
        self.blocks += o.blocks;
        self.instrs += o.instrs;
        self.temps += o.temps;
        self.allocas += o.allocas;
        self.promotable += o.promotable;
        self.loads += o.loads;
        self.stores += o.stores;
        self.phis += o.phis;
        self.phi_edges_from_branch += o.phi_edges_from_branch;
    }

    pub fn of_module(module: &IRModule) -> Stats {
        let mut st = Stats::default();
        for f in &module.functions {
            if !f.is_extern {
                st.add(&Stats::of(f));
            }
        }
        st
    }
}

/// Verify every non-extern function in a module.
///
/// Returns `(function name, violation)` pairs so a report can be grouped
/// either way.
pub fn verify_module(module: &IRModule) -> Vec<(String, Violation)> {
    let mut out = Vec::new();
    for f in &module.functions {
        if f.is_extern {
            continue;
        }
        for v in verify(f) {
            out.push((f.name.clone(), v));
        }
    }
    out
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

    fn assign(dst: &str, v: i64) -> IRInstr {
        IRInstr::Assign {
            dst: IRTemp::new(dst),
            src: IRValue::Const(IRConst::Int(v)),
            ty: IRType::I64,
        }
    }

    fn add(dst: &str, a: &str, b: &str) -> IRInstr {
        IRInstr::BinOp {
            dst: IRTemp::new(dst),
            op: IRBinOp::Add,
            lhs: t(a),
            rhs: t(b),
            ty: IRType::I64,
        }
    }

    /// entry → (then | else) → join. The shape every dominance test needs.
    fn diamond(join_instrs: Vec<IRInstr>) -> IRFunction {
        func(vec![
            block(
                "entry",
                vec![assign("c", 1)],
                IRTerminator::BinBranch {
                    cond: t("c"),
                    true_label: "then".into(),
                    false_label: "else".into(),
                },
            ),
            block("then", vec![assign("a", 1)], IRTerminator::Jump("join".into())),
            block("else", vec![assign("b", 2)], IRTerminator::Jump("join".into())),
            block("join", join_instrs, IRTerminator::Return(Some(t("c")))),
        ])
    }

    #[test]
    fn a_straight_line_function_is_ssa() {
        let f = func(vec![block(
            "entry",
            vec![assign("a", 1), assign("b", 2), add("c", "a", "b")],
            IRTerminator::Return(Some(t("c"))),
        )]);
        assert_eq!(verify(&f), Vec::new());
    }

    #[test]
    fn a_temp_defined_twice_is_reported() {
        let f = func(vec![block(
            "entry",
            vec![assign("a", 1), assign("a", 2)],
            IRTerminator::Return(Some(t("a"))),
        )]);
        let v = verify(&f);
        assert_eq!(v.len(), 1, "{:?}", v);
        assert!(matches!(&v[0], Violation::MultiplyDefined { temp, at }
                         if temp == "a" && at.len() == 2), "{:?}", v);
    }

    #[test]
    fn a_use_before_its_definition_in_the_same_block_is_reported() {
        let f = func(vec![block(
            "entry",
            vec![add("c", "a", "a"), assign("a", 1)],
            IRTerminator::Return(Some(t("c"))),
        )]);
        let v = verify(&f);
        assert!(
            v.iter().any(|x| matches!(x, Violation::NotDominated { temp, .. } if temp == "a")),
            "{:?}", v
        );
    }

    #[test]
    fn an_instruction_may_not_use_the_temp_it_defines() {
        let f = func(vec![block(
            "entry",
            vec![assign("a", 1), add("b", "b", "a")],
            IRTerminator::Return(Some(t("b"))),
        )]);
        let v = verify(&f);
        assert!(
            v.iter().any(|x| matches!(x, Violation::NotDominated { temp, .. } if temp == "b")),
            "a self-referential definition must not verify: {:?}", v
        );
    }

    #[test]
    fn a_value_from_one_arm_of_a_branch_does_not_dominate_the_join() {
        // `a` is defined only on the `then` path but used after the merge.
        // `c` comes from the entry, which does dominate the join, so exactly
        // one of the two operands is at fault.
        let f = diamond(vec![add("d", "a", "c")]);
        let v = verify(&f);
        assert_eq!(v.len(), 1, "{:?}", v);
        assert!(matches!(&v[0], Violation::NotDominated { temp, used_in, defined_in }
                         if temp == "a" && used_in == "join" && defined_in == "then"),
                "{:?}", v);
    }

    #[test]
    fn a_well_formed_phi_at_the_join_is_ssa() {
        let f = diamond(vec![IRInstr::Phi {
            dst: IRTemp::new("d"),
            ty: IRType::I64,
            incoming: vec![(t("a"), "then".into()), (t("b"), "else".into())],
        }]);
        assert_eq!(verify(&f), Vec::new());
    }

    #[test]
    fn a_phi_missing_an_incoming_edge_is_reported() {
        let f = diamond(vec![IRInstr::Phi {
            dst: IRTemp::new("d"),
            ty: IRType::I64,
            incoming: vec![(t("a"), "then".into())],
        }]);
        let v = verify(&f);
        assert!(
            v.iter().any(|x| matches!(x, Violation::PhiEdges { block, .. } if block == "join")),
            "{:?}", v
        );
    }

    #[test]
    fn a_phi_operand_is_checked_at_its_own_edge_not_at_the_phi() {
        // This is the case that a naive verifier gets wrong: `b` is defined in
        // `else`, which does not dominate `join`, and it is still a legal phi
        // operand on the `else` edge.
        let f = diamond(vec![IRInstr::Phi {
            dst: IRTemp::new("d"),
            ty: IRType::I64,
            incoming: vec![(t("a"), "then".into()), (t("b"), "else".into())],
        }]);
        assert_eq!(verify(&f), Vec::new());

        // …and the same operand on the WRONG edge is not.
        let f = diamond(vec![IRInstr::Phi {
            dst: IRTemp::new("d"),
            ty: IRType::I64,
            incoming: vec![(t("b"), "then".into()), (t("a"), "else".into())],
        }]);
        let v = verify(&f);
        assert_eq!(v.len(), 2, "both operands are on the wrong edge: {:?}", v);
    }

    #[test]
    fn a_loop_carried_phi_is_ssa() {
        // entry → head; head → (body | exit); body → head.
        let f = func(vec![
            block("entry", vec![assign("init", 0)], IRTerminator::Jump("head".into())),
            block(
                "head",
                vec![
                    IRInstr::Phi {
                        dst: IRTemp::new("i"),
                        ty: IRType::I64,
                        incoming: vec![(t("init"), "entry".into()), (t("next"), "body".into())],
                    },
                    assign("one", 1),
                ],
                IRTerminator::BinBranch {
                    cond: t("i"),
                    true_label: "body".into(),
                    false_label: "exit".into(),
                },
            ),
            block("body", vec![add("next", "i", "one")], IRTerminator::Jump("head".into())),
            block("exit", Vec::new(), IRTerminator::Return(Some(t("i")))),
        ]);
        assert_eq!(
            verify(&f),
            Vec::new(),
            "a back edge carrying a value defined later in program order is the \
             normal shape of a loop and must verify"
        );
    }

    #[test]
    fn a_parameter_is_a_definition_at_entry() {
        let mut f = func(vec![block(
            "entry",
            vec![add("c", "param_x", "param_x")],
            IRTerminator::Return(Some(t("c"))),
        )]);
        f.params = vec![("x".into(), IRType::I64)];
        assert_eq!(verify(&f), Vec::new());
    }

    #[test]
    fn an_undefined_temp_is_reported_rather_than_assumed() {
        let f = func(vec![block(
            "entry",
            vec![add("c", "nowhere", "nowhere")],
            IRTerminator::Return(Some(t("c"))),
        )]);
        let v = verify(&f);
        assert!(
            v.iter().any(|x| matches!(x, Violation::Undefined { temp, .. } if temp == "nowhere")),
            "{:?}", v
        );
    }

    #[test]
    fn an_unreachable_block_is_not_checked() {
        // `orphan` uses a temp defined nowhere, but nothing branches to it.
        let f = func(vec![
            block("entry", vec![assign("a", 1)], IRTerminator::Return(Some(t("a")))),
            block("orphan", vec![add("z", "ghost", "ghost")], IRTerminator::Unreachable),
        ]);
        assert_eq!(verify(&f), Vec::new());
    }

    #[test]
    fn a_branch_to_a_label_no_block_defines_is_reported() {
        let f = func(vec![block(
            "entry",
            vec![assign("a", 1)],
            IRTerminator::Jump("nowhere".into()),
        )]);
        let v = verify(&f);
        assert!(
            v.iter().any(|x| matches!(x, Violation::DanglingTarget { label, .. } if label == "nowhere")),
            "{:?}", v
        );
    }

    // ---- promotability ----------------------------------------------------

    fn alloca(dst: &str, ty: IRType) -> IRInstr {
        IRInstr::Alloca { dst: IRTemp::new(dst), ty }
    }

    #[test]
    fn a_scalar_alloca_only_loaded_and_stored_is_promotable() {
        let f = func(vec![block(
            "entry",
            vec![
                alloca("p", IRType::I64),
                IRInstr::Store {
                    ptr: t("p"),
                    val: IRValue::Const(IRConst::Int(1)),
                    ty: IRType::I64,
                },
                IRInstr::Load { dst: IRTemp::new("v"), ptr: t("p"), ty: IRType::I64 },
            ],
            IRTerminator::Return(Some(t("v"))),
        )]);
        assert_eq!(promotable_allocas(&f), vec![("p".to_string(), IRType::I64)]);
    }

    #[test]
    fn an_alloca_whose_address_is_passed_to_a_call_is_not_promotable() {
        let f = func(vec![block(
            "entry",
            vec![
                alloca("p", IRType::I64),
                IRInstr::Call {
                    dst: None,
                    func: "f".into(),
                    args: vec![t("p")],
                    ret_ty: IRType::Void,
                },
            ],
            IRTerminator::Return(None),
        )]);
        assert!(promotable_allocas(&f).is_empty(), "the address escapes");
    }

    #[test]
    fn an_alloca_whose_address_is_stored_is_not_promotable() {
        // `store %p, %q` puts p's ADDRESS into q's slot; anything may load it.
        // `q` itself is a pointer slot and stays promotable — the escape is
        // p's, not q's.
        let ptr_ty = IRType::Ptr(Box::new(IRType::I64));
        let f = func(vec![block(
            "entry",
            vec![
                alloca("p", IRType::I64),
                alloca("q", ptr_ty.clone()),
                IRInstr::Store { ptr: t("q"), val: t("p"), ty: ptr_ty },
            ],
            IRTerminator::Return(None),
        )]);
        let promo = promotable_allocas(&f);
        assert!(
            !promo.iter().any(|(n, _)| n == "p"),
            "p's address escapes through the value operand: {:?}",
            promo
        );
        assert!(promo.iter().any(|(n, _)| n == "q"), "{:?}", promo);
    }

    #[test]
    fn a_store_that_narrows_the_value_is_not_promotable() {
        // `store i8 %c` where `%c` is an i64 call result is a NARROWING that
        // the store performs. Promoting it would put the i64 straight into an
        // i8 phi — which clang rejects outright, and which on T3 would simply
        // have been the wrong value. Found on examples/data_structures.mt.
        let f = func(vec![block(
            "entry",
            vec![
                alloca("p", IRType::I8),
                IRInstr::Call {
                    dst: Some(IRTemp::new("c")),
                    func: "g".into(),
                    args: Vec::new(),
                    ret_ty: IRType::I64,
                },
                IRInstr::Store { ptr: t("p"), val: t("c"), ty: IRType::I8 },
            ],
            IRTerminator::Return(None),
        )]);
        assert!(promotable_allocas(&f).is_empty());
    }

    #[test]
    fn a_narrow_call_result_blocks_promotion() {
        // `ret_ty: Trit` in the IR, `call i64` in the emitted LLVM. See
        // `narrow_call_results`. examples/data_structures.mt is where this
        // surfaced.
        let f = func(vec![block(
            "entry",
            vec![
                alloca("p", IRType::Trit),
                IRInstr::Call {
                    dst: Some(IRTemp::new("c")),
                    func: "TernaryTrie::get".into(),
                    args: Vec::new(),
                    ret_ty: IRType::Trit,
                },
                IRInstr::Store { ptr: t("p"), val: t("c"), ty: IRType::Trit },
            ],
            IRTerminator::Return(None),
        )]);
        assert!(promotable_allocas(&f).is_empty());
    }

    #[test]
    fn a_store_of_a_matching_temp_is_still_promotable() {
        let f = func(vec![block(
            "entry",
            vec![
                alloca("p", IRType::I64),
                IRInstr::Call {
                    dst: Some(IRTemp::new("c")),
                    func: "g".into(),
                    args: Vec::new(),
                    ret_ty: IRType::I64,
                },
                IRInstr::Store { ptr: t("p"), val: t("c"), ty: IRType::I64 },
            ],
            IRTerminator::Return(None),
        )]);
        assert_eq!(promotable_allocas(&f), vec![("p".to_string(), IRType::I64)]);
    }

    #[test]
    fn an_alloca_reached_through_getptr_is_not_promotable() {
        let f = func(vec![block(
            "entry",
            vec![
                alloca("p", IRType::I64),
                IRInstr::GetPtr {
                    dst: IRTemp::new("e"),
                    ptr: t("p"),
                    idx: IRValue::Const(IRConst::Int(0)),
                    ty: IRType::I64,
                },
            ],
            IRTerminator::Return(None),
        )]);
        assert!(promotable_allocas(&f).is_empty());
    }

    #[test]
    fn an_alloca_accessed_at_a_different_width_is_not_promotable() {
        // The uniform 8-byte slot convention means a pointer is sometimes read
        // at a width other than the one it was allocated at. Promoting that
        // would silently change what the Load returns.
        let f = func(vec![block(
            "entry",
            vec![
                alloca("p", IRType::I8),
                IRInstr::Load { dst: IRTemp::new("v"), ptr: t("p"), ty: IRType::I64 },
            ],
            IRTerminator::Return(Some(t("v"))),
        )]);
        assert!(promotable_allocas(&f).is_empty());
    }

    #[test]
    fn an_aggregate_alloca_is_not_a_candidate() {
        let f = func(vec![block(
            "entry",
            vec![alloca("p", IRType::Struct("S".into()))],
            IRTerminator::Return(None),
        )]);
        assert!(promotable_allocas(&f).is_empty());
    }

    #[test]
    fn the_promotable_list_is_deterministic() {
        let f = func(vec![block(
            "entry",
            vec![
                alloca("z", IRType::I64),
                alloca("a", IRType::I64),
                alloca("m", IRType::I64),
            ],
            IRTerminator::Return(None),
        )]);
        let names: Vec<String> = promotable_allocas(&f).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, vec!["a", "m", "z"], "phi placement must not depend on hash order");
    }

    // ---- critical-edge splitting -----------------------------------------

    #[test]
    fn a_diamond_has_no_critical_edges() {
        // entry has two successors but then/else each have one predecessor;
        // then/else have one successor each. Nothing to split.
        let mut f = diamond(Vec::new());
        assert_eq!(split_critical_edges(&mut f), 0);
        assert_eq!(f.blocks.len(), 4);
    }

    #[test]
    fn a_branch_straight_to_a_join_is_a_critical_edge() {
        // entry → {then, join}; join also has `then` as a predecessor. The
        // entry→join edge is critical: entry cannot put a value on it without
        // also putting it on entry→then.
        let mut f = func(vec![
            block(
                "entry",
                vec![assign("c", 1)],
                IRTerminator::BinBranch {
                    cond: t("c"),
                    true_label: "then".into(),
                    false_label: "join".into(),
                },
            ),
            block("then", vec![assign("a", 1)], IRTerminator::Jump("join".into())),
            block(
                "join",
                vec![IRInstr::Phi {
                    dst: IRTemp::new("p"),
                    ty: IRType::I64,
                    incoming: vec![(t("a"), "then".into()), (t("c"), "entry".into())],
                }],
                IRTerminator::Return(Some(t("p"))),
            ),
        ]);
        assert_eq!(split_critical_edges(&mut f), 1);
        assert_eq!(f.blocks.len(), 4);

        // The phi now names the new block, and the new block jumps to the join.
        let join = f.blocks.iter().find(|b| b.label == "join").unwrap();
        let IRInstr::Phi { incoming, .. } = &join.instrs[0] else { panic!() };
        let labels: HashSet<&str> = incoming.iter().map(|(_, l)| l.as_str()).collect();
        assert!(labels.contains("then"), "{:?}", incoming);
        assert!(!labels.contains("entry"), "the entry edge was split: {:?}", incoming);
        let new_label = incoming.iter().find(|(_, l)| l != "then").unwrap().1.clone();
        let nb = f.blocks.iter().find(|b| b.label == new_label).expect("the split block");
        assert!(nb.instrs.is_empty());
        assert!(matches!(&nb.term, IRTerminator::Jump(l) if l == "join"));

        assert_eq!(verify(&f), Vec::new(), "splitting must preserve SSA");
    }

    #[test]
    fn splitting_leaves_every_phi_predecessor_ending_in_a_jump() {
        let mut f = func(vec![
            block(
                "entry",
                vec![assign("c", 1)],
                IRTerminator::BinBranch {
                    cond: t("c"),
                    true_label: "join".into(),
                    false_label: "other".into(),
                },
            ),
            block(
                "other",
                vec![assign("b", 2)],
                IRTerminator::BinBranch {
                    cond: t("c"),
                    true_label: "join".into(),
                    false_label: "exit".into(),
                },
            ),
            block(
                "join",
                vec![IRInstr::Phi {
                    dst: IRTemp::new("p"),
                    ty: IRType::I64,
                    incoming: vec![(t("c"), "entry".into()), (t("b"), "other".into())],
                }],
                IRTerminator::Return(Some(t("p"))),
            ),
            block("exit", Vec::new(), IRTerminator::Return(None)),
        ]);
        assert_eq!(split_critical_edges(&mut f), 2);
        let cfg = Cfg::of(&f);
        let join = cfg.index["join"];
        for &p in &cfg.preds[join] {
            assert!(
                matches!(f.blocks[p].term, IRTerminator::Jump(_)),
                "{} still branches into a phi block",
                f.blocks[p].label
            );
        }
        assert_eq!(verify(&f), Vec::new());
    }

    #[test]
    fn splitting_is_idempotent() {
        let mut f = func(vec![
            block(
                "entry",
                vec![assign("c", 1)],
                IRTerminator::BinBranch {
                    cond: t("c"),
                    true_label: "then".into(),
                    false_label: "join".into(),
                },
            ),
            block("then", Vec::new(), IRTerminator::Jump("join".into())),
            block("join", Vec::new(), IRTerminator::Return(None)),
        ]);
        assert_eq!(split_critical_edges(&mut f), 1);
        assert_eq!(split_critical_edges(&mut f), 0, "a second pass has nothing to do");
    }

    // ---- dominance itself, independently of the verifier ------------------

    #[test]
    fn the_entry_dominates_every_reachable_block() {
        let f = diamond(Vec::new());
        let cfg = Cfg::of(&f);
        let d = Dominators::of(&cfg);
        for b in 0..cfg.len() {
            assert!(d.dominates(0, b), "entry must dominate block {}", b);
        }
    }

    #[test]
    fn neither_arm_of_a_diamond_dominates_the_join() {
        let f = diamond(Vec::new());
        let cfg = Cfg::of(&f);
        let d = Dominators::of(&cfg);
        let (then_b, else_b, join) = (cfg.index["then"], cfg.index["else"], cfg.index["join"]);
        assert!(!d.dominates(then_b, join));
        assert!(!d.dominates(else_b, join));
        assert_eq!(d.idom[join], Some(cfg.index["entry"]));
    }

    #[test]
    fn the_dominance_frontier_of_a_diamond_arm_is_the_join() {
        let f = diamond(Vec::new());
        let cfg = Cfg::of(&f);
        let d = Dominators::of(&cfg);
        let df = d.frontiers(&cfg);
        let join = cfg.index["join"];
        assert_eq!(df[cfg.index["then"]], HashSet::from([join]));
        assert_eq!(df[cfg.index["else"]], HashSet::from([join]));
        assert!(df[cfg.index["entry"]].is_empty());
        assert!(df[join].is_empty());
    }

    #[test]
    fn a_loop_head_is_in_its_own_bodys_frontier() {
        let f = func(vec![
            block("entry", Vec::new(), IRTerminator::Jump("head".into())),
            block(
                "head",
                vec![assign("c", 1)],
                IRTerminator::BinBranch {
                    cond: t("c"),
                    true_label: "body".into(),
                    false_label: "exit".into(),
                },
            ),
            block("body", Vec::new(), IRTerminator::Jump("head".into())),
            block("exit", Vec::new(), IRTerminator::Return(None)),
        ]);
        let cfg = Cfg::of(&f);
        let d = Dominators::of(&cfg);
        let df = d.frontiers(&cfg);
        let head = cfg.index["head"];
        assert_eq!(df[cfg.index["body"]], HashSet::from([head]));
        assert_eq!(
            df[head],
            HashSet::from([head]),
            "the head is in its own frontier — that is what puts the loop phi there"
        );
    }
}
