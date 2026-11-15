# SPDX-License-Identifier: GPL-3.0-or-later
"""Shared pieces of the gfx-morph prototypes (numpy / scipy, f64).

Everything here is written from formulas (README §2), not from any third-party code:
- HIIR-style polyphase half-band (the coefficient design of gfx-dsp `oversample`, re-typed);
- ADAA1 shapers (gfx-dsp `adaa` laws) and the extra curves of README §2.4;
- Linkwitz-Riley 4 crossovers with all-pass compensation (gfx-dsp `crossover` topology);
- a synthetic sung voice (a band-limited glottal-derivative source, cosine-phase harmonics at 1 / k, through
  four formant resonators; `glottal()` is an unused time-domain alternative that aliases), sibilants,
  stage bleed (pink noise).
"""
import numpy as np
from scipy import signal

FS = 48000.0
RNG = np.random.default_rng(1234)


def db(x):
    return 20 * np.log10(np.maximum(np.abs(x), 1e-30))


def rms(x):
    return np.sqrt(np.mean(np.asarray(x, dtype=float) ** 2) + 1e-300)


def rms_db(x):
    return db(rms(x))


# ---------------------------------------------------------------- half-band (HIIR design) -------------

def _tp(transition):
    k = np.tan((1 - transition * 2) * np.pi / 4) ** 2
    kk = (1 - k * k) ** 0.25
    e = 0.5 * (1 - kk) / (1 + kk)
    e2 = e * e
    e4 = e2 * e2
    q = e * (1 + e4 * (2 + e4 * (15 + 150 * e4)))
    return k, q


def halfband_coefs(n=8, transition=0.04):
    k, q = _tp(transition)
    order = 2 * n + 1
    out = []
    for i in range(n):
        c = i + 1
        num, j, ii = 0.0, 1.0, 0
        while True:
            t = q ** (ii * (ii + 1)) * np.sin((2 * ii + 1) * c * np.pi / order) * j
            num += t
            j = -j
            ii += 1
            if abs(t) <= 1e-100 or ii > 200:
                break
        den, j, ii = 0.0, -1.0, 1
        while True:
            t = q ** (ii * ii) * np.cos(2 * ii * c * np.pi / order) * j
            den += t
            j = -j
            ii += 1
            if abs(t) <= 1e-100 or ii > 200:
                break
        ww = num * q ** 0.25 / (den + 0.5)
        w2 = ww * ww
        x = np.sqrt((1 - w2 * k) * (1 - w2 / k)) / (1 + w2)
        out.append((1 - x) / (1 + x))
    return np.array(out)


HB = halfband_coefs()


def _chain(x, coefs):
    for c in coefs:
        x = signal.lfilter([c, 1.0], [1.0, c], x)
    return x


def up2(x):
    a = _chain(x, HB[0::2])
    b = _chain(x, HB[1::2])
    y = np.empty(2 * len(x))
    y[0::2] = a
    y[1::2] = b
    return y


def down2(y):
    a = _chain(y[1::2], HB[0::2])
    b = _chain(y[0::2], HB[1::2])
    return 0.5 * (a + b)


def up(x, factor):
    while factor > 1:
        x = up2(x)
        factor //= 2
    return x


def down(y, factor):
    while factor > 1:
        y = down2(y)
        factor //= 2
    return y


# ---------------------------------------------------------------- shapers (ADAA1) ------------------------

def ln_cosh(x):
    a = np.abs(x)
    return a + np.log1p(np.exp(-2 * a)) - np.log(2)


def tri(x):  # triangle fold, period 4, |y| <= 1
    return np.abs(((x - 1) % 4) - 2) - 1


def tri_F(x):  # antiderivative of tri: periodic (the integral over one period is 0), continuous
    u = (x - 1) % 4  # tri = 1 - u on [0, 2), u - 3 on [2, 4)
    return np.where(u < 2, u - u * u / 2, u * u / 2 - 3 * u + 4)


SHAPERS = {
    # name: (f, F)
    "soft": (np.tanh, ln_cosh),
    "hard": (lambda x: np.clip(x, -1, 1),
             lambda x: np.where(np.abs(x) <= 1, 0.5 * x * x, np.abs(x) - 0.5)),
    "tube": (lambda x: np.tanh(x + 0.3) - np.tanh(0.3),
             lambda x: ln_cosh(x + 0.3) - x * np.tanh(0.3)),
    "fold": (tri, tri_F),
    "rectify": (np.abs, lambda x: x * np.abs(x) / 2),
}


