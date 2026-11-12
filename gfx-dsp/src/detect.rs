// SPDX-License-Identifier: GPL-3.0-or-later
//! Monophonic pitch detection (YIN) and diatonic harmony.
//!
//! - [`PitchDetector`]: YIN (de Cheveigné & Kawahara, *YIN, a fundamental frequency estimator for speech
//!   and music*, JASA 2002) on the input low-passed and decimated to ~12 kHz, analysed every 32 decimated
//!   samples (~2.7 ms). Range 75 Hz – 1.5 kHz (drop-D to the 22nd fret of the high E with room). Cost is
//!   bounded: one analysis is a fixed `W × τmax` difference function. Allocates in `prepare` only.
//! - [`NoteTracker`]: the played pitch class, with confirmation and hysteresis (a bend between two notes
//!   keeps the old one until it is clearly the new one; unvoiced frames hold the last note).
//! - [`harmony_interval`]: the interval that keeps a voice in a key / scale.

use crate::svf::{Svf, SvfMode};

/// Lowest and highest detected fundamentals (Hz).
pub const F_MIN: f32 = 75.0;
pub const F_MAX: f32 = 1500.0;
/// The decimated analysis rate's target (Hz).
const RATE: f32 = 12000.0;
/// Decimated samples between analyses.
const HOP: usize = 32;
/// CMNDF threshold for the first dip (YIN's absolute threshold) and the voicing limit.
const THRESH: f32 = 0.15;
const UNVOICED: f32 = 0.35;

#[derive(Clone, Default)]
pub struct PitchDetector {
    aa: [Svf; 2],
    dec: usize,
    phase: usize,
    rate: f32,
    w: usize,
    tau_min: usize,
    tau_max: usize,
    /// circular buffer of the last `w + tau_max` decimated samples
    ring: Vec<f32>,
    at: usize,
    frame: Vec<f32>,
    d: Vec<f32>,
    /// the raw difference function (the refinement interpolates it: less biased than the normalised one)
    raw: Vec<f32>,
    since: usize,
    hz: f32,
    clarity: f32,
    voiced: bool,
    fresh: bool,
}

