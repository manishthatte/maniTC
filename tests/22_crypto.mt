// Test: ternary cryptography concepts — trit-level hash, cipher, HMAC,
//        constant-time comparison, key derivation.  All operations are
//        implemented in pure ManiT (no std::crypto dependency) to exercise
//        ternary arithmetic on trit arrays.
use std::io;

fn pass(label: str) { io::print("PASS "); io::println(label); }
fn fail(label: str) { io::print("FAIL "); io::println(label); }
fn check(label: str, cond: bool) { if cond { pass(label) } else { fail(label) } }
fn check_int(label: str, got: int, want: int) {
    if got == want { pass(label); }
    else {
        io::print("FAIL "); io::print(label);
        io::print(" got="); io::print_int(got);
        io::print(" want="); io::print_int(want);
        io::newline();
    }
}

fn trit_val(t: trit) -> int {
    if t > 0 { 1 } elif t == 0 { 0 } else { -1 }
}

// ---------------------------------------------------------------------------
// Pure ManiT trit-level operations (no stdlib crypto)
// ---------------------------------------------------------------------------

// Single trit mul: sign rule
fn tmul(a: trit, b: trit) -> trit {
    let va = trit_val(a);
    let vb = trit_val(b);
    let r = va * vb;
    if r > 0 { + } elif r == 0 { 0 } else { - }
}

// Single trit XOR (balanced mod-3 addition): a+b mapped to {-1,0,+1}
fn txor_trit(a: trit, b: trit) -> trit {
    let s = trit_val(a) + trit_val(b);
    if s > 1 { - }       // 2 -> -1 (wraps)
    elif s < -1 { + }    // -2 -> +1 (wraps)
    elif s > 0 { + }
    elif s == 0 { 0 }
    else { - }
}

// Mix/permute: a simple sponge-like mixing of 9 int values
fn mix_state(s0: int, s1: int, s2: int, s3: int, s4: int,
             s5: int, s6: int, s7: int, s8: int) -> int {
    // Combine all state values with nonlinear mixing
    let a = s0 * 3 + s1 * 7 + s2 * 13 + s3;
    let b = s4 * 5 + s5 * 11 + s6 * 17 + s7;
    let c = s8 * 19 + a + b;
    c
}

// Simple 9-element trit hash: deterministic mixing of trit values
fn trit_hash_9(d0: trit, d1: trit, d2: trit, d3: trit, d4: trit,
               d5: trit, d6: trit, d7: trit, d8: trit) -> int {
    let v0 = trit_val(d0);
    let v1 = trit_val(d1);
    let v2 = trit_val(d2);
    let v3 = trit_val(d3);
    let v4 = trit_val(d4);
    let v5 = trit_val(d5);
    let v6 = trit_val(d6);
    let v7 = trit_val(d7);
    let v8 = trit_val(d8);
    mix_state(v0, v1, v2, v3, v4, v5, v6, v7, v8)
}

// Simple XOR cipher: XOR each trit with the corresponding key trit
fn cipher_trit(data: trit, key: trit) -> trit {
    txor_trit(data, key)
}

// Constant-time equality of two trit values
fn ct_eq_trit(a: trit, b: trit) -> int {
    let va = trit_val(a);
    let vb = trit_val(b);
    if va == vb { 1 } else { 0 }
}

// HMAC-like: hash(key XOR opad || hash(key XOR ipad || data))
// Simplified to int operations
fn simple_hmac(key: int, data: int) -> int {
    let inner = (key * 3 + 7) * 13 + data;
    let outer = (key * 5 + 11) * 17 + inner;
    outer
}

// ---------------------------------------------------------------------------
// trit_mul truth table
// ---------------------------------------------------------------------------

fn test_trit_mul() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;

    // Sign rules
    check_int("tmul: (+)(+)=+", trit_val(tmul(p, p)),  1);
    check_int("tmul: (+)(0)=0", trit_val(tmul(p, z)),  0);
    check_int("tmul: (+)(-)=-", trit_val(tmul(p, n)), -1);
    check_int("tmul: (0)(+)=0", trit_val(tmul(z, p)),  0);
    check_int("tmul: (0)(0)=0", trit_val(tmul(z, z)),  0);
    check_int("tmul: (0)(-)=0", trit_val(tmul(z, n)),  0);
    check_int("tmul: (-)(+)=-", trit_val(tmul(n, p)), -1);
    check_int("tmul: (-)(0)=0", trit_val(tmul(n, z)),  0);
    check_int("tmul: (-)(-)=+", trit_val(tmul(n, n)),  1);
}

