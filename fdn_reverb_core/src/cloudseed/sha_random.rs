//! AudioLib::ShaRandom – GPL-3.0
//! Byte-exact Rust port of `CloudSeed.Native/AudioLib/ShaRandom.h/.cpp` (legacy-v1).
//!
//! Generates a deterministic pseudo-random series in [0, 1) by iterating SHA-256 over an
//! 8-byte little-endian `i64` seed and reading back `u32` values in little-endian order
//! (matching the original x86 memory layout used by the C++ port).
//!
//! Two flavours are provided:
//!   • `generate_into` / `generate_cross_into` – **zero-allocation**, write into caller
//!     arrays (used on the audio thread when seeds change),
//!   • `generate` / `generate_cross` – convenience wrappers returning `Vec` (tests/tooling).

use crate::cloudseed::sha256::sha256_8;

/// Maximum number of values the zero-allocation path supports.
/// `ShaRandom` is used with counts of 24, 36 and 100 in this engine; 256 is a safe cap.
pub const MAX_COUNT: usize = 256;

/// `ShaRandom::Generate(seed, count)` into a caller-provided slice (zero-allocation).
/// The slice must have at least `count` elements.
pub fn generate_into(seed: i64, count: usize, out: &mut [f64]) {
    // C++: `auto iterations = count * sizeof(unsigned int) / (256 / 8) + 1;`
    let iterations = count.wrapping_mul(4) / 32 + 1;
    // C++: `bytes = sha256(&bytes[0], 8)` – the first hash is over the 8-byte seed, every
    // following hash is over the *first 8 bytes* of the previous 32-byte digest.
    let mut block: [u8; 8] = seed.to_le_bytes();
    // Byte list buffer: iterations * 32 bytes. For count ≤ MAX_COUNT this fits comfortably.
    let mut bytes = [0u8; MAX_COUNT * 4 + 64];

    for it in 0..iterations {
        let digest = sha256_8(&block);
        let start = it * 32;
        bytes[start..start + 32].copy_from_slice(&digest);
        block.copy_from_slice(&digest[..8]);
    }

    for (i, slot) in out.iter_mut().take(count).enumerate() {
        let base = i * 4;
        let val = u32::from_le_bytes([
            bytes[base],
            bytes[base + 1],
            bytes[base + 2],
            bytes[base + 3],
        ]);
        *slot = val as f64 / u32::MAX as f64;
    }
}

/// `ShaRandom::Generate(seed, count, crossSeed)` into a caller slice (zero-allocation):
/// mixes the series of `seed` with the series of its bitwise complement.
pub fn generate_cross_into(seed: i64, count: usize, cross_seed: f64, out: &mut [f64]) {
    let mut a = [0.0f64; MAX_COUNT];
    let mut b = [0.0f64; MAX_COUNT];
    generate_into(seed, count, &mut a[..count]);
    generate_into(!seed, count, &mut b[..count]); // C++: `auto seedB = ~seed;`

    for i in 0..count {
        out[i] = a[i] * (1.0 - cross_seed) + b[i] * cross_seed;
    }
}

/// `ShaRandom::Generate(seed, count)` – convenience wrapper (heap-allocating).
#[cfg(test)]
pub fn generate(seed: i64, count: usize) -> Vec<f64> {
    let mut out = vec![0.0f64; count];
    generate_into(seed, count, &mut out);
    out
}

/// `ShaRandom::Generate(seed, count, crossSeed)` – convenience wrapper (heap-allocating).
#[cfg(test)]
pub fn generate_cross(seed: i64, count: usize, cross_seed: f64) -> Vec<f64> {
    let mut out = vec![0.0f64; count];
    generate_cross_into(seed, count, cross_seed, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_deterministic() {
        let mut a = [0.0f64; 24];
        let mut b = [0.0f64; 24];
        generate_into(12345, 24, &mut a);
        generate_into(12345, 24, &mut b);
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x, y);
        }
        // First value reference from a Python model of the same algorithm.
        assert!((a[0] - 0.317_220_025).abs() < 1e-4);
    }

    #[test]
    fn test_generate_cross_range() {
        let mut v = [0.0f64; 100];
        generate_cross_into(7, 100, 0.5, &mut v);
        for x in &v {
            assert!((0.0..=1.0).contains(x));
        }
    }
}
