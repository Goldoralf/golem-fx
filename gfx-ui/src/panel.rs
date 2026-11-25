// SPDX-License-Identifier: GPL-3.0-or-later
//! The expert panel: every parameter that is not on the face, by the descriptor's groups.

use crate::bundle::{Bundle, Depends, ParamInfo, ParamKind};
use crate::entry::Entry;
use crate::params::{Binding, Held, Param, Setter};
use crate::theme::Theme;
use nih_plug_egui::egui::{self, vec2, Align, Color32, CornerRadius, Layout, PointerButton, Sense, Stroke};

pub const WIDTH: f32 = 400.0;
const LABEL: f32 = 132.0;
const ROW: f32 = 22.0;

/// Whether a parameter's `depends` holds for the current values.
pub fn enabled(bundle: &Bundle, binding: &Binding, depends: &Option<Depends>) -> bool {
    let Some(d) = depends else { return true };
    let (Some(info), Some(p)) = (bundle.params.get(&d.param), binding.get(&d.param)) else { return true };
    let cur = p.plain().round();
    match &d.equals {
        serde_json::Value::Bool(b) => (cur > 0.5) == *b,
        serde_json::Value::Number(n) => n.as_f64().is_some_and(|n| (n as f32 - p.plain()).abs() < 1e-6),
        serde_json::Value::String(s) => info
            .options
            .as_ref()
            .and_then(|o| o.get(cur.max(0.0) as usize))
            .is_some_and(|o| o.eq_ignore_ascii_case(s)),
        _ => true,
    }
}

/// The sections: the front parameters the face does not carry, the groups, then what is in none.
pub fn sections(bundle: &Bundle) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let mut add = |name: &str, ids: Vec<&String>| {
        let ids: Vec<String> = ids.into_iter().filter(|id| bundle.params.get(id).is_some() && !bundle.on_face(id)).cloned().collect();
        if !ids.is_empty() {
            out.push((name.to_string(), ids));
        }
    };
    add("Main", bundle.front.iter().collect());
    for g in &bundle.groups {
        add(&g.name, g.params.iter().filter(|id| !bundle.front.contains(id)).collect());
    }
    let grouped = |id: &String| bundle.front.contains(id) || bundle.groups.iter().any(|g| g.params.contains(id));
    add("Other", bundle.params.order.iter().filter(|id| !grouped(id)).collect());
    out
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    bundle: &Bundle,
    binding: &Binding,
    setter: Setter,
    held: &mut Option<Held>,
    entry: &mut Option<Entry>,
    theme: &Theme,
    sections: &[(String, Vec<String>)],
) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        for (i, (name, ids)) in sections.iter().enumerate() {
            egui::CollapsingHeader::new(egui::RichText::new(name).strong().color(theme.text))
                .id_salt(("group", i))
                .default_open(i == 0 || sections.len() <= 2)
                .show(ui, |ui| {
                    for id in ids {
                        let (Some(info), Some(p)) = (bundle.params.get(id), binding.get(id)) else { continue };
                        let on = enabled(bundle, binding, &info.depends);
                        ui.add_enabled_ui(on, |ui| row(ui, id, name, info, p, setter, held, entry, theme));
                    }
                });
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut egui::Ui,
    id: &str,
    section: &str,
    info: &ParamInfo,
    p: Param,
    setter: Setter,
    held: &mut Option<Held>,
    entry: &mut Option<Entry>,
    theme: &Theme,
) {
    // "Low Mid Gain" under "Low Mid" reads "Gain".
    let name = info.name.strip_prefix(section).map(str::trim_start).filter(|rest| !rest.is_empty()).unwrap_or(&info.name);
    ui.horizontal(|ui| {
        ui.set_height(ROW);
        let label = ui.allocate_ui_with_layout(vec2(LABEL, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(LABEL);
            ui.set_max_width(LABEL);
            ui.add(egui::Label::new(egui::RichText::new(name).color(theme.text)).truncate())
        });
        if let Some(help) = &info.help {
            label.response.on_hover_text(help);
        }
        match info.kind {
            ParamKind::Bool => {
                let mut on = p.plain() > 0.5;
                if switch(ui, &mut on, theme).clicked() {
                    setter.once(p, if on { 1.0 } else { 0.0 });
                }
            }
            ParamKind::Enum => {
                let options = info.options.as_deref().unwrap_or(&[]);
                let cur = p.plain().round().max(0.0) as usize;
                egui::ComboBox::from_id_salt(("enum", id))
                    .width(ui.available_width() - 8.0)
                    .selected_text(options.get(cur).map_or("", |s| s.as_str()))
                    .show_ui(ui, |ui| {
                        for (i, name) in options.iter().enumerate() {
                            if ui.selectable_label(i == cur, name).clicked() && i != cur {
                                setter.once(p, p.to_normalized(i as f32));
                            }
                        }
                    });
            }
            ParamKind::Float => slider(ui, info, p, setter, held, entry, theme),
        }
    });
}

