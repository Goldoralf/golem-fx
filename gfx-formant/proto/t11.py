from live2 import *
n=int(2.5*fs)
def env_exact(x,s,f0,k=3,minms=8.0,delta=2.0,maxit=8):
    T=fs/f0; kk=max(k,int(np.ceil(minms*fs/1000/T))); Lf=min(kk*T,NFFT)
    if Lf==NFFT: Lf=np.floor(NFFT/T)*T
    m=int(np.ceil(Lf)); tau=np.arange(m)+0.5+(Lf-m); w=np.where(tau>0,0.5-0.5*np.cos(2*np.pi*tau/Lf),0)
    X=np.fft.rfft(x[s-m:s]*w/np.sum(w)*2,NFFT); lm=20*np.log10(abs(X)+1e-9)
    O=int(np.clip(0.5*fs/f0,24,400)); V,it=te(pool(lm),O,maxit=maxit,delta=delta); return V,lm,it,Lf
def run(x,f0a,alpha,tau_ms,k=3,f0err=0.0):
    y=np.zeros(n); hprev=np.zeros(NT); hprev[0]=1; hnew=hprev; pend=None; Vs=None; lam=(np.arange(HOP)+1)/HOP
    a_s=0.0 if tau_ms==0 else np.exp(-HOP/(tau_ms*fs/1000))
    for s in range(0,n-HOP,HOP):
        hprev=hnew
        if pend is not None: hnew=pend
        lo=max(0,s-NT+1); seg=x[lo:s+HOP]
        a=sg.lfilter(hprev,1,seg)[s-lo:]; b=sg.lfilter(hnew,1,seg)[s-lo:]
        y[s:s+HOP]=(1-lam)*a+lam*b
        if s>=NFFT+HOP:
            f=f0a[s-int(0.01*fs)]*(1+f0err)   # detector lags ~10 ms
            V,lm,it,_=env_exact(x,s,f,k)
            Vs=V if Vs is None else a_s*Vs+(1-a_s)*V
            pend,_=minphase(ratio(up(Vs),alpha,norm_P=10**(lm/10)))
    return y
def nonharm(y,f0):
    N=1<<16; w=np.blackman(N); s0=int(0.9*fs); Y=20*np.log10(abs(np.fft.rfft(y[s0:s0+N]*w))+1e-12)
    fb=np.arange(len(Y))*fs/N; k=np.round(fb/f0); mask=(abs(fb-k*f0)>4)&(fb>60)&(fb<8000)
    return Y[mask].max()-Y.max()
print("steady /a/: strongest non-harmonic component (dBc), +4/-4 st; f0 = 98.3, 147.1, 221.7, 397")
for name,tau,k,err in (("exact 3T Hann",0,3,0),("exact 3T, f0 err +1%",0,3,0.01),("exact 3T, f0 err +3%",0,3,0.03),("exact 3T + 10 ms",10,3,0),("exact 3T + 10 ms, err 1%",10,3,0.01),("exact 4T",0,4,0),("exact 2T",0,2,0),("exact 2T + 10ms err 1%",10,2,0.01)):
    row=[]
    for f0 in (98.3,147.1,221.7,397.0):
        s,ph,fi=source2(f0,n); x=tract(s,VOW['a'])*0.1
        row.append(f"{nonharm(run(x,fi,2**(4/12),tau,k,err),f0):6.1f}/{nonharm(run(x,fi,2**(-4/12),tau,k,err),f0):6.1f}")
    print(f"  {name:26s} "+"  ".join(row))
