//! FastSin – GPL-3.0
//! Rust port of `CloudSeed.Native/FastSin.h` (legacy-v1): a 32768-entry sine lookup table
//! used by the modulation oscillators inside `ModulatedAllpass` / `ModulatedDelay`.

use std::sync::LazyLock;

const DATA_SIZE: usize = 32768;

static DATA: LazyLock<Vec<f64>> = LazyLock::new(|| {
    (0..DATA_SIZE)
        .map(|i| (2.0 * std::f64::consts::PI * i as f64 / DATA_SIZE as f64).sin())
        .collect()
});

/// Returns `sin(2π · index)` where `index` is a phase in `[0, 1)`.
#[inline]
pub fn get(index: f64) -> f64 {
    let i = (index * 32767.99999) as usize;
    DATA[i.min(DATA_SIZE - 1)]
}
