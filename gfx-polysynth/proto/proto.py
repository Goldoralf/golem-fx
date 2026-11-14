"""gfx-polysynth design prototype.
1) IMD of a full-band fuzz vs per-band harmonic generation on chords (engine (b)).
2) Time to name a chord's notes with a semitone-grid constant-Q bank + harmonic summation (engine (a)).
Synthetic strings, 48 kHz."""
import numpy as np
from scipy.signal import lfilter

FS = 48000
rng = np.random.default_rng(1)

def midi_hz(m):
    return 440.0 * 2 ** ((m - 69) / 12)

def string(f0, dur, t0=0.0, B=1e-4, t60=3.0, p=0.13, bright=1.0, level=1.0):
    n = int(dur * FS)
    t = np.arange(n) / FS
    y = np.zeros(n)
    s = t - t0
    on = s >= 0
    k = 1
    while True:
        fk = f0 * k * np.sqrt(1 + B * k * k)
        if fk > 7000:
            break
        a = abs(np.sin(np.pi * k * p)) / k ** (1.0 / bright)
        tk = t60 / (1 + 0.12 * k)
        ph = rng.uniform(0, 2 * np.pi)
        y[on] += a * np.exp(-6.91 * s[on] / tk) * np.sin(2 * np.pi * fk * s[on] + ph)
        k += 1
    # pick transient: 4 ms of noise, -26 dB
    i0 = int(t0 * FS)
    m = int(0.004 * FS)
    y[i0:i0 + m] += 0.05 * rng.standard_normal(m) * np.hanning(m)
    return level * y

def chord(notes, dur=1.6, spread=0.02, **kw):
    y = sum(string(midi_hz(m), dur, t0=0.05 + i * spread / max(1, len(notes) - 1), **kw)
            for i, m in enumerate(notes))
    return y / np.max(np.abs(y)) * 0.5

def cbank(x, fc, bw, order=2):
    """complex one-pole cascade, pole bandwidth bw (Hz, -3 dB full width of one pole)."""
    r = np.exp(-np.pi * bw / FS)
    a = r * np.exp(2j * np.pi * fc / FS)
    z = x.astype(complex)
    for _ in range(order):
        z = lfilter([1 - r], [1, -a], z)
    # normalise centre gain to 1
    return z

def erb(f):
    return 24.7 * (4.37 * f / 1000 + 1)

# ---------------- engine (b): IMD ----------------
def harmonic_mask(freqs, notes, B=1e-4, tol_c=25.0):
    m = np.zeros_like(freqs, dtype=bool)
    for nn in notes:
        f0 = midi_hz(nn)
        for k in range(1, 400):
            fk = f0 * k * np.sqrt(1 + B * min(k, 60) ** 2)
            if fk > FS / 2:
                break
            tol = max(3.0, fk * (2 ** (tol_c / 1200) - 1))
            m |= np.abs(freqs - fk) < tol
            # pure harmonic too (products of harmonic partials)
            fk2 = f0 * k
            m |= np.abs(freqs - fk2) < tol
    return m

def imd_db(y, notes, seg=(0.35, 1.35)):
    a, b = int(seg[0] * FS), int(seg[1] * FS)
    s = y[a:b] * np.blackman(b - a)
    S = np.abs(np.fft.rfft(s)) ** 2
    f = np.fft.rfftfreq(b - a, 1 / FS)
    sel = (f > 30) & (f < 8000)
    m = harmonic_mask(f, notes)
    inh = S[sel & ~m].sum()
    tot = S[sel].sum()
    return 10 * np.log10(inh / tot + 1e-20)

def fuzz(x, gain=30):
    y = np.tanh(gain * x)
    return y

def band_centres(f_lo=60, f_hi=6000, step_erb=1.0, floor_hz=0.0):
    # ERB-number spacing
    def erbn(f):
        return 21.4 * np.log10(4.37e-3 * f + 1)
    def inv(e):
        return (10 ** (e / 21.4) - 1) / 4.37e-3
    e = np.arange(erbn(f_lo), erbn(f_hi), step_erb)
    return inv(e)

def perband(x, waveform='square', step_erb=1.0, width=1.0, order=2, nmax=15, fcap=8000):
    fcs = band_centres(step_erb=step_erb)
    out = np.zeros(len(x))
    for fc in fcs:
        bw = width * step_erb * erb(fc)
        z = cbank(x, fc, bw, order)
        mag = np.abs(z) + 1e-12
        u = z / mag
        acc = np.zeros(len(x))
        if waveform == 'square':
            ns = [n for n in range(1, nmax + 1, 2)]
            w = {n: 1.0 / n for n in ns}
        elif waveform == 'saw':
            ns = list(range(1, nmax + 1))
            w = {n: 1.0 / n for n in ns}
        elif waveform == 'identity':
            ns = [1]
            w = {1: 1.0}
        un = np.ones(len(x), complex)
        for n in range(1, max(ns) + 1):
            un = un * u
            if n in w and n * fc < fcap:
                acc += w[n] * un.real
        out += mag * acc
    return out

