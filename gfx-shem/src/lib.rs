// SPDX-License-Identifier: GPL-3.0-or-later
//! gfx-shem — golem's clean, shaped tight boost (design: README.md).
//!
//! The Grind's boost network, simulated from its netlist (gfx-dsp's `circuit`): an op-amp stage whose
//! frequency-dependent feedback and shunt cut the bass and lift the presence, up to +20 dB. No clipping.
//! The Grind knob is the passive level pot after the network, expressed as the gain at the plateau.

use gfx_dsp::circuit::{Circuit, Element, Netlist};
use gfx_dsp::smooth::{LinearRamp, OnePole};
use gfx_dsp::{Effect, ParamInfo};

#[cfg(feature = "plugin")]
pub mod plugin;

pub static PARAMS: [ParamInfo; 3] = [
    ParamInfo::float("grind", "Grind", -8.0, 20.0, 10.0).with_exp(),
    ParamInfo::enumeration("voice", "Voice", 2, 0),
    ParamInfo::float("bright", "Bright", -6.0, 6.0, 0.0),
];

/// The two shunt arms (ohms): Focused, Full.
pub const ARMS: [f64; 2] = [2700.0, 11800.0];
/// Where the plateau is read for the Grind calibration (Focused arm).
pub const PLATEAU_HZ: f64 = 8000.0;
pub const BRIGHT_HZ: f64 = 3000.0;
pub const DC_HZ: f64 = 5.0;
const MAX_CH: usize = 2;

/// The boost network (the design review's nodal model of the clone), output at the level pot's top.
///
/// Nodes: 1 in (op-amp +), 2 op-amp −, 3 op-amp out, 4 shunt node N, 5 T-network M, 6 T-network A,
/// 7 output B, 8 / 9 internal series nodes.
///
/// The two coupling capacitors (1 µF into N, 4.7 µF at the output) are shorts here: with the
/// feedback they form a +7 dB resonance at ≈ 2.5 Hz that would ring for seconds after every note.
/// A first-order DC blocker at `DC_HZ` stands for them; above 20 Hz the response is unchanged.
pub fn network(shunt_ohms: f64) -> Netlist {
    use Element::*;
    Netlist {
        nodes: 10,
        elements: vec![
            OpAmp(1, 2, 3),
            R(3, 2, 220e3),     // feedback 220 kΩ to the inverting input
            R(2, 4, 1e-3),      // the 1 µF coupling: a short (see `network`)
            R(4, 8, 16e3),      // 16 kΩ + 10 nF from N to the output
            C(8, 7, 10e-9),
            R(4, 5, 3900.0),    // the T-network
            R(5, 6, 160.0),
            C(5, 6, 47e-9),
            R(5, 0, 20e3),
            C(5, 0, 470e-9),
            R(6, 7, 2200.0),
            R(3, 7, 1e-3),      // the 4.7 µF output coupling: a short
            R(7, 0, 5440.0),    // the level pot (220 Ω + 5 kΩ + 220 Ω)
            R(4, 9, shunt_ohms), // the shunt arm: R + 100 nF to ground
            C(9, 0, 100e-9),
        ],
        input: 1,
        output: 7,
    }
}

/// |H| in dB of the analog network at `freq`.
pub fn network_db(shunt_ohms: f64, freq: f64) -> f64 {
    let (re, im) = network(shunt_ohms).response(freq);
    20.0 * re.hypot(im).log10()
}

/// A first-order high shelf (bilinear, prewarped): gain 1 at DC, `gain` above the corner.
#[derive(Clone, Copy, Debug, Default)]
struct Shelf {
    b0: f64,
    b1: f64,
    a1: f64,
    x1: f64,
    y1: f64,
}

