from live import *
def source2(f0,n,vib=0.0):
    t=np.arange(n)/fs
    f=f0*2**(vib/1200*np.sin(2*np.pi*6*t)); ph=np.cumsum(f)/fs
    s=np.zeros(n); k=1
    while k*f0*1.04<20000:
        s+=np.cos(2*np.pi*k*ph)/k; k+=1
    return s,ph,f
def frame_env(x,s,f0,Lp=3.0,mp=None,delta=2.0,maxit=8,init=None,order=None,Lfix=None):
    T=fs/f0; L=int(np.clip(Lp*T if Lfix is None else Lfix,0.008*fs,NFFT))
    m=min(256,L//4) if mp is None else int(min(mp*T,L/2))
    w=awin(L,m); X=np.fft.rfft(x[s-L:s]*w/np.sum(w)*2,NFFT); lm=20*np.log10(abs(X)+1e-9)
    O=int(np.clip(0.5*fs/f0,24,400)) if order is None else order
    A0=pool(lm); V,it=te(A0,O,init=init,maxit=maxit,delta=delta)
    return V,lm,it
def live2(x,f0,alpha,Lp=3.0,mp=None,delta=2.0,maxit=8,warm=False,cutclamp=None,ntaps=NT,oracle=None,sched=1,order=None,Lfix=None,alpha_fn=None):
    n=len(x); y=np.zeros(n); f0a=np.full(n,float(f0)) if np.isscalar(f0) else f0
    hprev=np.zeros(ntaps); hprev[0]=1; hnew=hprev.copy(); q=[]; Vp=None; Ls=[]; its=[]; Vs=[]
    lam=(np.arange(HOP)+1)/HOP
    for s in range(0,n-HOP,HOP):
        hprev=hnew
        if len(q)>=sched: hnew=q.pop(0)
        lo=max(0,s-ntaps+1); seg=x[lo:s+HOP]
        a=sg.lfilter(hprev,1,seg)[s-lo:]; b=a if hnew is hprev else sg.lfilter(hnew,1,seg)[s-lo:]
        y[s:s+HOP]=(1-lam)*a+lam*b
        if s>=NFFT+HOP:
            V,lm,it=frame_env(x,s,f0a[s-1],Lp,mp,delta,maxit,Vp if warm else None,order,Lfix)
            Vp=V; E=up(V) if oracle is None else oracle(s)
            al=alpha if alpha_fn is None else alpha_fn(s)
            Ld=ratio(E,al,norm_P=10**(lm/10))
            if cutclamp is not None: Ld=np.maximum(Ld,-cutclamp)
            h,_=minphase(Ld,ntaps); q.append(h); Ls.append(Ld); its.append(it); Vs.append(V)
        else: q.append(hnew)
    return y,np.array(Ls),np.array(its),np.array(Vs)
def track(y,ph,kmax,win_ms=20):
    """harmonic amplitudes over time (dB), heterodyne with known phase"""
    L=int(win_ms*fs/1000)|1; w=np.hanning(L); w/=w.sum(); out=[]
    for k in range(1,kmax+1):
        z=y*np.exp(-2j*np.pi*k*ph); a=np.convolve(z,w,'same'); out.append(20*np.log10(2*abs(a)+1e-12))
    return np.array(out)
def true_env_db(F,f,f0):
    return 20*np.log10((f0/np.maximum(f,1))*tract_resp(F,np.maximum(f,1)))
