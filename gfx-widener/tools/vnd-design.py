#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline design of gfx-widener's two-band velvet sequences (README §2.2, "Velvet details").

Written from the papers (Alary, Politis & Välimäki, DAFx-17; Schlecht, Alary, Välimäki & Habets,
DAFx-18), no code copied. What differs from them: the filter feeds the *side* of an M/S matrix whose
mid is the dry, so the quantity to flatten is the one-channel level |1 ± g·V(f)| in auditory bands,
not |V(f)|.

The generator modelled here is exactly the one coded in `src/decorr.rs`:

    side = V_long ∗ LP(M) + V_short ∗ (M − LP(M))
    LP   = one-pole lp += a·(M − lp), a = 1 − exp(−2π·1000 / fs)
    V_long : 16 pulses at round(frac · time · fs) samples
    V_short: 14 pulses at round(frac · time / 4 · fs) samples

Each pulse is (frac, gain). Gains come from a nearly flat envelope (6 dB of decay over the sequence),
the pulse's sign, a balance of the long sequence's positive and negative pulses (Σ g = 0: no lean of
the sub-bass to one side) and a normalisation Σ g² = 1 per sequence (unit power gain: `amount` maps
to correlation).

Constraints, all evaluated at once over `time` 20, 30, 40 ms × 44.1, 48, 96 kHz (tap rounding
included), the cost being the worst case:
  (1) one-channel level, L and R, at g = 0.6, in ERB bands 200 Hz – 16 kHz and in third-octaves,
      re the channel's own mean (the mean power of its bins 200 Hz – 16 kHz);
  (2) side level per ERB band: the power sum |L|² + |R|² within ±1 dB (§5);
  (3) Σ g = 0 on V_long (by construction) and Σ g² = 1 on each;
  (4) the generated sides of the 8 variants pairwise correlated ≤ 0.2 on pink noise 200 Hz – 12 kHz
      (each variant is optimised against the ones before it);
  (5) regularity: the largest normalised autocorrelation of one channel's impulse response at lags
      0.5 – 40 ms (reported; a flat envelope keeps it low by itself);
  (6) the side's power gain on pink noise 200 Hz – 12 kHz within ±0.2 dB (the correlation law: a
      first run without it let the side sink by 0.7 – 1.4 dB in the lows, where the colour is
      hardest, and r missed the law by 0.1 at g = 1);
  (7) 90 % of the long sequence's energy within 0.82 · `time` (the first run reached 21 ms at 20 ms
      below 500 Hz; §5 asks ≤ 20).

Search: simulated annealing on (positions, signs), with the response of every configuration updated
incrementally (one pulse moves at a time). Deterministic (fixed seeds).

Usage:
    python3 gfx-widener/tools/vnd-design.py            # design, report, write src/vnd_tables.rs
    python3 gfx-widener/tools/vnd-design.py --check    # report the committed tables only
    python3 gfx-widener/tools/vnd-design.py --allpass  # the Allpass generator's 8 sets → src/ap_tables.rs
    python3 gfx-widener/tools/vnd-design.py --single   # the single-band A/B reference (not shipped)
