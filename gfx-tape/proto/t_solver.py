import numpy as np, math, sys
from ja import *
p = P(); fs = 192000; R = 32
print("tone accuracy at 192 kHz: rms error / Ms (%), vs continuous reference (32x finer, H-domain)")
print("f  A/a | time-RK4(Chow)  H-RK4 n=1  H n<=2(hmax=a/4)  H n<=2(hmax=k/2) avg-n")
for f in (1000, 5000, 10000, 15000, 20000):
    for A in (1, 4, 8):
        N = int(fs*0.02)
        tf = np.arange(N*R)/(fs*R); Hf = A*a*np.sin(2*np.pi*f*tf)
        ref, _ = run_H(p, Hf, 1); ref = ref[R-1::R]   # sample aligned: fine index R-1 -> t=(R-1)/(fs R)... use shift below
        t = np.arange(N)/fs
        # coarse samples at t_i = (i+1)/fs - 1/(fs R)  to align exactly with ref
        tc = (np.arange(N)+1)/fs - 1/(fs*R); H = A*a*np.sin(2*np.pi*f*tc)
        e = lambda y: 100*np.sqrt(np.nanmean((y[N//2:]-ref[N//2:])**2))/Ms if np.all(np.isfinite(y)) else float('nan')
        yT = run_T(p, H, fs); y1, _ = run_H(p, H, 1); y2, n2 = run_H(p, H, hmax=a/4); y3, n3 = run_H(p, H, hmax=k/2)
        print(f"{f:6d} {A} | {e(yT):8.3f} {e(y1):8.3f} {e(y2):8.3f} ({n2:.2f}) {e(y3):8.3f} ({n3:.2f})")
print("\nstability: worst-case steps")
for name, H in (("nyquist alternation +-8a", 8*a*np.array([1,-1]*2000.)), ("square 5k +-8a", 8*a*np.sign(np.sin(2*np.pi*5000*np.arange(4000)/fs+.1))),
                ("nyquist +-4a", 4*a*np.array([1,-1]*2000.)), ("nyquist +-2a", 2*a*np.array([1,-1]*2000.)), ("white +-8a", 8*a*np.random.default_rng(1).uniform(-1,1,4000))):
    yT = run_T(p, H, fs); res = [name, "time:", "NaN" if np.any(~np.isfinite(yT)) else f"max|M|/Ms {np.max(abs(yT))/Ms:.2f}"]
    for n in (1, 2, 4, 8):
        y, _ = run_H(p, H, n); res += [f"H n={n}:", "NaN" if np.any(~np.isfinite(y)) else f"{np.max(abs(y))/Ms:.2f}"]
    print(*res)
