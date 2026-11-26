# SPDX-License-Identifier: GPL-3.0-or-later
"""Review measurements (README, Review 2026-11-26): the draft sieve against the reviewed one, on inputs with
a noise floor (−100 dBFS: organ.py's inputs start from digital silence, which blanks the draft's purity for
~90 ms and hides its attack). Usage: python3 run_review.py [--reviewed]   (about 15 min under a 3 GB cap)"""
import sys
import numpy as np
from scipy.signal import butter, sosfiltfilt
import organ as O
from sieve_review import Reviewed, FB, TICK

NOISE = 1e-5
WINS = ((0.07, 0.20), (0.20, 0.45), (0.45, 1.4))
VAR = (('draft', None), ('reviewed', {}), ('reviewed + lowest fast', dict(lowest_fast=True)))
if '--reviewed' in sys.argv:   # the reviewed sieve alone (after a change to it)
    VAR = (('reviewed', {}),)


def make(kw, x, ons, reg='888000000'):
    org = O.Organ(registration=reg).analyse(x).sieve() if kw is None else Reviewed(registration=reg, **kw).analyse(x).sieve(onsets=ons)
    org.envelope(onsets=ons)
    return org


def roots_of(org):
    return (org.F > 0.5) & (org.s > 0.5) & org.cand


def near(org, f0, cents=30.0):
    return 1200 * np.abs(np.log2(np.maximum(org.fh, 1) / f0)) < cents


def ghost(org, keep, y, wins):
    """Output of root bands outside `keep` (bands × ticks), re the organ's output, per window."""
    F0 = org.F.copy()
    org.F = org.F * keep
    yt = org.render()
    org.F = F0
    g = y - yt
    return [10 * np.log10(np.sum(g[int(a * FB):int(b * FB)] ** 2) / (np.sum(y[int(a * FB):int(b * FB)] ** 2) + 1e-20) + 1e-15)
            for a, b in wins]


def bloom(org, f0, t0):
    hit = (roots_of(org) & near(org, f0)).any(axis=0)
    tt = org.idx / FB
    m = hit & (tt > t0)
    return 1000 * (tt[m][0] - t0) if m.any() else np.nan


def play(events, dur, seed):
    """events: (time, note, string id). A string picked again is damped (3 ms) and restarts."""
    rng = np.random.default_rng(seed)
    N = int(dur * FB)
    x = np.zeros(N)
    ev = sorted(events)
    for i, e in enumerate(ev):
        y = O.string(O.NOTE[e[1]], dur, FB, rng, p=rng.uniform(0.1, 0.2), t0=e[0])
        nxt = [f[0] for f in ev[i + 1:] if f[2] == e[2]]
        if nxt:
            i1 = int(nxt[0] * FB)
            n3 = int(0.003 * FB)
            y[i1 - n3:i1] *= np.linspace(1, 0, n3)
            y[i1:] = 0
        x += y
    return x + NOISE * rng.standard_normal(N)


def single_notes():
    print('== 1. Single notes, noise floor −100 dBFS: output of root bands that are not the fundamental (dB re the organ,')
    print('      worst / median of 4 seeds, per window after the pick at 0.05 s); when the fundamental becomes a root ==')
    for label, kw in VAR:
        print(label)
        for nm in ('E2', 'A2', 'D3', 'G3', 'B3', 'E4', 'A4'):
            res, bl, nr = [], [], []
            for seed in range(1, 5):
                rng = np.random.default_rng(seed)
                x = O.string(O.NOTE[nm], 1.2, FB, rng, p=rng.uniform(0.08, 0.25), t0=0.05) + NOISE * rng.standard_normal(int(1.2 * FB))
                org = make(kw, x, [0.05])
                res.append(ghost(org, near(org, O.NOTE[nm]), org.render(), WINS))
                bl.append(bloom(org, O.NOTE[nm], 0.05))
                nr.append(roots_of(org).sum(axis=0).max())
            res = np.array(res)
            print('   %s: ' % nm + ' | '.join('%.2f–%.2f s %6.1f / %6.1f' % (w[0], w[1], res[:, i].max(), np.median(res[:, i]))
                                             for i, w in enumerate(WINS)) + ' | root %3.0f – %3.0f ms after the pick | max roots %d' % (
                np.nanmin(bl), np.nanmax(bl), max(nr)))


