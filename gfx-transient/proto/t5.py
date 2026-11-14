import numpy as np
from core import *
from scipy.signal import freqs
rng = np.random.default_rng(7)
t = np.arange(int(4 * FS)) / FS
white = rng.standard_normal(len(t)) * 0.1
r = detector(white, attack=12, sustain=12)
Da = r['Da'][FS:]; Ds = r['Ds'][FS:]
for dref in [2, 4, 12]:
    sh = 100 * (1 - np.exp(-np.maximum(0, Da - 1.5) / dref)); ss = 100 * (1 - np.exp(-Ds / dref))
    print(f"white Dref {dref}: attack share with D0=1.5: rms {np.sqrt(np.mean(sh**2)):.2f}% max {sh.max():.1f}% | sustain share mean {ss.mean():.1f}% p-p {ss.max()-ss.min():.1f}% (Ds mean {Ds.mean():.2f} dB)")

print("== I shelf: static table (att_freq 1 kHz) and monotonicity")
def H(g, f, f0=1000.0, kind='hi'):
    s = 1j * f / f0; rg = np.sqrt(g)
    return 1 + (g - 1) * s / (s + rg) if kind == 'hi' else 1 + (g - 1) * (1 / rg) / (s + 1 / rg)
for gdb in [12, 24, -12, -24]:
    g = 10 ** (gdb / 20)
    print(f" {gdb:+d} dB Highs: " + " ".join(f"{20*np.log10(abs(H(g, f))):+.1f}" for f in [80, 300, 1000, 3000, 8000]) + "   Lows: " + " ".join(f"{20*np.log10(abs(H(g, f, kind='lo'))):+.1f}" for f in [80, 300, 1000, 3000, 8000]))
ff = np.logspace(0, 5, 4000)
ok = True
for gdb in np.linspace(-24, 24, 97):
    g = 10 ** (gdb / 20)
    for kind in ['hi', 'lo']:
        m = 20 * np.log10(np.abs(H(g, ff, kind=kind)))
        d = np.diff(m)
        mono = np.all(d >= -1e-9) or np.all(d <= 1e-9)
        inb = m.min() >= min(0, gdb) - 1e-6 and m.max() <= max(0, gdb) + 1e-6
        ok &= mono and inb
print(" monotonic and within [0, G] for all gains -24..+24, both kinds:", ok)
# fixed-cutoff 2nd-order HP mix dip
for gdb in [12, 24]:
    g = 10 ** (gdb / 20); s = 1j * ff / 1000
    m = 20 * np.log10(np.abs(1 + (g - 1) * s * s / (s * s + np.sqrt(2) * s + 1)))
    print(f" 2nd-order fixed HP mix at {gdb:+d}: min {m.min():.2f} dB")

print("== I2 time-varying TPT shelf: digital, per-sample cutoff = f0*sqrt(g)")
def shelf_tv(x, gdb, f0=1000.0, kind='hi', fs=FS):
    y = np.zeros_like(x); s = 0.0
    g = 10 ** (gdb / 20)
    for i in range(len(x)):
        fc = f0 * np.sqrt(g[i]) if kind == 'hi' else f0 / np.sqrt(g[i])
        k = np.tan(np.pi * min(fc, 0.45 * fs) / fs); G = k / (1 + k)
        v = (x[i] - s) * G; lp = v + s; s = lp + v
        y[i] = x[i] + (g[i] - 1) * ((x[i] - lp) if kind == 'hi' else lp)
    return y
n = int(0.5 * FS); tt = np.arange(n) / FS
# static digital response check at +12, 3 kHz and 80 Hz
for f, want in [(80, 0.1), (3000, 10.5), (8000, 11.8)]:
    x = np.sin(2 * np.pi * f * tt); y = shelf_tv(x, np.full(n, 12.0))
    print(f"  digital +12 dB @ {f} Hz: {20*np.log10(np.abs(y[n//2:]).max()):+.2f} dB (analog {want:+.1f})")
# gain trajectory: jump to +24 with 0.5 ms smooth then decay 15 ms, on an 80 Hz sine and on DC-free low signal
ksm = np.exp(-1 / (0.0005 * FS)); gd = np.zeros(n); tgt = np.zeros(n)
i0 = int(0.2 * FS); tgt[i0:] = 24 * np.exp(-(np.arange(n - i0) / FS) / 0.015)
acc = 0.0
for i in range(n):
    acc = tgt[i] if tgt[i] < acc else tgt[i] + (acc - tgt[i]) * ksm; gd[i] = acc
for f in [41.2, 80, 300]:
    x = 0.5 * np.sin(2 * np.pi * f * tt)
    for kind in ['hi', 'lo']:
        y = shelf_tv(x, gd, kind=kind); yf = x * 10 ** (gd / 20)
        d = y - x; df = yf - x
        print(f"  {f} Hz sine, +24 dB burst gain, {kind}: peak of added signal {20*np.log10(np.abs(d).max()/0.5+1e-12):+.1f} dB re input (Full-band would add {20*np.log10(np.abs(df).max()/0.5):+.1f}); max sample step of added {np.abs(np.diff(d)).max()/0.5:.4f}")
# worst case: abrupt (unsmoothed) gain step +24 dB on a 80 Hz sine at its peak: discontinuity?
gd2 = np.zeros(n); gd2[int(0.2*FS)+150:] = 24
for kind in ['hi', 'lo']:
    x = 0.5 * np.sin(2 * np.pi * 80 * tt); y = shelf_tv(x, gd2, kind=kind)
    print(f"  unsmoothed +24 dB step, 80 Hz, {kind}: max |dy| per sample {np.abs(np.diff(y)).max():.4f} (input max step {np.abs(np.diff(x)).max():.4f}); settles to {20*np.log10(np.abs(y[-2000:]).max()/0.5):+.2f} dB")
# stability under random audio-rate cutoff modulation
gr = rng.uniform(-24, 24, n); xr = rng.standard_normal(n)
for kind in ['hi', 'lo']:
    y = shelf_tv(xr, gr, kind=kind); print(f"  random per-sample gain +-24 dB, noise in, {kind}: max |y| {np.abs(y).max():.1f} (bound ~ {np.abs(xr).max()*10**(24/20):.1f})")
