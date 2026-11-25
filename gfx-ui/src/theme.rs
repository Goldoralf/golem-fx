// SPDX-License-Identifier: GPL-3.0-or-later
//! The chrome: dark, the pedal's own colour as the accent, the pedals' font.

use nih_plug_egui::egui::{self, Color32, FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

/// Saira Semi Condensed (OFL, `visuals/assets/fonts`): the font of the pedals' labels.
static FONT: &[u8] = include_bytes!("../../visuals/assets/fonts/SairaSemiCondensed-SemiBold.ttf");

pub struct Theme {
    pub accent: Color32,
    pub bg: Color32,
    pub bar: Color32,
    pub track: Color32,
    pub text: Color32,
    pub dim: Color32,
}

fn hex(s: &str) -> Option<Color32> {
    let s = s.strip_prefix('#')?;
    let n = u32::from_str_radix(s, 16).ok()?;
    (s.len() == 6).then(|| Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

impl Theme {
    pub fn new(accent: &str) -> Self {
        Self {
            accent: hex(accent).unwrap_or(Color32::from_rgb(0xc8, 0xa2, 0x4a)),
            bg: Color32::from_rgb(0x15, 0x13, 0x11),
            bar: Color32::from_rgb(0x0d, 0x0b, 0x09),
            track: Color32::from_rgb(0x2c, 0x28, 0x24),
            text: Color32::from_rgb(0xe8, 0xe2, 0xd8),
            dim: Color32::from_rgb(0x8c, 0x84, 0x78),
        }
    }

    pub fn apply(&self, ctx: &egui::Context) {
        let mut fonts = FontDefinitions::empty();
        fonts.font_data.insert("saira".into(), Arc::new(FontData::from_static(FONT)));
        fonts.families.insert(FontFamily::Proportional, vec!["saira".into()]);
        fonts.families.insert(FontFamily::Monospace, vec!["saira".into()]);
        ctx.set_fonts(fonts);

        let mut v = egui::Visuals::dark();
        v.panel_fill = self.bg;
        v.window_fill = Color32::from_rgb(0x1d, 0x1a, 0x17);
        v.extreme_bg_color = self.bar;
        v.override_text_color = Some(self.text);
        v.selection.bg_fill = self.accent.gamma_multiply(0.55);
        v.selection.stroke.color = self.text;
        v.widgets.inactive.weak_bg_fill = self.track;
        v.widgets.inactive.bg_fill = self.track;
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x3a, 0x35, 0x30);
        v.widgets.hovered.bg_stroke.color = self.accent;
        v.widgets.active.weak_bg_fill = Color32::from_rgb(0x3a, 0x35, 0x30);
        v.widgets.open.weak_bg_fill = self.track;
        ctx.set_visuals(v);
        ctx.style_mut(|s| {
            s.interaction.tooltip_delay = 0.4;
            for f in s.text_styles.values_mut() {
                f.size = (f.size + 1.0).min(15.0);
            }
        });
    }
}
