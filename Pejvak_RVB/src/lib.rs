//! # PEJVAK RVB – CloudReverb Rust Port (GPL-3.0)
//!
//! Faithful Rust port of Valdemar Erlingsson's CloudReverb (legacy-v1).
//! The DSP is a direct translation of `audio_engine/*.h`. The plugin
//! wrapper mirrors the C++ `PluginProcessor`'s 49-parameter model,
//! including the DiffusionFeedback 0.92 DSP clamp and the tail-aware
//! Smart Idle logic.
//!
//! On top of the 49 CloudSeed parameters, this wrapper exposes the
//! PEJVAK Texture Rack (six post-reverb layers) and the Velvet-Noise
//! diffusion stage. All extension parameters default to 0 / neutral,
//! so a freshly loaded instance is bit-identical to plain CloudReverb.
//!
//! ## Audio-thread guarantees
//!
//! * Zero heap allocation inside `process()`. All buffers are sized in
//!   `activate()` and reused in place.
//! * Every parameter is cached. Only parameters whose value actually
//!   changed are forwarded to the DSP core, so the expensive
//!   recomputation inside `ReverbChannel` (SHA-256 seed series,
//!   delay-line layouts, filter updates) runs only on user input.
//! * The entire DSP chain is wrapped in `catch_unwind`, so a panic in
//!   any layer is contained and cannot crash the host.

#![allow(non_snake_case)]

use fdn_reverb_core::cloudseed::{
    scaled_parameter_value, Parameter, ReverbController, ReverbEngine, TextureParamId,
    DEFAULT_PARAMS, PARAM_COUNT,
};
use nice_plug::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

#[cfg(feature = "gui-egui")]
pub mod editor;

#[cfg(feature = "gui-egui")]
pub mod presets;

#[cfg(feature = "gui-egui")]
mod manual;

// ============================================================================
// Constants
// ============================================================================

/// Threshold below which a signal is considered "silent" for the
/// purposes of the Smart Idle logic. When both the input and the
/// output peaks stay below this level for `IDLE_HOLD_S` seconds,
/// the plugin enters sleep mode and skips DSP entirely.
///
/// -100 dBFS is well below the noise floor of any real converter
/// and far below the lowest level a user can perceive in a mix. It
/// is chosen so that even a very quiet reverb tail has finished
/// decaying before sleep kicks in.
const SLEEP_LEVEL: f32 = 1.0e-5;

/// Threshold above which a sleeping plugin wakes up.
///
/// Deliberately 10× `SLEEP_LEVEL`, creating a hysteresis band that
/// prevents the plugin from flapping between awake and asleep on
/// borderline signals. A single threshold would cause a continuous
/// on-off oscillation on any signal whose peak hovers around the
/// value, which is both wasteful (CPU) and potentially audible
/// (intermittent gaps in the output).
const WAKE_LEVEL: f32 = 1.0e-4;

/// How long both input and output must stay below `SLEEP_LEVEL`
/// before the plugin goes to sleep, in seconds.
const IDLE_HOLD_S: f32 = 0.5;

/// C++ `PluginProcessor.cpp` clamps DiffusionFeedback's DSP value to 0.92.
const DIFFUSION_FEEDBACK_CAP: f64 = 0.92;

/// Total number of Texture-Rack + Velvet-Noise parameters. Kept as a
/// named constant so the cache-array type stays readable; its value
/// comes from the enum itself so the two cannot drift apart silently.
const TEX_COUNT: usize = TextureParamId::COUNT;

/// Compile-time sanity check: the plugin cache array is sized for
/// exactly 32 texture parameters. If a new variant is added to
/// `TextureParamId`, this assertion fails the build and forces a
/// deliberate review of `TEX_COUNT` and the push order in
/// `process_inner()`.
const _: () = assert!(
    TextureParamId::COUNT == 32,
    "TextureParamId::COUNT changed — review TEX_COUNT and push_tex ordering",
);

/// Default texture-chain order, packed one byte per slot:
/// `[0, 1, 2, 3, 4, 5]` = Shimmer → Mist → Drift → Crystal → Scatter → Bloom.
const DEFAULT_TEXTURE_ORDER_PACKED: u64 = 0x0000_0504_0302_0100;

// ============================================================================
// File-based user settings (default editor size)
// ============================================================================
//
// We persist the user's preferred editor size to a small text file in
// the OS-specific per-user config directory. This is intentionally
// separate from the plugin's `#[persist]` state, because the plugin
// state lives inside the host's project / preset data and is lost
// when the user removes and re-adds the plugin instance. A file on
// disk survives plugin removal and is available to any future
// instance of the plugin on the same machine.

/// Path to the user settings file, creating the containing directory
/// if needed. Returns `None` if the OS doesn't expose a per-user
/// config directory or if directory creation fails.
fn settings_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var("APPDATA").ok()
    } else {
        std::env::var("HOME").ok()
    };
    let mut dir = PathBuf::from(base?);
    dir.push("PEJVAK RVB");
    std::fs::create_dir_all(&dir).ok()?;
    dir.push("settings.txt");
    Some(dir)
}

