//! Reusable UI components and interaction handlers.

use super::theme::{lerp_color, Theme, KILL_RED};
use super::*;
use egui::{self, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use nice_plug::prelude::{Param, ParamSetter};

#[inline]
pub fn hover_tip(resp: egui::Response, text: impl Into<String>) -> egui::Response {
    let show = resp.ctx.memory(|m| {
        m.data
            .get_temp::<bool>(egui::Id::unique("pejvak_show_popups"))
            .unwrap_or(true)
    });
    if show {
        resp.on_hover_text(text.into())
    } else {
        resp
    }
}

pub fn handle_drag<P: Param>(
    ui: &mut Ui,
    response: &egui::Response,
    param: &P,
    setter: &ParamSetter,
) {
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

pub fn handle_scroll<P: Param>(
    ui: &mut Ui,
    response: &egui::Response,
    param: &P,
    setter: &ParamSetter,
) {
    if !response.hovered() {
        return;
    }
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

pub fn handle_double_click<P: Param>(
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

pub fn knob_row(ui: &mut Ui, scale: f32, count: usize, add: impl FnOnce(&mut Ui)) {
    let gap = ROW_INNER_GAP * scale;
    let row_w = KNOB_SLOT_W * scale * count as f32 + gap * (count.saturating_sub(1)) as f32;
    let pad = ((ui.available_width() - row_w) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.add_space(pad);
        add(ui);
    });
}

pub fn knob_row_compact(ui: &mut Ui, scale: f32, count: usize, add: impl FnOnce(&mut Ui)) {
    let gap = ROW_INNER_GAP * scale;
    let row_w = KNOB_SMALL_SLOT_W * scale * count as f32 + gap * (count.saturating_sub(1)) as f32;
    let pad = ((ui.available_width() - row_w) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.add_space(pad);
        add(ui);
    });
}

pub fn fader_row(ui: &mut Ui, scale: f32, count: usize, add: impl FnOnce(&mut Ui)) {
    let gap = ROW_INNER_GAP * scale;
    let row_w = FADER_SLOT_W * scale * count as f32 + gap * (count.saturating_sub(1)) as f32;
    let pad = ((ui.available_width() - row_w) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.add_space(pad);
        add(ui);
    });
}

pub fn numbox_row<P: Param>(
    ui: &mut Ui,
    scale: f32,
    items: &[(&str, &P, &str)],
    setter: &ParamSetter,
    accent: egui::Color32,
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

#[allow(clippy::too_many_arguments)]
pub fn switch_row_at_columns<P: Param>(
    ui: &mut Ui,
    scale: f32,
    num_columns: usize,
    column_w: f32,
    items: &[(usize, &str, &P)],
    setter: &ParamSetter,
    accent: egui::Color32,
    theme: &Theme,
) {
    let gap = ROW_INNER_GAP * scale;
    let slot_w = column_w * scale;
    let row_w = slot_w * num_columns as f32 + gap * (num_columns.saturating_sub(1)) as f32;
    let pad = ((ui.available_width() - row_w) * 0.5).max(0.0);
    let slot_h = 22.0 * scale;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.add_space(pad);
        let mut iter = items.iter().peekable();
        for col in 0..num_columns {
            let is_at_this_col = iter.peek().map(|(c, _, _)| *c == col).unwrap_or(false);
            let item = if is_at_this_col { iter.next() } else { None };
            let (slot_rect, _) = ui.allocate_exact_size(Vec2::new(slot_w, slot_h), Sense::hover());
            if let Some((_, label, param)) = item {
                let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(slot_rect));
                child_ui.vertical_centered(|ui| {
                    switch_tiny(ui, theme, label, *param, setter, accent, scale);
                });
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub fn knob<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    label: &str,
    param: &P,
    setter: &ParamSetter,
    tooltip: &str,
    accent: egui::Color32,
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

#[allow(clippy::too_many_arguments)]
pub fn knob_compact<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    label: &str,
    param: &P,
    setter: &ParamSetter,
    tooltip: &str,
    accent: egui::Color32,
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

#[allow(clippy::too_many_arguments)]
pub fn fader<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    label: &str,
    param: &P,
    setter: &ParamSetter,
    tooltip: &str,
    accent: egui::Color32,
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

#[allow(clippy::too_many_arguments)]
pub fn numbox<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    label: &str,
    param: &P,
    setter: &ParamSetter,
    _accent: egui::Color32,
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

                let id = egui::Id::unique(("numbox", param.name()));
                let val_str = param.to_string();
                let slot_id = egui::Id::unique("pejvak_current_editing");
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
                            let cur = param.unmodulated_normalized_value();
                            let new_val = (cur + direction as f32 * 0.005).clamp(0.0, 1.0);
                            setter.begin_set_parameter(param);
                            setter.set_parameter_normalized(param, new_val);
                            setter.end_set_parameter(param);
                        }
                    };

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

                    {
                        let drag_accum_id = resp.id.with("drag_accum");
                        if resp.dragged() {
                            let mut accum: f32 =
                                ui.memory(|m| m.data.get_temp(drag_accum_id).unwrap_or(0.0));
                            accum -= resp.drag_delta().y;
                            const DRAG_THRESHOLD: f32 = 12.0;
                            while accum.abs() >= DRAG_THRESHOLD {
                                let direction = if accum > 0.0 { 1 } else { -1 };
                                apply_step(direction);
                                accum -= direction as f32 * DRAG_THRESHOLD;
                            }
                            ui.memory_mut(|m| m.data.insert_temp(drag_accum_id, accum));
                        } else {
                            ui.memory_mut(|m| m.data.remove::<f32>(drag_accum_id));
                        }
                    }

                    if resp.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                    }

                    let hovered = resp.hovered();
                    if resp.clicked() {
                        ui.memory_mut(|m| {
                            m.data.insert_temp(slot_id, id);
                            m.data.insert_temp(id.with("str"), val_str.clone());
                            m.data.insert_temp(id.with("focus"), true);
                            m.data.insert_temp(id.with("select_all"), true);
                        });
                    }

                    if ui.is_rect_visible(rect) {
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
                        .align(egui::Align2::CENTER_CENTER)
                        .text_color(t.txt)
                        .background_color(t.numbox_bg)
                        .show(ui);
                    let resp = output.response;

                    let need_focus: bool =
                        ui.memory(|m| m.data.get_temp(id.with("focus")).unwrap_or(false));
                    if need_focus {
                        resp.request_focus();
                        ui.memory_mut(|m| m.data.insert_temp(id.with("focus"), false));
                    }

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
                        ui.memory_mut(|m| m.data.insert_temp(id.with("select_all"), false));
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
                        ui.memory_mut(|m| m.data.insert_temp(id.with("str"), text));
                    }
                }
            });
        },
    );
}

