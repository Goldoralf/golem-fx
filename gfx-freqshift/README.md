# gfx-freqshift

> **Status: CODED** (v0.1.0, 2026-11-15). Designed by Claude Opus 5.5 and reviewed by Fable 5.2
> (2026-11-14: §7 holds the decisions, the last section the review), then coded as designed, with
> the new gfx-dsp module `ssb` (the per-rate Hilbert pair with the exact nome, the complex shifter, the
> second pass as the negative-frequency filter).
>
> - **Measured**: the pair's image is −86.8 dB at 48 kHz, −87.8 dB at 44.1 kHz, −99.7 dB at 96 kHz and
>   −91.6 dB at 192 kHz, within 0.1 dB of the design. On the plugin, the worst image over the bench's
>   grid (30 Hz – 15 kHz, ±5 to ±1,000 Hz, `zero_cut` on and off) is −86.9 dB. The zero cut leaves the
>   50 Hz reflection of 100 Hz − 150 Hz at −87 dB. The `dry_match` null is −81.8 dB.
> - **Tests**: 19 behaviour tests and 9 `ssb` unit tests pass. The bench is green (28 / 28, with one
>   documented exemption: the step to `fbk` 110 %, §8).
> - **CPU**: 136 ns per stereo frame at default settings with a mono source (the right channel is
>   computed once). About 230 ns with true stereo input.
> - **Open**: listening on the DIs (dispersion, phaser corners, `sat` knee); golem-rack's §3.4
>   conversion. The deviations from the design are in §8.

golem's **frequency shifter**: a single-sideband (Bode) shifter with a feedback loop, LFO and
envelope. It moves every partial by the same number of hertz, so the harmonics stop being harmonic.
- **A few tenths of a hertz to 5 Hz**, mixed with the dry: endless phasing and a sweet, metallic detune
  (the barber-pole).
- **A few to a few hundred hertz**: bells, metal, clangour.
- **With feedback**: spirals that climb or fall forever (barber-pole phaser or flanger), and drifting
  echoes.

It stands for golem's `surge-freqshift` block (Surge XT Effects, FX type 7, category `pitch`). It keeps
that block's ids `shift`, `shift_r`, `time`, `fbk` and `mix` ([golem contract](../docs/golem-contract.md)).

## 1. State of the art

Survey of 2026-11-14. **[V]** read in source code, a paper or a manual · **[M]** manufacturer page or
listing · **[F]** forum post or review · **[A]** computed here (scripts in the review notes, numbers
reproducible) · **[I]** inference, or not re-checked at the source.

### The block we replace: Surge XT Frequency Shifter [V]

Read in `src/common/dsp/effects/FrequencyShifterEffect.cpp` and `Parameter.cpp` (surge-synthesizer/surge,
GPL-3).

- **Algorithm: Weaver, not Hilbert.**
  1. The input is multiplied by a quadrature oscillator at fs/4 (`o1`, rate π/2 − min(0, ω)).
  2. Both products go through a steep half-band low-pass (`HalfRateFilter(6, true)`).
  3. They are multiplied by a second quadrature oscillator at π/2 + max(0, ω) (`o2`).
  4. Out = 2 · (re + im).

  The half-band's transition sits at fs/4, which maps to **0 Hz and Nyquist** of the input, so the image
  is rejected least there. [I] (no measurement published.)
- **Loop.** `buffer = in + softclip(fbk_gain · wet)`, then `wet = shift(read(buffer, time))`, read with a
  12-tap windowed sinc.
  - The wet is **always delayed** by `time`, even at feedback 0.
  - The minimum is 2^−8 s = 3.9 ms; the line holds 2^18 samples (5.46 s at 48 kHz).
- **Parameters** (`ct_*` types). golem writes each one normalized, n = (v − min) / (max − min):

  | Surge param | Type | Plain range | DSP law |
  |---|---|---|---|
  | Left (`shift`) | `ct_freq_shift` | −10 … +10 | Hz = v · 10 → **±100 Hz** (±10 kHz with "extend range", off by default) |
  | Right (`shift_r`) | `ct_percent_bipolar` | −1 … +1 | **R shift = L shift × v**: a ratio, not a second shift. At exactly v = 1 the right oscillators slew onto the left ones (one-pole, 1 % per block). **Surge's own default is v = 1** (n = 1.0), not the catalogue's 0.5 |
  | Time (`time`) | `ct_envtime` | −8 … +5 | seconds = 2^v (3.9 ms … 32 s, clamped by the line) |
  | Feedback (`fbk`) | `ct_percent` | 0 … 1 | gain = `amp_to_linear(v)` = **v³** |
  | Mix | `ct_percent` | 0 … 1 | Surge's own mix stays at 1.0. golem's `mix` is **REAPER's per-FX Wet knob** (`test_surge_desktop.py`: "le Mix du catalogue = le Wet de REAPER") |

- **What it lacks**: LFO, envelope, sideband choice, stereo phase offset, loop filters, any handling of
  partials that cross 0 Hz or Nyquist, and zero-delay operation.

### Hardware and plug-ins

| Reference | What it does best | What we take |
|---|---|---|
| Harald Bode / Moog 1630 (and the 735 rack, identical) [V] (Bode & Moog, AES 1972, specs quoted by ModularGrid and the Behringer 1630 reissue) | Analog Hilbert networks. ±5 kHz shift, **both sidebands at once** (Up and Down outputs, then a mix), 40 dB rejection of the unwanted sideband, 30 Hz – 16 kHz, a squelch against carrier bleed | Both sidebands and a balance between them (`sideband`); the ±5 kHz range is wider than any guitar use (we stop at ±2 kHz, §3) |
| Bode feedback "barber-pole" (Sean Costello on the Eventide forum) [F] | Classic shifter feedback runs through **12th-order IIR Hilbert networks with a lot of phase lag**: that dispersion makes a barber-pole **phaser**. Swap the all-pass chain for a short delay and it becomes a barber-pole **flanger**. Costello's recipe: 6 or more first-order all-passes, coefficients near the unit circle, the shifter, one output fed back | `loop` = Phaser (a dispersive chain in the loop) or Delay; `time` 0 – 10 ms = flanger spiral |
| Valhalla FreqEcho [M] | A shifter in an analog-style delay loop; low-cut and high-cut in the loop; tempo sync; small shifts = chorus / double-tracking, larger = barber-pole and runaway echoes | `time` up to 2 s with `sync`, loop `low_cut` / `high_cut`, feedback above 100 % bounded |
| Ableton Live 12 Shifter [V] (manual) | Freq and Ring modes; **Wide**: the right channel shifts the opposite way; an LFO (10 shapes, L/R phase, spin) and an **envelope follower** in Hz; delay with feedback and a Tone filter in the loop; "for phasing, keep Fine to about 2 Hz or less" | Opposite stereo shifts (`shift_r` −100 %), `stereo_phase`, `env_depth` in Hz |
| Fractal Ring Modulator SSB modes, EHX Ring Thing [V] (see gfx-ringmod) | SSB Upper / Lower on a pedal | Covered by gfx-ringmod; we share the `ssb` module |
| u-he Uhbik-S, Sinevibes Whirl, SPC Freek [M] | Studio shifters: "barber-pole waves to metallic", Bode-model barber-pole phaser with progressive feedback | Confirms the use set |
| Forums and manuals (Cherry Audio, Image-Line, Ableton) [F] | Loved: small shifts for phasing (especially with regeneration up); "endless glissando". Disliked: harmonics lost beyond small shifts, mud when shifting down, carrier bleed, zipper on fast sweeps, the image (an unwanted second partial that beats) | Image ≤ −80 dB; `zero_cut` against mud; a continuous-phase oscillator (no zipper by construction) |

### Hilbert transformer designs [V, A]

