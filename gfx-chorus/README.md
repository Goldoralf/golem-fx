# gfx-chorus

> **Status: PROPOSED** — design, no code yet.

A complete chorus, voiced per instrument.

- Parameters: [PARAMS.md](PARAMS.md)
- Voicing per profile and per mode: [VOICING.md](VOICING.md)
- Built on [gfx-dsp](../gfx-dsp/): the chorus holds its voicing and its wrapper; the DSP blocks live in
  the shared library

## Front panel (the golem block)

Profile · Mode · Rate · Depth · Warmth · Mix. Everything else is in expert mode, with defaults set by
the profile.

## Profiles

Guitar Clean · Guitar Drive · Bass · Keys · Voice. A profile sets the anchors of every expert group:
filters, crossover, amount of colour, modulation scale.

## Signal chain

1. Instrument profile
2. Crossover: lows stay dry and mono below an adjustable frequency
3. Input stage: soft saturation, pre-emphasis
4. Delay engine: Clean (high-quality interpolation) · BBD (anti-aliasing / reconstruction filters,
   compander, clock noise) · Tape (wow / flutter)
5. Topology: Classic · Dual (Juno I/II) · Dimension (complementary voices, cross-mix) · Ensemble
   (3–6 voices) · Doubler (random drift)
6. LFO: sine, triangle, rounded triangle, smoothed random, Juno triangle; drift, stereo phase, tempo sync
7. Wet filtering: high-pass, low-pass (warmth)
8. Stereo: width, mono → stereo, mono-safe mode
9. Output: mix, level compensation

## golem constraints

Zero latency (no linear-phase oversampling), `check-block-stall` green on both engines, stable
parameter identifiers, smoothing compatible with the EXP pedal, scene changes without clicks.
