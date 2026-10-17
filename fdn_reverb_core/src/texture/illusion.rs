//! PEJVAK – ILLUSION layer – GPL-3.0
//!
//! A granular cloud with pitched overtones and a musical envelope
//! follower.
//!
//! # Anti-click design
//!
//! Granular engines click for well-known reasons. ILLUSION addresses
//! every one of them:
//!
//!   1. **Grain length capped by pitch rate.** A pitch-shifted grain
//!      reads faster (rate > 1) than the write head, so it eventually
//!      catches up and reads *ahead* of what's been written. The
//!      classic fix — used by every serious pitch-shifter — is to
//!      shorten the grain when the ratio is large. The cap here is
//!      `(buf_len * 0.45) / max(1, rate)`.
//!   2. **Independent smoothstep fade-in / fade-out** on every grain,
//!      so the first derivative is zero at both ends.
//!   3. **Catmull-Rom cubic read** on the grain position — no
//!      linear-interpolation noise at large pitch ratios.
//!   4. **DC blocker** on the grain sum, killing the slow drift that
//!      a long feedback tail accumulates.
//!   5. **Soft clip** (rational tanh) on the feedback path, with a
//!      hard safety limit of ±4 on what enters the capture buffer.
//!   6. **Output smoothing** at 10 kHz to erase the last residual
//!      discontinuity from grain overlap.
//!   7. **Denormal flush** below 1e-30 after long silences.
//!   8. **16 grain voices** so nothing is silently dropped at high
//!      clock rates.

use std::f64::consts::TAU;

const BUF_SECS: f64 = 2.0;
const MAX_GRAINS: usize = 16;
const MIN_GRAIN_MS: f64 = 30.0;
const MAX_GRAIN_MS: f64 = 300.0;
const MIN_RATE_HZ: f64 = 0.5;
const MAX_RATE_HZ: f64 = 8.0;
const WET_MAKEUP: f64 = 1.3;
const MIN_TONE_HZ: f64 = 800.0;
const MAX_TONE_HZ: f64 = 18_000.0;

const GRAIN_FADE_IN_MS: f64 = 8.0;
const GRAIN_FADE_OUT_MS: f64 = 20.0;

const DENORM: f64 = 1.0e-30;
const BUF_ENTRY_LIMIT: f64 = 4.0;

const SEQUENCE: [i32; 16] = [0, 7, 3, 12, 5, 15, 10, 19, 7, 3, 12, 5, 15, 7, 0, 0];

#[derive(Clone, Copy)]
struct Grain {
    active: bool,
    read_pos: f64,
    rate: f64,
    elapsed: f64,
    length: f64,
    pan: f64,
    fade_in: f64,
    fade_out: f64,
}

impl Grain {
    const INACTIVE: Grain = Grain {
        active: false,
        read_pos: 0.0,
        rate: 1.0,
        elapsed: 0.0,
        length: 0.0,
        pan: 0.0,
        fade_in: 0.0,
        fade_out: 0.0,
    };
}

pub struct Chime {
    samplerate: f64,

    buf_l: Vec<f64>,
    buf_r: Vec<f64>,
    write: usize,
    buf_len: usize,

    clock_phase: f64,
    clock_rate: f64,
    current_step: usize,

    grains: [Grain; MAX_GRAINS],

    fb_l: f64,
    fb_r: f64,
    rng_state: u64,

    dc_x_l: f64,
    dc_y_l: f64,
    dc_x_r: f64,
    dc_y_r: f64,

    lp_state_l: f64,
    lp_state_r: f64,
    lp_coeff: f64,

    out_smooth_l: f64,
    out_smooth_r: f64,
    out_smooth_coeff: f64,

    /// Smoothed `spark`, advanced per sample. Prevents the dry/wet
    /// crossfade from stepping when the enable switch toggles.
    spark_smoothed: f64,
    spark_smooth_coeff: f64,