def chords():
    print('\n== 2. Chords (20 ms strum), noise floor: ghost-root output per window (worst of 3 seeds); when each note becomes a root ==')
    for name, notes in O.CHORDS.items():
        f0s = [O.NOTE[n] for n in notes]
        print(name)
        for label, kw in VAR:
            res, nr = [], []
            bl = {n: [] for n in notes}
            for seed in (1, 2, 3):
                x = O.chord(notes, 1.2, FB, seed)
                x = x + NOISE * np.random.default_rng(seed).standard_normal(len(x))
                org = make(kw, x, [0.05])
                keep = np.zeros_like(org.fh, bool)
                for f0 in f0s:
                    keep |= near(org, f0)
                res.append(ghost(org, keep, org.render(), WINS))
                for n_ in notes:
                    bl[n_].append(bloom(org, O.NOTE[n_], 0.05))
                nr.append(roots_of(org).sum(axis=0).max())
            res = np.array(res)
            print('   %-23s' % label + ' | '.join('%6.1f' % res[:, i].max() for i in range(3)) + ' dB | root at (ms, worst seed; – = never): '
                  + ' '.join('%s %s' % (n_, '–' if np.all(np.isnan(v)) else '%.0f' % np.nanmax(v)) for n_, v in bl.items())
                  + ' | max roots %d' % max(nr))


SEQ = {
    'arpeggio open E, 0.3 s apart, strings ringing': [(0.05 + 0.3 * i, n, i) for i, n in enumerate(['E2', 'B2', 'E3', 'G#3', 'B3', 'E4'])],
    'melody E3 E4 B3 over a ringing E2': [(0.05, 'E2', 0), (0.5, 'E3', 1), (0.9, 'E4', 2), (1.3, 'B3', 3)],
    'bass E2 enters under a held E4': [(0.05, 'E4', 0), (0.6, 'E2', 1)],
    'bass E2 x3 under an E4 picked alone first': [(0.05, 'E4', 2), (0.5, 'E2', 0), (0.95, 'E2', 0), (1.4, 'E2', 0)],
    'bass E2 x4 under E4 + G#3 strummed with it': [(0.05, 'E2', 0), (0.06, 'G#3', 1), (0.07, 'E4', 2), (0.5, 'E2', 0), (0.95, 'E2', 0), (1.4, 'E2', 0)],
    'octave dyad E3 + E4, together': [(0.05, 'E3', 0), (0.055, 'E4', 1)],
    'single E2 picked x5': [(0.05 + 0.4 * i, 'E2', 0) for i in range(5)],
    'single A2 picked x5': [(0.05 + 0.4 * i, 'A2', 0) for i in range(5)],
    'open E strummed x3, 0.5 s apart': [(0.05 + 0.5 * j + 0.004 * i, n, i) for j in range(3) for i, n in enumerate(['E2', 'B2', 'E3', 'G#3', 'B3', 'E4'])],
}


