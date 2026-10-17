//! FFDN feedback matrices – GPL-3.0
//!
//! A feedback matrix determines how energy from one delay line is
//! distributed to all the others on every round trip through the
//! network. For the FDN to be stable, the matrix must be
//! **lossless** — i.e. its singular values are all 1.0 — so that a
//! feedback gain `g < 1` produces a decaying tail and never blows
//! up.
//!
//! # Why Householder?
//!
//! The classical choices are:
//!
//!   * **Hadamard**     — requires N to be a power of two; highly
//!                        structured, so the echo pattern can have
//!                        audible periodicities.
//!   * **Random**       — perfect mixing, but N² multiplies per
//!                        sample. For N=16 that's 256 ops/sample.
//!   * **Householder**  — reflections through a hyperplane; lossless,
//!                        mixes well, and reduces to **2N** ops per
//!                        sample by exploiting the structure of the
//!                        reflection formula. For N=16 that's 32
//!                        ops/sample.
//!
//! Householder wins on the cost/quality curve, so it's our default.

/// Maximum number of delay lines per FFDN network. The matrix types
/// store their state in fixed-size arrays of this length so that
/// changing the active line count or matrix type never allocates on
/// the audio thread. Must match `crate::ffdn::MAX_LINES`.
pub const MAX_LINES: usize = 16;

pub trait FeedbackMatrix: Send {
    fn size(&self) -> usize;
    fn apply(&mut self, x: &[f64], y: &mut [f64]);
}

pub struct HouseholderMatrix {
    /// Sign vector, one ±1 per line. Only the first `n` elements are
    /// meaningful; the rest are unused and stay zero.
    signs: [f64; MAX_LINES],
    /// Actual matrix size (4..=MAX_LINES). Kept separate from the
    /// array length so `size()` returns the correct value.
    n: usize,
    /// Cached 1/N, so we do a multiply instead of a divide per sample.
    inv_n: f64,
}

impl HouseholderMatrix {
    pub fn new(n: usize, seed: u64) -> Self {
        debug_assert!(n <= MAX_LINES, "HouseholderMatrix size > MAX_LINES");
        let n = n.min(MAX_LINES).max(1);
        let mut state = seed | 1;
        let mut signs = [0.0; MAX_LINES];
        for i in 0..n {
            // xorshift64 — fast, deterministic, no external crate.
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            signs[i] = if (state & 1) == 0 { 1.0 } else { -1.0 };
        }
        Self {
            signs,
            n,
            inv_n: 1.0 / n as f64,
        }
    }
}

impl FeedbackMatrix for HouseholderMatrix {
    #[inline]
    fn size(&self) -> usize {
        self.n
    }

    fn apply(&mut self, x: &[f64], y: &mut [f64]) {
        let n = self.n;
        debug_assert_eq!(x.len(), n);
        debug_assert_eq!(y.len(), n);

        // Pass 1: compute the dot product of `signs` with `x`.
        let mut dot = 0.0;
        for i in 0..n {
            dot += self.signs[i] * x[i];
        }
        let scale = -2.0 * dot * self.inv_n;

        // Pass 2: subtract the reflected component.
        for i in 0..n {
            y[i] = x[i] + scale * self.signs[i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the matrix is orthogonal by comparing
    /// `AᵀA = I` numerically. This is the property that guarantees
    /// feedback stability.
    #[test]
    fn householder_is_orthogonal() {
        let n = 8;
        let mut m = HouseholderMatrix::new(n, 0xDEAD_BEEF);

        // Build A explicitly by applying it to each unit vector.
        let mut a = vec![vec![0.0; n]; n];
        let mut x = vec![0.0; n];
        let mut y = vec![0.0; n];
        for col in 0..n {
            for i in 0..n {
                x[i] = if i == col { 1.0 } else { 0.0 };
            }
            m.apply(&x, &mut y);
            for row in 0..n {
                a[row][col] = y[row];
            }
        }

        // Compute AᵀA and check it's the identity.
        for i in 0..n {
            for j in 0..n {
                let mut sum = 0.0;
                for k in 0..n {
                    sum += a[k][i] * a[k][j];
                }
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (sum - expected).abs() < 1e-12,
                    "AᵀA[{},{}] = {} (expected {})",
                    i,
                    j,
                    sum,
                    expected
                );
            }
        }
    }

    /// Ensures the transform preserves signal energy.
    #[test]
    fn householder_preserves_energy() {
        let n = 8;
        let mut m = HouseholderMatrix::new(n, 42);
        let x: Vec<f64> = (0..n).map(|i| (i as f64 * 0.37).sin()).collect();
        let mut y = vec![0.0; n];
        m.apply(&x, &mut y);

        let e_in: f64 = x.iter().map(|v| v * v).sum();
        let e_out: f64 = y.iter().map(|v| v * v).sum();
        assert!(
            (e_in - e_out).abs() < 1e-12,
            "energy in = {}, out = {}",
            e_in,
            e_out
        );
    }
}
