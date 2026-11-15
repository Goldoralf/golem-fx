# gfx-dsp

> **Status: PROPOSED** — design, no code yet.

The shared DSP library of golem-fx: the building blocks every effect uses, written once, tested once.
An effect (`gfx-chorus`, later `gfx-flanger`, `gfx-vibrato`…) is a thin assembly of these blocks plus
its voicing and its nih-plug wrapper.

## Rules

1. **No plugin dependency.** gfx-dsp knows nothing of nih-plug, VST3 or a host. It processes buffers of
   `f32` samples. That keeps it testable offline and reusable by the render tool of the listening bench.
2. **Real-time safe.** Nothing on the audio path allocates, locks, logs or does I/O. Memory is sized in
   `prepare(sample_rate, max_block)` and never again.
3. **Zero latency** unless a block says otherwise in its name and its documentation. Oversampling uses
   minimum-phase IIR filters.
4. **Denormals and NaN handled at the source**: flush-to-zero on the audio thread, and every recursive
   block (filters, feedback) is tested with silence after a burst.
5. **Sample-rate independent**: every block is tested at 44.1, 48, 96 and 192 kHz.
6. **A block = a struct with `prepare`, `reset`, `process`.** Parameters are plain setters; smoothing is
   the caller's choice, through the smoothing block.
7. **Deterministic.** Every LFO and random source starts from a fixed state on `reset` (phase 0, fixed
   seed). Two renders of the same input give the same output, bit for bit
   ([golem contract](../docs/golem-contract.md)).
8. **Measured, not assumed.** Each block has tests that measure its behaviour (frequency response,
   interpolation error, aliasing, noise floor), not just that it runs.

## Modules

