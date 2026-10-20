//! ReverbChannel – GPL-3.0
//!
//! Faithful Rust port of `CloudSeed.Native/ReverbChannel.h` (legacy-v1).
//! All state and arithmetic uses `f64` to match the C++ `double` type.
//!
//! On top of the CloudSeed port, this module hosts the Velvet-Noise
//! **VFDN** diffusion stage: two independent VN filters, one on the
//! input of the FDN and one on the output. Per Fagerström et al.
//! (DAFx 2020), "placing the VNS filters at both the input and output
//! of the FDN yields a denser echo pattern than using only one".
//! Both filters sit OUTSIDE every feedback loop, so the system is
//! unconditionally stable regardless of diffusion amount.

use crate::cloudseed::allpass_diffuser::AllpassDiffuser;
use crate::cloudseed::delay_line::DelayLine;
use crate::cloudseed::hp1::Hp1;
use crate::cloudseed::lp1::Lp1;
use crate::cloudseed::modulated_delay::ModulatedDelay;
use crate::cloudseed::multitap_diffuser::MultitapDiffuser;
use crate::cloudseed::sha_random;
use crate::cloudseed::velvet::VelvetNoiseFilter;

/// Number of delay lines per channel (matches `TotalLineCount`).
pub const TOTAL_LINE_COUNT: usize = 12;

/// Total number of CloudReverb parameters (matches `Parameter::Count`).
const PARAM_COUNT: usize = 49;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelLR {
    Left,
    Right,
}

// Parameter index constants — mirror `Parameter.h` in order.
const P_PREDELAY: usize = 1;
const P_HIGH_PASS: usize = 2;
const P_LOW_PASS: usize = 3;
const P_TAP_ENABLED: usize = 4;
const P_TAP_COUNT: usize = 5;
const P_TAP_LENGTH: usize = 6;
const P_TAP_GAIN: usize = 7;
const P_TAP_DECAY: usize = 8;
const P_DIFFUSION_ENABLED: usize = 9;
const P_DIFFUSION_STAGES: usize = 10;
const P_DIFFUSION_DELAY: usize = 11;
const P_DIFFUSION_FEEDBACK: usize = 12;
const P_LATE_DELAY_ENABLED: usize = 13;
const P_LINE_COUNT: usize = 14;
const P_LINE_DELAY: usize = 15;
const P_LINE_DECAY: usize = 16;
const P_LATE_DIFFUSION_ENABLED: usize = 17;
const P_LATE_DIFFUSION_STAGES: usize = 18;
const P_LATE_DIFFUSION_DELAY: usize = 19;
const P_LATE_DIFFUSION_FEEDBACK: usize = 20;
const P_POST_LOW_SHELF_GAIN: usize = 21;
const P_POST_LOW_SHELF_FREQUENCY: usize = 22;
const P_POST_HIGH_SHELF_GAIN: usize = 23;
const P_POST_HIGH_SHELF_FREQUENCY: usize = 24;
const P_POST_CUTOFF_FREQUENCY: usize = 25;
const P_EARLY_DIFFUSION_MOD_AMOUNT: usize = 26;
const P_EARLY_DIFFUSION_MOD_RATE: usize = 27;
const P_LINE_MOD_AMOUNT: usize = 28;
const P_LINE_MOD_RATE: usize = 29;
const P_LATE_DIFFUSION_MOD_AMOUNT: usize = 30;
const P_LATE_DIFFUSION_MOD_RATE: usize = 31;
const P_TAP_SEED: usize = 32;
const P_DIFFUSION_SEED: usize = 33;
const P_DELAY_SEED: usize = 34;
const P_POST_DIFFUSION_SEED: usize = 35;
const P_CROSS_SEED: usize = 36;
const P_DRY_OUT: usize = 37;
const P_PREDELAY_OUT: usize = 38;
const P_EARLY_OUT: usize = 39;
const P_MAIN_OUT: usize = 40;
const P_HI_PASS_ENABLED: usize = 42;
const P_LOW_PASS_ENABLED: usize = 43;
const P_LOW_SHELF_ENABLED: usize = 44;
const P_HIGH_SHELF_ENABLED: usize = 45;
const P_CUTOFF_ENABLED: usize = 46;
const P_LATE_STAGE_TAP: usize = 47;
const P_INTERPOLATION: usize = 48;

