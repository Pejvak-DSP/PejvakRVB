//! FFDN delay line – GPL-3.0
//!
//! A modulated, damped delay line for the FFDN. In addition to the
//! one-pole low-pass that shapes the overall high-frequency content,
//! each line carries a low-shelf and a high-shelf filter for
//! **frequency-dependent attenuation** — the ability to give the bass,
//! mid, and treble bands different RT60 values.
//!
//! # Signal path per sample
//!
//! ```text
//!   read → damping (one-pole LP) → low-shelf → high-shelf → out
//! ```
//!
//! Reading uses fractional interpolation when the LFO is active.
//! When modulation is disabled (`amount = 0`), a fast integer-only
//! path is used and the LFO phase does not advance.

use crate::cloudseed::biquad::{Biquad, FilterType};

/// One-pole low-pass filter for the master HF damping.
#[derive(Clone, Debug)]
struct DampingFilter {
    coeff: f64,
    state: f64,
}

impl DampingFilter {
    fn new() -> Self {
        Self {
            coeff: 1.0,
            state: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        self.state += self.coeff * (x - self.state);
        self.state
    }

    fn reset(&mut self) {
        self.state = 0.0;
    }
}

/// A single modulated delay line in an FFDN.
pub struct FfdnDelayLine {
    buffer: Vec<f64>,
    write_idx: usize,
    delay_samples: f64,
    target_delay_samples: f64,
    damping: DampingFilter,

    // ---- Frequency-dependent attenuation -----------------------------
    /// Low shelf, applied below `bass_freq`. Zero dB means no change
    /// to the bass band's decay.
    low_shelf: Biquad,
    /// High shelf, applied above `treble_freq`. Negative dB means
    /// faster treble decay.
    high_shelf: Biquad,

    // ---- : loop HP/LP filters --------------------------------
    /// High-pass filter inside the feedback loop. At the neutral
    /// frequency (20 Hz) the filter is **bypassed entirely** so the
    /// signal path is bit-identical to the pre- engine.
    loop_hp: Biquad,
    /// Low-pass filter inside the feedback loop. Bypassed at the
    /// neutral frequency (20 kHz).
    loop_lp: Biquad,
    /// True when `loop_hp` should be applied. Only recomputed when
    /// the cutoff frequency changes; the per-sample path checks
    /// this flag instead of comparing floats.
    loop_hp_enabled: bool,
    /// True when `loop_lp` should be applied.
    loop_lp_enabled: bool,

    // ---- : mid-focus shelf pair ------------------------------
    /// Low shelf at 400 Hz. At `mid_focus = 0` gain is 0 dB
    /// (bypass); at 1 it is -6 dB (cuts the lows, focusing the tail
    /// on the mid band).
    ///
    /// # Why cut, not boost
    ///
    /// A positive-gain peak filter inside the feedback loop is
    /// mathematically unstable: the loop gain at the peak frequency
    /// would be `g_mid × peak_gain`, which exceeds 1 as soon as
    /// `peak_gain > 1/g_mid`. For any long decay (`g_mid → 1`) the
    /// allowed boost is essentially zero. Cutting the *surrounding*
    /// bands is the only way to implement a "mid focus" that is
    /// stable across the whole decay range, and it is also what
    /// every serious FDN reverb does (Valhalla, Cinematic Rooms,
    /// Tai Chi).
    mid_low: Biquad,
    /// High shelf at 1500 Hz. Same gain mapping as `mid_low`.
    mid_high: Biquad,
    /// True when the shelf pair should be applied. Only recomputed
    /// when `mid_focus` crosses the bypass threshold.
    mid_focus_enabled: bool,

    sample_rate: f64,

