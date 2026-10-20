//! ReverbController – GPL-3.0
//!
//! Faithful Rust port of `CloudSeed.Native/ReverbController.h` (legacy-v1).
//! Owns one `ReverbChannel` per side. Parameters are stored normalised
//! (0..1) and converted to physical values through `scaled_parameter_value`,
//! exactly like the original.
//!
//! On top of the CloudSeed port, this module hosts the PEJVAK Texture
//! Rack (six post-reverb layers) and the Velvet-Noise diffusion stage.
//! Both are exposed through a single typed entry point,
//! `set_texture_parameter(TextureParamId, f64)`, so the plugin never
//! needs to build strings or perform dynamic type lookups on the audio
//! thread.

use crate::cloudseed::allpass_diffuser::AllpassDiffuser;
use crate::cloudseed::hp1::Hp1;
use crate::cloudseed::lp1::Lp1;
use crate::cloudseed::modulated_delay::ModulatedDelay;
use crate::cloudseed::params::{scaled_parameter_value, PARAM_COUNT};
use crate::cloudseed::reverb_channel::{ChannelLR, ReverbChannel};
use crate::ffdn::FfdnCore;
use crate::texture::illusion::Illusion;
use crate::texture::{TextureLayer, TextureRack};
use wide::f64x4;

/// Internal block buffer size (matches the original `bufferSize = 128`).
pub const BUFFER_SIZE: usize = 4096;

/// Selects which reverb engine is currently active.
///
/// The two engines are architecturally independent: CloudSeed uses
/// twelve parallel delay lines with a deep diffusion front-end, while
/// FFDN uses a compact network of eight delay lines joined by a
/// lossless feedback matrix. Switching engines mid-session is safe;
/// the audio thread picks up the change on the next block.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReverbEngine {
    /// The original CloudSeed legacy-v1 port.
    CloudSeed,
    /// The new Filtered Feedback Delay Network.
    Ffdn,
}

/// Number of delay lines in the FFDN engine. Chosen to give dense
/// echo patterns with modest CPU cost. Matches the size of the
/// Householder feedback matrix, which is O(N) per sample, so going
/// higher is cheap but rarely improves perceptual density.
const FFDN_LINES: usize = 8;

// ============================================================================
// Texture parameter identifiers
// ============================================================================

/// Typed identifier for every parameter that is NOT part of the 49-param
/// CloudSeed array. Covers the six Texture Rack layers plus the three
/// Velvet-Noise diffusion controls.
///
/// The enum replaces the previous string-based dispatch for three
/// reasons:
///
///   1. **Compile-time exhaustiveness.** Every `match` on this enum is
///      checked by the compiler; adding a new parameter forces every
///      dispatch site to be updated.
///
///   2. **Zero-cost indexing.** `TextureParamId::index()` returns the
///      discriminant, so the plugin's cache array (`[f64; 32]`) can be
///      indexed directly without a hash lookup or a string comparison.
///
///   3. **No `TypeId` on the audio thread.** The previous design called
///      `get_*_mut()` six times per parameter (once per layer) to find
///      the right downcast target. The typed dispatch calls exactly one
///      accessor per parameter.
///
/// The numeric values are part of the public ABI of the cache array, so
/// they must not be reordered without updating `lib.rs` in lockstep.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(usize)]
pub enum TextureParamId {
    // ---- Shimmer --------------------------------------------------------
    ShimmerMix = 0,
    ShimmerPitch,
    ShimmerFeedback,
    ShimmerLowCut,
    ShimmerHighCut,

    // ---- Mist -----------------------------------------------------------
    MistMix,
    MistHP,
    MistFeedback,
    MistDelay,
    MistRate,

    // ---- Drift ----------------------------------------------------------
    DriftMix,
    DriftFeedback,
    DriftDelay,
    DriftRate,
    DriftDepth,

    // ---- Crystal --------------------------------------------------------
    CrystalMix,
    CrystalSize,
    CrystalDensity,
    CrystalFeedback,

    // ---- Scatter --------------------------------------------------------
    ScatterMix,
    ScatterSpread,
    ScatterDensity,
    ScatterSpeed,
    ScatterFeedback,

    // ---- Bloom ----------------------------------------------------------
    BloomMix,
    BloomRise,
    BloomSize,
    BloomFeedback,
    BloomTone,

    // ---- Velvet-Noise diffusion (per-channel, not a Texture layer) ------
    VelvetDiffusion,
    VelvetDensity,
    VelvetTone,
}

impl TextureParamId {
    /// Total number of texture + velvet parameters.
    ///
    /// Used to size the plugin-side cache array. Must match the number
    /// of enum variants above; a compile-time assert in `lib.rs` keeps
    /// the two in sync.
    pub const COUNT: usize = 32;

    /// Numeric index of this parameter, matching its position in the
    /// plugin cache array. Provided as an inherent method so callers do
    /// not need a `as usize` cast.
    #[inline(always)]
    pub fn index(self) -> usize {
        self as usize
    }

    /// Returns true if this parameter controls a layer's `mix` value.
    ///
    /// Only `Mix` parameters can change the rack's `active` flag, so
    /// the controller only triggers `TextureRack::recompute_active()`
    /// for these. This is a public helper because the plugin may want
    /// to know the same thing when deciding whether to invalidate a
    /// cached value.
    #[inline(always)]
    pub fn is_mix(self) -> bool {
        matches!(
            self,
            Self::ShimmerMix
                | Self::MistMix
                | Self::DriftMix
                | Self::CrystalMix
                | Self::ScatterMix
                | Self::BloomMix
        )
    }
}

// ============================================================================
// ReverbController
// ============================================================================

pub struct ReverbController {
    samplerate: i32,
    channel_l: ReverbChannel,
    channel_r: ReverbChannel,
    left_channel_in: Vec<f64>,
    right_channel_in: Vec<f64>,
    parameters: [f64; PARAM_COUNT],

    // ---- Output-stage smoothing ---------------------------------
    // These are used by both engines:
    //   * `smoothed_width` is applied in the final M/S stage of
    //     both engines.
    //   * `smoothed_dry_gain` and `smoothed_wet_gain` are used only
    //     by the FFDN path (the CloudSeed path smooths its own
    //     per-channel gains inside `ReverbChannel`).
    smoothed_width: f64,
    smoothed_dry_gain: f64,
    smoothed_wet_gain: f64,
    smooth_coeff: f64,
    // Scratch buffers for the per-sample smoothed values, so they
    // can be passed as slices to the static output helpers.
    smooth_width_scratch: Vec<f64>,
    smooth_dry_scratch: Vec<f64>,

    frozen: bool,
    texture_rack: TextureRack,

    /// ILLUSION: pitched reverb sequencer. Runs *after* the Texture Rack
    /// on the wet signal, so it always sees the fully processed reverb
    /// texture. It is deliberately not part of the reorderable chain.
    illusion: Illusion,

