// Test: T3ISA register-allocation regressions.
//
// Both bugs below produced silently wrong arithmetic — no trap, no diagnostic,
// just a different answer — and both needed enough register pressure to show up,
// so they survived every "compiles and exits 0" check the suite had.
//
//   1. Call operands were materialised before the caller-save stores, which
//      forced them into R21/R22/R24/R25 — the registers the call sequence itself
//      uses for the fn_ptr, the move scratch and the return stash.  From the
//      third spilled operand onward the fn_ptr move overwrote an argument, so
//      `op(a, b)` reached the callee as `op(a, a)`.
//
//   2. `dst_reg` returned R23 for an already-spilled temp without storing it
//      back to its slot.  Where a join block's phi was spilled by one
//      predecessor, another predecessor left its copy in R23 and the slot kept an
//      unrelated temp, so the join read the wrong value.
//
// Each test is written to need spilling: enough live values across a call, or
// enough nested loops, that the R1-R20 pool runs out.
use std::io;

fn pass(label: str) { io::print("PASS "); io::println(label); }
fn check(label: str, cond: bool) { if cond { pass(label) } else { io::print("FAIL "); io::println(label); } }

fn check_trit(label: str, got: trit, want: trit) {
    if got == want { pass(label) }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got="); io::print_trit(got);
        io::print(" want="); io::print_trit(want);
        io::newline();
    }
}

// ---------------------------------------------------------------------------
// 1. Indirect call whose operands spill.
//
// The padding prints keep enough temps live across the call that both arguments
// end up in spill slots.  Before the fix the callee saw its first argument
// twice, so `tand` returned `a` and `tor` returned `+` for every pair.
// ---------------------------------------------------------------------------
fn apply(op: fn(trit, trit) -> trit, a: trit, b: trit) -> trit {
    return op(a, b);
}

fn test_indirect_call_operands() {
    let vals: [trit] = [+, 0, -];
    let names: [str] = [" + ", " 0 ", " - "];
    let mut ri = 0;
    let mut min_ok = true;
    let mut max_ok = true;

    for a in vals {
        // These prints are load-bearing: they create the register pressure that
        // pushes the call operands into spill slots.
        io::print("    ");
        io::print(names[ri]);
        io::print("  |");
        let mut ci = 0;
        for b in vals {
            let got_min = apply(fn(x: trit, y: trit) -> trit => x tand y, a, b);
            let got_max = apply(fn(x: trit, y: trit) -> trit => x tor y, a, b);
            if got_min != (a tand b) { min_ok = false; }
            if got_max != (a tor  b) { max_ok = false; }
            io::print("    ");
            io::print_trit(got_min);
            io::print("  ");
            ci = ci + 1;
        }
        io::newline();
        ri = ri + 1;
    }
    check("indirect call passes the second argument", min_ok);
    check("indirect call passes both arguments", max_ok);
}

// ---------------------------------------------------------------------------
// 2. Direct call with more spilled operands than the old scratch ladder held.
// ---------------------------------------------------------------------------
fn pick(a: trit, b: trit, c: trit, d: trit) -> trit {
    return (a tand b) tor (c tand d);
}

fn test_direct_call_operands() {
    let vals: [trit] = [+, 0, -];
    let mut ok = true;
    for a in vals {
        for b in vals {
            for c in vals {
                for d in vals {
                    let want = (a tand b) tor (c tand d);
                    if pick(a, b, c, d) != want { ok = false; }
                }
            }
        }
    }
    check("direct call keeps four spilled operands distinct", ok);
}

// ---------------------------------------------------------------------------
// 3. Phi destinations spilled on one predecessor of a join.
//
// `tand`/`tor` short-circuit, so each one builds a join block with a phi.  Three
// nested loops over these give enough pressure that a phi gets spilled on one
// arm and not the other — the case that silently read a stale slot.  The first
// loop nest matters: it is what exhausts the pool before the second begins.
// ---------------------------------------------------------------------------
fn test_spilled_phi_join() {
    let vals: [trit] = [+, 0, -];

    // Keep both nests exactly as they are, and keep them back to back.  Binding
    // lhs/rhs before comparing, and leaving the first nest in front of the
    // second, is what drives a phi into a spill slot on one arm of a
    // short-circuit join and not the other.  Anything that lowers the pressure
    // hides the bug — in particular a reporting call placed *between* the nests,
    // whose caller-save spills and restores the whole pool and so resets the
    // allocator.  That is why both results are reported only at the end.
    let mut comm_ok = true;
    for a in vals {
        for b in vals {
            if (a tand b) != (b tand a) { comm_ok = false; }
        }
    }

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

    check("tand commutes", comm_ok);
    check("tand distributes over tor", dist_ok);
}

// ---------------------------------------------------------------------------
// 4. Spot-check the identities the bugs actually broke, spelled out.
// ---------------------------------------------------------------------------
fn test_known_wrong_values() {
    let zero: trit = 0;
    let pos:  trit = +;
    let neg:  trit = -;

    // Reported as + before fix 2 (a=0, c=+ read a stale phi slot).
    check_trit("min(0, +)", zero tand pos, 0);
    check_trit("max(0, -)", zero tor  neg, 0);
    check_trit("min(+, 0)", pos  tand zero, 0);
    check_trit("max(-, 0)", neg  tor  zero, 0);

    // Reported as the first operand before fix 1.
    check_trit("apply min(+, -)", apply(fn(x: trit, y: trit) -> trit => x tand y, +, -), -);
    check_trit("apply max(-, +)", apply(fn(x: trit, y: trit) -> trit => x tor  y, -, +), +);
    check_trit("apply second arg", apply(fn(x: trit, y: trit) -> trit => y, +, -), -);
}

fn main() {
    io::println("-- indirect call operands --");
    test_indirect_call_operands();

    io::println("-- direct call operands --");
    test_direct_call_operands();

    io::println("-- spilled phi at join --");
    test_spilled_phi_join();

    io::println("-- known wrong values --");
    test_known_wrong_values();

    io::println("all regalloc regression tests done");
}