    // ---- Modulation -------------------------------------------------
    lfo_phase: f64,
    lfo_phase_increment: f64,
    mod_amount_samples: f64,
}

impl FfdnDelayLine {
    /// Creates a delay line whose buffer can hold `max_delay_samples`
    /// samples. Shelf filters start at 0 dB (neutral).
    pub fn new(max_delay_samples: usize, sample_rate: f64) -> Self {
        let size = max_delay_samples.max(2);

        // Low shelf: default 0 dB, crossover 250 Hz.
        let mut low_shelf = Biquad::new(FilterType::LowShelf, sample_rate);
        low_shelf.set_slope(1.0);
        low_shelf.set_gain_db(0.0);
        low_shelf.set_frequency(250.0);
        low_shelf.update();

        // High shelf: default 0 dB, crossover 4 kHz.
        let mut high_shelf = Biquad::new(FilterType::HighShelf, sample_rate);
        high_shelf.set_slope(1.0);
        high_shelf.set_gain_db(0.0);
        high_shelf.set_frequency(4000.0);
        high_shelf.update();

        // ---- : loop HP/LP + mid focus ------------------------
        // Loop HP: 20 Hz (neutral, bypassed by default). Butterworth
        // Q (1/sqrt(2) ≈ 0.707) so the response is maximally flat.
        let mut loop_hp = Biquad::new(FilterType::HighPass, sample_rate);
        loop_hp.set_q(0.707);
        loop_hp.set_frequency(20.0);
        loop_hp.update();

        // Loop LP: 20 kHz (neutral, bypassed by default).
        let mut loop_lp = Biquad::new(FilterType::LowPass, sample_rate);
        loop_lp.set_q(0.707);
        loop_lp.set_frequency(20_000.0);
        loop_lp.update();

        // Mid focus: shelf pair at 400 / 1500 Hz, both starting at
        // 0 dB (neutral, bypassed by default). The `slope = 1.0`
        // gives a gentle 6 dB/oct transition, which sounds more
        // natural than a steep shelf on the loop path.
        let mut mid_low = Biquad::new(FilterType::LowShelf, sample_rate);
        mid_low.set_slope(1.0);
        mid_low.set_gain_db(0.0);
        mid_low.set_frequency(400.0);
        mid_low.update();

        let mut mid_high = Biquad::new(FilterType::HighShelf, sample_rate);
        mid_high.set_slope(1.0);
        mid_high.set_gain_db(0.0);
        mid_high.set_frequency(1500.0);
        mid_high.update();

        Self {
            buffer: vec![0.0; size],
            write_idx: 0,
            delay_samples: (size / 2) as f64,
            target_delay_samples: (size / 2) as f64,
            damping: DampingFilter::new(),
            low_shelf,
            high_shelf,
            loop_hp,
            loop_lp,
            mid_low,
            mid_high,
            mid_focus_enabled: false,
            loop_hp_enabled: false,
            loop_lp_enabled: false,
            sample_rate,
            lfo_phase: 0.0,
            lfo_phase_increment: 0.0,
            mod_amount_samples: 0.0,
        }
    }

    /// Sets the delay length in samples. Clamped to `buffer.len() - 2`
    /// to keep fractional interpolation in range. The current
    /// `delay_samples` glides toward this target over the next few
    /// hundred samples — see `read_raw`.
    pub fn set_delay(&mut self, samples: usize) {
        let new = (samples as f64).min((self.buffer.len() - 2) as f64);
        self.target_delay_samples = new;
        // First-ever call (buffer just allocated): snap so the read
        // head starts at the right position instead of gliding up
        // from zero.
        if self.delay_samples == 0.0 {
            self.delay_samples = new;
        }
    }

    /// Sets the master damping cutoff in Hz.
    pub fn set_damping_hz(&mut self, hz: f64, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let fc = hz.clamp(20.0, sample_rate * 0.49);
        self.damping.coeff = 1.0 - (-2.0 * std::f64::consts::PI * fc / sample_rate).exp();
    }

    /// Sets the low and high shelf filters.
    ///
    /// * `low_gain_db` / `low_freq`: gain and crossover of the low
    ///   shelf. Positive gain slows the bass decay; negative speeds
    ///   it up.
    /// * `high_gain_db` / `high_freq`: same for the high shelf.
    ///   Negative gain (the common case for reverb) speeds up the
    ///   treble decay.
    pub fn set_shelves(
        &mut self,
        low_gain_db: f64,
        low_freq: f64,
        high_gain_db: f64,
        high_freq: f64,
    ) {
        self.low_shelf.set_gain_db(low_gain_db);
        self.low_shelf.set_frequency(low_freq);
        self.low_shelf.update();

        self.high_shelf.set_gain_db(high_gain_db);
        self.high_shelf.set_frequency(high_freq);
        self.high_shelf.update();
    }

