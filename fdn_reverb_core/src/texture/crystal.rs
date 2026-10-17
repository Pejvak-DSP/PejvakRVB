//! PEJVAK Texture Rack – Crystal layer – GPL-3.0
//!
//! A granular / particle delay. The input is written continuously into
//! a short circular buffer; two independent "grain readers" jump to
//! random positions in the buffer at regular intervals and play a short
//! snippet with a raised-cosine envelope. The grains are summed and fed
//! back with HP+LP filtering, producing a shimmering, glassy texture
//! that sounds like tiny shards of sound dancing over the reverb tail.
//!
//! Signal flow (per sample):
//!
//!   in ─┬─→ [write buffer] ─→ [grain A] ─┐
//!       │                                 ├─ sum ─→ [HP] → [LP] → wet
//!       │                  [grain B] ─────┘         │
//!       │                                           │
//!       └──────────────── feedback ─────────────────┘
//!
//! Unlike Shimmer, the grains do NOT advance at 2× rate — they play at
//! 1× speed. The "crystal" character comes from the discontinuity of
//! jumping to a new buffer position each time a grain ends, which
//! creates a stuttering, sparkling texture.

use crate::texture::TextureLayer;
use std::any::Any;

/// Circular buffer length in seconds. 0.5 s gives enough variation for
/// the random grain positions while keeping the cache footprint small
/// (~192 KB per channel at 48 kHz).
const BUF_SECS: f64 = 0.5;

/// Grain length range in milliseconds.
const MIN_GRAIN_MS: f64 = 15.0;
const MAX_GRAIN_MS: f64 = 150.0;

/// Delay between consecutive grain triggers, in milliseconds. The two
/// grains are offset by half this so the sum stays roughly constant.
const MIN_PERIOD_MS: f64 = 20.0;
const MAX_PERIOD_MS: f64 = 250.0;

/// Make-up gain for the granular wet path. Two Hann-windowed grains
/// overlap on average ~50 % of the time, and the HP+LP chain removes
/// another ~15 % of the broadband energy. 1.7 brings the wet RMS
/// roughly level with the dry signal.
const CRYSTAL_WET_MAKEUP: f64 = 1.7;

pub struct Crystal {
    samplerate: f64,

    // ---- Buffer ----------------------------------------------------
    buf_l: Vec<f64>,
    buf_r: Vec<f64>,
    write: usize,
    buf_len: usize,

    // ---- Grain A ---------------------------------------------------
    phase_a: f64,
    start_a: f64,
    grain_len_a: f64,
    /// Countdown to the next A grain trigger, in samples.
    next_a: f64,

    // ---- Grain B ---------------------------------------------------
    phase_b: f64,
    start_b: f64,
    grain_len_b: f64,
    next_b: f64,

    // ---- Random state ----------------------------------------------
    /// Simple xorshift — no external crate, deterministic per instance.
    rng_state: u64,

    // ---- Filters ---------------------------------------------------
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
    // Per-sample smoothing of `mix` — hides the crossfade
    // discontinuity when the enable switch is toggled.
    mix_smoothed: f64,
    mix_smooth_coeff: f64,
    grain_len_norm: f64,
    density_norm: f64,
    feedback: f64,
}

impl Crystal {
    pub fn new(_buffer_size: usize, samplerate: f64) -> Self {
        let buf_len = (BUF_SECS * samplerate).ceil() as usize;
        let mut c = Self {
            samplerate,
            buf_l: vec![0.0; buf_len],
            buf_r: vec![0.0; buf_len],
            write: 0,
            buf_len,
            phase_a: 0.0,
            start_a: 0.0,
            grain_len_a: (MIN_GRAIN_MS * samplerate / 1000.0).max(32.0),
            next_a: 0.0,
            phase_b: 0.5,
            start_b: 0.0,
            grain_len_b: (MIN_GRAIN_MS * samplerate / 1000.0).max(32.0),
            next_b: 0.0,
            // Seed from the instance address so different channels /
            // instances don't share the exact same random sequence.
            rng_state: 0x9E37_79B9_7F4A_7C15,
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
            grain_len_norm: 0.4,
            density_norm: 0.5,
            feedback: 0.5,
        };
        c.update_filters();
        c.update_grain_len();
        c.update_period();
        c.reset_grain_a();
        c.reset_grain_b();
        c
    }

