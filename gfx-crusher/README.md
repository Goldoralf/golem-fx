# gfx-crusher

> **Status: DESIGNED** (2026-11-14) — design only, no code. Reviewed the same day by Fable 5.2: verdict
> **Strong**, fixes applied in place (see the Review section at the end).

golem's **bitcrusher and sample-rate reducer**. It stands for golem's `bitcrusher` block (today MDA
Degrade) and keeps its parameter ids `quant`, `rate` and `nonlin` ([golem contract](../docs/golem-contract.md)).

The idea: **a virtual converter, simulated exactly**. The pedal models an ADC → DAC pair running at its
own clock (any rate, fractional, moving) and its own word format (linear, companded, float, ADPCM, delta).
Everything that should alias, aliases, because the virtual converter aliases. Nothing else does: the
host-rate rendering is band-limited, so there is no zipper, no inharmonic fizz from the host grid, and
sweeping the rate sounds like turning a real sampler's clock. Around it sits what players actually
hear as "sampler warmth" — the anti-alias and reconstruction filters — and a fix for what they hate:
fizz on silence and tails that sputter then cut.

It is **not** a codec emulator (gfx-glitch does spectral damage with frames and latency), not an
in-loop degrader (gfx-delay's Lo-Fi model) and not a distortion (gfx-drive). Zero reported latency (minimum-phase
kernels, a few samples of group delay: §2.4), mono 1 / 1.

## 1. State of the art

Survey of 2026-11-14. **[V]** read in a source, a manual or the code · **[I]** inference, or a fact not
re-checked against a primary source · **[A]** our calculation.

### What golem plays today: MDA Degrade

