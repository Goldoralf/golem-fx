from geq import *
from t4 import metrics
from t6 import scanG
import sys
def sections(cent,maxgap=1.0,mingap=0.0):
    out=[cent[0]]
    for a,b in zip(cent[:-1],cent[1:]):
        gap=np.log2(b/a); k=1 if gap<mingap else max(2,int(np.ceil(gap/maxgap)))
        out+= [a*2**(gap*j/k) for j in range(1,k+1)]
    return np.array(out)
def mk2(cent,sec,sr,c,w,clamp=27,lam=1e-3,edge='neigh'):
    G=GEQ(sec,sr,c=c,outer=False,clamp=clamp,lam=lam,edge=edge); cs=np.array(cent,float)
    wv=np.array([w[0] if np.min(abs(np.log2(f/cs)))<1e-9 else (w[1] if np.min(abs(np.log2(f/sec)))<1e-9 else w[2]) for f in G.fp])
    G.w=wv; W=np.diag(wv); B=G.B; G.P=np.linalg.solve(B.T@W@B+lam*np.eye(B.shape[1]),B.T@W); return G
if sys.argv[1]=='gb':
    for mg,mn in ((9,0),(1.0,0),(1.0,0.5),(0.9,0.5),(0.7,0.5)):
        sec=sections(GB,mg,mn)
        for c in (0.4,):
            for e in ('neigh','min'):
                scanG(f"GB maxgap{mg} min{mn} n={len(sec)} {e}",mk2(GB,sec,48000,c,(1,.15,.05),edge=e),GB,48000,N=80)
if sys.argv[1]=='sweep':
    # one chord iteration per tick, warm start: g6 swept -15 -> +15 in 50 ms (75 ticks of 32 at 48k) on a busy curve
    cent=OCT; sec=sections(cent); G=mk2(cent,sec,48000,0.4,(1,.15,.05)); L=np.log2(np.array(cent))
    base=np.array([3,-6,8,-4,6,0,-9,5,-3,7,-5.]); g=None; worst=0; worstc=0
    for ticks in (75,19,5):
      g=None; worst=0
      for t in range(ticks+12):
        fad=base.copy(); fad[5]=-15+30*min(t,ticks)/ticks
        p=PchipInterpolator(L,fad); T=lambda f: p(np.clip(np.log2(f),L[0],L[-1])); tt=T(G.fp)
        if g is None: g,_=G.solve(None,10,T=T)
        g=np.clip(g+G.P@(tt-G.resp(g,G.fp)),-27,27)
        gs,_=G.solve(None,10,T=T)
        worst=max(worst,np.max(abs(g-gs))); ec=np.max(abs(G.resp(g,np.array(cent))-fad))
        if t==ticks+3: settle=np.max(abs(g-gs))
      print(f"sweep 30 dB in {ticks} ticks: worst lag of 1-iteration warm start vs converged gains {worst:.3f} dB; 3 ticks after stop {settle:.4f} dB")
