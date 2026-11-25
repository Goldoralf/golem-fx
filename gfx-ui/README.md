# gfx-ui — the shared editor of the golem-fx plugins

> **Status: PROTOTYPE** (2026-11-22) on gfx-boost and gfx-grit, seen running standalone under X11, not
> yet in a DAW. One editor for every golem-fx VST3 when it is opened in a DAW (REAPER, Ardour, Bitwig,
> Live…). golem's engine runs the plugins headless and never opens it. Not done yet: §10.

## 1. Goal

Today a golem-fx plugin has no editor: a DAW shows its own list of sliders, 50 of them for Grit. The
editor gives every plugin, with no code of its own:

1. **its pedal**, the drawing of `visuals/` (same finish, motif, knobs, selectors, toggles, faders), live:
   the controls of the face are played on the drawing;
2. **every other parameter** in an expert panel built from the descriptor (groups, units, options, help);
3. **the factory presets** of `presets/<crate>.json`, from the first version;
4. **its meters** (gain reduction, gate state…) where the plugin has output parameters.

A new pedal gets its editor from its face, its descriptor and its presets: one line in its `plugin.rs`.

Out of scope for v1: user presets saved by the editor (the DAW saves the plugin state and its own
presets), a footswitch that bypasses (bypass belongs to the host: the switch and the LED are drawn on),
MIDI learn, a free-resizable window (three fixed sizes instead).

## 2. What exists and is reused

| Source | Gives |
|---|---|
| `visuals/js/golem-pedals.js`, `gfxPedal(crate, {values})` | The pedal as SVG at given values; each control is a group `.gp-knob / .gp-selector / .gp-toggle / .gp-fader` with `data-param="<id>"` |
| `visuals/data/descriptors.json` | Per plugin: `front` (the face's parameters), `groups` (expert groups, `custom` switch), `params` (kind, range, default, unit, taper, step, decimals, options, `depends`, help), `meters`, `presets` |
| `presets/<crate>.json` | The factory presets (872 in all), values in the parameters' own units, enums by option name |
| nih-plug fork | `nih_plug_egui` (egui on baseview, OpenGL); `Params::param_map()` lists every parameter by id |

Measured on 2026-11-22 (node, `@resvg/resvg-js`, 3 px per unit): a full pedal rasterises in about 100 ms,
one control alone in 1 – 2 ms. Cutting a pedal into *body* (the drawing without its control groups) and
one *layer* per control, then compositing them, gives the full drawing back within 2 / 255 on 45 of the 57
pedals. The 12 others: five overlap by anti-aliasing only (≤ 8 / 255 on a few dozen pixels), six need the
group's ancestors kept (Blackwork and Clay faces draw controls inside a transformed group; the slider face
of gfx-geq), one to look at (gfx-freqshift). §4.2 fixes the method and makes the check a build failure.

## 3. Choice of technique

The pedal is redrawn **natively from the same SVG**, with the `resvg` crate (the engine `render-rig.js`
already uses through `resvg-js`, so both render alike).

| Rejected | Why |
|---|---|
| A web view running `golem-pedals.js` | A browser per plugin window, a system dependency on each OS (WebView2, WebKitGTK), fragile inside Linux hosts |
| PNG film strips rendered at build time | Pixel-safe, but about 1 MB per plugin (textured knobs compress badly), 57 plugins, and a blurred pedal at any other size |
| Porting the renderers to Rust | 250 KB of drawing code to keep in step by hand |

The JavaScript renderer cannot run in the plugin, so what depends on a value is **pre-computed as SVG
text**: for each control, its group drawn at N positions. SVG text is small (a control group is about
1.3 KB), nearly identical from one position to the next, and compresses about twenty times.

## 4. Data: one bundle per plugin

### 4.1 The file

`visuals/tools/build-ui.js` (node, run with the other visuals tools) writes `gfx-ui/bundles/<crate>.gfxui`
for every pedal: one JSON document, deflated, **committed** (generated, do not edit; `--check` verifies
they are current, as `build-descriptors.py --check` does). Committing them keeps node out of the Rust
build and out of the release builds. Expected size: 20 – 60 KB each.

