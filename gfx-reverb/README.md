# gfx-reverb

> **Status: CODED** (v0.1.0, 2026-11-12) — all eleven algorithms on gfx-dsp's reverb engine (Hadamard diffuser, FDN-16 with Jot 3-band attenuation and seeded random-walk modulation, Dattorro tank with an extra all-pass per half, dispersive springs, tap bursts); shimmer insert (bounded by construction), freeze / infinite, ducking, swell, era, trails; RT60 within −4 … +2 % (FDN) and +12 % (plate at 8 s) of the setting; 6 behaviour tests, bench green. Choices made in code: delay lengths glide to whole-sample targets and are read at whole samples once settled (an interpolator in a loop shortens the decay); `era` runs the tail at the period's rate (80s 31.25 kHz, 70s 20 kHz) through 4-pole anti-alias and Catmull-Rom reconstruction converters, with 12 / 10-bit input and 1.5× modulation on 70s, at the same RT60 and level as Modern (2026-11-13; Modern bit-identical to before, 70s 28 – 35 % cheaper); the plate's input bandwidth filter was inverted until 2026-11-13 (−60 dB: the plate was nearly silent), fixed and level-matched to the hall; the shimmer feed is scaled by 1 / √(1 + decay) and soft-limited; the plate uses four output taps per side; CPU 525 ns / sample at 48 kHz (2.5 % of a core, above the 2 % target).

golem's reverb. Stands for three golem blocks (today Dragonfly Hall, Plate and Room), keeping their
parameter ids and units ([golem contract](../docs/golem-contract.md)):

| golem block | Today | Catalogue ids |
|---|---|---|
| `reverb` | Dragonfly Hall (Freeverb3 early reflections + zrev2 FDN) | `mix` 0 – 60 %, `decay` 0.4 – 4 s, `size` 10 – 50 m, `hicut` 3 – 12 kHz |
| `reverb-plate` | Dragonfly Plate | `mix` 0 – 60 %, `decay` 0.3 – 3 s, `predelay` 0 – 40 ms, `hicut` 3 – 12 kHz |
| `reverb-room` | Dragonfly Room (early reflections + Progenitor allpass loop) | `mix` 0 – 60 %, `decay` 0.3 – 2.5 s, `size` 8 – 24 m, `hicut` 3 – 12 kHz |

The ids share their units across the three blocks, so **one plugin** covers them, with an `algo`
parameter golem pins per block at insert (agreed with the rack session: `init` in the Golem Engine table,
`FX_INIT` under REAPER — reverb = Hall, reverb-plate = Plate, reverb-room = Room, never touched again). **Shimmer in two places, on purpose.** gfx-reverb has a simple shimmer (`shimmer`, `shimmer_pitch`): a
colour added to any of its algorithms — a plate with an octave halo, a shimmering spring, a rising Cloud.
[gfx-shimmer](../gfx-shimmer/) is the effect whose subject is the shimmer: two detuned voices, shifter
placement, a freeze that stops the climb, glide, reverse grains; it stands for golem's `reverb-shimmer`
block, whose `mix` is a crossfade (here `mix` is a wet level, so one plugin cannot be both). Same engine,
same code.

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

1. **Two tail topologies**: an FDN-16 (hall, room, chamber, cathedral) and a Dattorro-family tank (plate).
   The published Dattorro tank is thin and slightly metallic at long decays: ours adds diffusers in the
   loop, and the bench compares it with an FDN voiced as a plate before the Plate algorithm is frozen.
2. **Density first**: a Hadamard diffuser before the tail, so it is dense within ~20 ms on a pick attack.
3. **Modulation that kills the metallic ring without chorus**: slow seeded random walks on some lines
   (Lexicon), optionally a time-varying matrix (Schlecht).
4. **Decay per band** with Jot attenuation filters; `hicut` = damping, zita's definition.
5. **Live**: predelay, ducking, freeze / infinite, trails.
6. Skipped: convolution (latency or no modulation / freeze), more than 3 decay bands (v2), scattering delay
   networks, shimmer (gfx-shimmer), oversampling.
7. **Full coverage is the goal** (BigSky has 12 machines, VintageVerb 22 algorithms): beyond the five core
   algorithms, the design specifies Ambience, Spring, Bloom, Cloud, Gated and Reverse (§3). They share the
   engine's blocks (diffuser, FDN, tank, envelope), so each is a voicing and a routing, not a new plugin.
   Built in that order after the core five; Spring can take over golem's `surge-spring` block later.

## 3. Design

```
in ─┬──────────────────────────────────────────────────────────────── dry (× kill_dry) ──┐
    └▶ ×on ▶ low cut ▶ predelay ▶ early reflections ─────────────────┐                    │
                                └▶ diffuser (Hadamard, 8 ch × 4) ▶ tail ─┴▶ width ▶ × mix × duck ┴▶ out
                                   tail = FDN-16 (Hadamard or Householder — bench, Jot 3-band attenuation, seeded random-walk mod)
                                        or Dattorro tank (Plate)
```

