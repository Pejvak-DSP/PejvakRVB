//! Preset browser UI.

use super::theme::Theme;
use super::widgets::*;
use super::xml::*;
use crate::presets::{CATEGORIES, FACTORY_PRESETS};
use crate::PejvakRvbParams;
use egui::{self, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use nice_plug::prelude::ParamSetter;
use std::sync::Arc;

pub fn preset_menu(
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
    let trigger_id = egui::Id::unique("pejvak_preset_trigger");
    let state_id = trigger_id.with("state");
    let opened_at_id = trigger_id.with("opened_at");
    let closing_at_id = trigger_id.with("closing_at");

    const PRESET_FADE_SECS: f64 = 0.14;
    let mut popup_open: bool = ui.memory(|m| m.data.get_temp(state_id).unwrap_or(false));
    let now = ui.input(|i| i.time);
    let opened_at: f64 = ui.memory(|m| m.data.get_temp(opened_at_id).unwrap_or(now));
    let closing_at: f64 = ui.memory(|m| m.data.get_temp(closing_at_id).unwrap_or(0.0));
    let is_closing = closing_at > 0.0;

    let raw = if is_closing {
        ((closing_at - now) / PRESET_FADE_SECS).clamp(0.0, 1.0) as f32
    } else {
        ((now - opened_at) / PRESET_FADE_SECS).clamp(0.0, 1.0) as f32
    };
    let popup_fade = raw * raw * (3.0 - 2.0 * raw);

    if is_closing && popup_fade <= 0.0 {
        ui.memory_mut(|m| {
            m.data.insert_temp(state_id, false);
            m.data.remove::<f64>(opened_at_id);
            m.data.remove::<f64>(closing_at_id);
        });
        popup_open = false;
    }
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
        if is_closing { /* Ignore */
        } else if popup_open {
            ui.memory_mut(|m| m.data.insert_temp(closing_at_id, now + PRESET_FADE_SECS));
        } else {
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
    }
    if next_resp.clicked() {
        let cur_idx = FACTORY_PRESETS
            .iter()
            .position(|fp| fp.name == current.as_str())
            .unwrap_or(0);
        let new_idx = (cur_idx + 1) % FACTORY_PRESETS.len();
        apply_factory_preset(p, s, &FACTORY_PRESETS[new_idx]);
        *current = FACTORY_PRESETS[new_idx].name.to_string();
    }

    let cur_cat = FACTORY_PRESETS
        .iter()
        .find(|fp| fp.name == current.as_str())
        .map(|fp| fp.category)
        .unwrap_or(0);
    let cur_accent = CATEGORIES[cur_cat].color;

    if ui.is_rect_visible(prev_rect) {
        let painter = ui.painter();
        painter.rect_filled(prev_rect, 5.0 * scale, t.track_bg.gamma_multiply(0.6));
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(
                    prev_rect.center().x + 3.0 * scale,
                    prev_rect.center().y - 4.0 * scale,
                ),
                Pos2::new(
                    prev_rect.center().x + 3.0 * scale,
                    prev_rect.center().y + 4.0 * scale,
                ),
                Pos2::new(prev_rect.center().x - 3.0 * scale, prev_rect.center().y),
            ],
            if prev_resp.hovered() { t.accent } else { t.dim },
            Stroke::NONE,
        ));
    }

    if ui.is_rect_visible(trigger_rect) {
        let painter = ui.painter();
        painter.rect_filled(trigger_rect, 5.0 * scale, t.track_bg.gamma_multiply(0.6));
        painter.circle_filled(
            Pos2::new(trigger_rect.left() + 12.0 * scale, trigger_rect.center().y),
            3.5 * scale,
            cur_accent,
        );
        painter.text(
            Pos2::new(trigger_rect.left() + 24.0 * scale, trigger_rect.center().y),
            egui::Align2::LEFT_CENTER,
            if current.is_empty() {
                "Presets"
            } else {
                current.as_str()
            },
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
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(
                    next_rect.center().x - 3.0 * scale,
                    next_rect.center().y - 4.0 * scale,
                ),
                Pos2::new(
                    next_rect.center().x - 3.0 * scale,
                    next_rect.center().y + 4.0 * scale,
                ),
                Pos2::new(next_rect.center().x + 3.0 * scale, next_rect.center().y),
            ],
            if next_resp.hovered() { t.accent } else { t.dim },
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
        egui::Color32::from_rgb(0xF6, 0xF7, 0xFA)
    } else {
        egui::Color32::from_rgb(0x1A, 0x21, 0x36)
    };

    let search_id = trigger_id.with("search");
    let mut search_text: String = ui.memory(|m| m.data.get_temp(search_id).unwrap_or_default());

    let area_response = egui::Area::new(trigger_id.with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(popup_pos)
        .show(ui.ctx(), |ui| {
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
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 50)
                    } else {
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 110)
                    },
                })
                .inner_margin(egui::Margin::same(6));

            frame
                .show(ui, |ui| {
                    ui.set_min_size(Vec2::new(popup_w - 12.0 * scale, popup_h - 12.0 * scale));
                    ui.set_max_size(Vec2::new(popup_w - 12.0 * scale, popup_h - 12.0 * scale));
                    let mut action: Option<Action> = None;

                    let base_text = if is_light {
                        egui::Color32::from_rgb(0x10, 0x12, 0x18)
                    } else {
                        t.txt
                    };
                    ui.visuals_mut().override_text_color = Some(base_text);

                    let search_bg = if is_light {
                        egui::Color32::from_rgb(0xE0, 0xE3, 0xE8)
                    } else {
                        egui::Color32::from_rgb(0x0D, 0x12, 0x23)
                    };
                    let search_fg = if is_light {
                        egui::Color32::from_rgb(0x10, 0x12, 0x18)
                    } else {
                        t.txt
                    };
                    let search_hint = if is_light {
                        egui::Color32::from_rgb(0x88, 0x8E, 0x98)
                    } else {
                        t.dim2
                    };

                    ui.add_space(2.0 * scale);
                    let search_resp = ui.add(
                        egui::TextEdit::singleline(&mut search_text)
                            .id(egui::Id::unique(search_id))
                            .hint_text(egui::RichText::new("Search presets…").color(search_hint))
                            .font(egui::FontId::proportional(11.5 * scale))
                            .text_color(search_fg)
                            .background_color(search_bg)
                            .desired_width(f32::INFINITY),
                    );

                    if search_resp.gained_focus() {
                        ui.memory_mut(|m| {
                            m.data
                                .insert_temp(egui::Id::unique("pejvak_text_edit_active"), true)
                        });
                    }
                    if search_resp.lost_focus() {
                        ui.memory_mut(|m| {
                            if m.focused().is_none() {
                                m.data
                                    .insert_temp(egui::Id::unique("pejvak_text_edit_active"), false);
                            }
                        });
                    }
                    ui.add_space(2.0 * scale);
                    ui.separator();

                    let search_lower = search_text.trim().to_lowercase();

                    {
                        let init_selected = current.as_str() == "Init";
                        if preset_entry(ui, is_light, "Init", cur_accent, init_selected, scale)
                            .clicked()
                        {
                            action = Some(Action::Apply(0));
                        }
                        if preset_entry(ui, is_light, "Save Preset…", t.dim, false, scale).clicked()
                        {
                            action = Some(Action::Save);
                        }
                        if preset_entry(ui, is_light, "Load Preset…", t.dim, false, scale).clicked()
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
                                category_header(ui, cat.name, cat.description, cat.color, scale);
                                for (idx, fp) in items {
                                    if preset_entry(
                                        ui,
                                        is_light,
                                        fp.name,
                                        cat.color,
                                        current.as_str() == fp.name,
                                        scale,
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
                                    if preset_entry(
                                        ui,
                                        is_light,
                                        name,
                                        t.accent,
                                        current.as_str() == name.as_str(),
                                        scale,
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
                        apply_cloudseed_partial(p, s, &parsed.cloudseed);
                        apply_texture_params(p, s, &parsed.texture, &parsed.enabled);
                        if let Some(order) = parsed.texture_order {
                            apply_texture_order(p, order);
                        }
                        if let Some(eq) = parsed.eq_enabled {
                            apply_eq_master(p, s, eq);
                        }
                        apply_ffdn_params(p, s, &parsed.ffdn);
                        apply_illusion_params(
                            p,
                            s,
                            &parsed.illusion,
                            parsed.illusion_enabled,
                            parsed.illusion_seen,
                        );
                        *current = name;
                    }
                }
            }
        }
        ui.memory_mut(|m| m.data.insert_temp(closing_at_id, now + PRESET_FADE_SECS));
    }

    let popup_rect = Rect::from_min_size(popup_pos, Vec2::new(popup_w, popup_h));
    if !is_closing && ui.input(|i| i.pointer.any_click()) {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            if !popup_rect.contains(pos)
                && !trigger_rect.contains(pos)
                && !prev_rect.contains(pos)
                && !next_rect.contains(pos)
            {
                ui.memory_mut(|m| m.data.insert_temp(closing_at_id, now + PRESET_FADE_SECS));
            }
        }
    }
}

