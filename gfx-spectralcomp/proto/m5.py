import os
# Legacy presets: reference vs converted gfx-spectralcomp (Studio), mean gain per octave, pink noise and the clean DI
import numpy as np, sc, ref
from scipy.io import wavfile
fs=48000
sr,di=wavfile.read(os.path.expanduser('~/golem/library/di-refs/guitar/clean.wav')); di=di/2**31
rng=np.random.default_rng(7)
def pink(n,lev):
    X=np.fft.rfft(rng.standard_normal(n)); f=np.fft.rfftfreq(n,1/fs); X[1:]/=np.sqrt(f[1:]); X[0]=0
    y=np.fft.irfft(X,n); return y*10**(lev/20)/np.std(y)
PRE={'gentle-glue':dict(thresh=-24,down=2,up=1.2,center=420,slope=0,attack=150,release=300,window=2048),
 'de-harsh-vocal':dict(thresh=-30,down=4,up=1,center=3000,slope=-6,attack=20,release=120,window=1024),
 'keys-even':dict(thresh=-22,down=3,up=1.3,center=600,slope=-1.5,attack=50,release=250,window=2048),
 'pad-hold':dict(thresh=-30,down=4,up=3,center=500,slope=0,attack=150,release=500,window=4096),
 'pink-noise-wash':dict(thresh=-40,down=8,up=12,center=420,slope=0,attack=300,release=1000,window=4096),
 'vocal-smooth':dict(thresh=-26,down=3,up=1.5,center=1000,slope=-3,attack=60,release=200,window=2048),
 'vocal-upfront':dict(thresh=-32,down=2,up=3,center=800,slope=0,attack=80,release=300,window=2048)}
def convert(p):
    N=p['window']; q=dict(p)
    q['thresh']=p['thresh']-10*np.log10(N/96)
    q['slope']=0.693*(p['slope']-3)   # tone conversion, chosen by m5fit.py (pink-flat 0.693*s+0.93 fits worse)
    t=0.75*np.log(p['center'])/np.log(22050); q['up']=1/((1/p['up'])*(1-t)+t)
    q.update(range=60,lift_range=24,guard=-96,knee=6,res=2,auto_gain=False)
    return q
OCT=[125,250,500,1000,2000,4000,8000]
def per_oct(f,G,P):
    out=[]
    for fc in OCT:
        m=(f>=fc/np.sqrt(2))&(f<fc*np.sqrt(2)); w=P[:,m]; g=G[:,m]
        out.append(10*np.log10(np.sum(w*10**(g/10))/np.sum(w)))
    return np.array(out)
for sig_name,x in (('pink -18 dBFS',pink(6*fs,-18)),('clean DI',di)):
  print("==",sig_name,"  mean power gain per octave",OCT,"(ref | ours | diff)")
  for name,p in PRE.items():
    pr={k:v for k,v in p.items() if k!='window'}; f,G,H=ref.ref_gains(x,fs,N=p['window'],**pr)
    Xr=np.abs(np.fft.rfft(np.stack([np.concatenate([np.zeros(p['window']-H),x])[m*H:m*H+p['window']]*sc.hann(p['window']) for m in range(len(x)//H)]),axis=1))**2
    gr=per_oct(f,G[10:],Xr[10:])
    q=dict(sc.DEF,**convert(p)); d=sc.Detector(q,fs); e,X,Ls,g=d.gains(x)
    go=per_oct(d.f,g[10:],np.abs(X[10:])**2)
    print(f"{name:16s} ref "+' '.join('%6.1f'%v for v in gr)+" | ours "+' '.join('%6.1f'%v for v in go)+" | max diff %.1f"%np.max(np.abs(gr-go)))
    q2=convert(p); 
  print()
for name,p in PRE.items():
    q=convert(p); print(f"{name}: thresh {q['thresh']:.1f}, slope {q['slope']:.2f}, up {q['up']:.2f}")
