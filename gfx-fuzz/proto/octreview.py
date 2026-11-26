"""Octave Up: the review's runs (Fable 5.2, 2026-11-26). usage: octreview.py guitar|load|flux|cw|lm|cutload
guitar : the octave through a pickup (not the 100 Ohm buffer every design run used);
load   : the rectifier into the next input and its cable (the design loads it with the 500k pot alone);
flux   : the winding's peak flux linkage Lm * i against the part's rating (8.25 mWb: 11 V rms at 300 Hz);
cw     : the winding-capacitance "null" of the design at 176.4 kHz and at 1.4112 MHz (a trapezoidal artefact?);
lm     : which octave figures survive Lm = 0.3 / 1 / 3 H (for bench rows that do not hang on an unknown);
cutload: the cut against the coupled solve with the output loaded."""
import sys
import numpy as np
from octave import octave, XF, D1N34A, is_t, Ckt2
from sim import vt_of
from octrun import harm, db

fs = 176400.0
SC = (2.5, 6e3, 600e-12, 125e3)        # single coil: L, R, Cp + cable, 250k volume || 250k tone
HB = (4.5, 8e3, 650e-12, 250e3)        # humbucker, 500k pots
LOAD = (1e6, 470e-12)                  # next input 1 MOhm, about 4 m of cable


def tone(f0, amp, settle=0.12, outs=('out', 'w', 'w1'), fs=fs, **kw):
    c = octave(fs, **kw)
    n = int(round(round(0.06 * f0) * fs / f0)); t = np.arange(int(settle * fs) + n) / fs
    y, its, bad = c.run(amp * np.sin(2 * np.pi * f0 * t), 1, list(outs))
    return y, n, its, bad


def h21(y, f0, n, fs=fs):
    h = harm(y, f0, fs, n); return db(h[1]) - db(h[0]), h


def stage2(vp, fs, xf=XF, tc=25.0, d=D1N34A, load=None):
    a = np.sqrt(1500 / 600) / 2 / (1 + xf['sigma'])
    c = Ckt2(); c.fs = fs; vt = vt_of(tc)
    c.v('s1', '0', 0.0); c.v('s3', '0', 0.0)
    c.r('s1', 'p1', xf['dcr_p'] / 2); c.r('s3', 'p3', xf['dcr_p'] / 2)
    isd = is_t(d['Is'], d['eg'], tc, n=d['n'])
    c.diode('p1', 'out', isd, d['n'] * vt); c.diode('p3', 'out', isd, d['n'] * vt)
    c.r('out', '0', 500e3)
    if load: c.r('out', '0', load[0]); c.c('out', '0', load[1])
    else: c.r('out', 'zz', 1.0); c.c('zz', '0', 1e-15)
    c.W = []; c.build(fs); c.dc()
    out = np.zeros(len(vp)); v = c.PJ @ c.x_op; x = c.x_op
    vC = c.PC @ x; iC = np.zeros(len(c.C))
    for k in range(len(vp)):
        b = c.rhs_src(); b[c.N + 0] = a * vp[k]; b[c.N + 1] = -a * vp[k]
        b += c.PC.T @ (c.gc * vC + iC)
        x, v, _, ok = c.newton(b, v, 1e-8, 30)
        vn = c.PC @ x; iC = c.gc * (vn - vC) - iC; vC = vn
        out[k] = c.volt(x, 'out')
    return out


what = sys.argv[1]
if what == 'guitar':
    print('H2-H1 (dB) at the rectified output, fuzz 70 %, source level = EMF peak (pickup) or DI peak (buffer)')
    for name, pk in (('buffer 100 Ohm', None), ('single coil', SC), ('humbucker', HB)):
        for f0 in (82.4, 196.0, 440.0, 659.3, 987.8):
            r = []
            for amp in (0.03, 0.1, 0.3):
                y, n, its, bad = tone(f0, amp, fuzz=0.7, pickup=pk, outs=('out', 'w'))
                d, h = h21(y[:, 0], f0, n); r.append('%+6.1f (duty %.2f)' % (d, (y[-n:, 1] > 0).mean()))
            print('%-15s %6.0f Hz: 30 mV %s | 0.1 V %s | 0.3 V %s' % (name, f0, *r), flush=True)
