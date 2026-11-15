# SPDX-License-Identifier: GPL-3.0-or-later
"""Review r2: the de-fizz law beyond p4's phrase (README §2.3).

1. p4's phrase, full-band: where does the 's' end up against the vowel (bypass: -6 dB)? A de-fizz that
   leaves it far under the bypass ratio is a lisp.
2. Detector position: the HF share after a +12 dB `tilt` (pivot 1 kHz) and on a breathy vowel
   (aspiration noise 2 - 8 kHz at -20 / -12 dB re the vowel): does a vowel reach the -14 dB threshold?
3. Non-vocal sources: a hi-hat-like burst train and white hiss: the share, and the drive duck at 50 %.
4. A real take (GFX_MORPH_VOICE): the share's distribution by level, the time the duck acts on loud
   frames, Soft 30 dB: hash in 1 - 4 kHz on the frames the detector flags, de-fizz 0 / 50 / 100 %.
5. The same law at drive 12 / 20 / 30 / 42 dB on p4's phrase (does it hold away from 30 dB?).
"""
import sys
import numpy as np
from scipy import signal
sys.path.insert(0, "review")
import rvoice
from common import FS, phrase, voice, lr4, rms_db, rms, up, down, adaa1

from r2_defizz_lib import share_of, cut_of, TD, TM


def design(sig, amt, drive=30.0):
    sh, _ = share_of(sig)
    g = cut_of(sh, 0.18 * amt)
    d = np.maximum(0.0, drive + g)
    y = down(adaa1(up(sig, 2) * np.repeat(10 ** (d / 20), 2), "soft"), 2) * 10 ** (np.interp(d, TD, TM) / 20)
    lo, hi = lr4(y, 4500.0)
    return lo + hi * 10 ** (g / 20), g


def mid(y):
    return signal.sosfilt(signal.butter(4, [1000, 4000], "band", fs=FS, output="sos"), y)


x, R = phrase()
S = lambda y: np.concatenate([y[R["s1"]], y[R["s2"]]])
V = lambda y: np.concatenate([y[R["a"]], y[R["o"]]])
print("== 1. p4's phrase, full band: 's' re vowel (dB)")
print(f"  bypass {rms_db(S(x)) - rms_db(V(x)):+5.1f}")
for amt in (0, 25, 50, 75, 100):
    y, g = design(x, amt)
    print(f"  Soft 30 dB, de-fizz {amt:3d} %: 's' re vowel {rms_db(S(y)) - rms_db(V(y)):+5.1f} | 's' in 1-4 kHz re vowel in 1-4 kHz {rms_db(S(mid(y))) - rms_db(V(mid(y))):+5.1f}")

print("== 2. HF share of vowels (median, dB): threshold -14, full cut at -6")
tb, ta = signal.butter(1, 1000, "high", fs=FS)


def tilt12(s):  # +-6 dB first-order shelves around 1 kHz: 0.5 x + 1.5 hp1(x) (approximation of the pair)
    return 0.5 * s + 1.5 * signal.lfilter(tb, ta, s)


rng = np.random.default_rng(5)
for vw in ("a", "e", "o"):
    v = voice(220.0, 0.6, vw, -18.0, 0.01)
    row = [f"plain {np.median(share_of(v)[0][4800:]):+6.1f}", f"tilt +12 {np.median(share_of(tilt12(v))[0][4800:]):+6.1f}"]
    for asp in (-20, -12):
        n = signal.lfilter(*signal.butter(2, [2000, 8000], "band", fs=FS), rng.standard_normal(len(v)))
        n = n / rms(n) * rms(v) * 10 ** (asp / 20)
        row.append(f"breath {asp} dB {np.median(share_of(v + n)[0][4800:]):+6.1f}")
        row.append(f"breath {asp} dB + tilt {np.median(share_of(tilt12(v + n))[0][4800:]):+6.1f}")
    print(f"  '{vw}': " + " | ".join(row))

