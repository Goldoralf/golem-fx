# SPDX-License-Identifier: GPL-3.0-or-later
"""p4: sibilance into distortion (README §2.3).

The phrase 's a (gap) s o': sibilants (4 - 10 kHz noise) at -24 dBFS RMS, vowels at -18 dBFS RMS, into
Soft at 30 dB, 2x + ADAA, voice-table make-up. The shaper turns an 's' into broadband hash: its products
land in 1 - 4 kHz, where the ear is most sensitive, and no de-esser after the dirt can take them out
without taking the voice's presence with them.

Compared, with the same detector (HF share = power above 4.5 kHz / full-band power, 5 ms RMS):
  A. nothing;
  B. de-esser AFTER the dirt (the usual mix advice): the band above 4.5 kHz cut dynamically;
  C. gfx-morph's de-fizz BEFORE the dirt: the input's band above 4.5 kHz cut (LR4 band gain);
  D. C plus the same cut after (both).
Metric: output level of the 's' in 1 - 4 kHz and above 4.5 kHz, re the vowel's output in the same band;
and how much the vowels themselves are touched (their output level change re A).
"""
import numpy as np
from scipy import signal
from common import FS, phrase, shape, lr4, rms_db, bw2

x, R = phrase()
DRIVE = 30.0
MAKEUP = -17.15  # voice table, Soft, 30 dB (p3)


def hf_share_cut(sig, range_db, thr=-14.0, knee=8.0, att=0.5, rel=30.0, det=5.0):
    """Cut (dB, <= 0) from the HF share of `sig`."""
    b, a = signal.butter(2, 4500, "high", fs=FS)
    hf = signal.lfilter(b, a, signal.lfilter(b, a, sig))
    ad = np.exp(-1 / (det * 1e-3 * FS))
    pf = signal.lfilter([1 - ad], [1, -ad], sig ** 2)
    ph = signal.lfilter([1 - ad], [1, -ad], hf ** 2)
    share = 10 * np.log10((ph + 1e-20) / (pf + 1e-20))
    target = -range_db * np.clip((share - thr) / knee, 0, 1)
    aa = np.exp(-1 / (att * 1e-3 * FS))
    ar = np.exp(-1 / (rel * 1e-3 * FS))
    g = np.zeros_like(sig)
    s = 0.0
    for i, t in enumerate(target):
        c = aa if t < s else ar
        s = c * s + (1 - c) * t
        g[i] = s
    return g, share


def band_cut(sig, g_db, fc=4500.0):
    lo, hi = lr4(sig, fc)
    return lo + hi * 10 ** (g_db / 20)


def dirt(sig):
    return shape(sig, "soft", DRIVE, 2, True) * 10 ** (MAKEUP / 20)


def bands(y):
    b1 = signal.butter(4, [1000, 4000], "band", fs=FS, output="sos")
    b2 = signal.butter(4, 4500, "high", fs=FS, output="sos")
    return signal.sosfilt(b1, y), signal.sosfilt(b2, y)


g_in, share_in = hf_share_cut(x, 18.0)
print(f"HF share of the input: vowel 'a' {np.median(share_in[R['a']]):+.1f} dB, 'o' {np.median(share_in[R['o']]):+.1f} dB, "
      f"'s' {np.median(share_in[R['s1']]):+.1f} dB (threshold -14, knee 8)")

cases = {}
cases["A none"] = dirt(x)
yA = cases["A none"]
gB, _ = hf_share_cut(yA, 18.0)  # detector after the dirt reads the distorted signal
cases["B de-ess after"] = band_cut(yA, gB)
cases["C de-fizz before"] = dirt(band_cut(x, g_in))
yC = cases["C de-fizz before"]
gD, _ = hf_share_cut(yC, 18.0)
cases["D before + after"] = band_cut(yC, gD)