```jsonc
{
  "version": 1,
  "crate": "gfx-grit", "name": "GoleM FX Grit", "class_id": "GoleMFxGrit00001",
  "face": {
    "view_box": [-6, -4, 124, 216],
    "defs": "<defs>…</defs>",          // the shared defs (GolemPedals.defs()) the drawing uses, text as paths
    "body": "<svg …>…</svg>",          // the pedal, on, without its control groups
    "controls": [
      { "param": "drive", "kind": "knob",     "bbox": [x, y, w, h], "frames": ["<svg…>", …] },   // 65 frames
      { "param": "quality", "kind": "toggle", "bbox": […],          "frames": [off, on] }
    ],
    "variants": null                   // gfx-geq: {"param": "layout", "faces": [face, face, face]}
  },
  "front": ["wet", "quality", "drive", "focus", "level"],
  "groups": [{ "id": "low", "name": "Low", "custom": null, "params": […] }, …],
  "params": { "<id>": { name, kind, min, max, default, unit, display, step, decimals, options, depends, help } },
  "meters": [{ "id": "gr", "name": "Gain Reduction", "min": 0, "max": 90 }],
  "presets": [{ "id", "type", "name", "desc", "default", "params": {…} }]   // refs, notes, tags left out
}
```

### 4.2 Cutting a pedal

- **Text becomes paths** in the tool (resvg-js writes the simplified tree back as SVG), so the plugin
  embeds no font and builds `resvg` without its text feature.
- **Body** = `gfxPedal(crate, {on: true})` minus every control group.
- **A frame** = the SVG header, the pedal's own defs, the opening tags of the group's ancestors (their
  transforms apply), the group at that value, the closing tags.
- **Positions**: a knob has 65 frames over its sweep (64 steps, under 5° each), a fader 49, a selector
  one per option, a toggle 2. The parameter itself is never quantised, only its drawing.
- **`bbox`** = the union of the frames' opaque pixels, in viewBox units, widened by 1.5 units: the
  control's hit area, and the rectangle its texture covers.
- **A face that changes with a parameter** (gfx-geq draws another set of sliders for each `layout`):
  one face per option, listed in `variants`; the editor shows the one the parameter selects.
- **The check, at build time**: for the defaults, both ends of every control and 8 random settings (each
  value on its frame), the tool rasterises `body + frames`, each frame cut to its `bbox` as the editor
  draws it, and the full `gfxPedal(crate, {values})`, and fails above 8 / 255 on more than 0.05 % of
  the pixels. A renderer that one day draws a value outside its control group (a
  readout elsewhere on the face) is caught here, not by a user.
- Parameter ids are checked against the descriptor; a control whose parameter does not exist fails.

## 5. The crate

```
gfx-ui/
  src/lib.rs        editor(), bundle!(), the Bundle types (serde)
  src/face.rs       resvg rasterising, texture cache, hit testing, control gestures
  src/panel.rs      the expert panel (groups, widgets, depends)
  src/presets.rs    the preset bar: apply, match, previous / next
  src/params.rs     descriptor id -> ParamPtr, plain <-> normalised, value text, the gesture held open
  src/entry.rs      a value typed in (right click on a knob or a slider)
  src/raster.rs     SVG to pixels, with no window
  tests/bundles.rs  every committed bundle is one the editor can show
  src/theme.rs      colours and fonts of the chrome
  bundles/          <crate>.gfxui (generated)
  examples/snapshot.rs   draws a bundle's face to a PNG without a window (the check against node)
```

Dependencies: `nih_plug`, `nih_plug_egui`, `resvg` (no default features), `miniz_oxide`, `serde`,
`serde_json`. The audio thread is not touched: the editor reads and writes parameters through
nih-plug's `ParamSetter` (begin / set / end gestures, so automation records).

In a plugin:

```rust
// Cargo.toml:  gui = ["plugin", "dep:gfx-ui"], not in the default features
#[cfg(feature = "gui")]
fn editor(&mut self, _: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
    gfx_ui::editor(self.params.clone(), gfx_ui::bundle!("gfx-boost"))
}
```

`bundle!` is `include_bytes!` of the crate's file, so a plugin embeds its own bundle only. The window
state lives in the editor, not in the plugin's `Params`: **the plugin's saved state does not change**,
golem's sessions and the descriptors stay as they are.

**The editor is opt-in** (decided by the maintainer, 2026-11-25): golem plays the pedals without it, it has
its own drawers. `cargo xtask bundle <crate> --release` and the release of golem-binaries build a pedal as
before, with no editor and no X11 / OpenGL to link. `--features gui` adds the editor; `cargo xtask dist`
builds every pedal with it into `dist/vst3/<platform>/` and packs them (`--headless`: the other build, into
`dist/vst3-headless/<platform>/`).

Binding is by id: the bundle's parameter ids are the plugin's `#[id]`s. At the editor's creation every
bundle id is looked up in `param_map()`; a debug build asserts none is missing, a release build hides
the control (never a panic in a host).

## 6. The window

