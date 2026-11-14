import sys; sys.path.insert(0,'.')
from proto import *
import p6
for name in ['E5 power (E2 B2 E3)','open E major','A minor barre V','major third low E2 G#2','major third E4 G#4','single E2']:
    notes=CHORDS[name]; x=chord(notes)
    a=imd_db(p6.pb(x,0.5,floor=40.0,order=2),notes)
    b=imd_db(p6.pb(x,0.33,floor=20.0,order=4,nmax=9),notes)
    c=imd_db(fuzz(x),notes)
    print(f'{name:26s} fast(o2 .5ERB f40) {a:6.1f}  clean n9 {b:6.1f}  fuzz {c:6.1f}')
