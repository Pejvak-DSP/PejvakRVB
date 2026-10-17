//! PEJVAK RVB – egui editor for nice-plug 0.4.2.
//!
//! Two pages:
//!   * MAIN    — the 49 CloudSeed reverb parameters (7 cards).
//!   * TEXTURE — the PEJVAK Texture Rack layers, one card per layer,
//!     plus a full-width SIGNAL CHAIN card at the bottom.
//!
//! Page switching uses a single toggle button and a smooth fade
//! transition (150 ms out, 150 ms in) driven by `Ui::multiply_opacity`.

use std::f32::consts::PI;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

use egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::{Param, ParamSetter};
use nice_plug_egui::{
    create_egui_editor, EguiEditor, EguiNiceSettings, NiceEguiApp, RepaintNotifier,
};

use crate::presets::{CATEGORIES, FACTORY_PRESETS};
use crate::PejvakRvbParams;

use std::sync::atomic::Ordering;

/// Forward a keyboard event directly to the DAW's main window.
///
/// `baseview` consumes keyboard events and does not forward unhandled
/// ones to the parent window (its `EventStatus::Ignored` only tells
/// `baseview` itself to skip its own handling). Since the plugin
/// editor is a top-level window owned by the DAW, we walk up the
/// owner chain with `GA_ROOTOWNER` and `PostMessageW` the key to the
/// owning top-level window — which is where the DAW's shortcut
/// handler lives.
#[cfg(target_os = "windows")]
fn forward_key_to_daw(key: egui::Key, pressed: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetForegroundWindow, PostMessageW, GA_ROOTOWNER, WM_KEYDOWN, WM_KEYUP,
    };

    // Virtual-key code for the keys we care about. Modifier keys are
    // skipped on purpose: the DAW tracks Ctrl/Shift/Alt state itself.
    let vk: usize = match key {
        egui::Key::Space => 0x20,
        egui::Key::Enter => 0x0D,
        egui::Key::Escape => 0x1B,
        egui::Key::Tab => 0x09,
        egui::Key::Backspace => 0x08,
        egui::Key::Delete => 0x2E,
        egui::Key::Home => 0x24,
        egui::Key::End => 0x23,
        egui::Key::ArrowUp => 0x26,
        egui::Key::ArrowDown => 0x28,
        egui::Key::ArrowLeft => 0x25,
        egui::Key::ArrowRight => 0x27,
        egui::Key::Num0 => 0x30,
        egui::Key::Num1 => 0x31,
        egui::Key::Num2 => 0x32,
        egui::Key::Num3 => 0x33,
        egui::Key::Num4 => 0x34,
        egui::Key::Num5 => 0x35,
        egui::Key::Num6 => 0x36,
        egui::Key::Num7 => 0x37,
        egui::Key::Num8 => 0x38,
        egui::Key::Num9 => 0x39,
        egui::Key::A => 0x41,
        egui::Key::B => 0x42,
        egui::Key::C => 0x43,
        egui::Key::D => 0x44,
        egui::Key::E => 0x45,
        egui::Key::F => 0x46,
        egui::Key::G => 0x47,
        egui::Key::H => 0x48,
        egui::Key::I => 0x49,
        egui::Key::J => 0x4A,
        egui::Key::K => 0x4B,
        egui::Key::L => 0x4C,
        egui::Key::M => 0x4D,
        egui::Key::N => 0x4E,
        egui::Key::O => 0x4F,
        egui::Key::P => 0x50,
        egui::Key::Q => 0x51,
        egui::Key::R => 0x52,
        egui::Key::S => 0x53,
        egui::Key::T => 0x54,
        egui::Key::U => 0x55,
        egui::Key::V => 0x56,
        egui::Key::W => 0x57,
        egui::Key::X => 0x58,
        egui::Key::Y => 0x59,
        egui::Key::Z => 0x5A,
        _ => return,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return;
        }

        // GA_ROOTOWNER walks the owner chain up to the top-level
        // owning window. For a plugin editor owned by the DAW, this
        // returns the DAW's main window.
        let target = GetAncestor(hwnd, GA_ROOTOWNER);
        if target.is_null() {
            return;
        }

        let (msg, lparam) = if pressed {
            (WM_KEYDOWN, 1isize)
        } else {
            (WM_KEYUP, 0xC0000001isize)
        };

        PostMessageW(target, msg, vk, lparam);
    }
}

// ============================================================================
// Editor pages
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditorPage {
    Main,
    Texture,
}

/// Action requested by the Undo/Redo header buttons, applied after
/// the header RTL closure has finished.
#[derive(Clone, Copy)]
enum HistoryAction {
    Undo,
    Redo,
}

/// Which modal panel is currently open (if any). Stored in egui
/// memory so it survives across frames.
///
/// The Manual entry is intentionally absent: clicking "Manual" in the
/// ☰ menu now opens the bundled HTML file in the user's browser
/// rather than a modal, so only About remains here.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ModalPanel {
    About,
}

// ============================================================================
// Window & layout constants
// ============================================================================

pub const WINDOW_W: u32 = 1100;
pub const WINDOW_H: u32 = 570;

const PAGE_GAP: f32 = 6.0;
const OUTER_MARGIN_H: f32 = 16.0;
const OUTER_MARGIN_V: f32 = 8.0;
const CARD_PAD: f32 = 8.0;
const CARD_TITLE_H: f32 = 24.0;

const KNOB_SLOT_W: f32 = 60.0;
const KNOB_SLOT_H: f32 = 76.0;
const KNOB_D: f32 = 42.0;

const KNOB_SMALL_SLOT_W: f32 = 52.0;
const KNOB_SMALL_SLOT_H: f32 = 66.0;
const KNOB_SMALL_D: f32 = 34.0;

const FADER_SLOT_W: f32 = 46.0;
const FADER_SLOT_H: f32 = 112.0;
const FADER_H: f32 = 72.0;

const NB_SLOT_W: f32 = 74.0;
const NB_SLOT_H: f32 = 34.0;

const ROW_INNER_GAP: f32 = 6.0;
const ROW_OUTER_GAP: f32 = 4.0;

const TEX_COLUMN_GAP: f32 = 10.0;

const PAGE_FADE_SECS: f32 = 0.15;

/// Duration of each phase of the CLOUD ↔ FFDN switch animation.
///
/// deliberately longer than `PAGE_FADE_SECS` so the engine
/// switch feels like a soft cross-dissolve rather than a page flip.
/// The full animation takes 2 × this value (~500 ms), which reads as
/// smooth without ever feeling sluggish.
const ENGINE_FADE_SECS: f32 = 0.25;

/// Standard smoothstep easing curve: 0 → 1 with a gentle
/// accelerate-then-decelerate shape. Applied to the engine fade
/// so the cards ease in/out instead of ramping linearly.
#[inline]
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Accent colour used by the Texture kill switch and any other UI
/// element that needs to reflect the locked state. Matches the
/// Interp toggle colour in the header so that the two reds read as
/// the same "active" red across the whole interface.
const KILL_RED: Color32 = Color32::from_rgb(0xEF, 0x44, 0x44);

/// PEJVAK DSP logo, embedded at compile time. Displayed at the top
/// of the About modal. Same file that the HTML manual references as
/// `logo.png`; keeping a single copy in `manual/` avoids drift
/// between the two consumers.
const LOGO_PNG: &[u8] = include_bytes!("../../manual/logo.png");

/// UI scale presets shown in the Size menu. Each entry is a
/// percentage of the `WINDOW_W × WINDOW_H` reference size.
/// The values are chosen so a user can quickly jump between the most
/// common scaling levels without needing to drag the corner handle
/// precisely.
const UI_SCALE_PRESETS: &[(u32, &str)] = &[
    (70, "70 %"),
    (80, "80 %"),
    (90, "90 %"),
    (100, "100 %"),
    (110, "110 %"),
    (120, "120 %"),
    (130, "130 %"),
    (150, "150 %"),
    (170, "170 %"),
    (200, "200 %"),
];

/// Uniform spacing between header control groups. With the
/// `item_spacing` override inside the RTL closure (see
/// `draw_editor`), this value is now the *total* gap between
/// adjacent control groups.
const HEADER_GAP: f32 = 7.0;

/// Spacing between adjacent buttons inside the same group
/// (Undo / Redo). Slightly tighter than `HEADER_GAP` so the pair
/// reads as a unit while still being visually close to the group
/// rhythm.
const HEADER_TIGHT_GAP: f32 = 3.0;

// ============================================================================
// Undo / Redo
// ============================================================================

/// Full snapshot of every user-visible parameter.
///
/// Used by the undo/redo system to restore the plugin to a previous
/// state. Every field is a plain value type; the struct derives
/// `PartialEq` so the UI can detect "something changed" by simple
/// comparison.
///
/// Scope: CloudSeed (49), Texture + Velvet (32), Texture enable (6),
/// FFDN (37), ILLUSION (8), ILLUSION enable (1), and the four global
/// booleans (bypass, freeze, interp, eq). Texture chain order is
/// captured as the packed u64 that the DSP already consumes.
#[derive(Clone, PartialEq)]
struct UndoSnapshot {
    cloudseed: [f32; 49],
    texture: [f32; 32],
    texture_enabled: [bool; 6],
    ffdn: [f32; 37],
    illusion: [f32; 8],
    chime_enabled: bool,
    eq_enabled: bool,
    bypass: bool,
    freeze: bool,
    interp: bool,
    texture_order: u64,
    preset_name: String,
    page: EditorPage,
}

/// Maximum number of undo steps retained. The redo stack is bounded
/// by the same value.
const UNDO_DEPTH: usize = 50;

/// Grace period after applying an undo/redo snapshot during which
/// automatic change-detection is suppressed. Parameter values move
/// toward their targets over a few frames (nice-plug smooths them),
/// so without this the UI would immediately re-trigger a "change"
/// and push a spurious undo entry.
const UNDO_SUPPRESS_S: f64 = 0.25;

/// Captures the current state of every tracked parameter.
fn capture_undo_snapshot(p: &PejvakRvbParams, preset_name: &str, page: EditorPage) -> UndoSnapshot {
    let mut cloudseed = [0.0f32; 49];
    for i in 0..49 {
        cloudseed[i] = p.normalized(i) as f32;
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

    let ffdn: [f32; 37] = [
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
        chime_enabled: p.illusion.enabled.value(),
        eq_enabled: p.eq.enabled.value(),
        bypass: p.global.bypass.value(),
        freeze: p.global.freeze.value(),
        interp: p.switches.interpolation.value(),
        texture_order: p.texture_order.load(std::sync::atomic::Ordering::Relaxed),
        preset_name: preset_name.to_string(),
        page,
    }
}

/// Restores the plugin state from a snapshot.
///
/// Every parameter push goes through `begin_set_parameter` /
/// `end_set_parameter` so the host records the change as a discrete
/// automation event.
///
/// Note: `apply_preset` (the 49-param convenience wrapper) is NOT
/// used here because it also requests a DSP reset — which would cut
/// the reverb tail. Undo/redo should be a smooth transition, not a
/// hard cut.
fn apply_undo_snapshot(p: &Arc<PejvakRvbParams>, s: &ParamSetter, snap: &UndoSnapshot) -> String {
    p.reset_requested
        .store(true, std::sync::atomic::Ordering::Relaxed);

    for i in 0..49 {
        set_one(p, s, i, snap.cloudseed[i]);
    }

    // Texture: reuse the existing helper, but feed it plain values.
    apply_texture_params(p, s, &snap.texture, &snap.texture_enabled);

    // Restore *all* FFDN slots, including index 0 (the engine
    // selector). Engine switches never create an undo entry on their
    // own — that is enforced in `snapshots_differ` — but the engine
    // *is* part of the state an undo restores, so that undo lands on
    // the same engine the change was made in.
    let mut ffdn_opts: [Option<f32>; 37] = [None; 37];
    for i in 0..37 {
        ffdn_opts[i] = Some(snap.ffdn[i]);
    }
    apply_ffdn_params(p, s, &ffdn_opts);

    // ILLUSION: always "seen", so every field is pushed.
    apply_chime_params(p, s, &snap.illusion, Some(snap.chime_enabled), true);

    // Master EQ switch.
    apply_eq_master(p, s, snap.eq_enabled);

    // Globals + interp.
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

    // Texture chain order (atomic, not a host parameter).
    p.texture_order
        .store(snap.texture_order, std::sync::atomic::Ordering::Relaxed);

    // Return the preset name so the caller can restore the label in
    // the preset browser along with the parameter values.
    snap.preset_name.clone()
}

/// Returns true if the two snapshots differ by more than `eps` in
/// any numeric field.
///
/// A strict `!=` comparison would fire every frame because
/// nice-plug smooths `FloatParam::value()` toward its target over
/// several frames. That keeps `change_detected_at` permanently set
/// and prevents the debounce timer from ever expiring — with the
/// result that the undo stack never fills up and the Undo/Redo
/// buttons stay permanently disabled.
fn snapshots_differ(a: &UndoSnapshot, b: &UndoSnapshot, eps: f32) -> bool {
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
    for i in 1..37 {
        if (a.ffdn[i] - b.ffdn[i]).abs() > eps {
            return true;
        }
    }
    for i in 0..8 {
        if (a.illusion[i] - b.illusion[i]).abs() > eps {
            return true;
        }
    }
    // Page changes (MAIN ↔ TEXTURE) never create an undo entry on
    // their own — they are a context switch, like the engine
    // selector. The page is still captured in the snapshot so that
    // undo restores the page the change was made on.
    a.chime_enabled != b.chime_enabled
        || a.eq_enabled != b.eq_enabled
        || a.bypass != b.bypass
        || a.freeze != b.freeze
        || a.interp != b.interp
        || a.texture_order != b.texture_order
        || a.preset_name != b.preset_name
}

// ============================================================================
// Native file-dialog state
// ============================================================================

#[derive(Default)]
struct PresetDialogState {
    open: bool,
    save_path: Option<PathBuf>,
    load_path: Option<PathBuf>,
}

fn preset_dialog() -> &'static Mutex<PresetDialogState> {
    static S: OnceLock<Mutex<PresetDialogState>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(PresetDialogState::default()))
}

fn pd_lock() -> std::sync::MutexGuard<'static, PresetDialogState> {
    preset_dialog().lock().unwrap_or_else(|e| e.into_inner())
}

fn spawn_save_dialog() {
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
        let path = dlg.save_file();
        let mut st = pd_lock();
        st.save_path = path;
        st.open = false;
    });
}

fn spawn_load_dialog() {
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
        let path = dlg.pick_file();
        let mut st = pd_lock();
        st.load_path = path;
        st.open = false;
    });
}

// ============================================================================
// Preset I/O
// ============================================================================

fn user_presets_dir() -> Option<PathBuf> {
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

fn user_preset_names() -> Vec<String> {
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

// ----------------------------------------------------------------------------
// Texture Rack parameter names — used for XML save/load
// ----------------------------------------------------------------------------

const TEXTURE_PARAM_NAMES: [&str; 32] = [
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

const TEXTURE_ENABLED_NAMES: [&str; 6] = [
    "ShimmerEnabled",
    "MistEnabled",
    "DriftEnabled",
    "CrystalEnabled",
    "ScatterEnabled",
    "BloomEnabled",
];

const CHIME_PARAM_NAMES: [&str; 8] = [
    "ChimeSpark",
    "ChimeStill",
    "ChimeAura",
    "ChimeVeil",
    "ChimeWide",
    "ChimeTint",
    "ChimeRate",
    "ChimeGrain",
];

const CHIME_ENABLED_NAME: &str = "ChimeEnabled";

/// Names of the 30 FFDN parameters, in the same order as the
/// `FfdnParams` struct fields. Used for XML serialisation inside the
/// `<FFDN>` block. Order must match the layout of the `ffdn` array in
/// `ParsedPreset`.
///
/// Layout history:
///   * slots  0..13  — original FFDN core
///   * slots 13..21  — FFDN CHARACTER
///   * slots 21..30  — FFDN NETWORK / FILTER / DYNAMICS
///
/// The append-only layout keeps every previous XML version loadable:
/// `ParsedPreset::ffdn` slots that a given XML did not contain stay
/// `None`, and `apply_ffdn_params` leaves the corresponding parameter
/// untouched.
const FFDN_PARAM_NAMES: [&str; 37] = [
    // ---- FFDN CORE ------------------------------------
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
    // ---- FFDN CHARACTER -------------------------------
    "FfdnCross",
    "FfdnDrive",
    "FfdnTiltFreq",
    "FfdnTiltAmt",
    "FfdnErLevel",
    "FfdnDriveEnabled",
    "FfdnTiltEnabled",
    "FfdnErEnabled",
    // ---- FFDN NETWORK ---------------------------------
    "FfdnLines",
    "FfdnDensity",
    "FfdnDecorr",
    // ---- FFDN FILTER ----------------------------------
    "FfdnLoopHP",
    "FfdnLoopLP",
    "FfdnMidFocus",
    // ---- FFDN DYNAMICS --------------------------------
    "FfdnAttack",
    "FfdnDucking",
    "FfdnGate",
    // ---- FFDN CARD MASTER ENABLES --------------------
    "FfdnSpaceEnabled",
    "FfdnNetworkEnabled",
    "FfdnCharacterEnabled",
    "FfdnShelvesEnabled",
    "FfdnModulationEnabled",
    "FfdnFilterEnabled",
    "FfdnDynamicsEnabled",
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

/// Reads the current normalised value of one FFDN parameter from the
/// plugin state. `UseFfdn` is a `BoolParam` and is mapped to 0.0 / 1.0
/// so it can share the same XML format as the numeric parameters.
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
        // ---- FFDN CHARACTER ---------------------------
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
        // ---- FFDN NETWORK -----------------------------
        "FfdnLines" => p.ffdn.lines.value(),
        "FfdnDensity" => p.ffdn.density.value(),
        "FfdnDecorr" => p.ffdn.decorr.value(),
        // ---- FFDN FILTER ------------------------------
        "FfdnLoopHP" => p.ffdn.loop_hp.value(),
        "FfdnLoopLP" => p.ffdn.loop_lp.value(),
        "FfdnMidFocus" => p.ffdn.mid_focus.value(),
        // ---- FFDN DYNAMICS ----------------------------
        "FfdnAttack" => p.ffdn.attack.value(),
        "FfdnDucking" => p.ffdn.ducking.value(),
        "FfdnGate" => p.ffdn.gate.value(),
        // ---- FFDN CARD MASTER ENABLES ----------------
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
        _ => 0.0,
    }
}

fn params_to_xml(p: &PejvakRvbParams) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CloudReverb>\n");

    for i in 0..49 {
        let name = fdn_reverb_core::cloudseed::Parameter::from_index(i)
            .map(|x| x.name())
            .unwrap_or("Unknown");
        let norm = p.normalized(i);
        let phys = fdn_reverb_core::cloudseed::scaled_parameter_value(i, norm);
        s.push_str(&format!(
            "  <PARAM id=\"{}\" value=\"{:.6}\"/>\n",
            name, phys
        ));
    }

    // ---- PEJVAK extensions (top-level, before <FFDN>) ---------------
    // Master EQ switch.
    let eq_on = if p.eq.enabled.value() { 1.0 } else { 0.0 };
    s.push_str(&format!(
        "  <PARAM id=\"EqEnabled\" value=\"{:.6}\"/>\n",
        eq_on
    ));

    // ---- FFDN engine block ------------------------------------------
    // All 13 FFDN parameters are grouped inside a single <FFDN> block.
    // Reading is backward-compatible: legacy XMLs that stored only
    // `<PARAM id="FfdnDiffusion">` at the top level are still parsed
    // correctly (see `xml_to_params`).
    //
    // The order of the writes follows `FFDN_PARAM_NAMES`, so the file
    // reads naturally: engine selector first, then space/decay/damping,
    // then modulation, then matrix, then shelves, then diffusion.
    s.push_str("  <FFDN>\n");
    for name in FFDN_PARAM_NAMES.iter() {
        let v = ffdn_param_value(p, name);
        s.push_str(&format!(
            "    <PARAM id=\"{}\" value=\"{:.6}\"/>\n",
            name, v
        ));
    }
    s.push_str("  </FFDN>\n");

    // ---- ILLUSION section ----------------------------------------------
    s.push_str("  <Chime>\n");
    let chime_on = if p.illusion.enabled.value() { 1.0 } else { 0.0 };
    s.push_str(&format!(
        "    <PARAM id=\"{}\" value=\"{:.6}\"/>\n",
        CHIME_ENABLED_NAME, chime_on
    ));
    let chime_vals: [f32; 8] = [
        p.illusion.spark.value(),
        p.illusion.still.value(),
        p.illusion.aura.value(),
        p.illusion.veil.value(),
        p.illusion.wide.value(),
        p.illusion.tint.value(),
        p.illusion.rate.value(),
        p.illusion.grain.value(),
    ];
    for (i, name) in CHIME_PARAM_NAMES.iter().enumerate() {
        s.push_str(&format!(
            "    <PARAM id=\"{}\" value=\"{:.6}\"/>\n",
            name, chime_vals[i]
        ));
    }
    s.push_str("  </Chime>\n");

    s.push_str("  <TextureRack>\n");

    // ---- Signal-chain order -----------------------------------------
    // Stored as a comma-separated list of six layer indices (0..5).
    // Example: "0,1,2,3,4,5" = Shimmer → Mist → Drift → Crystal →
    // Scatter → Bloom.
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

    // ---- Enabled flags for each texture layer -----------------------
    for name in TEXTURE_ENABLED_NAMES.iter() {
        let v = if texture_enabled_value(p, name) {
            1.0
        } else {
            0.0
        };
        s.push_str(&format!(
            "    <PARAM id=\"{}\" value=\"{:.6}\"/>\n",
            name, v
        ));
    }

    // ---- 32 Texture + Velvet parameters -----------------------------
    for name in TEXTURE_PARAM_NAMES.iter() {
        let v = texture_param_value(p, name);
        s.push_str(&format!(
            "    <PARAM id=\"{}\" value=\"{:.6}\"/>\n",
            name, v
        ));
    }

    s.push_str("  </TextureRack>\n");
    s.push_str("</CloudReverb>\n");
    s
}

struct ParsedPreset {
    cloudseed: [f32; 49],
    cs_seen: [bool; 49],
    texture: [f32; 32],
    enabled: [bool; 6],
    texture_order: Option<[usize; 6]>,
    eq_enabled: Option<bool>,
    ffdn: [Option<f32>; 37],
    illusion: [f32; 8],
    chime_enabled: Option<bool>,
    chime_seen: bool,
}

fn for_each_param(content: &str, mut f: impl FnMut(&str, f32)) {
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
                f(&id, v);
            }
        }
        i = gt + 1;
    }
}

fn xml_to_params(path: &Path) -> Option<ParsedPreset> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut data = ParsedPreset {
        cloudseed: [0.0; 49],
        cs_seen: [false; 49],
        texture: [0.0; 32],
        enabled: [false; 6],
        texture_order: None,
        eq_enabled: None,
        ffdn: [None; 37],
        illusion: [0.0; 8],
        chime_enabled: None,
        chime_seen: false,
    };

    // ---- Section extraction -----------------------------------------
    // We slice the document into three regions:
    //
    //   main_section     — everything outside <TextureRack>
    //   ffdn_section     — contents of <FFDN>...</FFDN> (may be "")
    //   texture_section  — contents of <TextureRack>...</TextureRack>
    //
    // The FFDN block is deliberately *nested inside* main_section
    // rather than carved out of it, because `for_each_param` on the
    // main block ignores any id that does not match a CloudSeed
    // parameter name — and no FFDN name does. That keeps the parsing
    // single-pass for the CloudSeed half of the document.
    let texture_start = content.find("<TextureRack>");
    let texture_end = texture_start.and_then(|s| {
        content[s..]
            .find("</TextureRack>")
            .map(|e| s + e + "</TextureRack>".len())
    });

    let (main_section, texture_section): (&str, &str) = match (texture_start, texture_end) {
        (Some(s), Some(e)) => (&content[..s], &content[s..e]),
        _ => (content.as_str(), ""),
    };

    // Extract the <FFDN> block from main_section. Missing block is
    // fine — legacy XMLs predate it.
    let ffdn_section: &str = match main_section.find("<FFDN>") {
        Some(s) => match main_section[s..].find("</FFDN>") {
            Some(e_rel) => {
                let e = s + e_rel + "</FFDN>".len();
                &main_section[s..e]
            }
            None => "",
        },
        None => "",
    };

    // ---- <Chime> block ----------------------------------------------
    let chime_section: &str = match content.find("<Chime>") {
        Some(s) => match content[s..].find("</Chime>") {
            Some(e_rel) => {
                let e = s + e_rel + "</Chime>".len();
                &content[s..e]
            }
            None => "",
        },
        None => "",
    };

    if !chime_section.is_empty() {
        for_each_param(chime_section, |id, v| {
            if id == CHIME_ENABLED_NAME {
                data.chime_enabled = Some(v >= 0.5);
                data.chime_seen = true;
                return;
            }
            if let Some(idx) = CHIME_PARAM_NAMES.iter().position(|&n| n == id) {
                data.illusion[idx] = v.clamp(0.0, 1.0);
                data.chime_seen = true;
            }
        });
    }

    // ---- Top-level block: CloudSeed params + EqEnabled --------------
    for_each_param(main_section, |id, v| {
        if id == "EqEnabled" {
            data.eq_enabled = Some(v >= 0.5);
            return;
        }
        // Legacy fallback: XMLs written before stored the FFDN
        // diffusion amount as a top-level <PARAM>. We only accept it
        // when the new <FFDN> block did not already provide a value,
        // so the new format always wins if both are present.
        if id == "FfdnDiffusion" && data.ffdn[12].is_none() {
            data.ffdn[12] = Some(v.clamp(0.0, 1.0));
            return;
        }
        if let Some(p) = fdn_reverb_core::cloudseed::Parameter::from_name(id) {
            let idx = p.index();
            data.cloudseed[idx] = denormalize(idx, v);
            data.cs_seen[idx] = true;
        }
    });

    // ---- <FFDN> block -----------------------------------------------
    // FFDN parameter names all start with "Ffdn" or "UseFfdn", so
    // they cannot collide with any CloudSeed or Texture parameter.
    if !ffdn_section.is_empty() {
        for_each_param(ffdn_section, |id, v| {
            if let Some(idx) = FFDN_PARAM_NAMES.iter().position(|&n| n == id) {
                data.ffdn[idx] = Some(v.clamp(0.0, 1.0));
            }
        });
    }

    // ---- <TextureRack> block ----------------------------------------
    for_each_param(texture_section, |id, v| {
        if let Some(idx) = TEXTURE_PARAM_NAMES.iter().position(|&n| n == id) {
            data.texture[idx] = v.clamp(0.0, 1.0);
        } else if let Some(idx) = TEXTURE_ENABLED_NAMES.iter().position(|&n| n == id) {
            data.enabled[idx] = v >= 0.5;
        }
    });

    // The `<Order>` tag has a different shape from `<PARAM>`, so it is
    // parsed separately by a dedicated function.
    data.texture_order = parse_texture_order(texture_section);

    if data.cs_seen.iter().all(|&b| b) {
        Some(data)
    } else {
        None
    }
}

