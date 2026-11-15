# SPDX-License-Identifier: GPL-3.0-or-later
"""Review r5: group delay of the chain at the README's own defaults (§2.10, §3).
p1's "default chain" is pre HP 80 Hz + LP 12 kHz + the 2x pair. The defaults are `pre_hp` 100 Hz, `pre_lp`
off, and `defizz` 50 %, whose LR4 band gain at 4.5 kHz is an all-pass in the path at every gain.
Rows: the 2x / 4x pair alone, + pre HP 100, + the de-fizz all-pass, Two (250 Hz), Three (250 / 3,500 Hz,
the defaults; p1 used 3,000), and the first sample that carries 1 % / 10 % / 50 % of the impulse's energy."""
import numpy as np
from scipy import signal
from common import FS, up, down, lr4, bw2

ff = np.linspace(20, 20000, 16000)
PTS = (100, 150, 300, 1000, 3000, 4500, 8000)


def gd(y):
    ph = np.unwrap(np.angle(signal.freqz(y, worN=ff, fs=FS)[1]))
    g = -np.gradient(ph, 2 * np.pi * ff / FS) / FS * 1e3
    return [g[np.argmin(np.abs(ff - f))] for f in PTS]


def ap(x, fc):
    lo, hi = lr4(x, fc)
    return lo + hi


def row(lab, y):
    e = np.cumsum(y ** 2) / np.sum(y ** 2)
    first = np.argmax(np.abs(y) > 1e-9)
    print(f"{lab:46s} | " + " ".join(f"{v:6.3f}" for v in gd(y)) + f" | first {first}, |y[0]| {abs(y[0]):.4f}, energy 1/10/50 % at sample {np.argmax(e > .01)}/{np.argmax(e > .1)}/{np.argmax(e > .5)}")


x = np.zeros(1 << 15)
x[0] = 1
print("group delay ms at " + " ".join(f"{f}" for f in PTS) + " Hz")
row("2x pair", down(up(x, 2), 2))
row("4x pair", down(up(x, 4), 4))
hp = signal.lfilter(*bw2(100, "high"), x)
row("pre HP 100 + 2x", down(up(hp, 2), 2))
d = ap(hp, 4500)
row("DEFAULT: HP 100 + de-fizz AP 4.5k + 2x", down(up(d, 2), 2))
row("DEFAULT at quality on (4x)", down(up(d, 4), 4))
row("Two: default + AP 250", down(up(ap(d, 250), 2), 2))
row("Three: default + AP 250 + AP 3.5k", down(up(ap(ap(d, 250), 3500), 2), 2))
row("Three, de-fizz 0 (no AP 4.5k), HP off", down(up(ap(ap(x, 250), 3500), 2), 2))
# with the review's shelf (identity at rest) the de-fizz adds nothing on a vowel:
row("REVIEW default: HP 100 + 2x (shelf de-fizz)", down(up(hp, 2), 2))
row("REVIEW default at quality on (4x)", down(up(hp, 4), 4))
row("REVIEW Two: HP 100 + AP 250 + 2x", down(up(ap(hp, 250), 2), 2))
row("REVIEW Three: HP 100 + AP 250 + AP 3.5k + 2x", down(up(ap(ap(hp, 250), 3500), 2), 2))
# phase at the chest: the comb against bone conduction depends on the phase, not on the group delay
for lab, y in (("DEFAULT (draft)", down(up(d, 2), 2)), ("REVIEW default", down(up(hp, 2), 2)), ("2x pair alone", down(up(x, 2), 2)), ("REVIEW Three", down(up(ap(ap(hp, 250), 3500), 2), 2))):
    H = signal.freqz(y, worN=[100, 150, 250, 500, 1000, 2000, 4000, 8000], fs=FS)[1]
    print(f"{lab:16s}: phase deg at 100/150/250/500/1k/2k/4k/8k Hz: " + " ".join(f"{np.degrees(np.angle(h)):+7.1f}" for h in H))
