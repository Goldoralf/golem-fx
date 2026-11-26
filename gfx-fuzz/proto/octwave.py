"""Octave Up: the waveforms (duty of the driven winding, rails of Q3) at 440 Hz."""
import numpy as np
from octrun import run
fs = 176400.0
for amp in (0.03, 0.1, 0.3, 1.0):
    for fuzz in (0.7, 1.0):
        ho, hw, its, bad, nf, dt, y = run(fs, 440.0, amp, fuzz)
        o, w, c3 = y[:, 0], y[:, 1], y[:, 2]
        print('%.2f V fuzz %3.0f%%: Q3 C %.2f..%.2f V; winding %+.2f..%+.2f V, duty(+) %.2f; out peak %.2f V mean %.2f V' % (
            amp, fuzz * 100, c3.min(), c3.max(), w.min(), w.max(), (w > 0).mean(), o.max(), o.mean()), flush=True)
