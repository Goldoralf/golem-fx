# gfx-octafuzz

> **Status: CODED** (v0.1.0, 2026-11-26; roadmap 0.8, item 1). **GoleM FX Octafuzz**: the transformer
> octave fuzz (Tycobrahe-type: Octavia / Octavio), solved as a circuit with the guitar in front of it.
> A thin pedal over gfx-fuzz's engine, decided by Raph on 2026-11-26 (gfx-fuzz README, O6). **The design
> is in [gfx-fuzz/README.md](../gfx-fuzz/README.md)**: §2.3 "Octave Up" (netlist, transformer cut, L_m,
> the bloom, the DC guard), §2.7, §2.8 (CPU), §5 (bench rows), and its "Review (Fable 5.2, 2026-11-26) —
> Octave Up". This file holds the pedal's parameters, what was coded and the measurements.
>
> Measured (details in "As coded"): DC point within 0.03 V of the prototype on every node; the Octave
> row of §5 green with the prototype's figures to ±0.2 dB (440 Hz: +23.6 / +1.9 / −8.9 dB at 30 mV /
> 0.1 V / 0.3 V); thump 0.31 V against 0.92 V rms (prototype 0.31 / 0.92); flux linkage 13.3 / 8.7 /
> 4.2 mWb; bench 28 / 28. **CPU: 7.7 – 7.9 µs per sample mono (37 – 38 % of a core at 48 kHz)**, above
> the contract's ≈ 20 % under the per-pedal exception Raph granted (expected 30 – 40 %). **Aliasing:
> the −65 dB rule is not met at 8× on the rectified output** (−57.4 dB at 1.2 kHz; the fuzz tap meets it
> at −70.8 dB): the design's −76 dB was an artefact of its test frequency. The maintainer decides (16×
> gives −66.6 dB at twice the CPU). **L_m = 1 H**, the design's working value, until Raph measures a
> 42TM022.

## Parameters

Ids keep gfx-fuzz's meanings, ranges and defaults (gfx-fuzz §3); `model`, `stack`, `mids` and `leakage`
are not taken (one circuit, silicon transistors). `octave` is the pedal's own id, on the face.

### Front panel

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `fuzz` | Fuzz | f | 0 – 100 % | 70 % | EXP rule → pot, every sample | ✓ | The 1 kΩ B fuzz rheostat in the loop's AC ground (100 % = R_MIN): the series-series feedback around Q1 – Q3 |
| `octave` | Octave | f | 0 – 100 % | 100 % | 20 ms (EXP rule) | ✓ | Linear crossfade from the fuzz alone (0 %, the tap a · v(winding)) to the rectified output (100 %, the stock pedal); 50 % is a clone's lifted diode to first order |
| `tone` | Tone | f | −100 – +100 % | 0 % | 20 ms | ✓ | Tilt ±6 dB around 800 Hz after the circuit, 0 = off (the circuit has no tone control) |
| `level` | Level | f | −24 – +24 dB | 0 dB | 20 ms | ✓ | 0 dB = unity loudness at the defaults (K = −21.7 dB; the volume pot fully up is `level` +21.7 dB) |
| `guitar_vol` | Guitar Vol | f | 0 – 100 % | 100 % | EXP rule → pot, every sample | ✓ | The guitar's volume knob, real taper and impedance, in the netlist |
| `buffer` | Buffer | b | off / on | off | 30 ms crossfade | | A buffer between the guitar and the pedal: the pickup no longer sees the ≈ 270 kΩ input |

### Expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `pickup` | Pickup | e | Single Coil · Humbucker · P-90 · Active | Single Coil | 30 ms crossfade | | The guitar the DI was recorded with: coil, pots, cable (gfx-fuzz §2.2) |
| `bias` | Bias | f | −100 – +100 % | 0 % | 300 ms | ✓ | The trimmer R2 (Q1's base resistor, stock 680 kΩ) × 2^(+bias / 100): + raises `bias_v` (4.0 – 6.3 V at 9 V) |
| `temp` | Temperature | f | 0 – 50 °C | 25 °C | 300 ms | | Device temperature: the silicon barely moves, the germanium rectifier diodes follow |
| `voltage` | Voltage | f | 3 – 18 V | 9 V | 100 ms | ✓ | Battery voltage: low = starved, 18 V = headroom |
| `sag` | Sag | f | 0 – 100 % | 0 % | 100 ms | | Battery internal resistance, 0 – 2 kΩ with a 47 µF reservoir |
| `input` | Input | f | −12 – +12 dB | 0 dB | 100 ms | | For a DI hotter or weaker than 0 dBFS = 1 V |
| `mix` | Mix | f | 0 – 100 % | 100 % | 20 ms | ✓ | Linear crossfade dry ↔ wet, the dry through the same oversampler path |
| `bass_keep` | Bass Keep | f | 0 (off) – 500 Hz | 0 | 30 ms | | Below it: dry (shared id) |
| `bypass` | Bypass | b | off / on | off | 10 ms | | Shared bypass |
| `bias_v` | Bias (V) | f, read-only | 0 – 18 V | — | — | | Q3's collector (the transformer's driver), 50 ms average, as a multimeter reads it |
| `supply_v` | Supply (V) | f, read-only | 0 – 18 V | — | — | | The circuit's supply now, sag included |

