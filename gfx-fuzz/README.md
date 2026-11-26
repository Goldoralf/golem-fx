# gfx-fuzz

> **Status: CODED** (v0.1.0, 2026-11-15). Designed and reviewed 2026-11-14 (Fable 5.2, Review at the
> end; figures marked **[P]** come from its prototype). Built on the shared gfx-dsp `circuit_nl`
> (gfx-dist's solver, extended without changing a bit of gfx-dist's renders) and a new shared gfx-dsp
> **`pickup`** module (the guitar's network, the EMF filter; gfx-acoustic will reuse it). Two models
> ship: **Ge Pair** (graded germanium face, 8×) and **Si Four** (the muff, solved whole, 4×, five
> stacks). Every DC figure of the review reproduces (4.50 V trimmed; 3.33 / 4.50 / 6.67 V over 15 / 25 /
> 35 °C; leakage 3.01 / 4.50 / 7.64 V; raw DAFx-17 pair 8.18 V; silicon gate 0.65 V at 4.5 V), the
> cleanup reproduces (germanium THD −7.1 → −16.9 → −23.2 dB at `guitar_vol` 100 / 50 / 30 %, buffered
> −2.3 / −7.4 / −10.4 dB), aliases −69.4 dB (Ge Pair) and −70.7 dB (Si Four) at 1.2 kHz, the bench is
> green (28 / 28). CPU on a performance core, bench defaults: **4.4 µs per sample (21 % of a core)**
> for Ge Pair, ≈ 4.9 µs (23 %) for Si Four. **Si Pair is coded but not shipped**: its aliases need 16×,
> where it costs ≈ 9 µs per sample (44 %). Ge Three is not coded (no netlist entered from a named
> document). Deviations and numbers: "As coded" below.
>
> **Octave Up: DESIGNED 2026-11-26** (roadmap 0.8, item 1; not coded). It is now the **transformer
> octave**, the circuit of the Octavia / Octavio this item names, and no longer the phase-splitter type that
> open question 4 first chose. Its netlist comes from madbean's Retrograde24 (2024), which prints the values
> and measured voltages, and from PCB Guitar Mania's Octavius (2021) schematic, which gives the topology.
> The prototype's DC point matches Retrograde24's measured voltages to 0.1 – 0.2 V. The transformer is
> lightly loaded, so it becomes one inductor plus ideal buffers, a cut that circuit_nl already makes
> exactly. That means **no gfx-dsp change**, and every shipped model keeps its renders bit for bit. The
> model runs at 8×, measured at −76 dB of alias at 1.2 kHz. Its estimated cost is **8 – 10 µs per sample,
> above the contract's ≈ 20 %**, which is open question O3. The magnetising inductance is [U], so the part
> needs to be measured. §2.3 "Octave Up" has the details; the questions are listed at the end.
>
> **Octave Up reviewed 2026-11-26** (last section). The netlist is confirmed on a third document that prints
> its schematic (Aion FX Octagon), every prototype figure reproduces, and the design stands. Two things
> keep it from shipping, and no document settles either: the CPU, twice the contract's bound, and L_m.
>
> **Decided by Raph, 2026-11-26.** Octave Up ships as a **separate thin pedal, gfx-octafuzz** ("GoleM FX
> Octafuzz"), over this crate's engine (gfx-fuzz is already an rlib): the netlist is an internal circuit
> kind that gfx-fuzz's `model` does not list, and gfx-fuzz's `model` enum, face and UI bundle stay as
> shipped. `octave` goes on gfx-octafuzz's face. The DSP sections below (§2.3 "Octave Up", §2.7, §2.8)
> stay here as the engine's design; a `gfx-octafuzz/README.md` stub will point here at coding time. CPU
> (O3): the netlist is coded in gfx-bench first and measured; a per-pedal CPU exception for gfx-octafuzz
> is accepted if the figure after the cheap steps stays above the contract (expected 30 – 40 %). L_m
> (O1): Raph measures one 42TM022; until then the 1 H / 0.1 – 0.5 H estimates stand, and the Octave
> row's marks are derived again afterwards.

## As coded (v0.1.0)

**Files.** gfx-dsp: new `pickup.rs` (5 unit tests); `circuit_nl.rs` extended (+ 2 unit tests). Here:
`circuits.rs` (the face and the muff netlists, the guitar and the supply in front), `lib.rs` (the
processor: one compiled circuit per (model, pickup, buffer, stack) per channel, 48 in all, built in
≈ 6 ms at `prepare`; two slots crossfade), `plugin.rs` (class id `GoleMFxFuzz00001`, mono 1 / 1 then
2 / 2, latency 0, read-only meters `bias_v` and `supply_v`), `tests.rs` (13 behaviour tests, 6
diagnostics).

**gfx-dsp changes** (gfx-dist's renders null bit for bit on golem's crunch DI, every circuit, gain 50 /
100 %, before and after; its 19 tests pass):
- `BjtModel` gains the emission coefficients `nf`, `nr` (DAFx-17's sets have 1.17); the temperature law
  uses NF as §2.5 decided. ngspice's BJT law has no NF, so `to_spice()` writes Eg / NF and XTI / NF.
  `BjtModel::iceo()` gives the open-base current builders measure.
- Up to 12 variable resistors per stage (the muff has 9: guitar volume, sustain, tone, mids, sag, bias).
- Probes (`Netlist::probe`, `NonlinearCircuit::probe`): meters read once per host sample, not solved at
  every oversampled one.
- Speed, bit-exact: the dot products run four to eight rows abreast (each row still summed in index
  order), K·Jf sums only Jf's device blocks (the skipped terms are exact zeros), the port count is a
  compile-time constant in Newton, the LU's inner loops without bounds checks. ≈ 1.6× on the solve.
- Opt-in, used here only: `set_fast_solve` (reciprocal pivots: one division per pivot) and
  `set_quadratic_seed` (a three-point seed when the three previous samples were plain Newton; −15 % of
  iterations on the muff, little on the faces). A chord variant (reusing the factors) was tried and
  dropped: on the faces it cost more iterations than it saved.

**The source model (§2.2).** `pickup::Guitar` stamps R (+ L), C_pu, the tone pot full up, the volume
pot's track (A law of gfx-dsp `pot`, ends at 10 Ω) and the cable into the netlist; `EmfFilter` rebuilds
the EMF at the netlist's rate. **Deviation**: the two Butterworth poles sit at **48 kHz**, not 20 kHz:
at 20 kHz they are −1.5 dB at 16 kHz, which breaks the design's own invariant (0.1 dB to 16 kHz);
measured at 48 kHz, every pickup, 176.4 – 768 kHz: within 0.1 dB, 20 Hz – 16 kHz, in the frequency
domain and through the real filter and trapezoidal netlist. The single coil's DI is the EMF +3.5 dB at
4 kHz and −14 dB at 10 kHz, and the load table of §2.2 (face 8 kΩ, muff 130 kΩ) reproduces within
0.3 dB. Active = a 10 kΩ resistive source, the same filter (first order then). **Deviation**: with
`buffer` on, the guitar is still in the netlist, its own linear stage before the buffer (an exact cut at
the buffer's output), so at `guitar_vol` 100 % the pedal's input is the DI within 0.1 dB, not
bit-exact: one path for every knob position, no switch.

**The circuits.** The face: §2.3's values (R3 the trimmer: **13.02 kΩ** germanium, **6.23 kΩ** silicon,
recomputed by bisection in `tests::trimmers_give_4v5`; the 470 Ω tap from the supply, as the
prototype), junction capacitances 100 / 410 pF (Ge) and 5 / 10 pF (Si), the graded pair (Q1 βF 75,
I_CEO 80 µA; Q2 βF 120, 200 µA at 22.8 °C, so Is = 4.54 / 7.28 µA). The muff: the review prototype's
typical NYC values [C / U] (no junction capacitances: its 470 pF dominate), the sustain pot linear [U],
the first clipper's 100 kΩ base resistor as the trimmer (stock, not re-trimmed). Stack values from Kit
Rae's version notes (bigmuffpage.com, parts 1 – 3), C8 / C9 its standard 10 nF / 4 nF: **Classic** 39 kΩ
low-pass / 22 kΩ high-pass [low-pass U], **Early** (1974 violet Ram's Head, "tone resistors enlarged to
39k") 39 / 39 kΩ, **Triangle** (the 1971 standard V1, "33k at R5 and R8") 33 / 33 kΩ, **Russian** (Green
Civil War, "R8 almost all 20k") 20 / 22 kΩ; the Russian's 390 Ω emitters and other version differences
are not modelled (`stack` is the stack only). `mids`: 0 – 22 kΩ in series with the low-pass capacitor.
Polarity: both shipped circuits are non-inverting in mid-band (linearized phase at 300 Hz: +15° face,
−21° muff); POLARITY = +1. **Deviation**: §5 checks the phase at 1 kHz; there the muff's own stack and
coupling network turn it by 90 – 110°, so the test runs at 300 Hz (both models within ±25° of the dry).

**Oversampling** (−12 dBFS, `fuzz` 100 %, `buffer` on, worst alias 20 Hz – 20 kHz re the fundamental,
44.1 kHz host):

| Model | 4× | 8× | 16× | Shipped at |
|---|---|---|---|---|
| Ge Pair, 1.2 / 5 kHz | −54.1 / −34.6 dB | **−69.4** / −47.3 dB | −106.2 / −63.4 dB | 8× |
| Si Pair, 1.2 / 5 kHz | −40.6 / −25.6 dB | −56.3 / −32.0 dB | **−101.1** / −43.5 dB | (16×, not shipped) |
| Si Four, 1.2 / 5 kHz | **−70.7** / −61.6 dB | −103.5 / −89.3 dB | −121.9 / −118.5 dB | 4× |

The prototype's figures reproduce within 1 dB. Ge Pair passes the −65 dB rule, 0.6 dB short of the
−70 dB target; Si Four meets the target. Newton on these runs: no unsolved sample, no fallback.

**CPU** (48 kHz, mono, pink noise at 0.1 RMS as the bench, pinned to a performance core of the
i9-13900H; an efficiency core is ≈ 2× slower, and the shared machine moves figures by ±8 %):

| Model | `buffer` off (default) | `buffer` on | Mean iterations | README bound |
|---|---|---|---|---|
| Ge Pair (8×) | 4.2 – 4.5 µs | 5.3 – 5.7 µs | 2.4 | 2.5 µs |
| Si Four (4×) | 4.8 – 4.9 µs | 5.1 µs | 1.5 | 4.5 µs |
| Si Pair (16×) | 9.1 µs | 10.5 µs | 2.6 | 4.5 µs |

The bench's line: 4,423 ns per sample mono pinned (4,911 unpinned), 8,877 ns per stereo frame. The
cost is the exponentials and a 4 × 4 or 10 × 10 Jacobian solve per iteration, at ≈ 2.4 iterations per
oversampled sample on the faces (the face's Q2 enters and leaves saturation on every cycle). Ge Pair is
at the contract's ≈ 20 %; Si Four is 15 % above it and ships for the maintainer to decide (the next step is §2.8's
block Gauss-Seidel); Si Pair, at twice the bound, waits for the solution cache of §2.8.

**Linked mono** (after v0.1.0, from golem's first live test). REAPER hands every block 2-in / 2-out;
on a guitar lane the mono DI arrives as L == R, and two circuits (≈ 8 µs per sample each on the
maintainer's loaded machine, ≈ 16 – 22 µs per stereo frame) overran the 20.8 µs sample period at 48 kHz:
silence with faint stutters live, offline renders fine. The rule, decided per sample (so the output does
not depend on the block size):
- After `prepare` / `reset` a stereo instance is linked: channel 0's circuit runs alone and its output
  is copied to channel 1 (the meters read channel 0, as before).
- Linked, the first sample whose L and R differ (bit comparison) unlinks: channel 1 takes a copy of
  channel 0's whole state, without allocating (every compiled circuit's states, pots, devices and supply
  by `NonlinearCircuit::copy_state_from`, both slots' oversamplers by gfx-dsp's new
  `OversamplerN::copy_from`, the tilt, the bass-keep split, the DC guard), and runs on from there with its
  own input. Its history was channel 0's, so this is exactly true stereo, with no step of any kind.
- Unlinked, after 60 ms of bit-identical L and R, channel 1's output fades linearly onto channel 0's in
  30 ms (`FADE_MS`), then channel 1 stops; if L and R differ during the fade it runs back at the same
  rate. After such a relink channel 1 plays channel 0's output, not what its own different past would
  have given (a decaying tail at most): the only case where linked mono is not bit-exact with true
  stereo, and only after 60 ms of identical inputs.
- Nulls against v0.1.0 (48 kHz, blocks of 192 as golem's REAPER, 2.4 s of pink noise with a knob, a
  model, a mix, a bass-keep and a temperature change on the way): mono, stereo L != R, stereo L == R, and
  L == R switching to L != R at 0.85 s (inside a model change's settle): **bit-identical**. Tests:
  `linked_stereo_is_the_mono_output` (bit for bit), `link_switches_are_exact_and_click_free` (channel 1
  bit-identical to true stereo up to the relink; HF burst at each transition −60 dB or less re peak, bound
  −20 dB; no sample step above channel 0's own), `linked_mono_is_block_size_invariant` (blocks 1, 64, 512,
  4096, bit for bit, transitions included). Bench 28 / 28.
- CPU (same loaded machine, unpinned, 48 kHz, pink noise, blocks of 64): mono 7.9 µs per sample;
  stereo L == R **16.5 → 8.2 µs per frame**; stereo L != R 15.7 – 15.9 µs (unchanged: two circuits). The
  bench's line: 8,401 ns mono, 7,996 ns per stereo frame (its stereo feeds L == R). The tone tilt's two
  gains are now recomputed only when `tone` moves (bit-exact; within the noise).

**Other measurements.**
- DC (25 °C, 9 V; `tests::operating_points_follow_the_design`): Ge Pair 4.50 V; 15 / 25 / 35 °C 3.33 /
  4.50 / 6.67 V; leakage 0 / 50 / 100 / 200 / 400 % 3.01 / 3.76 / 4.50 / 5.98 / 7.64 V; `bias` ±100 %
  6.67 / 0.42 V (Si 6.59 / 0.71 V); 4.5 V supply Ge 2.56 V, Si 0.65 V (the gate); Si 18 V 12.76 V; Si over
  15 → 35 °C 4.37 → 4.63 V; raw DAFx-17 pair, stock 8.2 kΩ: 8.18 V; Q2's I_CEO 200 µA.
- Cleanup (200 Hz, EMF 50 mV, `fuzz` 100 %, single coil): THD −7.1 / −16.9 / −23.2 dB at `guitar_vol`
  100 / 50 / 30 %, level −14.4 / −15.5 / −16.7 dB; buffered −2.3 / −7.4 / −10.4 dB (the prototype: −7 →
  −12 → −17, buffered −2 / −7 / −10).
- Level: K = −2.5 dB (Ge Pair), −11.3 dB (Si Four) on gfx-dist's DI proxy (golem's DIs are not in the
  repository); `level` −K is the hardware volume fully up.
- Muff stack (linearized whole pedal, sustain 0): scoop 17 – 19 dB below the 100 Hz level at noon,
  13.5 – 14.5 dB with `mids` 100 %. The README's 8.7 dB is the stack alone; the pedal's coupling
  network deepens it. Not checked against a Laplace stack within 0.25 dB (the row of §5).
- Sag 100 %: the supply 7.97 V at rest (the circuit's own current through 2 kΩ), 7.84 V under a hard
  note, back within 50 mV in 400 ms.
- Newton (`tests::newton_solves_the_hard_inputs`): +24 dB, DC, 0.7 RMS pink noise, at 3 / 18 V, 0 / 50 °C,
  leakage 400 %, buffered full fuzz: no NaN, no unsolved sample. Worst sample 25 – 30 iterations.
- Switches: the incoming slot runs 60 ms unheard before its 30 ms fade, so its coupling capacitors have
  settled on the signal (a model change measured −17 dB of HF burst without it, under −35 dB with it).
- Bench: 28 / 28. **Deviation**: click bound −20 dB, not −35: an `input` step of ±24 dB (−21 dB at
  96 kHz) or `guitar_vol` 0 → 100 % (−31 dB at 192 kHz) moves 24 dB of signal into or out of the face,
  whose coupling capacitors charge and shift its bias for ~100 ms: it blocks (−20 dB for 50 ms) and
  recovers through a louder, brighter bias, the circuit's own transient. `input` smooths over 100 ms
  (20 ms in §3). Model, buffer and bias steps stay below −35 dB.

**Not done (for later or for the maintainer).**
- Ge Three: no netlist entered from a named clone document; Octave Up: designed 2026-11-26 (§2.3),
  to code as the engine of the separate pedal gfx-octafuzz (not a `model` option); Si Pair: CPU. The `model`
  enum lists the shipped ones in the README's order (Ge Pair, Si Four), as the coding brief asks;
  appending Si Pair later puts it after Si Four, against the README's order, or reorders the enum
  (contract: enums only grow at the end): **The maintainer decides** before Si Pair ships.
- §5 rows not run: ngspice fixtures through `to_spice()` and the transient against the offline reference
  (the DC figures are checked against the review's independent solve instead), the real-knob proxy,
  the source-noise row, the zipper sweep, the worst-block CPU, the gate's 40 dB-faster decay (the gate
  is tested only as "no unsolved sample, finite"), the listening tests.
- `voltage`, `bias`, `temp`, `leakage`, `sag` glide through one-poles (300 / 100 ms) rather than the EXP
  rule; the meters are a 50 ms average (`bias_v`) and the instantaneous supply.
- Voicing by ear: the graded pair, the muff's [U] values, the sustain taper, K on real DIs.

golem's **fuzz**. It is a new block: five classic fuzz circuits, simulated from their schematics as
transistor networks, including the guitar in front of them. The aim is the part of fuzz that every
digital fuzz gets wrong: the pedal reacts to the **pickup and the guitar's volume knob**, and to its own
**bias, temperature and battery**. It is not a waveshaper: the sound comes out of the circuit's
equations.

Families, by generic name (no trademark on the pedal): the two-transistor germanium and silicon
**"face"** fuzz, the three-transistor germanium **"bender"**, the four-transistor sustaining **"muff"**
with its tone-stack variants and a mids control, and a transformer-rectified **octave-up** fuzz. The
gated "velcro" sound is not a separate model: it is a starved supply and a cold bias, available on every
model (§2.6).

Markers: **[V]** read in the source or the paper · **[M]** manufacturer page or manual · **[C]** DIY clone
schematic or kit document · **[A]** calculation (method stated, script reproducible) · **[F]** forum or
review opinion · **[U]** not verified · **[P]** measured on the review's prototype (numpy, full MNA +
Newton with trapezoidal companions: the same equations as the DK solve; for Octave Up, the design
prototype of 2026-11-26 in `proto/`: `octave.py` and the `oct*.py` scripts, same simulator plus coupled
inductors).

## 1. State of the art

Survey of 2026-11-14.

### 1.1 Why fuzz is hard to simulate

- **The input is a current, not a voltage.** The face circuit's first transistor takes its base current
  from the guitar. Its input impedance is low: about 5 – 8 kΩ, depending on the fuzz pot ([ElectroSmash
  analysis, via its summary; ElectroSmash was unreachable when checked] [F/U]; a DIY measurement gives
  ≈ 8 kΩ [F]). So the pickup's inductance and the guitar's volume pot become part of the circuit.
  - Pickup ≈ 2.5 H + 6 kΩ (single coil) into 8 kΩ: a first-order low-pass at (6 + 8) kΩ / (2π · 2.5 H)
    ≈ 890 Hz [A, typical pickup values U]. The pickup's high end is gone before anything clips.
  - Turning the guitar's audio-taper pot to 70 % of its travel (wiper at 26 % of the track, exponential
    10 %-at-half law) puts 48 kΩ (250 kΩ pot) or 96 kΩ (500 kΩ pot) of source resistance in front of
    the 8 kΩ input [P]. The pedal loses **17 dB (250 kΩ) or 22 dB (500 kΩ) more** than a buffered input
    would. Two things then happen together [P, §2.2]: the first transistor's base is driven through
    a large resistance, i.e. by a current, and a transistor is linear in its base current where it is
    exponential in its base voltage (at full fuzz the emitter capacitor removes the feedback, so
    nothing else linearises it); and the pot's resistance isolates the pickup's inductance from the
    8 kΩ load, so the treble comes back (single coil, re the full-up response: −24 dB at 100 Hz but only
    −6 dB at 10 kHz). Less drive, a more linear first stage and more top: the "glassy clean with the
    guitar's volume" that players praise [F].
  - Behind a buffer, the same knob only lowers the level (−12 dB at 70 %, −20 dB at 50 %), the fuzz
    stays in clipping longer and the top does not come back: the cleanup is "harsh" or absent [F].
- **Fractal says it outright.** Cliff Chase, quoted on the Fractal forum: "No digital device will do a fuzz
  properly because the input impedance is nonlinear. Digital devices necessarily have an input buffer"
  [F, second-hand]. The Axe-Fx III answers with **hardware**: an input-impedance setting "Auto" that
  follows the first block. Its firmware 4.00 notes require the fuzz to be the first block after Input 1,
  with the input impedance set to Auto [F, quoted note].
- **Neural DSP's Quad Cortex** users complain that its fuzzes "do not react the same way fuzzes do in
  real life", have "too much high end", and that the impedance is a global setting [F,
  [unity.neuraldsp.com](https://unity.neuraldsp.com/t/fuzz-models/9119)].
- **golem has no hardware impedance.** The guitar reaches golem through an audio interface's Hi-Z input,
  which is a buffer. The answer here is the **source model** (§2.2): the DI recorded at a Hi-Z input is
  the guitar's open-circuit output voltage (resonance and cable included), so the guitar's impedance
  can be put back in software, in front of the simulated transistors, and only the **difference**
  between the interface's load and the pedal's load is applied.

### 1.2 Papers

| Work | What it gives us |
|---|---|
| Holmes, Holters & van Walstijn, *Comparison of Germanium BJT Models for Real-time Circuit Simulation*, DAFx-17 ([PDF](https://www.hsu-hh.de/ant/wp-content/uploads/sites/699/2017/10/Holmes-Holters-van-Walstijn-2017-Comparison-of-Germanium-Bipolar-Junction-Transistor-Models-for-Real-time-Circuit-Simulation2.pdf)) [V, table 1 and table 4 re-read at the review] | Measured AC128 / OC44 parameters, Ebers-Moll vs Gummel-Poon. AC128, Ebers-Moll: Is 23.75 µA, βF 44.90, βR 4.568, NF 1.168, NR 1.171, at V_T = 25.5 mV (**22.8 °C**: the set's reference temperature). OC44, Ebers-Moll: Is 2.029 µA, βF 108.1, βR 8.602, NF 1.062, NR 1.105. Gummel-Poon (AC128: βF 229.6, Ise 2.19 µA, Isc 7.55 µA, C_cb 100 pF; OC44: C_eb 410 pF, C_cb 10 pF) costs 2.2× (Fuzz Face) to 3.9× (Rangemaster) more. Listening at **8× oversampling**: the models differ audibly, plain Ebers-Moll has the most high end. Constant junction capacitances (AC Ebers-Moll) cost little. **"Any first extension to the Ebers-Moll model should be AC effects."** Fuzz Face, nodal DK method, damped Newton: 3.6 iterations per sample on average, on a guitar chord at 10 – 100 mV |
| Holmes & van Walstijn, DAFx-15 (robust iterative solver for distortion circuits); DAFx-16 (Rangemaster parameter optimisation); DAFx-19 (potentiometer laws, Big Muff tone stack measured on a breadboard, [page](https://bholmesqub.github.io/DAFx19)) [V, from abstracts] | Newton damping; parameters fitted to measurements; real pot tapers |
| Yeh, *Digital Implementation of Musical Distortion Circuits by Analysis and Simulation*, PhD Stanford 2009; Yeh, Abel & Smith, K-method, IEEE TASLP 2010 [V, known; also cited in gfx-drive] | Nonlinear state space from circuit equations: the basis of every DK-method |
| Holters & Zölzer, *Physical Modelling of a Wah-wah Pedal … Nodal DK Method … with Variable Parts*, DAFx-11 [V, title]; Holters & Zölzer, generalized state-space derivation from schematics, EUSIPCO 2015 → [ACME.jl](https://github.com/HSU-ANT/ACME.jl) [U, licence not checked] | A netlist → nonlinear state space, automatically; pots changed at run time |
| Werner, Nangia, Smith & Abel, *Resolving Wave Digital Filters with Multiple/Multiport Nonlinearities*, DAFx-15 ([abstract](https://dafx.de/paper-archive/details/4fsq4pZNdCbUzyzAL8xsQg)) [V, abstract] | A WDF of the Big Muff clipping stage (one BJT + a diode pair). The WDF alternative to the DK-method |
| Dempwolf, PhD Hamburg 2012, Fuzz Face example ([PDF](https://www.hsu-hh.de/ant/wp-content/uploads/sites/699/2019/10/Dempwolf_Diss_Druckversion.pdf)) [V, table of contents] | A discrete state-space Fuzz Face |
| Parker et al., Bilbao et al. (ADAA); Holters, *ADAA for stateful systems*, DAFx-19 [V, cited in gfx-drive] | ADAA does not apply simply to a stateful circuit: we oversample instead (§2.4) |

### 1.3 Open source

| Project | What it does | What we take |
|---|---|---|
| [ChowDSP BYOD](https://github.com/Chowdhury-DSP/BYOD) (GPL-3) [V, read 2026-11-14] | **Muff Drive**: 1 – 4 clipping stages; each one is an ideal amplifier of gain −Rc/Re (−10 k / 150) with the diode pair and 470 pF in its feedback, solved by a scalar Newton (4 or 8 fixed iterations). **Fuzz Machine**: a neural black box of a custom fuzz. The repository also holds a **Fuzz Face nodal-DK model** generated from a netlist (C1 2.2 µF, C2 20 µF, C3 10 nF, R1 33 kΩ, R2 470 Ω, R3 8.2 kΩ, R4 100 kΩ, 1 kΩ fuzz pot, 500 kΩ load; Ebers-Moll, Is 17 µA, βF 90 / 100; Newton until Δ < 10⁻² or 8 iterations). **Bass Face**: neural | Confirms the face netlist values and that a 4-port DK solve runs in real time. BYOD models no pickup, no bias, no temperature: its face sees an ideal voltage source |
| [chowdsp_wdf](https://github.com/Chowdhury-DSP/chowdsp_wdf) (BSD-3) [V, cited in gfx-drive] | WDF library, R-type adaptors | The WDF route, not taken (open question 3) |

### 1.4 Plugins and modelers

| Product | Fuzz offer | Notes |
|---|---|---|
| Line 6 Helix [M, model list] | Arbitrator (face), Triangle and Bighorn (muff variants), Tycoctavia (octave), Industrial, Ballistic, Pocket, Wringer, Thrifter, Xenomorph… | Separate models. A "Guitar In-Z" setting with an Auto mode [U]. No bias or temperature control [U] |
| Fractal Axe-Fx III [F] | Face, Bender, Pi, Octave fuzz types in its Drive block, with generic Bias / Clip type controls | Variable hardware input impedance (§1.1) |
| Neural DSP Quad Cortex [F] | Fuzz models and captures | Players say it is "not there yet" for fuzz |
| Kuassa Efektor FZ3603; Audiority Blue Face; Mixwave Benson Germanium Fuzz (with a mix knob players praise); Gorilla DSP Old Vintage Fuzz (2025) [M / F, roundups] | Single-family germanium fuzzes | None of them documents pickup loading |
| golem today | NAM captures through the `pedal-nam` archetypes `fuzz-face`, `big-muff`, `tone-bender`, `octavia`, `bass-fuzz` (catalogue v46) [V] | A capture is made through a re-amp box (a buffer): it records the fuzz **without** its guitar interaction, and cannot have bias, temperature or a guitar volume |

### 1.5 What players love and hate [F]

- **Love**: the guitar's volume knob as the clean channel; "touch" (pick softly, it cleans up); the
  sputtery decay of a cold or starved germanium face; the muff's sustain; the octave's ring on the neck
  pickup above the 12th fret.
- **Hate**: digital fuzz is fizzy and too bright (the missing pickup loading); a buffer in front kills
  the face; germanium pedals change with the weather (accepted as charm in a simulation, as long as the
  player chooses it); the muff disappears in a band mix (the mid scoop: hence the mids mods); the octave
  is messy on chords.

### 1.6 What we conclude

1. **Simulate the circuit, guitar included.** The interaction is the product: the coil's EMF rebuilt
   from the DI, the guitar's own network (coil, pots, cable), then the transistors.
2. **Ebers-Moll with junction capacitances**, plus germanium leakage and temperature laws. The DAFx-17
   result says this is the first extension worth having; Gummel-Poon's DC terms cost 2 – 3× more for
   less audible gain.
3. **A nonlinear nodal DK solver** in gfx-dsp (`circuit_nl`, the same module as gfx-dist's), built from
   the netlist at `initialize`: one engine for every model, both pedals, and future circuits.
4. **The bias is a trimmer** set to the builders' target at 25 °C, so temperature and voltage move the
   pedal away from it as they would a real one.
5. **Oversample, do not ADAA**: the circuit is stateful. The factor is set per model from measured
   aliases (§2.4): only the muff's own capacitors band-limit its edges enough to run low.

### 1.7 How this differs from gfx-drive's Fuzz voice

gfx-drive's Fuzz voice is two cascaded static shapers with an inter-stage high-pass, alias-free with
ADAA, cheap, and morphable under EXP across every drive family. It stays the right block for "some fuzz
in a chain", and for continuous morphing.

gfx-fuzz does what a static shaper cannot:

| Behaviour | Why a shaper cannot do it | Where it comes from here |
|---|---|---|
| Guitar-volume cleanup | It is the **source impedance** changing the input current, not a level change | The guitar's network in the netlist, driven by the rebuilt EMF |
| Buffer before vs after | Same reason | `buffer` changes the source impedance seen by the input |
| Duty cycle and asymmetry that change with level and over a note | The coupling capacitors charge on hard hits and shift the **operating point** (blocking distortion) | Capacitor states in the solver |
| Gating, sputter, "velcro" | A starved, mis-biased transistor stops conducting at low signal | Supply voltage, sag and bias as circuit values |
| Germanium drift and leakage | Device physics | Temperature-scaled Is, Vt and leakage current |
| The octave's rectified tone, and its bloom on the decay | A transformer and two diodes behind a fuzz whose duty cycle moves with the level: the octave is pure only while the coupling capacitors hold the bias (§2.3) | In the netlist |

Cost: about 5 – 12× gfx-drive's CPU per channel (§2.8).

## 2. Design

```
DI ─▶ × input ─▶ ↑N ─▶ EMF filter (§2.2) ─▶ [ netlist, solved per sample: coil ─ guitar pots ─ cable ─ (buffer) ─ fuzz circuit (model) ─ (tone stack) ─ volume pot ]
                        supply: V(voltage) ─ R(sag) ─ reservoir ─▶ circuit's Vcc          (bias trimmer: a part; temperature, leakage: device constants)
                 ─▶ × polarity ─▶ ↓N ─▶ tone tilt (non-muff models) ─▶ mix / bass_keep (dry through the same up / down) ─▶ × K(model) × level ─▶ out
N = 4, 8 or 16, per model (§2.4)
```

### 2.1 Scale

0 dBFS = **1 V peak** at the pedal's input, as gfx-emet. golem's reference DIs peak at −3 to −8 dBFS (0.4 – 0.7 V),
real pickups. `input` (−12 to +12 dB) trims a hotter or weaker source. The output is in volts at the
pedal's volume pot fully up, times a fixed constant K per model.
- **`level` 0 dB is unity loudness, on every model** (gfx-dist's rule, adopted): K is fixed per model so
  that at the defaults the reference DIs come out at the bypass loudness (±1 LU), measured once on the
  bench and hard-coded. The circuits differ by more than 6 dB with their pots full up (prototype, full
  fuzz: Ge Pair ≈ −15 dBV, Si Pair ≈ −9 dBV, the muff ≈ −8 dBV [P]); "0 dB = the pot fully up" would make
  every `model` change a level jump, and a default cannot depend on `model` (contract). The table of K
  gives "the hardware pot fully up" in dB of `level` for each model.

### 2.2 The guitar in front: the source model

- **What the DI is [A, P].** A DI recorded on a Hi-Z input is the guitar's output voltage with its volume
  full up, loaded by its own pots, its cable and the interface's 1 MΩ. It is **not** the pickup's
  open-circuit voltage: it already holds the resonance of L_pu against the cable. Single coil, 500 pF
  cable: the DI is the coil's EMF +3.5 dB at 4 kHz, −7 dB at 7 kHz, −14 dB at 10 kHz; humbucker: +5.6 dB
  at 3 kHz, −20 dB at 10 kHz [P]. (The first draft took the DI for the EMF behind (R_pu + s·L_pu) ∥ C_pu
  and loaded it again with the pot and the cable: the resonance was applied twice, +4 to +7 dB at the
  peak and −12 to −40 dB above it, whatever the pedal's load [P].)
- **Model.** The guitar is one linear network, in the netlist:
  EMF e → R_pu + L_pu → pickup node; from that node to ground C_pu, the tone pot full up (a resistor: its
  capacitor is a short at audio) and the volume pot's track; the wiper → cable capacitance → pedal input.
  Write Y_rec for the admittance the pickup node saw **at recording** (C_pu + C_cable, volume pot, tone
  pot, 1 MΩ). Then DI = e / (1 + (R_pu + s·L_pu)·Y_rec), so
  **e = DI · (1 + (R_pu + s·L_pu)·Y_rec)**: a second-order inverse filter, made proper by two poles at
  20 kHz (Butterworth), run as one biquad at the oversampled rate (bilinear, as the netlist's own
  trapezoidal rule, so the two cancel exactly). Its gain is +14 dB at 10 kHz and +24 dB at 20 kHz
  (single coil; humbucker +20 / +30 dB) [P]; the simulated network takes it back, and what is left is
  what the real guitar would deliver into the simulated load.
- **Invariant (bench).** `guitar_vol` 100 %, load ≥ 1 MΩ: the pedal's input node is the DI within
  0.1 dB to 16 kHz, **whatever the `pickup` values**. A wrong guess of L_pu or of the cable therefore
  cannot colour the sound by itself: it only misplaces the *change* that the pedal's load makes. With
  `guitar_vol` at 100 % the model is exactly Thevenin's theorem at the guitar's jack: the DI behind
  Z_th = (R_pu + s·L_pu) ∥ 1 / Y_rec.
- **What the load then does [P]** (single coil, re the DI):

  | Load | 100 Hz | 1 kHz | 4 kHz | 10 kHz |
  |---|---|---|---|---|
  | Face, ≈ 8 kΩ, `guitar_vol` 100 % | −4.7 dB | −8.7 dB | −22.1 dB | −12.4 dB |
  | Face, `guitar_vol` 70 % | −28.7 dB | −29.0 dB | −32.8 dB | −18.1 dB |
  | Face, `guitar_vol` 50 % | −31.6 dB | −32.0 dB | −35.6 dB | −20.2 dB |
  | Muff, ≈ 130 kΩ, 100 % | −0.3 dB | −0.5 dB | −4.3 dB | −0.6 dB |
  | Buffer, 50 % | −20.1 dB | −20.5 dB | −24.3 dB | −10.1 dB |

  The face flattens the resonance and darkens the guitar; rolling the volume back costs 24 dB of drive
  and gives back 18 dB of treble relative to it. The muff only shaves the peak: for it the source model
  is a refinement, for the faces it is the sound.
- **`pickup`** sets the values (typical, [U]; refined against measurements if the maintainer has any). The same
  values serve at recording and in the simulation:

  | Option | L_pu | R_pu | C_pu | Volume pot | Tone pot | Cable | Resonance on 1 MΩ |
  |---|---|---|---|---|---|---|---|
  | Single coil | 2.5 H | 6 kΩ | 100 pF | 250 kΩ A | 250 kΩ | 500 pF | 4.1 kHz |
  | Humbucker | 4.5 H | 8 kΩ | 150 pF | 500 kΩ A | 500 kΩ | 500 pF | 2.9 kHz |
  | P-90 | 7 H | 8 kΩ | 120 pF | 500 kΩ A | 500 kΩ | 500 pF | 2.4 kHz |
  | Active | — (source 10 kΩ, resistive) | | | 25 kΩ A | — | 500 pF | — |

  Active: the guitar's preamp is already a buffer of ≈ 10 kΩ output impedance (EMG's published figure
  [M / U]); no inverse filter, the DI behind 10 kΩ and the 25 kΩ pot.
- **Taper.** Audio pots follow gfx-dsp `pot::a_taper` (exponential, 10 % of the track at half travel:
  26 % at 70 %). Track ends are clamped at 10 Ω so no branch reaches 0 Ω.
- **`buffer`** on: an ideal buffer (1 MΩ in, 100 Ω out) between the cable and the pedal. The guitar
  network is then linear and solved apart (at `guitar_vol` 100 % it is the identity: the DI goes
  straight in, no inverse filter), and the fuzz sees 100 Ω: what a pedal after a buffer hears.
- **`guitar_vol`** is the guitar's volume knob, rotation 0 – 100 %. It is the **EXP control** of this
  pedal: a foot on the guitar's volume, on the real impedance law. Both halves of the track change
  every base-rate sample (§2.4, pots).
- **What the source model cannot know.** If the player rolls the **real** guitar knob, the DI arrives
  attenuated and re-filtered, and the model still assumes the knob full up. The drop in drive still
  cleans the fuzz, but without the extra source resistance and with the wrong treble. The bench
  measures that error (§5, "Real-knob proxy"). Decision: no estimator in v1 (open question 1).
- **Noise.** The inverse filter lifts the interface's noise above 8 kHz by up to 24 – 30 dB before the
  network takes it back. Into a face the net is negative; at low `guitar_vol` the net lift is the real
  guitar's (≤ +10 dB at 10 kHz re the DI). Bench row "Source noise".

### 2.3 The circuits

Each model is a netlist in the source, component values cited. Circuit values marked [C] come from the
documented clone schematics; those marked [C / U] are entered at coding from the named document and
checked against the references of §5.

- **`model` Ge Pair (face, germanium).** Two PNP germanium transistors, direct-coupled, shunt-series
  feedback. R1 33 kΩ (Q1 collector), R2 470 Ω + R3 8.2 kΩ (Q2 collector; output from their junction),
  R4 100 kΩ (Q2 emitter → Q1 base), 1 kΩ fuzz pot in Q2's emitter with 20 µF on its wiper, C1 2.2 µF in,
  C3 10 nF out, 500 kΩ volume pot [C, matches BYOD's netlist V]. Junction capacitances constant: C_cb
  100 pF (DAFx-17's AC128), C_eb 410 pF (DAFx-17 gives it for the OC44 only; taken as the order of
  magnitude [U]).
  - **Devices: graded, not the raw DAFx-17 pair [P].** Two of the measured AC128 (β_F 45, I_CEO 262 µA
    by the model's own equations, §2.5) make a dead pedal: with stock values Q2's collector sits at
    8.2 V at 25 °C (5.9 V at 15 °C, 8.3 V at 35 °C; the draft's 7.95 / 5.25 / 8.32 V are reproduced with
    its reference temperature and law), and trimming it to 4.5 V takes 47 kΩ and leaves 0.3 V at
    15 °C. The bias is then the difference of two leakage currents. That part is
    what builders reject. The pedal's set is what they select [F]: **Q1 β_F 75, I_CEO 80 µA; Q2 β_F 120,
    I_CEO 200 µA**, with the measured N_F, N_R and β_R (so Is = 4.5 and 7.3 µA). With it, stock values
    give 6.1 V at 25 °C, and the trimmed pedal moves **3.3 → 4.5 → 6.7 V over 15 → 25 → 35 °C** [P]: a
    pedal that changes with the weather and still plays. The raw DAFx-17 pair stays in the tests, as
    the set whose waveforms the paper publishes. The graded figures are a voicing start [U]: The maintainer's
    ears, or a measured pair, decide.
  - **Bias trimmer: Q2's collector resistor** (the 8.2 kΩ, the part builders replace by a trimmer [F]),
    not Q1's as first written: on silicon Q1's resistor cannot reach 4.5 V at all (3.9 V at 100 kΩ,
    4.2 V at 200 kΩ [P]). Stock = the value that gives 4.5 V at 25 °C, 9 V, for the graded
    germanium set [P] (13.0 kΩ; 6.2 kΩ on silicon). The output tap (470 Ω) does not move with it.
- **`model` Si Pair (face, silicon).** The same circuit with NPN silicon transistors (BC108-class,
  β_F ≈ 300, Is ≈ 2·10⁻¹⁴ A [U]; C_cb ≈ 5 pF, C_eb ≈ 10 pF [U]). Stock values give 3.0 – 3.2 V at 25 °C [P,
  reproducing the draft]; trimmed (**6.2 kΩ**) the collector moves only 4.37 → 4.63 V over 15 → 35 °C
  [P]. More gain, brighter, harder edges, no drift [F]. On the prototype it cleans up **more** than the
  germanium pair with the guitar's volume (§5, Cleanup), where players say "less smoothly": to be
  heard, not assumed.
- **`model` Ge Three (bender, three germanium transistors).** A common-emitter input stage feeding a
  direct-coupled pair like the face's; the fuzz pot sets the last stage's gain. More gain and a raspier
  top than the face; it cleans up less [F]. Values [C / U] (MkII-type clone documents). Devices start
  from DAFx-17's OC44 set for the input stage (I_CEO 28 µA by its equations: a good part) and the graded
  pair for the other two [U].
- **`model` Si Four (muff, four silicon transistors).** Input booster, two clipping stages (two 1N914
  diodes anti-parallel and 470 pF between collector and base), the passive tone stack, an output booster
  [V, ElectroSmash's structure via its summary; values C / U]. Sustain pot 100 kΩ between the booster and
  the first clipper; volume 100 kΩ.
  - **Tone stack** (`stack`): a low-pass branch (R, C to ground) and a high-pass branch (C, R to ground)
    mixed by the 100 kΩ tone pot. The variants differ by their values: Classic (1970s), Early (1973
    "Ram's Head" era), Triangle (1969 – 70), Russian (1990s Soviet-made), and Bypass (the stack removed,
    flat and loud, a common mod [F]). Values per variant entered from Kit Rae's documented schematics
    ([bigmuffpage.com](https://www.bigmuffpage.com/)) [C / U].
  - **Scoop [A]** (nodal analysis, assumed Classic values: low-pass 22 kΩ + 10 nF, high-pass 3.9 nF + 22 kΩ,
    15 kΩ source, 100 kΩ load): at noon the response dips **8.7 dB below its 100 Hz level, minimum near
    1.6 kHz**. At the bass end of the pot it is a low-pass (−4.6 / −9.7 / −23 dB at 100 Hz / 1 kHz /
    5 kHz), at the treble end a high-pass (−18.7 / −14.7 / −9.0 dB). All reproduced [P]. The low-pass
    resistor is what moves the notch: with 39 kΩ (a value often quoted for the 1970s units [U]) the dip
    is 8.6 dB at 1.1 kHz [P]. Which value belongs to which variant is read from the documents, not
    assumed.
  - **`mids`** (0 – 100 %): a variable resistor, 0 – 22 kΩ, in series with the low-pass capacitor: the
    "resistor to ground" mid mod [F, Coda Effects]. At 100 % the scoop shrinks from 8.7 to 3.9 dB and
    moves down to ≈ 790 Hz [A, same assumptions; P]. A resistor in parallel with the pot (tried) does not
    fill the notch [A].
- **Octave Up: the transformer octave** (designed 2026-11-26; replaces the phase-splitter
  choice of open question 4, see "Why the transformer type" below). **Ships as the pedal gfx-octafuzz**
  (decided by Raph, 2026-11-26, O6): an internal circuit kind of this engine, not a `model` option of
  gfx-fuzz; its parameters are in §3 "gfx-octafuzz". A three-transistor silicon fuzz
  drives an audio transformer whose centre-tapped winding feeds two germanium diodes, a full-wave
  rectifier, into the volume pot. Octave strongest on single notes, neck pickup, above the 12th fret;
  chords give ring-mod-like intermodulation, as the real one [F].
  - **What the reference is.** The roadmap's reference, the MXR Octavio (M267, 2020, MXR Standard
    line), is presented as the circuit of Dunlop's earlier Hendrix Octavio (JH-OC1, the "cheese
    wedge") in a smaller box with true bypass [F: Guitar Interactive and Total Guitar reviews; Dunlop's page
    names no part, M]. Builders report a transformer in the JH-OC1, a 3.6 : 1 + 1 part, possibly by MCE
    Magnetics [F: freestompboxes thread 4041, read through a search summary, the forum refuses robots;
    U]. The circuit is the Tycobrahe Octavia's, attributed to Roger Mayer's 1967 pedal. The original is
    positive ground, with an MPSA18 and two 2N6519 [C: Fuzz Central's Octavia page]. The clone documents
    build a negative-ground conversion around a Xicon **42TM022** (1.5 kΩ CT : 600 Ω CT), "used in almost
    every Octavia clone in the past 20 years and found to perform identically to the type used in the
    original" [C: Aion FX Octagon]. The transistors differ per document: a 2N5087 and two 2N4401 (Fuzz
    Central, Retrograde24), a 2N3906 and two 2N3904 (Octagon). Roger Mayer's own later Octavias have no
    transformer and are not modelled.
  - **Netlist** [C: values from madbean's **Retrograde24** build document, 2024-11-21, BOM page; that
    document prints no schematic, so the topology is read from PCB Guitar Mania's **Octavius** schematic,
    v1.1, 2021-03-25, which has the same parts except R3 680 kΩ and other designators. Retrograde24's
    designators are used here]. Negative ground, 9 V. The review read Retrograde24's BOM and voltage pages
    again and checked every connection below on a third document, **Aion FX Octagon** 1.0.0 (2024-07-04),
    which prints its schematic: the same topology and values, with 820 kΩ / 680 kΩ / 180 kΩ on Q1's base as
    Retrograde24. Its differences are outside the model or small: a 2.2 MΩ input pull-down instead of 1 MΩ, a
    1 kΩ C fuzz pot, BAT46 in its parts list (1N34A on its schematic):
    - input: C1 100 nF in series; R1 1 MΩ from the input jack to ground; C2 150 pF from Q1's base to ground;
    - **Q1** 2N5087 (PNP), common emitter. Base bias: R2 680 kΩ to ground, R3 820 kΩ to a node that
      R4 180 kΩ ties to the supply, decoupled by 33 µF to ground. Collector load R5 220 kΩ to ground, with
      R6 220 Ω + C4 1 nF from the collector to ground: a first-order treble cut, the 1 nF shunting the
      220 kΩ load above ≈ 720 Hz (the source of the "tonalité en retrait"), floor set by the 220 Ω
      [A]. Emitter: R7 1 kΩ to node Y;
    - **the loop**: R8 47 kΩ from Q3's collector to Y. From Y, 100 µF to the 1 kΩ B **fuzz** rheostat
      (lugs 1 and 2 to ground; 100 % = R_MIN). Series-series negative feedback, as on the face. At full
      fuzz Y is an AC ground and only R7 degenerates Q1;
    - **Q2** 2N4401, an emitter follower: collector to the supply, base on Q1's collector, R9 22 kΩ from
      the emitter to ground;
    - **Q3** 2N4401, common emitter: base on Q2's emitter, R10 470 Ω ∥ C5 220 µF from the emitter to
      ground, R11 1.2 kΩ from the collector to the supply;
    - **transformer**: Q3's collector → 33 µF → the 600 Ω winding (end to end, centre tap unused) → ground
      (on Octagon's schematic: the part's S side, pins 6 and 4, pin 5 open; on its P side pin 2 is grounded
      and pins 1 and 3 go to the diodes).
      The 1.5 kΩ winding's centre tap is grounded and its two ends feed D1, D2 (1N34A, anodes on the
      winding) into **out**. The 500 kΩ A volume pot runs from out to ground, wiper to the jack. There is
      no output capacitor: the next input's coupling capacitor sees the rectifier's DC (see "DC guard"
      below). The octave switch of the clones is replaced by `octave` (§3). Octagon's has three positions:
      octave, one diode lifted (a half-wave), and that diode lifted with the other shorted (the winding
      straight into the pot: the "fuzz only" that Retrograde24 says is louder).
    - devices [U: common SPICE models; β from Retrograde24's measured hFE 169 / 173, its two 2N4401; the
      2N5087's is not measured. None of these constants matters much (review, `octreview.py devices`): the
      diode's Is over 25× or the disputed published model moves H2 − H1 by at most 0.7 dB, the 2N4401's β
      from 100 to 300 by 2.3 dB, and every β from 100 to 400 moves `bias_v` by 0.07 V]: 2N4401 Is 26 fA,
      β_F 170, β_R 1.0, C_cb 7.3 pF, C_eb 26 pF; 2N5087 Is 6.7 fA, β_F 254, β_R 2.7, C_cb 9.5 pF, C_eb
      9 pF; 1N34A Is 0.1 µA, N 1.3 (0.30 V at 1 mA, fitted to typical figures: the only published model, Is
      200 pA with N 2.19, gives 0.87 V at 1 mA and is disputed in its own thread [F]), Eg 0.67 eV.
    - **DC point [P]** (9.26 V: the rail Retrograde24 itself reads on Q2's collector, from a 9.42 V
      adapter; the design first assumed 9.17 V): Q1 E / B / C 4.32 / 3.77 / 2.91 V, Q2 E 2.33 V, Q3 C / E
      4.99 / 1.67 V (3.6 mA). The document measures 4.18 / 3.54 / 2.93, 2.38, 4.85 / 1.73 V. Q1's base
      reads 4 % low through a 10 MΩ meter on its 405 kΩ divider [A]. Its "Q3 B 3.04 V" cannot be right,
      since Q3's base is Q2's emitter [A]. With Octavius's R3 of 680 kΩ, Q3's collector would sit at 5.30 V
      and Q1's base at 4.11 V: Retrograde24's voltages fit 820 kΩ. At 9 V: Q3 C 4.87 V.
  - **The transformer: one inductor and an ideal ratio.** The 42TM022's turns ratio is √(1500 / 600)
    = 1.58 end to end, so each half of the 1.5 kΩ winding gives **a = 0.79** times the 600 Ω winding's
    voltage [A, from the impedances, M]. The secondary drives only two diodes into 500 kΩ (µA), so its
    current reflected into the primary is negligible. With the secondaries open, their voltages are
    exactly ±a / (1 + σ) times the primary's, so leakage drops out. The model is therefore:
    - stage 1: the guitar, Q1 – Q3 and the 600 Ω winding as **one inductor L_m** (+ 45 Ω DCR [U]), with
      a `Buffer` from its node into a divider of ratio a, then a `Buffer` → node s1. **Cut at s1**: an
      ideal source's output, exact in `circuit_nl`'s sense (§2.4);
    - stage 2: s1, an ideal inverting op-amp (two equal resistors) → s3 = −s1, D1 from s1 and D2 from s3
      into out, 500 kΩ. 2 ports, no state. The halves' DCR (65 Ω in all [U]) is negligible against
      500 kΩ and omitted.
    - **Checked against the full coupled solve [P]** (3 × 3 inductance matrix, leakage σ = 0.3 % of each
      winding's self-inductance, DCRs, 50 pF per half: `octcut*.py`). Against the monolithic solve without
      winding capacitance the cut nulls at **−37 to −58 dB** re the output (440 Hz at 30 mV – 0.3 V, 1.3 kHz,
      82 Hz). With the winding capacitance as well, the harmonic levels H1 – H4 agree within 0.1 dB, but
      the null is only −6 to −19 dB. That is not the cut: the null of Cw = 0 against Cw = 50 pF is the same
      number. It is not audio either (review): 50 pF rings against the leakage inductance at 265 kHz,
      above the prototype's Nyquist frequency, where the trapezoidal rule folds the ring to 78 kHz. Below
      20 kHz, Cw = 50 pF differs from Cw = 0 by −57 dB of the output, at 176.4 kHz and at 1.41 MHz alike
      [P, `octreview.py cw`]. Winding capacitance is [U] and dropped. No coupled-inductor element is
      needed in `circuit_nl` (§2.9).
    - **The rectifier's output is not loaded, by decision (review).** The real pedal drives a cable and
      the next input from a 500 kΩ node behind two diodes: a peak detector. 1 MΩ changes nothing. 470 pF
      (about 4 m of cable) lowers the output by 1 – 2.6 dB and fills the fundamental's null on high notes:
      H2 − H1 at 659 Hz, 50 mV goes from +20.5 to +15.6 dB, and to +9.5 dB with 1 nF; at 440 Hz and below
      470 pF moves it by less than 2 dB [P, `octreview.py load`]. With that capacitor the cut is no longer exact: the
      diodes charge it through the leakage inductance, and the cut differs from the coupled solve by
      −15 to −18 dB (H2 – H4 by 0.2 – 1.5 dB) [P, `octreview.py cutband`], with a leakage that is [U]. The
      cable belongs to a rig, not to the pedal, and golem's next block is a buffer: the model stops at the
      pot. Any comparison with a real pedal or a capture must state its cable (open question O2).
    - **L_m [U]: the number that matters, and nobody publishes it.** The datasheet gives impedances and a
      300 Hz – 3.4 kHz telephone range (Lm ≥ 0.16 H if 300 Hz were the −3 dB point at 600 Ω / 600 Ω [A]).
      The DCR is quoted as 65 / 45 Ω and as 55 / 30 Ω by two distributor tables [U]. Design value **1 H** on the
      600 Ω winding. L_m forms a high-pass with Q3's 1.2 kΩ (190 Hz at 1 H) and a series resonance with the
      33 µF (28 Hz at 1 H). It also gives the **flyback**: when Q3 cuts off, its collector rises above the
      supply, to 10.9 V on 9 V at 440 Hz [P]. On low notes Q3 stays on longer and lets go of more current:
      the winding reaches +12.9 V at 82 Hz and +16.9 V at 41 Hz (0.3 V in, `fuzz` 100 %), so the collector
      passes 18 – 22 V [P, `octreview.py flux`]. No junction breaks down there (a 2N4401 holds 40 V) and
      none is modelled. The octave at small signal moves with L_m: H2 − H1 = +5.8 / +24.2 / +20.7 dB at
      0.3 / 1 / 3 H (440 Hz, 30 mV) [P]. What moves is the fundamental's null, not the octave (review): H2
      stays within 1 dB over that range and H1 moves by 18 dB. The two half-cycles are not alike. With Q3
      off, the winding is driven through 1.2 kΩ and its voltage droops with L_m / 1.2 kΩ (0.8 ms at 1 H);
      with Q3 saturated, it is held. L_m sets how unlike the two halves are, and the rectifier turns that
      difference into fundamental. Two rough estimates put the real part below 1 H rather than above [A,
      review]: the telephone bound above (0.16 H) and the core (EI-14, a 45 Ω winding of a few hundred
      turns: 0.1 – 0.5 H at µ_r ≈ 1000). 1 H stays the working value until the part is measured (LCR meter
      on a 42TM022, open question O1; decided 2026-11-26: Raph measures one part). Every octave figure
      below is at 1 H.
    - **Core saturation: not modelled** [A; figures redone by the review, P, `octreview.py flux`]. The
      part is rated 200 mW at 300 Hz on 600 Ω: 11 V rms, a peak flux linkage of 8.25 mWb. In the model the
      flux linkage is L_m times the winding's current, and it is one-sided: a saturated Q3 puts ≈ 3.3 V
      across the winding for as long as it stays on. At 1 H, `fuzz` 100 %: 8.8 mWb on a guitar's low E
      (82 Hz, 0.1 – 1 V in), 4.2 mWb at 196 Hz, 1.8 mWb at 440 Hz, and 13.3 mWb on a bass's low E (41 Hz).
      At 0.3 H: 5.8 mWb at most. At 3 H: 10.8 mWb at 82 Hz and 20 mWb at 41 Hz. So a guitar reaches the
      rated level on its lowest notes only, and a bass passes it by 1.6×. A datasheet sets that level
      below audible saturation, but none says by how much. Bench diagnostic: the peak flux linkage,
      reported per note. The current at which L_m starts to drop is part of the measurement of O1.
  - **How the octave appears, and why it blooms on the decay [P]** (`octrun.py`, `octwave.py`, `octnotes.py`;
    `buffer` on, `fuzz` 70 %). Full-wave rectifying a symmetric wave gives pure even harmonics. A
    rectangle with duty d ≠ 50 % rectifies to a rectangle at f with amplitude ∝ |1 − 2d|. Q3 is already
    rail to rail at 30 mV in (1.7 – 8.9 V), with the winding's duty at 0.49, and the output is **2f, 24 dB
    above f** at 440 Hz. Driven harder, Q1's base-emitter junction charges C1, the bias shifts, and the duty
    moves: 0.55 at 0.1 V, 0.65 at 0.3 V, 0.77 at 1 V. The fundamental comes back: H2 − H1 = +1.6 / −9.1 / −5.5 dB.
    So the octave is weak on the pick attack and grows as the note decays, the touch players describe
    [F]. Higher notes give more octave (50 mV, `fuzz` 70 %: 82 / 165 / 330 / 659 / 1319 Hz → H2 − H1 −2.3 /
    +3.1 / +10.4 / +23.7 / +9.3 dB): the ring on high notes, and the low strings stay a fuzz. At 0.2 V every
    note sits near −5 dB. `fuzz` 100 % saturates sooner: 440 Hz at 30 mV gives −0.7 dB. **The
    README's bench row "440 Hz at 0.3 V: 880 Hz ≥ 440 Hz" does not hold on this circuit** (−9.1 dB); §5 now
    measures at sustain levels. Whether a real pedal agrees needs a recording (open question O2).
  - **The old row was wrong, not the model (review).** That row was written on 2026-11-14 for a
    phase-splitter circuit that had no netlist. The octave's loss at 0.3 V does not come from the test's
    buffer: through a pickup (Single Coil or Humbucker, volume full up, the level taken on the EMF) the
    figures stay within 2.1 dB of the buffered ones at 82 – 988 Hz and 30 mV – 0.3 V, and the duty is the
    same to 0.01 [P, `octreview.py guitar`]. It comes from the input clamp, which every build of this
    circuit has. On negative peaks Q1 saturates against its 220 kΩ load and its base-emitter junction
    conducts into R7 and the fuzz pot: the input impedance falls from ≈ 270 kΩ to ≈ 1.3 kΩ, C1 charges,
    and it discharges only through the bias network (≈ 370 kΩ: 37 ms). In steady state the duty depends on
    the level and no longer on the source impedance. The documents agree with the consequences: the
    "swells" of Fuzz Central's page, the strongest octave "with the tone rolled all the way off" and the
    ring-modulator sounds "with the volume knob on your guitar rolled down" [C: Retrograde24]. What a
    recording still has to confirm is the level at which the octave gives way (O2).
  - **Why the transformer type, not the phase splitter** (open question 4 reopened). (1) The roadmap's
    sound reference is this circuit. (2) It has named documents with values, and one with measured
    voltages that the netlist reproduces. The phase-splitter family (fOXX Tone Machine and its clones:
    Aion FX Vulcan, 2N5089 ×4, BAT46 [C, values not read: the document's parts pages were not
    extracted]) still has none read. (3) The objection of 2026-11-14 ("magnetising and leakage inductances
    of a part nobody publishes") shrinks to one number: leakage drops out with the secondaries open, and
    L_m is one [U] constant with a measured sensitivity. The phase-splitter type stays a candidate,
    appended later as **Octave Split** if a document is read; it is not designed further here.
  - **`bias`**: the trimmer is **R2** (Q1's base resistor to ground, stock 680 kΩ, not re-trimmed: the
    document's value gives 4.87 V). Here R2 = 680 kΩ × 2^(+`bias` / 100), so that **+ raises `bias_v`** as on
    the faces. `bias_v` = Q3's collector: −100 / −50 / 0 / +50 / +100 % → 4.04 / 4.22 / 4.87 / 5.58 / 6.28 V
    [P]. Below stock the loop holds the collector near 4 V. 15 / 35 °C: 4.90 / 4.85 V (silicon). `voltage`
    4.5 / 18 V: 2.80 / 9.00 V. `leakage` is inert (silicon transistors; the germanium diodes follow `temp`
    through their Is). `sag` as on every model.
  - **Input impedance** ≈ 270 kΩ (R1 ∥ the divider ∥ Q1's base) [A], between the muff's 130 kΩ and a
    buffer: the pickup is barely loaded (§2.2), plus C2's 150 pF next to the cable. At small signal only:
    on negative peaks above a few tens of mV it falls to ≈ 1.3 kΩ (the clamp above), which is why the
    guitar's volume knob changes the character and not only the level.
  - **DC guard: 20 Hz, second order, on Octave Up only.** The rectified output carries the note's
    envelope (mean 2.1 V over a note's first 100 ms). Through the shared 5 Hz first-order guard, a decaying
    330 Hz note at 0.2 V leaves an infrasonic swing of 1.41 V peak, against 0.92 V rms of audio. Through a 20 Hz
    second-order high-pass, 0.31 V [P, `octthump.py`]. An amp's input capacitor passes the real pedal's
    thump and its next stages cut it. golem's chain may hold a limiter or a compressor next, so the
    thump stops here. The other models keep the 5 Hz guard, bit for bit. **Decided by the review (O7):
    20 Hz.** Figures reproduced. The stronger reason is the amp: golem's amps are NAM captures, trained on
    guitar signals, and 1.4 V of 5 – 15 Hz under the note is outside what they were trained on. A bass's
    low B (31 Hz) loses 0.7 dB, a low E 0.2 dB. The guard sits on the model's output, after the `octave`
    crossfade.

The models use the `tone` control in two ways (§3): on Si Four it moves the stack's pot; on the others,
which have no tone control, it is a gentle tilt after the circuit, neutral at 0.

**Polarity.** Each netlist's output is multiplied by a fixed sign so that its small-signal mid-band
gain is positive (the two- and four-transistor circuits do not invert; a three-transistor one does).
Without it `mix` and `bass_keep` would subtract the dry signal on Ge Three.

### 2.4 The solver: gfx-dsp `circuit_nl`, shared with gfx-dist

**One module.** gfx-dist and gfx-fuzz were designed in parallel and each specified a nonlinear solver.
There is one: gfx-dsp **`circuit_nl`**, type **`NonlinearCircuit`**, netlists written in
`circuit::Netlist` with new elements. gfx-dist's §2.2 and §2.10 are the reference text for the solve; this
section states what gfx-fuzz uses and adds. Whichever pedal is coded first creates the module; the
other extends it; neither forks it. §2.9 gives the API.

- **Formulation** (Yeh 2010; Holters & Zölzer 2011, 2015). From the netlist, modified nodal analysis with
  trapezoidal companion models for every capacitor and inductor gives
  x[n] = A·x[n−1] + B·u[n] + C·i[n],  p[n] = D·x[n−1] + E·u[n],  v[n] = p[n] + K·i[n],
  y[n] = Dy·x[n−1] + Ey·u[n] + Fy·i[n],
  where u holds the inputs (the reconstructed EMF, the battery's voltage), v and i are the voltages and
  currents of the **nonlinear ports**, and the devices give i = f(v; T, leakage).
  - Ports: two per transistor (v_BE, v_BC), one per diode or anti-parallel pair. Ge Pair and Si Pair: 4;
    Ge Three: 6; Si Four: 10; Octave Up: 6 + 2 in two stages, cut at an ideal buffer (§2.3).
- **Newton on v**, in f64, analytic Jacobian. A cap of 8 with "keep the last iterate", as first written,
  does not hold [P: faces with the graded devices, `buffer` on, full fuzz, 384 kHz, `pnjlim`, seed = the
  previous solution]:

  | Input | Ge Pair: mean, cap of 8 reached | Si Pair: mean, cap of 8 reached |
  |---|---|---|
  | 200 Hz, 100 mV | 2.95, 0.02 % | 3.0, 0.04 % |
  | 1.2 kHz, 1 V | 4.0, 0.07 % | 4.5, 0.9 % |
  | 5 kHz, +24 dB | 4.5, 1.5 % | 6.1, 7.4 % |
  | Full-scale noise | 6.1, 9.4 % | 6.3, 15.5 % |

  The draft expected ≈ 2 iterations and the cap on ≤ 0.01 % of samples. The hard samples are those
  where a transistor enters or leaves saturation or cut-off. gfx-dist's review found the same on its
  op-amp clipper (1 – 2 % of a sine's samples needing 9 – 17 iterations). Keeping the last iterate
  there injects a wrong current into the states. So gfx-fuzz uses gfx-dist's solve unchanged:
  1. seed: the previous solution, extrapolated linearly in p **only when the two previous samples
     were solved by plain Newton**. An extrapolation built on a sample that needed the fallback
     diverges and makes the next sample fail too: on the prototype one failure became 99 % of the
     following samples [P];
  2. Newton with **backtracking** (step halved until ‖g‖² falls) and SPICE's `pnjlim` on every junction;
     exponentials linear beyond 40·N·V_T; stop at |Δv| < 10⁻⁶ V or **20 iterations**;
  3. on failure, **continuation in p** from the previous sample's (p, v), in 4 steps, then 16 [P: it
     solved every sample of every run here; the worst sample cost about 60 iterations in all, on a
     prototype without backtracking];
  4. if that fails, the sample keeps the **previous solution** and a counter is incremented. The bench
     requires 0.

  The caps are fixed and the path depends on the data only: two renders are bit-identical (contract).
  - **`pnjlim`, exactly.** The draft described it as "at most 2·N·V_T per iteration". That is not the
    rule and would be worse: a junction leaving 5 V of reverse bias would need 80 iterations. `pnjlim`
    acts only above the critical voltage N·V_T·ln(N·V_T / (√2·Is)) and compresses the step
    logarithmically; below it the step is free.
  - LU with partial pivoting from 3 ports up (gfx-dist). Uniqueness: transistors and diodes are passive
    and monotone, and no model has a positive-feedback loop at the algebraic level; the bench checks
    the sign of det J over the port range all the same.
  - A fast exp (bit-trick + polynomial, relative error < 10⁻⁷, four lanes at once) is **required**, not
    optional: `exp` calls are the cost (§2.8). The bench checks it against `f64::exp` (null ≤ −100 dB).
- **Si Four is solved whole: the partition is dropped.** The draft cut the muff at its coupling
  capacitors, each stage seeing the next one's small-signal impedance, on the grounds that they feed
  "high-impedance bias networks", and expected a −50 dB difference. They do not: a clipping stage's
  input is 10 kΩ into a shunt-feedback node whose impedance collapses when its diodes conduct, hung on
  a 10 kΩ collector. Measured on a muff netlist with typical values, one of golem's DIs: the partitioned
  solve differs from the joint one by **−11 to −20 dB** re the output [P]. A few percent of error at a
  collector, then two clipping stages of gain. This is gfx-dist's rule: a cut is exact only at an ideal
  op-amp output or at a node that draws no current; a collector is neither. And the cut bought
  little: the 10 × 10 LU is not the cost, the 10 exponentials per iteration are, cut or not.
  - The joint 10-port Newton is well behaved here: mean 2.2 – 2.7 iterations at 8×, never more than 7
    [P]. The 470 pF across each stage slows every edge.
  - If the CPU bound fails (§2.8): block Gauss-Seidel over the four stages **iterated to the joint
    residual** (exact at convergence, small blocks), never a frozen load.
- **Pots and variable parts: a Woodbury update every base-rate sample** (gfx-dist §2.2; Holters &
  Zölzer 2011), not a rebuild every 16 samples. A pot is two resistances, i.e. a rank-2 change of the
  MNA matrix; the matrices are corrected through a 2 × 2 solve, O(n²·r) ≈ 1,000 multiply-adds for the
  largest model, ≈ 0.1 µs per base sample while a control moves, nothing otherwise. The 16-sample
  staircase left −50 to −55 dB of zipper on gfx-dist's prototype; the contract asks −80 dB.
  States are capacitor voltages and inductor currents: a moving pot never jumps them.
  Variable parts here: `guitar_vol` (both halves of the track), `fuzz`, `tone` / `mids` (Si Four),
  `bias`, `sag`. `voltage`, `temp` and `leakage` are not parts: an input and device constants.
- **DC operating point.** At `initialize`, for each model: Newton with source stepping (the supply
  ramped 5 % → 100 % in 20 steps) [P: converges on every case of §2.3 and §2.6, 3 – 18 V, 5 – 45 °C],
  giving the seed and the reset state: a reset starts at rest, no power-on thump. Later re-solves
  (reset at another `temp` / `voltage` / `bias`, model switch) run **inline, warm-started from that
  seed**, bounded at 20 iterations (tens of µs); if one does not converge the model starts from the
  seed and re-biases audibly over its own time constants, which is what the real pedal does.
- **Oversampling, per model** (gfx-dsp half-band stages, minimum phase). Measured on the prototype, −12 dBFS sine, full fuzz,
  `buffer` on (the worst case: no pickup in front), worst alias in 20 Hz – 20 kHz re the fundamental [P]:

  | Model | 4× | 8× | 16× |
  |---|---|---|---|
  | Ge Pair, 1.2 kHz | −53 dB | **−70 dB** | −89 dB |
  | Ge Pair, 5 kHz | −42 dB | −54 dB | −67 dB |
  | Si Pair, 1.2 kHz | −39 dB | −54 dB | **−69 dB** |
  | Si Pair, 5 kHz | −26 dB | −34 dB | −46 dB |
  | Si Four (joint), 1.2 kHz | **−77 dB** | −111 dB | |
  | Si Four (joint), 5 kHz | −63 dB | −93 dB | |

  - The draft's argument ("the junction capacitances band-limit the edges: ≈ 50 dB below an ideal
    square at 8×", hence −80 dB on every model) holds for the muff, whose 470 pF feedback capacitors
    slew every collector. It does not for the faces: 100 pF (germanium) or 5 pF (silicon) on a few kΩ is
    a pole far above the audio band. The silicon figures depend on C_cb and C_eb, which are [U] here.
  - **Decision: the factor is a constant per model, set by one rule** — worst alias ≤ −65 dB at
    1.2 kHz in this test (target −70), the 5 kHz figure reported. From the prototype: **Ge Pair 8×,
    Si Pair 16×, Si Four 4×**; Ge Three starts at 8× and takes what the bench says
    (`Oversampler8x`, and gfx-dist's `Oversampler16x`; one stage fewer per doubling of the host rate).
  - **Octave Up: 8×** [P, `octalias.py`, `octalias2.py`; the monolithic coupled solve, `fuzz` 100 %,
    `buffer` on, −12 dBFS]. At 1.2 kHz the worst alias is **−40.8 dB at 4×, −76.2 dB at 8×, −77.2 dB at 16×**;
    at 5 kHz, −23.6 / −32.4 dB at 4× / 8×. The 8× and 16× figures are a floor: the 33 µF bias filter still
    drifts by 10 mV over the window and its ±50 Hz sidebands around each harmonic are excluded (guard
    ±150 Hz, not alias.py's ±32 Hz; with ±32 Hz the floor reads −53 dB at 8× and at 16× alike). Split by
    stage: the fuzz alone (the winding's voltage) aliases at −45.9 dB at 4× and −83.9 dB at 8×. So the
    amplifier needs 8× by itself, not only the rectifier. Rectifying at 8× behind a 4× fuzz gives −40.1 dB.
    The rectifier at 8×: −74.5 dB. Reproduced by the review, to the decimal. One doubling buys 35 dB
    because the winding's edges are not instantaneous: Q1's collector ramps on C4's 1 nF and Q3 multiplies
    the ramp, which gives edges of 1 – 8 µs (10 – 90 % in 8 µs, steepest slope 5.8 V / µs at 32× [P,
    `octreview.py edges`]). A sample lasts 5.7 µs at 4× and 2.8 µs at 8×. The figure therefore hangs on C4
    and on Q3's gain: a variant without C4, or with faster devices, is measured again before it ships. The
    bench states its guard and its settling time in the row: the bias filter's constant is 5 s.
    A 4× default for every model, as open question 7 proposed, is not possible: −39 to −53 dB on the
    faces.
  - Why −70 and not gfx-dist's −85: a guitar has no 5 kHz fundamental (the DIs hold −30 to −45 dB
    there re their body), with the pickup in front (the default) the drive above 1 kHz is 9 – 22 dB
    lower again (§2.2), and a −70 dB alias sits 45 dB under the fuzz's own neighbouring harmonics.
    One more doubling costs ≈ 1.7× the heaviest block of the suite. If the listening test on high bends
    (lead.wav) hears it, the constant goes up; it never becomes a user option.
  - **Why not ADAA**: the nonlinearity is inside the circuit's state. Holters' stateful ADAA exists but is
    not general enough for a multi-port network.
- **Determinism and safety.** f64 states; flush-to-zero; the unsolved counter exported to the bench. The
  bench feeds +24 dB, DC and full-scale noise.

### 2.5 Temperature and leakage

- **`temp`** (0 – 50 °C) sets T in every device:
  - V_T = k·T / q;
  - Is(T) = Is(T₀) · (T / T₀)^(XTI / N) · exp((Eg / (N·k))·(1 / T₀ − 1 / T)): **SPICE's law, ideality factor
    included**, XTI = 3, Eg = 0.67 eV (Ge), 1.12 eV (Si), so that `to_spice()` with `.temp` gives the same
    device. T₀ is the device set's own (22.8 °C for the DAFx-17 germanium sets, 25 °C otherwise). For the
    AC128 set Is grows ×2.25 per +10 °C (×2.57 without N, as first written) [P].
- **Leakage is already in Ebers-Moll [A, P].** With the base open, the model's collector current is
  I_CEO = Is·(1 + (β_F + 1) / β_R): 262 µA for the measured AC128, of which I_CBO ≈ Is / β_R = 5.2 µA is the
  collector junction's saturation current. An extra current source of I_CEO / (β_F + 1), as first written,
  counts it twice, and does almost nothing: 100 µA on Q2 moved its collector by 0.03 V [P]. What sets a
  germanium face's bias is Q1's own I_CBO against its base current.
- **`leakage`** (germanium models only; **relative**, 0 – 400 %, 100 % = the model's devices) scales the
  **collector junction's saturation current**, Is / β_R → k·Is / β_R, on every germanium transistor of the
  model. No extra element, no extra port, passive, and temperature scales it with Is (×2.25 per 10 °C,
  the textbook "doubles every 10 °C"). 0 % = a junction that does not leak. Ge Pair, trimmed, 25 °C:
  Q2 at 3.0 / 3.8 / 4.5 / 6.0 / 7.6 V for 0 / 50 / 100 / 200 / 400 % [P]. The description shows the
  resulting I_CEO in µA (Q2: 200 µA at 100 %), the number builders measure.
- **No hidden re-bias.** The trimmer (§2.6) is fixed at its 25 °C, 9 V value per model: moving `temp`,
  `leakage` or `voltage` moves the pedal off bias, as on a real board.
- Changes glide over 300 ms (a temperature or a trimmer does not jump), so the DC shift reaches the output
  as an infrasonic swell, not a thump. Device constants are run-time parameters of the solver: a glide
  changes f(v), never a matrix.

### 2.6 Bias, supply, sag: the gate and the velcro

- **`bias`** (−100 to +100 %, 0 = stock): the trimmer the builder set. On the face models it is **Q2's
  collector resistor** (§2.3), solved at design time so that Q2 sits at **4.5 V at 25 °C, 9 V**,
  hard-coded per model with a test that recomputes it. The resistor is stock × 2^(−`bias` / 100):
  +100 % halves it (Q2's collector rises to ≈ 6.6 V: Q2 starved of swing towards the supply), −100 %
  doubles it (≈ 0.4 – 0.7 V: Q2 saturated) [P, both faces]. Which side is called "gated" and which
  "thin" is written from listening, not assumed. On Ge Three, the same law on the last stage's
  collector resistor; on Si Four, on the first clipper's base bias resistor; on Octave Up, on Q1's
  base resistor to ground, R2, with the exponent's sign turned so that + still raises `bias_v` (§2.3). A
  read-only meter **`bias_v`** shows the magnitude of Q2's collector voltage (Si Four: the first
  clipper's; Octave Up: Q3's, the transformer's driver), the number a builder reads with a multimeter.
- **`voltage`** (3 – 18 V, default 9 V): the battery's open-circuit voltage. Above 9 V: the "18 V
  adapter" use on silicon fuzzes (more headroom, louder, less compressed [F]); the trimmed Si Pair's
  Q2 then sits at 12.8 V [P]. It is an input of the solver (vector u): no matrix is rebuilt.
- **`sag`** (0 – 100 %): the battery's internal resistance, 0 – 2 kΩ, with a 47 µF reservoir at the
  circuit's supply node. The circuit's own current then pulls the supply down on hard hits, with a time
  constant of up to 94 ms [A]. A dying 9 V battery is far less than 2 kΩ [U]; the range is wider on
  purpose, for the effect. The resistance is a variable part (§2.4).
- **What the gate is [P]** (trimmed pedals, 25 °C): on a 6 / 4.5 / 3 V supply the silicon face's Q2
  collapses to 1.8 / 0.65 / 0.46 V (saturated): a note gets through only while it is loud, then the pedal
  cuts it. That is the gated, "velcro" decay. The germanium face's Q2 follows the supply instead
  (3.2 / 2.6 / 1.8 V): it stays biased, softer and splattier rather than gated. Both are what players
  describe [F].
- Read-only meter **`supply_v`**: the circuit's supply voltage now (sag included).

### 2.7 Around the circuit

- **Tone tilt** (Ge Pair, Si Pair, Ge Three, Octave Up): first-order tilt around 800 Hz, ±6 dB at
  `tone` ±100 %; 0 = off. gfx-dsp `svf` (first-order shelves).
- **`mix`** (0 – 100 %, linear crossfade, 100 % = all fuzz): the dry is the DI as it came in (not the
  loaded pickup signal), through the same up / down filters (gfx-dsp oversampler identity path) and
  with the model's polarity (§2.3), so it is phase-aligned except for the circuit's own coupling
  high-passes. Those are part of the sound, as on a blend knob.
- **`bass_keep`** (shared id): below the frequency, the dry signal replaces the fuzz (Linkwitz-Riley 4,
  dry through the same identity path, as gfx-emet). For bass.
- **Model switch**: the new model starts from its operating point at the current `temp`, `voltage`,
  `bias` and `leakage`: the inline DC re-solve of §2.4, warm-started from the seed of `initialize` (no
  pre-run). Then a 30 ms equal-power crossfade. During the fade both run: the CPU peak is two models
  for 30 ms. The two oversamplers may differ (4× / 8× / 16×); their group delays differ by a fraction
  of a millisecond, inside the fade.
- **`pickup` switch**: new element values and a new EMF filter: two source networks, the same
  crossfade.
- **Buffer switch**: two systems compiled (guitar network inside or apart), the same crossfade.
- **`bypass`**: nih-plug's bypass parameter, 10 ms crossfade (no tail).
- **Mono 1 / 1** first, stereo 2 / 2 offered as two independent channels (a fuzz is mono by nature).

### 2.8 Latency and CPU

- **Latency: reported 0.** The minimum-phase oversampling adds a few samples of group delay at low
  frequencies, not reported (the gfx-drive / gfx-emet precedent). The bench skips its latency check.
- **CPU [A, from the prototype's iteration counts].** The draft counted ≈ 2 iterations and the
  exponentials as flops: 1.1 – 2.0 µs, 5 – 13 % of a core. On guitar-level signals the faces take **≈ 3
  iterations** per sample at 8× (4 – 6 on hot or high inputs; DAFx-17 reports 3.6), the joint muff 2.2 –
  2.7 at 8× and 3.4 at 4× [P]; and the cost is the exponentials (two per transistor and per diode pair,
  per iteration), as gfx-dist's review measured (2 – 3× its estimate). Per sample at 48 kHz, mono,
  with the 4-lane fast exp (≈ 4 ns per value [U]):

  | Model | States n | Ports m | Factor | Estimate | Bench bound |
  |---|---|---|---|---|---|
  | Ge Pair | ≈ 12 (incl. coil, cable, junction caps) | 4 | 8× | ≈ 1.8 µs | ≤ 2.5 µs |
  | Si Pair | ≈ 12 | 4 | 16× | ≈ 3.7 µs | ≤ 4.5 µs |
  | Ge Three | ≈ 15 | 6 | 8× | ≈ 3.2 µs | ≤ 4.0 µs |
  | Si Four (joint) | ≈ 20 | 10 | 4× | ≈ 3.9 µs | ≤ 4.5 µs |
  | Octave Up (2026-11-26) | ≈ 17 + 0 | 6 + 2 | 8× | **≈ 8 – 10 µs** | see below |

  That is **9 – 26 % of one core**, not 5 – 13 %: by far the heaviest golem-fx block per channel
  (gfx-drive 736 ns stereo, gfx-emet 198 ns). With `f64::exp` (15 – 20 ns) every line is 1.5 – 2× higher,
  which is why the fast exp is part of the design.
- **Octave Up's estimate (2026-11-26) [A, from the coded models and the prototype's counts].** As coded,
  Ge Pair costs 0.55 µs per oversampled sample (4 ports, 2.4 iterations) and Si Four 1.22 µs (10 ports,
  1.5 iterations). On the prototype at 8×, Octave Up's stage 1 takes 3.5 iterations on average, 4.0 at 4×.
  The monolithic solve takes 4.4 (1.2 kHz, −12 dBFS, `fuzz` 100 %), max 18 – 19, 3 % of samples above
  8, 0 fallback, 0 unsolved; at 5 kHz, 6.4, 20 % above 8 [P]. The prototype has no backtracking and no
  quadratic seed, which brought the coded faces from ≈ 3 to 2.4. Taking ≈ 2.8 iterations of a 6-port,
  17-state stage at ≈ 0.35 – 0.45 µs each, plus stage 2 (2 ports, 2.7 iterations, no state, ≈ 0.1 µs),
  gives 1.0 – 1.3 µs per oversampled sample: **≈ 8 – 10 µs per host sample, 40 – 50 % of a core**, above
  the bound. In order, the ways down:
  1. the solution cache of §2.8's decision, already the route for Si Pair;
  2. **Q2 as a one-port device**: its base-collector junction never conducts (forward voltage −4.9 to
     −5.0 V on every prototype run, 82 Hz – 1.2 kHz, up to 1 V in [P, `octports.py`]). Q1 and Q3 do
     saturate (0.62 / 0.75 V). A forward-only `Bjt` option in `circuit_nl` brings stage 1 to 5 ports. It is
     a new branch, taken only when set, so gfx-dist's and the shipped models' nulls hold. ≈ −15 %;
  3. `set_fast_solve`, `set_quadratic_seed` (as Ge Pair and Si Four).
  **Decided by Raph, 2026-11-26 (O3):** the netlist is coded in gfx-bench first and measured (mean and
  worst block, with ways 2 and 3); if the figure stays above ≈ 20 % after them (expected 30 – 40 %), a
  per-pedal CPU exception is granted to gfx-octafuzz (one instance per lane, linked mono halving stereo).
  **Review.** The estimate follows from the coded models, and no cheaper structure was found. A cut
  inside R8's loop is the muff's partition again (−11 to −20 dB). 4× is ruled out by the fuzz stage itself.
  Stage 2 is about a tenth of the cost. The precedent is Si Pair, not Si Four: at 44 % it is coded and not
  shipped. So the order is: the netlist in gfx-bench first, with ways 2 and 3, and the measured mean and
  worst block; then it ships under ≈ 20 %, or above it by a decision the maintainer takes on that figure,
  or it waits for the cache with Si Pair. Way 2 holds by topology, not only on the runs: Q2's base cannot
  pass Q1's emitter, which sits below node Y, itself below Q3's mean collector voltage and so below the
  supply. The exception is a fast downward step of `voltage` while the 100 µF still holds Y: a row of the
  bench (§5, Octave Up DC). Stage 2 needs no op-amp either: `circuit_nl` takes several inputs, so the two
  half-windings are two entries of u, +a · v and −a · v.
- **Decision (open question 7).** Up to ≈ 4 µs (20 % of a core at 48 kHz) is accepted for a pedal used
  once per lane, in front of an amp model that costs as much. A model that measures above its bound
  does not ship at a lower oversampling: it gets, in order, the k-d-tree solution cache of ACME
  (Holters & Zölzer) for the 4-port faces, then block Gauss-Seidel for Si Four (§2.4). Si Pair returns
  to 8× only if the alias rule holds there with the datasheet's junction capacitances.
  The worst case is bounded by the fixed caps (20 iterations, then 4 + 16 continuation steps); the
  bench reports the worst block as well as the mean, because a live rack fails on the peak.

### 2.9 gfx-dsp: reused and new — the shared `circuit_nl`

- **Reused**: `circuit` (netlist language, MNA stamping, `Netlist::response` for the tests' linear
  references); `oversample` (gfx-emet's private 8× cascade moves to gfx-dsp as **`Oversampler8x`**;
  gfx-dist adds `Oversampler16x`; a 4× is the first two stages); `pot` (taper laws, from gfx-emet);
  `svf` (tilt, and gfx-emet's Linkwitz-Riley 4 split with `bass_keep`'s identity path); `smooth` /
  `exp_smooth` (pots, EXP).
- **New: `circuit_nl`**, one module for gfx-dist and gfx-fuzz (and later gfx-wah's transistor stage, amp
  preamps). It is **not** inside `circuit`, as first written here: `circuit` stays the linear path
  that gfx-shem and gfx-emet use, unchanged. Names fixed in both documents: module `circuit_nl`, type
  `NonlinearCircuit`, netlists in `circuit::Netlist` with new elements. gfx-dist's §2.10 holds the same
  table from its side. What the module must cover from its first version:

  | Area | API | Needed by |
  |---|---|---|
  | Linear elements | R, C, **L**, ideal op-amp; `VSource` with DC value and series resistance (battery); `ISource`. Coupled inductors are not needed: Octave Up's transformer is lightly loaded, so it is one L plus ideal buffers (§2.3, checked against a coupled solve). Kept for a later circuit with a loaded transformer | L, battery: gfx-fuzz. Op-amp: gfx-dist |
  | Inputs | Vector u: several signal inputs, the supply as an input (so `voltage` never rebuilds a matrix) | Both |
  | Devices | `Diode`, diode pair (Shockley + series R; C_j as a linear capacitor); **`Bjt` NPN / PNP**, Ebers-Moll with separate collector-junction saturation current (`leakage`), C_eb / C_cb, SPICE temperature law; MOSFET clipper; op-amp macromodel (tanh, slew), discrete op-amp macro. **Every device constant is a run-time parameter** (`set_temperature`, `set_device_param`): no matrix depends on one | BJT, temperature, leakage: gfx-fuzz. Op-amps, MOSFET: gfx-dist |
  | Variable parts | Pots (two- and three-terminal, ganged) and single resistors declared variable; **Woodbury update, callable every base-rate sample**, no allocation; ends clamped (10 Ω) | Both (`guitar_vol`, `fuzz`, `bias`, `sag` here) |
  | Stages | A directed graph of stages; a cut only where it is exact (ideal op-amp output, zero-current node). **Monolithic stages up to ≈ 10 ports** with pivoted LU. An approximate cut is a separate, named mode, accepted only by a cut check against the monolithic solve | Graph: gfx-dist. 10-port stage: gfx-fuzz (a fuzz's transistors sit in one loop). gfx-fuzz uses no cut |
  | Solve | gfx-dist §2.2: guarded seed, backtracking Newton, `pnjlim`, cap 20, continuation in p (4, then 16), hold of the previous solution, unsolved counter, fixed caps → bit-identical renders | Both |
  | DC point | `solve_dc()` with source stepping at `initialize` (seed, reset state); bounded inline re-solve, warm-started | Both; with temperature and leakage here |
  | Meters | Any node voltage readable per block (`bias_v`, `supply_v`) | gfx-fuzz |
  | Analysis | `linearized()` → a `Netlist` at the operating point (small-signal targets, input impedance); an **offline reference mode** (any rate, monolithic, tolerance 10⁻¹²); **`Netlist::to_spice()`**: the same netlist and device constants as an ngspice deck (`.model` cards, `.temp`), so no number is copied by hand | Both |
  | Real time | No allocation after `prepare`, f64, flush-to-zero, deterministic | Both |

- **References.** ngspice is not installed on the build machine. The bench's reference is therefore the
  offline mode (64×, monolithic); `to_spice()` is run once against ngspice wherever it is installed and
  its outputs are committed as generated fixtures (deck + result), so the cross-check runs everywhere.

## 3. Parameters

New block: no catalogue ids. Proposed golem block type **`fuzz`**, category **drive**, golem-fx-only
(catalogue v43 rule: quarantined where golem-fx does not play it). Shared ids keep their contract meaning
(`mix` — its law stated below —, `bass_keep`, `bypass`). Ids not taken on purpose:

- `fuzz`, not `gain` or `drive`: it is the circuit's fuzz pot (its own taper), not dB, and `drive` is the
  contract's "extra saturation, neutral at 0".
- `tone`: relative, 0 = the model's stock (contract: "parameters that must follow the model are relative").
- Enum labels are generic family names, never trademarks (open question 5).

All ranges are linear (contract); the tapers are the pots' own laws, applied inside.

### Front panel

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `fuzz` | Fuzz | f | 0 – 100 % | 70 % | EXP rule → pot, every sample | ✓ | The model's fuzz / sustain / attack pot, on its own taper (face: 1 kΩ linear; muff: 100 kΩ) |
| `tone` | Tone | f | −100 – +100 % | 0 % | 20 ms | ✓ | Si Four: the stack's pot (0 = noon, the stock setting). Others: tilt ±6 dB at 800 Hz, 0 = off |
| `level` | Level | f | −24 – +24 dB | 0 dB | 20 ms | ✓ | 0 dB = unity loudness on every model (§2.1); a gain after the circuit |
| `guitar_vol` | Guitar Vol | f | 0 – 100 % | 100 % | EXP rule → pot, every sample | ✓ | The guitar's volume knob, real taper and impedance: **the cleanup** |
| `model` | Model | e | Ge Pair · Si Four (option ranks 0 · 1, the shipping order; Si Pair, Ge Three and any Octave Split are appended after, in the order they ship; Octave Up is not an option: it ships as gfx-octafuzz) | Ge Pair | 30 ms crossfade | | The circuit (§2.3) |
| `buffer` | Buffer | b | off / on | off | 30 ms crossfade | | A buffer between guitar and fuzz: low source impedance, no cleanup magic |

### Expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `pickup` | Pickup | e | Single Coil · Humbucker · P-90 · Active | Single Coil | 30 ms crossfade | | The guitar the DI was recorded with: coil, pots, cable (§2.2). Set by golem at insert when it knows the guitar |
| `bias` | Bias | f | −100 – +100 % | 0 % | 300 ms | ✓ | The trimmer: 0 = 4.5 V at 25 °C, 9 V; +100 % halves the resistor (`bias_v` up), −100 % doubles it (`bias_v` down) |
| `temp` | Temperature | f | 0 – 50 °C | 25 °C | 300 ms | | Device temperature: germanium drifts, silicon barely |
| `leakage` | Leakage | f | 0 – 400 % | 100 % | 300 ms | | Germanium collector-junction leakage, relative: 100 % = the model's devices (inert on Si Pair, Si Four) |
| `voltage` | Voltage | f | 3 – 18 V | 9 V | 100 ms | ✓ | Battery voltage: low = gated, sputtery; 18 V = more headroom |
| `sag` | Sag | f | 0 – 100 % | 0 % | 100 ms | | Battery internal resistance, 0 – 2 kΩ with a 47 µF reservoir |
| `stack` | Tone Stack | e | Classic · Early · Triangle · Russian · Bypass | Classic | 30 ms crossfade | | Si Four's stack variant (inert elsewhere) |
| `mids` | Mids | f | 0 – 100 % | 0 % | 20 ms | ✓ | Si Four: fills the scoop (0 = stock) |
| `input` | Input | f | −12 – +12 dB | 0 dB | 20 ms | | For a DI hotter or weaker than 0 dBFS = 1 V |
| `mix` | Mix | f | 0 – 100 % | 100 % | 20 ms | ✓ | Linear crossfade dry ↔ fuzz, dry phase-aligned |
| `bass_keep` | Bass Keep | f | 0 (off) – 500 Hz | 0 | 30 ms | | Below it: dry (shared id) |
| `bypass` | Bypass | b | off / on | off | 10 ms | | Shared bypass |
| `bias_v` | Bias (V) | f, read-only | 0 – 18 V | — | — | | Q2's collector voltage (magnitude, 50 ms average), as a multimeter reads it (Si Four: the first clipper's; gfx-octafuzz: Q3's) |
| `supply_v` | Supply (V) | f, read-only | 0 – 18 V | — | — | | The circuit's supply now, sag included |

No default depends on another parameter. `leakage` is relative because the devices differ per model
(contract: a parameter that must follow the model is a percentage of the model's own). `stack`, `mids`
are inert outside Si Four, `leakage` on the silicon models: golem greys them through `depends` (the descriptor's groups). Expert groups (for
`visuals/data/groups.json`): **Guitar** (`pickup`, `buffer`, `input`), **Circuit** (`bias`, `temp`,
`leakage`), **Power** (`voltage`, `sag`), **Muff** (`stack`, `mids`), **Blend** (`mix`, `bass_keep`).
gfx-fuzz's `PARAMS` are unchanged by Octave Up (decided 2026-11-26, O6).

### gfx-octafuzz (the Octave Up pedal, decided by Raph 2026-11-26)

A separate thin crate, **gfx-octafuzz** ("GoleM FX Octafuzz"), depending on gfx-fuzz's engine with the
Octave Up circuit fixed (§2.3); its own face, presets, plugin class and bench. A `gfx-octafuzz/README.md`
stub will point here at coding time. Its ids keep gfx-fuzz's meanings, ranges and defaults: `fuzz`,
`tone` (tilt), `level`, `guitar_vol`, `buffer`, `pickup`, `bias`, `temp`, `voltage`, `sag`, `input`,
`mix`, `bass_keep`, `bypass`, `bias_v` (Q3's collector), `supply_v`. Not taken: `model`, `stack`, `mids`,
`leakage` (silicon only). One id of its own, **on the face**:

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `octave` | Octave | f | 0 – 100 % | 100 % | 20 ms | ✓ | Linear crossfade from the fuzz alone (0 %) to the rectified output, all octave (100 %, the stock pedal). The fuzz is the tap s1 = a · v(winding), the same scale as each half-winding, so the peaks match. A second circuit output, no extra solve. The three positions of a clone's switch [C: Octagon] lie on this control: 100 % is its octave, 50 % its lifted diode to first order (0.5 · (|x| + x) is one half-wave), 0 % its shorted diode, the winding straight into the pot |

Proposed golem block type: a new one (golem-rack's catalogue call), category drive. Expert groups as
gfx-fuzz's minus Muff; `mix`, `bass_keep` in Blend.

## 4. Applications

Presets become `presets/gfx-fuzz.json`, level-matched to the bypass (±2 LU on the bench DIs) with
`level`; the rows marked (gfx-octafuzz) go to `presets/gfx-octafuzz.json` instead. No artist or trademark names.

| Use / preset idea | Settings to start from | Feature that matters |
|---|---|---|
| **Sixties Ge** (the reference face) | Ge Pair, `fuzz` 100 %, `guitar_vol` 100 %, Single Coil | Pickup loading: warm, not fizzy |
| **Volume-Knob Clean** | Ge Pair or Si Pair, `fuzz` 100 %, EXP on `guitar_vol` 100 → 30 % | The cleanup: bright and nearly clean at the heel, full fuzz at the toe, almost no level change [P: §5], no channel switch |
| **Behind a Buffer** (to hear why it matters) | as above, `buffer` on | The same knob only turns it down |
| **Silicon Lead** | Si Pair, `fuzz` 85 %, `tone` +25 %, Humbucker | Brighter, more gain, tighter |
| **Summer Stage** (hot germanium) | Ge Pair, `temp` 35 °C (`bias_v` ≈ 6.7 V) | Drifts off bias: thinner, splattier |
| **Winter Garage** | Ge Pair, `temp` 12 °C (`bias_v` ≈ 3 V) | The other side: darker, more compressed |
| **Leaky Pair** | Ge Pair, `leakage` 250 % | An unselected pair: the pedal a builder would have rejected |
| **Misbiased Spitter** | Ge Pair, `bias` −70 % or +70 % (the side chosen by ear, read on `bias_v`), `fuzz` 100 % | Q2 off its bias: sputter on the decay |
| **Velcro Gate** | Si Pair, `voltage` 4 V, `bias` −40 %, `sag` 40 % | Gated decay, chopped notes |
| **Dying Battery** (EXP) | Si Pair, EXP on `voltage` 9 → 3 V | From fuzz to broken gate under the foot |
| **Eighteen Volts** | Si Pair or Si Four, `voltage` 18 V | More headroom, louder, less compressed |
| **Three-Stage Raunch** | Ge Three, `fuzz` 80 %, `tone` −15 % | Raspy, sustaining British-style fuzz |
| **Wall of Sustain** | Si Four, Classic, `fuzz` 70 %, `tone` 0 % | The scoop and the sustain |
| **Doom Wall** | Si Four, Russian, `fuzz` 90 %, `tone` −40 %, `mids` 20 % | Dark, thick |
| **Band-Mix Muff** | Si Four, `mids` 80 % | Scoop filled: stays audible in a band |
| **Tone-Bypassed Muff** | Si Four, Bypass | Loud, flat, mid-heavy |
| **Octave Ring** (gfx-octafuzz) | `fuzz` 70 %, neck pickup (Humbucker), `tone` −40 %, play above the 12th fret | The rectified octave: 2f up to 24 dB over f on sustained high notes [P] |
| **Octave Bloom** (gfx-octafuzz) | `fuzz` 60 %, `input` −6 dB, single notes held | Attack in fuzz, the octave growing on the decay: the coupling capacitors releasing the bias (§2.3) |
| **Octave Blend** (gfx-octafuzz) | `octave` 50 %, `fuzz` 80 % | The octave as a colour on a plain fuzz; chords stay readable |
| **Ring Chords** (gfx-octafuzz) | `fuzz` 100 %, double stops and bends | Intermodulation, the "dive-bomb" on double-stop bends that the clone documents praise [C: Retrograde24] |
| **Octave Lead, Hot** (gfx-octafuzz) | `bias` +60 % (`bias_v` ≈ 5.7 V), `voltage` 12 V | Q3 further from saturation: more headroom before the duty moves, the octave holds on harder picking (to be heard) |
| **Fuzz Bass** | Si Four, Russian, `bass_keep` 120 Hz, `mix` 70 % | The fundamental stays |
| **Parallel Fuzz** | Ge Pair, `mix` 50 % | Blend knob, as the plugins players praise |
| **Swell Fuzz** (EXP) | Ge Pair, EXP on `fuzz` | The pot in the circuit, no zipper |

## 5. Bench suite (on top of gfx-bench's common checks)

**gfx-bench options**: `skip_latency` (minimum-phase oversampling, not reported); `click_bound_db` −35 dB
(a model, buffer or bias change moves the operating point: real harmonics and an infrasonic swell for up
to 300 ms, as gfx-emet's headroom).

Targets come from the netlist: `circuit_nl`'s offline reference mode (64×, monolithic, tolerance 10⁻¹²),
the `circuit` Laplace analysis of the linearized network, and the committed ngspice fixtures generated
through `to_spice()` (§2.9). Never copied by hand. Figures marked [P] below are the prototype's and
become the thresholds' starting point.

| Check | Pass |
|---|---|
| DC operating point | Every model, 25 °C, 9 V: node voltages within 1 mV of the ngspice fixture; also at 15 °C and 35 °C (the SPICE temperature law, §2.5) within 5 mV |
| Bias trimmer | Face models, `bias` 0: `bias_v` 4.5 ± 0.05 V at 25 °C, 9 V; `bias` ±100 %: ≈ 6.6 V / ≤ 0.8 V |
| Temperature | Ge Pair, 15 → 25 → 35 °C: 3.3 → 4.5 → 6.7 V ± 0.3 V [P]; Si Pair: moves < 0.4 V. Raw DAFx-17 pair, stock values, at its own 22.8 °C reference: 8.2 V ± 0.2 V at 25 °C [P] (the device equations against the review's independent solve) |
| Leakage | Ge Pair, `leakage` 0 / 100 / 400 %: 3.0 / 4.5 / 7.6 V ± 0.3 V [P]; open-base collector current of Q2 at 100 % = 200 µA ± 2 % |
| Supply | Trimmed Si Pair at 4.5 V: `bias_v` ≤ 0.8 V; Ge Pair: 2.6 ± 0.3 V [P]; 18 V: no unsolved sample |
| Transient vs reference | 200 Hz and 1.2 kHz sines at 10 mV, 100 mV, 1 V, every model, `buffer` on: error ≤ −40 dB re the output against the offline mode; Ge Pair with the raw DAFx-17 pair against the ngspice fixture |
| **Source invariant** | `guitar_vol` 100 %, the pedal replaced by 1 MΩ, each `pickup`: the input node equals the DI within 0.1 dB, 20 Hz – 16 kHz; with `buffer` on, bit-exact pass-through to the fuzz input |
| Pickup loading | `buffer` off, small signal, `fuzz` 0, `guitar_vol` 100 / 70 / 50 %: response within 0.5 dB of the linearized network (Laplace), 20 Hz – 10 kHz (targets: the table of §2.2) |
| **Cleanup** | `fuzz` 100 %, 200 Hz at an EMF of 50 mV, `guitar_vol` 100 → 50 → 30 %. Ge Pair: THD falls (−7 → −12 → −17 dB [P]) while the output level moves < 2 dB; at each position the **buffered** THD is ≥ 4 dB higher (−2 / −7 / −10 dB [P]). Si Pair: THD ≤ −30 dB at 50 %, buffered ≥ 20 dB higher [P] |
| Real-knob proxy | Reference: EMF → pot at 50 % → fuzz. Ours: the same pot into a 1 MΩ "interface", then gfx-fuzz at `guitar_vol` 100 %. The spectral difference is measured and reported (no pass mark: it sizes open question 1) |
| Source noise | DI = silence + white noise at −100 dBFS, each `pickup`, `guitar_vol` 100 / 50 %, Ge Pair: output noise in 8 – 20 kHz no more than 3 dB above the same run with the EMF filter replaced by the exact (unlimited) inverse evaluated offline |
| Muff stack | Each variant at `tone` −100 / 0 / +100 % and `mids` 0 / 100 %: within 0.25 dB of the Laplace network |
| Polarity | Every model, 1 kHz at 1 mV, `mix` 100 %: output in phase with the input (±90°); `bass_keep` 120 Hz: no notch deeper than 3 dB at the crossover. gfx-octafuzz at `octave` 0 % (the fuzz tap; the rectified output has no stable fundamental phase) |
| Level | Defaults, the guitar DIs, each model: loudness within ±1 LU of the bypass (K, §2.1) |
| Newton | DIs and the test signals, every model, 44.1 – 192 kHz: **0 unsolved samples**; continuation used on ≤ 0.5 % of samples on the DIs; mean iterations ≤ 3.5 on the DIs (reported for +24 dB, DC, full-scale noise: no NaN, no unsolved); det J keeps its sign over the port range at every pot position |
| Aliasing | −12 dBFS 1.2 kHz and 5 kHz sines, `fuzz` 100 %, `buffer` on, each model at its factor: worst alias in 20 Hz – 20 kHz ≤ −65 dB re the fundamental at 1.2 kHz (target −70); the 5 kHz figure and the next factor up are written into the status line |
| Octave | gfx-octafuzz, `buffer` on, `fuzz` 70 %, defaults otherwise, L_m = 1 H; sines, measured over 60 ms after 120 ms of tone. **30 mV (a held note):** H2 ≥ H1 + 12 dB at 440, 659 and 988 Hz [P +23.8 / +21.6 / +20.5] (the roadmap's "2f nettement au-dessus de f sur une note aiguë"); H2 ≤ H1 + 6 dB at 82 Hz [P 0.0] (the low strings stay a fuzz). **0.1 V:** H2 within ±6 dB of H1 at 440 – 988 Hz [P +1.9 / +2.6 / +1.9]. **0.3 V (the attack):** H2 ≤ H1 − 3 dB at 196 – 988 Hz [P −12.8 to −5.8]: the clamp is part of the circuit, and a change that removes it must fail. **Through the guitar** (Single Coil, `buffer` off, the level on the EMF), the same notes at 30 mV and 0.3 V: within 4 dB of the buffered figures [P ≤ 1.6]. The marks hold for L_m = 1 H and are derived again when Raph's 42TM022 is measured (O1, decided 2026-11-26): the first mark falls to +2 – +6 dB at 0.3 H (§2.3). Replaces "440 Hz at 0.3 V: 880 ≥ 440", which the circuit does not do (§2.3). Also on lead.wav through the guitar: 2f above f on the sustained notes above the 12th fret (the roadmap's "Fait quand") |
| Octave Up cut | gfx-octafuzz's engine. The cut against the coupled-winding solve is a prototype result (`proto/octcut3.py`: null −37 to −58 dB, harmonics within 0.1 dB [P]); no coupled element is coded for a test. In Rust, the "Transient vs reference" row covers the two-stage netlist, and the 8× render is checked against 64× monolithic on the same netlist |
| Octave Up DC | gfx-octafuzz. 9.26 V: Q1 E / B / C, Q2 E, Q3 C / E within 0.15 V of 4.32 / 3.77 / 2.91, 2.33, 4.99 / 1.67 V [P; Retrograde24 measures 4.18 / 3.54 / 2.93, 2.38, 4.85 / 1.73]. 9 V, `bias` +100 / −100 %: 6.28 / 4.04 V ± 0.2 V. The winding's peak flux linkage reported at 41, 82 and 196 Hz, 0.3 V (saturation diagnostic; [P] 13.3 / 8.8 / 4.2 mWb against 8.25 rated). `voltage` stepped from 18 to 3 V at full speed: no unsolved sample, with and without the forward-only Q2 (§2.8) |
| Octave thump | gfx-octafuzz, a decaying 330 Hz note at 0.2 V from silence: infrasonic (< 20 Hz) peak ≤ 0.4 × the audio rms [P 0.31 / 0.92 V] |
| Gate | Velcro Gate preset on a decaying note: the output falls 40 dB faster than the input after the gate point |
| Sag | `sag` 100 %, a hard chord: `supply_v` droops and recovers with the ≈ 94 ms constant |
| Zipper | `guitar_vol`, `fuzz`, `tone`, `bias` (and gfx-octafuzz's `octave`) swept by EXP steps: control-rate sidebands ≤ −80 dB (contract rule) |
| Determinism, CPU | Strict null; §2.8 bounds per model, mono, 48 kHz, **mean and worst block** (gfx-octafuzz: its measured figure, under its per-pedal exception if above ≈ 20 %, O3); fast exp against `f64::exp`: null ≤ −100 dB |
| Listening | lead.wav, crunch.wav and clean.wav (the maintainer's DIs) into a NAM clean and edge-of-breakup amp; A / B against golem's NAM captures of the same families (`fuzz-face`, `big-muff`, `tone-bender`, `octavia`), against gfx-drive Fuzz, the volume-knob roll played live on EXP; high bends at each model's factor against the next one up; graded pair against the raw DAFx-17 pair |

## Visual

- **Category: `drive`**, crate `gfx-fuzz`.
- **Finish (decided at review): anodized, copper.** `CRATE_FINISH['gfx-fuzz'] = 'anodized'`,
  `METAL_CRATE_HUE['gfx-fuzz'] = '#b0602a'` (germanium copper). No change to `metalHue`: the crate
  finish is the existing mechanism (gfx-gapmeter's precedent), where thrash would need an exception to
  the rule that `THRASH_HUE.drive` wins. Reasons: gfx-drive and gfx-dist are both thrash red `#c8160e`,
  a third red pedal would be told apart by its motif alone at tile size; and no other pedal is copper
  (drive's anodized orange `#e04a00` is never drawn, the family being thrash; utility is `#f0a500`,
  the companions are black). A fuzz is the vintage pedal of the family: engraved metal suits it better
  than diamond plate. The engraving reads dark on this hue (its luma is above the anodized threshold).
  To check on the family sheet at tile size, next to gfx-crusher's raspberry.
- **Motif: `sputter`** (new). On each side of the rune, a square wave drawn as a stroke, about 3 periods
  across the half-face: flat tops at ±7 units, vertical edges with 1.5-unit fillets. Towards the rune,
  each pulse is narrower and lower (widths 6 → 4.5 → 3 → 2 units, heights 100 → 75 → 50 → 30 %), as a
  fuzz gating out on its decay. The last two pulses are at 60 % and 35 % opacity, so the wave dies into
  the rune. Engraved lines (the finish's usual darker stroke). The wave stays inside the rune band
  (amplitude ≤ 0.6 of the cartouche's height), clear of the four knobs.
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-fuzz": {"name": "Fuzz", "blocks": ["fuzz"], "knobs": ["fuzz", "tone", "level", "guitar_vol"],
               "selectors": ["model"], "toggles": ["buffer"], "category": "drive", "color": "#b0602a"}
  ```
- **Octave Up does not change this face (decided by Raph, 2026-11-26, O6).** gfx-fuzz's `faces.json`
  entry, `model` selector, finish, motif, glyph and UI bundle stay as shipped. Octave Up gets its own
  face as the pedal **gfx-octafuzz**, with `octave` on it as a knob (O5 answered by the pedal); its face
  entry, finish, motif and glyph are written in the `gfx-octafuzz/README.md` stub at coding time. Its
  `bias_v` meter reads Q3.
- **Glyph** (`visuals/glyphs/fuzz.svg`): a square wave that narrows and shrinks, the sputter.

  ```svg
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>Fuzz (gfx)</title><path d='M2 17V6h5v11h2.5V8.5H13V17h2.5v-6h2.5v6h2v-3.5h1.5V17H22'/></svg>
  ```
- No maker's livery (no round enclosure, no muff's silver-and-red), no logo or trademark.

## Open questions for review — answered

1. **The real guitar knob.** No estimator in v1, and none planned: a level cannot tell a rolled-back
   knob from a soft pick, which is exactly the difference the fuzz reacts to. `guitar_vol` on EXP is
   the answer; the "Real-knob proxy" row sizes what a player who uses the real knob loses.
2. **golem's DIs.** `~/golem/library/di-refs/index.json` says "DI sèche (Volt)": a Universal Audio Volt
   Hi-Z input, 1 MΩ by its published specification [M, retailer listings]. The load assumption holds.
   The index does not name the pickups (6-, 7- and 8-string guitars; the last two may be active).
   Asked of golem-rack: add `pickup` (and, if known, the pot values and cable length) to each DI
   reference and to the lane's instrument, so golem sets `pickup` at insert. The files peak at −3 to
   −8 dBFS (lead-clean −15) and fall 30 – 45 dB from their body to 4 – 8 kHz. The interface's gain is
   unknown, so "0 dBFS = 1 V" is a convention: `input` is the calibration, also set at insert.
3. **DK vs WDF.** DK, settled by the shared module: one solver for gfx-dist and gfx-fuzz. A face's two
   transistors in one feedback loop are an R-type WDF with a multiport nonlinearity, i.e. the same
   Newton behind more machinery.
4. **Octave Up**: the transistor phase-splitter type in v1 (§2.3); the transformer type appended once
   a part is measured. **Reopened 2026-11-26**: Octave Up is the transformer type (§2.3, open questions
   below); the phase splitter becomes a later Octave Split. Decided 2026-11-26: it ships as the pedal
   gfx-octafuzz, not as a `model` option.
5. **Labels.** Kept: Ge Pair, Si Pair, Ge Three, Si Four, Octave Up; Classic, Early, Triangle, Russian,
   Bypass. Generic, short, and they say what the circuit is; "face", "bender", "muff" live in the
   descriptions and the presets carry the recognisable names. gfx-dist does the same (Op-Amp Dist,
   Discrete OD…).
6. **Finish**: anodized copper (§Visual).
7. **CPU**: the real cost is 9 – 26 % of a core, accepted up to ≈ 20 % (§2.8). 4× for every model is not
   possible (§2.4); the factor is per model (4× / 8× / 16×), not a user option.
8. **Gummel-Poon**: phase 2, as proposed, and only for the germanium models (2.2× the cost on the
   face by DAFx-17's table). The graded Ebers-Moll pair is voiced first.
9. **Values marked [C / U]**: entered and checked at coding, one named document per model. The leakage
   doubling rate is no longer a separate constant (§2.5).

## Open questions for review — Octave Up (2026-11-26)

For a reviewer, before coding. The figures come from the prototype in `proto/` (`octave.py`: the netlist
and the coupled-winding solve; `octrun.py`, `octsweep.py`, `octwave.py`, `octnotes.py`: the octave;
`octcut*.py`: the cut; `octalias*.py`: aliases; `octports.py`: junctions; `octthump.py`: the envelope).
Every run was capped (`systemd-run … MemoryMax=3G`).

**After the review of 2026-11-26** (last section; its runs are `proto/octreview.py`). Closed: O4 (the cut
stands, with the output unloaded), O7 (20 Hz), O8 (the rank is taken when the model ships), O9 (the
device constants move the octave by 2.3 dB at most: they are not where the uncertainty is). Half
answered: O2 (the old row was wrong, the model is not; the level at which the octave gives way still
needs a recording). Open, each with an owner in the review's last table: O1, O3, O5 and O6.
**Decided by Raph, 2026-11-26:** O6 (the pedal gfx-octafuzz), O5 with it (`octave` on its face), O8
(moot: Octave Up takes no rank in gfx-fuzz's `model`), O3 (measure first; per-pedal exception accepted
above the contract), O1 (Raph measures a 42TM022). Still open: O2.

- **O1. L_m of the 42TM022.** It is the design's only number that matters and is not documented (§2.3):
  it changes the small-signal octave by 18 dB over 0.3 – 3 H. Proposed: an LCR measurement of one part
  (600 Ω winding, open secondary, 100 Hz – 1 kHz, at 0.1 and 1 V), and both DCRs, before voicing; 1 H
  until then. Same question for the Dunlop part (3.6 : 1 + 1 [F / U]) if the maintainer wants that ratio
  as well: in the cut model it only changes `a` and L_m.
- **O2. Is the octave right at playing levels?** On the prototype the octave dominates only while the
  coupling capacitors hold the bias: +24 dB at 30 mV, +1.6 dB at 0.1 V, −9 dB at 0.3 V (440 Hz, `fuzz`
  70 %). It grows on high notes and on the decay. This matches the player lore (neck pickup, tone down,
  above the 12th fret) but contradicts the old bench row. A recording of a real Octavia-type pedal at
  known input levels (or golem's NAM `octavia` capture, fed sines at 30 mV / 0.1 / 0.3 V) settles it. If
  the real pedal keeps its octave at 0.3 V, suspect C1 / Q1's base charge (the duty shift) and the 1N34A
  fit first.
- **O3. CPU.** Estimated 8 – 10 µs per sample at 8× (40 – 50 % of a core), with 4× ruled out (−41 dB:
  the fuzz stage itself aliases). Ship above the ≈ 20 % contract like Si Four at 23 %, wait for the
  solution cache like Si Pair, or add the forward-only BJT option (Q2: one port fewer, ≈ −15 %, a new
  branch only)? The coded figure decides; the bench's worst block matters more than the mean
  (prototype: 3 % of samples above 8 iterations at 1.2 kHz, 20 % at 5 kHz).
- **O4. The cut.** The transformer becomes one inductor plus ideal buffers, dropping the rectifier's
  reflected current and the winding capacitance. Measured: the null against a coupled solve without
  winding capacitance is −37 to −58 dB, and the harmonics agree within 0.1 dB with it. Is that accepted
  as a modelling choice (it is exact in `circuit_nl`'s cut rules: a `Buffer` output), or does the
  reviewer want the coupled-inductor element after all (a 3 × 3 trapezoidal companion: a new `Element`,
  stamped only when present)?
- **O5. `octave` in the expert panel, not on the face.** The roadmap lists "un mélange de l'octave" among
  the extra controls. Here it is a linear crossfade from the fuzz tap to the rectified output (100 % =
  stock), in the Blend group. Alternatives: a fifth knob on the face (redraws a shipped pedal), or a
  diode-lift switch like the clones (gives a half-wave octave, not "fuzz alone").
- **O6. A pedal or a model?** The roadmap names a new pedal (`gfx-octafuzz`, "GoleM FX Octafuzz", with
  its own face and presets). This design makes it a gfx-fuzz model instead: the guitar network, `bias`,
  `temp`, `voltage`, `sag`, the solver, linked mono and the bench come for free, and golem's catalogue
  gets no new block type. A separate face could still be a second plugin class of this crate with
  `model` pinned. The maintainer chooses. The roadmap's "Fait quand" rows are met either way: the octave
  row of §5, the alias rule at 8×, latency 0, the build-ui cut check.
- **O7. The thump.** A 20 Hz second-order DC guard on Octave Up only (the real pedal sends its envelope
  to the amp, §2.3). Accept it, or keep the 5 Hz guard and leave the thump to the amp model?
- **O8. Enum and labels.** `model` becomes Ge Pair · Si Four · Octave Up (rank 2). If Si Pair or Ge Three
  ships first, Octave Up takes the next rank instead; ranks never move once shipped. The label stays
  "Octave Up" (no maker's name); the presets carry the references in their descriptions only.
- **O9. Device constants [U].** The 2N4401 / 2N5087 Is and β_R come from common SPICE models, the β from
  Retrograde24's hFE, and the 1N34A fit (Is 0.1 µA, N 1.3) is not a published model. The DC point checks
  the transistors to 0.1 – 0.2 V; nothing checks the diodes except listening. The clone documents also
  recommend BAT46 Schottkys over the 1N34A ("performs better" [C: Aion FX]). A diode choice as an expert
  enum is left out on purpose.
- **Not run on the prototype** (for the bench): `guitar_vol` cleanup through the pickup, `voltage` 3 /
  18 V transients, `temp` extremes, +24 dB / DC / full-scale noise, chords, the zipper on `octave`.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good**, Strong once `circuit_nl` passes its bench on a face and the germanium pair is
voiced. The idea is right and nobody ships it: the fuzz solved as a circuit **with the guitar in the
netlist**. BYOD's face sees an ideal source; the modelers answer with hardware. The survey is honest
and its key source checks out (DAFx-17's tables were re-read: every quoted parameter is exact). The
designer's own numbers reproduce to the last digit (AC128 bias 7.95 / 5.25 / 8.32 V, silicon 3.03 V and
0.56 / 0.38 V starved, 18 kΩ for 4.5 V, the muff's 8.7 dB scoop at 1.64 kHz, 3.9 dB at 790 Hz with the
mids mod). What did not hold is what was built on them: the source model counted the pickup twice,
the germanium pair as specified is a dead pedal, the leakage control did nothing, the trimmer could
not bias the silicon model, the Newton rules, the muff's partition, the −80 dB alias claim and the CPU
estimate. All are fixed above; none changes the architecture.

Prototype (`proto-gfx-fuzz/` in the review scratchpad: `sim.py`, a small MNA + Newton simulator with
junction ports; `face.py`, `dc3.py`, `dc4.py` operating points; `pickup.py` source model; `tr.py`,
`exp1.py`, `clean2.py` transients, Newton statistics, cleanup; `alias.py`, `amuff.py` aliases;
`muff.py` the partition; `stack.py` the tone stack). Figures marked [P] come from it. The muff netlist
uses typical values from memory and the silicon junction capacitances are guesses: its figures size the
problem, the bench settles them.

| # | Change | Why |
|---|---|---|
| 1 | **Source model rewritten** (§2.2): the DI is the guitar's loaded output, the EMF is rebuilt by a band-limited inverse filter, the guitar's own network is in the netlist; invariant "Hi-Z load → the DI"; tone pot added; Active at 10 kΩ | The draft took the DI for the coil's open-circuit voltage ("within 0.5 dB below 5 kHz") and loaded it again with pot and cable. The DI already holds the resonance: +3.5 dB at 4 kHz, −14 dB at 10 kHz re the EMF. The draft's model was wrong by +4 to +7 dB at the peak and −12 to −40 dB above, at every setting [P] |
| 2 | **Cleanup explained and measured** (§1.1, §5) | The mechanism is current drive of the first base plus the treble coming back, not "logarithmic compression". On the prototype the unbuffered face is 5 – 8 dB cleaner than the buffered one at every knob position (germanium) and 17 – 29 dB (silicon), at almost constant output level. The pedal does what it claims; the bench row now carries those figures instead of "≥ 6 dB at equal level" |
| 3 | **Germanium devices: a graded pair**, raw DAFx-17 pair kept for the tests (§2.3) | Two measured AC128 (β 45, I_CEO 262 µA) bias at 8.2 V and cannot be trimmed into a pedal that survives ±10 °C |
| 4 | **`leakage` redefined**: relative (0 – 400 %), scaling the collector junction's saturation current; no extra source (§2.5) | Ebers-Moll already leaks (I_CEO = Is·(1 + (β_F + 1) / β_R)); the added source counted it twice and moved the bias by 0.03 V at its default [P]. Relative, because devices differ per model (contract) |
| 5 | **Temperature law = SPICE's (with N); T₀ per device set (22.8 °C for DAFx-17's)** | So `to_spice()` + `.temp` describes the same device; the paper's V_T is 25.5 mV |
| 6 | **Bias trimmer = Q2's collector resistor**; `bias` defined by the resistor and the meter, not by "cold / hot" | Q1's resistor cannot bring the silicon face to 4.5 V (4.2 V at 200 kΩ) [P]; which side gates is for the ear |
| 7 | **One solver: gfx-dsp `circuit_nl` / `NonlinearCircuit`**, API table (§2.9); the solve is gfx-dist's (backtracking, `pnjlim`, cap 20, continuation, hold) plus a guard on the extrapolated seed | Two documents specified two solvers. Cap 8 + "keep the last iterate": 0.02 – 15 % of samples hit the cap depending on the input, mean 3 – 6 iterations, not 2 [P]. `pnjlim` was mis-stated as a 2·N·V_T clamp |
| 8 | **Si Four solved whole; partition removed** (§2.4) | −11 to −20 dB against the joint solve, not −50 dB [P]: a clipping stage's input is a low, nonlinear impedance on a collector. The LU it saved is not the cost |
| 9 | **Pots: Woodbury update every base-rate sample** | gfx-dist's measurement: a rebuild every 16 samples leaves −50 to −55 dB of zipper |
| 10 | **Oversampling per model from measured aliases: Ge Pair 8×, Si Pair 16×, Si Four 4×**; rule ≤ −65 dB at 1.2 kHz (target −70) | −80 dB at 8× on every model does not exist: Ge Pair −70 dB, Si Pair −54 dB at 8×; the muff −77 dB already at 4× [P] |
| 11 | **CPU 1.1 – 2.0 → 1.8 – 4.5 µs** (9 – 26 % of a core); fast exp required; worst block benched; accepted up to ≈ 20 % | Iterations and `exp` calls were under-counted, as in gfx-dist |
| 12 | **DC re-solve inline, warm-started**; the 50 ms silent pre-run removed | Simpler, bounded, same as gfx-dist |
| 13 | **`level` 0 dB = unity loudness (K per model)**, range to +24 dB | The models differ by > 6 dB pot-full-up; a default cannot follow `model`. gfx-dist's rule |
| 14 | **Polarity normalised per model** | A three-transistor fuzz inverts: `mix` and `bass_keep` would cancel |
| 15 | **Octave Up = phase-splitter type**; transformer type deferred | No published transformer values; every part of the splitter type is documented |
| 16 | **`voltage` to 18 V**; meters to 18 V; presets added (Winter Garage, Leaky Pair, Eighteen Volts) | The 18 V use is common on silicon fuzzes; the new controls need their uses |
| 17 | **Bench rewritten**: source invariant, noise, polarity, level, leakage, supply rows; reference = offline mode + committed ngspice fixtures; Newton row "0 unsolved" | ngspice is not installed here; the old Newton and partition rows could not pass |
| 18 | **Visual: anodized copper decided**; all open questions answered | Two thrash-red drive pedals already; the crate-finish mechanism exists |

Checked and left as written: the DK formulation; Ebers-Moll + constant junction capacitances first
(DAFx-17's own conclusion), Gummel-Poon in phase 2; no ADAA; the supply / sag network; the gate
explanation (silicon collapses at 4.5 V, confirmed); the muff's tone stack and `mids` mod; reported
latency 0 with `skip_latency`; ids (`fuzz` not `gain` / `drive`, relative `tone`, `model`, shared `mix`,
`bass_keep`, `bypass`), linear ranges, no mode-dependent default, read-only meters; mono first; the
labels; the `sputter` motif and the glyph.

Coverage: the five families are the right five, and `guitar_vol`, `buffer`, `bias`, `temp`, `voltage`
are more than any product surveyed offers. Left out on purpose, first candidates to append: the
transformer octave, an input-capacitor ("fat / treble") switch on the faces, a silicon three-transistor
variant, gate-style fuzzes with their own oscillation controls.

**For gfx-dist to align** (its review is done and already names `circuit_nl`, `NonlinearCircuit`,
`Oversampler8x`; this document now follows it):

- its API table says "current sources (leakage)" and "the pickup as a Thevenin source": leakage is a
  BJT parameter (collector-junction saturation current) and the pickup is an EMF filter + network;
  the module needs L, not a leakage source;
- §2.2 step 1: extrapolate the seed only from samples solved by plain Newton (one fallback otherwise
  poisons the following samples [P]);
- the "approximate cut" mode: measured here at −11 to −20 dB on the circuit it was meant for; keep it
  out of the first version;
- BJT constants as run-time parameters with the SPICE temperature law and a T₀ per device set; its
  booster's transistor uses the same `Bjt`;
- a 4× path (two half-band stages) beside `Oversampler8x` / `16x`; node-voltage read-out for meters;
  `to_spice()` with committed fixtures; one fast exp in gfx-dsp for both pedals;
- alias policy: −85 dB there, −70 dB at 1.2 kHz here, each argued; one sentence in both documents once
  the maintainer has chosen.

Still open:

| Point | Owner |
|---|---|
| Who creates `circuit_nl`; first coding task = the solver on Ge Pair and on gfx-dist's Op-Amp Dist, before any plugin code | golem-fx |
| The graded germanium pair (β 75 / 120, I_CEO 80 / 200 µA) is a starting point: by ear against the raw pair, or a measured pair | The maintainer |
| CPU: up to ≈ 20 % of a core per mono instance accepted here; confirm, or cap lower (then the solution cache first) | The maintainer, after the bench |
| Alias policy −70 dB (here) vs −85 dB (gfx-dist); Si Pair at 16× depends on C_cb / C_eb from a datasheet | The maintainer; bench |
| Cleanup depth: the model gives 5 – 8 dB (Ge) and 17 – 29 dB (Si) of THD advantage over a buffer; whether that matches a real pedal needs a real pedal or a trusted recording | The maintainer, listening |
| Pickup of each DI and of each lane's instrument (`pickup`, `input` set at insert); real L / R / C of the maintainer's guitars if he can measure them | golem-rack, the maintainer |
| Catalogue entry `fuzz` (new type, category drive); whether the archetypes `fuzz-face`, `big-muff`, `tone-bender`, `octavia`, `bass-fuzz` also offer gfx-fuzz with `model` pinned; EXP on `guitar_vol` as the default assignment | golem-rack |
| Add `level` (0 dB = unity) to the contract's shared-id table, with gfx-dist | The maintainer (contract) |
| Netlist values [C / U] of Ge Three, Si Four's variants, Octave Up; silicon device constants; the fast exp's speed | golem-fx, at coding |
| Copper next to gfx-crusher's raspberry and the thrash reds at tile size | The maintainer, on the family sheet |

**Enum order (decided 2026-11-15):** `model` lists the models in the order they ship — Ge Pair, Si Four in
v0.1.0 — and every later model (Si Pair once its CPU fits, Ge Three; Octave Up no longer, it ships as gfx-octafuzz, 2026-11-26) is appended at the end, as the
contract requires. The design's table order is documentation only; option ranks are what sessions store.

## Review (Fable 5.2, 2026-11-26) — Octave Up

**Verdict: Good.** Strong once the transformer has been measured and the CPU question is decided. The
design chose the right circuit, read it from named documents and reproduced a builder's measured
voltages. Its central result is the kind a static octave fuzz cannot give: the octave depends on the
level because the input clamps, so it blooms as the note decays. The review confirmed the netlist on a
third document, re-ran every prototype script (all figures reproduce) and extended the prototype where
the design had not looked: through a pickup, into a cable, the core's flux, the device constants. No
error touches the architecture. Two things keep the model from shipping, and no document settles either.

Can it rival the best emulations? On structure, yes: the guitar is in the netlist, the fuzz and the
transformer are solved together, and the octave's level dependence comes from the circuit and not from a
voicing curve. On accuracy, not yet provable: the purity of the octave hangs on one unpublished number
(L_m), and nothing has been compared with a real pedal. The bench's listening row and one LCR
measurement decide it, more than any further design.

Sources read by the review: Aion FX Octagon 1.0.0 (2024-07-04; parts list, build notes, printed
schematic), madbean Retrograde24 (2024-11-21; BOM, build notes, voltage page) and Retrograde (2022),
Fuzz Central's Octavia page. Not reached: PCB Guitar Mania's Octavius, the freestompboxes thread, any
datasheet or measurement of the 42TM022's inductance (two searches, nothing).

| # | Change | Why |
|---|---|---|
| 1 | **Netlist confirmed on a printed schematic** (Octagon); the sentence on the clones' transistors corrected | Every value and connection of §2.3 matches, including the point most easily got wrong: the 600 Ω side is driven (pins 6 – 4, centre tap open) and the 1.5 kΩ side feeds the diodes with its centre tap grounded. "Every clone uses a 2N5087 and two 2N4401" was not true: Octagon uses a 2N3906 and two 2N3904 |
| 2 | **DC point at 9.26 V**, not 9.17 V; bench row updated; `octave.py`'s bias table printed with the README's sign | Retrograde24 reads the rail itself (Q2 C 9.26 V). The fit stays within 0.15 V (Q1's base after the meter's loading). The script printed `bias` with the opposite sign to §2.3 |
| 3 | **The old bench row was wrong, the model is not** (§2.3); the Octave row rewritten with margins, an attack mark and a guitar mark | The row predates any netlist. The loss of the octave at 0.3 V is the input clamp (Q1 saturated, its junction into 1.3 kΩ, C1 discharging in 37 ms). It does not depend on the source: Single Coil and Humbucker give the buffered figures within 2.1 dB [P]. The design's marks sat on its own measured values (+24.2 for a mark of +15) with no row for the attack; the new row fails if the clamp disappears |
| 4 | **L_m: what it moves is the fundamental's null**, not the octave; 1 H is probably high | Over 0.3 – 3 H the octave's own level stays within 1 dB and H1 moves by 18 dB. Two rough estimates (telephone band, core geometry) give 0.1 – 0.5 H. At 0.3 H the octave still leads on high notes, by +2 to +6 dB instead of +20 |
| 5 | **Saturation figures redone as flux linkage**; flyback figures completed | The volt-second comparison mixed an rms and a peak. In the model the flux is one-sided and set by Q3's on-time: 8.8 mWb on a guitar's low E against 8.25 mWb rated, 13.3 mWb on a bass's. The collector reaches 18 – 22 V on low notes, not 10.9 V |
| 6 | **Winding capacitance: the reason corrected** | The −6 to −19 dB "null" is a 265 kHz ring folded by the trapezoidal rule. In the audio band 50 pF changes the output by −57 dB. Dropping it is right |
| 7 | **The rectifier's load: stated and decided** (unloaded) | The design never loaded the output. A cable's 470 pF – 1 nF takes 5 – 11 dB off H2 − H1 at 659 Hz, and with it the cut is no longer exact (−15 to −18 dB, through a leakage that is [U]). The model stops at the pot; comparisons must state their cable |
| 8 | **`octave`: the claim about the clones' switch corrected** | Octagon's switch has three positions, and they are this control's 100, 50 and 0 % to first order. The "fuzz alone" exists on the clones |
| 9 | **CPU: the precedent is Si Pair**; bench first; forward-only Q2 argued by topology; two inputs instead of an op-amp in stage 2 | 40 – 50 % is the figure at which Si Pair was coded and not shipped. No cheaper structure exists (a cut in the loop, 4×, a cheaper stage 2: all measured or ruled out) |
| 10 | **Aliases: reproduced and explained** | −40.8 / −76.2 dB at 4× / 8×. The 35 dB step comes from edges of 1 – 8 µs, set by C4 and Q3's gain: the factor is tied to those parts |
| 11 | **DC guard 20 Hz: decided** | Figures reproduced (1.41 → 0.31 V). The amp behind is a capture trained on guitar signals |
| 12 | **Device constants: closed** | Measured: 2.3 dB at most on the octave, 0.07 V on the bias, for any plausible diode or β |

Verified with numbers (review runs, `proto/octreview.py`, plus the design's scripts run again):

- DC point, `bias`, `temp`, `voltage` tables: identical. H2 − H1 tables of `octrun.py`: identical
  (+24.2 / +1.6 / −9.1 dB at 440 Hz; the review's longer settling gives +23.8 / +1.9 / −9.1).
- Through the guitar (EMF level, `fuzz` 70 %), 30 mV / 0.1 V / 0.3 V. Buffer: 82 Hz 0.0 / −3.8 / −10.8; 196 Hz
  +11.8 / −0.6 / −12.8; 440 Hz +23.8 / +1.9 / −9.1; 659 Hz +21.6 / +2.6 / −7.1; 988 Hz +20.5 / +1.9 / −5.8 dB.
  Single Coil: +0.7 / −3.7 / −9.7; +13.4 / −0.2 / −11.2; +25.2 / +2.3 / −9.1; +22.8 / +2.8 / −7.7; +20.4 / +1.4 /
  −6.6. Humbucker: within 2.1 dB of the buffer. Duty 0.49 / 0.55 / 0.65 at 440 Hz and above, on all three.
- L_m 0.3 / 1 / 3 H (buffer, output on 1 MΩ ∥ 470 pF): 440 Hz 30 mV +4.9 / +22.1 / +19.6; 659 Hz 50 mV +1.9 /
  +15.6 / +8.9; 988 Hz 50 mV +4.0 / +18.0 / +6.5; 82 Hz 50 mV −14.6 / −2.9 / +6.4 dB.
- The cut against the coupled solve: −37.6 to −58.4 dB unloaded (the design's figures); −15 to −18 dB
  with 470 pF on the output at 440 Hz 0.3 V and 1.3 kHz, in band, at 176.4 and 705.6 kHz.
- Aliases at 1.2 kHz: −40.8 dB (4×), −76.2 dB (8×); stage 1 alone −45.9 / −83.9 dB; Newton 4.4 iterations
  on the coupled solve, 3.5 on stage 1 at 8×, no fallback, no unsolved sample.
- The envelope: 1.41 V of infrasound through the 5 Hz guard, 0.31 V through 20 Hz, for 0.92 V rms of audio.
- No slow oscillation of the 33 µF against L_m under a steady tone: the duty stays at 0.68 ± 0.01 over
  0.4 s at 0.3, 1 and 3 H.

Checked and left as written: the transformer type over the phase splitter; the three reasons given; the
negative-ground conversion; `bias` on R2 with its sign; `bias_v` on Q3's collector; core saturation and
winding capacitance left out; the cut at an ideal source and "no gfx-dsp change"; 8×; `octave` as a
crossfade from the winding tap, a new id appended after `bypass`; the presets; the label "Octave Up".

What stays open:

| Point | Owner |
|---|---|
| **O1. Measure a 42TM022** (a few euros): L of the 600 Ω winding at 100 Hz – 1 kHz and 0.1 – 3 V, both DCRs, leakage (secondary shorted), and the current at which L drops. Until then 1 H, probably high; the Octave row's marks are derived again afterwards. **Decided by Raph, 2026-11-26:** Raph measures one 42TM022 (inductance vs level, DCRs, leakage, saturation knee); until then the 1 H working value and the 0.1 – 0.5 H estimates stand, and the bench marks are derived again after | Raph (owner); golem-fx for the marks |
| **O3. CPU.** Code the two-stage netlist in gfx-bench first, with the forward-only Q2, `set_fast_solve` and `set_quadratic_seed`; report mean and worst block. Then: under ≈ 20 % it ships; above, the maintainer decides on the figure, or it waits for the solution cache with Si Pair. The review's expectation: 30 – 40 % after the cheap steps, so a decision will be needed. **Decided by Raph, 2026-11-26:** code the netlist in gfx-bench first and measure; a per-pedal CPU exception for gfx-octafuzz is accepted if the figure after the cheap steps stays above the contract (expected 30 – 40 %) | golem-fx at coding |
| **O6. A pedal or a model.** The review recommends the roadmap's pedal, as a thin crate: `gfx-octafuzz` depends on gfx-fuzz (already an rlib, with the netlist as an internal kind that `model` does not list, as Si Pair is today), fixes the circuit, and has its own face and presets. Reasons: the roadmap was decided by the maintainer the same day and a design document does not overturn it; `octave` is the control the roadmap asks for and belongs on a face (O5 then answers itself); gfx-fuzz's `model` enum, face and UI bundle are not touched, so nothing shipped can move; a CPU exception, if one is granted, goes to one pedal and not to one position of a selector. §2.3, §2.7 and §2.8 hold either way; §3 and the face paragraph of §6 are written for the model variant and change if the pedal is chosen. **Decided by Raph, 2026-11-26:** the separate thin pedal **gfx-octafuzz** ("GoleM FX Octafuzz") over gfx-fuzz's engine; gfx-fuzz's `model` enum and face untouched; `octave` on gfx-octafuzz's face. §3 and §6 rewritten accordingly; the DSP stays in this README, a `gfx-octafuzz/README.md` stub points here at coding | Decided |
| **O2. The level at which the octave gives way** (model: about 0.1 V at 440 Hz, `fuzz` 70 %). A real pedal or golem's NAM `octavia` capture fed 440 and 659 Hz at 30 mV, 0.1 V and 0.3 V, the cable stated. If the real one keeps its octave louder, the suspects are, in order: L_m, the cable, then the clamp | The maintainer, listening; golem-rack for the capture |
| The cable on the rectifier: left out. If the A / B shows too much octave on the highest notes, add 1 MΩ ∥ C on stage 2's output (one state) before touching any device constant, and accept the cut's −15 dB or code the coupled element | golem-fx, after listening |
| Core saturation on bass (1.6× the rated flux at 41 Hz in the model): the diagnostic is in the bench; a saturating inductor only if the measurement of O1 shows the knee below 13 mWb | golem-fx, after O1 |
| The Dunlop part (3.6 : 1 + 1, [F / U]): not pursued; it changes `a` and L_m only | The maintainer, if wanted |
| Catalogue: a new block type for the pedal (the pedal being decided, O6), or `model` pinned on `fuzz` for the archetype `octavia` (no longer possible); EXP on `octave` or on `guitar_vol` as the default assignment | golem-rack |
