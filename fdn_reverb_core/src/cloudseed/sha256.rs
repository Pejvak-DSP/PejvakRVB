//! SHA-256 – GPL-3.0
//! Rust port of `CloudSeed.Native/Utils/Sha256.h/.cpp` (legacy-v1), itself based on the
//! public-domain FIPS 180-2 implementation by Olivier Gay (2005). The byte-exact behaviour is
//! required by `ShaRandom`, which hashes an 8-byte little-endian seed to generate the
//! deterministic decorrelation series used throughout the CloudSeed engine.
//!
//! Two entry points are provided:
//!   • `sha256`   – general-purpose, returns a `Vec` (used by tests / tooling),
//!   • `sha256_8` – zero-allocation variant for the exact 8-byte seed hashing that
//!     `ShaRandom` performs in the audio thread.

/// The 64 SHA-256 round constants.
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Initial hash state (H0).
const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

#[inline(always)]
fn rotr(x: u32, n: u32) -> u32 {
    x.rotate_right(n)
}

/// Compresses a single 64-byte message block into `state`.
fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for (i, chunk) in block.as_chunks::<4>().0.iter().enumerate() {
        w[i] = u32::from_be_bytes(*chunk);
    }
    for i in 16..64 {
        let s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >> 3);
        let s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }

    let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h) = (
        state[0], state[1], state[2], state[3], state[4], state[5], state[6], state[7],
    );

    for i in 0..64 {
        let s1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

/// Finalises `state` into the 32-byte digest.
fn finalise(state: &[u32; 8]) -> [u8; 32] {
    let mut digest = [0u8; 32];
    for (i, s) in state.iter().enumerate() {
        digest[i * 4..i * 4 + 4].copy_from_slice(&s.to_be_bytes());
    }
    digest
}

/// Zero-allocation SHA-256 of exactly 8 bytes (the seed used by `ShaRandom`).
/// The message fits in a single padded 64-byte block, so no heap is touched.
pub fn sha256_8(input: &[u8; 8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    block[..8].copy_from_slice(input);
    block[8] = 0x80;
    // 8 bytes → bit length 64, stored big-endian at the end of the block.
    block[56..64].copy_from_slice(&64u64.to_be_bytes());

    let mut state = H0;
    compress(&mut state, &block);
    finalise(&state)
}

/// Computes the SHA-256 digest of `input` (general-purpose, heap-allocating).
#[cfg(test)]
pub fn sha256(input: &[u8]) -> [u8; 32] {
    let mut state = H0;

    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut msg = Vec::with_capacity(input.len() + 72);
    msg.extend_from_slice(input);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for block in msg.as_chunks::<64>().0 {
        compress(&mut state, block);
    }

    finalise(&state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_empty() {
        let d = sha256(&[]);
        let hex: String = d.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_abc() {
        let d = sha256(b"abc");
        let hex: String = d.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_sha256_8_matches_general() {
        // 8-byte input must produce the same digest through both paths.
        for seed in [0i64, 1, 12345, -1, i64::MAX, i64::MIN] {
            let bytes = seed.to_le_bytes();
            let a = sha256_8(&bytes);
            let b = sha256(&bytes);
            assert_eq!(a, b, "seed {seed}");
        }
    }
}