/// Reads `default_width` and `default_height` from the settings file.
/// Returns `(0, 0)` if the file doesn't exist or is malformed, which
/// the caller interprets as "use the factory default size".
fn load_default_size() -> (u32, u32) {
    let path = match settings_path() {
        Some(p) => p,
        None => return (0, 0),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return (0, 0),
    };
    let mut w = 0u32;
    let mut h = 0u32;
    for line in content.lines() {
        if let Some(v) = line.strip_prefix("default_width=") {
            w = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("default_height=") {
            h = v.trim().parse().unwrap_or(0);
        }
    }
    (w, h)
}

/// Writes `default_width` and `default_height` to the settings file.
/// Silently ignores I/O errors — a failed write is not fatal, the
/// user simply won't get the new default on the next plugin launch.
pub fn save_default_size(w: u32, h: u32) {
    if let Some(path) = settings_path() {
        let content = format!("default_width={}\ndefault_height={}\n", w, h);
        let _ = std::fs::write(&path, content);
    }
}

// ============================================================================
// Smart Idle (matches C++ `AudioPluginAudioProcessor::processBlock`)
// ============================================================================

/// Two-level silence detector that lets the plugin skip DSP work
/// during long stretches of silence.
///
/// State transitions:
///
/// ```text
///                          in_peak ≥ WAKE_LEVEL
///                    ┌─────────────────────────────────┐
///                    ▼                                 │
///                  AWAKE                             ASLEEP
///                    │                                 ▲
///                    └── (in_peak < SLEEP_LEVEL ───────┘
///                        and out_peak < SLEEP_LEVEL
///                        for IDLE_HOLD_S seconds)
/// ```
///
/// The two-threshold design (`WAKE_LEVEL` > `SLEEP_LEVEL`) is
/// essential: with a single threshold, any signal whose peak hovers
/// around that value would cause the plugin to oscillate between
/// the two states every block, which is both wasteful (CPU) and
/// potentially audible (intermittent gaps in the output).
struct TailIdle {
    /// True while the plugin is in sleep mode.
    sleeping: bool,
    /// Accumulated count of consecutive silent samples (not blocks).
    silent_samples: usize,
    /// Set to `true` for exactly one block after a sleep → awake
    /// transition. The caller uses this to invalidate its parameter
    /// caches exactly once per wake-up. Cleared at the start of every
    /// `wake_check` call, and only set back to `true` if we actually
    /// wake up this block.
    did_wake: bool,
}

impl TailIdle {
    fn new() -> Self {
        Self {
            sleeping: false,
            silent_samples: 0,
            did_wake: false,
        }
    }

    /// Returns true if the plugin should process audio this block.
    ///
    /// Sets `did_wake()` to `true` for the one block in which the
    /// plugin transitions from ASLEEP to AWAKE.
    #[inline]
    fn wake_check(&mut self, in_peak: f32) -> bool {
        // Reset the one-shot wake flag at the top of every call. It
        // is set back to `true` only if we actually wake up this
        // block.
        self.did_wake = false;

        if self.sleeping {
            if in_peak >= WAKE_LEVEL {
                self.sleeping = false;
                self.silent_samples = 0;
                self.did_wake = true;
                return true;
            }
            return false;
        }
        true
    }

    /// True for exactly one block after a sleep → awake transition.
    ///
    /// The caller (`process_inner`) uses this to invalidate its
    /// parameter caches so that every parameter is re-pushed to the
    /// DSP engine on the first block after waking up. Without this,
    /// a parameter change that the host makes while the plugin is
    /// asleep would be silently swallowed by the cache: the audio
    /// thread never saw it, so `last_pushed` still holds the old
    /// value and the next block skips the push.
    #[inline]
    fn did_wake(&self) -> bool {
        self.did_wake
    }

    /// Accumulates silent samples and returns `true` the first time
    /// the hold threshold is reached (i.e. the plugin should go to
    /// sleep).
    ///
    /// Returns `false` on every subsequent silent block while
    /// sleeping, so the caller only runs its sleep-time cleanup
    /// exactly once per transition, not on every silent block.
    #[inline]
    fn sleep_check(
        &mut self,
        in_peak: f32,
        out_peak: f32,
        samples: usize,
        hold_samples: usize,
    ) -> bool {
        if self.sleeping {
            return false;
        }

        if in_peak < SLEEP_LEVEL && out_peak < SLEEP_LEVEL {
            self.silent_samples = self.silent_samples.saturating_add(samples);
            if self.silent_samples >= hold_samples {
                self.sleeping = true;
                self.silent_samples = 0;
                return true;
            }
        } else {
            self.silent_samples = 0;
        }
        false
    }
}

// ============================================================================
// Parameter helpers
// ============================================================================

#[inline]
fn bool_to_f64(b: bool) -> f64 {
    if b {
        1.0
    } else {
        0.0
    }
}

/// Creates a 0..1 normalised `FloatParam` for the given CloudSeed
/// parameter index, wired to the physical-value display and parse
/// functions.
///
/// Parameters whose physical value is inherently discrete (tap counts,
/// line counts, diffusion-stage counts) get an explicit
/// `with_step_size` so that:
///
///   * the host's automation lanes snap to the correct integer
///     positions, instead of moving smoothly through values that
///     truncate to the same integer,
///   * the host's "next value" / "previous value" navigation for
///     automation points works sensibly,
///   * the DAW's generic slider (if it shows one) has the right
///     number of stops.
fn p_norm(index: usize) -> FloatParam {
    let default = DEFAULT_PARAMS[index] as f32;
    let step_size: Option<f32> = match index {
        5 => Some(1.0 / 49.0),
        10 => Some(1.0 / 7.0),
        14 => Some(1.0 / 11.0),
        18 => Some(1.0 / 7.0),
        _ => None,
    };

    let display_name: &str = match index {
        40 => "Late", // CloudSeed "MainOut" is labelled "Late" in the UI.
        _ => Parameter::from_index(index).unwrap().name(),
    };

    let param = FloatParam::new(
        display_name,
        default,
        FloatRange::Linear { min: 0.0, max: 1.0 },
    )
    .with_value_to_string(Arc::new(move |v| display_parameter(index, v)))
    .with_string_to_value(Arc::new(move |s| parse_parameter(index, s)));

    match step_size {
        Some(s) => param.with_step_size(s),
        None => param,
    }
}

/// Formats a normalised parameter value as its physical-unit string
/// (e.g. "250 ms", "-12.3 dB", "post").
fn display_parameter(index: usize, normalized: f32) -> String {
    use Parameter::*;
    let p = match Parameter::from_index(index) {
        Some(p) => p,
        None => return format!("{:.2}", normalized),
    };
    let v = scaled_parameter_value(index, normalized as f64);
    match p {
        InputMix => format!("{:.2}", v),
        PreDelay => format!("{:.0} ms", v),
        HighPass
        | LowPass
        | PostLowShelfFrequency
        | PostHighShelfFrequency
        | PostCutoffFrequency => format!("{:.0} Hz", v),
        TapCount => format!("{:.0} taps", v),
        TapLength => format!("{:.0} ms", v),
        TapGain => format!("{:.1} dB", 20.0 * v.max(1e-9).log10()),
        TapDecay => format!("{:.2}", v),
        DiffusionStages | LateDiffusionStages => format!("{:.0} stages", v),
        DiffusionDelay | LateDiffusionDelay => format!("{:.0} ms", v),
        DiffusionFeedback | LateDiffusionFeedback => format!("{:.2}", v),
        LineCount => format!("{:.0} lines", v),
        LineDelay => format!("{:.0} ms", v),
        LineDecay => {
            if v < 1.0 {
                format!("{:.0} ms", v * 1000.0)
            } else {
                format!("{:.2} s", v)
            }
        }
        PostLowShelfGain | PostHighShelfGain => format!("{:.1} dB", 20.0 * v.max(1e-9).log10()),
        EarlyDiffusionModAmount | LineModAmount | LateDiffusionModAmount => {
            format!("{:.2} ms", v)
        }
        EarlyDiffusionModRate | LineModRate | LateDiffusionModRate => format!("{:.2} Hz", v),
        TapSeed | DiffusionSeed | DelaySeed | PostDiffusionSeed => format!("{:.0}", v),
        CrossSeed => format!("{:.2}", v),
        DryOut | PredelayOut | EarlyOut | MainOut => {
            let db = 20.0 * v.max(1e-9).log10();
            if db <= -59.5 {
                "-inf".to_string()
            } else {
                format!("{:.1} dB", db)
            }
        }
        WidthOut => format!("{:.0} %", v * 100.0),
        LateStageTap => {
            if normalized < 0.5 {
                "pre".to_string()
            } else {
                "post".to_string()
            }
        }
        HiPassEnabled | LowPassEnabled | LowShelfEnabled | HighShelfEnabled | CutoffEnabled
        | Interpolation => {
            if normalized < 0.5 {
                "off".to_string()
            } else {
                "on".to_string()
            }
        }
        TapEnabled | DiffusionEnabled | LateDelayEnabled | LateDiffusionEnabled => {
            if normalized < 0.5 {
                "off".to_string()
            } else {
                "on".to_string()
            }
        }
    }
}

/// Parses a physical-unit string back into a normalised parameter
/// value by bisecting the forward `scaled_parameter_value` map.
fn parse_parameter(index: usize, s: &str) -> Option<f32> {
    let t = s.trim().to_lowercase();
    if t.starts_with("-inf") {
        return Some(0.0);
    }
    if t == "on" {
        return Some(1.0);
    }
    if t == "off" {
        return Some(0.0);
    }
    if t == "pre" {
        return Some(0.0);
    }
    if t == "post" {
        return Some(1.0);
    }
    let cleaned: String = t
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
        .collect();
    let val: f64 = cleaned.parse().ok()?;

    let mut lo = 0.0f64;
    let mut hi = 1.0f64;
    let target: f64 = if t.contains("db") {
        10f64.powf(val / 20.0)
    } else if t.contains('%') {
        val / 100.0
    } else {
        val
    };
    let increasing = scaled_parameter_value(index, 1.0) >= scaled_parameter_value(index, 0.0);
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        let v = scaled_parameter_value(index, mid);
        if (v < target) == increasing {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some((0.5 * (lo + hi)) as f32)
}

/// Push one Texture-Rack parameter into the DSP core **only if its
/// value differs from the cached value**. Uses the typed enum so the
/// controller can dispatch with a jump table instead of a string match.
#[inline]
fn push_tex(
    ctrl: &mut ReverbController,
    cache: &mut [f64; TEX_COUNT],
    id: TextureParamId,
    value: f64,
) {
    let idx = id.index();
    if value != cache[idx] {
        ctrl.set_texture_parameter(id, value);
        cache[idx] = value;
    }
}

// ============================================================================
// Parameter groups
// ============================================================================

/// Input-stage parameters.
#[derive(Params)]
pub struct InputParams {
    #[id = "InputMix"]
    pub mix: FloatParam,
    #[id = "PreDelay"]
    pub predelay: FloatParam,
    #[id = "HighPass"]
    pub high_pass: FloatParam,
    #[id = "LowPass"]
    pub low_pass: FloatParam,
}

impl Default for InputParams {
    fn default() -> Self {
        use Parameter::*;
        Self {
            mix: p_norm(InputMix as usize),
            predelay: p_norm(PreDelay as usize),
            high_pass: p_norm(HighPass as usize),
            low_pass: p_norm(LowPass as usize),
        }
    }
}

/// Early-stage (multitap + diffusion) parameters.
#[derive(Params)]
pub struct EarlyParams {
    #[id = "TapEnabled"]
    pub tap_enabled: BoolParam,
    #[id = "TapCount"]
    pub tap_count: FloatParam,
    #[id = "TapLength"]
    pub tap_length: FloatParam,
    #[id = "TapGain"]
    pub tap_gain: FloatParam,
    #[id = "TapDecay"]
    pub tap_decay: FloatParam,
    #[id = "DiffusionEnabled"]
    pub diffusion_enabled: BoolParam,
    #[id = "DiffusionStages"]
    pub diffusion_stages: FloatParam,
    #[id = "DiffusionDelay"]
    pub diffusion_delay: FloatParam,
    #[id = "DiffusionFeedback"]
    pub diffusion_feedback: FloatParam,
}

impl Default for EarlyParams {
    fn default() -> Self {
        use Parameter::*;
        Self {
            tap_enabled: BoolParam::new("TapEnabled", DEFAULT_PARAMS[TapEnabled as usize] >= 0.5),
            tap_count: p_norm(TapCount as usize),
            tap_length: p_norm(TapLength as usize),
            tap_gain: p_norm(TapGain as usize),
            tap_decay: p_norm(TapDecay as usize),
            diffusion_enabled: BoolParam::new(
                "DiffusionEnabled",
                DEFAULT_PARAMS[DiffusionEnabled as usize] >= 0.5,
            ),
            diffusion_stages: p_norm(DiffusionStages as usize),
            diffusion_delay: p_norm(DiffusionDelay as usize),
            diffusion_feedback: p_norm(DiffusionFeedback as usize),
        }
    }
}

/// Late-stage (FDN delay lines + late diffusion) parameters.
#[derive(Params)]
pub struct LateParams {
    #[id = "LateDelayEnabled"]
    pub late_delay_enabled: BoolParam,
    #[id = "LineCount"]
    pub line_count: FloatParam,
    #[id = "LineDelay"]
    pub line_delay: FloatParam,
    #[id = "LineDecay"]
    pub line_decay: FloatParam,
    #[id = "LateDiffusionEnabled"]
    pub late_diffusion_enabled: BoolParam,
    #[id = "LateDiffusionStages"]
    pub late_diffusion_stages: FloatParam,
    #[id = "LateDiffusionDelay"]
    pub late_diffusion_delay: FloatParam,
    #[id = "LateDiffusionFeedback"]
    pub late_diffusion_feedback: FloatParam,
}

impl Default for LateParams {
    fn default() -> Self {
        use Parameter::*;
        Self {
            late_delay_enabled: BoolParam::new(
                "LateDelayEnabled",
                DEFAULT_PARAMS[LateDelayEnabled as usize] >= 0.5,
            ),
            line_count: p_norm(LineCount as usize),
            line_delay: p_norm(LineDelay as usize),
            line_decay: p_norm(LineDecay as usize),
            late_diffusion_enabled: BoolParam::new(
                "LateDiffusionEnabled",
                DEFAULT_PARAMS[LateDiffusionEnabled as usize] >= 0.5,
            ),
            late_diffusion_stages: p_norm(LateDiffusionStages as usize),
            late_diffusion_delay: p_norm(LateDiffusionDelay as usize),
            late_diffusion_feedback: p_norm(LateDiffusionFeedback as usize),
        }
    }
}

/// EQ parameters.
#[derive(Params)]
pub struct EqParams {
    /// Master enable. When off, the three per-stage enables
    /// (`LowShelfEnabled`, `HighShelfEnabled`, `CutoffEnabled`,
    /// parameters 44–46) are forced to `false` for the DSP engine,
    /// regardless of their individual stored values. The stored
    /// values are preserved in the parameters, so turning the
    /// master switch back on restores the user's per-stage
    /// selection.
    #[id = "EqEnabled"]
    pub enabled: BoolParam,

    #[id = "PostLowShelfGain"]
    pub low_shelf_gain: FloatParam,
    #[id = "PostLowShelfFrequency"]
    pub low_shelf_frequency: FloatParam,
    #[id = "PostHighShelfGain"]
    pub high_shelf_gain: FloatParam,
    #[id = "PostHighShelfFrequency"]
    pub high_shelf_frequency: FloatParam,
    #[id = "PostCutoffFrequency"]
    pub cutoff_frequency: FloatParam,
}

impl Default for EqParams {
    fn default() -> Self {
        use Parameter::*;
        Self {
            enabled: BoolParam::new("EqEnabled", true),
            low_shelf_gain: p_norm(PostLowShelfGain as usize),
            low_shelf_frequency: p_norm(PostLowShelfFrequency as usize),
            high_shelf_gain: p_norm(PostHighShelfGain as usize),
            high_shelf_frequency: p_norm(PostHighShelfFrequency as usize),
            cutoff_frequency: p_norm(PostCutoffFrequency as usize),
        }
    }
}

/// Modulation parameters.
#[derive(Params)]
pub struct ModulationParams {
    #[id = "EarlyDiffusionModAmount"]
    pub early_diffusion_mod_amount: FloatParam,
    #[id = "EarlyDiffusionModRate"]
    pub early_diffusion_mod_rate: FloatParam,
    #[id = "LineModAmount"]
    pub line_mod_amount: FloatParam,
    #[id = "LineModRate"]
    pub line_mod_rate: FloatParam,
    #[id = "LateDiffusionModAmount"]
    pub late_diffusion_mod_amount: FloatParam,
    #[id = "LateDiffusionModRate"]
    pub late_diffusion_mod_rate: FloatParam,
}

impl Default for ModulationParams {
    fn default() -> Self {
        use Parameter::*;
        Self {
            early_diffusion_mod_amount: p_norm(EarlyDiffusionModAmount as usize),
            early_diffusion_mod_rate: p_norm(EarlyDiffusionModRate as usize),
            line_mod_amount: p_norm(LineModAmount as usize),
            line_mod_rate: p_norm(LineModRate as usize),
            late_diffusion_mod_amount: p_norm(LateDiffusionModAmount as usize),
            late_diffusion_mod_rate: p_norm(LateDiffusionModRate as usize),
        }
    }
}

/// Seed parameters.
#[derive(Params)]
pub struct SeedParams {
    #[id = "TapSeed"]
    pub tap_seed: FloatParam,
    #[id = "DiffusionSeed"]
    pub diffusion_seed: FloatParam,
    #[id = "DelaySeed"]
    pub delay_seed: FloatParam,
    #[id = "PostDiffusionSeed"]
    pub post_diffusion_seed: FloatParam,
    #[id = "CrossSeed"]
    pub cross_seed: FloatParam,
}

impl Default for SeedParams {
    fn default() -> Self {
        use Parameter::*;
        Self {
            tap_seed: p_norm(TapSeed as usize),
            diffusion_seed: p_norm(DiffusionSeed as usize),
            delay_seed: p_norm(DelaySeed as usize),
            post_diffusion_seed: p_norm(PostDiffusionSeed as usize),
            cross_seed: p_norm(CrossSeed as usize),
        }
    }
}

/// Output mix parameters.
#[derive(Params)]
pub struct OutputParams {
    #[id = "DryOut"]
    pub dry_out: FloatParam,
    #[id = "PredelayOut"]
    pub predelay_out: FloatParam,
    #[id = "EarlyOut"]
    pub early_out: FloatParam,
    #[id = "MainOut"]
    pub main_out: FloatParam,
    #[id = "Width"]
    pub width: FloatParam,
}

impl Default for OutputParams {
    fn default() -> Self {
        use Parameter::*;
        Self {
            dry_out: p_norm(DryOut as usize),
            predelay_out: p_norm(PredelayOut as usize),
            early_out: p_norm(EarlyOut as usize),
            main_out: p_norm(MainOut as usize),
            width: p_norm(WidthOut as usize),
        }
    }
}

/// Boolean switch parameters.
#[derive(Params)]
pub struct SwitchParams {
    #[id = "HiPassEnabled"]
    pub hi_pass_enabled: BoolParam,
    #[id = "LowPassEnabled"]
    pub low_pass_enabled: BoolParam,
    #[id = "LowShelfEnabled"]
    pub low_shelf_enabled: BoolParam,
    #[id = "HighShelfEnabled"]
    pub high_shelf_enabled: BoolParam,
    #[id = "CutoffEnabled"]
    pub cutoff_enabled: BoolParam,
    #[id = "LateStageTap"]
    pub late_stage_tap: BoolParam,
    #[id = "Interpolation"]
    pub interpolation: BoolParam,
}

impl Default for SwitchParams {
    fn default() -> Self {
        use Parameter::*;
        Self {
            hi_pass_enabled: BoolParam::new(
                "HiPassEnabled",
                DEFAULT_PARAMS[HiPassEnabled as usize] >= 0.5,
            ),
            low_pass_enabled: BoolParam::new(
                "LowPassEnabled",
                DEFAULT_PARAMS[LowPassEnabled as usize] >= 0.5,
            ),
            low_shelf_enabled: BoolParam::new(
                "LowShelfEnabled",
                DEFAULT_PARAMS[LowShelfEnabled as usize] >= 0.5,
            ),
            high_shelf_enabled: BoolParam::new(
                "HighShelfEnabled",
                DEFAULT_PARAMS[HighShelfEnabled as usize] >= 0.5,
            ),
            cutoff_enabled: BoolParam::new(
                "CutoffEnabled",
                DEFAULT_PARAMS[CutoffEnabled as usize] >= 0.5,
            ),
            // `LateStageTap` is stored as a bool internally but its
            // semantics are pre/post, not off/on. Give the host a
            // custom display string so automation lanes and the
            // generic parameter list read correctly. The underlying
            // `bool` value is unchanged, so automation curves and
            // saved state from previous versions still work.
            late_stage_tap: BoolParam::new(
                "LateStageTap",
                DEFAULT_PARAMS[LateStageTap as usize] >= 0.5,
            )
            .with_value_to_string(Arc::new(|v| {
                if v {
                    "post".to_string()
                } else {
                    "pre".to_string()
                }
            })),
            interpolation: BoolParam::new(
                "Interpolation",
                DEFAULT_PARAMS[Interpolation as usize] >= 0.5,
            ),
        }
    }
}

/// Shimmer texture layer parameters.
#[derive(Params)]
pub struct ShimmerParams {
    #[id = "ShimmerMix"]
    pub mix: FloatParam,
    #[id = "ShimmerPitch"]
    pub pitch: FloatParam,
    #[id = "ShimmerFeedback"]
    pub feedback: FloatParam,
    #[id = "ShimmerLowCut"]
    pub low_cut: FloatParam,
    #[id = "ShimmerHighCut"]
    pub high_cut: FloatParam,
    #[id = "ShimmerEnabled"]
    pub enabled: BoolParam,
}

impl Default for ShimmerParams {
    fn default() -> Self {
        Self {
            mix: FloatParam::new("ShimmerMix", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            pitch: FloatParam::new(
                "ShimmerPitch",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                let semis = ((v - 0.5) * 24.0).round() as i32;
                if semis >= 0 {
                    format!("+{} st", semis)
                } else {
                    format!("{} st", semis)
                }
            })),
            feedback: FloatParam::new(
                "ShimmerFeedback",
                0.6,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            low_cut: FloatParam::new(
                "ShimmerLowCut",
                0.2,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} Hz", 20.0 + v * 1980.0))),
            high_cut: FloatParam::new(
                "ShimmerHighCut",
                0.6,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} Hz", 200.0 + v * 15000.0))),
            enabled: BoolParam::new("ShimmerEnabled", false),
        }
    }
}

/// Mist texture layer parameters.
#[derive(Params)]
pub struct MistParams {
    #[id = "MistMix"]
    pub mix: FloatParam,
    #[id = "MistHP"]
    pub hp: FloatParam,
    #[id = "MistFeedback"]
    pub feedback: FloatParam,
    #[id = "MistDelay"]
    pub delay: FloatParam,
    #[id = "MistRate"]
    pub rate: FloatParam,
    #[id = "MistEnabled"]
    pub enabled: BoolParam,
}

impl Default for MistParams {
    fn default() -> Self {
        Self {
            mix: FloatParam::new("MistMix", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            hp: FloatParam::new("MistHP", 0.1, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} Hz", 100.0 + v * 2900.0))),
            feedback: FloatParam::new(
                "MistFeedback",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            delay: FloatParam::new("MistDelay", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} ms", 8.0 + v * 32.0))),
            rate: FloatParam::new("MistRate", 0.25, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| {
                    let hz = 0.05 + v * 24.95;
                    if hz < 10.0 {
                        format!("{:.2} Hz", hz)
                    } else {
                        format!("{:.0} Hz", hz)
                    }
                })),
            enabled: BoolParam::new("MistEnabled", false),
        }
    }
}

/// Drift texture layer parameters.
#[derive(Params)]
pub struct DriftParams {
    #[id = "DriftMix"]
    pub mix: FloatParam,
    #[id = "DriftFeedback"]
    pub feedback: FloatParam,
    #[id = "DriftDelay"]
    pub delay: FloatParam,
    #[id = "DriftRate"]
    pub rate: FloatParam,
    #[id = "DriftDepth"]
    pub depth: FloatParam,
    #[id = "DriftEnabled"]
    pub enabled: BoolParam,
}

impl Default for DriftParams {
    fn default() -> Self {
        Self {
            mix: FloatParam::new("DriftMix", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            feedback: FloatParam::new(
                "DriftFeedback",
                0.3,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            delay: FloatParam::new("DriftDelay", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.1} ms", 5.0 + v * 20.0))),
            rate: FloatParam::new("DriftRate", 0.3, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.2} Hz", 0.02 + v * 0.50))),
            depth: FloatParam::new("DriftDepth", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.2} ms", v * 0.4))),
            enabled: BoolParam::new("DriftEnabled", false),
        }
    }
}

/// Crystal texture layer parameters.
#[derive(Params)]
pub struct CrystalParams {
    #[id = "CrystalMix"]
    pub mix: FloatParam,
    #[id = "CrystalSize"]
    pub size: FloatParam,
    #[id = "CrystalDensity"]
    pub density: FloatParam,
    #[id = "CrystalFeedback"]
    pub feedback: FloatParam,
    #[id = "CrystalEnabled"]
    pub enabled: BoolParam,
}

impl Default for CrystalParams {
    fn default() -> Self {
        Self {
            mix: FloatParam::new("CrystalMix", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            size: FloatParam::new(
                "CrystalSize",
                0.4,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} ms", 15.0 + v * 135.0))),
            density: FloatParam::new(
                "CrystalDensity",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} ms", 20.0 + (1.0 - v) * 230.0))),
            feedback: FloatParam::new(
                "CrystalFeedback",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            enabled: BoolParam::new("CrystalEnabled", false),
        }
    }
}

/// Scatter texture layer parameters.
#[derive(Params)]
pub struct ScatterParams {
    #[id = "ScatterMix"]
    pub mix: FloatParam,
    #[id = "ScatterSpread"]
    pub spread: FloatParam,
    #[id = "ScatterDensity"]
    pub density: FloatParam,
    #[id = "ScatterSpeed"]
    pub speed: FloatParam,
    #[id = "ScatterFeedback"]
    pub feedback: FloatParam,
    #[id = "ScatterEnabled"]
    pub enabled: BoolParam,
}

impl Default for ScatterParams {
    fn default() -> Self {
        Self {
            mix: FloatParam::new("ScatterMix", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            spread: FloatParam::new(
                "ScatterSpread",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} ms", 30.0 + v * 370.0))),
            density: FloatParam::new(
                "ScatterDensity",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                let interval_ms = 15.0 + (1.0 - v) * (400.0 - 15.0);
                let grains_per_sec = 1000.0 / interval_ms;
                if grains_per_sec < 10.0 {
                    format!("{:.1} grains/s", grains_per_sec)
                } else {
                    format!("{:.0} grains/s", grains_per_sec)
                }
            })),
            speed: FloatParam::new(
                "ScatterSpeed",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} ms", 80.0 + (1.0 - v) * 1420.0))),
            feedback: FloatParam::new(
                "ScatterFeedback",
                0.3,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            enabled: BoolParam::new("ScatterEnabled", false),
        }
    }
}

/// Bloom texture layer parameters.
#[derive(Params)]
pub struct BloomParams {
    #[id = "BloomMix"]
    pub mix: FloatParam,
    #[id = "BloomRise"]
    pub rise: FloatParam,
    #[id = "BloomSize"]
    pub size: FloatParam,
    #[id = "BloomFeedback"]
    pub feedback: FloatParam,
    #[id = "BloomTone"]
    pub tone: FloatParam,
    #[id = "BloomEnabled"]
    pub enabled: BoolParam,
}

/// ILLUSION – granular cloud with pitched overtones.
///
/// Eight parameters, all stored as normalised 0..1 values on the
/// host side. The DSP maps them to their physical ranges internally.
#[derive(Params)]
pub struct IllusionParams {
    #[id = "IllusionSpark"]
    pub spark: FloatParam,
    #[id = "IllusionStill"]
    pub still: FloatParam,
    #[id = "IllusionAura"]
    pub aura: FloatParam,
    #[id = "IllusionVeil"]
    pub veil: FloatParam,
    #[id = "IllusionWide"]
    pub wide: FloatParam,
    #[id = "IllusionTint"]
    pub tint: FloatParam,
    #[id = "IllusionRate"]
    pub rate: FloatParam,
    #[id = "IllusionGrain"]
    pub grain: FloatParam,
    #[id = "IllusionEnabled"]
    pub enabled: BoolParam,
}

impl Default for IllusionParams {
    fn default() -> Self {
        Self {
            spark: FloatParam::new("IllusionSpark", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            still: FloatParam::new("IllusionStill", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| {
                    if v < 0.02 {
                        "scattered".to_string()
                    } else if v > 0.98 {
                        "calm".to_string()
                    } else {
                        format!("{:.0} %", v * 100.0)
                    }
                })),
            // AURA: normalised 0..1 maps to -24..=+24 semitones.
            // 0.5 = 0 st (unison with the internal sequence).
            aura: FloatParam::new(
                "IllusionAura",
                0.5, // 0 st
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(1.0 / 48.0) // 1 semitone per step
            .with_value_to_string(Arc::new(|v| {
                let semis = ((v - 0.5) * 48.0).round() as i32;
                if semis == 0 {
                    "unison".to_string()
                } else {
                    format!("{:+} st", semis)
                }
            })),
            veil: FloatParam::new("IllusionVeil", 0.4, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            wide: FloatParam::new(
                "IllusionWide",
                0.5, // 0.5 * 2 = 1.0 = natural width
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                let w = v * 2.0;
                if w < 0.05 {
                    "mono".to_string()
                } else {
                    format!("{:.0} %", w * 100.0)
                }
            })),
            tint: FloatParam::new("IllusionTint", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| {
                    let hz = 800.0 + v * (18000.0 - 800.0);
                    format!("{:.0} Hz", hz)
                })),
            rate: FloatParam::new("IllusionRate", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| {
                    let hz = 0.5 * (8.0_f32 / 0.5).powf(v);
                    format!("{:.2} Hz", hz)
                })),
            grain: FloatParam::new("IllusionGrain", 0.4, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} ms", 30.0 + v * 270.0))),
            enabled: BoolParam::new("IllusionEnabled", false),
        }
    }
}

impl Default for BloomParams {
    fn default() -> Self {
        Self {
            mix: FloatParam::new("BloomMix", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            rise: FloatParam::new("BloomRise", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| {
                    let ms = 50.0 + (1.0 - v) * (2000.0 - 50.0);
                    if ms < 1000.0 {
                        format!("{:.0} ms", ms)
                    } else {
                        format!("{:.2} s", ms / 1000.0)
                    }
                })),
            size: FloatParam::new("BloomSize", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} ms", 60.0 + v * (250.0 - 60.0)))),
            feedback: FloatParam::new(
                "BloomFeedback",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            tone: FloatParam::new("BloomTone", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| {
                    format!("{:.0} Hz", 2000.0 + v * (12000.0 - 2000.0))
                })),
            enabled: BoolParam::new("BloomEnabled", false),
        }
    }
}

/// Velvet-Noise diffusion parameters.
#[derive(Params)]
pub struct VelvetParams {
    #[id = "VelvetDiffusion"]
    pub diffusion: FloatParam,
    #[id = "VelvetDensity"]
    pub density: FloatParam,
    #[id = "VelvetTone"]
    pub tone: FloatParam,
}

impl Default for VelvetParams {
    fn default() -> Self {
        Self {
            diffusion: FloatParam::new(
                "VelvetDiffusion",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            density: FloatParam::new(
                "VelvetDensity",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                format!("{:.0} p/s", 1500.0 + v * (3000.0 - 1500.0))
            })),
            tone: FloatParam::new("VelvetTone", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| {
                    const MAX_RRS_WIDTH: f32 = 30.0;
                    let width = 1.0 + v * (MAX_RRS_WIDTH - 1.0);
                    if v < 0.02 {
                        "off".to_string()
                    } else {
                        format!("{:.0} smp", width)
                    }
                })),
        }
    }
}

/// FFDN engine parameters.
///
/// These are only active when the ENGINE card's switch is set to
/// FFDN; when CloudSeed is selected, the values are stored but not
/// forwarded to the audio thread.
#[derive(Params)]
pub struct FfdnParams {
    #[id = "UseFfdn"]
    pub use_ffdn: BoolParam,

    #[id = "FfdnSize"]
    pub size: FloatParam,

    #[id = "FfdnDecay"]
    pub decay: FloatParam,

    #[id = "FfdnDamping"]
    pub damping: FloatParam,

    #[id = "FfdnSpread"]
    pub spread: FloatParam,

    /// Modulation depth. 0 = off, 3 ms = noticeable chorus.
    #[id = "FfdnModAmt"]
    pub mod_amount: FloatParam,

    /// Modulation rate. 0.1 Hz = very slow, 5 Hz = chorus-like.
    #[id = "FfdnModRate"]
    pub mod_rate: FloatParam,

    /// Feedback matrix type.
    /// 0.0 = Householder (dense), 1.0 = Velvet (sparse).
    #[id = "FfdnMatrix"]
    pub matrix_type: FloatParam,

    /// Bass RT60 multiplier. `2^((v - 0.5) * 2)`.
    #[id = "FfdnBassMult"]
    pub bass_mult: FloatParam,

    /// Bass crossover frequency.
    #[id = "FfdnBassFreq"]
    pub bass_freq: FloatParam,

    /// Treble RT60 multiplier.
    #[id = "FfdnTrebleMult"]
    pub treble_mult: FloatParam,

    /// Treble crossover frequency.
    #[id = "FfdnTrebleFreq"]
    pub treble_freq: FloatParam,

    /// Input-diffusion amount (0..1). 0 = bypass (bit-identical),
    /// 1 = strong smoothing of the initial transient.
    #[id = "FfdnDiffusion"]
    pub diffusion: FloatParam,

    // ============================================================
    // FFDN CHARACTER
    // ============================================================
    /// Cross-feed between the L and R networks (0..1). 0 = fully
    /// independent stereo, 0.5 = mono sum, 1.0 = swapped channels.
    #[id = "FfdnCross"]
    pub cross: FloatParam,

    /// Soft saturation in the feedback loop (0..1).
    #[id = "Saturation"]
    pub drive: FloatParam,

    /// Crossover of the tilt filter (Hz).
    #[id = "FfdnTiltFreq"]
    pub tilt_freq: FloatParam,

    /// Spectral tilt. 0.0 = bass-heavy, 0.5 = flat, 1.0 = treble-heavy.
    #[id = "FfdnTiltAmt"]
    pub tilt_amount: FloatParam,

    /// Early-reflection blend (0..1).
    #[id = "FfdnErLevel"]
    pub er_level: FloatParam,

    /// Enable switches for the three CHARACTER stages.
    #[id = "SaturationEnabled"]
    pub drive_enabled: BoolParam,
    #[id = "FfdnTiltEnabled"]
    pub tilt_enabled: BoolParam,
    #[id = "FfdnErEnabled"]
    pub er_enabled: BoolParam,

    // ============================================================
    // FFDN NETWORK
    // ============================================================
    /// Number of active delay lines per FFDN network, 4..16.
    #[id = "FfdnLines"]
    pub lines: FloatParam,

    /// Input distribution density, 0..1.
    #[id = "FfdnDensity"]
    pub density: FloatParam,

    /// L/R network delay offset amount, 0..1.
    #[id = "FfdnDecorr"]
    pub decorr: FloatParam,

    // ============================================================
    // FFDN FILTER
    // ============================================================
    /// Loop high-pass cutoff (Hz). 20 Hz is neutral.
    #[id = "FfdnLoopHP"]
    pub loop_hp: FloatParam,

    /// Loop low-pass cutoff (Hz). 20 kHz is neutral.
    #[id = "FfdnLoopLP"]
    pub loop_lp: FloatParam,

    /// Mid-band emphasis (0..1). 0 is neutral.
    #[id = "FfdnMidFocus"]
    pub mid_focus: FloatParam,

    // ============================================================
    // FFDN DYNAMICS
    // ============================================================
    /// Attack envelope amount (0..1). 0 is neutral.
    #[id = "FfdnAttack"]
    pub attack: FloatParam,

    /// Ducking amount (0..1). 0 is neutral.
    #[id = "FfdnDucking"]
    pub ducking: FloatParam,

    /// Gate threshold (0..1). 0 is neutral.
    #[id = "FfdnGate"]
    pub gate: FloatParam,

    // ============================================================
    // FFDN CARD MASTER ENABLES
    // ============================================================
    //
    // When a card's switch is off, the audio thread pushes neutral
    // values for that card's parameters to the DSP, and the UI greys
    // the card out. The user's stored parameter values are preserved
    // so toggling the switch back on restores the exact previous
    // state. This mirrors the master-EQ-switch pattern used on the
    // CloudSeed page.
    #[id = "FfdnSpaceEnabled"]
    pub space_enabled: BoolParam,
    #[id = "FfdnNetworkEnabled"]
    pub network_enabled: BoolParam,
    #[id = "FfdnCharacterEnabled"]
    pub character_enabled: BoolParam,
    #[id = "FfdnShelvesEnabled"]
    pub shelves_enabled: BoolParam,
    #[id = "FfdnModulationEnabled"]
    pub modulation_enabled: BoolParam,
    #[id = "FfdnFilterEnabled"]
    pub filter_enabled: BoolParam,
    #[id = "FfdnDynamicsEnabled"]
    pub dynamics_enabled: BoolParam,
}

impl Default for FfdnParams {
    fn default() -> Self {
        Self {
            use_ffdn: BoolParam::new("UseFfdn", false),
            size: FloatParam::new(
                "FfdnSize",
                0.23, // ~50 ms
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} ms", 5.0 + v * 195.0))),
            decay: FloatParam::new(
                "FfdnDecay",
                0.05, // ~3.1 s with the new 60 s range
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                let s = 0.1 + v * 59.9; // 0.1 s .. 60 s
                if s < 1.0 {
                    format!("{:.0} ms", s * 1000.0)
                } else {
                    format!("{:.2} s", s)
                }
            })),
            damping: FloatParam::new(
                "FfdnDamping",
                0.385, // ~8000 Hz
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} Hz", 500.0 + v * 19500.0))),
            spread: FloatParam::new("FfdnSpread", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),
            mod_amount: FloatParam::new(
                "FfdnModAmt",
                0.167, // ~0.5 ms
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.2} ms", v * 3.0))),
            mod_rate: FloatParam::new(
                "FfdnModRate",
                0.122, // ~0.7 Hz
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.2} Hz", 0.1 + v * 4.9))),
            matrix_type: FloatParam::new(
                "FfdnMatrix",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(1.0)
            .with_value_to_string(Arc::new(|v| {
                if v < 0.5 {
                    "House".to_string()
                } else {
                    "Velvet".to_string()
                }
            })),

            bass_mult: FloatParam::new(
                "FfdnBassMult",
                0.5, // 2^0 = 1.0, neutral
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                let m = 2.0_f32.powf((v - 0.5) * 2.0);
                format!("{:.2}x", m)
            })),

            bass_freq: FloatParam::new(
                "FfdnBassFreq",
                0.537, // ~250 Hz
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} Hz", 50.0 * 20.0_f32.powf(v)))),

            treble_mult: FloatParam::new(
                "FfdnTrebleMult",
                0.5, // 2^0 = 1.0, neutral
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                let m = 2.0_f32.powf((v - 0.5) * 2.0);
                format!("{:.2}x", m)
            })),

            treble_freq: FloatParam::new(
                "FfdnTrebleFreq",
                0.512, // ~4000 Hz
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} Hz", 1000.0 * 15.0_f32.powf(v)))),

            diffusion: FloatParam::new(
                "FfdnDiffusion",
                0.5, // moderate default
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),

            // ---- FFDN CHARACTER ---------------------------
            // Defaults chosen so that a fresh instance is bit-identical
            // to the pre- engine: all three CHARACTER switches
            // start OFF, and the numeric values are their neutral
            // positions (cross = 0 = independent, drive = 0 = linear,
            // tilt_amount = 0.5 = flat).
            cross: FloatParam::new("FfdnCross", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),

            drive: FloatParam::new("Saturation", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),

            tilt_freq: FloatParam::new(
                "FfdnTiltFreq",
                0.5, // ~1 kHz on the log scale below
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                // 100 Hz .. 10 kHz, log-spaced.
                let hz = 100.0 * 100.0_f32.powf(v);
                format!("{:.0} Hz", hz)
            })),

            tilt_amount: FloatParam::new(
                "FfdnTiltAmt",
                0.5, // flat
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                let db = (v - 0.5) * 12.0;
                format!("{:+.1} dB", db)
            })),

            er_level: FloatParam::new(
                "FfdnErLevel",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),

            drive_enabled: BoolParam::new("SaturationEnabled", false),
            tilt_enabled: BoolParam::new("FfdnTiltEnabled", false),
            er_enabled: BoolParam::new("FfdnErEnabled", false),

            // ---- FFDN NETWORK -----------------------------
            // `lines = 0.333` maps to 8 lines, matching the fixed
            // value used before this release. `density` and `decorr`
            // default to their neutral centers.
            lines: FloatParam::new(
                "FfdnLines",
                0.333,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(1.0 / 12.0)
            .with_value_to_string(Arc::new(|v| {
                let n = 4 + (v * 12.0).round() as i32;
                format!("{}", n.clamp(4, 16))
            })),

            density: FloatParam::new(
                "FfdnDensity",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),

            decorr: FloatParam::new("FfdnDecorr", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| {
                    // Map 0..1 -> 1%..7% L/R delay offset.
                    let pct = 1.0 + v * 6.0;
                    format!("{:.1} %", pct)
                })),

            // ---- FFDN FILTER ------------------------------
            loop_hp: FloatParam::new(
                "FfdnLoopHP",
                0.0, // 20 Hz, neutral
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                if v < 0.005 {
                    "off".to_string()
                } else {
                    // Log-spaced 20..500 Hz
                    let hz = 20.0 * (500.0_f32 / 20.0).powf(v);
                    format!("{:.0} Hz", hz)
                }
            })),

            loop_lp: FloatParam::new(
                "FfdnLoopLP",
                1.0, // 20 kHz, neutral
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| {
                if v > 0.995 {
                    "off".to_string()
                } else {
                    let hz = 2000.0 * (20000.0_f32 / 2000.0).powf(v);
                    format!("{:.0} Hz", hz)
                }
            })),

            mid_focus: FloatParam::new(
                "FfdnMidFocus",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),

            // ---- FFDN DYNAMICS ----------------------------
            attack: FloatParam::new("FfdnAttack", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),

            ducking: FloatParam::new(
                "FfdnDucking",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),

            gate: FloatParam::new("FfdnGate", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| format!("{:.0} %", v * 100.0))),

            // ---- FFDN CARD MASTER ENABLES ----------------
            // All default to `true` so an existing session that never
            // touched these is bit-identical to the pre- engine.
            space_enabled: BoolParam::new("FfdnSpaceEnabled", true),
            network_enabled: BoolParam::new("FfdnNetworkEnabled", true),
            character_enabled: BoolParam::new("FfdnCharacterEnabled", true),
            shelves_enabled: BoolParam::new("FfdnShelvesEnabled", true),
            modulation_enabled: BoolParam::new("FfdnModulationEnabled", true),
            filter_enabled: BoolParam::new("FfdnFilterEnabled", true),
            dynamics_enabled: BoolParam::new("FfdnDynamicsEnabled", true),
        }
    }
}

