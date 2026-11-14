# NUPOLS schedule prototype: head 64 + levels x4, time-distributed, shared FDL, staggered swap.
import numpy as np
C=64
def layout(L, live_all=False, class_len=None):
    # returns list of (B, start, nparts_used, nparts_layout)
    specs=[(64,64,7),(256,512,6),(1024,2048,6),(4096,8192,6),(8192,32768,None)]
    out=[]
    for B,s,n in specs:
        cl = class_len if class_len else L
        nl = n if n is not None else max(0,-(-(cl-s)//B))
        nl = min(nl, max(0,-(-(cl-s)//B)))
        nu = min(nl, max(0,-(-(L-s)//B)))
        out.append((B,s,nu,nl))
    return out

class Kernel:
    def __init__(self,h):
        self.h=h; self.head=np.zeros(C); self.head[:min(C,len(h))]=h[:C]
        self.parts=[]
        for B,s,nu,nl in layout(len(h)):
            P=[]
            for j in range(nu):
                seg=np.zeros(2*B); t=h[s+j*B:s+(j+1)*B]; seg[:len(t)]=t
                P.append(np.fft.rfft(seg))
            self.parts.append(P)

class Engine:
    def __init__(self, class_len, stagger=True):
        self.lv=layout(class_len)             # FDL sized for class
        self.x=[]                              # input history (list; proto)
        self.fdl=[[] for _ in self.lv]         # per level list of spectra, newest last
        self.stag=[0,0,1,2,3] if stagger else [0]*5
        self.kern=[]                           # list of dict(k=Kernel, out=[level bufs], gain per level fn)
        self.n=0
        self.work=[]                           # flops per chunk
    def add_kernel(self,k):
        # staggered start: each level begins computing for k at its next cycle start
        e=dict(k=k, acc=[None]*5, out=[dict() for _ in self.lv], first=[None]*5, headfrom=self.n)
        self.kern.append(e); return e
    def boundary(self):
        n=self.n; x=np.asarray(self.x); fl=C*C*2*len(self.kern)
        for li,(B,s,_,nl) in enumerate(self.lv):
            if nl==0: continue
            ns=B//C; once=(s==B)
            ph=(n//C)%ns                       # slot within level cycle; cycle starts when block completes
            slot=ph-self.stag[li]
            if once:
                if n>=B:
                    w=np.zeros(2*B); seg=x[max(0,n-2*B):n]; w[2*B-len(seg):]=seg   # only x[<n]
                    self.fdl[li].append(np.fft.rfft(w)); fl+=2.5*2*B*np.log2(2*B)
                    for e in self.kern:
                        P=e['k'].parts[li]
                        if not P: continue
                        acc=sum(self.fdl[li][-1-j]*P[j] for j in range(min(len(P),len(self.fdl[li]))))
                        fl+=8*(B+1)*len(P)+2.5*2*B*np.log2(2*B)
                        e['out'][li][n]=np.fft.irfft(acc)[B:]     # for times [n, n+B)
                        if e['first'][li] is None: e['first'][li]=n
                continue
            if n<B: continue
            if slot==0:
                T=n-self.stag[li]*C                 # block completed at T
                w=np.zeros(2*B); seg=x[max(0,T-2*B):T]; w[2*B-len(seg):]=seg
                self.fdl[li].append(np.fft.rfft(w)); fl+=2.5*2*B*np.log2(2*B)
                for e in self.kern:
                    e['acc'][li]=(T, np.zeros(B+1,complex), 0) if e['k'].parts[li] else None
            nwork=ns-1-self.stag[li]-1              # slots for MACs: 1..ns-2 (relative)
            for e in self.kern:
                a=e['acc'][li]
                if a is None: continue
                T,acc,done=a; P=e['k'].parts[li]
                if 0<=slot:
                    # MAC share: spread len(P) partitions over slots [0 .. last-1], IFFT in last slot
                    last=ns-1-self.stag[li]
                    target=len(P) if slot>=last-1 else int(np.ceil(len(P)*(slot+1)/max(1,last)))
                    target=min(target,len(P))
                    for j in range(done,target):
                        sh=(s-2*B)//B
                        if len(self.fdl[li])>j+sh: acc+=self.fdl[li][-1-j-sh]*P[j]
                        fl+=8*(B+1)
                    done=target
                    if slot==last:
                        e['out'][li][T+B]=np.fft.irfft(acc)[B:]   # for times [T+B, T+2B)
                        fl+=2.5*2*B*np.log2(2*B)
                        if e['first'][li] is None: e['first'][li]=T+B
                        e['acc'][li]=None; continue
                    e['acc'][li]=(T,acc,done)
        self.work.append(fl)
    def step(self,xn):
        if self.n%C==0: self.boundary()
        self.x.append(xn); n=self.n; res=[]
        xh=np.asarray(self.x[-C:])[::-1]
        for e in self.kern:
            parts=[float(np.dot(e['k'].head[:len(xh)],xh))]
            for li,(B,s,_,nl) in enumerate(self.lv):
                if not e['k'].parts[li]: parts.append(0.0); continue
                t0=(n//B)*B
                blk=e['out'][li].get(t0)
                if blk is None:
                    assert e['first'][li] is None or n<e['first'][li], ("DEADLINE MISS",li,n)
                    parts.append(None)
                else: parts.append(blk[n-t0])
            res.append(parts)
        self.n+=1
        return res

if __name__=="__main__":
    rng=np.random.default_rng(1)
    for L in (300,2000,9600,40000,70000):
        h=rng.standard_normal(L)*np.exp(-np.arange(L)/(L/5))
        N=L+3*16384+777
        x=rng.standard_normal(N)
        E=Engine(96000); e=E.add_kernel(Kernel(h))
        y=np.zeros(N)
        for n in range(N):
            r=E.step(x[n])[0]
            y[n]=sum(v for v in r if v is not None)
        ref=np.convolve(x,h)[:N]
        err=np.sqrt(np.mean((y-ref)**2)/np.mean(ref**2))
        print(f"L={L:6d} err {20*np.log10(err+1e-300):7.1f} dB  y[0]==h0*x0: {abs(y[0]-h[0]*x[0])<1e-12}")
    # swap test: kernel 2 arrives at t_sw; each level of k2 valid from first[li]; check it equals full conv from then on
    L1,L2=9600,60000
    h1=rng.standard_normal(L1)*np.exp(-np.arange(L1)/2000); h2=rng.standard_normal(L2)*np.exp(-np.arange(L2)/12000)
    N=120000; x=rng.standard_normal(N); E=Engine(96000); E.add_kernel(Kernel(h1)); tsw=50001; e2=None
    comp=np.full((N,6),np.nan)
    for n in range(N):
        if n==tsw: e2=E.add_kernel(Kernel(h2))
        r=E.step(x[n])
        if e2 is not None: comp[n]=[np.nan if v is None else v for v in r[1]]
    # reference per segment
    segs=[(0,64),(64,512),(512,2048),(2048,8192),(8192,32768),(32768,L2)]
    print("swap at",tsw)
    for i,(a,b) in enumerate(segs):
        hs=np.zeros(L2); hs[a:b]=h2[a:b]; ref=np.convolve(x,hs)[:N]
        ok=~np.isnan(comp[:,i]); first=np.argmax(ok)
        err=np.sqrt(np.mean((comp[ok,i]-ref[ok])**2)/np.mean(ref[ok]**2))
        print(f" seg taps[{a},{b}) new kernel valid from t+{first-tsw:6d} samples; equals full-history conv: err {20*np.log10(err+1e-300):.0f} dB")
