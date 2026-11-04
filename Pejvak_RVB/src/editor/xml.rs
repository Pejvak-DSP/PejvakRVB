//! XML parsing, saving, and state snapshotting for presets.

use super::EditorPage;
use crate::PejvakRvbParams;
use nice_plug::prelude::{BoolParam, FloatParam, ParamSetter};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub const UNDO_DEPTH: usize = 50;
pub const UNDO_SUPPRESS_S: f64 = 0.25;

#[derive(Clone, PartialEq)]
pub struct UndoSnapshot {
    pub cloudseed: [f32; 49],
    pub texture: [f32; 32],
    pub texture_enabled: [bool; 6],
    pub ffdn: [f32; 39],
    pub illusion: [f32; 8],
    pub illusion_enabled: bool,
    pub eq_enabled: bool,
    pub bypass: bool,
    pub freeze: bool,
    pub interp: bool,
    pub texture_order: u64,
    pub preset_name: String,
    pub page: EditorPage,
}

pub fn capture_undo_snapshot(
    p: &PejvakRvbParams,
    preset_name: &str,
    page: EditorPage,
) -> UndoSnapshot {
    let mut cloudseed = [0.0f32; 49];
    for (i, slot) in cloudseed.iter_mut().enumerate() {
        *slot = p.normalized(i) as f32;
    }

    let texture: [f32; 32] = [
        p.shimmer.mix.value(),
        p.shimmer.pitch.value(),
        p.shimmer.feedback.value(),
        p.shimmer.low_cut.value(),
        p.shimmer.high_cut.value(),
        p.mist.mix.value(),
        p.mist.hp.value(),
        p.mist.feedback.value(),
        p.mist.delay.value(),
        p.mist.rate.value(),
        p.drift.mix.value(),
        p.drift.feedback.value(),
        p.drift.delay.value(),
        p.drift.rate.value(),
        p.drift.depth.value(),
        p.crystal.mix.value(),
        p.crystal.size.value(),
        p.crystal.density.value(),
        p.crystal.feedback.value(),
        p.scatter.mix.value(),
        p.scatter.spread.value(),
        p.scatter.density.value(),
        p.scatter.speed.value(),
        p.scatter.feedback.value(),
        p.bloom.mix.value(),
        p.bloom.rise.value(),
        p.bloom.size.value(),
        p.bloom.feedback.value(),
        p.bloom.tone.value(),
        p.velvet.diffusion.value(),
        p.velvet.density.value(),
        p.velvet.tone.value(),
    ];
    let texture_enabled: [bool; 6] = [
        p.shimmer.enabled.value(),
        p.mist.enabled.value(),
        p.drift.enabled.value(),
        p.crystal.enabled.value(),
        p.scatter.enabled.value(),
        p.bloom.enabled.value(),
    ];

    let ffdn: [f32; 39] = [
        if p.ffdn.use_ffdn.value() { 1.0 } else { 0.0 },
        p.ffdn.size.value(),
        p.ffdn.decay.value(),
        p.ffdn.damping.value(),
        p.ffdn.spread.value(),
        p.ffdn.mod_amount.value(),
        p.ffdn.mod_rate.value(),
        p.ffdn.matrix_type.value(),
        p.ffdn.bass_mult.value(),
        p.ffdn.bass_freq.value(),
        p.ffdn.treble_mult.value(),
        p.ffdn.treble_freq.value(),
        p.ffdn.diffusion.value(),
        p.ffdn.cross.value(),
        p.ffdn.drive.value(),
        p.ffdn.tilt_freq.value(),
        p.ffdn.tilt_amount.value(),
        p.ffdn.er_level.value(),
        if p.ffdn.drive_enabled.value() {
            1.0
        } else {
            0.0
        },
        if p.ffdn.tilt_enabled.value() {
            1.0
        } else {
            0.0
        },
        if p.ffdn.er_enabled.value() { 1.0 } else { 0.0 },
        p.ffdn.lines.value(),
        p.ffdn.density.value(),
        p.ffdn.decorr.value(),
        p.ffdn.loop_hp.value(),
        p.ffdn.loop_lp.value(),
        p.ffdn.mid_focus.value(),
        p.ffdn.attack.value(),
        p.ffdn.ducking.value(),
        p.ffdn.gate.value(),
        if p.ffdn.space_enabled.value() {
            1.0
        } else {
            0.0
        },
        if p.ffdn.network_enabled.value() {
            1.0
        } else {
            0.0
        },
        if p.ffdn.character_enabled.value() {
            1.0
        } else {
            0.0
        },
        if p.ffdn.shelves_enabled.value() {
            1.0
        } else {
            0.0
        },
        if p.ffdn.modulation_enabled.value() {
            1.0
        } else {
            0.0
        },
        if p.ffdn.filter_enabled.value() {
            1.0
        } else {
            0.0
        },
        if p.ffdn.dynamics_enabled.value() {
            1.0
        } else {
            0.0
        },
        p.ffdn.matrix_mod.value(),
        p.ffdn.matrix_rate.value(),
    ];

    let illusion: [f32; 8] = [
        p.illusion.spark.value(),
        p.illusion.still.value(),
        p.illusion.aura.value(),
        p.illusion.veil.value(),
        p.illusion.wide.value(),
        p.illusion.tint.value(),
        p.illusion.rate.value(),
        p.illusion.grain.value(),
    ];

    UndoSnapshot {
        cloudseed,
        texture,
        texture_enabled,
        ffdn,
        illusion,
        illusion_enabled: p.illusion.enabled.value(),
        eq_enabled: p.eq.enabled.value(),
        bypass: p.global.bypass.value(),
        freeze: p.global.freeze.value(),
        interp: p.switches.interpolation.value(),
        texture_order: p.texture_order.load(std::sync::atomic::Ordering::Relaxed),
        preset_name: preset_name.to_string(),
        page,
    }
}

pub fn apply_undo_snapshot(
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    snap: &UndoSnapshot,
) -> String {
    for i in 0..49 {
        set_one(p, s, i, snap.cloudseed[i]);
    }

    let mut tex_opts: [Option<f32>; 32] = [None; 32];
    for (i, &v) in snap.texture.iter().enumerate() {
        tex_opts[i] = Some(v);
    }
    let mut enabled_opts: [Option<bool>; 6] = [None; 6];
    for (i, &b) in snap.texture_enabled.iter().enumerate() {
        enabled_opts[i] = Some(b);
    }
    apply_texture_params(p, s, &tex_opts, &enabled_opts);

    let mut ffdn_opts: [Option<f32>; 39] = [None; 39];
    for (slot, &v) in ffdn_opts.iter_mut().zip(snap.ffdn.iter()) {
        *slot = Some(v);
    }
    apply_ffdn_params(p, s, &ffdn_opts);

    let mut ill_opts: [Option<f32>; 8] = [None; 8];
    for (i, &v) in snap.illusion.iter().enumerate() {
        ill_opts[i] = Some(v);
    }
    apply_illusion_params(p, s, &ill_opts, Some(snap.illusion_enabled), true);

    apply_eq_master(p, s, snap.eq_enabled);

    macro_rules! push_bool {
        ($param:expr, $val:expr) => {{
            s.begin_set_parameter(&$param);
            s.set_parameter(&$param, $val);
            s.end_set_parameter(&$param);
        }};
    }
    push_bool!(p.global.bypass, snap.bypass);
    push_bool!(p.global.freeze, snap.freeze);
    push_bool!(p.switches.interpolation, snap.interp);

    p.texture_order
        .store(snap.texture_order, std::sync::atomic::Ordering::Relaxed);
    snap.preset_name.clone()
}

