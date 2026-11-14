from t12 import *
def run3(x,f0a,alpha,nsub,tau_ms=0,powavg=False):
    y=np.zeros(n); hprev=np.zeros(NT); hprev[0]=1; hnew=hprev; pend=None; Es=None; lam=(np.arange(HOP)+1)/HOP
    a_s=0.0 if tau_ms==0 else np.exp(-HOP/(tau_ms*fs/1000))
    for s in range(0,n-HOP,HOP):
        hprev=hnew
        if pend is not None: hnew=pend
        lo=max(0,s-NT+1); seg=x[lo:s+HOP]
        a=sg.lfilter(hprev,1,seg)[s-lo:]; b=sg.lfilter(hnew,1,seg)[s-lo:]
        y[s:s+HOP]=(1-lam)*a+lam*b
        if s>=NFFT+HOP:
            f=f0a[s-int(0.01*fs)]; T=fs/f; O=int(np.clip(0.5*fs/f,24,400))
            if powavg:
                P=np.mean([10**(spec(x,s-int(round(j*T/nsub)),f)/10) for j in range(nsub)],0); E=up(te(pool(10*np.log10(P)),O)[0])
            else:
                E=np.mean([up(te(pool(spec(x,s-int(round(j*T/nsub)),f)),O)[0]) for j in range(nsub)],0)
            Es=E if Es is None else a_s*Es+(1-a_s)*E
            pend,_=minphase(ratio(Es,alpha))
    return y
print("steady /a/: strongest non-harmonic (dBc) +4/-4 st; f0 = 98.3, 147.1, 221.7, 397")
for name,nsub,tau,pa in (("1 frame",1,0,False),("2 frames T/2 apart, env mean",2,0,False),("2 frames, power mean then TE",2,0,True),("3 frames T/3 apart, env mean",3,0,False),("2 frames + 10 ms",2,10,False)):
    row=[]
    for f0 in (98.3,147.1,221.7,397.0):
        s,ph,fi=source2(f0,n); x=tract(s,VOW['a'])*0.1
        row.append(f"{nonharm(run3(x,fi,2**(4/12),nsub,tau,pa),f0):6.1f}/{nonharm(run3(x,fi,2**(-4/12),nsub,tau,pa),f0):6.1f}")
    print(f"  {name:30s} "+"   ".join(row))
