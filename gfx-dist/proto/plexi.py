# Plexi Dist (PedalPCB "Ultisol Distortion" document, revised 8/19/20: a Red Rox-type Marshall-in-a-box,
# LM833 on 9 V). Usage: python3 plexi.py ac | alias <osf> [contour 0..1] [supply, 1 = 9 V] [gain 0..1]
import sys
import numpy as np
from nl import Net, harmonics, worst_alias

VA = 9.0 - 0.30
VB = VA / 2
LM833 = dict(sr=7e6, gbw=15e6, a0=1e5, hi=VA - 1.5, lo=1.5)
GE = dict(Is=2e-7, N=1.3)        # generic germanium signal diode [U]
SI4001 = dict(Is=1.4e-8, N=1.98) # 1N4001-class [U]
SI4148 = dict(Is=2.52e-9, N=1.752)


def a_taper(t):
    return (10 ** (2 * t) - 1) / 99


def c_taper(t):
    return 1 - a_taper(1 - t)


def build(gain=0.5, tone=0.5, contour=0.5, sig=None, diodes=True, supply=1.0):
    va = VA * supply
    vb = va / 2
    op = dict(LM833, hi=va - 1.5)
    n = Net()
    n.v("in", sig if sig else (lambda t: 0.0))
    n.v("vb", vb)
    n.c("in", "0", 100e-12)
    n.r("in", "0", 2.2e6)
    n.c("in", "p1", 10e-9)
    n.r("p1", "vb", 1e6)
    n.opamp("p1", "m1", "o1", **op)
    n.r("m1", "l1", 3.3e3)
    n.c("l1", "0", 47e-9)
    n.c("m1", "o1", 100e-12)
    n.r("m1", "N", 4.7e3)
    # the gain pot (100 k reverse log): wiper on IC1.1's output, pin 1 to the feedback, pin 3 to the
    # next stage's input resistor: one rotation raises the first stage's gain and the second's together
    r1w = 100e3 * c_taper(gain)
    n.r("N", "o1", max(r1w, 1.0))
    n.r("o1", "G", max(100e3 - r1w, 1.0))
    n.c("N", "G", 1e-9)
    n.c("G", "g5", 68e-9)
    n.r("g5", "m2", 4.7e3)
    n.opamp("vb", "m2", "o2", **op)
    n.r("o2", "m2", 680e3)
    n.c("o2", "m2", 68e-12)
    n.c("o2", "c8", 100e-9)
    n.r("c8", "D", 4.7e3)
    if diodes:  # asymmetric: Ge + 1N4001 one way, 1N4001 + 1N4148 the other, to ground
        n.diode("D", "dA", **GE)
        n.diode("dA", "0", **SI4001)
        n.diode("dB", "D", **SI4001)
        n.diode("0", "dB", **SI4148)
    n.r("D", "T", 3e3)
    n.c("T", "tt", 47e-9)
    n.r("tt", "0", max(100e3 * a_taper(tone), 1.0))
    n.c("T", "c10", 220e-9)
    n.r("c10", "m3", 47e3)
    n.opamp("vb", "m3", "o3", **op)
    n.r("o3", "m3", 220e3)
    n.c("o3", "m3", 470e-12)
    # the bridged-T contour: 10 k + 10 k, 6n8 across, 4n7 and (33 n + 50 k log rheostat) to ground
    n.r("o3", "K", 10e3)
    n.c("K", "0", 4.7e-9)
    n.c("K", "kc", 33e-9)
    n.r("kc", "0", max(50e3 * a_taper(contour), 1.0))
    n.r("K", "L", 10e3)
    n.c("o3", "L", 6.8e-9)
    n.c("L", "0", 4.7e-9)
    n.c("L", "c16", 470e-9)
    n.r("c16", "m4", 100e3)
    n.opamp("vb", "m4", "o4", **op)
    n.r("o4", "m4", 100e3)
    n.c("o4", "m4", 220e-12)
    n.r("o4", "r15", 1e3)
    n.c("r15", "out", 4.7e-6)
    n.r("out", "0", 10e3 * 1e6 / (10e3 + 1e6))   # master 10 k fully up into a 1 M load
    return n


