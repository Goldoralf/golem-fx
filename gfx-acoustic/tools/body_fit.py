#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""body-fit — gfx-acoustic's offline body tables (README §2.6).

Builds `src/bodies.rs` (mode tables, FIR correlations) and `data/bodies.f32` (the high-band FIR
kernels, 6 models × 3 mics × 1,024 taps, f32 little-endian, 48 kHz) from:

- the **Mores archive** (Zenodo 4604577, CC BY 4.0): guitar g34 (classical Spanish guitar, Madrid
  1971, measured in an anechoic room), hammer → bridge accelerometers (poles) and → three near-field
  mics (gains, residual FIR). Guitars g07, g30 and g62 are never used (the archive's Comments.pdf);
- the **IEM / DEGA guitar recordings** (Phaidra o:68022 and o:69273, Public Domain Mark 1.0): the
  steel-string's under-saddle piezo DI → the two C414s (bridge, neck), session 1 to fit, session 2 to
  test; the classical guitar's C414s for the Classical model's near-field tilt;
- the parametric voicings of README §1.3 / §2.6 for Dreadnought, Jumbo, Parlor and Round-back.

The raw data never enter the repository: `--fetch DIR` downloads them by URL and checks their
SHA-256, then the fit runs on that directory. `--parametric-only` builds every model from the
parametric voicings (the fallback when the data cannot be fetched).

    python3 gfx-acoustic/tools/body_fit.py --fetch /tmp/body-data
    python3 gfx-acoustic/tools/body_fit.py --data /tmp/body-data

Needs numpy and scipy. Deterministic (fixed seeds).
"""

import argparse
import hashlib
import io
import os
import sys
import urllib.request
import wave
import zipfile

import numpy as np
from scipy import signal as sg

FS = 48000
FIR_LEN = 1024
FADE = (int(0.015 * FS), int(0.021 * FS))  # half-Hann fade 15 → 21 ms
XOVER = 1000.0  # modal band / FIR band crossover (LR4)
MAX_MODES = 48
SM = 1 / 12.0  # the Concert magnitude: 1/12-octave smoothing
MODELS = ["Dreadnought", "Jumbo", "Concert", "Parlor", "Classical", "Round-back"]
MICS = ["Bridge", "Sound hole", "Neck"]

# D(s) = (s + w0) / (s + ws): force → the sound-hole EMF reference (README §2.7)
C_STRING = 2 * 0.648 * 166.0  # m/s, the six open strings' geometric mean (README §2.2)
X_S = 0.200
W_S = C_STRING / (np.sqrt(2) * X_S)
W_0 = 2 * np.pi * 30.0

SOURCES = {
    "mores": ("https://zenodo.org/api/records/4604577/files/qualified_selected_impulses.mat/content",
              "qualified_selected_impulses.mat",
              "8a016fab0b3f086a49b2e740c2588fc61938fd09251aab5314413ea4bab72fb6"),
    "iem1": ("https://phaidra.kug.ac.at/api/object/o:68022/download", "o68022.zip",
             "dc18a55b3d418a273ef090b00fc3a04e8e9eb0954012a6c7d84b9c8f83b7119a"),
    "iem2": ("https://phaidra.kug.ac.at/api/object/o:69273/download", "o69273.zip",
             "75e4e0e433037094a1464f421046e4932bfe23271ad9c52af640a19949cabd3a"),
}
GUITAR = 34  # the archive's g34; never 7, 30, 62
EXCLUDED = (7, 30, 62)


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def fetch(dirname):
    os.makedirs(dirname, exist_ok=True)
    for key, (url, name, digest) in SOURCES.items():
        path = os.path.join(dirname, name)
        if not os.path.exists(path):
            print(f"fetching {url}", file=sys.stderr)
            req = urllib.request.Request(url, headers={"User-Agent": "body-fit"})
            with urllib.request.urlopen(req) as r, open(path, "wb") as f:
                while True:
                    b = r.read(1 << 20)
                    if not b:
                        break
                    f.write(b)
        got = sha256(path)
        if got != digest:
            sys.exit(f"{name}: SHA-256 {got} != {digest}")
    print("data present, SHA-256 checked", file=sys.stderr)


# ---------------------------------------------------------------------------------------------------
# helpers

def third_octaves(lo=50.0, hi=16000.0):
    c = 1000.0 * 2.0 ** (np.arange(-30, 30) / 3.0)
    return c[(c >= lo) & (c <= hi)]


def band_db(f, p, centres, width=1 / 3):
    """Mean power per fractional-octave band (dB)."""
    out = []
    for c in centres:
        m = (f >= c * 2 ** (-width / 2)) & (f < c * 2 ** (width / 2))
        out.append(10 * np.log10(np.mean(p[m]) + 1e-30) if m.any() else np.nan)
    return np.array(out)


def smooth_complex(f, h, width=1 / 6):
    out = h.copy()
    lf = np.log2(np.maximum(f, 1e-9))
    cs = np.concatenate([[0], np.cumsum(h)])
    lo = np.searchsorted(lf, lf - width / 2)
    hi = np.searchsorted(lf, lf + width / 2, side="right")
    n = np.maximum(hi - lo, 1)
    out = (cs[hi] - cs[lo]) / n
    out[0] = h[0]
    return out


def d_inv(f):
    s = 2j * np.pi * f
    return (s + W_S) / (s + W_0)


def read_wav(data):
    with wave.open(io.BytesIO(data)) as w:
        n, sw, fs = w.getnframes(), w.getsampwidth(), w.getframerate()
        raw = np.frombuffer(w.readframes(n), np.uint8)
    if sw == 3:
        b = raw.reshape(-1, 3).astype(np.int32)
        v = (b[:, 0] | (b[:, 1] << 8) | (b[:, 2] << 16))
        v = np.where(v >= 1 << 23, v - (1 << 24), v)
        x = v / float(1 << 23)
    elif sw == 2:
        x = np.frombuffer(raw.tobytes(), "<i2") / 32768.0
    else:
        x = np.frombuffer(raw.tobytes(), "<i4") / float(1 << 31)
    if fs != FS:
        g = np.gcd(FS, fs)
        x = sg.resample_poly(x, FS // g, fs // g)
    return x


# ---------------------------------------------------------------------------------------------------
# vector fitting (Gustavsen & Semlyen 1999), common poles, real coefficients

def vf_basis(s, poles):
    cols = []
    for a in poles:
        if a.imag == 0:
            cols.append(1 / (s - a))
        else:
            cols.append(1 / (s - a) + 1 / (s - a.conj()))
            cols.append(1j / (s - a) - 1j / (s - a.conj()))
    return np.array(cols).T


def vf_poles(s, H, poles, weight, iters=12):
    """Relocates `poles` (upper half-plane representatives) to fit all rows of H."""
    for _ in range(iters):
        P = vf_basis(s, poles)
        n = P.shape[1]
        rows_r, rows_b = [], []
        for k in range(H.shape[0]):
            W = weight[k][:, None]
            A = np.hstack([W * P, W * np.ones((len(s), 1)), -W * H[k][:, None] * P])
            b = weight[k] * H[k]
            A = np.vstack([A.real, A.imag])
            bb = np.concatenate([b.real, b.imag])
            q, r = np.linalg.qr(A)
            rows_r.append(r[n + 1:, n + 1:])
            rows_b.append((q.T @ bb)[n + 1:])
        Ar = np.vstack(rows_r)
        br = np.concatenate(rows_b)
        c, *_ = np.linalg.lstsq(Ar, br, rcond=None)
        # zeros of sigma(s) = 1 + sum c basis: eig(A - b c^T)
        A = np.zeros((n, n))
        bv = np.zeros(n)
        i = 0
        for a in poles:
            if a.imag == 0:
                A[i, i] = a.real
                bv[i] = 1
                i += 1
            else:
                A[i:i + 2, i:i + 2] = [[a.real, a.imag], [-a.imag, a.real]]
                bv[i:i + 2] = [2, 0]
                i += 2
        z = np.linalg.eigvals(A - np.outer(bv, c))
        z = np.where(z.real > 0, -z.real + 1j * z.imag, z)
        new = [p for p in z if p.imag > 1e-6]
        new += [complex(p.real, 0) for p in z if abs(p.imag) <= 1e-6]
        poles = np.array(new)
    return poles


def vf_residues(s, h, poles, weight):
    """Least-squares residues (complex, one per pole) and a constant, for one response."""
    P = vf_basis(s, poles)
    A = np.hstack([P, np.ones((len(s), 1))]) * weight[:, None]
    b = h * weight
    c, *_ = np.linalg.lstsq(np.vstack([A.real, A.imag]), np.concatenate([b.real, b.imag]), rcond=None)
    out, i = [], 0
    for a in poles:
        if a.imag == 0:
            out.append(c[i] + 0j)
            i += 1
        else:
            out.append(c[i] + 1j * c[i + 1])
            i += 2
    return np.array(out), c[-1]


def mode_of(pole, r):
    """(f, Q, a, b): r/(s-p) + r*/(s-p*) = a·(w0/Q)·s/D + b·w0²/D."""
    w0 = abs(pole)
    q = w0 / (-2 * pole.real)
    alpha = 2 * r.real
    beta = -2 * (r.real * pole.real + r.imag * pole.imag)
    return w0 / (2 * np.pi), q, alpha * q / w0, beta / w0 ** 2


def modal_response(f, modes, fs=FS):
    """The TPT SVF bank's response (bilinear, prewarped at each mode), modes = [(f, q, a, b)]."""
    h = np.zeros(len(f), complex)
    for fm, q, a, b in modes:
        g = np.tan(np.pi * min(fm, 0.49 * fs) / fs)
        om = np.tan(np.pi * np.minimum(f, 0.4999 * fs) / fs) / g
        s = 1j * om
        d = s * s + s / q + 1
        h += (a * s / q + b) / d
    return h


def fit_modes(f, Hpole, Hmics, lo=50.0, hi=1400.0, npairs=60, label=""):
    """Common poles from Hpole (rows), residues per row of Hmics. Returns modes per mic.

    Vector fitting over 45 Hz – 1.5 kHz (relative weights); the poles inside lo – hi kept with Q
    clamped to 2 – 300 (a 1 s measurement window cannot tell more), the MAX_MODES strongest by their
    peak contribution to the mics, and the mics' residues refitted on those alone (no constant term:
    the bank must stand without one)."""
    m = (f >= 45) & (f <= 1500)
    fs_ = f[m]
    s = 2j * np.pi * fs_
    init = 2 * np.pi * np.geomspace(50, 1450, npairs)
    poles = np.array([-w / 200 + 1j * w for w in init])
    w_pole = [1 / np.maximum(np.abs(smooth_complex(f, h, 1 / 3))[m], 1e-12) for h in Hpole]
    poles = vf_poles(s, Hpole[:, m], poles, w_pole)

    def clamp(p):
        w0 = abs(p)
        q = float(np.clip(w0 / (-2 * p.real), 2, 300))
        sig = w0 / (2 * q)
        return complex(-sig, np.sqrt(max(w0 * w0 - sig * sig, 0)))
    keep = [clamp(p) for p in poles if p.imag > 0 and lo <= abs(p) / (2 * np.pi) <= hi]
    keep = np.array(sorted(keep, key=lambda p: abs(p)))
    w_mic = [1 / np.maximum(np.abs(smooth_complex(f, h, 1 / 3))[m], 1e-12) for h in Hmics]

    def resid(k):
        return np.array([vf_residues_nod(s, h[m], k, w) for h, w in zip(Hmics, w_mic)])
    res = resid(keep)
    if len(keep) > MAX_MODES:
        score = np.sum(np.abs(res), axis=0) / np.array([-p.real for p in keep])
        idx = np.sort(np.argsort(score)[::-1][:MAX_MODES])
        keep = keep[idx]
        res = resid(keep)
    out = [[mode_of(p, r) for p, r in zip(keep, row)] for row in res]
    print(f"{label}: {len(poles)} poles relocated, {len(keep)} modes kept "
          f"({min(abs(keep)) / 2 / np.pi:.0f} – {max(abs(keep)) / 2 / np.pi:.0f} Hz)", file=sys.stderr)
    return out


def vf_residues_nod(s, h, poles, weight):
    """Least-squares residues with the poles fixed and no constant term."""
    P = vf_basis(s, poles) * weight[:, None]
    b = h * weight
    c, *_ = np.linalg.lstsq(np.vstack([P.real, P.imag]), np.concatenate([b.real, b.imag]), rcond=None)
    out, i = [], 0
    for a in poles:
        if a.imag == 0:
            out.append(c[i] + 0j)
            i += 1
        else:
            out.append(c[i] + 1j * c[i + 1])
            i += 2
    return np.array(out)


def residual_fir(f, Hmeas, modes, nfft):
    """The measured IR minus the modal one, LR4 high-passed at XOVER, faded 15 → 21 ms, 1,024 taps."""
    res = Hmeas - modal_response(f, modes)
    ir = np.fft.irfft(res, nfft)[:FS // 4]
    sos = sg.butter(2, XOVER, "high", fs=FS, output="sos")
    ir = sg.sosfilt(np.vstack([sos, sos]), ir)
    ir = ir[:FIR_LEN].copy()
    a, b = FADE
    ir[a:b] *= 0.5 + 0.5 * np.cos(np.pi * np.arange(b - a) / (b - a))
    ir[b:] = 0
    return ir


def fit_error(f, Hmeas, modes, fir, nfft, lo=70.0, hi=10000.0):
    """Body (modal + FIR) vs its data, 1/6-octave smoothed: worst |dB| at lo – hi."""
    hf = np.fft.rfft(fir, nfft)
    model = modal_response(f, modes) + hf
    c = 1000 * 2 ** (np.arange(-60, 60) / 6)
    c = c[(c >= lo) & (c <= hi)]
    a = band_db(f, np.abs(Hmeas) ** 2, c, 1 / 6)
    b = band_db(f, np.abs(model) ** 2, c, 1 / 6)
    d = b - a
    return np.nanmax(np.abs(d)), c[np.nanargmax(np.abs(d))]


# ---------------------------------------------------------------------------------------------------
# the measured models

def mores_responses(mat_path):
    from scipy.io import loadmat
    assert GUITAR not in EXCLUDED
    d = loadmat(mat_path)["qualified_selected_impulses"][GUITAR - 1]
    n = FS
    roll = 0.5 * np.cos(np.arange(n) * np.pi / n) + 0.5
    H = np.zeros((5, n // 2 + 1), complex)
    for hit in range(3):  # bass, centre, treble: the strings span the bridge
        seg = d[hit * n:(hit + 1) * n]
        ham = seg[:, 0]
        i0 = int(np.argmax(np.abs(ham)))
        w = np.zeros(n)
        w[max(0, i0 - 20):i0 + 60] = 1.0
        F = np.fft.rfft(ham * w)
        reg = 1e-3 * np.max(np.abs(F)) ** 2
        for ch in range(1, 6):
            x = seg[:, ch] * roll
            if ch >= 3:  # mics: the sound's travel removed (first arrival − 2 samples)
                fa = int(np.argmax(np.abs(x) > 0.1 * np.max(np.abs(x))))
                x = np.roll(x, -(fa - i0 - 2) if fa - i0 > 2 else 0)
            # the hammer at index i0: its delay removed with it
            X = np.fft.rfft(np.roll(x, -i0))
            Fh = np.fft.rfft(np.roll(ham * w, -i0))
            H[ch - 1] += X * Fh.conj() / (np.abs(Fh) ** 2 + reg) / 3
    f = np.fft.rfftfreq(n, 1 / FS)
    return f, H, n


def welch_h1(x, y, nfft=16384):
    f, sxy = sg.csd(x, y, fs=FS, nperseg=nfft, noverlap=3 * nfft // 4, window="hann")
    _, sxx = sg.welch(x, fs=FS, nperseg=nfft, noverlap=3 * nfft // 4, window="hann")
    _, syy = sg.welch(y, fs=FS, nperseg=nfft, noverlap=3 * nfft // 4, window="hann")
    h = sxy / np.maximum(sxx, 1e-30)
    coh = np.abs(sxy) ** 2 / np.maximum(sxx * syy, 1e-30)
    # csd conjugates the first argument: H1 = Sxy / Sxx with Sxy = E[X* Y]
    return f, h, coh, sxx, syy


def smooth_power(f, p, width):
    lf = np.log2(np.maximum(f, 1e-9))
    cs = np.concatenate([[0], np.cumsum(p)])
    lo = np.searchsorted(lf, lf - width / 2)
    hi = np.searchsorted(lf, lf + width / 2, side="right")
    out = (cs[hi] - cs[lo]) / np.maximum(hi - lo, 1)
    out[0] = p[0]
    return out


def minimum_phase(mag):
    """The minimum-phase response of a magnitude on an rfft grid (real cepstrum)."""
    n = 2 * (len(mag) - 1)
    lm = np.log(np.maximum(np.concatenate([mag, mag[-2:0:-1]]), 1e-12))
    cep = np.fft.ifft(lm).real
    cep[1:n // 2] *= 2
    cep[n // 2 + 1:] = 0
    return np.exp(np.fft.fft(cep))[:len(mag)]


def iem_tracks(zpath, takes, names):
    z = zipfile.ZipFile(zpath)
    out = {k: [] for k in names}
    for t in takes:
        for k in names:
            out[k].append(read_wav(z.read(f"Audio/{t}_{k}.wav")))
    n = {k: sum(len(v) for v in out[k]) for k in names}
    m = min(n.values())
    return {k: np.concatenate(out[k])[:m] for k in names}


def onset_shift(h, nfft):
    ir = np.fft.irfft(h, nfft)
    a = np.abs(ir[:int(0.01 * FS)])
    return max(int(np.argmax(a > 0.2 * a.max())) - 2, 0)


def concert_fit(iem1, iem2):
    takes1 = ["Flageolet_Taylor", "Jazzkadenz_Taylor", "Joyride_Taylor", "Kleiderschrank_Taylor",
              "Sommerhit1998_Taylor", "Sommerhit2016_Taylor"]
    tr = iem_tracks(iem1, takes1, ["DI", "Mic#1", "Mic#2", "Ref#1", "Ref#2"])
    nfft = 16384
    H, coh = {}, {}
    for k in ["Mic#1", "Mic#2", "Ref#1", "Ref#2"]:
        f, h1, coh[k], sxx, syy = welch_h1(tr["DI"], tr[k], nfft)
        # A piezo sums six strings, each through its own saddle path: one input explains the mic only
        # in part (median coherence ≈ 0.35, measured), so H1 is biased low wherever the coherence is
        # (by √coh in magnitude). The magnitude is the spectral ratio √(Syy / Sxx) = |H1| / √coh
        # (the long-term spectrum a mono input must produce), smoothed to 1/12 octave, its phase the
        # minimum phase: the bulk delay goes with it (README "As coded": deviation)
        mag = np.sqrt(smooth_power(f, syy, SM) / np.maximum(smooth_power(f, sxx, SM), 1e-30))
        H[k] = minimum_phase(mag)
    sh = onset_shift(np.fft.rfft(np.fft.irfft(h1, nfft)), nfft)
    low = (f > 80) & (f < 8000)
    print(f"Concert: coherence median {np.median(coh['Mic#1'][low]):.2f} / {np.median(coh['Mic#2'][low]):.2f}, "
          f"≥ 0.8 on {100 * np.mean(coh['Mic#1'][low] >= 0.8):.0f} % / "
          f"{100 * np.mean(coh['Mic#2'][low] >= 0.8):.0f} % of the bins at 80 Hz – 8 kHz", file=sys.stderr)
    # room: C414 vs NTI at the same spot, 1/3 octaves below 300 Hz, levels aligned at 300 Hz – 2 kHz
    # and the cardioid's proximity effect (a slope in dB / octave below 300 Hz) removed
    c = third_octaves(50, 2000)
    room = []
    for mic, ref in [("Mic#1", "Ref#1"), ("Mic#2", "Ref#2")]:
        a = band_db(f, np.abs(H[mic]) ** 2, c)
        b = band_db(f, np.abs(H[ref]) ** 2, c)
        mid = (c >= 300) & (c <= 2000)
        d = (a - b) - np.nanmean((a - b)[mid])
        lo = c < 300
        p = np.polyfit(np.log2(c[lo]), d[lo], 1)
        d[lo] -= np.polyval(p, np.log2(c[lo]))
        room += [cc for cc, dd in zip(c, d) if cc < 300 and abs(dd) > 3]
    room = sorted(set(room))
    print(f"Concert: 1/3-octave bands where the C414s and the NTIs disagree > 3 dB below 300 Hz (room): "
          f"{[round(x) for x in room]}", file=sys.stderr)
    Hb = H["Mic#1"]  # force-like: stored as measured; D⁻¹ runs in the plugin
    Hn = H["Mic#2"]
    modes = fit_modes(f, np.array([Hb, Hn]), np.array([Hb, Hn]), label="Concert")
    # modes inside a room band are dropped (README §2.6: kept only where the reference mics agree)
    def in_room(fm):
        return any(cc * 2 ** (-1 / 6) <= fm < cc * 2 ** (1 / 6) for cc in room)
    drop = [i for i, md in enumerate(modes[0]) if in_room(md[0])]
    modes = [[md for i, md in enumerate(row) if i not in drop] for row in modes]
    print(f"Concert: {len(drop)} modes dropped in room bands, {len(modes[0])} kept", file=sys.stderr)
    firs = [residual_fir(f, h, md, nfft) for h, md in zip([Hb, Hn], modes)]
    for name, h, md, fr in zip(["bridge", "neck"], [Hb, Hn], modes, firs):
        e, at = fit_error(f, h, md, fr, nfft)
        print(f"Concert {name}: body vs its data, worst {e:.1f} dB (at {at:.0f} Hz), 1/6 oct 70 Hz – 10 kHz", file=sys.stderr)
    # held out: session 2, piezo through the fit vs the C414s, long-term 1/3 octaves
    takes2 = ["MelancholiA_Taylor", "MelancholiB_Taylor"]
    t2 = iem_tracks(iem2, takes2, ["DI-Box-Western", "Bridge-C414", "Neck-C414"])
    check = []
    for name, key, md, fr in zip(["bridge", "neck"], ["Bridge-C414", "Neck-C414"], modes, firs):
        f2, sxx = sg.welch(t2["DI-Box-Western"], fs=FS, nperseg=nfft)
        _, syy = sg.welch(t2[key], fs=FS, nperseg=nfft)
        hm = modal_response(f2, md) + np.fft.rfft(fr, nfft)
        c = third_octaves(80, 8000)
        a = band_db(f2, syy, c)
        b = band_db(f2, sxx * np.abs(hm) ** 2, c)
        d = b - a
        d -= np.nanmean(d)  # session 2 was set up anew: levels aligned
        q = (c >= 1000) & (c <= 3000)
        check.append((name, np.nanmax(np.abs(d)), np.nanmax(np.abs(d[q]))))
        print(f"Concert {name}, held-out session 2: piezo through the fit vs the C414, 1/3 oct 80 Hz – 8 kHz: "
              f"worst {np.nanmax(np.abs(d)):.1f} dB (1 – 3 kHz: {np.nanmax(np.abs(d[q])):.1f} dB); per band "
              + " ".join(f"{x:+.1f}" for x in d), file=sys.stderr)
    return f, modes, firs, nfft, {"room": room, "held_out": check, "bulk": sh, "coh": float(np.median(coh["Mic#1"][low]))}


def classical_fit(mat_path, iem1):
    f, H, nfft = mores_responses(mat_path)
    acc = H[0:2]
    ue, tre, bas = H[2], H[3], H[4]
    Hb = 0.5 * (tre + bas)
    Hn = ue
    # near-field tilt (README §2.6): the model's long-term spectrum under real playing (the IEM
    # steel-string piezo as the force) against the IEM classical guitar's C414s, one slope
    tilt = 0.0
    if iem1:
        tr = iem_tracks(iem1, ["Jazzkadenz_Hanika"], ["Mic#1", "Mic#2"])
        tp = iem_tracks(iem1, ["Jazzkadenz_Taylor", "Sommerhit1998_Taylor", "Sommerhit2016_Taylor"], ["DI"])
        ff, s1 = sg.welch(tr["Mic#1"], fs=FS, nperseg=8192)
        _, s2 = sg.welch(tr["Mic#2"], fs=FS, nperseg=8192)
        _, sp = sg.welch(tp["DI"], fs=FS, nperseg=8192)
        hh = np.interp(ff, f, np.abs(0.5 * (Hb + Hn)) ** 2)
        c = third_octaves(100, 8000)
        d = band_db(ff, 0.5 * (s1 + s2), c) - band_db(ff, sp * hh, c)
        ok = np.isfinite(d)
        tilt = float(np.polyfit(np.log2(c[ok] / 1000), d[ok], 1)[0])
        print(f"Classical: near-field tilt {tilt:+.2f} dB / octave (pivot 1 kHz)", file=sys.stderr)
        tilt = float(np.clip(tilt, -3, 3))
    t = np.maximum(f, 20) / 1000.0
    mag = t ** (tilt / 6.0206)
    # minimum-phase version of the tilt (real cepstrum), applied to the mic responses
    n = 2 * (len(f) - 1)
    lm = np.log(np.concatenate([mag, mag[-2:0:-1]]))
    cep = np.fft.ifft(lm).real
    cep[1:n // 2] *= 2
    cep[n // 2 + 1:] = 0
    tmin = np.exp(np.fft.fft(cep))[:len(f)]
    Hb, Hn = Hb * tmin, Hn * tmin
    modes = fit_modes(f, acc, np.array([Hb, Hn]), label="Classical")
    firs = [residual_fir(f, h, md, nfft) for h, md in zip([Hb, Hn], modes)]
    for name, h, md, fr in zip(["bridge", "neck"], [Hb, Hn], modes, firs):
        e, at = fit_error(f, h, md, fr, nfft)
        print(f"Classical {name}: body vs its data, worst {e:.1f} dB (at {at:.0f} Hz), 1/6 oct 70 Hz – 10 kHz", file=sys.stderr)
    # T60 of the strongest low modes, from the fit
    return f, modes, firs, nfft, {"tilt": tilt}


# ---------------------------------------------------------------------------------------------------
# parametric voicings (README §1.3, §2.6)

# (f Hz, Q, dB at the sound hole) — D-28-class dreadnought: A0 102, T1 ≈ 198, seesaw 376 / 436
# (barely forward), Table 3.1's top modes 163 / 326 / 390 / 431 / 643 / 733 / 756, then a sparse
# fill to 1.2 kHz [U, voicing]
DREAD = [(102, 28, 0), (163, 20, -10), (198, 24, -3), (326, 26, -8), (376, 30, -15), (390, 26, -9),
         (436, 30, -14), (530, 28, -13), (643, 30, -10), (733, 34, -12), (756, 34, -12), (820, 32, -15),
         (890, 34, -14), (960, 36, -16), (1040, 36, -15), (1120, 38, -16), (1190, 38, -17)]
ROUND = [(110, 8, 0), (210, 10, -4), (330, 10, -10), (520, 12, -12), (780, 12, -14), (1000, 12, -16)]
CLASSICAL = [(98, 26, 0), (200, 22, -2), (240, 22, -5), (275, 24, -9), (550, 26, -8), (660, 28, -12),
             (820, 30, -14), (960, 32, -15), (1100, 34, -16)]
CONCERT = [(112, 26, 0), (180, 20, -8), (215, 24, -3), (350, 26, -8), (420, 30, -11), (470, 28, -10),
           (690, 30, -11), (800, 34, -13), (900, 34, -14), (1010, 36, -15), (1130, 38, -16)]

# relative body area (README §2.6: scaled by area [A]); frequencies × area^-1/2
SCALE = {"Jumbo": (1.19, 2.0), "Parlor": (0.70, -2.0)}


def mic_gains(fm, db):
    """Bridge / sound hole / neck gains of a parametric mode [U, voicing]: the bridge mic hears
    less of the air mode and more of the top; the neck mic the reverse."""
    g = 10 ** (db / 20)
    if fm < 130:
        m = (0.45, 1.0, 0.9)
    elif fm < 300:
        m = (0.8, 1.0, 0.85)
    else:
        m = (1.25, 1.0, 0.6)
    return [g * x for x in m]


def parametric_modes(table, fscale=1.0, a0_db=0.0):
    rows = [[], [], []]
    for fm, q, db in table:
        f = fm * fscale
        g = mic_gains(f, db + (a0_db if fm < 130 else 0.0))
        for i in range(3):
            rows[i].append((f, q, g[i], 0.0))
    return rows


def synthetic_fir(seed, t60_1k, t60_16k, tilt_db):
    """Exponentially decaying noise, T60 tilted with frequency, high-passed at XOVER (README §2.6)."""
    rng = np.random.default_rng(seed)
    n = FIR_LEN
    t = np.arange(n) / FS
    y = np.zeros(n)
    sos_b = None
    for k, fc in enumerate([1000, 2000, 4000, 8000, 16000]):
        t60 = np.exp(np.interp(np.log(fc), np.log([1000, 16000]), np.log([t60_1k, t60_16k])))
        lo, hi = fc / np.sqrt(2), min(fc * np.sqrt(2), 0.49 * FS)
        sos_b = sg.butter(2, [lo, hi], "band", fs=FS, output="sos")
        x = sg.sosfilt(sos_b, rng.standard_normal(n + 512))[512:]
        y += x * np.exp(-6.908 * t / t60) * 10 ** (tilt_db * k / 20)
    # the direct radiation of the bridge's impulse: the tail's largest sample comes first (README §5 onset row)
    y[0] += 2.0 * np.max(np.abs(y))
    sos = sg.butter(2, XOVER, "high", fs=FS, output="sos")
    y = sg.sosfilt(np.vstack([sos, sos]), y)
    a, b = FADE
    y[a:b] *= 0.5 + 0.5 * np.cos(np.pi * np.arange(b - a) / (b - a))
    y[b:] = 0
    return y


def band_level(fir, lo=1600, hi=4000, nfft=8192):
    h = np.fft.rfft(fir, nfft)
    f = np.fft.rfftfreq(nfft, 1 / FS)
    m = (f >= lo) & (f <= hi)
    return 10 * np.log10(np.mean(np.abs(h[m]) ** 2) + 1e-30)


def modal_peak(modes):
    f = np.geomspace(60, 1200, 2000)
    return 20 * np.log10(np.max(np.abs(modal_response(f, modes))) + 1e-30)


def parametric_model(name, fir_db_re_modes, seed):
    if name == "Round-back":
        modes = parametric_modes(ROUND)
        t60 = (0.03, 0.012)
    elif name == "Classical":
        modes = parametric_modes(CLASSICAL)
        t60 = (0.05, 0.015)
    elif name == "Concert":
        modes = parametric_modes(CONCERT)
        t60 = (0.06, 0.02)
    else:
        sc, a0 = SCALE.get(name, (1.0, 0.0))
        modes = parametric_modes(DREAD, sc ** -0.5, a0)
        t60 = (0.07, 0.02)
    firs = []
    for i, tilt in enumerate([0.5, -0.5, -2.0]):  # per octave band step: bridge brighter, neck darker
        fir = synthetic_fir(seed * 10 + i, t60[0], t60[1], tilt)
        target = modal_peak(modes[i]) + fir_db_re_modes
        fir *= 10 ** ((target - band_level(fir)) / 20)
        firs.append(fir)
    return modes, firs


# ---------------------------------------------------------------------------------------------------
# assembly

def with_hole(modes, firs):
    """Bridge, sound hole (the mean of the two: README §2.6), neck."""
    b, n = modes
    h = [(mb[0], mb[1], 0.5 * (mb[2] + mn[2]), 0.5 * (mb[3] + mn[3])) for mb, mn in zip(b, n)]
    return [b, h, n], [firs[0], 0.5 * (firs[0] + firs[1]), firs[1]]


def normalise(modes, firs, force_like):
    """Scales a body so the sound-hole response's mean power over 1/3 octaves 100 Hz – 8 kHz is 1."""
    nfft = 1 << 16
    f = np.fft.rfftfreq(nfft, 1 / FS)
    h = modal_response(f, modes[1]) + np.fft.rfft(firs[1], nfft)
    if force_like:
        h = h * d_inv(f)
    c = third_octaves(100, 8000)
    p = np.mean(10 ** (band_db(f, np.abs(h) ** 2, c) / 10))
    k = 1 / np.sqrt(p)
    modes = [[(fm, q, a * k, b * k) for fm, q, a, b in row] for row in modes]
    firs = [fr * k for fr in firs]
    # polarity: the sound hole's response in phase with its input at 80 Hz – 2 kHz (log-weighted)
    m = (f >= 80) & (f <= 2000)
    if np.sum((h.real / np.maximum(f, 1))[m]) < 0:
        modes = [[(fm, q, -a, -b) for fm, q, a, b in row] for row in modes]
        firs = [-fr for fr in firs]
    return modes, firs


def rho(a, b):
    return float(np.dot(a, b) / np.sqrt(np.dot(a, a) * np.dot(b, b) + 1e-30))


def f32(v):
    return np.format_float_scientific(np.float32(v), unique=True, trim="-")


NOTICE = """\
// Classical body: fitted from guitar g34 of R. Mores, *Archive for the acoustical documentation of
// classical Spanish guitars, flamenco guitars and romantic guitars from private and public
// collections — bridge mobility*, Zenodo, 2021, doi:10.5281/zenodo.4604577, CC BY 4.0
// (https://creativecommons.org/licenses/by/4.0/). Modified: windowed, converted and reduced to a
// modal table and a short filter by golem-fx. (g34: Manuel Contreras, Madrid, 1971.)
//
// Concert body: fitted from F. Zotter and D. Rudrich, IEM / DEGA guitar recordings ("Gemeinfreie
// Gitarrenaufnahmen" 1 and 2, https://phaidra.kug.ac.at/o:68022, https://phaidra.kug.ac.at/o:69273),
// Public Domain Mark 1.0 (https://creativecommons.org/publicdomain/mark/1.0/). Modified: reduced to a
// modal table and a short filter by golem-fx.
"""


def write(models, out_rs, out_bin, report):
    with open(out_bin, "wb") as fb:
        for m in models:
            for fr in m["firs"]:
                fb.write(np.asarray(fr, "<f4").tobytes())
    lines = ["// SPDX-License-Identifier: GPL-3.0-or-later",
             "// Generated by tools/body_fit.py — do not edit. The body tables (README §2.6): per model, the",
             "// modes (f Hz, Q, and per mic the band-pass and low-pass gains) and the correlation of the",
             "// neighbouring mics' FIR kernels (bridge–sound hole, sound hole–neck); the kernels are in",
             "// data/bodies.f32 (6 models × 3 mics × 1,024 taps, f32 LE, 48 kHz).",
             "//"]
    lines += NOTICE.rstrip("\n").split("\n")
    lines += ["//"] + ["// " + r for r in report]
    lines += ["", "use crate::body::{BodyData, Mode};", ""]
    lines.append("pub static BODIES: [BodyData; 6] = [")
    for m in models:
        lines.append("    BodyData {")
        lines.append(f"        name: \"{m['name']}\",")
        lines.append(f"        measured: {'true' if m['measured'] else 'false'},")
        lines.append(f"        force_like: {'true' if m['force_like'] else 'false'},")
        lines.append(f"        rho: [{f32(m['rho'][0])}, {f32(m['rho'][1])}],")
        lines.append("        modes: &[")
        for i in range(len(m["modes"][0])):
            fm, q = m["modes"][0][i][0], m["modes"][0][i][1]
            g = ", ".join(f"[{f32(m['modes'][k][i][2])}, {f32(m['modes'][k][i][3])}]" for k in range(3))
            lines.append(f"            Mode {{ f: {f32(fm)}, q: {f32(q)}, g: [{g}] }},")
        lines.append("        ],")
        lines.append("    },")
    lines.append("];")
    with open(out_rs, "w") as f:
        f.write("\n".join(lines) + "\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--fetch", help="download the data into DIR (SHA-256 checked), then fit")
    ap.add_argument("--data", help="directory holding the downloaded files")
    ap.add_argument("--parametric-only", action="store_true")
    here = os.path.dirname(os.path.abspath(__file__))
    ap.add_argument("--out", default=os.path.join(here, "..", "src", "bodies.rs"))
    ap.add_argument("--bin", default=os.path.join(here, "..", "data", "bodies.f32"))
    a = ap.parse_args()
    data = a.fetch or a.data
    if a.fetch:
        fetch(a.fetch)
    measured = {}
    report = []
    if data and not a.parametric_only:
        for k, (_, name, digest) in SOURCES.items():
            if sha256(os.path.join(data, name)) != digest:
                sys.exit(f"{name}: SHA-256 mismatch")
        iem1 = os.path.join(data, SOURCES["iem1"][1])
        iem2 = os.path.join(data, SOURCES["iem2"][1])
        f, modes, firs, nfft, info = concert_fit(iem1, iem2)
        measured["Concert"] = (modes, firs, info)
        f, modes, firs, nfft, info = classical_fit(os.path.join(data, SOURCES["mores"][1]), iem1)
        measured["Classical"] = (modes, firs, info)
    # the parametric high band's level re the modes: as measured on Concert's sound hole, if any
    fir_db = -14.0
    if "Concert" in measured:
        md, fr, _ = measured["Concert"]
        m3, f3 = with_hole(md, fr)
        fir_db = band_level(f3[1]) - modal_peak(m3[1])
        print(f"parametric high band: {fir_db:+.1f} dB re the modal peak (from Concert)", file=sys.stderr)
    models = []
    for i, name in enumerate(MODELS):
        if name in measured:
            md, fr, info = measured[name]
            modes, firs = with_hole(md, fr)
            force_like, is_measured = True, True
        else:
            modes, firs = parametric_model(name, fir_db, i + 1)
            force_like = False
            is_measured = False
            if name in ("Concert", "Classical"):
                report.append(f"{name}: PARAMETRIC PLACEHOLDER (the measured data were not fetched)")
        modes, firs = normalise(modes, firs, force_like)
        r = [rho(firs[0], firs[1]), rho(firs[1], firs[2])]
        models.append({"name": name, "modes": modes, "firs": firs, "rho": r, "force_like": force_like,
                       "measured": is_measured})
        print(f"{name}: {len(modes[0])} modes, rho {r[0]:.2f} / {r[1]:.2f}", file=sys.stderr)
    if "Concert" in measured:
        info = measured["Concert"][2]
        report.append(f"Concert: IEM session 1 (Taylor piezo DI → C414 bridge / neck), magnitude √(Syy / Sxx) (median coherence {info["coh"]:.2f}), minimum phase;")
        report.append("  room bands (C414 vs NTI > 3 dB below 300 Hz): " + (", ".join(f"{x:.0f} Hz" for x in info["room"]) or "none"))
        report.append("  held out (session 2, 1/3 oct 80 Hz – 8 kHz): " + "; ".join(
            f"{n} worst {a:.1f} dB, 1 – 3 kHz {b:.1f} dB" for n, a, b in info["held_out"]))
    if "Classical" in measured:
        report.append(f"Classical: Mores g34, poles from the bridge accelerometers, near-field tilt {measured['Classical'][2]['tilt']:+.2f} dB / octave")
    os.makedirs(os.path.dirname(os.path.abspath(a.bin)), exist_ok=True)
    write(models, a.out, a.bin, report)
    print(f"wrote {a.out} and {a.bin}", file=sys.stderr)


if __name__ == "__main__":
    main()
