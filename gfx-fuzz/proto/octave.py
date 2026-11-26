"""Octave Up (transformer octave, design of 2026-11-26): the Tycobrahe-type circuit, values of madbean's
Retrograde24 (2024) build document, topology of PCB Guitar Mania's Octavius schematic (v1.1, 2021).
sim.py's MNA + Newton, plus coupled inductors (a linear transformer) with a trapezoidal companion:
i_w[n] = G v_w[n] + h,  G = (T/2) L^-1,  h = i_w[n-1] + G v_w[n-1]."""
import numpy as np
from sim import Ckt, vt_of, KQ

# devices [U: common SPICE models; betas from Retrograde24's measured hFE]
Q2N4401 = dict(Is=26.03e-15, bf=170.0, br=1.01, cjc=7.3e-12, cje=26e-12)
Q2N5087 = dict(Is=6.73e-15, bf=254.0, br=2.68, cjc=9.5e-12, cje=9e-12)
D1N34A = dict(Is=0.1e-6, n=1.3, eg=0.67)          # 0.30 V at 1 mA [U, fitted to typical figures]

# 42TM022: 1.5 kOhm CT : 600 Ohm CT. Lm on the 600 winding [U]; leakage sigma of each self inductance [U];
# DCR 65 / 45 Ohm [U, distributor listing]
XF = dict(lm=1.0, sigma=0.003, dcr_p=65.0, dcr_s=45.0, cw=50e-12, rcore=None)


class Ckt2(Ckt):
    def __init__(self):
        super().__init__(); self.W = []; self.Lw = None

    def xfmr(self, windings, turns, lm, sigma):
        """windings: [(a, b)], turns relative to winding 0; L_ij = lm n_i n_j + delta_ij sigma lm n_i^2."""
        n = np.array(turns, float)
        self.W = [(self.n(a), self.n(b)) for a, b in windings]
        self.Lw = lm * np.outer(n, n) + np.diag(sigma * lm * n ** 2)

    def add_l(self, a, b, L):
        """one more, uncoupled, inductor (review 2026-11-26: the pickup); call after xfmr()."""
        self.W.append((self.n(a), self.n(b)))
        k = len(self.W); Lw = np.zeros((k, k)); Lw[:k - 1, :k - 1] = self.Lw; Lw[-1, -1] = L; self.Lw = Lw

    def jeval(self, v):
        # exponentials continued linearly beyond 40 N V_T (circuit_nl's EXP_LIM)
        a = v / self.nvt; e = np.exp(np.minimum(a, 40.0)); lin = a > 40.0
        ex = np.where(lin, e * (1 + a - 40.0), e)
        return self.Is * (ex - 1), self.Is * e / self.nvt + 1e-12

    def newton(self, b, v0, tol=1e-8, maxit=20, lim='pnjlim'):
        try:
            return super().newton(b, v0, tol, maxit, lim)
        except np.linalg.LinAlgError:
            return None, v0, maxit, False

    def relin(self, dc=False):
        super().relin(dc)
        if not self.W: return
        M = self.M; P = np.zeros((len(self.W), M))
        for k, (a, b) in enumerate(self.W):
            if a >= 0: P[k, a] = 1
            if b >= 0: P[k, b] = -1
        self.PW = P
        if dc or self.fs is None:
            self.GW = 1e3 * np.eye(len(self.W))
        else:
            self.GW = (0.5 / self.fs) * np.linalg.inv(self.Lw)
        self.G = self.G + P.T @ self.GW @ P

    def run(self, u, src, out, tol=1e-8, maxit=20, cont=True, extrap=True):
        x = self.x_op
        vC = self.PC @ x; iC = np.zeros(len(self.C))
        vW = self.PW @ x; iW = 1e3 * vW; vW = vW * 0      # DC: windings were 1 mS^-1 shorts
        v = self.PJ @ x; vp = v.copy()
        oi = [self.nodes[o] for o in out]
        y = np.zeros((len(u), len(oi))); its = np.zeros(len(u), int); bad = np.zeros(len(u), bool)
        b0 = self.rhs_src(); bprev = None; self.nfall = 0; plain = True
        for k in range(len(u)):
            b = b0.copy(); b[self.N + src] = u[k]
            b += self.PC.T @ (self.gc * vC + iC)
            b -= self.PW.T @ (iW + self.GW @ vW)
            seed = self.pnjlim(2 * v - vp, v) if (extrap and plain) else v
            vp = v; xs, vs = x, v
            x, v, its[k], ok = self.newton(b, seed, tol, maxit)
            plain = ok
            if not ok:
                x, v, i2, ok = self.newton(b, vs, tol, maxit); its[k] += i2
            if not ok and cont and bprev is not None:
                for K in (4, 16):
                    vv = vs
                    for j in range(1, K + 1):
                        x, vv, i2, ok = self.newton(bprev + (b - bprev) * j / K, vv, tol, maxit); its[k] += i2
                        if not ok: break
                    if ok: v = vv; break
                self.nfall += 1
            if not ok: x, v = xs, vs
            bprev = b; bad[k] = not ok
            vn = self.PC @ x; iC = self.gc * (vn - vC) - iC; vC = vn
            vn = self.PW @ x; iW = iW + self.GW @ (vn + vW); vW = vn
            y[k] = x[oi]
        return y, its, bad