#[derive(Clone, Debug)]
pub struct ReverbChannel {
    parameters: [f64; PARAM_COUNT],
    samplerate: f64,
    buffer_size: usize,

    pre_delay: ModulatedDelay,
    multitap: MultitapDiffuser,
    diffuser: AllpassDiffuser,
    lines: Vec<DelayLine>,
    high_pass: Hp1,
    low_pass: Lp1,

    /// **Input** Velvet-Noise diffusion. Applied to the signal feeding
    /// the FDN delay lines. Per the VFDN topology (Fagerström et al.,
    /// DAFx 2020), placing a VN filter at the input of the FDN
    /// decorrelates the incoming samples before they enter the
    /// feedback network, which yields a denser echo pattern at no
    /// cost to stability — this filter is outside every feedback
    /// loop.
    velvet_in: VelvetNoiseFilter,

    /// **Output** Velvet-Noise diffusion. Applied to the wet sum, as
    /// in the previous single-filter design. Together with `velvet_in`
    /// this completes the input → FDN → output diffusion chain.
    velvet_out: VelvetNoiseFilter,

    /// Shared diffusion amount (0..1). Both filters receive the same
    /// value so the knob moves as a single control.
    velvet_amount: f64,

    /// Shared "darkening" amount (0..1). 0 = bright impulse pulses,
    /// 1 = fully widened rectangular pulses (Dark Velvet Noise).
    velvet_tone: f64,

    temp_buffer: Vec<f64>,
    early_buffer: Vec<f64>,
    out_buffer: Vec<f64>,

    delay_line_seed: i64,
    post_diffusion_seed: i64,
    /// Cached line-decay seeds (recomputed only when delay/cross seed changes).
    delay_line_seeds: [f64; TOTAL_LINE_COUNT * 3],

    line_count: usize,
    high_pass_enabled: bool,
    low_pass_enabled: bool,
    tap_enabled: bool,
    diffuser_enabled: bool,
    late_delay_enabled: bool,

    dry_out: f64,
    predelay_out: f64,
    early_out: f64,
    line_out: f64,
    cross_seed: f64,
    channel_lr: ChannelLR,
    frozen: bool,
}

