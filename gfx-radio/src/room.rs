// SPDX-License-Identifier: GPL-3.0-or-later
//! Where the listener stands (README §2.6): Small Room, Hall (with distant horns), Street (facade
//! reflections and a short tail), on gfx-dsp's reverb `Fdn` and `Diffuser`. Seeded: deterministic.

use crate::voicing::Room;
use gfx_dsp::delay::DelayLine;
use gfx_dsp::reverb::{Diffuser, Fdn};

/// Discrete taps: (ms, dB, low-pass Hz).
const HALL_TAPS: [(f64, f64, f64); 3] = [(60.0, -8.0, 2500.0), (115.0, -12.0, 2200.0), (180.0, -16.0, 2000.0)];
const STREET_TAPS: [(f64, f64, f64); 4] = [(45.0, -6.0, 5000.0), (95.0, -9.0, 4000.0), (150.0, -12.0, 3500.0), (210.0, -15.0, 3000.0)];

/// The diffuse part's gain per room, so a unit-energy impulse leaves about unit energy (measured:
/// tests::room_energy).
const NORM: [f64; 4] = [0.0, 1.566, 1.27, 1.204];

/// A room's shape: Fdn scale, diffuser scale, RT60 (low, mid, high), discrete taps.
type Shape = (f64, f64, [f64; 3], &'static [(f64, f64, f64)]);

/// One room (the old one rings out on a switch while the new one fills).
#[derive(Clone)]
pub struct RoomSlot {
    pub kind: Room,
    diff: Diffuser,
    fdn: Fdn,
    taps: DelayLine,
    tap_n: usize,
    tap_d: [usize; 4],
    tap_g: [f32; 4],
    tap_k: [f32; 4],
    tap_s: [f32; 4],
    fdn_g: f32,
    /// the input's gain (ramps on a switch)
    pub gin: f64,
    /// samples since this slot stopped being fed, and the running window peak
    pub age: u64,
    win_peak: f32,
    win_n: u32,
    pub quiet: bool,
    sr: f64,
}

impl RoomSlot {
    pub fn new() -> Self {
        Self {
            kind: Room::None,
            diff: Diffuser::default(),
            fdn: Fdn::default(),
            taps: DelayLine::new(),
            tap_n: 0,
            tap_d: [1; 4],
            tap_g: [0.0; 4],
            tap_k: [0.0; 4],
            tap_s: [0.0; 4],
            fdn_g: 0.0,
            gin: 0.0,
            age: 0,
            win_peak: 0.0,
            win_n: 0,
            quiet: true,
            sr: 48000.0,
        }
    }

    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr;
        self.diff.prepare(sr as f32, 3, 40.0, 0xD1FF);
        self.fdn.prepare(sr as f32, 1.6);
        self.taps.prepare((0.22 * sr) as usize + 8);
    }

    pub fn configure(&mut self, kind: Room) {
        self.kind = kind;
        let sr = self.sr;
        let (scale, dscale, rt, taps): Shape = match kind {
            Room::None => (0.3, 0.3, [0.1, 0.1, 0.1], &[]),
            Room::Small => (0.35, 0.3, [0.45, 0.4, 0.25], &[]),
            Room::Hall => (1.5, 1.0, [2.4, 2.2, 1.2], &HALL_TAPS),
            Room::Street => (0.8, 0.6, [0.9, 0.8, 0.5], &STREET_TAPS),
        };
        self.diff.set_scale(dscale);
        self.diff.settle();
        self.fdn.set_scale(scale);
        self.fdn.settle();
        self.fdn.set_decay(rt[0], rt[1], rt[2], 250.0, 3000.0);
        self.fdn.set_mod(0.0, 0.5);
        self.fdn.settle_mod();
        self.fdn_g = NORM[kind as usize] as f32;
        self.tap_n = taps.len();
        for (i, t) in taps.iter().enumerate() {
            self.tap_d[i] = (t.0 * 0.001 * sr) as usize;
            self.tap_g[i] = 10f64.powf(t.1 / 20.0) as f32;
            self.tap_k[i] = (1.0 - (-std::f64::consts::TAU * t.2 / sr).exp()) as f32;
        }
    }

    pub fn reset(&mut self) {
        self.diff.reset();
        self.fdn.reset();
        self.taps.reset();
        self.tap_s = [0.0; 4];
        self.age = 0;
        self.win_peak = 0.0;
        self.win_n = 0;
        self.quiet = true;
    }

    /// One frame of the sends (left, right); returns (left, right).
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let g = self.gin as f32;
        let (l, r) = (l * g, r * g);
        let mut d = [l, r, l, r, l, r, l, r];
        self.diff.process(&mut d);
        let mut inp = [0.0f32; 16];
        for i in 0..8 {
            inp[i] = 0.5 * d[i];
            inp[i + 8] = if i % 2 == 0 { -0.5 * d[i] } else { 0.5 * d[i] };
        }
        let (fl, fr) = self.fdn.process(&inp);
        let (mut ol, mut or) = (fl * self.fdn_g, fr * self.fdn_g);
        if self.tap_n > 0 {
            self.taps.push(0.5 * (l + r));
            for i in 0..self.tap_n {
                let t = self.taps.tap(self.tap_d[i]);
                self.tap_s[i] += self.tap_k[i] * (t - self.tap_s[i]);
                let v = self.tap_s[i] * self.tap_g[i];
                // alternate sides: the reflections come from both facades
                if i % 2 == 0 {
                    ol += v;
                    or += 0.7 * v;
                } else {
                    ol += 0.7 * v;
                    or += v;
                }
            }
        }
        // a quiet detector on a fixed 2,048-sample grid (block-size independent)
        self.win_peak = self.win_peak.max(ol.abs()).max(or.abs());
        self.win_n += 1;
        if self.win_n == 2048 {
            self.quiet = self.win_peak < 1e-4;
            self.win_peak = 0.0;
            self.win_n = 0;
        }
        (ol, or)
    }
}

impl Default for RoomSlot {
    fn default() -> Self {
        Self::new()
    }
}
