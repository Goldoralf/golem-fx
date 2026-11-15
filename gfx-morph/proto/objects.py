# SPDX-License-Identifier: GPL-3.0-or-later
"""The objects of the Box stage (README §2.7), generated from physics, no recording.

Each object is a list of modes (f Hz, Q, gain) plus two 'skirt' corners (the non-resonant transmission of
the dense mode region, a 2nd-order high-pass and low-pass). Formulas are textbook acoustics [K]:
- closed-open tube (a can, a bucket, a jar held to the mouth): axial f = (2n-1) c / (4 L'), L' = L + 0.6 a;
- open-open tube (a pipe): f = n c / (2 L'), L' = L + 1.2 a;
- closed cylinder transverse modes: f = c / (2 pi) sqrt((j'_mn / a)^2 + (q pi / L)^2), j' = zeros of J'_m;
- rectangular cavity: f = c / 2 sqrt((l/Lx)^2 + (m/Ly)^2 + (n/Lz)^2);
- Helmholtz resonator: f = c / (2 pi) sqrt(A / (V L_eff));
- thin-ring flexural (wall) modes of a cylinder: f_n = 1 / (2 pi) h / (a^2 sqrt 12) sqrt(E / rho) n (n^2 - 1) / sqrt(n^2 + 1);
- clamped circular plate (a drum lid): f = lambda^2 / (2 pi a^2) sqrt(D / (rho h)), D = E h^3 / (12 (1 - nu^2)).
Q from the material (T60 = 6.91 Q / (pi f)). Mode gains: the mode shape at the source (the opening) times
at the listening point `position` along the main axis, times a coupling weight per family.
"""
import numpy as np

C = 343.0
JP = [(1.841, 1), (3.054, 2), (3.832, 0), (4.201, 3), (5.318, 1), (5.331, 4), (6.416, 5), (6.706, 2), (7.016, 0)]
# zeros of J'_m: (value, m); (0,0) plane-wave mode excluded (it is the axial series)
PLATE = [10.22, 21.26, 34.88, 39.77, 51.04, 60.82, 69.67, 84.58]
MAX_MODES = 48


def ring_modes(a, h, E, rho, nmax=9):
    k = h / (a * a * np.sqrt(12)) * np.sqrt(E / rho) / (2 * np.pi)
    return [k * n * (n * n - 1) / np.sqrt(n * n + 1) for n in range(2, nmax + 1)]


def closed_open(L, a, q_air, pos, fmax=8000):
    Lp = L + 0.6 * a
    out = []
    n = 1
    while True:
        f = (2 * n - 1) * C / (4 * Lp)
        if f > fmax:
            break
        # closed end at x = 0, opening (source) at x = 1; pressure shape cos((2n-1) pi x / 2) is 0 at the
        # opening: the mouth drives volume velocity there, whose shape is sin(...) = +-1: use the velocity
        # shape at the source and the pressure shape at the listening point
        src = np.sin((2 * n - 1) * np.pi / 2)
        mic = np.cos((2 * n - 1) * np.pi * pos / 2)
        out.append((f, q_air * (1 + 0.15 * (n - 1)) ** -0.5, src * (0.35 + 0.65 * mic)))
        n += 1
    # transverse modes, q = axial index of the closed-open quarter-wave family
    for jv, m in JP:
        for q in range(0, 3):
            kz = (2 * q + 1) * np.pi / (2 * Lp)
            f = C / (2 * np.pi) * np.sqrt((jv / a) ** 2 + kz ** 2)
            if f < fmax:
                out.append((f, q_air * 0.8, 0.5 * np.cos((2 * q + 1) * np.pi * pos / 2) * (0.6 if m else 1.0)))
    return out


def open_open(L, a, q_air, pos, fmax=8000):
    Lp = L + 1.2 * a
    out = []
    n = 1
    while n * C / (2 * Lp) < fmax:
        f = n * C / (2 * Lp)
        mic = np.cos(n * np.pi * pos)
        out.append((f, q_air * n ** -0.35, 0.4 + 0.6 * mic))  # radiation lowers Q with n
        n += 1
    return out


def rect(Lx, Ly, Lz, q, pos, fmax=2500):
    out = []
    for l in range(0, 12):
        for m in range(0, 10):
            for n in range(0, 10):
                if l == m == n == 0:
                    continue
                f = C / 2 * np.sqrt((l / Lx) ** 2 + (m / Ly) ** 2 + (n / Lz) ** 2)
                if f < fmax:
                    # source in a corner of the open face (all cos = 1), mic along x
                    out.append((f, q, np.cos(l * np.pi * pos) * (0.5 ** (m > 0)) * (0.5 ** (n > 0))))
    return out


def helmholtz(A, V, Leff):
    return C / (2 * np.pi) * np.sqrt(A / (V * Leff))


T60_MAX = {"Tin Can": 0.6, "Bucket": 0.4, "Pipe": 0.5, "Bottle": 1.0, "Cardboard Box": 0.1, "Oil Drum": 1.5,
           "Glass Jar": 1.2, "Cone": 0.3, "Hands": 0.05}  # s: the object held, filled, touched (voicing [I])


