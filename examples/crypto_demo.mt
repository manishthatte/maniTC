// crypto_demo.mt — Ternary Cryptography Demonstration
//
// Demonstrates balanced ternary cryptographic primitives:
// hash, HMAC, TRNG, symmetric cipher.
//
// Authored by: Manish Jagdish Thatte

use std::crypto;

fn main() {
    print("=== Ternary Cryptography Demo ===");
    print("");

    // Ternary hash
    print("--- Trit Hash ---");
    let data: [trit; 27] = [+1, -1, 0, +1, -1, 0, +1, -1, 0,
                             +1, -1, 0, +1, -1, 0, +1, -1, 0,
                             +1, -1, 0, +1, -1, 0, +1, -1, 0];
    let digest = crypto::trit_hash(data);
    print("  Input:  27-trit word");
    print("  Digest: 27-trit hash");
    print("  Hash[0..2]: ", digest[0], " ", digest[1], " ", digest[2]);
    print("");

    // HMAC
    print("--- Trit HMAC ---");
    let key: [trit; 27] = [+1; 27];  // all +1 key
    let mac = crypto::trit_hmac(key, data);
    print("  Key:  all +1");
    print("  HMAC[0..2]: ", mac[0], " ", mac[1], " ", mac[2]);
    print("");

    // Symmetric cipher
    print("--- Symmetric Cipher ---");
    let cipher = crypto::cipher_init(key);
    let plaintext: [trit; 27] = [+1, 0, -1, +1, 0, -1, +1, 0, -1,
                                  +1, 0, -1, +1, 0, -1, +1, 0, -1,
                                  +1, 0, -1, +1, 0, -1, +1, 0, -1];
    let ciphertext = crypto::cipher_encrypt(cipher, plaintext);
    let recovered = crypto::cipher_decrypt(cipher, ciphertext);
    print("  Plaintext[0..2]:  ", plaintext[0], " ", plaintext[1], " ", plaintext[2]);
    print("  Ciphertext[0..2]: ", ciphertext[0], " ", ciphertext[1], " ", ciphertext[2]);
    print("  Recovered[0..2]:  ", recovered[0], " ", recovered[1], " ", recovered[2]);

    // Verify round-trip
    let match_count: int = 0;
    let i: int = 0;
    while i < 27 {
        tif plaintext[i] == recovered[i] {
            match_count = match_count + 1;
        }
        i = i + 1;
    }
    print("  Round-trip: ", match_count, "/27 trits match");
    print("");

    print("=== Crypto demo complete ===");
}