impl ReverbChannel {
    pub fn new(buffer_size: usize, samplerate: i32, left_or_right: ChannelLR) -> Self {
        let sr = samplerate as f64;

        let pre_delay = ModulatedDelay::new(buffer_size, (sr * 1.0) as usize, 100);
        let multitap = MultitapDiffuser::new(samplerate as usize);
        let diffuser = AllpassDiffuser::new(sr, 150.0, buffer_size);
        let high_pass = Hp1::new(sr);
        let low_pass = Lp1::new(sr);

        let mut lines = Vec::with_capacity(TOTAL_LINE_COUNT);
        for _ in 0..TOTAL_LINE_COUNT {
            lines.push(DelayLine::new(buffer_size, samplerate));
        }

        // ---- Velvet-Noise decorrelation --------------------------------
        // Each of the four VN filters in the plugin (2 channels × 2
        // stages) uses a **completely different base seed**, so all
        // four VN patterns are uncorrelated. This is the standard
        // VFDN arrangement (Fagerström et al., DAFx 2020): the four
        // diffusion patterns must be independent for the network to
        // reach full echo density and for the stereo image to be
        // genuinely wide.
        //
        // Channel-specific seed: left uses the "A" constant, right
        // uses "B". Input and output stages within a channel use the
        // channel seed XOR'd with a fixed mixing constant, which
        // produces an independent sequence without needing a third
        // magic number.
        const SEED_LEFT: u64 = 0x9E37_79B9_7F4A_7C15;
        const SEED_RIGHT: u64 = 0xA24B_AED4_963E_E407;
        const VN_OUT_XOR: u64 = 0x5555_5555_5555_5555;

        let channel_seed = match left_or_right {
            ChannelLR::Left => SEED_LEFT,
            ChannelLR::Right => SEED_RIGHT,
        };

        let mut velvet_in = VelvetNoiseFilter::with_seed_and_rate(channel_seed, sr);
        velvet_in.set_density(0.35);
        
        let mut velvet_out =
            VelvetNoiseFilter::with_seed_and_rate(channel_seed ^ VN_OUT_XOR, sr);
        velvet_out.set_density(0.65);

        let mut ch = Self {
            parameters: [0.0; PARAM_COUNT],
            samplerate: sr,
            buffer_size,
            pre_delay,
            multitap,
            diffuser,
            lines,
            high_pass,
            low_pass,
            velvet_in,
            velvet_out,
            velvet_amount: 0.0,
            velvet_tone: 0.5,
            temp_buffer: vec![0.0; buffer_size],
            early_buffer: vec![0.0; buffer_size],
            out_buffer: vec![0.0; buffer_size],
            delay_line_seed: 0,
            post_diffusion_seed: 0,
            delay_line_seeds: [0.0; TOTAL_LINE_COUNT * 3],
            line_count: 8,
            high_pass_enabled: false,
            low_pass_enabled: false,
            tap_enabled: true,
            diffuser_enabled: true,
            late_delay_enabled: true,
            dry_out: 1.0,
            predelay_out: 0.0,
            early_out: 0.5,
            line_out: 0.5,
            cross_seed: 0.0,
            channel_lr: left_or_right,
            frozen: false,
        };

        ch.high_pass.set_cutoff_hz(20.0);
        ch.low_pass.set_cutoff_hz(20000.0);
        ch.diffuser.set_interpolation_enabled(true);
        ch.recompute_line_seeds();
        ch
    }

    pub fn get_samplerate(&self) -> f64 {
        self.samplerate
    }

    pub fn set_samplerate(&mut self, samplerate: i32) {
        self.samplerate = samplerate as f64;
        self.high_pass.set_samplerate(self.samplerate);
        self.low_pass.set_samplerate(self.samplerate);
        for line in &mut self.lines {
            line.set_samplerate(self.samplerate);
        }
        // VN filters scale their sequences, LP coefficient, and RRS
        // window to the new rate.
        self.velvet_in.set_sample_rate(self.samplerate);
        self.velvet_out.set_sample_rate(self.samplerate);
        // Re-apply sample-rate dependent parameters (matches the C++ `SetSamplerate`).
        self.set_parameter(P_PREDELAY, self.parameters[P_PREDELAY]);
        self.set_parameter(P_TAP_LENGTH, self.parameters[P_TAP_LENGTH]);
        self.set_parameter(P_DIFFUSION_DELAY, self.parameters[P_DIFFUSION_DELAY]);
        self.set_parameter(P_LINE_DELAY, self.parameters[P_LINE_DELAY]);
        self.set_parameter(
            P_LATE_DIFFUSION_DELAY,
            self.parameters[P_LATE_DIFFUSION_DELAY],
        );
        self.set_parameter(
            P_EARLY_DIFFUSION_MOD_RATE,
            self.parameters[P_EARLY_DIFFUSION_MOD_RATE],
        );
        self.set_parameter(P_LINE_MOD_RATE, self.parameters[P_LINE_MOD_RATE]);
        self.set_parameter(
            P_LATE_DIFFUSION_MOD_RATE,
            self.parameters[P_LATE_DIFFUSION_MOD_RATE],
        );
        self.set_parameter(P_LINE_MOD_AMOUNT, self.parameters[P_LINE_MOD_AMOUNT]);
        self.update_lines();
    }