| `algo` | Tail | Early reflections | Modulation | Character |
|---|---|---|---|---|
| Hall | FDN-16 | sparse, late | medium | Long, smooth, wide |
| Plate | Dattorro tank | none | tank allpasses | Instant density, bright |
| Room | FDN-16, short lines | dense, early | low | Short, close |
| Chamber | FDN-16 | dense | low | Between room and hall |
| Cathedral | FDN-16, long lines | sparse, very late | medium | Very long |
| Ambience | early reflections + a very short diffuse tail | dense, the main signal | none | "Not a reverb": air and width around a dry amp sim (decay 0.1 – 0.6 s) |
| Spring | 1 – 3 parallel spring lines: dispersive allpass chains (the "drip" chirp), low-passed feedback, slight random modulation ([Välimäki, Parker & Abel 2010](https://www.aes.org/e-lib/browse.cfm?elib=15310) [I link]) | none | spring wobble | Fender tank; `drive` = dwell |
| Bloom | diffuser fed back on itself before the FDN | none | medium | The tail **fades in** (0.2 – 2 s) then decays: pads behind the note |
| Cloud | FDN-16 with very long lines and heavy diffusion (Supermassive / Blackhole territory) | none | high | Huge, slow, enveloping; decay up to 60 s |
| Gated | diffuser + early taps with a flat envelope cut after `size` | the tail itself | none | 80s gated snare / guitar stabs; non-decaying burst |
| Reverse | the same taps with a rising envelope, ending on the beat | the tail itself | none | Swell-in before silence; `predelay` = length, syncable |

What the shared controls mean per algorithm:

| Control | Hall · Room · Chamber · Cathedral · Cloud | Plate | Spring | Bloom | Gated · Reverse | Ambience |
|---|---|---|---|---|---|---|
| `decay` | RT60 | RT60 | RT60 of the springs | RT60 after the bloom | — (length is `size`) | 0.1 – 0.6 s |
| `size` | room dimension (delay scale) | plate size | spring length (chirp rate) | bloom time | burst length (50 – 600 ms, or `sync`) | space of the reflections |
| `diffusion` | smoothness of the onset | density | drip amount | how slow the fade-in | grain of the burst | density |
| `early` | reflections level | — | — | — | — | the effect itself |
| `drive` | — | — | dwell (how hard the springs are hit) | — | — | — |

- **Decay**: per-line low shelf + high shelf + damping low-pass from `hicut`, gains recomputed from the
  current delay lengths; bass decay set by `bass_mult` around `bass_xover`.
- **Size**: a slewed target (300 – 500 ms) for the delay lengths, read through the modulation interpolators —
  a slight glide in the tail, never a click; the decay stays put. Buffers sized at initialisation for the
  largest size at the highest sample rate.
- **Stereo**: decorrelated output taps; mono sum checked for combing.
- **Shimmer**: gfx-shimmer's regeneration insert with one voice — tail output → loop low-pass (clamped to
  0.45 · sr / ratio, anti-alias) → delay-line shifter (equal-power heads, seeded jitter) → back into the
  tail's input; loop gain < 1 by construction; reported latency 0 (the shifter's delay is part of the tail).
  At `shimmer` 0 the insert is bypassed: no CPU, bit-identical to the reverb without it.
- **Freeze**: input muted, attenuation 1, **shimmer out of the loop** (otherwise the frozen tail would
  climb forever). **Infinite**: new notes still enter, the tail never decays.
- **Trails**: `on` is the bypass parameter (`make_bypass`): the tail rings out.
- **Determinism** (golem compares REAPER and Ardour renders by null test): every random source from a fixed
  seed, reset on `reset()`; coefficients recomputed on a fixed 16-sample grid counted from reset, smoothing
  per sample — so the output does not depend on the host's block size.
- **CPU**: target ≤ 2 % of one core per instance at 48 kHz; the 16 lines processed as SIMD lanes.

## 4. Parameters

Catalogue ids kept; ranges wider; no negative minimum.

