// SPDX-License-Identifier: GPL-3.0-or-later
//! A plugin's bundle (`bundles/<crate>.gfxui`, written by `visuals/tools/build-ui.js`): a ten-byte header
//! read when the plugin loads, then the deflated JSON document read when the editor first opens.

use serde::Deserialize;
use std::collections::BTreeMap;

pub const VERSION: u8 = 1;

/// The part of a bundle that is read without inflating it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Header {
    /// Whether the plugin has parameters that are not on its face.
    pub panel: bool,
    /// The meter rows under the pedal.
    pub rows: u8,
    /// The face's size in viewBox units.
    pub width: u16,
    pub height: u16,
}

impl Header {
    pub fn read(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 10 || &bytes[..4] != b"GFXU" || bytes[4] != VERSION {
            return None;
        }
        Some(Self {
            panel: bytes[5] & 1 != 0,
            rows: (bytes[5] >> 1) & 0x0f,
            width: u16::from_le_bytes([bytes[6], bytes[7]]),
            height: u16::from_le_bytes([bytes[8], bytes[9]]),
        })
    }
}

#[derive(Deserialize, Debug)]
pub struct Bundle {
    pub version: u8,
    #[serde(rename = "crate")]
    pub krate: String,
    pub name: String,
    pub face: Face,
    pub front: Vec<String>,
    pub groups: Vec<Group>,
    /// In the plugin's own order.
    pub params: IndexedParams,
    #[serde(default)]
    pub meters: Vec<Meter>,
    #[serde(default)]
    pub presets: Vec<Preset>,
}