| Design | Latency | Image rejection | Verdict |
|---|---|---|---|
| FIR Hilbert | N / 2 samples; reaching 15 Hz needs thousands of taps | Arbitrary | Rejected: latency. Costello also notes that FIR shifters lack the "phase wiggles" the feedback sound needs |
| Weaver (Surge) | 0 (IIR half-band) | Weakest near 0 Hz and Nyquist | Rejected: no gain over a Hilbert pair, two oscillators |
| Niemitalo 4 + 4 all-pass pair, coefficients found by evolutionary search; band = 0.998 · Nyquist, so **in proportion to fs** | 0 | 90° ± 0.7° → **−44 dB** at 44.1 kHz. The band's bottom edge scales with fs: at 96 kHz the error reaches 1.3° at 41 Hz (−39 dB) and 20° at 20 Hz (−15 dB) [A] | Good but fixed to 44.1 kHz |
| Elliptic polyphase IIR half-band (Valenzuela–Constantinides; closed form as in de Soras's HIIR), rotated by fs/4 with z → jz: the two half-band paths become a Hilbert pair, and the half-band's stopband attenuation becomes the image rejection | 0 | Designed **per rate for a band in Hz**: 15 Hz – (fs/2 − 15 Hz), equiripple. With the **exact elliptic nome** (§2.2): 8 + 8 sections give **−86.8 dB** at 48 kHz, −87.8 dB at 44.1 kHz, −79 dB at 88.2 / 96 kHz, −73 dB at 176.4 / 192 kHz; 10 + 10 give −100 dB at 88.2 / 96 kHz and −92 dB at 176.4 / 192 kHz. With HIIR's truncated nome series the same 8 + 8 reach only −82 / −71 / −60 dB, and more sections do not help [A, recomputed at review] | **Chosen**: exact nome, 8 + 8 up to 50 kHz, 10 + 10 above |

Through-zero shifts need no special engine: the analytic signal times e^{jφ(t)} with a **signed**
frequency and a continuous phase crosses 0 Hz seamlessly. What crosses zero is the **partials** (a
150 Hz partial shifted by −200 Hz reflects to 50 Hz); that is the "through-zero" growl, and the mud
players complain about.

### What we conclude

1. **A per-rate, closed-form IIR Hilbert pair, 8 + 8 sections (10 + 10 above 50 kHz), f64**: zero
   latency and an image ≥ 40 dB better than the Bode's 40 dB and Niemitalo's 44 dB. The band bottom is
   fixed in Hz (15 Hz), so a 5-string bass's B (31 Hz) shifts cleanly at every rate.
2. **Every edge case handled in the wet path**:
   - a 20 Hz high-pass, so DC and subsonic thumps do not become tones at |shift|;
   - **a second pass through the same pair after the shift** (`zero_cut`): it removes whatever landed
     on a negative frequency, which is both the partials that crossed 0 Hz and those that wrapped past
     Nyquist. −87 dB, a 15 Hz transition, no tracking filter (§2.2).
3. **Surge's loop topology and ids, Bode's sideband balance, Costello's two barber-poles** (dispersive
   or delay loop), FreqEcho's loop filters, Ableton's stereo opposite shifts, LFO and envelope in Hz.
4. **`time` 0 means a classic Bode shifter** with an undelayed wet. Surge's values convert exactly (§3.4).

## 2. Design

### 2.1 Signal path (per channel)

```
in ─┬──────────────────── dry (optionally HP 20 Hz + path A, twice with `zero_cut`: `dry_match`) ─────────────────────┐
    │                                                                                                               ▼
    └─▶ (+) ─▶ buf ─▶ read(time) ─▶ HP 20 Hz ─▶ [Phaser chain] ─▶ Hilbert ─▶ × e^{jφ} ─▶ zero cut (or anti-wrap LP) ─▶ wet ─▶ mix ─▶ out
         ▲                                                                                             │
         └──── sat ◀─ × fbk ◀─ high_cut ◀─ low_cut ◀──────────────── (one sample) ◀────────────────────┘
   shift = shift_c + lfo_depth · LFO_c + env_depth · ENV        (c = L or R; R × shift_r)
```

- **Loop.** buf[n] = x[n] + sat(g · F(wet[n − 1])), then wet[n] = SSB(P(read(buf, time)[n])).
  - g = `fbk` / 100 (linear gain) and F = loop `low_cut` / `high_cut`.
  - P = the 20 Hz high-pass, plus the Phaser chain when `loop` = Phaser.
  - `time` 0 reads the sample just written: the wet is **not delayed**, and the loop delay is one sample
    plus the networks' dispersion.
- **Delay line**: gfx-dsp `delay`; it glides over 100 ms when `time` moves (Doppler on the wet, as in
  Surge). Sized for 2,000 ms at the highest rate.
  - Hermite read from one sample of delay up. A 4-point read needs a sample ahead of the tap, so below
    one sample (only during a glide to or from 0) the read is linear between x[n] and x[n − 1], and at
    0 it is the sample itself.
