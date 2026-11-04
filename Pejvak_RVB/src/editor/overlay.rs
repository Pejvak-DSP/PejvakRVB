//! Overlays and Modals (About, Update, Disabled).

use super::theme::{Theme, KILL_RED};
use super::ModalPanel;
use egui::{self, Color32, Rect, Sense, Stroke, Ui, Vec2};
use std::sync::{Arc, Mutex};

const LOGO_PNG: &[u8] = include_bytes!("../../../manual/logo.png");

pub fn draw_plugin_disabled_overlay(ui: &mut Ui, t: &Theme, scale: f32) {
    let screen = ui.max_rect();
    let is_light = t.bg_top.r() > 128;

    egui::Area::new(egui::Id::unique("pejvak_plugin_disabled_overlay"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .interactable(true)
        .movable(false)
        .show(ui.ctx(), |ui| {
            ui.set_min_size(screen.size());
            ui.set_max_size(screen.size());
            let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            let painter = ui.painter();
            painter.rect_filled(
                rect,
                0.0,
                Color32::from_black_alpha(if is_light { 175 } else { 200 }),
            );
            painter.text(
                rect.center() + egui::vec2(0.0, -10.0 * scale),
                egui::Align2::CENTER_CENTER,
                "PLUGIN DISABLED",
                egui::FontId::proportional(26.0 * scale),
                KILL_RED,
            );
            painter.text(
                rect.center() + egui::vec2(0.0, 26.0 * scale),
                egui::Align2::CENTER_CENTER,
                "Enable the plugin in your DAW to resume editing.",
                egui::FontId::proportional(12.0 * scale),
                Color32::from_rgb(0xE6, 0xEA, 0xF4),
            );
        });
}

pub fn draw_modal_panel(
    ui: &mut Ui,
    t: &Theme,
    scale: f32,
    update_phase: &Arc<Mutex<crate::updater::UpdatePhase>>,
    update_busy: &Arc<std::sync::atomic::AtomicBool>,
) {
    let modal_id = egui::Id::unique("pejvak_modal_panel");
    let opened_at_id = modal_id.with("opened_at");
    let closing_at_id = modal_id.with("closing_at");

    let panel: Option<ModalPanel> = ui.memory(|m| m.data.get_temp(modal_id));
    let Some(panel) = panel else {
        return;
    };

    const MODAL_FADE_SECS: f64 = 0.22;
    let now = ui.input(|i| i.time);
    let opened_at: f64 = ui.memory(|m| m.data.get_temp(opened_at_id).unwrap_or(0.0));
    let closing_at: f64 = ui.memory(|m| m.data.get_temp(closing_at_id).unwrap_or(0.0));
    let is_closing = closing_at > 0.0;

    let raw = if is_closing {
        ((closing_at - now) / MODAL_FADE_SECS).clamp(0.0, 1.0) as f32
    } else {
        ((now - opened_at) / MODAL_FADE_SECS).clamp(0.0, 1.0) as f32
    };
    let progress = raw * raw * (3.0 - 2.0 * raw);

    if is_closing && progress <= 0.0 {
        ui.memory_mut(|m| {
            m.data.remove::<ModalPanel>(modal_id);
            m.data.remove::<f64>(opened_at_id);
            m.data.remove::<f64>(closing_at_id);
        });
        return;
    }
    if progress < 1.0 {
        ui.ctx().request_repaint();
    }

    let grace_elapsed = !is_closing && (now - opened_at > 0.30);
    let is_light = t.bg_top.r() > 128;
    let screen = ui.max_rect();

    ui.ctx()
        .layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::unique("modal_backdrop"),
        ))
        .rect_filled(
            screen,
            0.0,
            Color32::from_rgba_unmultiplied(
                0,
                0,
                0,
                ((if is_light { 80.0 } else { 140.0 }) * progress).round() as u8,
            ),
        );

    let modal_w = (480.0 * scale).min(screen.width() * 0.68);
    let modal_h = (340.0 * scale).min(screen.height() * 0.82);
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

    egui::Area::new(egui::Id::unique("modal_area"))
        .order(egui::Order::Foreground)
        .fixed_pos(modal_rect.min)
        .show(ui.ctx(), |ui| {
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

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(match panel {
                            ModalPanel::About => "About",
                            ModalPanel::Update => "Check for updates",
                        })
                        .size(14.0 * scale)
                        .strong()
                        .color(t.accent),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (r, r_resp) =
                            ui.allocate_exact_size(Vec2::splat(20.0 * scale), Sense::click());
                        if r_resp.hovered() {
                            ui.painter()
                                .rect_filled(r, 4.0 * scale, t.accent.gamma_multiply(0.20));
                        }
                        let a = r.shrink(6.0 * scale);
                        ui.painter().line_segment(
                            [a.left_top(), a.right_bottom()],
                            Stroke::new(1.5 * scale, body_col),
                        );
                        ui.painter().line_segment(
                            [a.right_top(), a.left_bottom()],
                            Stroke::new(1.5 * scale, body_col),
                        );
                        if r_resp.clicked() {
                            close_clicked = true;
                        }
                    });
                });
                ui.add_space(8.0 * scale);
                ui.separator();
                ui.add_space(8.0 * scale);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match panel {
                        ModalPanel::About => draw_about_body(ui, t, body_col, scale),
                        ModalPanel::Update => {
                            draw_update_body(ui, t, body_col, scale, update_phase, update_busy)
                        }
                    });
            });
        });

    if is_closing {
        return;
    }
    let backdrop_clicked = grace_elapsed
        && ui.input(|i| i.pointer.any_click())
        && ui
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|pos| !modal_rect.contains(pos));
    if close_clicked || backdrop_clicked || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.memory_mut(|m| m.data.insert_temp(closing_at_id, now + MODAL_FADE_SECS));
    }
}

