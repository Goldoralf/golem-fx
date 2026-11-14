"""E10: Poly naming on the designer's early reading (p3.name3), 6 seeds, readings every 10 ms,
acceptance = the note present in 2 successive readings (+ the fundamental check inside name3).
Counts voices that would start on a note neither played nor covered, and the time each played, uncovered note takes."""
import sys; sys.path.insert(0, sys.argv[1])
import io, contextlib
with contextlib.redirect_stdout(io.StringIO()):
    import proto, p2, p3
import numpy as np
FS = proto.FS
names = ['single E2', 'single E4', 'major third E4 G#4', 'major third low E2 G#2', 'E5 power (E2 B2 E3)', 'open E major', 'open C major', 'A minor barre V']
extra = {'open G': [43, 47, 50, 55, 59, 67], 'open D': [50, 57, 62, 66], 'open A': [45, 52, 57, 61, 64], 'G5 power': [43, 50, 55], 'E minor': [40, 47, 52, 55, 59, 64]}
ch = {k: proto.CHORDS[k] for k in names}; ch.update(extra)
tot_wrong = 0; tot_strums = 0
print('chord                    | per seed: time (ms) until every uncovered note has a voice / wrong voices started / uncovered notes never voiced in 600 ms')
for name, notes in ch.items():
    row = []
    for seed in range(1, 7):
        proto.rng = np.random.default_rng(seed)
        x = proto.chord(notes, dur=0.75)
        mids, E = proto.cq_envelopes(x, rel_bw=0.5, min_bw=6.0); _, R = proto.cq_envelopes(x, rel_bw=10, min_bw=25.0)
        must = set(notes) - p2.covered(notes); allowed = set(notes); t_last = 0.07
        prev = set(); voiced = {}; wrong = set()
        for i in range(int(0.05 * FS), int(0.67 * FS), 480):
            g = set(p3.name3(E[:, i], R[:, i], mids)); acc = g & prev; prev = g
            for m in acc:
                if m not in voiced: voiced[m] = (i / FS - t_last) * 1000
            wrong |= (acc - allowed)
        tfull = max([voiced.get(m, np.inf) for m in must])
        miss = len([m for m in must if m not in voiced])
        row.append(f'{tfull:4.0f}/{len(wrong)}/{miss}'); tot_wrong += len(wrong); tot_strums += 1
    print(f'{name:24s} | ' + '  '.join(row), flush=True)
print('wrong voices', tot_wrong, 'in', tot_strums, 'strums')