    // Parameters.
    spark: f64,
    still: f64,
    aura_semis: f64,
    veil: f64,
    wide: f64,
    tint: f64,
    rate_norm: f64,
    grain_norm: f64,
}

impl Chime {
    pub fn new(samplerate: f64) -> Self {
        let buf_len = (BUF_SECS * samplerate).ceil() as usize;

        let smooth_hz = 10_000.0_f64.min(samplerate * 0.45);
        let out_smooth_coeff = 1.0 - (-TAU * smooth_hz / samplerate).exp();

        let mut c = Self {
            samplerate,
            buf_l: vec![0.0; buf_len],
            buf_r: vec![0.0; buf_len],
            write: 0,
            buf_len,
            clock_phase: 0.0,
            clock_rate: 2.0,
            current_step: 0,
            grains: [Grain::INACTIVE; MAX_GRAINS],
            fb_l: 0.0,
            fb_r: 0.0,
            rng_state: 0xC0FF_EE00_1234_5678,
            dc_x_l: 0.0,
            dc_y_l: 0.0,
            dc_x_r: 0.0,
            dc_y_r: 0.0,
            lp_state_l: 0.0,
            lp_state_r: 0.0,
            lp_coeff: 1.0,
            out_smooth_l: 0.0,
            out_smooth_r: 0.0,
            out_smooth_coeff,
            spark_smoothed: 0.0,
            // 20 ms one-pole. Slow enough to hide the crossfade
            // discontinuity, fast enough that the effect still feels
            // instantaneous.
            spark_smooth_coeff: 1.0 - (-1.0 / (0.020 * samplerate)).exp(),
            spark: 0.0,
            still: 0.5,
            aura_semis: 0.0,
            veil: 0.4,
            wide: 1.0,
            tint: 0.5,
            rate_norm: 0.5,
            grain_norm: 0.4,
        };
        c.update_rate();
        c.update_tone();
        c
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

    fn update_rate(&mut self) {
        let t = self.rate_norm.clamp(0.0, 1.0);
        self.clock_rate = MIN_RATE_HZ * (MAX_RATE_HZ / MIN_RATE_HZ).powf(t);
    }

    fn update_tone(&mut self) {
        let t = self.tint.clamp(0.0, 1.0);
        let hz = MIN_TONE_HZ + t * (MAX_TONE_HZ - MIN_TONE_HZ);
        self.lp_coeff = 1.0 - (-TAU * hz / self.samplerate).exp();
    }

    fn spawn_grain(&mut self) {
        let slot = match (0..MAX_GRAINS).find(|&i| !self.grains[i].active) {
            Some(s) => s,
            None => return,
        };

        // ---- Pitch ------------------------------------------------------
        let semis = if self.aura_semis.abs() < 0.5 {
            SEQUENCE[self.current_step % 16] as f64
        } else {
            self.aura_semis
        };
        let rate = 2.0_f64.powf(semis / 12.0);

        // ---- Grain length, capped by rate ------------------------------
        // The key anti-crackle rule: a grain whose read head moves
        // faster than the write head must be shorter than the buffer
        // half-length, otherwise it catches the write head and reads
        // garbage. The 0.45 factor leaves a safety margin for the
        // fade-out tail.
        let desired_len_ms = MIN_GRAIN_MS + self.grain_norm * (MAX_GRAIN_MS - MIN_GRAIN_MS);
        let desired_len = desired_len_ms * 1e-3 * self.samplerate;

        let rate_cap = rate.max(1.0);
        let max_safe_len = (self.buf_len as f64 * 0.45) / rate_cap;
        let grain_len = desired_len.min(max_safe_len).max(64.0);

        let fade_in = (GRAIN_FADE_IN_MS * 1e-3 * self.samplerate)
            .min(grain_len * 0.45)
            .max(1.0);
        let fade_out = (GRAIN_FADE_OUT_MS * 1e-3 * self.samplerate)
            .min(grain_len * 0.45)
            .max(1.0);

        // ---- Read position (STILL controls scatter) --------------------
        let max_offset = (self.buf_len as f64 * 0.5).max(1.0);
        let wander = 1.0 - self.still.clamp(0.0, 1.0);
        let reach = (0.05 + 0.95 * wander).clamp(0.0, 1.0);

        // Additional safety: ensure the starting position is at least
        // (grain_len * (rate - 1)) samples behind the write head, so
        // the read never crosses the write during the grain.
        let travel = grain_len * (rate - 1.0).max(0.0);
        let min_offset = (travel + 64.0).min(max_offset);

        let range = (max_offset - min_offset).max(0.0);
        let offset = min_offset + self.rng_next() * range * reach;

        let mut start = self.write as f64 - offset;
        if start < 0.0 {
            start += self.buf_len as f64;
        }

        let pan_amount = 0.3 + 0.7 * wander;
        let pan = (self.rng_next() * 2.0 - 1.0) * pan_amount;

        self.grains[slot] = Grain {
            active: true,
            read_pos: start,
            rate,
            elapsed: 0.0,
            length: grain_len,
            pan,
            fade_in,
            fade_out,
        };
    }

    #[inline]
    fn read_cubic(buf: &[f64], pos: f64) -> f64 {
        let len = buf.len() as isize;
        if len < 4 {
            return 0.0;
        }

        let p = pos.rem_euclid(len as f64);
        let i = p.floor() as isize;
        let frac = p - i as f64;

        let i_m1 = (i - 1).rem_euclid(len) as usize;
        let i0 = i.rem_euclid(len) as usize;
        let i_p1 = (i + 1).rem_euclid(len) as usize;
        let i_p2 = (i + 2).rem_euclid(len) as usize;

        let xm1 = buf[i_m1];
        let x0 = buf[i0];
        let x1 = buf[i_p1];
        let x2 = buf[i_p2];

        let a = -0.5 * xm1 + 1.5 * x0 - 1.5 * x1 + 0.5 * x2;
        let b = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
        let c = -0.5 * xm1 + 0.5 * x1;
        let d = x0;

        a * frac * frac * frac + b * frac * frac + c * frac + d
    }

    #[inline]
    fn grain_envelope(t: f64, grain_len: f64, fade_in: f64, fade_out: f64) -> f64 {
        let fi = fade_in / grain_len;
        let fo = fade_out / grain_len;
        if t < fi {
            let x = t / fi;
            x * x * (3.0 - 2.0 * x)
        } else if t > 1.0 - fo {
            let x = (1.0 - t) / fo;
            x * x * (3.0 - 2.0 * x)
        } else {
            1.0
        }
    }

    #[inline]
    fn flush_denorm(x: f64) -> f64 {
        if x.abs() < DENORM {
            0.0
        } else {
            x
        }
    }

    #[inline]
    fn soft_clip(x: f64) -> f64 {
        let x = x.clamp(-3.0, 3.0);
        let x2 = x * x;
        x * (27.0 + x2) / (27.0 + 9.0 * x2)
    }

    pub fn process_inplace(&mut self, left: &mut [f64], right: &mut [f64]) {
        if self.buf_len == 0 {
            return;
        }

        // Fast-path: fully faded out AND target is 0. Once the
        // smoother has settled at 0 we can short-circuit entirely.
        if self.spark <= 0.0 && self.spark_smoothed < 1.0e-6 {
            return;
        }

        let n = left.len().min(right.len());
        let fb_gain = self.veil.clamp(0.0, 0.85);
        let buf_len = self.buf_len;
        let buf_len_f = buf_len as f64;
        let clock_inc = self.clock_rate / self.samplerate;
        let smooth = self.out_smooth_coeff;
        let spark_coeff = self.spark_smooth_coeff;
        let spark_target = self.spark;

        for i in 0..n {
            // Per-sample smoother on the crossfade amount. This is
            // the primary fix for the click that used to be heard
            // when the enable switch was toggled.
            self.spark_smoothed += spark_coeff * (spark_target - self.spark_smoothed);
            let s = self.spark_smoothed.clamp(0.0, 1.0);
            let dry_gain = (1.0 - s).sqrt();
            let wet_gain = s.sqrt() * WET_MAKEUP;

            let in_l = left[i];
            let in_r = right[i];

            // ---- Write with feedback and safety limit ------------------
            let pre_l = (in_l + self.fb_l * fb_gain).clamp(-BUF_ENTRY_LIMIT, BUF_ENTRY_LIMIT);
            let pre_r = (in_r + self.fb_r * fb_gain).clamp(-BUF_ENTRY_LIMIT, BUF_ENTRY_LIMIT);
            self.buf_l[self.write] = if pre_l.is_finite() { pre_l } else { 0.0 };
            self.buf_r[self.write] = if pre_r.is_finite() { pre_r } else { 0.0 };

            // ---- Clock --------------------------------------------------
            self.clock_phase += clock_inc;
            if self.clock_phase >= 1.0 {
                self.clock_phase -= 1.0;
                self.current_step = (self.current_step + 1) % 16;
                self.spawn_grain();
            }

            // ---- Grain sum ----------------------------------------------
            let mut sum_l = 0.0;
            let mut sum_r = 0.0;
            for g in 0..MAX_GRAINS {
                let grain = &mut self.grains[g];
                if !grain.active {
                    continue;
                }

                let t = grain.elapsed / grain.length;
                if t >= 1.0 {
                    grain.active = false;
                    continue;
                }

                let env = Self::grain_envelope(t, grain.length, grain.fade_in, grain.fade_out);

                let read_l = Self::read_cubic(&self.buf_l, grain.read_pos);
                let read_r = Self::read_cubic(&self.buf_r, grain.read_pos);

                let pan_l = ((1.0 - grain.pan) * 0.5).sqrt();
                let pan_r = ((1.0 + grain.pan) * 0.5).sqrt();

                sum_l += read_l * env * pan_l;
                sum_r += read_r * env * pan_r;

                grain.elapsed += 1.0;
                grain.read_pos += grain.rate;
                if grain.read_pos >= buf_len_f {
                    grain.read_pos -= buf_len_f;
                }
            }

            // ---- DC blocker --------------------------------------------
            let dc_l = sum_l - self.dc_x_l + 0.995 * self.dc_y_l;
            self.dc_x_l = sum_l;
            self.dc_y_l = Self::flush_denorm(dc_l);
            let dc_l = self.dc_y_l;

            let dc_r = sum_r - self.dc_x_r + 0.995 * self.dc_y_r;
            self.dc_x_r = sum_r;
            self.dc_y_r = Self::flush_denorm(dc_r);
            let dc_r = self.dc_y_r;

            // ---- Tone filter --------------------------------------------
            self.lp_state_l += self.lp_coeff * (dc_l - self.lp_state_l);
            self.lp_state_r += self.lp_coeff * (dc_r - self.lp_state_r);
            let wet_l = Self::flush_denorm(self.lp_state_l);
            let wet_r = Self::flush_denorm(self.lp_state_r);

            // ---- Stereo width -------------------------------------------
            let mid = (wet_l + wet_r) * 0.5;
            let side = (wet_l - wet_r) * 0.5 * self.wide;
            let out_l = mid + side;
            let out_r = mid - side;

            // ---- Output smoothing --------------------------------------
            self.out_smooth_l += smooth * (out_l - self.out_smooth_l);
            self.out_smooth_r += smooth * (out_r - self.out_smooth_r);
            let out_l = Self::flush_denorm(self.out_smooth_l);
            let out_r = Self::flush_denorm(self.out_smooth_r);

            // ---- Feedback with soft clip -------------------------------
            self.fb_l = Self::soft_clip(out_l * 0.5);
            self.fb_r = Self::soft_clip(out_r * 0.5);

            // ---- Advance write head ------------------------------------
            self.write += 1;
            if self.write >= buf_len {
                self.write = 0;
            }

            // ---- Output blend ------------------------------------------
            left[i] = in_l * dry_gain + out_l * wet_gain;
            right[i] = in_r * dry_gain + out_r * wet_gain;
        }
    }

    pub fn clear_buffers(&mut self) {
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write = 0;
        self.clock_phase = 0.0;
        self.current_step = 0;
        self.fb_l = 0.0;
        self.fb_r = 0.0;
        self.dc_x_l = 0.0;
        self.dc_y_l = 0.0;
        self.dc_x_r = 0.0;
        self.dc_y_r = 0.0;
        self.lp_state_l = 0.0;
        self.lp_state_r = 0.0;
        self.out_smooth_l = 0.0;
        self.out_smooth_r = 0.0;
        self.spark_smoothed = 0.0;
        for g in 0..MAX_GRAINS {
            self.grains[g] = Grain::INACTIVE;
        }
    }


    pub fn set_samplerate(&mut self, sr: f64) {
        if (sr - self.samplerate).abs() < 1.0 {
            return;
        }
        self.samplerate = sr;
        let new_len = (BUF_SECS * sr).ceil() as usize;
        if new_len != self.buf_len {
            self.buf_l = vec![0.0; new_len];
            self.buf_r = vec![0.0; new_len];
            self.buf_len = new_len;
            self.write = 0;
        }
        let smooth_hz = 10_000.0_f64.min(sr * 0.45);
        self.out_smooth_coeff = 1.0 - (-TAU * smooth_hz / sr).exp();
        self.spark_smooth_coeff = 1.0 - (-1.0 / (0.020 * sr)).exp();
        for g in 0..MAX_GRAINS {
            self.grains[g] = Grain::INACTIVE;
        }
        self.update_rate();
        self.update_tone();
    }

    // ---- Parameter setters ----------------------------------------------

    /// Sets the granular amount. The dry/wet crossfade is smoothed
    /// internally (20 ms one-pole), and a rising edge from exactly
    /// 0 triggers a full buffer clear so that re-enabling the
    /// effect never produces a click.
    #[inline]
    pub fn set_spark(&mut self, v: f64) {
        let old = self.spark;
        self.spark = v.clamp(0.0, 1.0);
        // Rising edge: `old` was exactly 0 (disabled) and now we're
        // active. Perform a full reset including the capture buffer.
        // The buffer holds the last 2 seconds of audio captured
        // before the layer was bypassed; if TX was engaged for a
        // while, that content is stale and would produce a click
        // when the read head starts playing it back. Clearing the
        // buffer makes the layer start from silence, which — with
        // the 20 ms crossfade ramp — is completely click-free.
        if old <= 0.0 && self.spark > 0.0 {
            self.clear_buffers();
        }
    }
    #[inline]
    pub fn set_still(&mut self, v: f64) {
        self.still = v.clamp(0.0, 1.0);
    }
    #[inline]
    pub fn set_aura_semitones(&mut self, semis: f64) {
        self.aura_semis = semis.clamp(-24.0, 24.0);
    }
    #[inline]
    pub fn set_veil(&mut self, v: f64) {
        self.veil = v.clamp(0.0, 0.85);
    }
    #[inline]
    pub fn set_wide(&mut self, v: f64) {
        self.wide = v.clamp(0.0, 2.0);
    }
    #[inline]
    pub fn set_tint(&mut self, v: f64) {
        self.tint = v.clamp(0.0, 1.0);
        self.update_tone();
    }
    #[inline]
    pub fn set_rate_norm(&mut self, v: f64) {
        self.rate_norm = v.clamp(0.0, 1.0);
        self.update_rate();
    }
    #[inline]
    pub fn set_grain_norm(&mut self, v: f64) {
        self.grain_norm = v.clamp(0.0, 1.0);
    }
}
