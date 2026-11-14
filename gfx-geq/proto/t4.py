from geq import *
from t2 import dense
import sys
def metrics(G,cent,fad,sr,iters=6):
    cent=np.array(cent,float); L=np.log2(cent); p=PchipInterpolator(L,fad); T=lambda f: p(np.clip(np.log2(f),L[0],L[-1]))
    g,n=G.solve(None,iters,T=T)
    ec=np.max(np.abs(G.resp(g,cent)-fad))
    hi=min(18000,0.45*sr,cent[-1]); f=cent[0]*2**(np.arange(0,int(48*np.log2(hi/cent[0]))+1)/48)
    r=G.resp(g,f); l=np.log2(f); idx=np.clip(np.searchsorted(L,l,side='right')-1,0,len(L)-2)
    lo=np.minimum(fad[idx],fad[idx+1]); up=np.maximum(fad[idx],fad[idx+1])
    env=np.max(np.maximum(lo-r,r-up).clip(0))
    return ec,env,np.max(abs(r-T(f))),np.max(abs(g)),n
def scan(name,cent,sr,N=150,amp=12,dens=False,**kw):
    G=GEQ(dense(cent) if dens else cent,sr,**kw); M=len(cent); rng=np.random.default_rng(1)
    R=np.array([metrics(G,cent,rng.uniform(-amp,amp,M),sr) for i in range(N)])
    s=f"{name:26s} rand centre max {R[:,0].max():4.2f} p99 {np.percentile(R[:,0],99):4.2f} | envelope-viol max {R[:,1].max():4.2f} p99 {np.percentile(R[:,1],99):4.2f} | vsPCHIP max {R[:,2].max():4.2f} | gmax {R[:,3].max():4.1f} it {R[:,4].mean():.1f}"
    for nm,f in [('allup',np.full(M,float(amp))),('zig',amp*(-1.0)**np.arange(M)),('lone',np.eye(M)[M//2]*amp)]:
        e=metrics(G,cent,f,sr); s+=f" | {nm} c{e[0]:.2f} env{e[1]:.2f} g{e[3]:.0f}"
    print(s,flush=True)
if __name__=="__main__":
    lay=dict(OCT=OCT,GT=GT,GB=GB)[sys.argv[1]]
    for c in (0.3,0.4,0.5):
        scan(f"single c{c} w(1,.5)",lay,48000,c=c,outer=False)
        scan(f"single c{c} w(1,.1)",lay,48000,c=c,outer=False,wts=(1,.1,.1))
        scan(f"dense  c{c} w(1,.5)",lay,48000,c=c,outer=False,dens=True)
    scan(f"dense  c.6 w(1,.5)",lay,48000,c=.6,outer=False,dens=True)
