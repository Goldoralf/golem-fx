# SPDX-License-Identifier: GPL-3.0-or-later
"""p5: the Box stage, a modal object bank (README §2.7).

1. Each object: mode count, range, T60, normalisation; the IR length a convolution would need.
2. Level: pink-weighted power of the bank = 0 dB (bypass) at `box` 100 %, every object, every `size`.
3. Motion: `size` swept 50 -> 200 % in 0.5 s, `position` 0 -> 100 %, object morph Tin Can -> Bucket, on a
   sung vowel, with a per-sample TPT SVF bank (coefficients every sample): finite, no step (the
   largest sample-to-sample change re the static renders), level within a few dB.
"""
import numpy as np
from scipy import signal
from common import FS, voice, pink, rms_db, db
from objects import OBJECTS, bank

FREQ = np.geomspace(20, 20000, 2048)


def bpf(f0, Q, fs=FS):  # RBJ constant-0-dB-peak band-pass = bilinear (pre-warped) of the TPT SVF's band-pass * k
    w = 2 * np.pi * f0 / fs
    al = np.sin(w) / (2 * Q)
    b = np.array([al, 0, -al])
    a = np.array([1 + al, -2 * np.cos(w), 1 - al])
    return b / a[0], a / a[0]


def response(B, fs=FS):
    H = np.zeros(len(FREQ), complex)
    for f, Q, g in zip(B["f"], B["Q"], B["g"]):
        if g == 0:
            continue
        b, a = bpf(f, Q, fs)
        H += g * signal.freqz(b, a, worN=FREQ, fs=fs)[1]
    bh, ah = signal.butter(2, B["hp"], "high", fs=fs)
    bl, al = signal.butter(2, B["lp"], "low", fs=fs)
    sk = signal.freqz(bh, ah, worN=FREQ, fs=fs)[1] * signal.freqz(bl, al, worN=FREQ, fs=fs)[1]
    return H + 10 ** (B["skirt"] / 20) * sk


def pink_gain(H):
    band = (FREQ >= 100) & (FREQ <= 8000)
    w = 1 / FREQ[band]  # pink weight per Hz; FREQ is log-spaced so each point already ~ f df: use 1 per point
    p = np.mean(np.abs(H[band]) ** 2)
    return -10 * np.log10(p)


def ir(B, n, fs=FS):
    x = np.zeros(n)
    x[0] = 1
    y = np.zeros(n)
    for f, Q, g in zip(B["f"], B["Q"], B["g"]):
        if g:
            y += g * signal.lfilter(*bpf(f, Q, fs), x)
    bh, ah = signal.butter(2, B["hp"], "high", fs=fs)
    bl, al = signal.butter(2, B["lp"], "low", fs=fs)
    y += 10 ** (B["skirt"] / 20) * signal.lfilter(bl, al, signal.lfilter(bh, ah, x))
    return y


print("== 1. objects (position 50 %, size 100 %, decay 100 %)")
norms = {}
for name in OBJECTS[1:]:
    B = bank(name)
    H = response(B)
    K = pink_gain(H)
    norms[name] = K
    h = ir(B, int(2.5 * FS)) * 10 ** (K / 20)
    e = np.cumsum(h[::-1] ** 2)[::-1]
    t60 = np.argmax(e / e[0] < 1e-6) / FS
    nz = B["g"] != 0
    t60m = 6.91 * B["Q"][nz] / (np.pi * B["f"][nz])
    Hn = 20 * np.log10(np.abs(H) + 1e-12) + K
    pk = FREQ[np.argmax(Hn)]
    print(f"  {name:13s}: {B['n']:2d} modes {B['f'][nz].min():6.0f} - {B['f'][nz].max():6.0f} Hz | mode T60 "
          f"{t60m.min()*1e3:5.0f} - {t60m.max()*1e3:5.0f} ms | IR to -60 dB {t60*1e3:6.0f} ms = {int(t60*FS):6d} taps | "
          f"norm {K:+5.1f} dB | strongest peak {pk:6.0f} Hz {Hn.max():+5.1f} dB | response p-p (200 Hz - 5 kHz) "
          f"{Hn[(FREQ>200)&(FREQ<5000)].max()-Hn[(FREQ>200)&(FREQ<5000)].min():4.1f} dB")

print("== 2. normalisation across size (pink-weighted power, dB re bypass, after the 100 % normalisation)")
for name in ("Tin Can", "Oil Drum", "Hands"):
    row = []
    for s in (0.25, 0.5, 1.0, 2.0, 4.0):
        B = bank(name, size=s)
        row.append(-pink_gain(response(B)) + norms[name])
    print(f"  {name:10s}: size 25/50/100/200/400 % -> " + " ".join(f"{v:+5.1f}" for v in row)
          + "  (a static per-size table, interpolated, removes it: computed, not measured live)")


