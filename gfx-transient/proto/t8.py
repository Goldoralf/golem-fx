import numpy as np
from core import *
rng = np.random.default_rng(7)
print("== final spec (dead zone 1.5, half rule, smoothed G_sus). D: second hit share / first hit share")
for hp in [0, 120, 200, 300]:
    for rel in [-6, -12, -18]:
        row = []
        for dt in [0.03, 0.05, 0.08, 0.125, 0.25]:
            x = place(1.0, [(0.1, snare(0.5)), (0.1 + dt, snare(0.5, amp=0.5 * 10 ** (rel / 20), seed=5))])
            r = detector(x, sc_hpf=hp, attack=12, rule='half', att_time=10, smooth=0.05)
            i = int((0.1 + dt) * FS); first = r['Ga'][int(.1*FS):int(.1*FS)+200].max()
            row.append((r['Ga'][i:i + int(0.02 * FS)].max() - r['Ga'][i-1]) / first)
        print(f" hpf {hp:3d} W {r['W']*1e3:4.1f} ms, hit2 {rel:4d} dB: dt 30/50/80/125/250 ms -> " + " ".join(f"{v:.2f}" for v in row))
x = place(3.0, [(0.1, ks_string(82.4, 1.0)), (0.6, ks_string(110, 1.0, amp=0.35)), (1.2, snare(0.5)), (1.5, snare(0.5, amp=0.2)), (1.9, ks_string(330, 1.0, amp=0.5))])
ref = detector(x, attack=12, sustain=12, thresh=-96, smooth_sus=True)['G']
for lv in [-12, -24, -36]:
    r = detector(x * 10 ** (lv / 20), attack=12, sustain=12, thresh=-96, smooth_sus=True)
    print(f" level indep. lv {lv}: {np.max(np.abs(r['G']-ref)):.3f} dB")
t = np.arange(int(3 * FS)) / FS
white = rng.standard_normal(len(t)) * 0.1
for sens in [50, 100]:
    r = detector(white, attack=12, sustain=12, sens=sens, smooth_sus=True)
    print(f" white noise sens {sens}: G_att max {r['Ga'][FS:].max():.2f} dB; G_sus mean {r['Gs'][FS:].mean():.2f} p-p {np.ptp(r['Gs'][FS:]):.2f} dB")
for f, hp in [(41.2, 80), (61.7, 120), (82.4, 120), (82.4, 160)]:
    s = ks_string(f, 1.0, decay_db_s=6, asym=0.7, pick=0.13)
    r = detector(s, sc_hpf=hp, attack=24, sustain=24, rule='half', sens=100, smooth_sus=True)
    g = r['Ga'][int(0.3*FS):int(0.8*FS)]; gs = r['Gs'][int(0.3*FS):int(0.8*FS)]
    per = int(FS / f); pp = max(gs[i:i+per].max() - gs[i:i+per].min() for i in range(0, len(gs) - per, per))
    print(f" string {f} Hz sc_hpf {hp} sens 100 +-24: G_att p-p {np.ptp(g):.3f}; G_sus ripple per period {pp:.3f} dB (drift incl.)")
# re-pick rises
for sens in [50, 100]:
    out = []
    for rise in [3, 6, 12, 24]:
        n = int(0.6 * FS); tt = np.arange(n) / FS; x = 0.05 * np.sin(2*np.pi*1000*tt); i0 = int(0.3*FS); x[i0:] *= 10 ** (rise / 20)
        r = detector(x, attack=12, sens=sens, smooth=0.05); out.append(f"{r['Ga'][i0:i0+500].max()/12*100:.0f}%")
    print(f" re-pick rises 3/6/12/24 dB sens {sens}: " + " ".join(out))