    /// Sets the loop HP/LP cutoffs in Hz.
    ///
    /// At `hp_hz <= 25` the high-pass is bypassed entirely. At
    /// `lp_hz >= 19500` the low-pass is bypassed. Both bypasses are
    /// implemented via a flag check on the per-sample path, so the
    /// neutral setting is bit-identical to the pre- engine.
    pub fn set_loop_filters(&mut self, hp_hz: f64, lp_hz: f64) {
        // ---- Loop HP -------------------------------------------------
        if hp_hz > 25.0 {
            self.loop_hp.set_frequency(hp_hz);
            self.loop_hp.update();
            self.loop_hp_enabled = true;
        } else {
            self.loop_hp_enabled = false;
        }

        // ---- Loop LP -------------------------------------------------
        if lp_hz < 19_500.0 {
            self.loop_lp.set_frequency(lp_hz);
            self.loop_lp.update();
            self.loop_lp_enabled = true;
        } else {
            self.loop_lp_enabled = false;
        }
    }

    /// Sets the mid-focus amount (0..1). At 0 the shelf pair is
    /// bypassed (bit-identical to the pre- engine); at 1 the
    /// lows below 400 Hz and highs above 1500 Hz are attenuated by
    /// 6 dB, focusing the tail on the mid band.
    ///
    /// The shelves only **cut**, never boost. A boost inside the
    /// feedback loop would multiply `g_mid`, and for any decay time
    /// above ~2 s that product exceeds 1, making the network
    /// unstable. Cutting the surrounding bands achieves the same
    /// perceptual effect — the mid band is what remains dominant —
    /// without touching the loop gain.
    pub fn set_mid_focus(&mut self, amount: f64) {
        let a = amount.clamp(0.0, 1.0);
        if a < 0.005 {
            self.mid_focus_enabled = false;
        } else {
            // Map 0..1 to 0..-6 dB. Both shelves move together so the
            // "focus" is symmetric around the mid band.
            let cut_db = -a * 6.0;
            self.mid_low.set_gain_db(cut_db);
            self.mid_low.update();
            self.mid_high.set_gain_db(cut_db);
            self.mid_high.update();
            self.mid_focus_enabled = true;
        }
    }

    /// Updates the shelf filters' sample rate (called on rate change).
    pub fn set_samplerate_shelves(&mut self, sr: f64) {
        self.sample_rate = sr;
        self.low_shelf.set_samplerate(sr);
        self.high_shelf.set_samplerate(sr);
        // the loop HP/LP and mid-focus shelf pair also hold
        // frequency-derived coefficients that must be recomputed at
        // the new rate.
        self.loop_hp.set_samplerate(sr);
        self.loop_lp.set_samplerate(sr);
        self.mid_low.set_samplerate(sr);
        self.mid_high.set_samplerate(sr);
    }

    /// Sets the modulation depth (in samples) and rate (in Hz).
    pub fn set_modulation(&mut self, amount_samples: f64, rate_hz: f64, sample_rate: f64) {
        self.mod_amount_samples = amount_samples.max(0.0);
        self.lfo_phase_increment = (rate_hz / sample_rate).max(0.0);
    }

    /// Sets the LFO start phase (0..1).
    pub fn set_lfo_phase(&mut self, phase: f64) {
        self.lfo_phase = phase - phase.floor();
    }

    /// Clears every buffer and filter state. Does **not** reset the
    /// LFO phase, so a running LFO keeps its phase across a clear.
    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.write_idx = 0;
        self.damping.reset();
        self.low_shelf.clear_buffers();
        self.high_shelf.clear_buffers();
        self.loop_hp.clear_buffers();
        self.loop_lp.clear_buffers();
        self.mid_low.clear_buffers();
        self.mid_high.clear_buffers();
    }