- **`sat`**: a soft clip on the fed-back signal, transparent below −6 dBFS, ceiling 0 dBFS. It bounds
  the **return**, so buf ≤ |x| + 1; the wet can still peak above that through the all-pass networks.
  Feedback up to 110 % stays bounded (gfx-delay's rule).
- **Why the loop drains.** Each pass moves the energy by `shift`; with `zero_cut` on it leaves the band
  at one end or the other, so even 100 % decays. It self-sustains only at shift ≈ 0 (a plain comb), or
  with `zero_cut` off on a downward shift: a partial below |shift| then bounces between f and
  |shift| − f for ever (the "mud", now on purpose).
- **`time_sync`** (contract list) sets `time` from the host tempo, as a duration; Off = `time` in ms.

### 2.2 The SSB core: new gfx-dsp module `ssb`

- **Hilbert pair.**
  - Each section is the one-multiply z² all-pass y = c · (x + y[n−2]) − x[n−2], kept from `pitch.rs`;
    8 sections per path up to 50 kHz, 10 above, states in f64.
  - The coefficients come from the closed-form elliptic half-band design (de Soras's HIIR formulas) for
    the transition width 2 · 15 Hz / fs, with N = 16 (or 20) coefficients sorted ascending.
    - **The nome is computed exactly**: q = exp(−π · K(k′) / K(k)), with K by the arithmetic-geometric
      mean (five steps in f64), and the theta series summed until the terms vanish. HIIR's short
      polynomial for q is accurate for the wide transitions of an oversampler, not for 30 Hz in
      48 kHz (q ≈ 0.27 – 0.33): it costs 4 dB at 48 kHz, 8 dB at 96 kHz and 12 dB at 192 kHz, and
      adding sections does not win them back [A].
    - Rejection = 6.02 + 5 · (2N + 1) · log₁₀ q dB: −86.8 dB at 48 kHz (N = 16), −99.7 dB at 96 kHz and
      −91.6 dB at 192 kHz (N = 20). The simulated pair matches the formula within 0.1 dB [A].
    - Computed in `prepare` for the actual rate: closed form, microseconds, no allocation. No embedded
      tables (a test pins the 48 kHz coefficients).
  - **Which path takes the extra one-sample delay**: the **odd-ranked** coefficients (2nd, 4th… of the
    ascending list) form path B, and **B is the delayed one**. That is the path whose first coefficient
    is the larger (0.186 against 0.051 at 48 kHz). Path A (un-delayed) is the in-phase output.
- **Bug in the coded pair, confirmed at review** [A]. gfx-dsp `pitch::Hilbert` and `pitch::FreqShifter`
  delay the wrong path: Niemitalo delays the cascade that starts with 0.6923878² = 0.479; the code
  delays the one that starts with 0.4021921² = 0.162. Computed from their coefficients at 48 kHz, the
  image is:
  - −37 dB at 82 Hz;
  - **−18 dB at 1 kHz**;
  - −2 dB at 5 kHz (the "shifter" is a ring modulator there);
  - and from 6 kHz to 18 kHz the shift goes the **wrong way** (at 15 kHz the wanted sideband is 8 dB
    below the image).
  - With the delay moved to the other path, the same coefficients give −44 to −63 dB from 30 Hz to
    20 kHz, and +Hz still goes up with the code's `i · cos + q · sin`.
  - gfx-glitch's private copy delays the right path, matching its "> 35 dB" test.
  - The unit test `freq_shifter_moves_by_hertz` misses the bug: it counts zero crossings (the wanted
    sideband still dominates at 1 kHz), and its 1,600-sample peak windows each hold two whole 60 Hz
    beat cycles, so the peaks never differ.
  - Users of the faulty code: gfx-delay `loop_shift`, gfx-octaver (via `pitch::Hilbert`), and the
    gfx-ringmod design (Upper / Lower). **Decision (§7.1): fix `pitch.rs` in place now**, a separate
    change with an FFT image test; the users then move to `ssb` when it lands.
- **Oscillator.**
  - An f64 phase accumulator φ += shift / fs, wrapped, then sin / cos. The phase is continuous, so a
    shift change, a modulated sweep or a through-zero crossing never steps the waveform (no zipper).
  - The shifted signal is kept **complex**: w = (I + jQ) · e^{jφ}. Without `zero_cut` the wet is Re w =
    I · cos φ − Q · sin φ. The sign convention is fixed by the bench: +shift must go up.
  - **Parked phase.** When a channel's commanded shift is exactly 0 (base at 0 and both depths at 0, or
    `shift_r` at 0 for the right), φ slews to 0 by the shortest way at 1 Hz at most. A parked shifter is
    then always the same filter, whatever was played before. Left frozen, φ could sit at 180° and the
    50 % mix would cancel.
- **Sideband balance** (`sideband`, −100 … +100 %), b = `sideband` / 100:
  - w = (I + jQ) · (cos φ + j · b · sin φ), which is ((1 + b) / 2) · the signed shift +
    ((1 − b) / 2) · its mirror, whatever the sign of the shift.
  - **+100 % = the signed shift alone** (the default).
  - **0 % = both sidebands** = I · cos φ: a carrier-free ring modulator, each sideband at −6 dB.
  - −100 % = the mirror sideband. This is the Bode's Up / Down mix.
- **Input high-pass**: a 2nd-order Butterworth at 20 Hz before the pair (−0.7 dB at a bass B, −24 dB at
  5 Hz). A shifter turns whatever sits near 0 Hz into a tone at |shift|: DC, but also a pick thump or a
  hand on the strings. A one-pole at 5 Hz would let those through.
- **Zero cut** (`zero_cut`, on by default): **the complex signal w goes through the same pair again,
  used as a filter**: wet = ½ · (A · Re w − z⁻¹B · Im w).
  - Rotated by fs/4, the half-band passes the positive frequencies and stops the negative ones. A
    partial shifted below 0 Hz **is** a negative frequency in w; so is one shifted past Nyquist, which
    wraps to just above −fs/2. One static filter removes both, at the pair's own rejection.
  - Measured on the 48 kHz design [A]: a 100 Hz sine shifted −150 Hz leaves its 50 Hz reflection at
    **−87 dB**; 23 kHz shifted +2 kHz leaves its wrap at −97 dB; partials that land on the right side
    pass at 0.0 dB, down to 20 Hz. Within ±15 Hz of 0 the cut is gradual (−6 dB at 0 Hz).
  - Nothing tracks the shift, so nothing is modulated: it is exact under any LFO, envelope or EXP move.
  - Rejected: the first design's 4th-order high-pass tracking |shift| + 20 Hz. It leaves that same
    50 Hz reflection at −18 dB (the bench asked −60 dB), and its cutoff must be recomputed per sample.
  - Cost: the wet crosses the pair twice, so its dispersion doubles (§2.4). To be listened on the bass
    DI; a 6 + 6 pair for this second pass (−64 dB, 25 % less delay) is the fallback.
  - Off: partials reflect through zero, the analog Bode's growl. The second pass is crossfaded out
    (20 ms) and the anti-wrap low-pass takes its place.
- **Anti-wrap** (only with `zero_cut` off): a 4th-order low-pass (two gfx-dsp TPT SVF sections) at
  min(20 kHz, 0.5 · fs − s_up − 1 kHz), with s_up the largest upward shift of either active sideband,
  followed per sample with a fast tan approximation (error < 10⁻⁴).
  - What it prevents: in one pass, a wrap lands above fs/2 − s_up, which is ultrasonic for the shifts
    of this pedal. In the loop that energy is trapped (each pass folds it back into the same top band)
    and builds up; a downward modulation then walks it into the audible range.
  - At 48 kHz and +2 kHz it cuts at 20 kHz, so nothing audible is lost. No oversampling and no latency.
- **Phaser chain** (`loop` = Phaser): 12 first-order all-passes, corners log-spaced from 100 Hz to
  3.2 kHz, fixed. The corners are a starting point, to be voiced by ear.
  - The pair is already Costello's dispersive network: 16π of phase over the band (32π with
    `zero_cut`), about 0.75 notch per octave (1.5). So `loop` = Delay at `time` 0 **is a barber-pole
    phaser** with wide-spaced notches.
  - The chain adds 12π between 100 Hz and 3.2 kHz, 1.2 notch per octave more where a guitar lives: the
    dense, vocal spiral. `time` 1 – 10 ms turns either into a barber-pole flanger.
- **Dry match** (`dry_match`, off by default): the dry goes through its own copy of the 20 Hz high-pass
  and of path A (twice when `zero_cut` is on): what the wet crosses at shift 0.
  - Off: the dry is bit-exact. At a small shift and `time` 0, each partial beats against the dry with a
    phase that varies with frequency (path A's dispersion), so the notches spread across the spectrum
    and move: **phasing**.
  - On: every partial beats in phase, so the whole sound swells and fades at the shift rate:
    **tremolo**. At shift 0, `time` 0, `fbk` 0 the wet equals the matched dry within the pair's
    quadrature error: a **−83 dB null from 15 Hz to Nyquist** with `zero_cut` on [A].
  - With `zero_cut` off, the anti-wrap low-pass stays in the wet only: its phase (3° at 1 kHz, 14° at
    5 kHz, 31° at 10 kHz at 48 kHz) makes the highs beat slightly late. Tremolo still, no exact null.

### 2.3 Stereo, modulation

- **Layout: 2 / 2 first, 1 / 1 offered.**
  - The block is stereo by nature: Surge's block is stereo, `shift_r` is an L/R control, and opposite
    shifts are a core use. Under the contract, golem turns the lane stereo here: before an amp, every
    block after it then runs in stereo. gfx-ringmod (mono first) is the pedal for clang before the amp.
  - On 1 / 1, only the left path runs (`shift_r` and `stereo_phase` are unused).
  - When the right is locked to the left (below) and the two inputs are equal (a mono DI on a stereo
    lane), the left is computed once and copied.
- **Right channel**: s_R = (`shift_r` / 100) · (`shift` + `lfo_depth` · LFO_R + `env_depth` · ENV),
  with LFO_R = the LFO at + `stereo_phase`.
  - At exactly +100 % with `stereo_phase` 0, the right oscillator **phase-locks** to the left: its
    phase slews onto the left's by the shortest way in 50 ms, then is set equal (Surge slews without
    end). From then on L = R bit for bit on equal inputs, and the mono sum never cancels.
  - At −100 % the right shifts the opposite way (Ableton's Wide). Summed to mono, that is both
    sidebands: a ring modulator. Stated in the Applications.
- **LFO** (gfx-dsp `lfo`):
  - `lfo_shape` uses gfx-glitch's list and order (Sine · Triangle · Saw Up · Saw Down · Square · Random
    Steps · Random Smooth), with a 3 ms smoother on the output.
  - `rate` 0.01 – 20 Hz, or `sync` (the contract's list, phase from the host position). The shared id
    `sync` keeps its shared meaning, the LFO's, as in every coded effect; the delay time gets its own
    id, `time_sync`.
  - Additive in Hz, so the shift may cross zero.
  - Deterministic: phase 0 and a fixed seed on reset.
- **Envelope**: the contract's level-independent follower (`env_sens`, `env_att`, `env_rel`) on the
  input, scaled by `env_depth` in Hz (signed). The pick drives the shift up or down.
- **Smoothing.** `shift`, `lfo_depth`, `env_depth`, `fbk` and `mix` take the contract's EXP smoother
  (gfx-dsp `exp_smooth`). Since the shift only sets a frequency, the phase integrates any step without a
  click. `sideband`, `dry_match`, `zero_cut` and `loop` crossfade over 20 ms.

### 2.4 Latency, CPU

- **Latency: zero, reported 0.**
  - The dry is never delayed.
  - The wet carries the Hilbert pair's dispersion, which any zero-latency shifter has: group delay
    10.8 ms at 41 Hz, 5.9 ms at 82 Hz, 0.54 ms at 1 kHz, 0.12 ms at 5 kHz (48 kHz design [A],
    confirmed at review).
  - **With `zero_cut` on (the default) the wet crosses the pair twice: 21.7 ms at 41 Hz, 11.9 ms at
    82 Hz, 1.1 ms at 1 kHz.** Mixed with the dry it is the phasing itself; at `mix` 100 % the lows
    arrive after the highs. Analog shifters do the same; the amount is to be judged on the bass DI.
  - It is the same in both paths, so the quadrature is unaffected.
- **CPU estimate** per stereo frame at 48 kHz:

  | Part | Cost |
  |---|---|
  | Hilbert pair, 2 × 16 sections, f64 (1 mul + 2 add each) | ≈ 35 ns |
  | Second pass (`zero_cut`), 2 × 16 sections; or the anti-wrap SVFs with per-sample coefficients | ≈ 35 ns |
  | sin / cos | ≈ 25 ns |
  | Delay read, input HP, loop filters, sat, LFO, envelope, smoothers | ≈ 45 ns |
  | Phaser chain (when on) | ≈ 15 ns |
  | `dry_match` (when on): HP + 16 or 32 sections | ≈ 20 – 35 ns |

  Total **≈ 155 – 190 ns per stereo frame, 0.7 – 0.9 % of a core**; half of it when L = R is copied.
  Budget for the bench: 300 ns.

## 3. Parameters

### 3.1 Ids and units: decision

- **The five catalogue ids are kept as nih-plug ids**, with real units: Hz, %, ms. golem converts the
  stored normalized values **once** with §3.4 (presets and rig chains), as for gfx-crusher.
  - **This follows the contract's rule for third-party 0 – 1 scales** (added 2026-11-14: keep 0 – 1
    when its law is linear in a real unit, as gfx-ringmod does; convert when it is not). The reason: Surge's scale cannot hold this pedal. `shift` 0 – 1 is ±100 Hz, `time`
    cannot reach 0 (the classic shifter), `fbk` is a cube that stops at 100 %. A value stored for
    Surge and one stored for gfx-freqshift therefore do not mean the same thing: golem-rack must agree
    to the one-time conversion (see Review).
