# gfx-tape

> **Status: CODED** (v0.1.0, 2026-11-15). Designed and reviewed 2026-11-14 (Fable 5.2, Review section at
> the end). The chain runs as designed for all eight models (Studio Reel, Home Reel, Cassette, Portable
> Cassette, Microcassette, VHS, Vinyl, Shellac 78; AM Radio is gfx-radio's and never enters the enum):
> Jiles-Atherton integrated along H with the substep rule n = min(8, ⌈|Δh| / k⌉), c = 0.9 at 100 % bias,
> relaxation toward the anhysteretic, the per-speed record drive-boost table and its exact inverse, at
> ≥ 352.8 kHz (8× at 44.1 / 48 kHz) through minimum-phase half-bands; the transport generated as speed
> (two passes of each rotating part, drift, random wow and flutter, snags, stop / spin-up) read with
> Lagrange-5 around a slewed D0; spacing-loss dropouts, crinkle, crackle and scratches, modulation noise,
> hiss / mechanical noise in Follow / Constant / Duck, the compander, the ALC, azimuth, and the `wf_out`
> meter. New gfx-dsp module `tape` (`Hysteresis`, `TapeSat` for gfx-grit, `SpacingFilter`, `Shelf1`,
> `BandNoise`, `Wobble`, the static makeup table). Bench 28 / 28; 22 behaviour tests plus 6 unit tests here
> and 10 in gfx-dsp. Low levels pass at unity on every model (≤ 0.27 dB at 1 kHz), the level is held over
> 36 dB of `input` (≤ 0.06 dB, Shellac 0.64 dB), aliasing ≤ −79.7 dB (Studio at +12 / +18, Cassette at
> +12). CPU 1.0 µs / sample mono at the defaults (render timing; the bench's line read 1.4 µs on a loaded
> machine), 1.6 µs worst case: over the 1.1 µs budget, see "As coded". **The `wf_out` weighting is
> fitted to the reviewer's recalled table [U]: it must be checked against AES6-2008 before the meter is
> trusted.** Deviations: "As coded" below.

## As coded (v0.1.0)

**Files.** gfx-dsp `tape.rs` (`Hysteresis`, `TapeSat`, `Shelf1`, `record_shelf`, `level_loss_db`,
`SpacingFilter` and `matched_lowpass1`, `BandNoise`, `Wobble`) and `Oversampler2x::copy_from` (an addition:
a linked stereo pair splits without allocating); here `lib.rs` (the processor), `models.rs` (the model
tables, the drive-boost and replay-τ tables), `transport.rs` (ε generator, snags, the stop machine, the
weighting meter), `medium.rs` (dropout / crinkle lanes, crackle and scratches, hiss / mechanical /
modulation noise, the Follow law, ALC, compander, the disc core), `plugin.rs` (class id
`GoleMFxTape00001`, mono 1 / 1 then 2 / 2, latency 0, `bypass` as the VST3 bypass, `wf_out` read-only),
`tests.rs`. The model, transport components, noise levels and wear profile values are the README's
[I] starting points; where it gave a range the middle was taken, where it gave none the value is marked
[I] in `models.rs` (Home / Portable / Micro / VHS flutter extras, mechanical-noise spectra and levels,
static spacing 0 µm: the rated band is the aligned machine).

**Measured** (tests and `gfx-bench check gfx-tape`, 48 kHz unless stated):

| What | Target (§5) | Measured |
|---|---|---|
| J-A, sines 50 Hz – 15 kHz up to 4 a (384 kHz) | ≤ 0.5 % of M_s vs a 64-substep reference; ≤ 1.3 substeps to 10 kHz | ≤ 0.5 % (asserted); ≤ 1.3 substeps; 1.000 on a played signal at `input` +12 |
| J-A, ±8 a square, Nyquist alternation, white noise | no NaN, no reset, \|M\| ≤ M_s | none, none, ≤ M_s at c = 0.3 / 0.9 / 0.98; error vs reference 1.2 – 2.7 % of M_s (the Review's 0.5 % is not reproduced, see 2.) |
| Small signal (−30 dB re ref, 1 kHz) | ±1 dB per model | Studio −0.01, Home 0.00, Cassette +0.11, Portable +0.11, Micro +0.19, VHS +0.10, Vinyl +0.13, Shellac +0.27 dB; 30 ips flat ±1 dB 200 Hz – 16 kHz; record × playback shelf null < 1e−12 |
| No expander; THD at −20 dB re a | gain −40 dB → ref within 1 dB; ≈ 0.05 % | within 1 dB; THD at −20 dB re a < 0.2 % (asserted; 0.038 % on Home Reel at −20 dB re ref); unbiased c = 0.17: below −10 dB and above 1 % THD (asserted), as the Review measured |
| Under-bias | THD at −20 dB re ref at 60 % ≥ 3× that at 100 % | 0.260 % vs 0.038 % (6.8×), Home Reel |
| MOL (3 % THD, 1 kHz) | reels +10 … +14, Cassette +6 … +8 dB re ref | Studio +14.0, Home +11.3, Cassette +8.0, VHS +9.0; Portable +13.9 and Micro +12.8 with their ALC holding the level |
| HF compression (1 dB points) | 10 kHz ≥ 10 dB below 1 kHz at 1⅞ ips, ≤ 4 dB at 30 | 1⅞: +8.2 / −2.2 dB (10.4 dB); 30 ips: +9.8 / +8.2 dB (1.6 dB) |
| Relaxation | < −140 dBFS within 1 s after a +12 dB note | yes (the bench's silence floor reads −240 dBFS) |
| Aliasing, 1 – 12 kHz, `input` +12 / +18 | ≤ −76 dB | Studio ≤ −80.0 dB; Cassette at +12 ≤ −79.7 dB; Cassette at +18: −68.4 dB at 9.2 kHz, −51.5 dB at 12 kHz (see 3.) |
| Head bump, Studio 7½ / 15 / 30 ips | f_b, 2 f_b, 4 f_b ± 5 % | 19.9 / 39.0 / 78.0 Hz (f_b 19.05 / 38.1 / 76.2: the bell rides the contour's high-pass, +2 – 4 %), ratios 1.96 / 2.00 |
| Transport | knob = unweighted peak ± 10 % | periodic part over 20 s: 0.77 (Studio), 0.74 (Cassette), 0.67 (Vinyl) of the knob (see 7.); `spread` 100 random parts correlation ≤ 0.15; meter on a 4 Hz deviation within 3 %; weighting fit within 0.3 dB of the [U] table |
| Zero transport, D0 | 0 at wow = flutter = snags = 0; never < 1 sample; slews | exactly 0, line bypassed; D0 0.83 ms at the defaults, 40 ms cap at wow 4 % / rate 25 %; rise ≤ 20 ms/s, fall 1 ms/s; read head ≥ 3 samples |
| Snags | excursion ≤ 6 ms, ∫ε = 0 | ≤ 5.04 ms over 60 s, returns to 0 after each event |
| Stop | 0 in `stop_time` ± 5 %; spin-up, no click | silent 0.40 s after a 400 ms stop (± 30 ms asserted); release burst above 10 kHz < −40 dB re the tone; level back within 1 dB after ¼ `stop_time` + 30 ms |
| Hiss S/N per model, Constant | ± 1 dB | every model within 0.1 dB of §2.1; `input` −12 vs +24 moves it < 0.5 dB |
| Follow / Duck | exactly 0 within 750 ms; Duck −20 dB | additive noise 0 from 0.725 s (the whole output < −140 dBFS at 0.724 s); Duck −20 ± 1.5 dB |
| Modulation noise | model level ± 2 dB; absent in silence | Studio −54.9 dB (−55), Micro −36.7 dB (−35); exactly 0 in silence |
| Compander | unity ± 0.5 dB; hiss pushed down | encode → decode exact to 1e−9 (unit test); on tape within 0.02 dB of NR off at −30 / −10 / 0 dB re ref; Cassette hiss in silence −68 → −117 dBFS |
| Mix 50 %, wow 0, `input` −12 | ± 1 dB of the wet, 40 Hz – 16 kHz | within 0.01 dB from 1 to 16 kHz, 0.16 dB at 100 Hz; −1.1 dB at 40 Hz (see 9.) |
| Level, −18 dBFS sine, `input` −12 → +24 | ± 0.5 dB | ≤ 0.06 dB on every tape model and Vinyl; Shellac −0.64 dB at +24 |
| Rates 44.1 / 48 / 96 / 192 kHz | ± 0.5 dB | 100 Hz, 1 kHz, 8 kHz within 0.5 dB |
| Dropouts | rate ± 20 % over 60 s; synced on the grid | 153 events for 180 expected (−15 %); `drop_sync` 1/8 at 120 BPM: one per step; the same 3 µm event 30 dB deeper at 10 kHz at 1⅞ than at 15 ips |
| Vinyl tracing | even order, rising with f and inward | H2 −51.3 dB at 500 Hz, −35.6 dB at 2 kHz, −36.0 dB at 500 Hz on the inner groove |
| Stereo | `spread` 0 one transport; mono input on 2 / 2 bit-exact | L = R bit-exact (computed once); a later real stereo input takes over the left's state |
| Bench | — | 28 / 28 at 44.1 – 192 kHz; worst parameter step −46 dB (bound −40) |
| CPU (48 kHz) | ≤ 1.1 µs mono at defaults, ≤ 2.2 µs worst | 1.0 µs (20 s render), 1.4 – 1.5 µs in the bench's line on a machine at load 7 – 10; worst (Cassette, +18, compander, failures) 1.6 µs |
| Presets | ± 2 LU of bypass | 19 presets on clean.wav (100 Hz high-passed RMS): ±0.4 dB at `level` 0, `level` set on the four that need it |

**Deviations from the spec, and why.**

1. **Spacing sections are magnitude-matched one-poles** (DC, Nyquist and min(corner, fs / 4) matched to
   the analog section, closed form), not TPT sections: a bilinear corner cannot sit above Nyquist, and at
   15 ips a 1 – 2 µm spacing puts both corners past 24 kHz, where TPT sections give no loss at all (−1.7 dB
   missing at 12 kHz). Measured within 1.5 dB of 54.6 · d · f / v below 0.3 fs, within 2.5 dB above
   (1.8 dB at 16 kHz / 48 kHz for 1 µm at 1⅞). Still first order, one state, modulation-safe; d = 0 exact.
2. **J-A worst cases**: no NaN, no reset and |M| ≤ M_s hold, but the error against the 64-substep
   reference on a ±8 a square / Nyquist alternation is 1.2 – 2.7 % of M_s (the Review measured 0.5 % at
   c = 0.17 and without the soft clip of H). Sines stay within 0.5 %. The bench asks stability here.
3. **Cassette at `input` +18 aliases above 9 kHz** (−68 dB at 9.2 kHz, −51 dB at 12 kHz): the 12 dB drive
   boost puts a 12 kHz sine at +27 dB re a, deep into the ±8 a clip of H. Studio and Cassette at +12 meet
   −76 dB. The test reports this case without bounding it.
4. **CPU**: the core is latency-bound (each RK4 step is four dependent g evaluations, an exp and two
   divisions each: ≈ 146 ns per internal sample), so 8× costs ≈ 1.0 – 1.4 µs mono at the defaults against
   the 1.1 µs estimate; the worst case meets 2.2 µs. Already done: one division per g, no transcendental
   per sample outside the core. The J-A lanes are scalar, not SIMD: a real stereo input costs ≈ 2× mono (a
   mono input on 2 / 2 is computed once). The owner's call (Review "Still open"): accept, or 4×.
5. **The playback inverse shelf sits inside `TapeSat`**, right after the core (the chain is linear from
   there, so only the order of LTI stages changes); the hiss is shaped to its final spectrum directly (the
   replay boost: a zero at 1 / 2πτ, a pole at 20 kHz) and added after it, before spacing and band edges.
6. **Additive noises are output-referred**: injected scaled by the inverse of the output compensation, so
   their level is the model's S/N re the −18 dBFS reference whatever `input` is (the Noise row's "moving
   `input` does not move the noise"), and the compander still expands them.
7. **The transport knobs bound the peak**: each rotating part is two passes (record, playback) of
   amplitude ½ with independent diffusing phases and ±25 % amplitude walks, so the periodic part peaks at
   0.67 – 0.77 of the knob over 20 s, not ±10 %; random parts are scaled by 3σ = knob and mixed with the
   periodic ones as √((1 − ρ)² + ρ²). The random processes are designed once at their nominal frequencies
   and stepped at a variable clock (`wow_rate` × speed ratio, clamped 0.125 – 8×), so no redesign happens
   in `process`.
8. **The dry runs the wet's whole half-band cascade** (each stage at its own rate, nothing between), not
   one `Oversampler2x::identity` per stage: an identity at fs is not the stage at 2 fs or 4 fs, and the
   per-stage version measured −11.3 dB at 5 kHz at `mix` 50 %. Now within 0.01 dB from 1 to 16 kHz.
9. **Mix 50 % at 40 Hz is −1.1 dB from the in-phase sum** on Studio Reel: the head bump and its contour
   high-pass are minimum-phase filters of the wet that the dry does not share. From 100 Hz up it is within
   0.2 dB.
10. **ALC sits before `input`** and holds the instrument's peaks at −6 dB re the reference sine's peak, so
    the static compensation still holds across `input`; MOL then reads higher on the ALC models.
11. **Shellac's static makeup leaves out the tracing term**: −0.64 dB at `input` +24 (bound ±0.5).
12. **Azimuth's within-track loss** is folded into the spacing filter as d = 0.182 · w · tan θ (the sinc
    and the spacing law matched at their 6 dB point); the stereo delay s · tan θ / v is exact.
13. **Band edges**: a 4th-order Butterworth low-pass at the rated HF edge (scaled with the speed, dropped
    above 0.45 fs), a 2nd-order high-pass at the rated LF edge on the models that list one; the bump
    models (Studio, Home) take their low end from the contour only. Gap loss is not modelled apart.
14. **Smaller choices**: dropout pool of 6 simultaneous events; crackle and scratches shared by both
    channels (one stylus); `bias`'s HF sensitivity as a 5 kHz high shelf (+1 / −2 dB); the cuts engage
    through a 5 ms crossfade (a switched 4th-order filter stepped −24 dB at 192 kHz); `stop` released
    during the wind-down fades the old head out in 10 ms before the spin-up; a test-only `no_mod_noise`
    switch (the modulation noise is never switched in use).

**Left for the owner.** Voicing by ear (c(bias), the drive-boost table, MOL per model, hiss and
mechanical spectra, crackle levels, all [I]); reference recordings per model; **the IEC 386 / AES6
weighting table against AES6-2008 before `wf_out` is trusted**; the CPU (accept ≈ 1.0 – 1.4 µs, or 4×);
the champagne body, rune light and `ribbon` motif at tile size by eye; golem-rack: type `tape` in
`lofi`, `role_blocks.drums`, EXP on `spacing`, `bias`, `dropouts`, `wf_out` display, D0 against parallel
lanes (0.83 ms at the defaults).

golem's **tape and lo-fi media pedal**: the sound of a signal that has been recorded onto something and
played back. A tape machine (record EQ, hysteresis with bias, head bump, alignment and its limits, speed
1⅞ – 30 ips), its transport (wow and flutter with the spectra real machines measure, not one sine), its
faults (dropouts, crinkles, snags, tape stop), its noise (hiss, modulation noise, mechanical noise) and the
other media built on the same chain: cassette, dictation microcassette, VHS linear track, vinyl and 78
shellac. It plays the role of a tape pedal (Generation Loss, Deco's saturation half) and of the studio tape
plugins (ChowTape, J37, Wow Control, RC-20, iZotope Vinyl).

It is **not an echo**: there is no repeat and no feedback (that is gfx-delay's Tape model), not a chorus
(gfx-chorus's Tape engine), not a converter (gfx-crusher), not an analog clipper (gfx-drive). It is a new
golem block with no catalogue predecessor: type **`tape`**, category **`lofi`** (§3). Mono 1 / 1 first,
2 / 2 offered (§2.7). **Reported latency 0**; the wet carries a small, setting-dependent transport delay
(≈ 1.5 ms at the defaults, §2.3) and a few samples of minimum-phase group delay, as gfx-vibrato and
gfx-drive do.

## 1. State of the art

Survey of 2026-11-14. **[V]** read in a source, a manual or the code · **[I]** inference, or a fact not
re-checked against a primary source · **[U]** unverified (forum, vendor claim) · **[A]** our calculation.

### Physics in five lines

1. **Recording is nonlinear, playback is linear.** The record head's field magnetises the tape through a
   hysteresis loop; AC bias (50 – 150 kHz, ~10× the signal) linearises it. What remains — soft
   compression, odd harmonics, highs saturating first at low speeds — is the "tape sound" [V, Chowdhury
   2019 §3.3; Jiles & Atherton 1986].
2. **Everything else is wavelength.** A frequency f is recorded as a wavelength λ = v / f. Spacing, gap,
   azimuth and contour effects depend on λ, so they move by one octave per doubling of the speed [V,
   Chowdhury eq. 13 – 14; [Richard Hess](https://richardhess.com/notes/2007/08/26/tape-speed-track-width-frequency-response-and-dynamic-range/)].
3. **Static losses are aligned out.** A real machine's record / playback EQ is aligned so the small-signal
   response is flat within its rated band; the residual is the band's edge and the head bump. What a
   listener hears change is what moves **after** alignment: spacing (dropouts, wear, dirty heads), azimuth,
   bias, level [A].
4. **The transport is shared.** Wow and flutter are speed deviations of the whole tape (both channels,
   record and playback), made of a few rotating parts' periodic components plus random motion [V,
   Arnardottir, Abel & Smith AES 2008; Chowdhury §3.4].
5. **Noise has two kinds**: hiss (additive, constant) and **modulation noise** (multiplicative, it exists
   only under the signal) [I, standard tape theory].

### Papers

- Chowdhury, *Real-time Physical Modelling for Analog Tape Machines*, DAFx 2019
  ([pdf](https://ccrma.stanford.edu/~jatin/420/tape/TapeModel_DAFx.pdf)) [V]: Jiles-Atherton (J-A)
  equation (18) solved with RK4 on a trapezoidal derivative of H; small-|Q| approximations of the
  Langevin function; tanh by a Gauss continued fraction; playback losses V = V₀ · e^(−kd) ·
  (1 − e^(−kδ)) / (kδ) · sin(kg/2) / (kg/2), k = 2πf / v; ferric parameters M_s 3.5e5 A/m, a 22 kA/m,
  k 27 kA/m, c 0.17, α 1.6e−3; a **real 55 kHz bias tone**, hence **16× oversampling**; wow / flutter
  measured on a Sony TC-260 with a pulse train. Its own verdict: convincing, "not nearly close enough to
  fool the listener".
- Jiles & Atherton, *Theory of ferromagnetic hysteresis*, J. Magn. Magn. Mater. 1986 [I, cited by
  Chowdhury]: M = M_rev + M_irr, anhysteretic M_an = M_s · L((H + αM) / a); the model is
  **rate-independent** (M depends on the path of H, not on time) [V, from eq. 18: dM/dt = Ḣ · g(H, M, sign Ḣ)].
- Arnardottir, Abel & Smith, *A Digital Model of the Echoplex Tape Delay*, AES 125, 2008 [I, read for
  gfx-delay]: wow / flutter as a few quasi-periodic components plus drift.
- Holters & Zölzer, *Circuit simulation with inductors and transformers based on the J-A model*, DAFx 2016
  [I]: the J-A numerics Chowdhury builds on.
- Wow and flutter measurement: IEC 386 = DIN 45507 = CCIR 409-3 = AES6-2008, one weighting curve peaking
  at **4 Hz** (≈ 6 dB / octave each side, steeper below 0.5 Hz), 3.15 kHz test tone, quasi-peak detector;
  JIS / NAB quote WRMS [V, [Wikipedia](https://en.wikipedia.org/wiki/Wow_and_flutter_measurement), slope from
  an AES summary; **the exact table [U]**, AES6 not read]. Typical weighted figures: professional machines
  ≈ 0.02 %, good cassette decks ≈ 0.08 % [V, same page]; personal cassette players 0.2 – 0.4 % [I].
- Replay EQ time constants [V, [IASA 5.4.10](https://iasa-web.org/node/490),
  [reeltoreel.nl](https://www.reeltoreel.nl/wiki/index.php/NAB_vs._IEC)]: NAB 3180 + 50 µs (7½ and 15 ips);
  IEC 35 µs (15), 70 µs (7½); AES 17.5 µs (30); cassette 3180 + 120 µs (Type I), 70 µs (Type II); 3¾ ips
  3180 + 90 µs [I]. Vinyl: RIAA 3180 / 318 / 75 µs [I, standard]. **Direction** (review): on tape these
  constants describe the **recorded flux**, which *falls* above 1 / (2πτ) for a constant input; the replay
  head differentiates the flux, the replay amplifier integrates and **boosts** above the corner by
  √(1 + (2πfτ)²): +17.6 dB at 10 kHz for 120 µs, +13.1 dB for 70 µs, +7.7 dB for 35 µs, +3.4 dB for
  17.5 µs [A]. That boost is why 70 µs is 4.5 dB quieter than 120 µs [I, standard theory; head = flux
  derivative V in Nakamichi's Technical Bulletin 2]. Only vinyl pre-emphasises on record and cuts on replay.
- Head bump: measured ≈ 35 – 40 Hz at 15 ips on two machines, "everything below 70 Hz rolled off" at
  30 ips [V, [Endino](https://www.endino.com/graphs/)]; another source gives +1 dB at 63 Hz at 7½ ips [V,
  quoted in search, head-dependent].

### Open source

| Project (licence) | Read | What it does | What we take / do better |
|---|---|---|---|
| [ChowTape / AnalogTapeModel](https://github.com/jatinchowdhury18/AnalogTapeModel) (GPL-3) | [V] code | **Hysteresis**: J-A in normalised units (M_s = 0.5 + 1.5·(1 − sat), a = M_s / (0.01 + 6·drive), c = √(1 − width) − 0.01, k = 0.479, α = 1.6e−3); Ḣ by an α-transform (α = 0.75); solvers RK2, RK4, NR4, NR8 and a neural **STN**; input clipped per solver "to avoid unstable hysteresis"; **"Tape Bias" is the J-A `c`, no bias tone** (v1's tone sat at the oversampled Nyquist). Variable 1 – 16× oversampling, linear or minimum phase, latency reported. **Loss**: linear-phase FIR from spacing / thickness / gap (latency = order / 2); head bump = one peak filter at v / (500·gap), ≤ +3.5 dB. **Wow**: a sine + an Ornstein-Uhlenbeck process + a frequency drift drawn **once per block** (block-size dependent). **Flutter**: 3 harmonics of one rate. Wow and flutter modulate **delay** in samples (pitch deviation ∝ rate). **Chew / dropout**: \|x\|^p mixed in over random windows. **Azimuth**: delay = width · sin θ · **v** · fs | J-A as the core (§2.2); **integrate in H, not in time** (no Ḣ estimate; stable at Nyquist where the time form diverges, §2.2 [P]); bias with its HF cost; zero-latency losses; pitch-domain transport with multi-component spectra; seeded, block-size-independent randomness. Its azimuth formula multiplies by the speed where physics divides (delay = s·tan θ / v) [A] |
| [Airwindows ToTape8](https://github.com/airwindows/airwindows) (MIT) | [V] code | "Dubly" encode / decode around the tape (a µ-law-shaped HF compander); flutter as a 1,000-sample delay swept at a random speed per cycle, **L and R drawn separately but each pulled toward the other's phase**; "bias" as a chain of slew limiters (over) and sticky hold (under); lows → sin() saturation, highs → 1 − cos thinning (split-band); head bump = cubic-limited resonator + two biquads; ClipOnly2 | A compander as a real noise-reduction layer (§2.6); split-band insight confirmed by physics (pre-emphasis makes highs saturate first, §2.2); the near-linked stereo flutter becomes our `spread` |
| Airwindows IronOxide5, ToTape6 (MIT) | [I] | Earlier, cheaper variants | — |
| gfx-delay Tape model (ours) | [V] code | In the loop: model low-pass 7 kHz · (1 − 0.6·age), +2.5 dB bell at 90 Hz, tanh (ADAA) ×1.2, dropouts as level dips (seeded), mod = 0.35·(1 + 2·age)·sin(0.55 Hz) + 0.03·sin(6.8 Hz) + 0.15·drift **ms of delay** | A loop colour; right for an echo. gfx-tape is the full machine; its `tape` module can later replace these few lines (§2.9) |
| gfx-chorus Tape engine (ours) | [V] code | Voice delay + 0.5·wow·sin(0.5 Hz) + 0.05·flutter·sin(7.3 Hz) ms, hiss gated by the playing | Same |

### Commercial references

| Reference | What defines it | What we take |
|---|---|---|
| [Chase Bliss Generation Loss MKII](https://www.chasebliss.com/generation-loss-mkii) ([manual](https://static1.squarespace.com/static/622176a9b8d15d57ffbf5700/t/630f944dc69253001929210d/1661965406555/Generation+Loss+MKII_Manual_Pedal_Chase+Bliss.pdf)) [V] | Knobs Wow ("slow, smooth, random"), Flutter ("fast, twitchy, random", **pitch and amplitude**), Saturate (hysteresis), Failure (**drops**: brief silences, the head losing contact; **snags**: abrupt pitch spikes, tape stuck; **crinkles and pops**), Model (12 EQ profiles sampled from VCRs, cassette 4-tracks, a camcorder, dictation recorders, a toy, a Walkman-type player, a brittle ¼" reel; "copy of a copy" generations), Volume. Dry: none / small / unity ("unity" = chorus). Noise toggle = hiss + mechanical noise (levels in hidden options). Aux: tape **Stop**, Filter bypass, Fail (all failures max). **Spread**: failures differ L / R. "Around 10 o'clock is the sweet spot; higher settings evoke struggling machinery and melted tape" | Wow / flutter as **random** processes; failures as separate events; snags; tape stop; noise as a toggle on the face; models = media profiles; a stereo spread of failures |
| [Strymon Deco](https://strymon.net/deco) [V, vendor and retailer pages] | Tape saturation (from transparent to "smooth overdriven"), Volume, Tone (V2); Doubletracker: Lag Time (flange → chorus → slap → 500 ms echo), Blend, Wobble; Sum / Invert / Bounce | The saturation half; the doubletracker is gfx-delay's / gfx-chorus's |
| [Goodhertz Wow Control](https://manuals.goodhertz.com/3.7/wow-ctrl/) [V] | Amount 0 – 100 % (default 15 %), Flutter Amount (0 % = smooth wow), phase offset for synced use; **analog noise "modulated by the signal"**, part of each Analog Mode | Signal-modulated noise (our modulation noise); wow / flutter balance |
| [XLN RC-20](https://xlnaudio.com/products/effect/rc-20_retro_color) [V, reviews] | Six modules (Noise, Wobble, Distort, Digital, Space, Magnetic); each with **Flux** (random variation); Magnetic = tape **volume drops**; v2: noise can **follow the track or duck** under it [V, release notes] | Noise modes Follow / Duck / Constant |
| [iZotope Vinyl](https://www.kvraudio.com/product/vinyl-by-izotope/details) (free) [V, secondary] | Mechanical noise (rumble), electrical noise (hum), Wear, Dust, Scratch, Warp, **Year** (player era as filtering, 1930 → today), RPM 33 / 45 / 78, Spin Down | The vinyl and shellac models; crackle; spin-down = our `stop` |
| [Waves J37](https://images.equipboard.com/uploads/item/manual/15102/waves-j37-tape-manual.pdf) [V, manual snippets] | Three tape formulas, speed, **bias**, saturation, noise, wow rate / depth (default = the measured machine), flutter rate / depth | Formula, bias, wow rate relative to the machine |
| Waves Kramer Master Tape, Softube Tape, UAD ATR-102 / A800 [I] | Speed 7½ / 15 / 30, bias, flux / formula, wow / flutter / noise | The studio-reel model and its speeds |

### Forum feedback: what players love and hate

- ChowTape [V, [KVR thread](https://www.kvraudio.com/forum/viewtopic.php?p=7816839)]: some users prefer
  the original (heavier) hysteresis sound to the cheaper later one; the STN mode is "~3× less CPU than NR8"
  but "distorts less gracefully at extreme settings"; the hysteresis "generates some low-frequency noise,
  not just DC, between 0 and 30 Hz"; the hysteresis's group delay was not fully compensated at first.
- Generation Loss [V, manual]: wow is best subtle; flutter "turned down just to where you stop noticing it"
  is the realistic setting. Players use it as an overdrive, an EQ and a chorus (dry at unity).
- Hiss in silence before a high-gain amp is the classic complaint about noisy pedals [U, the crusher's
  sources on fizz]; lo-fi producers want the opposite (crackle in the gaps) [V, RC-20 v2 adding Duck].
- "Seasick" wow from a single sine: periodic warble reads as a vibrato, not a machine [U, common forum
  wisdom; consistent with Gen Loss and Wow Control describing wow as random].
- Level-dependence: a tape plugin only "works" when hit at the right level [U]. Handled by `input` with
  compensated output and a stated reference level (§2.2).

### What we conclude

1. **One chain, many media.** Record EQ → nonlinear medium → transport → medium faults and noise →
   playback losses and EQ. A model is a parameter set on that chain, not a separate algorithm.
2. **J-A hysteresis, integrated in H** (rate-independent form), 8× oversampled with minimum-phase
   half-bands; **bias modelled by its effects** (loop width, relaxation to the anhysteretic, HF
   erasure), never as a tone — the tone is what forced 16×.
3. **Static losses aligned out, dynamic losses physical.** A spacing-loss filter family (dB ∝ f · d / v)
   serves dirty heads, wear and **dropouts**: highs vanish first, and the same debris is mild at 15 ips and
   brutal at 1⅞ — dropouts that sound like tape, not like a volume knob.
4. **Pitch-domain transport**: wow / flutter generated as speed deviation ε(t) with each model's
   component frequencies (capstan, pinch roller, reels, platter) plus random motion; the delay is its
   integral (gfx-vibrato's method). Linked across channels: a transport has one speed.
5. **Noise that follows the playing — physically**: modulation noise is multiplicative (silence stays
   silent by nature); hiss, mechanical noise and crackle get a mode: Follow (default), Constant, Duck.
6. **Real units where the field has them**: wow and flutter in % of speed (presets cite machine specs), a
   read-only weighted W&F meter, record level in dB re the model's reference, spacing in µm, azimuth in
   arc-minutes, speed in ips.

## 2. Design

```
in ─┬──────────────────────────────────────── dry (D0 delay + the same half-bands, §2.8) ────────────┐
    └▶ × input ▶ [ALC] ▶ [NR encode] ▶ RECORD EQ ─▶ 8× ▶ J-A HYSTERESIS (bias) ▶ ↓8× ─▶ DC / 10 Hz HP   │
                                                                                         │           │
       TRANSPORT ε(t) = wow + flutter + snags + stop  ──▶  read head: D(t) = D0 − ∫ε dt ◀──┘           │
                                                                    │                                │
       + modulation noise (× signal) · + hiss, mechanical, crackle (Follow / Constant / Duck)        │
       ▶ SPACING (static + wear + dropouts + crinkle) ▶ azimuth ▶ PLAYBACK EQ + alignment + head bump  │
       ▶ [NR decode] ▶ low_cut / high_cut ▶ × level × 1/input ─────────────────▶ mix (crossfade) ─────┴▶ out
```

### 2.1 Models

A model = medium + machine: nonlinearity, EQ standard per speed, rated band (alignment), head-bump length,
transport components, noise levels, ALC, default share of random wow. Every value below is a voicing
starting point **[I]** unless marked, to be tuned against references (§7).

| `model` | Nonlinearity | Speed (Model) | EQ at that speed | Rated band (−3 dB) | Bump | Transport components (ε) [A from the dimensions, dimensions I] | S/N (hiss, re ref.) | Extras |
|---|---|---|---|---|---|---|---|---|
| **Studio Reel** (default) | J-A, "hot" formula | 15 ips | IEC 35 µs | 30 Hz – 22 kHz | L_eff 10 mm → **38 Hz** at 15 ips [A, Endino] | capstan Ø 25 mm → 4.8 Hz; pinch roller Ø 38 mm → 3.2 Hz; reels (r 50 – 130 mm) 0.47 – 1.2 Hz; idler 15 – 30 Hz; scrape flutter 2 – 5 kHz | −68 dB | — |
| **Home Reel** (consumer ¼" quarter-track) | J-A, ferric | 7½ ips | NAB 3180 + 50 µs | 40 Hz – 16 kHz | 8 mm → 24 Hz | belt-driven capstan Ø 6 mm → 10 Hz; flywheel 1 – 2 Hz; reels 0.3 – 0.8 Hz | −58 dB | more random wow |
| **Cassette** (Type I deck) | J-A, ferric, low MOL at HF | 1⅞ ips | 3180 + 120 µs | 40 Hz – 14 kHz | 4 mm → 12 Hz (masked by the LF roll-off) | capstan Ø 2.4 mm → 6.3 Hz; pinch Ø 10 mm → 1.5 Hz; hubs (r 11 – 27 mm) 0.28 – 0.69 Hz | −50 dB | — |
| **Portable Cassette** (personal player / recorder) | J-A, ferric | 1⅞ ips | 120 µs | 80 Hz – 10 kHz | — | as Cassette, plus motor cogging 20 – 40 Hz and battery-servo hunting 0.1 – 0.3 Hz; mostly random | −46 dB | ALC (attack 5 ms, release 1.5 s) |
| **Microcassette** (dictation) | J-A, narrow track | 2.4 cm/s | 120 µs | 300 Hz – 5 kHz | — | capstan → ~10 Hz; strong motor noise | −40 dB | ALC, mechanical noise |
| **VHS** (linear audio track, SP) | J-A, ferric | 33.35 mm/s | 3180 + 120 µs | 100 Hz – 10 kHz | — | capstan → ~2 Hz; tension servo 0.5 – 1 Hz | −42 dB | — |
| **Vinyl** (LP, 33⅓) | cutter / stylus: tracing distortion (even order, rising with frequency) [I] | — | RIAA | 30 Hz – 16 kHz | — | **eccentricity at 0.556 Hz** (ε = e / r: 0.2 mm off-centre = 0.14 % at the rim, 0.33 % at r = 60 mm [A]); warp 0.556 Hz + harmonics; belt motor ~5 Hz | −60 dB surface noise | crackle, rumble 10 – 40 Hz |
| **Shellac 78** | acoustic / early electric limits, heavy tracing | — | early 78 curve (≈ 500 Hz turnover, no treble roll-off) [I] | 100 Hz – 5 kHz | — | rotation 1.3 Hz | −35 dB | loud crackle |
| *AM Radio* (moved to gfx-radio, 2026-11-14: never appended here) | broadcast limiter + envelope detection | — | — | 100 Hz – 4.5 kHz | — | slow selective fading 0.05 – 0.5 Hz (level, not pitch) | static | heterodyne whistle |

- **Speed** (`speed`: Model · 1⅞ · 3¾ · 7½ · 15 · 30 ips) applies to every tape model: the physics scales
  (losses and bump move an octave per doubling, EQ standard follows the table, transport component
  frequencies scale with v). Cassette at 3¾ is the double-speed 4-track; Studio Reel at 30 is the AES
  mastering machine. Inert on Vinyl, Shellac and Radio (their rotation is part of the model).
- **Formula** (`formula`: Model · Ferric · High Output · Chrome): sets the J-A set (M_s, a, k) and, on
  cassette models, the 120 / 70 µs EQ. Values start from Chowdhury's ferric set rescaled to normalised
  units (ChowTape v2's ratios); High Output raises MOL ≈ +3 dB, Chrome lowers HF saturation [I].
- **AM Radio** is designed but deferred: it shares the chain's filters and noise but its nonlinearity is a
  broadcast limiter and an envelope detector, and its "wow" is fading. It enters the enum when coded.

### 2.2 Record side: EQ, hysteresis, bias

**Level.** `input` (dB) scales the signal so that **a −18 dBFS RMS sine at `input` 0 dB sits at the model's
reference fluxivity (0 VU)**. MOL (3 % THD at 1 kHz) is about +10 to +14 dB above reference on the reel
models, +6 to +8 on Cassette [I, voicing]. In the core's units, with c = 0.9 (below): 1 % THD at H ≈ −1.5 dB
re a, 3 % at +3.5 dB, 5.3 % at +6 dB [P] — so the Studio reference sits at H_ref ≈ a · 10^(−7.5 / 20) and
MOL − (1 % point) ≈ 5 dB, which is what the defaults assume. The output is compensated: the knob changes
how hard the tape is hit, not the volume (gfx-crusher's rule). Here −`input` alone is not enough, because
tape compresses where a converter clips: a reference sine loses 4 dB at +12 dB re a and 8 dB at +18 [P].
The compensation is **−`input` + m(`input`)**, m = the level a −18 dBFS sine loses in the core at that
setting, tabulated per model × formula at `prepare` (a static function of the knob: no detector, no auto
gain). Default **+6 dB**: guitar at −18 dBFS gets ≈ 1 % THD — glue, audible at once, not a fuzz.

**Record EQ** (host rate, linear) and its exact inverse after the core. What the core must see is the
**record head's field**, not the standard's flux curve (which falls at HF, §1): a record amplifier boosts
the highs to overcome the record-side losses (thickness, self-demagnetisation, bias erasure), all functions
of the wavelength, and that boost is why short wavelengths saturate first. It is a voicing table, one
first-order shelf per model × speed × formula, set from the tape's HF headroom deficit (3 % point at
10 kHz vs 1 kHz, output-referred) [I, starting points, to be tuned against references]:

| Speed | 1⅞ ips (Type I) | 1⅞ (Chrome, 70 µs) | 3¾ | 7½ | 15 | 30 |
|---|---|---|---|---|---|---|
| Drive boost at 10 kHz | +12 dB | +10 dB | +9 dB | +6 dB | +3.5 dB | +1.5 dB |

Highs saturate first; cymbals and pick attacks compress; that is cassette. (The first draft used the
standard's time constant itself — +17.6 / +7.7 / +3.5 dB — as a record pre-emphasis: right arithmetic,
wrong side of the machine; those figures are the replay boost and now shape the hiss, §2.4.)

**Hysteresis: Jiles-Atherton, integrated along H.** Eq. (18) is dM/dt = Ḣ · g(H, M, δ) with δ = sign Ḣ.
Since the model is rate-independent, we integrate **dM/dH = g(H, M, δ)** from H(n−1) to H(n), δ = sign of
the step (exact for the piecewise-linear path between samples; a reversal between two samples loses only
the tip of the peak):

- RK4 along the H path, in f64, with **substeps sized on k**, the model's own relaxation length in H:
  **n = min(8, ⌈|ΔH| / k⌉)**. The irreversible term is a relaxation of length k / (1 − c), so RK4 diverges
  beyond |ΔH| ≈ 2.8 k / (1 − c) per step; the clip at ±8 a allows steps of 16 a = 13 k (Chowdhury's ferric
  set). Measured [P]: with one or two steps a ±8 a square or a Nyquist alternation blows up to 10⁶ · M_s
  (the draft's "≤ 2 substeps at a / 4" was both wasteful — always 2 on loud signals — and unstable); with
  the rule above the worst case stays within 0.5 % of M_s; sines up to 10 kHz at 4 a cost 1.0 – 1.2 steps on
  average, a 15 kHz sine at 8 a 2.6.
- No Ḣ estimate at all. ChowTape's α-transform derivative of H is what makes the time-domain form
  unstable near Nyquist (Chowdhury §4.1) and what its per-solver input clips guard against. Checked [P,
  192 kHz, Chowdhury's set]: the time form returns NaN on an alternation at Nyquist **at any level**, the
  H form never does under the substep rule. On audio-band sines the gain is real but modest — error
  0.02 – 0.36 % of M_s (time) vs 0.00 – 0.18 % (H) up to 4 a and 10 kHz — and both need substeps above
  that. The advantage is robustness, not accuracy. A soft clip of H at ±8 a remains as the bound the
  substep cap relies on.
- The J-A δ_M indicator (irreversible term only when M_an − M has the sign of ΔH) is kept, as in ChowTape:
  it removes the non-physical negative susceptibility at reversals. It may switch inside a step: g is then
  discontinuous but bounded, the error is in the figures above.
- Langevin with Chowdhury's small-|Q| series (|Q| < 1e−3 → Q / 3) and a rational tanh; NaN or |M| > 20·M_s
  resets the state to M_an(H) (counted by the bench, never expected).
- **Reversible share c ≈ 0.9 at optimum bias.** Chowdhury's unbiased c = 0.17 cannot be an aligned
  machine: at low level only the reversible part moves, so the gain is c times the anhysteretic slope
  (−15.4 dB), it rises 10 dB with level (an expander), and THD is already 2.2 % at −20 dB re a [P]. With
  c = 0.9 the low-level gain is −0.9 dB, THD 0.05 % at −20 dB re a, and the loop still opens under drive.
  ChowTape's default (width 0.5 → c ≈ 0.7) sits between: its sound is an under-biased machine's.
- **Makeup**: the output is divided by the **reversible** small-signal slope at 100 % bias,
  c · M_s / (3a − c α M_s) [A] (the anhysteretic slope M_s / (3a − α M_s) of the draft is reached only at
  c = 1), so low levels pass at unity whatever the formula; it does **not** follow `bias`, so under-bias
  is heard as what it is (quiet notes lose level and gain grit).

**Bias** (`bias`, 50 – 150 %, 100 = the model's optimum). A tape particle is magnetised once, as it leaves
the gap, while the bias field decays: optimally biased tape follows the **anhysteretic** curve, and the
time-domain J-A is a phenomenological stand-in [I, magnetic recording theory]. So bias acts on three
things, never through a tone:

| `bias` | J-A `c` (reversible share) | Relaxation to M_an | HF erasure (spacing-equivalent) | HF sensitivity | Sound |
|---|---|---|---|---|---|
| 50 % (under) | 0.3 (linear from 100 %) | 250 ms | none | +1 dB | Crossover grit on quiet notes (low-level gain −9 dB, the "dead zone"), more distortion, brighter, and the memory of ChowTape v1: remanence −13 dB re the last peak at c = 0.5 [P] |
| 100 % | 0.9 | 50 ms | model's | 0 | Aligned machine: remanence −32 dB re the last peak [P] |
| 150 % (over) | 0.98 (linear from 100 %) | 20 ms | grows (≈ 0.5 µm [I]) | −2 dB | Cleaner at low level, darker; the same saturation ceiling |

**Relaxation.** After loud playing the J-A state keeps a remanent M: a DC step, and the 0 – 30 Hz noise
ChowTape's users report. Real tape has no such memory (each particle is fresh). M is therefore pulled
towards M_an(H) with the time constant of the table. Measured [P]: at 50 ms the steady loop of a 41 Hz note
changes by 0.6 % (rms) at c = 0.9 and 4.4 % at c = 0.5, 0.03 % and 0.2 % at 1 kHz; THD moves by ≈ 0.1 point at most.
The memory itself is short even without it: a quiet tone following a loud one erases the remanence in
under 100 ms (J-A accommodation), the relaxation only makes the first 50 ms cleaner (2nd harmonic −52 →
−63 dB at c = 0.9, −33 → −44 dB at c = 0.5) and empties the state in true silence, where nothing else
would. A 2nd-order 10 Hz high-pass after the core removes the step.

**Oversampling.** gfx-dsp `oversample` half-bands (minimum phase, no reported latency): internal rate ≥
352.8 kHz — **8× at 44.1 / 48 kHz, 4× at 88.2 / 96, 2× at 176.4 / 192**. Fixed in code, not a parameter
(group delay must not depend on a setting). 4× was the draft's starting point by analogy with gfx-drive;
the loop's kinks at reversals decay slowly and the prototype settles it [P, non-harmonic energy below
16 kHz re the fundamental, c = 0.9, H = +6 … +18 dB re a]:

| Sine | 4× | 8× |
|---|---|---|
| 3 – 5 kHz | −77 … −85 dB | −85 … −88 dB |
| 9.2 kHz | −70 … −77 dB | −82 … −85 dB |
| 12 kHz | −57 … −66 dB | −78 … −82 dB |

At c = 0.5 (under-biased) 8× still leaves −64 … −68 dB at 12 kHz for mid levels: accepted, it is a dirty
setting and the model's band edge follows.

**Vinyl and Shellac** use no J-A: RIAA (or the 78 curve) pre-emphasis, then a tracing
distortion on the groove velocity v: the stylus tip (radius r) rides a displacement error −(r / 2V²) · v²
(V = groove speed), so **y = v − 2κ · v · dv/dt**, κ = r / 2V² [A, second-order tracing term]: 2nd
harmonic, rising 6 dB / octave faster than the signal and ×5.9 from the outer groove (r 146 mm) to the
inner (60 mm) — `wear` moves it inward. Computed at 2× (a product of two band-limited signals), then a
soft cutter limit with ADAA1 [I: voicing]. (The draft's v + κ · v · |v| is an odd function: 3rd harmonic,
not the even order it claimed.) `bias` and
`formula` are inert there; `input` keeps its meaning (cutting level).

### 2.3 Transport: wow, flutter, snags, stop

Speed deviation **ε(t)** (fraction of nominal speed), built from seeded processes, summed for two
independent passes — the **record pass and the playback pass** (two seeds): a recording played back is
warped twice, which is one reason real wow never repeats [A].

| Band | Components (per model, §2.1) | Process |
|---|---|---|
| Drift | 0.02 – 0.2 Hz | Ornstein-Uhlenbeck (seeded), corner 0.05 Hz |
| Wow | Rotating parts below 4 Hz (reels, pinch roller, platter, servo hunting) | Each a sinusoid at f_k with its harmonics (eccentricity is not a sine: 2nd at −10 dB), whose amplitude and phase random-walk with ~0.1 Hz bandwidth; reel frequencies glide slowly within their range |
| Flutter | Capstan, idlers, motor cogging (4 – 40 Hz); random flutter 40 – 200 Hz | Periodic components as above with ~0.5 Hz bandwidth; plus band-passed noise |
| Scrape | 2 – 5 kHz stick-slip | Not a pitch effect: rendered as modulation noise (§2.4) |

- **`wow`** = unweighted **peak** speed deviation of drift + wow bands (0 – 4 %); **`flutter`** = that of
  the flutter band (0 – 1 %). 1 % = 17.2 cents; 4 % = 68 cents, melted tape [A]. The model gives the
  spectrum; the knobs give the amount. Defaults: **wow 0.10 %, flutter 0.05 %** (1.7 and 0.9 cents peak):
  alive, never seasick.
- **`wow_rate`** (25 – 400 %, 100 % = the model's frequencies) scales every component frequency of the
  drift and wow bands; **`wow_rand`** (−100 … +100 %, 0 = the model's balance) moves energy between the
  periodic components (−: a regular, machine-like wobble) and the random processes (+: erratic, Gen Loss's
  "unpredictable vibrato").
- **Read-only meter `wf_out`**: ε through the IEC 386 / AES6 weighting (4 Hz peak), RMS over 2 s, in %
  (WRMS, the JIS / NAB figure; DIN quasi-peak reads ≈ 1.3 – 2× higher depending on the spectrum [I]). A
  preset can then say "0.04 % weighted, a studio machine" and golem can display it. Weighting, dB at Hz
  **[U: recalled by the reviewer from the DIN 45507 / IEC 386 table, not re-read; check against AES6-2008
  before the meter ships]**: 0.2 −30.6 · 0.315 −19.7 · 0.4 −15.0 · 0.63 −8.4 · 0.8 −6.0 · 1 −4.2 · 1.6 −1.8 ·
  2 −0.9 · 4 0 · 6.3 −0.9 · 10 −2.1 · 20 −5.9 · 40 −10.4 · 63 −14.2 · 100 −17.3 · 200 −23.0. Realised as a
  filter fitted to the table at the control rate (ε is generated there), so the meter costs nothing in
  the audio loop. The knobs stay **unweighted peak**: machine specs are weighted, so a deck quoted at
  0.08 % WRMS is `wow` + `flutter` of roughly 0.15 – 0.3 % depending on its spectrum; presets quote
  `wf_out`, not the knobs.

**Pitch domain** (gfx-vibrato's method). The read head lags by **D(t) = D0 − ∫ε dt**, the integral leaky at
0.02 Hz (it bounds the drift's random walk), read with gfx-dsp `delay` Lagrange-5 at the host rate. The
pitch ratio is exactly 1 + ε. D0 is the smallest centre that keeps D ≥ 1 sample: D0 = 1.2 · Σ peak
excursions (ε_k / 2πf_k per periodic component, 3σ of the random ones) + snag headroom, clamped to 40 ms
[A]: **≈ 1.5 ms at the defaults**, 12.7 ms for 4 % wow at 0.5 Hz. D0 **rises at up to 20 ms/s** (a ≤ 2 %
pitch sag while wow comes in: the tape slowing, which is musical) and **falls at ≤ 1 ms/s** (0.1 %, 1.7
cents: inaudible). With `wow`, `flutter` and snags at 0, D0 = 0 and the delay line is bypassed (exact zero
delay). Not reported as latency: it moves with settings (gfx-vibrato precedent).

**Snags** (`snags`, 0 – 100 %): Poisson events (rate up to 0.5 / s), each a speed transient: ε drops by up
to −30 % over 10 – 40 ms, then overshoots so that ∫ε over the event is 0 (tension catches up) [I]. A pitch
"hiccup" with bounded delay excursion (≤ 6 ms, included in D0).

**Stop** (`stop`, bool, footswitch-able) and `stop_time` (50 – 3,000 ms): speed ramps to 0 along a
motor-like curve (exponential, then a linear stall), the read head falls behind (buffer 3.5 s at the
highest rate, allocated in `initialize`). Release: the wet restarts from silence reading the live input
with speed ramping 0.3 → 1 over `stop_time` / 4 (spin-up, pitch rising), then the accumulated lag is
removed by a 30 ms crossfade to the D0 head. A full stop outputs silence (the input during a stop is not
"recorded", as on a real machine). Gen Loss's Aux Stop and iZotope Vinyl's Spin Down.

### 2.4 Faults and noise on the medium

**Spacing filter** (new, gfx-dsp `tape`). Spacing loss is **54.6 · d / λ dB = 54.6 · d · f / v dB** [V,
Chowdhury eq. 13]: linear in f, and in d / v. Example [A]: 2 µm of debris costs 2.9 dB at 10 kHz at 15 ips,
22.9 dB at 1⅞ ips (and 2.3 dB at 1 kHz). A dB-linear-in-f response is not rational, so it is fitted — **once, in the normalised frequency x =
f / f0, f0 = v / (2π d)** (the loss is e^(−x)): **four one-pole TPT low-passes, one at 0.445 f0 and three
at 3.06 f0** [P: within 1.4 dB of the target down to −30 dB; five poles (0.44 f0, 4 × 3.70 f0) give 1.1 dB].
The corners move with d / v, so there is no table and no interpolation, and every d and speed has the same
accuracy. As d → 0 the corners pass Nyquist and each TPT section tends to y = x exactly (g / (1 + g) → 1):
zero spacing is a true bypass. Minimum phase, no latency, modulation-safe. Below −30 dB it keeps falling
(24 dB / octave). The draft's three fixed shelves (1.5, 5, 14 kHz) cannot do it: a 30 dB loss needs a
21 dB / octave slope where three first-order shelves give less; the best fit is 3.2 – 4.6 dB off once the
loss at 10 kHz passes 12 dB [P], against its own ±1.5 dB target.

d(t) = model's static spacing + `spacing` (user, 0 – 20 µm) + wear + dropout and crinkle events.

**Dropouts** (`dropouts`, 0 – 100 %): Poisson events, rate 0 – 3 / s; each a d(t) excursion with a
raised-cosine rise (2 – 10 ms) and fall (10 – 80 ms), duration lognormal (median 40 ms), depth lognormal
(median 3 µm, 1 % reach 15 µm), plus a level dip of −0.3 dB per µm beyond 5 µm (deep loss of contact) [I].
**Musical by physics**: highs vanish first and the lows hold; the same event is a shimmer at 15 ips and a
hole at 1⅞. `drop_sync` (the shared sync list, Off by default) puts the events on the host grid: each grid
step has a dropout with probability ∝ `dropouts`, starting on the step (±0 samples, phase from the host
transport): rhythmic tape gating that still sounds like tape.

**Crinkle** (`crinkle`, 0 – 100 %): bursts (100 – 600 ms) of fast spacing modulation, |band-passed noise
20 – 80 Hz| × up to 4 µm, plus a matching ±1 dB AM: Gen Loss's "crinkles", the crumbling of creased tape.

**Crackle** (`crackle`, 0 – 100 %): impulses (Poisson, heavy-tailed amplitudes, Pareto α ≈ 1.5), each a
short decaying burst shaped by a 10 – 14 kHz stylus resonance, added **before** the playback de-emphasis
(RIAA makes them ticks, not clicks) [I]. On Vinyl a share of them are **scratches**: they repeat every
rotation (1.8 s at 33⅓, 0.77 s at 78), drifting slowly. On tape models crackle is static and splice
ticks (rarer, duller).

**Modulation noise** (always on, scaled by the model and `wear`): y = x · (1 + m(t)), m = band-limited noise
(20 Hz – 5 kHz, including the scrape-flutter band) at −55 dB (Studio) to −35 dB (Microcassette) re the
signal [I]. It exists only under the signal: the physical reason tape "breathes" with the playing.

**Additive noises**, injected after the core and before the playback losses and band edge: **hiss**
(shaped per model: the standard's replay boost over the head's losses — a rising spectrum that flattens
at the band edge; 70 µs models are 4.5 dB quieter above ≈ 5 kHz than 120 µs ones [A, §1]; on Vinyl,
surface noise before the RIAA cut; `hiss` dB relative to the model's S/N, §2.1),
**mechanical noise** (`mech`, dB relative: motor hum harmonics, transport whir, vinyl rumble 10 – 40 Hz),
**crackle**. All three follow `noise_mode`:

| `noise_mode` | Gain of the additive noises | Use |
|---|---|---|
| **Follow** (default) | min(1, env / −24 dBFS) (1:1 below −24 dBFS), env = gfx-dsp `SlidingMax` 25 ms → release **linear in dB, 120 dB/s**, read on the block's input **before `input`** (the knob must not move the noise); **exactly 0 below −90 dBFS** (reached ≤ 0.75 s after a note at any level stops) | Live rigs: silence stays silent, hiss sits under notes like real tape |
| Constant | 1 | Recording, ambience: the real machine |
| Duck | 1 − 0.9 · min(1, env / −24 dBFS) | Lo-fi beds: crackle and hiss in the gaps, out of the way while playing (RC-20 v2) |

The draft's one-pole 300 ms release could not meet its own bench ("exactly 0 within 500 ms"): from
−24 dBFS to the −90 dBFS switch a one-pole takes 7.6 time constants, 2.3 s, and the hard switch would then
cut a tail still 66 dB down — inaudible on Studio Reel (−152 dBFS), not on Shellac before a high-gain amp.
A dB-linear release ends where it says. In Follow the hiss is 1:1 with the playing below −24 dBFS, so it is
never heard alone: the mode trades realism for a silent rig, and says so.

`noise` (bool, on the face) switches the additive noises on / off (20 ms fade); modulation noise is not
switched (it is part of the medium and silent in silence).

**Wear** (`wear`, 0 – 100 %): the macro of age and failure, per the model's own profile: static spacing
+0 … 3 µm (dulling), dropouts, crinkle, snags (tape) or crackle and scratches (vinyl), modulation noise
+10 dB, random-wow share +30 %. The experts `dropouts`, `crinkle`, `snags`, `crackle` add **on top**
(default 0: the contract's neutral-on-top rule).

### 2.5 Playback: alignment, EQ, head bump, azimuth

- **Playback EQ**: the exact inverse of the record shelf (§2.2), TPT sections; the standard's own replay
  boost appears only where it is heard, in the hiss spectrum (§2.4).
- **Alignment and band edge**: the model's measured small-signal response is what a well-aligned unit
  gives: a high-shelf / low-pass pair that sets the rated −3 dB points (§2.1) and gap loss beyond them
  (sin(kg / 2) / (kg / 2), g = 1.5 – 3 µm by model, built into the same low-pass fit). Thickness loss is
  **not** modelled separately: the alignment cancels it on any real machine [A]; only post-alignment changes
  (spacing, azimuth, bias) are dynamic.
- **Head bump** (`head_bump`, 0 – 200 %, 100 = the model's): a bell at f_b = v / L_eff (+2 dB, Q 1.4) plus a
  dip at ≈ 2.2 f_b (−0.7 dB) and the 2nd-order low-frequency roll-off below 0.5 f_b (the contour effect's
  ripples, first two kept) [I: shapes from Endino's plots]. 38 Hz at 15 ips, 76 Hz at 30 ips (where the lows
  below it fall away, as measured), 19 Hz at 7½ on the Studio head.
- **Azimuth** (`azimuth`, −30 … +30 arc-minutes, 0 default): within-track HF loss
  sin(π w tan θ / λ) / (π w tan θ / λ) (w = track width by model: cassette 0.6 mm → −2 dB at 10 kHz for 10′,
  −21 dB for 30′ [A]), folded into the spacing filter as an equivalent; in stereo, an inter-channel delay
  s · tan θ / v (s = track-centre spacing; cassette 1 mm at 10′ → 61 µs: a mono-sum notch at 8.2 kHz [A]).
  The physical cause of "this cassette sounds dull in mono".
- **ALC** (`alc`: Model · Off · On; Portable Cassette and Microcassette own it): a recorder's automatic
  level control before the record EQ (attack 5 ms, release 1.5 s, −20 dB range, target −6 dB re reference):
  the breathing dictation sound. Inert at Off.

### 2.6 Noise reduction (companders)

`nr`: **Off** · **Compander** (v1) · *Sliding Band* (appended when coded).

- **Compander**: a 2:1 RMS compander with pre-emphasis, encode before the tape, 1:2 expand after
  (broadband, 10 dB pre-emphasis, RMS 10 ms detector) [I: generic, after the broadcast-and-studio
  companders]. **Topology (it decides the tracking)**: both detectors read the **encoded** signal — the
  encoder in feedback (gain = 1 / rms of its own output), the decoder in feed-forward (gain = rms of what
  comes off the tape) — with the same detector filter, the same one-sample convention (gain from the
  detector's state before the current sample) and unity at the model's reference level. Then decode is the
  exact inverse of encode on a perfect channel (bench ±0.5 dB, in practice rounding), and the 10 ms
  detector's ripple on low notes cancels instead of distorting. A feed-forward encoder reading the input
  would not track. On tape it **mistracks** as
  real ones do: HF loss, dropouts and level errors are expanded 2× — the pumping and breathing of 4-track
  demos, which is the point. Hiss injected at the tape is pushed down by the expander (its own level again, in dB, in the
  gaps: −68 dB re reference plays at ≈ −136 dB), so Compander is also a quiet-machine option.
- **Sliding Band**: a single-band HF sliding-shelf compander (the consumer-cassette type, ≈ 10 dB at HF).
  Designed only in outline; deferred until the Compander passes.

### 2.7 Stereo

Mono 1 / 1 first (tape saturation is not stereo by nature); 2 / 2 offered.

- **Physical (`spread` 0 %, default)**: one transport — ε is identical on both channels (correlation of the
  two pitch tracks = 1); hiss, mechanical noise and modulation noise are independent per track; dropout
  events are shared with ρ = 0.7 depth correlation (debris spans part of the tape width); azimuth adds its
  inter-channel delay. J-A runs in two f64 SIMD lanes.
- **`spread`** (0 – 100 %): un-physical width, Gen Loss's "Spread" and ToTape8's semi-independent flutter:
  the random parts of ε, the dropout events and the crinkles become independent per channel in proportion
  (periodic components keep their common phase so the image does not tear). At 100 % L and R wobble
  independently: a wide warble chorus.
- A mono input on a 2 / 2 layout with `spread` 0 and `azimuth` 0 computes once and copies L to R
  (bit-exact).

### 2.8 Output, mix, bypass

- `low_cut` (2-pole, 20 – 1,000 Hz, 20 = off) and `high_cut` (2-pole, 1 – 20 kHz, 20 = off) after playback,
  on top of the model's band — the same ids and units as gfx-crusher.
- `level` (−24 … +12 dB) final trim; `input` compensation as §2.2.
- **`mix`**: linear crossfade dry ↔ wet (100 % = wet only), the gfx-crusher law; at 50 % with wow it is the
  "dry at unity" chorus of Gen Loss. The dry is **delayed by D0 (slewed with it) and passed through the same
  half-band pairs** (`Oversampler2x::identity`, one per stage — gfx-drive's fix, and what gfx-dsp's
  `oversample` documents). A whole-sample delay is not enough: the half-bands' phase runs away from a pure
  delay above 10 kHz, and a 50 % mix against an integer-delayed dry measures −0.8 dB at 10 kHz, −4.2 dB at
  14 kHz and **−11.9 dB at 16 kHz** [P, two stages at 48 kHz; three stages −13 dB at 18 kHz]. The record
  and playback EQs are exact inverses and need nothing. The identities run only while `mix` < 100 %.
  At wow 0 a 50 % mix is comb-free; with wow the wet swings around the dry (a centred chorus).
- **`bypass`**: the nih-plug bypass parameter (what the coded effects ship); 10 ms crossfade, no tail to
  keep (D0 ≤ 40 ms).
- **Latency: reported 0.** The wet carries D0 (0 – 40 ms, ≈ 1.5 ms at the defaults) and ≈ 6 samples of
  minimum-phase group delay at low frequencies [P]; nothing looks ahead. Within the contract by the
  gfx-vibrato precedent (a delay that moves with the settings cannot be reported without the host
  re-syncing on every knob move), and harmless inside the block since the dry follows D0. What it does
  cost is stated for golem-rack: a lane carrying gfx-tape runs D0 late against a parallel lane (a DI
  split, a second microphone); at the defaults that is 1.5 ms, a comb from 333 Hz if the two are summed.
  "Studio Glue" presets (wow ≤ 0.03 %) stay under 0.5 ms; wow, flutter and snags at 0 give exactly 0.

### 2.9 gfx-dsp reuse and new code

| Module | Use |
|---|---|
| `oversample` | 2× half-band stages, cascaded to 8× (gfx-drive's construction); `identity` per stage on the dry when `mix` < 100 % (§2.8) |
| `delay` | Transport read head (Lagrange-5), dry alignment line, stop buffer |
| `svf` | Record / playback EQ, alignment, head bump, `low_cut` / `high_cut`, spacing shelves (TPT, modulation-safe) |
| `adaa` | Vinyl tracing nonlinearity, cutter limit |
| `envelope` | `SlidingMax` + release for `noise_mode`; ALC detector; compander RMS |
| `lfo` (`Rng`), `transport` | Seeded processes; `drop_sync` grid phase |
| `smooth`, `exp_smooth` | Parameter smoothing; the EXP rule on `input`, `wow`, `flutter`, `wear`, `mix`, `level` |
| **New: `tape`** (listed in gfx-dsp's README since the start, never written) | `Hysteresis` (J-A along H, RK4 with substeps, relaxation, bias mapping, SIMD ×2); `Transport` (ε generator: components, OU, snags, stop; pitch-domain read with D0 management); `SpacingFilter` (fitted family, tables at `prepare`); `TapeNoise` (hiss, modulation noise, crackle, mechanical, modes). **Written in gfx-dsp from the first commit**: gfx-grit's design already books a "Tape" module calling this engine (its enum appends it when it exists), so `Hysteresis` has a second user by design and must expose a self-contained entry — `TapeSat`: record shelf → J-A → inverse shelf → makeup, at a caller-supplied internal rate, no transport, no noise. gfx-delay's Tape model and gfx-chorus's Tape engine **stay as they are** (loop and voice colours, voiced and preset as such); adopting `Transport` there is a v2 question for the owner, not an offer this design depends on |
| New in the crate | Model tables (§2.1), compander, ALC, IEC 386 weighting meter |

### 2.10 CPU [A, estimate]

Per host sample, mono, 48 kHz: J-A at 8×, one RK4 step (4 evaluations of g, each a rational tanh, ~3
divisions: ≈ 25 ns) ≈ 800 ns; three half-band pairs ≈ 70 ns; ~15 TPT sections ≈ 45 ns; transport and read
≈ 20 ns; noise and events ≈ 20 ns. **≈ 0.95 µs (4.6 % of a core); worst ≈ 2 µs** on sustained loud HF (2 – 3
substeps) with the compander; the cap of 8 substeps bounds a pathological input (full-scale alternation)
at ≈ 6.5 µs, which no instrument produces — the bench reports it rather than bounding it. Stereo ≈ 1.3×
mono (J-A in two SIMD lanes). By far the heaviest golem-fx pedal per sample (gfx-emet: 198 ns): the
price of −80 dB aliasing on a hysteresis loop. A cheaper core exists if the bench finds this too dear: at
c ≥ 0.9 the loop is within 1 dB of the static anhysteretic curve, which takes ADAA at 2×; it is the
fallback for gfx-grit's module, not for this pedal.

## 3. Parameters

New golem block: **type `tape`**, category **`lofi`**, golem-fx only (a `quarantine` like `met`, `glitch`:
no original plugin). Roles: guitar, bass, keys, voice, drums (lofi is offered to voice and drums; golem-rack
adds `tape` to `role_blocks.drums`). No catalogue ids to keep: these ids and **names** are the join keys
from the first ship and never change (descriptor rules). All ranges linear; enums grow at the end only.

### Front panel

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `input` | Record | f | −12 – +24 dB | +6 dB | 20 ms | ✓ | Record level into the medium: 0 dB = −18 dBFS at the model's reference; output compensated by −`input` + the model's static makeup (§2.2). Same id and meaning as gfx-crusher's `input` (drive into the medium, loudness held); narrower range, and EXP here |
| `wow` | Wow | f | 0 – 4 % | 0.10 % | 50 ms | ✓ | Peak slow speed deviation (drift + wow bands); the model gives the spectrum |
| `flutter` | Flutter | f | 0 – 1 % | 0.05 % | 50 ms | ✓ | Peak fast speed deviation (4 – 200 Hz) |
| `wear` | Wear | f | 0 – 100 % | 0 % | 50 ms | ✓ | Age and failures per the model's profile: dulling, dropouts, crinkles, snags / crackle |
| `mix` | Mix | f | 0 – 100 % | 100 % | 20 ms | ✓ | Dry ↔ wet crossfade, dry aligned (50 % + wow = tape chorus) |
| `model` | Model | e | Studio Reel · Home Reel · Cassette · Portable Cassette · Microcassette · VHS · Vinyl · Shellac 78 | Studio Reel | 30 ms wet dip | | The medium and machine (§2.1) |
| `speed` | Speed | e | Model · 1⅞ ips · 3¾ ips · 7½ ips · 15 ips · 30 ips | Model | 30 ms wet dip | | Tape speed: losses, bump, EQ, transport scale; inert on Vinyl / Shellac / Radio |
| `noise` | Noise | b | off / on | on | 20 ms | | Hiss, mechanical noise, crackle (per `noise_mode`) |

Defaults are sound-forward but safe (gfx-crusher's lesson): an inserted block is heard at once (+6 dB into
a 15 ips machine, light living wow), and never surprises a live rig (no failures, noise in Follow mode).
None depends on another parameter.

### Expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `formula` | Formula | e | Model · Ferric · High Output · Chrome | Model | 30 ms dip | | J-A set; cassette EQ 120 / 70 µs |
| `bias` | Bias | f | 50 – 150 % | 100 % | 50 ms | ✓ | Under: crossover grit, brighter; over: cleaner, darker (§2.2) |
| `head_bump` | Head Bump | f | 0 – 200 % | 100 % | 50 ms | | Contour effect, relative to the model's |
| `spacing` | Spacing | f | 0 – 20 µm | 0 µm | 50 ms | ✓ | Extra head-to-tape spacing: speed-dependent HF loss (dirty heads, worn tape) |
| `azimuth` | Azimuth | f | −30 – +30 ′ | 0 ′ | 50 ms | | Head tilt: HF loss; in stereo, inter-channel delay |
| `alc` | ALC | e | Model · Off · On | Model | — | | Recorder's automatic level control |
| `nr` | Noise Reduction | e | Off · Compander (· Sliding Band) | Off | 30 ms dip | | Compander encode / decode, mistracking included |
| `wow_rate` | Wow Speed | f | 25 – 400 % | 100 % | 200 ms | ✓ | Scales the drift / wow component frequencies |
| `wow_rand` | Wow Randomness | f | −100 – +100 % | 0 % | 200 ms | | − periodic, machine-like; + erratic |
| `snags` | Snags | f | 0 – 100 % | 0 % | 50 ms | | Pitch hiccups (on top of `wear`) |
| `dropouts` | Dropouts | f | 0 – 100 % | 0 % | 50 ms | ✓ | Spacing events (on top of `wear`) |
| `drop_sync` | Dropout Sync | e | shared sync list (Off · 4/1 … 1/32) | Off | — | | Dropouts on the host grid |
| `crinkle` | Crinkle | f | 0 – 100 % | 0 % | 50 ms | | Creased-tape crumble (on top of `wear`) |
| `crackle` | Crackle | f | 0 – 100 % | 0 % | 50 ms | | Clicks, scratches (Vinyl repeats per rotation) |
| `noise_mode` | Noise Mode | e | Follow · Constant · Duck | Follow | 50 ms | | §2.4 |
| `hiss` | Hiss | f | −40 – +20 dB | 0 dB | 50 ms | | Relative to the model's S/N |
| `mech` | Mechanical | f | −40 – +20 dB | 0 dB | 50 ms | | Relative to the model's motor / rumble noise |
| `stop` | Stop | b | off / on | off | — | | Tape stop / spin-up (§2.3) |
| `stop_time` | Stop Time | f | 50 – 3,000 ms | 600 ms | — | | Stop ramp; spin-up = ¼ of it |
| `spread` | Spread | f | 0 – 100 % | 0 % | 200 ms | | Stereo decorrelation of the random parts (2 / 2 only) |
| `low_cut` | Low Cut | f | 20 – 1,000 Hz | 20 Hz (off) | 30 ms | | After playback |
| `high_cut` | High Cut | f | 1,000 – 20,000 Hz | 20,000 Hz (off) | 30 ms | ✓ | After playback |
| `level` | Level | f | −24 – +12 dB | 0 dB | 20 ms | ✓ | Output trim |
| `bypass` | Bypass | b | off / on | off | 10 ms | | VST3 bypass parameter |
| `wf_out` | W&F (weighted) | f, read-only | 0 – 5 % | — | — | | IEC 386-weighted WRMS of the current transport |

Expert groups (`visuals/data/groups.json`): **Machine** (`formula`, `bias`, `head_bump`, `spacing`,
`azimuth`, `alc`, `nr`), **Transport** (`wow_rate`, `wow_rand`, `snags`, `stop`, `stop_time`, `spread`,
`wf_out`), **Failures** (`dropouts`, `drop_sync`, `crinkle`, `crackle`), **Noise** (`noise_mode`, `hiss`,
`mech`), **Output** (`low_cut`, `high_cut`, `level`).

Parameters inert on some models say so and are never re-mapped: `speed`, `formula`, `bias`,
`head_bump`, `azimuth` on Vinyl / Shellac; `alc` at Off.

`eq` (EQ Standard: Model · NAB · IEC) was removed at the review: the standard belongs to the model × speed
table (§2.1), its audible share is a hiss tilt and ≤ 2.6 dB of HF drive, and an enum can be added later
but never withdrawn. 25 expert parameters remain; none of the others is redundant (see the Review).

### Visual

- **Category `lofi`** (with gfx-crusher; golem's existing lofi category, which is offered to voice and drums
  lanes).
- **Crate hue `#d8c8a0`, deck champagne** (`METAL_CRATE_HUE['gfx-tape']`): the pale gold-anodized
  faceplate of a reel deck. Decided at the review against the draft's ferric brown `#94461a`: brown is
  **not** free — gfx-fuzz proposes copper `#b0602a`, also anodized, ΔE*ab 11 from `#94461a` (same hue angle
  22° / 24°, close value), which is one colour at tile size; drive's orange `#e04a00` sits at 20°. A darker
  oxide (`#6e2c12`) only reaches ΔE 26. Champagne is ≥ 44 from every hue in use (gapmeter grey 44, fuzz
  copper 48, ringmod brass 53) [A, CIELAB]. It is the one pale body of the set, like the gap meter's
  grey a deliberate exception to "saturated": the engraving reads dark on it, and the rune needs its own
  light (`RUNE_LIGHT['gfx-tape'] = '#94461a'`: the oxide survives as the glow and the ribbon's ink).
- **Finish: anodized** (`CRATE_FINISH['gfx-tape'] = 'anodized'`), overriding lofi's thrash: a machined deck
  panel versus the crusher's and grit's arcade lacquer. Three raspberry thrash pedals in one family would
  be told apart by motif only; the tape machine is the one that is not a toy.
- **Motif `ribbon`** (new): a tape ribbon — two parallel lines 2.5 units apart — crossing the face through
  the rune zone (below the knob grid, which covers x 6 … W − 6 from y 10: gfx-freqshift's review), waving
  with an irregular wow (sum of two sines at 1 and 2.7 cycles across the face, amplitude 3 units); the
  ribbon **breaks in two short gaps** (3 and 5 units: dropouts) and is clipped by the rune's disc so it
  appears to pass behind it; three tiny dots (r 0.6) near one gap as oxide specks. Anodized: engraved
  (dark 1.6 + light 0.7 offset, the `motifSvg` convention); thrash if forced: yellow lined in black.
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-tape": {"name": "Tape", "blocks": ["tape"], "knobs": ["input", "wow", "flutter", "wear", "mix"],
               "selectors": ["model", "speed"], "toggles": ["noise"], "category": "lofi", "color": "#d8c8a0"}
  ```
- **Glyph** (`visuals/glyphs/tape.svg`): a cassette — distinct from `delay-tape`'s open reels — with a
  wobbling tape window:

  ```svg
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>Tape (gfx)</title><rect x='2.5' y='5' width='19' height='14' rx='1.5'/><circle cx='8.5' cy='11' r='2'/><circle cx='15.5' cy='11' r='2'/><path d='M7 19l1.5-3h7l1.5 3'/><path d='M10.5 11c1-1 2-1 3 0' opacity='.5'/></svg>
  ```
- No maker's livery, logo or trademark; model names are generic (no machine brands), preset names carry no
  artist or trademark.

## 4. Applications

Reference settings become block presets (`presets/gfx-tape.json`), level-matched to the bypass (±2 LU on
the bench DIs) with `level`; presets with failures state `noise_mode` and `wear` explicitly.

| Use / preset idea | Settings to start from | Feature that matters |
|---|---|---|
| **Studio Glue** (always-on, guitar or keys) | Studio Reel, 15 ips, `input` +6, wow 0.03 %, flutter 0.02 %, `noise` off | J-A soft compression; aligned machine; inaudible transport |
| **Hot 30** (bright, tight lows, mastering-deck sheen) | Studio Reel, 30 ips, `input` +9, `head_bump` 100 % | Bump at 76 Hz, lows below fall away; 17.5 µs = little HF compression |
| **Bass Bump** | Studio Reel, 15 ips, `input` +8, `bias` 110 % | Head bump at 38 Hz under the fundamental; over-bias rounds the attack |
| **Under-Biased Grit** (dirty clean tone) | Home Reel, 7½ ips, `bias` 60 %, `input` +12 | Loop opens: crossover grit on quiet notes |
| **Cassette Demo** | Cassette, `input` +8, wow 0.15 %, flutter 0.08 %, `nr` Compander, `noise` on, Follow | Pre-emphasis saturates highs first; compander breathing |
| **4-Track Double Speed** | Cassette, `speed` 3¾, `nr` Compander, `input` +10 | Speed scales losses and transport |
| **Walkman Warble** (lo-fi chords) | Portable Cassette, wow 0.6 %, flutter 0.2 %, `wow_rand` +40 %, `wear` 30 % | Random wow; ALC; dropouts sound like tape |
| **Melted Tape** (EXP) | Cassette, EXP on `wow` 0.2 → 4 %, `wow_rate` 60 % | Pitch-domain transport, D0 rises cleanly |
| **VHS Chorus** | VHS, `mix` 50 %, wow 0.8 %, flutter 0.15 %, 2 / 2 `spread` 60 % | Crossfade with aligned dry = centred chorus; spread widens |
| **Dictaphone Voice** | Microcassette, `input` +12, `noise` on, `mech` +6 | 300 Hz – 5 kHz band, ALC pumping, motor noise |
| **Dusty Vinyl Keys** (lo-fi beats) | Vinyl, `crackle` 40 %, `wear` 30 %, `noise_mode` Duck | Crackle in the gaps, scratches repeat every 1.8 s |
| **78 Shellac** | Shellac 78, `input` +4, `noise_mode` Constant | 100 Hz – 5 kHz, loud surface noise, rotation wow |
| **Dropout Rhythm** (performance) | Cassette, `dropouts` 70 %, `drop_sync` 1/8, `wear` 0 | Grid-locked spacing dropouts: highs gate in time |
| **Tape Stop Stomp** | any model, `stop` on a footswitch, `stop_time` 800 ms | Stop and spin-up, no click |
| **Seasick Lead** | Home Reel, wow 1.2 %, `wow_rand` −60 %, `wow_rate` 150 % | Periodic, machine-like wobble |
| **Crumbling Archive** | Home Reel, `wear` 70 %, `crinkle` 40 %, `snags` 30 %, `azimuth` 15 ′ | Every failure physical; mono-sum dullness from azimuth |
| **Drum Bus Tape** (drums lane) | Studio Reel, 15 ips, `input` +12, `noise` off | Transient softening by J-A; HF compression |
| **Parallel Tape Drive** (bass) | `mix` 40 %, `input` +18, Cassette | Dry aligned: no comb, thick low-mid |
| **Dirty Heads** (EXP) | any tape model, EXP on `spacing` 0 → 10 µm | Speed-dependent HF loss, a physical "tone" pedal |

Not attempted: playback at a different speed than recording (half-speed octave-down: needs a buffer and
latency; gfx-pitch's job), echoes (gfx-delay), generations as a parameter (a "copy of a copy" is two
gfx-tape blocks in series, which the rack does; a preset pair can state it).

## 5. Bench suite (on top of gfx-bench's common checks)

**gfx-bench options**: `click_bound_db` −40 dB — dropouts, snags, crinkle and stop create real
high-frequency changes that are the effect (gfx-drive −25, gfx-crusher −30); measured with failures at 0 the
bound is the common −50. `skip_latency` — the wet carries D0 and minimum-phase group delay (gfx-drive,
gfx-vibrato precedent); the "Zero transport" row measures the true value. Silence floor −140 dBFS holds at
the defaults (Follow mode is exactly 0 in silence, modulation noise is multiplicative, the relaxation drains
the J-A state), measured from 1 s after the input stops.

| Check | Pass |
|---|---|
| J-A solver | Against an offline reference (adaptive RK45, 1e−9 tolerance, along H): major and minor loops for 50 Hz – 15 kHz sines at three levels, error ≤ 0.5 % of M_s up to 4 a, ≤ 2 % at 8 a above 10 kHz (the piecewise-linear path, not the solver); ±8 a square, Nyquist alternation, white noise at ±8 a: no NaN, no reset counted, \|M\| ≤ M_s; average substeps on the guitar DIs ≤ 1.3 |
| Small signal | `input` −30 dB re reference: response per model × speed within ±1 dB of the voicing target; record EQ × playback EQ with the core bypassed: flat ±0.1 dB |
| Saturation | THD at 1 kHz vs `input`: monotonic; MOL (3 %) per model within ±2 dB of the table; HF compression: at 1⅞ ips the 10 kHz 3 % point ≥ 10 dB below the 1 kHz one, at 30 ips ≤ 4 dB [I targets]; `bias` 60 %: THD at −20 dB re reference ≥ 3× that at 100 % (crossover); at 100 % the gain from −40 dB re reference to reference moves by ≤ 1 dB (no expander) |
| Relaxation | A note at +12 dB decaying over 300 ms: energy below 30 Hz ≤ −60 dB re the playing. The same cut dead at a peak: ≤ −30 dB (the remanence is −32 dB re the peak at 100 % bias [P], and the cut itself is a louder click); output < −140 dBFS within 1 s |
| Aliasing | Sines 1 – 12 kHz at `input` +12 and +18 dB on Cassette and Studio Reel: non-harmonic energy below 16 kHz re fundamental ≤ −76 dB at 100 % bias (prototype at 8×: −78 … −88 dB), ≤ −60 dB at `bias` 50 % |
| Head bump | Bump frequency at 7½ / 15 / 30 ips = f_b, 2 f_b, 4 f_b ± 5 % |
| Transport | 3.15 kHz tone, Hilbert discriminator: unweighted peak deviation = `wow` / `flutter` ± 10 %; the model's components at their frequencies ± 5 % (spectrum of ε); `wf_out` = an offline AES6 weighting of the measured ε ± 10 %; at 0 / 0 no measurable FM (≤ 0.001 %) |
| Zero transport | `wow` 0, `flutter` 0, `snags` 0: D0 = 0, measured delay = the static group delay only, constant |
| D0 | EXP sweep of `wow` 0 → 4 % in 0.5 s: D never < 1 sample; pitch sag ≤ 2.1 %; sweep back: pitch offset ≤ 0.11 % |
| Stereo | `spread` 0: L / R pitch tracks correlation ≥ 0.999; mono input, 2 / 2, spread 0, azimuth 0: L = R bit-exact; `spread` 100: correlation of the random parts ≤ 0.1 |
| Spacing filter | Static `spacing` 1 / 5 / 20 µm at 1⅞ / 15 ips: within ±1.5 dB of 54.6·d·f / v wherever the target ≥ −30 dB; `spacing` 0: bit-exact bypass of the sections; modulated at 80 Hz: no sidebands beyond the intended AM ≥ −80 dB |
| Dropouts | Rate ± 20 % over 60 s; each event click-free (HF burst ≤ −60 dB re peak); `drop_sync` 1/8 at 120 BPM: onsets on the grid ± 1 sample; same event set at 15 ips drops ≥ 15 dB less at 10 kHz than at 1⅞ |
| Snags, stop | Snag delay excursion ≤ 6 ms, ∫ε over an event = 0 ± 1 %; stop: pitch reaches 0 in `stop_time` ± 5 %, silence after; release: no click, spin-up then lag removed within `stop_time` / 4 + 30 ms |
| Noise | Hiss S/N per model at `hiss` 0 dB ± 1 dB; Follow: input stops → additive noise falls at 120 dB/s ± 10 % and is exactly 0 within 750 ms; moving `input` does not move the noise; Duck: noise −20 dB while playing above −24 dBFS, back within 300 ms; Constant steady ± 0.5 dB |
| Modulation noise | Sidebands around a 1 kHz tone at the model's level ± 2 dB; absent in silence |
| Compander | Encode → decode, core bypassed: unity ± 0.5 dB, tone-burst level error ≤ 0.5 dB, THD of a 41 Hz tone ≤ 0.05 % (detector ripple cancels); on tape with a 20 dB dropout: the expanded dip measures 2× in dB ± 20 % (mistracking is modelled) |
| Mix | `mix` 50 %, wow 0, `input` −12 dB, Studio Reel: within ±1 dB of the wet alone from 40 Hz to 16 kHz (the dry runs the same half-bands) |
| Level | `input` −12 → +24 dB on a −18 dBFS sine: output within ±0.5 dB (the static makeup); on a guitar DI within ±3 dB; every factory preset within ±2 LU of the bypass |
| Rates | 44.1 / 48 / 96 / 192 kHz: small-signal response ±0.5 dB, MOL ±0.5 dB, transport components identical |
| Determinism | Strict null between two renders; block sizes 32 / 512 / 4096 null (ChowTape's per-block drift is the counter-example) |
| CPU | ≤ 1.1 µs / sample mono at defaults, ≤ 2.2 µs worst (Cassette, +18 dB, compander, failures), 48 kHz; the full-scale alternation is reported, not bounded |
| Listening | clean.wav, lead.wav, funk.wav, bass, keys, voice and drum renders: against ChowTape (same speed / bias region), against references supplied by the owner (§7.3), blind and level-matched; tails and silence with a high-gain amp after the block |

## 6. Positioning

| Pedal | Its tape | Difference |
|---|---|---|
| gfx-delay Tape | Loop colour: low-pass, bump, tanh, sine wow, level-dip dropouts, per repeat | gfx-tape is the machine in series, no repeats; its `Transport` and spacing filter may later replace the loop's few lines (sound change, presets re-checked) |
| gfx-chorus Tape engine | A sine wow / flutter on chorus voices, hiss gated | gfx-tape's `mix` 50 % with VHS / Portable is a different chorus: a whole transport, one voice |
| gfx-vibrato | Periodic pitch modulation, depth in cents | gfx-tape's transport is a machine's spectrum, mostly not periodic |
| gfx-crusher | Digital converter | Different medium; both in `lofi`, chained for "sampled from a tape" |
| gfx-drive | Analog clipping | J-A is softer, frequency-dependent through pre-emphasis, and level-referenced |
| gfx-grit | Multi-effect of colour modules; books a "Tape" module | It calls gfx-dsp `tape::TapeSat` (§2.9): the saturation only, by amount. The machine, its transport and its faults stay here |

Against the field: ChowTape's hysteresis and loss physics are covered (with a stable, latency-free,
seeded implementation — its hysteresis voiced as a biased machine by default, ChowTape's own heavier loop under `bias` < 100 %), Generation Loss's failures, models, noise and stop are covered (spread included),
Wow Control's signal-modulated noise and RC-20's follow / duck are covered, iZotope Vinyl's crackle,
rumble, warp and spin-down are covered. Beyond them: wow / flutter in measured units with a weighted meter,
spectra by machine, dropouts that obey wavelength physics and can lock to the grid, companders that
mistrack, azimuth. Not attempted: a learned per-unit capture of specific machines (golem plays captures
through NAM), pitched playback, echo.

## 7. Open questions — settled at the review

1. **J-A along H with relaxation** — **kept, with c = 0.9 at optimum bias.** The integration along H is
   valid (the model is rate-independent) and is the stable form. The relaxation does not remove the
   "past distorts the future" character: the reversible share does. That character is the under-biased
   loop (c ≤ 0.7, ChowTape's range), and it lives on `bias` < 100 % with a slower relaxation (250 ms at
   50 %). No level threshold: J-A erases its own remanence in < 100 ms under any signal (§2.2).
2. **Oversampling** — **8×** (internal ≥ 352.8 kHz). 4× leaves −57 … −66 dB at 12 kHz. CPU ≈ 0.95 µs.
3. **Voicing references**: open, the owner's (drive-boost table of §2.2 and model tables of §2.1 are [I]).
4. **IEC 386 weighting table**: a table is given in §2.3 from the reviewer's memory, still [U]; the
   standard was not reachable. To check before the meter ships; nothing else depends on it.
5. **Units of `wow` / `flutter`** — **% of speed, unweighted peak**, kept. Presets quote `wf_out`.
6. **Defaults** — kept: `noise` on in Follow (never heard alone, §2.4), wow 0.10 %.
7. **`drop_sync`** — kept. `sync` is the modulation's everywhere it is coded (gfx-freqshift's review).
8. **Stop in v1** — kept: 5.4 MB at 192 kHz, allocated once; the buffer covers `stop_time` 3 s.
9. **Vinyl nonlinearity** — replaced by the second-order tracing term (§2.2): one product and one
   difference per sample, and the right harmonic. A contact-geometry model is not worth more here.
10. **Catalogue**: golem-rack's (type `tape`, `lofi`, `role_blocks.drums`; EXP on expert ids).
11. **Shared `tape` module** — in gfx-dsp from the start, for gfx-grit; gfx-delay and gfx-chorus unchanged.
12. **Visual** — champagne `#d8c8a0` anodized (§3); the `ribbon` motif's gaps and oxide specks are what
    separate it from `waves`: keep both gaps even at the smallest tile.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good**, Strong once the core's voicing is confirmed by ear. The architecture is right and is
the best plan surveyed: one chain for every medium, hysteresis integrated along H, bias by its effects
instead of a tone, wavelength physics for the faults, a transport generated as speed, noise that follows
the playing. The arithmetic is almost all correct (checked: spacing loss 54.6 · d / λ and its three
examples; the 17.6 / 7.7 / 3.5 dB figures; every transport frequency of §2.1; head-bump frequencies;
cents; azimuth −2 / −21 dB, 61 µs, 8.2 kHz; vinyl eccentricity; D0 12.7 ms; the buffer size). But the
core as written would not have sounded like an aligned machine, three specifications failed their own
bench rows, and one piece of physics was on the wrong side of the tape. Prototypes [P] are in
`proto-gfx-tape/` of the session scratchpad (numpy, Chowdhury's ferric set, 192 / 384 kHz).

Changes made, and why:

| # | Change | Why |
|---|---|---|
| 1 | **Reversible share c = 0.9 at optimum bias** (0.3 at 50 %, 0.98 at 150 %); makeup = the reversible slope | With Chowdhury's c = 0.17 the low-level gain is −15.4 dB under the stated makeup, rises 10 dB with level, and THD is 2.2 % at −20 dB re a [P]: an expander with crossover distortion at every level, the opposite of "low levels pass at unity". The makeup formula was the anhysteretic slope, exact only at c = 1 |
| 2 | **Substeps n = min(8, ⌈\|ΔH\| / k⌉)** instead of ≤ 2 at a / 4 | "g is bounded" does not make RK4 stable: the irreversible term is a relaxation of length k, and inside the ±8 a clip a step can be 13 k. One or two steps diverge to 10⁶ · M_s on a square or a Nyquist alternation [P]. The a / 4 threshold also spent two steps on every loud sample |
| 3 | **The H-form's advantage restated** | Verified: the time form (RK4, α-transform) gives NaN at Nyquist at any level, the H form does not. On audio-band sines the accuracy gain is 1.2 – 5×, not a difference of kind. Sign changes of ΔH are exact; δ_M switching inside a step is bounded |
| 4 | **8× oversampling** (was 4×, "the bench decides") | [P]: 4× leaves −57 … −66 dB of aliasing for a 12 kHz sine, −70 … −77 dB at 9 kHz; 8× gives −78 … −88 dB. Bench bound set to what 8× does (−76 dB), CPU budget raised to ≈ 0.95 µs |
| 5 | **Record EQ = a drive-boost table, not the standard's time constant** | The standards' constants are the recorded flux falling at HF and a replay *boost*; the draft had them as record pre-emphasis and replay cut. The numbers were right, their place was not: hiss would have come out 17.6 dB dull at 10 kHz on cassette, where real cassette hiss is bright. The HF-saturates-first behaviour is kept, on its real cause (the record amplifier's boost against wavelength losses) |
| 6 | **Relaxation: claim corrected, tied to `bias`** (20 / 50 / 250 ms) | "Unchanged within 2 % above 20 Hz" holds at c = 0.9 (0.6 % at 41 Hz), not under-biased (4.4 %). Open question 1 answered by measurement |
| 7 | **Spacing filter: four one-pole low-passes scaled by v / d** | The three fixed shelves miss the target by 3.2 – 4.6 dB [P]; the scaled design is 1.4 dB everywhere, needs no table and bypasses exactly at d = 0 |
| 8 | **Dry through the same half-bands** | An integer-delayed dry combs: −11.9 dB at 16 kHz at `mix` 50 % [P]. The bench's "no notch deeper than 3 dB" could not pass |
| 9 | **Follow: release linear in dB (120 dB/s), detector before `input`** | The one-pole 300 ms release needs 2.3 s to reach the −90 dBFS switch; the bench asked 500 ms |
| 10 | **Output compensation = −`input` + a static per-model makeup** | −`input` alone loses 4 – 8 dB at high settings (tape compresses); the Level row (±3 dB over 36 dB of `input`) could not pass |
| 11 | **Compander topology specified** (both detectors on the encoded signal) | "Unity ± 0.5 dB" is a property of that topology only. Expander figure corrected (−68 → −136 dB, not "−20 dB") |
| 12 | **Vinyl: y = v − 2κ · v · dv/dt** | v · \|v\| is odd: it made the 3rd harmonic while the text claimed even order. The tracing term is as cheap and correct |
| 13 | **`eq` removed** (25 expert parameters) | The only superfluous one. The others each carry a preset of §4 or a reference's feature |
| 14 | **`tape` in gfx-dsp from the start**, with `TapeSat` for gfx-grit | gfx-grit already books the module; gfx-delay and gfx-chorus keep their own |
| 15 | **Hue `#d8c8a0` champagne** (was `#94461a`) | ΔE 11 from gfx-fuzz's copper, same finish |
| 16 | Wow / flutter: knobs unweighted peak vs weighted specs made explicit; weighting table given [U]; latency cost for parallel lanes stated; bench rows rewritten to match 1 – 12; CPU table | Consequences |

Checked and left as written: the chain and its order; pitch-domain transport with D0 management (rates,
bounds, slews); record and playback passes; snags with zero net lag; stop; dropouts as spacing events and
`drop_sync`; crinkle, crackle, modulation noise; ALC; azimuth; stereo and `spread`; ids, linear ranges,
no mode-dependent default, `bypass`, enums growing at the end; `input` against gfx-crusher's (same id,
same meaning); the Applications table.

Coverage: nothing missing against ChowTape, Generation Loss, Wow Control, RC-20, iZotope Vinyl and the
studio emulations. Left out on purpose and rightly: a bias tone, generations as a parameter, half-speed
playback, a learned capture.

Still open:

| Point | Owner |
|---|---|
| Voicing by ear of c(bias), the drive-boost table, MOL per model, hiss spectra: all [I]. The 100 % default must be compared blind with ChowTape at its default (c ≈ 0.7): if players prefer the heavier loop, the answer is the default `bias` of presets, not c at 100 % | The maintainer / listening |
| Reference recordings per model (§7.3) | The maintainer |
| IEC 386 / AES6 weighting table against the standard | golem-fx, before `wf_out` ships |
| CPU ≈ 0.95 µs / sample (4.6 % of a core, mono): accepted, or 4× with the −60 dB aliasing it measures | The maintainer, after the bench |
| Reproduce the [P] figures in gfx-bench (they come from a piecewise-linear H at the oversampled rate, ideal resampling: the half-bands will add their own) | golem-fx, first coding task |
| Champagne against the family sheet; `RUNE_LIGHT`; the `ribbon` motif at tile size. gfx-fuzz's copper is itself still a proposal: if it is refused, ferric brown is free again | The maintainer |
| New type `tape` in `lofi`, `role_blocks.drums`; EXP on expert ids (`spacing`, `bias`, `dropouts`); `wf_out` display; D0 offset against parallel lanes | golem-rack |
| Add `input` and `level` to the contract's shared-ids table | The maintainer (contract) |

**Note (2026-11-14, after the gfx-radio review):** AM radio lives in gfx-radio, whose channel is simulated in
complex baseband (detuning distortion, fading, static, whistle). It is no longer a deferred `model` option of
this pedal and is never appended to the enum.

**Note (2026-11-15, coding):** the `wf_out` meter implements the weighting from the table of §2.3 (a fit
within 0.3 dB of it). That table is still [U]: it **must be checked against AES6-2008 (IEC 386 / DIN
45507) before the meter is trusted**; until then `wf_out` is a relative reading, and presets should not
quote it as a machine spec.
