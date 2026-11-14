import numpy as np
from scipy.signal import lfilter, resample_poly, freqz
fs=48000
def voice(f0,n,seed=0,formants=((700,90),(1220,110),(2600,160),(3300,200),(4500,250)),hnr=25):
    rng=np.random.default_rng(seed); x=np.zeros(n); t=0.0
    while t<n-2:
        T=fs/f0*(1+0.005*rng.normal()); i=int(t); fr=t-i
        x[i]+=1-fr; x[i+1]+=fr; t+=T
    x=lfilter([1],[1,-0.97],x)            # glottal tilt (-6 dB/oct more)
    x=lfilter([1,-1],[1],x)
    x+=rng.normal(0,np.std(x)*10**(-hnr/20),n)
    for f,b in formants:
        r=np.exp(-np.pi*b/fs); a=[1,-2*r*np.cos(2*np.pi*f/fs),r*r]; x=lfilter([sum(a)],a,x)
    return x/np.max(np.abs(x))
def autoc(fr,p): 
    w=fr*np.hanning(len(fr)); return np.array([w[:len(w)-k]@w[k:] for k in range(p+1)])
def lev(R):
    p=len(R)-1; a=np.zeros(p+1); a[0]=1; E=R[0]; ks=[]
    for i in range(1,p+1):
        k=-(R[i]+a[1:i]@R[i-1:0:-1])/E; ks.append(k)
        a[1:i]=a[1:i]+k*a[i-1:0:-1]; a[i]=k; E*=(1-k*k)
    return a,np.array(ks)
def env_peaks(a,lo=300,hi=4000):
    w,h=freqz([1],a,8192,fs=fs); m=20*np.log10(abs(h)); idx=[i for i in range(1,len(w)-1) if m[i]>m[i-1] and m[i]>m[i+1] and lo<w[i]<hi]
    return np.round(w[idx]).astype(int)
def specenv(x,nfft=16384):
    # cepstral-smoothed envelope for judging results independent of LPC
    X=np.abs(np.fft.rfft(x*np.hanning(len(x)),nfft))+1e-9; c=np.fft.irfft(np.log(X)); L=int(fs/ (1.6*300)); c[L:-L]=0
    return np.fft.rfftfreq(nfft,1/fs),np.fft.rfft(c).real*20/np.log(10)
def peaks(f,m,lo=400,hi=3500):
    return [int(f[i]) for i in range(1,len(f)-1) if m[i]>m[i-1] and m[i]>m[i+1] and lo<f[i]<hi]
pre=lambda x: lfilter([1,-0.95],[1],x)
for f0 in [120,220,440,700]:
  x=voice(f0,fs)
  for st in [2,-2]:
    r=2**(st/12)
    # resample (pitch up r: read faster)
    n=np.arange(int(len(x)/r)-4)*r; i=n.astype(int); fr=n-i; y=x[i]*(1-fr)+x[i+1]*fr
    N=960; s0=20000
    fx=pre(x)[s0:s0+N]; fy=pre(y)[int(s0/r):int(s0/r)+N]
    out={}
    for p in [20,48]:
        Rx=autoc(fx,p+8); Ax,kx=lev(Rx[:p+1]*np.exp(-0.5*(2*np.pi*60*np.arange(p+1)/fs)**2))
        Ryd=autoc(fy,p); Ayd,_=lev(Ryd*np.exp(-0.5*(2*np.pi*60*np.arange(p+1)/fs)**2))
        # R_y(k) = R_x(r k): cubic interpolation of Rx (even function)
        lag=np.arange(p+1)*r; Rfull=np.concatenate([Rx[:0:-1],Rx]); c=len(Rx)-1
        def interp(l):
            i=int(np.floor(l)); f=l-i; ym1,y0,y1,y2=Rfull[c+i-1],Rfull[c+i],Rfull[c+i+1],Rfull[c+i+2]
            return y0+0.5*f*(y1-ym1+f*(2*ym1-5*y0+4*y1-y2+f*(3*(y0-y1)+y2-ym1)))
        Ryw=np.array([interp(l) for l in lag]); Ayw,kw=lev(Ryw*np.exp(-0.5*(2*np.pi*60*np.arange(p+1)/fs)**2))
        stable=np.all(np.abs(kw)<1)
        for name,Ay in [('est',Ayd),('warp',Ayw)]:
            z=lfilter([1],Ax,lfilter(Ay,[1],y))
            out[(p,name)]=z
        d=np.max(np.abs(20*np.log10(np.abs(freqz(Ayd,[1],2048,fs=fs)[1][:340])/np.abs(freqz(Ayw,[1],2048,fs=fs)[1][:340]))))
        out[(p,'d')]=d; out[(p,'s')]=stable; out[(p,'pk')]=env_peaks(Ax)
    f,mx=specenv(x[s0:s0+4096]); _,my=specenv(y[int(s0/r):int(s0/r)+4096])
    band=(f>300)&(f<4000)
    def dev(z): 
        _,mz=specenv(z[int(s0/r):int(s0/r)+4096]); dd=(mz-mx)[band]; dd-=dd.mean(); return np.sqrt(np.mean(dd**2))
    dy=(my-mx)[band]; dy-=dy.mean()
    print(f"f0 {f0} {st:+d} st: envelope error 0.3-4 kHz rms dB: plain {np.sqrt(np.mean(dy**2)):.1f} | p20 est {dev(out[(20,'est')]):.1f} warp {dev(out[(20,'warp')]):.1f} | p48 est {dev(out[(48,'est')]):.1f} warp {dev(out[(48,'warp')]):.1f} | A_y est-vs-warp max dB p20 {out[(20,'d')]:.1f} p48 {out[(48,'d')]:.1f} stable {out[(20,'s')]},{out[(48,'s')]} | LPC peaks p20 {out[(20,'pk')]} p48 {out[(48,'pk')]}")
