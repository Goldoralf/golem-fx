# gfx-comp

> **Status: PROPOSED** — design, no code yet.

golem's compressor. Stands for golem's `comp` block (today ReaComp), keeping its parameter ids and
units ([golem contract](../docs/golem-contract.md)). The aim is the best compressor for a live
guitar / bass / voice rack: transparent when asked, characterful when asked, never pumping by accident,
zero latency.

## 1. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[K]** known but not re-checked.

### Commercial references

| Product | What it does best | What we take |
|---|---|---|
| [FabFilter Pro-C 3](https://www.fabfilter.com/help/pro-c/using/styleandcharacter) [V] | 14 "styles" that are really topology (feedforward / feedback) + release program-dependence + knee; saturation as a **separate** Character stage (Tube, Diode, Bright), pre or post | Characters as combinations of topology, detector, knee and release; saturation as its own stage |
| [TDR Kotelnikov](https://docs.tokyodawn.net/kotelnikov-manual/) [V] | Two detection paths: **peak** with its own fast release (transients) and **RMS** with its own release, auto-lengthened on sustained reduction; sidechain low-frequency relaxation; bit-transparent at 0 dB reduction | The dual-path release for the Clean character; bit-transparency as a rule |
| [Cytomic The Glue](https://cytomic.com/product/glue/) [V] | SSL bus detector solved by nodal analysis; Range (max reduction); soft sidechain HPF; phase-accurate dry/wet; minimum-phase oversampling with zero latency | Range, sidechain HPF, minimum-phase oversampling, coherent mix |
| [Klanghelm DC8C](https://klanghelm.com/contents/products/DC8C) [V] | Continuous **feedforward ↔ feedback** blend; program dependence for attack and release separately; true 0 ms attack | FF ↔ FB blend as the core of our characters |
| [Waves Renaissance Compressor](https://josephnilo.com/blog/waves-rcomp-explained/) [V] | ARC: the release knob becomes a scale that the signal varies | Auto release |
| [UA 1176](https://en.wikipedia.org/wiki/1176_Peak_Limiter) / [LA-2A](https://en.wikipedia.org/wiki/LA-2A_Leveling_Amplifier) [V] | 1176: feedback, sub-ms attack, ratio and ballistics that depend on the programme. LA-2A: two-stage release from the lamp + photocell memory (≈ 60 ms for the first half, then 1–15 s) [K] | The FET and Opto characters |
| [MXR Dyna Comp](https://www.dafx.de/paper-archive/2011/Papers/22_e.pdf) / Ross [V] | OTA gain cell, fast attack, slow release, high ratio: the country / funk "squash" | The Pedal character |
| [Origin Cali76](https://www.origineffects.com/product/cali76-compact-deluxe/) [V] | 1176 on a pedal with a parallel dry control | Mix |
| [Line 6 Helix](https://line6.com/helix-models) [V] | Guitar rack set: Red Squeeze (Dyna Comp), Kinky Comp (Xotic SP), LA Studio Comp, Deluxe Comp, 3-Band | Confirms the characters a guitarist expects |

### Open source

| Project | Idea |
|---|---|
| [LSP Compressor](https://lsp-plug.in/?page=manuals&section=compressor_mono) | FF / FB / link sidechain position; Peak, RMS, LPF, SMA detectors; **Release Threshold** (two-stage release); upward and boosting modes |
| [CTAGDRC](https://github.com/p-hlp/CTAGDRC) | Clean Giannoulis implementation: quadratic soft knee in dB, smooth ballistics after the gain computer, **crest-factor auto attack / release** |
| [Faust compressors.lib](https://sources.debian.org/src/faust/2.30.5~ds0-2/libraries/compressors.lib) (Bart Brouns) | FB ↔ FF crossfade; "strength" instead of ratio |
| [magnetophon/lamb](https://github.com/magnetophon/lamb) | Shapeable attack and release curves |
| [Squeezer](https://github.com/mzuther/Squeezer) (GPL-3) | Optical and FET detector modes, three release curves |
| [Airwindows ButterComp2, Pressure5](https://www.airwindows.com/buttercomp2/) | Release that slows down when hot; interleaved bipolar compressors |
| [x42 dpl](https://github.com/x42/dpl.lv2) | Hold extended on loud low frequencies (bass) |
| [Rudrich SimpleCompressor](https://github.com/DanielRudrich/SimpleCompressor) | Correct lookahead (fade the gain curve in) — for later, if ever |

### Papers

- Giannoulis, Massberg & Reiss, *Digital Dynamic Range Compressor Design — A Tutorial and Analysis*,
  JAES 2012 ([PDF](https://www.eecs.qmul.ac.uk/~josh/documents/2012/GiannoulisMassbergReiss-dynamicrangecompression-JAES2012.pdf)):
  feedforward, smooth peak detector in dB **after** the gain computer, quadratic soft knee [K].
- Giannoulis, Massberg & Reiss, *Parameter Automation in a Dynamic Range Compressor*, JAES 2013: crest-factor
  automation of attack and release.
- Gerat, Eichas & Zölzer, 1176LN grey-box model, AES 143 (2017); Eichas et al., optocoupler compressor model;
  DAFx-23 paper 50, vactrol model; Wright et al., grey-box compression modelling, DAFx-20.
- Sierra, *Is oversampling always the solution?*, AES 154 (2023): band-limit the gain signal as an
  alternative to oversampling.

### What we conclude

1. A "character" is a choice of **topology** (FF / FB blend), **detector**, **knee**, **release
   behaviour** and **colour**. That is what Pro-C, DC8C and Helix sell; modelling every circuit is not
   needed to get there.
2. **Program-dependent release** (two-stage) is the single biggest quality jump for guitar sustain and
   voice.
3. A **sidechain high-pass** is the biggest win for bass and drop-tuned guitar (no pumping on lows).
4. **Mix** (parallel) covers funk squash and the blend pedals.
5. **Bit-transparent** when nothing is compressed; **no aliasing** on fast attack (minimum-phase 2×
   oversampling of the gain stage only, when it matters).

## 2. Our design

### Rule: the static curve is the same in every character

`thresh`, `ratio` and `knee` mean the same thing in every character: a steady tone at a given level comes
out at the same level. Feedback characters compensate their ratio (a feedback stage with slope R − 1 on
the output overshoot gives a static ratio R). Only the **dynamics** (how fast, how program-dependent)
and the **colour** change. So a golem preset keeps its level when the character changes, and blind
comparisons between characters are fair.

### Signal path

```
in ─┬─────────────────────────────────────────────── dry ──┐
    └▶ sidechain HPF ▶ detector (peak / RMS / crest) ▶ gain computer (thresh, ratio, knee)
         ▲ (FB: output)                                 ▶ ballistics in dB (attack, release, auto) ▶ range
    in ▶ gain (×) ▶ colour (drive) ▶ makeup ─────────────── wet ─┴▶ mix ▶ out
          └─ 2× min-phase oversampling around gain × colour, only when the character or drive needs it
```

### Characters

| `character` | Topology | Detector | Knee default | Release | Colour | For |
|---|---|---|---|---|---|---|
| **Clean** | FF | peak + RMS (crest), Kotelnikov-style | 6 dB | dual path: fast after transients, slow after sustained reduction | none, bit-transparent | default; voice, keys, clean guitar |
| **Bus** | FF | RMS-ish (SSL-like) | 2 dB | single, slightly program-dependent | light | glue on a mix, keys, after amp |
| **FET** | FB | peak, sub-ms capable | 0 dB | program-dependent | odd harmonics | lead guitar, bass punch, aggressive voice |
| **Opto** | FB | smoothed RMS | 12 dB | two-stage LA-2A: fast half, then slow tail charged by how long and hard it worked | warm, even | voice, bass, clean guitar sustain |
| **Pedal** | FB | peak, fast fixed rise | 0 dB | slow, with the makeup in the loop (OTA) | OTA grit, a little hiss | country / funk squash (Dyna Comp, Ross) |

## 3. Parameters

Catalogue ids and units kept; ranges wider. Defaults = the catalogue's.

| id | Name | Type | Range | Catalogue | Default | Curve | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `thresh` | Threshold | f | −60 – 0 dB | −40 – −6 | −18 dB | lin | ✓ | |
| `ratio` | Ratio | f | 1 – 30 :1 | 1.5 – 10 | 4 :1 | log | | 30 ≈ limiting |
| `makeup` | Makeup | f | −12 – +24 dB | −6 – +12 | +3 dB | lin | ✓ | Output gain of the wet |
| `attack` | Attack | f | 0.02 – 200 ms | 1 – 50 | 15 ms | log | | |
| `release` | Release | f | 5 – 3,000 ms | 40 – 400 | 150 ms | log | | With `auto_release`: the fast stage, the slow one follows |
| `character` | Character | e | Clean · Bus · FET · Opto · Pedal | new | Clean | — | | §2 |
| `knee` | Knee | f | 0 – 24 dB | new | per character | lin | | Quadratic soft knee in dB |
| `auto_release` | Auto Release | b | off / on | new | on | — | | Program-dependent release of the character |
| `sc_hpf` | Sidechain Low Cut | f | 0 – 500 Hz | new | 0 (off) | log | | 12 dB/oct on the detection only |
| `range` | Range | f | 0 – 60 dB | new | 60 dB | lin | | Maximum gain reduction |
| `drive` | Drive | f | 0 – 100 % | new | 0 % | lin | | Extra colour of the character |
| `mix` | Mix | f | 0 – 100 % | new | 100 % | lin | ✓ | Parallel compression, phase-coherent (zero latency) |

Read-only (meter, [golem contract](../docs/golem-contract.md)): `gr`, gain reduction in dB.

`knee` has a default per character, but a parameter cannot change another one (contract): its stored
default is 6 dB and the character only shifts it when `knee` is left at its default. **To decide:** or
`knee` becomes "Knee Offset" (± dB around the character's knee). The bench comparison will tell.

Not taken, and why:

- **Lookahead**: it needs reported latency; golem wants none. Little value for live guitar.
- **External sidechain (key input)**: a second input golem's lanes do not route today.
- **Auto makeup**: golem's presets set `makeup` by ear; an automatic one would move every preset's level.
  A loudness-matched A/B lives in gfx-bench instead.

## 4. Bench suite (on top of gfx-bench's common checks)

| Check | Signal | Pass |
|---|---|---|
| Static curve | slow level ramp, every character | output vs input matches thresh / ratio / knee ± 0.1 dB, the same in every character |
| Transparency | signal below threshold − knee / 2, drive 0, mix 100 % | bit-identical to the input |
| Attack / release | level steps, +20 dB and −20 dB | time constants within 5 % of the setting (Clean, Bus) |
| Auto release | short burst vs long sustained reduction | release after the burst ≪ release after the sustain |
| Aliasing | 5 kHz sine bursts, FET and Pedal, attack 0.02 ms | aliased components ≤ −90 dBFS |
| Sidechain HPF | 50 Hz + 1 kHz mix, `sc_hpf` 150 Hz | the 50 Hz component does not drive the reduction |
| Range | +30 dB step, `range` 6 dB | reduction stops at 6 dB |
| Mix | mix 50 %, any character | no comb filtering (dry and wet in phase) |
| Presets | golem's 8 comp presets (Standard, Gentle, Squash, Parallel smash, Bass anchor, Keys glue, LA2A vocal, Vocal opto) on clean, funk, lead-clean | renders kept for blind listening against ReaComp |
