import numpy as np, scipy.io.wavfile as wav
from scipy.signal import resample_poly
from sim import Ckt, vt_of
VT=vt_of(25.)
def muff(fs,start=1,active=None,sustain=1.0,vdc=0.0):
    """start: first stage present (its input node is driven by V source 1). active: set of nonlinear stages (others frozen)."""
    c=Ckt(); c.fs=fs; act=active or {1,2,3,4}
    q=lambda cn,bn,en,k: c.bjt(cn,bn,en,Is=2e-14,bf=300.,br=5.,nf=1.,nr=1.,vt=VT,freeze=k not in act)
    c.v('vcc','0',9.0)
    drive={1:'in',2:'sw',3:'c2',4:'c3'}[start]; c.v(drive,'0',vdc)
    if start<=1:
        c.c('in','x1',100e-9); c.r('x1','b1',39e3); c.r('b1','0',100e3); c.r('c1','b1',470e3); c.c('c1','b1',470e-12)
        c.r('vcc','c1',10e3); c.r('e1','0',100.); q('c1','b1','e1',1)
        c.c('c1','st',100e-9); c.r('st','sw',max((1-sustain)*100e3,1.)); c.r('sw','0',max(sustain*100e3,1.))
    for k,(i,o) in ((2,('sw','c2')),(3,('c2','c3'))):
        if start<=k:
            c.c(i,'x%d'%k,100e-9); c.r('x%d'%k,'b%d'%k,10e3); c.r('b%d'%k,'0',100e3); c.r(o,'b%d'%k,470e3); c.c(o,'b%d'%k,470e-12)
            c.c(o,'d%d'%k,100e-9)
            c.diode('d%d'%k,'b%d'%k,2.5e-9,1.75*VT,freeze=k not in act); c.diode('b%d'%k,'d%d'%k,2.5e-9,1.75*VT,freeze=k not in act)
            c.r('vcc',o,10e3); c.r('e%d'%k,'0',150.); q(o,'b%d'%k,'e%d'%k,k)
    c.r('c3','A',22e3); c.c('A','0',10e-9); c.c('c3','B',3.9e-9); c.r('B','0',22e3); c.r('A','W',50e3); c.r('W','B',50e3)
    c.c('W','b4',100e-9); c.r('vcc','b4',390e3); c.r('b4','0',100e3); c.r('vcc','c4',10e3); c.r('e4','0',2.7e3); q('c4','b4','e4',4)
    c.c('c4','out',100e-9); c.r('out','0',100e3)
    c.build(fs); c.dc(); return c
fs=384000.
sr,x=wav.read('/home/raph/golem/library/di-refs/guitar/crunch.wav'); x=x.astype(float)/2**31
i=np.argmax(abs(x)); seg=x[max(i-4800,0):i+9600]; u=resample_poly(seg,8,1)
rms=lambda z:np.sqrt((z**2).mean()); db=lambda z:20*np.log10(z+1e-30)
for sus in (1.0,0.5):
  for gain,gn in ((1.0,'DI'),(0.1,'DI -20 dB')):
    cj=c=muff(fs,sustain=sus); yA,its,bad=c.run(u*gain,1,['out','sw','c2','c3'],maxit=20,cont=True)
    print('sustain %.0f%% %s joint: mean iters %.2f max %d, >8: %.3f%%, out rms %.3f V peak %.2f'%(sus*100,gn,its.mean(),its.max(),100*(its>8).mean(),rms(yA[:,0]),abs(yA[:,0]).max()),flush=True)
    sig=u*gain; res=[]
    for k,node in ((1,'sw'),(2,'c2'),(3,'c3'),(4,'out')):
        c=muff(fs,start=k,active={k},sustain=sus,vdc=0.0 if k==1 else cj.volt(cj.x_op,{2:'sw',3:'c2',4:'c3'}[k]))
        y,it2,_=c.run(sig if k==1 else sig,1,[node],maxit=20,cont=True)
        sig=y[:,0]
        res.append(it2.mean())
    n0=int(0.02*fs)
    print('   partitioned (frozen small-signal loads): error %.1f dB re output ; per-stage mean iters %s'%(db(rms(sig[n0:]-yA[n0:,0])/rms(yA[n0:,0])),np.round(res,2)),flush=True)
    for j,node in enumerate(('sw','c2','c3')): pass
