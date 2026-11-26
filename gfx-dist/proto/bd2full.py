# Discrete OD, whole circuit (Aion FX Sapphire document, stock BD-2 values; the Sapphire's Bass pot left
# out, as stock). The two discrete op-amps either at transistor level (JFET pair + PNP) or as the
# proposed 3-port stage (the pair as one square-law port + the PNP). `fat` = the switched bass cap of
# the "Custom" variant: 18 nF -> 101 nF (33 nF + 68 nF) into the tone pot.
# Usage: python3 bd2full.py ac | level | head [f0 ...] | alias <osf> [tl|pair|pairt|pairl] [level 0..1] [fat|-] [dBFS] | thd ...
import sys
import numpy as np
from nl import Net, harmonics, worst_alias
from bd2 import VA, VB, JF, PNP, a_taper

NPN = dict(pnp=False, Is=2e-14, bf=400.0, br=4.0)
OP4558 = dict(sr=1.7e6, gbw=3e6, a0=1e5)


def discrete(n, tag, gp, gm, out, ccomp, mode):
    d = "d" + tag
    if mode == "tl":
        s = "s" + tag
        n.jfet(d, gp, s, **JF)
        n.jfet("va", gm, s, **JF)
        n.r(s, "0", 4.7e3)
    else:  # pair = fixed tail; pairt = the tail follows the common mode (implicit); pairl = the same, v_cm lagged
        n.pair(d, gp, gm, I0=0.93e-3, beta=JF["beta"], vto=JF["vto"], rt=0.0 if mode == "pair" else 4.7e3,
               lag=mode == "pairl")
    n.r("va", d, 2.2e3)
    n.bjt(out, d, "va", **PNP)
    n.c(d, out, ccomp)
    n.r(out, "0", 2.2e3)


def build(gain=0.5, tone=0.5, level=1.0, fat=False, mode="pair", sig=None):
    n = Net()
    n.v("in", sig if sig else (lambda t: 0.0))
    n.v("va", VA)
    n.v("vb", VB)
    n.r("in", "a", 10e3)
    n.c("a", "g1", 47e-9)
    n.r("g1", "vb", 1e6)
    n.buf("g1", "s1")                       # Q1, the input follower (impedance only)
    n.c("s1", "g2", 100e-9)
    n.r("g2", "vb", 220e3)
    rg = max(250e3 * a_taper(gain), 10.0)   # the dual gang, both halves
    discrete(n, "2", "g2", "g3", "o1", 47e-12, mode)
    n.r("o1", "x1", rg)
    n.r("x1", "g3", 22e3)
    n.c("g3", "o1", 47e-12)
    n.c("g3", "y1", 150e-9)
    n.r("y1", "0", 1.5e3)
    # the fixed network and the clipper
    n.r("o1", "X", 100e3)
    n.c("o1", "c6", 220e-12)
    n.r("c6", "Y", 330e3)
    n.c("X", "Y", 100e-9)
    n.c("X", "Z", 47e-9)
    n.r("Z", "0", 15e3)
    n.r("Y", "Z", 1e6)
    for a, k in (("Y", "dA"), ("dA", "0"), ("0", "dB"), ("dB", "Y")):
        n.diode(a, k)
    n.c("Y", "g5", 2.2e-9)
    n.r("g5", "vb", 1e6)
    discrete(n, "5", "g5", "g6", "o2", 100e-12, mode)
    n.r("o2", "fb2", rg)
    n.r("fb2", "g6", 33e3)
    n.c("g6", "o2", 100e-12)
    n.r("g6", "y2", 2.2e3)
    n.c("y2", "0", 1e-6)
    # trim, tone, volume
    n.r("o2", "t", 5.6e3)
    n.c("o2", "t", 6.8e-9)
    n.c("t", "0", 5.6e-9)
    n.c("t", "p3", 101e-9 if fat else 18e-9)
    rt = max(10e3 * tone, 1.0)
    n.r("p3", "tw", max(10e3 - rt, 1.0))
    n.r("tw", "p1", rt)
    n.c("p1", "0", 18e-9)
    rv = max(100e3 * a_taper(level), 1.0)
    n.r("tw", "vw", max(100e3 - rv, 1.0))
    n.r("vw", "0", rv)
    n.c("vw", "p", 47e-9)
    n.r("p", "vb", 470e3)
    n.opamp("p", "m", "o3", hi=VA - 1.0, lo=1.0, **OP4558)
    n.c("o3", "m", 2.2e-9)
    n.r("o3", "m", 6.8e3)
    n.c("m", "G", 56e-9)
    n.c("G", "B8", 56e-9)
    n.r("B8", "vb", 470e3)
    n.bjt("va", "B8", "E8", **NPN)
    n.r("E8", "0", 10e3)
    n.r("G", "E8", 1.2e3)
    n.c("o3", "b9", 1e-6)
    n.r("b9", "vb", 100e3)
    n.buf("b9", "e9")
    n.c("e9", "q", 1e-6)
    n.r("q", "0", 100e3)
    n.r("q", "out", 1e3)
    n.r("out", "0", 1e6)
    return n


