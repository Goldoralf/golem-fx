# gfx-pitch

> **Status: CODED** (v0.1.0, 2026-11-12) — three VST3s; Whammy, classic and detune on gfx-dsp's centred splice-search shifter (fixed mean delay, per-sample ratio, correlation splices with sub-sample refinement), Poly on gfx-dsp's low-latency phase vocoder: **gfx-pitch** (Whammy, 12 ms reported; Poly, 6.6 ms Fast / 10 ms Stable reported; second voice, fine, glide, slew limit; dry delayed by the latency), **gfx-pitch-classic** (superpitch's two-head splicer, window / overlap literal, optional splice search, latency 0), **gfx-detune** (two ±25-cent voices, 3 ms centre, doubling delay by crossfade, latency 0); all allocate in `prepare` only. Tests: Whammy in tune within 0.2 cents at every integer shift from −24 to +24 at 82 – 880 Hz (measured ≤ 0.07; until 2026-11-13 the second head spliced against the first head's already-advanced position, a drift growing with the shift: +1.5 cents at +12, +6.8 at +24), detune exact to 0.001 cent. A grain that a sweep would carry out of the line (planned at one ratio, the ratio moving meanwhile) has the rest of its fade compressed at the same window phase, decided for the head pair so they stay staggered: the worst parameter-step burst went from −55 to −82 dB, poly octaves each note of a chord, sweeps without a splice click, latency = the engine's centre; bench green. **Poly, 2026-11-13: the phase vocoder ships** (gfx-dsp `vocoder.rs`, renamed `pv_shift.rs` on 2026-11-14; the band-split candidate it replaces is in git history, commit 3c2653d). Peak-locked shifting (Laroche & Dolson 1999: each spectral peak moves its region, bins keep their phases relative to the peak) on **asymmetric windows** (Mauler & Martin 2007): a long analysis window (8192 at 48 kHz, 6 Hz bins: a low-E power chord is resolved) that falls only over the last `m` samples, and a short synthesis window (`2m`), so the latency is the synthesis window's, not the analysis length — Fast `m` 160 (319 samples, 6.6 ms), Stable `m` 240 (479, 10 ms), hop `m`. Phases are referenced to the synthesis centre; a peak's phase advances by the ratio × its measured advance from the previous frame's nearest peak, so at ratio 1 the output is exactly the delayed input (tested on noise: Poly at shift 0 mixes with the dry without combing). Both voices share one analysis. Measured on ten guitar chords (sines; `poly_metric`): a partial keeps −1.6 to −1.7 dB of its level on its target on average, −6.3 dB at worst (Stable; Fast −2.1 / −8.2), where the band-split kept −11 to −20 on average; on a sequence of decaying chords with 6 harmonics, 95 – 99 % of the wet's energy lands on the shifted harmonics (purity −0.2 to −0.5 dB; band-split −1 to −5.6), and the previous chord 40 – 120 ms after a change sits −17 to −24 dB down (band-split −18 to −29). Prototyped first in Python (scratch): a multi-resolution split (long frame for the lows, short for the highs) measured no better than one long frame, and a 4096 frame lost close low chords (−27 to −31 dB worst), so both modes share the 8192 analysis and differ by latency. CPU per channel at 48 kHz: 150 – 240 ns / sample on a six-note chord (Stable / Fast; the 8192 FFT pair is 72 / 108 of it), 520 – 790 ns on white noise, the worst case (a third of the bins are peaks; libm trig replaced by ~1e-5 polynomials, branch-free peak picking). gfx-bench checks it as `pitch-poly` (gfx-pitch with the Poly engine): 28 / 28, latency measured = reported at every rate. Renders A/B against the band-split: `bench-out/pitch-poly/` (clean.wav at −5 / +7 / +12, both modes; the vocoder keeps the DI's level within 2 dB, the band-split lost 5.5). **Phase 2 (harmony) coded 2026-11-12**: `harmony` / `key` / `scale` appended; YIN detector in gfx-dsp (`detect.rs`, ~12 kHz analysis every ~2.7 ms, 75 Hz – 1.5 kHz, harmonic-lock check) with a confirming note tracker; on the lead DIs the tracked pitch class agrees with an offline reference on 97 % (lead) / 97 % (lead-clean) of voiced frames, the rest being onset timing, and the harmony voice stays in the scale 95.5 % of the time (a fixed +4: 59 – 61 %). Detector cost ~60 – 80 ns / sample at 48 kHz, only while `harmony` is on. **Triggered bend DESIGNED 2026-11-26** (roadmap 0.8 item 4, §2.1, not coded): the Whammy Ricochet's movement, `bend` / `interval` / `rise` / `fall` / `curve` / `rest` appended; with `bend` never pressed the output is bit-identical to v0.1.0. Reviewed by Fable 5.2 the same day (folded in; see the Review section at the end): the wet at rest is not the delayed input once a bend has returned (measured), hence `rest`. **Decided by Raph, 2026-11-26:** the gesture bool is `bend` and the interval `interval` (the draft's `trigger` / `bend`); `rest` (Mix · Dry, default Mix) and the 100 / 250 ms, 8 s, Linear defaults accepted.

golem's pitch shifters: whammy, harmony, poly transposition, detune / doubler. They stand for three golem
blocks ([golem contract](../docs/golem-contract.md)):

| golem block | Today | Catalogue ids |
|---|---|---|
| `pitch` | ReaPitch — **quarantined** (it freezes the engine) | `shift` −12 … 12 st, `mix` 0 … 100 % |
| `pitch-shift-js` | REAPER JS superpitch | `shift` −12 … 12 st, `window` 10 … 100 ms, `overlap` 1 … 50 ms, `wet` / `dry` −120 … +6 dB |
| `detune` | guitarix GxDetune | `detune` 0 … 1, `wet` 0 … 1, `dry` 0 … 1 |

`wet` and `dry` mean dB in one block and 0 … 1 in another: one plugin cannot carry both under the same id.
So **three VST3s on one shared core** (in gfx-dsp):

- **gfx-pitch** → `pitch`: the modern shifter (Whammy and Poly engines).
- **gfx-pitch-classic** → `pitch-shift-js`: the delay-line shifter with superpitch's window / overlap.
- **gfx-detune** → `detune`: detune / doubler.

Unlike every other golem-fx effect, a good pitch shifter needs **some latency**. It is fixed per engine,
reported to the host at initialisation or engine change only (never when `shift` moves), and kept
whammy-grade.

## 1. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference or not re-checked. Latency figures from
forums are anecdotal.

### Commercial references