// ============================================================================
// Global parameters
// ============================================================================

/// Global (bypass / freeze) parameters.
#[derive(Params)]
pub struct GlobalParams {
    #[id = "bypass"]
    pub bypass: BoolParam,
    #[id = "freeze"]
    pub freeze: BoolParam,
}

impl Default for GlobalParams {
    fn default() -> Self {
        Self {
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            freeze: BoolParam::new("Freeze", false),
        }
    }
}

// ============================================================================
// Top-level parameters struct
// ============================================================================

#[derive(Params)]
pub struct PejvakRvbParams {
    #[nested(group = "Input")]
    pub input: InputParams,

    #[nested(group = "Early")]
    pub early: EarlyParams,

    #[nested(group = "Late")]
    pub late: LateParams,

    #[nested(group = "EQ")]
    pub eq: EqParams,

    #[nested(group = "Modulation")]
    pub modulation: ModulationParams,

    #[nested(group = "Seeds")]
    pub seeds: SeedParams,

    #[nested(group = "Output")]
    pub output: OutputParams,

    #[nested(group = "Switches")]
    pub switches: SwitchParams,

    #[nested(group = "Global")]
    pub global: GlobalParams,

    #[nested(group = "Texture Shimmer")]
    pub shimmer: ShimmerParams,