- The **display names keep the catalogue labels** (`Shift L`, `Shift R`, `Time`, `Feedback`, `Mix`):
  golem joins on names (descriptor rules).
- `shift_r` **keeps Surge's meaning**: a ratio of the left shift, −100 … +100 %.
  - Why not a second shift in Hz: one knob then moves the stereo image from "same" through "right dry"
    to "opposite", and EXP or the LFO on `shift` moves both sides.
  - The plugin's default is +100 %, as Surge's own (R = L). The catalogue's default 0.5 leaves the
    right channel unshifted: its note "0.5 = no shift" is true of `shift` only (see Review).
- `shift`: **±2,000 Hz, linear.**
  - It contains Surge's ±100 Hz with room for bells and clang (FreqEcho stops at ±1 kHz).
  - Beyond 2 kHz, a guitar is pure noise-like clangour.
  - Fine resolution near 0 Hz is golem's UI's business (contract: linear ranges only). The plugin's
    resolution is that of a float: 0.01 Hz is exact enough at any setting.
  - **Advice to golem's UI**: a symmetric curve, with half of the knob's travel for ±10 Hz, e.g.
    Hz = sign(u) · 10 · 200^(2 · |u| − 1) for |u| ≥ 0.5 and 20 · u below (u = −1 … +1). An EXP pedal
    should be mapped to a sub-range (heel 0, toe +5 Hz), never to the full range: 7 bits over 4 kHz
    are 31 Hz steps.
  - No `fine` parameter: two knobs for one frequency make every preset ambiguous.
- `time`: **0 – 2,000 ms, linear.** Surge's 3.9 ms minimum becomes 0 (an undelayed Bode). Its 32 s top is
  clamped: everything above n = 0.692 maps to 2,000 ms, and no golem preset goes there.
- `fbk`: **0 – 110 %**, linear loop gain (Surge: n³, ≤ 100 %).
- `mix`: **0 – 100 %, linear crossfade**, out = (1 − m) · dry + m · wet. This is the catalogue's law,
  REAPER's Wet knob [I: linear law to be confirmed by golem-rack].