def is_t(Is, eg, tc, t0=25.0, n=1.0):
    T, T0 = tc + 273.15, t0 + 273.15
    return Is * (T / T0) ** (3 / n) * np.exp(eg / (n * KQ) * (1 / T0 - 1 / T))


def octave(fs, fuzz=1.0, vcc=9.0, tc=25.0, xf=XF, rsrc=100.0, r3=820e3, r2=680e3, octave_on=True,
           caps=True, q=(Q2N5087, Q2N4401, Q2N4401), d=D1N34A, stage1=False, pickup=None, load=None):
    """pickup (review): (L, R, Cp + cable, Rvol || Rtone) between the source (then the EMF) and `in`, volume
    full up, instead of the 100 Ohm buffer. load (review): (R, C) from `out` to ground, the next input and
    the cable behind the volume pot full up."""
    c = Ckt2(); c.fs = fs; vt = vt_of(tc)
    c.v('vcc', '0', vcc); c.v('src', '0', 0.0)           # sources 0 (supply), 1 (signal)
    if pickup is None:
        c.r('src', 'in', rsrc)                             # `buffer` on: 100 Ohm
    else:
        c.r('src', 'pk', pickup[1]); c.c('in', '0', pickup[2]); c.r('in', '0', pickup[3])
    c.r('in', '0', 1e6)                                    # R1
    c.c('in', 'b1', 100e-9)                                # C1
    c.c('b1', '0', 150e-12)                                # C2
    c.r('b1', '0', r2)                                     # R2 (the bias trimmer)
    c.r('b1', 'x', r3)                                     # R3 820k (Octavius: 680k)
    c.r('x', 'vcc', 180e3); c.c('x', '0', 33e-6)           # R4, filter cap
    c.r('e1', 'y', 1e3)                                    # R7
    c.c('y', 'f', 100e-6)                                  # fuzz cap
    f = min(max(fuzz, 1e-3), 1.0)
    c.r('f', '0', max((1 - f) * 1e3, 10.0))                # 1k B rheostat, 100 % = 10 Ohm (R_MIN)
    c.r('y', 'c3', 47e3)                                   # R8, the loop
    c.r('c1', '0', 220e3)                                  # R5
    c.r('c1', 'z', 220.0); c.c('z', '0', 1e-9)             # R6 + 1n
    c.r('e2', '0', 22e3)                                   # Q2 emitter
    c.r('e3', '0', 470.0); c.c('e3', '0', 220e-6)          # Q3 emitter
    c.r('vcc', 'c3', 1.2e3)                                # R11
    c.c('c3', 'w', 33e-6)                                  # to the 600 Ohm winding
    c.r('w', 'w1', xf['dcr_s'])
    if xf.get('rcore'): c.r('w', '0', xf['rcore'])
    if stage1:                                             # secondaries open: one inductor
        c.xfmr([('w1', '0')], [1.0], xf['lm'] * (1 + xf['sigma']), 0.0)
        if pickup is not None: c.add_l('pk', 'in', pickup[0])
        for (cn, bn, en, pnp), dv in zip((('c1', 'b1', 'e1', True), ('vcc', 'c1', 'e2', False), ('c3', 'e2', 'e3', False)), q):
            c.bjt(cn, bn, en, Is=is_t(dv['Is'], 1.11, tc), bf=dv['bf'], br=dv['br'], nf=1.0, nr=1.0, vt=vt, pnp=pnp)
            if caps: c.c(cn, bn, dv['cjc']); c.c(bn, en, dv['cje'])
        c.build(fs); c.dc(); return c
    hp = xf['dcr_p'] / 2
    c.r('p1', 'p1i', hp); c.r('p3', 'p3i', hp)
    a = np.sqrt(1500 / 600) / 2
    c.xfmr([('w1', '0'), ('p1i', '0'), ('0', 'p3i')], [1.0, a, a], xf['lm'], xf['sigma'])
    if pickup is not None: c.add_l('pk', 'in', pickup[0])
    if load is not None:
        c.r('out', '0', load[0])
        if load[1]: c.c('out', '0', load[1])
    if xf['cw']: c.c('p1', '0', xf['cw']); c.c('p3', '0', xf['cw'])
    isd = is_t(d['Is'], d['eg'], tc, n=d['n'])
    c.diode('p1', 'out', isd, d['n'] * vt)
    if octave_on: c.diode('p3', 'out', isd, d['n'] * vt)
    c.r('out', '0', 500e3)                                 # volume 500k A, full up
    for (cn, bn, en, pnp), dv in zip((('c1', 'b1', 'e1', True), ('vcc', 'c1', 'e2', False), ('c3', 'e2', 'e3', False)), q):
        c.bjt(cn, bn, en, Is=is_t(dv['Is'], 1.11, tc), bf=dv['bf'], br=dv['br'], nf=1.0, nr=1.0, vt=vt, pnp=pnp)
        if caps: c.c(cn, bn, dv['cjc']); c.c(bn, en, dv['cje'])
    c.build(fs); c.dc(); return c


