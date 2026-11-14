from live2 import *
import sys
dur=int(1.2*fs)
cfgs={"design (asym 3T, fall<=L/4, d2, 8 it)":dict(),
      "Hann 3T causal, d2, 8 it":dict(mp=1.5),
      "Hann 3T causal, d1, 16 it":dict(mp=1.5,delta=1.0,maxit=16),
      "Hann 3T, d1, 2048 taps":dict(mp=1.5,delta=1.0,maxit=16,ntaps=2048),
      "Hann 3T, d1, cut clamp 30":dict(mp=1.5,delta=1.0,maxit=16,cutclamp=30)}
for f0 in (100,220,400):
  for vib in (0,50):
    s,ph,fi=source2(f0,dur,vib); x=tract(s,VOW['a'])*0.1
    kmax=int(5000/f0/1.04); ks=np.arange(1,kmax+1)
    for st in (-7,-4,4,7,12):
        al=2**(st/12)
        for name,kw in cfgs.items():
            y,Ls,its,_=live2(x,f0,al,**kw)
            A=track(y,ph,kmax); t0,t1=int(0.5*fs),int(1.1*fs); A=A[:,t0:t1:64]
            fk=ks[:,None]*fi[None,t0:t1:64]
            tgt=np.array([true_env_db(VOW['a'],fk[i]/al,f0) for i in range(kmax)])
            sel=(ks*f0>=200)
            d=(A-tgt)[sel]; d-=d.mean()
            wob=np.ptp(A[sel]-(tgt[sel] if vib else 0),axis=1)
            print(f"f0 {f0} vib {vib:2d} st {st:+3d} {name:38s}: err vs true target rms {d.std():4.1f} max {abs(d).max():5.1f} dB | harmonic level wobble ptp max {wob.max():5.2f} med {np.median(wob):4.2f} dB | Lmax {Ls.max():5.1f} Lmin {Ls.min():6.1f}")
        print()