def db(x):
    return 20 * np.log10(np.abs(x) + 1e-30)


def dc(n, **kw):
    """DC point; a common-mode tail cannot be source-stepped from 0 V: seed from the fixed-tail circuit."""
    if kw.get("mode") in ("pairt", "pairl"):
        s = build(**dict(kw, mode="pair"))
        x, _ = s.dc()
        n.build(0)
        n.xdc, _, ok = n.newton(x, n.rhs_src(0), cap=200)
        return n.xdc, ok
    return n.dc()


if __name__ == "__main__":
    what = sys.argv[1] if len(sys.argv) > 1 else "ac"
    f = np.array([40, 60, 80, 100, 120, 150, 200, 300, 500, 707, 1e3, 2e3, 3e3, 5e3, 1e4])
    if what == "ac":
        print("f (Hz):", f.astype(int).tolist())
        for g in (0.0, 0.5, 1.0):
            n = build(gain=g)
            n.dc()
            print(f"in->o1 (stage 1) gain {g:.0%}:", np.round(db(n.ac(f, 0, "o1")), 1).tolist())
        n = build(gain=0.5)
        x, ok = n.dc()
        print("DC:", {k: round(n.node(x, k), 3) for k in ("o1", "Y", "o2", "o3", "d2", "d5", "B8", "E8")})
        # the post-clipper network: from the clip node Y (a source stepping in for the diodes) to the output
        for fat in (False, True):
            for tone in (0.0, 0.5, 1.0):
                m = build(gain=0.5, tone=tone, fat=fat)
                m.dc()
                hy = m.ac(f, 0, "out") / m.ac(f, 0, "Y")
                print(f"Y->out tone {tone:.0%} fat {fat:d}:", np.round(db(hy), 1).tolist())
        m = build()
        m.dc()
        lift = m.ac(f, 0, "o3") / m.ac(f, 0, "p")
        print("IC stage (p->o3), the gyrator's lift:", np.round(db(lift), 1).tolist())
    elif what == "level":
        # the largest stage-2 swing (0..8 V, i.e. 4 V peak) through trim, tone and volume into the IC stage
        ff = np.geomspace(40, 2000, 60)
        for lv in (1.0, 0.9, 0.8, 0.7, 0.6, 0.5):
            m = build(gain=0.5, tone=0.0, level=lv)
            m.dc()
            hmax = np.max(np.abs(m.ac(ff, 0, "o3") / m.ac(ff, 0, "o2")))
            print(f"level pot {lv:.0%}: o2->o3 peak gain {db(hmax):5.1f} dB -> 4 V peak gives {4*hmax:5.2f} V (IC headroom ~3 V)")
    elif what == "head":
        # review 2026-11-26: what the IC stage's LINEAR stamp is asked for, in the time domain. A clipped
        # stage 2 is a square, not a sine: its steady-state waveform (o2, full gain, a 0 dBFS sine in) is
        # carried through the linear response o2 -> o3 (the ac ratio; ideal within the audio band) per
        # level / tone / fat, and the peak read. Usage: bd2full.py head [f0 ...]
        fs = 384000.0
        f0s = [float(a) for a in sys.argv[2:]] or [55.0, 82.4, 110.0, 146.8, 220.0, 440.0, 1205.0]
        for f0 in f0s:
            per = int(round(fs / f0))
            f0 = fs / per
            kw = dict(gain=1.0, tone=0.5, level=0.5, mode="pairt", sig=lambda t: 1.0 * np.sin(2 * np.pi * f0 * t))
            n = build(**kw)
            dc(n, **kw)
            nset = int(np.ceil(0.12 * fs / per)) * per
            out, its, fails = n.tran(fs, nset + per, ["o2"])
            w = out[nset:, 0]
            W = np.fft.rfft(w - w.mean())
            fr = np.arange(len(W)) * f0
            keep = (fr > 0) & (fr <= 20000.0)
            for level in (1.0, 0.9, 0.7, 0.5):
                row = []
                for fat in (False, True):
                    for tone in (0.0, 0.5, 1.0):
                        m = build(gain=1.0, tone=tone, level=level, fat=fat)
                        m.dc()
                        H = np.zeros(len(W), complex)
                        H[keep] = m.ac(fr[keep], 0, "o3") / m.ac(fr[keep], 0, "o2")
                        row.append(np.max(np.abs(np.fft.irfft(W * H, per))))
                print(f"{f0:7.1f} Hz, o2 {w.min():.2f}..{w.max():.2f} V, level {level:.0%}: IC stage asked (V peak) "
                      f"fat off, tone 0/50/100 %: {row[0]:.2f} {row[1]:.2f} {row[2]:.2f}; fat on: {row[3]:.2f} {row[4]:.2f} {row[5]:.2f} "
                      f"(headroom {VA / 2 - 1.0:.2f} V)", flush=True)
    elif what in ("alias", "thd"):
        osf = int(sys.argv[2]) if len(sys.argv) > 2 else 8
        mode = sys.argv[3] if len(sys.argv) > 3 else "pairl"   # the designed form (review 2026-11-26; was the fixed tail)
        level = float(sys.argv[4]) if len(sys.argv) > 4 else 0.9   # the netlist's fixed level pot (was 0.5)
        fat = len(sys.argv) > 5 and sys.argv[5] == "fat"
        f0 = 1205.0
        amp = 10 ** (float(sys.argv[6]) / 20) if len(sys.argv) > 6 else 10 ** (-12 / 20)
        fs = 48000.0 * osf
        kw = dict(gain=1.0, tone=1.0, level=level, fat=fat, mode=mode, sig=lambda t: amp * np.sin(2 * np.pi * f0 * t))
        n = build(**kw)
        dc(n, **kw)
        nset, nwin = int(0.1 * fs), int(0.2 * fs)
        out, its, fails = n.tran(fs, nset + nwin, ["out", "o2", "o3", "o1"])
        y = out[nset:, 0]
        a, fa = worst_alias(y - y.mean(), fs, f0)
        h = harmonics(y - y.mean(), fs, f0, 9)
        print(f"o3 (IC stage) {out[nset:,2].min():.2f}..{out[nset:,2].max():.2f} V, o1 {out[nset:,3].min():.2f}..{out[nset:,3].max():.2f} V")
        print(f"{mode} {osf}x level {level:.0%} fat {fat:d}: worst alias {a:.1f} dB at {fa:.0f} Hz; h2..h5 {np.round(db(h[1:5]/h[0]),1).tolist()} dBc; "
              f"Newton {its[nset:].mean():.2f} mean, {its.max()} worst, {fails} unsolved; o2 {out[nset:,1].min():.2f}..{out[nset:,1].max():.2f} V")