    pub fn get_output(&self) -> &[f64] {
        &self.out_buffer
    }

    /// Sets one (already scaled) parameter. `para` uses CloudReverb's
    /// `Parameter` indices.
    pub fn set_parameter(&mut self, para: usize, value: f64) {
        if para >= PARAM_COUNT {
            return;
        }
        self.parameters[para] = value;

        match para {
            P_PREDELAY => {
                self.pre_delay.sample_delay = self.ms2samples(value) as usize;
            }
            P_HIGH_PASS => self.high_pass.set_cutoff_hz(value),
            P_LOW_PASS => self.low_pass.set_cutoff_hz(value),
            P_TAP_ENABLED => {
                let new_val = value >= 0.5;
                if new_val != self.tap_enabled {
                    self.multitap.clear_buffers();
                }
                self.tap_enabled = new_val;
            }
            P_TAP_COUNT => self.multitap.set_tap_count(value as usize),
            P_TAP_LENGTH => {
                self.multitap
                    .set_tap_length(self.ms2samples(value) as usize);
            }
            P_TAP_GAIN => self.multitap.set_tap_gain(value),
            P_TAP_DECAY => self.multitap.set_tap_decay(value),
            P_DIFFUSION_ENABLED => {
                let new_val = value >= 0.5;
                if new_val != self.diffuser_enabled {
                    self.diffuser.clear_buffers();
                }
                self.diffuser_enabled = new_val;
            }
            P_DIFFUSION_STAGES => self.diffuser.stages = (value as usize).clamp(1, 8),
            P_DIFFUSION_DELAY => {
                self.diffuser.set_delay(self.ms2samples(value) as usize);
            }
            P_DIFFUSION_FEEDBACK => self.diffuser.set_feedback(value),
            P_LATE_DELAY_ENABLED => {
                let new_val = value >= 0.5;
                if new_val != self.late_delay_enabled && !new_val {
                    for line in &mut self.lines {
                        line.clear_buffers();
                    }
                }
                self.late_delay_enabled = new_val;
            }
            P_LINE_COUNT => {
                self.line_count = (value as usize).clamp(1, self.lines.len());
            }
            P_LINE_DELAY | P_LINE_DECAY => {
                self.update_lines();
            }
            P_LATE_DIFFUSION_ENABLED => {
                for line in &mut self.lines {
                    let new_val = value >= 0.5;
                    if new_val != line.diffuser_enabled {
                        line.clear_diffuser_buffer();
                    }
                    line.diffuser_enabled = new_val;
                }
            }
            P_LATE_DIFFUSION_STAGES => {
                for line in &mut self.lines {
                    line.set_diffuser_stages(value as usize);
                }
            }
            P_LATE_DIFFUSION_DELAY => {
                let samples = self.ms2samples(value) as usize;
                for line in &mut self.lines {
                    line.set_diffuser_delay(samples);
                }
            }
            P_LATE_DIFFUSION_FEEDBACK => {
                for line in &mut self.lines {
                    line.set_diffuser_feedback(value);
                }
            }
            P_POST_LOW_SHELF_GAIN => {
                for line in &mut self.lines {
                    line.set_low_shelf_gain(value);
                }
            }
            P_POST_LOW_SHELF_FREQUENCY => {
                for line in &mut self.lines {
                    line.set_low_shelf_frequency(value);
                }
            }
            P_POST_HIGH_SHELF_GAIN => {
                for line in &mut self.lines {
                    line.set_high_shelf_gain(value);
                }
            }
            P_POST_HIGH_SHELF_FREQUENCY => {
                for line in &mut self.lines {
                    line.set_high_shelf_frequency(value);
                }
            }
            P_POST_CUTOFF_FREQUENCY => {
                for line in &mut self.lines {
                    line.set_cutoff_frequency(value);
                }
            }
            P_EARLY_DIFFUSION_MOD_AMOUNT => {
                self.diffuser.set_modulation_enabled(value > 0.0);
                self.diffuser.set_mod_amount(self.ms2samples(value));
            }
            P_EARLY_DIFFUSION_MOD_RATE => self.diffuser.set_mod_rate(value),
            P_LINE_MOD_AMOUNT
            | P_LINE_MOD_RATE
            | P_LATE_DIFFUSION_MOD_AMOUNT
            | P_LATE_DIFFUSION_MOD_RATE => {
                self.update_lines();
            }
            P_TAP_SEED => self.multitap.set_seed(value as i64),
            P_DIFFUSION_SEED => self.diffuser.set_seed(value as i64),
            P_DELAY_SEED => {
                self.delay_line_seed = value as i64;
                self.recompute_line_seeds();
                self.update_lines();
            }
            P_POST_DIFFUSION_SEED => {
                self.post_diffusion_seed = value as i64;
                self.update_post_diffusion();
            }
            P_CROSS_SEED => {
                self.cross_seed = if self.channel_lr == ChannelLR::Right {
                    value
                } else {
                    0.0
                };
                self.multitap.set_cross_seed(value);
                self.diffuser.set_cross_seed(value);
                self.recompute_line_seeds();
                self.update_lines();
                self.update_post_diffusion();
            }
            P_DRY_OUT => self.dry_out = value,
            P_PREDELAY_OUT => self.predelay_out = value,
            P_EARLY_OUT => self.early_out = value,
            P_MAIN_OUT => self.line_out = value,
            P_HI_PASS_ENABLED => self.high_pass_enabled = value >= 0.5,
            P_LOW_PASS_ENABLED => self.low_pass_enabled = value >= 0.5,
            P_LOW_SHELF_ENABLED => {
                for line in &mut self.lines {
                    line.low_shelf_enabled = value >= 0.5;
                }
            }
            P_HIGH_SHELF_ENABLED => {
                for line in &mut self.lines {
                    line.high_shelf_enabled = value >= 0.5;
                }
            }
            P_CUTOFF_ENABLED => {
                for line in &mut self.lines {
                    line.cutoff_enabled = value >= 0.5;
                }
            }
            P_LATE_STAGE_TAP => {
                for line in &mut self.lines {
                    line.late_stage_tap = value >= 0.5;
                }
            }
            P_INTERPOLATION => {
                for line in &mut self.lines {
                    line.set_interpolation_enabled(value >= 0.5);
                }
            }
            _ => {}
        }
    }

