// SPDX-License-Identifier: GPL-3.0-or-later
//! The Spectral core (README §2.2) at the bank rate (22.05 – 24 kHz at every host rate): gfx-dsp
//! `spectral` (banks, selection, rotations, f̂, purity, hold, floor, handover, ownership mask: moved
//! there unchanged so that gfx-organ shares it) with this pedal's shaper.
//!
//! - **Harmonic shaper** ([`WaveShaper`]): y_b = s_b · g_b · 2|z_b| / |H_b(f̂_b)| · Σ_n Re(c_n uⁿ),
//!   n ≤ 9 and n f̂ under fcap = 0.42 × the bank rate (each harmonic faded by its margin to fcap over
//!   500 Hz); purity gate w_b = exp(−σ²_IF / (0.25 B_b)²) between the shaped band and the band itself;
//!   `octave` (u², u⁴, branch-tracked √u, ⁴√u), `sub` (Re √u under 400 Hz), `detune` (a copy of every
//!   harmonic turned by a rotor running `detune` cents sharp of the band), `noise` (a band of noise
//!   around the partial).
//!
//! The move into gfx-dsp is guarded by a byte-identical null of this pedal's renders (`tests.rs`).

use gfx_dsp::filterbank::{self, BandSpec};
use gfx_dsp::noise::White;
use gfx_dsp::spectral::{self as sp, Analysis, Shaper};
use std::f64::consts::{PI, TAU};

pub use gfx_dsp::spectral::{clean_grid, fast_atan2, fast_exp2, fast_grid, fast_log2, grid, BankMode, FCAP, GRID_LOW_HZ, TICK, TOP_HZ};

/// Harmonics per band.
pub const NH: usize = 9;
/// A harmonic fades over its last this many Hz under fcap.
const FADE_HZ: f64 = sp::FADE_HZ;
/// Sub and the branch-tracked roots: bands under this (Hz) carry `sub`.
const SUB_BELOW: f64 = 400.0;

/// Σ_n w_n Re(c_n vⁿ), n ≤ [`NH`], w_n = clamp((fcap − n f) / 500 Hz, 0, 1).
#[inline(always)]
fn series(vr: f64, vi: f64, f: f64, fcap: f64, c: &[(f64, f64); NH]) -> f64 {
    let (mut pr, mut pi) = (vr, vi);
    let mut s = 0.0;
    let mut fnn = f;
    for cn in c.iter() {
        let margin = fcap - fnn;
        if margin <= 0.0 {
            break;
        }
        let w = (margin * (1.0 / FADE_HZ)).min(1.0);
        s += w * (cn.0 * pr - cn.1 * pi);
        let qr = pr * vr - pi * vi;
        pi = pr * vi + pi * vr;
        pr = qr;
        fnn += f;
    }
    s
}

/// The waveform coefficients c_n (README §2.2), normalised so Σ |c_n|² = 1 (the RMS of a unit sine).
pub fn coefs(wave: usize, pw: f64) -> [(f64, f64); NH] {
    let mut c = [(0.0, 0.0); NH];
    for (i, cn) in c.iter_mut().enumerate() {
        let n = (i + 1) as f64;
        *cn = match wave {
            // Saw: −j / n (Σ sin nθ / n, the falling saw)
            0 => (0.0, -1.0 / n),
            // Pulse: sin(π n pw) / n (a pulse centred on θ = 0)
            1 => ((PI * n * pw).sin() / n, 0.0),
            // Triangle: −j (−1)^((n−1)/2) / n², odd n
            2 => {
                if i % 2 == 0 {
                    let s = if (i / 2) % 2 == 0 { 1.0 } else { -1.0 };
                    (0.0, -s / (n * n))
                } else {
                    (0.0, 0.0)
                }
            }
            // Sine: c₁ = 1
            _ => {
                if i == 0 {
                    (1.0, 0.0)
                } else {
                    (0.0, 0.0)
                }
            }
        };
    }
    let e: f64 = c.iter().map(|x| x.0 * x.0 + x.1 * x.1).sum();
    let g = if e > 1e-12 { 1.0 / e.sqrt() } else { 0.0 };
    for cn in c.iter_mut() {
        cn.0 *= g;
        cn.1 *= g;
    }
    c
}

/// What the engine sets on the core at each tick.
#[derive(Clone, Copy, Debug)]
pub struct CoreSettings {
    pub bank: BankMode,
    /// The harmonic law (already crossfaded by the engine).
    pub c: [(f64, f64); NH],
    /// `octave` now and the one being faded to, and the fade (0 = `oct_a` only).
    pub oct_a: i32,
    pub oct_b: i32,
    pub oct_x: f64,
    /// `sub` 0 – 1, `noise` 0 – 1, `compress` 0 – 1.
    pub sub: f64,
    pub noise: f64,
    pub compress: f64,
    /// `detune` (cents).
    pub detune: f64,
    /// `thresh` (dBFS), `lowest` (Hz).
    pub thresh_db: f64,
    pub lowest: f64,
    /// The engine fade (0 Spectral, 1 Mono) and `residue` (0 – 1).
    pub mono: f64,
    pub residue: f64,
}

