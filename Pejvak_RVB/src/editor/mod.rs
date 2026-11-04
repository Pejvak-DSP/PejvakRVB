//! PEJVAK RVB – Editor Root Module

pub mod header;
pub mod overlay;
pub mod pages;
pub mod presets_ui;
pub mod theme;
pub mod widgets;
pub mod xml;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;

use nice_plug::context::gui::GuiContext;
use nice_plug::editor::SpawnedEditor;
use nice_plug::editor::dpi::NativeSize;
use nice_plug::prelude::{Editor, ParentWindowHandle, ParamSetter};
use nice_plug::editor::HostMethods;
use nice_plug_egui::{
    create_egui_editor, EguiEditor, EguiNiceSettings, NiceEguiApp, RepaintNotifier,
};

use egui::{self, Color32, Pos2, Stroke, Ui, Vec2};

use crate::PejvakRvbParams;

pub use theme::Theme;
pub use xml::*;

// ============================================================================
// Constants
// ============================================================================

pub const WINDOW_W: u32 = 1100;
pub const WINDOW_H: u32 = 570;

pub const PAGE_GAP: f32 = 6.0;
pub const OUTER_MARGIN_H: f32 = 16.0;
pub const OUTER_MARGIN_V: f32 = 8.0;
pub const CARD_PAD: f32 = 8.0;
pub const CARD_TITLE_H: f32 = 24.0;

pub const KNOB_SLOT_W: f32 = 60.0;
pub const KNOB_SLOT_H: f32 = 76.0;
pub const KNOB_D: f32 = 42.0;

pub const KNOB_SMALL_SLOT_W: f32 = 52.0;
pub const KNOB_SMALL_SLOT_H: f32 = 66.0;
pub const KNOB_SMALL_D: f32 = 34.0;

pub const FADER_SLOT_W: f32 = 46.0;
pub const FADER_SLOT_H: f32 = 112.0;
pub const FADER_H: f32 = 72.0;

pub const NB_SLOT_W: f32 = 74.0;
pub const NB_SLOT_H: f32 = 34.0;

pub const ROW_INNER_GAP: f32 = 6.0;
pub const ROW_OUTER_GAP: f32 = 4.0;

pub const TEX_COLUMN_GAP: f32 = 10.0;

pub const PAGE_FADE_SECS: f32 = 0.15;
pub const ENGINE_FADE_SECS: f32 = 0.25;

pub const HEADER_GAP: f32 = 7.0;
pub const HEADER_TIGHT_GAP: f32 = 3.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditorPage {
    Main,
    Texture,
}

