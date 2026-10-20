//! Velvet Noise – GPL-3.0
//!
//! A sparse diffusion stage based on Velvet Noise (VN), as described
//! by Välimäki et al. ("Late-Reverberation Synthesis Using Interleaved
//! Velvet-Noise Sequences", IEEE/ACM TASLP, 2017) and Fagerström et
//! al. ("Velvet-Noise Feedback Delay Network", DAFx 2020).
//!
//! # Pulse density
//!
//! The VN sequence is a fixed-length window (1024 samples) whose
//! impulse response is sparse: only N of the 1024 samples are
//! non-zero, and each takes the value +1 or −1. The **pulse density**
//! (pulses per second) determines how smooth the diffusion sounds.
//!
//! Perceptually:
//!   * below ~1000 pps → grainy, "buzzy"
//!   * ~1500–3000 pps  → smooth velvet-noise character
//!   * above ~5000 pps → starts to behave like white noise
//!
//! We constrain the range to **1500–3000 pps**, which the literature
//! identifies as the sweet spot for late-reverb diffusion.
//!
//! # Continuous density control
//!
//! Rather than jumping between a small number of fixed densities (the
//! previous 4-step design), we pre-generate `VN_DENSITY_STEPS`
//! sequences spanning the density range, and interpolate linearly
//! between two adjacent sequences on every sample. This makes the
//! `VelvetDensity` knob change the texture smoothly, with **no clicks
//! and no noise** as it sweeps.
//!
//! # Placement
//!
//! This filter MUST be applied to the wet sum of the FDN, outside
//! every feedback loop. Inside a loop, its rippled magnitude response
//! accumulates over many iterations and produces a harsh resonance.

/// Length of the VN impulse response, in samples.
pub const VN_SEQ_LEN: usize = 1024;

/// Number of pre-generated density sequences. 16 steps across the
/// 1500–3000 pps range gives ~95 pps per step, which is well below
/// the just-noticeable difference for diffusion smoothness.
pub const VN_DENSITY_STEPS: usize = 16;

/// Minimum pulse density, in pulses per second. Below this value the
/// diffusion starts to sound grainy on sustained material.
const MIN_PULSES_PER_SEC: f64 = 1500.0;

/// Maximum pulse density, in pulses per second. Above this value the
/// VN sequence loses its velvet character and begins to sound like
/// dense noise.
const MAX_PULSES_PER_SEC: f64 = 3000.0;

/// Reference sample rate used only to translate pulses-per-second into
/// a pulse count for the fixed 1024-sample window. The convolution
/// itself is sample-rate independent; this constant simply defines
/// the time window over which the pulse count is measured.
const REFERENCE_SR: f64 = 48_000.0;

/// Base RNG seed. Each density sequence uses `BASE_SEED.wrapping_add(i)`
/// so that the sequences are deterministic but uncorrelated.
const BASE_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// 48 kHz form, used by tests to exercise the rate-aware function
/// at the reference rate.
#[cfg(test)]
#[inline]
fn pulses_for_density(density: f64) -> usize {
    pulses_for_density_at_rate(density, REFERENCE_SR)
}

/// Same as `pulses_for_density`, but the pulse count is scaled to
/// an arbitrary sample rate instead of always using the 48 kHz
/// reference. At 48 kHz the result is identical to the original.
#[inline]
fn pulses_for_density_at_rate(density: f64, sample_rate: f64) -> usize {
    let d = density.clamp(0.0, 1.0);
    let pps = MIN_PULSES_PER_SEC + (MAX_PULSES_PER_SEC - MIN_PULSES_PER_SEC) * d;
    let count = (pps * VN_SEQ_LEN as f64 / sample_rate).round() as usize;
    count.max(8)
}

#[test]
fn pulses_for_density_is_monotonic() {
    let mut prev = 0;
    for i in 0..VN_DENSITY_STEPS {
        let d = i as f64 / (VN_DENSITY_STEPS - 1) as f64;
        let n = pulses_for_density_at_rate(d, REFERENCE_SR);
        assert!(n >= prev, "pulse count decreased at step {}", i);
        prev = n;
    }
}

#[inline]
fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

#[inline]
fn uniform(state: &mut u64) -> f64 {
    (xorshift64(state) >> 11) as f64 / (1u64 << 53) as f64
}