print("== 3. non-vocal sources")
n = int(2 * FS)
hat = signal.lfilter(*signal.butter(2, 6000, "high", fs=FS), rng.standard_normal(n)) * np.exp(-(np.arange(n) % 6000) / 1200.0)
kick = np.sin(2 * np.pi * 55 * np.arange(n) / FS) * np.exp(-(np.arange(n) % 24000) / 4000.0)
drums = 0.1 * hat / rms(hat) + 0.3 * kick
sh, _ = share_of(drums)
g = cut_of(sh, 9.0)
print(f"  kick + hi-hat loop: share median {np.median(sh):+.1f}, p90 {np.percentile(sh, 90):+.1f} dB; drive duck at 50 %: mean {g.mean():.1f} dB, under -3 dB {100*np.mean(g < -3):.0f} % of the time")
hiss = 10 ** (-70 / 20) * rng.standard_normal(n)
sh, _ = share_of(hiss)
print(f"  white hiss at -70 dBFS (a preamp's floor, no voice): share {np.median(sh):+.1f} dB -> the duck sits at its maximum in silence")
v = voice(187.3, 0.5, "a", -18.0, 0.01)
sig = np.concatenate([hiss[:int(0.5 * FS)], v + hiss[:len(v)]])
sh, phf = share_of(sig)
for amt in (50, 100):
    g = cut_of(sh, 0.18 * amt)
    on = int(0.5 * FS)
    print(f"    de-fizz {amt} %: drive duck at the vowel's onset +0 / 10 / 30 / 60 / 100 ms: " + " / ".join(f"{g[on + int(ms*48)]:.1f}" for ms in (0, 10, 30, 60, 100)) + " dB")
# the fix of the review: weight the cut by the HF band's own level (0 under -66 dBFS, 1 from -60)
w = np.clip((phf + 66) / 6, 0, 1)
g2 = cut_of(-14 + 8 * np.clip((sh + 14) / 8, 0, 1) * w, 18.0)
print(f"    with the HF-level floor (-66 .. -60 dBFS): duck at the onset +0 / 10 / 30 ms: " + " / ".join(f"{g2[on + int(ms*48)]:.1f}" for ms in (0, 10, 30)) + " dB")

print("== 5. the law away from 30 dB of drive (p4's phrase): 's' in 1-4 kHz re vowel in 1-4 kHz, de-fizz 0 / 50 / 100 %")
for d in (12, 20, 30, 42):
    row = []
    for amt in (0, 50, 100):
        y, _ = design(x, amt, d)
        row.append(f"{rms_db(S(mid(y))) - rms_db(V(mid(y))):+6.1f}")
    print(f"  drive {d}: " + " / ".join(row))

print("== 4. a real take")
r = rvoice.load()
if r is None:
    print("  GFX_MORPH_VOICE not set")
    sys.exit()
m, _ = rvoice.active(r)
r = r / np.sqrt(np.mean(r[m] ** 2)) * 10 ** (-18 / 20)
sh, phf = share_of(r)
L = 10 * np.log10(signal.lfilter([1 - np.exp(-1 / 240)], [1, -np.exp(-1 / 240)], r * r) + 1e-20)
for lo, hi in ((-70, -50), (-50, -40), (-40, -30), (-30, 0)):
    k = (L > lo) & (L <= hi)
    print(f"  level {lo} .. {hi} dBFS: {100*k.mean():4.1f} % of the take | share median {np.median(sh[k]):+6.1f}, 90th {np.percentile(sh[k], 90):+6.1f} | above -14: {100*np.mean(sh[k] > -14):4.1f} %, above -6: {100*np.mean(sh[k] > -6):4.1f} %")
flag = sh > -10
y0, _ = design(r, 0)
ref = rms_db(mid(y0)[m & ~flag])
for amt in (0, 50, 100):
    y, g = design(r, amt)
    print(f"  Soft 30 dB, de-fizz {amt:3d} %: flagged frames ({100*flag.mean():.1f} % of the take) in 1-4 kHz re the rest {rms_db(mid(y)[flag]) - ref:+6.1f} dB | "
          f"unflagged active frames: level {rms_db(y[m & ~flag]) - rms_db(y0[m & ~flag]):+5.2f} dB | duck under -3 dB on frames above -30 dBFS: {100*np.mean(g[L > -30] < -3):.2f} %")
