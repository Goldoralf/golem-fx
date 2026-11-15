# gfx-ringmod

> **Status: CODED** (v0.1.0, 2026-11-15). Designed 2026-11-14, reviewed by Fable 5.2 (§7 holds the
> decisions, the Review section the changes), then coded with the new gfx-dsp module `track` (the
> phase-locked partial tracker of gfx-sustainer §2.7, shared with gfx-sustainer and gfx-polysynth) and
> Parker's diode law plus a generic ADAA1 in gfx-dsp `adaa`; Upper / Lower use gfx-dsp `ssb`.
>
> - **The phase lock was prototyped first and kept**: it passes the "Bends" and "Lock engage" rows
>   (±50 ¢ 6 Hz vibrato: 7.8 ¢ at E2, 4.3 ¢ at A2, 1.1 ¢ at A3, 0.8 ¢ at E4; without the lock the glide
>   trails by 52 – 53 ¢), so `lock`, `phase` and `state` ship. Locked carriers are exact to < 0.1 ¢ and a
>   harmonic A3 × 2 or × 3/2 leaves −98 to −104 dB of non-harmonic energy (−23 dB without the lock);
>   `phase` lands within 5° from E2 to E5.
> - **The free carrier is mda's**: products within 0.1 dB of −6 dB and every spur ≤ −90 dB at 44.1 / 48 /
>   96 kHz; the feedback matches mda's recursion within 0.5 dB at 48 kHz (1 dB at 44.1 / 96 kHz); a
>   15 kHz × 16 kHz ring leaves aliases ≤ −100 dB; SSB rejects the other sideband by more than 40 dB;
>   Diode is within 0.5 dB of Parker's equations at 64× and its aliases are ≤ −88.6 dB.
> - **Tests**: 23 behaviour tests (gfx-ringmod), 9 `track` and 2 new `adaa` unit tests (gfx-dsp). Bench
>   28 / 28 (latency exempt, click bound −40 dB, both documented in §8). CPU: 121 ns / sample mono at the
>   default, 263 ns with Track, **422 ns in the worst case (budget 300)**: see §8.
> - Deviations and what is left are listed in §8.

golem's **ring modulator**. It multiplies the guitar by a carrier. The carrier is either a free
oscillator (bells, sci-fi, robot voice, tremolo turning into clangour) or a carrier that **follows the
note you play** (harmonic ring tones, synth octaves, hollow subs). It stands for two golem blocks that
third-party plugins play today:

| golem block | Played today by | Catalogue ids (all 0 – 1, raw normalized) |
|---|---|---|
| `ringmod` (modulation) | mda RingMod (mda-lv2) | `freq` 0.3, `fine` 0.5, `feedback` 0 |
| `surge-treemonster` (pitch) | Surge XT Effects, FX type Treemonster | `thr` 0.3, `speed` 0.5, `lowcut` 0.3, `highcut` 0.7, `pitch` 0.5, `ring` 0.5, `mix` 0.5 |

The two blocks share no id, so one plugin carries all ten. A `carrier` selector picks Free (the
`ringmod` block) or Track (the `surge-treemonster` block); golem sets it at insert
([golem contract](../docs/golem-contract.md), "values that differ per golem block").

## 1. State of the art

Survey of 2026-11-14. **[V]** read in source code or a manual · **[M]** manufacturer page or listing ·
**[F]** forum post or review · **[I]** inference, or not re-checked at the source.

### The two blocks we replace (read in their source)

- **mda RingMod** [V] (`mdaRingMod.cpp`, mda-lv2):
  - carrier `sin(φ)`, with f = 100 · (`fine` + 160 · `freq`) Hz, so **f = 16,000 · freq + 100 · fine**.
    The range runs 0 – 16.1 kHz, linear in Hz. The display shows the coarse part floored to 100 Hz, but
    the audio uses the continuous value.
  - Feedback: `y[n] = (0.95 · feedback · y[n−1] + x[n]) · sin(φ[n])`. This is a one-sample loop at the
    host rate, with no filter and no limiter.
  - 100 % wet, no mix. Phase 0 at start, so a 0 Hz carrier is silent.
  - The right channel reads the left's feedback state (a bug; irrelevant in mono).
- **Surge XT Treemonster** [V] (`sst-effects` `TreemonsterCore.h`, GPL-3):
  - **Detector.** Positive zero crossings of the input, through a 2-pole high-pass (`lowcut`) and
    low-pass (`highcut`). A crossing counts only if the sample is above `thr` and the period is longer
    than 16 samples. A period more than 10× the smoothed one is rejected.
  - **Glide.** The period is smoothed by a one-pole, `c = 0.9999 − speed⁴ · 0.0999 / 128` per 48 kHz
    sample: **τ = 208 ms at `speed` 0, 140 ms at 0.5, 104 ms at 0.6, 24 ms at 1**.
  - **Carrier.** A quadrature sine at (1 / period) · 2^(`pitch` / 12).
  - **Wet.** wet = ring · (x · c) + (1 − ring) · (env · c), so `ring` crossfades the ring-modulated
    guitar against a pure sine voice scaled by an envelope (5 ms attack, 500 ms release, on the signed
    sample).
  - **Mix.** A linear crossfade, dry · (1 − mix) + wet · mix.
  - **Exposed ranges (VST3, all linear 0 – 1)** [V] (`Parameter.cpp`, `set_value_f01`):

    | Param | Range | Note |
    |---|---|---|
    | `thr` | −96 – 0 dB | |
    | `pitch` | −60 – +60 st | |
    | `lowcut`, `highcut` | −60 – +70 st around 440 Hz | 13.75 Hz – 25.1 kHz |
    | `speed`, `ring`, `mix` | 0 – 1 | |

  - **Low / High Cut are deactivated by default** in Surge's FX plugin. Each has a separate "deactivated"
    toggle that golem's catalogue does not expose. So in golem's rig, `lowcut` / `highcut` most likely
    never acted [I]; to check on the rig.
  - **Weak points** [V]: the pitch is one zero-crossing interval, so a strong 2nd harmonic reads an
    octave high; the only protection is the glide; nothing holds the pitch on a chord.

### Hardware and other software

