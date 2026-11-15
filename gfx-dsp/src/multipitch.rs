// SPDX-License-Identifier: GPL-3.0-or-later
//! Chord analysis on a side chain: the notes of a strum (gfx-sustainer README §2.2 / §2.7; API agreed
//! with gfx-polysynth §2.3). Users: gfx-sustainer (`max_notes` 4), gfx-polysynth (6).
//!
//! - The input is low-passed (4th-order Butterworth at 0.35 × the analysis rate, two TPT sections,
//!   the decimator design of `detect`) and decimated to ~6 kHz.
//! - Every 256 decimated samples (43 ms) one analysis runs on the **longest Hann window that lies after
//!   the onset**: 512, 1,024 or 2,048 points (85, 171, 341 ms). The windows follow gfx-tuner's rule
//!   T = 4 / Δf: 85 ms separates notes 47 Hz apart, 341 ms 12 Hz.
//! - **Klapuri-style harmonic summation with iterative cancellation** (Klapuri, *Multiple fundamental
//!   frequency estimation by summing harmonic amplitudes*, ISMIR 2006): the salience of a candidate cell
//!   [f_lo, f_hi] (1/8 semitone) is Σ_m g(f0, m) · max |R(k)| over the bins of [m f_lo, m f_hi] (widened by
//!   a bin, and upwards by the inharmonicity of a wound string, B = 3·10⁻⁴), g = (f0 + 27) / (m f0 + 320),
//!   m f0 under 0.35 × the rate, m ≤ 10. The best candidate's f0 is refined from its first three
//!   partials (parabolic interpolation on the log magnitude), then its partials are cancelled from the
//!   residual R by their **spectrally smoothed** amplitude (the minimum of the partial and the mean of
//!   its neighbours: a note an octave above keeps the excess of the shared partials). Up to `max_notes`
//!   iterations; a note under 0.2 × the first one's salience ends the analysis.
//! - A reading is **confirmed** when two successive analyses agree (same count, each note within
//!   ±50 ¢). [`Analyzer::notes`] returns confirmed readings only, sorted by pitch; [`Analyzer::early`]
//!   (research, off unless [`Analyzer::set_early`]) the last unconfirmed one, never meant to start a
//!   sound.
//!
//! Additions to the agreed API (gfx-sustainer §2.7): [`Analyzer::onset`] (the windows restart at a pick)
//! and [`Analyzer::set_active`] (the analyses run only while the caller's YIN is unclear; the decimated
//! history always fills). Allocation in `prepare` only; deterministic.

use crate::svf::{Svf, SvfMode};
use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// The most notes any caller may ask for.
pub const MAX_NOTES: usize = 6;
/// The analysis rate's target (Hz).
const RATE: f32 = 6000.0;
/// The windows (decimated samples) and the hop.
pub const WINDOWS: [usize; 3] = [512, 1024, 2048];
const HOP: usize = 256;
/// The highest fundamental (Hz).
pub const F_MAX: f32 = 1200.0;
/// Partials considered: m ≤ M_MAX, m f0 ≤ PART_TOP × the rate.
const M_MAX: usize = 10;
const PART_TOP: f32 = 0.35;
/// Candidate cells per semitone.
const CELLS_PER_ST: f32 = 8.0;
/// Klapuri's weight g = (f0 + α) / (m f0 + β).
const ALPHA: f32 = 27.0;
const BETA: f32 = 320.0;
/// The inharmonicity the bands allow upwards (a wound string).
const B_INH: f32 = 3e-4;
/// A note under this share of the first one's salience ends the analysis.
const STOP: f32 = 0.15;
/// A note's fundamental is at least this share of its strongest of partials 1 – 4 (−20 dB).
const FUND_MIN: f32 = 0.1;
/// A note within this of one already found is the same note (cents).
const MERGE_CENTS: f32 = 70.0;
/// Two successive analyses agree within this (cents).
const AGREE_CENTS: f32 = 50.0;
/// The bins scaled by a cancellation, each side of the peak (the Hann main lobe).
const LOBE: usize = 2;

