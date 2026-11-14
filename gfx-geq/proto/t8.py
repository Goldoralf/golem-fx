from geq import *
from t6 import scanG
from t7 import mk2
import sys
def sections(cent):
    out=[cent[0]]
    for a,b in zip(cent[:-1],cent[1:]):
        gap=np.log2(b/a); k=int(np.ceil(gap/0.7)); k=max(k,2) if gap>=0.5 else k
        out+= [a*2**(gap*j/k) for j in range(1,k+1)]
    return np.array(out)
if __name__=="__main__":
  nm=sys.argv[1]; lay=dict(OCT=OCT,GT=GT,GB=GB)[nm]; sec=sections(lay); w=(1,.15,.05)
  for sr in (44100,48000,192000):
      scanG(f"{nm} n={len(sec)} {sr} ±12",mk2(lay,sec,sr,.4,w),lay,sr,N=250)
  scanG(f"{nm} n={len(sec)} 48k ±15",mk2(lay,sec,48000,.4,w),lay,48000,N=250,amp=15)
  scanG(f"{nm} n={len(sec)} 48k ±15 c.35",mk2(lay,sec,48000,.35,w),lay,48000,N=100,amp=15)
