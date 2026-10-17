//! PEJVAK Texture Rack – Shimmer layer – GPL-3.0
//!
//! A classic Eno/Lanois shimmer: a feedback loop containing a +12
//! semitone granular pitch shifter, a low-cut before it, and a high-cut
//! after it. The pitch shifter uses two read heads with a Hann crossfade
//! — the simplest real-time octave-up technique that does not require an
//! FFT and stays glitch-free on sustained material.
//!
//! Signal flow (per sample):
//!
//!   in ─┬─→ [write buffer] ─→ [head A] ─┐
//!       │                                ├─ Hann xfade ─→ [LPF] ─→ wet
//!       │                                │                            │
//!       │                  [head B] ─────┘                            │
//!       │                                                             │
//!       └──────────────────── fb ─── HP(wet) ←────────────────────────┘
//!
//! Read position for head A is `write - grain_len + phase_a`. As
//! `phase_a` advances faster than 1 sample per output sample, the read
//! head moves *forward* relative to the write head — a pitch up-shift.
//! Head B is offset by half a grain so the two Hann windows sum to 1.

use crate::texture::TextureLayer;
use std::any::Any;

/// Internal buffer length for the pitch shifter, in seconds. 0.2 s is
/// enough for a +12 st shift at every sample rate we care about and keeps
/// the cache footprint tiny (~38 KB per channel at 48 kHz).
const GRAIN_BUFFER_SECS: f64 = 0.2;

/// Grain length, in seconds. Longer grains sound smoother on sustained
/// material but add latency; 60 ms is the sweet spot used by most
/// hardware shimmer units.
const GRAIN_SECS: f64 = 0.06;

/// Maximum pitch shift in semitones (parameter maps 0..1 to 0..+12).
const MAX_SEMITONES: f64 = 12.0;

/// Make-up gain applied to the wet signal so its RMS is close to the
/// dry signal's RMS. Without this, the crossfade would drop the
/// perceived loudness as `mix` rises, because the wet path is
/// naturally quieter than the input.
const SHIMMER_WET_MAKEUP: f64 = 1.0;

pub struct Shimmer {
    samplerate: f64,

    // ---- Per-channel state ----------------------------------------
    buf_l: Vec<f64>,
    buf_r: Vec<f64>,
    write: usize,
    buf_len: usize,

    // Two read heads: `pos_a`, `pos_b` are the phases (in samples) of
    // the current grain, in [0, grain_len). The actual read position is
    // `write - grain_len + pos_*` wrapped into the buffer.
    pos_a: f64,
    pos_b: f64,
    rate: f64,
    grain_len: f64,

    // ---- Filters (one-pole each, cheap and transparent) ------------
    hp_state_l: f64,
    hp_state_r: f64,
    hp_coeff: f64,
    lp_state_l: f64,
    lp_state_r: f64,
    lp_coeff: f64,

    // ---- Feedback --------------------------------------------------
    fb_l: f64,
    fb_r: f64,

    // ---- Parameters ------------------------------------------------
    mix: f64,
    mix_smoothed: f64,
    mix_smooth_coeff: f64,
    pitch_norm: f64,
    feedback: f64,
    low_cut_hz: f64,
    high_cut_hz: f64,
}

impl Shimmer {
    pub fn new(_buffer_size: usize, samplerate: f64) -> Self {
        let buf_len = (GRAIN_BUFFER_SECS * samplerate).ceil() as usize;
        let grain_len = (GRAIN_SECS * samplerate).max(64.0);

        let mut s = Self {
            samplerate,
            buf_l: vec![0.0; buf_len],
            buf_r: vec![0.0; buf_len],
            write: 0,
            buf_len,
            pos_a: 0.0,
            // Head B starts half a grain ahead so the two Hann windows
            // sum to a constant 1 across the crossfade.
            pos_b: grain_len * 0.5,
            rate: 2.0,
            grain_len,
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
            mix_smooth_coeff: 1.0 - (-1.0 / (0.020 * samplerate)).exp(),
            pitch_norm: 1.0,
            feedback: 0.6,
            low_cut_hz: 200.0,
            high_cut_hz: 8000.0,
        };
        s.update_pitch();
        s.update_filters();
        s
    }

    fn update_pitch(&mut self) {
        // pitch_norm 0..1 maps to -12..+12 semitones.
        //   0.0 → -12 st (rate 0.5, octave down)
        //   0.5 →   0 st (rate 1.0, unison)
        //   1.0 → +12 st (rate 2.0, octave up)
        let semis = (self.pitch_norm - 0.5) * 2.0 * MAX_SEMITONES;
        self.rate = (semis / 12.0).exp2(); // 0.5 .. 2.0
    }

