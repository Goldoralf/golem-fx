import numpy as np
from scipy.signal import lfilter, butter
import sys; sys.argv=['x']
exec(open('design.py').read().split("import sys")[0])
fs=48000
co,_=exact(16,30/fs)
def casc(x,cs):
    y=x
    for c in cs: y=lfilter([c,0,-1],[1,0,-c],y)
    return y
def pair(x):
    a=casc(x,co[0::2]); b=casc(x,co[1::2]); b=np.concatenate([[0],b[:-1]])
    return a,b
N=fs*4;n=np.arange(N)
def spec(y):
    seg=y[-2*fs:]*np.blackman(2*fs); S=np.abs(np.fft.rfft(seg))/(0.42*2*fs/2); return S
def lvl(S,f): k=int(round(f*2)); return 20*np.log10(S[k-3:k+4].max()+1e-300)
def shift(x,s,post):
    I,Q=pair(x); ph=2*np.pi*s*n/fs
    # try both sign conventions and report
    wr=I*np.cos(ph)-Q*np.sin(ph); wi=I*np.sin(ph)+Q*np.cos(ph)
    if not post: return wr
    a=casc(wr,co[0::2]); b=casc(wi,co[1::2]); b=np.concatenate([[0],b[:-1]])
    return 0.5*(a-b), 0.5*(a+b)
x=np.sin(2*np.pi*1000*n/fs)
S=spec(shift(x,100,False)); print('direction: 1100 %.1f 900 %.1f'%(lvl(S,1100),lvl(S,900)))
for name,f,s,land,refl in (('zero',100,-150,None,50),('pass',1000,-150,850,None),('zero-near',140,-150,None,10),('zero-far',60,-400,None,340),('wrap',23000,2000,None,23000),('wrap2',21000,3500,None,23500),('pass-hi',15000,2000,17000,None),('low pass',170,-150,20,None),('low2',82,-50,32,None)):
    x=np.sin(2*np.pi*f*n/fs)
    S0=spec(shift(x,s,False)); p,m=shift(x,s,True); Sp=spec(p); Sm=spec(m)
    t=land or refl
    print(name,f,s,'target',t,'raw %.1f | post(a-b) %.1f | post(a+b) %.1f'%(lvl(S0,t),lvl(Sp,t),lvl(Sm,t)))
# 4th-order butter HP at |s|+20 for comparison
b,a=butter(4,170/(fs/2),'high'); x=np.sin(2*np.pi*100*n/fs); S=spec(shift(lfilter(b,a,x),-150,False)); print('BW4 HP 170: 50 Hz refl %.1f'%lvl(S,50))
# dry match null at shift 0: wet(post) vs A(A(x))
rng=np.random.default_rng(1); x=rng.standard_normal(N)
p,m=shift(x,0,True); d=casc(casc(x,co[0::2]),co[0::2])
for nm,w in (('a-b',p),('a+b',m)):
    e=w-d; E=np.abs(np.fft.rfft(e[-fs:]*np.hanning(fs))); D=np.abs(np.fft.rfft(d[-fs:]*np.hanning(fs)))
    for lo,hi in ((5,15),(15,30),(30,100),(100,20000),(20000,23900)):
        print(nm,lo,hi,'null %.1f dB'%(10*np.log10((E[lo:hi]**2).sum()/(D[lo:hi]**2).sum())))
# dry match without post, with BW4 LP at 20k and HP parked 15 Hz and DC 5Hz (original chain) vs A(x)
from scipy.signal import bilinear
def tptlp(fc):  # butter with prewarp = scipy butter
    return butter(4,fc/(fs/2))
b1,a1=tptlp(20000); b2,a2=butter(4,15/(fs/2),'high'); b3,a3=butter(1,5/(fs/2),'high')
u=lfilter(b3,a3,lfilter(b2,a2,lfilter(b1,a1,x))); I,Q=pair(u); d=casc(x,co[0::2]); e=I-d
E=np.abs(np.fft.rfft(e[-fs:]*np.hanning(fs))); D=np.abs(np.fft.rfft(d[-fs:]*np.hanning(fs)))
for lo,hi in ((30,50),(70,100),(200,400),(900,1100),(4500,5500),(9000,11000)):
    print('original chain',lo,hi,'null %.1f dB'%(10*np.log10((E[lo:hi]**2).sum()/(D[lo:hi]**2).sum())))
