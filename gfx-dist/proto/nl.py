# A small nonlinear MNA solver for the 2026-11-26 design of three more circuits (README "Three more
# circuits"): R, C (trapezoidal), voltage sources, unity buffers, the circuit_nl op-amp macromodel
# (tanh or square-law input stage into 30 pF, leak, soft-plus rail clamp, ideal output buffer),
# diodes, Ebers-Moll BJTs and a Shichman-Hodges JFET with an optional soft cutoff. Newton with
# backtracking per sample; device Jacobians by central differences (a prototype, not the product).
# DC point (caps open, gmin), AC analysis (linearized at the DC point), transient.
# Review 2026-11-26: the pair port's `lag` (v_cm latched from the previous sample in `tran`); the circuit
# scripts' alias modes take the settings that move the result (bd2full: model, level, fat, input level;
# odr: spectrum; plexi: contour, supply, gain) and bd2full gains `head` (the IC stage's swing in time).
import numpy as np

VT = 0.025852
GMIN = 1e-12


def expl(x):
    return np.exp(x) if x < 30.0 else np.exp(30.0) * (1.0 + x - 30.0)


def softplus(x, k):
    z = x / k
    return k * (z + np.log1p(np.exp(-z))) if z > 0 else k * np.log1p(np.exp(z))