impl Default for CoreSettings {
    fn default() -> Self {
        Self { bank: BankMode::Dual, c: coefs(0, 0.5), oct_a: 0, oct_b: 0, oct_x: 0.0, sub: 0.0, noise: 0.0, compress: 0.4, detune: 0.0, thresh_db: -66.0, lowest: 70.0, mono: 0.0, residue: 1.0 }
    }
}

/// The shaper's per-band state of one bank.
#[derive(Clone, Debug, Default)]
struct WaveBands {
    low: Vec<bool>,
    na: Vec<f64>,
    vr_last: Vec<f64>,
    vi_last: Vec<f64>,
    s1r: Vec<f64>,
    s1i: Vec<f64>,
    s2r: Vec<f64>,
    s2i: Vec<f64>,
    dr: Vec<f64>,
    di: Vec<f64>,
    der: Vec<f64>,
    dei: Vec<f64>,
    nr: Vec<f64>,
    ni: Vec<f64>,
    pwr: Vec<f64>,
}

impl WaveBands {
    fn prepare(&mut self, grid: &[BandSpec], fb: f64) {
        let n = grid.len();
        for v in [&mut self.na, &mut self.vr_last, &mut self.vi_last, &mut self.s1r, &mut self.s1i, &mut self.s2r, &mut self.s2i, &mut self.dr, &mut self.di, &mut self.der, &mut self.dei, &mut self.nr, &mut self.ni, &mut self.pwr] {
            *v = vec![0.0; n];
        }
        self.low = vec![false; n];
        for b in 0..n {
            let s = grid[b];
            self.low[b] = s.centre < SUB_BELOW;
            // the noise band: a complex one-pole of corner max(B, f / 4)
            let nc = s.width.max(0.25 * s.centre);
            self.na[b] = 1.0 - (-TAU * nc / fb).exp();
        }
    }

    fn reset(&mut self) {
        for v in [&mut self.vr_last, &mut self.vi_last, &mut self.s1i, &mut self.s2i, &mut self.di, &mut self.nr, &mut self.ni, &mut self.pwr] {
            v.iter_mut().for_each(|x| *x = 0.0);
        }
        for b in 0..self.low.len() {
            self.s1r[b] = 1.0;
            self.s2r[b] = 1.0;
            self.dr[b] = 1.0;
            self.der[b] = 1.0;
            self.dei[b] = 0.0;
        }
    }
}

/// What every band of a sample shares.
#[derive(Clone, Copy, Debug, Default)]
struct RenderConsts {
    c: [(f64, f64); NH],
    /// c₁ / |c₁| (the unshaped band's phase)
    c1r: f64,
    c1i: f64,
    fcap: f64,
    oct_a: i32,
    oct_b: i32,
    oct_x: f64,
    need_r1: bool,
    need_r2: bool,
    main: f64,
    copy: f64,
    ratio: f64,
    sub: f64,
    noise: f64,
    measure: bool,
    a_pw: f64,
    a_pw_att: f64,
}

/// gfx-polysynth's harmonic shaper on the gfx-dsp core.
#[derive(Clone, Debug, Default)]
pub struct WaveShaper {
    s: CoreSettings,
    fb: f64,
    bands: [WaveBands; 2],
    r: RenderConsts,
    noise: White,
}

impl Shaper for WaveShaper {
    fn prepare(&mut self, clean: &[BandSpec], fast: &[BandSpec], fb: f64) {
        self.fb = fb;
        self.bands[0].prepare(clean, fb);
        self.bands[1].prepare(fast, fb);
        self.r.fcap = sp::FCAP * fb;
        // the owned bands' power: attack 2 ms, release 20 ms (a 10 ms mean lagged the attack by 3 dB)
        self.r.a_pw = 1.0 - (-1.0 / (0.020 * fb)).exp();
        self.r.a_pw_att = 1.0 - (-1.0 / (0.002 * fb)).exp();
    }

    fn reset(&mut self) {
        self.bands[0].reset();
        self.bands[1].reset();
        self.noise = White::new(0x5917_C0DE);
        self.noise.reset();
    }

