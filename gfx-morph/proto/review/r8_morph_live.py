# SPDX-License-Identifier: GPL-3.0-or-later
"""Review r8: the morph's level hold with the LIVE correlation (README §2.5), which p6 does not run.
A and B are level-matched by their static tables (not per-signal, as p6 does), rho is read from 30 ms
exponential averages of A B, A^2, B^2, clamped to -0.5 .. 1, and the gain 1 / sqrt(..) smoothed 20 ms.
Output level at morph 25 / 50 / 75 % re the mean of the two ends, on p3's phrase and on a real take;
also the largest and smallest 50 ms frame (does the slow gain pump?), and Rectify with / without its DC."""
import sys
import numpy as np
from scipy import signal
sys.path.insert(0, "review")
import rvoice
from common import FS, voice, shape, rms_db, rms

D = 24.0
vref = voice(187.3, 0.5, "a", -18.0)


def dcblock(y, fc=20.0):
    return signal.lfilter(*signal.butter(1, fc, "high", fs=FS), y)


def typed(x, name, dc=True):
    if name == "rectify":  # (1 - r) x + r |x|, r = d / 48, inside the 2x pair
        from common import up, down, adaa1
        u = up(x, 2)
        r = D / 48
        y = down((1 - r) * np.concatenate([[0.0], 0.5 * (u[1:] + u[:-1])]) + r * adaa1(u, "rectify"), 2)
        yr = down((1 - r) * np.concatenate([[0.0], 0.5 * (vr_u[1:] + vr_u[:-1])]) + r * adaa1(vr_u, "rectify"), 2)
    else:
        y = shape(x, name, D, 2, True)
        yr = shape(vref, name, D, 2, True)
    if dc:
        y, yr = dcblock(y), dcblock(yr)
    return y * rms(vref[2000:]) / rms(yr[2000:])  # the static table of the type


from common import up
vr_u = up(vref, 2)


def blend(A, B, mu, live=True):
    y = (1 - mu) * A + mu * B
    if not live:
        return y
    a = np.exp(-1 / (30e-3 * FS))
    lp = lambda s: signal.lfilter([1 - a], [1, -a], s)
    rho = np.clip(lp(A * B) / np.sqrt(lp(A * A) * lp(B * B) + 1e-30), -0.5, 1.0)
    k = 1 / np.sqrt((1 - mu) ** 2 + mu ** 2 + 2 * mu * (1 - mu) * rho)
    a2 = np.exp(-1 / (20e-3 * FS))
    return y * signal.lfilter([1 - a2], [1, -a2], k)


def frames(y, ref, m, n=2400):
    d = []
    for i in range(0, len(y) - n, n):
        if m[i:i + n].all():
            d.append(rms_db(y[i:i + n]) - rms_db(ref[i:i + n]))
    return (min(d), max(d)) if d else (0, 0)


srcs = {}
a_ = voice(187.3, 0.7, "a", -18.0, jitter=0.01)
o_ = voice(150.0, 0.7, "o", -18.0, jitter=0.01, seed=9)
srcs["p3 phrase"] = (np.concatenate([a_, np.zeros(2400), o_]), np.concatenate([np.ones(len(a_)), np.zeros(2400), np.ones(len(o_))]) > 0)
r = rvoice.load()
if r is not None:
    m, _ = rvoice.active(r)
    srcs["real take"] = (r / np.sqrt(np.mean(r[m] ** 2)) * 10 ** (-18 / 20), m)
for lab, (x, m) in srcs.items():
    print(f"== {lab}")
    T = {n: typed(x, n) for n in ("soft", "hard", "tube", "fold", "rectify")}
    T["rectify, DC kept"] = typed(x, "rectify", dc=False)
    for a, b in (("soft", "hard"), ("soft", "fold"), ("tube", "rectify"), ("soft", "rectify"), ("soft", "rectify, DC kept")):
        A, B = T[a], T[b]
        ends = 0.5 * (rms_db(A[m]) + rms_db(B[m]))
        lin = [rms_db(blend(A, B, mu, False)[m]) - ends for mu in (0.25, 0.5, 0.75)]
        liv = [rms_db(blend(A, B, mu)[m]) - ends for mu in (0.25, 0.5, 0.75)]
        y5 = blend(A, B, 0.5)
        ref = 10 ** (ends / 20) * x / rms(x[m])
        lo, hi = frames(y5, 0.5 * (np.abs(A) + np.abs(B)) + 1e-12, m)
        print(f"  {a:5s} -> {b:16s}: ends {rms_db(A[m]) - rms_db(x[m]):+5.1f} / {rms_db(B[m]) - rms_db(x[m]):+5.1f} dB re input | linear {' '.join(f'{v:+5.1f}' for v in lin)} | live rho {' '.join(f'{v:+5.2f}' for v in liv)}")