    // ---- FFDN engine -------------------------------------------------
    /// The FFDN instance. Always allocated, even when the CloudSeed
    /// engine is active, so that switching engines does not need to
    /// allocate on the audio thread.
    ffdn: FfdnCore,
    /// Which engine processes the current block.
    engine: ReverbEngine,
    /// Stereo staging buffers for the FFDN path. FFDN works on f64
    /// slices; the plugin boundary converts f32 → f64 → f32.
    ffdn_buf_l: Vec<f64>,
    ffdn_buf_r: Vec<f64>,
    /// Cached FFDN parameter values, so we only call the (relatively
    /// expensive) internal recompute methods when a value actually
    /// changes.
    ffdn_size_ms: f64,
    ffdn_decay_sec: f64,
    ffdn_damping_hz: f64,
    ffdn_spread: f64,

    // ---- FFDN input-stage filters -----------------------------------
    // The CloudSeed engine applies HP, LP, and PreDelay inside each
    // `ReverbChannel`. The FFDN has no equivalent internal stage, so
    // the controller owns a parallel copy for the FFDN path. These
    // filters are only touched when the corresponding CloudSeed
    // parameters change (see `set_parameter`).
    ffdn_pre_delay_l: ModulatedDelay,
    ffdn_pre_delay_r: ModulatedDelay,
    ffdn_hp_l: Hp1,
    ffdn_hp_r: Hp1,
    ffdn_lp_l: Lp1,
    ffdn_lp_r: Lp1,
    ffdn_hp_enabled: bool,
    ffdn_lp_enabled: bool,

    // ---- FFDN input diffusion ---------------------------------------
    // Two independent allpass cascades (one per channel), placed
    // between the input-stage filters and the FDN network itself.
    //
    // `AllpassDiffuser` is the same 4-stage cascade that CloudSeed
    // uses for its early-diffusion stage; it's a robust, well-tested
    // implementation. We seed L and R with different values so the
    // two channels receive genuinely different diffusion patterns.
    //
    // When `ffdn_diffusion < 0.001` the cascade is skipped entirely
    // and the signal path is bit-identical to a plain FFDN.
    ffdn_diff_l: AllpassDiffuser,
    ffdn_diff_r: AllpassDiffuser,
    ffdn_diffusion: f64,

    // ---- Mono scratch (NEW) -----------------------------------------
    // Scratch stereo f32 buffers used only by `process_mono_inplace`
    // when the FFDN engine is active. The plugin's mono fallback path
    // has no stereo input to give us, so we duplicate the mono signal
    // into these two buffers, run the normal stereo FFDN path on
    // them, then sum the result back into `mono`.
    //
    // Allocated once in `new()`; never reallocated. Sized to
    // `BUFFER_SIZE` so any block the plugin hands us fits.
    mono_stereo_l: Vec<f32>,
    mono_stereo_r: Vec<f32>,
}

impl ReverbController {
    pub fn new(samplerate: i32) -> Self {
        let texture_rack = TextureRack::new(BUFFER_SIZE, samplerate as f64);
        let ffdn = FfdnCore::new(FFDN_LINES, BUFFER_SIZE, samplerate as f64);

        // FFDN input-stage filters. Same buffer sizes and defaults
        // as the CloudSeed channels' internal copies.
        let sr = samplerate as f64;
        let max_pre_delay_samples = (sr * 1.0) as usize; // 1 s headroom
        let ffdn_pre_delay_l = ModulatedDelay::new(BUFFER_SIZE, max_pre_delay_samples, 100);
        let ffdn_pre_delay_r = ModulatedDelay::new(BUFFER_SIZE, max_pre_delay_samples, 100);
        let ffdn_hp_l = Hp1::new(sr);
        let ffdn_hp_r = Hp1::new(sr);
        let ffdn_lp_l = Lp1::new(sr);
        let ffdn_lp_r = Lp1::new(sr);

        // FFDN input-diffusion cascades. Two very different seeds so
        // the L and R allpass sequences are uncorrelated. Base delay
        // is 15 ms, which produces per-stage delays of ~1.5 to 15 ms
        // (each stage's actual delay is the base times a per-stage
        // multiplier in [0.1, 1.0] derived from the seed).
        const DIFF_SEED_L: i64 = 0x1234_5678_9ABC_DEF0u64 as i64;
        const DIFF_SEED_R: i64 = 0x0FED_CBA9_8765_4321u64 as i64;
        let diff_delay_samples = (sr * 0.015) as usize;

        let mut ffdn_diff_l = AllpassDiffuser::new(sr, 150.0, BUFFER_SIZE);
        ffdn_diff_l.set_seed(DIFF_SEED_L);
        ffdn_diff_l.set_cross_seed(0.0);
        ffdn_diff_l.stages = 4;
        ffdn_diff_l.set_modulation_enabled(false);
        ffdn_diff_l.set_delay(diff_delay_samples);

        let mut ffdn_diff_r = AllpassDiffuser::new(sr, 150.0, BUFFER_SIZE);
        ffdn_diff_r.set_seed(DIFF_SEED_R);
        ffdn_diff_r.set_cross_seed(0.0);
        ffdn_diff_r.stages = 4;
        ffdn_diff_r.set_modulation_enabled(false);
        ffdn_diff_r.set_delay(diff_delay_samples);

        Self {
            samplerate,
            channel_l: ReverbChannel::new(BUFFER_SIZE, samplerate, ChannelLR::Left),
            channel_r: ReverbChannel::new(BUFFER_SIZE, samplerate, ChannelLR::Right),
            left_channel_in: vec![0.0; BUFFER_SIZE],
            right_channel_in: vec![0.0; BUFFER_SIZE],
            parameters: [0.0; PARAM_COUNT],
            smoothed_width: f64::NAN,
            smoothed_dry_gain: f64::NAN,
            smoothed_wet_gain: f64::NAN,
            smooth_coeff: 1.0 - (-1.0 / (sr * 0.010)).exp(),
            smooth_width_scratch: vec![0.0; BUFFER_SIZE],
            smooth_dry_scratch: vec![0.0; BUFFER_SIZE],
            frozen: false,
            texture_rack,
            illusion: Illusion::new(samplerate as f64),
            ffdn,
            engine: ReverbEngine::CloudSeed,
            ffdn_buf_l: vec![0.0; BUFFER_SIZE],
            ffdn_buf_r: vec![0.0; BUFFER_SIZE],
            ffdn_size_ms: 50.0,
            ffdn_decay_sec: 3.0,
            ffdn_damping_hz: 8000.0,
            ffdn_spread: 0.5,
            ffdn_pre_delay_l,
            ffdn_pre_delay_r,
            ffdn_hp_l,
            ffdn_hp_r,
            ffdn_lp_l,
            ffdn_lp_r,
            ffdn_hp_enabled: false,
            ffdn_lp_enabled: false,
            ffdn_diff_l,
            ffdn_diff_r,
            ffdn_diffusion: 0.0,
            mono_stereo_l: vec![0.0; BUFFER_SIZE],
            mono_stereo_r: vec![0.0; BUFFER_SIZE],
        }
    }

    /// Enables / disables freeze.
    ///
    /// Forwards to both the CloudSeed channels and the FFDN network.
    /// The CloudSeed path cuts the input to the delay lines and pins
    /// their loop gains near unity; the FFDN path does the equivalent
    /// on its own two networks. Dry signal continues to pass through
    /// in both cases.
    pub fn set_freeze(&mut self, frozen: bool) {
        if self.frozen == frozen {
            return;
        }
        self.frozen = frozen;
        self.channel_l.set_freeze(frozen);
        self.channel_r.set_freeze(frozen);
        self.ffdn.set_freeze(frozen);
    }

