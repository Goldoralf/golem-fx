import sys; sys.path.insert(0,'.')
from proto import *
from p4 import perband2
import itertools
cfgs=[(2,15,0.5),(2,5,0.5),(4,15,0.5),(4,5,0.5),(4,5,0.33),(4,3,0.5)]
print(f"{'input':28s} "+' '.join(f'o{o} n{n} s{s}'.rjust(12) for o,n,s in cfgs))
for name,notes in CHORDS.items():
    x=chord(notes)
    r=[imd_db(perband2(x,'square',step_erb=s,f_shape=1200,nmax=n,purity=True,order=o),notes) for o,n,s in cfgs]
    print(f"{name:28s} "+' '.join(f'{v:12.1f}' for v in r))