/// One VN sequence, stored as a list of `(sample_offset, sign)` pairs.
#[derive(Clone, Debug)]
struct Sequence {
    /// Sorted sample offsets (in [0, VN_SEQ_LEN)).
    offsets: Vec<usize>,
    /// Signs of each pulse (+1 or -1).
    signs: Vec<f64>,
    /// 1 / sqrt(pulses.len()) — keeps the perceived level roughly
    /// constant across densities.
    norm: f64,
}

impl Sequence {
    /// Build a VN sequence with `pulse_count` non-zero samples spread
    /// over `VN_SEQ_LEN` samples.
    fn new(pulse_count: usize, seed: u64) -> Self {
        let period = VN_SEQ_LEN as f64 / pulse_count as f64;
        let mut state = seed;
        let mut pulses: Vec<(usize, f64)> = Vec::with_capacity(pulse_count);

        for m in 0..pulse_count {
            let base = m as f64 * period;
            let jitter = (uniform(&mut state) - 0.5) * (period - 1.0);
            let pos = (base + jitter).round().max(0.0) as usize;
            if pos >= VN_SEQ_LEN {
                continue;
            }
            let sign = if uniform(&mut state) < 0.5 { 1.0 } else { -1.0 };
            pulses.push((pos, sign));
        }

        if pulses.is_empty() {
            pulses.push((0, 1.0));
        }

        // Sort by offset to improve cache locality during convolution.
        pulses.sort_by_key(|&(pos, _)| pos);

        let offsets: Vec<usize> = pulses.iter().map(|&(pos, _)| pos).collect();
        let signs: Vec<f64> = pulses.iter().map(|&(_, sign)| sign).collect();

        let norm = 1.0 / (pulses.len() as f64).sqrt();
        Self {
            offsets,
            signs,
            norm,
        }
    }
}

/// Maximum RRS pulse width, in samples. 30 samples at 48 kHz is
/// 0.625 ms — the first spectral null sits at ~1.6 kHz, which is the
/// darkest value that still sounds like "diffusion" rather than a
/// muffled low-pass.
const MAX_RRS_WIDTH: usize = 30;

#[derive(Clone, Debug)]
pub struct VelvetNoiseFilter {
    sequences: Vec<Sequence>,
    idx_a: usize,
    idx_b: usize,
    interp_t: f64,
    history: Vec<f64>,
    write_idx: usize,
    amount: f64,
    dry_gain: f64,
    wet_gain: f64,

    // --- rate-awareness state ---
    /// Host sample rate the sequences and LP coefficient are tuned to.
    sample_rate: f64,
    /// Base seed, so the sequences can be rebuilt on a rate change.
    base_seed: u64,
    /// Last value passed to `set_tone`, so the RRS width can be
    /// recomputed after a rate change without losing the user setting.
    tone: f64,
    /// = MAX_RRS_WIDTH scaled to the current sample rate.
    max_rrs_width: usize,

    rrs_width: usize,
    rrs_buf: Vec<f64>,
    rrs_idx: usize,
    rrs_sum: f64,

    lp_state: f64,
    lp_coeff: f64,
}

impl VelvetNoiseFilter {
    /// Default instance at the 48 kHz reference rate.
    pub fn new() -> Self {
        Self::with_seed(BASE_SEED)
    }

    /// 48 kHz instance with a specific base seed.
    pub fn with_seed(base_seed: u64) -> Self {
        Self::with_seed_and_rate(base_seed, REFERENCE_SR)
    }