mid_a, hf_a = bands(yA)
ref_mid = rms_db(mid_a[R["a"]])
ref_hf = rms_db(hf_a[R["a"]])
for k, y in cases.items():
    m, h = bands(y)
    s_mid = rms_db(np.concatenate([m[R["s1"]], m[R["s2"]]])) - ref_mid
    s_hf = rms_db(np.concatenate([h[R["s1"]], h[R["s2"]]])) - ref_hf
    v_full = rms_db(np.concatenate([y[R["a"]], y[R["o"]]])) - rms_db(np.concatenate([yA[R["a"]], yA[R["o"]]]))
    v_hf = rms_db(np.concatenate([h[R["a"]], h[R["o"]]])) - rms_db(np.concatenate([hf_a[R["a"]], hf_a[R["o"]]]))
    print(f"{k:18s}: 's' in 1-4 kHz re vowel {s_mid:+6.1f} dB | 's' above 4.5 kHz re vowel {s_hf:+6.1f} dB | "
          f"vowels: level {v_full:+5.2f} dB, above 4.5 kHz {v_hf:+5.2f} dB")

# the clean (bypass) reference: what an 's' is relative to a vowel without any dirt
m, h = bands(x)
print(f"{'bypass (no dirt)':18s}: 's' in 1-4 kHz re vowel {rms_db(np.concatenate([m[R['s1']], m[R['s2']]])) - rms_db(m[R['a']]):+6.1f} dB | "
      f"'s' above 4.5 kHz re vowel {rms_db(np.concatenate([h[R['s1']], h[R['s2']]])) - rms_db(h[R['a']]):+6.1f} dB")
# de-fizz amount law: range 18 dB at 100 %; sweep
for amt in (0, 25, 50, 75, 100):
    g, _ = hf_share_cut(x, 0.18 * amt)
    y = dirt(band_cut(x, g))
    mm, hh = bands(y)
    print(f"  defizz {amt:3d} %: 's' in 1-4 kHz re vowel {rms_db(np.concatenate([mm[R['s1']], mm[R['s2']]])) - ref_mid:+6.1f} dB")

# E. drive ducking: during an 's' the drive into the shaper drops by the cut (dB) and the make-up
#    follows the table at the lowered drive, so the 's' passes the shaper nearly linear (no hash);
#    then the same cut on the band above 4.5 kHz after the dirt (the 's' itself, now clean, is lowered).
TABLE_D = np.array([0, 6, 12, 18, 24, 30, 36, 42, 48], float)
TABLE_M = np.array([0.1, -5.5, -10.4, -13.9, -16.0, -17.0, -17.5, -17.7, -17.8])  # Soft, voice (p3)
print("== E. drive ducking by the de-fizz cut (gfx-morph's law), then the cut after the dirt")
from common import up, down, adaa1
for amt in (0, 50, 100):
    g, _ = hf_share_cut(x, 0.18 * amt)          # cut in dB, <= 0
    d_eff = np.maximum(0.0, DRIVE + g)           # drive lowered by the cut
    mk = np.interp(d_eff, TABLE_D, TABLE_M)
    gu = np.repeat(10 ** (d_eff / 20), 2)
    y = down(adaa1(up(x, 2) * gu, "soft"), 2) * 10 ** (mk / 20)
    y = band_cut(y, g)
    mm, hh = bands(y)
    va = np.concatenate([y[R["a"]], y[R["o"]]])
    print(f"  defizz {amt:3d} %: 's' in 1-4 kHz re vowel {rms_db(np.concatenate([mm[R['s1']], mm[R['s2']]])) - ref_mid:+6.1f} dB"
          f" | above 4.5 kHz {rms_db(np.concatenate([hh[R['s1']], hh[R['s2']]])) - ref_hf:+6.1f} dB"
          f" | vowels level {rms_db(va) - rms_db(np.concatenate([yA[R['a']], yA[R['o']]])):+5.2f} dB, "
          f"above 4.5 kHz {rms_db(np.concatenate([hh[R['a']], hh[R['o']]])) - rms_db(np.concatenate([hf_a[R['a']], hf_a[R['o']]])):+5.2f} dB")
