import numpy as np
from scipy.signal import kaiserord, firwin
def kern(N, p, s, rows=128, A=None):
    """continuous kernel h(t), t in [-N/2, N/2), sampled at rows per level-sample. pass p, stop s (cycles/level sample)"""
    d = s - p; fc = (p + s) / 2
    if A is None: A = 2.285 * (N - 1) * 2 * np.pi * d + 8
    beta = 0.1102 * (A - 8.7) if A > 50 else 0.5842 * (A - 21) ** 0.4 + 0.07886 * (A - 21)
    t = (np.arange(N * rows + 1) - N * rows / 2) / rows
    h = 2 * fc * np.sinc(2 * fc * t) * np.i0(beta * np.sqrt(np.clip(1 - (2 * t / N) ** 2, 0, 1))) / np.i0(beta)
    return t, h, A
def response(N, p, s, rows=128, lerp=True, over=8, norm=True):
    t, h, A = kern(N, p, s, rows)
    if norm:  # normalise each phase row to DC gain 1
        hh = h[:-1].reshape(N, rows)
        g = hh.sum(0); hh = hh / g; h = np.append(hh.reshape(-1), h[-1] / g[0])
    # piecewise-linear continuous kernel, sampled 'over' times finer
    if lerp:
        tf = np.linspace(t[0], t[-1], (len(t) - 1) * over + 1)
        hf = np.interp(tf, t, h)
    else:
        tf, hf = t, h; over = 1
    M = 1 << 22
    H = np.abs(np.fft.rfft(hf, M)) / (rows * over)
    f = np.arange(len(H)) * rows * over / M  # cycles per level sample
    return f, 20 * np.log10(H + 1e-300), A
def report(N, p, s, rows=128, label=''):
    f, H, A = response(N, p, s, rows)
    pas = H[f <= p]; stop = H[(f >= s) & (f < 64)]
    f2, H2, _ = response(N, p, s, 4096, lerp=False)
    stop2 = H2[(f2 >= s) & (f2 < 64)]
    far = H[(f >= rows - 1) & (f < rows + 1)].max() if rows < 300 else 0
    print(f"{label:28s} N={N} p={p:.4f} s={s:.4f} Kaiser A={A:5.1f} | pass ripple {pas.min():+.3f}/{pas.max():+.3f} dB | stop(lerp {rows} rows) {stop.max():6.1f} | stop(ideal) {stop2.max():6.1f} | image at rows±1: {far:6.1f}")
print("== designer's scheme: sqrt2 switch, 32 taps")
report(32, .40, .60, label='rho<=1')
for j in (1, 4, 8):
    rp = 2 ** (j / 16); report(32, .40 / rp, .60 / rp, label=f'rho+=2^({j}/16)={rp:.3f}')
print("== octave switch")
for N in (32, 40, 44, 48, 56, 64):
    report(N, .20, .30, label='rho+=2')
for j in (8, 9, 12):
    rp = 2 ** (j / 16)
    for N in (32, 48): report(N, .40 / rp, .60 / rp, label=f'rho+=2^({j}/16)={rp:.3f}')
print("== phase rows (32 taps, rho<=1 and 48 taps rho=2)")
for rows in (32, 64, 128, 256):
    report(32, .40, .60, rows, label=f'{rows} rows'); report(48, .2, .3, rows, label=f'{rows} rows')