/// One note of a reading.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Note {
    /// Fundamental (Hz), parabolic-interpolated.
    pub hz: f32,
    /// Its harmonic-sum salience (the residual's weighted partial amplitudes, linear).
    pub salience: f32,
}

/// One window's tables: the FFT plan, the Hann window and every candidate's partial bands.
struct Win {
    n: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    hann: Vec<f32>,
    /// per candidate: the first band's index into `bands` and the number of partials
    first: Vec<u32>,
    count: Vec<u8>,
    /// (lo bin, hi bin, weight)
    bands: Vec<(u16, u16, f32)>,
}

/// The chord analyser (a side chain: no audio passes through it).
pub struct Analyzer {
    sr: f32,
    dec: usize,
    phase: usize,
    rate: f32,
    aa: [Svf; 2],
    max_notes: usize,
    f_min: f32,
    /// candidate cells: centre f0 (Hz); the analysis starts at `c_from` (`set_f_min`)
    cands: Vec<f32>,
    c_from: usize,
    wins: Vec<Win>,
    ring: Vec<f32>,
    at: usize,
    /// decimated samples since the onset (saturating)
    since: u64,
    active: bool,
    early_on: bool,
    frame: Vec<f32>,
    spec: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    mag: Vec<f32>,
    res: Vec<f32>,
    sal: Vec<f32>,
    /// the last analysis, the confirmed reading
    last: [Note; MAX_NOTES],
    last_n: usize,
    have_last: bool,
    conf: [Note; MAX_NOTES],
    conf_n: usize,
    analyses: u64,
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn cents(a: f32, b: f32) -> f32 {
    1200.0 * (a / b).log2()
}

impl Analyzer {
    pub fn new() -> Self {
        Self {
            sr: 48000.0,
            dec: 8,
            phase: 0,
            rate: RATE,
            aa: [Svf::new(), Svf::new()],
            max_notes: 4,
            f_min: 70.0,
            cands: Vec::new(),
            c_from: 0,
            wins: Vec::new(),
            ring: Vec::new(),
            at: 0,
            since: 0,
            active: true,
            early_on: false,
            frame: Vec::new(),
            spec: Vec::new(),
            scratch: Vec::new(),
            mag: Vec::new(),
            res: Vec::new(),
            sal: Vec::new(),
            last: [Note::default(); MAX_NOTES],
            last_n: 0,
            have_last: false,
            conf: [Note::default(); MAX_NOTES],
            conf_n: 0,
            analyses: 0,
        }
    }

    /// Sizes everything (allocates): the decimator for `sample_rate`, candidates from `f_min` to
    /// 1.2 kHz, `max_notes` (1 ..= 6) iterations per analysis.
    pub fn prepare(&mut self, sample_rate: f32, f_min: f32, max_notes: usize) {
        self.sr = sample_rate;
        self.dec = (sample_rate / RATE).round().max(1.0) as usize;
        self.rate = sample_rate / self.dec as f32;
        let fc = PART_TOP * self.rate;
        self.aa[0].set(SvfMode::LowPass, fc, 0.541_196, 0.0, sample_rate);
        self.aa[1].set(SvfMode::LowPass, fc, 1.306_563, 0.0, sample_rate);
        self.max_notes = max_notes.clamp(1, MAX_NOTES);
        self.f_min = f_min.clamp(20.0, F_MAX / 2.0);
        // candidate cells, 1/8 semitone
        let n_c = (CELLS_PER_ST * 12.0 * (F_MAX / self.f_min).log2()).ceil() as usize + 1;
        self.cands = (0..n_c).map(|i| self.f_min * 2f32.powf(i as f32 / (12.0 * CELLS_PER_ST))).collect();
        let half = 2f32.powf(0.5 / (12.0 * CELLS_PER_ST));
        let top = PART_TOP * self.rate;
        let mut planner = RealFftPlanner::<f32>::new();
        let rate = self.rate;
        self.wins = WINDOWS
            .iter()
            .map(|&n| {
                let fft = planner.plan_fft_forward(n);
                let hann = (0..n).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos()).collect();
                let bin = rate / n as f32;
                let kmax = n / 2;
                let mut first = Vec::with_capacity(n_c);
                let mut count = Vec::with_capacity(n_c);
                let mut bands = Vec::with_capacity(n_c * M_MAX);
                for &f0 in &self.cands {
                    first.push(bands.len() as u32);
                    let mut c = 0u8;
                    for m in 1..=M_MAX {
                        let fm = m as f32 * f0;
                        if fm > top {
                            break;
                        }
                        let mf = m as f32;
                        let lo = (mf * f0 / half / bin).floor() as isize - 1;
                        let hi = (mf * f0 * half * (1.0 + 0.5 * B_INH * mf * mf) / bin).ceil() as isize + 1;
                        let lo = lo.clamp(1, kmax as isize - 1) as u16;
                        let hi = hi.clamp(lo as isize, kmax as isize - 1) as u16;
                        bands.push((lo, hi, (f0 + ALPHA) / (mf * f0 + BETA)));
                        c += 1;
                    }
                    count.push(c);
                }
                Win { n, fft, hann, first, count, bands }
            })
            .collect();
        let nmax = WINDOWS[2];
        let scr = self.wins.iter().map(|w| w.fft.get_scratch_len()).max().unwrap_or(0);
        self.ring = vec![0.0; nmax];
        self.frame = vec![0.0; nmax];
        self.spec = vec![Complex::default(); nmax / 2 + 1];
        self.scratch = vec![Complex::default(); scr.max(1)];
        self.mag = vec![0.0; nmax / 2 + 1];
        self.res = vec![0.0; nmax / 2 + 1];
        self.sal = vec![0.0; n_c];
        self.c_from = 0;
        self.reset();
    }

