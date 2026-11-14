import numpy as np
from core import *
rng = np.random.default_rng(7)
print("== D soft hit after a loud one (snare 200 dB/s). share of `attack` given to hit 2 (hit 1 = 1.00 ref)")
for hp, rule in [(0, 'half'), (100, 'gate'), (100, 'half'), (200, 'half'), (300, 'half')]:
    for rel in [-6, -12, -18]:
        row = []
        for dt in [0.015, 0.03, 0.05, 0.08, 0.125, 0.25]:
            x = place(1.0, [(0.1, snare(0.5)), (0.1 + dt, snare(0.5, amp=0.5 * 10 ** (rel / 20), seed=5))])
            r = detector(x, sc_hpf=hp, attack=12, rule=rule, att_time=10)
            i = int((0.1 + dt) * FS)
            row.append(r['Ga'][i:i + int(0.02 * FS)].max() / 12 - r['Ga'][i-1]/12)
        print(f" hpf {hp:3d} {rule} W {r['W']*1e3:4.1f} ms, hit2 {rel:4d} dB: dt 15/30/50/80/125/250 ms -> " + " ".join(f"{v:.2f}" for v in row))
print("== D2 flam: soft grace (-10 dB) 25 ms BEFORE the main hit: share on grace / main")
x = place(1.0, [(0.1, snare(0.5, amp=0.16, seed=5)), (0.125, snare(0.5))])
r = detector(x, attack=12, att_time=10)
print("  grace %.2f main %.2f" % (r['Ga'][int(.1*FS):int(.12*FS)].max()/12, r['Ga'][int(.125*FS):int(.15*FS)].max()/12))

print("== E steady noise-like signals: G_att with attack +12 (dB): rms / 99th pct / max, by D_ref and W")
t = np.arange(int(4 * FS)) / FS
white = rng.standard_normal(len(t)) * 0.1
pink = np.cumsum(rng.standard_normal(len(t))); pink = sosfilt(butter(2, 40, 'hp', fs=FS, output='sos'), pink); pink = 0.1 * pink / pink.std()
chord = sum(np.sin(2 * np.pi * f * t + i) for i, f in enumerate([82.4, 123.5, 164.8, 207.6, 246.9, 329.6]))
dist = np.tanh(8 * chord / 6) * 0.3
semi = 0.25 * (np.sin(2*np.pi*82.4*t) + np.sin(2*np.pi*87.3*t))
detu = 0.25 * (np.sin(2*np.pi*110*t) + 0.8*np.sin(2*np.pi*111.5*t))
for name, s in [('white', white), ('pink', pink), ('clean chord E', chord * 0.05), ('dist chord', dist), ('semitone dyad 5Hz beat', semi), ('detuned unison 1.5Hz', detu)]:
    for hp in [0, 200]:
        out = []
        for dref in [2, 4, 6, 12, 22]:
            r = detector(s, sc_hpf=hp, attack=12, dref=dref, rule='half')
            g = r['Ga'][FS:]
            out.append(f"Dref{dref}: {np.sqrt(np.mean(g**2)):.2f}/{np.percentile(g,99):.2f}/{g.max():.2f}")
        print(f" {name:24s} hpf {hp:3d}: " + " | ".join(out) + f"   Da 99pct {np.percentile(r['Da'][FS:],99):.2f}")
