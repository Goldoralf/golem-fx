from geq import *
from t2 import dense
from t4 import metrics
import sys
def mk(cent,sr,c,w,edge='neigh',clamp=27,lam=1e-3):
    G=GEQ(dense(cent),sr,c=c,outer=False,edge=edge,clamp=clamp,lam=lam)
    cs=np.array(cent,float); fine=dense(cent)
    wv=np.array([w[0] if np.min(abs(np.log2(f/cs)))<1e-9 else (w[1] if np.min(abs(np.log2(f/fine)))<1e-9 else w[2]) for f in G.fp])
    G.w=wv; W=np.diag(wv); B=G.B; G.P=np.linalg.solve(B.T@W@B+lam*np.eye(B.shape[1]),B.T@W); return G
def scanG(name,G,cent,sr,N=100,amp=12):
    M=len(cent); rng=np.random.default_rng(1)
    R=np.array([metrics(G,cent,rng.uniform(-amp,amp,M),sr) for i in range(N)])
    s=f"{name:30s} rand centre max {R[:,0].max():4.2f} | env max {R[:,1].max():4.2f} | vsPCHIP max {R[:,2].max():4.2f} p99 {np.percentile(R[:,2],99):4.2f} | gmax {R[:,3].max():4.1f} it {R[:,4].mean():.1f}"
    for nm,f in [('allup',np.full(M,float(amp))),('zig',amp*(-1.0)**np.arange(M)),('lone',np.eye(M)[M//2]*amp)]:
        e=metrics(G,cent,f,sr); s+=f" | {nm} c{e[0]:.2f} env{e[1]:.2f} P{e[2]:.2f} g{e[3]:.0f}"
    print(s,flush=True)
if __name__=='__main__':
    lay=dict(OCT=OCT,GT=GT,GB=GB)[sys.argv[1]]
    for c in (0.3,0.4,0.5):
        for w in ((1,.5,.25),(1,.3,.1),(1,.15,.05)):
            scanG(f"{sys.argv[1]} dense c{c} w{w}",mk(lay,48000,c,w),lay,48000)
    scanG(f"{sys.argv[1]} dense c.4 w(1,.3,.1) ±15",mk(lay,48000,.4,(1,.3,.1)),lay,48000,amp=15)
    scanG(f"{sys.argv[1]} dense c.4 w(1,.3,.1) 44.1k",mk(lay,44100,.4,(1,.3,.1)),lay,44100)
