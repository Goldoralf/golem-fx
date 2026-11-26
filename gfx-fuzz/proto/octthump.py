"""Octave Up: the rectifier's envelope (the octave output has no output capacitor). A 330 Hz note,
0.2 V, 250 ms, decaying, from silence: the output through the current 5 Hz first-order DC guard and
through a 20 Hz second-order high-pass; the infrasonic part (< 20 Hz) against the audio."""
import numpy as np
from scipy.signal import butter, sosfilt
from octave import octave
fs = 176400.0
t = np.arange(int(0.6 * fs)) / fs
env = np.where(t > 0.05, np.exp(-(t - 0.05) / 0.25), 0.0)
u = 0.2 * env * np.sin(2 * np.pi * 330.0 * t)
y = octave(fs, fuzz=0.7).run(u, 1, ['out'])[0][:, 0]
y = y - y[0]
lo = butter(4, 20.0, 'low', fs=fs, output='sos'); au = butter(4, 40.0, 'high', fs=fs, output='sos')
for name, sos in (('5 Hz, 1st order', butter(1, 5.0, 'high', fs=fs, output='sos')), ('20 Hz, 2nd order', butter(2, 20.0, 'high', fs=fs, output='sos'))):
    z = sosfilt(sos, y); inf = sosfilt(lo, z); aud = sosfilt(au, z)
    print('%-17s: infrasonic (<20 Hz) peak %.2f V; audio (>40 Hz) peak %.2f V, RMS %.2f V over the note' % (
        name, abs(inf).max(), abs(aud).max(), np.sqrt((aud[int(0.05 * fs):int(0.3 * fs)] ** 2).mean())))
print('raw rectified output: mean over the first 100 ms of the note %.2f V' % y[int(0.05 * fs):int(0.15 * fs)].mean())
