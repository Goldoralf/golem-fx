import numpy as np, math
from ja import *
p = P(); fs = 192000
alt = np.array([1.,-1.]*2000)
tests = [("nyq alt 8a", 8*a*alt), ("nyq alt 4a", 4*a*alt), ("nyq alt 2a", 2*a*alt), ("nyq alt 1a", a*alt),
         ("square5k 8a", 8*a*np.sign(np.sin(2*np.pi*5000*np.arange(4000)/fs+.1))),
         ("white 8a", 8*a*np.random.default_rng(1).uniform(-1,1,4000)),
         ("sine 20k 8a", 8*a*np.sin(2*np.pi*20000*np.arange(4000)/fs)),
         ("sine 90k 8a", 8*a*np.sin(2*np.pi*90000*np.arange(4000)/fs)),
         ("sine 90k 1a", 1*a*np.sin(2*np.pi*90000*np.arange(4000)/fs)),
         ("sine 90k .1a", .1*a*np.sin(2*np.pi*90000*np.arange(4000)/fs))]
for name, H in tests:
    ref,_ = run_H(p, H, 64)
    def s(y): return "NaN" if np.any(~np.isfinite(y)) else f"max {np.max(abs(y))/Ms:6.2f} err {100*np.sqrt(np.mean((y-ref)**2))/Ms:7.2f}%"
    res = [f"{name:13s}", "time:", s(run_T(p, H, fs))]
    for n in (1, 2, 4):
        res += [f"| H n={n}:", s(run_H(p, H, n)[0])]
    y,nn = run_H(p, H, hmax=k/2, cap=16); res += [f"| H hmax=k/2 cap16 ({nn:.1f}):", s(y)]
    print(*res)
print("dH/k of a 8a step:", 16*a/k, " RK4 stability limit on (1-c)dH/k ~2.78")