#[derive(Deserialize, Debug)]
pub struct Face {
    /// x, y, width, height.
    pub view_box: [f32; 4],
    pub accent: String,
    pub body: String,
    pub controls: Vec<Control>,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ControlKind {
    Knob,
    Selector,
    Toggle,
    Fader,
}

#[derive(Deserialize, Debug)]
pub struct Control {
    pub param: String,
    pub kind: ControlKind,
    /// x, y, width, height in viewBox units: what the frames cover, and the hit area.
    pub bbox: [f32; 4],
    /// The control alone, drawn at each position (an SVG document each).
    pub frames: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub custom: Option<String>,
    pub params: Vec<String>,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ParamKind {
    Float,
    Enum,
    Bool,
}

#[derive(Deserialize, Debug)]
pub struct ParamInfo {
    pub name: String,
    pub kind: ParamKind,
    pub min: f32,
    pub max: f32,
    /// A number, or a boolean for a switch.
    pub default: serde_json::Value,
    /// The taper of the control: "lin" or "log".
    #[serde(default)]
    pub display: Option<String>,
    #[serde(default)]
    pub options: Option<Vec<String>>,
    #[serde(default)]
    pub depends: Option<Depends>,
    #[serde(default)]
    pub help: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct Depends {
    pub param: String,
    pub equals: serde_json::Value,
}

/// A row of meters under the pedal (gfx-ui/meters.json, merged by build-ui.js).
#[derive(Deserialize, Debug)]
pub struct Meter {
    pub kind: MeterKind,
    pub label: String,
    /// One parameter, or several for `bands`.
    pub ids: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub min: f32,
    #[serde(default = "one")]
    pub max: f32,
    /// A signed value: the bar fills from the middle.
    #[serde(default)]
    pub center: bool,
    #[serde(default)]
    pub states: Vec<String>,
    #[serde(default)]
    pub format: Option<String>,
}

fn one() -> f32 {
    1.0
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MeterKind {
    Gr,
    Bar,
    Lamp,
    State,
    Value,
    Bands,
}

impl Meter {
    /// Its height in meter rows.
    pub fn rows(&self) -> u8 {
        if self.kind == MeterKind::Bands { 2 } else { 1 }
    }
}

#[derive(Deserialize, Debug)]
pub struct Preset {
    pub id: String,
    #[serde(rename = "type", default)]
    pub block: Option<String>,
    pub name: String,
    #[serde(default)]
    pub desc: Option<String>,
    pub params: BTreeMap<String, serde_json::Value>,
}

/// The parameters by id, keeping the document's order (serde_json's map would sort them).
#[derive(Debug, Default)]
pub struct IndexedParams {
    pub order: Vec<String>,
    pub by_id: BTreeMap<String, ParamInfo>,
}

impl ParamInfo {
    /// Whether the control moves by ratios: a millisecond counts at 5 ms, not at 2 s.
    fn log(&self) -> bool {
        self.kind == ParamKind::Float && self.display.as_deref() == Some("log") && self.max > self.min
    }

    /// Where a normalised value stands on its control's travel, 0 to 1. The plugins' ranges are linear
    /// (docs/golem-contract.md) and the pedal is drawn so; the hand moves along the taper.
    pub fn position(&self, normalized: f32) -> f32 {
        let n = normalized.clamp(0.0, 1.0);
        match (self.log(), self.min > 0.0) {
            (false, _) => n,
            (true, true) => ((self.min + n * (self.max - self.min)) / self.min).ln() / (self.max / self.min).ln(),
            // A range from zero has no ratio to follow: a cubic law, as fine near zero.
            (true, false) => n.cbrt(),
        }
    }

    /// The normalised value at a place of the control's travel (the inverse of `position`).
    pub fn normalized(&self, position: f32) -> f32 {
        let t = position.clamp(0.0, 1.0);
        match (self.log(), self.min > 0.0) {
            (false, _) => t,
            (true, true) => (self.min * (self.max / self.min).powf(t) - self.min) / (self.max - self.min),
            (true, false) => t * t * t,
        }
    }

    /// A value of the descriptor or of a preset in the parameter's own unit: a number, a boolean, or an
    /// option's name. `None` for anything else, an unknown option included.
    pub fn plain(&self, value: &serde_json::Value) -> Option<f32> {
        match (self.kind, value) {
            (_, serde_json::Value::Bool(b)) => Some(f32::from(*b)),
            (ParamKind::Enum, serde_json::Value::String(s)) => {
                Some(self.options.as_ref()?.iter().position(|o| o.eq_ignore_ascii_case(s))? as f32)
            }
            (_, serde_json::Value::Number(n)) => Some(n.as_f64()? as f32),
            (_, serde_json::Value::String(s)) => s.parse().ok(),
            _ => None,
        }
    }
}

impl Control {
    /// The frame that draws a position `t` (0 to 1 over the parameter's range) or, for a stepped
    /// parameter, the option `plain`. `build-ui.js` draws the frames by the same rule.
    pub fn frame(&self, kind: ParamKind, t: f32, plain: f32) -> usize {
        let last = self.frames.len().saturating_sub(1);
        let i = match kind {
            ParamKind::Float => (t.clamp(0.0, 1.0) * last as f32).round() as usize,
            ParamKind::Enum | ParamKind::Bool => plain.round().max(0.0) as usize,
        };
        i.min(last)
    }
}

impl IndexedParams {
    pub fn get(&self, id: &str) -> Option<&ParamInfo> {
        self.by_id.get(id)
    }
}

impl<'de> Deserialize<'de> for IndexedParams {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = IndexedParams;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a map of parameters")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<IndexedParams, A::Error> {
                let mut out = IndexedParams::default();
                while let Some((id, p)) = map.next_entry::<String, ParamInfo>()? {
                    out.order.push(id.clone());
                    out.by_id.insert(id, p);
                }
                Ok(out)
            }
        }
        d.deserialize_map(V)
    }
}

impl Bundle {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        Header::read(bytes).ok_or("not a gfx-ui bundle of this version")?;
        let json = miniz_oxide::inflate::decompress_to_vec_zlib(&bytes[10..]).map_err(|e| format!("inflate: {e:?}"))?;
        serde_json::from_slice(&json).map_err(|e| format!("json: {e}"))
    }

    /// Whether the face carries this parameter (the panel shows the others).
    pub fn on_face(&self, id: &str) -> bool {
        self.face.controls.iter().any(|c| c.param == id)
    }
}
