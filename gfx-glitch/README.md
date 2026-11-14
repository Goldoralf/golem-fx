# gfx-glitch

> **Status: CODED** (v0.1.0, 2026-11-14). The design v2 was reviewed by Fable 5.2 (§7). Then Raph decided:
> **no footswitch interaction**. You set the pedal up beforehand and play, so the freeze / pad engine
> and its counters are gone, and pads come from Sustain and the Hold effects.
>
> - **Engine**: a symmetric STFT of 512 – 32,768 points, its frame work spread over the hop on sample
>   counts.
> - **Spectrum**: sustain (T60), pitch, spread, cloud, three effect slots with 29 effects, and density.
> - **Wet**: drift (SSB), low cut, tone, soft limiter. An LFO and an envelope modulate.
> - **Tests**: 5 behaviour tests pass; the bench is green (28 / 28).
> - **CPU**: 317 ns per stereo frame at default settings (1.5 % of a core). 3.3 % with three slots,
>   density, sustain, pitch and cloud all on.
> - **Listening**: renders on golem's DI takes are in `bench-out/glitch/`. 17 factory presets (§5);
>   Raph's ear is still to come.

golem's **spectral pedal**. It cuts the guitar into frequency bins and lets you play with them:
- sustain the partials into a pad;
- blur everything into a cloud;
- shift or stretch it;
- thin it, crush it, scatter it, stutter it.

It is built for the stage: the dry guitar is never delayed, its CPU is flat, and you set it before
playing.

It is not a reverb (gfx-reverb, gfx-shimmer), a granular delay or a pitch shifter (gfx-pitch). It uses
the suite's ids where the meaning is the same (`mix`, `pitch`, `rate`, `sync`, `lpf`, `hpf`,
`env_sens` / `env_att` / `env_rel`, `width`, `trails`).

## 1. The sounds it is built for

- **Pads from what you play.** A long Sustain or a Hold effect lets each partial ring after the pick:
  - chords stack into a pad;
  - Ring Hold (the default) keeps the strong partials and lets the weak ones go.
- **Cloud.** A long analysis, a long sustain and random phases: everything turns into a slowly
  evolving wash.
- **Octave and synth layers.** Pitch ±24 st on a sustained wash: organ and string layers.
- **Textures that move.** Spread, slide and bin shuffles under a tempo-synced LFO or the pick attack.
- **Lo-fi and broken.** Bin quantisation (codec colour), corrupted decoding, scattered or stuttered
  holds on clean arpeggios.

## 2. State of the art

Survey of 2026-11-13. **[V]** read in a manual, a paper or a source · **[I]** inference or not re-checked.

| Reference | What makes it special | What we take |
|---|---|---|
| Unfiltered Audio SpecOps (manual, read) [V] | An STFT of 128 – 32,768 points with spectral frame feedback, pitch / inharmonic shift / stretch, three effect slots on frequency regions, a per-bin compander, and patchable modulators. Its latency equals the FFT size; large windows spike the CPU; a size change drops out | Region-limited bin effects in slots and a per-bin compander. Not its latency rule, not its patching |
| gfx-dsp `vocoder.rs` (ours) [V] | A peak-locked phase vocoder | Peak regions moved as rigid units (the frequency map) |
| Paulstretch (P. Nasca, GPL) [I] | Random phases on long windows: the reference cloud | `cloud` |
| Michael Norris Spectral, Ableton Spectral Time, CDP pvoc (LGPL) [I] | Spectral freeze, blur, shuffle as studio tools | Confirms the effect set |

Our own algorithms, structure and names. SpecOps's manual describes behaviours; we design from musical
goals. Nothing comes from its code, presets or UI.

## 3. Design (as coded)

```
in ─┬──────────────────────────────────────────────────────────── dry (unity, never delayed; `align` option) ─▶ (+) ─▶ out
    │                                                                                                              ▲
    └▶ trim ─▶ STFT ─▶ SUSTAIN ─▶ MAP ─▶ CLOUD ─▶ SLOT A ─▶ SLOT B ─▶ SLOT C ─▶ DENSITY ─▶ ISTFT ─▶ drift ─▶ HP ─▶ LP ─▶ wet ─▶ limiter
      envelope (env_sens, env_att, env_rel) · LFO (shape, rate / sync) ──▶ one destination each
```

### Engine (`size`, `window`), engine.rs

