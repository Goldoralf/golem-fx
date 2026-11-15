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
| `svf` | Trapezoidal (TPT) state-variable filter and first-order allpass; matched coefficient designs (Vicanek, Orfanidis); coefficient interpolation | eq, wah, autowah, phaser, deesser, every tone filter |
| `adaa` | First-order antiderivative antialiasing for memoryless shapers, incl. "residual" form (exact below the knee) | drive, limiter, comp, tremolo, rotary |
| `oversample` | 2× / 4× polyphase IIR half-band (minimum phase); a matching "identity" path so a dry signal stays phase-coherent with the processed one | drive, comp, octaver |
| `envelope` | Peak, RMS, dB-domain followers; sliding maximum (O(1)); relative (band vs full-band) detector | gate, comp, deesser, eq, autowah, duckers |
| `exp_smooth` | The EXP-pedal smoother of the golem contract (interval-adaptive ramp + critically damped smoother + slew limit) | every EXP-capable parameter |
| `transport` | Host tempo / position → phase; sync divisions; relocation handling | every synced LFO, delay, killswitch |
| `fdn` | Hadamard / Householder mixing, multichannel diffuser, Jot attenuation filters, seeded delay sets | reverb, shimmer |
| `shifter` | Delay-line pitch shifter (2 – 3 heads, equal-power or splice-search), band-split variant | pitch, shimmer, detune |
| `stft` | Real-time STFT, phase-gradient phase vocoder | pitch (Poly, if retained) |
| `filterbank` | ERB complex IIR filterbank, IIR Hilbert pair | octaver (Poly), barberpole |
| `motor` | First-order inertia with separate accelerate / decelerate constants, angle integration | rotary, vibrato rise |
| `onset` | Zero-latency onset detectors, each usable alone, all relative to the playing level: `Level` (note level, phrase level, floor) and `LevelGate` (L); `Pick` (A: fast − slow envelope in dB with a slope test against the last period's peak; its own 1.5 kHz high-passed envelope or an **external level in dB**; R, slope window, floor, refractory as arguments); `Flux` (B: 16-band shape flux against a peak-held maximum filter, tremolo-corrected, adaptive threshold); `PitchJump` (C: YIN note changes, glides drag the reference, integer-ratio jumps ignored); `Strum` (one onset per strum, same-note hold-off, slides merged) | gfx-swell (coded); gfx-ducker (`Pick` on its sliding maximum), gfx-grains, gfx-killswitch (later, null-guarded) |
| `swell` | The swell's gain engine: five rise curves with inverses (Square, Linear, S-Curve, dB, Pedal), rise / hold / bow decay / spent, raised-cosine retrigger dip continued through c⁻¹, scheduled (lookahead) triggers, reset fade | gfx-swell; gfx-delay, gfx-reverb, gfx-shimmer `swell` (later) |
| `divider` | Adaptive Schmitt trigger + flip-flops (analog octave divider) | octaver |
| `geq` | Accurate graphic EQ (ACGE with fill-in sections): fill-in rule, Q from the edge gain, monotone cubic (PCHIP) target, fixed-centre matched bell (cached fitting grid), per-layout plan (interaction matrix, regularised pseudo-inverse) with cold solve and warm-started chord correction, all fixed-size | geq |
| `ssb` | Single sideband: the elliptic all-pass Hilbert pair designed per rate (band 15 Hz – fs / 2 − 15 Hz, exact nome; 8 + 8 sections up to 50 kHz, 10 + 10 above; image −87 dB at 48 kHz), signed phasor, complex shifter with sideband balance, the pair's second pass as a negative-frequency filter | freqshift; ringmod (Upper / Lower); delay, octaver, glitch when they leave `pitch::Hilbert` |
| `blep` | One minimum-phase band-limited kernel (Kaiser prototype, cepstral minimum phase; 32 taps × 64 phases, pass band ±0.5 dB to 0.35 fs, stop band −102 dB from 0.6 fs, 2.6 samples of group delay): a `Reader` (the input at a fractional instant, and its running integral), a `Renderer` (minBLEP steps of a held signal), the FIR that matches a read + render pair for a parallel dry | crusher (later: delay's Lo-Fi) |
| `converter` | A virtual ADC → DAC pair, halves callable apart: the `Clock` (f64 phase accumulator, seeded jitter), the `Adc` (clip, TPDF dither, Linear / µ-law / Float words, Tread / Riser / Truncate), the `Dac` (R-2R ladder errors, power-law curve), integrate-and-dump sampling, the tails window | crusher, radio (its codec between the halves) |

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
