import numpy as np
np.set_printoptions(precision=1, suppress=True, linewidth=220)
def bell_db(f, f0, q, g):
    a = 10**(g/40); w = f/f0
    num = (1-w*w)**2 + (a/q*w)**2; den = (1-w*w)**2 + (w/(a*q))**2
    return 10*np.log10(num/den)
fk = 80*2**(np.arange(16)/2)
f = np.geomspace(60, 18000, 3000)
def resp(g, q, ff): return sum(bell_db(ff, fk[k], q, -g[k]) for k in range(16) if g[k]>0) + 0*ff
def Bmat(q, G): return np.array([[bell_db(fj, f0, q, -G)/-G for f0 in fk] for fj in fk])
targets = {}
for D in [6, 12, 18, 30]:
    targets[f"uniform {D}"] = np.full(16, float(D))
    t = np.zeros(16); t[7]=D; targets[f"single {D}"] = t
    w = np.clip(1 - np.abs(np.arange(16)-8)/5, 0, 1); targets[f"tri {D}"] = D*w   # voice-like bump
    t = np.zeros(16); t[4:8]=D; t[11:13]=D*0.5; targets[f"blocks {D}"] = t
    rng = np.random.default_rng(1); targets[f"random {D}"] = D*rng.random(16)
def score(g, t, q):
    r = -resp(g, q, f); rc = -resp(g, q, fk)
    # target curve: linear interpolation in log f between centres
    tc = np.interp(np.log(f), np.log(fk), t)
    lo = np.minimum.reduce([np.interp(np.log(f), np.log(fk), np.minimum(t, np.roll(t,1))), np.interp(np.log(f), np.log(fk), np.minimum(t, np.roll(t,-1)))])
    m = (f>=fk[0])&(f<=fk[-1])
    return np.abs(rc-t).max(), (r[m]-tc[m]).max(), (r[m]-tc[m]).min(), r.max()
for q in [2.87, 2.4, 2.0]:
    C = np.linalg.inv(Bmat(q, 12.0))
    print(f"==== Q {q}")
    for name, t in targets.items():
        out = []
        # raw
        out.append(("raw", t.copy()))
        # fixed inverse, clamp
        out.append(("inv", np.clip(C@t, 0, 60)))
        # inverse + 1 and 2 Newton-like corrections on centre error
        g = np.clip(C@t, 0, 60)
        for it in range(2):
            e = t - (-resp(g, q, fk)); g = np.clip(g + C@e, 0, 60)
            out.append((f"inv+{it+1}", g.copy()))
        s = "  ".join(f"{n}: ctr {a:4.1f} over {b:+5.1f} under {c:+5.1f} max {d:4.1f}" for n,g in out for a,b,c,d in [score(g,t,q)])
        print(f"{name:11s} {s}")
