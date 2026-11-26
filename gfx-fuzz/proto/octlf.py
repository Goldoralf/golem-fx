"""Octave Up: the slow motion under a steady tone (C 33 uF to the winding + Lm: a series LC near 28 Hz)."""
import sys
import numpy as np
from octave import octave, XF
fs = 176400.0
for lm, cc in ((1.0, 'stock'), (3.0, 'stock'), (0.3, 'stock')):
    c = octave(fs, fuzz=1.0, xf=dict(XF, lm=lm))
    t = np.arange(int(0.8 * fs)) / fs
    y, its, bad = c.run(0.25 * np.sin(2 * np.pi * 1204.0 * t), 1, ['out', 'w'])
    win = int(0.01 * fs); k = len(t) // win
    duty = [(y[i * win:(i + 1) * win, 1] > 0).mean() for i in range(k)]
    mo = [y[i * win:(i + 1) * win, 0].mean() for i in range(k)]
    print('Lm %.1f H (LC %.0f Hz): duty per 10 ms, 0.4-0.8 s: %s' % (lm, 1 / (2 * np.pi * np.sqrt(lm * 33e-6)), ' '.join('%.2f' % d for d in duty[40:80])))
    print('   out mean: %s' % ' '.join('%.2f' % d for d in mo[40:80]), flush=True)