impl PitchDetector {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.dec = (sample_rate / RATE).round().max(1.0) as usize;
        self.rate = sample_rate / self.dec as f32;
        let fc = (0.2 * self.rate).min(2500.0);
        for s in self.aa.iter_mut() {
            s.set(SvfMode::LowPass, fc, std::f32::consts::FRAC_1_SQRT_2, 0.0, sample_rate);
        }
        self.tau_min = (self.rate / F_MAX).floor().max(2.0) as usize;
        self.tau_max = (self.rate / F_MIN).ceil() as usize + 1;
        self.w = self.tau_max;
        let len = self.w + self.tau_max;
        self.ring = vec![0.0; len];
        self.frame = vec![0.0; len];
        self.d = vec![0.0; self.tau_max + 2];
        self.raw = vec![0.0; self.tau_max + 2];
        self.reset();
    }

    pub fn reset(&mut self) {
        for s in self.aa.iter_mut() {
            s.reset();
        }
        self.ring.iter_mut().for_each(|x| *x = 0.0);
        self.at = 0;
        self.phase = 0;
        self.since = 0;
        self.hz = 0.0;
        self.clarity = 0.0;
        self.voiced = false;
        self.fresh = false;
    }

    /// Feeds one input sample. Returns true when a new analysis is ready.
    #[inline]
    pub fn push(&mut self, x: f32) -> bool {
        let a = self.aa[0].process(x);
        let y = self.aa[1].process(a);
        self.phase += 1;
        if self.phase < self.dec {
            return false;
        }
        self.phase = 0;
        self.ring[self.at] = y;
        self.at += 1;
        if self.at == self.ring.len() {
            self.at = 0;
        }
        self.since += 1;
        if self.since < HOP {
            return false;
        }
        self.since = 0;
        self.analyse();
        self.fresh = true;
        true
    }

    fn analyse(&mut self) {
        let len = self.ring.len();
        // oldest first
        let (a, b) = self.ring.split_at(self.at);
        self.frame[..b.len()].copy_from_slice(b);
        self.frame[b.len()..len].copy_from_slice(a);
        let (w, tmax) = (self.w, self.tau_max);
        // the most recent `w` samples against `tau` earlier ones
        let cur = &self.frame[len - w..];
        let energy: f32 = cur.iter().map(|v| v * v).sum::<f32>() / w as f32;
        if energy < 1e-7 {
            // below about −70 dBFS: silence
            self.voiced = false;
            self.clarity = 0.0;
            return;
        }
        self.d[0] = 0.0;
        for tau in 1..=tmax {
            let past = &self.frame[len - w - tau..len - tau];
            // eight independent sums: the compiler may not reorder one float sum, it vectorises these
            let mut acc = [0.0f32; 8];
            let ((c8, cr), (p8, pr)) = (cur.as_chunks::<8>(), past.as_chunks::<8>());
            for (c, q) in c8.iter().zip(p8) {
                for k in 0..8 {
                    let e = c[k] - q[k];
                    acc[k] += e * e;
                }
            }
            let mut s: f32 = acc.iter().sum();
            for (c, q) in cr.iter().zip(pr) {
                s += (c - q) * (c - q);
            }
            self.d[tau] = s;
        }
        self.raw.copy_from_slice(&self.d);
        // cumulative mean normalised difference
        let mut run = 0.0f32;
        self.d[0] = 1.0;
        for tau in 1..=tmax {
            run += self.d[tau];
            self.d[tau] = if run > 0.0 { self.d[tau] * tau as f32 / run } else { 1.0 };
        }
        // first dip under the threshold (then down to its bottom), else the global minimum
        let mut best = 0;
        let mut tau = self.tau_min;
        while tau < tmax {
            if self.d[tau] < THRESH {
                while tau + 1 < tmax && self.d[tau + 1] < self.d[tau] {
                    tau += 1;
                }
                best = tau;
                break;
            }
            tau += 1;
        }
        if best == 0 {
            best = (self.tau_min..tmax).min_by(|&p, &q| self.d[p].total_cmp(&self.d[q])).unwrap_or(self.tau_min);
        }
        // harmonic lock (a strong 3rd or 2nd harmonic, bridge pickups): when a multiple of the lag has a
        // clearly deeper dip, the true period is that multiple
        for k in [3usize, 2] {
            let c = best * k;
            if c + 2 >= tmax {
                continue;
            }
            let m = (c - 2..=c + 2).min_by(|&p, &q| self.d[p].total_cmp(&self.d[q])).unwrap_or(c);
            if self.d[m] < 0.5 * self.d[best] && self.d[best] > 0.04 {
                best = m;
                break;
            }
        }
        let dm = self.d[best];
        self.clarity = (1.0 - dm).max(0.0);
        if dm > UNVOICED || best <= self.tau_min || best >= tmax - 1 {
            self.voiced = false;
            return;
        }
        // parabolic refinement
        let (y0, y1, y2) = (self.raw[best - 1], self.raw[best], self.raw[best + 1]);
        let den = y0 - 2.0 * y1 + y2;
        let off = if den.abs() > 1e-12 { (0.5 * (y0 - y2) / den).clamp(-0.5, 0.5) } else { 0.0 };
        self.hz = self.rate / (best as f32 + off);
        self.voiced = true;
    }

    /// The last analysis' fundamental (Hz), when voiced.
    pub fn hz(&self) -> Option<f32> {
        if self.voiced {
            Some(self.hz)
        } else {
            None
        }
    }

    /// 1 − the CMNDF minimum: 1 = perfectly periodic.
    pub fn clarity(&self) -> f32 {
        self.clarity
    }

    /// Takes the "new analysis" flag.
    pub fn take_fresh(&mut self) -> bool {
        std::mem::take(&mut self.fresh)
    }
}

/// Fractional MIDI note of a frequency.
#[inline]
pub fn midi_of(hz: f32) -> f32 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

/// The played pitch class (0 = C … 11 = B), from successive analyses.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoteTracker {
    pc: Option<u8>,
    cand: Option<u8>,
    count: u8,
}

impl NoteTracker {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// One analysis. A new class is taken when it is near a note (within ±0.4 st: a bend between notes
    /// keeps the old one) and seen in two analyses running; unvoiced frames hold.
    pub fn update(&mut self, hz: Option<f32>) {
        let Some(hz) = hz else {
            self.count = 0;
            return;
        };
        let m = midi_of(hz);
        let r = m.round();
        if (m - r).abs() > 0.4 {
            self.count = 0;
            return;
        }
        let pc = (r as i32).rem_euclid(12) as u8;
        if Some(pc) == self.pc {
            self.count = 0;
            return;
        }
        if Some(pc) == self.cand {
            self.count += 1;
        } else {
            self.cand = Some(pc);
            self.count = 1;
        }
        if self.count >= 2 || self.pc.is_none() {
            self.pc = Some(pc);
            self.count = 0;
        }
    }