| Product | Approach / latency | What we take |
|---|---|---|
| DigiTech Whammy 5 / DT / Ricochet ([guide](https://www.soundtech.co.uk/news/digitech-whammy-range-drop-dt-ricochet-guide-pitch-shifting)) [V] | "Classic" (mono, 44.1 kHz) and "Chords" (poly, runs at 22 kHz, 20 Hz – 11 kHz) | Two engines; the poly one may run at reduced bandwidth to save latency and CPU |
| Line 6 Helix ([KVR](https://www.kvraudio.com/forum/viewtopic.php?p=8713154)) [V, anecdotal] | Simple Pitch: no latency but smeared transients; Poly Wham ≈ 9 ms, Poly Pitch ≈ 10 ms, Poly Capo ≈ 10 ms; "Fast" vs "Stable" tracking | **~10 ms poly is achievable**; a speed / stability choice |
| Fractal Axe-Fx III ([Blocks Guide](https://www.fractalaudio.com/downloads/manuals/fas-guides/Fractal-Audio-Blocks-Guide.pdf)) [V] | Classic / Advanced Whammy, Virtual Capo (~10 – 25 ms), Detune… | Latency per type is accepted practice |
| Eventide H9 / PitchFactor / MicroPitch ([Eventide](https://www.eventideaudio.com/forums/reply/148067)) [V] | A few ms of I/O latency; H910 / H3000 are delay-line splicers, H3000 added a splice-point search ("deglitch") [I] | MicroPitch-style detune at almost no latency; splice search |
| Boss OC-5 [V, anecdotal] | Analog-style octave down ≈ 1 ms; poly octave up ≈ 10 ms | Subharmonic octaves belong to golem's `octaver` block |
| Mooer Pitchbox [V, anecdotal] | ≈ 19 ms | What "too slow" feels like |
| Soundtoys Little AlterBoy ([KVR](https://www.kvraudio.com/forum/viewtopic.php?p=6266972)) [V] | ≈ 60 ms: formant quality costs latency | Formants: not for v1 |

### Algorithms

- **Delay line ("rotating tape")**: read heads sweep through a buffer and crossfade every window; the ratio
  can change every sample, so EXP sweeps are naturally smooth. Splice artefacts (comb, flutter). This is
  what superpitch's window / overlap control [I].
- **WSOLA** (Verhelst & Roelands 1993) / **PSOLA** (Moulines & Charpentier 1990): the splice point is chosen by
  cross-correlation or on pitch periods; most of the warble disappears on single notes for one search
  window of lookahead; fails on chords.
- **Phase vocoder**: Laroche & Dolson 1999 (peak-locked shifting); Průša & Holighaus, *Phase Vocoder Done Right*
  ([arXiv](https://arxiv.org/abs/2202.07382)) [V]: phase-gradient integration, no peak picking, polyphonic,
  latency ≈ one frame; Juillerat & Hirsbrunner report frequency-domain shifting at 12 ms
  ([AES](https://aes2.org/publications/elibrary-page/?id=14788)) [V].
- Survey: Driedger & Müller, *A Review of Time-Scale Modification of Music Signals*, 2016; Geraint Luff,
  *Four Ways to Write a Pitch-Shifter*, ADC 2022, with [code](https://github.com/Signalsmith-Audio/pitch-time-example-code) [V].

### Open source

| Project | Licence | Note |
|---|---|---|
| [Signalsmith Stretch](https://github.com/Signalsmith-Audio/signalsmith-stretch) [V] | MIT | Polyphonic, best permissive reference; defaults ≈ 100 ms, shorter blocks configurable |
| [Rubber Band](https://breakfastquay.com/rubberband/) [V] | GPL | R3 start delay 1,280 – 2,048 samples; its live shifter is "still not a low-latency effect": wrong fit |
| [SoundTouch](https://www.surina.net/soundtouch/README.html) [V] | LGPL-2.1 | WSOLA splice search; ≈ 100 ms |
| [Bungee](https://github.com/bungee-audio-stretch/bungee) [V] | MPL-2.0 | Granular phase vocoder |
| Guitarix GxDetune [V] | GPL | Bernsee / Adriaensen FFT shifter, ~2,048 samples in HQ, a low-latency mode; detune range ¼ or ⅓ semitone (sources disagree) |
| x42-autotune (zita-at1) [I] | GPL | Pitch-synchronous resampling, low latency: the PSOLA model |

### What we conclude

1. **Whammy (mono)**: delay-line shifter with a WSOLA-style splice search — per-sample ratio for perfect
   EXP sweeps, ~5 – 10 ms.
2. **Poly** is the hardest problem in golem-fx, and a plain phase vocoder does not solve it: a 512-point
   frame at 48 kHz (the ~11 ms the survey hoped for) has 94 Hz bins — a guitar's low E (82 Hz) and the
   notes of a chord fall into the same bin, and the result warbles. Two candidates, to **prototype in
   gfx-bench before choosing**:
   - **Band-split time domain**: 3 – 4 Linkwitz-Riley bands, each with its own splice-search delay-line
     shifter whose window fits the band (≈ 40 ms below 200 Hz, 20 ms to 800 Hz, 10 ms to 3 kHz, 5 ms
     above). Each band holds few partials, so splices stay clean on chords; highs (the transients) pass
     with a short delay, lows lag more — the behaviour of the poly pedals.
   - **Multi-resolution phase vocoder**: long frames for the lows, short frames for the highs, phase-gradient
     integration in each.
   The bench decides on chords (roughness, transient smear) and on latency; a Fast / Stable choice stays.
3. **Detune**: a tiny-ratio delay-line shifter with a long crossfade: inaudible splices, ~0 latency.
4. Skipped for v1: formant preservation (voice; 20+ ms), time-stretch, subharmonic
   octave (`octaver`), MIDI whammy (`whammy-dt`), autotune (`pitch-correct`).

## 2. gfx-pitch (block `pitch`)

| id | Name | Type | Range | Catalogue | Default | EXP | Role |
|---|---|---|---|---|---|---|---|
| `shift` | Shift | f | −24 – +24 st | −12 – +12 | +12 st | ✓ | Smoothed in semitones (≈ 10 ms one-pole + slew limit), converted per sample |
| `mix` | Mix | f | 0 – 100 % | same | 50 % | ✓ | Equal-power dry / wet; the dry is delayed by the reported latency so the mix stays coherent |
| `engine` | Engine | e | Whammy · Poly | new | Whammy | | Whammy: single notes, pitch-synchronous splices, 12 ms. Poly: chords, a phase vocoder with a long analysis and a short synthesis window, 6.6 / 10 ms reported |
| `tracking` | Tracking | e | Fast · Stable | new | Fast | | Poly synthesis window: Fast = 6.6 ms (tighter, a partial ~0.5 dB less on its target); Stable = 10 ms |
| `glide` | Glide | f | 0 – 1,000 ms | new | 0 ms | | Time for `shift` to reach a new value (heel-to-toe dives) |
| `shift2` | Shift 2 | f | −24 – +24 st | new | 0 st (off) | ✓ | Second voice, for harmonies / octaves |
| `voice2` | Voice 2 Level | f | −60 – 0 dB | new | −60 dB (off) | | |
| `fine` | Fine | f | −50 – +50 cents | new | 0 | | Fine tuning of voice 1 (detuned doubles, tuning to a recording) |
| `harmony` | Harmony | e | Off · Scale | new | Off | | Scale: `shift` and `shift2` become scale steps above the played note (see below); `glide` is not used |
| `key` | Key | e | C · C# · D · Eb · E · F · F# · G · Ab · A · Bb · B | new | C | | golem knows the song's key and can set it per song |
| `scale` | Scale | e | Major · Minor · Harmonic Minor · Melodic Minor · Dorian · Phrygian · Lydian · Mixolydian · Major Pentatonic · Minor Pentatonic | new | Major | | Grows at the end only |
| `bend` | Bend | b | off / on | new | off | | **Designed 2026-11-26** (§2.1). The footswitch gesture: on = bent to `shift` + `interval`, off = back to `shift`. A level, not a pulse: momentary or latching is golem's foot mode. Never stored on in a preset or a scene |
| `interval` | Bend Interval | f | −24 – +24 st | new | +12 st | | Added to `shift` (and `shift2`) while triggered; semitones, also in `harmony` Scale. The bent pitch is held to ±24 st: the travel is shortened, not cut (`shift` +12, `interval` +24 travels +12 in `rise`) |
| `rise` | Rise | f | 0 – 8,000 ms | new | 100 ms | | Time of the full travel rest → bend (the Ricochet's Shift, the PS-6's Rise Time). Under \|`interval`\| ms (Smooth: 1.125 × \|`interval`\| ms) the slew limit (1 st / ms) sets the time |
| `fall` | Fall | f | 0 – 8,000 ms | new | 250 ms | | Time of the full travel bend → rest (Return, Fall Time) |
| `curve` | Curve | e | Linear · Smooth | new | Linear | | Shape of the travel: Linear = constant rate in semitones (a treadle at constant foot speed); Smooth = eases out of rest and into the target (smoothstep). Grows at the end only |
| `rest` | At Rest | e | Mix · Dry | new | Mix | | What plays while not bending. Mix = as today: the wet stays in the mix (needed when `shift` ≠ 0: a harmony that bends). Dry = the wet fades in with the bend and out after the return (10 ms), the dry alone at unity in between: the Ricochet bypassed. The setting for every `shift` 0 use (§2.1, measured). Grows at the end only |

**Harmony (Scale).** The played note is detected on the undelayed input (so ~12 ms ahead of the wet) by YIN
(de Cheveigné & Kawahara 2002) on a ~12 kHz decimation, every ~2.7 ms; a new pitch class is taken after two
analyses agree and only within ±0.4 st of a note (a bend keeps the old note; unvoiced frames hold it; octave
errors cannot change a pitch class). `shift` becomes round(shift × notes in the scale / 12) steps: in a
7-note scale +3 / +4 = a third, +5 a fourth, +7 a fifth, +9 a sixth, +12 an octave; negative = below. The
note sits on the scale degree at or below it and the voice goes `steps` degrees away; a chromatic note keeps
its lower degree's interval. Interval changes pass the 1 st / ms slew limit (a ~1 – 4 ms bend). `fine` still
applies to voice 1. Built for single notes (either engine); chords give no stable note, and the last one holds.

Latency: reported per engine and tracking mode, set at initialisation or when they change; never when
`shift` moves. The Whammy engine exposes its detected pitch internally (bounded-cost detector): intelligent
harmony (phase 2) and a tuner-grade readout build on it.

### 2.1 Triggered bend (the Whammy Ricochet's movement) — DESIGNED 2026-11-26

Until now a bend is played by driving `shift` from the EXP pedal; `glide` (one-pole, the same time up and down)
only smooths it. The Ricochet and its kin bend **on a switch**: up to an interval when pressed, back on release,
with separate up and down times.

**State of the art.** Survey of 2026-11-26. **[V]** read in the manufacturer's manual · **[M]** manufacturer page
or a magazine review · **[F]** forum post · **[U]** not verified.

| Product | What the switch does | Times, shape | What we take |
|---|---|---|---|
| DigiTech Whammy Ricochet ([manual](https://www.adorama.com/col/productManuals/DTWHAMMYRCHT.pdf)) [V] | PITCH: 2nd, 4th, 5th, 7th, octave, 2 octaves, octave + dry; RANGE up / down. Concentric **Shift** ("the rate at which the note will be pitch shifted to the interval when the effect is enabled") and **Return** ("the rate at which it returns to its normal pitch when bypassed"). MOMENTARY switch: on = shifted while held, off = each press toggles; on with the effect on inverts it (held = back to normal). Classic / Chords engines. Its own example settings: *Killing in the Name Of*, *Dive Bombs* (Classic), *Whole Step Stabs*, *7 String* (Chords) | No figures in the manual; "instant" at minimum, about 8 s at maximum ([MusicRadar](https://www.musicradar.com/reviews/guitars/digitech-whammy-ricochet-642447)) [M]. Shape not documented [U]; a "trajectory" LED ladder shows the rate | The model: interval, two times, momentary or latching, the ramp a function of the switch's state |
| Boss PS-6 Harmonist, S-BEND ([manual](https://cdn.roland.com/assets/media/pdf/PS-6_OM.pdf)) [V] | "The S-BEND effect will be obtained while you step on the pedal" (momentary); SHIFT picks −1 … −3 oct, +1 … +4 oct, or rising / falling with vibrato (OUTPUT A rises, B falls) | RISE TIME and FALL TIME knobs, no figures | Rise and fall are the common names |
| EHX Pitch Fork / Pico Pitch Fork ([Pitch Fork manual mirror](https://vjmedia.wpi.edu/images/7/79/EHX-Pitch_Fork.pdf)) [M] | Latch or momentary; in momentary the EXP input sets the glissando rate; Pico: a SWEEP knob for the bend time | One glissando time, ~4 ms – 2 s, "returns to unity at the same rate" ([Sound On Sound](https://www.soundonsound.com/reviews/electro-harmonix-pitch-fork)) [M] | A short minimum (≈ instant) is expected |
| DigiTech Whammy DT ([Premier Guitar](https://www.premierguitar.com/gear/digitech-whammy-dt-pedal-review)) [M] | A Momentary switch for the drop tune, instant | none | Instant = our `rise` 0 (the slew limit's few ms) |
| Line 6 HX One, FLUX ([Sound On Sound](https://www.soundonsound.com/reviews/line-6-hx-one)) [M] | Press and hold: assigned parameters ramp to custom values; latching only, users ask for momentary ([Line 6 forum](https://line6.com/support/topic/70620-momentary-whammy-with-flux-switch)) [F] | Flux On and Off times and a transition curve shape | Separate times **and** a curve |
| Line 6 Helix ([forum](https://line6.com/support/topic/67394-can-helix-do-digitech-ricochet-effect/)) [F] | No ramp on a footswitch: users work around with snapshots or a Momentary bypass (an instant jump) | — | The gap is real on modellers |
| Fractal Axe-Fx III ([forum](https://forum.fractalaudio.com/threads/global-damping-of-pedal-1-2-values.204800)) [F] | A control switch (momentary or latching) on a modifier whose **attack / release** damping makes the ramp | Per modifier [U] | The same two times, generic |

Players: the Ricochet's own first example is *Killing in the Name Of* (Tom Morello played it on a Whammy's
treadle: the switch replaces the rocking foot with a fixed, repeatable movement), then dive bombs, whole-step
chord stabs and a "7-string" drop. On golem the foot is often on the EXP pedal already (wah, volume); a switch
bend frees it.

**Design.**

- **The rest stays `shift`; the bend is a separate interval, `interval`, added to it** (not "`shift` becomes the
  target"). Then `pitch` keeps its catalogue ids and meanings, a preset that never presses `bend` is the
  plugin of today (bit-identical, below), and both gestures combine: EXP on `shift` moves the rest, the switch
  jumps a fixed interval from wherever the foot is. Ricochet = `shift` 0, `interval` its interval, `mix` 100 %. A
  separate mode switch was rejected: "is bending on?" is `interval` ≠ 0 plus a press, nothing to configure.
- **One state value** φ ∈ [0, 1], per instance. While `bend` is on, φ rises at 1 / `rise` per ms; while off
  it falls at 1 / `fall` per ms; 0 ms = the step (then the slew limit, below). `rise` / `fall` are thus the
  times of the **full** travel at a constant rate: a release half-way returns in `fall` / 2, a press during the
  fall turns back from where it is. No jump in any order of presses.
- **Offset**, per voice: o = e × c(φ), with e = clamp(b + `interval`, −24, 24) − b the travel actually available
  from the rest b (review: the draft clamped the sum instead, which ended the rise early and left a dead time
  of up to the whole `fall` before the pitch moved on release — `shift` +12, `interval` +24 reached +24 at φ = 0.5).
  c = φ (Linear) or 3φ² − 2φ³ (Smooth): linear in semitones, i.e. a constant rate in log frequency, as a treadle
  at constant foot speed. Smooth changes only the shape; the times hold while `rise` / `fall` ≥ 1.125 × |e| ms
  (its steepest point is 1.5 × the mean rate; the slew limit clips it below 1.5 × |e| ms and it still lands on
  time down to 1.125 ×). Smooth eases the two ends of a **full** travel: a release mid-rise turns back at once
  from the curve's steep middle (no jump in pitch, a corner in its rate). φ is an f64, advanced per sample.
- **Where it enters the chain** (per sample, `process_block`, checked against `lib.rs` at review): the rest b is
  `sh1` (the one-pole's output, Off) or `harmony_interval(…shift)` (Scale); `t1 = b + o + fine·0.01`; voice 2
  the same with `shift2` and no `fine`; then the existing 1 st / ms slew limit on `st_out`, then the ratio.
  b + o stays inside ±24 by construction, so no clamp is added to the path.
  - **After `glide`'s one-pole**: `glide` keeps its meaning (the time `shift` takes to a new value, EXP dives)
    and does not double-smooth the bend; `rise` / `fall` are the bend's times, exactly.
  - **Before the slew limit**: a 0 ms rise or fall becomes a |Δ| ms bend (12 ms for an octave, 24 ms for two),
    the "instant" of the Whammy DT and the Ricochet at minimum, never a one-sample step. Below |`interval`| ms the
    slew sets the time; stated in the table.
  - **Both voices**: the bend moves the whole wet (a two-voice stack bends as one); the dry stays, through
    `mix`. A voice that stays put while the other bends can be appended later (`bend_voices`).
  - **Harmony Scale**: the bend is chromatic semitones added to the in-key interval (the Ricochet's intervals
    are chromatic), so +2 from a diatonic third is a whole step above it whatever the scale.
  - **Poly**: the ratio is taken per frame (hop 3.3 ms Fast, 5 ms Stable), so the bend follows in hop steps,
    as an EXP sweep already does.
  - The ±24 st limit shortens the travel (shift −12, bend −24 travels −12, in the full `rise`).
  - `interval`, `curve`, `rise` or `fall` moved mid-travel or while held: φ is untouched, so the target may step
    (a new `interval` at φ = 1) and the slew limit turns the step into a 1 st / ms bend. No click, no restart.
- **Bit-identical when unused, by construction** (review): while φ = 0 the code takes **today's expressions
  untouched** — `(sh1 + fine * 0.01) as f64` in f32 then f64 (Off), `harmony_interval(…) as f64 +
  (fine * 0.01) as f64` (Scale) — behind one branch `if phi > 0.0`, rather than adding a zero and arguing it
  away. (Adding it would also hold: e × 0.0 = ±0.0, `harmony_interval` stays inside ±24 for a `shift` in range,
  2 × the scale's notes at most; but a branch needs no argument and costs nothing.) `rest` Mix leaves the mix
  angle's expression as it is. A `bend` change reaches `set()` through the wrapper's settings compare, as
  any parameter does today; read at review: `OnePole::set_target` only stores the target, `set_time` recomputes
  the same coefficient, `mix.set_target` runs only when `mix` moved. Nothing restarts.
- **`reset()`** settles φ to the current `bend` (0 or 1), the wet gate (below) to open or shut accordingly,
  and includes o in `st_out` — again only when φ > 0, so `st_out`'s reset value is today's otherwise. A session
  saved with `bend` on reloads bent, without a ramp.
- **Latency**: unchanged and still reported only on an engine / tracking change (`latency_of`); the bend never
  moves it. An engine change during a bend dips the output as today; φ keeps running under the dip.
- **The gesture is a level, and momentary / latching is golem's.** `bend` is a VST3 bool (step count 1):
  on = bend, off = return; it says nothing about press or release. golem's footswitch writes 1 on press and
  0 on release (momentary) or flips it on each press (latching), as for gfx-grains `freeze`, gfx-ducker
  `force`, gfx-killswitch `kill`, gfx-sustainer `feed` (levels; gfx-swell's `fire` is the one edge-triggered
  gesture). **golem has no momentary foot action yet** as far as this repository records it (gfx-ducker §6,
  item 7: "press = on, release = off, not a toggle; golem's `foot_mode` knows `transport`, `sample` and `drums`
  gestures today"; the draft cited a "linux-pass item 10" that no document here contains — golem-rack to
  confirm), so only the latching form exists until golem adds it, and the Ricochet's main gesture (hold)
  waits for it. The block's footswitch stays the bypass, and a block preset can put it on `bend`
  (gfx-sustainer's proposal). The Ricochet's inverted momentary (held = back to rest) is golem's too, or
  `shift` / `interval` set the other way round. **No plugin-side latch mode** (settled at review): every held
  gesture of the range is a level and golem's foot mode decides; a `trig_mode` would make the stored value
  differ from the state and duplicate golem's own setting.
- **Host timing**: `SAMPLE_ACCURATE_AUTOMATION` stays false, so a toggle lands at the next buffer's start
  (≤ 2.7 ms at 128 samples, 48 kHz); the ramp then runs at sample rate. The Whammy's 12 ms latency delays the
  audible start like everything else in the wet. `bend` is not EXP-marked and not smoothed (the ramp is its
  smoothing); REAPER reads 0 … 0.5 as off, golem writes 0 / 1 (contract).
- **The wet at rest, and `rest`** (rewritten at review, from measurements on the v0.1.0 code). The draft said:
  at `shift` 0 the wet is the delayed input, so `mix` 50 % gives a steady +3 dB at rest. That holds **only
  until the first bend**. Measured (48 kHz, six-harmonic tones 82 – 660 Hz and chords, `shift` 0 → a bend for
  1 s → 0, read 1 – 2 s after the return):

  | | Before any bend | After a bend has returned |
  |---|---|---|
  | Wet against the delayed input, `mix` 100 % (null) | Whammy −45 to −62 dB, Poly −98 to −102 dB | no null at all, +1 to +5 dB, both engines |
  | Level, `mix` 100 % | 0.0 dB | Whammy 0.0 dB; **Poly −2.0 to −2.9 dB**, and it stays there |
  | Level, `mix` 50 % | +3.0 dB | **−4.8 to +2.9 dB**, another value after each bend, both engines |

  The Whammy's two heads come back period-aligned to each other, not to the centre (each splice aligns on the
  other head, nothing re-centres them at ratio 1); the vocoder's phases keep the advance they accumulated. So
  after a bend the rest is a comb of the dry against a wet at an arbitrary phase: a different tone and level
  after every press. This is v0.1.0's behaviour when an EXP sweep comes back to 0, but a triggered bend returns
  to rest after every gesture, so it cannot be left as a footnote.
  - **`rest` Dry** (new, appended; default Mix = today): a wet gate w, target 1 while `bend` is on, or
    φ > 0, or the offset's own slew-limited copy (1 st / ms, voice 1) has not come back to 0; else 0. A 10 ms
    raised-cosine ramp (a `LinearRamp`, as the engine dip's). The mix angle becomes `mix` × π / 2 × w: at rest
    the output is the dry alone at unity, delayed by the latency (0 dB, no comb, chords intact through the
    Whammy engine too); bent, the equal-power mix of today. At `mix` 100 % it crossfades dry → voice over the
    first 10 ms of the bend. The gate reads the offset, not `st_out`, so a harmony note change does not open it.
  - `rest` Mix stays for a rest that is itself shifted (`shift` ≠ 0: a harmony that bends, EXP + switch),
    where the wet is meant to sound all the time and is not coherent with the dry anyway.
  - The pure Ricochet is `shift` 0, `rest` Dry, `mix` 100 %; its "octave + dry" is the same at `mix` 50 %.
  - Not decided here: making the engines themselves return to the delayed input at ratio 1 (heads re-centred,
    phases reset). It would cure Mix at `shift` 0 and the EXP heel as well, but it touches gfx-dsp and the
    bit-identity of existing renders; golem-fx's to measure at coding.
- **Cost**: a branch per sample at rest; bending, a few multiply-adds, a second slew and the gate. No allocation.

## 3. gfx-pitch-classic (block `pitch-shift-js`)

The two-head delay-line shifter, superpitch-compatible: `window` and `overlap` used literally.

| id | Name | Type | Range | Catalogue | Default | Role |
|---|---|---|---|---|---|---|
| `shift` | Shift | f | −24 – +24 st | −12 – +12 | 0 st | |
| `window` | Window | f | 10 – 100 ms | same | 50 ms | Time between splices |
| `overlap` | Overlap | f | 1 – 50 ms | same | 20 ms | Crossfade length, clamped to window / 2 |
| `wet` | Wet | f | −120 – +6 dB | same | 0 dB | Independent gain |
| `dry` | Dry | f | −120 – +6 dB | same | −120 dB | Independent gain |
| `search` | Splice Search | b | off / on | new | off | WSOLA-style splice point (H3000 "deglitch"); on adds a fixed lookahead |

Reported latency 0 with `search` off, as the JS (the average delay ≈ window / 2 is inherent and documented).

## 4. gfx-detune (block `detune`)

| id | Name | Type | Range | Catalogue | Default | Role |
|---|---|---|---|---|---|---|
| `detune` | Detune | f | 0 – 1 | same | 0.3 | 0 … ± 25 cents (to check against GxDetune's range in its source) |
| `wet` | Wet | f | 0 – 1 | same | 0.5 | Linear |
| `dry` | Dry | f | 0 – 1 | same | 0.7 | Linear |
| `delay` | Doubling Delay | f | 0 – 30 ms | new | 0 ms | Offset of the detuned voices (doubler) |
| `voices` | Voices | e | 1 · 2 | new | 2 | 2 = one up, one down (± detune), panned when stereo |

Two tiny-ratio delay-line shifters with a ~50 ms window and long crossfades; reported latency 0.

## 5. Applications

| Use | Plugin · settings | Feature that matters |
|---|---|---|
| Whammy dive / octave-up squeal | gfx-pitch, Whammy, `mix` 100 %, EXP on `shift` 0 → +12 (or 0 → −12, −24) | Per-sample ratio, smoothed in semitones: no steps, no splice clicks |
| Drop tuning without retuning (virtual capo) | gfx-pitch, Poly, `shift` −1 … −5 st, `mix` 100 % | Polyphonic quality on chords; ~10 ms |
| 12-string | gfx-pitch, Poly, `shift` +12, `mix` 35 %, `fine` +5 cents | Octave double with a slight detune |
| Octave-down thickening for riffs | gfx-pitch, Whammy, `shift` −12, `mix` 40 % | (For a synth-like sub, use gfx-octaver) |
| Fixed harmony (thirds, fifths) | gfx-pitch, Whammy, `shift` +4 or +7, `mix` 45 % | |
| In-key twin leads (Thin Lizzy, Maiden) | gfx-pitch, `harmony` Scale, `key` / `scale` of the song, `shift` +4 (a third above), `mix` 50 % | Add `shift2` +7, `voice2` −4 dB for three voices |
| Two-voice harmony | `shift` +4, `shift2` +7, `voice2` −3 dB | |
| Stereo widening, no wobble | gfx-detune, `detune` 0.35, `voices` 2, `wet` = `dry` | ± cents, zero latency |
| Vocal / guitar doubling | gfx-detune, `delay` 18 ms, `detune` 0.25 | |
| Old "superpitch" presets | gfx-pitch-classic | Window / overlap as before; `search` on removes the warble |
| Squeal on the switch (the Ricochet's *Killing in the Name Of*) | gfx-pitch, Whammy, `mix` 100 %, `shift` 0, `rest` Dry, `interval` +24 (two octaves, as Morello's Whammy: the reference of our own preset `two-octaves-up-solo`; +12 for the tamer octave), `rise` 0, `fall` 0; `bend` on a momentary footswitch | 24 ms up and down by the slew limit (12 ms for an octave): rhythmic squeals with the foot free. Needs golem's momentary foot action |
| Dive bomb on a switch | Whammy, `mix` 100 %, `shift` 0, `rest` Dry, `interval` −24, `rise` 1,500 ms, `fall` 150 ms, `curve` Smooth | Separate times: a slow dive, a fast snap back |
| Whole-step chord stabs | Poly, `mix` 100 %, `shift` 0, `rest` Dry, `interval` −2 (or +2), `rise` 20 ms, `fall` 40 ms | Poly on chords; short, repeatable bends; the chord at rest is the dry, at its level |
| "7 string": a low note on demand | Poly, `mix` 100 %, `shift` 0, `rest` Dry, `interval` −5, `rise` 0, `fall` 0, latching | The drop-tune switch of the Whammy DT, without retuning. Without `rest` Dry the guitar comes back 2 – 3 dB down after the first use (Poly, measured) |
| Octave lift into a chorus (pad swell) | Poly, `mix` 50 %, `shift` 0, `rest` Dry, `interval` +12, `rise` 3,000 ms, `fall` 2,000 ms, `curve` Smooth | Long rise; the dry stays under it, 0 dB at both ends |
| Harmony that bends | `harmony` Scale, `shift` +4 (a third above), `mix` 50 %, `rest` Mix, `interval` +1 or +2, `rise` 150 ms | Chromatic bend off the in-key voice, a twin-lead "bend into the note" |
| Bend from a capo (EXP + switch) | Poly, `rest` Mix, EXP on `shift` (rest −5 … 0), `interval` +12, `rise` 80 ms | The rest follows the foot, the switch jumps an octave from it |

## 6. Real-time and bench

- Every buffer and FFT plan is allocated in `initialize()` (100 ms at 192 kHz); `reset()` only zeroes.
  Nothing allocates or blocks on enable — the reason ReaPitch was quarantined.
- Pitch detection, where used, is a bounded-cost YIN / AMDF over a fixed window: no cost spikes.

| Check | Pass |
|---|---|
| Enable | `check-block-stall` green on both engines (the ReaPitch failure) |
| Latency | reported = measured, for every engine and tracking mode |
| Whammy sweep | `shift` 0 → +12 st in 300 ms on a held note: continuous pitch, no splice click |
| Poly | ten chords at −12 … +12 st (`poly_metric`), a decaying chord sequence (purity, previous-chord residue), both candidates measured; the vocoder shipped 2026-11-13 |
| Pitch accuracy | ± 1 cent on a sine at every integer shift |
| Detune | splices inaudible: HF burst bound on a held note |
| Listening | lead-clean.wav, clean.wav (chords), lead.wav; against REAPER's superpitch and GxDetune |
| Bend unused (§2.1) | `bend` never set: every gfx-pitch preset (`presets/gfx-pitch.json`, golem's `block-presets/pitch/`) renders **bit-identical** to v0.1.0 on lead.wav and clean.wav (max \|Δ\| = 0), both engines, harmony Off and Scale, including a `shift` sweep and a `reset()` mid-render. The reference is taken **before `lib.rs` is touched**: a first commit adds the test with the v0.1.0 output hashes (or the renders), the second adds the bend |
| Bend timing (§2.1) | Unit test on the applied interval (`st_out`, read through a test accessor: the audio follows it by the latency), `bend` set at a known sample, `interval` +12, Linear: 90 % of the travel at 0.9 × `rise` ± 1 ms, half-way at `rise` / 2 ± 0.1 st, back at rest within `fall` ± 1 ms, for `rise` / `fall` ∈ {0, 50, 300, 2,000} ms (0 ms: \|`interval`\| ms ± 1 by the slew); Smooth: half-way at `rise` / 2, 90 % at 0.804 × `rise`, the end on time; a release half-way returns in `fall` / 2 ± 1 ms; `shift` +12, `interval` +24: +24 reached at `rise`, not before, and the pitch leaves +24 within 1 ms of the release; no interval step larger than the slew limit's per sample; the end pitch in tune within 0.2 cents (as today) |
| Rest (§2.1) | `rest` Dry, `shift` 0, after ten bends: the output at rest nulls against the input delayed by the latency (≤ −90 dB), both engines, `mix` 50 % and 100 %; opening and closing the gate adds no HF burst beyond the sweep bound. `rest` Mix: recorded, not bounded (the comb of §2.1) |
| Bend clicks (§2.1) | Whammy, `interval` +12, `rise` 0, on a held note: HF burst within the Whammy-sweep bound; Poly, a chord, `interval` −2 in 20 ms: within the Poly sweep bound. gfx-bench's `param_steps` keeps `bend` in (it is a sweep, as `shift`'s own step is); a miss there is a finding, not a reason to skip it |
| Bend in hosts (§2.1) | REAPER and Ardour: `bend` written 0 / 1 lands off / on; the ramp starts within one buffer of the write; `check-engine-blocks` toggles it |

## 7. Visual changes (triggered bend, 2026-11-26)

Only what changes; everything else stays as drawn (`visuals/js/golem-pedals.js`, `visuals/data/faces.json`).

- The gfx-pitch face gains one toggle, `bend` ("Bend"), drawn as a `.gp-toggle` as gfx-ducker's `force`:
  `faces.json` → `"gfx-pitch": { … "toggles": ["trigger"] }`. Knobs (`shift`, `shift2`, `mix`), the `engine`
  selector, finish, hue and the `stairs` motif are unchanged.
- `interval`, `rise`, `fall`, `curve`, `rest` are not on the face: gfx-ui's expert panel shows them in a new group "Bend"
  (`visuals/data/descriptors.json`); `rise` / `fall` take `display: log` in the descriptor (the cubic law from zero), so 0 – 500 ms is not
  squeezed into the first 6 % of a linear 0 – 8,000 ms travel.
- In the editor the toggle is a click (latching); the momentary gesture is the footswitch's. The gfx-pitch
  bundle (`gfx-ui/bundles/gfx-pitch.gfxui`) is written again from the face.
- gfx-pitch-classic and gfx-detune: no change.

## Open questions (triggered bend) — after the review of 2026-11-26

Settled at review (reasons in the Review section below):

- **Plugin-side latch mode** (was 2): no. `bend` is a level; momentary or latching is golem's foot mode.
- **Both voices bend** (was 3): yes in this version. The drone under a bending voice is the dry (`mix`); a
  `bend_voices` enum (Both · Voice 1) can be appended later without touching a preset.
- **The level at rest** (was 4): not "+3 dB, accept it" — the rest is a comb after the first bend (§2.1,
  measured). `rest` (Mix · Dry) is added; Dry is the setting of every `shift` 0 use. Accepted by Raph,
  2026-11-26 (`rest`, Mix · Dry, default Mix).
- **Curve set** (was 6): Linear · Smooth ship; a third shape is appended only if listening asks.

Still open:

1. **Id of the gesture.** In this range `trigger` is already an **enum** naming what starts an effect, in two
   coded pedals (gfx-killswitch: 3 options; gfx-sustainer: Auto · Switch), and the held gestures are bools named
   by their action: `kill`, `feed`, `force`, `freeze`, `fire`. Nothing collides technically (VST3 ids are per
   plugin, catalogue ids per block, gfx-bench's skip lists are per pedal). The review's recommendation: the
   gesture `bend` (its display name is already "Bend") and the interval `interval`; the roadmap says `trigger`.
   **Decided by Raph, 2026-11-26: the bool is `bend` and the interval is `interval`** (−24 … +24 st, default
   +12), replacing the draft's `trigger` (bool) / `bend` (interval) throughout this document. Ids are
   permanent once shipped.
2. **Ranges and defaults**: 0 – 8,000 ms after the Ricochet's ~8 s (one review); 100 / 250 ms defaults are ours.
   The Ricochet's real curve is not documented: an owner measuring it (a sine, the switch, a pitch track) would
   tell whether Linear or Smooth is the closer default. **Decided by Raph, 2026-11-26: accepted** — `rise` /
   `fall` 100 / 250 ms, 8 s maximum, Linear by default; a measured Ricochet can still move the default curve
   before shipping.
3. **golem's side** (golem-rack): the momentary foot action (press = 1, release = 0), without which only the
   latching uses exist; its timing on REAPER, where OSC values land in batches on the main thread (contract,
   Expression pedal): a press and release inside one batch must not be merged into nothing, and a rhythmic
   squeal needs the press to land within a few ms. Catalogue: `pitch` gains `bend` (bool), `interval`, `rise`,
   `fall`, `curve`, `rest`, none EXP-marked; `bend` never stored on in a preset or scene; whether a block
   preset puts the block's footswitch on `bend`; a footswitch LED could follow `bend` (golem knows it) or
   a read-only φ meter if the LED should show the travel, as the Ricochet's trajectory ladder.
4. **The engines at ratio 1 after a sweep** (golem-fx): re-centring the Whammy's heads and resetting the
   vocoder's phases at unison would make `rest` Mix clean at `shift` 0 and fix Poly's −2 to −2.9 dB after a
   return; to try in gfx-dsp against the bit-identity test, apart from this item. **Coded 2026-11-26** (gfx-dsp
   `CenteredShifter::set_recentre`, `pv_shift` at ratio 1; test `the_wet_returns_to_the_delayed_input_after_a_bend`):
   after a bend through `shift` (+12, −12, +2, +24, −5 on tones of 82 – 660 Hz, E5 and Am) the wet nulls against
   the delayed input at −144 dB (Whammy) / −134 dB (Poly), level 0.000 dB; it gets there within 1.6 s (Whammy: the
   heads glide back at ≤ 7 cents) / 270 ms (Poly). Before: +4.9 dB at worst (no null), Poly down to −4.2 dB.

## Review (Fable 5.2, 2026-11-26) — triggered bend

Review of §2.1, its parameters, applications, tests and visual notes, written the same day by Claude Opus 5.5
for roadmap 0.8 item 4. Read against `gfx-pitch/src/lib.rs` and `plugin.rs` (v0.1.0), gfx-dsp's `pitch.rs`,
`pv_shift.rs`, `detect.rs` and `smooth.rs`, the golem contract, and the other pedals' gestures. Every finding
is applied above.

### Verdict

**Sound, codable after one id decision** (taken by Raph, 2026-11-26: `bend` / `interval`). The structure is right: a separate interval added to `shift`, one
state φ, the ramp entering after `glide`'s one-pole and before the slew limit, the gesture as a level. One
claim was false and hid a real defect of the rest state (the wet at rest); one formula had a dead time; the
rest are precisions. No Rust was changed; nothing here is coded.

### Findings

| # | Finding | Severity | What was done |
|---|---|---|---|
| 1 | **"At `shift` 0 the wet is the delayed input, +3 dB at `mix` 50 %" is true only before the first bend.** Measured on v0.1.0: after a bend returns, the wet no longer nulls against the delayed input (from −45 … −102 dB to +1 … +5 dB), the `mix` 50 % rest level lands anywhere from −4.8 to +2.9 dB, different after each bend, and Poly's wet stays 2.0 – 2.9 dB down at `mix` 100 % | Error (sound) | §2.1 rewritten with the table; **`rest` (Mix · Dry) appended**, default Mix; the five `shift` 0 applications set `rest` Dry; a bench line |
| 2 | **Clamping the sum `shift` + o** ends the rise early and leaves a dead time on release (`shift` +12, `interval` +24: +24 at φ = 0.5, then up to `fall` / 2 with no movement after the release) | Error (behaviour) | The travel's end is clamped instead: e = clamp(b + `interval`) − b, o = e × c(φ); the times hold; no clamp left in the path; a test case |
| 3 | **Bit-identity argued, not built.** The argument is correct as far as it goes (checked: e × 0.0 = ±0.0, `harmony_interval` ≤ 2 octaves for a `shift` in range, the one-pole stays between its values), but it rests on the coder keeping f32 / f64 order in two differently typed branches, and "bit-identical to v0.1.0" had no reference to compare with | Risk | A branch `if phi > 0.0` keeps today's expressions verbatim, in `process_block` and `reset()`; the test takes its v0.1.0 hashes in a commit before the change, and covers a sweep and a mid-render reset |
| 4 | **Timing test wrong for Smooth and vague on what is measured**: "90 % at 0.9 × `rise`" is Linear only (smoothstep is at 97 % there; its 90 % is at 0.804); the audio lags the applied interval by the latency | Test | Test split per curve, read on `st_out` |
| 5 | **Smooth against the slew limit**: its peak rate is 1.5 × the mean, so "the times hold" needs `rise` ≥ 1.125 × \|travel\| ms, not \|`interval`\| ms; and it eases full travels only (a mid-travel release turns at the steep point) | Precision | Stated in the table and in §2.1 |
| 6 | **Application "Killing in the Name Of" at `interval` +12**: the reference this repository already holds (preset `two-octaves-up-solo`, Rolling Stone 2023) is two octaves up | Error (reference) | `interval` +24, 24 ms by the slew; +12 kept as the variant. The Ricochet manual's own setting for that example was not re-read [U] |
| 7 | **`trigger` clashes in vocabulary with two coded pedals, not one** (gfx-killswitch too), and the range's gestures are verbs (`kill`, `feed`, `force`, `freeze`, `fire`) | Naming | Open question 1, with a recommendation; ids left as the roadmap names them. **Decided by Raph, 2026-11-26:** `bend` (bool) and `interval`, applied throughout |
| 8 | **"linux-pass item 10"** is cited as golem's open item for the momentary action; no document in this repository contains it. `gfx-swell`'s `fire` was listed as a level: it is an edge | Reference | Reworded on gfx-ducker §6 item 7, which does say it; `kill` listed, `fire` set apart |
| 9 | Unstated: what an `interval` / `curve` / `rise` change does mid-travel; that the bend never moves the latency; `reset()` with `bend` stored on; Poly after a return | Gaps | Stated in §2.1 |

### Verified, no change needed

- **Where the ramp enters.** `process_block` takes `sh1 = self.shift.next()` (the one-pole, ≥ 10 ms, `glide`),
  builds `t1` / `t2`, then `st_out += (t − st_out).clamp(−lim, lim)` with `lim = 1000 / sr` (1 st / ms), then
  `exp2`. Adding o to `t1` / `t2` is after the one-pole and before the slew, as designed: `rise` / `fall` are not
  smoothed twice, and a 0 ms travel becomes \|travel\| ms. In Scale the one-pole is bypassed already
  (`harmony_interval` reads `shift` raw); the offset adds to its integer result in f64.
- **`set()` on a `bend` change restarts nothing** (read: `OnePole::set_target`, `set_time`, the guarded
  `mix.set_target`). `SAMPLE_ACCURATE_AUTOMATION` is false: one settings compare per buffer, so the toggle lands
  at the buffer's start (2.7 ms at 128 samples).
- **Latency and engines.** `latency_of` depends on engine and tracking only; the dry line holds 30 ms. Poly takes
  the ratio at the next frame (`set_ratio`: "taken at the next frame"; hop = m, 3.3 / 5 ms). Both shifters clamp
  the ratio to 0.25 – 4, so ±24 st is the real limit (`fine` on top of ±24 is clipped there, as today).
- **Contract.** Ids and the two enums are appended; ranges linear (`rise` / `fall` 0 – 8,000 ms: knob feel is
  the UI's, and `display: log` with its cubic law from zero exists in gfx-ui); the bool is written 0 / 1; no
  default depends on another parameter; `rise` (ms) and `curve` (a shape) mean what they mean in gfx-swell,
  gfx-sustainer and gfx-vibrato. `ParamInfo::boolean` and the faces' `toggles` exist.
- **Curves.** Linear in semitones is the constant-rate treadle; smoothstep has zero rate at both ends and
  1.5 × in the middle; both are monotone, so no overshoot past the interval.

How the measurements were made: a scratch program outside the repository linking gfx-pitch v0.1.0 as it stands
(no plugin feature), 48 kHz, mono, 256-sample blocks; `shift` 0 for 1 s, the bend for 1 s through `shift`
itself, 0 again for 2 s; RMS over the last second against the input, and the null against the input delayed by
the reported latency. Tones of six harmonics at 82, 110, 196, 330 and 660 Hz, an E5 power chord and an open
A minor; bends +12, −12, +2, +24, −5. The `rest` gate itself is designed, not measured.

### Open, with an owner

| Point | Owner |
|---|---|
| Gesture id: `trigger` (roadmap) or `bend` + `interval` (review's recommendation), before coding. **Decided by Raph, 2026-11-26:** `bend` (bool) and `interval` (−24 … +24 st, default +12) | Decided; golem-rack for the catalogue |
| `rest` as a sixth parameter, its name and options; confirm by ear that Dry is the Ricochet's rest. **Decided by Raph, 2026-11-26:** `rest` (Mix · Dry, default Mix) accepted; Dry checked by ear at coding | golem-fx at coding (the ear check) |
| `rise` / `fall` defaults (100 / 250 ms), the 8 s maximum, Linear as default; a measured Ricochet if one can be borrowed. **Decided by Raph, 2026-11-26:** accepted as written | Decided |
| The bit-identity reference taken before the change; the `rest` gate's fade (10 ms) and its click bound; `bend` in `param_steps` | golem-fx at coding |
| Engines at ratio 1 after a sweep (heads re-centred, vocoder phases reset): Poly's −2 to −2.9 dB after a return, the comb in `rest` Mix | golem-fx at coding (gfx-dsp), apart from this item |
| Momentary foot action and its timing on REAPER; whether "linux-pass item 10" is this item; catalogue entries; `bend` never stored on; LED | golem-rack |