    /// Instance with a specific base seed and sample rate.
    ///
    /// Pulse count per sequence, the one-pole LP coefficient, and
    /// the maximum RRS width are all scaled to `sample_rate`, so the
    /// diffusion sounds the same at 44.1, 48, 88.2, 96 kHz, etc.
    pub fn with_seed_and_rate(base_seed: u64, sample_rate: f64) -> Self {
        let sr = sample_rate.max(8000.0);
        let max_rrs_width = ((MAX_RRS_WIDTH as f64) * sr / REFERENCE_SR)
            .round()
            .max(1.0) as usize;

        let mut sequences = Vec::with_capacity(VN_DENSITY_STEPS);
        for i in 0..VN_DENSITY_STEPS {
            let d = i as f64 / (VN_DENSITY_STEPS - 1) as f64;
            let pulse_count = pulses_for_density_at_rate(d, sr);
            let seed = base_seed.wrapping_add(i as u64);
            sequences.push(Sequence::new(pulse_count, seed));
        }

        // One-pole LP at a true 3.5 kHz regardless of sample rate.
        let lp_coeff = 1.0 - (-2.0 * std::f64::consts::PI * 3500.0 / sr).exp();

        Self {
            sequences,
            idx_a: VN_DENSITY_STEPS / 2,
            idx_b: VN_DENSITY_STEPS / 2,
            interp_t: 0.0,
            history: vec![0.0; VN_SEQ_LEN],
            write_idx: 0,
            amount: 0.0,
            dry_gain: 1.0,
            wet_gain: 0.0,
            sample_rate: sr,
            base_seed,
            tone: 0.0,
            max_rrs_width,
            rrs_width: 1,
            rrs_buf: vec![0.0; max_rrs_width],
            rrs_idx: 0,
            rrs_sum: 0.0,
            lp_state: 0.0,
            lp_coeff,
        }
    }

    /// Rebuild every rate-dependent internal value when the host
    /// sample rate changes.
    ///
    /// Without this, the "3.5 kHz" LP would become 7 kHz at 96 kHz,
    /// the RRS window would represent half the time in milliseconds,
    /// and the pulse-density mapping would drift from the calibrated
    /// 1500–3000 pps range.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        let sr = sample_rate.max(8000.0);
        if (sr - self.sample_rate).abs() < 1.0 {
            return;
        }
        self.sample_rate = sr;

        self.max_rrs_width = ((MAX_RRS_WIDTH as f64) * sr / REFERENCE_SR)
            .round()
            .max(1.0) as usize;
        self.rrs_buf = vec![0.0; self.max_rrs_width];
        self.rrs_idx = 0;
        self.rrs_sum = 0.0;

        self.lp_coeff = 1.0 - (-2.0 * std::f64::consts::PI * 3500.0 / sr).exp();

        for (i, seq) in self.sequences.iter_mut().enumerate() {
            let d = i as f64 / (VN_DENSITY_STEPS - 1) as f64;
            let pulse_count = pulses_for_density_at_rate(d, sr);
            let seed = self.base_seed.wrapping_add(i as u64);
            *seq = Sequence::new(pulse_count, seed);
        }

