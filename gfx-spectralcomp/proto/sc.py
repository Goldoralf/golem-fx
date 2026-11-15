# SPDX-License-Identifier: GPL-3.0-or-later
"""gfx-spectralcomp prototype engine (README §2). Offline, numpy; written for measurement, not speed.

One detector, two appliers:
- detector: periodic Hann STFT, hop N/4; calibrated power per bin P_k = c |X_k|^2, c = 2 / (N sum w^2);
  constant-Q band sum E_k over +-res/2 octaves (at least +-2 bins) by prefix sums; L_k = 10 log10(E_k / 0.5)
  (0 dB = a full-scale sine); dB-domain attack / release per hop.
- static curve per bin: T_k = thresh + slope log2(f_k / center); downward and upward with a quadratic knee,
  `range`, `lift_range`, noise `guard` (a boost never exceeds L - guard).
- Studio: per-bin gains in the same STFT (WOLA, Hann / Hann, / 1.5); latency N - 1 samples.
- Live: the gain curve averaged on third-octave points, realised by a zero-latency ACGE cascade of matched
  bells (gfx-geq's solver), one warm-started correction per 1.33 ms tick.
"""
import numpy as np
from scipy.signal import lfilter
from acge import GEQ, design, q_law

THIRDS = [40, 50, 63, 80, 100, 125, 160, 200, 250, 315, 400, 500, 630, 800, 1000, 1250, 1600, 2000, 2500,
          3150, 4000, 5000, 6300, 8000, 10000, 12500, 16000]

DEF = dict(thresh=-36.0, down=4.0, up=1.0, center=420.0, slope=0.0, attack=150.0, release=300.0,
           window=2048, mode='live', knee=6.0, res=2.0, guard=-72.0, range=24.0, lift_range=12.0,
           auto_gain=True, mix=100.0, out=0.0, detect='self', hop_div=8, live_hop_div=4, tick_ms=4 / 3)


def hann(N):
    n = np.arange(N)
    return 0.5 - 0.5 * np.cos(2 * np.pi * n / N)


def n_for(window, fs):
    """`window` is given in samples at 48 kHz; the size follows the rate family."""
    fam = 1 if fs < 60000 else (2 if fs < 120000 else 4)
    return int(window) * fam


def band_edges(N, fs, res_st, hmin=2.0):
    nb = N // 2 + 1
    k = np.arange(nb, dtype=float)
    r = res_st / 12.0
    up = np.maximum(hmin, k * (2 ** (r / 2) - 1))
    dn = np.maximum(hmin, k * (1 - 2 ** (-r / 2)))
    return k - dn, k + up


def band_sum(P, lo, hi):
    """Sum of P over [lo, hi] in bins, bin i occupying [i - .5, i + .5); fractional edges weighted."""
    nb = P.shape[-1]
    S = np.concatenate([np.zeros(P.shape[:-1] + (1,)), np.cumsum(P, axis=-1)], axis=-1)

    def C(y):
        x = np.clip(y + 0.5, 0, nb)
        i = np.minimum(np.floor(x).astype(int), nb - 1)
        return S[..., i] + (x - i) * P[..., i]
    return C(hi) - C(lo)


def soft(x, W):
    """0 below -W/2, x above W/2, the quadratic knee between (Giannoulis et al.)."""
    if W <= 0:
        return np.maximum(x, 0.0)
    return np.where(x <= -W / 2, 0.0, np.where(x >= W / 2, x, (x + W / 2) ** 2 / (2 * W)))


def static_gain(L, T, p, f=None):
    over = L - T
    gd = -(1 - 1 / p['down']) * soft(over, p['knee'])
    gd = np.maximum(gd, -p['range'])
    gu = (1 - 1 / p['up']) * soft(-over, p['knee'])
    gu = np.minimum(gu, np.maximum(0.0, L - p['guard']))
    gu = np.minimum(gu, p['lift_range'])
    if f is not None:
        gu = np.where(f < 20.0, 0.0, gu)
    return gd + gu


def thresholds(f, p):
    lf = np.log2(np.maximum(f, 1.0) / p['center'])
    return p['thresh'] + p['slope'] * lf


def auto_makeup(p, N, fs):
    """Static make-up: pink noise at -18 dBFS RMS (20 Hz - 20 kHz) is level-matched to bypass."""
    nb = N // 2 + 1
    k = np.arange(nb)
    f = k * fs / N
    lo, hi = band_edges(N, fs, p['res'])
    flo = np.clip((lo) * fs / N, 20, 20000)
    fhi = np.clip((hi) * fs / N, 20, 20000)
    tot = 10 ** (-18 / 10)
    E = tot * np.log(np.maximum(fhi / flo, 1)) / np.log(1000)
    L = 10 * np.log10(np.maximum(E / 0.5, 1e-14))
    g = static_gain(L, thresholds(f, p), p, f)
    fk = np.clip(f, 20, 20000)
    wpink = np.where((f >= 20) & (f <= 20000), 1 / fk, 0.0)
    G = np.sum(wpink * 10 ** (g / 10)) / np.sum(wpink)
    return -10 * np.log10(G)


