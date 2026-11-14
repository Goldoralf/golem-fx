# Sinusoid-fit vibrato split: p ~ c + a cos wt + b sin wt over the last 1.25 period, 8 candidate rates.
import numpy as np
fs=200.0   # ~cycle rate (one pitch value per glottal cycle at 200 Hz)
FR=np.arange(4.0,8.01,0.5)
def fit(tt,pp,f):
    M=np.stack([np.ones_like(tt),np.cos(2*np.pi*f*tt),np.sin(2*np.pi*f*tt)],1)
    co,res,_,_=np.linalg.lstsq(M,pp,rcond=None); r=pp-M@co
    return co,np.sqrt(np.mean(r*r))
def run(p,hyst=23,lag=3,ramp_ms=60,amin=12,amax=110,shape=0.3,inn0=30,innA=0.5):
    n=len(p); t=np.arange(n)/fs
    wv=0.0; tgt=None; model=None; hold_until=-1; start=0  # start = first sample of the current "segment"
    o=dict(t=np.zeros(n),w=np.zeros(n),pq=np.zeros(n),A=np.zeros(n)); bad=0
    for i in range(n):
        # 1. innovation against the model fitted `lag` samples ago
        exit_=False
        if model is not None:
            f,co,A=model
            pred=co[0]+co[1]*np.cos(2*np.pi*f*t[i])+co[2]*np.sin(2*np.pi*f*t[i])
            if abs(p[i]-pred)>max(inn0,innA*A): bad+=1
            else: bad=0
            if bad>=2: exit_=True
        if exit_:
            model=None; start=i-1; bad=0; hold_until=i+int(0.02*fs)
        # 2. refit on the samples of this segment older than `lag`
        best=None
        e=i-lag+1
        if e-start>=int(fs/4.0*1.0):   # at least 250 ms? no: per-candidate window below
            for f in FR:
                L=int(round(1.25*fs/f))
                if e-L<start: continue
                co,eps=fit(t[e-L:e],p[e-L:e],f); A=np.hypot(co[1],co[2])
                if best is None or eps/max(A,1e-9)<best[3]: best=(f,co,A,eps/max(A,1e-9))
        if best and amin<=best[2]<=amax and best[3]<shape: model=best[:3]
        elif model is not None and best is not None and (best[2]<amin*0.7 or best[3]>shape*1.5): model=None
        # 3. weight and split
        if model is None: wv=max(0.0,wv-1/(0.02*fs))
        else: wv=min(1.0,wv+1/(ramp_ms*1e-3*fs))
        if model is not None:
            f,co,A=model; vib=co[1]*np.cos(2*np.pi*f*t[i])+co[2]*np.sin(2*np.pi*f*t[i])
        else: vib=0.0; A=0
        pq=p[i]-wv*(p[i]-(model[1][0] if model is not None else p[i]))
        if tgt is None: tgt=100*round(pq/100)
        if abs(pq-tgt)>50+hyst: tgt=100*round(pq/100)
        o['t'][i]=tgt;o['w'][i]=wv;o['pq'][i]=pq;o['A'][i]=A
    return o
def ss(t,t0,d):
    u=np.clip((t-t0)/d,0,1); return u*u*(3-2*u)
t=np.arange(int(3*fs))/fs
fl=lambda tg:int(np.sum(np.diff(tg)!=0))
rng=np.random.default_rng(1)
J=4.0 # cents rms cycle jitter
for depth,fv,step,dur in [(30,6,200,.06),(50,6,200,.06),(50,5,200,.06),(50,7,200,.06),(100,6,200,.06),(50,6,100,.06),(100,5.5,100,.08),(50,6,200,.2),(50,6,-300,.05),(20,6.5,100,.05),(70,4.5,-100,.1)]:
    vib=depth*np.sin(2*np.pi*fv*(t-0.3))*ss(t,0.3,0.2)
    p=20+vib+step*ss(t,1.5,dur)+rng.normal(0,J,len(t))
    o=run(p); tg=o['t']; goal=100*round((20+step)/100)
    idx=np.where((t>1.4)&(tg==goal))[0]
    tch=(t[idx[0]]-1.5-dur/2)*1000 if len(idx) else float('nan')
    eng=np.where(o['w']>0.99)[0]
    re=np.where((o['w']>0.99)&(t>1.5+dur))[0]
    print(f"vib±{depth}@{fv} step{step:+d}/{int(dur*1000)}ms: flips={fl(tg)} (ideal 1-{abs(step)//100}) engaged {1000*(t[eng[0]]-0.3) if len(eng) else -1:.0f} ms after vib start; resid p_q pk={np.abs(o['pq'][(t>.9)&(t<1.45)]-20).max():.0f}c; target {goal} at {tch:+.0f} ms vs mid; re-engaged {1000*(t[re[0]]-1.5-dur) if len(re) else -1:.0f} ms after landing")
for c in [50,30,70]:
    for d in [60,100]:
        p=c+d*np.sin(2*np.pi*6*(t-0.3))*ss(t,0.3,0.2)+rng.normal(0,J,len(t)); o=run(p)
        print(f"vib ±{d} centre {c}: flips total {fl(o['t'])}, after 0.8 s {fl(o['t'][t>.8])}, w_v {o['w'][t>1].mean():.2f}")
p=np.zeros_like(t)
for k in range(1,12): p+=(200 if k%2 else -200)*ss(t,0.5+0.15*k,0.04)
p+=rng.normal(0,J,len(t)); o=run(p); print("melisma 200c/150ms: flips",fl(o['t']),"w_v max",o['w'].max(), "wrong %.0f%%"%(100*np.mean((o['t']!=100*np.round(p/100))[(t>.6)&(t<2.2)])))
p=np.zeros_like(t)
for k in range(1,12): p+=(100 if k%2 else -100)*ss(t,0.5+0.15*k,0.04)
p+=rng.normal(0,J,len(t)); o=run(p); print("melisma 100c/150ms (3.3 Hz): flips",fl(o['t']),"(true 11) w_v max",o['w'].max())
p=np.zeros_like(t)
for k in range(1,16): p+=(200 if k%2 else -200)*ss(t,0.5+0.09*k,0.03)
p+=rng.normal(0,J,len(t)); o=run(p); print("200c alternation @5.5Hz: flips",fl(o['t']),"w_v max",o['w'].max())
p=50*np.tanh(3*np.sin(2*np.pi*6*t))+50+rng.normal(0,J,len(t)); o=run(p); print("semitone trill @6Hz: flips after 1 s",fl(o['t'][t>1]),"w_v",o['w'][t>1].mean())
p=-80*(1-ss(t,0.05,0.12))+rng.normal(0,J,len(t)); o=run(p); print("scoop: w_v max",o['w'].max(),"flips",fl(o['t']))
p=30*np.sin(2*np.pi*0.7*t)+rng.normal(0,8,len(t)); o=run(p); print("slow drift + 8c jitter: w_v max",o['w'].max(),"flips",fl(o['t']))
p=60*ss(t,1.0,0.5)+rng.normal(0,J,len(t)); o=run(p); print("slow bend 60c/500ms: w_v max",o['w'].max())
