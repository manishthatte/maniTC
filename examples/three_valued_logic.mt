// examples/three_valued_logic.mt
// Comprehensive three-valued logic demo for maniT.
//
// Three-valued (ternary) logic generalises classical Boolean logic by
// introducing an "Unknown" / "Indeterminate" third truth value alongside
// True and False.  This is Kleene's strong three-valued logic, where:
//
//   True    <->  trit +  (+1)
//   Unknown <->  trit  0  (0)
//   False   <->  trit -  (-1)
//
// Demonstrates:
//   - bool3 literals (True, Unknown, False) and tif branching
//   - tand / tor / tnot / txor truth tables
//   - Łukasiewicz logic identities
//   - Practical use case: multi-sensor fusion with unknown readings
//   - Ternary voting / fault-tolerance (TMR — Triple Modular Redundancy)
//   - Epistemic interpretation: what we know / don't know / know is false

use std::io;
use std::fmt;
use std::ternary;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
fn heading(s: str) {
    io::println("");
    io::println("================================================");
    io::print("  ");
    io::println(s);
    io::println("================================================");
}

// Return the string name of a bool3 value.
fn bool3_name(b: bool3) -> str {
    tif (b as trit) {
        + => return "True",
        0 => return "Unknown",
        - => return "False",
    }
}

// Print a 3x3 truth table for a binary bool3 operator.
fn print_truth_table(op_name: str, op: fn(trit, trit) -> trit) {
    io::print("  ");
    io::println(op_name);
    io::println("       |   +      0      -  ");
    io::println("  -----+---------------------");

    let vals: [trit] = [+, 0, -];
    let names: [str] = [" + ", " 0 ", " - "];
    let mut ri = 0;
    for a in vals {
        io::print("    ");
        io::print(names[ri]);
        io::print("  |");
        for b in vals {
            let result = op(a, b);
            io::print("    ");
            io::print_trit(result);
            io::print("  ");
        }
        io::println("");
        ri = ri + 1;
    }
    io::println("");
}

