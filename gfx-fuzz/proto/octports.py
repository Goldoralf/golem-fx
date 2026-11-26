"""Octave Up: which junctions ever conduct (a reverse junction that never does could be dropped: one port fewer)."""
import numpy as np
from octave import octave
fs = 352800.0
for amp, f0 in ((1.0, 82.4), (1.0, 1204.0), (0.25, 1204.0)):
    c = octave(fs, fuzz=1.0, stage1=True); t = np.arange(int(0.08 * fs)) / fs
    y, its, bad = c.run(amp * np.sin(2 * np.pi * f0 * t), 1, ['c1', 'b1', 'e1', 'e2', 'c3', 'e3'])
    c1, b1, e1, e2, c3, e3 = y.T
    print('%.2f V %4.0f Hz: max forward v: Q1 BE %.2f BC %.2f | Q2 BE %.2f BC %.2f | Q3 BE %.2f BC %.2f' % (
        amp, f0, (e1 - b1).max(), (c1 - b1).max(), (c1 - e2).max(), (c1 - 9.0).max(), (e2 - e3).max(), (e2 - c3).max()), flush=True)