elif what == 'load':
    for f0, amp in ((440.0, 0.03), (440.0, 0.1), (659.3, 0.05), (196.0, 0.1)):
        for name, ld in (('500k alone', None), ('+ 1M', (1e6, 0.0)), ('+ 1M || 220 pF', (1e6, 220e-12)), ('+ 1M || 470 pF', LOAD), ('+ 1M || 1 nF', (1e6, 1e-9))):
            y, n, its, bad = tone(f0, amp, fuzz=0.7, load=ld, outs=('out',))
            x = y[-n:, 0] - y[-n:, 0].mean(); h = harm(y[:, 0], f0, fs, n)
            Y = np.abs(np.fft.rfft(x * np.hanning(n))); fr = np.arange(len(Y)) * fs / n
            hi = np.sqrt((Y[(fr > 3000) & (fr < 20000)] ** 2).sum() / (Y[(fr > 50) & (fr < 20000)] ** 2).sum())
            print('%4.0f Hz %.2f V %-16s: H1 %6.1f H2 %6.1f H3 %6.1f H4 %6.1f dBV, H2-H1 %+5.1f, rms %.3f V, mean %.2f V, share above 3 kHz %.1f dB' % (
                f0, amp, name, *[db(a) for a in h], db(h[1]) - db(h[0]), x.std(), y[-n:, 0].mean(), db(hi)), flush=True)
elif what == 'flux':
    rated = 11.0 * np.sqrt(2) / (2 * np.pi * 300.0)
    print('rated flux linkage (200 mW at 300 Hz on 600 Ohm, sine): %.2f mWb' % (rated * 1e3))
    for lm in (0.3, 1.0, 3.0):
        for f0, amp in ((41.2, 0.3), (82.4, 0.1), (82.4, 0.3), (82.4, 1.0), (196.0, 0.3), (440.0, 0.3), (440.0, 1.0)):
            y, n, its, bad = tone(f0, amp, fuzz=1.0, xf=dict(XF, lm=lm), stage1=True, outs=('w', 'w1', 'c3'), settle=0.2)
            i = (y[-n:, 0] - y[-n:, 1]) / XF['dcr_s']; lam = lm * (1 + XF['sigma']) * i
            print('Lm %.1f H %5.0f Hz %.1f V: winding current %+.1f .. %+.1f mA, flux linkage %+.1f .. %+.1f mWb (%.1f x rated), winding %+.1f .. %+.1f V' % (
                lm, f0, amp, i.min() * 1e3, i.max() * 1e3, lam.min() * 1e3, lam.max() * 1e3, abs(lam).max() / rated, y[-n:, 1].min(), y[-n:, 1].max()), flush=True)
elif what == 'cw':
    f0, amp = 440.0, 0.1
    for rate in (176400.0, 1411200.0):
        res = {}
        for name, cw in (('Cw 0', 0.0), ('Cw 50 pF', 50e-12)):
            y, n, its, bad = tone(f0, amp, fuzz=0.7, xf=dict(XF, cw=cw), outs=('out',), fs=rate, settle=0.08)
            res[name] = y[-n:, 0]
        e = res['Cw 0'] - res['Cw 50 pF']; r = res['Cw 0'] - res['Cw 0'].mean()
        E = np.abs(np.fft.rfft(e)); fr = np.arange(len(E)) * rate / len(e)
        inb = np.sqrt((E[fr < 20000] ** 2).sum() / (E ** 2).sum())
        print('fs %.0f: Cw 50 pF against Cw 0: null %.1f dB re output rms; share of the difference below 20 kHz %.1f dB; its peak at %.0f kHz' % (
            rate, db(e.std() / r.std()), db(inb), fr[np.argmax(E[1:]) + 1] / 1e3), flush=True)
elif what == 'lm':
    print('H2-H1 (dB), fuzz 70 %, humbucker and buffer, Lm 0.3 / 1 / 3 H')
    for name, pk in (('buffer', None), ('humbucker', HB)):
        for f0, amp in ((440.0, 0.03), (440.0, 0.1), (659.3, 0.05), (987.8, 0.05), (82.4, 0.05), (82.4, 0.3)):
            r = []
            for lm in (0.3, 1.0, 3.0):
                y, n, its, bad = tone(f0, amp, fuzz=0.7, pickup=pk, xf=dict(XF, lm=lm), outs=('out',), load=LOAD)
                d, h = h21(y[:, 0], f0, n); r.append('%+6.1f (H2 %5.1f dBV)' % (d, db(h[1])))
            print('%-9s %5.0f Hz %.2f V: %s' % (name, f0, amp, ' | '.join(r)), flush=True)
elif what == 'cutload':
    for f0, amp in ((440.0, 0.03), (440.0, 0.3), (1318.5, 0.1), (82.4, 0.3)):
        for ld in (None, LOAD, (1e6, 1e-9)):
            n = int(0.06 * fs); t = np.arange(int(0.12 * fs) + n) / fs; u = amp * np.sin(2 * np.pi * f0 * t)
            y0 = octave(fs, fuzz=0.7, xf=dict(XF, cw=0.0), load=ld).run(u, 1, ['out'])[0][:, 0]
            ys = octave(fs, fuzz=0.7, stage1=True).run(u, 1, ['w1'])[0][:, 0]
            yc = stage2(ys, fs, load=ld)
            e = y0[-n:] - yc[-n:]; r = y0[-n:] - y0[-n:].mean()
            print('%6.0f Hz %.2f V, load %s: cut vs coupled solve %.1f dB re output rms' % (f0, amp, 'none' if ld is None else '1M || %.0f pF' % (ld[1] * 1e12), db(e.std() / r.std())), flush=True)