impl Shelf {
    fn set(&mut self, gain_db: f64, freq: f64, sr: f64) {
        let a = 10f64.powf(gain_db / 20.0);
        // H(s) = (a s + w) / (s + w), with s → K (1 − z⁻¹) / (1 + z⁻¹), K = w / tan(w T / 2)
        let k = (std::f64::consts::PI * freq / sr).tan();
        let d = 1.0 + k;
        self.b0 = (a + k) / d;
        self.b1 = (k - a) / d;
        self.a1 = (k - 1.0) / d;
    }
    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 - self.a1 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

pub struct Shem {
    sr: f64,
    /// [channel][arm]
    nets: Vec<[Circuit; 2]>,
    shelves: [Shelf; MAX_CH],
    dc: [(f64, f64); MAX_CH],
    dc_a: f64,
    /// dB → linear gain that puts the Focused plateau at 0 dB
    norm: f64,
    grind: OnePole,
    voice: LinearRamp, // 0 = Focused, 1 = Full
    bright: OnePole,
    bright_set: f32,
    ctl: u32,
    gain: f64,
}

impl Shem {
    pub fn new() -> Self {
        Self {
            sr: 48000.0,
            nets: Vec::new(),
            shelves: [Shelf::default(); MAX_CH],
            dc: [(0.0, 0.0); MAX_CH],
            dc_a: 1.0,
            norm: 1.0,
            grind: OnePole::new(20.0, 10.0, 1e-4),
            voice: LinearRamp::new(30.0, 0.0),
            bright: OnePole::new(30.0, 0.0, 1e-4),
            bright_set: f32::NAN,
            ctl: 0,
            gain: 1.0,
        }
    }

    pub fn set_grind(&mut self, db: f32) {
        self.grind.set_target(db.clamp(-8.0, 20.0));
    }
    pub fn set_voice(&mut self, full: bool) {
        self.voice.set_target(if full { 1.0 } else { 0.0 });
    }
    pub fn set_bright(&mut self, db: f32) {
        self.bright.set_target(db.clamp(-6.0, 6.0));
    }

    fn control(&mut self) {
        let g = self.grind.value() as f64;
        self.gain = self.norm * 10f64.powf(g / 20.0);
        let b = self.bright.value();
        if b != self.bright_set {
            self.bright_set = b;
            for s in &mut self.shelves {
                s.set(b as f64, BRIGHT_HZ, self.sr);
            }
        }
    }
}

impl Default for Shem {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Shem {
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize, channels: usize) {
        self.sr = sample_rate as f64;
        let ch = channels.clamp(1, MAX_CH);
        self.nets = (0..ch)
            .map(|_| [Circuit::new(network(ARMS[0]), self.sr), Circuit::new(network(ARMS[1]), self.sr)])
            .collect();
        // the Grind knob reads the gain at the Focused plateau
        self.norm = 10f64.powf(-network_db(ARMS[0], PLATEAU_HZ) / 20.0);
        let rc = 1.0 / (2.0 * std::f64::consts::PI * DC_HZ);
        self.dc_a = rc / (rc + 1.0 / self.sr);
        self.grind.prepare(sample_rate);
        self.voice.prepare(sample_rate);
        self.bright.prepare(sample_rate);
        self.bright_set = f32::NAN;
        self.control();
    }

    fn reset(&mut self) {
        for n in &mut self.nets {
            n[0].reset();
            n[1].reset();
        }
        for s in &mut self.shelves {
            s.reset();
        }
        self.dc = [(0.0, 0.0); MAX_CH];
        self.grind.settle();
        self.voice.settle();
        self.bright.settle();
        self.bright_set = f32::NAN;
        self.ctl = 0;
        self.control();
    }

    fn set_param(&mut self, id: &str, value: f32) {
        match id {
            "grind" => self.set_grind(value),
            "voice" => self.set_voice(value >= 0.5),
            "bright" => self.set_bright(value),
            _ => {}
        }
    }

    fn process(&mut self, buffers: &mut [&mut [f32]]) {
        let n = buffers.first().map_or(0, |b| b.len());
        let nch = buffers.len().min(self.nets.len());
        for i in 0..n {
            // every sample while a control moves (no zipper), every 32 once settled
            let moving = !self.grind.is_settled() || !self.bright.is_settled();
            if moving || self.ctl == 0 {
                self.control();
            }
            self.ctl = (self.ctl + 1) % 32;
            self.grind.next();
            self.bright.next();
            let w = self.voice.next() as f64;
            for c in 0..nch {
                let x = buffers[c][i] as f64;
                let a = self.nets[c][0].process(x);
                let b = self.nets[c][1].process(x);
                let y = (1.0 - w) * a + w * b;
                let y = self.shelves[c].process(y * self.gain);
                let (px, py) = self.dc[c];
                let d = self.dc_a * (py + y - px);
                self.dc[c] = (y, d);
                buffers[c][i] = d as f32;
            }
        }
    }
}

#[cfg(test)]
mod tests;