fn denormalize(index: usize, physical: f32) -> f32 {
    if is_switch(index) {
        return if physical >= 0.5 { 1.0 } else { 0.0 };
    }
    use fdn_reverb_core::cloudseed::scaled_parameter_value;
    let physical = physical as f64;
    let mut lo = 0.0f64;
    let mut hi = 1.0f64;
    let increasing = scaled_parameter_value(index, 1.0) >= scaled_parameter_value(index, 0.0);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        let v = scaled_parameter_value(index, mid);
        if (v < physical) == increasing {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (0.5 * (lo + hi)) as f32
}

fn is_switch(index: usize) -> bool {
    matches!(index, 4 | 9 | 13 | 17 | 42 | 43 | 44 | 45 | 46 | 47 | 48)
}

fn attr(seg: &str, key: &str) -> Option<String> {
    let pat = format!("{}=\"", key);
    let s = seg.find(&pat)? + pat.len();
    let e = seg[s..].find('"')? + s;
    Some(seg[s..e].to_string())
}

/// Parses `<Order value="0,1,2,3,4,5"/>` out of the `<TextureRack>`
/// section. Returns `None` if the tag is missing, malformed, or if
/// the six numbers are not a valid permutation of `0..6`.
fn parse_texture_order(content: &str) -> Option<[usize; 6]> {
    let start = content.find("<Order")?;
    let end_rel = content[start..].find("/>")?;
    let tag = &content[start..start + end_rel + 2];
    let val = attr(tag, "value")?;

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

/// Applies a signal-chain order to the shared atomic. Packs the six
/// indices into a `u64`, one byte per slot, matching the layout used
/// by the audio thread in `PejvakRvbPlugin::process_inner`.
fn apply_texture_order(p: &Arc<PejvakRvbParams>, order: [usize; 6]) {
    let mut packed: u64 = 0;
    for (i, &idx) in order.iter().enumerate() {
        packed |= (idx as u64) << (i * 8);
    }
    p.texture_order
        .store(packed, std::sync::atomic::Ordering::Relaxed);
}

/// Applies the master EQ switch to the plugin state. Used during
/// preset load and by the "set one parameter" helpers.
///
/// This is a single `BoolParam` push, so the host records it as one
/// discrete automation event.
fn apply_eq_master(p: &Arc<PejvakRvbParams>, s: &ParamSetter, enabled: bool) {
    s.begin_set_parameter(&p.eq.enabled);
    s.set_parameter(&p.eq.enabled, enabled);
    s.end_set_parameter(&p.eq.enabled);
}

/// Applies the FFDN parameter set parsed from an XML preset.
///
/// Only the slots that are `Some` in `params` are pushed to the host.
/// This is what makes loading a legacy preset safe: if the XML did not
/// contain (say) `FfdnBassMult`, the user's current value for that
/// parameter is preserved instead of being silently overwritten by a
/// default.
///
/// `UseFfdn` is a `BoolParam`, so it uses the boolean `set_parameter`
/// form; every other entry is a normalised `FloatParam` and uses
/// `set_parameter_normalized`.
fn apply_ffdn_params(p: &Arc<PejvakRvbParams>, s: &ParamSetter, params: &[Option<f32>; 37]) {
    // ---- UseFfdn (BoolParam) ---------------------------------------
    if let Some(v) = params[0] {
        s.begin_set_parameter(&p.ffdn.use_ffdn);
        s.set_parameter(&p.ffdn.use_ffdn, v >= 0.5);
        s.end_set_parameter(&p.ffdn.use_ffdn);
    }

    // ---- All FloatParams -------------------------------------------
    macro_rules! set_opt {
        ($param:expr, $val:expr) => {{
            if let Some(v) = $val {
                s.begin_set_parameter(&$param);
                s.set_parameter_normalized(&$param, v.clamp(0.0, 1.0));
                s.end_set_parameter(&$param);
            }
        }};
    }
    // Boolean parameters use the same pattern but with the boolean
    // `set_parameter` form.
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
    // ---- FFDN CHARACTER ---------------------------------
    set_opt!(p.ffdn.cross, params[13]);
    set_opt!(p.ffdn.drive, params[14]);
    set_opt!(p.ffdn.tilt_freq, params[15]);
    set_opt!(p.ffdn.tilt_amount, params[16]);
    set_opt!(p.ffdn.er_level, params[17]);
    set_opt_bool!(p.ffdn.drive_enabled, params[18]);
    set_opt_bool!(p.ffdn.tilt_enabled, params[19]);
    set_opt_bool!(p.ffdn.er_enabled, params[20]);
    // ---- FFDN NETWORK ---------------------------------
    set_opt!(p.ffdn.lines, params[21]);
    set_opt!(p.ffdn.density, params[22]);
    set_opt!(p.ffdn.decorr, params[23]);
    // ---- FFDN FILTER ----------------------------------
    set_opt!(p.ffdn.loop_hp, params[24]);
    set_opt!(p.ffdn.loop_lp, params[25]);
    set_opt!(p.ffdn.mid_focus, params[26]);
    // ---- FFDN DYNAMICS --------------------------------
    set_opt!(p.ffdn.attack, params[27]);
    set_opt!(p.ffdn.ducking, params[28]);
    set_opt!(p.ffdn.gate, params[29]);
    // ---- FFDN CARD MASTER ENABLES --------------------
    set_opt_bool!(p.ffdn.space_enabled, params[30]);
    set_opt_bool!(p.ffdn.network_enabled, params[31]);
    set_opt_bool!(p.ffdn.character_enabled, params[32]);
    set_opt_bool!(p.ffdn.shelves_enabled, params[33]);
    set_opt_bool!(p.ffdn.modulation_enabled, params[34]);
    set_opt_bool!(p.ffdn.filter_enabled, params[35]);
    set_opt_bool!(p.ffdn.dynamics_enabled, params[36]);
}

/// Applies the ILLUSION parameters parsed from an XML preset.
///
/// Only called when the `<Chime>` block was actually present in the
/// file. Legacy XMLs predating  will not touch the ILLUSION
/// parameters at all, preserving the user's current state.
fn apply_chime_params(
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    illusion: &[f32; 8],
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
    macro_rules! set_f {
        ($param:expr, $val:expr) => {{
            s.begin_set_parameter(&$param);
            s.set_parameter_normalized(&$param, $val.clamp(0.0, 1.0));
            s.end_set_parameter(&$param);
        }};
    }
    set_f!(p.illusion.spark, illusion[0]);
    set_f!(p.illusion.still, illusion[1]);
    set_f!(p.illusion.aura, illusion[2]);
    set_f!(p.illusion.veil, illusion[3]);
    set_f!(p.illusion.wide, illusion[4]);
    set_f!(p.illusion.tint, illusion[5]);
    set_f!(p.illusion.rate, illusion[6]);
    set_f!(p.illusion.grain, illusion[7]);
}

// ============================================================================
// Theme
// ============================================================================

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg_top: Color32,
    pub bg_bottom: Color32,
    pub line: Color32,
    pub txt: Color32,
    pub dim: Color32,
    pub dim2: Color32,
    pub knob_bg: Color32,
    pub knob_border: Color32,
    pub track_bg: Color32,
    pub thumb_bg: Color32,
    pub card_top: Color32,
    pub card_bottom: Color32,
    pub input: Color32,
    pub early: Color32,
    pub late: Color32,
    pub tone: Color32,
    pub output: Color32,
    pub accent: Color32,
    pub logo: Color32,
    pub tex_shimmer: Color32,
    pub tex_mist: Color32,
    pub tex_drift: Color32,
    pub tex_crystal: Color32,
    pub tex_scatter: Color32,
    pub tex_bloom: Color32,
    pub tex_chime: Color32,
    pub velvet: Color32,
    pub early_diffusion: Color32,
    pub late_diffusion: Color32,
    pub numbox_bg: Color32,
    pub led_ring: Color32,
    pub led_off: Color32,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg_top: Color32::from_rgb(0x16, 0x1E, 0x36),
            bg_bottom: Color32::from_rgb(0x0A, 0x0F, 0x1E),
            line: Color32::from_rgb(0x25, 0x2E, 0x48),
            txt: Color32::from_rgb(0xE6, 0xEA, 0xF4),
            dim: Color32::from_rgb(0x8D, 0x97, 0xB0),
            dim2: Color32::from_rgb(0x5E, 0x68, 0x82),
            knob_bg: Color32::from_rgb(0x1A, 0x22, 0x3C),
            knob_border: Color32::from_rgb(0x2C, 0x37, 0x54),
            track_bg: Color32::from_rgb(0x28, 0x31, 0x4C),
            thumb_bg: Color32::from_rgb(0x22, 0x2B, 0x44),
            card_top: Color32::from_rgb(0x1C, 0x25, 0x40),
            card_bottom: Color32::from_rgb(0x13, 0x1A, 0x30),
            input: Color32::from_rgb(0x6E, 0xA8, 0xFE),
            early: Color32::from_rgb(0x63, 0xD2, 0xA6),
            late: Color32::from_rgb(0xC4, 0x92, 0xFF),
            tone: Color32::from_rgb(0xFF, 0xB5, 0x6B),
            output: Color32::from_rgb(0xFF, 0x8F, 0x8F),
            accent: Color32::from_rgb(0x67, 0xE8, 0xF9),
            logo: Color32::from_rgb(0xE9, 0xF4, 0xFF),
            early_diffusion: Color32::from_rgb(0x5D, 0xBA, 0x7D),
            late_diffusion: Color32::from_rgb(0xCA, 0x7A, 0xD4),
            tex_shimmer: Color32::from_rgb(0xFF, 0xD5, 0x4F),
            tex_mist: Color32::from_rgb(0x4D, 0xD0, 0xE1),
            tex_drift: Color32::from_rgb(0x81, 0xC7, 0x84),
            tex_crystal: Color32::from_rgb(0x64, 0xB5, 0xF6),
            tex_scatter: Color32::from_rgb(0xFF, 0x8A, 0x65),
            tex_bloom: Color32::from_rgb(0xBA, 0x68, 0xC8),
            // Icy white — a cool near-white with a faint blue cast.
            tex_chime: Color32::from_rgb(0xE6, 0xF2, 0xFF),
            velvet: Color32::from_rgb(0xB0, 0x8A, 0xE8),
            numbox_bg: Color32::from_rgb(0x0D, 0x12, 0x23),
            led_ring: Color32::from_rgb(0x08, 0x0B, 0x14),
            led_off: Color32::from_rgb(0x20, 0x26, 0x38),
        }
    }
    pub fn light() -> Self {
        Self {
            bg_top: Color32::from_rgb(0xE3, 0xE7, 0xEE),
            bg_bottom: Color32::from_rgb(0xD7, 0xDC, 0xE6),
            line: Color32::from_rgb(0xC3, 0xC9, 0xD4),
            txt: Color32::from_rgb(0x1D, 0x21, 0x29),
            dim: Color32::from_rgb(0x4D, 0x53, 0x5F),
            dim2: Color32::from_rgb(0x77, 0x7E, 0x8C),
            knob_bg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            knob_border: Color32::from_rgb(0xB9, 0xC0, 0xCD),
            track_bg: Color32::from_rgb(0xCC, 0xD2, 0xDC),
            thumb_bg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            card_top: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            card_bottom: Color32::from_rgb(0xF0, 0xF3, 0xF8),
            input: Color32::from_rgb(0x1A, 0x73, 0xE8),
            early: Color32::from_rgb(0x0E, 0x9F, 0x6E),
            late: Color32::from_rgb(0x8B, 0x3D, 0xF5),
            tone: Color32::from_rgb(0xE8, 0x59, 0x0C),
            output: Color32::from_rgb(0xE0, 0x31, 0x31),
            accent: Color32::from_rgb(0x0C, 0x85, 0x99),
            logo: Color32::from_rgb(0x8F, 0xBA, 0xDB),
            early_diffusion: Color32::from_rgb(0x3E, 0x9E, 0x5F),
            late_diffusion: Color32::from_rgb(0xAD, 0x5C, 0xB8),
            tex_shimmer: Color32::from_rgb(0xF5, 0x7F, 0x17),
            tex_mist: Color32::from_rgb(0x00, 0x83, 0x8F),
            tex_drift: Color32::from_rgb(0x2E, 0x7D, 0x32),
            tex_crystal: Color32::from_rgb(0x15, 0x65, 0xC0),
            tex_scatter: Color32::from_rgb(0xD8, 0x43, 0x15),
            tex_bloom: Color32::from_rgb(0x6A, 0x1B, 0x9A),
            tex_chime: Color32::from_rgb(0x16, 0x2B, 0x5A),
            velvet: Color32::from_rgb(0x6E, 0x46, 0xA8),
            numbox_bg: Color32::from_rgb(0xC8, 0xCC, 0xD2),
            led_ring: Color32::from_rgb(0xB4, 0xB9, 0xC1),
            led_off: Color32::from_rgb(0x9A, 0x9F, 0xA9),
        }
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| {
        (x as f32 + (y as f32 - x as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color32::from_rgba_unmultiplied(
        m(a.r(), b.r()),
        m(a.g(), b.g()),
        m(a.b(), b.b()),
        m(a.a(), b.a()),
    )
}

impl Theme {
    pub fn lerp(&self, other: &Theme, t: f32) -> Theme {
        macro_rules! f {
            ($($x:ident),*) => {
                Theme { $($x: lerp_color(self.$x, other.$x, t)),* }
            };
        }
        f!(
            bg_top,
            bg_bottom,
            line,
            txt,
            dim,
            dim2,
            knob_bg,
            knob_border,
            track_bg,
            thumb_bg,
            card_top,
            card_bottom,
            input,
            early,
            late,
            tone,
            output,
            accent,
            logo,
            early_diffusion,
            late_diffusion,
            tex_shimmer,
            tex_mist,
            tex_drift,
            tex_crystal,
            tex_scatter,
            tex_bloom,
            tex_chime,
            velvet,
            numbox_bg,
            led_ring,
            led_off
        )
    }
}

impl Theme {
    pub fn greyed(&self) -> Theme {
        let grey = Color32::from_rgb(0x55, 0x58, 0x60);
        let grey_text = Color32::from_rgb(0x6E, 0x70, 0x78);
        Theme {
            bg_top: self.bg_top,
            bg_bottom: self.bg_bottom,
            line: self.line,
            txt: grey_text,
            dim: grey_text,
            dim2: grey_text,
            knob_bg: self.knob_bg,
            knob_border: self.knob_border,
            track_bg: self.track_bg,
            thumb_bg: self.thumb_bg,
            card_top: self.card_top,
            card_bottom: self.card_bottom,
            input: grey,
            early: grey,
            late: grey,
            tone: grey,
            output: grey,
            accent: grey,
            logo: self.logo,
            early_diffusion: grey,
            late_diffusion: grey,
            tex_shimmer: grey,
            tex_mist: grey,
            tex_drift: grey,
            tex_crystal: grey,
            tex_scatter: grey,
            tex_bloom: grey,
            tex_chime: grey,
            velvet: grey,
            numbox_bg: self.numbox_bg,
            led_ring: self.led_ring,
            led_off: self.led_off,
        }
    }
}

// ============================================================================
// Editor entry
// ============================================================================

pub fn create_editor(params: Arc<PejvakRvbParams>) -> Option<EguiEditor<PejvakEguiApp>> {
    // Read the current engine from the parameter set, so a fresh
    // editor opens showing the engine the host actually has
    // selected (rather than always starting on CLOUD).
    let initial_engine = params.ffdn.use_ffdn.value();

    let app = PejvakEguiApp {
        params: params.clone(),
        gui_context: None,
        current_preset: "Init".to_string(),
        user_presets: user_preset_names(),
        current_page: EditorPage::Main,
        pending_page: None,
        page_fade: 1.0,
        initial_size_applied: false,
        text_edit_active: false,
        engine_fade: 1.0,
        current_engine_ffdn: initial_engine,
        pending_engine: None,
        undo_stack: std::collections::VecDeque::with_capacity(UNDO_DEPTH),
        redo_stack: Vec::with_capacity(UNDO_DEPTH),
        last_snapshot: None,
        suppress_until: 0.0,
        was_focused: None,
        last_history_apply: 0.0,
    };

    let mut settings = EguiNiceSettings::default();
    settings.resize_hint.can_resize = true;
    settings.resize_hint.can_resize_horizontally = true;
    settings.resize_hint.can_resize_vertically = true;
    settings.resize_hint.preserve_aspect_ratio = true;
    settings.resize_hint.aspect_ratio_width = WINDOW_W;
    settings.resize_hint.aspect_ratio_height = WINDOW_H;

    create_egui_editor(
        params.editor_state.clone(),
        RepaintNotifier::default(),
        settings,
        app,
    )
}

/// Draws a minimalist sun icon: a filled centre disc surrounded by
/// eight evenly-spaced rays. Used inside the theme toggle for the
/// "Light" state.
fn draw_sun_icon(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    painter.circle_filled(center, radius * 0.40, color);

    let ray_inner = radius * 0.60;
    let ray_outer = radius * 0.94;
    let ray_stroke = Stroke::new(radius * 0.14, color);
    for i in 0..8 {
        let a = i as f32 * (PI * 2.0 / 8.0);
        let dir = Vec2::new(a.cos(), a.sin());
        painter.line_segment(
            [center + dir * ray_inner, center + dir * ray_outer],
            ray_stroke,
        );
    }
}

/// Draws a solid crescent-moon icon.
///
/// Uses the "overdraw" trick: paint a solid disc, then paint a second
/// disc in the button's background colour, offset to the right. The
/// visible remainder is a perfect crescent with two natural horns.
fn draw_moon_icon(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32, bg: Color32) {
    // 1. Solid disc — the moon.
    painter.circle_filled(center, radius, color);

    // 2. Bite out the crescent. Offset up-and-right so the horns sit
    //    at roughly the 1-o'clock and 5-o'clock positions.
    let cut_x = radius * 0.60;
    let cut_y = -radius * 0.10;
    let cut_r = radius * 0.95;
    painter.circle_filled(center + Vec2::new(cut_x, cut_y), cut_r, bg);
}

/// Small square button used for the Undo / Redo controls. Renders a
/// single glyph centred in a 30×30 rounded square.
///
/// When `enabled` is false the button is drawn dim and the click is
/// ignored.
/// Which arrow icon the button should draw.
#[derive(Clone, Copy)]
enum HistoryIcon {
    Undo,
    Redo,
}

/// Draws a curved "undo" arrow: a bezier curve starting at the
/// bottom-right, curving up and over, ending at the top-left with an
/// arrowhead pointing left. Mirrored vertically for the redo icon.
fn draw_history_icon(
    painter: &egui::Painter,
    center: Pos2,
    icon: HistoryIcon,
    color: Color32,
    scale: f32,
) {
    // SVG source: svgrepo.com/svg/389483/undo (viewBox 0 0 24 24)
    //
    // Path:
    //   M7.125 3.75 3 7.875 l4.125 4.125   ← arrowhead (3 points)
    //   M3 7.875 h11.25                     ← horizontal body line
    //   a4.875 4.875 0 1 1 0 9.75           ← 180° arc
    //   H9                                  ← tail
    //
    // We draw the arrowhead as a filled triangle and the rest as a
    // polyline. Redo is the same shape mirrored on the x-axis.
    let stroke = Stroke::new(1.4 * scale, color);
    let flip = match icon {
        HistoryIcon::Undo => 1.0_f32,
        HistoryIcon::Redo => -1.0_f32,
    };

    // Icon footprint = 24 SVG units mapped to ~15 px on screen at
    // scale 1.0. This is deliberately smaller than the segment so
    // the arrow has breathing room, matching the compact look of
    // the MAIN|TEXTURE tabs.
    let svg_scale = scale * 19.0 / 24.0;
    let to_screen = |x: f32, y: f32| -> Pos2 {
        Pos2::new(
            center.x + flip * (x - 12.0) * svg_scale,
            center.y + (y - 12.0) * svg_scale,
        )
    };

    // ---- Arrowhead (filled triangle) ----
    // Vertices from the SVG path: (7.125, 3.75), (3, 7.875), (7.125, 12).
    let ah_top = to_screen(7.125, 3.75);
    let ah_tip = to_screen(3.0, 7.875);
    let ah_bot = to_screen(7.125, 12.0);
    painter.add(egui::Shape::convex_polygon(
        vec![ah_top, ah_tip, ah_bot],
        color,
        egui::Stroke::NONE,
    ));

    // ---- Body: line + arc + tail ----
    // Start at the arrowhead tip (3, 7.875), go right to (14.25, 7.875),
    // sweep clockwise 180° to (14.25, 17.625), then left to (9, 17.625).
    let mut pts: Vec<Pos2> = Vec::with_capacity(24);
    pts.push(to_screen(3.0, 7.875));
    pts.push(to_screen(14.25, 7.875));

    // 180° arc, center (14.25, 12.75), radius 4.875.
    // Start angle -π/2 (top), end angle +π/2 (bottom).
    let arc_cx = 14.25_f32;
    let arc_cy = 12.75_f32;
    let arc_r = 4.875_f32;
    let steps = 20;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let angle = -std::f32::consts::FRAC_PI_2 + t * std::f32::consts::PI;
        pts.push(to_screen(
            arc_cx + arc_r * angle.cos(),
            arc_cy + arc_r * angle.sin(),
        ));
    }
    pts.push(to_screen(9.0, 17.625));

    painter.add(egui::Shape::line(pts, stroke));
}

/// Builds a short, human-readable summary of the differences between
/// two snapshots. Used by the Undo/Redo tooltips to tell the user
/// which parameters the next undo/redo will actually affect.
fn summarize_diff(a: &UndoSnapshot, b: &UndoSnapshot) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Engine switch (index 0 of the FFDN block).
    if (a.ffdn[0] - b.ffdn[0]).abs() > 0.5 {
        let target = if b.ffdn[0] > 0.5 { "FFDN" } else { "Cloud" };
        parts.push(format!("Engine → {}", target));
    }

    // CloudSeed params (49).
    for i in 0..49 {
        if (a.cloudseed[i] - b.cloudseed[i]).abs() > 1.0e-3 {
            if let Some(p) = fdn_reverb_core::cloudseed::Parameter::from_index(i) {
                parts.push(p.name().to_string());
            }
        }
    }

    // FFDN params (skip index 0 — handled above).
    for i in 1..37 {
        if (a.ffdn[i] - b.ffdn[i]).abs() > 1.0e-3 {
            parts.push(FFDN_PARAM_NAMES[i].to_string());
        }
    }

    // ILLUSION params.
    for i in 0..8 {
        if (a.illusion[i] - b.illusion[i]).abs() > 1.0e-3 {
            parts.push(CHIME_PARAM_NAMES[i].to_string());
        }
    }

    // Texture + Velvet params.
    for i in 0..32 {
        if (a.texture[i] - b.texture[i]).abs() > 1.0e-3 {
            parts.push(TEXTURE_PARAM_NAMES[i].to_string());
        }
    }

    // Boolean switches.
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
    if a.chime_enabled != b.chime_enabled {
        parts.push("ILLUSION".to_string());
    }
    if a.texture_enabled != b.texture_enabled {
        parts.push("Texture switches".to_string());
    }
    if a.texture_order != b.texture_order {
        parts.push("Signal Chain".to_string());
    }

    // Format: at most 3 items, then "+N more".
    const MAX: usize = 3;
    if parts.is_empty() {
        "no change".to_string()
    } else if parts.len() <= MAX {
        parts.join(", ")
    } else {
        format!("{}, +{} more", parts[..MAX].join(", "), parts.len() - MAX)
    }
}

/// Combined segmented Undo / Redo switch styled like the
/// MAIN|TEXTURE tabs: a single rounded track with two halves.
///
/// # Why one interact call, not two
///
/// Calling `ui.interact()` twice on two adjacent rects is unreliable
/// in an RTL parent layout: the hit-test ordering inside egui can
/// give priority to whichever call happened to be registered first,
/// and depending on the layout pass that is not always the one under
/// the pointer. The result was that clicks silently went to the
/// wrong segment or to neither.
///
/// This version allocates ONE `Sense::click()` rect for the whole
/// widget and then splits it into segments purely for drawing and
/// for determining which half was hit. Hover is also derived from
/// the pointer position rather than from per-segment responses, so
/// both halves respond identically regardless of layout direction.
fn history_switch(
    ui: &mut Ui,
    t: &Theme,
    undo_available: bool,
    redo_available: bool,
    undo_label: &str,
    redo_label: &str,
    scale: f32,
) -> (bool, bool) {
    let seg_w = 28.0 * scale;
    let seg_h = 24.0 * scale;
    let outer_pad = 2.0 * scale;
    let total_w = seg_w * 2.0 + outer_pad * 2.0;
    let total_h = seg_h + outer_pad * 2.0;
    let radius = 6.0 * scale;

    // Single clickable area for the whole widget.
    let (outer_rect, outer_resp) =
        ui.allocate_exact_size(Vec2::new(total_w, total_h), Sense::click());

    // Two halves of the track, purely for drawing and hit-testing.
    let r_undo = Rect::from_min_size(
        Pos2::new(outer_rect.left() + outer_pad, outer_rect.top() + outer_pad),
        Vec2::new(seg_w, seg_h),
    );
    let r_redo = Rect::from_min_size(
        Pos2::new(r_undo.right(), outer_rect.top() + outer_pad),
        Vec2::new(seg_w, seg_h),
    );

    // Which half is the pointer over?
    let hover_pos = ui.ctx().pointer_hover_pos();
    let hover_undo = hover_pos.map_or(false, |p| r_undo.contains(p));
    let hover_redo = hover_pos.map_or(false, |p| r_redo.contains(p));
    let show_popups = ui.memory(|m| {
        m.data
            .get_temp::<bool>(egui::Id::new("pejvak_show_popups"))
            .unwrap_or(true)
    });
    // Tooltip drawn manually: egui 0.36 no longer exposes
    // `show_tooltip_at_pointer`, and re-allocating a hover response
    // on top of the click rect would steal the click. Instead we
    // paint a small floating box near the pointer.
    if (hover_undo || hover_redo) && show_popups {
        // Compose the full text: action prefix + parameter list.
        let text = if hover_undo {
            if undo_available {
                if undo_label.is_empty() {
                    "Undo last change".to_string()
                } else {
                    format!("Undo: {}", undo_label)
                }
            } else {
                "Nothing to undo".to_string()
            }
        } else if redo_available {
            if redo_label.is_empty() {
                "Redo last change".to_string()
            } else {
                format!("Redo: {}", redo_label)
            }
        } else {
            "Nothing to redo".to_string()
        };

        if let Some(ptr) = ui.ctx().pointer_hover_pos() {
            let font = egui::FontId::proportional(11.0 * scale);
            let galley = ui.painter().layout_no_wrap(text.clone(), font, t.txt);
            let pad = egui::Vec2::new(8.0 * scale, 5.0 * scale);
            let tip_pos = ptr + egui::Vec2::new(12.0 * scale, 14.0 * scale);
            let tip_rect = egui::Rect::from_min_size(tip_pos, galley.size() + pad * 2.0);

            let tip_layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("history_tip"));
            let tip_painter = ui.ctx().layer_painter(tip_layer);

            tip_painter.rect_filled(tip_rect, 4.0 * scale, t.card_top);
            tip_painter.rect_stroke(
                tip_rect,
                4.0 * scale,
                egui::Stroke::new(1.0, t.line),
                egui::StrokeKind::Inside,
            );
            tip_painter.galley(tip_rect.min + pad, galley, t.txt);
        }
    }

    // Which half (if any) was clicked?
    let (clicked_undo, clicked_redo) = if outer_resp.clicked() {
        match outer_resp.interact_pointer_pos() {
            Some(p) if r_undo.contains(p) => (undo_available, false),
            Some(p) if r_redo.contains(p) => (false, redo_available),
            _ => (false, false),
        }
    } else {
        (false, false)
    };

    let painter = ui.painter();
    let inner_radius = radius - outer_pad * 0.4;

    // Track background.
    painter.rect_filled(outer_rect, radius, t.track_bg.gamma_multiply(0.55));

    // Hover tint. We show it on disabled halves too, but with a
    // dimmer tone, so the user can see the widget responds to the
    // cursor even when there is nothing to undo yet.
    if hover_undo {
        let mul = if undo_available { 0.14 } else { 0.05 };
        painter.rect_filled(r_undo, inner_radius, t.accent.gamma_multiply(mul));
    }
    if hover_redo {
        let mul = if redo_available { 0.14 } else { 0.05 };
        painter.rect_filled(r_redo, inner_radius, t.accent.gamma_multiply(mul));
    }

    // Icons. Use a generous icon scale so the arrows fill most of
    // their half — the segment height (24 px at scale 1.0) is the
    // same as the header tabs, so the visual rhythm matches.
    let icon_scale = scale * 0.90;

    let undo_col = if !undo_available {
        t.dim2.gamma_multiply(0.55)
    } else if hover_undo {
        t.accent
    } else {
        t.txt
    };
    let redo_col = if !redo_available {
        t.dim2.gamma_multiply(0.55)
    } else if hover_redo {
        t.accent
    } else {
        t.txt
    };

    draw_history_icon(
        painter,
        r_undo.center(),
        HistoryIcon::Undo,
        undo_col,
        icon_scale,
    );
    draw_history_icon(
        painter,
        r_redo.center(),
        HistoryIcon::Redo,
        redo_col,
        icon_scale,
    );

    (clicked_undo, clicked_redo)
}

/// Single-button theme toggle.
///
/// The button always displays the icon of the theme the user will
/// switch **to** — sun when currently dark, moon when currently light.
fn theme_toggle_button(
    ui: &mut Ui,
    t: &Theme,
    is_light: bool,
    p: &Arc<PejvakRvbParams>,
    scale: f32,
) {
    let size = 30.0 * scale;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());

    if resp.clicked() {
        p.is_light_theme
            .store(!is_light, std::sync::atomic::Ordering::Relaxed);
    }

    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter();
    let hover = resp.hovered();

    let bg_col = if hover {
        t.track_bg.gamma_multiply(0.80)
    } else {
        t.track_bg.gamma_multiply(0.55)
    };
    painter.rect_filled(rect, 6.0 * scale, bg_col);

    let icon_col = if hover { t.accent } else { t.dim };
    let icon_r = size * 0.25;
    if is_light {
        // Show moon in light mode (click to go dark).
        draw_moon_icon(painter, rect.center(), icon_r, icon_col, bg_col);
    } else {
        // Show sun in dark mode (click to go light).
        draw_sun_icon(painter, rect.center(), icon_r, icon_col);
    }

    let _ = hover_tip(
        resp,
        if is_light {
            "Switch to dark theme"
        } else {
            "Switch to light theme"
        },
    );

    if hover {
        ui.ctx().request_repaint();
    }
}

/// Compact CLOUD|FFDN engine selector styled to match the MAIN|TEXTURE
/// tabs and the Bypass/Freeze/Interp toggle group.
///
/// # Why this lives in the header
///
/// In the ENGINE card that used to sit at the bottom-right of
/// the MAIN page was removed to make room for the new FFDN NETWORK,
/// FFDN FILTER, and FFDN DYNAMICS cards. The engine selector that
/// used to be inside that card now lives here, in the header, where
/// it is always visible regardless of which page the user is on.
///
/// Visually it matches the MAIN|TEXTURE tabs: a rounded track with a
/// sliding accent highlight, and a smooth 280 ms animation when the
/// user clicks either segment.
fn engine_switch_header(
    ui: &mut Ui,
    t: &Theme,
    use_ffdn: bool,
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    scale: f32,
) {
    let labels = ["CLOUD", "FFDN"];
    let font = egui::FontId::proportional(11.5 * scale);
    let pad_x = 12.0 * scale;
    let seg_h = 24.0 * scale;
    let outer_pad = 3.0 * scale;
    let radius = 6.0 * scale;

    // Measure segment widths from label galleys so the two segments
    // are proportional to their text.
    let galleys: Vec<_> = labels
        .iter()
        .map(|l| {
            ui.painter()
                .layout_no_wrap(l.to_string(), font.clone(), Color32::WHITE)
        })
        .collect();
    let seg_widths: Vec<f32> = galleys.iter().map(|g| g.size().x + pad_x * 2.0).collect();
    let total_w: f32 = seg_widths.iter().sum::<f32>() + outer_pad * 2.0;
    let total_h = seg_h + outer_pad * 2.0;

    // Allocate the full switch as ONE widget. Everything below uses
    // absolute coordinates relative to `outer_rect`, which is
    // independent of the surrounding layout direction (LTR or RTL).
    let (outer_rect, _) = ui.allocate_exact_size(Vec2::new(total_w, total_h), Sense::hover());

    // Build segment rects left-to-right, always starting at the
    // *left* edge of `outer_rect`. This is what the fix is about:
    // in an RTL parent layout, using cursor-based allocation could
    // place rects where the click test didn't match the visual
    // position. Using absolute coordinates eliminates the ambiguity.
    let mut x = outer_rect.left() + outer_pad;
    let seg_rects: Vec<Rect> = seg_widths
        .iter()
        .map(|&w| {
            let r = Rect::from_min_size(
                Pos2::new(x, outer_rect.top() + outer_pad),
                Vec2::new(w, seg_h),
            );
            x += w;
            r
        })
        .collect();

    // ---- Interactions -----------------------------------------------
    // Collect click and hover state up front so the drawing pass can
    // use them. We use `interact` with explicit rects and unique IDs
    // so each segment gets its own hit region.
    let mut hovered = [false, false];
    let mut clicked: Option<bool> = None;
    for (i, seg_rect) in seg_rects.iter().enumerate() {
        let id = egui::Id::new(("engine_switch_header_hit", labels[i]));
        let resp = ui.interact(*seg_rect, id, Sense::click());
        if resp.clicked() {
            clicked = Some(i == 1);
        }
        hovered[i] = resp.hovered();
        let _ = hover_tip(
            resp,
            match i {
                0 => "Use the CloudSeed reverb engine",
                _ => "Use the FFDN (Filtered Feedback Delay Network) engine",
            },
        );
    }

    // use the click target IMMEDIATELY for the animation
    // and drawing, so the button responds on the same frame as the
    // click. The parameter change is still applied at the bottom of
    // this function, but the visuals no longer wait for the host to
    // reflect the new value — which on some DAWs takes a few frames
    // and made the button feel "sticky".
    let effective_use_ffdn = clicked.unwrap_or(use_ffdn);

    let anim = ui.ctx().animate_bool_with_time(
        egui::Id::new("engine_switch_header_anim"),
        effective_use_ffdn,
        0.28,
    );

    // ---- Draw -------------------------------------------------------
    let painter = ui.painter();
    painter.rect_filled(outer_rect, radius, t.track_bg.gamma_multiply(0.55));
    let inner_radius = radius - outer_pad * 0.4;

    for (i, seg_rect) in seg_rects.iter().enumerate() {
        let is_active = (i == 1) == effective_use_ffdn;
        if hovered[i] && !is_active {
            painter.rect_filled(*seg_rect, inner_radius, t.accent.gamma_multiply(0.10));
        }
    }

    let r_cloud = seg_rects[0];
    let r_ffdn = seg_rects[1];
    let highlighted = Rect::from_min_size(
        Pos2::new(
            r_cloud.left() + (r_ffdn.left() - r_cloud.left()) * anim,
            r_cloud.top(),
        ),
        Vec2::new(
            r_cloud.width() + (r_ffdn.width() - r_cloud.width()) * anim,
            r_cloud.height(),
        ),
    );
    painter.rect_filled(highlighted, inner_radius, t.accent.gamma_multiply(0.28));

    for (i, seg_rect) in seg_rects.iter().enumerate() {
        let dist = (anim - i as f32).abs();
        let active_amount = (1.0 - dist).clamp(0.0, 1.0);
        let text_col = lerp_color(t.dim, t.accent, active_amount);
        painter.text(
            seg_rect.center(),
            egui::Align2::CENTER_CENTER,
            labels[i],
            font.clone(),
            text_col,
        );
    }

    if anim > 0.001 && anim < 0.999 {
        ui.ctx().request_repaint();
    }

    // ---- Apply click ------------------------------------------------
    // Use `set_parameter` with an explicit begin/end pair so the
    // change is recorded as a discrete automation event.
    if let Some(target) = clicked {
        if target != use_ffdn {
            s.begin_set_parameter(&p.ffdn.use_ffdn);
            s.set_parameter(&p.ffdn.use_ffdn, target);
            s.end_set_parameter(&p.ffdn.use_ffdn);

            ui.ctx().request_repaint();
        }
    }
}

/// Draws a small square button that toggles all six Texture Rack
/// layers on or off at once.
///
/// Two states:
///
///   * **Inactive** (kill off): the button shows a plain `TX` label
///     in the theme's dim or accent colour. Clicking it saves the
///     current enable-state of all six layers and then disables
///     every layer.
///
///   * **Active** (kill on): the button pulses a soft red, giving
///     an unmistakable visual cue that the Texture section is
///     locked. Clicking it restores the saved enable-state and
///     releases the lock.
///
/// While the kill switch is active, the individual Enable switches
/// inside each Texture card are non-interactive (see
/// `section_title_with_enabled_switch`); the user must release the
/// kill switch first. This is purely a UI lock — the underlying
/// parameters remain automatable by the host.
fn texture_kill_button(
    ui: &mut Ui,
    t: &Theme,
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    scale: f32,
) {
    let size = 30.0 * scale;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());

    // The "saved" array captures the enable-state at the moment the
    // kill was engaged, so it can be restored on the next click.
    let kill_id = egui::Id::new("tx_kill_state");
    let saved: Option<[bool; 7]> = ui.memory(|m| m.data.get_temp(kill_id));

    // Current enable-state of all seven layers (including Illusion).
    let current = [
        p.shimmer.enabled.value(),
        p.mist.enabled.value(),
        p.drift.enabled.value(),
        p.crystal.enabled.value(),
        p.scatter.enabled.value(),
        p.bloom.enabled.value(),
        p.illusion.enabled.value(),
    ];
    let any_enabled = current.iter().any(|&b| b);
    let all_off = !any_enabled;

    // Derive "kill active" from the params, not just from memory.
    // ...
    let kill_active = saved.is_some() && all_off;

    // Publish the derived state for the card switches below.
    ui.memory_mut(|m| {
        m.data.insert_temp(
            egui::Id::new("pejvak_tx_kill_active"),
            kill_active,
        );
    });

    // ---- Pending-commit guard ---------------------------------------
    //
    // While a previous kill-toggle's undo commit is still scheduled
    // (100 ms window), ignore additional clicks. Without this guard,
    // a fast double-click would re-enter the `else` branch on the
    // second click while `p.*.enabled.value()` still reports the
    // pre-toggle state (nice-plug propagates BoolParams
    // asynchronously), overwriting the saved-state memory with a
    // stale snapshot and silently losing the user's previous
    // configuration.
    let now = ui.ctx().input(|i| i.time);
    let commit_at: f64 = ui.memory(|m| {
        m.data
            .get_temp(egui::Id::new("pejvak_force_undo_commit_at"))
            .unwrap_or(0.0)
    });
    let commit_pending = commit_at > 0.0 && now < commit_at;

    // ---- Click handling ---------------------------------------------
    if resp.clicked() && !commit_pending {
        if kill_active {
            if let Some(saved) = saved {
                set_all_texture_enabled(s, p, saved);
            }
            ui.memory_mut(|m| m.data.remove::<[bool; 7]>(kill_id));
        } else {
            // Always allow engaging the lock...
            ui.memory_mut(|m| m.data.insert_temp(kill_id, current));
            set_all_texture_enabled(s, p, [false; 7]);
        }
        // Schedule a forced undo commit 100 ms from now. The delay
        // is important: nice-plug propagates `BoolParam` updates
        // asynchronously, and reading them back in the same frame
        // (or even the next one) can return stale values. 100 ms is
        // imperceptible to the user and reliably past the propagation
        // window on every host we have tested.
        //
        // `now` is reused from above rather than re-queried inside
        // the memory lock, so the two egui locks (input and memory)
        // are never held simultaneously.
        ui.memory_mut(|m| {
            m.data.insert_temp(
                egui::Id::new("pejvak_force_undo_commit_at"),
                now + 0.10,
            );
        });
    }

    // ---- Draw -------------------------------------------------------
    let hover = resp.hovered();
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        if kill_active {

            painter.rect_filled(rect, 6.0 * scale, KILL_RED.gamma_multiply(0.10));

            let inner_rect = rect.shrink(3.0 * scale);

            let t_now = ui.ctx().input(|i| i.time) as f32;
            let pulse = ((t_now * 7.0).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            let bg_intensity = 0.20 + pulse * 0.25;
            painter.rect_filled(
                inner_rect,
                4.0 * scale,
                KILL_RED.gamma_multiply(bg_intensity),
            );

            // Label on top of both.
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "TX",
                egui::FontId::proportional(11.0 * scale),
                KILL_RED,
            );
        } else {
            let bg = if hover {
                t.track_bg.gamma_multiply(0.85)
            } else {
                t.track_bg.gamma_multiply(0.55)
            };
            painter.rect_filled(rect, 6.0 * scale, bg);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "TX",
                egui::FontId::proportional(11.0 * scale),
                t.dim,
            );
        }
    }

    // ---- Tooltip ----------------------------------------------------
    let tooltip = if kill_active {
        "Release the lock and restore the previous Texture layers"
    } else {
        "Lock all Texture layers off (blocks UI toggles)"
    };
    let _ = hover_tip(resp, tooltip);

    if kill_active || hover {
        ui.ctx().request_repaint();
    }
}

/// Applies the given enable-state to all six Texture Rack layers.
///
/// Each assignment is wrapped in `begin_set_parameter` /
/// `end_set_parameter` so the host records it as a discrete
/// automation event, exactly as if the user had clicked each layer's
/// own Enable switch.
/// Applies the given enable-state to all seven Texture layers
/// (including ILLUSION). Each assignment is wrapped in `begin_set_parameter`
/// / `end_set_parameter` so the host records it as a discrete
/// automation event, exactly as if the user had clicked each layer's
/// own Enable switch.
fn set_all_texture_enabled(s: &ParamSetter, p: &Arc<PejvakRvbParams>, state: [bool; 7]) {
    macro_rules! set {
        ($param:expr, $val:expr) => {{
            s.begin_set_parameter(&$param);
            s.set_parameter(&$param, $val);
            s.end_set_parameter(&$param);
        }};
    }
    set!(p.shimmer.enabled, state[0]);
    set!(p.mist.enabled, state[1]);
    set!(p.drift.enabled, state[2]);
    set!(p.crystal.enabled, state[3]);
    set!(p.scatter.enabled, state[4]);
    set!(p.bloom.enabled, state[5]);
    set!(p.illusion.enabled, state[6]);
}

