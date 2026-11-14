from live2 import *
n=int(2.5*fs)
def run(x,f0,alpha,Lp,mp,tau_ms):
    y=np.zeros(n); hprev=np.zeros(NT); hprev[0]=1; hnew=hprev; pend=None; Vs=None; lam=(np.arange(HOP)+1)/HOP
    a_s=0.0 if tau_ms==0 else np.exp(-HOP/(tau_ms*fs/1000))
    for s in range(0,n-HOP,HOP):
        hprev=hnew
        if pend is not None: hnew=pend
        lo=max(0,s-NT+1); seg=x[lo:s+HOP]
        a=sg.lfilter(hprev,1,seg)[s-lo:]; b=sg.lfilter(hnew,1,seg)[s-lo:]
        y[s:s+HOP]=(1-lam)*a+lam*b
        if s>=NFFT+HOP:
            V,lm,it=frame_env(x,s,f0,Lp=Lp,mp=mp)
            Vs=V if Vs is None else a_s*Vs+(1-a_s)*V
            pend,_=minphase(ratio(up(Vs),alpha,norm_P=10**(lm/10)))
    return y
def nonharm(y,f0):
    N=1<<16; w=np.blackman(N); s0=int(0.9*fs); Y=20*np.log10(abs(np.fft.rfft(y[s0:s0+N]*w))+1e-12)
    fb=np.arange(len(Y))*fs/N; k=np.round(fb/f0); mask=(abs(fb-k*f0)>4)&(fb>60)&(fb<8000)
    return Y[mask].max()-Y.max()
print("steady /a/, strongest non-harmonic component (dBc), formant +4 / -4; f0 = 98.3, 147.1, 221.7, 397")
for name,Lp,mp,tau in (("design asym 3T",3.0,None,0),("Hann 3T",3.0,1.5,0),("Hann 4T",4.0,2.0,0),("Hann 3T + 10 ms",3.0,1.5,10),("Hann 3T + 20 ms",3.0,1.5,20),("Hann 4T + 10 ms",4.0,2.0,10)):
    row=[]
    for f0 in (98.3,147.1,221.7,397.0):
        s,ph,fi=source2(f0,n); x=tract(s,VOW['a'])*0.1
        row.append(f"{nonharm(run(x,f0,2**(4/12),Lp,mp,tau),f0):6.1f}/{nonharm(run(x,f0,2**(-4/12),Lp,mp,tau),f0):6.1f}")
    print(f"  {name:18s} "+"  ".join(row))
s,ph,fi=source2(98.3,n); print("  input reference", round(nonharm(tract(s,VOW['a'])*0.1,98.3),1))
