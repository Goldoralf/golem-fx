# SPDX-License-Identifier: GPL-3.0-or-later
"""p3: level behaviour (README §2.5, §2.6).

1. Static make-up tables: computed on gfx-grit's reference (1 kHz sine, -18 dBFS peak) or on a vocal
   reference (the sung 'a' at -18 dBFS RMS). Then the sung phrase (two vowels, vibrato) is played at
   -18 and -8 dBFS RMS through drive 0 .. 48 dB: output RMS (voiced parts) re input, in dB.
2. Bleed and feedback risk: stage bleed (pink noise at -50 dBFS RMS) in the gaps. Small-signal gain of the
   driven, compensated shaper re bypass, without and with the guard (downward expander before the dirt:
   threshold `guard`, ratio 1:3 below it, range 40 dB, attack 1 ms, release 80 ms, 3 dB hysteresis).
3. What the guard costs at an onset (no lookahead): level lost in the first 5 / 10 / 20 ms of a vowel
   that starts from bleed.
"""
import numpy as np
from common import (FS, voice, pink, shape, rms_db, db, onepole_env)

DRIVES = [0, 6, 12, 18, 24, 30, 36, 42, 48]
TYPES = ["soft", "hard", "tube", "fold"]

sine = 10 ** (-18 / 20) * np.sin(2 * np.pi * 1000 * np.arange(int(0.3 * FS)) / FS)
vref = voice(187.3, 0.5, "a", -18.0)


def table(ref, name):
    out = {}
    for d in DRIVES:
        y = shape(ref, name, d, 2, True)
        out[d] = rms_db(ref[2000:]) - rms_db(y[2000:])
    return out


tabs = {n: {"grit": table(sine, n), "voice": table(vref, n)} for n in TYPES}

a = voice(187.3, 0.7, "a", -18.0, jitter=0.01)
o = voice(150.0, 0.7, "o", -18.0, jitter=0.01, seed=9)
phr = np.concatenate([a, np.zeros(2400), o])
voiced = np.concatenate([np.ones(len(a)), np.zeros(2400), np.ones(len(o))]) > 0

print("== 1. output RMS re input (dB) on the sung phrase, drive 0..48 dB, 2x + ADAA, static make-up")
for n in TYPES:
    for tname in ("grit", "voice"):
        for lvl in (-18, -8):
            x = phr * 10 ** ((lvl + 18) / 20)
            row = []
            for d in DRIVES:
                y = shape(x, n, d, 2, True) * 10 ** (tabs[n][tname][d] / 20)
                row.append(rms_db(y[voiced]) - rms_db(x[voiced]))
            row = np.array(row)
            print(f"  {n:5s} table {tname:5s} input {lvl:+d} dBFS: " + " ".join(f"{v:+5.1f}" for v in row)
                  + f"   spread {row.max()-row.min():4.1f} dB")
print("  make-up tables (dB) at drive " + " ".join(str(d) for d in DRIVES))
for n in TYPES:
    for tname in ("grit", "voice"):
        print(f"  {n:5s} {tname:5s}: " + " ".join(f"{tabs[n][tname][d]:+6.1f}" for d in DRIVES))


# 2. bleed and the guard
def expander(x, thr_db, ratio=4.0, rng_db=40.0, att=1.0, rel=80.0, hyst=3.0, det_ms=5.0, law="ratio"):
    """Downward expander on an RMS detector (one-pole on x^2, det_ms), gain smoothed in dB.
    Below thr: (ratio - 1) dB of cut per dB under it, at most rng_db. Hysteresis: the gate is 'open'
    (no cut) from thr + hyst and closes under thr - hyst; in between it keeps its state."""
    a_det = np.exp(-1 / (det_ms * 1e-3 * FS))
    p = 0.0
    g = np.zeros_like(x)
    open_ = False
    gdb_s = -rng_db
    aa = np.exp(-1 / (att * 1e-3 * FS))
    ar = np.exp(-1 / (rel * 1e-3 * FS))
    for i, v in enumerate(x):
        p = a_det * p + (1 - a_det) * v * v
        e = 10 * np.log10(p + 1e-20)
        if e > thr_db + hyst:
            open_ = True
        elif e < thr_db - hyst:
            open_ = False
        if law == "cap":  # the gfx-morph law: remove the dirt's small-signal gain over a 6 dB knee
            target = 0.0 if open_ else -rng_db * min(1.0, max(0.0, (thr_db - e) / 6.0))
        else:
            target = 0.0 if (open_ or e >= thr_db) else max(-rng_db, (ratio - 1) * (e - thr_db))
        c = aa if target > gdb_s else ar
        gdb_s = c * gdb_s + (1 - c) * target
        g[i] = gdb_s
    return x * 10 ** (g / 20), g


bleed = pink(int(1.5 * FS), -50.0)
print("== 2. stage bleed (pink, -50 dBFS RMS): small-signal gain re bypass (voice-table make-up)")
print("   guard = RMS 5 ms detector, 1:4 below the threshold, range = G_ss(drive) + 6 dB")
for n in ("soft", "hard"):
    for d in (12, 24, 36, 48):
        mk = tabs[n]["voice"][d]
        y = shape(bleed, n, d, 2, True) * 10 ** (mk / 20)
        g0 = rms_db(y[4000:]) - rms_db(bleed[4000:])
        gss = d + mk
        row = [f"G_ss {gss:+5.1f}", f"measured, no guard {g0:+5.1f}"]
        for thr in (-60, -45, -40):
            xg, _ = expander(bleed, thr, rng_db=max(0.0, gss) + 6)
            yg = shape(xg, n, d, 2, True) * 10 ** (mk / 20)
            row.append(f"guard {thr}: {rms_db(yg[int(0.3*FS):]) - rms_db(bleed[int(0.3*FS):]):+6.1f}")
        for thr in (-45, -40):
            xg, _ = expander(bleed, thr, rng_db=max(0.0, gss) + 3, law="cap")
            yg = shape(xg, n, d, 2, True) * 10 ** (mk / 20)
            row.append(f"CAP law {thr}: {rms_db(yg[int(0.3*FS):]) - rms_db(bleed[int(0.3*FS):]):+6.1f}")
        print(f"  {n:5s} drive {d:2d} dB: " + " | ".join(row) + " dB")

# 3. onset cost of the guard
print("== 3. guard at -45 dBFS, drive 36 dB, CAP law (range G_ss + 3 = 21.5 dB): level at a vowel onset over bleed (no lookahead), re no guard")
v = voice(187.3, 0.4, "a", -18.0)
on = int(0.5 * FS)
x = np.concatenate([bleed[:on], v + bleed[on:on + len(v)]])
xg, g = expander(x, -45.0, rng_db=21.5, law="cap")
for ms in (1, 2, 5, 10, 20):
    seg = slice(on, on + int(ms * 1e-3 * FS))
    print(f"  first {ms:2d} ms: {rms_db(xg[seg]) - rms_db(x[seg]):+6.2f} dB  (gain at {ms} ms: {g[on+int(ms*1e-3*FS)-1]:+6.1f} dB)")
for lvl in (-30, -40):
    v2 = v * 10 ** ((lvl + 18) / 20)
    x2 = np.concatenate([bleed[:on], v2 + bleed[on:on + len(v2)]])
    xg2, g2 = expander(x2, -45.0, rng_db=21.5, law="cap")
    print(f"  a quiet vowel at {lvl} dBFS RMS: mean gain over the vowel {np.mean(g2[on+2400:on+len(v2)]):+5.1f} dB")
