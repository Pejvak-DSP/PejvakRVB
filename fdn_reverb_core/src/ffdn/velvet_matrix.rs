//! Velvet (blocked) feedback matrix – GPL-3.0
//!
//! A sparse feedback matrix that mixes each delay line with only a
//! small, fixed number of neighbours — the "Velvet" pattern
//! described by Fagerström et al. (DAFx 2020). Compared to the fully
//! dense Householder matrix, the sparse version produces a more
//! natural, less metallic echo pattern at the same O(N) cost.
//!
//! # Structure
//!
//! The matrix is the composition of two operations:
//!
//!   1. **Blocked Householder**: the N lines are grouped into blocks
//!      of `BLOCK_SIZE`, and each block undergoes an independent
//!      Householder reflection. This mixes lines within a block.
//!
//!   2. **Permutation**: a fixed pseudo-random permutation shuffles
//!      the lines after the block step. Because the permutation is
//!      applied *after* the reflection, every line ends up receiving
//!      energy from a block it does not "naturally" belong to — the
//!      mixing graph is thus non-trivial and becomes dense over two
//!      round trips through the network.
//!
//! The composition `P · H` of an orthogonal permutation and an
//! orthogonal block-Householder is itself orthogonal, so the
//! stability guarantee of the FDN is preserved exactly.

use super::feedback_matrix::{FeedbackMatrix, MAX_LINES};

const BLOCK_SIZE: usize = 4;

pub struct VelvetMatrix {
    n: usize,
    /// Flattened sign vectors, one per block: `signs[b * BS + k]`.
    /// Only the first `n` elements are meaningful.
    signs: [f64; MAX_LINES],
    /// Fixed post-mix permutation. Only the first `n` entries are used.
    perm: [usize; MAX_LINES],
    /// Scratch buffer for the intermediate post-Householder values.
    scratch: [f64; MAX_LINES],
    /// Cached `1 / BLOCK_SIZE`.
    inv_block: f64,
    /// Size of the final partial block (`n % BLOCK_SIZE`); 0 when `n`
    /// is a multiple of `BLOCK_SIZE`.
    rem: usize,
    /// Cached `1 / rem`.
    inv_rem: f64,
}

impl VelvetMatrix {
    pub fn new(n: usize, seed: u64) -> Self {
        // `n` may be any line count in [BLOCK_SIZE, MAX_LINES]. A final
        // partial block (when `n` is not a multiple of `BLOCK_SIZE`)
        // is handled inside `apply` as a smaller Householder
        // reflection — still orthogonal, so the stability guarantee of
        // the FDN is preserved for every line count.
        let n = n.clamp(BLOCK_SIZE, MAX_LINES);

        let mut state = seed | 1;
        // --- Sign vectors -------------------------------------------
        let mut signs = [0.0; MAX_LINES];
        for slot in signs.iter_mut().take(n) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *slot = if (state & 1) == 0 { 1.0 } else { -1.0 };
        }

        // --- Fisher-Yates shuffle -----------------------------------
        let mut perm = [0usize; MAX_LINES];
        for (i, slot) in perm.iter_mut().take(n).enumerate() {
            *slot = i;
        }
        for i in (1..n).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let j = (state as usize) % (i + 1);
            perm.swap(i, j);
        }

        let rem = n % BLOCK_SIZE;
        Self {
            n,
            signs,
            perm,
            scratch: [0.0; MAX_LINES],
            inv_block: 1.0 / BLOCK_SIZE as f64,
            // Cached so `apply` avoids a division per sample.
            inv_rem: if rem > 0 { 1.0 / rem as f64 } else { 0.0 },
            rem,
        }
    }
}

impl FeedbackMatrix for VelvetMatrix {
    #[inline]
    fn size(&self) -> usize {
        self.n
    }

    fn apply(&mut self, x: &[f64], y: &mut [f64]) {
        let bs = BLOCK_SIZE;
        let num_blocks = self.n / bs;

        // Step 1: block-Householder into `scratch`.
        for b in 0..num_blocks {
            let base = b * bs;
            let mut dot = 0.0;
            for k in 0..bs {
                dot += self.signs[base + k] * x[base + k];
            }
            let scale = -2.0 * dot * self.inv_block;
            for k in 0..bs {
                self.scratch[base + k] = x[base + k] + scale * self.signs[base + k];
            }
        }

        // A final partial block gets its own (smaller) Householder
        // reflection. Each block acts on a disjoint set of indices, so
        // the block diagonal composition stays orthogonal for any `n`.
        if self.rem > 0 {
            let base = num_blocks * bs;
            let mut dot = 0.0;
            for k in 0..self.rem {
                dot += self.signs[base + k] * x[base + k];
            }
            let scale = -2.0 * dot * self.inv_rem;
            for k in 0..self.rem {
                self.scratch[base + k] = x[base + k] + scale * self.signs[base + k];
            }
        }

        // Step 2: apply the permutation on the way out.
        for (slot, &perm_i) in y[..self.n].iter_mut().zip(self.perm[..self.n].iter()) {
            *slot = self.scratch[perm_i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply_matrix_to_basis(m: &mut VelvetMatrix, col: usize) -> Vec<f64> {
        let n = m.size();
        let x: Vec<f64> = (0..n).map(|i| if i == col { 1.0 } else { 0.0 }).collect();
        let mut y = vec![0.0; n];
        m.apply(&x, &mut y);
        y
    }

    #[test]
    #[allow(clippy::needless_range_loop)]
    fn velvet_is_orthogonal() {
        let n = 8;
        let mut m = VelvetMatrix::new(n, 0xC0FFEE);

        // Build A explicitly and verify AᵀA = I.
        let mut a = vec![vec![0.0; n]; n];
        for col in 0..n {
            let y = apply_matrix_to_basis(&mut m, col);
            for row in 0..n {
                a[row][col] = y[row];
            }
        }
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

    /// The stability guarantee must hold for *every* line count,
    /// including the ones with a final partial block (n not a multiple
    /// of the 4-line block size).
    #[test]
    #[allow(clippy::needless_range_loop)]
    fn velvet_is_orthogonal_for_every_line_count() {
        for n in 4..=MAX_LINES {
            let mut m = VelvetMatrix::new(n, 0xC0FFEE);
            assert_eq!(m.size(), n, "matrix must not truncate to a multiple of 4");

            let mut a = vec![vec![0.0; n]; n];
            for col in 0..n {
                let y = apply_matrix_to_basis(&mut m, col);
                for row in 0..n {
                    a[row][col] = y[row];
                }
            }
            for i in 0..n {
                for j in 0..n {
                    let mut sum = 0.0;
                    for k in 0..n {
                        sum += a[k][i] * a[k][j];
                    }
                    let expected = if i == j { 1.0 } else { 0.0 };
                    assert!(
                        (sum - expected).abs() < 1e-12,
                        "n = {n}: AᵀA[{i},{j}] = {} (expected {expected})",
                        sum
                    );
                }
            }
        }
    }

    /// Regression: with a line count that is not a multiple of 4 the
    /// matrix previously truncated to the largest multiple of 4, so
    /// the trailing lines' feedback inputs were never written (they
    /// kept stale values and effectively dropped out of the network).
    #[test]
    fn apply_writes_every_output_slot() {
        for n in 5..=7 {
            let mut m = VelvetMatrix::new(n, 0xC0FFEE);
            let x: Vec<f64> = vec![1.0; n];
            let mut y = vec![f64::NAN; n];
            m.apply(&x, &mut y);
            assert!(
                y.iter().all(|v| v.is_finite()),
                "n = {n}: apply left output slots unwritten"
            );
        }
    }
}
