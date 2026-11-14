"""Tiny MNA + Newton circuit simulator (review prototype for gfx-fuzz). Trapezoidal companions,
junction ports v = P x, currents injected by Q i(v): the same structure as a nodal DK solve."""
import numpy as np

KQ = 8.617333262e-5  # V/K


def vt_of(tc):
    return KQ * (tc + 273.15)


class Ckt:
    def __init__(self):
        self.nodes = {}
        self.R, self.C, self.L, self.V, self.I = [], [], [], [], []
        self.J = []      # junctions: (p, n, Is, nvt)
        self.B = []      # branches: (a, b, {j: coef})
        self.frozen = set()
        self.fs = None
        self.built = False

    def n(self, name):
        if name == '0':
            return -1
        if name not in self.nodes:
            self.nodes[name] = len(self.nodes)
        return self.nodes[name]

    def r(self, a, b, R): self.R.append([self.n(a), self.n(b), R]); return len(self.R) - 1
    def c(self, a, b, C): self.C.append((self.n(a), self.n(b), C))
    def l(self, a, b, L): self.L.append((self.n(a), self.n(b), L))
    def v(self, a, b, val=0.0): self.V.append([self.n(a), self.n(b), val]); return len(self.V) - 1
    def i(self, a, b, val): self.I.append([self.n(a), self.n(b), val]); return len(self.I) - 1

    def junction(self, p, n, Is, nvt):
        self.J.append((self.n(p), self.n(n), Is, nvt)); return len(self.J) - 1

    def diode(self, a, k, Is, nvt, freeze=False):
        j = self.junction(a, k, Is, nvt)
        self.B.append((self.n(a), self.n(k), {j: 1.0}))
        if freeze: self.frozen.add(j)
        return j

    def bjt(self, c, b, e, Is, bf, br, nf, nr, vt, pnp=False, freeze=False):
        if not pnp:
            jf = self.junction(b, e, Is, nf * vt); jr = self.junction(b, c, Is, nr * vt)
            self.B += [(self.n(b), self.n(e), {jf: 1 / bf}), (self.n(b), self.n(c), {jr: 1 / br}),
                       (self.n(c), self.n(e), {jf: 1.0, jr: -1.0})]
        else:
            jf = self.junction(e, b, Is, nf * vt); jr = self.junction(c, b, Is, nr * vt)
            self.B += [(self.n(e), self.n(b), {jf: 1 / bf}), (self.n(c), self.n(b), {jr: 1 / br}),
                       (self.n(e), self.n(c), {jf: 1.0, jr: -1.0})]
        if freeze: self.frozen |= {jf, jr}
        return jf, jr

    # ---- assembly
    def build(self, fs=None):
        N = len(self.nodes); M = N + len(self.V)
        self.N, self.M = N, M

        def inc(pairs):
            P = np.zeros((len(pairs), M))
            for k, (a, b) in enumerate(pairs):
                if a >= 0: P[k, a] = 1
                if b >= 0: P[k, b] = -1
            return P
        self.PR = inc([(a, b) for a, b, _ in self.R])
        self.PC = inc([(a, b) for a, b, _ in self.C])
        self.PL = inc([(a, b) for a, b, _ in self.L])
        self.PJ = inc([(p, n) for p, n, _, _ in self.J])
        self.PI = inc([(a, b) for a, b, _ in self.I])
        self.Is = np.array([j[2] for j in self.J]); self.nvt = np.array([j[3] for j in self.J])
        self.vcrit = self.nvt * np.log(self.nvt / (np.sqrt(2) * self.Is))
        Q = np.zeros((M, len(self.J)))
        for a, b, co in self.B:
            for j, cf in co.items():
                if a >= 0: Q[a, j] += cf
                if b >= 0: Q[b, j] -= cf
        self.Q = Q
        self.fz = np.array([j in self.frozen for j in range(len(self.J))], bool)
        self.fs = fs
        self.built = True
        self.relin()

    def relin(self, dc=False):
        M, N = self.M, self.N
        G = np.zeros((M, M))
        G[:N, :N] += 1e-12 * np.eye(N)
        gR = np.array([1 / r[2] for r in self.R]) if self.R else np.zeros(0)
        G += self.PR.T @ (gR[:, None] * self.PR)
        if dc or self.fs is None:
            self.gc = np.zeros(len(self.C)); self.gl = np.full(len(self.L), 1e3)
        else:
            T = 1 / self.fs
            self.gc = np.array([2 * c[2] / T for c in self.C]); self.gl = np.array([T / (2 * l[2]) for l in self.L])
        G += self.PC.T @ (self.gc[:, None] * self.PC) + self.PL.T @ (self.gl[:, None] * self.PL)
        for k, (a, b, _) in enumerate(self.V):
            r = N + k
            if a >= 0: G[a, r] += 1; G[r, a] += 1
            if b >= 0: G[b, r] -= 1; G[r, b] -= 1
        self.G = G

    def rhs_src(self, scale=1.0):
        b = np.zeros(self.M)
        for k, (_, _, val) in enumerate(self.V): b[self.N + k] = val * scale
        if self.I:
            b -= self.PI.T @ (np.array([i[2] for i in self.I]) * scale)
        return b

    def jeval(self, v):
        e = np.exp(np.minimum(v / self.nvt, 80.0))
        return self.Is * (e - 1), self.Is * e / self.nvt + 1e-12

    def pnjlim(self, vn, vo):
        out = vn.copy()
        for k in range(len(vn)):
            vt = self.nvt[k]
            if vn[k] > self.vcrit[k] and abs(vn[k] - vo[k]) > 2 * vt:
                if vo[k] > 0:
                    arg = 1 + (vn[k] - vo[k]) / vt
                    out[k] = vo[k] + vt * np.log(arg) if arg > 0 else self.vcrit[k]
                else:
                    out[k] = vt * np.log(vn[k] / vt)
        return out

    def newton(self, b, v0, tol=1e-8, maxit=8, lim='pnjlim'):
        """returns x, v, iterations, converged."""
        v = v0.copy()
        x = None
        for it in range(1, maxit + 1):
            ve = np.where(self.fz, self.v_op, v) if self.fz.any() else v
            i, g = self.jeval(ve)
            A = self.G + self.Q @ (g[:, None] * self.PJ)
            x = np.linalg.solve(A, b - self.Q @ (i - g * ve))
            if not np.isfinite(x).all():
                return x, v0, it, False
            vn = self.PJ @ x
            d = np.abs(vn - v)[~self.fz]
            if d.size == 0 or d.max() < tol:
                return x, vn, it, True
            if lim == 'pnjlim':
                v = self.pnjlim(vn, v)
            elif lim == 'clamp':     # the README's literal wording: at most 2 N Vt per iteration
                v = v + np.clip(vn - v, -2 * self.nvt, 2 * self.nvt)
            else:
                v = vn
        return x, v, maxit, False

    def dc(self, steps=20):
        if not self.built: self.build(self.fs)
        self.relin(dc=True)
        v = np.zeros(len(self.J)); self.v_op = v
        fz = self.fz; self.fz = np.zeros_like(fz)
        for s in np.linspace(0.05, 1, steps):
            x, v, it, ok = self.newton(self.rhs_src(s), v, tol=1e-11, maxit=300)
            assert ok, 'dc not converged'
        self.fz = fz
        self.x_op, self.v_op = x, self.PJ @ x
        self.relin()
        return x

    def volt(self, x, name):
        return 0.0 if name == '0' else x[self.nodes[name]]

    def run(self, u, src, out, tol=1e-8, maxit=8, lim='pnjlim', extrap=True, x0=None, cont=False):
        """u: samples for V source index src. Returns outputs (len(u) x len(out)), iteration counts, non-converged flags."""
        x = self.x_op if x0 is None else x0
        vC = self.PC @ x; iC = np.zeros(len(self.C))
        vL = self.PL @ x; iL = 1e3 * vL
        vL = vL * 0
        v = self.PJ @ x; vp = v.copy()
        oi = [self.nodes[o] for o in out]
        y = np.zeros((len(u), len(oi))); its = np.zeros(len(u), int); bad = np.zeros(len(u), bool)
        b0 = self.rhs_src(); bprev = b0.copy(); self.nfall = 0
        for n in range(len(u)):
            b = b0.copy(); b[self.N + src] = u[n]
            if len(self.C): b += self.PC.T @ (self.gc * vC + iC)
            if len(self.L): b -= self.PL.T @ (iL + self.gl * vL)
            seed = 2 * v - vp if extrap else v
            vp = v
            xs, vs = x, v
            x, v, its[n], ok = self.newton(b, seed, tol, maxit, lim)
            if not ok and cont:
                for K in (4, 16):
                    vv = vs
                    for k in range(1, K + 1):
                        x, vv, i2, ok = self.newton(bprev + (b - bprev) * k / K, vv, tol, maxit, lim)
                        its[n] += i2
                        if not ok: break
                    if ok: v = vv; break
                if not ok: x, v = xs, vs
                self.nfall += 1
            bprev = b
            bad[n] = not ok
            if len(self.C):
                vn = self.PC @ x; iC = self.gc * (vn - vC) - iC; vC = vn
            if len(self.L):
                vn = self.PL @ x; iL = iL + self.gl * (vn + vL); vL = vn
            y[n] = x[oi]
        self.x_last = x
        return y, its, bad