| Module | Content | First user |
|---|---|---|
| `delay` | Circular delay line, fractional read with interpolators: linear (reference only), Hermite, Lagrange (order 3 and 5), Thiran allpass, windowed sinc | gfx-chorus (Clean engine) |
| `lfo` | Sine, triangle, rounded triangle, smoothed random, Juno triangle; phase offsets, tempo sync, drift | gfx-chorus |
| `smooth` | One-pole and linear parameter smoothers, crossfader for structural changes | every effect |
| `filter` | Biquads (RBJ), one-pole, state-variable; Linkwitz-Riley crossovers (2nd and 4th order) | gfx-chorus (crossover, wet filters) |
| `sat` | Soft saturators (tanh, asymmetric, diode-like), with optional 2×/4× minimum-phase oversampling | gfx-chorus (input stage) |
| `bbd` | Bucket-brigade model: variable-clock sampling, anti-aliasing and reconstruction filters, clock noise, stage count | gfx-chorus (BBD engine) |
| `dynamics` | Compander (NE570-style), envelope followers | gfx-chorus (compander) |
| `tape` | Jiles-Atherton hysteresis integrated along H (RK4, substeps n = min(8, ⌈\|Δh\| / k⌉), soft clip at ±8 a, relaxation toward the anhysteretic, bias → reversible share and relaxation), `TapeSat` (record shelf → J-A → exact inverse shelf → reversible-slope makeup, at the caller's rate), the static makeup table, first-order shelves with exact inverses, the spacing-loss filter (four magnitude-matched one-poles scaled by v / d), seeded band noise and rotating-part wobbles | gfx-tape; gfx-grit's Tape module (`TapeSat`). gfx-delay and gfx-chorus keep their own tape code |
| `circuit` | Small linear circuits from a netlist (R, C, ideal op-amps): MNA with trapezoidal companion models (exact bilinear), pots changed without touching the state; Laplace-domain analysis for tests | gfx-shem (boost network), gfx-emet (808 tone stage) |
| `noise` | White, pink, band-limited noise; deterministic seeds for tests | gfx-chorus |
| `stereo` | Mid/side, width, mono-safety helpers | gfx-chorus |
| `level` | Loudness estimate for level compensation | gfx-chorus |

Added at the design review (2026-11-12), needed by the 22 designs:

| Module | Content | Users |
|---|---|---|
| `svf` | Trapezoidal (TPT) state-variable filter and first-order allpass; matched coefficient designs (Vicanek, Orfanidis); coefficient interpolation; `SvfCoefs` (precomputed TPT coefficients for tables, `set_coefs_ramped`); `OnePoleTpt`, a TPT one-pole whose cut-off may move every sample, with `tan_fast` (a [7/6] continued fraction, < 1e-5 up to 0.45 fs) | eq, wah, autowah, phaser, deesser, every tone filter; transient (the attack shelf); ducker (Spectral tables) |
| `adaa` | First-order antiderivative antialiasing for memoryless shapers, incl. "residual" form (exact below the knee); `Adaa1Fn` for any `Antiderivative` (parameters may move), `TanhResidual`, Parker's diode law (`ParkerDiode`, `DiodePair`) | drive, limiter, comp, tremolo, rotary |
| `oversample` | 2× / 4× polyphase IIR half-band (minimum phase); a matching "identity" path so a dry signal stays phase-coherent with the processed one; `OversamplerN`: 1× – 16× f64 cascade (the designs' `Oversampler8x` / `16x` = 3 / 4 stages) | drive, comp, octaver; gfx-dist, gfx-fuzz |
| `envelope` | Peak, RMS, dB-domain followers; sliding maximum (O(1)); relative (band vs full-band) detector | gate, comp, deesser, eq, autowah, duckers |
| `exp_smooth` | The EXP-pedal smoother of the golem contract (interval-adaptive ramp + critically damped smoother + slew limit) | every EXP-capable parameter |
| `transport` | Host tempo / position → phase; sync divisions; relocation handling | every synced LFO, delay, killswitch |
| `fdn` | Hadamard / Householder mixing, multichannel diffuser, Jot attenuation filters, seeded delay sets | reverb, shimmer |
| `shifter` | Delay-line pitch shifter (2 – 3 heads, equal-power or splice-search), band-split variant | pitch, shimmer, detune |
| `stft` | Real-time STFT, phase-gradient phase vocoder | pitch (Poly, if retained) |
| `filterbank` | ERB complex IIR filterbank, IIR Hilbert pair | octaver (Poly), barberpole |
| `motor` | First-order inertia with separate accelerate / decelerate constants, angle integration | rotary, vibrato rise |
| `onset` | Zero-latency onset detectors, each usable alone, all relative to the playing level: `Level` (note level, phrase level, floor) and `LevelGate` (L); `Pick` (A: fast − slow envelope in dB with a slope test against the last period's peak; its own 1.5 kHz high-passed envelope or an **external level in dB**; R, slope window, floor, refractory as arguments); `Flux` (B: 16-band shape flux against a peak-held maximum filter, tremolo-corrected, adaptive threshold); `PitchJump` (C: YIN note changes, glides drag the reference, integer-ratio jumps ignored); `Strum` (one onset per strum, same-note hold-off, slides merged) | gfx-swell (coded); gfx-ducker (coded: `Pick` on its sliding maximum), gfx-grains, gfx-killswitch (later, null-guarded) |
| `swell` | The swell's gain engine: five rise curves with inverses (Square, Linear, S-Curve, dB, Pedal), rise / hold / bow decay / spent, raised-cosine retrigger dip continued through c⁻¹, scheduled (lookahead) triggers, reset fade | gfx-swell; gfx-delay, gfx-reverb, gfx-shimmer `swell` (later) |
| `sidechain` | The detector's level for the "one period of the lowest note" dynamics blocks: the window rule W = clamp(1.2 / max(`hpf` / divisor, 30 Hz), 8 ms, 40 ms) (divisor 1 = gfx-gate's close detector, 2 = gfx-transient), `ScFilter` (Butterworth high-pass on from 10 Hz, low-pass off from 20 kHz), `Phrase` (a peak level held 5 s, then −3 dB / s, never under the floor). The peak itself is `envelope::SlidingMax` | gfx-transient, gfx-ducker (coded); gfx-gate, gfx-swell's `onset` when next touched (null-guarded) |
| `duck` | The ducking engine: activation target with knee and hysteresis (`KeyGate`), linear activation with hold (`Activation`), raised-cosine map and inverse, the fixed-shape dip (raised-cosine fall, hold, `tension` recovery in linear gain; restartable from the current gain, or read from a phase), Spectral: 16 half-octave bands from 56.6 Hz (Q 2), 4th-order analysis with 2 / 60 ms followers, weights from `spread`, the bells' interaction matrix, cut-only projected Gauss-Seidel, per-band coefficient tables (0 – 60 dB, 0.5 dB steps) built in `prepare` | gfx-ducker (coded); gfx-delay, gfx-reverb, gfx-shimmer ducking (later, audible change) |
| `transient` | The differential transient detector (SPL's principle in dB): floor 48 dB under the phrase level, attack pair (E_f against a one-pole attack E_s), sustain pair (10 ms release against a `sus_time` release), one law share(D) = 1 − exp(−max(0, D − 1.5 dB) / D_ref), D_ref = 22 − 20 · `sens`; level-independent above `thresh`. `Ramp`: the lookahead engine of a share (moving maximum, two boxes, a delay of L − R + 2: full at the onset) | gfx-transient (coded); gfx-comp or drum blocks later |
| `divider` | Adaptive Schmitt trigger + flip-flops (analog octave divider) | octaver |
| `geq` | Accurate graphic EQ (ACGE with fill-in sections): fill-in rule, Q from the edge gain, monotone cubic (PCHIP) target, fixed-centre matched bell (cached fitting grid), per-layout plan (interaction matrix, regularised pseudo-inverse) with cold solve and warm-started chord correction, all fixed-size | geq |
| `ssb` | Single sideband: the elliptic all-pass Hilbert pair designed per rate (band 15 Hz – fs / 2 − 15 Hz, exact nome; 8 + 8 sections up to 50 kHz, 10 + 10 above; image −87 dB at 48 kHz), signed phasor, complex shifter with sideband balance, the pair's second pass as a negative-frequency filter | freqshift; ringmod (Upper / Lower); delay, octaver, glitch when they leave `pitch::Hilbert` |
| `blep` | One minimum-phase band-limited kernel (Kaiser prototype, cepstral minimum phase; 32 taps × 64 phases, pass band ±0.5 dB to 0.35 fs, stop band −102 dB from 0.6 fs, 2.6 samples of group delay): a `Reader` (the input at a fractional instant, and its running integral), a `Renderer` (minBLEP steps of a held signal), the FIR that matches a read + render pair for a parallel dry | crusher (later: delay's Lo-Fi) |
| `circuit_nl` | Nonlinear circuits solved per sample by the nodal DK method (`NonlinearCircuit`, its own `Netlist`; the linear `circuit` is untouched): R, C (trapezoidal or backward Euler), L, ideal op-amps and buffers, sources from an input vector u (signals, supply), variable resistors and pots with tapers (Woodbury update every sample, ends 10 Ω), devices with run-time constants — clippers (Shockley strings, exact series collapse, MOSFET channel), BJT (Ebers–Moll NPN / PNP, SPICE temperature law, collector-junction leakage scale), op-amp macromodel (tanh or square-law stage into a fixed 30 pF: SR and GBW from one law, A₀ leak, soft-plus rail clamps following the supply). Solve: guarded extrapolated seed, Newton with SPICE limiting and Armijo backtracking, then the previous solution, then plain limited steps, then continuation in p (4, 16), then hold (counted); explicit 1 – 2-port solves, pivoted LU above, up to 12 ports. Stage graph from declared exact cuts; monolithic reference mode; DC point (gmin, seed, continuation, source stepping) solved inline; small-signal `ac_response`; `to_spice()` decks; shared `fast_exp` (Estrin, < 10⁻⁹) | gfx-dist (coded); gfx-fuzz |
| `pot` | Taper laws: B linear, A (gfx-emet's exponential, ≈ 9 % at half), C, W | gfx-dist; gfx-fuzz |
| `converter` | A virtual ADC → DAC pair, halves callable apart: the `Clock` (f64 phase accumulator, seeded jitter), the `Adc` (clip, TPDF dither, Linear / µ-law / Float words, Tread / Riser / Truncate), the `Dac` (R-2R ladder errors, power-law curve), integrate-and-dump sampling, the tails window | crusher, radio (its codec between the halves) |
| `track` | The phase-locked partial tracker (gfx-sustainer §2.7's API): complex demodulation at the NCO's phase averaged over one period (a comb on every harmonic), PI loop of bandwidth clamp(f0 / 2.5, 14, 100) Hz whose integrator holds the frequency within ±1 st of the feed-forward, Snap / Keep engage, hold at the 150 ms mean, partials at k · ψ with a ±15 ¢ trim on a 4 Hz loop, weak-fundamental reference | ringmod (Track); sustainer, polysynth |
| `crossover` | Linkwitz-Riley crossovers in three layers, **one set of names for the suite** (gfx-bassdi §2.9 names them; gfx-grit's `LrSplit` / `LrTree<N>` are `Split` / `Tree` here, gfx-mbcomp cites both): (1) `Split`, a two-way LR2 / LR4 / LR8 split giving `low`, `high` and their sum, the all-pass **twin**, from one tick (the high band computed as twin − low; LR2's high is the inverted high-pass, so low + high = twin in every order); (2) `BandGain`, twin + (g − 1) · band (phase = the twin's for every g ≥ 0, bit-identical to it at g = 1), and its stand-alone `Twin` for a parallel path; (3) `Tree`, N ≤ 6 bands with the twins of the splits each band did not pass (Prisma's topology). TPT one-poles / SVFs, f64 audio and state, log-Hz glide per split; cost per split 2 one-pole / 2 / 5 SVF ticks (LR2 / 4 / 8), per twin 1 / 1 / 2. Measured: section vs twin phase ≤ 4.1e−9 rad (LR2 / 4 / 8, 44.1 – 192 kHz, g 0 – +12 dB), tree sum = the twins' cascade within 1e−12, band phases equal within 1e−5 rad | gfx-bassdi, gfx-grit (coded); gfx-mbcomp (layers 1 and 3; it adds `BwTree` and `Linearizer` as a fourth layer); gfx-drive, gfx-emet, gfx-flanger when next touched |

Numerics: audio is `f32`; recursive filter states, phase accumulators and angles are `f64` (a 20 Hz filter
at 192 kHz loses precision in `f32`). Multi-line structures (FDN, filterbanks, multi-voice) are written for
SIMD lanes from the start.

Modules are written when an effect needs them, not ahead of time. A module moves here from an effect
only once its interface is settled.

## Workspace

```
golem-fx/
├── Cargo.toml        workspace
├── gfx-dsp/          this library
├── gfx-chorus/       the chorus: voicing + nih-plug wrapper (VST3)
├── gfx-bench/        offline render and measurement tool (DI in → WAV out, reports)
└── xtask/            nih-plug bundler (VST3)
```
