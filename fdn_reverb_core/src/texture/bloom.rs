//! PEJVAK Texture Rack – Bloom layer – GPL-3.0
//!
//! A rising, diffuse reverb texture inspired by the Alesis Midiverb II
//! "Bloom" algorithm (Keith Barr, 1987) and its modern interpretations
//! (ValhallaShimmer, UVI Bloom). The defining characteristic is a slow
//! attack: instead of starting immediately, the wet signal swells into
//! full amplitude over 50–2000 ms, creating a "reverse reverb" or
//! "blooming" character.
//!
//! Reference:
//!   * ValhallaShimmer Notes (Sean Costello): "values around 0.5 to
//!     0.618 will cause the reverb to slowly fade in" — the allpass
//!     coefficient controls the attack time.
//!   * UVI Bloom: "cleverly offset reverb engines" create the Rise
//!     effect through time-offset parallel diffusion chains.
//!
//! Signal flow (per sample):
//!
//!   in ─→ [Envelope Follower] ─→ [Bloom Envelope Generator]
//!   │                                    │
//!   └─→ [Allpass Diffusion × 4] ─→ [Delay Line] ─→ × envelope ─→ wet
//!         ↑                                  │
//!         └──────── feedback ────────────────┘
//!
//! The allpass diffusion cascade uses coefficients around 0.5, which
//! produces a delayed onset and long attack. The bloom envelope rises
//! slowly on note onset and decays smoothly afterwards, shaped by the
//! `Rise` parameter.

use crate::texture::TextureLayer;
use std::any::Any;

/// Number of allpass stages in the diffusion cascade. Four is the
/// minimum for a smooth, non-grainy diffusion character.
const AP_STAGES: usize = 4;

/// Allpass delay times, in milliseconds. These are relatively long
/// (20–60 ms) compared to typical early-diffusion allpasses (1–10 ms),
/// which is what gives Bloom its "slowing down time" quality.
const AP_DELAYS_MS: [f64; AP_STAGES] = [23.0, 37.0, 51.0, 67.0];

/// Allpass coefficient. 0.5 is the value Keith Barr used in the
/// original Midiverb; it produces the slow attack that defines Bloom.
const AP_COEFF: f64 = 0.5;

/// Main reverb delay line length, in milliseconds.
const MIN_DELAY_MS: f64 = 60.0;
const MAX_DELAY_MS: f64 = 250.0;

/// Envelope attack range: 50 ms to 2000 ms. The Rise parameter maps
/// linearly into this range (inverted: higher Rise = slower attack).
const MIN_ATTACK_MS: f64 = 50.0;
const MAX_ATTACK_MS: f64 = 2000.0;

/// Envelope release range: 200 ms to 5000 ms.
const MIN_RELEASE_MS: f64 = 200.0;
const MAX_RELEASE_MS: f64 = 5000.0;

/// Maximum feedback gain. With the 1/AP_STAGES normalisation this
/// keeps the loop gain well below unity at all times.
const MAX_FEEDBACK: f64 = 0.85;

/// Make-up gain for the bloom wet path. The one-pole low-pass and the
/// slowly-rising bloom envelope both attenuate the wet signal; 1.5
/// brings the steady-state wet RMS up to roughly the dry signal's.
const BLOOM_WET_MAKEUP: f64 = 1.5;

pub struct Bloom {
    samplerate: f64,

    // ---- Envelope follower (tracks input amplitude) -----------------
    env_in: f64,
    env_attack_in: f64,
    env_release_in: f64,

    // ---- Bloom envelope (the rising shape) --------------------------
    bloom_env: f64,
    bloom_attack: f64,
    bloom_release: f64,

    // ---- Allpass diffusion cascade (per channel) --------------------
    ap_bufs_l: Vec<Vec<f64>>,
    ap_bufs_r: Vec<Vec<f64>>,
    ap_idx: Vec<usize>,
    ap_delays: Vec<usize>,

    // ---- Main delay line (per channel) ------------------------------
    delay_buf_l: Vec<f64>,
    delay_buf_r: Vec<f64>,
    delay_write: usize,
    delay_len: usize,
    delay_read: usize,

    // ---- Feedback ---------------------------------------------------
    fb_l: f64,
    fb_r: f64,

    // ---- Tone filter (one-pole LP) ----------------------------------
    lp_state_l: f64,
    lp_state_r: f64,
    lp_coeff: f64,

    // ---- Parameters -------------------------------------------------
    mix: f64,
    // Per-sample smoothing of `mix` — hides the crossfade
    // discontinuity when the enable switch is toggled.
    mix_smoothed: f64,
    mix_smooth_coeff: f64,
    rise_norm: f64,
    size_norm: f64,
    feedback: f64,
    tone_norm: f64,
}