// ---------------------------------------------------------------------------
// 1. bool3 Literals and tif
// ---------------------------------------------------------------------------
fn demo_bool3_literals() {
    heading("1. bool3 Literals and tif Branching");

    let t: bool3 = True;
    let u: bool3 = Unknown;
    let f: bool3 = False;

    io::println("  bool3 values and their trit representations:");
    io::print("    True    -> trit: "); io::println_trit(t as trit);
    io::print("    Unknown -> trit: "); io::println_trit(u as trit);
    io::print("    False   -> trit: "); io::println_trit(f as trit);

    io::println("");
    io::println("  tif branching on bool3:");
    let tests: [bool3] = [True, Unknown, False];
    for b in tests {
        io::print("    tif ");
        io::print(bool3_name(b));
        io::print(" => ");
        tif (b as trit) {
            + => io::println("branch +  (True)"),
            0 => io::println("branch 0  (Unknown)"),
            - => io::println("branch -  (False)"),
        }
    }

    // bool3 in conditional logic.
    io::println("");
    io::println("  Classical if/elif/else on bool3 (treating Unknown as not-True):");
    for b in tests {
        io::print("    ");
        io::print(bool3_name(b));
        io::print(" -> ");
        if (b as trit) == + {
            io::println("definitely true");
        } elif (b as trit) == 0 {
            io::println("indeterminate — cannot decide");
        } else {
            io::println("definitely false");
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Truth Tables
// ---------------------------------------------------------------------------
fn demo_truth_tables() {
    heading("2. Ternary Operator Truth Tables");

    io::println("  tnot (negation: flip sign):");
    io::println("    tnot + = -   (True  -> False)");
    io::println("    tnot 0 = 0   (Unknown -> Unknown)");
    io::println("    tnot - = +   (False -> True)");
    io::println("");

    // tand (Łukasiewicz conjunction = min).
    print_truth_table("tand  (conjunction, min(a,b)):",
        fn(a: trit, b: trit) -> trit => a tand b);

    // tor (Łukasiewicz disjunction = max).
    print_truth_table("tor   (disjunction, max(a,b)):",
        fn(a: trit, b: trit) -> trit => a tor b);

    // txor (mod-3 addition, balanced).
    print_truth_table("txor  (exclusive-or, (a+b) mod3 balanced):",
        fn(a: trit, b: trit) -> trit => a txor b);

    // Łukasiewicz implication: a -> b = max(tnot a, b).
    print_truth_table("Luk implication  (max(tnot a, b)):",
        fn(a: trit, b: trit) -> trit => (tnot a) tor b);

    // Łukasiewicz biconditional: a <-> b = tnot(a txor b).
    print_truth_table("Luk biconditional (tnot(a txor b)):",
        fn(a: trit, b: trit) -> trit => tnot (a txor b));
}

// ---------------------------------------------------------------------------
// 3. Logic Identities
// ---------------------------------------------------------------------------
fn demo_identities() {
    heading("3. Three-Valued Logic Identities");

    // Verify De Morgan's laws over all trit combinations.
    io::println("  De Morgan (tnot(a tand b) == tnot(a) tor tnot(b)):");
    let vals: [trit] = [+, 0, -];
    let mut all_ok = true;
    for a in vals {
        for b in vals {
            let lhs = tnot (a tand b);
            let rhs = (tnot a) tor (tnot b);
            if lhs != rhs { all_ok = false; }
        }
    }
    io::print("    Holds for all (a,b): ");
    io::println_bool(all_ok);

    io::println("");
    io::println("  De Morgan (tnot(a tor b) == tnot(a) tand tnot(b)):");
    let mut all_ok2 = true;
    for a in vals {
        for b in vals {
            let lhs = tnot (a tor b);
            let rhs = (tnot a) tand (tnot b);
            if lhs != rhs { all_ok2 = false; }
        }
    }
    io::print("    Holds for all (a,b): ");
    io::println_bool(all_ok2);

    io::println("");
    io::println("  Double negation (tnot(tnot a) == a):");
    let mut dn_ok = true;
    for a in vals {
        if tnot (tnot a) != a { dn_ok = false; }
    }
    io::print("    Holds: ");
    io::println_bool(dn_ok);

    io::println("");
    io::println("  Law of excluded middle (a tor tnot(a)):");
    io::println("  (Not a tautology in 3-valued logic — can be Unknown!)");
    for a in vals {
        io::print("    a=");
        io::print_trit(a);
        io::print("  =>  a tor tnot(a) = ");
        io::println_trit(a tor (tnot a));
    }

    io::println("");
    io::println("  Idempotence: a tand a == a, a tor a == a");
    let mut idemp_ok = true;
    for a in vals {
        if (a tand a) != a { idemp_ok = false; }
        if (a tor  a) != a { idemp_ok = false; }
    }
    io::print("    Holds: ");
    io::println_bool(idemp_ok);

    io::println("");
    io::println("  Commutativity: a tand b == b tand a, a tor b == b tor a");
    let mut comm_ok = true;
    for a in vals {
        for b in vals {
            if (a tand b) != (b tand a) { comm_ok = false; }
            if (a tor  b) != (b tor  a) { comm_ok = false; }
        }
    }
    io::print("    Holds: ");
    io::println_bool(comm_ok);

    io::println("");
    io::println("  Distributivity: a tand (b tor c) == (a tand b) tor (a tand c)");
    let mut dist_ok = true;
    for a in vals {
        for b in vals {
            for c in vals {
                let lhs = a tand (b tor c);
                let rhs = (a tand b) tor (a tand c);
                if lhs != rhs { dist_ok = false; }
            }
        }
    }
    io::print("    Holds: ");
    io::println_bool(dist_ok);
}

// ---------------------------------------------------------------------------
// 4. Practical example: multi-sensor fusion
// ---------------------------------------------------------------------------
// A system has three independent sensors (A, B, C).
// Each returns one of three states:
//   + = normal / OK
//   0 = uncertain / no reading
//   - = fault / alarm
//
// We aggregate them using ternary voting and ternary AND/OR logic.

struct SensorReading {
    label: str,
    value: trit,
}

fn sensor_status_str(v: trit) -> str {
    tif v {
        + => return "OK      ",
        0 => return "UNKNOWN ",
        - => return "FAULT   ",
    }
}

fn demo_sensor_fusion() {
    heading("4. Practical: Multi-Sensor Fusion with bool3");

    // Scenario 1: all sensors agree — clear outcome.
    io::println("  Scenario 1: All sensors report OK (+)");
    let s1a: trit = +;
    let s1b: trit = +;
    let s1c: trit = +;
    let vote1 = ternary::trit_median(s1a, s1b, s1c);
    io::print("    A=+ B=+ C=+  =>  vote="); io::println(sensor_status_str(vote1));

    // Scenario 2: majority fault — should alarm.
    io::println("");
    io::println("  Scenario 2: Two of three sensors report FAULT (-)");
    let s2a: trit = +;
    let s2b: trit = -;
    let s2c: trit = -;
    let vote2 = ternary::trit_median(s2a, s2b, s2c);
    io::print("    A=+ B=- C=-  =>  vote="); io::println(sensor_status_str(vote2));

    // Scenario 3: split vote with unknown — indeterminate.
    io::println("");
    io::println("  Scenario 3: One OK, one Unknown, one Fault");
    let s3a: trit = +;
    let s3b: trit = 0;
    let s3c: trit = -;
    let vote3 = ternary::trit_median(s3a, s3b, s3c);
    io::print("    A=+ B=0 C=-  =>  vote="); io::println(sensor_status_str(vote3));

    // Scenario 4: all unknown — system cannot decide.
    io::println("");
    io::println("  Scenario 4: All sensors return Unknown");
    let vote4 = ternary::trit_median(0, 0, 0);
    io::print("    A=0 B=0 C=0  =>  vote="); io::println(sensor_status_str(vote4));

    // Combining subsystems with tand / tor.
    io::println("");
    io::println("  System safety: engine AND brakes must both be OK.");
    io::println("  (using tand for conservative AND — min of two sensor readings)");
    let engine = +;         // OK
    let brakes = 0;         // Unknown (sensor offline)
    let safe = engine tand brakes;
    io::print("    engine=OK, brakes=Unknown  =>  safe=");
    io::println(sensor_status_str(safe));
    // Result is 0 (Unknown) — system cannot certify safety.

    io::println("");
    io::println("  Emergency stop: trigger if engine OR brakes is FAULT.");
    io::println("  (using tor for conservative OR — max, but we check for fault=-)");
    let eng2 = +;
    let brk2 = -;   // brakes fault!
    // fault is represented by -; "any fault" means min value -> tand.
    let emergency = eng2 tand brk2;   // min(+1,-1) = -1 => FAULT wins
    io::print("    engine=OK, brakes=FAULT  =>  combined=");
    io::println(sensor_status_str(emergency));
}

// ---------------------------------------------------------------------------
// 5. Epistemic interpretation
// ---------------------------------------------------------------------------
// True    = "we know this is true"
// Unknown = "we don't know"
// False   = "we know this is false"
//
// This is useful for knowledge bases, partial evaluations, and type checkers.
fn demo_epistemic() {
    heading("5. Epistemic Interpretation (Knowledge States)");

    io::println("  Assigning knowledge states to propositions:");

    struct Fact {
        proposition: str,
        known: bool3,
    }

    let facts: [Fact] = [
        Fact { proposition: "The sky is blue",          known: True    },
        Fact { proposition: "It will rain tomorrow",    known: Unknown },
        Fact { proposition: "2 + 2 = 5",                known: False   },
        Fact { proposition: "P = NP",                   known: Unknown },
        Fact { proposition: "maniT uses ternary logic", known: True    },
    ];

    for fact in facts {
        io::print("    \"");
        io::print(fact.proposition);
        io::print("\"");
        io::print("  ->  ");
        io::println_bool3(fact.known);
    }

    io::println("");
    io::println("  Inference: combining knowledge claims with tand / tor");
    io::println("  Claim A (True) tand Claim B (Unknown):");
    let ca: bool3 = True;
    let cb: bool3 = Unknown;
    let combined_a = (ca as trit) tand (cb as trit);
    io::print("    True tand Unknown = ");
    io::println(bool3_name(combined_a as bool3));
    io::println("    (cannot assert the conjunction when one part is uncertain)");

    io::println("");
    io::println("  Claim C (True) tor Claim D (Unknown):");
    let cc: bool3 = True;
    let cd: bool3 = Unknown;
    let combined_b = (cc as trit) tor (cd as trit);
    io::print("    True tor Unknown = ");
    io::println(bool3_name(combined_b as bool3));
    io::println("    (can assert disjunction because one part is certainly true)");

    io::println("");
    io::println("  Negation of Unknown is still Unknown:");
    let unk: bool3 = Unknown;
    io::print("    tnot Unknown = ");
    io::println(bool3_name((tnot (unk as trit)) as bool3));
}

// ---------------------------------------------------------------------------
// 6. Triple Modular Redundancy (TMR)
// ---------------------------------------------------------------------------
// Classical fault-tolerance technique: run three independent computations;
// vote on the result.  Here we show how ternary voting directly maps to
// trit_median from std::ternary.
fn compute_result(input: int, fault_injected: bool) -> trit {
    if fault_injected {
        return -;   // simulated fault output
    }
    // Simple classification: positive -> +, zero -> 0, negative -> -
    if input > 0 { return +; }
    elif input == 0 { return 0; }
    else { return -; }
}

fn demo_tmr() {
    heading("6. Triple Modular Redundancy (TMR) with trit_median");

    let input = 5;   // positive number
    io::print("  Input value: ");
    io::println_int(input);
    io::println("");

    // Case 1: no faults.
    let r1a = compute_result(input, false);
    let r1b = compute_result(input, false);
    let r1c = compute_result(input, false);
    let voted1 = ternary::trit_median(r1a, r1b, r1c);
    io::print("  No faults   A="); io::print_trit(r1a);
    io::print(" B="); io::print_trit(r1b);
    io::print(" C="); io::print_trit(r1c);
    io::print("  voted="); io::println_trit(voted1);

    // Case 2: one fault.
    let r2a = compute_result(input, false);
    let r2b = compute_result(input, true);    // B is faulty
    let r2c = compute_result(input, false);
    let voted2 = ternary::trit_median(r2a, r2b, r2c);
    io::print("  One fault   A="); io::print_trit(r2a);
    io::print(" B="); io::print_trit(r2b);
    io::print(" C="); io::print_trit(r2c);
    io::print("  voted="); io::print_trit(voted2);
    io::println("  (majority overrides fault)");

    // Case 3: two faults.
    let r3a = compute_result(input, false);
    let r3b = compute_result(input, true);
    let r3c = compute_result(input, true);
    let voted3 = ternary::trit_median(r3a, r3b, r3c);
    io::print("  Two faults  A="); io::print_trit(r3a);
    io::print(" B="); io::print_trit(r3b);
    io::print(" C="); io::print_trit(r3c);
    io::print("  voted="); io::print_trit(voted3);
    io::println("  (majority corrupted — wrong result)");

    io::println("");
    io::println("  TMR can tolerate exactly 1 fault out of 3 modules.");
    io::println("  trit_median(a, b, c) is the native ternary majority vote.");
}

// ---------------------------------------------------------------------------
// 7. Comparison with classical binary logic
// ---------------------------------------------------------------------------
fn demo_comparison() {
    heading("7. Three-Valued vs Two-Valued Logic");

    io::println("  In classical (Boolean) logic:");
    io::println("    AND truth table (2x2): 4 cells");
    io::println("    OR  truth table (2x2): 4 cells");
    io::println("    The law of excluded middle always holds: a OR NOT(a) = True");
    io::println("");
    io::println("  In Kleene 3-valued logic:");
    io::println("    AND truth table (3x3): 9 cells");
    io::println("    OR  truth table (3x3): 9 cells");
    io::println("    The law of excluded middle does NOT always hold:");
    io::print("      Unknown tor tnot(Unknown) = Unknown tor Unknown = ");
    io::println_trit((0 as trit) tor (tnot (0 as trit)));
    io::println("");
    io::println("  Practical differences:");
    io::println("    Binary: NULL in SQL is implicitly 'unknown' but SQL has");
    io::println("            three-valued semantics layered on top of binary.");
    io::println("    maniT:  Unknown is a first-class value; tif handles it");
    io::println("            directly without special-casing.");
    io::println("");
    io::println("  Classical tautologies that break in 3-valued logic:");
    let a: trit = 0;  // Unknown
    io::print("    a = Unknown:");
    io::print("  a tor tnot(a) = ");   io::print_trit(a tor (tnot a));
    io::print("   a tand tnot(a) = "); io::println_trit(a tand (tnot a));
    io::println("    (Both are Unknown, not True/False as in classical logic)");
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
fn main() {
    io::println("maniT Three-Valued Logic Demo");
    io::println("=============================");

    demo_bool3_literals();
    demo_truth_tables();
    demo_identities();
    demo_sensor_fusion();
    demo_epistemic();
    demo_tmr();
    demo_comparison();

    io::println("");
    io::println("Demo complete.");
}