class Detector:
    """Per-hop gain curves in dB from a signal (the main or the key)."""

    def __init__(s, p, fs):
        s.p = p
        s.fs = fs
        s.N = n_for(p['window'], fs)
        s.H = s.N // p['hop_div']
        s.w = hann(s.N)
        s.c = 2.0 / (s.N * np.sum(s.w ** 2))
        s.f = np.arange(s.N // 2 + 1) * fs / s.N
        s.lo, s.hi = band_edges(s.N, fs, p['res'], p.get('hmin', 2.0))
        s.T = thresholds(s.f, p)

    def frames(s, x):
        """Frames ending at e_m = mH + H - 1, zero history before 0. Returns (ends, spectra X)."""
        N, H = s.N, s.H
        xp = np.concatenate([np.zeros(N - H), x])
        M = len(x) // H
        idx = np.arange(N)[None, :] + (np.arange(M) * H)[:, None]
        F = xp[idx] * s.w
        return np.arange(M) * H + H - 1, np.fft.rfft(F, axis=1)

    def levels(s, X):
        P = s.c * np.abs(X) ** 2
        E = band_sum(P, s.lo, s.hi)
        return 10 * np.log10(np.maximum(E / 0.5, 1e-14))

    def smooth(s, L):
        p, fs, H = s.p, s.fs, s.H
        aa = 1 - np.exp(-H / (p['attack'] * 1e-3 * fs))
        ar = 1 - np.exp(-H / (p['release'] * 1e-3 * fs))
        Ls = np.empty_like(L)
        st = L[0].copy()   # first frame: instant
        for m in range(L.shape[0]):
            d = L[m] - st
            st = st + np.where(d > 0, aa, ar) * d
            Ls[m] = st
        return Ls

    def gains(s, x):
        ends, X = s.frames(x)
        Ls = s.smooth(s.levels(X))
        return ends, X, Ls, static_gain(Ls, s.T, s.p, s.f)


def studio(x, p, fs, key=None, return_all=False):
    d = Detector(p, fs)
    ends, X, Ls, g = d.gains(x if key is None else key)
    if key is not None:
        X = d.frames(x)[1]
    Y = X * 10 ** (g / 20)
    y = np.fft.irfft(Y, n=d.N, axis=1) * d.w / (1.5 * p['hop_div'] / 4)
    N, H = d.N, d.H
    M = len(ends)
    ola = np.zeros(M * H + N)
    for m in range(M):
        ola[m * H: m * H + N] += y[m]
    # frame m covers absolute samples [mH - (N - H), mH + H); ola index i <-> absolute i - (N - H)
    D = N - 1
    out_abs = ola[(N - H):(N - H) + len(x)]          # aligned (zero-delay) reconstruction
    wet = np.concatenate([np.zeros(D), out_abs])[:len(x)]
    dry = np.concatenate([np.zeros(D), x])[:len(x)]
    mk = auto_makeup(p, N, fs) if p['auto_gain'] else 0.0
    m_ = p['mix'] / 100
    yout = ((1 - m_) * dry + m_ * wet * 10 ** (mk / 20)) * 10 ** (p['out'] / 20)
    if return_all:
        return yout, dict(D=D, g=g, Ls=Ls, ends=ends, det=d, makeup=mk, wet=wet, dry=dry)
    return yout


class LiveEQ:
    def __init__(s, fs, c=0.4, wts=(1, .5, .25)):
        s.fc = [f for f in THIRDS if f < 0.45 * fs]
        s.geq = GEQ(s.fc, fs, c=c, wts=wts, clamp=40.0)
        s.fs = fs
        s.box = 0.0   # 0: point sampling (the detector is already a third of an octave wide)

    def targets_from_bins(s, gk, f):
        """dB mean of the bin gains over +-1/6 octave around each design point (interp if no bin)."""
        fp = s.geq.fp
        lf = np.log2(np.maximum(f[1:], 1e-9))
        g1 = gk[..., 1:]
        out = np.empty(gk.shape[:-1] + (len(fp),))
        for i, fq in enumerate(fp):
            m = np.abs(lf - np.log2(fq)) <= s.box
            if s.box > 0 and m.sum() >= 1:
                out[..., i] = g1[..., m].mean(axis=-1)
            else:
                out[..., i] = np.array([np.interp(np.log2(fq), lf, row) for row in g1.reshape(-1, g1.shape[-1])]).reshape(gk.shape[:-1])
        return out


def live(x, p, fs, key=None, return_all=False, corrections=1):
    p = dict(p, res=max(p['res'], p.get('live_res', 6.0)), hop_div=p.get('live_hop_div', 4))   # Live: resolution at least half an octave
    d = Detector(p, fs)
    ends, X, Ls, g = d.gains(x if key is None else key)
    eq = LiveEQ(fs)
    Tm = eq.targets_from_bins(g, d.f)          # (frames, points)
    H = d.H
    tick = int(round(p['tick_ms'] * 1e-3 * fs))
    n = len(x)
    # target at time t: ramp from T_{m-1} to T_m over [e_m + 1, e_m + 1 + H)
    tgt_t = np.zeros((n, Tm.shape[1]))
    nT = Tm.shape[1]
    prev = np.zeros(nT)
    starts = ends + 1
    # piecewise linear per point, evaluated only at tick ends
    tick_ends = np.arange(tick - 1, n, tick)
    tt = np.zeros((len(tick_ends), nT))
    for j, te in enumerate(tick_ends):
        m = np.searchsorted(starts, te, side='right') - 1
        if m < 0:
            tt[j] = 0
            continue
        a = min(1.0, (te - starts[m] + 1) / H)
        tm1 = Tm[m - 1] if m >= 1 else np.zeros(nT)
        tt[j] = tm1 + a * (Tm[m] - tm1)
    G = eq.geq
    gsec = np.zeros(len(eq.fc))
    zi = [np.zeros(2) for _ in eq.fc]
    y = x.astype(float).copy()
    out = np.zeros(n)
    err = []
    flat = True
    for j, te in enumerate(tick_ends):
        t0 = te - tick + 1
        seg = y[t0:te + 1]
        t = tt[j]
        if np.max(np.abs(t)) < 1e-6 and np.max(np.abs(gsec)) < 1e-6:
            out[t0:te + 1] = seg      # whole cascade bypassed when flat (gfx-geq)
            continue
        for _ in range(corrections):
            dg = G.P @ (t - G.resp(gsec, G.fp))
            gsec = np.clip(gsec + dg, -40, 40)
        des = G.designs(gsec)
        v = seg
        for i, B in enumerate(des):
            b0, b1, b2, a1, a2 = B
            v, zi[i] = lfilter([b0, b1, b2], [1, a1, a2], v, zi=zi[i])
        out[t0:te + 1] = v
        if return_all and j % 8 == 0:
            isc = np.isin(G.fp, G.fc)
            err.append(np.max(np.abs(G.resp(gsec, G.fp)[isc] - t[isc])))
    tail = n - (tick_ends[-1] + 1)
    if tail > 0:
        out[-tail:] = y[-tail:]
    mk = auto_makeup(p, d.N, fs) if p['auto_gain'] else 0.0
    m_ = p['mix'] / 100
    yout = ((1 - m_) * x + m_ * out * 10 ** (mk / 20)) * 10 ** (p['out'] / 20)
    if return_all:
        return yout, dict(D=0, g=g, Ls=Ls, ends=ends, det=d, makeup=mk, wet=out, Tm=Tm, eq=eq, err=err)
    return yout


def run(x, fs, key=None, return_all=False, **kw):
    p = dict(DEF)
    p.update(kw)
    f = studio if p['mode'] == 'studio' else live
    return f(x, p, fs, key=key, return_all=return_all)


# ---------- test signals (synthetic, no recordings) ----------
def pluck(f0, dur, fs, seed=0, bright=0.5):
    """Karplus-Strong string, a guitar-like decaying harmonic tone."""
    rng = np.random.default_rng(seed)
    n = int(dur * fs)
    L = int(round(fs / f0))
    buf = rng.uniform(-1, 1, L)
    buf = buf - buf.mean()
    y = np.zeros(n)
    a = 0.996 ** (440 / f0) if f0 < 440 else 0.996
    prev = 0.0
    for i in range(n):
        v = buf[i % L]
        y[i] = v
        nv = a * (bright * v + (1 - bright) * 0.5 * (v + buf[(i + 1) % L]))
        buf[i % L] = nv
    return y


def guitar_phrase(fs, seed=1):
    """A strummed E major, then single notes (82 - 659 Hz), peak about -10 dBFS."""
    rng = np.random.default_rng(seed)
    dur = 6.0
    y = np.zeros(int(dur * fs))
    chord = [82.41, 123.47, 164.81, 207.65, 246.94, 329.63]
    for i, f0 in enumerate(chord):
        t0 = int((0.05 + 0.012 * i) * fs)
        s = pluck(f0, 2.5, fs, seed=seed + i)
        y[t0:t0 + len(s)] += s[:len(y) - t0] * 0.5
    notes = [329.63, 392.0, 440.0, 493.88, 587.33, 659.26, 493.88, 329.63]
    for j, f0 in enumerate(notes):
        t0 = int((2.6 + 0.4 * j) * fs)
        s = pluck(f0, 1.2, fs, seed=seed + 20 + j) * (0.9 if j % 2 else 0.6)
        y[t0:t0 + len(s)] += s[:len(y) - t0]
    return y / np.max(np.abs(y)) * 10 ** (-10 / 20)
