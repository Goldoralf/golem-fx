# SPDX-License-Identifier: GPL-3.0-or-later
"""Measurements of README §1.4 on synthetic strings. Usage: python3 run_synth.py [--quick]"""
import sys
import numpy as np
from scipy.signal import butter, sosfilt
import organ as O

QUICK = '--quick' in sys.argv
SEEDS = [1, 2] if QUICK else [1, 2, 3]
REGS = ['888000000', '008000000', '888800000', '838000000', '808808008', '888888888']
NOTES = ['E2', 'A2', 'D3', 'G3', 'B3', 'E4', 'A4']
DUR = 1.6


def single_notes():
    print('== 1. Registration accuracy, single notes (body 0.4 – 1.4 s) ==')
    print('   per drawbar: RMS / worst level error (dB, best gain); extra = energy off the registration lines')
    res = {}
    for seed in SEEDS:
        rng = np.random.default_rng(100 + seed)
        for nm in NOTES:
            f0 = O.NOTE[nm]
            x = O.string(f0, DUR, O.FB, rng, p=rng.uniform(0.08, 0.25), t0=0.05)
            for law in ('roots', 'all'):
                org = O.Organ(law=law).analyse(x)
                if law == 'roots':
                    org.sieve()
                org.envelope(onsets=[0.05])
                for rg in REGS:
                    org.d = O.drawbar_gain(O.reg(rg))
                    y = org.render()
                    err, extra, _ = O.registration_error(y, f0, org.d)
                    res.setdefault((law, rg), []).append((np.sqrt(np.mean(err ** 2)), np.max(np.abs(err)), extra))
    print('   %-10s %-28s %-28s' % ('reg', 'roots: rms / worst / extra', 'all partials: rms / worst / extra'))
    for rg in REGS:
        a = np.array(res[('roots', rg)])
        b = np.array(res[('all', rg)])
        print('   %-10s %5.2f / %5.2f / %6.1f dB      %5.2f / %5.2f / %6.1f dB' % (
            rg, a[:, 0].mean(), a[:, 1].max(), a[:, 2].max(), b[:, 0].mean(), b[:, 1].max(), b[:, 2].max()))


def cheby(x, d, fs=O.FB):
    """Full-band reference: Chebyshev T_r of the low-passed, envelope-normalised input (integer footages)."""
    sos = butter(4, 1200, fs=fs, output='sos')
    xl = sosfilt(sos, x)
    env = np.maximum(O.one_pole(np.abs(xl), 0.005, fs), 1e-6) * np.pi / 2
    env = np.maximum.accumulate(env[::-1])[::-1] * 0 + env  # keep simple: follower
    xn = np.clip(xl / (env + 1e-9), -1, 1)
    y = np.zeros_like(x)
    th = np.arccos(xn)
    for k, r in enumerate(O.RATIOS):
        if d[k] > 0 and float(r).is_integer():
            y += d[k] * env * np.cos(r * th)
    return y


def true_root_mask(org, f0s, cents=30.0):
    """Bands (× ticks) whose f̂ is within ±cents of a played fundamental."""
    m = np.zeros_like(org.fh, bool)
    for f0 in f0s:
        m |= 1200 * np.abs(np.log2(np.maximum(org.fh, 1) / f0)) < cents
    return m