# 3. motion: per-sample TPT SVF bank
def tpt_bank(x, F, Qs, G, hp, lp, skirt_db, fs=FS):
    """F, Qs, G: arrays (n_samples, M) or (M,) for static. Returns y."""
    n = len(x)
    M = F.shape[-1]
    ic1 = np.zeros(M)
    ic2 = np.zeros(M)
    y = np.zeros(n)
    static = F.ndim == 1
    for i in range(n):
        f = F if static else F[i]
        q = Qs if static else Qs[i]
        g = G if static else G[i]
        gg = np.tan(np.pi * f / fs)
        k = 1 / q
        a1 = 1 / (1 + gg * (gg + k))
        a2 = gg * a1
        a3 = gg * a2
        v3 = x[i] - ic2
        v1 = a1 * ic1 + a2 * v3
        v2 = ic2 + a2 * ic1 + a3 * v3
        ic1 = 2 * v1 - ic1
        ic2 = 2 * v2 - ic2
        y[i] = np.dot(g, k * v1)
    bh, ah = signal.butter(2, hp, "high", fs=fs)
    bl, al = signal.butter(2, lp, "low", fs=fs)
    return y + 10 ** (skirt_db / 20) * signal.lfilter(bl, al, signal.lfilter(bh, ah, x))


v = voice(187.3, 0.8, "a", -18.0, jitter=0.01)
N = len(v)
ramp = np.clip((np.arange(N) - 0.15 * FS) / (0.5 * FS), 0, 1)

print("== 3. motion on a sung 'a' (per-sample coefficients); max |dy| re the static render's max |dy|; level re bypass")
cases = []
# size sweep 50 -> 200 %
B0 = bank("Tin Can", size=0.5)
B1 = bank("Tin Can", size=2.0)
Fs_ = np.exp(np.log(B0["f"])[None] * (1 - ramp[:, None]) + np.log(B1["f"])[None] * ramp[:, None])
cases.append(("Tin Can size 50 -> 200 %", Fs_, np.broadcast_to(B0["Q"], Fs_.shape), np.broadcast_to(B0["g"], Fs_.shape),
              B0, B1))
# position 0 -> 100 %
P0 = bank("Tin Can", pos=0.0)
P1 = bank("Tin Can", pos=1.0)
Gp = P0["g"][None] * (1 - ramp[:, None]) + P1["g"][None] * ramp[:, None]
cases.append(("Tin Can position 0 -> 100 %", np.broadcast_to(P0["f"], Gp.shape), np.broadcast_to(P0["Q"], Gp.shape), Gp, P0, P1))
# morph Tin Can -> Bucket (rank-paired slots, log f and log Q interpolated, gains linear)
M0 = bank("Tin Can")
M1 = bank("Bucket")
Fm = np.exp(np.log(M0["f"])[None] * (1 - ramp[:, None]) + np.log(M1["f"])[None] * ramp[:, None])
Qm = np.exp(np.log(M0["Q"])[None] * (1 - ramp[:, None]) + np.log(M1["Q"])[None] * ramp[:, None])
Gm = (M0["g"] * 10 ** (norms["Tin Can"] / 20))[None] * (1 - ramp[:, None]) + (M1["g"] * 10 ** (norms["Bucket"] / 20))[None] * ramp[:, None]
cases.append(("morph Tin Can -> Bucket", Fm, Qm, Gm, M0, M1))
for lab, F, Q, G, A, Bb in cases:
    y = tpt_bank(v, np.array(F), np.array(Q), np.array(G), A["hp"], A["lp"], A["skirt"])
    ya = tpt_bank(v, A["f"], A["Q"], G[0], A["hp"], A["lp"], A["skirt"])
    yb = tpt_bank(v, Bb["f"], Bb["Q"], G[-1], Bb["hp"], Bb["lp"], Bb["skirt"])
    st = max(np.max(np.abs(np.diff(ya))), np.max(np.abs(np.diff(yb))))
    mv = np.max(np.abs(np.diff(y)))
    seg = slice(int(0.15 * FS), int(0.65 * FS))
    print(f"  {lab:28s}: finite {np.all(np.isfinite(y))}, max|dy| {mv/st:4.2f} x static; level during the move "
          f"{rms_db(y[seg]) - rms_db(v[seg]):+5.1f} dB (static ends {rms_db(ya[seg]) - rms_db(v[seg]):+5.1f} / "
          f"{rms_db(yb[seg]) - rms_db(v[seg]):+5.1f} dB, unnormalised)")

# 4. what a convolution engine would cost for the same objects (gfx-cab measured, README §2.7)
print("== 4. convolution instead: IR lengths above; gfx-cab measures 75.6 ns / sample mono for a 200 ms IR,")
print("   0.80 % of a core for 2 s at 48 kHz; a kernel change crossfades over 30 ms (convolve)")

# 5. pink normalisation read on a voice: the level a singer hears change when `box` goes 0 -> 100 %
print("== 5. level of the pink-normalised object on the sung phrase ('a' 187 Hz + 'o' 150 Hz), re bypass")
from common import voice as _v
vv = np.concatenate([_v(187.3, 0.6, "a", -18.0), _v(150.0, 0.6, "o", -18.0, seed=9)])
for name in OBJECTS[1:]:
    B = bank(name)
    h = ir(B, int(1.0 * FS)) * 10 ** (norms[name] / 20)
    y = signal.fftconvolve(vv, h)[:len(vv)]
    print(f"  {name:13s}: {rms_db(y[2400:]) - rms_db(vv[2400:]):+5.1f} dB  -> the voice-referenced table adds the opposite")