    /// Raises the lowest candidate within what `prepare` sized (no allocation): a caller's `lowest`
    /// knob. Below the prepared floor it is the prepared floor.
    pub fn set_f_min(&mut self, hz: f32) {
        self.c_from = self.cands.iter().position(|&f| f >= hz * 0.995).unwrap_or(self.cands.len().saturating_sub(1));
    }

    /// Silence, no reading, the windows start now.
    pub fn reset(&mut self) {
        for s in self.aa.iter_mut() {
            s.reset();
        }
        self.phase = 0;
        self.ring.iter_mut().for_each(|v| *v = 0.0);
        self.at = 0;
        self.since = 0;
        self.last_n = 0;
        self.have_last = false;
        self.conf_n = 0;
        self.analyses = 0;
    }

    /// A pick: the windows restart (none reaches back before it) and the readings are cleared.
    pub fn onset(&mut self) {
        self.since = 0;
        self.last_n = 0;
        self.have_last = false;
        self.conf_n = 0;
    }

    /// Analyses run only while active (the caller's YIN is unclear); the history always fills.
    pub fn set_active(&mut self, on: bool) {
        self.active = on;
    }

    /// Research only: keep the last unconfirmed reading for [`Self::early`].
    pub fn set_early(&mut self, on: bool) {
        self.early_on = on;
    }

    /// The analysis rate (Hz).
    pub fn rate(&self) -> f32 {
        self.rate
    }

    /// Analyses run since `reset` (diagnostics).
    pub fn analyses(&self) -> u64 {
        self.analyses
    }

