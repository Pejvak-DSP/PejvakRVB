//! DelayLine – GPL-3.0
//!
//! Faithful Rust port of `CloudSeed.Native/DelayLine.h` (legacy-v1).
//! All buffers and arithmetic use `f64`.
//!
//! Note: The Velvet-Noise diffusion filter lives in `ReverbChannel`,
//! not here. VN must be applied to the summed wet output of all
//! delay lines, outside every feedback loop, per the VFDN structure
//! described in Fagerström et al. (DAFx 2020).

use crate::cloudseed::allpass_diffuser::{AllpassDiffuser, MAX_STAGE_COUNT};
use crate::cloudseed::biquad::{Biquad, FilterType};
use crate::cloudseed::lp1::Lp1;
use crate::cloudseed::modulated_delay::ModulatedDelay;

#[derive(Clone, Debug)]
pub struct DelayLine {
    delay: ModulatedDelay,
    diffuser: AllpassDiffuser,
    low_shelf: Biquad,
    high_shelf: Biquad,
    low_pass: Lp1,
    temp_buffer: Vec<f64>,
    mixed_buffer: Vec<f64>,
    filter_output_buffer: Vec<f64>,
    buffer_size: usize,
    feedback: f64,
    samplerate: f64,
    pub diffuser_enabled: bool,
    pub low_shelf_enabled: bool,
    pub high_shelf_enabled: bool,
    pub cutoff_enabled: bool,
    pub late_stage_tap: bool,
}

impl DelayLine {
    pub fn new(buffer_size: usize, samplerate: i32) -> Self {
        let sr = samplerate as f64;

        let low_pass = Lp1::new(sr);
        let delay = ModulatedDelay::new(buffer_size, (sr * 2.0) as usize, 10000);
        let diffuser = AllpassDiffuser::new(sr, 150.0, buffer_size);

        let mut low_shelf = Biquad::new(FilterType::LowShelf, sr);
        low_shelf.set_slope(1.0);
        low_shelf.set_gain_db(-20.0);
        low_shelf.set_frequency(20.0);

        let mut high_shelf = Biquad::new(FilterType::HighShelf, sr);
        high_shelf.set_slope(1.0);
        high_shelf.set_gain_db(-20.0);
        high_shelf.set_frequency(19000.0);

        low_shelf.update();
        high_shelf.update();

        let mut d = Self {
            delay,
            diffuser,
            low_shelf,
            high_shelf,
            low_pass,
            temp_buffer: vec![0.0; buffer_size],
            mixed_buffer: vec![0.0; buffer_size],
            filter_output_buffer: vec![0.0; buffer_size],
            buffer_size,
            feedback: 0.0,
            samplerate: sr,
            diffuser_enabled: false,
            low_shelf_enabled: false,
            high_shelf_enabled: false,
            cutoff_enabled: false,
            late_stage_tap: false,
        };
        d.set_samplerate(sr);
        d.set_diffuser_seed(1, 0.0);
        d
    }

    pub fn set_samplerate(&mut self, samplerate: f64) {
        self.samplerate = samplerate;
        self.diffuser.set_samplerate(samplerate);
        self.low_pass.set_samplerate(samplerate);
        self.low_shelf.set_samplerate(samplerate);
        self.high_shelf.set_samplerate(samplerate);
    }

    pub fn set_diffuser_seed(&mut self, seed: i64, cross_seed: f64) {
        self.diffuser.set_seed(seed);
        self.diffuser.set_cross_seed(cross_seed);
    }

    pub fn set_delay(&mut self, delay_samples: usize) {
        self.delay.sample_delay = delay_samples;
    }

    pub fn get_delay(&self) -> usize {
        self.delay.sample_delay
    }

    pub fn set_feedback(&mut self, feedb: f64) {
        self.feedback = feedb;
    }

    pub fn set_diffuser_delay(&mut self, delay_samples: usize) {
        self.diffuser.set_delay(delay_samples);
    }

    pub fn set_diffuser_feedback(&mut self, feedb: f64) {
        self.diffuser.set_feedback(feedb);
    }

    pub fn set_diffuser_stages(&mut self, stages: usize) {
        self.diffuser.stages = stages.clamp(1, MAX_STAGE_COUNT);
    }

    pub fn set_low_shelf_gain(&mut self, gain: f64) {
        self.low_shelf.set_gain(gain);
        self.low_shelf.update();
    }

    pub fn set_low_shelf_frequency(&mut self, frequency: f64) {
        self.low_shelf.set_frequency(frequency);
        self.low_shelf.update();
    }

    pub fn set_high_shelf_gain(&mut self, gain: f64) {
        self.high_shelf.set_gain(gain);
        self.high_shelf.update();
    }

