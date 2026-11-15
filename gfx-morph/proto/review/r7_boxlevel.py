# SPDX-License-Identifier: GPL-3.0-or-later
"""Review r7: a computed level constant for the box instead of a table per object and size.
A unity-peak band-pass of quality Q has  integral |H|^2 d ln f = pi / (2 Q).  With pink weighting over
100 Hz - 8 kHz (L = ln 80), neglecting the overlap of modes:
    P ~ [ sum_m g_m^2 pi / (2 Q_m) w(f_m)  +  s^2 ln(min(lp, 8k) / max(hp, 100)) ] / L
w(f) = 1 inside the band, 0 outside. The constant is -10 log10 P, recomputed at the control tick (48
multiply-adds), so it follows size, decay, position and the object morph. Checked against the numerical
pink power of the full response (modes overlapping, skirt cross-terms) on every object and setting."""
import sys
import numpy as np
sys.path.insert(0, "review")
from objects import OBJECTS, bank
import p5_box_lib as P

L = np.log(8000 / 100)


def analytic(B):
    f, Q, g = B["f"], B["Q"], B["g"]
    w = (f >= 100) & (f <= 8000)
    p = np.sum(g[w] ** 2 * np.pi / (2 * Q[w]))
    lo, hi = max(B["hp"], 100.0), min(B["lp"], 8000.0)
    p += 10 ** (B["skirt"] / 10) * max(0.0, np.log(hi / lo))
    return -10 * np.log10(p / L)


worst = 0.0
print("object: residual pink level (dB re bypass) with the computed constant, at position 0/25/50/75/100 | decay 25/50/100/200/400 | size 25/50/100/200/400")
for name in OBJECTS[1:]:
    rows = []
    for key, vals in (("pos", (0, .25, .5, .75, 1)), ("decay", (.25, .5, 1, 2, 4)), ("size", (.25, .5, 1, 2, 4))):
        r = []
        for v in vals:
            B = bank(name, **{key: v})
            r.append(analytic(B) - P.pink_gain(P.response(B)))
        worst = max(worst, np.max(np.abs(r)))
        rows.append(" ".join(f"{x:+5.1f}" for x in r))
    print(f"  {name:13s}: " + " | ".join(rows))
print(f"worst |residual| {worst:.1f} dB (the table per object and size leaves up to 8.9 dB over position and 7.6 dB over decay: r6)")

# A build-time table instead: pink constant on a grid, linear interpolation; error at the grid's midpoints.
print("== table over position (9 points), log decay (5), log size (5): interpolation error at the midpoints, dB")
F = np.geomspace(20, 20000, 1024)
k_of = lambda name, **kw: P.pink_gain(P.response(bank(name, **kw), F), F)
tot = 0.0
for name in OBJECTS[1:]:
    errs = {}
    for key, grid in (("pos", np.linspace(0, 1, 9)), ("decay", np.geomspace(.25, 4, 5)), ("size", np.geomspace(.25, 4, 5))):
        k = np.array([k_of(name, **{key: v}) for v in grid])
        mids = (grid[:-1] + grid[1:]) / 2 if key == "pos" else np.sqrt(grid[:-1] * grid[1:])
        km = np.array([k_of(name, **{key: v}) for v in mids])
        errs[key] = np.max(np.abs(km - (k[:-1] + k[1:]) / 2))
    tot = max(tot, max(errs.values()))
    print(f"  {name:13s}: position {errs['pos']:.2f} | decay {errs['decay']:.2f} | size {errs['size']:.2f}")
print(f"worst {tot:.2f} dB")
