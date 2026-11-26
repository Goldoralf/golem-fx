"""Octave Up: what sets the octave. Lm of the transformer, the bias (R2), the input level."""
import numpy as np
from octave import XF
from octrun import run, db
fs = 176400.0
print('440 Hz, fuzz 70 %%, buffer on; H2-H1 at the output (dB), at 0.03 / 0.1 / 0.3 / 1 V')
for lm in (0.1, 0.3, 1.0, 3.0):
    for bias in (-50, 0, 50, 100):
        r = []
        for amp in (0.03, 0.1, 0.3, 1.0):
            ho, hw, its, bad, nf, dt, _ = run(fs, 440.0, amp, 0.7, xf=dict(XF, lm=lm), r2=680e3 * 2 ** (bias / 100))
            r.append('%+5.1f' % (db(ho[1]) - db(ho[0])))
        print('Lm %.1f H, bias %+4d %% (R2 %4.0fk): %s   (out H2 at 0.3 V %.1f dBV)' % (lm, bias, 680 * 2 ** (bias / 100), ' '.join(r), db(ho[1])), flush=True)