pub fn snapshots_differ(a: &UndoSnapshot, b: &UndoSnapshot, eps: f32) -> bool {
    for i in 0..49 {
        if (a.cloudseed[i] - b.cloudseed[i]).abs() > eps {
            return true;
        }
    }
    for i in 0..32 {
        if (a.texture[i] - b.texture[i]).abs() > eps {
            return true;
        }
    }
    if a.texture_enabled != b.texture_enabled {
        return true;
    }
    // Skip index 0 (use_ffdn): the engine selector is a context
    // switch, not a parameter edit — it must not create an undo
    // step on its own. It is still captured in the snapshot, so
    // it is restored together with genuine parameter changes.
    for i in 1..39 {
        if (a.ffdn[i] - b.ffdn[i]).abs() > eps {
            return true;
        }
    }
    for i in 0..8 {
        if (a.illusion[i] - b.illusion[i]).abs() > eps {
            return true;
        }
    }
    a.illusion_enabled != b.illusion_enabled
        || a.eq_enabled != b.eq_enabled
        || a.bypass != b.bypass
        || a.freeze != b.freeze
        || a.interp != b.interp
        || a.texture_order != b.texture_order
        || a.preset_name != b.preset_name
}

pub fn summarize_diff(a: &UndoSnapshot, b: &UndoSnapshot) -> String {
    let mut parts: Vec<String> = Vec::new();
    if (a.ffdn[0] - b.ffdn[0]).abs() > 0.5 {
        parts.push(format!(
            "Engine → {}",
            if b.ffdn[0] > 0.5 { "FFDN" } else { "Cloud" }
        ));
    }
    for i in 0..49 {
        if (a.cloudseed[i] - b.cloudseed[i]).abs() > 1.0e-3 {
            if let Some(p) = fdn_reverb_core::cloudseed::Parameter::from_index(i) {
                parts.push(p.name().to_string());
            }
        }
    }
    for (i, &name) in FFDN_PARAM_NAMES.iter().enumerate().skip(1) {
        if (a.ffdn[i] - b.ffdn[i]).abs() > 1.0e-3 {
            parts.push(name.to_string());
        }
    }
    for (i, &name) in ILLUSION_PARAM_NAMES.iter().enumerate() {
        if (a.illusion[i] - b.illusion[i]).abs() > 1.0e-3 {
            parts.push(name.to_string());
        }
    }
    for (i, &name) in TEXTURE_PARAM_NAMES.iter().enumerate() {
        if (a.texture[i] - b.texture[i]).abs() > 1.0e-3 {
            parts.push(name.to_string());
        }
    }
    if a.bypass != b.bypass {
        parts.push("Bypass".to_string());
    }
    if a.freeze != b.freeze {
        parts.push("Freeze".to_string());
    }
    if a.interp != b.interp {
        parts.push("Interp".to_string());
    }
    if a.eq_enabled != b.eq_enabled {
        parts.push("EQ".to_string());
    }
    if a.illusion_enabled != b.illusion_enabled {
        parts.push("ILLUSION".to_string());
    }
    if a.texture_enabled != b.texture_enabled {
        parts.push("Texture switches".to_string());
    }
    if a.texture_order != b.texture_order {
        parts.push("Signal Chain".to_string());
    }
    const MAX: usize = 3;
    if parts.is_empty() {
        "no change".to_string()
    } else if parts.len() <= MAX {
        parts.join(", ")
    } else {
        format!("{}, +{} more", parts[..MAX].join(", "), parts.len() - MAX)
    }
}

#[derive(Default)]
pub struct PresetDialogState {
    pub open: bool,
    pub save_path: Option<PathBuf>,
    pub load_path: Option<PathBuf>,
}

static PRESET_DIALOG: OnceLock<Mutex<PresetDialogState>> = OnceLock::new();

