# SPDX-License-Identifier: GPL-3.0-or-later
"""Visual: CIELAB (D65) delta-E 76 of candidate body hues against every metal hue in visuals/js/golem-pedals.js."""
import numpy as np

USED = {"dynamics": "#4a5a6e", "drive": "#e04a00", "utility": "#f0a500", "filter": "#00a8c0", "eq": "#0058e0",
        "modulation": "#00a84f", "delay": "#3a28f0", "reverb": "#6a1fd1", "pitch": "#c4148c", "lofi": "#e8336f",
        "preamp": "#b316c0", "cab": "#00a898", "gfx-shimmer": "#8a3ff0", "gfx-gapmeter": "#6b7480",
        "gfx-glitch": "#72c000", "gfx-ringmod": "#c9b000", "gfx-tape": "#d8c8a0", "gfx-widener": "#c0e0f8",
        "gfx-fuzz": "#b0602a", "gfx-tuner": "#126e1c", "gfx-correct": "#7a1450", "gfx-acoustic": "#552210",
        "gfx-radio": "#4b5320", "gfx-stretch": "#a0a8ff", "gfx-polysynth": "#ff9ecf", "gfx-swell (proposed)": "#bf1dd7"}
CAND = {"oxblood": "#7a0f1f", "rust": "#a5381c", "bruise violet": "#4b2a5e", "gunmetal": "#2e3338",
        "acid chartreuse": "#b8e000", "hazard orange": "#ff6a00", "toxic teal": "#00786e", "bone": "#e8e0cc",
        "nicotine": "#b8962e", "deep crimson": "#9a0d2a", "electric blue": "#0090ff", "sulfur": "#e0d000",
        "blood orange": "#d23a1e", "petrol": "#1d4e5f", "black cherry": "#4a0f22"}


def lab(h):
    rgb = np.array([int(h[i:i + 2], 16) for i in (1, 3, 5)]) / 255
    rgb = np.where(rgb > 0.04045, ((rgb + 0.055) / 1.055) ** 2.4, rgb / 12.92)
    M = np.array([[0.4124, 0.3576, 0.1805], [0.2126, 0.7152, 0.0722], [0.0193, 0.1192, 0.9505]])
    xyz = M @ rgb / np.array([0.95047, 1.0, 1.08883])
    f = np.where(xyz > (6 / 29) ** 3, np.cbrt(xyz), xyz / (3 * (6 / 29) ** 2) + 4 / 29)
    return np.array([116 * f[1] - 16, 500 * (f[0] - f[1]), 200 * (f[1] - f[2])])


rows = []
for n, h in CAND.items():
    d = sorted((np.linalg.norm(lab(h) - lab(u)), k) for k, u in USED.items())
    rows.append((d[0][0], n, h, d[:3]))
for dmin, n, h, d in sorted(rows, reverse=True):
    print(f"{n:16s} {h}: L* {lab(h)[0]:4.0f} nearest " + ", ".join(f"{k} {v:.0f}" for v, k in d))