    #[nested(group = "Texture Mist")]
    pub mist: MistParams,

    #[nested(group = "Texture Drift")]
    pub drift: DriftParams,

    #[nested(group = "Texture Crystal")]
    pub crystal: CrystalParams,

    #[nested(group = "Texture Scatter")]
    pub scatter: ScatterParams,

    #[nested(group = "Texture Bloom")]
    pub bloom: BloomParams,

    #[nested(group = "Illusion")]
    pub illusion: IllusionParams,

    #[nested(group = "Velvet")]
    pub velvet: VelvetParams,
    pub texture_order: Arc<AtomicU64>,

    #[nested(group = "FFDN")]
    pub ffdn: FfdnParams,

    #[cfg(feature = "gui-egui")]
    pub reset_requested: Arc<AtomicBool>,

    #[cfg(feature = "gui-egui")]
    pub editor_state: Arc<nice_plug_egui::EguiEditorState>,
    #[cfg(feature = "gui-egui")]
    #[persist = "is_light_theme"]
    pub is_light_theme: Arc<AtomicBool>,

    #[cfg(feature = "gui-egui")]
    #[persist = "show_popups"]
    pub show_popups: Arc<AtomicBool>,

    /// Name of the currently-loaded preset, persisted with the
    /// plugin state so the label survives a close/reopen of the
    /// editor and a session reload.
    ///
    /// Using `Arc<Mutex<String>>` (rather than an atomic) because
    /// nice-plug's `#[persist]` requires interior mutability and
    /// `String` has no atomic form.
    #[cfg(feature = "gui-egui")]
    #[persist = "current_preset"]
    pub current_preset: Arc<std::sync::Mutex<String>>,

