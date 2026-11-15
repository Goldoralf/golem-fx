# Review check 3: does a short smoothing of the GAIN across bins (after the static curve) remove the static images and cut the pre-echo?
import numpy as np, sc
from scipy.ndimage import convolve1d
fs=48000
orig_sg=sc.static_gain
def make(kern):
    def sg(L,T,p,f=None):
        g=orig_sg(L,T,p,f)
        return g if kern is None else convolve1d(g,np.array(kern,float)/np.sum(kern),axis=-1,mode='nearest')
    return sg
def worst(y,f0,D,H):
    s=y[D+fs:D+3*fs]; S=np.abs(np.fft.rfft(s*np.blackman(len(s)))); f=np.fft.rfftfreq(len(s),1/fs)
    c=S[np.argmin(np.abs(f-f0))]; w=-300
    for k in range(1,9):
        for sg_ in (-1,1):
            fq=abs(f0+sg_*k*fs/H)
            if fq<fs/2-50 and abs(fq-f0)>20:
                m=np.abs(f-fq)<3; w=max(w,20*np.log10(S[m].max()/c))
    return w
F=(82.4,110.0,146.8,196.0,261.6,329.6,440.0,659.3,987.8,1975.5,3951.0,7040.0)
rng=np.random.default_rng(3)
n=int(1.4*fs); xp=rng.standard_normal(n)*10**(-80/20); on=int(0.6*fs); s=sc.pluck(329.63,0.8,fs,seed=5); s=s/np.abs(s).max()*10**(-6/20); xp[on:on+len(s)]+=s
def pre(y,D):
    yy=y[D:]; seg=lambda a,b: 20*np.log10(np.sqrt(np.mean(yy[on-int(a*fs/1e3):on-int(b*fs/1e3)]**2))+1e-12)
    pk=20*np.log10(np.sqrt(np.mean(yy[on:on+int(0.01*fs)]**2))); return seg(5,0)-pk,seg(20,5)-pk
for name,kern in (("none",None),("[1 2 1]",[1,2,1]),("[1 4 6 4 1]",[1,4,6,4,1]),("hann 7",[1,6,15,20,15,6,1])):
    sc.static_gain=make(kern)
    for hd in (8,4):
      for W in (1024,2048):
        row=[]; gerr=[]
        for f0 in F:
            t=np.arange(4*fs)/fs; x=10**(-12/20)*np.sin(2*np.pi*f0*t)
            y,A=sc.run(x,fs,mode='studio',window=W,attack=10,release=50,auto_gain=False,hop_div=hd,return_all=True)
            row.append(worst(y,f0,A['D'],A['det'].H)); D=A['D']
            gerr.append(20*np.log10(np.std(y[D+fs:D+3*fs])/np.std(x[fs:3*fs]))+18.0)
        y,A=sc.run(xp,fs,mode='studio',window=W,thresh=-40,down=8,up=8,attack=5,release=100,auto_gain=False,lift_range=24,range=60,hop_div=hd,return_all=True)
        p=pre(y,A['D'])
        print(f"gain kernel {name:12s} N{W} hop N/{hd}: static images worst {max(row):.0f} dBc, median {np.median(row):.0f}; tone gain err max {np.max(np.abs(gerr)):.2f} dB; pre-echo 0-5 ms {p[0]:.1f}, 5-20 ms {p[1]:.1f}")
sc.static_gain=orig_sg
