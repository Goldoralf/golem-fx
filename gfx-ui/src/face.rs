// SPDX-License-Identifier: GPL-3.0-or-later
//! The pedal, live: the body drawn once, each control's frame drawn on first use and kept as a texture.

use crate::bundle::{Bundle, ControlKind, ParamKind};
use crate::entry::Entry;
use crate::params::{Binding, Held, Setter};
use crate::raster;
use crate::theme::Theme;
use nih_plug_egui::egui::{self, pos2, vec2, Color32, ColorImage, PointerButton, Rect, Sense, TextureHandle, TextureOptions};
use std::collections::HashMap;

/// Points per viewBox unit at 100 %.
pub const UNIT: f32 = 3.0;
/// Points of vertical drag for a knob's whole sweep.
const SWEEP: f32 = 200.0;
/// Points of vertical drag from one option of a selector to the next.
const OPTION: f32 = 24.0;
/// Scroll points to a wheel notch (egui's line height).
const NOTCH: f32 = 40.0;

#[derive(Default)]
pub struct FaceView {
    /// Physical pixels per viewBox unit the textures were drawn at.
    px_per_unit: f32,
    body: Option<TextureHandle>,
    frames: HashMap<(usize, usize), TextureHandle>,
    /// The selector a drag is turning, and where the drag stands in options.
    turn: Option<(usize, f32)>,
    /// The control under the wheel and the scroll it has not used yet.
    wheel: (usize, f32),
}

fn texture(ctx: &egui::Context, name: String, svg: &str, region: [i32; 4], px_per_unit: f32) -> Option<TextureHandle> {
    let img = raster::render(svg, region, px_per_unit).ok()?;
    let img = ColorImage::from_rgba_premultiplied([img.width, img.height], &img.rgba);
    Some(ctx.load_texture(name, img, TextureOptions::LINEAR))
}

impl FaceView {
    /// Textures belong to one window: forget them when the editor opens again.
    pub fn forget(&mut self) {
        *self = Self::default();
    }

