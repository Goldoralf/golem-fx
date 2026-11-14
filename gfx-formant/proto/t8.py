from live2 import *
import live2
print("== J. Instrument: chord E2 B2 E3 G#3 (decaying harmonics), L=2048, O=120, +4 st; wobble of L at partials, iterations, new partials")
n=int(2.0*fs); t=np.arange(n)/fs; x=np.zeros(n); parts=[]
for f in (82.41,123.47,164.81,207.65):
    for k in range(1,40):
        if k*f<9000: x+=np.cos(2*np.pi*k*f*t+k*k*0.3)/k**1.3; parts.append(k*f)
x*=0.05; parts=np.array(parts)
for name,kw in (("design asym 2048 fall 256",dict(Lfix=2048,order=120,mp=None)),("Hann 2048",dict(Lfix=2048,order=120,mp=1e9)),("asym 2048 fall 512",dict(Lfix=2048,order=120,mp=512/ (fs/100))),("design, maxit 30",dict(Lfix=2048,order=120,mp=None,maxit=30))):
    y,Ls,its,_=live2.live2(x,100.0,2**(4/12),**kw)
    Lp=np.array([np.interp(parts[parts<5000],np.arange(1025)*fs/NFFT,l) for l in Ls[100:]])
    # new partials: spectrum of y vs x, look for components not at input partials
    N=1<<15; w=np.blackman(N); Y=20*np.log10(abs(np.fft.rfft(y[20000:20000+N]*w))+1e-12); X=20*np.log10(abs(np.fft.rfft(x[20000:20000+N]*w))+1e-12)
    fb=np.arange(len(Y))*fs/N; mask=np.ones(len(Y),bool)
    for p in parts: mask&=abs(fb-p)>12
    mask&=(fb>60)&(fb<8000)
    print(f"  {name:26s}: L ptp at partials max {np.ptp(Lp,0).max():.2f} med {np.median(np.ptp(Lp,0)):.2f} dB | iters mean {its.mean():.1f} max {its.max()} | strongest non-partial component {Y[mask].max()-Y.max():.1f} dBc (input: {X[mask].max()-X.max():.1f})")
