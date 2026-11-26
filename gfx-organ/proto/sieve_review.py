# SPDX-License-Identifier: GPL-3.0-or-later
"""The root sieve as reviewed (README §2.3, Review 2026-11-26), on the draft model of organ.py.

Four changes to organ.Organ.sieve, each behind a switch:
- k runs to the top of the bank (kmax 48), so the high partials of a low note are tested;
- `settle`: the tolerance of the overtone test is widened by what the two bands' frequencies may still be
  off by (15 ¢ per τ_b short of 9.5 τ_b, capped at 80 ¢). A band is confirmed a root when the exact test
  has passed for 4 τ_b AND the widened one passes; once confirmed it only needs the exact test;
- `same_pick`: a band is the overtone of a lower band only if that band was struck by the pick that
  brought it in (complex-domain novelty per band over 68 ms after each pick, or entering the selection
  within 150 ms of it; the lower band itself, or one of its partials that is not a partial of the candidate).
  `nov_thr` (1.0 x what the band held) says when a band belongs to a pick; `nov_parent` says when a lower
  band was struck by it (None: the same test; 0.25: as soon as it is disturbed by a quarter, which covers
  more: README §1.4 (6) gives both). A confirmed root keeps its status across
  later picks and into the neighbouring band (a bend); after a gap in the selection (up to 250 ms) it
  is confirmed again as a note of its own pick;
- `lowest_fast` (off; a measured candidate, not the design): the lowest candidate is confirmed after 2 τ_b.
"""
import numpy as np
import organ as O

FB, TICK = O.FB, O.TICK


def settle_cents(age_tau, slope=15.0, end=9.5, cap=80.0):
    return np.clip(slope * (end - age_tau), 0.0, cap)