    pub fn process(&mut self, input: &[f64]) {
        let n = input.len().min(self.buffer_size);

        // ---- input HP / LP ------------------------------------------------
        if self.high_pass_enabled {
            self.temp_buffer[..n].copy_from_slice(&input[..n]);
            self.high_pass.process_inplace(&mut self.temp_buffer[..n]);
        }
        if self.low_pass_enabled {
            if !self.high_pass_enabled {
                self.temp_buffer[..n].copy_from_slice(&input[..n]);
            }
            self.low_pass.process_inplace(&mut self.temp_buffer[..n]);
        }
        if !self.high_pass_enabled && !self.low_pass_enabled {
            self.temp_buffer[..n].copy_from_slice(&input[..n]);
        }

        // Completely zero if no input present (anti-CPU-spike gate, matches C++).
        for i in 0..n {
            let v = self.temp_buffer[i];
            if v * v < 0.000_000_001 {
                self.temp_buffer[i] = 0.0;
            }
        }

        // ---- pre-delay ----------------------------------------------------
        self.pre_delay.process(&self.temp_buffer[..n]);

        // ---- multitap (optional) ------------------------------------------
        if self.tap_enabled {
            let pre_out = self.pre_delay.get_output();
            self.multitap.process(&pre_out[..n]);
        }

        // ---- early diffuser (optional) ------------------------------------
        if self.tap_enabled {
            let src = self.multitap.get_output();
            self.early_buffer[..n].copy_from_slice(&src[..n]);
        } else {
            let src = self.pre_delay.get_output();
            self.early_buffer[..n].copy_from_slice(&src[..n]);
        }

        if self.diffuser_enabled {
            self.diffuser.process(&self.early_buffer[..n]);
        }

        if self.diffuser_enabled {
            let dst = self.diffuser.get_output();
            self.early_buffer[..n].copy_from_slice(&dst[..n]);
        }

        // Wet content going into the late delay lines equals the early stage.
        if self.frozen {
            self.temp_buffer[..n].fill(0.0);
            self.early_buffer[..n].fill(0.0);
        } else {
            self.temp_buffer[..n].copy_from_slice(&self.early_buffer[..n]);
        }

        // ---- INPUT Velvet-Noise diffusion --------------------------------
        // The signal feeding the FDN delay lines passes through the
        // input VN filter first. This decorrelates the incoming
        // samples before they enter the feedback network, which is
        // exactly the VFDN input-stage described by Fagerström et al.
        //
        // Because this filter is OUTSIDE the delay-line feedback
        // loops, its rippled magnitude response cannot accumulate:
        // the network is unconditionally stable regardless of the
        // amount value.
        let apply_velvet = self.velvet_amount > 0.0;
        if apply_velvet {
            for i in 0..n {
                self.temp_buffer[i] = self.velvet_in.process(self.temp_buffer[i]);
            }
        }

        // ---- late delay lines --------------------------------------------
        if self.late_delay_enabled {
            for i in 0..self.line_count {
                self.lines[i].process(&self.temp_buffer[..n]);
            }
            for i in 0..self.line_count {
                if i == 0 {
                    let out = self.lines[0].get_output();
                    self.temp_buffer[..n].copy_from_slice(&out[..n]);
                } else {
                    let out = self.lines[i].get_output();
                    for j in 0..n {
                        self.temp_buffer[j] += out[j];
                    }
                }
            }
            let per_line_gain = 1.0 / (self.line_count as f64).sqrt();
            for i in 0..n {
                self.temp_buffer[i] *= per_line_gain;
            }
        }

        // ---- output mix (with OUTPUT Velvet-Noise diffusion) -------------
        let predelay_out = self.pre_delay.get_output();

        for i in 0..n {
            // Wet sum: everything except the dry signal.
            let wet = self.predelay_out * predelay_out[i]
                + self.early_out * self.early_buffer[i]
                + self.line_out * self.temp_buffer[i];

            let wet_vn = if apply_velvet {
                self.velvet_out.process(wet)
            } else {
                wet
            };

            self.out_buffer[i] = self.dry_out * input[i] + wet_vn;
        }
    }