class Net:
    def __init__(self):
        self.nodes = {"0": -1}
        self.R, self.C, self.V, self.E, self.dev = [], [], [], [], []

    def n(self, name):
        if name not in self.nodes:
            self.nodes[name] = len(self.nodes) - 1
        return self.nodes[name]

    def r(self, a, b, v):
        self.R.append((self.n(a), self.n(b), v))

    def c(self, a, b, v):
        self.C.append((self.n(a), self.n(b), v))

    def v(self, a, fn):  # source to ground; fn(t) or a constant
        self.V.append((self.n(a), fn if callable(fn) else (lambda t, k=fn: k)))

    def buf(self, inp, out):  # ideal unity buffer
        self.E.append((self.n(inp), self.n(out)))

    def diode(self, a, k, Is=2.52e-9, N=1.752):
        self.dev.append(("D", (self.n(a), self.n(k)), dict(Is=Is, nvt=N * VT)))

    def bjt(self, c, b, e, pnp=False, Is=1e-14, bf=300.0, br=4.0):
        self.dev.append(("Q", (self.n(c), self.n(b), self.n(e)), dict(Is=Is, bf=bf, br=br, s=-1.0 if pnp else 1.0)))

    def jfet(self, d, g, s, beta=16e-3, vto=-0.5, lam=0.003, Is=1e-14, smooth=0.0):
        self.dev.append(("J", (self.n(d), self.n(g), self.n(s)), dict(beta=beta, vto=vto, lam=lam, Is=Is, sm=smooth)))

    def pair(self, d, gp, gm, I0=0.93e-3, beta=18e-3, rt=0.0, vto=-0.45, lag=False):
        # lag (review 2026-11-26): the common mode is read from the previous sample's solution (the
        # one-port form of README 2.11); False = solved implicitly. The DC point is always implicit.
        self.dev.append(("P", (self.n(d), self.n(gp), self.n(gm)), dict(I0=I0, beta=beta, rt=rt, vto=vto, lag=lag, vcm=None)))

    def opamp(self, p, m, out, sr=1.7e6, gbw=3e6, a0=1e5, hi=7.5, lo=1.5, square=False):
        w = "w_" + out
        cc = 30e-12
        self.c(w, "0", cc)
        self.buf(w, out)
        it = sr * cc
        vlin = sr / (2 * np.pi * gbw)
        self.dev.append(("A", (self.n(p), self.n(m), self.n(w)),
                         dict(it=it, vlin=vlin, g=it / vlin / a0, hi=hi, lo=lo, sq=square)))

    # device currents leaving each terminal into the device (KCL: + at the node equation)
    @staticmethod
    def dcur(kind, p, v):
        if kind == "D":
            x = v[0] - v[1]
            i = p["Is"] * (expl(x / p["nvt"]) - 1.0)
            return [i, -i]
        if kind == "Q":
            s = p["s"]
            vbe, vbc = s * (v[1] - v[2]), s * (v[1] - v[0])
            ef, er = expl(vbe / VT) - 1.0, expl(vbc / VT) - 1.0
            ic = p["Is"] * (ef - er) - p["Is"] / p["br"] * er
            ib = p["Is"] / p["bf"] * ef + p["Is"] / p["br"] * er
            return [s * ic, s * ib, -s * (ic + ib)]
        if kind == "J":
            pd, vg, ps = v
            sw = pd < ps
            ed, es = (ps, pd) if sw else (pd, ps)
            vgs, vds = vg - es, ed - es
            x = vgs - p["vto"]
            vov = softplus(x, p["sm"]) if p["sm"] > 0 else max(x, 0.0)
            if vds < vov:
                i = p["beta"] * vds * (2 * vov - vds) * (1 + p["lam"] * vds)
            else:
                i = p["beta"] * vov * vov * (1 + p["lam"] * vds)
            if sw:
                i = -i  # into the physical drain
            igs = p["Is"] * (expl((vg - ps) / VT) - 1)
            igd = p["Is"] * (expl((vg - pd) / VT) - 1)
            return [i - igd, igs + igd, -i - igs]
        if kind == "P":
            # a square-law JFET pair on a fixed tail I0: the + device's drain current (1 port)
            vd = v[1] - v[2]
            I0, b = p["I0"], p["beta"]
            if p.get("rt"):
                # the tail follows the common mode: I0 = v_s / R_t, v_s = v_cm - V_GS(I0 / 2)
                a = 1.0 / np.sqrt(2 * b)
                vcm = p["vcm"] if (p.get("lag") and p.get("vcm") is not None) else 0.5 * (v[1] + v[2])
                u = (-a + np.sqrt(a * a + 4 * p["rt"] * max(vcm - p["vto"], 0.0))) / (2 * p["rt"])
                I0 = u * u
            lim = np.sqrt(I0 / b)
            if abs(vd) < lim:
                dl = b * vd * np.sqrt(2 * I0 / b - vd * vd)
            else:
                dl = I0 * np.sign(vd)
            return [0.5 * (I0 + dl), 0.0, 0.0]
        if kind == "A":
            vp, vm, w = v
            x = (vp - vm) / p["vlin"]
            if p["sq"]:
                y = 0.5 * x
                gin = p["it"] * (y * (2 - abs(y)) if abs(y) < 1 else np.sign(y))
            else:
                gin = p["it"] * np.tanh(x)
            mid = 0.5 * (p["hi"] + p["lo"])
            gc = p["it"] / 0.05
            clamp = gc * (softplus(w - p["hi"], 0.05) - softplus(p["lo"] - w, 0.05))
            return [0.0, 0.0, -gin + p["g"] * (w - mid) + clamp]
        raise ValueError(kind)

    def build(self, h):
        self.nn = len(self.nodes) - 1
        self.nv = len(self.V) + len(self.E)
        N = self.nn + self.nv
        G = np.zeros((N, N))

        def st(a, b, g):
            if a >= 0:
                G[a, a] += g
            if b >= 0:
                G[b, b] += g
            if a >= 0 and b >= 0:
                G[a, b] -= g
                G[b, a] -= g
        for a, b, r in self.R:
            st(a, b, 1.0 / r)
        for k in range(self.nn):
            G[k, k] += GMIN
        self.Gr = G.copy()
        if h:
            for a, b, c in self.C:
                st(a, b, 2 * c / h)
        k = self.nn
        for a, fn in self.V:
            G[a, k] += 1
            G[k, a] += 1
            k += 1
        for i, o in self.E:
            G[o, k] += 1
            G[k, o] += 1
            G[k, i] -= 1
            k += 1
        self.G = G
        self.N = N

    def F(self, x, b):
        f = self.G @ x - b
        J = self.G.copy()
        for kind, nodes, p in self.dev:
            v = [x[n] if n >= 0 else 0.0 for n in nodes]
            i = self.dcur(kind, p, v)
            for a, ia in zip(nodes, i):
                if a >= 0:
                    f[a] += ia
            for j, nj in enumerate(nodes):
                if nj < 0:
                    continue
                e = 1e-7
                vp, vm = list(v), list(v)
                vp[j] += e
                vm[j] -= e
                ip, im = self.dcur(kind, p, vp), self.dcur(kind, p, vm)
                for a, ia, ib in zip(nodes, ip, im):
                    if a >= 0:
                        J[a, nj] += (ia - ib) / (2 * e)
        return f, J

    def newton(self, x, b, cap=40, tol=1e-9):
        its = 0
        for _ in range(cap):
            its += 1
            f, J = self.F(x, b)
            dx = np.linalg.solve(J, -f)
            m = np.max(np.abs(dx[: self.nn]))
            if m > 0.3:  # crude limiting: no node moves more than 0.3 V per iteration
                dx *= 0.3 / m
            n0 = f @ f
            s = 1.0
            for _ in range(8):
                xt = x + s * dx
                ft, _ = self.F(xt, b)
                if ft @ ft <= n0 * (1 - 0.2 * s) or n0 == 0:
                    break
                s *= 0.5
            x = x + s * dx
            if np.max(np.abs(s * dx[: self.nn])) < tol:
                return x, its, True
        return x, its, False

    def rhs_src(self, t):
        b = np.zeros(self.N)
        k = self.nn
        for a, fn in self.V:
            b[k] = fn(t)
            k += 1
        return b

    def dc(self, t=0.0):
        self.build(0)
        x = np.zeros(self.N)
        b = self.rhs_src(t)
        ok = False
        for steps in (1, 10, 50):
            x = np.zeros(self.N)
            ok = True
            for s in range(1, steps + 1):
                x, _, ok1 = self.newton(x, b * s / steps, cap=200)
                ok = ok and ok1
            if ok:
                break
        self.xdc = x
        return x, ok

    def node(self, x, name):
        k = self.nodes[name]
        return x[k] if k >= 0 else 0.0

    def ac(self, freqs, src_index, out):
        """Small-signal response from voltage source `src_index` to node `out` at the DC point."""
        x = self.xdc
        self.build(0)
        _, J = self.F(x, self.rhs_src(0))
        Cm = np.zeros((self.N, self.N))
        for a, b2, c in self.C:
            for (p, q, sg) in ((a, a, 1), (b2, b2, 1), (a, b2, -1), (b2, a, -1)):
                if p >= 0 and q >= 0:
                    Cm[p, q] += sg * c
        res = []
        for f in freqs:
            A = J + 2j * np.pi * f * Cm
            bb = np.zeros(self.N, complex)
            bb[self.nn + src_index] = 1.0
            y = np.linalg.solve(A, bb)
            res.append(y[self.nodes[out]])
        return np.array(res)

    def tran(self, fs, n, probes, x0=None):
        h = 1.0 / fs
        self.build(h)
        x = self.xdc.copy() if x0 is None else x0
        vc = np.array([(x[a] if a >= 0 else 0) - (x[b] if b >= 0 else 0) for a, b, c in self.C])
        ic = np.zeros(len(self.C))
        geq = np.array([2 * c / h for a, b, c in self.C])
        out = np.zeros((n, len(probes)))
        its = np.zeros(n, int)
        fails = 0
        xp = x.copy()
        pidx = [self.nodes[p] for p in probes]
        bprev = None
        lagged = [(nodes, p) for kind, nodes, p in self.dev if kind == "P" and p.get("lag")]

        def latch(xs):
            for nodes, p in lagged:
                p["vcm"] = 0.5 * sum(xs[m] if m >= 0 else 0.0 for m in nodes[1:])
        latch(x)
        for k in range(n):
            b = self.rhs_src(k * h)
            hist = geq * vc + ic
            for j, (a, b2, c) in enumerate(self.C):
                if a >= 0:
                    b[a] += hist[j]
                if b2 >= 0:
                    b[b2] -= hist[j]
            seed = 2 * x - xp if k > 1 else x
            xn, it, ok = self.newton(seed, b, cap=20, tol=1e-6)
            if not ok:
                xn, it2, ok = self.newton(x.copy(), b, cap=20, tol=1e-6)
                it += it2
            # continuation in the right-hand side from the previous sample's (solved) one, 4 then 16 steps
            for steps in (4, 16):
                if ok or k == 0:
                    break
                xn, ok = x.copy(), True
                for s in range(1, steps + 1):
                    xn, it2, ok1 = self.newton(xn, bprev + (b - bprev) * s / steps, cap=20, tol=1e-6)
                    it += it2
                    ok = ok and ok1
            if not ok:
                fails += 1
                xn = x.copy()  # hold the previous solution (circuit_nl's last tier)
            bprev = b
            its[k] = it
            vn = np.array([(xn[a] if a >= 0 else 0) - (xn[b2] if b2 >= 0 else 0) for a, b2, c in self.C])
            ic = geq * (vn - vc) - ic
            vc = vn
            xp, x = x, xn
            latch(x)
            out[k] = [x[p] for p in pidx]
        return out, its, fails


