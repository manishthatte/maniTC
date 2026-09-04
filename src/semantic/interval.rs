//! B6 — integer intervals, the decidable fragment refinement types check over.
//!
//! © Manish Jagdish Thatte
//!
//! An `Interval` is what the checker knows about a value: a closed range, open
//! on either side where nothing is known. `None`/`None` is "anything", which is
//! the answer for every expression the fragment does not cover, and the reason
//! this checker REFUTES rather than proves — it reports what it can show is
//! wrong and is silent otherwise.
//!
//! **Every operation saturates to unknown rather than wrapping.** An interval
//! arithmetic that overflowed `i64` silently would compute a bound smaller than
//! the true one and then "prove" a violation that is not there — a false
//! rejection, which is the one failure mode a checker like this must not have.
//! `checked_*` is used throughout and its `None` widens the answer.

/// What is known about an integer value: a closed interval, open where nothing
/// is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interval {
    pub lo: Option<i64>,
    pub hi: Option<i64>,
}

impl Interval {
    pub const UNKNOWN: Interval = Interval { lo: None, hi: None };

    pub fn exact(v: i64) -> Interval {
        Interval { lo: Some(v), hi: Some(v) }
    }

    pub fn new(lo: Option<i64>, hi: Option<i64>) -> Interval {
        Interval { lo, hi }
    }

    /// Is anything known at all?
    pub fn is_unknown(&self) -> bool {
        self.lo.is_none() && self.hi.is_none()
    }

    /// The interval contains no value — `10 <= x <= 5`.
    ///
    /// A declaration whose refinement is empty can never be satisfied by any
    /// call, so it is refused where it is written rather than at every call
    /// site that fails.
    pub fn is_empty(&self) -> bool {
        matches!((self.lo, self.hi), (Some(l), Some(h)) if l > h)
    }

    /// Is every value of `self` inside `other`?
    ///
    /// `false` when `self` is open on a side `other` bounds — not knowing is
    /// not the same as violating, and this predicate is only ever used to
    /// ACCEPT, never to reject.
    pub fn within(&self, other: &Interval) -> bool {
        if let Some(ol) = other.lo {
            match self.lo {
                Some(sl) if sl >= ol => {}
                _ => return false,
            }
        }
        if let Some(oh) = other.hi {
            match self.hi {
                Some(sh) if sh <= oh => {}
                _ => return false,
            }
        }
        true
    }

    /// Do `self` and `other` share no value at all?
    ///
    /// This is the REJECTION predicate, and it is deliberately the complement
    /// of `within` rather than its negation: a value that might be inside and
    /// might be outside is neither accepted nor refused.
    pub fn disjoint_from(&self, other: &Interval) -> bool {
        if let (Some(sh), Some(ol)) = (self.hi, other.lo) {
            if sh < ol {
                return true;
            }
        }
        if let (Some(sl), Some(oh)) = (self.lo, other.hi) {
            if sl > oh {
                return true;
            }
        }
        false
    }

    pub fn add(&self, o: &Interval) -> Interval {
        Interval {
            lo: opt2(self.lo, o.lo, |a, b| a.checked_add(b)),
            hi: opt2(self.hi, o.hi, |a, b| a.checked_add(b)),
        }
    }

    pub fn sub(&self, o: &Interval) -> Interval {
        // Subtraction FLIPS the operand's bounds: the smallest `a - b` is the
        // smallest `a` minus the LARGEST `b`. Getting this backwards produces
        // an interval narrower than the truth, which is the false-rejection
        // failure mode this module exists to avoid.
        Interval {
            lo: opt2(self.lo, o.hi, |a, b| a.checked_sub(b)),
            hi: opt2(self.hi, o.lo, |a, b| a.checked_sub(b)),
        }
    }

    /// Multiplication needs all four corners, because a negative operand
    /// swaps which end is which.
    pub fn mul(&self, o: &Interval) -> Interval {
        let (Some(al), Some(ah), Some(bl), Some(bh)) = (self.lo, self.hi, o.lo, o.hi) else {
            return Interval::UNKNOWN;
        };
        let mut corners = Vec::with_capacity(4);
        for a in [al, ah] {
            for b in [bl, bh] {
                match a.checked_mul(b) {
                    Some(v) => corners.push(v),
                    None => return Interval::UNKNOWN,
                }
            }
        }
        Interval {
            lo: corners.iter().copied().min(),
            hi: corners.iter().copied().max(),
        }
    }

    pub fn neg(&self) -> Interval {
        Interval {
            lo: self.hi.and_then(|v| v.checked_neg()),
            hi: self.lo.and_then(|v| v.checked_neg()),
        }
    }

    /// Render as the interval notation a diagnostic can quote.
    pub fn display(&self) -> String {
        match (self.lo, self.hi) {
            (Some(l), Some(h)) if l == h => format!("{l}"),
            (Some(l), Some(h)) => format!("{l}..{h}"),
            (Some(l), None) => format!("{l}.."),
            (None, Some(h)) => format!("..{h}"),
            (None, None) => "unknown".to_string(),
        }
    }
}

fn opt2(a: Option<i64>, b: Option<i64>, f: impl Fn(i64, i64) -> Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(x), Some(y)) => f(x, y),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtraction_flips_the_operand_bounds() {
        // [0,10] - [1,2] is [-2, 9], not [-1, 8].
        let r = Interval::new(Some(0), Some(10)).sub(&Interval::new(Some(1), Some(2)));
        assert_eq!(r, Interval::new(Some(-2), Some(9)));
    }

    #[test]
    fn multiplication_considers_every_corner() {
        // [-3,2] * [-4,5] spans -15 .. 12, and only the corner scan finds both.
        let r = Interval::new(Some(-3), Some(2)).mul(&Interval::new(Some(-4), Some(5)));
        assert_eq!(r, Interval::new(Some(-15), Some(12)));
    }

    #[test]
    fn overflow_widens_to_unknown_rather_than_wrapping() {
        let big = Interval::exact(i64::MAX);
        assert!(big.add(&Interval::exact(1)).is_unknown());
        assert!(big.mul(&Interval::exact(2)).is_unknown());
    }

    #[test]
    fn not_knowing_is_neither_within_nor_disjoint() {
        let unknown = Interval::UNKNOWN;
        let bound = Interval::new(Some(-100), Some(100));
        assert!(!unknown.within(&bound), "unknown must not be accepted");
        assert!(!unknown.disjoint_from(&bound), "unknown must not be refused");
    }

    #[test]
    fn disjointness_is_decided_on_both_sides() {
        let bound = Interval::new(Some(-100), Some(100));
        assert!(Interval::exact(500).disjoint_from(&bound));
        assert!(Interval::exact(-500).disjoint_from(&bound));
        assert!(!Interval::exact(0).disjoint_from(&bound));
        // Touching the boundary is inside it.
        assert!(!Interval::exact(100).disjoint_from(&bound));
        assert!(!Interval::exact(-100).disjoint_from(&bound));
    }

    #[test]
    fn an_inverted_interval_is_empty() {
        assert!(Interval::new(Some(10), Some(5)).is_empty());
        assert!(!Interval::new(Some(5), Some(10)).is_empty());
        assert!(!Interval::new(Some(5), None).is_empty());
    }
}