### 3.2 Front panel

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `shift` | Shift L | f | −2,000 – +2,000 Hz | +2 Hz | EXP | ✓ | Moves every partial by this many Hz (left, and the base of the right). ±0.1 – 5 Hz: phasing, barber-pole, metallic detune; 10 – 100 Hz: inharmonic, bells; above: clang |
| `fbk` | Feedback | f | 0 – 110 % | 0 % | EXP | ✓ | The shifted wet fed back: each pass is shifted again. Spirals, barber-pole, drifting echoes. Above 100 % the loop runs away, bounded |
| `time` | Time | f | 0 – 2,000 ms | 0 ms | glide 100 ms | ✓ | Delay before the shifter, inside the loop. 0 = classic shifter; 1 – 10 ms = barber-pole flanger; 20 – 2,000 ms = shifted echoes |
| `mix` | Mix | f | 0 – 100 % | 50 % | EXP | ✓ | Linear dry / wet crossfade (the catalogue's law). 50 % = the deepest phasing |

### 3.3 Expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `shift_r` | Shift R | f | −100 – +100 % | +100 % | EXP | ✓ | Right shift as a ratio of the left: +100 % = same (phase-locked), 0 = right unshifted, −100 % = opposite (stereo spread) |
| `loop` | Loop | e | Delay · Phaser | Delay | crossfade | | Phaser adds a dispersive 12-stage all-pass chain in the loop: a denser barber-pole phaser at `time` 0 (Delay alone already phases, through the Hilbert pair) |
| `sideband` | Sideband | f | −100 – +100 % | +100 % | 20 ms | ✓ | +100 = the signed shift only; 0 = both sidebands (carrier-free ring mod); −100 = the mirror (Bode's Up / Down mix) |
| `zero_cut` | Zero Cut | b | off / on | on | crossfade | | Removes the partials shifted below 0 Hz (no mud) or past Nyquist, by a second pass through the Hilbert pair. Off = they reflect: the through-zero growl |
| `dry_match` | Dry Match | b | off / on | off | crossfade | | Dry through the wet's own all-pass path: small shifts swell like a tremolo instead of phasing; shift 0 nulls to −83 dB |
| `time_sync` | Time Sync | e | contract list (Off · 4/1 … 1/32) | Off | glide | | `time` from the host tempo (a duration) |
| `low_cut` | Loop Low Cut | f | 20 – 1,000 Hz | 20 Hz (off) | 30 ms | | In the feedback return: each pass thinner. gfx-delay's id and range |
| `high_cut` | Loop High Cut | f | 1,000 – 20,000 Hz | 20,000 Hz (off) | 30 ms | ✓ | In the feedback return: each pass darker; tames upward spirals. gfx-delay's id and range |
| `lfo_depth` | LFO Depth | f | 0 – 500 Hz | 0 Hz | EXP | ✓ | ± Hz added to the shift by the LFO; may cross zero |
| `rate` | LFO Rate | f | 0.01 – 20 Hz | 0.5 Hz | EXP | ✓ | When `sync` = Off |
| `sync` | LFO Sync | e | contract list | Off | — | | LFO period and phase from the host position (shared id) |
| `lfo_shape` | LFO Shape | e | Sine · Triangle · Saw Up · Saw Down · Square · Random Steps · Random Smooth | Sine | — | | gfx-glitch's order |
| `stereo_phase` | Stereo Phase | f | 0 – 180° | 0° | 30 ms | | Right LFO offset (shared id) |
| `env_depth` | Envelope Depth | f | −1,000 – +1,000 Hz | 0 Hz | EXP | ✓ | Hz added by the input envelope: the pick pushes the shift |
| `env_sens` | Env Sensitivity | f | 0 – 100 % | 50 % | — | | Shared id |
| `env_att` | Env Attack | f | 1 – 100 ms | 5 ms | — | | Shared id |
| `env_rel` | Env Release | f | 20 – 2,000 ms | 250 ms | — | | Shared id |
| `bypass` | Bypass | b | off / on | off | §Trails | | Bypass with tail (nih-plug `make_bypass`). The id every coded effect ships (§7.10) |
| `trails` | Trails | b | off / on | on | — | | Loop rings out after off |

No mode-dependent defaults. Every user parameter is neutral at its default, except `shift` (+2 Hz) and
`mix` (50 %), so the pedal sounds when inserted bare. golem pins its own values at insert.

### 3.4 Conversion from the `surge-freqshift` catalogue (golem engine table)

n = the stored normalized value.

| id | Formula | Catalogue default n → |
|---|---|---|
| `shift` | Hz = 200 · n − 100 | 0.5 → 0 Hz |
| `shift_r` | % = 200 · n − 100 | 0.5 → 0 % (**right unshifted**) |
| `time` | ms = min(2,000, 1,000 · 2^(13 · n − 8)) | 0.3 → 58.3 ms |
| `fbk` | % = 100 · n³ | 0 → 0 % |
| `mix` | % = 100 · n | 0.5 → 50 % |

The three existing block presets, converted (what they really did in Surge):

| Preset | shift | shift_r | time | fbk | mix | Note |
|---|---|---|---|---|---|---|
| barberpole | +4 Hz | +4 % (R +0.16 Hz) | 143.6 ms | 27.5 % | 45 % | Its author clearly meant R = L. As stored, the right channel is almost unshifted |
| metal-smear | +40 Hz | +40 % (R +16 Hz) | 15.1 ms | 12.5 % | 40 % | |
| radio-ghost | +24 Hz | +16 % (R +3.8 Hz) | 23.7 ms | 2.7 % | 50 % | |

Recommendation to golem-rack: re-author these presets in real units, and set `shift_r` to +100 % where
"same as left" was meant. The catalogue default `shift_r` 0.5 is an error too: Surge's own default is
1.0 (R = L), and 0.5 leaves the right channel dry on every inserted block, in Surge today as well (see Review).

## 4. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Barber-pole phasing (endless, never turns round) | `shift` +0.3 Hz, `mix` 50 %, `fbk` 0, `time` 0 | Image ≤ −84 dB: one clean beating partial, no second one fighting it |
| Barber-pole phaser spiral | `loop` Phaser, `shift` +0.5 Hz, `fbk` 70 %, `time` 0, `mix` 50 % | Dispersive loop (Costello) |
| Barber-pole flanger | `loop` Delay, `time` 3 ms, `shift` −0.4 Hz, `fbk` 75 %, `high_cut` 8 kHz | Comb teeth gliding endlessly down |
| Sweet metallic detune / chorus | `shift` +1.5 Hz, `shift_r` −100 %, `time` 12 ms, `mix` 40 % | Opposite stereo shifts; a short delay decorrelates the wet |
| Tremolo-like swell | `shift` +4 Hz, `dry_match` on, `mix` 50 % | Phase-matched dry: whole-band beating |
| Bell / clang lead | `shift` +180 Hz, `mix` 60 %, `high_cut` 6 kHz | Inharmonic partials |
| Carrier-free ring mod | `sideband` 0 %, `shift` 400 Hz, `mix` 100 % | Both sidebands, no carrier bleed |
| Through-zero swirl | `shift` 0, `lfo_depth` 3 Hz, `rate` 0.15 Hz, `mix` 50 % | Continuous-phase oscillator crossing 0 Hz |
| Zap on every pick | `shift` 0, `env_depth` +600 Hz, `env_rel` 150 ms, `mix` 50 % | Envelope in Hz |
| Falling ghost echoes | `time` 350 ms (or `time_sync` 1/8•), `shift` −30 Hz, `fbk` 70 %, `low_cut` 150 Hz, `mix` 35 % | `zero_cut` keeps the falling repeats from turning into mud |
| Rising spiral echoes (FreqEcho style) | `time` 250 ms, `shift` +12 Hz, `fbk` 85 %, `high_cut` 5 kHz | `zero_cut` inside the loop also removes what wraps past Nyquist |
| Runaway wall (EXP on feedback) | `fbk` 105 %, `time` 80 ms, `shift` +7 Hz, `high_cut` 4 kHz | Bounded saturation; recovers when lowered |
| Bass growl | `shift` −60 Hz, `zero_cut` off, `mix` 35 % | The through-zero reflection, on purpose |
| Radio between stations | `shift` +24 Hz, `shift_r` +16 %, `time` 24 ms, `fbk` 3 %, `mix` 50 % | The converted `radio-ghost` |

**Preset ideas** (ids for `presets/gfx-freqshift.json`, no artist names): `endless-phase`, `spiral-phaser`,
`spiral-flanger-down`, `metal-detune-wide`, `swell-beat`, `bell-lead`, `ring-carrierless`,
`zero-swirl`, `pick-zap`, `ghost-echo-fall`, `spiral-echo-rise`, `runaway-wall`, `bass-growl`,
`radio-ghost`, `metal-smear`, `barberpole` (the last three are the catalogue presets, in real units).

### Visual

- **Category**: `pitch` (the catalogue's). Body hue: `METAL_HUE.pitch` `#c4148c`.
  - No crate hue: the motif already sets it apart from the gfx-pitch pedals.
  - gfx-ringmod takes brass, so the two never look alike.
- **Finish**: anodized (the pitch family's `auto` finish).
- **Motif `barber`** (new, in `motifParts`): two barber-pole columns, one at each edge of the face.
  - **Columns**: x ∈ [4, 16] and [W − 16, W − 4], over the rune zone only, y from cy − ch to cy + ch
    (as `curve` and `stairs`). The knob grid spans x 6 … W − 6 from y 10, so columns starting at y 14
    would run under the knobs. Each column has its two edges as vertical lines (opacity .5).
  - **Stripes**: diagonals every 9 px, slope ≈ 30°, clipped to the column.
    - The left column's stripes **rise to the right** and the right column's **rise to the left**: the
      two channels shifting opposite ways.
    - Their opacity grows from .35 at the bottom to 1 at the top, the endless climb.
  - Drawn with `line()`, so both finishes work: engraved in anodized, yellow lined in black in thrash.
    It stays clear of the centred knobs and the rune.
- **Face** (`visuals/data/faces.json`):
  `"gfx-freqshift": {"name": "Shifter", "blocks": ["surge-freqshift"], "knobs": ["shift", "fbk", "time", "mix"], "selectors": ["loop"], "category": "pitch"}`.
  No toggles. "Shifter" is a plain word, not a maker's or a person's name ("Bode" is avoided).
- **Glyph** (24 × 24, `fill="none" stroke="currentColor" stroke-width="1.8"`, round caps and joins, as
  `glitch.svg`): **a spectrum moved by the same number of hertz**.
  - Three partials of a harmonic series, then the same three moved right by the same offset (fainter),
    under an arrow. Spacing 7, offset 3: each ghost sits next to its partial (1.2 px of air), 2.2 px
    from the next one, so the six bars read as three pairs. (With spacing 6 and offset 3.5, the ghosts
    stood 0.7 px from the next partial: a comb of six.)

  ```
  <path d='M3.5 21V10M10.5 21V7M17.5 21V13'/>
  <path d='M6.5 21V10M13.5 21V7M20.5 21V13' opacity='.45'/>
  <path d='M3.5 3.5h16M17 1.5l2.5 2-2.5 2'/>
  ```

  - It **replaces** `surge-freqshift.svg` (§7.11): two opposed arrows say "pitch up / down", which is
    what the catalogue's note says this block is not. A pedal's rune is its first block's glyph.

## 5. Bench suite (on top of gfx-bench's common checks)

Targets are computed in the test, never copied by hand. Every check runs at 44.1, 48, 96 and 192 kHz
(the common rule). The image targets per rate are below.

| Check | Pass |
|---|---|
| Direction | +100 Hz on a 1 kHz sine lands at 1,100 Hz; −100 Hz at 900 Hz (catches a swapped sign or path) |
| Pair design | Computed in the test from the coefficients: quadrature error over 15 Hz – fs/2 − 15 Hz gives an image within 0.5 dB of 6.02 + 5 · (2N + 1) · log₁₀ q, at 44.1, 48, 88.2, 96, 176.4 and 192 kHz, and at an odd rate (50 kHz) |
| Image rejection | Sines at 30 Hz, 82 Hz, 1 kHz, 5 kHz, 15 kHz; shifts ±5, ±100, ±1,000 Hz; `zero_cut` on and off; image measured by FFT (Blackman, 2 s): ≤ −84 dB at 44.1 / 48 kHz (8 + 8), ≤ −88 dB at 88.2 – 192 kHz (10 + 10). The same measurement is the regression test of `pitch::FreqShifter` once fixed (≤ −40 dB with Niemitalo's pair) |
| Low end | 41.2 Hz (bass E) and 30.9 Hz (B) shifted +3 Hz: image ≤ −84 dB at 48 kHz, level within 1 dB (the 20 Hz high-pass) |
| DC and thump | Input DC 0.1 plus silence, shift +50 Hz: nothing at 50 Hz above −100 dBFS once settled. A 5 Hz sine at −20 dBFS, shift +200 Hz: nothing between 150 and 250 Hz above −43 dBFS |
| Through zero | `shift` 0, `lfo_depth` 50 Hz, `rate` 2 Hz on a 440 Hz sine: no click (step-click bound of the common checks), output envelope flat ±0.1 dB |
| Zero cut | 100 Hz sine, shift −150 Hz: the 50 Hz reflection ≤ −80 dB with `zero_cut` on, within 0.5 dB of the input with it off. 170 Hz, shift −150 Hz: the 20 Hz result within 0.5 dB (nothing wanted is cut). The same 100 Hz sine under `shift` −100 Hz, `lfo_depth` 100 Hz at 5 Hz: the output matches an offline reference (FFT analytic signal, shifted, negative frequencies zeroed) within −60 dB |
| Parked | After any shift history, `shift` set to 0 (depths 0): within 1 s the output equals that of a fresh instance at shift 0, within −80 dB |
| Wrap | Sine at 0.47 · fs, shift +2 kHz: the wrapped component ≤ −80 dBc with `zero_cut` on. Then noise for 5 s with `fbk` 100 %, `time` 3 ms, shift +2 kHz, `zero_cut` on and off: once the input stops, the band above fs/2 − 2.5 kHz falls below −80 dBFS within 5 s (nothing stays trapped under Nyquist) |
| Sideband | `sideband` 0 → both sidebands within 0.1 dB of each other; −100 → the mirror sideband alone (image ≤ −80 dB) |
| Dry match | `dry_match` on, `zero_cut` on, shift 0, `time` 0, `fbk` 0, noise: wet − matched dry ≤ −75 dB from 20 Hz to 20 kHz. At shift +4 Hz, `mix` 50 %: the output envelope is a full-depth 4 Hz beat (trough ≤ −40 dB) on a chord |
| Loop | Impulse, `time` 100 ms, `fbk` 50 %, shift +10 Hz: the k-th repeat at 100·k ms ± 1 sample, level 0.5^k ± 0.2 dB, centroid shifted by k · 10 Hz |
| Stability | White noise at −12 dBFS, 10 s at `fbk` 110 % in every `loop`, `time` 0 / 3 / 500 ms, shift 0 and ±7 Hz, `zero_cut` on and off: the fed-back signal ≤ 0 dBFS, output peak ≤ +12 dBFS, no NaN. After the input stops with `fbk` 100 %, shift ±50 Hz, `time` 3 ms, `zero_cut` on: the tail falls below −80 dBFS within 30 s (the energy leaves by the band's ends) |
| Stereo | `shift_r` +100 %, `stereo_phase` 0, equal inputs: L = R bit-exact from reset, and again at most 100 ms after `shift_r` moves away and back (phase lock, then set equal); −100 %: L up, R down by the same Hz |
| Short times | `time` glided 0 → 1 ms → 0 on a 1 kHz sine: no click (the read below one sample), and at `time` 0 the wet is undelayed (impulse at sample 0 plus the pair's response) |
| EXP sweep on `shift` | 50 Hz-stepped sweep 0 → 500 Hz: control-rate sidebands ≤ −80 dB (contract rule) |
| Surge conversion | §3.4 formulas reproduce the three presets' values; the renders of `barberpole` / `metal-smear` / `radio-ghost` land on Surge's REAPER renders' partial frequencies within 0.1 Hz (golem-rack renders) |
| Latency / determinism / CPU | 0 samples reported, dry impulse at sample 0; strict null; ≤ 300 ns per stereo frame at 48 kHz |
| Listening | `gfx-bench apps gfx-freqshift` (one render per Applications row) on clean, crunch and bass DIs; blind A/B against Surge's renders for the three presets |

gfx-bench options:
- step-click bound: default; `time` jumps may Doppler but not click;
- silence floor: default −140 dBFS at `fbk` ≤ 100 %, exempt at 110 %, which self-sustains on purpose,
  as gfx-delay.

## 6. Reuse from gfx-dsp

| Reused | Improved or new |
|---|---|
| `pitch.rs`'s z² all-pass section (1 mul) and f64 phase accumulator | New module `ssb`: correct path delay; per-rate closed-form design (exact nome, 8 + 8 or 10 + 10) with the band fixed in Hz (15 Hz); image −87 dB vs −44 dB (correct Niemitalo) and −18 dB at 1 kHz (current `pitch.rs`); the pair reused as the negative-frequency filter (`zero_cut`) |
| `delay` (Hermite read, glide), `svf` (TPT, tracking filters), `lfo`, `envelope`, `exp_smooth`, `transport`, `smooth` | Input high-pass, `zero_cut` second pass, anti-wrap tracking low-pass, signed through-zero shift with a parked phase, `sideband` balance, right-channel phase lock: all in `ssb`, so gfx-delay, gfx-octaver, gfx-glitch and gfx-ringmod can switch to it |

## 7. Questions asked of the review, and the decisions

1. **The `pitch.rs` quadrature bug: confirmed** (§2.2, Review). **Fix it in place now**, then migrate.
   - The fix is one line per struct (delay `i`, not `q`); the sign of `hz` does not change. Add an FFT
     image test (≤ −40 dB from 30 Hz to 20 kHz at 44.1 and 48 kHz) next to the zero-crossing one.
   - It is a shipped defect, not a voicing: gfx-delay's `loop_shift` is today a half ring modulator
     above 2 kHz and shifts the wrong way from 6 kHz. Waiting for `ssb` would leave it so.
   - gfx-octaver takes its analytic signal from the same pair: the error is small where its
     fundamentals are (2° at 82 Hz, 5° at 330 Hz) and grows above. Re-listen both; tell golem-rack that
     two shipped sounds change (no id, range or name does).
   - When `ssb` lands, gfx-delay, gfx-octaver and gfx-glitch's private copy move to it, and
     `pitch::Hilbert` / `FreqShifter` are retired. gfx-ringmod's design should name `ssb` directly.
2. **`shift_r` stays Surge's ratio.** The id keeps the block's meaning, and the ratio is the better
   control. The three presets and the catalogue default were written as if it were a shift: that is
   for golem-rack to correct (see Review), not for the plugin to imitate.
3. **`shift` ±2,000 Hz linear: kept.** Not ±500 Hz (bells and clang need the range). The low end is
   golem's knob curve (advice in §3.1). No `fine` parameter.
4. **`time` 0 – 2,000 ms linear: fine.** Nothing above 2 s is used, and a shifted echo longer than 2 s
   is gfx-delay with `loop_shift`.
5. **2 / 2 first: yes.** Stereo by nature under the contract (an L/R control in the block, three
   presets that use it). The cost is stated in §2.3: placed before an amp, the lane turns stereo.
6. **High rates: neither −60 dB nor half-rate.** The limit was the nome series, not the design. Exact
   nome, 10 + 10 sections above 50 kHz: −100 dB at 96 kHz, −92 dB at 192 kHz.
7. **`dry_match` off by default: yes.** "Barberpole" means notches that climb: the unmatched dry. On is
   the tremolo-like variant, a preset (`swell-beat`).
8. **Phaser chain: the enum stays, no `disperse` amount.** Two voiced topologies are easier to preset
   than a continuum, and the enum can grow at the end (a 6-stage chain, say) if the ear asks. Noted in
   §2.2: Delay at `time` 0 already phases.
9. **Overlap with gfx-ringmod: keep both, one rule.**
   - gfx-ringmod owns the **carrier**: free or tracked, musical intervals, waveforms, the diode ring.
     Its Upper / Lower are there for the tracked carrier (the clean sub-shift of the Ring Thing and
     the Fractal block), which this pedal cannot do.
   - gfx-freqshift owns the **shift in Hz**: feedback, barber-poles, through-zero, opposite stereo
     shifts, shifted echoes. `sideband` 0 is a by-product (sine, fixed Hz, no carrier model): not
     advertised as "the ring modulator".
   - Both use `ssb`; neither duplicates the other's panel.
10. **Bypass id: `bypass`** (off by default), with `trails`. gfx-delay, gfx-reverb, gfx-shimmer,
    gfx-glitch and gfx-killswitch ship it; the contract's table still says `on` and should be aligned.
11. **Glyph: replace the catalogue's.** One block, one glyph; the new one says what the block does.
12. **REAPER's Wet law**: linear, out = (1 − w) · dry + w · wet, kept as `mix`. Still to be confirmed
    by a golem-rack measurement (see Review).

## 8. As coded (v0.1.0, 2026-11-15)

Files:
- `gfx-dsp/src/ssb.rs`: `HilbertDesign` (closed form, exact nome by the AGM, `rejection_db`, `response`,
  `image_db`), `Allpass` (one path), `HilbertPair` (`analytic`, `split`, `positive`), `Phasor`
  (signed advance, shortest-way slew) and `SsbShifter` (`shift` → complex w, `zero_cut`, `process` for
  an external carrier phase: the API gfx-ringmod's Upper / Lower can call).
- `src/lib.rs`: the processor; `src/plugin.rs`: the VST3 (class id `GoleMFxFreqSh001`, 2 / 2 then
  1 / 1, latency 0); `src/tests.rs`: the §5 rows that run on synthetic signals.

### Measurements

All rates are 44.1, 48, 96 and 192 kHz unless stated.

| Check (§5) | Target | Measured |
|---|---|---|
| Pair design (`ssb` tests, 7 rates incl. 50 kHz) | within 0.5 dB of the formula | within 0.1 dB. −87.8 / −86.8 / −100.8 / −99.7 / −92.5 / −91.6 dB at 44.1 / 48 / 88.2 / 96 / 176.4 / 192 kHz. The 48 kHz coefficients are pinned to proto/design.py (1e-12) |
| Direction | +100 Hz → 1,100 Hz, −100 Hz → 900 Hz | level within 0.1 dB; the other side under −80 dB |
| Image rejection (grid of 5 sines × 6 shifts × `zero_cut`) | ≤ −84 dB (8 + 8), ≤ −88 dB (10 + 10) | worst −86.9 dB at 44.1 kHz, −87.8 dB at 48 kHz, −95.6 dB at 96 kHz, −91.6 dB at 192 kHz |
| Low end, +3 Hz at 48 kHz | image ≤ −84 dB, level within 1 dB | 41.2 Hz: −0.23 dB, image −136 dB; 30.9 Hz: −0.70 dB, image −138 dB (the second pass removes the image a second time) |
| DC and thump | ≤ −100 dBFS; ≤ −43 dBFS | DC: nothing measurable; the 5 Hz thump at +200 Hz: −44.1 dBFS |
| Through zero (±50 Hz at 2 Hz) | envelope ±0.1 dB, no click | 0.015 dB peak to peak; HF under −60 dB |
| Zero cut, 100 Hz − 150 Hz | ≤ −80 dB; off: within 0.5 dB | −88.3 / −87.0 / −108.3 / −95.0 dB; off and 170 Hz → 20 Hz: within 0.5 dB |
| Parked | −80 dB within 1 s | −149 dB |
| Wrap, single pass (0.47 fs + 2 kHz) | ≤ −80 dBc | −88.7 dBc at 44.1 kHz, −93.2 dBc at 48 kHz (no wrap at the higher rates) |
| Wrap in the loop (48 kHz, cut on and off) | top band < −80 dBFS within 5 s | nothing left (< −800 dBFS) |
| Sideband | 0: within 0.1 dB; −100: image ≤ −80 dB | both met |
| Dry match, null | ≤ −75 dB, 20 Hz – 20 kHz | −81.8 dB, worst third-octave |
| Dry match, beat on a chord | trough ≤ −40 dB | −54.6 dB |
| Loop (1 kHz burst, 100 ms, 50 %, +10 Hz) | 0.5^k ± 0.2 dB, +k · 10 Hz | met for 5 repeats; frequency within 1 Hz. Timing: see the deviations |
| Stability, 110 %, 36 settings × 10 s | output ≤ +12 dBFS, no NaN | worst +9.5 dBFS |
| Drain, 100 %, ±50 Hz, 3 ms | < −80 dBFS within 30 s | nothing left |
| Stereo | L = R bit-exact, again ≤ 100 ms after the lock; −100 %: opposite | met |
| Short times, impulse | no click; wet on sample 0 | met |
| EXP sweep (50 Hz steps) | sidebands ≤ −80 dB | met |
| Bench common checks | all | 28 / 28 |
| CPU (bench, mono source on 2 / 2) | ≤ 300 ns / stereo frame | 136 ns (130 ns mono). True stereo: ≈ 230 ns at default, ≈ 340 ns with `loop` Phaser and `dry_match` on |

### Deviations from the design, and why

1. **`time` glide**: a 100 ms one-pole, with the read head's speed limited to one sample per sample
   (the Doppler stays between a freeze and an octave up) and the speed itself smoothed over 10 ms. As
   designed (a plain 100 ms glide), a 0 → 2,000 ms step moved the head at 20 samples per sample: a burst
   at −39 dB above 10 kHz. The bench's step check now passes at −68 dB. A jump of a whole second takes
   about a second.
2. **Bench exemption, the step to `fbk` 110 %** (new `Options::step_exempt` in gfx-bench, empty for
   every other effect). At `time` 0 the loop's fastest-growing modes are the high ones, so the runaway
   starts with a burst above 10 kHz: −16 dB at 96 kHz, −9 dB at 192 kHz, then it settles. This is the
   self-oscillation §5 already exempts from the silence floor. Every other step passes the default
   bound.
3. **Loop timing**: as the equation says, the loop is `time` + 1 sample, plus the group delay of the
   pair and of the loop's cuts. Repeat k arrives at k · (time + 1 sample) − 1 sample + the dispersion,
   not "100 · k ms ± 1 sample": 1.6 samples late by the 5th repeat at 1 kHz (the 20 Hz / 20 kHz loop
   cuts). The test checks it against that model, ±2 samples.
4. **The right channel's lock**: the phase slews onto the left's over 50 ms, then waits for the
   smoothers to settle. The right output then crossfades onto the left's over 20 ms and takes its whole
   state (delay line included, copied without allocating: `DelayLine::copy_from`, new in gfx-dsp). Only
   then is L = R bit-exact and computed once. One side effect: if both inputs are bit-equal for 20 ms
   (digital silence on a true stereo lane), the right merges onto the left, tail included. When the
   inputs part again, the right resumes from the left's state (one copy of the line).
5. **Loop cuts**: always in the return (2nd-order Butterworth high- and low-pass). Their "off" values,
   20 Hz and 20 kHz (at most 0.45 fs), are not bypassed. The −3 dB at 20 kHz per pass is part of the loop's
   colour.
6. **Anti-wrap low-pass**: recomputed exactly (tan) whenever its cutoff moves by more than 1 Hz, rather
   than with a fast tan approximation.
7. **Envelope**: gfx-glitch's level-independent mapping (a 30 dB window whose floor is −20 − 40 ·
   `env_sens` dBFS), then × `env_depth` Hz.
8. **Not coded as tests**: the zero cut under modulation against an offline FFT reference (the
   reference would also need the pair's dispersion), and the Surge render comparison (golem-rack's
   renders). The §3.4 conversion is in the factory presets (`radio-ghost`, `metal-smear`,
   `barberpole`, the last with `shift_r` +100 % as its author meant).
9. **`ssb` section form**: y = c · x + (c · y[n−2] − x[n−2]). It is the same filter as `pitch.rs`'s
   section, with one multiply-add on the critical path instead of three operations (true stereo
   default: 300 → 230 ns).

`pitch.rs` was not touched: its delay is already on the right path. Moving gfx-delay, gfx-octaver and
gfx-glitch to `ssb` is a separate change (§7.1).

## Review (Fable 5.2, 2026-11-14)

**Verdict: Strong**, after the fixes below. The survey is right, the choice of a per-rate elliptic
all-pass pair is the right one (zero latency, a band fixed in Hz, an image 40 dB under the analog
reference), and Surge's laws were read correctly: checked against `FrequencyShifterEffect.cpp` (shift
× 10 Hz, right = ratio, default ratio 1.0, feedback = v³, time = 2^v s, soft clip on the return). Two
things did not hold on paper: the protection filters, and the exact null of `dry_match`. Both are solved
by one idea the design already paid for: the Hilbert pair used a second time, as a filter. What cannot
be settled here is voicing: the loop's dispersion and the phaser chain are for the ear.

### The Hilbert bug in gfx-dsp `pitch.rs`: **confirmed**

Read in the code and computed from its coefficients (frequency response, then a time-domain run of
`FreqShifter` as coded; 48 kHz):

| Input | Image, as coded (delay on path B) | Image, delay on path A |
|---|---|---|
| 82 Hz | −37 dB | −51 dB |
| 1 kHz | **−18 dB** (time domain: −17.9 dB) | −49 dB |
| 5 kHz | −2.4 dB | −47 dB |
| 10 kHz | wrong way: wanted −11.9 dB, image −0.3 dB | −46 dB |
| 15 kHz | wrong way: wanted 7.8 dB under the image | −44 dB |

The quadrature is 75° at 1 kHz, 16° at 5 kHz, and negative from 6 to 18 kHz. Niemitalo's page delays
the cascade starting with 0.6923878; the code delays the other. gfx-glitch's copy is right. The
designer's figures and the explanation of the test that missed it are all correct.

### Changes made, and why

| # | Change | Why |
|---|---|---|
| 1 | **Hilbert design: exact nome**, 8 + 8 up to 50 kHz, 10 + 10 above; no embedded tables | The −82 / −72 / −60 dB of the design reproduce exactly with HIIR's 4-term series for q, and stay there with 10 + 10 or 12 + 12 sections. With q = exp(−π K′ / K): −86.8 dB at 48 kHz, −99.7 dB at 96 kHz, −91.6 dB at 192 kHz. "Saturates at high rates" was the approximation |
| 2 | **`zero_cut` = a second pass through the pair** on the complex shifted signal; the tracking 4th-order high-pass is dropped | The high-pass at \|shift\| + 20 Hz leaves the bench's own case (100 Hz, −150 Hz) at −18.5 dB, where the bench asked −60 dB. The second pass measures −87 dB, also removes the wrap past Nyquist (−97 dB), follows any modulation for free and costs what the two SVF pairs cost |
| 3 | **Anti-wrap low-pass only with `zero_cut` off**; its reason rewritten | In one pass a wrap lands above fs/2 − shift: ultrasonic here, not "an inharmonic mirror". The real fault is the loop: that band traps energy, and a downward modulation brings it down |
| 4 | **Input high-pass 2nd order at 20 Hz** instead of a one-pole at 5 Hz | A shifter makes a tone at \|shift\| of anything near 0 Hz. A one-pole at 5 Hz passes a 10 Hz thump at −1 dB |
| 5 | **`dry_match`** takes the wet's whole static path (high-pass, path A once or twice); the claim is now a measured −83 dB null | As designed (5 Hz blocker, high-pass parked at 15 Hz, 20 kHz low-pass in the wet only), the "exact" null was +1 dB at 30 – 50 Hz, −6 dB at 82 Hz, −13 dB at 5 kHz |
| 6 | **Parked phase**: φ returns to 0 when the commanded shift is exactly 0 | A frozen φ is arbitrary: at 180° the 50 % mix cancels, and the null of `dry_match` held only from reset |
| 7 | **Sideband formula** written once for both signs, on the complex signal | "The roles swap for a negative shift" left the crossfade undefined at the sign change |
| 8 | **Delay read below one sample** specified | A Hermite read needs a sample ahead of the tap; `time` 0 was to read "the sample just written" |
| 9 | `sat` bounds the return, not the output; the Stability check restated (output ≤ +12 dBFS on −12 dBFS noise). "Why the loop drains" added | "Peak ≤ 0 dBFS" cannot hold after all-pass networks. The drain check needs a shift that leaves the band in 30 s: at 0.3 Hz it never does |
| 10 | **`sync` = the LFO's, `time_sync` = the delay's**; `lfo_sync` dropped | "`sync` goes to `time`, as in gfx-delay": gfx-delay has no `sync` (its `time` is the catalogue's list). In the coded effects (chorus, phaser, tremolo, vibrato, glitch…) `sync` is the modulation's |
| 11 | **`bypass`**, not `on` | What every coded effect with a tail ships |
| 12 | `high_cut` in Hz (1,000 – 20,000), as gfx-delay | Same id, same unit |
| 13 | **Units**: the departure from "the catalogue's unit" is stated and justified | gfx-ringmod kept 0 – 1, gfx-crusher converts: the document must say which, and that golem-rack has to accept the conversion |
| 14 | `shift_r`: Surge's default is 1.0; the catalogue's 0.5 called an error | Read in `init_default_values`. With 0.5, every inserted block has a dry right channel |
| 15 | Stereo: lock = slew then **set equal**; L computed once when R is locked on equal inputs; the cost of 2 / 2 before an amp stated | A slew alone never gives the bit-exact L = R the bench asks. A mono DI on a stereo lane would pay twice for the same samples |
| 16 | `loop`: Delay at `time` 0 described as what it is, a sparse barber-pole phaser | The pair alone turns 16π (32π with `zero_cut`): the loop is dispersive before any chain is added |
| 17 | **Bench**: Pair design, Parked, Short times, thump added; image ≤ −84 / −88 dB; zero cut ≤ −80 dB and checked under modulation against an offline reference; wrap checked in the loop | Targets follow the new design; each new mechanism has its check |
| 18 | **Visual**: motif columns moved to the rune zone; glyph re-spaced | The knob grid covers x 6 … W − 6 from y 10: columns from y 14 ran under the knobs. Ghost bars stood 0.7 px from the next partial |
| 19 | Cross-references §3.3 → §3.4; CPU table; latency note (dispersion doubled by `zero_cut`) | Errors and consequences of the above |

Checked and left as written: the Surge reading (Weaver, loop, 12-tap sinc, 2^18 line, laws), the three
converted presets (4 Hz / 0.16 Hz, 40 / 16 Hz, 24 / 3.8 Hz; 143.6, 15.1, 23.7 ms; 27.5, 12.5, 2.7 %),
the 0.692 clamp, Niemitalo's figures at 44.1 and 96 kHz (−44 dB; −39 dB at 41 Hz and −15 dB at 20 Hz),
the group delays (10.8 / 5.9 / 0.54 / 0.12 ms), which path is delayed, the continuous-phase oscillator,
feedback to 110 % with the clip on the return, linear ranges, no mode-dependent default, `pitch`
category and hue, the `barber` motif as an idea.

Coverage: Bode / Moog (both sidebands and their mix; the squelch is an analog cure with no digital
disease), Surge (all five, plus what it lacks), Ableton Shifter (Wide, LFO, envelope, delay; its Ring
mode is `sideband` 0; its Pitch mode belongs to gfx-pitch), FreqEcho (sync, loop cuts, runaway).
Nothing superfluous: `dry_match` is the only narrow control, and it costs one flag. Left out on
purpose: a feedback polarity switch (with a moving shift the sign is only a phase), per-channel LFO
rates (Ableton's Spin), a shift range switch (the Bode's scale, golem's knob curve does it).

### Still open

- **Owner (Raph)**
  - Fix `pitch.rs` now (§7.1) and re-listen gfx-delay `loop_shift` and gfx-octaver.
  - gfx-ringmod's design: point Upper / Lower to `ssb`, and settle its own units question in the light
    of §3.1 here (it keeps 0 – 1, this pedal and gfx-crusher convert).
  - The contract's shared-id table: `on` → `bypass`.
- **Listening** (cannot be settled on paper): the doubled dispersion with `zero_cut` on a bass DI at
  `mix` 100 % (fallback: a 6 + 6 second pass); the phaser chain's corners and stage count; `sat`'s
  knee at 105 – 110 %; whether `zero_cut` on is the right default for the "bass growl" crowd.
- **golem-rack**
  - Accept the one-time conversion of §3.4 (non-linear for `time` and `fbk`), or ask for the 0 – 1
    scale: the latter would cap the pedal at Surge's ranges.
  - Catalogue default `shift_r` 0.5 → 1.0 (also right for Surge today); re-author the three presets
    with `shift_r` +100 % where "same as left" was meant.
  - Confirm REAPER's Wet law (linear) by measurement, and how the Golem Engine applies `mix` to a
    block that now has its own.
  - The knob curve for `shift` (§3.1) and EXP mapped to a sub-range.
  - The new glyph replaces `surge-freqshift.svg`; the block turns a lane stereo.

