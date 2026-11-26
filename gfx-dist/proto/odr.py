# Studio OD (Aion FX Andromeda document = stock ODR-1 with its Bass pot fully up and the D6 / D7 jumper
# bridged; JRC4558 macros). Usage: python3 odr.py ac | alias <osf> [spectrum 0..1]
import sys
import numpy as np
from nl import Net, harmonics, worst_alias

VA = 9.0 - 0.30 - 0.05      # 1N5817, 47 ohm at ~5 mA [A]
VB = VA / 2                 # 15 k / 15 k, 47 uF (unbuffered; an AC ground at audio)
OP4558 = dict(sr=1.7e6, gbw=3e6, a0=1e5, hi=VA - 1.0, lo=1.0)


def a_taper(t):
    return (10 ** (2 * t) - 1) / 99


def build(gain=0.5, spectrum=0.5, sig=None, diodes=True):
    n = Net()
    n.v("in", sig if sig else (lambda t: 0.0))
    n.v("va", VA)
    n.v("vb", VB)
    n.r("in", "gq", 33e3)
    n.r("gq", "0", 1e6)
    n.buf("gq", "sq")                       # Q1, the 2N5457 follower (impedance only)
    n.c("sq", "a1", 68e-9)
    n.r("a1", "p1", 2.7e3)
    n.c("p1", "0", 22e-9)
    n.r("p1", "vb", 10e3)
    n.opamp("p1", "m1", "o1", **OP4558)
    n.r("m1", "b5", 1.5e3)                  # the Andromeda's Bass pot fully up = 0 ohm: stock
    n.c("b5", "0", 2.2e-6)
    n.r("m1", "c4", 820.0)
    n.c("c4", "0", 82e-9)
    n.c("m1", "o1", 120e-12)
    if diodes:
        n.diode("m1", "o1")
        n.diode("o1", "m1")
    n.r("m1", "g6", 1.8e3)
    n.r("g6", "o1", max(250e3 * a_taper(gain), 10.0))
    # the hard clipper to the bias
    n.c("o1", "c6", 2.2e-6)
    n.r("c6", "D", 12e3)
    n.c("D", "0", 2.7e-9)
    if diodes:
        n.diode("D", "vb")
        n.diode("vb", "D")
    n.r("D", "E", 39e3)
    n.c("E", "F", 82e-9)
    n.c("F", "0", 1e-9)
    n.r("F", "0", 12e3)
    n.r("E", "M", 10e3)
    n.r("M", "vb", 43e3)
    # Spectrum: 25 k linear between the + side (22 n || 5k1 from M) and the - side (1k2 to IC2A's -),
    # its wiper into a 27 n shunt and the gyrator branch (100 n, 8n2, 150 k, 2k2, 3k3, the NPN)
    n.c("M", "P", 22e-9)
    n.r("M", "P", 5.1e3)
    rs = 25e3 * spectrum
    n.r("P", "Y", max(rs, 1.0))
    n.r("Y", "p3", max(25e3 - rs, 1.0))
    n.r("p3", "m2", 1.2e3)
    n.c("Y", "0", 27e-9)
    n.c("Y", "X", 100e-9)
    n.c("X", "B2", 8.2e-9)
    n.r("B2", "vb", 150e3)
    n.bjt("va", "B2", "E2", Is=2e-14, bf=400.0, br=4.0)
    n.r("E2", "0", 3.3e3)
    n.r("X", "E2", 2.2e3)
    n.opamp("M", "m2", "o2", **OP4558)
    n.r("m2", "vb", 10e3)
    n.r("o2", "m2", 20e3)
    n.c("o2", "m2", 560e-12)
    n.r("o2", "n5", 4.7e3)
    n.c("n5", "0", 8.2e-9)
    n.opamp("n5", "m3", "o3", **OP4558)
    n.r("m3", "l3", 1.2e3)
    n.c("l3", "0", 1e-6)
    n.r("o3", "m3", 22e3)
    n.c("o3", "m3", 4.7e-9)
    n.c("o3", "k3", 82e-9)
    n.r("k3", "m3", 5.1e3)
    # level 50 k log fully up, the follower, 1 uF into 150 k
    n.c("o3", "lv", 2.2e-6)
    n.r("lv", "vb", 50e3)
    n.c("lv", "p4", 470e-9)
    n.r("p4", "vb", 150e3)
    n.buf("p4", "o4")
    n.c("o4", "out", 1e-6)
    n.r("out", "0", 150e3)
    return n


def db(x):
    return 20 * np.log10(np.abs(x) + 1e-30)


