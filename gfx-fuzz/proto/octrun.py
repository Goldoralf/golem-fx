"""Octave Up: the octave (2f vs f), its dependence on the note, the level and `fuzz`; Newton counts.
usage: octrun.py [fs]"""
import sys, time
import numpy as np
from octave import octave, XF


def harm(y, f0, fs, n):
    x = y[-n:] - y[-n:].mean()
    w = np.hanning(n); Y = np.abs(np.fft.rfft(x * w)) * 2 / w.sum()
    k0 = f0 * n / fs
    return [Y[int(round(h * k0)) - 2:int(round(h * k0)) + 3].max() for h in (1, 2, 3, 4)]


def db(a): return 20 * np.log10(max(a, 1e-12))


def run(fs, f0, amp, fuzz=1.0, settle=0.08, xf=XF, **kw):
    c = octave(fs, fuzz=fuzz, xf=xf, **kw)
    n = int(round(0.05 * fs / (fs / f0))) * int(round(fs / f0)) or int(0.05 * fs)
    n = int(round(round(0.06 * f0) * fs / f0))
    t = np.arange(int(settle * fs) + n) / fs
    t0 = time.time()
    y, its, bad = c.run(amp * np.sin(2 * np.pi * f0 * t), 1, ['out', 'w', 'c3'])
    dt = time.time() - t0
    ho = harm(y[:, 0], f0, fs, n); hw = harm(y[:, 1], f0, fs, n)
    return ho, hw, its, bad, c.nfall, dt, y[-n:]


if __name__ == '__main__':
    fs = float(sys.argv[1]) if len(sys.argv) > 1 else 176400.0
    print('fs %.0f; out = rectified (500k volume full up); w = the driven 600 Ohm winding' % fs)
    for fuzz in (0.7, 1.0):
        for f0, amp in ((440.0, 0.3), (110.0, 0.3), (220.0, 0.3), (880.0, 0.3), (440.0, 0.03), (440.0, 0.1), (1320.0, 0.1)):
            ho, hw, its, bad, nf, dt, _ = run(fs, f0, amp, fuzz)
            print('fuzz %3.0f%% %6.0f Hz %.2f V: out H1 %6.1f H2 %6.1f H3 %6.1f dBV (H2-H1 %+5.1f dB) | winding H1 %6.1f H2 %6.1f | iters mean %.2f max %d >8 %.2f%% fallbacks %d unsolved %d (%.0f us/smp)' % (
                100 * fuzz, f0, amp, db(ho[0]), db(ho[1]), db(ho[2]), db(ho[1]) - db(ho[0]), db(hw[0]), db(hw[1]),
                its.mean(), its.max(), 100 * (its > 8).mean(), nf, bad.sum(), dt / len(its) * 1e6), flush=True)