pub fn draw_arc_knob(
    ui: &mut Ui,
    rect: Rect,
    t: &Theme,
    accent: egui::Color32,
    norm: f32,
    scale: f32,
) {
    let painter = ui.painter();
    let center = rect.center();
    let r = rect.width() * 0.5;
    let norm = norm.clamp(0.0, 1.0);
    let start = 0.75 * std::f32::consts::PI;
    let sweep = 1.5 * std::f32::consts::PI;
    let steps = 48;

    let bg_stroke = 1.5 * scale;
    let mut bg = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let a = start + (i as f32 / steps as f32) * sweep;
        bg.push(center + Vec2::new(a.cos(), a.sin()) * r);
    }
    painter.add(egui::Shape::line(
        bg.clone(),
        Stroke::new(bg_stroke, t.track_bg),
    ));
    painter.circle_filled(bg[0], bg_stroke * 0.35, t.track_bg);
    painter.circle_filled(bg[bg.len() - 1], bg_stroke * 0.35, t.track_bg);

    if norm > 0.005 {
        let active = ((steps as f32 * norm) as usize).max(2);
        let mut act = Vec::with_capacity(active + 1);
        for i in 0..=active {
            let frac = (i as f32 / active as f32) * norm;
            let a = start + frac * sweep;
            act.push(center + Vec2::new(a.cos(), a.sin()) * r);
        }
        let act_stroke = 2.0 * scale;
        painter.add(egui::Shape::line(
            act.clone(),
            Stroke::new(act_stroke, accent),
        ));
        painter.circle_filled(act[0], act_stroke * 0.35, accent);
        painter.circle_filled(act[act.len() - 1], act_stroke * 0.35, accent);
    }

    let body_r = r * 0.62;
    painter.circle_filled(center, body_r, t.knob_bg);
    painter.circle_stroke(center, body_r, Stroke::new(1.0, t.knob_border));

    let a = start + norm * sweep;
    let inner = center + Vec2::new(a.cos(), a.sin()) * (body_r * 0.35);
    let outer = center + Vec2::new(a.cos(), a.sin()) * (body_r * 0.88);
    let ind_stroke = 2.0 * scale;
    painter.line_segment([inner, outer], Stroke::new(ind_stroke, accent));
    painter.circle_filled(inner, ind_stroke * 0.35, accent);
    painter.circle_filled(outer, ind_stroke * 0.35, accent);
}