#[derive(Clone, Copy)]
pub enum HistoryAction {
    Undo,
    Redo,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModalPanel {
    About,
    Update,
}

#[inline]
pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(target_os = "windows")]
static LAST_FORWARDED_KEY: std::sync::Mutex<Option<(egui::Key, std::time::Instant)>> =
    std::sync::Mutex::new(None);

    #[cfg(target_os = "windows")]
    fn post_vk_to_host(vk: u16, scancode: u16, extended: bool, pressed: bool) {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetParent, PostMessageW, WM_KEYDOWN, WM_KEYUP,
        };
    
        unsafe {
            // Get the currently focused window (which is the plugin's window).
            let focus = GetFocus();
            if focus.is_null() { return; }
            
            // Get the parent window (which is the DAW's window).
            let host = GetParent(focus);
            if host.is_null() { return; }
    
            // Construct the lParam exactly as the OS would for a physical key press.
            let mut lparam: isize = 1 | ((scancode as isize) << 16);
            if extended {
                lparam |= 1 << 24;
            }
    
            let msg = if pressed {
                WM_KEYDOWN
            } else {
                // For KEYUP, set the previous state (bit 30) and transition state (bit 31).
                lparam |= (1 << 30) | (1 << 31);
                WM_KEYUP
            };
    
            PostMessageW(host, msg, vk as usize, lparam);
        }
    }
    
    #[cfg(target_os = "windows")]
    fn forward_key_to_daw(key: egui::Key, pressed: bool) {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    
        // (VK, Scancode, Extended flag)
        let (vk, scancode, extended): (u16, u16, bool) = match key {
            // --- Standard Keys ---
            egui::Key::Space => (VK_SPACE, 0x39, false),
            egui::Key::Enter => (VK_RETURN, 0x1C, false),
            egui::Key::Escape => (VK_ESCAPE, 0x01, false),
            egui::Key::Tab => (VK_TAB, 0x0F, false),
            egui::Key::Backspace => (VK_BACK, 0x0E, false),
            
            // --- Navigation / Arrows (Extended = True) ---
            egui::Key::Delete => (VK_DELETE, 0x53, true),
            egui::Key::Home => (VK_HOME, 0x47, true),
            egui::Key::End => (VK_END, 0x4F, true),
            egui::Key::ArrowUp => (VK_UP, 0x48, true),
            egui::Key::ArrowDown => (VK_DOWN, 0x50, true),
            egui::Key::ArrowLeft => (VK_LEFT, 0x4B, true),
            egui::Key::ArrowRight => (VK_RIGHT, 0x4D, true),
    
            // --- Numpad Keys (Extended = False for host compatibility) ---
            egui::Key::Num0 => (VK_NUMPAD0, 0x52, false),
            egui::Key::Num1 => (VK_NUMPAD1, 0x4F, false),
            egui::Key::Num2 => (VK_NUMPAD2, 0x50, false),
            egui::Key::Num3 => (VK_NUMPAD3, 0x51, false),
            egui::Key::Num4 => (VK_NUMPAD4, 0x4B, false),
            egui::Key::Num5 => (VK_NUMPAD5, 0x4C, false),
            egui::Key::Num6 => (VK_NUMPAD6, 0x4D, false),
            egui::Key::Num7 => (VK_NUMPAD7, 0x47, false),
            egui::Key::Num8 => (VK_NUMPAD8, 0x48, false),
            egui::Key::Num9 => (VK_NUMPAD9, 0x49, false),
            egui::Key::Plus => (VK_ADD, 0x4E, false),
            egui::Key::Minus => (VK_SUBTRACT, 0x4A, false),
            egui::Key::Slash => (VK_DIVIDE, 0x35, false),
            egui::Key::Period => (VK_DECIMAL, 0x53, false),
            egui::Key::Comma => (0x6C, 0x7C, false), // VK_SEPARATOR
    
            // --- Letters ---
            egui::Key::A => (0x41, 0x1E, false),
            egui::Key::B => (0x42, 0x30, false),
            egui::Key::C => (0x43, 0x2E, false),
            egui::Key::D => (0x44, 0x20, false),
            egui::Key::E => (0x45, 0x12, false),
            egui::Key::F => (0x46, 0x21, false),
            egui::Key::G => (0x47, 0x22, false),
            egui::Key::H => (0x48, 0x23, false),
            egui::Key::I => (0x49, 0x17, false),
            egui::Key::J => (0x4A, 0x24, false),
            egui::Key::K => (0x4B, 0x25, false),
            egui::Key::L => (0x4C, 0x26, false),
            egui::Key::M => (0x4D, 0x32, false),
            egui::Key::N => (0x4E, 0x31, false),
            egui::Key::O => (0x4F, 0x18, false),
            egui::Key::P => (0x50, 0x19, false),
            egui::Key::Q => (0x51, 0x10, false),
            egui::Key::R => (0x52, 0x13, false),
            egui::Key::S => (0x53, 0x1F, false),
            egui::Key::T => (0x54, 0x14, false),
            egui::Key::U => (0x55, 0x16, false),
            egui::Key::V => (0x56, 0x2F, false),
            egui::Key::W => (0x57, 0x11, false),
            egui::Key::X => (0x58, 0x2D, false),
            egui::Key::Y => (0x59, 0x15, false),
            egui::Key::Z => (0x5A, 0x2C, false),
            _ => return,
        };
    
        if let Ok(mut last) = LAST_FORWARDED_KEY.lock() {
            *last = Some((key, std::time::Instant::now()));
        }
    
        post_vk_to_host(vk, scancode, extended, pressed);
    }
    
    #[cfg(target_os = "windows")]
    fn forward_key_to_daw_star() {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_MULTIPLY;
        
        // Store Num8 as a proxy for '*' to prevent host echoing.
        if let Ok(mut last) = LAST_FORWARDED_KEY.lock() {
            *last = Some((egui::Key::Num8, std::time::Instant::now()));
        }
        
        post_vk_to_host(VK_MULTIPLY, 0x37, false, true);
        post_vk_to_host(VK_MULTIPLY, 0x37, false, false);
    }

