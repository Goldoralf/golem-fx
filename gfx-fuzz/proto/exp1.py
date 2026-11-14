import numpy as np
from tr import *
fs=384000.
def thd(y,fs,f0):
    n=len(y); w=np.blackman(n); Y=abs(np.fft.rfft(y*w)); b=int(round(f0*n/fs))
    h=[Y[k*b-3:k*b+4].max() for k in range(1,20) if k*b+4<len(Y)]
    return 20*np.log10(np.sqrt(sum(x*x for x in h[1:]))/h[0]), 20*np.log10(h[0]/(w.sum()/2))
print('== Newton stats, Ge face, ideal source, 8x (384 kHz)')
t=np.arange(int(0.04*fs))/fs
rng=np.random.default_rng(1)
sigs={'100mV 200Hz':0.1*np.sin(2*np.pi*200*t),'1V 1.2kHz':np.sin(2*np.pi*1200*t),'+24dB 5kHz':15.8*np.sin(2*np.pi*5000*t),'full-scale noise':rng.uniform(-1,1,len(t))}
for name,u in sigs.items():
    for lim,tol,ex,mx in (('pnjlim',1e-8,True,8),('pnjlim',1e-8,False,8),('clamp',1e-7,True,8),('pnjlim',1e-8,True,50)):
        for kind,kn in ((GE,'Ge'),(SIL,'Si')):
            c=face(fs,kind); y,its,bad=c.run(u,1,['out'],tol=tol,maxit=mx,lim=lim,extrap=ex)
            print('%-17s %s %-6s tol %.0e extrap %d cap %2d: mean %.2f max %d not-converged %.3f%%'%(name,kn,lim,tol,ex,mx,its.mean(),its.max(),100*bad.mean()))
print('== cleanup: EMF 0.15 V peak 200 Hz single coil, fuzz 100%')
t=np.arange(int(0.3*fs))/fs; u=0.15*np.sin(2*np.pi*200*t); n0=int(0.2*fs)
for kind,kn in ((GE,'Ge'),(SIL,'Si')):
  for rot in (1.0,0.85,0.7,0.5,0.3):
    r=[]
    for src in ('pickup','buffer'):
        if src=='pickup':
            c=face(fs,kind,'pickup',rot); y,its,bad=c.run(u,1,['out','in'],maxit=50)
        else:  # guitar network unloaded -> ideal buffer -> 100 ohm
            cb=face(fs,kind,'buffer',rot); yb,_,_=None,None,None
            # voltage at the wiper with 1 Mohm load: linear, compute by a run of the guitar network alone
            g=Ckt(); L,R,Cp,Rv,Cc=(2.5,6e3,100e-12,250e3,500e-12); a=taper(rot)
            g.v('src','0',0.); g.r('src','pa',R); g.l('pa','p',L); g.c('p','0',Cp); g.r('p','0',Rv); g.r('p','in',max((1-a)*Rv,1.)); g.r('in','0',max(a*Rv,1.)); g.c('in','0',Cc); g.r('in','0',1e6)
            g.build(fs); g.dc(); w,_,_=g.run(u,0,['in'])
            c=face(fs,kind,'ideal'); y,its,bad=c.run(w[:,0],1,['out','in'],maxit=50)
        d,l=thd(y[n0:,0],fs,200.); r.append((d,l))
    print('%s vol %3d%%: unbuffered THD %6.1f dB level %6.1f dBV | buffered THD %6.1f dB level %6.1f dBV'%(kn,rot*100,*r[0],*r[1]))
