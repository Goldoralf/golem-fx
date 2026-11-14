import numpy as np, scipy.signal as sg
fs=48000; HOP=256; NFFT=2048; NT=512
VOW={'a':(730,1090,2440),'e':(530,1840,2480),'i':(270,2290,3010),'o':(570,840,2410),'u':(300,870,2240)}
HI=[3300,4500]+[5500+1000*i for i in range(15)]
BW=(80,90,120,150,200)+tuple(250+25*i for i in range(15))
def reson(f,bw):
    r=np.exp(-np.pi*bw/fs); a=[1,-2*r*np.cos(2*np.pi*f/fs),r*r]; return [sum(a)],a
def tract_resp(F,freqs):
    H=np.ones(len(freqs))
    for f,bw in zip(list(F)+HI,BW):
        b,a=reson(f,bw); H*=abs(sg.freqz(b,a,worN=freqs,fs=fs)[1])
    return H
def source(f0,n,vib=0.0):
    t=np.arange(n)/fs
    f=f0*2**(vib/1200*np.sin(2*np.pi*6*t)); ph=np.cumsum(f)/fs
    s=np.zeros(n); k=1
    while k*f0*1.04<20000:
        s+=np.cos(2*np.pi*k*ph)/k; k+=1
    return s
def tract(s,F):
    y=s
    for f,bw in zip(list(F)+HI,BW):
        b,a=reson(f,bw); y=sg.lfilter(b,a,y)
    return y
def tract_tv(s,F0,F1,n0,ntrans):
    # time-varying cascade, linear glide of F1..F3 from F0 to F1 starting at n0 over ntrans samples
    y=s.copy()
    for i in range(len(BW)):
        fa=(list(F0)+HI)[i]; fb=(list(F1)+HI)[i]; bw=BW[i]; r=np.exp(-np.pi*bw/fs)
        out=np.zeros(len(y)); y1=y2=0.0
        for n in range(len(y)):
            lam=min(max((n-n0)/max(ntrans,1),0),1); f=fa*(fb/fa)**lam
            a1=-2*r*np.cos(2*np.pi*f/fs); a2=r*r; b0=1+a1+a2
            v=b0*y[n]-a1*y1-a2*y2; y2=y1; y1=v; out[n]=v
        y=out
    return y
def awin(L,m):
    L=int(L); m=int(m); w=np.zeros(L)
    w[:L-m]=0.5-0.5*np.cos(np.pi*(np.arange(L-m)+0.5)/(L-m))
    w[L-m:]=0.5+0.5*np.cos(np.pi*(np.arange(m)+0.5)/m)
    return w
def logspec(x,n,L,sym=False):
    L=int(min(max(L,0.008*fs),NFFT))
    if sym: w=np.hanning(L); seg=x[n-L//2:n-L//2+L]
    else: w=awin(L,min(256,L//4)); seg=x[n-L:n]
    X=np.fft.rfft(seg*w/np.sum(w)*2,NFFT)
    return 20*np.log10(np.abs(X)+1e-9)
def pool(lm):
    p=np.empty(513); e=np.concatenate(([lm[1]],lm,[lm[-2]]))
    for j in range(513): p[j]=max(e[2*j],e[2*j+1],e[2*j+2])
    return p
def lifter(A,O):
    c=np.fft.irfft(A,1024); c[O+1:1024-O]=0; return np.fft.rfft(c).real
def te(A0,O,init=None,maxit=8,delta=2.0):
    A=A0 if init is None else np.maximum(A0,init); it=0
    while True:
        V=lifter(A,O); it+=1
        if np.max(A0-V)<=delta or it>=maxit: return V,it
        A=np.maximum(A0,V)
def up(V): return np.interp(np.arange(1025)/2,np.arange(513),V)
def ratio(E,alpha,clamp=18.0,norm_P=None):
    k=np.arange(1025); src=k/alpha
    Ew=np.interp(np.minimum(src,1024),k,E)
    L=Ew-E
    floor=E<np.max(E)-60
    L=np.where(floor,np.minimum(L,0),L)
    L=np.minimum(L,clamp)
    f=k*fs/NFFT
    fade=np.clip(f/40,0,1)*np.clip((0.5*fs-f)/(0.05*fs),0,1)
    L=L*fade
    if norm_P is not None:
        g=10*np.log10(np.sum(norm_P)/np.sum(norm_P*10**(L/10))); L=L+np.clip(g,-12,12)
    return L
def minphase(Ldb,ntaps=NT):
    l=Ldb*np.log(10)/20
    c=np.fft.irfft(l,NFFT); c[1:1024]*=2; c[1025:]=0
    h=np.fft.irfft(np.exp(np.fft.rfft(c)),NFFT)
    full=h.copy(); h=h[:ntaps].copy()
    if ntaps<NFFT: h[-64:]*=0.5+0.5*np.cos(np.pi*(np.arange(64)+0.5)/64)
    return h,full
def live(x,f0,alpha,warm=False,maxit=8,sym=False,look=0,Lfix=None,order=None,smooth=0.0,ret=False):
    """f0: scalar or array per sample. returns y (and per-hop L list)"""
    n=len(x); y=np.zeros(n); f0a=np.full(n,f0) if np.isscalar(f0) else f0
    hprev=np.zeros(NT); hprev[0]=1; hnew=hprev.copy(); pend=None; Vp=None; Ls=[]; its=[]; Es=[]
    xd=np.concatenate((np.zeros(look),x))[:n]
    lam=(np.arange(HOP)+1)/HOP
    for s in range(0,n-HOP,HOP):
        # filter pending (computed from frame ending at s-HOP) takes effect over [s, s+HOP)
        hprev=hnew
        if pend is not None: hnew=pend
        lo=max(0,s-NT+1); seg=xd[lo:s+HOP]
        a=sg.lfilter(hprev,1,seg)[s-lo:]; b=sg.lfilter(hnew,1,seg)[s-lo:]
        y[s:s+HOP]=(1-lam)*a+lam*b
        if s>=NFFT+HOP:
            T=fs/f0a[s]; L=3*T if Lfix is None else Lfix
            lm=logspec(x,s,L,sym=False)
            O=int(np.clip(0.5*fs/f0a[s],24,400)) if order is None else order
            A0=pool(lm); V,it=te(A0,O,init=Vp if warm else None,maxit=maxit)
            if smooth>0 and Vp is not None: V=smooth*Vp+(1-smooth)*V
            Vp=V; E=up(V); P=10**(lm/10)
            Ld=ratio(E,alpha,norm_P=P); pend,_=minphase(Ld); Ls.append(Ld); its.append(it); Es.append(E)
    if ret: return y,np.array(Ls),np.array(its),np.array(Es)
    return y
def harm_levels(y,f0,n0,n1):
    seg=y[n0:n1]; t=np.arange(len(seg))/fs; w=np.hanning(len(seg)); out=[]
    k=1
    while k*f0<8000:
        c=np.sum(seg*w*np.exp(-2j*np.pi*k*f0*t))/np.sum(w)*2; out.append(20*np.log10(abs(c)+1e-12)); k+=1
    return np.array(out)
