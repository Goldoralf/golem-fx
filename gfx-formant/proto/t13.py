from t12 import *
import t12
def run2(x,f0a,alpha,mode,norm,tau_ms=0,exact=True,asym=False):
    y=np.zeros(n); hprev=np.zeros(NT); hprev[0]=1; hnew=hprev; pend=None; Es=None; lam=(np.arange(HOP)+1)/HOP; gs=None
    a_s=0.0 if tau_ms==0 else np.exp(-HOP/(tau_ms*fs/1000)); ag=np.exp(-HOP/(0.03*fs))
    for s in range(0,n-HOP,HOP):
        hprev=hnew
        if pend is not None: hnew=pend
        lo=max(0,s-NT+1); seg=x[lo:s+HOP]
        a=sg.lfilter(hprev,1,seg)[s-lo:]; b=sg.lfilter(hnew,1,seg)[s-lo:]
        y[s:s+HOP]=(1-lam)*a+lam*b
        if s>=NFFT+HOP:
            f=f0a[s-int(0.01*fs)]
            if asym: V,lm,it=frame_env(x,s,f)
            else: lm=spec(x,s,f)
            if asym: E=up(V)
            elif mode=='te': E=up(te(pool(lm),int(np.clip(0.5*fs/f,24,400)))[0])
            else: E=harm_env(lm,f,lift=(mode=='hl'))
            Es=E if Es is None else a_s*Es+(1-a_s)*E
            L=ratio(Es,alpha)
            if norm!='none':
                P=10**(lm/10); g=10*np.log10(np.sum(P)/np.sum(P*10**(L/10)))
                if norm=='smooth': gs=g if gs is None else ag*gs+(1-ag)*g; g=gs
                L=L+np.clip(g,-12,12)
            pend,_=minphase(L)
    return y
print("steady /a/: strongest non-harmonic component (dBc) +4/-4 st; f0 = 98.3, 147.1, 221.7, 397")
for name,mode,norm,tau,asym in (("TE, norm per hop",'te','hop',0,False),("TE, no norm",'te','none',0,False),("TE, norm smoothed 30 ms",'te','smooth',0,False),("TE, norm 30 ms, env 10 ms",'te','smooth',10,False),("harm peaks, no norm",'h','none',0,False),("harm peaks, norm 30 ms",'h','smooth',0,False),("DESIGN asym window, norm 30 ms",'te','smooth',0,True)):
    row=[]
    for f0 in (98.3,147.1,221.7,397.0):
        s,ph,fi=source2(f0,n); x=tract(s,VOW['a'])*0.1
        row.append(f"{nonharm(run2(x,fi,2**(4/12),mode,norm,tau,asym=asym),f0):6.1f}/{nonharm(run2(x,fi,2**(-4/12),mode,norm,tau,asym=asym),f0):6.1f}")
    print(f"  {name:30s} "+"   ".join(row))
