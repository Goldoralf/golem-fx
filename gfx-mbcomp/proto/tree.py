import numpy as np
from scipy import signal
fs=48000.
Q8=[0.5411961,1.3065630]
def sec(fc,kind,Q=1/np.sqrt(2)):
    w=2*fs*np.tan(np.pi*fc/fs)
    if kind=='lp': b,a=[0,0,w*w],[1,w/Q,w*w]
    elif kind=='hp': b,a=[1,0,0],[1,w/Q,w*w]
    else: b,a=[1,-w/Q,w*w],[1,w/Q,w*w]
    return signal.bilinear(b,a,fs)
def split(fc,order):
    if order==4: return [sec(fc,'lp')]*2,[sec(fc,'hp')]*2,[sec(fc,'ap')]
    return [sec(fc,'lp',q) for q in Q8]*2,[sec(fc,'hp',q) for q in Q8]*2,[sec(fc,'ap',q) for q in Q8]
def bwsplit(fc):  # BW4
    return [sec(fc,'lp',q) for q in Q8],[sec(fc,'hp',q) for q in Q8]
def fr(secs,f):
    H=np.ones(len(f),dtype=complex)
    for b,a in secs: H*=signal.freqz(b,a,worN=f,fs=fs)[1]
    return H
def tree(xs,order,f):
    S=[split(x,order) for x in xs]; n=len(xs); bands=[]
    for b in range(n+1):
        H=np.ones(len(f),dtype=complex)
        for k,(lp,hp,ap) in enumerate(S):
            H*= fr(hp,f) if k<b else fr(lp,f) if k==b else fr(ap,f)
        bands.append(H)
    A=np.ones(len(f),dtype=complex)
    for lp,hp,ap in S: A*=fr(ap,f)
    return bands,A
def bwtree(xs,f):
    S=[bwsplit(x) for x in xs]; n=len(xs); bands=[]
    for b in range(n+1):
        H=np.ones(len(f),dtype=complex)
        for k,(lp,hp) in enumerate(S):
            if k<b: H*=fr(hp,f)
            elif k==b: H*=fr(lp,f)
        bands.append(H)
    return bands
if __name__=='__main__':
    f=np.geomspace(10,23000,6000)
    rng=np.random.default_rng(3)
    for order in (4,8):
      for xs in ([120,2500],[120,600,2500],[40,60,90],[40,60,800],[500,3000,12000],[100,1500]):
        bands,A=tree(xs,order,f)
        wph=0; hi=-1e9; lo=1e9; negM=0
        for B in bands:
            M=B/A; negM=min(negM,M.real.min()); wph=max(wph,np.abs(M.imag).max())
        for t in range(300):
            gdb=rng.uniform(-30,12,len(bands)); g=10**(gdb/20)
            Y=sum(gi*B for gi,B in zip(g,bands)); m=20*np.log10(np.abs(Y))
            hi=max(hi,(m-gdb.max()).max()); lo=min(lo,(m-gdb.min()).min())
        s=20*np.log10(np.abs(sum(bands)))
        gd=-np.gradient(np.unwrap(np.angle(A)),2*np.pi*f)*1e3
        i20=np.searchsorted(f,20); i1k=np.searchsorted(f,1000)
        print(order,xs,'sum %.1e dB | min M %.1e imagM %.1e | above max %.2e below min %.2e | gd 20Hz %.1f peak %.1f@%.0f 1k %.2f'%(np.abs(s).max(),negM,wph,hi,lo,gd[i20],gd.max(),f[gd.argmax()],gd[i1k]))
    # detection tree
    for xs in ([120,2500],[120,600,2500]):
        D=bwtree(xs,f); P=sum(np.abs(d)**2 for d in D)
        print('BW tree',xs,'power sum dev %.1e'%np.abs(P-1).max())
        for order in (4,8):
            bands,A=tree(xs,order,f)
            for k,x in enumerate(xs):
                for fq in (x,x/2,2*x):
                    i=np.searchsorted(f,fq)
                    print('  LR%d x=%g f=%g: det lo %.1f hi %.1f | audio lo %.1f hi %.1f'%(order,x,fq,*(20*np.log10(np.abs(v[i])+1e-12) for v in (D[k],D[k+1],bands[k],bands[k+1]))))
