//! PEJVAK RVB – Factory Preset Library – GPL-3.0
//!
//! 100 presets across 16 categories, covering both the CloudSeed
//! (legacy CloudReverb) and FFDN engines, plus creative Illusion /
//! Shimmer presets that drive the Texture Rack.
//!
//! Categories 0–7 are CloudSeed. Categories 8–13 are FFDN. Categories
//! 14–15 are creative hybrids built on the Texture Rack + Illusion.

use egui::Color32;

// ============================================================================
// Categories
// ============================================================================

pub struct PresetCategory {
    pub name: &'static str,
    pub icon: &'static str,
    pub color: Color32,
    pub description: &'static str,
}

pub const CATEGORIES: [PresetCategory; 16] = [
    PresetCategory {
        name: "Ambience",
        icon: "A",
        color: Color32::from_rgb(0x96, 0xB8, 0xC9),
        description: "RT60 0.3–0.7 s · felt, not heard",
    },
    PresetCategory {
        name: "Rooms",
        icon: "R",
        color: Color32::from_rgb(0xA8, 0xB5, 0xA0),
        description: "RT60 0.7–1.5 s · tight real spaces",
    },
    PresetCategory {
        name: "Chambers",
        icon: "C",
        color: Color32::from_rgb(0xD4, 0xB8, 0x96),
        description: "RT60 1.2–2.0 s · classic echo chambers",
    },
    PresetCategory {
        name: "Plates",
        icon: "P",
        color: Color32::from_rgb(0xC9, 0xB8, 0xD4),
        description: "RT60 1.6–3.0 s · bright, dense, no room",
    },
    PresetCategory {
        name: "Halls",
        icon: "H",
        color: Color32::from_rgb(0x8B, 0x9D, 0xC3),
        description: "RT60 2.5–4.0 s · concert halls",
    },
    PresetCategory {
        name: "Cathedral",
        icon: "X",
        color: Color32::from_rgb(0xB8, 0xA8, 0xD4),
        description: "RT60 4.5–7.0 s · sacred stone spaces",
    },
    PresetCategory {
        name: "Textures",
        icon: "T",
        color: Color32::from_rgb(0xD4, 0xA0, 0xB8),
        description: "CloudSeed + Texture Rack · creative",
    },
    PresetCategory {
        name: "Extreme",
        icon: "E",
        color: Color32::from_rgb(0xD4, 0x96, 0x7A),
        description: "RT60 12–25 s · non-physical",
    },
    PresetCategory {
        name: "FFDN Spaces",
        icon: "S",
        color: Color32::from_rgb(0x86, 0xC8, 0xA8),
        description: "Filtered FDN · rooms & small spaces",
    },
    PresetCategory {
        name: "FFDN Halls",
        icon: "F",
        color: Color32::from_rgb(0x7D, 0xA8, 0xD4),
        description: "Filtered FDN · concert & symphonic",
    },
    PresetCategory {
        name: "FFDN Plates",
        icon: "V",
        color: Color32::from_rgb(0xC0, 0x88, 0xC8),
        description: "Filtered FDN · plate & metallic",
    },
    PresetCategory {
        name: "FFDN Ambient",
        icon: "N",
        color: Color32::from_rgb(0x9E, 0x7A, 0xE0),
        description: "Filtered FDN · heavy modulation",
    },
    PresetCategory {
        name: "FFDN Character",
        icon: "K",
        color: Color32::from_rgb(0xE0, 0xA0, 0x70),
        description: "Filtered FDN · saturation + tilt",
    },
    PresetCategory {
        name: "FFDN Dynamic",
        icon: "Y",
        color: Color32::from_rgb(0xE0, 0x80, 0x80),
        description: "Filtered FDN · attack, ducking, gate",
    },
    PresetCategory {
        name: "Illusion",
        icon: "I",
        color: Color32::from_rgb(0xE6, 0xF2, 0xFF),
        description: "Granular cloud + pitched overtones",
    },
    PresetCategory {
        name: "Shimmer",
        icon: "M",
        color: Color32::from_rgb(0xFF, 0xD5, 0x4F),
        description: "Pitch-shifted octave worlds",
    },
];

// ============================================================================
// Engine
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FactoryEngine {
    CloudSeed,
    Ffdn,
}

// ============================================================================
// Texture Rack configuration
// ============================================================================

#[derive(Clone, Copy)]
pub struct TextureConfig {
    pub shimmer_mix: f32,
    pub mist_mix: f32,
    pub drift_mix: f32,
    pub crystal_mix: f32,
    pub scatter_mix: f32,
    pub bloom_mix: f32,
}

impl TextureConfig {
    pub const NONE: TextureConfig = TextureConfig {
        shimmer_mix: 0.0,
        mist_mix: 0.0,
        drift_mix: 0.0,
        crystal_mix: 0.0,
        scatter_mix: 0.0,
        bloom_mix: 0.0,
    };

    pub const fn single(
        shimmer: f32,
        mist: f32,
        drift: f32,
        crystal: f32,
        scatter: f32,
        bloom: f32,
    ) -> Self {
        TextureConfig {
            shimmer_mix: shimmer,
            mist_mix: mist,
            drift_mix: drift,
            crystal_mix: crystal,
            scatter_mix: scatter,
            bloom_mix: bloom,
        }
    }
}

// ============================================================================
// Illusion (ILLUSION) configuration
// ============================================================================

#[derive(Clone, Copy)]
pub struct IllusionConfig {
    pub spark: f32,
    pub still: f32,
    pub aura_norm: f32, // 0..1 → -24..+24 st
    pub veil: f32,
    pub wide: f32,
    pub tint: f32,
    pub rate: f32,
    pub grain: f32,
}

impl IllusionConfig {
    pub const OFF: IllusionConfig = IllusionConfig {
        spark: 0.0,
        still: 0.5,
        aura_norm: 0.5,
        veil: 0.4,
        wide: 0.5,
        tint: 0.5,
        rate: 0.5,
        grain: 0.4,
    };
}

// ============================================================================
// FFDN configuration
// ============================================================================

/// Full FFDN parameter set. Values are normalised 0..1 in the same
/// ranges the plugin exposes in `FfdnParams`; the physical mapping
/// happens in `lib.rs::process_inner`.
#[derive(Clone, Copy)]
pub struct FfdnConfig {
    pub size: f32,
    pub decay: f32,
    pub damping: f32,
    pub spread: f32,
    pub mod_amount: f32,
    pub mod_rate: f32,
    pub velvet_matrix: bool,
    pub bass_mult: f32,
    pub bass_freq: f32,
    pub treble_mult: f32,
    pub treble_freq: f32,
    pub diffusion: f32,
    pub cross: f32,
    pub drive: f32,
    pub tilt_freq: f32,
    pub tilt_amount: f32,
    pub er_level: f32,
    pub drive_enabled: bool,
    pub tilt_enabled: bool,
    pub lines: u8,
    pub density: f32,
    pub decorr: f32,
    pub loop_hp: f32,
    pub loop_lp: f32,
    pub mid_focus: f32,
    pub attack: f32,
    pub ducking: f32,
    pub gate: f32,
}

impl Default for FfdnConfig {
    fn default() -> Self {
        Self {
            size: 0.23,
            decay: 0.05,
            damping: 0.385,
            spread: 0.5,
            mod_amount: 0.167,
            mod_rate: 0.122,
            velvet_matrix: false,
            bass_mult: 0.5,
            bass_freq: 0.537,
            treble_mult: 0.5,
            treble_freq: 0.512,
            diffusion: 0.5,
            cross: 0.0,
            drive: 0.0,
            tilt_freq: 0.5,
            tilt_amount: 0.5,
            er_level: 0.0,
            drive_enabled: false,
            tilt_enabled: false,
            lines: 8,
            density: 0.5,
            decorr: 0.5,
            loop_hp: 0.0,
            loop_lp: 1.0,
            mid_focus: 0.0,
            attack: 0.0,
            ducking: 0.0,
            gate: 0.0,
        }
    }
}

// ============================================================================
// Preset
// ============================================================================

pub struct FactoryPreset {
    pub name: &'static str,
    pub category: usize,
    pub engine: FactoryEngine,
    /// CloudSeed 49-param array. Ignored when `engine == Ffdn`.
    pub values: [f32; 49],
    /// FFDN parameters. Ignored when `engine == CloudSeed`.
    pub ffdn: FfdnConfig,
    pub texture: TextureConfig,
    /// Illusion (ILLUSION) configuration. `spark == 0.0` means disabled.
    pub illusion: IllusionConfig,
}

