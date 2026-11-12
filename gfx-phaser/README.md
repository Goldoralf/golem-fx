# gfx-phaser

> **Status: CODED** (v0.1.0, 2026-11-12) — TPT all-pass cascade (exact prewarped corners, exponential sweep) with zero-delay feedback; Classic, Script 90, Block 90, Stone, Vibe (gfx-dsp Uni-Vibe core, lamp lag, chorus / vibrato), Bi-Phase (series / parallel, second LFO same / reverse / free), Deep 12, Barber ↑ / ↓; spread, colour, skew, eight LFO shapes, envelope / manual sweeps, tone, bass keep, host sync; stereo 2 / 2 first; 6 behaviour tests, bench green. Choices made in code: the barberpole is two cascades on sawtooth sweeps, crossfaded (the dual-flanger method), not an SSB shifter in the loop; the Classic family is linear at `color` 50 % (their own character is ideal), saturation comes in above it; notches sit where the bilinear map puts them (each stage's corner is exact).

golem's phaser. Stands for golem's `phaser-js` block (today REAPER's JS `guitar/phaser`), keeping its
parameter ids and units ([golem contract](../docs/golem-contract.md)). The aim: an exact, click-free
allpass engine with the classic voices — Phase 90 script and block, Small Stone, Uni-Vibe, Bi-Phase,
deep 12-stage, barberpole — zero latency.

## 1. The block being replaced

REAPER's JS `guitar/phaser` (LGPL, read on this machine): four first-order allpass stages with
`coef = (1 − d) / (1 + d)`, `d = 2f / fs`, **no prewarping** (the real break frequency is ≈ (2 / π)·f: the
labelled Hz are ~36 % off), a sweep linear in Hz, feedback from stage 4 with one sample of delay, output
`dry + wet · y4`. And **`lfoPhase = 0` in `@slider`: moving any parameter resets the LFO**, so an EXP pedal
or automation breaks the sweep. Not copied.

## 2. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference; **[U]** not checked.

### References

| Reference | What it does best | What we take |
|---|---|---|
| MXR Phase 90 — Eichas et al., *Physical Modeling of the MXR Phase 90*, DAFx 2014 ([PDF](https://dafx14.fau.de/papers/dafx14_felix_eichas_physical_modeling_of_the_.pdf)) [V] | 4 JFET allpass stages; one resistor adds feedback; the early **"script"** version has none; each JFET measured, they "strongly influence the tonal behaviour" | Script / Block models; per-stage mismatch |
| Uni-Vibe — Darabundit, Wedelich & Bischoff (Eventide), DAFx 2019 ([PDF](https://ccrma.stanford.edu/~champ/files/DAFx2019_paper_31.pdf)) [V] | 4 stages with **unequal capacitors** lit by one lamp; LFO ≈ 0 – 7.6 Hz; rate drifts with intensity; a block capacitor makes each stage a high shelf ("throb"); only ~2 notches audible at a time; LDR attack ≠ release; asymmetric clipping per stage | **The Vibe model** |
| [Guitarix Vibe](https://github.com/brummer10/guitarix/blob/master/trunk/src/plugins/vibe.cc) (GPL-2+) [V] | Capacitors 15 nF, 220 nF, 470 pF, 4.7 nF; lamp = one-pole after a turn-on curve; LDR with different attack / release | Ready reference for the Vibe |
| [Strymon Mobius](https://www.strymon.net/manuals/Mobius_UserManual_REVF.pdf) [V] | 2 – 16 stages, **Barber Pole** (from frequency shifters), regen, SPREAD stereo, a separate Vibe | Stage counts, barberpole, stereo spread |
| [Soundtoys PhaseMistress](https://www.soundtoys.com/product/phasemistress/) [V] | 2 – 24 stages, envelope and rhythm modes, 69 "styles" that change the structure | Envelope sweep; models as styles |
| [Moog MF-103](https://www.long-mcquade.com/2620/Keyboards/Sound_Processors_and_FX/Moog/Moogerfooger_Phaser.htm) [V] | 6 or 12 stages, 6-octave sweep | Deep model |
| [Surge Phaser](https://github.com/surge-synthesizer/surge/blob/main/src/common/dsp/effects/PhaserEffect.cpp) (GPL-3) [V] | Count 2 – 16, Spread, Sharpness (stage Q), Feedback ±, Tone, Stereo, S&H and noise LFOs; rate off = manual | Spread, sharpness, tone |
| [ChowPhaser](https://github.com/jatinchowdhury18/ChowPhaser) (BSD-3) [V] | Feedback and modulation stages on one LFO; LFO **skew**; drive / dirt nonlinearities in the feedback | Skew, colour |
| Small Stone, Maestro PS-1, Mu-Tron Bi-Phase [U] | OTA "Color" = more feedback; Bi-Phase = two phasers, two LFOs, series / parallel | Stone and Bi-Phase models |

### Papers and technique

- Zavalishin, *The Art of VA Filter Design* ([PDF](https://www.discodsp.net/VAFilterDesign_2.1.2.pdf)); Smith,
  PASP, *Phasing with First-Order Allpass Filters*: the **TPT first-order allpass** with g = tan(πf / fs) —
  exact frequencies, stable and click-free under audio-rate modulation.
- The cascade and its feedback are linear, so the feedback loop can be **solved exactly (zero-delay
  feedback)**: y = (G·x + S) / (1 − k·G) — no one-sample delay, resonance right at high frequencies [I].
  With nonlinear stages, one Newton step or the unit-delay loop.
- Huovilainen, DAFx 2005: one-pole cascades with embedded nonlinearities.
- Esqueda, Välimäki & Parker, DAFx 2015: barberpole phasing; an IIR Hilbert pair (polyphase allpass) gives
  a zero-latency single-sideband shifter for the loop [I].

### What we conclude

1. A **ZDF cascade of TPT allpasses** with exact, prewarped frequencies and an **exponential** sweep
   (f = low · (high / low)^u).
2. **Models** are stage count, spread, feedback, nonlinearity and LFO behaviour — not separate engines.
3. The **Vibe** is the one that needs its own care: unequal stages, shelving, lamp / LDR lag, clipping.
4. Envelope and manual sweeps; the LFO never resets on a parameter change; quadrature stereo.

## 3. Design

| `model` | Stages | Feedback | Character |
|---|---|---|---|
| Classic | `stages` | `feedback` | Ideal, linear |
| Script 90 | 4 | none | JFET colour, slight mismatch |
| Block 90 | 4 | internal fixed | JFET colour |
| Stone | 4 | `color` adds feedback | OTA |
| Vibe | 4 unequal (15 n / 220 n / 470 p / 4.7 n) | none | Shelving stages, lamp + LDR lag, biased tanh; chorus / vibrato |
| Bi-Phase | 2 × 6 | per phaser | Two LFOs, series / parallel |
| Deep 12 | 12 | `feedback` | Moog-style 6-octave sweep |
| Barber ↑ / ↓ | `stages` | SSB shifter in the loop | Endless rise / fall |

Output law: `dry + wet · phased` (the JS's), with `dry` now a parameter.

## 4. Parameters

Catalogue ids kept; `low` / `high` are now exact (prewarped) frequencies.

| id | Name | Type | Range | Catalogue | Default | EXP | Role |
|---|---|---|---|---|---|---|---|
| `rate` | Rate | f | 0.05 – 8 Hz | 0.05 – 5 | 0.5 Hz | ✓ | |
| `low` | Low | f | 20 – 4,000 Hz | 40 – 2,000 | 440 Hz | | Bottom of the sweep |
| `high` | High | f | 100 – 16,000 Hz | 200 – 8,000 | 1,600 Hz | | Top of the sweep |
| `feedback` | Feedback | f | −60 – −1 dB | same | −3 dB | ✓ | Magnitude |
| `wet` | Wet | f | −60 – +6 dB | same | 0 dB | | |
| `model` | Model | e | Classic · Script 90 · Block 90 · Stone · Vibe · Bi-Phase · Deep 12 · Barber ↑ · Barber ↓ | new | Classic | | |
| `stages` | Stages | e | 2 · 4 · 6 · 8 · 10 · 12 · 16 | new | 4 | | Classic and Barber |
| `spread` | Spread | f | 0 – 100 % | new | 0 % | | Unequal stage frequencies |
| `fb_polarity` | Feedback Polarity | e | + · − | new | + | | |
| `dry` | Dry | f | −60 – 0 dB | new | 0 dB | | |
| `manual` | Manual | f | 0 – 100 % | new | 50 % | ✓ | Sweep position between low and high; the EXP target |
| `depth` | Depth | f | 0 – 100 % | new | 100 % | ✓ | |
| `shape` | LFO Shape | e | Sine · Triangle · Log · Exp · Square (smoothed) · Ramp · Random · Steps | new | Sine | | |
| `skew` | Skew | f | 0 – 100 % (50 = symmetric) | new | 50 % | | |
| `sweep_src` | Sweep Source | e | LFO · Envelope · Manual · LFO + Envelope | new | LFO | | |
| `env_sens`, `env_att`, `env_rel` | Envelope | f | contract's shared ids | new | 50 %, 10 ms, 300 ms | | |
| `stereo_phase` | Stereo Phase | f | 0 – 180° | new | 90° | | Quadrature by default |
| `sync` | Sync | e | Off · 4 bars … 1/16 (+ dotted / triplets) | new | Off | | Phase from the host's position |
| `color` | Color | f | 0 – 100 % | new | 50 % | | 50 % = the model's own nonlinearity and mismatch (on Stone: its Color switch above 50 %); 0 = ideal |
| `lfo2` | Second LFO | e | Same · Reverse · Free | new | Same | | Bi-Phase: phaser B follows A, mirrors it, or runs on `rate2` |
| `rate2` | Rate 2 | f | 0.05 – 8 Hz | new | 0.3 Hz | | Bi-Phase, `lfo2` = Free |
| `routing` | Routing | e | Series · Parallel | new | Series | | Bi-Phase: A → B, or A left / B right |
| `lamp_lag` | Lamp Lag | f | 5 – 100 ms | new | 30 ms | | Vibe |
| `vibe_mode` | Vibe Mode | e | Chorus · Vibrato | new | Chorus | | Vibe: vibrato = wet only |
| `tone` | Tone | f | 0 – 100 % (50 = flat) | new | 50 % | | Low-pass ↔ high-pass on the wet |
| `bass_keep` | Bass Keep | f | 0 (off) – 300 Hz | new | 0 | | Lows stay dry and mono |

`skew` and `tone` use 0 – 100 % with 50 % as neutral, to avoid negative minimums.

## 5. Applications

The catalogue's law is `dry + wet · phased`: the deepest notches need `wet` = `dry`.

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Phase 90 "script" (early Van Halen, Gilmour) | Script 90, `rate` 0.6 Hz, wet 0 dB | No feedback: smooth, vocal |
| Phase 90 "block" | Block 90, `rate` 1 Hz | Internal feedback: more pronounced |
| Small Stone swirl | Stone, `rate` 0.3 Hz, `color` 80 % | OTA, feedback via Color |
| Uni-Vibe (Hendrix "Machine Gun", Trower) | Vibe, `rate` 1.2 Hz, `depth` 80 %, before the amp | Unequal stages, lamp lag, throb |
| Vibe as vibrato | Vibe, `vibe_mode` Vibrato | |
| Bi-Phase stereo sweep (Smashing Pumpkins) | Bi-Phase, `lfo2` Reverse, `routing` Parallel | Two phasers mirrored L / R |
| Deep slow sweep on keys / pads | Deep 12, `rate` 0.1 Hz, `feedback` −6 dB | 6 notches over 6 octaves |
| Endless rise / fall | Barber ↑ / ↓, `rate` 0.2 Hz | Barberpole |
| Funk auto-phase | Classic 4 stages, `sweep_src` Envelope | Envelope sweep |
| Phase with the foot | `sweep_src` Manual, EXP on `manual` | Exponential, click-free sweep |
| Sample-and-hold phase (synth) | `shape` Steps, `sync` 1/8 | |
| Bass | Script 90, `bass_keep` 120 Hz | |

## 6. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| Exact frequencies | at a fixed position, notch frequencies match the prewarped design within 1 % (the JS is 36 % off) |
| Exponential sweep | notch moves evenly in log-frequency |
| LFO continuity | moving any parameter never jumps the LFO phase (the JS defect) |
| Modulation | audio-rate sweep: no click, no instability |
| Feedback | −1 dB, both polarities: bounded, resonance where expected |
| Vibe | two audible notches; shelving "throb"; rate drift with depth as in DAFx-19 |
| Barber | notches move one way for 30 s with no audible reset |
| Listening | clean.wav and funk.wav against the JS block and against the Phase 90 / Vibe references |