    fn tick(&mut self, a: &Analysis) {
        let s = self.s;
        // the detune rotors' step: δ times the (transposed) band's phase increment
        let delta = (s.detune / 1200.0).exp2() - 1.0;
        let fac = (s.oct_b as f64).exp2();
        for (wb, bk) in self.bands.iter_mut().zip([&a.clean, &a.fast]) {
            for b in 0..bk.n {
                if s.detune > 0.0 {
                    let w = delta * TAU * bk.fhat[b] * fac / a.fb;
                    wb.der[b] = w.cos();
                    wb.dei[b] = w.sin();
                    let m = (wb.dr[b] * wb.dr[b] + wb.di[b] * wb.di[b]).sqrt();
                    if m > 1e-12 {
                        wb.dr[b] /= m;
                        wb.di[b] /= m;
                    } else {
                        wb.dr[b] = 1.0;
                        wb.di[b] = 0.0;
                    }
                } else {
                    wb.dr[b] = 1.0;
                    wb.di[b] = 0.0;
                }
            }
        }
        // ---- what every band shares this tick
        let r = &mut self.r;
        r.c = s.c;
        let m1 = (s.c[0].0 * s.c[0].0 + s.c[0].1 * s.c[0].1).sqrt();
        (r.c1r, r.c1i) = if m1 > 1e-9 { (s.c[0].0 / m1, s.c[0].1 / m1) } else { (1.0, 0.0) };
        r.oct_a = s.oct_a;
        r.oct_b = s.oct_b;
        r.oct_x = s.oct_x.clamp(0.0, 1.0);
        r.need_r1 = s.sub > 0.0 || (r.oct_a < 0 && r.oct_x < 1.0) || (r.oct_b < 0 && r.oct_x > 0.0);
        r.need_r2 = (r.oct_a == -2 && r.oct_x < 1.0) || (r.oct_b == -2 && r.oct_x > 0.0);
        // detune: the copy and the original at −3 dB each, faded in over the first 2 ¢
        let th = 0.25 * PI * (s.detune / 2.0).clamp(0.0, 1.0);
        r.main = th.cos();
        r.copy = if s.detune > 0.0 { th.sin() } else { 0.0 };
        r.ratio = (s.detune / 1200.0).exp2();
        r.sub = s.sub;
        r.noise = s.noise;
        r.measure = s.mono > 0.0;
    }

    #[inline(always)]
    fn band(&mut self, bank: usize, b: usize, ur: f64, ui: f64, m: f64, kk: f64, mk: f64, f: f64, pur: f64, out: &mut (f64, f64)) {
        let r = &self.r;
        let wb = &mut self.bands[bank];
        // the roots, when an octave down or `sub` needs them
        let (mut r1, mut r2) = ((1.0, 0.0), (1.0, 0.0));
        if r.need_r1 {
            r1 = sp::sqrt_branch(ur, ui, &mut wb.s1r[b], &mut wb.s1i[b]);
            if r.need_r2 {
                r2 = sp::sqrt_branch(r1.0, r1.1, &mut wb.s2r[b], &mut wb.s2i[b]);
            }
        }
        let amp = m * kk;
        let detune_on = r.copy > 0.0;
        let (mut s_main, mut s_copy) = (0.0, 0.0);
        let (dr, di) = (wb.dr[b], wb.di[b]);
        for (oct, w) in [(r.oct_a, 1.0 - r.oct_x), (r.oct_b, r.oct_x)] {
            if w <= 0.0 {
                continue;
            }
            let (vr, vi) = match oct {
                0 => (ur, ui),
                1 => (ur * ur - ui * ui, 2.0 * ur * ui),
                2 => {
                    let (ar, ai) = (ur * ur - ui * ui, 2.0 * ur * ui);
                    (ar * ar - ai * ai, 2.0 * ar * ai)
                }
                -1 => r1,
                _ => r2,
            };
            let fv = f * (oct as f64).exp2();
            let w1 = ((r.fcap - fv) * (1.0 / FADE_HZ)).clamp(0.0, 1.0);
            let sh = series(vr, vi, fv, r.fcap, &r.c);
            // the band unshaped: its fundamental at the wave's phase
            s_main += w * (pur * sh + (1.0 - pur) * w1 * (r.c1r * vr - r.c1i * vi));
            if detune_on {
                let (cr, ci) = (vr * dr - vi * di, vr * di + vi * dr);
                let fd = fv * r.ratio;
                let w1d = ((r.fcap - fd) * (1.0 / FADE_HZ)).clamp(0.0, 1.0);
                let sd = series(cr, ci, fd, r.fcap, &r.c);
                s_copy += w * (pur * sd + (1.0 - pur) * w1d * (r.c1r * cr - r.c1i * ci));
            }
            if oct == r.oct_b {
                wb.vr_last[b] = vr;
                wb.vi_last[b] = vi;
            }
        }
        if detune_on {
            let nr = dr * wb.der[b] - di * wb.dei[b];
            wb.di[b] = dr * wb.dei[b] + di * wb.der[b];
            wb.dr[b] = nr;
        }
        let mut y = r.main * s_main;
        if r.sub > 0.0 && wb.low[b] {
            y += r.sub * r1.0;
        }
        if r.noise > 0.0 {
            let a = wb.na[b];
            wb.nr[b] += (self.noise.next_f64() - wb.nr[b]) * a;
            wb.ni[b] += (self.noise.next_f64() - wb.ni[b]) * a;
            // unit RMS of Re(u ξ): the one-pole's variance a / (2 − a) of a uniform (1 / 3) source
            let g = ((2.0 - a) * 3.0 / a).sqrt() * std::f64::consts::FRAC_1_SQRT_2;
            y += r.noise * g * (ur * wb.nr[b] - ui * wb.ni[b]);
        }
        let ya = amp * y;
        if r.measure {
            // the band's power from its envelope (the shaped wave's RMS is that of a sine of the
            // same amplitude): no per-period ripple
            let p2 = 0.5 * amp * amp;
            wb.pwr[b] += (p2 - wb.pwr[b]) * if p2 > wb.pwr[b] { r.a_pw_att } else { r.a_pw };
        }
        out.0 += ya * mk;
        if detune_on {
            out.1 += amp * r.copy * s_copy * mk;
        }
    }
}

