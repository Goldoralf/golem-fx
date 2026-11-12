# gfx-chorus — voicing

> **Status: PROPOSED** — design, no code yet. Every number below is a **starting point**, taken from
> published descriptions of the reference units (Boss CE-2, Roland Juno-60, Roland Dimension D, string
> ensembles) and from common practice. They are tuned by ear on the listening bench; this document is
> updated with the values that survive.

This page fills the **P** values of [PARAMS.md](PARAMS.md).

## 1. Who sets what

Two layers, never mixed:

- **The mode** sets the **shape of the modulation**: voice count, centre delay, excursion, LFO shape, phase
  between voices, cross-mix, default rate. A Dimension sounds like a Dimension whatever the instrument.
- **The profile** sets **everything else**: crossover, wet filters, colour engine and amounts, stereo,
  mix, the default mode, and a **scale** applied to the mode's default depth.

So 5 modes × 5 profiles give 25 voicings from two tables of 5 columns, not from 25 presets.

When a group is **Custom**, its own controls replace the layer's anchors for that group only.

## 2. Mode anchors (Modulation group)

| | Classic | Dual | Dimension | Ensemble | Doubler |
|---|---|---|---|---|---|
| Reference | Boss CE-2 | Juno-60 I / II | Roland Dimension D | String ensembles (Solina) | Studio double-tracking |
| `voices` | 1 | 2 | 2, complementary | 3 (expert: 3–6) | 2 |
| `delay_base` | 7 ms | 3.5 ms | 6 ms | 10 ms | 18 ms |
| `depth` default | 3 ms | 1.8 ms | 1 ms | 2.5 ms | 0.8 ms |
| `lfo_shape` | Triangle | Juno | Rounded | Sine | Random |
| `voice_spread` | — | 180° | 180° | 120° (360° / voices) | — (independent random) |
| `crossmix` | 0 % | 0 % | 50 % | 0 % | 0 % |
| `rate` default | 0.8 Hz | 0.5 Hz (Juno I; Juno II ≈ 0.8 Hz) | 0.4 Hz | 0.6 Hz | 0.25 Hz |

Built into the mode, not exposed as parameters:

- **Dimension**: the two voices are modulated in antiphase and each output carries one voice plus the
  other one inverted and attenuated (`crossmix`). The pitch movements cancel out in part, which is why a
  Dimension sounds wide but not seasick.
- **Ensemble**: a second, fast and shallow LFO (≈ 6 Hz, ≈ 10 % of the excursion) is added on top of the
  slow one, as string ensembles do; it gives the shimmer.
- **Doubler**: each voice follows its own smoothed random path (no periodic LFO), so nothing repeats.

## 3. Profile anchors

| Group | Parameter | Guitar Clean | Guitar Drive | Bass | Keys | Voice |
|---|---|---|---|---|---|---|
| — | default `mode` | Classic | Dimension | Classic | Dual | Doubler |
| Modulation | default depth scale (× mode's `depth`) | 1.0 | 0.7 | 0.8 | 1.0 | 0.6 |
| Modulation | `drift` | 10 % | 15 % | 5 % | 5 % | 60 % |
| Crossover | `xover_on` | off | on | on | on | off |
| Crossover | `xover_freq` | (120 Hz) | 150 Hz | 200 Hz | 100 Hz | (120 Hz) |
| Color | `engine` | BBD | Clean | BBD | BBD | Clean |
| Color | `drive` | 25 % | 5 % | 30 % | 20 % | 0 % |
| Color | `bbd_bw` | 6 kHz | (8 kHz) | 5 kHz | 8 kHz | (10 kHz) |
| Color | `compander` | 40 % | 15 % | 40 % | 60 % | 0 % |
| Color | `noise` | 15 % | 0 % | 10 % | 40 % | 0 % |
| Filters | `wet_hpf` | 80 Hz | 250 Hz | 40 Hz | 60 Hz | 150 Hz |
| Filters | `wet_lpf` | 8 kHz | 5 kHz | 4 kHz | 10 kHz | 12 kHz |
| Stereo | `stereo_phase` | 90° | 180° | 90° | 180° | 90° |
| Stereo | `width` | 100 % | 100 % | 60 % | 100 % | 70 % |
| Stereo | `mono_safe` | off | on | on | off | on |
| Output | `mix` default | 50 % | 35 % | 50 % | 50 % | 30 % |
| Output | `mix_law` | Linear | Equal Power | Linear | Linear | Equal Power |

Values in parentheses are used only if the user switches that part on in a Custom group (e.g. a BBD
engine on the Voice profile).

Why each profile looks like this:

- **Guitar Clean** — the generous, coloured pedal chorus: BBD engine, a dark wet, some hiss, a linear
  mix for the deep phase notches that make a CE-2 sound like a CE-2.
- **Guitar Drive** — the chorus sits after the distortion and must not blur it: clean engine (any noise
  or fizz is already loud), tight lows (crossover at 150 Hz and a 250 Hz wet low-cut), a dark wet to
  keep the fizz out, Dimension for width without pitch wobble, a lower mix.
- **Bass** — the lows are the instrument: below 200 Hz the signal stays dry and mono, only the mids and
  highs move. Narrower stereo, mono-safe, a dark BBD wet (as bass chorus pedals do).
- **Keys** — the Juno: BBD, strong compander, audible hiss — the noise is part of the sound.
- **Voice** — a discreet double: no colour at all, random drift instead of a periodic wobble, a low mix,
  and a 150 Hz wet low-cut so the double does not thicken the chest of the voice.

## 4. Warmth (front panel)

Warmth 50 % = the anchors above. 0 % moves towards clean and bright, 100 % towards dark and coloured.
Each profile moves only what makes sense for it:

| Profile | `wet_lpf` | `bbd_bw` | `drive` | `compander` | `noise` |
|---|---|---|---|---|---|
| Guitar Clean | +1 → −1 oct | +0.5 → −0.5 oct | × 0 → × 2 | × 0.5 → × 1.5 | × 0.5 → × 1.5 |
| Guitar Drive | +0.5 → −1 oct | — | × 0 → × 2 | × 0.5 → × 1.5 | — |
| Bass | +0.5 → −1 oct | +0.5 → −0.5 oct | × 0 → × 2 | × 0.5 → × 1.5 | × 0.5 → × 1.5 |
| Keys | +1 → −1 oct | +0.5 → −0.5 oct | × 0 → × 2 | × 0.5 → × 1.5 | × 0.5 → × 2 |
| Voice | +0.5 → −0.5 oct | — | — | — | — |

Read "+1 → −1 oct" as: at 0 % the cut-off is one octave above its anchor, at 100 % one octave below.
Octave shifts are interpolated linearly in octaves, multipliers linearly; results are clamped to the
parameter's range.

## 5. Depth (front panel)

Depth is the excursion itself, in ms (golem catalogue id `chorus.depth`). Its default is the mode's
`depth` × the profile's scale, clamped to the parameter's range, 0.5 – 10 ms (e.g. Guitar Drive in
Dimension: 1 ms × 0.7 = 0.7 ms). The range uses a log curve, so the subtle settings get most of the knob's travel.

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

- Every number in §2 and §3 against reference recordings, blind, level-matched.
- The Dimension cross-mix ratio: 50 % is a guess; the original's balance is to be measured.
- Juno I / II rates: commonly quoted around 0.5 and 0.8 Hz; check against a recording.
- Guitar Drive with the Clean engine: try BBD with zero noise as an alternative.
