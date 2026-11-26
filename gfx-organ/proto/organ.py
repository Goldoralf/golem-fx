# SPDX-License-Identifier: GPL-3.0-or-later
"""gfx-organ prototype (numpy): the drawbar law on the polysynth Spectral core's clean bank.

Not the pedal: a model of README §2.2 – §2.4 small enough to measure the core idea.

- Bank: complex one-pole cascades, order 4, ⅓ ERB apart and wide (≥ 20 Hz), hanging from 2 kHz down to
  ~40 Hz, at the bank rate FB = 24 kHz (gfx-polysynth's clean bank, §2.2 there).
- One band per partial: smoothed band levels, local-maximum selection s_b (6 dB crossfade), design
  rotations ρ_b (two neighbours read one phase at their crossover), skirt gain |H_b(f̂)|.
- Drawbar law: every footage is a power of the band's unit phasor u = e^{jθ}: 16' = u^½, 5⅓' = u^{3/2},
  8' = u, 4' = u², 2⅔' = u³, 2' = u⁴, 1⅗' = u⁵, 1⅓' = u⁶, 1' = u⁸. θ is the band's unwrapped phase, so
  the half powers are branch-tracked by construction (gfx-octaver Poly's √z).
- Root sieve: a selected band is an overtone if a lower selected band sits at f / k (k = 2 … 8) within a
  window that allows for string stretch; only roots get the registration ("roots"), or every partial
  does ("all", the POG-style law).
- Organ envelope: each root's level is its body peak (dyn = 1: the strings' balance kept; < 1 compresses
  towards the loudest root, which boosts weak spurious roots: README §2.4 makes it a global gain), held
  until the band falls under a relative floor.
"""
import numpy as np
from scipy.signal import lfilter, resample_poly

FB = 24000.0
TOP = 2000.0
TICK = 16

# footage → ratio to the played note (8' = 1)
FOOT = [('16', 0.5), ('5⅓', 1.5), ('8', 1.0), ('4', 2.0), ('2⅔', 3.0), ('2', 4.0), ('1⅗', 5.0),
        ('1⅓', 6.0), ('1', 8.0)]
RATIOS = np.array([r for _, r in FOOT])


def drawbar_gain(n):
    """Drawbar position 0 – 8 → linear gain, 8 = 0 dB, −3 dB per step, 0 = off."""
    n = np.asarray(n, float)
    return np.where(n > 0, 10 ** (-3.0 * (8 - n) / 20), 0.0)


def reg(s):
    """'888000000' → nine positions."""
    return np.array([int(c) for c in s])


def erb(f):
    return 24.7 + 0.108 * f


def erb_num(f):
    return 21.4 * np.log10(1 + 0.00437 * f)


def erb_freq(e):
    return (10 ** (e / 21.4) - 1) / 0.00437


def grid(low=40.0, step=1 / 3, width=1 / 3, minw=20.0):
    top = erb_num(TOP)
    c = []
    k = 0
    while True:
        f = erb_freq(top - step * k)
        if f < low:
            break
        c.append(f)
        k += 1
    c = np.array(c[::-1])
    w = np.maximum(width * erb(c), minw)
    return c, w


def band_resp(fc, w, f, order=4, fs=FB):
    """Complex response of one band (cascade of `order` one-poles, unit gain at fc) at f."""
    r = np.exp(-np.pi * w / fs)
    p = r * np.exp(2j * np.pi * fc / fs)
    zi = np.exp(-2j * np.pi * f / fs)
    h = (1 - r) / (1 - p * zi)
    return h ** order


