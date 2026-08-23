// Test: Binary-ternary bridge module (Claim 17)
//
// Two-bit-per-trit encoding:
//   +1 → (1, 0)
//    0 → (0, 0)
//   -1 → (0, 1)
//   invalid: (1, 1) → 0 with error

use std::bridge;
use std::test;

fn main() {
    // --- Single trit encoding/decoding ---

    let (b1, b0) = bridge::trit_to_bits(+1);
    test::assert(b1 == 1 tand b0 == 0, "+1 encodes to (1,0)");

    let (b1, b0) = bridge::trit_to_bits(0);
    test::assert(b1 == 0 tand b0 == 0, "0 encodes to (0,0)");

    let (b1, b0) = bridge::trit_to_bits(-1);
    test::assert(b1 == 0 tand b0 == 1, "-1 encodes to (0,1)");

    test::assert(bridge::bits_to_trit(1, 0) == +1, "(1,0) decodes to +1");
    test::assert(bridge::bits_to_trit(0, 0) == 0,  "(0,0) decodes to 0");
    test::assert(bridge::bits_to_trit(0, 1) == -1, "(0,1) decodes to -1");
    test::assert(bridge::bits_to_trit(1, 1) == 0,  "(1,1) invalid → 0");
    print("PASS single trit encoding/decoding");

    // --- Validity check ---
    test::assert(bridge::is_valid_encoding(1, 0) == true,  "(1,0) valid");
    test::assert(bridge::is_valid_encoding(0, 0) == true,  "(0,0) valid");
    test::assert(bridge::is_valid_encoding(0, 1) == true,  "(0,1) valid");
    test::assert(bridge::is_valid_encoding(1, 1) == false, "(1,1) invalid");
    print("PASS validity check");

    // --- Round-trip ---
    let trits: [trit; 5] = [+1, -1, 0, +1, -1];
    let mut i: int = 0;
    while i < 5 {
        let t = trits[i];
        let (b1, b0) = bridge::trit_to_bits(t);
        let t2 = bridge::bits_to_trit(b1, b0);
        test::assert(t == t2, "round-trip trit encoding");
        i = i + 1;
    }
    print("PASS round-trip encoding");

    print("All bridge tests passed.");
}
