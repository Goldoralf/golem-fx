# SPDX-License-Identifier: GPL-3.0-or-later
"""Review r3: do the vocal make-up tables hold on a real take? (README §2.5)
The tables are measured on a steady synthetic 'a' at -18 dBFS RMS (crest 8 dB). A real sung phrase has
dynamics: here it is set to -18 dBFS RMS over its active parts and run through Soft / Hard / Fold at
drive 0..48 dB with the vocal table and with gfx-grit's sine table. Output re input, RMS over the active
parts, and the loudest / quietest quarter of the 20 ms frames apart.
Needs GFX_MORPH_VOICE; without it, the synthetic phrase at three levels shows the same law."""
import sys
import numpy as np
sys.path.insert(0, "review")
import rvoice
from common import FS, voice, shape, rms_db

DR = [0, 6, 12, 18, 24, 30, 36, 42, 48]
sine = 10 ** (-18 / 20) * np.sin(2 * np.pi * 1000 * np.arange(int(0.3 * FS)) / FS)
vref = voice(187.3, 0.5, "a", -18.0)


def table(ref, name):
    return [rms_db(ref[2000:]) - rms_db(shape(ref, name, d, 2, True)[2000:]) for d in DR]


x = rvoice.load()
if x is None:
    print("GFX_MORPH_VOICE not set: synthetic vowel at -28 / -18 / -8 dBFS RMS")
    for name in ("soft", "hard"):
        tv = table(vref, name)
        for lvl in (-28, -18, -8):
            v = voice(187.3, 0.6, "a", lvl, 0.01)
            print(f"  {name} input {lvl}: " + " ".join(f"{rms_db(shape(v, name, d, 2, True)[2000:]) + m - lvl:+5.1f}" for d, m in zip(DR, tv)))
    sys.exit()
m, l = rvoice.active(x)
x = x / np.sqrt(np.mean(x[m] ** 2)) * 10 ** (-18 / 20)
la = 10 * np.log10(np.convolve(x * x, np.ones(960) / 960, mode="same") + 1e-20)
q1, q3 = np.percentile(la[m], [25, 75])
loud, quiet = m & (la >= q3), m & (la <= q1)
print(f"real take: active RMS -18.0 dBFS, peak {20*np.log10(np.abs(x).max()):+.1f} dBFS, 20 ms level quartiles {q1:.1f} / {q3:.1f} dBFS")
print("drive dB:                  " + " ".join(f"{d:5d}" for d in DR))
for name in ("soft", "hard", "fold"):
    for tname, ref in (("grit ", sine), ("voice", vref)):
        t = table(ref, name)
        rows = {"all": [], "loud": [], "quiet": []}
        for d, mk in zip(DR, t):
            y = shape(x, name, d, 2, True) * 10 ** (mk / 20)
            rows["all"].append(rms_db(y[m]) - rms_db(x[m]))
            rows["loud"].append(rms_db(y[loud]) - rms_db(x[loud]))
            rows["quiet"].append(rms_db(y[quiet]) - rms_db(x[quiet]))
        for k, r in rows.items():
            print(f"  {name:5s} table {tname} {k:5s}: " + " ".join(f"{v:+5.1f}" for v in r) + f"   spread {max(r)-min(r):4.1f}")