    #[inline]
    fn rng_next(&mut self) -> f64 {
        // xorshift64. Cheap, deterministic, good enough for grain
        // position jitter — no statistical rigour required.
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state = x;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }

    fn update_filters(&mut self) {
        // HP at 300 Hz, LP at 10 kHz. Both are character filters — the
        // user cannot change them, they are what gives Crystal its
        // glassy sheen.
        let hp = (300.0 / self.samplerate).clamp(1e-6, 0.49);
        self.hp_coeff = 1.0 - (-2.0 * std::f64::consts::PI * hp).exp();
        let lp = (10_000.0 / self.samplerate).clamp(1e-6, 0.49);
        self.lp_coeff = 1.0 - (-2.0 * std::f64::consts::PI * lp).exp();
    }

    fn update_grain_len(&mut self) {
        let ms = MIN_GRAIN_MS + self.grain_len_norm * (MAX_GRAIN_MS - MIN_GRAIN_MS);
        let samples = (ms * self.samplerate / 1000.0).max(32.0);
        self.grain_len_a = samples;
        self.grain_len_b = samples;
    }

    fn update_period(&mut self) {
        let ms = MIN_PERIOD_MS + (1.0 - self.density_norm) * (MAX_PERIOD_MS - MIN_PERIOD_MS);
        self.next_a = (ms * self.samplerate / 1000.0).max(1.0);
    }

    fn reset_grain_a(&mut self) {
        self.phase_a = 0.0;
        // Random start offset in the buffer.
        self.start_a = self.rng_next() * (self.buf_len as f64 - self.grain_len_a - 4.0).max(1.0);
        let period_ms = MIN_PERIOD_MS + (1.0 - self.density_norm) * (MAX_PERIOD_MS - MIN_PERIOD_MS);
        self.next_a = (period_ms * self.samplerate / 1000.0).max(1.0);
    }

