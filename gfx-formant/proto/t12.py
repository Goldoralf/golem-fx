from live2 import *
n=int(2.5*fs); fg=np.arange(1025)*fs/NFFT
def spec(x,s,f0,k=3,minms=8.0):
    T=fs/f0; kk=max(k,int(np.ceil(minms*fs/1000/T))); Lf=min(kk*T,NFFT)
    if Lf==NFFT: Lf=np.floor(NFFT/T)*T
    m=int(np.ceil(Lf)); tau=np.arange(m)+0.5+(Lf-m); w=np.where(tau>0,0.5-0.5*np.cos(2*np.pi*tau/Lf),0)
    X=np.fft.rfft(x[s-m:s]*w/np.sum(w)*2,NFFT); return 20*np.log10(abs(X)+1e-9)
def harm_env(lm,f0,lift=True):
    """per-harmonic-band peak (parabolic), linear interpolation in dB, returned on 1025 bins"""
    b0=f0*NFFT/fs; K=int(1024/b0-0.5); pf=[];pv=[]
    for k in range(1,K+1):
        lo=int(np.ceil((k-0.5)*b0)); hi=min(int(np.floor((k+0.5)*b0)),1023)
        if hi<=lo: break
        i=lo+int(np.argmax(lm[lo:hi+1])); i=min(max(i,1),1023)
        y0,y1,y2=lm[i-1],lm[i],lm[i+1]; den=y0-2*y1+y2
        off=0.5*(y0-y2)/den if (den<0 and y1>=y0 and y1>=y2) else 0.0
        pf.append(i+off); pv.append(y1-0.25*(y0-y2)*off)
    E=np.interp(np.arange(1025),pf,pv)
    if lift:
        O=int(np.clip(0.5*fs/f0,24,400)); c=np.fft.irfft(E,NFFT); q=np.arange(NFFT); q=np.minimum(q,NFFT-q)
        c*=np.where(q<=O,0.5+0.5*np.cos(np.pi*q/max(O,1)) ,0) ; E=np.fft.rfft(c).real
    return E
def run(x,f0a,alpha,mode,tau_ms=0,err=0.0,ret=False):
    y=np.zeros(n); hprev=np.zeros(NT); hprev[0]=1; hnew=hprev; pend=None; Es=None; lam=(np.arange(HOP)+1)/HOP; Ls=[]
    a_s=0.0 if tau_ms==0 else np.exp(-HOP/(tau_ms*fs/1000))
    for s in range(0,n-HOP,HOP):
        hprev=hnew
        if pend is not None: hnew=pend
        lo=max(0,s-NT+1); seg=x[lo:s+HOP]
        a=sg.lfilter(hprev,1,seg)[s-lo:]; b=sg.lfilter(hnew,1,seg)[s-lo:]
        y[s:s+HOP]=(1-lam)*a+lam*b
        if s>=NFFT+HOP:
            f=f0a[s-int(0.01*fs)]*(1+err); lm=spec(x,s,f)
            if mode=='te': E=up(te(pool(lm),int(np.clip(0.5*fs/f,24,400)))[0])
            else: E=harm_env(lm,f,lift=(mode=='hl'))
            Es=E if Es is None else a_s*Es+(1-a_s)*E
            L=ratio(Es,alpha,norm_P=10**(lm/10)); pend,_=minphase(L); Ls.append(L)
    return (y,np.array(Ls)) if ret else y
def nonharm(y,f0):
    N=1<<16; w=np.blackman(N); s0=int(0.9*fs); Y=20*np.log10(abs(np.fft.rfft(y[s0:s0+N]*w))+1e-12)
    fb=np.arange(len(Y))*fs/N; k=np.round(fb/f0); mask=(abs(fb-k*f0)>4)&(fb>60)&(fb<8000)
    return Y[mask].max()-Y.max()
if __name__=="__main__":
    print("steady /a/: strongest non-harmonic component (dBc) +4/-4 st | accuracy vs true target rms/max dB (+4, then -4, then +7); f0 = 98.3, 147.1, 221.7, 397")
    for name,mode,tau,err in (("TE (Hann 3T exact)",'te',0,0),("harmonic peaks, linear",'h',0,0),("harmonic peaks + lifter",'hl',0,0),("harmonic peaks, f0 err 2%",'h',0,0.02),("harmonic peaks + lifter, err 2%",'hl',0,0.02)):
        row=[]
        for f0 in (98.3,147.1,221.7,397.0):
            s,ph,fi=source2(f0,n); x=tract(s,VOW['a'])*0.1; kmax=int(5000/f0); ks=np.arange(1,kmax+1); sel=ks*f0>=200
            acc=[]
            for st in (4,-4,7):
                al=2**(st/12); y=run(x,fi,al,mode,tau,err)
                A=track(y,ph,kmax)[:,int(1.5*fs)]; d=(A-true_env_db(VOW['a'],ks*f0/al,f0))[sel]; d-=d.mean(); acc.append(f"{d.std():.1f}/{abs(d).max():.0f}")
                if st==4: nh=nonharm(y,f0)
                if st==-4: nh2=nonharm(y,f0)
            row.append(f"{nh:6.1f}/{nh2:6.1f} | "+" ".join(acc))
        print(f"  {name:30s} "+"   ".join(row))