    pub fn get_samplerate(&self) -> i32 {
        self.samplerate
    }

    pub fn set_samplerate(&mut self, samplerate: i32) {
        self.samplerate = samplerate;
        self.smooth_coeff = 1.0 - (-1.0 / (samplerate as f64 * 0.010)).exp();
        self.channel_l.set_samplerate(samplerate);
        self.channel_r.set_samplerate(samplerate);
        self.texture_rack.set_samplerate(samplerate as f64);
        self.illusion.set_samplerate(samplerate as f64);
        self.ffdn.set_sample_rate(samplerate as f64);

        // FFDN input-stage filters need their sample rate refreshed
        // and their frequency-derived coefficients recomputed.
        let sr = samplerate as f64;
        self.ffdn_hp_l.set_samplerate(sr);
        self.ffdn_hp_r.set_samplerate(sr);
        self.ffdn_lp_l.set_samplerate(sr);
        self.ffdn_lp_r.set_samplerate(sr);

        // Pre-delay length is stored in samples, so recompute from
        // the persisted millisecond value.
        let predelay_ms = scaled_parameter_value(1, self.parameters[1]);
        let samples = (predelay_ms / 1000.0 * sr) as usize;
        self.ffdn_pre_delay_l.sample_delay = samples;
        self.ffdn_pre_delay_r.sample_delay = samples;

        // HP/LP cutoff frequencies are stored as Hz inside the
        // filter objects; re-set them from the physical values so
        // that the coefficients are correct at the new rate.
        let hp_hz = scaled_parameter_value(2, self.parameters[2]);
        let lp_hz = scaled_parameter_value(3, self.parameters[3]);
        self.ffdn_hp_l.set_cutoff_hz(hp_hz);
        self.ffdn_hp_r.set_cutoff_hz(hp_hz);
        self.ffdn_lp_l.set_cutoff_hz(lp_hz);
        self.ffdn_lp_r.set_cutoff_hz(lp_hz);

        // Diffusers: 15 ms base delay at the new rate.
        let diff_delay_samples = (sr * 0.015) as usize;
        self.ffdn_diff_l.set_samplerate(sr);
        self.ffdn_diff_l.set_delay(diff_delay_samples);
        self.ffdn_diff_r.set_samplerate(sr);
        self.ffdn_diff_r.set_delay(diff_delay_samples);
    }

    pub fn get_parameter_count(&self) -> usize {
        PARAM_COUNT
    }

    pub fn get_all_parameters(&self) -> &[f64; PARAM_COUNT] {
        &self.parameters
    }

    /// Current normalised value of one parameter.
    pub fn get_parameter(&self, index: usize) -> f64 {
        self.parameters[index.min(PARAM_COUNT - 1)]
    }

    /// Converts a normalised parameter to its physical value, matching
    /// `ReverbController::GetScaledParameter` in C++.
    pub fn get_scaled_parameter(&self, index: usize) -> f64 {
        if index >= PARAM_COUNT {
            return 0.0;
        }
        scaled_parameter_value(index, self.parameters[index])
    }

    /// Stores the normalised value and forwards the scaled value to both channels.
    pub fn set_parameter(&mut self, index: usize, value: f64) {
        if index >= PARAM_COUNT {
            return;
        }
        self.parameters[index] = value;

        // Width is handled by the controller itself; not forwarded.
        // The target is stored in `self.parameters[41]` (already set
        // above). The smoothed value is advanced per-sample in
        // `process_*_inplace` and used to fill `smooth_width_scratch`.
        if index == 41 {
            return;
        }

        let scaled = self.get_scaled_parameter(index);
        self.channel_l.set_parameter(index, scaled);
        self.channel_r.set_parameter(index, scaled);

        // Mirror the shared input-stage parameters onto the FFDN's
        // independent copies. Only PreDelay, HighPass, LowPass, and
        // the two enable switches are relevant — the FFDN has no
        // use for the CloudSeed-specific diffusion/delay-line params.
        match index {
            1 => {
                // PreDelay [ms] -> samples.
                let samples = (scaled / 1000.0 * self.samplerate as f64) as usize;
                self.ffdn_pre_delay_l.sample_delay = samples;
                self.ffdn_pre_delay_r.sample_delay = samples;
            }
            2 => {
                self.ffdn_hp_l.set_cutoff_hz(scaled);
                self.ffdn_hp_r.set_cutoff_hz(scaled);
            }
            3 => {
                self.ffdn_lp_l.set_cutoff_hz(scaled);
                self.ffdn_lp_r.set_cutoff_hz(scaled);
            }
            42 => self.ffdn_hp_enabled = scaled >= 0.5,
            43 => self.ffdn_lp_enabled = scaled >= 0.5,
            _ => {}
        }
    }

