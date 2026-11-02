//! ModulatedDelay – GPL-3.0
//!
//! Faithful Rust port of `CloudSeed.Native/ModulatedDelay.h` (legacy-v1).
//! All buffers and arithmetic use `f64`. SIMD removed: the original is
//! scalar, so vectorising changes the summation order.

use crate::cloudseed::fast_sin;
use crate::cloudseed::modulated_allpass::{msvc_rand_nth, LFO_INSTANCE_COUNTER};
use std::sync::atomic::Ordering;

/// Modulation re-quantisation rate (matches C++).
pub const MODULATION_UPDATE_RATE: usize = 8;

#[derive(Clone, Debug)]
pub struct ModulatedDelay {
    delay_buffer: Vec<f64>,
    output: Vec<f64>,
    buffer_size: usize,
    write_index: usize,
    read_index_a: usize,
    read_index_b: usize,
    samples_processed: usize,
    delay_buffer_size_samples: usize,
    mod_phase: f64,
    gain_a: f64,
    gain_b: f64,
    pub sample_delay: usize,
    pub mod_amount: f64,
    pub mod_rate: f64,
}

impl ModulatedDelay {
    pub fn new(buffer_size: usize, delay_buffer_size_samples: usize, sample_delay: usize) -> Self {
        // Defensive: a buffer length of 0 or 1 would make the
        // two-tap interpolation read out of bounds and trip
        // `wrap_sub`'s debug assertion. Every call site already
        // passes a sane size (>= 2), but a future caller that
        // forgets would produce undefined behaviour inside the
        // `unsafe` block in `process()`.
        let delay_buffer_size_samples = delay_buffer_size_samples.max(2);

        // Same scheme as ModulatedAllpass: a per-instance id from the
        // shared counter, hashed through the MSVC rand() walk.
        let instance_id = LFO_INSTANCE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut d = Self {
            delay_buffer: vec![0.0; delay_buffer_size_samples],
            output: vec![0.0; buffer_size],
            buffer_size,
            write_index: 0,
            read_index_a: 0,
            read_index_b: 0,
            samples_processed: 0,
            delay_buffer_size_samples,
            mod_phase: 0.01 + 0.98 * msvc_rand_nth(instance_id),
            gain_a: 0.0,
            gain_b: 0.0,
            sample_delay,
            mod_amount: 0.0,
            mod_rate: 0.0,
        };
        d.update();
        d
    }

    pub fn process(&mut self, input: &[f64]) {
        let n = input.len().min(self.buffer_size);
        let buf_len = self.delay_buffer_size_samples;
        debug_assert_eq!(self.delay_buffer.len(), buf_len);
        debug_assert!(n <= input.len());
        debug_assert!(n <= self.output.len());

        // SAFETY: indices (`write_index`, `read_index_a`, `read_index_b`)
        // are kept in [0, buf_len) by the explicit wraps in this loop and
        // by `update()`. `input` and `output` have length >= n.
        unsafe {
            let input_ptr = input.as_ptr();
            let output_ptr = self.output.as_mut_ptr();
            let buf_ptr = self.delay_buffer.as_mut_ptr();

            for i in 0..n {
                if self.samples_processed == MODULATION_UPDATE_RATE {
                    self.update();
                }
                *buf_ptr.add(self.write_index) = *input_ptr.add(i);
                *output_ptr.add(i) = *buf_ptr.add(self.read_index_a) * self.gain_a
                    + *buf_ptr.add(self.read_index_b) * self.gain_b;

                self.write_index += 1;
                self.read_index_a += 1;
                self.read_index_b += 1;
                if self.write_index >= buf_len {
                    self.write_index -= buf_len;
                }
                if self.read_index_a >= buf_len {
                    self.read_index_a -= buf_len;
                }
                if self.read_index_b >= buf_len {
                    self.read_index_b -= buf_len;
                }
                self.samples_processed += 1;
            }
        }
    }

    pub fn get_output(&self) -> &[f64] {
        &self.output
    }

    /// Reallocates the circular delay buffer to `n` samples. Used when
    /// the sample rate changes: several delay buffers are sized from
    /// the rate (e.g. the 1 s pre-delay headroom), so the new rate's
    /// maximum delay only fits after a reallocation.
    ///
    /// # Real-time safety
    ///
    /// Not real-time safe (heap reallocation). Call this from a
    /// control path, never from the audio thread.
    pub fn set_delay_buffer_samples(&mut self, n: usize) {
        let n = n.max(2);
        if n == self.delay_buffer_size_samples {
            return;
        }
        self.delay_buffer = vec![0.0; n];
        self.delay_buffer_size_samples = n;
        self.write_index = 0;
        self.read_index_a = 0;
        self.read_index_b = 0;
        self.update();
    }

