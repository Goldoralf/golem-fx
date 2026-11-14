import numpy as np
from scipy.interpolate import PchipInterpolator
PI=np.pi
def amag2(n,d,w):
    num=(n[0]-n[2]*w*w)**2+(n[1]*w)**2; den=(d[0]-d[2]*w*w)**2+(d[1]*w)**2
    return num/np.maximum(den,1e-300)
def bell(q,g):
    a=10**(g/40); return ([1,a/q,1],[1,1/(a*q),1])
def design_direct(n,d,f0,sr):
    w0=2*PI*min(max(f0/sr,1e-6),0.4999)
    d0,d1,d2=d; wp=np.sqrt(d0/d2)*w0; zeta=d1/(2*np.sqrt(d0*d2)); e=np.exp(-zeta*wp)
    a1=-2*e*np.cos(np.sqrt(1-zeta*zeta)*wp) if zeta<=1 else -2*e*np.cosh(np.sqrt(zeta*zeta-1)*wp)
    a2=e*e
    a0p=(1+a1+a2)**2; a1p=(1-a1+a2)**2; a2p=-4*a2
    t0=amag2(n,d,0.0); tn=amag2(n,d,PI/w0); b0p=t0*a0p; b1p=tn*a1p
    lo=max(w0/16,1e-4); hi=min(w0*16,0.95*PI)
    w=lo*(hi/lo)**(np.arange(25)/24); s=np.sin(w/2)**2; p0,p1,p2=1-s,s,4*(1-s)*s
    target=amag2(n,d,w/w0)*(a0p*p0+a1p*p1+a2p*p2)
    ok=(target>1e-12)&(p2>1e-12); r=target-b0p*p0-b1p*p1; wg=1/(target*target)
    num=np.sum((wg*p2*r)[ok]); den=np.sum((wg*p2*p2)[ok]); b2p=num/den if den>0 else 0
    sb0=np.sqrt(max(b0p,0)); sb1=np.sqrt(max(b1p,0)); W=0.5*(sb0+sb1)
    b0=0.5*(W+np.sqrt(max(W*W+b2p,0))); b1=0.5*(sb0-sb1); b2=W-b0
    return (b0,b1,b2,a1,a2)
def design(q,g,f0,sr):
    if g<0:
        n,d=bell(q,-g); B=design_direct(n,d,f0,sr)
        return (1/B[0],B[3]/B[0],B[4]/B[0],B[1]/B[0],B[2]/B[0])
    n,d=bell(q,g); return design_direct(n,d,f0,sr)
def dmag_db(B,f,sr):
    b0,b1,b2,a1,a2=B; s=np.sin(PI*f/sr)**2; p0,p1,p2=1-s,s,4*(1-s)*s
    n=(b0+b1+b2)**2*p0+(b0-b1+b2)**2*p1-4*b0*b2*p2; d=(1+a1+a2)**2*p0+(1-a1+a2)**2*p1-4*a2*p2
    return 10*np.log10(np.maximum(n,1e-300)/np.maximum(d,1e-300))
def q_law(G,edge_ratio,c):
    x=(1-edge_ratio**2)**2/edge_ratio**2
    if abs(G)<1e-6: iq2=x*c/(1-c)
    else:
        al=10**(abs(G)/20); K=al**(2*c); iq2=x*(K-1)/(al-K/al)
    return 1/np.sqrt(iq2)
