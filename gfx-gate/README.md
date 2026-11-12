# gfx-gate

> **Status: PROPOSED** — design, no code yet.

golem's noise gate. Stands for golem's `gate` block (today ReaGate), keeping its parameter ids and units
([golem contract](../docs/golem-contract.md)). A new sound, not a clone: it keeps what makes a pedal gate
useful in front of a high-gain amp, and removes what makes ReaGate (as golem uses it) click and chatter.

## 1. What changes from ReaGate

golem-rack measured ReaGate as golem leaves it (`~/golem/docs/ardour-plan.md` §7c, 2026-11-12):
sample-by-sample peak detection, gain ramped **linearly** 0 → 1 over Attack and 1 → 0 over Release,
closed = fully silent, sidechain filters as RBJ biquads with Q = 1.

| | ReaGate (as golem uses it) | gfx-gate | Why |
|---|---|---|---|
| Detection | raw peak, sample by sample | peak follower: instant rise, fixed ~5 ms fall | a raw peak dips at every zero crossing; on a low E near the threshold it chatters unless Hold covers half a period |
| Opening | linear gain ramp over Attack | S-curve (raised cosine) over Attack; Attack 0 = a 50 µs minimum | a linear ramp has a corner at each end → an audible tick on fast attacks |
| Closing | linear gain ramp over Release | linear **in dB** from 0 dB to the floor over Release | sounds like a natural decay; a linear gain ramp ends abruptly in the last dB |
| Closed level | silent | `range`, new parameter (default: silent) | a slightly open gate (−20 … −40 dB) is more natural on clean or bass |
| Reopening during Release | — | resumes from the current gain, never jumps | no click on fast re-attacks (chugs) |
| Sidechain filters | RBJ, Q = 1 | Butterworth (Q = 0.707) | flat pass-band, the threshold means the same at every frequency |
| Stereo | — | linked detection (max of both channels), same gain on both | no image shift |

Kept as is: threshold, hysteresis (close threshold = threshold + hysteresis), hold, zero latency (no
lookahead — it would need reported latency, and golem wants none).

## 2. Parameters

Catalogue ids and units are kept; ranges are a little wider. Defaults = the catalogue's.

| id | Name | Type | Range | Catalogue | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|---|
| `thresh` | Threshold | f | −90 – 0 dB | −70 – −10 | −50 dB | lin | 20 ms | | Opening level, peak, after the sidechain filters |
| `attack` | Attack | f | 0 – 100 ms | 0 – 50 | 3 ms | log | — | | Time to open fully (S-curve) |
| `hold` | Hold | f | 0 – 1,000 ms | 0 – 500 | 20 ms | log | — | | Stays open this long after the level falls below the close threshold |
| `release` | Release | f | 5 – 2,000 ms | 20 – 1,000 | 150 ms | log | — | | Time from open to `range`, linear in dB |
| `hpf` | Sidechain Low Cut | f | 0 – 2,000 Hz | 0 – 1,000 | 0 (off) | log | 20 ms | | High-pass on the detection only; 0 = off |
| `lpf` | Sidechain High Cut | f | 500 – 22,000 Hz | 1,000 – 20,000 | 20,000 Hz | log | 20 ms | | Low-pass on the detection only; ≥ 20 kHz = off |
| `hyst` | Hysteresis | f | −24 – 0 dB | −12 – 0 | 0 dB | lin | 20 ms | | Close threshold = `thresh` + `hyst` |
| `range` | Range | f | −90 – 0 dB | new | −90 dB (= silent) | lin | 20 ms | | Level when closed; −90 = fully silent |

Times are not smoothed: they are read when a phase starts. `hpf` keeps golem's convention 0 = off,
because its block presets use it (Soft: `hpf` 0).

**To check on the bench (contract):** `thresh`, `hyst` and `range` have negative minimums; check that
REAPER and Ardour write them exactly (plain ↔ normalized, both ways).

## 3. Behaviour

States, per sample:

```
CLOSED ──level > thresh──▶ ATTACK ──gain = 1──▶ OPEN
OPEN ──level < thresh + hyst──▶ HOLD ──hold elapsed──▶ RELEASE ──gain = range──▶ CLOSED
HOLD / RELEASE ──level > thresh──▶ ATTACK (from the current gain) or OPEN
```

- The level is the peak follower on the sidechain-filtered input (both channels in stereo).
- The gain moves along its curve from wherever it is; a phase never restarts from 0 or 1.
- Changing `thresh` while closed never opens on a step alone: the level must cross it.

## 4. Bench suite (on top of gfx-bench's common checks)

| Check | Signal | Pass |
|---|---|---|
| Opening time | sine step, below → above threshold | reaches −0.1 dB of the input in Attack ± 1 sample |
| Hold + Release time | sine step, above → silence | starts closing after Hold; reaches `range` after Release ± 1 sample |
| Hysteresis | slow level ramp up then down | opens at `thresh`, closes at `thresh + hyst` (± 0.1 dB) |
| No chatter | decaying low E (82 Hz), hold 0, hyst 0 | a single open → close sequence |
| Clicks | sine bursts at Attack 0 and Release 5 ms | no energy above 10 kHz beyond the sine's own spectrum + a bound |
| Sidechain filters | sine steps at 40 Hz with `hpf` 120 Hz | does not open |
| Range | above, then below threshold, `range` −30 dB | closed level = −30 dB ± 0.1 |
| Stereo link | signal on L only | same gain on L and R |
| Chug test | `high-gain.wav`, golem preset Chug-tight | no false openings in rests; renders kept for listening |

Listening is done in context, gate before a high-gain amp, through golem (the rack renders the chain);
with golem's five gate presets: Standard, Soft, Chug-tight, Bass gate, Vocal gate.
