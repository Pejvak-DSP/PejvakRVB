//! PEJVAK Texture Rack – Drift layer – GPL-3.0
//!
//! A slow, subtle pitch drift: a short fractional delay whose length is
//! modulated by an extremely slow LFO (0.02–0.5 Hz). The perceived effect
//! is not pitch shift but a gentle "wobble" that makes the reverb feel
//! alive, as if the room were breathing.
//!
//! Signal flow (per sample):
//!
//!   in ─┬─→ [delay (drift-modulated)] ──→ wet
//!       │                ↑
//!       └── feedback ────┘
//!
//! The delay time is short (5–25 ms) and the modulation depth is tiny
//! (a fraction of a millisecond), so the pitch artifact stays under a
//! few cents — inaudible on its own, but present as movement.

use crate::texture::TextureLayer;
use std::any::Any;

/// Delay buffer length in seconds. 50 ms covers the whole mod range
/// plus safety.
const DELAY_BUFFER_SECS: f64 = 0.05;

/// Base delay range (seconds).
const MIN_DELAY_SECS: f64 = 0.005;
const MAX_DELAY_SECS: f64 = 0.025;

/// Modulation depth in seconds. Very small — 0.3 ms at 48 kHz is about
/// 14 samples of swing. This is enough to make the drift audible without
/// sounding like a chorus.
const MOD_DEPTH_SECS: f64 = 0.0004;

const DRIFT_WET_MAKEUP: f64 = 1.0;

pub struct Drift {
    samplerate: f64,

    // ---- Delay line (per channel) --------------------------------
    buf_l: Vec<f64>,
    buf_r: Vec<f64>,
    write: usize,
    buf_len: usize,

    // ---- LFO ------------------------------------------------------
    lfo_phase: f64,
    lfo_rate_hz: f64,
    /// Second LFO at a slightly different rate, summed with the first to
    /// avoid a perfectly periodic feel.
    lfo_phase2: f64,
    lfo_rate2_hz: f64,

    // ---- Feedback -------------------------------------------------
    fb_l: f64,
    fb_r: f64,

    // ---- Parameters -----------------------------------------------
    mix: f64,
    // Per-sample smoothing of `mix` — hides the crossfade
    // discontinuity when the enable switch is toggled.
    mix_smoothed: f64,
    mix_smooth_coeff: f64,
    feedback: f64,
    /// Base delay as 0..1 within MIN_DELAY..MAX_DELAY.
    delay_norm: f64,
    /// LFO rate as 0..1 within 0.02..0.5 Hz.
    rate_norm: f64,
    /// LFO depth as 0..1 within 0..MOD_DEPTH_SECS.
    depth_norm: f64,
}

impl Drift {
    pub fn new(_buffer_size: usize, samplerate: f64) -> Self {
        let buf_len = (DELAY_BUFFER_SECS * samplerate).ceil() as usize;
        let mut d = Self {
            samplerate,
            buf_l: vec![0.0; buf_len],
            buf_r: vec![0.0; buf_len],
            write: 0,
            buf_len,
            lfo_phase: 0.0,
            lfo_rate_hz: 0.15,
            lfo_phase2: 0.33,
            lfo_rate2_hz: 0.11,
            fb_l: 0.0,
            fb_r: 0.0,
            mix: 0.0,
            mix_smoothed: 0.0,
            // 20 ms one-pole smoother for the dry/wet crossfade.
            mix_smooth_coeff: 1.0 - (-1.0 / (0.020 * samplerate)).exp(),
            feedback: 0.3,
            delay_norm: 0.5,
            rate_norm: 0.3,
            depth_norm: 1.0,
        };
        d.update_lfo();
        d
    }

    fn update_lfo(&mut self) {
        // 0.02 .. 0.52 Hz. The upper bound is still sub-audio, so the
        // modulation never produces an audible vibrato.
        self.lfo_rate_hz = 0.02 + self.rate_norm * 0.50;
        // Second LFO runs slightly slower than the first so the sum
        // never repeats exactly.
        self.lfo_rate2_hz = self.lfo_rate_hz * 0.73;
    }

    fn base_delay_samples(&self) -> f64 {
        let secs = MIN_DELAY_SECS + self.delay_norm * (MAX_DELAY_SECS - MIN_DELAY_SECS);
        secs * self.samplerate
    }

    #[inline]
    fn read_interp(buf: &[f64], pos: f64) -> f64 {
        let len = buf.len();
        let p = pos.rem_euclid(len as f64);
        let i0 = p as usize;
        let i1 = if i0 + 1 >= len { 0 } else { i0 + 1 };
        let frac = p - i0 as f64;
        buf[i0] * (1.0 - frac) + buf[i1] * frac
    }
}

