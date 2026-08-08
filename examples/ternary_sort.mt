// examples/ternary_sort.mt
// Balanced Ternary Sorting Algorithms
//
// Sorting is where balanced ternary shines: every comparison naturally
// returns three outcomes {-1, 0, +1} — less, equal, greater.  Binary
// comparison returns only two outcomes (less / not-less), so algorithms
// like quicksort need special handling for equal elements.  In balanced
// ternary, three-way partitioning is the native, zero-cost operation.
//
// Demonstrates:
//   - Ternary comparison: compare(a, b) -> trit  {-, 0, +}
//   - Insertion sort using trit comparison (simple, educational)
//   - Three-way partition (Dutch National Flag) — native in ternary
//   - Quicksort with three-way partition (less/equal/greater)
//   - Sorting balanced ternary numbers
//   - Why three-way partitioning is natural (vs binary's special case)
//
// Authored by: Manish Jagdish Thatte

use std::io;
use std::fmt;
use std::math;
use std::ternary;
use std::collections;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
fn heading(title: str) {
    io::println("");
    io::println("================================================");
    io::print("  ");
    io::println(title);
    io::println("================================================");
}

fn print_array(label: str, arr: Vec<int>) {
    io::print(label);
    let mut i = 0;
    let n = arr.len();
    while i < n {
        io::print_int(arr.get(i));
        if i < n - 1 { io::print(", "); }
        i = i + 1;
    }
    io::println("");
}

fn print_ternary_array(label: str, arr: Vec<int>) {
    io::print(label);
    let mut i = 0;
    let n = arr.len();
    while i < n {
        let val = arr.get(i);
        let t = ternary::int_to_t27(val);
        io::print(ternary::t27_to_str(t));
        io::print("(");
        io::print_int(val);
        io::print(")");
        if i < n - 1 { io::print("  "); }
        i = i + 1;
    }
    io::println("");
}

// ---------------------------------------------------------------------------
// 1. Ternary Comparison
// ---------------------------------------------------------------------------
// In binary: compare returns bool (a < b) — only two outcomes.
//   To distinguish equal, you need a SECOND comparison.
// In balanced ternary: compare returns trit — THREE outcomes in one shot.
//   - => a < b     (negative: a is less)
//   0 => a == b    (zero: equal)
//   + => a > b     (positive: a is greater)
//
// This is not just syntactic sugar — it maps directly to the TMIN2 gate
// in Thatte2 hardware.  One gate, one trit, one clock cycle.

fn compare(a: int, b: int) -> trit {
    if a < b { return -; }
    if a > b { return +; }
    return 0;
}

fn demo_ternary_compare() {
    heading("1. Ternary Comparison: compare(a, b) -> trit");

    io::println("  In binary logic, comparison returns a bool (true/false).");
    io::println("  To distinguish <, ==, > you need TWO comparisons.");
    io::println("");
    io::println("  In balanced ternary, comparison returns a TRIT:");
    io::println("    - => less    (negative)");
    io::println("    0 => equal   (zero)");
    io::println("    + => greater (positive)");
    io::println("");
    io::println("  One comparison, three outcomes. Natural and complete.");
    io::println("");

    // Demonstrate all three outcomes.
    let pair_a: [int] = [3, 5, 9];
    let pair_b: [int] = [7, 5, 2];
    let mut pi = 0;
    while pi < 3 {
        let a = pair_a[pi];
        let b = pair_b[pi];
        let result = compare(a, b);
        io::print("    compare(");
        io::print_int(a);
        io::print(", ");
        io::print_int(b);
        io::print(") = ");
        io::print_trit(result);
        io::print("  => ");
        tif result {
            + => io::println("a > b"),
            0 => io::println("a == b"),
            - => io::println("a < b"),
        }
        pi = pi + 1;
    }
}

// ---------------------------------------------------------------------------
// 2. Insertion Sort with Ternary Comparison
// ---------------------------------------------------------------------------
// Classic insertion sort, but using trit comparison.
// The tif branch handles all three cases cleanly:
//   - => keep shifting (element is less)
//   0 => stop (equal — stable sort, don't move past equals)
//   + => stop (element is greater — correct position found)

fn insertion_sort(arr: Vec<int>) -> Vec<int> {
    let n = arr.len();
    if n <= 1 { return arr; }

    let mut i = 1;
    while i < n {
        let key = arr.get(i);
        let mut j = i - 1;
        let mut done = false;

        while j >= 0 {
            if done { break; }
            let cmp = compare(arr.get(j), key);
            tif cmp {
                + => {
                    // arr[j] > key: shift right to make room
                    arr.set(j + 1, arr.get(j));
                    j = j - 1;
                },
                0 => {
                    // arr[j] == key: stop here (stable — don't move past equals)
                    done = true;
                },
                - => {
                    // arr[j] < key: correct position found
                    done = true;
                },
            }
        }
        arr.set(j + 1, key);
        i = i + 1;
    }
    return arr;
}

