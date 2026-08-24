//! The language version a compilation targets.
//!
//! Recommendation R2. Two changes in this family — round-to-nearest division
//! (C4) and `int` meaning 27 trits on every backend (N5) — alter what existing,
//! correct programs compute. R2's condition for making them is that a program
//! must be able to say which language it is written in, that both behaviours be
//! available while code moves across, and that there be a way to FIND every
//! site that needs looking at before anything moves.
//!
//! This module is the first of those three. The second is the pair of IR
//! operations `DivNear`/`RemNear`, which exist alongside the truncating pair
//! rather than replacing it. The third is the `division-semantics` lint
//! (`lint.rs`), which is `allow` by default and lists every `/` and `%` on an
//! integer type when asked — the migration backlog, generated on demand, in
//! exactly the way `undeclared-native` generates A1's.
//!
//! **V1 remains the default.** R2 argues that delay is preferable to doing a
//! change of this kind casually, and flipping the default in the same change
//! that introduces the behaviour would be doing it casually. `--lang v2` opts
//! in; a later release decides whether the default moves.
//!
//! © Manish Jagdish Thatte

use std::fmt;

/// Largest value a 27-trit balanced-ternary word can hold, `(3^27 − 1) / 2`.
///
/// The range is symmetric — `T27_MIN` is exactly `−T27_MAX` — which is not a
/// coincidence to be maintained but a property of the representation: there is
/// no sign bit, so there is no extra negative value and no minimum whose
/// negation overflows. Every `abs`, `neg` and rounding rule below is total
/// because of it.
///
/// `codegen_t3::isa::T3_MAX` is the same number reached from the other side —
/// the machine's word rather than the language's `int`. Under V2 they are
/// required to agree, and `the_language_word_matches_the_machine_word` fails
/// the build if they ever stop.
pub const T27_MAX: i64 = 3_812_798_742_493;
/// Smallest value a 27-trit balanced-ternary word can hold.
pub const T27_MIN: i64 = -T27_MAX;

/// Which version of the maniT language a compilation is written in.
///
/// Ordered, and the ordering is chronological: `V1 < V2`. Feature predicates
/// are written as methods rather than as comparisons at the use sites, so that
/// a third version can turn one off again without every site being wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum LangVersion {
    /// The language as it shipped: `/` truncates, `int` is the target's word.
    #[default]
    V1,
    /// C4 and N5: `/` rounds to nearest, `int` is 27 trits everywhere.
    V2,
}

impl LangVersion {
    /// The spelling used by `--lang` and recorded in diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            LangVersion::V1 => "v1",
            LangVersion::V2 => "v2",
        }
    }

    /// Parse a `--lang` argument. `None` for anything unrecognised — the
    /// caller reports it rather than silently compiling the default, because a
    /// typo that quietly selected V1 would produce a program whose arithmetic
    /// is not the one its author asked for.
    pub fn from_name(s: &str) -> Option<LangVersion> {
        match s {
            "v1" | "V1" | "1" => Some(LangVersion::V1),
            "v2" | "V2" | "2" => Some(LangVersion::V2),
            _ => None,
        }
    }

    /// Every version, for diagnostics that list the alternatives.
    pub fn all() -> &'static [LangVersion] {
        &[LangVersion::V1, LangVersion::V2]
    }

    /// C4: whether the surface `/` and `%` on an integer type round to nearest.
    ///
    /// The two operators move TOGETHER. `(a / b) * b + (a % b) == a` holds
    /// today and would stop holding if `/` rounded while `%` truncated, so the
    /// modes are pairs — `(div_nearest, rem_balanced)` and
    /// `(div_trunc, rem_trunc)` — and the identity holds in both.
    pub fn division_rounds_to_nearest(self) -> bool {
        self >= LangVersion::V2
    }

    /// N5: whether `int` is a 27-trit word on every backend.
    ///
    /// Under V1 it is i64 on LLVM and 27 trits on T3, so a value in
    /// `(T27_MAX, 2^63−1]` exists on one backend and not on the other:
    /// `let m: int = 3812798742493; m + 1` traps on T3 and yields
    /// 3812798742494 on LLVM. Under V2 the LLVM backend range-checks `int`
    /// arithmetic so both agree, and `trint` is the wider type for code that
    /// wants the machine word.
    pub fn int_is_27_trits(self) -> bool {
        self >= LangVersion::V2
    }
}

// ---------------------------------------------------------------------------
// C4 — the rounding rule
// ---------------------------------------------------------------------------

