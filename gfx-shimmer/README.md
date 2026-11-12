# gfx-shimmer

> **Status: CODED** (v0.1.0, 2026-11-12) — shifters in the reverb's feedback (gfx-dsp's loop shifter: equal-power heads, seeded jitter, forward or reverse grains), two detuned voices over ±24 st, loop low / high cut (anti-alias clamp), placement Regen / Input / Both, spaces Hall / Cathedral / Cloud (FDN-16) and Plate, bloom, freeze that stops the climb, infinite, ducking, swell, pitch glide, trails; 4 behaviour tests (T60 at shimmer 0, first pass an octave up, bounded at full shimmer in every space and character, freeze holds without climbing), bench green. Choices made in code: the regeneration adds only the shifted signal (the tank keeps its own decay, so `decay` stays the T60) and is scaled by 1 / √(1 + decay) and soft-limited; CPU 707 ns / sample at 48 kHz (3.4 % of a core, above the 2 % target). The blind acceptance test against ValhallaShimmer (§2) is still to run.

golem's shimmer reverb. Stands for golem's `reverb-shimmer` block (today guitarix GxShimmiZita:
zita-rev1 + a pitch shifter **limited to ±6 st**, so the classic octave shimmer is out of reach — golem's
"Octave halo" preset plays a tritone). Keeps the block's parameter ids and units
([golem contract](../docs/golem-contract.md)). Built on the gfx-reverb tank and the gfx-pitch delay-line
shifter. gfx-reverb carries a one-voice version of the same insert as a colour on any algorithm, and
gfx-delay a shifter in its loop (shimmer delay); this plugin is the full instrument.

## 1. Local facts

- Catalogue → GxShimmiZita ports: `mix` = dry / wet, `decay` = mid-band T60 (1 – 8 s), `shimmer` = blend of
  the shifted signal **inside** the loop, `pitch` = shift (hard-limited to ±6 st), `hicut` = damping
  crossover.
- golem presets: Standard 40 / 4 / 50 / +5 / 6000; Ambient shimmer 55 / 7 / 70 / +6 / 7000; Octave halo
  45 / 5 / 55 / +6 / 6500 (meant to be +12); Dark descent 40 / 6 / 60 / −6 / 4500. **Golem side, once gfx-shimmer
  ships: Octave halo and Ambient shimmer → +12.**
- `golem_shimmer.jsfx` (MIT, Windows / macOS): 8-line Hadamard FDN with a 2-head shifter on every line,
  3-band decay, shifter off below 0.05 st. A useful comparison.

## 2. Can it approach ValhallaShimmer?

Yes — more reachably than for any other effect here, and the bench will say how close:

- **The recipe is public.** Its author documented the principle (shifter in the feedback path of diffuse
  delay lines), why the shifter must stay simple (the reverb scrambles phase; random splices *are* the
  sound), and the role of loop filtering and diffusion. Our design follows it.
- **What is left is voicing**: the smoothness of the diffusion network, modulation amounts, gain staging per
  pass. That is ear work against the reference — blind, level-matched, on the same DI — not missing
  science.
- **Where this design goes further** than the reference's controls (as listed in its manual): two
  independent detuned voices over ±24 st, freeze that stops the climb (Eventide), infinite, ducking, swell,
  shifter placement (Strymon), choice of space, pitch glide, trails.
- **Acceptance test**: on clean.wav and lead-clean.wav, at matched settings (+12, long decay), a listener
  cannot reliably prefer the reference over gfx-shimmer in a blind A / B. Until then it is not done.

## 3. State of the art

Survey of 2026-11-12. **[V]** read in a primary source; **[I]** inference or secondary.

