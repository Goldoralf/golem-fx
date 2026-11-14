import numpy as np
from zoom2 import *
from t2 import sigs
import warnings; warnings.filterwarnings('ignore')
cases=[("clean",110.0,dict()),("200c-below 0dB",110.0,dict(symp=(98.0,0))),("200c-below -10dB",110.0,dict(symp=(98.0,-10))),("200c-below +10dB",110.0,dict(symp=(98.0,10))),
       ("98c-below -6dB (G2/B0x3)",98.0,dict(symp=(30.868,3.5))),("E2 ring +10dB",110.0,dict(symp=(82.407,10)))]
print("late (1.5-2.8 s) max|err| cents, A2 target;  rows = L,R periods / window")
for win in ('hann','blackmanharris'):
  for Lp,Rp in ((6,6),(12,12),(16,8),(16,16),(24,8),(24,24),(32,8),(32,16),(48,16)):
    row=[]
    for nm,f,kw in cases:
        x,true=sigs(f,**{'off':0,**kw})
        o=Zoom2(f,Lp,Rp,win=win,Tmax=9).run(x,0.05)
        t=o[:,0]-0.05; e=cents(o[:,1],true); m=(t>1.5)&(t<2.8)
        row.append(f"{np.nanmax(np.abs(e[m])):8.3f}")
    print(f"{win[:5]:5s} L={Lp:2d} R={Rp:2d} T={(Lp+Rp):3d}p :"," ".join(row))
print("cases:",[c[0] for c in cases])
# baseline design (IIR) with a sane fb
from zoom import Zoom
for mode,Bk,Tp,lo,hi in (('fast',0.08,8,0.04,0.25),('strobe',0.04,30,0.3,1.2)):
    row=[]
    for nm,f,kw in cases:
        x,true=sigs(f,**{'off':0,**kw})
        o=Zoom(f,Bk*f,np.clip(Tp/f,lo,hi),fbmin=400).run(x,0.05)
        t=o[:,0]-0.05; e=cents(o[:,1],true); m=(t>1.5)&(t<2.8)
        row.append(f"{np.nanmax(np.abs(e[m])):8.3f}")
    print(f"design {mode:6s}          :"," ".join(row))
