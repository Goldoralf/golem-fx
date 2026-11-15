# SPDX-License-Identifier: GPL-3.0-or-later
"""p2: aliasing on a sung vowel (README §2.4).

Signal: a steady 'a' (f0 = 187.3 Hz, no jitter, so every non-aliased product is a harmonic of f0), at
-18 dBFS RMS and at -6 dBFS RMS (a scream into a hot preamp). Metric: energy away from the harmonics
(below 20 kHz) relative to the energy on them, in dB (lower is cleaner). A 32x render gives the floor
of the metric itself (window leakage, the shaper's own content above 20 kHz that is not folded).
"""
import numpy as np
from common import voice, shape, harmonic_split, FS

F0 = 187.3
x = voice(F0, 1.2, "a", -18.0)[int(0.2 * FS):]
print(f"input itself: inharmonic re harmonic {harmonic_split(x[2048:], F0):.1f} dB (metric floor)")
x6 = x * 10 ** (12 / 20)

rows = []
for name in ("soft", "hard", "tube", "rectify", "fold"):
    for drive in (24, 48):
        for sig, lab in ((x, "-18"), (x6, "-6")):
            vals = []
            for factor, adaa in ((1, False), (2, False), (2, True), (4, True), (32, False)):
                y = shape(sig, name, drive, factor, adaa)
                vals.append(harmonic_split(y[2048:], F0))
            rows.append((name, drive, lab, vals))
            print(f"{name:8s} drive {drive:2d} dB in {lab} dBFS RMS: inharmonic re harmonic  "
                  f"1x {vals[0]:6.1f} | 2x {vals[1]:6.1f} | 2x+ADAA {vals[2]:6.1f} | 4x+ADAA {vals[3]:6.1f} | "
                  f"32x ref {vals[4]:6.1f} dB")
