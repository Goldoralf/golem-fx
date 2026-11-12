# gfx-chorus

> **Status: PROPOSED** — design, no code yet.

A complete chorus, voiced per instrument. Stands for golem's `chorus` block (ids `rate`, `depth`, `mix`;
[golem contract](../docs/golem-contract.md)).

- Parameters: [PARAMS.md](PARAMS.md)
- Voicing per profile and per mode: [VOICING.md](VOICING.md)
- Built on [gfx-dsp](../gfx-dsp/): the chorus holds its voicing and its wrapper; the DSP blocks live in
  the shared library

## State of the art

Survey of 2026-11-12. **[V]** read in the source; **[U]** forum or unverified.

### Commercial references

| Product | What it does best | What we take |
|---|---|---|
| [Arturia Chorus DIMENSION-D / JUN-6](https://www.arturia.com/products/software-effects/chorus-dimension-d/overview) [V] | Authentic fixed modes measured on original units, plus advanced controls (LFO shapes, width, "Dynamic Color" = compander, manual rate / depth, sync) | Authentic modes first, expert controls second |
| [TAL-Chorus-LX](https://tal-software.com/products/tal-chorus-lx) [V] | A minimal Juno-60 whose voicing is right | The bar for Dual: few controls, exact sound |
| [Eventide TriceraChorus](https://www.eventideaudio.com/products/effects/chorus-plugin/tricerachorus) [V] | Three-voice L / C / R BBD chorus, micro-pitch detune, **envelope-controlled rate and mix** | Envelope follower → depth / mix |
| [Strymon Mobius](https://www.strymon.net/manuals/Mobius_UserManual_REVF.pdf) [V] | "dBucket": BBD with a **logarithmic LFO**; "Multi": three lines per channel, no warble at high mix | Log LFO shape |
| [ValhallaSpaceModulator](https://valhalladsp.com/shop/modulation/valhalla-space-modulator/) [U] | 11 algorithms incl. through-zero, detune, doubling | Doubler; through-zero belongs to a future flanger |
| UVI Thorus, D16 Syntorus 2 [U] | Morphable 2 – 8 voices, clean / BBD engines | Engine choice |

What the best have in common: exact hardware rates and delays **with** the surrounding filters and
companding; a non-sinusoidal, smoothed or log LFO; modulation that does not sound like a moving pitch
(complementary voices, constant detune); dynamics-aware modulation.

### Open source (all compatible with GPL-3)

| Project | Idea | Licence |
|---|---|---|
| [jpcima/ensemble-chorus](https://github.com/jpcima/ensemble-chorus) | 3 – 6 BBD lines, slow + fast LFO pairs, LFO randomness, Raffel & Smith BBD | BSL-1.0 |
| [jpcima/rc-effect-playground](https://github.com/jpcima/rc-effect-playground) | Juno-60 chorus (Hera), BBD filters, Juno reference recordings | ISC |
| [chowdsp_utils BBD](https://github.com/Chowdhury-DSP/chowdsp_utils/tree/master/modules/dsp/chowdsp_dsp_utils/Delay/BBD) | Holters & Parker BBD with input / output filter banks | GPL-3 (per module) |
| [Surge XT BBD Ensemble](https://github.com/surge-synthesizer/surge/blob/main/src/common/dsp/effects/BBDEnsembleEffect.cpp) | Shared-voice stereo (L = 1 + 2, R = 2 + 3), filters tracking the clock (`f · clock^0.75`) | GPL-3 |
| [Choroboros](https://github.com/EsotericShadow/choroboros-open-source) | Interpolation menu, BBD 5th-order filtering | GPL-3 |
| [pendragon-andyh/Juno60](https://github.com/pendragon-andyh/Juno60/blob/master/Chorus/README.md) | Measured Juno-60 chorus rates, delays, filters | MIT |
| [AnalogTapeModel](https://github.com/jatinchowdhury18/AnalogTapeModel), [Vital](https://github.com/mtytel/vital) | Tape wow / flutter | GPL-3 |

### Papers

- Dattorro, *Effect Design Part 2: Delay-Line Modulation and Chorus*, JAES 1997
  ([PDF](https://ccrma.stanford.edu/~dattorro/EffectDesignPart2.pdf)).
- Laakso, Välimäki et al., *Splitting the Unit Delay*, IEEE SPM 1996: for a **modulated** delay, Lagrange
  3 – 5 or windowed sinc; the first-order allpass interpolator produces transients when modulated.
- Raffel & Smith, *Practical Modeling of Bucket-Brigade Device Circuits*, DAFx 2010.
- Holters & Parker, *A Combined Model for a BBD and its Input and Output Filters*, DAFx 2018.
- Gabrielli, D'Angelo & Squartini, *Antialiasing in BBD Chips Using BLEP*, DAFx 2025
  ([PDF](https://dafx25.dii.univpm.it/wp-content/uploads/2025/09/DAFx25_paper_29.pdf)) [V]: keeps the BBD's
  wanted aliasing, removes the simulation's spurious one, no oversampling — zero latency.

### What we conclude

1. **Modes are reference units with measured values** (CE-2, Juno I / II / I+II, Dimension 1 / 2 / 3),
   not generic topologies. See [VOICING.md](VOICING.md) §2.
2. **The BBD engine works in the clock domain**: the LFO moves the clock, delay = N / (2 f_clk), the
   anti-alias and reconstruction filters track the clock, and BLEP removes the simulation's aliasing.
3. **Emphasis, not companding, for the CE-2 and the Juno**; companding for the Dimension.
4. **Interpolation**: Lagrange 3, Lagrange 5 (default), windowed sinc. No allpass.
5. **Added**: envelope follower → depth / mix, detune lock, log LFO, analogue imperfection, shared-voice
   stereo for mono safety.

## Front panel (the golem block)

Profile · Mode · Rate · Depth · Warmth · Mix. Everything else is in expert mode, with defaults set by
the profile and the mode.

## Profiles

Guitar Clean · Guitar Drive · Bass · Keys · Voice. A profile sets the anchors of every expert group:
filters, crossover, amount of colour, envelope follower, stereo, modulation scale.

## Signal chain

1. Instrument profile
2. Crossover: lows stay dry and mono below an adjustable frequency
3. Input stage: soft saturation, pre-emphasis
4. Delay engine: Clean (Lagrange / sinc interpolation) · BBD (clock-domain model, tracking filters, BLEP,
   compander, clock noise) · Tape (wow / flutter)
5. Topology: Classic (CE-2) · Dual I / II / I+II (Juno-60) · Dimension 1 / 2 / 3 · Ensemble (Solina) ·
   Doubler
6. LFO: sine, smoothed triangle, rounded, smoothed random, log; drift, imperfection, stereo phase, tempo
   sync, detune lock
7. Envelope follower → depth, mix
8. Wet filtering: high-pass, low-pass (warmth)
9. Stereo: width, mono → stereo, mono-safe mode
10. Output: mix, level compensation

## golem constraints

Zero latency (no linear-phase oversampling; BLEP instead of oversampling in the BBD), `check-block-stall`
green on both engines, stable parameter ids, deterministic modulation (phase 0, fixed seed at reset),
smoothing compatible with the EXP pedal, scene changes without clicks.