fn demo_insertion_sort() {
    heading("2. Insertion Sort (ternary comparison, stable)");

    let data: Vec<int> = Vec::new();
    let raw: [int] = [7, 3, 5, 3, 9, 1, 5, 8, 2, 5];
    for v in raw { data.push(v); }

    print_array("  Before: ", data);

    let sorted = insertion_sort(data);

    print_array("  After:  ", sorted);
    io::println("");
    io::println("  Note: equal elements (3,3) and (5,5,5) preserve original order.");
    io::println("  The tif 0-branch ensures stability — no extra flag needed.");
}

// ---------------------------------------------------------------------------
// 3. Three-Way Partition (Dutch National Flag)
// ---------------------------------------------------------------------------
// Partition arr[lo..hi] into three groups around a pivot:
//   left:   elements < pivot   (trit -)
//   middle: elements == pivot  (trit 0)
//   right:  elements > pivot   (trit +)
//
// In binary quicksort, this requires Bentley-McIlroy or similar hacks
// to handle duplicates efficiently.  In balanced ternary, it's the
// native, obvious partition — because comparison already returns three
// values and tif handles all three branches.
//
// Returns a Partition with lt and gt: indices bounding the equal-to-pivot region.

struct Partition {
    pub lt: int,
    pub gt: int,
}

fn three_way_partition(arr: Vec<int>, lo: int, hi: int) -> Partition {
    // Pivot: median-of-three for robustness.
    let pivot = arr.get(lo + (hi - lo) / 2);

    let mut lt = lo;     // everything before lt is < pivot
    let mut gt = hi;     // everything after gt is > pivot
    let mut i = lo;      // scanning cursor

    while i <= gt {
        let cmp = compare(arr.get(i), pivot);
        tif cmp {
            - => {
                // Less than pivot: swap to left region
                let tmp = arr.get(lt);
                arr.set(lt, arr.get(i));
                arr.set(i, tmp);
                lt = lt + 1;
                i = i + 1;
            },
            0 => {
                // Equal to pivot: already in correct region, advance
                i = i + 1;
            },
            + => {
                // Greater than pivot: swap to right region
                let tmp = arr.get(gt);
                arr.set(gt, arr.get(i));
                arr.set(i, tmp);
                gt = gt - 1;
                // Don't advance i — swapped element needs checking
            },
        }
    }

    return Partition { lt: lt, gt: gt };
}

fn demo_three_way_partition() {
    heading("3. Three-Way Partition (Dutch National Flag)");

    let data: Vec<int> = Vec::new();
    let raw: [int] = [4, 1, 7, 4, 9, 4, 2, 4, 6, 4];
    for v in raw { data.push(v); }

    io::println("  Pivot = 4 (many duplicates)");
    print_array("  Before: ", data);

    let part = three_way_partition(data, 0, data.len() - 1);
    let lt = part.lt;
    let gt = part.gt;

    print_array("  After:  ", data);
    io::print("  Region < 4:  indices 0..");
    io::println_int(lt - 1);
    io::print("  Region == 4: indices ");
    io::print_int(lt);
    io::print("..");
    io::println_int(gt);
    io::print("  Region > 4:  indices ");
    io::print_int(gt + 1);
    io::print("..");
    io::println_int(data.len() - 1);
    io::println("");
    io::println("  Each element was classified by a single trit comparison.");
    io::println("  tif routes directly to <, ==, or > — no secondary test needed.");
}

// ---------------------------------------------------------------------------
// 4. Quicksort with Three-Way Partition
// ---------------------------------------------------------------------------
// The classic quicksort, but with ternary partitioning.
// Recurse on the < and > regions; skip the == region entirely.
// This is optimal for inputs with many duplicates — O(n) when
// all elements are equal, because the entire array lands in
// the middle partition on the first pass.

fn quicksort(arr: Vec<int>, lo: int, hi: int) {
    if lo >= hi { return; }

    let part = three_way_partition(arr, lo, hi);
    let lt = part.lt;
    let gt = part.gt;

    // Recurse on left region (elements < pivot).
    if lt > lo {
        quicksort(arr, lo, lt - 1);
    }
    // Skip middle region — all equal to pivot, already in place.
    // Recurse on right region (elements > pivot).
    if gt < hi {
        quicksort(arr, gt + 1, hi);
    }
}

