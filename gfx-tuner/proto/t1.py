import numpy as np
from zoom import *
rng=np.random.default_rng(1)
def trial(name,f0,off,mode,Binh=0,glide=0,noise_db=-60,symp=None,dur=3.0,verbose=True,excl=None,Bk=None,Tk=None,seed=0):
    ft=f0; fn=f0*2**(off/1200)
    x=pluck(fn,dur,Binh=Binh,glide=glide,phase_seed=seed)
    if symp: # (freq, gain_db) ringing before and during
        x+=pluck(symp[0],dur,t0=-0.3,amp=0.25*10**(symp[1]/20),phase_seed=seed+7)
    x+=10**(noise_db/20)*np.random.default_rng(seed+3).standard_normal(len(x))
    if mode=='fast': B=0.08*ft; T=np.clip(8/ft,0.04,0.25)
    else: B=max(0.04*ft,1.1); T=np.clip(30/ft,0.3,1.2)
    if Bk: B=Bk*ft
    if Tk: T=Tk
    z=Zoom(ft,B,T,excl=excl); o=z.run(x,0.05)
    true=fn*np.sqrt(1+Binh)
    t=o[:,0]-0.05; err=cents(o[:,1],true); sig=o[:,2]
    ok=~np.isnan(err)
    first=t[ok][0] if ok.any() else np.nan
    def stable(th):
        bad=np.where(ok&(np.abs(err)>th)&(t<2.5))[0]
        if not ok.any(): return np.nan
        return first if len(bad)==0 else t[min(bad[-1]+1,len(t)-1)]
    thr=0.3 if mode=='fast' else 0.1
    sthr=0.3 if mode=='fast' else 0.05
    ss=np.where(ok&(sig<sthr))[0]
    tsig=t[ss[0]] if len(ss) else np.nan
    late=ok&(t>1.5)&(t<2.5)
    r=dict(first=first,t_err=stable(thr),t_sig=tsig,bias=np.nanmean(err[late]),sd=np.nanstd(err[late]),sigpred=np.nanmean(sig[late]),err_at_sig=err[ss[0]] if len(ss) else np.nan)
    if verbose: print(f"{name:34s} B={B:6.2f} T={T:.3f} first={r['first']:.2f}s |err|<{thr}: {r['t_err']:.2f}s  sig<{sthr}: {r['t_sig']:.2f}s (err there {r['err_at_sig']:+.3f})  late bias {r['bias']:+.4f} sd {r['sd']:.4f} pred {r['sigpred']:.4f}")
    return r
if __name__=='__main__':
    notes={'A0':27.5,'B0':30.868,'E1':41.203,'E2':82.407,'A2':110.0,'E4':329.63,'E6':1318.5}
    for mode in ('fast','strobe'):
        print('==',mode,'clean, no glide')
        for nm,f in notes.items():
            for off in (0,3,-20,49):
                trial(f"{nm} {off:+d}c",f,off,mode)
        print('==',mode,'glide +8c tau150, Binh 2e-4')
        for nm,f in notes.items():
            trial(f"{nm} glide",f,3,mode,Binh=2e-4,glide=8)
        print('==',mode,'sympathetic: A2 read, E2 +10 dB')
        trial("A2 + E2 ringing +10dB",110.0,0,mode,symp=(82.407,10))
        trial("A2 + G2(200c below) +10dB",110.0,0,mode,symp=(98.0,10))
        trial("E2 + D2(200c below) +0dB",82.407,0,mode,symp=(73.416,0))
        trial("B0 + A0... E1 +0dB",30.868,0,mode,symp=(41.203,0))
