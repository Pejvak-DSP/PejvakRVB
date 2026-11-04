//! Main pages UI (MAIN & TEXTURE).

use super::theme::{Theme, lerp_color};
use super::widgets::*;
use super::*;
use crate::PejvakRvbParams;
use egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use nice_plug::prelude::ParamSetter;
use std::sync::Arc;

pub fn draw_main_page(
    ui: &mut Ui,
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    t: &Theme,
    scale: f32,
    is_light: bool,
    use_ffdn_visual: bool,
) {
    if use_ffdn_visual {
        draw_main_page_ffdn(ui, p, s, t, scale);
    } else {
        draw_main_page_cloudseed(ui, p, s, t, scale, is_light);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_main_page_cloudseed(
    ui: &mut Ui,
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    t: &Theme,
    scale: f32,
    _is_light: bool,
) {
    let gap_y = PAGE_GAP * scale;
    let pad_px = CARD_PAD * scale;

    ui.columns(3, |cols| {
        cols[0].vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap_y;
            draw_card(ui, t, scale, p.early.tap_enabled.value(), "multitap", |ui, ct| {
                section_title_with_switch(ui, ct, "MULTITAP DELAY", ct.early, "", &p.early.tap_enabled, s, scale);
                ui.add_enabled_ui(p.early.tap_enabled.value(), |ui| {
                    knob_row(ui, scale, 3, |ui| {
                        knob(ui, ct, "Length", &p.early.tap_length, s, "TapLength", ct.early, scale);
                        knob(ui, ct, "Gain", &p.early.tap_gain, s, "TapGain", ct.early, scale);
                        knob(ui, ct, "Decay", &p.early.tap_decay, s, "TapDecay", ct.early, scale);
                    });
                    ui.add_space(ROW_OUTER_GAP * scale);
                    numbox_row(ui, scale, &[
                        ("Count", &p.early.tap_count, "Number of taps in the multitap diffuser.\n1 = single tap, 50 = dense early reflections."),
                        ("Seed", &p.seeds.tap_seed, "Deterministic PRNG seed for the tap positions.\nChange to get a different tap pattern."),
                    ], s, ct.early, ct);
                });
            });

            draw_card(ui, t, scale, p.late.late_delay_enabled.value(), "late_delay", |ui, ct| {
                section_title_with_switch(ui, ct, "LATE DELAY", ct.late, "", &p.late.late_delay_enabled, s, scale);
                ui.add_enabled_ui(p.late.late_delay_enabled.value(), |ui| {
                    knob_row(ui, scale, 4, |ui| {
                        knob(ui, ct, "Delay", &p.late.line_delay, s, "LineDelay", ct.late, scale);
                        knob(ui, ct, "Decay", &p.late.line_decay, s, "LineDecay", ct.late, scale);
                        knob(ui, ct, "Mod Amt", &p.modulation.line_mod_amount, s, "LineModAmount", ct.late, scale);
                        knob(ui, ct, "Mod Rate", &p.modulation.line_mod_rate, s, "LineModRate", ct.late, scale);
                    });
                    ui.add_space(ROW_OUTER_GAP * scale);
                    numbox_row(ui, scale, &[
                        ("Lines", &p.late.line_count, "Number of active delay lines in the late FDN.\n1 = thin, 12 = dense."),
                        ("Seed", &p.seeds.delay_seed, "PRNG seed for the per-line delays and modulation.\nChange to get a different tail."),
                    ], s, ct.late, ct);
                });
            });

            let remaining_px = (ui.available_height() - gap_y - 4.0 * scale).max(60.0);
            let route_min_h = ((remaining_px - 2.0 * pad_px) / scale).max(80.0);
            draw_card_h(ui, t, scale, true, "route", route_min_h, |ui, ct| {
                section_title_with_switch(ui, ct, "ROUTE", ct.accent, "Pre/Post", &p.switches.late_stage_tap, s, scale);
                ui.add_space(4.0 * scale);
                route_diagram(ui, ct, !p.switches.late_stage_tap.value(), scale);
                ui.add_space(ROW_OUTER_GAP * scale);
                ui.label(egui::RichText::new(if p.switches.late_stage_tap.value() { "EQ is applied after the delay lines." } else { "EQ is applied before the delay lines." }).size(9.0 * scale).color(ct.dim2));
            });
        });

        cols[1].vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap_y;
            draw_card(ui, t, scale, true, "input", |ui, ct| {
                section_title(ui, ct, "INPUT", ct.input, scale);
                knob_row(ui, scale, 4, |ui| {
                    knob(ui, ct, "Blend", &p.input.mix, s, "InputMix", ct.input, scale);
                    knob(ui, ct, "Pre-Dly", &p.input.predelay, s, "PreDelay", ct.input, scale);
                    knob(ui, ct, "Low Cut", &p.input.high_pass, s, "HighPass", ct.input, scale);
                    knob(ui, ct, "High Cut", &p.input.low_pass, s, "LowPass", ct.input, scale);
                });
                ui.add_space(ROW_OUTER_GAP * scale);
                switch_row_at_columns(ui, scale, 4, KNOB_SLOT_W, &[
                    (2, "HP", &p.switches.hi_pass_enabled), (3, "LP", &p.switches.low_pass_enabled),
                ], s, ct.input, ct);
            });

            draw_card(ui, t, scale, p.eq.enabled.value(), "eq", |ui, ct| {
                section_title_with_switch(ui, ct, "EQ", ct.tone, "", &p.eq.enabled, s, scale);
                ui.add_enabled_ui(p.eq.enabled.value(), |ui| {
                    knob_row_compact(ui, scale, 5, |ui| {
                        knob_compact(ui, ct, "Low Frq", &p.eq.low_shelf_frequency, s, "PostLowShelfFrequency", ct.tone, scale);
                        knob_compact(ui, ct, "Low Gain", &p.eq.low_shelf_gain, s, "PostLowShelfGain", ct.tone, scale);
                        knob_compact(ui, ct, "Hi Gain", &p.eq.high_shelf_gain, s, "PostHighShelfGain", ct.tone, scale);
                        knob_compact(ui, ct, "Hi Frq", &p.eq.high_shelf_frequency, s, "PostHighShelfFrequency", ct.tone, scale);
                        knob_compact(ui, ct, "Cutoff", &p.eq.cutoff_frequency, s, "PostCutoffFrequency", ct.tone, scale);
                    });
                    ui.add_space(ROW_OUTER_GAP * scale);
                    switch_row_at_columns(ui, scale, 5, KNOB_SMALL_SLOT_W, &[
                        (0, "Low", &p.switches.low_shelf_enabled), (3, "High", &p.switches.high_shelf_enabled), (4, "Cutoff", &p.switches.cutoff_enabled),
                    ], s, ct.tone, ct);
                });
            });

            let remaining_px = (ui.available_height() - gap_y - 4.0 * scale).max(60.0);
            let mixer_min_h = ((remaining_px - 2.0 * pad_px) / scale).max(80.0);
            draw_card_h(ui, t, scale, true, "mixer", mixer_min_h, |ui, ct| {
                section_title(ui, ct, "MIXER", ct.output, scale);
                fader_row(ui, scale, 5, |ui| {
                    fader(ui, ct, "Dry", &p.output.dry_out, s, "DryOut", ct.output, scale);
                    fader(ui, ct, "PreDly", &p.output.predelay_out, s, "PredelayOut", ct.output, scale);
                    fader(ui, ct, "Early", &p.output.early_out, s, "EarlyOut", ct.output, scale);
                    fader(ui, ct, "Late", &p.output.main_out, s, "MainOut", ct.output, scale);
                    fader(ui, ct, "Width", &p.output.width, s, "Width", ct.output, scale);
                });
            });
        });

        cols[2].vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap_y;
            draw_card(ui, t, scale, p.early.diffusion_enabled.value(), "early_diffusion", |ui, ct| {
                section_title_with_switch(ui, ct, "EARLY DIFFUSION", ct.early_diffusion, "", &p.early.diffusion_enabled, s, scale);
                ui.add_enabled_ui(p.early.diffusion_enabled.value(), |ui| {
                    knob_row(ui, scale, 4, |ui| {
                        knob(ui, ct, "Delay", &p.early.diffusion_delay, s, "DiffusionDelay", ct.early_diffusion, scale);
                        knob(ui, ct, "Feedback", &p.early.diffusion_feedback, s, "DiffusionFeedback", ct.early_diffusion, scale);
                        knob(ui, ct, "Mod Amt", &p.modulation.early_diffusion_mod_amount, s, "EarlyDiffusionModAmount", ct.early_diffusion, scale);
                        knob(ui, ct, "Mod Rate", &p.modulation.early_diffusion_mod_rate, s, "EarlyDiffusionModRate", ct.early_diffusion, scale);
                    });
                    ui.add_space(ROW_OUTER_GAP * scale);
                    numbox_row(ui, scale, &[
                        ("Stages", &p.early.diffusion_stages, "Number of allpass stages in the early diffusion cascade.\n1 = minimal, 8 = smoothest."),
                        ("Seed", &p.seeds.diffusion_seed, "PRNG seed for the allpass delay lengths.\nChange to get a different diffusion character."),
                    ], s, ct.early_diffusion, ct);
                });
            });

            draw_card(ui, t, scale, p.late.late_diffusion_enabled.value(), "late_diffusion", |ui, ct| {
                section_title_with_switch(ui, ct, "LATE DIFFUSION", ct.late_diffusion, "", &p.late.late_diffusion_enabled, s, scale);
                ui.add_enabled_ui(p.late.late_diffusion_enabled.value(), |ui| {
                    knob_row(ui, scale, 4, |ui| {
                        knob(ui, ct, "Delay", &p.late.late_diffusion_delay, s, "LateDiffusionDelay", ct.late_diffusion, scale);
                        knob(ui, ct, "Feedback", &p.late.late_diffusion_feedback, s, "LateDiffusionFeedback", ct.late_diffusion, scale);
                        knob(ui, ct, "Mod Amt", &p.modulation.late_diffusion_mod_amount, s, "LateDiffusionModAmount", ct.late_diffusion, scale);
                        knob(ui, ct, "Mod Rate", &p.modulation.late_diffusion_mod_rate, s, "LateDiffusionModRate", ct.late_diffusion, scale);
                    });
                    ui.add_space(ROW_OUTER_GAP * scale);
                    numbox_row(ui, scale, &[
                        ("Stages", &p.late.late_diffusion_stages, "Number of allpass stages per late delay line.\n1 = minimal, 8 = smoothest."),
                        ("Seed", &p.seeds.post_diffusion_seed, "PRNG seed for the per-line diffuser delays.\nChange to vary the late diffusion."),
                    ], s, ct.late_diffusion, ct);
                });
            });

            let remaining_px = (ui.available_height() - gap_y - 4.0 * scale).max(60.0);
            let velvet_min_h = ((remaining_px - 2.0 * pad_px) / scale).max(80.0);
            draw_card_h(ui, t, scale, true, "velvet", velvet_min_h, |ui, ct| {
                section_title(ui, ct, "VELVET NOISE", ct.velvet, scale);
                knob_row(ui, scale, 3, |ui| {
                    knob(ui, ct, "Velvet", &p.velvet.diffusion, s, "Velvet-Noise diffusion amount.", ct.velvet, scale);
                    knob(ui, ct, "Density", &p.velvet.density, s, "Pulse density.", ct.velvet, scale);
                    knob(ui, ct, "Tone", &p.velvet.tone, s, "Pulse width.", ct.velvet, scale);
                });
            });
        });
    });
}