    pub fn clear_buffers(&mut self) {
        self.temp_buffer.fill(0.0);
        self.early_buffer.fill(0.0);
        self.out_buffer.fill(0.0);
        self.low_pass.reset();
        self.high_pass.reset();
        self.pre_delay.clear_buffers();
        self.multitap.clear_buffers();
        self.diffuser.clear_buffers();
        self.velvet_in.clear_buffers();
        self.velvet_out.clear_buffers();
        for line in &mut self.lines {
            line.clear_buffers();
        }
    }

    /// Enables / disables freeze. When on, the input to the delay lines
    /// is cut and the loop gain is pinned at ~1.0 so the tail holds
    /// indefinitely.
    pub fn set_freeze(&mut self, frozen: bool) {
        if self.frozen == frozen {
            return;
        }
        self.frozen = frozen;
        self.update_lines();
    }

    /// Sets the Velvet-Noise diffusion amount (0..1). Applied to BOTH
    /// the input and output VN filters of this channel.
    pub fn set_velvet_diffusion(&mut self, amount: f64) {
        self.velvet_amount = amount.clamp(0.0, 1.0);
        self.velvet_in.set_amount(self.velvet_amount);
        self.velvet_out.set_amount(self.velvet_amount);
    }

    /// Sets the VN pulse density as a continuous normalised value
    /// (0.0 = 1500 pulses/sec, 1.0 = 3000 pulses/sec). Both filters
    /// move together, but they intentionally track slightly different
    /// positions inside the range so their pulse patterns stay
    /// uncorrelated:
    ///
    ///   * input filter:  density × 0.85 + 0.00
    ///   * output filter: density × 0.85 + 0.15
    ///
    /// This ±7.5% offset is below the just-noticeable difference for
    /// density but large enough to break the symmetry between the
    /// two VN sequences, which is the whole point of the VFDN design.
    pub fn set_velvet_density(&mut self, density: f64) {
        let d = density.clamp(0.0, 1.0);
        let d_in = (d * 0.85 + 0.00).clamp(0.0, 1.0);
        let d_out = (d * 0.85 + 0.15).clamp(0.0, 1.0);
        self.velvet_in.set_density(d_in);
        self.velvet_out.set_density(d_out);
    }

