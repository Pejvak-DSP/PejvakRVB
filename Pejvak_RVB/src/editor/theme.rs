//! Theme definitions for PEJVAK RVB.

use egui::Color32;

pub const KILL_RED: Color32 = Color32::from_rgb(0xEF, 0x44, 0x44);

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
    pub tex_illusion: Color32,
    pub velvet: Color32,
    pub early_diffusion: Color32,
    pub late_diffusion: Color32,
    pub numbox_bg: Color32,
    pub led_ring: Color32,
    pub led_off: Color32,
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
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
            tex_illusion: Color32::from_rgb(0xE6, 0xF2, 0xFF),
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
            tex_illusion: Color32::from_rgb(0x16, 0x2B, 0x5A),
            velvet: Color32::from_rgb(0x6E, 0x46, 0xA8),
            numbox_bg: Color32::from_rgb(0xC8, 0xCC, 0xD2),
            led_ring: Color32::from_rgb(0xB4, 0xB9, 0xC1),
            led_off: Color32::from_rgb(0x9A, 0x9F, 0xA9),
        }
    }

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
            tex_illusion,
            velvet,
            numbox_bg,
            led_ring,
            led_off
        )
    }

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
            tex_illusion: grey,
            velvet: grey,
            numbox_bg: self.numbox_bg,
            led_ring: self.led_ring,
            led_off: self.led_off,
        }
    }
}
