# gfx-grains

> **Status: DESIGNED** (2026-11-14) — design only, no code. Design by Claude Opus 5.5, reviewed by
> Fable 5.2 the same day: the fixes are in the text, the list is in the [Review](#review-fable-52-2026-11-14).

golem's **granular processor and granular delay**. A rolling stereo buffer is played back as a cloud of
up to 128 short windowed grains. Each grain has its own position, length, pitch, direction, envelope and
place in the stereo field. Around that core sit a **hold** (freeze) that sustains a chord without
sounding like a loop, **tempo-synced rhythmic grains**, grains taken from **your last notes** (strum and
arpeggio chains), a **phrase loop** cut into slices that can be shuffled, transposed and reversed, and
**feedback** into the buffer (granular shimmer, smeared echoes).

It stands for golem's `granular` block ("Granular Freeze", today ZamGrains) and keeps its parameter ids
`freeze`, `grains` and `pos` and their display names ([golem contract](../docs/golem-contract.md)).
Stereo by nature: **2 / 2 first**, 1 / 1 second. **Zero reported latency**: the dry passes untouched,
and the wet is a delay by nature (≥ 1.5 ms).

The idea: **a granular engine with no artifacts of its own.** Every artifact of the usual granulators
comes from the engine, not from the sound: pitched grains alias (linear or no interpolation), clicks at
grain edges, a loop seam in the freeze, level that jumps with density, randomness that changes with
block size. Here, pitched grains are **band-limited** (a ratio-tracking windowed-sinc kernel over the
buffer and its half-rate copy), every envelope is C¹ at its edges, the freeze is a **stationary random
scan**, never a loop, the level is **normalised to the source's** inside the bounds the cloud's
statistics give, and the cloud is **reproducible to the sample** (seeded, block-size-independent). What
remains is the texture you asked for.

## 1. State of the art

Survey of 2026-11-14. **[V]** read in source code or a manual · **[M]** manufacturer page or listing ·
**[F]** forum post or review · **[A]** computed here · **[I]** inference, or not re-checked at the
source · **[U]** could not be verified.

### What golem plays today: ZamGrains

Read in [`ZamGrainsPlugin.cpp`](https://github.com/zamaudio/zam-plugins/blob/master/plugins/ZamGrains/ZamGrainsPlugin.cpp)
and the installed `ZamGrains_dsp.ttl` (GPL-2.0+) [V, read again at the review]. Mono 1 / 1. Ports:
Output Gain (−60 … 0 dB), **Grains** (1 – 100, integer, default 1), Grain Speed `gs` (0.1 – 20, default 1), Play Speed `ps` (0.1 – 20,
default 1), Loop time `time` (5 – 1,000 ms, default 160), **Freeze** (toggle), and three *output* ports
(Grain Position, Playback Position, Final Position). golem's catalogue exposes `freeze` (0 – 1),
`grains` (1 – 64) and `pos` (0 – 1).

The algorithm [V]: a buffer of `time` ms; a window of `time / grains` samples; **two** read heads half a
window apart, each `z[(S&H(posz · ps) + phase · gs) mod L]` with **integer indices** (no
interpolation), weighted by sin² windows. The output is 100 % wet; there is no dry. Recording stops while
Freeze is on.

What this means for golem [A]:

1. **At the defaults (gs = ps = 1) the block is transparent.** Each head's start is the write index
   sampled at its window start, plus its phase: both heads read the sample just written. The output is
   the input times sin² + cos² ≈ 1 (a small ripple, the window uses N − 1 for N).
2. **`grains` is not a number of grains.** There are always two heads; `grains` sets the window to
   160 ms / grains (8 → 20 ms, 32 → 5 ms, 4 → 40 ms). Since both heads read the same sample, it changes
   almost nothing.
3. **`pos` does nothing.** "Grain Position" is an *output* port in ZamGrains 4. golem's engine table
   already notes it as absent.
4. **Freeze is a 160 ms hard loop.** Recording stops, both heads keep reading the same index, so the
   output is the last 160 ms repeated, with an unwindowed seam (a click each cycle unless the content
   happens to be periodic).
5. So golem's three presets ("Standard" 8 / 0.5, "Shimmer cloud" 32 / 0.5, "Stutter" 4 / 0.3) all play
   the dry signal, and with Freeze on all play the same stutter. Their names state an intent that the
   plugin never delivered.

### golem's own freeze: GoleM Freeze

Read in `~/golem/plugins/golem_freeze/src/lib.rs` [V]: spectral, STFT of 1,024 / 2,048 / 4,096 points
(Hann², 4× overlap), **reported latency** of the window (2,048 → 43 ms). When engaged it averages the
magnitudes of 4 frames, measures each bin's instantaneous frequency, and resynthesises with phases
advancing at that rate. `smear` adds phase jitter, `tone` tilts ±6 dB / oct, `decay` is an exponential
time constant after release, and `mix` is the level of the hold under the always-passing direct. It
gives a smooth, grainless sustain of the **notes**. Because of its latency it is kept off a guitar
lane's main path. Its footswitch drives `freeze`, not the bypass (`switch_param_of` in `golem_rig.lua`).
The block has been **quarantined since 2026-11-12** (catalogue note: "it does not work well", judged by
ear) [V]: today golem has no working hold gesture at all.

### Hardware and plugins

| Reference | What it does best | What we take |
|---|---|---|
| [Mutable Instruments Clouds](https://pichenettes.github.io/mutable-instruments-documentation/modules/clouds/manual/) (MIT code) [V] | Position / size / pitch / density / texture. Density left of noon = constant rate, right = random. Texture morphs box → triangle → Hann. Freeze stops recording. Blend = dry/wet, spread (random pan), feedback, reverb. "Between 40 and 60" grains | The control set; regular ↔ random onsets; window morph; random pan; feedback into the buffer |
| [Mutable Instruments Beads](https://pichenettes.github.io/mutable-instruments-documentation/modules/beads/manual/) (MIT) [V] | Time (0 = most recent), **clocked density** (divider / probability), size **CCW = reversed** grains up to 4 s, shape independent of direction, pitch ±24 st with "virtual notches", **attenurandomizers** per parameter, delay mode | Probability per grain (`chance`), ±24 st, per-parameter randomness, the envelope is independent of direction |
| [Hologram Microcosm](https://cdn.shopify.com/s/files/1/0920/2928/8752/files/MC_manual_WEB.pdf) [V] | Eleven engines. **Mosaic** (overlapping loops at ½ / 1 / 2 / 4× speed), **Seq** (loops reordered into rhythms), **Glide** (loops gliding between speeds), **Haze** (grain wash), **Tunnel** (drone micro-loops), **Strum** ("rhythmic chains of recent note onsets"), **Arp** ("recent note onsets into arpeggios"), Blocks / Interrupt (glitches), Pattern / Warp (tap delays). Tap-tempo. 60 s phrase looper | Grains taken **from note onsets** (Strum mode); the phrase cut into slices that can be reordered, transposed and reversed (Phrase mode); per-grain glide |
| Red Panda Particle 2 ([manual](https://media.rockboard.de/Artikell_PDF/Manual_PDF/Red%20Panda/EN/RPA_PARTICLE_V2_Manual_EN.pdf)) [I] | Granular delay and pitch modes. Chop = grain size, smooth or stepped. **Grain freeze** (buffer held) vs delay freeze (output recycled). Pitch quantised to semitones / octaves / octaves + fifths / intervals | Interval sets for per-grain pitch; freeze as "buffer held" |
| Chase Bliss Mood [I] | A wet channel (reverb / delay / slip) and a micro-looper (env / tape / stretch) running into each other; lo-fi clock | Phrase-loop transformations; "always listening" buffer |
| [Ableton Granulator III](https://www.ableton.com/en/packs/granulator-iii/) [M] | Classic (two overlapping grains per channel), Loop (crossfading playback with random position, **reverse chance**), Cloud (up to 20 unsynced grains) | Reverse as a probability; the three families of scheduling |
| Output Portal [I] | Big preset library of evolving granular textures. Grain pitch / density / size / spray with a macro and modulation matrix. Delay and reverb after the grains | That the texture comes from slow modulation of spray, pitch spread and size; the presets matter more than the panel |
| Arturia Efx Fragments [U] | Granular multi-mode effect with tempo-synced and scattered grain modes and a freeze | Confirms the sync / scatter / freeze triad |
| Audio Damage Quanta [I] | Granular synth with live input, per-grain pitch randomness quantised to scales | Scale-quantised pitch randomness |
| [Argotlunar](https://github.com/mourednik/argotlunar) (GPL) [I] | Real-time delay-line granulator. Each grain parameter (amplitude, duration, delay, pitch, glissando, inter-onset time, pan) has a randomisation amount | Glissando per grain; every grain parameter drawn once at grain start |
| Curtis Roads, *Microsound* (MIT Press, 2001) [I] | Synchronous vs asynchronous granular synthesis. Grains of 1 – 100 ms. A regular grain rate is heard as a pitch or formant. Short windows widen each grain's spectrum. Window shape matters | `regular` (a synchronous stream is a buzz, by design); envelope edges; the Particle Buzz use |

### What players love and hate [F] [I]

From pedal and plugin threads (Elektronauts Microcosm / Particle topics, Gearspace, The Gear Page; read
as patterns, not quotes):

- **Love**: pads from what you just played; octave clouds; "it plays back my own notes rhythmically";
  reverse swells; anything that moves slowly and is never the same twice.
- **Hate**: **the freeze sounds like a loop** (an audible period); **metallic buzz** at small sizes
  that the player did not ask for; **clicks** on grain edges or on freeze; **level jumps** when density
  or size moves; pitched-up grains that **fizz** (aliasing); muddy lows when the cloud is thick
  (the bass smeared under the guitar); randomness that is "too random to be musical" (pitch
  randomness without a scale); the freeze **catching the pick attack**, or the silence before a chord
  when the switch is a little late; CPU spikes on dense clouds; menus.

### What we conclude

1. **Band-limited grains.** None of the surveyed open engines band-limits pitched grains (Clouds and
   Beads interpolate; ZamGrains does not even interpolate) [I]. A ratio-tracking kernel over the buffer
   and one half-rate copy gives a fixed alias bound at a bounded cost per grain (§2.3).
2. **A freeze that is a field, not a loop.** Hold the last *L* seconds and keep drawing grains at random
   positions inside them. Use long windows, small detune and random pan. Nothing repeats with a period.
   Grains avoid transients and silence (the onset map, §2.6).
3. **Grains from note onsets.** gfx-dsp's `onset` module (gfx-swell's, shared) marks the buffer, so grains
   can start on notes. This gives Microcosm's Strum / Arp chains, and lets the freeze hold what rings
   after the last pick, without the pick.
4. **Statistics, not instantaneous gain, for level.** The expected overlap gives the two bounds
   (incoherent and coherent sum); a slow measurement places the gain between them. Never divide by the
   instantaneous window sum (that would flatten the grain texture into a compressor).
5. **Four schedulers** under one engine: Cloud (free, regular ↔ random), Rhythm (host grid), Strum
   (recent onsets), Phrase (the last phrase cut into slices).
6. **Musical randomness**: per-grain pitch drawn from **interval sets** (octaves, fifths, triads,
   scales); probability per grain; a `seed` to pick another reproducible cloud.

## 2. Design

```
in L/R ─┬──────────────────────────────────────────── dry (unity, `kill_dry`) ───────────────────┐
        │                                                                                         │
        └▶ + ◀── feedback (low_cut ▶ high_cut ▶ soft clip) ◀──────────────┐                       │
           ▼                                                              │                       │
   RECORD buffer (always records, 2·Lmax) ── onset map (gfx-dsp `onset`)   │                       │
     ├─ level 0 (fs)   ├─ level 1 (fs/2)                     half-band FIR │                       │
     │                                                                    │                       │
     │  freeze ↑ : HOLD = the last L of RECORD (copy-on-freeze, §2.6)      │                       │
     ▼                                                                    │                       │
   SCHEDULER (mode: Cloud · Rhythm · Strum · Phrase; seeded variates)      │                       │
     ▼  spawns grains: start, size, ratio(t), direction, envelope, pan     │                       │
   GRAIN POOL (128 voices) ── band-limited read (level k, kernel j) ──▶ Σ ─┴▶ × norm ▶ low_cut ▶   │
                                                                 high_cut ▶ × mix ───────────────┴▶ out
```

### 2.1 Buffers

- **RECORD**: stereo, interleaved, f32, always recording, length 2·*L*max + margin (*L*max = 10 s).
  A mirrored guard region at the end (as long as the longest kernel) makes every kernel read contiguous,
  with no wrap test.
- **Half-rate copy** (level 1): RECORD decimated by 2 through a 51-tap linear-phase half-band FIR, pass
  to 0.2 *f*s and ≥ 80 dB from 0.3 *f*s [A, Kaiser estimate], polyphase (≈ 13 MACs per output sample and
  channel). Its group delay (25 input samples) is constant and added to the read positions. Level 1 is
  written every 2 input samples. It is clean up to 0.4 of its own rate; between 0.4 and 0.5 it holds
  attenuated and folded content, which is why it is only read at ratios ≥ 2 (§2.3). A quarter-rate level
  is not needed (§2.3).
- **HOLD**: *L*max + 30 ms, with its own half-rate copy. Filled at freeze (§2.6).
- **Onset map**: per 32-sample block, one byte for the rise of an onset marked there (0 = none) and one
  byte of level (dB), for RECORD and HOLD (60 KB).
- Memory [A]: (20 + 10.03 s) × 2 ch × 4 B × 1.5 (half-rate copy) ≈ **17 MB at 48 kHz**, 35 MB at 96 kHz,
  69 MB at 192 kHz, in f32. Allocated in `initialize` (not on enable); see §7.6.

The wet never reads closer than **1.5 ms** to the write head on level 0 (72 samples at 48 kHz) and
**2.5 ms** on level 1. That lets the engine run in ticks of 32 samples, grain by grain (SIMD-friendly),
with feedback written back to the buffer: a grain never reads a sample of its own tick [A: level 0,
32 + 24 taps of the longest kernel = 56 < 66, the 1.5 ms of 44.1 kHz; level 1, 32 + 2 × 24 + 25 of
group delay = 105 < 110]. The ticks are counted from the last reset, not from the host's blocks.

### 2.2 Grains

A grain is drawn **once, at its start** (Argotlunar's rule), so moving any knob only affects new grains.
Movement is zipper-free by construction: no parameter needs smoothing except `mix`, the cuts and
`feedback`.

| Property | Drawn from |
|---|---|
| Start position *p* (delay from now, or from the hold's end) | mode's centre ± `spray` · *L* · *u*₁, with *u*₁ uniform in [−1, 1]; **folded** into the readable range (below) |
| Length *T* | `size` (ms) · (1 + 0.25·*u*₂·`spray`/100). Size jitter follows spray: one control for "scatter" |
| Ratio *r(t)* | 2^((`pitch` + *q*(`pitch_rand`, `scale`, *u*₃) + `detune`·*u*₄/100 + `glide`·*t*/*T*) / 12) |
| Direction | reverse if *u*₅ < `reverse` |
| Pan θ | `spread` · *u*₆ (equal-power) |
| Plays at all | *u*₇ < `chance` |

- **Fixed variate order.** Every grain draws twelve variates (*u*₁ … *u*₇, four re-draws for §2.6 and a
  spare) in the same order from one seeded xorshift per layer (gfx-dsp `lfo::Rng`,
  seed = 0x6772616E · (`seed` + 1), plus the layer's index), **whether or not they are used**.
  Turning `spray` from 10 % to 30 % then scales the *same* deviations. The cloud keeps its identity
  under the knob, and two renders always match.
- **Pitch quantisation** (*q*): with `scale` Off, a uniform interval in ±`pitch_rand` st. Otherwise, a
  **uniform choice among the members of the interval set** that lie within ±`pitch_rand` (0 always
  included). Drawing one member uniformly avoids the bias of quantising a uniform draw toward the
  members with wide catchment. Sets (relative to the played note, extended by octaves): Chromatic
  {all}, Octaves {0}, Fifths {0, 7}, Major Triad {0, 4, 7}, Minor Triad {0, 3, 7}, Major / Minor /
  Major Pentatonic / Minor Pentatonic (gfx-dsp `detect::SCALES`, entries 0, 1, 8 and 9). `pitch` itself
  is not quantised: it is the base transposition, continuous for EXP.
- **Ratio range**: 0.25 – 4 in total (±24 st), clamped after the sum.
- **Readable range**. A forward grain at ratio *r* > 1 gains (*r* − 1)·*T* on the write head, so its
  start delay is at least the level's minimum (§2.1) + (*r* − 1)·*T*. A reverse grain, or one at
  *r* < 1, falls behind, so it must end inside the buffer (live) or inside the hold (frozen). A draw
  outside that window is **reflected** back into it, not clamped: clamping would pile grains on the two
  edges, where they would read the same samples again and again (the one way this engine could sound
  like a loop). At `pos` 0, pitched-up grains are therefore pushed back slightly: this is physics, and
  §3 says so on the knob.
- **Voice pool**: 128. When the pool is full, **a new grain is dropped**, never stolen: nothing is cut,
  so nothing clicks. It is deterministic, and the read-only `active` meter shows it. The Poisson
  overlap at `grains` 64 stays under 89 voices 99.9 % of the time [A: 64 + 3.1·√64].
- **Two layers at once** (a hold releasing under the live cloud, or under a new hold, §2.6): two layers
  at 64 would average 128 and drop half their grains. So while two layers sound, each one's overlap is
  capped at 48 (96 in all, under 127 voices 99.9 % of the time [A]), and its normalisation uses the
  capped value. Never three: a third layer ends the oldest in 50 ms.

### 2.3 Band-limited reading

The problem: a grain at ratio *r* is a resampler. Above *r* = 1 its input must be band-limited to
*f*s / (2*r*), or the partials above fold back as inharmonic fizz. Below 1, the images must be
rejected. A fixed interpolator (linear, Hermite, Lagrange, gfx-dsp's 16-tap `SincTable`) does neither
at large ratios.

The solution, at a bounded cost per grain. **[P]** = measured on the review's numpy prototype (the
half-band, the kernel bank with its 129-row table, a sine sweep of 100 Hz – 18 kHz at each ratio).

- **Level choice: an octave switch.** Level 0 (the buffer) serves every *r* < 2; level 1 (half rate)
  serves *r* ≥ 2. The within-level ratio is ρ = *r* / 2^*k*: ρ ≤ 2 on level 0, and **1 ≤ ρ ≤ 2 on
  level 1**.
  - Why never ρ < 1 on level 1: a decimated level is only clean up to 0.4 of its rate. Its top band
    (0.4 – 0.5) holds what the half-band left of the octave above, folded, and the images of that band
    sit at 0.5 – 0.6, inside any kernel's transition. Read at ρ ≥ 1, all of it lands above 0.4 *f*s at the
    output, where folding is allowed. Read at ρ < 1, it lands below.
  - The first draft switched at ρ = √2 and read levels 1 and 2 at ρ down to 0.71. Measured [P]: at
    +6.1 and +7 st a 12.0 kHz source tone gives a spur at 17 kHz, **12 dB** under it; at +18.1 and +19 st
    a 5.9 kHz tone does the same. The octave switch removes the case, the 13.6 kHz bandwidth dip that
    came with it, and the quarter-rate level.
- **Kernel bank**: Kaiser-windowed sinc, 129 phases (linear interpolation between phases, each row
  normalised to a DC gain of 1). 17 cut-offs: one for ρ ≤ 1 (pass 0.40, stop 0.60 of the level's rate)
  and 16 for ρ ∈ (1, 2] in steps of 2^(1/16), each with pass 0.40 / ρ⁺ and stop 0.60 / ρ⁺ (ρ⁺ = the
  step's upper edge). β from Kaiser's formula for each transition.
  - **32 taps** for ρ⁺ ≤ √2 (9 rows), **48 taps** for ρ⁺ > √2 (8 rows): 347 KB of tables [A].
  - Stop bands [P]: 97 dB (ρ ≤ 1), 92 dB (2^(1/16)), 82 dB (2^(1/4)), **71.7 dB** (√2, 32 taps), 99 dB
    (2^(9/16), 48 taps), 88 dB (2^(3/4)), **75.2 dB** (2). Pass-band ripple ≤ 0.003 dB. With 32 taps
    alone the last three would be 67, 62 and 52 dB.
  - The 129-row table with linear interpolation is not the limit: its own images sit at −98 dB (32
    taps) and −110 dB (48 taps) [P]. 64 rows would give −86 dB, 32 rows −74 dB.
- **What it guarantees** [P]: for source content below 0.4 *f*s, every alias and image that lands below
  0.4 *f*s at the output is **≥ 72 dB** down (worst cases 72.8 dB at +17.9 st and 75.4 dB at +11.9 st;
  ≥ 79 dB elsewhere, ≥ 95 dB for ρ ≤ 1.06). Above 0.4 *f*s (19.2 kHz at 48 kHz) folding is allowed.
  That is the bound of the circuit pedals (contract, −70 dB).
  - Not covered, as in any interpolator: source content between 0.4 and 0.5 *f*s (19.2 – 24 kHz) read
    at *r* < 1 keeps an image next to it. A guitar, a voice or a keyboard has nothing there.
- **Bandwidth**: a grain's output is flat to 0.4 *f*s · 2^(−1/16) = **18.4 kHz at 48 kHz** at any ratio
  ≥ 1, and naturally to 0.4 *f*s · *r* when pitched down.
- **Glide**: the cut-off row follows ρ(*t*), tick by tick (32 samples). When the row or the level
  changes, the grain is read twice over one tick and crossfaded (at most 16 times per octave of glide).
  A kernel chosen once from the largest ratio would leave a +12 st glide starting at half the band, and
  a +24 st glide at a quarter (4.8 kHz).
- **Unity fast path**: when *r* = 1 exactly (`pitch` integer 0, no `pitch_rand`, `detune`, `glide`),
  the start is rounded to a whole sample and the grain copies samples, **bit-exact**, at 2 MACs per
  grain-sample. That is the default patch.
- **Rejected alternatives**: (a) one kernel widened ×*r* on level 0 alone (128 taps at +24 st); (b) an
  octave switch with 32 taps everywhere (52 dB at ρ = 2 [P]); (c) the √2 switch with 32 taps (above);
  (d) 48 taps everywhere (+50 % on the common small ratios, for nothing audible).

### 2.4 Envelopes

The window is *w*(φ), φ ∈ [0, 1] along the grain, in time (independent of direction, as in Beads).

- **`skew`** warps φ so the peak sits at *p* = 0.5 + 0.48·`skew`/100 (piecewise-linear warp). At −100 %
  the window is percussive (fast attack, long fall); at +100 % it is a swell.
- **`shape`** morphs the warped window: 0 % = flat (Tukey), 50 % = triangle, 100 % = Hann (Clouds'
  texture, in the same order). Values in between are linear blends from a 3 × 1,024 table.
- **Edge guard (click-free)**: the flat and triangular windows are multiplied by raised-cosine ramps of
  min(2 ms, *T*/4) at both ends, so they start and end at 0 with zero slope (C¹), never with a step or
  a corner. 2 ms keeps the splatter of each edge below about 500 Hz of bandwidth [A, raised-cosine main
  lobe ≈ 2 / ramp]. With a 2 ms grain the window degenerates to a Hann.
  - **The Hann is left alone**: it is C¹ already, and a guard on it would break its constant overlap-add
    (the unity of §2.5 and of the bench).
  - **`skew` is bounded by the same time**: the short side of a skewed window lasts at least
    min(2 ms, *T*/4). At ±100 % a 10 ms Hann would otherwise fall in 0.2 ms: a click.
- **Phase**: grain onsets are scheduled in f64 (§2.7). The window is evaluated at its exact phase,
  fraction of a sample included. Rounding each onset to a sample would leave a ripple in a regular
  stream's sum.
- Reverse + `skew` −100 % is a reversed sound with a hard attack. Reverse + `skew` +100 % is the
  classic reverse swell. Both stay available because the envelope does not follow direction.

### 2.5 Level: bounds from the cloud's statistics, the point between them measured

With *N* grains overlapping on average, each reading *x* (power σ²) through a window *w*, and *c* the
mean correlation between what two overlapping grains read, the wet power is

  *P* / σ² = *N*·E[*w*²] + *c*·(*N*²·E[*w*]² + *V* − *N*·E[*w*²])

where *V* is the variance of the window sum Σ*w* over time.

- **Two bounds.** *c* = 0 (the grains read unrelated samples): *P* = *N*·E[*w*²]·σ². *c* = 1 (they all
  read the same sample): *P* = (*N*²·E[*w*]² + *V*)·σ², the power of Σ*w* · *x*.
- ***V*** depends on the onsets: *N*·E[*w*²] for Poisson; ≈ *N*·E[*w*²] / *k* for a gamma interval of
  shape *k* (within 15 % [P]); the ripple of the periodic sum when metronomic (0 for a Hann at an
  integer *N* ≥ 2, computed from the window table when a parameter changes).
- The first draft used *N*·(*N* − 1)·E[*w*]² for the cross term, which is neither case: it left a
  regular Hann stream 1.0 dB low at *N* = 2 and a Poisson stream 2.3 dB high at *N* = 1 [P].
- E[*w*], E[*w*²]: from the current `shape` / `skew` (table, per tick).
- *N* = expected overlap = onset rate × mean length × `chance`, from each mode's onset rate (§2.7).

**Where *c* comes from.**
- **Pinned to 1** when the layer's grains provably read the same samples: ratio 1 (the unity path),
  `spray` 0, one position. The gain is then exact, and a regular Hann stream is the input, delayed
  (ZamGrains' transparent case, without its N − 1 ripple).
- **Measured everywhere else.** No formula can give it: it is the signal's autocorrelation at the
  offsets between grains. The draft's model, *c* = exp(−σ / 2 ms), is right at both ends (no scatter;
  more than about 30 ms of scatter) and wrong between them by up to **13 dB** at 64 grains, 6 dB at 8 [P]:
  white noise and a guitar DI decorrelate in a fraction of a millisecond, a 440 Hz note in 2 ms, a
  110 Hz note in 10 ms. That middle range is small `spray` (under 3 % of 1 s), `detune` alone, and a
  `pitch` swept through 0: common settings.
- The measurement uses two powers the engine already has, each smoothed over 400 ms and updated per
  tick: *A* = the layer's wet power before the gain, *B* = Σ over its grains of (*w*·*x*)², the power
  the layer would have if nothing were correlated. Then

    *g* = 1 / √( *N*·E[*w*²] · clamp(*A* / *B*, 1, (*N*²·E[*w*]² + *V*) / (*N*·E[*w*²])) )

  It brings the wet to the source's power, and can never leave the two bounds. Measured [P], Poisson
  clouds of 8 and 64 grains, scatter 0.1 – 100 ms, on white and pink noise, 110 and 440 Hz notes and a
  guitar DI: mean level within **0.6 dB** of the source in every case.
- Below −90 dBFS of *B* the ratio is held. After a reset it starts from the draft's model.
- *g* never reacts to the instantaneous Σ*w*, and 400 ms is longer than a grain's flicker. The price
  is small: at 8 grains and 10 – 100 ms of scatter, the 400 ms level of the cloud wanders over
  2.0 – 3.4 dB (5th to 95th percentile) with a fixed gain and over 1.9 – 2.9 dB with this one [P]. Level
  stays within ±2 LU from `grains` 1 to 64 (bench, §5).
- A regular stream of pitched grains is a comb filter (each grain is a copy a few ms away): the gain
  follows the mean level of what the comb leaves, slowly, and does not flatten the comb.

### 2.6 Hold (freeze)

- **Engage** (`freeze` on): the grains switch to **HOLD**, the last *L* = `length` of RECORD (+30 ms
  pre-roll for Phrase seams). The live layer stops spawning, so what you play over the hold stays dry.
  RECORD **keeps recording**, so the live layer has its history when the hold is released.
- **Copy-on-freeze**: HOLD is filled from RECORD oldest-first, at 8 samples per sample, after the
  engage (a `memcpy` of 24 floats per sample with the half-rate copy: no spike in any block, no
  allocation). A grain chooses its buffer when it starts: RECORD until the whole hold is copied, HOLD
  after. RECORD is intact for that long: the writer needs 2·*L*max − *L* ≥ 10 s to reach the hold's
  oldest sample, the copy takes *L* / 8 ≤ 1.25 s, and a grain lives 2 s at most.
- **Engage again while a hold is releasing** (hold a new chord while the last one fades: the hold
  pedals' most used move). The releasing layer is still reading HOLD, so the copy **waits** for it: the
  new hold plays from RECORD meanwhile. The wait is bounded by the writer: the copy must start within
  2·*L*max − *L* − *L* / 8 − 2 s of the engage (16.9 s at the default *L*, 6.75 s at 10 s). A release
  that would last longer is shortened to end there. A third layer ends the oldest in 50 ms (§2.2).
- **Placement** (Cloud, Rhythm): positions are drawn inside the hold and never cross its ends
  (the readable-range rule, §2.2: reflected, not clamped). So there is no seam to cross.
- **A field, not a loop**: with `scan` 0 the position centre stays put. Each grain draws its own offset
  (`spray`), its own length, its own detune and pan, and its own onset time (`regular` < 100 %). No
  quantity has a period, so there is no loop to hear. A frozen 30 s output has no autocorrelation peak
  in its envelope (bench, §5). Recommended starting point: `size` 300 – 600 ms, `spray` 40 – 70 %,
  `detune` 5 – 10 cents, `spread` 70 %, Hann.
- **The marks**: gfx-dsp **`onset`**, the module gfx-swell's design creates and gfx-ducker's review
  made the only one (`onset::Pick`, its detector A: 1.5 kHz high-pass, fast envelope in dB against its
  own 150 ms average, slope test; plus the level gate and the strum window, so a six-string strum is
  one onset). It runs on the input, sample by sample, and writes each onset's rise (dB) into the map.
  This crate does not write a detector of its own. The module's later detectors (flux, pitch jump) cost
  nothing in lateness here, since a mark is written back in time: an option for Strum on legato
  playing, not in v1.
- **Attack and silence avoidance (`skip`)**: 0 % = off. Above, an attack counts when its rise is at
  least 12 − 9 · `skip` / 100 dB (the module's own scale: 100 % also counts soft attacks).
  - **The field is what rings.** The hold's span becomes the part of *L* after the last counted attack
    (+ 40 ms), when that part lasts at least 100 ms; `pos` and `spray` then refer to it. Pressed 0.3 s
    after a chord, the hold is that chord, not the 1.7 s before it (the previous chord, or silence).
    Pressed within 100 ms of a pick, it is the span before that pick.
  - **Rejection is a rule, not a probability.** A draw whose source span touches an attack (5 ms before
    its mark to 40 ms after), or a block more than 30 dB under the hold's peak, is re-drawn, up to four
    times, with the grain's own spare variates (deterministic). If all four fail, the grain is dropped.
    The draft rejected with probability `skip`: at "Hold Chord" (27 grains per second, 22 % of the
    draws over the pick) `skip` 70 % would have let 1.8 picks per second through [A], against the
    bench's 1 per 10 s.
  - **Grains fit the field.** In the hold, a grain's source span is at most half the field. A press
    0.3 s after the chord therefore starts with grains of about 130 ms whatever `size` says: shorter
    grains, or the pick. The player's remedy is to press later.
- **`live`** (0 – 100 %, default 100 %): the level of the live layer. At 0 % the pedal is silent until
  `freeze`: the hold-pedal gesture (a chord held under a dry solo, then released), which the draft could
  only play with a cloud running behind it.
- **Release** (`freeze` off): the hold layer keeps spawning grains from HOLD, with its gain falling to
  −60 dB in `decay` (T60, exponential). The live layer restarts at once from RECORD, which has kept
  recording. During the release both layers run, each capped at 48 grains (§2.2).
- **Hold + feedback**: the hold is not written to, so feedback cannot pile up on it (Clouds' rule). The
  live layer's feedback still runs.
- **`scan`** moves the position centre through [0, *L*] at `scan` % of real time, wrapping. In the hold,
  +100 % plays the frozen phrase forward at its own speed (a granular time-stretch at 100 %; 25 % =
  four times slower), and −100 % plays it backward. Live, it sweeps the delay as a sawtooth. The wrap
  only moves where the *next* grains start: each grain is windowed, so a wrap is never a click. A scan
  wrap does mark a period; that is the player's choice, and the default is 0.

### 2.7 Modes (schedulers)

| Mode | Onsets | Positions | `grains` means | Inert controls |
|---|---|---|---|---|
| **Cloud** | Free: rate = `grains` / `size`. `regular` 0 % = Poisson (exponential intervals); 100 % = metronomic; in between, a gamma-distributed interval (shape 1 → 50) [A] | `pos`·*L* ± `spray`, moving with `scan` | **Overlap**: the average number of grains sounding at once | `sync` |
| **Rhythm** | Host grid (`sync`; Off = every `size`). `chance` decides each step | `pos`·*L* ± `spray`, quantised to whole steps back (beat-repeats land on the beat), moving with `scan` | **Grains per step** (a cluster; with `spray` / `pitch_rand` it becomes a chord). Grains of a step that would be identical (same position, ratio and pan) are played as one | `regular` |
| **Strum** | Host grid (`sync`; Off = every `size`/2) | **The recent note onsets** (the marks of gfx-dsp `onset`, §2.6: one per strum) within the last `pos`·*L*, newest first, cycling; each grain starts 5 ms before its onset so it keeps the attack. ± `spray` (small). No onset in range: no grain | **How many recent onsets** are cycled (1 = repeat the last note: Microcosm Strum A) | `regular`, `scan` |
| **Phrase** | Slice clock: *L* / `grains` per slice (or `length_sync` / `grains`) | The phrase = the last *L* (live: re-captured every cycle, a looping delay; held: the frozen phrase). Slice *j* plays slice σ(*j*): identity, or with probability `spray` a random slice (shuffle) | **Slices per phrase** (ZamGrains' own meaning of "grains") | `pos`, `scan`, `regular`, `shape`, `skew`, `size` |

- **Phrase seams**: each slice grain lasts *T* + *X* (*X* = min(*T*/2, 30 ms)). It fades in over *X*
  reading the *X* before its slice start (pre-roll) and fades out over the *X* after its end, so
  consecutive slice grains overlap by *X*. The crossfade law depends on contiguity: **amplitude-
  complementary** when σ(*j* + 1) = σ(*j*) + 1 (same samples → coherent, exact unity, inaudible), and
  **equal-power** otherwise (the loop's wrap, a shuffle, a reversed or transposed slice). The loop seam
  is thus a looper's crossfade from the end into the pre-roll before the start, never a jump. This is
  what ZamGrains' freeze lacked. Per-slice `reverse`, `pitch_rand` / `scale` (Mosaic-like octave
  slices), `glide` (Glide-like), and `chance` (a missed slice = a rest) all apply.
- **Tempo**: from the host (`transport`), phase-locked to the host position. Without a tempo, 120 BPM.
  `length_sync` sets *L* to a note value (shared list) and overrides `length`.
- **Scheduling is per sample, in f64 counters**, independent of the host block size. Everything
  computed "per tick" (window statistics, the level's powers, kernel rows) uses the 32-sample ticks
  counted from the last reset (§2.1). A 32-sample host block and a 4,096-sample one give the same cloud,
  bit for bit, for the same parameter values.
- **One knob, four readings.** `grains` is the density in every mode; the table says of what. It is
  one id on purpose (§7.1).

### 2.8 Stereo

Each grain reads the stereo pair at its position as mid *M* and side *S*. Its output is
L = *M*·*g*L(θ) + *S*, R = *M*·*g*R(θ) − *S*, with an equal-power pan θ of the mid. `spread` 0: every
grain is centred and a stereo source keeps its image. `spread` 100: each grain's mid sits anywhere in
the field. A mono DI on both inputs has *S* = 0, and with `spread` 0 gives L = R bit-exact. The 1 / 1
layout sums *M* only (no pan law loss).

### 2.9 Feedback and tone

- Wet (after normalisation) → `low_cut` (2-pole) → `high_cut` (2-pole) → DC blocker → soft clip
  (tanh knee at −3 dBFS, ceiling +3 dBFS) → × `feedback` → added to the input written to RECORD. The
  cuts also act on the wet output, as in a delay's "repeats darken" behaviour.
- With `pitch` +12 and feedback the octaves climb: **a granular shimmer**. With `pitch_rand` and
  `scale` Fifths they climb in fifths. `feedback` 100 % + the soft clip gives a bounded, self-renewing
  cloud (bench: ≤ +6 dBFS peak for 60 s).
- `low_cut` on the wet is the bass-lane control: the low end stays in the unity dry, clean and centred
  (the role `bass_keep` plays elsewhere; not added, it would duplicate it).

### 2.10 Mix, trails, latency

- **`mix`**: **wet level with the dry at unity** (the delay / reverb law of the contract), linear,
  0 – 100 %. `kill_dry` removes the dry (for parallel routing, or a 100 % wet send): the id and the
  meaning of gfx-delay, gfx-reverb and gfx-shimmer.
- **`bypass`** is the bypass parameter with trails (nih-plug `make_bypass`, as every coded tail
  effect ships). With `trails` on, bypass stops the input to RECORD and to the scheduler's new grains in
  live mode, and the grains in flight finish their windows (≤ `size`). A held cloud releases in `decay`.
  With `trails` off, bypass fades the wet out in 20 ms.
- **Latency: 0 reported.** The dry is untouched. The wet is a delay of ≥ 1.5 ms by construction (it
  reads the past). The half-band and kernel group delays are absorbed into read positions, not into
  the output. The onset marks are written 0.3 – 1 ms after the pick; the grains read them later.
- **No allocation in `process`.** Buffers, the kernel bank and the window tables are built in
  `initialize`. Enabling does no work beyond clearing indices (no buffer zeroing on enable: RECORD's
  stale content is masked by a "valid length" counter that grows from 0).

### 2.11 CPU estimate [A]

Per grain-sample, stereo, 32 taps: 2 × 32 MACs (the kernel) + 32 lerps (the phase interpolation of the
kernel row, shared by both channels) + window, pan and bookkeeping ≈ 110 flops. That is about 20 cycles
with AVX2 FMA on contiguous interleaved taps ≈ 7 ns at 3 GHz. With 48 taps (ratios in the upper half of
each octave: +6 … +12 and +18 … +24 st): ≈ 10 ns.

| Case | Grains sounding | ns / sample (48 kHz, stereo) |
|---|---|---|
| Defaults (8, unity fast path) | ≈ 8 | ≈ 60 (half-band, onset detector and map, filters, scheduler, level) |
| 8, `pitch_rand` 12 | ≈ 8 | ≈ 120 |
| 64, pitched, glide | ≈ 64 (peaks ≈ 89) | ≈ 500 – 900 |
| Two layers, 48 + 48, all at 48 taps | ≈ 96 (peaks ≈ 127) | ≤ 1,300 |

That is 0.3 % of a core at the defaults and ≤ 6 % at the very worst. It sits next to gfx-reverb
(315 ns) and gfx-shimmer (388 ns). To be measured; the targets are in §5.

### 2.12 gfx-dsp reuse and what is new

| Reused | For |
|---|---|
| `lfo::Rng` | Every variate (seeded, reset on `reset`) |
| `transport` (`Sync`, `phase_at`, `note_samples`) | Rhythm / Strum grid, `length_sync` |
| `svf` | `low_cut`, `high_cut` |
| `envelope::PeakFollower` | The map's level byte |
| `onset` (gfx-swell's module, `onset::Pick` + level gate + strum window; written by whichever of gfx-swell, gfx-ducker and this crate is coded first, to the interface of gfx-ducker §2.10) | The onset marks (§2.6): Strum's notes, the hold's field and `skip` |
| `detect::SCALES` | Major / Minor / pentatonic interval sets |
| `smooth`, `exp_smooth` | `mix`, `feedback`, the cuts; EXP on `pos`, `size`, `pitch`, `mix`, `spray` |
| `delay::lagrange…` (tests only) | Reference for the kernel's accuracy at ρ = 1 |

Not reused: `pitch::LoopShifter` / `GrainShifter` (two heads, Lagrange 3, no band-limiting; fine inside
a reverb's loop, where gfx-shimmer uses them, but a cloud is heard bare). Its Bhaskara sine (equal-power
heads) is reused for the pan law.

**New**, in gfx-dsp so that gfx-delay (reverse, loop pitch) and gfx-shimmer can share them:
- `grain`: `MipBuffer` (stereo interleaved, mirrored guard, half-rate copy, valid-length counter),
  `KernelBank` (ratio-tracking Kaiser sinc, 32 / 48 taps), `Window` (shape × skew tables, edge guard),
  `GrainPool`.

**Not new**: `onset`. gfx-swell's design creates it, and gfx-ducker's review ruled that there is one
module of that name. The draft planned a second detector here (a fast / slow envelope ratio); it is
dropped. What this crate adds to the module's interface is nothing: it reads the onsets and their rise.

The four schedulers, the hold, the onset map and the normalisation stay in the crate.

## 3. Parameters

### Ids, units, the 2026-11-14 rule

| Catalogue id | ZamGrains law | Decision |
|---|---|---|
| `freeze` (0 – 1, raw) | A toggle at 0.5 | **Kept**: a `BoolParam` named "Freeze". REAPER reads a bool as ≤ 0.5 off / > 0.5 on; golem writes 0 / 1. Presets unchanged |
| `grains` (1 – 64, raw) | Windows per loop (always two heads) | **Kept** as an `IntParam` 1 – 64 named "Grains". The meaning is ours (§2.7); in Phrase it is ZamGrains' own (slices per loop). Not a 0 – 1 scale, so the rule does not apply |
| `pos` (0 – 1, raw) | **None**: an output port in ZamGrains 4 | **Kept 0 – 1**, named "Grain Position": the fraction of `length` back from now (or from the hold's end). Linear in ms at any `length`, so this is the rule's "keep 0 – 1 when linear in a real unit" branch. Displayed as "% · ms" |

Display names are the join keys (descriptor rules): "Freeze", "Grains", "Grain Position" stay exactly
as in the catalogue.

**No conversion of saved values is needed**. Every catalogue value means something sensible in the new
engine. But **block presets are re-authored by intent** (presets file), because what ZamGrains played
was the dry signal (§1). "Standard", "Shimmer cloud" and "Stutter" keep their ids and names and get new
settings (§4). A saved rig chain with `freeze` 0 played the dry signal; with gfx-grains it plays a cloud
at `mix` 50 %. That is the contract's "new sounds, presets will sound different", and it is kept (§7.7).
golem-rack counts such chains; `mix` 0 % is the faithful fallback if Raph wants one.

### Front panel

| id | Name | Type | Range | Catalogue | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `freeze` | Freeze | b | off / on | 0 – 1 | off | — (§2.6) | | Hold: grains come from the last `length`, which stops changing; release fades in `decay`. The footswitch gesture (§7.4). A bool here, as in the catalogue and in gfx-stretch (gfx-reverb's and gfx-shimmer's `freeze` are enums: their own blocks) |
| `grains` | Grains | i | 1 – 64 | 1 – 64 | 8 | per grain | ✓ | Density: overlap (Cloud), grains per step (Rhythm), onsets cycled (Strum), slices (Phrase) |
| `pos` | Grain Position | f | 0 – 1 | 0 – 1 | 0.5 | per grain | ✓ | Where grains start: fraction of `length` back (Strum: how far back onsets are taken). Pitched-up grains start at least (*r* − 1)·`size` back (§2.2) |
| `size` | Size | f | 2 – 2,000 ms | new | 120 ms | per grain | ✓ | Grain length (Phrase: inert, the slices set it) |
| `pitch` | Pitch | f | −24 – +24 st | new | 0 st | per grain | ✓ | Base transposition of every grain (continuous; display snaps to semitones within ±5 cents) |
| `mix` | Mix | f | 0 – 100 % | new | 50 % | 20 ms | ✓ | **Wet level, dry at unity** (delay / reverb law) |
| `mode` | Mode | e | Cloud · Rhythm · Strum · Phrase | new | Cloud | in-flight grains finish | | The scheduler (§2.7) |

### Expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `length` | Length | f | 50 – 10,000 ms | 1,000 ms | per grain | | The buffer span `pos`, `spray` and the hold refer to; Phrase: the loop |
| `length_sync` | Length Sync | e | shared `sync` list | Off | — | | *L* as a note value (overrides `length`; capped at 10 s) |
| `spray` | Spray | f | 0 – 100 % | 10 % | per grain | ✓ | Position scatter ± % of *L* (and ±25 % size jitter at 100 %). Phrase: shuffle probability |
| `scan` | Scan | f | −200 – +200 % | 0 % | 20 ms | ✓ | Position travel in % of real time; 0 = holds; +100 % in the hold = time-stretch at speed 1 |
| `regular` | Regularity | f | 0 – 100 % | 0 % | per grain | | Cloud onsets: 0 = random (Poisson), 100 = metronomic (synchronous: a buzz at `grains`/`size` Hz, on purpose) |
| `chance` | Chance | f | 0 – 100 % | 100 % | per grain | ✓ | Probability that a scheduled grain plays |
| `shape` | Shape | f | 0 – 100 % | 100 % | per grain | | Window: flat → triangle → Hann (edges always C¹, §2.4) |
| `skew` | Skew | f | −100 – +100 % | 0 % | per grain | | Peak position: −100 = percussive, +100 = swell |
| `reverse` | Reverse | f | 0 – 100 % | 0 % | per grain | ✓ | Probability that a grain plays backwards |
| `pitch_rand` | Pitch Random | f | 0 – 24 st | 0 st | per grain | ✓ | Per-grain random interval range (±) |
| `scale` | Scale | e | Off · Chromatic · Octaves · Fifths · Major Triad · Minor Triad · Major · Minor · Major Pent · Minor Pent | Off | — | | Interval set the random interval is drawn from (§2.2) |
| `detune` | Detune | f | 0 – 50 cents | 0 cents | per grain | | Random ± cents per grain, after the scale (chorus, decorrelation of a hold) |
| `glide` | Glide | f | −24 – +24 st | 0 st | per grain | | Pitch travel over each grain's life |
| `spread` | Spread | f | 0 – 100 % | 50 % | per grain | | Random pan of each grain's mid (§2.8) |
| `sync` | Sync | e | shared list | Off | — | | Rhythm / Strum step (shared id) |
| `seed` | Seed | i | 0 – 999 | 0 | at reset / change | | Picks another reproducible cloud (restarts the variate stream) |
| `skip` | Attack Skip | f | 0 – 100 % | 0 % | per grain | | 0 = off. Above: the hold is what rings after the last attack, and no grain covers an attack or near-silence; the value sets which attacks count (rise 12 → 3 dB) (§2.6) |
| `live` | Live Level | f | 0 – 100 % | 100 % | 20 ms | ✓ | Level of the live layer; 0 % = silent until `freeze` (the hold-pedal gesture) |
| `decay` | Decay | f | 0.05 – 10 s | 1.0 s | — | | Hold release, **T60** (as gfx-reverb's and gfx-shimmer's `decay`; §7.5) |
| `feedback` | Feedback | f | 0 – 100 % | 0 % | 20 ms | ✓ | Wet back into the buffer (soft-clipped) |
| `low_cut` | Low Cut | f | 20 – 1,000 Hz | 20 Hz (off) | 30 ms | | Wet and loop, 2-pole |
| `high_cut` | High Cut | f | 1,000 – 20,000 Hz | 20,000 Hz (off) | 30 ms | ✓ | Wet and loop, 2-pole |
| `kill_dry` | Kill Dry | b | off / on | off | 20 ms | | Removes the dry (parallel lane); shared with gfx-delay, gfx-reverb, gfx-shimmer. (`dry` is a level in dB elsewhere, so the draft's bool `dry` is renamed) |
| `trails` | Trails | b | off / on | on | — | | §2.10 |
| `bypass` | Bypass | b | off / on | off | — | | `make_bypass`, with trails |
| `active` | Grains (now) | f, read-only | 0 – 128 | — | — | | Voices sounding; flags drops at 128 |
| `state` | State | f, read-only | 0 Live · 1 Held · 2 Releasing | — | — | | For golem's footswitch LED |

No default depends on another parameter. Controls inert in a mode are greyed through `depends` in
descriptors (no hidden remapping). All ranges are linear. `size` 2 – 2,000 ms gives a 7-bit EXP sweep
15.7 ms steps, which is fine because grains are drawn per grain: there is no zipper to hear.
`grains` and `seed` are `IntParam`s; enums only grow at the end. The two meters are read-only
`FloatParam`s (`read_only()` of golem's nih-plug fork, as gfx-gate's `state`): the fork has no read-only
integer or enum.

Expert groups (`visuals/data/groups.json`): **Grain** (`spray`, `shape`, `skew`, `reverse`, `chance`,
`regular`), **Pitch** (`pitch_rand`, `scale`, `detune`, `glide`), **Time** (`length`, `length_sync`,
`scan`, `sync`, `seed`), **Hold** (`decay`, `skip`, `live`), **Space & tone** (`spread`, `feedback`,
`low_cut`, `high_cut`, `kill_dry`), **Bypass** (`trails`).

### Visual

- **Category: `modulation`**, the catalogue's own, so golem-rack has no category change to make.
  golem's granular block is a texture effect placed where modulation goes; gfx-glitch is already there.
- **No crate hue**: the body is the modulation green `#00a84f`, and the motif sets the pedal apart, as
  for gfx-vocoder, gfx-flanger and gfx-phaser. The draft's moss green `#3cb43c` (hue 120°) sits in the
  gap by hue only: ΔE*ab **15** from the modulation green, 23 from gfx-glitch's lime `#72c000`, 31 from
  gfx-tuner's `#126e1c` [P, CIELAB D65]. gfx-tuner's and gfx-vocoder's reviews found the same: there is
  no room for a third bright green, and the one dark green is taken. Ice blue (the catalogue's
  `#5ec8e0`) would sit on filter's `#00a8c0`.
- **Finish: anodized** (the modulation family's).
- **Motif: `grains`** (new). Either side of the rune, about 14 small grain windows: Hann arches 3 – 7
  units wide and 2 – 6 high, placed by a fixed seed (derived from the crate name, so the drawing is
  stable). Each has a dot at its peak. They are denser and larger toward the rune and fade at the edges
  (opacity 35 → 90 %), like a cloud converging. Anodized: engraved arches and dots in the rune light.
  The arches stay in the rune zone, clear of the knob grid (the gfx-freqshift review's lesson).
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-grains": {"name": "Grains", "blocks": ["granular"], "knobs": ["grains", "size", "pos", "pitch", "mix"],
                 "selectors": ["mode"], "toggles": ["freeze"], "category": "modulation", "color": "#00a84f"}
  ```
- **Glyph** (proposed, style of `glitch.svg`): three overlapping grain windows of different heights,
  and two loose grains above.

  ```svg
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>Granular (gfx)</title><path d='M2 20C4 20 3.8 12 6 12S8 20 10 20'/><path d='M7 20C9.5 20 9.25 4 12 4S14.5 20 17 20'/><path d='M14 20C16 20 15.8 11 18 11S20 20 22 20'/><circle cx='4.5' cy='5.5' r='.9' fill='currentColor' stroke='none'/><circle cx='19.5' cy='5' r='.9' fill='currentColor' stroke='none'/></svg>
  ```
  golem's existing `granular.svg` (a cloud of dots) reads as particles or snow, next to freeze's ❄.
  The windows say "grains", and the cloud of dots stays free for the `freeze` block's snowflake family.
  **The new glyph is taken** (§7.8).
- No maker's livery, logo or trademark on the pedal.

## 4. Applications

Reference settings become block presets (`presets/gfx-grains.json`, ids permanent). Level-matched to
the bypass within ±2 LU on the bench DIs. Preset names carry no artist or trademark names. Unlisted
parameters are at their defaults.

| Use / preset idea | Settings to start from | Feature that matters |
|---|---|---|
| **Standard** (re-authored: a gentle cloud behind the playing) | Cloud, `grains` 8, `pos` 0.5, `length` 1,000, `size` 120, `spray` 10 %, `mix` 50 % | Band-limited, normalised cloud at ~500 ms |
| **Hold Chord** (freeze a chord, solo over it) | Cloud, `size` 450, `grains` 12, `pos` 0.4, `length` 2,000, `spray` 60 %, `detune` 8 cents, `spread` 70 %, `skip` 70 %, `live` 0 %, `decay` 3 s, `mix` 70 %; `freeze` on the footswitch | Silent until held. Stationary random field: no loop period, no pick attack; the hold is what rings after the last strum |
| **Hold over a Cloud** (the same, with the cloud behind the playing) | Hold Chord with `live` 100 %, `mix` 50 % | Both layers; the cloud stops feeding while held |
| **Granular Delay** (pitched, smeared echoes) | Cloud, `pos` 0.4, `length` 1,000, `size` 80, `grains` 4, `regular` 100 %, `spray` 0, `feedback` 40 %, `high_cut` 6,000, `mix` 35 % | Coherent normalisation: a clean delay at spray 0 |
| **Shimmer Cloud** (re-authored: octave halo) | Cloud, `grains` 16, `size` 220, `pitch_rand` 12, `scale` Octaves, `feedback` 45 %, `spray` 30 %, `high_cut` 8,000, `mix` 40 % | Band-limited +12 grains in a loop: no fizz as it climbs |
| **Fifths Halo** | Cloud, `pitch_rand` 19, `scale` Fifths, `grains` 12, `size` 300, `spray` 40 %, `feedback` 30 %, `mix` 40 % | Interval-set randomness |
| **Chord Cloud** (one note → a triad) | Cloud, `pitch_rand` 12, `scale` Major Triad, `grains` 10, `size` 250, `detune` 6, `spread` 80 % | Triad sets |
| **Reverse Swells** | Cloud, `reverse` 100 %, `skew` +100 %, `size` 500, `grains` 4, `pos` 0.6, `mix` 50 % | Envelope independent of direction |
| **Stutter** (re-authored: beat-repeat glitch) | Rhythm, `sync` 1/16, `grains` 1, `pos` 0.3, `length_sync` 1/1, `spray` 25 %, `chance` 70 %, `shape` 0 %, `mix` 60 % | Host grid, positions on whole steps, flat grains with C¹ edges |
| **Tape-Stop Repeats** | Rhythm, `sync` 1/8, `glide` −12, `size` 250, `grains` 1, `mix` 50 % | Glide per grain |
| **Strum Chains** (your last notes, rhythmically) | Strum, `sync` 1/16, `grains` 4, `pos` 0.6, `length` 2,000, `size` 180, `skew` −60 %, `mix` 45 % | Onset-sourced grains with their attack |
| **Arp Shards** | Strum, `sync` 1/16, `grains` 3, `pitch_rand` 12, `scale` Minor Pent, `size` 140, `spread` 90 % | Onsets + scale-quantised steps |
| **Repeat the Last Note** | Strum, `grains` 1, `sync` 1/8, `size` 300, `feedback` 20 % | Strum with one onset |
| **Phrase Loop** (looping delay, one bar) | Phrase, `length_sync` 1/1, `grains` 8, `mix` 50 %; `freeze` = hold the bar | Seams as looper crossfades, exact unity when contiguous |
| **Octave Mosaic** | Phrase, `length_sync` 1/2, `grains` 8, `pitch_rand` 12, `scale` Octaves, `chance` 85 % | Per-slice octave choice |
| **Phrase Shuffle** | Phrase, `length_sync` 1/1, `grains` 16, `spray` 50 %, `reverse` 25 % | Shuffle with equal-power seams |
| **Glide Loops** | Phrase, `length` 1,500, `grains` 6, `glide` +12, `mix` 50 % | Glide on slices |
| **Particle Buzz** (synth-like formant) | Cloud, `size` 8, `grains` 24, `regular` 100 %, `pos` 0.05, `pitch` +7, `mix` 60 % | Synchronous stream = a pitch (Roads) |
| **Dust** (sparse crackle) | Cloud, `grains` 1, `chance` 30 %, `size` 25, `spray` 100 %, `pos` 1, `spread` 100 %, `skew` −80 %, `mix` 40 % | Probability, random pan |
| **Time-Stretch Freeze** | Cloud, `freeze` on, `scan` 25 %, `size` 200, `grains` 8, `spray` 5 %, `length` 4,000 | Scan through the hold: the phrase at quarter speed |
| **Granular Doubler** (voice, keys) | Cloud, `pos` 0.02, `size` 90, `grains` 6, `detune` 15, `spray` 2 %, `spread` 60 %, `mix` 35 % | Short delay, decorrelated detune |
| **Bass Pad** (bass lane) | Cloud, `low_cut` 180, `size` 400, `grains` 8, `spray` 30 %, `skip` 50 %, `mix` 40 %; `freeze` as a gesture | The lows stay in the clean dry |

## 5. Bench suite (on top of gfx-bench's common checks)

**gfx-bench options**: `stereo_by_nature` true. Silence floor at the default (−140 dBFS): with
`feedback` 0 the wet has nothing left to read once the burst is older than `pos`·*L* + `spray`·*L* +
`size` (≈ 0.72 s at the defaults). Latency check kept (the dry at unity dominates the correlation at
lag 0). Click bound at the default.

| Check | Pass |
|---|---|
| Unity | Cloud, `pitch` 0, `spray` 0, `regular` 100 %, Hann, `grains` 2 / 8 / 64, `kill_dry` on, `mix` 100: output nulls the input delayed by `pos`·*L* to **≤ −120 dB** (the reads are bit-exact; the sum of the Hann windows is 1 to f32 rounding, not to the bit). `regular` 0 %: long-term level within 0.5 dB |
| Kernel | Each of the 17 kernels, through the 129-row table: pass ±0.01 dB to its pass edge, stop ≥ 71 dB from its stop edge out to 64 × the level's rate (prototype: 71.7 – 99 dB), DC gain 1 ± 1e−6 per phase |
| Aliasing | Sine sweep 20 Hz – 18 kHz at `pitch` −24, −12, +1, +5.9, +6.1, +7, +11.9, +12, +13, +17.9, +18.1, +19, +24 and `glide` ±12: every component not at *r*·*f* is ≤ −70 dB re the source tone below 0.4 *f*s (prototype: worst −72.8 dB at +17.9); the wanted tone is within 1.5 dB up to 18 kHz at the output |
| Glide | `glide` +12 and +24 on pink noise: the grain's spectrum is flat to 18 kHz at its start and at its end; no burst at a row or level change (click detector) |
| Half-rate copy | Level 1 against an offline ideal decimation: ≤ −80 dB below 0.2 *f*s; read positions compensate the group delay (a grain at *r* = 1.5 on level 0 and one at *r* = 3 on level 1 null an offline sinc resampler ≤ −65 dB below 0.3 *f*s) |
| Clicks | `shape` 0 / 50 / 100, `skew` ±100, `size` 2 – 2,000 ms, random knob moves every 50 ms on a sine and a DI: no broadband burst above −60 dB re the signal (common click detector) |
| Freeze, no loop | DI chord, freeze, 30 s, "Hold Chord" settings: the autocorrelation of the output's 10 ms RMS envelope has no peak > 0.3 between 50 ms and 10 s (a naive 160 ms loop: ≈ 1.0); spectral centroid within ±10 %, level within ±1.5 dB over 30 s |
| Freeze capture | Chord, freeze pressed 300 ms and 1 s after the attack, `skip` 70 %: the onset detector finds **no** attack in 30 s of held output. The same after 1 s of silence, and after a different chord (a fifth away): nothing of the silence above −50 dB, the earlier chord's partials ≤ −40 dB in the hold. A six-string strum over 60 ms counts as one attack. `skip` 0: the whole `length` is used |
| Edges | Hold with `pos` 0 and 1, `spray` 100 %: the histogram of grain start positions has no bin above twice the mean (reflection, not clamping) |
| Copy-on-freeze | Freeze with `length` 10 s, keep playing 30 s, release with `decay` 10 s: the hold layer nulls against a hold rendered from an offline copy (≤ −120 dB); no block costs more than 1.5 × the mean while the copy runs |
| Hold again | Freeze, release with `decay` 10 s, freeze a second chord 1 s later, at `length` 1 and 10 s: the first hold keeps fading with no click and no content of the second chord; the second holds; a third engage ends the first in 50 ms |
| Live level | `live` 0 %: output = dry (bit-exact) until `freeze`; the hold then comes in with no click |
| Release | `decay` 1 / 3 s: hold layer −60 dB at T60 ± 10 %; the live layer resumes within one `size` |
| Level | `grains` 1 → 64, `spray` 0 / 0.05 / 0.2 / 1 / 50 %, `pitch_rand` 0 / 12, `detune` 0 / 10, `regular` 0 / 100, `shape` 0 / 100, on pink noise, a 110 Hz and a 440 Hz note and a DI: wet loudness within ±2 LU of the source (prototype: ±0.6 dB; the draft's model: up to −13 dB); a `spray` sweep 0 → 5 % shows no dip; every factory preset within ±2 LU of the bypass |
| Determinism | Two renders null exactly; host blocks of 32, 64, 512, 4,096 and a random-size schedule null exactly against each other; REAPER vs Golem Engine ≤ −50 dB (contract) |
| Seed / variate order | `seed` 1 vs 0: correlation < 0.5; `spray` 10 → 30 %: grain onset times identical (same variates) |
| Tempo | Rhythm 1/8 at 120 and 97 BPM: onsets within ±1 sample of the host grid after a relocation; no tempo → 120 BPM |
| Strum | Synthetic plucked notes at known times: grains start within 6 ms before each onset; with `grains` 1 only the last onset is used |
| Phrase seams | Phrase, contiguous slices, `pitch` 0: output nulls the phrase loop ≤ −90 dB except across the wrap; the wrap crossfade has no level dip > 1 dB on noise (equal-power) and no step |
| Feedback | `feedback` 100 %, `pitch` +12, pink noise at −12 dBFS for 60 s: peak ≤ +6 dBFS, no DC > −60 dBFS, no NaN; `feedback` 90 %, `pitch` 0: decays |
| Stereo | Mono DI on both inputs, `spread` 0: L = R bit-exact; `spread` 100: inter-channel correlation of the wet < 0.3; 1 / 1 layout runs and sums mid only |
| Rate independence | 44.1 / 48 / 96 / 192 kHz: grain lengths and onsets equal in ms (± 1 sample), alias bound held |
| Pool | `grains` 64 through a release with 64: each layer capped at 48, fewer than 0.1 % of grains dropped, no click, level of each layer within 1 dB of its uncapped level; `active` reports |
| Trails | `bypass` with `trails` on: the cloud finishes within `size`; off: wet gone in 20 ms, no click |
| CPU | ≤ 120 ns / sample at defaults, ≤ 900 ns at `grains` 64 + `pitch_rand` 12 + `glide` + `feedback`, ≤ 1,300 ns worst (two layers, `pitch` +11), 48 kHz stereo |
| Listening | clean.wav, lead.wav, keys and voice renders through every §4 preset; Hold Chord against GoleM Freeze on the same chord (texture vs sustain, not a winner); blind against a reference granulator owned by Raph (§7.9) |

## 6. Positioning

| Pedal | Its sustain / grains | Difference |
|---|---|---|
| golem `freeze` (GoleM Freeze, spectral; quarantined) | Smooth, grainless sustain of the **notes**; 21 – 85 ms reported latency | gfx-grains: zero latency, a **textured** hold you can move, pitch, scatter, stretch. **gfx-grains does not stand for `freeze`**, and neither does gfx-stretch (its review, the same day): the block is reworked as a thin crate on gfx-dsp `stft`, Raph's call. Until then "Hold Chord" is golem's only working hold. Shared ids `freeze`, `mix` mean the same gesture; `decay` differs in unit (§7.5) |
| gfx-glitch (spectral Sustain, Hold effects) | Spectral max-hold, set up beforehand, no footswitch | gfx-grains is time-domain, footswitch-held, rhythmic, onset-aware |
| gfx-delay (reverse, loop pitch) | A pitched or reversed **repeat** in a delay loop | gfx-grains is many grains, random or rhythmic; its `grain` module can give gfx-delay a band-limited reverse later |
| gfx-shimmer / gfx-reverb freeze | Shifters inside a reverb tank; a frozen tank | gfx-grains is dry granular: no tank, positions you choose |
| gfx-stretch (designed and reviewed the same day) | Spectral, holds 60 s, stretches to ×1,024 | gfx-grains is time-domain, holds 10 s, and `scan` stretches a few ×. No shared buffer code (its takes are f16, no half-rate copy). Shared: the `onset` module, `state` as a read-only float |

Against the field: Clouds' and Beads' control sets are covered (position, size, pitch, density
regular ↔ random, texture, spread, feedback, freeze, probability, ±24 st, reverse). Granulator III's
three families are covered, Particle's interval sets too, and Microcosm's onset-based Strum / Arp and
phrase slicing (Mosaic, Seq, Glide) are covered as modes and presets. Beyond them: band-limited pitched
grains with a stated bound, a freeze that is provably not periodic, attack and silence avoidance,
level by cloud statistics, block-size-independent determinism. Not attempted: Microcosm's per-mode
filters and bit-crushing (the rack's gfx-crusher and filters), a built-in reverb (Clouds, Beads,
Microcosm: the rack has gfx-reverb after it), a 60 s phrase looper (golem's looper), MIDI-note grain
pitch.

## 7. Open questions: decisions of the review

1. **`grains` semantics per mode** → **one id, kept.** The contract's rule is about defaults (none
   depends on `mode` here), and a second id would leave one of the two dead in every mode. All four
   readings are a density, 8 is sensible in all of them, Phrase's is ZamGrains' own, and a preset
   carries its `mode`. The cost is in the UI: golem should caption the knob by mode (overlap / per step
   / notes / slices) from the descriptor. golem-rack's.
2. **Kernel** → **neither option as drafted**: an octave switch on one half-rate level, 32 taps up to
   ρ = √2 and 48 above (§2.3). ≥ 72 dB measured, flat to 18.4 kHz at every ratio, +50 % cost on half
   the pitched range only. The √2 switch measured −12 dB spurs (its levels read below their own rate).
   The 129-row table holds: −98 dB.
3. **Coherence** → **measured**, as a slow ratio of two powers, clamped between the model's two bounds
   and pinned to 1 on the unity path (§2.5). The model alone is 13 dB off in the middle range; the
   measurement is within 0.6 dB and costs one accumulation per grain and tick.
4. **The footswitch** → **`freeze`**, with `bypass` (trails) for the block's on / off, as GoleM Freeze:
   golem adds this plugin to `switch_param_of`. The catalogue's own name for the block is "Granular
   Freeze". `state` feeds the LED. Latching or momentary is golem's foot mode. gfx-glitch's "no
   footswitch" was about a pedal with nothing to hold. golem-rack's to implement.
5. **`decay`** → **T60 here**, as in gfx-reverb and gfx-shimmer, the two coded pedals with a `decay` in
   seconds. GoleM Freeze's value is a time constant (T60 = 6.91 × it: its 0.6 s default fades in
   4.1 s) [V]. The reworked `freeze` block should move to T60 and convert its saved values once
   (× 6.91, range 0.35 – 69 s). Raph's call, with that rework.
6. **Memory** → **f32, *L*max 10 s, accepted**: 17 MB at 48 kHz now that the quarter-rate level is gone
   (69 MB at 192 kHz). 16-bit storage would requantise the feedback loop on every pass. To check with
   golem-rack: 17 MB written in `initialize` against the engine's stall check (gfx-stretch's review
   raises the same point for 51 MB).
7. **Defaults** → **kept** (`mix` 50 %, `spray` 10 %, `spread` 50 %). A block that does nothing when
   inserted is ZamGrains' fault, not a behaviour to preserve, and the contract accepts new sounds.
   golem-rack counts the saved chains with `granular`; converting them to `mix` 0 is Raph's call.
8. **Glyph and hue** → the **new glyph**; **no crate hue** (§3, Visual): moss green is ΔE 15 from the
   modulation green. golem's catalogue entry (❄, `#5ec8e0`) changes with the swap.
9. **Reference for blind listening**: Raph's (open).
10. **Absolute key** → **not in v1, no id reserved.** The sets are relative to the played note, as in
    the references. Only Octaves and Fifths are safe on any note of a key; the triads and scales move
    in parallel (a major third on every note) and are a colour, not a harmoniser. A key-aware version
    needs a pitch per grain position (YIN on the buffer) and gfx-pitch's `key` / `scale` ids: a phase 2
    to decide after listening.
11. **Catalogue**: plugin swap (mono → 2 / 2 first, the lane turns stereo), `freeze` as bool, `pos`
    becomes effective, presets re-authored, category unchanged, glyph and colour. All golem-rack's.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good**, Strong once the bench reproduces the prototype's figures. The survey is right and
the idea ("no artifacts of its own") is the right one: band-limited grains, a hold that is a field, a
level that does not jump, sample-exact determinism. The reading of ZamGrains was checked line by line
against `ZamGrainsPlugin.cpp` and the installed `.ttl`, and all of it holds: transparent at
gs = ps = 1 (both heads read the sample just written, sin² + cos²), `grains` = the window length only,
"Grain Position" an output port, freeze = the loop of `time` ms with an unwindowed seam, and the three
presets (8 / 0.5, 32 / 0.5, 4 / 0.3) all playing the dry signal. GoleM Freeze's `decay` is a time
constant (0.6 s → 4.1 s T60), as stated. But three of the five promises did not hold as drafted: the
alias bound, the level, and the hold without its pick. All three are fixed in the text; the first two
are measured on a numpy prototype ([P]; scripts in the review's scratchpad, `proto-gfx-grains/`).

### Changes made, and why

| # | Change | Why |
|---|---|---|
| 1 | **Octave switch on one half-rate level; 32 taps to ρ = √2, 48 above; quarter-rate level removed** (§2.1, §2.3) | With the √2 switch, levels 1 and 2 were read at ρ down to 0.71. A decimated level is dirty between 0.4 and 0.5 of its rate, and that band then lands in the audio band: a 12 kHz tone at +7 st, or a 5.9 kHz tone at +19 st, gave a spur at 17 – 18 kHz **12 dB** under the tone [P]. Now ≥ 72.8 dB at every tested ratio, and flat to 18.4 kHz instead of 13.6 |
| 2 | Kernel figures measured; 17 kernels; the 129-row question answered | 71.7 dB at √2 (32 taps), 75.2 dB at 2 (48 taps); the table's own images at −98 dB |
| 3 | **Glide follows ρ(t)** with a one-tick crossfade at row and level changes | A kernel fixed from the largest ratio left a +12 st glide starting at 9.6 kHz of band, a +24 st one at 4.8 kHz |
| 4 | Minimum read distance 2.5 ms on level 1; ticks counted from reset | 32 + 48 + 25 samples do not fit in 72. Block-size independence needs the tick grid off the host's blocks |
| 5 | **Level formula corrected** (the *V* term) | *N*(*N* − 1)·E[*w*]² is neither Poisson nor regular: −1.0 dB on the "exact unity" case at 2 grains, +2.3 dB on one Poisson grain [P] |
| 6 | **Coherence measured, inside the model's bounds; pinned on the unity path** (§2.5) | exp(−σ / 2 ms) is up to 13 dB wrong between "no scatter" and "30 ms of scatter", on noise, notes and a DI alike. Bounded measurement: ≤ 0.6 dB [P], texture kept |
| 7 | **Edge guard not applied to the Hann; `skew` bounded in time; windows evaluated at their exact phase** (§2.4) | A guard on the Hann breaks its overlap-add, so the Unity check could not pass. `skew` ±100 % on a short grain was a 0.2 ms edge. Onsets rounded to samples ripple a regular stream |
| 8 | Unity check: null ≤ −120 dB, not bit-exact | The reads are exact; a sum of f32 windows is not |
| 9 | **Positions reflected, not clamped** | Clamping piles grains on the hold's edges: repeated identical reads, the loop the design promises to avoid |
| 10 | **`skip` is a rule, not a probability; the hold's field is what rings after the last attack; grains fit the field** (§2.6) | At `skip` 70 %, 1.8 picks per second leaked through, against a bench of 1 per 10 s. A 450 ms grain cannot avoid a pick 300 ms back. With *L* = 2 s, most grains came from before the chord |
| 11 | **The onset detector is gfx-dsp `onset`** (gfx-swell's; `onset::Pick`, level gate, strum window) | The draft created a second detector under the same module name, which gfx-ducker's review had already ruled out. The strum window also makes a strum one onset for Strum mode |
| 12 | **Copy-on-freeze at 8 ×; a second engage during a release specified** (§2.6) | Holding a new chord while the last fades overwrote HOLD under the grains still reading it. The copy now waits, bounded by the writer |
| 13 | **Layer cap 48 + 48** (§2.2) | Two layers at 64 average 128 voices: half the grains dropped, with the level |
| 14 | **`live`** added (reviewer's addition) | "Freeze a chord, solo over it" had a cloud running before the press. At 0 % the pedal is a hold pedal |
| 15 | `dry` (bool) → **`kill_dry`**; `active` and `state` as read-only floats; `freeze` bool noted against the enums of gfx-reverb / gfx-shimmer | Shared ids: `dry` is a level in dB in five designs, `kill_dry` the parallel-lane bool in three. The fork has only `FloatParam::read_only()` (gfx-stretch's review asked for the same alignment) |
| 16 | **No crate hue** | `#3cb43c` is ΔE 15 from the modulation green and 23 from gfx-glitch: the conclusion of gfx-tuner's and gfx-vocoder's reviews |
| 17 | Facts: GoleM Freeze is quarantined and footswitch-driven; ZamGrains' own `grains` default is 1; memory 17 / 35 / 69 MB; CPU table with 48 taps; tables 347 KB | Accuracy |
| 18 | Bench: Kernel, Aliasing, Glide, Half-rate copy, Freeze capture, Edges, Hold again, Live level, Level, Pool, CPU rewritten or added | Each new rule has its check; the old thresholds were unreachable or too loose |

Checked and left as written: grains drawn once at their start with a fixed variate order; the interval
sets and their uniform choice; drop-not-steal; the half-band (80 dB by Kaiser's estimate); the unity
fast path; the envelope independent of direction; Phrase seams (amplitude-complementary when
contiguous, equal-power otherwise); the M / S pan; feedback through the cuts and a soft clip, never
into the hold; `mix` as wet level under a unity dry; `bypass` with trails; zero reported latency;
linear ranges; no mode-dependent default; `pos` kept 0 – 1 (linear in ms); 2 / 2 first; the modes and
their inert controls; the new glyph and the `grains` motif.

Coverage: Clouds, Beads, Granulator III, Particle and Microcosm's Strum / Arp / Mosaic / Glide are
covered. Nothing is superfluous; the widest surface is Phrase mode, which is also the catalogue
block's original meaning. With `live` the hold pedals (the EHX gesture) are covered too.

### The `freeze` block: one answer with gfx-stretch's review

**Neither pedal stands for golem's `freeze` block.** gfx-stretch shares one id of six and has another
gesture. gfx-grains shares three (`freeze`, `mix`, `decay`) and, with `live` 0 %, plays the gesture
at zero latency, but as a textured hold, without `smear`, `tone` or `window`: another sound under the
same name. The block stays its own, quarantined, and its rework is a thin crate on gfx-dsp `stft`
(gfx-stretch's review), with `decay` moved to T60. Meanwhile "Hold Chord" on the `granular` block is
the working hold.

### What stays open

| Point | Owner |
|---|---|
| Catalogue: plugin swap, lane turns stereo, `freeze` bool, presets re-authored, glyph / colour; footswitch → `freeze` (`switch_param_of`); per-mode caption of `grains`; count of saved chains and whether they get `mix` 0; 17 MB in `initialize` against the stall check | golem-rack |
| `live`: keep (reviewer's addition) or drop; reference granulator for the blind test; the `freeze` rework and its `decay` conversion | Raph |
| Who writes gfx-dsp `onset` first (gfx-swell, gfx-ducker or this crate) | golem-fx |
| Reproduce [P] in gfx-bench: kernel stop bands, the alias table, the level table. The half-band's 80 dB is still a Kaiser estimate | Bench |
| By ear: the measured level on note changes at 64 grains and small `spray` (the gain may move up to 16 dB in 400 ms there; fallback: the model alone); the first second of a hold pressed early (short grains), and whether the hold should keep capturing for a moment after the press; `skip`'s 40 ms and 100 ms; Hold Chord against a spectral hold | Raph, on `gfx-bench apps gfx-grains` |
| Flux and pitch-jump onsets for Strum on legato playing; key-aware pitch sets | Later, after listening |
