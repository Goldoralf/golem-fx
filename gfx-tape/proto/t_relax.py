import numpy as np, math
from ja import *
from t_level import harm
fs = 48000
print("D. change of the steady waveform by a relaxation, rms(diff)/rms (%) and THD none -> tau")
for cc in (0.9, 0.5):
    p = P(c=cc)
    for f in (41, 82, 200, 1000):
        N = int(fs*max(0.4, 12/f)); t = np.arange(N)/fs; H = a*10**(4/20)*np.sin(2*np.pi*f*t)
        y0,_ = run_H(p, H, hmax=k, cap=8); s = slice(N//2, N)
        h0 = harm(y0[s]-np.mean(y0[s]), f, fs); thd0 = 100*math.sqrt(sum(x*x for x in h0[1:]))/h0[0]
        row = f"c={cc} {f:5d} Hz THD {thd0:5.2f}% |"
        for tau in (0.05, 0.25, 1.0):
            y,_ = run_H(p, H, hmax=k, cap=8, tau=tau, fs=fs)
            h = harm(y[s]-np.mean(y[s]), f, fs); thd = 100*math.sqrt(sum(x*x for x in h[1:]))/h[0]
            row += f" tau {tau*1e3:4.0f} ms: diff {100*np.std(y[s]-y0[s])/np.std(y0[s]):5.2f}% THD {thd:5.2f}% gain {20*math.log10(h[0]/h0[0]):+.2f} dB |"
        print(row)
print("\nE. loud 1 kHz burst (+12 dB re a, stops at a peak of H... then H=0), then quiet 1 kHz at -30 dB re a")
f = 1000
n1 = int(fs*0.2) + 12  # stop right at a positive peak (quarter period = 12 samples)
t1 = np.arange(n1)/fs; t2 = np.arange(int(fs*1.0))/fs
H = np.concatenate([a*10**(12/20)*np.sin(2*np.pi*f*t1), np.zeros(int(fs*0.05)), a*10**(-30/20)*np.sin(2*np.pi*f*t2)])
q0 = n1 + int(fs*0.05)
for cc in (0.9, 0.5, 0.17):
    p = P(c=cc); slope = cc*p.Ms/(3*p.a-cc*p.alpha*p.Ms)
    for tau in (None, 0.05, 0.25):
        y,_ = run_H(p, H, hmax=k, cap=8, tau=tau, fs=fs)
        rem = y[n1+100]/p.Ms
        row = f"c={cc} tau={tau}: remanence after stop {rem:+.3f} Ms ({20*math.log10(abs(rem)*p.Ms/(slope*a*10**(12/20))+1e-12):+.1f} dB re linear loud peak) | quiet tone gain/H2 at"
        for t0 in (0.0, 0.1, 0.5):
            s = slice(q0+int(fs*t0), q0+int(fs*t0)+int(fs*0.05)); h = harm(y[s]-np.mean(y[s]), f, fs)
            row += f" {t0:.1f}s: {20*math.log10(h[0]/(slope*a*10**(-30/20))):+.2f} dB, H2 {20*math.log10(h[1]/h[0]+1e-12):.0f} dB, DC {np.mean(y[s])/p.Ms:+.3f};"
        print(row)