elif what == 'cutband':
    # the same, the difference split in band (< 20 kHz) and out of band, at two rates, and by harmonic
    for rate in (176400.0, 705600.0):
        for f0, amp in ((440.0, 0.3), (1318.5, 0.1)):
            n = int(round(round(0.06 * f0) * rate / f0)); t = np.arange(int(0.12 * rate) + n) / rate; u = amp * np.sin(2 * np.pi * f0 * t)
            y0 = octave(rate, fuzz=0.7, xf=dict(XF, cw=0.0), load=LOAD).run(u, 1, ['out'])[0][:, 0]
            ys = octave(rate, fuzz=0.7, stage1=True).run(u, 1, ['w1'])[0][:, 0]
            yc = stage2(ys, rate, load=LOAD)
            e = y0[-n:] - yc[-n:]; r = y0[-n:] - y0[-n:].mean()
            E = np.abs(np.fft.rfft(e)); Rr = np.abs(np.fft.rfft(r)); fr = np.arange(len(E)) * rate / n; b = fr < 20000
            ha = harm(y0, f0, rate, n); hb = harm(yc, f0, rate, n)
            print('fs %.0f %6.0f Hz %.2f V, 1M || 470 pF: cut vs coupled: all %.1f dB, below 20 kHz %.1f dB re the output in that band; H1..H4 differ by %s dB' % (
                rate, f0, amp, db(e.std() / r.std()), db(np.sqrt((E[b] ** 2).sum() / (Rr[b] ** 2).sum())), ' '.join('%+.2f' % (db(x) - db(y)) for x, y in zip(hb, ha))), flush=True)
elif what == 'edges':
    # how fast the winding's edges are (why 8x and not 4x): 10 - 90 % times at 32x, 1204 Hz, -12 dBFS, fuzz 100 %
    rate = 44100.0 * 32
    y, n, its, bad = tone(1204.0, 0.25, fuzz=1.0, stage1=True, outs=('w1', 'c1'), fs=rate, settle=0.06)
    w = y[-n:, 0]; per = int(round(rate / 1204.0)); seg = w[:3 * per]
    lo, hi = np.percentile(seg, 2), np.percentile(seg, 98); a, b = lo + 0.1 * (hi - lo), lo + 0.9 * (hi - lo)
    st = np.where(seg > b, 2, np.where(seg < a, 0, 1)); t = []; last = None; k0 = 0
    for k in range(len(st)):
        if st[k] != 1:
            if last is not None and st[k] != last and k - k0 > 0: t.append(('rise' if st[k] == 2 else 'fall', (k - k0) / rate * 1e6))
            last = st[k]; k0 = k
    print('winding %.1f .. %.1f V; 10 - 90 %% edges (us): %s' % (lo, hi, ', '.join('%s %.1f' % e for e in t[:8])))
    d = np.abs(np.diff(w)) * rate; print('largest slope %.1f V/us' % (d.max() / 1e6))
elif what == 'devices':
    # O9: how much the unverified device constants matter
    from octave import Q2N5087, Q2N4401
    for name, d in (('1N34A fit (0.1 uA, N 1.3)', D1N34A), ('Is / 5', dict(D1N34A, Is=0.02e-6)), ('Is x 5', dict(D1N34A, Is=0.5e-6)),
                    ('published model (200 pA, N 2.19)', dict(Is=200e-12, n=2.19, eg=0.67))):
        r = []
        for f0, amp in ((440.0, 0.03), (440.0, 0.1), (659.3, 0.05)):
            y, n, its, bad = tone(f0, amp, fuzz=0.7, d=d, outs=('out',)); dd, h = h21(y[:, 0], f0, n)
            r.append('%+5.1f dB (H2 %5.1f dBV)' % (dd, db(h[1])))
        print('%-34s H2-H1: %s' % (name, ' | '.join(r)), flush=True)
    for bf1 in (150.0, 254.0, 400.0):
        for bf23 in (100.0, 170.0, 300.0):
            c = octave(None, vcc=9.0, caps=False, q=(dict(Q2N5087, bf=bf1), dict(Q2N4401, bf=bf23), dict(Q2N4401, bf=bf23)))
            print('2N5087 beta %.0f, 2N4401 beta %.0f: Q3 C %.2f V' % (bf1, bf23, c.volt(c.x_op, 'c3')))
    for bf23 in (100.0, 300.0):
        y, n, its, bad = tone(440.0, 0.03, fuzz=0.7, q=(Q2N5087, dict(Q2N4401, bf=bf23), dict(Q2N4401, bf=bf23)), outs=('out',))
        print('2N4401 beta %.0f: 440 Hz 30 mV H2-H1 %+.1f dB' % (bf23, h21(y[:, 0], 440.0, n)[0]), flush=True)
