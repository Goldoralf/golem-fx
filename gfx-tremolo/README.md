# gfx-tremolo

> **Status: CODED** (v0.1.0, 2026-11-12) — Clean (Calf Pulsator's law), Bias (antialiased push-pull with moving bias), Optical (lamp → LDR), Neon (striking lamp → LDR), Harmonic (antiphase bands); duty, 2 ms square edges, host sync with position lock, dynamics, stereo phase; model and wave changes crossfade; 7 behaviour tests, bench green.

golem's tremolo. Stands for golem's `tremolo` block (today Calf Pulsator), keeping its parameter ids and
units ([golem contract](../docs/golem-contract.md)). The aim: the amp tremolos — bias, optical, neon,
harmonic — modelled as what they are (an LFO driving a circuit), plus a clean tremolo that keeps today's
presets. Hard gates, step patterns and pick triggers belong to [gfx-killswitch](../gfx-killswitch/).

## 1. What golem has today

- Calf Pulsator (Linux) and `golem_tremolo.jsfx` (Windows / macOS, same law):
  `in · (lfo · 0.5 · amt + amt / 2) + in · (1 − amt)` — linear amplitude modulation.
- Calf's native right-channel offset is 0.5: in stereo the "tremolo" is really an auto-panner.
- Presets: Helicopter, Standard, Surf, Swamp.

## 2. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference; **[U]** not checked.

| Reference | What it does best | What we take |
|---|---|---|
| [Strymon, *Amplifier Tremolo Technology*](https://www.strymon.net/amplifier-tremolo-technology-white-paper/) [V] | All vintage types share one RC phase-shift LFO (a mildly distorted sine): **the circuit after the LFO makes the difference**. Bias: crossover distortion near the minimum, compression near the maximum, supply sag. Photocell: neon bulb, long high level with short dips, choppy at full intensity; the LDR loads the signal and shifts the tone. Harmonic: antiphase bands, each moved on the tube's curve — gain and harmonics change, "mildly phaser-like" | Model = LFO → circuit |
| [Strymon Flint](https://www.strymon.net/product/flint/) [V] | '61 Harmonic, '63 Power Tube, '65 Photocell | The three core models |
| [Fractal Tremolo / Panner](https://www.fractalaudio.com/downloads/manuals/fas-guides/Fractal-Audio-Blocks-Guide.pdf) [V] | Bias, Harmonic (crossover + slope), Optical 1 / 2, **Neon** (fixed original LFO), Panner; duty, L / R phase, **Ducking** ("depth backs off when you dig in, like tube amp trems") | Ducking; neon vs incandescent |
| [Line 6 Helix](https://helixhelp.netlify.app/models/) [V], [measured](https://tremolo-project.blogspot.com/2017/09/line-6-helix-all-tremolo-modes-examined.html) [V] | Optical (blackface, choppy), 60s Bias (Vox, smooth, off-centre), Harmonic (bass / treble frequency, duty) | Duty that leans the waveform |
| [Chase Bliss Gravitas](https://www.chasebliss.com/s/Gravitas_Manual_Pedal_Chase-Bliss.pdf) [V] | Standard, Harmonic and **both** together | Blend of bias and harmonic |
| [Guitarix tremolo.dsp](https://github.com/brummer10/guitarix/blob/master/trunk/src/faust/tremolo.dsp) (GPL) [V] | Vactrol: LDR 300 Ω – 1 MΩ on a log curve, 5 ms lit / 60 ms dark, divider | Optical starting point |
| [Airwindows TremoSquare](https://github.com/airwindows/airwindows/tree/master/plugins/LinuxVST/src/TremoSquare) (MIT) [V] | Square tremolo that switches only at zero crossings: never clicks, but the edges wander in time | Not taken: fixed short edges instead |
| Harmonic crossover [U] | Disputed: ~400 Hz, or a low-pass at 144 Hz and a high-pass at 636 Hz with a gap; the imperfect overlap gives the phasing ([Fender patent US2973681](https://www.effectrode.com/wp-content/uploads/2020/05/leo_fender_harmonic_vibrato_patent_us2973681.pdf)) | Gapped bands, crossover exposed |

### What we conclude

1. **Five models**: Clean (today's sound), Bias, Optical, Neon, Harmonic.
2. The LFO is shaped by `wave`, then drives the circuit; the circuit gives the character.
3. **Ducking** (depth backs off when you dig in) and **duty** are the live-grade extras.
4. Stereo phase defaults to 0 (a tremolo, not Calf's accidental auto-pan).

## 3. Design

| `model` | Circuit | Notes |
|---|---|---|
| **Clean** (default) | Linear AM, Calf-compatible | Keeps today's presets; safe for bass and keys |
| **Bias** | Push-pull pair `y = T(b + x) − T(b − x)`, `T(u) = max(0, u)^1.5`; the LFO moves the bias `b` | Crossover distortion near the trough, compression at the top, level-dependent; sag (an envelope on the output) lowers `b`: ducking built in. ADAA, no oversampling |
| **Optical** | Incandescent lamp (thermal one-pole 20 – 40 ms) → LDR (5 ms attack, 60 ms release, log R) → divider | Intensity drives the lamp: shape changes with depth |
| **Neon** | Neon lamp (strikes above a threshold, goes out below a lower one) → the same LDR | Low intensity = shorter dips, not shallower; choppy at full; fixed LFO shape |
| **Harmonic** | Low-pass at `xover` / 2 and high-pass at `xover` · 1.6 (gapped), antiphase gains, each band through a mild triode curve whose bias follows the LFO | `blend` mixes it with Bias (Gravitas "both") |

- `wave` Square uses fixed 2 ms raised-cosine edges (as gfx-killswitch). Not zero-crossing switching:
  waiting for a crossing shifts each edge by up to half a period (12 ms on a bass's low E) and blurs the
  rhythm — the same reason gfx-gate rejects it.
- Phase 0 at reset, never reset by a parameter change; synced from the host's position when `sync` is on,
  re-locked after a relocation with a 5 ms gain slew.

## 4. Parameters

Catalogue ids kept; ranges widened where safe.

| id | Name | Type | Range | Catalogue | Default | EXP | Role |
|---|---|---|---|---|---|---|---|
| `rate` | Rate | f | 0.5 – 15 Hz | 2 – 12 | 5 Hz | ✓ | |
| `depth` | Depth | f | 0 – 1 | same | 0.6 | ✓ | Trough gain = 1 − depth; lamp drive in Optical / Neon |
| `wave` | Wave | e | Sine · Triangle · Square | same | Sine | | Pre-shape of the LFO |
| `model` | Model | e | Clean · Bias · Optical · Neon · Harmonic | new | Clean | | |
| `sync` | Sync | e | Off · 1/1 … 1/32 (+ dotted / triplets) | new | Off | | |
| `duty` | Shape | f | 0 – 1 | new | 0.5 | | Leans the waveform |
| `xover` | Crossover | f | 100 – 2,000 Hz | new | 400 Hz | | Harmonic |
| `blend` | Harmonic Mix | f | 0 – 1 | new | 1 | | Harmonic ↔ Bias |
| `drive` | Tube Drive | f | 0 – 1 | new | 0.3 | | Bias and Harmonic nonlinearity |
| `duck` | Dynamics | f | 0 – 1 | new | 0 | | Playing harder reduces the depth |
| `stereo_phase` | Stereo Phase | f | 0 – 180° | new | 0° | | L / R LFO offset; 180° = auto-pan |
| `level` | Level | f | 0 – 12 dB | new | 0 dB | | Makeup (a tremolo lowers the average level) |

## 5. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Surf / spaghetti western | Optical, rate 6 Hz, depth 0.8 | Lamp + LDR: rounded, lopsided throb |
| Blackface "Vibrato" channel, choppy | Neon, rate 5 Hz, depth 0.9 | Neon strike / extinguish |
| Swampy, warm (CCR) | Bias, rate 4 Hz, depth 0.6, `drive` 40 % | Bias tremolo: breathes with the playing, softens when you dig in |
| Brownface harmonic shimmer | Harmonic, rate 3.5 Hz, depth 0.8, `xover` 400 Hz | Bands in antiphase: phaser-like, level stays steady |
| Subtle movement on clean chords | Clean, rate 3 Hz, depth 0.3 | |
| Helicopter | Clean, `wave` Square, rate 10 Hz, depth 1 | Fixed 2 ms edges |
| Tempo-locked | `sync` 1/8 or 1/8T | Phase from the host's position |
| Depth that follows the playing | `duck` 60 % | Loud playing = less tremolo, the tail pulses |
| Auto-pan | Clean, Sine, `stereo_phase` 180° | |
| Bass / keys | Clean (no added distortion) | |
| Level drop compensated | `level` +3 dB at deep settings | |

## 6. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| Clean | identical gain law to Calf Pulsator at golem's four presets (mono) |
| Envelope shape | gain envelope per model on a sine: shape and duty as specified |
| Bias | distortion appears near the trough only, grows with input level |
| Harmonic | 100 Hz and 3 kHz band gains in antiphase |
| Square | 2 ms edges: no click (HF burst bound), edges on the beat ± 1 sample when synced |
| Phase | moving any parameter never jumps the LFO phase |
| Mono | `stereo_phase` 0: L = R |
