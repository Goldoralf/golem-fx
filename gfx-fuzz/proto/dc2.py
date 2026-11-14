import numpy as np
from scipy.optimize import brentq
from face import *
d=AC128
f=lambda rc,**k: q2(face(rc1=rc,caps=False,**k))
rc=brentq(lambda r:f(r,tc=25)-4.5,10e3,33e3)
print('Ge: Rc1 for 4.5 V at 25C (T0=22.8): %.1fk ; then 15/20/25/30/35C:'%(rc/1e3),['%.2f'%f(rc,tc=t) for t in (15,20,25,30,35)])
print('   spice-N temp law:',['%.2f'%f(rc,tc=t,spice_n=True) for t in (15,25,35)])
print('   supply 12/9/6/4.5/3:',['%.2f'%f(rc,vcc=v) for v in (12,9,6,4.5,3)])
print('   bias x2 / x0.5 :', '%.2f %.2f'%(f(rc*2),f(rc/2)))
# leakage through bR: ICEO = Is((bf+1)/br+1)
for k in (0.25,0.5,1,2,3):
    iceo=k*262.4e-6; br=(d['bf']+1)/(iceo/d['Is']-1)
    print('   leakage %3d%% (ICEO %.0f uA, bR %.2f): Q2 %.2f V'%(k*100,iceo*1e6,br,f(rc,br=br)))
s=dict(SI)
g=lambda rc,**k: q2(face(rc1=rc,caps=False,dev=s,pnp=False,**k))
rcs=brentq(lambda r:g(r)-4.5,10e3,200e3)
print('Si: Rc1 for 4.5 V: %.1fk; 15/35C:'%(rcs/1e3),['%.2f'%g(rcs,tc=t) for t in (15,35)],' supply 6/4.5/3:',['%.2f'%g(rcs,vcc=v) for v in (6,4.5,3)], 'bias x2,x0.5: %.2f %.2f'%(g(rcs*2),g(rcs/2)))
print('face input resistance (small signal, Ge trimmed, fuzz 100%), by perturbation:')
for fz in (1.0,0.5,0.0):
    c=face(rc1=rc,caps=False,fuzz=fz); c.build(None); x=c.dc()
    # AC: need caps as shorts: approximate with tiny-signal transient at 1 kHz later; here DC-coupled estimate of node b1
    i,g_=c.jeval(c.PJ@x); A=c.G+c.Q@(g_[:,None]*c.PJ)
    # short C2 (20uF) and C1 for mid-band: add big conductance across w-0
    A2=A.copy(); w=c.nodes['w']; A2[w,w]+=1e3
    b=np.zeros(c.M); b[c.nodes['b1']]=1.0
    print('   fuzz %.0f%%: Rin(b1) = %.2f kOhm'%(fz*100, np.linalg.solve(A2,b)[c.nodes['b1']]/1e3))
