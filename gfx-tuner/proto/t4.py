import numpy as np,sys
from zoom3 import *
notes={'A0':27.5,'B0':30.868,'E1':41.203,'E2':82.407,'D3':146.83,'E4':329.63,'E6':1318.5}
def go(mode,pmin,pmax,Tlo,Thi,tol,stol,span_fn,seeds=3):
    print(f"== {mode}: {pmin}..{pmax}p [{Tlo},{Thi}]s; settled: span<= {stol}c")
    cases=[]
    for nm,f in notes.items():
        cases+=[(f"{nm} clean",f,dict(off=-37)),(f"{nm} glide8",f,dict(off=3,Binh=2e-4,glide=8)),(f"{nm} glide20 tau.25",f,dict(off=3,glide=20,gtau=0.25))]
    cases+=[("D3 + E2 ring -10dB",146.83,dict(symp=(82.407,-10))),("D3 + E2 ring 0dB",146.83,dict(symp=(82.407,0))),
            ("A2 + 200c-below -10dB",110.0,dict(symp=(98.0,-10))),("A2 + 200c-below -20dB",110.0,dict(symp=(98.0,-20))),("A2 + E2 +10dB",110.0,dict(symp=(82.407,10))),
            ("G2 + B0 ring (x3 98c) -10dB",98.0,dict(symp=(30.868,-10))),("E2 noise -40dB",82.407,dict(noise_db=-40)),("B0 noise -50dB",30.868,dict(noise_db=-50))]
    for nm,f,kw in cases:
        r=[]
        for sd in range(seeds):
            kw2={'off':0,**kw}; gt=kw2.pop('gtau',None)
            if gt:
                fn=f*2**(kw2['off']/1200); x=pluck(fn,3.2,glide=kw2['glide'],gtau=gt,phase_seed=sd); x+=1e-3*np.random.default_rng(sd).standard_normal(len(x)); true=fn
            else: x,true=sigs(f,seed=sd,**kw2)
            fc=true*2**((4 if sd%2 else -4)/1200)
            o=run(x,fc,0.05,pmin,pmax,Tlo,Thi)
            Tm=np.clip(pmax/fc,Tlo,Thi)
            first,terr,ts,worst,frac,c,t=settled(o,true,tol,9,span_fn(Tm),stol=stol)
            late=(t>1.8)&(t<2.8); r.append((first,terr,ts,worst,np.nanmax(np.abs(c[late]))))
        r=np.array(r)
        print(f"  {nm:28s} first {r[:,0].max():.2f}s  |err|<={tol} from {r[:,1].max():.2f}s  settled@{np.nanmin(r[:,2]):.2f}-{np.nanmax(r[:,2]):.2f}s ({np.isnan(r[:,2]).sum()} never)  worst|err| settled {np.nanmax(r[:,3]) if not np.isnan(r[:,3]).all() else np.nan:.3f}  late max {r[:,4].max():.3f}")
go('Fast',8,24,0.08,0.6,0.3,0.2,lambda T:max(0.15,T/2))
go('Strobe',8,48,0.3,1.2,0.1,0.07,lambda T:max(0.3,T/2))
