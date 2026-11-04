//! Header UI elements (tabs, toggles, menu).

use super::theme::{lerp_color, Theme, KILL_RED};
use super::widgets::*;
use super::*;
use crate::PejvakRvbParams;
use egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use nice_plug::prelude::{Param, ParamSetter};
use std::f32::consts::PI;

pub const UI_SCALE_PRESETS: &[(u32, &str)] = &[
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
    (250, "250 %"),
    (300, "300 %"),
];

pub fn draw_sun_icon(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    painter.circle_filled(center, radius * 0.40, color);
    let ray_stroke = Stroke::new(radius * 0.14, color);
    for i in 0..8 {
        let a = i as f32 * (PI * 2.0 / 8.0);
        let dir = Vec2::new(a.cos(), a.sin());
        painter.line_segment(
            [
                center + dir * (radius * 0.60),
                center + dir * (radius * 0.94),
            ],
            ray_stroke,
        );
    }
}

pub fn draw_moon_icon(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    color: Color32,
    bg: Color32,
) {
    painter.circle_filled(center, radius, color);
    painter.circle_filled(
        center + Vec2::new(radius * 0.60, -radius * 0.10),
        radius * 0.95,
        bg,
    );
}

#[derive(Clone, Copy)]
pub enum HistoryIcon {
    Undo,
    Redo,
}

pub fn draw_history_icon(
    painter: &egui::Painter,
    center: Pos2,
    icon: HistoryIcon,
    color: Color32,
    scale: f32,
) {
    let stroke = Stroke::new(1.4 * scale, color);
    let flip = match icon {
        HistoryIcon::Undo => 1.0,
        HistoryIcon::Redo => -1.0,
    };
    let svg_scale = scale * 19.0 / 24.0;
    let to_screen = |x: f32, y: f32| -> Pos2 {
        Pos2::new(
            center.x + flip * (x - 12.0) * svg_scale,
            center.y + (y - 12.0) * svg_scale,
        )
    };

    let ah_top = to_screen(7.125, 3.75);
    let ah_tip = to_screen(3.0, 7.875);
    let ah_bot = to_screen(7.125, 12.0);
    painter.add(egui::Shape::convex_polygon(
        vec![ah_top, ah_tip, ah_bot],
        color,
        egui::Stroke::NONE,
    ));

    let mut pts: Vec<Pos2> = Vec::with_capacity(24);
    pts.push(to_screen(3.0, 7.875));
    pts.push(to_screen(14.25, 7.875));
    for i in 0..=20 {
        let t = i as f32 / 20.0;
        let angle = -std::f32::consts::FRAC_PI_2 + t * std::f32::consts::PI;
        pts.push(to_screen(
            14.25 + 4.875 * angle.cos(),
            12.75 + 4.875 * angle.sin(),
        ));
    }
    pts.push(to_screen(9.0, 17.625));
    painter.add(egui::Shape::line(pts, stroke));
}

