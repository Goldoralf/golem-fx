import numpy as np
from scipy.optimize import brentq
from sim import Ckt, vt_of
from face import AC128, SI, is_t
def face2(q1,q2,tc=25.,vcc=9.,rc1=33e3,r3=8.2e3,pnp=True,leak=1.0):
    c=Ckt(); vt=vt_of(tc); s=-1. if pnp else 1.
    c.v('vcc','0',s*vcc); c.r('c1','vcc',rc1); c.r('vcc','o',470.); c.r('o','c2',r3); c.r('e2','b1',100e3); c.r('e2','0',1e3)
    for (cn,bn,en),d in ((('c1','b1','0'),q1),(('c2','c1','e2'),q2)):
        c.bjt(cn,bn,en,Is=is_t(d,tc),bf=d['bf'],br=d['br']/max(leak,1e-9),nf=d['nf'],nr=d['nr'],vt=vt,pnp=pnp)
    return abs(c.volt(c.dc(),'c2'))
def mk(bf,iceo,br=4.568): 
    Is=iceo/((bf+1)/br+1); return dict(AC128,Is=Is,bf=bf,br=br)
A=AC128
for name,q1,q2 in (('DAFx-17 AC128 x2',A,A),('graded: Q1 b75 ICEO 80uA, Q2 b120 ICEO 200uA',mk(75.,80e-6),mk(120.,200e-6)),('graded low-leak: Q1 b80 40uA, Q2 b110 100uA',mk(80.,40e-6),mk(110.,100e-6))):
    print(name,' Is uA: %.2f %.2f'%(q1['Is']*1e6,q2['Is']*1e6))
    print('   stock 33k/8.2k, 15/25/35C:',['%.2f'%face2(q1,q2,tc=t) for t in (15,25,35)])
    try:
        r3=brentq(lambda r:face2(q1,q2,r3=r)-4.5,1e3,200e3)
        print('   R3 trim for 4.5 V: %.1fk -> 5/15/25/35/45C:'%(r3/1e3),['%.2f'%face2(q1,q2,tc=t,r3=r3) for t in (5,15,25,35,45)],' supply 6/4.5/3:',['%.2f'%face2(q1,q2,vcc=v,r3=r3) for v in (6,4.5,3)])
        print('   leakage 0/50/200/400%%:',['%.2f'%face2(q1,q2,r3=r3,leak=k) for k in (0,.5,2,4)], ' bias x2 / x0.5 (R3):', '%.2f %.2f'%(face2(q1,q2,r3=2*r3),face2(q1,q2,r3=r3/2)))
    except Exception as e: print('   R3 trim fails',e)
    try:
        rc=brentq(lambda r:face2(q1,q2,rc1=r)-4.5,5e3,150e3)
        print('   Rc1 trim for 4.5 V: %.1fk -> 15/25/35C:'%(rc/1e3),['%.2f'%face2(q1,q2,tc=t,rc1=rc) for t in (15,25,35)])
    except Exception as e: print('   Rc1 trim fails')
s=SI
print('Si stock:',face2(s,s,pnp=False),' Rc1 100k/200k:',face2(s,s,pnp=False,rc1=100e3),face2(s,s,pnp=False,rc1=200e3))
r3=brentq(lambda r:face2(s,s,pnp=False,r3=r)-4.5,1e3,20e3)
print('Si R3 trim: %.2fk -> 15/35C'%(r3/1e3),['%.2f'%face2(s,s,pnp=False,r3=r3,tc=t) for t in (15,35)],' supply 6/4.5/3/18:',['%.2f'%face2(s,s,pnp=False,r3=r3,vcc=v) for v in (6,4.5,3,18)],' R3 x2/x0.5:',['%.2f'%face2(s,s,pnp=False,r3=r3*k) for k in (2,.5)])