// ----------------------------------------------------------------------------
// Helper: build a CloudSeed preset with common defaults
// ----------------------------------------------------------------------------

const NO_FFDN: FfdnConfig = FfdnConfig {
    size: 0.23,
    decay: 0.05,
    damping: 0.385,
    spread: 0.5,
    mod_amount: 0.167,
    mod_rate: 0.122,
    velvet_matrix: false,
    bass_mult: 0.5,
    bass_freq: 0.537,
    treble_mult: 0.5,
    treble_freq: 0.512,
    diffusion: 0.5,
    cross: 0.0,
    drive: 0.0,
    tilt_freq: 0.5,
    tilt_amount: 0.5,
    er_level: 0.0,
    drive_enabled: false,
    tilt_enabled: false,
    lines: 8,
    density: 0.5,
    decorr: 0.5,
    loop_hp: 0.0,
    loop_lp: 1.0,
    mid_focus: 0.0,
    attack: 0.0,
    ducking: 0.0,
    gate: 0.0,
};

const NO_CLOUDSEED: [f32; 49] = [0.0; 49];

// ============================================================================
// FACTORY PRESETS
// ============================================================================

pub const FACTORY_PRESETS: &[FactoryPreset] = &[
    // ======================================================================
    // INIT
    // ======================================================================
    FactoryPreset {
        name: "Init",
        category: 0,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.00, 0.010, 0.10, 0.60, 1.0, 0.30, 0.30, 0.95, 0.85, 1.0, 0.5714, 0.30, 0.65, 1.0,
            0.5833, 0.50, 0.41, 1.0, 0.5714, 0.44, 0.65, 0.80, 0.30, 0.85, 0.70, 0.75, 0.30, 0.42,
            0.55, 0.42, 0.62, 0.42, 0.001149, 0.000188, 0.000336, 0.000500, 0.50, 0.90, 0.00, 0.72,
            0.78, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // AMBIENCE (0) — CloudSeed — 6 presets
    // ======================================================================
    FactoryPreset {
        name: "Vocal Presence",
        category: 0,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.00, 0.005, 0.12, 0.72, 0.0, 0.20, 0.20, 0.72, 1.0, 1.0, 0.4286, 0.20, 0.45, 1.0,
            0.4167, 0.35, 0.29, 1.0, 0.4286, 0.30, 0.48, 0.78, 0.42, 0.90, 0.72, 0.52, 0.12, 0.20,
            0.28, 0.16, 0.40, 0.20, 0.000550, 0.000320, 0.000420, 0.000680, 0.50, 0.88, 0.00, 0.62,
            0.72, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Tape Room Glue",
        category: 0,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.00, 0.003, 0.10, 0.55, 0.0, 0.18, 0.20, 0.68, 1.0, 1.0, 0.4286, 0.18, 0.42, 1.0,
            0.4167, 0.32, 0.31, 1.0, 0.4286, 0.28, 0.50, 0.72, 0.46, 0.92, 0.76, 0.48, 0.10, 0.18,
            0.26, 0.15, 0.38, 0.20, 0.000600, 0.000350, 0.000450, 0.000720, 0.50, 0.90, 0.00, 0.58,
            0.68, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "String Bed",
        category: 0,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.05, 0.008, 0.12, 0.65, 0.0, 0.24, 0.22, 0.72, 0.95, 1.0, 0.4286, 0.22, 0.48, 1.0,
            0.4167, 0.38, 0.32, 1.0, 0.4286, 0.30, 0.50, 0.75, 0.40, 0.88, 0.68, 0.55, 0.15, 0.22,
            0.30, 0.18, 0.42, 0.20, 0.000520, 0.000300, 0.000400, 0.000650, 0.50, 0.86, 0.00, 0.65,
            0.74, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Guitar Body",
        category: 0,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.00, 0.006, 0.15, 0.55, 0.0, 0.22, 0.25, 0.70, 0.95, 1.0, 0.4286, 0.20, 0.44, 1.0,
            0.4167, 0.35, 0.33, 1.0, 0.4286, 0.30, 0.48, 0.72, 0.44, 0.88, 0.72, 0.55, 0.12, 0.20,
            0.28, 0.18, 0.42, 0.22, 0.000570, 0.000320, 0.000420, 0.000680, 0.50, 0.86, 0.00, 0.60,
            0.70, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Stereo Enhancer",
        category: 0,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.00, 0.005, 0.10, 0.70, 0.0, 0.25, 0.20, 0.70, 0.90, 1.0, 0.5714, 0.22, 0.45, 1.0,
            0.4167, 0.35, 0.31, 1.0, 0.4286, 0.30, 0.48, 0.75, 0.35, 0.85, 0.65, 0.62, 0.15, 0.22,
            0.32, 0.20, 0.42, 0.22, 0.000500, 0.000280, 0.000380, 0.000650, 0.50, 0.85, 0.00, 0.65,
            0.72, 0.62, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Noir Space",
        category: 0,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.00, 0.010, 0.20, 0.45, 0.0, 0.20, 0.25, 0.68, 0.85, 1.0, 0.4286, 0.25, 0.55, 1.0,
            0.4167, 0.40, 0.36, 1.0, 0.4286, 0.35, 0.52, 0.55, 0.55, 0.95, 0.78, 0.42, 0.10, 0.18,
            0.26, 0.15, 0.38, 0.18, 0.000620, 0.000380, 0.000480, 0.000750, 0.50, 0.88, 0.00, 0.55,
            0.68, 0.50, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // ROOMS (1) — CloudSeed — 8 presets
    // ======================================================================
    FactoryPreset {
        name: "Small Wood Room",
        category: 1,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.00, 0.012, 0.05, 0.60, 1.0, 0.22, 0.25, 0.85, 0.90, 1.0, 0.5714, 0.25, 0.55, 1.0,
            0.5000, 0.42, 0.36, 1.0, 0.5714, 0.38, 0.58, 0.78, 0.28, 0.88, 0.65, 0.72, 0.20, 0.30,
            0.40, 0.28, 0.55, 0.30, 0.000450, 0.000260, 0.000340, 0.000550, 0.50, 0.78, 0.00, 0.66,
            0.78, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Tight Drum Room",
        category: 1,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.05, 0.020, 0.05, 0.68, 1.0, 0.30, 0.28, 0.88, 0.88, 1.0, 0.7143, 0.32, 0.60, 1.0,
            0.5833, 0.45, 0.40, 1.0, 0.7143, 0.40, 0.60, 0.85, 0.22, 0.85, 0.60, 0.78, 0.22, 0.32,
            0.45, 0.30, 0.58, 0.32, 0.000400, 0.000240, 0.000320, 0.000520, 0.50, 0.75, 0.00, 0.72,
            0.80, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Vocal Booth",
        category: 1,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.00, 0.008, 0.08, 0.55, 1.0, 0.18, 0.22, 0.80, 0.92, 1.0, 0.4286, 0.22, 0.50, 1.0,
            0.5000, 0.38, 0.42, 1.0, 0.4286, 0.32, 0.52, 0.72, 0.32, 0.90, 0.60, 0.65, 0.15, 0.25,
            0.35, 0.22, 0.45, 0.25, 0.000500, 0.000300, 0.000380, 0.000620, 0.50, 0.80, 0.00, 0.62,
            0.74, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Live Room L",
        category: 1,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.08, 0.025, 0.05, 0.62, 1.0, 0.35, 0.32, 0.85, 0.80, 1.0, 0.7143, 0.38, 0.62, 1.0,
            0.5000, 0.45, 0.44, 1.0, 0.5714, 0.42, 0.62, 0.82, 0.30, 0.82, 0.60, 0.72, 0.22, 0.32,
            0.45, 0.30, 0.58, 0.32, 0.000420, 0.000260, 0.000340, 0.000540, 0.50, 0.78, 0.00, 0.68,
            0.78, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Brick Room",
        category: 1,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.06, 0.015, 0.08, 0.62, 1.0, 0.42, 0.30, 0.72, 0.72, 1.0, 0.7143, 0.38, 0.65, 1.0,
            0.5833, 0.48, 0.45, 1.0, 0.7143, 0.45, 0.65, 0.88, 0.22, 0.75, 0.58, 0.82, 0.25, 0.35,
            0.48, 0.32, 0.60, 0.35, 0.000380, 0.000220, 0.000300, 0.000480, 0.50, 0.72, 0.00, 0.72,
            0.80, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Concrete Garage",
        category: 1,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.10, 0.030, 0.05, 0.55, 1.0, 0.45, 0.35, 0.70, 0.68, 1.0, 0.7143, 0.42, 0.68, 1.0,
            0.5833, 0.55, 0.46, 1.0, 0.7143, 0.48, 0.68, 0.72, 0.32, 0.85, 0.65, 0.60, 0.18, 0.28,
            0.42, 0.28, 0.52, 0.28, 0.000420, 0.000260, 0.000340, 0.000540, 0.50, 0.78, 0.00, 0.65,
            0.76, 0.50, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Tiled Room",
        category: 1,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.12, 0.020, 0.10, 0.75, 1.0, 0.28, 0.25, 0.72, 0.85, 1.0, 0.5714, 0.30, 0.55, 1.0,
            0.5000, 0.42, 0.43, 1.0, 0.5714, 0.38, 0.58, 0.82, 0.42, 0.80, 0.58, 0.65, 0.18, 0.28,
            0.40, 0.25, 0.52, 0.28, 0.000480, 0.000280, 0.000380, 0.000580, 0.50, 0.78, 0.00, 0.65,
            0.76, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Drum Hall",
        category: 1,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.15, 0.035, 0.05, 0.70, 1.0, 0.40, 0.40, 0.82, 0.78, 1.0, 0.7143, 0.42, 0.68, 1.0,
            0.6667, 0.58, 0.47, 1.0, 0.7143, 0.48, 0.68, 0.82, 0.30, 0.80, 0.62, 0.75, 0.25, 0.35,
            0.52, 0.35, 0.60, 0.38, 0.000380, 0.000220, 0.000300, 0.000480, 0.50, 0.75, 0.00, 0.70,
            0.80, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // CHAMBERS (2) — CloudSeed — 7 presets
    // ======================================================================
    FactoryPreset {
        name: "EMI Chamber",
        category: 2,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.05, 0.025, 0.08, 0.60, 1.0, 0.30, 0.50, 0.72, 0.72, 1.0, 0.7143, 0.42, 0.62, 1.0,
            0.6667, 0.58, 0.43, 1.0, 0.7143, 0.45, 0.65, 0.82, 0.32, 0.80, 0.62, 0.78, 0.25, 0.35,
            0.52, 0.35, 0.60, 0.38, 0.000380, 0.000220, 0.000300, 0.000500, 0.50, 0.75, 0.00, 0.68,
            0.78, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Vintage Echo Chamber",
        category: 2,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.05, 0.030, 0.10, 0.50, 1.0, 0.28, 0.45, 0.68, 0.75, 1.0, 0.5714, 0.42, 0.60, 1.0,
            0.5833, 0.52, 0.46, 1.0, 0.5714, 0.45, 0.62, 0.78, 0.38, 0.85, 0.65, 0.68, 0.22, 0.32,
            0.50, 0.32, 0.55, 0.35, 0.000420, 0.000250, 0.000320, 0.000520, 0.50, 0.78, 0.00, 0.68,
            0.78, 0.50, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Scoring Chamber",
        category: 2,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.02, 0.035, 0.06, 0.68, 1.0, 0.40, 0.55, 0.75, 0.78, 1.0, 0.8571, 0.50, 0.68, 1.0,
            0.7500, 0.62, 0.46, 1.0, 0.7143, 0.50, 0.68, 0.86, 0.28, 0.78, 0.60, 0.82, 0.28, 0.38,
            0.58, 0.38, 0.62, 0.42, 0.000350, 0.000200, 0.000280, 0.000480, 0.50, 0.72, 0.00, 0.70,
            0.80, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Marble Chamber",
        category: 2,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.10, 0.020, 0.08, 0.75, 1.0, 0.42, 0.48, 0.72, 0.72, 1.0, 0.7143, 0.46, 0.68, 1.0,
            0.6667, 0.55, 0.47, 1.0, 0.7143, 0.48, 0.68, 0.88, 0.24, 0.72, 0.58, 0.85, 0.30, 0.40,
            0.60, 0.40, 0.65, 0.45, 0.000340, 0.000200, 0.000280, 0.000460, 0.50, 0.72, 0.00, 0.72,
            0.80, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Chamber Pop Vocals",
        category: 2,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.10, 0.020, 0.10, 0.60, 1.0, 0.28, 0.48, 0.70, 0.78, 1.0, 0.8571, 0.42, 0.65, 1.0,
            0.5833, 0.55, 0.45, 1.0, 0.7143, 0.42, 0.65, 0.82, 0.22, 0.78, 0.60, 0.82, 0.28, 0.38,
            0.55, 0.38, 0.60, 0.42, 0.000380, 0.000220, 0.000300, 0.000500, 0.50, 0.72, 0.00, 0.78,
            0.85, 0.50, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Dense Diffusion Chamber",
        category: 2,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.08, 0.030, 0.08, 0.65, 1.0, 0.38, 0.52, 0.72, 0.75, 1.0, 1.0000, 0.55, 0.75, 1.0,
            0.8333, 0.62, 0.48, 1.0, 1.0000, 0.55, 0.72, 0.84, 0.28, 0.78, 0.62, 0.80, 0.32, 0.42,
            0.60, 0.42, 0.65, 0.45, 0.000340, 0.000200, 0.000280, 0.000460, 0.50, 0.72, 0.00, 0.70,
            0.80, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Tube Chamber",
        category: 2,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.12, 0.045, 0.05, 0.55, 1.0, 0.32, 0.55, 0.70, 0.80, 1.0, 0.5714, 0.45, 0.62, 1.0,
            0.5833, 0.55, 0.42, 1.0, 0.5714, 0.40, 0.62, 0.80, 0.36, 0.85, 0.65, 0.72, 0.30, 0.40,
            0.55, 0.40, 0.60, 0.42, 0.000380, 0.000220, 0.000300, 0.000500, 0.50, 0.72, 0.00, 0.62,
            0.78, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // PLATES (3) — CloudSeed — 7 presets
    // ======================================================================
    FactoryPreset {
        name: "EMT 140 Plate",
        category: 3,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.00, 0.000, 0.25, 0.78, 0.0, 0.30, 0.50, 0.68, 0.70, 1.0, 1.0000, 0.55, 0.78, 1.0,
            0.7500, 0.72, 0.48, 1.0, 0.8571, 0.60, 0.75, 0.88, 0.28, 0.78, 0.60, 0.82, 0.30, 0.38,
            0.55, 0.38, 0.60, 0.42, 0.000350, 0.000200, 0.000280, 0.000450, 0.50, 0.72, 0.00, 0.72,
            0.82, 0.50, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Vocal Plate Bright",
        category: 3,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.05, 0.000, 0.20, 0.85, 0.0, 0.32, 0.55, 0.72, 0.75, 1.0, 1.0000, 0.60, 0.80, 1.0,
            0.8333, 0.78, 0.50, 1.0, 0.8571, 0.62, 0.72, 0.90, 0.22, 0.72, 0.55, 0.85, 0.35, 0.45,
            0.62, 0.42, 0.65, 0.45, 0.000320, 0.000180, 0.000260, 0.000420, 0.50, 0.68, 0.00, 0.78,
            0.85, 0.50, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Drum Plate Dark",
        category: 3,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.08, 0.000, 0.20, 0.55, 0.0, 0.35, 0.55, 0.60, 0.65, 1.0, 1.0000, 0.55, 0.75, 1.0,
            0.8333, 0.68, 0.44, 1.0, 0.8571, 0.55, 0.72, 0.78, 0.42, 0.85, 0.68, 0.72, 0.25, 0.32,
            0.52, 0.32, 0.55, 0.35, 0.000400, 0.000240, 0.000320, 0.000500, 0.50, 0.78, 0.00, 0.68,
            0.78, 0.50, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Vocal Plate Warm",
        category: 3,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.10, 0.005, 0.15, 0.65, 0.0, 0.28, 0.52, 0.72, 0.82, 1.0, 1.0000, 0.55, 0.75, 1.0,
            0.7500, 0.72, 0.50, 1.0, 0.8571, 0.58, 0.68, 0.85, 0.30, 0.78, 0.58, 0.82, 0.32, 0.40,
            0.60, 0.40, 0.62, 0.45, 0.000320, 0.000180, 0.000260, 0.000420, 0.50, 0.72, 0.00, 0.82,
            0.85, 0.50, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Slap Plate",
        category: 3,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.15, 0.000, 0.20, 0.80, 0.0, 0.38, 0.42, 0.72, 0.78, 1.0, 0.8571, 0.58, 0.78, 1.0,
            0.6667, 0.68, 0.50, 1.0, 0.7143, 0.58, 0.72, 0.90, 0.20, 0.72, 0.58, 0.85, 0.35, 0.45,
            0.65, 0.42, 0.68, 0.45, 0.000300, 0.000180, 0.000260, 0.000420, 0.50, 0.65, 0.00, 0.78,
            0.85, 0.50, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Ambient Plate Sustain",
        category: 3,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.12, 0.000, 0.15, 0.78, 0.0, 0.42, 0.70, 0.62, 0.62, 1.0, 1.0000, 0.65, 0.82, 1.0,
            0.9167, 0.78, 0.56, 1.0, 1.0000, 0.68, 0.78, 0.88, 0.22, 0.72, 0.55, 0.82, 0.35, 0.45,
            0.65, 0.45, 0.68, 0.48, 0.000320, 0.000180, 0.000260, 0.000420, 0.50, 0.62, 0.00, 0.72,
            0.82, 0.50, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Steel Plate Metallic",
        category: 3,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.02, 0.000, 0.20, 0.88, 0.0, 0.35, 0.58, 0.72, 0.78, 1.0, 1.0000, 0.60, 0.80, 1.0,
            0.8333, 0.75, 0.52, 1.0, 0.8571, 0.62, 0.75, 0.90, 0.28, 0.72, 0.58, 0.85, 0.32, 0.40,
            0.60, 0.42, 0.62, 0.45, 0.000350, 0.000200, 0.000280, 0.000450, 0.50, 0.68, 0.00, 0.75,
            0.82, 0.50, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // HALLS (4) — CloudSeed — 8 presets
    // ======================================================================
    FactoryPreset {
        name: "Concert Hall L",
        category: 4,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.15, 0.045, 0.05, 0.70, 1.0, 0.35, 0.55, 0.62, 0.72, 1.0, 0.8571, 0.55, 0.75, 1.0,
            0.7500, 0.72, 0.55, 1.0, 0.7143, 0.55, 0.72, 0.85, 0.35, 0.72, 0.70, 0.78, 0.28, 0.38,
            0.62, 0.38, 0.62, 0.42, 0.000350, 0.000220, 0.000300, 0.000480, 0.50, 0.75, 0.00, 0.72,
            0.82, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Musikverein Warm",
        category: 4,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.12, 0.050, 0.08, 0.60, 1.0, 0.38, 0.55, 0.65, 0.68, 1.0, 0.8571, 0.55, 0.72, 1.0,
            0.7500, 0.72, 0.57, 1.0, 0.7143, 0.55, 0.70, 0.78, 0.42, 0.78, 0.65, 0.75, 0.22, 0.32,
            0.58, 0.35, 0.55, 0.38, 0.000400, 0.000250, 0.000320, 0.000500, 0.50, 0.78, 0.00, 0.72,
            0.80, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Recital Hall",
        category: 4,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.18, 0.030, 0.05, 0.65, 1.0, 0.32, 0.50, 0.62, 0.78, 1.0, 0.7143, 0.50, 0.72, 1.0,
            0.6667, 0.62, 0.55, 1.0, 0.7143, 0.50, 0.72, 0.82, 0.32, 0.78, 0.65, 0.75, 0.28, 0.38,
            0.55, 0.38, 0.60, 0.42, 0.000380, 0.000240, 0.000320, 0.000500, 0.50, 0.78, 0.00, 0.72,
            0.80, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Bright Orchestral Hall",
        category: 4,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.20, 0.040, 0.08, 0.82, 1.0, 0.42, 0.60, 0.72, 0.72, 1.0, 0.8571, 0.60, 0.80, 1.0,
            0.8333, 0.78, 0.58, 1.0, 0.8571, 0.62, 0.78, 0.92, 0.24, 0.68, 0.60, 0.85, 0.35, 0.42,
            0.68, 0.45, 0.68, 0.48, 0.000320, 0.000200, 0.000280, 0.000440, 0.50, 0.70, 0.00, 0.78,
            0.85, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Dark Symphonic Hall",
        category: 4,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.08, 0.055, 0.15, 0.40, 1.0, 0.42, 0.42, 0.60, 0.82, 1.0, 0.8571, 0.55, 0.80, 1.0,
            0.7500, 0.55, 0.60, 1.0, 0.7143, 0.55, 0.78, 0.60, 0.52, 0.92, 0.75, 0.55, 0.18, 0.28,
            0.48, 0.32, 0.55, 0.35, 0.000400, 0.000250, 0.000320, 0.000500, 0.50, 0.82, 0.00, 0.62,
            0.80, 0.50, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Golden Hall",
        category: 4,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.05, 0.065, 0.05, 0.72, 1.0, 0.45, 0.65, 0.72, 0.85, 1.0, 1.0000, 0.65, 0.85, 1.0,
            0.9167, 0.78, 0.60, 1.0, 0.8571, 0.65, 0.78, 0.82, 0.38, 0.72, 0.68, 0.78, 0.32, 0.42,
            0.68, 0.48, 0.70, 0.48, 0.000300, 0.000180, 0.000260, 0.000420, 0.50, 0.72, 0.00, 0.72,
            0.85, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Chamber Hall",
        category: 4,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.20, 0.025, 0.05, 0.62, 1.0, 0.32, 0.45, 0.62, 0.75, 1.0, 0.7143, 0.48, 0.70, 1.0,
            0.6667, 0.60, 0.55, 1.0, 0.7143, 0.48, 0.72, 0.85, 0.32, 0.78, 0.65, 0.75, 0.28, 0.38,
            0.55, 0.40, 0.60, 0.42, 0.000380, 0.000240, 0.000320, 0.000500, 0.50, 0.78, 0.00, 0.70,
            0.80, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Wide Opera Hall",
        category: 4,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.25, 0.055, 0.06, 0.68, 1.0, 0.42, 0.60, 0.68, 0.78, 1.0, 0.8571, 0.58, 0.78, 1.0,
            0.8333, 0.72, 0.58, 1.0, 0.8571, 0.60, 0.78, 0.82, 0.35, 0.72, 0.65, 0.80, 0.30, 0.40,
            0.62, 0.42, 0.65, 0.45, 0.000340, 0.000210, 0.000290, 0.000460, 0.50, 0.72, 0.00, 0.72,
            0.85, 0.62, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // CATHEDRAL (5) — CloudSeed — 6 presets
    // ======================================================================
    FactoryPreset {
        name: "Notre Dame",
        category: 5,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.02, 0.075, 0.10, 0.60, 1.0, 0.48, 0.70, 0.72, 0.88, 1.0, 1.0000, 0.70, 0.85, 1.0,
            0.9167, 0.78, 0.65, 1.0, 0.8571, 0.68, 0.80, 0.75, 0.42, 0.80, 0.68, 0.75, 0.35, 0.45,
            0.72, 0.48, 0.70, 0.48, 0.000280, 0.000180, 0.000240, 0.000400, 0.50, 0.72, 0.00, 0.68,
            0.82, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Endless Nave",
        category: 5,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.00, 0.100, 0.10, 0.62, 1.0, 0.55, 0.75, 0.75, 0.92, 1.0, 1.0000, 0.78, 0.90, 1.0,
            1.0000, 0.82, 0.66, 1.0, 1.0000, 0.72, 0.82, 0.78, 0.38, 0.78, 0.68, 0.78, 0.38, 0.48,
            0.75, 0.52, 0.72, 0.52, 0.000260, 0.000160, 0.000220, 0.000380, 0.50, 0.68, 0.00, 0.72,
            0.85, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "St. Paul's Dome",
        category: 5,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.05, 0.085, 0.12, 0.65, 1.0, 0.50, 0.72, 0.72, 0.88, 1.0, 1.0000, 0.72, 0.88, 1.0,
            0.9167, 0.78, 0.65, 1.0, 0.8571, 0.70, 0.80, 0.78, 0.38, 0.78, 0.65, 0.78, 0.35, 0.45,
            0.72, 0.48, 0.70, 0.50, 0.000300, 0.000180, 0.000260, 0.000420, 0.50, 0.70, 0.00, 0.72,
            0.85, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Underground Crypt",
        category: 5,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.00, 0.060, 0.05, 0.35, 1.0, 0.42, 0.68, 0.68, 0.85, 1.0, 0.8571, 0.62, 0.82, 1.0,
            0.7500, 0.62, 0.66, 1.0, 0.7143, 0.62, 0.78, 0.55, 0.55, 0.92, 0.75, 0.50, 0.18, 0.28,
            0.50, 0.32, 0.55, 0.35, 0.000420, 0.000250, 0.000320, 0.000520, 0.50, 0.78, 0.00, 0.62,
            0.78, 0.50, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Cloister",
        category: 5,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.08, 0.070, 0.08, 0.58, 1.0, 0.45, 0.68, 0.70, 0.85, 1.0, 0.8571, 0.65, 0.85, 1.0,
            0.8333, 0.72, 0.66, 1.0, 0.8571, 0.65, 0.78, 0.72, 0.42, 0.82, 0.68, 0.72, 0.32, 0.42,
            0.62, 0.42, 0.62, 0.45, 0.000320, 0.000200, 0.000280, 0.000460, 0.50, 0.72, 0.00, 0.68,
            0.80, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Milan Duomo",
        category: 5,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.05, 0.095, 0.15, 0.55, 1.0, 0.52, 0.78, 0.72, 0.92, 1.0, 1.0000, 0.78, 0.88, 1.0,
            1.0000, 0.82, 0.68, 1.0, 1.0000, 0.75, 0.82, 0.72, 0.45, 0.82, 0.72, 0.72, 0.38, 0.48,
            0.75, 0.52, 0.72, 0.52, 0.000280, 0.000180, 0.000240, 0.000400, 0.50, 0.68, 0.00, 0.70,
            0.85, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // TEXTURES (6) — CloudSeed + Texture Rack — 8 presets
    // ======================================================================
    FactoryPreset {
        name: "Shimmer Bloom",
        category: 6,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.15, 0.060, 0.05, 0.72, 1.0, 0.45, 0.62, 0.72, 0.80, 1.0, 0.8571, 0.60, 0.78, 1.0,
            0.7500, 0.68, 0.66, 1.0, 0.8571, 0.62, 0.78, 0.85, 0.30, 0.78, 0.58, 0.80, 0.42, 0.50,
            0.68, 0.48, 0.68, 0.50, 0.000320, 0.000200, 0.000260, 0.000420, 0.50, 0.62, 0.00, 0.72,
            0.82, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.55, 0.0, 0.35, 0.0, 0.0, 0.0),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Ethereal Cloud",
        category: 6,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.18, 0.075, 0.08, 0.68, 1.0, 0.52, 0.72, 0.70, 0.78, 1.0, 0.8571, 0.68, 0.82, 1.0,
            0.9167, 0.75, 0.68, 1.0, 0.8571, 0.72, 0.78, 0.80, 0.28, 0.72, 0.55, 0.80, 0.38, 0.48,
            0.65, 0.48, 0.68, 0.50, 0.000300, 0.000180, 0.000240, 0.000400, 0.50, 0.65, 0.00, 0.72,
            0.82, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.0, 0.45, 0.0, 0.30, 0.25, 0.35),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Crystal Cave",
        category: 6,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.22, 0.065, 0.08, 0.85, 1.0, 0.55, 0.75, 0.72, 0.78, 1.0, 0.8571, 0.72, 0.85, 1.0,
            0.9167, 0.78, 0.70, 1.0, 0.8571, 0.75, 0.80, 0.90, 0.24, 0.68, 0.55, 0.85, 0.42, 0.52,
            0.72, 0.52, 0.72, 0.55, 0.000280, 0.000160, 0.000220, 0.000380, 0.50, 0.60, 0.00, 0.78,
            0.85, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.0, 0.0, 0.0, 0.55, 0.30, 0.0),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Ghost Choir",
        category: 6,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.25, 0.085, 0.08, 0.62, 1.0, 0.55, 0.80, 0.68, 0.72, 1.0, 0.8571, 0.72, 0.88, 1.0,
            1.0000, 0.82, 0.70, 1.0, 0.8571, 0.78, 0.80, 0.78, 0.38, 0.78, 0.62, 0.78, 0.45, 0.55,
            0.72, 0.55, 0.75, 0.58, 0.000280, 0.000180, 0.000220, 0.000380, 0.50, 0.58, 0.00, 0.72,
            0.85, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.35, 0.0, 0.40, 0.0, 0.0, 0.45),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Frozen Glass",
        category: 6,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.15, 0.080, 0.12, 0.82, 1.0, 0.48, 0.68, 0.70, 0.80, 1.0, 0.7143, 0.65, 0.80, 1.0,
            0.8333, 0.78, 0.70, 1.0, 0.8571, 0.72, 0.80, 0.92, 0.22, 0.65, 0.55, 0.88, 0.40, 0.50,
            0.70, 0.50, 0.72, 0.52, 0.000300, 0.000180, 0.000240, 0.000400, 0.50, 0.60, 0.00, 0.75,
            0.85, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.25, 0.35, 0.0, 0.40, 0.0, 0.0),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Velvet Drape",
        category: 6,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.10, 0.045, 0.10, 0.55, 1.0, 0.42, 0.58, 0.72, 0.82, 1.0, 0.7143, 0.55, 0.75, 1.0,
            0.7500, 0.65, 0.68, 1.0, 0.8571, 0.62, 0.75, 0.72, 0.38, 0.85, 0.65, 0.72, 0.35, 0.45,
            0.62, 0.45, 0.65, 0.48, 0.000350, 0.000220, 0.000280, 0.000480, 0.50, 0.70, 0.00, 0.72,
            0.80, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.0, 0.55, 0.0, 0.0, 0.0, 0.0),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Slate Rain",
        category: 6,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.20, 0.070, 0.10, 0.60, 1.0, 0.42, 0.60, 0.68, 0.82, 1.0, 0.7143, 0.62, 0.78, 1.0,
            0.7500, 0.68, 0.66, 1.0, 0.7143, 0.62, 0.75, 0.72, 0.42, 0.82, 0.68, 0.72, 0.35, 0.45,
            0.62, 0.45, 0.65, 0.48, 0.000350, 0.000220, 0.000280, 0.000460, 0.50, 0.70, 0.00, 0.72,
            0.80, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.0, 0.0, 0.0, 0.0, 0.45, 0.30),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Whisper Veil",
        category: 6,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.08, 0.055, 0.15, 0.55, 1.0, 0.38, 0.55, 0.70, 0.85, 1.0, 0.7143, 0.55, 0.75, 1.0,
            0.7500, 0.62, 0.68, 1.0, 0.7143, 0.60, 0.78, 0.62, 0.50, 0.90, 0.72, 0.58, 0.22, 0.32,
            0.52, 0.35, 0.58, 0.38, 0.000380, 0.000240, 0.000320, 0.000500, 0.50, 0.78, 0.00, 0.62,
            0.78, 0.50, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.0, 0.45, 0.35, 0.0, 0.0, 0.25),
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // EXTREME (7) — CloudSeed — 6 presets
    // ======================================================================
    FactoryPreset {
        name: "Infinite Sustain",
        category: 7,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.00, 0.040, 0.10, 0.68, 1.0, 0.55, 0.78, 0.72, 0.92, 1.0, 1.0000, 0.75, 0.90, 1.0,
            1.0000, 0.78, 0.80, 1.0, 1.0000, 0.72, 0.85, 0.78, 0.35, 0.78, 0.62, 0.78, 0.35, 0.45,
            0.72, 0.50, 0.72, 0.50, 0.000280, 0.000180, 0.000240, 0.000400, 0.50, 0.55, 0.00, 0.72,
            0.88, 0.75, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.0, 0.0, 0.0, 0.0, 0.0, 0.35),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Void",
        category: 7,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.05, 0.120, 0.10, 0.35, 1.0, 0.62, 0.85, 0.78, 0.95, 1.0, 1.0000, 0.82, 0.92, 1.0,
            1.0000, 0.75, 0.82, 1.0, 1.0000, 0.72, 0.88, 0.55, 0.55, 0.92, 0.78, 0.52, 0.20, 0.30,
            0.55, 0.35, 0.60, 0.35, 0.000400, 0.000250, 0.000320, 0.000520, 0.50, 0.72, 0.00, 0.55,
            0.78, 0.75, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.30, 0.40, 0.0, 0.0, 0.45, 0.0),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Reverse Bloom",
        category: 7,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.20, 0.000, 0.05, 0.82, 1.0, 0.65, 0.82, 0.72, 0.65, 1.0, 1.0000, 0.78, 0.88, 1.0,
            0.9167, 0.82, 0.82, 1.0, 1.0000, 0.78, 0.82, 0.92, 0.20, 0.68, 0.48, 0.90, 0.48, 0.58,
            0.75, 0.55, 0.75, 0.58, 0.000260, 0.000160, 0.000220, 0.000380, 0.50, 0.55, 0.00, 0.82,
            0.88, 0.75, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.0, 0.0, 0.0, 0.0, 0.0, 0.55),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Gated Bloom",
        category: 7,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.00, 0.000, 0.05, 0.78, 1.0, 0.35, 0.42, 0.88, 0.95, 1.0, 0.7143, 0.42, 0.62, 1.0,
            0.5000, 0.48, 0.82, 1.0, 0.7143, 0.50, 0.65, 0.88, 0.30, 0.78, 0.62, 0.75, 0.22, 0.32,
            0.48, 0.30, 0.55, 0.32, 0.000420, 0.000260, 0.000340, 0.000550, 0.50, 0.72, 0.00, 0.78,
            0.85, 0.75, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.0, 0.0, 0.0, 0.35, 0.40, 0.0),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Radio Static Hall",
        category: 7,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.35, 0.020, 0.45, 0.28, 1.0, 0.38, 0.45, 0.72, 0.88, 1.0, 0.5714, 0.35, 0.62, 1.0,
            0.5000, 0.42, 0.80, 1.0, 0.5714, 0.42, 0.62, 0.55, 0.62, 0.92, 0.82, 0.42, 0.15, 0.25,
            0.42, 0.28, 0.48, 0.28, 0.000520, 0.000320, 0.000420, 0.000650, 0.50, 0.85, 0.00, 0.52,
            0.62, 0.75, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.45, 0.0, 0.35, 0.0, 0.55, 0.0),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Time Smear",
        category: 7,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.30, 0.100, 0.05, 0.72, 1.0, 0.62, 0.85, 0.72, 0.90, 1.0, 1.0000, 0.82, 0.92, 1.0,
            1.0000, 0.78, 0.84, 1.0, 1.0000, 0.78, 0.85, 0.82, 0.38, 0.78, 0.58, 0.78, 0.48, 0.58,
            0.75, 0.55, 0.75, 0.58, 0.000280, 0.000160, 0.000240, 0.000400, 0.50, 0.55, 0.00, 0.72,
            0.85, 0.75, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.35, 0.30, 0.25, 0.25, 0.30, 0.35),
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // FFDN SPACES (8) — FFDN — 6 presets
    // ======================================================================
    FactoryPreset {
        name: "FFDN Small Room",
        category: 8,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.15,
            decay: 0.02,
            damping: 0.55,
            spread: 0.6,
            mod_amount: 0.10,
            mod_rate: 0.15,
            diffusion: 0.35,
            bass_mult: 0.45,
            treble_mult: 0.40,
            lines: 8,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Studio Room",
        category: 8,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.22,
            decay: 0.035,
            damping: 0.50,
            spread: 0.55,
            mod_amount: 0.15,
            mod_rate: 0.18,
            diffusion: 0.45,
            bass_mult: 0.48,
            treble_mult: 0.45,
            lines: 8,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Drum Room",
        category: 8,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.10,
            decay: 0.02,
            damping: 0.65,
            spread: 0.7,
            mod_amount: 0.08,
            mod_rate: 0.2,
            diffusion: 0.30,
            bass_mult: 0.4,
            treble_mult: 0.35,
            lines: 8,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Vocal Booth",
        category: 8,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.13,
            decay: 0.015,
            damping: 0.55,
            spread: 0.5,
            mod_amount: 0.10,
            mod_rate: 0.15,
            diffusion: 0.40,
            bass_mult: 0.40,
            treble_mult: 0.55,
            lines: 8,
            density: 0.6,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Live Room",
        category: 8,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.30,
            decay: 0.06,
            damping: 0.48,
            spread: 0.5,
            mod_amount: 0.20,
            mod_rate: 0.2,
            diffusion: 0.5,
            bass_mult: 0.5,
            treble_mult: 0.5,
            lines: 10,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Big Wood",
        category: 8,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.35,
            decay: 0.09,
            damping: 0.42,
            spread: 0.55,
            mod_amount: 0.18,
            mod_rate: 0.15,
            diffusion: 0.55,
            bass_mult: 0.55,
            treble_mult: 0.45,
            lines: 10,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // FFDN HALLS (9) — FFDN — 6 presets
    // ======================================================================
    FactoryPreset {
        name: "FFDN Concert Hall",
        category: 9,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.55,
            decay: 0.15,
            damping: 0.45,
            spread: 0.55,
            mod_amount: 0.22,
            mod_rate: 0.13,
            diffusion: 0.6,
            bass_mult: 0.55,
            treble_mult: 0.50,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Symphony Hall",
        category: 9,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.65,
            decay: 0.22,
            damping: 0.48,
            spread: 0.5,
            mod_amount: 0.25,
            mod_rate: 0.12,
            diffusion: 0.65,
            bass_mult: 0.6,
            treble_mult: 0.48,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Chamber Hall",
        category: 9,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.42,
            decay: 0.11,
            damping: 0.50,
            spread: 0.6,
            mod_amount: 0.20,
            mod_rate: 0.15,
            diffusion: 0.55,
            bass_mult: 0.52,
            treble_mult: 0.50,
            lines: 10,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Cathedral Hall",
        category: 9,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.78,
            decay: 0.28,
            damping: 0.38,
            spread: 0.45,
            mod_amount: 0.28,
            mod_rate: 0.10,
            diffusion: 0.7,
            bass_mult: 0.62,
            treble_mult: 0.42,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Ice Hall",
        category: 9,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.60,
            decay: 0.18,
            damping: 0.65,
            spread: 0.55,
            mod_amount: 0.24,
            mod_rate: 0.15,
            diffusion: 0.65,
            bass_mult: 0.35,
            treble_mult: 0.68,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Subterranean",
        category: 9,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.85,
            decay: 0.32,
            damping: 0.28,
            spread: 0.55,
            mod_amount: 0.30,
            mod_rate: 0.08,
            diffusion: 0.6,
            bass_mult: 0.72,
            treble_mult: 0.35,
            lines: 12,
            mid_focus: 0.15,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // FFDN PLATES (10) — FFDN — 5 presets
    // ======================================================================
    FactoryPreset {
        name: "FFDN Bright Plate",
        category: 10,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.45,
            decay: 0.13,
            damping: 0.75,
            spread: 0.85,
            mod_amount: 0.15,
            mod_rate: 0.10,
            diffusion: 0.7,
            bass_mult: 0.35,
            treble_mult: 0.65,
            velvet_matrix: true,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Vintage Plate",
        category: 10,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.40,
            decay: 0.11,
            damping: 0.55,
            spread: 0.80,
            mod_amount: 0.18,
            mod_rate: 0.12,
            diffusion: 0.75,
            bass_mult: 0.42,
            treble_mult: 0.55,
            velvet_matrix: true,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Steel Plate",
        category: 10,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.38,
            decay: 0.15,
            damping: 0.82,
            spread: 0.85,
            mod_amount: 0.12,
            mod_rate: 0.10,
            diffusion: 0.75,
            bass_mult: 0.30,
            treble_mult: 0.72,
            velvet_matrix: true,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Vocal Plate",
        category: 10,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.42,
            decay: 0.10,
            damping: 0.60,
            spread: 0.80,
            mod_amount: 0.20,
            mod_rate: 0.13,
            diffusion: 0.78,
            bass_mult: 0.40,
            treble_mult: 0.58,
            velvet_matrix: true,
            lines: 12,
            density: 0.6,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Dense Plate",
        category: 10,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.55,
            decay: 0.20,
            damping: 0.65,
            spread: 0.85,
            mod_amount: 0.22,
            mod_rate: 0.10,
            diffusion: 0.85,
            bass_mult: 0.45,
            treble_mult: 0.60,
            velvet_matrix: true,
            lines: 16,
            density: 0.55,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // FFDN AMBIENT (11) — FFDN — 6 presets
    // ======================================================================
    FactoryPreset {
        name: "FFDN Endless Space",
        category: 11,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.75,
            decay: 0.75,
            damping: 0.35,
            spread: 0.65,
            mod_amount: 0.45,
            mod_rate: 0.08,
            diffusion: 0.7,
            bass_mult: 0.62,
            treble_mult: 0.40,
            lines: 16,
            ..NO_FFDN
        },
        texture: TextureConfig::single(0.25, 0.0, 0.35, 0.0, 0.0, 0.30),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Drifting Pad",
        category: 11,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.60,
            decay: 0.55,
            damping: 0.42,
            spread: 0.6,
            mod_amount: 0.55,
            mod_rate: 0.10,
            diffusion: 0.65,
            bass_mult: 0.55,
            treble_mult: 0.50,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::single(0.0, 0.45, 0.40, 0.0, 0.0, 0.0),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Deep Chasm",
        category: 11,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.90,
            decay: 0.65,
            damping: 0.22,
            spread: 0.55,
            mod_amount: 0.40,
            mod_rate: 0.06,
            diffusion: 0.6,
            bass_mult: 0.75,
            treble_mult: 0.30,
            lines: 16,
            mid_focus: 0.10,
            ..NO_FFDN
        },
        texture: TextureConfig::single(0.0, 0.30, 0.0, 0.0, 0.0, 0.35),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Glass Labyrinth",
        category: 11,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.65,
            decay: 0.60,
            damping: 0.72,
            spread: 0.70,
            mod_amount: 0.35,
            mod_rate: 0.14,
            diffusion: 0.75,
            bass_mult: 0.35,
            treble_mult: 0.68,
            velvet_matrix: true,
            lines: 16,
            ..NO_FFDN
        },
        texture: TextureConfig::single(0.30, 0.0, 0.0, 0.40, 0.0, 0.0),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Nebula",
        category: 11,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.70,
            decay: 0.68,
            damping: 0.45,
            spread: 0.75,
            mod_amount: 0.50,
            mod_rate: 0.09,
            diffusion: 0.70,
            bass_mult: 0.55,
            treble_mult: 0.55,
            lines: 16,
            ..NO_FFDN
        },
        texture: TextureConfig::single(0.0, 0.55, 0.30, 0.0, 0.0, 0.0),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Ice Cave",
        category: 11,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.68,
            decay: 0.55,
            damping: 0.78,
            spread: 0.65,
            mod_amount: 0.30,
            mod_rate: 0.12,
            diffusion: 0.70,
            bass_mult: 0.28,
            treble_mult: 0.75,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::single(0.0, 0.0, 0.0, 0.35, 0.0, 0.0),
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // FFDN CHARACTER (12) — FFDN — 5 presets
    // ======================================================================
    FactoryPreset {
        name: "FFDN Saturated Hall",
        category: 12,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.55,
            decay: 0.18,
            damping: 0.45,
            spread: 0.55,
            mod_amount: 0.20,
            mod_rate: 0.13,
            diffusion: 0.6,
            bass_mult: 0.55,
            treble_mult: 0.50,
            drive: 0.55,
            drive_enabled: true,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Tilted Hall",
        category: 12,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.55,
            decay: 0.15,
            damping: 0.50,
            spread: 0.55,
            mod_amount: 0.20,
            mod_rate: 0.13,
            diffusion: 0.6,
            bass_mult: 0.50,
            treble_mult: 0.50,
            tilt_freq: 0.55,
            tilt_amount: 0.68,
            tilt_enabled: true,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Warm Cross",
        category: 12,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.50,
            decay: 0.16,
            damping: 0.45,
            spread: 0.60,
            mod_amount: 0.25,
            mod_rate: 0.12,
            diffusion: 0.6,
            bass_mult: 0.58,
            treble_mult: 0.48,
            cross: 0.15,
            drive: 0.30,
            drive_enabled: true,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Analog Plate",
        category: 12,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.42,
            decay: 0.13,
            damping: 0.58,
            spread: 0.80,
            mod_amount: 0.18,
            mod_rate: 0.10,
            diffusion: 0.75,
            bass_mult: 0.42,
            treble_mult: 0.55,
            velvet_matrix: true,
            drive: 0.45,
            drive_enabled: true,
            tilt_freq: 0.55,
            tilt_amount: 0.55,
            tilt_enabled: true,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Vintage Chamber",
        category: 12,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.38,
            decay: 0.10,
            damping: 0.52,
            spread: 0.60,
            mod_amount: 0.22,
            mod_rate: 0.14,
            diffusion: 0.55,
            bass_mult: 0.55,
            treble_mult: 0.48,
            drive: 0.35,
            drive_enabled: true,
            tilt_freq: 0.48,
            tilt_amount: 0.44,
            tilt_enabled: true,
            lines: 10,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // FFDN DYNAMIC (13) — FFDN — 4 presets
    // ======================================================================
    FactoryPreset {
        name: "FFDN Soft Attack Hall",
        category: 13,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.55,
            decay: 0.18,
            damping: 0.48,
            spread: 0.55,
            mod_amount: 0.20,
            mod_rate: 0.13,
            diffusion: 0.6,
            bass_mult: 0.55,
            treble_mult: 0.50,
            attack: 0.55,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Ducked Vocal",
        category: 13,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.45,
            decay: 0.16,
            damping: 0.55,
            spread: 0.6,
            mod_amount: 0.18,
            mod_rate: 0.12,
            diffusion: 0.6,
            bass_mult: 0.5,
            treble_mult: 0.55,
            ducking: 0.70,
            lines: 10,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Gated Space",
        category: 13,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.60,
            decay: 0.30,
            damping: 0.55,
            spread: 0.55,
            mod_amount: 0.22,
            mod_rate: 0.13,
            diffusion: 0.65,
            bass_mult: 0.52,
            treble_mult: 0.52,
            gate: 0.65,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "FFDN Pop Pad",
        category: 13,
        engine: FactoryEngine::Ffdn,
        values: NO_CLOUDSEED,
        ffdn: FfdnConfig {
            size: 0.50,
            decay: 0.18,
            damping: 0.50,
            spread: 0.60,
            mod_amount: 0.25,
            mod_rate: 0.12,
            diffusion: 0.6,
            bass_mult: 0.55,
            treble_mult: 0.50,
            attack: 0.45,
            ducking: 0.40,
            lines: 12,
            ..NO_FFDN
        },
        texture: TextureConfig::NONE,
        illusion: IllusionConfig::OFF,
    },
    // ======================================================================
    // ILLUSION (14) — ILLUSION + Texture — 6 presets
    // ======================================================================
    FactoryPreset {
        name: "Illusion Dream",
        category: 14,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.15, 0.060, 0.05, 0.72, 1.0, 0.45, 0.62, 0.72, 0.80, 1.0, 0.8571, 0.60, 0.78, 1.0,
            0.7500, 0.68, 0.66, 1.0, 0.8571, 0.62, 0.78, 0.85, 0.30, 0.78, 0.58, 0.80, 0.42, 0.50,
            0.68, 0.48, 0.68, 0.50, 0.000320, 0.000200, 0.000260, 0.000420, 0.50, 0.62, 0.00, 0.72,
            0.82, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig {
            spark: 0.55,
            still: 0.65,
            aura_norm: 0.5, // 0 st (internal sequence)
            veil: 0.5,
            wide: 0.55,
            tint: 0.45,
            rate: 0.4,
            grain: 0.55,
        },
    },
    FactoryPreset {
        name: "Illusion Cascade",
        category: 14,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.20, 0.045, 0.08, 0.68, 1.0, 0.50, 0.62, 0.72, 0.78, 1.0, 0.8571, 0.62, 0.80, 1.0,
            0.8333, 0.72, 0.68, 1.0, 0.8571, 0.65, 0.78, 0.82, 0.32, 0.72, 0.60, 0.78, 0.40, 0.48,
            0.65, 0.48, 0.68, 0.50, 0.000300, 0.000180, 0.000240, 0.000400, 0.50, 0.60, 0.00, 0.75,
            0.85, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig {
            spark: 0.65,
            still: 0.35,
            aura_norm: 0.5,
            veil: 0.55,
            wide: 0.65,
            tint: 0.55,
            rate: 0.65,
            grain: 0.40,
        },
    },
    FactoryPreset {
        name: "Illusion Cathedral",
        category: 14,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.05, 0.075, 0.10, 0.60, 1.0, 0.48, 0.70, 0.72, 0.88, 1.0, 1.0000, 0.70, 0.85, 1.0,
            0.9167, 0.78, 0.65, 1.0, 0.8571, 0.68, 0.80, 0.75, 0.42, 0.80, 0.68, 0.75, 0.35, 0.45,
            0.72, 0.48, 0.70, 0.48, 0.000280, 0.000180, 0.000240, 0.000400, 0.50, 0.72, 0.00, 0.68,
            0.82, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::NONE,
        illusion: IllusionConfig {
            spark: 0.45,
            still: 0.85,
            aura_norm: 0.4, // -4.8 st
            veil: 0.60,
            wide: 0.60,
            tint: 0.35,
            rate: 0.35,
            grain: 0.70,
        },
    },
    FactoryPreset {
        name: "Illusion Bloom",
        category: 14,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.18, 0.070, 0.06, 0.70, 1.0, 0.45, 0.62, 0.72, 0.80, 1.0, 0.8571, 0.60, 0.78, 1.0,
            0.7500, 0.68, 0.66, 1.0, 0.8571, 0.62, 0.78, 0.85, 0.30, 0.78, 0.58, 0.80, 0.42, 0.50,
            0.68, 0.48, 0.68, 0.50, 0.000320, 0.000200, 0.000260, 0.000420, 0.50, 0.62, 0.00, 0.72,
            0.82, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.0, 0.0, 0.0, 0.0, 0.0, 0.40),
        illusion: IllusionConfig {
            spark: 0.50,
            still: 0.70,
            aura_norm: 0.58, // +3.8 st
            veil: 0.45,
            wide: 0.60,
            tint: 0.52,
            rate: 0.45,
            grain: 0.60,
        },
    },
    FactoryPreset {
        name: "Illusion Crystal",
        category: 14,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.22, 0.065, 0.08, 0.85, 1.0, 0.55, 0.75, 0.72, 0.78, 1.0, 0.8571, 0.72, 0.85, 1.0,
            0.9167, 0.78, 0.70, 1.0, 0.8571, 0.75, 0.80, 0.90, 0.24, 0.68, 0.55, 0.85, 0.42, 0.52,
            0.72, 0.52, 0.72, 0.55, 0.000280, 0.000160, 0.000220, 0.000380, 0.50, 0.60, 0.00, 0.78,
            0.85, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.0, 0.0, 0.0, 0.45, 0.25, 0.0),
        illusion: IllusionConfig {
            spark: 0.60,
            still: 0.45,
            aura_norm: 0.60, // +4.8 st
            veil: 0.40,
            wide: 0.70,
            tint: 0.68,
            rate: 0.55,
            grain: 0.35,
        },
    },
    FactoryPreset {
        name: "Illusion Haze",
        category: 14,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.12, 0.080, 0.10, 0.55, 1.0, 0.42, 0.58, 0.72, 0.82, 1.0, 0.7143, 0.55, 0.75, 1.0,
            0.7500, 0.65, 0.68, 1.0, 0.8571, 0.62, 0.75, 0.72, 0.38, 0.85, 0.65, 0.72, 0.35, 0.45,
            0.62, 0.45, 0.65, 0.48, 0.000350, 0.000220, 0.000280, 0.000480, 0.50, 0.70, 0.00, 0.72,
            0.80, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.0, 0.50, 0.0, 0.0, 0.0, 0.25),
        illusion: IllusionConfig {
            spark: 0.40,
            still: 0.75,
            aura_norm: 0.5,
            veil: 0.55,
            wide: 0.55,
            tint: 0.35,
            rate: 0.30,
            grain: 0.65,
        },
    },
    // ======================================================================
    // SHIMMER (15) — Texture Rack + CloudSeed — 5 presets
    // ======================================================================
    FactoryPreset {
        name: "Shimmer Deep Hall",
        category: 15,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.15, 0.060, 0.05, 0.72, 1.0, 0.45, 0.62, 0.72, 0.80, 1.0, 0.8571, 0.60, 0.78, 1.0,
            0.7500, 0.68, 0.66, 1.0, 0.8571, 0.62, 0.78, 0.85, 0.30, 0.78, 0.58, 0.80, 0.42, 0.50,
            0.68, 0.48, 0.68, 0.50, 0.000320, 0.000200, 0.000260, 0.000420, 0.50, 0.62, 0.00, 0.72,
            0.82, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.60, 0.0, 0.0, 0.0, 0.0, 0.20),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Shimmer Rising",
        category: 15,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.15, 0.060, 0.05, 0.72, 1.0, 0.45, 0.62, 0.72, 0.80, 1.0, 0.8571, 0.60, 0.78, 1.0,
            0.7500, 0.68, 0.66, 1.0, 0.8571, 0.62, 0.78, 0.85, 0.30, 0.78, 0.58, 0.80, 0.42, 0.50,
            0.68, 0.48, 0.68, 0.50, 0.000320, 0.000200, 0.000260, 0.000420, 0.50, 0.62, 0.00, 0.72,
            0.82, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.55, 0.0, 0.30, 0.0, 0.0, 0.45),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Shimmer Cathedral",
        category: 15,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.05, 0.075, 0.10, 0.60, 1.0, 0.48, 0.70, 0.72, 0.88, 1.0, 1.0000, 0.70, 0.85, 1.0,
            0.9167, 0.78, 0.65, 1.0, 0.8571, 0.68, 0.80, 0.75, 0.42, 0.80, 0.68, 0.75, 0.35, 0.45,
            0.72, 0.48, 0.70, 0.48, 0.000280, 0.000180, 0.000240, 0.000400, 0.50, 0.72, 0.00, 0.68,
            0.82, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.55, 0.0, 0.0, 0.0, 0.0, 0.35),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Shimmer Void",
        category: 15,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.05, 0.120, 0.10, 0.35, 1.0, 0.62, 0.85, 0.78, 0.95, 1.0, 1.0000, 0.82, 0.92, 1.0,
            1.0000, 0.75, 0.82, 1.0, 1.0000, 0.72, 0.88, 0.55, 0.55, 0.92, 0.78, 0.52, 0.20, 0.30,
            0.55, 0.35, 0.60, 0.35, 0.000400, 0.000250, 0.000320, 0.000520, 0.50, 0.72, 0.00, 0.55,
            0.78, 0.75, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.55, 0.0, 0.0, 0.0, 0.35, 0.20),
        illusion: IllusionConfig::OFF,
    },
    FactoryPreset {
        name: "Shimmer Mist",
        category: 15,
        engine: FactoryEngine::CloudSeed,
        values: [
            0.12, 0.065, 0.08, 0.65, 1.0, 0.45, 0.62, 0.72, 0.82, 1.0, 0.8571, 0.55, 0.75, 1.0,
            0.7500, 0.68, 0.68, 1.0, 0.8571, 0.60, 0.75, 0.82, 0.35, 0.78, 0.62, 0.75, 0.38, 0.45,
            0.65, 0.45, 0.65, 0.48, 0.000320, 0.000200, 0.000260, 0.000420, 0.50, 0.68, 0.00, 0.72,
            0.80, 0.50, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        ffdn: NO_FFDN,
        texture: TextureConfig::single(0.50, 0.45, 0.0, 0.0, 0.0, 0.30),
        illusion: IllusionConfig::OFF,
    },
];

// ============================================================================
// Helpers
// ============================================================================

pub fn find_by_name(name: &str) -> Option<&'static FactoryPreset> {
    FACTORY_PRESETS.iter().find(|p| p.name == name)
}

pub fn presets_in_category(cat: usize) -> Vec<&'static FactoryPreset> {
    FACTORY_PRESETS
        .iter()
        .filter(|p| p.category == cat)
        .collect()
}

pub fn category_for(idx: usize) -> usize {
    FACTORY_PRESETS.get(idx).map(|p| p.category).unwrap_or(0)
}