if __name__ == "__main__":
    what = sys.argv[1] if len(sys.argv) > 1 else "ac"
    if what == "ac":
        f = np.array([50, 80, 100, 150, 200, 300, 500, 700, 1e3, 1.5e3, 2e3, 3e3, 5e3, 8e3, 1.2e4])
        ff = np.geomspace(30, 15000, 400)
        print("f (Hz):", f.astype(int).tolist())
        for g in (0.0, 0.5, 1.0):
            n = build(gain=g, diodes=False)
            n.dc()
            print(f"in->o1 gain {g:.0%}:", np.round(db(n.ac(f, 0, "o1")), 1).tolist())
        n = build(gain=0.5)
        x, ok = n.dc()
        print("DC:", ok, {k: round(n.node(x, k), 3) for k in ("sq", "o1", "D", "M", "o2", "o3", "B2", "E2")})
        for sp in (0.0, 0.25, 0.5, 0.75, 1.0):
            m = build(gain=0.5, spectrum=sp, diodes=False)
            m.dc()
            h = m.ac(f, 0, "out") / m.ac(f, 0, "D")
            hh = np.abs(m.ac(ff, 0, "out") / m.ac(ff, 0, "D"))
            hm = np.abs(m.ac(ff, 0, "o2") / m.ac(ff, 0, "M"))
            k = np.argmax(hm)
            print(f"D->out spectrum {sp:.0%}:", np.round(db(h), 1).tolist(),
                  f"| IC2A stage alone peaks {db(hm[k]):.1f} dB at {ff[k]:.0f} Hz")
        # can IC2B clip? the clip node swings ~0.6 V peak around the bias
        for sp in (0.0, 0.5, 1.0):
            m = build(gain=0.5, spectrum=sp, diodes=False)
            m.dc()
            h3 = np.abs(m.ac(ff, 0, "o3") / m.ac(ff, 0, "D"))
            k = np.argmax(h3)
            print(f"D->o3 spectrum {sp:.0%}: peak {db(h3[k]):.1f} dB at {ff[k]:.0f} Hz -> 0.6 V peak at D gives {0.6*h3[k]:.2f} V (headroom ~{VA/2-1:.2f} V)")
    elif what == "alias":
        osf = int(sys.argv[2]) if len(sys.argv) > 2 else 8
        f0 = 1205.0
        amp = 10 ** (-12 / 20)
        fs = 48000.0 * osf
        spectrum = float(sys.argv[3]) if len(sys.argv) > 3 else 1.0
        n = build(gain=1.0, spectrum=spectrum, sig=lambda t: amp * np.sin(2 * np.pi * f0 * t))
        n.dc()
        nset, nwin = int(0.1 * fs), int(0.2 * fs)
        out, its, fails = n.tran(fs, nset + nwin, ["out", "o3"])
        y = out[nset:, 0]
        a, fa = worst_alias(y - y.mean(), fs, f0)
        h = harmonics(y - y.mean(), fs, f0, 5)
        print(f"{osf}x spectrum {spectrum:.0%}: worst alias {a:.1f} dB at {fa:.0f} Hz; h2..h5 {np.round(db(h[1:]/h[0]),1).tolist()} dBc; "
              f"Newton {its[nset:].mean():.2f} mean, {its.max()} worst, {fails} unsolved; o3 {out[nset:,1].min():.2f}..{out[nset:,1].max():.2f} V")
    elif what == "more":
        ff = np.geomspace(30, 15000, 400)
        for sp in (0.0, 0.5, 1.0):
            m = build(gain=0.5, spectrum=sp, diodes=False)
            m.dc()
            h2 = np.abs(m.ac(ff, 0, "o2") / m.ac(ff, 0, "D"))
            k = np.argmax(h2)
            tot = np.abs(m.ac(ff, 0, "out"))
            j = np.argmax(tot)
            pre = np.abs(m.ac(ff, 0, "o1"))
            i = np.argmax(pre)
            print(f"spectrum {sp:.0%}: D->o2 peak {db(h2[k]):.1f} dB at {ff[k]:.0f} Hz (0.6 V -> {0.6*h2[k]:.2f} V); "
                  f"in->out peak {db(tot[j]):.1f} dB at {ff[j]:.0f} Hz, at 100 Hz {db(np.interp(100, ff, tot)):.1f}, 1 kHz {db(np.interp(1000, ff, tot)):.1f}, 3 kHz {db(np.interp(3000, ff, tot)):.1f}; "
                  f"pre-clip (in->o1) peak at {ff[i]:.0f} Hz, {db(pre[i]) - db(np.interp(100, ff, pre)):.1f} dB over 100 Hz")