class Bank:
    def __init__(self, low=40.0, order=4):
        self.c, self.w = grid(low)
        self.order = order
        self.n = len(self.c)
        r = np.exp(-np.pi * self.w / FB)
        self.p = r * np.exp(2j * np.pi * self.c / FB)
        self.g = 1 - r
        # crossovers (equal level between neighbours) and design rotations
        rho = np.zeros(self.n)
        self.fx = np.zeros(self.n - 1)
        for b in range(self.n - 1):
            lo, hi = self.c[b], self.c[b + 1]
            for _ in range(60):
                m = 0.5 * (lo + hi)
                d = abs(band_resp(self.c[b], self.w[b], m)) - abs(band_resp(self.c[b + 1], self.w[b + 1], m))
                lo, hi = (m, hi) if d > 0 else (lo, m)
            fx = 0.5 * (lo + hi)
            self.fx[b] = fx
            phb = np.angle(band_resp(self.c[b], self.w[b], fx))
            phn = np.angle(band_resp(self.c[b + 1], self.w[b + 1], fx))
            rho[b + 1] = rho[b] + phb - phn
        self.rho = rho
        self.tau = 1 / (np.pi * self.w)

    def run(self, x):
        """x real at FB → z (bands × samples), the analytic band signals (a sine at fc reads its amplitude)."""
        z = np.empty((self.n, len(x)), complex)
        for b in range(self.n):
            y = x.astype(complex)
            for _ in range(self.order):
                y = lfilter([self.g[b]], [1, -self.p[b]], y)
            z[b] = 2 * y  # analytic: a real sine of amplitude a gives |z| = a at the centre
        return z


def one_pole(x, tau, fs, axis=-1):
    a = np.exp(-1 / (tau * fs))
    return lfilter([1 - a], [1, -a], x, axis=axis)


def att_rel(x, att, rel, fs):
    """Attack / release one-pole on a 2-D (bands × ticks) array, along ticks."""
    aa = np.exp(-1 / (att * fs))
    ar = np.exp(-1 / (rel * fs))
    y = np.empty_like(x)
    s = x[:, 0].copy()
    for t in range(x.shape[1]):
        v = x[:, t]
        a = np.where(v > s, aa, ar)
        s = a * s + (1 - a) * v
        y[:, t] = s
    return y


