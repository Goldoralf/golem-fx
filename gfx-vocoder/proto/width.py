import numpy as np
from bank import *
def rip(n,w,spacing,mode,lo=100.,hi=7000.,fs=48000.,ret=False):
    fc,d=centres(lo,hi,n,spacing)
    if mode=='diff':
        d2=np.empty(n); d2[1:-1]=(fc[2:]-fc[:-2])/2; d2[0]=fc[1]-fc[0]; d2[-1]=fc[-1]-fc[-2]
        # extrapolate edges geometrically
        d2[0]=d2[1]*d2[1]/d2[2]; d2[-1]=d2[-2]*d2[-2]/d2[-3]; d=d2
    if mode=='geo':  # geometric mean of the two gaps
        g=np.diff(fc); d2=np.empty(n); d2[1:-1]=np.sqrt(g[1:]*g[:-1]); d2[0]=d2[1]*d2[1]/d2[2]; d2[-1]=d2[-2]*d2[-2]/d2[-3]; d=d2
    b=w*d; f=np.geomspace(lo*1.3,hi/1.3,3000)
    T=20*np.log10(abs(bankresp(f,fc,b,fs).sum(0)))
    return T.max()-T.min()
for sp in ('erb','log'):
  for mode in ('local','diff','geo'):
    print(sp,mode, ' '.join(f"N{n}:{min((rip(n,w,sp,mode),round(w,2)) for w in np.arange(0.5,1.31,0.05))}" .replace('np.float64','') for n in (8,10,11,12,16,20,24,32,40)))