"""

import argparse
import os
import re
import sys
from multiprocessing import Pool

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "src", "vnd_tables.rs")

N_LONG, N_SHORT, N_VARIANTS = 16, 14, 8
SHORT_RATIO = 4.0  # V_short spans time / 4
SPLIT_HZ = 1000.0  # the one-pole between the two bands
DECAY_DB = 6.0  # envelope decay over each sequence
MIN_LONG, MIN_SHORT = 0.025, 0.1  # first pulse ≥ 0.5 ms at time 20 ms
MIN_GAP = 0.006  # distinct pulses after rounding (≥ 1.3 samples at 44.1 kHz, 20 ms, short)
G = 0.6  # amount × width of the specification
TIMES = (20.0, 30.0, 40.0)
RATES = (44100.0, 48000.0, 96000.0)
CONFIGS = [(t, fs) for t in TIMES for fs in RATES]

# ---------------------------------------------------------------- frequency grid and bands
F = np.arange(150.0, 16500.0, 5.0)  # uniform: band means by cumulative sums


def erb_bands(flo=200.0, fhi=16000.0):
    out, fc = [], flo
    while fc <= fhi:
        bw = 24.7 * (4.37 * fc / 1000.0 + 1.0)
        out.append((fc - bw / 2, fc + bw / 2))
        fc += bw / 2
    return out


def third_bands(flo=200.0, fhi=16000.0):
    fc = 1000.0 * 2.0 ** (np.arange(-7, 13) / 3.0)
    fc = fc[(fc >= flo) & (fc <= fhi)]
    return [(c * 2 ** (-1 / 6), c * 2 ** (1 / 6)) for c in fc]


def edges(bands, f=F):
    lo = np.searchsorted(f, [b[0] for b in bands])
    hi = np.searchsorted(f, [b[1] for b in bands])
    return lo, np.maximum(hi, lo + 1)


ERB_LO, ERB_HI = edges(erb_bands())
TH_LO, TH_HI = edges(third_bands())
MEAN_LO, MEAN_HI = np.searchsorted(F, 200.0), np.searchsorted(F, 16000.0)
CORR = (F >= 200.0) & (F <= 12000.0)
PINK = 1.0 / F


def band_mean(p, lo, hi):
    c = np.concatenate([[0.0], np.cumsum(p)])
    return (c[hi] - c[lo]) / (hi - lo)


def one_pole_lp(fs, f=F):
    a = 1.0 - np.exp(-2.0 * np.pi * SPLIT_HZ / fs)
    z1 = np.exp(-2j * np.pi * f / fs)
    return a / (1.0 - (1.0 - a) * z1)


LP = {fs: one_pole_lp(fs) for fs in RATES}


# ---------------------------------------------------------------- gains from (frac, sign)
def gains(frac, sign, balance):
    e = sign * 10.0 ** (-DECAY_DB / 20.0 * frac)
    if balance:
        p, n = e[e > 0].sum(), -e[e < 0].sum()
        e = np.where(e > 0, e * (p + n) / (2 * p), e * (p + n) / (2 * n))
    return e / np.sqrt(np.sum(e * e))


def positions(frac, length):
    return np.round(frac * length)


def side_response(lf, lg, sf, sg, t_ms, fs, f=F):
    """Γ(f) of one configuration, from (frac, gain) tables."""
    L = t_ms * fs / 1000.0
    lp = one_pole_lp(fs, f)
    vl = (lg[None, :] * np.exp(-2j * np.pi * f[:, None] * positions(lf, L)[None, :] / fs)).sum(1)
    if sf is None:
        return vl
    vs = (sg[None, :] * np.exp(-2j * np.pi * f[:, None] * positions(sf, L / SHORT_RATIO)[None, :] / fs)).sum(1)
    return lp * vl + (1.0 - lp) * vs


def colour(Gm, g=G):
    """(ERB devs of L and R, third-octave devs, power sum |L|² + |R|² per ERB band re its flat value), dB.

    The power sum is 2|M|²(1 + g²|Γ|²): its flatness is the side level's, seen as the listener hears it
    (§5 "Power sum", ±1 dB). A bound of ±1 dB on |Γ|² itself cannot hold in the lowest ERB bands, where
    a 20 ms sequence has about one degree of freedom per band."""
    out = []
    for s in (1.0, -1.0):
        p = np.abs(1.0 + s * g * Gm) ** 2
        m = p[MEAN_LO:MEAN_HI].mean()
        out.append((10 * np.log10(band_mean(p, ERB_LO, ERB_HI) / m), 10 * np.log10(band_mean(p, TH_LO, TH_HI) / m)))
    erb = np.concatenate([out[0][0], out[1][0]])
    th = np.concatenate([out[0][1], out[1][1]])
    side = 10 * np.log10(band_mean((1.0 + g * g * np.abs(Gm) ** 2) / (1.0 + g * g), ERB_LO, ERB_HI))
    return erb, th, side


def corr(G1, G2):
    w = PINK[CORR]
    a, b = G1[CORR], G2[CORR]
    return np.real(np.sum(w * a * np.conj(b))) / np.sqrt(np.sum(w * np.abs(a) ** 2) * np.sum(w * np.abs(b) ** 2))


def pink_gain_db(Gm):
    """The side's power gain on pink noise 200 Hz – 12 kHz (dB): what the correlation law sees."""
    w = PINK[CORR]
    return 10 * np.log10(np.sum(w * np.abs(Gm[CORR]) ** 2) / np.sum(w))


