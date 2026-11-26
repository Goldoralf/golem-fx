# gfx-dist

> **Status: CODED** (v0.1.0, 2026-11-15). Designed 2026-11-14 by Claude Opus 5.5, reviewed by Claude
> Fable 5.2 (Review at the end). The shared solver is built first, as the Review asked: new gfx-dsp
> **`circuit_nl`** (`NonlinearCircuit`, nodal DK, Woodbury pots every sample, exact stage cuts, inline
> DC point, op-amp macromodel, diodes, BJT with SPICE's temperature law, `to_spice()`, shared fast
> `exp`), proven on the Op-Amp Dist (0.300 V/µs slew for 0.3, no unsolved sample on any signal or part,
> 0.8 Newton iterations per stage-sample at the defaults), then four circuits ship: **Op-Amp Dist,
> Booster Dist, Blend OD, MOSFET OD**. **The Discrete OD is not coded** (its discrete op-amps need a
> JFET model and a transistor-level reference to fit the macro against): the `circuit` enum lists the
> four in the README's order and the Discrete OD is appended when it ships. Under the owner's alias
> policy every circuit runs at 8× (the Op-Amp Dist's Ideal swap at 16×): worst alias of a 1.2 kHz sine
> at full gain −80 to −88 dB with the circuits' own parts, −71 dB worst swap. Bench 28 / 28, 19
> behaviour tests, CPU 4.2 µs / sample mono at the defaults (20 % of a core, the contract's bound).
> Deviations and numbers: "As coded" below.
>
> **Three more circuits DESIGNED 2026-11-26** (roadmap 0.8, item 3), by Claude Opus 5.5, not coded: the
> **Discrete OD** finished (the BD-2 family, with the "Custom" bass switch as a new `fat` id), the
> **Studio OD** (the ODR-1 family; its Spectrum is its `tone_stack`) and the **Plexi Dist** (a Red
> Rox-type Marshall-in-a-box; its Contour is a new `contour` id), appended to `circuit` in that order after
> MOSFET OD. The four coded circuits are untouched (bit-identical renders, presets by option name). The
> Review's "discrete op-amp macro" does not hold: a pure op-amp macro has **no even harmonics** where the
> transistor stage has −42 dBc of 2nd [P], so each discrete op-amp ships as **a square-law pair port
> whose tail follows the common mode + the real PNP** (3 ports; harmonics 2 – 5 within 0.1 dB of the
> transistor-level stage at every level tried, 2 – 9 within 0.8 dB [P]); `circuit_nl`
> gains a JFET device (the reference) and that pair port. Prototype: `proto/nl.py` and three circuit
> scripts. Design: §1.1 (re-read and new circuits), §2.11, §3, §4, §5, "Visual changes", "Open questions
> (2026-11-26)". **Reviewed 2026-11-26 by Claude Fable 5.2** ("Review … three voicings" at the end): the
> three netlists hold against their documents; the one-port pair with the lagged common mode is verified
> and kept; the Discrete OD's level pot is fixed at 50 %, not 90 % (a clipped stage 2 and `fat` made the
> IC stage clip); the Plexi Dist misses the alias policy with its Contour scooped (−66.6 dB) and at 18 V
> (−60.5 dB): the maintainer's decision. **Decided by Raph, 2026-11-26:** the Contour's scoop is a
> setting (−65 dB bound: it passes); the Plexi's `supply` stops at 150 % (−65.9 dB) and the "18 Volt
> Plexi" preset is dropped; ≈ 24 % of a core is accepted for the Discrete OD and the Plexi Dist if the
> coded measurement is ≤ 5 µs per sample. Names, `fat`, `contour` expert-only at 70 %, the Red Rox: as
> the review settled, confirmed by ear at coding.

## As coded (v0.1.0)

**Files.** gfx-dsp: `circuit_nl.rs` (+ `circuit_nl/tests.rs`, 19 unit tests), `pot.rs` (taper laws),
`oversample.rs` (`OversamplerN`, 1× – 16× f64 cascade; the existing 2× unchanged). Here: `circuits.rs`
(the four netlists, each from the clone document named in §2.1), `lib.rs` (the processor: two slots per
channel holding every circuit compiled, crossfades, `bass_keep` twin, K), `plugin.rs` (class id
`GoleMFxDist00001`, mono 1 / 1 then 2 / 2, latency 0), `tests.rs`, `fixtures/` (the offline reference and
the four ngspice decks).

**The solver as built** (gfx-dsp `circuit_nl`, shared with gfx-fuzz):
- Its own `Netlist` (the brief forbids touching the linear `circuit` module that gfx-emet / gfx-shem use;
  the README wanted "`circuit::Netlist` with new elements"). Elements R, C (trapezoidal, or backward Euler
  per capacitor), L, ideal op-amp, ideal buffer, variable resistors and pots (taper, three-terminal or
  rheostat, ganged by the caller), sources driven by an input vector u, current sources; devices with
  every constant a run-time parameter: clipper (two Shockley strings with an exact series collapse, an
  optional extra junction, an optional MOSFET channel), BJT (Ebers–Moll, NPN / PNP, SPICE temperature
  law, collector-junction leakage scale), op-amp macromodel (tanh or square-law stage into a fixed 30 pF,
  so a part swap changes no matrix; A₀ leak; rails as multiples of `set_supply`).
- Newton, per stage, in port voltages: seed extrapolated only after two plain solves; SPICE limiting
  (`pnjlim` on junctions, the input stage ≤ 3 V_lin, the integrator stopped at its rail's knee when it
  arrives from inside); Armijo backtracking along the Newton direction when the limited step does not
  lower ‖g‖²; stop at |Δv| < 10⁻⁶ V or a residual below it; cap 20. Fallbacks, in order: plain Newton
  from the previous solution, the same without line search (SPICE's way), continuation in p in 4 then 16
  steps (each step retried without line search), then the previous solution **and its currents** are
  held and counted. The two extra tiers were needed: the fast op-amps (TL072, Ideal) on the Booster Dist
  left 35 – 250 unsolved samples per second with backtracking alone (the merit function crawls on the
  rail clamp's exponential); with them, none on any test signal.
- The rail clamp is a soft-plus conductance (knee 50 mV, the full slew current held within 50 mV), not
  an exponential "diode": Newton converges on it quadratically; a piecewise-quadratic knee halved every
  step.
- Woodbury every sample while a pot moves (a k × k solve, k ≤ 8 per stage), nominal at the geometric mean
  of each variable's range; ends clamped at 10 Ω. Checked against a rebuild: < 10⁻⁹ V.
- DC point: caps open, inductors shorted, gmin 10⁻¹² S on every node (the transient carries the same gmin,
  so a start from the DC point is silent to < 1 µV); devices' starting points (op-amps at mid-rail), seed,
  continuation from the seed's inputs, source stepping. Re-solved inline at `reset` and at every slot
  start: 17 – 48 µs for a whole circuit (bound: 133 µs, 10 % of a 64-sample block).
- `fast_exp`: 2^k · degree-8 Taylor by Estrin's scheme, relative error < 10⁻⁹ (null ≤ −180 dB).
- `ac_response` replaces `linearized()` → `circuit::Netlist`: an op-amp macro linearizes to a
  transconductor, which the linear netlist cannot hold; the Laplace analysis runs on the same netlist.
- `to_spice()`: element cards, `.model` cards, the macros as B-sources, `.temp`, `.op`. ngspice is not
  installed: the four decks are committed in `fixtures/` with the offline reference (monolithic, 64×,
  Newton to 10⁻¹²: fundamental and harmonics 2 – 9 at four levels / frequencies / gains per circuit);
  a test keeps the decks in step with the code, another holds the real-time solve to the reference.

**Measured** (tests and `gfx-bench check gfx-dist`):

| What | Target (§5, policy) | Measured |
|---|---|---|
| Small signal (0.1 mV, gain / tone 0 / 50 / 100 %, 200 Hz – 4 kHz) | ±0.25 dB of the linearized netlist | within 0.25 dB, four circuits |
| Anchors (Op-Amp Dist) | filter 470 Hz / 19.3 kHz ± 3 %; 67 dB, −3 dB near 430 Hz; slew 0.9 – 1.05 SR | **541** / 19,383 Hz (the analytic RC with the follower's 1 MΩ gate resistor, which §1.1 left out: 538 / 19,263); peak 64.3 dB (ideal asymptote 67.3), 58.3 dB at 430 Hz; slew 0.302 V/µs for 0.300 |
| Large signal vs reference | harmonics 2 – 9 within 1 dB (≥ −60 dBc) | within 1 dB, 16 cases; fundamentals within 0.5 dB |
| Stage cuts vs monolithic, tol 10⁻¹² | ≤ −100 dB | −156 / −185 / −146 / −170 dB |
| Newton (squares and noise +12 dBFS, every `opamp` / `diodes` option; 44.1 – 192 kHz) | 0 unsolved, mean ≤ 3 | 0 unsolved, no NaN; 0.80 / 0.92 / 0.70 / 1.00 iterations per stage-sample at the defaults, worst sample 6; worst 32-sample block 3,779 iterations (all stages, all oversampled samples) |
| Uniqueness | det J keeps its sign | over an 11-point grid per port, every stage, pots 0 / 50 / 100 % |
| Stiff modes | ≤ −80 dB above fs / 4 | the band 0.4 – 0.5 fs is no louder than the same band solved at 2 fs (−82 vs −75 dB on the Op-Amp Dist): **no stiff mode**; the absolute −80 dB cannot hold on a clipped wave (−63 to −82 dB of genuine harmonics there), so the test compares against the finer solve. The 100 pF stays trapezoidal |
| Aliasing, 1.2 kHz, −12 dBFS, full gain, tone open (policy) | ≤ −70 dB own parts, ≤ −65 dB any swap | Op-Amp Dist −87 / −71 dB (TL072; Ideal at 16× −112), Booster Dist −80 / −71, Blend OD −85 / −82, MOSFET OD −88 / −85 |
| Taper | ±0.3 dB, gain 0 … 100 % | within 0.3 dB |
| DC point, activation / reset / switch | ≤ −60 dBFS | below −60 dBFS on all four |
| Supply | monotonic bias and rails, no click | op-amp bias rises monotonically 50 → 200 %; a sweep under a sine stays solved |
| Switches (`circuit`, `diodes`, `opamp`, `hp` under 200 Hz) | below the bound | worst −40 dB re peak |
| Zipper (EXP 7-bit steps every 20 ms vs per-sample) | ≤ −80 dB | `gain` −91 dB, `tone_stack` −97 dB |
| Bass keep (120 Hz, full gain) | 60 Hz THD ≤ −40 dB, ±60 Hz sidebands ≤ −40 dB; no notch > 3 dB | THD −85 to −108 dB, sidebands −54 to −143 dB; within +0.34 / +0.01 dB of the unsplit path at gain 0 |
| Level (K) | unity ± 1 LU at the defaults | ±1 dB on a DI proxy (below) |
| Cleanup (`input` 0 / −6 / −12 dB) | THD falls monotonically | yes on three circuits; **the Blend OD's THD rises** as the input falls (−21.3 → −18.6 dB): its clean feeds scale with the input while the clipped path stays pinned. Reported, not asserted |
| Bench | — | 28 / 28 at 44.1 – 192 kHz; silence floor ≤ −155 dBFS |
| CPU, 48 kHz mono | ≤ 20 % of a core (≈ 4.2 µs) | 4.19 µs at the defaults on pink noise (8.31 µs / stereo frame); on a two-sine test: Op-Amp Dist 3.1, Booster Dist 4.0, Blend OD 3.8, MOSFET OD 4.1 µs |

**K(circuit)** (`level` 0 dB = unity at gain 50 %, tone 50 %), measured on a DI proxy (plucked notes
E2 – E4, −6 dBFS peak, RMS after a 100 Hz high-pass): golem's DIs are not in this repository, so the K are
to re-measure on them (±1 LU target).

| `circuit` | K (dB) | Hardware level pot fully up = `level` |
|---|---|---|
| Op-Amp Dist | −14.0 | +14.0 dB |
| Booster Dist | −3.6 | +3.6 dB |
| Blend OD | −27.0 | **+27.0 dB** (beyond `level`'s +24: its summing amp gives the clean feed +23 dB) |
| MOSFET OD | −16.3 | +16.3 dB |

**Linked mono** (after v0.1.0, from golem's first live test). REAPER hands every block 2-in / 2-out;
on a guitar lane the mono DI arrives as L == R, and two circuits (≈ 7 µs per sample each on the
maintainer's loaded machine, ≈ 14 – 17 µs per stereo frame) left too little of the 20.8 µs sample period
at 48 kHz: stutters live, offline renders fine. The rule, decided per sample (so the output does not
depend on the block size), as gfx-fuzz's:
- After `prepare` / `reset` a stereo instance is linked: channel 0's circuit runs alone and its output
  is copied to channel 1.
- Linked, the first sample whose L and R differ (bit comparison) unlinks: channel 1 takes a copy of
  channel 0's whole state, without allocating (every compiled circuit and twin by
  `NonlinearCircuit::copy_state_from`, both slots' oversamplers by gfx-dsp's `OversamplerN::copy_from`,
  `tight`, the bass-keep split, the DC guard), and runs on from there with its own input: exactly true
  stereo, with no step of any kind.
- Unlinked, after 60 ms of bit-identical L and R, channel 1's output fades linearly onto channel 0's in
  30 ms, then channel 1 stops; if L and R differ during the fade it runs back at the same rate. After a
  relink channel 1 plays channel 0's output, not what its own different past would have given: the only
  case where linked mono is not bit-exact with true stereo.
- Nulls against v0.1.0 (48 kHz, blocks of 192, 2.4 s of pink noise with a gain, a circuit, a `tight`, a
  bass-keep, a `diodes` and an `input` change on the way): mono, stereo L != R, stereo L == R, and L == R
  switching to L != R at 0.85 s: **bit-identical**. Tests: `linked_stereo_is_the_mono_output`,
  `link_switches_are_exact_and_click_free` (channel 1 bit-identical to true stereo up to the relink; HF
  burst at each transition −57 dB or less re peak, bound −30 dB; the unlink after the relink inside a
  circuit fade; no sample step above channel 0's own), `linked_mono_is_block_size_invariant` (blocks 1,
  64, 512, 4096, bit for bit). Bench 28 / 28.
- CPU (same loaded machine, unpinned, 48 kHz, pink noise, blocks of 64): mono 6.8 µs per sample; stereo
  L == R **13.6 → 6.8 µs per frame**; stereo L != R 13.8 – 14.2 µs (unchanged). The bench's line: 6,876 ns
  mono, 6,915 ns per stereo frame (its stereo feeds L == R).

**Deviations from the spec, and why.**

1. **Discrete OD not shipped** (enum of four). It needs a JFET device in `circuit_nl`, the transistor-level
   reference of its discrete op-amps to fit the square-law macro against (§2.3, §5), and the fixed Fender
   stack's values, which the Sapphire document gives but were not re-read here. The square-law macro and
   the output resistance are in `circuit_nl` already.
2. **Rates by the owner's policy**, not the README's fixed 16× / −85 dB: the Op-Amp Dist runs at 8× (its
   Ideal swap −57 dB at 8×, so that swap alone runs at 16×: a second compiled variant, crossfaded in like
   a circuit); the other three at 8×. One stage fewer per doubling of the host rate.
3. **"Ideal" op-amp = 50 V/µs, 20 MHz, rail to rail**, not infinite: a macro with no slew and no bandwidth
   is no macro, and 1,000 V/µs / 100 MHz left unsolved samples at 8×.
4. **Part swaps start the incoming slot from its DC point** (then the 20 ms crossfade), not from a copy of
   the running state: "None" → silicon on a copied state put 3.5 V across a silicon pair, a 60-iteration
   walk down the exponential that failed, and held currents ran away (output 10⁷). The DC start costs
   nothing audible inside the fade.
5. **The rail clamp, Newton fallbacks and stop rule** as described above (soft-plus clamp; previous-
   solution and plain-step tiers; a residual below 10⁻⁶ V also converges; the hold keeps the previous
   currents).
6. **Click bound −30 dB**, not −35: `tight` 1000 → 20 Hz moves the drive 3 dB and the clip's phase over
   its 30 ms glide, and the square-ish output's own HF differs by −32 dB re peak between the glide and the
   steady state (−59 dB for 1000 → 100 Hz). Every other step stays at or below −36 dB. `tight` leaves its
   20 Hz bypass by a 5 ms crossfade (the filter keeps running).
7. **Stage-cut check against a reference with 75 Ω and a current limit inside the loops: not run.** The
   cuts are exact against the monolithic solve (above); the LM308's output current limit was not added
   (no reference showed it reached).
8. **Small-signal test at 0.1 mV**, not 1 mV: 1 mV through 67 dB is 2.2 V, which clips.
9. **Blend OD sum and comb rows not coded**: in the linear regime the sum is superposition of one linear
   network (exact by construction); the −12 dBFS sweep for combs is left to the listening bench.
10. **Netlist values marked [U] in `circuits.rs`**: the Booster Dist's inter-stage 47 nF and the exact
    wiring of its tone arms; the Blend OD's gang A modelled as the leg rheostat only (its shunt of the
    stage's input at minimum gain is not modelled), its + input network (100 nF, 10 kΩ, 1 MΩ) and its
    clean network's topology; the 1N34A, 2N7000 body-diode and 2SC2240-class constants (generic). The maintainer:
    by ear, and from the documents when they are re-read.
11. **gfx-emet keeps its own 8× cascade** (the brief: never change a coded effect without a null).
    `OversamplerN` is the shared one from here on.
12. `hp` is a variable resistor at 1 GΩ when off; `opamp` is inert nowhere (the Discrete OD is absent).

**Left for the owner.** Listening against the NAM captures and BYOD / ChowCentaur (§5 Listening); the K
on golem's real DIs; the [U] values; whether the Blend OD's +27 dB "fully up" needs `level` to +30 dB;
the Discrete OD.

golem's **circuit-simulated distortion pedals**. Five classic pedal circuits, each entered as a netlist and
solved sample by sample as a nonlinear circuit: the op-amp hard clipper whose slow op-amp *is* the sound,
the transistor booster into an op-amp and diodes, the discrete-op-amp blues overdrive, the charge-pumped
clean-blend "transparent" overdrive, and the MOSFET overdrive. One `circuit` selector; each circuit's own
pots sit on three shared knobs (gain, tone, level) that mean "the pot's travel" on every circuit. Nothing
is fitted, nothing is a waveshaper with a filter around it: the knobs move resistors in the netlist, and
the circuit decides what happens.

It is a **new block**: proposed type **`dist`**, category **drive**, no catalogue ids to keep. Mono 1 / 1,
stereo 2 / 2 offered; reported latency 0 (minimum-phase 8× / 16× oversampling, a few samples of group delay,
the gfx-drive / gfx-emet precedent).

What it is not: gfx-drive (one parametric engine that morphs between families under an EXP pedal and
stands for the legacy `guitar-drive` ids), gfx-emet (the 808 family, done once and thoroughly — **the TS
circuit is deliberately not in this pedal**), gfx-fuzz (the transistor fuzzes, a separate pedal on the same
solver: owner's decision), NAM captures (one fixed setting of one unit). §6 positions it.

## 1. State of the art

Survey of 2026-11-14. **[V]** read or verified in the source during this survey · **[S]** read on a
published clone schematic or datasheet during the review (sources in §2.1; electrosmash.com resolved for
neither the survey nor the review, so none of this is read from it) · **[C]** circuit fact recalled from
a published analysis, to re-read when the netlist is entered · **[A]** calculation · **[P]** measured on
the review's prototype (§Review) · **[F]** forum or review opinion · **[U]** not verified.

### 1.1 The five circuits

**Op-amp hard clipper with a slow op-amp ("rat" family).**
- Structure [S: Aion Helios and GGG Rodent schematics, Madbean BOM, all agreeing]: input 22 nF, 1 MΩ to
  the 4.5 V bias, 1 kΩ series, 1 nF to ground; a non-inverting LM308 with 30 pF compensation (pins 1 –
  8); a 100 kΩ log distortion pot as a rheostat in the feedback with 100 pF across it; the gain leg is two
  RC branches in parallel (560 Ω + 4.7 µF, 47 Ω + 2.2 µF). Then 4.7 µF, **1 kΩ** into two 1N914 to ground
  (hard clip, ±0.65 V), the "filter": 100 kΩ log pot (rheostat) + 1.5 kΩ into 3.3 nF, a 2N5458 follower
  and a 100 kΩ log volume pot. 9 V single supply.
- Filter [A]: the 1 kΩ is in series with it, so the small-signal corner (diodes off) runs from
  **19.3 kHz** fully open to **470 Hz** fully closed. The often-quoted 32 kHz / 475 Hz leave the 1 kΩ
  out; 32 kHz is what the filter sees *while the diodes conduct* (they shunt the node). The corner is
  level-dependent: a detail only a solved circuit gets.
- Gain [A]: 1 + 100 kΩ / (560 ∥ 47 Ω) ≈ 2,300 ≈ **67 dB** at full distortion. The two branches set
  two low corners (60 Hz and 1.5 kHz): the bass is boosted far less than the mids at full gain.
- **The op-amp is the character**, in two ways:
  - *Gain-bandwidth* [S, A]: with 30 pF the LM308 crosses unity near 1 MHz (datasheet curve), so at full
    distortion the closed loop is only 1 MHz / 2,300 ≈ **430 Hz** wide. Every gain setting ends on the
    same high-frequency asymptote (the LTspice study [V]): the more gain, the darker, before anything
    clips. 30 pF is the unity-gain value: over-compensation at these gains, and the sound.
  - *Slew* [S]: ≈ 0.3 V/µs. This is **not a datasheet table value**: the follower pulse curve reads
    ≈ 0.29 V/µs falling and ≈ 0.2 V/µs rising. About 40 – 60× slower than a TL071. On 9 V the op-amp
    swings to within ≈ 1 V of its rails and is slew-limited and rail-clipped before the diodes see
    anything: square waves become trapezoids whose slopes do not depend on frequency.
  - Leete, Tarr & Ko, *Non-Ideal Operational Amplifier Emulation in Digital Model of Analog Distortion
    Effect Pedal* (AES 152nd Convention e-brief, 2022, e-library 21727) attribute the sound "largely" to
    the LM308's slew rate, gain-bandwidth and supply [V, abstract only].
  - The LM308 limits its output current near 5 mA [S, datasheet curve]. The diodes draw ≤ 3 mA through
    the 1 kΩ, but with the distortion pot near zero the output drives the 47 Ω leg directly: the reference
    run says whether that limit is ever reached (§5).
- Variants players know [F]: LED clippers (louder, more open), germanium (softer), "op-amp only" (diodes
  lifted), a TL071 / OP07 swap (OP07: 0.3 V/µs and 0.6 MHz [S, datasheet] — the same slew, a little
  darker; it needs no option of its own), a dying battery.

**Transistor booster → op-amp → diodes ("DS-1" family).** [S: Aion Comet v1.1 and legacy schematics]
- Input emitter follower (1 kΩ, 47 nF, 470 kΩ bias, 10 kΩ emitter). A common-emitter **booster**: 10 kΩ
  collector, 22 Ω emitter, 470 kΩ ∥ 250 pF collector-to-base, 100 kΩ base to ground; ≈ 35 dB, hot enough
  to clip softly on its own. Coupling 470 nF (68 nF after 1994) into 100 kΩ.
- Op-amp stage, non-inverting: the 100 kΩ **linear** distortion pot is a three-terminal divider (output →
  pot → 4.7 kΩ + 1 µF to ground, wiper to the − input), 250 pF from output to wiper. Gain =
  1 + D·100 k / ((1 − D)·100 k + 4.7 k): **0 to +26.9 dB** [A] (MusicRadar: "about 25 dB" [V]).
- Clipper: 2.2 kΩ, 470 nF, one antiparallel pair of small-signal silicon diodes (1S2473, then 1N4148),
  **10 nF across them** (2.2 kΩ · 10 nF: a 7.2 kHz low-pass that softens every edge).
- **Passive tone stack**: a low-pass arm (6.8 kΩ, 100 nF) and a high-pass arm (22 nF, 2.2 kΩ, 6.8 kΩ)
  crossfaded by a 20 kΩ linear pot; mid scoop near 500 Hz at noon. Level 100 kΩ linear; output follower.
- **Which op-amp** [S]: not a 4558 and not the M5218 (that is the Blues Driver's IC). 1978 – 1994: the
  TA7136P, a single-in-line preamp IC that is not a true op-amp; 1994 – 2016: BA728N, M5223AL, NJM2904 /
  NJM3404 — all **LM358-class** single-supply parts (slow, output swinging to ground but only to within
  ≈ 1.5 V of the supply [U, recalled]); a 4580 only after 2016. The netlist is the dual-op-amp topology
  with the pre-1994 values (what the clone documents do); its `Circuit` op-amp is the 358-class macro.
- Character [F]: thin and buzzy into a clean amp, "great into a cranked amp", the tone's scoop at noon.
  Boss itself describes the tone as "clockwise: more highs, fewer lows" [V].

**Discrete overdrive ("Blues Driver" family).** [S: Aion Sapphire schematic; V: Premier Guitar mods
article, PedalPCB breadboard thread]
- JFET input follower, then two **discrete op-amps** (a 2SK184 pair on a 4.7 kΩ tail, a PNP output on a
  2.2 kΩ load), running from ≈ 8 V. Their gains are set together by a **dual-gang 250 kΩ log** pot, each
  gang a rheostat in series with a fixed resistor:
  - stage 1: 22 kΩ + pot, 47 pF across; gain leg 1.5 kΩ + 150 nF (**707 Hz**: it cuts the bass before it
    clips); **+24 to +45 dB** above the corner [A];
  - a fixed Fender-type tone stack between the stages (treble at 0, bass and mid full);
  - **diodes: two in series each way, to ground, after stage 1 and that stack — before stage 2**, not
    after it;
  - stage 2: 33 kΩ + pot, 100 pF across; gain leg 2.2 kΩ + 1 µF (72 Hz); +24 to +42 dB [A]. **It has no
    clipper of its own: it clips against its rails**, softly and unevenly. That is the amp-like breakup.
- After stage 2: 5.6 kΩ ∥ 6.8 nF and 5.6 nF to ground (the "5 kHz" trim), the 10 kΩ linear tone pot
  between two 18 nF, the **level pot (100 kΩ log) — before the last stage**, then an IC op-amp (M5218)
  with 6.8 kΩ ∥ 2.2 nF feedback and a gyrator in its gain leg (56 nF, 1.2 kΩ, 470 kΩ: L ≈ 31.6 H,
  resonance **≈ 120 Hz** [A]). Its lift is "about 6 dB" per Wampler and ≈ 11.7 dB by an ideal-follower
  calculation: the netlist decides. An antiparallel pair across that op-amp's inputs is protection only.
- Character [V, F]: very touch-sensitive, cleans up with the guitar's volume, **known for too much
  treble** — "most players end up turning the tone knob down" (Thermionic Studios wiki).

**Charge-pumped clean-blend overdrive ("Klon-like", the transparent family).** [S: Aion Refractor
schematic; V: Coda Effects, ChowCentaur source]
- **Supplies**: the first dual op-amp (input buffer and gain stage) runs on plain **9 V**; only the
  second (summing and treble) runs on the charge pump's ≈ +18 / −9 V. So the gain stage has 9 V of
  swing and can rail-clip; the 27 V belong to the sum, which is why the clean feed never clips.
- Gain stage: non-inverting TL072 half, 422 kΩ ∥ 390 pF feedback, 15 kΩ ∥ 82 nF + 2 kΩ + gang A (100 kΩ
  linear) in the leg: **+13 to +28 dB** [A]. Gang A also shunts the stage's input to the bias: at
  minimum gain the stage receives nothing.
- Clipper: 1 µF, 1 kΩ, two germanium 1N34A **to ground** (≈ 0.35 V), no capacitor across, 1 µF, 47 kΩ
  into the summing node.
- **Three clean feeds** into the same node: gang B (the second 100 kΩ gang) fades a full-range network
  (27 kΩ; 12 kΩ + 27 nF; 22 kΩ + 2.2 nF to the diode side of the 47 kΩ) **down to nothing at maximum
  gain**; a fixed low-passed path (1.5 kΩ, 1 µF, 15 kΩ: ≈ 117 Hz) **stays at every setting** — clean
  bass under the clipped mids.
- Summing amp: inverting, 392 kΩ ∥ 820 pF. Treble: inverting unity stage with a 10 kΩ linear pot and
  3.9 nF (active shelving). Output pot 10 kΩ linear.
- Character [F]: "transparent" — the guitar's own tone with mids filled in; most loved as a clean boost
  into a pushed amp; low-gain grit "over a clean core". Its phase behaviour is the subtle part: the clean
  and clipped paths have different filters, and their sum is what the ear calls transparent.
- Open source [V]: ChowCentaur (Chowdhury; BSD-3) and Chowdhury's *A Comparison of Virtual Analog
  Modelling Techniques for Desktop and Embedded Implementations* (arXiv 2009.02833, 2020). Read in its
  source: **ideal op-amps plus static hard clips** (±4.5 V after the gain stage, −13.1 / +11.7 V after
  the sum), WDFs for the feed-forward networks and the diodes, 2× oversampling on the diode stage only,
  an optional RNN for the gain stage. No slew, no gain-bandwidth, no rail inside a loop.

**MOSFET overdrive ("OCD" family).** [S: PedalParts "Disorder" schematic and per-version BOM,
cross-checked with Madbean's and a Fractal forum thread]
- **No input buffer in v1.x** (a JFET stage only appears in v2): 22 nF, 10 kΩ, 470 kΩ to the bias; input
  impedance ≈ 325 kΩ.
- TL082, **two** non-inverting stages. Stage 1: drive pot (1 MΩ log; 500 kΩ on the first versions) +
  18 kΩ, 220 pF across; leg 2.2 kΩ + 100 nF (723 Hz): **+19 to +53 dB** [A]. At full drive the closed
  loop is only ≈ 6.5 kHz wide with a 3 MHz op-amp [A]: gain-bandwidth is audible here too.
- Clipper: 10 kΩ, then two 2N7000 back to back **to the 4.5 V bias**, each with its gate tied to its
  drain, 1 nF across. Wired so, the *other* device's body diode (≈ 0.6 V) conducts long before the
  ≈ 2 V threshold: the body diodes clip. An optional 1N34A in series with one device makes it ≈ 0.6 /
  0.9 V ("in very few units" per the BOM). Then 10 kΩ into stage 2: 150 kΩ ∥ 220 pF over 39 kΩ + 100 nF,
  **+13.7 dB — enough to reach its own rails** when the clip is 0.9 V.
- Output, passive and unbuffered: 1 µF, 33 kΩ, a shunt (47 nF + the 10 kΩ tone pot as a rheostat), the
  500 kΩ volume pot. **HP / LP switch: 22 kΩ in parallel with the 33 kΩ** (13.2 kΩ: louder, brighter).
- It derives from an earlier overdrive design [F]. The netlist is **v1.4** (the values above), the best
  documented; v1.5 – v2 have no reliable published values.
- Character [F]: "amp-like", more open and louder than a TS, from light crunch to classic rock; HP mode
  brighter and looser.

#### Added 2026-11-26 (roadmap 0.8, item 3)

Tags as above; **[V]** here = read on the named build document's schematic page during this survey
(rendered and read, not recalled); **[P]** = measured on this survey's prototype (`proto/nl.py`, §2.11).

**Discrete overdrive, re-read** (Aion FX *Sapphire* document v1.0.1, 2024-08-08, schematic p. 9 and parts
list [V]). The description above holds; what the re-read adds or corrects:
- **Discrete op-amp**: Q2 / Q3 (2SK184-GR; 2SK209-GR in the clone) share R5 4.7 kΩ to ground; the + input's
  drain has R6 2.2 kΩ to VA and drives the PNP's base (2SA1335-GR; 2N3906 in the clone), the − input's drain
  goes straight to VA; PNP emitter at VA, collector = output, 2.2 kΩ to ground; **47 pF (stage 1) / 100 pF
  (stage 2) base-to-collector** — the Miller compensation; another 47 / 100 pF across the feedback.
  Prototype DC point [P]: tail 0.93 mA, output at 4.11 V for VB = 4.05 V.
- **The "fixed tone stack" is a Fender TMB stack with its pots replaced by resistors** [V]: 100 kΩ slope
  resistor, 220 pF + 330 kΩ (treble), 100 nF (bass) and 1 MΩ (bass pot, full), 47 nF + 15 kΩ (mid); its
  output node carries the two series 1N914 pairs to ground and 2.2 nF into stage 2's gate (1 MΩ to VB).
- **Supply**: 9 V, 1N5817, a capacitance multiplier (2N5088-class, 2.2 kΩ, 47 µF): **VA ≈ 8.1 V** [A];
  VB = VA / 2 buffered by the first half of the 4558 / M5218.
- **The gyrator's lift is +5.8 dB at ≈ 120 Hz** [P] (the IC stage with its follower-gyrator solved, 4558
  macro): Wampler's "about 6 dB" is right, the ideal-follower 11.7 dB is not.
- **Stage 2 at transistor level** [P] (§2.11): rails ≈ 0.03 V and 8.07 V (the class-A PNP: soft towards
  ground, hard at the top); its loop gain is low (open-loop ≈ 56 dB [A]; at full gain the stage gives
  40.7 dB where the ideal is 42.3): **2nd harmonic −42 dBc at 10 mV in, full gain, long before any rail**.
  The even order is the tone; a symmetric op-amp macro cannot produce it.
- **The "Custom" variant (Keeley "Phat" mod)**: no maker's document; builders' threads on the PedalPCB
  forum agree on one change [F]: the **18 nF into the tone pot** (Sapphire C15, Cobalt Drive C14) becomes
  **33 nF, with 68 nF switched in parallel** (101 nF) — "Phat" on. A second report, C1 47 → 100 nF [F], is
  inaudible: that cap sees 1 MΩ (3.4 → 1.6 Hz) [A]. Measured on the prototype [P], across the tone pot
  (0 → 100 %): 101 nF lifts the output **+7.6 → +8.2 dB at 40 Hz, +4.5 → +5.2 dB at 120 Hz, +2.0 → +2.2 dB
  at 1 kHz** — ≈ 3 dB of bass over the mids, and a little more level. The same builders heard "little
  difference" [F]: it is a bass switch, not a new pedal.
- **Read at the review (Sapphire p. 9 [V])**, absent above: **D6 / D7, a 1N914 pair between IC1B's two
  inputs** (they conduct only when that op-amp leaves its linear range: a stage stamped linear does not
  need them, a stage given its macro does); the Sapphire's Bass pot (25 kΩ linear) is in **series** with the
  6.8 kΩ of IC1B's feedback (stock = 0 Ω); the level pot's wiper feeds 47 nF into IC1B's + input (470 kΩ to
  VB).

**Studio overdrive ("ODR" family).** [V: Aion FX *Andromeda* document v1.1.0 (2026-03-24), schematic p. 7
and parts list; it is the ODR-1 with a Bass pot added (stock = fully up = 0 Ω) and two optional series diodes
(stock = jumper bridged). F: Aion's text, PedalPCB forum (the "Nobleman", an exact replica).] Nordland's
own ODR-C document (Andromeda Deluxe) adds three controls around the same core; not modelled.
- Input: 33 kΩ into a 2N5457 source follower (1 MΩ gate to ground, 3.3 kΩ source); 68 nF, 2.7 kΩ, 22 nF to
  ground, 10 kΩ to VB at IC1A's + input: **a 184 Hz high-pass and a 3.4 kHz low-pass before any gain**, and
  2.1 dB of loss [A; P: −4.4 dB at 1 kHz, −6.3 dB at 184 Hz and at 3.4 kHz. The 22 nF sees 2.7 kΩ ∥ 10 kΩ, not
  2.7 kΩ alone (2.7 kHz, as first written)].
- **IC1A (JRC4558), soft clipper**: feedback 1.8 kΩ + 250 kΩ log gain pot, 120 pF and a 1N914 pair; two
  legs to ground, 1.5 kΩ + 2.2 µF (48 Hz) and **820 Ω + 82 nF (2.4 kHz)**: the gain is highest in the
  upper mids. Prototype [P]: in → IC1A out peaks at **2.8 kHz, 11 dB above 100 Hz**; full gain 45 dB at 2 kHz.
- **Hard clipper to the bias**: 2.2 µF, 12 kΩ, 2.7 nF to ground (4.9 kHz), a 1N914 pair to VB.
- Then a passive bass-forward network (39 kΩ; 82 nF into 1 nF ∥ 12 kΩ to ground; 10 kΩ; 43 kΩ to VB) into
  **IC2A, the Spectrum stage**: non-inverting, 20 kΩ ∥ 560 pF over 10 kΩ to VB; the **Spectrum pot (25 kΩ
  linear)** sits between the + side (22 nF ∥ 5.1 kΩ from the + input) and the − side (1.2 kΩ to the −
  input), its wiper into 27 nF to ground and a series branch 100 nF + an NPN gyrator (8.2 nF, 150 kΩ,
  2.2 kΩ, 3.3 kΩ: L ≈ 2.7 H, series resonance ≈ 306 Hz [A]). A Baxandall-like pan: wiper at the + side
  = dark, at the − side = bright. Prototype [P] (clip node → output): at 0 % the response falls from ≈ 700 Hz
  (5 kHz 15.7 dB under 1 kHz); at 100 % it rises to 3 kHz (+4.1 dB over 1 kHz; in → out, +5.6 dB). Aion's wording,
  "a 700 Hz low-pass and a 5 kHz high-pass mixed with a fixed 2.1 kHz boost" [V], is the same picture
  with the 2.4 kHz leg of IC1A as the "fixed boost".
- **IC2B, a bass shelf with gain**: 4.7 kΩ, 8.2 nF (4.1 kHz) into the + input; 22 kΩ ∥ 4.7 nF ∥
  (82 nF + 5.1 kΩ) over 1.2 kΩ + 1 µF: **+25.7 dB below ≈ 380 Hz, +13 dB above** [A]. **It rail-clips**:
  the 0.6 V of the hard clipper reaches 3.5 V at 54 Hz (Spectrum noon) and 4.5 V at 257 Hz (100 %) against
  ≈ 3.3 V of headroom [P]. The ODR's "enormous low end" (Aion [V]) is this stage, and it saturates on low
  notes.
- Level 50 kΩ log, a 4558 follower, 1 µF into 150 kΩ. Supply 9 V, 1N5817, 47 Ω, VB = 15 kΩ / 15 kΩ with
  47 µF (unbuffered): VA ≈ 8.65 V [A].
- Character [F]: "natural", amp-like, a secret weapon of Nashville sessions; the complaints are the bass
  (hence the Andromeda's Bass knob and Nordland's own Low Cut) and an unusual tone control.

**Marshall-in-a-box ("Plexi" family): which circuit.** The roadmap names Dawner Prince's Red Rox and the
Rockbox Boiling Point; the Review named the Guv'nor / Bluesbreaker. Searched:
- **Red Rox**: PedalPCB's *Ultisol Distortion* document (revised 8/19/20) gives the full schematic and
  parts list [V]; Dawner Prince's manual confirms the Contour pot and a 9 – 18 V supply [V, manual].
- Boiling Point: no published clone document found [U].
- Guv'nor: Madbean *Monocle* and General Guitar Gadgets documents exist [V: found, not read]; a Marshall
  product, op-amp + diodes + a three-band stack, nearer the Booster Dist than a Plexi amp.
- Bluesbreaker: an op-amp with diodes in its feedback, close to the Studio OD's first stage.

**Chosen: the Red Rox, from the Ultisol document**: it is one of the two references the roadmap names, it
has a documented schematic, and it is the one built as an amp model rather than a pedal: a cascade of
three op-amp gain stages with a bright cap, a hard asymmetric clipper and a bridged-T mid control. The
circuit [V]:
- Input 100 pF and 2.2 MΩ to ground, 10 nF into IC1.1's + (1 MΩ to VREF: 16 Hz).
- **IC1.1 (LM833)**, non-inverting: leg 3.3 kΩ + 47 nF (1.0 kHz), 100 pF across, feedback 4.7 kΩ + the
  **gain pot (100 kΩ reverse log, C)**: wiper on IC1.1's output, one end on the 4.7 kΩ, the other into
  68 nF + 4.7 kΩ to **IC2.1's virtual ground**, **1 nF across the pot** (a "bright cap", as across an amp's
  volume pot). One rotation raises IC1.1's gain (7.6 → 30.3 dB) and IC2.1's (16 → 43 dB) together.
- **IC2.1**, inverting: 680 kΩ ∥ 68 pF (3.4 kHz). Prototype [P], in → IC2.1 out: 67.6 dB at 1 – 1.5 kHz at
  full gain, 28 dB at 50 Hz (tight lows, like a Plexi's coupling caps); at gain 0 the bright cap tilts it
  15 → 30 dB from 50 Hz to 12 kHz.
- **Clipper**: 100 nF, 4.7 kΩ, to ground **germanium + 1N4001 one way, 1N4001 + 1N4148 the other**
  (≈ +0.9 / −1.2 V: asymmetric).
- **Tone**: 3 kΩ, then 47 nF + 100 kΩ log rheostat to ground (treble cut; 0 Ω = darkest).
- **IC2.2**, inverting, 47 kΩ in, 220 kΩ ∥ 470 pF (+13.4 dB, low-pass 1.54 kHz). **It rail-clips at 9 V**:
  the clipper's 1.2 V gives 5.1 V against 2.85 V of headroom [P]; at 18 V it does not (7.2 V).
- **Contour**: a bridged T, 10 kΩ + 10 kΩ with 6.8 nF across, 4.7 nF and 33 nF + the **Contour pot
  (50 kΩ log rheostat)** to ground at the middle, 4.7 nF at the end. Prototype [P], 1 kHz re 100 Hz:
  **−11.7 dB at 0 %** (dip at 1.0 kHz), −7.2 dB at 50 % (830 Hz), −2.5 dB at 100 %.
- IC1.2: unity inverting, 100 kΩ ∥ 220 pF; 1 kΩ, 4.7 µF into the master pot (10 kΩ log). Supply 9 V,
  1N5817, VREF 10 kΩ / 10 kΩ + 47 µF.
- Character [F]: a cranked plexi into a clean amp; the Contour from scooped to full mids; louder and
  cleaner at 18 V.

### 1.2 Methods

| Method | What it gives | For us |
|---|---|---|
| Yeh, Abel & Smith, *Simplified, Physically-Informed Models of Distortion and Overdrive Pedals*, DAFx 2007 [V, via gfx-drive] | Pre-filter → static clipper → post-filter; the RC across the diodes as an embedded low-pass | Good enough for gfx-drive; it cannot slew, cannot rail-clip inside a loop, cannot sum a clean feed with the right phase |
| Yeh, Abel & Smith, K-method (IEEE TASLP 2010); Yeh & Smith, DAFx 2008 (WDF vs state space for distortion circuits) [V refs, via gfx-drive] | Nonlinear state space from a circuit, nonlinearity solved implicitly, optionally tabulated | The family we use |
| Holters & Zölzer, **nodal DK-method**: *Physical Modelling of a Wah-Wah Effect Pedal as a Case Study for Application of the Nodal DK Method to Circuits with Variable Parts*, DAFx-11; *A Generalized Method for the Derivation of Non-Linear State-Space Models from Circuit Schematics*, EUSIPCO 2015 [V, read] | MNA → discrete state space x' = A x + B u + C i(v), v = D x + E u + F i(v). **A pot is a low-rank update**: the matrices are precomputed without it and corrected through the Woodbury identity, a k × k inverse for k pots (eq. 52 – 55); damped Newton from the previous sample | **Chosen**: it starts from the MNA stamping gfx-dsp `circuit` already does, and its pot update is cheap enough to run every sample (§2.2) |
| Holters & Zölzer, *A k-d Tree Based Solution Cache for the Non-linear Equation of Circuit Simulations*, EUSIPCO 2016 [V]; ACME.jl (Holters' Julia circuit framework) [U] | Speed: reuse previous solutions as Newton seeds | Optional speed-up if the CPU bound fails (§2.6) |
| Werner et al., WDFs with multiple / multiport nonlinearities, DAFx 2015; WDF modelling of circuits with op-amps, EUSIPCO 2016 [V refs, via gfx-drive; U] | Op-amps and several nonlinearities in WDF need R-type adaptors and an implicit multiport solve anyway | Rejected: no gain in simplicity for loops with op-amp macromodels; pots are cheap in DK at control rate too |
| Guitarix `dkbuilder` (GPL) [U] | SPICE netlist → DK model with tabulated nonlinearities; many pedal models shipped in Guitarix | Same idea, offline tables. We solve online (pots move continuously, supply moves) |
| Holters, *Antiderivative Antialiasing for Stateful Systems*, DAFx 2019; La Pastina et al., DAFx 2021 [V, via gfx-drive] | ADAA inside a state space | Not in v1: multi-port, implicit, stability limits. Oversampling alone is simpler but **not** enough at 8× on the sharpest circuit (§2.5); a one-port stateful ADAA on the diode stages is the research item if the rates of §2.5 cost too much |
| Damskägg, Juvela & Välimäki, SMC 2019; Wright et al. 2019 – 2020 [U] | Black-box RNNs of TS9 / DS-1 / Big Muff-class pedals at fixed or conditioned settings | NAM already covers the capture approach in golem; we want every pot position and the supply |
| Op-amp macromodels (Boyle family) [V, general] | Slew = the input stage's tail current charging the compensation capacitor | **Our op-amp model** (§2.3) |

### 1.3 Plugins and units

| Reference | Notes |
|---|---|
| Line 6 Helix / HX drive models | Individual models of these families (rat, klon, OCD types…) [U: model list not re-checked]. Criticised for fizz at high gain on some models [F, U] |
| Fractal Axe-Fx / FM Drive block | All five families as models of one block, with **generic expert controls (Clip Type, Bias, Slew Rate, supply)** [V for the controls, via gfx-drive] — the precedent for our `diodes`, `opamp`, `supply` |
| Neural DSP (Archetype pedal sections, Quad Cortex) | Klon-like and TS-like drives inside amp suites; captures of these pedals [U] |
| BYOD (Chowdhury, GPL-3) [V, via gfx-drive] | Mouse Drive (rat), Centaur and others as modules; ADAA waveshapers and WDF |
| ChowCentaur (BSD-3) [V, source read] | Ideal op-amps + static clips, 2× oversampling on the diode stage only (§1.1) |
| TSE (808, R47), Kuassa Efektor (distortion bundle), Mercuriall [U] | Single-pedal circuit-informed plugins; not inspected |
| NAM captures (in golem today, `pedal-nam`) | Exact at one setting; no pots, no supply, no input-level physics beyond the capture's training range |

### 1.4 What players love and hate [F]

- **Love**: the RAT filter doing "most of the work"; the DS-1 into a cranked amp; the BD-2's dynamics and
  cleanup; the Klon's clean boost; the OCD as an "amp in a box"; known mods (LEDs, germanium, battery).
- **Hate** in plugins: fizz and aliasing at high gain; a "static" feel — no cleanup with the guitar's
  volume, no difference between soft and hard picking; level jumps when switching models; the BD-2's
  treble; DS-1s thin into a clean amp.

### What we conclude

1. **Simulate the circuits**, op-amps included, rather than shape a curve: the RAT's gain-bandwidth,
   slew and rail clipping, the Klon's clean sum and the BD-2's rail-clipping discrete stage are all
   **inside feedback loops or summing nodes**, where a "filter → shaper → filter" model is wrong by
   construction.
2. **Nodal DK-method on gfx-dsp's MNA**, a globalised Newton per sample at 8× or 16× (per circuit), pots
   as variable parts updated every sample by a low-rank correction. Stages cut only where the cut is
   exact (small Newton systems).
3. **Op-amps as two-port macromodels** (tanh input stage charging the compensation capacitor — slew and
   gain-bandwidth from one law — and a rail limiter): the RAT's character, and the knob that swaps
   op-amps for free.
4. **Knobs are pot travel** with the circuit's own taper: noon is noon on every circuit; no defaults per
   circuit (contract).
5. **The clean blend is the circuit's own** sum, solved inside the oversampled domain: phase alignment
   is physical, not a compensation.
6. Skipped: the 808 (gfx-emet); **fuzzes (gfx-fuzz, a separate pedal: they need the pickup in the
   netlist and bias / temperature controls, which would be dead weight here)**; pickup / cable loading
   (gfx-fuzz owns it; these five circuits have buffered or ≥ 325 kΩ inputs); the true-bypass / buffer
   switching.
7. (2026-11-26) **Discrete op-amps are not op-amps**: their low loop gain and class-A PNP make even
   harmonics before any rail; they are solved as a square-law pair and a real PNP (§2.11). The Studio OD
   and the Plexi Dist each have an op-amp that **rail-clips after the clipper** (IC2B's bass shelf, IC2.2):
   those keep their macros, as §2.1's rule says.

## 2. Design

```
in ─▶ × input ─▶ tight (HPF) ─▶ ↑8/16 ─▶ [ circuit: nonlinear netlist solved per sample (DK + Newton) ] ─▶ ↓ ─▶ DC block ─▶ × level · K(circuit) ─▶ out
                                         └ bass_keep: LR4 split before ↑; low band through the circuit's linear output twin (§2.7) ─┘
```

- **Scale.** 0 dBFS = 1 V peak at the pedal's input (gfx-emet's convention). golem's DIs peak at −3 to
  −8 dBFS (0.4 – 0.7 V, real pickups). `input` trims a hotter or weaker source.
- **Mono 1 / 1** first; stereo 2 / 2 offered as two independent channels.

### 2.1 Netlists

Each circuit is a netlist in the source (component values, part models, pot tapers), taken from **one
published schematic, named in the source comment** (settled at the review):

| `circuit` | Source schematic | Version |
|---|---|---|
| Op-Amp Dist | Aion FX *Helios* documentation, checked against GGG *Rodent* | The LM308 / 1N914 original |
| Booster Dist | Aion FX *Comet* v1.1 documentation | Dual-op-amp topology, pre-1994 values (470 nF, 1 µF, 250 pF) |
| Discrete OD | Aion FX *Sapphire* documentation (v1.0.1) | Stock values (no Bass pot, 250 kΩ dual gang, the 1N914s); `fat` = 18 nF ∥ 83 nF (the "Phat" position, PedalPCB forum [F]) |
| Blend OD | Aion FX *Refractor* documentation | Original values (input bias 1 MΩ, 1N34A) |
| MOSFET OD | PedalParts *Disorder* documentation | v1.4: 1 MΩ log drive, 10 kΩ tone, 47 nF, no germanium |
| Studio OD (2026-11-26) | Aion FX *Andromeda* documentation (v1.1.0) | The stock ODR-1: Bass pot at 0 Ω, D6 / D7 jumpered |
| Plexi Dist (2026-11-26) | PedalPCB *Ultisol Distortion* documentation (rev. 8/19/20) | As drawn: LM833, master fully up |

The netlist contains everything between the input jack and the output, except: the bypass switching,
battery / adapter filtering, LEDs, and followers whose only role is impedance (modelled as ideal, with
their coupling capacitors and bias resistors kept). Where the output is passive (MOSFET OD) the next
device is a 1 MΩ load.

| `circuit` | Stages (§2.2), states ≈ | Nonlinear ports per stage | Rate at 48 kHz | Pots on the shared knobs |
|---|---|---|---|---|
| Op-Amp Dist | gain stage (5 C) → clipper + filter (2 C) | 2 (op-amp) · 1 (diode pair) | **16×** | `gain` = distortion 100 kΩ log; `tone_stack` = filter 100 kΩ log, **reversed** (100 % = fully open) |
| Booster Dist | booster (4 C) → op-amp stage (3 C) → clipper + tone stack (5 C) | 2 (BJT) · 2 (op-amp) · 1 (pair, 10 nF across) | 8× | `gain` = distortion 100 kΩ linear (three-terminal); `tone_stack` = tone 20 kΩ linear |
| Discrete OD | discrete stage 1 + fixed stack + diodes (7 C) → discrete stage 2 + trim + tone (7 C) → IC stage + gyrator (4 C) | **4 · 3 · 0** (pair port + PNP each, §2.11) | 8× | `gain` = the dual-gang 250 kΩ log (both gangs); `tone_stack` = tone 10 kΩ linear; `fat` = the 83 nF branch; the level pot fixed at 50 % |
| Studio OD | IC1A + feedback pair (4 C) → hard clipper + network + Spectrum stage, IC2A and the gyrator linear (9 C) → IC2B bass shelf + level + follower (6 C) | 3 · 1 · 2 | 8× (§2.11) | `gain` = 250 kΩ log; `tone_stack` = Spectrum 25 kΩ linear (0 % = wiper at the + side = darkest) |
| Plexi Dist | IC1.1 + gain pot + IC2.1 (5 C, one stage: the 1 nF across the pot couples them, §2.11) → clipper + tone + IC2.2 (5 C) → contour + IC1.2, linear (7 C) | 4 · 3 · 0 | 8× (§2.11) | `gain` = 100 kΩ reverse log (three-terminal, both stages); `tone_stack` = tone 100 kΩ log rheostat; `contour` = 50 kΩ log rheostat |
| Blend OD | gain stage (4 C) → clean feeds + clipper + summing amp (8 C) → treble (1 C, linear) | 2 (op-amp on 9 V) · 3 (Ge pair, summing op-amp) · 0 | 8× | `gain` = the dual gang (gain + clean fade, **both**); `tone_stack` = treble 10 kΩ linear |
| MOSFET OD | stage 1 (3 C) → clipper + stage 2 (4 C) → output network (2 C, linear) | 2 · 3 (1 more with the Ge) · 0 | 8× | `gain` = drive 1 MΩ log; `tone_stack` = tone 10 kΩ (0 % = darkest); `hp` = the 22 kΩ switch |

- **Op-amps that cannot leave their linear range** for an input ≤ +12 dBFS (the Blend OD's treble stage on
  27 V, input followers) are stamped linear (single pole or ideal): no ports. The reference run (§5)
  proves it per op-amp; one that can clip keeps its two ports.
- **`level`** is a gain after the circuit (§2.4); the level pot is in the netlist at a fixed position.
  - Level pot last (Op-Amp Dist, Booster Dist, Blend OD, MOSFET OD): fully up; its loading is then part
    of the response.
  - **Discrete OD: the level pot sits before the IC stage.** Fully up, that stage (+6 to +12 dB at
    120 Hz, 8 V supply) may clip on a rail-to-rail stage 2; the hardware at a normal level does not. The
    pot is fixed where the IC stage stays linear for any stage-2 output — **50 % of its rotation, noon**
    (review 2026-11-26; 90 % as first designed) — and the IC stage is then linear (0 ports). Stated in the
    source; the one departure from "fully up". The numbers [P, `bd2full.py level` and `head`]: the first
    estimate carried a 4 V-peak **sine** through the response (3.4 V asked fully up, 2.1 V at 90 %, against
    3.05 V of headroom). But stage 2 clipped is a **square** (0.04 – 8.08 V), and its waveform carried
    through the same linear response asks more: at 110 Hz, tone open, **5.5 V fully up and 3.2 V at 90 %**
    — it clips — and `fat` adds a third (7.1 and 4.3 V). At 70 %: 1.26 V, 1.67 V with `fat`. **At 50 %:
    0.48 V, 0.64 V with `fat`** (worst of 55 Hz – 1.2 kHz, tone 0 / 50 / 100 %): 2.4 V inside the rails, and
    still 0.7 V inside at `supply` 50 % (0.32 V asked of 1.0 V). The stage being linear, the position is
    only a scale: K (§2.4) absorbs it and the response does not move (the pot's 8 kΩ into 47 nF / 470 kΩ);
    noon is also where the hardware is played. D6 / D7 across IC1B's inputs (§1.1) never conduct in a
    linear stage and are left out.

**Tapers.** The `gain` and `tone_stack` percentages are **pot rotation**; each pot has its law in the
netlist: B = linear; A (log) = gfx-emet's `a_taper`, (10^(2t) − 1) / 99 (≈ 9 % of the resistance at half
travel), moved to gfx-dsp; C = its mirror. The laws above are the clone documents' parts lists [S], not
factory service notes.

### 2.2 The solver: nodal DK-method (gfx-dsp `circuit_nl`, shared with gfx-fuzz)

`circuit_nl` is **one module for every nonlinear circuit in golem-fx** (§2.10 lists what it must cover
for gfx-fuzz). It extends gfx-dsp `circuit` (linear MNA, trapezoidal companion models), following
Holters & Zölzer:

- **Stamping**: the same MNA as `circuit` (R, C, ideal op-amp, sources), plus nonlinear ports.
  Capacitors are discretised by the trapezoidal rule at the oversampled rate; their voltages are the
  states x.
- **System**: x[n+1] = A x + B u + C i, v = D x + E u + F i, y = Dₒ x + Eₒ u + Fₒ i, with i = f(v) the
  ports' currents (Shockley, Ebers–Moll, tanh…). u is a vector: a stage may have several inputs, and the
  supply is one of them.
- **Nonlinear equation** per sample: g(v) = p + F f(v) − v = 0 with p = D x + E u, solved by Newton in
  f64 with an analytic Jacobian. Plain Newton with a cap of 8 does not hold [P: the Op-Amp Dist's gain
  stage and clipper, 384 kHz, sines and squares from −12 to +12 dBFS]:
  - the mean is 2.4 – 2.9 iterations with the LM308 and up to 6 with a fast op-amp at full gain, but
    **1 – 2 % of the samples of an ordinary −12 dBFS sine at full gain need 9 – 17** (each entry into the
    diodes or a rail);
  - a fast op-amp (TL072: 34 V per sample of slew at 384 kHz) crosses from rail to rail inside one
    sample; undamped Newton then fails outright, twice per period.

  So the solve is specified as:
  1. seed: the previous solution extrapolated linearly;
  2. Newton with **backtracking** (step halved until ‖g‖² falls, Armijo) and device limiting (pnjlim on
     junctions and on the rail clamps; a tanh port moves by ≤ 3 V_lin per iteration); stop at
     |Δv| < 10⁻⁶ V (−120 dB re 1 V) or **20 iterations**;
  3. on failure, **continuation in p**: the solution is continuous in p, so the solve is repeated from
     the previous sample's (p, v) to the new p in 4 steps, then 16. On the prototype 4 steps rescued
     every case; the worst sample cost 43 iterations in all, on two samples per period;
  4. if that fails, the sample keeps the **previous solution** (not the last iterate, which may sit on
     the wrong rail) and a counter is incremented. The bench requires 0 (§5).

  The caps are fixed and the path depends on the data only: two renders are bit-identical.
  - Jacobian of size = the stage's port count (1 – 4): explicit inverse for 1 – 2, LU **with partial
    pivoting** for 3 – 4. An op-amp port is a controlled source: the stage is not passive and nothing
    guarantees a well-conditioned Jacobian.
  - Uniqueness: a diode-only stage is passive and monotone, one solution. An op-amp stage has one as
    long as its loop is negative at the algebraic level; the bench checks the sign of det J over the
    whole port range for every pot position.
- **Stiff modes.** The macromodel frees nodes an ideal op-amp pins: the Op-Amp Dist's 100 pF sees 43 Ω,
  a 4 ns time constant, which the trapezoidal rule turns into a mode ringing at half the oversampled
  rate with a factor −0.99 per sample [A]. Its excitation is small (≈ 1 mV at the inverting input [A])
  but the bench measures it (§5); if it shows, that capacitor alone is discretised by backward Euler.
- **Stage cuts.** A circuit is solved as a **directed graph** of stages (not a chain: the Blend OD's
  buffer feeds the gain stage and the summing stage). A cut is allowed only where it is exact:
  - (a) at the output of an op-amp whose macromodel output is an ideal voltage source. Feedback and
    load both hang on that source: neither changes the upstream solution;
  - (b) at a node whose downstream side draws no current: an op-amp's + input, a follower's gate. The
    bias resistors and coupling capacitors on that node belong to the upstream stage.
  - **Not at a transistor's collector** (the booster's 10 kΩ collector drives its load: the cut is at the
    op-amp's + input, rule b), **not at a discrete op-amp's output** (2.2 kΩ collector load: the fixed
    stack and the diodes it drives are in its stage), not inside the MOSFET OD's passive output.
  - **Output resistance is zero in the real-time model.** Stamping 50 – 100 Ω "downstream", as first
    written, moves the feedback take-off behind it and breaks the exactness it claims. The reference run
    has the resistance inside the loop; the error is what the Stage-cut check measures (§5). An output
    current limit (LM308 ≈ 5 mA) is added to a part only if the reference shows it reached.
  - The Op-Amp Dist becomes a 2-port stage and a 1-port stage instead of one 3-port system.
- **Pots** (`gain`, `tone_stack`): Holters & Zölzer's variable parts. The matrices are precomputed
  without the pots (S₀⁻¹); a pot value enters through (R_v + Q)⁻¹, a k × k inverse for k pots (k ≤ 2
  per stage: a dual gang, a three-terminal pot), and an O(n²) correction of the stage's matrices.
  - While a smoother moves this runs **every base-rate sample**, never otherwise. A rebuild every 16
    samples, as first written, is a 3 kHz staircase on the gain: the error is −50 to −55 dB re the signal
    during a sweep [P], against the contract's −80 dB. "The states are kept" prevents jumps, not
    steps.
  - (R_v + Q) stays invertible at both ends of the travel, so a pot at 0 Ω needs no special case.
  - `supply` is an input, not a matrix change. `hp`, `diodes`, `opamp` rebuild (off the hot path:
    prepared when the parameter changes, swapped at the crossfade of §2.8).
- **DC operating point.** Bias matters (the booster, the discrete op-amps, the pump rails). All five
  circuits' DC points are solved in `initialize` (capacitors open, Newton with source stepping) and kept
  as seeds. On `reset`, a `circuit` change or a part swap, the DC point is re-solved **inline** from its
  seed with a bounded Newton (≤ 50 iterations of a ≤ 20 × 20 LU, tens of µs [A], measured by the
  bench): no background task, no lock. Activation is silent: no thump, no settling.
- **Real-time**: matrices sized for the largest circuit at `initialize`; no allocation in `process`.

### 2.3 Op-amp and device models

**Op-amp macromodel** (two ports, per op-amp):
- Input stage: a transconductor i = I_t · tanh(v_d / V_lin) charging the compensation capacitor C_c,
  with a leak setting the open-loop DC gain A₀. Hence **slew rate SR = I_t / C_c** and **GBW = I_t /
  (2π V_lin C_c)**: one law gives both, and the output trapezoids of a slewing op-amp come out by
  themselves. Checked [P]: the prototype slews at 0.300 V/µs for SR = 0.3 V/µs.
  - For the LM308 this is physics, not a fit: 0.3 V/µs and 1 MHz give V_lin = 48 mV ≈ 2 V_T (52 mV), the
    law of an undegenerated bipolar pair. The slew-to-bandwidth ratio of the part is the model's.
  - A JFET-input part (TL072) has a square-law pair; the tanh with its V_lin is a stand-in, harmless
    because that part never slews audibly here. Its gain-bandwidth does matter (MOSFET OD: 6.5 kHz
    closed loop at full drive).
  - Limits: one pole only (no second pole, no overshoot), and a symmetric slew. The LM308's curve reads
    0.2 V/µs rising and 0.29 falling; the model carries I_t⁺ and I_t⁻ so the asymmetry can be entered
    once a measurement or a SPICE model confirms it [U].
- Output: an ideal buffer of the integrator voltage; the integrator is clamped at the rails by a
  diode-like conductance, so saturation recovery is physical — no wind-up. **The two rails are separate
  constants** (V_sat⁺, V_sat⁻): a 358-class part reaches ground but stops ≈ 1.5 V under the supply.
- Rails follow the supply input (bias = supply / 2), so `supply` moves headroom, bias and slew-to-rail
  ratios together.

| `opamp` part | SR | GBW | V_lin = SR / (2π GBW) [A] | Swing to the rails | Used by |
|---|---|---|---|---|---|
| LM308 (30 pF) | ≈ 0.3 V/µs [S: curve, not a table value] | ≈ 1 MHz [S, curve] | 48 mV | ≈ 1 V each [S, at ±15 V] | Op-Amp Dist |
| LM358 class | ≈ 0.3 – 0.5 V/µs [U] | ≈ 0.7 – 1 MHz [U] | ≈ 80 mV | 0 V / ≈ 1.5 V [U] | Booster Dist |
| 4558 | 1.7 V/µs (2.2 in the current datasheet) [S] | 3 MHz (4 in the current datasheet) [S] | 90 mV | ≈ 1 V [S] | (option) |
| TL072 / TL082 | 13 V/µs (20 in the current datasheet) [S] | 3 MHz (5.25) [S] | 0.69 V | ≈ 1.5 V [S] | Blend OD, MOSFET OD |
| Ideal | ∞ | ∞ | — | none | (reference) |

The Discrete OD's IC stage is linear (§2.1) and has no macromodel.

**Discrete op-amps** (Discrete OD) — **superseded 2026-11-26 by §2.11** (a pair port and the real PNP;
the macro below cannot make the stage's even harmonics): the same two-port structure, with the JFET pair's square-law transfer
(saturating at ±I_tail) in place of the tanh, a low A₀, an **asymmetric** class-A output limit **and an
output resistance** (the 2.2 kΩ collector load is not hidden by the loop once the stage clips, and
stage 2 clips on purpose). Its constants are fitted to a transistor-level netlist of the stage, solved
offline by the same engine at 64× (§5): the macro must match its harmonics within 1 dB. If the fit
fails, the transistor-level stage ships (4 – 6 ports, a higher CPU bound).

**Devices**: diodes by Shockley with series resistance (Is, n, Rs per part: 1N914 / 1N4148, 1N34A, red LED,
2N7000 body diode); diode pairs as one port (i = Is · (e^(v/nVt) − e^(−v/nVt)) for a matched pair, two
ports when asymmetric); the BJT by Ebers–Moll (2 ports); the MOSFET clipper as its body diode in
parallel with the diode-connected channel (square law above ≈ 2 V; it only conducts when the diodes are
lifted or swapped). One `exp` per port and per iteration serves both exponentials of a pair and the
tanh. Part constants from SPICE models in the source, each named [U until entered].

### 2.4 Shared knobs, no mode-dependent defaults

- `gain`, `tone_stack`: pot rotation, 0 – 100 %, on every circuit. **50 % is noon everywhere.** The
  defaults (50 %, 50 %, 0 dB) never depend on `circuit` (contract).
- `tone_stack` is oriented **0 % = darkest** on every circuit (gfx-emet's meaning). The Op-Amp Dist's
  filter pot is wired the other way, so the netlist reads it as rotation 100 % − `tone_stack`:
  "filter at 9 o'clock" on the hardware (20 % of the travel) is `tone_stack` 80 %. Kept (review): one
  meaning per id across circuits is worth more than the hardware's legend, the selector would otherwise
  flip the knob's sense, and the log pot reversed gives a usable law — corner 470 Hz at 0 %, ≈ 4.2 kHz
  at 50 %, 19.3 kHz at 100 % [A]. Presets write the converted value.
- **`level` 0 dB is unity loudness, on every circuit** (review; it was "the level pot fully up"). Fully
  up, these five pedals differ by some 20 dB [A, U until measured: the Blend OD is a strong booster, the
  Booster Dist loses ≈ 12 dB in its tone stack]: a selector that jumps by that much is the "level jumps when switching
  models" of §1.4. So each circuit carries a **fixed constant K(circuit)**, in the source and in the
  table below, such that at `gain` 50 %, `tone_stack` 50 %, `level` 0 dB the reference DI comes out at
  the bypass loudness ± 1 LU. It is the circuit's intrinsic voicing, not a default that follows a mode
  (contract): `level` has one default and one meaning. The table also gives "hardware level pot fully
  up" in dB of `level`, so the real relative loudness stays one number away.

  | `circuit` | K (dB) | Hardware fully up = `level` |
  |---|---|---|
  | (five rows) | measured on the bench | −K |

  `diodes`, `opamp` and `supply` are not compensated: an LED swap is louder, as on the hardware.
- Expert controls that follow the circuit start with a **`Circuit`** option (`diodes`, `opamp`) or are
  relative with 100 % = the circuit's own (`supply`).

### 2.5 Aliasing: oversampling, rate per circuit

- The whole circuit runs oversampled through cascaded gfx-dsp half-band stages, minimum phase
  (gfx-emet's 8× cascade moved into gfx-dsp as `oversample::Oversampler8x`, the name gfx-fuzz uses, plus
  a fourth stage for 16×). At 384 kHz the trapezoidal rule is accurate for every audible pole.
- **No ADAA** in v1: the nonlinearities sit inside implicit loops (§1.2).
- **8× does not reach −90 dB on a circuit with nothing across its diodes** [P: Op-Amp Dist netlist,
  −12 dBFS sine, full gain, filter fully open; worst alias in 20 Hz – 20 kHz re the fundamental, ideal
  decimator]:

  | Op-amp · clippers | 2 kHz: 8× / 16× / 32× | 5 kHz: 8× / 16× / 32× |
  |---|---|---|
  | LM308 · silicon (the stock circuit) | −77 / −97 / −138 dB | **−64** / −88 / −123 dB |
  | LM308 · LED | −89 / −131 / −142 | −81 / −112 / −149 |
  | LM308 · none | −91 / −110 / −139 | −81 / −101 / −130 |
  | TL072 · silicon | −70 / −80 / −101 | **−49** / −68 / −84 |
  | TL072 · LED | −74 / −91 / −117 | −51 / −82 / −96 |
  | 4558 · silicon | −71 / −81 / −102 | −48 / −66 / −83 |

  - The slow op-amp is its own anti-alias filter: with the LM308 each doubling buys 20 – 35 dB. The
    LED / None options are *cleaner* than the stock diodes, not worse: the 16× fallback first written
    for them was aimed at the wrong case.
  - A fast op-amp into bare diodes is a square wave: ≈ 18 dB per doubling, and −90 dB is out of reach
    below 32×.
- **Rates** (at 44.1 / 48 kHz; halved at 88.2 / 96 kHz, quartered above):
  - **Op-Amp Dist: 16×** (the cheapest circuit, 3 ports: affordable). Target **≤ −85 dB** at 5 kHz with
    its own op-amp.
  - The other four: **8×**. Each has its own low-pass at the clip (10 nF across the Booster Dist's
    diodes, 1 nF across the MOSFET OD's behind 10 kΩ, the 390 pF and 820 pF around the Blend OD's, the
    discrete stages' 47 / 100 pF) [A]; target ≤ −85 dB, measured per circuit, 16× for any that misses.
  - Op-Amp Dist with `opamp` 4558 / TL072 / Ideal: 16× gives −66 to −68 dB at 5 kHz. Stated bound
    **−65 dB**; 32× (−84 dB) if the CPU bound of §2.6 allows it. Other circuits under a swap: measured,
    same rule.
- The bench signal (a 5 kHz sine at full gain) is the worst case; on a guitar the same aliases sit
  under a dense harmonic spectrum.

### 2.6 CPU

The cost is the **transcendentals**, not the matrix products: per oversampled sample, one `exp` per port
and per Newton iteration (≈ 5 – 10 ns each in f64) against ≈ 100 multiply-adds for the linear part.
Counting an `exp` as one flop, as first written, was optimistic by about 2×.

| `circuit` | Ports | Rate | Estimate, mono at 48 kHz [A] |
|---|---|---|---|
| Op-Amp Dist | 3 | 16× | ≈ 1.2 µs / sample |
| Booster Dist | 5 | 8× | ≈ 1.2 µs |
| Blend OD | 5 | 8× | ≈ 1.2 µs |
| MOSFET OD | 5 – 6 | 8× | ≈ 1.4 µs |
| Discrete OD | 5 (macros) | 8× | ≈ 1.5 µs; ≈ 2.5 µs if the stages ship transistor-level. **2026-11-26: 7 ports (pair + PNP), ≈ 5.0 µs on the coded solver's measured scale (§2.11)** |
| Studio OD, Plexi Dist (2026-11-26) | 6, 7 | 4×, 8× | ≈ 2.3, ≈ 5.0 µs (§2.11) |

- **Bound: ≤ 2.0 µs / sample mono (≈ 10 % of a core), 2.5 µs for a transistor-level Discrete OD**; a
  `circuit` or part crossfade doubles it for 30 ms. gfx-emet measures 198 ns at 8× with a closed-form
  clipper: this pedal costs 6 – 10× that. Stereo doubles it.
- The worst samples (rail crossings) cost 10 – 40 iterations but are two per period: the bench reports
  the worst 32-sample block, not the worst sample.
- If the bound fails: linear stamps for the op-amps that never clip (§2.1), a fast `exp` checked by a
  null against `f64::exp` (gfx-fuzz's rule), the k-d tree / last-solution cache (Holters & Zölzer
  2016), then 1-port tabulation of the diode stages (v(p) is monotonic; a table per pot setting).

### 2.7 Bass keep and the clean blend

- **Blend OD**: its clean paths are in the netlist. The clean feeds, the clipped path and the summing amp
  are one stage, solved together in the oversampled domain: **the phase relationship is the circuit's**,
  nothing to align. This is the answer to "clean blend phase alignment": no parallel dry path exists to
  drift. It also keeps its bass clean by itself (the fixed 117 Hz path).
- **`bass_keep`** (shared id, all circuits): a Linkwitz-Riley 4th-order split before the oversampler; the
  high band enters the circuit; the low band goes, **at unity gain**, through the **linear twin** of the
  circuit's post-clipper network (the tone and output stages with the clippers removed, a linear
  `circuit` netlist at the same pot positions), through the oversampler's identity path of the same
  rate, then × K · `level` with the circuit's output.
  - The post-clipper filters are therefore matched on both paths; the pre-clipper ones are not, by
    design (they are what the bass is being kept from). The twin carries the circuit's small-signal
    **polarity** at the crossover (a constant per circuit; the Blend OD's two inversions cancel).
  - **Not the whole circuit linearized** (the fallback first proposed): its small-signal gain is the
    full gain — +67 dB on the Op-Amp Dist — and the "kept" bass would come out hundreds of volts hot.
    The low band's level is the input's: against a clipped high band at ±0.6 V that is the balance a
    bass player expects, and `tone_stack` still shapes both.
  - What cannot be had: phase coherence at every gain. The driven band's phase depends on level once it
    clips. The checks are therefore on what the control is for (§5): the low band stays clean under
    full drive, and the sum has no hole where the driven path is still linear.
- No global `blend`: gfx-drive has a parallel blend; here the transparent family *is* the blend.

### 2.8 Switching and smoothing

- `circuit` change: the new circuit starts from its DC operating point (solved inline, §2.2), then both
  circuits run for a **30 ms equal-power crossfade**, each through its own oversampler (the rates may
  differ); only the selected circuit runs otherwise.
- `diodes`, `opamp` changes: the part constants change in the netlist; 20 ms crossfade between the old
  and new solution (two solves during the fade).
- `gain`, `tone_stack`: smoothed (30 ms), applied as pot moves every sample (§2.2). `supply`: 100 ms, an
  input of the netlist. `level`, `input`: 20 ms.
- `hp`: a switch in the netlist, crossfaded 20 ms.

### 2.9 Latency

Reported 0. The minimum-phase half-bands add a few samples of group delay at low frequencies (gfx-emet
measured the same cascade; the 16× chain adds a fraction of a sample); stated, not reported, as gfx-drive
and gfx-emet do; bench `skip_latency`. The circuits add their own analog phase, which is the sound.

### 2.10 gfx-dsp reuse and what is new

| Reused | For |
|---|---|
| `circuit` (netlist language, MNA stamping, Laplace `Netlist::response`) | The DK derivation; the linear twin; analytic small-signal targets in tests |
| `oversample` (half-bands; gfx-emet's 8× cascade, moved to gfx-dsp) | 8× / 16× |
| `svf` (LR4 split; gfx-emet's, with the identity path) | `bass_keep` |
| `smooth` | Parameter smoothing |
| `exp_smooth` | EXP on `gain`, `tone_stack`, `level` |

| New | Content | Users |
|---|---|---|
| `circuit_nl` | See below | gfx-dist, **gfx-fuzz**, later gfx-wah's transistor stage, amp preamps |
| `oversample::Oversampler8x`, `Oversampler16x` | gfx-emet's 8× cascade moved here (emet switches to it, behaviour unchanged, bench-checked), and a fourth stage | gfx-emet, gfx-dist, gfx-fuzz |
| `pot` | Taper laws (`a_taper` from gfx-emet, linear, reverse, W) | gfx-emet, gfx-dist, gfx-fuzz |

**`circuit_nl` is one shared module** (review; gfx-fuzz is designed in parallel and needs the same
solver). Whichever pedal is coded first creates it; the other extends it; neither forks it. Both
documents use these names: module `circuit_nl`, type `NonlinearCircuit`, the netlist language stays
`circuit::Netlist` with new elements. It must cover, from its first version:

| Area | Needed by gfx-dist | Needed by gfx-fuzz as well |
|---|---|---|
| Elements | R, C, ideal op-amp, pots as variable parts (two- and three-terminal, ganged) | **L** (pickup), coupled inductors, sources with a series resistance (battery), current sources (leakage) |
| Inputs | Vector u: several signal inputs per stage, the supply as an input (so `supply` and sag never rebuild a matrix) | The pickup as a Thevenin source in the netlist; a sagging supply |
| Devices | Diode, diode pair, BJT (Ebers–Moll), MOSFET clipper, op-amp macromodel, discrete op-amp macro | BJT NPN / PNP with germanium leakage, junction capacitances and **temperature** (Is, V_T), every constant a run-time parameter |
| Solve | §2.2: backtracking Newton, limiting, continuation fallback, fixed caps, partial pivoting, the unsolved counter | **Monolithic stages up to ≈ 10 ports**: a fuzz's transistors sit in one feedback loop and cannot be cut |
| Stage graph | Cuts declared by the netlist's author, only under rules (a) / (b) | An approximate cut (a stage loaded by the next one's small-signal impedance) is a different, named mode, accepted only by the cut check |
| DC point | Source stepping at `initialize`, bounded inline re-solve | The same with leakage and temperature; re-biasing as an audible, slow effect |
| Pots | Woodbury update every sample while moving | The guitar's volume pot in the source network |
| Analysis | `NonlinearCircuit::linearized()` → a `Netlist` response at the operating point (small-signal targets); an offline reference mode (any rate, monolithic, tight tolerance) | `Netlist::to_spice()` for an ngspice cross-check |
| Real time | No allocation after `prepare`, f64, deterministic, flush-to-zero | Same |

### 2.11 Three more circuits (designed 2026-11-26)

Roadmap 0.8, item 3. **Prototype** (`proto/`, Python + numpy, run under the 3 GB memory cap):
`nl.py`, a small nonlinear MNA solver with the same element and device laws as `circuit_nl` (trapezoidal
caps, the op-amp macro with its soft-plus rail clamp, Shockley diodes, Ebers–Moll, a Shichman–Hodges JFET,
the pair port below; Newton with backtracking and continuation in the right-hand side; Jacobians by
differences: slower and cruder than `circuit_nl`, so its iteration counts are upper bounds); `bd2.py`
(the Discrete OD's stage 2 alone, three models), `bd2full.py`, `odr.py`, `plexi.py` (whole circuits:
`ac`, `alias <osf>`, and per-script extras). Numbers marked [P] in §1.1 and here come from them. Part
constants are generic [U] (2SK184-GR: β 18 mA/V², V_TO −0.45 V, λ 4·10⁻³; PNP Is 2·10⁻¹⁴, β_F 300;
4558 and LM833 from §2.3's law with their datasheet SR / GBW; germanium Is 2·10⁻⁷, n 1.3; 1N4001 Is
1.4·10⁻⁸, n 1.98).

#### The discrete op-amp: a pair port and the real PNP, not a macro

§2.3's plan (fit the square-law op-amp macro, with asymmetric rails and an output resistance, to a
transistor-level reference) fails on the prototype before any fitting: the stage's character is even
order **below** clipping, and a differential pair's law is odd. Stage 2 alone (Sapphire values, 1 kHz,
its 5.6 kΩ / 6.8 nF / 5.6 nF load) [P]:

| Model | Ports | gain 50 %, 10 / 50 / 200 mV in: h2 (dBc) | gain 100 %, 10 / 50 / 200 mV in: h2 / h3 (dBc) |
|---|---|---|---|
| Transistor level (2 JFETs, the 4.7 kΩ tail, the PNP) | 6 | −66.5 / −52.1 / −42.7 | −42.3 / −65.4 · −42.1 / −20.0 · −36.8 / −10.5 |
| **Pair port (square law, fixed 0.93 mA tail) + the PNP (Ebers–Moll)** | **3** | −66.9 / −52.5 / −42.9 | −42.7 / −66.0 · **−39.8** / −20.8 · −36.7 / −10.5 |
| **Pair port, tail following the common mode (I_t = v_s / 4.7 kΩ) + the PNP** | **3** | −66.5 / −52.0 / −42.8 | −42.3 / −65.4 · −42.1 / −20.0 · −36.8 / −10.5 |
| **The same, v_cm read from the previous sample (the form that ships; review, `bd2.py pairl`)** | **3** | −66.5 / −52.0 / −42.8 | −42.3 / −65.4 · −42.1 / −20.0 · −36.8 / −10.5 |
| Square-law op-amp macro (SR 9.3 V/µs, GBW 4.6 MHz, A₀ 640, rails 0.05 / 8.05 V) | 2 | −154 / −154 / −113 | −147 / −90 · −108 / −19 · −57 / −10 |

- Gains (h1) agree within 0.3 dB in every row; the swing (0.03 – 8.07 V) within 0.1 V.
- With a fixed tail the pair port is within 0.5 dB of the transistor level everywhere except gain 100 %
  at 50 mV (h2 2.3 dB high): the stage is non-inverting, so its + input carries the signal and the tail
  current follows it. **With the tail following the common mode** — I_t = v_s / R_t, v_s = v_cm −
  V_GS(I_t / 2), solved in closed form (a quadratic in √I_t; `proto/bd2.py pairt`) — harmonics 2 – 5 of
  every row match within 0.1 dB; 2 – 9 within 0.35 dB down to −60 dBc and 0.8 dB at worst (the 8th at
  −62 dBc, gain 100 %, 200 mV) [P, review]. That is the model.
- The tail makes the port's current depend on v(+) and v(−) separately, not only on their difference. To
  keep one port, v_cm is taken from the previous sample's solution (one oversampled sample of lag: 2.6 µs
  at 8×, on a bias that moves at audio rate). **Verified at the review** [P, `nl.py`'s `lag`, `bd2.py
  pairl`, `bd2full.py alias 8 pairl`]: the lagged form is the implicit one within **0.05 dB** on every
  harmonic ≥ −80 dBc at 8× (stage 2; gain 0 / 50 / 100 %; 10 mV – 1 V; 1 kHz and 4 kHz), within 0.43 dB
  at 4× (4 kHz, 1 V, gain 100 %); the whole circuit gives the same worst alias as the transistor level
  (−72.2 dB for −72.2 dB), the same Newton counts (3.33 mean, 19 worst), no unsolved sample. The loop
  through the lag is weak (the tail's share of the drain current is 0.5 / R_t = 0.11 mS against the
  pair's 2.9 mS): no instability seen at any level. **One port, settled**; the two-port form (v_cm as a
  sense port, +0.45 µs per stage) stays only as the bench row's fallback.
- Limits of the port [P, review]: it is the transistor level while both JFETs stay saturated and their
  gates reverse-biased — up to ≈ 3 V peak at stage 1's gate. A full-scale signal with `input` at +12 dB
  (4 V peak) drives the real + JFET's gate above its drain: the 2nd harmonic is then 2.1 dB off (−29.0 for
  −31.1 dBc), where the real Q1 follower would be clipping too (it is an ideal buffer here). Stated, not
  fixed. λ and the gate junctions are absent from the port by construction.
- So **each discrete op-amp = one pair port + one PNP (2 ports) = 3 ports**, against 2 for the macro and
  6 at transistor level. The output's asymmetry, its output resistance and its slew (I_tail / C_Miller
  ≈ 9 V/µs) come out of the PNP and the real 47 / 100 pF; nothing is fitted: β and V_TO are the JFET's,
  R_t is the 4.7 kΩ of the netlist.

#### `circuit_nl` additions (gfx-dsp; described here, not edited)

```rust
/// Shichman–Hodges JFET (SPICE level 1), drain / source symmetric (the higher of the two is the drain).
/// Ports (v_GS, v_GD) like the BJT's (v_BE, v_BC); currents (i_G, i_D).
pub struct JfetModel {
    pub n_channel: bool,
    pub vto: f64,     // pinch-off, V (negative for N)
    pub beta: f64,    // A / V²
    pub lambda: f64,  // 1 / V
    pub is: f64,      // gate junctions' saturation current, A; 0 = no gate current (exact cuts at a gate)
    pub n: f64,       // their emission coefficient
    pub vtotc: f64,   // V / °C   (ngspice VTOTC; 0 = SPICE's default)
    pub betatce: f64, // % / °C   (ngspice BETATCE; 0)
    pub tnom: f64,
    pub soft: f64,    // knee width at pinch-off, V: v_ov = soft · ln(1 + e^((v_GS − V_TO) / soft)); 0 = SPICE's hard knee
}
DeviceModel::Jfet(JfetModel);  Netlist::jfet(d, g, s, model) -> usize;   // 2 ports

/// A JFET differential pair on a tail, as one transconductor port (the op-amp macro's input port,
/// with the exact pair law and its own output node): i(drain → 0) = I_t / 2 + Δ / 2,
/// Δ = β v_d √(2 I_t / β − v_d²) for |v_d| < √(I_t / β), ±I_t beyond; v_d = v(+) − v(−); no gate current.
/// tail_r > 0: I_t follows the common mode, I_t = v_s / tail_r with v_s = v_cm − (V_TO + √(I_t / 2β)),
/// v_cm from the previous sample (one port); i_tail is then only the DC seed. I_t is a constant during
/// a sample's Newton solve; it is latched from the node voltages after each solved sample.
pub struct PairModel { pub beta: f64, pub i_tail: f64, pub tail_r: f64 /* 0 = fixed tail */, pub vto: f64 }
DeviceModel::Pair(PairModel);  Netlist::jfet_pair(plus, minus, drain, model) -> usize;   // 1 port
```
- The latched v_cm is state (review): the **DC point is solved with the tail implicit** — a fixed point,
  I_t from v_cm, re-solve, until v_cm moves < 10⁻⁹ V, seeded by `i_tail` (source stepping alone cannot
  start a tail that depends on a common mode near 0 V [P]); `reset` and the slot start re-latch it (a
  moving `supply` needs nothing: the latch follows the bias); `copy_state_from` copies it (linked mono);
  a held (unsolved) sample keeps the old latch. The offline reference solves the transistor level, so
  the lag is measured by the bench row, not assumed.
- Real time: the pair port (no `exp`; C¹ at saturation) and the existing BJT. The JFET device serves the
  **transistor-level reference** (offline mode, monolithic, 64×) and `to_spice()` (`J` cards and an `NJF`
  / `PJF` `.model` with VTO, BETA, LAMBDA, IS, N, VTOTC, BETATCE; `soft` > 0 is written as a B-source
  instead, flagged in the deck), and gfx-fuzz / later preamps. Newton limiting: `pnjlim` on both gate
  junctions; the channel needs none (square law). Temperature: Is by the `Junction` law, V_TO and β by
  VTOTC / BETATCE (ngspice's level-1 laws).
- `soft` exists because SPICE's knee has a zero Jacobian below pinch-off; a stage whose JFET can be cut
  off (a fuzz, a JFET booster) otherwise relies on the line search. 0 for every fixture.
- Fixtures: `fixtures/discrete-od.cir` (the transistor-level deck, both discrete stages with their JFETs),
  `studio-od.cir`, `plexi-dist.cir`, and the reference lines in `reference.txt`, as for the four.

#### Netlists, stages, DC points

Each from the document of §2.1; every value in §1.1. Inputs as for the four (u₀ signal, u₁ supply, u₂
bias). Followers whose only role is impedance are ideal (Discrete OD: Q1, Q9 and the VB buffer; Studio OD:
Q1, IC1B; Plexi Dist: none).

| `circuit` | Stage cuts (§2.2 rules) | Linear stamps (proved by [P]) | DC point [P] |
|---|---|---|---|
| Discrete OD | stage 1 (pair, PNP Q4, the fixed stack, the 2 × 2 diodes) → **cut at Q5's gate** (rule b: the pair port draws no gate current) → stage 2 (pair, PNP Q7, trim, tone, the level pot at 50 %) → cut at IC1B's + input (rule b) → IC stage, gyrator, output | IC1B (≤ 0.64 V peak with the level pot at 50 %, §2.1); Q8 the gyrator's follower (0.3 mA, signal ≪ bias) | VA 8.1 V, VB 4.05; tails 0.93 mA; outputs 4.11 V; PNP bases 7.45 V; Q8 base / emitter 3.69 / 3.08 V |
| Studio OD | IC1A with its feedback pair → cut at IC1A's output (rule a) → hard clipper, network, IC2A → cut at IC2A's output (rule a, ideal output of a linear stamp) → IC2B, level, follower | IC2A (≤ 1.0 V peak for the clipper's 0.6 V at any Spectrum: +4.4 dB at most, at 5.9 kHz); Q2 the gyrator's follower (1.0 mA) | VA 8.65 V, VB 4.33; every op-amp output 4.33 V; Q2 base / emitter 3.95 / 3.31 V |
| Plexi Dist | IC1.1, the gain pot, IC2.1 (one stage: see below) → cut at IC2.1's output (rule a) → clipper, tone, IC2.2 → cut at IC2.2's output → contour, IC1.2, master | IC1.2 (unity, after the contour's loss) | VA 8.7 V, VREF 4.35; every op-amp output 4.35 V; the clipper at 0 V (coupled) |

- The Plexi's gain pot is one three-terminal pot whose two halves sit in two loops (IC1.1's feedback,
  IC2.1's input). A cut at IC1.1's output would look exact by rule (a), but the 1 nF across the pot joins
  IC1.1's feedback node to IC2.1's input network: downstream current flows into the upstream loop. **Not
  a cut**: IC1.1 and IC2.1 are one stage (4 ports), cut at IC2.1's output. Ports 4 · 3 · 0 = 7.
- Op-amps that keep their macro: Studio OD IC1A (a feedback clipper: the op-amp's GBW and the diodes
  share the loop) and IC2B (rail-clips on low notes: 3.5 – 4.5 V asked of ≈ 3.3 V); Plexi IC1.1 (30 dB on
  a −12 dBFS input = 7.9 V asked), IC2.1, IC2.2 (5.1 V asked of 2.85 V at 9 V).
- `supply` (relative, 100 % = 9 V at the jack): the Discrete OD's VA follows through its regulator
  (VA ≈ 0.9 · V_supply after the Schottky and the multiplier's V_BE, an input of the netlist, as the
  pumped rails of the Blend OD); the Studio OD's and the Plexi's VA = V_supply − 0.35 V. **200 % on the
  Plexi Dist would be the 18 V its maker's manual allows** [V]: IC2.2 stops clipping (7.2 V of headroom).
  **Decided by Raph, 2026-11-26 (open question 11): the Plexi Dist's supply stops at 150 %** (13.5 V,
  −65.9 dB of alias): above it the circuit holds 150 %; the id's range stays 50 – 200 % for the others.

#### Aliasing and rates

Worst alias of a 1.2 kHz sine (1,205 Hz: an exact window whose aliases miss the harmonics), −12 dBFS,
full gain, tone fully open, own parts, ideal decimator (a Blackman–Harris window over 0.2 s, 50 Hz – 20 kHz,
harmonics ± 30 Hz excluded) [P]:

| `circuit` | 4× | 8× | Rate chosen | Newton per sample at 8× (prototype: mean / worst) |
|---|---|---|---|---|
| Discrete OD | −60.7 dB | **−72.2 dB** | 8× | 3.3 / 19 (whole circuit, monolithic) |
| Studio OD | **−77.0 dB** | −104.4 dB | **4×** | 2.2 / 6 (4×: 2.8 / 9) |
| Plexi Dist (`contour` 50 %) | −49.7 dB | **−71.3 dB** | 8× | 3.5 / 33 |

Re-run and extended at the review [P]; the table's Discrete OD row is now the model that ships (the first
figures, −60.3 / −70.6 dB, were measured with the **fixed** tail and the level pot at 50 %):

| Case (8× unless said) | Worst alias |
|---|---|
| Discrete OD: transistor level · pair port, implicit tail · **lagged tail** · fixed tail | −72.2 · −72.4 · **−72.2** · −70.6 dB |
| Discrete OD, `fat` on · the level pot fully up | −72.4 · −72.1 dB |
| Discrete OD, `supply` 50 % · 200 % (transistor level) | −71.5 · −71.3 dB |
| Studio OD at 4×, Spectrum 100 % · 0 % | −77.0 · −81.2 dB |
| Plexi Dist, `contour` 100 % · 50 % · **0 %** | −71.7 · −71.3 · **−66.6 dB** |
| Plexi Dist, `supply` 150 % · **200 % (18 V)** · 200 % at `gain` 70 % · 200 % at 16× | −65.9 · **−60.5** · −62.4 · −80.1 dB |
| Plexi Dist, `gain` 70 % (9 V) | −74.3 dB |

- **The Discrete OD passes by 2.2 dB**, not 0.6, and identically at transistor level: the bench measures
  it on the coded solver with the real decimator.
- **The Plexi Dist passes only with its Contour at noon or above.** Its worst alias sits at 395 Hz (the
  319th harmonic folding around 384 kHz: IC1.1 and IC2.1 rail-clip a 68 dB gain into near-squares, and
  the alias falls 21 dB per doubling of the rate), below the fundamental; the bridged T scoops the
  fundamental under it: 1,205 Hz re 395 Hz is −0.9 dB at `contour` 70 %, −2.2 dB at 50 %, **−6.9 dB at
  0 %**. So **−66.6 dB with the scoop fully in**: inside the −65 dB bound of a swap, outside the −70 dB
  of "own parts". And **at 18 V it is −60.5 dB**: IC2.2 no longer clips (it was re-squaring the wave and
  capturing the alias with it), and the rails of the first two stages are twice as far apart. 16× gives
  −80.1 dB there, at twice the cost (≈ 10 µs, ≈ 48 % of a core: stutters on the maintainer's loaded
  machine, §As coded). **Not settled by the review** — open question 11: the policy's reading for a
  post-clipper scoop, and what the Plexi's `supply` above ≈ 140 % does (accept −60 dB on an expert
  setting, run 16× there, or stop the Plexi's supply at 150 %). **Decided by Raph, 2026-11-26:** the
  scoop is a setting, under the −65 dB bound (−66.6 dB passes); the Plexi's `supply` stops at 150 %
  (−65.9 dB).
- A swap (`opamp` on the Plexi, `diodes` on both) that misses −65 dB runs its variant at 16×, the
  Op-Amp Dist's Ideal precedent. The Studio OD's feedback clipper and its 2.7 nF / 4.9 kHz pole before
  the hard pair are their own anti-alias filter: it is the first circuit at 4× (`Circuit.stages` 2), with
  4 dB of margin at either end of the Spectrum.
- `supply` was not in the alias bench of the four coded circuits either: the bench row (§5) now measures
  every circuit at 50 % and 200 %.
- The prototype solves each circuit monolithically (no cuts, Jacobians by differences); its counts are
  upper bounds: `circuit_nl`'s stage cuts and analytic Jacobians give 0.7 – 1.0 per stage-sample on the
  four.

#### CPU (estimate)

Measured on the four (§As coded): 3.1 µs at 3 ports, 4.0 – 4.1 µs at 5 – 6 ports, all at 8× — about
1.75 µs plus 0.45 µs per port [A]. Hence, mono at 48 kHz:

| `circuit` | Ports | Rate | Estimate |
|---|---|---|---|
| Discrete OD | 7 (4 · 3) | 8× | ≈ 5.0 µs |
| Studio OD | 6 (3 · 1 · 2) | 4× | ≈ 2.3 µs |
| Plexi Dist | 7 (4 · 3) | 8× | ≈ 5.0 µs |

The contract's bound is ≈ 20 % of a core (4.2 µs); two circuits would exceed it by ≈ 20 %, for a pedal
used once per lane, with linked mono already halving the stereo case. 4× is not the way out (−61 / −50 dB
against the −70 dB policy). The estimate is a straight line through three points of the four coded
circuits; it knows nothing of the pair port (no `exp`: cheaper than a junction port) nor of iteration
counts (the prototype's means are 3.3 – 3.5 for these two). Review's reading: **4.5 – 5 µs, to measure
before anything is decided**; the bound that matters live is the maintainer's loaded machine (4.2 µs
here was 6.8 µs there, of a 20.8 µs sample period). In order, if the bench confirms the estimate: (1) `circuit_nl`'s last-solution
cache (§2.6) on the pair / PNP stages; (2) the 4558 / LM833 stages that the bench proves linear at
−6 dBFS stamped linear (the Plexi's IC1.1 is the candidate below full gain: 30 dB on a −12 dBFS input
clips, on a −20 dBFS input it does not); (3) otherwise the owner accepts ≈ 24 % for these two circuits.
**Decided by Raph, 2026-11-26:** ≈ 24 % accepted for the Discrete OD and the Plexi Dist if the coded
measurement is ≤ 5 µs per sample.

#### Switching, compatibility

- `circuit` appends **Discrete OD · Studio OD · Plexi Dist** after MOSFET OD (ids `discrete-od`,
  `studio-od`, `plexi-dist`). The four coded circuits' netlists, constants and rates do not change:
  **renders of the four are bit-identical** before / after (a test renders every preset of
  `presets/gfx-dist.json` and the 16 reference cases, strict null). Presets store options by name; the
  plugin's state stores the variant id: both safe.
- **REAPER normalizes an enum as index / (n − 1)** (contract): a host automation lane or a golem
  value written as a normalized number would move (MOSFET OD = 3 / 3 = 1.0 now, 3 / 6 = 0.5 after). golem
  writes from its catalogue's option list: **the catalogue entry gains the three options in the same
  release** (golem-rack). Old REAPER projects with automation on `circuit`: not migrated (stated).
  **What goes wrong if the two are out of step** (review; the gfx-grit design met the same): with the
  new plugin and a four-option catalogue, golem on REAPER writes k / 3 and the plugin reads round(6 k / 3)
  = 2 k — Booster Dist plays the **Blend OD**, Blend OD the **Discrete OD**, MOSFET OD the **Plexi
  Dist**; only Op-Amp Dist survives. With the old plugin and a seven-option catalogue, k / 6 lands on
  round(k / 2). Every existing golem chain with a Dist on REAPER is hit; the Golem Engine is not (plain
  values through the controller), nor are a saved state (variant ids `opamp-dist` … `mosfet-od`) or a
  preset (option names). So this is a **release gate, not a note**: the VST3 with seven options and the
  catalogue with seven options ship together, and golem-rack's `check-engine-blocks.py` (which reads each
  parameter back) asserts on REAPER that every `circuit` option of the catalogue reads back under its own
  name — it fails on either mismatch. The same holds for the new ids: `fat` and `contour` are appended
  **after** `bass_keep`, so no existing parameter changes its index (REAPER envelopes address a VST3
  parameter by index).
- Crossfades as §2.8: a `circuit` change, 30 ms, the incoming circuit from its DC point (the inline DC
  solve has more ports here: bound unchanged, 133 µs, measured); `fat` and `contour`: below.
- `fat` is a variable resistor in series with the 83 nF (**22 MΩ off**, 10 Ω on), as `hp`: the 83 nF is
  charged to the 18 nF's DC voltage at the DC point, so the switch moves the resistor through Woodbury
  with a 20 ms exponential glide in log(R) and needs no crossfade; checked against the click bound.
  Review: 22 MΩ, not the 1 GΩ first written — the 18 nF holds stage 2's bias (4.1 V), and behind 1 GΩ
  the 83 nF would follow a moved bias (`supply`) with an 83 s time constant, then thump by volts when
  switched in; behind 22 MΩ it follows in 1.8 s, as behind a real switch's pull-down, and the off
  position is the stock circuit within 0.1 dB at 20 Hz [A]. gmin (10⁻¹² S) on the branch's middle node is
  then 2·10⁻⁵ of the divider: no step.
- `contour` is a pot (rheostat, 50 kΩ log) on the Woodbury path, smoothed 30 ms like `tone_stack`.
- `bass_keep` twins (§2.7): Discrete OD from stage 2's output (trim, tone, level at 50 %, IC stage with the
  gyrator, output: a twin from the diode node would carry stage 2's +24 – 42 dB), polarity +1; Studio OD
  from the hard clipper's node (network, Spectrum stage, IC2B's shelf), polarity +1; Plexi Dist from the
  clipper's node (tone, IC2.2, contour, IC1.2), polarity −1 (IC2.1 inverts before the clipper).
- K(circuit): measured on the bench as for the four; "hardware fully up" for the Discrete OD is the
  level pot at 100 % against the netlist's fixed 50 % (**+20.7 dB** more [P]; the IC stage clips long
  before, §2.1).

## 3. Parameters

New block: no catalogue ids. Shared ids keep their contract meaning (`tight`, `bass_keep`). As in
gfx-emet: `gain` is the main gain (not `drive`, reserved for "extra saturation, neutral at 0"), and
`tone_stack` is the circuit's tone pot (not `tone`, gfx-drive's tilt). `circuit` is not `model` (a voicing
of one engine) nor `mode` (a behaviour): it swaps the netlist. Ranges linear (contract); knob feel is
golem's UI's business.

### Front panel

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `circuit` | Circuit | e | Op-Amp Dist · Booster Dist · Blend OD · MOSFET OD (coded order) · **Discrete OD · Studio OD · Plexi Dist** (appended 2026-11-26) | Op-Amp Dist | 30 ms crossfade | | Which netlist is solved (§2.1). Enum grows at the end only (other op-amp / diode drives; fuzzes live in gfx-fuzz) |
| `gain` | Gain | f | 0 – 100 % | 50 % | 30 ms | ✓ | The circuit's gain pot, by rotation, with its real taper (Blend OD: both gangs, so the clean fades as the gain rises) |
| `tone_stack` | Tone | f | 0 – 100 % | 50 % | 30 ms | ✓ | The circuit's tone pot by rotation, 0 % = darkest on every circuit (the Op-Amp Dist's filter is read reversed: 100 % = fully open, §2.4). Studio OD: the **Spectrum** pot (0 % = wiper at the + side: dark, 700 Hz roll-off; 100 % = bright, upper-mid lift); Plexi Dist: its treble-cut rheostat |
| `level` | Level | f | −24 – +24 dB | 0 dB | 20 ms | ✓ | Output; 0 dB = unity loudness at the defaults on every circuit (§2.4, which also gives "hardware pot fully up" per circuit). +24 dB so the boosters reach their real maximum |

### Expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `diodes` | Clippers | e | Circuit · Silicon · Germanium · LED · MOSFET · MOSFET + Ge · None | Circuit | 20 ms crossfade | | Swaps the clipping devices in place (the classic mods). MOSFET + Ge = the asymmetric pair of some MOSFET OD units. None = lifted: the op-amp's rails (or the next stage) clip. On the Discrete OD, the series pairs keep their count. Studio OD: both pairs (IC1A's feedback and the hard pair to the bias). Plexi Dist: each side keeps two junctions of the chosen part (Circuit = Ge + 1N4001 / 1N4001 + 1N4148, asymmetric) |
| `opamp` | Op-Amp | e | Circuit · LM308 · LM358 · 4558 · TL072 · Ideal | Circuit | 20 ms crossfade | | The macromodel of the circuit's gain-stage op-amp(s) (§2.3); linear stamps and followers are not touched. **Inert on the Discrete OD** (its stages are discrete and its IC stage is linear). Studio OD: IC1A and IC2B (4558); Plexi Dist: IC1.1, IC2.1, IC2.2 (Circuit = LM833: 7 V/µs, 15 MHz, swing 1.5 V from each rail [S, datasheet; U on 9 V]) |
| `supply` | Supply | f | 50 – 200 % | 100 % | 100 ms | | Relative to the circuit's own supply (9 V; the Blend OD's pumped rails follow; the Discrete OD's 8.1 V through its regulator): headroom, bias, rails. 78 % ≈ a tired 7 V battery; 200 % = the 18 V mod. **Plexi Dist: held at 150 % above it** (decided 2026-11-26, open question 11: −65.9 dB of alias at 150 %, −60.5 dB at 200 %) |
| `hp` | HP Mode | b | off / on | off | 20 ms crossfade | | MOSFET OD's HP / LP switch (22 kΩ across the 33 kΩ: louder, brighter); inert on the other circuits |
| `fat` | Fat | b | off / on | off | 20 ms glide (a resistor, §2.11) | | **Discrete OD's bass switch** (the "Custom" variant: 83 nF switched across the 18 nF into the tone pot = the 101 nF "Phat" position): +5 dB at 120 Hz, +8 dB at 40 Hz, +2 dB at 1 kHz [P, §1.1]; inert on the other circuits |
| `contour` | Contour | f | 0 – 100 % | **70 %** | 30 ms | ✓ | **Plexi Dist's Contour pot** (50 kΩ log rheostat in its bridged T), by rotation: 0 % = deepest mid scoop (−11.7 dB at 1 kHz re 100 Hz), 50 % −7.2 dB, 70 % −4.6 dB, 100 % nearly flat (−2.5 dB) [P]; inert on the other circuits. Default 70 %, not noon (review): the log law puts noon at 9 % of the track, a half scoop, and a "Plexi" is a mid-forward voice; 70 % is also where the alias margin is widest (§2.11). Its whole travel passes the alias policy as a setting (−65 dB; decided 2026-11-26). To confirm by ear before K is measured — a default is fixed once shipped |
| `tight` | Tight | f | 20 – 1,000 Hz | 20 Hz (off) | 30 ms | | Shared id: first-order high-pass at the circuit's input, on top of its own filters; bypassed at 20 Hz |
| `input` | Input | f | −12 – +12 dB | 0 dB | 20 ms | | Input trim, for a source hotter or weaker than a pickup at 0 dBFS = 1 V |
| `bass_keep` | Bass Keep | f | 0 (off) – 500 Hz | 0 | 30 ms | | Shared id: below it the signal skips the clipping (§2.7) |

Expert groups (for `visuals/data/groups.json`): **Circuit** (`diodes`, `opamp`, `supply`, `hp`, `fat`,
`contour`), **Input** (`input`, `tight`, `bass_keep`). Visibility (`depends`, as `hp` today): `hp` when
`circuit` = MOSFET OD, `fat` when Discrete OD, `contour` when Plexi Dist; `opamp` hidden on the Discrete OD.

**Ids added 2026-11-26** (ids are permanent once shipped): `fat`, `contour`, appended after `bass_keep` in
the parameter list. Why these and not a shared knob: each is one circuit's own control, as `hp` is (the
rule just below); a shared "fourth pot" id would mean three different things on three circuits and nothing
on four. The **Studio OD needs no new id**: its Spectrum is its only tone pot, so it is its `tone_stack`
(the ODR-1's own knobs are Gain, Spectrum, Level: exactly the face's three). "Preamp" (roadmap): the Red
Rox has no such pot; the role a "preamp" knob would have — how hard the first stages are driven — is its
`gain` (one pot drives both first stages) and `input`; no id is invented for it.

`hp` is inert outside the MOSFET OD, `fat` outside the Discrete OD, `contour` outside the Plexi Dist and `opamp` on the Discrete OD, as gfx-crusher's `quantizer` is on
formats that do not use it: no hidden re-mapping. A later circuit with its own switch appends its own id.
No `bypass` parameter: no tail, the host's bypass is enough (as gfx-drive and gfx-emet).

## Visual

- **Category: drive**, family finish **thrash** (FAMILY_FINISH.drive): gloss lacquer, diamond plate, the
  motif in yellow lined in black. The body hue is the thrash drive red `#c8160e` (`THRASH_HUE.drive`
  overrides any crate hue in `metalHue`, so no crate hue is proposed). It stands beside gfx-drive (same
  family, flames) and is told apart by its motif and face. Blackwork stays for the high-gain companions
  (Met, Emet, Shem).
- **Motif: `traces`** (new). Printed-circuit traces: three yellow traces each side run from the body's
  edge to the rune's cartouche, at y = cy − 0.6·ch, cy, cy + 0.6·ch (inside the rune band, clear of the
  knob grid that the gfx-freqshift review flagged). Each trace: horizontal from x = 4 to x = 0.18·W, a
  45° jog of ±6 units (outer traces bend away from the centre), horizontal to the cartouche's edge
  (cx − cw / 2 − 4), ending in a round pad (filled circle r 2.2). The middle trace carries a **diode
  symbol** at x ≈ 0.27·W: a filled triangle 6 wide pointing to the rune, with a bar. Mirrored on the
  right. Thrash: yellow, lined in black; anodized (if forced): engraved lines, pads as rings.

  ```js
  case 'traces': {
    const L = cx - cw / 2 - 4, sides = [[1, 0], [-1, W]];
    sides.forEach(([s, x0]) => [-.6, 0, .6].forEach(k => {
      const y = cy + k * ch, jog = k === 0 ? 0 : 6 * Math.sign(k), xa = x0 + s * 4, xb = x0 + s * W * .18,
            xe = s > 0 ? L : W - L;
      line(`M${f1(xa)} ${f1(y)}H${f1(xb)}L${f1(xb + s * 6)} ${f1(y + jog)}H${f1(xe)}`);
      shape(`M${f1(xe - 2.2)} ${f1(y + jog)}a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0Z`);
      if (k === 0) { const xd = x0 + s * W * .27;
        shape(`M${f1(xd - s * 3)} ${f1(y - 3.5)}L${f1(xd + s * 3)} ${f1(y)}L${f1(xd - s * 3)} ${f1(y + 3.5)}Z`);
        line(`M${f1(xd + s * 3)} ${f1(y - 3.5)}V${f1(y + 3.5)}`); }
    }));
    break;
  }
  ```
  (`MOTIF['gfx-dist'] = 'traces'`; coordinates to check on the family sheet at tile size. The pad is two
  half-arcs centred on the trace's end: the one-arc form first written left its centre to the renderer.)
- **Face** (`visuals/data/faces.json`): name word **"Dist"**, three knobs, the circuit selector.

  ```json
  "gfx-dist": {"name": "Dist", "blocks": ["dist"], "knobs": ["gain", "tone_stack", "level"],
               "selectors": ["circuit"], "category": "drive", "color": "#c8160e"}
  ```
- **Glyph** (`visuals/glyphs/dist.svg`): a hard-clipped sine with the cut-off peaks as faint ghosts.

  ```svg
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>Dist (circuit-simulated distortions)</title><path d='M2 12C3.5 8 4.5 6 6 6h3c1.5 0 2 2 3 6s1.5 6 3 6h3c1.5 0 2.5-2 4-6'/><path d='M6 6Q7.5 1.5 9 6M15 18Q16.5 22.5 18 18' opacity='.55'/></svg>
  ```
- No maker's livery, logo or trademark on the pedal; circuit names on the selector are generic.

### Visual changes (2026-11-26)

- **Face**: unchanged (`faces.json` keeps three knobs and the `circuit` selector). The selector draws one
  tick per option (`selectorClay` in `visuals/js/golem-pedals.js`, 30° apart): 4 → **7 ticks, the sweep
  90° → 180°**; its label (the option, lower case) gains "discrete od", "studio od", "plexi dist" — the
  longest, "booster dist", is unchanged, so the label's width is too. Nothing to draw by hand.
- **Descriptors** (`visuals/data/descriptors.json`, regenerated from the plugin): `circuit` max 3 → 6 and
  its options; `fat` (bool) and `contour` (%) in the Circuit group; `groups.json` gains the two
  `depends` rules of §3. The gfx-ui bundle is rebuilt (`build-ui.js gfx-dist`, one pedal per process,
  under the memory cap).
- Motif, glyph, colours: unchanged.

## 4. Applications

Reference settings become block presets (`presets/gfx-dist.json`, ≈ 15, generic names, refs by style
not by trademark). Presets are level-matched to the bypass (±2 LU on the bench DIs) with `level`: the
`level` values below are indications (with K, 0 dB is already near unity at the defaults).

| Use / preset idea | Settings to start from | Feature that matters |
|---|---|---|
| **Slew Wall** (90s alt-rock / grunge rhythm) | Op-Amp Dist, `gain` 70 %, `tone_stack` 60 %, `level` −6 dB | Slew-limited op-amp: thick, not fizzy |
| **Filter Lead** | Op-Amp Dist, `gain` 85 %, `tone_stack` 45 % | The filter does the voicing |
| **Rodent Crunch** (low-gain, into a clean amp) | Op-Amp Dist, `gain` 30 %, `tone_stack` 70 % | Op-amp barely slewing: open crunch |
| **Turbo Lights** (LED mod) | Op-Amp Dist, `diodes` LED, `gain` 75 %, `level` −12 dB | Higher clip: louder, more open, rail clipping heard |
| **Op-Amp Only** | Op-Amp Dist, `diodes` None, `gain` 90 %, `level` −15 dB | Rail clipping + slew: the raw LM308 |
| **Dying Battery** (sputter, gated decay) | Op-Amp Dist, `supply` 70 %, `gain` 80 % | Supply moves rails and bias |
| **Rodent Bass** | Op-Amp Dist, `gain` 60 %, `tone_stack` 55 %, `bass_keep` 120 Hz | Fundamental clean, through the same filter as the driven band |
| **Orange Crunch Into Stack** (80s hard rock into a cranked amp) | Booster Dist, `gain` 55 %, `tone_stack` 60 %, `tight` 120 Hz | Booster + op-amp + scoop; tight lows |
| **Booster Lead** | Booster Dist, `gain` 85 %, `tone_stack` 45 % | Transistor soft clip before the diodes: singing sustain |
| **Edge of Breakup** (blues, volume-knob player) | Discrete OD, `gain` 35 %, `tone_stack` 35 % | Stage 2 clipping on its rails: cleans up with the guitar's volume; tone below noon (the treble) |
| **Cranked Blues** | Discrete OD, `gain` 75 %, `tone_stack` 40 % | Asymmetric discrete breakup, bass bump |
| **Transparent Push** (clean boost into a pushed amp) | Blend OD, `gain` 10 %, `tone_stack` 50 %, `level` −6 dB | Clean feeds dominant; mids filled in; the sum on the pumped 27 V never clips |
| **Gritty Core** | Blend OD, `gain` 50 %, `tone_stack` 55 % | Germanium grit and a 9 V gain stage over a clean core: the circuit's own phase-coherent sum |
| **Blend Into Slew** (stack) | Blend OD `gain` 15 % before a second Op-Amp Dist | Two instances; the first tightens and pushes the second |
| **Amp in a Box** (indie / classic rock) | MOSFET OD, `gain` 55 %, `tone_stack` 50 %, `hp` off | Body-diode knee into a second gain stage; `diodes` MOSFET + Ge for the asymmetric units |
| **HP Roar** | MOSFET OD, `gain` 70 %, `hp` on | Brighter, looser, louder |
| **Germanium Rat** | Op-Amp Dist, `diodes` Germanium, `gain` 65 % | Softer knee, more output |
| **Solo Lift** | any circuit, EXP on `gain` | Pot travel with the real taper, smoothed per contract |
| *Added 2026-11-26* | | |
| **Fat Blues** (modern blues rhythm into a clean amp) | Discrete OD, `gain` 40 %, `tone_stack` 40 %, `fat` on | The "Custom" bass switch under the asymmetric discrete breakup |
| **Volume-Knob Blues** | Discrete OD, `gain` 60 %, `tone_stack` 35 % | Even-order breakup from the pair and the class-A PNP: cleans up with the guitar's volume |
| **Studio Crunch** (Nashville session rhythm) | Studio OD, `gain` 35 %, `tone_stack` 55 %, `tight` 120 Hz | Upper-mid pre-emphasis into two clippers; `tight` stands in for the clones' Bass knob (the stock bass is huge) |
| **Spectrum Lead** | Studio OD, `gain` 75 %, `tone_stack` 75 % | The Spectrum's bright side over IC2B's shelf clipping on low notes |
| **Dark Spectrum** (jazz / neo-soul edge) | Studio OD, `gain` 25 %, `tone_stack` 15 % | The Spectrum's 700 Hz roll-off: warm, still articulate |
| **Plexi Crunch** (70s hard-rock rhythm into a clean amp) | Plexi Dist, `gain` 40 %, `tone_stack` 55 %, `contour` 70 % | Three op-amp stages, the bright cap at low gain, mids kept |
| **Scooped Stack** (80s metal rhythm) | Plexi Dist, `gain` 80 %, `contour` 0 %, `tight` 100 Hz | The bridged-T scoop at 1 kHz (−11.7 dB) |
| **Plexi Bass** | Plexi Dist, `gain` 50 %, `contour` 60 %, `bass_keep` 100 Hz | The fundamental clean through the post-clipper twin (polarity −1) |

## 5. Bench suite (on top of gfx-bench's common checks)

**gfx-bench options**: `skip_latency` (minimum-phase group delay, gfx-emet's precedent); `click_bound_db`
−35 dB (a high-gain clipper puts real energy above 10 kHz on any step: gfx-drive's precedent, tightened if
measured). **Reference**: the same netlists solved by `circuit_nl`'s offline mode — monolithic (no stage
cuts, op-amp output resistance and current limit inside the loops), 64×, Newton to 1e−12, transistor-level
for the discrete op-amps. ngspice is not installed here;
if it becomes available, a one-off cross-check of each netlist's DC point and 1 kHz harmonics is added.
Targets are computed in the tests, not copied by hand.

| Check | Pass |
|---|---|
| Small signal | Each circuit, 1 mV sine (no clipping), `gain` 0 / 50 / 100 %, `tone_stack` 0 / 50 / 100 %: within 0.25 dB of the linearized netlist's Laplace response (`Netlist::response`), 20 Hz – 20 kHz |
| Anchors (Op-Amp Dist) | Gain-stage small-signal gain at full gain follows the analysis with the macromodel's GBW (67 dB asymptote, −3 dB near 430 Hz); small-signal filter corner 470 Hz at 0 % and 19.3 kHz at 100 % ± 3 % (1 kΩ included, diodes off); output slew of a 10 kHz full-gain square ≤ SR × 1.05 and ≥ SR × 0.9 |
| Large signal | Each circuit, 100 Hz / 1 kHz sines at −24 / −12 / 0 dBFS, three `gain` settings: harmonics 2 – 9 within 1 dB (−60 dB re fundamental and above) of the reference; waveform null ≤ −30 dB |
| Stage cuts | Cut graph vs monolithic with zero output resistance, same rate: null ≤ −100 dB (the cuts are exact). Vs the reference with 75 Ω and the current limit in the loop: harmonics within 0.5 dB, null ≤ −40 dB; a circuit that misses gets the resistance stamped and loses that cut |
| Discrete macromodel | Discrete OD macro vs its transistor-level reference: harmonics 2 – 9 within 1 dB at three levels, asymmetry (2nd harmonic) within 1 dB |
| Blend OD sum | `gain` 0 / 25 / 100 %: the clean and clipped contributions (reference run with each path muted) sum to the output within 0.1 dB at 100 Hz – 10 kHz small-signal; no comb (no notch > 1 dB) on a sweep at −12 dBFS |
| Newton | All test signals and DIs, plus squares and full-scale noise at +12 dBFS, every `opamp` / `diodes` option, 44.1 – 192 kHz: 0 unsolved samples, no NaN; mean iterations ≤ 3 at defaults; the continuation fallback on ≤ 1 % of samples; worst sample and worst 32-sample block reported |
| Uniqueness | det J keeps its sign over the port range, every stage, pots at 0 / 50 / 100 % |
| Stiff modes | Oversampled output before decimation, full-gain 1 kHz sine: energy above a quarter of the oversampled rate ≤ −80 dB re the signal (else backward Euler on the offending capacitor, §2.2) |
| Aliasing | −12 dBFS 2 and 5 kHz sines, full gain, `tone_stack` 100 %: aliases in the audible band ≤ −85 dB re the fundamental on every circuit with its own op-amp, every `diodes` option (16× for a circuit that misses); ≤ −65 dB with a swapped fast op-amp (§2.5) |
| Taper | Small-signal 1 kHz gain vs `gain` at 0, 10 … 100 %: matches the netlist with its pot law within 0.3 dB |
| DC point | Activation, `circuit` switch and `reset`: output transient ≤ −60 dBFS; no DC at the output; the inline re-solve's worst time reported (bound: 10 % of a 64-sample block) |
| Supply | `supply` swept 50 → 200 % over 2 s under a sine: no click above the bound; rails and bias move monotonically |
| Switches | `circuit`, `diodes`, `opamp`, `hp` switched under a 200 Hz sine: no click above the bound |
| Zipper | `gain` and `tone_stack` swept on a 1 mV sine (nothing masks it), and EXP 50 Hz stepped (contract): no control-rate sidebands > −80 dB re the signal; pot at 0 % and 100 % reached without a singular update |
| Bass keep | `bass_keep` 120 Hz, 60 Hz + 1 kHz at −12 dBFS, full gain, every circuit: THD of the 60 Hz ≤ −40 dB and no ±60 Hz sidebands above −40 dB around 1 kHz (the low band is clean). Op-Amp Dist at `gain` 0, 1 mV (its driven path is then unity and linear): no notch deeper than 3 dB within ±1 octave of the crossover |
| Level | Defaults on each circuit, reference DIs: loudness within ± 1 LU of the bypass (K); the K table printed |
| Cleanup | DI at 0, −6, −12 dB (`input`): THD falls monotonically; the Discrete OD and Blend OD drop by ≥ 10 dB of THD between 0 and −12 dB at `gain` 50 % [A, target to confirm against the reference] |
| Determinism, CPU | Strict null; ≤ 2.0 µs / sample mono at 48 kHz per circuit (≤ 10 % of a core; 2.5 µs for a transistor-level Discrete OD), worst case reported per circuit |
| Listening | crunch.wav, lead.wav, high-gain.wav through a NAM amp, each circuit against NAM captures of the real pedal families at matching settings (`pedal-nam`, where golem has them), BYOD's Mouse Drive / Centaur and ChowCentaur, and gfx-drive Distortion / Overdrive; blind, level-matched |
| *Added 2026-11-26* | |
| Old presets, old circuits | Every preset of `presets/gfx-dist.json` and the 16 reference cases rendered before and after the three circuits are added: **bit-identical** (strict null), at 44.1 – 192 kHz; a state saved before (four options) reloads to the same circuit. **gfx-fuzz too** (review): `circuit_nl` is shared, so its presets and reference cases are rendered before and after the JFET and pair devices are added, strict null |
| Enum append on REAPER (review; golem-rack's proof) | With the seven-option plugin, each of the catalogue's `circuit` options written through golem_rig reads back under its own name; the check fails with a four-option catalogue (§2.11, release gate) |
| Response curves (roadmap's "Fait quand") | Each new circuit: the small-signal response (0.1 mV) at `gain` 0 / 50 / 100 %, `tone_stack` 0 / 50 / 100 % (and `contour` 0 / 50 / 100 %, `fat` off / on) within 0.25 dB of `ac_response`, the curves written to the bench report |
| Anchors, Discrete OD | Stage gains at 1 kHz: stage 1 +21.9 / +27.6 / +41.7 dB, stage 2 +24.1 / +28.3 / +40.7 dB at `gain` 0 / 50 / 100 % ± 0.5 dB (the low loop gain included) [P]; the IC stage's lift **+5.8 dB ± 0.5 at 120 Hz ± 5 %** [P]; `fat` at `tone_stack` 50 %: +4.8 ± 0.5 dB at 120 Hz, +2.0 ± 0.5 dB at 1 kHz |
| Anchors, Studio OD | In → IC1A out peaks at **2.8 kHz ± 5 %, 11 ± 0.5 dB over 100 Hz** at `gain` 50 %; clip node → out at Spectrum 0 %: 5 kHz 15.7 ± 1 dB under 1 kHz; at 100 %: 3 kHz +4.1 ± 1 dB over 1 kHz [P] |
| Anchors, Plexi Dist | In → IC2.1 out 67.6 ± 0.5 dB at 1 kHz, full gain; Contour 1 kHz re 100 Hz −11.7 / −7.2 / −2.5 dB ± 0.5 at 0 / 50 / 100 % [P] |
| Discrete stage vs transistor level | Each discrete op-amp (pair port with the common-mode tail + PNP) vs the transistor-level reference (2 JFETs, offline, 64×): harmonics 2 – 9 within 1 dB (those ≥ −60 dBc; the prototype's worst is 0.35 dB there, 0.8 dB on an 8th at −62 dBc) at 10 / 50 / 200 mV / 1 V, 1 kHz and 4 kHz, three gains (replaces the "Discrete macromodel" row); a miss with the lagged v_cm → the 2-port pair (§2.11; not expected: the lag costs ≤ 0.05 dB at 8× [P]) |
| Rail clipping where claimed | Studio OD IC2B and Plexi IC2.2 reach their rails on a 100 Hz sine at −12 dBFS, full gain (the macros are needed); IC2A, IC1.2 and the Discrete OD's IC stage stay ≥ 0.5 V inside them on a +6 dBFS sine **of 41 – 330 Hz and 1.2 kHz** at every setting — full gain, `tone_stack` 0 and 100 %, `fat` on, `supply` 50 – 200 % (their linear stamps hold). Review: a clipped stage is a square, whose peak after a network exceeds the sine figure (1.55× on the Discrete OD with the tone open [P]), and `fat` raises it by a third: the test probes the node in the time domain, it does not multiply a response |
| Aliasing (policy) | 1.2 kHz, −12 dBFS, full gain, tone open: ≤ −70 dB with own parts — Discrete OD and Plexi Dist at 8× (prototype −70.6 / −71.3 dB), Studio OD at 4× (−77.0 dB), the Discrete OD with the lagged tail (prototype −72.2 dB), `contour` at 100 %; ≤ −65 dB under every `diodes` / `opamp` option, 16× for a variant that misses. Plexi Dist at `contour` 0 %: ≤ −65 dB (a setting, decided 2026-11-26; prototype −66.6 dB); all seven circuits at `supply` 50 % and 200 % (prototype: Discrete OD −71.5 / −71.3 dB), the Plexi Dist at its 150 % cap: ≤ −65 dB (prototype −65.9 dB; 200 % reads as 150 %) |
| CPU | Measured per new circuit; ≤ 4.2 µs mono at 48 kHz; Discrete OD and Plexi Dist ≤ 5 µs (≈ 24 %, accepted 2026-11-26) (§2.11's estimate ≈ 5.0 / 2.3 / 5.0 µs) |
| `fat`, `contour` switches | Under a 200 Hz sine at full gain: no click above the bound; `fat` from its DC point silent (the 83 nF charged) |

## 6. Positioning

| Pedal | What it is | Why gfx-dist exists beside it |
|---|---|---|
| gfx-drive | One parametric engine: filters + shaper families, slew as a level-dependent low-pass, EXP-morphable, alias-free, legacy ids | gfx-dist is the real circuits: slew and rails inside the loop, real tapers, real clean sums. Costs 6 – 10× more CPU and cannot morph between families |
| gfx-emet | The 808 family, high-headroom, three knobs | The TS stays there (`headroom` −13 dB is the 808); gfx-dist covers the other classic circuits |
| NAM (`pedal-nam`) | One setting of one unit, exact | gfx-dist has every pot position, the supply, the mods, and responds to the guitar's volume by physics |
| gfx-fuzz | The transistor fuzzes, with the pickup in the netlist | Same solver (`circuit_nl`), different front end and controls: op-amp and diode drives here, transistor fuzzes there |

Against the field: Helix / Fractal model these families; Fractal exposes generic expert controls (clip
type, slew, supply) — gfx-dist gets the same controls **as physical parts** (diodes, op-amp, supply) in a
solved circuit. BYOD and ChowCentaur are the open-source state of the art for single circuits; ChowCentaur
solves its op-amps as ideal with static clips and oversamples only its diode stage 2×. gfx-dist puts five
circuits on one solver, with op-amp dynamics inside the loops, exact stage cuts and a shared,
deterministic bench.

## Open questions for review — settled (2026-11-14)

1. **`level` reference**: **unity loudness at the defaults, by a fixed constant K per circuit** (§2.4),
   with "hardware fully up" given per circuit. The hardware reference suits a one-circuit pedal
   (gfx-emet); behind a selector it is a 20 dB jump.
2. **Schematic sources**: fixed, one clone document per circuit (§2.1); the values of §1.1 are read from
   them. Still to read when each netlist is entered: SPICE constants of the parts, the original bias
   dividers, the DS-1's transistor type.
3. **Op-amp constants**: table of §2.3. The LM308's 0.3 V/µs and 1 MHz are curve readings, and they are
   mutually consistent with a plain bipolar pair, which is the model. LM358-class figures remain [U].
4. **`bass_keep` twin**: the post-clipper twin at unity gain. The whole linearized circuit is ruled out
   (§2.7).
5. **Pickup loading**: not here. gfx-fuzz owns the pickup source network, built in `circuit_nl`; if it
   proves audible on a ≥ 325 kΩ input, the same front end is appended here as one expert enum.
6. **16× fallback**: not a fallback and not for LED / None. The Op-Amp Dist runs at 16× always; fast
   op-amp swaps carry a −65 dB bound (§2.5).
7. **Name and type**: `dist`, "Dist", category drive: kept. For golem-rack (below).
8. **Fuzz**: a separate **gfx-fuzz** (owner's decision), on the same `circuit_nl` (§2.10). Nothing fuzz
   is appended to `circuit` here.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good**, Strong once the solver passes its own bench. The idea is right and nobody ships it:
five circuits solved with their op-amps' dynamics inside the loops (ChowCentaur, the open-source
reference, uses ideal op-amps with static clips). The op-amp law is better than the draft knew: the
LM308's slew and bandwidth are exactly those of the tanh pair it proposes. What did not hold: the
circuits as recalled (four of five had a structural error), the Newton specification, the pot update,
the alias target, the CPU estimate and two of the stage cuts. All are fixed above; none changes the
architecture.

Prototype (`proto-gfx-dist/` in the review scratchpad: `rat.c`, `rat3.c`, `alias.py`, `zipper.py`): the
Op-Amp Dist's gain stage with the macromodel, its clipper and filter, implicit trapezoidal, Newton.
Figures marked [P] come from it. It is one circuit; the other four are inferred.

| # | Change | Why |
|---|---|---|
| 1 | **Circuits re-read from schematics** (§1.1, §2.1); one source document per circuit fixed | electrosmash was unreachable again; the clone documents were read instead. Errors found: the DS-1's op-amp is not a 4558 nor an M5218 (TA7136P, then LM358-class parts) and its pot is a three-terminal linear one with 10 nF across the diodes; the BD-2's diodes sit **before** stage 2, which clips on its rails, and its level pot is before the last stage; the Klon's gain stage runs on **9 V** (only the sum has the pumped rails) and its full-range clean feed is gone at maximum gain (a fixed bass path stays); the OCD has no JFET input in v1.x, two op-amp stages, clippers to the bias, and an HP switch that parallels a series resistor |
| 2 | **RAT filter corner 19.3 kHz, not 32 kHz**, small-signal; bench anchor corrected | The 1 kΩ before the diodes is in series with the filter. 32 kHz holds only while the diodes conduct |
| 3 | **Gain-bandwidth named as the RAT's first mechanism** (430 Hz closed loop at full gain); slew figure qualified | 0.3 V/µs is read from a curve, not a table; the curve is asymmetric (0.2 / 0.29). The model keeps I_t⁺ / I_t⁻ |
| 4 | **Newton respecified**: backtracking, limiting, cap 20, continuation in p, hold the previous solution; partial pivoting | "8 iterations, 2 – 3 expected, 0 unconverged" fails on an ordinary sine: 1 – 2 % of samples need 9 – 17 [P]; a fast op-amp fails plain Newton twice per period [P]. "The Jacobian of a passive circuit" does not apply: op-amp ports are active |
| 5 | **Pots: Woodbury update every sample**, not a rebuild every 16 | The cited paper's actual method. The 16-sample staircase leaves −50 to −55 dB of error on a clean signal [P]; the contract asks −80 dB |
| 6 | **Stage cuts**: a graph, two exactness rules, output resistance removed from the real-time model; three cuts moved | The Klon is not a chain. A resistance stamped "downstream" breaks the exactness it was meant to refine. A collector and a discrete op-amp's output are not voltage sources |
| 7 | **DC point solved inline**, seeds from `initialize`; the background task is dropped | Tens of µs, bounded; a lock-free hand-off was complexity without a need |
| 8 | **Oversampling per circuit; target −85 dB; table of measured aliases** | 8× gives −64 dB on the stock Op-Amp Dist at 5 kHz, −49 dB with a fast op-amp [P]. The fallback aimed at LED / None, which are the cleanest cases |
| 9 | **CPU 0.4 – 1.2 → 1.2 – 2.5 µs**, bound 2.0 µs | `exp` calls dominate and were counted as flops. Port counts lowered where an op-amp provably stays linear |
| 10 | **Op-amp model**: separate rails, optional current limit, output resistance on the discrete macro; `opamp` gains LM358 | A 358-class part reaches ground; stage 2 of the Discrete OD clips, so its output is not stiff |
| 11 | **`level` 0 dB = unity loudness (K per circuit)**, range to +24 dB | Open question 1; §2.4 |
| 12 | **`bass_keep`**: twin at unity gain with the circuit's polarity; the linearized-circuit fallback removed; bench rewritten | The fallback would pass the bass at the circuit's full small-signal gain. The old notch test at `gain` 0 was blind on circuits whose path is +35 dB there |
| 13 | **`tone_stack` reversal kept**, example corrected (9 o'clock = 80 %, not 75 %), law given | One meaning per id; §2.4 |
| 14 | **`circuit_nl` made one shared module**, API table for gfx-fuzz, names aligned | Two documents were specifying two solvers (`circuit_nl` here, `NonlinearCircuit` inside `circuit` there; `Os8` vs `Oversampler8x`) |
| 15 | Fuzz removed from this pedal; `diodes` gains MOSFET + Ge; `opamp` inert on the Discrete OD; taper law corrected (gfx-emet's is exponential, not two-slope); references verified; motif pad path | Owner's decision on gfx-fuzz; accuracy |

Checked and left as written: the DK formulation; trapezoidal states kept across pot moves; SR = I_t / C_c
(0.300 V/µs measured for 0.3 [P]) and GBW from the same law; the gain (67 dB) and the two leg corners
of the Op-Amp Dist; reported latency 0 with `skip_latency` (the gfx-drive / gfx-emet precedent); the
ids (`gain`, `tone_stack`, `circuit` vs `model` / `mode`), linear ranges, no mode-dependent default;
`hp` inert elsewhere; the Blend OD's sum solved in one stage; the thrash finish and the `traces` motif
as an idea.

Coverage: the five families are the right five beside gfx-emet and gfx-fuzz. Fractal's expert controls
are matched as physical parts. Left out on purpose: a Timmy-type (feedback clipping: gfx-emet's
territory), a Marshall-type (Guv'nor / Bluesbreaker) — the first candidates to append. Nothing
superfluous; `hp` is the only one-circuit control.

Still open:

| Point | Owner |
|---|---|
| The solver on the four circuits not prototyped (iteration counts, cuts, aliases at 8×); this is the first coding task, on the Op-Amp Dist, before any plugin code | golem-fx |
| CPU: ≈ 10 % of a core per mono instance accepted, or cap lower (then the cache / tabulation path) | The maintainer, after the bench |
| Fast op-amp swaps on the Op-Amp Dist at −65 dB aliases: ship, run at 32×, or drop TL072 / Ideal from `opamp` there | The maintainer, after the bench |
| Discrete OD: macro vs transistor-level stages; the level pot's fixed position; the gyrator's lift (6 vs 11.7 dB) | Bench (reference run) |
| DS-1 version: dual-op-amp topology with pre-1994 values is modelled, not the TA7136P itself (no model of that IC exists); LM358-class constants [U] | The maintainer, by ear; datasheets |
| LM308 slew asymmetry and output current limit; SPICE constants of every part; ngspice cross-check (not installed) | golem-fx |
| Who creates `circuit_nl` (gfx-dist or gfx-fuzz first); gfx-fuzz's document to adopt the same module, type and oversampler names, the Newton rules of §2.2 (its cap of 8 and "keep the last iterate" have the same flaw) and the per-sample pot update | The maintainer / the gfx-fuzz review |
| Add `level` and `tone_stack` to the contract's shared-id table, with "0 dB = unity" for a multi-circuit pedal | The maintainer (contract) |
| Catalogue entry `dist` (new type, category drive, no ids to keep); golem already has pedal archetypes `rat`, `ds-1`, `blues-driver`, `klon`, `ocd` bound to `pedal-nam` captures: whether each also offers gfx-dist with `circuit` pinned | golem-rack |
| Whether EXP may target `gain` on a block whose `circuit` is pinned; presets per archetype | golem-rack |
| `traces` beside gfx-drive's flames on the same red at tile size | The maintainer, on the family sheet |

### Alignment with gfx-fuzz (2026-11-14, after both reviews)

The gfx-fuzz review (its §2.9 holds the shared `circuit_nl` API table) asks this design to align on:
- leakage is a BJT parameter (scaled collector-junction saturation current), not a current source; the
  pickup is an EMF inverse filter plus the guitar network in the netlist, so the module needs inductors;
- the seed-extrapolation guard: extrapolate the Newton seed only from samples solved by plain Newton;
- no "approximate cut" mode in the first version;
- BJT and op-amp constants as run-time parameters, with SPICE's temperature law and a reference
  temperature per device set;
- a 4× path, node-voltage read-out for meters, `to_spice()` with committed fixtures, one shared fast `exp`;
- alias policy: this design targets −85 dB, gfx-fuzz −70 dB at 1.2 kHz (≤ −65 dB worst); one policy for
  both is the owner's call.

**Policy settled by the maintainer (2026-11-14):** one alias policy for all circuit simulations — worst alias ≤ −70 dB
for a 1.2 kHz sine at full gain (≤ −65 dB worst model or part swap), oversampling chosen per circuit from the
measured aliases; CPU up to ~20 % of a core per mono instance accepted (see `docs/golem-contract.md`). The
−85 dB target and the fixed 16× for the Op-Amp Dist above are re-decided by the bench against this policy.

## Open questions for review (2026-11-26: three more circuits) — answered at the review

1. **Names on the selector**: "Discrete OD", "Studio OD", "Plexi Dist". "Plexi" is a player's nickname
   for a period of amps, not a maker's mark, and the roadmap uses it; if the review wants no association
   at all, "Stack Dist". "Studio OD" was chosen over "Natural OD" (the ODR-1's own subtitle).
   **Review: kept, all three.** The four coded names say the topology and two of these say the voice;
   the selector is read by a player, and "Plexi" names no maker (the roadmap's rule) while saying what
   the circuit is for. "Natural OD" would be the original's own subtitle: rightly avoided. The
   maintainer has the last word on "Plexi".
2. **`fat` as a switch, not a fourth circuit.** The Custom is one capacitor on the Discrete OD; as an
   enum entry it would be a fourth copy of a netlist and a K of its own. Kept as a switch (like `hp`),
   which is also what the hardware has. The Keeley value is from builders' threads [F], no maker's
   document; its "Phat off" position (33 nF) is not offered: off = the stock 18 nF.
   **Review: a switch, settled.** It is +5 dB at 120 Hz on the same netlist; an enum entry would also
   cost a `circuit` crossfade for what a 20 ms resistor glide does. Its value stays [F].
3. **Plexi source**: the Red Rox via PedalPCB's Ultisol (a documented schematic, one of the roadmap's two
   references) rather than a Guv'nor (a Marshall pedal with a three-band stack) or the Boiling Point (no
   document found). If the owner wanted the Guv'nor, it is a fourth append, not a replacement.
   **Review: the Red Rox, settled.** The roadmap names it; its schematic was re-read at the review and
   the netlist holds part for part; a cascade of gain stages into a mid control is nearer an amp than
   the Guv'nor's one stage and stack (the 2026-11-14 Review named Guv'nor / Bluesbreaker as "a
   Marshall-type", before the roadmap chose). Its weakness is one source: a single clone trace, no
   second document to cross-check, an unnamed germanium diode, and an LM833 below its datasheet's
   supply range on 9 V — and its three op-amps rail-clip, so the macro's rails are the sound at high
   gain. A capture of the real pedal (`pedal-nam`) is what would make it Strong.
4. **`contour` on the face?** It is the Plexi Dist's second tone pot, but a fourth knob would be dead on
   six circuits: kept in the expert Circuit group with a `depends`. golem's rack could pin it on a Plexi
   archetype. **Review: expert, settled** (the `hp` precedent); its default moves to 70 % (§3). golem-rack:
   a Plexi archetype pins `circuit` and shows `contour`.
5. **CPU**: Discrete OD and Plexi Dist estimated ≈ 5.0 µs (≈ 24 % of a core) at 8×, where 4× misses the
   alias policy (−60 / −50 dB). Accept, or the cache / linear-stamp path of §2.11 first.
   **Review: measure first; then the maintainer's decision.** The estimate is an extrapolation (§2.11);
   4× is confirmed out (−60.7 / −49.7 dB). If the bench reads ≤ 5 µs, the review's advice is to accept it
   for these two circuits: mono, that is 8 µs of a 20.8 µs sample period on the loaded machine where
   two circuits at 14 – 17 µs stuttered, and linked mono keeps a guitar lane at one circuit.
   **Decided by Raph, 2026-11-26:** ≈ 24 % accepted for these two circuits if the coded measurement is
   ≤ 5 µs per sample.
6. **The discrete stage's tail**: following the common mode is needed (a fixed tail misses h2 by 2.3 dB
   at one point) and matches within 0.1 dB when solved implicitly [P]; the one-sample-lag form that keeps
   one port is untested on the prototype — the bench row decides between it and a 2-port pair.
   **Review: tested, one port settled** (§2.11): the lag costs ≤ 0.05 dB at 8×, the whole circuit's
   aliases and Newton counts are those of the transistor level.
7. **The Discrete OD's level pot at 90 %** to keep the IC stage linear: the hardware fully up clips that
   stage by ≈ 1.3 dB on stage 2's largest swing. Alternative: keep the pot at 100 % and give IC1B its
   macro (2 more ports, ≈ +0.9 µs).
   **Review: the principle holds, the position did not: 50 %** (§2.1). At 90 % a clipped stage 2 asks
   the IC stage for 3.2 V (4.3 V with `fat`) of 3.05 V. A macro for IC1B would buy the sound of a BD-2
   with its volume fully up at full gain — a setting nobody plays — for 2 ports on a circuit already
   over budget, and would need D6 / D7 as well: not taken.
8. **REAPER automation of `circuit`** written before the append moves (index / (n − 1)); golem's own
   values are safe once its catalogue lists the seven options. Accepted as stated, or a migration note
   for golem-rack.
   **Review: accepted for a user's own REAPER envelopes; a release gate for golem** (§2.11): out of
   step, three of the four existing circuits select another one on REAPER. Plugin and catalogue ship
   together, with the read-back check.
9. **Part constants** [U]: 2SK184-GR (β, V_TO, λ), 2SA1335-GR, the germanium diode of the Plexi's clipper,
   1N4001, LM833 on 9 V. To read from SPICE models when the netlists are entered; the anchors of §5 are
   written so that they hold for the circuit, not for one constant set. **Review: stays open** (golem-fx).
10. Prototype limits: Jacobians by differences, monolithic solves, no stage cuts; the aliases are
   properties of the discretized circuits (converged to 10⁻⁶ V) and stand, the iteration counts do not.
   **Review: agreed**; the aliases were reproduced to 0.1 dB on a re-run.
11. **(Raised by the review.) The Plexi Dist against the alias policy** (§2.11): −71.3 dB at `contour`
   50 %, −66.6 dB at 0 %; −60.5 dB at `supply` 200 %. For the maintainer: (a) is a post-clipper scoop "own
   parts" (−70 dB: then the Plexi fails below ≈ 40 % of its Contour) or a setting under the −65 dB
   bound (then it passes)? (b) at 18 V: accept −60 dB on an expert setting, run the Plexi at 16× above
   ≈ 140 % of supply (−80 dB, ≈ 10 µs), or stop the Plexi's `supply` at 150 % (−65.9 dB) and drop the
   "18 Volt Plexi" preset. The review's advice: (a) a setting, −65 dB; (b) stop at 150 % — 16× does not fit
   a live lane, and −60 dB is outside every bound the policy has.
   **Decided by Raph, 2026-11-26:** as the review advises: (a) the scoop is a setting (−65 dB: the Plexi
   passes over the whole Contour); (b) the Plexi's `supply` stops at 150 % (−65.9 dB) and the "18 Volt
   Plexi" preset is dropped.

## Review (Fable 5.2, 2026-11-26) — three voicings

**Verdict.** The three circuits are the right three and the work is sound: every netlist was re-read
against its document at this review and holds part for part; the stage cuts, port counts, DC points and
small-signal figures were recomputed and hold. The idea that carries the Discrete OD — a square-law
pair port plus the real PNP instead of an op-amp macro — is right, is new, and is now verified in the
form that ships. Three things did not hold: the level pot's position (an error: the linear stamp would
have been asked for more than its rails), the alias figures of the Discrete OD (measured on another
model than the one designed; the real one is better) and the Plexi Dist's place inside the alias policy
(it passes at one Contour setting and at 9 V only). None changes the architecture.

| `circuit` | Verdict | Rivals, in its use | Main risk |
|---|---|---|---|
| Discrete OD | **Strong** | The big modellers' BD-2 types, NAM captures; no model known to the review solves the discrete stages with their even order below clipping [U] | CPU (≈ 24 % of a core, estimated); part constants [U] |
| Studio OD | **Strong** | The big modellers' ODR-1 types, NAM captures | The 4558 rails of IC2B, which clip the low notes: they are the "enormous low end"; Q1 taken as ideal |
| Plexi Dist | **Good** | Marshall-in-a-box pedals as captures; amp models do the real thing better, and that is not this pedal's job | One source document; three rail-clipping LM833s on an unspecified 9 V; aliases at the scooped end and at 18 V |

Prototype: the design's `proto/` scripts, re-run and extended (all under the 3 GB cap): `nl.py` gained
the pair port's `lag`; `bd2.py pairl`; `bd2full.py alias <osf> <model> <level> <fat> <dBFS>` (its
defaults were the fixed tail and the level pot at 50 %; they are now the designed form) and `head`;
`odr.py alias <osf> <spectrum>`; `plexi.py alias <osf> <contour> <supply> <gain>`.

| # | Change | Why |
|---|---|---|
| 1 | **Discrete OD: level pot fixed at 50 %, not 90 %** (§2.1, §2.11, §5) | The 90 % came from a sine through a response. Stage 2 clipped is a square: 3.2 V asked of 3.05 V at 90 %, 4.3 V with `fat`, which the estimate had not included [P]. 50 %: 0.64 V at worst. The IC stage stays a linear stamp |
| 2 | **Pair port with the lagged common mode: verified, one port settled** (§2.11); DC rule, latch and limits added to the API text | Only the implicit form had been run. Lag vs implicit ≤ 0.05 dB at 8× (0.43 dB at 4×); whole circuit = transistor level (alias −72.2 for −72.2 dB, same Newton counts, 0 unsolved). The DC point needs the tail implicit; the latch is state |
| 3 | **Discrete OD's alias: −72.2 dB at 8×, 2.2 dB of margin, not 0.6** (§2.11) | `bd2full.py` built the **fixed-tail** pair and the level pot at 50 % whatever the text said; the designed model and the transistor level agree at −72.2 dB. With `fat` −72.4; `supply` 50 / 200 % −71.5 / −71.3 dB |
| 4 | **Plexi Dist's alias depends on `contour` and `supply`; stated, bench rows added, open question 11** | −71.3 dB was the Contour at noon. The alias sits at 395 Hz and the bridged T scoops the fundamental under it: −66.6 dB at 0 %. At 18 V: −60.5 dB (IC2.2 stops clipping); 16× gives −80.1 dB at twice the cost |
| 5 | **`contour` default 70 %, not 50 %** (§3) | A log pot's noon is 9 % of its track: −7.2 dB of scoop as the default voice of a "Plexi". 70 % = −4.6 dB, and the widest alias margin. To confirm by ear |
| 6 | **Enum append: a release gate with a read-back check**, not a note (§2.11, §5) | Out of step on REAPER, golem's Booster Dist plays the Blend OD, Blend OD the Discrete OD, MOSFET OD the Plexi Dist. `fat` / `contour` appended after `bass_keep` so no parameter index moves |
| 7 | **`fat` off = 22 MΩ, not 1 GΩ** (§2.11) | The 18 nF holds 4.1 V of bias; behind 1 GΩ the 83 nF trails a moved `supply` by 83 s and thumps when switched in |
| 8 | **Bit-identity extended to gfx-fuzz** (§5) | `circuit_nl` is shared: its new devices must not move gfx-fuzz's renders either |
| 9 | Survey corrections (§1.1): the Studio OD's input low-pass is 3.4 kHz (2.7 kΩ ∥ 10 kΩ), not 2.7 kHz; D6 / D7 across IC1B's inputs and the Bass pot's place read from the Sapphire; `fat`'s lift given across the tone pot; "hardware fully up" is +20.7 dB over the netlist's level position (the +5.6 dB written was a level, not a difference) | Read on the schematics; recomputed |
| 10 | Pair-port match stated as measured: harmonics 2 – 5 within 0.1 dB, 2 – 9 within 0.35 dB down to −60 dBc, 0.8 dB on one 8th at −62 dBc; the bench row takes the −60 dBc floor, 1 V and 4 kHz | "Every harmonic within 0.1 dB" held for 2 – 5 only |
| 11 | CPU text: the 5.0 µs is an extrapolation, to measure before deciding; 4× confirmed out (−60.7 dB) | Three points of a line; the loaded machine is the bound that matters |

**Verified with numbers** (prototype, generic part constants):
- Netlists against the documents, part by part: Aion *Sapphire* v1.0.1 p. 9, Aion *Andromeda* v1.1.0
  p. 7, PedalPCB *Ultisol Distortion* p. 4 (the gain pot's wiring — wiper on IC1.1's output, 1 nF across
  the track, the far end into IC2.1 — read on an enlargement). PedalPCB's *Cobalt Drive* lists the same
  two 18 nF (C14, C15); its schematic was not re-read. No error in any of the three netlists.
- Discrete OD: stage 1 +21.9 / +27.6 / +41.7 dB, gyrator lift +5.8 dB at 120 Hz, DC point (outputs
  4.11 V, bases 7.45 V, Q8 3.69 / 3.08 V): as written. Stage 2, transistor level vs pair port: table of
  §2.11 reproduced, lagged row added.
- Studio OD: pre-clip peak 2.8 kHz, 11.0 dB over 100 Hz; 45.0 dB at 2 kHz; Spectrum curves; IC2A ≤ 0.99 V;
  IC2B asked 3.5 / 4.5 V of 3.3 V; alias at 4× −77.0 dB (Spectrum 100 %), −81.2 dB (0 %): as written.
- Plexi Dist: 67.6 dB at 1 – 1.5 kHz, 28 dB at 50 Hz, the bright cap's 15 → 30 dB; Contour −11.7 / −7.2 /
  −2.5 dB; IC2.2 asked 5.1 V of 2.85 V; alias −49.7 / −71.3 dB at 4× / 8×: as written.
- Limit found: a full-scale signal with `input` at +12 dB takes the pair port 2.1 dB off the
  transistor level on the 2nd harmonic (stage 1's JFET leaves saturation); inside a pickup's range
  (0 dBFS) they agree.

Checked and left as written: the three source documents; the pair port + PNP and its 3 ports; the JFET
device for the reference only; the Plexi's first two op-amps as one stage (the 1 nF across the pot does
couple them); the Studio OD at 4×; Spectrum as `tone_stack`, 0 % = dark; no "preamp" id; `fat` and
`contour` as one-circuit ids, inert elsewhere, hidden by `depends`; ids `discrete-od`, `studio-od`,
`plexi-dist` appended in that order; the face unchanged; the presets (one flagged); the `bass_keep`
twins and their polarities; `supply` 200 % as the Plexi's 18 V in principle.

Still open:

| Point | Owner |
|---|---|
| The Plexi Dist against the alias policy: the Contour's scoop (−66.6 dB) and 18 V (−60.5 dB) — open question 11. **Decided by Raph, 2026-11-26:** the scoop is a setting (−65 dB passes); `supply` capped at 150 % on the Plexi (−65.9 dB); "18 Volt Plexi" dropped | Decided |
| CPU of the Discrete OD and the Plexi Dist once measured (estimate ≈ 24 % of a core against the contract's ≈ 20 %). **Decided by Raph, 2026-11-26:** ≈ 24 % accepted if the coded measurement is ≤ 5 µs per sample | golem-fx (the measurement) |
| "Plexi" on the selector. **Decided by Raph, 2026-11-26:** the three names as the review settled (with `fat` a switch, `contour` expert-only, the Red Rox source), confirmed by ear at coding | Decided |
| `contour`'s default (70 %) by ear, before K is measured and before it ships. **Decided by Raph, 2026-11-26:** 70 %, as the review settled; confirmed by ear at coding | golem-fx at coding |
| The three aliases, the lag row and the Newton counts on the coded solver with the real decimator; alias at `supply` 50 / 200 % for all seven circuits (the Plexi at its 150 % cap) | golem-fx at coding |
| Part constants from SPICE models (2SK184-GR / 2SK209-GR, 2SA1335, the Plexi's germanium diode, 1N4001, LM833 and 4558 swing on 9 V); the Studio OD's Q1 (gate at 0 V, self-biased: does it cut off on a hot pickup's negative peaks?) checked once with the new JFET device in the reference | golem-fx at coding |
| The pair port's DC fixed point and latch in `circuit_nl`; gfx-fuzz renders bit-identical after the additions | golem-fx at coding |
| The Keeley "Phat" value (33 nF + 68 nF) has no maker's document; a second source for the Red Rox; a capture of each pedal for the listening row | The maintainer (sources, `pedal-nam`) |
| Catalogue: `circuit` gains its three options **in the same release as the VST3**, with the REAPER read-back check; `fat` and `contour` entries; a Plexi archetype that shows `contour`, `fat` on the `blues-driver` archetype | golem-rack |