pub fn pd_lock() -> std::sync::MutexGuard<'static, PresetDialogState> {
    PRESET_DIALOG
        .get_or_init(|| Mutex::new(PresetDialogState::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn spawn_save_dialog() {
    if pd_lock().open {
        return;
    }
    pd_lock().open = true;
    std::thread::spawn(|| {
        let dir = user_presets_dir();
        let mut dlg = rfd::FileDialog::new()
            .set_title("Save PEJVAK Preset")
            .set_file_name("MyPreset.xml")
            .add_filter("PEJVAK Preset", &["xml"]);
        if let Some(dir) = dir {
            dlg = dlg.set_directory(&dir);
        }
        let mut st = pd_lock();
        st.save_path = dlg.save_file();
        st.open = false;
    });
}

pub fn spawn_load_dialog() {
    if pd_lock().open {
        return;
    }
    pd_lock().open = true;
    std::thread::spawn(|| {
        let dir = user_presets_dir();
        let mut dlg = rfd::FileDialog::new()
            .set_title("Load PEJVAK Preset")
            .add_filter("PEJVAK Preset", &["xml"]);
        if let Some(dir) = dir {
            dlg = dlg.set_directory(&dir);
        }
        let mut st = pd_lock();
        st.load_path = dlg.pick_file();
        st.open = false;
    });
}

pub fn user_presets_dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var("APPDATA").ok()
    } else {
        std::env::var("HOME").ok()
    };
    let mut dir = PathBuf::from(base?);
    dir.push("PEJVAK RVB");
    dir.push("Presets");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

pub fn user_preset_names() -> Vec<String> {
    let dir = match user_presets_dir() {
        Some(d) => d,
        None => return Vec::new(),
    };
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|x| x.to_str()) == Some("xml") {
                if let Some(stem) = p.file_stem().and_then(|x| x.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
    }
    names.sort();
    names
}

pub const TEXTURE_PARAM_NAMES: [&str; 32] = [
    "ShimmerMix",
    "ShimmerPitch",
    "ShimmerFeedback",
    "ShimmerLowCut",
    "ShimmerHighCut",
    "MistMix",
    "MistHP",
    "MistFeedback",
    "MistDelay",
    "MistRate",
    "DriftMix",
    "DriftFeedback",
    "DriftDelay",
    "DriftRate",
    "DriftDepth",
    "CrystalMix",
    "CrystalSize",
    "CrystalDensity",
    "CrystalFeedback",
    "ScatterMix",
    "ScatterSpread",
    "ScatterDensity",
    "ScatterSpeed",
    "ScatterFeedback",
    "BloomMix",
    "BloomRise",
    "BloomSize",
    "BloomFeedback",
    "BloomTone",
    "VelvetDiffusion",
    "VelvetDensity",
    "VelvetTone",
];

pub const TEXTURE_ENABLED_NAMES: [&str; 6] = [
    "ShimmerEnabled",
    "MistEnabled",
    "DriftEnabled",
    "CrystalEnabled",
    "ScatterEnabled",
    "BloomEnabled",
];
pub const ILLUSION_PARAM_NAMES: [&str; 8] = [
    "IllusionSpark",
    "IllusionStill",
    "IllusionAura",
    "IllusionVeil",
    "IllusionWide",
    "IllusionTint",
    "IllusionRate",
    "IllusionGrain",
];
pub const ILLUSION_ENABLED_NAME: &str = "IllusionEnabled";

pub const FFDN_PARAM_NAMES: [&str; 39] = [
    "UseFfdn",
    "FfdnSize",
    "FfdnDecay",
    "FfdnDamping",
    "FfdnSpread",
    "FfdnModAmt",
    "FfdnModRate",
    "FfdnMatrix",
    "FfdnBassMult",
    "FfdnBassFreq",
    "FfdnTrebleMult",
    "FfdnTrebleFreq",
    "FfdnDiffusion",
    "FfdnCross",
    "FfdnDrive",
    "FfdnTiltFreq",
    "FfdnTiltAmt",
    "FfdnErLevel",
    "FfdnDriveEnabled",
    "FfdnTiltEnabled",
    "FfdnErEnabled",
    "FfdnLines",
    "FfdnDensity",
    "FfdnDecorr",
    "FfdnLoopHP",
    "FfdnLoopLP",
    "FfdnMidFocus",
    "FfdnAttack",
    "FfdnDucking",
    "FfdnGate",
    "FfdnSpaceEnabled",
    "FfdnNetworkEnabled",
    "FfdnCharacterEnabled",
    "FfdnShelvesEnabled",
    "FfdnModulationEnabled",
    "FfdnFilterEnabled",
    "FfdnDynamicsEnabled",
    "FfdnMatrixMod",
    "FfdnMatrixRate",
];

fn texture_param_value(p: &PejvakRvbParams, name: &str) -> f32 {
    match name {
        "ShimmerMix" => p.shimmer.mix.value(),
        "ShimmerPitch" => p.shimmer.pitch.value(),
        "ShimmerFeedback" => p.shimmer.feedback.value(),
        "ShimmerLowCut" => p.shimmer.low_cut.value(),
        "ShimmerHighCut" => p.shimmer.high_cut.value(),
        "MistMix" => p.mist.mix.value(),
        "MistHP" => p.mist.hp.value(),
        "MistFeedback" => p.mist.feedback.value(),
        "MistDelay" => p.mist.delay.value(),
        "MistRate" => p.mist.rate.value(),
        "DriftMix" => p.drift.mix.value(),
        "DriftFeedback" => p.drift.feedback.value(),
        "DriftDelay" => p.drift.delay.value(),
        "DriftRate" => p.drift.rate.value(),
        "DriftDepth" => p.drift.depth.value(),
        "CrystalMix" => p.crystal.mix.value(),
        "CrystalSize" => p.crystal.size.value(),
        "CrystalDensity" => p.crystal.density.value(),
        "CrystalFeedback" => p.crystal.feedback.value(),
        "ScatterMix" => p.scatter.mix.value(),
        "ScatterSpread" => p.scatter.spread.value(),
        "ScatterDensity" => p.scatter.density.value(),
        "ScatterSpeed" => p.scatter.speed.value(),
        "ScatterFeedback" => p.scatter.feedback.value(),
        "BloomMix" => p.bloom.mix.value(),
        "BloomRise" => p.bloom.rise.value(),
        "BloomSize" => p.bloom.size.value(),
        "BloomFeedback" => p.bloom.feedback.value(),
        "BloomTone" => p.bloom.tone.value(),
        "VelvetDiffusion" => p.velvet.diffusion.value(),
        "VelvetDensity" => p.velvet.density.value(),
        "VelvetTone" => p.velvet.tone.value(),
        _ => 0.0,
    }
}

fn texture_enabled_value(p: &PejvakRvbParams, name: &str) -> bool {
    match name {
        "ShimmerEnabled" => p.shimmer.enabled.value(),
        "MistEnabled" => p.mist.enabled.value(),
        "DriftEnabled" => p.drift.enabled.value(),
        "CrystalEnabled" => p.crystal.enabled.value(),
        "ScatterEnabled" => p.scatter.enabled.value(),
        "BloomEnabled" => p.bloom.enabled.value(),
        _ => false,
    }
}

fn ffdn_param_value(p: &PejvakRvbParams, name: &str) -> f32 {
    match name {
        "UseFfdn" => {
            if p.ffdn.use_ffdn.value() {
                1.0
            } else {
                0.0
            }
        }
        "FfdnSize" => p.ffdn.size.value(),
        "FfdnDecay" => p.ffdn.decay.value(),
        "FfdnDamping" => p.ffdn.damping.value(),
        "FfdnSpread" => p.ffdn.spread.value(),
        "FfdnModAmt" => p.ffdn.mod_amount.value(),
        "FfdnModRate" => p.ffdn.mod_rate.value(),
        "FfdnMatrix" => p.ffdn.matrix_type.value(),
        "FfdnBassMult" => p.ffdn.bass_mult.value(),
        "FfdnBassFreq" => p.ffdn.bass_freq.value(),
        "FfdnTrebleMult" => p.ffdn.treble_mult.value(),
        "FfdnTrebleFreq" => p.ffdn.treble_freq.value(),
        "FfdnDiffusion" => p.ffdn.diffusion.value(),
        "FfdnCross" => p.ffdn.cross.value(),
        "FfdnDrive" => p.ffdn.drive.value(),
        "FfdnTiltFreq" => p.ffdn.tilt_freq.value(),
        "FfdnTiltAmt" => p.ffdn.tilt_amount.value(),
        "FfdnErLevel" => p.ffdn.er_level.value(),
        "FfdnDriveEnabled" => {
            if p.ffdn.drive_enabled.value() {
                1.0
            } else {
                0.0
            }
        }
        "FfdnTiltEnabled" => {
            if p.ffdn.tilt_enabled.value() {
                1.0
            } else {
                0.0
            }
        }
        "FfdnErEnabled" => {
            if p.ffdn.er_enabled.value() {
                1.0
            } else {
                0.0
            }
        }
        "FfdnLines" => p.ffdn.lines.value(),
        "FfdnDensity" => p.ffdn.density.value(),
        "FfdnDecorr" => p.ffdn.decorr.value(),
        "FfdnLoopHP" => p.ffdn.loop_hp.value(),
        "FfdnLoopLP" => p.ffdn.loop_lp.value(),
        "FfdnMidFocus" => p.ffdn.mid_focus.value(),
        "FfdnAttack" => p.ffdn.attack.value(),
        "FfdnDucking" => p.ffdn.ducking.value(),
        "FfdnGate" => p.ffdn.gate.value(),
        "FfdnSpaceEnabled" => {
            if p.ffdn.space_enabled.value() {
                1.0
            } else {
                0.0
            }
        }
        "FfdnNetworkEnabled" => {
            if p.ffdn.network_enabled.value() {
                1.0
            } else {
                0.0
            }
        }
        "FfdnCharacterEnabled" => {
            if p.ffdn.character_enabled.value() {
                1.0
            } else {
                0.0
            }
        }
        "FfdnShelvesEnabled" => {
            if p.ffdn.shelves_enabled.value() {
                1.0
            } else {
                0.0
            }
        }
        "FfdnModulationEnabled" => {
            if p.ffdn.modulation_enabled.value() {
                1.0
            } else {
                0.0
            }
        }
        "FfdnFilterEnabled" => {
            if p.ffdn.filter_enabled.value() {
                1.0
            } else {
                0.0
            }
        }
        "FfdnDynamicsEnabled" => {
            if p.ffdn.dynamics_enabled.value() {
                1.0
            } else {
                0.0
            }
        }
        "FfdnMatrixMod" => p.ffdn.matrix_mod.value(),
        "FfdnMatrixRate" => p.ffdn.matrix_rate.value(),
        _ => 0.0,
    }
}

pub fn params_to_xml(p: &PejvakRvbParams) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CloudReverb>\n");
    for i in 0..49 {
        let name = fdn_reverb_core::cloudseed::Parameter::from_index(i)
            .map(|x| x.name())
            .unwrap_or("Unknown");
        s.push_str(&format!(
            "  <PARAM id=\"{}\" value=\"{:.6}\"/>\n",
            name,
            fdn_reverb_core::cloudseed::scaled_parameter_value(i, p.normalized(i))
        ));
    }
    s.push_str(&format!(
        "  <PARAM id=\"EqEnabled\" value=\"{:.6}\"/>\n",
        if p.eq.enabled.value() { 1.0 } else { 0.0 }
    ));

    s.push_str("  <FFDN>\n");
    for name in FFDN_PARAM_NAMES.iter() {
        s.push_str(&format!(
            "    <PARAM id=\"{}\" value=\"{:.6}\"/>\n",
            name,
            ffdn_param_value(p, name)
        ));
    }
    s.push_str("  </FFDN>\n");

    s.push_str("  <Illusion>\n");
    s.push_str(&format!(
        "    <PARAM id=\"{}\" value=\"{:.6}\"/>\n",
        ILLUSION_ENABLED_NAME,
        if p.illusion.enabled.value() { 1.0 } else { 0.0 }
    ));
    let illusion_vals: [f32; 8] = [
        p.illusion.spark.value(),
        p.illusion.still.value(),
        p.illusion.aura.value(),
        p.illusion.veil.value(),
        p.illusion.wide.value(),
        p.illusion.tint.value(),
        p.illusion.rate.value(),
        p.illusion.grain.value(),
    ];
    for (i, name) in ILLUSION_PARAM_NAMES.iter().enumerate() {
        s.push_str(&format!(
            "    <PARAM id=\"{}\" value=\"{:.6}\"/>\n",
            name, illusion_vals[i]
        ));
    }
    s.push_str("  </Illusion>\n");

    s.push_str("  <TextureRack>\n");
    let packed = p.texture_order.load(std::sync::atomic::Ordering::Relaxed);
    let ord = [
        (packed & 0xFF) as usize,
        ((packed >> 8) & 0xFF) as usize,
        ((packed >> 16) & 0xFF) as usize,
        ((packed >> 24) & 0xFF) as usize,
        ((packed >> 32) & 0xFF) as usize,
        ((packed >> 40) & 0xFF) as usize,
    ];
    s.push_str(&format!(
        "    <Order value=\"{},{},{},{},{},{}\"/>\n",
        ord[0], ord[1], ord[2], ord[3], ord[4], ord[5]
    ));
    for name in TEXTURE_ENABLED_NAMES.iter() {
        s.push_str(&format!(
            "    <PARAM id=\"{}\" value=\"{:.6}\"/>\n",
            name,
            if texture_enabled_value(p, name) {
                1.0
            } else {
                0.0
            }
        ));
    }
    for name in TEXTURE_PARAM_NAMES.iter() {
        s.push_str(&format!(
            "    <PARAM id=\"{}\" value=\"{:.6}\"/>\n",
            name,
            texture_param_value(p, name)
        ));
    }
    s.push_str("  </TextureRack>\n</CloudReverb>\n");
    s
}

