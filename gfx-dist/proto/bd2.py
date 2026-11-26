# Discrete OD (Aion FX Sapphire document, stock BD-2 values): the second discrete op-amp stage at
# transistor level (2SK184-GR pair on 4.7 k, 2SA1335-class PNP on 2.2 k), against the square-law macro.
# Usage: python3 bd2.py [tl] [pair] [pairt] [pairl] [macro]  (under the memory cap). Prints the DC point, the gain and the
# harmonics of a 1 kHz sine at three levels and two gains, per model (pairt: the tail follows the common mode;
# pairl, added at the review of 2026-11-26: the same with v_cm read from the previous sample, the form that ships).
import sys
import numpy as np
from nl import Net, harmonics

VA, VB = 8.1, 4.05          # 9 V - 1N5817 - the capacitance multiplier's Vbe [A]
JF = dict(beta=18e-3, vto=-0.45, lam=4e-3, Is=1e-14, smooth=0.0)   # 2SK184-GR class [U]
PNP = dict(pnp=True, Is=2e-14, bf=300.0, br=4.0)                    # 2SA1335-GR class [U]


def a_taper(t):
    return (10 ** (2 * t) - 1) / 99


def stage2(rot, amp, f0, transistor=True, macro=None):
    """transistor: True = both JFETs and the PNP; "pair" = the pair as one square-law port on a fixed
    tail (no JFET device) + the PNP; False = the square-law op-amp macro (`macro`: its constants)."""
    n = Net()
    n.v("in", lambda t: VB + amp * np.sin(2 * np.pi * f0 * t))
    n.v("va", VA)
    n.v("vb", VB)
    n.c("in", "g5", 2.2e-9)
    n.r("g5", "vb", 1e6)
    rpot = max(250e3 * a_taper(rot), 10.0)
    if transistor in ("pair", "pairt", "pairl"):
        n.pair("d5", "g5", "g6", I0=0.93e-3, beta=JF["beta"], rt=0.0 if transistor == "pair" else 4.7e3, vto=JF["vto"],
               lag=transistor == "pairl")
        n.r("va", "d5", 2.2e3)
        n.bjt("out", "d5", "va", **PNP)
        n.c("d5", "out", 100e-12)
    elif transistor:
        n.jfet("d5", "g5", "s", **JF)
        n.jfet("va", "g6", "s", **JF)
        n.r("s", "0", 4.7e3)
        n.r("va", "d5", 2.2e3)
        n.bjt("out", "d5", "va", **PNP)
        n.c("d5", "out", 100e-12)
    else:
        n.opamp("g5", "g6", "out", **macro)
    n.r("out", "0", 2.2e3)
    n.c("g6", "out", 100e-12)
    n.r("g6", "fb", 33e3)
    n.r("fb", "out", rpot)
    n.r("g6", "leg", 2.2e3)
    n.c("leg", "0", 1e-6)
    # its load: 5.6 k || 6.8 nF into 5.6 nF and the tone network (approximated by the 5.6 nF)
    n.r("out", "t", 5.6e3)
    n.c("out", "t", 6.8e-9)
    n.c("t", "0", 5.6e-9)
    return n


def run(rot, amp, transistor=True, macro=None, f0=1000.0, osf=8, nh=5):
    n = stage2(rot, amp, f0, transistor, macro)
    if transistor in ("pairt", "pairl"):
        # source stepping cannot start a tail that follows a common mode near 0 V: seed from the fixed tail
        s = stage2(rot, amp, f0, "pair", macro)
        x, ok = s.dc()
        n.build(0)
        n.xdc, _, ok = n.newton(x, n.rhs_src(0), cap=200)
        x = n.xdc
    else:
        x, ok = n.dc()
    fs = 48000.0 * osf
    per = int(round(fs / f0))
    out, its, fails = n.tran(fs, per * 40, ["out"])
    y = out[-per * 10:, 0]
    h = harmonics(y - y.mean(), fs, f0, nh)
    return n.node(x, "out"), y.min(), y.max(), h, its[per * 5:].mean(), its.max(), fails


MACRO = dict(sr=9.3e6, gbw=4.6e6, a0=640.0, hi=8.05, lo=0.05, square=True)

if __name__ == "__main__":
    modes = sys.argv[1:] or ["tl", "pair", "macro"]
    for mode in modes:
        tr = {"tl": True, "pair": "pair", "pairt": "pairt", "pairl": "pairl", "macro": False}[mode]
        for rot in (0.5, 1.0):
            for amp in (0.01, 0.05, 0.2):
                vdc, lo, hi, h, mit, xit, fails = run(rot, amp, tr, MACRO)
                hd = 20 * np.log10(h[1:] / h[0] + 1e-15)
                print(f"{mode:5s} gain {rot:.0%} in {amp*1e3:4.0f} mV: dc {vdc:.3f} V swing {lo:.2f}..{hi:.2f} V  "
                      f"h1 {20*np.log10(h[0]/amp):5.1f} dB  h2..h5 {np.round(hd,1)} dBc  it {mit:.2f}/{xit} fail {fails}", flush=True)
