# gfx-polysynth

> **Status: CODED** (v0.1.0, 2026-11-15). Designed by Claude Opus 5.5, reviewed by Claude Fable 5.2 on
> 2026-11-14 (Review section), coded as reviewed with the deviations listed in "As coded". **v1 = Spectral +
> Mono** (`engine` declares those two only; Poly is held back). The Spectral core runs two explicit
> gfx-dsp `filterbank` lists (61 clean bands, order 4, ⅓ ERB; 41 fast bands, order 2, ½ ERB; 2 kHz down to
> 24 Hz) at 22.05 – 24 kHz at every host rate, one band per partial (level selection, design rotations,
> skirt gain), a harmonic shaper with complex waveform coefficients, a purity gate, the per-band hold and
> floor, and the event-driven fast → clean handover; Mono adds one tracked voice (gfx-dsp `osc`, `track`,
> `detect`, `onset`, `multipitch` with 6 notes) bridged by the core on the attack. New in gfx-dsp: `vcf`
> (gfx-autowah's filter path, bit-identical to it), `osc::pulse_width` / `triangle`, `filterbank::band_gain`
> / `crossover` / `design_rotation`. Zero latency. Measured on synthetic strings: harmonics within 0.04 dB
> (clean) and 0.8 dB (fast) of the waveform as a sine crosses the bands (the drafted shaper: 5 – 26 dB);
> chords −33 … −46 dB of off-harmonic energy on the clean bank and in Dual's body (a full-band fuzz: −17 …
> −24 dB); Dual follows a vibrato with 14 – 16 ms of lag at 91 – 97 % depth and a hammer-on in 14 ms; Mono's
> voice within 2.8 – 4.0 ¢ RMS of bends and vibrato, no gap over 2.7 dB at the take-over; bench 28 / 28 at
> 44.1 – 192 kHz; CPU 1.49 – 1.62 µs per sample on the bench's pink noise (7.1 – 7.8 % of a core, budget
> 8 %; a shared machine), 4.9 – 6.7 % on strums. The DI rows of §5 wait for the maintainer's recordings.

golem's **polyphonic guitar synthesizer**, from a normal guitar or bass DI (no hex pickup, no MIDI).
Each note of a chord becomes a synth tone: a band-limited waveform, a resonant filter with its own
envelope, an amplitude envelope that can follow the string or be a synth's ADSR, detune, sub and
noise. It plays the role of the Boss SY-1 / SY-200 / SY-300, the EHX Synth9 / Mel9 / Key9 family, the
Meris Enzo (mono and poly modes), and of what hex systems (Roland GR, Fishman TriplePlay) and Jam
Origin's MIDI Guitar do in software.

**Three engines, one honest trade-off.** Nobody can name the notes of a low chord in less than about
0.1 s (§1.3): the physics of resolving two partials a few hertz apart. So:
- **Spectral** (default) works in the audio domain, without naming any note. A filterbank splits the
  guitar so that each band holds one partial; each band is turned into a waveform at its own phase.
  It is polyphonic by construction, has no tracking error, and follows bends by itself. It is the
  "synth-ified guitar" of the Boss and EHX pedals, done with a measured advantage on chords (§2.2).
- **Mono** is the lead and bass synth: one voice, glide, exact bends, octave guard, bridged by
  Spectral on the attack.
- **Poly** (research, **not in v1**) gives every named note a real synth voice (oscillators, its own
  filter, its own envelope, pitch from a phase-locked tracker). Naming a chord safely takes 0.13 – 0.4 s
  (§1.3), so the Spectral engine plays the attack and hands each note to its voice once it is named
  (§2.4). It is an engine for held chords and pads: at more than two or three strums a second the
  voices never arrive and what is heard is Spectral. The review's prototype found the drafted start
  rule far from its wrong-voice target (§1.3), so the `engine` enum of v1 does not declare it: an enum
  option cannot be withdrawn, and can be appended.

Zero reported latency in every engine; the inherent lags (filterbank group delay, naming time) are
stated in §2.6 with their targets.

It is a **new golem block** (`polysynth`, proposed). Checked on 2026-11-14 in
`~/golem/library/catalog/effects.json` (79 entries): no block makes synth tones from a guitar. `vital`,
`organ`, `ep`, `piano` are MIDI-played instruments (category `keys`); `octaver` and `surge-treemonster`
are the nearest audio effects. Against the golem-fx pedals:

| Pedal | What it makes from the guitar | Polyphonic | Pitch source | Latency |
|---|---|---|---|---|
| gfx-octaver Synth | one oscillator locked to the divider, sine → square, under the note | no (mono) | divider edges | 0 |
| gfx-octaver Poly | octaves of every note (z² / \|z\|, √z per ERB band) | yes | none (per band) | 0 (band lag) |
| gfx-ringmod Track | the input × a carrier at a tracked interval | the input yes, carrier mono | YIN + phase lock | 0 |
| gfx-vocoder Tracked | a carrier chord following a voice, shaped by its formants | carrier from one pitch | YIN | 0 (band lag) |
| gfx-sustainer | the note's own partials, phase-locked, added on top | ≤ 4 notes (All) | `track` + `multipitch` | 0 (bloom ≥ 90 ms) |
| **gfx-polysynth** | **a synth tone per partial (Spectral) or per note (Mono; Poly later): waveform, filter, envelopes** | **yes** (Spectral) | Spectral: none · Mono: YIN + `track` · Poly (later): `multipitch` + `track` | **0** (§2.6) |

## 1. State of the art

Survey of 2026-11-14. **[V]** read in a source, a manual or a paper · **[M]** manufacturer's page ·
**[F]** review or forum post · **[A]** computed here (numpy prototype on synthetic strings, §1.3) ·
**[I]** inference · **[U]** claimed, could not be verified.

### 1.1 Pedals, hex systems and software

| Reference | How it makes the sound | What players report | What we take |
|---|---|---|---|
| Boss SY-1 / SY-200 / SY-300 | "Latency-free", all sounds polyphonic, no special pickup [M]. Sound On Sound (SY-1): "much of the sound is derived by processing the signal from the strings themselves rather than tracking the pitch to drive an oscillator" [F]; the SY-200 shares the engine [F]. 44.1 kHz, 32-bit float (SY-200) [M] | "Near-zero latency", no tracking errors [F]; **"playing chords reveals an underlying grumble"** on octave patches (SoS SY-1) [F]; "the tone can wobble slightly when you play chords" (SoS SY-200) [F]; level drops on the top two strings on some patches [F]; needs a clean signal, first in the chain [F]; fixed attack, no slow onsets [F] | The audio-domain approach is the market leader; its fault is the chord "grumble" (intermodulation), which §2.2 measures and reduces |
| EHX Synth9 / Mel9 / Key9 / B9 / C9 | "Does not trigger samples", no MIDI, no special pickup [M]. SoS (Synth9): "all the other synth waveforms are derived from processing the guitar's output signal… the only pitch tracking involved relates to the filters"; the poly Solo Synth sounds use "a separate fuzz circuit for each string"; the portamento voice "appears to use a more traditional form of pitch tracking" and is the one with tracking problems and "occasional pops and thumps" [F]. Usable to the 23rd fret on the high E; no drive before it [M] | "Feels very immediate" [F]; sensitive to touch noise (internal compressor) [F]; in tune or it sounds wrong [F]; owners split from "quite terrible" to "surprisingly good" tracking [F] | Per-string (per-band) shaping is the right idea; tracked voices are where the glitches live, so they need a guard and a bridge |
| Meris Enzo / Enzo X | Modes Mono, Poly, Arp, Dry; two oscillators per voice (saw / square), detune, filter with envelope [F]. **Poly: chords, but no pitch bend** (SoS) [F]; Mono: reliable "if you only play one note at a time… two or more notes… quite sporadic" [F] | Poly "fast and reliable" (SoS), "possible but not great", weak on low-string chords (owners) [F]; long sustain → note clashes on chord changes [F] | The complaint "the real synth (bends, glide) is mono-only": Poly here follows bends per voice (§2.4) |
| Roland GR (GK hex pickup), Boss SY-1000 / GM-800, Fishman TriplePlay | One pitch detector per string (hex pickup): the chord problem disappears, the period problem stays. GM-800 with GK-2A: ~58 ms extra latency on the open low E, ~35 ms on the high E (one forum measurement, MIDI path) [F] | Low strings slow; scale-length and pickup-height sensitivity; better than software on speed, worse on false triggers (one forum comparison with MIDI Guitar) [F] | Out of scope (needs a hex pickup); the reference for what tracked voices can reach |
| Jam Origin MIDI Guitar 2 / 3 | Polyphonic transcription from mono audio, "13 years" of development, ML-trained models in v3 [M]. Latency switchable 6 / 11 ms (v1, 2014) [F], plug-in faster than standalone by 3 – 6 ms (SoS 2017) [F]; no figure published for chords [U] | Picked, fingerstyle and legato tracked; spurious notes "at least related to what you played" [F]; trouble holding more than 4 notes (one App Store review) [F]; "always a slight latency" (Jam Origin) [M] | Proof that transcription works; its single-note latency is not a chord latency (§1.3) |
| Schramm, Visi, Brasil, Johann, *A polyphonic pitch tracking embedded system for rapid instrument augmentation*, NIME 2018 [V] | PLCA with a per-guitar template dictionary on a constant-Q spectrogram, 2,048-sample window, hop 256 – 1,024 at 44.1 kHz, median filter over 3 frames, wavetable voices | **Total latency ≈ 45 ms (hop 512) and 70 ms (hop 1,024)**; hop 256 not felt by guitarists. Errors: **chord notes detected one octave below**, **false notes at the 2nd and 3rd harmonics of low notes (E2 – A2)**, and **glitches from note on / off chatter** when an activation hovers between the two thresholds [V] | The three failure modes our Poly guard targets (§2.4) |
| Klapuri, *Multiple fundamental frequency estimation by summing harmonic amplitudes*, ISMIR 2006 [I] | Harmonic summation with iterative estimation and cancellation; spectral smoothness for the cancelled note | — | `multipitch`'s method (gfx-sustainer §2.2) |
| NMF / PLCA templates, CREPE, Basic Pitch (Spotify, 2022) [I] | Learned or template-based; frames of 64 – 1,000 ms and an offline or block-based pipeline | — | Too heavy and too latent for a zero-latency pedal: not taken |
| gfx-octaver Poly (coded) [V] | 28-band ERB bank of complex one-poles, z² / \|z\| up, branch-tracked √z down | — | The per-band phase idea; its bank is too coarse for chords at order 1 (§2.2) |

### 1.2 Forum feedback: what players love and hate

- **Glitches on chords**: notes that are not played, octave jumps, chatter (Enzo owners, NIME 2018,
  SY-200 launch firmware) [F, V]. → Never start a voice on an unconfirmed note; let the Spectral engine
  carry the gap (§2.4).
- **Latency** on low strings, the hex systems' complaint (GM-800 ~58 ms on low E) [F]. → Zero reported;
  Spectral's fast bank is within 2 dB of its body level 20 ms after the pick on every note [P] (§2.6).
- **"Sounds like a fuzz, not a synth"**, the cheap guitar-synth sound and the SY-1's chord "grumble" [F].
  It has five causes, each with a cure here: (1) **intermodulation** between the strings in one
  nonlinearity (cure: one shaper per narrow band, 10 – 30 dB less than a fuzz on the body in §1.3); (2) the **string's decay**
  instead of a held level (`compress`, `amp_mode` Gate); (3) **no filter envelope** (`filt_env`); (4) the **pick's attack** instead of the synth's (`attack`, the Gate envelope);
  (5) **inharmonic, beating partials** of the strings (only a tracked voice is an exact waveform: the
  reason Mono, and later Poly, exist).
- **Tracking on bends**: portamento voices that step or lag (Synth9) [F]; Enzo Poly cannot bend [F].
  → Spectral follows bends by construction, with its band lag: the fast bank's 10 – 16 ms; the clean
  bank's 29 – 64 ms is too slow for a vibrato, so a band whose pitch moves goes back to the fast bank
  (§2.2). Mono's voice is phase-locked to the string by `track` (gfx-sustainer: ≤ 0.3 rad on a ±30 ¢,
  6 Hz vibrato at E4 [A, sustainer]).
