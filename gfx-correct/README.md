# gfx-correct

> **Status: CODED** (v0.1.0, 2026-11-15). Designed 2026-11-14 by Claude Opus 5.5, reviewed by Claude
> Fable 5.2 (Review section), decided by Raph (end of file), coded as reviewed with the deviations listed
> in "As coded". gfx-golem's epoch tracker moved to gfx-dsp `psola` (gfx-golem byte-identical before and
> after, 54 renders) with the new fixed-delay `Reader`; gfx-dsp `pitch` gained `PeriodShifter`.
>
> - **Live** (default): latency 9.3 / 6.6 / 4.3 ms (Low / Mid / High); a 220 Hz note sung 40 ¢ flat is
>   within 10 ¢ 24.9 ms after its onset; a ±50 ¢ 6 Hz vibrato keeps 13.3 % of its depth at `vibrato` 0 %
>   and 99.0 % (centre +1.6 ¢) at 100 %. **Studio**: 37.3 / 26.2 / 17.3 ms, the first cycles of a note
>   after silence within 0.4 ¢ of the target, vibrato 1.0 % / 98.9 %.
> - The identity (corr 0, offset 0) is exact in both modes, voiced or not, and again after the Live head's
>   re-centring; a midpoint vibrato of ±60 / ±100 ¢ flips the target 3 times before the model engages,
>   never after; jitter 0.78 × the input's at `filter` 0 (Live), 0.89 × (Studio).
> - 15 behaviour tests, bench 28 / 28 at 44.1 – 192 kHz; CPU 55 ns / sample mono (0.26 % of a core),
>   64 ns stereo. Raph's sung phrase in E minor, hard settings: mean distance to the scale 44 → 7 – 9 ¢.

golem's **pitch correction** for voice and mono instruments: it hears the note sung, chooses the note
it should be, and moves the voice there, as hard as a robot or as gently as a good singer on a good day.
It stands for golem's `pitch-correct` block ("Pitch Correct", category `pitch`, role `voice`, today
x42-Autotune, Robin Gareus' LV2 port of Fons Adriaensen's Zita-AT1) and keeps its parameter ids `corr`,
`filter`, `fastmode`, `offset`, `key`, `scale`, `m00 … m11` ([golem contract](../docs/golem-contract.md)).

The idea in four lines:
- **Two detectors**: YIN names the note and guards against octave errors; the glottal epochs give the pitch
  of every cycle, so the correction is computed from what the voice did one cycle ago, not 20 – 30 ms ago.
- **The correction acts on the steady pitch, not on the vibrato.** The vibrato is modelled (a sinusoid
  fitted to the last cycle and a quarter) and kept, removed or deepened, so hard tuning no longer warbles
  a vibrato and natural tuning no longer flattens it.
- **Two times**, as the best tuners have: `transition` (how fast a new note is caught) and `filter` (how
  tightly a held note is kept). Together they are Auto-Tune's Retune Speed + Humanize.
- **Live**: a pitch-synchronous resampling shifter (the Auto-Tune / Zita family) at **4.3 – 9.3 ms**
  reported latency, by register. **Studio**: epoch-synchronous PSOLA with exact formants and every onset
  corrected from its first cycle, 17 – 37 ms.

Layout: mono 1 / 1 first, 2 / 2 second (analysis on (L + R) / 2, both channels shifted by the same
decisions). Latency is reported (§2.7).

## 1. State of the art

Survey of 2026-11-14. **[V]** read in source code, a paper or a manual · **[M]** manufacturer page or
listing · **[F]** forum post or review · **[A]** computed here · **[I]** inference, or not re-checked at
the source.

### 1.1 The block we replace: x42-Autotune / Zita-AT1 [V]

Read in [x42/fat1.lv2](https://github.com/x42/fat1.lv2) (GPL-2+): `src/retuner.cc`, `src/retuner.h`,
`src/fat1.cc`, `lv2ttl/fat1.base.ttl.in`, `presets/*.ttl`.

- **Detector**: at 44.1 / 48 kHz the input is upsampled ×2; every 4 fragments of 128 frames (10.67 ms at
  48 kHz, 11.6 ms at 44.1 kHz) an FFT autocorrelation over a 2,048-point Hann window is taken. The window
  reads every second sample of the 4,096-sample upsampled buffer: it spans 2,048 input samples, **42.7 ms**
  at 48 kHz, and its centre is 21.3 ms old. The spectrum is weighted by 1 / (1 + (f / 3 kHz)²) and the
  result normalised by the window's own autocorrelation. Among the peaks above 0.8 between 60 and
  1,200 Hz, the one with the largest value × window autocorrelation wins (short lags favoured), refined
  by a parabola. After two failed estimates the note bias is dropped; after more than five (64 ms) the
  voice counts as unvoiced, the error is reset to 0 and the read head is crossfaded back to its centre.
- **Target**: the nearest enabled note of the 12-bit mask, in octaves from the reference (`tuning`,
  400 – 480 Hz). The current note gets a **bias**: `bias` (0 – 1, not in golem's catalogue, so at its
  default 0.5) subtracts bias / 13 octaves from its distance: a held note is kept until the voice is
  **73 ¢** from it, 23 ¢ past the midpoint [A]. Mask all off = no correction.
- **Correction law**: ratio = 2^(`offset` / 12 − error · `corr`). `corr` is a **linear fraction** of the
  error removed. `offset` is in semitones (±2).
- **`filter`**: `set_corrfilt(v)` sets the coefficient to Δ / v with Δ = 4 · 128 / fs-ish (10.67 ms at
  48 kHz): a one-pole on the error **while the note stays the same**, of time constant τ = −Δ / ln(1 − Δ / v)
  ≈ v − Δ / 2: 0.02 → 14 ms, 0.1 → 94 ms, 0.25 → 245 ms, 0.5 → 495 ms [A]. **`filter` is a time constant
  in seconds.** When the note changes, the filter is bypassed and the error jumps to the new value
  (ttl comment, and `finderror`).
- **Shifter**: resampling of the input at the ratio, with jumps of a whole number of periods (at least one
  128-frame fragment) crossfaded over one fragment (raised cosine, 2.7 ms). Formants move with the ratio.
- **`fastmode`** ("Fast Correction", "Reduces latency by allowing initially uncorrected signal"): the read
  head moves 7/16 of the buffer closer to the input. Reported latency **1,056 samples (22.0 ms at 48 kHz)**
  normally, **160 samples (3.3 ms)** in fast mode (both include the resampler's 32) [V]. Normally the audio
  sits at the analysis window's centre. In fast mode it is 896 samples (18.7 ms) newer than that centre,
  plus up to one 10.67 ms update: the correction comes **19 – 29 ms late** [A]. A 6 Hz vibrato "removed"
  with a 24 ms lag keeps 2 · sin(π · 6 · 0.024) = 87 % of its depth: fast mode cannot flatten a vibrato,
  it chases it.
- **MIDI**: `mode` Auto · MIDI · Manual; held MIDI notes replace the mask; pitch bend adds to `offset`
  (`bendrange`). Not used by golem.
- Its own presets: *Live* (corr 0.84, filter 0.033, fast), *Slightly Corrected* (corr 0.33, filter 0.0425).
- **Licence**: GPL-2-or-later, compatible with golem-fx's GPL-3. Nothing is copied: the detector (YIN and
  epochs), the note logic, the shifter and the formant path here are different algorithms. Only the
  parameter ids and the three laws (`corr`, `filter`, `bias`) are kept, as the contract asks.

**What golem does with it today** ([V], `~/golem`):
- Catalogue (`library/catalog/effects.json`): `corr` 0 – 1 (label Amount), `filter` 0.02 – 0.5 (label
  **Speed**, note "bas = correction lente/naturelle, haut = rapide"), `fastmode` (label **Hard**, block note
  "Hard = fastmode (effet T-Pain)"), `offset` ±2 st, `key` / `scale` (Chromatic · Major · Minor) **virtual**,
  `m00 … m11` hidden.
- `bridge/compile_rig.py` `_expand_pitch_correct`: key + scale → the 12 mask bits, key and scale removed.
- Presets: *Transparent* (default: corr 0.7, filter 0.06, Chromatic), *Tight pop* (1.0, 0.25, Major),
  *Hard tune* (1.0, 0.5, fastmode, Minor), all with key C and offset 0.
- **Two misreadings**, to report to golem-rack: `filter` is a smoothing **time** (higher = slower, more
  natural), so "Speed" and its note are inverted, and *Hard tune* at 0.5 is the loosest within-note
  setting there is; `fastmode` is a latency choice, not "Hard". The T-Pain sound in x42 comes from its
  instant note changes plus a short `filter`. (x42's ttl says it too: its scale points are Slow 0.5,
  Med 0.1, Fast 0.02.)
- **The song key**: `song_key_of(preset)` reads the patch's `key` (or its Standard block's key / grid);
  `_expand_pitch_harmony` gives it to gfx-pitch's harmony through a virtual `song_key` (on by default).
  `pitch-correct` does **not** follow it: it stays at its own key / scale. So *Tight pop* snaps to C major
  and *Hard tune* to C minor whatever the song's key: wrong notes in any other key (§3.5).