| Reference | What it does best | What we take |
|---|---|---|
| Diode-ring circuits (Bode, 1960s modulars, early pedals) [I] | Switching sound: a large carrier makes the diodes chop, so odd carrier harmonics add sidebands. Diode mismatch leaks the carrier; units had a balance trim | `model` Diode, `balance` (mismatch → bleed, even products) |
| [J. Parker, *A simple digital model of the diode-based ring-modulator*, DAFx 2011](https://dafx.de/paper-archive/details/wsLCCYWaozH_7uZS9pCYTw) [V] (equations read in the source of [pink's implementation](https://github.com/kunstmusik/pink/blob/master/src/main/pink/effects/ringmod.clj); re-read at the review) | Transformerless ring as static nonlinearities: out = G(c + x/2) − G(c − x/2), each G a pair of opposed diodes, G(v) = D(v) + D(−v). The diode law D: 0 below vb = 0.2, a quadratic knee h · (v − vb)² / (2 vl − 2 vb) up to vl = 0.4, slope h above. Parker suggests heavy oversampling (reported by the Csound port: 32×, 8 – 16× "reasonable" [F]) | The Diode model, with ADAA instead of 32× |
| Hoffmann-Burchardi (DAFx 2008), Bernardini et al. (WDF diode ring, AES 2018) [I] | Full circuit models with transformers | Reference for the bench, not the engine (cost) |
| Moog MF-102 [M] ([vintagesynth](https://www.vintagesynth.com/moog/mf-102-ring-modulator)) | Carrier 2 Hz – 4 kHz (Lo / Hi), **LFO on the carrier 0.1 – 25 Hz, sine / square**, Drive (input level, LED), Mix (dry ↔ RM crossfade), CV / expression on every knob, **carrier in / out jacks** | The LFO on the carrier; the input drive; the sweep from tremolo into ring. An external carrier is planned (§7.10) |
| EHX Frequency Analyzer [M] ([effectsdatabase](https://www.effectsdatabase.com/model/eh/freqanalyzer)) | Shift / Fine / Blend, with "high order filters [that] reduce cross distortions" | Wet low-pass against harshness; the `freq` + `fine` pair (as mda) |
| EHX Ring Thing [M] ([Perfect Circuit](https://www.perfectcircuit.com/electro-harmonix-ring-thing.html)) | Ring, **single sideband upper / lower**, carrier waveforms, a sweepable low-pass, a **tune-to-played-note** hold | `mode` Upper / Lower; `wave` |
| EarthQuaker Data Corrupter [M] ([manual](https://www.earthquakerdevices.com/s/EQD-Data-Corrupter-Manual-R4-WEB.pdf)) | Analog **PLL** locked to a square-fuzzed input. Subharmonic intervals down to 3 octaves, glide or vibrato on the oscillator | **Phase lock** to the played note; glide |
| Fractal Axe-Fx III Ring Modulator [V] (Blocks Guide, Aug. 2022, p. 97) | Classic / SSB Upper / SSB Lower; frequency 2 – 2,000 Hz × **multiplier 0.25 – 4**; **pitch tracking** with the multiplier kept ("very pure-sounding to totally exterminated"); a Bass range; High Cut | Track with an interval; SSB; wet high cut |
| Pigtronix Ringmaster, Subdecay Vitruvian Mod [M], [F] ([Delicious Audio](https://delicious-audio.com/?p=31194)) | Pitch-tracking analog ring mod. Great on single notes; **on chords the tracker "gets confused and jumps"** [F]; out-of-tune carrier when tracking is off | Confidence-gated tracking that **holds** instead of jumping |
| Calf Ring Modulator [I] | Free carrier, detune and L / R phase, two LFOs | `stereo_phase` |
| Airwindows [I] | No dedicated ring modulator | — |

### What players love and hate [F]

From the forum posts and reviews above:

- **Harsh, out of tune.** A fixed carrier is in tune with one note only. That is the classic
  complaint, and the reason for tracking modes, the dry blend and the single-sideband modes.
- **Sweet spots at musical intervals.** Carriers near the note's octave, fifth or unison give bell and
  organ tones. Everything between gives clangour.
- **Carrier bleed** in silence on analog units, cured by a squelch circuit [M]. It is loved as a
  character in some vintage units.
- **Tracking glitches**:
  - octave jumps on decays and chords;
  - a warble while a note is held;
  - a lag on fast passages.
- **The LFO on the carrier** (MF-102) is the most loved move: a slow sweep through the sidebands.
- **Low-end mud.** Difference tones fall under the note, and a tracked carrier at unity makes a DC
  offset that wobbles.

### Two facts that drive the design [I] (analysis)

1. **Aliasing of a ring modulator.** A product x · c contains f_x ± f_c, so an ideal multiply at 48 kHz
   aliases whenever f_x + f_c > 24 kHz.
   - Run at **2×**: the input is band-limited to 24 kHz, the carrier's partials stay below 20 kHz, so
     every sum stays below 44 kHz < 48 kHz, the Nyquist of the 2× rate. Nothing folds at 96 kHz. The
     decimator (gfx-dsp half-band, > 100 dB beyond 25.9 kHz) then removes the sums; what sits in its
     transition band folds back above 22.1 kHz only. A pure multiply needs no more than that.
   - The bounds, at a host rate fs: the input up to 0.46 · fs, the carrier clamp at min(20 kHz, 0.45 · fs).
     Input content in the top 4 % (22.1 – 24 kHz at 48 kHz) leaves an image through the up-sampler; a
     guitar has nothing there.
   - A non-sine carrier must itself be band-limited (no naive square).
   - **Two cases fall outside the argument**: a nonlinear ring (Diode, below) and the **feedback
     loop**, which multiplies again on every pass (f_x ± k · f_c). Both are bounded and benched, not
     alias-free by construction.
2. **Phase lock, not only frequency tracking.**
   - With a carrier at exactly r · f0, the products land at (n ± r) · f0. For r = p / q that is a
     harmonic series of f0 / q: of f0 for r = 1, 2; of f0 / 2 for r = 1/2, 3/2. A small denominator
     is what makes an interval sound "in tune" through a ring.
   - A carrier that only tracks the frequency, with an error δ, puts every product off by δ. A
     few-cent error gives the slow "warble" players hear in tracking pedals, and Treemonster's 24 – 208 ms
     glide adds a lag.
   - Locking the carrier's **phase** to the fundamental (the Data Corrupter's PLL idea, done exactly)
     removes that error: the carrier is r times the string's real first partial, which a period
     estimate only approaches. An equal-tempered fifth (1.4983) still beats against 3/2, which is
     what `tuning` Just solves.
   - **The limit**: a string is slightly inharmonic (partial n sits a little above n · f0), so the
     products of the upper partials still drift slowly against each other, as in any octave pedal.
     Lock removes the tracker's own beat, not the string's. "Stationary" below is exact on a harmonic
     test tone only.

## 2. Design

```
in ─┬─────────────────────────────────────────────── dry (through the oversampler's identity path) ───────────┐
    │                                                                                                         mix (linear crossfade)
    ├─▶ detector: lowcut → highcut → thr gate → YIN (f0, clarity);  in → tracking band-pass at f0 (phase θ)
    │                                  │
    │            carrier = NCO( base · r(pitch) · 2^((LFO + env)/12) ), phase-locked to θ in Track
    │                                  │
    └─▶ ↑2 (↑4 Diode) ─▶ drive ─▶ [ Ring: multiply | Diode: G(c + x/2) − G(c − x/2) | Upper / Lower: SSB ] ◀─ feedback (mda law)
                                          ─▶ ring crossfade (x · c ↔ env · c) ─▶ ↓ ─▶ DC · hpf · lpf ─▶ squelch ─▶ bass_keep ─▶ level ─▶ out
```

### Carrier frequency (both carriers)

- **Base.**
  - Free: f_base = 16,000 · `freq` + 100 · `fine` Hz, the mda law.
  - Track: f_base = the tracked fundamental f0.
- **Interval.** `pitch` (0 – 1 = −60 … +60 st, the Surge law) multiplies the base in **both** carriers:
  - P = −60 + 120 · `pitch`, and r = 2^(P / 12) (Equal), or the just ratio (Just, below).
  - At the default 0.5 (0 st) the mda block is unchanged.
  - This is also the carrier's musical, log-scale sweep: the EXP target for "carrier sweep" in Free.
    `freq` is linear over 16 kHz, so one 7-bit EXP step on it is 126 Hz: too coarse to sweep.
- **Motion**, in semitones, multiplies after the interval (it is never bent by `tuning`):
  - `lfo_depth` · LFO(`rate` / `sync`, `shape`);
  - `env_amount` · the envelope (`env_sens`, `env_att`, `env_rel`).
- **Clamp**: f_c = clamp(f_base · r · 2^(motion / 12), 0, min(20 kHz, 0.45 · fs)).
- **Smoothing.** Frequency parameters are smoothed in the log domain, 20 ms. EXP goes through gfx-dsp
  `exp_smooth`.
- **Oscillator.** An f64 phase accumulator, advanced by the instantaneous frequency: φ += f_c / fs. It
  is never computed as r times an unwrapped phase (a moving r would then swing the phase by r's
  change times the whole elapsed phase). Phase 0 on reset (determinism), so a 0 Hz carrier is silent,
  as in mda.
- **`wave`**: Sine · Triangle · Square, from **band-limited wavetables**.
  - One table per octave, harmonics up to 20 kHz at the table's top frequency, crossfaded between
    neighbouring octaves.
  - Table length at least 4 × its highest harmonic (4,096 points for the lowest octaves, 2,048 above),
    read with a cubic Hermite: a 2,048-point table holding 1,024 harmonics is critically sampled, and
    an interpolated read of it is wrong at the top.
  - Under 20 Hz the lowest table is used, so a square's edges soften as the carrier slows (≈ 80 µs at
    6 Hz): a click-free chop.
  - With every carrier partial below 20 kHz, the 2× argument above holds for every wave.
  - Memory: 2 × 10 tables (the sine needs one), built in `prepare`.

### Track carrier

- **Detector input**: the input through `lowcut` (2-pole high-pass) and `highcut` (2-pole low-pass),
  with Surge's laws and Q 0.707. They feed the pitch detection only (YIN and the onset pre-estimate).
  The level gate and the phase band-pass read the unfiltered input: at its catalogue default, 131 Hz,
  `lowcut` takes 8 dB off a low E's fundamental, which YIN does not mind (the period is unchanged) but
  the phase lock would.
- **Gate (`thr`)**: below `thr` (peak follower on |x|, dB, 6 dB of hysteresis), the tracker holds the
  last pitch. Surge likewise ignores crossings below the threshold.
- **Coarse pitch**: gfx-dsp `detect::PitchDetector`, already in gfx-pitch and gfx-octaver.
  - YIN at ~12 kHz, one analysis every ~2.7 ms, up to 1.5 kHz.
  - **The lowest note follows `lowcut`**: f_min = clamp(lowcut_Hz / 1.75, 30, 150) Hz, through
    `set_range` (buffers sized for 30 Hz in `prepare`). The default 131 Hz gives 75 Hz, the guitar
    range; a bass preset at 0.2 (62 Hz) gives 35 Hz. There is no hidden guitar / bass switch, and
    raising `lowcut` makes the tracker faster, which the player can hear.
  - Harmonic lock is on, so it does not take the 2nd harmonic for the fundamental.
  - A frame is used only if its clarity passes; otherwise the last confident f0 is held.
  - On chords, clarity drops, so the carrier **holds** instead of jumping (the Ringmaster's complaint).
- **Response time.** YIN needs two periods of the lowest note: ≈ 27 ms at f_min 75 Hz, ≈ 57 ms at
  35 Hz. This is a response time on the side chain, not audio latency: the audio is never delayed.
  Until the new pitch is known, the carrier holds the previous one.
- **Onset pre-estimate**, for notes above ≈ 110 Hz only.
  - At an onset (the pick detector of gfx-killswitch, to move to gfx-dsp as `onset`), positive zero
    crossings of the detector signal are timed (Treemonster's own method, interpolated).
  - A provisional f0 is accepted only when **two successive periods agree within 3 %** and fall in
    the detector's range: three crossings, so at most 3 periods. That beats YIN only above ≈ 110 Hz
    (9 ms at E4); below, YIN is as fast and the pre-estimate is skipped. One interval alone is not
    trusted: on an attack it often measures the 2nd harmonic, and a wrong provisional pitch corrected
    20 ms later is worse than a held one.
  - YIN's first voiced frame replaces it, if they differ by more than 50 cents.
- **Glide (`speed`)**: the carrier's frequency follows the target with Surge's τ law (208 → 24 ms), so
  Treemonster presets keep their portamento.
  - With `lock` off, the one-pole glide acts on everything, bends and vibrato included: Treemonster.
  - With `lock` on, it shapes the **move from one note to the next** only. Once the glided frequency
    is within 30 cents of the target it has arrived, and the carrier then follows the note itself
    (YIN's f0 smoothed 10 ms, plus the loop below).
  - `snap` (expert, off by default): a new confirmed note jumps (10 ms) instead of gliding.
- **Phase lock (`lock`, on by default).**
  - **Phase of the fundamental.** Two cascaded band-pass SVFs at the confident f0 (Q 4 each: the 2nd
    harmonic 31 dB down; retuned at control rate). The last SVF's band-pass and low-pass outputs are
    in exact quadrature at every frequency (the low-pass is the band-pass integrated), so they are
    the analytic pair: θ = atan2 of the two. No Hilbert transformer, no added delay.
  - **Loop.** A second-order PLL (PI, damping 0.707) steers the base NCO's frequency so that its phase
    ψ follows θ. Its bandwidth is its own, f_L = clamp(f0 / 8, 8, 30) Hz: it does not come from
    `speed` (1 / (2π τ) would be 1 Hz at the default, too slow for any vibrato). The NCO's centre
    frequency is YIN's f0 (feed-forward); the loop only corrects the remainder, clamped to ± 1 semitone.
  - **Engaging without a chirp.** When lock engages, the current phase difference is taken as the
    loop's reference, so the loop starts at zero error and never pulls the phase in. Phase errors are
    wrapped to ± π and corrected through the frequency only; ψ never jumps.
  - **When it engages**: the glide has arrived, the SVFs have settled (4 periods after a retune), YIN
    is confident, the band-pass holds more than −20 dB of the input's level, and θ's own frequency
    is within 50 cents of YIN's. If any fails, or the loop's correction hits its clamp, the NCO runs
    free at the tracked frequency (still within YIN's ± 2 cents).
  - **Carrier.** φ_c += r · Δψ per sample (Δψ = the base NCO's increment, loop correction included).
  - Result: on a harmonic tone, integer and just ratios give stationary spectra; bends and vibrato
    are followed within the loop's bandwidth.
  - `phase` (expert, 0 – 360°) is the carrier's phase against the fundamental. It matters only when
    **2 r is an integer** (unison, octaves, r = 1/2, 3/2…): there, two products land on each
    harmonic (partial k − r up, partial k + r down) and their phases decide whether they add or
    cancel. Frequency-only trackers cannot offer this.
    - While 2 r is an integer and r is still, φ_c is slewed (50 ms) onto r · ψ₂ + `phase`, with ψ₂ = ψ
      wrapped to 4π. Elsewhere `phase` does nothing and φ_c runs on.
    - The band-pass's and the oversampler's phase at f0 are known in closed form and compensated, so
      `phase` means the same on every note (± 10°, benched).
    - At r = 1 the DC term is (a₁ / 2) · cos(`phase`). The default is **90°**: no DC at unison, so no
      thump on the attacks.
- **`tuning`**: Equal · Just. In Just, an integer P maps to a just interval:
  - 1, 16/15, 9/8, 6/5, 5/4, 4/3, 7/5, 3/2, 8/5, 5/3, 9/5, 15/8, × 2^octave (octave = ⌊P / 12⌋, so
    negative intervals are the same ratios an octave lower);
  - the ratios are chosen for their small denominators (products are harmonics of f0 / q): hence 7/5
    for the tritone, not 45/32;
  - between integers, the interval is interpolated linearly in cents between its two neighbours, so a
    sweep of `pitch` is continuous (a "just ratio × detune" would step by up to 18 cents at each
    semitone).

  It applies to `pitch` in both carriers, not to the LFO or the envelope.

### Modulator (`mode`, `model`)

- **`mode` Ring** with `model` **Clean**: wet = x · c. The ideal four-quadrant multiplier (mda,
  Treemonster).
  - `drive` adds `tanh` saturation of the input before the multiply (MF-102 Drive): 0 – 100 % is
    0 – 24 dB of gain k into tanh(k x) / k, unity for small signals.
  - It runs at 2×. ADAA1 is applied to the **residual** tanh(k x) / k − x, and x passes straight. So
    `drive` 0 is exactly the ideal multiplier, and the linear path does not take ADAA1's roll-off
    (a 2-tap average: −2 dB at 20 kHz at the 2× rate).
- **`mode` Ring** with `model` **Diode**: Parker's model, at **4×** with **ADAA1 on each branch**.
  - wet = G_a(c + x/2) − G_b(c − x/2), with G(v) = D₁(v) + D₂(−v) and D Parker's diode law (vb 0.2,
    vl 0.4, h 1). With matched diodes, G(v) = D(|v|).
  - **ADAA applies as is**: each branch is a memoryless function of one scalar, u = c ± x/2, and ADAA1
    is taken along u. G is piecewise quadratic / linear, so its antiderivative is a closed-form
    piecewise cubic. G is C¹ (only its curvature jumps, at vb and vl), so its harmonics already fall
    by 18 dB / octave. That is what lets 4× + ADAA1 stand in for Parker's 32×; the bench decides (§7.9).
  - ADAA1 costs −0.5 dB at 20 kHz at the 4× rate and 1/8 sample of delay on the wet. Neither is
    compensated.
  - `carrier_level` (−12 … +12 dB, 0 dB = Parker's unit carrier) moves the carrier across the knee.
    - High: the diodes switch, a square-like carrier with odd sidebands.
    - Around −8 … −12 dB the carrier's peak (0.4 … 0.25) rides the quadratic knee: the ring multiplies
      softly, but only on the peaks, and the input opens the diodes as much as the carrier does. A
      starved, gated, level-dependent sound, not a clean multiplier.
  - `drive` scales the input into the ring.
  - `balance` (0 – 100 %) mismatches the diodes **inside** branch a: vb of D₁ × (1 + 0.25 · balance),
    vb of D₂ × (1 − 0.25 · balance). The branch then treats the carrier's two half-cycles
    differently, and the carrier's fundamental leaks: the unbalanced vintage unit.
    - A mismatch between the two branches (the first draft) would not do it. Each G would stay even,
      so the leak would be at 2 · f_c and DC only.
    - The sign of the mismatch only flips the leak's polarity, hence a one-sided range.
  - The output gain is fixed: the f_x ± f_c pair equals Clean's (−6 dB each) for a sine carrier at
    `carrier_level` 0 dB and a small input. It does not follow `carrier_level`, which must stay audible.
- **`mode` Upper / Lower** (single sideband): the analytic input times the quadrature carrier, so a
  single sideband f_x ± f_c, at 2× (the sum stays below 44 kHz, as for Ring).
  - The Hilbert pair is the gfx-dsp `ssb` module that gfx-freqshift brings, run at the 2× rate. **Not
    `pitch::Hilbert` as it stands**: it delays the wrong path, and its image is only −18 dB at 1 kHz
    (found by the gfx-freqshift design, confirmed at this review from its coefficients).
  - The pair shifts the wet's phase against the dry, so a `mix` below 1 colours instead of combing:
    every frequency shifter does.
  - With a tracked carrier, Lower at r = 1/2 is a clean sub-shift, the Ring Thing / Fractal SSB sound.
  - `model`, `wave`, `drive` and `feedback` do not apply (Sine only: an SSB of a square is a frequency
    shifter bank, not planned).
  - Why here and not only in gfx-freqshift: that pedal shifts by a fixed number of hertz; here the
    shift **follows the note** (§7.10).
- **Feedback (`feedback`)**: the mda law, y = (0.95 · feedback · y_prev + x) · c.
  - The loop delay is fixed at 1 / 48,000 s at every rate: two samples at 96 kHz, a Lagrange read
    elsewhere. Run at 2× on the up-sampled input, this is the mda recursion itself while the result
    stays below 24 kHz; the timbre does not change with the rate.
  - **Stability.** |0.95 · feedback · c| < 1, so the loop is stable. But a very slow carrier holds c
    near 1 for seconds, and the loop is then a one-pole with up to 26 dB of gain. A soft limiter in
    the loop (transparent below −1 dBFS) keeps it bounded.
  - **Aliasing.** Each pass multiplies again: pass k reaches f_x + k · f_c, about 6.5 dB lower per pass
    at `feedback` 1. A product folds into the audible band beyond 76 kHz. So the loop is clean
    (≤ −90 dB) for carriers up to ≈ 4 kHz at full feedback, and higher at lower settings. Above
    that it aliases, far less than mda at 1×. Accepted: no filter is added in the loop, which
    would change the mda law.
  - It applies in Ring mode, both models.
- **`ring`** (Treemonster's law): wet = ring · RM + (1 − ring) · env · c.
  - env is a peak follower on |x| (5 ms / 500 ms), Surge's times.
  - At 1 the output is pure ring modulation (the mda block). Lower values blend in a sine voice that
    follows the playing: Treemonster's synth.
  - The voice is gated by `thr`, so a silent input leaves no carrier.

### Output

- **Squelch.**
  - The wet fades out when the input falls below `thr` (the tracker's gate, with its hysteresis), in
    both carriers: 5 ms to open, 100 ms to close, fixed. It does not borrow `env_rel`, which belongs to
    the envelope → carrier follower.
  - This removes the Diode `balance` bleed and the carrier voice in silence: the MF-102 squelch. A
    Clean ring needs none (silence in, silence out).
  - At `thr` 0 (−96 dB) the bleed stays, for those who want it.
- **DC.** A 2-pole 20 Hz high-pass on the wet, always on. A carrier on a harmonic of the note makes a
  DC term that follows the note's envelope; at 5 Hz the attacks would still thump.
- **`hpf`** (wet high-pass, 2-pole, 0 = off – 1,000 Hz) against the low-end mud of difference tones.
- **`lpf`** (wet low-pass, 2-pole, 1 – 20 kHz, 20 kHz = off) against the harshness of sum tones
  (Frequency Analyzer, Fractal High Cut).
- **`bass_keep`** (shared): below it, the signal stays dry, mono and unmodulated. It is a
  Linkwitz-Riley split, the dry band through the same oversampler identity path, as in gfx-emet.
- **`mix`**: the Treemonster law, a **linear crossfade**, dry · (1 − mix) + wet · mix. The dry goes
  through the oversampler's identity path so that a carrier under 20 Hz (tremolo) does not comb against
  the dry. `mix` 1 is the mda block (100 % wet).
  - The dry follows the wet's path: one 2× stage for Clean and SSB, two for Diode, the two dry taps
    crossfaded with `model` (20 ms).
  - At `mix` 0.5 a slow carrier gives 0.5 · x · (1 + c): a true tremolo at the carrier's rate. At
    `mix` 1 it is x · c: twice the rate, with polarity flips.
- **`level`**: output trim, −24 … +12 dB, 0 dB = unity. A sine-carrier RM is about −3 dB RMS below the
  input; it is not compensated, as in both originals.

### Layout, latency, CPU

- **Layout.** Mono 1 / 1 first; stereo 2 / 2 second.
  - It is not stereo by nature: mda and Treemonster are both "the same on both sides" by default.
  - In stereo, the right carrier's phase is offset by `stereo_phase` (0 – 180°; 90° puts the two
    sides' sidebands in quadrature). The carrier is this effect's modulator, so the shared id keeps
    its meaning. The carrier's LFO stays common: both sides keep the same frequency.
  - In Track, one detector runs on L + R.
- **Latency.** **Zero reported.**
  - The minimum-phase oversamplers add a few samples of low-frequency group delay to the wet and the dry
    alike. This is not reported: the gfx-drive / gfx-emet precedent.
  - The pitch detector is a side chain. Its 27 – 57 ms are a response time of the carrier, not audio
    latency; nothing looks ahead.
- **CPU** (estimate, mono, 48 kHz):

  | Path | Estimate |
  |---|---|
  | Free, Clean | ≈ 40 ns / sample (2× half-bands, wavetable, multiply) |
  | Track | + ≈ 90 ns (YIN amortised as in gfx-pitch harmony, two SVFs, atan2, PLL) |
  | Diode | + ≈ 80 ns (4×, two ADAA branches) |
  | Worst case (Track + Diode + LFO + bass_keep) | ≈ 230 ns / sample, ≈ 1.1 % of one core |

  The budget is 300 ns (mono).
- **Real-time.** Tables, the YIN buffers and the oversamplers are sized in `prepare`. Nothing is
  allocated in `process`.

### What it reuses from gfx-dsp

| Module | Used for |
|---|---|
| `oversample` | 2× half-band (cascaded to 4× for Diode); `identity` for the dry and `bass_keep` |
| `adaa` | Diode branches (new shaper: Parker's g, closed-form antiderivative); `drive` tanh |
| `detect` | `PitchDetector` (YIN, harmonic lock, clarity) |
| `ssb` (new, from gfx-freqshift) | the corrected Hilbert pair for Upper / Lower. Until it lands, gfx-ringmod must not use `pitch::Hilbert` |
| `svf` | detector `lowcut` / `highcut`, tracking band-pass and its quadrature pair, DC, wet `hpf` / `lpf`, Linkwitz-Riley for `bass_keep` |
| `envelope` | `PeakFollower` (ring voice 5 / 500 ms, `thr` gate, env → carrier) |
| `lfo`, `transport` | carrier LFO (Sine, Triangle, Square, seeded Random), `sync` from the host position |
| `smooth`, `exp_smooth` | parameters; EXP on `freq`, `fine`, `pitch`, `mix`, `ring`, `lfo_depth`, `rate`, `feedback` |
| `delay` | the feedback loop's fixed 1 / 48,000 s (Lagrange read) |

New code: band-limited wavetable oscillator (worth moving to gfx-dsp, `osc`), the PLL, Parker's shaper
in `adaa` (and the residual form of the tanh). To move into gfx-dsp: gfx-killswitch's pick detector, as
`onset` (it is private to that crate today).

## 3. Parameters

### Decision: ids and units

**Catalogue ids kept, with their catalogue scale (0 – 1) and the original plugins' laws. No preset
conversion.**

- **The contract says so.** It keeps "the catalogue's unit", and lists "the catalogue's own scale (e.g.
  0 – 1)" among real units. These blocks have no other unit in the catalogue (`min` 0, `max` 1, map
  `raw`).
- **Precedent**: gfx-wah and gfx-autowah keep `wah` 0 – 1. Confirmed at the review (§7.1).
- **Real units would buy nothing here.** The contract allows linear ranges only.
  - mda's `freq` is already linear in Hz. A `freq` in Hz would be the same knob, scaled by 16,000.
  - Surge's `lowcut` / `highcut` are **linear in semitones**: logarithmic in Hz, the right feel for a
    filter. Converted to Hz, they would have to become linear in Hz, which is worse.
  - Surge's `pitch` and `thr` are linear in semitones and dB.
- **One preset plays everywhere.** golem may play the block through mda / Surge on one engine and
  through gfx-ringmod on another (golem draws gfx blocks as enamel where golem-fx is absent). Keeping
  the scale lets one stored value mean the same carrier on both, with no per-engine conversion.
- **Real units are shown, not stored.** Each parameter's display string is its real value ("4,850 Hz",
  "+12 st", "131 Hz", "−67 dB", "104 ms"). That is what golem's sheet and golem.fm's AI read in the
  descriptor.

**Display names are join keys.** golem joins its catalogue knobs to ours on the parameter's `name`. So
the catalogue ids carry the catalogue's `name` (`ringmod`: "Freq", "Fine", "Feedback") or, where that
name is Surge's "FX Parameter n", its `label` ("Threshold", "Speed", "Low Cut", "High Cut", "Pitch",
"Ring Mod", "Mix"). Our own wet filters are therefore "Wet Low Cut" / "Wet High Cut" (§7.2).

For the record, the exact laws (golem can display them, and the 6 existing presets convert as below):

| id | Real value | Law |
|---|---|---|
| `freq`, `fine` | carrier Hz | f = 16,000 · freq + 100 · fine |
| `feedback` | loop gain | 0.95 · feedback |
| `thr` | dB | −96 + 96 · thr |
| `speed` | glide τ | τ = 1 / ((10⁻⁴ + speed⁴ · 0.0999 / 128) · 48,000) s |
| `lowcut`, `highcut` | Hz | 440 · 2^((−60 + 130 · v) / 12) |
| `pitch` | st | −60 + 120 · pitch |
| `ring`, `mix` | fraction | as is |

| Preset | Real values |
|---|---|
| ringmod/standard | 4,850 Hz |
| ringmod/bell-tone | 8,862 Hz |
| ringmod/robot-voice | 1,950 Hz, loop gain 0.33 |
| treemonster/ring-tracker | −67 dB, τ 104 ms, 131 Hz – 2.64 kHz, 0 st, ring 0.6, mix 0.5 |
| treemonster/octave-monster | +12 st, τ 87 ms, ring 0.8, mix 0.55 |
| treemonster/sub-growl | −62 dB, τ 140 ms, 62 Hz – 1.24 kHz, −12 st, ring 0.7 |

**The ringmod presets look mis-authored.**
- "Robot voice" at 1.95 kHz and "Standard" at 4.85 kHz are far above the classic ranges (30 – 50 Hz for a
  robot voice; a few hundred Hz for a musical ring). "Bell tone" at 8.86 kHz is a whistle, not a bell.
- Their authors probably read `freq` as a musical 0 – 1 knob.
- §4 re-authors them under the same preset ids (§7.4). The new values use the same law, so they are
  also right where the block still plays through mda.

### Front panel

| id | Name | Type | Range | Catalogue | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `carrier` | Carrier | e | Free · Track | new | Free | xfade 20 ms | | Which oscillator. golem sets Track at insert for `surge-treemonster` |
| `freq` | Freq | f | 0 – 1 (0 – 16 kHz, linear) | `ringmod` | **0.025 (450 Hz with `fine`)** | log 20 ms | ✓ | Free carrier, coarse (mda law). Inactive in Track |
| `fine` | Fine | f | 0 – 1 (+0 – 100 Hz) | `ringmod` | 0.5 | log 20 ms | ✓ | Free carrier, fine. Inactive in Track |
| `pitch` | Pitch | f | 0 – 1 (−60 … +60 st) | `surge-treemonster` | 0.5 (0 st) | 20 ms | ✓ | Carrier = base × interval, in both carriers; the musical sweep |
| `ring` | Ring Mod | f | 0 – 1 | `surge-treemonster` | **1** | 20 ms | ✓ | 1 = pure ring modulation; lower blends the carrier as a sine voice |
| `feedback` | Feedback | f | 0 – 1 | `ringmod` | 0 | 20 ms | ✓ | mda's recursive ring (bounded), cascaded sidebands. Ring mode only |
| `mix` | Mix | f | 0 – 1 | `surge-treemonster` | **1** | 20 ms | ✓ | Linear crossfade dry ↔ wet (catalogue law) |

### Expert

| id | Name | Type | Range | Catalogue | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `thr` | Threshold | f | 0 – 1 (−96 … 0 dB) | `surge-treemonster` | 0.3 (−67 dB) | — | | Tracking gate (Track) and squelch (both carriers) |
| `speed` | Speed | f | 0 – 1 (τ 208 → 24 ms) | `surge-treemonster` | 0.5 (140 ms) | — | | Carrier glide (Surge law); with `lock`, between notes only |
| `lowcut` | Low Cut | f | 0 – 1 (13.8 Hz – 25 kHz) | `surge-treemonster` | 0.3 (131 Hz) | 20 ms | | Detector high-pass (detection only); also sets the lowest tracked note (÷ 1.75) |
| `highcut` | High Cut | f | 0 – 1 (13.8 Hz – 25 kHz) | `surge-treemonster` | 0.7 (2.64 kHz) | 20 ms | | Detector low-pass (detection only) |
| `mode` | Mode | e | Ring · Upper · Lower | new | Ring | xfade 20 ms | | Ring, or one sideband |
| `model` | Model | e | Clean · Diode | new | Clean | xfade 20 ms | | Ideal multiplier or Parker diode ring (Ring mode) |
| `wave` | Carrier Wave | e | Sine · Triangle · Square | new | Sine | xfade 20 ms | | Band-limited carrier (Ring mode) |
| `tuning` | Tuning | e | Equal · Just | new | Just | xfade 20 ms | | Just ratios make tracked intervals beat-free |
| `lock` | Phase Lock | b | off / on | new | on | — | | Track: carrier phase-locked to the fundamental (the loop engages and releases by itself, without a step) |
| `snap` | Snap | b | off / on | new | off | — | | Track: new notes jump instead of gliding |
| `phase` | Lock Phase | f | 0 – 360° | new | 90° | 20 ms | | Track + lock, at unison, octaves and half ratios: which harmonics add or cancel. 90° = no DC at unison |
| `drive` | Drive | f | 0 – 100 % | new (shared) | 0 % | 20 ms | | Input saturation, 0 – 24 dB into tanh (Clean); input into the ring (Diode) |
| `carrier_level` | Carrier Level | f | −12 … +12 dB | new | 0 dB | 20 ms | | Diode: starved and gated ↔ hard switching |
| `balance` | Balance | f | 0 – 100 % | new | 0 % | 20 ms | | Diode mismatch: carrier leak |
| `rate` | LFO Rate | f | 0.1 – 25 Hz | new | 1 Hz | 20 ms | ✓ | Carrier LFO (MF-102 range) |
| `lfo_depth` | LFO Depth | f | 0 – 24 st | new | 0 st | 20 ms | ✓ | Peak carrier sweep |
| `shape` | LFO Shape | e | Sine · Triangle · Square · Random | new | Sine | — | | Square = trill between two carriers; Random = seeded S&H, slewed 2 ms |
| `sync` | LFO Sync | e | shared list (Off · 4/1 … 1/32) | new (shared) | Off | — | | Rate from the host position |
| `stereo_phase` | Stereo Phase | f | 0 – 180° | new (shared) | 0° | 20 ms | | R carrier phase offset (stereo layout) |
| `env_amount` | Env → Carrier | f | −48 … +48 st | new | 0 st | 20 ms | | Picking moves the carrier |
| `env_sens` | Env Sensitivity | f | 0 – 100 % | new (shared) | 50 % | — | | |
| `env_att` | Env Attack | f | 1 – 100 ms | new (shared) | 5 ms | — | | |
| `env_rel` | Env Release | f | 20 – 2,000 ms | new (shared) | 200 ms | — | | |
| `hpf` | Wet Low Cut | f | 0 (off) – 1,000 Hz | new | 0 | 20 ms | | Wet high-pass: difference-tone mud |
| `lpf` | Wet High Cut | f | 1,000 – 20,000 Hz | new | 20,000 (off) | 20 ms | | Wet low-pass: sum-tone harshness |
| `bass_keep` | Bass Keep | f | 0 (off) – 500 Hz | new (shared) | 0 | 20 ms | | Below it: dry, unmodulated |
| `level` | Level | f | −24 … +12 dB | new | 0 dB | 20 ms | | Output trim |

Read-only: `carrier_hz` (the carrier's frequency now, 0 – 20,000 Hz), so golem can show a tracked carrier;
`state` (shared: 0 = free or holding, 1 = tracking, 2 = locked).

Notes:

- **Defaults differ from the catalogue in three places**, each set by golem at insert per block:
  - `freq`: 0.025 here, catalogue 0.3 = 4.85 kHz, harsh on a guitar;
  - `ring`: 1 here, catalogue 0.5;
  - `mix`: 1 here, catalogue 0.5.

  The plugin's defaults are the `ringmod` block's (pure, 100 % wet). `ring` and `mix` are not ids of
  that block, so golem never writes them for it: their defaults **must** be 1, or the mda block would
  not be itself. For `surge-treemonster`, golem writes `carrier` Track, `ring` 0.5, `mix` 0.5 at
  insert. `carrier` is not a catalogue id: it has to be in golem's engine table (`init`) for that
  block, or the block plays a free 450 Hz ring.
- **"Inactive" parameters are dependencies, not mode-dependent defaults.** Examples: `freq` / `fine` in
  Track; `model` / `wave` / `drive` / `feedback` in Upper / Lower; `carrier_level` / `balance` in
  Clean; `lock` / `snap` / `phase` / `speed` / the detector in Free. They keep their values and grey
  out in golem (`depends`). `thr` stays live in Free: it is the squelch.
- **`tuning` defaults to Just**, the second option of its enum. No other parameter depends on it, and
  it equals Equal at unison and octaves (§7.6).
- **Enum orders are final** and append-only.

## 4. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Classic ring-mod solo (1970s heavy rock, fusion) | Free, `freq` 0.03 (≈ 530 Hz), `mix` 0.7, `lpf` 6 kHz | Band-limited, alias-free sum tones; wet low-pass |
| Bells / gong on arpeggios | Free, `freq` 0.06 (1 kHz), Diode, `carrier_level` +3 dB, `mix` 0.6 | Diode switching sidebands, ADAA at 4× |
| Robot / transmitter voice | Free, `freq` 0.0015 + `fine` 0.15 (≈ 39 Hz), Clean, `feedback` 0.3, `hpf` 150 Hz | Low carrier + mda feedback; voice lane |
| Tremolo melting into clangour | Free, `freq` 0, `fine` 0.06 (6 Hz), `mix` 0.5, EXP on `pitch` 0.5 → 0.9 (6 → 96 Hz) | `pitch` as a log sweep; at `mix` 0.5 the slow end is a true tremolo |
| Carrier LFO sweep (MF-102 move) | Free, `freq` 0.02, `rate` 0.3 Hz, `lfo_depth` 12 st, `mix` 0.8 | LFO in semitones on the carrier |
| Trill between two carriers | Free, `shape` Square, `sync` 1/16, `lfo_depth` 7 st | Synced square LFO |
| Harmonic ring lead (in tune everywhere) | Track, `pitch` 0.5 (unison), `lock` on, `snap` on, `ring` 1, `mix` 0.6 | Phase-locked: octave + harmonics, no tracker warble |
| Octave-up synth | Track, `pitch` 0.6 (+12 st), `ring` 0.8, `mix` 0.55 | Treemonster's "Octave monster", without the tracker's beat |
| Hollow sub (square-ish octave down) | Track, `pitch` 0.4 (−12 st), `snap` on, `hpf` 40 Hz, `mix` 0.5 | r = 1/2: odd harmonics of f0/2 |
| Fifth organ | Track, `pitch` 0.5583 (+7 st), Just, `snap` on, `ring` 0.7 | Just 3/2: harmonics of f0/2, organ-like |
| Clean sub shift | Track, `mode` Lower, `pitch` 0.4 (−12 st), `snap` on, `mix` 0.5 | SSB: only the lower sideband, a shift that follows the note |
| Lock-phase colour | Track, `pitch` 0.5, `ring` 1, `mix` 1, sweep `phase` 0 → 180° | Harmonics add or cancel: a comb only a locked carrier has |
| Carrier in the song's key | Free, `freq` + `fine` computed by golem from the key's tonic or fifth | No new parameter: f = 16,000 · freq + 100 · fine |
| Synth glide lead | Track, `snap` off, `speed` 0.3 (≈ 195 ms), `ring` 0.3 | Treemonster portamento; sine voice |
| Picking-swept carrier | Free, `freq` 0.02, `env_amount` +24 st, `env_sens` 60 % | Envelope → carrier |
| Broken radio | Free, Diode, `balance` 60 %, `carrier_level` −9 dB, `thr` 0 (bleed on), `lpf` 3 kHz | Mismatch leak, starved diodes, squelch off |
| Bass ring without mud | Track, `bass_keep` 120 Hz, `pitch` 0.6 | Fundamental kept dry and mono |

Factory presets (`presets/gfx-ringmod.json`, the source of truth; names without artists or brands):
- `ringmod`: "Ring solo", "Bell tower", "Transmitter", "Tremolo to clang", "Slow sweep", "Broken radio";
- `surge-treemonster`: "Harmonic ring", "Octave synth", "Hollow sub", "Fifth organ", "Glide lead",
  "Sub shift".

The six existing preset ids are stable and stay. The three Treemonster ones keep their values. The three
`ringmod` ones are re-authored under their ids (§7.4): `standard` → 450 Hz (`freq` 0.025, `fine` 0.5),
`robot-voice` → 39 Hz with `feedback` 0.3, `bell-tone` → 1 kHz (`freq` 0.06, `fine` 0.5).

## 5. Bench suite (on top of gfx-bench's common checks)

Targets are computed in the tests from the laws above (mda, Surge, Parker), not copied by hand.

| Check | Pass |
|---|---|
| mda law | Free, Clean, `feedback` 0, `mix` 1, sine in: output spectrum = f_x ± f_c with f_c = 16,000 · freq + 100 · fine, each within 0.1 dB of −6 dB; everything else ≤ −90 dB |
| mda feedback | `feedback` 0.35, 450 Hz carrier, at 48 kHz: matches the mda recursion (reference implementation in the test) within 0.5 dB, 20 Hz – 10 kHz; at 44.1 and 96 kHz the same within 1 dB (fixed loop delay) |
| Feedback, bounded and clean | `feedback` 1, 0.1 Hz carrier, 0 dBFS in: output ≤ +6 dBFS, no NaN. `feedback` 1, carriers ≤ 4 kHz: aliases ≤ −90 dB; above: reported in `alias-grid` |
| Surge laws | Unit tests: `pitch`, `lowcut` / `highcut`, `thr` and `speed` map to the values of §3 within 0.1 % (τ measured on a step of f0) |
| Alias, Clean | 15 kHz sine × 16 kHz carrier: only 1 kHz below 20 kHz; aliases ≤ −100 dB. Square carrier at 3 kHz × 10 kHz sine: ≤ −90 dB. The same at 44.1 kHz. `drive` 0: bit-identical to the plain multiply |
| SSB | Upper and Lower, sine carrier 50 Hz – 5 kHz, sine in 60 Hz – 15 kHz: the other sideband ≤ −40 dB; +`pitch` in Upper goes up |
| Alias, Diode | −12 dBFS 5 kHz sine, 2 kHz carrier, `carrier_level` +6 dB: aliases ≤ −80 dB re the strongest product (with vs without ADAA reported). `alias-grid` up to a 10 kHz carrier at +12 dB decides ADAA1 / ADAA2 / 8× (§7.9) |
| Diode model | Static check against Parker's equations (no ADAA, 64× reference) within 0.5 dB on the first 8 products |
| Balance / squelch | Diode, silence in: no carrier at `balance` 0 (≤ −100 dBFS); at 50 % and `thr` 0 the bleed is at **f_c** (not only 2 · f_c); at `thr` 0.3 it is ≤ −100 dBFS 200 ms after the input stops |
| Tracking accuracy | Sines 41 Hz (bass, `lowcut` 0.2) – 1.5 kHz and guitar DIs: settled carrier within ±2 cents of r · f0 |
| Phase lock | Track, `pitch` 0.6 and 0.5583 (Just), a harmonic test tone at A3 (10 partials): non-harmonic energy ≤ −60 dB, and carrier / f0 exact within 0.1 cent; `lock` off for comparison. A held A3 from a DI: reported (the string's inharmonicity sets the floor) |
| Lock engage | Note onsets on the DIs: no carrier excursion > 10 cents when the loop engages or releases; no phase step |
| Lock phase | Harmonic test tone, r = 1, E2 – E5: wet DC at 90° at least 15 dB below its value at 0°; the measured carrier phase within ±10° of `phase` |
| Just law | `pitch` swept in 0.001 steps: the ratio is continuous, and exact at the twelve integers |
| Bends | ±50 cents vibrato at 6 Hz: carrier error ≤ 5 cents from A2 up and ≤ 10 cents at E2 (lock); the glide's lag (no lock) |
| Response | Note change E2 → A2 → E4, `snap` on: E4 within 20 cents in ≤ 3 periods + 2 ms (11 ms, onset pre-estimate); E2 and A2 within 10 cents in ≤ 30 ms (YIN). No provisional pitch more than 1 semitone wrong on the DIs |
| Decays and chords | low E decaying to silence; strummed chords (crunch.wav): no carrier jump > 1 semitone per 500 ms; octave errors counted = 0 on the decay |
| DC | Track, unity, lock on, `phase` 0° (worst case): wet below 10 Hz ≤ −60 dB re the wet's RMS |
| Mix law | `mix` 0.5: output = 0.5 dry + 0.5 wet (linear) within 0.05 dB, dry phase-aligned (null ≤ −60 dB with wet muted) |
| Bass keep | Flat sum within 0.1 dB across the crossover at `ring` 0 / `mix` 0 |
| EXP | 128-step sweeps of `pitch` and `freq`: control sidebands ≤ −80 dB, no audible steps |
| Determinism, CPU | Strict null; worst case ≤ 300 ns / sample mono at 48 kHz |
| Listening | clean, lead-clean, funk, lead DIs, the six existing presets and the twelve proposed ones; level-matched A / B against mda RingMod and Surge Treemonster rendered in REAPER (both Linux, GPL) |

gfx-bench additions:
- `--trace` for `render`: a CSV of f0, clarity, carrier Hz and lock state per analysis frame, for
  plotting tracking glitches;
- an `alias-grid` sweep (input × carrier frequencies → worst alias, as a table);
- the same two A / B references used for the listening check.

## 6. Visual

- **Category**: modulation.
  - Its primary block, `ringmod`, is catalogued as modulation. Treemonster is "pitch", but its sound is
    a modulation.
  - The finish follows the family: **anodized**.
- **Crate hue: brass, `#c9b000`** (METAL_CRATE_HUE).
  - The ring modulator's sound is bell metal.
  - It sits in the free gap of the palette between utility amber (`#f0a500`, ≈ 41°) and glitch lime
    (`#72c000`, ≈ 84°), at ≈ 53°. It is 12° from the amber, but darker, and the utility family is
    thrash where this pedal is anodized.
  - It sets it apart from the green of chorus / vibrato / tremolo, which it would otherwise share.
  - Its luma selects dark ink by itself.
- **Motif `sidebands`**: the product of two sines, engraved above and below the rune.
  - A fast sine (period ≈ 9 px) whose amplitude follows a slow sine across the width (period ≈ W / 1.5),
    so it pinches to zero and swells twice: the ring-modulated waveform.
  - Its two envelope curves (± |slow sine|) are drawn alongside at opacity 0.4.
  - Drawn as polylines sampled every 1 px, like `waves`.
  - In thrash it would take the yellow-on-black stroke, like every line motif.
- **Face** (`faces.json`):

  ```json
  "gfx-ringmod": { "name": "Ring", "blocks": ["ringmod", "surge-treemonster"],
                   "knobs": ["freq", "pitch", "ring", "mix"], "selectors": ["carrier"],
                   "category": "modulation", "color": "#c9b000" }
  ```

  Four knobs and one selector, the gfx-octaver layout.
- **Glyph** (24 × 24, single colour, stroke 1.8, round caps, the style of `glitch.svg`): **a ring
  holding a beat**.
  - A circle, `cx 12 cy 12 r 9`.
  - Inside it, the ring-modulated wave: one polyline `M5.5 12 L8 10 L10.5 15 L13.5 8.5 L16 14 L18.5 12`.
    It is a carrier zig-zag whose amplitude swells to the middle and pinches at both ends.
  - Five strokes, 2.5 – 3 px apart. The first draft had nine strokes 1.5 px apart under a 1.8 px pen:
    a filled lens at 24 px and a blot at 16 px. Its envelope curves are dropped for the same reason.

  It replaces `ringmod.svg` (a circle and an asterisk) as the `ringmod` block's glyph, hence the pedal's
  rune; `surge-treemonster` keeps its own (§7.11). It carries no logo, maker name or livery.

## 7. Decisions on the open questions (review of 2026-11-14)

1. **Ids and units: confirmed.** The ten ids stay on the catalogue's 0 – 1 scale with the original laws.
   - The contract names "the catalogue's own scale (e.g. 0 – 1)" as a real unit, and the catalogue
     gives these ids nothing else (`min` 0, `max` 1, map `raw`). gfx-wah and gfx-autowah set the
     precedent with `wah`.
   - Hz, st or dB under the same ids would change the unit of a catalogue id, which the contract
     forbids, and would force the cut-offs to be linear in Hz.
   - The cost is `freq`: 16 kHz on one linear knob, the musical part in its first 6 %. `pitch` (log)
     and `fine` are the playable controls; knob feel is golem's UI's business.
2. **Join keys.** golem joins on the parameter's `name`. Decided here: our names equal the
   catalogue's `name` for `ringmod` ("Freq", "Fine", "Feedback") and the catalogue's `label` for
   `surge-treemonster`. For that block golem-rack has to join on `label` or on id, since
   "FX Parameter n" cannot be ours. **Open on golem's side.**
3. **Plugin defaults: accepted.** `ring` 1 and `mix` 1 are required (see the notes of §3). `freq`
   0.025 only shows in a bare host, since golem writes the catalogue's value at insert. golem's
   `ringmod` default should move from 0.3 (4.85 kHz) to 0.025 (450 Hz): **The maintainer's call**, golem-side.
4. **Re-authoring: yes, all three `ringmod` presets, under their ids** (values in §4). Preset ids are
   stable forever; their values are not. **The maintainer's ear** confirms the three sounds.
5. **Low / High Cut: active, catalogue defaults kept (0.3 / 0.7).** They are catalogue ids, so they
   must act. They are made safe: they feed the pitch detection only, never the phase band-pass, and
   `lowcut` also sets the lowest tracked note. "Off" defaults (0 / 1) would leave hum and pick noise
   in the detector and slow it to its 30 Hz window.
6. **`tuning` Just by default: accepted.** Nothing depends on it; it equals Equal at unison and
   octaves, which is all the existing presets use. Two fixes: the law is continuous between
   semitones, and the tritone is 7/5.
7. **`snap` off by default: kept.** The portamento is the Treemonster block's character and its three
   presets rely on it. The new "in tune" presets set `snap` on. **The maintainer's ear** may reverse it.
8. **PLL: redesigned, and still the main risk.** Own bandwidth, feed-forward from YIN, engage without
   pull-in, quadrature from the SVF, strict engage conditions (§2). **Prototype it in gfx-bench first**,
   on the DIs with `--trace`, before the ids are frozen. If it fails the "Lock engage" and "Bends"
   checks, ship frequency-only tracking and drop `lock` and `phase` rather than ship dead ids.
9. **Diode: 4× + ADAA1 is the plan.** ADAA is valid here (one scalar per branch) and G is C¹. If
   `alias-grid` misses −80 dB at high carriers and levels, go to **ADAA2 first** (the second
   antiderivative is a closed-form quartic, the same 4×), and to 8× only after that.
10. **Scope.**
    - **SSB stays.** It costs one shared module, the Ring Thing and Fractal's block both have it, and
      here the shift follows the played note, which gfx-freqshift (a shift in hertz, with a loop)
      does not do. The two pedals overlap only on a free-carrier shift.
    - **"Tune to played note": not added.** Track with `thr` raised already holds the last note; a
      `hold` switch can be appended later.
    - **Song key: no parameter.** golem can compute `freq` + `fine` from the key (§4).
    - **External carrier** (MF-102's carrier input, one lane ringing another): planned as a third
      `carrier` option on an auxiliary bus, once golem routes key inputs. The enum is append-only.
11. **Visual: accepted with two changes.** Brass and anodized are fine. The glyph is simplified (§6)
    and replaces `ringmod.svg` for the block, since the pedal's rune is its first block's glyph;
    `surge-treemonster` keeps its glyph. To announce to golem-rack with the face.

## 8. As coded (v0.1.0, 2026-11-15)

Files: `src/lib.rs` (the processor and `PARAMS`), `src/osc.rs` (the wavetables), `src/plugin.rs` (VST3
`GoleMFxRingmod01`, "GoleM FX Ringmod", mono 1 / 1 first then 2 / 2, latency 0, read-only meters
`carrier_hz` and `state`), `src/tests.rs`. Shared: gfx-dsp `track.rs` (new), `adaa.rs` (`Antiderivative`,
`Adaa1Fn`, `TanhResidual`, `ParkerDiode`, `DiodePair`: added, nothing existing changed).

### The tracker (gfx-dsp `track`, gfx-sustainer §2.7's API)

`prepare(sample_rate, max_partials)`, `reset`, `set_f0(hz, confident)`, `set_floor(dbfs)`,
`engage(Snap | Keep)`, `process(x) → Lock { psi, inc, level, locked }`, `partial(k)`, `hold()`, plus
`set_ff_time`, `set_partials`, `set_reference` (the weak-fundamental rule), `resume` (lifts `hold`'s
latch) and `reference()`. Partials k ≥ 2 run at k · ψ + φ_k with a ±15 ¢ trim ratio on a 4 Hz loop
(four one-poles at 0.4 f0), as specified; they are unit-tested on an inharmonic string (B = 3·10⁻⁴,
within 0.2 ¢) but unused here (K = 0).

Two deviations from §2.7, both measured on synthetic harmonic tones at 48 kHz (feed-forward 25 ms late,
3 ¢ off):

| Specified | Coded | Why (numbers) |
|---|---|---|
| Phase detector low-passed by two one-poles at f0 / 3, loop bandwidth clamp(f0 / 5, 14, 50) Hz | A **one-period moving average** of the demodulated signal (a comb with a zero on every harmonic, half a period of delay), bandwidth **clamp(f0 / 2.5, 14, 100) Hz** | With the one-poles a ±50 ¢ 6 Hz vibrato left 38 ¢ of error at E2 and 22 ¢ at A2 (bench asks 10 / 5); with the comb 8.0 / 4.3 ¢, 1.1 ¢ at A3. Bends of 200 ¢ in 150 ms: 9.5 ¢ (E2), 7 ¢ (A2), 1.8 ¢ (E4) |
| Feed-forward + loop correction (±1 st) | Once engaged the integrator holds the **whole** frequency; the feed-forward only bounds it (±1 st) and runs the NCO before the loop engages | YIN's f0 trails a vibrato by its window: added inside the loop it is a disturbance as large as the vibrato (lock was lost on every cycle at ±50 ¢) |

Also: the engage test uses the error's circular variance over **10 ms** (< 0.02): over 50 ms a
feed-forward 6 ¢ off rotates the error enough to never engage; the 50 ms variance still decides the loss
(> 0.125). A new note is a jump of more than 50 ¢ **between successive readings** (a vibrato moves a few
cents per reading).

### Deviations in gfx-ringmod

| Spec | Coded | Why |
|---|---|---|
| Phase lock by the band-pass pair and its own PLL (§2) | gfx-dsp `track` (the README's closing note) | The pair lagged a vibrato by 2.5 periods |
| "gfx-ringmod always uses Keep" (gfx-sustainer §2.2) | **Snap** | ψ is a reference here, never heard: the carrier accumulates r · `inc` and is pulled onto its `phase` target by itself. Keep biases `inc` by up to 5 ¢ for seconds on low notes while it pulls the offset in (4 s at E1): an audible beat, the warble the lock removes |
| `phase`: the carrier slewed onto its target in 50 ms | 50 ms time constant, **at most 8 ¢** of carrier frequency | A 50 ms slew of half a turn is a 10 Hz chirp (200 ¢ at E2); 8 ¢ keeps the "Lock engage" bound (≤ 10 ¢) |
| At an onset the tracker keeps its loop | The pick detector calls `track::hold()` until the new note is named | The loop otherwise follows the attack (it dragged the carrier 110 ¢ the wrong way at E2 → A2) |
| Pick detector: fast / slow envelope difference | Same, on the **unfiltered** input, the slow follower a peak follower (10 ms up, 200 ms down) | Through `lowcut`'s skirt a low note's vibrato read as picks; a mean follower reads a steady note's crest factor as one |
| Onset pre-estimate: two agreeing zero-crossing periods | Same, crossings through a **Schmitt trigger** (armed under −30 % of the level) | Plain crossings read the harmonics' wiggles (E2 read as 544 Hz) |
| `snap`: "jumps (10 ms)" | A straight line in log frequency, 10 ms long | A 10 ms one-pole took 60 ms to come within 10 ¢ from far away |
| Gate: peak follower | Sliding maximum over 35 ms (one period of 30 Hz) | Ripple-free on low notes, and silence is seen 35 ms after the input stops |
| Squelch: open 5 ms / close 100 ms | Linear ramps of 5 / 100 ms, to exactly 0 | A one-pole never reaches 0: the leak was still −62 dB at 200 ms (bench row: ≤ −100 dBFS) |
| `drive` 0 – 100 % into tanh(k x) / k | The residual is faded in over the first 5 % | tanh(x) at 0 dB already saturates: a step at 0 → 0.1 % |
| Diode `drive` "scales the input into the ring" | Input × k, output ÷ k | Small-signal sidebands stay at Clean's −6 dB whatever the drive |
| SSB level (unspecified) | One sideband at 1 / √2 | The power of the ring's two sidebands |
| SSB `zero_cut` (unspecified) | Off | A Lower shift past 0 Hz folds back, as on the hardware |
| Mix's dry: always through the oversampler | Computed only while heard (`mix` < 1 or `bass_keep`); it shares the wet's up-samplers | CPU (−32 ns); the up-samplers are the same filters on the same input |
| Bass keep's low band "mono" | The mean of the channels' dry taps | — |
| `carrier_level`, `balance`, `phase`, etc. smoothed 20 ms | As specified; `freq` / `fine` in log(f + 1), `pitch` in cents (20 ms after the EXP smoother) | log(f + 1) lands exactly on 0 Hz |

### Bench rows (§5) as measured

| Row | Result |
|---|---|
| mda law | products −6.02 ± 0.1 dB, spurs ≤ −90 dB, 44.1 / 48 / 96 kHz (three carriers) |
| mda feedback | within 0.5 dB at 48 kHz, 1 dB at 44.1 / 96 kHz (third-octave bands 50 Hz – 8 kHz, noise under 4 kHz so mda's own aliasing does not enter) |
| Feedback bounded | `feedback` 1, 0.1 Hz carrier, 0 dBFS: peak ≤ +6 dBFS, Clean and Diode |
| Surge laws | `pitch`, `thr`, cut-offs, `speed` (208 / 140 / 104 / 24 ms) exact (unit tests) |
| Alias, Clean | 15 × 16 kHz: ≤ −100 dB; square at 3 kHz × 10 kHz: ≤ −90 dB; 44.1 and 48 kHz |
| SSB | other sideband ≤ −40 dB (carriers 50 Hz – 5 kHz, inputs 60 Hz – 15 kHz) |
| Alias, Diode | ≤ −88.6 dB re the strongest product (5 kHz, 2 kHz, +6 dB). ADAA1 at 4× suffices: ADAA2 / 8× not needed |
| Diode model | first 8 products within 0.5 dB of Parker's equations at 64× |
| Balance / squelch | matched: ≤ −100 dBFS; 50 %: leak at f_c −38 dBFS (2 f_c −80); `thr` 0.3: ≤ −100 dBFS 200 ms after the input stops |
| Tracking accuracy | 41 Hz (`lowcut` 0.2) – 1.5 kHz sines: within 2 ¢ without the lock, 0.1 ¢ with it |
| Phase lock | A3 × 2: −103.8 dB non-harmonic, × 3/2 (Just): −97.7 dB, carrier exact (0.000 ¢); lock off −24 dB |
| Lock phase | carrier within +0.9° (E2) … +5.0° (E5) of `phase` |
| Lock engage / Response | E2 → A2 → E4 with `snap`: E2 within 10 ¢ after 28 ms, A2 (after a ringing E2) 35 ms, E4 within 20 ¢ after 22 ms; then never more than 8.5 / 9.6 / 17.6 ¢ off during the note. **Misses** the row's 30 ms (A2) and 3 periods + 2 ms (E4: the pre-estimate answers in 3 periods, then `snap`'s 10 ms ramp) |
| Bends | ±50 ¢ 6 Hz: lock 7.8 ¢ (E2), 4.3 (A2), 1.1 (A3), 0.8 (E4); no lock 52 – 53 ¢ |
| DC | unity, `phase` 0°: wet under 10 Hz ≤ −60 dB re its RMS |
| Mix law, dry | 0.5 = the linear crossfade (< −100 dB); the dry nulls against the oversampler's identity (< −60 dB) |
| Bass keep | flat within 0.1 dB, 40 Hz – 5 kHz around 120 Hz |
| Stereo phase | the right products offset by `stereo_phase` (±1°) |
| Common checks | 28 / 28. Latency exempt (the wet does not correlate with the input; the minimum-phase oversamplers are not reported, the gfx-drive precedent); click bound −40 dB: `freq` or `pitch` end to end sweeps the carrier 10 octaves in 20 ms, a chirp of −45 dB above 10 kHz, not a click |
| CPU (mono, 48 kHz) | Free / Clean 121 ns, Upper 185 ns, Track + lock 263 ns, Track + Diode + LFO + `bass_keep` at mix 0.6 **422 ns** (budget 300). One pass of gfx-dsp's f64 half-band costs 16 ns and Diode needs eight (wet 4× and its dry); the tracker costs 85 ns, YIN 28 ns (`lowcut` 0.3) and 118 ns at a 30 Hz floor (`lowcut` 0.2). An f32 / SIMD oversampler and a decimated YIN for bass (gfx-tuner's) are the ways down |

Not run (need recordings or golem's rig): the DI rows (decays and chords on crunch.wav, the held A3
from a DI, octave errors on decays, "no provisional pitch more than 1 st wrong on the DIs"), the
`--trace` and `alias-grid` bench additions, the EXP control-sideband row, and the listening A / B against
mda RingMod and Surge Treemonster.

### Left for the owner

- **By ear**: `snap` default (off), `phase` default 90°, the Just table (7/5, 9/5), the re-authored
  `standard` / `robot-voice` / `bell-tone` and the 18 presets (`presets/gfx-ringmod.json`).
- **On DIs**: the tracker on real strings (lead-clean bends and vibrato, a bass DI), chords and decays.
- **CPU** over budget in the worst case (above).
- golem-side items of the Review's "Still open" table are unchanged (engine table `init` `carrier` =
  Track for `surge-treemonster`, the join on `label`, the `ringmod.freq` catalogue default, the glyph).

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good.** Strong on the free carrier, the Diode model and the parameter contract; **at risk on
the phase lock**, which is new ground and must be prototyped before its ids ship. Rivals: mda and
Treemonster are surpassed on paper (alias-free, a tracker that holds); MF-102 and Fractal's block are
covered except the external carrier; nothing on the market phase-locks a ring carrier, so that part has
no reference to copy and no proof yet.

### Verified

- mda's law and recursion, the six presets' real values, Surge's glide law and τ values: recomputed,
  correct. The cut-off, pitch and threshold values follow from the ranges quoted from `Parameter.cpp`,
  which were not re-read.
- Treemonster's detector, glide, envelope times and mixes: re-read in `TreemonsterCore.h`, correct.
- Parker's diode law and network: re-read in pink's source, correct. The "32×" is second-hand (Csound
  list); the paper itself could not be fetched.
- The 2× claim: correct, with the bounds now stated (input up to 0.46 · fs; half-band > 100 dB).
- Zero latency: correct. YIN is a side chain; its 27 – 57 ms are a response time.

### Changes made, and why

| # | Change | Why |
|---|---|---|
| 1 | **PLL bandwidth no longer comes from `speed`**; own bandwidth f0 / 8 (8 – 30 Hz), YIN feed-forward | 1 / (2π τ) is 1 Hz at the default: it could not follow the 6 Hz vibrato its own bench asked for |
| 2 | **Carrier phase accumulated (φ_c += r · Δψ), not r × unwrapped ψ** | With r × ψ, any LFO, envelope or EXP move of r swings the phase by Δr times the whole elapsed phase: a chirp that grows with time |
| 3 | **`phase` redefined**: acts where 2 r is an integer, re-referenced on ψ mod 4π, default 90° | The draft said it balanced "DC against the octave": at r = 1 the octave's level does not depend on it, only the DC does. What it really sets is how the two products on each harmonic interfere. 90° removes the unison DC |
| 4 | **Lock engages at zero error**; strict engage conditions; `state` meter | Pulling in a random phase is a 20 ms chirp of tens of cents on every note |
| 5 | **Phase from two cascaded SVFs (Q 4) and their BP / LP quadrature**, fed from the unfiltered input | Q 2 left the 2nd harmonic only 10 dB down, and it is often louder than the fundamental. The Hilbert pair was not needed, and `lowcut` at 131 Hz would have starved the low strings' fundamental |
| 6 | **`lock` on: glide between notes only**, then follow the note | Glide and lock contradicted each other |
| 7 | **Onset pre-estimate needs two agreeing periods; used above ≈ 110 Hz** | "Under one period" was wrong (two crossings span one period, after waiting for the first), a single interval often reads the 2nd harmonic, and below 110 Hz YIN is as fast |
| 8 | **The detector's lowest note follows `lowcut`** | "40 Hz for bass" had no parameter behind it: a hidden mode |
| 9 | **`balance` mismatches the two diodes of one branch**, range 0 – 100 % | Detuning one branch against the other keeps each G even: the leak would be at 2 · f_c and DC, not the carrier. The sign was inaudible |
| 10 | **`carrier_level` low end described as starved / gated**; fixed output normalisation | It is not "near multiplication": below vb nothing passes |
| 11 | **ADAA on the tanh's residual**; ADAA1's roll-off and delay stated | ADAA1 on the whole signal costs 2 dB at 20 kHz at 2×, even at `drive` 0, and would fail the "mda law" check |
| 12 | **Feedback: stability and aliasing stated**, bench added | The 2× argument does not cover a loop that multiplies on each pass; the limiter's real reason is the 26 dB of loop gain under a slow carrier |
| 13 | **SSB uses the `ssb` module, not `pitch::Hilbert`** | That pair delays the wrong path: recomputed here, image −18 dB at 1 kHz at 48 kHz, −24 dB at 96 kHz; with the right path delayed, ≤ −44 dB |
| 14 | **Just: continuous law, 7/5 for the tritone**, motion outside the mapping | Steps of up to 18 cents during a sweep; 45/32 gives a series of f0 / 32, which is clangour |
| 15 | **Wavetables: 4 × the top harmonic, Hermite read** | 2,048 points for 1,024 harmonics is critically sampled |
| 16 | **Squelch has its own fixed times and hysteresis** | It borrowed `env_rel`, tying three functions to one knob |
| 17 | **DC blocker 20 Hz, 2-pole** | The DC term follows the note's envelope; 5 Hz lets the attacks thump |
| 18 | **`stereo_phase` offsets the carrier's phase only** | Offsetting the LFO too gave the two sides different carrier frequencies: two meanings on one shared id |
| 19 | **Display names = catalogue names / labels**; wet filters renamed "Wet Low Cut" / "Wet High Cut" | golem joins on `name`; "Frequency" would have orphaned `freq`, and two "Low Cut" would have collided |
| 20 | **Inharmonicity stated**; lock bench on a harmonic test tone | "Stationary spectrum" cannot be measured to −60 dB on a real string |
| 21 | Bench: SSB rejection, feedback bound, lock engage, lock phase, Just law, 44.1 kHz alias, leak at f_c, realistic response times | Each new or corrected claim has its check |
| 22 | Glyph simplified; lime's hue corrected (84°, not 90°); pink citation points to the source; `onset` is not in gfx-dsp yet; dry path follows the model; tremolo at `mix` 0.5 | Accuracy |

### Still open

| Point | Owner |
|---|---|
| **PLL prototype** in gfx-bench on the DIs, before `lock` / `phase` / `state` are frozen (§7.8) | golem-fx, first task |
| gfx-dsp `ssb` (corrected Hilbert) must land before Upper / Lower; gfx-freqshift's design lists gfx-delay `loop_shift` and gfx-octaver as users of the faulty pair | golem-fx (gfx-freqshift's design) |
| Engine table: `surge-treemonster` → gfx-ringmod with `init` `carrier` = Track; join that block on `label` or id | golem-rack |
| Catalogue default of `ringmod.freq` (0.3 → 0.025); re-authored `standard`, `robot-voice`, `bell-tone` | The maintainer (ear), then golem-rack |
| `snap` default; `phase` default 90°; the Just table (7/5, 9/5) | The maintainer's ear |
| Were `lowcut` / `highcut` ever active in golem's rig (Surge's "deactivated" toggles)? | golem-rack, on the rig |
| New `ringmod` glyph, face, brass crate hue | golem-rack, with the release |
| External carrier (aux bus) once golem routes key inputs | golem-rack, later |
| Parker's own figure for oversampling, to read in the paper | golem-fx, before quoting it |

**Note (2026-11-14, after the gfx-sustainer review):** the tracker this design specifies (a band-pass pair
ahead of the PLL) delays the string's pitch modulation by about 2.5 periods; on a ±30 ¢, 6 Hz vibrato the
locked carrier sat 0.9–1.8 rad off the string, and lock was lost at ±50 ¢ on the 3rd harmonic. The shared
gfx-dsp `track` module is respecified in `gfx-sustainer/README.md` §2.7 (complex demodulation at the NCO's
phase, the filter inside the loop, bandwidth f0/5, partials at k·ψ with a ratio trim). gfx-ringmod's Track
carrier uses that module; align this section when `track` is coded. **Done (2026-11-15)**: §8 records
the tracker as coded and where it departs from both texts.
