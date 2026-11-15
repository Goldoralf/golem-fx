# SPDX-License-Identifier: GPL-3.0-or-later
"""Review r4: aliasing on higher voices and on Fold (README §2.4).
p2 uses one pitch (187.3 Hz). A scream or a soprano sits at 400 - 900 Hz: fewer harmonics mask an alias,
and the source has more level above 4 kHz. Same metric as p2 (inharmonic re harmonic, below 20 kHz).
Also: a memoryless curve on a periodic input has a periodic output, so every inharmonic dB is an alias,
at 32x too; Fold with the drive law halved (0 - 24 dB)."""
import numpy as np
from common import voice, shape, harmonic_split, FS

print("f0 Hz | type drive in | 2x+ADAA | 4x+ADAA | 32x")
for f0 in (187.3, 293.7, 523.9, 881.3):
    x = voice(f0, 1.2, "a", -18.0)[int(0.2 * FS):]
    for name, d, lvl in (("soft", 36, -18), ("soft", 48, -18), ("soft", 48, -6), ("hard", 48, -6), ("fold", 24, -18), ("fold", 24, -6), ("fold", 48, -18)):
        s = x * 10 ** ((lvl + 18) / 20)
        v = [harmonic_split(shape(s, name, d, f, a)[2048:], f0) for f, a in ((2, True), (4, True), (32, False))]
        print(f"{f0:6.1f} | {name:5s} {d:2d} dB {lvl:+3d} dBFS | {v[0]:6.1f} | {v[1]:6.1f} | {v[2]:6.1f}")
