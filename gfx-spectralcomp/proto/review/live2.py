# Review: sc.live with a switchable target rule.  variant 'A' = as written; 'B' = proposed (see r6_mode_consistency.py)
import numpy as np, sc
from scipy.signal import lfilter
def targets(variant,d,g,X,eq,hw=1/3):
    if variant=='A': return eq.targets_from_bins(g,d.f)
    fp=eq.geq.fp; f=d.f; P=np.abs(X)**2; lf=np.log2(np.maximum(f,1e-9)); T=np.empty((g.shape[0],len(fp)))
    for i,fq in enumerate(fp):
        m=np.abs(lf-np.log2(fq))<=hw
        if m.sum()<1: m=np.zeros_like(m); m[np.argmin(np.abs(lf-np.log2(fq)))]=True
        T[:,i]=10*np.log10(np.maximum(np.sum(P[:,m]*10**(g[:,m]/10),axis=1),1e-300)/np.maximum(np.sum(P[:,m],axis=1),1e-300))
    Tp=np.pad(T,((0,0),(1,1)),mode='edge'); return .25*Tp[:,:-2]+.5*T+.25*Tp[:,2:]
def live(x,fs,variant='A',key=None,corrections=1,**kw):
    p=dict(sc.DEF); p.update(kw)
    p['hop_div']=p.get('live_hop_div',4)
    if variant=='A': p['res']=max(p['res'],6.0)
    d=sc.Detector(p,fs); ends,X,Ls,g=d.gains(x if key is None else key)
    eq=sc.LiveEQ(fs,c=p.get('c_bell',0.4)); Tm=targets(variant,d,g,X,eq); H=d.H; G=eq.geq
    tick=int(round(p['tick_ms']*1e-3*fs)); n=len(x); nT=Tm.shape[1]; starts=ends+1
    tick_ends=np.arange(tick-1,n,tick); gsec=np.zeros(len(eq.fc)); zi=[np.zeros(2) for _ in eq.fc]
    out=x.astype(float).copy(); err=[]; gmax=0; opp=0; gtone=[]
    for j,te in enumerate(tick_ends):
        m=np.searchsorted(starts,te,side='right')-1
        if m<0: continue
        a=min(1.0,(te-starts[m]+1)/H); tm1=Tm[m-1] if m>=1 else np.zeros(nT); t=tm1+a*(Tm[m]-tm1)
        t0=te-tick+1
        if np.max(np.abs(t))<1e-6 and np.max(np.abs(gsec))<1e-6: continue
        for _ in range(corrections): gsec=np.clip(gsec+G.P@(t-G.resp(gsec,G.fp)),-40,40)
        v=x[t0:te+1]
        for i,B in enumerate(G.designs(gsec)):
            b0,b1,b2,a1,a2=B; v,zi[i]=lfilter([b0,b1,b2],[1,a1,a2],v,zi=zi[i])
        out[t0:te+1]=v; gmax=max(gmax,np.abs(gsec).max())
        if j%8==0:
            isc=np.isin(G.fp,G.fc); err.append(np.max(np.abs(G.resp(gsec,G.fp)[isc]-t[isc])))
    return out,dict(det=d,g=g,Tm=Tm,err=np.array(err),gmax=gmax,eq=eq)
