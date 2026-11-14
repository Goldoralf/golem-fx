import numpy as np
from core import *
x = place(3.0, [(0.1, ks_string(82.4, 1.0)), (0.6, ks_string(110, 1.0, amp=0.35)), (1.2, snare(0.5)), (1.5, snare(0.5, amp=0.2)), (1.9, ks_string(330, 1.0, amp=0.5))])
print("== A detail: attack only / sustain only, thresh -66")
r0a = detector(x, attack=12, thresh=-96); r0s = detector(x, sustain=12, thresh=-96); r0c = detector(x, sustain=-12, thresh=-96)
for lv in [-12, -24, -36]:
    xa = x * 10 ** (lv / 20)
    ra = detector(xa, attack=12); rs = detector(xa, sustain=12); rc = detector(xa, sustain=-12)
    i = np.argmax(np.abs(ra['Ga'] - r0a['Ga']))
    print(f" lv {lv}: attack diff max {np.max(np.abs(ra['Ga']-r0a['Ga'])):.2f} dB at t={i/FS:.3f}s; sustain+ diff {np.max(np.abs(rs['Gs']-r0s['Gs'])):.2f}; sustain- diff {np.max(np.abs(rc['Gs']-r0c['Gs'])):.2f}")
# level in dBFS of signal at where the max sustain cut differs
print("== half rule, fundamental near hpf/2, KS strings (sustained part 0.3-0.8 s), attack 24 sustain 24")
for f, hp in [(41.2, 80), (41.2, 70), (55, 100), (82.4, 160), (82.4, 120), (73.4, 120), (61.7, 120), (98, 200), (110, 200)]:
    for order in [2, 4]:
        for rule in ['gate', 'half']:
            for asym in [0.0, 0.7]:
                s = ks_string(f, 1.0, decay_db_s=6, asym=asym, pick=0.13)
                r = detector(s, sc_hpf=hp, hpf_order=order, attack=24, sustain=0, rule=rule)
                g = r['Ga'][int(0.3*FS):int(0.8*FS)]
                print(f" f {f:5.1f} hpf {hp} ord {order} rule {rule} asym {asym}: W {r['W']*1e3:.1f} ms  G_att p-p {g.max()-g.min():.3f} dB")
