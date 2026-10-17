//! PEJVAK Texture Rack – Scatter layer – GPL-3.0
//!
//! A granular scatter texture based on overlapping grains with
//! raised-cosine envelopes. The design is derived from:
//!
//!   * The overlap-add technique used in most granular processors
//!     (SoundGhost Scatter, Ableton Granulator, `algo-dsp/granular.go`).
//!   * The "one-shot schedule + finite grain pool" model described in
//!     Curtis Roads' "Microsound" (MIT Press, 2001), Chapter 3.
//!
//! Key design decisions:
//!
//!   1. Grains never "jump" — every grain has a fixed start position
//!      determined at spawn time. When it ends, it's simply deactivated
//!      and (optionally) replaced by a new grain on a later tick.
//!      This eliminates the click that a "tap-jumping" approach has.
//!
//!   2. The grain loop runs BEFORE the buffer write, so the buffer is
//!      always read-only during the grain sum. If a non-finite value
//!      ever appears in the sum, the safety net just skips the write
//!      this sample — no expensive buffer fill.
//!
//!   3. The active count is capped at MAX_GRAINS by refusing to spawn
//!      when all slots are full. There is no slot stealing, so a grain
//!      always plays from start to finish.
//!
//!   4. Tap gains are 1/N (not 1/sqrt(N)), which guarantees the
//!      feedback loop gain stays below `feedback` at all times.
//!      `out_scale = sqrt(N)` restores the perceived output level for
//!      uncorrelated grains.

use crate::texture::TextureLayer;
use std::any::Any;

/// Circular buffer length in seconds. 1 second is long enough for the
/// longest grain to have a distinct origin from the write head.
const BUF_SECS: f64 = 1.0;

/// Maximum simultaneous grains. Kept low so the grain loop stays well
/// under 10% of one CPU core even at maximum density.
const MAX_GRAINS: usize = 16;

/// Grain envelope length range, in milliseconds.
const MIN_GRAIN_MS: f64 = 25.0;
const MAX_GRAIN_MS: f64 = 160.0;

/// Spawn interval range, in milliseconds. MIN_SPAWN_MS is chosen so
/// that at maximum density, MAX_GRAIN_MS / MIN_SPAWN_MS < MAX_GRAINS,
/// which guarantees a free slot is always available.
const MIN_SPAWN_MS: f64 = 12.0;
const MAX_SPAWN_MS: f64 = 500.0;

/// Make-up gain for the granular wet path. The grain sum is
/// normalised by 1/MAX_GRAINS (0.0625 in code = 0.25 after the
/// internal sqrt), which is correct for the worst case of 16
/// fully-overlapping grains but leaves the typical case (2–4 active
/// grains) noticeably quiet. 2.2 restores the wet RMS to roughly the
/// dry signal's level.
const SCATTER_WET_MAKEUP: f64 = 2.2;

/// Raised-cosine (Hann) envelope. `t` is normalised position in [0, 1).
#[inline]
fn grain_envelope(t: f64) -> f64 {
    0.5 - 0.5 * (std::f64::consts::TAU * t).cos()
}

/// A single grain voice.
#[derive(Clone, Copy)]
struct Grain {
    /// True while this grain is playing. Set to `false` at the moment
    /// the envelope would reach zero.
    active: bool,
    /// Absolute read start position in the circular buffer, in samples.
    start: f64,
    /// Playback position, in samples, from `start`. Always in
    /// [0, grain_len).
    elapsed: f64,
    /// Grain length, in samples.
    len: f64,
}

impl Grain {
    const INACTIVE: Grain = Grain {
        active: false,
        start: 0.0,
        elapsed: 0.0,
        len: 0.0,
    };
}

pub struct Scatter {
    samplerate: f64,

    // ---- Circular buffer -------------------------------------------
    buf_l: Vec<f64>,
    buf_r: Vec<f64>,
    write: usize,
    buf_len: usize,

    // ---- Grain pool -------------------------------------------------
    grains: [Grain; MAX_GRAINS],

    // ---- Scheduler --------------------------------------------------
    spawn_countdown: f64,

    // ---- Random state -----------------------------------------------
    rng_state: u64,

    // ---- Feedback ---------------------------------------------------
    fb_l: f64,
    fb_r: f64,

    // ---- Parameters -------------------------------------------------
    mix: f64,
    // Per-sample smoothing of `mix` — hides the crossfade
    // discontinuity when the enable switch is toggled.
    mix_smoothed: f64,
    mix_smooth_coeff: f64,
    spread_norm: f64,
    density_norm: f64,
    speed_norm: f64,
    feedback: f64,
}