fn draw_main_page_ffdn(
    ui: &mut Ui,
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    t: &Theme,
    scale: f32,
) {
    let gap_y = PAGE_GAP * scale;
    let pad_px = CARD_PAD * scale;

    ui.columns(3, |cols| {
        cols[0].vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap_y;
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
                            "Base delay length.",
                            ct.accent,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Decay",
                            &p.ffdn.decay,
                            s,
                            "Target RT60.",
                            ct.accent,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Spread",
                            &p.ffdn.spread,
                            s,
                            "Spread of delay lines.",
                            ct.accent,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Diffusion",
                            &p.ffdn.diffusion,
                            s,
                            "Input diffusion.",
                            ct.accent,
                            scale,
                        );
                    });
                });
            });

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
                            "Active delay lines.",
                            ct.late,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Density",
                            &p.ffdn.density,
                            s,
                            "Input energy distribution.",
                            ct.late,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Decorr",
                            &p.ffdn.decorr,
                            s,
                            "L/R delay offset.",
                            ct.late,
                            scale,
                        );
                    });
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
                        "Dense Householder",
                        "Sparse Velvet",
                        ct.late,
                        scale,
                    );
                });
            });

            let filter_on = p.ffdn.filter_enabled.value();
            let remaining_px = (ui.available_height() - gap_y - 4.0 * scale).max(60.0);
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
                                "High-pass in feedback loop.",
                                ct.early_diffusion,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Loop LP",
                                &p.ffdn.loop_lp,
                                s,
                                "Low-pass in feedback loop.",
                                ct.early_diffusion,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Mid Focus",
                                &p.ffdn.mid_focus,
                                s,
                                "Focus tail on mid band.",
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

        cols[1].vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap_y;
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
                            "Cross-feed.",
                            ct.tone,
                            scale,
                        );
                        knob_compact(
                            ui,
                            ct,
                            "Sat",
                            &p.ffdn.drive,
                            s,
                            "Velvet saturation.",
                            ct.tone,
                            scale,
                        );
                        knob_compact(
                            ui,
                            ct,
                            "Tilt Frq",
                            &p.ffdn.tilt_freq,
                            s,
                            "Tilt crossover.",
                            ct.tone,
                            scale,
                        );
                        knob_compact(
                            ui,
                            ct,
                            "Tilt Amt",
                            &p.ffdn.tilt_amount,
                            s,
                            "Spectral tilt.",
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

            let remaining_px = (ui.available_height() - gap_y - 4.0 * scale).max(60.0);
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
                        "ER level",
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

        cols[2].vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap_y;
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
                            "Bass RT60 multiplier.",
                            ct.tex_shimmer,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Bass Hz",
                            &p.ffdn.bass_freq,
                            s,
                            "Low shelf crossover.",
                            ct.tex_shimmer,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Treb",
                            &p.ffdn.treble_mult,
                            s,
                            "Treble RT60 multiplier.",
                            ct.tex_shimmer,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Treb Hz",
                            &p.ffdn.treble_freq,
                            s,
                            "High shelf crossover.",
                            ct.tex_shimmer,
                            scale,
                        );
                    });
                });
            });

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
                            "Primary HF shaper.",
                            ct.tex_bloom,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Mod Amt",
                            &p.ffdn.mod_amount,
                            s,
                            "LFO depth.",
                            ct.tex_bloom,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Mod Rate",
                            &p.ffdn.mod_rate,
                            s,
                            "LFO speed.",
                            ct.tex_bloom,
                            scale,
                        );
                    });
                    ui.add_space(ROW_OUTER_GAP * scale);
                    knob_row(ui, scale, 2, |ui| {
                        knob(
                            ui,
                            ct,
                            "Matrix Amt",
                            &p.ffdn.matrix_mod,
                            s,
                            "Time-varying feedback matrix.",
                            ct.tex_bloom,
                            scale,
                        );
                        knob(
                            ui,
                            ct,
                            "Matrix Rate",
                            &p.ffdn.matrix_rate,
                            s,
                            "Matrix rotation speed.",
                            ct.tex_bloom,
                            scale,
                        );
                    });
                });
            });

            let dynamics_on = p.ffdn.dynamics_enabled.value();
            let remaining_px = (ui.available_height() - gap_y - 12.0 * scale).max(60.0);
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
                        let used = KNOB_SLOT_H * scale + 6.0 * scale;
                        let target = dynamics_h * scale - CARD_TITLE_H * scale - 12.0 * scale;
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
                                "Softens initial transient.",
                                ct.tex_scatter,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Ducking",
                                &p.ffdn.ducking,
                                s,
                                "Reduces wet while dry present.",
                                ct.tex_scatter,
                                scale,
                            );
                            knob(
                                ui,
                                ct,
                                "Gate",
                                &p.ffdn.gate,
                                s,
                                "Silences wet when below threshold.",
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

pub fn draw_texture_page(
    ui: &mut Ui,
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    t: &Theme,
    scale: f32,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = PAGE_GAP * scale;
        ui.spacing_mut().item_spacing.x = TEX_COLUMN_GAP * scale;
        ui.columns(3, |cols| {
            cols[0].vertical(|ui| {
                ui.spacing_mut().item_spacing.y = PAGE_GAP * scale;
                draw_card(
                    ui,
                    t,
                    scale,
                    p.shimmer.enabled.value(),
                    "tex_shimmer",
                    |ui, ct| {
                        section_title_with_enabled_switch(
                            ui,
                            ct,
                            "SHIMMER",
                            ct.tex_shimmer,
                            &p.shimmer.enabled,
                            s,
                            scale,
                        );
                        ui.add_enabled_ui(p.shimmer.enabled.value(), |ui| {
                            knob_row_compact(ui, scale, 5, |ui| {
                                knob_compact(
                                    ui,
                                    ct,
                                    "Pitch",
                                    &p.shimmer.pitch,
                                    s,
                                    "Pitch shift.",
                                    ct.tex_shimmer,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Feedback",
                                    &p.shimmer.feedback,
                                    s,
                                    "Feedback amount.",
                                    ct.tex_shimmer,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Mix",
                                    &p.shimmer.mix,
                                    s,
                                    "Amount of shimmer tail.",
                                    ct.tex_shimmer,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "LowCut",
                                    &p.shimmer.low_cut,
                                    s,
                                    "HP before pitch shifter.",
                                    ct.tex_shimmer,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "HighCut",
                                    &p.shimmer.high_cut,
                                    s,
                                    "LP after pitch shifter.",
                                    ct.tex_shimmer,
                                    scale,
                                );
                            });
                        });
                    },
                );
            });

            cols[1].vertical(|ui| {
                ui.spacing_mut().item_spacing.y = PAGE_GAP * scale;
                draw_card(
                    ui,
                    t,
                    scale,
                    p.mist.enabled.value(),
                    "tex_mist",
                    |ui, ct| {
                        section_title_with_enabled_switch(
                            ui,
                            ct,
                            "MIST",
                            ct.tex_mist,
                            &p.mist.enabled,
                            s,
                            scale,
                        );
                        ui.add_enabled_ui(p.mist.enabled.value(), |ui| {
                            knob_row_compact(ui, scale, 5, |ui| {
                                knob_compact(
                                    ui,
                                    ct,
                                    "HP",
                                    &p.mist.hp,
                                    s,
                                    "HP before delay.",
                                    ct.tex_mist,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Feedback",
                                    &p.mist.feedback,
                                    s,
                                    "Diffusion feedback.",
                                    ct.tex_mist,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Mix",
                                    &p.mist.mix,
                                    s,
                                    "Amount of mist.",
                                    ct.tex_mist,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Delay",
                                    &p.mist.delay,
                                    s,
                                    "Base delay.",
                                    ct.tex_mist,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Rate",
                                    &p.mist.rate,
                                    s,
                                    "LFO rate.",
                                    ct.tex_mist,
                                    scale,
                                );
                            });
                        });
                    },
                );
            });

            cols[2].vertical(|ui| {
                ui.spacing_mut().item_spacing.y = PAGE_GAP * scale;
                draw_card(
                    ui,
                    t,
                    scale,
                    p.drift.enabled.value(),
                    "tex_drift",
                    |ui, ct| {
                        section_title_with_enabled_switch(
                            ui,
                            ct,
                            "DRIFT",
                            ct.tex_drift,
                            &p.drift.enabled,
                            s,
                            scale,
                        );
                        ui.add_enabled_ui(p.drift.enabled.value(), |ui| {
                            knob_row_compact(ui, scale, 5, |ui| {
                                knob_compact(
                                    ui,
                                    ct,
                                    "Feedback",
                                    &p.drift.feedback,
                                    s,
                                    "Drift feedback.",
                                    ct.tex_drift,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Delay",
                                    &p.drift.delay,
                                    s,
                                    "Base delay.",
                                    ct.tex_drift,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Mix",
                                    &p.drift.mix,
                                    s,
                                    "Amount of drift.",
                                    ct.tex_drift,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Rate",
                                    &p.drift.rate,
                                    s,
                                    "LFO rate.",
                                    ct.tex_drift,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Depth",
                                    &p.drift.depth,
                                    s,
                                    "Modulation depth.",
                                    ct.tex_drift,
                                    scale,
                                );
                            });
                        });
                    },
                );
            });
        });

        ui.spacing_mut().item_spacing.x = TEX_COLUMN_GAP * scale;
        ui.columns(3, |cols| {
            cols[0].vertical(|ui| {
                ui.spacing_mut().item_spacing.y = PAGE_GAP * scale;
                draw_card(
                    ui,
                    t,
                    scale,
                    p.crystal.enabled.value(),
                    "tex_crystal",
                    |ui, ct| {
                        section_title_with_enabled_switch(
                            ui,
                            ct,
                            "CRYSTAL",
                            ct.tex_crystal,
                            &p.crystal.enabled,
                            s,
                            scale,
                        );
                        ui.add_enabled_ui(p.crystal.enabled.value(), |ui| {
                            knob_row_compact(ui, scale, 4, |ui| {
                                knob_compact(
                                    ui,
                                    ct,
                                    "Size",
                                    &p.crystal.size,
                                    s,
                                    "Grain length.",
                                    ct.tex_crystal,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Density",
                                    &p.crystal.density,
                                    s,
                                    "Time between grains.",
                                    ct.tex_crystal,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Mix",
                                    &p.crystal.mix,
                                    s,
                                    "Amount of granular texture.",
                                    ct.tex_crystal,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Feedback",
                                    &p.crystal.feedback,
                                    s,
                                    "Grain cloud feedback.",
                                    ct.tex_crystal,
                                    scale,
                                );
                            });
                        });
                    },
                );
            });

            cols[1].vertical(|ui| {
                ui.spacing_mut().item_spacing.y = PAGE_GAP * scale;
                draw_card(
                    ui,
                    t,
                    scale,
                    p.scatter.enabled.value(),
                    "tex_scatter",
                    |ui, ct| {
                        section_title_with_enabled_switch(
                            ui,
                            ct,
                            "SCATTER",
                            ct.tex_scatter,
                            &p.scatter.enabled,
                            s,
                            scale,
                        );
                        ui.add_enabled_ui(p.scatter.enabled.value(), |ui| {
                            knob_row_compact(ui, scale, 5, |ui| {
                                knob_compact(
                                    ui,
                                    ct,
                                    "Spread",
                                    &p.scatter.spread,
                                    s,
                                    "Delay spread.",
                                    ct.tex_scatter,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Density",
                                    &p.scatter.density,
                                    s,
                                    "Grain spawn rate.",
                                    ct.tex_scatter,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Mix",
                                    &p.scatter.mix,
                                    s,
                                    "Amount of scattered echoes.",
                                    ct.tex_scatter,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Speed",
                                    &p.scatter.speed,
                                    s,
                                    "Spawn rate scaling.",
                                    ct.tex_scatter,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Feedback",
                                    &p.scatter.feedback,
                                    s,
                                    "Scatter feedback.",
                                    ct.tex_scatter,
                                    scale,
                                );
                            });
                        });
                    },
                );
            });

            cols[2].vertical(|ui| {
                ui.spacing_mut().item_spacing.y = PAGE_GAP * scale;
                draw_card(
                    ui,
                    t,
                    scale,
                    p.bloom.enabled.value(),
                    "tex_bloom",
                    |ui, ct| {
                        section_title_with_enabled_switch(
                            ui,
                            ct,
                            "BLOOM",
                            ct.tex_bloom,
                            &p.bloom.enabled,
                            s,
                            scale,
                        );
                        ui.add_enabled_ui(p.bloom.enabled.value(), |ui| {
                            knob_row_compact(ui, scale, 5, |ui| {
                                knob_compact(
                                    ui,
                                    ct,
                                    "Rise",
                                    &p.bloom.rise,
                                    s,
                                    "Attack time.",
                                    ct.tex_bloom,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Size",
                                    &p.bloom.size,
                                    s,
                                    "Diffusion space size.",
                                    ct.tex_bloom,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Mix",
                                    &p.bloom.mix,
                                    s,
                                    "Amount of blooming texture.",
                                    ct.tex_bloom,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Feedback",
                                    &p.bloom.feedback,
                                    s,
                                    "Bloom feedback.",
                                    ct.tex_bloom,
                                    scale,
                                );
                                knob_compact(
                                    ui,
                                    ct,
                                    "Tone",
                                    &p.bloom.tone,
                                    s,
                                    "Brightness.",
                                    ct.tex_bloom,
                                    scale,
                                );
                            });
                        });
                    },
                );
            });
        });

        draw_texture_order_card(ui, p, t, scale);
        draw_illusion_card(ui, p, s, t, scale);
    });
}

