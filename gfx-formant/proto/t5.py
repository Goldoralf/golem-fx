from live2 import *
fgrid=np.arange(513)*fs/1024
print("== B. warm start: /a/ -> /i/ at t=0.6 s (20 ms glide), f0 150; envelope 300 ms after vs cold-start converged")
f0=150; dur=int(1.3*fs); s,ph,fi=source2(f0,dur); n0=int(0.6*fs); ntr=int(0.02*fs)
x=tract_tv(s,VOW['a'],VOW['i'],n0,ntr)*0.1
ref,_,_=frame_env(x,int(1.0*fs),f0,mp=1.5,maxit=200,delta=0.5)
for warm in (False,True):
    Vp=None; out={}
    for st in range(NFFT+HOP,int(1.0*fs)+1,HOP):
        V,lm,it=frame_env(x,st,f0,mp=1.5,init=Vp if warm else None); Vp=V
        out[st]=V
    last=max(out); d=out[last]-ref; sel=(fgrid>200)&(fgrid<5000)
    print(f"  warm={warm}: env minus cold reference at t=1.0 s: max +{d[sel].max():.1f} dB, min {d[sel].min():.1f}, rms {d[sel].std():.1f}; at 1000 Hz (old F2 of /a/, valley of /i/): {d[int(1000/fs*1024)]:+.1f} dB")
print("== D. lag on the /a/->/i/ glide: rms error (dB, harmonics 200-5k) of output vs true target, by time after the glide starts")
kmax=int(5000/f0); ks=np.arange(1,kmax+1)
def Ft(n):
    lam=np.clip((n-n0)/ntr,0,1); return [a*(b/a)**lam for a,b in zip(VOW['a'],VOW['i'])]
for st in (-4,7):
    al=2**(st/12)
    for name,kw in (("design asym",dict()),("Hann 3T",dict(mp=1.5)),("Hann 2T",dict(Lp=2.0,mp=1.0)),("Hann 3T sched0",dict(mp=1.5,sched=0))):
        y,Ls,its,_=live2(x,f0,al,**kw)
        A=track(y,ph,kmax,win_ms=14)
        row=[]
        for dt in (-20,5,10,15,20,25,30,40,50,60,80,120):
            n=n0+int(dt*fs/1000); tg=true_env_db(Ft(n),ks*f0/al,f0); d=A[:,n]-tg; sel=ks*f0>=200; d=d[sel]-np.mean(d[sel]); row.append(f"{dt}:{d.std():.1f}/{abs(d).max():.0f}")
        print(f"  st {st:+d} {name:15s} t(ms):rms/max  "+"  ".join(row))
print("== G. steady wobble variants (f0 100/150/220/400, st +4): max / median ptp of harmonic levels (dB)")
for name,kw in (("Hann 3T",dict(mp=1.5)),("Hann 4T",dict(Lp=4.0,mp=2.0)),("Hann 2T",dict(Lp=2.0,mp=1.0)),("asym 3T fall 1T",dict(mp=1.0))):
    row=[]
    for f in (100,150,220,400):
        s2,ph2,_=source2(f,int(1.2*fs)); x2=tract(s2,VOW['a'])*0.1; km=int(5000/f)
        y,_,_,_=live2(x2,f,2**(4/12),**kw); A=track(y,ph2,km)[1:,int(0.5*fs):int(1.1*fs):64]; p=np.ptp(A,1); row.append(f"{p.max():.2f}/{np.median(p):.2f}")
    print(f"  {name:16s} "+"  ".join(row))