Read in [mda-lv2's `mdaDegrade.cpp`](https://gitlab.com/drobilla/mda-lv2/-/raw/main/src/mdaDegrade.cpp)
and `Degrade.ttl` [V]. Six parameters; golem's catalogue exposes three (Quant, Rate, Non-Lin) and leaves
the others at the LV2 defaults (Headroom 0.8, Post Filter 0.9, Output 0.5).

| MDA param (0 – 1) | Code | Meaning |
|---|---|---|
| Headroom `h` | `clp = 10^(−1.5 + 1.5h)` | Hard clip of the output at 30·(h − 1) dB. Default 0.8 → **−6 dBFS** |
| Quant `q` | `g1 = 2^(2 + int(12q))`, `y = int(b0 · g1) / (2 g1)` with `b0 = L + R` | **bits = 4 + ⌊12q⌋** (4 – 16) for a mono source on both inputs. `int()` truncates toward zero: a 2-LSB dead zone at zero |
| Rate `r` | `tn = int(e^(18·|r − 0.5|))` | Hold for `tn` host samples: **R = fs / ⌊e^(18·|r − 0.5|)⌋**. r = 0.5 → no reduction; 0 or 1 → fs / 8,103 (5.9 Hz at 48 kHz). Integer divider: the rate moves in steps and depends on the host rate |
| (side of 0.5) | `mode = r > 0.5` | r < 0.5: **plain sample-and-hold** (raw aliasing). r > 0.5: **integrate-and-dump** (a box average over the period, then hold) |
| Post Filter `f` | 8 one-poles in cascade | Nominal 10^(2.3 + 2f) Hz, but 8 poles: the true −3 dB point is lower [A]. Default 0.9 → nominal 12.6 kHz, **−3 dB at 9.3 kHz** (48 kHz), −3.4 dB at 10 kHz |
| Non-Lin `p` | `|y|^e`, e = 10^(−0.15·a), a = 2·|p − 0.5| | **0.5 = linear.** p > 0.5: both polarities (odd, "thickening"); p < 0.5: one polarity only (even). Display 200·|p − 0.5| % |
| Output `o` | `10^(2o − 1)` | −20 … +20 dB, 0.5 = 0 dB |

Also [V]: the output is **mono** ((L + R) / 2 on both outputs), **polarity-inverted** (`g1` is negative),
and the hold counter restarts on every parameter change (`tcount = 1` in `setParameter`: the alias pattern
jumps at each automation step). The power curve acts on the quantised value, referenced to full scale:
it raises a level L dBFS by 0.29·|L| dB at 100 % (+5.8 dB at −20 dBFS, +11.7 dB at −40). Non-Lin's unit
label reads "Odd<>Eve", the reverse of the code (the left half bends one polarity: even).

Two findings for golem [A]:

1. **golem's `nonlin` = 0 is MDA's maximum even distortion**, not "off": the catalogue default and every
   preset assume 0 = clean. "Standard" (q 0.5, r 0.5, p 0) plays today as 10 bits, no rate reduction,
   one-sided power curve e = 0.708, −6 dBFS hard clip, 9.3 kHz post filter.
2. **The presets were set blind to the curves.** "8-bit" (q 0.65) is 11 bits; "AM radio" (r 0.25) holds
   for 90 samples: R = 533 Hz.

### Hardware references

| Reference | What defines its sound | What we take |
|---|---|---|
| 12-bit drum sampler, 26.04 kHz ([Rossum SP-1200 reissue](https://www.rossum-electro.com/products/sp-1200/)) [V] | 12-bit linear, 26.04 kHz; the reissue keeps "the original's aliasing & imaging artifacts"; filtered **and** unfiltered outputs; analog ladder filter on some channels ([E-A-SKI page](https://www.rossum-electro.com/products/sp-1200-e-a-ski-special-edition)) | Aliasing kept on purpose, reconstruction filter optional: the **Drum Machine** model |
| 12-bit rack sampler (S950 class) [I] | Steep 6th-order Butterworth low-pass ([gearnews on SP950](https://www.gearnews.com/wavetracing-sp950-the-sound-of-the-sp-1200-and-s950-in-one-plug-in/)); forum consensus: its "warmth" is the filter softening the aliasing, both are 12-bit ([gearspace](https://gearspace.com/threads/s950-vs-sp1200.113961/)) | The **Sampler** model: warmth = filter, not bits |
| 8-bit companded drum machines (DMX, LinnDrum) [V] | 8-bit words through a **µ-law DAC** (AM6070), ≈ 12-bit dynamic range ([Wikipedia DMX](https://en.wikipedia.org/wiki/Oberheim_DMX), [Electric Druid](https://electricdruid.net/?p=3184)) | **µ-law** format: 8 bits that do not fizz on tails |
| Early 8-bit keyboard samplers (Fairlight, Mirage, Emulator) [I] | 8-bit, ≈ 20 – 33 kHz, analog voice filters | Presets (format + rate + model) |
| Telephony G.711 [I] | µ-law (µ = 255) / A-law (A = 87.6), 8 bits, 8 kHz, 300 – 3,400 Hz band | µ-law, `low_cut` / `high_cut` (A-law: not kept, §2.2) |
| 8-/16-bit consoles and voice chips [I] | 4-bit ADPCM (arcade voice chips, IMA), console BRR + **4-point Gaussian interpolation** (the muffled 32 kHz console sound), 1-bit delta (DPCM) | **ADPCM** and **Delta** formats, **Console** model |
| Early digital delays [I] | 8 – 12-bit, low clock, sometimes delta modulation | Presets |

### Plugins and code

| Reference | What it does best | What we take |
|---|---|---|
| [D16 Decimort 2](https://d16.pl/decimort2) [V, vendor] | "Approximative" pre-filter tied to the resample rate; "Image" post-filter; **jitter** (short random fluctuations of the rate); **mid-tread vs mid-riser** quantiser ("drastically different dynamics response"); dither; "zero internal aliasing" | All five, as orthogonal controls |
| [Airwindows DeRez2 / DeRez3](https://github.com/airwindows/airwindows) (MIT) [V] | **Fractional rate** by phase accumulator; the held value interpolated between the two input samples around the hold instant; continuous bit depth; µ-law as the "soft" side; DeRez3 reconstructs with a Bézier curve (a smooth "reconstruction") | Fractional clock with sub-sample sampling; continuous bits |
| [Surge XT BitCrusher](https://github.com/surge-synthesizer/sst-effects) (GPL-3) [V] | Phase-accumulator S&H, continuous bit depth (2^bd), a **zero-point offset** (shifts the quantiser grid), **key-tracked rate**, SVF after | Pitch-tracked rate (`track`), on guitar through a pitch detector |
| MDA Degrade (above) [V] | Integrate-and-dump mode, power-law DAC | **Average** model; `nonlin` / `asym` |
| [TAL-Bitcrusher](https://tal-software.com/products/tal-bitcrusher), [Tritik Krush](https://www.kvraudio.com/product/krush2-by-tritik) [V] | Drive → crush → resonant filter, LFO modulation | Rate modulation (LFO, envelope) |
| [Goodhertz Lossy](https://manuals.goodhertz.com/3.13/lossy) [V] | Codec-like spectral loss, packet loss / repeat | Out of scope: gfx-glitch's territory |
| Plogue Chipcrusher [I] | Chip-console converter emulation | Confirms ADPCM / delta as converter formats, not codecs |
| ChowDSP [I] | No dedicated crusher found; his ADAA work is the general reference for antialiased nonlinearities | See §2.4: here nothing needs ADAA |

### Research on band-limited steps

- A sample-and-hold output is a staircase; rendered naïvely on the host grid its steps alias. **BLEP**
  (band-limited step) corrections fix this (Välimäki, Pekonen & Nam, JASA 2012
  ([pdf](https://mac.kaist.ac.kr/pubs/ValimakiPeknenNam-jasa2012.pdf)); Esqueda, Bilbao & Välimäki on clipped
  signals) [V]. A DAFx25 BBD paper finds BLEP effective on the stepped output, and **input interpolation
  of minor importance** ([DAFx25 paper 29](https://dafx25.dii.univpm.it/wp-content/uploads/2025/09/DAFx25_paper_29.pdf)) [V].
  That second result does **not** carry over here: a BBD sits behind an anti-alias filter and a high
  clock; a crusher's Raw model has no filter, so the interpolator's leftover images (linear: only −38 dB
  for a 5 kHz partial at 48 kHz [A]) would be sampled and folded as aliases no converter makes (§2.2).
- **minBLEP** (minimum-phase BLEP, Brandt 2001) puts the whole correction after the step: causal, no
  look-ahead, nothing to report. It is not delay-free: the minimum-phase low-pass has a few samples of
  group delay (as gfx-dsp `oversample`'s half-bands) [I, standard].

### Forum feedback: what players love and hate

- **Fizz on silence / tails**: at low bits, noise "while not playing" and tails that buzz then cut; the
  standard advice is a gate after it, whose action is itself audible on tails ([Eventide](https://www.eventideaudio.com/?p=128574),
  [PedalPCB](https://forum.pedalpcb.com/threads/captain-bit-noise-floor.12234/latest)) [V].
- **Level jumps** with the bit depth (a 1-bit mid-riser is a −6 dBFS square whatever the input; a
  mid-tread at 4 bits swallows everything below −24 dBFS) [A]. This is the converter itself: not
  "fixed", but handled by `input` (use of the bits, output compensated) and level-matched presets.
- **Zipper** when sweeping the rate: integer dividers jump (MDA, gfx-delay's Lo-Fi) [V].
- **Tone after decimation**: an unfiltered crusher is all fizz; the loved samplers are loved for their
  filters (above) [V, forum].

### What we conclude

1. **Simulate the converter, render it band-limited.** Every nonlinearity (clip, quantiser, format, DAC
   curve) acts on the virtual samples, a discrete sequence: its "aliasing" is the real converter's, by
   definition. Two operations are continuous-time: reading the input at a fractional instant, and the
   hold staircase. Both go through the same minimum-phase band-limited kernel (impulse to sample, step to
   render). No oversampling, no ADAA, no reported latency.
2. **A fractional, glide-able clock** (phase accumulator in f64), with jitter as a real clock defect.
3. **Filters are the voicing**: the model chooses the anti-alias and reconstruction filters; bits and
   rate stay the player's.
4. **Formats beyond linear PCM**: µ-law (companded 8-bit), float (gain-ranging), ADPCM, delta — all
   sample-by-sample, no latency.
5. **Tails handled by the converter itself**: a level window tied to the LSB (1 – 2 LSB), crossfading to a
   clean (rate-reduced but unquantised) tail or to silence.
6. **Tuned crush**: a rate locked to a multiple of the played pitch puts the aliases on the note's
   harmonic series (within the detector's error: a slow beat, §2.1).

## 2. Design

```
in ─┬──────────────────────────────────────────────── dry (kernel-delay aligned, §2.4) ─────┐
    └▶ × input ▶ low_cut ▶ PRE (model) ──▶ VIRTUAL ADC @ R(t) ──────────────▶ VIRTUAL DAC ──┤
                                          sample at t_k (min-phase kernel,     word → level │
                                            fractional, jitter)                nonlin / asym│
                                          clip at 0 dBFS                       ladder error │
                                          dither ▶ quantiser (format, shape)                │
                                          tails (LSB-referenced)                            │
         R(t) = rate · 2^(rate_shift / 12) · mod (LFO / env), then track (N · f0)           │
                                                                ▼                           │
                     hold staircase rendered with minBLEP at the host rate ▶ POST (model)   │
                     ▶ high_cut ▶ × level × 1/input ──────────────────────▶ mix (crossfade)─┴▶ out
```

### 2.1 Virtual clock

- **Phase accumulator** in f64: φ += R / fs per host sample; a sampling instant t_k falls where φ crosses
  an integer, at a fractional position inside the host sample. R is continuous: no divider, no steps.
- **R = `rate` · 2^(`rate_shift` / 12) · mod**, clamped to 20 Hz … fs. `rate` (Hz, the catalogue id) is what
  presets cite; `rate_shift` (semitones, −72 … 0) is the **sweep control**: linear in its range as the
  contract demands, and a pitch-like feel under an expression pedal (0.57 st per 7-bit step), where a
  linear Hz range would spend half the travel on the top octave.
- R is smoothed in the log domain (20 ms, plus the contract's EXP smoother on `rate` and `rate_shift`), so
  a sweep is a glide in pitch. Changes never restart the clock (MDA did).
- **Off**: R ≥ min(fs, 48,000 Hz) → the virtual clock is the host clock: no kernel, no hold, no BLEP, the
  dry undelayed. Crossing the boundary crossfades over 10 ms (the two paths differ by the kernels' group
  delay). `rate` values below 48,000 Hz sound the same at every host rate; **Off is the one host-dependent
  setting**: its clip, quantiser and DAC curve run at fs (§2.4).
- **Jitter** (`jitter`): each instant moves by a seeded random offset, uniform ±25 % of a period at 100 %,
  low-passed so it wanders over a few periods (short-period fluctuations, as Decimort); consecutive
  offsets differ by less than half a period, so instants stay ordered. Aliases and images smear into
  noise; the dry pitch never moves (ADC and DAC share the one clock). The model filters follow the
  smoothed R, not the jittered instants.
- **Track** (`track`): gfx-dsp `detect` (YIN, `prepare_range` 40 Hz – 1.5 kHz if the CPU bound holds, else
  the 75 Hz default: a bass below it then does not lock) follows the input; **R = N · f0**, N = max(1,
  round(R_target / f0)), R_target being the R above (so `rate_shift` and `mod` step through the integer
  multiples: an arpeggio of harmonic crushes, never an inharmonic glide). Every harmonic k·f0 then folds
  onto |k − mN|·f0.
  - **Precision** [A]: a pitch error ε detunes the m-th fold by m·R·ε Hz. At R = 4 kHz and 2 cents (0.12 %)
    that is a 5 Hz beat: in tune. At 16 kHz it is 18 Hz: rough. Track is therefore specified for
    R ≤ 8 kHz; above, it still works but beats. f0 is the detector's refined period through a 30 ms
    smoother; string inharmonicity adds its own slow beating on high partials (the instrument's, kept).
  - **N has hysteresis**: it changes only when R_target / f0 leaves [N − 0.6, N + 0.6], so a bend or a
    vibrato across a rounding boundary does not make R jump back and forth.
  - On a change of note R glides over 20 ms. **Unvoiced frames hold the last R** (pick attacks, chords,
    noise); only after 250 ms unvoiced does R glide back to R_target (200 ms). Returning at once would
    chirp on every attack.
- **Modulation** (`mod`): LFO (sine, phase 0 on reset, `sync` from the transport) or the envelope
  (`env_sens` / `env_att` / `env_rel`); `mod_depth` ±100 % = ±3 octaves of R, bipolar so "pick harder →
  crunchier" and its reverse both exist.

### 2.2 Virtual ADC

- **Sampling**: the value at t_k is read through the **minimum-phase band-limited kernel** of §2.4 (the
  impulse table: 32 host samples ending at t_k, phase chosen by the fractional position). Causal, no
  look-ahead, and its images are ≥ 80 dB down, so nothing folds that an ideal converter would not fold.
  Linear interpolation was rejected: its images (−67 dB for a 1 kHz partial, −38 dB at 5 kHz, at 48 kHz
  [A]) are sampled at R and land as foreign aliases, and its treble droop changes with the fractional
  position (amplitude modulation at the beat rate). Cubic would cost a reported sample for −75 dB at
  5 kHz. Cost: 32 MACs per virtual sample.
- **Average** model only: the input is integrated over [t_{k−1}, t_k] instead of point-sampled (whole
  host samples plus trapezoid edge terms): MDA's right half, at any rate. The box's own sinc adds to the
  rejection, so the edges need no kernel (foreign aliases ≤ −60 dB, bench).
- **Clip** at the converter's full scale (0 dBFS after `input`): a hard clip on the samples, as a real
  ADC saturates. Discrete, so no antialiasing is due.
- **Dither** (`dither`): TPDF, 100 % = ±1 LSB (2 LSB peak-to-peak), added in the coded domain (after
  companding). Seeded. Linear, µ-law and Float; ignored by ADPCM and Delta.
- **Quantiser shape** (`quantizer`; Linear, µ-law, Float): **Tread** (round: zero is a level, silence
  stays silent), **Riser** (zero between two levels: quiet input → ±½ LSB buzz, 1 bit = a square, the
  fuzz), **Truncate** (toward zero: a 2-LSB dead zone, MDA's `int()`).
- **Format** (`law`); `quant` is the word size in bits, continuous where the format allows it:

  | `law` | Coding | `quant` means |
  |---|---|---|
  | Linear | uniform PCM, step 2^(1 − bits) | bits, fractional allowed (2^bits levels): sweeps without steps |
  | µ-law | compress (continuous µ = 255 curve), quantise, expand | code-word bits; 8 = the drum machines' and the telephone's (within one step of G.711's segmented tables) |
  | Float | 3-bit exponent (8 octaves, 48 dB of gain ranging) + mantissa | mantissa bits: grain proportional to level, never fizz on tails |
  | ADPCM | IMA / DVI adaptive step (89-step table; the 3-, 4- and 5-bit index tables, a 2-bit one [I]), at the virtual rate | code bits, rounded to 2 – 5 (stepped by nature; 5 ms crossfade on change) |
  | Delta | CVSD: 1 bit per virtual sample, syllabic step adaptation [I: time constant by ear] | finest step = 2^(−bits): slope overload on attacks, granular hiss on tails |

  ADPCM and Delta are sample-level converter formats of the period's hardware (voice chips, consoles,
  delta-modulation delays), zero-latency and frame-free: they give time-domain artefacts (slope overload,
  adaptive grain), nothing like gfx-glitch's spectral frames. That is why they belong here. They are
  coded **after** Linear / µ-law / Float pass the ideal-model bench, and enter the enum only when coded
  (enums grow at the end, so this costs nothing). **A-law was dropped**: at equal bits it is µ-law with a
  slightly coarser bottom step, not a different sound; it can be appended if a use appears.

### 2.3 Virtual DAC

- **Transfer curve** (`nonlin`, `asym`): y = sign(v) · r · (|v| / r)^e, e = 10^(−0.15 · nonlin / 100)
  (MDA's exponent law, 0.708 at 100 %), with r = −18 dBFS so a typical guitar level keeps its loudness
  (MDA's curve is referenced to full scale: +5.3 dB at −18 dBFS, more below). `asym` bends only the
  positive half (even harmonics, MDA's left side). It is a memoryless map of the held levels **before**
  the BLEP rendering: the staircase keeps its instants, so the rendering stays exact, and the harmonics it
  adds fold at R / 2 as a real DAC's code-dependent error does.
- **Ladder error** (`ladder`): an R-2R DAC with seeded bit-weight errors (MSB error up to ±1 LSB at
  100 %, smaller bits proportionally). The mid-scale code transition gets a wrong-sized step: the
  crossover-like grit of old ladder DACs on quiet signals. Linear and Float only [I: the magnitude is a
  voicing choice, to tune by ear].
- **Tails** (`tails`), the fizz-on-silence fix. What is hated is precise: below ~1.5 LSB a Tread
  quantiser only outputs {−1, 0, +1} LSB — a pulse train at a **constant** level while the note decays,
  then a cut at ½ LSB; a Riser buzzes at ±½ LSB for ever.
  - **Detector**: gfx-dsp `SlidingMax` over 25 ms (one period of 40 Hz: ripple-free on low notes, the
    gfx-gate lesson) on the converter's input, then a one-pole: 1 ms toward "crushed", 20 ms toward
    "tail". A new note is crushed from its first virtual sample.
  - **Law**: a level window, not a switch — no hysteresis, no timed crossfade. With s the format's step at
    zero (Linear: 2^(1 − bits); µ-law: the companded step at zero; ADPCM and Delta: their minimum step,
    which otherwise idles as a tone at R / 2), g = 0 above 2 s, 1 below 1 s, a smoothstep in dB between.

    | `quant` (Linear) | Fully crushed above | Fully tail below |
    |---|---|---|
    | 4 bits | −12 dBFS | −18 dBFS |
    | 8 bits | −36 dBFS | −42 dBFS |
    | 12 bits | −60 dBFS | −66 dBFS |

  - **Clean** (default): each virtual sample is q + g · (x − q), x the unquantised sample, **before** the
    hold: one staircase, one BLEP path, no second render. The two differ by at most one LSB, so the fade is
    a fade of the quantisation error: click-free by construction. Tails keep the sampler's rate and
    filters and decay to true silence.
  - **Gate**: the virtual sample is q · (1 − g): the hard lo-fi cut, de-clicked (dither fades with it).
  - **Raw**: nothing (riser buzz, sputter, truncate gating: the converter as it is).
  - Float needs none (its grain scales with level).
  - **At 6 bits and below the window reaches playing levels** (table): Clean then lets soft playing through
    unquantised instead of swallowing it. That is a sound, not always the wanted one: presets at ≤ 6 bits
    state `tails` explicitly, and `input` is the way to push a quiet source up into the bits.

### 2.4 Rendering and filters

- **The kernel** (new gfx-dsp `blep`): one Kaiser-windowed low-pass, pass band to 0.35 fs (±0.5 dB), stop
  band ≥ 86 dB from 0.6 fs, converted to minimum phase (cepstral method) at `prepare`, **32 taps × 64
  phases**, linear interpolation between phases (interpolation images ≈ −90 dB [A]); each phase row
  normalised to unity DC gain. Two tables from it: the **impulse** (sampling, §2.2) and its running sum,
  the **step** (rendering). The transition band 0.5 – 0.6 fs folds onto 0.4 – 0.5 fs only (19 – 24 kHz at
  48 kHz): the usual BLEP trade, inaudible. The first draft's 32 × 16 could not hold 80 dB: 16 taps at
  that rejection need a 0.31 fs transition band (Kaiser estimate [A]), i.e. a droop from 9 kHz.
- **minBLEP rendering**: each held value change Δ_k at the fractional instant t_k adds Δ_k ·
  (minimum-phase band-limited step − ideal step) to a residual ring buffer. Causal (the correction
  starts at the step): no look-ahead. Only the images and aliases of the virtual converter remain, the
  ones the virtual system really makes.
- **Why no oversampling and no ADAA**: clip, quantiser, format and DAC curve all act on the discrete
  virtual sequence, whose spectrum is periodic in R by nature: there is nothing to antialias. The two
  continuous-time operations (sampling, hold) go through the kernel. With `rate` Off the converter **is**
  the host's: an `input` overdrive then clips at fs and folds at fs / 2 — digital clipping, the sound
  asked for, but it differs between 44.1, 48 and 96 kHz hosts. Stated, not hidden; golem runs at 48 kHz.
- **Models** (`model`: the virtual sampler's filters; bits, rate and format stay the player's):

  | `model` | PRE (anti-alias, host rate) | POST (reconstruction, host rate) | Sound |
  |---|---|---|---|
  | **Raw** (default) | none | none: the bare staircase (images up to fs / 2) | The classic crusher, MDA's left half |
  | **Clean** | 8th-order elliptic low-pass at 0.45 R (0.1 dB ripple, ≥ 70 dB stop band from 0.55 R) | the same | Band-limited decimation: bandwidth and grain, no foldover |
  | **Average** | integrate-and-dump (box over one period) | none | MDA's right half: a softer, sinc-shaped fold |
  | **Sampler** | 6th-order Butterworth at 0.45 R | 6th-order Butterworth at 0.42 R | The warm 12-bit rack sampler |
  | **Drum Machine** | none (hot-sampled: aliasing kept) | 4-pole ladder at 0.5 R, light resonance | Ringy aliasing, smoothed images |
  | **Console** | none | 3-tap Gaussian FIR **on the virtual samples** (≈ [0.18, 0.64, 0.18]: −11 dB at R / 2 [I]), then the hold | The muffled console voice |

  - **Console** was "4-point Gaussian interpolation instead of a hold": evaluated at host instants that
    interpolator is not band-limited (its images near R fold at fs / 2 when R > fs / 2: aliases the console
    never made) and it needs future samples. At native pitch the console's interpolator **is** this fixed
    FIR at the sample rate; as a discrete filter before the hold it is exact, alias-free and costs one
    virtual period of delay.
  - **Drum Machine** is a voicing, not the unit's schematic [I]: the original has an input filter, and
    its famous ring comes from unfiltered outputs and pitched playback (which an insert cannot do: no
    "record at 45, play at 33" here).
  - Cutoffs track the smoothed R(t), clamped to 0.45 fs, through TPT SVF sections; while R moves the
    coefficients are recomputed every 16 samples and **ramped** between (`set_biquad_ramped`), never
    stepped (elliptic zeros through the SVF's mixing outputs). Sampler, Drum Machine and Console voicings
    are [I], to tune by ear.
- **Tone**: `low_cut` (2-pole, before the ADC: telephone and AM band) and `high_cut` (4-pole Butterworth
  after POST, on top of the model's own), both off by default. A resonant multimode filter (Decimort,
  Krush, TAL) is **not** added: in golem that is the next block in the rack.
- **Gain**: `input` drives the converter (clip, use of the bits); the output is compensated by
  −`input`, so `input` changes how hard the converter is hit, not the volume. `level` is the final trim.
- **Mix**: a linear crossfade dry ↔ wet (100 % = wet only; equal-gain, since both are correlated). The
  dry is **delayed by the two kernels' low-frequency group delay** (a fixed whole number of samples,
  ≈ 4 – 8, measured at build [A]; the gfx-drive precedent: the clean path matches the processed one).
  Without it the blend's first notch falls inside the wet's band as soon as R > ≈ 4 kHz [A]; with it only
  the hold's own T / 2 remains, whose notch sits at R, outside the base band. At Off there is no kernel
  and the dry is undelayed.
- **Stereo**: mono 1 / 1 first (not stereo by nature); 2 / 2 offered with one shared clock (jitter,
  track and modulation linked) so the image stays coherent. No polarity inversion, no mono sum (both
  were MDA's).
- **Latency: reported 0.** No look-ahead anywhere. The wet carries the kernels' minimum-phase group
  delay (a few samples, as gfx-drive and gfx-emet) plus the hold's T / 2 and the model filters; the
  detector steers R and never delays the audio. At Off the measured delay is exactly 0.
- **CPU** [A, estimate]: per host sample the clock, ≤ 32 MACs of BLEP residual, ≤ 8 SVF sections, a
  sliding maximum; per virtual sample (≤ fs) 32 MACs of sampling kernel and the format's work.
  ≈ 100 – 180 ns per sample mono at 48 kHz (≤ 1 % of a core), worst ≈ 300 ns with Clean + track +
  modulation (cf. gfx-emet 198 ns).

### 2.5 gfx-dsp reuse

| Module | Use |
|---|---|
| `svf` | PRE / POST sections (Butterworth, elliptic via mixing outputs, ladder approximation), `low_cut`, `high_cut`; coefficient ramps while R moves |
| `smooth`, `exp_smooth` | Parameter smoothing; the EXP rule on `quant`, `rate`, `rate_shift`, `nonlin`, `mix`, `mod_depth` |
| `envelope` | `SlidingMax` for the tails detector; `PeakFollower` for the envelope modulation |
| `lfo`, `transport` | Rate LFO, `sync` |
| `detect` | `track` (YIN pitch + clarity; `prepare_range`) |
| `oversample`, `adaa` | **Not used**, by design (§2.4) |
| **New: `blep`** | The minimum-phase kernel (impulse + step tables, built at `prepare`), a fractional-instant reader and a residual buffer for any stepped signal. Candidate users: gfx-delay's Lo-Fi model. (gfx-dsp `bbd` already integrates its staircase exactly through its analog filters: no need) |
| **New: `converter`** (in the crate first) | Clock, formats, quantiser shapes, DAC curve, ladder. Moves to gfx-dsp once gfx-delay's Lo-Fi adopts it (today an integer divider at the host rate, mid-tread rounding: it zippers and changes with fs) |

## 3. Parameters

### Ids and units

**Decided: real units** (bits, Hz, %) under the catalogue's three ids, the **names kept exactly** ("Quant",
"Rate", "Non-Lin": golem joins descriptors to catalogue knobs by name). The contract asks for the
catalogue's unit; this is a stated exception, on gfx-freqshift's precedent (Surge's 0 – 1 → Hz, %, ms),
not gfx-ringmod's (0 – 1 kept): ringmod's laws were monotonic and worth keeping, MDA's are not a scale —
`quant` is a staircase (4 + ⌊12q⌋), `rate` folds two models around 0.5 with a divider that depends on the
host rate, `nonlin` is neutral at 0.5 while the catalogue believes 0. Keeping 0 – 1 would freeze those
accidents into permanent ids. Real units make a value mean the same at 44.1, 48 and 96 kHz and let
presets cite references (26.04 kHz, 8 bits µ-law). **Ranges are linear** (contract).
It needs golem-rack's agreement: the catalogue entry switches its min / max / default, and saved values
are converted once (below). If golem-rack cannot migrate saved chains, the fallback is gfx-ringmod's: the
three ids at 0 – 1 with monotonic laws of our own (`quant` = 1 + 15q bits, `rate` = 48,000 · 2^(−10·(1 − r)) Hz,
`nonlin` = 100·p %) — worse for presets, never preferable otherwise.

**Conversion from MDA** (fs = 48 kHz, golem's rate; for other rates replace 48,000 by fs):

| Catalogue value | gfx-crusher |
|---|---|
| `quant` q | `quant` = 4 + ⌊12q⌋ bits (q = 1 → 16), `law` Linear, `quantizer` Truncate |
| `rate` r | r = 0.5 → `rate` 48,000 (Off); otherwise `rate` = 48,000 / ⌊e^(18·|r − 0.5|)⌋ Hz (min clamped to 50); `model` Raw if r < 0.5, Average if r > 0.5 |
| `nonlin` p (faithful) | `nonlin` = 200·|p − 0.5| %; `asym` 100 % if p < 0.5, else 0 %. Same curve shape; the level differs (ours is neutral at −18 dBFS, MDA's adds 5.3 dB there at 100 %) |
| `nonlin` p (by intent) | `nonlin` = 100·p %, `asym` 0 % |
| implicit MDA defaults | Post filter ≈ `high_cut` 9,300 Hz (MDA's 8 poles are gentler than our 4-pole: the match is approximate); `tails` Raw; the −6 dBFS clip and the polarity inversion are dropped (new sound, accepted) |

| Existing preset (q, r, p) | Faithful (what golem plays today) | Recommended |
|---|---|---|
| Standard (0.5, 0.5, 0) | 10 bits Truncate, Off, `nonlin` 100 % `asym` 100 %, high_cut 9.3 kHz | Re-author as the new default: 8 bits, 16 kHz, Raw (10 bits with the rate Off is nearly inaudible) |
| 8-bit (0.65, 0.45, 0.1) | 11 bits, 24,000 Hz Raw, 80 % even | Re-author: 8 bits Linear, 16 kHz, Raw (= Standard; or 8 bits, 22,050 Hz) |
| AM radio (0.35, 0.25, 0.3) | 8 bits, 533 Hz Raw, 40 % even | Re-author: 8 bits µ-law, 11,025 Hz Clean, `low_cut` 300, `high_cut` 4,500, `nonlin` 30 % |
| Broken speaker (0.4, 0.3, 0.6) | 8 bits, 1,333 Hz Raw, 20 % odd | Re-author: 6 bits, 6 kHz Raw, `tails` Raw, `nonlin` 60 %, `asym` 40 %, `ladder` 50 % |

**Rule (decided)**: **block presets and the catalogue default are re-authored by intent**
(`presets/gfx-crusher.json`) — the contract already accepts that presets sound different, and today's
sounds are accidents (a "clean" default playing MDA's strongest even distortion). **Saved rig chains
convert faithfully**, because a player tuned them by ear on what MDA really did — if any exist:
golem-rack counts them first; with none, the faithful column is dropped. Faithful needs golem's engine
table to set non-catalogue ids (`quantizer`, `model`, `asym`, `high_cut`, `tails`) on conversion.

### Front panel

| id | Name | Type | Range | Catalogue | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `quant` | Quant | f | 1 – 16 bits | 0 – 1 (MDA) | 8 bits | 20 ms | ✓ | Word size of the virtual converter; fractional for Linear (a sweep has no steps) |
| `rate` | Rate | f | 50 – 48,000 Hz | 0 – 1 (MDA) | 16,000 Hz | 20 ms, log | ✓ | The virtual clock; 48,000 (or R ≥ fs) = Off. Fractional, glides |
| `nonlin` | Non-Lin | f | 0 – 100 % | 0 – 1 (MDA) | 0 % | 30 ms | ✓ | The DAC's power-law curve (thickening), level-neutral at −18 dBFS |
| `mix` | Mix | f | 0 – 100 % | new | 100 % | 20 ms | ✓ | Dry ↔ wet linear crossfade (parallel crush) |
| `model` | Model | e | Raw · Clean · Average · Sampler · Drum Machine · Console | new | Raw | 20 ms crossfade | | The virtual sampler's filters (§2.4) |

**Defaults (decided): sound-forward** — 8 bits, 16 kHz, Raw, `tails` Clean. An inserted crusher must be
heard at once; 10 bits with the rate Off (the converted catalogue default) sounds like a broken block.
None depends on another parameter. golem may still pin other values at insert.

### Expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `law` | Format | e | Linear · µ-law · Float · ADPCM · Delta | Linear | crossfade | | Word format (§2.2); the last two enter the enum when coded |
| `quantizer` | Quantizer | e | Tread · Riser · Truncate | Tread | — | | Where zero sits: silence stays silent / buzzes / dead zone (MDA) |
| `dither` | Dither | f | 0 – 100 % | 0 % | 30 ms | | TPDF, 100 % = ±1 LSB; tails fade it too |
| `tails` | Tails | e | Clean · Gate · Raw | Clean | — | | Below 1 – 2 LSB: unquantised tail / silence / as is (§2.3) |
| `input` | Input | f | −24 – +24 dB | 0 dB | 20 ms | | Drive into the converter (clip, use of bits); output compensated |
| `level` | Level | f | −24 – +12 dB | 0 dB | 20 ms | ✓ | Output trim |
| `low_cut` | Low Cut | f | 20 – 1,000 Hz | 20 Hz (off) | 30 ms | | 2-pole, before the converter |
| `high_cut` | High Cut | f | 1,000 – 20,000 Hz | 20,000 Hz (off) | 30 ms | ✓ | 4-pole, after the model's reconstruction |
| `asym` | Asymmetry | f | 0 – 100 % | 0 % | 30 ms | | Share of `nonlin` applied to one polarity only (even harmonics) |
| `ladder` | DAC Error | f | 0 – 100 % | 0 % | 30 ms | | R-2R bit-weight mismatch: grit on quiet signals (Linear, Float) |
| `rate_shift` | Rate Shift | f | −72 – 0 st | 0 st | 20 ms | ✓ | Transposes the clock down (R = `rate` · 2^(st / 12)): **the EXP sweep control**, pitch-like feel |
| `jitter` | Jitter | f | 0 – 100 % | 0 % | 30 ms | ✓ | Clock jitter, ±25 % of a period at 100 % (seeded) |
| `track` | Track | b | off / on | off | — | | Rate locked to N × the played pitch: aliases on the harmonic series (best at R ≤ 8 kHz) |
| `mod` | Rate Mod | e | Off · LFO · Envelope | Off | — | | Source modulating the clock |
| `mod_depth` | Mod Depth | f | −100 – +100 % | 0 % | 30 ms | ✓ | ±3 octaves of rate at ±100 % |
| `mod_rate` | Mod Rate | f | 0.05 – 10 Hz | 0.5 Hz | 30 ms | ✓ | LFO rate when `sync` = Off |
| `sync` | Sync | e | shared list (Off · 4/1 … 1/32) | Off | — | | Shared id |
| `env_sens`, `env_att`, `env_rel` | Env Sens / Attack / Release | f | 0 – 100 %, 1 – 100 ms, 20 – 2,000 ms | 50 %, 5 ms, 200 ms | — | | Shared ids |
| `rate_out` | Rate (now) | f, read-only | 0 – 48,000 Hz | — | — | | Effective R after shift, modulation and track, for golem's display |

`quant` and `rate` keep their EXP rule; on ADPCM `quant` is stepped by nature (crossfaded). `quantizer`,
`dither` and `ladder` are inert on the formats that do not use them (§2.2, §2.3): no hidden re-mapping.

Expert groups (for `visuals/data/groups.json`): **Converter** (`law`, `quantizer`, `dither`, `tails`,
`ladder`), **Gain** (`input`, `level`), **Tone** (`low_cut`, `high_cut`, `asym`), **Clock** (`rate_shift`,
`jitter`, `track`, `mod`, `mod_depth`, `mod_rate`, `sync`, `env_*`).

### Visual

- **Category: `lofi`**, not `utility`: a bitcrusher is a dirt effect, and golem already has a `lofi`
  category (CATEGORY_HUE `#e07a3a`, holding `grit`). The move also puts the block where golem's
  `role_categories` want it: `lofi` is offered to voice and drums lanes (the "AM radio" preset is a voice
  preset). The catalogue change is golem-rack's.
- **Hue**: `METAL_HUE.lofi = '#e8336f'` (raspberry). golem's lofi orange sits too close to drive's
  `#e04a00`; raspberry is free between pitch's magenta `#c4148c` and the thrash drive red `#c8160e`
  (about 20° of hue from each: enough with a different motif, to confirm on the family sheet at tile size).
- **Finish: Thrash** (`FAMILY_FINISH.lofi = 'thrash'`): it is dirt, like drive; gloss raspberry with the
  motif in yellow lined in black reads as an arcade cabinet.
- **Motif: `pixels`** (new; `stairs` is pitch's, `bins` is gfx-glitch's). Either side of the rune, a sine
  sampled into square pixels: one column every 6 units; in each, a 4 × 4 square at the sine's value
  (1.5 periods across the face) rounded to 5 levels; a thin stepped line at 35 % opacity joins the squares
  as the hold staircase; the two columns nearest the rune are 3 × 3 and fainter, so the wave dissolves
  into the rune. Thrash: yellow squares lined in black; anodized (if forced): engraved squares.
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-crusher": {"name": "Crush", "blocks": ["bitcrusher"], "knobs": ["quant", "rate", "nonlin", "mix"],
                  "selectors": ["model"], "toggles": ["track"], "category": "lofi", "color": "#e8336f"}
  ```
- **Glyph (decided): keep golem's existing `bitcrusher` glyph** (a stepped line over a faint sine): it
  already says "sampled and held", and the glyph is the link between tile and pedal.
- No maker's livery, logo or trademark on the pedal.

## 4. Applications

Reference settings become block presets (`presets/gfx-crusher.json`); no artist or trademark names.
Presets are level-matched to the bypass (±2 LU on the bench DIs) with `level`; those at ≤ 6 bits state
`tails`.

| Use / preset idea | Settings to start from | Feature that matters |
|---|---|---|
| **Rack Sampler 12** (warm boom-bap grit on clean guitar or keys) | `model` Sampler, `quant` 12, `rate` 26,040 | Filters as the warmth; fractional rate |
| **Hot-Sampled 26k** (ringy, aliased) | `model` Drum Machine, `quant` 12, `rate` 26,040 | Aliasing kept on purpose |
| **Companded 8** (drum-machine 8-bit that does not fizz) | `law` µ-law, `quant` 8, `rate` 28,000, `model` Raw | µ-law: grain on loud, clean on quiet |
| **Phone Line** | `law` µ-law, `quant` 8, `rate` 8,000, `model` Clean, `low_cut` 300, `high_cut` 3,400 | G.711 band |
| **AM Radio** | `law` µ-law, `quant` 8, `rate` 11,025, `model` Clean, `low_cut` 300, `high_cut` 4,500, `nonlin` 30 % | |
| **Console Voice** | `law` ADPCM, `quant` 4, `rate` 32,000, `model` Console | Gaussian muffle, ADPCM grain |
| **Delta Robot** | `law` Delta, `quant` 8, `rate` 16,000 | Slope overload on picking |
| **1-Bit Fuzz** | `quant` 1, `quantizer` Riser, `tails` Gate, `rate` 48,000, `high_cut` 6,000, `level` −9 dB | Riser square (−6 dBFS whatever the input), gated cleanly |
| **8-Bit Lead** (the default) | `quant` 8, `rate` 16,000, `model` Raw, `tails` Clean | Classic crush with clean tails |
| **Tuned Crush** (leads, arpeggios) | `track` on, `rate` 4,000, `quant` 6, `tails` Raw, `model` Raw | Aliases on the harmonic series: in tune |
| **Tuned Arpeggio** | `track` on, `rate` 2,000, `mod` LFO, `sync` 1/8, `mod_depth` 40 % | The LFO steps through integer multiples of f0 |
| **Sample-Rate Sweep** (EXP) | `rate` 48,000, EXP on `rate_shift` 0 → −72 st (48 kHz → 750 Hz), `model` Raw | Fractional clock, pitch-like travel: a glide, no zipper |
| **Pick-Harder Crush** | `mod` Envelope, `mod_depth` −60 %, `rate` 16,000 | Rate drops with the attack |
| **Synced Wobble** | `mod` LFO, `sync` 1/8, `mod_depth` 50 %, `rate` 8,000 | Host-synced, deterministic |
| **Dusty Jitter** | `jitter` 60 %, `quant` 10, `rate` 12,000, `model` Sampler | Clock jitter smears the aliases |
| **Parallel Grit** (bass, keep the low end) | `mix` 40 %, `quant` 6, `tails` Raw, `rate` 6,000 | Crossfade blend, dry aligned |
| **Old Ladder DAC** | `quant` 12, `ladder` 70 %, `model` Sampler | Low-level crossover grit |
| **Hit the Converter** | `input` +12 dB, `quant` 10, `rate` 48,000 | Clip at full scale, level compensated |
| **Clean Decimate** (bandwidth only) | `model` Clean, `quant` 16, `rate` 8,000 | No foldover: a brick-wall "small speaker" |

## 5. Bench suite (on top of gfx-bench's common checks)

**gfx-bench options**: `click_bound_db` −30 dB — a `quant` step 16 → 1 bit or a `rate` jump creates real
high-frequency content that is the effect, not a click (gfx-drive's precedent, −25 dB); to tighten if
the measurement allows. `skip_latency` — the common check wants the correlation peak at the reported lag,
and the wet carries minimum-phase group delay and the hold's T / 2 (gfx-drive's and gfx-emet's precedent);
the Off row below measures the true 0. Silence floor −140 dBFS holds because `tails` Clean and Tread make
silence exactly 0. **Aliasing**: the usual "aliases below X dB" check cannot apply — aliasing is the
product. It is replaced by a check against an **ideal model**: the intended aliasing is computed, and
only the excess is measured.

| Check | Pass |
|---|---|
| Ideal-model residue | Sine sweeps (20 Hz – 16 kHz) and guitar DI at `rate` 3k / 8k / 26.04k / 37.3k, `quant` 16, Raw: reference = the ideal converter (input evaluated at t_k by sinc interpolation, held, band-limited at fs / 2 by the exact Si-function step sum). Magnitude spectra agree within 0.5 dB below 0.35 fs; energy at frequencies absent from the reference ≤ −80 dB re peak below 0.4 fs, ≤ −40 dB above. Average: ≤ −60 dB |
| Kernel | Pass band ±0.5 dB to 0.35 fs, stop band ≥ 86 dB from 0.6 fs, each phase's DC gain 1 ± 1e−6; low-frequency group delay reported by the build = the dry's alignment delay |
| Off is transparent | `rate` Off, `quant` 16, Tread, Raw, `mix` 100: output = input within the 16-bit error (≤ −96 dBFS), measured latency 0; `mix` 0: bit-exact dry |
| Fractional rate, no zipper | 1 kHz sine, `rate` swept 48,000 → 500 Hz in 2 s, then `rate_shift` 0 → −72 st by EXP 50 Hz steps: no broadband burst above −60 dB re peak; the main alias's frequency trajectory continuous (no jump > one bin between 10 ms frames) |
| Rate independence | Same `rate` (8,000 Hz) at 44.1 / 48 / 96 kHz: alias frequencies identical ± 1 Hz, levels within 1 dB below 15 kHz |
| Quantiser shapes | Tread: silence → 0; Riser: silence → ±½ LSB with `tails` Raw, 0 with Clean; Truncate: dead zone 2 LSB. Level count 2^bits ± 1 at integer bits |
| Formats | µ-law: follows the continuous µ = 255 curve, within one step of G.711's tables at 8 bits; Float: SNR constant within 1 dB from −6 to −42 dBFS; ADPCM / Delta: deterministic and stable (no overflow on a full-scale square), idle output 0 with `tails` Clean |
| MDA conversion | The test embeds MDA Degrade's quantiser and hold (no post filter, no clip, `nonlin` at neutral). For the four golem presets converted faithfully (Truncate, Raw / Average, `tails` Raw, `nonlin` 0): same level set and dead zone, same hold period, alias frequencies identical, alias levels within 1.5 dB below 10 kHz. No waveform null: MDA's staircase is naive, ours band-limited |
| Tails | 440 Hz and 82 Hz notes decaying 60 dB in 2 s, `quant` 4 / 8 / 12: Clean → g follows the table of §2.3 within 1 dB; below 1 LSB no component above −90 dBFS that is not in the unquantised path; no HF burst > −60 dB re peak through the window; no modulation of g at the note's period (82 Hz). Gate → 0 below 1 LSB, de-clicked. A note attacked from silence is fully crushed within 2 ms |
| Level | `quant` 4 → 16, Tread, rate Off, DI at −12 dBFS RMS, `tails` Raw: loudness within ±3 dB; `nonlin` 0 → 100 %: within ±1.5 dB at −18 dBFS; every factory preset within ±2 LU of the bypass |
| Model filters | Clean: passband ±0.1 dB to 0.45 R, stop band ≥ 70 dB from 0.55 R, tracking R within 2 % during a sweep, no zipper sidebands > −80 dB under a 10 Hz ±3-octave LFO; Sampler / Drum Machine: responses match their stated orders and cutoffs within 0.5 dB; Console: the 3-tap response, no foreign alias at `rate` 32,000 |
| Track | Synthetic sawtooth 110 / 220 / 440 Hz, `rate` 4,000: aliases within ±5 cents of harmonics of f0, beat ≤ 6 Hz; a ±50-cent vibrato across an N boundary: N constant; DI melody: no R excursion > 5 % during pick attacks; chords hold R, no click. By ear above 8 kHz |
| Mix | `mix` 50 %, `quant` 16, Raw, `rate` 6k / 16k: no notch deeper than 3 dB below 0.4 R |
| Jitter, LFO | Strict null between two renders (seeded); `jitter` 0 → exact clock |
| CPU | ≤ 180 ns / sample mono at defaults, ≤ 350 ns worst case (Clean + track + LFO + ADPCM), 48 kHz |
| Listening | clean.wav, lead.wav, funk.wav, keys and drum renders against MDA Degrade (golem today) and the presets' intents; tails and silence with a gain stage after it; blind, level-matched against a reference crusher on the same DIs (owner's choice, §7.9) |

## 6. Positioning

| Pedal | Its lo-fi | Difference |
|---|---|---|
| gfx-delay Lo-Fi | Rate / bits **inside the loop**, cumulative per repeat | gfx-crusher is the converter in series; gfx-delay may adopt its `converter` later |
| gfx-glitch Crush / Corrupt / Bin Clip | **Spectral**: bins' levels and phases quantised, frames, latency | gfx-crusher is time-domain, sample-level, no reported latency |
| gfx-drive | Analog clipping | `nonlin` is a DAC curve on held samples, not a drive |

Against the field: Decimort 2's set is covered (both filters, jitter, quantiser shapes, dither, pre-amp)
except its resonant multimode filter (the rack's job); beyond it: formats, tails, track, the LSB ladder,
a rate that glides and means the same at every host rate. Not attempted: codec loss (Lossy → gfx-glitch),
the sampler trick of pitched record / playback (needs a buffer: latency), bitwise mangling (AND / XOR
masks: a possible later `law`, appended).

## 7. Open questions — settled at review

1. **Units** → **real units**, as an agreed exception to "keep the catalogue's unit" (§3, gfx-freqshift's
   precedent). Fallback (0 – 1 with monotonic laws of our own) only if golem-rack cannot migrate.
2. **`nonlin` conversion** → by intent for block presets and the default; faithful for saved rig chains,
   and only if golem-rack finds any.
3. **Linear `rate` range** → kept (contract), and **not** left to a display taper: `rate_shift` in
   semitones is the EXP sweep control (§2.1).
4. **Defaults** → sound-forward: 8 bits, 16 kHz, Raw. The old "Standard" intent survives as nothing: it
   was inaudible.
5. **Formats** → ADPCM and Delta are in scope; coded after the linear core passes the bench, and listed
   in the enum only then. A-law dropped.
6. **minBLEP at 1×** → 32 × 16 was too short for −80 dB. **64 phases × 32 taps**, stop band from 0.6 fs
   (§2.4). Still 1×: no oversampling.
7. **Sampling interpolation** → neither linear (foreign aliases at −38 dB) nor cubic (a reported sample):
   the same minimum-phase kernel, transposed. No reported latency.
8. **Tails default** → **Clean**, with the window lowered from 3 LSB to 1 – 2 LSB and made a level law
   (§2.3): at the default 8 bits it acts below −36 dBFS, exactly where a gain stage after the block would
   turn the sputter into fizz. Presets at ≤ 6 bits choose for themselves.
9. **Model voicings** → cannot be settled on paper. Method: blind, level-matched comparison on the bench
   DIs against one reference per model, chosen and supplied by the owner (a hardware recording where one
   exists, else the best emulation he owns). Until then the three voicings stay [I].
10. **Visual** → `lofi`, raspberry, Thrash, `pixels`: accepted; hue separation to confirm at tile size;
    keep the existing glyph.
11. **Stereo** → linked clock only. Decorrelated jitter would widen by smearing the image at random; the
    rack has better wideners. Can be added later as a new parameter without breaking anything.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Strong.** The central idea — simulate the converter exactly, render it band-limited — is
right, is what separates this from every phase-accumulator crusher surveyed, and holds under scrutiny:
every nonlinearity really is a function of the discrete virtual sequence, and a memoryless DAC curve on
held levels leaves a staircase the BLEP renders exactly. The claims about MDA Degrade were checked line
by line against `mdaDegrade.cpp` and `Degrade.ttl`: all hold (bits = 4 + ⌊12q⌋ for a source on both
inputs, two-sided rate with an integer divider, `nonlin` 0 = strongest one-sided curve, −6 dBFS clip at
Headroom 0.8, inverted polarity, mono sum, 8-pole post filter at −3 dB ≈ 9.29 kHz, the four presets'
real values). The design had one wrong inference, two under-sized specifications and one mechanism tuned
too wide; none touches the architecture.

Changes made, and why:

| # | Change | Why |
|---|---|---|
| 1 | **Sampling: linear interpolation → the minimum-phase kernel** (§2.2) | Linear interpolation leaves the input's images at −67 dB (1 kHz) to −38 dB (5 kHz); sampled at R they become aliases no converter makes, and the bench's own −80 dB bound could not pass. The DAFx25 "interpolation matters little" result is a BBD result (filtered input, high clock) and does not transfer. The transposed BLEP kernel costs 32 MACs per virtual sample and no latency |
| 2 | **Kernel 32 × 16 → 64 phases × 32 taps**, stop band from 0.6 fs, design stated (§2.4) | 16 taps cannot give 80 dB without drooping from 9 kHz; 32 phases with linear interpolation stop near −78 dB |
| 3 | **"Zero latency" → "reported 0, a few samples of minimum-phase group delay"**; bench gets `skip_latency` and an Off row measuring 0 | minBLEP is causal, not delay-free; gfx-bench's latency check would have failed at any setting below Off (gfx-drive's precedent) |
| 4 | **Dry aligned to the kernels' group delay** (§2.4, Mix) | Unaligned, the parallel blend notches inside the wet's band above R ≈ 4 kHz. Same fix as gfx-drive's blend in the first review |
| 5 | **Tails**: 3 LSB switch with hysteresis and a timed fade → **1 – 2 LSB level window**, sliding-maximum detector, fade done on the virtual samples; Delta and ADPCM idle covered; threshold table; rule for ≤ 6-bit presets | At 3 LSB a 4-bit crusher was "tail" below −8.5 dBFS — it would hardly crush a guitar. A 50 ms peak follower ripples on low notes (the gfx-gate error). A level law cannot chatter; fading before the hold needs no second render and cannot click |
| 6 | **Console model**: Gaussian interpolation instead of a hold → 3-tap Gaussian FIR on the virtual samples, then the hold | The interpolator evaluated at host instants is not band-limited and needs future samples: it broke both "nothing else aliases" and causality |
| 7 | **Track**: precision stated (beat = m·R·ε), specified for R ≤ 8 kHz, hysteresis on N, unvoiced frames hold R, modulation steps through multiples | "Aliases stay harmonic" is true only to the detector's error; returning to `rate` on every unvoiced frame would chirp on each pick attack; N flipped on vibrato |
| 8 | **`rate_shift`** (−72 – 0 st) added as the EXP sweep control | A linear 50 – 48,000 Hz range gives a 7-bit pedal 377 Hz steps and half its travel to the top octave. Semitones are a contract unit, linear, and feel right |
| 9 | **Off defined once**: R ≥ min(fs, 48,000); stated as the one host-rate-dependent setting | The draft's "no oversampling" claim was silent on clipping at fs when Off |
| 10 | **Defaults 10 bits / Off → 8 bits / 16 kHz** | The converted catalogue default is inaudible |
| 11 | **A-law removed**; ADPCM / Delta staged; µ-law specified as the continuous curve (bench: within one step of G.711, not "equal") | A-law is not a distinct sound; enums can grow, never shrink. Continuous bits and G.711's segmented tables cannot be bit-equal |
| 12 | **Units and conversion settled** (§3), with the fallback written out | Open question 1 – 2 |
| 13 | **Bench**: ideal-model bounds per band, kernel row, Mix row, tails rows (82 Hz, attack), track on a synthetic tone, preset level row; **MDA row is no longer a null** | A band-limited staircase cannot null against MDA's naive one, and the DAC curves differ by their reference level |
| 14 | Facts: MDA's curve adds 0.29 dB per dB below full scale (not "up to +6 dB"); the hold restart is a pattern jump, not strictly a click; "level jumps" removed from the list of things fixed; Drum Machine flagged as a voicing; `bbd` removed from `blep`'s candidate users; CPU estimate raised | Accuracy |

Still open:

| Point | Owner |
|---|---|
| Catalogue entry: real units (min / max / default), category `lofi`, new default preset; engine-table conversion; count of saved rig chains using `bitcrusher` (decides whether the faithful conversion exists at all) | golem-rack |
| Whether golem's EXP assignment can target `rate_shift` (a non-catalogue id) | golem-rack |
| References for the Sampler / Drum Machine / Console voicings (§7.9) | Raph |
| Kernel group delay (the dry's alignment), Console FIR coefficients, CVSD time constant, 2-bit ADPCM table, ladder magnitude: all [I] / [A] until measured | Bench |
| YIN range down to 40 Hz for bass lanes: only if the CPU bound holds | Bench |
| Raspberry against drive's thrash red at tile size | Raph, on the family sheet |
