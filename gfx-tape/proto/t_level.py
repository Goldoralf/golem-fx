import numpy as np, math
from ja import *
fs = 192000
def harm(y, f, fs, nh=9):
    N = len(y); t = np.arange(N)/fs; w = np.hanning(N)
    return [abs(2*np.sum(w*y*np.exp(-2j*np.pi*h*f*t))/np.sum(w)) for h in range(1, nh+1)]
f = 1000; N = int(fs*0.03)
t = np.arange(N)/fs
# stability / accuracy check of the proposed rule n = min(8, ceil(|dH|/k))
p = P()
for name, H in (("nyq alt 8a", 8*a*np.array([1.,-1.]*2000)), ("white 8a", 8*a*np.random.default_rng(1).uniform(-1,1,4000)), ("sine 15k 8a", 8*a*np.sin(2*np.pi*15000*np.arange(4000)/fs)), ("sine 10k 4a", 4*a*np.sin(2*np.pi*10000*np.arange(4000)/fs))):
    ref,_ = run_H(p, H, 64); y,nn = run_H(p, H, hmax=k, cap=8)
    print(f"rule hmax=k cap 8: {name:12s} avg n {nn:.2f} max {np.max(abs(y))/Ms:.2f} err {100*np.sqrt(np.mean((y-ref)**2))/Ms:.3f}%")
for cc in (0.17, 0.5, 0.7, 0.9, 0.98):
    p = P(c=cc); slope = p.Ms/(3*p.a - p.alpha*p.Ms)
    print(f"\nc={cc}: level(dB re a) | gain re anhysteretic slope (dB) | THD % | H3 dB | H2 dB")
    for dB in (-60, -40, -30, -20, -12, -6, 0, 6, 12, 18):
        A = a*10**(dB/20); H = A*np.sin(2*np.pi*f*t)
        y,_ = run_H(p, H, hmax=k, cap=8); y = y[N//3:]
        h = harm(y - np.mean(y), f, fs)
        thd = 100*math.sqrt(sum(x*x for x in h[1:]))/h[0]
        print(f"  {dB:4d} | {20*math.log10(h[0]/(A*slope)):7.2f} | {thd:7.3f} | {20*math.log10(h[2]/h[0]+1e-12):7.1f} | {20*math.log10(h[1]/h[0]+1e-12):7.1f}")
