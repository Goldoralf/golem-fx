# SPDX-License-Identifier: GPL-3.0-or-later
"""The core on golem's guitar DIs (README §1.4). Usage: python3 run_di.py OUT_DIR [take ...]

Takes are read from ~/golem/library/di-refs/guitar/<take>.wav (GFX_DI_DIR overrides), never copied.
No ground truth exists for a DI: the script reports what can be checked without one, and writes renders
(48 kHz, organ only and organ + dry) for listening.
- roots on the 12-TET grid: share of root time within ±30 ¢ of an equal-tempered pitch, the tuning offset
  taken as the median deviation (a ghost from intermodulation or leakage lands anywhere);
- roots per frame (a chord of six strings has at most six roots, fewer when strings are covered);
- pitch classes of the roots, weighted by level (the takes are in E).
"""
import os
import sys
import wave
import numpy as np
from scipy.signal import resample_poly
import organ as O

NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B']


def read_wav(p):
    w = wave.open(p)
    n, sw, fs, ch = w.getnframes(), w.getsampwidth(), w.getframerate(), w.getnchannels()
    b = np.frombuffer(w.readframes(n), np.uint8)
    if sw == 3:
        a = b.reshape(-1, 3)
        x = (a[:, 0].astype(np.int32) | (a[:, 1].astype(np.int32) << 8) | (a[:, 2].astype(np.int32) << 16))
        x = np.where(x >= 1 << 23, x - (1 << 24), x) / float(1 << 23)
    elif sw == 2:
        x = np.frombuffer(b, np.int16) / 32768.0
    else:
        x = np.frombuffer(b, np.int32) / float(1 << 31)
    x = x.reshape(-1, ch).mean(axis=1)
    return x, fs


def write_wav(p, y, fs=48000):
    y = np.clip(y, -1, 1)
    w = wave.open(p, 'wb')
    w.setnchannels(1)
    w.setsampwidth(2)
    w.setframerate(fs)
    w.writeframes((y * 32767).astype(np.int16).tobytes())
    w.close()


def onsets(x, fs):
    """Spectral-flux onsets (the pedal uses gfx-dsp `onset`, detector A)."""
    hop, nf = 256, 1024
    w = np.hanning(nf)
    frames = np.lib.stride_tricks.sliding_window_view(x, nf)[::hop] * w
    S = np.abs(np.fft.rfft(frames, axis=1))
    flux = np.maximum(np.diff(np.log1p(100 * S), axis=0), 0).sum(axis=1)
    thr = np.median(flux) + 3 * np.std(flux)
    on = []
    last = -1
    for i, v in enumerate(flux):
        if v > thr and (last < 0 or i - last > int(0.08 * fs / hop)):
            on.append((i + 1) * hop / fs)
            last = i
    return on


def main():
    out = sys.argv[1]
    takes = sys.argv[2:] or ['clean', 'lead-clean']
    os.makedirs(out, exist_ok=True)
    d = os.environ.get('GFX_DI_DIR', os.path.expanduser('~/golem/library/di-refs')) + '/guitar'
    for take in takes:
        x48, fs = read_wav(os.path.join(d, take + '.wav'))
        x48 = x48[:int(12 * fs)]
        x = O.to_bank(x48, fs)
        on = onsets(x, O.FB)
        renders = {}
        for reg in ('888000000', '008000000', '888800000'):
            org = O.Organ(registration=reg, perc=3 if reg == '888800000' else None).analyse(x).sieve()
            org.envelope(onsets=on)
            y = org.render(onsets=on if org.perc else ())
            renders[reg] = y
        # statistics from the last analysis (the sieve does not depend on the registration)
        roots = (org.F > 0.5) & (org.s > 0.5) & org.cand
        f = org.fh[roots]
        lv = 10 ** (org.ldb[roots] / 20)
        midi = 69 + 12 * np.log2(np.maximum(f, 1) / 440)
        dev = (midi - np.round(midi)) * 100
        off = np.median(dev)
        dev2 = ((midi - off / 100) - np.round(midi - off / 100)) * 100
        on_grid = np.mean(np.abs(dev2) < 30)
        per = roots.sum(axis=0)
        active = per[per > 0]
        pcs = np.zeros(12)
        np.add.at(pcs, np.round(midi - off / 100).astype(int) % 12, lv)
        pcs /= pcs.sum()
        print('== %s (%.1f s, %d onsets) ==' % (take, len(x) / O.FB, len(on)))
        print('   tuning offset %+.1f ¢; roots within ±30 ¢ of the grid: %.1f %% of root time' % (off, 100 * on_grid))
        print('   roots per sounding frame: median %d, 95th pct %d, max %d' % (
            np.median(active), np.percentile(active, 95), active.max()))
        top = np.argsort(pcs)[::-1][:7]
        print('   pitch classes (level-weighted): ' + ', '.join('%s %.0f %%' % (NAMES[i], 100 * pcs[i]) for i in top))
        dry = x / (np.max(np.abs(x)) + 1e-9)
        for reg, y in renders.items():
            y = y / (np.max(np.abs(y)) + 1e-9) * 0.8
            y48 = resample_poly(y, 2, 1)
            write_wav(os.path.join(out, '%s-%s.wav' % (take, reg)), y48)
            write_wav(os.path.join(out, '%s-%s-dry.wav' % (take, reg)),
                      resample_poly(0.6 * y + 0.4 * dry * 0.8, 2, 1))


if __name__ == '__main__':
    main()
