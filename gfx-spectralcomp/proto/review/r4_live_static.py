# Review check 4: Live's ACGE on lone tones: section gains, convergence with 1 correction per tick, clamp
import numpy as np, sc
fs=48000
eq=sc.LiveEQ(fs); G=eq.geq
print("points",len(G.fp),"sections",len(G.fc),"cond(B'WB)=%.1f"%np.linalg.cond(G.B.T@np.diag(G.w)@G.B))
def steady(f0,lev,res=6.0,iters=200,clamp=40,**kw):
    p=dict(sc.DEF,window=1024,res=res,hop_div=4,down=4,up=3,thresh=-36,knee=6,**kw)
    t=np.arange(fs)/fs; x=10**(lev/20)*np.sin(2*np.pi*f0*t)
    d=sc.Detector(p,fs); e,X=d.frames(x); L=d.levels(X)[-1]; g=sc.static_gain(L,d.T,p,d.f)
    tgt=eq.targets_from_bins(g[None,:],d.f)[0]
    want=np.interp(np.log2(f0),np.log2(d.f[1:]),g[1:])
    gs=np.zeros(len(G.fc)); hist=[]
    for i in range(iters):
        dg=G.P@(tgt-G.resp(gs,G.fp)); gs=np.clip(gs+dg,-clamp,clamp)
        hist.append(G.resp(gs,np.array([f0]))[0])
    return want,hist,gs,tgt
for f0 in (150.,1000.,5000.,7100.):
  for lev in (-24,-12,0):
    want,h,gs,tgt=steady(f0,lev)
    osc=np.ptp(h[-20:])
    print(f"{f0:6.0f} Hz {lev:4d} dBFS: want {want:6.1f} dB; realised after 1/4/16/64/200 corrections {h[0]:6.1f} {h[3]:6.1f} {h[15]:6.1f} {h[63]:6.1f} {h[-1]:6.1f} (p-p of last 20: {osc:.2f}); section gains min {gs.min():6.1f} max {gs.max():6.1f}; at clamp: {int(np.sum(np.abs(gs)>39.9))}")
print("== upward region: lone tone at -60 dBFS (noise floor absent): targets and sections")
want,h,gs,tgt=steady(1000.,-60); print("want %.1f realised %.1f; sections min %.1f max %.1f; target min %.1f max %.1f"%(want,h[-1],gs.min(),gs.max(),tgt.min(),tgt.max()))
print("== broadband sanity: worst |section gain| on random smooth targets of +-12 and +-24 dB (half-octave correlated)")
rng=np.random.default_rng(0)
for amp in (12,24):
    mx=[];er=[]
    for i in range(100):
        r=rng.uniform(-amp,amp,len(G.fp)); r=np.convolve(r,[.25,.5,.25],'same')*1.6; r=np.clip(r,-amp,amp)
        gs=np.zeros(len(G.fc))
        for _ in range(30): gs=np.clip(gs+G.P@(r-G.resp(gs,G.fp)),-40,40)
        mx.append(np.abs(gs).max()); isc=np.isin(G.fp,G.fc); er.append(np.abs(G.resp(gs,G.fp)-r)[isc].max())
    print(f"+-{amp} dB: largest section gain {max(mx):.1f} dB (median {np.median(mx):.1f}); error at centres worst {max(er):.2f} median {np.median(er):.2f}")
