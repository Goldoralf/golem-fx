# SPDX-License-Identifier: GPL-3.0-or-later
"""The legacy block's algorithm re-derived from its published formulas (Robbert van der Helm's Spectral
Compressor, nih-plug, GPL-3; read for formulas only, written here from scratch) for comparison:
- Hann / Hann STFT, overlap 16 (the plugin default; golem never sets it), bins scaled by sqrt(gc),
  gc = 1 / ((ov / 4) * 1.5) / N; per-bin magnitude |X_k| (not power, no band);
- linear-magnitude one-pole follower at the hop rate, attack when rising;
- threshold T_k = thresh + (slope - 3) * ln(f_k / center)   (natural log: 'dB/oct' is per neper);
- ratios with hi-freq rolloff: 1/r_k = (1/r)(1 - t) + t, t = rolloff * ln(f_k) / ln(22050)
  (defaults: upward 0.75, downward 0); knee 6 dB both (Giannoulis), upward only above 20 Hz.
Returns per-frame gains (dB) and the frame grid."""
import numpy as np
import sc

def ref_gains(x, fs, thresh, down, up, center, slope, attack, release, N, ov=16, knee=6.0, up_roll=0.75, dn_roll=0.0):
    H = N // ov
    w = sc.hann(N)
    gc = 1 / ((ov / 4) * 1.5) / N
    xp = np.concatenate([np.zeros(N - H), x])
    M = len(x) // H
    f = np.arange(N // 2 + 1) * fs / N
    lnf = np.log(np.maximum(f, 1e-9))
    T = thresh + (slope - 3) * (lnf - np.log(center))
    def r_of(r, roll):
        t = roll * lnf / np.log(22050.0)
        return 1 / ((1 / r) * (1 - t) + t)
    rd = r_of(down, dn_roll); ru = r_of(up, up_roll)
    es = H / fs
    aa = np.exp(-1 / (attack / 1e3 / es)); ar = np.exp(-1 / (release / 1e3 / es))
    env = np.full(len(f), np.sqrt(0.5) / 8)
    G = np.empty((M, len(f)))
    for m in range(M):
        X = np.fft.rfft(xp[m * H:m * H + N] * w * np.sqrt(gc))
        mag = np.abs(X)
        env = np.where(env > mag, ar * env + (1 - ar) * mag, aa * env + (1 - aa) * mag)
        L = 20 * np.log10(np.maximum(env, 1e-5))
        over = L - T
        gd = -(1 - 1 / rd) * sc.soft(over, knee)
        gu = (1 - 1 / ru) * sc.soft(-over, knee)
        gu = np.where((f > 20) & (L > -100), gu, 0)
        G[m] = gd + gu
    return f, G, H
