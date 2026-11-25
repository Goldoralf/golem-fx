// SPDX-License-Identifier: GPL-3.0-or-later
//! The factory presets: load one, step through them, and name the one the current values match.

use crate::bundle::{Bundle, Preset};
use crate::params::{Binding, Param, Setter};
use crate::theme::Theme;
use nih_plug_egui::egui::{self, vec2, Sense, Stroke};

/// A preset's value for a parameter, normalised: a number in the parameter's unit, a boolean, or an
/// option's name (or its index).
pub fn target(bundle: &Bundle, id: &str, value: &serde_json::Value, p: Param) -> Option<f32> {
    Some(p.to_normalized(bundle.params.get(id)?.plain(value)?))
}

/// The bar's width at most: both arrows, the list, the spacing between them. A narrow pedal's window
/// leaves less, and the list takes what is left.
pub const WIDTH: f32 = 252.0;
const ARROW: f32 = 20.0;

/// Whether a parameter stands at `normalized`, as far as its printed value tells.
fn same(p: Param, normalized: f32) -> bool {
    let d = (normalized - p.normalized()).abs();
    d < 1e-6 || (d < 0.02 && p.text(normalized) == p.text(p.normalized()))
}

#[derive(Default)]
pub struct PresetBar {
    /// The parameters' stamp the match was computed for.
    stamp: Option<u64>,
    current: Option<usize>,
    /// The preset loaded last: two presets may hold the same values, and the bar names this one.
    loaded: Option<usize>,
}

impl PresetBar {
    pub fn load(bundle: &Bundle, binding: &Binding, setter: Setter, preset: &Preset) {
        for (id, value) in &preset.params {
            let Some(p) = binding.get(id).filter(|p| !p.is_bypass()) else { continue };
            if let Some(n) = target(bundle, id, value, p) {
                if !same(p, n) {
                    setter.once(p, n);
                }
            }
        }
    }

    fn matches(bundle: &Bundle, binding: &Binding, preset: &Preset) -> bool {
        preset.params.iter().all(|(id, value)| match binding.get(id).filter(|p| !p.is_bypass()) {
            Some(p) => target(bundle, id, value, p).is_some_and(|n| same(p, n)),
            None => true,
        })
    }

    /// The preset the current values are: the one loaded last if it still holds, or the first that does.
    fn matching(&self, bundle: &Bundle, binding: &Binding) -> Option<usize> {
        let holds = |i: &usize| Self::matches(bundle, binding, &bundle.presets[*i]);
        self.loaded.filter(holds).or_else(|| (0..bundle.presets.len()).find(holds))
    }

    pub fn show(&mut self, ui: &mut egui::Ui, bundle: &Bundle, binding: &Binding, setter: Setter, theme: &Theme) {
        if bundle.presets.is_empty() {
            return;
        }
        let stamp = binding.stamp(&bundle.params.order);
        if self.stamp != Some(stamp) {
            self.stamp = Some(stamp);
            self.current = self.matching(bundle, binding);
        }
        let count = bundle.presets.len();
        let list = (ui.available_width() - 2.0 * (ARROW + ui.spacing().item_spacing.x)).max(80.0);
        let mut load = None;
        if arrow(ui, false, theme).clicked() {
            load = Some(self.current.map_or(count - 1, |c| (c + count - 1) % count));
        }
        let several = {
            let first = &bundle.presets[0].block;
            bundle.presets.iter().any(|p| &p.block != first)
        };
        let name = self.current.map_or("—", |c| bundle.presets[c].name.as_str());
        let combo = egui::ComboBox::from_id_salt("presets").width(list).height(420.0).selected_text(name).show_ui(ui, |ui| {
            let mut block: Option<&Option<String>> = None;
            for (i, preset) in bundle.presets.iter().enumerate() {
                if several && block != Some(&preset.block) {
                    block = Some(&preset.block);
                    ui.label(egui::RichText::new(preset.block.as_deref().unwrap_or("")).small().color(theme.dim));
                }
                let r = ui.selectable_label(self.current == Some(i), &preset.name);
                let r = match &preset.desc {
                    Some(d) => r.on_hover_text(d),
                    None => r,
                };
                if r.clicked() {
                    load = Some(i);
                }
            }
        });
        if let Some(desc) = self.current.and_then(|c| bundle.presets[c].desc.as_ref()) {
            combo.response.on_hover_text(desc);
        }
        if arrow(ui, true, theme).clicked() {
            load = Some(self.current.map_or(0, |c| (c + 1) % count));
        }
        if let Some(i) = load {
            Self::load(bundle, binding, setter, &bundle.presets[i]);
            self.loaded = Some(i);
            self.stamp = None;
        }
    }
}

/// A triangle button (drawn: the chrome's font has no arrows).
fn arrow(ui: &mut egui::Ui, right: bool, theme: &Theme) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ARROW, ARROW), Sense::click());
    let c = rect.center();
    let s = if right { 4.5 } else { -4.5 };
    let color = if resp.hovered() { theme.accent } else { theme.text };
    ui.painter().add(egui::Shape::convex_polygon(
        vec![egui::pos2(c.x - s, c.y - 5.5), egui::pos2(c.x + s, c.y), egui::pos2(c.x - s, c.y + 5.5)],
        color,
        Stroke::NONE,
    ));
    resp
}