- **Sizes**: 512 · 1,024 · 2,048 · 4,096 · 8,192 · 16,384 · 32,768 points, at hop size / 4.
  - The wet is late by **size + hop**. At 48 kHz that is 13 ms, 27 ms, 53 ms, 107 ms, 213 ms, 427 ms and
    0.85 s.
  - Sizes are in samples at every rate: at 96 kHz, 1,024 points means 13 ms and twice the bin width.
  - Presets are written at 48 kHz.
- **Window** (analysis / synthesis; final order, append-only):
  - **Smooth**: Hann / Hann.
  - **Raw**: rectangular / Hann. Transparent when nothing acts; it splinters as soon as something does.
  - **Ringing**: rectangular / rectangular. Hard frame edges, so any bin effect buzzes at the frame
    rate, tuned by the size.

  The synthesis window is normalised so the analysis × synthesis products sum to one: with every stage
  neutral, **the wet is the input delayed** (to −90 dB at every size and window, tested).
- **Frames are rotated by N / 2**: phases refer to the frame's centre, so a partial's bins share its
  phase. Without this, a peak moved by an odd number of bins comes out wrong.
- **Per-sample schedule**: a frame is worked in 16 steps per channel, spread evenly over the hop on
  sample counts, never on host blocks. Its output is overlap-added from one hop after the analysis.
  - The steps: FFT, polar in four quarters, sustain, map + cloud, slot A, slot B, slot C, density,
    cartesian in four quarters, inverse FFT + overlap-add.
  - The result is identical for any block size (the bench's block-size check passes at −240 dB).
  - Parameters are latched when the frame is analysed.
- **Size or window change**: a second engine starts on the live input. Once its first frames (which
  still hold the silence it started on) have played out, the wet crossfades over max(50 ms, its size),
  equal power. Both engines run during the change.
- **Memory**: sized in `prepare` for 32,768 points, about 12 MB in stereo for the two engines.
  `prepare` also writes to every buffer, so no first touch of a fresh page lands in `process`.

### Sustain (`sustain`, `damp`)

- **`sustain`** (0 – 10,000 ms, 0 = off) is the **T60** of a spectral max-hold:
  - each bin keeps max(input, held × a), where a = 10^(−3 · hop / (T60 · fs));
  - while held, its phase runs at the frequency of its region's peak, so the bins of one partial stay
    locked;
  - a louder input takes over with its own phase.

  This gives the same decay at every size (1,024 against 8,192: within 15 %, tested).
- **`damp`** (0 – 100 %): the highs sustain less. The T60 is divided by up to 8 at 10 kHz at 100 %.

### Frequency map (`pitch`, `spread`, Move slots)

Each peak region of the sustained spectrum moves as a rigid unit, keeping its bins' relative phases.
- The peak's position is interpolated, and its frequency f goes through:
  1. `pitch` (±24 st, f × 2^(st/12));
  2. `spread` (±100 %): f′ = 100 Hz × (f / 100 Hz)^(1 + 0.25 · spread), an inharmonic stretch (up) or
     squeeze (down);
  3. the **Move** slots in order (18 – 20, below).
- A peak's phase runs at its new frequency.
- What leaves 0 – Nyquist is dropped.

### Cloud (`cloud`)

Each frame's output phases get a fresh random offset of up to ±π · cloud. At 100 % with a long size and
sustain, this is the Paulstretch wash.

### Effect slots A, B, C, slots.rs

Each slot has:
- `fx_*`: the effect;
- `amt_*`: the amount, 0 – 100 %; 0 skips the slot;
- `lo_*` / `hi_*`: the region in octaves above 20 Hz, 0 – 10 (20 Hz – 20.5 kHz). If lo > hi the two
  are swapped.

The order is A → B → C.

Common rules:
- Levels are in dB **relative to the frame's peak**, so a preset does not depend on the guitar's level
  or the size.
- Level-dependent gains are smoothed over 20 ms.
- Probabilities are **per second**; counts are fractions of the region.
- Randoms are seeded. Each channel's randoms blend a shared and an own source by `width`.
- A bin that leaves the region drops its hold.

The list (enum by rank, final order, append-only):

| # | Effect | Amount 0 → 100 % |
|---|---|---|
| 0 | Off | Slot skipped |
| **Filter** | | |
| 1 | Low Slope | Full level at `lo`, falling to −(amount × 60) dB at `hi` and above |
| 2 | High Slope | The mirror |
| 3 | Band Keep | Outside the region: down by up to 60 dB |
| 4 | Band Cut | Inside: down by up to 60 dB (tested: ≥ 40 dB on noise, ±1 dB outside) |
| 5 | Band Lift | Inside: up by up to 18 dB |
| 6 | Tilt | Brighter: up to +6 dB per octave through the region's centre, flat shelves beyond |
| **Tame** | | |
| 7 | Quiet Cut | Bins under −60 → −6 dB re peak removed (3 dB knee) |
| 8 | Loud Cut | Bins over 0 → −40 dB re peak removed: noise and air remain |
| 9 | Sharpen | Gain (bin / peak)^(3a): quiet bins fall faster |
| 10 | Thin | Bins more than 12 → 0 dB under both neighbours cut by up to 60 dB |
| 11 | Peak Only | Everything but the loudest peak's region in the slot cut by up to 60 dB |
| **Hold** | | |
| 12 | Hold | A regional max-hold, T60 0 → 10 s |
| 13 | Scatter Hold | Each bin frozen for 50 – 500 ms, starting at 0 → 20 per second |
| 14 | Stutter Hold | The whole region frozen for 30 – 250 ms, starting at 0 → 30 per second |
| 15 | Attack Hold | On an attack in the region (energy up by 9 → 3 dB in a frame), the region is held for 0.2 → 1 s |
| 16 | Ring Hold | A max-hold whose T60 (0.05 → 6 s) grows with the bin's level relative to the loudest: strong partials ring |
| 17 | Haze Hold | A max-hold (T60 1 → 5 s) plus a slow random walk of each bin's level (±0 → 12 dB) |
| **Move** (in the frequency map) | | |
| 18 | Spread Region | Partials spread upwards from `lo` (×1 → ×1.5); past `hi` they are dropped |
| 19 | Squeeze Region | Squeezed towards `lo` (×1 → ×0.67) |
| 20 | Slide Region | Content slid up by 0 → 1 region width, wrapping round inside the region |
| **Texture** | | |
| 21 | Blur | Each bin replaced by the mean over ±0 → 300 cents |
| 22 | Crush | Levels quantised to 64 → 2 steps over 60 dB, phases to 64 → 4 steps |
| 23 | Corrupt | Crush whose level table is read 0 → 7 steps off (alternating up and down) and whose phase table is scrambled |
| 24 | Sparkle | Each bin's level × a random factor of ±0 → 12 dB, new every 20 ms |
| 25 | Bin Clip | Bins limited to 0 → −30 dB re peak (tanh knee), the region's power kept (make-up ≤ +24 dB) |
| 26 | Flip | The region reversed in frequency, from its edges inwards (100 % = all) |
| **Glitch** | | |
| 27 | Phase Swap | Phases of mirrored bins swapped (edges inwards), levels kept |
| 28 | Clump | A random bin copied over its 1 → 16 neighbours for 100 ms, at 0 → 30 per second |
| 29 | Shuffle | 0 → 10 % of the region's bins swapped in pairs, new pairs every 20 ms |

Every effect is neutral at 0 % and stays finite and bounded at 100 % (tested on noise plus a chord).

### Density (`density`)

After the slots, each bin has its own compressor / expander (−100 – +100 %, 0 = off):
- The gains are computed on the spectrum smoothed over ±2 bins, against the mean level of the bins
  above −60 dB re peak.
- **Above 0**: compression towards that mean, up to 8 : 1. Fuller and flatter.
- **Below 0**: expansion, up to 1 : 3. Sparser.
- Attack 10 ms, release 200 ms.
- The frame never comes out louder than it went in.

### Modulation

- **LFO**:
  - `lfo_shape` (final order): Sine · Triangle · Saw Up · Saw Down · Square · Random Steps · Random
    Smooth;
  - `rate` 0.02 – 20 Hz, or `sync` (the contract's list; phase from the host position while it plays);
  - `lfo_depth` ±100 %, `lfo_dest`.
  - A 3 ms smoother on its output, so there are no edge clicks.
- **Envelope**: `env_sens`, `env_att`, `env_rel` (the contract's level-independent follower), plus
  `env_depth` ±100 % and `env_dest`.
- **Destinations** (final order, append-only), at full depth:

  | Destination | Moves by |
  |---|---|
  | Off | Nothing |
  | Amount A · Amount B · Amount C | ±100 % |
  | All Amounts | ±100 % on all three |
  | Sustain | ±5 s |
  | Pitch | ±12 st |
  | Spread | ±100 % |
  | Drift | ±500 Hz |
  | Region A · Region B · Region C | `lo` and `hi` together, ±5 oct |
  | Density | ±100 % |
  | Tone | `lpf` ±3 oct |
  | Wet | Gain × (1 ± 1) |

  A negative envelope depth to Wet ducks the wet while you play.
- Spectral destinations act per frame, latched at analysis. Drift, Tone and Wet act per sample.

### Output

- **`mix`** (0 – 100 %, default 35 %): the dry stays at unity up to 50 % while the wet rises to unity.
  Above 50 % the dry falls, to −∞ at 100 %. The pedal never drops the dry when it is engaged.
- **`drift`** (±500 Hz): every partial moves by the same number of Hz (bells, metal). It is a
  single-sideband shifter on the wet: a Hilbert pair of all-pass cascades and a quadrature oscillator.
  - The image is rejected by more than 35 dB (tested).
  - It is engaged through a 30 ms crossfade, never at a zero crossing of a sweep.
- **`hpf`** (20 – 500 Hz, default 60 Hz) and **`lpf`** (1 – 20 kHz, default 12 kHz): 4th-order filters
  on the wet.
- **`wet`** (−60 – +6 dB): the wet's level for scenes. It leaves `mix`'s law intact.
- **Soft limiter** on the wet: a tanh knee from −6 dBFS, ceiling −1 dBFS.
- **`align`** (bool, **setup only**, default off): delays the dry by the wet's latency and reports it
  (crossfaded over 20 ms).
  - Never make it a scene value: a reported latency that changes at runtime makes REAPER and Ardour
    recompute their delay compensation, which can click.
  - A coherent wet (a filter effect, low sustain) under 100 % mix with `align` off gives a comb (small
    sizes) or a slap-back (large sizes). Such presets are written at 100 % or with `align`.
  - Pads and clouds are decorrelated and need no alignment.
- **Stereo**: 2 / 2 first, 1 / 1 second (stereo by nature). `width` scales the per-channel difference
  of the randoms (0 = identical channels).
- **`bypass`** (the VST3 bypass) with **`trails`** (default on): the wet rings out on silence.
- **Safety**: a frame that goes non-finite resets its channel's spectral state; a non-finite output
  resets the engines and fades the wet back in over 10 ms. Real time: no allocation, lock or I/O in
  `process`. Deterministic (seeded; `reset` reseeds).

## 4. Parameters

New block: no catalogue ids. Every range is linear (golem contract). Enum orders above are final;
options only ever get appended. In this version, as in the other gfx pedals, the order of PARAMS is the
order of the plugin's parameters.

### Front panel (the pedal's face)

| id | Name | Type | Range | Default | Role |
|---|---|---|---|---|---|
| `fx_a` | Effect | e | §3 list | Ring Hold | Slot A's effect: the pedal's main colour |
| `amt_a` | Amount | f | 0 – 100 % | 40 % | How much of slot A's effect |
| `sustain` | Sustain | f | 0 – 10,000 ms | 0 ms | How long each partial rings after it is played (T60): the pad |
| `mix` | Mix | f | 0 – 100 % | 35 % | Dry / wet: the dry stays at unity up to 50 %, then falls |

### Expert

| id | Name | Type | Range | Default | Role |
|---|---|---|---|---|---|
| `size` | Size | e | 512 · 1024 · 2048 · 4096 · 8192 · 16384 · 32768 | 2048 | Analysis length: small = live and grainy, large = smooth pads and clouds; the wet is late by size + a quarter |
| `window` | Window | e | Smooth · Raw · Ringing | Smooth | Frame shape: Raw splinters, Ringing buzzes at the frame rate |
| `trim` | Input | f | −24 – +24 dB | 0 dB | Level into the spectral engine |
| `cloud` | Cloud | f | 0 – 100 % | 0 % | Random phases each frame: from focused to a blurred wash |
| `damp` | Damp | f | 0 – 100 % | 30 % | Shortens the sustain of the highs, like a real decaying pad |
| `pitch` | Pitch | f | −24 – +24 st | 0 st | Shifts the spectrum by semitones, partials kept whole |
| `spread` | Spread | f | −100 – +100 % | 0 % | Stretches the partials apart (up) or squeezes them (down): inharmonic, bell-like |
| `drift` | Drift | f | −500 – +500 Hz | 0 Hz | Moves every partial by the same Hz: metallic, detuned |
| `lo_a` | Region A Low | f | 0 – 10 oct | 0 | Bottom of slot A's region, in octaves above 20 Hz |
| `hi_a` | Region A High | f | 0 – 10 oct | 10 | Top of slot A's region, in octaves above 20 Hz |
| `fx_b` | Effect B | e | §3 list | Off | Slot B's effect, after A |
| `amt_b` | Amount B | f | 0 – 100 % | 0 % | How much of slot B's effect |
| `lo_b` | Region B Low | f | 0 – 10 oct | 0 | Bottom of slot B's region |
| `hi_b` | Region B High | f | 0 – 10 oct | 10 | Top of slot B's region |
| `fx_c` | Effect C | e | §3 list | Off | Slot C's effect, after B |
| `amt_c` | Amount C | f | 0 – 100 % | 0 % | How much of slot C's effect |
| `lo_c` | Region C Low | f | 0 – 10 oct | 0 | Bottom of slot C's region |
| `hi_c` | Region C High | f | 0 – 10 oct | 10 | Top of slot C's region |
| `density` | Density | f | −100 – +100 % | 0 % | Evens the bins out (fuller) or spreads them apart (sparser) |
| `lfo_shape` | LFO Shape | e | §3 list | Sine | The LFO's waveform |
| `rate` | LFO Rate | f | 0.02 – 20 Hz | 0.5 Hz | The LFO's speed when not synced (depends: sync = Off) |
| `sync` | LFO Sync | e | the contract's list | Off | Locks the LFO to the host tempo |
| `lfo_depth` | LFO Depth | f | −100 – +100 % | 0 % | How far the LFO moves its destination |
| `lfo_dest` | LFO Destination | e | §3 list | Off | What the LFO moves |
| `env_sens` | Envelope Sensitivity | f | 0 – 100 % | 50 % | How soft a pick still opens the envelope |
| `env_att` | Envelope Attack | f | 1 – 100 ms | 10 ms | How fast the envelope follows the pick |
| `env_rel` | Envelope Release | f | 20 – 2,000 ms | 200 ms | How fast it falls back |
| `env_depth` | Envelope Depth | f | −100 – +100 % | 0 % | How far the envelope moves its destination; negative ducks |
| `env_dest` | Envelope Destination | e | §3 list | Off | What the envelope moves |
| `wet` | Wet Level | f | −60 – +6 dB | 0 dB | The wet's level, for scenes |
| `hpf` | Low Cut | f | 20 – 500 Hz | 60 Hz | Removes rumble from the wet |
| `lpf` | Tone | f | 1,000 – 20,000 Hz | 12,000 Hz | Darkens the wet |
| `width` | Width | f | 0 – 100 % | 70 % | How differently the two channels' randoms move |
| `align` | Align Dry | b | off / on | off | Delays the dry to match the wet; setup only, never a scene value |
| `trails` | Trails | b | off / on | on | The wet rings out when the pedal is bypassed |
| `bypass` | Bypass | b | off / on | off | The VST3 bypass |

## 5. Applications and listening renders

Rendered with `gfx-bench render` on golem's guitar DI takes (`library/di-refs/guitar/clean.wav` and
`lead-clean.wav`), stereo, in `bench-out/glitch/<take>-<name>.wav`:

| Render | Settings |
|---|---|
| 01-default | Ring Hold 40 %, mix 35 % |
| 02-cloud-wash | size 16,384, sustain 4,000 ms, cloud 80 %, Blur 30 %, mix 50 % |
| 03-slow-swell | size 8,192, sustain 2,500 ms, density +40 %, no slot, mix 50 % |
| 04-shimmer-haze | Haze Hold 50 % + Sparkle 20 % on 7 – 9.3 oct (2.5 – 12 kHz), sustain 1,500 ms, mix 50 % |
| 05-lofi | size 1,024, Crush 50 %, tone 7 kHz, mix 100 % |
| 06-broken-codec | size 1,024, Corrupt 60 %, Shuffle 20 %, mix 100 % |
| 07-stereo-stutter | size 1,024, Stutter Hold 40 %, width 100 %, mix 50 % |
| 08-octave-pad | size 8,192, sustain 5,000 ms, pitch +12, tone 8 kHz, mix 40 % |
| 09-metal-bells | drift +150 Hz, Thin 50 %, sustain 3,000 ms, mix 50 % |
| 10-ghost-partials | Quiet Cut 60 %, Peak Only 30 %, sustain 800 ms, mix 60 % |

17 factory presets are in `presets/gfx-glitch.json` (format of presets/README.md), default `ringing-partials`.
They are designed, not copied from any third-party pack. Each is rendered on both DI takes in
`bench-out/glitch/presets/<take>-<id>.wav`; Raph's listening is still to come.

## 6. Measurements

**Bench** (`gfx-bench check gfx-glitch`, every rate): 28 / 28. Two bounds are declared:
- **Silence floor −80 dBFS**: Ring Hold's tail is the effect; it measures −92 to −109 dBFS.
- **Step click bound −40 dB**, for two cases:
  - trim +24 → −24 dB drives the wet's limiter, then the held partials fall back through it as they
    decay (−42 dB);
  - the Ringing window's hard edges while its engine fades in (−45 dB at 192 kHz).

**Behaviour tests** (`cargo test -p gfx-glitch`):
- identity at every size × window, stereo;
- drift up by 200 Hz with the image under −35 dB, and pitch +12 lands an octave up;
- every effect neutral at 0 % and bounded at 100 %;
- Band Cut depth;
- sustain T60 independent of the size.

**Hop spurs** (Fable's metric: 220 Hz sine, effect at 50 %, size 2,048; the strongest component at
220 ± k · fs / hop re the carrier; `cargo test -p gfx-glitch probe_hop_spurs -- --ignored --nocapture`):

| Result | Effects |
|---|---|
| ≤ −76 dB | Off, the filters, Thin, Peak Only, Hold, Scatter, Stutter, Attack, Ring, Clump, Shuffle |
| −65 dB | Crush |
| −58 to −52 dB | Sharpen, Sparkle, Haze, Quiet Cut |
| −45 dB and −42 dB (over the −50 dB gate) | Blur and Bin Clip. They reshape a partial's main lobe, which no window sum can hide; it is part of their sound, to be judged by ear |
| −16 dB and −13 dB | Corrupt and Phase Swap, broken by intent |
| Not meaningful | Loud Cut, the Move effects and Flip: the carrier itself is cut or moved |

**CPU** (48 kHz):
- **Default settings**: 169 ns per sample mono, 317 ns per frame stereo (1.5 % of a core).
- **Everything on** (three slots, density, sustain, pitch, cloud), stereo, blocks of 64 samples:
  - mean 3.2 – 3.5 % of a core at every size;
  - worst block ≤ 25 % of its duration up to 16,384 points, and 72 % at 32,768 (one FFT step; the
    gate was 25 %). 32,768 with blocks of 64 is the one combination to avoid.
- `cargo test -p gfx-glitch probe_cpu_worst_block -- --ignored --nocapture`.

**Size change** (a chord, 1,024 ↔ 8,192, 512 → 32,768): no gap; the deepest 10 ms dip during the
crossfade is −5 to −9 dB.

## 7. History

**Fable 5.2's review of design v1 (2026-11-13)**. Fable modelled v1's low-latency engine
(an asymmetric analysis window, 2 × 256 synthesis) in numpy. Findings and what changed:

| Finding | What changed |
|---|---|
| B1: the asymmetric engine buzzes under bin effects | All engines are symmetric; 512 and 1,024 are the live sizes |
| B2: a per-bin frozen frame is phasey | Peak regions move and hold as rigid units, with phases locked to the peak |
| B5: per-frame quantities changed meaning with the size | Time constants (T60), rates per second, fractions of bins |

Also from that review:
- CPU budgeted per sample, with an absolute worst-block bound;
- true latency size + hop;
- the comb of a coherent wet (`align`, the dry-at-unity `mix` law);
- levels relative to the frame peak, with knees;
- every effect bounded and defined;
- regions in octaves;
- `drift` in the time domain.

**golem-rack (2026-11-13)**: a late wet with a reported latency of 0 is fine; `align` stays setup-only.

**Raph (2026-11-14): no footswitch interaction.** This removed the separate pad engine, `freeze`,
`clear`, `freeze_mode`, `capture`, `freeze_in` / `freeze_out`, `motion`, `octave`, `pad` and the `state`
meter (findings B3 and B4 lapse with them). No golem work is needed: no new `foot_mode`, no `trigger`
flag. Pads come from Sustain and the Hold effects, set before playing.
