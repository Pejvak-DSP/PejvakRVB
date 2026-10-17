//! AudioLib::Hp1 – GPL-3.0
//!
//! Faithful Rust port of `CloudSeed.Native/AudioLib/Hp1.h` (legacy-v1):
//! a one-pole high-pass with the original alpha derivation and the
//! original "zero-input noise gate" behaviour.

use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct Hp1 {
    fs: f64,
    b0: f64,
    a1: f64,
    cutoff_hz: f64,
    lp_out: f64,
    pub output: f64,
}

impl Hp1 {
    pub fn new(samplerate: f64) -> Self {
        let mut f = Self {
            fs: samplerate,
            b0: 0.0,
            a1: 0.0,
            cutoff_hz: 20.0,
            lp_out: 0.0,
            output: 0.0,
        };
        f.set_samplerate(samplerate);
        f.set_cutoff_hz(20.0);
        f
    }

    pub fn set_samplerate(&mut self, samplerate: f64) {
        self.fs = samplerate;
        self.update();
    }

    pub fn set_cutoff_hz(&mut self, hz: f64) {
        self.cutoff_hz = hz;
        self.update();
    }

    fn update(&mut self) {
        let mut cutoff = self.cutoff_hz;
        if cutoff >= self.fs * 0.5 {
            cutoff = self.fs * 0.499;
        }
        let x = 2.0 * PI * cutoff / self.fs;
        let nn = 2.0 - x.cos();
        let alpha = nn - (nn * nn - 1.0).max(0.0).sqrt();
        self.a1 = alpha;
        self.b0 = 1.0 - alpha;
    }

    #[inline]
    pub fn process(&mut self, input: f64) -> f64 {
        if input == 0.0 && self.lp_out < 1.0e-12 {
            self.output = 0.0;
        } else {
            self.lp_out = self.b0 * input + self.a1 * self.lp_out;
            self.output = input - self.lp_out;
        }
        self.output
    }

    pub fn process_inplace(&mut self, buffer: &mut [f64]) {
        for x in buffer.iter_mut() {
            *x = self.process(*x);
        }
    }

    pub fn reset(&mut self) {
        self.lp_out = 0.0;
        self.output = 0.0;
    }
}
