// stdlib/std/test.mt
// Assertions for maniT test programs.
//
// Usage:
//   use std::test;
//   test::assert(x == 3, "x is three");
//
// WHY THIS MODULE IS PURE ManiT, not native. An assertion needs exactly two
// primitives -- print a message and stop with a non-zero status -- and both
// already exist on both backends (`io::println`, `env::exit`, which the T3
// emitter lowers to SYSCALL #550). Writing it natively would mean a C runtime
// symbol, and a C runtime symbol is precisely what T3 does not have: that is
// the whole reason gui_* programs fail to assemble for T3 while linking
// happily for LLVM. As ManiT source it is compiled into the program by
// stdlib_expand and both backends get it from one definition.
//
// WHY THE CONDITION IS bool3 AND NOT bool. maniT comparisons yield `bool`,
// which coerces to bool3 (+1 / -1), but `tand`, `tor` and comparisons against
// `unknown` yield a genuine three-valued result. If `assert` took a `bool`,
// the `unknown` case would have to collapse into one of the two binary
// answers before the assertion ever saw it -- and an assertion that cannot
// tell "false" from "not known to be true" is worse than no assertion, because
// it reports a definite verdict it does not have. So the condition is bool3
// and there are three outcomes, one per trit:
//
//     +1  true      the assertion holds        -> pass
//      0  unknown   the assertion is undecided -> FAIL, reported as `unknown`
//     -1  false     the assertion is violated  -> FAIL, reported as `false`
//
// Both failures exit(1), but they print differently, because "your logic is
// wrong" and "your logic never resolved" are different bugs.

// ---------------------------------------------------------------------------
// Failure reporting
// ---------------------------------------------------------------------------

// Print an assertion failure and terminate with a non-zero exit status.
// Separated from `assert` so every entry point below reports identically.
fn fail(kind: str, msg: str) {
    io::print("ASSERTION FAILED [");
    io::print(kind);
    io::print("] ");
    io::println(msg);
    env::exit(1);
}

// The passing arm. `tif` arms are expressions, so each of the three needs
// something to evaluate; this is the nothing.
fn pass() { }

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

// Assert that `cond` is TRUE (+1). `unknown` and `false` both fail, and are
// reported separately.
fn assert(cond: bool3, msg: str) {
    tif cond {
        + => pass(),
        0 => fail("unknown", msg),
        - => fail("false", msg),
    }
}

// Assert that `cond` is UNKNOWN (0) -- that the question is genuinely
// undecided. There is no binary equivalent of this assertion; it is the one
// that makes a three-valued logic testable at all.
fn assert_unknown(cond: bool3, msg: str) {
    tif cond {
        + => fail("true, expected unknown", msg),
        0 => pass(),
        - => fail("false, expected unknown", msg),
    }
}

// Assert that `cond` is FALSE (-1) -- definitely false, not merely not-true.
fn assert_false(cond: bool3, msg: str) {
    tif cond {
        + => fail("true, expected false", msg),
        0 => fail("unknown, expected false", msg),
        - => pass(),
    }
}
