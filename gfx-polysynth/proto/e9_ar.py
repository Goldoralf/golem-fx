from lib import *
import lib
fs = 24000; cb = clean_bank(); Rc = design_rot(cb); fb = fast_bank(); Rf = design_rot(fb)
def make(att, rel, width):
    def sw(bank, Z, width_db=6.0, tau=0.03):
        ka = np.exp(-1 / (att * bank.fs)); kr = np.exp(-1 / (rel * bank.fs)); M = np.abs(Z); A = np.empty_like(M); s = np.zeros(M.shape[0])
        for i in range(M.shape[1]):
            m = M[:, i]; k = np.where(m > s, ka, kr); s = k * s + (1 - k) * m; A[:, i] = s
        L = 20 * np.log10(A + 1e-12)
        up = np.vstack([L[1:], np.full((1, L.shape[1]), -300.0)]); dn = np.vstack([np.full((1, L.shape[1]), -300.0), L[:-1]])
        return np.clip((L - up) / width + 0.5, 0, 1) * np.clip((L - dn) / width + 0.5, 0, 1)
    return sw
def env(y, ms=10):
    n = int(ms * 1e-3 * fs); k = np.hanning(2 * n + 1); k /= k.sum(); return np.sqrt(np.convolve(y * y, k, 'same'))
ts = (0.02, 0.04, 0.06, 0.08, 0.10, 0.12, 0.15, 0.20, 0.3)
for att, rel, width, tg in ((0.003, 0.05, 6.0, 0.03), (0.003, 0.05, 6.0, 0.01), (0.002, 0.1, 6.0, 0.03), (0.003, 0.05, 3.0, 0.03)):
    lib.sel_weights = make(att, rel, width); print('--- att', att, 'rel', rel, 'width', width, 'tau_g', tg)
    for label, m in (('E2', 40), ('E3', 52), ('E4', 64)):
        x = down2(chord(np.random.default_rng(1), [m], dur=0.8, t0=0.1, t60=30.0)); t = np.arange(len(x)) / fs - 0.1
        for bn, bk, R in (('clean', cb, Rc), ('fast', fb, Rf)):
            y = shape_sel(bk, bk.analyse(x), 'saw', rot=R, tau_g=tg); e = env(y); body = np.median(e[(t > 0.4) & (t < 0.6)])
            print(f'  {label} {bn:5s} ' + ' '.join(f'{20 * np.log10(e[np.argmin(np.abs(t - q))] / body):6.1f}' for q in ts))
    row = []
    for name in ('E5 power', 'open E', 'open C', 'Am barre V', 'E4+G#4', 'E2+G#2'):
        notes = CHORDS[name]; x = down2(chord(np.random.default_rng(1), notes)); Z = cb.analyse(x)
        row.append(f'{name} {imd_db(shape_sel(cb, Z, "square", rot=Rc, tau_g=tg), notes):.1f}')
    print('  chords sq clean: ' + ' | '.join(row), flush=True)