    pub fn pitch_class(&self) -> Option<u8> {
        self.pc
    }
}

/// The scales (semitones above the key), in the `scale` parameter's order: the list only grows at the end.
pub const SCALES: [&[i32]; 10] = [
    &[0, 2, 4, 5, 7, 9, 11], // Major
    &[0, 2, 3, 5, 7, 8, 10], // Minor (natural)
    &[0, 2, 3, 5, 7, 8, 11], // Harmonic Minor
    &[0, 2, 3, 5, 7, 9, 11], // Melodic Minor
    &[0, 2, 3, 5, 7, 9, 10], // Dorian
    &[0, 1, 3, 5, 7, 8, 10], // Phrygian
    &[0, 2, 4, 6, 7, 9, 11], // Lydian
    &[0, 2, 4, 5, 7, 9, 10], // Mixolydian
    &[0, 2, 4, 7, 9],        // Major Pentatonic
    &[0, 3, 5, 7, 10],       // Minor Pentatonic
];

/// The interval (semitones) that keeps a voice in `key` (0 = C) / `scale` above a played pitch class.
///
/// `shift` (semitones, the nominal interval) becomes a number of scale steps: round(shift × notes / 12):
/// in a 7-note scale +3 / +4 = a third, +5 = a fourth, +7 = a fifth, +12 = an octave (negative: below).
/// The played note is placed on the scale degree at or below it; the voice goes to the degree `steps`
/// away. A chromatic (out-of-scale) note keeps its lower degree's interval: passing tones move in parallel.
pub fn harmony_interval(pc: u8, key: u8, scale: usize, shift: f32) -> i32 {
    let sc = SCALES[scale.min(SCALES.len() - 1)];
    let n = sc.len() as i32;
    let steps = (shift * n as f32 / 12.0).round() as i32;
    let rel = (pc as i32 - key as i32).rem_euclid(12);
    let deg = sc.iter().rposition(|&s| s <= rel).unwrap_or(0) as i32;
    let t = deg + steps;
    let target = sc[t.rem_euclid(n) as usize] + 12 * t.div_euclid(n);
    target - sc[deg as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saw(f: f32, sr: f32, n: usize) -> Vec<f32> {
        // a band-limited-ish guitar-like tone: harmonics with 1/k fall-off up to 5 kHz
        (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                let mut s = 0.0;
                let mut k = 1;
                while k as f32 * f < 5000.0 && k < 30 {
                    s += (std::f32::consts::TAU * f * k as f32 * t).sin() / k as f32;
                    k += 1;
                }
                0.2 * s
            })
            .collect()
    }

    #[test]
    fn detects_guitar_range_within_a_tenth_of_a_semitone() {
        for sr in [44100.0, 48000.0, 96000.0] {
            let mut d = PitchDetector::default();
            d.prepare(sr);
            for f in [77.8f32, 82.41, 110.0, 146.8, 196.0, 246.9, 329.6, 440.0, 659.3, 987.8, 1318.5] {
                d.reset();
                let x = saw(f, sr, (0.1 * sr) as usize);
                let mut last = None;
                for v in x {
                    if d.push(v) {
                        last = d.hz();
                    }
                }
                let hz = last.unwrap_or_else(|| panic!("unvoiced at {f} Hz, sr {sr}"));
                let err = midi_of(hz) - midi_of(f);
                assert!(err.abs() < 0.1, "{f} Hz at {sr}: detected {hz} ({err:+.3} st)");
            }
        }
    }

    #[test]
    fn noise_and_silence_are_unvoiced() {
        let mut d = PitchDetector::default();
        d.prepare(48000.0);
        let mut rng = crate::lfo::Rng::new(3);
        let mut voiced = 0;
        let mut total = 0;
        for _ in 0..48000 {
            if d.push(0.3 * rng.bipolar()) {
                total += 1;
                voiced += d.hz().is_some() as usize;
            }
        }
        assert!(voiced * 20 < total, "noise voiced in {voiced} / {total} analyses");
        d.reset();
        for _ in 0..4800 {
            if d.push(0.0) {
                assert!(d.hz().is_none());
            }
        }
    }

    #[test]
    fn tracker_confirms_and_holds_through_bends() {
        let mut t = NoteTracker::default();
        let hz = |m: f32| 440.0 * ((m - 69.0) / 12.0).exp2();
        t.update(Some(hz(62.0)));
        assert_eq!(t.pitch_class(), Some(2));
        // a bend from D toward E: between notes it holds D
        for m in [62.2, 62.5, 62.7] {
            t.update(Some(hz(m)));
            assert_eq!(t.pitch_class(), Some(2));
        }
        // E needs two analyses
        t.update(Some(hz(64.0)));
        assert_eq!(t.pitch_class(), Some(2));
        t.update(Some(hz(64.05)));
        assert_eq!(t.pitch_class(), Some(4));
        // unvoiced holds; an octave of the same class changes nothing
        t.update(None);
        t.update(Some(hz(76.0)));
        assert_eq!(t.pitch_class(), Some(4));
    }

    #[test]
    fn diatonic_thirds_fifths_and_chromatic_notes() {
        // C major, a third above (+4): C→E 4, D→F 3, E→G 3, F→A 4, G→B 4, A→C 3, B→D 3
        let thirds: Vec<i32> = [0u8, 2, 4, 5, 7, 9, 11].iter().map(|&pc| harmony_interval(pc, 0, 0, 4.0)).collect();
        assert_eq!(thirds, [4, 3, 3, 4, 4, 3, 3]);
        // +3 is a third too
        assert_eq!(harmony_interval(2, 0, 0, 3.0), 3);
        // fifths: B→F (the diminished fifth stays in key), the rest perfect
        let fifths: Vec<i32> = [0u8, 2, 4, 5, 7, 9, 11].iter().map(|&pc| harmony_interval(pc, 0, 0, 7.0)).collect();
        assert_eq!(fifths, [7, 7, 7, 7, 7, 7, 6]);
        // octave and unison, a third below
        assert_eq!(harmony_interval(4, 0, 0, 12.0), 12);
        assert_eq!(harmony_interval(4, 0, 0, 0.0), 0);
        assert_eq!(harmony_interval(4, 0, 0, -4.0), -4); // E → C below
        assert_eq!(harmony_interval(0, 0, 0, -3.0), -3); // C → A below
        // a chromatic C# in C major moves like C (parallel)
        assert_eq!(harmony_interval(1, 0, 0, 4.0), 4);
        // key of A minor (key 9, natural minor): A→C 3, B→D 3, C→E 4
        assert_eq!(harmony_interval(9, 9, 1, 4.0), 3);
        assert_eq!(harmony_interval(11, 9, 1, 4.0), 3);
        assert_eq!(harmony_interval(0, 9, 1, 4.0), 4);
        // E minor pentatonic (E G A B D): a "third" (2 steps of 5) above E is A
        assert_eq!(harmony_interval(4, 4, 9, 4.0), 5);
        // every result lands in the scale for diatonic notes
        for scale in 0..SCALES.len() {
            for key in 0..12u8 {
                for &deg in SCALES[scale] {
                    let pc = ((key as i32 + deg) % 12) as u8;
                    for shift in [-12.0, -7.0, -4.0, 3.0, 4.0, 7.0, 9.0, 12.0, 24.0] {
                        let i = harmony_interval(pc, key, scale, shift);
                        let rel = (pc as i32 + i - key as i32).rem_euclid(12);
                        assert!(SCALES[scale].contains(&rel), "scale {scale} key {key} pc {pc} shift {shift}");
                    }
                }
            }
        }
    }
}