    /// Reads the delayed sample and applies the full per-line filter
    /// chain: damping → loop HP → loop LP → low shelf → high shelf
    /// → mid focus.
    ///
    /// All filters in the chain are bypassed when their parameter is
    /// at the neutral value, so the pre- behaviour is
    /// preserved bit-for-bit for presets that do not touch the new
    /// FFDN FILTER parameters.
    #[inline]
    pub fn read(&mut self) -> f64 {
        let delayed = self.read_raw();
        let mut s = self.damping.process(delayed);

        // ---- Loop HP / LP ---------------------------------
        // Applied *after* damping so they shape the spectrum that has
        // already been low-passed, which is what the user expects:
        // the loop filter acts on the actual signal circulating in
        // the network, not on the raw delayed sample.
        if self.loop_hp_enabled {
            s = self.loop_hp.process(s);
        }
        if self.loop_lp_enabled {
            s = self.loop_lp.process(s);
        }

        // ---- Existing shelves ---------------------------------------
        s = self.low_shelf.process(s);
        s = self.high_shelf.process(s);

        // ---- Mid focus ------------------------------------
        // A shelf pair that cuts the bands below 400 Hz and above
        // 1500 Hz. This focuses the tail on the mid band without
        // ever boosting the loop gain, so stability is preserved
        // across the entire decay range.
        if self.mid_focus_enabled {
            s = self.mid_low.process(s);
            s = self.mid_high.process(s);
        }

        s
    }

    /// Raw sample extraction, without any filtering.

    #[inline]
    fn read_raw(&mut self) -> f64 {
        let len = self.buffer.len();
        const SMOOTH: f64 = 0.0007;
        self.delay_samples += (self.target_delay_samples - self.delay_samples) * SMOOTH;

        // LFO offset (zero when modulation is off).
        let mod_offset = if self.mod_amount_samples == 0.0 {
            0.0
        } else {
            let phase_rad = std::f64::consts::TAU * self.lfo_phase;
            self.mod_amount_samples * phase_rad.sin()
        };

        let delay_f = (self.delay_samples + mod_offset).clamp(1.0, (len - 2) as f64);

        let read_pos = self.write_idx as f64 - delay_f;
        let read_pos = if read_pos < 0.0 {
            read_pos + len as f64
        } else {
            read_pos
        };
        let i0 = read_pos.floor() as usize;
        let i1 = if i0 + 1 >= len { 0 } else { i0 + 1 };
        let frac = read_pos - i0 as f64;
        let delayed = self.buffer[i0] * (1.0 - frac) + self.buffer[i1] * frac;

        // Advance LFO only when modulation is active.
        if self.mod_amount_samples != 0.0 {
            self.lfo_phase += self.lfo_phase_increment;
            if self.lfo_phase >= 1.0 {
                self.lfo_phase -= 1.0;
            }
        }

        delayed
    }

