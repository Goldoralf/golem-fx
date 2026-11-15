# SPDX-License-Identifier: GPL-3.0-or-later
"""Review r1: what the guard's cap law covers and what it leaves (README §2.2).

1. Loop gain is a per-frequency matter: the largest gain, at any frequency, that the stages AFTER the
   dirt add to a small signal: the box (per object, box 100 %, pink-normalised + the voice constant of
   p5), the resonant post low-pass, the comp's static make-up, the echo's comb.
2. `mix` < 100 %: the pedal's small-signal gain is 20 log10(1 - m + m 10^(G_ss/20)), not G_ss.
3. Bleed while the singer sings (gate open): the incremental gain of Soft for pink bleed riding on a vowel.
4. The cap law on a real take (GFX_MORPH_VOICE): how much of the sung signal sits under the threshold and
   is cut, for T = -55 / -45 / -40 dBFS and a cap of 21.5 dB (drive 36 dB) or 33 dB (48 dB).
"""
import sys
import numpy as np
from scipy import signal
sys.path.insert(0, "review")
import rvoice
from common import FS, voice, pink, shape, rms_db, rms
from objects import OBJECTS, bank
import p5_box_lib as P

print("== 1. largest small-signal gain after the dirt, dB re bypass")
vv = np.concatenate([voice(187.3, 0.6, "a", -18.0), voice(150.0, 0.6, "o", -18.0, seed=9)])
F = np.geomspace(40, 12000, 6000)
for name in OBJECTS[1:]:
    B = bank(name)
    H = P.response(B, F)
    K = P.pink_gain(H, F)
    h = P.ir(B, int(1.0 * FS)) * 10 ** (K / 20)
    kv = -(rms_db(signal.fftconvolve(vv, h)[:len(vv)][2400:]) - rms_db(vv[2400:]))
    row = []
    for box in (0.5, 1.0):
        T = box * H * 10 ** ((K + kv) / 20) + (1 - box)
        row.append(f"box {int(box*100):3d} %: {20*np.log10(np.abs(T).max()):+5.1f} dB at {F[np.argmax(np.abs(T))]:5.0f} Hz")
    print(f"  {name:13s}: voice constant {kv:+4.1f} dB | " + " | ".join(row))
for r in (25, 50, 100):
    q = 0.707 + (6 - 0.707) * r / 100
    print(f"  post low-pass, reso {r:3d} %: Q {q:.2f}, peak {20*np.log10(q / np.sqrt(1 - 1/(4*q*q))) if q > 0.707 else 0:+5.1f} dB")
for c in (40, 70, 100):
    thr, ratio = -6 - 0.24 * c, 1 + 0.07 * c
    gr = max(0.0, (-18 - thr)) * (1 - 1 / ratio)
    print(f"  comp {c:3d} %: threshold {thr:+.1f} dBFS, ratio {ratio:.1f}, GR at the -18 dBFS reference {gr:.1f} dB -> static make-up {gr/2:+.1f} dB on small signals")
for m, f in ((0.2, 0.15), (0.25, 0.4), (0.5, 0.6), (1.0, 0.9)):
    print(f"  echo mix {int(m*100)} %, feedback {int(f*100)} %: comb peak {20*np.log10(1 + m / (1 - f)):+5.1f} dB")

print("== 2. small-signal gain of the pedal against `mix` (Soft, voice table), dB")
for d, gss in ((24, 8.0), (36, 18.5), (48, 30.2)):
    print(f"  drive {d}: " + " ".join(f"mix {int(m*100):3d} %: {20*np.log10(1 - m + m*10**(gss/20)):+5.1f}" for m in (0.2, 0.35, 0.5, 1.0))
          + f" | the README's cut is G_ss + 3 = {gss+3:.1f} dB at every mix")

print("== 3. bleed under the voice (gate open): incremental gain of the dirt for pink bleed at -50 dBFS riding on a vowel at -18 dBFS RMS")
v = voice(187.3, 1.0, "a", -18.0, 0.01)
b = pink(len(v), -50.0)
MK = {24: -16.0, 36: -17.5, 48: -17.8}
for d in (24, 36, 48):
    y1 = shape(v + b, "soft", d, 2, True) * 10 ** (MK[d] / 20)
    y0 = shape(v, "soft", d, 2, True) * 10 ** (MK[d] / 20)
    alone = shape(b, "soft", d, 2, True) * 10 ** (MK[d] / 20)
    print(f"  drive {d}: bleed alone {rms_db(alone[4000:]) - rms_db(b[4000:]):+5.1f} dB | bleed under the vowel {rms_db((y1 - y0)[4000:]) - rms_db(b[4000:]):+5.1f} dB")

print("== 4. the cap law on a real take")
x = rvoice.load()
if x is None:
    print("  GFX_MORPH_VOICE not set")
    sys.exit()
m, _ = rvoice.active(x)
x = x / np.sqrt(np.mean(x[m] ** 2)) * 10 ** (-18 / 20)


def cap(x, thr, rng, hyst=3.0, att=1.0, rel=80.0, det=5.0):
    ad = np.exp(-1 / (det * 1e-3 * FS))
    e = 10 * np.log10(signal.lfilter([1 - ad], [1, -ad], x * x) + 1e-20)
    aa, ar = np.exp(-1 / (att * 1e-3 * FS)), np.exp(-1 / (rel * 1e-3 * FS))
    g = np.zeros_like(x)
    s, op = -rng, False
    for i, v in enumerate(e):
        if v > thr + hyst:
            op = True
        elif v < thr - hyst:
            op = False
        t = 0.0 if op else -rng * min(1.0, max(0.0, (thr - v) / 6.0))
        c = aa if t > s else ar
        s = c * s + (1 - c) * t
        g[i] = s
    return g, e


la = 10 * np.log10(np.convolve(x * x, np.ones(960) / 960, mode="same") + 1e-20)
print(f"  take at -18 dBFS RMS (active); 20 ms level percentiles 1 / 5 / 10 / 25 %: " + " / ".join(f"{v:.0f}" for v in np.percentile(la[m], [1, 5, 10, 25])) + " dBFS")
for thr in (-55, -45, -40):
    for rng in (21.5, 33.0):
        g, e = cap(x, thr, rng)
        y = x * 10 ** (g / 20)
        cutm = m & (g < -3)
        # count closings inside phrases: gate goes below -3 dB and re-opens within 300 ms
        closed = (g < -3).astype(int)
        edges = np.flatnonzero(np.diff(closed) == 1)
        reop = sum(1 for i in edges if np.any(closed[i + 1:i + int(0.3 * FS)] == 0))
        print(f"  T {thr}, cap {rng:4.1f} dB: active time cut by more than 3 dB {100*cutm.sum()/m.sum():4.1f} % | energy of the active parts kept {rms_db(y[m]) - rms_db(x[m]):+5.2f} dB | "
              f"gate closings that re-open within 0.3 s {reop} in {len(x)/FS:.0f} s")
