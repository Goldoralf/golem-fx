from geq import *
from t4 import scan
import sys
which=sys.argv[1]
if which=='oct':
    for sr in (44100,48000,96000,192000):
        scan(f"OCT11 {sr} c.3 w.1",OCT,sr,c=.3,outer=False,wts=(1,.1,.1),N=120)
    scan(f"OCT11 48k c.3 w.25",OCT,48000,c=.3,outer=False,wts=(1,.25,.1),N=120)
    scan(f"OCT11 48k c.35 w.1",OCT,48000,c=.35,outer=False,wts=(1,.1,.1),N=120)
    scan(f"OCT11 48k c.3 w.1 ±15",OCT,48000,c=.3,outer=False,wts=(1,.1,.1),N=120,amp=15)
    scan(f"OCT11 48k c.3 w.1 ±15 cl30",OCT,48000,c=.3,outer=False,wts=(1,.1,.1),N=120,amp=15,clamp=30)
    scan(f"OCT10 48k c.3 w.1",OCT10,48000,c=.3,outer=False,wts=(1,.1,.1),N=120)
if which=='gt':
    for sr in (44100,96000):
        scan(f"GT7 {sr} c.3 w.1",GT,sr,c=.3,outer=False,wts=(1,.1,.1),N=120)
    scan(f"GT7 48k c.3 w.1 ±15",GT,48000,c=.3,outer=False,wts=(1,.1,.1),N=120,amp=15)
if which=='gb':
    for e in ('neigh','min','max'):
        scan(f"GB7 48k edge={e} w.1",GB,48000,edge=e,c=.3,outer=False,wts=(1,.1,.1),N=120)
        scan(f"GB7 48k edge={e} w.1 dense",GB,48000,edge=e,c=.3,outer=False,wts=(1,.1,.1),N=120,dens=True)
