import os
# Review check 11: legacy follower is linear-magnitude, ours is dB-domain. Does scaling attack / release improve the preset fit on the DIs?
import numpy as np, sc, ref
exec(open('m5.py').read().split("for sig_name")[0])
from scipy.io import wavfile
def rd(f): sr,x=wavfile.read(os.path.expanduser('~/golem/library/di-refs/guitar/')+f); return (x/2**31)
for dn in ('clean.wav','lead-clean.wav','crunch.wav'):
    x=rd(dn)[:12*fs]; R={}
    for name,p in PRE.items():
        pr={k:v for k,v in p.items() if k!='window'}; f,G,H=ref.ref_gains(x,fs,N=p['window'],**pr)
        Xr=np.abs(np.fft.rfft(np.stack([np.concatenate([np.zeros(p['window']-H),x])[m*H:m*H+p['window']]*sc.hann(p['window']) for m in range(len(x)//H)]),axis=1))**2
        R[name]=per_oct(f,G[10:],Xr[10:])
    for ka,kr in ((1,1),(0.5,1),(1,2.5),(0.5,2.5),(0.5,4)):
        err=[]
        for name,p in PRE.items():
            q=convert(p); q['attack']=p['attack']*ka; q['release']=p['release']*kr
            q=dict(sc.DEF,**q); d=sc.Detector(q,fs); e,X,Ls,g=d.gains(x)
            err.append(per_oct(d.f,g[10:],np.abs(X[10:])**2)-R[name])
        err=np.array(err); print(f"{dn:14s} attack x{ka} release x{kr}: rms {np.sqrt(np.mean(err**2)):.2f} dB, worst {np.abs(err).max():.2f} ({list(PRE)[np.argmax(np.abs(err).max(axis=1))]})")
for m,o in ((60,-6),(70,-2)):
    m_=m/100; g=10**(o/20); G=1-m_+m_*g; print(f"legacy mix {m} %, out {o} dB on the wet  ->  mix {100*m_*g/G:.1f} %, out {20*np.log10(G):+.2f} dB after the mix (exact)")