class Organ:
    """The core, offline (whole-signal numpy)."""

    def __init__(self, registration='888000000', law="roots", dyn=1.0, overtones=0.0, floor_db=-40.0,
                 abs_floor_db=-66.0, perc=None, perc_tau=0.2, perc_gain=0.5, tol_c=6.0,
                 bstretch=2e-4, kmax=16, depth_db=40.0, low=40.0, confirm=4.0, pure_min=0.5):
        self.d = drawbar_gain(reg(registration) if isinstance(registration, str) else registration)
        self.law = law
        self.dyn = dyn
        self.ov = overtones
        self.floor_db = floor_db
        self.abs_floor = 10 ** (abs_floor_db / 20)
        self.perc = perc  # None, 2 or 3
        self.perc_tau = perc_tau
        self.perc_gain = perc_gain
        self.tol_c = tol_c
        self.bstretch = bstretch
        self.kmax = kmax
        self.depth_db = depth_db
        self.bank = Bank(low)
        self.confirm = confirm
        self.pure_min = pure_min

    def analyse(self, x):
        bk = self.bank
        z = bk.run(x)
        n, N = z.shape
        th = np.unwrap(np.angle(z), axis=1)
        # instantaneous frequency, smoothed 10 ms
        dth = np.diff(th, axis=1, prepend=th[:, :1])
        f_inst = dth * FB / (2 * np.pi)
        fhat = one_pole(f_inst, 0.010, FB, axis=1)
        # purity: the instantaneous frequency's variance over 10 ms against (0.25 B)² (gfx-polysynth §2.2)
        var = np.maximum(one_pole(f_inst ** 2, 0.010, FB, axis=1) - fhat ** 2, 0)
        T = N // TICK
        idx = np.arange(T) * TICK + TICK - 1
        amp = np.abs(z)
        # levels: attack 3 ms, release 50 ms, at the tick rate
        a_t = amp[:, idx]
        lvl = att_rel(a_t, 0.003, 0.050, FB / TICK)
        ldb = 20 * np.log10(lvl + 1e-12)
        # selection
        c = lambda d: np.clip(d / 6 + 0.5, 0, 1)
        lpad = np.vstack([np.full((1, T), -300.0), ldb, np.full((1, T), -300.0)])
        s = c(ldb - lpad[:-2]) * c(ldb - lpad[2:])
        fh = fhat[:, idx]
        # clamp f̂ to the band's neighbourhood, skirt gain
        half = np.gradient(bk.c)[:, None] * 0.6
        fcl = np.clip(fh, bk.c[:, None] - half, bk.c[:, None] + half)
        skirt = np.abs(band_resp(bk.c[:, None], bk.w[:, None], fcl))
        A = lvl / np.maximum(skirt, 0.2)
        self.z, self.th, self.idx, self.T = z, th, idx, T
        self.ldb, self.s, self.fh, self.A = ldb, s, fh, A
        self.pur = np.exp(-var[:, idx] / (0.25 * bk.w[:, None]) ** 2)
        return self

    def sieve(self):
        """F_b (bands × ticks): 1 for a root, 0 for an overtone of a lower selected band."""
        s, fh, ldb = self.s, self.fh, self.ldb
        n, T = s.shape
        F = np.ones((n, T))
        Lmax = ldb.max(axis=0)
        cand = (s > 0.5) & (ldb > Lmax[None, :] + self.floor_db) & (ldb > 20 * np.log10(self.abs_floor))
        ks = np.arange(2, self.kmax + 1)
        st = 1200 / np.log(2) * self.bstretch * (ks ** 2 - 1) / 2  # cents of stretch allowed per k
        for t in range(T):
            pk = np.nonzero(cand[:, t])[0]
            if len(pk) < 2:
                continue
            fp = fh[pk, t]
            lp = ldb[pk, t]
            for i in range(1, len(pk)):
                lower = slice(0, i)
                ratio = fp[i] / fp[lower]
                cents = 1200 * np.log2(ratio[:, None] / ks[None, :])  # lower × k
                ok = (cents > -self.tol_c) & (cents < self.tol_c + st[None, :])
                ok &= (lp[lower][:, None] > lp[i] - self.depth_db)
                if ok.any():
                    F[pk[i], t] = 0.0
        # an impure band (two partials) is never a root
        F[self.pur < self.pure_min] = 0.0
        F[~cand] = 0.0
        # confirmation: a root sounds its registration once it has been a root for `confirm` τ_b; until then
        # (and never for an overtone) it plays its own partial as a sine ("pre", README §2.3)
        need = np.ceil(self.confirm * self.bank.tau * FB / TICK).astype(int)
        R = np.zeros_like(F)
        und = np.zeros_like(F)
        run = np.zeros(n, int)
        crun = np.zeros(n, int)   # ticks since the band became a candidate (a gap under 20 ms is bridged)
        gap = np.zeros(n, int)
        gmax = int(0.020 * FB / TICK)
        for t in range(T):
            run = np.where(F[:, t] > 0.5, run + 1, 0)
            c = cand[:, t]
            gap = np.where(c, 0, gap + 1)
            crun = np.where(c, crun + 1, np.where(gap > gmax, 0, crun))
            R[:, t] = run >= need
            # undecided until confirmed a root, for at most 2 `need` ticks; then, by default, an overtone
            und[:, t] = c & (run < need) & (crun < 2 * need)
        self.Fraw = F
        self.F = one_pole(R, 0.010, FB / TICK, axis=1)
        self.pre = one_pole(und, 0.005, FB / TICK, axis=1)
        self.cand = cand
        return self

    def envelope(self, onsets=()):
        """Organ level per band: body peak (held), compressed towards the loudest root; relative floor."""
        A, s = self.A, self.s
        n, T = A.shape
        fs_t = FB / TICK
        # peak since the last onset, attack 1 ms (ticks are 0.67 ms: take the running max), held
        P = np.empty_like(A)
        on_t = sorted(int(o * fs_t) for o in onsets)
        cur = A[:, 0].copy()
        j = 0
        for t in range(T):
            if j < len(on_t) and t >= on_t[j]:
                cur = A[:, t].copy()
                j += 1
            cur = np.maximum(cur, A[:, t])
            P[:, t] = cur
        if self.law == 'roots':
            w = self.F * s
        else:
            w = s
        Pm = (P * (w > 0.5)).max(axis=0) + 1e-12
        G = Pm[None, :] * (P / Pm[None, :]) ** self.dyn
        # gate: band over the relative floor and the absolute one (6 dB of hysteresis left out here)
        open_ = (self.ldb > 20 * np.log10(Pm)[None, :] + self.floor_db) & (A > self.abs_floor)
        gate = one_pole(open_.astype(float), 0.005, fs_t, axis=1)
        self.Gk = G * gate
        self.onsets = on_t
        return self

    def render(self, onsets=()):
        """Output at FB."""
        bk = self.bank
        n, N = self.z.shape
        T = self.T
        tt = np.arange(N)
        ticks = self.idx

        def up(m):  # ticks → samples, linear
            return np.array([np.interp(tt, ticks, row) for row in m])

        if self.law == 'roots':
            wr = up(self.s * self.F)
            wo = up(self.s * np.clip(1 - self.F - self.pre, 0, 1)) * self.ov
            wp = up(self.s * self.pre)
            wo += wp * self.d.max()
        else:
            wr = up(self.s)
            wo = np.zeros_like(wr)
            wp = np.zeros_like(wr)
        G = up(self.Gk)
        th = self.th + bk.rho[:, None]
        fh = up(self.fh)
        fcap = 0.42 * FB
        y = np.zeros(N)
        # percussion envelope: restarted by each onset, per band at the band's own rise would be §2.4;
        # here one global envelope from the given onsets
        pe = np.zeros(N)
        for o in onsets:
            i0 = int(o * FB)
            pe[i0:] = np.exp(-(np.arange(N - i0)) / (self.perc_tau * FB))
        for k, r in enumerate(RATIOS):
            if self.d[k] == 0:
                continue
            fade = np.clip((fcap - r * fh) / 500, 0, 1)
            y += self.d[k] * np.sum(wr * G * fade * np.cos(r * th), axis=0)
        if self.perc:
            r = 2.0 if self.perc == 2 else 3.0
            fade = np.clip((fcap - r * fh) / 500, 0, 1)
            # undecided bands take the percussion too (their partial × 2 or × 3 is a harmonic of the note):
            # the percussion speaks with the attack, not at the confirmation
            y += self.perc_gain * pe * np.sum((wr + wp) * G * fade * np.cos(r * th), axis=0)
        y += np.sum(wo * G * np.cos(th), axis=0)
        return y


