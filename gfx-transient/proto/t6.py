import numpy as np
from core import *
from core import _cmax
D0 = 1.5
def share_from(Da, dref=12.0): return 1 - np.exp(-np.maximum(0, Da - D0) / dref)
def zl(sh, smooth_ms):
    k = np.exp(-1 / (smooth_ms * 1e-3 * FS)); o = np.zeros_like(sh); a = 0.0
    for i, s in enumerate(sh):
        a = s if s < a else s + (a - s) * k; o[i] = a
    return o
def la(sh, L_ms, smooth_ms):
    L = int(round(L_ms * 1e-3 * FS)); R = int(round(np.clip(smooth_ms, 0.25, L_ms) * 1e-3 * FS))
    h = _cmax(sh, R + 1)
    r1 = R // 2; r2 = R - r1
    def box(a, n):
        if n <= 0: return a
        c = np.cumsum(np.concatenate([np.zeros(n + 1), a])); return (c[n + 1:] - c[:-n - 1]) / (n + 1)
    b = box(box(h, r1), r2)
    out = np.concatenate([np.zeros(L - R), b])[:len(sh)]
    return out, L
print("== attack-law table with dead zone 1.5 dB, default sens (step envelope)")
for u in [0.05, 0.1, 0.29, 0.5, 1, 1.5, 2, 3]:
    D = -20 * np.log10(1 - np.exp(-u)); print(f"  t/att_time {u}: D_a {D:.1f} dB share {100*(1-np.exp(-max(0,D-D0)/12)):.0f}%  (no dead zone {100*(1-np.exp(-D/12)):.0f}%)")
n = int(0.3 * FS); t = np.arange(n) / FS; i0 = int(0.1 * FS)
x = np.zeros(n); x[i0:] = 0.5 * np.sin(2 * np.pi * 1000 * (t[i0:] - t[i0]))
r = detector(x, att_time=15)
sh = share_from(r['Da'])
per = 48
def cyc_gain(g, k, lag=0): return g[i0 + lag + k * per: i0 + lag + (k + 1) * per]
print("== J 1 kHz burst from silence, att_time 15")
for A in [12, -24]:
    for sm in [0.05, 0.5, 1.0, 5.0]:
        g = A * zl(sh, sm)
        y = x * 10 ** (g / 20)
        c1 = 20 * np.log10(np.abs(y[i0:i0 + per]).max() / 0.5)
        print(f"  ZL  attack {A:+d} smooth {sm}: gain at samples 1/6/12/24/48/144: " + " ".join(f"{g[i0+k]:.1f}" for k in [1, 6, 12, 24, 48, 144]) + f" | 1st-cycle peak {c1:+.1f} dB")
    for L in [1, 2, 5]:
        for sm in [0.5, 1.0]:
            s2, Ls = la(sh, L, sm); g = A * s2
            xd = np.concatenate([np.zeros(Ls), x])[:n]; y = xd * 10 ** (g / 20)
            c = [g[i0 + Ls + k] for k in [-48, -24, 0, 12, 48, 96, 144]]
            c1 = 20 * np.log10(np.abs(y[i0 + Ls:i0 + Ls + per]).max() / 0.5); c3 = 20 * np.log10(np.abs(y[i0 + Ls + 2 * per:i0 + Ls + 3 * per]).max() / 0.5)
            pre = np.abs(np.diff(g[:i0 + Ls])).max()
            print(f"  LA {L} ms smooth {sm}: gain at onset-1ms/-0.5ms/0/+0.25/+1/+2/+3 ms: " + " ".join(f"{v:.1f}" for v in c) + f" | cycle1 {c1:+.1f} cycle3 {c3:+.1f} dB")
print("== same with att_time 15 on gain inside the first cycle (LA 2, smooth 0.5): max-min over cycle 1 for +12: ", end='')
s2, Ls = la(sh, 2, 0.5); g = 12 * s2; print(f"{np.ptp(g[i0+Ls:i0+Ls+per]):.2f} dB;  ZL smooth 0.5: {np.ptp(12*zl(sh,0.5)[i0:i0+per]):.2f} dB")

print("== C bass E1 41.2 Hz full-band attack +12: Delta (wet-dry) energy above 300 Hz relative to the dry attack's, and gain over the first periods")
s = place(1.0, [(0.1, ks_string(41.2, 0.9, pick=0.13))])
sos = butter(4, 300, 'hp', fs=FS, output='sos')
for at in [10, 15, 50, 100]:
    r = detector(s, att_time=at); g = 12 * zl(share_from(r['Da']), 0.5)
    y = s * 10 ** (g / 20); d = y - s
    j = int(0.1 * FS); T = int(FS / 41.2)
    e_d = np.sum(sosfilt(sos, d)[j:j + 4 * T] ** 2); e_s = np.sum(sosfilt(sos, s)[j:j + 4 * T] ** 2)
    print(f"  att_time {at}: gain at 0.25T/0.5T/1T/2T/3T: " + " ".join(f"{g[j+int(k*T)]:.1f}" for k in [0.25, 0.5, 1, 2, 3]))