Expert groups (`visuals/data/groups.json`): Guitar (`pickup`, `input`), Circuit (`bias`, `temp`), Power
(`voltage`, `sag`), Blend (`mix`, `bass_keep`, `bypass`).

## Visual

- **Face** (`visuals/data/faces.json`): knobs `fuzz`, `octave`, `tone`, `level`, `guitar_vol`; toggle
  `buffer`; block `octafuzz`; category drive.
- **Finish: gfx-fuzz's**: anodized, germanium copper `#b0602a` (`CRATE_FINISH`, `METAL_CRATE_HUE`); the
  motif tells the two apart.
- **Motif `octring`** (new): left of the rune a sine (two periods), right of it the same sine full-wave
  rectified (four arches: the octave), at ±0.3 of the cartouche's height (≤ 7 units); a thin pill round
  the cartouche at 60 % (as tall as the waves + 3 units each side), the octave's ring.
- **Glyph** `octafuzz` (`visuals/data/glyphs.json`): a sine whose second half is drawn rectified, the
  arches at twice its frequency.

## Presets (`presets/gfx-octafuzz.json`)

The five of gfx-fuzz §4, levels matched on the DI proxy (±0.3 dB): **Octave Ring** (default: `fuzz`
70 %, Humbucker, `tone` −40 %), **Octave Bloom** (`fuzz` 60 %, `input` −6 dB), **Octave Blend**
(`octave` 50 %, `fuzz` 80 %), **Ring Chords** (`fuzz` 100 %), **Octave Lead, Hot** (`bias` +60 %,
`voltage` 12 V: `bias_v` 7.5 V there).

## As coded (v0.1.0)

**Files.**
- `gfx-octafuzz/src/lib.rs`: `Octafuzz`, its `PARAMS`, `set_param` routing `octave` to the engine;
  `plugin.rs`: class id **`GoleMFxOctaFz001`**, mono 1 / 1 then 2 / 2 (linked mono as gfx-fuzz), latency
  0, meters `bias_v`, `supply_v`, the gfx-ui editor; `tests.rs`: 11 behaviour tests, 4 diagnostics (`#[ignore]`: surveys, CPU, preset levels).
- gfx-fuzz (the engine; its `model` enum, `PARAMS`, face and UI bundle unchanged):
  - `circuits.rs`: `octave_up` (the netlist of §2.3, values from Retrograde24 / Octagon), the devices
    (2N4401, 2N5087, the 1N34A fit Is 0.1 µA / N 1.3), **`L_M`**, `DCR_600`, `OCT_A`, `R2_OCT`,
    `Q2_FORWARD`; `Circuit` gains `bias_sign` (−1 on the shipped models), `tap` (the fuzz tap) and
    `nodes` (the test probes).
  - `lib.rs`: the internal kind `OCTAVE` (not in `SHIPPED`: gfx-fuzz's `model` cannot reach it),
    `Config::octave_all`, `Fuzz::octafuzz()` (its pool is Octave Up's eight (pickup, buffer)
    configurations), `Fuzz::set_octave`, the `octave` crossfade at the oversampled rate before the
    decimation (both outputs are rows of the same solve, aligned), `HighPass2`: the **20 Hz
    second-order DC guard** after the crossfade, in place of the 5 Hz one, on this mode only.
  - `tests.rs`: `shipped_renders_are_bit_identical` (written before any change: FNV-1a of gfx-fuzz's
    renders, both models × buffer, knob / temperature / voltage / sag / mix / bass-keep / model changes,
    stereo L ≠ R, 96 kHz: **identical after**), `octave_up_dc_point`, `octave_up_q2_forward_matches_the_full_bjt`,
    `octave_up_voltage_step_without_the_forward_q2`, `octave_up_flux_linkage`, the CPU harness
    `cpu_octave_netlist`.
- gfx-dsp `circuit_nl`: **`DeviceModel::BjtForward`** / `Netlist::bjt_forward` (§2.8 way 2): one port
  (v_BE), the collector current i_f = Is (e^(v_BE / N_F V_T) − 1) on the port, the base current i_f / βF
  as a weighted second injection of the same port column (βF is therefore in the matrices; a part swap
  must keep it, asserted); the reverse junction's saturation current (fA) dropped. New branches only:
  gfx-dist's renders are bit-identical (`renders_are_bit_identical_to_before_the_forward_only_bjt`,
  written before the change: every circuit, two gains, every clipper and op-amp option) and gfx-fuzz's
  too; its unit test `forward_only_bjt_matches_ebers_moll_out_of_saturation` (NPN and PNP).