/// `a / b` rounded to the nearest integer, ties away from zero (C4).
///
/// **Why round at all.** `/` truncates today because C does. In balanced
/// ternary that is the wrong default twice over: dropping low trits IS
/// rounding to nearest — `TSHR` already rounds correctly — and truncation
/// throws that away to imitate a representation this machine does not use.
///
/// **Why ties away from zero.** The alternative considered was round-half-to-
/// even, which is the statistically unbiased tie-break. It was rejected because
/// balanced ternary's unbiasedness claim comes from the REPRESENTATION, not
/// from the tie-break, and the property worth preserving here is the symmetry
/// the representation already has: `div_nearest(-a, b) == -div_nearest(a, b)`
/// for every `a` and every non-zero `b`. Half-to-even does not have it.
///
/// `b` must be non-zero; every caller has already trapped or refused a zero
/// divisor, because a division by zero is a fault rather than a value and this
/// function has no way to report one.
pub fn div_nearest(a: i64, b: i64) -> i64 {
    debug_assert!(b != 0, "div_nearest requires a non-zero divisor");
    let q = a.wrapping_div(b);
    let r = a.wrapping_rem(b);
    // NEGATIVE magnitudes, deliberately, and this is the whole subtlety of the
    // function. The test to make is `2|r| >= |b|`, and both the doubling and
    // the `abs` can overflow: `i64::MIN` has no representable positive
    // magnitude. `−|x|` always does. Rewritten over negative magnitudes the
    // test is total for every pair of machine integers with no widening and
    // no special case:
    //
    //     2|r| >= |b|   ⟺   |r| >= |b| − |r|   ⟺   −|r| <= −(|b| − |r|)
    //
    // and `−(|b| − |r|)` is `nb − nr`, whose value lies in [i64::MIN, −1]
    // because `0 < |b| − |r| <= |b|`. This is also exactly the form both
    // backends emit — one instruction on T3, a straight-line select chain on
    // LLVM — so there is no place where the rule is stated twice in two
    // shapes and only one of them audited.
    let nr = if r > 0 { -r } else { r };
    let nb = if b > 0 { -b } else { b };
    if nr <= nb - nr {
        // Away from zero means away from the QUOTIENT's sign, which is the
        // sign of the two operands taken together, not of either one.
        if (a < 0) == (b < 0) {
            q.wrapping_add(1)
        } else {
            q.wrapping_sub(1)
        }
    } else {
        q
    }
}

/// The remainder that pairs with [`div_nearest`] — the balanced remainder.
///
/// Defined as `a - div_nearest(a, b) * b`, which is the only definition that
/// keeps `(a / b) * b + (a % b) == a` true. That identity holds today and
/// would have broken if `/` had been changed to round while `%` went on
/// truncating, which is why C4 is a change to BOTH operators and why the two
/// modes are pairs: `(div_nearest, rem_balanced)` and `(div_trunc, rem_trunc)`.
///
/// The result lies in `[-|b|/2, +|b|/2]` — it can be negative for a positive
/// `a`, which the truncating remainder never is. `7 % 2` is `-1` under V2 and
/// `+1` under V1, and `4 * 2 + (-1) == 7` either way.
pub fn rem_balanced(a: i64, b: i64) -> i64 {
    debug_assert!(b != 0, "rem_balanced requires a non-zero divisor");
    a.wrapping_sub(div_nearest(a, b).wrapping_mul(b))
}

