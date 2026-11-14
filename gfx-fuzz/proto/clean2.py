import numpy as np
from tr import *
fs=384000.
def thd(y,fs,f0):
    n=len(y); w=np.blackman(n); Y=abs(np.fft.rfft(y*w)); b=int(round(f0*n/fs))
    h=[Y[k*b-3:k*b+4].max() for k in range(1,20)]
    return 20*np.log10(np.sqrt(sum(x*x for x in h[1:]))/h[0]), 20*np.log10(h[0]/(w.sum()/2))
t=np.arange(int(0.2*fs))/fs; n0=int(0.1*fs)
L,R,Cp,Rv,Cc=(2.5,6e3,100e-12,250e3,500e-12)
for kind,kn in ((GE,'Ge'),(SIL,'Si')):
  for emf in (0.05,0.15):
    u=emf*np.sin(2*np.pi*200*t)
    for rot in (1.0,0.7,0.5,0.3,0.15):
        c=face(fs,kind,'pickup',rot); y,its,bad=c.run(u,1,['out'],maxit=20,cont=True,extrap=False); r0=thd(y[n0:,0],fs,200.)
        g=Ckt(); a=taper(rot)
        g.v('src','0',0.); g.r('src','pa',R); g.l('pa','p',L); g.c('p','0',Cp); g.r('p','0',Rv); g.r('p','in',max((1-a)*Rv,1.)); g.r('in','0',max(a*Rv,1.)); g.c('in','0',Cc); g.r('in','0',1e6)
        g.build(fs); g.dc(); w,_,_=g.run(u,0,['in'])
        c=face(fs,kind,'ideal'); y,its2,bad2=c.run(w[:,0],1,['out'],maxit=20,cont=True,extrap=False); r1=thd(y[n0:,0],fs,200.)
        print('%s EMF %3.0f mV vol %3d%%: unbuffered THD %6.1f dB level %6.1f dBV | buffered THD %6.1f dB level %6.1f dBV | iters %.2f / %.2f unsolved %d'%(kn,emf*1e3,rot*100,*r0,*r1,its.mean(),its2.mean(),bad.sum()+bad2.sum()),flush=True)