    /// Sets the VN darkening amount (0..1). Applied to both the input
    /// and output VN filters of this channel.
    pub fn set_velvet_tone(&mut self, tone: f64) {
        let t = tone.clamp(0.0, 1.0);
        self.velvet_tone = t;
        self.velvet_in.set_tone(t);
        self.velvet_out.set_tone(t);
    }

    fn update_lines(&mut self) {
        let line_delay_samples = self.ms2samples(self.parameters[P_LINE_DELAY]) as usize as f64;
        let line_decay_millis = self.parameters[P_LINE_DECAY] * 1000.0;
        let line_decay_samples = self.ms2samples(line_decay_millis);

        let line_mod_amount = self.ms2samples(self.parameters[P_LINE_MOD_AMOUNT]);
        let line_mod_rate = self.parameters[P_LINE_MOD_RATE];

        let late_diffusion_mod_amount =
            self.ms2samples(self.parameters[P_LATE_DIFFUSION_MOD_AMOUNT]);
        let late_diffusion_mod_rate = self.parameters[P_LATE_DIFFUSION_MOD_RATE];

        let count = self.lines.len();

        for i in 0..count {
            let mod_amount = line_mod_amount * (0.7 + 0.3 * self.delay_line_seeds[i + count]);
            let mod_rate = line_mod_rate * (0.7 + 0.3 * self.delay_line_seeds[i + 2 * count])
                / self.samplerate;

            let mut delay_samples = (0.5 + 1.0 * self.delay_line_seeds[i]) * line_delay_samples;
            if delay_samples < mod_amount + 2.0 {
                delay_samples = mod_amount + 2.0;
            }

            let db_after_1_iteration = delay_samples / line_decay_samples * (-60.0);
            let gain_after_1_iteration = if self.frozen {
                0.999_95
            } else {
                10.0f64.powf(db_after_1_iteration / 20.0)
            };

            self.lines[i].set_delay(delay_samples as usize);
            self.lines[i].set_feedback(gain_after_1_iteration);
            self.lines[i].set_line_mod_amount(mod_amount);
            self.lines[i].set_line_mod_rate(mod_rate);
            self.lines[i].set_diffuser_mod_amount(late_diffusion_mod_amount);
            self.lines[i].set_diffuser_mod_rate(late_diffusion_mod_rate);
        }
    }

    fn update_post_diffusion(&mut self) {
        for i in 0..self.lines.len() {
            let seed = self.post_diffusion_seed.wrapping_mul(i as i64 + 1);
            self.lines[i].set_diffuser_seed(seed, self.cross_seed);
        }
    }

    fn recompute_line_seeds(&mut self) {
        sha_random::generate_cross_into(
            self.delay_line_seed,
            TOTAL_LINE_COUNT * 3,
            self.cross_seed,
            &mut self.delay_line_seeds,
        );
    }

    fn ms2samples(&self, ms: f64) -> f64 {
        ms / 1000.0 * self.samplerate
    }
}