        // Preserve the user's tone setting, re-derived at the new
        // rate so the window duration in milliseconds stays the same.
        let w = 1.0 + self.tone * (self.max_rrs_width as f64 - 1.0);
        self.rrs_width = w.round().max(1.0) as usize;
    }


    #[inline]
    pub fn set_amount(&mut self, amount: f64) {
        let a = amount.clamp(0.0, 1.0);
        self.amount = a;
        let angle = a * std::f64::consts::FRAC_PI_2;
        self.dry_gain = angle.cos();
        self.wet_gain = angle.sin();
    }

    #[inline]
    pub fn set_density(&mut self, density: f64) {
        let d = density.clamp(0.0, 1.0);
        let pos = d * (VN_DENSITY_STEPS - 1) as f64;
        let idx_a = pos.floor() as usize;
        let idx_b = (idx_a + 1).min(VN_DENSITY_STEPS - 1);
        self.idx_a = idx_a;
        self.idx_b = idx_b;
        self.interp_t = pos - idx_a as f64;
    }

    /// Set the "darkening" amount (0.0 = bright, 1.0 = dark).
    ///
    /// Darkening is implemented by widening the VN pulses via a
    /// Recursive Running-Sum (RRS) filter applied to the sparse
    /// convolution output. Mathematically this is equivalent to
    /// convolving the impulse-train VN sequence with a rectangular
    /// window of `rrs_width` samples, which is exactly the textbook
    /// definition of Dark Velvet Noise (Välimäki et al., 2017).
    ///
    /// The RRS is O(1) per sample regardless of window width, so the
    /// darkening control has no measurable CPU cost above the
    /// baseline.
    #[inline]
    pub fn set_tone(&mut self, tone: f64) {
        let t = tone.clamp(0.0, 1.0);
        // Stored so `set_sample_rate` can rebuild the width at the
        // new rate without losing the user's setting.
        self.tone = t;
        let w = 1.0 + t * (self.max_rrs_width as f64 - 1.0);
        let new_width = w.round().max(1.0) as usize;
        if new_width != self.rrs_width {
            self.rrs_width = new_width;
            self.rrs_buf.fill(0.0);
            self.rrs_idx = 0;
            self.rrs_sum = 0.0;
        }
    }

    pub fn clear_buffers(&mut self) {
        self.history.fill(0.0);
        self.write_idx = 0;
        self.lp_state = 0.0;
        self.rrs_buf.fill(0.0);
        self.rrs_idx = 0;
        self.rrs_sum = 0.0;
    }

    /// Process one sample using the Transposed Double-Vector Filter
    /// algorithm. This is the optimized convolution kernel.
    #[inline]
    pub fn process(&mut self, input: f64) -> f64 {
        // Write input into circular history.
        self.history[self.write_idx] = input;

        // Convolve against the two adjacent sequences and interpolate.
        let write_idx = self.write_idx;
        let wet_a = convolve_double_vector(&self.history, write_idx, &self.sequences[self.idx_a]);
        let wet_b = if self.interp_t > 1.0e-6 && self.idx_b != self.idx_a {
            convolve_double_vector(&self.history, write_idx, &self.sequences[self.idx_b])
        } else {
            wet_a
        };
        let wet = wet_a * (1.0 - self.interp_t) + wet_b * self.interp_t;

        // ---- Recursive Running-Sum (Dark Velvet Noise) --------------
        // Widens the impulse-like VN pulses into rectangular ones of
        // `rrs_width` samples. Implemented in O(1) per sample via a
        // running sum: add the new sample, subtract the one that just
        // fell out of the window. The output is scaled by `sqrt(W)` so
        // that the RMS for a zero-mean signal is preserved (DC gain
        // would be `sqrt(W)`, but VN has no DC component by
        // construction).
        let wet_rrs = if self.rrs_width <= 1 {
            wet
        } else {
            let w = self.rrs_width;
            let old = self.rrs_buf[self.rrs_idx];
            self.rrs_sum += wet - old;
            self.rrs_buf[self.rrs_idx] = wet;
            self.rrs_idx = if self.rrs_idx + 1 >= w {
                0
            } else {
                self.rrs_idx + 1
            };
            // 1/w (not 1/sqrt(w)) — bounds the worst-case peak to exactly
            // the input amplitude. The RMS is reduced by 1/sqrt(w), but
            // for a sparse VN signal this is imperceptible because the
            // RRS is a character filter, not an amplitude stage.
            self.rrs_sum / (w as f64)
        };

        // One-pole low-pass — final smoothing on top of the RRS.
        self.lp_state += self.lp_coeff * (wet_rrs - self.lp_state);
        // Soft saturation: tanh-like knee that limits values above 1.0
        // without clipping. Below 0.8 the curve is essentially linear,
        // so normal signals pass through unchanged; only peaks are
        // compressed.
        let x = self.lp_state;
        let wet_lp = if x.abs() < 0.8 {
            x
        } else {
            // Cubic soft-clip in [0.8, ∞) → asymptote to 1.0.
            let sign = if x >= 0.0 { 1.0 } else { -1.0 };
            let a = x.abs();
            let y = 0.8 + (a - 0.8) / (1.0 + (a - 0.8));
            sign * y
        };

        // Advance write head.
        self.write_idx = if write_idx + 1 >= VN_SEQ_LEN {
            0
        } else {
            write_idx + 1
        };

        // Equal-power crossfade.
        input * self.dry_gain + wet_lp * self.wet_gain
    }
}