pub fn history_switch(
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

    let (outer_rect, outer_resp) =
        ui.allocate_exact_size(Vec2::new(total_w, total_h), Sense::click());
    let r_undo = Rect::from_min_size(
        Pos2::new(outer_rect.left() + outer_pad, outer_rect.top() + outer_pad),
        Vec2::new(seg_w, seg_h),
    );
    let r_redo = Rect::from_min_size(
        Pos2::new(r_undo.right(), outer_rect.top() + outer_pad),
        Vec2::new(seg_w, seg_h),
    );

    let hover_pos = ui.ctx().pointer_hover_pos();
    let hover_undo = hover_pos.is_some_and(|p| r_undo.contains(p));
    let hover_redo = hover_pos.is_some_and(|p| r_redo.contains(p));
    let show_popups = ui.memory(|m| {
        m.data
            .get_temp::<bool>(egui::Id::unique("pejvak_show_popups"))
            .unwrap_or(true)
    });

    if (hover_undo || hover_redo) && show_popups {
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
            let galley =
                ui.painter()
                    .layout_no_wrap(text, egui::FontId::proportional(11.0 * scale), t.txt);
            let pad = egui::Vec2::new(8.0 * scale, 5.0 * scale);
            let tip_rect = egui::Rect::from_min_size(
                ptr + egui::Vec2::new(12.0 * scale, 14.0 * scale),
                galley.size() + pad * 2.0,
            );
            let tip_painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::unique("history_tip"),
            ));
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
    painter.rect_filled(outer_rect, radius, t.track_bg.gamma_multiply(0.55));

    if hover_undo {
        painter.rect_filled(
            r_undo,
            inner_radius,
            t.accent
                .gamma_multiply(if undo_available { 0.14 } else { 0.05 }),
        );
    }
    if hover_redo {
        painter.rect_filled(
            r_redo,
            inner_radius,
            t.accent
                .gamma_multiply(if redo_available { 0.14 } else { 0.05 }),
        );
    }

    draw_history_icon(
        painter,
        r_undo.center(),
        HistoryIcon::Undo,
        if !undo_available {
            t.dim2.gamma_multiply(0.55)
        } else if hover_undo {
            t.accent
        } else {
            t.txt
        },
        scale * 0.90,
    );
    draw_history_icon(
        painter,
        r_redo.center(),
        HistoryIcon::Redo,
        if !redo_available {
            t.dim2.gamma_multiply(0.55)
        } else if hover_redo {
            t.accent
        } else {
            t.txt
        },
        scale * 0.90,
    );

    (clicked_undo, clicked_redo)
}

