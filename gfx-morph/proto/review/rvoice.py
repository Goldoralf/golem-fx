# SPDX-License-Identifier: GPL-3.0-or-later
"""Review helper: a real voice take for the checks that a synthetic vowel cannot answer.
Set GFX_MORPH_VOICE to a mono or stereo WAV (the maintainer's sung phrase); returns None when unset."""
import os
import numpy as np
from scipy.io import wavfile
from scipy import signal


def load(fs_out=48000, env="GFX_MORPH_VOICE"):
    p = os.environ.get(env)
    if not p or not os.path.exists(p):
        return None
    fs, x = wavfile.read(p)
    x = x.astype(float)
    if x.ndim > 1:
        x = x.mean(axis=1)
    x /= (np.abs(x).max() + 1e-12)
    if fs != fs_out:
        g = np.gcd(int(fs), int(fs_out))
        x = signal.resample_poly(x, fs_out // g, fs // g)
    return x


def active(x, fs=48000, win_ms=20.0, floor_db=-35.0):
    """Mask of the samples whose 20 ms RMS is within floor_db of the take's loudest 20 ms."""
    n = int(win_ms * 1e-3 * fs)
    p = np.convolve(x * x, np.ones(n) / n, mode="same")
    l = 10 * np.log10(p + 1e-20)
    return l > l.max() + floor_db, l
