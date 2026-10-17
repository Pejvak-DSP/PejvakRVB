//! PEJVAK Texture Rack – Mist layer – GPL-3.0
//!
//! A soft, hazy texture: the input is high-passed, sent through a very
//! short modulated delay with light feedback, then low-passed. A slow
//! LFO drifts the delay time back and forth, so the layer sounds like
//! it is breathing inside a fog rather than echoing in a room.
//!
//! Signal flow (per sample):
//!
//!   in ─→ HPF ─→ [delay + LFO drift] ─→ LPF ─→ wet
//!                        ↑                      │
//!                        └── feedback ──────────┘
//!
//! The delay length is short (8–40 ms) — much shorter than a reverb
//! pre-delay — so it acts as a diffuse thickening rather than an echo.

use crate::texture::TextureLayer;
use std::any::Any;

/// Delay buffer length in seconds. 100 ms comfortably covers the whole
/// mod range plus safety.
const DELAY_BUFFER_SECS: f64 = 0.1;

/// Minimum / maximum base delay in seconds.
const MIN_DELAY_SECS: f64 = 0.008;
const MAX_DELAY_SECS: f64 = 0.040;

/// Modulation depth in seconds. Kept small so the pitch artifact from
/// the moving read head stays under a few cents.
const MOD_DEPTH_SECS: f64 = 0.003;

const MIST_WET_MAKEUP: f64 = 1.0;

pub struct Mist {
    samplerate: f64,

    // ---- Delay line (per channel) --------------------------------
    buf_l: Vec<f64>,
    buf_r: Vec<f64>,
    write: usize,
    buf_len: usize,

    // ---- LFO ------------------------------------------------------
    lfo_phase: f64,
    lfo_rate_hz: f64,

    // ---- One-pole HPF (before delay) -----------------------------
    hp_state_l: f64,
    hp_state_r: f64,
    hp_coeff: f64,

    // ---- One-pole LPF (after delay) ------------------------------
    lp_state_l: f64,
    lp_state_r: f64,
    lp_coeff: f64,

    // ---- Feedback -------------------------------------------------
    fb_l: f64,
    fb_r: f64,

    // ---- Parameters -----------------------------------------------
    mix: f64,
    // Per-sample smoothing of `mix` — hides the crossfade
    // discontinuity when the enable switch is toggled.
    mix_smoothed: f64,
    mix_smooth_coeff: f64,
    hp_hz: f64,
    feedback: f64,
    /// Base delay as a fraction of the min..max range (0..1).
    delay_norm: f64,
    /// LFO rate as a fraction of the 0.05..2 Hz range (0..1).
    rate_norm: f64,
}

impl Mist {
    pub fn new(_buffer_size: usize, samplerate: f64) -> Self {
        let buf_len = (DELAY_BUFFER_SECS * samplerate).ceil() as usize;
        let mut m = Self {
            samplerate,
            buf_l: vec![0.0; buf_len],
            buf_r: vec![0.0; buf_len],
            write: 0,
            buf_len,
            lfo_phase: 0.0,
            lfo_rate_hz: 0.3,
            hp_state_l: 0.0,
            hp_state_r: 0.0,
            hp_coeff: 0.0,
            lp_state_l: 0.0,
            lp_state_r: 0.0,
            lp_coeff: 0.0,
            fb_l: 0.0,
            fb_r: 0.0,
            mix: 0.0,
            mix_smoothed: 0.0,
            // 20 ms one-pole smoother for the dry/wet crossfade.
            mix_smooth_coeff: 1.0 - (-1.0 / (0.020 * samplerate)).exp(),
            hp_hz: 400.0,
            feedback: 0.5,
            delay_norm: 0.5,
            rate_norm: 0.25,
        };
        m.update_filters();
        m.update_lfo();
        m
    }

    fn update_filters(&mut self) {
        let hp = (self.hp_hz / self.samplerate).clamp(1e-6, 0.49);
        self.hp_coeff = 1.0 - (-2.0 * std::f64::consts::PI * hp).exp();
        // LPF is fixed at 8 kHz — just enough to round off the fizz
        // without killing the shimmer.
        let lp = (8000.0 / self.samplerate).clamp(1e-6, 0.49);
        self.lp_coeff = 1.0 - (-2.0 * std::f64::consts::PI * lp).exp();
    }

