# SPDX-License-Identifier: GPL-3.0-or-later
"""Review copy of p5_box.py's helpers (response, pink normalisation, IR, TPT bank) as functions."""
import numpy as np
from scipy import signal

FS = 48000.0
FREQ = np.geomspace(20, 20000, 2048)


def bpf(f0, Q, fs=FS):
    w = 2 * np.pi * f0 / fs
    al = np.sin(w) / (2 * Q)
    a = np.array([1 + al, -2 * np.cos(w), 1 - al])
    return np.array([al, 0, -al]) / a[0], a / a[0]


def skirt_ba(B, fs=FS):
    return signal.butter(2, B["hp"], "high", fs=fs), signal.butter(2, B["lp"], "low", fs=fs)


def response(B, freq=FREQ, fs=FS):
    H = np.zeros(len(freq), complex)
    for f, Q, g in zip(B["f"], B["Q"], B["g"]):
        if g:
            H += g * signal.freqz(*bpf(f, Q, fs), worN=freq, fs=fs)[1]
    (bh, ah), (bl, al) = skirt_ba(B, fs)
    return H + 10 ** (B["skirt"] / 20) * signal.freqz(bh, ah, worN=freq, fs=fs)[1] * signal.freqz(bl, al, worN=freq, fs=fs)[1]


def pink_gain(H, freq=FREQ):
    band = (freq >= 100) & (freq <= 8000)
    return -10 * np.log10(np.mean(np.abs(H[band]) ** 2))


def ir(B, n, fs=FS):
    x = np.zeros(n)
    x[0] = 1
    y = np.zeros(n)
    for f, Q, g in zip(B["f"], B["Q"], B["g"]):
        if g:
            y += g * signal.lfilter(*bpf(f, Q, fs), x)
    (bh, ah), (bl, al) = skirt_ba(B, fs)
    return y + 10 ** (B["skirt"] / 20) * signal.lfilter(bl, al, signal.lfilter(bh, ah, x))


def render(B, x, K_db=0.0, fs=FS):
    y = np.zeros(len(x))
    for f, Q, g in zip(B["f"], B["Q"], B["g"]):
        if g:
            y += g * signal.lfilter(*bpf(f, Q, fs), x)
    (bh, ah), (bl, al) = skirt_ba(B, fs)
    return (y + 10 ** (B["skirt"] / 20) * signal.lfilter(bl, al, signal.lfilter(bh, ah, x))) * 10 ** (K_db / 20)
