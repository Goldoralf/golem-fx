import numpy as np
from scipy.ndimage import maximum_filter1d
from scipy.signal import butter, sosfilt, lfilter
FS = 48000

def slidmax(a, w):
    w = max(1, int(w))
    # causal: max over [n-w+1, n]
    pad = np.concatenate([np.zeros(w - 1), a])
    return maximum_filter1d(pad, size=w, origin=(w - 1) // 2 if False else 0, mode='constant')[w//2: w//2 + len(a)] if False else _cmax(a, w)

def _cmax(a, w):
    from numpy.lib.stride_tricks import sliding_window_view
    pad = np.concatenate([np.zeros(w - 1), a])
    return sliding_window_view(pad, w).max(axis=1)

def window(sc_hpf, rule='gate'):
    if rule == 'gate':
        return np.clip(1.2 / max(sc_hpf, 30.0), 0.008, 0.040)
    if rule == 'half':
        return np.clip(1.2 / max(sc_hpf / 2, 30.0), 0.008, 0.040)
    return rule  # seconds

def detector(x, sc_hpf=0.0, hpf_order=2, att_time=15.0, sus_time=300.0, sens=50.0, thresh=-66.0,
             attack=0.0, sustain=0.0, smooth=0.5, rule='gate', rf_rel=10.0, dref=None, fs=FS,
             smooth_sus=False, floor_win=48.0, D0=1.5):
    sc = x.copy()
    if sc_hpf > 0:
        sc = sosfilt(butter(hpf_order, sc_hpf, 'hp', fs=fs, output='sos'), sc)
    W = window(sc_hpf, rule)
    p = _cmax(np.abs(sc), int(W * fs))
    n = len(x)
    th = 10 ** (thresh / 20)
    ka = np.exp(-1 / (att_time * 1e-3 * fs))
    krf = np.exp(-1 / (rf_rel * 1e-3 * fs))
    krs = np.exp(-1 / (sus_time * 1e-3 * fs))
    ksm = np.exp(-1 / (smooth * 1e-3 * fs))
    Dref = dref if dref is not None else 22 - 20 * sens / 100
    P = th; hold = 0; fall = 10 ** (-3 / 20 / fs)
    Es = th; Erf = th; Ers = th; sh = 0.0; shs = 0.0
    Da = np.zeros(n); Ds = np.zeros(n); Ga = np.zeros(n); Gs = np.zeros(n); Fl = np.zeros(n)
    fw = 10 ** (-floor_win / 20)
    for i in range(n):
        pi = p[i]
        if pi >= P: P = pi; hold = int(5 * fs)
        elif hold > 0: hold -= 1
        else: P = max(P * fall, th, pi)
        F = max(th, P * fw)
        Ef = max(pi, F)
        Es = max(Es, F)
        if Ef < Es: Es = Ef
        else: Es = Ef + (Es - Ef) * ka
        da = 20 * np.log10(Ef / Es)
        # sustain pair
        Erf = pi if pi > Erf else pi + (Erf - pi) * krf
        erf = max(Erf, F)
        Ers = pi if pi > Ers else pi + (Ers - pi) * krs
        ers = max(Ers, F)
        ds = 20 * np.log10(ers / erf)
        ta = 1 - np.exp(-max(0.0, da - D0) / Dref)
        sh = ta if ta < sh else ta + (sh - ta) * ksm      # one-pole on rises only
        gs = sustain * (1 - np.exp(-max(0.0, ds - D0) / Dref))
        if sustain > 0: gs = min(gs, max(0.0, 20 * np.log10(erf / F)))
        if smooth_sus:
            shs = gs + (shs - gs) * ksm; gs = shs
        Da[i] = da; Ds[i] = ds; Ga[i] = attack * sh; Gs[i] = gs; Fl[i] = F
    return dict(p=p, Da=Da, Ds=Ds, Ga=Ga, Gs=Gs, G=Ga + Gs, F=Fl, W=W)

def ks_string(f, dur, decay_db_s=8.0, amp=0.5, pick=0.25, bright=0.5, fs=FS, seed=1, asym=0.0):
    """decaying harmonic string with plucked spectrum + short pick noise; asym adds even harmonics phase"""
    t = np.arange(int(dur * fs)) / fs
    y = np.zeros_like(t)
    for k in range(1, 40):
        if k * f > 0.45 * fs: break
        a = np.sin(np.pi * k * pick) / k ** (1 + (1 - bright))
        y += a * np.exp(-t * (decay_db_s * (1 + 0.15 * (k - 1))) / 8.686) * np.sin(2 * np.pi * k * f * t + asym * k)
    y /= np.max(np.abs(y))
    rng = np.random.default_rng(seed)
    click = rng.standard_normal(len(t)) * np.exp(-t / 0.002) * 0.6
    click = lfilter([1, -0.95], [1], click)
    y = y + click * 0.5
    return amp * y / np.max(np.abs(y))

def snare(dur, decay_db_s=200.0, amp=0.5, fs=FS, seed=2):
    t = np.arange(int(dur * fs)) / fs
    rng = np.random.default_rng(seed)
    y = rng.standard_normal(len(t)) * np.exp(-t * decay_db_s / 8.686) * 0.6 + np.sin(2 * np.pi * 190 * t) * np.exp(-t * decay_db_s * 0.7 / 8.686)
    return amp * y / np.max(np.abs(y))

def place(total, items, fs=FS):
    y = np.zeros(int(total * fs))
    for t0, s in items:
        i = int(t0 * fs); m = min(len(s), len(y) - i); y[i:i + m] += s[:m]
    return y
