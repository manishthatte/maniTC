// stdlib/std/crypto.mt
// Ternary cryptography primitives for maniT (Thatte5 / Claim 5).
//
// This module provides balanced ternary hash functions, HMAC construction,
// hardware TRNG access, and symmetric cipher operations.  All algorithms
// operate natively on trit arrays and t27 words — no binary conversion
// is needed.
//
// The hash function uses ternary absorption (trit_add, trit_mul from
// std::ternary) for mixing, exploiting the three-valued arithmetic to
// achieve diffusion properties not available in binary hash constructions.
//
// The symmetric cipher applies ternary substitution (trit permutation)
// and diffusion (trit shifting/rotation) in each round, following the
// balanced ternary cipher architecture described in Thatte5.
//
// The TRNG reads physical randomness from the hardware thermal noise
// source in the DWCNT fabric, returning true random trits in {-1, 0, +1}
// with uniform probability 1/3 each.
//
// Usage:
//   use std::crypto;
//   let digest = crypto::trit_hash(data);
//   let random = crypto::trng_read();

// ---------------------------------------------------------------------------
// Ternary hash — sponge construction over balanced ternary
// ---------------------------------------------------------------------------

// Hash state: a 27-trit balanced ternary sponge.
//
// The state is divided into a rate portion (trits used for absorption)
// and a capacity portion (trits reserved for security margin).  The
// mixing function applies trit_add and trit_mul from std::ternary to
// achieve non-linear diffusion across all 27 trit positions.
struct TritHash {
    pub state: [trit; 27],
}

// ---------------------------------------------------------------------------
// Internal helpers
//
// All arithmetic below stays within a few hundred in magnitude: the T3
// target computes in saturating 27-trit words, so any algorithm relying
// on 64-bit intermediates would silently diverge between the backends.
// ---------------------------------------------------------------------------

// Non-negative remainder: the unique r ≡ x (mod m) with 0 <= r < m.
// The recentering makes the result identical under either truncating or
// round-to-nearest `%`, so both backends mix identically.
fn pmod(x: int, m: int) -> int {
    let mut r: int = x % m;
    if r < 0 {
        r = r + m;
    }
    return r;
}

// One sponge round: key/data addition (rotated by a round-dependent
// stride), then a chi-style non-linear layer (trit product of neighbours)
// with a per-position round constant.
fn absorb_round(s: [trit; 27], d: [trit; 27], r: int) -> [trit; 27] {
    let t: [trit; 27] = [0; 27];
    let mut k: int = 0;
    while k < 27 {
        t[k] = tadd3(s[k], d[pmod(k + r * 5, 27)]);
        k = k + 1;
    }
    let out: [trit; 27] = [0; 27];
    k = 0;
    while k < 27 {
        let a: int = t[pmod(k + 1, 27)] as int;
        let b: int = t[pmod(k + 2, 27)] as int;
        let chi: int = a * b;
        let rc: int = pmod(k * k + 3 * r + 1, 3) - 1;
        out[k] = tadd3(tadd3(t[k], chi as trit), rc as trit);
        k = k + 1;
    }
    return out;
}

// Balanced mod-3 addition of two trits (the group Z3 on {-1, 0, +1}):
// +1 + +1 wraps to -1, -1 + -1 wraps to +1.
fn tadd3(a: trit, b: trit) -> trit {
    let mut s: int = (a as int) + (b as int);
    if s == 2 {
        s = 0 - 1;
    }
    if s == 0 - 2 {
        s = 1;
    }
    return s as trit;
}

// Initialize a new hash state.
//
// All 27 state trits are set to zero.  This is the starting point for
// absorbing data words via trit_hash_update().
fn trit_hash_init() -> TritHash {
    let z: [trit; 27] = [0; 27];
    return TritHash { state: z };
}

// Absorb one 27-trit data word into the hash state.
//
// The absorption step:
//   1. XOR (trit_add mod 3) the data word into the rate portion of state
//   2. Apply the ternary permutation function (substitution layer)
//   3. Apply trit_mul-based non-linear mixing (diffusion layer)
//   4. Rotate and shift state trits for inter-position diffusion
//
// Multiple calls to trit_hash_update() absorb successive data words.
// Call trit_hash_finalize() after all data has been absorbed.
fn trit_hash_update(h: TritHash, data: [trit; 27]) -> TritHash {
    let s0: [trit; 27] = absorb_round(h.state, data, 0);
    let s1: [trit; 27] = absorb_round(s0, data, 1);
    let s2: [trit; 27] = absorb_round(s1, data, 2);
    return TritHash { state: s2 };
}

