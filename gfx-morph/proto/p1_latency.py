# SPDX-License-Identifier: GPL-3.0-or-later
"""p1: latency and group delay of the zero-latency chain (README §2.9).

- an impulse through the linear skeleton (pre filters, the 3-band LR4 tree, the 2x and 4x half-band pairs;
  the box at 0 % is bypassed and the other stages are IIR too): the first output sample is sample 0;
- group delay of each part at 100 Hz / 250 Hz / 1 kHz / 3 kHz;
- what the linear-phase alternatives would have to report.
"""
import numpy as np
from scipy import signal
from common import FS, up, down, lr4, split3, bw2, rbj_peak, group_delay_ms, HB

print("== half-band design (8 coefs, transition 0.04):", np.round(HB, 6))

# 1. impulse through up/down pairs
for factor in (2, 4):
    x = np.zeros(4096)
    x[0] = 1.0
    y = down(up(x, factor), factor)
    first = np.argmax(np.abs(y) > 1e-9)
    w, h = signal.freqz(y, worN=[100, 250, 1000, 3000, 10000, 16000, 20000], fs=FS)
    ph = np.unwrap(np.angle(signal.freqz(y, worN=np.linspace(10, 20000, 4000), fs=FS)[1]))
    f = np.linspace(10, 20000, 4000)
    gd = -np.gradient(ph, 2 * np.pi * f / FS)
    at = lambda fr: gd[np.argmin(np.abs(f - fr))]
    print(f"== {factor}x up+down: first nonzero output sample {first}; magnitude dB "
          + " ".join(f"{fr}:{20*np.log10(abs(v)):+.3f}" for fr, v in zip(w, h)))
    print(f"   group delay samples @100 {at(100):.2f} @1k {at(1000):.2f} @3k {at(3000):.2f} @10k {at(10000):.2f} @16k {at(16000):.2f}"
          f"  (= {at(1000)/FS*1e3:.3f} ms at 1 kHz)")

# 2. crossovers
for fc in (150, 250, 400):
    x = np.zeros(1 << 15)
    x[0] = 1
    lo, hi = lr4(x, fc)
    ap = lo + hi
    f = np.array([60, 100, 200, 250, 500, 1000, 3000])
    w, H = signal.freqz(ap, worN=f, fs=FS)
    ph = np.unwrap(np.angle(signal.freqz(ap, worN=np.linspace(5, 6000, 6000), fs=FS)[1]))
    ff = np.linspace(5, 6000, 6000)
    gd = -np.gradient(ph, 2 * np.pi * ff / FS) / FS * 1e3
    at = lambda fr: gd[np.argmin(np.abs(ff - fr))]
    print(f"== LR4 all-pass sum at {fc} Hz: |H| max dev {np.max(np.abs(20*np.log10(np.abs(H)))):.2e} dB; "
          f"group delay ms @60 {at(60):.2f} @100 {at(100):.2f} @{fc} {at(fc):.2f} @1k {at(1000):.3f} @3k {at(3000):.3f}"
          f"; analytic DC 2*sqrt2/(2 pi fc) = {2*np.sqrt(2)/(2*np.pi*fc)*1e3:.2f} ms")
for fc in (2000, 3000, 5000):
    x = np.zeros(1 << 14); x[0] = 1
    lo, hi = lr4(x, fc)
    ph = np.unwrap(np.angle(signal.freqz(lo + hi, worN=np.linspace(5, 20000, 8000), fs=FS)[1]))
    ff = np.linspace(5, 20000, 8000)
    gd = -np.gradient(ph, 2 * np.pi * ff / FS) / FS * 1e3
    at = lambda fr: gd[np.argmin(np.abs(ff - fr))]
    print(f"== LR4 all-pass at {fc} Hz: group delay ms @200 {at(200):.3f} @1k {at(1000):.3f} @{fc} {at(fc):.3f}")

# 3. whole default chain impulse (Single band, 2x): pre HP 80 + LP 12k + honk 0 dB, post nothing
x = np.zeros(8192); x[0] = 1
y = signal.lfilter(*bw2(80, "high"), x)
y = signal.lfilter(*bw2(12000, "low"), y)
y = down(up(y, 2), 2)
first = np.argmax(np.abs(y) > 1e-9)
ff = np.linspace(20, 20000, 8000)
ph = np.unwrap(np.angle(signal.freqz(y, worN=ff, fs=FS)[1]))
gd = -np.gradient(ph, 2 * np.pi * ff / FS) / FS * 1e3
at = lambda fr: gd[np.argmin(np.abs(ff - fr))]
print(f"== default chain (pre HP 80, LP 12k, 2x pair), linear skeleton: first output sample {first}; "
      f"group delay ms @150 {at(150):.2f} @300 {at(300):.2f} @1k {at(1000):.3f} @3k {at(3000):.3f} @8k {at(8000):.3f}")
# three-band default 250 / 3000
x = np.zeros(1 << 15); x[0] = 1
lo, mid, hi = split3(x, 250, 3000)
s = lo + mid + hi
s = signal.lfilter(*bw2(80, "high"), s)
s = down(up(s, 2), 2)
ph = np.unwrap(np.angle(signal.freqz(s, worN=ff, fs=FS)[1]))
gd = -np.gradient(ph, 2 * np.pi * ff / FS) / FS * 1e3
print(f"== Three bands 250 / 3000 + HP 80 + 2x: first output sample {np.argmax(np.abs(s) > 1e-9)}; group delay ms "
      f"@150 {at(150):.2f} @300 {at(300):.2f} @1k {at(1000):.3f} @3k {at(3000):.3f} @8k {at(8000):.3f}")

# 4. the comb a singer hears (bone conduction + in-ear): first notch of direct + delayed copy
for tau_ms in (0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0):
    print(f"   comb: wet delayed {tau_ms:4.2f} ms against bone conduction -> first notch {1/(2*tau_ms*1e-3):7.0f} Hz")

# 5. linear-phase alternatives (what they would have to report)
# (a) linear-phase LR4-like FIR crossover at 250 Hz: an FIR matching LR4 selectivity needs its impulse
#     response (LR4 low-pass at 250 Hz) down to -60 dB; its length / 2 is the latency.
x = np.zeros(1 << 16); x[0] = 1
lo, _ = lr4(x, 250)
e = np.cumsum(lo[::-1] ** 2)[::-1]
n60 = np.argmax(e / e[0] < 1e-6)
print(f"== linear-phase crossover at 250 Hz: LR4 impulse energy to -60 dB within {n60} samples "
      f"-> symmetric FIR ~{2*n60} taps, latency ~{n60} samples = {n60/FS*1e3:.1f} ms")
# (b) linear-phase half-band FIR with the IIR's rejection (>100 dB, transition 0.04 fs at the inner rate)
N_fir = int(np.ceil((100 - 8) / (2.285 * 2 * np.pi * 0.04 * 2))) + 1  # Kaiser estimate, transition in inner fs
for factor in (2, 4):
    lat = (N_fir - 1) / 2  # samples at the inner rate per filter, up + down
    lat_base = 2 * lat / 2 if factor == 2 else (2 * lat / 2 + 2 * lat / 4)
    print(f"== linear-phase half-band FIR (Kaiser estimate, 100 dB, 0.04): ~{N_fir} taps per stage; "
          f"{factor}x up+down latency ~{lat_base:.0f} base samples = {lat_base/FS*1e3:.2f} ms")