def adaa1(x, name):
    f, F = SHAPERS[name]
    xp = np.concatenate([[0.0], x[:-1]])
    d = x - xp
    Fx = F(x)
    Fp = np.concatenate([[F(np.array([0.0]))[0]], Fx[:-1]])
    small = np.abs(d) < 1e-7
    out = np.where(small, f(0.5 * (x + xp)), (Fx - Fp) / np.where(small, 1.0, d))
    return out


def naive(x, name):
    return SHAPERS[name][0](x)


def shape(x, name, drive_db, factor=2, use_adaa=True):
    """Drive (dB) into a shaper inside an up / down pair. Returns the base-rate output."""
    g = 10 ** (drive_db / 20)
    u = up(x, factor) * g
    y = adaa1(u, name) if use_adaa else naive(u, name)
    return down(y, factor)


# ---------------------------------------------------------------- filters ---------------------------------

def bw2(fc, kind, fs=FS):
    return signal.butter(2, fc, btype=kind, fs=fs)


def lr4(x, fc, fs=FS):
    bl, al = bw2(fc, "low", fs)
    bh, ah = bw2(fc, "high", fs)
    lo = signal.lfilter(bl, al, signal.lfilter(bl, al, x))
    hi = signal.lfilter(bh, ah, signal.lfilter(bh, ah, x))
    return lo, hi


def ap_twin(x, fc, fs=FS):
    lo, hi = lr4(x, fc, fs)
    return lo + hi


def split3(x, f1, f2, fs=FS):
    lo, rest = lr4(x, f1, fs)
    lo = ap_twin(lo, f2, fs)
    mid, hi = lr4(rest, f2, fs)
    return lo, mid, hi


def rbj_peak(f0, gain_db, q, fs=FS):
    A = 10 ** (gain_db / 40)
    w = 2 * np.pi * f0 / fs
    al = np.sin(w) / (2 * q)
    b = np.array([1 + al * A, -2 * np.cos(w), 1 - al * A])
    a = np.array([1 + al / A, -2 * np.cos(w), 1 - al / A])
    return b / a[0], a / a[0]


def onepole_env(x, att_ms, rel_ms, fs=FS):
    """Peak follower on |x| (linear), attack / release one-poles."""
    aa = np.exp(-1 / (att_ms * 1e-3 * fs))
    ar = np.exp(-1 / (rel_ms * 1e-3 * fs))
    e = np.zeros_like(x)
    s = 0.0
    ax = np.abs(x)
    for i, v in enumerate(ax):
        c = aa if v > s else ar
        s = c * s + (1 - c) * v
        e[i] = s
    return e


# ---------------------------------------------------------------- test signals -----------------------------

VOWELS = {  # (F1, F2, F3, F4) Hz, bandwidths (Hz); standard textbook values (Peterson & Barney style)
    "a": ((730, 1090, 2440, 3400), (90, 110, 160, 250)),
    "e": ((530, 1840, 2480, 3500), (80, 100, 160, 250)),
    "o": ((570, 840, 2410, 3400), (80, 90, 160, 250)),
}


def glottal(f0, n, fs=FS, jitter=0.0, oq=0.6, sk=0.7, seed=7):
    """Rosenberg-type pulse train, derivative taken (lip radiation)."""
    rng = np.random.default_rng(seed)
    y = np.zeros(n)
    t = 0.0
    while t < n:
        T = fs / f0 * (1 + jitter * rng.standard_normal())
        To = oq * T
        Tp = sk * To
        i0 = int(t)
        for k in range(int(To) + 1):
            idx = i0 + k
            if idx >= n:
                break
            tt = idx - t
            if tt < 0:
                continue
            if tt < Tp:
                y[idx] += 0.5 * (1 - np.cos(np.pi * tt / Tp))
            elif tt < To:
                y[idx] += np.cos(0.5 * np.pi * (tt - Tp) / (To - Tp))
        t += T
    return np.diff(np.concatenate([[0.0], y]))


def formants(x, vowel="a", fs=FS):
    fr, bws = VOWELS[vowel]
    y = x
    for f, bw in zip(fr, bws):
        r = np.exp(-np.pi * bw / fs)
        a = [1, -2 * r * np.cos(2 * np.pi * f / fs), r * r]
        y = signal.lfilter([sum(a)], a, y)
    return y