    /// Sets a Texture-Rack or Velvet-Noise parameter.
    ///
    /// This bypasses the 49-param CloudSeed array so that existing
    /// presets keep working unchanged: texture parameters live in a
    /// separate namespace, persisted separately by the plugin.
    ///
    /// # Performance
    ///
    /// The dispatch is a single `match` on a `#[repr(usize)]` enum, so
    /// the compiler turns it into a jump table. Each arm touches exactly
    /// one layer, and `recompute_active()` is only called when the
    /// parameter can actually affect the rack's active flag (i.e. when
    /// it is one of the six `*Mix` variants). Non-mix changes are
    /// therefore O(1) with no rack-wide scan.
    #[inline]
    pub fn set_texture_parameter(&mut self, id: TextureParamId, value: f64) {
        match id {
            // ---- Shimmer -------------------------------------------------
            TextureParamId::ShimmerMix => {
                if let Some(s) = self.texture_rack.get_shimmer_mut() {
                    s.set_mix(value);
                }
            }
            TextureParamId::ShimmerPitch => {
                if let Some(s) = self.texture_rack.get_shimmer_mut() {
                    s.set_pitch_norm(value);
                }
            }
            TextureParamId::ShimmerFeedback => {
                if let Some(s) = self.texture_rack.get_shimmer_mut() {
                    s.set_feedback(value);
                }
            }
            TextureParamId::ShimmerLowCut => {
                let hz = 20.0 + value.clamp(0.0, 1.0) * 1980.0;
                if let Some(s) = self.texture_rack.get_shimmer_mut() {
                    s.set_low_cut_hz(hz);
                }
            }
            TextureParamId::ShimmerHighCut => {
                let hz = 200.0 + value.clamp(0.0, 1.0) * 15000.0;
                if let Some(s) = self.texture_rack.get_shimmer_mut() {
                    s.set_high_cut_hz(hz);
                }
            }

            // ---- Mist ----------------------------------------------------
            TextureParamId::MistMix => {
                if let Some(m) = self.texture_rack.get_mist_mut() {
                    m.set_mix(value);
                }
                self.texture_rack.recompute_active();
            }
            TextureParamId::MistHP => {
                let hz = 100.0 + value.clamp(0.0, 1.0) * 2900.0;
                if let Some(m) = self.texture_rack.get_mist_mut() {
                    m.set_hp_hz(hz);
                }
            }
            TextureParamId::MistFeedback => {
                if let Some(m) = self.texture_rack.get_mist_mut() {
                    m.set_feedback(value);
                }
            }
            TextureParamId::MistDelay => {
                if let Some(m) = self.texture_rack.get_mist_mut() {
                    m.set_delay_norm(value);
                }
            }
            TextureParamId::MistRate => {
                if let Some(m) = self.texture_rack.get_mist_mut() {
                    m.set_rate_norm(value);
                }
            }

            // ---- Drift ---------------------------------------------------
            TextureParamId::DriftMix => {
                if let Some(d) = self.texture_rack.get_drift_mut() {
                    d.set_mix(value);
                }
                self.texture_rack.recompute_active();
            }
            TextureParamId::DriftFeedback => {
                if let Some(d) = self.texture_rack.get_drift_mut() {
                    d.set_feedback(value);
                }
            }
            TextureParamId::DriftDelay => {
                if let Some(d) = self.texture_rack.get_drift_mut() {
                    d.set_delay_norm(value);
                }
            }
            TextureParamId::DriftRate => {
                if let Some(d) = self.texture_rack.get_drift_mut() {
                    d.set_rate_norm(value);
                }
            }
            TextureParamId::DriftDepth => {
                if let Some(d) = self.texture_rack.get_drift_mut() {
                    d.set_depth_norm(value);
                }
            }

            // ---- Crystal -------------------------------------------------
            TextureParamId::CrystalMix => {
                if let Some(c) = self.texture_rack.get_crystal_mut() {
                    c.set_mix(value);
                }
                self.texture_rack.recompute_active();
            }
            TextureParamId::CrystalSize => {
                if let Some(c) = self.texture_rack.get_crystal_mut() {
                    c.set_grain_len_norm(value);
                }
            }
            TextureParamId::CrystalDensity => {
                if let Some(c) = self.texture_rack.get_crystal_mut() {
                    c.set_density_norm(value);
                }
            }
            TextureParamId::CrystalFeedback => {
                if let Some(c) = self.texture_rack.get_crystal_mut() {
                    c.set_feedback(value);
                }
            }

            // ---- Scatter -------------------------------------------------
            TextureParamId::ScatterMix => {
                if let Some(s) = self.texture_rack.get_scatter_mut() {
                    s.set_mix(value);
                }
                self.texture_rack.recompute_active();
            }
            TextureParamId::ScatterSpread => {
                if let Some(s) = self.texture_rack.get_scatter_mut() {
                    s.set_spread_norm(value);
                }
            }
            TextureParamId::ScatterDensity => {
                if let Some(s) = self.texture_rack.get_scatter_mut() {
                    s.set_density_norm(value);
                }
            }
            TextureParamId::ScatterSpeed => {
                if let Some(s) = self.texture_rack.get_scatter_mut() {
                    s.set_speed_norm(value);
                }
            }
            TextureParamId::ScatterFeedback => {
                if let Some(s) = self.texture_rack.get_scatter_mut() {
                    s.set_feedback(value);
                }
            }

            // ---- Bloom ---------------------------------------------------
            TextureParamId::BloomMix => {
                if let Some(b) = self.texture_rack.get_bloom_mut() {
                    b.set_mix(value);
                }
                self.texture_rack.recompute_active();
            }
            TextureParamId::BloomRise => {
                if let Some(b) = self.texture_rack.get_bloom_mut() {
                    b.set_rise_norm(value);
                }
            }
            TextureParamId::BloomSize => {
                if let Some(b) = self.texture_rack.get_bloom_mut() {
                    b.set_size_norm(value);
                }
            }
            TextureParamId::BloomFeedback => {
                if let Some(b) = self.texture_rack.get_bloom_mut() {
                    b.set_feedback(value);
                }
            }
            TextureParamId::BloomTone => {
                if let Some(b) = self.texture_rack.get_bloom_mut() {
                    b.set_tone_norm(value);
                }
            }

            // ---- Velvet Noise (per-channel, not a Texture layer) ---------
            TextureParamId::VelvetDiffusion => {
                self.channel_l.set_velvet_diffusion(value);
                self.channel_r.set_velvet_diffusion(value);
            }
            TextureParamId::VelvetDensity => {
                let density = value.clamp(0.0, 1.0);
                self.channel_l.set_velvet_density(density);
                self.channel_r.set_velvet_density(density);
            }
            TextureParamId::VelvetTone => {
                let tone = value.clamp(0.0, 1.0);
                self.channel_l.set_velvet_tone(tone);
                self.channel_r.set_velvet_tone(tone);
            }
        }
    }

    /// Sets the processing order of the Texture Rack layers.
    ///
    /// `order[i]` is the index of the layer to process at step `i`,
    /// where the layer indices are:
    ///   0 = Shimmer, 1 = Mist, 2 = Drift,
    ///   3 = Crystal, 4 = Scatter, 5 = Bloom.
    ///
    /// The array must be a valid permutation of `0..6`; invalid input
    /// is ignored. Called from the audio thread once per block when
    /// the plugin detects that the order has changed.
    #[inline]
    pub fn set_texture_order(&mut self, order: [usize; 6]) {
        self.texture_rack.set_order(order);
    }

    /// Selects the active reverb engine.
    ///
    pub fn set_engine(&mut self, engine: ReverbEngine) {
        if self.engine == engine {
            return;
        }

        match self.engine {
            ReverbEngine::CloudSeed => {
                self.channel_l.clear_buffers();
                self.channel_r.clear_buffers();
            }
            ReverbEngine::Ffdn => {
                self.ffdn.clear();
                self.ffdn_pre_delay_l.clear_buffers();
                self.ffdn_pre_delay_r.clear_buffers();
                self.ffdn_hp_l.reset();
                self.ffdn_hp_r.reset();
                self.ffdn_lp_l.reset();
                self.ffdn_lp_r.reset();
                self.ffdn_diff_l.clear_buffers();
                self.ffdn_diff_r.clear_buffers();
            }
        }

        self.engine = engine;
    }

    /// Returns the currently active engine.
    pub fn engine(&self) -> ReverbEngine {
        self.engine
    }

    /// Sets the FFDN base delay in milliseconds. Values below 5 ms
    /// produce metallic flutter; values above 200 ms sound like
    /// discrete echoes rather than a reverb.
    pub fn set_ffdn_size_ms(&mut self, ms: f64) {
        if (ms - self.ffdn_size_ms).abs() < 1e-6 {
            return;
        }
        self.ffdn_size_ms = ms;
        self.ffdn.set_base_delay_ms(ms);
    }

    /// Sets the FFDN target RT60 in seconds.
    pub fn set_ffdn_decay_sec(&mut self, seconds: f64) {
        if (seconds - self.ffdn_decay_sec).abs() < 1e-6 {
            return;
        }
        self.ffdn_decay_sec = seconds;
        self.ffdn.set_decay_seconds(seconds);
    }

