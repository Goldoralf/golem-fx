# gfx-reverb

> **Status: PROPOSED** — design, no code yet.

golem's reverb. Stands for three golem blocks (today Dragonfly Hall, Plate and Room), keeping their
parameter ids and units ([golem contract](../docs/golem-contract.md)):

| golem block | Today | Catalogue ids |
|---|---|---|
| `reverb` | Dragonfly Hall (Freeverb3 early reflections + zrev2 FDN) | `mix` 0 – 60 %, `decay` 0.4 – 4 s, `size` 10 – 50 m, `hicut` 3 – 12 kHz |
| `reverb-plate` | Dragonfly Plate | `mix` 0 – 60 %, `decay` 0.3 – 3 s, `predelay` 0 – 40 ms, `hicut` 3 – 12 kHz |
| `reverb-room` | Dragonfly Room (early reflections + Progenitor allpass loop) | `mix` 0 – 60 %, `decay` 0.3 – 2.5 s, `size` 8 – 24 m, `hicut` 3 – 12 kHz |

The ids share their units across the three blocks, so **one plugin** covers them, with an `algo`
parameter golem pins per block (to agree; fallback: three VST3 classes with `algo` fixed). The shimmer is
[gfx-shimmer](../gfx-shimmer/), built on this engine; the spring stays golem's `surge-spring`.

The aim: dense, smooth tails with no metallic ring, accurate decay per band, the two sounds people
recognise (FDN hall / room, Dattorro plate), and the live features — trails, freeze, ducking — on a CPU
budget that allows several instances.

## 1. What players use

golem's 16 presets stay moderate: hall decay 0.8 – 4 s, size 14 – 50 m, hicut 3.8 – 11 kHz, mix 20 – 40 %;
plate decay 0.8 – 2.4 s, predelay 0 – 25 ms, mix 20 – 30 %; room decay 0.5 – 1.3 s, size 8 – 16 m, mix
18 – 30 %. Nobody uses extreme settings.

## 2. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference or not re-checked (valhalladsp.com
refused automated reading; its design notes come from search excerpts).

### Commercial references

