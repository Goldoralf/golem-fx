import numpy as np, wave
rng = np.random.default_rng(1)
fs = 48000; DUR = 30
def sig(kind):
    n = fs * (DUR + 3)
    if kind == 'white': x = rng.standard_normal(n)
    elif kind == 'pink':
        X = np.fft.rfft(rng.standard_normal(n)); f = np.arange(len(X)); f[0] = 1; x = np.fft.irfft(X / np.sqrt(f), n)
    elif kind == 'tone110':   # sustained guitar-like note, harmonics 1/k
        t = np.arange(n) / fs; x = sum(np.sin(2 * np.pi * 110 * k * t + k) / k for k in range(1, 30))
    elif kind == 'tone440':
        t = np.arange(n) / fs; x = sum(np.sin(2 * np.pi * 440 * k * t + k) / k for k in range(1, 12))
    elif kind == 'di':
        w = wave.open('/home/raph/code/golem-fx/bench-out/shimmer-ab/for-windows/clean.wav'); 
        a = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16 if w.getsampwidth()==2 else np.int32).astype(float)
        a = a.reshape(-1, w.getnchannels())[:, 0]; x = np.tile(a, int(np.ceil(n / len(a))))[:n]
    return x / np.sqrt(np.mean(x ** 2))
def cloud(x, N, T_ms, spray_ms, regular, kshape=None):
    """unity-ratio grains, Hann. returns wet power / source power"""
    T = int(T_ms * fs / 1000); n = fs * DUR; base = fs  # 1 s back
    lam = N / T
    if regular: on = np.arange(0, n - T, 1 / lam)
    else:
        k = kshape or 1.0
        iv = rng.gamma(k, 1 / (lam * k), int(n * lam * 1.2) + 10); on = np.cumsum(iv); on = on[on < n - T]
    w = 0.5 - 0.5 * np.cos(2 * np.pi * (np.arange(T) + 0.5) / T)
    y = np.zeros(n + T); s = np.zeros(n + T)
    off = np.rint(rng.uniform(-1, 1, len(on)) * spray_ms * fs / 1000).astype(int)
    for o, d in zip(on, off):
        i = int(o); fr = o - i
        ww = 0.5 - 0.5 * np.cos(2 * np.pi * (np.arange(T) + 0.5 - fr) / T) if regular else w
        src = i + 2 * fs - base + d
        y[i:i + T] += ww * x[src:src + T]; s[i:i + T] += ww
    sl = slice(T, n - T)
    return np.mean(y[sl] ** 2), np.var(s[sl]), np.mean(s[sl])
Ew, Ew2 = 0.5, 0.375
def doc_model(N, spray_ms):
    sig = spray_ms / np.sqrt(3); c = np.exp(-sig / 2.0)
    return N * Ew2 + c * N * (N - 1) * Ew ** 2
def new_model(N, c, V):
    return N * Ew2 + c * (N * N * Ew * Ew + V - N * Ew2)
print("V (variance of window sum): measured vs N*E[w2]/k")
for N in (2, 8, 64):
    for k in (1, 2, 5, 50):
        _, V, m = cloud(np.zeros(fs * (DUR + 3)), N, 120, 0, False, k)
        print(f"  N={N:2d} gamma k={k:2d}: V={V:8.3f}  N*Ew2/k={N * Ew2 / k:8.3f}  mean={m:.3f}")
    _, V, m = cloud(np.zeros(fs * (DUR + 3)), N, 120, 0, True); print(f"  N={N:2d} regular: V={V:.2e} mean={m:.4f}")
print("\nwet level re source (dB) after normalisation by each model; 0 = perfect.  T=120 ms")
for kind in ('white', 'pink', 'tone110', 'tone440', 'di'):
    x = sig(kind)
    for regular in (False, True):
        for N in (1, 2, 8, 64):
            row = []
            for spray_ms in (0, 0.1, 0.5, 2, 10, 100):
                P, V, m = cloud(x, N, 120, spray_ms, regular)
                Vm = 0 if (regular and N > 1) else (N * Ew2 if not regular else Ew2 - Ew ** 2)
                c_doc = np.exp(-spray_ms / np.sqrt(3) / 2)
                row.append(f"{10 * np.log10(P / doc_model(N, spray_ms)):+5.1f}/{10 * np.log10(P / new_model(N, c_doc, Vm)):+5.1f}")
            print(f"  {kind:8s} {'reg ' if regular else 'pois'} N={N:2d} spray(ms) 0|.1|.5|2|10|100 doc/new: " + '  '.join(row))
