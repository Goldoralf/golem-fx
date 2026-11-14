import numpy as np,sys
from zoom2 import *
def sigs(f0,off,Binh=0,glide=0,noise_db=-60,symp=None,dur=3.2,seed=0):
    fn=f0*2**(off/1200); x=pluck(fn,dur,Binh=Binh,glide=glide,phase_seed=seed)
    if symp: x+=pluck(symp[0],dur,t0=-0.3,amp=0.25*10**(symp[1]/20),phase_seed=seed+7,rolloff=symp[2] if len(symp)>2 else 1.0)
    x+=10**(noise_db/20)*np.random.default_rng(seed+3).standard_normal(len(x))
    return x,fn*np.sqrt(1+Binh)
def ev(o,true,thr,sthr,dthr):
    t=o[:,0]-0.05; err=cents(o[:,1],true); ok=~np.isnan(err)
    first=t[ok][0]
    bad=np.where(ok&(np.abs(err)>thr)&(t<2.8))[0]
    terr=first if len(bad)==0 else t[min(bad[-1]+1,len(t)-1)]
    st=ok&(o[:,2]<sthr)&(o[:,3]<dthr)
    # settled = 3 hops running
    run=st&np.roll(st,1)&np.roll(st,2); ss=np.where(run)[0]
    tset=t[ss[0]] if len(ss) else np.nan
    worst=np.nanmax(np.abs(err[run])) if len(ss) else np.nan
    late=ok&(t>2.0)&(t<2.8)
    return first,terr,tset,worst,np.nanmean(err[late]),np.nanstd(err[late]),np.nanmean(o[late,2])
def table(mode,Lp,Rp,Tmin,Tmax,thr,sthr,dthr,win='hann',Bpre=0.5,grow=True):
    print(f"== {mode}: L={Lp} R={Rp} periods, T in [{Tmin},{Tmax}] s, win {win}, Bpre {Bpre}; tol {thr}c, settled: sig<{sthr} & drift<{dthr}")
    notes={'A0':27.5,'B0':30.868,'E1':41.203,'E2':82.407,'A2':110.0,'E4':329.63,'E6':1318.5}
    cases=[]
    for nm,f in notes.items():
        cases+= [(f"{nm} +0c",f,dict(off=0)),(f"{nm} +49c",f,dict(off=49)),(f"{nm} -100c",f,dict(off=-100)),(f"{nm} glide8 B2e-4",f,dict(off=3,Binh=2e-4,glide=8))]
    cases+=[("A2 + E2 ring +10dB",110.0,dict(symp=(82.407,10))),("A2 + 200c-below +10dB",110.0,dict(symp=(98.0,10))),("A2 + 200c-below 0dB",110.0,dict(symp=(98.0,0))),
            ("A2 + 200c-below -10dB",110.0,dict(symp=(98.0,-10))),("G2 + B0x3 (98c below) -6dB",98.0,dict(symp=(30.868,-6+9.5,1.0))),
            ("E2 noise -40dB",82.407,dict(noise_db=-40)),("B0 noise -40dB",30.868,dict(noise_db=-40))]
    for nm,f,kw in cases:
        x,true=sigs(f,**{'off':0,**kw})
        z=Zoom2(f,Lp,Rp,win=win,Bpre=Bpre,Tmin=Tmin,Tmax=Tmax); o=z.run(x,0.05,grow=grow)
        first,terr,tset,worst,b,sd,sp=ev(o,true,thr,sthr,dthr)
        print(f"  {nm:28s} T={z.T:.3f} first {first:.2f}s  |err|<{thr} from {terr:.2f}s  settled@{tset:.2f}s worst|err| while settled {worst:.3f}  late bias {b:+.4f} sd {sd:.4f} sigpred {sp:.4f}")
if __name__=='__main__':
    a=sys.argv[1:]
    if not a or a[0]=='fast': table('Fast',6,6,0.05,0.4,0.3,0.15,0.3)
    if not a or a[0]=='strobe': table('Strobe',16,16,0.25,1.1,0.1,0.03,0.08,win='blackmanharris')