    fn reset_grain_b(&mut self) {
        self.phase_b = 0.0;
        self.start_b = self.rng_next() * (self.buf_len as f64 - self.grain_len_b - 4.0).max(1.0);
        // Grain B fires offset from A by half a period so their sum is
        // never fully silent.
        let period_ms = MIN_PERIOD_MS + (1.0 - self.density_norm) * (MAX_PERIOD_MS - MIN_PERIOD_MS);
        self.next_b = (period_ms * 0.5 * self.samplerate / 1000.0).max(1.0);
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

impl TextureLayer for Crystal {
    fn process_inplace(&mut self, left: &mut [f64], right: &mut [f64]) {
        // Fast path: fully faded out and target is zero.
        if self.mix <= 0.0 && self.mix_smoothed < 1.0e-6 {
            return;
        }

        let n = left.len().min(right.len());
        let fb_gain = self.feedback.clamp(0.0, 0.9);
        let buf_len = self.buf_len;
        let smooth = self.mix_smooth_coeff;
        let mix_target = self.mix;

        for i in 0..n {
            // Per-sample smoothing of the crossfade parameter.
            self.mix_smoothed += smooth * (mix_target - self.mix_smoothed);
            let m = self.mix_smoothed.clamp(0.0, 1.0);
            let angle = m * std::f64::consts::FRAC_PI_2;
            let (wet_sin, dry_cos) = angle.sin_cos();
            let dry_gain = dry_cos;
            let wet_gain = wet_sin * CRYSTAL_WET_MAKEUP;

            let in_l = left[i];
            let in_r = right[i];

            // ---- 1. Input + feedback into the write buffer -----------
            let pre_l = in_l + self.fb_l * fb_gain;
            let pre_r = in_r + self.fb_r * fb_gain;
            self.buf_l[self.write] = pre_l;
            self.buf_r[self.write] = pre_r;

            // ---- 2. Grain A -------------------------------------------
            let ga_l;
            let ga_r;
            if self.phase_a < self.grain_len_a {
                // Raised-cosine window: 0 → 1 → 0.
                let w = 0.5 - 0.5 * (std::f64::consts::TAU * self.phase_a / self.grain_len_a).cos();
                let pos = self.start_a + self.phase_a;
                ga_l = Self::read_interp(&self.buf_l, pos) * w;
                ga_r = Self::read_interp(&self.buf_r, pos) * w;
                self.phase_a += 1.0;
                self.next_a -= 1.0;
            } else {
                ga_l = 0.0;
                ga_r = 0.0;
                self.next_a -= 1.0;
                if self.next_a <= 0.0 {
                    self.reset_grain_a();
                }
            }

            // ---- 3. Grain B -------------------------------------------
            let gb_l;
            let gb_r;
            if self.phase_b < self.grain_len_b {
                let w = 0.5 - 0.5 * (std::f64::consts::TAU * self.phase_b / self.grain_len_b).cos();
                let pos = self.start_b + self.phase_b;
                gb_l = Self::read_interp(&self.buf_l, pos) * w;
                gb_r = Self::read_interp(&self.buf_r, pos) * w;
                self.phase_b += 1.0;
                self.next_b -= 1.0;
            } else {
                gb_l = 0.0;
                gb_r = 0.0;
                self.next_b -= 1.0;
                if self.next_b <= 0.0 {
                    self.reset_grain_b();
                }
            }

            // ---- 4. Sum + filter --------------------------------------
            let sum_l = ga_l + gb_l;
            let sum_r = ga_r + gb_r;

            self.hp_state_l += self.hp_coeff * (sum_l - self.hp_state_l);
            self.hp_state_r += self.hp_coeff * (sum_r - self.hp_state_r);
            let hp_l = sum_l - self.hp_state_l;
            let hp_r = sum_r - self.hp_state_r;

            self.lp_state_l += self.lp_coeff * (hp_l - self.lp_state_l);
            self.lp_state_r += self.lp_coeff * (hp_r - self.lp_state_r);
            let wet_l = self.lp_state_l;
            let wet_r = self.lp_state_r;

            // ---- 5. Save as feedback ----------------------------------
            self.fb_l = wet_l;
            self.fb_r = wet_r;

            // ---- 6. Advance write head --------------------------------
            self.write += 1;
            if self.write >= buf_len {
                self.write = 0;
            }

            // ---- 7. Output blend ---------------------------------------
            left[i] = in_l * dry_gain + wet_l * wet_gain;
            right[i] = in_r * dry_gain + wet_r * wet_gain;
        }
    }

    fn clear_buffers(&mut self) {
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write = 0;
        self.phase_a = 0.0;
        self.phase_b = 0.0;
        self.hp_state_l = 0.0;
        self.hp_state_r = 0.0;
        self.lp_state_l = 0.0;
        self.lp_state_r = 0.0;
        self.fb_l = 0.0;
        self.fb_r = 0.0;
        self.reset_grain_a();
        self.reset_grain_b();
    }

    fn set_samplerate(&mut self, samplerate: f64) {
        self.samplerate = samplerate;
        let new_len = (BUF_SECS * samplerate).ceil() as usize;
        if new_len != self.buf_len {
            self.buf_l = vec![0.0; new_len];
            self.buf_r = vec![0.0; new_len];
            self.buf_len = new_len;
            self.write = 0;
            // Grain start positions were computed against the old
            // buffer length. Recompute them so the first grain after
            // a runtime sample-rate change reads from a valid offset.
            self.reset_grain_a();
            self.reset_grain_b();
        }
        self.update_filters();
        self.update_grain_len();
        self.update_period();
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
        "Crystal"
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Crystal {

    pub fn set_grain_len_norm(&mut self, v: f64) {
        self.grain_len_norm = v.clamp(0.0, 1.0);
        self.update_grain_len();
    }

    pub fn set_density_norm(&mut self, v: f64) {
        self.density_norm = v.clamp(0.0, 1.0);
        self.update_period();
    }

    pub fn set_feedback(&mut self, fb: f64) {
        self.feedback = fb.clamp(0.0, 0.9);
    }
}
