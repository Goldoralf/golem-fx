import os
import numpy as np, scipy.io.wavfile as wf
from onset import follower, duck
def detB(ax,fs,slow_ms=20,rel=100,rng=9,dead=1.5):
    fa=20*np.log10(follower(ax,.5,rel,fs)); a=np.exp(-1/(slow_ms*1e-3*fs)); sl=np.empty_like(fa); e=fa[0]
    for i,v in enumerate(fa):
        e=v if v<e else a*e+(1-a)*v; sl[i]=e
    return np.clip((fa-sl-dead)/rng,0,1)
def detDraft(ax,fs): return np.clip(20*np.log10(follower(ax,1,1,fs)/follower(ax,30,30,fs))/12,0,1)
fs=48000; t=np.arange(int(1.5*fs))/fs
# synthetic: plucked low E (82.4 Hz, 8 harmonics, decaying 4 dB/s... ) starting at 0.2 s after silence, then a second pick at 0.9 s over the ringing note
def note(f0,t0,amp):
    tt=np.clip(t-t0,0,None); y=sum(np.sin(2*np.pi*f0*k*tt+k)/k*np.exp(-tt*(1+0.6*k)) for k in range(1,9)); return amp*y*(t>=t0)
x=note(82.4,0.2,0.5)+note(110,0.9,0.5); x+=1e-5*np.random.default_rng(0).standard_normal(len(x))
for nm,det in (('draft',detDraft),('B cascade',detB)):
    o=det(np.abs(x)+1e-6,fs); d=1-duck(o,fs,60,20)
    seg=lambda a,b: d[int(a*fs):int(b*fs)]
    print(f"synthetic {nm:10s}: min d at pick1 {seg(.2,.22).min():.2f}, pick2 (over ringing note) {seg(.9,.92).min():.2f}; sustain .45-.85 s: mean {seg(.45,.85).mean():.3f} min {seg(.45,.85).min():.3f}; time to d>0.9 after pick1: {np.argmax(seg(.2,1.)[int(.005*fs):]>0.9)/fs*1000+5:.0f} ms")
for name in ('funk','clean','high-gain','crunch'):
    fs,x=wf.read(os.path.expanduser(f'~/golem/library/di-refs/guitar/{name}.wav')); x=x.astype(np.float64)
    if x.ndim>1: x=x.mean(1)
    x/=np.max(np.abs(x)); x=x[:fs*12]
    for nm,det in (('draft',detDraft),('B cascade',detB)):
        o=det(np.abs(x)+1e-6,fs); d=1-duck(o,fs,60,20)
        act=np.abs(x)>1e-3
        print(f"{name:9s} {nm:10s}: mean d (signal present) {d[act].mean():.3f}; share of time d<0.5: {np.mean(d[act]<.5)*100:.0f}% ; d<0.95: {np.mean(d[act]<.95)*100:.0f}%")
