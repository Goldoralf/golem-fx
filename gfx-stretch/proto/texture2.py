import numpy as np
from scipy.signal import hilbert
exec(open('texture.py').read().split("T=120")[0])
t=np.arange(int(40*fs))/fs
x=0.5*np.sin(2*np.pi*440*t)
N=8192
for name,fn in (("paul ov2",lambda: paul(x,N,8,400)),("ours ov4",lambda: ours(x,N,8,400,4))):
    y=fn()[4*N:]
    e=np.abs(hilbert(y[:2**22]))**2
    e=e[::64]; e=e/e.mean()
    d=e-1
    ac=np.fft.irfft(np.abs(np.fft.rfft(d))**2); ac/=ac[0]
    lag=np.argmax(ac<np.exp(-1))*64/N
    lag0=np.argmax(ac<0.0)*64/N
    Y=np.abs(np.fft.rfft(y[:2**22]*np.hanning(2**22)))**2
    f=np.fft.rfftfreq(2**22,1/fs)
    c=np.cumsum(Y)/Y.sum()
    bw=lambda q: (f[np.searchsorted(c,1-q/2)]-f[np.searchsorted(c,q/2)])/(fs/N)
    print(f"{name}: power var {np.var(e):.2f} (exp. dist = 1)  frac below -10 dB {np.mean(e<0.1):.3f}  below -20 dB {np.mean(e<0.01):.4f}  env corr 1/e {lag:.3f} N, zero {lag0:.2f} N   bandwidth 50% {bw(0.5):.2f} bins, 90% {bw(0.1):.2f} bins, 99% {bw(0.01):.2f} bins")
# cloud level law: identity phases + uniform offset ±pi*cloud per bin per hop, Hann/Hann hop N/4, amount 1
print("cloud law (amount 1, Hann pair hop N/4; power relative to input)")
N=4096;H=N//4;w=0.5-0.5*np.cos(2*np.pi*np.arange(N)/N)
sigs={'noise':0.2*rng.standard_normal(fs*20),'chord':0.15*sum(np.sin(2*np.pi*f*h*t[:fs*20]+h)/h for f in (82.4,123.5,164.8,207.7,246.9,329.6) for h in range(1,9))}
for nm,s in sigs.items():
    for c in (0,0.1,0.25,0.5,0.75,1.0):
        out=np.zeros(len(s)+N)
        for m in range((len(s)-N)//H):
            X=np.fft.rfft(s[m*H:m*H+N]*w)
            X=X*np.exp(1j*rng.uniform(-np.pi*c,np.pi*c,len(X)))
            out[m*H:m*H+N]+=np.fft.irfft(X,N)*w
        P=np.mean(out[N:len(s)-N]**2)/np.mean(s**2)
        sc=np.sinc(c)
        print(f"  {nm} cloud {c:4.2f}: measured {P:.3f}   README rho=sinc {0.5625+sc*1.6875:.3f} ({10*np.log10((0.5625+sc*1.6875)/P):+.2f} dB)   rho=sinc^2 {0.5625+sc*sc*1.6875:.3f} ({10*np.log10((0.5625+sc*sc*1.6875)/P):+.2f} dB)")