def sequences():
    print('\n== 3. Sequences, noise floor: share of seeds (4) in which each note is a root 160 – 280 ms after its pick, and')
    print('      160 – 400 ms after the last pick; ghost-root output from 70 ms on (worst / median) ==')
    for name, ev in SEQ.items():
        ev = sorted(ev)
        dur = ev[-1][0] + 0.45
        ons = []
        for e in ev:
            if not ons or e[0] - ons[-1] > 0.08:
                ons.append(e[0])
        names = list(dict.fromkeys(e[1] for e in ev))
        print(name)
        for label, kw in VAR[:2]:
            first, last, gh = [], [], []
            for seed in range(1, 5):
                x = play(ev, dur, seed)
                org = make(kw, x, ons)
                r = roots_of(org)
                tt = org.idx / FB
                first.append([(r[:, (tt > e[0] + 0.16) & (tt < e[0] + 0.28)] & near(org, O.NOTE[e[1]])[:, (tt > e[0] + 0.16) & (tt < e[0] + 0.28)]).any(axis=0).mean() for e in ev])
                m = (tt > ev[-1][0] + 0.16) & (tt < ev[-1][0] + 0.40)
                last.append([(r[:, m] & near(org, O.NOTE[nm])[:, m]).any(axis=0).mean() for nm in names])
                keep = np.zeros_like(org.fh, bool)
                for e in ev:
                    keep[:, tt >= e[0]] |= near(org, O.NOTE[e[1]])[:, tt >= e[0]]
                gh.append(ghost(org, keep, org.render(), ((0.07, dur),))[0])
            first = (np.array(first) > 0.5).mean(axis=0)
            last = (np.array(last) > 0.5).mean(axis=0)
            print('   %-9s after its pick: %s' % (label, ' '.join('%s %.2f' % (e[1], v) for e, v in zip(ev, first))))
            print('   %-9s at the end:     %s | ghost %.1f / %.1f dB' % ('', ' '.join('%s %.2f' % (n, v) for n, v in zip(names, last)), max(gh), np.median(gh)))


def bend():
    print('\n== 4. Bend: a sine at G3, +200 ¢ in 0.3 s and back, 80 0000 000; the 16\' line alone (80 – 135 Hz) ==')
    rng = np.random.default_rng(0)
    N = int(2.0 * FB)
    t = np.arange(N) / FB
    cents = 200 * np.clip((t - 0.5) / 0.3, 0, 1) - 200 * np.clip((t - 1.1) / 0.3, 0, 1)
    x = 0.25 * np.sin(2 * np.pi * np.cumsum(196.0 * 2 ** (cents / 1200)) / FB) * (t >= 0.05) + NOISE * rng.standard_normal(N)
    sos = butter(4, [80, 135], btype='band', fs=FB, output='sos')
    for label, kw in VAR[:2]:
        org = make(kw, x, [0.05], reg='800000000')
        y = sosfiltfilt(sos, org.render())
        e = np.sqrt(O.one_pole(y ** 2, 0.010, FB))
        seg = e[int(0.35 * FB):int(1.6 * FB)]
        print('   %-9s lowest point %.1f dB re the median; under −6 dB for %.0f ms' % (
            label, 20 * np.log10(seg.min() / np.median(seg)), 1000 * np.mean(seg < 0.5 * np.median(seg)) * 1.25))


def settling():
    print('\n== 5. How fast a clean band reads its partial: f̂ error (¢) after the pick, single notes, 4 seeds (worst) ==')
    for nm in ('E2', 'A2', 'E3', 'E4'):
        rows = []
        for seed in range(1, 5):
            rng = np.random.default_rng(seed)
            x = O.string(O.NOTE[nm], 0.6, FB, rng, p=rng.uniform(0.08, 0.25), t0=0.05) + NOISE * rng.standard_normal(int(0.6 * FB))
            org = O.Organ().analyse(x)
            b = np.argmax(org.ldb[:, int(0.4 * FB / TICK)] * near(org, O.NOTE[nm])[:, int(0.4 * FB / TICK)] - 1000 * ~near(org, O.NOTE[nm])[:, int(0.4 * FB / TICK)])
            rows.append([1200 * np.log2(org.fh[b, int((0.05 + d) * FB / TICK)] / (O.NOTE[nm] * np.sqrt(1 + 1e-4))) for d in (0.04, 0.064, 0.09, 0.12, 0.15, 0.2)])
        rows = np.array(rows)
        w = rows[np.argmax(np.abs(rows), axis=0), np.arange(rows.shape[1])]
        print('   %s (τ_b %.0f ms): ' % (nm, 1000 * org.bank.tau[b]) + ' '.join('%d ms %+.0f' % (1000 * d, v) for d, v in zip((0.04, 0.064, 0.09, 0.12, 0.15, 0.2), w)))


if __name__ == '__main__':
    if '--reviewed' not in sys.argv:
        settling()
    bend()
    single_notes()
    chords()
    sequences()
