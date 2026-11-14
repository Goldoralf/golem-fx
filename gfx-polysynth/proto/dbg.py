import sys; sys.path.insert(0,'.')
from proto import *
for notes in ([40],[40,47,52],[40,47,52,56,59,64]):
    x = chord(notes, dur=0.5)
    mids, E = cq_envelopes(x, rel_bw=0.5, min_bw=6.0)
    for t in (0.1,0.15,0.25,0.4):
        i=int(t*FS); print(notes, t, name_notes(E[:,i], mids), np.round(E[:12,i]/E[:,i].max(),2))