| Reference | What it does best | What we take |
|---|---|---|
| Eno / Lanois ([Valhalla DSP](https://valhalladsp.wordpress.com/2010/05/11/enolanois-shimmer-sound-how-it-is-made/)) [V] | A feedback loop with an octave-up pitch shifter and a long reverb, shaped by loop gain and EQ (highs rolled off on the return) | The loop is the instrument |
| Sean Costello on decorrelation ([Valhalla DSP](https://valhalladsp.wordpress.com/2010/05/12/shimmer-modulation-auto-correlation-and-decorrelation/)) [V] | Inside a reverb the phase is scrambled: a shifter cannot find good splice points, splices land at random and spread sidebands — **that spreading is the sound** | A plain delay-line shifter, no splice search |
| [ValhallaShimmer](https://www.valhalladsp.com/shimmer/ValhallaShimmerNotes.pdf) [V] | Shifter in the feedback of diffuse delay lines, ±12 st; Single / Dual (up + down) / **Reverse** grains ("smoother… organ-like"); loop low / high cut; Dark colour removes most loop aliasing; diffusion 0.5 – 0.618 gives "Bloom" | Two voices, reverse grains, loop filters, bloom |
| [Eventide H9 Shimmer](https://downloads.eventideaudio.com/Product+Manuals/H9AlgorithmGuide+V12.pdf) [V] | Reverb → delay → **two shifters A / B** → 3-band decay in the loop; **Pitch-Decay** separate from Decay; **Pitch Freeze** (climb stops) and Pitch + Verb Freeze; "Heaven": A and B just above and below +1200 cents | Detuned voices, freeze that stops the climb |
| [Strymon BigSky](https://www.manualslib.com/manual/963033/Strymon-Bigsky.html) [V] | Two voices from the reverberated signal; **Input / Regen / In + Regen** placement (Input does not climb) | Placement |
| [Meris Mercury7](https://www.meris.us/product/mercury7/) [V] | "Pitch Vector" inside the tank (octave down, slight up / down, fifth, octave up); swell envelope | Interval choices, swell |
| Shimmer from FDN nonlinearities — Dal Santo, Pi, Prawda, Schlecht & Välimäki, DAFx 2026 ([page](http://research.spa.aalto.fi/publications/papers/dafx26-shimmer/), [code, MIT](https://github.com/gdalsanto/shimmer-fdn-reverb)) [V] | Shimmer without a shifter | To watch; not the classic sound |

Open source: [Mutable Instruments Clouds](https://github.com/pichenettes/eurorack/tree/master/clouds/dsp/fx)
(MIT) — the canonical 2-head shifter, short and readable; [CloudSeedCore](https://github.com/GhostNoteAudio/CloudSeedCore)
(MIT) — diffusers and modulated lines; [elysiera](https://github.com/azur1s/elysiera) (GPL-3) — nih-plug +
shifters + zita-rev1, our own stack.

### What we conclude

1. **The shifter goes inside the loop** (every pass climbs); Input placement is the tame option.
2. **Two voices**, +12 first, +7 / +19 / +24 / −12 next; voices detuned a few cents for stereo.
3. **A cheap delay-line shifter**: the reverb defeats splice search, and a phase vocoder smears on every pass.
   Reverse grains for the smooth organ sound.
4. **Loop filters before the shifter**: high cut against build-up and aliasing, low cut against octave-down
   mud.
5. **Freeze stops the climb** by taking the shifter out of the loop.

## 4. Design

```
in ─┬──────────────────────────────────────────────────────────────── dry ──┐
    └▶ (Input voices) ▶ gfx-reverb tank ─────────────────────────── wet ──┴▶ mix ▶ out
                         ▲                         │
                         └── voices ◀ low / high cut ◀ predelay ◀ regeneration
```

- **Regeneration insert**: the tank's output → loop filters → shifter voices → back into the tank input.
  One insert for the whole tank, not a shifter on every line (8× the CPU, and the T60 calibration
  suffers).
- Loop signal = equal-power blend of (1 − `shimmer`) · unshifted and `shimmer` · (voice 1 + `voice2` · voice 2).
- **Shifter**: gfx-pitch's delay-line core, 2 – 3 heads, **equal-power** crossfade (a linear one loses
  ~1.8 dB per pass on decorrelated content and skews the decay), 50 – 80 ms windows with seeded jitter,
  different on L and R. Reverse-grain option. At `pitch` 0 the voice passes unshifted.
- **Anti-aliasing**: the loop low-pass runs before the shifter, clamped to ≤ 0.45 · sr / ratio.
- **Safety**: loop gain < 1 by construction (the shifter's mean delay included in the T60 maths), DC
  blocker, low cut, a slow transparent limiter in the loop.
- **Freeze**: loop gain 1, input muted, shifter out of the loop.
- **Trails**: `make_bypass`; the input is muted, the tank rings out.
- **Latency: reported 0.** The shifter sits in the wet path's loop: its delay lengthens the tail like
  extra predelay. Reporting it would delay the dry, which is wrong.
- **Determinism**: per-voice fixed head phases, fixed-seed jitter.

## 5. Parameters

Catalogue ids kept; `pitch` widened so the octave is reachable.

| id | Name | Type | Range | Catalogue | Default | EXP | Role |
|---|---|---|---|---|---|---|---|
| `mix` | Mix | f | 0 – 100 % | same | 40 % | ✓ | Equal-power dry / wet |
| `decay` | Decay | f | 0.5 – 20 s | 1 – 8 | 4 s | ✓ | Mid-band T60 |
| `shimmer` | Shimmer | f | 0 – 100 % | same | 50 % | ✓ | Share of shifted signal in the loop |
| `pitch` | Pitch | f | −24 – +24 st | −6 – +6 | **+12 st** | | Voice 1; smoothed in semitones |
| `hicut` | High Cut | f | 1,000 – 16,000 Hz | 2,000 – 12,000 | 6,000 Hz | ✓ | Loop low-pass, also anti-alias |
| `pitch2` | Pitch 2 | f | −24 – +24 st | new | +7 st | | Voice 2 |
| `voice2` | Voice 2 Level | f | 0 – 100 % | new | 0 % | | |
| `detune` | Detune | f | 0 – 50 cents | new | 6 cents | | ± split L / R |
| `lowcut` | Low Cut | f | 20 – 1,000 Hz | new | 150 Hz | | Loop high-pass |
| `placement` | Placement | e | Regen · Input · Both | new | Regen | | Input does not climb |
| `size` | Size | f | 0 – 100 % | new | 60 % | | Tank scale |
| `predelay` | Predelay | f | 0 – 500 ms | new | 20 ms | | |
| `bloom` | Bloom | f | 0 – 100 % | new | 70 % | | Diffusion; mid values fade in slowly |
| `moddepth` | Modulation | f | 0 – 100 % | new | 25 % | | Low by default: the shifter already modulates |
| `character` | Character | e | Grainy · Smooth | new | Grainy | | Smooth = reverse grains |
| `space` | Space | e | Hall · Cathedral · Cloud · Plate | new | Hall | | Which gfx-reverb tail carries the shimmer: Cloud = the biggest and slowest (ValhallaShimmer's large modes), Plate = tight and bright |
| `mod_rate` | Modulation Rate | f | 0 – 100 % | new | 50 % | | 50 % = the space's own |
| `swell` | Swell | f | 0 (off) – 2,000 ms | new | 0 | | Auto volume swell into the reverb: no pick attack in the pad (Mercury7) |
| `voice_pan` | Voice Spread | f | 0 – 100 % | new | 50 % | | Voice 1 towards the left, voice 2 towards the right |
| `bass_mult` | Bass Decay | f | 0.5 – 2 × | new | 1 × | | As gfx-reverb |
| `tone` | Tone | f | 0 – 100 % (50 = flat) | new | 50 % | | Tilt on the wet |
| `glide` | Pitch Glide | f | 0 – 2,000 ms | new | 50 ms | | Time for `pitch` / `pitch2` to reach a new value: scene changes sweep instead of jumping |
| `duck` | Ducking | f | 0 – 100 % | new | 0 % | ✓ | Wet ducked while playing |
| `freeze` | Freeze | e | Off · Freeze · Infinite | new | Off | ✓ | Freeze = the pad holds, the climb stops (shifter out of the loop), new notes stay out; Infinite = the pad holds and new notes add to it |
| `width` | Width | f | 0 – 100 % | new | 100 % | | |
| `kill_dry` | Kill Dry | b | off / on | new | off | | Parallel lane |
| `trails` | Trails | b | off / on | new | on | | |
| `on` | On | b | off / on | new | on | | Bypass parameter with trails |
| `wet_level` | Wet Level | f | −24 – +24 dB | new | 0 dB | | Wet gain. GxShimmiZita's wet is far hotter than a calibrated reverb (golem-rack: +10.5 dB overall at mix 30 %); golem can pin this at insert for preset loudness |

## 6. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Classic octave shimmer (Eno / Lanois, The Edge) | pitch +12, shimmer 50 %, decay 5 s, mix 40 %, hicut 6 kHz | Shifter in the loop: each pass climbs an octave |
| Organ-like halo | `character` Smooth, pitch +12, `pitch2` +19, `voice2` 40 % | Reverse grains; octave + fifth |
| Fifths, cinematic | pitch +7, `pitch2` +12, `voice2` 60 % | Two voices |
| Dark descent | pitch −12, shimmer 60 %, hicut 4.5 kHz, `lowcut` 200 Hz | Loop low cut against mud |
| Subtle sparkle on clean chords | shimmer 25 %, decay 3 s, mix 25 % | |
| Shimmer that does not climb | `placement` Input | One octave above, no runaway |
| Pad that stays out of the way | `duck` 50 %, `bloom` 60 % | Ducking + slow fade-in |
| Hold a chord as a drone | `freeze` on a footswitch | The climb stops, the pad sustains |
| Swells with the volume pedal | mix 60 %, decay 8 s, EXP on `mix` or `shimmer` | |
| ValhallaShimmer-style giant pad | `space` Cloud, pitch +12, shimmer 60 %, decay 12 s, `bloom` 60 %, `moddepth` 20 % | Diffuse big tail + in-loop shifter |
| Octave up and down together (Valhalla "Dual") | pitch +12, `pitch2` −12, `voice2` 100 %, `lowcut` 200 Hz | Two voices |
| Attack-free strings | `swell` 500 ms, `character` Smooth, mix 70 % | Swell + reverse grains |
| Bright, tight shimmer on rhythm | `space` Plate, decay 2 s, shimmer 35 % | |
| Growing wash you can play into | `freeze` Infinite | |
| Chord change sweeps the pad's interval | scene sets `pitch` +7 → +12, `glide` 1 s | Pitch glide |

## 7. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| Stability | pitch +12, shimmer 100 %, decay 20 s, 60 s of sine sweep: output bounded |
| T60 | at shimmer 0, T60 within ±10 % of `decay` |
| Octave | pitch +12: the first pass is one octave up within ±2 cents |
| Freeze | no climb: spectrum centroid stable over 30 s |
| Determinism | strict null between two renders, and between REAPER and Ardour |
| CPU | tank + 2 voices stereo ≤ 2 % of one core at 48 kHz |
| Spaces | stable and bounded in every `space` at pitch +12, shimmer 100 % |
| Listening | clean.wav and lead-clean.wav with golem's four presets (Octave halo at +12), against GxShimmiZita and golem_shimmer.jsfx; then the blind acceptance test of §2 |