/// Conditionally attaches a hover tooltip. Respects the "Show pop-ups"
/// toggle in the plugin menu; when off, the returned `Response` is
/// identical to the input and no tooltip is attached.
///
/// Uses `resp.ctx` so callers do not need to pass `ui` around — every
/// existing `.on_hover_text("...")` becomes `hover_tip(resp, "...")`.
#[inline]
fn hover_tip(resp: egui::Response, text: impl Into<String>) -> egui::Response {
    let show = resp.ctx.memory(|m| {
        m.data
            .get_temp::<bool>(egui::Id::new("pejvak_show_popups"))
            .unwrap_or(true)
    });
    if show {
        // Convert to String first — String implements Into<WidgetText>.
        resp.on_hover_text(text.into())
    } else {
        resp
    }
}

/// Draws the app menu button (hamburger icon) and its popup.
///
/// The popup lists four items stacked vertically: Manual, Size,
/// About, and Check for updates. Selecting an item opens the
/// corresponding modal (or triggers the update check placeholder).
fn menu_button(ui: &mut Ui, t: &Theme, scale: f32, p: &Arc<PejvakRvbParams>) {
    let size = 30.0 * scale;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());

    let id = resp.id;
    let open_id = id.with("open");
    let modal_id = egui::Id::new("pejvak_modal_panel");

    let mut open: bool = ui.memory(|m| m.data.get_temp(open_id).unwrap_or(false));
    if resp.clicked() {
        open = !open;
    }

    // ---- Button (hamburger icon) ----
    let hover = resp.hovered();
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let bg_col = if hover || open {
            t.track_bg.gamma_multiply(0.80)
        } else {
            t.track_bg.gamma_multiply(0.55)
        };
        painter.rect_filled(rect, 6.0 * scale, bg_col);

        let icon_col = if hover || open { t.accent } else { t.dim };
        let line_w = size * 0.48;
        let line_h = 1.5 * scale;
        let line_gap = size * 0.19;
        let cx = rect.center().x;
        let cy = rect.center().y;
        for i in 0..3 {
            let y = cy + (i as f32 - 1.0) * line_gap;
            let line_rect = Rect::from_center_size(Pos2::new(cx, y), Vec2::new(line_w, line_h));
            painter.rect_filled(line_rect, line_h * 0.5, icon_col);
        }
    }

    let _ = hover_tip(resp, "Plugin menu");
    ui.memory_mut(|m| m.data.insert_temp(open_id, open));

    if !open {
        return;
    }

    // ========================================================================
    // Popup + Size flyout
    // ========================================================================
    let popup_w = 220.0 * scale;
    let row_h = 26.0 * scale;
    let pad = 6.0 * scale;
    let items = ["Manual", "Size", "About", "Show pop-ups"];
    let popup_h = row_h * items.len() as f32 + pad * 2.0;

    let popup_pos = Pos2::new((rect.right() - popup_w).max(4.0), rect.bottom() + 4.0);
    let popup_rect = Rect::from_min_size(popup_pos, Vec2::new(popup_w, popup_h));

    // Size row (index 1). Pre-compute for bridge / submenu logic.
    let size_row_rect = Rect::from_min_size(
        Pos2::new(popup_pos.x + pad, popup_pos.y + pad + row_h),
        Vec2::new(popup_w - 2.0 * pad, row_h),
    );

    let is_light = t.bg_top.r() > 128;
    let bg = if is_light {
        Color32::from_rgb(0xF6, 0xF7, 0xFA)
    } else {
        Color32::from_rgb(0x1A, 0x21, 0x36)
    };
    let base_col = if is_light {
        Color32::from_rgb(0x1D, 0x21, 0x29)
    } else {
        t.txt
    };
    let shadow_col = if is_light {
        Color32::from_rgba_unmultiplied(0, 0, 0, 50)
    } else {
        Color32::from_rgba_unmultiplied(0, 0, 0, 110)
    };

    let size_open_id = id.with("size_submenu_open");
    let prev_open: bool = ui.memory(|m| m.data.get_temp(size_open_id).unwrap_or(false));

    let mut clicked_item: Option<usize> = None;
    let mut clicked_size: Option<u32> = None;
    let mut set_default_clicked = false;
    let mut size_row_hovered = false;
    let mut other_row_hovered = false;

    // ---- Main popup ----
    egui::Area::new(id.with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(popup_pos)
        .show(ui.ctx(), |ui| {
            let frame = egui::Frame::NONE
                .fill(bg)
                .stroke(Stroke::new(1.0, t.line))
                .corner_radius(8.0 * scale)
                .shadow(egui::epaint::Shadow {
                    offset: [0, 6],
                    blur: 110,
                    spread: 0,
                    color: shadow_col,
                })
                .inner_margin(egui::Margin::same(pad.round() as i8));

            frame.show(ui, |ui| {
                ui.set_min_size(Vec2::new(popup_w - pad * 2.0, popup_h - pad * 2.0));
                ui.set_max_size(Vec2::new(popup_w - pad * 2.0, popup_h - pad * 2.0));

                let show_popups_now = ui.memory(|m| {
                    m.data
                        .get_temp::<bool>(egui::Id::new("pejvak_show_popups"))
                        .unwrap_or(true)
                });

                for (i, label) in items.iter().enumerate() {
                    let (r, r_resp) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), row_h),
                        Sense::click(),
                    );
                    let is_size_row = i == 1;
                    let is_toggle_row = i == 3;

                    if is_size_row {
                        if r_resp.hovered() {
                            size_row_hovered = true;
                        }
                    } else if r_resp.hovered() {
                        other_row_hovered = true;
                    }

                    if r_resp.hovered() {
                        ui.painter()
                            .rect_filled(r, 4.0 * scale, t.accent.gamma_multiply(0.15));
                    }

                    ui.painter().text(
                        Pos2::new(r.left() + 12.0 * scale, r.center().y),
                        egui::Align2::LEFT_CENTER,
                        *label,
                        egui::FontId::proportional(11.5 * scale),
                        base_col,
                    );

                    if is_size_row {
                        let arrow_c = Pos2::new(r.right() - 12.0 * scale, r.center().y);
                        let col = if r_resp.hovered() { t.accent } else { t.dim2 };
                        ui.painter().add(egui::Shape::convex_polygon(
                            vec![
                                Pos2::new(arrow_c.x - 2.0 * scale, arrow_c.y - 4.0 * scale),
                                Pos2::new(arrow_c.x - 2.0 * scale, arrow_c.y + 4.0 * scale),
                                Pos2::new(arrow_c.x + 3.0 * scale, arrow_c.y),
                            ],
                            col,
                            Stroke::NONE,
                        ));
                    } else if is_toggle_row {
                        // Checkmark on the right edge.
                        let check_c = Pos2::new(r.right() - 14.0 * scale, r.center().y);
                        if show_popups_now {
                            let s = 3.5 * scale;
                            ui.painter().line_segment(
                                [
                                    Pos2::new(check_c.x - s, check_c.y),
                                    Pos2::new(check_c.x - s * 0.3, check_c.y + s * 0.7),
                                ],
                                Stroke::new(1.8 * scale, t.accent),
                            );
                            ui.painter().line_segment(
                                [
                                    Pos2::new(check_c.x - s * 0.3, check_c.y + s * 0.7),
                                    Pos2::new(check_c.x + s, check_c.y - s * 0.6),
                                ],
                                Stroke::new(1.8 * scale, t.accent),
                            );
                        }
                        if r_resp.clicked() {
                            // Toggle the params flag; sync into memory
                            // immediately so `hover_tip` sees the
                            // change on the same frame.
                            let new_val = !show_popups_now;
                            p.show_popups.store(new_val, Ordering::Relaxed);
                            ui.memory_mut(|m| {
                                m.data
                                    .insert_temp(egui::Id::new("pejvak_show_popups"), new_val);
                            });
                        }
                    } else if r_resp.clicked() {
                        clicked_item = Some(i);
                    }
                }
            });
        });

    // ---- Size flyout submenu ----
    //
    // The submenu stays open while the pointer is anywhere inside the
    // union of the popup and the submenu. That bounding box covers
    // the gap between the Size row and the submenu, so the pointer
    // never crosses a "dead" region on its way over.

    // Pre-compute the submenu geometry so the safe zone can include it.
    let sub_w = 150.0 * scale;
    let sub_row_h = 24.0 * scale;
    let sub_pad = 6.0 * scale;
    let n_presets = UI_SCALE_PRESETS.len();
    let sub_h = sub_row_h * (n_presets as f32 + 1.8) + sub_pad * 2.0;

    let preferred_x = popup_pos.x + popup_w + 4.0 * scale;
    let screen = ui.max_rect();
    let sub_x = if preferred_x + sub_w > screen.right() - 4.0 {
        (popup_pos.x - sub_w - 4.0 * scale).max(4.0)
    } else {
        preferred_x
    };
    let sub_pos = Pos2::new(sub_x, size_row_rect.top());
    let sub_rect = Rect::from_min_size(sub_pos, Vec2::new(sub_w, sub_h));

    // Safe zone = bounding box of popup and submenu. Any pointer
    // position inside this rectangle keeps the submenu alive.
    let safe_zone = popup_rect.union(sub_rect);

    let pointer_pos = ui.input(|i| i.pointer.hover_pos());
    let pointer_in_safe_zone = pointer_pos.map_or(false, |p| safe_zone.contains(p));

    // Hovering any non-Size row immediately closes the flyout. Without
    // this guard, the safe zone (popup ∪ submenu) would keep the
    // submenu open while the pointer travels over Manual or About.
    let show_submenu =
        size_row_hovered || (prev_open && pointer_in_safe_zone && !other_row_hovered);

    if show_submenu {
        egui::Area::new(id.with("size_area"))
            .order(egui::Order::Foreground)
            .fixed_pos(sub_pos)
            .show(ui.ctx(), |ui| {
                let frame = egui::Frame::NONE
                    .fill(bg)
                    .stroke(Stroke::new(1.0, t.line))
                    .corner_radius(8.0 * scale)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 6],
                        blur: 110,
                        spread: 0,
                        color: shadow_col,
                    })
                    .inner_margin(egui::Margin::same(sub_pad.round() as i8));

                frame.show(ui, |ui| {
                    ui.set_min_size(Vec2::new(sub_w - sub_pad * 2.0, sub_h - sub_pad * 2.0));
                    ui.set_max_size(Vec2::new(sub_w - sub_pad * 2.0, sub_h - sub_pad * 2.0));

                    for &(pct, label) in UI_SCALE_PRESETS {
                        let (r, r_resp) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), sub_row_h),
                            Sense::click(),
                        );
                        if r_resp.hovered() {
                            ui.painter()
                                .rect_filled(r, 4.0 * scale, t.accent.gamma_multiply(0.15));
                        }
                        ui.painter().text(
                            Pos2::new(r.left() + 10.0 * scale, r.center().y),
                            egui::Align2::LEFT_CENTER,
                            label,
                            egui::FontId::proportional(11.0 * scale),
                            base_col,
                        );
                        if r_resp.clicked() {
                            clicked_size = Some(pct);
                        }
                    }

                    ui.add_space(2.0 * scale);
                    ui.separator();
                    ui.add_space(2.0 * scale);

                    let (r, r_resp) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), sub_row_h),
                        Sense::click(),
                    );
                    if r_resp.hovered() {
                        ui.painter()
                            .rect_filled(r, 4.0 * scale, t.accent.gamma_multiply(0.15));
                    }
                    ui.painter().text(
                        Pos2::new(r.left() + 10.0 * scale, r.center().y),
                        egui::Align2::LEFT_CENTER,
                        "Set as default size",
                        egui::FontId::proportional(10.5 * scale),
                        if r_resp.hovered() { t.accent } else { base_col },
                    );
                    if r_resp.clicked() {
                        set_default_clicked = true;
                    }
                });
            });
    }

    ui.memory_mut(|m| m.data.insert_temp(size_open_id, show_submenu));

    // ---- Apply selections ----
    if let Some(idx) = clicked_item {
        ui.memory_mut(|m| {
            m.data.insert_temp(open_id, false);
            m.data.insert_temp(size_open_id, false);
        });
        let now = ui.input(|i| i.time);
        match idx {
            0 => {
                // Manual opens the bundled HTML file in the user's
                // default browser instead of showing a modal. The
                // browser gives a much richer reading experience
                // (search, TOC, dark/light themes, print-to-PDF) and
                // costs nothing at runtime — the HTML is embedded in
                // the binary and simply written to disk on first use.
                crate::manual::open_in_browser();
            }
            2 => {
                ui.memory_mut(|m| {
                    m.data.insert_temp(modal_id, ModalPanel::About);
                    m.data.insert_temp(modal_id.with("opened_at"), now);
                    m.data.remove::<f64>(modal_id.with("closing_at"));
                });
            }
            _ => {}
        }
    }

    if let Some(pct) = clicked_size {
        ui.memory_mut(|m| {
            m.data.insert_temp(open_id, false);
            m.data.insert_temp(size_open_id, false);
        });
        let target_w = (WINDOW_W as f32 * pct as f32 / 100.0).round().max(200.0);
        let target_h = (WINDOW_H as f32 * pct as f32 / 100.0).round().max(120.0);
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                target_w, target_h,
            )));
    }

    if set_default_clicked {
        let screen = ui.max_rect();
        let w = screen.width().round().max(200.0) as u32;
        let h = screen.height().round().max(120.0) as u32;
        crate::save_default_size(w, h);
        ui.memory_mut(|m| {
            m.data.insert_temp(open_id, false);
            m.data.insert_temp(size_open_id, false);
        });
    }

    // ---- Close on outside click ----
    let in_popup_or_submenu =
        |p: Pos2| popup_rect.contains(p) || rect.contains(p) || sub_rect.contains(p);
    if ui.input(|i| i.pointer.any_click()) {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            if !in_popup_or_submenu(pos) {
                ui.memory_mut(|m| {
                    m.data.insert_temp(open_id, false);
                    m.data.insert_temp(size_open_id, false);
                });
            }
        }
    }
}

pub struct PejvakEguiApp {
    params: Arc<PejvakRvbParams>,
    gui_context: Option<GuiContext>,
    current_preset: String,
    user_presets: Vec<String>,
    current_page: EditorPage,
    pending_page: Option<EditorPage>,
    page_fade: f32,
    initial_size_applied: bool,
    text_edit_active: bool,
    engine_fade: f32,
    current_engine_ffdn: bool,
    pending_engine: Option<bool>,
    undo_stack: std::collections::VecDeque<UndoSnapshot>,
    redo_stack: Vec<UndoSnapshot>,
    last_snapshot: Option<UndoSnapshot>,
    suppress_until: f64,
    was_focused: Option<bool>,
    last_history_apply: f64,
}

impl NiceEguiApp for PejvakEguiApp {
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        nice_gui_ctx: GuiContext,
        _frame: &mut nice_plug_egui::Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        self.gui_context = Some(nice_gui_ctx);

        // ---- Font / shape anti-aliasing ----------------------------
        egui_ctx.tessellation_options_mut(|opts| {
            opts.feathering = true;
            opts.feathering_size_in_pixels = 1.0;
        });

        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "Monoton".to_owned(),
            Arc::new(egui::FontData::from_static(include_bytes!(
                "../fonts/Monoton-Regular.ttf"
            ))),
        );
        fonts.families.insert(
            egui::FontFamily::Name("Monoton".into()),
            vec!["Monoton".to_owned()],
        );
        egui_ctx.set_fonts(fonts);
        Ok(())
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut nice_plug_egui::Frame) {
        let gui_ctx = match self.gui_context.clone() {
            Some(c) => c,
            None => return,
        };
        let setter = gui_ctx.param_setter();

        // Cache the persisted "show pop-ups" flag in egui memory so the
        // `hover_tip` helper can read it without needing the params.
        let show_popups = self.params.show_popups.load(Ordering::Relaxed);
        ui.memory_mut(|m| {
            m.data
                .insert_temp(egui::Id::new("pejvak_show_popups"), show_popups);
        });

        ui.ctx().all_styles_mut(|s| {
            s.interaction.selectable_labels = false;
        });

        // Force a burst of repaints when the window regains focus, so the
        // swap chain is rebuilt and the first frame appears immediately
        // instead of waiting for the next input event.
        let focused = ui.ctx().input(|i| i.focused);
        if self.was_focused == Some(false) && focused {
            ui.ctx().request_repaint();
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(120));
        }
        self.was_focused = Some(focused);

        // ============================================================
        // Undo / Redo — leading-edge commit with suppression window
        // ============================================================
        //
        let now = ui.input(|i| i.time);
        let user_active = ui.input(|i| {
            i.pointer.any_down()
                || i.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::Key { .. }
                            | egui::Event::MouseWheel { .. }
                            | egui::Event::Text(_)
                            | egui::Event::Paste(_)
                    )
                })
        });
        // Extend the suppression deadline while the user is interacting,
        // so a drag commits once on release instead of every 250 ms.
        if user_active {
            self.suppress_until = now + UNDO_SUPPRESS_S;
        }

        let current = capture_undo_snapshot(&self.params, &self.current_preset, self.current_page);
        let has_change = self
            .last_snapshot
            .as_ref()
            .map_or(false, |prev| snapshots_differ(prev, &current, 1.0e-4));

        let history_grace = (now - self.last_history_apply) < 0.8;
        let should_absorb = !has_change || (history_grace && !user_active);

        if self.last_snapshot.is_none() {
            self.last_snapshot = Some(current);
        } else if has_change && now >= self.suppress_until && !should_absorb {
            // Commit the previous state as a single undo step.
            let prev_owned = self.last_snapshot.take().unwrap();
            self.undo_stack.push_back(prev_owned);
            while self.undo_stack.len() > UNDO_DEPTH {
                self.undo_stack.pop_front();
            }
            self.redo_stack.clear();
            self.suppress_until = now + UNDO_SUPPRESS_S;
            self.last_snapshot = Some(current);
        } else if should_absorb {
            // Absorb the current state into the baseline without
            // creating an undo entry.
            self.last_snapshot = Some(current);
        }

        // ---- One-shot: apply the user's preferred window size ------
        if !self.initial_size_applied {
            self.initial_size_applied = true;
            let w = self.params.default_width.load(Ordering::Relaxed);
            let h = self.params.default_height.load(Ordering::Relaxed);
            if w > 0 && h > 0 {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::Vec2::new(
                        w as f32, h as f32,
                    )));
            }
        }

        // Refresh the cached flag from egui memory. The widgets set
        // this value when they enter / leave edit mode.
        self.text_edit_active = Self::read_text_edit_active(ui);

        self.poll_preset_dialog(&setter);
        self.user_presets = user_preset_names();

        let is_light = self
            .params
            .is_light_theme
            .load(std::sync::atomic::Ordering::Relaxed);

        let current_size = ui.max_rect().size();
        let last_size_id = egui::Id::new("pejvak_last_window_size");
        let last_size: Option<Vec2> = ui.memory(|m| m.data.get_temp(last_size_id));
        if last_size != Some(current_size) {
            ui.memory_mut(|m| m.data.insert_temp(last_size_id, current_size));
            ui.ctx().request_repaint();
        }

        if ui.input(|i| i.pointer.any_down()) {
            ui.ctx().request_repaint();
        }

        let dt = ui.input(|i| i.stable_dt).clamp(0.0, 0.1);
        let step = dt / PAGE_FADE_SECS;
        // separate step for the engine fade so CLOUD↔FFDN
        // animates at its own (slower) pace.
        let engine_step = dt / ENGINE_FADE_SECS;

        if let Some(target) = self.pending_page {
            self.page_fade = (self.page_fade - step).max(0.0);
            if self.page_fade <= 0.0 {
                self.current_page = target;
                self.pending_page = None;
            }
            ui.ctx().request_repaint();
        } else if self.page_fade < 1.0 {
            self.page_fade = (self.page_fade + step).min(1.0);
            ui.ctx().request_repaint();
        }

        // ============================================================
        // two-phase CLOUD ↔ FFDN animation
        // ============================================================
        //
        // Same shape as the MAIN ↔ TEXTURE animation above:
        //
        //   1. When the user picks a different engine (parameter
        //      change), we latch the target in `pending_engine`.
        //   2. Every frame, `engine_fade` ramps toward 0.0.
        //   3. The instant it hits 0.0 we flip `current_engine_ffdn`
        //      and clear `pending_engine`.
        //   4. `engine_fade` then ramps back up toward 1.0.
        //
        // The parameter (`params.ffdn.use_ffdn`) and the visual
        // state (`current_engine_ffdn`) are deliberately decoupled:
        // the parameter updates immediately for the DSP, but the
        // cards you see only swap at the bottom of the fade — which
        // is what produces the smooth "dissolve" effect.
        let param_engine = self.params.ffdn.use_ffdn.value();
        let visual_target = self.pending_engine.unwrap_or(self.current_engine_ffdn);

        if param_engine != visual_target {
            if param_engine == self.current_engine_ffdn {
                // User clicked back to the currently-drawn engine
                // before the fade finished — cancel the transition.
                self.pending_engine = None;
            } else {
                // Start (or retarget) a transition.
                self.pending_engine = Some(param_engine);
            }
        }

        if let Some(target) = self.pending_engine {
            // Fade out the current engine's cards.
            // uses `engine_step` (slower) instead of `step`.
            self.engine_fade = (self.engine_fade - engine_step).max(0.0);
            if self.engine_fade <= 0.0 {
                // Bottom of the fade: swap engine and start
                // fading the new cards in on the next frame.
                self.current_engine_ffdn = target;
                self.pending_engine = None;
            }
            ui.ctx().request_repaint();
        } else if self.engine_fade < 1.0 {
            // Fade in the new engine's cards.
            self.engine_fade = (self.engine_fade + engine_step).min(1.0);
            ui.ctx().request_repaint();
        }

        let params = self.params.clone();
        let user_presets = self.user_presets.clone();
        let mut current_preset = std::mem::take(&mut self.current_preset);
        let mut current_page = self.current_page;
        let mut pending_page = self.pending_page;
        let fade = self.page_fade;
        let engine_fade = self.engine_fade;
        let use_ffdn_visual = self.current_engine_ffdn;

        // Pre-compute button availability once, so the closure can
        // read them without re-borrowing `self`.
        let undo_available_pre = !self.undo_stack.is_empty();
        let redo_available_pre = !self.redo_stack.is_empty();

        // Build human-readable labels describing which parameters
        // the next undo/redo will affect. Diff is taken against the
        // current baseline (`last_snapshot`) so the labels stay in
        // sync with what the user actually sees right now.
        let undo_label = self
            .undo_stack
            .back()
            .and_then(|target| {
                self.last_snapshot
                    .as_ref()
                    .map(|cur| summarize_diff(cur, target))
            })
            .unwrap_or_default();
        let redo_label = self
            .redo_stack
            .last()
            .and_then(|target| {
                self.last_snapshot
                    .as_ref()
                    .map(|cur| summarize_diff(cur, target))
            })
            .unwrap_or_default();

        let mut pending_action: Option<HistoryAction> = None;

        show_resizable_delta(
            ui,
            Vec2::new(WINDOW_W as f32 * 0.70, WINDOW_H as f32 * 0.70),
            egui::Id::new("pejvak_rvb_resizable"),
            |ui| {
                let theme = {
                    let t = ui.ctx().animate_bool_with_time(
                        egui::Id::new("pejvak_theme_fade"),
                        is_light,
                        0.28,
                    );
                    Theme::dark().lerp(&Theme::light(), t)
                };

                let screen = ui.max_rect();
                let sx = screen.width() / WINDOW_W as f32;
                let sy = screen.height() / WINDOW_H as f32;
                let scale = sx.min(sy).clamp(0.45, 2.6);

                let mut style: egui::Style = (**ui.style()).clone();
                style.visuals.dark_mode = !is_light;
                style.visuals.panel_fill = Color32::TRANSPARENT;
                style.visuals.window_fill = theme.bg_top;
                style.visuals.override_text_color = Some(theme.txt);
                style.spacing.item_spacing = Vec2::new(PAGE_GAP * scale, PAGE_GAP * scale);
                ui.set_style(style);

                let mut mesh = egui::Mesh::default();
                mesh.colored_vertex(screen.left_top(), theme.bg_top);
                mesh.colored_vertex(screen.right_top(), theme.bg_top);
                mesh.colored_vertex(screen.right_bottom(), theme.bg_bottom);
                mesh.colored_vertex(screen.left_bottom(), theme.bg_bottom);
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                ui.painter().add(egui::Shape::mesh(mesh));

                draw_editor(
                    ui,
                    &params,
                    &setter,
                    &theme,
                    scale,
                    is_light,
                    &mut current_preset,
                    &user_presets,
                    &mut current_page,
                    &mut pending_page,
                    fade,
                    engine_fade,
                    use_ffdn_visual,
                    undo_available_pre,
                    redo_available_pre,
                    &undo_label,
                    &redo_label,
                    &mut pending_action,
                );

                // Draw the modal panel inside this closure so that
                // `ui.max_rect()` is the true plugin area, not a
                // shrunk rect left over from a previous layout pass.
                draw_modal_panel(ui, &theme, scale);
            },
        );


        // ============================================================
        // Force-commit check — reads a flag set on the PREVIOUS frame
        // ============================================================
        //
        // Widgets like the TX kill button request an immediate undo
        // commit by setting `pejvak_force_undo_commit` to true. We
        // check that flag here — at the *top* of the next frame —
        // because nice-plug propagates `BoolParam::value()` asynchronously.
        // Checking it in the same frame the click happened could read
        // the old parameter values and silently drop the undo entry.
        // One frame of delay is imperceptible to the user and fully
        // reliable.
        {
            let force_id = egui::Id::new("pejvak_force_undo_commit_at");
            let commit_at: f64 = ui.memory(|m| m.data.get_temp(force_id).unwrap_or(0.0));
            let now = ui.input(|i| i.time);

            if commit_at > 0.0 {
                if now >= commit_at {
                    ui.memory_mut(|m| m.data.remove::<f64>(force_id));

                    let current = capture_undo_snapshot(
                        &self.params,
                        &current_preset,
                        self.current_page,
                    );
                    if let Some(prev) = self.last_snapshot.take() {
                        if snapshots_differ(&prev, &current, 1.0e-4) {
                            self.undo_stack.push_back(prev);
                            while self.undo_stack.len() > UNDO_DEPTH {
                                self.undo_stack.pop_front();
                            }
                            self.redo_stack.clear();
                            self.suppress_until = now + UNDO_SUPPRESS_S;
                            self.last_snapshot = Some(current);
                        } else {
                            self.last_snapshot = Some(prev);
                        }
                    } else {
                        self.last_snapshot = Some(current);
                    }
                } else {
                    // Still pending — keep the frame loop alive so the
                    // timer can fire even when the user is not
                    // interacting. Crucially, the repaint is only
                    // requested while the commit is ACTUALLY pending;
                    // the previous code used a stale local variable
                    // that had already been removed from memory.
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(20));
                }
            }
        }

        // ============================================================
        // Undo / Redo — trailing-edge commit with suppression window
        // ============================================================
        if let Some(act) = pending_action {
            let now = ui.input(|i| i.time);
            let result = match act {
                HistoryAction::Undo => self.do_undo(&setter, now),
                HistoryAction::Redo => self.do_redo(&setter, now),
            };
            if let Some((name, target_page)) = result {
                current_preset = name;
                // If the snapshot was taken on a different page,
                // schedule a page transition with the same two-phase
                // fade used by tab clicks.
                if target_page != current_page && pending_page.is_none() {
                    pending_page = Some(target_page);
                }
            }
            ui.ctx().request_repaint();
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }

        // The modal panel (Manual / About) is already drawn inside
        // the `show_resizable_delta` closure above, so that
        // `ui.max_rect()` returns the true plugin area. The previous
        // code also drew it here, which registered TWO `Area`
        // widgets with the same `Id` in a single frame — an egui
        // anti-pattern that can cause erratic layout behaviour and
        // extra work per frame. Removed.

        // forward keyboard events to the DAW.
        //
        // We forward only when a TextEdit is NOT active, so typed
        // characters stay inside the numbox / search field. This
        // bypasses `baseview`'s missing parent-forwarding entirely.
        //
        // The `set_key_capture` call is kept for completeness — it
        // lets `baseview` know our intent — but the actual forwarding
        // happens via `PostMessageW` in `forward_key_to_daw`.
        if self.text_edit_active {
            frame.set_key_capture(egui_baseview::KeyCapture::CaptureAll);
        } else {
            frame.set_key_capture(egui_baseview::KeyCapture::IgnoreAll);

            #[cfg(target_os = "windows")]
            {
                let key_events: Vec<(egui::Key, bool)> = ui.input(|i| {
                    i.events
                        .iter()
                        .filter_map(|ev| {
                            if let egui::Event::Key {
                                key,
                                pressed,
                                repeat,
                                ..
                            } = ev
                            {
                                // Skip auto-repeat; the DAW handles held keys itself.
                                if *repeat {
                                    None
                                } else {
                                    Some((*key, *pressed))
                                }
                            } else {
                                None
                            }
                        })
                        .collect()
                });
                for (key, pressed) in key_events {
                    forward_key_to_daw(key, pressed);
                }
            }
        }
        self.current_preset = current_preset;
        self.current_page = current_page;
        self.pending_page = pending_page;
    }
}

impl PejvakEguiApp {
    fn poll_preset_dialog(&mut self, setter: &ParamSetter) {
        let mut st = pd_lock();

        if let Some(path) = st.save_path.take() {
            let _ = std::fs::write(&path, params_to_xml(&self.params));
            if let Some(stem) = path.file_stem().and_then(|x| x.to_str()) {
                self.current_preset = stem.to_string();
            }
        }

        if let Some(path) = st.load_path.take() {
            if let Some(parsed) = xml_to_params(&path) {
                apply_preset(&self.params, setter, &parsed.cloudseed);
                apply_texture_params(&self.params, setter, &parsed.texture, &parsed.enabled);
                // Only restore the signal-chain order if the preset
                // actually declared one. Legacy XMLs keep the current
                // order.
                if let Some(order) = parsed.texture_order {
                    apply_texture_order(&self.params, order);
                }
                // Same rule for the master EQ switch: legacy presets
                // without the tag leave the current value untouched.
                if let Some(eq) = parsed.eq_enabled {
                    apply_eq_master(&self.params, setter, eq);
                }
                // FFDN parameters — only the ones that were actually
                // present in the XML are applied; the rest keep the
                // user's current values. This is what makes loading a
                // legacy preset (which only carried FfdnDiffusion)
                // non-destructive.
                apply_ffdn_params(&self.params, setter, &parsed.ffdn);
                apply_chime_params(
                    &self.params,
                    setter,
                    &parsed.illusion,
                    parsed.chime_enabled,
                    parsed.chime_seen,
                );
                if let Some(stem) = path.file_stem().and_then(|x| x.to_str()) {
                    self.current_preset = stem.to_string();
                }
            }
        }
    }

    /// Applies the top of the undo stack. Returns the preset name
    /// stored in the target snapshot so the caller can restore the
    /// label in the preset browser along with the values.
    fn do_undo(&mut self, setter: &ParamSetter, now: f64) -> Option<(String, EditorPage)> {
        let Some(target) = self.undo_stack.pop_back() else {
            return None;
        };

        let target_name = target.preset_name.clone();
        let target_page = target.page;

        if let Some(cur) = self.last_snapshot.take() {
            self.redo_stack.push(cur);
        }

        apply_undo_snapshot(&self.params, setter, &target);
        self.last_snapshot = Some(target);
        self.suppress_until = now + UNDO_SUPPRESS_S;
        self.last_history_apply = now;

        Some((target_name, target_page))
    }

    /// Applies the top of the redo stack. Returns the preset name
    /// stored in the target snapshot (see `do_undo`).
    fn do_redo(&mut self, setter: &ParamSetter, now: f64) -> Option<(String, EditorPage)> {
        let Some(target) = self.redo_stack.pop() else {
            return None;
        };

        let target_name = target.preset_name.clone();
        let target_page = target.page;

        if let Some(cur) = self.last_snapshot.take() {
            self.undo_stack.push_back(cur);
            while self.undo_stack.len() > UNDO_DEPTH {
                self.undo_stack.pop_front();
            }
        }

        apply_undo_snapshot(&self.params, setter, &target);
        self.last_snapshot = Some(target);
        self.suppress_until = now + UNDO_SUPPRESS_S;
        self.last_history_apply = now;

        Some((target_name, target_page))
    }

    /// Returns `true` if any TextEdit is currently in edit mode.
    fn read_text_edit_active(ui: &Ui) -> bool {
        ui.memory(|m| {
            let slot_active = m
                .data
                .get_temp::<egui::Id>(egui::Id::new("pejvak_current_editing"))
                .is_some();
            let flag_active = m
                .data
                .get_temp::<bool>(egui::Id::new("pejvak_text_edit_active"))
                .unwrap_or(false);
            slot_active || flag_active
        })
    }
}