    /// One input sample (host rate). Returns true when an analysis ran on it.
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
        self.at = (self.at + 1) % self.ring.len();
        self.since = self.since.saturating_add(1);
        if !self.active || self.since < WINDOWS[0] as u64 || !(self.since - WINDOWS[0] as u64).is_multiple_of(HOP as u64) {
            return false;
        }
        let w = WINDOWS.iter().rposition(|&n| n as u64 <= self.since).unwrap_or(0);
        self.analyse(w);
        true
    }

    /// The confirmed reading: at most `max_notes`, sorted by pitch; empty until two analyses agree.
    pub fn notes(&self) -> &[Note] {
        &self.conf[..self.conf_n]
    }

    /// Time since the onset the reading belongs to (ms).
    pub fn age_ms(&self) -> f32 {
        self.since as f32 * 1000.0 / self.rate
    }

    /// Research only (gfx-polysynth's Poly): the last analysis, unconfirmed; empty unless
    /// [`Self::set_early`]. Never used to start a sound.
    pub fn early(&self) -> &[Note] {
        if self.early_on {
            &self.last[..self.last_n]
        } else {
            &[]
        }
    }

    fn analyse(&mut self, wi: usize) {
        self.analyses += 1;
        let n = self.wins[wi].n;
        let len = self.ring.len();
        {
            let w = &self.wins[wi];
            for i in 0..n {
                // the newest n samples, oldest first
                let k = (self.at + len - n + i) % len;
                self.frame[i] = self.ring[k] * w.hann[i];
            }
            let _ = w.fft.process_with_scratch(&mut self.frame[..n], &mut self.spec[..n / 2 + 1], &mut self.scratch);
        }
        let kn = n / 2 + 1;
        for k in 0..kn {
            let m = self.spec[k].norm();
            self.mag[k] = m;
            self.res[k] = m;
        }
        let mut found = [Note::default(); MAX_NOTES];
        let mut nf = 0usize;
        let mut first_sal = 0.0f32;
        let bin = self.rate / n as f32;
        for _ in 0..self.max_notes {
            // salience of every candidate on the residual
            let (mut best, mut best_s) = (usize::MAX, 0.0f32);
            {
                let w = &self.wins[wi];
                for c in self.c_from..self.cands.len() {
                    let f = w.first[c] as usize;
                    let mut s = 0.0f32;
                    for &(lo, hi, g) in &w.bands[f..f + w.count[c] as usize] {
                        let mut m = 0.0f32;
                        for k in lo as usize..=hi as usize {
                            m = m.max(self.res[k]);
                        }
                        s += g * m;
                    }
                    self.sal[c] = s;
                    if s > best_s {
                        best_s = s;
                        best = c;
                    }
                }
            }
            if best == usize::MAX || best_s <= 1e-9 || (nf > 0 && best_s < STOP * first_sal) {
                break;
            }
            if nf == 0 {
                first_sal = best_s;
            }
            let f0 = self.refine(wi, best, bin);
            // a note has its own fundamental: a peak inside its first band, at least −20 dB re its
            // strongest of partials 1 – 4 (else it is a "missing fundamental" made of other notes'
            // partials: C#4's 3rd and 6th read as a 92 Hz note in the 85 ms window of an A chord)
            let real = self.has_fundamental(wi, best);
            self.cancel(f0, bin, kn);
            // a note found again (the residual its smoothed cancellation left) is not a new note
            if real && found[..nf].iter().all(|n| cents(f0, n.hz).abs() > MERGE_CENTS) {
                found[nf] = Note { hz: f0, salience: best_s };
                nf += 1;
            }
        }
        found[..nf].sort_by(|a, b| a.hz.total_cmp(&b.hz));
        // confirmation: two successive analyses agree
        let agree = self.have_last && self.last_n == nf && nf > 0 && (0..nf).all(|i| cents(found[i].hz, self.last[i].hz).abs() <= AGREE_CENTS);
        self.last = found;
        self.last_n = nf;
        self.have_last = true;
        if agree {
            self.conf = found;
            self.conf_n = nf;
        }
    }

    /// Whether the candidate's first band holds a peak of the residual (not an edge maximum: a
    /// neighbour's skirt) at least −20 dB re the strongest of its first four bands.
    fn has_fundamental(&self, wi: usize, c: usize) -> bool {
        let w = &self.wins[wi];
        let f = w.first[c] as usize;
        let n = (w.count[c] as usize).min(4);
        if n == 0 {
            return false;
        }
        let band_max = |lo: u16, hi: u16| -> (usize, f32) {
            let (lo, hi) = (lo as usize, hi as usize);
            let p = (lo..=hi).max_by(|&a, &b| self.res[a].total_cmp(&self.res[b])).unwrap_or(lo);
            (p, self.res[p])
        };
        let (lo, hi, _) = w.bands[f];
        let (p1, a1) = band_max(lo, hi);
        let interior = p1 > lo as usize && p1 < hi as usize;
        let strongest = w.bands[f..f + n].iter().map(|&(l, h, _)| band_max(l, h).1).fold(0.0f32, f32::max);
        interior && a1 >= FUND_MIN * strongest
    }

    /// The candidate's f0 from its first three partials: each partial's peak in the magnitude
    /// spectrum, parabolic on the log magnitude, divided by its number; weighted by amplitude.
    fn refine(&self, wi: usize, c: usize, bin: f32) -> f32 {
        let w = &self.wins[wi];
        let f = w.first[c] as usize;
        let (mut num, mut den) = (0.0f32, 0.0f32);
        for (m, &(lo, hi, _)) in w.bands[f..f + (w.count[c] as usize).min(3)].iter().enumerate() {
            let (lo, hi) = (lo as usize, hi as usize);
            let p = (lo..=hi).max_by(|&a, &b| self.mag[a].total_cmp(&self.mag[b])).unwrap_or(lo);
            let a = self.mag[p];
            if a <= 0.0 || p == 0 || p + 1 >= self.mag.len() {
                continue;
            }
            let (l0, l1, l2) = (self.mag[p - 1].max(1e-12).ln(), a.max(1e-12).ln(), self.mag[p + 1].max(1e-12).ln());
            let d = l0 - 2.0 * l1 + l2;
            let off = if d.abs() > 1e-12 { (0.5 * (l0 - l2) / d).clamp(-0.5, 0.5) } else { 0.0 };
            // only a peak inside the band (an edge maximum is a neighbour's skirt)
            if p == lo || p == hi {
                continue;
            }
            num += a * (p as f32 + off) * bin / (m + 1) as f32;
            den += a;
        }
        if den > 0.0 {
            num / den
        } else {
            self.cands[c]
        }
    }

    /// Cancels a note's partials from the residual by their smoothed amplitudes.
    fn cancel(&mut self, f0: f32, bin: f32, kn: usize) {
        let top = PART_TOP * self.rate;
        // (bin, residual amplitude, the original spectrum's amplitude there)
        let mut peaks = [(0usize, 0.0f32, 0.0f32); M_MAX];
        let mut np = 0;
        for m in 1..=M_MAX {
            let mf = m as f32;
            let fm = mf * f0 * (1.0 + 0.5 * B_INH * mf * mf * 0.5);
            if fm > top {
                break;
            }
            // the band: ±(a quarter tone at m = 1, ± 1.5 % beyond) and at least ±1 bin
            let tol = (fm * 0.015).max(bin);
            let lo = ((fm - tol) / bin).floor().max(1.0) as usize;
            let hi = (((fm + tol) / bin).ceil() as usize).min(kn - 2);
            if lo > hi {
                break;
            }
            // located in the original spectrum (a partial another note already took is still found)
            let p = (lo..=hi).max_by(|&a, &b| self.mag[a].total_cmp(&self.mag[b])).unwrap_or(lo);
            peaks[np] = (p, self.res[p], self.mag[p]);
            np += 1;
        }
        for i in 0..np {
            let (p, a, _) = peaks[i];
            if a <= 0.0 {
                continue;
            }
            // the smoothed amplitude: the geometric mean of the neighbours in the original spectrum (a
            // natural spectrum falling as 1 / k is cancelled whole; a partial that stands out above
            // them, shared with a note an octave above, keeps its excess; a neighbour already taken by
            // another note's cancellation still counts). The fundamental is cancelled whole: a lower
            // note sharing it would have been found first.
            let sub = if i == 0 {
                a
            } else if i + 1 < np {
                a.min((peaks[i - 1].2 * peaks[i + 1].2).sqrt())
            } else {
                a.min(peaks[i - 1].2)
            };
            let keep = ((a - sub) / a).clamp(0.0, 1.0);
            let lo = p.saturating_sub(LOBE);
            let hi = (p + LOBE).min(kn - 1);
            for k in lo..=hi {
                self.res[k] *= keep;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plucked string: partials k = 1 … with 1 / k amplitudes, inharmonic (B), decaying (T60).
    fn string(f0: f32, b: f32, t60: f32, amp: f32, n: usize, sr: f32) -> Vec<f32> {
        let mut x = vec![0.0f32; n];
        let mut k = 1;
        loop {
            let kf = k as f32;
            let fk = kf * f0 * (1.0 + b * kf * kf).sqrt();
            if fk > 0.45 * sr || k > 30 {
                break;
            }
            let a = amp / kf;
            let decay = (-6.9 / (t60 / (1.0 + 0.3 * kf)) / sr).exp();
            let mut g = a;
            let w = 2.0 * std::f32::consts::PI * fk / sr;
            let ph = 0.37 * kf;
            for (i, v) in x.iter_mut().enumerate() {
                *v += g * (w * i as f32 + ph).sin();
                g *= decay;
            }
            k += 1;
        }
        x
    }

    fn chord(notes: &[f32], sr: f32, secs: f32) -> Vec<f32> {
        let n = (sr * secs) as usize;
        let mut x = vec![0.0f32; n];
        for (i, &f) in notes.iter().enumerate() {
            let s = string(f, 1e-4, 4.0, 0.15, n, sr);
            // a strum: 8 ms between strings
            let d = (i as f32 * 0.008 * sr) as usize;
            for j in d..n {
                x[j] += s[j - d];
            }
        }
        x
    }

    fn run(x: &[f32], sr: f32, max_notes: usize) -> (Vec<Note>, f32) {
        let mut a = Analyzer::new();
        a.prepare(sr, 70.0, max_notes);
        let mut first = None;
        for (i, &v) in x.iter().enumerate() {
            a.push(v);
            if first.is_none() && !a.notes().is_empty() {
                first = Some(i as f32 / sr);
            }
        }
        (a.notes().to_vec(), first.unwrap_or(f32::INFINITY))
    }

    fn matches(found: &[Note], want: &[f32]) -> bool {
        found.len() == want.len() && found.iter().zip(want).all(|(n, &w)| cents(n.hz, w).abs() < 30.0)
    }

    #[test]
    fn names_a_single_note() {
        for &sr in &[44100.0f32, 48000.0, 96000.0] {
            for &f in &[82.41f32, 110.0, 196.0, 329.6, 659.3] {
                let x = string(f, 1e-4, 4.0, 0.3, (sr * 0.6) as usize, sr);
                let (notes, t) = run(&x, sr, 4);
                assert!(matches(&notes, &[f]), "{f} Hz at {sr}: {notes:?}");
                assert!(t < 0.2, "{f} Hz confirmed at {t:.3} s");
            }
        }
    }

    /// The lowest and highest notes of a reading against the played ones, every note a played one.
    fn judged(found: &[Note], played: &[f32]) -> (bool, bool, bool) {
        let real = !found.is_empty() && found.iter().all(|n| played.iter().any(|&p| cents(n.hz, p).abs() < 30.0));
        let lo = played.iter().copied().fold(f32::MAX, f32::min);
        let hi = played.iter().copied().fold(0.0, f32::max);
        let l = found.first().is_some_and(|n| cents(n.hz, lo).abs() < 30.0);
        let h = found.last().is_some_and(|n| cents(n.hz, hi).abs() < 30.0);
        (real, l, h)
    }

    #[test]
    fn names_chords() {
        let sr = 48000.0;
        // E2-B2-E3 (power chord), A2-E3-A3-C#4, a D major triad, E2-B2-E3-G3 (E minor, 4 strings)
        let cases: [&[f32]; 4] = [&[82.41, 123.47, 164.81], &[110.0, 164.81, 220.0, 277.18], &[146.83, 185.0, 220.0], &[82.41, 123.47, 164.81, 196.0]];
        let mut ok = true;
        for played in cases {
            let x = chord(played, sr, 0.8);
            let (notes, t) = run(&x, sr, 4);
            let (real, l, h) = judged(&notes, played);
            eprintln!("{played:?}: {:?} at {t:.3} s (real {real}, lowest {l}, highest {h})", notes.iter().map(|n| (n.hz, n.salience)).collect::<Vec<_>>());
            ok &= real && l && h && t <= 0.4;
        }
        assert!(ok);
    }

    #[test]
    fn random_voicings() {
        // 200 renders (gfx-sustainer §5 Chords): random root E2 – A3, power chord, power chord + major
        // or minor third, triad; each string ±10 ¢
        let sr = 48000.0;
        let mut rng = crate::lfo::Rng::new(0x5EED);
        let shapes: [&[f32]; 4] = [&[0.0, 7.0, 12.0], &[0.0, 7.0, 12.0, 16.0], &[0.0, 4.0, 7.0], &[0.0, 7.0, 12.0, 15.0]];
        let (mut n, mut good_l, mut good_h, mut good_all, mut late) = (0, 0, 0, 0, 0);
        for r in 0..200 {
            let root = 82.41 * 2f32.powf((rng.next_f64() * 17.0) as f32 / 12.0);
            let shape = shapes[r % shapes.len()];
            let played: Vec<f32> = shape.iter().map(|&st| root * 2f32.powf((st + (rng.next_f64() as f32 - 0.5) * 0.2) / 12.0)).collect();
            let x = chord(&played, sr, 0.6);
            let (notes, t) = run(&x, sr, 4);
            let (real, l, h) = judged(&notes, &played);
            n += 1;
            good_l += l as usize;
            good_h += h as usize;
            good_all += real as usize;
            late += (t > 0.4) as usize;
        }
        eprintln!("random voicings: lowest {good_l}/{n}, highest {good_h}/{n}, only played notes {good_all}/{n}, later than 0.4 s {late}");
        assert!(good_l * 100 >= 95 * n, "lowest {good_l}/{n}");
        assert!(good_h * 100 >= 90 * n, "highest {good_h}/{n}");
        assert!(late * 100 <= 5 * n);
    }

    #[test]
    fn power_chord_lowest_is_the_root_not_the_sub_octave() {
        let sr = 48000.0;
        for &root in &[82.41f32, 98.0, 110.0, 146.83] {
            let x = chord(&[root, root * 1.4983, root * 2.0], sr, 0.6);
            let (notes, _) = run(&x, sr, 4);
            assert!(!notes.is_empty(), "{root}: named");
            let low = notes[0].hz;
            assert!(cents(low, root).abs() < 30.0, "{root}: lowest {low:.1} Hz ({notes:?})");
        }
    }

    #[test]
    fn four_note_reading_unchanged_by_six() {
        // the iterations are the same whatever max_notes, a 4-note build stops after four: a 6-note
        // build reads at least the 4-note build's notes
        let sr = 48000.0;
        for played in [&[110.0f32, 164.81, 220.0, 277.18][..], &[82.41, 123.47, 164.81], &[98.0, 123.47, 146.83, 196.0, 246.94, 392.0]] {
            let x = chord(played, sr, 0.8);
            let (a, _) = run(&x, sr, 4);
            let (b, _) = run(&x, sr, 6);
            assert!(a.len() <= 4 && b.len() <= 6);
            assert!(a.iter().all(|n| b.iter().any(|m| m.hz == n.hz)), "{a:?} within {b:?}");
        }
    }

    #[test]
    fn onset_restarts_and_silence_is_empty() {
        let sr = 48000.0;
        let mut a = Analyzer::new();
        a.prepare(sr, 70.0, 4);
        for _ in 0..(sr as usize) {
            a.push(0.0);
        }
        assert!(a.notes().is_empty());
        let x = string(110.0, 1e-4, 4.0, 0.3, (sr * 0.5) as usize, sr);
        for &v in &x {
            a.push(v);
        }
        assert!(!a.notes().is_empty());
        a.onset();
        assert!(a.notes().is_empty() && a.age_ms() == 0.0);
        // the first analysis comes 512 decimated samples after the onset (85 ms)
        let mut ran = 0;
        for &v in x.iter().take((sr * 0.084) as usize) {
            ran += a.push(v) as usize;
        }
        assert_eq!(ran, 0);
        assert!(a.early().is_empty());
    }

    #[test]
    fn deterministic() {
        let sr = 48000.0;
        let x = chord(&[110.0, 164.81, 220.0], sr, 0.6);
        let (a, _) = run(&x, sr, 4);
        let (b, _) = run(&x, sr, 4);
        assert_eq!(a, b);
    }
}
