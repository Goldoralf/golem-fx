# gfx-delay

> **Status: PROPOSED** — design, no code yet.

golem's delay. Stands for golem's `delay` block (today ReaDelay, tempo-synced), keeping its parameter ids
and units ([golem contract](../docs/golem-contract.md)). The aim is the best live delay: three canonical
engines (digital, tape, bucket-brigade), a colourful but safe feedback loop, ducking so the repeats never
clutter the playing, and real trails.

## 1. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference or not re-checked (several vendor
sites refused automated reading).

### Commercial references

| Product | What it does best | What we take |
|---|---|---|
| [Soundtoys EchoBoy](https://www.soundtoys.com/product/echoboy/) [V] | 30 styles (EchoPlex, Space Echo, Memory Man, DM-2…), each a bundle of loop colour, modulation and saturation; Single / Dual / Ping-Pong / Rhythm | An engine = a bundle of loop colour + modulation + saturation |
| [TC 2290](https://www.soundonsound.com/reviews/tc-electronic-2290-p) [V] | "Dynamic delay": the repeats duck under the playing and bloom between phrases | **Ducking**, the biggest live win |
| [Strymon El Capistan](https://strymon.net/elcapistan) [V] / [Timeline](https://www.strymon.net/product/timeline/) [I] | Low End Contour (high-pass on the repeats), tape wear that scales with speed; dTape, dBucket; spillover | Low-cut in the loop; trails |
| [Valhalla Delay](https://valhalladsp.com/shop/delay/valhalla-delay/) [I] | Tape / BBD / Digital / Ghost / Pitch modes driven by a few macros (Era, Age) | Few macros over engine internals |
| [FabFilter Timeless 3](https://www.fabfilter.com/products/timeless-3-tape-delay-plugin) [I] | Filters in the loop, tape-style pitch glide on time change | Glide or crossfade, user's choice |
| [Line 6 DL4 MkII / Helix](https://line6.com/dl4-mkii/) [I] | Vintage Digital, Transistor Tape, Elephant Man (BBD), Ducked Delay | The three canonical engines |
| [Fractal Axe-Fx](https://www.fractalaudio.com/downloads/manuals/fas-guides/Fractal-Audio-Blocks-Guide.pdf) [I] | Mono / Stereo / Ping-Pong / Dual, ducker, drive and filters in the loop, spillover ([VP4](https://www.manualslib.com/manual/3588892/Fractal-Vp4.html?page=32) [V]) | Stereo modes, ducker |
| [Boss RE-202](https://www.boss.info/global/products/re-202/) / Echoplex EP-3 [I] | Space Echo saturation and wow; the Echoplex preamp is loved on its own | Drive before the tape |

### Open source

| Project (licence) | Idea |
|---|---|
| [Airwindows](https://github.com/airwindows/airwindows) (MIT) | TapeDelay2 (already golem's `delay-tape-aw`), PurestEcho, ToTape: cheap, characterful saturation and flutter |
| [CHOW Tape](https://github.com/jatinchowdhury18/AnalogTapeModel) (GPL-3) | Hysteresis, loss filters, wow / flutter |
| [jpcima/bbd-delay-experimental](https://github.com/jpcima/bbd-delay-experimental) (BSL-1.0) | Raffel & Smith BBD delay |
| [Surge XT](https://github.com/surge-synthesizer/surge) (GPL-3), [Vital](https://github.com/mtytel/vital) (GPL-3) | Crossfeed, filters in the loop, ping-pong variants |
| [LSP](https://github.com/lsp-plugins/lsp-plugins) (LGPL-3) | Clean, real-time-safe structure |

### Papers

- Arnardottir, Abel & Smith, *A Digital Model of the Echoplex Tape Delay*, AES 125 (2008)
  ([AES](https://www.aes.org/e-lib/browse.cfm?elib=14800)) [V]: wow and flutter as a few quasi-periodic
  components (capstan, pinch wheel) plus drift; anti-aliasing that follows the head. Deterministic by
  construction (fixed phases, seeded noise).
- Chowdhury, *Real-time Physical Modelling for Analog Tape Machines*, DAFx 2019 [V]: full hysteresis; too
  costly inside a feedback loop — a light saturator is enough here.
- Holters & Parker (DAFx 2018), Raffel & Smith (DAFx 2010): BBD, shared with gfx-chorus.
- Laakso, Välimäki et al., *Splitting the Unit Delay*, 1996: Lagrange / Hermite for a moving read head,
  never an allpass.

### What we conclude

1. Three engines cover the canon: **Digital, Tape, BBD**.
2. **Time changes** are either a **glide** (the read head slews: tape pitch bend) or a **crossfade** (two
   heads, no pitch artefact). Both are needed.
3. **The loop** carries the colour: low-cut, high-cut, soft saturation (bounded, so high feedback stays
   safe), modulation, DC blocker.
4. **Ducking** (TC 2290) and **trails** are what make a delay live-grade.
5. Skipped: multitap / rhythm, reverse, pitch / shimmer in the loop, groove / feel, tap tempo (the host
   gives the tempo), full tape hysteresis.

## 2. Design

### Signal path

```
in ─┬──────────────────────────────────────────────────────────────── dry (× kill_dry) ──┐
    └▶ ×on ▶ drive ▶ (+) ▶ write ▶ read (Lagrange, mod, glide / crossfade) ▶ low-cut ▶ high-cut
                      ▲                                                         ▶ sat ▶ DC ─┐
                      └────────────── × feedback (+ crossfeed in ping-pong) ◀───────────────┤
                                                                     wet × mix × duck ◀──────┴▶ out
```

### Engines

| `model` | Loop colour | Modulation | Time change (glide Auto) |
|---|---|---|---|
| **Digital** (default) | flat, slight top roll-off (SDE-3000 era) | very light | crossfade |
| **Tape** | head bump, high-frequency loss growing with each pass, tape saturation | wow + flutter (Echoplex model) | glide (pitch bend) |
| **BBD** | darker as the time grows (filters track the clock), compander softness, optional hiss | light, triangle | glide |

### Trails

Golem's on / off for this block goes through a parameter, not the host's bypass, so the repeats ring out:

- `on` is exported as nih-plug's bypass parameter (`make_bypass`, VST3 `kIsBypass`, inverted). **Ardour**
  then keeps the plugin running and drives that parameter instead of deactivating it [V, Ardour
  `PluginInsert::enable`]. **REAPER** stops processing a host-bypassed plugin, so golem switches gfx-delay
  by that parameter, as it already does for GoleM Freeze (`switch_param_of` in `golem_rig.lua`) —
  golem-side change, to agree.
- `on` → off: the input into the line fades out in 5 ms; the dry stays; with `trails` on the repeats
  decay naturally, with `trails` off they fade in 20 ms and the line is cleared.
- off → on: click-free.

## 3. Parameters

Catalogue ids kept; `time` keeps the catalogue's option order (options are appended at the end).

| id | Name | Type | Range | Catalogue | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `time` | Time | e | 1/4• · 1/4 · 1/8• · 1/8 · 1/8T · 1/16 · 1/2 · 1/4T · 1/16• · Free | first 6 | 1/8• | glide | | Note value at the host tempo (fallback 120 BPM); Free = `time_ms` |
| `feedback` | Feedback | f | 0 – 100 % | 0 – 75 | 35 % | 30 ms | ✓ | Loop gain; above ~90 % the saturator keeps it bounded |
| `mix` | Mix | f | 0 – 100 % | 0 – 50 | 25 % | 20 ms | ✓ | **Wet level, dry at unity** (ReaDelay's law, kept) |
| `model` | Model | e | Digital · Tape · BBD | new | Digital | crossfade | | §2 |
| `time_ms` | Time (free) | f | 20 – 2,000 ms | new | 375 ms | glide | ✓ | Used when `time` = Free |
| `glide` | Time Change | e | Auto · Glide · Crossfade | new | Auto | — | | Auto = the engine's own (table §2) |
| `glide_ms` | Glide Time | f | 10 – 1,000 ms | new | 150 ms | — | | Slew or crossfade duration |
| `low_cut` | Low Cut | f | 20 – 1,000 Hz | new | 80 Hz | 30 ms | | In the loop: each repeat thinner |
| `high_cut` | High Cut | f | 1 – 20 kHz | new | per model | 30 ms | ✓ | In the loop: each repeat darker |
| `drive` | Drive | f | 0 – 100 % | new | per model | 30 ms | | Saturation into the loop (Echoplex preamp idea) |
| `mod` | Modulation | f | 0 – 100 % | new | per model | 50 ms | | Wow / flutter / chorus depth; rates fixed per engine; deterministic |
| `duck` | Ducking | f | 0 – 24 dB | new | 0 dB | 30 ms | ✓ | Wet attenuation while playing |
| `duck_thresh` | Duck Threshold | f | −60 – 0 dB | new | −30 dB | 20 ms | | |
| `duck_release` | Duck Release | f | 50 – 2,000 ms | new | 300 ms | — | | How fast the repeats bloom back |
| `stereo` | Stereo | e | Mono · Stereo · Ping-Pong · Dual | new | Mono | crossfade | | Dual: right time = left × `ratio` |
| `ratio` | Dual Ratio | f | 0.5 – 2 × | new | 1.5 × | glide | | |
| `width` | Width | f | 0 – 100 % | new | 100 % | 30 ms | | Wet only |
| `kill_dry` | Kill Dry | b | off / on | new | off | 5 ms | | For parallel routing |
| `hold` | Hold | b | off / on | new | off | 20 ms | ✓ | Freeze: input muted, loop gain 1, saturation bypassed |
| `on` | On | b | off / on | new | on | §2 | | Bypass parameter with trails |
| `trails` | Trails | b | off / on | new | on | — | | |

"per model": the stored default is the Digital engine's; Tape and BBD shift it only while the parameter
sits at its default (same rule as `knee` in gfx-comp — to settle once for all effects).

Real-time: the line is allocated in `initialize` for 4 s at the highest sample rate, stereo (a half note
at 30 BPM). Modulation restarts at phase 0 and a fixed seed on reset.

## 4. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| Sync | at 120 BPM, each `time` value lands on its sample (± 1) |
| Tempo change | host tempo 120 → 90 mid-repeat: glide or crossfade per setting, no click |
| Feedback 100 % + drive | output bounded, no runaway, no NaN |
| Loop filters | the n-th repeat's spectrum matches n passes of low-cut / high-cut |
| Ducking | wet −`duck` dB while playing, back within `duck_release` |
| Trails | `on` off: repeats continue (trails) or fade in 20 ms (no trails); no click either way |
| Hold | the loop sustains without growing or decaying beyond ± 0.5 dB over 30 s |
| Mono / stereo | Mono mode: L = R |
| Presets | golem's 10 delay presets (Standard, Slapback, Dotted eighths, Ambient…) on clean and lead-clean |

Listening against ReaDelay (golem renders) and between engines, blind, level-matched.