impl Bloom {
    pub fn new(_buffer_size: usize, samplerate: f64) -> Self {
        let ap_delays: Vec<usize> = AP_DELAYS_MS
            .iter()
            .map(|ms| ((ms * samplerate / 1000.0).ceil() as usize).max(4))
            .collect();

        let ap_bufs_l: Vec<Vec<f64>> = ap_delays.iter().map(|&d| vec![0.0; d]).collect();
        let ap_bufs_r: Vec<Vec<f64>> = ap_delays.iter().map(|&d| vec![0.0; d]).collect();

        let delay_len = (MAX_DELAY_MS * samplerate / 1000.0).ceil() as usize;

        let mut b = Self {
            samplerate,
            env_in: 0.0,
            env_attack_in: 0.0,
            env_release_in: 0.0,
            bloom_env: 0.0,
            bloom_attack: 0.0,
            bloom_release: 0.0,
            ap_bufs_l,
            ap_bufs_r,
            ap_idx: vec![0; AP_STAGES],
            ap_delays,
            delay_buf_l: vec![0.0; delay_len],
            delay_buf_r: vec![0.0; delay_len],
            delay_write: 0,
            delay_len,
            delay_read: 0,
            fb_l: 0.0,
            fb_r: 0.0,
            lp_state_l: 0.0,
            lp_state_r: 0.0,
            lp_coeff: 0.0,
            mix: 0.0,
            mix_smoothed: 0.0,
            // 20 ms one-pole smoother for the dry/wet crossfade.
            mix_smooth_coeff: 1.0 - (-1.0 / (0.020 * samplerate)).exp(),
            rise_norm: 0.5,
            size_norm: 0.5,
            feedback: 0.5,
            tone_norm: 0.5,
        };
        b.update_envelope_coeffs();
        b.update_delay();
        b.update_tone();
        b
    }

    fn update_envelope_coeffs(&mut self) {
        // Input envelope follower: fast attack, moderate release.
        // This tracks the input amplitude so the bloom envelope can
        // trigger on note onset.
        let in_attack_ms = 5.0;
        let in_release_ms = 100.0;
        self.env_attack_in = 1.0 - (-1.0 / (in_attack_ms * self.samplerate / 1000.0)).exp();
        self.env_release_in = 1.0 - (-1.0 / (in_release_ms * self.samplerate / 1000.0)).exp();

        // Bloom envelope: slow attack (Rise) and slow release.
        // The Rise knob is inverted — 0.0 = fastest (50 ms),
        // 1.0 = slowest (2000 ms).
        let attack_ms = MIN_ATTACK_MS + (1.0 - self.rise_norm) * (MAX_ATTACK_MS - MIN_ATTACK_MS);
        let release_ms = MIN_RELEASE_MS + self.size_norm * (MAX_RELEASE_MS - MIN_RELEASE_MS);
        self.bloom_attack = 1.0 - (-1.0 / (attack_ms * self.samplerate / 1000.0)).exp();
        self.bloom_release = 1.0 - (-1.0 / (release_ms * self.samplerate / 1000.0)).exp();
    }

    fn update_delay(&mut self) {
        let ms = MIN_DELAY_MS + self.size_norm * (MAX_DELAY_MS - MIN_DELAY_MS);
        self.delay_read = (ms * self.samplerate / 1000.0).ceil() as usize;
        if self.delay_read >= self.delay_len {
            self.delay_read = self.delay_len - 1;
        }
    }

    fn update_tone(&mut self) {
        // Tone: 0 = dark (2 kHz), 1 = bright (12 kHz).
        let hz = 2000.0 + self.tone_norm * (12000.0 - 2000.0);
        self.lp_coeff = 1.0 - (-2.0 * std::f64::consts::PI * hz / self.samplerate).exp();
    }

    #[inline]
    fn process_allpass(buf: &mut [f64], idx: &mut usize, input: f64, coeff: f64) -> f64 {
        let len = buf.len();
        let delayed = buf[*idx];
        let out = -coeff * input + delayed;
        buf[*idx] = input + coeff * out;
        *idx += 1;
        if *idx >= len {
            *idx = 0;
        }
        out
    }
}

