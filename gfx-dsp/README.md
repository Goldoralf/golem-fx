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
7. **Measured, not assumed.** Each block has tests that measure its behaviour (frequency response,
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
| `tape` | Wow and flutter generators, tape hiss | gfx-chorus (Tape engine) |
| `noise` | White, pink, band-limited noise; deterministic seeds for tests | gfx-chorus |
| `stereo` | Mid/side, width, mono-safety helpers | gfx-chorus |
| `level` | Loudness estimate for level compensation | gfx-chorus |

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