    fn update_filters(&mut self) {
        let hp = (self.low_cut_hz / self.samplerate).clamp(1e-6, 0.49);
        self.hp_coeff = 1.0 - (-2.0 * std::f64::consts::PI * hp).exp();
        let lp = (self.high_cut_hz / self.samplerate).clamp(1e-6, 0.49);
        self.lp_coeff = 1.0 - (-2.0 * std::f64::consts::PI * lp).exp();
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

impl TextureLayer for Shimmer {
    fn process_inplace(&mut self, left: &mut [f64], right: &mut [f64]) {
        // Fast path: fully faded out and target is zero.
        if self.mix <= 0.0 && self.mix_smoothed < 1.0e-6 {
            return;
        }

        let n = left.len().min(right.len());
        let fb_gain = self.feedback.clamp(0.0, 0.95);
        let grain_len = self.grain_len;
        let buf_len = self.buf_len;
        let buf_len_f = buf_len as f64;
        let advance = self.rate - 1.0;
        let smooth = self.mix_smooth_coeff;
        let mix_target = self.mix;

        for i in 0..n {
            // Per-sample smoothing of the crossfade parameter.
            self.mix_smoothed += smooth * (mix_target - self.mix_smoothed);
            let m = self.mix_smoothed.clamp(0.0, 1.0);
            let angle = m * std::f64::consts::FRAC_PI_2;
            let (wet_sin, dry_cos) = angle.sin_cos();
            let dry_gain = dry_cos;
            let wet_gain = wet_sin * SHIMMER_WET_MAKEUP;

            let in_l = left[i];
            let in_r = right[i];

            // ---- 1. Shifter input = dry + feedback ------------------
            let shifter_in_l = in_l + self.fb_l * fb_gain;
            let shifter_in_r = in_r + self.fb_r * fb_gain;

            // ---- 2. Write into the circular buffer ------------------
            self.buf_l[self.write] = shifter_in_l;
            self.buf_r[self.write] = shifter_in_r;

            // ---- 3. Read both heads, relative to write head --------
            let w = self.write as f64;
            let read_a = (w - grain_len + self.pos_a).rem_euclid(buf_len_f);
            let read_b = (w - grain_len + self.pos_b).rem_euclid(buf_len_f);

            let ga = Self::read_interp(&self.buf_l, read_a);
            let gb = Self::read_interp(&self.buf_l, read_b);
            let ha = Self::read_interp(&self.buf_r, read_a);
            let hb = Self::read_interp(&self.buf_r, read_b);

            // ---- 4. Hann crossfade ---------------------------------
            let phase_a = self.pos_a / grain_len;
            let phase_b = self.pos_b / grain_len;
            let wa = 0.5 - 0.5 * (std::f64::consts::TAU * phase_a).cos();
            let wb = 0.5 - 0.5 * (std::f64::consts::TAU * phase_b).cos();
            let shifted_l = ga * wa + gb * wb;
            let shifted_r = ha * wa + hb * wb;

            // ---- 5. High-cut on shifter output ---------------------
            self.lp_state_l += self.lp_coeff * (shifted_l - self.lp_state_l);
            self.lp_state_r += self.lp_coeff * (shifted_r - self.lp_state_r);
            let wet_l = self.lp_state_l;
            let wet_r = self.lp_state_r;

            // ---- 6. Low-cut for next iteration's feedback ----------
            self.hp_state_l += self.hp_coeff * (wet_l - self.hp_state_l);
            self.hp_state_r += self.hp_coeff * (wet_r - self.hp_state_r);
            self.fb_l = wet_l - self.hp_state_l;
            self.fb_r = wet_r - self.hp_state_r;

            // ---- 7. Advance phases and write head ------------------
            self.pos_a += advance;
            self.pos_b += advance;
            if self.pos_a >= grain_len {
                self.pos_a -= grain_len;
            } else if self.pos_a < 0.0 {
                self.pos_a += grain_len;
            }
            if self.pos_b >= grain_len {
                self.pos_b -= grain_len;
            } else if self.pos_b < 0.0 {
                self.pos_b += grain_len;
            }
            self.write += 1;
            if self.write >= buf_len {
                self.write = 0;
            }

            // ---- 8. Output blend -----------------------------------
            left[i] = in_l * dry_gain + wet_l * wet_gain;
            right[i] = in_r * dry_gain + wet_r * wet_gain;
        }
    }

    fn clear_buffers(&mut self) {
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write = 0;
        self.pos_a = 0.0;
        self.pos_b = self.grain_len * 0.5;
        self.hp_state_l = 0.0;
        self.hp_state_r = 0.0;
        self.lp_state_l = 0.0;
        self.lp_state_r = 0.0;
        self.fb_l = 0.0;
        self.fb_r = 0.0;
    }

    fn set_samplerate(&mut self, samplerate: f64) {
        self.samplerate = samplerate;
        let buf_len = (GRAIN_BUFFER_SECS * samplerate).ceil() as usize;
        if buf_len != self.buf_len {
            self.buf_l = vec![0.0; buf_len];
            self.buf_r = vec![0.0; buf_len];
            self.buf_len = buf_len;
            self.write = 0;
        }
        self.grain_len = (GRAIN_SECS * samplerate).max(64.0);
        self.pos_a = 0.0;
        self.pos_b = self.grain_len * 0.5;
        self.update_pitch();
        self.update_filters();

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
        "Shimmer"
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Shimmer {
    /// Extra setters for the parameters that only this layer exposes.
    pub fn set_pitch_norm(&mut self, p: f64) {
        self.pitch_norm = p.clamp(0.0, 1.0);
        self.update_pitch();
    }

    pub fn set_feedback(&mut self, fb: f64) {
        self.feedback = fb.clamp(0.0, 0.95);
    }

    pub fn set_low_cut_hz(&mut self, hz: f64) {
        self.low_cut_hz = hz.clamp(20.0, self.samplerate * 0.45);
        self.update_filters();
    }

    pub fn set_high_cut_hz(&mut self, hz: f64) {
        self.high_cut_hz = hz.clamp(200.0, self.samplerate * 0.45);
        self.update_filters();
    }
}
