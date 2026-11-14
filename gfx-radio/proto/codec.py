import numpy as np, scipy.signal as sg
fs=8000; rng=np.random.default_rng(7)
def synth_voice(dur=8.0):
    n=int(fs*dur); t=np.arange(n)/fs
    f0=110*2**(0.3*np.sin(2*np.pi*0.7*t)+0.2*np.sin(2*np.pi*0.23*t)); ph=np.cumsum(f0/fs)
    src=np.diff(np.floor(ph),prepend=0)            # pulse train
    src=sg.lfilter([1],[1,-0.95],src)*0.1
    gate=(np.sin(2*np.pi*1.3*t)>-0.3).astype(float); gate=sg.lfilter([0.02],[1,-0.98],gate)
    voiced=(np.sin(2*np.pi*0.5*t+1)>-0.5)
    src=np.where(voiced,src,0.03*rng.standard_normal(n))*gate
    y=np.zeros(n); seg=80
    F=[(700,1200,2600),(300,2300,3000),(500,900,2500),(400,1900,2600),(650,1100,2400)]
    zi=[np.zeros(2)]*3; out=[]
    for i in range(0,n,seg):
        a=(i/fs*2.5); k=int(a)%len(F); k2=(k+1)%len(F); fr=a-int(a)
        x=src[i:i+seg]
        for j in range(3):
            fc=F[k][j]*(1-fr)+F[k2][j]*fr; r=np.exp(-np.pi*(60+40*j)/fs)
            aa=[1,-2*r*np.cos(2*np.pi*fc/fs),r*r]
            x,zi[j]=sg.lfilter([1-r],aa,x,zi=zi[j])
        out.append(x)
    y=np.concatenate(out); return y/np.abs(y).max()*0.5
def levinson(r,order):
    a=np.zeros(order+1); a[0]=1; e=r[0]; ks=np.zeros(order)
    for i in range(1,order+1):
        acc=r[i]+np.dot(a[1:i],r[i-1:0:-1]); k=-acc/e if e>1e-12 else 0.
        k=np.clip(k,-0.98,0.98); ks[i-1]=k
        a[1:i]=a[1:i]+k*a[i-1:0:-1][:i-1] if i>1 else a[1:i]; a[i]=k; e*=(1-k*k)
    return ks
def codec(x,q_lar=0.0,alpha=0.0,order=8,N=160,ramp=24):
    n=len(x); y=np.zeros(n); w=np.hamming(N)
    k_a=np.zeros(order); k_s=np.zeros(order); k_at=np.zeros(order); k_st=np.zeros(order)
    fa=np.zeros(order); bs=np.zeros(order)   # analysis delayed backward errs ; synthesis states
    for i in range(n):
        if i%N==0 and i>0:
            fr=x[i-N:i]*w; r=np.array([np.dot(fr[:N-l],fr[l:]) for l in range(order+1)])
            r[0]=r[0]*1.0001+1e-9; r*=np.exp(-0.5*(2*np.pi*60/fs*np.arange(order+1))**2)
            k=levinson(r,order); k_at=k
            if q_lar>0:
                lar=np.log((1+k)/(1-k)); k_st=np.tanh(np.round(lar/q_lar)*q_lar/2)
                k_st=np.clip(k_st,-0.98,0.98)
            else: k_st=k
        s=min(1.0,(i%N+1)/ramp) if i>=N else 1.0
        ka=k_a+(k_at-k_a)*s; ks=k_s+(k_st-k_s)*s
        if i%N==N-1: k_a,k_s=k_at,k_st
        # analysis lattice
        f=x[i]; bprev=x[i]; newb=np.empty(order)
        for m in range(order):
            fn=f+ka[m]*fa[m]; bn=fa[m]+ka[m]*f
            newb[m]=bprev; bprev=bn; f=fn
        fa=newb; e=f
        # excitation coarsening: regular-pulse grid (keep 1 of 3, x3)
        if alpha>0:
            ep=3*e if (i%3==0) else 0.0
            e=e+alpha*(ep-e)
        # synthesis lattice
        f=e
        for m in range(order-1,-1,-1):
            f=f-ks[m]*bs[m]
            if m<order-1: bs[m+1]=bs[m]+ks[m]*f
        bs[0]=f; y[i]=f
    return y
x=synth_voice()
def lvl(a): return 20*np.log10(np.sqrt(np.mean(a*a))+1e-12)
y0=codec(x); print('identity (q=0, alpha=0): residue %.1f dB re signal'%(lvl(y0-x)-lvl(x)))
for q,al in ((0.2,0),(0.5,0),(1.0,0),(0,0.3),(0,1.0),(0.5,0.5),(1.0,1.0)):
    y=codec(x,q,al)
    # per-frame level ratio
    N=160; r=[lvl(y[i:i+N])-lvl(x[i:i+N]) for i in range(N,len(x)-N,N) if lvl(x[i:i+N])>-50]
    print(f'LAR step {q:3.1f} excitation {al:3.1f}: error {lvl(y-x)-lvl(x):6.1f} dB re signal; frame level ratio min {min(r):5.1f} / median {np.median(r):5.1f} / max {max(r):5.1f} dB; peak {np.abs(y).max():.2f} (in 0.50)')
