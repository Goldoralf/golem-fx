# gfx-chorus — voicing

> **Status: PROPOSED** — design, no code yet. Numbers come from the state-of-the-art survey
> ([README.md](README.md) §State of the art). **[V]** = measured or read in a source; **[U]** = forum or
> unverified; **[I]** = our choice. Everything is tuned by ear on the listening bench, and this document is
> updated with the values that survive.

This page fills the **P** values of [PARAMS.md](PARAMS.md). Depth is always the **peak** deviation (± ms).

## 1. Who sets what

Two layers, never mixed:

- **The mode** sets the **shape of the modulation** and what the reference unit does intrinsically: voice
  count, centre delay, depth, LFO, phases, cross-mix, BBD stages, compander. A Dimension sounds like a
  Dimension whatever the instrument.
- **The profile** sets **everything else**: crossover, wet filters, colour engine and amounts, stereo,
  envelope follower, mix, the default mode, and a **scale** on the mode's default depth.

When a group is **Custom**, its own controls replace the layer's anchors for that group only.

## 2. Mode anchors

### Classic — Boss CE-2

Source: [ElectroSmash CE-2 analysis](https://www.electrosmash.com/boss-ce-2-analysis) [V].

| Anchor | Value | Note |
|---|---|---|
| Voices | 1, mono wet | |
| Centre delay / depth | 8 ms ± 2.5 ms [I] | MN3007 floor is 5.12 ms [V]; to tune by ear |
| LFO | Triangle, RC-smoothed (72 Hz) [V] | |
| Rate | default 0.8 Hz; the unit tops out near 3.5 Hz [V] | |
| BBD | 1024 stages, anti-alias / reconstruction ≈ 6.6 kHz two-pole [V] | |
| Emphasis | 100 % (≈ +15 dB above ~410 Hz – 2.3 kHz, de-emphasised after) [V] | |
| Compander | 0 % — the CE-2 has none [V] | |
| Mix | 50 / 50 by equal resistors [V] | |

### Dual — Roland Juno-60

Source: [pendragon-andyh Juno-60 chorus analysis](https://github.com/pendragon-andyh/Juno60/blob/master/Chorus/README.md)
(measured) [V].

| Anchor | Dual I | Dual II | Dual I+II |
|---|---|---|---|
| Rate default | 0.513 Hz | 0.863 Hz | 9.75 Hz (vibrato) |
| Centre delay / depth | 3.5 ± 1.85 ms (1.66 – 5.35 ms) | 3.5 ± 1.85 ms | 3.5 ± 0.2 ms (3.3 – 3.7 ms) |
| Voices | 2: right channel's modulation inverted (180°) | same | 1, mono |
| LFO | Triangle | Triangle | Triangle |
| BBD | 256 stages, ~70 kHz clock, 12 dB low-pass before the line | same | same |
| Compander | 0 % — none in the schematic [V] | | |

Arturia describes Mode II as deeper as well as faster [V]; the measurement says same depth. We follow
the measurement and check against reference recordings.

### Dimension — Roland Dimension D (SDD-320)

Sources: [SDD-320 owner's manual](https://archive.org/stream/SDD-320_owners_manual/SDD-320_owners_manual_djvu.txt)
[V], [Arturia Chorus DIMENSION-D manual](https://www.arturia.com/products/software-effects/chorus-dimension-d/overview)
("measured on original units") [V], Fractal forum [U].

| Anchor | Dimension 1 | Dimension 2 | Dimension 3 |
|---|---|---|---|
| Relative to mode 1 [V] | — | delay ≈ ½ | delay between 1 and 2, depth × 2 |
| Centre delay / depth [I, to measure] | 6 ± 0.6 ms | 3 ± 0.6 ms | 4.5 ± 1.2 ms |
| Rate default [U] | 0.25 Hz | 0.25 Hz | 0.5 Hz |
| Voices | 2, LFOs at 180°, triangle | | |
| Cross-mix | each output = its voice + the other voice **polarity-inverted and high-passed** (low shelf so the bass does not cancel) [V] | | |
| Compander | always on (compressor → delay → expander per channel) [V] | | |
| Dry | slightly bass-boosted [V] | | |

The original's button 4 is a wet boost combined with 1 – 3 [V]: covered by `mix`, not a mode.

### Ensemble — string ensembles (Solina)

Sources: [J. Haible, Triple Chorus](http://jhaible.com/legacy/triple_chorus/triple_chorus.html) [V],
[Surge XT BBD Ensemble](https://github.com/surge-synthesizer/surge/blob/main/src/common/dsp/effects/BBDEnsembleEffect.cpp) [V].

| Anchor | Value |
|---|---|
| Voices | 3, from two 3-phase LFOs (slow + fast) summed per voice |
| Slow LFO | 0.6 Hz default (forum 0.6 – 0.8 [U]; Surge 0.18) |
| Fast LFO | 6 Hz, about 20 % of the slow depth (Surge ≈ 20 % [V]) |
| Centre delay / depth | 5 ± 1 ms [I] |
| Stereo | shared middle voice: L = voices 1 + 2, R = voices 2 + 3 [V, Surge] — naturally mono-compatible |
| Imperfection | 30 %: the original's 120° phases and amplitudes are approximate, and that is part of the sound [V] |
| BBD | 512 stages; modulation drives the clock (delay ∝ 1 / f_clk) [V] |

### Doubler — studio double-tracking

| Anchor | Value |
|---|---|
| Voices | 2, each on its own smoothed random path (no periodic LFO) |
| Centre delay / depth | 18 ± 0.8 ms [I] |
| Rate default | 0.25 Hz (speed of the random wander) |

## 3. Profile anchors

| Group | Parameter | Guitar Clean | Guitar Drive | Bass | Keys | Voice |
|---|---|---|---|---|---|---|
| — | default `mode` | Classic | Dimension 2 | Classic | Dual I | Doubler |
| Modulation | default depth scale (× mode's depth) | 1.0 | 0.7 | 0.8 | 1.0 | 0.6 |
| Modulation | `drift` | 10 % | 15 % | 5 % | 5 % | 60 % |
| Modulation | `imperfection` (outside Ensemble) | 10 % | 10 % | 5 % | 15 % | 0 % |
| Crossover | `xover_on` | off | on | on | on | off |
| Crossover | `xover_freq` | (120 Hz) | 150 Hz | 200 Hz | 100 Hz | (120 Hz) |
| Color | `engine` | BBD | Clean | BBD | BBD | Clean |
| Color | `drive` | 25 % | 5 % | 30 % | 20 % | 0 % |
| Color | `emphasis` | 100 % | (100 %) | 100 % | 0 % | (0 %) |
| Color | `noise` | 15 % | 0 % | 10 % | 40 % | 0 % |
| Dynamics | `env_mix` | 0 % | −30 % | 0 % | 0 % | −20 % |
| Filters | `wet_hpf` | 80 Hz | 250 Hz | 40 Hz | 60 Hz | 150 Hz |
| Filters | `wet_lpf` | 8 kHz | 5 kHz | 4 kHz | 10 kHz | 12 kHz |
| Stereo | `stereo_phase` | 90° | 180° | 90° | 180° | 90° |
| Stereo | `width` | 100 % | 100 % | 60 % | 100 % | 70 % |
| Stereo | `mono_safe` | off | on | on | off | on |
| Output | `mix` default | 50 % | 35 % | 50 % | 50 % | 30 % |
| Output | `mix_law` | Linear | Equal Power | Linear | Linear | Equal Power |

`bbd_chip`, `compander` and `bbd_bw` come from the mode (§2), since they belong to the reference unit.
Values in parentheses apply only if the user switches that part on in a Custom group.

Why each profile looks like this:

- **Guitar Clean** — the CE-2: BBD with emphasis, dark wet, some hiss, linear 50 / 50 mix for the deep
  phase notches.
- **Guitar Drive** — after the distortion, it must not blur it: clean engine, tight lows (crossover
  150 Hz, wet low-cut 250 Hz), dark wet against fizz, Dimension 2 for width without wobble, and the wet
  ducked on attacks (`env_mix` −30 %) so chugs stay clear and the chorus blooms on sustains.
- **Bass** — below 200 Hz the signal stays dry and mono; only mids and highs move. Narrower stereo,
  mono-safe, dark BBD wet.
- **Keys** — the Juno: 256-stage BBD, no emphasis, audible hiss (from `noise`, not from a compander,
  since the Juno has none).
- **Voice** — a discreet double: no colour, random drift, low mix, wet low-cut at 150 Hz, the wet slightly
  ducked on syllable attacks.

## 4. Warmth (front panel)

Warmth 50 % = the anchors above. 0 % moves towards clean and bright, 100 % towards dark and coloured.

| Profile | `wet_lpf` | `bbd_bw` | `drive` | `noise` |
|---|---|---|---|---|
| Guitar Clean | +1 → −1 oct | +0.5 → −0.5 oct | × 0 → × 2 | × 0.5 → × 1.5 |
| Guitar Drive | +0.5 → −1 oct | — | × 0 → × 2 | — |
| Bass | +0.5 → −1 oct | +0.5 → −0.5 oct | × 0 → × 2 | × 0.5 → × 1.5 |
| Keys | +1 → −1 oct | +0.5 → −0.5 oct | × 0 → × 2 | × 0.5 → × 2 |
| Voice | +0.5 → −0.5 oct | — | — | — |

Octave shifts are interpolated linearly in octaves, multipliers linearly; results are clamped to the
parameter's range.

## 5. Depth (front panel)

Depth is the peak deviation itself, ± ms (golem catalogue id `chorus.depth`). Its default is the mode's
depth × the profile's scale, clamped to 0.1 – 10 ms. Log curve: the subtle settings get most of the
knob's travel. With `detune_lock` on, depth is scaled by (default rate / rate) so the pitch deviation
stays constant when the rate moves.

## 6. Placement in a golem chain

What the Designer should suggest when the block is inserted (a suggestion, never a constraint):

| Profile | Suggested position | Why |
|---|---|---|
| Guitar Clean | after amp and cab | stereo from there on; BBD hiss is not amplified by the amp |
| Guitar Drive | after amp and cab, never before a high-gain stage | a chorus before distortion turns into mush |
| Bass | after preamp / amp capture | the crossover works on the shaped tone |
| Keys | on the instrument's lane, after the instrument | |
| Voice | on the vocal lane, after compression | |

## 7. To validate on the bench

- Juno modes against the [pendragon Juno-60 recordings](https://github.com/pendragon-andyh/Juno60) and the
  [rc-effect-playground](https://github.com/jpcima/rc-effect-playground) references, level-matched.
- Dimension absolute delays and rates: only relative values are verified; the SDD-320 service notes are
  the next source.
- CE-2 centre delay and depth by ear (the analysis gives the chip's range, not the pedal's sweep).
- Ensemble slow-LFO rate: sources disagree (0.18 – 0.8 Hz).
- Guitar Drive with the Clean engine vs BBD with zero noise.
