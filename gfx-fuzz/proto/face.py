import numpy as np
from sim import Ckt, vt_of, KQ

AC128 = dict(Is=23.75e-6, bf=44.90, br=4.568, nf=1.168, nr=1.171, eg=0.67, t0=22.8)  # Vt 25.5 mV in the paper
SI = dict(Is=2e-14, bf=300.0, br=5.0, nf=1.0, nr=1.0, eg=1.12, t0=25.0)


def is_t(dev, tc, spice_n=False):
    T, T0 = tc + 273.15, dev['t0'] + 273.15
    n = dev['nf'] if spice_n else 1.0
    return dev['Is'] * (T / T0) ** (3 / n) * np.exp(dev['eg'] / (n * KQ) * (1 / T0 - 1 / T))


def face(dev=AC128, tc=25.0, vcc=9.0, rc1=33e3, fuzz=1.0, leak_iceo=0.0, pnp=True, src='ideal', gvol=1.0,
         pickup=(2.5, 6e3, 100e-12, 250e3, 500e-12), rsrc=100.0, caps=True, t0leak=25.0, br=None, spice_n=False,
         r3=8.2e3, bf=None):
    c = Ckt(); vt = vt_of(tc); Is = is_t(dev, tc, spice_n)
    s = -1.0 if pnp else 1.0
    c.v('vcc', '0', s * vcc)                       # source 0
    c.v('src', '0', 0.0)                           # source 1: the DI / EMF
    if src == 'ideal':
        c.r('src', 'in', rsrc)
    else:                                          # EMF -> R + L -> p ; Cpu ; pot ; cable
        L, R, Cp, Rv, Cc = pickup
        c.r('src', 'pa', R); c.l('pa', 'p', L); c.c('p', '0', Cp)
        c.r('p', '0', Rv)                          # tone pot, full up (its cap is a short at audio)
        a = max(gvol, 1e-4)
        c.r('p', 'in', max((1 - a) * Rv, 1.0)); c.r('in', '0', a * Rv); c.c('in', '0', Cc)
    c.c('in', 'b1', 2.2e-6)
    c.r('c1', 'vcc', rc1)
    c.r('vcc', 'o', 470.0); c.r('o', 'c2', r3)
    c.r('e2', 'b1', 100e3)
    f = min(max(fuzz, 1e-3), 1 - 1e-3)
    c.r('e2', 'w', (1 - f) * 1e3); c.r('w', '0', f * 1e3); c.c('w', '0', 20e-6)
    c.c('o', 'out', 10e-9); c.r('out', '0', 500e3)
    kw = dict(Is=Is, bf=bf or dev['bf'], br=br or dev['br'], nf=dev['nf'], nr=dev['nr'], vt=vt, pnp=pnp)
    c.bjt('c1', 'b1', '0', **kw); c.bjt('c2', 'c1', 'e2', **kw)
    if leak_iceo:
        il = leak_iceo / (dev['bf'] + 1) * 2 ** ((tc - t0leak) / 10)
        # collector -> base leakage of Q2 (PNP: current flows base -> collector outside)
        c.i('c2', 'c1', s * il)
    if caps:
        c.c('c1', 'b1', 100e-12); c.c('c2', 'c1', 100e-12); c.c('b1', '0', 400e-12); c.c('c1', 'e2', 400e-12)
    return c


def q2(c, pnp=True):
    x = c.dc(); return abs(c.volt(x, 'c2'))


if __name__ == '__main__':
    d = AC128
    print('intrinsic I_CEO of the EM set at T0: %.1f uA ; I_CBO %.2f uA' % (
        1e6 * d['Is'] * ((d['bf'] + 1) / d['br'] + 1), 1e6 * d['Is'] * (1 / d['br'] + 1 / (d['bf'] + 1))))
    print('Is(T)/Is(T0) per +10C, law without N: %.2f ; SPICE law (÷N): %.2f' % (
        is_t(d, 35) / is_t(d, 25), is_t(d, 35, True) / is_t(d, 25, True)))
    for label, kw in [('no extra leak', {}), ('+100uA ICEO source on Q2', dict(leak_iceo=100e-6)),
                      ('T0=25 instead of 22.8, +100uA', dict(leak_iceo=100e-6, dev=dict(AC128, t0=25.0))),
                      ('T0=25, no extra', dict(dev=dict(AC128, t0=25.0)))]:
        print('Ge face Q2 collector |V| at 15/25/35 C, %s:' % label,
              ['%.2f' % q2(face(tc=t, caps=False, **kw)) for t in (15, 25, 35)])
    dd = dict(AC128, t0=25.0)
    for rc in (15e3, 18e3, 22e3, 33e3):
        print('  Rc1 %2.0fk -> Q2 %.2f V (T0=25, +100uA)' % (rc / 1e3, q2(face(rc1=rc, caps=False, leak_iceo=100e-6, dev=dd))))
    # trimmer on Q2's collector resistor instead
    for r3 in (8.2e3, 22e3, 47e3):
        print('  R3 %2.0fk -> Q2 %.2f V' % (r3 / 1e3, q2(face(r3=r3, caps=False, dev=dd))))
    print('Ge face, higher-gain devices (bf 70/110 both set to 100):',
          ['%.2f' % q2(face(tc=t, caps=False, dev=dd, bf=100.0)) for t in (15, 25, 35)])
    print('Ge face, supply 4.5 / 3 V (stock Rc1, T0=25, +100uA):',
          ['%.2f' % q2(face(vcc=v, caps=False, leak_iceo=100e-6, dev=dd)) for v in (4.5, 3.0)])
    for bf in (100, 300, 500):
        s = dict(SI, bf=float(bf))
        print('Si face bf %d: Q2 at 15/25/35 C' % bf, ['%.2f' % q2(face(dev=s, tc=t, pnp=False, caps=False)) for t in (15, 25, 35)],
              ' at 4.5 V / 3 V supply:', ['%.2f' % q2(face(dev=s, vcc=v, pnp=False, caps=False)) for v in (4.5, 3.0)])
    for Is in (1e-14, 1e-13):
        s = dict(SI, Is=Is)
        print('Si face Is %.0e: 9V %.2f, 4.5V %.2f, 3V %.2f' % (Is, *[q2(face(dev=s, vcc=v, pnp=False, caps=False)) for v in (9, 4.5, 3)]))
