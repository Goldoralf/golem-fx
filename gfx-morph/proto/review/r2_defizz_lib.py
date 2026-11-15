# SPDX-License-Identifier: GPL-3.0-or-later
"""Review: the de-fizz detector and cut law of README §2.3 (shared by r2 and r2b)."""
import numpy as np
from scipy import signal
from common import FS

TD = np.array([0, 6, 12, 18, 24, 30, 36, 42, 48], float)
TM = np.array([0.1, -5.5, -10.4, -13.9, -16.0, -17.0, -17.5, -17.7, -17.8])


def share_of(sig, det=5.0):
    b, a = signal.butter(2, 4500, "high", fs=FS)
    hf = signal.lfilter(b, a, signal.lfilter(b, a, sig))
    ad = np.exp(-1 / (det * 1e-3 * FS))
    pf = signal.lfilter([1 - ad], [1, -ad], sig ** 2)
    ph = signal.lfilter([1 - ad], [1, -ad], hf ** 2)
    return 10 * np.log10((ph + 1e-20) / (pf + 1e-20)), 10 * np.log10(ph + 1e-20)


def cut_of(share, rng, thr=-14.0, knee=8.0, att=0.5, rel=30.0):
    t = -rng * np.clip((share - thr) / knee, 0, 1)
    aa, ar = np.exp(-1 / (att * 1e-3 * FS)), np.exp(-1 / (rel * 1e-3 * FS))
    g = np.zeros_like(t)
    s = 0.0
    for i, v in enumerate(t):
        c = aa if v < s else ar
        s = c * s + (1 - c) * v
        g[i] = s
    return g