    #[cfg(feature = "gui-egui")]
    pub default_width: Arc<AtomicU32>,
    #[cfg(feature = "gui-egui")]
    pub default_height: Arc<AtomicU32>,
}

impl Default for PejvakRvbParams {
    fn default() -> Self {
        // Read the persisted default size from disk. This runs at
        // plugin instantiation (not on the audio thread), so the
        // file I/O is safe. If the file is missing, both values are
        // 0 and the editor will use the factory default of
        // `WINDOW_W × WINDOW_H`.
        #[cfg(feature = "gui-egui")]
        let (dw, dh) = load_default_size();

        Self {
            input: InputParams::default(),
            early: EarlyParams::default(),
            late: LateParams::default(),
            eq: EqParams::default(),
            modulation: ModulationParams::default(),
            seeds: SeedParams::default(),
            output: OutputParams::default(),
            switches: SwitchParams::default(),
            global: GlobalParams::default(),
            shimmer: ShimmerParams::default(),
            mist: MistParams::default(),
            drift: DriftParams::default(),
            crystal: CrystalParams::default(),
            scatter: ScatterParams::default(),
            bloom: BloomParams::default(),
            illusion: IllusionParams::default(),
            velvet: VelvetParams::default(),
            ffdn: FfdnParams::default(),
            texture_order: Arc::new(AtomicU64::new(DEFAULT_TEXTURE_ORDER_PACKED)),
            #[cfg(feature = "gui-egui")]
            reset_requested: Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "gui-egui")]
            editor_state: nice_plug_egui::EguiEditorState::from_size(
                nice_plug::editor::dpi::Size::Logical(nice_plug::editor::dpi::LogicalSize::new(
                    editor::WINDOW_W as f64,
                    editor::WINDOW_H as f64,
                )),
                1.0,
            ),
            #[cfg(feature = "gui-egui")]
            is_light_theme: Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "gui-egui")]
            show_popups: Arc::new(AtomicBool::new(true)),
            #[cfg(feature = "gui-egui")]
            current_preset: Arc::new(std::sync::Mutex::new("Init".to_string())),
            #[cfg(feature = "gui-egui")]
            default_width: Arc::new(AtomicU32::new(dw)),
            #[cfg(feature = "gui-egui")]
            default_height: Arc::new(AtomicU32::new(dh)),
        }
    }
}

