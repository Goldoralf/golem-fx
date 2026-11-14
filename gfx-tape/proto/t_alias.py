import numpy as np, math
from ja import *
from scipy.signal.windows import blackmanharris
def alias(fs, f, A, cc, N0=16384):
    p = P(c=cc); N = N0*fs//192000; pre = 2000*fs//192000
    t = np.arange(N+pre)/fs; H = A*np.sin(2*np.pi*f*t)
    y,_ = run_H(p, H, hmax=k, cap=8); y = y[pre:]; y = y-np.mean(y)
    w = blackmanharris(N); S = np.abs(np.fft.rfft(y*w))**2; fr = np.fft.rfftfreq(N, 1/fs); bw = 6*fs/N
    fund = S[np.abs(fr-f) < bw].sum(); mask = (fr < 16000) & (fr > 20)
    for h in range(1, int(16000//f)+1): mask &= np.abs(fr-h*f) > bw
    tot = S[mask].sum(); pk = S[mask].max()*4  # ~ main-lobe sum
    return 10*math.log10(tot/fund+1e-30), 10*math.log10(pk/fund+1e-30)
for cc in (0.9, 0.5):
    print(f"c={cc}: non-harmonic energy below 16 kHz re fundamental (dB): total / largest line")
    for dB in (6, 12, 18):
        for f in (3011, 5011, 9173, 12007):
            r = [alias(fs, f, a*10**(dB/20), cc) for fs in (192000, 384000)]
            print(f"  H = {dB:+3d} dB re a, {f:6d} Hz | 4x: {r[0][0]:7.1f} / {r[0][1]:7.1f} | 8x: {r[1][0]:7.1f} / {r[1][1]:7.1f}")