# ------------------------------------------------------------------ synthetic strings (gfx-polysynth §1.3)

NOTE = {'E2': 82.407, 'F#2': 92.499, 'G#2': 103.83, 'A2': 110.0, 'B2': 123.47, 'C3': 130.81, 'D3': 146.83,
        'E3': 164.81, 'G3': 196.0, 'G#3': 207.65, 'A3': 220.0, 'B3': 246.94, 'C4': 261.63, 'D4': 293.66,
        'E4': 329.63, 'G#4': 415.30, 'A4': 440.0, 'B4': 493.88, 'E5': 659.26}


def string(f0, dur, fs, rng, p=0.13, B=1e-4, t60=3.0, fmax=7000.0, t0=0.0, level=0.25, bright=1.0):
    N = int(dur * fs)
    t = np.arange(N) / fs
    y = np.zeros(N)
    on = t >= t0
    tl = np.maximum(t - t0, 0)
    k = 1
    while True:
        fk = k * f0 * np.sqrt(1 + B * k * k)
        if fk > fmax:
            break
        a = abs(np.sin(np.pi * k * p)) / k ** bright
        t60k = t60 / (1 + 0.15 * (k - 1))
        y += on * a * np.exp(-6.91 * tl / t60k) * np.sin(2 * np.pi * fk * tl + rng.uniform(0, 2 * np.pi))
        k += 1
    # 4 ms pick noise
    nn = int(0.004 * fs)
    i0 = int(t0 * fs)
    y[i0:i0 + nn] += 0.3 * rng.standard_normal(min(nn, N - i0)) * np.hanning(2 * nn)[nn:][:max(0, min(nn, N - i0))]
    return level * y / max(1e-9, np.max(np.abs(y)))


