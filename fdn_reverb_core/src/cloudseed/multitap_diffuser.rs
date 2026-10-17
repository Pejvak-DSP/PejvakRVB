//! MultitapDiffuser – GPL-3.0
//!
//! Faithful Rust port of `CloudSeed.Native/MultitapDiffuser.h` (legacy-v1):
//! a delay line tapped at `count` positions with seed-decorrelated spacing
//! and exponential tap decay.
//!
//! All state uses fixed-size arrays, so updating taps or seeds never
//! allocates on the audio thread.

use crate::cloudseed::sha_random;

/// Maximum number of taps (matches `MaxTaps`).
pub const MAX_TAPS: usize = 50;

/// Number of seed values requested from `ShaRandom` (matches the original).
const SEED_VALUE_COUNT: usize = 100;

#[derive(Clone, Debug)]
pub struct MultitapDiffuser {
    buffer: Vec<f64>,
    output: Vec<f64>,
    len: usize,
    index: i64,
    tap_gains: [f64; MAX_TAPS],
    tap_positions: [usize; MAX_TAPS],
    tap_data: [f64; MAX_TAPS],
    seed_values: [f64; SEED_VALUE_COUNT],
    seed: i64,
    cross_seed: f64,
    count: usize,
    length: f64,
    gain: f64,
    decay: f64,
}

impl MultitapDiffuser {
    pub fn new(delay_buffer_size: usize) -> Self {
        let mut m = Self {
            buffer: vec![0.0; delay_buffer_size],
            output: vec![0.0; delay_buffer_size],
            len: delay_buffer_size,
            index: 0,
            tap_gains: [0.0; MAX_TAPS],
            tap_positions: [0; MAX_TAPS],
            tap_data: [0.0; MAX_TAPS],
            seed_values: [0.0; SEED_VALUE_COUNT],
            seed: 0,
            cross_seed: 0.0,
            count: 1,
            length: 1.0,
            gain: 1.0,
            decay: 0.0,
        };
        m.update_seeds();
        m
    }

    pub fn set_seed(&mut self, seed: i64) {
        self.seed = seed;
        self.update_seeds();
    }

    pub fn set_cross_seed(&mut self, cross_seed: f64) {
        self.cross_seed = cross_seed;
        self.update_seeds();
    }

    pub fn set_tap_count(&mut self, tap_count: usize) {
        self.count = tap_count.clamp(1, MAX_TAPS);
        self.update();
    }

    pub fn set_tap_length(&mut self, tap_length: usize) {
        self.length = tap_length as f64;
        self.update();
    }

    pub fn set_tap_decay(&mut self, tap_decay: f64) {
        self.decay = tap_decay;
        self.update();
    }

    pub fn set_tap_gain(&mut self, tap_gain: f64) {
        self.gain = tap_gain;
        self.update();
    }

    pub fn get_output(&self) -> &[f64] {
        &self.output
    }

    pub fn process(&mut self, input: &[f64]) {
        let n = input.len().min(self.len);
        let cnt = self.count.clamp(1, MAX_TAPS);
        debug_assert_eq!(self.buffer.len(), self.len);
        debug_assert_eq!(self.output.len(), self.len);
        debug_assert!(n <= input.len());

        // SAFETY: `self.index` is kept in [0, self.len) by the check at
        // the top of each iteration. `tap_idx` uses `% self.len` so it is
        // always in range. `input`, `buffer`, `output` have length >= n
        // (or >= self.len). The tap_positions/tap_gains arrays have fixed
        // length MAX_TAPS and `cnt <= MAX_TAPS`.
        unsafe {
            let input_ptr = input.as_ptr();
            let buffer_ptr = self.buffer.as_mut_ptr();
            let output_ptr = self.output.as_mut_ptr();

            for i in 0..n {
                if self.index < 0 {
                    self.index += self.len as i64;
                }
                let idx = self.index as usize;
                *buffer_ptr.add(idx) = *input_ptr.add(i);
                let mut acc = 0.0_f64;

                for j in 0..cnt {
                    // `idx < len` and `tap_positions[j] < len`, so
                    // `idx + tap_positions[j] < 2 * len`. One conditional
                    // subtract avoids the `%` (which is 20–40 cycles on
                    // x86) and is exactly equivalent.
                    let off = *self.tap_positions.get_unchecked(j);
                    let sum = idx + off;
                    let tap_idx = if sum >= self.len { sum - self.len } else { sum };
                    acc += *buffer_ptr.add(tap_idx) * *self.tap_gains.get_unchecked(j);
                }
                *output_ptr.add(i) = acc;

                self.index -= 1;
            }
        }
    }

    pub fn clear_buffers(&mut self) {
        self.buffer.fill(0.0);
        self.output.fill(0.0);
    }

    fn update(&mut self) {
        let count = self.count.clamp(1, MAX_TAPS);
        let mut length = self.length;
        if length < count as f64 {
            length = count as f64;
        }

        // Used to adjust the volume of the overall output as it grows with more taps.
        let tap_count_factor = 1.0 / (1.0 + ((count as f64) / (MAX_TAPS as f64)).sqrt());

        let mut s = 0usize;

        let mut sum_lengths = 0.0;
        for i in 0..count {
            let val = 0.1 + self.seed_values[s];
            s += 1;
            self.tap_data[i] = val;
            sum_lengths += val;
        }

        let scale_length = length / sum_lengths;
        self.tap_positions[0] = 0;
        for i in 1..count {
            self.tap_positions[i] =
                self.tap_positions[i - 1] + (self.tap_data[i] * scale_length) as usize;
        }

        let last_tap_pos = self.tap_positions[count - 1];
        for i in 0..count {
            // decay = 0 → no decay; decay = 1 → gain at the last tap ≈ 0.01 (-40 dB)
            let g = 10.0f64.powf(
                -self.decay * 2.0 * self.tap_positions[i] as f64 / (last_tap_pos as f64 + 1.0),
            );
            let tap = (2.0 * self.seed_values[s] - 1.0) * tap_count_factor;
            s += 1;
            self.tap_gains[i] = tap * g * self.gain;
        }

        // Set the tap vs. clean mix.
        self.tap_gains[0] = 1.0 - self.gain;
    }

    fn update_seeds(&mut self) {
        sha_random::generate_cross_into(
            self.seed,
            SEED_VALUE_COUNT,
            self.cross_seed,
            &mut self.seed_values,
        );
        self.update();
    }
}
