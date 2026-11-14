import numpy as np
exec(open('comp.py').read().split("for q in [2.87, 2.4, 2.0]:")[0])
def pgs(t, B, sweeps, g0=None):
    g = np.zeros(16) if g0 is None else g0.copy()
    for _ in range(sweeps):
        for k in range(16):
            g[k] = max(0.0, t[k] - (B[k]@g - g[k]))
    return g
for q in [2.87, 2.4, 2.0, 1.7]:
    B = Bmat(q, 12.0)
    print(f"==== Q {q}")
    for name, t in targets.items():
        s = ""
        for sw in [1, 2, 4]:
            g = pgs(t, B, sw)
            r = -resp(g, q, f); rc = -resp(g, q, fk)
            act = g > 0
            m = (f>=fk[0])&(f<=fk[-1])
            tc = np.interp(np.log(f), np.log(fk), t)
            s += f" | {sw} sw: max {r.max():5.1f} ctr-err(active) {np.abs(rc-t)[act].max():4.1f} under {(r[m]-tc[m]).min():+5.1f} over {(r[m]-tc[m]).max():+5.1f}"
        print(f"{name:11s}{s}")