CHORDS = {
    'E5 power (E2 B2 E3)': ['E2', 'B2', 'E3'],
    'open E (E2 B2 E3 G#3 B3 E4)': ['E2', 'B2', 'E3', 'G#3', 'B3', 'E4'],
    'open C (C3 E3 G3 C4 E4)': ['C3', 'E3', 'G3', 'C4', 'E4'],
    'A minor barre (A2 E3 A3 C4 E4)': ['A2', 'E3', 'A3', 'C4', 'E4'],
    'E4 + G#4': ['E4', 'G#4'],
    'E2 + G#2': ['E2', 'G#2'],
}


def chord(notes, dur, fs, seed, spread=0.020):
    rng = np.random.default_rng(seed)
    x = np.zeros(int(dur * fs))
    for i, nm in enumerate(notes):
        x += string(NOTE[nm], dur, fs, rng, p=rng.uniform(0.1, 0.2), t0=0.05 + i * spread / max(1, len(notes) - 1))
    return 0.5 * x / np.max(np.abs(x))


def to_bank(x, fs):
    if fs == FB:
        return x
    g = np.gcd(int(fs), int(FB))
    return resample_poly(x, int(FB) // g, int(fs) // g)


# ------------------------------------------------------------------ metrics

def spectrum(y, t0, t1, fs=FB):
    seg = y[int(t0 * fs):int(t1 * fs)]
    w = np.hanning(len(seg))
    S = np.abs(np.fft.rfft(seg * w)) * 2 / w.sum()
    f = np.fft.rfftfreq(len(seg), 1 / fs)
    return f, S


def comp_level(f, S, fq, tol_hz=None):
    """Amplitude of the component nearest fq (peak within ±max(3 bins, 15 ¢))."""
    df = f[1] - f[0]
    tol = max(3 * df, fq * (2 ** (15 / 1200) - 1)) if tol_hz is None else tol_hz
    m = (f > fq - tol) & (f < fq + tol)
    return S[m].max() if m.any() else 0.0


def offgrid_db(f, S, f0s, cents=25.0, lo=30.0, hi=10000.0, half=True, B=1e-4):
    """Energy not within ±cents of a multiple of f0/2 (half=True) of any played note, re total, lo – hi.
    The window widens upwards by the strings' stretch (a component from partial k sits up to
    866 · B · k² cents sharp: k ≤ f / f0)."""
    m = (f > lo) & (f < hi)
    on = np.zeros_like(f, bool)
    for f0 in f0s:
        base = f0 / 2 if half else f0
        h = np.round(f / base)
        h[h < 1] = 1
        c = 1200 * np.log2(np.maximum(f, 1e-9) / (h * base))
        st = 866 * B * (np.maximum(f, f0) / f0) ** 2
        # never narrower than the window's main lobe (± 3 bins)
        wc = 1200 * np.log2(1 + 3 * (f[1] - f[0]) / np.maximum(f, 1e-9))
        tol = np.maximum(cents, wc)
        on |= (c > -tol) & (c < tol + st)
    e = S ** 2
    tot = e[m].sum()
    off = e[m & ~on].sum()
    return 10 * np.log10(off / tot + 1e-15)


def registration_error(y, f0, d, t0=0.4, t1=1.4, B=1e-4):
    """Per-drawbar level error (dB, after a best gain) and the energy outside the registration's lines."""
    f, S = spectrum(y, t0, t1)
    # the string's own partial k sits at k f0 √(1 + B k²): the components follow the partial they come from
    stretch = np.sqrt(1 + B)
    lv = np.array([comp_level(f, S, r * f0 * stretch) for r in RATIOS])
    on = d > 0
    tgt = d[on]
    meas = lv[on]
    gdb = np.mean(20 * np.log10(meas + 1e-12) - 20 * np.log10(tgt))
    err = 20 * np.log10(meas + 1e-12) - 20 * np.log10(tgt) - gdb
    # energy outside the registration's lines (±15 ¢), 30 Hz – 10 kHz
    m = (f > 30) & (f < 10000)
    online = np.zeros_like(f, bool)
    for r in RATIOS[on]:
        fq = r * f0 * stretch
        online |= np.abs(f - fq) < max(3 * (f[1] - f[0]), fq * (2 ** (15 / 1200) - 1))
    e = S ** 2
    extra = 10 * np.log10(e[m & ~online].sum() / e[m].sum() + 1e-15)
    return err, extra, lv
