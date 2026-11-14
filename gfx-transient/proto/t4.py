import numpy as np
from core import *
rng = np.random.default_rng(7)
t = np.arange(int(4 * FS)) / FS
white = rng.standard_normal(len(t)) * 0.1
pink = np.cumsum(rng.standard_normal(len(t))); pink = sosfilt(butter(2, 40, 'hp', fs=FS, output='sos'), pink); pink = 0.1 * pink / pink.std()
detu = 0.25 * (np.sin(2*np.pi*110*t) + 0.8*np.sin(2*np.pi*111.5*t))
chord = 0.05*sum(np.sin(2 * np.pi * f * t + i) for i, f in enumerate([82.4, 123.5, 164.8, 207.6, 246.9, 329.6]))
print("== E2 dead zone D0 on the attack law: share stats (rms/99pct/max, % of attack) on steady signals")
for name, s in [('white', white), ('pink', pink), ('detuned', detu), ('chord', chord)]:
    r = detector(s, attack=12)
    Da = r['Da'][FS:]
    for dref in [4, 6, 12]:
        row = []
        for D0 in [0, 1, 1.5, 2, 3]:
            sh = 100 * (1 - np.exp(-np.maximum(0, Da - D0) / dref))
            row.append(f"D0={D0}: {np.sqrt(np.mean(sh**2)):.1f}/{np.percentile(sh,99):.1f}/{sh.max():.1f}")
        print(f" {name:8s} Dref {dref:2d}: " + " | ".join(row))
print("== re-pick rises: share by D0 at Dref 12 and 4: rise 3/6/12/24 dB")
for dref in [12, 4, 6]:
    for D0 in [0, 1.5, 2]:
        print(f" Dref {dref} D0 {D0}: " + " ".join(f"{100*(1-np.exp(-max(0,d-D0)/dref)):.0f}%" for d in [3, 6, 12, 24]))

print("== F decaying bass 41.2 Hz + sustain: gain ripple at note rate (p-p within one period, 0.5-1.5 s)")
for dec in [6, 15, 30]:
    s = ks_string(41.2, 2.0, decay_db_s=dec, pick=0.13)
    for sus_time in [300, 2000]:
        for dref in [12, 4, 2]:
            row = []
            for rf in [10, 20, 40]:
                r = detector(s, sustain=12, sus_time=sus_time, dref=dref, rf_rel=rf, thresh=-96, floor_win=90)
                g = r['Gs'][int(.5*FS):int(1.5*FS)]
                per = int(FS / 41.2)
                pp = max(g[i:i+per].max() - g[i:i+per].min() - abs(g[i+per]-g[i]) for i in range(0, len(g) - per - 1, per))
                row.append(f"rf{rf}: {pp:.3f}")
            print(f" decay {dec:2d} dB/s sus_time {sus_time} Dref {dref:2d}: ripple p-p dB " + " ".join(row) + f"  (G_sus at 1 s {r['Gs'][FS]:.1f} dB)")

print("== G floor bound: notes + hiss, sustain +24, thresh -66")
for hiss_db in [-70, -60, -50]:
    x = place(6.0, [(0.5, ks_string(110, 2.0, decay_db_s=20, amp=0.5)), (3.0, ks_string(147, 2.0, decay_db_s=20, amp=0.5))])
    x = x + rng.standard_normal(len(x)) * 10 ** (hiss_db / 20) / 3.5   # peaks ~ hiss_db
    r = detector(x, sustain=24, sus_time=1000)
    seg = r['Gs'][int(2.6*FS):int(2.95*FS)]; seg2 = r['Gs'][int(5.5*FS):]
    pk = 20*np.log10(r['p'][int(2.6*FS):int(2.95*FS)].mean())
    print(f" hiss peaks ~{pk:.1f} dBFS: floor {20*np.log10(r['F'][int(2.8*FS)]):.1f} dB; G_sus in gap {seg.mean():.2f} (max {seg.max():.2f}); later {seg2.mean():.2f}; G_sus max overall {r['Gs'].max():.1f}")

print("== H gain step at a new onset while the tail is boosted/cut (max |dG| per sample, dB)")
x = place(2.0, [(0.1, ks_string(110, 1.5, decay_db_s=40)), (0.7, ks_string(147, 1.0))])
for S in [12, -24]:
    for sm in [False, True]:
        r = detector(x, sustain=S, sus_time=1000, smooth_sus=sm)
        i0 = int(0.7 * FS); g = r['Gs']
        d = np.abs(np.diff(g[i0-10:i0 + 500]))
        k = np.argmax(d)
        # time for G to go from 90% to 10% of pre-onset value
        pre = g[i0 - 5]; a = np.where(np.abs(g[i0-5:i0+2000]) < 0.1 * abs(pre))[0]
        print(f" sustain {S:+d} smooth_sus {sm}: G before onset {pre:.1f} dB, max step {d.max():.2f} dB/sample, falls to 10% within {a[0]/FS*1e3 if len(a) else -1:.2f} ms")
