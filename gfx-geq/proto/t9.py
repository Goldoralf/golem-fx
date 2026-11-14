from geq import *
from t6 import scanG
from t7 import mk2
from t8 import sections
import sys
nm=sys.argv[1]; lay=dict(OCT=OCT,GT=GT,GB=GB)[nm]; sec=sections(lay); w=(1,.15,.05)
scanG(f"{nm} c.4 cl30 ±15",mk2(lay,sec,48000,.4,w,clamp=30),lay,48000,N=150,amp=15)
scanG(f"{nm} c.35 cl27 ±12",mk2(lay,sec,48000,.35,w),lay,48000,N=150,amp=12)
scanG(f"{nm} c.4 cl30 ±12 44.1k",mk2(lay,sec,44100,.4,w,clamp=30),lay,44100,N=150,amp=12)
