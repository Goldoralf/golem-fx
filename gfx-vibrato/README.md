# gfx-vibrato

> **Status: PROPOSED** — design, no code yet.

golem's pitch vibrato. Stands for golem's `vibrato` block (today guitarix GxTubeVibrato;
`golem_vibrato.jsfx` on Windows / macOS), keeping its parameter ids and units
([golem contract](../docs/golem-contract.md)). The aim: a vibrato whose depth means the same thing at
every speed, with the classic voices — clean, BBD (Boss VB-2), Magnatone varistor, Uni-Vibe, Hammond
scanner — and as little delay as physics allows.

## 1. What golem has today

From [gxtubevibrato.dsp](https://github.com/brummer10/guitarix/blob/master/trunk/src/LV2/faust/gxtubevibrato.dsp)
and `golem_vibrato.jsfx` [V]: delay = (10 … 15 ms) × depth, linear interpolation, triangle LFO; guitarix
also splits into 5 bands through 12AX7 stages.

Consequences [I, computed]:

- A hidden 5 – 10 ms delay on the whole signal.
- Depth scales the centre delay: an EXP sweep on depth bends the pitch (0 → 1 in 0.5 s ≈ +34 cents).
- A triangle delay gives a **square-wave pitch** (two pitches with hard jumps).
- Pitch depth grows with speed: several semitones at depth 1 and 14 Hz.
- **golem's vibrato presets look stale**: the catalogue moved `speed` to Hz (0.1 – 14) but Seasick,
  Standard and Vibe still store 0.25 / 0.5 / 0.4 — about ±5 cents, almost inaudible. They need
  re-authoring in Hz (golem side).

## 2. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference.

| Reference | Facts | What we take |
|---|---|---|
| Magnatone varistor ([Robbins, *Magnatone Vibrato Design*](https://dalmura.com.au/static/Magnatone%20vibrato%20design.pdf), [Magnatone](https://www.magnatoneusa.com/faqs/what-is-magnatones-true-pitch-shifting-vibrato-and-how-does-it-work/)) [V] | An RC bridge swings the phase by up to ~180°, R = varistors (~25:1): **phase modulation through a modulated first-order allpass**, centred near 1 kHz, so depth depends on frequency; slight residual AM; some models cascade two stages | **A true zero-latency vibrato** (no delay buffer) |
| Uni-Vibe, [DAFx 2019](https://ccrma.stanford.edu/~champ/files/DAFx2019_paper_31.pdf) [V] | Vibrato mode = the four unequal stages, wet only | Shared with gfx-phaser's Vibe |
| Hammond scanner, [Werner, Dunkel & Germain, DAFx 2016](https://dafx.de/paper-archive/2016/dafxpapers/38-DAFx-16_paper_54-PN.pdf) [V] | 19-stage LC ladder (~7 kHz); a rotating capacitor crossfades linearly between taps, there and back; rotor ≈ 6 – 7 Hz; V1 / V2 / V3 reach taps 9, 13, 19 (different depth and shape); chorus = vibrato + dry | **Scanner model** |
| [setBfree vibrato.c](https://github.com/pantherb/setBfree/blob/master/src/vibrato.c) (GPL) [V] | One fractional delay, sine, 7.25 Hz; offsets in **samples** (depth changes with the sample rate) | Constants; depth in ms, not samples |
| [Boss VB-2](https://support.roland.com/hc/en-us/articles/201967789-VB-2-Specifications) [V], [TC Shaker](https://im.static-thomann.de/pics/atg/atgdata/document/manual/259460_manual.pdf) [V] | Rate 2 – 15 Hz; **Rise Time** (150 ms – 5 s, on engage and release); tone; momentary latch | Rise time, tone |
| [Airwindows Vibrato](https://github.com/airwindows/airwindows/tree/master/plugins/LinuxVST/src/Vibrato) (MIT) [V] | Depth ∝ 1 / √speed (partial constant detune); a second LFO frequency-modulates the first (wow-like) | Constant detune; wobble |

### What we conclude

1. **Depth in cents, constant across speeds.**
2. **Minimal delay**: for a sine of peak c cents at rate f the smallest mean delay is
   (2^(c/1200) − 1) / (2πf) — 0.9 ms for 50 cents at 5 Hz, 9.3 ms at 0.5 Hz. It moves with the settings,
   so it is **not** reported as latency; it is kept as small as the depth needs, and changed slowly.
3. **Varistor gives a true zero-latency vibrato** (phase modulation, no delay line).
4. **Sine or rounded LFO**: a plain triangle gives a square-wave pitch.
5. **Rise time** on engage and release.

## 3. Design

| `model` | How | Latency |
|---|---|---|
| **Clean** (default) | Lagrange-5 delay, depth in cents constant across speeds, mean delay = the minimum the depth needs (capped at 25 ms: very low speeds saturate) | Unreported, minimal |
| **BBD** | gfx-dsp BBD engine, wet only, tone (Boss VB-2) | Unreported, minimal |
| **Varistor** | Two cascaded first-order allpasses centred ~1 kHz, R swept 25:1 on a log curve, slight AM | **Zero** |
| **Vibe** | gfx-phaser's Vibe block in vibrato mode | Zero |
| **Scanner** | Tap delay approximating the 19-stage ladder + its 7 kHz low-pass; piecewise-linear crossfade; depth sweeps V1 → V3 continuously; speed defaults to ~6.9 Hz; `mix` 0.5 = C1 – C3 | Unreported, ~1 ms |

- The centre delay changes through a ≥ 100 ms slew (moving it adds a pitch offset equal to its slope).
- `rise`: depth ramps in on engage and out on release; needs the bypass parameter (`make_bypass`) so the
  plugin keeps running; the LFO never stops.
- Phase 0 at reset, never reset by a parameter change.

## 4. Parameters

Catalogue ids kept.

| id | Name | Type | Range | Catalogue | Default | EXP | Role |
|---|---|---|---|---|---|---|---|
| `depth` | Depth | f | 0 – 1 | same | 0.4 | ✓ | 0 – 100 cents (Clean, BBD); stage swing (Varistor, Vibe); V1 → V3 (Scanner) |
| `speed` | Speed | f | 0.1 – 14 Hz | same | 5 Hz | ✓ | |
| `model` | Model | e | Clean · BBD · Varistor · Vibe · Scanner | new | Clean | | |
| `shape` | Shape | e | Sine · Rounded Triangle · Random | new | Sine | | Random = smoothed, fixed seed, with LFO wobble |
| `rise` | Rise Time | f | 0 – 5,000 ms | new | 0 ms | | On engage and release |
| `tone` | Tone | f | 0 – 1 | new | 0.7 | | Post low-pass |
| `mix` | Mix | f | 0 – 1 | new | 1 | | 0.5 = chorus (Hammond C modes, Uni-Vibe chorus) |
| `sync` | Sync | e | Off · divisions | new | Off | | |
| `stereo_phase` | Stereo Phase | f | 0 – 180° | new | 0° | | L / R LFO offset |
| `drive` | Drive | f | 0 – 1 | new | 0 | | Tube colour, outside the delay |

## 5. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Gentle pitch vibrato on clean chords | Clean, speed 5 Hz, depth 0.15 (15 cents) | Depth in cents: the same wobble at any speed |
| Seasick (lo-fi, shoegaze) | BBD, speed 2 Hz, depth 0.6, `tone` 0.5 | BBD darkness |
| Magnatone amp vibrato | Varistor, speed 5.5 Hz, depth 0.6 | Phase modulation: zero latency, depth strongest around 1 kHz |
| Vibe vibrato (Hendrix, Trower) | Vibe, speed 4 Hz, depth 0.7 | Shared with gfx-phaser |
| Hammond V3 / C3 on keys or guitar | Scanner, depth 1 (V3), `mix` 1 (vibrato) or 0.5 (chorus) | Scanner model |
| Vibrato that grows on held notes (VB-2) | `rise` 800 ms, switched by foot | Rise time on engage and release |
| Tape wow | Clean, `shape` Random, speed 0.8 Hz, depth 0.2 | Seeded random LFO |
| Chorus from a vibrato | any model, `mix` 0.5 | |
| Speed on the pedal | EXP on `speed`, depth constant in cents | |
| Whammy-bar wobble | EXP on `depth` | No pitch blip while the depth moves |

## 6. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| Constant depth | instantaneous frequency of 1 kHz and 4 kHz sines: peak cents within ±5 % of the setting from 0.5 to 14 Hz (Clean) |
| Varistor | zero latency; depth larger at 1 kHz than at 100 Hz and 10 kHz |
| Mean delay | equals the minimum for the depth (+ guard), ≤ 25 ms |
| EXP on depth | < 2 cents of pitch blip, no zipper |
| Rise | depth reaches 63 % at `rise` ± 5 % |
| Phase | moving any parameter never jumps the LFO phase |
| Interpolation | HF loss and aliasing at 10 kHz within bounds |
| Listening | lead-clean.wav and clean.wav, re-authored presets, against GxTubeVibrato |