class GEQ:
    def __init__(s,centres,sr,edge='neigh',c=0.3,wts=(1,.5,.25),outer=True,lam=1e-3,proto=15.0,clamp=27.0,pts='mid'):
        s.fc=np.array(centres,float); s.sr=sr; s.c=c; s.clamp=clamp; M=len(s.fc); L=np.log2(s.fc)
        dl=np.diff(L)
        s.er=np.zeros(M)
        for m in range(M):
            ds=[dl[m-1]] if m>0 else []
            if m<M-1: ds.append(dl[m])
            if edge=='neigh': d=np.mean(ds)
            elif edge=='half': d=np.mean(ds)/2
            elif edge=='min': d=min(ds)
            elif edge=='max': d=max(ds)
            else: d=np.mean(ds)*edge
            s.er[m]=2**d
        mids=np.sqrt(s.fc[:-1]*s.fc[1:])
        P=[(f,wts[0]) for f in s.fc]+[(f,wts[1]) for f in mids]
        if outer:
            P+= [(s.fc[0]/2**(dl[0]/2),wts[2]),(min(s.fc[-1]*2**(dl[-1]/2),0.47*sr),wts[2])]
        P.sort(); s.fp=np.array([p[0] for p in P]); s.w=np.array([p[1] for p in P])
        B=np.zeros((len(s.fp),M))
        for m in range(M):
            B[:,m]=dmag_db(design(q_law(proto,s.er[m],c),proto,s.fc[m],sr),s.fp,sr)/proto
        s.B=B; W=np.diag(s.w); s.P=np.linalg.solve(B.T@W@B+lam*np.eye(M),B.T@W)
    def designs(s,g): return [design(q_law(g[m],s.er[m],s.c),g[m],s.fc[m],s.sr) for m in range(len(g))]
    def resp(s,g,f):
        return sum(dmag_db(Bq,f,s.sr) for Bq in s.designs(g))
    def target_fn(s,fad):
        L=np.log2(s.fc); p=PchipInterpolator(L,fad)
        def T(f):
            l=np.log2(f); y=p(np.clip(l,L[0],L[-1])); return y
        return T
    def solve(s,fad,iters=4,tol=0.005,T=None):
        T=T or s.target_fn(fad); t=T(s.fp); g=np.clip(s.P@t,-s.clamp,s.clamp); n=0
        for i in range(iters):
            dg=s.P@(t-s.resp(g,s.fp)); g=np.clip(g+dg,-s.clamp,s.clamp); n+=1
            if np.max(np.abs(dg))<tol: break
        return g,n
def evaluate(geq,fad,iters=4,fmax=18000):
    g,n=geq.solve(fad,iters); T=geq.target_fn(fad)
    ec=np.max(np.abs(geq.resp(g,geq.fc)-fad))
    hi=min(fmax,0.45*geq.sr,geq.fc[-1]); lo=max(25,geq.fc[0])
    f=lo*2**(np.arange(0,int(24*np.log2(hi/lo))+1)/24)
    e=geq.resp(g,f)-T(f)
    return ec,np.max(np.abs(e)),np.sqrt(np.mean(e*e)),np.max(np.abs(g)),n
OCT=[31.25,62.5,125,250,500,1000,2000,4000,8000,12000,16000]
OCT10=[31.25,62.5,125,250,500,1000,2000,4000,8000,16000]
GT=[100,200,400,800,1600,3200,6400]
GB=[50,120,400,500,800,4500,10000]
def run(name,geq,N=300,amp=12,iters=4,seed=1):
    rng=np.random.default_rng(seed); M=len(geq.fc); R=[]
    for i in range(N): R.append(evaluate(geq,rng.uniform(-amp,amp,M),iters))
    R=np.array(R)
    spec=[('allup',np.full(M,amp)),('zigzag',amp*(-1.0)**np.arange(M)),('lone',np.eye(M)[M//2]*amp)]
    s=f"{name:34s} rand: centre max {R[:,0].max():5.2f} p99 {np.percentile(R[:,0],99):5.2f} | curve max {R[:,1].max():5.2f} p99 {np.percentile(R[:,1],99):5.2f} rms {R[:,2].mean():4.2f} | gmax {R[:,3].max():5.1f} it {R[:,4].mean():.1f}"
    for nm,f in spec:
        e=evaluate(geq,f,iters); s+=f" | {nm} c{e[0]:.2f} k{e[1]:.2f} g{e[3]:.0f}"
    print(s,flush=True)
