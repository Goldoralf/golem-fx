import numpy as np, time, sys
from sim import Ckt, vt_of
from face import AC128, SI, is_t
def mk(bf,iceo,br=4.568): return dict(AC128,Is=iceo/((bf+1)/br+1),bf=bf,br=br)
GE=(mk(75.,80e-6),mk(120.,200e-6),13.2e3,True); SIL=(SI,SI,6.24e3,False)
def taper(rot,b=81.): return (b**rot-1)/(b-1)
def face(fs,kind=GE,src='ideal',gvol=1.0,fuzz=1.0,pu=(2.5,6e3,100e-12,250e3,500e-12),tc=25.,vcc=9.,rsrc=100.,caps=True):
    q1,q2,r3,pnp=kind; c=Ckt(); c.fs=fs; vt=vt_of(tc); s=-1. if pnp else 1.
    c.v('vcc','0',s*vcc); c.v('src','0',0.)
    if src=='ideal': c.r('src','in',rsrc)
    else:
        L,R,Cp,Rv,Cc=pu; a=taper(gvol)
        c.r('src','pa',R); c.l('pa','p',L); c.c('p','0',Cp); c.r('p','0',Rv)
        c.r('p','in',max((1-a)*Rv,1.)); c.r('in','0',max(a*Rv,1.)); c.c('in','0',Cc)
        if src=='buffer': c.r('in','0',1e6)
    c.c('in','b1',2.2e-6); c.r('c1','vcc',33e3); c.r('vcc','o',470.); c.r('o','c2',r3); c.r('e2','b1',100e3)
    f=min(max(fuzz,1e-3),1-1e-3); c.r('e2','w',(1-f)*1e3); c.r('w','0',f*1e3); c.c('w','0',20e-6)
    c.c('o','out',10e-9); c.r('out','0',500e3)
    for (cn,bn,en),d in ((('c1','b1','0'),q1),(('c2','c1','e2'),q2)):
        c.bjt(cn,bn,en,Is=is_t(d,tc),bf=d['bf'],br=d['br'],nf=d['nf'],nr=d['nr'],vt=vt,pnp=pnp)
        if caps: c.c(cn,bn,100e-12 if pnp else 5e-12); c.c(bn,en,400e-12 if pnp else 10e-12)
    c.build(fs); c.dc(); return c
if __name__=='__main__':
    fs=384000.; c=face(fs); t=np.arange(int(0.05*fs))/fs
    t0=time.time(); y,its,bad=c.run(0.1*np.sin(2*np.pi*200*t),1,['out']); print('%.1f us/sample'%((time.time()-t0)/len(t)*1e6), its.mean(), its.max(), bad.sum(), abs(y).max())
