# gfx-grit

> **Status: DESIGNED** (2026-11-14) — design only, no code. Reviewed 2026-11-14 (Fable 5.2): see the
> Review section at the end; its changes are applied below.

golem's **multiband distortion**. It stands for golem's `grit` block ("Grit (multiband distortion)",
category `lofi`, today UnplugRed's Prisma) and keeps its 25 parameter ids and names
([golem contract](../docs/golem-contract.md)). Four bands, two dirt modules per band, crossovers, a
per-band gain, a wet control and a quality switch, as golem's presets already know them.

The idea: **the Trash / Saturn workflow as a live pedal**. The bands are split by phase-coherent
crossovers, so a band with nothing in it is untouched. Each band has its own dirt, antialiased (2× or 4×
oversampling with ADAA). The saturating modules are level-matched at a reference level, so the Amount
knob changes the tone far more than the volume. Two macro knobs (Drive, Focus) push the whole thing, so the pedal can be
played from four knobs while the 25-parameter engine stays underneath for presets. Mono 1 / 1 first,
reported latency 0.

It is not a single-band drive (gfx-drive), not an exact sampler converter (gfx-crusher), not a spectral
effect (gfx-glitch). §6 explains where it sits among them.

## 1. State of the art

Survey of 2026-11-14. **[V]** read in source code, a manual or a primary page · **[I]** inference, or a
fact not re-checked against a primary source · **[U]** could not be verified · **[A]** our calculation.

### 1.1 What golem plays today: Prisma

[UnplugRed `vsts`](https://github.com/unplugred/vsts), `plugins/prisma/source/processor.cpp` at the
commit golem pins (`5062b59`, AGPL-3.0-or-later), read line by line [V]:

- **Bands**: 4 (`BAND_COUNT`), split by JUCE `LinkwitzRileyFilter` sections in a tree: band 0 =
  LP(x1) → AP(x2) → AP(x3); band 1 = HP(x1) → LP(x2) → AP(x3); band 2 = HP(x1) → HP(x2) → LP(x3); band 3 =
  HP(x1) → HP(x2) → HP(x3) (each HP output copied down the tree). The all-pass stages make the sum of clean bands an all-pass:
  flat magnitude, not a null. JUCE's Linkwitz-Riley filter is 4th order, 24 dB / oct [I].
- **Crossover law**: `calcfilter(v) = mapToLog10(v, 20, 20000)` → **f = 20 · 1000^v Hz**: linear in
  octaves. 25 % = 112 Hz, 50 % = 632 Hz, 75 % = 3,557 Hz [A]. `calccross` keeps each crossover ≥ 0.02
  (0.2 octave) above the previous one and inside its own window: x1 in 2 – 94 %, x2 in 4 – 96 %, x3 in
  6 – 98 % (23 Hz – 17.4 kHz).
- **Modules**: up to 8 per band (default 4; golem's catalogue exposes 2), 23 types (`toid`, the
  catalogue's option order). Each has one value v ∈ [0, 1] with its own law (table below). A module with
  v = 0 is skipped by most types: the sound jumps when v leaves 0.
- **Band gain**: the wet is multiplied by **(2g)²**: 50 % = 0 dB, 100 % = +12.04 dB, 0 = −∞. The gain
  applies **to the wet only**: out_b = dry_b · (1 − wet) + wet_b · wet · (2g)².
- **Wet**: a linear dry / wet crossfade **per band**, the dry taken after the crossover (so no comb).
- **Oversampling** (`quality`): JUCE polyphase IIR 2×, on the whole chain (crossovers included); Off =
  no oversampling at all, no antialiasing. golem's rig measured pdc = 0 samples either way (catalogue
  `_validation`).
- **DC filter**: a 5 Hz one-pole high-pass on bands holding Asym, Rectify, DC or the stereo DC types,
  switched in and out (state reset) when the module list changes.
- **Smoothing**: 1 ms linear ramps on values; a 64-sample cosine declick on the band when modules change.
- Also present, not exposed by golem: mute / solo / bypass per band, A/B states, program slots, module
  count.

Prisma's module laws (v = the catalogue value / 100):

| # | Module | Law in Prisma [V] | Linear in a real unit? |
|---|---|---|---|
| 1 | Soft clip | val = 1 − (1 − ((1 − v)¹⁰ + 1 − v^0.2) / 2) · 0.99; y = (1 − (1 − min(\|x\|, 1))^(1/val)) · sgn x · (1 − 0.92 (1 − val)). Input clamped at 0 dBFS | No: drive = −20 log₁₀ val dB, 2.4 dB at v = 0.01, 40 dB at 1 |
| 2 | Hard clip | same val; y = clamp(x, ±val) · (0.08 / val + 0.92) | No (same) |
| 3 | Heavy | e = 1 − 0.99 v; y = \|x\|^e · sgn x · (1 − 0.92 (1 − e²)) | Linear in the exponent (the curve's own parameter) |
| 4 | Asym | e = 1 − 0.99 v; x ≥ 0: x^e; x < 0: (x + 1)^e − 1; × (1 − 0.86 (1 − e^1.5)); DC-filtered | Linear in the exponent |
| 5 | Rectify | y = (x (1 − v) + \|x\| v) (1 + v) | Linear crossfade to full-wave |
| 6 | Fold | g = 70 v² + 1; triangle fold of g·x (period 4) | No: v² |
| 7 | Sine fold | g = 35 v² + 1; y = sin(π g x) · min(5 v², 1) + x · max(1 − 5 v², 0) | No: v², crossfade |
| 8 | Zero cross | y₀ = (x − clamp(x, ±v¹⁰)) / (1 − v¹⁰), then y = y₀ · (1 − (1 − \|y₀\|)^12.5)^(3 / (1 − 0.98 v) − 3); v = 1 → silence | No. The v¹⁰ clip is inaudible below v ≈ 0.6; **the power term is the audible dead zone**. Its −6 dB point [P]: −40 dBFS at 10 %, −27 at 23 %, −19 at 50 %, −11 at 77.5 %, −5 at 90 % |
| 9 | Bit crush | step = 0.99 v⁵ + 0.01; y = round(x / step) · step | No: bits = log₂(2 / step), 7.6 bits at v = 0⁺, 1 bit at 1 |
| 10 | Sample divide | hold rate R = 40 · 1000^(1 − v) Hz (fractional counter, rate in Hz independent of oversampling; the sample at each new hold is a linear blend of old and new) | **Yes**: linear in octaves |
| 11 | DC | offset = 0.5 · (2v − 1)⁵; neutral at 50 % | No: fifth power |
| 12 | Width | side × 2v; neutral at 50 % (stereo only) | Yes |
| 13 | Gain | × (v / 0.35)²; neutral at 35 % | No: 40 log₁₀(v / 0.35) dB |
| 14 | Pan | per-channel law, neutral at 50 % (stereo only) | Yes (position) |
| 15 | Lowpass | TPT SVF, fc = 20 · 1000^(1 − v), resonance 1.2 | **Yes** (octaves) |
| 16 | Highpass | fc = 20 · 1000^v, resonance 1.1 | **Yes** |
| 17 | Peak | bell at 20 · 1000^v Hz, Q 1.5, gain = 30 · y dB, **y set only by dragging in Prisma's GUI** (0 – 1, default 0) | Frequency yes; **gain unreachable from golem: always 0 dB** |
| 18 | Dry/wet | y = wet (1 − v) + band dry · v (100 % = dry) | Yes |
| 19 | Stereo rectify | stereo only | Yes |
| 20 | Stereo DC | offset v⁵ · (c − 0.5) per channel | No |
| 21 | Ring mod | × cos(2π f t), f = 20 · 1000^v Hz | **Yes** (octaves) |
| 22 | Cabinet | JUCE convolution with one of 6 bundled IRs, index round(5v) (two 4×12 high-gain cabs in two mic takes, a "click" variant, a bass cab) | Stepped |

Findings for golem [A]:

1. **Peak does nothing in golem.** Its gain is a GUI-only value that no parameter sets; the Megaphone
   preset's "Peak 60" is inert today.
2. **Quality Off has no antialiasing at all**, and Quality On is 2× without ADAA: Fold and Bit crush alias
   heavily in both.
3. **Several laws are steep or discontinuous**: Bit crush jumps from bypass to 7.6 bits as v leaves 0;
   Zero cross is two laws multiplied, and silences a −18 dBFS signal from v ≈ 0.75; Soft clip spends its
   first 10 % on 0 → 9 dB.
4. **Heavy amplifies silence**: |x|^0.01 turns −90 dBFS of hiss into almost full scale.
5. Level: every module carries an ad-hoc make-up (the 0.92 factors), so loudness moves with Amount.
6. Prisma's own "Tight kick" preset, read as (value, type) pairs [V]: Width 0 → Gain 0.615 (+9.8 dB) →
   Zero cross 0.775 → Gain 0.23 (−7.3 dB), on the low band. It needs four modules; §4 gives the two-slot
   equivalent.

### 1.2 Commercial references

| Reference | What it does best | What we take |
|---|---|---|
| [FabFilter Saturn 2](https://www.fabfilter.com/help/saturn) [V] | Up to 6 bands, free crossovers, slopes 6 – 48 dB / oct, 28 styles; per-band drive, dynamics, feedback, tone; an optional **Linear Phase** mode (crossovers and oversampling), with latency; deep modulation | Per-band style + drive + level + mix; slope choice; Linear Phase considered and declined live (§2.2) |
| [Ableton Roar](https://www.ableton.com/en/packs/roar/) ([blog](https://www.ableton.com/en/blog/roar-meet-live-12s-new-processing-powerhouse/)) [V] | Six routings (Single, Serial, Parallel, Multi Band = 3 bands / 2 crossovers, Mid Side, Feedback); 12 shaper types; each stage only **Amount, Bias, Filter**: a small face on a deep engine | A small per-module surface; a global amount above the bands |
| iZotope Trash 2 / Trash (2024) ([MusicTech](https://musictech.com/reviews/trash-2-review/), [SOS](https://www.soundonsound.com/node/4931951)) [V] | Up to 4 bands (Trash 2), **two distortions in series per band**, 60+ algorithms in 7 families, filters and convolution around them | Two modules per band is the right size (Prisma's catalogue already has it) |
| Kilohearts Multipass ([blog](https://kilohearts.com/blog/five_things_you_might_not_know_about_multipass)) [V] | Up to 5 bands hosting any snapin; minimum-phase crossovers: "unavoidable (without adding latency) to mess a bit with the phase", warning that chained multiband effects add phase distortion | Honest about the all-pass sum: we state it and measure it |
| [Neural DSP Parallax](https://neuraldsp.com/plugins/parallax) [V, vendor] | Bass: **low band compressed and clean**, mid and high bands each with their own distortion, cab only on mid and high | The bass application (§4), low band left clean |
| [Darkglass B7K](https://www.darkglass.com/products/b7k2) [V, vendor] | Bass: distorted path in parallel with a clean one (Blend), **Grunt** sets how much bass reaches the clipper, **Attack** how much treble | Same goal, done here with crossovers instead of tone-shaped parallel paths |
| Craig Anderton, [*The Guitarist's Guide to Multiband Distortion*](https://www.harmonycentral.com/articles/the-guitarists-guide-to-multiband-distortion) [V] | Quadrafuzz (mid-1980s) as the first multiband distortion; the point is **less intermodulation**: chords stay readable; mda Bandisto (3 bands), Predatohm (4) | The reason the pedal exists for guitar chords |
| Line 6 Helix, Fractal Axe-Fx [I] | No dedicated multiband drive; players build one with a crossover split and two drive blocks (Anderton's multiband Helix presets, [Harmony Central](https://www.harmonycentral.com/news/guitar-accessories/craig-anderton%25E2%2580%2599s-amazing-multiband-helix-presets-r35778/)) | Confirms the use; a one-block answer is simpler for golem |
| Source Audio Ultrawave [U] | A multiband distortion stompbox | Not studied further |

### 1.3 Open source and papers

| Source | Idea | Licence / status |
|---|---|---|
| Prisma (above) [V] | LR4 tree with all-pass compensation; per-band dry / wet; modules at the oversampled rate | AGPL-3.0: ideas only; golem-fx is GPL-3, we write our own code |
| mda Bandisto [I] | 3 bands, one distortion each, the classic free multiband distortion | Open source (mda) |
| gfx-drive (ours) [V] | tanh → hard-clip family with closed-form antiderivatives, ADAA1 inside min-phase 2× / 4×; Fuzz = two stages; LR8 bass split | Reused (§2.7) |
| gfx-crusher (ours, designed) [V] | Fractional clock, continuous bits, Tread quantiser | Laws reused for Bit crush and Sample divide (§2.4) |
| Bilbao, Esqueda, Parker & Välimäki, IEEE SPL 2017; Parker, Zavalishin & Le Bivic, DAFx 2016 [V] | ADAA1 for memoryless shapers | Every module is memoryless or has a memoryless core |
| J. Chowdhury, [ADAA notebook](https://ccrma.stanford.edu/~jatin/Notebooks/adaa.html) [V] | ADAA1 alone is not enough; ADAA1 + 2× works well | Quality Off = 2× + ADAA1 |
| US patent 7206419, *Guitar preamplifier system with controllable distortion* [I, read in a search abstract] | Multiband distortion sounds better with **equiphase** (Linkwitz-Riley) splitting; less intermodulation on two-note intervals | LR splitting, never Butterworth |
| KVR / DSP threads ([KVR](https://www.kvraudio.com/forum/viewtopic.php?p=3985874), [Beis](https://beis.de/Elektronik/Linear_X-Over/LinearAmplAndPhaseXOver.html)) [V] | A multiband LR sum is all-pass only with compensation all-passes in each branch; flat magnitude, but a square wave comes out visibly dispersed | Compensation built in; dispersion stated and benched |

### 1.4 Forum feedback: what players love and hate

- **Love**: lows that stay tight under heavy dirt (bass, drop tunings, drums), chords that keep their
  notes, "dirt only where it matters" (the catalogue's own note: dirty the mids, keep lows clean, no fizz
  on top) [V, Anderton, Parallax, catalogue].
- **Hate**: crossovers that do not null even with every band clean ([Cycling '74](https://cycling74.com/forums/3-band-linkwitz-riley-crossover-filter-design),
  FabFilter cited there) [V]; "holes" at the crossover points with some minimum-phase designs [V, KVR];
  level jumps when the drive moves; deep UIs that take a session to dial (Trash, Saturn: [I], recurring in
  reviews); fizz and aliasing on top when every band is driven [I].

### 1.5 What we conclude

1. **Linkwitz-Riley splitting with all-pass compensation**: the clean sum is flat in magnitude and the
   band phases match at each crossover. No linear-phase mode in a live pedal (§2.2).
2. **One oversampling domain for the whole split**, as Prisma does, but with ADAA1 on every nonlinear
   module and 2× even with Quality off. All bands then share one group delay: coherent by construction.
   "Antialiased" means 15 – 20 dB less aliasing than the same shaper without ADAA, not alias-free: §2.1
   gives the figures.
3. **Laws that mean something**: drive in dB, depth in bits, rates and cut-offs in octaves, each linear
   over the knob. Prisma's octave laws are kept; its steep or broken ones are converted once (§3.2).
4. **Level-matched saturators** (static compensation at a reference level), so Amount is mostly a tone
   control.
5. **A four-knob face on top** (Drive, Focus, Wet, Level), so the pedal is played, not programmed.

## 2. Design

```
          ┌──────────────── up 2× / 4× (min-phase half-bands) ─────────────────────────────────────┐
in ─▶ ×1 ─┤ LR tree (x1, x2, x3; LR4 or LR8, all-pass compensated)                                   │
          │   band b ─┬─▶ slot 1 ─▶ slot 2 ─▶ DC block ─▶ (wet_b)                                    │
          │           └──────────────────────────────▶ (dry_b, aligned) ─▶ mix m_b ─▶ × gain_b ─▶ Σ ─┤
          └──────── solo / bypass ramps ──── droop correction ──── down 2× / 4× ───────────────────┘─▶ × level ─▶ out
   m_b = wet · <b>_mix ; slot amount = <b>_v + k_type · (drive + focus · w_b / 2) / 100 (clamped 0 – 100, §2.5)
```

### 2.1 Oversampling domain

- The input is upsampled once (gfx-dsp `oversample`, HIIR-style polyphase half-bands, minimum phase),
  the crossovers and every module run at the oversampled rate, the bands are summed, and the sum is
  downsampled once. **Quality Off = 2×, On = 4×** (two cascaded 2× stages, gfx-drive's choice). At host
  rates ≥ 88.2 kHz the factors halve (Off = 1×, On = 2×): the inner rate stays ≥ 88.2 kHz.
- Every band passes the same up / down pair, so every band carries the same group delay: coherence does
  not depend on which bands are driven. Linear modules (filters, ring mod) also profit: a ring-mod sum
  frequency up to 40 kHz is removed by the downsampler instead of folding.
- **ADAA1 on every nonlinear module**, inside the oversampling. At small signals ADAA1 is the two-tap
  average (x[n] + x[n−1]) / 2: half an inner sample of delay **and a cos(π f / fs_inner) roll-off**. A slot
  that holds a linear module or Off passes gfx-dsp's `HalfSample` (the same average) instead, and the
  band's dry gets two of them. Every path in every band then carries exactly the same response, one inner
  sample of delay with a cos² magnitude: the per-band mix and the band sum stay coherent (gfx-drive's
  blend lesson). When x[n] ≈ x[n−1] ADAA falls back to f at the midpoint (gfx-dsp `Adaa1`'s threshold).
- **Droop correction** (added at review). Two averages cost −0.95 dB at 10 kHz and **−4.0 dB at 20 kHz**
  at 2× (−0.23 / −0.95 dB at 4×) [P], on every band, even with every slot Off. Because the droop is common
  to all paths, one filter after the band sum, before the downsampler, removes it:
  C(z) = ((1 + a) / (1 + a z⁻¹))², a = 0.8. Residual ±0.06 dB to 20 kHz at 2×, ±0.012 dB at 4× [P]; it
  also gives the shaped signal back the highs ADAA1 takes. It is a constant, minimum-phase, two-pole
  filter: no coherence issue. (The exact inverse has its poles at z = −1; a = 0.8 leaves the zero at the
  inner Nyquist in place, where the downsampler's stop band is.)
- **What the alignment is worth** [P]: two equal paths half an inner sample apart lose 0.5 dB at 20 kHz
  when summed at 2× (one sample: 2.0 dB), nothing audible at the crossovers. So the alignment is a
  refinement, and the one exception is accepted: **Fuzz** holds two ADAA stages (one inner sample), half
  a sample more than its slot's allowance; a Fuzz band blended with its dry dips 0.5 dB at 20 kHz at 2×.
- **How much antialiasing** [P: numpy, ideal downsampler, sine at −6 dBFS, strongest aliased component
  below 20 kHz relative to the fundamental]:

  | Shaper, drive | 2× + ADAA1, sine at 2 / 5 / 9 kHz | 4× + ADAA1 |
  |---|---|---|
  | Hard clip, 24 dB (Amount 50 %) | −59 / −52 / −37 dB | −79 / −65 / −69 dB |
  | Hard clip, 48 dB (Amount 100 %) | −45 / −43 / −35 dB | −59 / −51 / −50 dB |
  | Hard clip, 24 dB, 5 kHz, **no ADAA** | −33 dB | −44 dB |
  | Triangle fold, 24 dB | −43 / −36 / −11 dB | −57 / −36 / −44 dB |

  ADAA1 buys about 20 dB at equal oversampling; 2× + ADAA1 is better than Prisma's Quality On (2×, no
  ADAA) and far better than its Off. It is not alias-free at full drive, and no folder is on a 9 kHz
  sine. What keeps the pedal clean in practice is the split itself: a shaper in the low or low-mid band
  only sees content below its crossover (a 500 Hz partial at 48 dB of clipping aliases at −58 dB at 2×,
  −87 dB at 4×), and the top band is the one presets leave clean or light. If the bench hears fizz on a driven top
  band, the upgrade path is ADAA2 on the clip / tanh family or 8× on Quality On (open, bench).
- Why not oversample per band: four up / down pairs cost what one pair plus the crossovers at the higher
  rate cost [A, §2.7], and a per-band choice would give bands different group delays (combs at the
  crossovers) unless the clean bands paid for an identity path anyway. For the same reason Quality Off
  never drops to 1× for a low band (open question 6).
- **`quality` and `slope` are set-up switches, not performance gestures**: the other engine is started,
  run for 30 ms beside the playing one (its crossovers and DC blocks settle; Ring mod and Sample divide
  phases are copied), then crossfaded over 10 ms. A cold crossfade would let a 112 Hz crossover ring in
  under the fade.

### 2.2 Crossovers

- **Linkwitz-Riley tree with compensation all-passes** (Prisma's topology): band 0 = LP(x1) · AP(x2) ·
  AP(x3), band 1 = HP(x1) · LP(x2) · AP(x3), band 2 = HP(x1) · HP(x2) · LP(x3), band 3 = HP(x1) · HP(x2) ·
  HP(x3), where each HP output is shared down the tree. Every band then carries the same all-pass
  A(x1) A(x2) A(x3) times its own magnitude response, and **the clean sum is that all-pass exactly**:
  magnitude flat, phases of all four bands equal at every frequency. Checked [P] on the analog
  prototypes and on the TPT forms at 96 kHz, at the default crossovers, at minimum spacing and at both
  ends of the range, LR4 and LR8: sum flat within 1e-6 dB, band phases equal within 1e-15 rad. The
  identity needs every section of one crossover to share one coefficient set, updated on the same
  sample; it is exact in steady state and approximate while a crossover moves.
- **`slope`**: 24 dB / oct (LR4, default, Prisma's) or 48 dB / oct (LR8). LR4 = two 2nd-order
  Butterworth TPT SVF sections per LP / HP, its all-pass = one 2nd-order section (Q = 1/√2). LR8 = four
  sections (Q 0.541, 1.307, twice), all-pass = two (Q 0.541, 1.307). The first section of a split gives
  its low-pass and its high-pass from one SVF tick: 3 ticks per LR4 split, 7 per LR8 split. LR8 halves
  the region where a partial is in two bands at once (less "double distortion" at the crossover, below)
  at about twice the dispersion.
- **Laws**: x_k = 0 – 100 %, **f = 20 · 1000^(x/100) Hz** (Prisma's, kept: §3.2). Ordering as Prisma:
  x2 ≥ x1 + 2 %, x3 ≥ x2 + 2 % (0.2 octave), the later crossover yields; effective values are held in
  Prisma's windows (x1 2 – 94 %, x2 4 – 96 %, x3 6 – 98 %: 23 Hz – 17.4 kHz), so three crossovers always
  fit. The parameters keep 0 – 100 %; the effective frequency is what is displayed. Smoothed in the log-frequency
  domain (20 ms) and recomputed per sample: TPT sections stay stable and click-free while they move
  (gfx-dsp `svf`).
- **Dispersion** [A, confirmed P]: the sum delays low frequencies by the three all-passes' group delays,
  2√2 / (2π f_c) each at DC for LR4: **4.0 + 0.7 + 0.1 = 4.9 ms** at the default crossovers (112 / 632 /
  3,557 Hz), peaking at 5.7 ms near 73 Hz; 0.53 ms at 1 kHz, 0.13 ms at 4 kHz. LR8: 9.0 ms at DC, a
  12.6 ms peak at 100 Hz, 0.32 ms at 4 kHz. Crossovers packed at minimum spacing around 112 Hz: 12.6 ms
  (LR4), 28 ms (LR8). That is the price of zero latency (Multipass and Saturn's default
  mode pay it too). It is audible as a softened attack on a square-wave test, rarely on a guitar or a bass
  through a cab [I: to confirm on the bench]. It matters when golem splits a lane and sums gfx-grit
  against a dry branch: that branch will comb unless it goes through gfx-grit too (stated in the
  catalogue note).
- **Linear phase: declined.** An FIR crossover at 112 Hz with LR4-like selectivity needs tens of
  milliseconds of reported latency and pre-rings driven transients; a live rack cannot use it, and the
  benefit (a null instead of an all-pass when every band is clean) does not survive the first driven
  band. Confirmed at review (open question 4).
- **What a split does to a driven signal** (both inherent to multiband distortion, measured in §5):
  - *Crossover dip*: a partial at f_c is in two bands at −6 dB each; each half saturates less than the
    whole would, so the distortion thins around each crossover. LR8 narrows the region.
  - *Intermodulation*: notes in different bands no longer intermodulate (the point: Anderton's chords).
    Inside one band they still do. A driven low band's harmonics spread over the whole spectrum; they are
    not filtered back into the band (no "post filter"): a Lowpass in slot 2 does that when wanted, and the
    low band is usually left clean.

### 2.3 Band path

- **Two slots** per band (catalogue `*_m1` / `*_m2`, values `*_v1` / `*_v2`): slot 1 then slot 2.
- **DC block** on every band's wet, always on (2nd-order high-pass at 5 Hz): Prisma switched it with the
  module list and reset its state, a click source. Always on costs 4 SVF ticks. It sits on the wet, before
  the mix and the gain, so a band's DC (Rectify at 100 %: 0.64 × peak) never meets a moving gain or mix
  (no thump on a solo, a mute or an EXP sweep). Its phase lead (10° at 40 Hz) is the same on every band's
  wet; against a band's own dry it costs under 0.1 dB at 20 Hz. With every slot Off the pedal is the
  all-pass of §2.2 times this high-pass (the bench reference includes it).
- **Per-band mix**: out_b = gain_b · (dry_b · (1 − m_b) + wet_b · m_b), m_b = `wet` · `<b>_mix`. The
  dry is the band after the crossover, aligned (§2.1): **the blend has no latency comb**. What a module
  does to the phase is its own sound and stays: a Lowpass, Highpass, Peak or Cabinet shifts the wet
  against the dry; Sample divide delays it by half a hold period (first notch at the hold rate when
  blended 50 / 50); a folder's fundamental changes sign as the drive rises. The memoryless saturators
  (Soft, Hard, Heavy, Tube, Rectify, Zero cross, Bit crush) blend without notches.
  - **The band gain applies to the whole band**, not to the wet only as in Prisma. A muted low band then
    stays muted at any `wet` (Prisma's Telephone at wet < 100 % leaks the dry lows). Identical to Prisma at
    `wet` 100 % or gain 0 dB, which covers all four catalogue presets (§3.3).
- **Solo / bypass** per band (expert): solo any band → the others are muted; bypass → the band passes
  its dry, gain 0 dB. 5 ms raised-cosine ramps. Prisma's per-band mute is not carried: `<b>_gain` at
  −60 dB is a mute, and solo covers the set-up use.
- **`level`**: −24 – +12 dB after the sum.

### 2.4 Modules

All modules run at the inner rate. "Amount" is the slot's value v (0 – 100 %) after Drive and Focus
(§2.5). **Comp.**: the module is level-matched (§2.6). The enum is the catalogue's list, in its order;
**Tube** and **Fuzz** are appended (enums only grow at the end). Every law is continuous from 0 %: no
jump when a value leaves 0 (Prisma skips a module at v = 0).

| # | Module | gfx-grit law of Amount v (0 – 100 %) | Display | Comp. | Antialiasing / notes |
|---|---|---|---|---|---|
| 0 | Off | — | — | | HalfSample (alignment). The linear modules below (11 – 18, 20 – 22) pass a HalfSample too |
| 1 | Soft clip | drive d = 0.48 v dB (0 – 48 dB); y = tanh(g x) | dB | ✓ | ADAA1 (gfx-dsp `Shaper::Tanh`); no 0 dBFS input clamp |
| 2 | Hard clip | d = 0.48 v dB; y = clip(g x, ±1) | dB | ✓ | ADAA1 (closed form, gfx-drive's `clip_af`) |
| 3 | Heavy | exponent e = 1 − 0.0099 v (Prisma's law); y = sgn x · \|x\|^e, **linear below −40 dBFS** (value-continuous knee) | exponent | ✓ | ADAA1 (F = \|x\|^(e+1) / (e + 1) above the knee). The knee caps the gain on hiss at +40 dB (Prisma: up to +90 dB) |
| 4 | Asym | e = 1 − 0.0099 v (kept); Prisma's two half-curves, knee as Heavy | exponent | ✓ | ADAA1; DC block after |
| 5 | Rectify | y = (1 − v/100) x + (v/100) \|x\| (kept) | % | ✓ | ADAA1 (F of \|x\| = x\|x\| / 2) |
| 6 | Fold | d = 0.48 v dB; triangle fold of g x, period 4 | dB | ✓ | ADAA1 (piecewise quadratic F). Folding is the widest-band module: §5 bounds |
| 7 | Sine fold | d = 0.48 v dB; y = sin(π/2 · g x) | dB | ✓ | ADAA1 (F = −2 cos(π g x / 2) / (π g)); soft saturation at 0 dB, folds above. Same fold count as Fold at the same dB (peaks at g x = 1) |
| 8 | Zero cross | dead zone T = −66 + 0.60 v dBFS (−66 – −6 dBFS); y = sgn x · max(\|x\| − T, 0) with a 6 dB soft knee (gain −6 dB at T + 6 dB) | dBFS | ✓ capped | ADAA1. Class-B crossover distortion; above T ≈ the signal's peaks it is a gate (a −18 dBFS sine is silent from 80 %) |
| 9 | Bit crush | bits = 16 − 0.15 v (16 – 1, fractional: step 2^(1 − bits)); Tread rounding (silence stays 0) | bits | | ADAA1 on the staircase (exact F). gfx-crusher's continuous-bits law. Within 1.2 dB of unity down to 4 bits at the reference level; below 3 bits a −18 dBFS signal rounds to silence (the gate-crush sound, as in Prisma): a Gain in slot 1 pushes the band into the bits |
| 10 | Sample divide | hold rate R = 40 · 1000^(1 − v/100) Hz (kept; 40 Hz – 40 kHz) | Hz | | gfx-crusher's fractional phase accumulator (f64). The held value is read at the fractional hold instant (linear interpolation); the inner sample that contains the edge takes the area mean of the old and new levels. That is ADAA1 of the staircase: the same half-sample delay as the other modules, and no edge jitter at the inner rate. The downsampler removes the images above fs / 2; the aliases below are the effect |
| 11 | DC | offset = v/100 − 0.5 in full-scale units, ±0.5 (neutral 50 %) | % | | Biases the next slot: slot 1 only. In slot 2 the DC block removes it (inert) |
| 12 | Width | side × 2v/100 (kept; neutral 50 %) | % | | Stereo layout only; inert in mono |
| 13 | Gain | −24 + 0.48 v dB (−24 – +24, neutral 50 %) | dB | | Not a mute: `<b>_gain` does that |
| 14 | Pan | Prisma's law (kept; neutral 50 %) | % | | Stereo only |
| 15 | Lowpass | fc = 20 · 1000^(1 − v/100) Hz (kept), Q 1.2 | Hz | | TPT SVF |
| 16 | Highpass | fc = 20 · 1000^(v/100) Hz (kept), Q 1.1 | Hz | | TPT SVF |
| 17 | Peak | f = 20 · 1000^(v/100) Hz (kept), Q 1.5, gain = `<b>_y<n>` (−30 – +30 dB, default +9 dB, new, §3) | Hz | | SVF bell |
| 18 | Dry/wet | y = wet · (1 − v) + band dry · v (kept; 100 % = dry) | % | | Dry aligned to the slot |
| 19 | Stereo rectify | Prisma's law (kept) | % | ✓ | Stereo only |
| 20 | Stereo DC | offset = (v/100) · (c − ½), c = 0 left, 1 right: ±0.5 at 100 % (linear) | % | | Stereo only |
| 21 | Ring mod | × cos(2π f t), f = 20 · 1000^(v/100) Hz (kept) | Hz | | Phase accumulator f64, phase 0 on reset; alias-free at the inner rate |
| 22 | Cabinet | 6 voicings, index round(5 v/100) (kept stepping), in the order of Prisma's roles: 0 4×12 closed (dark), 1 1×15 bass, 2 4×12 bright, 3 8″ small, 4 2×12 open, 5 1×12 combo | name | normalised | **Minimum-phase biquad voicings** (low resonance, presence, high roll-off), no convolution, zero latency; Prisma's IRs are not used (open question 7) |
| 23 | **Tube** (new) | d = 0.48 v dB; y = tanh(g x + b) − tanh(b), b = 0.3 | dB | ✓ | ADAA1 (`Shaper::Asym`): even harmonics; DC block after |
| 24 | **Fuzz** (new) | d = 0.48 v dB into gfx-drive's Fuzz core: two biased clip stages, a 40 Hz high-pass between them | dB | ✓ | ADAA1 on each stage: one inner sample, half a sample over the slot's allowance (accepted, §2.1); DC block after |

- **Stepped and type changes**: changing a slot's type crossfades old → new over 10 ms (both run during
  the fade); Cabinet's voicing index crossfades the same way. Amounts are smoothed 20 ms (Prisma: 1 ms
  linear) and follow the contract's EXP rule.
- **Tape** is not in the list: it is appended when gfx-tape (lot 2) has its engine, as a module calling
  that engine. An append is free for the plugin, not for golem: golem writes an enum as index / (n − 1)
  from the catalogue's option list (contract, REAPER normalization), so **the catalogue's eight option
  lists must hold exactly the plugin's options**. 25 at the first release; each later append is a
  catalogue change made with the plugin release.
- **Soft clip and Hard clip at 0 % are not Off**: unity gain into the shaper (tanh compresses a peak at
  −3 dBFS by 1.3 dB; Hard clip limits at 0 dBFS in the band).
- **Licence hygiene**. Prisma is AGPL-3.0-or-later; golem-fx is GPL-3. What is taken from it is its
  parameter laws, restated as formulas in §1.1 (facts needed to convert golem's presets). The modules
  are written from this document, not from Prisma's source: no source file, table, GUI asset or impulse
  response of Prisma enters the repository, its make-up factors (0.92, 0.86) are replaced by §2.6, and
  the bench's "Prisma" reference formulas are typed from §1.1. The designer's clone stays in a scratch
  directory.

### 2.5 Drive and Focus (the face)

- **`drive`** (shared id, 0 – 100 %, extra saturation on top of the modules' own): adds
  **k_type · drive / 100 points** to the Amount of every nonlinear module, clamped at 100. Each module
  then moves along its own law. Linear modules are untouched, so a band holding only a filter never
  changes.

  | Module | k_type | Full Drive adds |
  |---|---|---|
  | Soft clip, Hard clip, Fold, Sine fold, Tube, Fuzz | 50 | +24 dB |
  | Heavy, Asym | 50 | exponent −0.495 |
  | Rectify | 50 | +50 % |
  | Zero cross | 20 | dead zone +12 dB |
  | Bit crush | 20 | −3 bits |
  | Sample divide | 20 | hold rate −2 octaves |

  One weight for all (the first draft's 50 points) would have taken a Bit crush from 8 bits to 0.5, a
  Sample divide down five octaves and a Zero cross up 30 dB: on three of the four catalogue presets a
  Drive sweep ended in silence or in a 240 Hz hold. The weights are constants [I: to trim by ear], not
  parameters.
- **`focus`** (−100 – +100 %): adds **k_type · focus · w_b / 200 points**, w = (−1, −⅓, +⅓, +1) for
  (lo, lm, hm, hi): −100 % moves the dirt toward the lows, +100 % toward the highs (±12 dB on a
  saturator in the outer bands). Same clamp, same modules.
- Both default to 0: converted presets play exactly as converted. Both are EXP targets: one pedal sweep
  takes a Trash-style patch from edge to wreck (Drive) or from body to fizz (Focus).

### 2.6 Level compensation (auto-gain)

- Each compensated module carries a **static make-up c(type, v)**: the RMS ratio input / output of a
  1 kHz sine peaking at −18 dBFS through the shaper, tabulated at 65 points of v per type and
  interpolated, **limited to −30 – +12 dB**. The tables are generated at build time (const data): nothing
  is computed when the plugin turns on. The make-up follows the Amount after Drive and Focus.
- Static, not a follower: no pumping, no breathing on decays. What it does and does not do [P, tanh]:

  | Drive | Make-up | Small-signal gain (noise floor) | A −6 dBFS input comes out |
  |---|---|---|---|
  | 0 dB | 0 dB | 0 dB | −0.5 dB re input |
  | 12 dB | −11.5 dB | +0.5 dB | −4.4 dB |
  | 24 dB | −19.1 dB | +4.9 dB | −10.5 dB |
  | 48 dB | −20.9 dB | +27 dB | −12.0 dB (= the reference level) |

  At the reference level the loudness holds across Amount. Above about 24 dB of drive every input comes
  out near the reference level (that is saturation: the band is a square wave), and the quiet parts rise
  by the small-signal gain, as in any distortion. So "Amount is a tone control" is true at playing level;
  dynamics shrink with Amount, by nature. A square has more highs than a sine at equal RMS: expect + 2 to
  3 LU at full Amount (bench).
- **The reference level** is one constant for all modules. −18 dBFS peak is a first value [I]: the
  bench sets it from the per-band peak levels of the DIs (a band holds less than the full signal).
- **Not compensated**: Bit crush and Sample divide (unity by nature; a table measured where the
  quantiser gates would only add gain to silence). **Capped**: Zero cross, whose output vanishes when T
  reaches the signal (the +12 dB limit stops the table from chasing it).
- Always on (no toggle, open question 11): band gains and `level` set the levels; Prisma's 0.92 factors
  were the same idea done by hand.

### 2.7 Stereo, latency, CPU

- **Layout**: mono 1 / 1 first, stereo 2 / 2 second (not stereo by nature). In 1 / 1 the stereo
  modules (Width, Pan, Stereo rectify, Stereo DC) are inert, as in Prisma with one channel.
- **Latency: reported 0.** No look-ahead. The output carries the half-bands' minimum-phase group delay
  (a few samples at the base rate: gfx-drive measured ~6 at 4×), one inner sample of ADAA alignment
  (half a base sample at 2×) and the crossover dispersion (§2.2). gfx-bench: `skip_latency` (gfx-drive's
  precedent).
- **CPU** [A, estimate, mono, 48 kHz; tanh-ADAA ≈ 25 ns per call in f64, an SVF tick ≈ 4 ns]:

  | Case | Inner rate | Crossovers (15 SVF ticks LR4) | Modules | OS, mix, DC | Total |
  |---|---|---|---|---|---|
  | All Off | 96 kHz | 120 ns | 0 | 60 ns | **≈ 180 ns** |
  | Typical preset (2 – 3 nonlinear slots), Off | 96 kHz | 120 ns | 125 ns | 60 ns | **≈ 300 ns** |
  | Same, Quality On | 192 kHz | 240 ns | 250 ns | 100 ns | **≈ 600 ns** |
  | Worst (8 nonlinear slots, LR8, On) | 192 kHz | 400 ns | 800 ns | 100 ns | **≈ 1.3 µs** |

  Stereo doubles it. For scale: gfx-drive 736 ns stereo, gfx-reverb 315 ns. The worst case is 6 % of a
  core: accepted for a pedal whose presets use two to four slots. The crossover column is an upper
  bound: with the shared first section the tree is 12 ticks (LR4) or 27 (LR8) per inner sample, and the
  three `tan()` of the coefficients are computed only while a crossover moves. Heavy and Asym cost one
  `powf` per inner sample (≈ 30 ns [I]), more than tanh.

### 2.8 gfx-dsp reuse and what is new

| Module | Use |
|---|---|
| `oversample` | The one up / down pair (2× and 4×), `identity` not needed (every band is inside); the droop correction (§2.1) lives beside `HalfSample` in `adaa` |
| `adaa` | `Adaa1` + `Shaper::Tanh` (Soft clip), `Shaper::Asym` (Tube); `HalfSample` on Off / linear slots and on the dry |
| `svf` | Crossover sections, all-passes, Lowpass / Highpass / Peak modules, DC blocks, Cabinet voicings |
| `matched` | Cabinet voicings above fs / 4 at the inner rate are not needed (192 kHz): plain SVF suffices |
| `smooth`, `exp_smooth` | Amounts, gains, crossovers (log domain), Drive / Focus / Wet / Level under EXP |
| gfx-drive's shapers | Hard clip and Fuzz antiderivatives (`clip_f` / `clip_af`, the two-stage Fuzz core, the `Adaa` whose shaper may change every sample) — **moved into gfx-dsp `adaa`** as new `Shaper` variants (Clip, the tanh → clip blend) and a `FuzzCore`, with no change of sound for gfx-drive (its tests must still null); gfx-bassdi and gfx-grit then use the same code. The grit-only curves (Heavy, Asym, Rectify, Fold, Sine fold, Zero cross, staircase) stay in the crate |
| gfx-crusher's `converter` (designed, not coded) | Only its two laws: continuous bits with Tread rounding (step 2^(1 − bits)) and the f64 fractional clock. Not its engine: gfx-crusher samples and renders through a minimum-phase BLEP kernel, which a band-local module does not need inside 2× / 4× + ADAA. The two functions are a few lines; they live in gfx-grit and are not a shared module |
| **New: `crossover`** (gfx-dsp) | **One module, shared with gfx-bassdi** (which proposes the same name), in three layers. (1) `LrSplit`: one crossover point, order 4 or 8, low-pass and high-pass from shared SVF sections, and `LrAllpass`, its twin (same frequency, same rate, same coefficient update), with the log-domain frequency smoother. (2) `LrTree<N>`: the N-band tree with compensation all-passes (gfx-grit, then the lot-3 multiband compressor). (3) gfx-bassdi's band-gain sections S(g, f) = g · LP + HP and the `bass_keep` sum, built on (1). One test file holds the identities (LP + HP = AP, equal phase, flat tree sum). Whichever crate is coded first creates layer (1); gfx-drive, gfx-emet, gfx-chorus and gfx-flanger adopt it later |
| **New in the crate** | The 25 modules, the compensation tables (build-time generator), slot crossfades, the Drive / Focus weights |

## 3. Parameters

### 3.1 Ids, names, units

Every catalogue id and **name** is kept (names are golem's join keys: "Low Module 1 Type", "Low Module 1
Value", "Low Gain", "Low Mid Crossover", "High Mid Crossover", "High Crossover", "Wet", "Quality"). The
module enums keep the catalogue's 23 options in order; Tube and Fuzz are appended, and the catalogue's
option lists gain them at the same time (§2.4).

### 3.2 The 2026-11-14 rule applied

Prisma exposes every value on 0 – 1. Per the rule: **keep 0 – 1 where its law is linear in a real unit,
switch where it is not**, with a one-time conversion.

| Catalogue value | Prisma's law | Decision | gfx-grit |
|---|---|---|---|
| `x1`, `x2`, `x3` | f = 20 · 1000^x: linear in octaves | **Keep 0 – 100 %** (Surge cut-off precedent), display Hz | unchanged values |
| `wet` | linear crossfade | **Keep %** | unchanged |
| `quality` | bool | Keep | Off = 2×, On = 4× (both zero latency) |
| `*_gain` | (2g)²: dB = 40 log₁₀(2g), −∞ at 0 | **Switch to dB**: −60 – +12 dB, −60 = mute | **dB = max(−60, 40 · log₁₀(2 · g/100))**; g = 0 → −60 |
| `*_v1`, `*_v2` | one law per module type | **Keep the % unit** (a polymorphic slot cannot carry one real unit) but **redefine % per type** where Prisma's law is not linear in a real unit; the value is converted once, keyed by the slot's type | formulas below |

Module value conversions (v = old value / 100, v′ = new value / 100, clamped to [0, 1]):

| Type | Conversion | Why |
|---|---|---|
| Soft clip, Hard clip | val = 1 − (1 − ((1 − v)¹⁰ + 1 − v^0.2) / 2) · 0.99; **v′ = −20 log₁₀(val) / 48** | Prisma's drive in dB (2.4 dB at 1 %, 40 dB at 100 %) onto 0 – 48 dB linear |
| Fold | **v′ = 20 log₁₀(70 v² + 1) / 48** | Same: fold drive in dB |
| Sine fold | **v′ = min(5 v², 1) · 20 log₁₀(70 v² + 2) / 48** | Prisma's sin(π g x) peaks at g x = ½, ours at g x = 1: the same fold count needs twice the gain (the draft's 35 v² + 1 was 6 dB short). The first factor stands for Prisma's fade-in of the sine below v = 0.45. By intent: the curve itself changes |
| Zero cross | **v′ = (T(v) + 66) / 60**, T(v) in dBFS interpolated linearly in: v 0.05 → −67, 0.10 → −46, 0.20 → −34, 0.30 → −30, 0.40 → −27, 0.50 → −25, 0.60 → −22.5, 0.70 → −20, 0.80 → −16, 0.90 → −11, 0.95 → −8.5, 1 → −6 | T = the level where Prisma's whole curve has −6 dB of gain, minus 6 dB [P: computed from the law of §1.1]. The draft converted the v¹⁰ clip alone: Prisma's 23 % (audible: −6 dB at −27 dBFS) became a −90 dBFS dead zone, i.e. nothing |
| Bit crush | bits = log₂(2 / (0.99 v⁵ + 0.01)); **v′ = (16 − bits) / 15** | Bits linear (16 → 1) |
| Gain | **v′ = (40 log₁₀(v / 0.35) + 24) / 48** | dB linear, neutral at 50 %. Prisma's v < 0.088 (below −24 dB, v = 0 a mute) clamps to −24 dB |
| DC | **v′ = 0.5 + 0.5 · (2v − 1)⁵** | Linear offset |
| Stereo DC | **v′ = v⁵** | Linear offset, ±0.5 at 100 % as Prisma's |
| Heavy, Asym, Rectify, Sample divide, Width, Pan, Lowpass, Highpass, Peak, Dry/wet, Stereo rectify, Ring mod, Cabinet | **v′ = v** | Linear in a real unit (octaves, exponent, %), or stepped |

- **Who converts**: golem's engine table, once, for block presets and saved chains (it reads the slot's
  `*_m` to choose the formula). Needs golem-rack's agreement, as gfx-crusher's and gfx-freqshift's
  conversions; the catalogue entry changes `*_gain` to dB (min −60, max 12, default 0), its option
  lists (25) and its notes ("Keep Quality OFF live" no longer applies: both settings report 0). The
  `*_v` unit stays %; the real value (dB, bits, Hz) is the plugin's formatted string, which golem may
  show.
- **Rule for block presets** (gfx-crusher's): golem's four `grit` presets are converted **faithfully**
  (below, they are already sensible) and the new factory set is authored in `presets/gfx-grit.json`.
  **Saved rig chains convert faithfully**, if golem-rack finds any.
- **Fallback** if golem-rack cannot do a type-keyed conversion: keep Prisma's laws for the eight
  converted types under the same % (presets play "as is"), with the antialiasing, Heavy's knee and the
  compensation still ours. Worse for the knob, never better otherwise.

### 3.3 The four catalogue presets, converted

| Preset | Catalogue values | gfx-grit (faithful) |
|---|---|---|
| **Grit** (default) | hm: Sample divide 24, Gain 53; x 25 / 63 / 70; gains 50 | hm Sample divide 24 % (7.62 kHz), Gain 65 % (+7.2 dB); crossovers 112 Hz / 1.55 kHz / 2.52 kHz; gains 0 dB |
| **Megaphone** | lm: Hard clip 40; hm: Heavy 55, Peak 60; lo_gain 5, hi_gain 15; x 49 / 70 / 77 | lm Hard clip 42 % (20.4 dB); hm Heavy 55 % (e = 0.455), Peak 60 % (1.26 kHz), `hm_y2` **+9 dB (the default: the peak the preset's note describes; in Prisma it was inert, a bug of the headless use)**; lo −40 dB, hi −20.9 dB; 590 Hz / 2.52 kHz / 4.08 kHz |
| **Telephone** | lm: Soft clip 25; hm: Bit crush 20, Sample divide 15; lo, hi gain 0; x 39 / 50 / 74 | lm Soft clip 33 % (16.0 dB); hm Bit crush 56 % (7.6 bits), Sample divide 15 % (14.2 kHz); lo, hi −60 dB (muted); 296 Hz / 632 Hz / 3.32 kHz |
| **Broken** | lm: Fold 35; hm: Bit crush 45, Ring mod 30; hi: Sample divide 40; x 25 / 50 / 75; wet 80 | lm Fold 41 % (19.6 dB); hm Bit crush 66 % (6.1 bits), Ring mod 30 % (159 Hz); hi Sample divide 40 % (2.52 kHz); wet 80 % |

[A] from the formulas above. Levels differ from Prisma (our compensation vs its 0.92 factors): the new
presets are level-matched to the bypass (±2 LU on the bench DIs) through the band gains and `level`.

### 3.4 Front panel

| id | Name | Type | Range | Catalogue | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `drive` | Drive | f | 0 – 100 % | new (shared id) | 0 % | 20 ms | ✓ | Up to +50 points (saturators: +24 dB) or +20 points (Bit crush, Sample divide, Zero cross) on every nonlinear module's Amount (§2.5) |
| `focus` | Focus | f | −100 – +100 % | new | 0 % | 20 ms | ✓ | Tilts the dirt toward the lows (−) or the highs (+), ±25 points (saturators) at the outer bands |
| `wet` | Wet | f | 0 – 100 % | same | 100 % | 20 ms | ✓ | Per-band dry / wet crossfade (the catalogue's law; the dry is the band after the split) |
| `level` | Level | f | −24 – +12 dB | new | 0 dB | 20 ms | ✓ | Output trim |
| `quality` | Quality | b | off / on | same | off | 30 ms pre-roll, 10 ms crossfade | | Off = 2× + ADAA, On = 4× + ADAA; both reported latency 0 |

### 3.5 Bands (catalogue), × 4: `lo_`, `lm_`, `hm_`, `hi_`

Names as the catalogue: "Low …", "Low Mid …", "High Mid …", "High …".

| id | Name | Type | Range | Catalogue | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `<b>_m1`, `<b>_m2` | \<Band\> Module 1 / 2 Type | e | Off · Soft clip · … · Cabinet · Tube · Fuzz (25) | same 23, 2 appended | Off | 10 ms crossfade | | Slot type (§2.4) |
| `<b>_v1`, `<b>_v2` | \<Band\> Module 1 / 2 Value | f | 0 – 100 % | same unit, laws §2.4 | 0 % | 20 ms | ✓ | Amount, in the module's own law; displayed in its unit (dB, bits, Hz, exponent) |
| `<b>_gain` | \<Band\> Gain | f | −60 – +12 dB | **% → dB** (§3.2) | 0 dB | 20 ms | ✓ | Band level, whole band; −60 = mute |

Crossovers (catalogue group "Crossovers"):

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `x1` | Low Mid Crossover | f | 0 – 100 % (20 Hz – 20 kHz, octave-linear) | 25 % (112 Hz) | 20 ms, log | ✓ | Low / low-mid split |
| `x2` | High Mid Crossover | f | same | 50 % (632 Hz) | 20 ms, log | ✓ | Low-mid / high-mid split; pushed ≥ x1 + 2 % |
| `x3` | High Crossover | f | same | 75 % (3.56 kHz) | 20 ms, log | ✓ | High-mid / high split; pushed ≥ x2 + 2 % |

### 3.6 Expert (new)

| id | Name | Type | Range | Default | Smooth | Role |
|---|---|---|---|---|---|---|
| `<b>_mix` | \<Band\> Mix | f | 0 – 100 % | 100 % | 20 ms | The band's own wet, multiplied by `wet` (parallel dirt on one band: bass) |
| `<b>_y1`, `<b>_y2` | \<Band\> Module 1 / 2 Peak Gain | f | −30 – +30 dB | **+9 dB** | 20 ms | Gain of a Peak module in that slot (Prisma's GUI-only Y, now reachable, and signed: a cut before a clipper is a scoop); inert for other types. A constant default, so a Peak is heard as soon as it is chosen |
| `<b>_solo` | \<Band\> Solo | b | off / on | off | 5 ms | Hear one band (set-up); several soloed = their sum |
| `<b>_bypass` | \<Band\> Bypass | b | off / on | off | 5 ms | Band passes clean at 0 dB (modules, mix and gain ignored) |
| `slope` | Crossover Slope | e | 24 dB/oct · 48 dB/oct | 24 dB/oct | 30 ms pre-roll, 10 ms crossfade | LR4 (Prisma's) or LR8 (§2.2) |
| `bypass` | Bypass | b | off / on | off | 10 ms | nih-plug `make_bypass`, what the coded effects ship |

No default depends on another parameter. Values that differ per golem use are presets (§4). **50
parameters in all (25 catalogue, 25 new)**: the largest set in the suite, but 40 of them are the 4 × 10
of the bands, and golem's Designer shows the catalogue's 25 plus the face. A per-band mute can be added
later (ids are permanent, additions are free).

Expert groups (`visuals/data/groups.json`): **Low**, **Low Mid**, **High Mid**, **High** (each band's
`m`, `v`, `gain`, `mix`, `y`, `solo`, `bypass`), **Crossovers** (`x1` – `x3`, `slope`).

## 4. Applications

Reference settings become block presets (`presets/gfx-grit.json`), level-matched to the bypass. No
artist or trademark names. Unlisted bands: Off, 0 dB.

| Use / preset idea | Settings to start from | Feature that matters |
|---|---|---|
| **Grit** (catalogue default: mid crunch on drums and loops) | as §3.3 | Lows untouched, a sampled bite on the high mids |
| **Megaphone** (voice) | as §3.3 (`hm_y2` +9 dB) | Hard clip on the low mids, a peak at 1.26 kHz, highs and lows pulled down |
| **Telephone** (voice, intros) | as §3.3 | Muted outer bands = a band-pass, light crunch inside |
| **Broken** (lo-fi wreck) | as §3.3 | Fold, crush, ring and divide, 80 % wet |
| **Bass Split** (the Parallax / B7K move) | x1 37 % (≈ 250 Hz), lo Off; lm Tube 45 %; hm Hard clip 40 % → Lowpass 23 % (≈ 4 kHz); hi gain −12 dB; `slope` 48 | Fundamental clean and mono-solid, the grind above it; LR8 keeps lows out of the clip |
| **Bass Growl, parallel** | as Bass Split, `lm_mix` 60 %, `hm_mix` 70 % | Per-band parallel blend, no comb |
| **Chord Clarity** (guitar rhythm) | all four bands Soft clip 35 – 45 %, x 30 / 55 / 75 % | Notes in different bands do not intermodulate: chords stay readable at high gain |
| **Tight Low Drive** (drop-tuned riffs) | lo Off at −3 dB, lm Soft clip 40 %, hm Tube 50 %, hi Hard clip 20 % → Lowpass 17 % (≈ 6 kHz), x1 30 % (≈ 160 Hz) | Clean lows under a hot mid band: the "tight" boost done by bands |
| **No-Fizz Lead** | lm Tube 50 %, hm Soft clip 55 %, hi Off at −6 dB, x3 70 % (2.5 kHz) | The top band left clean and lowered: dirt without fizz |
| **Drum Bus Crunch** | lo Off; lm Heavy 30 %; hm Soft clip 35 %; hi Bit crush 50 % (8.5 bits); `wet` 60 % | Parallel crunch, kick untouched (all three are memoryless: the blend has no notch) |
| **Tight Kick** (Prisma's own preset, folded into two slots) | x1 30 %; lo Zero cross 65 % (dead zone −27 dBFS) → Gain 55 % (+2.4 dB) [A from §1.1 finding 6; by ear] | A dead zone on the kick's band only: the tail falls into it, the hit passes. Punch, short tail |
| **Fold Synth Top** (keys, synths) | hm Sine fold 40 %, hi Fold 30 % → Lowpass 17 % (≈ 6 kHz) | Folding above a clean body, antialiased at 4× |
| **Radio Air** | hm Sample divide 30 % (5 kHz), hi −60 dB, lo −20 dB | Band-limited, sampled voice |
| **Fuzz Upper** (guitar solos) | lm Fuzz 30 %, hm Fuzz 50 %, lo Off | Fuzz without mud: the low band never gates or farts out |
| **EXP: edge to wreck** | any preset, EXP on `drive` 0 → 100 % | One sweep moves every module along its own law, each by its weight (§2.5); saturators stay level-matched |
| **EXP: body to fizz** | Chord Clarity, EXP on `focus` −60 → +60 % | The dirt travels across the spectrum |
| **Set-up** | `<b>_solo` on each band in turn | Hear what each band does before mixing them |

Order in the chain (from the catalogue note): after the de-esser, before the compressor, for voice;
after the amp (or a cab) for drums and loops; before the amp for guitar if the amp is clean.

## 5. Bench suite (on top of gfx-bench's common checks)

**gfx-bench options**: `skip_latency` (minimum-phase group delay plus crossover dispersion; reported 0;
gfx-drive's precedent); `click_bound_db` −30 dB (a module change at high Amount creates real HF content,
gfx-crusher's bound); silence floor −140 dBFS (Tread rounding, the always-on DC block and Heavy's knee
make silence decay to 0). Bounds marked [P] come from the review's numpy prototype with an ideal
downsampler; the bench re-measures them with the real half-bands and then fixes them.

| Check | Pass |
|---|---|
| Clean sum | Every slot Off, gains 0 dB, `wet` 100 %, both slopes, both qualities, crossovers at defaults, at min spacing (2 %) and at both ends of their windows (2 / 4 / 6 % and 94 / 96 / 98 %): magnitude 20 Hz – 20 kHz flat ±0.1 dB at 2×, ±0.05 dB at 4× (droop correction residual, §2.1 [P]; the half-bands' own pass-band added); output nulls ≤ −60 dB against the reference = all-pass cascade A(x1)A(x2)A(x3) × the 5 Hz DC block × the measured response of the same engine with crossovers removed (half-bands, averages, correction) |
| Droop | The same with the correction switched off in a test build: −0.95 dB at 10 kHz and −4.0 dB at 20 kHz at 2× ±0.1 dB (proves the correction is doing the work, not the tolerance) |
| Coherence | One band on Soft clip 0 %, the others Off: the sum still flat ±0.1 dB at −40 dBFS (tanh linear there); `wet` 50 %, `<b>_mix` 50 %: no notch deeper than 0.2 dB anywhere. Same with Fuzz 0 % in one band: ≤ 0.6 dB at 20 kHz at 2× (the accepted half sample) |
| Crossover laws | x = 25 / 50 / 75 %: LR4 −6 dB points at 112.5 / 632.5 / 3,557 Hz ±1 %; ordering push at x2 < x1 |
| Prisma laws kept | Sample divide hold rate = 40 · 1000^(1 − v) Hz ±0.1 %; Ring mod frequency ±0.1 %; Lowpass / Highpass / Peak centre frequencies ±1 % |
| Conversions | The test holds Prisma's formulas, typed from §1.1: for each converted type, 101 values of v → v′ → our law gives Prisma's dB / bits within 0.01 (where v′ is not clamped); Zero cross: our −6 dB level within 3 dB of Prisma's curve for v in 0.1 – 0.95; Sine fold: same fold count (zero crossings of a ramp) for v ≥ 0.45; the four catalogue presets convert to the §3.3 table |
| Aliasing | 2 / 5 / 9 kHz sines at −6 dBFS in the band that holds them, 48 kHz, strongest aliased component below 20 kHz relative to the output fundamental [P, 4 dB of margin]. Soft / Hard / Tube at Amount 50 %: Off ≤ −55 / −48 / −33 dB, On ≤ −75 / −60 / −63 dB; at 100 %: Off ≤ −40 / −38 / −30 dB, On ≤ −55 / −46 / −44 dB. A 500 Hz sine in the low-mid band, Hard clip 100 %: Off ≤ −54 dB, On ≤ −80 dB. Every ADAA module: ≥ 15 dB better than the same curve without ADAA at the same factor (5 kHz, Amount 50 %). Heavy / Asym / Rectify / Zero cross / Fold / Sine fold / Fuzz: reported, then bounded from the first measurement. Bit crush and Sample divide: excess over their ideal model ≤ −70 dB (gfx-crusher's method, simplified). The draft's −80 / −90 dBFS at Amount 100 % was 40 – 50 dB beyond what 2× / 4× + ADAA1 can do |
| Level | Each compensated module (Zero cross up to T = −30 dBFS only), Amount 0 → 100 %: RMS within ±0.5 dB on the reference sine (the table itself), loudness within ±3 LU on the sine and on a guitar DI whose band peaks sit at the reference; `drive` 0 → 100 % on Megaphone within ±3 LU. Make-up never above +12 dB (Zero cross 100 %, Bit crush 1 bit: output silent, no gain on the noise) |
| Drive weights | `drive` 100 % on Grit, Telephone and Broken: output still above −40 dBFS RMS on the drum loop (no preset driven to silence); Sample divide moves 2 octaves, Bit crush 3 bits ±1 % |
| Heavy knee | Heavy 100 %, −90 dBFS white noise: output ≤ −60 dBFS (Prisma: ≈ −1 dBFS) |
| Drive / Focus | `drive` 0 and `focus` 0: bit-identical to the slots alone; a band of linear modules only: identical at any Drive / Focus |
| Intermodulation (report) | Power chord (E2 + B2) and an open E major chord: one band Soft clip 30 dB (crossovers at 94 / 96 / 98 %) vs four bands Soft clip 30 dB: energy at difference frequencies, reported; the four-band case expected ≥ 6 dB lower [I] |
| Crossover dip (report) | Sine sweep at −12 dBFS, all bands Soft clip 50 %: THD vs frequency, LR4 vs LR8; the dip at each crossover reported (expected narrower with LR8) |
| Clicks | Slot type changes under a held note, Cabinet voicing steps, crossover sweep by EXP 50 Hz steps, solo / bypass toggles and `<b>_gain` 0 → −60 dB with Rectify 100 % in the band (DC must not thump): HF burst ≤ −30 dB re peak; crossover EXP sidebands ≤ −80 dB (contract) |
| Quality and slope switches | Off ↔ On and 24 ↔ 48 under a held 82 Hz note: no gap, no burst above −30 dB re peak (pre-rolled engine, §2.1); reported latency 0 both ways |
| Stereo | 1 / 1: Width / Pan / Stereo modules leave the output unchanged; 2 / 2 with equal inputs: L = R bit-exact for the mono modules |
| Determinism | Ring mod and Sample divide phases from 0 on reset: two renders null |
| CPU | ≤ 320 ns / sample mono on the Grit preset (Off), ≤ 650 ns (On), worst case (8 nonlinear slots, LR8, On) ≤ 1.4 µs, 48 kHz |
| Listening | drums, bass DI (fingers, pick), guitar (clean.wav, crunch.wav into a clean amp), voice: the four catalogue presets against Prisma (golem today) and the §4 presets by intent; Bass Split blind against a parallel clean + drive patch built from gfx-drive |

## 6. Positioning

| Pedal | Its job | Difference |
|---|---|---|
| gfx-drive | One band, the pedal families (overdrive, distortion, fuzz, Legacy), `bass_keep` | gfx-grit's Soft / Hard / Tube / Fuzz modules use the same shapers per band; for one pedal-like drive, gfx-drive is better voiced (pre / post filters, slew, sag) |
| gfx-crusher | The exact virtual converter: BLEP-rendered staircase, formats, tails, tracked rate | gfx-grit's Bit crush and Sample divide are band-local seasoning with the same laws, oversampled but not exact. For the sampler sound, gfx-crusher |
| gfx-tape (lot 2, planned) | Tape saturation | Its engine becomes a module ("Tape", appended) when it exists |
| gfx-glitch | Spectral damage, frames, latency | Time-domain, zero latency |
| gfx-eq | Tone | Band gains here are a crossover's levels (24 / 48 dB / oct), not an EQ |
| Multiband comp (lot 3, planned) | Dynamics per band | Shares gfx-dsp `crossover` |

Against the field: Trash's two-modules-per-band structure, Roar's small per-stage surface and global
amount, Saturn's slopes and per-band level / mix, Parallax's clean-low bass move, Prisma's whole module
set with its presets. Beyond them for golem's use: antialiasing even at the lowest quality, level-matched
modules, laws that mean the same unit on every type, a phase-coherent per-band blend, and a four-knob
face that can be played with an expression pedal. Not attempted: per-band dynamics and feedback
(Saturn), per-band modulation (Roar, Saturn), linear phase, more than 4 bands or 2 slots (the catalogue's
size; enough for every use in §4).

## Visual

- **Category: `lofi`** (the catalogue's). Body hue: the family's — `METAL_HUE.lofi` `#e8336f`
  (raspberry), as proposed by gfx-crusher; **no crate hue**: the motif sets it apart from gfx-crusher, as
  gfx-freqshift is set apart in the pitch family.
- **Hue clash with gfx-swell, resolved at review (proposal for Raph)**. `#e8336f` and gfx-swell's crate
  hue `#d81e5b` are the same hue (340.1° and 340.3°). The gap they both chose, pitch's magenta (319°) to
  the thrash drive red (3°), is 44° wide: it holds one hue 20° from its neighbours, not two.
  - **lofi keeps raspberry**: a family hue carried by two pedals (gfx-crusher, gfx-grit; gfx-tape may
    join) outranks one crate's hue, and its thrash finish needs the distance from drive's red.
  - **gfx-swell moves to orchid `#bf1dd7`** (292°, saturation 76 % as the other metal hues): the middle of
    the next free gap, 27° from reverb's violet (265°) and from pitch's magenta (319°), more room than
    raspberry has. It stays anodized, with its `swells` motif. To note in gfx-swell's README (not edited
    here: `METAL_CRATE_HUE['gfx-swell']` and its face `color`).
  - Fallback if orchid reads as "pitch" at tile size: gfx-swell drops its crate hue for the dynamics
    slate and keeps a rose `RUNE_LIGHT`. Decided on the family sheet.
- **Finish: Thrash**, the lofi family's (`FAMILY_FINISH.lofi = 'thrash'`, proposed with gfx-crusher):
  gloss raspberry, the motif yellow lined in black.
- **Motif: `prism`** (new case in `motifParts`, drawn with `line()` so both finishes work), in the rune
  zone only, below the knob grid (the gfx-freqshift review's lesson):
  - Left of the rune: one beam, a straight line at y = cy from x = 6 to the cartouche's left edge − 2.
  - Right of the rune: four rays leaving the cartouche's right edge + 2 at y = cy, fanning to x = W − 6 at
    y = cy − 0.7 ch, cy − 0.23 ch, cy + 0.23 ch, cy + 0.7 ch. Each ray carries its band's dirt along its
    length: the top ray straight (the clean band), the second a sine with flattened crests (amplitude
    1.4, period 6: clipping), the third a triangle zigzag (amplitude 1.6, period 4: folding), the bottom one
    a staircase (step 3: crushing). Opacity 1, 0.85, 0.7, 0.55 from top to bottom so the fan reads as
    light dispersed by the prism.
  - Thrash: yellow strokes lined in black; anodized (if forced): engraved.
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-grit": {"name": "Grit", "blocks": ["grit"], "knobs": ["drive", "focus", "wet", "level"],
               "toggles": ["quality"], "category": "lofi"}
  ```
- **Glyph (decided): keep golem's existing `grit` glyph** (`visuals/glyphs/grit.svg`: a frame cut into
  three bands, each with a zigzag of its own pitch). It already says "bands, each with its dirt", in the
  style of `glitch.svg`, and it is the link between the catalogue tile and the pedal.
- No maker's livery, logo or trademark on the pedal; no IR or cab brand names in Cabinet's voicing names.

## Open questions for review

Each answered at the review of 2026-11-14.

1. **Units and conversion** (§3.2). **Decided: band gains in dB, module values converted once, keyed by
   the slot's type**; three conversions were corrected (Zero cross, Sine fold, Stereo DC's scale).
   Whether golem-rack's engine table can key a conversion on another parameter stays its call; the
   fallback (Prisma's laws kept for the eight types) is written out.
2. **Peak**. **Decided: `<b>_y1/_y2` kept (signed, −30 – +30 dB), default +9 dB.** A constant default
   breaks no rule, a Peak is heard when chosen, and Megaphone gets the peak its note describes. A fixed
   gain would lose the cut (a scoop before a clipper).
3. **Band gain on the whole band**. **Accepted**: identical on the four presets, and a muted band that
   stays muted at any `wet` is what a player expects.
4. **Crossover dispersion**. **Accepted, no linear-phase mode.** 4.9 ms at DC (5.7 ms peak at 73 Hz) is
   below what a bass or a kick shows as a softened attack through a cab [I, bench]. The one use that
   needs care is a golem split lane summing gfx-grit against a dry branch: it combs at the crossovers
   whatever the settings. Catalogue note: "in a split, put gfx-grit on the summed lane or use `wet`".
5. **Slope default**. **24 dB / oct**: Prisma's, half the dispersion; 48 is the bass-split presets'.
6. **Quality Off = 2×**. **Kept, never 1× per band**: one rate for the whole split is what keeps the
   bands coherent, and the split already protects the low bands (§2.1).
7. **Cabinet**. **Biquad voicings confirmed**, in the order of Prisma's roles; no IR of Prisma is used
   (§2.4, licence hygiene). The slot may call the lot-3 cab engine later, under the same six names.
8. **Drive / Focus**. **Additive points kept, with a weight per module family** (§2.5): one weight
   drove three of the four catalogue presets to silence or to a 240 Hz hold.
9. **Hue**. **lofi keeps raspberry; gfx-swell is proposed orchid `#bf1dd7`** (Visual). Raph decides on
   the family sheet.
10. **Tube / Fuzz now, Tape later**. **Agreed.** The list need not be closed before release, but every
    append is a joint plugin + catalogue change (§2.4).
11. **Static compensation always on**. **Yes, no toggle**: band gains and `level` do the rest. Limited
    to −30 – +12 dB; Bit crush and Sample divide are left alone (§2.6).

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good**, Strong once the bench confirms the prototype's figures. The architecture is right
and holds under the numbers: the all-pass-compensated Linkwitz-Riley tree sums flat and equiphase for
LR4 and LR8 (prototype: 1e-6 dB, 1e-15 rad, analog and TPT, at every crossover placement), the 4.9 ms
is exact, one oversampling stage around the split is the correct way to keep bands coherent, and the
per-type unit work is the right reading of the 2026-11-14 rule. Prisma's source was re-read at
`5062b59`: the tree, the crossover law, the band-gain and wet laws, and the Soft / Hard / Heavy / Asym /
Rectify / Fold / Bit crush / Sample divide / DC / Gain / Ring mod laws are as the survey says, and the
§3.3 table reproduces. The design had one mechanism that defeated its own bench (the alignment's
droop), antialiasing bounds 40 – 50 dB out of reach, three wrong conversions, and two rules (Drive,
compensation) that broke on the gating modules. None touches the structure. Prototype scripts:
`scratchpad/proto-gfx-grit/` (`xo.py`, `adaa.py`, `zc.py`, `zcfit.py`).

Changes made, and why:

| # | Change | Why |
|---|---|---|
| 1 | **Droop correction after the band sum** (§2.1), bench rows Clean sum / Droop | `HalfSample` is a two-tap average, as ADAA1 is at small signals: two per path cost 4.0 dB at 20 kHz at 2× (0.95 dB at 10 kHz) on every band, every slot Off. The "flat ±0.02 dB" row could not pass. The droop is common to all paths, so one two-pole filter (a = 0.8) restores ±0.06 dB |
| 2 | **Alignment put in proportion; Fuzz's extra half sample accepted** | A half-sample mismatch costs 0.5 dB at 20 kHz in a 50 / 50 blend at 2×. Fuzz has two ADAA stages and cannot fit a one-average slot |
| 3 | **Antialiasing stated with figures; bench bounds reset** (§2.1, §5) | −80 / −90 dBFS at Amount 100 % is not reachable: a 9 kHz sine clipped by 48 dB aliases at −35 dB (2× + ADAA1) and −50 dB (4×). ADAA1 buys ≈ 20 dB over naive; the split protects the low bands. Upgrade path named (ADAA2 or 8×) |
| 4 | **"The blend never combs" → "no latency comb"** (§2.3) | True for the memoryless saturators only: filters, Cabinet, Sample divide (half a hold period) and folders (sign of the fundamental) shift the wet against the dry. That is their sound, but the claim was wrong |
| 5 | **Zero cross: law −66 – −6 dBFS, conversion by table** (§1.1, §2.4, §3.2) | Prisma's audible dead zone is its power term, not the v¹⁰ clip the draft converted: Prisma's 23 % became −90 dBFS (nothing). Its −6 dB point runs −40 dBFS at 10 % to −5 at 90 % |
| 6 | **Sine fold conversion: 70 v² + 2, with the fade-in factor** | sin(π g x) folds twice as fast as sin(π/2 · g x): the draft was 6 dB short |
| 7 | **Stereo DC scale**: ±0.5 at 100 % | The draft's 0.5 · v · (c − ½) was half Prisma's v⁵ · (c − ½) under v′ = v⁵ |
| 8 | **Drive / Focus weights per family** (§2.5), bench row | 50 points for all: Bit crush 8 → 0.5 bits, Sample divide −5 octaves, Zero cross +30 dB. Now +24 dB / −3 bits / −2 octaves / +12 dB |
| 9 | **Compensation**: limited to −30 – +12 dB, Bit crush and Sample divide uncompensated, Zero cross capped; behaviour table; reference level made a bench constant | At −18 dBFS a Tread quantiser below 3 bits and a dead zone above −18 dBFS output silence: the table divided by zero. "Louder playing is still louder" is false above ≈ 24 dB of drive |
| 10 | **Sample divide: interpolated read, area-sampled edge** | A naive hold at the inner rate jitters its edges by up to one inner sample (inharmonic grit that is not the effect) and has no half-sample delay; the area mean is ADAA1 of the staircase, at the cost of one multiply |
| 11 | **DC block kept on the wet, reason and cost stated; bench reference includes it** | Before the mix and gain it prevents thumps on ramps; the all-Off null against the bare all-pass would have failed at 40 Hz (10° of lead) |
| 12 | **`quality` / `slope`: pre-rolled engine, then crossfade** | A cold engine crossfaded in 10 – 20 ms lets a 112 Hz crossover settle under the fade |
| 13 | **Crossover windows** (2 – 94 / 4 – 96 / 6 – 98 %) taken from Prisma's `calccross` | Without them x1 = 100 % leaves no room for x2, x3; bench rows that put "all crossovers at 20 kHz" corrected |
| 14 | **Peak gain default +9 dB**; **per-band mute dropped**: 54 → 50 parameters | Open question 2; `<b>_gain` −60 dB is a mute, and parameters can be added later, never removed |
| 15 | **Cabinet voicings ordered by Prisma's roles** (1 = bass, 3 = the small one) | A saved chain keeps the kind of cabinet it had |
| 16 | **gfx-dsp sharing settled** (§2.8): one `crossover` in three layers with gfx-bassdi; gfx-drive's shapers into `adaa`; gfx-crusher: laws only | Two designs proposed a `crossover` module with different contents; gfx-crusher's engine (BLEP converter) is not what a band-local module needs |
| 17 | **Enum appends need the catalogue's option lists in step** (§2.4, §3.1) | golem writes enums as index / (n − 1) from its own list: 23 options against the plugin's 25 would select the wrong module |
| 18 | **Licence hygiene paragraph** (§2.4) | AGPL-3 source read for its laws only; nothing of Prisma (code, tables, make-up factors, the six bundled IRs) is reused. Checked: the design asks for none of it |
| 19 | Facts: Tight kick read from Prisma's source and re-authored (the draft's −52 dBFS dead zone does nothing to a kick); dispersion peaks and LR8 figures; SVF tick counts; LR8 "twice" → 1.85 × at DC, 2.2 × at the peak; Megaphone converted with its peak; Gain's clamp; dangling §7.7 reference | Accuracy |
| 20 | **Visual**: hue clash resolved by proposal (lofi raspberry stays, gfx-swell → orchid `#bf1dd7`) | Both sit at 340°; the gap holds one hue |

Still open:

| Point | Owner |
|---|---|
| Type-keyed conversion of `*_v1/_v2` in the engine table (else the §3.2 fallback); `*_gain` in dB; option lists at 25; the note's "Keep Quality OFF live" removed; count of saved chains using `grit` | golem-rack |
| Showing the plugin's formatted value (dB, bits, Hz) for `*_v*`; EXP on `drive` / `focus` (non-catalogue ids) | golem-rack |
| gfx-swell's hue (`#d81e5b` → `#bf1dd7`), to carry into gfx-swell's README if accepted; raspberry against drive's thrash red at tile size | Raph, on the family sheet |
| Which of gfx-bassdi / gfx-grit creates gfx-dsp `crossover` layer (1); moving gfx-drive's shapers without changing its sound | golem-fx, at coding order |
| Aliasing bounds with the real half-bands; fizz on a driven top band → ADAA2 or 8× | Bench |
| Reference level of the compensation; Drive weights; loudness of a square against a sine (+2 – 3 LU expected) | Bench, by ear |
| Cabinet voicings, Tight Kick, the Zero cross knee; audibility of 4.9 ms (LR4) and 9 – 12.6 ms (LR8) of low-frequency dispersion on bass and kick | Bench, by ear |
| Meaning of Prisma's IR names (roles inferred [I]); US 7206419 read only as an abstract [I] | Not blocking |