fn draw_about_body(ui: &mut Ui, t: &Theme, body_col: Color32, scale: f32) {
    // Determine the exact physical size the logo will occupy on screen.
    let target_h = 64.0 * scale;
    let pixels_per_point = ui.ctx().pixels_per_point();

    // Load the image to get the aspect ratio first
    if let Ok(img) = image::load_from_memory(LOGO_PNG) {
        let aspect = img.width() as f32 / img.height().max(1) as f32;
        let target_w = target_h * aspect;

        // Convert logical size to physical pixels for 1:1 crisp rendering
        let phys_w = (target_w * pixels_per_point).round().max(1.0) as u32;
        let phys_h = (target_h * pixels_per_point).round().max(1.0) as u32;

        // Cache key includes physical dimensions so the logo re-renders smoothly if window is resized
        let logo_id = egui::Id::unique(format!("pejvak_logo_texture_{}_{}", phys_w, phys_h));

        let logo_texture: Option<egui::TextureHandle> = ui.ctx().data(|d| d.get_temp(logo_id));
        let logo_texture = match logo_texture {
            Some(tex) => Some(tex),
            None => {
                // Pre-scale the image exactly to the physical pixel size using high-quality Lanczos3
                let img = img.resize_exact(phys_w, phys_h, image::imageops::FilterType::Lanczos3);
                let rgba = img.to_rgba8();
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [phys_w as usize, phys_h as usize],
                    rgba.as_raw(),
                );

                let tex_options = egui::TextureOptions {
                    magnification: egui::TextureFilter::Linear,
                    minification: egui::TextureFilter::Linear,
                    wrap_mode: egui::TextureWrapMode::ClampToEdge,
                    mipmap_mode: None,
                };

                let tex = ui.ctx().load_texture("pejvak-logo", color_image, tex_options);
                ui.ctx().data_mut(|d| {
                    d.insert_temp(logo_id, tex.clone());
                });
                Some(tex)
            }
        };

        if let Some(tex) = logo_texture {
            ui.vertical_centered(|ui| {
                ui.add(egui::Image::new((tex.id(), egui::vec2(target_w, target_h))));
                ui.add_space(6.0 * scale);
            });
        }
    }
    
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("PEJVAK   RVB")
                .font(egui::FontId::new(
                    16.0 * scale,
                    egui::FontFamily::Name("Monoton".into()),
                ))
                .color(t.accent),
        );
        ui.add_space(2.0 * scale);
        ui.label(
            egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                .size(9.5 * scale)
                .color(t.dim2),
        );
    });
    ui.add_space(10.0 * scale);

    ui.label(egui::RichText::new("A faithful Rust port of Valdemar Erlingsson's CloudSeed (legacy-v1), with reference to the JUCE-based CloudReverb fork by xunil-cloud. Expanded with a dual engine architecture, a six-layer Texture Rack, Velvet-Noise diffusion, and a drag-and-drop Signal Chain. Both DSP engines run in pure f64 with zero allocation on the audio thread.").size(10.0 * scale).color(body_col));

    ui.add_space(10.0 * scale);
    ui.label(
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
        ui.label(egui::RichText::new(line).size(10.0 * scale).color(body_col));
    }

    ui.add_space(8.0 * scale);
    ui.label(
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
        "• 100 factory presets across 16 categories",
    ] {
        ui.label(egui::RichText::new(line).size(10.0 * scale).color(body_col));
    }

    ui.add_space(8.0 * scale);
    ui.label(
        egui::RichText::new("BUILT WITH")
            .size(9.0 * scale)
            .strong()
            .color(t.dim2),
    );
    ui.add_space(2.0 * scale);
    let _ = ui.label(
        egui::RichText::new("Rust  •  nice-plug 0.4.2  •  egui 0.37  •  wide 1.7.1")
            .size(10.0 * scale)
            .color(body_col),
    );

    ui.add_space(12.0 * scale);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("Pejvak DSP")
                .size(11.0 * scale)
                .strong()
                .color(t.accent),
        );
        ui.add_space(2.0 * scale);
        ui.label(
            egui::RichText::new("Designed & developed by Amin Yousefinejad")
                .size(10.0 * scale)
                .color(body_col),
        );
        ui.add_space(4.0 * scale);
        ui.hyperlink_to(
            egui::RichText::new("pejvak-dsp.github.io")
                .size(10.0 * scale)
                .color(t.accent),
            "https://pejvak-dsp.github.io",
        );
        ui.add_space(4.0 * scale);
        ui.label(
            egui::RichText::new("GPL-3.0-or-later")
                .size(9.5 * scale)
                .color(t.dim2),
        );
    });
}

