"""Octave Up: the cut, by spectrum. Harmonic levels (H1..H4, dBV) of: the monolithic solve, the same
with Cw = 0, and the cut (stage 1 + stage 2)."""
import numpy as np
from octave import octave, XF
from octcut import stage2
from octrun import harm, db
fs = 176400.0
for f0, amp in ((440.0, 0.1), (440.0, 0.3), (1318.5, 0.1)):
    n = int(round(round(0.06 * f0) * fs / f0)); t = np.arange(int(0.12 * fs) + n) / fs; u = amp * np.sin(2 * np.pi * f0 * t)
    ym = octave(fs, fuzz=0.7).run(u, 1, ['out'])[0][:, 0]
    y0 = octave(fs, fuzz=0.7, xf=dict(XF, cw=0.0)).run(u, 1, ['out'])[0][:, 0]
    ys = octave(fs, fuzz=0.7, stage1=True).run(u, 1, ['w1'])[0][:, 0]
    yc, _ = stage2(ys, fs)
    def nul(a, b): e = a[-n:] - b[-n:]; r = a[-n:] - a[-n:].mean(); return 20 * np.log10(np.sqrt((e ** 2).mean() / (r ** 2).mean()))
    for name, y in (('monolithic', ym), ('Cw = 0', y0), ('cut', yc)):
        h = harm(y, f0, fs, n)
        print('%6.0f Hz %.2f V %-10s H1..H4 %s dBV; null vs monolithic %s' % (f0, amp, name, ' '.join('%6.1f' % db(a) for a in h),
              '' if y is ym else '%.1f dB' % nul(ym, y)), flush=True)