pub struct ParsedPreset {
    pub cloudseed: [Option<f32>; 49],
    pub texture: [Option<f32>; 32],
    pub enabled: [Option<bool>; 6],
    pub texture_order: Option<[usize; 6]>,
    pub eq_enabled: Option<bool>,
    pub ffdn: [Option<f32>; 39],
    pub illusion: [Option<f32>; 8],
    pub illusion_enabled: Option<bool>,
    pub illusion_seen: bool,
}

pub fn for_each_param(content: &str, mut f: impl FnMut(&str, f32)) {
    let mut i = 0usize;
    while i < content.len() {
        let Some(lt_rel) = content[i..].find('<') else {
            break;
        };
        let s = i + lt_rel;
        let Some(gt_rel) = content[s..].find('>') else {
            break;
        };
        let gt = s + gt_rel;
        let tag = &content[s..=gt];
        if let (Some(id), Some(val_s)) = (attr(tag, "id"), attr(tag, "value")) {
            if let Ok(v) = val_s.parse::<f32>() {
                if v.is_finite() {
                    f(&id, v);
                }
            }
        }
        i = gt + 1;
    }
}

pub fn xml_to_params(path: &Path) -> Option<ParsedPreset> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut data = ParsedPreset {
        cloudseed: [None; 49],
        texture: [None; 32],
        enabled: [None; 6],
        texture_order: None,
        eq_enabled: None,
        ffdn: [None; 39],
        illusion: [None; 8],
        illusion_enabled: None,
        illusion_seen: false,
    };

    let texture_start = content.find("<TextureRack>");
    let texture_end = texture_start.and_then(|s| {
        content[s..]
            .find("</TextureRack>")
            .map(|e| s + e + "</TextureRack>".len())
    });
    let (main_section, texture_section) = match (texture_start, texture_end) {
        (Some(s), Some(e)) => (&content[..s], &content[s..e]),
        _ => (content.as_str(), ""),
    };
    let ffdn_section = match main_section.find("<FFDN>") {
        Some(s) => match main_section[s..].find("</FFDN>") {
            Some(e_rel) => &main_section[s..s + e_rel + 7],
            None => "",
        },
        None => "",
    };
    let illusion_section = match content.find("<Illusion>") {
        Some(s) => match content[s..].find("</Illusion>") {
            Some(e_rel) => &content[s..s + e_rel + 11],
            None => "",
        },
        None => "",
    };

    if !illusion_section.is_empty() {
        for_each_param(illusion_section, |id, v| {
            if id == ILLUSION_ENABLED_NAME {
                data.illusion_enabled = Some(v >= 0.5);
                data.illusion_seen = true;
                return;
            }
            if let Some(idx) = ILLUSION_PARAM_NAMES.iter().position(|&n| n == id) {
                data.illusion[idx] = Some(v.clamp(0.0, 1.0));
                data.illusion_seen = true;
            }
        });
    }

    let mut clean_main = main_section.to_string();
    if let (Some(s), Some(e)) = (clean_main.find("<FFDN>"), clean_main.find("</FFDN>")) {
        clean_main.replace_range(s..e + 7, "");
    }
    if let (Some(s), Some(e)) = (
        clean_main.find("<Illusion>"),
        clean_main.find("</Illusion>"),
    ) {
        clean_main.replace_range(s..e + 11, "");
    }
    for_each_param(&clean_main, |id, v| {
        if id == "EqEnabled" {
            data.eq_enabled = Some(v >= 0.5);
            return;
        }
        if id == "FfdnDiffusion" && data.ffdn[12].is_none() {
            data.ffdn[12] = Some(v.clamp(0.0, 1.0));
            return;
        }
        if let Some(p) = fdn_reverb_core::cloudseed::Parameter::from_name(id) {
            data.cloudseed[p.index()] = Some(denormalize(p.index(), v));
        }
    });

    if !ffdn_section.is_empty() {
        for_each_param(ffdn_section, |id, v| {
            if let Some(idx) = FFDN_PARAM_NAMES.iter().position(|&n| n == id) {
                data.ffdn[idx] = Some(v.clamp(0.0, 1.0));
            }
        });
    }

    for_each_param(texture_section, |id, v| {
        if let Some(idx) = TEXTURE_PARAM_NAMES.iter().position(|&n| n == id) {
            data.texture[idx] = Some(v.clamp(0.0, 1.0));
        } else if let Some(idx) = TEXTURE_ENABLED_NAMES.iter().position(|&n| n == id) {
            data.enabled[idx] = Some(v >= 0.5);
        }
    });

    data.texture_order = parse_texture_order(texture_section);

    let any_param_seen = data.cloudseed.iter().any(|x| x.is_some())
        || data.ffdn.iter().any(|x| x.is_some())
        || data.illusion_seen
        || data.eq_enabled.is_some()
        || data.texture_order.is_some()
        || data.enabled.iter().any(|x| x.is_some())
        || data.texture.iter().any(|x| x.is_some());

    if any_param_seen {
        Some(data)
    } else {
        None
    }
}

