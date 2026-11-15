// SPDX-License-Identifier: GPL-3.0-or-later
//! The guitar in front of a pedal (gfx-fuzz §2.2; gfx-acoustic reuses it): the pickup's coil, its
//! own capacitance, the tone and volume pots and the cable, as a linear network stamped into a
//! `circuit_nl` netlist, and the filter that rebuilds the coil's EMF from a DI.
//!
//! - **What a DI is.** A DI recorded on a Hi-Z input is the guitar's output voltage with its volume
//!   full up, loaded by its own pots, its cable and the interface (1 MΩ): it already holds the coil's
//!   resonance against the cable. Writing Y_rec for the admittance the pickup node saw at recording
//!   (C_pu + C_cable, the volume and tone pots' tracks, the interface), DI = e / (1 + (R + sL) Y_rec),
//!   so **e = DI · (1 + (R + sL) Y_rec)**: a second-order polynomial, made proper by two Butterworth
//!   poles at [`EMF_POLE_HZ`], run as one biquad by the bilinear transform at the netlist's own rate
//!   (the trapezoidal rule of the netlist is the same map, so filter and network cancel exactly).
//! - **Invariant.** Volume full up into the interface's load, the network gives the DI back through
//!   the Butterworth poles only: within 0.1 dB to 16 kHz whatever the pickup's values. A wrong guess
//!   of L or of the cable cannot colour the sound by itself: it only misplaces the change a pedal's
//!   load makes.
//! - An active pickup is a resistive source (its preamp's output impedance) behind the volume pot:
//!   L = 0, no coil capacitance, no tone pot; the same filter is first order then.
//!
//! The values are typical ([U]: refined against measurements of real guitars when there are some).

use crate::circuit_nl::{Netlist, Node, Pot};
use crate::pot::Taper;
use std::f64::consts::PI;

/// The load the DI was recorded into: a Hi-Z interface input (1 MΩ).
pub const INTERFACE_LOAD: f64 = 1e6;

/// The EMF filter's Butterworth poles (Hz). gfx-fuzz §2.2 wrote 20 kHz, but two poles at 20 kHz are
/// −1.5 dB at 16 kHz, which breaks the design's own invariant (0.1 dB to 16 kHz): at 48 kHz they are
/// −0.04 dB there. The filter runs at the oversampled rate (≥ 4 × 44.1 kHz), so 48 kHz stays well
/// below its Nyquist frequency.
pub const EMF_POLE_HZ: f64 = 48_000.0;

/// A guitar's output network.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guitar {
    /// Coil inductance (H); 0 = an active (resistive) source.
    pub l: f64,
    /// Coil (or preamp output) resistance (Ω).
    pub r: f64,
    /// The coil's own capacitance (F).
    pub c: f64,
    /// Volume pot (Ω) and its law.
    pub vol: f64,
    pub vol_taper: Taper,
    /// Tone pot (Ω) to ground at the pickup node, full up (its capacitor is a short at audio); 0 = none.
    pub tone: f64,
    /// Cable capacitance (F).
    pub cable: f64,
}

/// Single coil: 2.5 H, 6 kΩ, 100 pF; 250 kΩ A volume and tone; 500 pF of cable [U, typical].
pub const SINGLE_COIL: Guitar = Guitar { l: 2.5, r: 6e3, c: 100e-12, vol: 250e3, vol_taper: Taper::Log, tone: 250e3, cable: 500e-12 };
/// Humbucker: 4.5 H, 8 kΩ, 150 pF; 500 kΩ A volume and tone [U, typical].
pub const HUMBUCKER: Guitar = Guitar { l: 4.5, r: 8e3, c: 150e-12, vol: 500e3, vol_taper: Taper::Log, tone: 500e3, cable: 500e-12 };
/// P-90: 7 H, 8 kΩ, 120 pF; 500 kΩ A volume and tone [U, typical].
pub const P90: Guitar = Guitar { l: 7.0, r: 8e3, c: 120e-12, vol: 500e3, vol_taper: Taper::Log, tone: 500e3, cable: 500e-12 };
/// Active: a preamp of ≈ 10 kΩ output impedance (EMG's published figure [M / U]) behind a 25 kΩ A
/// volume pot, no tone pot loading, 500 pF of cable.
pub const ACTIVE: Guitar = Guitar { l: 0.0, r: 10e3, c: 0.0, vol: 25e3, vol_taper: Taper::Log, tone: 0.0, cable: 500e-12 };

