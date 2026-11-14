import numpy as np, scipy.io.wavfile as wf, scipy.signal as sg
from onset import follower, duck
def detB(ax,fs,slow_ms=20,rel=100,rng=9,dead=1.5):
    fa=20*np.log10(follower(ax,.5,rel,fs)); a=np.exp(-1/(slow_ms*1e-3*fs)); sl=np.empty_like(fa); e=fa[0]
    for i,v in enumerate(fa):
        e=v if v<e else a*e+(1-a)*v; e=max(e,v-(rng+dead)); sl[i]=e
    return np.clip((fa-sl-dead)/rng,0,1)
fs=48000; t=np.arange(int(1.5*fs))/fs
def note(f0,t0,amp):
    tt=np.clip(t-t0,0,None); y=sum(np.sin(2*np.pi*f0*k*tt+k)/k*np.exp(-tt*(1+0.6*k)) for k in range(1,9)); return amp*y*(t>=t0)
x=note(82.4,0.2,0.5)+note(110,0.9,0.5)+1e-5*np.random.default_rng(0).standard_normal(len(t))
for hold in (0,20,40):
    o=detB(np.abs(x)+1e-6,fs); d=1-duck(o,fs,60,hold); seg=lambda a,b: d[int(a*fs):int(b*fs)]
    g=lambda a,b: 20*np.log10(np.sqrt(np.mean(seg(a,b)**2))+1e-9)
    print(f"hold {hold} ms: pick1 side gain 0-20 ms {g(.2,.22):+.1f} dB, 20-40 ms {g(.22,.24):+.1f}, 40-80 {g(.24,.28):+.1f}, 100-150 {g(.3,.35):+.1f}; pick2 0-20 ms {g(.9,.92):+.1f}; sustain mean d {seg(.45,.85).mean():.3f}")
for name in ('funk','clean','high-gain','crunch'):
    fsr,x=wf.read(f'/home/raph/golem/library/di-refs/guitar/{name}.wav'); x=x.astype(np.float64)
    if x.ndim>1: x=x.mean(1)
    x/=np.max(np.abs(x)); x=x[:fsr*12]; o=detB(np.abs(x)+1e-6,fsr); d=1-duck(o,fsr,60,20); act=np.abs(x)>1e-3
    print(f"{name:9s}: mean d {d[act].mean():.3f}; d<0.5 {np.mean(d[act]<.5)*100:.0f}% of the time; d<0.95 {np.mean(d[act]<.95)*100:.0f}%")