impl PejvakRvbParams {
    /// Returns the normalised (0..1) value for parameter index `i`.
    pub fn normalized(&self, i: usize) -> f64 {
        use Parameter::*;
        let p = match Parameter::from_index(i) {
            Some(p) => p,
            None => return 0.0,
        };
        let v: f64 = match p {
            InputMix => self.input.mix.value() as f64,
            PreDelay => self.input.predelay.value() as f64,
            HighPass => self.input.high_pass.value() as f64,
            LowPass => self.input.low_pass.value() as f64,
            TapEnabled => bool_to_f64(self.early.tap_enabled.value()),
            TapCount => self.early.tap_count.value() as f64,
            TapLength => self.early.tap_length.value() as f64,
            TapGain => self.early.tap_gain.value() as f64,
            TapDecay => self.early.tap_decay.value() as f64,
            DiffusionEnabled => bool_to_f64(self.early.diffusion_enabled.value()),
            DiffusionStages => self.early.diffusion_stages.value() as f64,
            DiffusionDelay => self.early.diffusion_delay.value() as f64,
            DiffusionFeedback => self.early.diffusion_feedback.value() as f64,
            LateDelayEnabled => bool_to_f64(self.late.late_delay_enabled.value()),
            LineCount => self.late.line_count.value() as f64,
            LineDelay => self.late.line_delay.value() as f64,
            LineDecay => self.late.line_decay.value() as f64,
            LateDiffusionEnabled => bool_to_f64(self.late.late_diffusion_enabled.value()),
            LateDiffusionStages => self.late.late_diffusion_stages.value() as f64,
            LateDiffusionDelay => self.late.late_diffusion_delay.value() as f64,
            LateDiffusionFeedback => self.late.late_diffusion_feedback.value() as f64,
            PostLowShelfGain => self.eq.low_shelf_gain.value() as f64,
            PostLowShelfFrequency => self.eq.low_shelf_frequency.value() as f64,
            PostHighShelfGain => self.eq.high_shelf_gain.value() as f64,
            PostHighShelfFrequency => self.eq.high_shelf_frequency.value() as f64,
            PostCutoffFrequency => self.eq.cutoff_frequency.value() as f64,
            EarlyDiffusionModAmount => self.modulation.early_diffusion_mod_amount.value() as f64,
            EarlyDiffusionModRate => self.modulation.early_diffusion_mod_rate.value() as f64,
            LineModAmount => self.modulation.line_mod_amount.value() as f64,
            LineModRate => self.modulation.line_mod_rate.value() as f64,
            LateDiffusionModAmount => self.modulation.late_diffusion_mod_amount.value() as f64,
            LateDiffusionModRate => self.modulation.late_diffusion_mod_rate.value() as f64,
            TapSeed => self.seeds.tap_seed.value() as f64,
            DiffusionSeed => self.seeds.diffusion_seed.value() as f64,
            DelaySeed => self.seeds.delay_seed.value() as f64,
            PostDiffusionSeed => self.seeds.post_diffusion_seed.value() as f64,
            CrossSeed => self.seeds.cross_seed.value() as f64,
            DryOut => self.output.dry_out.value() as f64,
            PredelayOut => self.output.predelay_out.value() as f64,
            EarlyOut => self.output.early_out.value() as f64,
            MainOut => self.output.main_out.value() as f64,
            WidthOut => self.output.width.value() as f64,
            HiPassEnabled => bool_to_f64(self.switches.hi_pass_enabled.value()),
            LowPassEnabled => bool_to_f64(self.switches.low_pass_enabled.value()),
            LowShelfEnabled => bool_to_f64(self.switches.low_shelf_enabled.value()),
            HighShelfEnabled => bool_to_f64(self.switches.high_shelf_enabled.value()),
            CutoffEnabled => bool_to_f64(self.switches.cutoff_enabled.value()),
            LateStageTap => bool_to_f64(self.switches.late_stage_tap.value()),
            Interpolation => bool_to_f64(self.switches.interpolation.value()),
        };
        if i == 12 {
            v.min(DIFFUSION_FEEDBACK_CAP)
        } else {
            v
        }
    }
}

// ============================================================================
// Plugin
// ============================================================================

pub struct PejvakRvbPlugin {
    params: Arc<PejvakRvbParams>,
    engine: Option<ReverbController>,
    mono_scratch: Vec<f32>,
    sample_rate: f32,
    idle: TailIdle,
    last_pushed: [f64; PARAM_COUNT],
    last_tex: [f64; TEX_COUNT],
    /// Cache of the last FFDN parameter values pushed into the DSP
    /// core. Layout:
    ///   [0] use_ffdn (0.0/1.0)
    ///   [1] size_ms
    ///   [2] decay_sec
    ///   [3] damping_hz
    ///   [4] spread
    ///   [5] mod_amount_ms
    ///   [6] mod_rate_hz
    ///   [7] matrix_type (0.0 House / 1.0 Velvet)
    ///   [8] bass_mult
    ///   [9] treble_mult
    ///   [10] bass_freq
    ///   [11] treble_freq
    ///   [12] diffusion
    ///   -- FFDN CHARACTER --
    ///   [13] cross
    ///   [14] drive
    ///   [15] tilt_freq_hz
    ///   [16] tilt_amount
    ///   [17] er_level
    ///   [18] drive_enabled (0.0/1.0)
    ///   [19] tilt_enabled (0.0/1.0)
    ///   [20] er_enabled (0.0/1.0)
    ///   -- FFDN NETWORK / FILTER / DYNAMICS --
    ///   [21] lines (4..16 as f64)
    ///   [22] density
    ///   [23] decorr
    ///   [24] loop_hp_hz
    ///   [25] loop_lp_hz
    ///   [26] mid_focus
    ///   [27] attack
    ///   [28] ducking
    ///   [29] gate
    last_ffdn: [f64; 30],
    /// Cache of the last texture-order value actually pushed into the
    /// DSP core.
    last_order_packed: u64,
    /// Cache of the last ILLUSION parameter values pushed into the DSP
    /// core. Layout: [mix, rate, steps, octaves, scale, random, grain,
    /// feedback].
    last_illusion: [f64; 8],
}

impl Default for PejvakRvbPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(PejvakRvbParams::default()),
            engine: None,
            mono_scratch: Vec::new(),
            sample_rate: 48_000.0,
            idle: TailIdle::new(),
            last_pushed: [f64::NAN; PARAM_COUNT],
            last_tex: [f64::NAN; TEX_COUNT],
            last_order_packed: u64::MAX,
            last_illusion: [f64::NAN; 8],
            last_ffdn: [f64::NAN; 30],
        }
    }
}

impl Plugin for PejvakRvbPlugin {
    const NAME: &'static str = "PEJVAK RVB";
    // Brand, not author. The individual developer is credited in
    // Cargo.toml `authors`, the LICENSE file, and the About modal.
    const VENDOR: &'static str = "Pejvak DSP";
    const URL: &'static str = "https://github.com/Pejvak-DSP";
    const EMAIL: &'static str = "info@pejvakdsp.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    #[cfg(feature = "gui-egui")]
    type Editor = nice_plug_egui::EguiEditor<editor::PejvakEguiApp>;

    #[cfg(not(feature = "gui-egui"))]
    type Editor = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    #[cfg(feature = "gui-egui")]
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        editor::create_editor(self.params.clone())
    }

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        Box::new(|_| {})
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        self.idle = TailIdle::new();

        let ctrl = ReverbController::new(buffer_config.sample_rate as i32);
        self.engine = Some(ctrl);
        self.mono_scratch = vec![0.0; (buffer_config.max_buffer_size as usize).max(8192)];

        self.last_pushed = [f64::NAN; PARAM_COUNT];
        self.last_tex = [f64::NAN; TEX_COUNT];
        self.last_order_packed = u64::MAX;

        self.last_illusion = [f64::NAN; 8];
        self.last_ffdn = [f64::NAN; 30];

        true
    }

    fn reset(&mut self) {
        if let Some(e) = &mut self.engine {
            e.clear_buffers();
        }
        self.idle = TailIdle::new();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        if self.params.global.bypass.value() {
            return ProcessStatus::Normal;
        }

        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.process_inner(buffer)));

        match result {
            Ok(status) => status,
            Err(_) => {
                for ch in buffer.as_slice() {
                    ch.fill(0.0);
                }
                ProcessStatus::Normal
            }
        }
    }

    fn filter_state(_state: &mut PluginState) {}
}

// ============================================================================
// Real-time processing body (guarded by catch_unwind above)
// ============================================================================