def cfg_cost(Gm):
    erb, th, side = colour(Gm)
    c = np.max(np.abs(erb)) + 0.5 * np.sqrt(np.mean(erb**2)) + 0.5 * np.max(np.abs(th)) + 3.0 * max(0.0, np.max(np.abs(side)) - 0.8)
    # no cheating by thinning the side where pink noise has its power (the correlation law, §5)
    return c + 10.0 * max(0.0, abs(pink_gain_db(Gm)) - 0.2)


def long_t90(lf, lg):
    """Where 90 % of the long sequence's energy has arrived (fraction of `time`)."""
    o = np.argsort(lf)
    return lf[o][np.searchsorted(np.cumsum(lg[o] ** 2), 0.9)]


# ---------------------------------------------------------------- annealing
class Design:
    """Positions and signs with the per-configuration partial sums kept up to date."""

    def __init__(self, rng, single=False):
        self.single = single
        self.nl = 30 if single else N_LONG
        self.ns = 0 if single else N_SHORT
        self.rng = rng
        self.lf = self._init(self.nl, MIN_LONG)
        # long: half positive, half negative (the balance then only corrects the envelope)
        self.ls = rng.permutation(np.r_[np.ones(self.nl // 2), -np.ones(self.nl - self.nl // 2)])
        self.sf = self._init(self.ns, MIN_SHORT) if self.ns else None
        self.ss = rng.choice([-1.0, 1.0], self.ns) if self.ns else None
        self.rebuild()

    def _init(self, n, lo):
        e = np.linspace(lo, 1.0, n + 1)
        return e[:-1] + self.rng.uniform(0.1, 0.9, n) * (e[1:] - e[:-1])

    def _ph(self, frac, length, fs):
        return np.exp(-2j * np.pi * F[:, None] * positions(np.atleast_1d(frac), length)[None, :] / fs)

    def rebuild(self):
        # per config: Σ env·phase of the long + pulses, of the long − pulses, and Σ sign·env·phase (short)
        self.P, self.N, self.S = [], [], []
        el = 10.0 ** (-DECAY_DB / 20.0 * self.lf)
        for t, fs in CONFIGS:
            L = t * fs / 1000.0
            ph = self._ph(self.lf, L, fs)
            self.P.append((ph * (el * (self.ls > 0))[None, :]).sum(1))
            self.N.append((ph * (el * (self.ls < 0))[None, :]).sum(1))
            if self.ns:
                es = self.ss * 10.0 ** (-DECAY_DB / 20.0 * self.sf)
                self.S.append((self._ph(self.sf, L / SHORT_RATIO, fs) * es[None, :]).sum(1))

    def responses(self):
        el = 10.0 ** (-DECAY_DB / 20.0 * self.lf)
        p, n = el[self.ls > 0].sum(), el[self.ls < 0].sum()
        kp, kn = (p + n) / (2 * p), (p + n) / (2 * n)
        nl = np.sqrt(np.sum((el * np.where(self.ls > 0, kp, kn)) ** 2))
        if self.ns:
            es = 10.0 ** (-DECAY_DB / 20.0 * self.sf)
            ns = np.sqrt(np.sum(es**2))
        out = []
        for c, (t, fs) in enumerate(CONFIGS):
            vl = (kp * self.P[c] - kn * self.N[c]) / nl
            if self.ns:
                lp = LP[fs]
                out.append(lp * vl + (1.0 - lp) * self.S[c] / ns)
            else:
                out.append(vl)
        return out

    def tables(self):
        lg = gains(self.lf, self.ls, True)
        if self.ns:
            return self.lf.copy(), lg, self.sf.copy(), gains(self.sf, self.ss, False)
        return self.lf.copy(), lg, None, None

    # one move, returns an undo closure
    def move(self):
        rng = self.rng
        long = rng.random() < self.nl / (self.nl + self.ns)
        frac = self.lf if long else self.sf
        sign = self.ls if long else self.ss
        lo = MIN_LONG if long else MIN_SHORT
        k = rng.integers(len(frac))
        old_f, old_s = frac.copy(), sign.copy()
        if rng.random() < 0.25:
            if long:
                j = rng.choice(np.flatnonzero(sign != sign[k]))
                sign[k], sign[j] = sign[j], sign[k]
            else:
                sign[k] = -sign[k]
        else:
            f = np.clip(frac[k] + rng.normal(0.0, 1.0 / 12.0), lo, 1.0)
            others = np.delete(frac, k)
            if np.min(np.abs(others - f)) < MIN_GAP:
                return None
            frac[k] = f
        snap = (self.P, self.N, self.S)
        self.P, self.N, self.S = list(self.P), list(self.N), list(self.S)
        if (frac != old_f).any():
            self._shift(long, k, old_f[k], frac[k], sign[k])
        else:
            for i in np.flatnonzero(sign != old_s):
                self._flip(long, i, frac[i], sign[i])

        def undo():
            frac[:] = old_f
            sign[:] = old_s
            self.P, self.N, self.S = snap

        return undo

    def _shift(self, long, k, f0, f1, s):
        """One pulse moved: its old term out, its new term in, per configuration."""
        e0, e1 = 10.0 ** (-DECAY_DB / 20.0 * f0), 10.0 ** (-DECAY_DB / 20.0 * f1)
        for c, (t, fs) in enumerate(CONFIGS):
            L = t * fs / 1000.0 / (1.0 if long else SHORT_RATIO)
            p0, p1 = np.round(f0 * L), np.round(f1 * L)
            d = e1 * np.exp(-2j * np.pi * F * p1 / fs) - e0 * np.exp(-2j * np.pi * F * p0 / fs)
            if not long:
                self.S[c] = self.S[c] + s * d
            elif s > 0:
                self.P[c] = self.P[c] + d
            else:
                self.N[c] = self.N[c] + d

    def _flip(self, long, k, f, s):
        """Pulse k's sign became s: its term changes side (long) or polarity (short)."""
        e = 10.0 ** (-DECAY_DB / 20.0 * f)
        for c, (t, fs) in enumerate(CONFIGS):
            L = t * fs / 1000.0 / (1.0 if long else SHORT_RATIO)
            term = e * np.exp(-2j * np.pi * F * np.round(f * L) / fs)
            if not long:
                self.S[c] = self.S[c] + 2.0 * s * term
            elif s > 0:
                self.P[c] = self.P[c] + term
                self.N[c] = self.N[c] - term
            else:
                self.N[c] = self.N[c] + term
                self.P[c] = self.P[c] - term

    def _update(self, long):
        # full recompute of the touched sequence's sums (sign moves)
        if long:
            el = 10.0 ** (-DECAY_DB / 20.0 * self.lf)
            for c, (t, fs) in enumerate(CONFIGS):
                ph = self._ph(self.lf, t * fs / 1000.0, fs)
                self.P[c] = (ph * (el * (self.ls > 0))[None, :]).sum(1)
                self.N[c] = (ph * (el * (self.ls < 0))[None, :]).sum(1)
        else:
            es = self.ss * 10.0 ** (-DECAY_DB / 20.0 * self.sf)
            for c, (t, fs) in enumerate(CONFIGS):
                self.S[c] = (self._ph(self.sf, t * fs / 1000.0 / SHORT_RATIO, fs) * es[None, :]).sum(1)


def total_cost(resp, previous, d):
    cs = np.array([cfg_cost(Gm) for Gm in resp])
    c = cs.max() + 0.3 * cs.mean()
    # smear of the lows: §5 wants 90 % of the side below 500 Hz within `time` (filter ringing included)
    lf, lg, _, _ = d.tables()
    c += 20.0 * max(0.0, long_t90(lf, lg) - 0.82)
    if previous:
        worst = max(abs(corr(a, b)) for pr in previous for a, b in zip(resp, pr))
        c += 5.0 * max(0.0, worst - 0.15)
    return c


def anneal(args):
    seed, iters, previous, single = args
    rng = np.random.default_rng(seed)
    d = Design(rng, single)
    c = total_cost(d.responses(), previous, d)
    best = (c, d.tables())
    for it in range(iters):
        undo = d.move()
        if undo is None:
            continue
        c2 = total_cost(d.responses(), previous, d)
        T = 0.3 * (1.0 - it / iters) + 1e-3
        if c2 < c or rng.random() < np.exp((c - c2) / T):
            c = c2
            if c < best[0]:
                best = (c, d.tables())
        else:
            undo()
    return best


# ---------------------------------------------------------------- reports
def impulse(lf, lg, sf, sg, t_ms, fs):
    """One channel's side impulse response, sample by sample as the Rust code runs it."""
    L = t_ms * fs / 1000.0
    n = int(L) + 64
    a = 1.0 - np.exp(-2.0 * np.pi * SPLIT_HZ / fs)
    x = np.zeros(n)
    x[0] = 1.0
    lp = np.zeros(n)
    y = 0.0
    for i in range(n):
        y += a * (x[i] - y)
        lp[i] = y
    hp = x - lp
    v = np.zeros(n)
    for f, g in zip(positions(lf, L).astype(int), lg):
        v[f:] += g * lp[: n - f]
    if sf is not None:
        for f, g in zip(positions(sf, L / SHORT_RATIO).astype(int), sg):
            v[f:] += g * hp[: n - f]
    return v


def regularity(v, fs, g=G):
    h = np.r_[1.0, np.zeros(len(v) - 1)] + g * v
    nfft = 1 << int(np.ceil(np.log2(2 * len(h))))
    H = np.fft.rfft(h, nfft)
    r = np.fft.irfft(np.abs(H) ** 2, nfft)[: len(h)]
    lo, hi = int(0.0005 * fs), min(int(0.040 * fs), len(h) - 1)
    return np.max(np.abs(r[lo:hi])) / r[0]


def t90(v, fs, band):
    from scipy.signal import butter, sosfilt

    x = np.r_[v, np.zeros(int(0.05 * fs))]
    if band == "high":
        y = sosfilt(butter(4, 2000.0, "high", fs=fs, output="sos"), x)
    else:
        y = sosfilt(butter(4, 500.0, "low", fs=fs, output="sos"), x)
    e = np.cumsum(y**2)
    return np.searchsorted(e, 0.9 * e[-1]) / fs * 1000.0


def report(tables, times=TIMES, rates=RATES, out=sys.stdout):
    lines = []
    worst = dict(third=(0.0, 0.0), erb=(0.0, 0.0), side=(0.0, 0.0))
    for k, (lf, lg, sf, sg) in enumerate(tables):
        th_lo = th_hi = erb_lo = erb_hi = 0.0
        regs, hi90, lo90, pg = [], [], [], []
        for t in times:
            for fs in rates:
                Gm = side_response(lf, lg, sf, sg, t, fs)
                erb, th, side = colour(Gm)
                pg.append(pink_gain_db(Gm))
                th_lo, th_hi = min(th_lo, th.min()), max(th_hi, th.max())
                erb_lo, erb_hi = min(erb_lo, erb.min()), max(erb_hi, erb.max())
                worst["side"] = (min(worst["side"][0], side.min()), max(worst["side"][1], side.max()))
                v = impulse(lf, lg, sf, sg, t, fs)
                regs.append(regularity(v, fs))
                if t == min(times):
                    hi90.append(t90(v, fs, "high"))
                    lo90.append(t90(v, fs, "low"))
        worst["third"] = (min(worst["third"][0], th_lo), max(worst["third"][1], th_hi))
        worst["erb"] = (min(worst["erb"][0], erb_lo), max(worst["erb"][1], erb_hi))
        dc = lg.sum()
        lines.append(
            f"variant {k + 1}: one side 1/3 oct [{th_lo:+.2f}, {th_hi:+.2f}] dB, ERB [{erb_lo:+.2f}, {erb_hi:+.2f}] dB, "
            f"regularity ≤ {max(regs):.3f}, t90 at {min(times):g} ms: > 2 kHz {max(hi90):.1f} ms, < 500 Hz {max(lo90):.1f} ms, "
            f"pink gain [{min(pg):+.2f}, {max(pg):+.2f}] dB, Σg_long {dc:+.1e}"
        )
    if len(tables) > 1:
        cmax = 0.0
        for t in times:
            for fs in rates:
                R = [side_response(*tb, t, fs) for tb in tables]
                for i in range(len(R)):
                    for j in range(i):
                        cmax = max(cmax, abs(corr(R[i], R[j])))
        lines.append(f"variants pairwise |r| (pink 200 Hz – 12 kHz) ≤ {cmax:.3f}")
    lines.append(
        f"all: one side 1/3 oct [{worst['third'][0]:+.2f}, {worst['third'][1]:+.2f}] dB, "
        f"ERB [{worst['erb'][0]:+.2f}, {worst['erb'][1]:+.2f}] dB, power sum ERB [{worst['side'][0]:+.2f}, {worst['side'][1]:+.2f}] dB"
    )
    for ln in lines:
        print(ln, file=out)
    return lines


# ---------------------------------------------------------------- tables as Rust
def write_rust(tables, summary, path=OUT):
    def arr(frac, gain):
        return "[" + ", ".join(f"({f:.6}, {g:.9})" for f, g in zip(frac, gain)) + "]"

    s = [
        "// SPDX-License-Identifier: GPL-3.0-or-later",
        "//! Velvet sequences of the two-band decorrelator: GENERATED by `tools/vnd-design.py`, do not edit.",
        "//!",
        "//! Each pulse is (position as a fraction of the sequence's length, gain). V_long spans `time`,",
        "//! V_short spans `time` / 4. Gains: 6 dB envelope × sign, the long sequence balanced (Σ g = 0),",
        "//! Σ g² = 1 per sequence. Design report (g = 0.6, `time` 20 / 30 / 40 ms, 44.1 / 48 / 96 kHz):",
        "//!",
    ]
    s += [f"//! {ln}" for ln in summary]
    s += [
        "",
        "#![allow(clippy::approx_constant)]",
        "",
        f"pub const N_LONG: usize = {N_LONG};",
        f"pub const N_SHORT: usize = {N_SHORT};",
        f"pub const N_VARIANTS: usize = {N_VARIANTS};",
        "",
        "#[rustfmt::skip]",
        "pub static LONG: [[(f64, f64); N_LONG]; N_VARIANTS] = [",
    ]
    s += [f"    {arr(lf, lg)}," for lf, lg, _, _ in tables]
    s += ["];", "", "#[rustfmt::skip]", "pub static SHORT: [[(f64, f64); N_SHORT]; N_VARIANTS] = ["]
    s += [f"    {arr(sf, sg)}," for _, _, sf, sg in tables]
    s += ["];", ""]
    with open(path, "w") as f:
        f.write("\n".join(s))


def read_rust(path=OUT):
    txt = open(path).read()

    def block(name):
        b = txt.split(f"pub static {name}")[1].split("\n];")[0]
        rows = []
        for line in b.splitlines()[1:]:
            pairs = re.findall(r"\(([-0-9.e]+), ([-0-9.e]+)\)", line)
            if pairs:
                rows.append((np.array([float(p[0]) for p in pairs]), np.array([float(p[1]) for p in pairs])))
        return rows

    return [(lf, lg, sf, sg) for (lf, lg), (sf, sg) in zip(block("LONG"), block("SHORT"))]


AP_OUT = os.path.join(HERE, "..", "src", "ap_tables.rs")
AP_SECTIONS = 6


def allpass_response(fc, q, fs, f=F):
    """The cascade of second-order all-passes (TPT SVF x − 2k·BP = RBJ all-pass, bilinear prewarped)."""
    z1 = np.exp(-2j * np.pi * f / fs)
    h = np.ones_like(f, dtype=complex)
    for c, qq in zip(fc, q):
        w = 2 * np.pi * c / fs
        al = np.sin(w) / (2 * qq)
        b = [1 - al, -2 * np.cos(w), 1 + al]
        a = [1 + al, -2 * np.cos(w), 1 - al]
        h *= (b[0] + b[1] * z1 + b[2] * z1**2) / (a[0] + a[1] * z1 + a[2] * z1**2)
    return h


def design_allpass(seed=7, candidates=200, jitter=0.5):
    """README §2.2: 6 sections, centres log-spaced 300 Hz – 10 kHz with a seeded jitter, Q 0.5 – 0.9.

    §5 asks the 8 sets' sides to correlate ≤ 0.2 pairwise. That cannot hold for this generator: six
    broad sections spread over five octaves give every set nearly the same phase curve (random sets
    with the prototype's ±1/4-octave jitter correlate 0.57 – 0.98, median 0.86; the best 8 of 200
    such draws still reach 0.82). The jitter is widened to ±1/2 octave (sections sorted), which
    leaves the one-channel colour where it was (−7 … +3 dB third-octave), and the 8 sets are chosen
    among `candidates` draws to minimise the worst pairwise correlation (greedy from every start):
    0.62, reported, not bounded."""
    rng = np.random.default_rng(seed)
    cands = []
    for _ in range(candidates):
        fc = np.sort(np.geomspace(300.0, 10000.0, AP_SECTIONS) * 2.0 ** rng.uniform(-jitter, jitter, AP_SECTIONS))
        cands.append((fc, rng.uniform(0.5, 0.9, AP_SECTIONS)))
    resp = [allpass_response(fc, q, 48000.0) for fc, q in cands]
    n = len(cands)
    R = np.array([[abs(corr(resp[i], resp[j])) if i != j else 0.0 for j in range(n)] for i in range(n)])
    best = None
    for s0 in range(n):
        ch = [s0]
        while len(ch) < N_VARIANTS:
            ch.append(min((k for k in range(n) if k not in ch), key=lambda k: R[k, ch].max()))
        m = max(R[i, j] for i in ch for j in ch if i != j)
        if best is None or m < best[0]:
            best = (m, ch)
    sets = [cands[k] for k in best[1]]
    draws = candidates
    lines = [f"{N_VARIANTS} sets chosen among {draws} draws (jitter ±{jitter:g} octave)"]
    for k, (fc, q) in enumerate(sets):
        erb, th, _ = colour(allpass_response(fc, q, 48000.0))
        lines.append(f"set {k + 1}: one side 1/3 oct [{th.min():+.2f}, {th.max():+.2f}] dB, ERB [{erb.min():+.2f}, {erb.max():+.2f}] dB")
    cmax = max(abs(corr(allpass_response(*sets[i], fs), allpass_response(*sets[j], fs))) for i in range(len(sets)) for j in range(i) for fs in RATES)
    lines.append(f"sets pairwise |r| (pink 200 Hz – 12 kHz) ≤ {cmax:.3f}")
    for ln in lines:
        print(ln)
    s = [
        "// SPDX-License-Identifier: GPL-3.0-or-later",
        "//! All-pass sets of the Allpass generator: GENERATED by `tools/vnd-design.py --allpass`, do not edit.",
        "//!",
        "//! Each set: 6 × (centre Hz, Q) of second-order all-passes (README §2.2). Report (g = 0.6, 48 kHz):",
        "//!",
    ]
    s += [f"//! {ln}" for ln in lines]
    s += ["", "#![allow(clippy::approx_constant)]", "", f"pub const N_SECTIONS: usize = {AP_SECTIONS};", "", "#[rustfmt::skip]", f"pub static SETS: [[(f32, f32); N_SECTIONS]; {N_VARIANTS}] = ["]
    s += ["    [" + ", ".join(f"({c:.2f}, {qq:.4f})" for c, qq in zip(fc, q)) + "]," for fc, q in sets]
    s += ["];", ""]
    with open(AP_OUT, "w") as f:
        f.write("\n".join(s))
    print(f"written {os.path.relpath(AP_OUT)}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--allpass", action="store_true", help="design the Allpass generator's 8 sets")
    ap.add_argument("--iters", type=int, default=15000)
    ap.add_argument("--restarts", type=int, default=8)
    ap.add_argument("--check", action="store_true", help="report the committed tables")
    ap.add_argument("--rewrite", action="store_true", help="rewrite the committed tables with a fresh report")
    ap.add_argument("--single", action="store_true", help="design the single-band reference (not shipped)")
    a = ap.parse_args()
    if a.allpass:
        design_allpass()
        return
    if a.rewrite:
        tables = read_rust()
        write_rust(tables, report(tables))
        return
    if a.check:
        tables = read_rust()
        report(tables)
        print("--- shorter times (reported, not bounded)")
        for t in (10.0, 15.0):
            print(f"time {t:g} ms:")
            report(tables, times=(t,))
        return
    if a.single:
        with Pool(min(a.restarts, os.cpu_count() or 1)) as pool:
            res = pool.map(anneal, [(100 + r, a.iters, [], True) for r in range(a.restarts)])
        c, tb = min(res, key=lambda r: r[0])
        print("single band, 30 pulses over `time`:")
        report([tb])
        return
    tables, chosen = [], []
    for k in range(N_VARIANTS):
        previous = [[side_response(*tb, t, fs) for t, fs in CONFIGS] for tb in tables]
        with Pool(min(a.restarts, os.cpu_count() or 1)) as pool:
            res = pool.map(anneal, [(1000 * (k + 1) + r, a.iters, previous, False) for r in range(a.restarts)])
        c, tb = min(res, key=lambda r: r[0])
        tables.append(tb)
        print(f"variant {k + 1}: cost {c:.3f}", flush=True)
    summary = report(tables)
    write_rust(tables, summary)
    print(f"written {os.path.relpath(OUT)}")


if __name__ == "__main__":
    main()