// ============================================================================
// Root
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn show_resizable_delta<R>(
    ui: &mut Ui,
    min_size: Vec2,
    id: egui::Id,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    #[derive(Clone, Copy)]
    struct DragState {
        start_pointer: Pos2,
        start_size: Vec2,
    }

    let drag_state_id = id.with("drag_state");
    let drag_state: Option<DragState> = ui.memory(|m| m.data.get_temp(drag_state_id));

    let ui_rect = ui.available_rect_before_wrap();
    let mut content_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(ui_rect)
            .layout(*ui.layout()),
    );

    let ret = add_contents(&mut content_ui);

    const HANDLE_R: f32 = 16.0;
    let corner_rect = Rect::from_min_size(
        Pos2::new(ui_rect.right() - HANDLE_R, ui_rect.bottom() - HANDLE_R),
        Vec2::splat(HANDLE_R),
    );
    let corner_response = ui.interact(corner_rect, id.with("corner"), Sense::drag());

    if corner_response.drag_started() {
        if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
            ui.memory_mut(|m| {
                m.data.insert_temp(
                    drag_state_id,
                    DragState {
                        start_pointer: pointer,
                        start_size: ui_rect.size(),
                    },
                );
            });
        }
    }

    if corner_response.dragged() {
        if let Some(state) = drag_state {
            if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                let delta = pointer - state.start_pointer;
                let old = state.start_size;

                // Aspect ratio of the design. We lock to this every
                // frame so the window can never end up at the wrong
                // proportions, even if the host reported a
                // non-standard size on the previous frame.
                const DESIGN_ASPECT: f32 = WINDOW_W as f32 / WINDOW_H as f32;

                // Compute the growth factor for each axis, then
                // average them. This gives a natural feel:
                //   * dragging the corner diagonally grows both
                //     dimensions together,
                //   * dragging along a single axis grows both but
                //     less aggressively.
                // Using only one axis would ignore half the user's
                // input; using the max would feel jumpy on slight
                // off-axis movement.
                let fx = (old.x + delta.x) / old.x.max(1.0);
                let fy = (old.y + delta.y) / old.y.max(1.0);
                let f = ((fx + fy) * 0.5).max(0.1);

                // Apply the factor to width, then derive height from
                // the locked aspect ratio. If the result would violate
                // the minimum size on either axis, scale both up
                // together so that the aspect ratio is still
                // preserved.
                let mut new_w = old.x * f;
                let mut new_h = new_w / DESIGN_ASPECT;

                if new_w < min_size.x {
                    new_w = min_size.x;
                    new_h = new_w / DESIGN_ASPECT;
                }
                if new_h < min_size.y {
                    new_h = min_size.y;
                    new_w = new_h * DESIGN_ASPECT;
                }

                // Round to integers to avoid sub-pixel churn from
                // the host and from our own averaging.
                let target = Vec2::new(new_w.round(), new_h.round());

                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::InnerSize(target));
            }
        }
    }

    if corner_response.drag_stopped() {
        ui.memory_mut(|m| m.data.remove::<DragState>(drag_state_id));
    }

    if corner_response.hovered() || corner_response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
    }

    let painter = ui.painter();
    let col = ui
        .visuals()
        .widgets
        .active
        .fg_stroke
        .color
        .gamma_multiply(0.7);
    for k in 0..3 {
        let offset = (k as f32 + 1.0) * 4.0;
        let a = Pos2::new(corner_rect.right() - offset, corner_rect.bottom() - 1.0);
        let b = Pos2::new(corner_rect.right() - 1.0, corner_rect.bottom() - offset);
        painter.line_segment([a, b], Stroke::new(1.3, col));
    }

    ret
}

#[allow(clippy::too_many_arguments)]
fn draw_editor(
    ui: &mut Ui,
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    t: &Theme,
    scale: f32,
    is_light: bool,
    current_preset: &mut String,
    user_presets: &[String],
    current_page: &mut EditorPage,
    pending_page: &mut Option<EditorPage>,
    fade: f32,
    engine_fade: f32,
    use_ffdn_visual: bool,
    undo_available: bool,
    redo_available: bool,
    undo_label: &str,
    redo_label: &str,
    pending_action: &mut Option<HistoryAction>,
) {
    egui::Frame::NONE
        .inner_margin(egui::Margin {
            left: (OUTER_MARGIN_H * scale).round() as i8,
            right: (OUTER_MARGIN_H * scale).round() as i8,
            top: (OUTER_MARGIN_V * scale).round() as i8,
            bottom: (OUTER_MARGIN_V * scale).round() as i8,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = -6.0 * scale;

                    ui.label(
                        egui::RichText::new("PEJVAK   RVB")
                            .font(egui::FontId::new(
                                24.0 * scale,
                                egui::FontFamily::Name("Monoton".into()),
                            ))
                            .color(t.logo),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "HYBRID ALGORITHMIC REVERB  ·  v{}",
                            env!("CARGO_PKG_VERSION")
                        ))
                        .size(10.0 * scale)
                        .color(t.dim2),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Override egui's default `item_spacing` (which the
                    // outer style sets to `PAGE_GAP * scale` = 6 px)
                    // so that ONLY our explicit `add_space` calls
                    // control the gaps between header controls. Without
                    // this, the 6 px default stacks on top of every
                    // gap, making the header look twice as airy as
                    // intended.
                    ui.spacing_mut().item_spacing.x = 0.0;

                    let gap = HEADER_GAP * scale;
                    let tight = HEADER_TIGHT_GAP * scale;

                    // ---- Far-right cluster: theme + menu --------------------------
                    theme_toggle_button(ui, t, is_light, p, scale);
                    ui.add_space(tight);
                    menu_button(ui, t, scale, p);
                    ui.add_space(gap);

                    // ---- Preset browser -------------------------------------------
                    preset_menu(ui, t, p, s, scale, current_preset, user_presets);
                    ui.add_space(gap);

                    // ---- Undo / Redo (between preset and the toggle group) --------
                    // Single segmented widget; internally LTR so Undo
                    // is on the left and Redo on the right on screen.
                    let (undo_clicked, redo_clicked) = history_switch(
                        ui,
                        t,
                        undo_available,
                        redo_available,
                        undo_label,
                        redo_label,
                        scale,
                    );
                    if undo_clicked {
                        *pending_action = Some(HistoryAction::Undo);
                    }
                    if redo_clicked {
                        *pending_action = Some(HistoryAction::Redo);
                    }
                    ui.add_space(gap);

                    // ---- Bypass / Freeze / Interp toggle group --------------------
                    const BYPASS_ORANGE: Color32 = Color32::from_rgb(0xFF, 0x8C, 0x42);
                    const FREEZE_ICE: Color32 = Color32::from_rgb(0x7D, 0xD3, 0xFC);
                    const INTERP_GREEN: Color32 = Color32::from_rgb(0x7D, 0xDB, 0x88);

                    header_toggle_group(
                        ui,
                        t,
                        &[
                            ("Bypass", &p.global.bypass, BYPASS_ORANGE),
                            ("Freeze", &p.global.freeze, FREEZE_ICE),
                            ("Interp", &p.switches.interpolation, INTERP_GREEN),
                        ],
                        s,
                        scale,
                    );
                    ui.add_space(gap);

                    // ---- Engine selector ------------------------------------------
                    engine_switch_header(ui, t, p.ffdn.use_ffdn.value(), p, s, scale);
                    ui.add_space(gap);

                    // ---- TX kill switch -------------------------------------------
                    texture_kill_button(ui, t, p, s, scale);
                    ui.add_space(gap);

                    // ---- MAIN | TEXTURE tabs (leftmost) ---------------------------
                    let active_page = pending_page.unwrap_or(*current_page);
                    if let Some(target) = header_tabs(ui, t, active_page, scale) {
                        if pending_page.is_none() && fade >= 1.0 && target != *current_page {
                            *pending_page = Some(target);
                        }
                    }
                });
            });

            let sep_y = ui.cursor().top() + PAGE_GAP * 0.5 * scale;
            ui.painter().line_segment(
                [
                    Pos2::new(ui.max_rect().left(), sep_y),
                    Pos2::new(ui.max_rect().right(), sep_y),
                ],
                Stroke::new(1.0, t.line),
            );
            ui.add_space(PAGE_GAP * 2.0 * scale);

            let body_w = ui.available_width();
            // the CLOUD↔FFDN fade goes through a
            // smoothstep curve before being combined with the
            // (linear) MAIN↔TEXTURE fade. This is what makes the
            // engine switch *feel* smoother than a straight
            // linear ramp: the eye sees the cards ease in and out
            // rather than simply getting brighter/darker at a
            // constant rate.
            let body_alpha = (fade * smoothstep(engine_fade)).max(0.05);

            ui.vertical(|ui| {
                ui.set_min_width(body_w);
                ui.set_max_width(body_w);
                ui.multiply_opacity(body_alpha);
                match *current_page {
                    EditorPage::Main => {
                        // the visual engine flag is passed
                        // in, NOT read from the parameter, so that
                        // the CLOUD cards stay on screen during the
                        // fade-out phase of a transition to FFDN.
                        draw_main_page(
                            ui,
                            p,
                            s,
                            t,
                            scale,
                            is_light,
                            current_preset,
                            user_presets,
                            use_ffdn_visual,
                        );
                    }
                    EditorPage::Texture => {
                        draw_texture_page(ui, p, s, t, scale);
                    }
                }
                ui.multiply_opacity(1.0);
            });
        });
}

// ============================================================================
// Page toggle button
// ============================================================================

fn header_tabs(ui: &mut Ui, t: &Theme, active_page: EditorPage, scale: f32) -> Option<EditorPage> {
    const TAB_BLUE_DARK: Color32 = Color32::from_rgb(0x1A, 0x73, 0xE8);
    const TAB_BLUE_LIGHT: Color32 = Color32::from_rgb(0x15, 0x5A, 0xB8);
    let tab_blue = if t.bg_top.r() > 128 {
        TAB_BLUE_LIGHT
    } else {
        TAB_BLUE_DARK
    };

    let labels = ["MAIN", "TEXTURE"];
    let pages = [EditorPage::Main, EditorPage::Texture];

    let font = egui::FontId::proportional(11.5 * scale);
    let pad_x = 14.0 * scale;
    let seg_h = 24.0 * scale;
    let outer_pad = 3.0 * scale;
    let radius = 6.0 * scale;

    let galleys: Vec<_> = labels
        .iter()
        .map(|l| {
            ui.painter()
                .layout_no_wrap(l.to_string(), font.clone(), Color32::WHITE)
        })
        .collect();
    let seg_widths: Vec<f32> = galleys.iter().map(|g| g.size().x + pad_x * 2.0).collect();
    let total_w: f32 = seg_widths.iter().sum::<f32>() + outer_pad * 2.0;
    let total_h = seg_h + outer_pad * 2.0;

    let (outer_rect, _) = ui.allocate_exact_size(Vec2::new(total_w, total_h), Sense::hover());

    // The tab bar has two selectable segments (MAIN and TEXTURE).
    // The highlight slides between them with a short animation.
    let sel_anim = ui.ctx().animate_bool_with_time(
        egui::Id::new("header_tabs_selection"),
        active_page == EditorPage::Texture,
        0.30,
    );

    let mut x = outer_rect.left() + outer_pad;
    let seg_rects: Vec<Rect> = seg_widths
        .iter()
        .map(|&w| {
            let r = Rect::from_min_size(
                Pos2::new(x, outer_rect.top() + outer_pad),
                Vec2::new(w, seg_h),
            );
            x += w;
            r
        })
        .collect();

    // ---- Interact (collected up front so hover state is available
    //      before we draw) ---------------------------------------------
    let mut clicked: Option<EditorPage> = None;
    let mut hovered: [bool; 2] = [false, false];
    for (i, seg_rect) in seg_rects.iter().enumerate() {
        let id = egui::Id::new(("header_tab_hit", labels[i]));
        let resp = ui.interact(*seg_rect, id, Sense::click());
        if resp.clicked() {
            clicked = Some(pages[i]);
        }
        hovered[i] = resp.hovered();
        let _ = hover_tip(
            resp,
            if pages[i] == active_page {
                "Currently viewing this page"
            } else {
                "Switch to this page"
            },
        );
    }

    // ---- Draw --------------------------------------------------------
    let painter = ui.painter();
    painter.rect_filled(outer_rect, radius, t.track_bg.gamma_multiply(0.55));

    let r_main = seg_rects[0];
    let r_tex = seg_rects[1];
    let highlighted = Rect::from_min_size(
        Pos2::new(
            r_main.left() + (r_tex.left() - r_main.left()) * sel_anim,
            r_main.top(),
        ),
        Vec2::new(
            r_main.width() + (r_tex.width() - r_main.width()) * sel_anim,
            r_main.height(),
        ),
    );
    let inner_radius = radius - outer_pad * 0.4;

    // ---- Soft hover backgrounds for inactive segments ----------------
    // Only the tab that is *not* currently selected gets a hover
    // tint. This avoids double-highlighting the active tab when the
    // pointer happens to be over it.
    for (i, seg_rect) in seg_rects.iter().enumerate() {
        if hovered[i] && i != sel_index_for(active_page) {
            painter.rect_filled(*seg_rect, inner_radius, tab_blue.gamma_multiply(0.10));
        }
    }

    // Sliding highlight on top of the hover tint.
    painter.rect_filled(highlighted, inner_radius, tab_blue.gamma_multiply(0.28));

    // ---- Labels ------------------------------------------------------
    for (i, seg_rect) in seg_rects.iter().enumerate() {
        let dist = (sel_anim - i as f32).abs();
        let active_amount = (1.0 - dist).clamp(0.0, 1.0);
        let text_col = lerp_color(t.dim, tab_blue, active_amount);

        painter.text(
            seg_rect.center(),
            egui::Align2::CENTER_CENTER,
            labels[i],
            font.clone(),
            text_col,
        );
    }

    if sel_anim > 0.001 && sel_anim < 0.999 {
        ui.ctx().request_repaint();
    }

    clicked
}

/// Returns the segment index of the currently active tab. `0` for
/// MAIN, `1` for TEXTURE. Used by `header_tabs` to skip the hover
/// tint on the active segment.
#[inline]
fn sel_index_for(page: EditorPage) -> usize {
    match page {
        EditorPage::Main => 0,
        EditorPage::Texture => 1,
    }
}

fn header_toggle_group<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    items: &[(&str, &P, Color32)],
    setter: &ParamSetter,
    scale: f32,
) {
    let font = egui::FontId::proportional(11.5 * scale);
    let pad_x = 12.0 * scale;
    let seg_h = 24.0 * scale;
    let outer_pad = 3.0 * scale;
    let radius = 6.0 * scale;

    let galleys: Vec<_> = items
        .iter()
        .map(|(l, _, _)| {
            ui.painter()
                .layout_no_wrap(l.to_string(), font.clone(), Color32::WHITE)
        })
        .collect();
    let seg_widths: Vec<f32> = galleys.iter().map(|g| g.size().x + pad_x * 2.0).collect();
    let total_w: f32 = seg_widths.iter().sum::<f32>() + outer_pad * 2.0;
    let total_h = seg_h + outer_pad * 2.0;

    let (outer_rect, _) = ui.allocate_exact_size(Vec2::new(total_w, total_h), Sense::hover());

    let mut x = outer_rect.left() + outer_pad;
    let seg_rects: Vec<Rect> = seg_widths
        .iter()
        .map(|&w| {
            let r = Rect::from_min_size(
                Pos2::new(x, outer_rect.top() + outer_pad),
                Vec2::new(w, seg_h),
            );
            x += w;
            r
        })
        .collect();

    // ---- Interact + collect state -----------------------------------
    let mut hovered: Vec<bool> = vec![false; items.len()];
    let mut on_state: Vec<bool> = vec![false; items.len()];
    for (i, (label, param, _)) in items.iter().enumerate() {
        on_state[i] = param.unmodulated_normalized_value() > 0.5;
        let id = egui::Id::new(("header_toggle_group_hit", param.name()));
        let resp = ui.interact(seg_rects[i], id, Sense::click());
        hovered[i] = resp.hovered();
        if resp.clicked() {
            setter.begin_set_parameter(*param);
            setter.set_parameter_normalized(*param, if on_state[i] { 0.0 } else { 1.0 });
            setter.end_set_parameter(*param);
        }
        let _ = hover_tip(
            resp,
            match *label {
                "Bypass" => "Host-integrated bypass (dry passthrough).",
                "Freeze" => "Hold the reverb tail (infinite sustain).",
                "Interp" => "Enable delay-line interpolation (smoother modulation).",
                _ => "",
            },
        );
    }

    // ---- Draw --------------------------------------------------------
    let painter = ui.painter();
    painter.rect_filled(outer_rect, radius, t.track_bg.gamma_multiply(0.55));

    let inner_radius = radius - outer_pad * 0.4;

    // boost the hover tint in light mode. Against the pale
    // light-theme card background, the default 0.08 multiplier is
    // barely visible; 0.18 gives a clear but still subtle hint.
    let is_light = t.bg_top.r() > 128;

    for (i, (_label, _param, accent)) in items.iter().enumerate() {
        if hovered[i] && !on_state[i] {
            let mul = if is_light { 0.18 } else { 0.08 };
            painter.rect_filled(seg_rects[i], inner_radius, accent.gamma_multiply(mul));
        }
    }

    // in light mode, boost the active-state background and
    // use a slightly darkened accent for the text so the toggle
    // reads as clearly "on" instead of washed out.
    //
    //   dark  theme:  0.22 bg / lerp to full accent
    //   light theme:  0.55 bg / lerp to darkened accent
    //
    // The darkened accent (gamma_multiply 0.7) makes the label
    // readable against the pale card while keeping the hue faithful
    // to the toggle's identity colour.
    let bg_mul = if is_light { 0.55 } else { 0.22 };
    for (i, (label, param, accent)) in items.iter().enumerate() {
        let seg_rect = seg_rects[i];
        let on = on_state[i];

        let text_accent = if is_light {
            accent.gamma_multiply(0.70)
        } else {
            *accent
        };

        let anim = ui.ctx().animate_bool_with_time(
            egui::Id::new(("header_toggle_group_anim", param.name())),
            on,
            0.28,
        );

        if anim > 0.01 {
            painter.rect_filled(seg_rect, inner_radius, accent.gamma_multiply(bg_mul * anim));
        }

        let text_col = lerp_color(t.dim, text_accent, anim);
        painter.text(
            seg_rect.center(),
            egui::Align2::CENTER_CENTER,
            *label,
            font.clone(),
            text_col,
        );
    }

    let any_animating = items.iter().any(|(_, param, _)| {
        let on = param.unmodulated_normalized_value() > 0.5;
        let a = ui.ctx().animate_bool_with_time(
            egui::Id::new(("header_toggle_group_anim", param.name())),
            on,
            0.28,
        );
        a > 0.001 && a < 0.999
    });
    if any_animating {
        ui.ctx().request_repaint();
    }
}

// ============================================================================
// MAIN page
// ============================================================================

/// The MAIN page as it appears when the CloudSeed engine is selected.
/// Called by the dispatcher `draw_main_page`.
#[allow(clippy::too_many_arguments)]
fn draw_main_page_cloudseed(
    ui: &mut Ui,
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    t: &Theme,
    scale: f32,
    _is_light: bool,
    _current_preset: &mut String,
    _user_presets: &[String],
) {
    // We measure per-column available height so each third-row card
    // fills its column all the way to the bottom of the plugin. This
    // guarantees the three third-row cards share the same *bottom
    // edge y-coordinate*, even though their heights may differ
    // (because the first two rows in each column consume different
    // amounts of vertical space).
    //
    // `PAGE_GAP * scale` is the vertical spacing between consecutive
    // cards inside a column. We subtract one extra gap from
    // `available_height` to account for the spacing that egui will
    // insert before the third card; without this correction the card
    // would be one gap too tall and would overflow the plugin.
    let gap_y = PAGE_GAP * scale;
    let pad_px = CARD_PAD * scale;

    // Same bottom offset as the FFDN layout. Reduced from 30 to 18
    // so the MIXER card gains ~12 px of height without pressing
    // against the plugin's bottom edge.
    const BOTTOM_OFFSET: f32 = 12.0;

    ui.columns(3, |cols| {
        // ---------- Column 0 ----------
        cols[0].vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap_y;

            // ============ MULTITAP DELAY ============
            draw_card(
                ui,
                t,
                scale,
                p.early.tap_enabled.value(),
                "multitap",
                |ui, ct| {
                    section_title_with_switch(
                        ui,
                        ct,
                        "MULTITAP DELAY",
                        ct.early,
                        "",
                        &p.early.tap_enabled,
                        s,
                        scale,
                    );
                    let card_on = p.early.tap_enabled.value();
                    ui.add_enabled_ui(card_on, |ui| {
                        knob_row(ui, scale, 3, |ui| {
                            knob(
                                ui,
                                ct,
                                "Length",
                                &p.early.tap_length,
                                s,
                                "TapLength",
                                ct.early,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Gain",
                                &p.early.tap_gain,
                                s,
                                "TapGain",
                                ct.early,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Decay",
                                &p.early.tap_decay,
                                s,
                                "TapDecay",
                                ct.early,
                                scale,
                            );
                        });
                        ui.add_space(ROW_OUTER_GAP * scale);
                        numbox_row(
                            ui, scale,
                            &[
                                ("Count", &p.early.tap_count, "Number of taps in the multitap diffuser.\n1 = single tap, 50 = dense early reflections."),
                                ("Seed", &p.seeds.tap_seed, "Deterministic PRNG seed for the tap positions.\nChange to get a different tap pattern."),
                            ],
                            s, ct.early, ct,
                        );
                    });
                },
            );

            // ============ LATE DELAY ============
            draw_card(
                ui,
                t,
                scale,
                p.late.late_delay_enabled.value(),
                "late_delay",
                |ui, ct| {
                    section_title_with_switch(
                        ui,
                        ct,
                        "LATE DELAY",
                        ct.late,
                        "",
                        &p.late.late_delay_enabled,
                        s,
                        scale,
                    );
                    let card_on = p.late.late_delay_enabled.value();
                    ui.add_enabled_ui(card_on, |ui| {
                        knob_row(ui, scale, 4, |ui| {
                            knob(
                                ui,
                                ct,
                                "Delay",
                                &p.late.line_delay,
                                s,
                                "LineDelay",
                                ct.late,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Decay",
                                &p.late.line_decay,
                                s,
                                "LineDecay",
                                ct.late,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Mod Amt",
                                &p.modulation.line_mod_amount,
                                s,
                                "LineModAmount",
                                ct.late,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Mod Rate",
                                &p.modulation.line_mod_rate,
                                s,
                                "LineModRate",
                                ct.late,
                                scale,
                            );
                        });
                        ui.add_space(ROW_OUTER_GAP * scale);
                        numbox_row(
                            ui, scale,
                            &[
                                ("Lines", &p.late.line_count, "Number of active delay lines in the late FDN.\n1 = thin, 12 = dense."),
                                ("Seed", &p.seeds.delay_seed, "PRNG seed for the per-line delays and modulation.\nChange to get a different tail."),
                            ],
                            s, ct.late, ct,
                        );
                    });
                },
            );

            // ============ ROUTE ============
            // Fill the rest of the column so the bottom edge aligns
            // with the third-row cards in the other two columns.
            let remaining_px = (ui.available_height() - gap_y).max(60.0);
            let route_min_h = ((remaining_px - 2.0 * pad_px) / scale).max(80.0);
            draw_card_h(ui, t, scale, true, "route", route_min_h, |ui, ct| {
                section_title_with_switch(
                    ui,
                    ct,
                    "ROUTE",
                    ct.accent,
                    "Pre/Post",
                    &p.switches.late_stage_tap,
                    s,
                    scale,
                );
                ui.add_space(4.0 * scale);
                route_diagram(ui, ct, !p.switches.late_stage_tap.value(), scale);
                ui.add_space(ROW_OUTER_GAP * scale);

                let hint = if p.switches.late_stage_tap.value() {
                    "EQ is applied after the delay lines."
                } else {
                    "EQ is applied before the delay lines."
                };
                ui.label(egui::RichText::new(hint).size(9.0 * scale).color(ct.dim2));
            });
        });

        // ---------- Column 1 ----------
        cols[1].vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap_y;

            // ============ INPUT ============
            draw_card(ui, t, scale, true, "input", |ui, ct| {
                section_title(ui, ct, "INPUT", ct.input, scale);
                knob_row(ui, scale, 4, |ui| {
                    knob(
                        ui,
                        ct,
                        "Blend",
                        &p.input.mix,
                        s,
                        "InputMix",
                        ct.input,
                        scale,
                    );
                    knob(
                        ui,
                        ct,
                        "Pre-Dly",
                        &p.input.predelay,
                        s,
                        "PreDelay",
                        ct.input,
                        scale,
                    );
                    knob(
                        ui,
                        ct,
                        "Low Cut",
                        &p.input.high_pass,
                        s,
                        "HighPass",
                        ct.input,
                        scale,
                    );
                    knob(
                        ui,
                        ct,
                        "High Cut",
                        &p.input.low_pass,
                        s,
                        "LowPass",
                        ct.input,
                        scale,
                    );
                });
                ui.add_space(ROW_OUTER_GAP * scale);
                // CloudSeed INPUT has 4 knobs (Blend,
                // Pre-Dly, Low Cut, High Cut). HP under "Low Cut"
                // (col 2), LP under "High Cut" (col 3).
                switch_row_at_columns(
                    ui,
                    scale,
                    4,
                    KNOB_SLOT_W,
                    &[
                        (2, "HP", &p.switches.hi_pass_enabled),
                        (3, "LP", &p.switches.low_pass_enabled),
                    ],
                    s,
                    ct.input,
                    ct,
                );
            });

            // ============ EQ ============
            draw_card(ui, t, scale, p.eq.enabled.value(), "eq", |ui, ct| {
                section_title_with_switch(ui, ct, "EQ", ct.tone, "", &p.eq.enabled, s, scale);
                let card_on = p.eq.enabled.value();
                ui.add_enabled_ui(card_on, |ui| {
                    knob_row_compact(ui, scale, 5, |ui| {
                        knob_compact(
                            ui,
                            ct,
                            "Low Frq",
                            &p.eq.low_shelf_frequency,
                            s,
                            "PostLowShelfFrequency",
                            ct.tone,
                            scale,
                        );
                        knob_compact(
                            ui,
                            ct,
                            "Low Gain",
                            &p.eq.low_shelf_gain,
                            s,
                            "PostLowShelfGain",
                            ct.tone,
                            scale,
                        );
                        knob_compact(
                            ui,
                            ct,
                            "Hi Gain",
                            &p.eq.high_shelf_gain,
                            s,
                            "PostHighShelfGain",
                            ct.tone,
                            scale,
                        );
                        knob_compact(
                            ui,
                            ct,
                            "Hi Frq",
                            &p.eq.high_shelf_frequency,
                            s,
                            "PostHighShelfFrequency",
                            ct.tone,
                            scale,
                        );
                        knob_compact(
                            ui,
                            ct,
                            "Cutoff",
                            &p.eq.cutoff_frequency,
                            s,
                            "PostCutoffFrequency",
                            ct.tone,
                            scale,
                        );
                    });
                    ui.add_space(ROW_OUTER_GAP * scale);
                    // EQ has 5 compact knobs (Low Frq,
                    // Low Gain, Hi Gain, Hi Frq, Cutoff). The Low
                    // switch goes under "Low Frq" (col 0), High
                    // under "Hi Frq" (col 3), Cutoff under the
                    // Cutoff knob (col 4).
                    switch_row_at_columns(
                        ui,
                        scale,
                        5,
                        KNOB_SMALL_SLOT_W,
                        &[
                            (0, "Low", &p.switches.low_shelf_enabled),
                            (3, "High", &p.switches.high_shelf_enabled),
                            (4, "Cutoff", &p.switches.cutoff_enabled),
                        ],
                        s,
                        ct.tone,
                        ct,
                    );
                });
            });

            // ============ MIXER ============
            // Same bottom offset as FILTER and DYNAMICS so the three
            // cards' bottom edges line up.
            let remaining_px =
                (ui.available_height() - gap_y - BOTTOM_OFFSET * scale).max(60.0);
            let mixer_min_h = ((remaining_px - 2.0 * pad_px) / scale).max(80.0);
            draw_card_h(ui, t, scale, true, "mixer", mixer_min_h, |ui, ct| {
                section_title(ui, ct, "MIXER", ct.output, scale);
                fader_row(ui, scale, 5, |ui| {
                    fader(
                        ui,
                        ct,
                        "Dry",
                        &p.output.dry_out,
                        s,
                        "DryOut",
                        ct.output,
                        scale,
                    );
                    fader(
                        ui,
                        ct,
                        "PreDly",
                        &p.output.predelay_out,
                        s,
                        "PredelayOut",
                        ct.output,
                        scale,
                    );
                    fader(
                        ui,
                        ct,
                        "Early",
                        &p.output.early_out,
                        s,
                        "EarlyOut",
                        ct.output,
                        scale,
                    );
                    fader(
                        ui,
                        ct,
                        "Late",
                        &p.output.main_out,
                        s,
                        "MainOut",
                        ct.output,
                        scale,
                    );
                    fader(
                        ui,
                        ct,
                        "Width",
                        &p.output.width,
                        s,
                        "Width",
                        ct.output,
                        scale,
                    );
                });
            });
        });

        // ---------- Column 2 ----------
        cols[2].vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap_y;

            // ============ EARLY DIFFUSION ============
            draw_card(
                ui,
                t,
                scale,
                p.early.diffusion_enabled.value(),
                "early_diffusion",
                |ui, ct| {
                    section_title_with_switch(
                        ui,
                        ct,
                        "EARLY DIFFUSION",
                        ct.early_diffusion,
                        "",
                        &p.early.diffusion_enabled,
                        s,
                        scale,
                    );
                    let card_on = p.early.diffusion_enabled.value();
                    ui.add_enabled_ui(card_on, |ui| {
                        knob_row(ui, scale, 4, |ui| {
                            knob(
                                ui,
                                ct,
                                "Delay",
                                &p.early.diffusion_delay,
                                s,
                                "DiffusionDelay",
                                ct.early_diffusion,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Feedback",
                                &p.early.diffusion_feedback,
                                s,
                                "DiffusionFeedback",
                                ct.early_diffusion,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Mod Amt",
                                &p.modulation.early_diffusion_mod_amount,
                                s,
                                "EarlyDiffusionModAmount",
                                ct.early_diffusion,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Mod Rate",
                                &p.modulation.early_diffusion_mod_rate,
                                s,
                                "EarlyDiffusionModRate",
                                ct.early_diffusion,
                                scale,
                            );
                        });
                        ui.add_space(ROW_OUTER_GAP * scale);
                        numbox_row(
                            ui, scale,
                            &[
                                ("Stages", &p.early.diffusion_stages, "Number of allpass stages in the early diffusion cascade.\n1 = minimal, 8 = smoothest."),
                                ("Seed", &p.seeds.diffusion_seed, "PRNG seed for the allpass delay lengths.\nChange to get a different diffusion character."),
                            ],
                            s, ct.early_diffusion, ct,
                        );
                    });
                },
            );

            // ============ LATE DIFFUSION ============
            draw_card(
                ui,
                t,
                scale,
                p.late.late_diffusion_enabled.value(),
                "late_diffusion",
                |ui, ct| {
                    section_title_with_switch(
                        ui,
                        ct,
                        "LATE DIFFUSION",
                        ct.late_diffusion,
                        "",
                        &p.late.late_diffusion_enabled,
                        s,
                        scale,
                    );
                    let card_on = p.late.late_diffusion_enabled.value();
                    ui.add_enabled_ui(card_on, |ui| {
                        knob_row(ui, scale, 4, |ui| {
                            knob(
                                ui,
                                ct,
                                "Delay",
                                &p.late.late_diffusion_delay,
                                s,
                                "LateDiffusionDelay",
                                ct.late_diffusion,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Feedback",
                                &p.late.late_diffusion_feedback,
                                s,
                                "LateDiffusionFeedback",
                                ct.late_diffusion,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Mod Amt",
                                &p.modulation.late_diffusion_mod_amount,
                                s,
                                "LateDiffusionModAmount",
                                ct.late_diffusion,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Mod Rate",
                                &p.modulation.late_diffusion_mod_rate,
                                s,
                                "LateDiffusionModRate",
                                ct.late_diffusion,
                                scale,
                            );
                        });
                        ui.add_space(ROW_OUTER_GAP * scale);
                        numbox_row(
                            ui, scale,
                            &[
                                ("Stages", &p.late.late_diffusion_stages, "Number of allpass stages per late delay line.\n1 = minimal, 8 = smoothest."),
                                ("Seed", &p.seeds.post_diffusion_seed, "PRNG seed for the per-line diffuser delays.\nChange to vary the late diffusion."),
                            ],
                            s, ct.late_diffusion, ct,
                        );
                    });
                },
            );

            let remaining_px = (ui.available_height() - gap_y).max(60.0);
            let velvet_min_h = ((remaining_px - 2.0 * pad_px) / scale).max(80.0);
            draw_card_h(ui, t, scale, true, "velvet", velvet_min_h, |ui, ct| {
                // Card title and colour match the primary knob (Velvet).
                // The card was previously called "ENGINE", which collided
                // with the actual engine selector in the header; the new
                // name makes it clear that this card controls only the
                // Velvet-Noise diffusion stage inside CloudSeed.
                section_title(ui, ct, "VELVET NOISE", ct.velvet, scale);

                // Velvet-Noise diffusion controls. These are only
                // meaningful for the CloudSeed engine; the FFDN page
                // has its own layout (see `draw_main_page_ffdn`).
                knob_row(ui, scale, 3, |ui| {
                    knob(
                        ui,
                        ct,
                        "Velvet",
                        &p.velvet.diffusion,
                        s,
                        "Velvet-Noise diffusion amount.\n\
                         0 % = bypass (bit-identical to plain CloudReverb).\n\
                         Higher = denser, smoother late diffusion.",
                        ct.velvet,
                        scale,
                    );
                    knob(
                        ui,
                        ct,
                        "Density",
                        &p.velvet.density,
                        s,
                        "Pulse density of the velvet-noise sequence.\n\
                         0 = 1500 pulses/s, 1 = 3000 pulses/s.",
                        ct.velvet,
                        scale,
                    );
                    knob(
                        ui,
                        ct,
                        "Tone",
                        &p.velvet.tone,
                        s,
                        "Pulse width (Dark Velvet Noise).\n\
                         0 = impulse pulses (bright).\n\
                         1 = wide rectangular pulses (dark).",
                        ct.velvet,
                        scale,
                    );
                });
            });
        });
    });
}

/// Entry point for the MAIN page. Dispatches to the CloudSeed layout
/// or the FFDN layout depending on which engine is selected.
///
/// The two layouts are entirely independent: each has its own set of
/// cards, its own grouping of parameters, and its own visual rhythm.
/// The INPUT / EQ / MIXER cards are duplicated (identical content) so
/// that the shared parameters stay in a familiar position on both
/// pages.
#[allow(clippy::too_many_arguments)]
fn draw_main_page(
    ui: &mut Ui,
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    t: &Theme,
    scale: f32,
    is_light: bool,
    current_preset: &mut String,
    user_presets: &[String],
    use_ffdn_visual: bool,
) {
    // the `use_ffdn_visual` flag is the *visual* engine
    // state, which lags behind the parameter during a transition.
    // Using it (instead of `p.ffdn.use_ffdn.value()`) keeps the
    // old engine's cards on screen for the entire fade-out phase,
    // then swaps to the new engine's cards at the bottom of the
    // fade. The result is a smooth dissolve identical to the
    // MAIN ↔ TEXTURE transition.
    if use_ffdn_visual {
        draw_main_page_ffdn(ui, p, s, t, scale);
    } else {
        draw_main_page_cloudseed(ui, p, s, t, scale, is_light, current_preset, user_presets);
    }
}

