# SPDX-License-Identifier: GPL-3.0-or-later
"""Review r6: the Box stage beyond p5 (README §2.7).

1. Is the mode set stable under `position` and `size`? objects.bank() keeps the 48 strongest modes at the
   current position and drops modes above 0.45 fs or under 30 Hz at the current size.
2. Level of the pink-normalised object on a sung vowel against pitch (98 - 392 Hz, semitone steps, 'a' and
   'o'), against `position` and against `decay`: what one constant per object and size can hold.
3. Object morph by rank: level through the morph for three pairs, with p5's padding (silent slots parked at
   the padded object's last frequency) and with a silent slot taking its partner's f and Q (a pure fade).
4. The bank in f32: a mode at 61 / 30 Hz, Q 40, at 48 and 192 kHz, against f64 (peak frequency and gain).
"""
import sys
import numpy as np
from scipy import signal
sys.path.insert(0, "review")
from common import FS, voice, rms_db, rms
from objects import OBJECTS, bank, obj, T60_MAX, MAX_MODES
import p5_box_lib as P

print("== 1. mode sets")
for name in OBJECTS[1:]:
    n_all = len([m for m in obj(name, 0.5)["modes"] if m[0] >= 60])
    sets = []
    for pos in np.linspace(0, 1, 21):
        B = bank(name, pos=pos)
        sets.append(frozenset(np.round(B["f"][:B["n"]], 1)))
    union = frozenset().union(*sets)
    nsz = [bank(name, size=s)["n"] for s in (0.25, 0.5, 1, 2, 4)]
    zero = sum(1 for g in bank(name)["g"][:bank(name)["n"]] if abs(g) < 1e-9)
    print(f"  {name:13s}: {n_all:3d} candidate modes | distinct sets over position {len(set(sets)):2d}, union {len(union):3d} modes | "
          f"modes kept at size 25/50/100/200/400 %: {nsz} | zero-gain modes among the kept at position 50 %: {zero}")

print("== 2. level on a sung vowel re bypass, pink-normalised (dB)")
F0 = 98 * 2 ** (np.arange(25) / 12)
for name in OBJECTS[1:]:
    B = bank(name)
    K = P.pink_gain(P.response(B))
    lv = []
    for f0 in F0:
        for vw in ("a", "o"):
            v = voice(f0, 0.5, vw, -18.0)
            lv.append(rms_db(P.render(B, v, K)[4800:]) - rms_db(v[4800:]))
    lv = np.array(lv)
    pm = 10 * np.log10(np.mean(10 ** (lv / 10)))
    posl = []
    for pos in (0.0, 0.25, 0.5, 0.75, 1.0):
        Bp = bank(name, pos=pos)
        posl.append(-P.pink_gain(P.response(Bp)) + K)
    decl = []
    for dc in (0.25, 0.5, 1.0, 2.0, 4.0):
        Bd = bank(name, decay=dc)
        decl.append(-P.pink_gain(P.response(Bd)) + K)
    print(f"  {name:13s}: over pitch min {lv.min():+5.1f} max {lv.max():+5.1f} power-mean {pm:+5.1f} (p5's two notes: see results) | "
          f"pink power at position 0/25/50/75/100 %: " + " ".join(f"{v:+5.1f}" for v in posl)
          + " | at decay 25/50/100/200/400 %: " + " ".join(f"{v:+5.1f}" for v in decl))

print("== 3. morph by rank: level of a sung 'a' (187.3 Hz) re bypass at morph 0 / 25 / 50 / 75 / 100 %, static (dB)")
v = voice(187.3, 0.6, "a", -18.0)


def morph(A, B, KA, KB, mu, pure_fade):
    fa, qa, ga = A["f"].copy(), A["Q"].copy(), A["g"] * 10 ** (KA / 20)
    fb, qb, gb = B["f"].copy(), B["Q"].copy(), B["g"] * 10 ** (KB / 20)
    if pure_fade:
        sa, sb = np.arange(MAX_MODES) >= A["n"], np.arange(MAX_MODES) >= B["n"]
        fa[sa & ~sb], qa[sa & ~sb] = fb[sa & ~sb], qb[sa & ~sb]
        fb[sb & ~sa], qb[sb & ~sa] = fa[sb & ~sa], qa[sb & ~sa]
    M = dict(f=np.exp((1 - mu) * np.log(fa) + mu * np.log(fb)), Q=np.exp((1 - mu) * np.log(qa) + mu * np.log(qb)),
             g=(1 - mu) * ga + mu * gb, hp=np.exp((1 - mu) * np.log(A["hp"]) + mu * np.log(B["hp"])),
             lp=np.exp((1 - mu) * np.log(A["lp"]) + mu * np.log(B["lp"])),
             skirt=20 * np.log10((1 - mu) * 10 ** ((A["skirt"] + KA) / 20) + mu * 10 ** ((B["skirt"] + KB) / 20)))
    return M


for a, b in (("Tin Can", "Bucket"), ("Tin Can", "Oil Drum"), ("Hands", "Tin Can"), ("Bottle", "Cone")):
    A, B = bank(a), bank(b)
    KA, KB = P.pink_gain(P.response(A)), P.pink_gain(P.response(B))
    for pf in (False, True):
        row = [rms_db(P.render(morph(A, B, KA, KB, mu, pf), v)[4800:]) - rms_db(v[4800:]) for mu in (0, 0.25, 0.5, 0.75, 1)]
        print(f"  {a:8s} -> {b:8s} ({A['n']:2d} -> {B['n']:2d} modes), {'silent slot = partner' if pf else 'p5 padding        '}: " + " ".join(f"{x:+5.1f}" for x in row))

print("== 4. one TPT mode in f32 against f64 (white noise in, 4 s): level error and peak shift")


def tpt(x, f, q, fs, dt):
    g = dt(np.tan(np.pi * f / fs)); k = dt(1 / q)
    a1 = dt(1) / (dt(1) + g * (g + k)); a2 = g * a1; a3 = g * a2
    ic1 = dt(0); ic2 = dt(0)
    y = np.empty(len(x), dt)
    two = dt(2)
    for i in range(len(x)):
        v3 = x[i] - ic2
        v1 = a1 * ic1 + a2 * v3
        v2 = ic2 + a2 * ic1 + a3 * v3
        ic1 = two * v1 - ic1; ic2 = two * v2 - ic2
        y[i] = k * v1
    return y


rng = np.random.default_rng(3)
for fs in (48000.0, 192000.0):
    for f, q in ((61.0, 40.0), (30.0, 40.0), (30.0, 160.0)):
        n = int(2.0 * fs)
        x = rng.standard_normal(n) * 0.1
        y64 = tpt(x.astype(np.float64), f, q, fs, np.float64)
        y32 = tpt(x.astype(np.float32), f, q, fs, np.float32).astype(np.float64)
        print(f"  fs {fs/1000:.0f} kHz, mode {f:.0f} Hz Q {q:.0f}: f32 - f64 {rms_db(y32 - y64) - rms_db(y64):+7.1f} dB re the mode's output; level {rms_db(y32) - rms_db(y64):+.3f} dB")
