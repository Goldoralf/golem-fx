# Review check 6: do Live and Studio compress the same amount at the same settings? (targets only: ACGE error left out)
# A = Live as written (detector res held at 6 st, targets point-sampled)
# B = proposed: detector at `res` as set (Studio's gains), each design point = POWER-weighted mean of those gains over +-1/3 oct, then [1 2 1]/4 along the points
import numpy as np, sc
from scipy.io import wavfile
fs=48000
def rd(f): sr,x=wavfile.read('/home/raph/golem/library/di-refs/guitar/'+f); x=x/ (2**31 if x.dtype==np.int32 else 2**15); return x[:8*fs] if x.ndim==1 else x[:8*fs,0]
rng=np.random.default_rng(3)
def pink(n,lev):
    X=np.fft.rfft(rng.standard_normal(n)); f=np.fft.rfftfreq(n,1/fs); X[1:]/=np.sqrt(f[1:]); X[0]=0
    y=np.fft.irfft(X,n); return y*10**(lev/20)/np.std(y)
sig={'pink -18':pink(6*fs,-18),'clean DI':rd('clean.wav'),'crunch DI':rd('crunch.wav'),'high-gain DI':rd('high-gain.wav'),'lead-clean DI':rd('lead-clean.wav')}
eq=sc.LiveEQ(fs); fp=eq.geq.fp
OCT=[125,250,500,1000,2000,4000,8000]
def per_oct(f,G,P):
    out=[]
    for fc in OCT:
        m=(f>=fc/np.sqrt(2))&(f<fc*np.sqrt(2)); out.append(10*np.log10(np.sum(P[:,m]*10**(G[:,m]/10))/np.sum(P[:,m])))
    return np.array(out)
def tot(G,P,f):
    m=(f>60)&(f<12000); return 10*np.log10(np.sum(P[:,m]*10**(G[:,m]/10))/np.sum(P[:,m]))
def to_bins(T,f):  # targets at design points -> bins, linear in log f, flat outside
    return np.array([np.interp(np.log2(np.maximum(f,1)),np.log2(fp),row) for row in T])
def propB(g,P,f,hw=1/3):
    lf=np.log2(np.maximum(f,1e-9)); T=np.empty((g.shape[0],len(fp)))
    for i,fq in enumerate(fp):
        m=np.abs(lf-np.log2(fq))<=hw
        if m.sum()<1: m=np.zeros_like(m); m[np.argmin(np.abs(lf-np.log2(fq)))]=True
        T[:,i]=10*np.log10(np.sum(P[:,m]*10**(g[:,m]/10),axis=1)/np.maximum(np.sum(P[:,m],axis=1),1e-30))
    Tp=np.pad(T,((0,0),(1,1)),mode='edge'); return .25*Tp[:,:-2]+.5*T+.25*Tp[:,2:]
for setname,kw in (("defaults: thresh -36 down 4 up 1",dict()),("even: thresh -40 down 3 up 2 att 30 rel 200",dict(thresh=-40,down=3,up=2,attack=30,release=200))):
  print("==",setname," (N 1024, hop N/4, auto gain off)  total and per-octave power gain, dB")
  for nm,x in sig.items():
    pS=dict(sc.DEF,window=1024,hop_div=4,res=2.0,**kw); d=sc.Detector(pS,fs); e,X,Ls,gS=d.gains(x); P=np.abs(X)**2
    pA=dict(pS,res=6.0); dA=sc.Detector(pA,fs); _,_,_,gA=dA.gains(x); TA=to_bins(eq.targets_from_bins(gA,d.f),d.f)
    TB=to_bins(propB(gS,P,d.f),d.f)
    s=slice(20,None)
    a=[tot(G[s],P[s],d.f) for G in (gS,TA,TB)]
    print(f"{nm:14s} total: Studio {a[0]:6.2f} | Live as written {a[1]:6.2f} (diff {a[1]-a[0]:+.2f}) | Live proposed {a[2]:6.2f} (diff {a[2]-a[0]:+.2f})")
    print("    per octave diff to Studio, as written: "+' '.join('%+5.1f'%v for v in per_oct(d.f,TA[s],P[s])-per_oct(d.f,gS[s],P[s]))+"   proposed: "+' '.join('%+5.1f'%v for v in per_oct(d.f,TB[s],P[s])-per_oct(d.f,gS[s],P[s])))
  for res in (2.0,6.0): print("   auto make-up at res %g: %.2f dB"%(res,sc.auto_makeup(dict(sc.DEF,window=1024,res=res,**kw),1024,fs)))