/// The MAIN page as it appears when the FFDN engine is selected.
///
/// # Color scheme
///
/// Each card gets its own accent colour so the eye can instantly
/// tell the cards apart:
///
///   * **FFDN SPACE**      → `ct.accent` (cyan) — signature FFDN
///   * **FFDN NETWORK**    → `ct.late` (light purple) — topology
///   * **FFDN FILTER**     → `ct.early_diffusion` (green) — filtering
///   * **FFDN CHARACTER**  → `ct.tone` (orange) — matches the EQ card
///   * **FFDN SHELVES**    → `ct.tex_shimmer` (yellow) — shelf EQ
///   * **FFDN MODULATION** → `ct.tex_bloom` (magenta) — movement
///   * **FFDN DYNAMICS**   → `ct.tex_scatter` (coral) — envelope
///
/// INPUT and MIXER keep their existing colours (`ct.input` and
/// `ct.output`) because those two cards appear identically on the
/// CloudSeed page and their colour is part of the shared identity.
fn draw_main_page_ffdn(
    ui: &mut Ui,
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    t: &Theme,
    scale: f32,
) {
    let gap_y = PAGE_GAP * scale;
    let pad_px = CARD_PAD * scale;

    // Common bottom offset for the three third-row cards (FILTER,
    // MIXER, DYNAMICS). Reduced from 30 to 18 so all three cards
    // gain ~12 px of height while staying off the bottom edge.
    const BOTTOM_OFFSET: f32 = 12.0;

    ui.columns(3, |cols| {
        // ============================================================
        // ---------- Column 0 ----------
        // ============================================================
        cols[0].vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap_y;

            // ============ FFDN SPACE ============
            let space_on = p.ffdn.space_enabled.value();
            draw_card(ui, t, scale, space_on, "ffdn_space", |ui, ct| {
                section_title_with_switch(
                    ui,
                    ct,
                    "SPACE",
                    ct.accent,
                    "",
                    &p.ffdn.space_enabled,
                    s,
                    scale,
                );
                ui.add_enabled_ui(space_on, |ui| {
                    knob_row(ui, scale, 4, |ui| {
                        knob(
                            ui,
                            ct,
                            "Size",
                            &p.ffdn.size,
                            s,
                            "Base delay length of the FFDN.\n\
                             5 ms = tight, 200 ms = diffuse.",
                            ct.accent,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Decay",
                            &p.ffdn.decay,
                            s,
                            "Target RT60 of the FFDN.\n\
                             0.1 s = very tight, 60 s = infinite-ish.",
                            ct.accent,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Spread",
                            &p.ffdn.spread,
                            s,
                            "How far the individual delay lines are spread\n\
                             apart from the base Size.\n\
                             0 % = all equal, 100 % = ±50 %.",
                            ct.accent,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Diffusion",
                            &p.ffdn.diffusion,
                            s,
                            "Input diffusion: an allpass cascade placed\n\
                             before the FDN network.\n\
                             0 % = off (bit-identical to plain FFDN).\n\
                             Higher = smoother attack, denser early tail.",
                            ct.accent,
                            scale,
                        );
                    });
                });
            });

            // ============ FFDN NETWORK ============
            let network_on = p.ffdn.network_enabled.value();
            draw_card(ui, t, scale, network_on, "ffdn_network", |ui, ct| {
                section_title_with_switch(
                    ui,
                    ct,
                    "NETWORK",
                    ct.late,
                    "",
                    &p.ffdn.network_enabled,
                    s,
                    scale,
                );
                ui.add_enabled_ui(network_on, |ui| {
                    ui.add_space(4.0 * scale);

                    knob_row(ui, scale, 3, |ui| {
                        knob(
                            ui,
                            ct,
                            "Lines",
                            &p.ffdn.lines,
                            s,
                            "Number of active delay lines per network.\n\
                             4 = sparse, wide echo spacing.\n\
                             8 = default (matches pre-).\n\
                             16 = dense, smooth tail.",
                            ct.late,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Density",
                            &p.ffdn.density,
                            s,
                            "How the input energy is distributed across\n\
                             the delay lines.\n\
                             0 % = sparse (input concentrated on a few lines).\n\
                             50 % = uniform (flat distribution, the default).\n\
                             100 % = wide pattern (strong per-line variation).",
                            ct.late,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Decorr",
                            &p.ffdn.decorr,
                            s,
                            "Delay offset between the L and R networks.\n\
                             0 % = both networks share the same base delay.\n\
                             50 % = 3 % offset (default, matches pre-).\n\
                             100 % = 6 % offset, wider stereo field.",
                            ct.late,
                            scale,
                        );
                    });

                    // ---- Matrix type selector --------------------------
                    ui.add_space(6.0 * scale);
                    ui.label(
                        egui::RichText::new("MATRIX TYPE")
                            .size(8.5 * scale)
                            .color(ct.dim2),
                    );
                    ui.add_space(3.0 * scale);
                    horizontal_switch_2(
                        ui,
                        ct,
                        "HOUSEHOLDER",
                        "VELVET",
                        &p.ffdn.matrix_type,
                        s,
                        "Dense Householder matrix\n(smooth, uniform tail)",
                        "Sparse Velvet matrix\n(blocked mixing, natural tail)",
                        ct.late,
                        scale,
                    );
                });
            });

            // ============ FFDN FILTER ============
            // Uses the shared BOTTOM_OFFSET defined at the top of this
            // function so FILTER, MIXER, and DYNAMICS all end at the
            // same y-coordinate.
            let filter_on = p.ffdn.filter_enabled.value();
            let remaining_px =
                (ui.available_height() - gap_y - BOTTOM_OFFSET * scale).max(60.0);
            let filter_h = ((remaining_px - 2.0 * pad_px) / scale).max(80.0);
            draw_card_h(
                ui,
                t,
                scale,
                filter_on,
                "ffdn_filter",
                filter_h,
                |ui, ct| {
                    section_title_with_switch(
                        ui,
                        ct,
                        "FILTER",
                        ct.early_diffusion,
                        "",
                        &p.ffdn.filter_enabled,
                        s,
                        scale,
                    );
                    ui.add_enabled_ui(filter_on, |ui| {
                        ui.add_space(4.0 * scale);

                        knob_row(ui, scale, 3, |ui| {
                            knob(
                                ui,
                                ct,
                                "Loop HP",
                                &p.ffdn.loop_hp,
                                s,
                                "Removes low-frequency rumble from the tail.\n\
                             A high-pass inside the feedback loop.\n\
                             20 Hz = bypass (neutral, bit-identical to pre-4.80).\n\
                             Raising it improves stability on very long\n\
                             decays and 'lightens' the tail.",
                                ct.early_diffusion,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Loop LP",
                                &p.ffdn.loop_lp,
                                s,
                                "Extra HF attenuation inside the loop.\n\
                             A sharper 12 dB/oct low-pass, stacked on top\n\
                             of the master Damping (which is 6 dB/oct).\n\
                             20 kHz = bypass (neutral).\n\
                             Use when Damping alone is not enough — for\n\
                             example on very long tails that stay too bright\n\
                             even at Damping = 500 Hz.",
                                ct.early_diffusion,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Mid Focus",
                                &p.ffdn.mid_focus,
                                s,
                                "Focuses the tail on the mid band by shelving\n\
                             BOTH ends at once:\n\
                               -6 dB below 400 Hz (low shelf)\n\
                               -6 dB above 1500 Hz (high shelf)\n\
                             0 % = off.\n\
                             Different from Loop HP (cuts only below)\n\
                             and Loop LP (cuts only above): Mid Focus\n\
                             cuts both sides for a 'telephone' character.",
                                ct.early_diffusion,
                                scale,
                            );
                        });

                        ui.add_space(6.0 * scale);
                        ui.label(
                            egui::RichText::new("All filters bypass at their neutral value.")
                                .size(8.5 * scale)
                                .color(ct.dim2),
                        );
                    });
                },
            );
        });

        // ============================================================
        // ---------- Column 1 ----------
        // ============================================================
        cols[1].vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap_y;

            // ============ INPUT (بدون تغییر — بدون سوئیچ) ============
            draw_card(ui, t, scale, true, "input", |ui, ct| {
                section_title(ui, ct, "INPUT", ct.input, scale);
                knob_row(ui, scale, 3, |ui| {
                    knob(
                        ui,
                        ct,
                        "Pre-Dly",
                        &p.input.predelay,
                        s,
                        "PreDelay",
                        ct.input,
                        scale,
                    );
                    knob(
                        ui,
                        ct,
                        "Low Cut",
                        &p.input.high_pass,
                        s,
                        "HighPass",
                        ct.input,
                        scale,
                    );
                    knob(
                        ui,
                        ct,
                        "High Cut",
                        &p.input.low_pass,
                        s,
                        "LowPass",
                        ct.input,
                        scale,
                    );
                });
                ui.add_space(ROW_OUTER_GAP * scale);
                switch_row_at_columns(
                    ui,
                    scale,
                    3,
                    KNOB_SLOT_W,
                    &[
                        (1, "HP", &p.switches.hi_pass_enabled),
                        (2, "LP", &p.switches.low_pass_enabled),
                    ],
                    s,
                    ct.input,
                    ct,
                );
            });

            // ============ FFDN CHARACTER ============
            let character_on = p.ffdn.character_enabled.value();
            draw_card(ui, t, scale, character_on, "ffdn_character", |ui, ct| {
                section_title_with_switch(
                    ui,
                    ct,
                    "CHARACTER",
                    ct.tone,
                    "",
                    &p.ffdn.character_enabled,
                    s,
                    scale,
                );
                ui.add_enabled_ui(character_on, |ui| {
                    knob_row_compact(ui, scale, 4, |ui| {
                        knob_compact(
                            ui,
                            ct,
                            "Cross",
                            &p.ffdn.cross,
                            s,
                            "Cross-feed between L and R networks.\n\
                             0 % = fully independent stereo.\n\
                             50 % = mono sum, both networks see (L+R)/2.\n\
                             100 % = channels swapped.",
                            ct.tone,
                            scale,
                        );
                        knob_compact(
                            ui,
                            ct,
                            "Sat",
                            &p.ffdn.drive,
                            s,
                            "Velvet saturation inside the feedback loop.\n\
                             0 % = linear (bit-identical).\n\
                             Higher = warm, tubey compression of loud\n\
                             passages. Quiet tails decay unchanged.\n\
                             (Requires the Sat switch below.)",
                            ct.tone,
                            scale,
                        );
                        knob_compact(
                            ui,
                            ct,
                            "Tilt Frq",
                            &p.ffdn.tilt_freq,
                            s,
                            "Crossover of the spectral tilt filter.\n\
                             100 Hz .. 10 kHz, log-spaced.",
                            ct.tone,
                            scale,
                        );
                        knob_compact(
                            ui,
                            ct,
                            "Tilt Amt",
                            &p.ffdn.tilt_amount,
                            s,
                            "Spectral tilt.\n\
                             0 % = bass-heavy (−6 dB treble).\n\
                             50 % = flat.\n\
                             100 % = treble-heavy (+6 dB treble).",
                            ct.tone,
                            scale,
                        );
                    });
                    ui.add_space(ROW_OUTER_GAP * scale);
                    switch_row_at_columns(
                        ui,
                        scale,
                        4,
                        KNOB_SMALL_SLOT_W,
                        &[
                            (1, "Sat", &p.ffdn.drive_enabled),
                            (2, "Tilt", &p.ffdn.tilt_enabled),
                        ],
                        s,
                        ct.tone,
                        ct,
                    );
                });
            });

            // ============ MIXER ============
            // Same bottom offset as FILTER and DYNAMICS so the three
            // cards' bottom edges line up.
            let remaining_px =
                (ui.available_height() - gap_y - BOTTOM_OFFSET * scale).max(60.0);
            let mixer_min_h = ((remaining_px - 2.0 * pad_px) / scale).max(80.0);
            draw_card_h(ui, t, scale, true, "mixer", mixer_min_h, |ui, ct| {
                section_title(ui, ct, "MIXER", ct.output, scale);
                fader_row(ui, scale, 4, |ui| {
                    fader(
                        ui,
                        ct,
                        "Dry",
                        &p.output.dry_out,
                        s,
                        "DryOut",
                        ct.output,
                        scale,
                    );
                    fader(
                        ui,
                        ct,
                        "Early",
                        &p.ffdn.er_level,
                        s,
                        "Early reflections level.\n\
                         Mixes the pre-network signal directly into\n\
                         the wet output.",
                        ct.output,
                        scale,
                    );
                    fader(
                        ui,
                        ct,
                        "Late",
                        &p.output.main_out,
                        s,
                        "MainOut",
                        ct.output,
                        scale,
                    );
                    fader(
                        ui,
                        ct,
                        "Width",
                        &p.output.width,
                        s,
                        "Width",
                        ct.output,
                        scale,
                    );
                });
            });
        });

        // ============================================================
        // ---------- Column 2 ----------
        // ============================================================
        cols[2].vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap_y;

            // ============ FFDN SHELVES ============
            let shelves_on = p.ffdn.shelves_enabled.value();
            draw_card(ui, t, scale, shelves_on, "ffdn_shelves", |ui, ct| {
                section_title_with_switch(
                    ui,
                    ct,
                    "SHELVES",
                    ct.tex_shimmer,
                    "",
                    &p.ffdn.shelves_enabled,
                    s,
                    scale,
                );
                ui.add_enabled_ui(shelves_on, |ui| {
                    knob_row(ui, scale, 4, |ui| {
                        knob(
                            ui,
                            ct,
                            "Bass",
                            &p.ffdn.bass_mult,
                            s,
                            "Bass RT60 multiplier.\n\
                             1.0x = same as mid, >1 = longer bass.",
                            ct.tex_shimmer,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Bass Hz",
                            &p.ffdn.bass_freq,
                            s,
                            "Low shelf crossover frequency.",
                            ct.tex_shimmer,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Treb",
                            &p.ffdn.treble_mult,
                            s,
                            "Treble RT60 multiplier.\n\
                             <1 = faster treble decay.",
                            ct.tex_shimmer,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Treb Hz",
                            &p.ffdn.treble_freq,
                            s,
                            "High shelf crossover frequency.",
                            ct.tex_shimmer,
                            scale,
                        );
                    });
                });
            });

            // ============ FFDN MODULATION ============
            let modulation_on = p.ffdn.modulation_enabled.value();
            draw_card(ui, t, scale, modulation_on, "ffdn_modulation", |ui, ct| {
                section_title_with_switch(
                    ui,
                    ct,
                    "MODULATION",
                    ct.tex_bloom,
                    "",
                    &p.ffdn.modulation_enabled,
                    s,
                    scale,
                );
                ui.add_enabled_ui(modulation_on, |ui| {
                    knob_row(ui, scale, 3, |ui| {
                        knob(
                            ui,
                            ct,
                            "Damping",
                            &p.ffdn.damping,
                            s,
                            "Primary HF shaper for the tail.\n\
                             A gentle one-pole low-pass (6 dB/oct) applied\n\
                             inside every delay line.\n\
                             500 Hz = dark, 20 kHz = transparent.\n\
                             Use this first — it is what gives the reverb\n\
                             its natural, room-like tone. For extra HF\n\
                             attenuation at longer tails, add LoopLP\n\
                             in the FFDN FILTER card on top of this.",
                            ct.tex_bloom,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Mod Amt",
                            &p.ffdn.mod_amount,
                            s,
                            "Depth of the LFO modulation on each delay line.\n\
                             0 = off (static tail).\n\
                             Higher = chorus-like movement.",
                            ct.tex_bloom,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Mod Rate",
                            &p.ffdn.mod_rate,
                            s,
                            "Speed of the modulation LFO.\n\
                             0.1 Hz = very slow breathing, 5 Hz = vibrato.",
                            ct.tex_bloom,
                            scale,
                        );
                    });
                });
            });

            // ============ FFDN DYNAMICS ============
            // Fills the same vertical span as FILTER and MIXER, so the
            // three bottom cards share one bottom edge.
            let dynamics_on = p.ffdn.dynamics_enabled.value();
            let remaining_px =
                (ui.available_height() - gap_y - BOTTOM_OFFSET * scale).max(60.0);
            let dynamics_h = ((remaining_px - 2.0 * pad_px) / scale).max(80.0);
            draw_card_h(
                ui,
                t,
                scale,
                dynamics_on,
                "ffdn_dynamics",
                dynamics_h,
                |ui, ct| {
                    section_title_with_switch(
                        ui,
                        ct,
                        "DYNAMICS",
                        ct.tex_scatter,
                        "",
                        &p.ffdn.dynamics_enabled,
                        s,
                        scale,
                    );
                    ui.add_enabled_ui(dynamics_on, |ui| {
                        // Vertical centering: since the card is now
                        // taller than the 3-knob content, push the row
                        // down so the visual weight stays balanced.
                        let used = KNOB_SLOT_H * scale + 6.0 * scale;
                        let target =
                            dynamics_h * scale - CARD_TITLE_H * scale - 12.0 * scale;
                        if target > used {
                            ui.add_space((target - used) * 0.5);
                        }
                        knob_row(ui, scale, 3, |ui| {
                            knob(
                                ui,
                                ct,
                                "Attack",
                                &p.ffdn.attack,
                                s,
                                "Softens the initial transient of the wet\n\
                                 signal. 0 % = instant (bit-identical),\n\
                                 higher = slow swell up to 500 ms.",
                                ct.tex_scatter,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Ducking",
                                &p.ffdn.ducking,
                                s,
                                "Reduces the wet signal while the dry input\n\
                                 is present, then recovers in the silence.\n\
                                 0 % = off (bit-identical).",
                                ct.tex_scatter,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Gate",
                                &p.ffdn.gate,
                                s,
                                "Progressively silences the wet once its\n\
                                 envelope drops below a threshold.\n\
                                 0 % = off (bit-identical).",
                                ct.tex_scatter,
                                scale,
                            );
                        });
                    });
                },
            );
        });
    });
}
// ============================================================================
// TEXTURE page
// ============================================================================

fn draw_texture_page(
    ui: &mut Ui,
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    t: &Theme,
    scale: f32,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = PAGE_GAP * scale;

        // ---------- Row 1: SHIMMER | MIST | DRIFT ----------
        ui.spacing_mut().item_spacing.x = TEX_COLUMN_GAP * scale;
        ui.columns(3, |cols| {
            cols[0].vertical(|ui| {
                ui.spacing_mut().item_spacing.y = PAGE_GAP * scale;
                draw_card(ui, t, scale, p.shimmer.enabled.value(), "tex_shimmer", |ui, ct| {
                    section_title_with_enabled_switch(
                        ui, ct, "SHIMMER", ct.tex_shimmer, &p.shimmer.enabled, s, scale,
                    );
                    let card_on = p.shimmer.enabled.value();
                    ui.add_enabled_ui(card_on, |ui| {
                        knob_row_compact(ui, scale, 5, |ui| {
                            knob_compact(ui, ct, "Pitch", &p.shimmer.pitch, s,
                                "Pitch shift.\n0 = -12 st, 0.5 = unison, 1.0 = +12 st.",
                                ct.tex_shimmer, scale);
                            knob_compact(ui, ct, "Feedback", &p.shimmer.feedback, s,
                                "Feedback amount.", ct.tex_shimmer, scale);
                            knob_compact(ui, ct, "Mix", &p.shimmer.mix, s,
                                "Amount of shimmer tail.", ct.tex_shimmer, scale);
                            knob_compact(ui, ct, "LowCut", &p.shimmer.low_cut, s,
                                "HP before pitch shifter.", ct.tex_shimmer, scale);
                            knob_compact(ui, ct, "HighCut", &p.shimmer.high_cut, s,
                                "LP after pitch shifter.", ct.tex_shimmer, scale);
                        });
                    });
                });
            });

            cols[1].vertical(|ui| {
                ui.spacing_mut().item_spacing.y = PAGE_GAP * scale;
                draw_card(ui, t, scale, p.mist.enabled.value(), "tex_mist", |ui, ct| {
                    section_title_with_enabled_switch(
                        ui, ct, "MIST", ct.tex_mist, &p.mist.enabled, s, scale,
                    );
                    let card_on = p.mist.enabled.value();
                    ui.add_enabled_ui(card_on, |ui| {
                        knob_row_compact(ui, scale, 5, |ui| {
                            knob_compact(ui, ct, "HP", &p.mist.hp, s,
                                "HP before delay.", ct.tex_mist, scale);
                            knob_compact(ui, ct, "Feedback", &p.mist.feedback, s,
                                "Diffusion feedback.", ct.tex_mist, scale);
                            knob_compact(ui, ct, "Mix", &p.mist.mix, s,
                                "Amount of mist.", ct.tex_mist, scale);
                            knob_compact(ui, ct, "Delay", &p.mist.delay, s,
                                "Base delay (8..40 ms).", ct.tex_mist, scale);
                            knob_compact(ui, ct, "Rate", &p.mist.rate, s,
                                "LFO rate (0.05..25 Hz).", ct.tex_mist, scale);
                        });
                    });
                });
            });

            cols[2].vertical(|ui| {
                ui.spacing_mut().item_spacing.y = PAGE_GAP * scale;
                draw_card(ui, t, scale, p.drift.enabled.value(), "tex_drift", |ui, ct| {
                    section_title_with_enabled_switch(
                        ui, ct, "DRIFT", ct.tex_drift, &p.drift.enabled, s, scale,
                    );
                    let card_on = p.drift.enabled.value();
                    ui.add_enabled_ui(card_on, |ui| {
                        knob_row_compact(ui, scale, 5, |ui| {
                            knob_compact(ui, ct, "Feedback", &p.drift.feedback, s,
                                "Drift feedback.", ct.tex_drift, scale);
                            knob_compact(ui, ct, "Delay", &p.drift.delay, s,
                                "Base delay (5..25 ms).", ct.tex_drift, scale);
                            knob_compact(ui, ct, "Mix", &p.drift.mix, s,
                                "Amount of drift.", ct.tex_drift, scale);
                            knob_compact(ui, ct, "Rate", &p.drift.rate, s,
                                "LFO rate (0.02..0.52 Hz).", ct.tex_drift, scale);
                            knob_compact(ui, ct, "Depth", &p.drift.depth, s,
                                "Modulation depth.", ct.tex_drift, scale);
                        });
                    });
                });
            });
        });

        // ---------- Row 2: CRYSTAL | SCATTER | BLOOM ----------
        ui.spacing_mut().item_spacing.x = TEX_COLUMN_GAP * scale;
        ui.columns(3, |cols| {
            cols[0].vertical(|ui| {
                ui.spacing_mut().item_spacing.y = PAGE_GAP * scale;
                draw_card(ui, t, scale, p.crystal.enabled.value(), "tex_crystal", |ui, ct| {
                    section_title_with_enabled_switch(
                        ui, ct, "CRYSTAL", ct.tex_crystal, &p.crystal.enabled, s, scale,
                    );
                    let card_on = p.crystal.enabled.value();
                    ui.add_enabled_ui(card_on, |ui| {
                        knob_row_compact(ui, scale, 4, |ui| {
                            knob_compact(ui, ct, "Size", &p.crystal.size, s,
                                "Grain length (15..150 ms).", ct.tex_crystal, scale);
                            knob_compact(ui, ct, "Density", &p.crystal.density, s,
                                "Time between grains (20..250 ms).", ct.tex_crystal, scale);
                            knob_compact(ui, ct, "Mix", &p.crystal.mix, s,
                                "Amount of granular texture.", ct.tex_crystal, scale);
                            knob_compact(ui, ct, "Feedback", &p.crystal.feedback, s,
                                "Grain cloud feedback.", ct.tex_crystal, scale);
                        });
                    });
                });
            });

            cols[1].vertical(|ui| {
                ui.spacing_mut().item_spacing.y = PAGE_GAP * scale;
                draw_card(ui, t, scale, p.scatter.enabled.value(), "tex_scatter", |ui, ct| {
                    section_title_with_enabled_switch(
                        ui, ct, "SCATTER", ct.tex_scatter, &p.scatter.enabled, s, scale,
                    );
                    let card_on = p.scatter.enabled.value();
                    ui.add_enabled_ui(card_on, |ui| {
                        knob_row_compact(ui, scale, 5, |ui| {
                            knob_compact(ui, ct, "Spread", &p.scatter.spread, s,
                                "Delay spread (30..400 ms).", ct.tex_scatter, scale);
                            knob_compact(ui, ct, "Density", &p.scatter.density, s,
                                "Grain spawn rate.", ct.tex_scatter, scale);
                            knob_compact(ui, ct, "Mix", &p.scatter.mix, s,
                                "Amount of scattered echoes.", ct.tex_scatter, scale);
                            knob_compact(ui, ct, "Speed", &p.scatter.speed, s,
                                "Spawn rate scaling.", ct.tex_scatter, scale);
                            knob_compact(ui, ct, "Feedback", &p.scatter.feedback, s,
                                "Scatter feedback.", ct.tex_scatter, scale);
                        });
                    });
                });
            });

            cols[2].vertical(|ui| {
                ui.spacing_mut().item_spacing.y = PAGE_GAP * scale;
                draw_card(ui, t, scale, p.bloom.enabled.value(), "tex_bloom", |ui, ct| {
                    section_title_with_enabled_switch(
                        ui, ct, "BLOOM", ct.tex_bloom, &p.bloom.enabled, s, scale,
                    );
                    let card_on = p.bloom.enabled.value();
                    ui.add_enabled_ui(card_on, |ui| {
                        knob_row_compact(ui, scale, 5, |ui| {
                            knob_compact(ui, ct, "Rise", &p.bloom.rise, s,
                                "Attack time of the blooming envelope.\n0 = fast (50 ms), 1 = slow (2 s).",
                                ct.tex_bloom, scale);
                            knob_compact(ui, ct, "Size", &p.bloom.size, s,
                                "Diffusion space size (60..250 ms).",
                                ct.tex_bloom, scale);
                            knob_compact(ui, ct, "Mix", &p.bloom.mix, s,
                                "Amount of blooming texture.",
                                ct.tex_bloom, scale);
                            knob_compact(ui, ct, "Feedback", &p.bloom.feedback, s,
                                "Bloom feedback amount.",
                                ct.tex_bloom, scale);
                            knob_compact(ui, ct, "Tone", &p.bloom.tone, s,
                                "Brightness (2 kHz .. 12 kHz).",
                                ct.tex_bloom, scale);
                        });
                    });
                });
            });
        });

        // ---------- Row 3: SIGNAL CHAIN (full width) ----------
        draw_texture_order_card(ui, p, t, scale);

        // ---------- Row 4: ILLUSION (full width) ----------
        draw_chime_card(ui, p, s, t, scale);
    });
}

