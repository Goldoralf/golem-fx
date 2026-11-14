import numpy as np
fs=1000.0  # control rate
def bp_coefs(f0=5.5,Q=0.7):
    w=2*np.pi*f0/fs; a=np.sin(w)/(2*Q)
    b=np.array([a,0,-a]); A=np.array([1+a,-2*np.cos(w),1-a]); return b/A[0],A/A[0]
def run(p, mode='design', hyst=23, slope_thr=15.0, verbose=False):
    b,a=bp_coefs(); n=len(p)
    x1=x2=y1=y2=0.0
    wv=0.0; present=False; last_zc=-1; prev_half=None; pk=0.0; last_ok=-1e9
    tgt=None; out_t=np.zeros(n); out_w=np.zeros(n); out_pq=np.zeros(n); out_v=np.zeros(n)
    x1=x2=p[0]  # start settled
    pprev=p[0]; kill_until=-1; A=0.0; cen=p[0]
    for i in range(n):
        x=p[i]; y=b[0]*x+b[1]*x1+b[2]*x2-a[1]*y1-a[2]*y2
        # zero crossings of p_vib
        if (y1<=0<y) or (y1>=0>y):
            if last_zc>=0:
                half=i-last_zc
                ok = 55<=half<=140 and pk>=12
                if ok and prev_half: present=True; last_ok=i; A=pk
                prev_half = ok
            last_zc=i; pk=0.0
        pk=max(pk,abs(y))
        if i-last_zc>140: present=False; prev_half=None
        x2,x1=x1,x; y2,y1=y1,y
        slope=(x-pprev)*fs/100.0; pprev=x   # st/s
        pq = x - wv*y
        # quantise
        if tgt is None: tgt=100*round(pq/100)
        d=pq-tgt
        changed=False
        if abs(d)>50+hyst:
            tgt=100*round(pq/100); changed=True
        if mode=='design':
            if changed or abs(slope)>slope_thr: kill_until=i+1
        elif mode=='noslope':
            if changed: kill_until=i+1
        elif mode=='corridor':
            # leave-corridor test on raw pitch vs tracked centre
            cen += (pq-cen)*(1/50.0)
            if changed or abs(x-cen)>1.5*A+30: kill_until=i+1; 
        if i<kill_until:
            wv=max(0.0,wv-1/20.0); present=False; prev_half=None  # fall within 20 ms, re-detect
            if mode=='corridor': cen=x
        elif present: wv=min(1.0,wv+1/100.0)
        else: wv=max(0.0,wv-1/100.0)
        out_t[i]=tgt; out_w[i]=wv; out_pq[i]=pq; out_v[i]=y
    return out_t,out_w,out_pq,out_v
def sig(T=3.0):
    t=np.arange(int(T*fs))/fs; return t
def smoothstep(t,t0,d): return np.clip((t-t0)/d,0,1)**2*(3-2*np.clip((t-t0)/d,0,1))
def flips(tg): return int(np.sum(np.diff(tg)!=0))
t=sig()
# 1: note at +20, vibrato +-50 @6Hz from 0.3 s (grows 200 ms), legato step +200 over 60 ms at 1.5 s
for depth,fv in [(30,6),(50,6),(50,5),(50,7),(100,6)]:
    vib=depth*np.sin(2*np.pi*fv*(t-0.3))*smoothstep(t,0.3,0.2)
    p=20+vib+200*smoothstep(t,1.5,0.06)
    for mode in ['design','noslope','corridor']:
        tg,w,pq,v=run(p,mode)
        # time target reaches 200 after 1.5
        idx=np.where((t>1.4)&(tg==200))[0]
        tch=(t[idx[0]]-1.53)*1000 if len(idx) else None
        seg=(t>0.9)&(t<1.45); seg2=(t>2.3)
        print(f"vib±{depth}@{fv}Hz {mode:8s}: flips={flips(tg)} w_v(mean .9-1.45)={w[seg].mean():.2f} resid p_q pk={np.abs(pq[seg]-20).max():.0f}c  step detected {tch:+.0f} ms vs transition mid; w_v end={w[seg2].mean():.2f}; p_q sag after step min={pq[(t>1.56)&(t<1.8)].min():.0f} (true 220±{depth})")
print()
# 2: vibrato ±60 centred on a midpoint (50c), from 0.3 s
vib=60*np.sin(2*np.pi*6*(t-0.3))*smoothstep(t,0.3,0.2)
p=50+vib
for mode in ['design','noslope','corridor']:
    tg,w,pq,v=run(p,mode); print("midpoint vib ±60:",mode,"flips",flips(tg),"w_v mean after 1s",round(w[t>1].mean(),2), "flips after 1 s", flips(tg[t>1]))
# 2b same but 20 c off the midpoint i.e. centre 30
p=30+vib
for mode in ['design','noslope','corridor']:
    tg,w,pq,v=run(p,mode); print("centre 30 vib ±60:",mode,"flips",flips(tg),"flips after 1 s", flips(tg[t>1]),"w_v",round(w[t>1].mean(),2))
# 3 melisma: notes 0,200,0,200 every 150 ms (3.3 Hz), transitions 40 ms
p=np.zeros_like(t)
for k in range(1,12): p+= (200 if k%2 else -200)*smoothstep(t,0.5+0.15*k,0.04)
for mode in ['design','noslope','corridor']:
    tg,w,pq,v=run(p,mode); 
    err=np.mean(np.abs(tg-100*np.round(p/100))[(t>0.6)&(t<2.2)]>0)
    print("melisma 200c/150ms:",mode,"flips",flips(tg),"(true 11) wrong-target time %.0f%%"%(100*err),"w_v max",round(w.max(),2))
# 4 trill 100 c at 6 Hz (square-ish) -> indistinguishable from vibrato ±50
p=50*np.tanh(3*np.sin(2*np.pi*6*t))+50
for mode in ['design','noslope','corridor']:
    tg,w,pq,v=run(p,mode); print("trill 0/100 @6Hz:",mode,"flips",flips(tg),"flips after 1s",flips(tg[t>1]))
# BP step response
b,a=bp_coefs(); from scipy.signal import lfilter
s=lfilter(b,a,np.ones(1000)); print("BP step peak %.2f at %d ms; 20ms: %.2f"%(s.max(),s.argmax(),s[20]))
for f in [4,4.5,5.5,6,7,8]:
    w=np.exp(1j*2*np.pi*f/fs); H=(b[0]+b[1]/w+b[2]/w**2)/(1+a[1]/w+a[2]/w**2); print(f, "Hz |H|=%.3f ph=%.0f deg resid|1-H|=%.2f"%(abs(H),np.degrees(np.angle(H)),abs(1-H)))
