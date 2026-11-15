# SPDX-License-Identifier: GPL-3.0-or-later
"""p6: the dirt morph (README §2.4): type A and type B fed the same band, each level-matched by its own
voice table, blended y = (1 - m) A + m B. Both are memoryless with ADAA1 (the same half-sample), so the
blend has no comb; what can happen is partial cancellation when the two outputs are not correlated
(a folder's fundamental changes sign, a rectifier doubles the pitch). Measured on the sung 'a' at drive 24 dB:
correlation of A and B, level of the blend at m = 0.25 / 0.5 / 0.75 re the ends, linear and with the
correlation-aware gain 1 / sqrt((1-m)^2 + m^2 + 2 m (1-m) rho) (gfx-acoustic's FIR-blend rule).
"""
import numpy as np
from common import voice, shape, rms_db, rms

x = voice(187.3, 0.6, "a", -18.0, jitter=0.01)
D = 24.0
out = {}
for n in ("soft", "hard", "tube", "fold", "rectify"):
    y = shape(x, n, D, 2, True)
    out[n] = y / rms(y[2000:]) * rms(x[2000:])  # level-matched (the voice table, exactly)
for a, b in (("soft", "hard"), ("soft", "tube"), ("hard", "fold"), ("soft", "fold"), ("tube", "rectify"), ("soft", "rectify")):
    A, B = out[a][2000:], out[b][2000:]
    rho = np.dot(A, B) / np.sqrt(np.dot(A, A) * np.dot(B, B))
    lin = []
    cor = []
    for m in (0.25, 0.5, 0.75):
        y = (1 - m) * A + m * B
        lin.append(rms_db(y) - rms_db(A))
        k = 1 / np.sqrt((1 - m) ** 2 + m * m + 2 * m * (1 - m) * rho)
        cor.append(rms_db(y * k) - rms_db(A))
    print(f"{a:5s} -> {b:7s}: rho {rho:+.2f} | linear blend at 25/50/75 %: " + " ".join(f"{v:+5.1f}" for v in lin)
          + " dB | with the rho gain: " + " ".join(f"{v:+5.2f}" for v in cor) + " dB")