| id | Name | Type | Range | Catalogue | Default | EXP | Role |
|---|---|---|---|---|---|---|---|
| `mix` | Mix | f | 0 – 100 % | 0 – 60 | 30 % | ✓ | Wet level, dry at unity (catalogue law). 100 % with `kill_dry` for a parallel lane |
| `decay` | Decay | f | 0.1 – 60 s | 0.3 – 4 | 1.3 s | ✓ | Mid-band RT60 |
| `size` | Size | f | 5 – 60 m | 8 – 50 | 24 m | | Slewed. golem pins it at insert for blocks that do not expose it (plate: 15 m) |
| `predelay` | Predelay | f | 0 – 250 ms | 0 – 40 | 0 ms | | golem pins it at insert for blocks that do not expose it (hall: 15 ms, room: 5 ms) |
| `hicut` | High Cut | f | 1 – 20 kHz | 3 – 12 | 7 kHz | ✓ | Damping (RT60 halves there) + a gentle output low-pass |
| `algo` | Algorithm | e | Hall · Plate · Room · Chamber · Cathedral · Ambience · Spring · Bloom · Cloud · Gated · Reverse | new | Hall | | Pinned by golem per block (the first three are today's blocks) |
| `lowcut` | Low Cut | f | 20 – 500 Hz | new | 80 Hz | | High-pass into the reverb |
| `bass_mult` | Bass Decay | f | 0.5 – 2 × | new | 1 × | | Low-band RT60 ratio |
| `bass_xover` | Bass Crossover | f | 100 – 1,000 Hz | new | 300 Hz | | |
| `mod` | Modulation | f | 0 – 100 % | new | 50 % | | 50 % = the algorithm's own depth; 0 = static (may ring); 100 = lush, towards chorus |
| `diffusion` | Diffusion | f | 0 – 100 % | new | 80 % | | |
| `early` | Early Reflections | f | 0 – 100 % | new | 50 % | | 50 % = the algorithm's own level; ignored on Plate |
| `width` | Width | f | 0 – 150 % | new | 100 % | | |
| `era` | Era | e | Modern · 80s · 70s | new | Modern | | The converter and the modulation of the period (VintageVerb's "Color"): the tail runs at a reduced internal rate behind 4-pole converters. 80s = 31.25 kHz (Yamaha REV7 / SPX90), 12 kHz band, 12-bit input; 70s = 20 kHz (Lexicon 224), 8 kHz band, 10-bit input, 1.5× modulation. Early reflections and bursts stay at the host's rate. The reconstruction leaves faint images above the band (the grain). Switching dips through the algorithm gate and restarts the tail. Also cheaper (70s −30 %) |
| `mod_rate` | Modulation Rate | f | 0 – 100 % | new | 50 % | | 50 % = the algorithm's own (Lexicon "spin"); `mod` is the depth ("wander") |
| `high_mult` | High Decay | f | 0.1 – 1 × | new | 1 × | | High-band RT60 ratio above `hicut` / 2: with `bass_mult`, a three-band decay |
| `wet_low` | Wet Low Shelf | f | 0 – 100 % (50 = flat) | new | 50 % | | ±12 dB at 200 Hz on the wet |
| `wet_high` | Wet High Shelf | f | 0 – 100 % (50 = flat) | new | 50 % | | ±12 dB at 4 kHz on the wet: tone of the reverb without changing its decay |
| `swell` | Swell | f | 0 (off) – 2,000 ms | new | 0 | | Auto volume swell **into** the reverb: the pick attack is removed from the wet (pad / violin effect) |
| `drive` | Drive | f | 0 – 100 % | new | 0 % | | Saturation into the tank; on Spring = dwell |
| `springs` | Springs | e | 1 · 2 · 3 | new | 2 | | Spring |
| `sync` | Sync | e | contract's list | new | Off | | Predelay (and Gated / Reverse length) as a note value |
| `shimmer` | Shimmer | f | 0 – 100 % | new | 0 % | ✓ | Share of the tail fed back through a pitch shifter (gfx-shimmer's regeneration insert, one voice): each pass climbs by `shimmer_pitch`. Works on every algorithm with a tail (ignored on Gated, Reverse, Ambience) |
| `shimmer_pitch` | Shimmer Pitch | f | −12 – +24 st | new | +12 st | | Octave by default; +7 fifths, +19, +24, −12 |
| `duck` | Ducking | f | 0 – 24 dB | new | 0 dB | ✓ | |
| `duck_sens` | Duck Sensitivity | f | 0 – 100 % | new | 50 % | | Instead of a negative threshold |
| `duck_release` | Duck Release | f | 50 – 2,000 ms | new | 400 ms | | |
| `freeze` | Freeze | e | Off · Freeze · Infinite | new | Off | ✓ | Freeze = the tail holds, new notes stay out; Infinite = the tail holds and new notes add to it |
| `kill_dry` | Kill Dry | b | off / on | new | off | | |
| `on` | On | b | off / on | new | on | | Bypass parameter with trails |
| `trails` | Trails | b | off / on | new | on | |
| `dry` | Dry | f | −60 – 0 dB | new | 0 dB | | Dry level. golem pins −1.9 dB at insert on its three reverb blocks (Dragonfly's 80 % dry); approved by Raph, 2026-11-12 | |

## 5. Applications

Layouts: stereo 2 / 2 by default, mono 1 / 1 offered (the tail summed).

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Guitar "amp in a room" | Room, decay 0.7 s, size 12 m, mix 25 %, hicut 7 kHz | Dense early reflections: the dry amp sim gets air |
| Lead hall | Hall, decay 2.2 s, predelay 40 ms, mix 30 %, `duck` 6 dB | Predelay + ducking: the solo stays in front, the tail blooms in the gaps |
| Clean arpeggios, lush | Hall, decay 3 s, `mod` 70 %, hicut 6 kHz | Modulated tail without metallic ring |
| Vocal plate | Plate, decay 1.4 s, predelay 20 ms, `lowcut` 150 Hz, hicut 8 kHz | Instant density; low cut keeps the voice clear |
| Snare / drum plate | Plate, decay 1 s, mix 25 % | |
| Ambient pad under playing | Cathedral, decay 8 s, mix 45 %, `bass_mult` 0.7, `lowcut` 200 Hz | Bass decay below the mids: no mud at long decays |
| Freeze a chord and solo over it | `freeze` Freeze on a footswitch | |
| Ever-growing wash | `freeze` Infinite | New notes keep entering |
| Bass, a touch of space | Room, decay 0.5 s, mix 12 %, `lowcut` 250 Hz | Only the highs reverberate |
| Keys / piano | Chamber, decay 1.5 s, `mod` 30 % | Low modulation: no pitch wobble on sustained piano |
| Scene change, tail rings out | `on` off, `trails` on | |
| Parallel lane | `kill_dry` on, mix 100 % | |
| Surf / spaghetti western | Spring, `springs` 2, decay 2 s, `drive` 50 %, `diffusion` 70 %, mix 35 %, before or after the amp | Drip from the dispersive chains; dwell |
| Air around a dry amp sim, always on | Ambience, `size` 10 m, mix 20 % | Early reflections only: space without a tail |
| Pads that swell behind the playing | Bloom, `size` 30 m, decay 6 s, `swell` 400 ms, mix 50 % | Fade-in tail + attack removed |
| Post-rock wall / drone | Cloud, decay 30 s, `mod` 70 %, `duck` 6 dB, `lowcut` 250 Hz | Huge tail kept behind the playing |
| 80s gated stabs | Gated, `sync` 1/8, `diffusion` 80 %, mix 60 % | Flat burst, cut on the beat |
| Reverse swell into the note | Reverse, `sync` 1/4, mix 70 %, `kill_dry` optional | Rising envelope ending on the beat |
| Vintage digital hall (80s ballad lead) | Hall, `era` 80s, decay 2.8 s, `mod` 60 % | Reduced bandwidth and period modulation |
| Dark, warm 70s hall | Hall, `era` 70s, `wet_high` 35 % | |
| Bright tail that does not last in the highs | `wet_high` 65 %, `high_mult` 0.5 | Tone and decay are separate controls |
| Plate with an octave halo (clean arpeggios) | Plate, decay 2 s, `shimmer` 25 %, hicut 7 kHz | A colour on a normal reverb |
| Shimmering spring | Spring, `shimmer` 20 %, `shimmer_pitch` +12 | Only possible here |
| Rising Cloud | Cloud, decay 20 s, `shimmer` 45 %, `duck` 6 dB | |
| Fifths under a hall | Hall, `shimmer` 30 %, `shimmer_pitch` +7 | |
| Full shimmer instrument (two voices, placement, glide) | → gfx-shimmer | |

## 6. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| RT60 per octave | within ±10 % of target at decay 0.5, 2 and 8 s, every decaying algo; `bass_mult` and `high_mult` ratios within ±10 % |
| Spring | chirp visible in the spectrogram (dispersion curve as designed); no metallic ring on a pick attack |
| Gated / Reverse | burst length = `size` or the synced note value ± 1 ms; ends on the beat when synced |
| Bloom | onset reaches −3 dB of its peak after the bloom time ± 15 % |
| Echo density | normalised echo density reaches 1 within 30 ms (Plate), 80 ms (Hall) |
| Size sweep | no click; RT60 unchanged |
| Mono sum | spectrally flat within a bound (no comb) |
| Freeze | ±0.5 dB over 30 s; no climb with `shimmer` > 0 |
| Shimmer | `shimmer` 0: bit-identical to the reverb without the insert; `shimmer` 100 %, +12, decay 20 s, 60 s of sweep: bounded; first pass +12 st ± 2 cents |
| Trails | `on` off: the tail decays naturally |
| Host independence | identical output at 64- and 512-sample blocks; strict null REAPER vs Ardour |
| CPU | ≤ 2 % of one core per instance at 48 kHz |
| Listening | golem's 16 reverb presets on clean, lead-clean and keys, blind against Dragonfly |
