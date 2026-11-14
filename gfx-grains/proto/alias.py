import numpy as np, sys
sys.path.insert(0, sys.argv[0].rsplit('/',1)[0])
from kernel import kern
fs = 48000.
def halfband():
    N = 51; A = 80; beta = 0.1102 * (A - 8.7)
    n = np.arange(N) - 25
    h = 0.5 * np.sinc(0.5 * n) * np.kaiser(N, beta)
    return h / h.sum()
HB = halfband()
def level1(x):
    y = np.convolve(x, HB)[25:25 + len(x)]   # delay compensated
    return y[::2]
ROWS = 128
_bank = {}
def table(N, p, s):
    key = (N, round(p, 6), round(s, 6))
    if key not in _bank:
        t, h, A = kern(N, p, s, ROWS)
        hh = h[:-1].reshape(N, ROWS); g = hh.sum(0)
        tab = np.vstack([(hh / g).T, np.roll(hh[:, 0] / g[0], -1)[None, :]])  # rows+1 phases x N taps
        tab[-1, -1] = 0 if True else 0
        # row k: taps for fractional offset k/ROWS ; build directly for clarity
        tab = np.zeros((ROWS + 1, N))
        for k in range(ROWS + 1):
            tt = (np.arange(N) - N / 2 + 1) - k / ROWS      # tap m at integer offset (m - N/2 + 1), frac k/ROWS
            fc = (p + s) / 2
            beta = 0.1102 * (A - 8.7)
            w = np.i0(beta * np.sqrt(np.clip(1 - (2 * tt / N) ** 2, 0, 1))) / np.i0(beta)
            row = 2 * fc * np.sinc(2 * fc * tt) * w
            tab[k] = row / row.sum()
        _bank[key] = tab
    return _bank[key]
def read(buf, pos, N, p, s):
    tab = table(N, p, s)
    i = np.floor(pos).astype(int); fr = pos - i
    k = fr * ROWS; k0 = np.floor(k).astype(int); a = (k - k0)[:, None]
    taps = tab[k0] * (1 - a) + tab[k0 + 1] * a
    idx = i[:, None] + (np.arange(N) - N // 2 + 1)[None, :]
    return (buf[idx] * taps).sum(1)
def render(x, r, scheme):
    n_out = 1 << 15
    if scheme == 'sqrt2':
        k = 0 if r <= 2 ** .5 else (1 if r <= 2 * 2 ** .5 else 2)
        N = 32
    else:
        k = 0 if r < 2 else 1
        N = None
    buf = x
    for _ in range(k): buf = level1(buf)
    rho = r / 2 ** k
    if rho <= 1: p, s = .4, .6; N = N or 32
    else:
        rp = 2 ** (np.ceil(np.log2(rho) * 16 - 1e-9) / 16); p, s = .4 / rp, .6 / rp
        N = N or (32 if rp <= 2 ** .5 + 1e-9 else 48)
    pos = 200 + rho * np.arange(n_out)
    return read(buf, pos, N, p, s), k, rho, N
def worst(f, r, scheme):
    n = np.arange(int((1 << 15) * r) + 4000)
    x = np.sin(2 * np.pi * f / fs * n)
    y, k, rho, N = render(x, r, scheme)
    w = np.kaiser(len(y), 26); Y = np.abs(np.fft.rfft(y * w)) / (w.sum() / 2)
    fr = np.arange(len(Y)) * fs / len(y)
    fo = f * r
    Ydb = 20 * np.log10(Y + 1e-12)
    tone = Ydb[np.abs(fr - fo) < 60].max() if fo < fs / 2 else -200
    mask = (np.abs(fr - fo) > 60) & (fr < 0.4 * fs) & (fr > 20)
    j = np.argmax(np.where(mask, Ydb, -999))
    return Ydb[j], fr[j], tone
for scheme in ('sqrt2', 'octave'):
    print('==', scheme, ' worst spur below 0.4 fs (dB re source tone amplitude), over source 100 Hz..18 kHz')
    for st in (-24, -12, 1, 5.9, 6.1, 7, 11.9, 12, 13, 17.9, 18.1, 19, 24):
        r = 2 ** (st / 12)
        best = (-999, 0, 0, 0)
        for f in np.concatenate([np.arange(100, 9000, 487.), np.arange(9000, 18001, 121.)]):
            sp, fsp, tone = worst(f, r, scheme)
            if sp > best[0]: best = (sp, fsp, f, tone)
        _, k, rho, N = render(np.zeros(200000), r, scheme)
        print(f"  {st:+5.1f} st r={r:.3f} level {k} rho={rho:.3f} taps {N}: worst {best[0]:7.1f} dB at {best[1]:7.0f} Hz (source {best[2]:6.0f} Hz, wanted tone {best[3]:6.1f} dB)")