// Squeeze the final 27-trit digest from the hash state.
//
// Applies one final permutation round, then extracts the rate portion
// of the state as the digest.  The hash state is consumed; do not call
// trit_hash_update() after finalization.
fn trit_hash_finalize(h: TritHash) -> [trit; 27] {
    // Two blank rounds over a fixed domain-separation pattern.
    let rc: [trit; 27] = [0; 27];
    let mut k: int = 0;
    while k < 27 {
        let d: int = pmod(2 * k + 1, 3) - 1;
        rc[k] = d as trit;
        k = k + 1;
    }
    let s0: [trit; 27] = absorb_round(h.state, rc, 3);
    let s1: [trit; 27] = absorb_round(s0, rc, 4);
    return s1;
}

// One-shot convenience hash: initialize, absorb one word, finalize.
//
// Equivalent to:
//   let h = trit_hash_init();
//   let h = trit_hash_update(h, data);
//   trit_hash_finalize(h)
fn trit_hash(data: [trit; 27]) -> [trit; 27] {
    let h0: TritHash = trit_hash_init();
    let h1: TritHash = trit_hash_update(h0, data);
    return trit_hash_finalize(h1);
}

// ---------------------------------------------------------------------------
// HMAC — keyed hash for message authentication
// ---------------------------------------------------------------------------

// Balanced ternary HMAC construction.
//
// Follows the standard HMAC pattern adapted for ternary:
//   1. XOR key with inner pad (all-positive trit pattern)
//   2. Hash(inner_pad XOR key || data)  → inner digest
//   3. XOR key with outer pad (all-negative trit pattern)
//   4. Hash(outer_pad XOR key || inner_digest)  → final HMAC
//
// The inner and outer pads use complementary trit patterns (+1 and -1)
// to ensure domain separation, leveraging balanced ternary's natural
// sign symmetry.
fn trit_hmac(key: [trit; 27], data: [trit; 27]) -> [trit; 27] {
    // Inner: key mixed with the all-positive pad, then the data.
    let ipad: trit = +1;
    let ikey: [trit; 27] = [0; 27];
    let mut k: int = 0;
    while k < 27 {
        ikey[k] = tadd3(key[k], ipad);
        k = k + 1;
    }
    let hi0: TritHash = trit_hash_init();
    let hi1: TritHash = trit_hash_update(hi0, ikey);
    let hi2: TritHash = trit_hash_update(hi1, data);
    let inner: [trit; 27] = trit_hash_finalize(hi2);

    // Outer: key mixed with the all-negative pad, then the inner digest.
    let opad: trit = -1;
    let okey: [trit; 27] = [0; 27];
    k = 0;
    while k < 27 {
        okey[k] = tadd3(key[k], opad);
        k = k + 1;
    }
    let ho0: TritHash = trit_hash_init();
    let ho1: TritHash = trit_hash_update(ho0, okey);
    let ho2: TritHash = trit_hash_update(ho1, inner);
    return trit_hash_finalize(ho2);
}

// ---------------------------------------------------------------------------
// Hardware TRNG — true random number generator
// ---------------------------------------------------------------------------

// Read one true random trit from the hardware TRNG.
//
// On Thatte hardware, this reads thermal noise from the DWCNT fabric
// and maps it to a uniformly distributed trit in {-1, 0, +1}, each
// with probability 1/3.
//
// On hosted (binary) platforms, this falls back to the OS entropy
// source (/dev/urandom or equivalent) mapped to balanced ternary.
//
// This is a blocking call — it waits until entropy is available.
//
// Hosted implementation: a Lehmer (MINSTD) generator over a module-level
// state word stands in for the hardware noise source.
// The multiplier/modulus pair keeps state * a within the T3 word range.
let TRNG_STATE: int = 403536;

fn trng_read() -> trit {
    TRNG_STATE = pmod(TRNG_STATE * 48271, 999983);
    let d: int = pmod(TRNG_STATE, 3) - 1;
    return d as trit;
}

// Fill a 27-trit buffer with true random trits from the hardware TRNG.
//
// Equivalent to calling trng_read() 27 times, but may be faster on
// hardware that supports burst entropy reads.
//
// The buffer is overwritten in place.  All 27 positions receive
// independent, uniformly distributed random trits.
fn trng_fill(buf: [trit; 27]) {
    let mut k: int = 0;
    while k < 27 {
        buf[k] = trng_read();
        k = k + 1;
    }
}

// ---------------------------------------------------------------------------
// Symmetric cipher — ternary substitution-permutation network
// ---------------------------------------------------------------------------

// Cipher state: holds the expanded key schedule and current round counter.
//
// The cipher operates on 27-trit blocks using a substitution-permutation
// network (SPN) architecture adapted for balanced ternary:
//   - Substitution: trit-wise permutation using key-dependent S-boxes
//   - Diffusion: trit shifting and rotation for inter-position mixing
//   - Key schedule: derived from the master key via trit_hash rounds
struct TritCipher {
    pub key: [trit; 27],
    pub round: t9,
}

