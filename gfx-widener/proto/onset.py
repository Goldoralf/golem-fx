import os
import numpy as np, scipy.io.wavfile as wf, scipy.signal as sg, sys
def follower(x,att_ms,rel_ms,fs):
    a=np.exp(-1/(att_ms*1e-3*fs)); r=np.exp(-1/(rel_ms*1e-3*fs)); y=np.empty_like(x); e=0.
    for i,v in enumerate(x):
        c=a if v>e else r; e=c*e+(1-c)*v; y[i]=e
    return y
def duck(o,fs,rel_ms=60,hold_ms=0):
    r=np.exp(-1/(rel_ms*1e-3*fs)); y=np.empty_like(o); e=0.; h=0; H=int(hold_ms*1e-3*fs); pk=0.
    for i,v in enumerate(o):
        if v>=e: e=v; h=H
        elif h>0: h-=1
        else: e=r*e+(1-r)*v
        y[i]=e
    return y
for name in ('funk','clean','high-gain'):
    fs,x=wf.read(os.path.expanduser(f'~/golem/library/di-refs/guitar/{name}.wav')); x=x.astype(np.float64)
    if x.ndim>1: x=x.mean(1)
    x/=np.max(np.abs(x)); x=x[:fs*12]; ax=np.abs(x)+1e-6
    # reference onsets: energy rise > 9 dB between 10 ms frames, level above -40 dB
    fr=int(.01*fs); e=10*np.log10(np.add.reduceat(x*x,np.arange(0,len(x)-fr,fr))/fr+1e-12)
    on=[i*fr for i in range(2,len(e)) if e[i]-max(e[i-2],e[i-1])>9 and e[i]>-40]
    on=[o for k,o in enumerate(on) if k==0 or o-on[k-1]>0.08*fs]
    sus=np.zeros(len(x),bool)
    for k,o in enumerate(on):
        end=min(o+int(.5*fs), on[k+1]-int(.03*fs) if k+1<len(on) else len(x))
        if end>o+int(.15*fs): sus[o+int(.15*fs):end]=True
    sus&=(ax>10**(-50/20))|True
    variants={
      'draft sym 1ms/30ms':(follower(ax,1,1,fs),follower(ax,30,30,fs),12,0),
      'B att .5/20, rel 100, -1.5dB dead':(follower(ax,.5,100,fs),follower(ax,20,100,fs),9,1.5),
    }
    hp=sg.lfilter(*sg.butter(1,1000/(fs/2),'high'),x); ah=np.abs(hp)+1e-6
    variants['C = B on HP 1 kHz']=(follower(ah,.5,100,fs),follower(ah,20,100,fs),9,1.5)
    print(name, 'onsets',len(on),'sustain s',sus.sum()/fs)
    for k,(fa,sl,rng,dead) in variants.items():
        o=np.clip((20*np.log10(fa/sl)-dead)/rng,0,1); d=1-duck(o,fs,60,20)
        at=[d[o_:o_+int(.02*fs)].min() for o_ in on]; 
        at_mean=[np.sqrt(np.mean(d[o_:o_+int(.02*fs)]**2)) for o_ in on]
        ds=d[sus]
        print(f"  {k:36s} duck 0-20ms after onset: median min d={np.median(at):.2f}, rms gain {20*np.log10(np.median(at_mean)):+.1f} dB | sustain: mean d={ds.mean():.3f}, 5th pct d={np.percentile(ds,5):.2f}, ripple rms {np.std(np.diff(ds))*fs/1000:.3f}/ms")
