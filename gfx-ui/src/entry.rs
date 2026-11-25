// SPDX-License-Identifier: GPL-3.0-or-later
//! A value typed in: a right click on a knob or a slider opens a field, Enter sets what it reads.

use crate::params::{Param, Setter};
use nih_plug_egui::egui::{self, Rect};

pub struct Entry {
    /// The widget the field belongs to.
    id: egui::Id,
    param: Param,
    text: String,
    fresh: bool,
}

impl Entry {
    /// A field for the widget `id`, holding the parameter's value as the plugin prints it, unit left out.
    pub fn open(id: egui::Id, param: Param) -> Self {
        Self { id, param, text: param.bare_text(param.normalized()), fresh: true }
    }

    /// Shows the open field under its widget, if `id` is that widget. Enter sets the value (as the
    /// plugin reads it, so "1.5k" or "-6 dB" work where the plugin's own text does); Escape or a click
    /// elsewhere leaves it.
    pub fn show(slot: &mut Option<Entry>, ctx: &egui::Context, id: egui::Id, widget: Rect, setter: Setter) {
        let Some(entry) = slot.as_mut().filter(|e| e.id == id) else { return };
        let mut close = false;
        egui::Area::new(id.with("entry")).order(egui::Order::Foreground).fixed_pos(widget.left_bottom()).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let field = ui.add(egui::TextEdit::singleline(&mut entry.text).desired_width(84.0));
                if entry.fresh {
                    entry.fresh = false;
                    field.request_focus();
                    // Selected whole: typing replaces what the plugin printed.
                    let mut state = egui::TextEdit::load_state(ui.ctx(), field.id).unwrap_or_default();
                    let end = egui::text::CCursor::new(entry.text.chars().count());
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), end)));
                    state.store(ui.ctx(), field.id);
                } else if field.lost_focus() {
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if let Some(n) = entry.param.parse(&entry.text) {
                            setter.once(entry.param, n);
                        }
                    }
                    close = true;
                }
            });
        });
        if close {
            *slot = None;
        }
    }
}