def harmonic_source(f0, n, fs=FS, vib_cents=0.0, vib_hz=5.5):
    """Band-limited glottal-derivative source: cosine-phase harmonics (a pulse train), amplitude 1/k,
    every partial below 0.45 fs. With vib_cents > 0 the pitch has a vibrato (phase integrated)."""
    t = np.arange(n) / fs
    inst = f0 * 2 ** (vib_cents / 1200 * np.sin(2 * np.pi * vib_hz * t))
    ph = 2 * np.pi * np.cumsum(inst) / fs
    y = np.zeros(n)
    kmax = int(0.45 * fs / (f0 * 2 ** (abs(vib_cents) / 1200)))
    for k in range(1, kmax + 1):
        y += np.cos(k * ph) / k
    return y


def voice(f0=187.3, dur=1.0, vowel="a", level_db=-18.0, jitter=0.0, fs=FS, seed=7):
    """jitter > 0 adds a vibrato of jitter * 3000 cents (0.01 -> 30 cents) to sound alive."""
    n = int(dur * fs)
    v = formants(harmonic_source(f0, n, fs, vib_cents=jitter * 3000), vowel, fs)
    v = signal.lfilter(*bw2(70, "high", fs), v)
    return v / rms(v) * 10 ** (level_db / 20)


def sibilant(dur=0.15, level_db=-24.0, fs=FS, seed=3):
    n = int(dur * fs)
    w = np.random.default_rng(seed).standard_normal(n)
    b, a = signal.butter(4, [4000, 10000], btype="band", fs=fs)
    s = signal.lfilter(b, a, w)
    env = np.minimum(1, np.minimum(np.arange(n), n - np.arange(n)) / (0.01 * fs))
    s = s * env
    return s / rms(s[int(0.01 * fs):-int(0.01 * fs)]) * 10 ** (level_db / 20)


def pink(n, level_db, seed=11):
    w = np.random.default_rng(seed).standard_normal(n)
    b = [0.049922035, -0.095993537, 0.050612699, -0.004408786]
    a = [1, -2.494956002, 2.017265875, -0.522189400]
    p = signal.lfilter(b, a, w)
    return p / rms(p) * 10 ** (level_db / 20)


def phrase(fs=FS):
    """'sa — a — so' : sibilant, vowel, gap, vowel. Returns signal and region slices."""
    gap = np.zeros(int(0.3 * fs))
    s1 = sibilant(0.15, -24, fs)
    a = voice(187.3, 0.6, "a", -18, 0.01, fs)
    s2 = sibilant(0.15, -24, fs, seed=5)
    o = voice(155.0, 0.6, "o", -18, 0.01, fs, seed=9)
    parts = [gap, s1, a, gap, s2, o, gap]
    x = np.concatenate(parts)
    edges = np.cumsum([0] + [len(p) for p in parts])
    reg = {"gap1": slice(edges[0], edges[1]), "s1": slice(edges[1], edges[2]),
           "a": slice(edges[2], edges[3]), "gap2": slice(edges[3], edges[4]),
           "s2": slice(edges[4], edges[5]), "o": slice(edges[5], edges[6]),
           "gap3": slice(edges[6], edges[7])}
    return x, reg


def harmonic_split(y, f0, fs=FS, fmax=20000.0, half_bins=8):
    """Energy at harmonics of f0 vs elsewhere (below fmax), Blackman-Harris window."""
    n = len(y)
    w = signal.windows.blackmanharris(n)
    Y = np.abs(np.fft.rfft(y * w)) ** 2
    f = np.fft.rfftfreq(n, 1 / fs)
    df = f[1]
    harm = np.zeros_like(Y, dtype=bool)
    k = 1
    while k * f0 < fmax:
        c = int(round(k * f0 / df))
        harm[max(0, c - half_bins):c + half_bins + 1] = True
        k += 1
    band = (f > 20) & (f < fmax)
    eh = Y[band & harm].sum()
    ei = Y[band & ~harm].sum()
    return 10 * np.log10(ei / eh + 1e-30)


def group_delay_ms(b, a, freqs, fs=FS):
    w, gd = signal.group_delay((b, a), w=freqs, fs=fs)
    return gd / fs * 1e3
