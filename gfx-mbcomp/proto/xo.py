import numpy as np
from scipy import signal
fs=48000.
def bw2(fc,kind,Q=1/np.sqrt(2)):
    # bilinear 2nd order section with prewarp (TPT equiv)
    w=2*fs*np.tan(np.pi*fc/fs)
    if kind=='lp': b,a=[0,0,w*w],[1,w/Q,w*w]
    elif kind=='hp': b,a=[1,0,0],[1,w/Q,w*w]
    else: b,a=[1,-w/Q,w*w],[1,w/Q,w*w]
    return signal.bilinear(b,a,fs)
def casc(secs): return list(secs)
Q8=[0.5411961,1.3065630]
def split(fc,order):
    if order==4:
        lp=casc([bw2(fc,'lp')]*2); hp=casc([bw2(fc,'hp')]*2); ap=[bw2(fc,'ap')]
    else:
        lp=casc([bw2(fc,'lp',q) for q in Q8]*2); hp=casc([bw2(fc,'hp',q) for q in Q8]*2); ap=casc([bw2(fc,'ap',q) for q in Q8])
    return lp,hp,ap
def fr(ba,w):
    if isinstance(ba,list):
        H=np.ones(len(w),dtype=complex)
        for b,a in ba: H*=signal.freqz(b,a,worN=w,fs=fs)[1]
        return H
    return signal.freqz(ba[0],ba[1],worN=w,fs=fs)[1]
f=np.geomspace(10,22000,4000)
def tree(xs,order):
    S=[split(x,order) for x in xs]; n=len(xs)
    bands=[]
    for b in range(n+1):
        H=np.ones_like(f,dtype=complex)
        for k,(lp,hp,ap) in enumerate(S):
            if k<b: H*=fr(hp,f)
            elif k==b: H*=fr(lp,f)
            else: H*=fr(ap,f)
        bands.append(H)
    A=np.ones_like(f,dtype=complex)
    for lp,hp,ap in S: A*=fr(ap,f)
    return bands,A,S
for order in (4,8):
  for xs in ([120,2500],[120,600,2500],[30,40,50]):
    bands,A,S=tree(xs,order)
    rng=np.random.default_rng(1)
    worst_ph=0;bump=0
    for t in range(200):
        g=10**(rng.uniform(-30,12,len(bands))/20)
        Y=sum(gi*B for gi,B in zip(g,bands))
        ph=np.angle(Y/A); worst_ph=max(worst_ph,np.abs(ph).max())
        m=np.abs(Y); bump=max(bump, (20*np.log10(m/ g.max())).max())
    s=np.abs(sum(bands)); 
    # group delay of A at low f
    gd=-np.gradient(np.unwrap(np.angle(A)),2*np.pi*f)
    print(order,xs,'sum dev dB %.2e'%np.abs(20*np.log10(s)).max(),'phase dev vs A rad %.1e'%worst_ph,'max over gmax dB %.2e'%bump,'gd@20Hz %.1fms peak %.1fms@%dHz'%(gd[np.searchsorted(f,20)]*1e3,gd.max()*1e3,f[gd.argmax()]))