    /// Writes `input` at the current write head, then advances the
    /// head by one sample.
    #[inline]
    pub fn write(&mut self, input: f64) {
        let len = self.buffer.len();
        self.buffer[self.write_idx] = input;
        self.write_idx += 1;
        if self.write_idx >= len {
            self.write_idx = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_line_reads_back_after_delay_samples() {
        let mut dl = FfdnDelayLine::new(16, 48000.0);
        dl.set_delay(4);
        // The Size Smoothing feature (v4.90.0) makes `set_delay` glide
        // toward the target over ~30 ms instead of snapping, which is
        // the correct DSP behaviour for click-free Size sweeps. This
        // unit test however checks the pure read/write arithmetic, so
        // we force both the current position and the target to the
        // same value; the glide then becomes a no-op and the sample
        // written at `t = 0` lands exactly on `out[4]`.
        dl.delay_samples = 4.0;
        dl.target_delay_samples = 4.0;
        dl.damping.coeff = 1.0;
        dl.damping.state = 0.0;

        let mut out = Vec::new();
        out.push(dl.read());
        dl.write(1.0);
        for _ in 1..10 {
            out.push(dl.read());
            dl.write(0.0);
        }

        assert_eq!(out[0], 0.0);
        assert_eq!(out[3], 0.0);
        assert!((out[4] - 1.0).abs() < 1e-12);
    }

    /// Verifies that `set_delay` glides toward the new target instead
    /// of snapping. This is the core behaviour of the Size Smoothing
    /// feature (v4.90.0): the delay line's read head must move
    /// continuously, otherwise every Size knob turn would produce an
    /// audible click from the abrupt jump in read position.
    #[test]
    fn set_delay_glides_toward_target() {
        let mut dl = FfdnDelayLine::new(256, 48000.0);
        // `new` initialises both positions to `size / 2 = 128`.
        let initial = dl.delay_samples;
        assert!((initial - 128.0).abs() < 1e-9);

        // Request a much shorter delay. With Size Smoothing this must
        // NOT snap: the current position stays put, only the target
        // changes.
        dl.set_delay(20);
        assert_eq!(dl.target_delay_samples, 20.0);
        assert_eq!(dl.delay_samples, initial, "set_delay must not snap");

        // Advance one sample. `read_raw` applies one step of the
        // one-pole glide, so `delay_samples` moves by exactly
        // SMOOTH * (target - current) = 0.0007 * (20 - 128).
        const SMOOTH: f64 = 0.0007;
        let expected_after_one = initial + (20.0 - initial) * SMOOTH;
        let _ = dl.read();
        assert!(
            (dl.delay_samples - expected_after_one).abs() < 1e-12,
            "after one read: got {}, expected {}",
            dl.delay_samples,
            expected_after_one
        );

        // It must still be close to the initial value — a single step
        // is tiny compared to the full 108-sample gap.
        assert!(
            (dl.delay_samples - initial).abs() < 0.1,
            "one step should be < 0.1 samples, got {}",
            (dl.delay_samples - initial).abs()
        );

        // Advance many samples. After N samples the remaining error is
        // (1 - SMOOTH)^N of the initial gap. At N = 20_000 this is
        // about e^-14 ≈ 8e-7, so the residual gap (108 samples × 8e-7)
        // is well under 0.001 — far tighter than the 0.01 tolerance
        // we check below.
        for _ in 0..20_000 {
            let _ = dl.read();
        }
        assert!(
            (dl.delay_samples - 20.0).abs() < 0.01,
            "after 20 000 samples: delay_samples = {}, expected ≈ 20.0",
            dl.delay_samples
        );

        // The glide must be strictly monotonic: every sample moves the
        // read head closer to the target without overshoot.
        assert!(
            dl.delay_samples >= 20.0 && dl.delay_samples <= initial,
            "glide went outside [target, initial]: {}",
            dl.delay_samples
        );
    }

    /// Verifies the first-call snap branch in `set_delay`. When the
    /// delay line has just been allocated and the current position is
    /// still at its zero initialiser, gliding up from zero would make
    /// the read head lag behind the requested delay for hundreds of
    /// milliseconds. `set_delay` therefore snaps on the very first
    /// call so the line starts at the correct position.
    #[test]
    fn set_delay_snaps_on_first_call() {
        let mut dl = FfdnDelayLine::new(256, 48000.0);
        // Simulate the "freshly allocated, never positioned" state.
        // This cannot be reached through the public API because
        // `new` seeds both values to `size / 2`, but the branch is
        // defensive and worth covering.
        dl.delay_samples = 0.0;
        dl.target_delay_samples = 0.0;

        dl.set_delay(30);

        // Snap branch: both current and target end up at 30.
        assert_eq!(dl.delay_samples, 30.0, "first call must snap");
        assert_eq!(dl.target_delay_samples, 30.0);

        // A second call must NOT snap; it must glide.
        dl.set_delay(60);
        assert_eq!(dl.delay_samples, 30.0, "second call must glide");
        assert_eq!(dl.target_delay_samples, 60.0);

        // One read advances the position by one glide step toward 60.
        const SMOOTH: f64 = 0.0007;
        let expected = 30.0 + (60.0 - 30.0) * SMOOTH;
        let _ = dl.read();
        assert!(
            (dl.delay_samples - expected).abs() < 1e-12,
            "after one read: got {}, expected {}",
            dl.delay_samples,
            expected
        );
    }
}