impl PejvakRvbPlugin {
    fn process_inner(&mut self, buffer: &mut Buffer) -> ProcessStatus {
        let ctrl = match &mut self.engine {
            Some(e) => e,
            None => return ProcessStatus::Normal,
        };

        // ---- Preset-change reset ---------------------------------------
        // When the editor loads a preset it sets this flag. We act on it
        // here, on the audio thread, by clearing the DSP engine's internal
        // buffers (reverb tail, feedback loops, delay lines) so the
        // previous preset's sound is cut off immediately and cannot
        // bleed into the new one.
        //
        // We also invalidate the parameter cache so the next push loop
        // re-sends every value to the fresh engine, regardless of what
        // the previous preset's values were.
        #[cfg(feature = "gui-egui")]
        if self.params.reset_requested.swap(false, Ordering::Relaxed) {
            ctrl.clear_buffers();
            self.last_pushed = [f64::NAN; PARAM_COUNT];
            self.last_tex = [f64::NAN; TEX_COUNT];
            self.last_illusion = [f64::NAN; 8];
            self.last_ffdn = [f64::NAN; 30];
            self.idle = TailIdle::new();
        }

        // ---- Texture chain order ----------------------------------------
        // The UI stores the layer processing order as a packed u64. We
        // only forward it to the DSP core when it actually changes, so
        // this costs a single atomic load per block in the steady state.
        {
            let packed = self.params.texture_order.load(Ordering::Relaxed);
            if packed != self.last_order_packed {
                let order = [
                    (packed & 0xFF) as usize,
                    ((packed >> 8) & 0xFF) as usize,
                    ((packed >> 16) & 0xFF) as usize,
                    ((packed >> 24) & 0xFF) as usize,
                    ((packed >> 32) & 0xFF) as usize,
                    ((packed >> 40) & 0xFF) as usize,
                ];
                ctrl.set_texture_order(order);
                self.last_order_packed = packed;
            }
        }

        let mut in_peak = 0.0f32;
        for ch in buffer.as_slice_immutable() {
            for s in ch.iter() {
                let a = s.abs();
                if a > in_peak {
                    in_peak = a;
                }
            }
        }

        if !self.idle.wake_check(in_peak) {
            for ch in buffer.as_slice() {
                ch.fill(0.0);
            }
            return ProcessStatus::Normal;
        }

        // ---- Wake-transition cache invalidation ----------------------
        // When the plugin wakes up from sleep, invalidate the
        // parameter caches so that every parameter is re-pushed on
        // this block, exactly as if the plugin had just been
        // activated. This guards against the case where the host
        // changes a parameter while the plugin is asleep: the audio
        // thread never sees the change (because `process_inner`
        // returns early), so `last_pushed` / `last_tex` would
        // otherwise still hold the pre-sleep values and the next
        // push loop would skip the update.
        if self.idle.did_wake() {
            self.last_pushed = [f64::NAN; PARAM_COUNT];
            self.last_tex = [f64::NAN; TEX_COUNT];
        }

        // Master EQ switch: when disabled, force the three per-stage
        // enables (indices 44, 45, 46) to zero. The user's stored
        // values are preserved in the parameter set, so turning the
        // master back on restores their selection — the parameter
        // cache (`last_pushed`) will see the difference and re-push.
        let eq_off = !self.params.eq.enabled.value();

        for i in 0..PARAM_COUNT {
            let mut v = self.params.normalized(i);
            if eq_off && matches!(i, 44..=46) {
                v = 0.0;
            }
            if v != self.last_pushed[i] {
                ctrl.set_parameter(i, v);
                self.last_pushed[i] = v;
                self.idle.silent_samples = 0;
                self.idle.sleeping = false;
            }
        }
        ctrl.set_freeze(self.params.global.freeze.value());

        let cache = &mut self.last_tex;

        // ---- Shimmer ---------------------------------------------------
        let eff_shimmer_mix = if self.params.shimmer.enabled.value() {
            self.params.shimmer.mix.value()
        } else {
            0.0
        };
        push_tex(
            ctrl,
            cache,
            TextureParamId::ShimmerMix,
            eff_shimmer_mix as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::ShimmerPitch,
            self.params.shimmer.pitch.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::ShimmerFeedback,
            self.params.shimmer.feedback.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::ShimmerLowCut,
            self.params.shimmer.low_cut.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::ShimmerHighCut,
            self.params.shimmer.high_cut.value() as f64,
        );

        // ---- Mist ------------------------------------------------------
        let eff_mist_mix = if self.params.mist.enabled.value() {
            self.params.mist.mix.value()
        } else {
            0.0
        };
        push_tex(ctrl, cache, TextureParamId::MistMix, eff_mist_mix as f64);
        push_tex(
            ctrl,
            cache,
            TextureParamId::MistHP,
            self.params.mist.hp.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::MistFeedback,
            self.params.mist.feedback.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::MistDelay,
            self.params.mist.delay.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::MistRate,
            self.params.mist.rate.value() as f64,
        );

        // ---- Drift -----------------------------------------------------
        let eff_drift_mix = if self.params.drift.enabled.value() {
            self.params.drift.mix.value()
        } else {
            0.0
        };
        push_tex(ctrl, cache, TextureParamId::DriftMix, eff_drift_mix as f64);
        push_tex(
            ctrl,
            cache,
            TextureParamId::DriftFeedback,
            self.params.drift.feedback.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::DriftDelay,
            self.params.drift.delay.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::DriftRate,
            self.params.drift.rate.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::DriftDepth,
            self.params.drift.depth.value() as f64,
        );

        // ---- Crystal ---------------------------------------------------
        let eff_crystal_mix = if self.params.crystal.enabled.value() {
            self.params.crystal.mix.value()
        } else {
            0.0
        };
        push_tex(
            ctrl,
            cache,
            TextureParamId::CrystalMix,
            eff_crystal_mix as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::CrystalSize,
            self.params.crystal.size.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::CrystalDensity,
            self.params.crystal.density.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::CrystalFeedback,
            self.params.crystal.feedback.value() as f64,
        );

        // ---- Scatter ---------------------------------------------------
        let eff_scatter_mix = if self.params.scatter.enabled.value() {
            self.params.scatter.mix.value()
        } else {
            0.0
        };
        push_tex(
            ctrl,
            cache,
            TextureParamId::ScatterMix,
            eff_scatter_mix as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::ScatterSpread,
            self.params.scatter.spread.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::ScatterDensity,
            self.params.scatter.density.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::ScatterSpeed,
            self.params.scatter.speed.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::ScatterFeedback,
            self.params.scatter.feedback.value() as f64,
        );

        // ---- Bloom -----------------------------------------------------
        let eff_bloom_mix = if self.params.bloom.enabled.value() {
            self.params.bloom.mix.value()
        } else {
            0.0
        };
        push_tex(ctrl, cache, TextureParamId::BloomMix, eff_bloom_mix as f64);
        push_tex(
            ctrl,
            cache,
            TextureParamId::BloomRise,
            self.params.bloom.rise.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::BloomSize,
            self.params.bloom.size.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::BloomFeedback,
            self.params.bloom.feedback.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::BloomTone,
            self.params.bloom.tone.value() as f64,
        );

        // ---- Velvet Noise ----------------------------------------------
        push_tex(
            ctrl,
            cache,
            TextureParamId::VelvetDiffusion,
            self.params.velvet.diffusion.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::VelvetDensity,
            self.params.velvet.density.value() as f64,
        );
        push_tex(
            ctrl,
            cache,
            TextureParamId::VelvetTone,
            self.params.velvet.tone.value() as f64,
        );

        // ---- ILLUSION — granular cloud with pitched overtones ---------------

        let illusion_spark = if self.params.illusion.enabled.value() {
            self.params.illusion.spark.value() as f64
        } else {
            0.0
        };

        // AURA is stored as a normalised 0..1 host parameter, but the
        // DSP wants semitones. Convert here so the cache stores the
        // physical value (and a normalised-value change that would
        // round to the same semitone does not spam the DSP).
        let aura_norm = self.params.illusion.aura.value() as f64;
        let aura_semis = ((aura_norm - 0.5) * 48.0).round();

        let illusion_vals: [f64; 8] = [
            illusion_spark,
            self.params.illusion.still.value() as f64,
            aura_semis,
            self.params.illusion.veil.value() as f64,
            self.params.illusion.wide.value() as f64,
            self.params.illusion.tint.value() as f64,
            self.params.illusion.rate.value() as f64,
            self.params.illusion.grain.value() as f64,
        ];

        for (i, &val) in illusion_vals.iter().enumerate() {
            if val != self.last_illusion[i] {
                match i {
                    0 => ctrl.set_illusion_spark(val),
                    1 => ctrl.set_illusion_still(val),
                    2 => ctrl.set_illusion_aura(val),
                    3 => ctrl.set_illusion_veil(val),
                    4 => ctrl.set_illusion_wide(val),
                    5 => ctrl.set_illusion_tint(val),
                    6 => ctrl.set_illusion_rate(val),
                    7 => ctrl.set_illusion_grain(val),
                    _ => {}
                }
                self.last_illusion[i] = val;
            }
        }

        // ---- FFDN engine -------------------------------------------------
        let use_ffdn = self.params.ffdn.use_ffdn.value();
        let engine_target = if use_ffdn {
            ReverbEngine::Ffdn
        } else {
            ReverbEngine::CloudSeed
        };
        let engine_val = if use_ffdn { 1.0 } else { 0.0 };
        if engine_val != self.last_ffdn[0] {
            ctrl.set_engine(engine_target);
            self.last_ffdn[0] = engine_val;
        }

        // ---- Card master enables -----------------------------------------
        // When a card is off, we push neutral values for its parameters
        // so the DSP behaves as if the card didn't exist. The user's
        // stored values are preserved and restored the moment the
        // switch is turned back on (the change in effective value is
        // what triggers the cache to re-push).
        let space_on = self.params.ffdn.space_enabled.value();
        let network_on = self.params.ffdn.network_enabled.value();
        let character_on = self.params.ffdn.character_enabled.value();
        let shelves_on = self.params.ffdn.shelves_enabled.value();
        let modulation_on = self.params.ffdn.modulation_enabled.value();
        let filter_on = self.params.ffdn.filter_enabled.value();
        let dynamics_on = self.params.ffdn.dynamics_enabled.value();

        // ---- SPACE -------------------------------------------------------
        let size_ms = if space_on {
            5.0 + self.params.ffdn.size.value() as f64 * 195.0
        } else {
            50.0
        };
        if size_ms != self.last_ffdn[1] {
            ctrl.set_ffdn_size_ms(size_ms);
            self.last_ffdn[1] = size_ms;
        }

        let decay_sec = if space_on {
            0.1 + self.params.ffdn.decay.value() as f64 * 59.9
        } else {
            3.0
        };
        if decay_sec != self.last_ffdn[2] {
            ctrl.set_ffdn_decay_sec(decay_sec);
            self.last_ffdn[2] = decay_sec;
        }

        let spread = if space_on {
            self.params.ffdn.spread.value() as f64
        } else {
            0.5
        };
        if spread != self.last_ffdn[4] {
            ctrl.set_ffdn_spread(spread);
            self.last_ffdn[4] = spread;
        }

        let diffusion = if space_on {
            self.params.ffdn.diffusion.value() as f64
        } else {
            0.0
        };
        if diffusion != self.last_ffdn[12] {
            ctrl.set_ffdn_diffusion(diffusion);
            self.last_ffdn[12] = diffusion;
        }

        // ---- MODULATION (damping, modAmt, modRate) ----------------------
        let damping_hz = if modulation_on {
            500.0 + self.params.ffdn.damping.value() as f64 * 19500.0
        } else {
            8000.0
        };
        if damping_hz != self.last_ffdn[3] {
            ctrl.set_ffdn_damping_hz(damping_hz);
            self.last_ffdn[3] = damping_hz;
        }

        let mod_amount_ms = if modulation_on {
            self.params.ffdn.mod_amount.value() as f64 * 3.0
        } else {
            0.0
        };
        if mod_amount_ms != self.last_ffdn[5] {
            ctrl.set_ffdn_mod_amount_ms(mod_amount_ms);
            self.last_ffdn[5] = mod_amount_ms;
        }

        let mod_rate_hz = if modulation_on {
            0.1 + self.params.ffdn.mod_rate.value() as f64 * 4.9
        } else {
            0.7
        };
        if mod_rate_hz != self.last_ffdn[6] {
            ctrl.set_ffdn_mod_rate_hz(mod_rate_hz);
            self.last_ffdn[6] = mod_rate_hz;
        }

        // ---- NETWORK (matrix type) --------------------------------------
        let is_velvet = if network_on {
            self.params.ffdn.matrix_type.value() >= 0.5
        } else {
            false
        };
        let matrix_val = if is_velvet { 1.0 } else { 0.0 };
        if matrix_val != self.last_ffdn[7] {
            ctrl.set_ffdn_matrix_type(is_velvet);
            self.last_ffdn[7] = matrix_val;
        }

        // ---- SHELVES (frequency-dependent attenuation) ------------------
        let (bass_mult, treble_mult, bass_freq, treble_freq) = if shelves_on {
            (
                2.0_f64.powf((self.params.ffdn.bass_mult.value() as f64 - 0.5) * 2.0),
                2.0_f64.powf((self.params.ffdn.treble_mult.value() as f64 - 0.5) * 2.0),
                50.0 * 20.0_f64.powf(self.params.ffdn.bass_freq.value() as f64),
                1000.0 * 15.0_f64.powf(self.params.ffdn.treble_freq.value() as f64),
            )
        } else {
            (1.0, 1.0, 250.0, 4000.0)
        };

        let fda_changed = bass_mult != self.last_ffdn[8]
            || treble_mult != self.last_ffdn[9]
            || bass_freq != self.last_ffdn[10]
            || treble_freq != self.last_ffdn[11];

        if fda_changed {
            ctrl.set_ffdn_fda(bass_mult, treble_mult, bass_freq, treble_freq);
            self.last_ffdn[8] = bass_mult;
            self.last_ffdn[9] = treble_mult;
            self.last_ffdn[10] = bass_freq;
            self.last_ffdn[11] = treble_freq;
        }

        // ---- CHARACTER (cross, drive, tilt, ER) -------------------------
        let (cross, drive, tilt_freq_hz, tilt_amount, er_level, drive_on, tilt_on, er_on) =
            if character_on {
                (
                    self.params.ffdn.cross.value() as f64,
                    self.params.ffdn.drive.value() as f64,
                    100.0 * 100.0_f64.powf(self.params.ffdn.tilt_freq.value() as f64),
                    self.params.ffdn.tilt_amount.value() as f64,
                    self.params.ffdn.er_level.value() as f64,
                    self.params.ffdn.drive_enabled.value(),
                    self.params.ffdn.tilt_enabled.value(),
                    // ER is always on when CHARACTER is on. Its level
                    // is controlled by the MIXER's ER fader; the old
                    // per-stage switch was removed.
                    true,
                )
            } else {
                (0.0, 0.0, 1000.0, 0.5, 0.0, false, false, false)
            };

        if cross != self.last_ffdn[13] {
            ctrl.set_ffdn_cross_feed(cross);
            self.last_ffdn[13] = cross;
        }
        if drive != self.last_ffdn[14] {
            ctrl.set_ffdn_drive(drive);
            self.last_ffdn[14] = drive;
        }
        if tilt_freq_hz != self.last_ffdn[15] || tilt_amount != self.last_ffdn[16] {
            ctrl.set_ffdn_tilt(tilt_freq_hz, tilt_amount);
            self.last_ffdn[15] = tilt_freq_hz;
            self.last_ffdn[16] = tilt_amount;
        }
        if er_level != self.last_ffdn[17] {
            ctrl.set_ffdn_er_level(er_level);
            self.last_ffdn[17] = er_level;
        }

        let drive_val = if drive_on { 1.0 } else { 0.0 };
        if drive_val != self.last_ffdn[18] {
            ctrl.set_ffdn_drive_enabled(drive_on);
            self.last_ffdn[18] = drive_val;
        }
        let tilt_val = if tilt_on { 1.0 } else { 0.0 };
        if tilt_val != self.last_ffdn[19] {
            ctrl.set_ffdn_tilt_enabled(tilt_on);
            self.last_ffdn[19] = tilt_val;
        }
        let er_val = if er_on { 1.0 } else { 0.0 };
        if er_val != self.last_ffdn[20] {
            ctrl.set_ffdn_er_enabled(er_on);
            self.last_ffdn[20] = er_val;
        }

        // ---- NETWORK (lines, density, decorr) ---------------------------
        let lines = if network_on {
            (4.0 + (self.params.ffdn.lines.value() as f64 * 12.0).round()).clamp(4.0, 16.0)
        } else {
            8.0
        };
        if lines != self.last_ffdn[21] {
            ctrl.set_ffdn_lines(lines as usize);
            self.last_ffdn[21] = lines;
        }

        let density = if network_on {
            self.params.ffdn.density.value() as f64
        } else {
            0.5
        };
        if density != self.last_ffdn[22] {
            ctrl.set_ffdn_density(density);
            self.last_ffdn[22] = density;
        }

        let decorr = if network_on {
            self.params.ffdn.decorr.value() as f64
        } else {
            0.5
        };
        if decorr != self.last_ffdn[23] {
            ctrl.set_ffdn_decorr(decorr);
            self.last_ffdn[23] = decorr;
        }

        // ---- FILTER (LoopHP, LoopLP, MidFocus) --------------------------
        let loop_hp_hz = if filter_on {
            20.0 * (500.0_f64 / 20.0).powf(self.params.ffdn.loop_hp.value() as f64)
        } else {
            20.0
        };
        if loop_hp_hz != self.last_ffdn[24] {
            ctrl.set_ffdn_loop_hp(loop_hp_hz);
            self.last_ffdn[24] = loop_hp_hz;
        }

        let loop_lp_hz = if filter_on {
            2000.0 * (20000.0_f64 / 2000.0).powf(self.params.ffdn.loop_lp.value() as f64)
        } else {
            20000.0
        };
        if loop_lp_hz != self.last_ffdn[25] {
            ctrl.set_ffdn_loop_lp(loop_lp_hz);
            self.last_ffdn[25] = loop_lp_hz;
        }

        let mid_focus = if filter_on {
            self.params.ffdn.mid_focus.value() as f64
        } else {
            0.0
        };
        if mid_focus != self.last_ffdn[26] {
            ctrl.set_ffdn_mid_focus(mid_focus);
            self.last_ffdn[26] = mid_focus;
        }

        // ---- DYNAMICS (Attack, Ducking, Gate) ---------------------------
        let attack = if dynamics_on {
            self.params.ffdn.attack.value() as f64
        } else {
            0.0
        };
        if attack != self.last_ffdn[27] {
            ctrl.set_ffdn_attack(attack);
            self.last_ffdn[27] = attack;
        }

        let ducking = if dynamics_on {
            self.params.ffdn.ducking.value() as f64
        } else {
            0.0
        };
        if ducking != self.last_ffdn[28] {
            ctrl.set_ffdn_ducking(ducking);
            self.last_ffdn[28] = ducking;
        }

        let gate = if dynamics_on {
            self.params.ffdn.gate.value() as f64
        } else {
            0.0
        };
        if gate != self.last_ffdn[29] {
            ctrl.set_ffdn_gate(gate);
            self.last_ffdn[29] = gate;
        }

        // ---- process in blocks -----------------------------------------
        let mut total_samples = 0usize;
        for (_offset, block) in buffer.iter_blocks(4096) {
            let mut channels = block.into_iter();
            match (channels.next(), channels.next()) {
                (Some(left), Some(right)) => {
                    total_samples = total_samples.saturating_add(left.len());
                    ctrl.process_block_inplace(left, right);
                }
                (Some(left), None) => {
                    let n = left.len().min(self.mono_scratch.len());
                    total_samples = total_samples.saturating_add(n);
                    self.mono_scratch[..n].copy_from_slice(&left[..n]);
                    ctrl.process_mono_inplace(&mut left[..n]);
                }
                _ => {}
            }
        }

        let mut out_peak = 0.0f32;
        for ch in buffer.as_slice_immutable() {
            for s in ch.iter() {
                let a = s.abs();
                if a > out_peak {
                    out_peak = a;
                }
            }
        }
        let hold = (IDLE_HOLD_S * self.sample_rate) as usize;
        if self
            .idle
            .sleep_check(in_peak, out_peak, total_samples, hold)
        {
            // Entering sleep for the first time this silent stretch.
            // Clear the DSP engine's internal buffers so that when
            // the plugin wakes up, it starts from a pristine state
            // rather than with a half-decayed tail that would
            // otherwise be mixed into the first samples of new audio.
            //
            // This is safe from a click standpoint: to reach this
            // branch, both input and output peaks must have been
            // below `SLEEP_LEVEL` (-100 dBFS) for `IDLE_HOLD_S`
            // seconds, which means the tail is already completely
            // inaudible. Zeroing it cannot produce an audible step.
            ctrl.clear_buffers();
        }

        ProcessStatus::Normal
    }
}

// ============================================================================
// Format impls
// ============================================================================

impl Vst3Plugin for PejvakRvbPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"PejvakDspRVB0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Fx,
        Vst3SubCategory::Reverb,
        Vst3SubCategory::Spatial,
    ];
}

impl ClapPlugin for PejvakRvbPlugin {
    const CLAP_ID: &'static str = "com.pejvakdsp.pejvak-rvb";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("PEJVAK RVB – Hybrid Algorithmic Reverb");
    const CLAP_MANUAL_URL: Option<&'static str> = Some("https://github.com/Pejvak-DSP");
    const CLAP_SUPPORT_URL: Option<&'static str> = Some("https://github.com/Pejvak-DSP");
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Reverb,
        ClapFeature::Stereo,
    ];
}

nice_export_vst3!(PejvakRvbPlugin);
nice_export_clap!(PejvakRvbPlugin);