    pub fn size(bundle: &Bundle, zoom: f32) -> egui::Vec2 {
        vec2(bundle.face.view_box[2], bundle.face.view_box[3]) * UNIT * zoom
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        bundle: &Bundle,
        binding: &Binding,
        setter: Setter,
        held: &mut Option<Held>,
        entry: &mut Option<Entry>,
        theme: &Theme,
        zoom: f32,
    ) {
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        let px_per_unit = UNIT * zoom * ppp;
        if (px_per_unit - self.px_per_unit).abs() > 1e-3 {
            self.body = None;
            self.frames.clear();
            self.px_per_unit = px_per_unit;
        }
        let vb = bundle.face.view_box;
        let (pw, ph) = raster::face_pixels(vb, px_per_unit);
        let (rect, _) = ui.allocate_exact_size(vec2(pw as f32, ph as f32) / ppp, Sense::hover());
        // On the pixel grid, so the layers meet the body exactly.
        let origin = pos2((rect.min.x * ppp).round() / ppp, (rect.min.y * ppp).round() / ppp);
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));

        if self.body.is_none() {
            self.body = texture(&ctx, format!("{}-body", bundle.krate), &bundle.face.body, [0, 0, pw, ph], px_per_unit);
        }
        if let Some(body) = &self.body {
            ui.painter().image(body.id(), Rect::from_min_size(origin, vec2(pw as f32, ph as f32) / ppp), uv, Color32::WHITE);
        }

        for (n, control) in bundle.face.controls.iter().enumerate() {
            let (Some(info), Some(p)) = (bundle.params.get(&control.param), binding.get(&control.param)) else { continue };
            let px = raster::pixel_box(vb, control.bbox, px_per_unit);
            let at = Rect::from_min_size(origin + vec2(px[0] as f32, px[1] as f32) / ppp, vec2(px[2] as f32, px[3] as f32) / ppp);

            // While the pointer holds the control it is drawn where the pointer put it: the plugin's
            // own value may follow a block later.
            let holding = held.as_ref().filter(|h| h.param == p).map(|h| h.at);
            let normalized = holding.map_or_else(|| p.normalized(), |t| info.normalized(t));
            let frame = control.frame(info.kind, normalized, p.plain());
            if let std::collections::hash_map::Entry::Vacant(slot) = self.frames.entry((n, frame)) {
                if let Some(t) = texture(&ctx, format!("{}-{}-{}", bundle.krate, control.param, frame), &control.frames[frame], px, px_per_unit) {
                    slot.insert(t);
                }
            }
            if let Some(t) = self.frames.get(&(n, frame)) {
                ui.painter().image(t.id(), at, uv, Color32::WHITE);
            }

            let continuous = info.kind == ParamKind::Float && matches!(control.kind, ControlKind::Knob | ControlKind::Fader);
            let count = info.options.as_ref().map_or(2, |o| o.len()).max(2);
            let sense = if continuous || count > 2 { Sense::click_and_drag() } else { Sense::click() };
            let id = ui.id().with(("ctl", n));
            let resp = ui.interact(at, id, sense);
            let notch = if resp.hovered() { self.notches(ui, n) } else { 0 };

            if continuous {
                if resp.drag_started_by(PointerButton::Primary) {
                    Held::grab(held, setter, p, info.position(p.normalized()));
                }
                if resp.dragged_by(PointerButton::Primary) {
                    if let Some(h) = held.as_mut().filter(|h| h.param == p) {
                        let fine = if ui.input(|i| i.modifiers.shift) { 0.1 } else { 1.0 };
                        let to = (h.at - resp.drag_delta().y / SWEEP * fine).clamp(0.0, 1.0);
                        if to != h.at {
                            h.at = to;
                            setter.set(p, info.normalized(to));
                        }
                    }
                }
                if resp.double_clicked() {
                    setter.once(p, p.default_normalized());
                }
                if notch != 0 {
                    let fine = if ui.input(|i| i.modifiers.shift) { 0.002 } else { 0.02 };
                    let to = (info.position(p.normalized()) + notch as f32 * fine).clamp(0.0, 1.0);
                    setter.once(p, info.normalized(to));
                }
                if resp.secondary_clicked() {
                    *entry = Some(Entry::open(id, p));
                }
                Entry::show(entry, &ctx, id, at, setter);
            } else {
                // A selector or a toggle: one option per click, per notch, per stretch of drag.
                let cur = p.plain().round().max(0.0) as usize;
                let mut next = None;
                if resp.clicked() {
                    next = Some((cur + 1) % count);
                }
                if notch != 0 {
                    next = Some((cur as i32 + notch).clamp(0, count as i32 - 1) as usize);
                }
                if resp.drag_started_by(PointerButton::Primary) {
                    self.turn = Some((n, cur as f32));
                }
                if resp.dragged_by(PointerButton::Primary) {
                    if let Some((_, v)) = self.turn.as_mut().filter(|t| t.0 == n) {
                        *v = (*v - resp.drag_delta().y / OPTION).clamp(0.0, count as f32 - 1.0);
                        next = Some(v.round() as usize);
                    }
                }
                if resp.drag_stopped() {
                    self.turn = None;
                }
                if let Some(options) = info.options.as_ref().filter(|o| o.len() > 2) {
                    resp.context_menu(|ui| {
                        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                            for (i, name) in options.iter().enumerate() {
                                if ui.selectable_label(i == cur, name).clicked() {
                                    setter.once(p, p.to_normalized(i as f32));
                                    ui.close_menu();
                                }
                            }
                        });
                    });
                }
                if let Some(to) = next.filter(|to| *to != cur) {
                    setter.once(p, p.to_normalized(to as f32));
                }
            }

            let text = format!("{}  {}", info.name, p.text(normalized));
            if holding.is_some() || self.turn.is_some_and(|t| t.0 == n) {
                // egui shows no tooltip under a pressed button: the value is written by hand.
                readout(&ctx, at, &text, theme);
            } else if held.is_none() && self.turn.is_none() {
                let help = info.help.clone();
                resp.on_hover_ui_at_pointer(|ui| {
                    ui.set_max_width(260.0);
                    ui.strong(text);
                    if let Some(help) = help {
                        ui.label(help);
                    }
                });
            }
        }
    }

    /// Whole wheel notches over the control `n` since the last call (smooth scrolling adds up to them;
    /// with Shift held the wheel comes on the other axis).
    fn notches(&mut self, ui: &egui::Ui, n: usize) -> i32 {
        if self.wheel.0 != n {
            self.wheel = (n, 0.0);
        }
        self.wheel.1 += ui.input(|i| i.raw_scroll_delta.x + i.raw_scroll_delta.y);
        let whole = (self.wheel.1 / NOTCH).trunc();
        self.wheel.1 -= whole * NOTCH;
        whole as i32
    }
}

/// A control's name and value above it (under it when there is no room), over everything.
fn readout(ctx: &egui::Context, control: Rect, text: &str, theme: &Theme) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("gfx-readout")));
    let galley = painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(13.0), theme.text);
    let size = galley.size() + vec2(14.0, 8.0);
    let screen = ctx.screen_rect();
    let mut min = pos2(control.center().x - size.x / 2.0, control.top() - size.y - 2.0);
    if min.y < screen.top() + 2.0 {
        min.y = control.bottom() + 2.0;
    }
    min.x = min.x.clamp(screen.left() + 2.0, (screen.right() - size.x - 2.0).max(screen.left() + 2.0));
    let rect = Rect::from_min_size(min, size);
    painter.rect_filled(rect, 4.0, theme.bar.gamma_multiply(0.94));
    painter.galley(rect.min + vec2(7.0, 4.0), galley, theme.text);
}
