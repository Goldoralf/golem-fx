import numpy as np, sys
from core import *
np.set_printoptions(precision=2, suppress=True, linewidth=200)
print("== A level independence (guitar re-picks + snare), attack+12 sustain+12")
x = place(3.0, [(0.1, ks_string(82.4, 1.0)), (0.6, ks_string(110, 1.0, amp=0.35)), (1.2, snare(0.5)), (1.5, snare(0.5, amp=0.2)), (1.9, ks_string(330, 1.0, amp=0.5))])
ref = None
for lv in [0, -12, -24, -36]:
    for th in [-96, -66]:
        r = detector(x * 10 ** (lv / 20), attack=12, sustain=12, thresh=th)
        if lv == 0 and th == -96: ref = r['G']
        print(f" level {lv:4d} thresh {th}: max |G - G0| = {np.max(np.abs(r['G'] - ref)):.3f} dB   (G range {r['G'].min():.2f}..{r['G'].max():.2f})")

print("== B steady notes: gain p-p after 300 ms, attack 24 sustain 24, sens 50 / 100")
t = np.arange(int(1.5 * FS)) / FS
def asym_wave(f):  # bass-like asymmetric wave: one big peak per period
    return 0.5 * (np.sin(2*np.pi*f*t) + 0.6*np.sin(2*np.pi*2*f*t + 0.9) + 0.35*np.sin(2*np.pi*3*f*t + 0.4)) / 1.6
for name, sig in [('sine31', 0.5*np.sin(2*np.pi*31*t)), ('sine41', 0.5*np.sin(2*np.pi*41.2*t)), ('asym31', asym_wave(31)), ('asym41', asym_wave(41.2)), ('asym82', asym_wave(82.4))]:
    for hp in [0, 70, 120]:
        for rule in ['gate', 'half']:
            for sens in [50, 100]:
                r = detector(sig, sc_hpf=hp, attack=24, sustain=24, sens=sens, rule=rule)
                g = r['G'][int(0.3*FS):]
                d = r['Da'][int(0.3*FS):]
                if (g.max()-g.min()) > 1e-6 or (hp == 0 and rule=='gate'):
                    print(f" {name} hpf {hp:3d} rule {rule} W {r['W']*1e3:.1f} ms sens {sens}: gain p-p {g.max()-g.min():.3f} dB, Da max {d.max():.2f} dB")