    pub fn set_high_shelf_frequency(&mut self, frequency: f64) {
        self.high_shelf.set_frequency(frequency);
        self.high_shelf.update();
    }

    pub fn set_cutoff_frequency(&mut self, frequency: f64) {
        self.low_pass.set_cutoff_hz(frequency);
    }

    pub fn set_line_mod_amount(&mut self, amount: f64) {
        self.delay.mod_amount = amount;
    }

    pub fn set_line_mod_rate(&mut self, rate: f64) {
        self.delay.mod_rate = rate;
    }

    pub fn set_diffuser_mod_amount(&mut self, amount: f64) {
        self.diffuser.set_modulation_enabled(amount > 0.0);
        self.diffuser.set_mod_amount(amount);
    }

    pub fn set_diffuser_mod_rate(&mut self, rate: f64) {
        self.diffuser.set_mod_rate(rate);
    }

    pub fn set_interpolation_enabled(&mut self, value: bool) {
        self.diffuser.set_interpolation_enabled(value);
    }

    /// Tapped output selection (matches the C++ `GetOutput`).
    pub fn get_output(&self) -> &[f64] {
        if self.late_stage_tap {
            if self.diffuser_enabled {
                self.diffuser.get_output()
            } else {
                &self.mixed_buffer
            }
        } else {
            self.delay.get_output()
        }
    }

    pub fn process(&mut self, input: &[f64]) {
        let n = input.len().min(self.buffer_size);

        // Feedback: input + previous block's final (post-EQ) output * feedback gain.
        for i in 0..n {
            self.mixed_buffer[i] = input[i] + self.filter_output_buffer[i] * self.feedback;
        }

        if self.late_stage_tap {
            // POST-PROCESS ROUTING: FX first, then EQ.
            //   mixed -> [diffuser ->] delay -> EQ -> filter_output_buffer
            if self.diffuser_enabled {
                let (mixed, diffuser) = (&self.mixed_buffer, &mut self.diffuser);
                diffuser.process(&mixed[..n]);
                let (diffuser, delay) = (&self.diffuser, &mut self.delay);
                let d_out = diffuser.get_output();
                delay.process(&d_out[..n.min(d_out.len())]);
            } else {
                let (mixed, delay) = (&self.mixed_buffer, &mut self.delay);
                delay.process(&mixed[..n]);
            }
            let dly_out = self.delay.get_output();
            let copy_len = n.min(dly_out.len());
            self.temp_buffer[..copy_len].copy_from_slice(&dly_out[..copy_len]);

            if self.low_shelf_enabled {
                self.low_shelf.process_inplace(&mut self.temp_buffer[..n]);
            }
            if self.high_shelf_enabled {
                self.high_shelf.process_inplace(&mut self.temp_buffer[..n]);
            }
            if self.cutoff_enabled {
                self.low_pass.process_inplace(&mut self.temp_buffer[..n]);
            }
        } else {
            // PRE-PROCESS ROUTING: EQ first, then FX.
            //   mixed -> EQ -> [diffuser ->] delay -> filter_output_buffer
            self.temp_buffer[..n].copy_from_slice(&self.mixed_buffer[..n]);

            if self.low_shelf_enabled {
                self.low_shelf.process_inplace(&mut self.temp_buffer[..n]);
            }
            if self.high_shelf_enabled {
                self.high_shelf.process_inplace(&mut self.temp_buffer[..n]);
            }
            if self.cutoff_enabled {
                self.low_pass.process_inplace(&mut self.temp_buffer[..n]);
            }

            if self.diffuser_enabled {
                let (temp, diffuser) = (&self.temp_buffer, &mut self.diffuser);
                diffuser.process(&temp[..n]);
                let (diffuser, delay) = (&self.diffuser, &mut self.delay);
                let d_out = diffuser.get_output();
                delay.process(&d_out[..n.min(d_out.len())]);
            } else {
                let (temp, delay) = (&self.temp_buffer, &mut self.delay);
                delay.process(&temp[..n]);
            }
            let dly_out = self.delay.get_output();
            let copy_len = n.min(dly_out.len());
            self.temp_buffer[..copy_len].copy_from_slice(&dly_out[..copy_len]);
        }

        self.filter_output_buffer[..n].copy_from_slice(&self.temp_buffer[..n]);
    }

    pub fn clear_diffuser_buffer(&mut self) {
        self.diffuser.clear_buffers();
    }

    pub fn clear_buffers(&mut self) {
        self.delay.clear_buffers();
        self.diffuser.clear_buffers();
        self.low_shelf.clear_buffers();
        self.high_shelf.clear_buffers();
        self.low_pass.reset();
        self.temp_buffer.fill(0.0);
        self.filter_output_buffer.fill(0.0);
    }
}