def chords():
    print('\n== 2. Chords: intermodulation floor, ghost notes, missed notes ==')
    print('   off-grid = energy > 25 ¢ from any multiple of f0/2 of a played note (f0 for the integer registration)')
    print('   ghost = output from root bands that are not a played fundamental, re total (body 0.4 – 1.4 s / attack 0 – 150 ms)')
    for name, notes in O.CHORDS.items():
        f0s = [O.NOTE[n] for n in notes]
        rows = {}
        for seed in SEEDS:
            x = O.chord(notes, DUR, O.FB, seed)
            # roots and all-partials, two registrations
            for law in ('roots', 'all'):
                org = O.Organ(law=law).analyse(x)
                if law == 'roots':
                    org.sieve()
                org.envelope(onsets=[0.05])
                for rg, half in (('888000000', True), ('008880000', False)):
                    org.d = O.drawbar_gain(O.reg(rg))
                    y = org.render()
                    f, S = O.spectrum(y, 0.4, 1.4)
                    og = O.offgrid_db(f, S, f0s, half=half)
                    rows.setdefault((law, rg), []).append(og)
                    if law == 'roots' and rg == '888000000':
                        # ghost energy: re-render with the true roots only
                        keep = true_root_mask(org, f0s)
                        F0 = org.F.copy()
                        org.F = org.F * keep
                        y_true = org.render()
                        org.F = F0
                        g = y - y_true
                        for (t0, t1, key) in ((0.4, 1.4, 'ghost body'), (0.05 + 0.02, 0.07 + 0.15, 'ghost attack')):
                            i0, i1 = int(t0 * O.FB), int(t1 * O.FB)
                            e = np.sum(g[i0:i1] ** 2) / (np.sum(y[i0:i1] ** 2) + 1e-20)
                            rows.setdefault(key, []).append(10 * np.log10(e + 1e-15))
                        # missed notes: played, not covered, never a root on the body
                        body = (org.idx > 0.4 * O.FB) & (org.idx < 1.4 * O.FB)
                        roots = (org.F > 0.5) & (org.s > 0.5) & org.cand
                        miss = 0
                        covered = 0
                        for f0 in f0s:
                            cov = any(abs(1200 * np.log2(f0 / (k * g0))) < 20 for g0 in f0s if g0 < f0 * 0.9
                                      for k in range(2, 9))
                            hit = roots[:, body] & (1200 * np.abs(np.log2(np.maximum(org.fh[:, body], 1) / f0)) < 30)
                            frac = hit.any(axis=0).mean()
                            if cov:
                                covered += 1
                            elif frac < 0.5:
                                miss += 1
                        rows.setdefault('miss', []).append(miss)
                        rows.setdefault('covered', []).append(covered)
            d = O.drawbar_gain(O.reg('008880000'))
            y = cheby(x, d)
            f, S = O.spectrum(y, 0.4, 1.4)
            rows.setdefault(('cheby', '008880000'), []).append(O.offgrid_db(f, S, f0s, half=False))
            f, S = O.spectrum(x, 0.4, 1.4)
            rows.setdefault('dry', []).append(O.offgrid_db(f, S, f0s, half=False))
        r = lambda k: '%6.1f … %6.1f' % (min(rows[k]), max(rows[k]))
        print('  %s' % name)
        print('     off-grid 888000000: roots %s | all %s dB' % (r(('roots', '888000000')), r(('all', '888000000'))))
        print('     off-grid 008880000: roots %s | all %s | full-band Chebyshev %s | dry %s dB' % (
            r(('roots', '008880000')), r(('all', '008880000')), r(('cheby', '008880000')), r('dry')))
        print('     ghost (roots, 888000000): body %s dB, attack %s dB; missed notes %s, covered %s' % (
            r('ghost body'), r('ghost attack'), max(rows['miss']), max(rows['covered'])))


def percussion():
    print('\n== 3. Percussion and envelope (E3, 008000000 + 3rd, decay 200 ms) ==')
    rng = np.random.default_rng(7)
    f0 = O.NOTE['E3']
    x = O.string(f0, 2.0, O.FB, rng, t0=0.05, t60=3.0)
    org = O.Organ(registration='008000000', perc=3, perc_tau=0.2, perc_gain=1.0).analyse(x).sieve()
    org.envelope(onsets=[0.05])
    y = org.render(onsets=[0.05])
    for t in (0.1, 0.3, 0.6, 1.0, 1.8):
        f, S = O.spectrum(y, t - 0.05, t + 0.05)
        a1 = O.comp_level(f, S, f0)
        a3 = O.comp_level(f, S, 3 * f0)
        print('   t = %.2f s: 8\' %6.1f dB, perc (3rd) %6.1f dB re 8\'' % (t, 20 * np.log10(a1 + 1e-12),
                                                                   20 * np.log10(a3 / (a1 + 1e-12) + 1e-12)))
    # input decays 6.9 dB / 0.3 s at T60 3 s; the organ holds
    f, S = O.spectrum(x, 0.25, 0.35)
    a = O.comp_level(f, S, f0)
    f, S = O.spectrum(x, 1.75, 1.85)
    b = O.comp_level(f, S, f0)
    print('   input fundamental fell %.1f dB from 0.3 to 1.8 s' % (20 * np.log10(b / a)))


def attack():
    print('\n== 4. Rise time (clean bank only; the pedal plays the attack on the fast bank, gfx-polysynth Dual) ==')
    for nm in ('E2', 'A2', 'E3', 'E4'):
        rng = np.random.default_rng(3)
        f0 = O.NOTE[nm]
        x = O.string(f0, 1.0, O.FB, rng, t0=0.05)
        org = O.Organ(registration='008000000').analyse(x).sieve()
        org.envelope(onsets=[0.05])
        y = org.render()
        e = np.sqrt(O.one_pole(y ** 2, 0.005, O.FB))
        body = e[int(0.4 * O.FB):int(0.5 * O.FB)].mean()
        t3 = np.argmax(e > body * 10 ** (-3 / 20)) / O.FB - 0.05
        print('   %s: −3 dB of the body %.0f ms after the pick' % (nm, 1000 * t3))


if __name__ == '__main__':
    single_notes()
    chords()
    percussion()
    attack()
