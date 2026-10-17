//! ModulatedAllpass – GPL-3.0
//!
//! Faithful Rust port of `CloudSeed.Native/ModulatedAllpass.h` (legacy-v1).
//!
//! Every buffer and every arithmetic operation uses `f64` (matching the
//! C++ `double` type). The previous version used `f32` for CPU savings,
//! but the accumulation error across the 12-line feedback loop is
//! audible — the tail decays ~40% faster than CloudReverb's. This port
//! prioritises bit-level fidelity over speed.
//!
//! SIMD has been removed for the same reason: the original is scalar,
//! so four-lane vectorisation changes the summation order and therefore
//! the rounding.

use crate::cloudseed::fast_sin;
use std::sync::atomic::{AtomicU32, Ordering};

/// Global instance counter shared by `ModulatedAllpass` and `ModulatedDelay`.
/// C++ uses the process-global `std::rand()` sequence to give every filter
/// a distinct LFO starting phase. To match that behaviour without a global
/// RNG mutex, we hand each new instance a unique integer id and hash it
/// through the MSVC `rand()` walk. The counter is shared across both types
/// so no two instances ever get the same phase.
pub static LFO_INSTANCE_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Fixed all-pass delay buffer size in samples (100 ms at 192 kHz – matches
/// the original `DelayBufferSamples = 19200`).
pub const DELAY_BUFFER_SAMPLES: usize = 19200;

/// Modulation re-quantisation rate (matches `ModulationUpdateRate = 8`).
pub const MODULATION_UPDATE_RATE: usize = 8;

#[derive(Clone, Debug)]
pub struct ModulatedAllpass {
    delay_buffer: Vec<f64>,
    output: Vec<f64>,
    buffer_size: usize,
    index: usize,
    samples_processed: usize,
    mod_phase: f64,
    delay_a: usize,
    delay_b: usize,
    gain_a: f64,
    gain_b: f64,
    pub sample_delay: usize,
    pub feedback: f64,
    pub mod_amount: f64,
    pub mod_rate: f64,
    pub interpolation_enabled: bool,
    pub modulation_enabled: bool,
}

impl ModulatedAllpass {
    pub fn new(buffer_size: usize, sample_delay: usize) -> Self {
        let instance_id = LFO_INSTANCE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut a = Self {
            delay_buffer: vec![0.0; DELAY_BUFFER_SAMPLES],
            output: vec![0.0; buffer_size],
            buffer_size,
            index: DELAY_BUFFER_SAMPLES - 1,
            samples_processed: 0,
            mod_phase: 0.01 + 0.98 * msvc_rand_nth(instance_id),
            delay_a: 0,
            delay_b: 0,
            gain_a: 0.0,
            gain_b: 0.0,
            sample_delay,
            feedback: 0.0,
            mod_amount: 0.0,
            mod_rate: 0.0,
            interpolation_enabled: true,
            modulation_enabled: false,
        };
        a.update();
        a
    }

    pub fn get_output(&self) -> &[f64] {
        &self.output
    }

    pub fn clear_buffers(&mut self) {
        self.delay_buffer.fill(0.0);
        self.output.fill(0.0);
    }

    /// Processes one block into the internal output buffer.
    pub fn process(&mut self, input: &[f64]) {
        let n = input.len().min(self.buffer_size);
        if self.modulation_enabled {
            self.process_with_mod(input, n);
        } else {
            self.process_no_mod(input, n);
        }
    }

    fn process_no_mod(&mut self, input: &[f64], n: usize) {
        let buf_len = DELAY_BUFFER_SAMPLES;
        // Assertions for unsafe blocks — verify at runtime in debug only.
        debug_assert!(n <= input.len());
        debug_assert!(n <= self.output.len());
        debug_assert_eq!(self.delay_buffer.len(), buf_len);

        let sample_delay_i64 = self.sample_delay as i64;
        let buf_len_i64 = buf_len as i64;
        let fb = self.feedback;

        // SAFETY (whole loop): `input`, `output`, and `delay_buffer` all
        // have length >= the maximum index used. `self.index` and
        // `delayed_index` are kept in [0, buf_len) by the explicit wraps.
        unsafe {
            let input_ptr = input.as_ptr();
            let output_ptr = self.output.as_mut_ptr();
            let buf_ptr = self.delay_buffer.as_mut_ptr();

            for i in 0..n {
                let mut delayed_index = self.index as i64 - sample_delay_i64;
                if delayed_index < 0 {
                    delayed_index += buf_len_i64;
                }
                let buf_out = *buf_ptr.add(delayed_index as usize);
                let in_val = *input_ptr.add(i) + buf_out * fb;
                *buf_ptr.add(self.index) = in_val;
                *output_ptr.add(i) = buf_out - in_val * fb;

                self.index += 1;
                if self.index >= buf_len {
                    self.index -= buf_len;
                }
                self.samples_processed += 1;
            }
        }
    }

    fn process_with_mod(&mut self, input: &[f64], n: usize) {
        let buf_len = DELAY_BUFFER_SAMPLES;
        debug_assert!(n <= input.len());
        debug_assert!(n <= self.output.len());
        debug_assert_eq!(self.delay_buffer.len(), buf_len);

        // SAFETY (whole loop): same as `process_no_mod`. `delay_a`,
        // `delay_b` are always < buf_len (see `update()`); `index` is
        // kept in [0, buf_len). `input` and `output` have length >= n.
        unsafe {
            let input_ptr = input.as_ptr();
            let output_ptr = self.output.as_mut_ptr();
            let buf_ptr = self.delay_buffer.as_mut_ptr();

            for i in 0..n {
                if self.samples_processed >= MODULATION_UPDATE_RATE {
                    self.update();
                }
                let buf_out = if self.interpolation_enabled {
                    let ia = wrap_sub(self.index, self.delay_a, buf_len);
                    let ib = wrap_sub(self.index, self.delay_b, buf_len);
                    *buf_ptr.add(ia) * self.gain_a + *buf_ptr.add(ib) * self.gain_b
                } else {
                    let ia = wrap_sub(self.index, self.delay_a, buf_len);
                    *buf_ptr.add(ia)
                };
                let in_val = *input_ptr.add(i) + buf_out * self.feedback;
                *buf_ptr.add(self.index) = in_val;
                *output_ptr.add(i) = buf_out - in_val * self.feedback;
                self.index += 1;
                if self.index >= buf_len {
                    self.index -= buf_len;
                }
                self.samples_processed += 1;
            }
        }
    }

    fn update(&mut self) {
        self.mod_phase += self.mod_rate * MODULATION_UPDATE_RATE as f64;
        // Match C++ exactly: subtract the floor so the phase wraps into
        // [0, 1) whether it crossed 1.0 or (theoretically) dipped below 0.
        self.mod_phase -= self.mod_phase.floor();
        let m = fast_sin::get(self.mod_phase);
        // Match C++: the clamp writes back into the member so it persists
        // across `update()` calls, not just for this one frame.
        if self.mod_amount >= self.sample_delay as f64 {
            self.mod_amount = self.sample_delay as f64 - 1.0;
        }
        let mut total_delay = self.sample_delay as f64 + self.mod_amount * m;
        if total_delay <= 0.0 {
            // should no longer be required (kept for parity with the original)
            total_delay = 1.0;
        }
        self.delay_a = total_delay as usize;
        self.delay_b = self.delay_a + 1;
        let partial = total_delay - self.delay_a as f64;
        self.gain_a = 1.0 - partial;
        self.gain_b = partial;
        self.samples_processed = 0;
    }
}

/// `base - sub` with wraparound into `[0, len)`.
///
/// Matches the C++ idiom `idx += len * (idx < 0)` — uses a conditional
/// add instead of `%`, which is the slowest ALU op on x86 (~20–40 cycles
/// per `div`). Callers must guarantee `sub < len`, which holds for every
/// caller in this module because `delay_a`/`delay_b` are always derived
/// from `sample_delay + mod_amount * m` and `sample_delay` is bounded by
/// the delay buffer size (see `update()`).
#[inline(always)]
fn wrap_sub(base: usize, sub: usize, len: usize) -> usize {
    debug_assert!(sub < len, "wrap_sub: sub must be < len");
    if base >= sub {
        base - sub
    } else {
        base + len - sub
    }
}

/// MSVC CRT `rand()` sequence walk. Used only for deterministic LFO
/// initial phases. Kept here (rather than in a shared module) because
/// `ModulatedDelay` also needs it but a shared module would create a
/// circular reference through `mod.rs`.
const RAND_SEED: u32 = 1;

pub fn msvc_rand_nth(n: u32) -> f64 {
    let mut s = RAND_SEED;
    let mut out = 0.0;
    for _ in 0..=n {
        s = s.wrapping_mul(214_013).wrapping_add(2_531_011);
        out = ((s >> 16) & 0x7fff) as f64 / 32767.0;
    }
    out
}
