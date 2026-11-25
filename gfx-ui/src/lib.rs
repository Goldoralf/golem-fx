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

pub use bundle::{Bundle, Header};
pub use params::Binding;

use face::FaceView;
use nih_plug::prelude::{Editor, Params};
use nih_plug_egui::egui::{self, vec2, Align, Layout};
use nih_plug_egui::{create_egui_editor, EguiState};
use entry::Entry;
use params::{Held, Setter};
use presets::PresetBar;
use std::sync::Arc;
use theme::Theme;

/// The bundle of a crate, embedded in the plugin: `gfx_ui::bundle!("gfx-boost")`.
#[macro_export]
macro_rules! bundle {
    ($krate:literal) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../gfx-ui/bundles/", $krate, ".gfxui")) as &'static [u8]
    };
}

const BAR: f32 = 38.0;
const METERS: f32 = 26.0;

/// The window's size in logical pixels, from the bundle's header alone.
pub fn window_size(head: Header) -> (u32, u32) {
    let w = head.width as f32 * face::UNIT + if head.panel { panel::WIDTH } else { 0.0 };
    let h = head.height as f32 * face::UNIT + BAR;
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
            meters(ui, bundle, binding, theme);
        });
    egui::CentralPanel::default().frame(egui::Frame::new().fill(theme.bg).inner_margin(egui::Margin::same(8))).show(ctx, |ui| {
        if !sections.is_empty() {
            panel::show(ui, bundle, binding, setter, held, entry, theme, sections);
        }
    });
}

/// The plugin's output parameters as bars over their own range.
fn meters(ui: &mut egui::Ui, bundle: &Bundle, binding: &Binding, theme: &Theme) {
    for meter in &bundle.meters {
        let Some(p) = binding.get(&meter.id) else { continue };
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), METERS), egui::Sense::hover());
        let bar = rect.shrink2(vec2(10.0, 8.0));
        ui.painter().rect_filled(bar, 2.0, theme.track);
        let mut fill = bar;
        fill.set_width(bar.width() * p.normalized().clamp(0.0, 1.0));
        ui.painter().rect_filled(fill, 2.0, theme.accent);
        resp.on_hover_text(format!("{}  {}", meter.name, p.text(p.normalized())));
    }
}