if __name__ == '__main__':
    for vcc, r3 in ((9.26, 820e3), (9.26, 680e3), (9.0, 820e3)):
        c = octave(None, vcc=vcc, r3=r3, caps=False); x = c.x_op
        V = {k: c.volt(x, k) for k in ('b1', 'e1', 'c1', 'e2', 'c3', 'e3')}
        print('Vcc %.2f R3 %.0fk: Q1 E %.2f B %.2f C %.2f | Q2 E %.2f | Q3 C %.2f E %.2f (Ic3 %.2f mA)' % (
            vcc, r3 / 1e3, V['e1'], V['b1'], V['c1'], V['e2'], V['c3'], V['e3'], V['e3'] / 470 * 1e3))
    print('Retrograde24 (9.42 V one-spot; rail 9.26 V read on Q2 C): Q1 E 4.18 B 3.54 C 2.93 | Q2 E 2.38 | Q3 C 4.85 E 1.73')
    for bias in (-100, -50, 0, 50, 100):
        c = octave(None, r2=680e3 * 2 ** (bias / 100), caps=False)
        print('bias %+4d %%: R2 %.0fk -> Q3 C %.2f V' % (bias, 680 * 2 ** (bias / 100), c.volt(c.x_op, 'c3')))
    for tc in (15, 35):
        c = octave(None, tc=tc, caps=False); print('temp %d C: Q3 C %.2f V' % (tc, c.volt(c.x_op, 'c3')))
    for v in (4.5, 18.0):
        c = octave(None, vcc=v, caps=False); print('supply %.1f V: Q3 C %.2f V' % (v, c.volt(c.x_op, 'c3')))
