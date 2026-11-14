from live2 import *
import live2
n=int(2.5*fs); t=np.arange(n)/fs
def chord(notes,dec=1.3):
    x=np.zeros(n); parts=[]
    for f in notes:
        for k in range(1,60):
            if k*f<9000: x+=np.cos(2*np.pi*k*f*t+k*k*0.3)/k**dec; parts.append(k*f)
    return x*0.05,np.array(parts)
def run(x,alpha,Lfix,order,mp,tau_ms,maxit=8):
    y=np.zeros(n); hprev=np.zeros(NT); hprev[0]=1; hnew=hprev; pend=None; Vs=None; Ls=[]; lam=(np.arange(HOP)+1)/HOP
    a_s=0.0 if tau_ms==0 else np.exp(-HOP/(tau_ms*fs/1000))
    for s in range(0,n-HOP,HOP):
        hprev=hnew
        if pend is not None: hnew=pend
        lo=max(0,s-NT+1); seg=x[lo:s+HOP]
        a=sg.lfilter(hprev,1,seg)[s-lo:]; b=sg.lfilter(hnew,1,seg)[s-lo:]
        y[s:s+HOP]=(1-lam)*a+lam*b
        if s>=NFFT+HOP:
            V,lm,it=frame_env(x,s,100.0,mp=mp,order=order,Lfix=Lfix,maxit=maxit)
            Vs=V if Vs is None else a_s*Vs+(1-a_s)*V
            Ld=ratio(up(Vs),alpha,norm_P=10**(lm/10)); pend,_=minphase(Ld); Ls.append(Ld)
    return y,np.array(Ls)
def newpart(x,y,parts,guard=3):
    N=1<<16; w=np.blackman(N); s0=int(0.9*fs)
    Y=20*np.log10(abs(np.fft.rfft(y[s0:s0+N]*w))+1e-12); X=20*np.log10(abs(np.fft.rfft(x[s0:s0+N]*w))+1e-12)
    fb=np.arange(len(Y))*fs/N; mask=(fb>60)&(fb<8000)
    for p in parts: mask&=abs(fb-p)>guard
    return Y[mask].max()-Y.max(), X[mask].max()-X.max()
for cname,notes in (("E2 B2 E3 G#3",(82.41,123.47,164.81,207.65)),("single E2",(82.41,)),("A2 E3 (fifth)",(110.0,164.81))):
    x,parts=chord(notes)
    print(f"== {cname}, +4 st")
    for name,kw in (("design: asym 2048/256, O 120",dict(Lfix=2048,order=120,mp=None,tau_ms=0)),
                    ("Hann 2048, O 120",dict(Lfix=2048,order=120,mp=1e9,tau_ms=0)),
                    ("Hann 2048, O 60",dict(Lfix=2048,order=60,mp=1e9,tau_ms=0)),
                    ("Hann 2048, O 120, smooth 50 ms",dict(Lfix=2048,order=120,mp=1e9,tau_ms=50)),
                    ("Hann 2048, O 120, smooth 150 ms",dict(Lfix=2048,order=120,mp=1e9,tau_ms=150)),
                    ("Hann 2048, O 60, smooth 150 ms",dict(Lfix=2048,order=60,mp=1e9,tau_ms=150)),
                    ("asym 2048/256, O 60, smooth 150 ms",dict(Lfix=2048,order=60,mp=None,tau_ms=150))):
        y,Ls=run(x,2**(4/12),**kw)
        Lp=np.array([np.interp(parts[parts<5000],np.arange(1025)*fs/NFFT,l) for l in Ls[150:]])
        dd=np.abs(np.diff(Lp,axis=0))
        a,b=newpart(x,y,parts)
        print(f"  {name:36s}: hop-to-hop |dL| at partials: max {dd.max():5.2f} mean {dd.mean():4.2f} dB | strongest new component {a:6.1f} dBc (input {b:.1f})")