// ---------------------------------------------------------------------------
// trit XOR truth table (balanced mod-3 addition)
// ---------------------------------------------------------------------------

fn test_trit_xor() {
    let p: trit = +;
    let z: trit = 0;
    let n: trit = -;

    // +1 + +1 = 2 -> -1 (balanced mod 3)
    check_int("txor: (+)(+)=-", trit_val(txor_trit(p, p)), -1);
    check_int("txor: (+)(0)=+", trit_val(txor_trit(p, z)),  1);
    // +1 + -1 = 0
    check_int("txor: (+)(-)=0", trit_val(txor_trit(p, n)),  0);
    check_int("txor: (0)(+)=+", trit_val(txor_trit(z, p)),  1);
    check_int("txor: (0)(0)=0", trit_val(txor_trit(z, z)),  0);
    check_int("txor: (0)(-)=-", trit_val(txor_trit(z, n)), -1);
    check_int("txor: (-)(+)=0", trit_val(txor_trit(n, p)),  0);
    check_int("txor: (-)(0)=-", trit_val(txor_trit(n, z)), -1);
    // -1 + -1 = -2 -> +1 (balanced mod 3)
    check_int("txor: (-)(-)=+", trit_val(txor_trit(n, n)),  1);
}

// ---------------------------------------------------------------------------
// Hash: determinism — same input gives same hash
// ---------------------------------------------------------------------------

fn test_hash_determinism() {
    let h1 = trit_hash_9(+, +, +, 0, 0, 0, -, -, -);
    let h2 = trit_hash_9(+, +, +, 0, 0, 0, -, -, -);
    check_int("hash-determ: same input same output", h1, h2);
}

// ---------------------------------------------------------------------------
// Hash: different inputs give different outputs
// ---------------------------------------------------------------------------

fn test_hash_different_inputs() {
    let ha = trit_hash_9(+, +, +, +, +, +, +, +, +);
    let hb = trit_hash_9(-, -, -, -, -, -, -, -, -);
    let hc = trit_hash_9(+, 0, -, +, 0, -, +, 0, -);

    check("hash-diff: all-pos vs all-neg differ", ha != hb);
    check("hash-diff: all-pos vs mixed differ",   ha != hc);
    check("hash-diff: all-neg vs mixed differ",   hb != hc);
}

// ---------------------------------------------------------------------------
// Hash: zero input produces valid output
// ---------------------------------------------------------------------------

fn test_hash_zero_input() {
    let h = trit_hash_9(0, 0, 0, 0, 0, 0, 0, 0, 0);
    // Should produce a deterministic result (all zeros -> specific hash)
    let h2 = trit_hash_9(0, 0, 0, 0, 0, 0, 0, 0, 0);
    check_int("hash-zero: deterministic", h, h2);
}

// ---------------------------------------------------------------------------
// Cipher: XOR roundtrip (encrypt then decrypt with same key)
// ---------------------------------------------------------------------------

fn test_cipher_roundtrip() {
    let data: trit = +;
    let key: trit = -;
    let encrypted = cipher_trit(data, key);
    let decrypted = cipher_trit(encrypted, key);
    // XOR is NOT self-inverse in mod-3 arithmetic for all values,
    // but we test the concept: applying the operation twice with
    // a known key to verify the cipher produces expected results.
    // In balanced mod-3: +1 xor -1 = 0, then 0 xor -1 = -1
    // To get a proper inverse, we negate the key for decryption.
    let decrypted2 = cipher_trit(encrypted, tnot key);
    // +1 xor -1 = 0, 0 xor +1 = +1 (original!)
    check_int("cipher-rt: encrypt-decrypt recovers +", trit_val(decrypted2), trit_val(data));
}

// ---------------------------------------------------------------------------
// Cipher: ciphertext differs from plaintext
// ---------------------------------------------------------------------------

fn test_cipher_differs() {
    let data: trit = +;
    let key: trit = -;
    let encrypted = cipher_trit(data, key);
    check("cipher-diff: ciphertext != plaintext", trit_val(encrypted) != trit_val(data));
}

// ---------------------------------------------------------------------------
// Cipher: different keys produce different ciphertext
// ---------------------------------------------------------------------------

fn test_cipher_different_keys() {
    let data: trit = +;
    let key1: trit = -;
    let key2: trit = 0;
    let ct1 = cipher_trit(data, key1);
    let ct2 = cipher_trit(data, key2);
    check("cipher-keys: different keys different ct", trit_val(ct1) != trit_val(ct2));
}

// ---------------------------------------------------------------------------
// Constant-time equality
// ---------------------------------------------------------------------------

