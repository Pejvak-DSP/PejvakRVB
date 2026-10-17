//! Parameter definitions – GPL-3.0
//!
//! Faithful Rust port of `CloudSeed.Native/Parameter.h` + the
//! `GetScaledParameter` switch inside `ReverbController.h`. Every mapping
//! matches the C++ formula exactly, so the values the DSP sees are
//! bit-identical to what the original plugin would compute.

use super::allpass_diffuser::MAX_STAGE_COUNT;
use super::multitap_diffuser::MAX_TAPS;
use super::value_tables;

/// Total number of parameters (matches `Parameter::Count`).
pub const PARAM_COUNT: usize = 49;

/// The canonical CloudReverb legacy-v1 parameter enum.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(usize)]
pub enum Parameter {
    // Input
    InputMix = 0,
    PreDelay,
    HighPass,
    LowPass,
    // Early
    TapEnabled,
    TapCount,
    TapLength,
    TapGain,
    TapDecay,
    DiffusionEnabled,
    DiffusionStages,
    DiffusionDelay,
    DiffusionFeedback,
    // Late
    LateDelayEnabled,
    LineCount,
    LineDelay,
    LineDecay,
    LateDiffusionEnabled,
    LateDiffusionStages,
    LateDiffusionDelay,
    LateDiffusionFeedback,
    // Frequency Response
    PostLowShelfGain,
    PostLowShelfFrequency,
    PostHighShelfGain,
    PostHighShelfFrequency,
    PostCutoffFrequency,
    // Modulation
    EarlyDiffusionModAmount,
    EarlyDiffusionModRate,
    LineModAmount,
    LineModRate,
    LateDiffusionModAmount,
    LateDiffusionModRate,
    // Seeds
    TapSeed,
    DiffusionSeed,
    DelaySeed,
    PostDiffusionSeed,
    // Seed Mix
    CrossSeed,
    // Output
    DryOut,
    PredelayOut,
    EarlyOut,
    MainOut,
    WidthOut,
    // Switches
    HiPassEnabled,
    LowPassEnabled,
    LowShelfEnabled,
    HighShelfEnabled,
    CutoffEnabled,
    LateStageTap,
    // Effects
    Interpolation,
}

impl Parameter {
    pub fn index(&self) -> usize {
        *self as usize
    }

