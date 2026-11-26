"""Octave Up: where the aliases come from. 1204 Hz, -12 dBFS, `fuzz` 100 %, buffer on.
(a) stage 1 alone (the winding voltage) at 4x and 8x; (b) stage 1 at 4x, the rectifier (stage 2,
memoryless) at 8x after a 2x interpolation, then back to the host rate's band."""
import numpy as np
from scipy.signal import resample_poly
from scipy.signal.windows import blackmanharris
from octave import octave
from octcut import stage2

f0 = 1204.0; GUARD = 150.0


def worst(x, fs, f0):
    n = len(x); x = x - x.mean(); Y = abs(np.fft.rfft(x * blackmanharris(n))); fr = np.arange(len(Y)) * fs / n
    hk = [Y[int(round(k * f0 * n / fs)) - 3:int(round(k * f0 * n / fs)) + 4].max() for k in range(1, int(20000 / f0) + 1)]
    m = (fr >= 150) & (fr <= 20000)
    for k in range(0, int(20000 / f0) + 2): m &= abs(fr - k * f0) > GUARD
    al = Y[m]; i = np.argmax(al)
    return 20 * np.log10(al[i] / hk[0]), fr[m][i]


for mult in (4, 8):
    fs = 44100.0 * mult; n = int(0.15 * fs); t = np.arange(int(0.25 * fs) + n) / fs
    s = octave(fs, fuzz=1.0, stage1=True); y, its, bad = s.run(0.25 * np.sin(2 * np.pi * f0 * t), 1, ['w1'])
    w = y[:, 0]
    a, fa = worst(w[-n:], fs, f0)
    print('%dx: stage 1 (winding) worst alias %.1f dB re H1 at %.0f Hz; stage-1 iterations mean %.2f max %d' % (mult, a, fa, its.mean(), its.max()), flush=True)
    o, i2 = stage2(w, fs); a, fa = worst(o[-n:], fs, f0)
    print('%dx: stage 2 at the same rate: worst alias %.1f dB at %.0f Hz; stage-2 iterations %.2f' % (mult, a, fa, i2.mean()), flush=True)
    if mult == 4:
        w2 = resample_poly(w, 2, 1)
        o2, _ = stage2(w2, 2 * fs); o = resample_poly(o2, 1, 2)
        a, fa = worst(o[-n:], fs, f0)
        print('4x stage 1, rectifier at 8x (2x interpolated): worst alias %.1f dB at %.0f Hz' % (a, fa), flush=True)