class Reviewed(O.Organ):
    def __init__(self, *a, settle=True, same_pick=True, parent_db=40.0, nov_thr=1.0, nov_parent=None, kmax=48, lowest_fast=False, **k):
        super().__init__(*a, kmax=kmax, **k)
        self.settle, self.same_pick, self.parent_db, self.nov_thr = settle, same_pick, parent_db, nov_thr
        self.lowest_fast = lowest_fast
        self.nov_parent = nov_parent

    def sieve(self, onsets=(0.05,)):
        s, fh, ldb, z, idx = self.s, self.fh, self.ldb, self.z, self.idx
        n, T = s.shape
        tau_t = self.bank.tau * FB / TICK                      # τ_b in ticks
        need = np.ceil(self.confirm * tau_t).astype(int)
        F = np.ones((n, T)); Fw = np.ones((n, T)); low = np.zeros((n, T), bool); since = np.zeros(T, int) + 10 ** 6
        Lmax = ldb.max(axis=0)
        cand = (s > 0.5) & (ldb > Lmax[None, :] + self.floor_db) & (ldb > 20 * np.log10(self.abs_floor))
        ks = np.arange(2, self.kmax + 1)
        st = 1200 / np.log(2) * self.bstretch * (ks ** 2 - 1) / 2
        on_tick = [int(o * FB / TICK) for o in onsets]
        ev_len = int((0.020 + 3 * self.bank.tau.max()) * FB / TICK)
        E = len(on_tick)
        new = np.zeros((n, E), bool); fev = np.zeros((n, E))
        struck = np.zeros((n, E), bool)
        birth = np.full(n, -1)
        age = np.zeros(n, int); gap = np.zeros(n, int); gmax = int(0.020 * FB / TICK)
        cache = {}
        snap = None; e_open = -1; touched = np.zeros(n, bool); hit = np.zeros(n, bool)
        ne = 0
        self.age_log = np.zeros((n, T)); self.birth_log = np.zeros((n, T), int); self.new = new
        R = np.zeros_like(F); und = np.zeros_like(F)
        run = np.zeros(n, int); runl = np.zeros(n, int); conf = np.zeros(n, bool)
        lim = np.maximum(2 * need, int(0.22 * FB / TICK)) if self.settle else 2 * need
        t3 = int(3 * tau_t.max())
        impure = self.pur < self.pure_min
        lastc = np.full(n, -10 ** 6); lastf = np.zeros(n); lastb = np.full(n, -1); lasta = np.zeros(n, int)
        hold_t = int(0.250 * FB / TICK)
        late_t = int(0.150 * FB / TICK); prev_c = np.zeros(n, bool)
        def excited(q, k, e, t, tolx):
            # was band q struck by pick e? itself new, or one of its partials that is not a partial of the candidate
            if struck[q, e]: return True
            fq = fh[q, t]
            for b in np.nonzero(struck[:, e] & cand[:, t])[0]:
                if b <= q: continue
                m = int(round(fh[b, t] / fq))
                if m < 2 or m > 8 or m % k == 0: continue
                c = 1200 * np.log2(fh[b, t] / (m * fq))
                if -self.tol_c - tolx < c < self.tol_c + tolx + 1200 / np.log(2) * self.bstretch * (m * m - 1) / 2:
                    return True
            return False
        for t in range(T):
            c = cand[:, t]
            gap = np.where(c, 0, gap + 1)
            age = np.where(c, age + 1, np.where(gap > gmax, 0, age))
            # picks: snapshot, running novelty (a band that gains a component re-ages), decision at the window's end
            if ne < E and t == on_tick[ne]:
                t0 = max(t - int(0.005 * FB / TICK), 0)
                snap = (idx[t0], z[:, idx[t0]].copy(), fh[:, t0].copy()); e_open = ne; touched[:] = False; hit[:] = False; ne += 1
                t_open = t
            if e_open >= 0:
                s0, z0, f0 = snap
                pred = z0 * np.exp(2j * np.pi * f0 * (idx[t] - s0) / FB)
                lv = np.abs(z[:, idx[t]])
                nw = (np.abs(z[:, idx[t]] - pred) > self.nov_thr * np.abs(z0)) & (lv > 10 ** (self.floor_db / 20) * lv.max())
                dist = (np.abs(z[:, idx[t]] - pred) > 0.25 * np.abs(z0)) & (lv > 10 ** (self.floor_db / 20) * lv.max())
                if self.nov_parent is not None:
                    hit |= (np.abs(z[:, idx[t]] - pred) > self.nov_parent * np.abs(z0)) & (lv > 10 ** (self.floor_db / 20) * lv.max())
                fresh = dist & ~touched
                age = np.where(fresh, np.minimum(age, t - t_open), age)
                touched |= dist
                if t - t_open >= ev_len:
                    nw = nw & c
                    struck[:, e_open] = (nw if self.nov_parent is None else hit & c)
                    new[:, e_open] = nw; fev[:, e_open] = fh[:, t]
                    birth = np.where(nw & ~conf & (t - lastc > hold_t), e_open, birth); e_open = -1
            # a root's status travels with its partial. At once into the neighbouring band (a bend) or through a
            # gap of the selection under 20 ms; after a longer gap (a pick's transient, up to 250 ms) the band
            # gets back the pick it belongs to and its age, and is confirmed again in the normal way
            inherit = np.zeros(n, bool)
            got = np.zeros(n, bool)
            if self.same_pick:
                for bnd in np.nonzero(c & ~conf)[0]:
                    for d in (bnd, bnd - 1, bnd + 1):
                        if 0 <= d < n and not conf[d] and t - lastc[d] <= hold_t and abs(1200 * np.log2(fh[bnd, t] / lastf[d])) < 50:
                            birth[bnd] = lastb[d]; age[bnd] = max(age[bnd], lasta[d])
                            inherit[bnd] = t - lastc[d] <= gmax
                            got[bnd] = True
                            break
                # a band that enters the selection within 150 ms of a pick was struck by that pick (the low bands
                # are not selected yet when the 68 ms window closes); later entries belong to no pick
                if ne > 0 and t - on_tick[ne - 1] <= late_t:
                    fresh_c = c & ~prev_c & ~got & ~conf & (t - lastc > hold_t)
                    new[fresh_c, ne - 1] = True
                    struck[fresh_c, ne - 1] = True
                    birth[fresh_c] = ne - 1
            prev_c = c.copy()
            self.age_log[:, t] = age
            if ne > 0: since[t] = t - on_tick[ne - 1]
            pk = np.nonzero(c)[0]
            if len(pk): low[pk[0], t] = True
            fp = fh[pk, t]; lp = ldb[pk, t]
            ex = settle_cents(age[pk] / tau_t[pk]) if self.settle else np.zeros(len(pk))
            for i in range(1, len(pk)):
                ratio = fp[i] / fp[:i]
                cents = 1200 * np.log2(ratio[:, None] / ks[None, :])
                tolx = (ex[:i] + ex[i])[:, None]
                lev = (lp[:i][:, None] > lp[i] - self.parent_db)
                if not lev.any(): low[pk[i], t] = True
                for wide, dst in ((False, F), (True, Fw)):
                    tx = tolx if wide else 0.0
                    ok = (cents > -self.tol_c - tx) & (cents < self.tol_c + tx + st[None, :]) & lev
                    if not ok.any(): continue
                    if not self.same_pick or birth[pk[i]] < 0:
                        dst[pk[i], t] = 0.0; continue
                    for j, kk in zip(*np.nonzero(ok)):
                        if excited(pk[j], ks[kk], birth[pk[i]], t, (ex[j] + ex[i]) if wide else 0.0):
                            dst[pk[i], t] = 0.0; break
            self.birth_log[:, t] = birth
            F[impure[:, t] | ~c, t] = 0.0; Fw[~c, t] = 0.0
            run = np.where(F[:, t] > 0.5, run + 1, 0)
            runl = np.where(low[:, t] & c, runl + 1, 0)
            ok = ((run >= need) & (Fw[:, t] > 0.5)) | (conf & (F[:, t] > 0.5))
            ok |= inherit & (F[:, t] > 0.5)
            if self.lowest_fast:
                ok |= (runl >= need // 2) & (since[t] >= t3)
            conf = ok
            lastc[ok] = t; lastf[ok] = fh[ok, t]; lastb[ok] = birth[ok]; lasta[ok] = age[ok]
            R[:, t] = ok
            und[:, t] = c & ~ok & (age < lim)
        self.Fraw = F
        self.F = O.one_pole(R, 0.010, FB / TICK, axis=1)
        self.pre = O.one_pole(und, 0.005, FB / TICK, axis=1)
        self.cand = cand
        return self