impl fmt::Display for LangVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_v1() {
        assert_eq!(LangVersion::default(), LangVersion::V1);
        assert!(!LangVersion::default().division_rounds_to_nearest());
        assert!(!LangVersion::default().int_is_27_trits());
    }

    #[test]
    fn names_round_trip() {
        for v in LangVersion::all() {
            assert_eq!(LangVersion::from_name(v.as_str()), Some(*v));
        }
        assert_eq!(LangVersion::from_name("v3"), None);
        assert_eq!(LangVersion::from_name(""), None);
    }

    #[test]
    fn v2_turns_both_features_on() {
        assert!(LangVersion::V2.division_rounds_to_nearest());
        assert!(LangVersion::V2.int_is_27_trits());
    }

    #[test]
    fn the_word_range_is_symmetric() {
        assert_eq!(T27_MIN, -T27_MAX);
        // (3^27 − 1) / 2, computed rather than restated.
        assert_eq!(T27_MAX, (3i64.pow(27) - 1) / 2);
    }

    /// The seven cases worked out by hand when the rule was chosen. Restated
    /// here as the executable form of that derivation.
    #[test]
    fn the_hand_verified_cases() {
        let cases = [
            (7, 2, 4, -1),
            (-7, 2, -4, 1),
            (1, 3, 0, 1),
            (2, 3, 1, -1),
            (-2, 3, -1, 1),
            (5, 3, 2, -1),
            (4, 3, 1, 1),
        ];
        for (a, b, q, r) in cases {
            assert_eq!(div_nearest(a, b), q, "{} / {}", a, b);
            assert_eq!(rem_balanced(a, b), r, "{} % {}", a, b);
        }
    }

    #[test]
    fn the_division_identity_holds_in_both_modes() {
        for a in -60i64..=60 {
            for b in -9i64..=9 {
                if b == 0 {
                    continue;
                }
                let (q, r) = (div_nearest(a, b), rem_balanced(a, b));
                assert_eq!(q * b + r, a, "nearest identity at {} / {}", a, b);
                assert_eq!((a / b) * b + (a % b), a, "truncating identity at {} / {}", a, b);
            }
        }
    }

    #[test]
    fn rounding_is_symmetric_about_zero() {
        for a in -200i64..=200 {
            for b in [-7i64, -3, -2, -1, 1, 2, 3, 7] {
                assert_eq!(
                    div_nearest(-a, b),
                    -div_nearest(a, b),
                    "symmetry at {} / {}",
                    a,
                    b
                );
            }
        }
    }

    /// The rounding is genuinely to NEAREST: no other integer is closer, and
    /// the tie goes to the larger magnitude. Checked against exact rational
    /// arithmetic in i128 rather than against a second implementation of the
    /// same idea.
    #[test]
    fn no_integer_is_closer_than_the_one_chosen() {
        for a in -100i64..=100 {
            for b in [-9i64, -5, -3, -2, -1, 1, 2, 3, 5, 9] {
                let q = div_nearest(a, b) as i128;
                let (a128, b128) = (a as i128, b as i128);
                // |a - q*b| is the error, scaled by |b|; compare it with the
                // neighbours on either side.
                let err = (a128 - q * b128).abs();
                for cand in [q - 1, q + 1] {
                    let e = (a128 - cand * b128).abs();
                    assert!(e >= err, "{}/{}: {} is closer than {}", a, b, cand, q);
                    if e == err {
                        assert!(
                            q.abs() > cand.abs(),
                            "{}/{}: tie between {} and {} did not go away from zero",
                            a, b, q, cand
                        );
                    }
                }
            }
        }
    }

    /// The balanced remainder is at most half the divisor in magnitude — the
    /// property that makes it "balanced", and the reason it can be negative
    /// where the truncating remainder never is.
    #[test]
    fn the_balanced_remainder_is_at_most_half_the_divisor() {
        for a in -400i64..=400 {
            for b in [-11i64, -4, -3, -2, -1, 1, 2, 3, 4, 11] {
                let r = rem_balanced(a, b) as i128;
                assert!(
                    r.abs() * 2 <= (b as i128).abs(),
                    "{} % {} = {} exceeds half the divisor",
                    a, b, r
                );
            }
        }
    }

    /// The negative-magnitude form against the obvious i128 one, over the
    /// awkward values as well as the ordinary ones.
    ///
    /// The two agree everywhere, which is the claim the comment in
    /// `div_nearest` makes; if a later edit reintroduces `abs`, this is what
    /// notices. `i64::MIN / -1` is excluded because it is not division
    /// rounding at all — the quotient is not representable, and both `sdiv`
    /// and the machine already have their own answer for it.
    #[test]
    fn the_negative_magnitude_form_matches_the_widened_one() {
        fn reference(a: i64, b: i64) -> i64 {
            let q = a.wrapping_div(b);
            let r = a.wrapping_rem(b);
            let ar = (r as i128).unsigned_abs();
            let ab = (b as i128).unsigned_abs();
            if ar * 2 >= ab {
                if (a < 0) == (b < 0) { q.wrapping_add(1) } else { q.wrapping_sub(1) }
            } else {
                q
            }
        }
        let interesting = [
            i64::MIN, i64::MIN + 1, -T27_MAX, -1_000_000, -7, -3, -2, -1,
            0, 1, 2, 3, 7, 1_000_000, T27_MAX, i64::MAX - 1, i64::MAX,
        ];
        for &a in &interesting {
            for &b in &interesting {
                if b == 0 || (a == i64::MIN && b == -1) {
                    continue;
                }
                assert_eq!(
                    div_nearest(a, b),
                    reference(a, b),
                    "div_nearest disagrees with the widened form at {} / {}",
                    a, b
                );
            }
        }
        for a in -300i64..=300 {
            for b in -13i64..=13 {
                if b == 0 {
                    continue;
                }
                assert_eq!(div_nearest(a, b), reference(a, b), "{} / {}", a, b);
            }
        }
    }

    #[test]
    fn the_language_word_matches_the_machine_word() {
        assert_eq!(T27_MAX, crate::codegen_t3::isa::T3_MAX);
        assert_eq!(T27_MIN, crate::codegen_t3::isa::T3_MIN);
    }
}