def db(x):
    return 20 * np.log10(np.abs(x) + 1e-30)


if __name__ == "__main__":
    what = sys.argv[1] if len(sys.argv) > 1 else "ac"
    ff = np.geomspace(30, 15000, 400)
    f = np.array([50, 100, 200, 300, 500, 700, 1e3, 1.5e3, 2e3, 3e3, 5e3, 8e3, 1.2e4])
    if what == "ac":
        print("f (Hz):", f.astype(int).tolist())
        for g in (0.0, 0.5, 1.0):
            n = build(gain=g, diodes=False)
            n.dc()
            h = n.ac(f, 0, "o2")
            print(f"in->o2 (before the clipper) gain {g:.0%}:", np.round(db(h), 1).tolist())
        for t in (0.0, 0.5, 1.0):
            m = build(tone=t, diodes=False)
            m.dc()
            print(f"D->o3 tone {t:.0%}:", np.round(db(m.ac(f, 0, "o3") / m.ac(f, 0, "D")), 1).tolist())
        for c in (0.0, 0.25, 0.5, 0.75, 1.0):
            m = build(contour=c, diodes=False)
            m.dc()
            h = np.abs(m.ac(ff, 0, "out") / m.ac(ff, 0, "o3"))
            k = np.argmin(h)
            print(f"o3->out contour {c:.0%}: min {db(h[k]):.1f} dB at {ff[k]:.0f} Hz; 100 Hz {db(np.interp(100, ff, h)):.1f}, "
                  f"5 kHz {db(np.interp(5000, ff, h)):.1f}")
        # can IC2.2 rail-clip? the clip node swings ~ +0.9 / -1.2 V
        for t in (0.0, 0.5, 1.0):
            m = build(tone=t, diodes=False)
            m.dc()
            h = np.abs(m.ac(ff, 0, "o3") / m.ac(ff, 0, "D"))
            k = np.argmax(h)
            print(f"tone {t:.0%}: D->o3 peak {db(h[k]):.1f} dB at {ff[k]:.0f} Hz -> 1.2 V gives {1.2*h[k]:.2f} V (headroom {VA/2-1.5:.2f} V at 9 V, {VA-1.5:.2f} V at 18 V)")
    elif what == "alias":
        osf = int(sys.argv[2]) if len(sys.argv) > 2 else 8
        f0 = 1205.0
        amp = 10 ** (-12 / 20)
        fs = 48000.0 * osf
        contour = float(sys.argv[3]) if len(sys.argv) > 3 else 0.5   # review 2026-11-26: the alias moves with it
        supply = float(sys.argv[4]) if len(sys.argv) > 4 else 1.0
        gain = float(sys.argv[5]) if len(sys.argv) > 5 else 1.0
        n = build(gain=gain, tone=1.0, contour=contour, supply=supply, sig=lambda t: amp * np.sin(2 * np.pi * f0 * t))
        n.dc()
        nset, nwin = int(0.1 * fs), int(0.2 * fs)
        out, its, fails = n.tran(fs, nset + nwin, ["out", "o2", "o3"])
        y = out[nset:, 0]
        a, fa = worst_alias(y - y.mean(), fs, f0)
        h = harmonics(y - y.mean(), fs, f0, 5)
        print(f"gain {gain:.0%} contour {contour:.0%} supply {supply:.0%} rms {20*np.log10(np.std(y)):.1f} dBV h1 {20*np.log10(h[0]):.1f} dBV")
        print(f"{osf}x: worst alias {a:.1f} dB at {fa:.0f} Hz; h2..h5 {np.round(db(h[1:]/h[0]),1).tolist()} dBc; "
              f"Newton {its[nset:].mean():.2f} mean, {its.max()} worst, {fails} unsolved; "
              f"o2 {out[nset:,1].min():.2f}..{out[nset:,1].max():.2f} V, o3 {out[nset:,2].min():.2f}..{out[nset:,2].max():.2f} V")
