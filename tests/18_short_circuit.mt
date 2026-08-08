// Test: T3Bool three-way short-circuit evaluation (Claim 23)
//
// TAND short-circuits on trit -1 (false):
//   tand(false, <anything>) → false without evaluating <anything>
//
// TOR short-circuits on trit +1 (true):
//   tor(true, <anything>) → true without evaluating <anything>
//
// trit 0 (unknown) always requires second-operand evaluation.

// --- Side-effect counter to verify short-circuit behaviour ---

let eval_count: int = 0;

fn side_effect(val: T3Bool) -> T3Bool {
    eval_count = eval_count + 1;
    val
}

fn reset() {
    eval_count = 0;
}

fn main() {
    // --- TAND short-circuit tests ---

    // tand(false, ...) should NOT evaluate RHS
    reset();
    let r1: T3Bool = false tand side_effect(true);
    assert(r1 == false, "tand(F,T) == F");
    assert(eval_count == 0, "tand(F,...) short-circuited — RHS not evaluated");
    print("PASS tand short-circuit on false");

    // tand(unknown, ...) MUST evaluate RHS
    reset();
    let r2: T3Bool = unknown tand side_effect(true);
    assert(r2 == unknown, "tand(U,T) == U");  // min(0, +1) = 0
    assert(eval_count == 1, "tand(U,...) evaluated RHS");
    print("PASS tand evaluates RHS on unknown");

    // tand(true, ...) MUST evaluate RHS
    reset();
    let r3: T3Bool = true tand side_effect(unknown);
    assert(r3 == unknown, "tand(T,U) == U");  // min(+1, 0) = 0
    assert(eval_count == 1, "tand(T,...) evaluated RHS");
    print("PASS tand evaluates RHS on true");

    // --- TOR short-circuit tests ---

    // tor(true, ...) should NOT evaluate RHS
    reset();
    let r4: T3Bool = true tor side_effect(false);
    assert(r4 == true, "tor(T,F) == T");
    assert(eval_count == 0, "tor(T,...) short-circuited — RHS not evaluated");
    print("PASS tor short-circuit on true");

    // tor(unknown, ...) MUST evaluate RHS
    reset();
    let r5: T3Bool = unknown tor side_effect(false);
    assert(r5 == unknown, "tor(U,F) == U");  // max(0, -1) = 0
    assert(eval_count == 1, "tor(U,...) evaluated RHS");
    print("PASS tor evaluates RHS on unknown");

    // tor(false, ...) MUST evaluate RHS
    reset();
    let r6: T3Bool = false tor side_effect(unknown);
    assert(r6 == unknown, "tor(F,U) == U");  // max(-1, 0) = 0
    assert(eval_count == 1, "tor(F,...) evaluated RHS");
    print("PASS tor evaluates RHS on false");

    // --- Truth table verification (all 9 combinations) ---

    assert((true  tand true)    == true,    "T tand T = T");
    assert((true  tand unknown) == unknown, "T tand U = U");
    assert((true  tand false)   == false,   "T tand F = F");
    assert((unknown tand true)  == unknown, "U tand T = U");
    assert((unknown tand unknown) == unknown, "U tand U = U");
    assert((unknown tand false) == false,   "U tand F = F");
    assert((false tand true)    == false,   "F tand T = F");
    assert((false tand unknown) == false,   "F tand U = F");
    assert((false tand false)   == false,   "F tand F = F");
    print("PASS tand truth table (9/9)");

    assert((true  tor true)    == true,    "T tor T = T");
    assert((true  tor unknown) == true,    "T tor U = T");
    assert((true  tor false)   == true,    "T tor F = T");
    assert((unknown tor true)  == true,    "U tor T = T");
    assert((unknown tor unknown) == unknown, "U tor U = U");
    assert((unknown tor false) == unknown, "U tor F = U");
    assert((false tor true)    == true,    "F tor T = T");
    assert((false tor unknown) == unknown, "F tor U = U");
    assert((false tor false)   == false,   "F tor F = F");
    print("PASS tor truth table (9/9)");

    print("All T3Bool short-circuit tests passed.");
}
