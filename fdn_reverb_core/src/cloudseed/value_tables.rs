//! AudioLib::ValueTables – GPL-3.0
//! Rust port of `CloudSeed.Native/AudioLib/ValueTables.h/.cpp` (legacy-v1).
//!
//! The original precomputes 4001-entry lookup tables at startup and normalises them so
//! `t[0] = 0` and `t[4000] = 1`. Only the tables actually used by `ReverbController`'s
//! parameter scaling are ported (`Response2Dec`, `Response3Dec`, `Response4Oct`).

use std::sync::LazyLock;

/// Table size used by the original (indices 0..=4000).
pub const TABLE_SIZE: usize = 4001;

/// Builds a table with the original two-pass construction:
/// `t[i] = f0(i / 4000)`, then normalised by `(t[i] - t[0]) / (1 - t[0])` and `t[0] = 0`.
fn build(f0: fn(f64) -> f64) -> Vec<f64> {
    let mut t = Vec::with_capacity(TABLE_SIZE);
    for i in 0..TABLE_SIZE {
        let x = i as f64 / 4000.0;
        t.push(f0(x));
    }
    let t0 = t[0];
    let denom = 1.0 - t0;
    for i in 1..TABLE_SIZE {
        t[i] = (t[i] - t0) / denom;
    }
    t[0] = 0.0;
    t
}

/// 2-decade response: `100^x / 100`.
fn resp_2_dec(x: f64) -> f64 {
    (100.0f64).powf(x) / 100.0
}

/// 3-decade response: `1000^x / 1000`.
fn resp_3_dec(x: f64) -> f64 {
    (1000.0f64).powf(x) / 1000.0
}

/// 4-octave response: `(16^x - 1) / 16 + 0.125/2`.
fn resp_4_oct(x: f64) -> f64 {
    ((16.0f64).powf(x) - 1.0) / 16.0 + 0.125 / 2.0
}

static RESPONSE_2_DEC: LazyLock<Vec<f64>> = LazyLock::new(|| build(resp_2_dec));
static RESPONSE_3_DEC: LazyLock<Vec<f64>> = LazyLock::new(|| build(resp_3_dec));
static RESPONSE_4_OCT: LazyLock<Vec<f64>> = LazyLock::new(|| build(resp_4_oct));

/// `ValueTables::Get(index, table)` – looks up a normalised `index` in `[0, 1]`.
#[inline]
pub fn get(index: f64, table: &[f64]) -> f64 {
    let idx = (index * 4000.999) as usize;
    table[idx.min(TABLE_SIZE - 1)]
}

/// 2-decade response table lookup (`Response2Dec`).
#[inline]
pub fn response_2_dec(index: f64) -> f64 {
    get(index, &RESPONSE_2_DEC)
}

/// 3-decade response table lookup (`Response3Dec`).
#[inline]
pub fn response_3_dec(index: f64) -> f64 {
    get(index, &RESPONSE_3_DEC)
}

/// 4-octave response table lookup (`Response4Oct`).
#[inline]
pub fn response_4_oct(index: f64) -> f64 {
    get(index, &RESPONSE_4_OCT)
}

/// Inverse lookup (binary search + linear interpolation) for a target response value.
/// Used by the plugin's text-entry parsing so a typed physical value maps back to a
/// normalised parameter.
#[inline]
fn invert(table: &[f64], value: f64) -> f64 {
    let v = value.clamp(table[0], table[TABLE_SIZE - 1]);
    let mut lo = 0usize;
    let mut hi = TABLE_SIZE - 1;
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if table[mid] < v {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let x0 = lo as f64 / 4000.0;
    let x1 = hi as f64 / 4000.0;
    let v0 = table[lo];
    let v1 = table[hi];
    let t = if (v1 - v0).abs() < 1e-15 {
        0.0
    } else {
        ((v - v0) / (v1 - v0)).clamp(0.0, 1.0)
    };
    (x0 + (x1 - x0) * t).clamp(0.0, 1.0)
}

/// Inverse of [`response_2_dec`].
pub fn invert_response_2_dec(value: f64) -> f64 {
    invert(&RESPONSE_2_DEC, value)
}

/// Inverse of [`response_3_dec`].
pub fn invert_response_3_dec(value: f64) -> f64 {
    invert(&RESPONSE_3_DEC, value)
}

/// Inverse of [`response_4_oct`].
pub fn invert_response_4_oct(value: f64) -> f64 {
    invert(&RESPONSE_4_OCT, value)
}