    fn update_lfo(&mut self) {
        self.lfo_rate_hz = 0.05 + self.rate_norm * (25.0 - 0.05);
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

impl TextureLayer for Mist {
    fn process_inplace(&mut self, left: &mut [f64], right: &mut [f64]) {
        // Fast path: fully faded out and target is zero.
        if self.mix <= 0.0 && self.mix_smoothed < 1.0e-6 {
            return;
        }

        let n = left.len().min(right.len());
        let fb_gain = self.feedback.clamp(0.0, 0.9);
        let buf_len = self.buf_len;
        let buf_len_f = buf_len as f64;
        let base_delay = self.base_delay_samples();
        let mod_depth = MOD_DEPTH_SECS * self.samplerate;
        let lfo_inc = self.lfo_rate_hz / self.samplerate;
        let smooth = self.mix_smooth_coeff;
        let mix_target = self.mix;

        for i in 0..n {
            // Per-sample smoothing of the crossfade parameter.
            self.mix_smoothed += smooth * (mix_target - self.mix_smoothed);
            let m = self.mix_smoothed.clamp(0.0, 1.0);
            let angle = m * std::f64::consts::FRAC_PI_2;
            let (wet_sin, dry_cos) = angle.sin_cos();
            let dry_gain = dry_cos;
            let wet_gain = wet_sin * MIST_WET_MAKEUP;

            let in_l = left[i];
            let in_r = right[i];

            // ---- 1. HPF on the input + feedback -------------------
            // `fb` carries the previous wet output (already filtered).
            let pre_l = in_l + self.fb_l * fb_gain;
            let pre_r = in_r + self.fb_r * fb_gain;
            self.hp_state_l += self.hp_coeff * (pre_l - self.hp_state_l);
            self.hp_state_r += self.hp_coeff * (pre_r - self.hp_state_r);
            let hp_l = pre_l - self.hp_state_l;
            let hp_r = pre_r - self.hp_state_r;

            // ---- 2. Write into the delay buffer -------------------
            self.buf_l[self.write] = hp_l;
            self.buf_r[self.write] = hp_r;

            // ---- 3. LFO-modulated read position -------------------
            // `phase_a` is the LFO's value in [-1, 1] via a sine.
            let lfo = (std::f64::consts::TAU * self.lfo_phase).sin();
            let delay_samples = (base_delay + mod_depth * lfo).clamp(1.0, buf_len_f - 2.0);
            let read_pos = self.write as f64 - delay_samples;

            let d_l = Self::read_interp(&self.buf_l, read_pos);
            let d_r = Self::read_interp(&self.buf_r, read_pos);

            // ---- 4. LPF on the delayed signal ---------------------
            self.lp_state_l += self.lp_coeff * (d_l - self.lp_state_l);
            self.lp_state_r += self.lp_coeff * (d_r - self.lp_state_r);
            let wet_l = self.lp_state_l;
            let wet_r = self.lp_state_r;

            // ---- 5. Store wet as next iteration's feedback --------
            self.fb_l = wet_l;
            self.fb_r = wet_r;

            // ---- 6. Advance LFO and write head --------------------
            self.lfo_phase += lfo_inc;
            if self.lfo_phase >= 1.0 {
                self.lfo_phase -= 1.0;
            }
            self.write += 1;
            if self.write >= buf_len {
                self.write = 0;
            }

            // ---- 7. Output blend -----------------------------------
            left[i] = in_l * dry_gain + wet_l * wet_gain;
            right[i] = in_r * dry_gain + wet_r * wet_gain;
        }
    }

    fn clear_buffers(&mut self) {
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write = 0;
        self.hp_state_l = 0.0;
        self.hp_state_r = 0.0;
        self.lp_state_l = 0.0;
        self.lp_state_r = 0.0;
        self.fb_l = 0.0;
        self.fb_r = 0.0;
        self.lfo_phase = 0.0;
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
        self.update_filters();
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
        "Mist"
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Mist {

    pub fn set_hp_hz(&mut self, hz: f64) {
        self.hp_hz = hz.clamp(50.0, self.samplerate * 0.45);
        self.update_filters();
    }

    pub fn set_feedback(&mut self, fb: f64) {
        self.feedback = fb.clamp(0.0, 0.9);
    }

    pub fn set_delay_norm(&mut self, d: f64) {
        self.delay_norm = d.clamp(0.0, 1.0);
    }

    pub fn set_rate_norm(&mut self, r: f64) {
        self.rate_norm = r.clamp(0.0, 1.0);
        self.update_lfo();
    }
}