/// Draws the "SIGNAL CHAIN" card at the bottom of the TEXTURE page.
///
/// Shows the six texture layers as a horizontal chain of chips joined
/// by arrows. Interaction model:
///
///   * **Live drag & reorder**: grab any chip and move it left or
///     right. As soon as the pointer crosses the *centre* of a
///     neighbouring chip, the two swap places immediately, and both
///     chips slide smoothly to their new slots over ~130 ms. Dragging
///     back reverses the swap just as smoothly. A floating ghost
///     follows the cursor so the user always knows which chip is
///     being moved.
///
///   * **Click on an arrow**: swaps the two adjacent layers with the
///     same animation.
fn draw_texture_order_card(ui: &mut Ui, p: &Arc<PejvakRvbParams>, t: &Theme, scale: f32) {
    use std::sync::atomic::Ordering;

    const NAMES: [&str; 6] = ["SHIMMER", "MIST", "DRIFT", "CRYSTAL", "SCATTER", "BLOOM"];
    const NO_DRAG: usize = usize::MAX;

    // Decode current order from the packed atomic.
    let packed = p.texture_order.load(Ordering::Relaxed);
    let order: [usize; 6] = [
        (packed & 0xFF) as usize,
        ((packed >> 8) & 0xFF) as usize,
        ((packed >> 16) & 0xFF) as usize,
        ((packed >> 24) & 0xFF) as usize,
        ((packed >> 32) & 0xFF) as usize,
        ((packed >> 40) & 0xFF) as usize,
    ];
    let mut new_order = order;

    // Persistent state: which layer is being dragged. Keyed by layer
    // index (0..5) so it survives the live reorders that happen
    // mid-drag.
    let drag_layer_id = egui::Id::new("texture_chain_dragged_layer");
    let mut dragged_layer: usize = ui
        .memory(|m| m.data.get_temp(drag_layer_id))
        .unwrap_or(NO_DRAG);

    // If no pointer button is held down, we cannot be dragging. Clears
    // any stale state from a frame where the drag ended without
    // `drag_stopped()` firing (e.g. window focus loss mid-drag).
    if !ui.input(|i| i.pointer.any_down()) {
        dragged_layer = NO_DRAG;
    }

    let mut any_animating = false;

    draw_card_h(ui, t, scale, true, "texture_order", 60.0, |ui, ct| {
        section_title(ui, ct, "SIGNAL CHAIN", ct.accent, scale);

        ui.label(
            egui::RichText::new("Drag a chip to reorder · Click an arrow to swap adjacent layers")
                .size(8.5 * scale)
                .color(ct.dim2),
        );
        ui.add_space(4.0 * scale);

        let colors: [Color32; 6] = [
            ct.tex_shimmer,
            ct.tex_mist,
            ct.tex_drift,
            ct.tex_crystal,
            ct.tex_scatter,
            ct.tex_bloom,
        ];

        // Enabled state of every layer. Read directly from the
        // parameters, so the chip colours reflect the actual switch
        // positions — including changes made by the TX kill button,
        // host automation, or a preset load.
        let enabled: [bool; 6] = [
            p.shimmer.enabled.value(),
            p.mist.enabled.value(),
            p.drift.enabled.value(),
            p.crystal.enabled.value(),
            p.scatter.enabled.value(),
            p.bloom.enabled.value(),
        ];

        // ---- Layout geometry ------------------------------------------
        // compressed chip height (52 → 38) so the whole
        // SIGNAL CHAIN card shrinks by ~14 px. Combined with the
        // single-row ILLUSION card below it, the TEXTURE page now fits
        // inside the plugin window without scrolling at every UI
        // scale.
        let chip_h = 38.0 * scale;
        let arrow_w = 24.0 * scale;
        let gap = 6.0 * scale;
        let avail = ui.available_width();
        let chip_w = ((avail - arrow_w * 5.0 - gap * 10.0) / 6.0).max(40.0 * scale);

        // Reserve the entire row as a single allocation. Individual
        // slot rects are computed by hand so that they stay put while
        // the chips animate through them.
        let row_w = chip_w * 6.0 + arrow_w * 5.0 + gap * 10.0;
        let (row_rect, _) = ui.allocate_exact_size(Vec2::new(row_w, chip_h), Sense::hover());
        let left = row_rect.left();
        let top = row_rect.top();

        let mut chip_rects = [Rect::ZERO; 6];
        let mut arrow_rects = [Rect::ZERO; 5];
        let mut x = left;
        for i in 0..6 {
            chip_rects[i] = Rect::from_min_size(Pos2::new(x, top), Vec2::new(chip_w, chip_h));
            x += chip_w + gap;
            if i < 5 {
                arrow_rects[i] = Rect::from_min_size(Pos2::new(x, top), Vec2::new(arrow_w, chip_h));
                x += arrow_w + gap;
            }
        }

        // ---- Chip interactions ----------------------------------------
        // The interaction rect is pinned to the chip's *slot*, not to
        // its animated visual position. That keeps the swap logic
        // stable: the pointer's x-coordinate is compared against slot
        // centres, never against in-flight chip positions.
        let mut chip_hover = [false; 6];
        for pos in 0..6 {
            let layer = new_order[pos];
            let chip_id = egui::Id::new(("chain_chip", layer));
            let resp = ui.interact(chip_rects[pos], chip_id, Sense::click_and_drag());

            if resp.drag_started() {
                dragged_layer = layer;
            }
            if resp.drag_stopped() {
                dragged_layer = NO_DRAG;
            }
            chip_hover[pos] = resp.hovered() && dragged_layer != layer;

            // Cursor feedback:
            //   * "grabbing" while this chip is being dragged, or the
            //     pointer button is pressed down on it (covers both
            //     the click-and-hold gesture and the active drag).
            //   * "pointing hand" on hover, but only while no chip is
            //     being dragged, so the cursor stays "grabbing" when
            //     the pointer travels over other chips mid-drag.
            if dragged_layer == layer || resp.is_pointer_button_down_on() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if resp.hovered() && dragged_layer == NO_DRAG {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }

        // ---- Arrow interactions ---------------------------------------
        let mut arrow_hover = [false; 5];
        for i in 0..5 {
            let arrow_id = egui::Id::new(("chain_arrow", i));
            let resp = ui.interact(arrow_rects[i], arrow_id, Sense::click());
            arrow_hover[i] = resp.hovered();

            // Arrow clicks are disabled while a chip is being dragged,
            // so the two interaction models never overlap in one
            // gesture.
            if resp.clicked() && dragged_layer == NO_DRAG {
                new_order.swap(i, i + 1);
            }
            if resp.hovered() && dragged_layer == NO_DRAG {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }

        // ---- Live reorder ---------------------------------------------
        // Trigger: the dragged chip's pointer has crossed the
        // **midpoint** between the current slot and an adjacent slot.
        // Two independent checks (left and right) run on every frame,
        // so a drag in either direction triggers a swap as soon as the
        // pointer has gone half a slot away.
        //
        // Immediate-mode means at most one swap per frame; the lerp
        // animation below hides any latency completely.
        //
        // The two midpoint checks are mutually exclusive by geometry
        // (`mid_left < center(dp) < mid_right`), so running both in the
        // same frame can never trigger two swaps — only one of them can
        // possibly be true.
        if dragged_layer != NO_DRAG {
            if let Some(pointer) = ui.ctx().pointer_latest_pos() {
                if let Some(dp) = new_order.iter().position(|&l| l == dragged_layer) {
                    let px = pointer.x;

                    // --- Check swap with the RIGHT neighbour ------------
                    if dp + 1 < 6 {
                        let mid_right =
                            (chip_rects[dp].center().x + chip_rects[dp + 1].center().x) * 0.5;
                        if px > mid_right {
                            new_order.swap(dp, dp + 1);
                        }
                    }

                    // --- Check swap with the LEFT neighbour -------------
                    // Deliberately NOT an `else if`: the previous version
                    // used `else if`, which meant a chip sitting in the
                    // middle of the chain could only ever swap with its
                    // right neighbour — dragging to the left was silently
                    // ignored.
                    if dp > 0 {
                        let mid_left =
                            (chip_rects[dp - 1].center().x + chip_rects[dp].center().x) * 0.5;
                        if px < mid_left {
                            new_order.swap(dp - 1, dp);
                        }
                    }
                }
            }
        }

        // ---- Animated visual positions --------------------------------
        // Each layer has its own "visual centre x" in egui memory.
        // Every frame it lerps toward the centre of its current slot.
        // The lerp factor is `dt * SPEED`, framerate-independent.
        let dt = ui.input(|i| i.stable_dt).clamp(0.0, 0.05);
        const SLIDE_SPEED: f32 = 20.0;

        let mut vis_x = [0.0_f32; 6];
        for pos in 0..6 {
            let layer = new_order[pos];
            let target_cx = chip_rects[pos].center().x;
            let vis_id = egui::Id::new(("chain_chip_vis", layer));

            let current: Option<f32> = ui.memory(|m| m.data.get_temp(vis_id));
            let start = current.unwrap_or(target_cx);

            let new_x = if (start - target_cx).abs() < 0.3 {
                target_cx
            } else {
                any_animating = true;
                let k = (dt * SLIDE_SPEED).clamp(0.0, 1.0);
                start + (target_cx - start) * k
            };

            ui.memory_mut(|m| m.data.insert_temp(vis_id, new_x));
            vis_x[layer] = new_x;
        }

        // ---- Draw the chips -------------------------------------------
        // Chips are painted at their *animated* positions, and painted
        // in the order of the sorted visual positions so that a chip
        // that is currently "behind" another (during a slide past)
        // appears beneath. For left-right slides this is a non-issue,
        // but the sort keeps the visual stacking sensible in either
        // direction.
        let painter = ui.painter();

        // Accent-grey colour used when a layer is switched off. This
        // matches the `greyed()` theme so the chips read as part of
        // the same visual system as the cards.
        let chip_grey = Color32::from_rgb(0x55, 0x58, 0x60);

        for pos in 0..6 {
            let layer = new_order[pos];
            let is_dragged = dragged_layer == layer;

            let hover_anim = if is_dragged {
                0.0
            } else {
                ui.ctx().animate_bool_with_time(
                    egui::Id::new(("chain_chip_hover_anim", layer)),
                    chip_hover[pos],
                    0.15,
                )
            };

            // Smoothly interpolate the chip's accent colour between
            // grey (layer disabled) and its full accent (layer
            // enabled). Uses the same 0.25 s curve as the card body
            // fade, so chip and card grey out together.
            let enabled_anim = ui.ctx().animate_bool_with_time(
                egui::Id::new(("chain_chip_enabled_anim", layer)),
                enabled[layer],
                0.25,
            );
            if enabled_anim > 0.001 && enabled_anim < 0.999 {
                any_animating = true;
            }
            let base_color = lerp_color(chip_grey, colors[layer], enabled_anim);

            let color = if is_dragged {
                base_color.gamma_multiply(0.30)
            } else {
                base_color
            };

            let visual_rect = Rect::from_min_size(
                Pos2::new(vis_x[layer] - chip_w * 0.5, top),
                Vec2::new(chip_w, chip_h),
            );

            if ui.is_rect_visible(visual_rect) {
                draw_chain_chip(painter, visual_rect, color, NAMES[layer], scale, hover_anim);
            }
        }

        // ---- Draw the arrows at fixed slot positions ------------------
        for i in 0..5 {
            draw_chain_arrow(painter, arrow_rects[i], ct, arrow_hover[i], scale);
        }

        // ---- Ghost that follows the cursor ----------------------------
        if dragged_layer != NO_DRAG {
            if let Some(pointer) = ui.ctx().pointer_latest_pos() {
                let ghost_layer_id =
                    egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("texture_order_ghost"));
                let layer_painter = ui.ctx().layer_painter(ghost_layer_id);
                let ghost_rect = Rect::from_center_size(
                    pointer + Vec2::new(0.0, -chip_h * 0.55),
                    Vec2::new(chip_w, chip_h),
                );
                draw_chain_chip(
                    &layer_painter,
                    ghost_rect,
                    colors[dragged_layer],
                    NAMES[dragged_layer],
                    scale,
                    0.7,
                );
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    });

    // Persist drag state for next frame.
    ui.memory_mut(|m| m.data.insert_temp(drag_layer_id, dragged_layer));

    // If any chip is mid-slide, keep the animation advancing even
    // without pointer movement.
    if any_animating {
        ui.ctx().request_repaint();
    }

    // If the order changed, re-pack and store. The audio thread picks
    // it up on the next block.
    if new_order != order {
        let mut packed: u64 = 0;
        for (i, &idx) in new_order.iter().enumerate() {
            packed |= (idx as u64) << (i * 8);
        }
        p.texture_order.store(packed, Ordering::Relaxed);
        ui.ctx().request_repaint();
    }
}

fn draw_chime_card(ui: &mut Ui, p: &Arc<PejvakRvbParams>, s: &ParamSetter, t: &Theme, scale: f32) {
    let chime_on = p.illusion.enabled.value();

    // Animate the accent colour so it greys out with the rest of the
    // card. Uses the same 0.25 s curve as `draw_card_h`, so the
    // accent and the card background always move in lockstep.
    let accent_anim =
        ui.ctx()
            .animate_bool_with_time(egui::Id::new("chime_accent_fade"), chime_on, 0.25);
    let accent_grey = Color32::from_rgb(0x55, 0x58, 0x60);
    let accent = lerp_color(accent_grey, t.tex_chime, accent_anim);

    draw_card_h(ui, t, scale, chime_on, "illusion", 80.0, |ui, ct| {
        section_title_with_enabled_switch(ui, ct, "ILLUSION", accent, &p.illusion.enabled, s, scale);
        ui.add_enabled_ui(chime_on, |ui| {
            // All eight ILLUSION knobs in a single row:
            //   Spark, Still, Aura, Veil, Wide, Tint, Rate, Grain.
            // 8 × 52 px + 7 gaps of 6 px = 458 px, which fits
            // comfortably inside the card at every UI scale and
            // keeps the card one row tall so the TEXTURE page never
            // overflows the plugin window.
            //
            // NOTE: `count` must match the number of `knob_compact`
            // calls below. It is used by `knob_row_compact` to centre
            // the row inside the card; a mismatch shifts the row
            // sideways. An earlier version left this at 10 (a leftover
            // from the pre-4.86 layout, when ILLUSION still had separate
            // Fade In / Fade Out knobs), which pushed the row ~2 knob
            // widths to the left.
            knob_row_compact(ui, scale, 8, |ui| {
                knob_compact(
                    ui, ct, "Spark", &p.illusion.spark, s,
                    "Granular cloud amount.\n0 % = bypass (dry passthrough).\nHigher = more of the pitched texture.",
                    accent, scale,
                );
                knob_compact(
                    ui, ct, "Still", &p.illusion.still, s,
                    "Motion smoothness.\n0 % = scattered grains across the buffer.\n100 % = calm, meditative flow.",
                    accent, scale,
                );
                knob_compact(
                    ui, ct, "Aura", &p.illusion.aura, s,
                    "Pitch shift in semitones.\n−24 st … +24 st.\n0 = unison (internal sequence).",
                    accent, scale,
                );
                knob_compact(
                    ui, ct, "Veil", &p.illusion.veil, s,
                    "Feedback into the capture buffer.\nHigher = repeating patterns, hazy wash.",
                    accent, scale,
                );
                knob_compact(
                    ui, ct, "Wide", &p.illusion.wide, s,
                    "Stereo width of the wet signal.\n0 % = mono, 100 % = natural, 200 % = wide.",
                    accent, scale,
                );
                knob_compact(
                    ui, ct, "Tint", &p.illusion.tint, s,
                    "Tone of the wet signal.\nLow = dark (800 Hz).\nHigh = bright (18 kHz).",
                    accent, scale,
                );
                knob_compact(
                    ui, ct, "Rate", &p.illusion.rate, s,
                    "Clock speed.\n0.5 Hz = very slow (one note every 2 s).\n8 Hz = fast arpeggio.",
                    accent, scale,
                );
                knob_compact(
                    ui, ct, "Grain", &p.illusion.grain, s,
                    "Length of each grain (30..300 ms).\nLonger = smoother, more legato.\nShorter = staccato, more percussive.",
                    accent, scale,
                );
            });
        });
    });
}

/// Draws a single chip in the SIGNAL CHAIN diagram as a chevron
/// ("process arrow") shape: a hexagon with a pointed right edge and
/// a notched left edge. The notch and the point have the same depth,
/// so adjacent chips visually slot into each other like the stages
/// of a flow diagram.
///
/// `hover` (0..1) brightens the fill. No border, no shadow — the
/// shape itself carries the visual weight.
/// draws a single chip's hexagon outline as a filled mesh.
/// Factored out of `draw_chain_chip` so the base and the hover
/// highlight can both use the same geometry.
/// computes the vertices of a hexagon's inward offset by
/// `d` pixels, so that every new edge is exactly `d` away from the
/// original.
///
/// Uses the interior-angle-bisector trick: for any vertex, moving
/// along the bisector by `d / sin(θ/2)` produces edges that are
/// uniformly `d` from the originals. Handles both convex vertices
/// and the reflex notch vertex correctly.
fn inset_hexagon_vertices(outer: &[Pos2; 6], d: f32) -> [Pos2; 6] {
    // Centroid (mean of vertices) — used only to decide which way
    // "inward" is for each vertex.
    let mut cx = 0.0_f32;
    let mut cy = 0.0_f32;
    for p in outer {
        cx += p.x;
        cy += p.y;
    }
    let centroid = Pos2::new(cx / 6.0, cy / 6.0);

    let mut inner = [Pos2::ZERO; 6];
    for i in 0..6 {
        let prev = outer[(i + 5) % 6];
        let curr = outer[i];
        let next = outer[(i + 1) % 6];

        let v_prev = prev - curr;
        let v_next = next - curr;
        let l_prev = v_prev.length().max(1e-6);
        let l_next = v_next.length().max(1e-6);

        let u_prev = v_prev / l_prev;
        let u_next = v_next / l_next;

        // Interior half-angle distance factor.
        let cos_theta = u_prev.dot(u_next).clamp(-1.0, 1.0);
        let sin_half = ((1.0 - cos_theta) * 0.5).max(1e-8).sqrt();

        // Angle bisector; flip it if it points away from the centroid.
        let mut bisector = u_prev + u_next;
        let bl = bisector.length().max(1e-6);
        bisector /= bl;
        if bisector.dot(centroid - curr) < 0.0 {
            bisector = -bisector;
        }

        inner[i] = curr + bisector * (d / sin_half);
    }
    inner
}

/// draws a filled hexagon with anti-aliasing.
///
/// The hexagon has one reflex vertex (the notch), so we split it
/// into two convex quads that share the diagonal from notch (5) to
/// tip (2). Both halves are strictly convex, so `convex_polygon`
/// runs them through egui's AA tessellator.
fn draw_hexagon_aa(painter: &egui::Painter, verts: &[Pos2; 6], fill: Color32) {
    let q1: Vec<Pos2> = vec![verts[5], verts[0], verts[1], verts[2]];
    let q2: Vec<Pos2> = vec![verts[5], verts[2], verts[3], verts[4]];
    painter.add(egui::Shape::convex_polygon(q1, fill, egui::Stroke::NONE));
    painter.add(egui::Shape::convex_polygon(q2, fill, egui::Stroke::NONE));
}

fn draw_chain_chip(
    painter: &egui::Painter,
    rect: Rect,
    color: Color32,
    name: &str,
    scale: f32,
    hover: f32,
) {
    let i = hover.clamp(0.0, 1.0);
    // Notch scales with the (now smaller) chip height. 0.40 gives
    // the same visual proportion as before at chip_h = 38.
    let notch = (rect.height() * 0.40).min(12.0 * scale);
    let mid_y = rect.center().y;

    let outer: [Pos2; 6] = [
        Pos2::new(rect.left(), rect.top()),
        Pos2::new(rect.right() - notch, rect.top()),
        Pos2::new(rect.right(), mid_y),
        Pos2::new(rect.right() - notch, rect.bottom()),
        Pos2::new(rect.left(), rect.bottom()),
        Pos2::new(rect.left() + notch, mid_y),
    ];

    // Base fill (dim).
    draw_hexagon_aa(painter, &outer, color.gamma_multiply(0.16));

    if i > 0.01 {
        let max_inset = rect.height().min(rect.width()) * 0.4;
        let inset = (2.0 * scale).min(max_inset);
        let inner = inset_hexagon_vertices(&outer, inset);
        let highlight = color.gamma_multiply(0.16 + 0.12 * i);
        draw_hexagon_aa(painter, &inner, highlight);
    }

    // Font reduced by ~10 % to match the shorter chip.
    painter.text(
        Pos2::new(rect.center().x, mid_y),
        egui::Align2::CENTER_CENTER,
        name,
        egui::FontId::proportional(9.5 * scale),
        color,
    );
}

/// Draws the separator between two chips in the SIGNAL CHAIN
/// diagram: a small ">>" glyph made of two chevrons. The double
/// chevron reads as "flow continues" and complements the chevron
/// shape of the chips themselves.
fn draw_chain_arrow(painter: &egui::Painter, rect: Rect, t: &Theme, hover: bool, scale: f32) {
    let color = if hover { t.accent } else { t.dim2 };

    let stroke = Stroke::new(3.0 * scale, color);
    let head = 6.5 * scale; // half-height of each chevron
    let depth = 5.0 * scale; // horizontal extent of each chevron
    let gap = 8.0 * scale; // spacing between the two chevrons

    // shift the visual centre slightly to the right.
    //
    // The ">" glyphs have their opening on the left and their tip
    // on the right, which makes the eye read the glyph as
    // left-heavy: even though the geometric centre of the two
    // chevrons is exactly at the middle of the arrow rect, the eye
    // perceives the pair as sitting closer to the left chip.
    //
    // A small rightward offset (3 logical units, scaled) restores
    // the perceived balance so the ">>" reads as centred between
    // the two chips.
    let c = rect.center() + Vec2::new(5.0 * scale, 0.0);

    for k in 0..2 {
        // Horizontal position of this chevron's tip.
        let tip_x = c.x + (k as f32 - 0.5) * (depth + gap);

        // Upper arm: from tip back up-left.
        painter.line_segment(
            [Pos2::new(tip_x - depth, c.y - head), Pos2::new(tip_x, c.y)],
            stroke,
        );
        // Lower arm: from tip back down-left.
        painter.line_segment(
            [Pos2::new(tip_x, c.y), Pos2::new(tip_x - depth, c.y + head)],
            stroke,
        );
    }
}

// ============================================================================
// Preset menu
// ============================================================================

fn preset_menu(
    ui: &mut Ui,
    t: &Theme,
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    scale: f32,
    current: &mut String,
    user_presets: &[String],
) {
    #[derive(Clone)]
    enum Action {
        Apply(usize),
        Save,
        Load,
        UserPreset(String),
    }

    let is_light = t.bg_top.r() > 128;
    let trigger_id = egui::Id::new("pejvak_preset_trigger");
    let state_id = trigger_id.with("state");
    let opened_at_id = trigger_id.with("opened_at");
    let closing_at_id = trigger_id.with("closing_at");

    // Duration of the preset popup open/close fade. Slightly snappier
    // than the modal panel fade (0.22 s) because the preset menu is a
    // frequently-used control and users expect it to feel responsive.
    const PRESET_FADE_SECS: f64 = 0.14;

    let mut popup_open: bool = ui.memory(|m| m.data.get_temp(state_id).unwrap_or(false));
    let now = ui.input(|i| i.time);
    let opened_at: f64 = ui.memory(|m| m.data.get_temp(opened_at_id).unwrap_or(now));
    let closing_at: f64 = ui.memory(|m| m.data.get_temp(closing_at_id).unwrap_or(0.0));
    let is_closing = closing_at > 0.0;

    // Smoothstep-eased fade progress. Ramps 0 → 1 while opening,
    // 1 → 0 while closing.
    let raw = if is_closing {
        ((closing_at - now) / PRESET_FADE_SECS).clamp(0.0, 1.0) as f32
    } else {
        ((now - opened_at) / PRESET_FADE_SECS).clamp(0.0, 1.0) as f32
    };
    let popup_fade = raw * raw * (3.0 - 2.0 * raw);

    // Closing animation finished: actually close the popup and clear
    // the animation timestamps so the next open starts fresh.
    if is_closing && popup_fade <= 0.0 {
        ui.memory_mut(|m| {
            m.data.insert_temp(state_id, false);
            m.data.remove::<f64>(opened_at_id);
            m.data.remove::<f64>(closing_at_id);
        });
        popup_open = false;
    }

    // Keep the frame loop alive during the fade so the animation
    // plays even when the user is not moving the mouse.
    if popup_open && popup_fade < 1.0 {
        ui.ctx().request_repaint();
    }

    let nav_w = 24.0 * scale;
    let trigger_w = 176.0 * scale;
    let trigger_h = 24.0 * scale;
    let gap = 3.0 * scale;

    let (row_rect, _) = ui.allocate_exact_size(
        Vec2::new(nav_w * 2.0 + gap * 2.0 + trigger_w, trigger_h),
        Sense::hover(),
    );

    let prev_rect = Rect::from_min_size(row_rect.min, Vec2::new(nav_w, trigger_h));
    let trigger_rect = Rect::from_min_size(
        Pos2::new(prev_rect.right() + gap, row_rect.top()),
        Vec2::new(trigger_w, trigger_h),
    );
    let next_rect = Rect::from_min_size(
        Pos2::new(trigger_rect.right() + gap, row_rect.top()),
        Vec2::new(nav_w, trigger_h),
    );

    let prev_resp = hover_tip(
        ui.interact(prev_rect, trigger_id.with("prev"), Sense::click()),
        "Previous preset",
    );
    let next_resp = hover_tip(
        ui.interact(next_rect, trigger_id.with("next"), Sense::click()),
        "Next preset",
    );
    let trigger_resp = hover_tip(
        ui.interact(trigger_rect, trigger_id.with("open"), Sense::click()),
        "Browse presets",
    );

    if trigger_resp.clicked() {
        if is_closing {
            // Ignore clicks during the close fade — a second click
            // would otherwise flip the state back to "open" and
            // stutter the animation.
        } else if popup_open {
            // Start the closing fade instead of snapping closed.
            ui.memory_mut(|m| {
                m.data.insert_temp(closing_at_id, now + PRESET_FADE_SECS);
            });
        } else {
            // Open with a fresh fade-in.
            popup_open = true;
            ui.memory_mut(|m| {
                m.data.insert_temp(state_id, true);
                m.data.insert_temp(opened_at_id, now);
                m.data.remove::<f64>(closing_at_id);
            });
        }
    }

    if prev_resp.clicked() {
        let cur_idx = FACTORY_PRESETS
            .iter()
            .position(|fp| fp.name == current.as_str())
            .unwrap_or(0);
        let new_idx = if cur_idx == 0 {
            FACTORY_PRESETS.len() - 1
        } else {
            cur_idx - 1
        };
        apply_factory_preset(p, s, &FACTORY_PRESETS[new_idx]);
        *current = FACTORY_PRESETS[new_idx].name.to_string();
        ui.memory_mut(|m| m.data.insert_temp(state_id, popup_open));
        return;
    }
    if next_resp.clicked() {
        let cur_idx = FACTORY_PRESETS
            .iter()
            .position(|fp| fp.name == current.as_str())
            .unwrap_or(0);
        let new_idx = (cur_idx + 1) % FACTORY_PRESETS.len();
        apply_factory_preset(p, s, &FACTORY_PRESETS[new_idx]);
        *current = FACTORY_PRESETS[new_idx].name.to_string();
        ui.memory_mut(|m| m.data.insert_temp(state_id, popup_open));
        return;
    }

    let cur_cat = FACTORY_PRESETS
        .iter()
        .find(|fp| fp.name == current.as_str())
        .map(|fp| fp.category)
        .unwrap_or(0);
    let cur_accent = CATEGORIES[cur_cat].color;

    let nav_col = t.dim;
    let nav_hover_col = t.accent;

    if ui.is_rect_visible(prev_rect) {
        let painter = ui.painter();
        painter.rect_filled(prev_rect, 5.0 * scale, t.track_bg.gamma_multiply(0.6));
        let c = prev_rect.center();
        let col = if prev_resp.hovered() {
            nav_hover_col
        } else {
            nav_col
        };
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(c.x + 3.0 * scale, c.y - 4.0 * scale),
                Pos2::new(c.x + 3.0 * scale, c.y + 4.0 * scale),
                Pos2::new(c.x - 3.0 * scale, c.y),
            ],
            col,
            Stroke::NONE,
        ));
    }

    if ui.is_rect_visible(trigger_rect) {
        let painter = ui.painter();
        painter.rect_filled(trigger_rect, 5.0 * scale, t.track_bg.gamma_multiply(0.6));
        let dot_c = Pos2::new(trigger_rect.left() + 12.0 * scale, trigger_rect.center().y);
        painter.circle_filled(dot_c, 3.5 * scale, cur_accent);
        let label = if current.is_empty() {
            "Presets"
        } else {
            current.as_str()
        };
        painter.text(
            Pos2::new(trigger_rect.left() + 24.0 * scale, trigger_rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(11.5 * scale),
            t.txt,
        );
        let arrow_c = Pos2::new(trigger_rect.right() - 10.0 * scale, trigger_rect.center().y);
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(arrow_c.x - 3.5 * scale, arrow_c.y - 1.5 * scale),
                Pos2::new(arrow_c.x + 3.5 * scale, arrow_c.y - 1.5 * scale),
                Pos2::new(arrow_c.x, arrow_c.y + 2.5 * scale),
            ],
            t.dim,
            Stroke::NONE,
        ));
    }

    if ui.is_rect_visible(next_rect) {
        let painter = ui.painter();
        painter.rect_filled(next_rect, 5.0 * scale, t.track_bg.gamma_multiply(0.6));
        let c = next_rect.center();
        let col = if next_resp.hovered() {
            nav_hover_col
        } else {
            nav_col
        };
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(c.x - 3.0 * scale, c.y - 4.0 * scale),
                Pos2::new(c.x - 3.0 * scale, c.y + 4.0 * scale),
                Pos2::new(c.x + 3.0 * scale, c.y),
            ],
            col,
            Stroke::NONE,
        ));
    }

    ui.memory_mut(|m| m.data.insert_temp(state_id, popup_open));
    if !popup_open {
        return;
    }

    let popup_w = 420.0 * scale;
    let popup_h = 460.0 * scale;
    let popup_pos = Pos2::new(
        (trigger_rect.right() - popup_w).max(4.0),
        trigger_rect.bottom() + 4.0,
    );

    let bg = if is_light {
        Color32::from_rgb(0xF6, 0xF7, 0xFA)
    } else {
        Color32::from_rgb(0x1A, 0x21, 0x36)
    };

    let search_id = trigger_id.with("search");
    let mut search_text: String = ui.memory(|m| m.data.get_temp(search_id).unwrap_or_default());

    let area_response = egui::Area::new(trigger_id.with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(popup_pos)
        .show(ui.ctx(), |ui| {
            // Fade the entire popup content with the same progress.
            ui.multiply_opacity(popup_fade);

            let frame = egui::Frame::NONE
                .fill(bg)
                .stroke(Stroke::new(1.0, t.line))
                .corner_radius(8.0 * scale)
                .shadow(egui::epaint::Shadow {
                    offset: [0, 6],
                    blur: 110,
                    spread: 0,
                    color: if is_light {
                        Color32::from_rgba_unmultiplied(0, 0, 0, 50)
                    } else {
                        Color32::from_rgba_unmultiplied(0, 0, 0, 110)
                    },
                })
                .inner_margin(egui::Margin::same(6));

            frame
                .show(ui, |ui| {
                    ui.set_min_size(Vec2::new(popup_w - 12.0 * scale, popup_h - 12.0 * scale));
                    ui.set_max_size(Vec2::new(popup_w - 12.0 * scale, popup_h - 12.0 * scale));

                    let mut action: Option<Action> = None;

                    let base_text = if is_light {
                        Color32::from_rgb(0x10, 0x12, 0x18)
                    } else {
                        t.txt
                    };
                    ui.visuals_mut().override_text_color = Some(base_text);

                    let search_bg = if is_light {
                        Color32::from_rgb(0xE0, 0xE3, 0xE8)
                    } else {
                        Color32::from_rgb(0x0D, 0x12, 0x23)
                    };
                    let search_fg = if is_light {
                        Color32::from_rgb(0x10, 0x12, 0x18)
                    } else {
                        t.txt
                    };
                    let search_hint = if is_light {
                        Color32::from_rgb(0x88, 0x8E, 0x98)
                    } else {
                        t.dim2
                    };

                    ui.add_space(2.0 * scale);
                    let search_resp = ui.add(
                        egui::TextEdit::singleline(&mut search_text)
                            .id(egui::Id::new(search_id))
                            .hint_text(egui::RichText::new("Search presets…").color(search_hint))
                            .font(egui::FontId::proportional(11.5 * scale))
                            .text_color(search_fg)
                            .background_color(search_bg)
                            .desired_width(f32::INFINITY),
                    );
                    // track edit mode so DAW shortcuts keep
                    // working outside the search box.
                    if search_resp.gained_focus() {
                        ui.memory_mut(|m| {
                            m.data
                                .insert_temp(egui::Id::new("pejvak_text_edit_active"), true);
                        });
                    }
                    if search_resp.lost_focus() {
                        // only clear the flag if the search box is the only
                        // thing that was editing. Otherwise we would clobber the flag
                        // for an unrelated text edit (e.g. a numbox in the MAIN page).
                        // Heuristic: if any widget still has focus after this frame,
                        // leave the flag alone — the top of the next `ui()` call will
                        // re-evaluate it.
                        ui.memory_mut(|m| {
                            if m.focused().is_none() {
                                m.data
                                    .insert_temp(egui::Id::new("pejvak_text_edit_active"), false);
                            }
                        });
                    }
                    ui.add_space(2.0 * scale);
                    ui.separator();

                    let search_lower = search_text.trim().to_lowercase();

                    // ---- Fixed header: Init / Save / Load ----------------
                    {
                        let cur_name = current.as_str();
                        let init_selected = cur_name == "Init";
                        if preset_entry(ui, t, is_light, "Init", cur_accent, init_selected, scale)
                            .clicked()
                        {
                            action = Some(Action::Apply(0));
                        }
                        if preset_entry(ui, t, is_light, "Save Preset…", t.dim, false, scale)
                            .clicked()
                        {
                            action = Some(Action::Save);
                        }
                        if preset_entry(ui, t, is_light, "Load Preset…", t.dim, false, scale)
                            .clicked()
                        {
                            action = Some(Action::Load);
                        }
                    }
                    ui.add_space(2.0 * scale);
                    ui.separator();
                    ui.add_space(2.0 * scale);

                    egui::ScrollArea::vertical()
                        .max_height(popup_h - 110.0 * scale)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let cur_name = current.as_str();

                            for (cat_idx, cat) in CATEGORIES.iter().enumerate() {
                                let items: Vec<(usize, &crate::presets::FactoryPreset)> =
                                    FACTORY_PRESETS
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, fp)| {
                                            fp.category == cat_idx && fp.name != "Init"
                                        })
                                        .filter(|(_, fp)| {
                                            search_lower.is_empty()
                                                || fp.name.to_lowercase().contains(&search_lower)
                                        })
                                        .collect();
                                if items.is_empty() {
                                    continue;
                                }
                                category_header(ui, t, cat.name, cat.description, cat.color, scale);
                                for (idx, fp) in items {
                                    let selected = cur_name == fp.name;
                                    if preset_entry(
                                        ui, t, is_light, fp.name, cat.color, selected, scale,
                                    )
                                    .clicked()
                                    {
                                        action = Some(Action::Apply(idx));
                                    }
                                }
                                ui.add_space(2.0 * scale);
                            }

                            if !user_presets.is_empty() {
                                category_header(
                                    ui,
                                    t,
                                    "User Presets",
                                    "Your saved presets",
                                    t.accent,
                                    scale,
                                );
                                for name in user_presets {
                                    if !search_lower.is_empty()
                                        && !name.to_lowercase().contains(&search_lower)
                                    {
                                        continue;
                                    }
                                    let selected = cur_name == name.as_str();
                                    if preset_entry(
                                        ui, t, is_light, name, t.accent, selected, scale,
                                    )
                                    .clicked()
                                    {
                                        action = Some(Action::UserPreset(name.clone()));
                                    }
                                }
                            }
                        });

                    action
                })
                .inner
        });

    ui.memory_mut(|m| m.data.insert_temp(search_id, search_text));

    // Ignore any interaction while the popup is fading out. This
    // prevents a stray click during the 140 ms fade from selecting
    // an item, applying a preset, or opening a file dialog — all of
    // which would feel like the popup had "come back to life" while
    // it was still visually disappearing.
    if let Some(act) = area_response.inner.filter(|_| !is_closing) {
        match act {
            Action::Apply(idx) => {
                let preset = &FACTORY_PRESETS[idx];
                apply_factory_preset(p, s, preset);
                *current = preset.name.to_string();
            }
            Action::Save => spawn_save_dialog(),
            Action::Load => spawn_load_dialog(),
            Action::UserPreset(name) => {
                if let Some(dir) = user_presets_dir() {
                    let path = dir.join(format!("{}.xml", name));
                    if let Some(parsed) = xml_to_params(&path) {
                        apply_preset(p, s, &parsed.cloudseed);
                        apply_texture_params(p, s, &parsed.texture, &parsed.enabled);
                        if let Some(order) = parsed.texture_order {
                            apply_texture_order(p, order);
                        }
                        if let Some(eq) = parsed.eq_enabled {
                            apply_eq_master(p, s, eq);
                        }
                        // FFDN + Chime parameters — same treatment as
                        // in `poll_preset_dialog`.
                        apply_ffdn_params(p, s, &parsed.ffdn);
                        apply_chime_params(
                            p,
                            s,
                            &parsed.illusion,
                            parsed.chime_enabled,
                            parsed.chime_seen,
                        );

                        *current = name;
                    }
                }
            }
        }
        // Fade out after an item is selected instead of snapping
        // closed.
        ui.memory_mut(|m| {
            m.data.insert_temp(closing_at_id, now + PRESET_FADE_SECS);
        });
    }

    let popup_rect = Rect::from_min_size(popup_pos, Vec2::new(popup_w, popup_h));

    if !is_closing && ui.input(|i| i.pointer.any_click()) {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            if !popup_rect.contains(pos)
                && !trigger_rect.contains(pos)
                && !prev_rect.contains(pos)
                && !next_rect.contains(pos)
            {
                // Start the closing fade instead of removing the
                // popup immediately.
                ui.memory_mut(|m| {
                    m.data.insert_temp(closing_at_id, now + PRESET_FADE_SECS);
                });
            }
        }
    }
}

fn preset_entry(
    ui: &mut Ui,
    _t: &Theme,
    is_light: bool,
    label: &str,
    accent: Color32,
    selected: bool,
    scale: f32,
) -> egui::Response {
    let h = 22.0 * scale;
    let full_w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(full_w, h), Sense::click());

    if !ui.is_rect_visible(rect) {
        return resp;
    }

    let painter = ui.painter();

    let hovered = resp.hovered();
    let bg = if selected {
        accent.gamma_multiply(if is_light { 0.22 } else { 0.20 })
    } else if hovered {
        accent.gamma_multiply(if is_light { 0.10 } else { 0.08 })
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        painter.rect_filled(rect, 4.0 * scale, bg);
    }

    if selected {
        let bar = Rect::from_min_size(
            Pos2::new(rect.left() + 1.0, rect.top() + 3.0),
            Vec2::new(2.5 * scale, rect.height() - 6.0),
        );
        painter.rect_filled(bar, 1.0, accent);
    }

    let dot_c = Pos2::new(rect.left() + 14.0 * scale, rect.center().y);
    painter.circle_filled(dot_c, 3.0 * scale, accent);

    let text_col = if selected {
        accent
    } else if is_light {
        Color32::from_rgb(0x1D, 0x21, 0x29)
    } else {
        Color32::from_rgb(0xE6, 0xEA, 0xF4)
    };
    painter.text(
        Pos2::new(rect.left() + 26.0 * scale, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(11.5 * scale),
        text_col,
    );

    if selected {
        let check_c = Pos2::new(rect.right() - 12.0 * scale, rect.center().y);
        let s = 3.5 * scale;
        painter.line_segment(
            [
                Pos2::new(check_c.x - s, check_c.y),
                Pos2::new(check_c.x - s * 0.3, check_c.y + s * 0.7),
            ],
            Stroke::new(1.8 * scale, accent),
        );
        painter.line_segment(
            [
                Pos2::new(check_c.x - s * 0.3, check_c.y + s * 0.7),
                Pos2::new(check_c.x + s, check_c.y - s * 0.6),
            ],
            Stroke::new(1.8 * scale, accent),
        );
    }

    resp
}

