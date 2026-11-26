"""Octave Up: the cut. Stage 1 = the amplifier and the 600 Ohm winding as one inductor Lm (1 + sigma)
(secondaries open: no current, so their voltages are exactly +-a/(1+sigma) times the winding's);
stage 2 = two ideal sources (two ideal op-amp gain blocks in circuit_nl: an exact cut) -> half DCR ->
diode -> 500k. What is dropped: the rectifier's current reflected into the primary (and Cw, leakage
on the secondaries). Compared with the monolithic coupled-inductor solve of octave.py."""
import numpy as np
from octave import octave, XF, D1N34A, is_t, Ckt2
from sim import vt_of

fs = 176400.0


def stage1(fs, xf=XF, **kw):
    c = octave(fs, xf=dict(xf, cw=0.0), stage1=True, **kw)
    return c


def stage2(vp, fs, xf=XF, tc=25.0, d=D1N34A):
    a = np.sqrt(1500 / 600) / 2 / (1 + xf['sigma'])
    c = Ckt2(); c.fs = fs; vt = vt_of(tc)
    c.v('s1', '0', 0.0); c.v('s3', '0', 0.0)
    c.r('s1', 'p1', xf['dcr_p'] / 2); c.r('s3', 'p3', xf['dcr_p'] / 2)
    isd = is_t(d['Is'], d['eg'], tc, n=d['n'])
    c.diode('p1', 'out', isd, d['n'] * vt); c.diode('p3', 'out', isd, d['n'] * vt)
    c.r('out', '0', 500e3); c.r('out', 'zz', 1.0); c.c('zz', '0', 1e-15)   # a dummy state for run()
    c.W = []
    c.build(fs); c.dc()
    # drive both sources: run() drives one source, so step manually
    out = np.zeros(len(vp)); its = np.zeros(len(vp), int); v = c.PJ @ c.x_op
    for k in range(len(vp)):
        b = c.rhs_src(); b[c.N + 0] = a * vp[k]; b[c.N + 1] = -a * vp[k]
        x, v, its[k], ok = c.newton(b, v, 1e-8, 20)
        out[k] = c.volt(x, 'out')
    return out, its


if __name__ == '__main__':
    for f0, amp in ((440.0, 0.03), (440.0, 0.1), (440.0, 0.3), (82.4, 0.3), (1318.5, 0.1)):
        n = int(0.1 * fs); t = np.arange(int(0.08 * fs) + n) / fs; u = amp * np.sin(2 * np.pi * f0 * t)
        m = octave(fs, fuzz=0.7); ym, im, bm = m.run(u, 1, ['out', 'w1'])
        s = octave(fs, fuzz=0.7, stage1=True); ys, is_, bs = s.run(u, 1, ['w1'])
        y2, i2 = stage2(ys[:, 0], fs)
        e = ym[-n:, 0] - y2[-n:]; ref = ym[-n:, 0] - ym[-n:, 0].mean()
        print('%6.0f Hz %.2f V: cut vs monolithic %.1f dB re output RMS; iterations: monolithic %.2f, stage 1 %.2f + stage 2 %.2f' % (
            f0, amp, 20 * np.log10(np.sqrt((e ** 2).mean()) / np.sqrt((ref ** 2).mean())), im.mean(), is_.mean(), i2.mean()), flush=True)
