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
> where it costs ≈ 9 µs per sample (44 %). Ge Three and Octave Up are not coded (no netlist entered from
> a named document). Deviations and numbers: "As coded" below.

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
at the contract's ≈ 20 %; Si Four is 15 % above it and ships for Raph to decide (the next step is §2.8's
block Gauss-Seidel); Si Pair, at twice the bound, waits for the solution cache of §2.8.

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

**Not done (for later or for Raph).**
- Ge Three and Octave Up: no netlist entered from a named clone document; Si Pair: CPU. The `model`
  enum lists the shipped ones in the README's order (Ge Pair, Si Four), as the coding brief asks;
  appending Si Pair later puts it after Si Four, against the README's order, or reorders the enum
  (contract: enums only grow at the end): **Raph decides** before Si Pair ships.
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
with its tone-stack variants and a mids control, and a rectifying **octave-up** fuzz. The
gated "velcro" sound is not a separate model: it is a starved supply and a cold bias, available on every
model (§2.6).

Markers: **[V]** read in the source or the paper · **[M]** manufacturer page or manual · **[C]** DIY clone
schematic or kit document · **[A]** calculation (method stated, script reproducible) · **[F]** forum or
review opinion · **[U]** not verified · **[P]** measured on the review's prototype (numpy, full MNA +
Newton with trapezoidal companions: the same equations as the DK solve).

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
| The octave's rectified tone | A phase splitter and two diodes, with their own impedances and thresholds | In the netlist |

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
- **`pickup`** sets the values (typical, [U]; refined against measurements if Raph has any). The same
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
    the set whose waveforms the paper publishes. The graded figures are a voicing start [U]: Raph's
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
- **`model` Octave Up.** Decided (open question 4): the **transistor phase-splitter octave**, not the
  transformer one. A gain stage, a transistor with equal collector and emitter resistors (two outputs
  in opposite phase), two germanium diodes that pass each half in turn (full-wave rectification), an
  output stage [F / C: the family of the transformerless octave fuzzes; one clone document is named at
  coding and its values read from it]. Every element is already in the solver and documented, where
  the transformer type needs the magnetizing and leakage inductances of a part nobody publishes.
  The transformer type is appended later as a sixth `model` once a part has been measured (LCR meter:
  primary inductance, leakage inductance, winding resistances); `circuit_nl` carries coupled inductors
  from its first version for that (§2.9).
  Octave strongest on single notes, neck pickup, above the 12th fret; chords give ring-mod-like
  intermodulation, as the real one [F].

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
    Ge Three: 6; Si Four: 10; Octave Up: ≈ 8.
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
    Si Pair 16×, Si Four 4×**; Ge Three and Octave Up start at 8× and take what the bench says
    (`Oversampler8x`, and gfx-dist's `Oversampler16x`; one stage fewer per doubling of the host rate).
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
  collector resistor; on Si Four, on the first clipper's base bias resistor; on Octave Up, on the gain
  stage's. A read-only meter **`bias_v`** shows the magnitude of Q2's collector voltage (Si Four: the
  first clipper's), the number a builder reads with a multimeter.
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
  | Octave Up | ≈ 16 | ≈ 8 | 8× | ≈ 4.5 µs | ≤ 5.5 µs |

  That is **9 – 26 % of one core**, not 5 – 13 %: by far the heaviest golem-fx block per channel
  (gfx-drive 736 ns stereo, gfx-emet 198 ns). With `f64::exp` (15 – 20 ns) every line is 1.5 – 2× higher,
  which is why the fast exp is part of the design.
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
  | Linear elements | R, C, **L**, coupled inductors (linear transformer: later octave model), ideal op-amp; `VSource` with DC value and series resistance (battery); `ISource` | L, battery, coupled inductors: gfx-fuzz. Op-amp: gfx-dist |
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
| `model` | Model | e | Ge Pair · Si Pair · Ge Three · Si Four · Octave Up | Ge Pair | 30 ms crossfade | | The circuit (§2.3) |
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
| `bias_v` | Bias (V) | f, read-only | 0 – 18 V | — | — | | Q2's collector voltage (magnitude, 50 ms average), as a multimeter reads it |
| `supply_v` | Supply (V) | f, read-only | 0 – 18 V | — | — | | The circuit's supply now, sag included |

No default depends on another parameter. `leakage` is relative because the devices differ per model
(contract: a parameter that must follow the model is a percentage of the model's own). `stack`, `mids`
are inert outside Si Four, `leakage` on the silicon models: golem greys them through `depends` (the descriptor's groups). Expert groups (for
`visuals/data/groups.json`): **Guitar** (`pickup`, `buffer`, `input`), **Circuit** (`bias`, `temp`,
`leakage`), **Power** (`voltage`, `sag`), **Muff** (`stack`, `mids`), **Blend** (`mix`, `bass_keep`).

## 4. Applications

Presets become `presets/gfx-fuzz.json`, level-matched to the bypass (±2 LU on the bench DIs) with
`level`. No artist or trademark names.

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
| **Octave Ring** | Octave Up, `fuzz` 80 %, neck pickup, play above the 12th fret | Rectified octave (phase splitter) |
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
| Polarity | Every model, 1 kHz at 1 mV, `mix` 100 %: output in phase with the input (±90°); `bass_keep` 120 Hz: no notch deeper than 3 dB at the crossover |
| Level | Defaults, the guitar DIs, each model: loudness within ±1 LU of the bypass (K, §2.1) |
| Newton | DIs and the test signals, every model, 44.1 – 192 kHz: **0 unsolved samples**; continuation used on ≤ 0.5 % of samples on the DIs; mean iterations ≤ 3.5 on the DIs (reported for +24 dB, DC, full-scale noise: no NaN, no unsolved); det J keeps its sign over the port range at every pot position |
| Aliasing | −12 dBFS 1.2 kHz and 5 kHz sines, `fuzz` 100 %, `buffer` on, each model at its factor: worst alias in 20 Hz – 20 kHz ≤ −65 dB re the fundamental at 1.2 kHz (target −70); the 5 kHz figure and the next factor up are written into the status line |
| Octave | Octave Up, 440 Hz at 0.3 V: the 880 Hz component ≥ the 440 Hz one |
| Gate | Velcro Gate preset on a decaying note: the output falls 40 dB faster than the input after the gate point |
| Sag | `sag` 100 %, a hard chord: `supply_v` droops and recovers with the ≈ 94 ms constant |
| Zipper | `guitar_vol`, `fuzz`, `tone`, `bias` swept by EXP steps: control-rate sidebands ≤ −80 dB (contract rule) |
| Determinism, CPU | Strict null; §2.8 bounds per model, mono, 48 kHz, **mean and worst block**; fast exp against `f64::exp`: null ≤ −100 dB |
| Listening | lead.wav, crunch.wav and clean.wav (Raph's DIs) into a NAM clean and edge-of-breakup amp; A / B against golem's NAM captures of the same families (`fuzz-face`, `big-muff`, `tone-bender`, `octavia`), against gfx-drive Fuzz, the volume-knob roll played live on EXP; high bends at each model's factor against the next one up; graded pair against the raw DAFx-17 pair |

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
   a part is measured.
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
  Raph has chosen.

Still open:

| Point | Owner |
|---|---|
| Who creates `circuit_nl`; first coding task = the solver on Ge Pair and on gfx-dist's Op-Amp Dist, before any plugin code | golem-fx |
| The graded germanium pair (β 75 / 120, I_CEO 80 / 200 µA) is a starting point: by ear against the raw pair, or a measured pair | Raph |
| CPU: up to ≈ 20 % of a core per mono instance accepted here; confirm, or cap lower (then the solution cache first) | Raph, after the bench |
| Alias policy −70 dB (here) vs −85 dB (gfx-dist); Si Pair at 16× depends on C_cb / C_eb from a datasheet | Raph; bench |
| Cleanup depth: the model gives 5 – 8 dB (Ge) and 17 – 29 dB (Si) of THD advantage over a buffer; whether that matches a real pedal needs a real pedal or a trusted recording | Raph, listening |
| Pickup of each DI and of each lane's instrument (`pickup`, `input` set at insert); real L / R / C of Raph's guitars if he can measure them | golem-rack, Raph |
| Catalogue entry `fuzz` (new type, category drive); whether the archetypes `fuzz-face`, `big-muff`, `tone-bender`, `octavia`, `bass-fuzz` also offer gfx-fuzz with `model` pinned; EXP on `guitar_vol` as the default assignment | golem-rack |
| Add `level` (0 dB = unity) to the contract's shared-id table, with gfx-dist | Raph (contract) |
| Netlist values [C / U] of Ge Three, Si Four's variants, Octave Up; silicon device constants; the fast exp's speed | golem-fx, at coding |
| Copper next to gfx-crusher's raspberry and the thrash reds at tile size | Raph, on the family sheet |

**Enum order (decided 2026-11-15):** `model` lists the models in the order they ship — Ge Pair, Si Four in
v0.1.0 — and every later model (Si Pair once its CPU fits, Ge Three, Octave Up) is appended at the end, as the
contract requires. The design's table order is documentation only; option ranks are what sessions store.