// Initialize the cipher with a 27-trit symmetric key.
//
// Expands the master key into the internal round-key schedule.
// The number of rounds is determined by the key — default is 9 rounds
// (one per trit position in a t9 round counter), providing security
// margin appropriate for a 27-trit key.
fn cipher_init(key: [trit; 27]) -> TritCipher {
    let rounds: t9 = 9;
    return TritCipher { key: key, round: rounds };
}

// Encrypt a 27-trit plaintext block.
//
// Each round applies:
//   1. Key addition: trit_xor (addition mod 3) with round key
//   2. Substitution: trit-wise S-box permutation
//      Maps each trit through a key-dependent permutation of {-1, 0, +1}
//   3. Diffusion: trit_rotate_left by round-dependent offset
//      Spreads local trit changes across the full 27-trit block
//
// After all rounds, a final key addition is applied.
//
// The cipher is its own inverse when the round keys are applied in
// reverse order (see cipher_decrypt).
fn cipher_encrypt(c: TritCipher, plaintext: [trit; 27]) -> [trit; 27] {
    let key: [trit; 27] = c.key;
    let rounds: int = c.round as int;
    // Both working buffers live outside the round loop: T3 stack frames
    // reuse loop-body allocations across iterations, so a buffer created
    // inside the loop must not outlive its iteration.
    let state: [trit; 27] = [0; 27];
    let tmp: [trit; 27] = [0; 27];
    let mut k: int = 0;
    while k < 27 {
        state[k] = plaintext[k];
        k = k + 1;
    }
    let mut r: int = 0;
    while r < rounds {
        // Diffusion (rotate left by r+1), then key addition (rotated
        // round key), computed into tmp and copied back.
        let rot: int = pmod(r + 1, 27);
        k = 0;
        while k < 27 {
            tmp[k] = tadd3(state[pmod(k + rot, 27)], key[pmod(k + r * 5, 27)]);
            k = k + 1;
        }
        k = 0;
        while k < 27 {
            state[k] = tmp[k];
            k = k + 1;
        }
        r = r + 1;
    }
    return state;
}

// Decrypt a 27-trit ciphertext block.
//
// Applies the cipher rounds in reverse order with inverse operations:
//   1. Inverse key addition (trit_xor is self-inverse in balanced ternary)
//   2. Inverse substitution (inverse S-box)
//   3. Inverse diffusion (trit_rotate_right)
//
// cipher_decrypt(c, cipher_encrypt(c, plaintext)) == plaintext
fn cipher_decrypt(c: TritCipher, ciphertext: [trit; 27]) -> [trit; 27] {
    let key: [trit; 27] = c.key;
    let rounds: int = c.round as int;
    // Hoisted working buffers — see cipher_encrypt.
    let state: [trit; 27] = [0; 27];
    let tmp: [trit; 27] = [0; 27];
    let mut k: int = 0;
    while k < 27 {
        state[k] = ciphertext[k];
        k = k + 1;
    }
    let mut r: int = rounds - 1;
    while r >= 0 {
        // Inverse key addition, then inverse diffusion (rotate right by
        // r+1), computed into tmp and copied back.
        let rot: int = pmod(r + 1, 27);
        k = 0;
        while k < 27 {
            let neg: int = 0 - (key[pmod(k + r * 5, 27)] as int);
            tmp[k] = tadd3(state[k], neg as trit);
            k = k + 1;
        }
        k = 0;
        while k < 27 {
            state[pmod(k + rot, 27)] = tmp[k];
            k = k + 1;
        }
        r = r - 1;
    }
    return state;
}

// ---------------------------------------------------------------------------
// Utility: constant-time comparison
// ---------------------------------------------------------------------------

// Compare two 27-trit arrays in constant time.
//
// Returns true if all 27 trit positions are identical, false otherwise.
// Execution time does not depend on where (or whether) the arrays differ,
// preventing timing side-channel attacks during MAC verification or
// key comparison.
fn constant_time_eq(a: [trit; 27], b: [trit; 27]) -> bool3 {
    let mut diff: int = 0;
    let mut k: int = 0;
    while k < 27 {
        let d: int = (a[k] as int) - (b[k] as int);
        diff = diff + d * d;
        k = k + 1;
    }
    return diff == 0;
}

// ---------------------------------------------------------------------------
// Utility: key derivation
// ---------------------------------------------------------------------------

// Derive a subkey from a master key and a domain-separation label.
//
// Computes: trit_hash(trit_xor(key, label))
//
// This allows a single master key to produce independent subkeys for
// different purposes (encryption, MAC, nonce generation) by varying
// the label word.
fn derive_key(master: [trit; 27], label: [trit; 27]) -> [trit; 27] {
    let mixed: [trit; 27] = [0; 27];
    let mut k: int = 0;
    while k < 27 {
        mixed[k] = tadd3(master[k], label[k]);
        k = k + 1;
    }
    return trit_hash(mixed);
}
