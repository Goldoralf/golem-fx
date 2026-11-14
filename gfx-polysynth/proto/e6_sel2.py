from lib import *
import lib
fs = 24000; cb = clean_bank(); Rc = design_rot(cb)
orig_sel = lib.sel_weights; orig_if = lib.inst_freq
def run(x, notes, tau_w, width, gc, tau_if=0.010, purity=True):
    lib.sel_weights = lambda bank, Z, width_db=3.0, tau=0.005: orig_sel(bank, Z, width, tau_w)
    return imd_db(lib.shape_sel(cb, cb.analyse(x), 'square', rot=Rc, gaincorr=gc, purity=purity), notes)
cfg = [(0.005, 3, True), (0.005, 3, False), (0.03, 3, False), (0.03, 6, False), (0.03, 6, True), (0.1, 6, False), (0.03, 12, False)]
print('cfg (tau_w, width_dB, gaincorr): ' + str(cfg))
for name in ('open E', 'E4+G#4', 'E2+G#2', 'E5 power', 'open C', 'Am barre V'):
    notes = CHORDS[name]; x = down2(chord(np.random.default_rng(1), notes))
    print(f'{name:12s} ' + ' '.join(f'{run(x, notes, *c):6.1f}' for c in cfg), flush=True)