    /// The CamelCase name used by CloudReverb presets (XML/JSON).
    pub fn name(&self) -> &'static str {
        match self {
            Parameter::InputMix => "InputMix",
            Parameter::PreDelay => "PreDelay",
            Parameter::HighPass => "HighPass",
            Parameter::LowPass => "LowPass",
            Parameter::TapEnabled => "TapEnabled",
            Parameter::TapCount => "TapCount",
            Parameter::TapLength => "TapLength",
            Parameter::TapGain => "TapGain",
            Parameter::TapDecay => "TapDecay",
            Parameter::DiffusionEnabled => "DiffusionEnabled",
            Parameter::DiffusionStages => "DiffusionStages",
            Parameter::DiffusionDelay => "DiffusionDelay",
            Parameter::DiffusionFeedback => "DiffusionFeedback",
            Parameter::LateDelayEnabled => "LateDelayEnabled",
            Parameter::LineCount => "LineCount",
            Parameter::LineDelay => "LineDelay",
            Parameter::LineDecay => "LineDecay",
            Parameter::LateDiffusionEnabled => "LateDiffusionEnabled",
            Parameter::LateDiffusionStages => "LateDiffusionStages",
            Parameter::LateDiffusionDelay => "LateDiffusionDelay",
            Parameter::LateDiffusionFeedback => "LateDiffusionFeedback",
            Parameter::PostLowShelfGain => "PostLowShelfGain",
            Parameter::PostLowShelfFrequency => "PostLowShelfFrequency",
            Parameter::PostHighShelfGain => "PostHighShelfGain",
            Parameter::PostHighShelfFrequency => "PostHighShelfFrequency",
            Parameter::PostCutoffFrequency => "PostCutoffFrequency",
            Parameter::EarlyDiffusionModAmount => "EarlyDiffusionModAmount",
            Parameter::EarlyDiffusionModRate => "EarlyDiffusionModRate",
            Parameter::LineModAmount => "LineModAmount",
            Parameter::LineModRate => "LineModRate",
            Parameter::LateDiffusionModAmount => "LateDiffusionModAmount",
            Parameter::LateDiffusionModRate => "LateDiffusionModRate",
            Parameter::TapSeed => "TapSeed",
            Parameter::DiffusionSeed => "DiffusionSeed",
            Parameter::DelaySeed => "DelaySeed",
            Parameter::PostDiffusionSeed => "PostDiffusionSeed",
            Parameter::CrossSeed => "CrossSeed",
            Parameter::DryOut => "DryOut",
            Parameter::PredelayOut => "PredelayOut",
            Parameter::EarlyOut => "EarlyOut",
            Parameter::MainOut => "MainOut",
            Parameter::WidthOut => "Width",
            Parameter::HiPassEnabled => "HiPassEnabled",
            Parameter::LowPassEnabled => "LowPassEnabled",
            Parameter::LowShelfEnabled => "LowShelfEnabled",
            Parameter::HighShelfEnabled => "HighShelfEnabled",
            Parameter::CutoffEnabled => "CutoffEnabled",
            Parameter::LateStageTap => "LateStageTap",
            Parameter::Interpolation => "Interpolation",
        }
    }

    pub fn from_name(name: &str) -> Option<Parameter> {
        (0..PARAM_COUNT)
            .find(|i| {
                Parameter::from_index(*i).map_or(false, |p| p.name().eq_ignore_ascii_case(name))
            })
            .and_then(Parameter::from_index)
    }

    pub fn from_index(index: usize) -> Option<Parameter> {
        let p = match index {
            0 => Parameter::InputMix,
            1 => Parameter::PreDelay,
            2 => Parameter::HighPass,
            3 => Parameter::LowPass,
            4 => Parameter::TapEnabled,
            5 => Parameter::TapCount,
            6 => Parameter::TapLength,
            7 => Parameter::TapGain,
            8 => Parameter::TapDecay,
            9 => Parameter::DiffusionEnabled,
            10 => Parameter::DiffusionStages,
            11 => Parameter::DiffusionDelay,
            12 => Parameter::DiffusionFeedback,
            13 => Parameter::LateDelayEnabled,
            14 => Parameter::LineCount,
            15 => Parameter::LineDelay,
            16 => Parameter::LineDecay,
            17 => Parameter::LateDiffusionEnabled,
            18 => Parameter::LateDiffusionStages,
            19 => Parameter::LateDiffusionDelay,
            20 => Parameter::LateDiffusionFeedback,
            21 => Parameter::PostLowShelfGain,
            22 => Parameter::PostLowShelfFrequency,
            23 => Parameter::PostHighShelfGain,
            24 => Parameter::PostHighShelfFrequency,
            25 => Parameter::PostCutoffFrequency,
            26 => Parameter::EarlyDiffusionModAmount,
            27 => Parameter::EarlyDiffusionModRate,
            28 => Parameter::LineModAmount,
            29 => Parameter::LineModRate,
            30 => Parameter::LateDiffusionModAmount,
            31 => Parameter::LateDiffusionModRate,
            32 => Parameter::TapSeed,
            33 => Parameter::DiffusionSeed,
            34 => Parameter::DelaySeed,
            35 => Parameter::PostDiffusionSeed,
            36 => Parameter::CrossSeed,
            37 => Parameter::DryOut,
            38 => Parameter::PredelayOut,
            39 => Parameter::EarlyOut,
            40 => Parameter::MainOut,
            41 => Parameter::WidthOut,
            42 => Parameter::HiPassEnabled,
            43 => Parameter::LowPassEnabled,
            44 => Parameter::LowShelfEnabled,
            45 => Parameter::HighShelfEnabled,
            46 => Parameter::CutoffEnabled,
            47 => Parameter::LateStageTap,
            48 => Parameter::Interpolation,
            _ => return None,
        };
        Some(p)
    }

    pub fn all() -> impl Iterator<Item = Parameter> {
        (0..PARAM_COUNT).filter_map(Parameter::from_index)
    }
}

