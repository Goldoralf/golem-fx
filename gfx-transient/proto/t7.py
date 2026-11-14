import numpy as np
from core import *
def sus(Ds, S, dref=12, D0=0.0): return S * (1 - np.exp(-np.maximum(0, Ds - D0) / dref))
print("== guitar note 110 Hz (KS, 8 dB/s fundamental, faster partials), sus_time 2000, sustain +10: G_sus at t")
s = place(5.0, [(0.1, ks_string(110, 4.9, decay_db_s=8))])
for st in [300, 1000, 2000, 3000]:
    r = detector(s, sus_time=st, thresh=-96, floor_win=90)
    for D0 in [0, 1.5]:
        g = sus(r['Ds'], 10, D0=D0)
        print(f"  sus_time {st} D0 {D0}: Ds at .15/.3/.6/1.1/2.1/4.1 s: " + " ".join(f"{r['Ds'][int((0.1+a)*FS)]:.1f}" for a in [.05, .2, .5, 1, 2, 4]) + " | G_sus: " + " ".join(f"{g[int((0.1+a)*FS)]:.1f}" for a in [.05, .2, .5, 1, 2, 4]))
print("== snare 60 dB/s and 200 dB/s, sustain +12 / -12, sus_time 1000 / 250 / 60")
for dec in [60, 200]:
    s = place(1.2, [(0.1, snare(1.0, decay_db_s=dec))])
    for st in [1000, 250, 60]:
        r = detector(s, sus_time=st, sc_hpf=120, rule='half')
        g = sus(r['Ds'], 12, D0=1.5)
        print(f"  decay {dec} sus_time {st}: share at 20/50/100/200/400 ms: " + " ".join(f"{g[int((0.1+a)*FS)]/12:.2f}" for a in [.02, .05, .1, .2, .4]))