```
┌───────────────────────────────────────────────────────────────┐
│ GoleM FX Grit      ◀  [ Grit ▾ ]  ▶          size ▾    ?     │  preset bar
├────────────────────┬──────────────────────────────────────────┤
│                    │ ▾ Low                                    │
│      the pedal     │   Module 1 Type  [Soft clip ▾]           │
│   (live controls)  │   Module 1 Amount ───●───── 24 %         │
│                    │ ▸ Low Mid                                │
│                    │ ▸ High Mid                               │
│  meters (if any)   │ ▸ Crossovers                             │
└────────────────────┴──────────────────────────────────────────┘
```

- **Size**: the pedal at 3 px per unit (372 × 648 for a compact stomp) plus a 400 px panel; the panel is
  left out when every parameter is on the face (Boost). Three sizes (100 / 125 / 150 %) in the bar, the
  drawing re-rasterised at each: always sharp. The host's DPI scale is honoured.
- **Pedal**: body rasterised once per size (about 100 ms, at opening); a control's frame rasterised on
  first use (1 – 2 ms) and cached as a texture, so a drag costs one texture switch.
  - knob, fader: vertical drag (200 px for the sweep, Shift = ten times finer), wheel, double-click =
    default, right-click = type a value;
  - selector: click = next option, right-click or wheel = the list;
  - toggle: click;
  - hover: name, value with unit, the help line.
  The plugins' ranges are linear and the pedal is drawn so; the hand follows the parameter's taper
  (`display: log`: by ratios, or a cubic law when the range starts at zero), on the face and in the panel.
  While a control is held its name and value are written above it, and it is drawn where the pointer
  put it (a host may apply the value a block later).
- **Expert panel**: one foldable section per group, open state kept while the window lives; a group's
  `custom` switch in its header; a parameter whose `depends` is not met is greyed, still readable.
  Float = slider with its value, enum = list, bool = switch. Parameters on the face are not repeated;
  parameters in no group go to a last section "Other".
- **Meters**: the plugin's output parameters, as bars under the pedal, 30 Hz.
- **Presets**: the list, grouped by block type when the plugin stands for several (Boost: `volume`,
  `clean-boost`), each with its description as a tooltip; ◀ ▶ step through. Loading sets every
  parameter the preset lists but the host's bypass, as one gesture per parameter (values already there
  are skipped), and leaves the others. The bar shows the
  preset whose values all match the current ones (within half a step), and `—` when none does: nothing
  is stored, so a session reopened shows the right name with no state of ours.
- **Chrome**: dark, the pedal's family colour as the accent, Saira Semi Condensed embedded once in gfx-ui
  (OFL) for the chrome's text.

## 7. Platforms

- Linux: baseview opens an X11 window (XWayland under Wayland), OpenGL. To test in Ardour 9 and REAPER.
- Windows, macOS: nih-plug's usual path; to test in REAPER on each.
- No network, no file access, no thread of its own; the editor allocates only on the GUI thread.

## 8. Checks

- `build-ui.js --check`: bundles current, cut check of §4.2.
- `cargo test -p gfx-ui`: every committed bundle parses; every preset value maps to a parameter and a
  value inside its range; frames count matches the control kind.
- `cargo run -p gfx-ui --example snapshot -- <crate> out.png`: the Rust rendering of a face, compared
  with node's by the same tolerance.
- By hand: open in REAPER and Ardour; automation written by a knob drag; preset load; three sizes.

## 9. Order of work

1. `build-ui.js` and the bundles of **gfx-boost** (two controls, no panel) and **gfx-grit** (five controls,
   45 expert parameters, 15 presets).
2. `gfx-ui`: face, panel, presets; the snapshot check; both plugins in REAPER and Ardour.
3. The maintainer's look at both. Then every other plugin (the Blackwork / Clay faces, gfx-geq's
   variants, the treadles), meters, a release of golem-binaries.

## 10. Not done yet

- **Sizes** (100 / 125 / 150 %) and HiDPI on Linux: `nih_plug_egui` keeps the window's size private and
  takes no scale from an X11 host; both need a patch in the fork.
- **Meters**: drawn under the pedal but the window leaves them no room, and many output parameters are
  counters or states a bar means nothing for (31 plugins have meters, gfx-tuner 23). Needs a kind
  (bar / lamp / text) in the descriptor.
- **Faces that are not cut yet**: gfx-geq (the body changes with the sliders and the layout), gfx-wah
  (the treadle is not a control), Blackwork and Clay to check one by one.
- A selector with many options (gfx-glitch: 30), `depends` on the face, a group's `custom` switch in its
  header, a failed OpenGL context (baseview aborts the host), the fork's `!is_processing` guard on
  parameter writes from the editor.