fn draw_texture_order_card(ui: &mut Ui, p: &Arc<PejvakRvbParams>, t: &Theme, scale: f32) {
    use std::sync::atomic::Ordering;
    const NAMES: [&str; 6] = ["SHIMMER", "MIST", "DRIFT", "CRYSTAL", "SCATTER", "BLOOM"];
    const NO_DRAG: usize = usize::MAX;

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

    let drag_layer_id = egui::Id::unique("texture_chain_dragged_layer");
    let mut dragged_layer: usize = ui
        .memory(|m| m.data.get_temp(drag_layer_id))
        .unwrap_or(NO_DRAG);
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
        let enabled: [bool; 6] = [
            p.shimmer.enabled.value(),
            p.mist.enabled.value(),
            p.drift.enabled.value(),
            p.crystal.enabled.value(),
            p.scatter.enabled.value(),
            p.bloom.enabled.value(),
        ];

        let chip_h = 38.0 * scale;
        let arrow_w = 24.0 * scale;
        let gap = 6.0 * scale;
        let avail = ui.available_width();
        let chip_w = ((avail - arrow_w * 5.0 - gap * 10.0) / 6.0).max(40.0 * scale);

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

        let mut chip_hover = [false; 6];
        for pos in 0..6 {
            let layer = new_order[pos];
            let resp = ui.interact(
                chip_rects[pos],
                egui::Id::unique(("chain_chip", layer)),
                Sense::click_and_drag(),
            );
            if resp.drag_started() {
                dragged_layer = layer;
            }
            if resp.drag_stopped() {
                dragged_layer = NO_DRAG;
            }
            chip_hover[pos] = resp.hovered() && dragged_layer != layer;

            if dragged_layer == layer || resp.is_pointer_button_down_on() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if resp.hovered() && dragged_layer == NO_DRAG {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }

        let mut arrow_hover = [false; 5];
        for i in 0..5 {
            let resp = ui.interact(
                arrow_rects[i],
                egui::Id::unique(("chain_arrow", i)),
                Sense::click(),
            );
            arrow_hover[i] = resp.hovered();
            if resp.clicked() && dragged_layer == NO_DRAG {
                new_order.swap(i, i + 1);
            }
            if resp.hovered() && dragged_layer == NO_DRAG {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }

        if dragged_layer != NO_DRAG {
            if let Some(pointer) = ui.ctx().pointer_latest_pos() {
                if let Some(dp) = new_order.iter().position(|&l| l == dragged_layer) {
                    let px = pointer.x;
                    if dp + 1 < 6 {
                        let mid_right =
                            (chip_rects[dp].center().x + chip_rects[dp + 1].center().x) * 0.5;
                        if px > mid_right {
                            new_order.swap(dp, dp + 1);
                        }
                    }
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

        let dt = ui.input(|i| i.stable_dt).clamp(0.0, 0.05);
        let mut vis_x = [0.0_f32; 6];
        for pos in 0..6 {
            let layer = new_order[pos];
            let target_cx = chip_rects[pos].center().x;
            let vis_id = egui::Id::unique(("chain_chip_vis", layer));
            let start: f32 = ui.memory(|m| m.data.get_temp(vis_id)).unwrap_or(target_cx);
            let new_x = if (start - target_cx).abs() < 0.3 {
                target_cx
            } else {
                any_animating = true;
                start + (target_cx - start) * (dt * 20.0).clamp(0.0, 1.0)
            };
            ui.memory_mut(|m| m.data.insert_temp(vis_id, new_x));
            vis_x[layer] = new_x;
        }

        let painter = ui.painter();
        let chip_grey = Color32::from_rgb(0x55, 0x58, 0x60);
        for pos in 0..6 {
            let layer = new_order[pos];
            let is_dragged = dragged_layer == layer;
            let hover_anim = if is_dragged {
                0.0
            } else {
                ui.ctx().animate_bool_with_time(
                    egui::Id::unique(("chain_chip_hover_anim", layer)),
                    chip_hover[pos],
                    0.15,
                )
            };
            let enabled_anim = ui.ctx().animate_bool_with_time(
                egui::Id::unique(("chain_chip_enabled_anim", layer)),
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

        for i in 0..5 {
            draw_chain_arrow(painter, arrow_rects[i], ct, arrow_hover[i], scale);
        }

        if dragged_layer != NO_DRAG {
            if let Some(pointer) = ui.ctx().pointer_latest_pos() {
                let layer_painter = ui.ctx().layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::unique("texture_order_ghost"),
                ));
                draw_chain_chip(
                    &layer_painter,
                    Rect::from_center_size(
                        pointer + Vec2::new(0.0, -chip_h * 0.55),
                        Vec2::new(chip_w, chip_h),
                    ),
                    colors[dragged_layer],
                    NAMES[dragged_layer],
                    scale,
                    0.7,
                );
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    });

    ui.memory_mut(|m| m.data.insert_temp(drag_layer_id, dragged_layer));
    if any_animating {
        ui.ctx().request_repaint();
    }

    if new_order != order {
        let mut packed: u64 = 0;
        for (i, &idx) in new_order.iter().enumerate() {
            packed |= (idx as u64) << (i * 8);
        }
        p.texture_order.store(packed, Ordering::Relaxed);
        ui.ctx().request_repaint();
    }
}

fn draw_illusion_card(
    ui: &mut Ui,
    p: &Arc<PejvakRvbParams>,
    s: &ParamSetter,
    t: &Theme,
    scale: f32,
) {
    let illusion_on = p.illusion.enabled.value();
    let accent_anim =
        ui.ctx()
            .animate_bool_with_time(egui::Id::unique("illusion_accent_fade"), illusion_on, 0.25);
    let accent = lerp_color(
        Color32::from_rgb(0x55, 0x58, 0x60),
        t.tex_illusion,
        accent_anim,
    );

    draw_card_h(ui, t, scale, illusion_on, "illusion", 80.0, |ui, ct| {
        section_title_with_enabled_switch(
            ui,
            ct,
            "ILLUSION",
            accent,
            &p.illusion.enabled,
            s,
            scale,
        );
        ui.add_enabled_ui(illusion_on, |ui| {
            knob_row_compact(ui, scale, 8, |ui| {
                knob_compact(
                    ui,
                    ct,
                    "Spark",
                    &p.illusion.spark,
                    s,
                    "Granular cloud amount.",
                    accent,
                    scale,
                );
                knob_compact(
                    ui,
                    ct,
                    "Still",
                    &p.illusion.still,
                    s,
                    "Motion smoothness.",
                    accent,
                    scale,
                );
                knob_compact(
                    ui,
                    ct,
                    "Aura",
                    &p.illusion.aura,
                    s,
                    "Pitch shift.",
                    accent,
                    scale,
                );
                knob_compact(
                    ui,
                    ct,
                    "Veil",
                    &p.illusion.veil,
                    s,
                    "Feedback.",
                    accent,
                    scale,
                );
                knob_compact(
                    ui,
                    ct,
                    "Wide",
                    &p.illusion.wide,
                    s,
                    "Stereo width.",
                    accent,
                    scale,
                );
                knob_compact(ui, ct, "Tint", &p.illusion.tint, s, "Tone.", accent, scale);
                knob_compact(
                    ui,
                    ct,
                    "Rate",
                    &p.illusion.rate,
                    s,
                    "Clock speed.",
                    accent,
                    scale,
                );
                knob_compact(
                    ui,
                    ct,
                    "Grain",
                    &p.illusion.grain,
                    s,
                    "Length of each grain.",
                    accent,
                    scale,
                );
            });
        });
    });
}

fn inset_hexagon_vertices(outer: &[Pos2; 6], d: f32) -> [Pos2; 6] {
    let centroid = Pos2::new(
        outer.iter().map(|p| p.x).sum::<f32>() / 6.0,
        outer.iter().map(|p| p.y).sum::<f32>() / 6.0,
    );
    let mut inner = [Pos2::ZERO; 6];
    for i in 0..6 {
        let prev = outer[(i + 5) % 6];
        let curr = outer[i];
        let next = outer[(i + 1) % 6];
        let u_prev = (prev - curr) / (prev - curr).length().max(1e-6);
        let u_next = (next - curr) / (next - curr).length().max(1e-6);
        let sin_half = ((1.0 - u_prev.dot(u_next).clamp(-1.0, 1.0)) * 0.5)
            .max(1e-8)
            .sqrt();
        let mut bisector = u_prev + u_next;
        bisector /= bisector.length().max(1e-6);
        if bisector.dot(centroid - curr) < 0.0 {
            bisector = -bisector;
        }
        inner[i] = curr + bisector * (d / sin_half);
    }
    inner
}

fn draw_hexagon_aa(painter: &egui::Painter, verts: &[Pos2; 6], fill: Color32) {
    painter.add(egui::Shape::convex_polygon(
        vec![verts[5], verts[0], verts[1], verts[2]],
        fill,
        egui::Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![verts[5], verts[2], verts[3], verts[4]],
        fill,
        egui::Stroke::NONE,
    ));
}

fn draw_chain_chip(
    painter: &egui::Painter,
    rect: Rect,
    color: Color32,
    name: &str,
    scale: f32,
    hover: f32,
) {
    let notch = (rect.height() * 0.40).min(12.0 * scale);
    let mid_y = rect.center().y;
    let outer = [
        Pos2::new(rect.left(), rect.top()),
        Pos2::new(rect.right() - notch, rect.top()),
        Pos2::new(rect.right(), mid_y),
        Pos2::new(rect.right() - notch, rect.bottom()),
        Pos2::new(rect.left(), rect.bottom()),
        Pos2::new(rect.left() + notch, mid_y),
    ];
    draw_hexagon_aa(painter, &outer, color.gamma_multiply(0.16));

    if hover > 0.01 {
        let inner = inset_hexagon_vertices(
            &outer,
            (2.0 * scale).min(rect.height().min(rect.width()) * 0.4),
        );
        draw_hexagon_aa(
            painter,
            &inner,
            color.gamma_multiply(0.16 + 0.12 * hover.clamp(0.0, 1.0)),
        );
    }
    
    // Increased font size from 9.5 to 11.5 for better readability.
    let font = egui::FontId::proportional(11.5 * scale);
    let text_pos = Pos2::new(rect.center().x, mid_y);
    
    // Draw the text normally.
    painter.text(text_pos, egui::Align2::CENTER_CENTER, name, font.clone(), color);
    
    // Faux Bold: draw the text a second time with a tiny offset (0.5px) 
    // to simulate a bolder font weight since egui's default proportional 
    // font doesn't include a native bold variant.
    painter.text(
        text_pos + Vec2::new(0.5 * scale, 0.0), 
        egui::Align2::CENTER_CENTER, 
        name, 
        font, 
        color
    );
}

fn draw_chain_arrow(painter: &egui::Painter, rect: Rect, t: &Theme, hover: bool, scale: f32) {
    let color = if hover { t.accent } else { t.dim2 };
    let stroke = Stroke::new(3.0 * scale, color);
    let c = rect.center() + Vec2::new(5.0 * scale, 0.0);
    for k in 0..2 {
        let tip_x = c.x + (k as f32 - 0.5) * (5.0 * scale + 8.0 * scale);
        painter.line_segment(
            [
                Pos2::new(tip_x - 5.0 * scale, c.y - 6.5 * scale),
                Pos2::new(tip_x, c.y),
            ],
            stroke,
        );
        painter.line_segment(
            [
                Pos2::new(tip_x, c.y),
                Pos2::new(tip_x - 5.0 * scale, c.y + 6.5 * scale),
            ],
            stroke,
        );
    }
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
            .animate_bool_with_time(egui::Id::unique("route_diagram_phase"), !pre_mode, 0.35);
    let painter = ui.painter();
    let eq_slot = 1.0 + phase * 2.0;
    let diff_slot = 2.0 - phase;
    let dly_slot = 3.0 - phase;
    let box_h = 18.0 * scale;
    let arrow_gap = 6.0 * scale;
    let inner_pad = 4.0 * scale;
    let arrow_head = 3.5 * scale;

    let box_w = ((full_w - arrow_gap * 4.0 - inner_pad * 2.0) / 5.0).max(28.0 * scale);
    let slot_w = box_w + arrow_gap;
    let cy = rect.center().y;
    let left = rect.left() + inner_pad;
    let font = egui::FontId::proportional(9.5 * scale);
    let slot_x = |slot: f32| left + slot * slot_w;

    for i in 0..4 {
        let x_start = slot_x(i as f32) + box_w;
        let x_end = slot_x((i + 1) as f32);
        let a_end = Pos2::new(x_end - 1.0, cy);
        painter.line_segment(
            [Pos2::new(x_start + 1.0, cy), a_end],
            Stroke::new(1.0, t.dim2),
        );
        painter.line_segment(
            [
                Pos2::new(a_end.x - arrow_head, cy - arrow_head * 0.7),
                a_end,
            ],
            Stroke::new(1.0, t.dim2),
        );
        painter.line_segment(
            [
                Pos2::new(a_end.x - arrow_head, cy + arrow_head * 0.7),
                a_end,
            ],
            Stroke::new(1.0, t.dim2),
        );
    }

    for (label, slot) in [
        ("IN", 0.0),
        ("EQ", eq_slot),
        ("DIFF", diff_slot),
        ("DLY", dly_slot),
        ("OUT", 4.0),
    ] {
        let box_rect = Rect::from_min_size(
            Pos2::new(slot_x(slot), cy - box_h * 0.5),
            Vec2::new(box_w, box_h),
        );
        if label == "EQ" {
            painter.rect_filled(box_rect, 3.0 * scale, t.accent.gamma_multiply(0.22));
            painter.text(
                box_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                font.clone(),
                t.accent,
            );
        } else {
            painter.rect_filled(box_rect, 3.0 * scale, t.track_bg.gamma_multiply(0.70));
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