fn switch(ui: &mut egui::Ui, on: &mut bool, theme: &Theme) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(34.0, 16.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool(resp.id, *on);
    let fade = if ui.is_enabled() { 1.0 } else { 0.4 };
    let bg = if *on { theme.accent } else { theme.track };
    ui.painter().rect_filled(rect, CornerRadius::same(8), bg.gamma_multiply(fade));
    let x = egui::lerp((rect.left() + 8.0)..=(rect.right() - 8.0), t);
    ui.painter().circle_filled(egui::pos2(x, rect.center().y), 6.0, Color32::from_gray(235).gamma_multiply(fade));
    resp
}

/// A bar over the parameter's travel (its taper), its value printed on it. Drag, click, double click for
/// the default, Shift for a drag ten times finer, right click to type the value.
fn slider(ui: &mut egui::Ui, info: &ParamInfo, p: Param, setter: Setter, held: &mut Option<Held>, entry: &mut Option<Entry>, theme: &Theme) {
    let width = (ui.available_width() - 8.0).max(60.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 18.0), Sense::click_and_drag());
    let under = |pos: egui::Pos2| ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    if resp.drag_started_by(PointerButton::Primary) {
        Held::grab(held, setter, p, info.position(p.normalized()));
    }
    if resp.dragged_by(PointerButton::Primary) {
        if let (Some(h), Some(pos)) = (held.as_mut().filter(|h| h.param == p), resp.interact_pointer_pos()) {
            let to = if ui.input(|i| i.modifiers.shift) { (h.at + resp.drag_delta().x / rect.width() * 0.1).clamp(0.0, 1.0) } else { under(pos) };
            if to != h.at {
                h.at = to;
                setter.set(p, info.normalized(to));
            }
        }
    }
    if resp.double_clicked() {
        setter.once(p, p.default_normalized());
    } else if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            setter.once(p, info.normalized(under(pos)));
        }
    }
    if resp.secondary_clicked() {
        *entry = Some(Entry::open(resp.id, p));
    }
    // Drawn where the pointer holds it: the plugin's own value may follow a block later.
    let at = held.as_ref().filter(|h| h.param == p).map_or_else(|| info.position(p.normalized()), |h| h.at);
    let fade = if ui.is_enabled() { 1.0 } else { 0.4 };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(3), theme.track.gamma_multiply(fade));
    let mut fill = rect;
    fill.set_width(rect.width() * at);
    painter.rect_filled(fill, CornerRadius::same(3), theme.accent.gamma_multiply(0.75 * fade));
    if resp.hovered() || resp.dragged() {
        painter.rect_stroke(rect, CornerRadius::same(3), Stroke::new(1.0_f32, theme.accent), egui::StrokeKind::Inside);
    }
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, p.text(info.normalized(at)), egui::FontId::proportional(12.5), theme.text.gamma_multiply(fade));
    Entry::show(entry, ui.ctx(), resp.id, rect, setter);
}
