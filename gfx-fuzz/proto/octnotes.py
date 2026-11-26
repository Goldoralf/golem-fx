"""Octave Up: the octave against the note (the ring on high notes), sustain level and attack level."""
import numpy as np
from octrun import run, db
fs = 176400.0
for amp in (0.05, 0.2):
    r = []
    for f0 in (82.4, 164.8, 329.6, 659.3, 1318.5):
        ho, hw, its, bad, nf, dt, _ = run(fs, f0, amp, 0.7)
        r.append('%6.0f Hz %+5.1f' % (f0, db(ho[1]) - db(ho[0])))
    print('fuzz 70 %%, %.2f V: H2-H1 dB: %s' % (amp, ' | '.join(r)), flush=True)