    /// Sets the FFDN one-pole damping cutoff frequency in Hz.
    pub fn set_ffdn_damping_hz(&mut self, hz: f64) {
        if (hz - self.ffdn_damping_hz).abs() < 1e-6 {
            return;
        }
        self.ffdn_damping_hz = hz;
        self.ffdn.set_damping_hz(hz);
    }

    /// Sets the FFDN delay spread (0..1). 0 makes every line the same
    /// length; 1 spreads them across ±50 % of the base delay.
    pub fn set_ffdn_spread(&mut self, spread: f64) {
        if (spread - self.ffdn_spread).abs() < 1e-6 {
            return;
        }
        self.ffdn_spread = spread;
        self.ffdn.set_delay_spread(spread);
    }

    /// Sets the FFDN input-diffusion amount (0..1).
    ///
    /// At `0.0` the allpass cascade is bypassed entirely and the
    /// signal path is bit-identical to a plain FFDN. At `1.0` the
    /// cascade's feedback reaches 0.7, which produces a strong
    /// smoothing of the initial transient without ever becoming
    /// metallic.
    ///
    /// This method does **not** allocate — only the feedback gains
    /// of the already-allocated allpass stages are updated.
    pub fn set_ffdn_diffusion(&mut self, amount: f64) {
        self.ffdn_diffusion = amount.clamp(0.0, 1.0);
        // Map 0..1 to a feedback of 0.0..0.7.
        let g = self.ffdn_diffusion * 0.7;
        self.ffdn_diff_l.set_feedback(g);
        self.ffdn_diff_r.set_feedback(g);
    }

    /// Sets the FFDN modulation depth in milliseconds (0..5).
    ///
    /// No local cache is kept here: the plugin-level `last_ffdn`
    /// array already filters out no-op calls, and `FfdnCore` stores
    /// the value internally.
    pub fn set_ffdn_mod_amount_ms(&mut self, ms: f64) {
        self.ffdn.set_mod_amount_ms(ms);
    }

    /// Sets the FFDN modulation rate in Hz (0.05..10).
    pub fn set_ffdn_mod_rate_hz(&mut self, hz: f64) {
        self.ffdn.set_mod_rate_hz(hz);
    }

    /// Selects the FFDN feedback matrix type.
    /// `false` = Householder (dense), `true` = Velvet (sparse).
    pub fn set_ffdn_matrix_type(&mut self, is_velvet: bool) {
        self.ffdn.set_matrix_type(is_velvet);
    }

    /// Sets the FFDN's frequency-dependent attenuation parameters.
    ///
    /// * `bass_mult`: `T60_bass / T60_mid`. 1.0 = neutral.
    /// * `treble_mult`: `T60_treble / T60_mid`. 1.0 = neutral.
    /// * `bass_freq`: crossover of the low shelf, Hz.
    /// * `treble_freq`: crossover of the high shelf, Hz.
    pub fn set_ffdn_fda(
        &mut self,
        bass_mult: f64,
        treble_mult: f64,
        bass_freq: f64,
        treble_freq: f64,
    ) {
        self.ffdn
            .set_fda(bass_mult, treble_mult, bass_freq, treble_freq);
    }

    // ============================================================
    // FFDN CHARACTER — pass-through to FfdnCore
    // ============================================================
    //
    // These four setters simply forward to the matching methods on
    // `FfdnCore`. They exist on the controller so the plugin's
    // `process_inner` never has to reach into `self.ffdn` directly,
    // which would break the encapsulation the controller uses to
    // keep its caching straightforward.

    /// Sets the FFDN cross-feed amount (0..1).
    pub fn set_ffdn_cross_feed(&mut self, v: f64) {
        self.ffdn.set_cross_feed(v);
    }

    /// Sets the FFDN feedback drive amount (0..1).
    pub fn set_ffdn_drive(&mut self, v: f64) {
        self.ffdn.set_drive(v);
    }

    /// Sets the FFDN tilt filter parameters.
    ///
    /// * `freq`   — crossover frequency, Hz (100..10000).
    /// * `amount` — 0.0 = bass-heavy, 0.5 = flat, 1.0 = treble-heavy.
    pub fn set_ffdn_tilt(&mut self, freq: f64, amount: f64) {
        self.ffdn.set_tilt(freq, amount);
    }

    /// Sets the FFDN early-reflection blend (0..1).
    pub fn set_ffdn_er_level(&mut self, v: f64) {
        self.ffdn.set_er_level(v);
    }

    /// Enables / disables the FFDN feedback drive stage.
    pub fn set_ffdn_drive_enabled(&mut self, enabled: bool) {
        self.ffdn.set_drive_enabled(enabled);
    }

    /// Enables / disables the FFDN tilt filter.
    pub fn set_ffdn_tilt_enabled(&mut self, enabled: bool) {
        self.ffdn.set_tilt_enabled(enabled);
    }

    /// Enables / disables the FFDN early-reflection mix.
    pub fn set_ffdn_er_enabled(&mut self, enabled: bool) {
        self.ffdn.set_er_enabled(enabled);
    }

    // ============================================================
    // FFDN NETWORK / FILTER / DYNAMICS — pass-through
    // ============================================================
    //
    // Nine additional parameters, added in. Like the
    // CHARACTER setters above, these simply forward to the matching
    // methods on `FfdnCore`. At  the core only stores them;
    // the DSP ships phase by phase.

    /// Sets the number of active delay lines per FFDN network (4..16).
    pub fn set_ffdn_lines(&mut self, n: usize) {
        self.ffdn.set_lines(n);
    }

    /// Sets the FFDN input distribution density (0..1).
    pub fn set_ffdn_density(&mut self, v: f64) {
        self.ffdn.set_density(v);
    }

    /// Sets the FFDN L/R network delay offset amount (0..1).
    pub fn set_ffdn_decorr(&mut self, v: f64) {
        self.ffdn.set_decorr(v);
    }

    /// Sets the FFDN loop high-pass cutoff in Hz.
    pub fn set_ffdn_loop_hp(&mut self, hz: f64) {
        self.ffdn.set_loop_hp(hz);
    }

    /// Sets the FFDN loop low-pass cutoff in Hz.
    pub fn set_ffdn_loop_lp(&mut self, hz: f64) {
        self.ffdn.set_loop_lp(hz);
    }

    /// Sets the FFDN mid-band emphasis (0..1).
    pub fn set_ffdn_mid_focus(&mut self, v: f64) {
        self.ffdn.set_mid_focus(v);
    }

    /// Sets the FFDN attack envelope amount (0..1).
    pub fn set_ffdn_attack(&mut self, v: f64) {
        self.ffdn.set_attack(v);
    }

    /// Sets the FFDN ducking amount (0..1).
    pub fn set_ffdn_ducking(&mut self, v: f64) {
        self.ffdn.set_ducking(v);
    }

    /// Sets the FFDN gate threshold (0..1).
    pub fn set_ffdn_gate(&mut self, v: f64) {
        self.ffdn.set_gate(v);
    }

    /// Clears only the FFDN's internal buffers, leaving the CloudSeed
    /// engine untouched.
    pub fn clear_ffdn_buffers(&mut self) {
        self.ffdn.clear();
    }