/// The Spectral core: gfx-dsp's analysis with [`WaveShaper`].
#[derive(Clone, Debug, Default)]
pub struct Core {
    core: sp::Core<WaveShaper>,
}

impl Core {
    pub fn new() -> Self {
        Self::default()
    }

    /// Designs both banks for the bank rate `fb` (allocates).
    pub fn prepare(&mut self, fb: f64) {
        self.core.prepare(fb);
    }

    pub fn reset(&mut self) {
        self.core.reset();
    }

    pub fn bank_rate(&self) -> f64 {
        self.core.bank_rate()
    }

    /// The settings for the coming ticks.
    pub fn set(&mut self, s: CoreSettings) {
        self.core.set(sp::CoreSettings { bank: s.bank, compress: s.compress, thresh_db: s.thresh_db, lowest: s.lowest, mono: s.mono, residue: s.residue });
        self.core.shaper.s = s;
    }

    /// A pick: every band goes to the fast bank (Dual).
    pub fn pick(&mut self) {
        self.core.pick();
    }

    /// The voice's note (Hz, after `octave`'s transposition is undone: the string's f0), 0 = none.
    pub fn set_owner(&mut self, hz: f64) {
        self.core.set_owner(hz);
    }

    /// One bank sample → (main, detune copy).
    #[inline]
    pub fn process(&mut self, x: f64) -> (f64, f64) {
        self.core.process(x)
    }

    // ---------------------------------------------------------------- the bridge

    /// The RMS (power follower 2 / 20 ms) of the core bands the owner takes, before the mask: the level the voice must
    /// start at (README §2.4, the bridge's step 4).
    pub fn owned_rms(&self, f0: f64) -> f64 {
        let mut p = 0.0;
        for (bk, wb) in [&self.core.a.clean, &self.core.a.fast].into_iter().zip(&self.core.shaper.bands) {
            for b in 0..bk.n {
                let fc = bk.spec[b].centre;
                let h = (fc / f0).round();
                if (1.0..=16.0).contains(&h) && (fc - h * f0).abs() <= (filterbank::erb(fc) / 6.0).max(bk.hs[b]) {
                    p += wb.pwr[b];
                }
            }
        }
        p.sqrt()
    }

    /// The phase (radians) of the loudest sounding band at `f` (the transposed fundamental), if any:
    /// the voice's oscillator starts there (the bridge's step 6).
    pub fn phase_at(&self, f: f64) -> Option<f64> {
        let mut best: Option<(f64, f64)> = None;
        for (bk, wb) in [&self.core.a.clean, &self.core.a.fast].into_iter().zip(&self.core.shaper.bands) {
            for b in 0..bk.n {
                if (bk.spec[b].centre - f).abs() > 1.5 * bk.hs[b] {
                    continue;
                }
                let w = bk.kt[b];
                if w > 0.0 && best.is_none_or(|(bw, _)| w > bw) {
                    best = Some((w, wb.vi_last[b].atan2(wb.vr_last[b])));
                }
            }
        }
        best.map(|(_, p)| p)
    }

    // ---------------------------------------------------------------- diagnostics (tests)

    /// The handover x of the clean band nearest `f` (1 = clean).
    pub fn x_at(&self, f: f64) -> f64 {
        self.core.x_at(f)
    }

    /// The bank designs (clean, fast).
    pub fn designs(&self) -> (&[BandSpec], &[BandSpec]) {
        self.core.designs()
    }

    /// The owned bands' mask-weighted gain (0 – 1 sounding share), max over the bands the owner holds.
    pub fn owned_mask(&self) -> f64 {
        self.core.owned_mask()
    }
}
