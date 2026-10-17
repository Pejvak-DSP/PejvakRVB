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

    pub fn clear_buffers(&mut self) {
        self.delay_buffer.fill(0.0);
        self.output.fill(0.0);
    }

    fn update(&mut self) {
        self.mod_phase += self.mod_rate * MODULATION_UPDATE_RATE as f64;
        // Same floor() wrap as ModulatedAllpass / C++.
        self.mod_phase -= self.mod_phase.floor();
        let m = fast_sin::get(self.mod_phase);
        let total_delay = self.sample_delay as f64 + self.mod_amount * m;
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
/// twice per sample in `update()`. Callers guarantee `sub < len`.
#[inline(always)]
fn wrap_sub(base: usize, sub: usize, len: usize) -> usize {
    debug_assert!(sub < len, "wrap_sub: sub must be < len");
    if base >= sub {
        base - sub
    } else {
        base + len - sub
    }
}