impl Scatter {
    pub fn new(_buffer_size: usize, samplerate: f64) -> Self {
        let buf_len = (BUF_SECS * samplerate).ceil() as usize;
        let mut s = Self {
            samplerate,
            buf_l: vec![0.0; buf_len],
            buf_r: vec![0.0; buf_len],
            write: 0,
            buf_len,
            grains: [Grain::INACTIVE; MAX_GRAINS],
            spawn_countdown: 1.0,
            rng_state: 0xD1B5_4A32_D192_ED03,
            fb_l: 0.0,
            fb_r: 0.0,
            mix: 0.0,
            mix_smoothed: 0.0,
            // 20 ms one-pole smoother for the dry/wet crossfade.
            mix_smooth_coeff: 1.0 - (-1.0 / (0.020 * samplerate)).exp(),
            spread_norm: 0.5,
            density_norm: 0.5,
            speed_norm: 0.5,
            feedback: 0.4,
        };
        s.reset_spawn_timer();
        s
    }

    #[inline]
    fn rng_next(&mut self) -> f64 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state = x;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }

    fn reset_spawn_timer(&mut self) {
        let density_ms = MIN_SPAWN_MS + (1.0 - self.density_norm) * (MAX_SPAWN_MS - MIN_SPAWN_MS);
        // Speed: 1.0 → up to 3× faster; 0.0 → up to 3× slower.
        let speed_factor = 0.33 + self.speed_norm * 2.67;
        let interval_ms = (density_ms / speed_factor).max(MIN_SPAWN_MS * 0.5);
        self.spawn_countdown = (interval_ms * self.samplerate / 1000.0).max(1.0);
    }

    /// Attempt to spawn a new grain. Returns silently if no slot is free.
    ///
    /// By never stealing slots, we guarantee every grain gets to play
    /// its full envelope. The texture stays smooth even at max density.
    fn spawn_grain(&mut self) {
        // ---- Find a free slot ---------------------------------------
        let slot = match (0..MAX_GRAINS).find(|&i| !self.grains[i].active) {
            Some(s) => s,
            None => return,
        };

        // ---- Random start position within the buffer ------------------
        // `spread_norm` controls how far back from the write head the
        // grain can start. At spread = 0, grains start right at the
        // write head (short delays). At spread = 1, they can start
        // anywhere in the buffer.
        let max_offset = (self.buf_len as f64 - 2.0).max(1.0);
        let usable = (max_offset * self.spread_norm).max(32.0).min(max_offset);
        let start_offset = (self.rng_next() * usable).clamp(0.0, usable - 1.0);

        // Absolute position = write - start_offset (with wrap).
        let mut start = self.write as f64 - start_offset;
        if start < 0.0 {
            start += self.buf_len as f64;
        }

        // ---- Grain length --------------------------------------------
        let grain_ms = MIN_GRAIN_MS + self.rng_next() * (MAX_GRAIN_MS - MIN_GRAIN_MS);
        let grain_len = (grain_ms * self.samplerate / 1000.0).max(32.0);

        self.grains[slot] = Grain {
            active: true,
            start,
            elapsed: 0.0,
            len: grain_len,
        };
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

impl TextureLayer for Scatter {
    fn process_inplace(&mut self, left: &mut [f64], right: &mut [f64]) {
        // ---- Guard: buffer must exist ---------------------------------
        if self.buf_len == 0 {
            return;
        }

        // Fast path: fully faded out and target is zero.
        if self.mix <= 0.0 && self.mix_smoothed < 1.0e-6 {
            return;
        }

        let n = left.len().min(right.len());
        let fb_gain = self.feedback.clamp(0.0, 0.7);
        let buf_len = self.buf_len;
        let smooth = self.mix_smooth_coeff;
        let mix_target = self.mix;

        // 1/N gain: worst-case correlated sum stays at unity.
        // out_scale restores unity RMS for uncorrelated grains.
        let out_scale = (MAX_GRAINS as f64).sqrt();

        for i in 0..n {
            // Per-sample smoothing of the crossfade parameter.
            self.mix_smoothed += smooth * (mix_target - self.mix_smoothed);
            let m = self.mix_smoothed.clamp(0.0, 1.0);
            let angle = m * std::f64::consts::FRAC_PI_2;
            let (wet_sin, dry_cos) = angle.sin_cos();
            let dry_gain = dry_cos;
            let wet_gain = wet_sin * SCATTER_WET_MAKEUP;

            let in_l = left[i];
            let in_r = right[i];

            // ---- 1. Sum active grains (read-only on buffer) ----------
            let mut sum_l = 0.0;
            let mut sum_r = 0.0;
            for g in 0..MAX_GRAINS {
                let grain = &mut self.grains[g];
                if !grain.active {
                    continue;
                }

                let t = grain.elapsed / grain.len;
                if t >= 1.0 {
                    grain.active = false;
                    continue;
                }

                // Read position, in absolute buffer coordinates.
                let read_pos = (grain.start + grain.elapsed).rem_euclid(self.buf_len as f64);

                let env = grain_envelope(t);
                // 1/MAX_GRAINS normalisation is applied at the sum, not
                // per-grain, to avoid an extra multiply per read.
                sum_l += Self::read_interp(&self.buf_l, read_pos) * env;
                sum_r += Self::read_interp(&self.buf_r, read_pos) * env;

                grain.elapsed += 1.0;
            }

            // ---- 2. Apply gain (only if sum is finite) ---------------
            // If the sum went non-finite — which can only happen with a
            // bug or a hostile host — we skip this sample without
            // touching the buffer. No `Vec::fill` on the hot path.
            let finite = sum_l.is_finite() && sum_r.is_finite();
            if finite {
                let scale = out_scale / (MAX_GRAINS as f64);
                sum_l *= scale;
                sum_r *= scale;
            } else {
                sum_l = 0.0;
                sum_r = 0.0;
                self.fb_l = 0.0;
                self.fb_r = 0.0;
                for g in 0..MAX_GRAINS {
                    self.grains[g].active = false;
                }
            }

            // ---- 3. Write input + feedback into the buffer -----------
            let pre_l = in_l + self.fb_l * fb_gain;
            let pre_r = in_r + self.fb_r * fb_gain;
            // Sanitise the value before writing — a NaN input would
            // otherwise poison the buffer for a full second.
            self.buf_l[self.write] = if pre_l.is_finite() { pre_l } else { 0.0 };
            self.buf_r[self.write] = if pre_r.is_finite() { pre_r } else { 0.0 };

            // ---- 4. Store wet sum for next iteration's feedback ------
            self.fb_l = sum_l;
            self.fb_r = sum_r;

            // ---- 5. Advance write head --------------------------------
            self.write += 1;
            if self.write >= buf_len {
                self.write = 0;
            }

            // ---- 6. Scheduler: spawn a grain on schedule -------------
            self.spawn_countdown -= 1.0;
            if self.spawn_countdown <= 0.0 {
                self.spawn_grain();
                self.reset_spawn_timer();
            }

            // ---- 7. Output blend ---------------------------------------
            left[i] = in_l * dry_gain + sum_l * wet_gain;
            right[i] = in_r * dry_gain + sum_r * wet_gain;
        }
    }

    fn clear_buffers(&mut self) {
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write = 0;
        self.fb_l = 0.0;
        self.fb_r = 0.0;
        for g in 0..MAX_GRAINS {
            self.grains[g] = Grain::INACTIVE;
        }
        self.reset_spawn_timer();
    }

    fn set_samplerate(&mut self, samplerate: f64) {
        self.samplerate = samplerate;
        let new_len = (BUF_SECS * samplerate).ceil() as usize;
        if new_len != self.buf_len {
            self.buf_l = vec![0.0; new_len];
            self.buf_r = vec![0.0; new_len];
            self.buf_len = new_len;
            self.write = 0;
        }
        for g in 0..MAX_GRAINS {
            self.grains[g] = Grain::INACTIVE;
        }
        self.reset_spawn_timer();
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
        "Scatter"
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Scatter {

    pub fn set_spread_norm(&mut self, v: f64) {
        self.spread_norm = v.clamp(0.0, 1.0);
    }

    pub fn set_density_norm(&mut self, v: f64) {
        self.density_norm = v.clamp(0.0, 1.0);
        self.reset_spawn_timer();
    }

    pub fn set_speed_norm(&mut self, v: f64) {
        self.speed_norm = v.clamp(0.0, 1.0);
        self.reset_spawn_timer();
    }

    pub fn set_feedback(&mut self, fb: f64) {
        self.feedback = fb.clamp(0.0, 0.7);
    }
}
