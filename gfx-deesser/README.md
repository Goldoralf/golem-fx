# gfx-deesser

> **Status: PROPOSED** — design, no code yet.

golem's de-esser. Stands for golem's `deesser` block (today golem's own `golem_deesser.jsfx`), keeping
its parameter ids and units ([golem contract](../docs/golem-contract.md)). The aim: sibilance tamed at
any input level without lisping, zero latency by default — and the same engine as a "de-harsh" for
guitar.

## 1. What the JSFX does today

From `~/golem/reaper/Effects/golem_deesser.jsfx`: mono sum → 2-pole high-pass at `freq` → peak follower
(1 ms / 60 ms) against an **absolute** threshold → ratio 6:1 capped at `range` → the band is subtracted
back (`x + hp·(g − 1)`).

Its weaknesses:

- **The threshold depends on the input level**: a compressor, the mic distance or the singer's dynamics
  move the trigger point.
- **No upper limit on detection**: air and cymbal bleed above ~12 kHz trigger it.
- **The subtractive split is lumpy**: `1 − H_hp` has a +1.76 dB bump at `freq` and only 6 dB / oct, so
  the dynamic curve is an uneven shelf.
- `listen` solos the band after reduction, not what the detector hears; stereo is not handled; no gain
  reduction is reported.

## 2. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference or not re-checked.

### Commercial references

| Product | What it does best | What we take |
|---|---|---|
| [FabFilter Pro-DS](https://www.fabfilter.com/help/pro-ds/using/advancedcontrols) [V] | "Single Vocal" (smart detection) vs "Allround" (HF limiter); wide band vs split band; lookahead up to 15 ms, **zero latency** without it; audition of the trigger | Vocal / Allround focus; listen = the trigger; lookahead optional |
| [Sonnox SuprEsser](https://media.uaudio.com/support/manuals/dd/Oxford%20SuprEsser%20DS%20Manual.pdf) [V] | **Auto level tracking**: the threshold follows the full-band level (fast up, slow down); the fader is an offset | **Relative detection**, the main win |
| [oeksound soothe2](https://oeksound.com/manuals/soothe2/) [V] | Detects resonances by their **relative prominence**; Delta (hear what is removed); FFT, so latency [I] | Relative detection; listen-the-removed |
| [Eiosis / Slate e2deesser](https://www.soundonsound.com/reviews/eiosis-e2-de-esser) [V] | Separates "sibilants" from "voiced" (air); level-independent detector | Cap the detection band so air does not trigger |
| [Waves Sibilance](https://www.waves.com/plugins/sibilance) [V] | Detects the sibilant component of the voice; a zero-latency "Live" version | Sibilance classification, live variant |
| [Weiss DS1-MK3](https://weiss.ch/products/pro-audio/ds1-mk3/) [V] | LP / BP / HP band shapes, M/S | Several shapes |

### Open source

| Project | Idea |
|---|---|
| [Calf Deesser](https://calf-studio-gear.org/doc/Deesser.html) [V] | Wide vs split; a peak filter in the sidechain only; "wideband sounds much cleaner on a single vocal" |
| [Airwindows DeEss](https://github.com/airwindows/airwindows/blob/master/plugins/LinuxVST/src/DeEss/DeEssProc.cpp) / [DeBess](https://github.com/airwindows/airwindows/blob/master/plugins/LinuxVST/src/DeBess/DeBessProc.cpp) [V] | No band at all: sibilance detected by **slew** (second-derivative-like); attenuation of the residual above a low-pass |

### What we conclude

1. **Relative detection**: sibilance = detection band level − full-band level (≤ 0 dB by construction, so
   the existing `thresh` range −50 … 0 dB fits). The threshold holds across singers, mics and upstream
   gain.
2. **A capped detection band** (`freq` to ~2.5 × `freq`, at most 16 kHz).
3. **A confidence weight** from zero-crossing rate / slew: sibilants are noise-like, vowels and guitar
   fundamentals are not. It blocks false triggers on bright vowels and pick attacks.
4. **Shapes**: wide (cleanest on a solo voice), shelf (anything with lows), bell (harshness).
5. **A phase-coherent, bump-free reduction**: a dynamic SVF shelf or bell whose gain moves per sample; 0 dB
   reduction = the input exactly.
6. **Lookahead only as an option**, reported to the host. Without it, the first ~1 ms of an "s" passes:
   mostly inaudible.

## 3. Design

```
in ─┬───────────────────────────────────────────▶ dynamic SVF (shelf / bell) or wide gain ─▶ out
    └▶ detector: band-pass freq … 2.5·freq ─ envelope ─┐                     ▲
       full-band envelope (fast up, slow down) ────────┴▶ relative level ▶ × ZCR weight (Vocal)
                                                         ▶ soft-knee gain computer (ratio, range)
```

- Envelopes: band peak / RMS hybrid, attack 0.5 ms, release `release`; full band fast on rises, slow on
  falls (SuprEsser).
- Gain computer: 6 dB soft knee, `ratio`, capped by `range`, smoothed in dB.
- Stereo: detection on max(|L|, |R|), the same gain on both channels.
- Lookahead (optional): delays the main path and the gain curve together, reported with
  `set_latency_samples`; preallocated for 10 ms at the highest sample rate.

## 4. Parameters

Catalogue ids kept; defaults = the catalogue's.

| id | Name | Type | Range | Catalogue | Default | Role |
|---|---|---|---|---|---|---|
| `thresh` | Threshold | f | −50 – 0 dB | −50 – 0 | −28 dB | Relative: band − full band; Absolute: band level |
| `freq` | Frequency | f | 1,500 – 14,000 Hz | 2,000 – 12,000 | 6,000 Hz | Start of the detection band; corner of the shelf / centre of the bell |
| `range` | Range | f | 0 – 24 dB | 0 – 24 | 10 dB | Maximum reduction |
| `listen` | Listen | b | off / on | bool | off | Solo of what the detector hears |
| `detect` | Detection | e | Relative · Absolute | new | Relative | Absolute = the JSFX's behaviour |
| `shape` | Shape | e | Shelf · Bell · Wide | new | Shelf | |
| `focus` | Focus | e | Vocal · Allround | new | Vocal | Vocal adds the zero-crossing confidence weight; Allround = pure high-frequency limiter (guitar de-harsh, cymbals) |
| `ratio` | Ratio | f | 1 – 20 :1 | new | 6 :1 | The JSFX's fixed ratio, now a parameter |
| `release` | Release | f | 10 – 200 ms | new | 60 ms | The JSFX's value |
| `lookahead` | Lookahead | f | 0 – 10 ms | new | 0 ms | Above 0: reported latency |

Read-only (meter): `gr`, gain reduction in dB.

## 5. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Lead vocal, live | Relative, Vocal, Shelf, `freq` 6 kHz, `thresh` −28 dB, `range` 8 dB | Relative detection: holds when the singer backs off the mic or a compressor sits upstream |
| Bright female vocal | `freq` 7.5 kHz, Bell | |
| Heavy sibilance | `ratio` 10, `range` 12 dB | |
| Pick click / fizz on an overdriven guitar | Allround, Bell, `freq` 3.5 kHz, `release` 40 ms | The same engine as a de-harsh |
| Cymbal bleed in a vocal mic | `freq` 6 kHz: the detector stops at 15 kHz | Capped detection band |
| Finding the sibilance | `listen` on, sweep `freq` | Hear the trigger |
| Studio pass, fully transparent | `lookahead` 5 ms | Catches the first millisecond; reported latency |

## 6. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| Transparency | below threshold: bit-identical output (no bump, unlike the JSFX) |
| Level independence | the same sibilant phrase at −30, −20, −10 dBFS: the same reduction ± 1 dB (Relative) |
| Air | a 14 kHz shelf of noise does not trigger with `freq` 6 kHz |
| Vowels | a bright vowel (formants 2 – 4 kHz) triggers ≤ 1 dB in Vocal focus |
| Latency | 0 samples at lookahead 0; reported = actual otherwise |
| Listening | voice/male.wav and voice/female.wav (to record) against the JSFX, level-matched; guitar crunch with Allround / Bell at 3.5 kHz |