/// Offline check on a real recording (ignored): GFX_DETECT_IN = raw f32 mono at GFX_DETECT_SR,
/// writes "time_s,hz_or_0,pitch_class_or_-1" per analysis to GFX_DETECT_OUT.
#[cfg(test)]
#[test]
#[ignore]
fn dump_tracking() {
    use std::io::Write;
    let path = std::env::var("GFX_DETECT_IN").expect("GFX_DETECT_IN");
    let sr: f32 = std::env::var("GFX_DETECT_SR").map_or(48000.0, |v| v.parse().unwrap());
    let bytes = std::fs::read(path).unwrap();
    let x: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    let mut d = PitchDetector::default();
    d.prepare(sr);
    let mut t = NoteTracker::default();
    let mut out = std::fs::File::create(std::env::var("GFX_DETECT_OUT").unwrap()).unwrap();
    let start = std::time::Instant::now();
    for (i, &v) in x.iter().enumerate() {
        if d.push(v) {
            t.update(d.hz());
            writeln!(out, "{},{},{}", i as f32 / sr, d.hz().unwrap_or(0.0), t.pitch_class().map_or(-1, |p| p as i32)).unwrap();
        }
    }
    eprintln!("{:.1} ns / sample", start.elapsed().as_secs_f64() * 1e9 / x.len() as f64);
}