| Product | What it does best | What we take |
|---|---|---|
| Lexicon 224 / 480L / PCM ([PCM92 manual](https://www.manualslib.com/manual/258129/Lexicon-Pcm92.html?page=45)) [V] | Griesinger tanks; Random Hall; **Spin and Wander** randomisers against colouration | Slow random-walk modulation |
| Valhalla VintageVerb / Room / Plate ([blog](https://valhalladsp.com/2011/01/21/reverbs-diffusion-allpass-delays-and-metallic-artifacts/)) [V excerpts] | Few macros over deep modes; modulated allpasses against metallic artefacts; "a little noise isn't bad" | Macros over hidden depth |
| [FabFilter Pro-R 2](https://www.fabfilter.com/products/pro-r-2-reverb-plugin) [V] | Six-band **decay rate EQ**, distance, character, ducking, freeze | Decay per band, ducking, freeze |
| [Strymon BigSky](https://www.strymon.net/product/bigsky/) [V] | Room / Hall / Plate / Spring / Bloom / Cloud…; **Persist** (trails), **Infinite** (new notes add) vs **Freeze** (they don't) | Trails, freeze vs infinite |
| Bricasti M7 [I] | Realism: dense, uncoloured early decay, accurate decay per frequency | The bar for halls |
| UAD EMT 140, Relab LX480 [I] | Plate damping; an exact 480L | The bar for plates |

### Open source

| Project (licence) | Idea |
|---|---|
| [Dragonfly](https://github.com/michaelwillis/dragonfly-reverb) (GPL-3) [V] | Today's baseline: Freeverb3 early reflections + zrev2 / Progenitor / NRev / Dattorro |
| [zita-rev1](https://kokkinizita.linuxaudio.org/linuxaudio/zita-rev1-doc/quickguide.html) (GPL) [V] | 8-line FDN; damping defined as "the frequency at which RT60 is half the mid value" — a musical `hicut`; its controls are not smoothed (what to avoid) |
| [CloudSeedCore](https://github.com/GhostNoteAudio/CloudSeedCore) (MIT) [V] | Predelay, multitap early reflections, diffuser, up to 12 modulated lines with shelves — **delay sets from integer seeds** (deterministic "randomness") |
| [Signalsmith, *Let's write a reverb*](https://signalsmith-audio.co.uk/writing/2021/lets-write-a-reverb/) [V] | Multichannel diffuser steps (delay, shuffle, polarity, **Hadamard**), each multiplying echo density; then a **Householder** FDN; modulate only some lines. Re-implement from the article (the example code has no licence) |
| [Airwindows](https://github.com/airwindows/airwindows) (MIT) [V] | Galactic, Verbity, kPlate, kCathedral, kGuitarHall: cheap, characterful |
| Mutable Instruments Clouds / Rings (MIT) [I] | A compact Dattorro tank for a microcontroller: a cheap plate reference |

### Papers

- Jot & Chaigne, *Digital Delay Networks for Designing Artificial Reverberators*, AES 1991 [I]: per-line
  **absorption filters** scaled by delay length — the decay holds whatever the lengths, so size can move
  without changing decay.
- Dattorro, *Effect Design Part 1*, JAES 1997 ([PDF](https://ccrma.stanford.edu/~dattorro/EffectDesignPart1.pdf)):
  Griesinger's plate tank, full delay tables, modulated tank allpasses.
- Schlecht & Habets, *Time-Varying Feedback Matrices in FDNs*, JASA 2015 ([doi](https://doi.org/10.1121/1.4928394))
  [V]: modulating the matrix instead of the delays — no pitch chorusing.
- Schlecht & Habets, *Accurate Reverberation Time Control in FDNs*, DAFx 2017; Prawda, Schlecht & Välimäki,
  *Improved Reverberation Time Control for FDNs*, DAFx 2019 ([DAFx](https://dafx.de/paper-archive/details/JbPmKKJc3yrN_fd36-MBoQ))
  [V]: octave attenuation filters; ~5 % RT error can be audible.
- Välimäki, Parker, Savioja, Smith & Abel, *Fifty Years of Artificial Reverberation*, IEEE TASLP 2012.

### What we conclude

1. **Two tail topologies**: an FDN-16 (hall, room, chamber, cathedral) and a Dattorro tank (plate).
2. **Density first**: a Hadamard diffuser before the tail, so it is dense within ~20 ms on a pick attack.
3. **Modulation that kills the metallic ring without chorus**: slow seeded random walks on some lines
   (Lexicon), optionally a time-varying matrix (Schlecht).
4. **Decay per band** with Jot attenuation filters; `hicut` = damping, zita's definition.
5. **Live**: predelay, ducking, freeze / infinite, trails.
6. Skipped: convolution (latency or no modulation / freeze), more than 3 decay bands (v2), scattering delay
   networks, spring (surge-spring), shimmer (gfx-shimmer), gated / reverse, oversampling.

## 3. Design

```
in ─┬──────────────────────────────────────────────────────────────── dry (× kill_dry) ──┐
    └▶ ×on ▶ low cut ▶ predelay ▶ early reflections ─────────────────┐                    │
                                └▶ diffuser (Hadamard, 8 ch × 4) ▶ tail ─┴▶ width ▶ × mix × duck ┴▶ out
                                   tail = FDN-16 (Householder, Jot 3-band attenuation, seeded random-walk mod)
                                        or Dattorro tank (Plate)
```

| `algo` | Tail | Early reflections | Modulation | Character |
|---|---|---|---|---|
| Hall | FDN-16 | sparse, late | medium | Long, smooth, wide |
| Plate | Dattorro tank | none | tank allpasses | Instant density, bright |
| Room | FDN-16, short lines | dense, early | low | Short, close |
| Chamber | FDN-16 | dense | low | Between room and hall |
| Cathedral | FDN-16, long lines | sparse, very late | medium | Very long |

- **Decay**: per-line low shelf + high shelf + damping low-pass from `hicut`, gains recomputed from the
  current delay lengths; bass decay set by `bass_mult` around `bass_xover`.
- **Size**: a slewed target (300 – 500 ms) for the delay lengths, read through the modulation interpolators —
  a slight glide in the tail, never a click; the decay stays put. Buffers sized at initialisation for the
  largest size at the highest sample rate.
- **Stereo**: decorrelated output taps; mono sum checked for combing.
- **Freeze**: input muted, attenuation 1. **Infinite**: new notes still enter, the tail never decays.
- **Trails**: `on` is the bypass parameter (`make_bypass`): the tail rings out.
- **Determinism** (golem compares REAPER and Ardour renders by null test): every random source from a fixed
  seed, reset on `reset()`; coefficients recomputed on a fixed 16-sample grid counted from reset, smoothing
  per sample — so the output does not depend on the host's block size.
- **CPU**: target ≤ 2 % of one core per instance at 48 kHz; the 16 lines processed as SIMD lanes.

## 4. Parameters

Catalogue ids kept; ranges wider; no negative minimum.

| id | Name | Type | Range | Catalogue | Default | EXP | Role |
|---|---|---|---|---|---|---|---|
| `mix` | Mix | f | 0 – 100 % | 0 – 60 | 30 % | ✓ | Wet level, dry at unity |
| `decay` | Decay | f | 0.2 – 20 s | 0.3 – 4 | 1.3 s | ✓ | Mid-band RT60 |
| `size` | Size | f | 5 – 60 m | 8 – 50 | per algo | | Slewed |
| `predelay` | Predelay | f | 0 – 250 ms | 0 – 40 | per algo | | |
| `hicut` | High Cut | f | 1 – 20 kHz | 3 – 12 | 7 kHz | ✓ | Damping (RT60 halves there) + a gentle output low-pass |
| `algo` | Algorithm | e | Hall · Plate · Room · Chamber · Cathedral | new | Hall | | Pinned by golem per block |
| `lowcut` | Low Cut | f | 20 – 500 Hz | new | 80 Hz | | High-pass into the reverb |
| `bass_mult` | Bass Decay | f | 0.5 – 2 × | new | 1 × | | Low-band RT60 ratio |
| `bass_xover` | Bass Crossover | f | 100 – 1,000 Hz | new | 300 Hz | | |
| `mod` | Modulation | f | 0 – 100 % | new | per algo | | Depth; rates fixed per algo |
| `diffusion` | Diffusion | f | 0 – 100 % | new | 80 % | | |
| `early` | Early Reflections | f | 0 – 100 % | new | per algo | | Ignored on Plate |
| `width` | Width | f | 0 – 150 % | new | 100 % | | |
| `duck` | Ducking | f | 0 – 24 dB | new | 0 dB | ✓ | |
| `duck_sens` | Duck Sensitivity | f | 0 – 100 % | new | 50 % | | Instead of a negative threshold |
| `duck_release` | Duck Release | f | 50 – 2,000 ms | new | 400 ms | | |
| `freeze` | Freeze | e | Off · Freeze · Infinite | new | Off | ✓ | |
| `kill_dry` | Kill Dry | b | off / on | new | off | | |
| `on` | On | b | off / on | new | on | | Bypass parameter with trails |
| `trails` | Trails | b | off / on | new | on | | |

## 5. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| RT60 per octave | within ±10 % of target at decay 0.5, 2 and 8 s, every algo |
| Echo density | normalised echo density reaches 1 within 30 ms (Plate), 80 ms (Hall) |
| Size sweep | no click; RT60 unchanged |
| Mono sum | spectrally flat within a bound (no comb) |
| Freeze | ±0.5 dB over 30 s |
| Trails | `on` off: the tail decays naturally |
| Host independence | identical output at 64- and 512-sample blocks; strict null REAPER vs Ardour |
| CPU | ≤ 2 % of one core per instance at 48 kHz |
| Listening | golem's 16 reverb presets on clean, lead-clean and keys, blind against Dragonfly |
