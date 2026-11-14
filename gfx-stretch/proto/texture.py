import numpy as np, sys
from scipy.signal import hilbert
fs=48000
rng=np.random.default_rng(1)
def paul(x, N, amount, T):
    B=N//2
    i=np.arange(N)
    w=0.53836-0.46164*np.cos(2*np.pi*i/(N+1.0))
    nfr=int(T*fs/B)
    out=np.zeros(nfr*B)
    old=np.zeros(N)
    ii=np.arange(B); tmp=np.pi/B
    a=0.5+0.5*np.cos(ii*tmp)
    corr=(0.853553390593-(1-0.853553390593)*np.cos(ii*2*tmp))*2.0
    p=0.0
    for m in range(nfr):
        s=int(p)%(len(x)-N)
        X=np.abs(np.fft.rfft(x[s:s+N]*w)); X[0]=0; X[-1]=0
        ph=rng.integers(0,32768,len(X))*np.pi/16384
        y=np.fft.irfft(X*np.exp(1j*ph),N)
        out[m*B:(m+1)*B]=(y[B:]*(1-a)+old[:B]*a)*corr
        old=y
        p+=B/amount
    return out
def ours(x, N, amount, T, ov=4, gain=None):
    H=N//ov
    i=np.arange(N)
    w=0.5-0.5*np.cos(2*np.pi*i/N)
    nfr=int(T*fs/H)
    out=np.zeros(nfr*H+N)
    p=0.0
    for m in range(nfr):
        s=int(p)%(len(x)-N)
        X=np.abs(np.fft.rfft(x[s:s+N]*w)); X[0]=0
        ph=rng.uniform(0,2*np.pi,len(X))
        y=np.fft.irfft(X*np.exp(1j*ph),N)
        out[m*H:m*H+N]+=y*w
        p+=H/amount
    g = 1/np.sqrt(0.375*np.sum(w**2)/H) if gain is None else gain
    return out[N:nfr*H]*g
def stats(name,y,x):
    P=np.mean(x**2)
    lvl=10*np.log10(np.mean(y**2)/P)
    L=int(0.05*fs); n=len(y)//L
    r=np.sqrt(np.mean(y[:n*L].reshape(n,L)**2,axis=1))
    rdb=20*np.log10(r/np.sqrt(np.mean(y**2)))
    crest=20*np.log10(np.max(np.abs(y))/np.sqrt(np.mean(y**2)))
    print(f"  {name:28s} level {lvl:+6.2f} dB  50ms-RMS std {np.std(rdb):5.2f} dB  min {rdb.min():6.1f}  p5 {np.percentile(rdb,5):6.1f}  crest {crest:4.1f} dB")
    return r
def modspec(y, N, label):
    # envelope power modulation spectrum: energy at frame rate fs/B
    L=480; n=len(y)//L
    e=np.mean(y[:n*L].reshape(n,L)**2,axis=1); e=e/np.mean(e)-1
    E=np.abs(np.fft.rfft(e*np.hanning(len(e))))**2
    f=np.fft.rfftfreq(len(e),L/fs)
    out=[]
    for fr in (fs/N, 2*fs/N, 4*fs/N):
        k=np.argmin(abs(f-fr)); pk=E[max(k-2,0):k+3].max(); med=np.median(E[(f>fr*0.6)&(f<fr*1.6)])
        out.append(f"{fr:.2f}Hz {10*np.log10(pk/med):5.1f}dB")
    print(f"    modulation peak/median at {label}: "+"  ".join(out))
T=120
t=np.arange(int(40*fs))/fs
sig={
 'sine 440':0.5*np.sin(2*np.pi*440*t),
 'chord E':0.15*sum(np.sin(2*np.pi*f*h*t+h)/h for f in (82.4,123.5,164.8,207.7,246.9,329.6) for h in range(1,9)),
 'noise':0.2*rng.standard_normal(len(t)),
}
for N in (32768, 4096):
    for name,x in sig.items():
        print(f"N={N} {name}")
        for amt in (8,):
            y=paul(x,N,amt,T); stats(f"paulstretch ov2 x{amt}",y,x); modspec(y,N,"fs/N,2fs/N,4fs/N")
            y=ours(x,N,amt,T,4); stats(f"ours hann ov4 x{amt}",y,x); modspec(y,N,"fs/N,2fs/N,4fs/N")
            y=ours(x,N,amt,T,2); stats(f"hann ov2 (sum w2 not const)",y,x); modspec(y,N,"")