pub fn draw_fader(
    ui: &mut Ui,
    rect: Rect,
    t: &Theme,
    accent: egui::Color32,
    norm: f32,
    scale: f32,
) {
    let painter = ui.painter();
    let cx = painter.round_to_pixel_center(rect.center().x);
    let top = painter.round_to_pixel_center(rect.top() + 4.0 * scale);
    let bot = painter.round_to_pixel_center(rect.bottom() - 4.0 * scale);

    let track_rect = Rect::from_min_max(
        Pos2::new(cx - 1.40625 * scale, top),
        Pos2::new(cx + 1.40625 * scale, bot),
    );
    painter.rect_filled(track_rect, 1.40625 * scale, t.track_bg);

    let norm = norm.clamp(0.0, 1.0);
    let fill_h = (bot - top) * norm;
    if fill_h > 0.5 {
        let fill_rect = Rect::from_min_max(
            Pos2::new(cx - 1.0 * scale, bot - fill_h),
            Pos2::new(cx + 1.0 * scale, bot),
        );
        painter.rect_filled(fill_rect, 1.0 * scale, accent);
    }

    let thumb_y = bot - fill_h;
    painter.circle_filled(Pos2::new(cx, thumb_y), 6.0 * scale, t.thumb_bg);
    painter.circle_stroke(
        Pos2::new(cx, thumb_y),
        6.0 * scale,
        Stroke::new(1.5 * scale, accent),
    );
}

pub fn editable_value<P: Param>(
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
    let slot_id = egui::Id::unique("pejvak_current_editing");
    let current: Option<egui::Id> = ui.memory(|m| m.data.get_temp(slot_id));
    let is_editing = current == Some(edit_id);

    if !is_editing {
        let (vrect, vresp) = ui.allocate_exact_size(Vec2::new(width, 14.0 * scale), Sense::click());
        let hovered = vresp.hovered();
        if ui.is_rect_visible(vrect) {
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
                m.data.insert_temp(slot_id, edit_id);
                m.data.insert_temp(edit_id.with("str"), val_str.clone());
                m.data.insert_temp(edit_id.with("focus"), true);
                m.data.insert_temp(edit_id.with("select_all"), true);
            });
        }
    } else {
        let mut text: String = ui
            .memory(|m| m.data.get_temp(edit_id.with("str")))
            .unwrap_or_else(|| val_str.clone());
        let output = egui::TextEdit::singleline(&mut text)
            .id(edit_id)
            .font(egui::FontId::proportional(10.5 * scale))
            .desired_width(width)
            .align(egui::Align2::CENTER_CENTER)
            .show(ui);
        let resp = output.response;

        let need_focus: bool =
            ui.memory(|m| m.data.get_temp(edit_id.with("focus")).unwrap_or(false));
        if need_focus {
            resp.request_focus();
            ui.memory_mut(|m| m.data.insert_temp(edit_id.with("focus"), false));
        }

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
            ui.memory_mut(|m| m.data.insert_temp(edit_id.with("select_all"), false));
        }

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
            ui.memory_mut(|m| {
                m.data.remove::<egui::Id>(slot_id);
                m.data.remove::<String>(edit_id.with("str"));
                m.data.remove::<bool>(edit_id.with("focus"));
                m.data.remove::<bool>(edit_id.with("select_all"));
            });
        } else {
            ui.memory_mut(|m| m.data.insert_temp(edit_id.with("str"), text));
        }
    }
}

