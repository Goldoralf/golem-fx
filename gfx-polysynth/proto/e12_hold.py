"""E12: as E10 with a hold time: a note gets a voice once present in every reading (10 ms apart) for H ms.
Readings start at the first string's onset. 4 seeds."""
import sys; sys.path.insert(0, sys.argv[1])
import io, contextlib
with contextlib.redirect_stdout(io.StringIO()):
    import proto, p2, p3
import numpy as np
FS = proto.FS
ch = {k: proto.CHORDS[k] for k in ['single E2', 'single E4', 'major third E4 G#4', 'major third low E2 G#2', 'E5 power (E2 B2 E3)', 'open E major', 'open C major', 'A minor barre V']}
ch.update({'single A2': [45], 'single D3': [50], 'open G': [43, 47, 50, 55, 59, 67], 'open D': [50, 57, 62, 66], 'open A': [45, 52, 57, 61, 64], 'G5 power': [43, 50, 55], 'E minor': [40, 47, 52, 55, 59, 64]})
HS = (20, 40, 60, 80, 120)
tot = {h: [0, 0, 0] for h in HS}
print('chord: for hold H in', HS, ': median ms to voice all uncovered notes / wrong voices per strum (mean) / uncovered notes never voiced (mean)')
for name, notes in ch.items():
    res = {h: [[], [], []] for h in HS}
    for seed in range(1, 5):
        proto.rng = np.random.default_rng(seed); x = proto.chord(notes, dur=0.75)
        mids, E = proto.cq_envelopes(x, rel_bw=0.5, min_bw=6.0); _, R = proto.cq_envelopes(x, rel_bw=10, min_bw=25.0)
        must = set(notes) - p2.covered(notes); allowed = set(notes); t_last = 0.07
        G = [(i, set(p3.name3(E[:, i], R[:, i], mids))) for i in range(int(0.05 * FS), int(0.70 * FS), 480)]
        for h in HS:
            k = h // 10; run = {}; voiced = {}; wrong = set()
            for i, g in G:
                run = {m: run.get(m, 0) + 1 for m in g}
                for m, c in run.items():
                    if c > k and m not in voiced:
                        voiced[m] = (i / FS - t_last) * 1000
                        if m not in allowed: wrong.add(m)
            res[h][0].append(max([voiced.get(m, 999) for m in must])); res[h][1].append(len(wrong)); res[h][2].append(len([m for m in must if m not in voiced]))
    print(f'{name:24s} ' + ' | '.join(f'{np.median(res[h][0]):4.0f} / {np.mean(res[h][1]):3.1f} / {np.mean(res[h][2]):3.1f}' for h in HS), flush=True)
    for h in HS: tot[h][0] += sum(res[h][1]); tot[h][1] += len(res[h][1]); tot[h][2] += sum(res[h][2])
for h in HS: print(f'hold {h} ms: wrong voices {tot[h][0]} in {tot[h][1]} strums; uncovered notes never voiced {tot[h][2]}')