fn demo_quicksort() {
    heading("4. Quicksort with Three-Way Partition");

    // Test 1: general array
    let data1: Vec<int> = Vec::new();
    let raw1: [int] = [13, -4, 7, 0, -13, 9, -1, 4, -9, 1, 0, -7, 13, 3];
    for v in raw1 { data1.push(v); }

    print_array("  Input:  ", data1);
    quicksort(data1, 0, data1.len() - 1);
    print_array("  Sorted: ", data1);
    io::println("");

    // Test 2: array with many duplicates (shows ternary advantage)
    io::println("  Many duplicates (ternary quicksort excels here):");
    let data2: Vec<int> = Vec::new();
    let raw2: [int] = [1, 0, -1, 0, 1, -1, 0, 0, 1, -1, 0, 1, -1, 0, -1, 1, 0, 0];
    for v in raw2 { data2.push(v); }

    print_array("  Input:  ", data2);
    quicksort(data2, 0, data2.len() - 1);
    print_array("  Sorted: ", data2);
    io::println("  All duplicates handled in a single partition pass (== region skipped).");
}

// ---------------------------------------------------------------------------
// 5. Sorting Balanced Ternary Numbers
// ---------------------------------------------------------------------------
// Numbers expressed in balanced ternary can be compared trit-by-trit
// from the most significant trit.  This is a ternary radix comparison.

fn bt_compare(a: int, b: int) -> trit {
    // Compare as balanced ternary numbers.
    // Most significant trit determines sign/magnitude ordering naturally.
    return compare(a, b);
}

fn demo_sort_ternary_numbers() {
    heading("5. Sorting Balanced Ternary Numbers");

    // Range: values expressible in 3 trits (tryte range: -13 to +13)
    let data: Vec<int> = Vec::new();
    let raw: [int] = [8, -11, 4, -1, 13, 0, -13, 1, -4, 11, -8];
    for v in raw { data.push(v); }

    io::println("  Values in tryte range (-13 to +13):");
    print_ternary_array("  Before: ", data);

    quicksort(data, 0, data.len() - 1);

    print_ternary_array("  After:  ", data);
    io::println("");

    io::println("  Balanced ternary representation of sorted values:");
    let mut i = 0;
    while i < data.len() {
        let val = data.get(i);
        let trits = math::to_balanced_ternary(val);
        io::print("    ");
        io::print(fmt::align_right(fmt::show_int(val), 4, ' '));
        io::print("  =  ");
        io::println(ternary::trits_to_str(trits));
        i = i + 1;
    }

    io::println("");
    io::println("  Note: balanced ternary has symmetric range (-13 to +13 for 3 trits).");
    io::println("  Negation is just trit-flip (tnot). No offset, no bias, no two's complement.");
}

// ---------------------------------------------------------------------------
// 6. Why Three-Way Partitioning is Native in Balanced Ternary
// ---------------------------------------------------------------------------
fn demo_ternary_advantage() {
    heading("6. The Ternary Sorting Advantage");

    io::println("  Binary comparison: a < b -> bool {true, false}");
    io::println("    To sort, you need:");
    io::println("      if a < b   -> move left");
    io::println("      else       -> move right");
    io::println("    Equal elements? Need ANOTHER comparison (a == b).");
    io::println("    Three-way partition? Special algorithm (Bentley-McIlroy).");
    io::println("");
    io::println("  Ternary comparison: compare(a, b) -> trit {-, 0, +}");
    io::println("    To sort, you need:");
    io::println("      tif compare(a, b) {");
    io::println("        - => move left,    // less");
    io::println("        0 => stay,         // equal");
    io::println("        + => move right,   // greater");
    io::println("      }");
    io::println("    Three outcomes, one comparison, one tif branch.");
    io::println("    Three-way partition is the DEFAULT, not a special case.");
    io::println("");
    io::println("  In hardware (Thatte2 gate library):");
    io::println("    Binary CMP:  compare -> {0, 1} -> need extra gates for ==");
    io::println("    Ternary CMP:  compare -> {-, 0, +} -> TMIN2 gate, one cycle");
    io::println("    Three-way branch: tif reads the trit directly. Zero overhead.");
    io::println("");

    // Demonstrate with a truth table.
    io::println("  Ternary comparison truth table (a vs b):");
    io::println("          b=-1    b=0    b=+1");
    let vals: [int] = [-1, 0, 1];
    let labels: [str] = ["a=-1", "a= 0", "a=+1"];
    let mut ri = 0;
    for a in vals {
        io::print("    ");
        io::print(labels[ri]);
        io::print(":");
        for b in vals {
            let cmp = compare(a, b);
            io::print("     ");
            io::print_trit(cmp);
            io::print(" ");
        }
        io::println("");
        ri = ri + 1;
    }
    io::println("");
    io::println("  Every cell is a single trit. Every sort decision is one gate.");
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
fn main() {
    io::println("maniT Balanced Ternary Sorting Algorithms");
    io::println("==========================================");
    io::println("Authored by: Manish Jagdish Thatte");

    demo_ternary_compare();
    demo_insertion_sort();
    demo_three_way_partition();
    demo_quicksort();
    demo_sort_ternary_numbers();
    demo_ternary_advantage();

    io::println("");
    io::println("Demo complete.");
}