pub fn draw_card<F>(ui: &mut Ui, t: &Theme, scale: f32, enabled: bool, id_key: &str, add: F)
where
    F: FnOnce(&mut Ui, &Theme),
{
    let anim =
        ui.ctx()
            .animate_bool_with_time(egui::Id::unique(("card_enable_fade", id_key)), enabled, 0.25);
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

pub fn draw_card_h<F>(
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
            .animate_bool_with_time(egui::Id::unique(("card_enable_fade", id_key)), enabled, 0.25);
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

pub fn section_title(ui: &mut Ui, t: &Theme, text: &str, accent: egui::Color32, scale: f32) {
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

#[allow(clippy::too_many_arguments)]
pub fn section_title_with_switch<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    text: &str,
    accent: egui::Color32,
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
    let switch_id = egui::Id::unique(("card_switch", switch_param.name()));
    let switch_resp = ui.interact(switch_rect, switch_id, Sense::click());
    let on = switch_param.unmodulated_normalized_value() > 0.5;
    if switch_resp.clicked() {
        setter.begin_set_parameter(switch_param);
        setter.set_parameter_normalized(switch_param, if on { 0.0 } else { 1.0 });
        setter.end_set_parameter(switch_param);
    }

    let tt = ui.ctx().animate_bool_with_time(
        egui::Id::unique(("card_switch_fade", switch_param.name())),
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

        if t.bg_top.r() < 128 {
            let shadow_center = Pos2::new(thumb_x, switch_rect.center().y + 1.0 * scale);
            let base_r = pill_h * 0.38;
            painter.circle_filled(
                shadow_center,
                base_r * 1.00,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 38),
            );
            painter.circle_filled(
                shadow_center,
                base_r * 1.18,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 18),
            );
            painter.circle_filled(
                shadow_center,
                base_r * 1.42,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 7),
            );
        }
        painter.circle_filled(
            Pos2::new(thumb_x, switch_rect.center().y),
            pill_h * 0.38,
            lerp_color(t.dim, egui::Color32::WHITE, tt),
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

pub fn section_title_with_enabled_switch<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    text: &str,
    accent: egui::Color32,
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
    let kill_active: bool = ui.memory(|m| {
        m.data
            .get_temp(egui::Id::unique("pejvak_tx_kill_active"))
            .unwrap_or(false)
    });

    let switch_id = egui::Id::unique(("texture_enabled_switch", enabled_param.name()));
    let switch_resp = ui.interact(switch_rect, switch_id, Sense::click());
    let on = enabled_param.unmodulated_normalized_value() > 0.5;

    if switch_resp.clicked() && !kill_active {
        setter.begin_set_parameter(enabled_param);
        setter.set_parameter_normalized(enabled_param, if on { 0.0 } else { 1.0 });
        setter.end_set_parameter(enabled_param);
    }

    let tt = ui.ctx().animate_bool_with_time(
        egui::Id::unique(("texture_enabled_switch_fade", enabled_param.name())),
        on,
        0.18,
    );
    if ui.is_rect_visible(switch_rect) {
        let painter = ui.painter();
        let track_col = if kill_active {
            KILL_RED.gamma_multiply(0.20)
        } else {
            lerp_color(t.track_bg, accent, tt)
        };
        painter.rect_filled(switch_rect, pill_h * 0.5, track_col);

        let off_x = switch_rect.left() + pill_h * 0.5;
        let on_x = switch_rect.right() - pill_h * 0.5;
        let thumb_x = off_x + (on_x - off_x) * tt;

        if t.bg_top.r() < 128 {
            let shadow_center = Pos2::new(thumb_x, switch_rect.center().y + 1.0 * scale);
            let base_r = pill_h * 0.38;
            painter.circle_filled(
                shadow_center,
                base_r * 1.00,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 38),
            );
            painter.circle_filled(
                shadow_center,
                base_r * 1.18,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 18),
            );
            painter.circle_filled(
                shadow_center,
                base_r * 1.42,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 7),
            );
        }
        painter.circle_filled(
            Pos2::new(thumb_x, switch_rect.center().y),
            pill_h * 0.38,
            if kill_active {
                KILL_RED.gamma_multiply(0.75)
            } else {
                lerp_color(t.dim, egui::Color32::WHITE, tt)
            },
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

    if kill_active {
        let _ = hover_tip(
            switch_resp,
            "Locked — release the TX button in the header to enable Texture layers",
        );
    }
    ui.add_space(4.0 * scale);
}