impl TextureLayer for Bloom {
    fn process_inplace(&mut self, left: &mut [f64], right: &mut [f64]) {
        // Fast path: fully faded out and target is zero.
        if self.mix <= 0.0 && self.mix_smoothed < 1.0e-6 {
            return;
        }

        let n = left.len().min(right.len());
        let fb_gain = self.feedback.clamp(0.0, MAX_FEEDBACK);
        let delay_len = self.delay_len;
        let smooth = self.mix_smooth_coeff;
        let mix_target = self.mix;

        for i in 0..n {
            // Per-sample smoothing of the crossfade parameter.
            self.mix_smoothed += smooth * (mix_target - self.mix_smoothed);
            let m = self.mix_smoothed.clamp(0.0, 1.0);
            let angle = m * std::f64::consts::FRAC_PI_2;
            let (wet_sin, dry_cos) = angle.sin_cos();
            let dry_gain = dry_cos;
            let wet_gain = wet_sin * BLOOM_WET_MAKEUP;

            let in_l = left[i];
            let in_r = right[i];

            // ---- 1. Envelope follower on the input -------------------
            let in_abs = (in_l.abs() + in_r.abs()) * 0.5;
            if in_abs > self.env_in {
                self.env_in += self.env_attack_in * (in_abs - self.env_in);
            } else {
                self.env_in += self.env_release_in * (in_abs - self.env_in);
            }

            // ---- 2. Bloom envelope generator -------------------------
            // The bloom envelope tracks a *target* derived from the
            // input envelope. When the input is present, the target is
            // high and the bloom envelope rises slowly toward it. When
            // the input stops, the envelope falls back toward zero.
            let target = (self.env_in * 4.0).min(1.0);
            if target > self.bloom_env {
                self.bloom_env += self.bloom_attack * (target - self.bloom_env);
            } else {
                self.bloom_env += self.bloom_release * (target - self.bloom_env);
            }

            // ---- 3. Allpass diffusion cascade ------------------------
            let mut diff_l = in_l + self.fb_l * fb_gain;
            let mut diff_r = in_r + self.fb_r * fb_gain;

            for s in 0..AP_STAGES {
                diff_l = Self::process_allpass(
                    &mut self.ap_bufs_l[s],
                    &mut self.ap_idx[s],
                    diff_l,
                    AP_COEFF,
                );
                diff_r = Self::process_allpass(
                    &mut self.ap_bufs_r[s],
                    &mut self.ap_idx[s],
                    diff_r,
                    AP_COEFF,
                );
            }

            // ---- 4. Main delay line ----------------------------------
            self.delay_buf_l[self.delay_write] = diff_l;
            self.delay_buf_r[self.delay_write] = diff_r;

            let read_idx = if self.delay_write >= self.delay_read {
                self.delay_write - self.delay_read
            } else {
                delay_len + self.delay_write - self.delay_read
            };

            let delayed_l = self.delay_buf_l[read_idx];
            let delayed_r = self.delay_buf_r[read_idx];

            self.delay_write += 1;
            if self.delay_write >= delay_len {
                self.delay_write = 0;
            }

            // ---- 5. Store feedback for next iteration ----------------
            self.fb_l = delayed_l;
            self.fb_r = delayed_r;

            // ---- 6. Tone filter (one-pole LP) ------------------------
            self.lp_state_l += self.lp_coeff * (delayed_l - self.lp_state_l);
            self.lp_state_r += self.lp_coeff * (delayed_r - self.lp_state_r);
            let filtered_l = self.lp_state_l;
            let filtered_r = self.lp_state_r;

            // ---- 7. Apply bloom envelope ------------------------------
            // The envelope is what gives Bloom its rising character.
            // A slow attack means the wet signal swells into full
            // amplitude over hundreds of milliseconds.
            let wet_l = filtered_l * self.bloom_env;
            let wet_r = filtered_r * self.bloom_env;

            // ---- 8. Output blend --------------------------------------
            left[i] = in_l * dry_gain + wet_l * wet_gain;
            right[i] = in_r * dry_gain + wet_r * wet_gain;
        }
    }

    fn clear_buffers(&mut self) {
        for buf in &mut self.ap_bufs_l {
            buf.fill(0.0);
        }
        for buf in &mut self.ap_bufs_r {
            buf.fill(0.0);
        }
        self.delay_buf_l.fill(0.0);
        self.delay_buf_r.fill(0.0);
        self.ap_idx.iter_mut().for_each(|i| *i = 0);
        self.delay_write = 0;
        self.fb_l = 0.0;
        self.fb_r = 0.0;
        self.env_in = 0.0;
        self.bloom_env = 0.0;
        self.lp_state_l = 0.0;
        self.lp_state_r = 0.0;
    }

    fn set_samplerate(&mut self, samplerate: f64) {
        self.samplerate = samplerate;
        let ap_delays: Vec<usize> = AP_DELAYS_MS
            .iter()
            .map(|ms| ((ms * samplerate / 1000.0).ceil() as usize).max(4))
            .collect();
        self.ap_bufs_l = ap_delays.iter().map(|&d| vec![0.0; d]).collect();
        self.ap_bufs_r = ap_delays.iter().map(|&d| vec![0.0; d]).collect();
        self.ap_delays = ap_delays;
        self.ap_idx = vec![0; AP_STAGES];

        let new_len = (MAX_DELAY_MS * samplerate / 1000.0).ceil() as usize;
        if new_len != self.delay_len {
            self.delay_buf_l = vec![0.0; new_len];
            self.delay_buf_r = vec![0.0; new_len];
            self.delay_len = new_len;
            self.delay_write = 0;
        }
        self.update_envelope_coeffs();
        self.update_delay();
        self.update_tone();
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
        "Bloom"
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Bloom {

    pub fn set_rise_norm(&mut self, v: f64) {
        self.rise_norm = v.clamp(0.0, 1.0);
        self.update_envelope_coeffs();
    }

    pub fn set_size_norm(&mut self, v: f64) {
        self.size_norm = v.clamp(0.0, 1.0);
        self.update_envelope_coeffs();
        self.update_delay();
    }

    pub fn set_feedback(&mut self, fb: f64) {
        self.feedback = fb.clamp(0.0, MAX_FEEDBACK);
    }

    pub fn set_tone_norm(&mut self, v: f64) {
        self.tone_norm = v.clamp(0.0, 1.0);
        self.update_tone();
    }
}