    pub fn clear_buffers(&mut self) {
        self.delay_buffer.fill(0.0);
        self.output.fill(0.0);
    }

    fn update(&mut self) {
        self.mod_phase += self.mod_rate * MODULATION_UPDATE_RATE as f64;
        // Same floor() wrap as ModulatedAllpass / C++.
        self.mod_phase -= self.mod_phase.floor();
        let m = fast_sin::get(self.mod_phase);
        let mut total_delay = self.sample_delay as f64 + self.mod_amount * m;
        // Safety clamp: the two interpolation taps read at
        // `delay_a` and `delay_a + 1`, so the delay must stay at or
        // below `len - 2`. For every legitimate configuration this
        // is a no-op; it only engages when a delay was set past the
        // buffer capacity — e.g. PreDelay at its 1000 ms maximum,
        // where the C++ original silently collapsed the delay to
        // zero and the read index wrapped out of bounds.
        let max_delay = self.delay_buffer_size_samples.saturating_sub(2) as f64;
        if total_delay > max_delay {
            total_delay = max_delay;
        }
        let delay_a = total_delay as usize;
        let delay_b = delay_a + 1;
        let partial = total_delay - delay_a as f64;
        self.gain_a = 1.0 - partial;
        self.gain_b = partial;
        self.read_index_a = wrap_sub(self.write_index, delay_a, self.delay_buffer_size_samples);
        self.read_index_b = wrap_sub(self.write_index, delay_b, self.delay_buffer_size_samples);
        self.samples_processed = 0;
    }
}

/// `base - sub` with wraparound into `[0, len)`.
///
/// Uses the C++ conditional-add idiom instead of `%` — the modulo
/// operator compiles to `div` on x86 (~20–40 cycles) and is called
/// twice per sample in `update()`. `update()` clamps the delay to
/// `len - 2`, so `sub < len` always holds.
#[inline(always)]
fn wrap_sub(base: usize, sub: usize, len: usize) -> usize {
    debug_assert!(sub < len, "wrap_sub: sub must be < len");
    if base >= sub {
        base - sub
    } else {
        base + len - sub
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: a `sample_delay` equal to the buffer length (the
    /// pre-delay maximum used to produce exactly this) must clamp to
    /// the usable range instead of tripping `wrap_sub`'s debug
    /// assertion or wrapping a read index out of bounds.
    #[test]
    fn max_sample_delay_stays_in_bounds() {
        let len = 480;
        let mut d = ModulatedDelay::new(64, len, 100);
        d.sample_delay = len; // == buffer length: the old off-by-one
        d.mod_amount = 0.0;
        let input: Vec<f64> = (0..64).map(|i| (i as f64) * 0.01).collect();
        d.process(&input);
        assert!(d.get_output().iter().all(|v| v.is_finite()));
    }

    /// Regression: a delay at the maximum usable range must round-trip
    /// the impulse after exactly `sample_delay` samples.
    #[test]
    fn max_usable_delay_round_trips_impulse() {
        // Sized like the plugin's pre-delay: 1 s at 48 kHz (+2 headroom).
        let mut d = ModulatedDelay::new(512, 482, 480);
        let mut input = vec![0.0; 512];
        input[0] = 1.0;
        d.process(&input);
        let out = d.get_output();
        assert!((out[480] - 1.0).abs() < 1e-9, "out[480] = {}", out[480]);
    }

    /// A `mod_amount` large enough to push the total delay past both
    /// ends of the buffer must stay clamped and finite (no wrap, no
    /// out-of-bounds read) at every modulation phase.
    #[test]
    fn oversized_modulation_stays_in_bounds() {
        let len = 480;
        let mut d = ModulatedDelay::new(64, len, len - 2);
        d.mod_amount = 1000.0;
        d.mod_rate = 0.05;
        let input: Vec<f64> = (0..64).map(|i| (i as f64) * 0.01).collect();
        // Several blocks so `update()` runs at many modulation phases.
        for _ in 0..32 {
            d.process(&input);
            assert!(d.get_output().iter().all(|v| v.is_finite()));
        }
    }
}