**Values left open by the design, and the choice.**
- **L_m = 1 H** (`circuits::L_M`, one line). The review's estimate of the real 42TM022 is 0.1 – 0.5 H
  (telephone band, core geometry); 1 H is the design's working value, the one Raph's decision keeps
  until his measurement (O1) and the one the Octave row's marks are set for. At 0.3 H the first mark
  falls to +2 – +6 dB (review); the marks are derived again after the measurement.
- DCR 45 Ω on the 600 Ω winding (one distributor table [U]); a = √(1500 / 600) / 2; leakage and winding
  capacitance dropped (the cut of §2.3); the rectifier unloaded (500 kΩ pot only, as decided).
- The fuzz pot: a linear 1 kΩ, (1 − `fuzz`) · 1 kΩ from the 100 µF to ground (the prototype's law).
- `bias` on R2 with the + sign; stock R2 not re-trimmed.

**Measurements** (`cargo test --release -p gfx-octafuzz --no-default-features`; gfx-fuzz's tests for
the circuit rows).
- DC at 9.26 V: Q1 E / B / C 4.31 / 3.76 / 2.90 V, Q2 E 2.33 V, Q3 C / E 4.98 / 1.67 V (prototype 4.32 /
  3.77 / 2.91, 2.33, 4.99 / 1.67; Retrograde24 4.18 / 3.54 / 2.93, 2.38, 4.85 / 1.73). `bias` −100 / −50 /
  0 / +50 / +100 %: 4.02 / 4.20 / 4.86 / 5.56 / 6.27 V; 15 / 35 °C 4.89 / 4.83 V; 4.5 / 18 V 2.79 / 8.97 V.
- **Octave row** (`buffer` on, `fuzz` 70 %, H2 − H1 over 60 ms after 120 ms), 30 mV / 0.1 V / 0.3 V:
  82 Hz −0.0 / −3.7 / −10.8; 196 Hz +11.7 / −0.6 / −12.7; 440 Hz +23.6 / +1.9 / −8.9; 659 Hz +21.5 /
  +2.7 / −6.9; 988 Hz +20.4 / +2.0 / −5.7 dB (prototype within 0.2 dB). Every mark holds (≥ +12 at 30 mV
  above 400 Hz, ≤ +6 at 82 Hz, ±6 at 0.1 V, ≤ −3 at 0.3 V from 196 Hz).
- Through the guitar (level on the EMF, `buffer` off): Single Coil within 1.6 dB of the buffered figures
  at every note and level (mark 4 dB), Humbucker within 2.1 dB.
- `octave` 0 / 50 / 100 % at 440 Hz, 30 mV: H2 − H1 −24.7 / −8.2 / +23.6 dB. The fuzz tap is +5.5 dB
  louder than the rectified output at the defaults ("fuzz only is louder", Retrograde24).
- Thump (decaying 330 Hz note, 0.2 V): infrasonic peak 0.31 V against 0.92 V rms of audio (mark 0.4×).
- Flux linkage at 0.3 V, `fuzz` 100 %: 13.3 / 8.7 / 4.2 mWb at 41 / 82 / 196 Hz (1.61 / 1.05 / 0.51 ×
  the rated 8.25 mWb; prototype 13.3 / 8.8 / 4.2). Core saturation not modelled (O1).
- Forward-only Q2 against the full Ebers–Moll Q2: −133 dB (440 Hz, 0.3 V) and −145 dB (82 Hz, 1 V) re
  peak. `voltage` stepped 18 → 3 V instantly on the bare circuit, with and without it: no unsolved sample.
- Polarity (`octave` 0 %, 1 kHz at 1 mV): +45° re the input (within ±90°); POLARITY = +1.
- K = −21.7 dB (level unity on gfx-fuzz's DI proxy).
- Newton: +24 dB, DC, 0.7 RMS pink, 3 / 18 V, 0 / 50 °C: no NaN, no unsolved sample; worst sample 18 –
  33 iterations.
- **Aliasing** (−12 dBFS, `fuzz` 100 %, `buffer` on, 44.1 kHz host; worst alias in 20 Hz – 20 kHz re the
  fundamental, every harmonic folded at the circuit's rate and at the host rate):

  | Factor | ≈ 1.2 kHz, `octave` 100 % | the fuzz tap, `octave` 0 % | ≈ 5 kHz, 100 % / 0 % |
  |---|---|---|---|
  | 4× | −41.9 dB | −48.1 dB | −19.8 / −36.1 dB |
  | **8× (shipped)** | **−57.4 dB** | **−70.8 dB** | −35.8 / −47.7 dB |
  | 16× | −66.6 dB | −83.6 dB | −37.2 / −54.5 dB |

  **Deviation from the design**: §2.4 measured −76.2 dB at 8× with 1204 Hz at 44.1 kHz × 8, where
  352.8 kHz / 1204 = 293.02: every fold lands 28 Hz (× n) from a harmonic and the ±150 Hz guard
  excluded them; at 4× (176.4 kHz / 1204 = 146.5) the folds land half-way and the design's −40.8 dB
  agrees with this −41.9 dB. Here the test frequency is chosen so the folds land half-way between
  harmonics at the circuit's rate (1198.0 Hz at 8×; at exactly 1200 Hz every fold would land on a
  harmonic, 352.8 kHz = 294 × 1.2 kHz). The rectified output misses the rule by 7.6 dB because the
  octave takes 9 dB off the fundamental the rule is measured against (H1 −22.4 dB at the output against
  −13.4 dB at the tap) while stage 1's aliases pass through. Tried: the rectifier (stateless) solved
  alone at 4× the circuit's rate: −59.3 dB for +3.5 µs per sample, dropped. **For the maintainer**:
  ship at 8× as decided (the test holds the measured −57.4 dB as a regression mark), or 16× (−66.6 dB,
  ≈ 2× the CPU, ≈ 75 % of a core). Note for gfx-fuzz: its alias test uses 1200 Hz at 44.1 kHz too, where
  the in-circuit folds at 8× land exactly on harmonics; its figures are worth measuring again this way.
- **CPU** (i9-13900H performance core pinned, shared machine ±8 %; 48 kHz, mono):
  - the netlist alone first (O3, `cpu_octave_netlist`: 8× = 384 kHz, pink 0.1 RMS and a 1.2 kHz sine,
    `fuzz` 70 / 100 %, buffer off / on): full Ebers–Moll Q2, no cheap step 9.0 – 10.9 µs per host
    sample; with `set_fast_solve` + `set_quadratic_seed` 8.9 – 10.9 µs (no gain here: 2.4 iterations of
    stage 1, 1.8 of stage 2 either way); with the **forward-only Q2** (5 + 2 ports) **8.0 – 8.7 µs**
    (−18 %). Silence costs 2.7 – 3.0 µs (the linear algebra of 18 states).
  - the pedal (gfx-bench, blocks of 128): **7,678 – 7,884 ns per sample mono (37 – 38 %)**, 7,679 –
    7,917 ns per stereo frame (L == R, linked). Worst block of 128 in `tests::cpu_pedal`: 17 – 32 µs
    per sample on the loaded machine (one block above the 20.8 µs period: pink-noise peaks reach 50
    iterations in a sample, 30 – 430 fallbacks per second of noise).
  - **Per-pedal exception**: above the ≈ 20 % contract, inside Raph's accepted 30 – 40 %. One instance per
    lane, mono.
- Bench: 28 / 28. **Deviation**: click bound −14 dB (gfx-fuzz −20): `input` ±24 dB −20 / −20 / −16 /
  −15 dB re peak at 44.1 / 48 / 96 / 192 kHz, `fuzz` 0 ↔ 100 % −18 dB at 96 kHz. A drive step moves the
  coupling capacitors and the input clamp and with them the octave (H2 − H1 swings by ≈ 30 dB between
  30 mV and 0.3 V): HF content that really changes. Latency check skipped (minimum-phase oversampling).

**Not done.**
- §5 rows not run: the zipper sweep (the bench's parameter steps cover `octave`), the 64× monolithic
  reference render, lead.wav through the guitar and the listening rows (golem's DIs are not in the
  repository), the real-pedal / NAM `octavia` comparison (O2).
- L_m measured (O1, Raph), core saturation (only if the knee is below 13 mWb), the cable on the
  rectifier (after listening).
- golem-rack: the catalogue entry (a new block type `octafuzz`, category drive).