- Voice chain order (`docs/blocks-doctrine.md`): gate → preamp → de-esser → **pitch-correct** → comp → eq.

### 1.2 Commercial and free tuners

| Reference | What makes it special | What we take |
|---|---|---|
| Antares Auto-Tune (Hildebrand, US 5,973,252, 1999) [V patent abstract; M] | Autocorrelation detection; correction by **resampling and adding / dropping whole cycles**; **Retune Speed** in ms (0 = instant, removes vibrato too); **Humanize** = slower retune on the sustained part of long notes; **Flex-Tune** = pulls only as the singer approaches the target; Natural Vibrato; low-latency mode "2.5 ms, or 175 samples at 44.1 kHz" (Antares' own blog; the two figures disagree: 175 samples are 4.0 ms), "< 1 ms" in Classic / Live modes [M] | The engine family for Live; the three behaviours, as `transition` + `filter`, `flex` |
| Antares AutoTune Advanced (2026) [M] | "DVA": detects the vibrato and leaves it alone even at the fastest retune | Vibrato separated before correction (§2.3) |
| Waves Tune Real-Time [M, F] | **Speed** (within a note, default 15 ms) and **Note Transition** (between notes, default 120 ms) separate; formant correction; latency "0 – 4 ms, depends on the note", reports 0 | Two times; latency tied to the period |
| Celemony Melodyne [M] | Offline note objects: the reference for transparency, not live | Bench reference only |
| Graillon 2 / 3 (Auburn Sounds, free tier) [M, F] | Correction with Smooth, Snap Range, **Inertia** (notes "stick", may change the melody); ≈ 23 ms design latency (G2) [U, not found again at the review] | Hysteresis exposed (`hysteresis`) |
| MeldaProduction MAutoPitch (free) [M] | Speed, depth, formant shift, stereo widening | — |
| GVST GSnap (free) [M] | Min / max frequency for detection (octave errors), MIDI pitch bend and vibrato | Register as a range (gfx-golem's `register`) |
| Newtone, TC-Helicon VoiceLive correction [I] | Hardware live correction, key from MIDI or the guitar's chords | MIDI target as v2 (§2.9) |

### 1.3 Research

| Reference | Idea | What we take |
|---|---|---|
| de Cheveigné & Kawahara, *YIN*, JASA 2002 [V] | CMNDF: few octave errors | gfx-dsp `detect`, the note and the guard |
| Mauch & Dixon, *pYIN*, ICASSP 2014 [V] | Several candidates + HMM | Candidates + continuity, no Viterbi (look-ahead) |
| Moulines & Charpentier, TD-PSOLA, Speech Comm. 1990 [V] | Pitch-synchronous grains on glottal epochs: formants kept | Studio engine, from gfx-golem |
| Laroche & Dolson, phase vocoder, 1999 [V] | Peak-locked PV | Rejected here: on one voice, the PV's phasiness and its formants-follow-pitch lose to time-domain methods at equal latency [I] |
| Kim et al., *CREPE*, ICASSP 2018 [V] | CNN pitch, robust | Too heavy for a per-lane live pedal |
| LPC envelope re-filtering (source-filter cross-synthesis) [I] | Whiten with one envelope, recolour with another | Rejected at the review (§2.6): order 20 at 48 kHz cannot resolve formants, a higher order follows the harmonics of high voices [A] |
| True-envelope ratio filter (Röbel & Rodet, DAFx 2005; gfx-formant's `specenv`) [V] | Envelope ratio as a minimum-phase FIR, zero latency | Live formant correction, staged (§2.6) |

### 1.4 Forum feedback

| What singers and engineers complain about | Cause | Answer here |
|---|---|---|
| **Warble on vibrato** [F] | Retune partly tracks the vibrato; a vibrato near a midpoint flips between two notes | Quantise the steady pitch (vibrato removed first), hysteresis (§2.3) |
| **Artefacts on breathy or rough voices** [F] | Unreliable periods forced into a shift | Correction fades with clarity (`sens`); one read head, so the breath is never combed (§2.5) |
| **Octave jumps** [F] | Detector errors, 2nd harmonic stronger | Register range, epochs held to YIN ± 25 %, ×2 / ÷2 jumps confirmed over 40 ms (gfx-golem) |
| **Latency for the singer in ears** [F, M] | 20+ ms engines (x42 normal 22 ms, Graillon ≈ 23 ms) | Live ≤ 9.3 ms in every register |
| **Robot onsets on a natural setting** [I] | One retune time for onset and sustain | `transition` vs `filter` |
| **Late correction live** (x42 fast mode: 19 – 29 ms) [A] | Correction applied to audio newer than its analysis | Cycle-rate pitch from epochs, delayed to meet its audio: at most a few ms off above 1.5 × the register's floor (§2.7) |

### 1.5 What we conclude

1. **Live = resampling with pitch-synchronous splices** (Hildebrand, Zita; gfx-dsp `PeriodShifter`, §2.5). It
   needs no future signal beyond interpolation: its delay is about half a period, the reason the commercial
   live modes reach a few ms. Formants move by the ratio: 3 % for a 50 ¢ correction, below the vowel's
   own variation [I]; for larger shifts (`offset`, sparse scales) gfx-formant's envelope filter can
   restore them (staged, §2.6).
2. **Studio = PSOLA** (gfx-golem's engine): exact formants, and the latency to look ahead.
3. **Correct the steady pitch, keep the vibrato apart**, and decide notes on the steady pitch.
4. **Two times** (`transition`, `filter`), a flex zone, a hysteresis: the controls the best tuners converged
   on, each with a precise law.
5. **Keep x42's ids and laws** where they are already real units (`corr` fraction, `filter` seconds,
   `offset` semitones): golem's presets keep their meaning; only golem's labels were wrong.

## 2. Design

### 2.1 Signal path

```
in ─┬─────────────────────────────────────────────────────────────────▶ shifter (Live | Studio) ─▶ out
    │                                                                        ▲ ratio r(t), splice T
    ├▶ HP 50 Hz ─▶ YIN (detect, register) ──▶ note guard ──┐                  │
    └▶ LP 1 kHz ─▶ epoch tracker (psola) ──▶ cycle pitch p ─┴▶ vibrato model ─▶ quantiser ─▶ correction law
                                                              centre c, w_v   (mask ∩ key/scale,   (transition, filter,
                                                                               hysteresis)          flex, corr, vibrato, offset)
```

All pitch arithmetic is in cents relative to `reference` (A4), f64. Control rate: each new cycle (epoch)
and each YIN hop (2.7 ms), interpolated per sample into the shifter's ratio. Stereo: one analysis on
(L + R) / 2, one ratio and the same splice positions for both channels.

### 2.2 Pitch: what the voice does now

- **YIN** (gfx-dsp `detect`, `prepare_range` with gfx-golem's `REGISTERS`: Low 60 – 500 Hz, Mid
  90 – 800 Hz, High 150 – 1,200 Hz; harmonic lock **off**, as gfx-golem for voices). Hop 2.7 ms.
  It names the note, gives the clarity (voicing) and bounds the epoch search.
- **The fixed window** (gfx-swell's review: window = 1 / f_min, 11.1 ms in Mid) is shortened once a note
  is tracked: `set_range(max(f_reg_min, 0.6 · f), f_reg_max)` (no allocation), back to the full range when
  voicing is lost. A 220 Hz note then answers within 7.6 ms + one period + one hop instead of
  11.1 + 4.5 + 2.7 ms [A]. The first sound of a phrase still waits for the full window (§2.7).
  - The narrowed range also shuts out the sub-octave (0.5 · f).
  - Its cost: a legato fall of more than a major sixth (below 0.6 · f) finds no dip. The full range is
    restored on the first analysis whose clarity falls under the close threshold, without waiting for the
    30 ms hold; the ratio is held meanwhile (≈ 20 ms). Bench row.
- **Epochs** (gfx-golem's tracker, moved to gfx-dsp `psola`): marks predicted at the previous mark + T,
  searched within ±25 % of YIN's period, polarity held per voiced segment, refined to sub-sample by
  cross-correlating the newest cycle with the previous one. **The cycle pitch** p = 1 / (m_k − m_(k−1)),
  averaged over the last two cycles, is the correction's input: one value per glottal cycle.
  - **Its age**: the two-cycle mean is centred one period back, and a mark is confirmed 0.25 T + 0.2 ms
    (the low-pass) after it: p is **about 1.3 periods old** (5.8 ms at 220 Hz, 13 ms at 100 Hz).
  - **At a voicing onset** the tracker is seeded backwards through its own history from YIN's first
    reading, so the first cycle pitch comes with that reading, not two periods later.
  - **Jitter.** A voice's periods differ by 0.5 – 1 % from one cycle to the next (9 – 17 ¢), at random. A
    correction that is one cycle late cannot cancel that: at `filter` 0 it adds a delayed copy, and the
    white part of the jitter grows by √1.5 = 1.22 [A]. This is the price of following the voice within a
    cycle; averaging four cycles brings it to 1.12 and doubles the lag. Kept at two cycles, with a bench
    row (§5): if the hard tune sounds hoarser than the dry voice, the average goes to three or four cycles
    at `filter` 0. From `filter` ≈ 20 ms up the one-pole removes the effect.
- **Why not the tuner's `zoom`** (gfx-tuner §2.4, rewritten by its review): it reaches 0.1 ¢ with windows of
  8 – 48 periods; correction needs 1 – 2 ¢ within one period. Cycle-to-cycle correlation is the right tool.
- **Guard**: a ×2 / ÷2 jump of YIN shorter than 40 ms is rejected (gfx-golem `OCTAVE_CONFIRM_MS`); the
  epoch period must stay within ±25 % of YIN's, else the tracker re-seeds from YIN.
- **Voicing** (gfx-golem's gate, `sens` and `thresh` with the same meanings): clarity opens at 0.8, closes
  under 0.65 at `sens` 50 %, 30 ms hold. The **correction weight** g_v ramps 0 → 1 in 10 ms on open and
  1 → 0 in 30 ms on close: unvoiced sound, breath and bleed get the offset only, never a guessed
  correction.

### 2.3 The vibrato model and the quantiser

The first design split the vibrato off with a 5.5 Hz band-pass (Q 0.7) gated by a 15 st / s slope limit
and by target changes. The review's prototype showed that it cannot work (§Review): a ±50 ¢ vibrato at
6 Hz has a slope of 19 st / s and closed its own gate, a vibrato wide enough to cross a note boundary
flipped the target before the split engaged and each flip closed it again (12 flips per second, for
ever), and a note step rang into the band-pass (65 % of the step after 32 ms). Replaced by a model fit:

- **Fit.** On each new cycle, least squares of p ≈ c + a · cos ωt + b · sin ωt over the last 1.25 periods
  of the vibrato, for nine rates (4 – 8 Hz in 0.5 Hz steps); the rate with the lowest residual wins.
  Three unknowns, a few hundred multiplies per cycle. The fit ends three cycles before the newest one.
- **A vibrato is present** when its half-extent A = √(a² + b²) lies in 12 – 110 ¢ and the residual is
  under 0.3 · A (it is a sinusoid, not a run of notes). Then w_v ramps to 1 over 60 ms.
- **Leaving it.** The model predicts the next cycles. When the pitch is more than max(30 ¢, 0.5 · A) off
  the prediction for two cycles running, the singer has left the vibrato for another note: w_v falls to 0
  within 20 ms and the fit starts again from that cycle. A target change alone does not close the split.
- **The centre** c is the steady pitch. The quantiser reads p_q = p − w_v · (p − c); the correction reads
  p_st = p − w_v · min(`vibrato`, 1) · (p − c) (§2.4). So with the split fully on, everything the singer
  does around the centre (the vibrato and its irregularities) rides on the target untouched.
- **Measured on synthetic contours** (4 ¢ rms jitter; `proto-gfx-correct/vib3.py`) [A]:
  - ±20 – 100 ¢ at 4.5 – 7 Hz: engaged 2 – 3 vibrato cycles after the vibrato is stable (0.3 – 0.55 s
    after it starts to grow), then no target flip, including ±60 and ±100 ¢ centred on a midpoint;
  - a note step of 100 – 300 ¢ under vibrato: the target changes 0 – 35 ms after the middle of the
    transition, and the split is back 0.27 s after the landing;
  - scoops, slow bends, slow drift with 8 ¢ of jitter, melismas of 100 and 200 ¢ every 150 ms, a
    whole-tone alternation at 5.5 Hz: every note is quantised. The split stays off, except on the
    semitone melisma (3.3 Hz), where w_v rose to 0.67 for a moment without a wrong note.
- **Limits**, for the bench on sung DIs:
  - Before the split engages, a wide vibrato still flips the target a few times (2 – 6 in the prototype;
    14 at ±100 ¢). `hysteresis` is the control for that.
  - **A semitone trill at 5 – 7 Hz is a ±50 ¢ vibrato** to any detector. The shape test rejects a
    squared one most of the time, not always. With `vibrato` at 100 % a trill taken for a vibrato is
    centred on one note and both its notes end 50 ¢ off. This is why `vibrato` defaults to 0 % (§3.1).
- **Allowed notes** = `m00 … m11` **∩** the notes of `key` / `scale` (Chromatic = all 12). Intersection
  makes both of golem's paths work: the mask written by `_expand_pitch_correct` with `scale` left at
  Chromatic, or `key` / `scale` written directly with the mask left all on. None allowed = no correction.
- **Quantiser** on p_q: the nearest allowed note, with the held note favoured by `hysteresis` cents past
  the midpoint (23 ¢ = x42's default bias, §1.1). Deciding on the centre is what stops the warble: a
  vibrato centred on a midpoint no longer flips the target 12 times a second.

### 2.4 The correction law

With e = q − p_st (target minus steady pitch, cents):

1. **Flex** (Flex-Tune-like): e_f = e · (1 − `flex` · s(|e|)), s = smoothstep from 15 ¢ (0) to 60 ¢ (1).
   At `flex` 100 % a note 15 ¢ off is fully pulled, one 60 ¢ or more away (a blue note, a deliberate bend
   in a sparse scale) is left as sung.
2. **Two times**: ĉ follows e_f through a one-pole of time constant τ:
   - τ = `transition` during a note's **onset phase**: from a target change or a voicing onset until the
     input pitch has settled (|dp / dt| < 2 st / s for 30 ms, on the two-cycle pitch with the model's
     sinusoid removed when w_v > 0) or 250 ms have passed;
   - τ = `filter` afterwards, for the held note.
   - `transition` 0 is x42's jump (the error is taken at once); `transition` = `filter` is a single retune
     time (Auto-Tune's Retune Speed); `transition` short and `filter` long is Auto-Tune's Humanize. The
     onset phase ends on the landing, not on the first estimate, so a scooped note is caught where it
     lands: x42's jump at the first estimate leaves the rest of the scoop on the note for `filter`
     seconds (up to 27 ¢ on a legato semitone, which flips 73 ¢ from the old note; up to 50 ¢ on a
     scooped onset in Chromatic) [A].
   - The one-pole's state ĉ is continuous when τ changes: no step in the correction.
3. **Shift** (cents): s = g_v · `corr` · ĉ + (`vibrato` − 1)⁺ · w_v · v_m + 100 · `offset`, with
   v_m = a · cos ωt + b · sin ωt, the model's sinusoid at the current cycle.
   - `vibrato` < 100 %: the share (1 − `vibrato`) of p − c stays in p_st, hence in e, and is corrected as
     far as τ allows: at 0 % the law is x42's (vibrato is error), except that notes are still chosen on p_q.
   - `vibrato` 100 %: e = q − c, a slow shift: the vibrato is kept exactly, whatever τ.
   - Above 100 % the model's sinusoid is added, not p − c: the vibrato deepens (200 % = doubled) and the
     jitter does not.
4. r = 2^(s / 1200), slewed per sample (no step in r larger than one control interval's change).

So `corr` 1, `filter` 0, `transition` 0, `vibrato` 0 = the stair-step robot; the same with `vibrato` 100 %
= the modern hard tune that keeps the singer's vibrato; `corr` 0.8, `transition` 40 ms, `filter` 0.15 s,
`vibrato` 100 %, `flex` 40 % = transparent.

### 2.5 Live engine (`fastmode` on): period-locked resampling

A new gfx-dsp shifter, `pitch::PeriodShifter`, beside `CenteredShifter` (same delay line, same correlation
code). It is not a mode of `CenteredShifter`: that one always plays two heads, crossfaded by sin² windows
over the whole grain. Locked k · T apart they would leave the periodic part intact, but the breath (no
period) would be the sum of two copies: a comb, and a 3 dB flutter at the grain rate (about 5 Hz for a
50 ¢ correction in Mid) [A]. Here one head plays, and a second one exists only during a splice.

- The read head moves at rate r around a **fixed centre** C (its delay changes by 1 − r per sample). When
  the delay reaches C ± T / 2 a second head starts **exactly one period** nearer the centre, and the two
  are crossfaded (raised cosine, amplitude-complementary) over min(T, 2 ms). Splices land on the same
  point of the glottal cycle: no phase jump, so no buzz at the splice rate [V principle; A for our
  parameters]. Shifting up repeats a cycle, shifting down drops one: the method itself, as in Zita and
  in Hildebrand's patent.
- **The jump length** is the tracker's current period, searched ±10 % by correlation on the 12 kHz
  signal, then refined over ±2 samples at the full rate with a parabola (a 12 kHz grid is 4 samples
  wide: too coarse for a clean splice on its own).
- Splice interval = T / |r − 1|: one every 34 periods for a 50 ¢ correction [A].
- **Centre**: C = T_max / 2 + 1 ms, T_max = 1 / register minimum. The 1 ms covers the crossfade at the
  largest ratio (2 ms · 0.26 at +4 st) and the interpolator.
- **Control delay.** The cycle pitch is 1.3 T old, the audio C old. Where 1.3 T < C (220 Hz and above in
  Mid) the ratio is delayed by the difference, so the correction meets its audio. Below, it is late by
  1.3 T − C: 6.4 ms for a 100 Hz voice in Mid. The head's own swing adds ±T / 2 either way.
- **No period** (unvoiced with `offset` ≠ 0, or clarity under the close threshold: open question 6): the
  jump length is the best correlation lag within ±50 % of the last period, clipped to the register. The
  same code with a wider search; on noise any lag does.
- **Back to the centre.** When the voice stops, the head stands wherever the last note left it, up to
  T / 2 from C: the consonant that follows would be early or late by up to 5.6 ms (Mid), each time
  differently. So at the first cycle with g_v = 0 and r = 1 the head is crossfaded to C over 5 ms (x42
  does the same). From there, at `offset` 0, the output is the input delayed by C, exactly (an identity:
  bench row, measured after a voiced passage too).
- **Formants follow r** (§2.6).
- 2 / 2: both channels use the splice positions found on (L + R) / 2.

### 2.6 Formants in Live: staged on gfx-formant's `specenv`

The shifted signal has the voice's envelope stretched by r: 3 % for a 50 ¢ correction, 12 % at `offset`
±2 st. For corrections under about 100 ¢ no live tuner restores it and nobody misses it. For the larger
shifts the tool exists in the suite: gfx-formant's Live engine applies E(f / α) / E(f) as a minimum-phase
FIR with no latency. Run on the shifter's output with α = 1 / r, it puts the formants back.

- **v1**: `formant_keep` acts in Studio only (PSOLA). In Live it does nothing; the parameter exists from
  v1 so no id changes later.
- **Stage 2**, once `specenv` has passed gfx-formant's bench: the filter after the shifter, with weight
  clamp((|s| − 30 ¢) / 70 ¢, 0, 1), so small corrections never pay for it (two 512-tap convolutions per
  sample, ≈ 49 M MAC / s, about 3 % of a core while it runs).
- **Gate for stage 2**: on Raph's sung phrase and a soprano recording, the filter at `offset` ±2 st must
  be preferred blind to plain resampling. If not, Live stays as in v1 and Studio remains the
  formant-exact path.
- **Why not the first design's LPC re-filter** (A_y(z) · y / A_x(z), order 20, lattice). Prototype on
  synthetic vowels, ±2 st, envelope error over 0.3 – 4 kHz (`proto-gfx-correct/lpc.py`) [A]:
  - plain resampling: 6.5 dB rms;
  - order 20 at 48 kHz: 3.2 – 4.2 dB. Twenty poles over 24 kHz cannot resolve F1 from F2;
  - order 48: 1.5 – 2.1 dB at 120 – 220 Hz, but 3.4 – 7.3 dB at 440 Hz, and at 700 Hz the recursion left
    the stable region. With few harmonics the poles sit on them (the known fault of LPC on high voices),
    so the re-filter would ring at the pitch the note was moved from [I];
  - the two envelopes do not share their lag: A_y comes from output frames, A_x from input frames.
  gfx-formant reached the same verdict on LPC from the literature. One envelope module in gfx-dsp, not two.

### 2.7 Studio engine (`fastmode` off): PSOLA with look-ahead; latency

gfx-dsp `psola`'s fixed-delay reader (gfx-formant §2.3: every output sample reads the two active grains,
asymmetric Hann halves on the epochs, synthesis marks at T / r, grains repeated or skipped), here with
**one voice and no gate on the output**: at r = 1 and α = 1 the reader returns the input delayed by D,
exactly, so unvoiced passages go through the same path and no crossfade between engines is needed.

- **Formants** stay put by construction (α = 1). `formant_keep` off resamples the grains by α = r (the x42
  timbre).
- **Unvoiced**: the marks freewheel at the last period and r = 1: noise is never re-spaced (re-spaced
  grains of noise buzz at the mark rate). `offset` then reaches unvoiced sound only through α, with
  `formant_keep` off. With it on, a transposed voice keeps its consonants where they were: correct.
- **Delay D = 2 · T_max + 4 ms.** The reader needs 1.25 · T_max + 0.3 ms (gfx-formant). The rest is
  look-ahead for the decisions: YIN's first reading of a phrase comes T_max + T + 2.7 ms after the onset
  (§2.2), at most 2 · T_max + 2.7 ms. With D above it, the voicing weight, the target and the correction
  are in place before the note's first cycle plays: **every note corrected from its first cycle**, in
  every register. (The first design's 1.25 · T_max + 4 ms made that claim and missed it by 0.4 ms at
  220 Hz and by 6 ms at 100 Hz, before the 10 ms voicing ramp.)
- With the look-ahead, the cycle pitch is a centred mean of three cycles (no jitter gain, §2.2; two
  cycles in the lowest 15 % of the register, where the look-ahead is shorter than a period) and the ratio
  is aligned on its audio exactly.

**Latency** (reported to the host; fixed per mode and register, changed only by these two setup
parameters, which make the host re-sync — never automate them):

| `fastmode` | Low (60 Hz) | Mid (90 Hz) | High (150 Hz) | Law [A] |
|---|---|---|---|---|
| on (Live, default) | **9.3 ms** | **6.6 ms** | **4.3 ms** | T_max / 2 + 1 ms |
| off (Studio) | 37.3 ms | 26.2 ms | 17.3 ms | 2 · T_max + 4 ms |
| x42, for reference | 22.0 / 3.3 ms (normal / fast) | | | [V] |

- In Live the true delay swings ±T / 2 around C between splices: at most ±5.6 ms in Mid, never more than
  T_max (= 11.1 ms) in total, the same behaviour as every resampling tuner.
- **First correction of a phrase**, Live (Mid, a 220 Hz note): YIN's first reading comes 15.6 – 18.3 ms
  after the onset (window 11.1 + one period 4.5 + up to one hop 2.7); the audio is 6.6 ms behind. So the
  first **9 – 12 ms** of the note as heard play at the previous ratio (or the offset alone), and the
  correction is complete after the 10 ms voicing ramp: **about 20 ms, four to five cycles**, into the
  note. x42's fast mode does the same, later ("initially uncorrected signal", 42.7 ms window). Inside a
  phrase there is no such wait: a new note is followed 1.3 periods behind the voice.
- **Alignment** of correction and audio in Live (§2.5): within ±T / 2 from 220 Hz up in Mid, so a 6 Hz
  vibrato removed at `vibrato` 0 % keeps ≤ 11 % of its depth at 3 ms; a 100 Hz voice, 6 ms late on
  average, keeps 24 % (x42 fast: 87 % at its mean 24 ms lag) [A].
- **Changing `fastmode` or `register`**: 10 ms fade-out, engines reset, new latency reported, fade-in.
  A golem preset that changes them makes the host re-sync when it is recalled: golem's presets should all
  carry the same `fastmode` (§3.5).
- **Bypass** (`bypass`, nih-plug `make_bypass`): output = input delayed by the reported latency, 10 ms
  crossfade: switching never moves the voice in time.

### 2.8 Reuse from gfx-dsp, new code, CPU

| Module | Status |
|---|---|
| `detect::PitchDetector` (`prepare_range`, `set_range`, `clarity`) | Reused as is (+ gfx-tuner's `candidates()` if it lands first) |
| `detect::SCALES` | Reused: `scale` 1 … 10 = SCALES[0 … 9] |
| **`psola`** (moved, shared with gfx-formant) | gfx-golem's epoch tracker, grains, voicing gate, octave rejection, `level_norm` and register table, moved from `gfx-golem/src/lib.rs` without behaviour change (gfx-golem's tests and strict null must stay green), plus gfx-formant's **fixed-delay streaming reader**. One module and one API for the three effects: a tracker that returns marks, the cycle period, clarity and the gate, and can be seeded backwards (§2.2); a reader with a delay D chosen by the caller (≥ 1.25 · T_max + 0.3 ms), per-grain r and α, 1 or 2 voices, identity at r = α = 1. Whichever of gfx-formant and gfx-correct is coded first makes the move; the other only uses it |
| **`pitch::PeriodShifter`** (new) | §2.5. ≈ 150 lines on `DelayLine` and `CenteredShifter`'s correlation search. `CenteredShifter` itself is not used |
| Vibrato model | In the crate (a 3 × 3 least-squares fit per cycle and rate): no other user yet |
| `specenv` (gfx-formant) | Stage 2 only (§2.6) |
| `svf`, `smooth` | Detection filters, ramps |
| `vocoder::PvShifter` | Not used (§1.3) |

**CPU** [A, to measure]: YIN Mid ≈ 6.7 M MAC / s (Low 15 M); epoch correlation and splice search on the
12 kHz signal ≈ 2 M; resampler, vibrato fit and control < 1 M. ≈ **0.5 % of one core** at 48 kHz mono in
Live, ≈ 1 % in Studio (gfx-golem's PSOLA measured 50 ns / sample). No allocation in `process`: buffers
sized for the Low register at 192 kHz in `prepare`. Deterministic: no random source anywhere.

### 2.9 v2: MIDI target (VST3 note input)

nih-plug `MidiConfig::Basic`. A new enum `mode` (x42's ids and order: Auto · MIDI · Manual, appended
after v1's parameters): **Auto** = held notes replace the allowed set while any is held; **MIDI** = only
held notes; **Manual** = v1. A further option "Melody" (exact MIDI note, octave included) would allow a
re-pitch of a sung line from a part. Pitch bend adds to `offset` (`bendrange`, 0 – 7 st as x42). Not in
v1: golem routes no MIDI to voice-lane effects today [I, golem-rack question].

## 3. Parameters

### 3.1 Ids and units: decisions

- **Catalogue ids kept**, with the catalogue's **names as display names** (join keys): `corr` "Correction",
  `filter` "Filter", `fastmode` "Fast Correction", `offset` "Offset", `m00 … m11` "C" … "B".
- **The 2026-11-14 rule** (third-party 0 – 1 scales): `corr` is linear in a real unit (fraction of the error
  removed) → kept 0 – 1, displayed in %. `filter` is already a **time constant in seconds** (§1.1; ours is
  exactly τ = `filter`, x42's within 6 ms) → kept, displayed in ms, minimum widened to 0 (instant) and
  maximum to 1 s, linear. `offset` is in semitones → kept. **No conversion of golem's stored values.**
- **`fastmode` keeps x42's meaning** (lower latency) as Live vs Studio. The plugin's default is **on**
  (golem is a live rig): golem's catalogue default (off) and its two presets that leave it off would put a
  live singer at 26 ms. Decided at the review (question 2): golem-rack flips the catalogue default and
  those two presets; until then its engine table pins it on at insert. The default depends on no other
  parameter, as the contract asks.
- **`key` / `scale` become real parameters** (golem's catalogue had them virtual): options in the
  catalogue's order by rank, gfx-pitch's scales appended. Effective notes = mask ∩ key / scale (§2.3), so
  both golem paths give the same result.
- `register`, `sens`, `thresh` from gfx-golem, same meaning; `reference` the A4 of x42 (`tuning` port) and
  of gfx-tuner's `reference`; `state` the contract's meter.
- No mode-dependent defaults: new parameters default to x42's behaviour (`transition` 0, `vibrato` 0 %,
  `flex` 0 %, `hysteresis` 23 ¢), so a bare insert sounds like the block it replaces.
  - `vibrato` 100 % (kept) would be the neutral value, and the better default. It stays at 0 % until the
    vibrato rows of the bench pass on sung DIs, because of the trill case (§2.3). Raph decides then;
    golem's presets carry 100 % from the start where it matters (§3.5).
- **Setup parameters**: `fastmode` and `register` change the reported latency. They belong to golem's
  Settings, not to scenes, EXP or the pedal's face.

### 3.2 Front panel

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `corr` | Correction | f | 0 – 1 (shown 0 – 100 %) | 1.0 | 20 ms | ✓ | Share of the pitch error removed. 1 = onto the note; 0.7 = audibly helped, still the singer's pitch |
| `filter` | Filter | f | 0 – 1 s (shown in ms) | 0.1 s | 20 ms | | How tightly a **held** note is kept (one-pole time constant). 0 = locked to the note; 0.1 – 0.3 s = natural drift kept. Higher = looser (x42's law) |
| `transition` | Transition | f | 0 – 500 ms | 0 ms | 20 ms | | How fast a **new** note is caught. 0 = instant step (x42, the stair-step robot); 20 – 60 ms = natural; 150 ms + = a slow slide into each note |
| `vibrato` | Vibrato | f | 0 – 200 % | 0 % | 30 ms | ✓ | The singer's vibrato through the correction: 0 % = treated as error (removed as far as `filter` allows), 100 % = kept intact even at `filter` 0, 200 % = doubled. Acts once a vibrato is recognised (2 – 3 of its cycles) |
| `key` | Key | e | C · C# · D · D# · E · F · F# · G · G# · A · A# · B | C | — | | Tonic of `scale` |
| `scale` | Scale | e | Chromatic · Major · Minor · Harmonic Minor · Melodic Minor · Dorian · Phrygian · Lydian · Mixolydian · Major Pentatonic · Minor Pentatonic | Chromatic | — | | Allowed notes (∩ the note mask). Append-only |
| `fastmode` | Fast Correction | b | off / on | **on** | setup | | On = Live (4.3 – 9.3 ms); off = Studio (PSOLA, exact formants, 17 – 37 ms, corrected from the first cycle). Changes the reported latency |

### 3.3 Expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `offset` | Offset | f | −2 – +2 st | 0 st | 20 ms | ✓ | Constant shift added to the correction; with `corr` 0, a plain transposition |
| `flex` | Flex | f | 0 – 100 % | 0 % | 30 ms | | Leaves far-off pitches alone: at 100 %, 15 ¢ off is pulled fully, 60 ¢ + is left as sung (bends, blue notes) |
| `hysteresis` | Note Hold | f | 0 – 50 ¢ | 23 ¢ | — | | How far past the midpoint the held note is kept (x42's `bias` 0.5 = 23 ¢). Higher = fewer note flips, more risk of holding a wrong note |
| `formant_keep` | Formant Keep | b | off / on | on | 30 ms | | Studio: keeps the vowel's formants when shifting (PSOLA); off = formants move with the pitch (x42). Live: no effect in v1, formants move with the pitch (§2.6) |
| `register` | Register | e | Low · Mid · High | Mid | setup | | Voice range (60 – 500 · 90 – 800 · 150 – 1,200 Hz); sets the Live latency |
| `reference` | Reference | f | 400 – 480 Hz | 440 Hz | — | | Pitch of A4 |
| `sens` | Sensitivity | f | 0 – 100 % | 50 % | 30 ms | | Voicing thresholds (gfx-golem): higher corrects rougher, breathier notes, at more risk |
| `thresh` | Threshold | f | −90 – 0 dB | −50 dB | 20 ms | | Under this input level, no correction (offset only): stage bleed is never retuned |
| `m00` … `m11` | C … B | b | off / on | on | — | | Note mask (hidden in golem; written by its compile) |
| `bypass` | Bypass | b | off / on | off | 10 ms | | Latency-preserving bypass |

### 3.4 Meters (read-only)

| id | Name | Range | Role |
|---|---|---|---|
| `state` | Voice State | 0 – 2 | 0 unvoiced / idle, 1 correcting, 2 gated by level |
| `note` | Target Note | 0 – 127 | MIDI number of the target (0 when none) |
| `error` | Pitch Error | −200 – +200 ¢ (clamped) | Steady pitch minus target, before correction: a tuner for the singer's monitor (x42's `error` port, in cents). A pentatonic gap with `hysteresis` at 50 ¢ reaches 200 ¢ |

### 3.5 Notes for golem-rack (engine table)

- No value conversion. Fix the catalogue labels / notes: `filter` "Speed" → a smoothing time, higher =
  looser; `fastmode` "Hard" → "Live / low latency"; block note.
- Re-voice the three golem presets (they are the misreadings): *Hard tune* `filter` 0.5 → 0,
  `transition` 0, `vibrato` 0 %; *Tight pop* `filter` 0.25 → 0.04, `transition` 15 ms, `vibrato` 100 %;
  *Transparent* `transition` 40 ms, `vibrato` 100 %, `flex` 40 %. All three `fastmode` on, and the
  catalogue default too: a preset recall must never change the latency (§2.7).
- `fastmode` and `register` are setup parameters: Settings only, never a scene value or an EXP target.
- Write `key` / `scale` to gfx-correct directly (the mask stays all on), keep `_expand_pitch_correct` for
  x42 under REAPER. Proposed: a virtual `song_key` for `pitch-correct` as for gfx-pitch: the song's tonic
  and Major / Minor replace key / scale unless the user's scale is already of that family, and **never
  when the scale is Chromatic** (a deliberate "any note") (question 3). Recommended at the review: today
  *Tight pop* and *Hard tune* are tuned to C whatever the song (§1.1). The plugin's `key` names are the
  catalogue's (C# … A#), gfx-pitch's are flats: golem writes the rank.

## 4. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Transparent live help (golem's default use) | corr 0.8, filter 0.15 s, transition 40 ms, vibrato 100 %, flex 40 %, song key, Live | Steady pitch corrected, vibrato and bends untouched, ≤ 9.3 ms |
| Singer with in-ears | register High (sopranos, 4.3 ms) or Mid (6.6 ms); Low only for basses | The latency is the register's: choose the highest that holds the voice's lowest note |
| Tight pop vocal | corr 1, filter 0.04 s, transition 15 ms, vibrato 100 %, key / scale | Two times: notes land fast, held notes stay human |
| Stair-step robot (2000s hard tune) | corr 1, filter 0, transition 0, vibrato 0 %, hysteresis 10 ¢, Minor | Instant steps, no vibrato |
| Modern hard tune keeping vibrato | corr 1, filter 0, transition 0, vibrato 100 % | Vibrato split |
| Exaggerated vibrato (effect) | corr 1, filter 0, transition 10 ms, vibrato 180 % | Vibrato gain |
| Backing vocals, choir lines | corr 1, filter 0.02 s, transition 30 ms, vibrato 50 % | Unison lines lock together |
| Blues / soul, bends kept | corr 0.9, filter 0.1 s, transition 50 ms, flex 100 %, Minor Pentatonic | Flex leaves blue notes |
| Weak singer, sparse scale | Major Pentatonic, hysteresis 35 ¢, transition 40 ms | Hysteresis: no hopping between distant notes |
| Studio render of a sung take | fastmode off, corr 1, filter 0.1 s, transition 30 ms, vibrato 100 % | Look-ahead: every onset corrected; exact formants |
| Fretless bass, cello (mono) | register Low, Chromatic, corr 0.7, filter 0.2 s, transition 60 ms, vibrato 100 % | 60 Hz detection |
| Trumpet, sax, violin | register High (violin) / Mid, corr 0.6, filter 0.2 s, flex 60 % | Register; flex keeps scoops |
| Voice a semitone lower for a capo'd song | corr 0, offset −1 st, formant keep on, `fastmode` off (Studio) | Plain transposition, formants kept. Live moves the formants by 6 % until stage 2 (§2.6) |
| Singer's pitch meter | any; read `error` and `note` | Read-only meters |

Factory presets (golem-fx `presets/gfx-correct.json`, ≈ 15, one per use above plus *Live Natural* = the
x42 *Live* preset (corr 0.84, filter 0.033 s) and *Slightly Corrected* (corr 0.33, filter 0.0425 s)), no
artist names in names. Place after the de-esser and before the compressor (golem's voice order).

## 5. Bench suite (on top of gfx-bench's common checks)

Signals: gfx-golem's synthetic voice (Rosenberg pulses through /a/ formants 700, 1,220, 2,600 Hz), with
0.7 % cycle jitter, vibrato, scoops, slides, trills, melismas, aspiration noise and /s/ bursts; Raph's
sung DI; an x42 render of the same inputs (golem REAPER) for A / B. All measured after removing the reported latency.

| Check | Pass |
|---|---|
| Latency | Measured = reported ±1 sample, both modes, three registers, 44.1 – 192 kHz; Live ≤ 9.3 ms |
| Identity | corr 0, offset 0: output nulls against the delayed input ≤ −60 dB (Live) / −45 dB (Studio), voiced and unvoiced |
| Re-centre (Live) | A corrected note, then /s/: 20 ms after the voicing closes the output is the input delayed by C ±1 sample; the 5 ms move leaves no click above the common bound |
| Snap | Steady vowel +35 ¢ off A3, filter 0, transition 0: within ±3 ¢ of A3 after 2 periods of the first reading |
| Laws | Pitch step response follows τ = `filter` (held) and τ = `transition` (onset) within ±10 %; `corr` 0.5 removes 50 ± 3 % |
| Vibrato kept | ±20, ±50 and ±100 ¢ at 4.5, 6 and 7.5 Hz on a note 20 ¢ off, filter 0, vibrato 100 %, from 0.6 s after the vibrato starts: centre within ±3 ¢, depth within ±10 % |
| Vibrato removed | ±50 ¢ at 6 Hz, vibrato 0 %: residual depth ≤ 15 % (Live, 220 Hz in Mid), ≤ 30 % (Live, 100 Hz in Mid), ≤ 5 % (Studio) |
| No warble | Vibrato ±60 and ±100 ¢ centred on a midpoint (Chromatic), vibrato 100 %: at most 6 target flips in the first 0.6 s, none after; then one note plus the kept vibrato |
| Note under vibrato | Step of 100, 200 and −300 ¢ in 50 – 200 ms under a ±50 ¢ vibrato: target changed ≤ 40 ms after the middle of the transition; split back ≤ 0.35 s after the landing |
| Not a vibrato | Melismas of 100 and 200 ¢ every 150 ms, a whole-tone alternation at 5.5 Hz, a squared semitone trill at 6 Hz, a scoop, a 60 ¢ bend in 0.5 s, vibrato 100 %: every note quantised as with vibrato 0 % (report the trill's result: §2.3) |
| Jitter | Steady vowel with 0.7 % cycle jitter, 20 ¢ off: output jitter ≤ 1.25 × the input's at filter 0 (Live), ≤ 1.05 × at filter ≥ 20 ms and in Studio |
| Scoop | 80 ¢ scoop into a note, filter 0.3 s, transition 30 ms: no residue > 5 ¢ 100 ms after landing |
| Scale / mask | Chromatic slide C4 → E4 in C Major: steps only on C, D, E; mask ∩ scale both paths; all off = no correction |
| Hysteresis | Switch point at 50 + `hysteresis` ¢ ±3 ¢ |
| Flex | flex 100 %: 15 ¢ error removed ≥ 90 %, 70 ¢ error removed ≤ 10 % |
| Octave errors | Vowel with a 2nd harmonic +6 dB over the fundamental, a creaky vowel: no ×2 / ÷2 target, no epoch slip |
| Wide leap | Legato octave and tenth down and up (220 ↔ 110, 330 ↔ 131 Hz): target right ≤ 40 ms after the landing, ratio held meanwhile, no click |
| Breathy | Voice at 10 dB harmonics-to-noise, 50 ¢ correction: the noise part's level modulation ≤ 0.5 dB outside the splices; no spurious line above −30 dB re the voice; `state` drops instead of glitching |
| Unvoiced | /s/ and breath: output = delayed input (offset 0) within −40 dB; vowel → /s/ → vowel without a click |
| Splices | Steady vowel, offset +1 st: inharmonic residue ≤ −35 dB, no click above the common bound |
| Formants | offset ±2 st: envelope peaks move ≤ 2 % (Studio, formant keep), by r ±1 % with keep off and in Live (v1). Stage 2: ≤ 4 % in Live with formant keep |
| Level | RMS within ±0.5 dB of the input for corrections up to ±1 st |
| Bleed | Pitched input under `thresh`: offset only, no correction |
| Changes | `key`, `scale`, `corr`, `vibrato` changed mid-note: no click; target glides at `transition` |
| Onset (Live) | Corrected pitch within 10 ¢ ≤ 25 ms after a sung onset (Mid, 220 Hz), ≤ 40 ms at 100 Hz |
| Onset (Studio) | The first cycle of a note sung 40 ¢ flat after silence and after /s/ plays within 5 ¢ of the target (filter 0, transition 0), 100 and 220 Hz in Mid |
| A / B | Blind, level-matched, Raph's voice: preferred to x42 at Transparent and Hard settings |
| Determinism, CPU | Strict null; ≤ 1 % of a core at 48 kHz mono in Live, ≤ 1.5 % in Studio |
| gfx-golem | After the move to `psola`: gfx-golem's tests and its strict null against its own earlier render |

## Visual

- **Category** `pitch`. **Crate hue `#7a1450`** (deep plum: a darker sister of the pitch family's
  `#c4148c`), rune light `#ff5fb4` (a `RUNE_LIGHT` crate entry: the default tint of so dark a hue is
  dull). **Finish**: anodized (the pitch family's). **Approved at the review, with one check.**
  - ΔE00, recomputed: 16.0 from pitch magenta (so from gfx-pitch, gfx-freqshift and gfx-formant, which
    keep the family hue), 20.6 from the preamp violet `#b316c0`, 21.1 from gfx-swell's rose `#d81e5b`,
    24.8 from the lofi raspberry `#e8336f`, 23.4 from reverb `#6a1fd1` [A].
  - Why a crate hue here when gfx-freqshift and gfx-formant have none: on a voice lane this pedal sits
    beside gfx-formant, and both would be magenta with a line motif.
  - **The check**: the plum is dark (L* 27, against 44 for the magenta) and the body's gradient halves
    it again. Its other voice-lane neighbour is gfx-golem, in blackwork. If the two read alike at tile
    size on the family sheet, fall back to the family magenta with no crate hue (the `staff` motif then
    does the work, as for gfx-formant).
- **Motif `staff`** (new): five engraved staff lines across the face; on the left half a thin free-hand
  pitch trace wanders between them with a small vibrato; on the right half it becomes flat segments resting
  exactly on lines and spaces, joined by short vertical risers. Drawn as two paths in the motif colour, the
  trace at 60 % opacity. At tile size the five lines become three, or they merge into a grey band.
- **Face** (`faces.json`): `{"name": "Correct", "blocks": ["pitch-correct"], "knobs": ["corr", "filter",
  "transition", "vibrato"], "selectors": ["key", "scale"], "category": "pitch", "color": "#7a1450"}`.
  No toggle: `fastmode` changes the latency, a foot or a finger must not reach it mid-song.
- **Glyph**: the block's existing `visuals/glyphs/pitch-correct.svg` (a note and a check) stays, since the
  face's first block names the glyph. If a crate glyph is wanted, a wavy line snapping into steps:
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>Pitch correction (gfx)</title><path d='M2.5 14c1.3-3 2.7-3 4 0s2.7 3 4 0'/><path d='M13 13.5h3v-4h3v-4h2.5'/><path d='M3 20.5h18' opacity='.5'/></svg>`

## Open questions for review

Answered at the review of 2026-11-14.

1. **Live formants** → **no LPC.** The prototype rejects it (§2.6). v1: `formant_keep` acts in Studio
   only. Stage 2: gfx-formant's `specenv` after the shifter, behind the same blind gate.
2. **`fastmode` default** → **on in the plugin; golem-rack flips the catalogue default and the two presets
   that leave it off**, and pins it on at insert until then. All golem presets carry the same value, so a
   preset recall never re-syncs the host.
3. **Song key** → **recommended: follow by default, never over Chromatic** (as written in §3.5). Today two
   of the three presets are tuned to C in every song. Raph decides.
4. **Vibrato thresholds** → the band-pass split is replaced by the model fit (§2.3), with prototype values:
   window 1.25 periods, 4 – 8 Hz, extent 12 – 110 ¢, residual < 0.3 · A, exit at max(30 ¢, 0.5 · A) for two
   cycles, ramps 60 / 20 ms. They and the onset-phase end (2 st / s for 30 ms, 250 ms cap) are still [A]:
   to set on sung DIs. The semitone trill stays an open risk, which is why `vibrato` defaults to 0 %.
5. **Latency by `fastmode` and `register`** → **accepted as setup parameters** (the rule of gfx-formant and
   gfx-glitch). One Live figure for all registers would cost sopranos 5 ms for nothing. Kept out of scenes,
   EXP and the face.
6. **Breathy voices** → no second engine. One head never combs the breath (§2.5); where no period is
   trusted the jump length comes from a free correlation search (±50 % of the last period), and at
   `offset` 0 the head does not move at all.
7. **MIDI target (v2)**: golem-rack's question, unchanged.
8. **Crate name — settled 2026-11-14**: "Auto-Tune" is Antares' trademark, so the crate was renamed from
   its working name to `gfx-correct` before any code; the face says "Correct". The README names the
   product only as a reference, which is fair use of the mark.
9. **`offset`** → **kept at ±2 st** (the catalogue's range). Larger shifts belong to gfx-pitch, gfx-golem
   and gfx-formant's Grain engine.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good; the vibrato split was at risk and is redesigned.** The frame is right and is what
separates this from x42 and from the free tuners: a cycle-rate pitch beside YIN, the steady pitch
corrected apart from the vibrato, two times with a precise law, a resampling engine at half a period of
latency and PSOLA for the exact path. The reading of x42 was checked line by line against `retuner.cc`,
`retuner.h`, `fat1.cc`, the ttl and the presets, and holds on every point that drives a decision:
`filter` is a smoothing time in seconds (coefficient 4 · 128 / (v · fs), τ ≈ v − Δ / 2; the ttl's own
scale points are Slow 0.5 / Fast 0.02), so golem's "Speed" label, its note and *Hard tune* at 0.5 are
inverted; `fastmode` is the read head moved 7/16 of the buffer (1,056 against 160 samples reported), not
"Hard"; `corr` is a linear fraction; `bias` 0.5 is 46 ¢ off the held note's distance, a flip at 73 ¢.
golem's side (catalogue, three presets, `_expand_pitch_correct`, `song_key_of`, `_expand_pitch_harmony`)
is as described, bar the presets' scales. No x42 code is used: every algorithm here is a different one,
and the licence (GPL-2-or-later) is compatible in any case. The design had two mechanisms that fail by
their own bench rows, one claim its numbers did not carry, and one module that duplicates a sister
design. Prototypes: `/tmp/claude-1000/-home-raph-code/e0dc0b09-d78a-4463-a90e-9918eadd7ea8/scratchpad/proto-gfx-correct/`
(`vib.py` the first design's split, `vib3.py` the model fit, `lpc.py` the re-filter, `de.py` the hues).

Changes made, and why:

| # | Change | Why |
|---|---|---|
| 1 | **Vibrato split: gated 5.5 Hz band-pass → sinusoid model fit with a prediction test** (§2.3, §2.4) | Prototype of the first design: the 15 st / s slope gate is under a ±50 ¢, 6 Hz vibrato's own slope (19 st / s), so w_v stayed at 0 for every depth above ±40 ¢; a vibrato crossing a note boundary flipped the target before the split engaged and each flip closed it (12 flips / s for ever: the "No warble" row could not pass); without the gate, a note step rang into the band-pass (65 % after 32 ms) and the target came 36 – 76 ms late. The fit engages in 2 – 3 vibrato cycles, holds on ±20 – 100 ¢ at 4.5 – 7 Hz including on a midpoint, sees a note step 0 – 35 ms after its middle, and stays off on scoops, bends, drift and melismas |
| 2 | **`vibrato` 100 % keeps p − c, above 100 % adds the model's sinusoid** | "Kept exactly" was not true of a band-pass output (gain 0.88 – 1, phase ±28° over 4 – 8 Hz); deepening p − c would deepen the jitter |
| 3 | **Live shifter: a new `PeriodShifter`, not a lock on `CenteredShifter`** (§2.5) | `CenteredShifter` plays two heads all the time: on a breathy voice the noise is combed and flutters. The design's own description (one head, a short crossfade per splice) is another structure |
| 4 | **Head re-centred when the voice stops**; identity row measured after a voiced passage | The head stopped wherever the note left it, up to T / 2 from C: "delayed by C, exactly" was false and consonants moved by up to ±5.6 ms |
| 5 | **Control delayed to meet the audio**; alignment stated per pitch | The cycle pitch is 1.3 periods old, not one, and at high notes it was ahead of the audio. "≤ 3 ms" holds from 220 Hz up in Mid; a 100 Hz voice is 6 ms late |
| 6 | **Splice refined at the full rate** (±2 samples) | A 12 kHz search grid is 4 samples wide |
| 7 | **LPC re-filter and the `lpc` module removed; Live formants staged on gfx-formant's `specenv`** (§2.6) | Prototype: order 20 at 48 kHz halves the envelope error at best (6.5 → 3.2 – 4.2 dB), order 48 fails on high voices. gfx-formant builds the right tool for the same job: one envelope module |
| 8 | **Studio delay 1.25 · T_max + 4 ms → 2 · T_max + 4 ms** (17.3 / 26.2 / 37.3 ms) | "Every note corrected from its first cycle" needs YIN's first reading before the note plays: T_max + T + 2.7 ms. The first figure missed it in Mid below 245 Hz |
| 9 | **Studio: unvoiced sound is never re-spaced** (r = 1, marks freewheel) | PSOLA on noise at r ≠ 1 buzzes at the mark rate; the design sent `offset` through it |
| 10 | **`psola`: one module with gfx-formant**, API stated (§2.8): tracker with backward seeding, fixed-delay reader with the caller's D | Both designs move gfx-golem's engine; gfx-formant already specifies the streaming reader this Studio engine needs |
| 11 | **Onset timing written out** (§2.7): Live, Mid, 220 Hz: first 9 – 12 ms uncorrected, complete at ≈ 20 ms | The answer to "how soon is a hard tune hard"; bench rows for 100 Hz and for Studio |
| 12 | **Jitter** stated (§2.2) with a bench row | A one-cycle-late correction raises the white jitter by 1.22 at `filter` 0. Not hidden, measured, with a fallback |
| 13 | **YIN range narrowing**: full range restored on the first unclear analysis; wide-leap bench row | A fall below 0.6 · f found no dip and waited for the 30 ms hold |
| 14 | x42 facts: window **42.7 ms** (not 21.3), fast-mode lag **19 – 29 ms** (not 8 – 19), a 6 Hz vibrato keeps **87 %** (not 49 %); peak rule, unvoiced count, re-centring; scoop residue 27 / 50 ¢ (not 23) | The window reads every second sample of a 4,096-sample upsampled buffer. All in the design's favour |
| 15 | golem facts: presets are Chromatic / C Major / C Minor, not "C Chromatic in every preset" | It makes the song-key question a real fault: two presets are tuned to C in any song |
| 16 | `error` meter ±100 → ±200 ¢; `formant_keep` text; `fastmode` off the pedal's face; setup-parameter rule; Antares' own latency figures disagree (noted); Graillon's 23 ms marked [U] | Accuracy and contract hygiene |
| 17 | Bench: vibrato rows over depth and rate, note under vibrato, not-a-vibrato (trill, melisma), jitter, re-centre, wide leap, breathy (noise flutter), Studio onset, gfx-golem null after the move; CPU bound lowered | Each new mechanism has its row |

Contract: catalogue ids kept with their units and no conversion (`corr` linear fraction, `filter` seconds,
`offset` semitones: the 2026-11-14 rule applies as the design says); ranges a little wider; `key` and
`scale` in the catalogue's order with the new scales appended; mask ∩ scale makes both golem paths give
the same notes; no default depends on another parameter; `bypass`, read-only meters, mono first: all in
order.

Still open:

| Point | Owner |
|---|---|
| Catalogue: `filter` label and note (a time, higher = looser), `fastmode` label (Live), default `fastmode` on, the three presets re-voiced (§3.5), `key` / `scale` written by rank, `fastmode` and `register` as Settings only | golem-rack |
| `song_key` for `pitch-correct` (recommended: on, never over Chromatic) | Raph, then golem-rack |
| `vibrato` default: 0 % now; 100 % once the vibrato rows pass on sung DIs | Raph, after the bench |
| Studio at 17 – 37 ms (first cycle corrected) against the first design's 12 – 25 ms (first 0 – 16 ms of some onsets uncorrected) | Raph, if a shorter Studio matters more than the claim |
| Vibrato model thresholds, onset-phase end, the semitone trill, jitter at `filter` 0 (two or four cycles) | Bench, on sung DIs (male, female) |
| Live formants, stage 2: after gfx-formant's `specenv`, behind the blind gate | golem-fx, after gfx-formant |
| Who moves gfx-golem's engine to `psola` (the first of gfx-formant and gfx-correct to be coded), and gfx-formant's review agreeing to the API in §2.8 | golem-fx |
| MIDI to a voice lane (v2) | golem-rack |
| Plum against gfx-golem's blackwork at tile size; three staff lines or five | Raph, on the family sheet |

### Decisions by Raph (2026-11-14)

- **Song key**: `pitch-correct` follows golem's song key (`song_key_of`), never over Chromatic.
- **Studio latency** 17.3 / 26.2 / 37.3 ms (Low / Mid / High `register`) is accepted.
- `vibrato` stays 0 % by default until the bench passes on sung DIs (the trill case), then 100 %.

## As coded (v0.1.0, 2026-11-15)

**Files.**
- gfx-dsp `psola.rs` (new): `Tracker` (gfx-golem's history, detector, octave rejection, voicing gate and
  epoch tracker, moved without an arithmetic change; plus `predicted_next`, `set_detector_range` and the
  optional backward seeding), `Reader` (the fixed-delay streaming reader of §2.7 / gfx-formant §2.3),
  `REGISTERS`, `voicing_thresholds`, `level_norm`, `hermite`, `Events`. 4 unit tests (marks, seeding,
  identity through an unvoiced burst, a +2 st shift).
- gfx-dsp `pitch.rs`: `PeriodShifter` (§2.5). 1 unit test (identity, ±1 / +0.5 st within 2 ¢, re-centred
  identity).
- gfx-golem `src/lib.rs`: uses `psola::Tracker`; its overlap-add writer and scheduler stay in the crate.
  **Bit-exact null**: 54 renders made before the move (guitar DIs lead, lead-clean, clean; the sung
  phrase at 44.1, 48 and 96 kHz; default, −24, a free interval with formant +4, formant −4 with a second
  voice, harmony, register Low with humanize and fine, register High with formant link and glide,
  filters and gate settings, stereo in blocks of 97) are byte-identical after it. Its 7 tests and bench
  (28 / 28) are green.
- `src/lib.rs` (the processor `Correct`, `PARAMS`, `SCALE_NAMES`, `latency_samples`, `allowed_notes`),
  `src/plugin.rs` (VST3 class `GoleMFxCorrect01`, mono 1 / 1 then 2 / 2, latency reported and updated
  on a setup change, `bypass` = `make_bypass`, meters `state`, `note`, `error`), `src/tests.rs` (15
  tests).
- `presets/gfx-correct.json`: 16 presets (§4, plus x42's *Live* and *Slightly Corrected*).

**Measured** (synthetic /a/ of §5, 48 kHz; the output's pitch read by the same epoch tracker).

| §5 row | Bound | Measured |
|---|---|---|
| Latency | Live ≤ 9.3 ms; ±1 sample | Live 448 / 315 / 208 samples (9.33 / 6.56 / 4.33 ms), Studio 1,792 / 1,259 / 832 (37.3 / 26.2 / 17.3 ms); the bench measures = reported at 44.1 – 192 kHz |
| Identity | ≤ −60 dB Live, −45 dB Studio | exact (max error < 1e-5, f32 rounding) in both modes, three registers, vibrato voice → noise → voice |
| Re-centre (Live) | delayed by C ±1 sample 20 ms after the voicing closes | exact (bit for bit) from 77 ms after the voice ends: YIN's last frames + the 30 ms hold, then the 30 ms weight ramp (§2.2) to g_v = 0, then the 5 ms move (§2.5). About 35 ms after the gate closes: the spec's own 30 ms ramp precedes the move, so its 20 ms cannot hold |
| Snap | ±3 ¢ | passes, both modes (+35 ¢ off A3) |
| Laws | `corr` 0.5 removes 50 ± 3 %; τ = `filter` ±10 % | passes both modes; the remaining error after τ is e^−1 ± 0.1 |
| Vibrato kept | centre ±3 ¢, depth ±10 % | Live +1.6 ¢ / 99.0 %; Studio +0.6 ¢ / 98.9 % (±50 ¢, 6 Hz, note 20 ¢ off) |
| Vibrato removed | ≤ 15 % Live 220 Hz, ≤ 5 % Studio | Live 13.3 %, Studio 1.0 % |
| No warble | ≤ 6 flips in the first 0.6 s, none after | ±60 and ±100 ¢ on a midpoint: 3 and 3, none after |
| Jitter | ≤ 1.25 × (Live, filter 0), ≤ 1.05 × (filter ≥ 20 ms, Studio) | 0.78 ×, 0.99 ×, 0.89 × (0.7 % cycle jitter, 9.5 ¢ rms cycle to cycle) |
| Scale / mask | steps on the scale; mask ∩ scale both paths; all off = none | passes (unit rows + the identity with every mask bit off) |
| Hysteresis | switch at 50 + `hysteresis` ¢ | held at +72 ¢, switched at +74 ¢ (23 ¢) |
| Flex | 15 ¢ removed ≥ 90 %, 70 ¢ ≤ 10 % | passes |
| Level | ±0.5 dB to ±1 st | +0.00 dB Live, +0.17 dB Studio (+1.45 st) |
| Bleed | offset only | exact delayed input, `state` 2 |
| Onset (Live) | within 10 ¢ ≤ 25 ms (220 Hz, Mid) | 24.9 ms |
| Onset (Studio) | first cycle within 5 ¢ | first three cycles within 0.4 ¢ (220 Hz and 101 Hz, after silence) |
| Determinism, stereo | strict | bit-identical; stereo L = R = mono |
| CPU | ≤ 1 % Live, ≤ 1.5 % Studio | Live 55 ns / sample (0.26 %), 64 ns stereo (bench); Studio renders of the 27 s sung phrase take the same time as Live (≤ 100 ns / sample with file I/O) |
| gfx-golem | strict null after the move | byte-identical, 54 renders |

**Deviations from the spec, and why.**
- **YIN's input** is the tracker's (gfx-golem's), with no extra 50 Hz high-pass: the tracker is shared and
  bit-exact for gfx-golem. The 1 kHz marker low-pass is the tracker's own.
- **Epoch guard**: the tracker keeps gfx-golem's rule (a measured period counts only within 30 % of
  YIN's; the period is pulled 5 % per mark toward YIN's), and a cycle pitch more than 30 % off the
  smoothed period is not given to the correction. No separate re-seed.
- **Studio cycle pitch**: the mean of the last three cycles (two in the register's lowest 15 %), stamped
  at its centre; with the control timeline every value meets its audio exactly. A voiced onset is
  back-dated: the correction weight ramps over the 10 ms before the first (backward-seeded) epoch, so the
  first cycle plays corrected.
- **Control timeline** (not in the spec's words, its intent): each control value carries the input time
  it describes; Live reads it at t − C (the "control delayed to meet its audio"), Studio's reader asks
  each grain's source cycle for its ratio.
- **Live splice search** runs on the full-rate analysis line with a ~12 kHz step, then ±2 samples at the
  full rate and a parabola (same result as a separate decimated copy, one buffer less).
- **Studio level**: per-grain gain 1 (the pulse amplitude kept), not `level_norm`: +0.17 dB at +1.45 st.
- **Studio `formant_keep` off** reads the grains at α = r from the plain history: no anti-alias copy
  (α ≤ 1.26 at the largest shift, images above 19 kHz at 48 kHz).
- **Plugin display**: `corr` is shown as a fraction and `filter` in seconds (the catalogue's own units),
  not % / ms, so golem's descriptors and the host agree.
- **psola `Reader`**: a grain starts when its known left half begins (else 1.3 periods ahead): starting
  early costs nothing, so the predicted mark is committed only when the next epoch is genuinely late. A
  gap between marks (the voice's return after freewheeling, a note under the floor) is bridged by evenly
  spaced committed marks.

**Not measured yet** (need sung DIs, x42 renders or more synthetic rows): A / B against x42, Formants
(envelope peaks in Studio), Breathy, Octave errors, Wide leap, Note under vibrato, Not a vibrato (the
semitone trill), Scoop, Changes, Splices residue. The vibrato model's thresholds and the onset-phase end
are the prototype's values (§2.3, §2.4), to set on sung DIs (Monday's session).
