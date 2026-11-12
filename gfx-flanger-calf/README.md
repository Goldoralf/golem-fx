# gfx-flanger-calf

> **Status: CODED** (v0.1.0, 2026-11-12). This is gfx-flanger's engine behind the ids of golem's `flanger` block (Calf Flanger).

golem has two flanger blocks whose ids clash in meaning:

| Block | Plugin today | Ids that clash |
|---|---|---|
| `flanger-js` | JS `guitar/flanger` | `feedback` in dB, `length` in ms |
| `flanger` | Calf Flanger | `depth` is the sweep width in ms; `feedback` is linear, −0.9 to 0.9 |

[gfx-flanger](../gfx-flanger/) stands for `flanger-js`. This second VST3 class stands for `flanger`, on the same engine, so Calf presets keep their values. Raph approved the second class on 2026-11-12.

| id | Range | Default | Meaning (Calf's) |
|---|---|---|---|
| `rate` | 0.05 – 5 Hz | 0.3 Hz | LFO rate |
| `depth` | 0.1 – 10 ms | 2 ms | Sweep width: the delay runs from `min_delay` to `min_delay + depth` |
| `feedback` | −0.99 – 0.99 | 0.6 | Linear and signed. Mapped to the engine's magnitude in dB (capped at −1 dB) and its polarity |
| `min_delay` | 0.1 – 10 ms | 0.5 ms | |
| `amount` | 0 – 1 | 0.5 | Wet level |
| `dry` | 0 – 1 | 1 | Dry level |
| `stereo_phase` | 0 – 180° | 90° | |
| `mode` | gfx-flanger's modes | Mistress | Same engine choice as gfx-flanger (Raph: keep the dark BBD default) |

- `min_delay` and `depth` glide together (log ms, 50 ms), so the two ends of the sweep never part ways while one of them moves.
- Class id `GoleMFxFlangerC1`. Stereo 2 / 2 first. Tape TZ modes report 1 ms of latency, as in gfx-flanger.
