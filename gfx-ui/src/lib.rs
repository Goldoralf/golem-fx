// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-ui — the shared editor of the golem-fx plugins (design: README.md).
//!
//! A plugin gives its parameters and its bundle; the editor draws the pedal with live controls, the
//! expert panel, the presets and the meters:
//!
//! ```ignore
//! fn editor(&mut self, _: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
//!     gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-boost"))
//! }
//! ```

pub mod bundle;
mod entry;
mod face;
mod panel;
mod params;
mod presets;
pub mod raster;
mod theme;

pub use bundle::{Bundle, Header, MeterKind};
pub use params::Binding;

use face::FaceView;
use nih_plug::prelude::{Editor, Params};
use nih_plug_egui::egui::{self, vec2, Align, Layout};
use nih_plug_egui::{create_egui_editor, EguiState};
use entry::Entry;
use params::{Held, Setter};
use presets::PresetBar;
use std::sync::Arc;
use std::time::Duration;
use theme::Theme;

/// The bundle of a crate, embedded in the plugin: `gfx_ui::bundle!("gfx-boost")`.
#[macro_export]
macro_rules! bundle {
    ($krate:literal) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../gfx-ui/bundles/", $krate, ".gfxui")) as &'static [u8]
    };
}

const BAR: f32 = 38.0;
/// A meter row under the pedal, and the margin around the rows.
const ROW: f32 = 22.0;
const METERS_PAD: f32 = 10.0;

/// The window's size in logical pixels, from the bundle's header alone.
pub fn window_size(head: Header) -> (u32, u32) {
    let w = head.width as f32 * face::UNIT + if head.panel { panel::WIDTH } else { 0.0 };
    let meters = if head.rows > 0 { head.rows as f32 * ROW + 2.0 * METERS_PAD } else { 0.0 };
    let h = head.height as f32 * face::UNIT + BAR + meters;
    (w.max(300.0).ceil() as u32, h.ceil() as u32)
}

/// What the editor keeps from one opening to the next: the parsed bundle (read at the first opening,
/// not when the plugin loads: golem's engine never opens an editor) and the gesture left open.
struct State {
    bytes: &'static [u8],
    binding: Binding,
    held: Option<Held>,
    ui: Option<Result<Ui, String>>,
}

struct Ui {
    bundle: Bundle,
    theme: Theme,
    sections: Vec<(String, Vec<String>)>,
    face: FaceView,
    presets: PresetBar,
    entry: Option<Entry>,
}

impl Ui {
    fn new(bytes: &[u8], binding: &Binding) -> Result<Self, String> {
        let bundle = Bundle::parse(bytes)?;
        // The bundle's ids are the plugin's `#[id]`s; one that is not hides its control.
        let missing: Vec<&String> = bundle.params.order.iter().filter(|id| binding.get(id).is_none()).collect();
        debug_assert!(missing.is_empty(), "{}: the bundle names parameters the plugin does not have: {missing:?}", bundle.krate);
        Ok(Self {
            theme: Theme::new(&bundle.face.accent),
            sections: panel::sections(&bundle),
            face: FaceView::default(),
            presets: PresetBar::default(),
            entry: None,
            bundle,
        })
    }
}

/// The editor of a plugin, or `None` if its bundle cannot be read (the host then shows its own).
pub fn editor(params: Arc<dyn Params>, bundle: &'static [u8]) -> Option<Box<dyn Editor>> {
    let head = Header::read(bundle)?;
    let (w, h) = window_size(head);
    let state = State { bytes: bundle, binding: Binding::new(params), held: None, ui: None };
    create_egui_editor(
        EguiState::from_size(w, h),
        state,
        |ctx, state| {
            // The window has a fixed size: egui's own Ctrl +/- zoom would overflow it.
            ctx.options_mut(|o| o.zoom_with_keyboard = false);
            let ui = state.ui.get_or_insert_with(|| Ui::new(state.bytes, &state.binding));
            if let Ok(ui) = ui {
                // Textures belong to the window that has just opened.
                ui.face.forget();
                ui.entry = None;
                ui.theme.apply(ctx);
            }
        },
        |ctx, setter, state| {
            let setter = Setter(setter.raw_context);
            // A gesture ends with the button, wherever the pointer is, and at the next opening if the
            // window closed under it.
            if state.held.is_some() && !ctx.input(|i| i.pointer.any_down()) {
                Held::release(&mut state.held, setter);
            }
            match &mut state.ui {
                Some(Ok(ui)) => draw(ctx, setter, &state.binding, &mut state.held, ui),
                Some(Err(e)) => {
                    egui::CentralPanel::default().show(ctx, |ui| ui.label(format!("This editor cannot be shown ({e}). The host's own controls still work.")));
                }
                None => {}
            }
        },
    )
}