/// Factory defaults — ChorusDelay preset (the C++ plugin's startup patch),
/// with two deliberate overrides:
///
///   * CrossSeed = 0.5 (instead of 0.0): makes L/R decorrelated so the
///     reverb is genuinely stereo out of the box.
///
///   * WidthOut = 0.5 (instead of 1.0): the `scaled_parameter_value`
///     mapping for index 41 multiplies by 2.0, so 0.5 → 1.0 which is
///     the correct "no change" position for the M/S width transform.
///     A value of 0.75 would map to 1.5 (150 %), etc.
pub const DEFAULT_PARAMS: [f64; PARAM_COUNT] = [
    0.0,               // 0  InputMix
    0.070_000_000_298, // 1  PreDelay       (~70 ms)
    0.0,               // 2  HighPass
    0.290_000_081_063, // 3  LowPass
    1.0,               // 4  TapEnabled
    0.346_938_775_510, // 5  TapCount
    1.0,               // 6  TapLength
    1.0,               // 7  TapGain
    0.865_000_128_746, // 8  TapDecay
    1.0,               // 9  DiffusionEnabled
    0.428_571_432_829, // 10 DiffusionStages
    0.435_000_061_989, // 11 DiffusionDelay
    0.725_000_262_260, // 12 DiffusionFeedback
    1.0,               // 13 LateDelayEnabled
    1.0,               // 14 LineCount
    0.684_854,         // 15 LineDelay
    0.680_000_126_362, // 16 LineDecay
    1.0,               // 17 LateDiffusionEnabled
    0.285_714_298_487, // 18 LateDiffusionStages
    0.544_999_957_085, // 19 LateDiffusionDelay
    0.659_999_966_621, // 20 LateDiffusionFeedback
    0.519_999_921_322, // 21 PostLowShelfGain
    0.314_999_908_209, // 22 PostLowShelfFrequency
    0.835_000_038_147, // 23 PostHighShelfGain
    0.730_000_138_283, // 24 PostHighShelfFrequency
    0.734_999_835_491, // 25 PostCutoffFrequency
    0.500_000_059_605, // 26 EarlyDiffusionModAmount
    0.425_000_101_328, // 27 EarlyDiffusionModRate
    0.590_000_033_379, // 28 LineModAmount
    0.469_999_939_203, // 29 LineModRate
    0.619_999_885_559, // 30 LateDiffusionModAmount
    0.425_000_190_734, // 31 LateDiffusionModRate
    0.001_149,         // 32 TapSeed
    0.000_188,         // 33 DiffusionSeed
    0.000_336,         // 34 DelaySeed
    0.000_5,           // 35 PostDiffusionSeed
    0.5,               // 36 CrossSeed   ← changed for stereo
    0.901_233_923_360, // 37 DryOut
    0.0,               // 38 PredelayOut
    0.624_261_051_128, // 39 EarlyOut
    0.570_361_946_138, // 40 MainOut
    0.5,               // 41 WidthOut    ← 0.5 × 2.0 = 100 %
    0.0,               // 42 HiPassEnabled
    0.0,               // 43 LowPassEnabled
    0.0,               // 44 LowShelfEnabled
    0.0,               // 45 HighShelfEnabled
    1.0,               // 46 CutoffEnabled
    1.0,               // 47 LateStageTap
    0.0,               // 48 Interpolation
];

