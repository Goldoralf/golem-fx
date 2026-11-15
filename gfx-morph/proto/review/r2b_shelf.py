# SPDX-License-Identifier: GPL-3.0-or-later
"""Review r2b: the de-fizz band cut as a minimum-phase high shelf instead of an LR4 band gain.
The LR4 band gain keeps its all-pass in the path at 0 dB (0.10 ms at 1 kHz on every vowel, r5). A TPT SVF
shelf y = low + sqrt(g) k band + g high (k = sqrt 2) is the identity at g = 1. Same phrase and metric as p4."""
import sys
import numpy as np
from scipy import signal
sys.path.insert(0, "review")
from common import FS, phrase, lr4, rms_db, up, down, adaa1
from r2_defizz_lib import share_of, cut_of, TD, TM


def shelf(x, g_db, fc=4500.0):
    g = np.tan(np.pi * fc / FS)
    k = np.sqrt(2.0)
    a1 = 1 / (1 + g * (g + k)); a2 = g * a1; a3 = g * a2
    ic1 = ic2 = 0.0
    y = np.empty_like(x)
    A = 10 ** (g_db / 20)
    for i, v in enumerate(x):
        v3 = v - ic2
        v1 = a1 * ic1 + a2 * v3
        v2 = ic2 + a2 * ic1 + a3 * v3
        ic1 = 2 * v1 - ic1; ic2 = 2 * v2 - ic2
        hp = v - k * v1 - v2
        y[i] = v2 + np.sqrt(A[i]) * k * v1 + A[i] * hp
    return y


x, R = phrase()
mid = lambda y: signal.sosfilt(signal.butter(4, [1000, 4000], "band", fs=FS, output="sos"), y)
top = lambda y: signal.sosfilt(signal.butter(4, 4500, "high", fs=FS, output="sos"), y)
S = lambda y: np.concatenate([y[R["s1"]], y[R["s2"]]])
V = lambda y: np.concatenate([y[R["a"]], y[R["o"]]])
sh, _ = share_of(x)
y_id = shelf(x, np.zeros(len(x)))
print(f"shelf at 0 dB against the input: {rms_db(y_id - x) - rms_db(x):.0f} dB (identity)")
for amt in (0, 50, 100):
    g = cut_of(sh, 0.18 * amt)
    d = np.maximum(0.0, 30.0 + g)
    y = down(adaa1(up(x, 2) * np.repeat(10 ** (d / 20), 2), "soft"), 2) * 10 ** (np.interp(d, TD, TM) / 20)
    lo, hi = lr4(y, 4500.0)
    yl = lo + hi * 10 ** (g / 20)
    ys = shelf(y, g)
    for lab, z in (("LR4 band gain", yl), ("SVF shelf    ", ys)):
        print(f"  de-fizz {amt:3d} % {lab}: 's' 1-4 kHz re vowel {rms_db(S(mid(z))) - rms_db(V(mid(z))):+6.1f} | 's' above 4.5 kHz re the dirt alone {rms_db(S(top(z))) - rms_db(S(top(y))):+6.1f} | 's' full band re vowel {rms_db(S(z)) - rms_db(V(z)):+6.1f}")