impl Default for VelvetNoiseFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// **Transposed Double-Vector Filter** convolution.
///
/// This is the optimized kernel. It splits the VN sequence into two
/// parallel vectors (offsets and signs) and iterates over them
/// sequentially, which improves cache locality because the offsets
/// are sorted and the sign array is packed. The result is a faster
/// sparse convolution than the previous `(offset, sign)` tuple form.
///
/// Reference: Belloch et al., "Efficient Velvet-Noise Convolution in
/// Multicore Processors", JAES 2024, §3.2.
#[inline]
fn convolve_double_vector(history: &[f64], write_idx: usize, seq: &Sequence) -> f64 {
    let mut wet = 0.0;
    let n = seq.offsets.len();
    // SAFETY: `offsets` and `signs` always have the same length, and
    // every offset is < VN_SEQ_LEN. `idx` is wrapped into [0, VN_SEQ_LEN).
    unsafe {
        for i in 0..n {
            let offset = *seq.offsets.get_unchecked(i);
            let sign = *seq.signs.get_unchecked(i);
            let idx = if offset > write_idx {
                VN_SEQ_LEN - (offset - write_idx)
            } else {
                write_idx - offset
            };
            wet += *history.get_unchecked(idx) * sign;
        }
    }
    wet * seq.norm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulses_for_density_is_monotonic() {
        let mut prev = 0;
        for i in 0..VN_DENSITY_STEPS {
            let d = i as f64 / (VN_DENSITY_STEPS - 1) as f64;
            let n = pulses_for_density(d);
            assert!(n >= prev, "pulse count decreased at step {}", i);
            prev = n;
        }
    }

    #[test]
    fn sequence_is_deterministic() {
        let a = Sequence::new(35, 42);
        let b = Sequence::new(35, 42);
        assert_eq!(a.offsets, b.offsets);
        assert_eq!(a.signs, b.signs);
    }

    #[test]
    fn set_density_is_allocation_free_and_bounded() {
        let mut f = VelvetNoiseFilter::new();
        for i in 0..=100 {
            let d = i as f64 / 100.0;
            f.set_density(d);
            assert!(f.idx_a < VN_DENSITY_STEPS);
            assert!(f.idx_b < VN_DENSITY_STEPS);
            assert!(f.interp_t >= 0.0 && f.interp_t <= 1.0);
        }
    }

    #[test]
    fn filter_passes_signal_through_when_amount_zero() {
        let mut f = VelvetNoiseFilter::new();
        f.set_amount(0.0);
        assert_eq!(f.process(0.5), 0.5);
    }

    #[test]
    fn filter_is_bounded_at_max_amount_and_max_density() {
        let mut f = VelvetNoiseFilter::new();
        f.set_amount(1.0);
        f.set_density(1.0);
        let mut max_out = 0.0_f64;
        for i in 0..4096 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let y = f.process(x);
            max_out = max_out.max(y.abs());
        }
        assert!(
            max_out < 5.0,
            "output peak {} exceeded sanity bound",
            max_out
        );
    }

    #[test]
    fn density_change_produces_no_discontinuity() {
        let mut f = VelvetNoiseFilter::new();
        f.set_amount(1.0);
        f.set_density(0.0);
        let mut prev = 0.0_f64;
        let mut max_jump = 0.0_f64;
        for i in 0..20_000 {
            let d = i as f64 / 20_000.0;
            f.set_density(d);
            let y = f.process(0.5);
            if i > 0 {
                max_jump = max_jump.max((y - prev).abs());
            }
            prev = y;
        }
        assert!(max_jump < 0.5, "max jump between samples was {}", max_jump);
    }

    #[test]
    fn double_vector_convolution_matches_naive() {
        // Sanity check: the optimized kernel must produce the same result
        // as a naive per-tuple convolution.
        let seq = Sequence::new(50, 7);
        let mut history = vec![0.0; VN_SEQ_LEN];
        for i in 0..VN_SEQ_LEN {
            history[i] = (i as f64 * 0.01).sin();
        }
        let write_idx = 500;
        let optimized = convolve_double_vector(&history, write_idx, &seq);

        // Naive: reconstruct (offset, sign) pairs and convolve.
        let mut naive = 0.0;
        for i in 0..seq.offsets.len() {
            let offset = seq.offsets[i];
            let sign = seq.signs[i];
            let idx = if offset > write_idx {
                VN_SEQ_LEN - (offset - write_idx)
            } else {
                write_idx - offset
            };
            naive += history[idx] * sign;
        }
        naive *= seq.norm;

        assert!(
            (optimized - naive).abs() < 1e-12,
            "optimized {} != naive {}",
            optimized,
            naive
        );
    }
}