pub fn theme_toggle_button(
    ui: &mut Ui,
    t: &Theme,
    is_light: bool,
    p: &std::sync::Arc<PejvakRvbParams>,
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
        draw_moon_icon(painter, rect.center(), icon_r, icon_col, bg_col);
    } else {
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

pub fn engine_switch_header(
    ui: &mut Ui,
    t: &Theme,
    use_ffdn: bool,
    p: &std::sync::Arc<PejvakRvbParams>,
    s: &ParamSetter,
    scale: f32,
) {
    let labels = ["CLOUD", "FFDN"];
    let font = egui::FontId::proportional(11.5 * scale);
    let pad_x = 12.0 * scale;
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

    let mut hovered = [false, false];
    let mut clicked: Option<bool> = None;
    for (i, seg_rect) in seg_rects.iter().enumerate() {
        let resp = ui.interact(
            *seg_rect,
            egui::Id::unique(("engine_switch_header_hit", labels[i])),
            Sense::click(),
        );
        if resp.clicked() {
            clicked = Some(i == 1);
        }
        hovered[i] = resp.hovered();
        let _ = hover_tip(
            resp,
            match i {
                0 => "Use the CloudSeed reverb engine",
                _ => "Use the FFDN engine",
            },
        );
    }

    let effective_use_ffdn = clicked.unwrap_or(use_ffdn);
    let anim = ui.ctx().animate_bool_with_time(
        egui::Id::unique("engine_switch_header_anim"),
        effective_use_ffdn,
        0.28,
    );

    let painter = ui.painter();
    painter.rect_filled(outer_rect, radius, t.track_bg.gamma_multiply(0.55));
    let inner_radius = radius - outer_pad * 0.4;

    for (i, seg_rect) in seg_rects.iter().enumerate() {
        if hovered[i] && (i == 1) != effective_use_ffdn {
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
        let active_amount = (1.0 - (anim - i as f32).abs()).clamp(0.0, 1.0);
        painter.text(
            seg_rect.center(),
            egui::Align2::CENTER_CENTER,
            labels[i],
            font.clone(),
            lerp_color(t.dim, t.accent, active_amount),
        );
    }
    if anim > 0.001 && anim < 0.999 {
        ui.ctx().request_repaint();
    }

    if let Some(target) = clicked {
        if target != use_ffdn {
            s.begin_set_parameter(&p.ffdn.use_ffdn);
            s.set_parameter(&p.ffdn.use_ffdn, target);
            s.end_set_parameter(&p.ffdn.use_ffdn);
            ui.ctx().request_repaint();
        }
    }
}

pub fn texture_kill_button(
    ui: &mut Ui,
    t: &Theme,
    p: &std::sync::Arc<PejvakRvbParams>,
    s: &ParamSetter,
    scale: f32,
) {
    let size = 30.0 * scale;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());

    let kill_id = egui::Id::unique("tx_kill_state");
    let saved: Option<[bool; 7]> = ui.memory(|m| m.data.get_temp(kill_id));
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
    let kill_active = saved.is_some() && !any_enabled;

    ui.memory_mut(|m| {
        m.data
            .insert_temp(egui::Id::unique("pejvak_tx_kill_active"), kill_active)
    });

    let now = ui.ctx().input(|i| i.time);
    let commit_at: f64 = ui.memory(|m| {
        m.data
            .get_temp(egui::Id::unique("pejvak_force_undo_commit_at"))
            .unwrap_or(0.0)
    });
    let commit_pending = commit_at > 0.0 && now < commit_at;

    if resp.clicked() && !commit_pending {
        if kill_active {
            if let Some(saved) = saved {
                set_all_texture_enabled(s, p, saved);
            }
            ui.memory_mut(|m| m.data.remove::<[bool; 7]>(kill_id));
        } else {
            // Remove the `any_enabled` check so the kill switch can be engaged
            // even if all texture layers are already off.
            ui.memory_mut(|m| m.data.insert_temp(kill_id, current));
            set_all_texture_enabled(s, p, [false; 7]);
            ui.memory_mut(|m| {
                m.data
                    .insert_temp(egui::Id::unique("pejvak_force_undo_commit_at"), now + 0.10)
            });
        }
    }

    let hover = resp.hovered();
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if kill_active {
            painter.rect_filled(rect, 6.0 * scale, KILL_RED.gamma_multiply(0.10));
            let inner_rect = rect.shrink(3.0 * scale);
            let pulse =
                ((ui.ctx().input(|i| i.time) as f32 * 7.0).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            painter.rect_filled(
                inner_rect,
                4.0 * scale,
                KILL_RED.gamma_multiply(0.20 + pulse * 0.25),
            );
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

    let _ = hover_tip(
        resp,
        if kill_active {
            "Release the lock and restore the previous Texture layers"
        } else {
            "Lock all Texture layers off (blocks UI toggles)"
        },
    );
    if kill_active || hover {
        ui.ctx().request_repaint();
    }
}

fn set_all_texture_enabled(s: &ParamSetter, p: &std::sync::Arc<PejvakRvbParams>, state: [bool; 7]) {
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

pub fn menu_button(ui: &mut Ui, t: &Theme, scale: f32, p: &std::sync::Arc<PejvakRvbParams>) {
    let size = 30.0 * scale;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let id = resp.id;
    let open_id = id.with("open");
    let modal_id = egui::Id::unique("pejvak_modal_panel");

    let mut open: bool = ui.memory(|m| m.data.get_temp(open_id).unwrap_or(false));
    if resp.clicked() {
        open = !open;
    }

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
        for i in 0..3 {
            let y = rect.center().y + (i as f32 - 1.0) * line_gap;
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(rect.center().x, y), Vec2::new(line_w, line_h)),
                line_h * 0.5,
                icon_col,
            );
        }
    }

    let _ = hover_tip(resp, "Plugin menu");
    ui.memory_mut(|m| m.data.insert_temp(open_id, open));

    if !open {
        return;
    }

    let popup_w = 220.0 * scale;
    let row_h = 26.0 * scale;
    let pad = 6.0 * scale;
    let items = [
        "Manual",
        "Size",
        "Check for updates",
        "Show pop-ups",
        "About",
    ];
    let popup_h = row_h * items.len() as f32 + pad * 2.0;

    let popup_pos = Pos2::new((rect.right() - popup_w).max(4.0), rect.bottom() + 4.0);
    let popup_rect = Rect::from_min_size(popup_pos, Vec2::new(popup_w, popup_h));
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
                        .get_temp::<bool>(egui::Id::unique("pejvak_show_popups"))
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
                        ui.painter().add(egui::Shape::convex_polygon(
                            vec![
                                Pos2::new(arrow_c.x - 2.0 * scale, arrow_c.y - 4.0 * scale),
                                Pos2::new(arrow_c.x - 2.0 * scale, arrow_c.y + 4.0 * scale),
                                Pos2::new(arrow_c.x + 3.0 * scale, arrow_c.y),
                            ],
                            if r_resp.hovered() { t.accent } else { t.dim2 },
                            Stroke::NONE,
                        ));
                    } else if is_toggle_row {
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
                            let new_val = !show_popups_now;
                            p.show_popups
                                .store(new_val, std::sync::atomic::Ordering::Relaxed);
                            ui.memory_mut(|m| {
                                m.data
                                    .insert_temp(egui::Id::unique("pejvak_show_popups"), new_val)
                            });
                            // Trigger the standard menu closing logic.
                            clicked_item = Some(i);
                        }
                    } else if r_resp.clicked() {
                        clicked_item = Some(i);
                    }
                }
            });
        });

    let sub_w = 150.0 * scale;
    let sub_row_h = 24.0 * scale;
    let sub_pad = 6.0 * scale;
    let n_presets = UI_SCALE_PRESETS.len();
    // Changed from 1.8 to 2.5 to provide enough room for the separator and 
    // the final "Set as default size" button, preventing accidental mouse leave.
    let sub_h = sub_row_h * (n_presets as f32 + 2.5) + sub_pad * 2.0;
    let preferred_x = popup_pos.x + popup_w + 4.0 * scale;
    let screen = ui.ctx().viewport_rect();
    let sub_x = if preferred_x + sub_w > screen.right() - 4.0 {
        (popup_pos.x - sub_w - 4.0 * scale).max(4.0)
    } else {
        preferred_x
    };
    let sub_pos = Pos2::new(sub_x, size_row_rect.top());
    let sub_rect = Rect::from_min_size(sub_pos, Vec2::new(sub_w, sub_h));

    let safe_zone = popup_rect.union(sub_rect);
    let pointer_in_safe_zone = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| safe_zone.contains(p));
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
                    ui.add_space(2.0 * scale); ui.separator(); ui.add_space(2.0 * scale);
                    let (r, r_resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), sub_row_h), Sense::click());
                    if r_resp.hovered() { ui.painter().rect_filled(r, 4.0 * scale, t.accent.gamma_multiply(0.15)); }
                    ui.painter().text(Pos2::new(r.left() + 10.0 * scale, r.center().y), egui::Align2::LEFT_CENTER, "Set as default size", egui::FontId::proportional(10.5 * scale), if r_resp.hovered() { t.accent } else { base_col });
                    if r_resp.clicked() { set_default_clicked = true; }
                });
            });
    }

    ui.memory_mut(|m| m.data.insert_temp(size_open_id, show_submenu));

    if let Some(idx) = clicked_item {
        ui.memory_mut(|m| {
            m.data.insert_temp(open_id, false);
            m.data.insert_temp(size_open_id, false);
        });
        let now = ui.input(|i| i.time);
        match idx {
            0 => {
                ui.memory_mut(|m| {
                    m.data
                        .insert_temp(egui::Id::unique("pejvak_manual_open_pending"), true);
                });
            }
            2 => {
                ui.memory_mut(|m| {
                    m.data
                        .insert_temp(egui::Id::unique("pejvak_check_updates_requested"), true)
                });
                ui.memory_mut(|m| {
                    m.data.insert_temp(modal_id, super::ModalPanel::Update);
                    m.data.insert_temp(modal_id.with("opened_at"), now);
                    m.data.remove::<f64>(modal_id.with("closing_at"));
                });
            }
            4 => {
                ui.memory_mut(|m| {
                    m.data.insert_temp(modal_id, super::ModalPanel::About);
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
    if set_default_clicked {
        let screen = ui.ctx().viewport_rect();
        let w = screen.width().round().max(200.0) as u32;
        let h = screen.height().round().max(120.0) as u32;
        if let Ok(mut size) = p.editor_size.lock() {
            *size = (w, h);
        }
        
        // Save globally so it persists across DAW restarts without requiring project save.
        if let Some(dir) = super::xml::user_presets_dir() {
            let path = dir.join("window_size.cfg");
            let _ = std::fs::write(path, format!("{}x{}", w, h));
        }
        
        ui.memory_mut(|m| { m.data.insert_temp(open_id, false); m.data.insert_temp(size_open_id, false); });
    }
}

pub fn header_tabs(
    ui: &mut Ui,
    t: &Theme,
    active_page: EditorPage,
    scale: f32,
) -> Option<EditorPage> {
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

    let sel_anim = ui.ctx().animate_bool_with_time(
        egui::Id::unique("header_tabs_selection"),
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

    let mut clicked: Option<EditorPage> = None;
    let mut hovered = [false, false];
    for (i, seg_rect) in seg_rects.iter().enumerate() {
        let resp = ui.interact(
            *seg_rect,
            egui::Id::unique(("header_tab_hit", labels[i])),
            Sense::click(),
        );
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

    for (i, seg_rect) in seg_rects.iter().enumerate() {
        let sel_index = match active_page {
            EditorPage::Main => 0,
            EditorPage::Texture => 1,
        };
        if hovered[i] && i != sel_index {
            painter.rect_filled(*seg_rect, inner_radius, tab_blue.gamma_multiply(0.10));
        }
    }

    painter.rect_filled(highlighted, inner_radius, tab_blue.gamma_multiply(0.28));
    for (i, seg_rect) in seg_rects.iter().enumerate() {
        let active_amount = (1.0 - (sel_anim - i as f32).abs()).clamp(0.0, 1.0);
        painter.text(
            seg_rect.center(),
            egui::Align2::CENTER_CENTER,
            labels[i],
            font.clone(),
            lerp_color(t.dim, tab_blue, active_amount),
        );
    }
    if sel_anim > 0.001 && sel_anim < 0.999 {
        ui.ctx().request_repaint();
    }
    clicked
}

pub fn header_toggle_group<P: Param>(
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

    let mut hovered: Vec<bool> = vec![false; items.len()];
    let mut on_state: Vec<bool> = vec![false; items.len()];
    for (i, (label, param, _)) in items.iter().enumerate() {
        on_state[i] = param.unmodulated_normalized_value() > 0.5;
        let resp = ui.interact(
            seg_rects[i],
            egui::Id::unique(("header_toggle_group_hit", param.name())),
            Sense::click(),
        );
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

    let painter = ui.painter();
    painter.rect_filled(outer_rect, radius, t.track_bg.gamma_multiply(0.55));
    let inner_radius = radius - outer_pad * 0.4;
    let is_light = t.bg_top.r() > 128;

    for (i, (_label, _param, accent)) in items.iter().enumerate() {
        if hovered[i] && !on_state[i] {
            painter.rect_filled(
                seg_rects[i],
                inner_radius,
                accent.gamma_multiply(if is_light { 0.18 } else { 0.08 }),
            );
        }
    }

    let bg_mul = if is_light { 0.55 } else { 0.22 };
    for (i, (label, param, accent)) in items.iter().enumerate() {
        let seg_rect = seg_rects[i];
        let on = on_state[i];
        let is_bypass_on = *label == "Bypass" && on;
        let (pulse_bg, pulse_text) = if is_bypass_on {
            let wave =
                (ui.ctx().input(|i| i.time) as f32 * std::f32::consts::TAU * 1.5).sin() * 0.5 + 0.5;
            (0.35 + wave * 1.30, 0.50 + wave * 0.50)
        } else {
            (1.0, 1.0)
        };

        if is_bypass_on {
            ui.ctx().request_repaint();
        }

        let text_accent = if is_light {
            accent.gamma_multiply(0.70)
        } else {
            *accent
        };
        let anim = ui.ctx().animate_bool_with_time(
            egui::Id::unique(("header_toggle_group_anim", param.name())),
            on,
            0.28,
        );

        if anim > 0.01 {
            painter.rect_filled(
                seg_rect,
                inner_radius,
                accent.gamma_multiply((bg_mul * anim * pulse_bg).min(1.0)),
            );
        }
        painter.text(
            seg_rect.center(),
            egui::Align2::CENTER_CENTER,
            *label,
            font.clone(),
            lerp_color(t.dim, text_accent, (anim * pulse_text).min(1.0)),
        );
    }

    let any_animating = items.iter().any(|(_, param, _)| {
        let a = ui.ctx().animate_bool_with_time(
            egui::Id::unique(("header_toggle_group_anim", param.name())),
            param.unmodulated_normalized_value() > 0.5,
            0.28,
        );
        a > 0.001 && a < 0.999
    });
    if any_animating {
        ui.ctx().request_repaint();
    }
}