fn draw(ctx: &egui::Context, setter: Setter, binding: &Binding, held: &mut Option<Held>, ui: &mut Ui) {
    let Ui { bundle, theme, sections, face, presets, entry } = ui;
    egui::TopBottomPanel::top("bar")
        .exact_height(BAR)
        .frame(egui::Frame::new().fill(theme.bar).inner_margin(egui::Margin::symmetric(10, 0)))
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                let name = bundle.name.strip_prefix("GoleM FX ").unwrap_or(&bundle.name);
                ui.label(egui::RichText::new(name.to_uppercase()).size(16.0).color(theme.accent));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Laid out left to right (previous, list, next) inside the right-aligned slot.
                    let width = ui.available_width().min(presets::WIDTH);
                    ui.allocate_ui_with_layout(vec2(width, BAR), Layout::left_to_right(Align::Center), |ui| {
                        presets.show(ui, bundle, binding, setter, theme)
                    });
                });
            });
        });
    let face_size = FaceView::size(bundle, 1.0);
    egui::SidePanel::left("face")
        .exact_width(face_size.x)
        .resizable(false)
        .frame(egui::Frame::new().fill(theme.bar))
        .show_separator_line(false)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            face.show(ui, bundle, binding, setter, held, entry, theme, 1.0);
            if !bundle.meters.is_empty() {
                ui.add_space(METERS_PAD);
                meters(ui, bundle, binding, theme);
                // The meters move with the audio, not with the pointer: about 30 frames a second.
                ctx.request_repaint_after(Duration::from_millis(33));
            }
        });
    egui::CentralPanel::default().frame(egui::Frame::new().fill(theme.bg).inner_margin(egui::Margin::same(8))).show(ctx, |ui| {
        if !sections.is_empty() {
            panel::show(ui, bundle, binding, setter, held, entry, theme, sections);
        }
    });
}

/// A MIDI note number as a name (60 = C4); 0 is no note.
fn note_name(n: f32) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let n = n.round() as i32;
    if n <= 0 { "—".into() } else { format!("{}{}", NAMES[(n % 12) as usize], n / 12 - 1) }
}