def obj(name, pos=0.5):
    """Returns dict(modes=[(f, Q, gain)], hp=Hz, lp=Hz, skirt=dB of the non-resonant path)."""
    if name == "Tin Can":
        a, L = 0.037, 0.11
        m = closed_open(L, a, 28, pos)
        m += [(f, 260, 0.12) for f in ring_modes(a, 0.00022, 200e9, 7850)]
        return dict(modes=m, hp=450, lp=6500, skirt=-14)
    if name == "Bucket":
        a, L = 0.13, 0.27
        m = closed_open(L, a, 14, pos, fmax=3000)
        m += [(f, 35, 0.15) for f in ring_modes(a, 0.0018, 1.5e9, 950)]
        return dict(modes=m, hp=150, lp=3500, skirt=-12)
    if name == "Pipe":
        return dict(modes=open_open(0.60, 0.025, 45, pos, fmax=6000), hp=200, lp=7000, skirt=-16)
    if name == "Bottle":
        fh = helmholtz(np.pi * 0.0095 ** 2, 7.5e-4, 0.08 + 1.7 * 0.0095)
        m = [(fh, 40, 1.4)]
        m += closed_open(0.22, 0.04, 30, pos, fmax=7000)[1:]
        m += [(f, 700, 0.10) for f in ring_modes(0.04, 0.003, 70e9, 2500) if f < 9000]
        return dict(modes=m, hp=90, lp=5000, skirt=-16)
    if name == "Cardboard Box":
        return dict(modes=rect(0.40, 0.30, 0.30, 7, pos), hp=120, lp=2200, skirt=-6)
    if name == "Oil Drum":
        a, L = 0.29, 0.88
        m = []
        for n in range(1, 30):
            f = n * C / (2 * L)
            if f > 2500:
                break
            m.append((f, 60 * n ** -0.2, np.cos(n * np.pi * pos)))
        for jv, mm in JP[:6]:
            for q in range(0, 3):
                f = C / (2 * np.pi) * np.sqrt((jv / a) ** 2 + (q * np.pi / L) ** 2)
                if f < 2500:
                    m.append((f, 50, 0.4 * np.cos(q * np.pi * pos)))
        s = np.sqrt((200e9 * 0.001 ** 3 / (12 * 0.91)) / (7850 * 0.001))
        m += [(lam * s / (2 * np.pi * a * a), 120, 0.5) for lam in PLATE]
        return dict(modes=m, hp=40, lp=2500, skirt=-10)
    if name == "Glass Jar":
        a, L = 0.045, 0.15
        m = closed_open(L, a, 32, pos)
        m += [(f, 900, 0.10) for f in ring_modes(a, 0.003, 70e9, 2500) if f < 12000]
        return dict(modes=m, hp=350, lp=7000, skirt=-14)
    if name == "Cone":
        # a truncated cone (the mouth at the narrow end): the complete-cone series n c / (2 L')
        m = open_open(0.62, 0.15, 14, pos, fmax=6000)
        return dict(modes=m, hp=280, lp=7500, skirt=-4)
    if name == "Hands":
        fh = helmholtz(2.0e-4, 1.5e-4, 0.015)
        return dict(modes=[(fh, 3.0, 1.0), (2.6 * fh, 4.0, 0.35)], hp=180, lp=3200, skirt=-3)
    raise KeyError(name)


OBJECTS = ["None", "Tin Can", "Bucket", "Pipe", "Bottle", "Cardboard Box", "Oil Drum", "Glass Jar", "Cone", "Hands"]


def bank(name, pos=0.5, size=1.0, decay=1.0, fs=48000.0):
    """The object's 48 slots: arrays f, Q, g (sorted by f), padded with silent modes; skirt corners."""
    o = obj(name, pos)
    # T60 cap of the object (a held can does not ring for 7 s): Q <= pi f T60_max / 6.91, at size 100 %
    m = [(f, min(Q, np.pi * f * T60_MAX[name] / 6.91), g) for f, Q, g in o["modes"] if f >= 60]
    m = sorted([(f / size, Q, g) for f, Q, g in m if 30 < f / size < 0.45 * fs], key=lambda t: t[0])
    # keep the 48 strongest by |gain| / sqrt(f) (low, strong modes first), then re-sort by frequency
    if len(m) > MAX_MODES:
        m = sorted(m, key=lambda t: -abs(t[2]) / np.sqrt(t[0]))[:MAX_MODES]
        m = sorted(m, key=lambda t: t[0])
    f = np.array([t[0] for t in m])
    Q = np.array([t[1] for t in m]) * decay
    g = np.array([t[2] for t in m])
    n = len(f)
    if n < MAX_MODES:
        pad = MAX_MODES - n
        f = np.concatenate([f, np.full(pad, f[-1])])
        Q = np.concatenate([Q, np.full(pad, Q[-1])])
        g = np.concatenate([g, np.zeros(pad)])
    return dict(f=f, Q=Q, g=g, n=n, hp=o["hp"] / size, lp=min(o["lp"] / size, 0.45 * fs), skirt=o["skirt"])