pub fn denormalize(index: usize, physical: f32) -> f32 {
    if is_switch(index) {
        return if physical >= 0.5 { 1.0 } else { 0.0 };
    }
    let target = physical as f64;
    let mut lo = 0.0f64;
    let mut hi = 1.0f64;
    let increasing = fdn_reverb_core::cloudseed::scaled_parameter_value(index, 1.0)
        >= fdn_reverb_core::cloudseed::scaled_parameter_value(index, 0.0);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if (fdn_reverb_core::cloudseed::scaled_parameter_value(index, mid) < target) == increasing {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let p_center = (0.5 * (lo + hi)) as f32;
    let step = match index {
        5 => Some(1.0 / 49.0),
        10 => Some(1.0 / 7.0),
        14 => Some(1.0 / 11.0),
        18 => Some(1.0 / 7.0),
        _ => None,
    };
    if let Some(s) = step {
        return (p_center / s).round() * s;
    }

    let v_center = fdn_reverb_core::cloudseed::scaled_parameter_value(index, p_center as f64);
    if v_center == target {
        return p_center;
    }

    let need_up = (v_center < target) == increasing;
    let first_sign: i64 = if need_up { 1 } else { -1 };
    let mut best = p_center;
    let mut best_dist = (v_center - target).abs();

    for sign in [first_sign, -first_sign] {
        let mut bits = p_center.to_bits() as i64;
        for _ in 0..64 {
            bits += sign;
            if bits < 0 || bits > 1.0f32.to_bits() as i64 {
                break;
            }
            let p = f32::from_bits(bits as u32);
            if !(0.0..=1.0).contains(&p) {
                break;
            }
            let v = fdn_reverb_core::cloudseed::scaled_parameter_value(index, p as f64);
            if v == target {
                return p;
            }
            let d = (v - target).abs();
            if d < best_dist {
                best = p;
                best_dist = d;
            }
        }
    }
    best
}

fn is_switch(index: usize) -> bool {
    matches!(index, 4 | 9 | 13 | 17 | 42 | 43 | 44 | 45 | 46 | 47 | 48)
}

pub fn attr(seg: &str, key: &str) -> Option<String> {
    let pat = format!("{}=\"", key);
    let mut search_from = 0;
    while let Some(pos) = seg[search_from..].find(&pat) {
        let abs_pos = search_from + pos;
        if abs_pos == 0 || seg.as_bytes()[abs_pos - 1].is_ascii_whitespace() {
            let s = abs_pos + pat.len();
            let e = seg[s..].find('"')? + s;
            return Some(seg[s..e].to_string());
        }
        search_from = abs_pos + 1;
    }
    None
}

pub fn parse_texture_order(content: &str) -> Option<[usize; 6]> {
    let start = content.find("<Order")?;
    let end_rel = content[start..].find("/>")?;
    let val = attr(&content[start..start + end_rel + 2], "value")?;
    let parts: Vec<&str> = val.split(',').collect();
    if parts.len() != 6 {
        return None;
    }
    let mut out = [0usize; 6];
    let mut seen = [false; 6];
    for (i, p) in parts.iter().enumerate() {
        let idx: usize = p.trim().parse().ok()?;
        if idx >= 6 || seen[idx] {
            return None;
        }
        seen[idx] = true;
        out[i] = idx;
    }
    Some(out)
}

pub fn apply_texture_order(p: &Arc<PejvakRvbParams>, order: [usize; 6]) {
    let mut packed: u64 = 0;
    for (i, &idx) in order.iter().enumerate() {
        packed |= (idx as u64) << (i * 8);
    }
    p.texture_order
        .store(packed, std::sync::atomic::Ordering::Relaxed);
}

pub fn apply_eq_master(p: &Arc<PejvakRvbParams>, s: &ParamSetter, enabled: bool) {
    s.begin_set_parameter(&p.eq.enabled);
    s.set_parameter(&p.eq.enabled, enabled);
    s.end_set_parameter(&p.eq.enabled);
}

pub fn apply_ffdn_params(p: &Arc<PejvakRvbParams>, s: &ParamSetter, params: &[Option<f32>; 39]) {
    if let Some(v) = params[0] {
        s.begin_set_parameter(&p.ffdn.use_ffdn);
        s.set_parameter(&p.ffdn.use_ffdn, v >= 0.5);
        s.end_set_parameter(&p.ffdn.use_ffdn);
    }
    macro_rules! set_opt {
        ($param:expr, $val:expr) => {{
            if let Some(v) = $val {
                s.begin_set_parameter(&$param);
                s.set_parameter_normalized(&$param, v.clamp(0.0, 1.0));
                s.end_set_parameter(&$param);
            }
        }};
    }
    macro_rules! set_opt_bool {
        ($param:expr, $val:expr) => {{
            if let Some(v) = $val {
                s.begin_set_parameter(&$param);
                s.set_parameter(&$param, v >= 0.5);
                s.end_set_parameter(&$param);
            }
        }};
    }

    set_opt!(p.ffdn.size, params[1]);
    set_opt!(p.ffdn.decay, params[2]);
    set_opt!(p.ffdn.damping, params[3]);
    set_opt!(p.ffdn.spread, params[4]);
    set_opt!(p.ffdn.mod_amount, params[5]);
    set_opt!(p.ffdn.mod_rate, params[6]);
    set_opt!(p.ffdn.matrix_type, params[7]);
    set_opt!(p.ffdn.bass_mult, params[8]);
    set_opt!(p.ffdn.bass_freq, params[9]);
    set_opt!(p.ffdn.treble_mult, params[10]);
    set_opt!(p.ffdn.treble_freq, params[11]);
    set_opt!(p.ffdn.diffusion, params[12]);
    set_opt!(p.ffdn.cross, params[13]);
    set_opt!(p.ffdn.drive, params[14]);
    set_opt!(p.ffdn.tilt_freq, params[15]);
    set_opt!(p.ffdn.tilt_amount, params[16]);
    set_opt!(p.ffdn.er_level, params[17]);
    set_opt_bool!(p.ffdn.drive_enabled, params[18]);
    set_opt_bool!(p.ffdn.tilt_enabled, params[19]);
    set_opt_bool!(p.ffdn.er_enabled, params[20]);
    set_opt!(p.ffdn.lines, params[21]);
    set_opt!(p.ffdn.density, params[22]);
    set_opt!(p.ffdn.decorr, params[23]);
    set_opt!(p.ffdn.loop_hp, params[24]);
    set_opt!(p.ffdn.loop_lp, params[25]);
    set_opt!(p.ffdn.mid_focus, params[26]);
    set_opt!(p.ffdn.attack, params[27]);
    set_opt!(p.ffdn.ducking, params[28]);
    set_opt!(p.ffdn.gate, params[29]);
    set_opt_bool!(p.ffdn.space_enabled, params[30]);
    set_opt_bool!(p.ffdn.network_enabled, params[31]);
    set_opt_bool!(p.ffdn.character_enabled, params[32]);
    set_opt_bool!(p.ffdn.shelves_enabled, params[33]);
    set_opt_bool!(p.ffdn.modulation_enabled, params[34]);
    set_opt_bool!(p.ffdn.filter_enabled, params[35]);
    set_opt_bool!(p.ffdn.dynamics_enabled, params[36]);
    set_opt!(p.ffdn.matrix_mod, params[37]);
    set_opt!(p.ffdn.matrix_rate, params[38]);
}

pub fn apply_illusion_params(
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    illusion: &[Option<f32>; 8],
    enabled: Option<bool>,
    seen: bool,
) {
    if !seen {
        return;
    }
    if let Some(on) = enabled {
        s.begin_set_parameter(&p.illusion.enabled);
        s.set_parameter(&p.illusion.enabled, on);
        s.end_set_parameter(&p.illusion.enabled);
    }
    macro_rules! set_opt_f {
        ($param:expr, $val:expr) => {{
            if let Some(v) = $val {
                s.begin_set_parameter(&$param);
                s.set_parameter_normalized(&$param, v.clamp(0.0, 1.0));
                s.end_set_parameter(&$param);
            }
        }};
    }
    set_opt_f!(p.illusion.spark, illusion[0]);
    set_opt_f!(p.illusion.still, illusion[1]);
    set_opt_f!(p.illusion.aura, illusion[2]);
    set_opt_f!(p.illusion.veil, illusion[3]);
    set_opt_f!(p.illusion.wide, illusion[4]);
    set_opt_f!(p.illusion.tint, illusion[5]);
    set_opt_f!(p.illusion.rate, illusion[6]);
    set_opt_f!(p.illusion.grain, illusion[7]);
}

pub fn apply_cloudseed_partial(
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    values: &[Option<f32>; 49],
) {
    p.reset_requested
        .store(true, std::sync::atomic::Ordering::Relaxed);
    for (i, v) in values.iter().enumerate() {
        if let Some(v) = v {
            set_one(p, s, i, *v);
        }
    }
}

pub fn apply_preset(p: &Arc<PejvakRvbParams>, s: &ParamSetter, values: &[f32; 49]) {
    p.reset_requested
        .store(true, std::sync::atomic::Ordering::Relaxed);
    for (i, &v) in values.iter().enumerate() {
        set_one(p, s, i, v);
    }
}

pub fn apply_texture_params(
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    texture: &[Option<f32>; 32],
    enabled: &[Option<bool>; 6],
) {
    macro_rules! set_b {
        ($param:expr, $val:expr) => {{
            if let Some(v) = $val {
                s.begin_set_parameter(&$param);
                s.set_parameter(&$param, v);
                s.end_set_parameter(&$param);
            }
        }};
    }
    macro_rules! set_f {
        ($param:expr, $val:expr) => {{
            if let Some(v) = $val {
                s.begin_set_parameter(&$param);
                s.set_parameter_normalized(&$param, v.clamp(0.0, 1.0));
                s.end_set_parameter(&$param);
            }
        }};
    }

    set_b!(p.shimmer.enabled, enabled[0]);
    set_b!(p.mist.enabled, enabled[1]);
    set_b!(p.drift.enabled, enabled[2]);
    set_b!(p.crystal.enabled, enabled[3]);
    set_b!(p.scatter.enabled, enabled[4]);
    set_b!(p.bloom.enabled, enabled[5]);
    set_f!(p.shimmer.mix, texture[0]);
    set_f!(p.shimmer.pitch, texture[1]);
    set_f!(p.shimmer.feedback, texture[2]);
    set_f!(p.shimmer.low_cut, texture[3]);
    set_f!(p.shimmer.high_cut, texture[4]);
    set_f!(p.mist.mix, texture[5]);
    set_f!(p.mist.hp, texture[6]);
    set_f!(p.mist.feedback, texture[7]);
    set_f!(p.mist.delay, texture[8]);
    set_f!(p.mist.rate, texture[9]);
    set_f!(p.drift.mix, texture[10]);
    set_f!(p.drift.feedback, texture[11]);
    set_f!(p.drift.delay, texture[12]);
    set_f!(p.drift.rate, texture[13]);
    set_f!(p.drift.depth, texture[14]);
    set_f!(p.crystal.mix, texture[15]);
    set_f!(p.crystal.size, texture[16]);
    set_f!(p.crystal.density, texture[17]);
    set_f!(p.crystal.feedback, texture[18]);
    set_f!(p.scatter.mix, texture[19]);
    set_f!(p.scatter.spread, texture[20]);
    set_f!(p.scatter.density, texture[21]);
    set_f!(p.scatter.speed, texture[22]);
    set_f!(p.scatter.feedback, texture[23]);
    set_f!(p.bloom.mix, texture[24]);
    set_f!(p.bloom.rise, texture[25]);
    set_f!(p.bloom.size, texture[26]);
    set_f!(p.bloom.feedback, texture[27]);
    set_f!(p.bloom.tone, texture[28]);
    set_f!(p.velvet.diffusion, texture[29]);
    set_f!(p.velvet.density, texture[30]);
    set_f!(p.velvet.tone, texture[31]);
}

pub fn set_one(p: &Arc<PejvakRvbParams>, s: &ParamSetter, i: usize, v: f32) {
    let v = v.clamp(0.0, 1.0);
    macro_rules! f {
        ($x:expr) => {{
            s.begin_set_parameter(&$x);
            s.set_parameter_normalized(&$x, v);
            s.end_set_parameter(&$x);
        }};
    }
    macro_rules! b {
        ($x:expr) => {{
            s.begin_set_parameter(&$x);
            s.set_parameter(&$x, v >= 0.5);
            s.end_set_parameter(&$x);
        }};
    }
    match i {
        0 => f!(p.input.mix),
        1 => f!(p.input.predelay),
        2 => f!(p.input.high_pass),
        3 => f!(p.input.low_pass),
        4 => b!(p.early.tap_enabled),
        5 => f!(p.early.tap_count),
        6 => f!(p.early.tap_length),
        7 => f!(p.early.tap_gain),
        8 => f!(p.early.tap_decay),
        9 => b!(p.early.diffusion_enabled),
        10 => f!(p.early.diffusion_stages),
        11 => f!(p.early.diffusion_delay),
        12 => f!(p.early.diffusion_feedback),
        13 => b!(p.late.late_delay_enabled),
        14 => f!(p.late.line_count),
        15 => f!(p.late.line_delay),
        16 => f!(p.late.line_decay),
        17 => b!(p.late.late_diffusion_enabled),
        18 => f!(p.late.late_diffusion_stages),
        19 => f!(p.late.late_diffusion_delay),
        20 => f!(p.late.late_diffusion_feedback),
        21 => f!(p.eq.low_shelf_gain),
        22 => f!(p.eq.low_shelf_frequency),
        23 => f!(p.eq.high_shelf_gain),
        24 => f!(p.eq.high_shelf_frequency),
        25 => f!(p.eq.cutoff_frequency),
        26 => f!(p.modulation.early_diffusion_mod_amount),
        27 => f!(p.modulation.early_diffusion_mod_rate),
        28 => f!(p.modulation.line_mod_amount),
        29 => f!(p.modulation.line_mod_rate),
        30 => f!(p.modulation.late_diffusion_mod_amount),
        31 => f!(p.modulation.late_diffusion_mod_rate),
        32 => f!(p.seeds.tap_seed),
        33 => f!(p.seeds.diffusion_seed),
        34 => f!(p.seeds.delay_seed),
        35 => f!(p.seeds.post_diffusion_seed),
        36 => f!(p.seeds.cross_seed),
        37 => f!(p.output.dry_out),
        38 => f!(p.output.predelay_out),
        39 => f!(p.output.early_out),
        40 => f!(p.output.main_out),
        41 => f!(p.output.width),
        42 => b!(p.switches.hi_pass_enabled),
        43 => b!(p.switches.low_pass_enabled),
        44 => b!(p.switches.low_shelf_enabled),
        45 => b!(p.switches.high_shelf_enabled),
        46 => b!(p.switches.cutoff_enabled),
        47 => b!(p.switches.late_stage_tap),
        48 => b!(p.switches.interpolation),
        _ => {}
    }
}

pub fn reset_texture_extras(p: &Arc<PejvakRvbParams>, s: &ParamSetter) {
    macro_rules! set_f {
        ($param:expr, $val:expr) => {{
            s.begin_set_parameter(&$param);
            s.set_parameter_normalized(&$param, ($val as f32).clamp(0.0, 1.0));
            s.end_set_parameter(&$param);
        }};
    }
    macro_rules! set_b {
        ($param:expr, $val:expr) => {{
            s.begin_set_parameter(&$param);
            s.set_parameter(&$param, $val);
            s.end_set_parameter(&$param);
        }};
    }
    set_b!(p.shimmer.enabled, false);
    set_f!(p.shimmer.mix, 0.0);
    set_f!(p.shimmer.pitch, 1.0);
    set_f!(p.shimmer.feedback, 0.6);
    set_f!(p.shimmer.low_cut, 0.2);
    set_f!(p.shimmer.high_cut, 0.6);
    set_b!(p.mist.enabled, false);
    set_f!(p.mist.mix, 0.0);
    set_f!(p.mist.hp, 0.1);
    set_f!(p.mist.feedback, 0.5);
    set_f!(p.mist.delay, 0.5);
    set_f!(p.mist.rate, 0.25);
    set_b!(p.drift.enabled, false);
    set_f!(p.drift.mix, 0.0);
    set_f!(p.drift.feedback, 0.3);
    set_f!(p.drift.delay, 0.5);
    set_f!(p.drift.rate, 0.3);
    set_f!(p.drift.depth, 1.0);
    set_b!(p.crystal.enabled, false);
    set_f!(p.crystal.mix, 0.0);
    set_f!(p.crystal.size, 0.4);
    set_f!(p.crystal.density, 0.5);
    set_f!(p.crystal.feedback, 0.5);
    set_b!(p.scatter.enabled, false);
    set_f!(p.scatter.mix, 0.0);
    set_f!(p.scatter.spread, 0.5);
    set_f!(p.scatter.density, 0.5);
    set_f!(p.scatter.speed, 0.5);
    set_f!(p.scatter.feedback, 0.3);
    set_b!(p.bloom.enabled, false);
    set_f!(p.bloom.mix, 0.0);
    set_f!(p.bloom.rise, 0.5);
    set_f!(p.bloom.size, 0.5);
    set_f!(p.bloom.feedback, 0.5);
    set_f!(p.bloom.tone, 0.5);
}

pub fn reset_velvet(p: &Arc<PejvakRvbParams>, s: &ParamSetter) {
    macro_rules! set_f {
        ($param:expr, $val:expr) => {{
            s.begin_set_parameter(&$param);
            s.set_parameter_normalized(&$param, ($val as f32).clamp(0.0, 1.0));
            s.end_set_parameter(&$param);
        }};
    }
    set_f!(p.velvet.diffusion, 0.0);
    set_f!(p.velvet.density, 0.5);
    set_f!(p.velvet.tone, 0.5);
}

pub fn reset_ffdn_card_masters(p: &Arc<PejvakRvbParams>, s: &ParamSetter) {
    macro_rules! set_b {
        ($param:expr) => {{
            s.begin_set_parameter(&$param);
            s.set_parameter(&$param, true);
            s.end_set_parameter(&$param);
        }};
    }
    set_b!(p.ffdn.space_enabled);
    set_b!(p.ffdn.network_enabled);
    set_b!(p.ffdn.character_enabled);
    set_b!(p.ffdn.shelves_enabled);
    set_b!(p.ffdn.modulation_enabled);
    set_b!(p.ffdn.filter_enabled);
    set_b!(p.ffdn.dynamics_enabled);
}

pub fn apply_factory_preset(
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    preset: &crate::presets::FactoryPreset,
) {
    use crate::presets::FactoryEngine;
    let target_ffdn = preset.engine == FactoryEngine::Ffdn;
    s.begin_set_parameter(&p.ffdn.use_ffdn);
    s.set_parameter(&p.ffdn.use_ffdn, target_ffdn);
    s.end_set_parameter(&p.ffdn.use_ffdn);

    reset_texture_extras(p, s);
    reset_velvet(p, s);
    reset_ffdn_card_masters(p, s);
    apply_eq_master(p, s, true);

    match preset.engine {
        FactoryEngine::CloudSeed => {
            const EXTREME_CAT: usize = 7;
            let width_norm: f32 = if preset.category == EXTREME_CAT {
                0.75
            } else {
                0.5
            };
            let mut vals = preset.values;
            vals[41] = width_norm;
            apply_preset(p, s, &vals);
        }
        FactoryEngine::Ffdn => {
            p.reset_requested
                .store(true, std::sync::atomic::Ordering::Relaxed);
            apply_ffdn_config(p, s, &preset.ffdn);
        }
    }

    let tex = preset.texture;
    apply_texture_layer(s, &p.shimmer.mix, &p.shimmer.enabled, tex.shimmer_mix);
    apply_texture_layer(s, &p.mist.mix, &p.mist.enabled, tex.mist_mix);
    apply_texture_layer(s, &p.drift.mix, &p.drift.enabled, tex.drift_mix);
    apply_texture_layer(s, &p.crystal.mix, &p.crystal.enabled, tex.crystal_mix);
    apply_texture_layer(s, &p.scatter.mix, &p.scatter.enabled, tex.scatter_mix);
    apply_texture_layer(s, &p.bloom.mix, &p.bloom.enabled, tex.bloom_mix);

    apply_illusion_config(p, s, &preset.illusion);
    apply_texture_order(p, [0, 1, 2, 3, 4, 5]);
}

fn apply_ffdn_config(p: &Arc<PejvakRvbParams>, s: &ParamSetter, cfg: &crate::presets::FfdnConfig) {
    macro_rules! set_f {
        ($param:expr, $val:expr) => {{
            s.begin_set_parameter(&$param);
            s.set_parameter_normalized(&$param, ($val as f32).clamp(0.0, 1.0));
            s.end_set_parameter(&$param);
        }};
    }
    macro_rules! set_b {
        ($param:expr, $val:expr) => {{
            s.begin_set_parameter(&$param);
            s.set_parameter(&$param, $val);
            s.end_set_parameter(&$param);
        }};
    }

    set_f!(p.ffdn.size, cfg.size);
    set_f!(p.ffdn.decay, cfg.decay);
    set_f!(p.ffdn.damping, cfg.damping);
    set_f!(p.ffdn.spread, cfg.spread);
    set_f!(p.ffdn.mod_amount, cfg.mod_amount);
    set_f!(p.ffdn.mod_rate, cfg.mod_rate);
    set_f!(
        p.ffdn.matrix_type,
        if cfg.velvet_matrix { 1.0 } else { 0.0 }
    );
    set_f!(p.ffdn.bass_mult, cfg.bass_mult);
    set_f!(p.ffdn.bass_freq, cfg.bass_freq);
    set_f!(p.ffdn.treble_mult, cfg.treble_mult);
    set_f!(p.ffdn.treble_freq, cfg.treble_freq);
    set_f!(p.ffdn.diffusion, cfg.diffusion);
    set_f!(p.ffdn.cross, cfg.cross);
    set_f!(p.ffdn.drive, cfg.drive);
    set_f!(p.ffdn.tilt_freq, cfg.tilt_freq);
    set_f!(p.ffdn.tilt_amount, cfg.tilt_amount);
    set_f!(p.ffdn.er_level, cfg.er_level);
    set_b!(p.ffdn.drive_enabled, cfg.drive_enabled);
    set_b!(p.ffdn.tilt_enabled, cfg.tilt_enabled);

    let lines_norm = ((cfg.lines.clamp(4, 16) as f32) - 4.0) / 12.0;
    set_f!(p.ffdn.lines, lines_norm);

    set_f!(p.ffdn.density, cfg.density);
    set_f!(p.ffdn.decorr, cfg.decorr);
    set_f!(p.ffdn.loop_hp, cfg.loop_hp);
    set_f!(p.ffdn.loop_lp, cfg.loop_lp);
    set_f!(p.ffdn.mid_focus, cfg.mid_focus);
    set_f!(p.ffdn.attack, cfg.attack);
    set_f!(p.ffdn.ducking, cfg.ducking);
    set_f!(p.ffdn.gate, cfg.gate);
    set_f!(p.ffdn.matrix_mod, 0.0);
    set_f!(p.ffdn.matrix_rate, 0.5);

    set_b!(p.ffdn.space_enabled, true);
    set_b!(p.ffdn.network_enabled, true);
    set_b!(p.ffdn.character_enabled, true);
    set_b!(p.ffdn.shelves_enabled, true);
    set_b!(p.ffdn.modulation_enabled, true);
    set_b!(p.ffdn.filter_enabled, true);
    set_b!(p.ffdn.dynamics_enabled, true);
}

fn apply_illusion_config(
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    cfg: &crate::presets::IllusionConfig,
) {
    let enabled = cfg.spark > 0.001;
    s.begin_set_parameter(&p.illusion.enabled);
    s.set_parameter(&p.illusion.enabled, enabled);
    s.end_set_parameter(&p.illusion.enabled);
    macro_rules! set_f {
        ($param:expr, $val:expr) => {{
            s.begin_set_parameter(&$param);
            s.set_parameter_normalized(&$param, ($val as f32).clamp(0.0, 1.0));
            s.end_set_parameter(&$param);
        }};
    }
    set_f!(p.illusion.spark, cfg.spark);
    set_f!(p.illusion.still, cfg.still);
    set_f!(p.illusion.aura, cfg.aura_norm);
    set_f!(p.illusion.veil, cfg.veil);
    set_f!(p.illusion.wide, cfg.wide);
    set_f!(p.illusion.tint, cfg.tint);
    set_f!(p.illusion.rate, cfg.rate);
    set_f!(p.illusion.grain, cfg.grain);
}

fn apply_texture_layer(
    s: &ParamSetter,
    mix_param: &FloatParam,
    enabled_param: &BoolParam,
    mix: f32,
) {
    let m = mix.clamp(0.0, 1.0);
    let enabled = m > 0.001;
    let effective_mix = if enabled { m } else { 0.0 };

    s.begin_set_parameter(enabled_param);
    s.set_parameter(enabled_param, enabled);
    s.end_set_parameter(enabled_param);

    s.begin_set_parameter(mix_param);
    s.set_parameter_normalized(mix_param, effective_mix);
    s.end_set_parameter(mix_param);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_orders() {
        assert_eq!(
            parse_texture_order(r#"<Order value="0,1,2,3,4,5"/>"#),
            Some([0, 1, 2, 3, 4, 5])
        );
        assert_eq!(
            parse_texture_order(r#"<Order value="5,4,3,2,1,0"/>"#),
            Some([5, 4, 3, 2, 1, 0])
        );
        assert_eq!(
            parse_texture_order(r#"<Order value="0, 1, 2, 3, 4, 5"/>"#),
            Some([0, 1, 2, 3, 4, 5])
        );
    }

    #[test]
    fn rejects_invalid_orders() {
        assert_eq!(parse_texture_order(r#"<Order value="0,0,2,3,4,5"/>"#), None);
        assert_eq!(parse_texture_order(r#"<Order value="0,1,2,3,4,6"/>"#), None);
        assert_eq!(parse_texture_order(r#"<Order value="0,1,2,3,4"/>"#), None);
        assert_eq!(parse_texture_order("<CloudReverb></CloudReverb>"), None);
        assert_eq!(parse_texture_order(r#"<Order value="0,1,2,3,4,x"/>"#), None);
    }

    #[test]
    fn test_denormalize_step_snap() {
        let p = denormalize(5, 6.0);
        let expected = 5.0 / 49.0;
        assert!(
            (p - expected).abs() < 1e-6,
            "Failed to snap to step grid: {}",
            p
        );
    }
}