    pub fn clear_buffers(&mut self) {
        self.channel_l.clear_buffers();
        self.channel_r.clear_buffers();
        self.texture_rack.clear_buffers();
        self.illusion.clear_buffers();
        self.ffdn.clear();

        self.ffdn_pre_delay_l.clear_buffers();
        self.ffdn_pre_delay_r.clear_buffers();
        self.ffdn_hp_l.reset();
        self.ffdn_hp_r.reset();
        self.ffdn_lp_l.reset();
        self.ffdn_lp_r.reset();
        self.ffdn_diff_l.clear_buffers();
        self.ffdn_diff_r.clear_buffers();
        // Force the output-stage smoothed gains to snap on the next
        // block. Preset load is the primary user of this path.
        self.smoothed_width = f64::NAN;
        self.smoothed_dry_gain = f64::NAN;
        self.smoothed_wet_gain = f64::NAN;
    }

    /// Processes one stereo block in place. Input/output are `f32` (the host
    /// buffer type); conversion to `f64` happens at this boundary.
    pub fn process_block_inplace(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len()).min(BUFFER_SIZE);

        match self.engine {
            ReverbEngine::CloudSeed => self.process_cloudseed_inplace(left, right, n),
            ReverbEngine::Ffdn => self.process_ffdn_inplace(left, right, n),
        }
    }

    /// CloudSeed path: the existing 12-line FDN with its full
    /// diffusion front-end, plus the Texture Rack and M/S width.
    fn process_cloudseed_inplace(&mut self, left: &mut [f32], right: &mut [f32], n: usize) {
        // InputMix stereo cross-blend.
        let cm = self.get_scaled_parameter(0) * 0.5;
        let cmi = 1.0 - cm;

        let cmi_v = f64x4::splat(cmi);
        let cm_v = f64x4::splat(cm);

        let mut i = 0;
        while i + 4 <= n {
            let l4 = f64x4::new([
                left[i] as f64,
                left[i + 1] as f64,
                left[i + 2] as f64,
                left[i + 3] as f64,
            ]);
            let r4 = f64x4::new([
                right[i] as f64,
                right[i + 1] as f64,
                right[i + 2] as f64,
                right[i + 3] as f64,
            ]);
            let lo = (l4 * cmi_v + r4 * cm_v).to_array();
            let ro = (r4 * cmi_v + l4 * cm_v).to_array();
            self.left_channel_in[i..i + 4].copy_from_slice(&lo);
            self.right_channel_in[i..i + 4].copy_from_slice(&ro);
            i += 4;
        }
        while i < n {
            let l = left[i] as f64;
            let r = right[i] as f64;
            self.left_channel_in[i] = l * cmi + r * cm;
            self.right_channel_in[i] = r * cmi + l * cm;
            i += 1;
        }

        self.channel_l.process(&self.left_channel_in[..n]);
        self.channel_r.process(&self.right_channel_in[..n]);

        // Texture Rack hook.
        let ol = self.channel_l.get_output();
        let or_ = self.channel_r.get_output();
        self.texture_rack.process(
            &ol[..n],
            &or_[..n],
            &mut self.left_channel_in[..n],
            &mut self.right_channel_in[..n],
        );
        // ILLUSION runs on the wet signal, after the Texture Rack.
        self.illusion.process_inplace(
            &mut self.left_channel_in[..n],
            &mut self.right_channel_in[..n],
        );

        // Per-sample width smoothing.
        let width_target = scaled_parameter_value(41, self.parameters[41]);
        if self.smoothed_width.is_nan() {
            self.smoothed_width = width_target;
        }
        for i in 0..n {
            self.smoothed_width += self.smooth_coeff * (width_target - self.smoothed_width);
            self.smooth_width_scratch[i] = self.smoothed_width;
        }

        Self::apply_output_with_width(
            left,
            right,
            &self.left_channel_in[..n],
            &self.right_channel_in[..n],
            &self.smooth_width_scratch[..n],
            n,
        );
    }

    /// FFDN path: the new compact network, plus the Texture Rack and
    /// M/S width.
    ///
    /// The FFDN takes f64 in/out, so we stage the input into
    /// `ffdn_buf_*`, let the network process it in place, then route
    /// the result through the same Texture + width chain that the
    /// CloudSeed path uses.
    ///
    /// # Zero-allocation guarantee
    ///
    /// Every operation on the audio thread runs on pre-allocated
    /// buffers. The staging, HP/LP, pre-delay, input diffusion, and
    /// network stages all use disjoint field borrows, so the borrow
    /// checker sees that reads of one buffer and writes to another do
    /// not conflict — no scratch copies, no `.clone()`, no `Vec::new`
    /// on this path.
    fn process_ffdn_inplace(&mut self, left: &mut [f32], right: &mut [f32], n: usize) {
        // 1. Stage f32 input into the f64 working buffers.
        //
        //    The InputMix parameter (index 0) is intentionally NOT
        //    applied on the FFDN path: its effect is already covered
        //    by `FfdnCross` inside `FfdnCore`, which offers a
        //    strictly wider range (up to full channel swap) and is
        //    applied at the correct point in the signal chain
        //    (after HP/LP/pre-delay, at the network input). Applying
        //    both would double the mono-summing effect and confuse
        //    the user with two "Blend" controls that do the same
        //    thing.
        //
        //    The `Blend` knob is therefore not shown on the FFDN
        //    INPUT card. The parameter itself is unchanged, and it
        //    still works normally on the CloudSeed page.
        for i in 0..n {
            self.ffdn_buf_l[i] = left[i] as f64;
            self.ffdn_buf_r[i] = right[i] as f64;
        }

        // 2. Input high-pass (in place on the staging buffers).
        if self.ffdn_hp_enabled {
            self.ffdn_hp_l.process_inplace(&mut self.ffdn_buf_l[..n]);
            self.ffdn_hp_r.process_inplace(&mut self.ffdn_buf_r[..n]);
        }

        // 3. Input low-pass (in place on the staging buffers).
        if self.ffdn_lp_enabled {
            self.ffdn_lp_l.process_inplace(&mut self.ffdn_buf_l[..n]);
            self.ffdn_lp_r.process_inplace(&mut self.ffdn_buf_r[..n]);
        }

        // 4. Pre-delay. `ModulatedDelay::process` reads its input
        //    slice and writes to its own internal output buffer; we
        //    then copy the result back into the staging buffers.
        //
        //    The explicit destructuring is important: it tells the
        //    borrow checker that `ffdn_buf_*` and `ffdn_pre_delay_*`
        //    are disjoint fields, so reading one while mutating the
        //    other is allowed. Without the destructuring the compiler
        //    would conservatively reject the read/write pair, which
        //    is why the previous version used `.clone()`.
        //
        //    No allocation here: the copy is a memcpy between two
        //    pre-allocated `Vec<f64>` of the same length.
        {
            let Self {
                ffdn_buf_l,
                ffdn_buf_r,
                ffdn_pre_delay_l,
                ffdn_pre_delay_r,
                ..
            } = self;

            ffdn_pre_delay_l.process(&ffdn_buf_l[..n]);
            ffdn_pre_delay_r.process(&ffdn_buf_r[..n]);

            ffdn_buf_l[..n].copy_from_slice(&ffdn_pre_delay_l.get_output()[..n]);
            ffdn_buf_r[..n].copy_from_slice(&ffdn_pre_delay_r.get_output()[..n]);
        }

        // 5. Input diffusion. Two independent allpass cascades run
        //    in place on the staging buffers. When the amount is
        //    zero, the entire block is skipped and behaviour is
        //    bit-identical to a plain FFDN. No allocation on this
        //    path — the diffusers' buffers were sized in `new()`.
        if self.ffdn_diffusion >= 0.001 {
            self.ffdn_diff_l.process(&self.ffdn_buf_l[..n]);
            {
                let out = self.ffdn_diff_l.get_output();
                self.ffdn_buf_l[..n].copy_from_slice(&out[..n]);
            }
            self.ffdn_diff_r.process(&self.ffdn_buf_r[..n]);
            {
                let out = self.ffdn_diff_r.get_output();
                self.ffdn_buf_r[..n].copy_from_slice(&out[..n]);
            }
        }

        // 6. FFDN network, in place.
        self.ffdn
            .process_inplace(&mut self.ffdn_buf_l[..n], &mut self.ffdn_buf_r[..n]);

        // 6. Wet gain (MainOut) with per-sample smoothing.
        let wet_target = self.get_scaled_parameter(40);
        if self.smoothed_wet_gain.is_nan() {
            self.smoothed_wet_gain = wet_target;
        }
        for i in 0..n {
            self.smoothed_wet_gain +=
                self.smooth_coeff * (wet_target - self.smoothed_wet_gain);
            self.ffdn_buf_l[i] *= self.smoothed_wet_gain;
            self.ffdn_buf_r[i] *= self.smoothed_wet_gain;
        }

        // 7. Texture rack (optional), then M/S width + dry mix.
        //
        //    Both branches destructure `self` into disjoint field
        //    borrows so that the texture rack can read `ffdn_buf_*`
        //    while writing to `left_channel_in` / `right_channel_in`,
        //    all without cloning. The output helper is a **static**
        //    function (not a method) precisely so that the caller's
        //    destructuring stays intact — calling a `&self` method
        //    here would re-borrow the whole struct and defeat the
        //    purpose.
        if self.texture_rack.is_active() {
            let Self {
                texture_rack,
                ffdn_buf_l,
                ffdn_buf_r,
                left_channel_in,
                right_channel_in,
                ..
            } = self;

            texture_rack.process(
                &ffdn_buf_l[..n],
                &ffdn_buf_r[..n],
                &mut left_channel_in[..n],
                &mut right_channel_in[..n],
            );
        } else {
            self.left_channel_in[..n].copy_from_slice(&self.ffdn_buf_l[..n]);
            self.right_channel_in[..n].copy_from_slice(&self.ffdn_buf_r[..n]);
        }

        // ILLUSION runs on the wet signal, after the Texture Rack.
        self.illusion.process_inplace(
            &mut self.left_channel_in[..n],
            &mut self.right_channel_in[..n],
        );

        // Per-sample smoothing of dry_gain and width, then final
        // output write.
        let dry_target = self.get_scaled_parameter(37);
        let width_target = scaled_parameter_value(41, self.parameters[41]);
        if self.smoothed_dry_gain.is_nan() {
            self.smoothed_dry_gain = dry_target;
        }
        if self.smoothed_width.is_nan() {
            self.smoothed_width = width_target;
        }
        for i in 0..n {
            self.smoothed_dry_gain +=
                self.smooth_coeff * (dry_target - self.smoothed_dry_gain);
            self.smoothed_width +=
                self.smooth_coeff * (width_target - self.smoothed_width);
            self.smooth_dry_scratch[i] = self.smoothed_dry_gain;
            self.smooth_width_scratch[i] = self.smoothed_width;
        }

        Self::apply_output_with_dry_and_width(
            left,
            right,
            &self.left_channel_in[..n],
            &self.right_channel_in[..n],
            &self.smooth_dry_scratch[..n],
            &self.smooth_width_scratch[..n],
            n,
        );
    }

    /// Writes the final stereo output by applying M/S width to the
    /// wet channels and mixing in the dry signal (which is still
    /// present in the `left` / `right` f32 buffers) with `dry_gain`.
    ///
    /// This is a **static** function rather than a method because it
    /// is called from `process_ffdn_inplace` while the caller has
    /// destructured `self` into disjoint field borrows. Taking `&self`
    /// would re-borrow the whole struct and silently defeat the
    /// destructuring, forcing a scratch copy.
    ///
    /// The `w` parameter is the stereo width already multiplied by
    /// 0.5, so the per-sample math collapses to a single multiply on
    /// the side component.
    #[inline]
    /// CloudSeed output stage: applies M/S width (smoothed
    /// per-sample via `widths`) and writes the stereo result into
    /// the f32 host buffers.
    ///
    /// The `widths` slice contains the per-sample smoothed width,
    /// already multiplied by 0.5. It must have length >= `n`.
    fn apply_output_with_width(
        left: &mut [f32],
        right: &mut [f32],
        in_l: &[f64],
        in_r: &[f64],
        widths: &[f64],
        n: usize,
    ) {
        let half_v = f64x4::splat(0.5);

        let mut i = 0;
        while i + 4 <= n {
            let l4 = f64x4::new([in_l[i], in_l[i + 1], in_l[i + 2], in_l[i + 3]]);
            let r4 = f64x4::new([in_r[i], in_r[i + 1], in_r[i + 2], in_r[i + 3]]);
            let w4 = f64x4::new([
                widths[i] * 0.5,
                widths[i + 1] * 0.5,
                widths[i + 2] * 0.5,
                widths[i + 3] * 0.5,
            ]);
            let mid = (l4 + r4) * half_v;
            let side = (l4 - r4) * w4;
            let lo = (mid + side).to_array();
            let ro = (mid - side).to_array();
            left[i] = lo[0] as f32;
            left[i + 1] = lo[1] as f32;
            left[i + 2] = lo[2] as f32;
            left[i + 3] = lo[3] as f32;
            right[i] = ro[0] as f32;
            right[i + 1] = ro[1] as f32;
            right[i + 2] = ro[2] as f32;
            right[i + 3] = ro[3] as f32;
            i += 4;
        }
        while i < n {
            let l = in_l[i];
            let r = in_r[i];
            let w = widths[i];
            let mid = (l + r) * 0.5;
            let side = (l - r) * 0.5 * w;
            left[i] = (mid + side) as f32;
            right[i] = (mid - side) as f32;
            i += 1;
        }
    }

    /// FFDN output stage: mixes the dry input (smoothed via
    /// `dry_gains`) with the wet signal, then applies M/S width
    /// (smoothed via `widths`), and writes into the host buffers.
    ///
    /// `dry_gains` and `widths` must have length >= `n`.
    fn apply_output_with_dry_and_width(
        left: &mut [f32],
        right: &mut [f32],
        wet_l: &[f64],
        wet_r: &[f64],
        dry_gains: &[f64],
        widths: &[f64],
        n: usize,
    ) {
        for i in 0..n {
            let l = wet_l[i] + (left[i] as f64) * dry_gains[i];
            let r = wet_r[i] + (right[i] as f64) * dry_gains[i];
            let mid = (l + r) * 0.5;
            let side = (l - r) * 0.5 * widths[i];
            left[i] = (mid + side) as f32;
            right[i] = (mid - side) as f32;
        }
    }

    /// Processes one stereo block with separate input/output slices.
    pub fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) {
        let n = in_l
            .len()
            .min(in_r.len())
            .min(out_l.len())
            .min(out_r.len())
            .min(BUFFER_SIZE);

        let cm = self.get_scaled_parameter(0) * 0.5;
        let cmi = 1.0 - cm;

        let cmi_v = f64x4::splat(cmi);
        let cm_v = f64x4::splat(cm);

        let mut i = 0;
        while i + 4 <= n {
            let l4 = f64x4::new([
                in_l[i] as f64,
                in_l[i + 1] as f64,
                in_l[i + 2] as f64,
                in_l[i + 3] as f64,
            ]);
            let r4 = f64x4::new([
                in_r[i] as f64,
                in_r[i + 1] as f64,
                in_r[i + 2] as f64,
                in_r[i + 3] as f64,
            ]);
            let lo = (l4 * cmi_v + r4 * cm_v).to_array();
            let ro = (r4 * cmi_v + l4 * cm_v).to_array();
            self.left_channel_in[i..i + 4].copy_from_slice(&lo);
            self.right_channel_in[i..i + 4].copy_from_slice(&ro);
            i += 4;
        }
        while i < n {
            let l = in_l[i] as f64;
            let r = in_r[i] as f64;
            self.left_channel_in[i] = l * cmi + r * cm;
            self.right_channel_in[i] = r * cmi + l * cm;
            i += 1;
        }

        self.channel_l.process(&self.left_channel_in[..n]);
        self.channel_r.process(&self.right_channel_in[..n]);

        // Per-sample width smoothing.
        let width_target = scaled_parameter_value(41, self.parameters[41]);
        if self.smoothed_width.is_nan() {
            self.smoothed_width = width_target;
        }
        for i in 0..n {
            self.smoothed_width += self.smooth_coeff * (width_target - self.smoothed_width);
            self.smooth_width_scratch[i] = self.smoothed_width;
        }

        // ---- Texture Rack hook ---------------------------------------
        if self.texture_rack.is_active() {
            let ol = self.channel_l.get_output();
            let or_ = self.channel_r.get_output();

            self.texture_rack.process(
                &ol[..n],
                &or_[..n],
                &mut self.left_channel_in[..n],
                &mut self.right_channel_in[..n],
            );

            for i in 0..n {
                let l = self.left_channel_in[i];
                let r = self.right_channel_in[i];
                let w = self.smooth_width_scratch[i];
                let mid = (l + r) * 0.5;
                let side = (l - r) * 0.5 * w;
                out_l[i] = (mid + side) as f32;
                out_r[i] = (mid - side) as f32;
            }
        } else {
            let ol = self.channel_l.get_output();
            let or_ = self.channel_r.get_output();
            for i in 0..n {
                let l = ol[i];
                let r = or_[i];
                let w = self.smooth_width_scratch[i];
                let mid = (l + r) * 0.5;
                let side = (l - r) * 0.5 * w;
                out_l[i] = (mid + side) as f32;
                out_r[i] = (mid - side) as f32;
            }
        }
    }

    /// Mono fallback: feeds the same signal to both reverb channels
    /// and sums the outputs. (Matches `ReverbController::ProcessMono`
    /// in the C++ original.)
    ///
    /// # Engine dispatch
    ///
    /// The plugin's `AUDIO_IO_LAYOUTS` declares stereo-only, so this
    /// path is only reached when a host hands us a single-channel
    /// buffer for some reason. It must nonetheless respect the
    /// current engine: the CloudSeed path is a single-channel-aware
    /// existing implementation, while the FFDN is inherently stereo,
    /// so we duplicate the mono signal into scratch stereo buffers,
    /// run the normal FFDN stereo path, and average the result.
    ///
    /// The scratch buffers are pre-allocated; `std::mem::take` swaps
    /// in an empty `Vec` (no allocation — `Vec::new` doesn't allocate)
    /// so we can pass `&mut [f32]` to `process_ffdn_inplace`, which
    /// itself needs `&mut self`. The buffers are put back at the end,
    /// with their capacity intact, so this does not allocate either.
    pub fn process_mono_inplace(&mut self, mono: &mut [f32]) {
        let n = mono.len().min(BUFFER_SIZE);

        match self.engine {
            ReverbEngine::CloudSeed => {
                for i in 0..n {
                    let m = mono[i] as f64;
                    self.left_channel_in[i] = m;
                    self.right_channel_in[i] = m;
                }

                self.channel_l.process(&self.left_channel_in[..n]);
                self.channel_r.process(&self.right_channel_in[..n]);

                let ol = self.channel_l.get_output();
                let or_ = self.channel_r.get_output();

                for i in 0..n {
                    mono[i] = ((ol[i] + or_[i]) * 0.5) as f32;
                }
            }
            ReverbEngine::Ffdn => {
                // Take the scratch buffers out of `self` so they can
                // be borrowed as `&mut [f32]` while `self` is also
                // borrowed mutably by `process_ffdn_inplace`.
                let mut buf_l = std::mem::take(&mut self.mono_stereo_l);
                let mut buf_r = std::mem::take(&mut self.mono_stereo_r);

                buf_l[..n].copy_from_slice(&mono[..n]);
                buf_r[..n].copy_from_slice(&mono[..n]);

                self.process_ffdn_inplace(&mut buf_l, &mut buf_r, n);

                // Sum the stereo output back to mono. The factor of
                // 0.5 matches the CloudSeed branch and keeps the
                // perceived level identical between engines.
                for i in 0..n {
                    mono[i] = (buf_l[i] + buf_r[i]) * 0.5;
                }

                self.mono_stereo_l = buf_l;
                self.mono_stereo_r = buf_r;
            }
        }
    }
    // ============================================================
    // ILLUSION — pitched reverb sequencer
    // ============================================================

    // ============================================================
    // ILLUSION — granular cloud with pitched overtones
    // ============================================================

    pub fn set_illusion_spark(&mut self, v: f64) {
        self.illusion.set_spark(v);
    }
    pub fn set_illusion_still(&mut self, v: f64) {
        self.illusion.set_still(v);
    }
    /// Sets AURA in semitones (-24..=+24). Values below |0.5| enable
    /// the internal deterministic sequence (unison mode).
    pub fn set_illusion_aura(&mut self, semitones: f64) {
        self.illusion.set_aura_semitones(semitones);
    }
    pub fn set_illusion_veil(&mut self, v: f64) {
        self.illusion.set_veil(v);
    }
    pub fn set_illusion_wide(&mut self, v: f64) {
        self.illusion.set_wide(v);
    }
    pub fn set_illusion_tint(&mut self, v: f64) {
        self.illusion.set_tint(v);
    }
    pub fn set_illusion_rate(&mut self, v: f64) {
        self.illusion.set_rate_norm(v);
    }
    pub fn set_illusion_grain(&mut self, v: f64) {
        self.illusion.set_grain_norm(v);
    }
}
