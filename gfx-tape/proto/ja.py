import math, numpy as np
Ms, a, alpha, k, c = 3.5e5, 2.2e4, 1.6e-3, 2.7e4, 0.17
class P:
    def __init__(s, Ms=Ms, a=a, alpha=alpha, k=k, c=c): s.Ms, s.a, s.alpha, s.k, s.c = Ms, a, alpha, k, c
def lang(Q):
    if abs(Q) < 1e-3: return Q/3, 1/3
    ct = 1/math.tanh(Q)
    return ct - 1/Q, 1/(Q*Q) - ct*ct + 1
def g(p, H, M, d):
    Q = (H + p.alpha*M)/p.a
    L, Lp = lang(Q)
    Md = p.Ms*L - M
    dM = 1.0 if (d > 0) == (Md > 0) else 0.0
    irr = (1-p.c)*dM*Md/((1-p.c)*d*p.k - p.alpha*Md)
    rev = p.c*p.Ms/p.a*Lp
    return (irr + rev)/(1 - p.alpha*rev)
def Man(p, H, M0=0.0):
    M = M0
    for _ in range(8):
        M = p.Ms*lang((H+p.alpha*M)/p.a)[0]
    return M
def step_H(p, H0, H1, M, n):
    dH = (H1-H0)/n
    if dH == 0: return M
    d = 1.0 if dH > 0 else -1.0
    H = H0
    for _ in range(n):
        k1 = g(p, H, M, d); k2 = g(p, H+dH/2, M+dH*k1/2, d); k3 = g(p, H+dH/2, M+dH*k2/2, d); k4 = g(p, H+dH, M+dH*k3, d)
        M += dH*(k1+2*k2+2*k3+k4)/6; H += dH
    return M
def run_H(p, Hs, nsub=1, hmax=None, cap=2, tau=None, fs=None, M0=0.0):
    out = np.empty(len(Hs)); M = M0; Hp = 0.0; nsteps = 0
    r = (1-math.exp(-1/(tau*fs))) if tau else 0.0
    for i, H in enumerate(Hs):
        n = nsub if hmax is None else max(1, min(cap, math.ceil(abs(H-Hp)/hmax)))
        M = step_H(p, Hp, H, M, n); nsteps += n
        if r: M += r*(Man(p, H, M) - M)
        if not math.isfinite(M): M = float('nan'); out[i:] = M; break
        out[i] = M; Hp = H
    return out, nsteps/len(Hs)
def run_T(p, Hs, fs, al=0.75):
    # ChowTape time-domain RK4 with alpha-transform derivative
    T = 1/fs; out = np.empty(len(Hs)); M = 0.0; Hp = 0.0; Hdp = 0.0
    def f(M, H, Hd):
        d = 1.0 if Hd > 0 else -1.0
        return Hd*g(p, H, M, d)
    for i, H in enumerate(Hs):
        Hd = (1+al)/T*(H-Hp) - al*Hdp
        k1 = T*f(M, Hp, Hdp); k2 = T*f(M+k1/2, (H+Hp)/2, (Hd+Hdp)/2); k3 = T*f(M+k2/2, (H+Hp)/2, (Hd+Hdp)/2); k4 = T*f(M+k3, H, Hd)
        M += (k1+2*k2+2*k3+k4)/6
        if not math.isfinite(M) or abs(M) > 1e3*p.Ms: out[i:] = float('nan'); break
        out[i] = M; Hp = H; Hdp = Hd
    return out
