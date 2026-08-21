// emulator/ordered.rs — Map and Set that iterate in INSERTION ORDER.
//
// Iteration order is part of the maniT language, not a property of whatever
// structure a backend happens to store the entries in.  Without that rule the
// same program prints different things on the two backends, which is the one
// thing the two-backend design exists to prevent.  Measured, inserting
// 50, 10, 30, 20, 40:
//
//     LLVM  40 50 30 20 10     <- open-addressed hash table, walked slot 0..cap
//     T3    10 20 30 40 50     <- BTreeMap, walked in key order
//
// Neither is a property of the program.  Insertion order is, and it is also
// the ONLY order the two backends can agree on without knowing the key type:
// keys reach the runtime type-erased as i64, and a string key is a pointer on
// LLVM but an intern id on T3, so any value-based ordering (sorted, say) is a
// different order on each side the moment a key is not an int.
//
// The BTreeMap/BTreeSet is kept for lookup, so containment stays logarithmic;
// `order` carries the sequence.  A re-inserted key keeps its original
// position — it was already present, so its insertion has already happened.
//
// Author: Manish Jagdish Thatte

use std::collections::{BTreeMap, BTreeSet};

/// A map whose iteration order is the order keys were first inserted in.
#[derive(Default, Clone)]
pub(super) struct OrderedMap {
    entries: BTreeMap<i64, i64>,
    order: Vec<i64>,
}

impl OrderedMap {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn insert(&mut self, k: i64, v: i64) {
        if self.entries.insert(k, v).is_none() {
            self.order.push(k);
        }
    }

    pub(super) fn get(&self, k: &i64) -> Option<&i64> {
        self.entries.get(k)
    }

    pub(super) fn contains_key(&self, k: &i64) -> bool {
        self.entries.contains_key(k)
    }

    pub(super) fn remove(&mut self, k: &i64) {
        if self.entries.remove(k).is_some() {
            self.order.retain(|e| e != k);
        }
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Keys in insertion order.
    pub(super) fn keys(&self) -> Vec<i64> {
        self.order.clone()
    }

    /// Values in the same order as `keys`, so the two can be paired by index.
    pub(super) fn values(&self) -> Vec<i64> {
        self.order
            .iter()
            .filter_map(|k| self.entries.get(k).copied())
            .collect()
    }
}

/// A set whose iteration order is the order elements were first inserted in.
#[derive(Default, Clone)]
pub(super) struct OrderedSet {
    entries: BTreeSet<i64>,
    order: Vec<i64>,
}

impl OrderedSet {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn insert(&mut self, x: i64) {
        if self.entries.insert(x) {
            self.order.push(x);
        }
    }

    pub(super) fn contains(&self, x: &i64) -> bool {
        self.entries.contains(x)
    }

    pub(super) fn remove(&mut self, x: &i64) {
        if self.entries.remove(x) {
            self.order.retain(|e| e != x);
        }
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Elements in insertion order.
    pub(super) fn iter(&self) -> Vec<i64> {
        self.order.clone()
    }

    /// Elements of `self` that are also in `other`, in SELF's order.
    pub(super) fn intersection(&self, other: &OrderedSet) -> OrderedSet {
        self.filtered(|x| other.contains(x))
    }

    /// Elements of `self` that are not in `other`, in SELF's order.
    pub(super) fn difference(&self, other: &OrderedSet) -> OrderedSet {
        self.filtered(|x| !other.contains(x))
    }

    /// All of `self` in self's order, then whatever `other` adds, in other's.
    pub(super) fn union(&self, other: &OrderedSet) -> OrderedSet {
        let mut out = self.clone();
        for x in &other.order {
            out.insert(*x);
        }
        out
    }

    pub(super) fn is_subset(&self, other: &OrderedSet) -> bool {
        self.entries.is_subset(&other.entries)
    }

    pub(super) fn is_superset(&self, other: &OrderedSet) -> bool {
        self.entries.is_superset(&other.entries)
    }

    pub(super) fn is_disjoint(&self, other: &OrderedSet) -> bool {
        self.entries.is_disjoint(&other.entries)
    }

    fn filtered(&self, keep: impl Fn(&i64) -> bool) -> OrderedSet {
        let mut out = OrderedSet::new();
        for x in &self.order {
            if keep(x) {
                out.insert(*x);
            }
        }
        out
    }
}
