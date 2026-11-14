from geq import *
import sys
def dense(c): 
    c=np.array(c,float); m=np.sqrt(c[:-1]*c[1:]); return np.sort(np.concatenate([c,m]))
def evalD(cent,sr,fad,edge='neigh',c=0.3,wts=(1,.5,.25),iters=4,cache={},dens=True,outer=True,lam=1e-3):
    key=(tuple(cent),sr,edge,c,wts,dens,outer,lam)
    if key not in cache: cache[key]=GEQ(dense(cent) if dens else cent,sr,edge=edge,c=c,wts=wts,outer=outer,lam=lam)
    G=cache[key]; L=np.log2(np.array(cent,float)); p=PchipInterpolator(L,fad)
    T=lambda f: p(np.clip(np.log2(f),L[0],L[-1]))
    g,n=G.solve(None,iters,T=T)
    ec=np.max(np.abs(G.resp(g,np.array(cent,float))-fad))
    hi=min(18000,0.45*sr,cent[-1]); lo=cent[0]
    f=lo*2**(np.arange(0,int(24*np.log2(hi/lo))+1)/24); e=G.resp(g,f)-T(f)
    return ec,np.max(np.abs(e)),np.sqrt(np.mean(e*e)),np.max(np.abs(g)),n
def runD(name,cent,sr,N=200,amp=12,**kw):
    rng=np.random.default_rng(1); M=len(cent)
    R=np.array([evalD(cent,sr,rng.uniform(-amp,amp,M),**kw) for i in range(N)])
    s=f"{name:30s} rand: centre max {R[:,0].max():5.2f} p99 {np.percentile(R[:,0],99):5.2f} | curve max {R[:,1].max():5.2f} p99 {np.percentile(R[:,1],99):5.2f} rms {R[:,2].mean():4.2f} | gmax {R[:,3].max():5.1f} it {R[:,4].mean():.1f}"
    for nm,f in [('allup',np.full(M,amp)),('zigzag',amp*(-1.0)**np.arange(M)),('lone',np.eye(M)[M//2]*amp)]:
        e=evalD(cent,sr,f,**kw); s+=f" | {nm} c{e[0]:.2f} k{e[1]:.2f} g{e[3]:.0f}"
    print(s,flush=True)
if __name__=="__main__":
    for nm,c in (('OCT11',OCT),('GT7',GT),('GB7',GB)):
        runD(f"{nm} dense c.3",c,48000)
        runD(f"{nm} dense c.3 it8",c,48000,iters=8)
        runD(f"{nm} dense c.4",c,48000,c=0.4)
        runD(f"{nm} single w(1,.5)",c,48000,dens=False)
        runD(f"{nm} single w(1,0) it12",c,48000,dens=False,wts=(1,1e-6,1e-6),iters=12)
