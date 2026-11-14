from live2 import *
f0=150; dur=int(1.3*fs); s,ph,fi=source2(f0,dur); n0=int(0.6*fs)
kmax=int(5000/f0); ks=np.arange(1,kmax+1); sel=ks*f0>=200
for ntr_ms in (20,60):
    ntr=int(ntr_ms*fs/1000)
    x=tract_tv(s,VOW['a'],VOW['i'],n0,ntr)*0.1
    def Ft(n):
        lam=np.clip((n-n0)/ntr,0,1); return [a*(b/a)**lam for a,b in zip(VOW['a'],VOW['i'])]
    print(f"== /a/->/i/ glide {ntr_ms} ms, f0 150: rms/max error (dB) of output harmonics vs true target, by time after glide start")
    for st in (0,-4,7):
        al=2**(st/12)
        for name,kw,look in (("design asym",dict(),0),("Hann 3T",dict(mp=1.5),0),("Hann 3T look 10ms",dict(mp=1.5),480),("Hann 3T look 20ms",dict(mp=1.5),960)):
            if st==0 and name!="Hann 3T": continue
            y,Ls,its,_=live2(x,f0,al,**kw)
            if look:
                # lookahead: audio delayed by `look` relative to the analysis = filters applied `look` earlier
                xd=np.concatenate((np.zeros(look),x))[:len(x)]
                # re-run with analysis on x, audio xd: emulate by shifting filter timeline
                y=np.zeros(len(x)); hs=[]
                # recompute filters
                _,Ls,_,_=live2(x,f0,al,**kw)
                first=(NFFT+HOP)//HOP+1
                hprev=np.zeros(NT); hprev[0]=1; hnew=hprev; lam=(np.arange(HOP)+1)/HOP; q=[]
                for i,s0 in enumerate(range(0,len(x)-HOP,HOP)):
                    hprev=hnew
                    j=i-first-1+1   # filter index available with 1-hop scheduling
                    if 0<=j<len(Ls): hnew,_=minphase(Ls[j])
                    lo=max(0,s0-NT+1); seg=xd[lo:s0+HOP]
                    a=sg.lfilter(hprev,1,seg)[s0-lo:]; b=sg.lfilter(hnew,1,seg)[s0-lo:]
                    y[s0:s0+HOP]=(1-lam)*a+lam*b
                y=np.concatenate((y[look:],np.zeros(look)))   # realign for measurement
            A=track(y,ph,kmax,win_ms=14); row=[]
            for dt in (-20,0,10,20,30,40,50,60,80,100,140):
                n=n0+int(dt*fs/1000); tg=true_env_db(Ft(n),ks*f0/al,f0); d=A[:,n]-tg; d=d[sel]-np.mean(d[sel]); row.append(f"{dt}:{d.std():.1f}/{abs(d).max():.0f}")
            print(f"  st {st:+d} {name:18s} "+"  ".join(row))
print("== G. steady wobble variants (f0 100/150/220/400, st +4): max/median ptp of harmonic levels (dB)")
for name,kw in (("Hann 3T",dict(mp=1.5)),("Hann 4T",dict(Lp=4.0,mp=2.0)),("Hann 2T",dict(Lp=2.0,mp=1.0)),("asym 3T fall 1T",dict(mp=1.0))):
    row=[]
    for f in (100,150,220,400):
        s2,ph2,_=source2(f,int(1.2*fs)); x2=tract(s2,VOW['a'])*0.1; km=int(5000/f)
        y,_,_,_=live2(x2,f,2**(4/12),**kw); A=track(y,ph2,km)[1:,int(0.5*fs):int(1.1*fs):64]; p=np.ptp(A,1); row.append(f"{p.max():.2f}/{np.median(p):.2f}")
    print(f"  {name:16s} "+"  ".join(row))