CHORDS = {
    'E5 power (E2 B2 E3)': [40, 47, 52],
    'open E major': [40, 47, 52, 56, 59, 64],
    'open C major': [48, 52, 55, 60, 64],
    'A minor barre V': [45, 52, 57, 60, 64, 69],
    'major third E4 G#4': [64, 68],
    'major third low E2 G#2': [40, 44],
    'single E2': [40],
    'single E4': [64],
}

def part1():
    print('== engine (b): non-harmonic energy (IMD) re total, 30 Hz - 8 kHz ==')
    print(f"{'input':28s} {'dry':>7s} {'fuzz':>7s} {'sq 1ERB':>8s} {'sq .5ERB':>8s} {'saw .5ERB':>9s}")
    for name, notes in CHORDS.items():
        x = chord(notes)
        r = [imd_db(x, notes), imd_db(fuzz(x), notes),
             imd_db(perband(x, 'square', 1.0), notes),
             imd_db(perband(x, 'square', 0.5), notes),
             imd_db(perband(x, 'saw', 0.5), notes)]
        print(f"{name:28s} " + ' '.join(f'{v:7.1f}' for v in r))

# ---------------- engine (a): naming the notes ----------------
def cq_envelopes(x, lo=40, hi=105, rel_bw=0.5, min_bw=6.0, order=2):
    """semitone-grid bank, envelope per band (|z|)."""
    mids = np.arange(lo, hi + 1)
    E = []
    for m in mids:
        fc = midi_hz(m)
        bw = max(rel_bw * fc * (2 ** (1 / 12) - 1), min_bw)
        z = cbank(x, fc, bw, order)
        E.append(np.abs(z) * (1 - np.exp(-np.pi * bw / FS)) ** 0 )
    E = np.array(E)
    # normalise each band's centre gain (cascade gain at centre is 1 for each pole: (1-r)/(1-r) = 1)
    return mids, E

H_OFF = [int(round(12 * np.log2(h))) for h in range(1, 17)]

def name_notes(spec, mids, lo_note=40, hi_note=76, max_notes=6, rel_floor=0.25):
    """Klapuri-style: harmonic sum on the semitone grid, iterative cancellation with a smoothness
    estimate. spec: band magnitudes (len(mids))."""
    r = spec.copy()
    idx0 = mids[0]
    found = []
    first = None
    for _ in range(max_notes):
        best, bm = 0, None
        for m in range(lo_note, hi_note + 1):
            if m in found:
                continue
            s = 0.0
            for h, off in enumerate(H_OFF, start=1):
                j = m + off - idx0
                if j >= len(r):
                    break
                s += r[j] * (0.6 + 1.0 / h)  # weight like Klapuri (f + a)/(h f + b), simplified
            if s > best:
                best, bm = s, m
        if bm is None:
            break
        if first is None:
            first = best
        elif best < rel_floor * first:
            break
        # require energy at the fundamental band itself (no missing-fundamental notes on guitar)
        if r[bm - idx0] < 0.08 * spec.max():
            # mark as not a note: cancel nothing, but exclude it
            found.append(bm)
            found.remove(bm)
            # exclude by zeroing its fundamental band contribution
            r[bm - idx0] *= 0.0
            continue
        found.append(bm)
        # cancel: subtract from each partial band the min of the band and a smooth estimate
        for h, off in enumerate(H_OFF, start=1):
            j = bm + off - idx0
            if j >= len(r):
                break
            nb = [r[k] for k in (j - 1, j, j + 1) if 0 <= k < len(r)]
            est = min(r[j], np.mean(nb))
            r[j] = max(0.0, r[j] - est)
    return sorted(found)

def part2():
    print('\n== engine (a): time to name the chord (semitone bank + harmonic sum) ==')
    for rel_bw, min_bw in [(0.5, 6.0), (0.5, 12.0), (1.0, 12.0), (1.0, 25.0)]:
        print(f'-- band width {rel_bw} semitone, floor {min_bw} Hz')
        for name, notes in CHORDS.items():
            x = chord(notes, dur=0.8)
            mids, E = cq_envelopes(x, rel_bw=rel_bw, min_bw=min_bw)
            t_last = 0.05 + 0.02  # last string's onset
            ok_since, t_ok = None, None
            wrong = set()
            for i in range(int(t_last * FS), int(0.75 * FS), 48):
                got = name_notes(E[:, i], mids)
                if got == sorted(notes):
                    if ok_since is None:
                        ok_since = i
                    elif i - ok_since >= int(0.05 * FS) and t_ok is None:
                        t_ok = (ok_since / FS - t_last) * 1000
                else:
                    ok_since = None
                    if t_ok is None:
                        wrong.add(tuple(got))
            # stability after naming
            print(f"   {name:26s} named after {('%.0f ms' % t_ok) if t_ok is not None else 'never':>8s}")

if __name__ == '__main__':
    part1()
    part2()