/// Pure form of `ReverbController::GetScaledParameter`: converts a
/// normalised value `p` for parameter `index` to its physical value,
/// using the original `ValueTables` response curves.
pub fn scaled_parameter_value(index: usize, p: f64) -> f64 {
    match index {
        // ---- Input -----------------------------------------------------
        0 => p,                                              // InputMix (stereo cross-blend)
        1 => (p * 1000.0) as i64 as f64,                     // PreDelay [ms]
        2 => 20.0 + value_tables::response_4_oct(p) * 980.0, // HighPass [Hz]
        3 => 400.0 + value_tables::response_4_oct(p) * 19_600.0, // LowPass [Hz]

        // ---- Early / Multitap ------------------------------------------
        4 => {
            if p < 0.5 {
                0.0
            } else {
                1.0
            }
        } // TapEnabled
        5 => 1.0 + (p * (MAX_TAPS as f64 - 1.0)).round(), // TapCount [1..=50]
        6 => (p * 500.0) as i64 as f64,                   // TapLength [ms]
        7 => value_tables::response_2_dec(p),             // TapGain
        8 => p,                                           // TapDecay

        9 => {
            if p < 0.5 {
                0.0
            } else {
                1.0
            }
        } // DiffusionEnabled
        10 => 1.0 + (p * (MAX_STAGE_COUNT as f64 - 1.0)).round(), // DiffusionStages [1..=8]
        11 => (10.0 + p * 90.0) as i64 as f64,                    // DiffusionDelay [ms]
        12 => p,                                                  // DiffusionFeedback

        // ---- Late -------------------------------------------------------
        13 => {
            if p < 0.5 {
                0.0
            } else {
                1.0
            }
        } // LateDelayEnabled
        14 => 1.0 + (p * 11.0).round(), // LineCount [1..=12]
        15 => (20.0 + value_tables::response_2_dec(p) * 980.0) as i64 as f64, // LineDelay [ms]
        16 => 0.05 + value_tables::response_3_dec(p) * 59.95, // LineDecay [s]

        17 => {
            if p < 0.5 {
                0.0
            } else {
                1.0
            }
        } // LateDiffusionEnabled
        18 => 1.0 + (p * (MAX_STAGE_COUNT as f64 - 1.0)).round(), // LateDiffusionStages [1..=8]
        19 => (10.0 + p * 90.0) as i64 as f64,                    // LateDiffusionDelay [ms]
        20 => p,                                                  // LateDiffusionFeedback

        // ---- Frequency Response -----------------------------------------
        21 => value_tables::response_2_dec(p), // PostLowShelfGain
        22 => 20.0 + value_tables::response_4_oct(p) * 980.0, // PostLowShelfFrequency [Hz]
        23 => value_tables::response_2_dec(p), // PostHighShelfGain
        24 => 400.0 + value_tables::response_4_oct(p) * 19_600.0, // PostHighShelfFrequency [Hz]
        25 => 400.0 + value_tables::response_4_oct(p) * 19_600.0, // PostCutoffFrequency [Hz]

        // ---- Modulation -------------------------------------------------
        26 => p * 2.5,                               // EarlyDiffusionModAmount [ms]
        27 => value_tables::response_2_dec(p) * 5.0, // EarlyDiffusionModRate [Hz]
        28 => p * 2.5,                               // LineModAmount [ms]
        29 => value_tables::response_2_dec(p) * 5.0, // LineModRate [Hz]
        30 => p * 2.5,                               // LateDiffusionModAmount [ms]
        31 => value_tables::response_2_dec(p) * 5.0, // LateDiffusionModRate [Hz]

        // ---- Seeds ------------------------------------------------------
        32 => (p * 1_000_000.0 + 0.001).floor(), // TapSeed
        33 => (p * 1_000_000.0 + 0.001).floor(), // DiffusionSeed
        34 => (p * 1_000_000.0 + 0.001).floor(), // DelaySeed
        35 => (p * 1_000_000.0 + 0.001).floor(), // PostDiffusionSeed

        // ---- Seed Mix / Output ------------------------------------------
        36 => p,                               // CrossSeed
        37 => value_tables::response_2_dec(p), // DryOut
        38 => value_tables::response_2_dec(p), // PredelayOut
        39 => value_tables::response_2_dec(p), // EarlyOut
        40 => value_tables::response_2_dec(p), // MainOut
        // WidthOut: 0..1 → 0..2. The user-facing convention is
        //     0.5 → 1.0 (100 %, unchanged stereo image)
        //     0.0 → 0.0 (fully mono)
        //     1.0 → 2.0 (200 % enhanced width)
        // The XML preset format stores 0.5 for "normal" width, so this
        // mapping keeps CloudReverb preset compatibility intact.
        41 => p * 2.0, // WidthOut

        // ---- Switches ---------------------------------------------------
        42 => {
            if p < 0.5 {
                0.0
            } else {
                1.0
            }
        } // HiPassEnabled
        43 => {
            if p < 0.5 {
                0.0
            } else {
                1.0
            }
        } // LowPassEnabled
        44 => {
            if p < 0.5 {
                0.0
            } else {
                1.0
            }
        } // LowShelfEnabled
        45 => {
            if p < 0.5 {
                0.0
            } else {
                1.0
            }
        } // HighShelfEnabled
        46 => {
            if p < 0.5 {
                0.0
            } else {
                1.0
            }
        } // CutoffEnabled
        47 => {
            if p < 0.5 {
                0.0
            } else {
                1.0
            }
        } // LateStageTap

        // ---- Effects ----------------------------------------------------
        48 => {
            if p < 0.5 {
                0.0
            } else {
                1.0
            }
        } // Interpolation

        _ => 0.0,
    }
}