- **Touch noise triggers sounds** (Synth9's compressor) [F]. → `thresh` gate on the bands and on the
  voices; Follow mode's level comes from the band, not from a compressed input.
- **Clean signal, first in chain** (every maker) [M]. → Same here; the Applications say so.

### 1.3 What can be achieved: the prototype [A]

A numpy prototype (48 kHz; strings of up to 7 kHz of partials, amplitudes |sin(π k p)| / k with p = 0.13,
inharmonicity B = 10⁻⁴, T60 3 s falling with k, a 4 ms pick noise, random phases; strum spread 20 ms)
measured both families. Scripts `proto.py`, `p2.py` – `p7.py` in the design session's scratchpad
(`polysynth/`); the results are reproducible from this description. Synthetic strings, no DI yet.

**(b) Per-band shaping.** Each band's analytic signal z gives a unit phasor u = z / |z|; a harmonic n of
the band is |z| · Re(uⁿ), exact for a band holding one partial. Metric: energy at frequencies that are
not a harmonic (±25 ¢) of any played note, relative to the total, 30 Hz – 8 kHz, on the held body
(0.35 – 1.35 s). Dry input: −48 … −60 dB (the measurement floor). A full-band tanh fuzz (gain 30) is the
reference. Square wave (odd n, weight 1 / n), bands shaped up to 1.2 kHz:

| Input | Full-band fuzz | Order 2, 0.5 ERB, ≥ 40 Hz | Order 4, ⅓ ERB, ≥ 20 Hz (n ≤ 9) |
|---|---|---|---|
| E5 power chord (E2 B2 E3) | −19 … −24 dB | −18 dB | **−41 dB** |
| Open E major | −21 … −22 dB | −18 dB | **−44 dB** |
| A minor barre (5th fret) | −18 … −20 dB | −18 dB | **−41 dB** |
| Major third E4 + G♯4 | −16 dB | −25 dB | **−57 dB** |
| Major third E2 + G♯2 (unresolved: 21 Hz apart) | −17 dB | −11 dB | **−29 dB** |
| Single E2 / E4 | −48 / −49 dB | −50 / −59 dB | −49 / −59 dB |

(The fuzz figures moved by up to 5 dB between runs with different random partial phases.)

- A **narrow, steep band** is what removes the "grumble": order 4 at ⅓ ERB is 20 – 40 dB cleaner than a
  fuzz on chords. An order-2 bank is no better than the fuzz: a neighbour partial 27 dB down in a band
  modulates its phase, and uⁿ multiplies that phase modulation by n.
- A **purity gate** (the band's instantaneous-frequency variance over 10 ms) that falls back to n = 1 in
  bands holding two partials gained 6 – 9 dB at order 2 and is kept.
- The price is **lag**: an order-N cascade of complex one-poles of full −3 dB width B delays its band by
  **N / (π B)** at the centre: 64 ms at the low E for the clean bank, 16 ms for the fast one (§2.2).
  Δt · Δf: resolving partials a few hertz apart takes time, in any method.

**Review's re-run [P]** (`proto-gfx-polysynth/` in the review scratchpad; the same strings, the banks at
24 kHz exactly as §2.2 specifies them, three seeds). The table above was measured on another
configuration than §2.2's (bands shaped up to 1.2 kHz, linear bands kept up to 6 kHz); as specified
(every band to 2 kHz shaped, nothing above), and with the shaper the review retained:

| Input (square, body 0.35 – 1.35 s) | Fuzz | Fast, drafted shaper | Fast, retained | Clean, drafted shaper | Clean, retained |
|---|---|---|---|---|---|
| E5 power chord | −19 … −22 dB | −15 … −16 dB | −22 … −24 dB | −40 … −44 dB | **−47 … −49 dB** |
| Open E major | −20 … −23 dB | −19 dB | −24 … −26 dB | −44 … −45 dB | **−37 … −39 dB** |
| Open C major | −18 dB | −14 … −16 dB | −22 … −23 dB | −34 … −37 dB | **−40 … −41 dB** |
| A minor barre | −19 … −21 dB | −17 … −21 dB | −27 … −29 dB | −40 … −41 dB | **−42 dB** |
| E4 + G♯4 | −17 … −20 dB | −20 … −21 dB | −25 dB | −42 dB | **−48 … −49 dB** |
| E2 + G♯2 (unresolved) | −16 … −17 dB | −11 dB | −18 dB | −29 … −30 dB | **−27 dB** |

- The drafted shaper (every band shaped and summed, weighted by |z|) fails on a point the metric above
  cannot see: **the timbre depends on the pitch.** Neighbouring bands read the same partial with
  different phases (order 4: ±115° at the crossover, ±191° one band away), and uⁿ multiplies that
  difference by n. A steady sine swept across a few band spacings: the level of output harmonic n swings
  by **5 – 26 dB** (n = 2 … 9, both banks), a comb that moves with every bend and vibrato [P].
- Retained: **one band per partial** (the local maximum of the band levels, crossfaded to its neighbour
  at their crossover, where a fixed design rotation makes the two phases equal), with the band's skirt
  gain divided out. Harmonic levels then stay within **0.5 dB** (clean) and **1.0 dB** (fast) across the
  same sweeps, and chords are as clean or cleaner (the skirt bands, which hold the most mixed signal, no
  longer sound) except the open E (−38 against −44 dB) [P].
- Correcting each band's phase from its instantaneous frequency instead (a "reassigned" phasor) is flat
  to 0.4 dB but costs 10 – 15 dB on chords: the band's phase slope is 0.4 rad / Hz, so any wobble of the
  frequency estimate becomes phase modulation, times n. Not taken [P].
- The "retained" columns were run with the levels and the frequency estimate smoothed over 30 ms. With
  §2.2's final constants (levels: attack 3 ms, release 50 ms; frequency: 10 ms, which removes an attack
  swell) the clean bank reads −33 … −47 dB on the five chords and −24 dB on E2 + G♯2 (one seed) [P].
- **The fast bank is a fuzz-grade attack on chords**: −14 … −21 dB as drafted (no better than the full-band
  fuzz), −22 … −29 dB retained. Dual's promise is the body, not the first 60 – 120 ms of a strum.

**(a) Naming the notes.** A semitone-grid bank (order 2, ½ semitone wide, ≥ 6 Hz), Klapuri-style harmonic
summation over 16 partials, cancellation with the smoothed partial amplitudes, a peak-picking rule (a
candidate must beat its two semitone neighbours) and a fundamental-presence check on a second, wide
(25 Hz) register bank. Time from the last string's onset to a correct reading held for 50 ms:

| Input | Named after |
|---|---|
| Single E4 / major third E4 + G♯4 | 8 ms / 28 ms |
| Single E2 / E2 + G♯2 | 144 ms / 152 ms |
| E5 power chord / open E major | 106 ms / 110 ms |
| Open C major / A minor barre | 287 ms / 359 ms (5 – 7 % of frames wrong after naming) |

- **These times are not the time to a safe voice** [P, review]. Reading the same bank every 10 ms from
  the first string's onset and starting a voice when a note is in two successive readings (the drafted
  rule, with the fundamental check): **4.4 wrong voices per strum** (346 in 78 strums, 13 inputs × 6
  seeds; a lone E2 starts 4 – 6). Requiring the note in every reading for 60 ms: 0.7 per strum; for
  120 ms: 0.18 per strum, and uncovered notes begin to be missed (15 inputs × 4 seeds). The target is 0.01. During the attack
  the bank is smeared and stably wrong for tens of milliseconds: two readings 10 ms apart agree on the
  same error. Hence §2.3: no voice starts on the early reading.
- **High notes are fast, low notes and full chords are not** (0.1 – 0.36 s), whatever the method: the
  same order as gfx-sustainer's `multipitch` windows (85 / 171 / 341 ms) and the NIME system's 45 – 70 ms
  per hop plus its smoothing.
- **Covered notes cannot be named**: in an open E chord, B3 = E2 × 3 (2 ¢) and E4 = E2 × 4 (exact), the
  collision table of gfx-tuner §2.5. Nothing in the spectrum says the B string is played. A synth voice
  on E2 already holds those partials: the chord is voiced, not every string. That is a limit to state,
  not a bug to fix.
- My first, naive cancellation named E3 on a lone E2 and leaked to semitone neighbours with wide bands:
  the octave and neighbour errors of the NIME paper reproduce at once. The guard rules of §2.4 are not
  optional.

**Conclusion.** The audio-domain engine is the robust, instant core; tracked voices are a layer on top
that takes over each note once it is named, and are never allowed to be the only thing that sounds
during naming. That is the hybrid the brief anticipated, now with numbers. The numbers also say which
layer is ready: Spectral (once its shaper sounds one band per partial) and Mono; Poly's naming is not.

## 2. Design

### 2.1 Signal path (mono; the 2 / 2 layout processes the mono sum, `spread` places the unison pair)

```
in ──┬──────────────────────────────────────────────────────────── dry gain (`dry`) ───────────────┐
     ├─ HPF 25 Hz ─ ↓ to 24 kHz ─ Spectral core (§2.2): fast bank ┐ handover   one band     ownership    │
     │                            clean bank ────────────────────┴ per band ─ per partial ─ mask (voice) │
     │                            ─ harmonic shaper ─ ↑ ─ core out ─ vcf* ─ amp env ──────┐               │
     └─ detection (§2.3): onset, YIN, multipitch, track ─ voice (§2.4): osc ─ vcf ─ VCA ──┴ synth gain ─ ceiling ─ sum ─ out
```

