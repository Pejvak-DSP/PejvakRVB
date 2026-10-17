//! AudioLib::Lp1 – GPL-3.0
//!
//! Faithful Rust port of `CloudSeed.Native/AudioLib/Lp1.h` (legacy-v1):
//! a one-pole low-pass with the original alpha derivation and the
//! original "zero-input noise gate" behaviour.

use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct Lp1 {
    fs: f64,
    b0: f64,
    a1: f64,
    cutoff_hz: f64,
    pub output: f64,
}

impl Lp1 {
    pub fn new(fs: f64) -> Self {
        let mut f = Self {
            fs,
            b0: 0.0,
            a1: 0.0,
            cutoff_hz: 20000.0,
            output: 0.0,
        };
        f.set_samplerate(fs);
        f.set_cutoff_hz(20000.0);
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
        if input == 0.0 && self.output < 1.0e-12 {
            self.output = 0.0;
        } else {
            self.output = self.b0 * input + self.a1 * self.output;
        }
        self.output
    }

    pub fn process_inplace(&mut self, buffer: &mut [f64]) {
        for x in buffer.iter_mut() {
            *x = self.process(*x);
        }
    }

    pub fn reset(&mut self) {
        self.output = 0.0;
    }
}
