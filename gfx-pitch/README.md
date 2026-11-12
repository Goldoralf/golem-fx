# gfx-pitch

> **Status: PROPOSED** — design, no code yet.

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
2. **Poly**: phase vocoder with phase-gradient integration, aiming at Helix's ~10 ms; a Fast / Stable choice.
3. **Detune**: a tiny-ratio delay-line shifter with a long crossfade: inaudible splices, ~0 latency.
4. Skipped for v1: formant preservation (voice; 20+ ms), scale-aware harmony, time-stretch, subharmonic
   octave (`octaver`), MIDI whammy (`whammy-dt`), autotune (`pitch-correct`).

## 2. gfx-pitch (block `pitch`)

| id | Name | Type | Range | Catalogue | Default | EXP | Role |
|---|---|---|---|---|---|---|---|
| `shift` | Shift | f | −24 – +24 st | −12 – +12 | +12 st | ✓ | Smoothed in semitones (≈ 10 ms one-pole + slew limit), converted per sample |
| `mix` | Mix | f | 0 – 100 % | same | 50 % | ✓ | Equal-power dry / wet; the dry is delayed by the reported latency so the mix stays coherent |
| `engine` | Engine | e | Whammy · Poly | new | Whammy | | Whammy: mono, splice search, ~5 – 10 ms. Poly: chords, ~10 – 25 ms |
| `tracking` | Tracking | e | Fast · Stable | new | Fast | | Poly frame size: Fast ≈ 11 – 15 ms, Stable ≈ 21 – 27 ms (smoother on sustained chords) |
| `glide` | Glide | f | 0 – 1,000 ms | new | 0 ms | | Time for `shift` to reach a new value (heel-to-toe dives) |
| `shift2` | Shift 2 | f | −24 – +24 st | new | 0 st (off) | ✓ | Second voice, for harmonies / octaves |
| `voice2` | Voice 2 Level | f | −60 – 0 dB | new | −60 dB (off) | | |

Latency: reported per engine and tracking mode, set at initialisation or when they change; never when
`shift` moves.

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

## 5. Real-time and bench

- Every buffer and FFT plan is allocated in `initialize()` (100 ms at 192 kHz); `reset()` only zeroes.
  Nothing allocates or blocks on enable — the reason ReaPitch was quarantined.
- Pitch detection, where used, is a bounded-cost YIN / AMDF over a fixed window: no cost spikes.

| Check | Pass |
|---|---|
| Enable | `check-block-stall` green on both engines (the ReaPitch failure) |
| Latency | reported = measured, for every engine and tracking mode |
| Whammy sweep | `shift` 0 → +12 st in 300 ms on a held note: continuous pitch, no splice click |
| Poly | an open E chord at +5 st: no beating beyond a bound, transients kept |
| Pitch accuracy | ± 1 cent on a sine at every integer shift |
| Detune | splices inaudible: HF burst bound on a held note |
| Listening | lead-clean.wav, clean.wav (chords), lead.wav; against REAPER's superpitch and GxDetune |
