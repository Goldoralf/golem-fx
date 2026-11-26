"""Octave Up: the cut against the monolithic solve without winding capacitance (what the cut drops
then is only the rectifier's current reflected into the primary)."""
import numpy as np
from octave import octave, XF
from octcut import stage2
fs = 176400.0
for f0, amp in ((440.0, 0.03), (440.0, 0.1), (440.0, 0.3), (1318.5, 0.1), (82.4, 0.3)):
    n = int(0.06 * fs); t = np.arange(int(0.12 * fs) + n) / fs; u = amp * np.sin(2 * np.pi * f0 * t)
    y0 = octave(fs, fuzz=0.7, xf=dict(XF, cw=0.0)).run(u, 1, ['out'])[0][:, 0]
    ys = octave(fs, fuzz=0.7, stage1=True).run(u, 1, ['w1'])[0][:, 0]
    yc, _ = stage2(ys, fs)
    e = y0[-n:] - yc[-n:]; r = y0[-n:] - y0[-n:].mean()
    print('%6.0f Hz %.2f V: cut vs monolithic (Cw 0) %.1f dB re output RMS' % (f0, amp, 10 * np.log10((e ** 2).mean() / (r ** 2).mean())), flush=True)