/// The plugin's output parameters, one row each (gfx-ui/meters.json says how).
fn meters(ui: &mut egui::Ui, bundle: &Bundle, binding: &Binding, theme: &Theme) {
    const LABEL: f32 = 96.0;
    const VALUE: f32 = 70.0;
    let small = egui::FontId::proportional(13.0);
    for meter in &bundle.meters {
        let params: Vec<_> = meter.ids.iter().map(|id| binding.get(id)).collect();
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW * meter.rows() as f32), egui::Sense::hover());
        let Some(Some(first)) = params.first().copied() else { continue };
        let row = rect.shrink2(vec2(14.0, 0.0));
        let painter = ui.painter_at(rect);
        let mid_y = row.top() + ROW / 2.0;
        painter.text(egui::pos2(row.left(), mid_y), egui::Align2::LEFT_CENTER, &meter.label, small.clone(), theme.dim);
        let area = egui::Rect::from_min_max(egui::pos2(row.left() + LABEL, row.top()), row.max);
        let span = (meter.max - meter.min).max(f32::EPSILON);
        let at = |v: f32| ((v - meter.min) / span).clamp(0.0, 1.0);
        let lamp = |on: bool, x: f32| {
            let c = egui::pos2(x + 6.0, mid_y);
            painter.circle_filled(c, 5.5, if on { theme.accent } else { theme.track });
            if on {
                painter.circle_filled(c, 9.0, theme.accent.gamma_multiply(0.18));
            }
        };
        let hover = match meter.kind {
            MeterKind::Gr | MeterKind::Bar => {
                let bar = egui::Rect::from_min_max(egui::pos2(area.left(), mid_y - 4.0), egui::pos2(area.right() - VALUE, mid_y + 4.0));
                painter.rect_filled(bar, 2.0, theme.track);
                let v = first.plain();
                let (a, b) = if meter.center { let z = at(0.0f32.clamp(meter.min, meter.max)); let t = at(v); (z.min(t), z.max(t)) } else { (0.0, at(v)) };
                let fill = egui::Rect::from_min_max(egui::pos2(bar.left() + bar.width() * a, bar.top()), egui::pos2(bar.left() + bar.width() * b, bar.bottom()));
                painter.rect_filled(fill, 2.0, theme.accent);
                if meter.center {
                    let x = bar.left() + bar.width() * at(0.0f32.clamp(meter.min, meter.max));
                    painter.line_segment([egui::pos2(x, bar.top() - 3.0), egui::pos2(x, bar.bottom() + 3.0)], egui::Stroke::new(1.0_f32, theme.dim));
                }
                let text = first.text(first.normalized());
                painter.text(egui::pos2(area.right(), mid_y), egui::Align2::RIGHT_CENTER, &text, small.clone(), theme.text);
                text
            }
            MeterKind::Lamp => {
                let on = first.plain() >= 0.5;
                lamp(on, area.left());
                (if on { "on" } else { "off" }).to_string()
            }
            MeterKind::State => {
                let i = first.plain().round().max(0.0) as usize;
                let name = meter.states.get(i).cloned().unwrap_or_else(|| i.to_string());
                lamp(i > 0, area.left());
                painter.text(egui::pos2(area.left() + 22.0, mid_y), egui::Align2::LEFT_CENTER, &name, small.clone(), theme.text);
                name
            }
            MeterKind::Value => {
                let text = match meter.format.as_deref() {
                    Some("midi") => note_name(first.plain()),
                    _ => first.text(first.normalized()),
                };
                painter.text(egui::pos2(area.left(), mid_y), egui::Align2::LEFT_CENTER, &text, small.clone(), theme.text);
                text
            }
            MeterKind::Bands => {
                let n = params.len().max(1) as f32;
                let w = area.width() / n;
                let top = area.top() + 4.0;
                let bottom = area.bottom() - 16.0;
                let zero = top + (bottom - top) * (1.0 - at(0.0));
                let mut parts = Vec::new();
                for (k, p) in params.iter().enumerate() {
                    let x = area.left() + w * k as f32 + w / 2.0;
                    let col = egui::Rect::from_min_max(egui::pos2(x - 4.0, top), egui::pos2(x + 4.0, bottom));
                    painter.rect_filled(col, 2.0, theme.track);
                    if let Some(p) = p {
                        let y = top + (bottom - top) * (1.0 - at(p.plain()));
                        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(col.left(), y.min(zero)), egui::pos2(col.right(), y.max(zero))), 2.0, theme.accent);
                        parts.push(format!("{} {}", meter.labels.get(k).map_or("", |s| s.as_str()), p.text(p.normalized())));
                    }
                    painter.line_segment([egui::pos2(col.left() - 2.0, zero), egui::pos2(col.right() + 2.0, zero)], egui::Stroke::new(1.0_f32, theme.dim));
                    if let Some(l) = meter.labels.get(k) {
                        painter.text(egui::pos2(x, area.bottom() - 2.0), egui::Align2::CENTER_BOTTOM, l, egui::FontId::proportional(11.0), theme.dim);
                    }
                }
                parts.join("\n")
            }
        };
        resp.on_hover_text(format!("{}  {}", meter.label, hover));
    }
}