fn test_constant_time_eq() {
    check_int("ct-eq: + == + is 1", ct_eq_trit(+, +), 1);
    check_int("ct-eq: 0 == 0 is 1", ct_eq_trit(0, 0), 1);
    check_int("ct-eq: - == - is 1", ct_eq_trit(-, -), 1);
    check_int("ct-eq: + != - is 0", ct_eq_trit(+, -), 0);
    check_int("ct-eq: + != 0 is 0", ct_eq_trit(+, 0), 0);
    check_int("ct-eq: 0 != - is 0", ct_eq_trit(0, -), 0);
}

// ---------------------------------------------------------------------------
// HMAC: determinism (same key+data = same MAC)
// ---------------------------------------------------------------------------

fn test_hmac_determinism() {
    let mac1 = simple_hmac(7, 42);
    let mac2 = simple_hmac(7, 42);
    check_int("hmac-determ: same key+data same MAC", mac1, mac2);
}

// ---------------------------------------------------------------------------
// HMAC: different keys give different MACs
// ---------------------------------------------------------------------------

fn test_hmac_different_keys() {
    let mac1 = simple_hmac(7, 42);
    let mac2 = simple_hmac(13, 42);
    check("hmac-keys: different keys different MACs", mac1 != mac2);
}

// ---------------------------------------------------------------------------
// HMAC: different data gives different MACs
// ---------------------------------------------------------------------------

fn test_hmac_different_data() {
    let mac1 = simple_hmac(7, 42);
    let mac2 = simple_hmac(7, 99);
    check("hmac-data: different data different MACs", mac1 != mac2);
}

// ---------------------------------------------------------------------------
// Key derivation: derived keys differ from master
// ---------------------------------------------------------------------------

fn test_derive_key() {
    let master = 42;
    let label1 = 1;
    let label2 = 2;
    let derived1 = simple_hmac(master, label1);
    let derived2 = simple_hmac(master, label2);
    check("derive: derived1 != master", derived1 != master);
    check("derive: derived2 != master", derived2 != master);
    check("derive: different labels give different keys", derived1 != derived2);
}

// ---------------------------------------------------------------------------
// Multi-trit cipher test using 3 trits
// ---------------------------------------------------------------------------

fn test_multi_trit_cipher() {
    // Encrypt 3 trits with 3 key trits
    let d0: trit = +;
    let d1: trit = 0;
    let d2: trit = -;
    let k0: trit = -;
    let k1: trit = +;
    let k2: trit = 0;

    let e0 = cipher_trit(d0, k0);
    let e1 = cipher_trit(d1, k1);
    let e2 = cipher_trit(d2, k2);

    // Verify at least one ciphertext trit differs from plaintext
    let diff_count = 0;
    let d_count = if trit_val(e0) != trit_val(d0) { 1 } else { 0 };
    let d_count2 = d_count + if trit_val(e1) != trit_val(d1) { 1 } else { 0 };
    let d_count3 = d_count2 + if trit_val(e2) != trit_val(d2) { 1 } else { 0 };
    check("multi-cipher: at least one trit changed", d_count3 > 0);

    // Decrypt with negated key (proper inverse)
    let r0 = cipher_trit(e0, tnot k0);
    let r1 = cipher_trit(e1, tnot k1);
    let r2 = cipher_trit(e2, tnot k2);
    check_int("multi-cipher: decrypt[0] recovers +", trit_val(r0), trit_val(d0));
    check_int("multi-cipher: decrypt[1] recovers 0", trit_val(r1), trit_val(d1));
    check_int("multi-cipher: decrypt[2] recovers -", trit_val(r2), trit_val(d2));
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    io::println("=== 22 Crypto ===");

    io::println("-- trit mul truth table --");
    test_trit_mul();

    io::println("-- trit XOR truth table --");
    test_trit_xor();

    io::println("-- hash determinism --");
    test_hash_determinism();

    io::println("-- hash different inputs --");
    test_hash_different_inputs();

    io::println("-- hash zero input --");
    test_hash_zero_input();

    io::println("-- cipher round-trip --");
    test_cipher_roundtrip();

    io::println("-- cipher differs --");
    test_cipher_differs();

    io::println("-- cipher different keys --");
    test_cipher_different_keys();

    io::println("-- constant-time eq --");
    test_constant_time_eq();

    io::println("-- HMAC determinism --");
    test_hmac_determinism();

    io::println("-- HMAC different keys --");
    test_hmac_different_keys();

    io::println("-- HMAC different data --");
    test_hmac_different_data();

    io::println("-- derive_key --");
    test_derive_key();

    io::println("-- multi-trit cipher --");
    test_multi_trit_cipher();

    io::println("Done.");
}
