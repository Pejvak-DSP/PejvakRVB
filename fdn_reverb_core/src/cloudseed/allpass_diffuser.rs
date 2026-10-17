//! AllpassDiffuser – GPL-3.0
//!
//! Faithful Rust port of `CloudSeed.Native/AllpassDiffuser.h` (legacy-v1).
//! Every buffer and arithmetic operation uses `f64`, matching the C++
//! `double` type. The chain-through-stages loop uses `split_at_mut` so the
//! borrow checker accepts reading stage `i-1`'s output while writing
//! stage `i` — no scratch buffer, no unsafe.

use crate::cloudseed::modulated_allpass::ModulatedAllpass;
use crate::cloudseed::sha_random;

/// Maximum number of cascaded diffusion stages (matches `MaxStageCount`).
pub const MAX_STAGE_COUNT: usize = 8;

/// Number of deterministic seed values needed per diffuser.
pub const SEED_VALUE_COUNT: usize = MAX_STAGE_COUNT * 3;

#[derive(Clone, Debug)]
pub struct AllpassDiffuser {
    filters: Vec<ModulatedAllpass>,
    delay: usize,
    mod_rate: f64,
    seed_values: [f64; SEED_VALUE_COUNT],
    seed: i64,
    cross_seed: f64,
    pub stages: usize,
    samplerate: f64,
    buffer_size: usize,
}

impl AllpassDiffuser {
    pub fn new(samplerate: f64, _delay_buffer_length_ms: f64, buffer_size: usize) -> Self {
        let mut filters = Vec::with_capacity(MAX_STAGE_COUNT);
        for _ in 0..MAX_STAGE_COUNT {
            filters.push(ModulatedAllpass::new(buffer_size, 100));
        }

        let mut d = Self {
            filters,
            delay: 0,
            mod_rate: 0.0,
            seed_values: [0.0; SEED_VALUE_COUNT],
            seed: 23456,
            cross_seed: 0.0,
            stages: 1,
            samplerate,
            buffer_size,
        };
        d.update_seeds();
        d.set_samplerate(samplerate);
        d
    }

    pub fn set_samplerate(&mut self, samplerate: f64) {
        self.samplerate = samplerate;
        self.set_mod_rate(self.mod_rate);
    }

    pub fn set_seed(&mut self, seed: i64) {
        self.seed = seed;
        self.update_seeds();
    }

    pub fn set_cross_seed(&mut self, cross_seed: f64) {
        self.cross_seed = cross_seed;
        self.update_seeds();
    }

    pub fn set_modulation_enabled(&mut self, enabled: bool) {
        for f in &mut self.filters {
            f.modulation_enabled = enabled;
        }
    }

    pub fn set_interpolation_enabled(&mut self, enabled: bool) {
        for f in &mut self.filters {
            f.interpolation_enabled = enabled;
        }
    }

    /// Output of the last active stage.
    pub fn get_output(&self) -> &[f64] {
        let idx = (self.stages.clamp(1, MAX_STAGE_COUNT) - 1).min(self.filters.len() - 1);
        self.filters[idx].get_output()
    }

    pub fn set_delay(&mut self, delay_samples: usize) {
        self.delay = delay_samples;
        self.update();
    }

    pub fn set_feedback(&mut self, feedback: f64) {
        for f in &mut self.filters {
            f.feedback = feedback;
        }
    }

    pub fn set_mod_amount(&mut self, amount: f64) {
        for (i, f) in self.filters.iter_mut().enumerate() {
            f.mod_amount = amount * (0.85 + 0.3 * self.seed_values[MAX_STAGE_COUNT + i]);
        }
    }

    pub fn set_mod_rate(&mut self, rate: f64) {
        self.mod_rate = rate;
        for (i, f) in self.filters.iter_mut().enumerate() {
            f.mod_rate =
                rate * (0.85 + 0.3 * self.seed_values[2 * MAX_STAGE_COUNT + i]) / self.samplerate;
        }
    }

    /// Processes one block through the active cascade stages.
    pub fn process(&mut self, input: &[f64]) {
        let stages = self.stages.clamp(1, MAX_STAGE_COUNT);
        let n = input.len().min(self.buffer_size);
        self.filters[0].process(&input[..n]);

        for i in 1..stages {
            let (prev, rest) = self.filters.split_at_mut(i);
            let src = prev[i - 1].get_output();
            let len = n.min(src.len());
            rest[0].process(&src[..len]);
        }
    }

    pub fn clear_buffers(&mut self) {
        for f in &mut self.filters {
            f.clear_buffers();
        }
    }

    fn update(&mut self) {
        for (i, f) in self.filters.iter_mut().enumerate() {
            let r = self.seed_values[i];
            let d = 10.0_f64.powf(r) * 0.1; // 0.1 … 1.0
            f.sample_delay = ((self.delay as f64) * d) as usize;
        }
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
