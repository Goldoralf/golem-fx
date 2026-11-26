"""Octave Up: worst alias (alias.py's method: -12 dBFS sine, `fuzz` 100 %, buffer on, 20 Hz - 20 kHz
re the fundamental; also re the strongest harmonic) at 4x / 8x / 16x of 44.1 kHz, and Newton counts."""
import sys
import numpy as np
from scipy.signal.windows import blackmanharris
from octave import octave
GUARD = float(sys.argv[4]) if len(sys.argv) > 4 else 8 * 6.67
LO = float(sys.argv[1]) if len(sys.argv) > 1 else 60.0
FACT = [int(a) for a in sys.argv[2].split(",")] if len(sys.argv) > 2 else [4, 8, 16]
F0 = [float(a) for a in sys.argv[3].split(",")] if len(sys.argv) > 3 else [1204.0, 5004.0]
for f0 in F0:
    for m in FACT:
        fs = 44100.0 * m
        n = int(0.15 * fs); t = np.arange(int(0.25 * fs) + n) / fs
        c = octave(fs, fuzz=1.0)
        y, its, bad = c.run(0.25 * np.sin(2 * np.pi * f0 * t), 1, ['out'])
        x = y[-n:, 0] - y[-n:, 0].mean(); Y = abs(np.fft.rfft(x * blackmanharris(n))); fr = np.arange(len(Y)) * fs / n
        harms = [Y[int(round(k * f0 * n / fs)) - 3:int(round(k * f0 * n / fs)) + 4].max() for k in range(1, int(20000 / f0) + 1)]
        fund = harms[0]; top = max(harms)
        mask = (fr >= LO) & (fr <= 20000)
        for k in range(0, int(20000 / f0) + 2): mask &= abs(fr - k * f0) > GUARD
        al = Y[mask]; i = np.argmax(al)
        print('f0 %.0f Hz %2dx: worst alias %.1f dB re H1 (%.1f dB re strongest harmonic H%d) at %.0f Hz; H2-H1 %+.1f dB; iters mean %.2f max %d >8 %.2f%% fallbacks %d unsolved %d' % (
            f0, m, 20 * np.log10(al[i] / fund), 20 * np.log10(al[i] / top), 1 + int(np.argmax(harms)), fr[mask][i],
            20 * np.log10(harms[1] / fund) if len(harms) > 1 else 0, its.mean(), its.max(), 100 * (its > 8).mean(), c.nfall, bad.sum()), flush=True)
