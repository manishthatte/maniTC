// bridge_demo.mt — Binary-Ternary Bridge Demonstration
//
// Shows how balanced ternary data is encoded for transmission over
// binary infrastructure (PCIe, USB, Ethernet) and decoded back.
//
// Two-bit-per-trit encoding:
//   Trit +1 → bits (1, 0)
//   Trit  0 → bits (0, 0)
//   Trit -1 → bits (0, 1)
//   Invalid  (1, 1) → mapped to trit 0 with error flag
//
// Authored by: Manish Jagdish Thatte

use std::bridge;

fn main() {
    print("=== Binary-Ternary Bridge Demo ===");
    print("");

    // Encode a balanced ternary number trit-by-trit
    let value: [trit; 5] = [+1, -1, 0, +1, -1];  // +1×81 -1×27 +0×9 +1×3 -1×1 = 81-27+3-1 = 56
    print("Ternary value: +1 -1  0 +1 -1  (decimal 56)");
    print("");

    // Encode to binary
    print("Encoding to binary (2 bits per trit):");
    let mut i: int = 0;
    while i < 5 {
        let (b1, b0) = bridge::trit_to_bits(value[i]);
        let valid = bridge::is_valid_encoding(b1, b0);
        print("  trit[", i, "] = ", value[i], " → bits (", b1, ",", b0, ")  valid=", valid);
        i = i + 1;
    }
    print("");
    print("Total: 5 trits encoded as 10 binary bits");
    print("Binary: 10 01 00 10 01");
    print("");

    // Decode back
    print("Decoding back to ternary:");
    let decoded: [trit; 5] = [0; 5];
    decoded[0] = bridge::bits_to_trit(1, 0);
    decoded[1] = bridge::bits_to_trit(0, 1);
    decoded[2] = bridge::bits_to_trit(0, 0);
    decoded[3] = bridge::bits_to_trit(1, 0);
    decoded[4] = bridge::bits_to_trit(0, 1);

    let mut match_count: int = 0;
    i = 0;
    while i < 5 {
        if value[i] == decoded[i] {
            match_count = match_count + 1;
        }
        i = i + 1;
    }
    print("  Round-trip: ", match_count, "/5 trits match");
    print("");

    // Invalid encoding handling
    print("Invalid encoding (1,1) → ", bridge::bits_to_trit(1, 1), " (mapped to 0)");
    print("");
    print("=== Bridge demo complete ===");
}
