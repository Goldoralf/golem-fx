import numpy as np
fs=1000.0
def bp_coefs(f0=5.5,Q=0.7):
    w=2*np.pi*f0/fs; a=np.sin(w)/(2*Q)
    b=np.array([a,0,-a]); A=np.array([1+a,-2*np.cos(w),1-a]); return b/A[0],A/A[0]
def run(p, hyst=23, ramp=60, kA=1.3, k0=35, latch=30):
    b,a=bp_coefs(); n=len(p)
    y1=y2=0.0; x1=x2=p[0]
    wv=0.0; present=False; last_zc=-1; prev_ok=False; pk=0.0; A=0.0
    tgt=None; cen=p[0]; kill_until=-1
    o=dict(t=np.zeros(n),w=np.zeros(n),pq=np.zeros(n))
    for i in range(n):
        x=p[i]
        if i<kill_until:
            # BP held settled on the raw pitch: no step ringing
            x1=x2=x; y1=y2=0.0; y=0.0; present=False; prev_ok=False; last_zc=-1; pk=0; A=0
        else:
            y=b[0]*x+b[1]*x1+b[2]*x2-a[1]*y1-a[2]*y2
            if (y1<=0<y) or (y1>=0>y):
                if last_zc>=0:
                    half=i-last_zc; ok=55<=half<=140 and pk>=12
                    if ok and prev_ok: present=True
                    if ok: A=pk if A==0 else 0.5*(A+pk)
                    prev_ok=ok
                    if not ok: present=False
                last_zc=i; pk=0.0
            pk=max(pk,abs(y))
            if last_zc>=0 and i-last_zc>140: present=False; prev_ok=False
            x2,x1=x1,x; y2,y1=y1,y
        # corridor exit: raw pitch leaves centre by more than the vibrato's extent
        if i>=kill_until and abs(x-cen)>kA*A+k0:
            kill_until=i+latch
        if i<kill_until: wv=max(0.0,wv-1/20.0)
        elif present: wv=min(1.0,wv+1.0/ramp)
        else: wv=max(0.0,wv-1/100.0)
        pq=x-wv*y
        cen+= (pq-cen)*(1/30.0) if i>=kill_until else (x-cen)*(1/5.0)
        if tgt is None: tgt=100*round(pq/100)
        if abs(pq-tgt)>50+hyst: tgt=100*round(pq/100)
        o['t'][i]=tgt;o['w'][i]=wv;o['pq'][i]=pq
    return o
def ss(t,t0,d):
    u=np.clip((t-t0)/d,0,1); return u*u*(3-2*u)
t=np.arange(3000)/fs
fl=lambda tg:int(np.sum(np.diff(tg)!=0))
rng=np.random.default_rng(1)
for depth,fv,step,dur in [(30,6,200,.06),(50,6,200,.06),(50,5,200,.06),(50,7,200,.06),(100,6,200,.06),(50,6,100,.06),(100,5.5,100,.08),(50,6,200,.2),(50,6,-300,.05)]:
    vib=depth*np.sin(2*np.pi*fv*(t-0.3))*ss(t,0.3,0.2)
    p=20+vib+step*ss(t,1.5,dur)+rng.normal(0,3,len(t))
    o=run(p); tg=o['t']; goal=100*round((20+step)/100)
    idx=np.where((t>1.4)&(tg==goal))[0]
    tch=(t[idx[0]]-1.5-dur/2)*1000 if len(idx) else float('nan')
    eng=np.where(o['w']>0.99)[0]
    print(f"vib±{depth}@{fv} step{step:+d}/{int(dur*1000)}ms: flips={fl(tg)} engaged at {eng[0]-300 if len(eng) else -1} ms after vib start; p_q resid pk(.9-1.45)={np.abs(o['pq'][(t>.9)&(t<1.45)]-20).max():.0f}c; target reaches {goal} {tch:+.0f} ms vs mid; w_v 2.4-3s={o['w'][t>2.4].mean():.2f}")
for c in [50,30,70]:
    p=c+60*np.sin(2*np.pi*6*(t-0.3))*ss(t,0.3,0.2)+rng.normal(0,3,len(t)); o=run(p)
    print(f"vib ±60 centre {c}: flips total {fl(o['t'])}, after 0.8 s {fl(o['t'][t>.8])}, w_v {o['w'][t>1].mean():.2f}")
p=np.zeros_like(t)
for k in range(1,12): p+=(200 if k%2 else -200)*ss(t,0.5+0.15*k,0.04)
o=run(p); print("melisma: flips",fl(o['t']),"w_v max",o['w'].max(), "wrong %.0f%%"%(100*np.mean((o['t']!=100*np.round(p/100))[(t>.6)&(t<2.2)])))
p=np.zeros_like(t)
for k in range(1,16): p+=(200 if k%2 else -200)*ss(t,0.5+0.09*k,0.03)   # 5.5 Hz run of whole tones
o=run(p); print("fast 200c alternation @5.5Hz: flips",fl(o['t']),"w_v max",o['w'].max())
p=50*np.tanh(3*np.sin(2*np.pi*6*t))+50; o=run(p); print("trill 0/100@6Hz: flips after 1 s",fl(o['t'][t>1]),"w_v",o['w'][t>1].mean())
# scoop -80 into note over 120 ms then drift
p=-80*(1-ss(t,0.05,0.12)); o=run(p); print("scoop: w_v max",o['w'].max(),"flips",fl(o['t']))