fn category_header(
    ui: &mut Ui,
    _t: &Theme,
    name: &str,
    description: &str,
    color: Color32,
    scale: f32,
) {
    let h = 26.0 * scale;
    let full_w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(full_w, h), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter();

    let bar = Rect::from_min_size(
        Pos2::new(rect.left() + 2.0, rect.top() + 5.0),
        Vec2::new(3.0 * scale, rect.height() - 10.0),
    );
    painter.rect_filled(bar, 1.5 * scale, color);

    painter.text(
        Pos2::new(rect.left() + 12.0 * scale, rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(11.5 * scale),
        color,
    );

    painter.text(
        Pos2::new(rect.right() - 8.0 * scale, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        description,
        egui::FontId::proportional(9.5 * scale),
        color.gamma_multiply(0.55),
    );
}

fn apply_preset(p: &Arc<PejvakRvbParams>, s: &ParamSetter, values: &[f32; 49]) {
    // Signal the audio thread to clear the DSP engine's internal
    // buffers (reverb tail, delay lines, feedback loops) so the
    // previous preset's sound is cut off immediately.
    p.reset_requested
        .store(true, std::sync::atomic::Ordering::Relaxed);

    for (i, &v) in values.iter().enumerate() {
        set_one(p, s, i, v);
    }
}

fn apply_texture_params(
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    texture: &[f32; 32],
    enabled: &[bool; 6],
) {
    macro_rules! set_b {
        ($param:expr, $val:expr) => {{
            s.begin_set_parameter(&$param);
            s.set_parameter(&$param, $val);
            s.end_set_parameter(&$param);
        }};
    }
    macro_rules! set_f {
        ($param:expr, $val:expr) => {{
            s.begin_set_parameter(&$param);
            s.set_parameter_normalized(&$param, ($val as f32).clamp(0.0, 1.0));
            s.end_set_parameter(&$param);
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

fn set_one(p: &Arc<PejvakRvbParams>, s: &ParamSetter, i: usize, v: f32) {
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

fn apply_factory_preset(
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    preset: &crate::presets::FactoryPreset,
) {
    use crate::presets::FactoryEngine;

    // 1. Set the engine.
    let target_ffdn = preset.engine == FactoryEngine::Ffdn;
    s.begin_set_parameter(&p.ffdn.use_ffdn);
    s.set_parameter(&p.ffdn.use_ffdn, target_ffdn);
    s.end_set_parameter(&p.ffdn.use_ffdn);

    // 2. Engine-specific parameters.
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
            apply_ffdn_config(p, s, &preset.ffdn);
        }
    }

    // 3. Texture Rack (unchanged for both engines).
    let tex = preset.texture;
    apply_texture_layer(s, &p.shimmer.mix, &p.shimmer.enabled, tex.shimmer_mix);
    apply_texture_layer(s, &p.mist.mix, &p.mist.enabled, tex.mist_mix);
    apply_texture_layer(s, &p.drift.mix, &p.drift.enabled, tex.drift_mix);
    apply_texture_layer(s, &p.crystal.mix, &p.crystal.enabled, tex.crystal_mix);
    apply_texture_layer(s, &p.scatter.mix, &p.scatter.enabled, tex.scatter_mix);
    apply_texture_layer(s, &p.bloom.mix, &p.bloom.enabled, tex.bloom_mix);

    // 4. Illusion (ILLUSION).
    apply_chime_config(p, s, &preset.illusion);

    // 5. Reset the signal chain order to default.
    apply_texture_order(p, [0, 1, 2, 3, 4, 5]);
}

/// Applies an FFDN configuration to the plugin parameters.
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

    // Lines is a stepped parameter: 4..16.
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

    // Card master switches — always ON in presets.
    set_b!(p.ffdn.space_enabled, true);
    set_b!(p.ffdn.network_enabled, true);
    set_b!(p.ffdn.character_enabled, true);
    set_b!(p.ffdn.shelves_enabled, true);
    set_b!(p.ffdn.modulation_enabled, true);
    set_b!(p.ffdn.filter_enabled, true);
    set_b!(p.ffdn.dynamics_enabled, true);
}

/// Applies an Illusion (ILLUSION) configuration. Presets whose `spark` is
/// zero disable the layer entirely.
fn apply_chime_config(
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    cfg: &crate::presets::ChimeConfig,
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
    mix_param: &nice_plug::prelude::FloatParam,
    enabled_param: &nice_plug::prelude::BoolParam,
    mix: f32,
) {
    let m = mix.clamp(0.0, 1.0);
    let enabled = m > 0.001;

    s.begin_set_parameter(enabled_param);
    s.set_parameter(enabled_param, enabled);
    s.end_set_parameter(enabled_param);

    s.begin_set_parameter(mix_param);
    s.set_parameter_normalized(mix_param, m);
    s.end_set_parameter(mix_param);
}

// ============================================================================
// Row primitives
// ============================================================================

fn knob_row(ui: &mut Ui, scale: f32, count: usize, add: impl FnOnce(&mut Ui)) {
    let gap = ROW_INNER_GAP * scale;
    let row_w = KNOB_SLOT_W * scale * count as f32 + gap * (count.saturating_sub(1)) as f32;
    let pad = ((ui.available_width() - row_w) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.add_space(pad);
        add(ui);
    });
}

fn knob_row_compact(ui: &mut Ui, scale: f32, count: usize, add: impl FnOnce(&mut Ui)) {
    let gap = ROW_INNER_GAP * scale;
    let row_w = KNOB_SMALL_SLOT_W * scale * count as f32 + gap * (count.saturating_sub(1)) as f32;
    let pad = ((ui.available_width() - row_w) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.add_space(pad);
        add(ui);
    });
}

fn knob_compact<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    label: &str,
    param: &P,
    setter: &ParamSetter,
    tooltip: &str,
    accent: Color32,
    scale: f32,
) {
    let slot_w = KNOB_SMALL_SLOT_W * scale;
    let slot_h = KNOB_SMALL_SLOT_H * scale;
    let d = KNOB_SMALL_D * scale;

    ui.allocate_ui_with_layout(
        Vec2::new(slot_w, slot_h),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.vertical_centered(|ui| {
                let lbl = ui.label(
                    egui::RichText::new(label.to_uppercase())
                        .size(8.5 * scale)
                        .color(t.dim2),
                );
                let _ = hover_tip(lbl, tooltip);
                ui.add_space(1.0 * scale);

                let (rect, response) =
                    ui.allocate_exact_size(Vec2::splat(d), Sense::click_and_drag());
                let response =
                    hover_tip(response, format!("{tooltip}\n\nCtrl+drag / scroll = fine"));

                handle_drag(ui, &response, param, setter);
                handle_scroll(ui, &response, param, setter);
                handle_double_click(ui, &response, param, setter);

                if ui.is_rect_visible(rect) {
                    draw_arc_knob(
                        ui,
                        rect,
                        t,
                        accent,
                        param.unmodulated_normalized_value(),
                        scale,
                    );
                }

                ui.add_space(1.0 * scale);
                let id = response.id.with("text_edit");
                editable_value(ui, t, param, setter, &id, slot_w - 2.0 * scale, scale);
            });
        },
    );
}

/// A horizontal two-segment toggle, styled like the header tabs:
/// a rounded track with a sliding accent highlight.
///
/// The left segment corresponds to `param < 0.5`, the right segment
/// to `param >= 0.5`. Use with `FloatParam`s that have a step size
/// of 1.0 to get a two-position switch.
fn horizontal_switch_2<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    left_text: &str,
    right_text: &str,
    param: &P,
    setter: &ParamSetter,
    left_tooltip: &str,
    right_tooltip: &str,
    accent: Color32,
    scale: f32,
) {
    let avail_w = ui.available_width();
    let h = 22.0 * scale;
    let gap = 4.0 * scale;
    let radius = 5.0 * scale;
    let seg_w = (avail_w - gap) * 0.5;

    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, h), Sense::hover());

    let r_left = Rect::from_min_size(rect.min, Vec2::new(seg_w, h));
    let r_right = Rect::from_min_size(
        Pos2::new(rect.left() + seg_w + gap, rect.top()),
        Vec2::new(seg_w, h),
    );

    let is_right = param.unmodulated_normalized_value() >= 0.5;
    let anim = ui.ctx().animate_bool_with_time(
        egui::Id::new(("hswitch_anim", param.name())),
        is_right,
        0.22,
    );

    let id_left = egui::Id::new(("hswitch_left", param.name()));
    let id_right = egui::Id::new(("hswitch_right", param.name()));
    let resp_left = ui.interact(r_left, id_left, Sense::click());
    let resp_right = ui.interact(r_right, id_right, Sense::click());

    if resp_left.clicked() && is_right {
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, 0.0);
        setter.end_set_parameter(param);
    }
    if resp_right.clicked() && !is_right {
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, 1.0);
        setter.end_set_parameter(param);
    }

    let painter = ui.painter();

    // Track (behind both segments).
    painter.rect_filled(r_left, radius, t.track_bg.gamma_multiply(0.45));
    painter.rect_filled(r_right, radius, t.track_bg.gamma_multiply(0.45));

    // Sliding highlight: single rect that crosses the gap, giving a
    // continuous "moving block" impression.
    let highlight_left = r_left.left() + (r_right.left() - r_left.left()) * anim;
    let highlight_rect =
        Rect::from_min_size(Pos2::new(highlight_left, r_left.top()), Vec2::new(seg_w, h));
    painter.rect_filled(highlight_rect, radius, accent.gamma_multiply(0.28));

    // Labels.
    let font = egui::FontId::proportional(9.5 * scale);
    painter.text(
        r_left.center(),
        egui::Align2::CENTER_CENTER,
        left_text,
        font.clone(),
        lerp_color(t.dim, accent, 1.0 - anim),
    );
    painter.text(
        r_right.center(),
        egui::Align2::CENTER_CENTER,
        right_text,
        font,
        lerp_color(t.dim, accent, anim),
    );

    let _ = hover_tip(resp_left, left_tooltip);
    let _ = hover_tip(resp_right, right_tooltip);

    if anim > 0.001 && anim < 0.999 {
        ui.ctx().request_repaint();
    }
}

fn fader_row(ui: &mut Ui, scale: f32, count: usize, add: impl FnOnce(&mut Ui)) {
    let gap = ROW_INNER_GAP * scale;
    let row_w = FADER_SLOT_W * scale * count as f32 + gap * (count.saturating_sub(1)) as f32;
    let pad = ((ui.available_width() - row_w) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.add_space(pad);
        add(ui);
    });
}

fn numbox_row<P: Param>(
    ui: &mut Ui,
    scale: f32,
    items: &[(&str, &P, &str)],
    setter: &ParamSetter,
    accent: Color32,
    theme: &Theme,
) {
    let gap = ROW_INNER_GAP * scale;
    let count = items.len();
    let row_w = NB_SLOT_W * scale * count as f32 + gap * (count.saturating_sub(1)) as f32;
    let pad = ((ui.available_width() - row_w) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.add_space(pad);
        for (label, param, tooltip) in items {
            numbox(ui, theme, label, *param, setter, accent, scale, tooltip);
        }
    });
}

/// draws a row of toggle switches, each positioned under
/// a specific knob column.
///
/// Uses the same column width, gap, and centering as `knob_row` /
/// `knob_row_compact`, so the toggles line up exactly beneath the
/// knobs they belong to. Empty columns are filled with a
/// spacer of the same width, keeping everything aligned.
///
/// `items` is a slice of `(column_index, label, param)` tuples.
/// The slice MUST be sorted by `column_index` ascending, and
/// `column_index` values must be < `num_columns`.
fn switch_row_at_columns<P: Param>(
    ui: &mut Ui,
    scale: f32,
    num_columns: usize,
    column_w: f32,
    items: &[(usize, &str, &P)],
    setter: &ParamSetter,
    accent: Color32,
    theme: &Theme,
) {
    let gap = ROW_INNER_GAP * scale;
    let slot_w = column_w * scale;
    let row_w = slot_w * num_columns as f32 + gap * (num_columns.saturating_sub(1)) as f32;
    let pad = ((ui.available_width() - row_w) * 0.5).max(0.0);

    // Height of each slot. Enough for the small LED + its glow.
    let slot_h = 22.0 * scale;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.add_space(pad);

        let mut iter = items.iter().peekable();
        for col in 0..num_columns {
            let is_at_this_col = iter.peek().map(|(c, _, _)| *c == col).unwrap_or(false);
            let item = if is_at_this_col { iter.next() } else { None };

            // allocate the same-sized slot for BOTH empty
            // and filled columns so item_spacing behaves identically
            // and the columns line up with the knob row above.
            let (slot_rect, _) = ui.allocate_exact_size(Vec2::new(slot_w, slot_h), Sense::hover());

            if let Some((_, label, param)) = item {
                // Centre the LED (which is now the whole widget) at
                // the slot's horizontal centre.
                let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(slot_rect));
                child_ui.vertical_centered(|ui| {
                    switch_tiny(ui, theme, label, *param, setter, accent, scale);
                });
            }
        }
    });
}

fn route_diagram(ui: &mut Ui, t: &Theme, pre_mode: bool, scale: f32) {
    let full_w = ui.available_width();
    let h = 30.0 * scale;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(full_w, h), Sense::hover());

    if !ui.is_rect_visible(rect) {
        return;
    }

    let phase =
        ui.ctx()
            .animate_bool_with_time(egui::Id::new("route_diagram_phase"), !pre_mode, 0.35);

    let painter = ui.painter();

    let eq_slot = 1.0 + phase * 2.0;
    let diff_slot = 2.0 - phase;
    let dly_slot = 3.0 - phase;

    let box_h = 18.0 * scale;
    let arrow_gap = 6.0 * scale;
    let inner_pad = 4.0 * scale;
    let arrow_head = 3.5 * scale;

    let slot_count = 5usize;
    let num_gaps = slot_count - 1;
    let available_for_boxes = full_w - arrow_gap * num_gaps as f32 - inner_pad * 2.0;
    let box_w = (available_for_boxes / slot_count as f32).max(28.0 * scale);
    let slot_w = box_w + arrow_gap;

    let cy = rect.center().y;
    let left = rect.left() + inner_pad;

    let font = egui::FontId::proportional(9.5 * scale);

    let slot_x = |slot: f32| left + slot * slot_w;

    for i in 0..num_gaps {
        let x_start = slot_x(i as f32) + box_w;
        let x_end = slot_x((i + 1) as f32);
        let arrow_color = t.dim2;
        let a_start = Pos2::new(x_start + 1.0, cy);
        let a_end = Pos2::new(x_end - 1.0, cy);
        painter.line_segment([a_start, a_end], Stroke::new(1.0, arrow_color));
        painter.line_segment(
            [
                Pos2::new(a_end.x - arrow_head, cy - arrow_head * 0.7),
                a_end,
            ],
            Stroke::new(1.0, arrow_color),
        );
        painter.line_segment(
            [
                Pos2::new(a_end.x - arrow_head, cy + arrow_head * 0.7),
                a_end,
            ],
            Stroke::new(1.0, arrow_color),
        );
    }

    let boxes: [(&str, f32); 5] = [
        ("IN", 0.0),
        ("EQ", eq_slot),
        ("DIFF", diff_slot),
        ("DLY", dly_slot),
        ("OUT", 4.0),
    ];

    for (label, slot) in boxes {
        let x = slot_x(slot);
        let box_rect = Rect::from_min_size(Pos2::new(x, cy - box_h * 0.5), Vec2::new(box_w, box_h));

        if label == "EQ" {
            let bg = t.accent.gamma_multiply(0.22);
            painter.rect_filled(box_rect, 3.0 * scale, bg);
            painter.text(
                box_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                font.clone(),
                t.accent,
            );
        } else {
            let bg = t.track_bg.gamma_multiply(0.70);
            painter.rect_filled(box_rect, 3.0 * scale, bg);
            painter.rect_stroke(
                box_rect,
                3.0 * scale,
                Stroke::new(1.0, t.line),
                egui::StrokeKind::Inside,
            );
            painter.text(
                box_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                font.clone(),
                t.dim,
            );
        }
    }

    if phase > 0.001 && phase < 0.999 {
        ui.ctx().request_repaint();
    }
}

fn switch_tiny<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    label: &str,
    param: &P,
    setter: &ParamSetter,
    accent: Color32,
    scale: f32,
) {
    let led_r = 5.0 * scale;
    let gap = 6.0 * scale;
    let label_w = 24.0 * scale;

    // the widget's allocated box is ONLY as wide as the
    // LED, so when the caller centres the widget under a knob the
    // LED ends up at the exact horizontal centre of that knob. The
    // label is painted outside the box, to the right of it, and
    // does not affect layout.
    let box_w = led_r * 2.0;
    let total_h = led_r * 2.0 + 3.0 * scale;

    let (rect, _) = ui.allocate_exact_size(Vec2::new(box_w, total_h), Sense::hover());
    let led_center = rect.center();
    let label_rect = Rect::from_min_size(
        Pos2::new(rect.right() + gap, rect.top()),
        Vec2::new(label_w, total_h),
    );

    let resp_id = egui::Id::new(("tiny_switch", param.name()));
    let led_rect = Rect::from_center_size(led_center, Vec2::splat(led_r * 2.4));
    let resp = ui.interact(led_rect, resp_id, Sense::click());
    let on = param.unmodulated_normalized_value() > 0.5;
    if resp.clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, if on { 0.0 } else { 1.0 });
        setter.end_set_parameter(param);
    }

    let tt = ui.ctx().animate_bool_with_time(
        egui::Id::new(("tiny_switch_fade", param.name())),
        on,
        0.28,
    );

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        // ---- Glow (concentric translucent circles) --------------
        if tt > 0.01 {
            for i in 0..3 {
                let r = led_r * (1.20 + i as f32 * 0.42);
                let alpha = ((1.0 - i as f32 / 3.0) * 60.0 * tt) as u8;
                painter.circle_filled(
                    led_center,
                    r,
                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha),
                );
            }
        }

        // ---- Outer ring (theme-interpolated) --------------------
        painter.circle_filled(led_center, led_r, t.led_ring);

        // ---- Inner bulb: lerp from "off" colour to accent -------
        let bulb_col = lerp_color(t.led_off, accent, tt);
        painter.circle_filled(led_center, led_r * 0.72, bulb_col);

        // ---- Specular highlight when on -------------------------
        if tt > 0.12 {
            let hi_alpha = (215.0 * tt) as u8;
            painter.circle_filled(
                Pos2::new(led_center.x - led_r * 0.20, led_center.y - led_r * 0.20),
                led_r * 0.28,
                Color32::from_rgba_unmultiplied(255, 255, 255, hi_alpha),
            );
        }

        // ---- LED outline ----------------------------------------
        painter.circle_stroke(
            led_center,
            led_r,
            Stroke::new(1.0, lerp_color(t.line, accent.gamma_multiply(0.75), tt)),
        );

        // ---- Label to the right of the LED ----------------------
        if !label.is_empty() {
            painter.text(
                label_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(10.0 * scale),
                if on { t.txt } else { t.dim2 },
            );
        }
    }
}

// ============================================================================
// Widgets
// ============================================================================

fn knob<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    label: &str,
    param: &P,
    setter: &ParamSetter,
    tooltip: &str,
    accent: Color32,
    scale: f32,
) {
    let slot_w = KNOB_SLOT_W * scale;
    let slot_h = KNOB_SLOT_H * scale;
    let d = KNOB_D * scale;

    ui.allocate_ui_with_layout(
        Vec2::new(slot_w, slot_h),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.vertical_centered(|ui| {
                let lbl = ui.label(
                    egui::RichText::new(label.to_uppercase())
                        .size(9.5 * scale)
                        .color(t.dim2),
                );
                let _ = hover_tip(lbl, tooltip);
                ui.add_space(2.0 * scale);

                let (rect, response) =
                    ui.allocate_exact_size(Vec2::splat(d), Sense::click_and_drag());
                let response =
                    hover_tip(response, format!("{tooltip}\n\nCtrl+drag / scroll = fine"));

                handle_drag(ui, &response, param, setter);
                handle_scroll(ui, &response, param, setter);
                handle_double_click(ui, &response, param, setter);

                if ui.is_rect_visible(rect) {
                    draw_arc_knob(
                        ui,
                        rect,
                        t,
                        accent,
                        param.unmodulated_normalized_value(),
                        scale,
                    );
                }

                ui.add_space(1.0 * scale);
                let id = response.id.with("text_edit");
                editable_value(ui, t, param, setter, &id, slot_w - 4.0 * scale, scale);
            });
        },
    );
}

fn fader<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    label: &str,
    param: &P,
    setter: &ParamSetter,
    tooltip: &str,
    accent: Color32,
    scale: f32,
) {
    let slot_w = FADER_SLOT_W * scale;
    let slot_h = FADER_SLOT_H * scale;
    let fh = FADER_H * scale;

    ui.allocate_ui_with_layout(
        Vec2::new(slot_w, slot_h),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.vertical_centered(|ui| {
                let lbl = ui.label(
                    egui::RichText::new(label.to_uppercase())
                        .size(9.5 * scale)
                        .color(t.dim2),
                );
                let _ = hover_tip(lbl, tooltip);
                ui.add_space(2.0 * scale);

                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(slot_w, fh), Sense::click_and_drag());
                let response =
                    hover_tip(response, format!("{tooltip}\n\nCtrl+drag / scroll = fine"));

                handle_drag(ui, &response, param, setter);
                handle_scroll(ui, &response, param, setter);
                handle_double_click(ui, &response, param, setter);

                if ui.is_rect_visible(rect) {
                    draw_fader(
                        ui,
                        rect,
                        t,
                        accent,
                        param.unmodulated_normalized_value(),
                        scale,
                    );
                }

                ui.add_space(1.0 * scale);
                let id = response.id.with("text_edit");
                editable_value(ui, t, param, setter, &id, slot_w - 2.0 * scale, scale);
            });
        },
    );
}

fn numbox<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    label: &str,
    param: &P,
    setter: &ParamSetter,
    _accent: Color32,
    scale: f32,
    tooltip: &str,
) {
    let slot_w = NB_SLOT_W * scale;
    let slot_h = NB_SLOT_H * scale;

    ui.allocate_ui_with_layout(
        Vec2::new(slot_w, slot_h),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.vertical_centered(|ui| {
                let lbl = ui.label(
                    egui::RichText::new(label.to_uppercase())
                        .size(9.0 * scale)
                        .color(t.dim2),
                );
                let _ = hover_tip(lbl, tooltip);

                let id = egui::Id::new(("numbox", param.name()));
                let val_str = param.to_string();

                // single-slot editing, same mechanism as
                // `editable_value`. Only ONE field in the entire
                // plugin can be in edit mode at a time.
                let slot_id = egui::Id::new("pejvak_current_editing");
                let current: Option<egui::Id> = ui.memory(|m| m.data.get_temp(slot_id));
                let editing = current == Some(id);

                if !editing {
                    let (rect, resp) = ui.allocate_exact_size(
                        Vec2::new(slot_w, 16.0 * scale),
                        Sense::click_and_drag(),
                    );

                    let resp = hover_tip(resp, tooltip);

                    let stepped = match param.name() {
                        "TapCount" => Some(1.0_f32 / 49.0),
                        "LineCount" => Some(1.0_f32 / 11.0),
                        "DiffusionStages" | "LateDiffusionStages" => Some(1.0_f32 / 7.0),
                        _ => None,
                    };

                    // Applies exactly one discrete step to the
                    // parameter. Both the scroll handler and the drag
                    // handler call this repeatedly until their
                    // accumulators are drained.
                    let apply_step = |direction: i32| {
                        if let Some(step) = stepped {
                            let cur = param.unmodulated_normalized_value();
                            let grid = (cur / step).round();
                            let next_grid = grid + direction as f32;
                            let max_grid = (1.0_f32 / step).round();
                            let clamped = next_grid.clamp(0.0, max_grid);
                            let new_val = (clamped * step).clamp(0.0, 1.0);

                            setter.begin_set_parameter(param);
                            setter.set_parameter_normalized(param, new_val);
                            setter.end_set_parameter(param);
                        } else {
                            // Continuous parameter (Seed). The 0.005
                            // step gives roughly 200 wheel clicks for
                            // a full sweep of the 0..1 range, which
                            // matches the perceived "speed" of the
                            // seeded parameter's 0..1_000_000 physical
                            // range.
                            let cur = param.unmodulated_normalized_value();
                            let new_val = (cur + direction as f32 * 0.005).clamp(0.0, 1.0);
                            setter.begin_set_parameter(param);
                            setter.set_parameter_normalized(param, new_val);
                            setter.end_set_parameter(param);
                        }
                    };

                    // ---- Scroll: accumulate smooth scroll, one unit = one step ----
                    if resp.hovered() {
                        let mut any_up = false;
                        let mut any_down = false;
                        ui.input(|i| {
                            for ev in &i.events {
                                if let egui::Event::MouseWheel { delta, .. } = ev {
                                    if delta.y > 0.0 {
                                        any_up = true;
                                    } else if delta.y < 0.0 {
                                        any_down = true;
                                    }
                                }
                            }
                        });
                        if any_up && !any_down {
                            apply_step(1);
                        } else if any_down && !any_up {
                            apply_step(-1);
                        }
                    }

                    // ---- Drag: 12 px of vertical movement per step ----
                    //
                    // Same idea as the scroll accumulator, but driven
                    // by the pointer's per-frame movement. Dragging
                    // *up* increases the value (the natural gesture
                    // for a vertical slider), so we subtract the y
                    // delta.
                    {
                        let drag_accum_id = resp.id.with("drag_accum");
                        if resp.dragged() {
                            let mut accum: f32 =
                                ui.memory(|m| m.data.get_temp(drag_accum_id).unwrap_or(0.0));
                            // Upward drag = positive direction.
                            accum -= resp.drag_delta().y;

                            const DRAG_THRESHOLD: f32 = 12.0;
                            while accum.abs() >= DRAG_THRESHOLD {
                                let direction = if accum > 0.0 { 1 } else { -1 };
                                apply_step(direction);
                                accum -= direction as f32 * DRAG_THRESHOLD;
                            }
                            ui.memory_mut(|m| m.data.insert_temp(drag_accum_id, accum));
                        } else {
                            // Reset the drag accumulator whenever we
                            // are not actively dragging. Without this
                            // a partial step (e.g. 7 px) would carry
                            // over to the next drag gesture and the
                            // first step would fire after only 5 px,
                            // which feels jumpy.
                            ui.memory_mut(|m| m.data.remove::<f32>(drag_accum_id));
                        }
                    }

                    // Cursor feedback while dragging.
                    if resp.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                    }

                    let hovered = resp.hovered();
                    if resp.clicked() {
                        ui.memory_mut(|m| {
                            // Claim the shared slot. Any other field
                            // in edit mode automatically reverts on
                            // the next frame.
                            m.data.insert_temp(slot_id, id);
                            m.data.insert_temp(id.with("str"), val_str.clone());
                            m.data.insert_temp(id.with("focus"), true);
                            m.data.insert_temp(id.with("select_all"), true);
                        });
                    }

                    if ui.is_rect_visible(rect) {
                        // use theme-interpolated colours so
                        // the background fades smoothly with the rest
                        // of the theme instead of jumping at the
                        // is_light boundary.
                        let bg = if resp.dragged() {
                            t.numbox_bg.gamma_multiply(0.75)
                        } else if hovered {
                            t.numbox_bg.gamma_multiply(1.15)
                        } else {
                            t.numbox_bg
                        };

                        ui.painter().rect_filled(rect, 3.0 * scale, bg);
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            &val_str,
                            egui::FontId::proportional(11.0 * scale),
                            t.txt,
                        );
                    }
                } else {
                    let mut text: String = ui
                        .memory(|m| m.data.get_temp(id.with("str")))
                        .unwrap_or_else(|| val_str.clone());

                    let output = egui::TextEdit::singleline(&mut text)
                        .id(id)
                        .font(egui::FontId::proportional(11.0 * scale))
                        .desired_width(slot_w)
                        .horizontal_align(egui::Align::Center)
                        .text_color(t.txt)
                        .background_color(t.numbox_bg)
                        .show(ui);
                    let resp = output.response;

                    let need_focus: bool =
                        ui.memory(|m| m.data.get_temp(id.with("focus")).unwrap_or(false));
                    if need_focus {
                        resp.request_focus();
                        ui.memory_mut(|m| {
                            m.data.insert_temp(id.with("focus"), false);
                        });
                    }

                    // Select-all on first focus so the existing
                    // number is highlighted as soon as the user
                    // clicks into the field.
                    let select_all: bool =
                        ui.memory(|m| m.data.get_temp(id.with("select_all")).unwrap_or(false));
                    if select_all && resp.has_focus() {
                        let ctx = ui.ctx().clone();
                        if let Some(mut state) = egui::TextEdit::load_state(&ctx, id) {
                            let end = text.chars().count();
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(end),
                                )));
                            state.store(&ctx, id);
                        }
                        ui.memory_mut(|m| {
                            m.data.insert_temp(id.with("select_all"), false);
                        });
                    }

                    let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let escape_pressed = ui.input(|i| i.key_pressed(egui::Key::Escape));

                    let should_exit =
                        escape_pressed || enter_pressed || (resp.lost_focus() && !need_focus);

                    if should_exit {
                        if !escape_pressed {
                            if let Some(nv) = param.string_to_normalized_value(&text) {
                                setter.begin_set_parameter(param);
                                setter.set_parameter_normalized(param, nv.clamp(0.0, 1.0));
                                setter.end_set_parameter(param);
                            }
                        }
                        ui.memory_mut(|m| {
                            m.data.remove::<egui::Id>(slot_id);
                            m.data.remove::<String>(id.with("str"));
                            m.data.remove::<bool>(id.with("focus"));
                            m.data.remove::<bool>(id.with("select_all"));
                        });
                    } else {
                        ui.memory_mut(|m| {
                            m.data.insert_temp(id.with("str"), text);
                        });
                    }
                }
            });
        },
    );
}

// ============================================================================
// Widget interaction helpers
// ============================================================================

fn handle_scroll<P: Param>(
    ui: &mut Ui,
    response: &egui::Response,
    param: &P,
    setter: &ParamSetter,
) {
    if !response.hovered() {
        return;
    }

    // ---- Stepped parameter: FfdnLines (grid 4..16) ----
    // Uses raw `Event::MouseWheel` so one physical notch = one step.
    // `smooth_scroll_delta` is unusable here: on a standard wheel it
    // accumulates ~50 units per notch, and on smooth trackpads it
    // spreads a single notch across several frames.
    if param.name() == "FfdnLines" {
        let mut any_up = false;
        let mut any_down = false;
        ui.input(|i| {
            for ev in &i.events {
                if let egui::Event::MouseWheel { delta, .. } = ev {
                    if delta.y > 0.0 {
                        any_up = true;
                    } else if delta.y < 0.0 {
                        any_down = true;
                    }
                }
            }
        });
        if any_up && !any_down {
            apply_lines_step(param, setter, 1);
        } else if any_down && !any_up {
            apply_lines_step(param, setter, -1);
        }
        return;
    }

    // ---- Continuous parameter (default) ----
    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
    if scroll.abs() > 0.5 {
        let fine = ui.input(|i| i.modifiers.ctrl);
        let cur = param.unmodulated_normalized_value();
        let step = (scroll * if fine { 0.00009 } else { 0.0006 }).clamp(-0.1, 0.1);
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, (cur + step).clamp(0.0, 1.0));
        setter.end_set_parameter(param);
    }
}

/// Applies one grid step to `FfdnLines`. The grid is `1/12`, giving
/// 13 positions (Lines = 4..=16).
fn apply_lines_step<P: Param>(param: &P, setter: &ParamSetter, direction: i32) {
    const STEP: f32 = 1.0 / 12.0;
    let cur = param.unmodulated_normalized_value();
    let grid = (cur / STEP).round();
    let max_grid = (1.0_f32 / STEP).round();
    let next = (grid + direction as f32).clamp(0.0, max_grid);
    let new_val = (next * STEP).clamp(0.0, 1.0);

    setter.begin_set_parameter(param);
    setter.set_parameter_normalized(param, new_val);
    setter.end_set_parameter(param);
}

fn handle_drag<P: Param>(ui: &mut Ui, response: &egui::Response, param: &P, setter: &ParamSetter) {
    if response.drag_started() {
        setter.begin_set_parameter(param);
        ui.memory_mut(|m| {
            m.data
                .insert_temp(response.id, param.unmodulated_normalized_value())
        });
    }
    if response.dragged() {
        let fine = ui.input(|i| i.modifiers.ctrl);
        let delta = -response.drag_delta().y * if fine { 0.0009 } else { 0.006 };
        let stored: f32 = ui
            .memory(|m| m.data.get_temp(response.id))
            .unwrap_or_else(|| param.unmodulated_normalized_value());
        let new_val = (stored + delta).clamp(0.0, 1.0);
        ui.memory_mut(|m| m.data.insert_temp(response.id, new_val));
        setter.set_parameter_normalized(param, new_val);
    }
    if response.drag_stopped() {
        setter.end_set_parameter(param);
        ui.memory_mut(|m| m.data.remove::<f32>(response.id));
    }
}

fn handle_double_click<P: Param>(
    _ui: &mut Ui,
    response: &egui::Response,
    param: &P,
    setter: &ParamSetter,
) {
    if response.double_clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, param.default_normalized_value());
        setter.end_set_parameter(param);
    }
}

// ============================================================================
// Draw helpers
// ============================================================================

fn draw_arc_knob(ui: &mut Ui, rect: Rect, t: &Theme, accent: Color32, norm: f32, scale: f32) {
    let painter = ui.painter();
    let center = rect.center();
    let r = rect.width() * 0.5;
    let norm = norm.clamp(0.0, 1.0);
    let start = 0.75 * PI;
    let sweep = 1.5 * PI;
    let steps = 48;

    // ─── Roundness of each arc endpoint ────────────────────────────
    // Fraction of the stroke width used as the cap-circle radius.
    //
    //   0.00 → flat end (no circle drawn)
    //   0.25 → subtly rounded
    //   0.50 → fully rounded (semicircular cap)
    //
    // Tweak each value independently to give every arc end its own
    // character. The values below are a balanced starting point: the
    // track ends slightly softer than the active arc, and both arc
    // starts (which sit right on top of each other) match so the
    // overlap is invisible.
    const BG_START_ROUNDNESS: f32 = 0.35;
    const BG_END_ROUNDNESS: f32 = 0.35;
    const ACT_START_ROUNDNESS: f32 = 0.35;
    const ACT_END_ROUNDNESS: f32 = 0.35;

    // ─── Background track (the dim, unfilled arc) ──────────────────
    let bg_stroke = 1.5 * scale;
    let bg_cap_start_r = bg_stroke * BG_START_ROUNDNESS;
    let bg_cap_end_r = bg_stroke * BG_END_ROUNDNESS;

    let mut bg = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let a = start + (i as f32 / steps as f32) * sweep;
        bg.push(center + Vec2::new(a.cos(), a.sin()) * r);
    }
    painter.add(egui::Shape::line(
        bg.clone(),
        Stroke::new(bg_stroke, t.track_bg),
    ));
    if bg_cap_start_r > 0.0 {
        painter.circle_filled(bg[0], bg_cap_start_r, t.track_bg);
    }
    if bg_cap_end_r > 0.0 {
        painter.circle_filled(bg[bg.len() - 1], bg_cap_end_r, t.track_bg);
    }

    // ─── Active arc (the bright, filled portion) ───────────────────
    if norm > 0.005 {
        let active = ((steps as f32 * norm) as usize).max(2);
        let mut act = Vec::with_capacity(active + 1);
        for i in 0..=active {
            let frac = (i as f32 / active as f32) * norm;
            let a = start + frac * sweep;
            act.push(center + Vec2::new(a.cos(), a.sin()) * r);
        }
        let act_stroke = 2.0 * scale;
        let act_cap_start_r = act_stroke * ACT_START_ROUNDNESS;
        let act_cap_end_r = act_stroke * ACT_END_ROUNDNESS;

        painter.add(egui::Shape::line(
            act.clone(),
            Stroke::new(act_stroke, accent),
        ));
        if act_cap_start_r > 0.0 {
            painter.circle_filled(act[0], act_cap_start_r, accent);
        }
        if act_cap_end_r > 0.0 {
            painter.circle_filled(act[act.len() - 1], act_cap_end_r, accent);
        }
    }

    // ─── Knob body ─────────────────────────────────────────────────
    let body_r = r * 0.62;
    painter.circle_filled(center, body_r, t.knob_bg);
    painter.circle_stroke(center, body_r, Stroke::new(1.0, t.knob_border));

    // ─── Indicator line ────────────────────────────────────────────
    let a = start + norm * sweep;
    let inner = center + Vec2::new(a.cos(), a.sin()) * (body_r * 0.35);
    let outer = center + Vec2::new(a.cos(), a.sin()) * (body_r * 0.88);
    let ind_stroke = 2.0 * scale;
    let ind_cap_r = ind_stroke * 0.35;
    painter.line_segment([inner, outer], Stroke::new(ind_stroke, accent));
    if ind_cap_r > 0.0 {
        painter.circle_filled(inner, ind_cap_r, accent);
        painter.circle_filled(outer, ind_cap_r, accent);
    }
}