def harmonics(y, fs, f0, nh=9):
    """Amplitudes of harmonics 1..nh (y holds an integer number of periods)."""
    n = len(y)
    X = np.abs(np.fft.rfft(y)) / n * 2
    k0 = int(round(f0 * n / fs))
    return np.array([X[k0 * k] for k in range(1, nh + 1)])


def worst_alias(y, fs, f0, band=20000.0, guard=6):
    """Worst non-harmonic component in 50 Hz - `band`, re the fundamental. y: an integer number of
    periods; detrended and Blackman-Harris windowed (the coupling capacitors' slow settling and the
    harmonics' leakage stay > 90 dB down `guard` bins away)."""
    n = len(y)
    t = np.arange(n)
    y = y - np.polyval(np.polyfit(t, y, 1), t)
    w = np.blackman(n) if False else (0.35875 - 0.48829 * np.cos(2 * np.pi * t / n) + 0.14128 * np.cos(4 * np.pi * t / n)
                                      - 0.01168 * np.cos(6 * np.pi * t / n))
    X = np.abs(np.fft.rfft(y * w))
    df = fs / n
    k0 = int(round(f0 / df))
    ks = np.arange(int(50 / df), int(band / df) + 1)
    r = ks % k0
    cand = ks[np.minimum(r, k0 - r) > guard]
    k = cand[np.argmax(X[cand])]
    return 20 * np.log10(X[k] / X[k0]), k * df