fn preset_entry(
    ui: &mut Ui,
    is_light: bool,
    label: &str,
    accent: egui::Color32,
    selected: bool,
    scale: f32,
) -> egui::Response {
    let h = 22.0 * scale;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click());
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
        egui::Color32::TRANSPARENT
    };
    if bg != egui::Color32::TRANSPARENT {
        painter.rect_filled(rect, 4.0 * scale, bg);
    }

    if selected {
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left() + 1.0, rect.top() + 3.0),
                Vec2::new(2.5 * scale, rect.height() - 6.0),
            ),
            1.0,
            accent,
        );
    }
    painter.circle_filled(
        Pos2::new(rect.left() + 14.0 * scale, rect.center().y),
        3.0 * scale,
        accent,
    );

    let text_col = if selected {
        accent
    } else if is_light {
        egui::Color32::from_rgb(0x1D, 0x21, 0x29)
    } else {
        egui::Color32::from_rgb(0xE6, 0xEA, 0xF4)
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

fn category_header(ui: &mut Ui, name: &str, description: &str, color: egui::Color32, scale: f32) {
    let h = 26.0 * scale;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter();
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(rect.left() + 2.0, rect.top() + 5.0),
            Vec2::new(3.0 * scale, rect.height() - 10.0),
        ),
        1.5 * scale,
        color,
    );
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