- **Spectral**: the core only; one global filter `vcf*` and one global envelope (paraphonic).
- **Mono** (and Poly, later): the core plays each note's attack; the voice takes over; a voice has its
  own filter and envelopes, and `vcf*` is bypassed for it (it still filters the core's residue).
- The dry is untouched (one gain, no filter) and sample-aligned: nothing is delayed.
- Layouts: 1 / 1 first (the contract's rule: the pedal sits first in a mono lane), 2 / 2 second.
- A **ceiling** ends the synth bus (after the synth gain, before the sum with the dry): transparent
  under −6 dBFS, an ADAA tanh above it, never past 0 dBFS. `compress` (up to +30 dB per band), `reso`
  (up to +26 dB at the peak) and unison voices can otherwise add up to tens of dB over the input.

### 2.2 The Spectral core

- **Banks** (gfx-dsp `filterbank`: complex one-pole cascades, f64 state, SIMD lanes), run at a **bank
  rate of 22.05 – 24 kHz at every host rate** (÷ 2 at 44.1 / 48 kHz, ÷ 4 at 88.2 / 96, ÷ 8 at 176.4 / 192),
  between minimum-phase half-band decimators and interpolators from `oversample` (≈ 0.1 – 0.2 ms of
  group delay [I, `oversample` figures; bench]). One design, one sound and one CPU cost at every rate.
  - **Clean**: order 4, bands ⅓ ERB apart and ⅓ ERB wide, never narrower than 20 Hz; 40 Hz – 2 kHz:
    **59 bands** [A]. Lag N / (π B): 64 ms from E2 to 330 Hz, 29 ms at 1 kHz, 16 ms at 2 kHz [A].
  - **Fast**: order 2, ½ ERB apart, ≥ 40 Hz wide: **40 bands**, lag 16 ms up to 660 Hz, 10 ms at 1 kHz,
    5 ms at 2 kHz [A].
  - **Widths here are the full −3 dB width B of one pole** (pole radius e^(−π B / fs)). gfx-vocoder's
    `GammaBank` states the half-width b (e^(−2π b / fs)): B = 2 b, so "20 Hz" here is b = 10 Hz there.
    These banks are also narrower against their spacing (b / Δ = 0.5 above the floor) than `GammaBank`'s
    flat-sum rule (w0 0.55 – 0.90) allows, on purpose: selectivity, not a flat linear sum, is the aim, and
    the shaper below does not sum neighbouring bands. `filterbank` therefore takes an explicit list of
    (centre, pole width, order) as well as `GammaBank`'s solved scales.
  - The lowest band follows `lowest` (× 0.8, so a bass's low B at 31 Hz has its band).
  - Above 2 kHz nothing of the guitar passes: the synth's highs are the shaped harmonics of the bands
    below (up to `fcap`). This also removes the pick's noise and the densest region of shared upper
    partials, where no zero-latency bank can separate the strings [A: 6 notes put a partial every
    20 – 80 Hz above 1 kHz, where ⅓ ERB is 45 – 80 Hz wide].
- **One band per partial** (new in `filterbank`, the review's change: §1.3). Summing every shaped band
  makes each harmonic's level swing 5 – 26 dB with the pitch. Instead:
  - **Selection.** L_b = the band level in dB, smoothed (attack 3 ms, release 50 ms). Band b sounds with
    the weight s_b = c(L_b − L_(b−1)) · c(L_b − L_(b+1)), c(d) = clamp(d / 6 dB + ½, 0, 1): 1 where the band
    is a local maximum, 0 on a neighbour's skirt, and a crossfade over ±3 dB around the frequency where
    two neighbours read the same level. The two weights there sum to 1.
  - **Design rotation.** Each band's phasor is multiplied by a fixed e^(jρ_b), ρ_(b+1) = ρ_b + φ_b(f×) −
    φ_(b+1)(f×), φ the bands' phase responses at their crossover f× (computed with the design): at the
    one place where two bands sound together, they read the same phase, for every harmonic.
  - **Skirt gain.** |z_b| is divided by |H_b(f̂_b)|, f̂_b the band's instantaneous frequency smoothed over
    10 ms and clamped to ±0.6 of the half-spacing (at most +5 dB): a partial reads its own level wherever
    it sits between two centres.
  - Measured [P]: harmonics 1 – 9 within 0.5 dB (clean) and 1.0 dB (fast) while a sine crosses several
    bands; chords in §1.3's review table. A sine sweep at `wave` Sine is flat within ±1 dB [bench]; no
    G_Σ or edge trim is needed (no linear sum).
  - The smoothing of f̂ is a trade to settle on the bench: at 30 ms the chords are 2 – 4 dB cleaner, and
    the output swells by 5 – 11 dB between 40 and 120 ms after a pick (during the attack every skirt
    band is for a moment a local maximum, and a slow f̂ over-corrects it); at 10 ms there is no swell [P].
- **Handover fast → clean** (`bank` Dual, the default), per band and from the band's own time constant
  τ_b = 1 / (π B_b) (clean bank: 16 ms up to 330 Hz, 7 ms at 1 kHz):
  - An **event** at a frequency puts that region on the fast bank (x = 0, reached in 5 ms). x then
    rises to 1 (the clean bank) on a raised cosine from 3.5 τ_b to 7.5 τ_b after the last event: 56 →
    119 ms up to 330 Hz, 25 → 54 ms at 1 kHz. A clean band sounds with x_b, a fast band with 1 − x
    interpolated at its centre. No level detector decides the moment: at 7.5 τ an order-4 band has
    settled within 0.5 dB.
  - Events: a pick (gfx-dsp `onset`, detector A: every band); **new energy** (the fast bank's level at
    that frequency exceeds the clean band's by more than 3 dB for 5 ms: a hammer-on, a slide, a note the
    onset detector missed); **pitch motion** (the two banks' frequencies for that partial differ by more
    than 15 ¢ for 5 ms: a bend or a vibrato in progress, which the clean band would follow 64 ms late and
    at about half its depth [I: (1 + (6 / 10)²)⁻² = 0.54 for 6 Hz through four 10 Hz poles]). So the
    clean bank sounds only where both banks agree, which is where a long window is valid; a note that
    keeps ringing while another is picked stays clean unless the pick is detected (then 60 – 120 ms of
    fast). [I: the two local events are not prototyped; bench rows "Legato" and "Vibrato".]
  - Measured with the pick event only, single notes E2 … E5, saw [P]: the output is within 2 dB of its
    body 20 ms after the pick; no dip deeper than 1.5 dB (10 ms RMS) through the crossfade, **without
    any phase rotation between the banks**. The drafted rotation of the sounding fast phasor onto the
    clean one (smoothed over 5 ms) is removed: it is a phase glide of up to n · π in 5 ms on harmonic n.
    If the bench finds single harmonics dipping during the crossfade, the silent side is the one to
    rotate: the clean band's phasor snapped onto the fast one's at the start of its fade-in, relaxing
    to its design rotation at no more than 5 ¢.
  - `bank` Fast = fast bank only (leads, bends: 10 – 16 ms of pitch lag; chords at fuzz-grade
    cleanliness, §1.3); Clean = clean bank only (pads: on the low strings the attack reaches −6 dB after
    about 60 ms and −1.5 dB after 80 – 100 ms [P]; no fast-bank grit).
- **Harmonic shaper** (per sounding band): u = e^(jρ_b) · z / |z| (guarded below the floor); the output of
  band b is
  **y_b = s_b · g_b · |z_b| / |H_b(f̂_b)| · Σ_n Re(c_n · uⁿ · e^(jnd_b))** over n ≤ n_max(b),
  - c_n **complex**, from `wave`, so that the band's waveform is the real one and not its high-crest
    twin: Saw −j / n (Σ sin nθ / n) · Pulse sin(π n `pw`) / n (real; 50 % = square) · Triangle
    −j (−1)^((n−1)/2) / n², odd n · Sine c₁ = 1 only. With the draft's real 1 / n weights on Re(uⁿ) a saw
    is −ln |2 sin θ/2|: crest factor 10.2 dB against 5.7 dB, and a square 7.3 dB against 1.6 dB (n ≤ 9)
    [P]: 4.5 – 5.7 dB of headroom lost into `drive` and the ceiling for the same spectrum. Normalised by
    the waveform's RMS, so switching waves keeps the level within ±1 dB.
  - n_max(b) = min(9, ⌊fcap / f_b⌋) with **fcap = 0.42 · the bank rate** (10 kHz at 48 kHz, 9.3 kHz at
    44.1); the top harmonic is faded by the margin (fcap − n · f̂) over 500 Hz, so a bend never pushes a
    harmonic past the bank's Nyquist [A: n ≤ 9 measured as clean as n ≤ 5 at order 4]. fcap sits under
    the half-band's pass-band edge (0.23 · 2 × the bank rate = 11 kHz), so the interpolator's image of
    the top harmonic is in its stop-band.
  - **Aliasing**: for one partial in a band, uⁿ is a single line at n f: nothing to fold. Two partials
    in a band make sidebands at n f ± k Δ that fall geometrically with k: with ≥ 1.9 kHz between fcap and
    Nyquist and Δ ≤ 80 Hz they are far under −70 dBc before they fold [I]. The exposed cases are a band
    near its floor (u = z / |z| of noise: closed by the floor's fade) and `octave` +2 (4 n): bench row.
  - **Purity gate**: w_b = exp(−σ²_IF / (0.25 · B_b)²), σ²_IF the instantaneous frequency's variance over
    10 ms. The band's output is w_b · (shaped) + (1 − w_b) · (the band unshaped, n = 1). Two partials in
    one band give the guitar's own sound, not a growl. It cannot see partials closer than about 30 Hz
    (their beat is slower than its window): those bands beat, as the strings do.
  - **Octave** (−2 … +2): +1 / +2 shape uⁿ at 2 n / 4 n (gfx-octaver's z² / |z|); −1 / −2 take gfx-octaver
    Poly's branch-tracked √z on bands below 400 Hz, then shape. The sub-octave quality is gfx-octaver
    Poly's, whatever its bench says; this pedal does not try to improve it.
  - **`sub`**: one more component per band below 400 Hz, |z| · Re(√u) (sine at half the band's
    frequency, branch-tracked), level `sub`.
  - **`detune`** (unison): a second copy of each harmonic rotated by e^(jnd_b), d_b += δ · Δφ_b per
    sample (Δφ_b = arg(u_t · ū_{t−1}), δ = 2^(`detune`/1200) − 1): the copy runs `detune` cents sharp of
    the band, whatever the note, and never needs the unwrapped phase. Copy and original at −3 dB each.
    At `detune` 0 the copy is not computed.
  - **`noise`**: band-limited noise in each band (xorshift, fixed seed), shaped by |z_b| (a breath that
    follows the strings).
- **Per-band level g_b** (`compress`, `thresh`):
  - P_b = a peak hold of |z_b| (attack 1 ms, release 3 s): the band's body.
  - g_b = (P_b / |z_b|)^c, c = `compress` / 100 %, capped at +30 dB: at 100 % every partial holds its body
    level until it falls under the floor (organ / synth sustain without the hiss: the floor stops it).
  - Floor: a band whose |z_b| falls under `thresh` − 10 dB (band-relative calibration: a sine of level L at
    the band centre reads L, i.e. twice the analytic band's modulus) fades out over 30 ms, with 6 dB of
    hysteresis.
- **Global filter and envelopes (Spectral)**: the shaped sum → `vcf` (§2.5) → amp envelope (§2.5). Both
  envelopes are triggered by every pick onset (detector A): paraphonic, as the Boss and EHX pedals.

### 2.3 Detection (side chain; Mono, and Poly later)

All of it reuses gfx-sustainer's detection (§2.2 there), with the same rules:
- **Onsets**: gfx-dsp `onset` (gfx-swell's module): pick detector A, legato pitch-jump detector C (≥ 70 ¢).
- **Single notes**: `detect::PitchDetector` (YIN at ~12 kHz, harmonic lock), floor `lowest`; first reading =
  window + a period + a hop: 29 ms at E2 with `lowest` 70 Hz, ≈ 12 ms at E4 [A, gfx-sustainer's rule];
  under 60 Hz, gfx-tuner's long detector (gfx-sustainer §2.2). Octave guard: a ×2 / ÷2 jump must hold
  40 ms (gfx-golem's `OCTAVE_CONFIRM_MS`; it moves into `detect` only under gfx-golem's own null test,
  otherwise this crate carries its copy).
- **Chords** (clarity poor, CMNDF > 0.2): `multipitch::Analyzer` exactly as gfx-sustainer specifies it
  (FFTs of 85 / 171 / 341 ms at 6 kHz every 43 ms, harmonic summation with cancellation; a reading is
  confirmed when two successive analyses agree within ±50 ¢). **Mono needs nothing more**: on a chord
  it plays the lowest confirmed note (gfx-sustainer's `chord` Lowest), and waits for it as that pedal
  does (at most 400 ms, the core sounding meanwhile).
- **`multipitch` extension, agreed here for both crates** (it costs gfx-sustainer one argument):

  ```
  multipitch::Analyzer
    prepare(sample_rate, f_min, max_notes)   // max_notes 1 ..= 6. gfx-sustainer passes 4: its results are unchanged
    reset()   push(x)
    notes() -> &[Note { hz, salience }]      // unchanged: confirmed readings only; the max_notes most salient, sorted by pitch
    age_ms()                                 // unchanged
    early() -> &[Note { hz, salience }]      // research, Poly only: an unconfirmed first reading; empty unless
                                             // enabled at prepare. Never used to start a sound (§1.3)
  ```

  - With `max_notes` = 4 the analysis must stop after four cancellations exactly as before: a 6-note
    caller must not change what a 4-note caller reads.
  - `track::Tracker` is used as gfx-sustainer wrote it (`prepare(sample_rate, 0)`: reference only,
    `Engage::Keep`, `Lock { psi, inc, level, locked }`); no change is asked.
- **Acceptance (Poly, when it comes)**: a voice starts only on a **confirmed FFT reading** (so 128 ms
  after the last onset at the earliest: the 85 ms window and one more analysis; up to 384 ms on low
  chords) **and** when the note's fundamental band in the Spectral core's clean bank is within 20 dB of
  the strongest band of the note's own partials (no "missing fundamental" notes on a guitar, which
  kills the sub-octave error). The drafted "stable over 2 readings" of a semitone-grid bank starts 4.4
  wrong voices per strum (§1.3) and is not a start rule; `early()` may only prepare a voice (allocate
  it, set its tracker's feed-forward) so that it is locked when the confirmation arrives. A note
  **leaves** when it is missing from two successive confirmed readings, or on a mute / touch
  (gfx-sustainer's rules), or when its tracker's level falls under `thresh` − 6 dB.
- **Phase lock**: one `track::Tracker` per voice (K = 0: reference only), fed forward by the accepted
  f0, engaged with `Engage::Keep`; the loop bandwidth clamp(f0 / 5, 14, 50) Hz follows bends and vibrato.
  The voice's oscillator runs at the tracker's increment: exact pitch, bends without steps. In Mono the
  oscillator starts at YIN's first reading, on YIN's f0, and the tracker pulls it in (it does not wait
  the five periods of the tracker's engage: 61 ms at E2).

### 2.4 Voices (Mono; Poly later) and the bridge

- **Voice** = 2 oscillators (gfx-dsp `osc`: PolyBLEP saw and pulse, triangle and sine from band-limited
  tables, f64 phase; osc 2 detuned by `detune`), `sub` (a third at f / 2, square), `noise`, a `vcf`, an
  amp and a filter envelope. `octave` transposes the voice. One voice in Mono, with a second slot so
  that a release overlaps the next note. (Poly: up to `voices` 1 – 6, 8 slots.)
- **Allocation** (Mono): the voice follows YIN's note, or the lowest confirmed note of a chord. A
  re-pick (detector A) retriggers its envelopes; a legato change (C) moves it with `glide`.
  (Poly: a voice per accepted note, keyed by the note; `glide` when one note left and one arrived
  within 40 ms; no stealing beyond 6: the quietest note stays on the core.)
- **Pitch**: `quantize` Off = the tracker's frequency (bends, vibrato, slides pass); Chromatic = the
  nearest semitone with `NoteTracker`'s hysteresis (a vibrato across a boundary does not trill; the
  robot step).
- **The bridge (attack → voice)**, the key to "no glitch, no gap":
  1. The pick is heard at once through the core's fast bank (within 2 dB of its body after 20 ms [P]).
  2. When the note is read (Mono: YIN's first reading, 12 – 29 ms after the pick), its voice starts
     **with envelopes back-dated to the onset** (a filter sweep or an attack does not restart at the
     handover) and fades in over **10 ms** (Mono; Poly 30 ms).
  3. In the same time the core is **muted in the bands that the voice owns**: the bands whose centre
     is within ±⅙ ERB (at least half a band spacing) of the note's partials 1 … 16, the lower note
     owning a shared partial, as in gfx-sustainer's All. With one band per partial (§2.2) that is
     enough: a named E2 leaves −41 dB of core. With the drafted shaper it left **−9 dB** (the two
     neighbours of each owned band still carry the partial 14 dB down, shaped): −27 dB needed
     ±max(⅓ ERB, 20 Hz), 34 bands of 59 for one note [P].
  4. **Level**: the voice's gain is set at the handover so that its RMS before the filter equals that
     of the core bands it replaces (measured over the last 10 ms); from there Follow tracks the changes
     of its tracker's level. Without this rule the step is arbitrary: the core's loudness comes from
     every partial under 2 kHz, the voice's from d₁ alone.
  5. Whatever no voice owns (a chord's other notes in Mono; an unnamed or covered note in Poly) keeps
     sounding through the core at `residue`. So a missed note is heard as the core's synth, never as
     silence.
  6. The voice's phase at the handover: the oscillator is locked to the string, the core band lags it by
     its group delay. Crossfading the two at one frequency could dip; the voice's fundamental starts
     **rotated onto the core band's phase**, and that rotation relaxes to zero over 200 ms through the
     tracker's offset (≤ 5 ¢, the `Engage::Keep` rule). [I, bench "Bridge" row.]
  7. What the bridge cannot hide: the timbre changes at the handover, from one waveform per partial
     (the string's own spectrum, shaped) to one waveform per note. In Mono it happens inside the attack
     (12 – 40 ms). In Poly it would happen 0.13 – 0.4 s into every chord: part of why Poly must be judged
     by ear before it ships.
- **Amp envelope**: `amp_mode` Follow = the note's level from its tracker (d₁, ripple-free for one
  partial) through `compress` (the same law as §2.2 on the voice), with `attack` as the minimum rise and
  `release` after the note leaves; Gate = ADSR (`attack`, `decay`, `sustain`, `release`), held while the
  note is accepted. Gate in Spectral: the same ADSR on the whole core, triggered by every pick.
- **When the voice ends** (mute, touch, lock lost): it releases over `release` with its frequency held
  (gfx-sustainer's rule), and the core's owned bands are released from the mask over 30 ms; they are
  then at or near the floor, so nothing steps.

### 2.5 Filter and envelopes (gfx-dsp `vcf`, new)

- `vcf` = gfx-autowah's filter path as coded [V, `gfx-autowah/src/lib.rs`], extracted into gfx-dsp so
  that several owners can use it: **drive (ADAA tanh of the input × k_d, divided by √k_d; skipped at
  0 dB) → TPT SVF × 1 or × 2**, `type` Low-pass · Band-pass · High-pass (gfx-autowah's first three
  options, same order), cutoff set per sample.
  - 4 poles = a first SVF at Q 0.707, then one at Q: one resonance, not two stacked. The band-pass is
    the analogue SVF's resonant output (gain Q at the peak).
  - **Q = 0.5 + 19.5 · `reso`²** (0.5 … 20: gfx-autowah's own `peak` range, the one its bench covers).
    The filter does not self-oscillate (the drive is ahead of it, not in its loop; the draft said
    otherwise): `reso` 100 % is a +26 dB peak, held by the synth bus's ceiling (§2.1).
  - `drive` 0 – 100 % (the contract's id and unit) = 0 – 24 dB into the tanh, linear in dB (gfx-autowah
    keeps its catalogue's dB).
  - **The move is guarded by a null.** gfx-autowah is a shipped crate: it switches to `vcf` only if every
    render of its bench (each `type` × `poles`, `drive` 0 and > 0, mono and stereo) is **bit-identical**
    to the current build and its tests pass unchanged. `vcf` therefore keeps the operation order and the
    f32 / f64 types of the present code. If one sample differs, gfx-autowah keeps its inline code and
    `vcf` is a copy; either way its VST3 is not re-announced as a change of sound.
- **Cutoff** = `cutoff` · 2^((E_f · `filt_env` + `key_track` · (note − 60) ) / 12), clamped to
  20 Hz – min(0.45 fs, 20 kHz). E_f the filter envelope 0 … 1, `key_track` relative to C4 (voices only;
  inactive in Spectral, which has no note). The coefficient (one `tan`) is computed per sample while
  the cutoff moves and held otherwise.
- **Filter envelope**: attack `fenv_att`, decay `fenv_dec` to 0 (an AD: the classic synth "pluck" /
  "wow"), depth scaled by the pick's strength: 1 − `fenv_vel` + `fenv_vel` · s, s = the onset's level
  relative to the last 2 s of picks (gfx-autowah's relative follower), 0 … 1.
- Envelopes are linear-in-dB (attack linear in amplitude), times as in gfx-swell; smoothed parameter
  changes (`smooth`, `exp_smooth` on the EXP ones).

### 2.6 Latency, CPU, real-time

- **Reported latency: 0** in every engine. The dry is sample-aligned. Lags inherent to the method:

  | What | Spectral Fast / attack | Spectral Clean / body | Mono voice | Poly voice (later) |
  |---|---|---|---|---|
  | Band lag N / (π B) (low E · 330 Hz · 1 kHz band) | 16 · 16 · 10 ms | 64 · 64 · 29 ms | — | — |
  | Level after a pick [P] | within 2 dB of the body at 20 ms, every note | low E: −6 dB at ≈ 60 ms, −1.5 dB at 80 – 100 ms; E4: at the body by 40 ms | core first, voice 12 – 29 ms later (+ 10 ms fade) | core first, voice 0.13 – 0.4 s later |
  | Pitch follows a bend with | 10 – 16 ms | 29 – 64 ms, about half the depth of a 6 Hz vibrato [I] | the tracker's loop (≤ 0.3 rad on vibrato [A, sustainer]) | same |

  The band lag is the onset of the band's envelope, not an echo; under ~16 ms it fuses with the pick
  (gfx-vocoder §2.2's argument). Plus ≈ 0.1 – 0.2 ms for the decimated path [I].
- **What the 64 ms means for the player.** A picked note is not late: the fast bank speaks first and
  Dual's crossfade (56 → 119 ms on the low strings) shows no dip deeper than 1.5 dB [P]. What a
  crossfade triggered by picks alone would not hide is everything played without a pick: hammer-ons,
  pull-offs and slides would speak in 60 – 100 ms, and a vibrato would arrive 64 ms late (138° of a 6 Hz
  cycle: against the `dry` it beats like a chorus) at half its depth. That is why §2.2 sends a band back
  to the fast bank on new energy or pitch motion, and why `bank` Clean is a pad setting, said so in its
  Role. Chords pay on the other side: the first 60 – 120 ms of a strum are the fast bank's, at −22 …
  −29 dB of off-harmonic energy [P].
- **CPU** [A, estimate to measure], banks at 24 kHz, f64 SIMD × 4:
  - clean band ≈ 32 flops (4 complex poles) + ≈ 60 (normalise: a square root and a division; up to 8
    complex multiplies; frequency, purity, selection, detune copy) ≈ 90; fast band ≈ 70; 59 + 40 bands ≈
    8.1 k flops per bank sample ≈ 195 M flops / s; the frequency estimate needs an `atan2` per band:
    taken at 1 / 8 of the bank rate on the phasor demodulated by the band centre, never per sample;
  - half-band filters, `vcf`, envelopes: < 5 M flops / s;
  - at ≈ 2 – 4 G flops / s effective with SIMD: **Spectral ≈ 5 – 10 % of a core** [A, wide: to measure];
    gfx-vocoder's measured-design figure for the same kind of band (≈ 80 flops, 330 – 520 ns per sample
    for 24 + 18 bands at 48 kHz, 1.6 – 2.5 % of a core) scales to about 3 – 4 % here;
  - what the estimate does not count, in our favour: a band off the selection (s_b = 0), under the
    floor, or on the silent side of the handover skips its shaper; the fast bank's shaper runs only
    around events. These lower the average, not the bound (a strum opens everything for 100 ms);
  - Mono adds YIN (≈ 11 M MAC / s, as gfx-ringmod), `multipitch` (≈ 1.5 M flops / s), one tracker and
    one voice: under 1 %. (Poly: 6 trackers and 6 voices, ≈ 2 – 3 % more.)
  - **Budget.** The contract's 20 % is for circuit simulations; this is not one, and the suite's other
    filterbank pedal (gfx-vocoder) is designed for ≤ 7 %. **≤ 8 % of a core per mono instance for v1,
    worst case (Dual, a strum, unison, Mono's voice sounding), the same at every sample rate** (the
    bank rate is fixed); ≤ 10 % with Poly later. That is the dearest non-circuit pedal of the suite: for
    the owner to accept. If the bench misses it, in this order: the clean bank drops to order 3 above
    1 kHz (where the bands are wide); `detune` in the core becomes one copy per band instead of per
    harmonic; the clean bank goes to ½ ERB above 600 Hz.
- **Real-time**: banks, FFT plans, voices and tracker state allocated in `prepare`; band designs (poles,
  rotations ρ_b, crossover tables) recomputed only when `lowest` changes, with gfx-vocoder's 40 ms bank
  crossfade, bounded and allocation-free. Flush-to-zero; phases and recursive state f64.
- **Determinism**: noise seeds fixed and reset; detune copies start at d = 0; every state zeroed on
  `reset()`.

### 2.7 gfx-dsp reuse and new code

| Reused | For |
|---|---|
| `filterbank` (gfx-vocoder's complex one-pole cascades and bank crossfade; not `GammaBank`'s scale, G_Σ or trims: §2.2) | Clean and fast banks |
| gfx-octaver Poly's z² / \|z\| and branch-tracked √z | `octave`, `sub` in the core |
| `oversample` (half-band IIR, minimum phase; cascaded ÷ 2 / ÷ 4 / ÷ 8) | The 22.05 – 24 kHz bank path |
| `onset` (gfx-swell) | Picks (A), legato (C) |
| `detect` (YIN, harmonic lock, `NoteTracker`, octave guard) | Mono, Chromatic (single notes in Poly) |
| `track`, `multipitch` (gfx-sustainer's APIs; `multipitch` with `max_notes`, §2.3) | Phase lock per voice; Mono's lowest note of a chord |
| `osc` (gfx-vocoder / gfx-ringmod plan), `noise` | Voice oscillators |
| `adaa`, `svf` | `vcf` |
| `envelope` (`PeakFollower`, `SlidingMax`), `smooth`, `exp_smooth` | Peak holds, levels, EXP |

| New | Content |
|---|---|
| **`vcf`** | gfx-autowah's drive → SVF × 1 / × 2 path as a reusable filter (autowah switches to it only if bit-identical, §2.5) |
| `filterbank` explicit design | A bank from a list of (centre, pole width, order), beside `GammaBank`'s solved scales; design rotations ρ_b and crossover tables |
| `filterbank::Shaper` | Band selection, skirt gain, phasor powers with complex c_n, purity gate, fcap fade, detune rotation, the event-driven fast → clean handover, ownership mask |
| `multipitch` | `max_notes` argument (v1); `early()` (research, with Poly) |
| in the crate | Voice, bridge, envelopes (ADSR, AD, Follow), ceiling, engine switching |

## 3. Parameters

### 3.1 Decisions

- New block, new ids. Linear ranges only; enums append-only once shipped; no mode-dependent defaults.
  A parameter that one engine ignores says so in its Role; none changes its default with the engine.
- **`engine` in v1 is Spectral · Mono.** Poly is appended as the third option when it passes §5, with
  its own parameter `voices` and meter `voices_on` (new ids can be added; an enum option cannot be
  removed, nor an order changed). Spectral first: it is the default and the pedal's identity.
- Ids shared with other golem-fx designs keep their meaning and unit: `thresh`, `lowest` (gfx-sustainer:
  same ranges and defaults), `dry` (gfx-sustainer: same range; the default differs, −60 dB here),
  `release` (gfx-sustainer; range wider here), `glide` (gfx-octaver, gfx-vocoder: 0 – 200 ms there,
  0 – 1,000 ms here for synth portamento), `wave` (gfx-vocoder: its first two options, Saw · Pulse, in
  the same order), `detune` (gfx-vocoder: same range), `quantize` (gfx-vocoder: its first two options,
  same order), `octave`, `noise`, `spread` (gfx-vocoder), `drive` (contract: 0 – 100 %), `type`, `poles`
  (gfx-autowah: its first three types, its two slopes, same order), `bypass`, `trails`, `state`.
- **`synth` and `dry` are two levels, not a `mix`**: accepted. The contract's `mix` is a catalogue id
  whose law belongs to an existing block; a new block need not have one, and gfx-sustainer set the
  precedent with `dry`. A synth is mostly played with the dry off or low, which no `mix` law says well.
- **`wave` has no Square**: Pulse at `pw` 50 % is the square (the same odd harmonics), so `pw` defaults
  to 50 %. One option fewer on the front selector, and the same first two options as gfx-vocoder.
- **Engine switching**: `engine` crossfades over 30 ms (the old engine's voice releases). `bank`, `wave`,
  `octave`, `type`, `poles` crossfade over 20 ms (structural changes, gfx-autowah's rule).

### 3.2 Front panel

| id | Name | Type | Range | Default | EXP | Role |
|---|---|---|---|---|---|---|
| `engine` | Engine | e | Spectral · Mono | Spectral | | Spectral: per-band synthesis, polyphonic, instant, no tracking. Mono: one voice, glide, exact bends, bridged by Spectral. (Poly: appended later, §3.1) |
| `wave` | Wave | e | Saw · Pulse · Triangle · Sine | Saw | | The oscillators (voices) and the per-band harmonic law (Spectral). Pulse at `pw` 50 % = square |
| `cutoff` | Cutoff | f | 40 – 12,000 Hz | 3,000 Hz | ✓ | Filter base frequency (golem's UI: a log knob; heel-to-toe EXP sweep) |
| `reso` | Resonance | f | 0 – 100 % | 20 % | ✓ | Filter peak: Q 0.5 … 20 (+26 dB at 100 %); no self-oscillation |
| `filt_env` | Env Amount | f | −48 – +48 st | +24 st | ✓ | How far the filter envelope moves the cutoff, up or down |
| `synth` | Synth | f | −60 – +12 dB | 0 dB | ✓ | Synth level; −60 = off |
| `dry` | Dry | f | −60 – 0 dB | −60 dB | ✓ | Guitar level, untouched; −60 = off (gfx-sustainer's id and range) |

### 3.3 Expert

| id | Name | Type | Range | Default | EXP | Role |
|---|---|---|---|---|---|---|
| `pw` | Pulse Width | f | 5 – 95 % | 50 % | ✓ | `wave` Pulse; 50 % = square |
| `octave` | Octave | i | −2 – +2 | 0 | | Transposes the synth (Spectral: z² / √z per band, gfx-octaver Poly's quality) |
| `sub` | Sub | f | 0 – 100 % | 0 % | ✓ | Octave-below square (voices) or sine (core, bands < 400 Hz) |
| `detune` | Detune | f | 0 – 50 ¢ | 0 ¢ | ✓ | Second oscillator (voices) or second copy (core) this many cents sharp: unison width |
| `noise` | Noise | f | 0 – 100 % | 0 % | | Breath shaped by the note (voices) or by each band (core) |
| `amp_mode` | Amp Mode | e | Follow · Gate | Follow | | Follow: the string's level (through `compress`). Gate: ADSR, held while the note sounds |
| `attack` | Attack | f | 0 – 2,000 ms | 2 ms | ✓ | Amp attack (Gate); minimum rise in Follow (swells) |
| `decay` | Decay | f | 5 – 4,000 ms | 400 ms | | Gate only |
| `sustain` | Sustain | f | 0 – 100 % | 70 % | | Gate only: level held after the decay |
| `release` | Release | f | 5 – 4,000 ms | 200 ms | | After the note ends (both modes) |
| `compress` | Hold | f | 0 – 100 % | 40 % | ✓ | How much of the string's decay is flattened, per band or per voice; 100 % = held until the floor (organ sustain) |
| `type` | Filter Type | e | Low-pass · Band-pass · High-pass | Low-pass | | `vcf` mode |
| `poles` | Poles | e | 2 · 4 | 4 | | 12 / 24 dB per octave |
| `drive` | Drive | f | 0 – 100 % | 0 % | ✓ | Saturation into the filter (contract id; 0 – 24 dB into the tanh); thickens resonance |
| `key_track` | Key Track | f | 0 – 100 % | 0 % | | Cutoff follows the note (voices; Spectral has no note) |
| `fenv_att` | Env Attack | f | 0 – 2,000 ms | 0 ms | | Filter envelope rise |
| `fenv_dec` | Env Decay | f | 5 – 4,000 ms | 300 ms | ✓ | Filter envelope fall to the base cutoff |
| `fenv_vel` | Env Velocity | f | 0 – 100 % | 50 % | | How much the pick strength scales `filt_env` |
| `bank` | Resolution | e | Dual · Fast · Clean | Dual | | Spectral core. Dual: fast on attacks, legato and bends, clean on held notes. Fast: tightest, chords gritty. Clean: cleanest chords; slow attack, late and shallow vibrato: pads (§2.6) |
| `residue` | Residue | f | 0 – 100 % | 100 % | | Mono (and Poly): level of the core where the voice does not own the band (the attack, a chord's other notes). 0 = the voice only: silent until read |
| `glide` | Glide | f | 0 – 1,000 ms | 0 ms | ✓ | Legato note changes (Mono) |
| `quantize` | Pitch | e | Off · Chromatic | Off | | Off: bends and vibrato pass. Chromatic: nearest semitone, held with hysteresis (voices) |
| `lowest` | Lowest Note | f | 30 – 150 Hz | 70 Hz | | Detector and bank floor: 70 Hz guitar, 35 Hz bass. Higher = faster |
| `thresh` | Noise Floor | f | −96 – −30 dB | −66 dB | | Below it no voice starts and no band sounds (6 dB of hysteresis): touch noise and hum stay silent |
| `spread` | Spread | f | 0 – 100 % | 0 % | | 2 / 2 only: the unison pair panned apart |
| `bypass` | Bypass | b | off / on | off | | `make_bypass` |
| `trails` | Trails | b | off / on | on | | On: bypass releases the synth over `release` |

Held back with Poly (not declared in v1): `voices` (i, 1 – 6, default 6), meter `voices_on` (0 – 6).

Read-only meters: `state` (0 silence · 1 core only · 2 voice sounding), `note` (the voice's tracked f0 in
Hz; 0 = none or Spectral).

Every user parameter is neutral or a plain synth starting point at its default; nothing depends on
`engine`. golem pins its own values at insert.

## 4. Applications

Clean DI, first in the lane (before any drive: every maker's advice, and the bands need clean
partials). Neck pickup helps the tracked voice (a stronger fundamental; gfx-tuner's advice).

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Synth stabs on chords (80s funk / pop) | Spectral, Pulse 50 %, Gate (A 2, D 180, S 0, R 120 ms), `cutoff` 600 Hz, `filt_env` +30 st, `fenv_dec` 200 ms, `reso` 40 % | Instant, polyphonic; fast strumming stays on the fast bank (tight, a little gritty) |
| Pad behind the band | Spectral, `bank` Clean, Saw, `detune` 12 ¢, `attack` 600 ms, `release` 1.5 s, `compress` 80 %, `cutoff` 1.5 kHz, `dry` −12 dB | Clean bank (its lag hidden by the slow attack), held level, unison |
| Brass section | Spectral, Saw, `attack` 30 ms, `filt_env` +36 st, `fenv_att` 40 ms, `fenv_dec` 350 ms | One filter envelope per pick (paraphonic). Per-voice envelopes and `key_track` come with Poly |
| Organ-like chords | Spectral, Pulse 50 %, `compress` 100 %, `amp_mode` Gate (A 5, S 100 %), `sub` 30 %, `cutoff` 5 kHz, `reso` 0 | Per-band hold: no decay, no hiss |
| Synth-bass from guitar | Mono, Saw, `octave` −1, `cutoff` 400 Hz, `filt_env` +24 st, `fenv_dec` 250 ms, `glide` 30 ms | Exact pitch, octave guard, glide |
| Bass guitar synth | Mono, Pulse 50 %, `lowest` 35 Hz, `sub` 50 %, `cutoff` 800 Hz | Detector and bank floor for bass |
| Squelchy lead with bends | Mono, Pulse 50 %, `reso` 70 %, `filt_env` +30 st, `fenv_vel` 80 %, `glide` 60 ms, `quantize` Off, `residue` 0 % | Phase-locked bends |
| Robot / stepped lead | Mono, Pulse 25 %, `quantize` Chromatic | Hysteresis: vibrato does not trill |
| Bending chord pads (what the Enzo's Poly cannot) | Spectral, `bank` Dual, Saw, `detune` 8 ¢, `compress` 60 % | Every band follows its own partial; a bending band goes back to the fast bank |
| Fuzz-synth / 8-bit | Spectral, `bank` Fast, Pulse 50 %, `compress` 100 %, `cutoff` 12 kHz, `reso` 0 | Per-band squares, tight attack |
| Guitar + synth layer | either engine, `dry` 0 dB, `synth` −6 dB | The dry untouched and in time |
| Swell strings (violin-like) | Spectral, Saw, `attack` 300 ms, Follow, `cutoff` 2.5 kHz, `detune` 6 ¢ | Attack as the minimum rise in Follow |
| Sci-fi sweep (EXP) | either engine, EXP on `cutoff`, `reso` 60 % | 7-bit EXP smoothed (contract rule) |

**Preset ideas** (`presets/gfx-polysynth.json`, no artist or trademark names): `chord-stab`,
`square-pad`, `wide-saw-pad`, `brass-section`, `organ-hold`, `guitar-bass-synth`, `bass-square-sub`,
`squelch-lead`, `robot-lead`, `bending-pad`, `eight-bit`, `layer-saw`, `swell-strings`, `sweep-exp`.

## 5. Bench suite (on top of gfx-bench's common checks)

Synthetic strings as §1.3 (generated in the test), plus the maintainer's DIs (lead-clean.wav, funk.wav, strums,
a bass DI). Every check at 44.1 / 48 / 96 / 192 kHz. Rows marked (Poly) are that engine's entry exam, run
on a bench build; they are not v1's.

| Check | Pass |
|---|---|
| Latency | 0 reported; dry impulse at sample 0 at every `dry` setting |
| Band design | Clean / fast banks: lag at the centre N / (π B) within 5 % of §2.2's figures; the same centres, widths and lags at the four sample rates (fixed bank rate) |
| **Timbre against pitch** | Saw, a sine glided over an octave from 82, 220 and 660 Hz, each bank: every output harmonic 1 – 9 under fcap stays within ±1 dB of c_n (the drafted shaper: 5 – 26 dB). `wave` Sine: the sweep flat ±1 dB |
| Chord cleanliness (the "grumble") | Pulse 50 %, `bank` Clean, the §1.3 metric on the body: E5, open E, open C, A minor barre ≤ −30 dB; E4 + G♯4 ≤ −40 dB; E2 + G♯2 ≤ −20 dB; and ≥ 8 dB under a full-band fuzz at equal output on every resolved chord (thresholds from §1.3's final-constants run, 3 dB of margin; to tighten once the DIs are measured). Dual's body (after 150 ms) the same. Fast: not worse than the fuzz (reported) |
| Attack | Single notes E2 … E5, Dual and Fast: synth output within 3 dB of its body level ≤ 20 ms after the pick; no swell over +3 dB in the first 150 ms (the f̂ smoothing) |
| Handover | Single notes and the five chords, Dual: no dip deeper than 1.5 dB in the 0 – 200 ms envelope (10 ms RMS), and none deeper than 3 dB on any of harmonics 1 – 5 of a single note |
| Legato | Hammer-ons, pull-offs and slides without a pick (DI and synthetic, level step 0, pitch step 2 – 5 semitones), Dual: the new note within 3 dB of its body ≤ 30 ms after the change |
| Vibrato | ±30 ¢, 6 Hz on E4 and A2, Dual: the synth's fundamental follows with ≤ 20 ms of lag and ≥ 85 % of the depth; held chord with one string in vibrato: the other strings' bands stay on the clean bank |
| Purity | Two partials 60 Hz apart in one band: output non-harmonic energy ≤ that of the band unshaped + 3 dB |
| Aliasing | Saw, `octave` 0 and +2, a 1 – 2 kHz glide; and two partials 40 Hz apart at 1.9 kHz: nothing above −70 dBc folds below 8 kHz |
| Hold | `compress` 100 %: a decaying E2 (T60 3 s) holds each partial within 1.5 dB until −10 dB above `thresh`; silence and touch noise under `thresh` stay under −90 dBFS |
| Level bound | −6 dBFS chord, `compress` 100 %, `reso` 100 % swept by EXP, `detune` on, `synth` +12 dB: output never above 0 dBFS (the ceiling), no click; the ceiling idle (bit-transparent) under −6 dBFS |
| No octave errors (Mono) | Lone E2, A2, D3 (bright and neck tones): no voice at the octave or the 12th, ever; no octave flip > 40 ms on lead-clean.wav |
| Bridge (Mono) | No gap > 3 dB, no level step > 1.5 dB and no click (common step-click bound) when the voice takes over; envelopes not restarted (filter sweep continuous); `residue` 0 is silent until the reading; the owned core bands ≤ −30 dB after the handover |
| Bends (Mono) | A 200 ¢ bend in 150 ms and a ±30 ¢ 6 Hz vibrato on E4 and G3: voice frequency within 10 ¢ RMS of the string; within 3 ¢ 50 ms after the bend stops |
| Note end | Mute (palm): the voice in release ≤ 60 ms; nothing chirps (frequency held in release, gfx-sustainer's rule) |
| `vcf` null | gfx-autowah built on `vcf` against its current build: bit-identical on its bench renders (every `type` × `poles`, `drive` 0 and > 0) |
| Engine switch | Each pair, under a ringing chord: no click, ≤ 30 ms crossfade |
| EXP | 50 Hz-stepped `cutoff` sweep 200 Hz → 5 kHz: control-rate sidebands ≤ −80 dB |
| Determinism | Strict null between two renders; L = R on equal inputs at `spread` 0 |
| CPU | ≤ 8 % of a core, worst case (Dual, strums, unison, Mono voice) at 48 kHz, and no more at 96 / 192 kHz; report each engine |
| Naming (Poly) | Synthetic and DI chords: uncovered notes voiced; time from the last onset ≤ 150 ms for notes ≥ 200 Hz, ≤ 400 ms for low notes and chords. **Wrong voices** (a note neither played nor covered) ≤ 1 per 100 strums on the DIs, and none on a single note |
| Bridge (Poly) | As Mono's, per voice; a chord with one bent string (open G, the B string bent 100 ¢): the other voices stay within 3 ¢ [I] |
| Voice count (Poly) | Seven named notes (two guitars' worth): 6 voices, the 7th audible through the core |
| Listening | `gfx-bench apps gfx-polysynth`: one render per Applications row; blind A/B of Spectral against a full-band fuzz + filter at equal level on chords; the f̂ smoothing at 10 and 30 ms; the bridge on lead-clean.wav; Poly (bench build) on held chords before any decision to ship it |

gfx-bench options: default step-click bound; silence floor −90 dBFS (the release tails of 4 s at
`release` maximum are the effect itself, as gfx-delay).

**Fallback**, if the bench says so:
- **The local handover events (new energy, pitch motion) chatter or misfire**: Dual keeps the pick
  event only and its Role says "picked playing"; legato and vibrato players use `bank` Fast.
- **The handover dips on single harmonics**: the clean band's phasor is snapped onto the fast one's
  (§2.2); if that fails too, `bank` default stays Dual with a longer crossfade (to 10 τ).
- **CPU over budget**: §2.6's three reductions.
- **Poly never passes its entry exam**: it is never appended; nothing in v1 depends on it.

## 6. Positioning

- Against the **Boss SY** and **EHX 9** pedals: the same instant, polyphonic, tracking-free core, with
  a measured answer to their chord "grumble" on held notes (a narrow, steep bank, one band per partial,
  a purity gate), a held level (`compress`), a real filter with its envelope, and a tracked mono voice.
  Not claimed: a cleaner attack on chords (the first 60 – 120 ms are the fast bank's).
- Against the **Meris Enzo**: Mono at least as good (phase-locked bends, octave guard, bridge on the
  attack). Its Poly is answered in v1 by Spectral, which bends and has no tracking errors but is
  paraphonic (one filter envelope); a per-voice Poly is the research item.
- Against **MIDI Guitar** and hex systems: no MIDI out and no external synth in v1 (see question 4);
  their polyphonic naming is exactly what the review's prototype could not make safe.
- Not a pitch shifter (gfx-pitch), an octaver (gfx-octaver keeps its OC-2 / Clean / Synth / Poly), or a
  vocoder (gfx-vocoder).

## Visual

- **Category `pitch`** (decided): the pedal hears the notes and makes new tones on them, like gfx-octaver
  and gfx-sustainer; golem offers `pitch` on guitar and bass lanes. A `synth` category would hold one
  pedal today; it is golem-rack's to open if a second guitar synth ever exists.
- **Crate hue `#ff9ecf`, neon rose** (`METAL_CRATE_HUE['gfx-polysynth']`), decided at review: the pink of
  an 80s synth's lettering, and a light tint of its own category's magenta.
  - The draft's phosphor mint `#20e0b0` was checked against modulation, filter, tuner and ice, but not
    against **gfx-cab's `#00a898`: ΔE\*ab 27** (ΔE00 16), with the modulation green at 31 (ΔE00 18) [P,
    CIELAB D65]. gfx-tape's review refused 26 and the suite's accepted hues sit at 33 or more. Brighter
    mints do no better (`#40f0c0` 31 / 35, `#7dffd6` 32 / 43): the green-cyan arc between modulation,
    cab, filter and glitch is full.
  - `#ff9ecf` [P]: ΔE\*ab **42** from raspberry `#e8336f`, 43 from periwinkle `#a0a8ff`, 44 from the pitch
    magenta `#c4148c`, more from every other hue in use (ΔE00 23 / 26 / 29). The other free-looking
    arcs are not free: corals fall 16 – 27 from the drive orange and copper, lemons 17 – 22 from brass,
    sky blues 19 – 21 from the filter cyan.
  - It is a pale body (L\* 76), like champagne and ice: `RUNE_LIGHT['gfx-polysynth'] = '#8a1060'`
    (4.75 : 1 on the body [P]).
- **Finish**: anodized (the family's).
- **Motif `saws`** (new case in `motifParts`, drawn with `line()`, so engraved on anodized and yellow
  lined in black on thrash): *a chord turned into oscillators*.
  - Two fields in the rune band, left of the cartouche (x from 6 to cx − cw / 2 − 6) and right of it
    (x from cx + cw / 2 + 6 to W − 6), y from cy − ch to cy + ch, clear of the knob grid (gfx-freqshift's
    review lesson).
  - In each field three sawtooth rows at y = cy − ch / 2, cy, cy + ch / 2, amplitude ch / 6, periods in
    the ratio 6 : 5 : 4 from bottom to top (a major triad: the higher note, the shorter period), each tooth
    a ramp then a vertical drop.
  - Opacity .45, .7, 1 from bottom to top. The left field's rows start with one sine cycle (the string)
    before the teeth; the right field's are teeth only.
  - Distinct from `waves` (sines), `pulse` (square edges), `stairs`, `sidebands` (a modulated sine),
    `bins` (bars), `modes` (lenses).
- **Face** (`visuals/data/faces.json`):
  `"gfx-polysynth": {"name": "Synth", "blocks": ["polysynth"], "knobs": ["cutoff", "reso", "filt_env", "synth", "dry"], "selectors": ["engine", "wave"], "category": "pitch"}`.
  Five knobs, two selectors, no toggle. "Synth" is a plain word.
- **Glyph** `visuals/glyphs/polysynth.svg` (24 × 24, `fill="none" stroke="currentColor" stroke-width="1.8"`,
  round caps and joins, title "Guitar synth (gfx)"): **three strings, each turning into a sawtooth** —
  one sine cycle (faint), then two teeth.

  ```
  <path d='M2 5Q4 0 6 5T10 5M2 12Q4 7 6 12T10 12M2 19Q4 14 6 19T10 19' opacity='.45'/>
  <path d='M10 5L14 2.5V7.5L18 2.5V7.5L22 2.5V7.5M10 12L14 9.5V14.5L18 9.5V14.5L22 9.5V14.5M10 19L14 16.5V21.5L18 16.5V21.5L22 16.5V21.5'/>
  ```

  Rows 7 px apart, each ±2.5 px [A: the quadratic's peak is half its control offset]; the drops at x 14
  and 18 are 4 px apart (2.2 px of air at 1.8 px of stroke). Distinct from `vital` (one wave over a line),
  `octaver` (circles and an arrow), `vibrato` / `chorus` (sines), `sustainer` (lenses).
- No maker's livery, logo or trademark name on the pedal.

## Questions settled at review

Answered at the review of 2026-11-14 (details in the sections named and in the Review):

1. **Category** → `pitch`, with a crate hue. No `synth` category for one pedal (Visual).
2. **Poly in v1 or not** → **not in v1.** Spectral + Mono is an acceptable v1: it covers the Boss / EHX
   role (polyphonic, instant) and the Enzo's Mono. `engine` = Spectral · Mono; Poly is appended when it
   passes its rows of §5 (§3.1).
3. **`multipitch` extensions** → agreed in §2.3: `max_notes` as a `prepare` argument (gfx-sustainer
   passes 4, results unchanged), `early()` as a research accessor that never starts a sound. `track`
   unchanged.
4. **MIDI / note output** → out of v1, and it waits for Poly's naming anyway. Mono could send one note
   with bend at no extra risk: a golem-rack decision (routing), not a design one.
5. **Arpeggiator** → later, as its own enum parameter on the voices (it needs named notes: Mono's one
   note, or Poly's), not as an `engine` option: an engine says where the sound comes from.
6. **`vcf`** → SVF only in v1, the code gfx-autowah has shipped. A ZDF ladder can be appended to this
   pedal's `type` later (ranks are per plugin; gfx-autowah's fourth option is Wah).
7. **Defaults by ear** → stay open for the maintainer: `compress` 40 %, `cutoff` 3 kHz, `filt_env` +24 st, the purity
   gate's 0.25 B, the handover's 3.5 τ → 7.5 τ, the selection's 6 dB and 3 / 50 ms, f̂'s 10 ms.
8. **Synthetic strings only** → stays open: every [A] and [P] figure must be rerun on DIs (neck and
   bridge pickup, a real pick) before §5's thresholds are frozen.
9. **Crate hue** → `#ff9ecf`; the mint was 27 from gfx-cab's teal (Visual).

## Sources

- Boss SY-200: [Sound On Sound review](https://www.soundonsound.com/node/4927168), [MusicRadar](https://musicradar.com/reviews/boss-sy-200), [Boss](https://www.boss.info/products/sy-200/), [Synthtopia (firmware)](https://synthtopia.com/?p=126036)
- Boss SY-1: [Sound On Sound review](https://www.soundonsound.com/reviews/boss-sy-1)
- EHX Synth9: [Sound On Sound review](https://www.soundonsound.com/reviews/electro-harmonix-synth-9), [EHX Mel9](https://ehx.com/products/mel9), [MusicPlayer forum on Mel9](https://forums.musicplayer.com/topic/161476-mel9-tape-replay-pedal)
- Meris Enzo: [Sound On Sound review](https://www.soundonsound.com/reviews/meris-enzo), [Vintage King review](https://vintageking.com/blog/meris-enzo-review/), [Elektronauts thread](https://www.elektronauts.com/t/meris-enzo-x/229247?page=2)
- Jam Origin: [technology page](https://www.jamorigin.com/technology/), [MIDI Guitar 3 beta](https://jamorigin.com/beta), [SoS review](https://www.soundonsound.com/reviews/jam-origin-midi-guitar), [jazzguitar.be thread](https://www.jazzguitar.be/forum/guitar-amps-gizmos/49391-best-midi-guitar-tracking-ive-found.html)
- Hex systems: [The Gear Forum, GM-800 / GK-5](https://thegearforum.com/threads/boss-gm-800-and-gk-5.2825/page-57), [SoS GR-55](https://www.soundonsound.com/reviews/roland-gr55)
- Schramm, Visi, Brasil, Johann, NIME 2018: [PDF](https://zenodo.org/records/1302651/files/nime2018_paper0027.pdf)
- Klapuri, ISMIR 2006 (harmonic summation) [I]; Derrien, DAFx-14, very low latency (mono) pitch tracking: [PDF](https://www.dafx14.fau.de/papers/dafx14_olivier_derrien_a_very_low_latency_pitch_.pdf)
- golem-fx: gfx-sustainer §2.2 / §2.7 and its Review (`track`, `multipitch`), gfx-tuner §2.5 (collision
  table), gfx-octaver (Poly bank), gfx-vocoder §2.2 / §2.5 / §2.9 (`filterbank`, `osc`), gfx-autowah (filter
  path), gfx-ringmod (Track carrier), `docs/golem-contract.md`, `docs/design-review.md`.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good for v1 (Spectral + Mono), with one defect of the draft corrected; Poly at risk and held
back.** The thesis is right and is the market's own: the audio-domain core is the instrument, tracked
voices are a layer. The survey holds (the Sound On Sound SY-1 and Enzo quotes and the NIME 2018 figures
were re-read; the catalogue has 79 entries and no guitar synth). The designer's prototype was re-run
(its tables reproduce) and extended: scripts in `proto-gfx-polysynth/` of the review scratchpad (`lib.py`
the banks and shapers as specified; `e1` – `e9` the Spectral core; `e10`, `e12` Poly's start rule; `e11`
crest factor and ownership; `hue.py`). Synthetic strings only, as the draft's.

### Measured [P] (24 kHz banks exactly as §2.2, strings as §1.3)

| Question | Result |
|---|---|
| Do the Spectral claims reproduce? | The draft's own scripts: yes (−41 … −57 dB clean, −16 … −24 dB fuzz). But they shaped bands up to 1.2 kHz and kept linear bands to 6 kHz. As §2.2 specifies: −34 … −45 dB on chords (E4 + G♯4 −42, not −57), E2 + G♯2 −29 dB; the fast bank −14 … −21 dB, i.e. the fuzz |
| Timbre against pitch, drafted shaper | Each output harmonic's level swings **5 – 26 dB** as a sine crosses a few bands (both banks): neighbouring bands read one partial with phases 115 – 191° apart, times n. The off-harmonic metric cannot see it; a bend would |
| Fixed rotations alone / phase from the instantaneous frequency | 4 – 7 dB of swing left / flat to 0.4 dB but chords 10 – 15 dB dirtier |
| **One band per partial** (selection + design rotation + skirt gain) | ≤ 0.5 dB (clean), ≤ 1.0 dB (fast); chords −37 … −49 dB clean and −22 … −29 dB fast (30 ms constants), −33 … −47 dB with the final constants |
| Does the fast → clean crossfade hide the 64 ms? | For picked notes, yes: within 2 dB of the body at 20 ms, no dip over 1.5 dB through a 3.5 τ → 7.5 τ crossfade, with no phase rotation. Clean alone on the low E: −6 dB at ≈ 60 ms, −1.5 dB at 80 – 100 ms. Not for legato or vibrato: see change 3 |
| Attack swell | With the skirt-gain frequency smoothed over 30 ms: +5 … +11 dB between 40 and 120 ms after a pick; over 10 ms: none |
| Waveform | Re(uⁿ) with real 1 / n weights is not a saw: crest 10.2 dB against 5.7 dB; square 7.3 against 1.6 dB |
| Core left under a named E2 | Drafted mask (±⅙ ERB) and shaper: −9 dB. One band per partial: −41 dB |
| Poly's start rule ("two agreeing readings" of the early bank + fundamental present) | **4.4 wrong voices per strum**; a 60 ms hold 0.7; a 120 ms hold 0.18 and misses begin. Target 0.01 |

### Changes made, and why

| # | Change | Why |
|---|---|---|
| 1 | **v1 = Spectral + Mono; `engine` = Spectral · Mono**; Poly, `voices`, `voices_on` held back; Poly's bench rows became its entry exam | Last row above. An enum option cannot be withdrawn but can be appended, so v1 must not declare what has not passed. Applications and Positioning rewritten for v1 |
| 2 | **Shaper: one band per partial** (level selection, design rotations ρ_b, skirt gain); G_Σ and edge trims dropped; bench row "Timbre against pitch" | Rows 2 – 4. Without it every bend and vibrato combs the harmonics |
| 3 | **Handover re-specified**: scheduled from each band's τ (3.5 → 7.5 τ), started per band by a pick, by new energy or by pitch motion; the 5 ms rotation of the sounding fast phasor removed; bench rows "Legato" and "Vibrato" | The draft's "when the clean level reaches the fast one, at the latest 80 ms" cut in at −2.6 dB; a pick-only trigger leaves hammer-ons 60 – 100 ms late and a vibrato 64 ms late at half depth; the rotation was a phase glide of n · π |
| 4 | **Waveform coefficients complex** (sine-phase series for saw and triangle); **`wave` = Saw · Pulse · Triangle · Sine, `pw` default 50 %** | Row 7; Square is Pulse at 50 %, and the first two options now match gfx-vocoder's `wave` |
| 5 | **Bank rate fixed at 22.05 – 24 kHz** at every host rate (was fs / 2, fs / 4 from 176.4 kHz) | At 96 kHz the draft ran the banks at 48 kHz: twice the CPU and another fcap, so another sound |
| 6 | Bank widths stated against `GammaBank`'s convention (B = 2 b); `filterbank` takes an explicit band list | The two documents used "width" for a pole's full width and for its half-width; these banks are outside `GammaBank`'s w0 range on purpose |
| 7 | **Poly's acceptance**: confirmed FFT readings only (≥ 128 ms), `early()` never starts a sound; **`multipitch` API agreed** (§2.3) | Last row; one interface before either crate is coded |
| 8 | **Bridge**: ownership width justified by measurement; **level-matching rule** at the handover; Mono's fade 10 ms and oscillator started on YIN's f0; the timbre change stated; note-end rule | Row 8; the draft had no rule for the voice's level against the core's, and Mono's voice would have waited the tracker's five periods (61 ms at E2) |
| 9 | **`vcf` corrected to gfx-autowah's code**: drive ahead of the filter (not in its loop), no self-oscillation, 4 poles = Q 0.707 then Q, Q capped at 20, `drive` % ↔ 0 – 24 dB; **bit-identical null as the condition of the move** | The code was read; gfx-autowah is shipped |
| 10 | **Ceiling** on the synth bus; bench row "Level bound" | `compress` +30 dB, `reso` +26 dB and unison had no bound |
| 11 | **CPU budget ≤ 8 % for v1** (≤ 10 % with Poly), the same at every rate; the vocoder scaling and the `atan2` rate stated; reductions ordered | The contract's 20 % is for circuits. The draft's 15 % had no basis in the contract |
| 12 | Latency table: measured rise times beside the band lags; "what the 64 ms means for the player" | The brief's question; N / (π B) is a group delay, not what the player waits for |
| 13 | Shared ids listed with what is and is not the same (`release`, `glide`, `dry` default, `wave`, `type`); `synth` + `dry` accepted | `glide` was said to be "gfx-octaver's, widened" while gfx-vocoder claims that range; a default differing across pedals is allowed, a meaning is not |
| 14 | **Hue `#ff9ecf`** (was `#20e0b0`), `RUNE_LIGHT` `#8a1060`; category `pitch` | The mint is ΔE\*ab 27 from gfx-cab's teal, which the draft did not compare |
| 15 | Bench thresholds taken from the measured figures; `vcf` null, CPU at every rate, Mono's rows separated from Poly's | Each corrected claim has its check |

Checked and left as written: the survey and its sources; the lag formula N / (π B) and the band counts
(59 and 40); the purity gate (its limit under ~30 Hz added); `compress`'s law (gfx-stretch's `compress`
has the same form); the octave and sub paths (gfx-octaver's); the filter envelope and its velocity; EXP
flags; zero reported latency; linear ranges; no mode-dependent default; `bypass` / `trails`; the motif
`saws`, the face and the glyph.

Coverage against the references: Boss SY and EHX 9 (instant polyphonic waveforms, filter, held level:
covered; their preset count and organ / Mellotron voicings are presets, not features); Enzo (Mono
covered; Poly answered by Spectral; Arp left out, question 5; its pitch-shifted Dry mode is gfx-pitch).
Left out on purpose: MIDI out, a ladder filter, self-oscillation, an LFO (golem's EXP and the
modulation pedals after it do that job; a `cutoff` LFO is the first thing to add if presets ask).

### Still open

| Point | Owner |
|---|---|
| **Spectral core in gfx-bench on the DIs** before anything is frozen: selection constants, f̂ smoothing (10 or 30 ms), the two local handover events (not prototyped), per-harmonic dips in the crossfade | golem-fx, first task |
| Open E reads −38 dB with one band per partial against −44 dB drafted (close partial pairs, E2 × 5 against G♯3 × 2): tune the selection's release, or accept | Bench |
| CPU: 8 % of a core for one pedal, the dearest non-circuit effect of the suite | The maintainer, after the measurement |
| Poly: a naming method that reaches ≤ 1 wrong voice per 100 strums (FFT readings alone are unmeasured here; the semitone bank is not it), then the listening test of its 0.13 – 0.4 s handover | golem-fx research, with `multipitch` |
| **gfx-sustainer to align** (its README is not edited here): `multipitch::Analyzer::prepare(sample_rate, f_min, max_notes)`, passing 4; `notes()` = the `max_notes` most salient confirmed notes, sorted by pitch, and a 4-note caller unaffected by a 6-note build; `early()` reserved; `track` unchanged. `OCTAVE_CONFIRM_MS` moves from gfx-golem into `detect` only under gfx-golem's null | golem-fx |
| gfx-vocoder to note: `filterbank` gains an explicit (centre, width, order) design beside `GammaBank`; widths there are half-widths | golem-fx |
| gfx-autowah on `vcf`: only if bit-identical; otherwise it keeps its code | golem-fx |
| Defaults by ear (question 7); `residue` default 100 % in Mono (a chord's other notes stay on the core) or 0 % (a classic mono synth) | The maintainer's ear |
| Catalogue entry `polysynth` (category `pitch`), block presets, `faces.json`, glyph, motif `saws`, hue `#ff9ecf` on the family sheet next to raspberry and the pitch magenta; note output to `vital` (question 4) | golem-rack |

## As coded (v0.1.0, 2026-11-15)

**Files.**
- gfx-dsp `vcf.rs` (new): `Vcf` (drive → SVF × 1 / × 2, Low-pass · Band-pass · High-pass, gfx-autowah's
  operation order and types), `VcfType`, `q_of`. 2 unit tests, one of them the null against a copy of
  gfx-autowah's inline path (every type × poles, drive 0 / 9 / 24 dB, swept and held cut-off, 44.1 and
  96 kHz: bit-identical). gfx-autowah itself is untouched (its move onto `vcf` stays its own null-guarded
  change).
- gfx-dsp `osc.rs` (added only): `pulse_width` (any width, mean removed), `poly_blamp`, `triangle`. gfx-dsp
  `filterbank.rs` (added only): `band_gain`, `crossover`, `design_rotation`. One unit test each file.
- `src/spectral.rs`: the core (banks, analysis tick, selection, skirt, shaper, purity, octave / sub / detune
  / noise, hold and floor, handover with its events and snap, ownership mask, the bridge's measures).
  **Moved 2026-11-26 (gfx-organ §2.8):** the analysis (banks, tick, selection, skirt, purity, hold and
  floor, handover, ownership mask) is now gfx-dsp `spectral`, generic over a per-band `Shaper` (static
  dispatch); this file keeps the waveform shaper (`WaveShaper`: series, octave / sub / detune / noise, the
  bridge's power and phase) and a `Core` wrapper with the old API. Guarded by `spectral_null_against_the_build_before_the_move`
  (14 settings × up to 3 rates, both engines: byte-identical); the core alone A / B against the old file
  in one binary, 7 rounds each: −2.2 … −7.5 % (no slower); `cpu_report` 968 – 1052 ns / sample at defaults
  after, 941 – 1080 before.
  `src/lib.rs`: the processor (`Polysynth`, `PARAMS`, labels), detection, Mono's voices and bridge,
  envelopes, the drive stage, `vcf` with structural crossfades, the ceiling, bypass. `src/plugin.rs`: the
  VST3, class id `GoleMFxPolysyn01`, "GoleM FX Polysynth", mono 1 / 1 first then 2 / 2, latency 0, `bypass`
  = `make_bypass`, read-only meters `state` and `note`. `src/tests.rs`: 17 behaviour tests (§5) + the fast
  math's test in spectral.rs + `cpu_report` (ignored).
- gfx-bench: `EFFECTS`, `make()`, `options()`: silence floor −90 dBFS (§5); latency check skipped (the wet is
  the pedal's own waveforms, uncorrelated with the input; the dry-impulse row is the crate's test); `drive`
  and `cutoff` steps exempt from the step check (below).
- `presets/gfx-polysynth.json`: the 14 preset ideas of §4 (defaults: `chord-stab` guitar, `bass-square-sub`
  bass). Visuals: face "Synth" (cutoff, reso, filt_env, synth, dry; engine, wave), groups, help, depends,
  glyph `polysynth`, motif `saws`, crate hue `#ff9ecf`, rune light `#8a1060`, anodized.

**Null proof.** gfx-dsp's existing code is unchanged (additions only). gfx-vocoder (default, Drone saw,
supersaw stereo), gfx-sustainer (default, Sustain + All), gfx-ringmod (default, Track) and gfx-autowah
(default, band-pass + drive + 4 poles) rendered on a synthetic guitar file with the base and the new
gfx-bench: byte-identical WAVs; their tests stay green (16 / 23 / 23).

**Measured** (48 kHz, synthetic strings as §1.3; `plain` = the filter open and still, Follow without rise,
`compress` 0, unless stated).

| §5 row | Bound | Measured |
|---|---|---|
| Latency | 0; the dry's impulse at sample 0 | 0 reported; the dry at sample 0 at `dry` 0 / −12 / −40 dB, both engines |
| Band design | lag within 5 % of N / (π B) and of §2.2 | clean 63.6 ms (81, 321 Hz), 29.1 ms (1 kHz), 15.8 ms (2 kHz); fast 15.9 / 9.5 / 5.3 ms; the same bands (bank rate 22.05 / 24 kHz) at 44.1 – 192 kHz; 61 clean bands (60 from 40 Hz) and 41 fast (40) |
| Timbre against pitch | every harmonic within ±1 dB of c_n (drafted: 5 – 26 dB) | Saw, a sine stepped over an octave from 82, 220, 660 Hz: clean ≤ 0.04 dB, fast ≤ 0.76 dB (swing ≤ 0.89 dB, at 82 Hz); `wave` Sine flat −0.04 … 0 dB from 82 Hz to 1.3 kHz |
| Chord cleanliness (Pulse 50 %) | Clean and Dual: E5 / open E / open C / Am barre ≤ −30, E4 + G♯4 ≤ −40, E2 + G♯2 ≤ −20 dB; ≥ 8 dB under the fuzz | clean / Dual (two seeds): E5 −45.5 … −46.4; open E −32.9 … −33.6; open C −34.9 … −36.3; Am −42.2 … −45.1; E4 + G♯4 −46.0 … −46.4; E2 + G♯2 −20.0 … −20.4 (at the bound). Fuzz −17.0 … −23.6. Fast −19.6 … −28.1 (not worse than the fuzz) |
| Attack | ≥ −3 dB of the body at 20 ms, no swell over +3 dB | A2 … E5: −2.6 … +0.6 dB (Dual and Fast), swells ≤ +2.2 dB. **E2: −3.7 dB (Fast), −4.1 dB (Dual)** (10 ms RMS centred at 20 ms; an order-2 band 40 Hz wide is at −2.9 dB 20 ms after a step) |
| Handover (Dual) | no dip over 1.5 dB in 0 – 200 ms | E2 1.32, A2 0.05, E4 0.33, E5 0.76 dB; E3 / B3 none |
| Legato (Dual) | new note within 3 dB ≤ 30 ms | E4 → A4 without a pick: 13.9 ms (Fast 11.5, Clean 69.3) |
| Vibrato (Dual) | ≤ 20 ms lag, ≥ 85 % depth; others stay clean | E4 13.9 ms / 97 %, A2 15.7 ms / 91 % (Clean 47 – 56 ms); A2 + E3 + C4 with C4 in vibrato: the other strings' bands at x = 1 throughout |
| Purity | shaped ≤ unshaped + 3 dB | two partials 60 Hz apart in one fast band (1.85 / 1.91 kHz): Saw −23.1, Pulse −23.3, unshaped −23.4 dB. Reported: the same pair at 1 kHz falls in two fast bands (each holds the other 13 dB down, purity ≈ 0.55): Saw −15.6, Pulse −19.5, unshaped −33.0 dB |
| Aliasing | nothing above −70 dBc folds under 8 kHz | Saw, `octave` 0 and +2, notes at 1.0 / 1.5 / 1.9 kHz and a pair 40 Hz apart at 1.9 kHz: worst −74.2 dBc (window skirts; no fold found) |
| Hold | `compress` 100 %: partials within 1.5 dB while 10 dB over `thresh` | E2 T60 3 s, partials 1 – 4: within 0.15 dB until the +30 dB cap or the floor; −80 dBFS noise under `thresh`: synth silent (−240 dBFS) |
| Level bound | never over 0 dBFS; the ceiling transparent under −6 dBFS | −6 dBFS chord, `compress` 100, `reso` 100 swept, `detune` 20, `drive` 50, `synth` +12: peak 0.00 dBFS; bit-transparent under 0.5 |
| No octave errors (Mono) | never the octave or the 12th | E2 / A2 / D3 / E4: voice after 25 / 22 / 22 / 11 ms, within 13.5 / 8.7 / 7.0 / 1.7 ¢ of the note from its first sample |
| Bridge (Mono) | no gap > 3 dB, no step > 1.5 dB; owned bands ≤ −30 dB; `residue` 0 silent until the reading | E4: gap 0.12, step 0.60 dB; A2: gap 2.65, step 1.09 dB; owned core bands at −240 dB; `residue` 0: exactly 0 before the voice |
| Bends (Mono) | ≤ 10 ¢ RMS; ≤ 3 ¢ 50 ms after | 200 ¢ in 150 ms: 2.8 ¢ RMS, ≤ 0.77 ¢ after; ±30 ¢ 6 Hz: 3.9 – 4.0 ¢ RMS (E4, G3) |
| Note end (Mono) | release ≤ 60 ms after a mute, no chirp | 49 ms; frequency held within 0.000 ¢ |
| Chords (Mono) | the lowest note, ≤ 400 ms | E5 power chord: E2 (+0 ¢) after 171 ms |
| Engine switch, structural changes | no click | engine, wave, octave, bank, type, poles, amp_mode under a held note: worst −59 dB (step measure) |
| Determinism | strict null; L = R at `spread` 0 | bit-identical renders, both engines; L = R = mono at `spread` 0, L ≠ R at 100 % |
| CPU | ≤ 8 % of a core worst case | bench line (pink noise: every band busy, Dual, defaults) 1,487 – 1,621 ns per sample mono (7.1 – 7.8 %), 1,528 – 1,673 ns stereo; strummed open E: Spectral 1,016 – 1,076 ns (4.9 – 5.2 %), with detune / sub / noise 1,133 – 1,201 ns (5.4 – 5.8 %), Mono 1,142 – 1,209 ns (5.5 – 5.8 %), Mono + detune 1,286 – 1,394 ns (6.2 – 6.7 %); the bank rate is fixed, so 96 / 192 kHz cost the same core plus the half-band cascade. Measured on a shared machine (load average 4 – 6): ±10 % |

Not run: the DI rows (lead-clean.wav, funk.wav, strums, a bass DI), the EXP sideband row (the EXP
parameters run on `exp_smooth`, tested there), the listening row, Poly's rows.

**Deviations from the design, and why.**
- **Skirt clamp ±1.2 half-spacings**, the review prototype's own (`lib.py`: ±0.6 of the spacing); §2.2's "±0.6
  of the half-spacing (at most +5 dB)" leaves a 7 dB dip at each clean crossover. At most +13.8 dB (clean).
- **Handover events**: new energy at **6 dB** (not 3) and only while the clean band rises (1 dB over its
  own 5 ms mean); pitch motion asks a pure fast band (w > 0.9) on a clean local maximum, reads the fast
  band's frequency through a further 15 ms, and once started is **held while the banks stay 6 ¢ apart**
  (40 ms of agreement ends it). As specified, the open E's 40 Hz-wide fast bands (a neighbour partial
  13 dB down wobbles their frequency ±10 – 20 ¢) put chord bands on the fast bank for good (Dual −17.6 dB on
  the E5 against −46.3 clean), and the A2 vibrato let the clean bank back between crossings (91 % / 15.7 ms
  now, 84 % / 23 ms before, with chirps at each crossfade).
- **The crossfade snap** (§2.2's fallback) is used: at the start of a band's fade-in its phasor is turned
  onto the fast band's (only from a pure one), and **held** instead of relaxed at 5 ¢: the relaxation
  shifted each band by up to 5 ¢ for seconds, independently, and cost 1 – 12 dB of chord cleanliness. Without
  the snap, E2's crossfade dipped 1.8 dB (bound 1.5).
- **Hold**: P_b is the peak since the last pick, held, and a pick lets it go to the band's level (§2.2's
  "release 3 s" lost 6.7 dB in 2 s of a T60 3 s E2, the Hold row allows 1.5).
- **Grid**: both grids hang from 2 kHz downwards to 24 Hz; a band under 0.8 × `lowest` is faded out (20 ms):
  `lowest` never redesigns a bank, so no bank crossfade is needed. The lowest band is the first grid centre
  at or above 0.8 × `lowest` (within ⅓ ERB of it).
- **Control rate** 16 bank samples (0.67 ms) with deterministic fast log₂ / 2^x / atan2 (≤ 2e-6, 1e-7,
  1.2e-5 rad): CPU. The frequency estimate is the rotation over the tick, demodulated by the centre (§2.6).
- **Detune**: the copy's rotor advances by δ × the band's smoothed frequency, not by arg(u_t ū_(t−1)) per
  sample (no atan2 per band per sample); the copy fades in over the first 2 ¢.
- **`octave` −1 / −2** take the branch-tracked √u (⁴√u) on **every** band, not only under 400 Hz (a note
  above 400 Hz would be silent). `sub` stays under 400 Hz.
- **`noise`**: each sounding band's phasor times a complex one-pole band of noise (corner max(B, f / 4)), not
  a separate noise bank.
- **Mono's voice**: the tracker engages with **Snap** (the voice runs its own oscillator at the tracker's
  frequency, smoothed by two 3 ms one-poles: the loop's increment jitters ±10 – 80 ¢ sample to sample on a
  D3); Keep left the voice 5 ¢ off for half a second after each note (bend row: 5 ¢ against 3). The
  oscillator starts on the core band's phase but is not relaxed onto the string's (inaudible, the frequency
  is locked).
- **Bridge level**: the core bands' power from their envelopes (½ amp², attack 2 / release 20 ms); the voice
  follows that level for max(20 ms, 3 periods), then k is frozen and Follow tracks the string (a 10 ms mean
  of y² lagged the attack by 3 dB; a peak follower on y² overshot by 2 dB).
- **Mono's start**: two clear YIN readings within 50 ¢ (the octave guard extended to the first reading); a
  chord: `multipitch`'s lowest confirmed note, YIN's reading after 400 ms. A pick releases the voice
  (frequency held) and the core plays the new attack. One YIN detector, 30 – 1,500 Hz (`set_range` to
  `lowest`), not gfx-tuner's long window under 60 Hz; the 40 ms octave guard is in the crate.
  `multipitch` is prepared for 6 notes; only the lowest is used.
- **Note end**: under `thresh`, or 24 dB under the last 60 ms maximum (after the pick's first 80 ms), or the
  tracker's level under `thresh` − 6 dB.
- **`drive`** runs in the crate ahead of `vcf` (called with 0 dB) so that it can fade in over its first
  3 dB, the input moved to the ADAA's half-sample timing as it does: gfx-autowah's switch at 0 dB stepped by
  8 %.
- **gfx-bench step exemptions** (the effect, not clicks): `drive` 0 ↔ 100 % (on the way the tanh's
  small-signal gain k_d / √k_d lifts the resonant synth over −6 dBFS and the ceiling soft-clips it: −34 dB of
  HF re peak, −63 dB at `reso` 0) and `cutoff` 40 Hz ↔ 12 kHz at 48 kHz (the resonance sweeps across the
  synth's own 9 – 10 kHz harmonics, which sit at the check's 10 kHz corner: −36 dB; −90 dB at 44.1 kHz,
  ≤ −54 dB at 96 – 192 kHz, −141 dB at `reso` 0). Every other step ≤ −50 dB.
- **`fenv_att`** is taken when the envelope is triggered (a change mid-attack jumped the cut-off: −23 dB of
  HF at the step check).
- **Spectral, `amp_mode` Follow**: `release` has no effect (the core holds no tail of its own); it acts in
  Gate (both engines) and on Mono's voice. Gate in Spectral is held while the input is over `thresh` and
  not muted; a note that rises without a pick (a swell) triggers it.
- **Attack row, E2**: −3.7 dB (Fast) / −4.1 dB (Dual) at 20 ms against the bound's −3 (an order-2, 40 Hz band
  is itself at −2.9 dB 20 ms after a step); A2 and above pass.
- **Purity row**: held in one band; two partials in neighbouring fast bands are shaped almost fully
  (w ≈ 0.55 at 0.25 B). A gate at 0.1 B passes that case (−26.7 dB) but costs 2.6 dB of timbre swing on
  low single notes in Fast; the constant is one of question 7's "defaults by ear".
- **Ceiling**: the ADAA quotient is clamped to ±1 (rounding reached 1 + 5e-14).

**Left for the owner.** The DI rows (neck and bridge pickups, a real pick, a bass DI) before any threshold
is frozen; by ear (question 7): `compress` 40 %, `cutoff` 3 kHz, `filt_env` +24 st, the purity gate's
0.25 B, the handover's 3.5 τ → 7.5 τ and its event thresholds (6 dB, 15 / 6 ¢), the selection's 6 dB and
3 / 50 ms, f̂'s 10 ms; `residue` 100 % or 0 % in Mono; the CPU (the dearest non-circuit pedal of the suite);
gfx-autowah's move onto `vcf` (its null); golem-rack's catalogue entry `polysynth` (category `pitch`), the
glyph and the hue on the family sheet.