/// What [`Guitar::stamp`] adds: the jack (the cable's end, where the pedal's input goes) and the
/// volume pot (turned with `NonlinearCircuit::set_pot`, rotation 0 – 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stamped {
    pub jack: Node,
    pub pickup: Node,
    pub volume: usize,
}

type Cx = (f64, f64);

fn cmul(a: Cx, b: Cx) -> Cx {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

fn cdiv(a: Cx, b: Cx) -> Cx {
    let d = b.0 * b.0 + b.1 * b.1;
    ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
}

fn cadd(a: Cx, b: Cx) -> Cx {
    (a.0 + b.0, a.1 + b.1)
}

fn cinv(a: Cx) -> Cx {
    cdiv((1.0, 0.0), a)
}

/// Parallel impedances.
fn par(a: Cx, b: Cx) -> Cx {
    cdiv(cmul(a, b), cadd(a, b))
}

impl Guitar {
    /// The conductance and capacitance the pickup node saw at recording: (G, C).
    pub fn recording_load(&self) -> (f64, f64) {
        let mut g = 1.0 / self.vol + 1.0 / INTERFACE_LOAD;
        if self.tone > 0.0 {
            g += 1.0 / self.tone;
        }
        (g, self.c + self.cable)
    }

    /// EMF / DI as a polynomial in s: p₀ + p₁ s + p₂ s² = 1 + (R + sL)(G + sC).
    pub fn inverse_poly(&self) -> [f64; 3] {
        let (g, c) = self.recording_load();
        [1.0 + self.r * g, self.r * c + self.l * g, self.l * c]
    }

    /// The EMF filter at sample rate `fs` (the netlist's), poles at `pole_hz`.
    pub fn emf_filter(&self, fs: f64, pole_hz: f64) -> EmfFilter {
        let p = self.inverse_poly();
        let wc = 2.0 * PI * pole_hz;
        let d = [1.0, std::f64::consts::SQRT_2 / wc, 1.0 / (wc * wc)];
        let k = 2.0 * fs;
        // bilinear: a polynomial q₀ + q₁ s + q₂ s² times (1 + z⁻¹)²
        let bl = |q: [f64; 3]| -> [f64; 3] {
            [q[0] + q[1] * k + q[2] * k * k, 2.0 * q[0] - 2.0 * q[2] * k * k, q[0] - q[1] * k + q[2] * k * k]
        };
        let (b, a) = (bl(p), bl(d));
        EmfFilter { b: [b[0] / a[0], b[1] / a[0], b[2] / a[0]], a: [a[1] / a[0], a[2] / a[0]], s: [0.0; 2] }
    }

    /// Stamps the guitar into `net`: `emf` (a node driven by the EMF) → R (+ L) → pickup node (its
    /// capacitance, the tone pot) → the volume pot's track to ground, wiper → the cable → the jack.
    pub fn stamp(&self, net: &mut Netlist, emf: Node, rotation: f64) -> Stamped {
        let p = net.named("pickup");
        if self.l > 0.0 {
            let a = net.node();
            net.r(emf, a, self.r);
            net.l(a, p, self.l);
        } else {
            net.r(emf, p, self.r);
        }
        if self.c > 0.0 {
            net.c(p, 0, self.c);
        }
        if self.tone > 0.0 {
            net.r(p, 0, self.tone);
        }
        let jack = net.named("jack");
        let volume = net.pot(0, jack, p, self.vol, self.vol_taper, rotation);
        net.c(jack, 0, self.cable);
        Stamped { jack, pickup: p, volume }
    }

    /// EMF → jack with `load` (Ω, complex) at the jack, at `freq` Hz (Laplace): the reference the
    /// netlist is checked against.
    pub fn response(&self, freq: f64, rotation: f64, load: (f64, f64)) -> (f64, f64) {
        let w = 2.0 * PI * freq;
        let zs = (self.r, w * self.l);
        // the pot's halves as the netlist has them (ends at R_MIN): ground → wiper, wiper → pickup
        let (lo, up) = Pot { start: None, end: None, total: self.vol, taper: self.vol_taper }.halves(rotation);
        let zc = |c: f64| if c > 0.0 { cinv((0.0, w * c)) } else { (1e30, 0.0) };
        let zw = par(par((lo, 0.0), zc(self.cable)), load);
        let mut zp = cadd((up, 0.0), zw);
        if self.c > 0.0 {
            zp = par(zp, zc(self.c));
        }
        if self.tone > 0.0 {
            zp = par(zp, (self.tone, 0.0));
        }
        cmul(cdiv(zp, cadd(zs, zp)), cdiv(zw, cadd((up, 0.0), zw)))
    }

    /// The DI at recording (EMF → jack, volume full up, the interface's load).
    pub fn recording(&self, freq: f64) -> (f64, f64) {
        self.response(freq, 1.0, (INTERFACE_LOAD, 0.0))
    }
}

/// The EMF filter: one biquad (transposed direct form II, f64).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmfFilter {
    b: [f64; 3],
    a: [f64; 2],
    s: [f64; 2],
}