fn draw_update_body(
    ui: &mut Ui,
    t: &Theme,
    body_col: Color32,
    scale: f32,
    update_phase: &Arc<Mutex<crate::updater::UpdatePhase>>,
    update_busy: &Arc<std::sync::atomic::AtomicBool>,
) {
    use crate::updater::UpdatePhase;
    let phase = update_phase
        .lock()
        .map(|g| g.clone())
        .unwrap_or(UpdatePhase::Idle);
    let busy = update_busy.load(std::sync::atomic::Ordering::Relaxed);

    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("PEJVAK   RVB")
                .font(egui::FontId::new(
                    14.0 * scale,
                    egui::FontFamily::Name("Monoton".into()),
                ))
                .color(t.accent),
        );
        ui.add_space(2.0 * scale);
        ui.label(
            egui::RichText::new(format!("Current version: v{}", env!("CARGO_PKG_VERSION")))
                .size(10.0 * scale)
                .color(t.dim2),
        );
    });
    ui.add_space(12.0 * scale);

    match phase {
        UpdatePhase::Idle => {
            ui.label(
                egui::RichText::new("Press the button below to check for a new version.")
                    .size(11.0 * scale)
                    .color(body_col),
            );
            ui.add_space(10.0 * scale);
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Check now").size(11.0 * scale),
                ))
                .clicked()
                && !busy
            {
                ui.memory_mut(|m| {
                    m.data
                        .insert_temp(egui::Id::unique("pejvak_check_updates_requested"), true)
                });
            }
        }
        UpdatePhase::Checking => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    egui::RichText::new("Checking for updates…")
                        .size(11.0 * scale)
                        .color(body_col),
                );
            });
        }
        UpdatePhase::UpToDate => {
            ui.label(
                egui::RichText::new("You are running the latest version.")
                    .size(12.0 * scale)
                    .color(t.accent),
            );
        }
        UpdatePhase::Available {
            version,
            notes,
            download_url,
            asset_name,
            digest,
        } => {
            ui.label(
                egui::RichText::new(format!("Version v{version} is available"))
                    .size(12.0 * scale)
                    .strong()
                    .color(t.accent),
            );
            ui.add_space(6.0 * scale);
            if !notes.is_empty() {
                ui.label(
                    egui::RichText::new("Release notes:")
                        .size(9.5 * scale)
                        .strong()
                        .color(t.dim2),
                );
                ui.label(
                    egui::RichText::new(notes)
                        .size(10.0 * scale)
                        .color(body_col),
                );
                ui.add_space(6.0 * scale);
            }
            ui.horizontal(|ui| {
                match (download_url, asset_name) {
                    (Some(url), Some(name)) => {
                        if ui
                            .add(egui::Button::new(
                                egui::RichText::new("Download & Install").size(11.0 * scale),
                            ))
                            .clicked()
                            && !busy
                        {
                            ui.memory_mut(|m| {
                                m.data.insert_temp(
                                    egui::Id::unique("pejvak_update_download_request"),
                                    (version.clone(), url.clone(), name.clone(), digest.clone()),
                                )
                            });
                        }
                    }
                    _ => {
                        ui.label(
                            egui::RichText::new(
                                "No installer found for this platform — use the releases page.",
                            )
                            .size(10.0 * scale)
                            .color(t.dim2),
                        );
                    }
                }
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("Open Releases Page").size(11.0 * scale),
                    ))
                    .clicked()
                {
                    crate::updater::open_releases_page();
                }
            });
        }
        UpdatePhase::Downloading { version } => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    egui::RichText::new(format!("Downloading v{version}…"))
                        .size(11.0 * scale)
                        .color(body_col),
                );
            });
        }
        UpdatePhase::Downloaded { version, path } => {
            ui.label(
                egui::RichText::new(format!("v{version} is downloaded and ready"))
                    .size(12.0 * scale)
                    .color(t.accent),
            );
            ui.add_space(6.0 * scale);
            ui.label(
                egui::RichText::new(format!("Saved to: {}", path.display()))
                    .size(9.5 * scale)
                    .color(t.dim2),
            );
            ui.add_space(8.0 * scale);
            ui.label(egui::RichText::new("Close your DAW before running the installer. The plugin file is locked while the DAW is open.").size(10.0 * scale).color(t.dim2));
            ui.add_space(8.0 * scale);
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Open Installer").size(11.0 * scale),
                ))
                .clicked()
            {
                if let Err(e) = crate::updater::launch_installer(&path) {
                    ui.colored_label(Color32::from_rgb(0xEF, 0x44, 0x44), e);
                }
            }
        }
        UpdatePhase::Error(e) => {
            ui.colored_label(
                Color32::from_rgb(0xEF, 0x44, 0x44),
                egui::RichText::new(format!("Update check failed: {e}")).size(11.0 * scale),
            );
        }
    }
}