/// Draws a vertical fader.
///
/// The track is drawn as a *filled rectangle* — not as a line
/// segment — so that the thickness is exact and never affected by
/// anti-aliasing of diagonal or curved strokes.
///
/// # Track / fill geometry
///
fn draw_fader(ui: &mut Ui, rect: Rect, t: &Theme, accent: Color32, norm: f32, scale: f32) {
    let painter = ui.painter();

    let cx_raw = rect.center().x;
    let top_raw = rect.top() + 4.0 * scale;
    let bot_raw = rect.bottom() - 4.0 * scale;

    // Pixel snapping for guaranteed uniform thickness.
    //
    // `round_to_pixel_center` snaps a coordinate to the exact
    // centre of a physical pixel. This is important because if a
    // rectangle's edges fall on pixel *boundaries* (integers),
    // anti-aliasing spreads the fill across two half-lit pixels —
    // which reads to the eye as "half-thickness". Snapping to the
    // centre of a pixel instead guarantees that the fill lands
    // fully inside a fixed number of physical pixels, so the
    // rendered thickness is exactly the geometric one.
    //
    // Vertical positions are snapped for the same reason: the top
    // and bottom of the fader are horizontal edges and benefit
    // identically.
    let cx = painter.round_to_pixel_center(cx_raw);
    let top = painter.round_to_pixel_center(top_raw);
    let bot = painter.round_to_pixel_center(bot_raw);

    // Restore the  technique with tighter tolerances.
    // The track is slightly wider than the fill to counter the
    // simultaneous-contrast illusion where the bright fill appears
    // wider than the dim track.
    let track_half_w = 1.40625 * scale;
    let fill_half_w = 1.0 * scale;

    // Draw both track and fill with the SAME snapped `cx` so they
    // are perfectly concentric.
    let track_rect = Rect::from_min_max(
        Pos2::new(cx - track_half_w, top),
        Pos2::new(cx + track_half_w, bot),
    );
    painter.rect_filled(track_rect, 0.0, t.track_bg);

    let norm = norm.clamp(0.0, 1.0);
    let full_h = bot - top;
    let fill_h = full_h * norm;

    if fill_h > 0.5 {
        let fill_rect = Rect::from_min_max(
            Pos2::new(cx - fill_half_w, bot - fill_h),
            Pos2::new(cx + fill_half_w, bot),
        );
        painter.rect_filled(fill_rect, 0.0, accent);
    }

    // Thumb (circle).
    let thumb_y = bot - fill_h;
    let thumb_r = 6.0 * scale;
    painter.circle_filled(Pos2::new(cx, thumb_y), thumb_r, t.thumb_bg);
    painter.circle_stroke(
        Pos2::new(cx, thumb_y),
        thumb_r,
        Stroke::new(1.5 * scale, accent),
    );
}

// ============================================================================
// Editable value
// ============================================================================

fn editable_value<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    param: &P,
    setter: &ParamSetter,
    base_id: &egui::Id,
    width: f32,
    scale: f32,
) {
    let val_str = param.to_string();
    let edit_id = base_id.with("text_edit");

    // single-slot editing.
    //
    // Instead of a per-field boolean flag (which could get stuck when
    // the user clicked from one field to another without an explicit
    // Enter), we use ONE shared slot in egui memory that holds the
    // ID of whichever field is currently being edited. Setting the
    // slot to a new field automatically reverts the previous one.
    //
    // This makes the "some fields don't get selected" and "background
    // stays weird after clicking away" bugs impossible by construction:
    // at most one field can be in edit mode, and the state always
    // reflects the current user intent.
    let slot_id = egui::Id::new("pejvak_current_editing");
    let current: Option<egui::Id> = ui.memory(|m| m.data.get_temp(slot_id));
    let is_editing = current == Some(edit_id);

    if !is_editing {
        // ---- Display mode ------------------------------------------
        let (vrect, vresp) = ui.allocate_exact_size(Vec2::new(width, 14.0 * scale), Sense::click());

        let hovered = vresp.hovered();
        if ui.is_rect_visible(vrect) {
            // Subtle hover highlight so the user can clearly see
            // which fields are clickable. Without this, a field that
            // is technically clickable but visually indistinct is
            // easy to miss.
            if hovered {
                ui.painter()
                    .rect_filled(vrect, 2.0 * scale, t.track_bg.gamma_multiply(0.55));
            }
            ui.painter().text(
                vrect.center(),
                egui::Align2::CENTER_CENTER,
                &val_str,
                egui::FontId::proportional(10.5 * scale),
                t.txt,
            );
        }

        if hover_tip(vresp, "Click to type a value.").clicked() {
            ui.memory_mut(|m| {
                // Claim the slot. Any other field that was in edit
                // mode automatically reverts on the next frame,
                // because `current == Some(edit_id_other)` will no
                // longer be true.
                m.data.insert_temp(slot_id, edit_id);
                m.data.insert_temp(edit_id.with("str"), val_str.clone());
                m.data.insert_temp(edit_id.with("focus"), true);
                m.data.insert_temp(edit_id.with("select_all"), true);
            });
        }
    } else {
        // ---- Edit mode ---------------------------------------------
        let mut text: String = ui
            .memory(|m| m.data.get_temp(edit_id.with("str")))
            .unwrap_or_else(|| val_str.clone());

        // The `.id(edit_id)` call is essential: it tells egui to
        // store the TextEdit's internal state (cursor position,
        // selection range, scroll) under our known ID, so that
        // `TextEdit::load_state()` below can find it.
        let output = egui::TextEdit::singleline(&mut text)
            .id(edit_id)
            .font(egui::FontId::proportional(10.5 * scale))
            .desired_width(width)
            .horizontal_align(egui::Align::Center)
            .show(ui);
        let resp = output.response;

        // One-shot focus request on the frame the editor opens.
        let need_focus: bool =
            ui.memory(|m| m.data.get_temp(edit_id.with("focus")).unwrap_or(false));
        if need_focus {
            resp.request_focus();
            ui.memory_mut(|m| m.data.insert_temp(edit_id.with("focus"), false));
        }

        // Select all text once, on the first frame the TextEdit has
        // focus. This is what makes the numbers appear "highlighted"
        // as soon as the user clicks into the field.
        let select_all: bool =
            ui.memory(|m| m.data.get_temp(edit_id.with("select_all")).unwrap_or(false));
        if select_all && resp.has_focus() {
            let ctx = ui.ctx().clone();
            if let Some(mut state) = egui::TextEdit::load_state(&ctx, edit_id) {
                let end = text.chars().count();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(end),
                    )));
                state.store(&ctx, edit_id);
            }
            ui.memory_mut(|m| {
                m.data.insert_temp(edit_id.with("select_all"), false);
            });
        }

        // Exit conditions:
        //   * Enter — commit the value and close.
        //   * Escape — discard the pending text and close.
        //   * Focus loss — commit and close (e.g. user clicked
        //     somewhere else).
        //
        // The `!need_focus` guard prevents a spurious exit on the
        // very first frame of the editor, when focus has not yet
        // been granted.
        let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let escape_pressed = ui.input(|i| i.key_pressed(egui::Key::Escape));

        let should_exit = escape_pressed || enter_pressed || (resp.lost_focus() && !need_focus);

        if should_exit {
            if !escape_pressed {
                if let Some(nv) = param.string_to_normalized_value(&text) {
                    setter.begin_set_parameter(param);
                    setter.set_parameter_normalized(param, nv.clamp(0.0, 1.0));
                    setter.end_set_parameter(param);
                }
            }
            // Release the slot and clean up all transient state.
            ui.memory_mut(|m| {
                m.data.remove::<egui::Id>(slot_id);
                m.data.remove::<String>(edit_id.with("str"));
                m.data.remove::<bool>(edit_id.with("focus"));
                m.data.remove::<bool>(edit_id.with("select_all"));
            });
        } else {
            ui.memory_mut(|m| {
                m.data.insert_temp(edit_id.with("str"), text);
            });
        }
    }
}

// ============================================================================
// Card
// ============================================================================

/// Draw a card body.
///
/// `id_key` MUST be unique per card. It is used to key the enabled/
/// disabled fade animation so that two cards can never share state.
fn draw_card<F>(ui: &mut Ui, t: &Theme, scale: f32, enabled: bool, id_key: &str, add: F)
where
    F: FnOnce(&mut Ui, &Theme),
{
    let anim =
        ui.ctx()
            .animate_bool_with_time(egui::Id::new(("card_enable_fade", id_key)), enabled, 0.25);
    let ct_owned = t.greyed().lerp(t, anim);
    let ct = &ct_owned;

    let pad = CARD_PAD * scale;
    let radius = 10.0 * scale;
    let bg_idx = ui.painter().add(egui::Shape::Noop);

    let frame = egui::Frame::NONE
        .corner_radius(egui::CornerRadius::same(radius as u8))
        .inner_margin(egui::Margin {
            left: pad.round() as i8,
            right: pad.round() as i8,
            top: pad.round() as i8,
            bottom: pad.round() as i8,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing = Vec2::new(ROW_INNER_GAP * scale, ROW_OUTER_GAP * scale);
            add(ui, ct);
        });

    let rect = frame.response.rect;
    if ui.is_rect_visible(rect) {
        ui.painter().set(
            bg_idx,
            rounded_gradient(rect, radius, t.card_top, t.card_bottom),
        );
        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, t.line),
            egui::StrokeKind::Inside,
        );
    }
}

/// Same as `draw_card`, but enforces a minimum content height (in
/// logical pixels, multiplied by `scale`). Used by cards whose
/// content is smaller than the available slot — such as the ROUTE,
/// ENGINE, MIXER, and SIGNAL CHAIN cards — so that the bottom edge
/// aligns with the neighbouring cards regardless of content size.
fn draw_card_h<F>(
    ui: &mut Ui,
    t: &Theme,
    scale: f32,
    enabled: bool,
    id_key: &str,
    min_height: f32,
    add: F,
) where
    F: FnOnce(&mut Ui, &Theme),
{
    let anim =
        ui.ctx()
            .animate_bool_with_time(egui::Id::new(("card_enable_fade", id_key)), enabled, 0.25);
    let ct_owned = t.greyed().lerp(t, anim);
    let ct = &ct_owned;

    let pad = CARD_PAD * scale;
    let radius = 10.0 * scale;
    let bg_idx = ui.painter().add(egui::Shape::Noop);

    let target_inner_h = min_height * scale;

    let frame = egui::Frame::NONE
        .corner_radius(egui::CornerRadius::same(radius as u8))
        .inner_margin(egui::Margin {
            left: pad.round() as i8,
            right: pad.round() as i8,
            top: pad.round() as i8,
            bottom: pad.round() as i8,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing = Vec2::new(ROW_INNER_GAP * scale, ROW_OUTER_GAP * scale);

            let start_y = ui.cursor().top();
            add(ui, ct);
            let used_h = ui.cursor().top() - start_y;

            if used_h < target_inner_h {
                ui.allocate_space(Vec2::new(0.0, target_inner_h - used_h));
            }
        });

    let rect = frame.response.rect;
    if ui.is_rect_visible(rect) {
        ui.painter().set(
            bg_idx,
            rounded_gradient(rect, radius, t.card_top, t.card_bottom),
        );
        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, t.line),
            egui::StrokeKind::Inside,
        );
    }
}

fn section_title(ui: &mut Ui, t: &Theme, text: &str, accent: Color32, scale: f32) {
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), CARD_TITLE_H * scale),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(
                egui::RichText::new(text)
                    .size(11.0 * scale)
                    .strong()
                    .color(accent),
            );
            let y = ui.cursor().top() + CARD_TITLE_H * 0.5 * scale;
            let left = ui.cursor().left() + 4.0 * scale;
            let right = ui.max_rect().right() - 2.0 * scale;
            ui.painter().line_segment(
                [Pos2::new(left, y), Pos2::new(right, y)],
                Stroke::new(1.0, t.line),
            );
        },
    );
}

fn section_title_with_switch<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    text: &str,
    accent: Color32,
    switch_label: &str,
    switch_param: &P,
    setter: &ParamSetter,
    scale: f32,
) {
    let extra = if switch_label.is_empty() {
        0.0
    } else {
        12.0 * scale
    };
    let row_h = CARD_TITLE_H * scale + extra;
    let avail_w = ui.available_width();
    let (row_rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, row_h), Sense::hover());

    let pill_w = 30.0 * scale;
    let pill_h = 14.0 * scale;

    let title_font = egui::FontId::proportional(11.0 * scale);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), title_font, accent);
    let title_w = galley.size().x;
    let title_y = row_rect.top() + 2.0 * scale + pill_h * 0.5 - galley.size().y * 0.5;
    ui.painter()
        .galley(Pos2::new(row_rect.left(), title_y), galley, accent);

    let switch_rect = Rect::from_min_size(
        Pos2::new(row_rect.right() - pill_w, row_rect.top() + 2.0 * scale),
        Vec2::new(pill_w, pill_h),
    );

    let switch_id = egui::Id::new(("card_switch", switch_param.name()));
    let switch_resp = ui.interact(switch_rect, switch_id, Sense::click());
    let on = switch_param.unmodulated_normalized_value() > 0.5;
    if switch_resp.clicked() {
        setter.begin_set_parameter(switch_param);
        setter.set_parameter_normalized(switch_param, if on { 0.0 } else { 1.0 });
        setter.end_set_parameter(switch_param);
    }

    let tt = ui.ctx().animate_bool_with_time(
        egui::Id::new(("card_switch_fade", switch_param.name())),
        on,
        0.18,
    );
    if ui.is_rect_visible(switch_rect) {
        let painter = ui.painter();
        painter.rect_filled(
            switch_rect,
            pill_h * 0.5,
            lerp_color(t.track_bg, accent, tt),
        );
        let off_x = switch_rect.left() + pill_h * 0.5;
        let on_x = switch_rect.right() - pill_h * 0.5;
        let thumb_x = off_x + (on_x - off_x) * tt;

        let is_dark = t.bg_top.r() < 128;
        if is_dark {
            let shadow_center = Pos2::new(thumb_x, switch_rect.center().y + 1.0 * scale);
            let base_r = pill_h * 0.38;
            painter.circle_filled(
                shadow_center,
                base_r * 1.00,
                Color32::from_rgba_unmultiplied(0, 0, 0, 38),
            );
            painter.circle_filled(
                shadow_center,
                base_r * 1.18,
                Color32::from_rgba_unmultiplied(0, 0, 0, 18),
            );
            painter.circle_filled(
                shadow_center,
                base_r * 1.42,
                Color32::from_rgba_unmultiplied(0, 0, 0, 7),
            );
        }

        painter.circle_filled(
            Pos2::new(thumb_x, switch_rect.center().y),
            pill_h * 0.38,
            lerp_color(t.dim, Color32::WHITE, tt),
        );
    }

    if !switch_label.is_empty() {
        let label_rect = Rect::from_min_size(
            Pos2::new(
                switch_rect.left() - 8.0 * scale,
                switch_rect.bottom() + 4.0 * scale,
            ),
            Vec2::new(switch_rect.width() + 16.0 * scale, 11.0 * scale),
        );
        ui.painter().text(
            label_rect.center(),
            egui::Align2::CENTER_CENTER,
            switch_label,
            egui::FontId::proportional(9.5 * scale),
            t.dim2,
        );
    }

    let line_y = switch_rect.center().y;
    let line_left = row_rect.left() + title_w + 6.0 * scale;
    let line_right = switch_rect.left() - 6.0 * scale;
    if line_right > line_left {
        ui.painter().line_segment(
            [Pos2::new(line_left, line_y), Pos2::new(line_right, line_y)],
            Stroke::new(1.0, t.line),
        );
    }

    ui.add_space(4.0 * scale);
}

/// Draws a card title with an Enable/Disable switch on the right.
///
/// When the Texture kill switch is engaged (see
/// `texture_kill_button`), the switch becomes **non-interactive**
/// and its tooltip changes to explain why. This prevents the user
/// from re-enabling individual layers while the kill switch is on,
/// which would otherwise leave the UI in an inconsistent state.
///
/// The lock is enforced at the UI level only; the host can still
/// write to the underlying `BoolParam` via automation.
fn section_title_with_enabled_switch<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    text: &str,
    accent: Color32,
    enabled_param: &P,
    setter: &ParamSetter,
    scale: f32,
) {
    let row_h = CARD_TITLE_H * scale;
    let avail_w = ui.available_width();
    let (row_rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, row_h), Sense::hover());

    let pill_w = 30.0 * scale;
    let pill_h = 14.0 * scale;

    let title_font = egui::FontId::proportional(11.0 * scale);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), title_font, accent);
    let title_w = galley.size().x;
    let title_y = row_rect.top() + 2.0 * scale + pill_h * 0.5 - galley.size().y * 0.5;
    ui.painter()
        .galley(Pos2::new(row_rect.left(), title_y), galley, accent);

    let switch_rect = Rect::from_min_size(
        Pos2::new(row_rect.right() - pill_w, row_rect.top() + 2.0 * scale),
        Vec2::new(pill_w, pill_h),
    );

    // Kill-switch state determines whether the switch is interactive.
    //
    // The boolean is published by `texture_kill_button` on the same
    // frame (the header is always drawn before the page body), so
    // the button and every card switch below it always agree on
    // whether the kill is currently active. Using the shared flag
    // instead of the raw `tx_kill_state` slot matters because the
    // slot can linger in memory after the params have drifted back
    // (e.g. an automation lane turning Shimmer on while the kill
    // is engaged); the derived flag tracks the params directly and
    // so stays consistent.
    let kill_active: bool = ui.memory(|m| {
        m.data
            .get_temp(egui::Id::new("pejvak_tx_kill_active"))
            .unwrap_or(false)
    });

    let switch_id = egui::Id::new(("texture_enabled_switch", enabled_param.name()));
    let switch_resp = ui.interact(switch_rect, switch_id, Sense::click());

    let on = enabled_param.unmodulated_normalized_value() > 0.5;

    // Only toggle the parameter if the kill switch is not engaged.
    if switch_resp.clicked() && !kill_active {
        setter.begin_set_parameter(enabled_param);
        setter.set_parameter_normalized(enabled_param, if on { 0.0 } else { 1.0 });
        setter.end_set_parameter(enabled_param);
    }

    let tt = ui.ctx().animate_bool_with_time(
        egui::Id::new(("texture_enabled_switch_fade", enabled_param.name())),
        on,
        0.18,
    );
    if ui.is_rect_visible(switch_rect) {
        let painter = ui.painter();

        // When locked, the switch is drawn in a soft red tone that
        // matches the kill button in the header.
        let track_col = if kill_active {
            KILL_RED.gamma_multiply(0.20)
        } else {
            lerp_color(t.track_bg, accent, tt)
        };
        painter.rect_filled(switch_rect, pill_h * 0.5, track_col);

        let off_x = switch_rect.left() + pill_h * 0.5;
        let on_x = switch_rect.right() - pill_h * 0.5;
        let thumb_x = off_x + (on_x - off_x) * tt;

        let is_dark = t.bg_top.r() < 128;
        if is_dark {
            let shadow_center = Pos2::new(thumb_x, switch_rect.center().y + 1.0 * scale);
            let base_r = pill_h * 0.38;
            painter.circle_filled(
                shadow_center,
                base_r * 1.00,
                Color32::from_rgba_unmultiplied(0, 0, 0, 38),
            );
            painter.circle_filled(
                shadow_center,
                base_r * 1.18,
                Color32::from_rgba_unmultiplied(0, 0, 0, 18),
            );
            painter.circle_filled(
                shadow_center,
                base_r * 1.42,
                Color32::from_rgba_unmultiplied(0, 0, 0, 7),
            );
        }

        let thumb_col = if kill_active {
            KILL_RED.gamma_multiply(0.75)
        } else {
            lerp_color(t.dim, Color32::WHITE, tt)
        };
        painter.circle_filled(
            Pos2::new(thumb_x, switch_rect.center().y),
            pill_h * 0.38,
            thumb_col,
        );
    }

    let line_y = switch_rect.center().y;
    let line_left = row_rect.left() + title_w + 6.0 * scale;
    let line_right = switch_rect.left() - 6.0 * scale;
    if line_right > line_left {
        ui.painter().line_segment(
            [Pos2::new(line_left, line_y), Pos2::new(line_right, line_y)],
            Stroke::new(1.0, t.line),
        );
    }

    // The tooltip explains the locked state when applicable.
    if kill_active {
        let _ = hover_tip(
            switch_resp,
            "Locked — release the TX button in the header to enable Texture layers",
        );
    }

    ui.add_space(4.0 * scale);
}

fn rounded_gradient(rect: Rect, radius: f32, top: Color32, bottom: Color32) -> egui::Shape {
    if !(rect.width() > 0.5 && rect.height() > 0.5) {
        return egui::Shape::Noop;
    }
    let r = radius.clamp(0.0, rect.width().min(rect.height()) * 0.5);
    let steps = 6;
    let mut perimeter = Vec::with_capacity(steps * 4 + 4);
    let centers = [
        Pos2::new(rect.left() + r, rect.top() + r),
        Pos2::new(rect.right() - r, rect.top() + r),
        Pos2::new(rect.right() - r, rect.bottom() - r),
        Pos2::new(rect.left() + r, rect.bottom() - r),
    ];
    let starts = [PI, 1.5 * PI, 0.0, 0.5 * PI];
    for c in 0..4 {
        let cc = centers[c];
        for s in 0..=steps {
            let a = starts[c] + (s as f32 / steps as f32) * 0.5 * PI;
            perimeter.push(Pos2::new(cc.x + a.cos() * r, cc.y + a.sin() * r));
        }
    }
    let mut mesh = egui::Mesh::default();
    let ymin = rect.top();
    let yspan = (rect.bottom() - ymin).max(1.0);
    let lc = |y: f32| {
        let tt = ((y - ymin) / yspan).clamp(0.0, 1.0);
        Color32::from_rgba_unmultiplied(
            (top.r() as f32 * (1.0 - tt) + bottom.r() as f32 * tt) as u8,
            (top.g() as f32 * (1.0 - tt) + bottom.g() as f32 * tt) as u8,
            (top.b() as f32 * (1.0 - tt) + bottom.b() as f32 * tt) as u8,
            (top.a() as f32 * (1.0 - tt) + bottom.a() as f32 * tt) as u8,
        )
    };
    let center = rect.center();
    mesh.colored_vertex(center, lc(center.y));
    for p in &perimeter {
        mesh.colored_vertex(*p, lc(p.y));
    }
    let n = perimeter.len() as u32;
    for i in 1..=n {
        let next = if i == n { 1 } else { i + 1 };
        mesh.add_triangle(0, i, next);
    }
    egui::Shape::mesh(mesh)
}

/// Draws the modal panel (Manual / About) centred in the plugin
/// window. The modal is drawn as a floating window with a dimmed
/// backdrop. Clicking outside the modal closes it.
///
/// The close path runs through a short fade-out animation: when the
/// user dismisses the modal (× button, backdrop click, or Escape),
/// `closing_at` is set and the fade plays out over `MODAL_FADE_SECS`.
/// Only when the fade reaches zero does the modal get removed from
/// memory. This gives a symmetric open/close feel and avoids the
/// "pop" of an instant dismissal.
fn draw_modal_panel(ui: &mut Ui, t: &Theme, scale: f32) {
    let modal_id = egui::Id::new("pejvak_modal_panel");
    let opened_at_id = modal_id.with("opened_at");
    let closing_at_id = modal_id.with("closing_at");

    let panel: Option<ModalPanel> = ui.memory(|m| m.data.get_temp(modal_id));
    let Some(panel) = panel else {
        return;
    };

    // Duration of the open and close fades, in seconds.
    const MODAL_FADE_SECS: f64 = 0.22;

    let now = ui.input(|i| i.time);
    let opened_at: f64 = ui.memory(|m| m.data.get_temp(opened_at_id).unwrap_or(0.0));
    let closing_at: f64 = ui.memory(|m| m.data.get_temp(closing_at_id).unwrap_or(0.0));
    let is_closing = closing_at > 0.0;

    // Progress: 1.0 = fully visible, 0.0 = fully transparent.
    //   * Opening: ramps 0 → 1 over `MODAL_FADE_SECS`.
    //   * Closing: ramps 1 → 0 over `MODAL_FADE_SECS`.
    // Smoothstep easing gives the fade a soft accelerate-then-
    // decelerate shape with no extra state.
    let raw = if is_closing {
        ((closing_at - now) / MODAL_FADE_SECS).clamp(0.0, 1.0) as f32
    } else {
        ((now - opened_at) / MODAL_FADE_SECS).clamp(0.0, 1.0) as f32
    };
    let progress = raw * raw * (3.0 - 2.0 * raw);

    // Closing animation has fully faded out: actually remove the
    // modal from memory so the next frame does not draw it at all.
    if is_closing && progress <= 0.0 {
        ui.memory_mut(|m| {
            m.data.remove::<ModalPanel>(modal_id);
            m.data.remove::<f64>(opened_at_id);
            m.data.remove::<f64>(closing_at_id);
        });
        return;
    }

    // Keep the frame loop running while the fade is animating.
    if progress < 1.0 {
        ui.ctx().request_repaint();
    }

    // Grace period: suppress backdrop-click-to-close for the first
    // 300 ms. Only meaningful while opening (or open); while
    // closing we ignore all input anyway.
    let grace_elapsed = !is_closing && (now - opened_at > 0.30);

    let is_light = t.bg_top.r() > 128;

    // ---- Dim backdrop over the whole plugin window ----
    let screen = ui.max_rect();
    let backdrop_layer = egui::LayerId::new(egui::Order::Middle, egui::Id::new("modal_backdrop"));

    let base_alpha = if is_light { 80.0 } else { 140.0 };
    ui.ctx().layer_painter(backdrop_layer).rect_filled(
        screen,
        0.0,
        Color32::from_rgba_unmultiplied(0, 0, 0, (base_alpha * progress).round() as u8),
    );

    let max_modal_w = screen.width() * 0.68;
    let max_modal_h = screen.height() * 0.82;
    let modal_w = (480.0 * scale).min(max_modal_w);
    let modal_h = (340.0 * scale).min(max_modal_h);
    let modal_rect = Rect::from_center_size(screen.center(), Vec2::new(modal_w, modal_h));

    let bg = if is_light {
        Color32::from_rgb(0xF6, 0xF7, 0xFA)
    } else {
        Color32::from_rgb(0x1A, 0x21, 0x36)
    };
    let body_col = if is_light {
        Color32::from_rgb(0x1D, 0x21, 0x29)
    } else {
        t.txt
    };

    let mut close_clicked = false;

    egui::Area::new(egui::Id::new("modal_area"))
        .order(egui::Order::Foreground)
        .fixed_pos(modal_rect.min)
        .show(ui.ctx(), |ui| {
            // Fade the entire modal content with the same progress.
            ui.multiply_opacity(progress);

            let frame = egui::Frame::NONE
                .fill(bg)
                .stroke(Stroke::new(1.0, t.line))
                .corner_radius(10.0 * scale)
                .shadow(egui::epaint::Shadow {
                    offset: [0, 10],
                    blur: 200,
                    spread: 0,
                    color: Color32::from_rgba_unmultiplied(0, 0, 0, 160),
                })
                .inner_margin(egui::Margin::same(16));

            frame.show(ui, |ui| {
                ui.set_min_size(Vec2::new(modal_w - 32.0, modal_h - 32.0));
                ui.set_max_size(Vec2::new(modal_w - 32.0, modal_h - 32.0));

                // ---- Header row (title + close button) ----
                ui.horizontal(|ui| {
                    let title = match panel {
                        ModalPanel::About => "About",
                    };
                    ui.label(
                        egui::RichText::new(title)
                            .size(14.0 * scale)
                            .strong()
                            .color(t.accent),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn_size = 20.0 * scale;
                        let (r, r_resp) =
                            ui.allocate_exact_size(Vec2::splat(btn_size), Sense::click());
                        if r_resp.hovered() {
                            ui.painter()
                                .rect_filled(r, 4.0 * scale, t.accent.gamma_multiply(0.20));
                        }
                        // Draw an "×" glyph
                        let stroke = Stroke::new(1.5 * scale, body_col);
                        let a = r.shrink(6.0 * scale);
                        ui.painter()
                            .line_segment([a.left_top(), a.right_bottom()], stroke);
                        ui.painter()
                            .line_segment([a.right_top(), a.left_bottom()], stroke);
                        if r_resp.clicked() {
                            close_clicked = true;
                        }
                    });
                });

                ui.add_space(8.0 * scale);
                ui.separator();
                ui.add_space(8.0 * scale);

                // ---- Body ----
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match panel {
                        ModalPanel::About => draw_about_body(ui, t, body_col, scale),
                    });
            });
        });

    // While closing, ignore all input so a second click during the
    // fade-out cannot restart or cancel the animation.
    if is_closing {
        return;
    }

    // ---- Close on backdrop click ----
    let backdrop_clicked = grace_elapsed
        && ui.input(|i| i.pointer.any_click())
        && ui
            .input(|i| i.pointer.interact_pos())
            .map_or(false, |pos| !modal_rect.contains(pos));

    if close_clicked || backdrop_clicked || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        // Start the close animation instead of removing immediately.
        // The modal stays in memory with `closing_at` set, and the
        // fade-out plays out over the next ~220 ms.
        ui.memory_mut(|m| {
            m.data.insert_temp(closing_at_id, now + MODAL_FADE_SECS);
        });
    }
}


/// Body of the About modal.
fn draw_about_body(ui: &mut Ui, t: &Theme, body_col: Color32, scale: f32) {
    // ---- Logo ----
    // Decode and cache the embedded PNG as an egui texture on first
    // use. The texture lives in `Context::data`, keyed by a stable
    // `Id`, so subsequent opens of the About modal reuse it without
    // re-decoding the image. The `image` crate is only pulled in for
    // the `png` feature, so decoding is a few hundred microseconds.
    let logo_id = egui::Id::new("pejvak_logo_texture");
    let logo_texture: Option<egui::TextureHandle> =
        ui.ctx().data(|d| d.get_temp(logo_id));
    let logo_texture = match logo_texture {
        Some(tex) => Some(tex),
        None => {
            match image::load_from_memory(LOGO_PNG) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        [w as usize, h as usize],
                        rgba.as_raw(),
                    );
                    let tex = ui.ctx().load_texture(
                        "pejvak-logo",
                        color_image,
                        egui::TextureOptions::LINEAR,
                    );
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(logo_id, tex.clone());
                    });
                    Some(tex)
                }
                Err(_) => None,
            }
        }
    };

    if let Some(tex) = logo_texture {
        ui.vertical_centered(|ui| {
            let target_h = 64.0 * scale;
            let aspect = tex.size_vec2().x / tex.size_vec2().y.max(1.0);
            let target_w = target_h * aspect;
            ui.add(egui::Image::new((tex.id(), egui::vec2(target_w, target_h))));
            ui.add_space(6.0 * scale);
        });
    }

    // ---- Title ----
    ui.vertical_centered(|ui| {
        let _ = ui.label(
            egui::RichText::new("PEJVAK   RVB")
                .font(egui::FontId::new(
                    16.0 * scale,
                    egui::FontFamily::Name("Monoton".into()),
                ))
                .color(t.accent),
        );
        ui.add_space(2.0 * scale);
        let _ = ui.label(
            egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                .size(9.5 * scale)
                .color(t.dim2),
        );
    });
    ui.add_space(10.0 * scale);

    // ---- Description ----
    let _ = ui.label(
        egui::RichText::new(
            "A faithful Rust port of Valdemar Erlingsson's CloudSeed \
             (legacy-v1), with reference to the JUCE-based CloudReverb \
             fork by xunil-cloud. Expanded with a dual engine \
             architecture, a six-layer Texture Rack, Velvet-Noise \
             diffusion, and a drag-and-drop Signal Chain. Both DSP \
             engines run in pure f64 with zero allocation on the \
             audio thread.",
        )
        .size(10.0 * scale)
        .color(body_col),
    );

    ui.add_space(10.0 * scale);
    let _ = ui.label(
        egui::RichText::new("DSP ENGINES")
            .size(9.0 * scale)
            .strong()
            .color(t.dim2),
    );
    ui.add_space(2.0 * scale);
    for line in [
        "• CloudSeed Legacy Reverb (49 params, bit-identical port)",
        "• Filtered Feedback Delay Network (FFDN)",
        "    – Householder & Velvet feedback matrices",
        "    – Frequency-dependent attenuation (bass/mid/treble)",
        "    – Modulated delay lines with size smoothing",
        "    – Velvet saturation, tilt filter, early reflections",
        "    – Attack, ducking, and gate dynamics",
    ] {
        let _ = ui.label(egui::RichText::new(line).size(10.0 * scale).color(body_col));
    }

    ui.add_space(8.0 * scale);
    let _ = ui.label(
        egui::RichText::new("EXTENSIONS")
            .size(9.0 * scale)
            .strong()
            .color(t.dim2),
    );
    ui.add_space(2.0 * scale);
    for line in [
        "• 6 Texture Rack layers (Shimmer, Mist, Drift, Crystal, Scatter, Bloom)",
        "• ILLUSION — granular cloud with pitched overtones",
        "• Velvet-Noise VFDN diffusion inside CloudSeed",
        "• Reorderable Texture Signal Chain (drag & drop)",
        "• 50-step Undo / Redo with parameter-name tooltips",
        "• 96 factory presets across 16 categories",
    ] {
        let _ = ui.label(egui::RichText::new(line).size(10.0 * scale).color(body_col));
    }

    ui.add_space(8.0 * scale);
    let _ = ui.label(
        egui::RichText::new("BUILT WITH")
            .size(9.0 * scale)
            .strong()
            .color(t.dim2),
    );
    ui.add_space(2.0 * scale);
    let _ = ui.label(
        egui::RichText::new("Rust  •  nice-plug 0.4.2  •  egui 0.36  •  wide 1.7")
            .size(10.0 * scale)
            .color(body_col),
    );

    // ---- Credits footer ----
    ui.add_space(12.0 * scale);
    ui.vertical_centered(|ui| {
        let _ = ui.label(
           egui::RichText::new("Pejvak DSP")
                .size(11.0 * scale)
                .strong()
                .color(t.accent),
        );
        ui.add_space(2.0 * scale);
        let _ = ui.label(
            egui::RichText::new("Designed & developed by Amin Yousefinejad")
                .size(10.0 * scale)
                .color(body_col),
        );
        ui.add_space(4.0 * scale);
        let _ = ui.label(
            egui::RichText::new("GPL-3.0-or-later")
                .size(9.5 * scale)
                .color(t.dim2),
        );
    });
}