fn rounded_gradient(
    rect: Rect,
    radius: f32,
    top: egui::Color32,
    bottom: egui::Color32,
) -> egui::Shape {
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
    let starts = [
        std::f32::consts::PI,
        1.5 * std::f32::consts::PI,
        0.0,
        0.5 * std::f32::consts::PI,
    ];
    for c in 0..4 {
        let cc = centers[c];
        for s in 0..=steps {
            let a = starts[c] + (s as f32 / steps as f32) * 0.5 * std::f32::consts::PI;
            perimeter.push(Pos2::new(cc.x + a.cos() * r, cc.y + a.sin() * r));
        }
    }
    let mut mesh = egui::Mesh::default();
    let ymin = rect.top();
    let yspan = (rect.bottom() - ymin).max(1.0);
    let lc = |y: f32| {
        let tt = ((y - ymin) / yspan).clamp(0.0, 1.0);
        egui::Color32::from_rgba_unmultiplied(
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

#[allow(clippy::too_many_arguments)]
pub fn horizontal_switch_2<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    left_text: &str,
    right_text: &str,
    param: &P,
    setter: &ParamSetter,
    left_tooltip: &str,
    right_tooltip: &str,
    accent: egui::Color32,
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
        egui::Id::unique(("hswitch_anim", param.name())),
        is_right,
        0.22,
    );

    let resp_left = ui.interact(
        r_left,
        egui::Id::unique(("hswitch_left", param.name())),
        Sense::click(),
    );
    let resp_right = ui.interact(
        r_right,
        egui::Id::unique(("hswitch_right", param.name())),
        Sense::click(),
    );

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
    painter.rect_filled(r_left, radius, t.track_bg.gamma_multiply(0.45));
    painter.rect_filled(r_right, radius, t.track_bg.gamma_multiply(0.45));

    let highlight_left = r_left.left() + (r_right.left() - r_left.left()) * anim;
    painter.rect_filled(
        Rect::from_min_size(Pos2::new(highlight_left, r_left.top()), Vec2::new(seg_w, h)),
        radius,
        accent.gamma_multiply(0.28),
    );

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

pub fn switch_tiny<P: Param>(
    ui: &mut Ui,
    t: &Theme,
    label: &str,
    param: &P,
    setter: &ParamSetter,
    accent: egui::Color32,
    scale: f32,
) {
    let led_r = 5.0 * scale;
    let gap = 6.0 * scale;
    let label_w = 24.0 * scale;
    let box_w = led_r * 2.0;
    let total_h = led_r * 2.0 + 3.0 * scale;

    let (rect, _) = ui.allocate_exact_size(Vec2::new(box_w, total_h), Sense::hover());
    let led_center = rect.center();
    let label_rect = Rect::from_min_size(
        Pos2::new(rect.right() + gap, rect.top()),
        Vec2::new(label_w, total_h),
    );

    let resp = ui.interact(
        Rect::from_center_size(led_center, Vec2::splat(led_r * 2.4)),
        egui::Id::unique(("tiny_switch", param.name())),
        Sense::click(),
    );
    let on = param.unmodulated_normalized_value() > 0.5;
    if resp.clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, if on { 0.0 } else { 1.0 });
        setter.end_set_parameter(param);
    }

    let tt = ui.ctx().animate_bool_with_time(
        egui::Id::unique(("tiny_switch_fade", param.name())),
        on,
        0.28,
    );

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if tt > 0.01 {
            for i in 0..3 {
                let r = led_r * (1.20 + i as f32 * 0.42);
                let alpha = ((1.0 - i as f32 / 3.0) * 60.0 * tt) as u8;
                painter.circle_filled(
                    led_center,
                    r,
                    egui::Color32::from_rgba_unmultiplied(
                        accent.r(),
                        accent.g(),
                        accent.b(),
                        alpha,
                    ),
                );
            }
        }
        painter.circle_filled(led_center, led_r, t.led_ring);
        painter.circle_filled(led_center, led_r * 0.72, lerp_color(t.led_off, accent, tt));

        if tt > 0.12 {
            painter.circle_filled(
                Pos2::new(led_center.x - led_r * 0.20, led_center.y - led_r * 0.20),
                led_r * 0.28,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, (215.0 * tt) as u8),
            );
        }
        painter.circle_stroke(
            led_center,
            led_r,
            Stroke::new(1.0, lerp_color(t.line, accent.gamma_multiply(0.75), tt)),
        );

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