impl TextureLayer for Drift {
    fn process_inplace(&mut self, left: &mut [f64], right: &mut [f64]) {
        // Fast path: fully faded out and target is zero.
        if self.mix <= 0.0 && self.mix_smoothed < 1.0e-6 {
            return;
        }

        let n = left.len().min(right.len());
        let fb_gain = self.feedback.clamp(0.0, 0.85);
        let buf_len = self.buf_len;
        let buf_len_f = buf_len as f64;
        let base_delay = self.base_delay_samples();
        let mod_depth = self.depth_norm.clamp(0.0, 1.0) * MOD_DEPTH_SECS * self.samplerate;
        let inc_a = self.lfo_rate_hz / self.samplerate;
        let inc_b = self.lfo_rate2_hz / self.samplerate;
        let smooth = self.mix_smooth_coeff;
        let mix_target = self.mix;

        for i in 0..n {
            // Per-sample smoothing of the crossfade parameter.
            self.mix_smoothed += smooth * (mix_target - self.mix_smoothed);
            let m = self.mix_smoothed.clamp(0.0, 1.0);
            let angle = m * std::f64::consts::FRAC_PI_2;
            let (wet_sin, dry_cos) = angle.sin_cos();
            let dry_gain = dry_cos;
            let wet_gain = wet_sin * DRIFT_WET_MAKEUP;

            let in_l = left[i];
            let in_r = right[i];

            // ---- 1. Input + feedback -------------------------------
            let pre_l = in_l + self.fb_l * fb_gain;
            let pre_r = in_r + self.fb_r * fb_gain;

            // ---- 2. Write into the delay buffer --------------------
            self.buf_l[self.write] = pre_l;
            self.buf_r[self.write] = pre_r;

            // ---- 3. Dual-LFO modulated read position ---------------
            // Sum of two sines → an irregular, breathing drift.
            let lfo_a = (std::f64::consts::TAU * self.lfo_phase).sin();
            let lfo_b = (std::f64::consts::TAU * self.lfo_phase2).sin();
            let lfo = (lfo_a + lfo_b) * 0.5;
            let delay_samples = (base_delay + mod_depth * lfo).clamp(1.0, buf_len_f - 2.0);
            let read_pos = self.write as f64 - delay_samples;

            let wet_l = Self::read_interp(&self.buf_l, read_pos);
            let wet_r = Self::read_interp(&self.buf_r, read_pos);

            // ---- 4. Store wet as next feedback ---------------------
            self.fb_l = wet_l;
            self.fb_r = wet_r;

            // ---- 5. Advance LFOs and write head -------------------
            self.lfo_phase += inc_a;
            if self.lfo_phase >= 1.0 {
                self.lfo_phase -= 1.0;
            }
            self.lfo_phase2 += inc_b;
            if self.lfo_phase2 >= 1.0 {
                self.lfo_phase2 -= 1.0;
            }
            self.write += 1;
            if self.write >= buf_len {
                self.write = 0;
            }

            // ---- 6. Output blend -----------------------------------
            left[i] = in_l * dry_gain + wet_l * wet_gain;
            right[i] = in_r * dry_gain + wet_r * wet_gain;
        }
    }

    fn clear_buffers(&mut self) {
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write = 0;
        self.fb_l = 0.0;
        self.fb_r = 0.0;
        self.lfo_phase = 0.0;
        self.lfo_phase2 = 0.33;
    }

    fn set_samplerate(&mut self, samplerate: f64) {
        self.samplerate = samplerate;
        let new_len = (DELAY_BUFFER_SECS * samplerate).ceil() as usize;
        if new_len != self.buf_len {
            self.buf_l = vec![0.0; new_len];
            self.buf_r = vec![0.0; new_len];
            self.buf_len = new_len;
            self.write = 0;
        }
        self.update_lfo();
        self.mix_smooth_coeff = 1.0 - (-1.0 / (0.020 * samplerate)).exp();
    }

    fn set_mix(&mut self, mix: f64) {
        let old = self.mix;
        self.mix = mix.clamp(0.0, 1.0);
        // Rising edge from exactly zero: perform a full reset,
        // including the delay buffer. `reset_runtime_state` alone
        // clears feedback and filter state but leaves the buffer
        // contents untouched; when the layer has been bypassed for
        // a long time (e.g. while the TX kill switch is engaged),
        // that buffer holds audio from minutes ago. Without
        // clearing it the read head would start producing stale
        // audio the moment the layer re-enables, which sounds like
        // a click or crackle even though the wet gain itself is
        // ramping in smoothly.
        if old <= 0.0 && self.mix > 0.0 {
            self.clear_buffers();
        }
    }

    fn mix(&self) -> f64 {
        self.mix
    }

    fn needs_processing(&self) -> bool {
        self.mix > 0.0 || self.mix_smoothed > 1.0e-6
    }

    fn name(&self) -> &'static str {
        "Drift"
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Drift {

    pub fn set_feedback(&mut self, fb: f64) {
        self.feedback = fb.clamp(0.0, 0.85);
    }

    pub fn set_delay_norm(&mut self, d: f64) {
        self.delay_norm = d.clamp(0.0, 1.0);
    }

    pub fn set_rate_norm(&mut self, r: f64) {
        self.rate_norm = r.clamp(0.0, 1.0);
        self.update_lfo();
    }

    pub fn set_depth_norm(&mut self, d: f64) {
        self.depth_norm = d.clamp(0.0, 1.0);
    }
}