impl EmfFilter {
    /// The identity (a buffered path: no EMF to rebuild).
    pub const IDENTITY: EmfFilter = EmfFilter { b: [1.0, 0.0, 0.0], a: [0.0, 0.0], s: [0.0; 2] };

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.s[0];
        self.s[0] = self.b[1] * x - self.a[0] * y + self.s[1];
        self.s[1] = self.b[2] * x - self.a[1] * y;
        y
    }

    pub fn reset(&mut self) {
        self.s = [0.0; 2];
    }

    /// Its response at `freq` Hz for the sample rate `fs` (tests).
    pub fn response(&self, freq: f64, fs: f64) -> (f64, f64) {
        let w = 2.0 * PI * freq / fs;
        let z1 = (w.cos(), -w.sin());
        let z2 = cmul(z1, z1);
        let num = cadd(cadd((self.b[0], 0.0), cmul((self.b[1], 0.0), z1)), cmul((self.b[2], 0.0), z2));
        let den = cadd(cadd((1.0, 0.0), cmul((self.a[0], 0.0), z1)), cmul((self.a[1], 0.0), z2));
        cdiv(num, den)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit_nl::NonlinearCircuit;

    fn db(z: (f64, f64)) -> f64 {
        20.0 * z.0.hypot(z.1).log10()
    }

    const ALL: [Guitar; 4] = [SINGLE_COIL, HUMBUCKER, P90, ACTIVE];

    #[test]
    fn the_di_holds_the_resonance() {
        // single coil on 1 MΩ: the DI is the EMF +3.5 dB at 4 kHz, −14 dB at 10 kHz (README §2.2 [P])
        let g = SINGLE_COIL;
        let a = db(g.recording(4000.0));
        let b = db(g.recording(10000.0));
        assert!((a - 3.5).abs() < 0.6, "4 kHz {a}");
        assert!((b + 14.0).abs() < 1.0, "10 kHz {b}");
        let f0 = 1.0 / (2.0 * PI * (g.l * (g.c + g.cable)).sqrt());
        assert!((f0 - 4100.0).abs() < 100.0, "{f0}");
    }

    #[test]
    fn inverse_filter_undoes_the_recording() {
        // filter × recording = the Butterworth poles only: within 0.1 dB to 16 kHz, every pickup
        for g in ALL {
            for fs in [176400.0, 192000.0, 384000.0, 768000.0] {
                let f = g.emf_filter(fs, EMF_POLE_HZ);
                for hz in [50.0, 200.0, 1000.0, 4000.0, 8000.0, 12000.0, 16000.0] {
                    // the analog recording at the bilinear-warped frequency (the netlist's own map)
                    let wa = 2.0 * fs * (PI * hz / fs).tan() / (2.0 * PI);
                    let t = cmul(f.response(hz, fs), g.recording(wa));
                    assert!(db(t).abs() < 0.1, "{g:?} {fs} {hz}: {}", db(t));
                }
            }
        }
    }

    #[test]
    fn load_table_of_the_design() {
        // README §2.2, single coil, re the DI: face ≈ 8 kΩ and muff ≈ 130 kΩ at full volume (the
        // part of the table that does not depend on the prototype's pot law)
        let g = SINGLE_COIL;
        let rel = |hz: f64, load: f64| db(g.response(hz, 1.0, (load, 0.0))) - db(g.recording(hz));
        for (hz, want) in [(100.0, -4.7), (1000.0, -8.7), (4000.0, -22.1), (10000.0, -12.4)] {
            assert!((rel(hz, 8e3) - want).abs() < 0.3, "face {hz}: {}", rel(hz, 8e3));
        }
        for (hz, want) in [(100.0, -0.3), (1000.0, -0.5), (4000.0, -4.3), (10000.0, -0.6)] {
            assert!((rel(hz, 130e3) - want).abs() < 0.3, "muff {hz}: {}", rel(hz, 130e3));
        }
    }

    #[test]
    fn netlist_matches_the_laplace_response() {
        // the stamped network (no devices) against `response`, the volume at 100 / 70 / 50 %, into 8 kΩ
        for g in ALL {
            for rot in [1.0, 0.7, 0.5] {
                let mut n = Netlist::new(1);
                let e = n.input(0);
                let s = g.stamp(&mut n, e, rot);
                n.r(s.jack, 0, 8e3);
                let o = n.output(s.jack);
                let c = NonlinearCircuit::new(&n, 384000.0);
                for hz in [100.0, 1000.0, 4000.0, 10000.0] {
                    let a = db(c.ac_response(hz, 0, o));
                    let b = db(g.response(hz, rot, (8e3, 0.0)));
                    assert!((a - b).abs() < 0.01, "{g:?} {rot} {hz}: {a} vs {b}");
                }
            }
        }
    }

    #[test]
    fn source_invariant_in_the_time_domain() {
        // DI → EMF filter → the stamped guitar into 1 MΩ, volume full up: the jack is the DI within
        // 0.1 dB, 20 Hz – 16 kHz (sines through the real filter and the trapezoidal netlist)
        let fs = 384000.0;
        for g in ALL {
            let mut n = Netlist::new(1);
            let e = n.input(0);
            let s = g.stamp(&mut n, e, 1.0);
            n.r(s.jack, 0, INTERFACE_LOAD);
            let o = n.output(s.jack);
            for hz in [20.0, 1000.0, 4100.0, 16000.0] {
                let mut c = NonlinearCircuit::new(&n, fs);
                c.init_dc();
                let mut f = g.emf_filter(fs, EMF_POLE_HZ);
                let len = (fs * (0.1 + 4.0 / hz)) as usize;
                let (mut ei, mut eo) = (0.0, 0.0);
                for k in 0..len {
                    let x = (2.0 * PI * hz * k as f64 / fs).sin();
                    c.set_input(0, f.process(x));
                    c.process();
                    if k > len / 2 {
                        ei += x * x;
                        eo += c.output(o).powi(2);
                    }
                }
                let d = 10.0 * (eo / ei).log10();
                assert!(d.abs() < 0.1, "{g:?} {hz} Hz: {d} dB");
            }
        }
    }
}