// ============================================================================
// App struct & Editor wrapper
// ============================================================================

pub struct PejvakEguiApp {
    pub params: Arc<PejvakRvbParams>,
    pub gui_context: Option<GuiContext>,
    pub current_preset: String,
    pub user_presets: Vec<String>,
    pub user_presets_last_refresh: f64,
    pub current_page: EditorPage,
    pub pending_page: Option<EditorPage>,
    pub page_fade: f32,
    pub initial_size_applied: bool,
    pub text_edit_active: bool,
    pub engine_fade: f32,
    pub current_engine_ffdn: bool,
    pub pending_engine: Option<bool>,
    pub undo_stack: std::collections::VecDeque<xml::UndoSnapshot>,
    pub redo_stack: Vec<xml::UndoSnapshot>,
    pub last_snapshot: Option<xml::UndoSnapshot>,
    pub suppress_until: f64,
    pub was_focused: Option<bool>,
    pub last_history_apply: f64,
    pub update_phase: Arc<Mutex<crate::updater::UpdatePhase>>,
    pub update_busy: Arc<std::sync::atomic::AtomicBool>,
}

pub struct PejvakEditor {
    inner: EguiEditor<PejvakEguiApp>,
    params: Arc<PejvakRvbParams>, // Added to access persisted size
}

impl Editor for PejvakEditor {
    type Handle = <EguiEditor<PejvakEguiApp> as Editor>::Handle;

    fn spawn(
        &self,
        parent: Option<ParentWindowHandle>,
        wait_for_parent: bool,
        fallback_scale_factor: Option<f64>,
        context: GuiContext,
        host: Option<HostMethods>,
    ) -> Result<SpawnedEditor<Self::Handle>, Box<dyn std::error::Error>> {
        self.inner
            .spawn(parent, wait_for_parent, fallback_scale_factor, context, host)
    }

    fn size(&self) -> NativeSize<u32> {
        // Read the persisted size so the DAW opens the window correctly
        // before the first `ui()` frame is ever rendered.
        let (w, h) = if let Ok(size) = self.params.editor_size.lock() {
            *size
        } else {
            (WINDOW_W, WINDOW_H)
        };
        
        NativeSize {
            width: w,
            height: h,
        }
    }
}

