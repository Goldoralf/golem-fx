# gfx-pitch

> **Status: CODED** (v0.1.0, 2026-11-12) — three VST3s on gfx-dsp's centred splice-search shifter (fixed mean delay, per-sample ratio, correlation splices with sub-sample refinement): **gfx-pitch** (Whammy, 12 ms reported; Poly, 14 ms Fast / 22 ms Stable reported; second voice, fine, glide, slew limit; dry delayed by the latency), **gfx-pitch-classic** (superpitch's two-head splicer, window / overlap literal, optional splice search, latency 0), **gfx-detune** (two ±25-cent voices, 3 ms centre, doubling delay by crossfade, latency 0); all allocate in `prepare` only. Tests: Whammy in tune within 0.2 cents at every integer shift from −24 to +24 at 82 – 880 Hz (measured ≤ 0.07; until 2026-11-13 the second head spliced against the first head's already-advanced position, a drift growing with the shift: +1.5 cents at +12, +6.8 at +24), detune exact to 0.001 cent. A grain that a sweep would carry out of the line (planned at one ratio, the ratio moving meanwhile) has the rest of its fade compressed at the same window phase, decided for the head pair so they stay staggered: the worst parameter-step burst went from −55 to −82 dB, poly octaves each note of a chord, sweeps without a splice click, latency = the engine's centre; bench green. Choices made in code: Poly is the band-split time-domain candidate with **nine** half-octave bands (two notes in one band defeat the splice search) and **one** centre delay for all bands (different delays combed where bands overlap) — so its latency is above the 10 ms target. Fixed 2026-11-13: the band tree had no phase compensation (each band now also passes the all-passes of the crossovers above it: the bands sum flat), and bands parked at random offsets after a start from silence; every band now splices at the same moments from the same positions (shared grain timing, silence → nominal start, ties → nearest the nominal), so at unison a chord comes out whole (two notes of the test chord were 14 – 17 dB down). Still open — the band-split's limit: once shifted, neighbouring bands splice on their own dominant periods, so a partial shared by two bands beats; over ten guitar chords a partial keeps −11 to −20 dB of its level on its target on average (the rest is spread in grain-rate sidebands), and which notes suffer moves with any detail. That is the vocoder candidate's job. Poly costs ~950 ns / sample (all-passes included); the multi-resolution vocoder was not prototyped. **Phase 2 (harmony) coded 2026-11-12**: `harmony` / `key` / `scale` appended; YIN detector in gfx-dsp (`detect.rs`, ~12 kHz analysis every ~2.7 ms, 75 Hz – 1.5 kHz, harmonic-lock check) with a confirming note tracker; on the lead DIs the tracked pitch class agrees with an offline reference on 97 % (lead) / 97 % (lead-clean) of voiced frames, the rest being onset timing, and the harmony voice stays in the scale 95.5 % of the time (a fixed +4: 59 – 61 %). Detector cost ~60 – 80 ns / sample at 48 kHz, only while `harmony` is on.

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
| `engine` | Engine | e | Whammy · Poly | new | Whammy | | Whammy: single notes, pitch-synchronous splices, ~5 – 10 ms. Poly: chords, reported latency = its highest band's delay (~10 ms target); lower bands lag more, unreported, as in the poly pedals |
| `tracking` | Tracking | e | Fast · Stable | new | Fast | | Poly windows: Fast = shorter (tighter, a little rougher on sustained chords); Stable = longer |
| `glide` | Glide | f | 0 – 1,000 ms | new | 0 ms | | Time for `shift` to reach a new value (heel-to-toe dives) |
| `shift2` | Shift 2 | f | −24 – +24 st | new | 0 st (off) | ✓ | Second voice, for harmonies / octaves |
| `voice2` | Voice 2 Level | f | −60 – 0 dB | new | −60 dB (off) | | |
| `fine` | Fine | f | −50 – +50 cents | new | 0 | | Fine tuning of voice 1 (detuned doubles, tuning to a recording) |
| `harmony` | Harmony | e | Off · Scale | new | Off | | Scale: `shift` and `shift2` become scale steps above the played note (see below); `glide` is not used |
| `key` | Key | e | C · C# · D · Eb · E · F · F# · G · Ab · A · Bb · B | new | C | | golem knows the song's key and can set it per song |
| `scale` | Scale | e | Major · Minor · Harmonic Minor · Melodic Minor · Dorian · Phrygian · Lydian · Mixolydian · Major Pentatonic · Minor Pentatonic | new | Major | | Grows at the end only |

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

## 6. Real-time and bench

- Every buffer and FFT plan is allocated in `initialize()` (100 ms at 192 kHz); `reset()` only zeroes.
  Nothing allocates or blocks on enable — the reason ReaPitch was quarantined.
- Pitch detection, where used, is a bounded-cost YIN / AMDF over a fixed window: no cost spikes.

| Check | Pass |
|---|---|
| Enable | `check-block-stall` green on both engines (the ReaPitch failure) |
| Latency | reported = measured, for every engine and tracking mode |
| Whammy sweep | `shift` 0 → +12 st in 300 ms on a held note: continuous pitch, no splice click |
| Poly | an open E chord and a low-E power chord at −2, +5, +12 st: roughness and transient smear measured for both candidate engines; the better one ships |
| Pitch accuracy | ± 1 cent on a sine at every integer shift |
| Detune | splices inaudible: HF burst bound on a held note |
| Listening | lead-clean.wav, clean.wav (chords), lead.wav; against REAPER's superpitch and GxDetune |