pub fn create_editor(params: Arc<PejvakRvbParams>) -> Option<PejvakEditor> {
    let initial_engine = params.ffdn.use_ffdn.value();
    let initial_preset = params
        .current_preset
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();

    let app = PejvakEguiApp {
        params: params.clone(),
        gui_context: None,
        current_preset: initial_preset,
        user_presets: Vec::new(),
        user_presets_last_refresh: f64::NEG_INFINITY,
        current_page: EditorPage::Main,
        pending_page: None,
        page_fade: 1.0,
        initial_size_applied: false,
        text_edit_active: false,
        engine_fade: 1.0,
        current_engine_ffdn: initial_engine,
        pending_engine: None,
        undo_stack: std::collections::VecDeque::with_capacity(xml::UNDO_DEPTH),
        redo_stack: Vec::with_capacity(xml::UNDO_DEPTH),
        last_snapshot: None,
        suppress_until: 0.0,
        was_focused: None,
        last_history_apply: 0.0,
        update_phase: Arc::new(Mutex::new(crate::updater::UpdatePhase::Idle)),
        update_busy: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    let mut settings = EguiNiceSettings::default();
    settings.resize_hint.can_resize = true;
    settings.resize_hint.can_resize_horizontally = true;
    settings.resize_hint.can_resize_vertically = true;
    // Enforce aspect ratio so the UI scales proportionally in both axes.
    settings.resize_hint.preserve_aspect_ratio = true;
    settings.resize_hint.aspect_ratio_width = WINDOW_W;
    settings.resize_hint.aspect_ratio_height = WINDOW_H;


    let inner = create_egui_editor(
        params.editor_state.clone(),
        RepaintNotifier::default(),
        settings,
        app,
    )?;

    Some(PejvakEditor {
        inner,
        params: params.clone(), // Pass params to the editor
    })
}

impl NiceEguiApp for PejvakEguiApp {
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        nice_gui_ctx: GuiContext,
        _frame: &mut nice_plug_egui::Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        self.gui_context = Some(nice_gui_ctx);

        egui_ctx.tessellation_options_mut(|opts| {
            opts.feathering = true;
            opts.feathering_size_in_pixels = 1.0;
        });

        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "Monoton".to_owned(),
            Arc::new(egui::FontData::from_static(include_bytes!(
                "../../fonts/Monoton-Regular.ttf"
            ))),
        );
        fonts.families.insert(
            egui::FontFamily::Name("Monoton".into()),
            vec!["Monoton".to_owned()],
        );
        egui_ctx.set_fonts(fonts);
        crate::manual::init();
        Ok(())
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut nice_plug_egui::Frame) {
        let gui_ctx = match self.gui_context.clone() {
            Some(c) => c,
            None => return,
        };
        let setter = gui_ctx.param_setter();

        let want_update_check = ui.memory(|m| {
            m.data
                .get_temp::<bool>(egui::Id::unique("pejvak_check_updates_requested"))
        }).unwrap_or(false);
        if want_update_check {
            ui.memory_mut(|m| {
                m.data
                    .remove::<bool>(egui::Id::unique("pejvak_check_updates_requested"))
            });
            self.start_update_check(ui.ctx().clone());
        }

        let want_download = ui.memory(|m| {
            m.data
                .get_temp::<(String, String, String, Option<String>)>(egui::Id::unique(
                    "pejvak_update_download_request",
                ))
        });
        if let Some((version, url, name, digest)) = want_download {
            ui.memory_mut(|m| {
                m.data
                    .remove::<(String, String, String, Option<String>)>(egui::Id::unique(
                        "pejvak_update_download_request",
                    ))
            });
            self.start_update_download(ui.ctx().clone(), version, url, name, digest);
        }

        let manual_pending = ui.memory(|m| {
            m.data
                .get_temp::<bool>(egui::Id::unique("pejvak_manual_open_pending"))
        }).unwrap_or(false);
        if manual_pending {
            ui.memory_mut(|m| {
                m.data
                    .remove::<bool>(egui::Id::unique("pejvak_manual_open_pending"))
            });
            crate::manual::open_in_browser();
        }

        let plugin_active = self.params.plugin_active.load(std::sync::atomic::Ordering::Relaxed);
        let poll_ms = if plugin_active { 100 } else { 33 };
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(poll_ms));

        if let Ok(p) = self.params.current_preset.try_lock() {
            if *p != self.current_preset {
                self.current_preset = p.clone();
            }
        }

        let show_popups = self.params.show_popups.load(Ordering::Relaxed);
        ui.memory_mut(|m| {
            m.data
                .insert_temp(egui::Id::unique("pejvak_show_popups"), show_popups)
        });

        ui.ctx().all_styles_mut(|s| s.interaction.selectable_labels = false);

        let focused = ui.ctx().input(|i| i.focused);
        if self.was_focused == Some(false) && focused {
            ui.ctx().request_repaint();
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
        }
        self.was_focused = Some(focused);

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
        if user_active {
            self.suppress_until = now + xml::UNDO_SUPPRESS_S;
        }

        let current = xml::capture_undo_snapshot(&self.params, &self.current_preset, self.current_page);
        let has_change = self
            .last_snapshot
            .as_ref()
            .is_some_and(|prev| xml::snapshots_differ(prev, &current, 1.0e-4));

        let history_grace = (now - self.last_history_apply) < 0.8;
        let should_absorb = !has_change || (history_grace && !user_active);

        if self.last_snapshot.is_none() {
            self.last_snapshot = Some(current);
        } else if has_change && now >= self.suppress_until && !should_absorb {
            let prev_owned = self.last_snapshot.take().unwrap();
            self.undo_stack.push_back(prev_owned);
            while self.undo_stack.len() > xml::UNDO_DEPTH {
                self.undo_stack.pop_front();
            }
            self.redo_stack.clear();
            self.suppress_until = now + xml::UNDO_SUPPRESS_S;
            self.last_snapshot = Some(current);
        } else if should_absorb {
            self.last_snapshot = Some(current);
        }

        if !self.initial_size_applied {
            // Read the persisted editor size and request it from the host.
            let (w, h) = if let Ok(size) = self.params.editor_size.lock() {
                *size
            } else {
                (WINDOW_W, WINDOW_H)
            };

            let current_size = ui.max_rect().size();
            // Keep requesting the size until the host actually applies it.
            // Some DAWs ignore the resize command on the very first frame.
            if (current_size.x - w as f32).abs() > 2.0 || (current_size.y - h as f32).abs() > 2.0 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(
                    egui::Vec2::new(w as f32, h as f32),
                ));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::MinInnerSize(
                    egui::Vec2::new(550.0, 285.0),
                ));
                ui.ctx().request_repaint();
            } else {
                self.initial_size_applied = true;
            }
        }

        self.text_edit_active = Self::read_text_edit_active(ui);
        self.poll_preset_dialog(&setter);

        let now_for_presets = ui.input(|i| i.time);
        if now_for_presets - self.user_presets_last_refresh > 0.5 {
            self.user_presets = xml::user_preset_names();
            self.user_presets_last_refresh = now_for_presets;
        }

        let is_light = self.params.is_light_theme.load(std::sync::atomic::Ordering::Relaxed);
        let current_size = ui.max_rect().size();

        // Removed the forced aspect ratio resizing. The host DAW controls the 
        // outer window size. The draw_editor function will automatically scale 
        // and center the UI to fit whatever size the DAW provides.
        let last_size_id = egui::Id::unique("pejvak_last_window_size");
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

        let param_engine = self.params.ffdn.use_ffdn.value();
        let visual_target = self.pending_engine.unwrap_or(self.current_engine_ffdn);
        if param_engine != visual_target {
            if param_engine == self.current_engine_ffdn {
                self.pending_engine = None;
            } else {
                self.pending_engine = Some(param_engine);
            }
        }

        if let Some(target) = self.pending_engine {
            self.engine_fade = (self.engine_fade - engine_step).max(0.0);
            if self.engine_fade <= 0.0 {
                self.current_engine_ffdn = target;
                self.pending_engine = None;
            }
            ui.ctx().request_repaint();
        } else if self.engine_fade < 1.0 {
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

        let undo_available_pre = !self.undo_stack.is_empty();
        let redo_available_pre = !self.redo_stack.is_empty();

        let undo_label = self
            .undo_stack
            .back()
            .and_then(|target| {
                self.last_snapshot
                    .as_ref()
                    .map(|cur| xml::summarize_diff(cur, target))
            })
            .unwrap_or_default();
        let redo_label = self
            .redo_stack
            .last()
            .and_then(|target| {
                self.last_snapshot
                    .as_ref()
                    .map(|cur| xml::summarize_diff(cur, target))
            })
            .unwrap_or_default();

        let mut pending_action: Option<HistoryAction> = None;
        let update_phase = self.update_phase.clone();
        let update_busy = self.update_busy.clone();

        let handle_color = if is_light {
            egui::Color32::from_rgb(0x40, 0x40, 0x40)
        } else {
            egui::Color32::from_rgb(0xC0, 0xC0, 0xC0)
        };
        let mut temp_style = ui.style().as_ref().clone();
        temp_style.visuals.widgets.noninteractive.fg_stroke.color = handle_color;
        temp_style.visuals.widgets.active.fg_stroke.color = handle_color;
        temp_style.visuals.widgets.inactive.fg_stroke.color = handle_color;
        temp_style.visuals.widgets.hovered.fg_stroke.color = handle_color;
        ui.set_style(temp_style);

        // Ensure the window fills the entire available space provided by the host,
        // so there are no mismatched borders or empty gaps on the edges.
        let screen = ui.max_rect();
        const TARGET_RATIO: f32 = WINDOW_W as f32 / WINDOW_H as f32;
        let current_ratio = screen.width() / screen.height();

        // Native proportional resizing:
        // Only enforce the aspect ratio when the user releases the mouse button
        // (finishes dragging the resize handle).
        if (current_ratio - TARGET_RATIO).abs() > 0.01 && ui.input(|i| i.pointer.any_released()) {
            let target_h = screen.width() / TARGET_RATIO;
            let target_w = screen.height() * TARGET_RATIO;

            let (new_w, new_h) = if (screen.width() - target_w).abs() < (screen.height() - target_h).abs() {
                (target_w, screen.height())
            } else {
                (screen.width(), target_h)
            };

            let final_w = new_w.max(550.0);
            let final_h = new_h.max(285.0);

            ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(
                egui::Vec2::new(final_w, final_h),
            ));
        }

        let theme = {
            let t = ui.ctx().animate_bool_with_time(
                egui::Id::unique("pejvak_theme_fade"),
                is_light,
                0.28,
            );
            Theme::dark().lerp(&Theme::light(), t)
        };

        let scale = (screen.width() / WINDOW_W as f32)
            .min(screen.height() / WINDOW_H as f32)
            .clamp(0.25, 2.6);

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

        overlay::draw_modal_panel(ui, &theme, scale, &update_phase, &update_busy);

        if !plugin_active {
            overlay::draw_plugin_disabled_overlay(ui, &theme, scale);
        }

        {
            let force_id = egui::Id::unique("pejvak_force_undo_commit_at");
            let commit_at: f64 = ui.memory(|m| m.data.get_temp(force_id).unwrap_or(0.0));
            let now = ui.input(|i| i.time);

            if commit_at > 0.0 {
                if now >= commit_at {
                    ui.memory_mut(|m| m.data.remove::<f64>(force_id));
                    let current = xml::capture_undo_snapshot(
                        &self.params,
                        &current_preset,
                        self.current_page,
                    );
                    if let Some(prev) = self.last_snapshot.take() {
                        if xml::snapshots_differ(&prev, &current, 1.0e-4) {
                            self.undo_stack.push_back(prev);
                            while self.undo_stack.len() > xml::UNDO_DEPTH {
                                self.undo_stack.pop_front();
                            }
                            self.redo_stack.clear();
                            self.suppress_until = now + xml::UNDO_SUPPRESS_S;
                            self.last_snapshot = Some(current);
                        } else {
                            self.last_snapshot = Some(prev);
                        }
                    } else {
                        self.last_snapshot = Some(current);
                    }
                } else {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(20));
                }
            }
        }

        if let Some(act) = pending_action {
            let now = ui.input(|i| i.time);
            let result = match act {
                HistoryAction::Undo => self.do_undo(&setter, now),
                HistoryAction::Redo => self.do_redo(&setter, now),
            };
            if let Some((name, target_page)) = result {
                current_preset = name;
                if target_page != current_page && pending_page.is_none() {
                    pending_page = Some(target_page);
                }
            }
            ui.ctx().request_repaint();
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }

        let mut has_star = false;
        ui.input(|i| {
            for ev in &i.events {
                if let egui::Event::Text(t) = ev {
                    if t == "*" { has_star = true; }
                }
            }
        });

        if !self.text_edit_active {
            #[cfg(target_os = "windows")]
            {
                // Loop prevention: Check if this key was just forwarded to the host.
                // If so, ignore it because the host is probably echoing it back.
                let ignore_key = {
                    if let Ok(last) = LAST_FORWARDED_KEY.lock() {
                        if let Some((k, t)) = *last {
                            if t.elapsed().as_millis() < 50 {
                                Some(k)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

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
                                if *repeat {
                                    None
                                } else if has_star && (*key == egui::Key::Num8) {
                                    // Skip Num8 because it's part of the '*' key
                                    None
                                } else if Some(*key) == ignore_key {
                                    // Ignore echoed keys from the DAW
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
                if has_star {
                    forward_key_to_daw_star();
                }
            }
        }
        self.current_preset = current_preset;

        if let Ok(mut p) = self.params.current_preset.try_lock() {
            if *p != self.current_preset {
                *p = self.current_preset.clone();
            }
        }
        self.current_page = current_page;
        self.pending_page = pending_page;
    }
}

impl PejvakEguiApp {
    fn poll_preset_dialog(&mut self, setter: &ParamSetter) {
        let mut st = xml::pd_lock();
        let mut touched_disk = false;

        if let Some(path) = st.save_path.take() {
            let xml_content = xml::params_to_xml(&self.params);
            let preset_name = path.file_stem().and_then(|x| x.to_str()).map(String::from);
            std::thread::spawn(move || {
                let _ = std::fs::write(&path, xml_content);
            });
            if let Some(name) = preset_name {
                self.current_preset = name;
            }
            touched_disk = true;
        }

        if let Some(path) = st.load_path.take() {
            if let Some(parsed) = xml::xml_to_params(&path) {
                xml::apply_cloudseed_partial(&self.params, setter, &parsed.cloudseed);
                xml::apply_texture_params(&self.params, setter, &parsed.texture, &parsed.enabled);
                if let Some(order) = parsed.texture_order {
                    xml::apply_texture_order(&self.params, order);
                }
                if let Some(eq) = parsed.eq_enabled {
                    xml::apply_eq_master(&self.params, setter, eq);
                }
                xml::apply_ffdn_params(&self.params, setter, &parsed.ffdn);
                xml::apply_illusion_params(
                    &self.params,
                    setter,
                    &parsed.illusion,
                    parsed.illusion_enabled,
                    parsed.illusion_seen,
                );
                if let Some(stem) = path.file_stem().and_then(|x| x.to_str()) {
                    self.current_preset = stem.to_string();
                }
            }
            touched_disk = true;
        }

        if touched_disk {
            self.user_presets_last_refresh = f64::NEG_INFINITY;
        }
    }

    fn do_undo(&mut self, setter: &ParamSetter, now: f64) -> Option<(String, EditorPage)> {
        let target = self.undo_stack.pop_back()?;
        let target_name = target.preset_name.clone();
        let target_page = target.page;

        if let Some(cur) = self.last_snapshot.take() {
            self.redo_stack.push(cur);
        }

        xml::apply_undo_snapshot(&self.params, setter, &target);
        self.last_snapshot = Some(target);
        self.suppress_until = now + xml::UNDO_SUPPRESS_S;
        self.last_history_apply = now;

        Some((target_name, target_page))
    }

    fn do_redo(&mut self, setter: &ParamSetter, now: f64) -> Option<(String, EditorPage)> {
        let target = self.redo_stack.pop()?;
        let target_name = target.preset_name.clone();
        let target_page = target.page;

        if let Some(cur) = self.last_snapshot.take() {
            self.undo_stack.push_back(cur);
            while self.undo_stack.len() > xml::UNDO_DEPTH {
                self.undo_stack.pop_front();
            }
        }

        xml::apply_undo_snapshot(&self.params, setter, &target);
        self.last_snapshot = Some(target);
        self.suppress_until = now + xml::UNDO_SUPPRESS_S;
        self.last_history_apply = now;

        Some((target_name, target_page))
    }

    fn start_update_check(&self, ctx: egui::Context) {
        use std::sync::atomic::Ordering;
        if self.update_busy.swap(true, Ordering::SeqCst) {
            return;
        }
        let phase = self.update_phase.clone();
        let busy = self.update_busy.clone();
        if let Ok(mut p) = phase.lock() {
            *p = crate::updater::UpdatePhase::Checking;
        }
        ctx.request_repaint();

        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::updater::check_now()
            }))
            .unwrap_or_else(|_| {
                crate::updater::UpdatePhase::Error(
                    "Update check crashed unexpectedly. Please try again.".to_string(),
                )
            });

            if let Ok(mut p) = phase.lock() {
                *p = result;
            }
            busy.store(false, Ordering::SeqCst);
            ctx.request_repaint();
        });
    }

    fn start_update_download(
        &self,
        ctx: egui::Context,
        version: String,
        url: String,
        name: String,
        digest: Option<String>,
    ) {
        use std::sync::atomic::Ordering;
        if self.update_busy.swap(true, Ordering::SeqCst) {
            return;
        }
        let phase = self.update_phase.clone();
        let busy = self.update_busy.clone();
        if let Ok(mut p) = phase.lock() {
            *p = crate::updater::UpdatePhase::Downloading {
                version: version.clone(),
            };
        }
        ctx.request_repaint();

        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::updater::download_installer(&url, &name, digest.as_deref())
            }))
            .unwrap_or_else(|_| {
                Err("Download crashed unexpectedly. Please try again.".to_string())
            });

            let phase_result = match result {
                Ok(path) => crate::updater::UpdatePhase::Downloaded { version, path },
                Err(e) => crate::updater::UpdatePhase::Error(e),
            };

            if let Ok(mut p) = phase.lock() {
                *p = phase_result;
            }
            busy.store(false, Ordering::SeqCst);
            ctx.request_repaint();
        });
    }

    fn read_text_edit_active(ui: &Ui) -> bool {
        ui.memory(|m| {
            // If nothing is focused in egui, we are definitely not typing.
            // This prevents the flag from getting stuck if a text box is destroyed.
            if m.focused().is_none() {
                return false;
            }
            let slot_active = m
                .data
                .get_temp::<egui::Id>(egui::Id::unique("pejvak_current_editing"))
                .is_some();
            let flag_active = m
                .data
                .get_temp::<bool>(egui::Id::unique("pejvak_text_edit_active"))
                .unwrap_or(false);
            slot_active || flag_active
        })
    }
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
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let gap = HEADER_GAP * scale;
                    let tight = HEADER_TIGHT_GAP * scale;

                    header::theme_toggle_button(ui, t, is_light, p, scale);
                    ui.add_space(tight);
                    header::menu_button(ui, t, scale, p);
                    ui.add_space(gap);

                    presets_ui::preset_menu(ui, t, p, s, scale, current_preset, user_presets);
                    ui.add_space(gap);

                    let (undo_clicked, redo_clicked) = header::history_switch(
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

                    const BYPASS_ORANGE: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x8C, 0x42);
                    const FREEZE_ICE: egui::Color32 = egui::Color32::from_rgb(0x7D, 0xD3, 0xFC);
                    const INTERP_GREEN: egui::Color32 = egui::Color32::from_rgb(0x7D, 0xDB, 0x88);

                    header::header_toggle_group(
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

                    header::engine_switch_header(ui, t, p.ffdn.use_ffdn.value(), p, s, scale);
                    ui.add_space(gap);

                    header::texture_kill_button(ui, t, p, s, scale);
                    ui.add_space(gap);

                    let active_page = pending_page.unwrap_or(*current_page);
                    if let Some(target) = header::header_tabs(ui, t, active_page, scale) {
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
            let body_alpha = (fade * smoothstep(engine_fade)).max(0.05);

            ui.vertical(|ui| {
                ui.set_min_width(body_w);
                ui.set_max_width(body_w);
                ui.multiply_opacity(body_alpha);
                match *current_page {
                    EditorPage::Main => {
                        pages::draw_main_page(ui, p, s, t, scale, is_light, use_ffdn_visual);
                    }
                    EditorPage::Texture => {
                        pages::draw_texture_page(ui, p, s, t, scale);
                    }
                }
                ui.multiply_opacity(1.0);
            });
        });
}