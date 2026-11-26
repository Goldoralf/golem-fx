# gfx-organ

> **Status: DESIGNED 2026-11-26** (Claude Opus 5.5), **reviewed 2026-11-26** (Claude Fable 5.2: the
> Review at the end). Not coded. Roadmap 0.8, item 2. Working name `gfx-organ`, displayed **"GoleM FX
> Organ"**. This directory holds this document and a numpy prototype (`proto/`), nothing else: no crate
> yet. The prototype's figures (§1.4) come from synthetic strings and golem's two clean guitar DIs.
> Synthetic strings: every drawbar within 0.8 dB of its target on single notes E2 – A4. The review
> rewrote the root sieve (§2.3) after measuring the draft's on inputs with a noise floor. With the
> reviewed sieve: no ghost root on single notes, ≤ −39 dB in the attack of six chords, every note of an
> arpeggio sounds. **The registration arrives 100 – 170 ms after the pick** (the draft said 64 ms; that
> figure was computed, not measured). The sieve on real DIs is the first task before any code (§5).
>
> **Decided by Raph, 2026-11-26.** The bloom (registration 100 – 170 ms after the pick, the 8' at 20 ms)
> is accepted for now, to be judged by ear at coding. Raph records annotated DIs (chords, arpeggios, fast
> lines, notes known); proving the root sieve on the coded core on them is the first organ task. As the
> review recommends: `mod` as depth, 3 dB drawbar steps (the law settled by ear on the bench, then
> frozen), "Rotor" for `leslie`, foldback under C3 kept, the CPU budget stated per rate.

golem's **guitar organ**: a polyphonic tonewheel organ played from a normal guitar or bass DI (no hex
pickup, no MIDI). It has nine drawbars, percussion, key click, the scanner vibrato / chorus and a Leslie.
It plays the role of the EHX B9 / C9 Organ Machine and of the organ patches of the Boss SY-1 / SY-200
(names used as sound references only: none appears in a displayed name or a parameter). golem already
plays a MIDI organ (`organ`, setBfree, category `keys`). This pedal is played from the guitar's audio.

**The idea in one line.** gfx-polysynth's Spectral core splits the guitar so that each band holds one
partial, and reads that partial's phase u = e^{jθ}. Every drawbar footage is a power of u: 16' = u^½,
5⅓' = u^{3/2}, 8' = u, 4' = u², 2⅔' = u³, 2' = u⁴, 1⅗' = u⁵, 1⅓' = u⁶, 1' = u⁸. The fifths and the
third (5⅓', 2⅔', 1⅗', 1⅓') need **no pitch tracking**: they are integer and half-integer powers of a
phase. What the B9 cannot do is apply them **to each note's fundamental only**. A **root sieve** (§2.3)
finds the bands that are not an overtone of a lower sounding band. Only those receive the registration.
So each string becomes one organ note with the drawbars' own spectrum, instead of the guitar's spectrum
copied nine times.

Zero reported latency. The inherent lags (§2.6) are the polysynth core's, plus the sieve's
confirmation for footages other than 8': 100 – 170 ms in the model [P].

It is a **new golem block** (`guitar-organ`, proposed: `organ` is taken by setBfree). Its expert ids are
the `organ` block's own (`db16` … `db1`, `perc`, `perc_harm`, `perc_decay`, `vibrato`, `leslie`), so a
registration names the same drawbars on both organs. The balance between two drawbars at different steps
is not the same: this pedal steps by 3 dB, setBfree linearly (§1.2, open question 6).

| Pedal | What it makes from the guitar | Polyphonic | Pitch source | Latency |
|---|---|---|---|---|
| gfx-octaver Poly | octaves of every partial (z² / \|z\|, √z per ERB band) | yes | none (per band) | 0 (band lag) |
| gfx-polysynth Spectral | a waveform per partial, filter, envelopes | yes | none (per band) | 0 (band lag) |
| gfx-polysynth, preset `organ-hold` | a pulse wave per partial, held level | yes | none | 0 |
| **gfx-organ** | **a registration per note (roots only), percussion, click, scanner, Leslie** | **yes** | **none: per-band phase, plus a root sieve on band frequencies** | **0** (§2.6) |

## 1. State of the art

Survey of 2026-11-26. Tags as the other designs: **[M]** manufacturer's page or manual · **[V]** read in
a manual, a paper or source code · **[F]** review or forum post · **[A]** computed here (`proto/`,
§1.4) · **[I]** inference · **[U]** claimed, not verified. Reddit, TheGearPage, Gearspace and Harmony
Central refused automated reading on the survey day. Forum lines from them come from search summaries
and are tagged [F/U].

### 1.1 The B9 / C9 and their rivals

| Reference | How it makes the sound | What players report | What we take |
|---|---|---|---|
| **EHX B9 Organ Machine** (2014) [M] | "Digital", mono in, two outputs (Organ, buffered Dry), 1 MΩ in [M]. Sound On Sound (P. White): "based on the same type of pitch-shifting and wave-shaping technology used in Electro-Harmonix's HOG"; the sound comes from the strings themselves; he suspects pitch detection steers the filters [F]. Audiofanzine says it "triggers notes" (no evidence given) [F]. EHX: C9 "based on the sound engine found in the B9", Key9 "based on the B9 and C9", Mel9 "the same technology" [M]. Literal POG code: [U] | Manual: first in the chain, bridge pickup best, no drive before it ("will muddy… tracking unstable"); nothing above the 23rd fret; "sloppy guitar playing will sound like a sloppy organ player"; "Organs do not bend" [M]. SoS: "surprisingly believable", but "occasional tonal warbling" on chords with a semitone (major 7ths) [F]. Premier Guitar: uneven response, "a threshold that once crossed boost[s] the output considerably", Jazz too bright for a dark B3 [F]. Audiofanzine: "the organ lagged a little behind", uneven dynamics [F]. No speed footswitch or expression input for the modulation (SoS, PG) [F]. Forum summaries: compressor first, single notes better than strums, "think like an organist" [F/U] | An audio-domain engine is the reference sound. Its faults are what §2 targets: warble on close intervals (intermodulation), level thresholds, and no Leslie speed by foot |
| B9 presets [M, manual] | 1 Fat & Full ("an extra octave below and above", chorus) · 2 Jazz (Jimmy Smith, chorus) · 3 Gospel ("upper octave drawbars added") · 4 Classic Rock ("Whiter Shade of Pale… a touch of distortion") · 5 Bottom End (B3 bass sounds) · 6 Octaves (fundamental + octave; CLICK adds key click and higher harmonics) · 7 Cathedral (tremolo; CLICK = tremolo depth) · 8 Continental (combo organ, vibrato; CLICK = vibrato depth) · 9 Bell Organ (EP × organ, tremolo; CLICK = bell amount). **MOD = modulation speed, off fully left; its type depends on the preset. The manual never says "rotary"** [M]. **CLICK = percussion**, single-trigger: "added to the very first note or chord… will retrigger only when current notes have been released and their amplitude falls below a threshold" [M] | — | Nine registers as front-panel identity; Mod and Click on the face; single-trigger percussion as an option (§2.4) |
| EHX C9 [M] | Tone Wheel · Prog (CLICK = a fifth above and an octave below) · Compact (Light My Fire) · Shimmer (MOD = attack) · Lord Purple (two sets of 3 drawbars on ORGAN and CLICK) · Mello Flutes · Blimp · Press Tone · Telstar (Clavioline). On bass it "tracks best above… C3"; the Mel9 goes down to A1 [M] | SoS: on full chords "some of the artificially generated harmonics drop out before others" as the strings decay [F] | The per-partial decay fault: our hold (§2.4) holds each root to its body level |
| EHX Key9 (organ preset 8, CTRL2 = rotating speaker speed), POG2 (organ by mixing −1 / +1 / +2 octaves; no organ preset), EarthQuaker Organizer (POG-style octave mixer, "Choir", Tone; discontinued) [M] | Octave mixers: every partial shifted, so the guitar's spectrum is copied at each octave | — | The "all partials" law, measured as the baseline in §1.4 |
| Boss SY-1 / SY-200 organ types [M] | Organ sounds "including many with rotary-style modulation", "latency-free"; SoS (SY-1): processing the strings' signal, not tracking [F] | SoS: organs "not as convincing as… B9 and C9"; some patches carry a vibrato that cannot be turned off [F] | Modulation must be switchable and set per register |
| Mooer Mooergan (micro) [F] | Three voices, Organ and Dry levels, one Mod knob; method undocumented [U] | Fewer options than the larger units [F] | — |
| Line 6 Helix, Meris Enzo [F] | No organ model; Leslie models; users build POG-style "organ" patches. Enzo: synth only | — | Nothing in that class does registrations |
| Pigtronix, Joyo, Donner, Valeton, Caline, Quad Cortex [U] | No guitar-organ product found | — | — |

**How the B9 works, verified as far as sources go.** No source documents it. The two Sound On Sound
reviews say "pitch-shifting and filtering" of the strings' own signal, related to the HOG. Its manual's
faults match an audio-domain engine, not tracked oscillators: warble on semitones, harmonics dropping out
one by one, a fret ceiling, "bends pass through" [I]. A tracked synth would quantise bends and misname
chords instead. This design therefore treats the B9 as **octave and harmonic generation on the
polyphonic signal, plus per-preset filters and envelopes**, and does not claim more. Latency: no
millisecond figure is published by anyone [U]; SoS heard none, Audiofanzine heard "a little".

### 1.2 The tonewheel organ itself

| Fact | Source | What we take |
|---|---|---|
| Footages 16' 5⅓' 8' 4' 2⅔' 2' 1⅗' 1⅓' 1' = harmonics 1, 3, 2, 4, 6, 8, 10, 12, 16 of the 16' sub-octave, i.e. 0.5, 1.5, 1, 2, 3, 4, 5, 6, 8 × the played note | SoS, G. Reid, *Synthesizing Tonewheel Organs* [V] | The nine powers of u |
| Drawbar steps 0 – 8, "1 the softest, 8 the loudest", no dB given by Hammond [M]. "≈ 3 dB per step" is third-party (Vorkoetter) [F]; a reader says ≈ 3 dB at the top, ≈ 6 dB near the bottom [F/U]; setBfree uses linear steps (step / 8) [V] | Hammond PDF, stefanv.com, setBfree | **3 dB per step** (8 = 0 dB, 1 = −21 dB, 0 = off): the prototype's law, kept by the review. setBfree's law was read in its source (`tonegen.c`: `drawBarLevel[bus][setting] = setting / 8.0`): steps 8 … 1 = 0, −1.2, −2.5, −4.1, −6.0, −8.5, −12.0, −18.1 dB. The two agree at 8 and at 0 only; at step 4 this pedal is 6 dB under golem's MIDI organ, at step 6 3.5 dB under. Registrations made of 8s and 0s (six of the nine registers) sound alike on both; the others do not. Open question 6 |
| **Tonewheel temperament**: f = 20 rev/s × teeth × gear ratio; the drawbars get 2⅔' = 2.99765 (−1.4 ¢), **1⅗' = 5.04094 (+14.1 ¢: a tempered major third, not the 5th harmonic)**, 1⅓' = 5.99529 (−1.4 ¢), 5⅓' = 1.49882 (−1.4 ¢) | Electric Druid [V] | `tuning` Tonewheel (default) / Pure. The +14 ¢ third's beating is part of the full-organ sound |
| **Foldback**: on the lowest 12 keys the 16' draws on wheels 13 – 24, the same octave as their 8': the manual's 16' never goes under wheel 13 (C, 65.4 Hz). The upper footages repeat the top octave (top wheel ≈ 5.9 kHz). 91 wheels serve 109 tones | Electric Druid [V] | **A footage under 65.4 Hz plays an octave higher, one over 5.9 kHz an octave lower** (§2.2). On a guitar's low E (82 Hz) the 16' (41 Hz) folds to 82 Hz, as on a B-3's lowest octave. On a bass it keeps the organ out of the sub-bass |
| Waveform: near-sine wheels (SoS); the lowest octave more complex [V]. Crosstalk ≈ −40 dB (setBfree's defaults) [V] | SoS, Electric Druid, setBfree `default.cfg` | Sines; leakage not modelled in v1 (open question 7) |
| **Percussion**: upper manual, 2nd (4') or 3rd (2⅔') harmonic; **single trigger** ("no further percussion effects occur until all keys… are released"); percussion on disconnects the 1' drawbar; Normal mode lowers the organ (the "volume drop"), Soft −6 dB; Fast raises the gain ≈ 50 % | Hammond service manual [V] | `perc_*` ids as golem's organ; the 1' mute and the volume drop kept (Custom), §2.4 |
| Percussion decay: setBfree fast 1.0 s, slow 4.0 s **to −60 dB**; Normal scales the drawbars × 0.605 (−4.4 dB), Soft 0.501 (−6 dB). No hardware figure found [U] | setBfree `default.cfg` [V] | τ = T60 / 6.91: 145 ms / 579 ms |
| **Key click**: 9 bronze contact springs on 9 busbars, closing at slightly different moments and switching mid-cycle; setBfree: "several random contact bounces" per busbar, bursts of 0.1 – 0.5 × 5.87 ms; Pekonen et al. model it as a 6th harmonic decaying in 70 ms | Service manual, setBfree, DAFx-11 [V] | Click = the registration gated by 9 deterministic bounce envelopes, plus a short burst (§2.4) |
| **Chorus-vibrato scanner**: 19-stage LC line (≈ 0.85 ms), a rotating capacitor crossfading adjacent taps at ≈ 6 – 7 Hz; V1 / V2 / V3 reach taps 9 / 13 / 19; chorus = vibrato + dry | Werner, Dunkel & Germain, DAFx-16; setBfree [V] | gfx-vibrato's coded Scanner model (taps 55 µs apart, V1 → V3), moved into gfx-dsp (§2.5) |
| **Leslie**: horn 48 / 400 rpm, drum 40 / 342 rpm, 800 Hz crossover; setBfree: horn τ 0.161 / 0.321 s, drum 4.127 / 1.371 s | HammondWiki, Wikipedia, setBfree `whirl.c` [V] | gfx-rotary, coded with exactly these constants (§2.5) |
| Registrations: Hammond's own sheet: jazz "88 8000 000", "88 8000 008"; pop ballad "80 0000 008". SoS: 88 8888 888 "very full and bright", 80 0000 000 near sine, 00 8030 200 near square. M. Finnigan (MusicRadar): 888800000 Smith-type (4' perc for solos), 668848588 gospel "Shoutin'", 803600000 mellow comping, 888000008 all-round | [M], [V], [F] | The nine registers (§2.7) |
| Vox Continental: top-octave oscillators + dividers, drawbars 16' 8' 4' and a IV mixture, flute and reed voices, single-speed vibrato. Farfisa Compact: tabs 16' / 8' / 4' (flute, oboe, trumpet, strings), 4 vibrato settings | Wikipedia [V] | The "Combo" register: bright, square-ish, scanner vibrato, no Leslie |

### 1.3 What players love and hate

- **Love**: one box, instant organ on chords (B9: "surprisingly believable"); Gospel, Cathedral and
  Continental are the favourites (SoS) [F]; the Leslie or chorus built in.
- **Chord warble** on close intervals (B9, SoS) and the SY-1's chord "grumble" (gfx-polysynth §1.2) [F]:
  intermodulation in a shared nonlinearity. → One band per partial (polysynth core), and the registration
  on roots only (§2.3). Measured in §1.4.
- **Uneven dynamics, a level threshold that jumps** (PG, Audiofanzine) [F]. An organ has no velocity. →
  A held body level with `touch` (how much the pick still matters), and gate hysteresis (§2.4).
- **Harmonics dropping out one by one as the chord decays** (C9, SoS) [F]. → Per-root hold until the
  floor. Each root's registration is generated from one band, so its drawbars fall together.
- **"Too bright for a dark B3"** (PG on Jazz) [F]: the guitar's upper partials pass into the organ. →
  The roots law: the drawbars' own spectrum, not the guitar's. `strings` adds the guitar back on purpose.
- **No Leslie speed by foot** (SoS, PG) [F]. → `leslie` Stop · Slow · Fast is on the face and is golem's
  organ footswitch id.
- **Low strings, bass** (C9 on bass "above C3") [M]. → Bank floor `lowest` (35 Hz for bass) and
  foldback (a bass's E1 does not make a 20 Hz 16').
- **Needs clean signal, first in chain, compressor helps** (every maker) [M]. → The same advice. The
  hold removes the need for a compressor.
- **Bends**: "Organs do not bend" (B9 manual) [M]. → The registration follows a bend on the clean
  bank's lag, and the 8' follows it within 10 – 16 ms (§2.6). We do not quantise.

### 1.4 What the prototype shows [A]

`proto/organ.py` (the model), `proto/run_synth.py` (synthetic strings), `proto/run_di.py` (DIs). The
bank is gfx-polysynth's clean bank exactly: order 4, ⅓ ERB apart and wide, ≥ 20 Hz, 59 bands 40 Hz –
2 kHz, at 24 kHz. It uses one band per partial (selection, design rotations, skirt gain) and 16-sample
control ticks. The strings are polysynth §1.3's: |sin(π k p)| / k with p 0.08 – 0.25,
B = 10⁻⁴, T60 3 s falling with k, a 4 ms pick, random phases, 20 ms strum spread. Three seeds. **The
attack runs on the clean bank only.** The pedal plays it on the fast bank (Dual), which polysynth
measured at ±2 dB of its body 20 ms after the pick. Run under a 3 GB memory cap: about 2 min (synthetic)
and 30 s (DIs).

**(1) Registration accuracy on single notes** (E2 A2 D3 G3 B3 E4 A4, body 0.4 – 1.4 s). Level error per
drawbar after one best gain, and the energy outside the registration's lines (±15 ¢):

| Registration | **Roots** (sieve): RMS / worst / outside | All partials (POG-style): RMS / worst / outside |
|---|---|---|
| 88 8000 000 | 0.24 / 0.77 dB / **−39.8 dB** | 3.34 / 8.37 dB / −0.5 dB |
| 00 8000 000 | 0 / 0 dB / −39.0 dB | 0 / 0 dB / −0.2 dB (the guitar's own spectrum) |
| 88 8800 000 | 0.23 / 0.69 / −40.3 | 3.79 / 9.74 / −0.9 |
| 83 8000 000 | 0.24 / 0.77 / −40.1 | 8.00 / 18.1 / −0.9 |
| 80 8808 008 | 0.21 / 0.81 / −41.3 | 3.83 / 11.5 / −1.3 |
| 88 8888 888 | 0.24 / 0.81 / −41.5 | 4.20 / 11.9 / −1.9 |

- **Applying the drawbars to every partial does not make an organ.** Half the output lies off the
  registration's lines. One drawbar's level is off by up to 18 dB, because partial k's 8' lands on the
  fundamental's k-th footage with an arbitrary, note-dependent phase. That is the octave-mixer sound
  (POG, Organizer, and plausibly the B9's "too bright").
- **Roots only gives the registration within 0.8 dB**, with 40 dB of clean spectrum below it. The
  half-integer footages (u^½, u^{3/2} from the unwrapped band phase) are as exact as the integer ones.

**(2) Chords** (body 0.4 – 1.4 s; registrations 88 8000 000 and 00 8880 000). *Off-grid* = energy more
than 25 ¢ (or 3 FFT bins) from every multiple of f0 / 2 of a played note (of f0 for the integer
registration), widened upwards by the strings' stretch. *Ghost* = the output of root bands that are not a
played fundamental, re the total. The reference is a full-band harmonic generator: Chebyshev T_n of the
low-passed, envelope-normalised input, the "one nonlinearity" way.

| Chord | Off-grid, roots (888 / 00888) | All partials (888 / 00888) | Full-band T_n (00888) | Ghost roots, body / attack 0 – 150 ms | Missed / covered |
|---|---|---|---|---|---|
| E5 power (E2 B2 E3) | −38 … −42 / −37 … −44 dB | −23 … −26 / −27 … −30 | −5 … −8 | none / ≤ −26 dB | 0 / 1 |
| Open E (E2 B2 E3 G♯3 B3 E4) | −20 … −36 / −20 … −37 | −22 … −27 / −27 … −30 | −6 … −7 | none / ≤ −27 | 0 / 3 |
| Open C (C3 E3 G3 C4 E4) | −34 … −40 / −31 … −35 | −25 … −27 / −24 … −28 | −5 | none / ≤ −48 | 0 / 2 |
| A minor barre (A2 E3 A3 C4 E4) | −29 … −46 / −29 … −49 | −25 … −35 / −29 … −36 | −4 … −5 | ≤ −28 / ≤ −25 | 0 / 2 |
| E4 + G♯4 | −45 … −46 / −51 … −56 | −22 … −26 / −23 … −30 | −4 | none / ≤ −34 | 0 / 0 |
| E2 + G♯2 (21 Hz apart, unresolved) | −28 … −35 / −22 … −29 | −8 … −10 / −5 … −6 | −4 … −5 | ≤ −26 / ≤ −15 | 0 / 0 |

- **No ghost note on the body, no missed note.** Every played note is a root, or is *covered*: it is an
  integer multiple of a lower played note, like E3, B3 and E4 over E2 in the open E. A covered string
  has no fundamental of its own in the spectrum (gfx-polysynth §1.3, gfx-tuner §2.5). It sounds as the
  lower root's 4', 2⅔' and 2' when the registration has them. That limit is stated in §2.3, not hidden.
  The 5th harmonic (+14 ¢ from a tempered third) stays a separate root: G♯3 sounds in the open E.
- **Intermodulation floor**: the roots law is 15 – 50 dB cleaner than a full-band generator on every
  resolved chord. It is up to 20 dB cleaner than the all-partials law on thirds and power chords. Its
  worst seed (open E, −20 dB) has a band that holds two partials 4 Hz apart (B3 = 2 × B2 = 3 × E2).
- **The attack is where ghosts live** (−15 … −48 dB in the first 150 ms). During the first tens of
  milliseconds the clean bank is smeared, and an overtone looks like a root. Hence the **confirmation**
  (§2.3): a band becomes a root after 4 τ_b as one. Until then it plays its own partial as a sine, which
  is a harmonic of the note, never a ghost. Swept: confirmation at 1 / 2 / 3 / 4 / 5 τ_b leaves the
  worst attack ghost (resolved chords) at −9 / −12 / −17 / −24 / −26 dB and the body at −16 / −18 / −22 /
  −27 / −31 dB. 4 τ_b is taken.
- An earlier version of the model let a band fall back to "undecided" whenever its purity dipped. In
  the open E the B3 band beats, and that toggling made 5.5 Hz sidebands at −20 dB. The rule now is that
  a band is undecided only during its first 2 × 4 τ_b, after which it is an overtone unless confirmed.

**(3) Envelope and percussion** (E3, 00 8000 000 + 3rd harmonic, τ 0.2 s). The input's fundamental falls
30 dB between 0.3 and 1.8 s. The organ's 8' holds −23.2 … −23.3 dB over the same span. The
percussion is at −10.6 dB re the 8' at 0.3 s, −23.7 at 0.6 s and −41 at 1.0 s: it decays by
8.7 dB per τ, as designed. Its source includes the undecided bands, so it speaks with the pick and not
at the confirmation.

**(4) Rise** (clean bank only, 00 8000 000 and 88 8000 000): −3 dB of the body 33 / 23 / 24 / 18 ms after
the pick on E2 / A2 / E3 / E4. On the fast bank (the pedal's Dual) the 8' is polysynth's: within 2 dB at
20 ms. The 16' and 5⅓' arrive at the confirmation: 4 τ_b = 64 ms on bands under 330 Hz, 29 ms at 1 kHz,
plus a 10 ms fade. *Review: this figure was computed from the rule, not measured. Measured in (6):
101 – 169 ms.*

**(5) golem's DIs** (`~/golem/library/di-refs/guitar/`, read and never copied; first 12 s). There is no
ground truth for a DI, so the script reports what can be checked without one. Renders (organ alone and
organ + dry, 48 kHz) go to the run's output directory for listening.

| Take | Roots within ±30 ¢ of the 12-TET grid (tuning offset removed) | Roots per sounding frame (median / 95 % / max) | Level-weighted pitch classes |
|---|---|---|---|
| `clean` (chords in E, 20 onsets) | **97.1 %** (offset −10.6 ¢) | 3 / 4 / 8 | B 27, E 22, G 17, D 16, A 14 % (C 2 %, F 0 %) |
| `lead-clean` (single-note lead, bends) | 88.4 % (offset −13.6 ¢) | 1 / 3 / 7 | A♯ 24, D 20, D♯ 18, C 16, F 9 % |

- On real chords 97 % of root time sits on the semitone grid, where a ghost from intermodulation or
  leakage would land anywhere. The pitch classes are the take's E-minor / G / D / A harmony.
- The lead take is played a semitone down. Its classes (A♯ D D♯ C F) are E major's B D♯ E C♯ F♯ minus
  one semitone [I]. Its 12 % off-grid is bends and vibrato, which an organ follows (§2.6).
- **The max of 8 roots on a six-string take is a fault to fix on the bench.** Attack frames with
  intermittent extra roots are the candidates. The bench row "Ghost roots on DIs" bounds it.

**(6) Review measurements [P]** (`proto/sieve_review.py`, `proto/run_review.py`; Fable 5.2,
2026-11-26). Tables (1) – (4) reproduce to the last digit. But every input above starts from **digital
silence**, and the model then reads a band's phase as 0 before the note: the jump to the band's
frequency keeps its purity under 0.5 for about 90 ms, which blanks the sieve through the attack. No
real input does that. With a noise floor of −100 dBFS added (far under any DI's), same strings:

| What | Draft sieve (§2.3 as designed) | Reviewed sieve (§2.3 as it now stands) |
|---|---|---|
| How fast a clean band reads its partial: f̂ error 40 / 64 / 90 / 120 / 150 / 200 ms after the pick (worst of 4 seeds) | E2 −86 / −29 / −14 / −7 / −3 / −1 ¢ · A2 −105 / −60 / −38 / −17 / −2 / +1 ¢ · E3 +62 / +31 / +20 / +10 / +3 / 0 ¢ · E4 −40 / −26 / −16 / −4 / +2 / 0 ¢ | the same core |
| Single notes E2 … A4: output of root bands that are not the fundamental, 70 – 200 ms after the pick (re the organ's output) | worst −4.2 … −9.7 dB on six notes of seven (median −5.8 … −14.3); up to **10 roots on one A2** | none |
| Same, 200 – 450 ms | −12.9 … −69 dB | none |
| Single notes: the fundamental becomes a root | **101 – 169 ms** after the pick | 101 – 169 ms |
| Six chords, 70 – 200 ms (worst of 3 seeds): E5 / open E / open C / A minor barre / E4 + G♯4 / E2 + G♯2 | −17.2 / −11.9 / −26.0 / −8.5 / −6.7 / −21.1 dB | none / none / −54.3 / none / −39.0 / none |
| Same chords, body 0.45 – 1.4 s | none except A minor −26.8 dB, E2 + G♯2 −57 dB | none except A minor −26.8 dB, E2 + G♯2 −67.5 dB |
| Chords: each note that is not covered becomes a root | 119 – 178 ms (G3 of open C 251, E4 of A minor 342) | 141 – 181 ms (the same two late notes) |
| Arpeggio of the open E, 0.3 s apart, strings ringing: notes that are a root after their pick | **3 of 6** (E3, B3, E4 never: they are covered by the ringing E2 and B2, and with 88 8000 000 make no sound at all) | 6 of 6 in 4 seeds of 4 |
| Melody E3, E4, B3 over a ringing E2 | 0 of 3 | E3 in 2 – 4 seeds of 4, E4 in 2 – 3, B3 in 4 |
| A bass E2 picked under a held E4 (once; three times) | the E4 **loses its registration** when the bass enters | kept in 4 seeds of 4 |
| Ghost-root output in those sequences, from 70 ms on (worst seed): arpeggio / melody / bass once / bass three times | −14.5 / −15.2 / −13.6 / −11.9 dB | −15.9 / none / −22.0 / −16.5 dB |
| One note picked five times (E2; A2) | −8.5; −5.0 dB | −13.2; −13.0 dB |
| Open E strummed three times | −14.7 dB (median −15.3) | −10.1 dB (median −18.4) |
| Octave dyad E3 + E4 picked together; an E4 strummed together with an E2 | E4 covered | E4 covered (unknowable: §2.3) |
| 200 ¢ bend of a G3 (sine), 80 0000 000: the 16' line | falls 22.9 dB, under −6 dB for 116 ms | lowest point −4.6 dB (the model's level ripple, the 8' has it too), never under −6 dB |
| Half powers: a sine anywhere between band centres (150 cases at 110, 200 and 600 Hz) | 16', 5⅓', 8' and 4' at the same level within 0.1 dB in every case | the same law |

- **The band's frequency is the limit, not the confirmation count.** At 4 τ_b (64 ms) a clean band still
  reads its partial 26 – 60 ¢ off; it is within the sieve's ±6 ¢ only after 120 – 150 ms. The draft's
  exact ratio test, run on those readings, lets overtones pass as roots through the attack, and the
  §1.4 (2) attack figures (−15 … −48 dB) only held because of the silence. The reviewed sieve widens
  the test by what the frequencies may still be off by (§2.3, step 3).
- **Covering must know which pick a band belongs to.** The draft covers any band that is a multiple of
  any lower sounding band, whenever. Over a ringing bass, every octave, twelfth and double octave played
  later is then silent in a registration without 4', 2⅔' or 2' (Trio, the default), and a held note
  is taken away when a bass note enters under it. §2.3, step 5.
- **What is left** in the reviewed model: ghost roots at −10 … −16 dB when a ringing string or chord is
  picked again; a held third that loses its registration when the bass under it is picked again (both
  sieves); the registration no earlier than 100 ms. These are the bench's first questions (§5), on the
  real core and real DIs: the model's selection is cruder than gfx-polysynth's coded one (its level
  ripple is ±4.6 dB where the coded core measured ≤ 0.5 dB).
- **On golem's two DIs** (12 s each, floors −54 and −74 dBFS; no ground truth): the reviewed sieve
  without its step 5 has at most 5 roots at once on both takes (draft: 8 and 7), and upper roots that
  are a multiple of a lower root for 0.4 % and 13.5 % of the pairs (draft: 1.3 % and 23.6 %). With
  step 5, `clean` goes to a median of 5 – 6 roots (draft 3): §2.3.
- A faster path was tried: confirm the lowest candidate after 2 τ_b. The fundamental is then a root at
  64 – 83 ms on six notes of seven (A2: 101 – 122 ms), at the price of ghosts in some attacks (A2
  −10.9 dB, A minor −16.1 dB, E5 −32.4 dB). Not adopted; kept as a switch in the prototype for the
  bench (open question 1).

**Conclusion.** The drawbars can be made exactly from the phase of each band. The fifths and the third
need no tracking: u^{3/2}, u³, u⁵, u⁶. The part that makes it an organ rather than an octave mixer is
the root sieve, which is cheap (band frequencies and one phasor per band at each pick). Its price is
time: the full registration arrives 100 – 170 ms after the pick [P], and the attack in between is the
strings' own partials as sines, plus percussion and click. That is the design, and its sieve is the
part that must pass on real DIs before the rest is coded.

## 2. Design

### 2.1 Signal path (mono core; the Leslie is stereo in the 2 / 2 layout)

```
in ──┬───────────────────────────────────────────────────────────────── dry gain (`dry`) ──────────────────────┐
     ├─ HPF 25 Hz ─ ↓ 24 kHz ─ Spectral core (gfx-dsp `spectral`, polysynth's): fast + clean banks,          │
     │                         one band per partial, handover ─ root sieve (§2.3) ─ organ law (§2.2):           │
     │                         drawbars on roots, partial sines on undecided bands, `strings` on overtones,    │
     │                         percussion (§2.4) ─ ↑ ─ hold / touch / swell (§2.4) ─ click (§2.4) ─┐          │
     └─ onset (gfx-dsp `onset`, detector A) ──────────────────────────────────────────────────────┘          │
        ─ scanner vibrato / chorus (§2.5) ─ Leslie: tube drive → rotors → mics (gfx-dsp `rotary`, §2.5)          │
        ─ `organ` gain ─ ceiling ─────────────────────────────────────────────────────────────────── sum ─ out
```

- The dry is untouched (one gain) and sample-aligned, as in gfx-polysynth.
- **Layouts: 1 / 1 first, 2 / 2 second.** The pedal sits first in a mono lane (every maker's advice, and
  the contract's mono-first rule). In 1 / 1 the Leslie's mics are summed exactly mono (gfx-rotary's
  `spacing` 0 path: L = R). In 2 / 2 the Leslie is stereo. This goes against the contract's
  "stereo-by-nature effects put 2 / 2 first", because the organ is a source and not a modulation
  effect: open question 3.
- A **ceiling** ends the organ bus before the sum, as gfx-polysynth's: transparent under −6 dBFS, an
  ADAA tanh above. A full registration (nine drawbars at 8 sum to 9 × the 8') plus percussion and drive
  can otherwise exceed the input by 20 dB.

### 2.2 The organ law (per band, in the core's shaper)

For band b with unit phasor u_b = e^{jθ_b} (θ_b unwrapped, design rotation included), body level G_b
(§2.4), selection s_b and root weight F_b (§2.3):

**y_b = s_b · G_b · [ F_b · Σ_f d_f · w_f · Re(u_b^{r_f} · e^{jφ_f}) + U_b · d_max · Re(u_b) + (1 − F_b − U_b) · σ · Re(u_b) ] + percussion**

- r_f = 0.5, 1.5, 1, 2, 3, 4, 5, 6, 8 (16' … 1'); d_f = 10^{−3 (8 − n_f) / 20} for drawbar position
  n_f ≥ 1, 0 when out (the 3 dB step: §1.2, open question 6).
- **Half powers** u^½, u^{3/2}: gfx-octaver Poly's branch-tracked √u (gfx-polysynth's `sqrt_branch`), then
  cubed for 5⅓'. Both use the same branch, so 16' and 5⅓' keep their phase relation. **Hand-over to the neighbouring band**
  happens when the selection moves a partial to the neighbouring band (a bend crosses a crossover). Two
  things travel with the partial:
  1. the √ branch: the neighbour's is chosen as the one nearest the sounding band's √ (after design
     rotation). Otherwise the 16' and 5⅓' may cancel through the crossfade (two copies π apart). The
     numpy model, which unwraps each band's phase on its own, showed no such cancellation in 150 static
     cases and one bend [P]; the rule stays, because nothing guarantees it;
  2. **the root's status** (§2.3: confirmed, the pick it belongs to, its age) and its held level P_b.
     Without it the new band starts as an undecided candidate and the registration drops out for a
     confirmation time at every crossover: measured on a 200 ¢ bend of a G3, the 16' fell 23 dB and
     stayed under −6 dB for 116 ms; with the status handed over, no drop beyond the model's own level
     ripple [P].
  Bench row "Bend across bands".
- **Tonewheel tuning** (`tuning` Tonewheel): φ_f is a rotor that runs the footage at the gear ratios'
  offset (−1.36 ¢ for 5⅓', 2⅔' and 1⅓'; +14.1 ¢ for 1⅗'). It is gfx-polysynth's detune rotor: its phase
  advances by δ_f × the band's smoothed frequency, with no atan2 per sample. Pure: φ_f = 0.
- **Foldback** w_f: a footage whose frequency r_f · f̂_b is under 65.4 Hz plays at 2 r_f. One over
  5.9 kHz plays at r_f / 2, or is dropped if that coincides with a lower footage already present. Each
  is crossfaded over ±50 ¢ around the boundary, so a bend never steps. Then fcap applies: each component
  is faded by its margin to 0.42 × the bank rate over 500 Hz (polysynth's rule). **What it means on a
  guitar**: every note under C3 (the low E string up to its 7th fret, the A string up to its 2nd) has
  its 16' folded onto its 8', as on a B-3's lowest octave. There 88 8000 000 is the note at +6 dB plus
  its fifth, with no sub-octave; the sub appears from C3 up. Faithful, and to be heard on the bench
  before it is frozen (open question 11).
- **Undecided bands** (U_b, §2.3) play their own partial as a sine at the loudest drawbar's level. It is
  the attack's sound, before the sieve has confirmed anything.
- **Overtones** play at `strings` (σ, 0 – 100 %) as sines: 0 = a pure organ; 100 % = every partial at its
  own level, which adds the guitar's colour (the B9-like brightness, chosen). The registration is never
  applied to an overtone.
- **Purity**: a band whose purity is under 0.5 (two partials) is never a root (polysynth's purity gate,
  w = exp(−σ²_IF / (0.25 B)²)). It plays as an overtone.
- **Aliasing**: one partial per band → each footage is one line. Footage 1' on a band near 1.3 kHz is
  10.4 kHz, under fcap at 48 kHz. The top harmonic's fade and foldback at 5.9 kHz keep everything under
  6 kHz in practice. Only `strings` and the click reach higher.
- Per band, at most 9 footages + 1 percussion component, and only on roots: ≤ 6 bands on a guitar (one
  per string; the `roots` meter and the state are sized for 12).
  Every other band does one sine (n = 1). The organ shaper is therefore cheaper than polysynth's (9
  harmonics on every selected band): §2.6.

### 2.3 The root sieve (new, in gfx-dsp `spectral`; rewritten by the review, §1.4 (6))

At each control tick (16 bank samples, 0.67 ms), on the **clean bank** (the fast bank cannot separate
the partials of a low note):

1. **Candidates**: selected bands (s_b > 0.5) above the absolute floor (`thresh`) and within 40 dB of
   the loudest band.
2. **Overtone test**: candidate p is an overtone of a lower candidate q if
   −6 ¢ − e < 1200 log₂(f̂_p / (k f̂_q)) < 6 ¢ + e + 866 · B_max · (k² − 1) ¢ for some k from 2 **to the
   top of the bank** (k ≤ 2 kHz / f̂_q), and L_q > L_p − 40 dB. B_max = 2 × 10⁻⁴ allows for string
   stretch (5.5 ¢ at k = 8, 22 ¢ at k = 16). The window is asymmetric because strings stretch upwards.
   6 ¢ separates the 5th harmonic from a tempered major third (13.7 ¢) [A]. e is the settling
   allowance of step 3; the *exact* test is the one with e = 0.
   - The draft stopped at k = 16: the 17th to 24th partials of a low string (1.4 – 2 kHz) were never
     tested and became roots, up to 10 on one A2, and that is the "8 roots" of the DI run [P]. Above
     k ≈ 17 the stretched window covers every pitch, so the rule there amounts to: no root more than
     four octaves above a sounding one (on a guitar: nothing above F6 while the low E rings).
3. **Settling allowance.** A band reads its partial's frequency slowly: 26 – 60 ¢ off at 4 τ_b, within
   6 ¢ only after 120 – 150 ms (§1.4 (6)). Each band carries an **age**: the time since it entered the
   selection, set back to the time since the pick when a pick disturbs it (step 5). Its allowance is
   e_b = clamp(15 ¢ × (9.5 − age_b / τ_b), 0, 80 ¢), and the test of p against q uses e = e_p + e_q. In
   the attack the test is therefore wide (a band near any multiple of a lower one waits); it closes to
   the exact one about 150 ms after the pick. Constants fitted on the prototype's bank [P]; to refit
   on the coded core.
4. **Impure** candidates (purity < 0.5) are not roots.
5. **Same pick.** A band can only be the overtone of a band that was **struck by the pick that
   brought it in**. At each pick (detector A) every band's phasor and frequency are kept; for 68 ms
   (20 ms + 3 τ of the widest lag) the band's phasor is compared with the kept one turned at the kept
   frequency. A band that gained more than what it held (|z − ẑ| > |ẑ|), or that enters the selection
   within 150 ms of the pick, *belongs* to that pick. A lower band was *struck* by it if it belongs
   to it too, or if one of its partials (k ≤ 8) that is not a partial of the candidate does. Step 2
   counts q against p only if q was struck by p's pick. So:
   - a note played over a ringing lower note (an arpeggio, a melody over a bass) is a root, even an
     octave or a twelfth above it;
   - a ringing string's own partials stay overtones (they came with its pick);
   - notes struck together (a strum, an octave dyad) are judged as the draft judged them.
   - **The threshold for "struck" is not settled** [P]. As above (the lower band must have gained
     as much as it held): the arpeggio's six notes are roots, and golem's `clean` DI shows a median
     of 6 roots, over 6 for 35 % of the time. With a quarter of what it held: the DI shows a median of
     5, over 6 for 1.3 % of the time, and the arpeggio keeps E3 in 3 seeds of 4, E4 in 2, B3 in none.
     One number trades the two faults; the answer is probably a better test than a threshold (the
     pick's energy over the lower note's whole comb), and it needs takes whose notes are known.
6. **Confirmation**: a band becomes a root (F_b → 1 over 10 ms) once it has passed the exact test and
   step 4 for **4 τ_b** without a break (τ_b = 1 / (π B_b): 64 ms under 330 Hz, 29 ms at 1 kHz) **and**
   passes the widened test now. Once confirmed it needs the exact test only, and it keeps the pick it
   belongs to: a later pick neither takes it away nor mutes it while frequencies settle again. It
   stops being a root when the exact test fails (10 ms).
7. **The status travels with the partial.** When the selection moves the partial to the neighbouring
   band (f̂ within 50 ¢ of the root's last reading, within 20 ms), that band is a root at once, with
   the same pick and age (§2.2). After a longer gap in the selection (a pick's transient on a held
   note, up to 250 ms) the band gets back its pick and age and is confirmed again by step 6.
8. **Undecided** (U_b): a candidate that is not a confirmed root, during its first
   max(2 × 4 τ_b, 220 ms) as a candidate (gaps under 20 ms are bridged). After that it is an overtone
   by default, so a band that beats cannot toggle (§1.4).

- **When the registration arrives** [P]: the fundamental of a single note is a root 101 – 169 ms after
  the pick, the notes of a strummed chord 141 – 181 ms (two notes of the six chords at 251 and 342 ms).
  The rule says 4 τ_b = 64 ms, but purity stays under 0.5 while the band's frequency still moves, so
  the count starts about 60 ms late. See §2.6.
- **Covered notes.** A note struck *together with* a lower note of which it is an integer multiple
  (octaves, the 12th, the double octave, 6 × f…) is an overtone. It sounds through the lower root's
  registration (4', 2⅔', 2', 1⅓'). **What that means in music**: a strummed open E (E2 B2 E3 G♯3 B3 E4)
  has three roots, E2, B2 and G♯3. With a registration that has the upper drawbars (Tutti, Choir,
  Nave) the chord is whole. With 88 8000 000 (Trio, the default) or 8' alone, E3, B3 and E4 are not
  there: the chord keeps its harmony and loses its top, and an octave dyad sounds as its lower note.
  An organist's left hand plays such voicings, so the result is an organ sound and not a wrong one, but
  it is not what the guitarist fretted. Two things bring the doubled strings back: `strings` (their
  partials as sines), and playing them one after the other (step 5). Nothing in the audio tells an E4
  struck with an E2 from the E2's 4th partial: a string's 2nd partial is within 1 dB of its fundamental
  (the string model, picked at 13 % of its length), so level cannot decide, and a 20 ms strum is shorter than a band's rise.
  Bench row "Covered notes".
- **Sympathetic strings** (open, to set on the DIs). An open low E rings by sympathy when an E4 is
  played. If its fundamental is within 40 dB of the E4's (measured case: 18 dB under), the E4 is
  covered and what sounds is an E2 organ note two octaves below the played one [P]. The depth of step 2
  (40 dB) is too lax for that and cannot simply be lowered: a low string's 2nd or 3rd partial can
  exceed its fundamental by 10 dB. 24 dB does not free the measured case. To set on real DIs, perhaps
  by k (a parent far weaker than its 4th or higher multiple is not a parent). Bench row "Sympathetic
  strings".
- **Unresolved intervals** (two notes closer than the clean band's resolution, e.g. E2 + G♯2, 21 Hz
  apart): the band is impure and plays as an overtone (`strings`), or one root takes it. Measured with
  the reviewed sieve: nothing in the attack, −21.5 dB at 200 – 450 ms, −67.5 dB on the body [P]. The
  organ answer is the same as the polysynth's: low thirds are mud on any instrument.
- **What is not solved** [P]: when a ringing string or chord is picked again the model still lets
  ghost roots through at −10 … −16 dB for the time the frequencies settle; a held third lost its
  registration when the bass under it was picked again. And on golem's `clean` DI (strummed chords, the
  strings never stop) step 5 gives a median of 5 or 6 roots where the draft gave 3 (step 5's two
  settings): partly the strings struck again by partial strums, which is right, partly over-rooting.
  There is no ground truth to tell (§5, "Annotated DIs"). Without step 5 the same take has at most 5
  roots and 0.4 % of upper roots that are a multiple of a lower one (draft: 8 and 1.3 %); on
  `lead-clean` at most 5 (draft 7) and 13.5 % (draft 23.6 %).
- **Cost**: per tick, for each candidate (≤ ~20), its lower candidates (≤ 19): one division and a
  rounding each (k = round(f̂_p / f̂_q)), a few hundred operations per tick. Per pick: one phasor and one
  frequency kept per band (59), and for 68 ms one complex multiply and compare per band per tick.
  Negligible beside the banks.

### 2.4 Envelope, percussion, click

- **Body level** G_b: P_b, the band's peak since the last pick (polysynth's hold: attack 1 ms, held, a
  pick lets it go to the band's level), divided by its skirt gain.
  - **`hold`** 0 – 100 %: G_b = P_b^h · |z_b|^{1 − h} (polysynth's `compress` law, capped at +30 dB). At
    100 % the default holds each root at its body level, like an organ key, until the band falls under
    the floor. 0 % follows the string's decay.
  - **`touch`** 0 – 100 %: how much the pick's strength still sets the level. The whole organ is scaled
    by (P_ref / P_max)^{1 − touch}, where P_max is the loudest **candidate** band's body (root or
    overtone; the draft took the loudest root, which raised a weak root standing alone, such as a
    sympathetic open string under a covered note, to full level) and P_ref = −18 dBFS. At
    0 % every chord comes out at one level (an organ has no velocity); at 100 % it follows the pick. This
    is a **global** gain: the balance inside a chord is never changed, so a weak spurious root is never
    boosted to the level of a real one (a per-band equaliser did exactly that in the first model: §1.4).
  - **Gate**: a band below `thresh` − 10 dB, or more than 40 dB under P_max, fades out over `release`,
    with 6 dB of hysteresis (polysynth's floor). A mute or a touch on the strings makes every band fall
    → the organ stops, as a key released.
  - **`swell`** 0 – 2,000 ms: an attack added on top of the register's own (organ volume swells; the
    Shimmer / Cathedral role). Envelope linear in amplitude, retriggered by a pick only when the organ
    had stopped.
- **Percussion** (registers, or Custom's `perc`, `perc_harm`, `perc_decay`, `perc_vol`):
  - Source: u² (2nd, the 4') or u³ (3rd, the 2⅔') of every **root and undecided** band, at the band's
    G_b. Undecided bands are included so that the percussion speaks with the pick; their u² and u³ are
    harmonics of the note [A].
  - Envelope: an exponential, τ = 145 ms (fast) or 579 ms (slow), i.e. setBfree's 1 s / 4 s to −60 dB.
    Its level is Normal 0 dB or Soft −6 dB. **Normal lowers the drawbars by 4.4 dB** (× 0.605,
    setBfree), the Hammond's "volume drop". Fast raises the percussion by +3.5 dB (the "≈ 50 %" of the
    service manual).
  - **The 1' drawbar is muted while percussion is on** (Hammond: its busbar is the trigger). Registers
    use that rule. In Custom it applies as on the organ.
  - **Trigger** (`perc_trig`): Pick (default): every pick onset (detector A) restarts it; legato
    notes do not. Single: the Hammond and B9 rule, restarting only after every root has fallen under the
    gate, i.e. staccato playing.
- **Key click** (`click` 0 – 100 %): on each pick onset, two parts.
  1. **Contact bounce**: the organ output (the drawbar sum, not the dry) is gated by 9 envelopes, one per
     footage, each closing at a fixed pseudo-random time in 0 – 3 ms with 1 – 3 bounces of 0.2 – 0.6 ms
     (deterministic tables, seed fixed; setBfree's model). This only happens on roots and undecided
     bands that start, i.e. at the pick.
  2. **A burst**: 4 ms of xorshift noise (fixed seed, reset), band-passed 1 – 4 kHz (`svf`), with a 70 ms,
     −20 dB tail at the 6th harmonic of the loudest root (Pekonen et al.). Its level is `click` × the
     pick's strength relative to the last 2 s (gfx-autowah's relative follower, as polysynth's
     `fenv_vel`).
  - At 0 % both are off. The register's own click is not separate: `click` is absolute (default 30 %).

### 2.5 Scanner and Leslie: inside the pedal (decided)

**Decision: both live inside gfx-organ**, by reusing the coded engines through gfx-dsp. Neither is
re-implemented.

- **Why inside, not "put gfx-rotary after it"**:
  1. The registers are a sound, and half of each is its modulation (B9: every preset carries one; the
     Gospel and Classic Rock sounds are a Leslie). One selector must recall all of it, which a second
     block cannot do.
  2. The Leslie speed is the organist's footswitch. It must be this block's `leslie`, the same id as
     golem's `organ`, so that golem's foot mapping is the same for both organs.
  3. Hammond order is organ → scanner → preamp → Leslie amp → rotors. The tube drive belongs between
     them, and the `organ` gain (the swell pedal) sits before the Leslie's drive, as on the instrument:
     the swell pedal drives the Leslie harder. With a separate rotary after, EXP on the organ level
     would not drive the rotor amp.
  4. Cost: gfx-rotary measured 375 ns per sample (1.8 % of a core), the scanner well under that.
- **Why not a copy**: the rotor model (inertia constants, path-length Doppler, angle shading,
  cabinets) is gfx-rotary's, voiced and benched. **`Rotary` and its `Rotor`s move from `gfx-rotary/src`
  into gfx-dsp `rotary`**, and gfx-rotary calls them. The move is guarded by a **bit-identical null**,
  as polysynth's `vcf`: every gfx-rotary bench render (each cabinet × speed × transition, mono and
  stereo) must be identical to the current build, with its tests unchanged. If one sample differs,
  gfx-rotary keeps its code and gfx-dsp holds a copy; the shipped VST3's sound does not change either
  way. The same applies to **gfx-vibrato's Scanner** (taps 55 µs apart, 19 taps, V1 → V3, Lagrange read,
  its low-pass) → gfx-dsp `scanner`, null-guarded on gfx-vibrato's Scanner renders.
  - What the code looks like today (read for the review): `gfx-rotary/src/lib.rs` is already free of
    nih-plug (`Rotary`, `Rotor`, `Settings`, on gfx-dsp only), so its move is a move. The Scanner is
    **not a struct**: it is one arm of `Vibrato::process`'s `match` (about ten lines: the tap read on
    the channel's delay line and `scan_lp`). That one is an extraction, to write as a small gfx-dsp
    type that gfx-vibrato's arm then calls.
  - **Rules of the three moves** (`rotary`, `scanner`, `spectral`): one commit each, before any organ
    code, each with its null and its owner's tests green; no arithmetic re-ordered, no `mul_add`
    introduced, same types (Rust does not contract or re-associate floats on its own, so identical
    source arithmetic gives identical samples whichever crate holds it); the owner's CPU re-measured
    (gfx-rotary 375 ns per sample, gfx-polysynth 1.49 – 1.62 µs: within 3 %).
- **`speaker` Off** (Custom) removes the Leslie, for a player who wants gfx-rotary's full panel after
  the pedal. The registers that carry no Leslie (Combo, Nave) are the same as Off.
- Exposed here: `leslie` (Stop · Slow · Fast: the golem organ's id and order, the footswitch), `drive`
  (on top of the register's own; contract id), `ramp` (gfx-rotary's id and law), `spacing` (2 / 2,
  gfx-rotary's id) and, in Custom, `speaker` (Off · 122 · 147 · 145 · Vibratone · 122 Rear; gfx-rotary's
  cabinets in their order, after Off). Everything else (mic distance, wobble, brake, stop angle, rotor
  speeds and time constants) stays at gfx-rotary's defaults, which are setBfree's constants. That is
  gfx-rotary's job if anyone wants more.
- **`mod`** 0 – 100 %, **50 % = the register's own modulation**, a relative parameter (contract rule).
  0 % = no modulation. 100 % = twice the register's depth, clamped:
  - for the scanner, depth along V1 → V3 (gfx-vibrato's continuous `depth`), with chorus keeping its
    mix 0.5;
  - for the Leslie, gfx-rotary's `depth` (Doppler and AM together), whose own scale reaches 1.5 × the
    real geometry at 100 %.
  - The speed is not on `mod`. The B9's MOD is a speed because the B9 has no footswitch; here the speed
    is `leslie` (and the scanner's rate is the instrument's fixed ≈ 6.9 Hz). Open question 2.
  - Kept by the review: an organist changes the rotor's speed by a switch with inertia and never its
    depth, so the switch is the footswitch and the knob is the amount; a speed knob would also make the
    two scanner registers (Nave, Combo) drift from the instrument's fixed rate. The roadmap's line says
    "Modulation (Leslie speed / vibrato)": the departure is the maintainer's to accept. **Accepted by
    Raph, 2026-11-26.**
- `vibrato` (Custom): off · V1 · C1 · V2 · C2 · V3 · C3, golem's `organ` option order.

### 2.6 Latency, CPU, real-time

- **Reported latency: 0.** The dry is sample-aligned. The inherent lags:

  | What | Low strings (bands < 330 Hz) | 1 kHz band | Source |
  |---|---|---|---|
  | 8' (and undecided partials) after a pick, Dual | within 2 dB of the body at 20 ms | same | polysynth [P, measured there] |
  | Same, clean bank only | −3 dB at 23 – 33 ms | 18 ms | §1.4 (4) [A] |
  | Other footages (16', 5⅓', 4' …): the registration | **101 – 169 ms** on single notes E2 … A4, 141 – 181 ms in strummed chords [P]; the rule alone (4 τ_b + 10 ms fade) would say 74 ms | not faster: it waits for the lower bands' frequencies | §1.4 (6), §2.3 |
  | Percussion, click | with the pick (onset detector) | same | §2.4 |
  | Pitch follows a bend: 8' | 10 – 16 ms (fast bank) | 10 ms | polysynth |
  | Pitch follows a bend: the registration | the clean band's 29 – 64 ms, about half the depth of a 6 Hz vibrato | | polysynth [I] |

- **What the bloom means.** On a pick the player hears, at once, the click, the percussion and the
  strings' partials as sines (the 8' within 2 dB at 20 ms). The registration (sub-octave, fifth, upper
  drawbars at their set levels, the overtones removed) comes in **100 – 170 ms** later [P]. Three
  consequences:
  1. **Chords, pads, comping, swells**: the bloom reads as the organ speaking, close to a rotor horn
     picking up. Nave's 180 ms attack hides it entirely.
  2. **Fast single-note lines** (eighth notes at 240 bpm are 125 ms apart): the registration does not
     arrive before the next note. A fast line in Trio is sine + percussion + click, and a slow one
     blooms. That is a real limit against a B9, whose lag is reported as "a little" and unmeasured.
     A legato line fares better than a picked one only if the root's status follows the note, which
     §2.3 does not attempt across different notes.
  3. **Under C3** the 16' is folded onto the 8' (§2.2), so on the low strings the bloom is the fifth
     arriving and the fundamental rising 6 dB, not a sub-octave appearing.
- **Why it cannot simply be shortened.** The decision "this band is a note, not an overtone" compares
  frequencies to 6 ¢, and a 20 Hz-wide band does not give its frequency to 6 ¢ before 120 – 150 ms
  (§1.4 (6)): that is the band's time–frequency trade, not a tuning constant. Confirming at 4 τ_b on
  exact ratios, as drafted, returned ghost roots at −4 … −17 dB through the attack. What can be
  shortened is the case with no doubt: a candidate with nothing under it. Tried: 64 – 83 ms on six
  notes of seven, with ghosts in some attacks (§1.4 (6)); a bench candidate, open question 1.
- **Roadmap criterion "latency at most Polysynth's"**: met for the reported latency (0), for the dry,
  and for the 8' path (the same core, the same handover). **Not met by the registration**, which
  Polysynth has no counterpart of. Whether the criterion is about the first sound (met) or the full
  sound (100 – 170 ms) is the maintainer's reading: open question 1. **Decided by Raph, 2026-11-26:**
  accepted for now (first sound at 20 ms, registration at 100 – 170 ms), to be judged by ear at coding.
- **Handover rule for the organ.** Dual's fast bank sounds during the attack (pick and new-energy
  events). **Pitch-motion events do not hand a root to the fast bank** (polysynth does): "organs do not
  bend", and a root on the fast bank would lose its registration (the sieve needs the clean bank). A
  vibrato therefore reaches the registration late and shallow, while the 8'-only undecided bands follow
  it. Bench row "Vibrato".
- **CPU** [A, estimate to measure], banks at 24 kHz as polysynth:
  - the core's banks and analysis are polysynth's: its measured worst case was 7.1 – 7.8 % of a core on
    pink noise with 9 harmonics shaped on every selected band;
  - here only roots (≤ 12) carry up to 10 components; the other selected bands carry 1. That is ≈ 40 %
    fewer shaper operations than polysynth's worst case, so **the core ≈ 5 – 7 %**;
  - sieve ≈ 0.1 % (per pick: 59 phasors kept and compared for 68 ms); click, percussion, ceiling
    < 0.2 %; scanner ≈ 0.3 %; Leslie 1.8 % (gfx-rotary, measured at 48 kHz: it runs at the host rate, so
    its share is to measure at 96 and 192 kHz, unlike the banks');
  - *Review*: the sum of the measured parts is 7.1 – 7.8 + 1.8 = 8.9 – 9.6 % before any saving from the
    lighter shaper, and the "≈ 40 % fewer shaper operations" is a count, not a measurement: the organ's
    roots also run two √ branches and four tuning rotors each. 10 % is plausible and has no margin;
  - **Budget: ≤ 10 % of a core per instance, worst case** (Dual, a strum, a full registration,
    percussion, scanner and Leslie on) **at 44.1 and 48 kHz**. The draft said "the same at every host
    rate": true of the banks (fixed rate), not of the scanner and the Leslie, which run at the host
    rate as in gfx-vibrato and gfx-rotary. gfx-rotary's 375 ns per sample is 1.8 % at 48 kHz, 3.6 % at
    96 kHz and 7.2 % at 192 kHz, so the same pedal is about 12 % at 96 kHz and 16 % at 192 kHz. Either
    that is accepted (stated per rate), or the rotary stage runs at half or a quarter rate above
    48 kHz, which gfx-rotary does not do and which would be this pedal's own path: open question 10.
    **Decided by Raph, 2026-11-26:** accepted per rate (≤ 10 % at 44.1 / 48 kHz, ≈ 12 % at 96 kHz,
    ≈ 16 % at 192 kHz), the rotary stage at the host rate; confirmed by the measurement.
    10 % is half the contract's 20 % (which is for circuit simulations). It would be the dearest non-circuit pedal
    of the suite: polysynth's 8 % plus a Leslie. If missed, in order: the clean bank drops to order 3 above
    1 kHz (polysynth's first reduction); the fast bank runs only in its attack windows; the scanner's
    Lagrange read becomes linear.
- **Real-time**: banks, tables (bounce, foldback, gear offsets), rotor and scanner delay lines (sized
  for 192 kHz, gfx-rotary's rule) are allocated in `prepare`. No allocation, lock or I/O in `process`.
  Flush-to-zero; phases and recursive state in f64.
- **Determinism**: noise and bounce tables from fixed seeds, reset on `reset()`; rotors at gfx-rotary's
  reset angles; scanner phase 0; tuning rotors at 0. A strict null between two renders.

### 2.7 The nine registers (our names) and Custom

Each register owns its drawbars, percussion, modulation, speaker, drive and attack. `mod`, `click`,
`drive`, `swell` act on top (§3.1). Registrations in Hammond notation (16' 5⅓' 8' 4' 2⅔' 2' 1⅗' 1⅓' 1').

| # | Register (displayed) | Role (B9 counterpart) | Drawbars | Percussion | Modulation at `mod` 50 % | Speaker, drive | Attack |
|---|---|---|---|---|---|---|---|
| 1 | **Tutti** | Fat & Full: octave below and above | 88 8800 008 | off | Leslie (Slow / Fast by `leslie`), depth 60 % | 122, 20 % | 0 |
| 2 | **Trio** | Jazz (the organ-trio sound: Hammond's "basic jazz", 88 8000 000 with the 3rd) | 88 8000 000 | 3rd, fast, normal | Leslie, 60 % | 122, 10 % | 0 |
| 3 | **Choir** | Gospel ("upper octave drawbars added") | 66 8848 588 (Finnigan's "Shoutin'") | off | Leslie, 70 % + scanner C3 | 122, 30 % | 0 |
| 4 | **Bark** | Classic Rock (a touch of distortion) | 88 8800 000 | 2nd, fast, normal | Leslie, 70 % | 147, 60 % | 0 |
| 5 | **Nave** | Cathedral | 80 8808 008 | off | scanner V1 (a slow, shallow shimmer) | Off | 180 ms |
| 6 | **Chime** | Bell organ | 00 8000 808 (8', 1⅗', 1': the tempered third rings) | 3rd, slow, normal | scanner C2 | 122 Stop (cabinet colour, rotors parked) | 0 |
| 7 | **Velvet** | Mellow (the roadmap's own item; Finnigan's "mellow comping") | 80 3600 000 | off | Leslie, 40 % | 122, 0 % | 20 ms |
| 8 | **Combo** | Continental (transistor combo organ) | 00 8030 200 (SoS: "near square") + `strings` floor 20 % (reedy) | off | scanner V2 (single-speed vibrato) | Off | 0 |
| 9 | **Coupler** | Octave Organ / B9 "Octaves" (fundamental + octave: an organ's octave coupler) | 00 8800 000 | 2nd, fast, soft | Leslie, 50 % | 122, 0 % | 0 |
| 10 | **Custom** | the drawbars, percussion, vibrato and speaker of the expert panel | `db16` … `db1` | `perc*` | `vibrato` + Leslie | `speaker` | 0 |

- The roadmap's working names (Fat & Full, Jazz, Gospel, Classic Rock, Cathedral, Continental) are
  **EHX's own B9 preset names** [M]. The roadmap asks for our own names. Checked by the review against
  the B9's and C9's lists (§1.1): the draft's "Octaves" was the B9's sixth preset word for word, and
  "Full" was half of "Fat & Full". They are now **Coupler** (an organ's octave coupler: 8' + 4') and
  **Tutti** (full organ). The other seven (Trio, Choir, Bark, Nave, Chime, Velvet, Combo) match no B9
  or C9 preset (the lists of §1.1); "combo organ" is the generic name of the instrument class.
- **No brand in a displayed name**: the `leslie` parameter is displayed "Rotor" (the id is golem's).
  `speaker`'s options are gfx-rotary's own, already shipped ("122", "147", "145", "Vibratone",
  "122 Rear"): model numbers, and one maker's product name. They must stay identical to gfx-rotary's;
  whether gfx-rotary renames "Vibratone" is the maintainer's call, outside this design.
- A Bass Organ register (B9's "Bottom End") is covered by Tutti or Custom with `lowest` 35 Hz.
- **Custom is last**, so that the nine registers match the B9's nine positions. A register added later
  goes after Custom (enums grow at the end). The order is permanent once shipped.

### 2.8 Relationship with gfx-polysynth (decided: a new crate on shared gfx-dsp modules)

- **The Spectral core moves into gfx-dsp `spectral`** (from `gfx-polysynth/src/spectral.rs`).
  gfx-polysynth then calls it, under a **byte-identical null** of its renders (both engines, every
  `bank`, `wave`, `octave`) and its 17 tests unchanged, the same discipline as `vcf`. The core gains a
  **pluggable shaper**: polysynth's waveform series and gfx-organ's organ law (§2.2) are two
  implementations of one per-band callback, given the phasor, the level, s_b, F_b and U_b. The root
  sieve (§2.3) lives in the core because it reads the core's band data. Polysynth does not use it in v1
  (open question 8).
  - **The risk is here, not in the two other moves.** Polysynth's shaper is not a function today: it is
    written inline in `Core::process`'s band loop (`spectral.rs`, with the √ branches, the detune rotor
    and the octave crossfade around it), and polysynth runs at 7.1 – 7.8 % of a core against a budget of
    8 %. So: the shaper is a **generic parameter** (a trait, static dispatch, `#[inline(always)]`),
    never a `dyn` call or a function pointer per band; the sieve is compiled out of polysynth's
    instance (a const or a zero-sized policy), not skipped at run time; and the move is accepted on
    **the byte-identical null and `cpu_report` within 3 % of today's figure**. If the CPU moves more,
    the fallback is not a copy of the whole core: gfx-dsp `spectral` then holds the analysis (banks,
    selection, rotations, f̂, purity, handover, sieve) and each pedal keeps its own band loop over it.
- **Why not a polysynth mode** (`wave` Organ or `engine` Organ):
  1. The face and the parameters are another instrument: registers, drawbars, percussion, click,
     scanner, Leslie, against wave, cutoff, resonance and filter envelope. As a mode, every one of these
     would be a polysynth parameter "ignored outside Organ" (14 + ignored ids on each side).
  2. Golem needs a block whose ids are the `organ` block's (registrations portable); polysynth's ids are
     its own.
  3. Polysynth is already at its 8 % budget. A Leslie and the organ law would push its worst case over
     it for every polysynth user.
  4. The B9 / C9 is a separate product line from the Synth9 for the same reason.
- **Shared code, then**: `spectral` (core + sieve), `rotary`, `scanner` (both moved, null-guarded),
  `onset`, `oversample`, `svf`, `noise`, `smooth` / `exp_smooth`, `envelope`, `adaa` (ceiling, drive).
  Organ law, percussion, click and registers are in the crate.

## 3. Parameters

### 3.1 Decisions

- New block, new ids, except **the golem `organ` block's own ids for what they name**: `db16`, `db513`,
  `db8`, `db4`, `db223`, `db2`, `db135`, `db113`, `db1` (integers 0 – 8), `perc` (bool), `perc_harm`
  (2nd · 3rd), `perc_decay` (slow · fast, that order), `vibrato` (off · V1 · C1 · V2 · C2 · V3 · C3),
  `leslie` (stop · slow · fast). Same meaning, same options, same order. **Not taken**: the organ
  block's `drive`, which is a bool there (setBfree's overdrive switch); here `drive` is the contract's
  0 – 100 %.
- Ids shared with golem-fx pedals keep meaning and unit: `dry`, `thresh`, `lowest`, `bank`, `release`
  (gfx-polysynth), `ramp`, `spacing` (gfx-rotary), `drive`, `bypass`, `trails`, `state` (contract).
- **Registers own their voicing; user parameters act on top** (contract): `mod` is relative (50 % = the
  register's); `drive` and `swell` add to the register's; `click` is absolute. **No default depends on
  the register.** The expert drawbars, percussion, vibrato and speaker act **only in Custom**, and their
  Role says so. They keep golem's `organ` defaults (88 8000 000, perc off, vibrato off), so Custom
  starts as the organ block starts.
- Linear ranges only; enums append-only; drawbars as nih-plug `IntParam`.
- Structural changes (`register`, `speaker`, `vibrato`, `perc_harm`, `tuning`, `bank`) crossfade over
  20 ms. Drawbar moves are smoothed over 10 ms (a drawbar pulled live must not click; organists do it).

### 3.2 Front panel

| id | Name | Type | Range | Default | EXP | Role |
|---|---|---|---|---|---|---|
| `register` | Register | e | Tutti · Trio · Choir · Bark · Nave · Chime · Velvet · Combo · Coupler · Custom | Trio | | The organ sound: drawbars, percussion, modulation, speaker (§2.7). Custom = the expert panel |
| `organ` | Organ | f | −60 – +12 dB | 0 dB | ✓ | Organ level **before the Leslie's drive**: on EXP it is the organ's swell pedal; −60 = off |
| `dry` | Dry | f | −60 – 0 dB | −60 dB | ✓ | Guitar level, untouched (gfx-polysynth's id and range) |
| `mod` | Mod | f | 0 – 100 % | 50 % | ✓ | Modulation depth relative to the register: 0 = none, 50 % = the register's, 100 % = twice (scanner V1 → V3, Leslie Doppler + AM) |
| `click` | Click | f | 0 – 100 % | 30 % | | Key click at each pick: contact bounce + burst, scaled by the pick's strength |
| `leslie` | Rotor | e | Stop · Slow · Fast | Slow | | Rotor speed, the footswitch. The id, the option ids (`stop`, `slow`, `fast`) and their order are golem `organ`'s; the displayed name carries no brand. The ramp is gfx-rotary's inertia. No effect when the register has no rotary speaker (Nave, Combo) or parks it (Chime) |

### 3.3 Expert

| id | Name | Type | Range | Default | EXP | Role |
|---|---|---|---|---|---|---|
| `db16` … `db1` | 16' · 5⅓' · 8' · 4' · 2⅔' · 2' · 1⅗' · 1⅓' · 1' | i | 0 – 8 | 8 8 8 0 0 0 0 0 0 | | Custom only. 3 dB per step, 0 = out (§2.2) |
| `perc` | Percussion | b | off / on | off | | Custom only |
| `perc_harm` | Perc Harmonic | e | 2nd · 3rd | 3rd | | Custom only. 4' or 2⅔' |
| `perc_decay` | Perc Decay | e | slow · fast | fast | | Custom only. τ 579 / 145 ms |
| `perc_vol` | Perc Volume | e | Normal · Soft | Normal | | Custom only. Normal: −4.4 dB on the drawbars (the B-3's drop); Soft: percussion −6 dB, no drop |
| `perc_trig` | Perc Trigger | e | Pick · Single | Pick | | Pick: every pick. Single: only after all notes have stopped (Hammond, B9). Applies to registers too |
| `vibrato` | Vibrato | e | off · V1 · C1 · V2 · C2 · V3 · C3 | off | | Custom only. Scanner (golem `organ`'s options) |
| `speaker` | Speaker | e | Off · 122 · 147 · 145 · Vibratone · 122 Rear | 122 | | Custom only. Off = no Leslie (gfx-rotary after the pedal) |
| `tuning` | Tuning | e | Tonewheel · Pure | Tonewheel | | Tonewheel: the gear ratios (1⅗' +14 ¢, the fifths −1.4 ¢). Pure: exact ratios |
| `drive` | Drive | f | 0 – 100 % | 0 % | ✓ | Leslie tube preamp, on top of the register's (contract id) |
| `ramp` | Ramp | f | 0 – 400 % | 100 % | ✓ | Scales the rotors' accelerate / decelerate times (gfx-rotary) |
| `spacing` | Mic Spacing | f | 0 – 100 % | 50 % | | 2 / 2 only (gfx-rotary); 1 / 1 sums the mics |
| `hold` | Hold | f | 0 – 100 % | 100 % | ✓ | 100 % = each note held at its body level until it stops (organ); 0 % = follows the string's decay |
| `touch` | Touch | f | 0 – 100 % | 30 % | | How much the pick's strength sets the level (a global gain: chord balance kept). 0 = every chord at one level |
| `strings` | Strings | f | 0 – 100 % | 0 % | ✓ | Partials that are not roots, as sines: 0 = pure organ, 100 % = the guitar's colour added |
| `swell` | Swell | f | 0 – 2,000 ms | 0 ms | ✓ | Attack added to the register's |
| `release` | Release | f | 5 – 2,000 ms | 60 ms | | After the note stops (mute, floor) |
| `bank` | Resolution | e | Dual · Fast · Clean | Dual | | gfx-polysynth's. Dual: the attack on the fast bank. Fast: the 8' tight, the registration still from the clean bank. Clean: slowest attack, cleanest |
| `lowest` | Lowest Note | f | 30 – 150 Hz | 70 Hz | | Bank floor: 70 Hz guitar, 35 Hz bass (gfx-polysynth's) |
| `thresh` | Noise Floor | f | −96 – −30 dB | −66 dB | | Below it no note starts (gfx-polysynth's) |
| `bypass` | Bypass | b | off / on | off | | `make_bypass` |
| `trails` | Trails | b | off / on | on | | On: bypass releases the organ over `release` and lets the Leslie's rotors coast |

Read-only meters: `state` (0 silence · 1 attack: undecided partials only · 2 organ: roots sounding),
`roots` (0 – 12: confirmed roots, for golem's UI and the bench).

Smoothing: `organ`, `dry`, `mod`, `drive`, `strings`, `hold` on gfx-dsp `exp_smooth` (EXP rule of the
contract). `click` and `touch` are read per pick. Drawbars 10 ms linear.

## 4. Applications

Clean DI, first in the lane, before any drive (every maker; the bands need clean partials). Neck or
bridge pickup both work. The bridge gives brighter roots and the neck stronger fundamentals: to judge
on the bench. Play "like an organist": let chords ring, mute to stop.

Two things the player must know (§2.3, §2.6). **Strings struck together an octave or a twelfth apart
make one organ note**, the lower one: with Trio or 8' alone a full open chord loses its doubled top
strings, and Tutti, Choir or Nave (which have the upper drawbars) or `strings` give them back. **The
registration takes 100 – 170 ms to speak**: chords and held notes bloom into it; a fast picked line
is heard as its sine, percussion and click.

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Soul-jazz comping and solos | Trio, `leslie` Slow ↔ Fast on a footswitch, `dry` −60 | Percussion per pick, roots-only registration (no "too bright"). Solos: medium tempo; fast lines stay ahead of the bloom |
| Gospel swells | Choir, EXP on `organ` (swell pedal), `swell` 300 ms | The swell drives the Leslie's preamp |
| 60s rock organ (Whiter-Shade style) | Bark, `drive` +20 %, `leslie` Fast on the chorus | 147 cabinet, drive before the rotors |
| Garage / combo organ (96 Tears role) | Combo, `mod` 60 % | Square-ish registration, scanner vibrato, no Leslie |
| Cathedral pad behind a band | Nave, `swell` 600 ms, `hold` 100 %, `dry` −12 dB | Slow attack hides the bloom; held chords |
| Bell / chime arpeggios | Chime, single notes above the 7th fret | Tempered 1⅗' third, slow 3rd percussion |
| Mellow ballad chords | Velvet, `touch` 0 % | Even level regardless of strum strength |
| Octave lead lines (Zeppelin-style organ doubles) | Coupler, `click` 60 %, `dry` −6 dB | 8' + 4', soft percussion |
| Guitar + organ layer | any register, `dry` 0 dB, `organ` −6 dB | Dry untouched and in time |
| Bass organ / organ bass lines | Custom 88 8000 000, `lowest` 35 Hz, `speaker` Off | Foldback keeps the 16' above 65 Hz |
| Keys player's registrations on guitar | Custom, golem `organ` presets' `db*` values copied | Same ids as the MIDI organ |
| Organ through a full rotary rig | any register with Custom `speaker` Off, gfx-rotary after | Leslie removable |
| Organ with guitar bite | Tutti, `strings` 40 % | Non-root partials as sines |
| Brake to a parked horn | `leslie` Stop | gfx-rotary's coasting brake |

**Preset ideas** (`presets/gfx-organ.json`, no artist or trademark names): `trio-comp`, `trio-solo-fast`,
`choir-swell`, `bark-rock`, `combo-garage`, `nave-pad`, `chime-arp`, `velvet-ballad`, `octave-lead`,
`organ-layer`, `bass-organ`, `custom-ballad-800000008`, `custom-full-888888888`, `no-leslie-rotary-after`.

## 5. Bench suite (on top of gfx-bench's common checks)

Synthetic strings as §1.4 (generated in the test) plus golem's DIs (`clean`, `lead-clean`, `funk`, the
bass takes). Every check at 44.1 / 48 / 96 / 192 kHz.

| Check | Pass |
|---|---|
| Latency | 0 reported; the dry's impulse at sample 0 at every `dry` |
| **Drawbar accuracy** | Single notes E2 … A4, the nine registers and 88 8888 888, Tonewheel and Pure: each drawbar's level within ±1 dB of its target after one gain (prototype ≤ 0.8 dB); outside the registration's lines ≤ −35 dB (prototype −39 … −42) |
| Drawbar step | One drawbar stepped 8 → 1: −3 dB per step ±0.3 dB |
| **Inputs have a noise floor** | Every synthetic input of this suite carries noise at −100 dBFS from its first sample, and each check is also run after 1 s of that noise. A check that passes only from digital silence does not pass (§1.4 (6)) |
| **No ghost notes on single notes** | E2 … A4, four seeds: output of root bands other than the fundamental ≤ −40 dB re the organ's output in every window from 70 ms on (reviewed model: none); never more than 1 root |
| **No ghost notes on chords** | The six chords of §1.4, three seeds: ghost-root output ≤ −35 dB at 70 – 200 ms on resolved chords (model: ≤ −39 dB), ≤ −20 dB at 200 – 450 ms on E2 + G♯2, body ≤ −25 dB (model: A minor −26.8 dB); every played note a root or covered by a note of the same strum; `roots` meter ≤ the number of strings |
| **Sequences** | Arpeggio of the open E (0.3 s apart, ringing): six roots of six, each within 200 ms of its pick. Melody E3 E4 B3 over a ringing E2: each a root. A bass E2 picked once and three times under a held E4: the E4's registration never dips more than 3 dB. Ghost-root output ≤ −20 dB from 70 ms on (model: arpeggio −15.9 dB, bass three times −16.5 dB: **not met yet**) |
| Picked again | One note picked five times; the open E strummed three times: ghost-root output ≤ −20 dB (model: −13 dB and −10 dB: **not met yet**); a held G♯3 keeps its registration when the E2 under it is picked again (model: lost) |
| Sympathetic strings | E4 with an open E2 ringing 18 and 30 dB under it: the E4 is a root; E2 alone still does not root its 2nd or 3rd partial at any pickup setting of the DIs |
| Ghost roots on DIs | `clean`, `funk`: roots within ±30 ¢ of the grid ≥ 95 % of root time; `roots` never above 6 for more than 20 ms (models on `clean`: draft max 8; reviewed without step 5 max 5; with step 5 max 8 – 9, over 6 for 1.3 – 35 % of the time by its threshold: **not met yet**) |
| Annotated DIs | Takes with the notes written down (see "What stays open"): every played note a root or covered by a note of its own strum; no root that is not a played note for more than 20 ms |
| **Intermodulation floor** | Off-grid energy (§1.4 metric, `strings` 0): E5, open C, A minor barre, E4 + G♯4 ≤ −30 dB; open E ≤ −20 dB; E2 + G♯2 ≤ −20 dB; and ≥ 15 dB under a full-band harmonic generator on every resolved chord |
| Covered notes | Open E strummed, 00 8000 000: E2, B2, G♯3 roots; E3, B3, E4 absent as roots; `strings` 100 % restores their partials. The same six strings picked one after the other: six roots (row "Sequences") |
| Third vs 5th harmonic | E2 + G♯3 (tempered) gives two roots; E2 alone does not root its 5th partial (a +2 ¢ stretched partial) |
| Bloom | 5⅓' of an E2 and 16' of a C3 (the 16' of an E2 is folded: §2.2): −3 dB of their body no later than 180 ms after the pick on single notes E2 … A4 (model: root at 101 – 169 ms), reported per note; 8' within 2 dB at 20 ms (Dual). The "lowest candidate" fast path (§1.4 (6)) measured beside it: time and ghosts |
| Hold | `hold` 100 %: a T60 3 s E2 holds every drawbar within 1.5 dB until 10 dB above `thresh`; a palm mute stops the organ within `release` + 30 ms |
| Touch | `touch` 0: two strums 12 dB apart give outputs within 1 dB; chord balance unchanged (each root's level ratio within 0.5 dB) |
| Percussion | 3rd, fast: decay to −60 dB in 1.0 s ±10 %; slow 4.0 s; Normal drops the drawbars 4.4 ±0.3 dB; the 1' muted; Pick retriggers on every pick and never on a hammer-on; Single only after silence |
| Click | `click` 0: bit-identical to no click; 100 %: burst ≤ 5 ms + 70 ms tail; deterministic (null) |
| Foldback | E2 with 16' only: the 16' plays at 82.4 Hz (folded), nothing under 65 Hz; a glide 60 → 70 Hz shows no step (±50 ¢ crossfade) |
| Bend across bands | A 200 ¢ bend on G3 with 80 0000 000 and 08 0000 000, the footage's own line measured alone: no dip > 3 dB as the partial crosses bands (branch and status hand-over, §2.2; draft model: −23 dB for 116 ms) |
| Vibrato | ±30 ¢, 6 Hz on E4: the 8' follows (≥ 85 % depth); the registration reported (lag, depth); no chirp at crossovers |
| Aliasing | 88 8888 888 on A4 … E5, 1' at fcap: nothing above −70 dBc folds below 8 kHz |
| Level bound | −6 dBFS chord, 88 8888 888, percussion Normal, `drive` 100 %, `organ` +12: never over 0 dBFS; the ceiling transparent under −6 dBFS |
| `rotary` null | gfx-rotary on gfx-dsp `rotary` against its current build: bit-identical on its bench renders |
| `scanner` null | gfx-vibrato Scanner on gfx-dsp `scanner`: bit-identical |
| `spectral` null | gfx-polysynth on gfx-dsp `spectral`: byte-identical renders; its tests unchanged; its `cpu_report` within 3 % of the figure before the move and still ≤ 8 % |
| Leslie | Speeds and inertia as gfx-rotary's rows; 1 / 1: L = R = mono sum |
| Register switch, structural changes | Under a ringing chord: no click, ≤ 20 ms crossfade; drawbar pulled 0 → 8 in 50 ms: no click (step-click bound) |
| EXP | `organ` 50 Hz-stepped sweep: control-rate sidebands ≤ −80 dB |
| Determinism | Strict null between two renders; 2 / 2 at `spacing` 0: L = R |
| CPU | ≤ 10 % of a core worst case at 44.1 / 48 kHz (Dual, strums, 88 8888 888, percussion, scanner, Leslie); reported at 96 / 192 kHz, where the scanner and the Leslie cost 2 × and 4 × (§2.6); per register; the core alone the same at every rate |
| Listening | `gfx-bench apps gfx-organ`: one render per Applications row; blind A / B of roots against all-partials at equal level on `clean`; confirmation 3 / 4 / 5 τ_b on chords (the bloom against the attack ghosts); 3 dB against linear drawbar steps; against setBfree playing the same chords by MIDI |

**Order of work.** The sieve is the pedal. Before any parameter, face or preset is coded: the Spectral
core's move into gfx-dsp (null), then §2.3 in gfx-bench on the coded core, against the rows from
"Inputs have a noise floor" to "Annotated DIs". The rows marked *not met yet* are the entry exam, as
Poly's were for gfx-polysynth.

**Fallback**, if the bench says so:
- **Step 5 of the sieve (same pick) over-roots on real takes**: it is switched off (the reviewed sieve
  without it measured clean on both DIs), and the pedal ships with the draft's covering: sequential
  octaves and twelfths over a ringing note are then silent without the upper drawbars, which
  Applications must say.
- **Ghosts when a ringing string is picked again stay above bound**: a pick that disturbs a root
  holds every *new* confirmation in the bands above it for the settling time (the bloom of those notes
  +150 ms), existing roots untouched.
- **Attack ghosts on DIs above bound**: a longer settling allowance (the bloom later by as much).
- **The sieve's window misjudges real strings** (stretch larger than 2 × 10⁻⁴ on a bass): B_max follows
  `lowest` (bass: 5 × 10⁻⁴), and the 5th-harmonic / third discrimination is reported as lost on bass.
- **CPU over budget**: §2.6's reductions.
- **A null fails** (`spectral`, `rotary`, `scanner`): the owner keeps its code; gfx-dsp holds a copy.

## 6. Positioning

- Against the **EHX B9 / C9**: the same instant, polyphonic, tracking-free family, with three measured
  differences:
  1. each string becomes one organ note with the registration's own spectrum (roots law), instead of
     the guitar's spectrum copied at each footage;
  2. the fifths and the third (5⅓', 2⅔', 1⅗', 1⅓') are real drawbars, with tonewheel tuning;
  3. intermodulation is 15 – 50 dB under a shared nonlinearity on chords.
  On top of that: a Leslie (gfx-rotary's) with its speed on a footswitch, full drawbars, percussion
  per pick or single-trigger, `hold` and `touch` instead of uneven thresholds. **Not claimed**: a
  faster full registration, nor every fretted string as its own note. The B9's attack latency is
  unpublished and described as slight; ours puts the first sound at 20 ms and the registration at
  100 – 170 ms [P], so on fast picked lines the B9 class is expected to sound more like an organ than
  this pedal does, and on chords, held notes and registrations with fifths this pedal more than the
  B9. That split is to be confirmed by ear against a B9 or C9 recording of the same DI.
- Against **Boss SY organ patches**: dedicated registers, switchable modulation, the Leslie inside.
- Against **POG-style octave mixers** (POG2, Organizer, Helix patches): the all-partials law is their
  sound. Measured in §1.4 as 3 – 8 dB RMS off a registration; `strings` 100 % gets near it on purpose.
- Against **golem's MIDI organ** (setBfree): not a replacement. That one is the instrument for keys.
  This one is the same registrations, played from strings.
- Not an octaver (gfx-octaver), a synth (gfx-polysynth) or a rotary (gfx-rotary, still the full Leslie
  panel).

## Visual

- **Category `pitch`** (as gfx-polysynth and gfx-sustainer: the pedal hears the notes and makes new
  tones on them). `keys` is golem's category for MIDI instruments (`input_claim: keys`) and does not fit
  a guitar-lane effect.
- **Crate hue `#20226a`, midnight blue** (`METAL_CRATE_HUE['gfx-organ']`): a lacquered console at night,
  the darkest blue of the family. Checked against the 26 hues in `golem-pedals.js` (CIELAB D65, the
  script of gfx-polysynth's review) [A]: ΔE\*ab **41** from the eq blue `#0058e0`, 41 from gfx-correct's
  plum `#7a1450`, 43 from the dynamics slate `#4a5a6e`, more from every other. The suite's accepted
  minimum is 33.
  - Rejected: walnut and wood browns (9 – 27 from gfx-acoustic's rosewood and gfx-fuzz's copper);
    ivory (10 – 12 from gfx-tape's champagne); bottle green (28 from gfx-radio); `#202a60` (32 from
    dynamics).
- Dark body (L\* 18), so `RUNE_LIGHT['gfx-organ'] = '#f0e6c8'`, the ivory of a drawbar's white tip:
  11.3 : 1 on the body [A].
- **Finish**: anodized (the family's).
- **Motif `drawbars`** (new case in `motifParts`, drawn with `line()`: engraved on anodized, yellow lined
  in black on thrash): *a registration pulled out*.
  - Two fields in the rune band, left and right of the cartouche, clear of the knob grid (as `saws`).
  - In each field nine vertical bars hang from a rail at the field's top edge, evenly spaced. Each bar
    is drawn as a line with a short wide cap (a tip 2.5 × the stroke wide).
  - Lengths are proportional to a registration: the left field 88 8000 000 (Trio), the right field
    88 8888 888 (full organ). A drawbar at 0 is drawn as its tip alone on the rail.
  - The Hammond colours as opacity: brown bars (16', 5⅓') .55, black (2⅔', 1⅗', 1⅓') .75, white (8', 4',
    2', 1') 1.
  - Distinct from `scale` (gfx-geq: sliders standing on a scale), `bins` (bars from the bottom),
    `skyline`, `tiers`.
- **Face** (`visuals/data/faces.json`):
  `"gfx-organ": {"name": "Organ", "blocks": ["guitar-organ"], "knobs": ["organ", "dry", "mod", "click"], "selectors": ["register", "leslie"], "category": "pitch"}`.
  Four knobs, two selectors: the B9's four knobs and nine-way switch, plus the footswitchable Leslie
  speed.
- **Glyph** `visuals/glyphs/guitar-organ.svg` (24 × 24, `fill="none" stroke="currentColor"
  stroke-width="1.8"`, round caps and joins, title "Guitar organ (gfx)"). A **string turning into
  drawbars**: one sine cycle (faint) on the left, then a rail with four hanging bars.

  ```
  <path d='M2 12Q4 7 6 12T10 12' opacity='.45'/>
  <path d='M11 3H22M13 3V16M16.5 3V16M20 3V9'/>
  <path d='M13 16v.01M16.5 16v.01M20 9v.01' stroke-width='3.4'/>
  ```

  Distinct from golem's `organ.svg` (five bars from a top edge over a floor line, no string), `polysynth`
  (strings into saws) and `octaver`.
- No maker's livery, logo or trademark name on the pedal.

## Open questions (as they stand after the review)

1. **The bloom, and the roadmap's "latency at most Polysynth's".** Reported latency 0 and the 8' path
   are Polysynth's. The registration arrives 100 – 170 ms after the pick in the model (the draft's 64 ms
   was not measured). Is that the criterion met (first sound) or missed (full sound)? The maintainer's
   reading. On the bench: the "lowest candidate" fast path (64 – 83 ms, ghosts in some attacks), and
   whether fast lines need a register of their own that waits for nothing (the 8' and integer footages
   on every selected band: the all-partials law, the B9 class's sound). **Decided by Raph, 2026-11-26:**
   accepted for now (the 8' at 20 ms, the registration at 100 – 170 ms); judged by ear at coding, with
   the fast path and a fast-line register as the bench's candidates if the ear says otherwise.
2. **`mod` is depth, not speed.** Kept by the review (§2.5); the roadmap's line says speed. The
   maintainer's to accept. **Decided by Raph, 2026-11-26: depth, as the review recommends.**
3. **Layout 1 / 1 first** (the organ is a source, first in a mono lane; the Leslie summed) against the
   contract's rule for stereo-by-nature effects (2 / 2 first). golem-rack's call.
4. **Block id** `guitar-organ` (name "Guitar Organ", short "Organ") and category `pitch` rather than
   `keys`. Checked: golem's catalogue has both categories, and `keys` blocks claim the MIDI input
   (`input_claim: keys`), which a lane effect must not. golem-rack's call.
5. **Register names**: settled by the review (Tutti, Trio, Choir, Bark, Nave, Chime, Velvet, Combo,
   Coupler; §2.7). Left to the maintainer: the displayed name "Rotor" for the id `leslie` (golem's
   own `organ` block displays "Leslie"), and gfx-rotary's "Vibratone" option, which this pedal's
   `speaker` list repeats. **Decided by Raph, 2026-11-26: "Rotor"**, as the review recommends
   ("Vibratone" stays gfx-rotary's call).
6. **Drawbar law**: 3 dB per step, kept. setBfree's is linear (read in its source). The two organs then
   agree on which drawbars are out and on registrations made of 8s and 0s, and differ by up to 6 dB on
   the middle steps (§1.2). Linear would make a `db*` preset sound alike on both; 3 dB is what the
   literature's registrations (Hammond's sheet, Finnigan's) assume. By ear on the bench, then frozen:
   the law cannot change after presets exist. **Decided by Raph, 2026-11-26: 3 dB steps, as the review
   recommends**; the law is settled by ear on the bench (A / B against setBfree), then frozen.
7. **Crosstalk / leakage** (the B-3's −40 dB wheel leakage, setBfree's) and the low wheels' non-sine
   shape: not modelled in v1. Wanted?
8. **The root sieve for gfx-polysynth**: a later `sieve` option there (one synth voice per note's
   fundamental in Spectral) would answer part of what Poly was for, at no tracking risk. Not in this
   design's scope; it inherits every open point of §2.3.
9. **The sieve on real takes** (§2.3, §5): the "struck" test of step 5, ghosts when a ringing string is
   picked again, sympathetic strings, the parent depth. Needs DIs whose notes are known. **Decided by
   Raph, 2026-11-26:** Raph records annotated DIs (chords, arpeggios, fast lines, notes known); proving
   the root sieve on the coded core on them is the first organ task.
10. **CPU**: ≤ 10 % at 44.1 / 48 kHz, an estimate with no margin; about 12 % at 96 kHz and 16 % at
    192 kHz because the Leslie runs at the host rate (§2.6). Accept per rate, or run the rotary stage
    at a lower rate here? The owner decides after measurement. **Decided by Raph, 2026-11-26:** accepted
    per rate, as the review states it (≤ 10 % / ≈ 12 % / ≈ 16 %), confirmed by the measurement.
11. **Foldback on the low strings**: every note under C3 has its 16' folded onto its 8' (§2.2), as on
    the instrument. Authentic, and it removes the sub-octave from the guitar's lowest octave, where
    riffs live. Keep, or let `lowest` move the fold (a guitar organ is not bound to 91 wheels)? By ear.
    **Decided by Raph, 2026-11-26: kept, as on the instrument** (the review's reading); revisited only if
    the ear asks.

## Sources

- EHX: [B9 manual](https://www.ehx.com/wp-content/uploads/2021/01/b9-manual.pdf), [C9 manual](https://www.ehx.com/wp-content/uploads/2021/01/c9-manual.pdf), [Key9 manual](https://www.ehx.com/wp-content/uploads/2021/01/key9-manual.pdf), [POG2 manual](https://www.ehx.com/wp-content/uploads/2020/10/pog2-manual.pdf), [B9](https://ehx.com/products/b9), [C9](https://ehx.com/products/c9)
- Reviews: [SoS B9](https://www.soundonsound.com/reviews/electro-harmonix-b9-organ-machine), [SoS C9](https://www.soundonsound.com/reviews/electro-harmonix-c9-organ-machine), [Premier Guitar B9](https://www.premierguitar.com/gear/electro-harmonix-b9-organ-machine-review), [Audiofanzine B9](https://en.audiofanzine.com/misc-guitar-effect/electro-harmonix/b9-organ-machine/editorial/reviews/review-of-the-electro-harmonix-b9.html), [SoS Boss SY-1](https://www.soundonsound.com/reviews/boss-sy-1), [Mixdown, Mooergan](https://mixdownmag.com.au/reviews/guitars/mooer-mooergan-organ-simulator-pedal/), [Gearspace B9 thread](https://gearspace.com/board/so-many-guitars-so-little-time/936637-electro-harmonix-b9-organ-machine.html) (search summary), [EarthQuaker Organizer](https://www.earthquakerdevices.com/organizer), [Line 6 forum, organ / POG patch](https://line6.com/support/topic/23056-organpog-style-patch/), [Boss SY-1](https://www.boss.info/global/products/sy-1/), [Boss SY-200](https://www.boss.info/global/products/sy-200/)
- Tonewheel organ: [Hammond service manual (archive.org)](https://archive.org/stream/HSM4952016/Hsm495-2016_djvu.txt), [Hammond drawbars & percussion](https://hammondorganco.com/wp-content/uploads/2022/09/03-DRAWBARS-PERCUSSION-corrected.pdf), [SoS, Synthesizing Tonewheel Organs](https://www.soundonsound.com/techniques/synthesizing-tonewheel-organs-part-1), [Electric Druid, technical aspects](https://electricdruid.net/technical-aspects-of-the-hammond-organ/), [Vorkoetter, drawbar science](https://www.stefanv.com/electronics/hammond_drawbar_science.html), [MusicRadar, Finnigan's settings](https://musicradar.com/how-to/mike-finnigans-favourite-organ-settings-and-how-to-recreate-them), [HammondWiki, Leslie speeds](http://www.dairiki.org/HammondWiki/LeslieRotationSpeed), [Wikipedia: Leslie speaker](https://en.wikipedia.org/wiki/Leslie_speaker), [Vox Continental](https://en.wikipedia.org/wiki/Vox_Continental), [Farfisa](https://en.wikipedia.org/wiki/Farfisa)
- Papers: Werner, Dunkel & Germain, DAFx-16 ([PDF](https://dafx.de/paper-archive/2016/dafxpapers/38-DAFx-16_paper_54-PN.pdf)); Pekonen, Pihlajamäki & Välimäki, DAFx-11 ([PDF](https://dafx.de/paper-archive/2011/Papers/49_e.pdf)); Herrera, Hanson & Abel, AES 127 (2009); Smith, Serafin, Abel & Berners, DAFx-02
- [setBfree](https://github.com/pantherb/setBfree) (`cfg/default.cfg`, `src/tonegen.c`, `src/vibrato.c`, `b_whirl/whirl.c`)
- golem-fx: gfx-polysynth (Spectral core, its review and As coded), gfx-octaver (Poly √z), gfx-rotary,
  gfx-vibrato (Scanner), gfx-tuner §2.5 (collision table), `docs/golem-contract.md`,
  `docs/design-review.md`; golem's catalogue `organ` block (catalogue v51) for the shared ids.

## Review (Fable 5.2, 2026-11-26)

**Verdict: the idea is right and worth building; the draft's root sieve was not sound as measured, and
the pedal is At risk on one point, the time its registration takes.** Drawbars as powers of each band's
phase hold up exactly, fifths and third included, and applying them to the notes' fundamentals only is
what no pedal of the B9 class does: on chords, held notes and registrations with fifths this can be the
best in its field. Two claims of the draft did not survive a noise floor in the prototype's input: "no
ghost notes" in the attack, and "the registration blooms 64 ms after the pick". The sieve is rewritten
(§2.3) and passes on single notes, strummed chords, arpeggios and bends in the model; it is not proven
on picked-again strings nor on real takes, and nothing brings the registration under 100 ms. So: build
it, sieve first, as gfx-polysynth's Poly was held to an entry exam. Until the bench says otherwise it
rivals a B9 on chords and pads and not on fast picked lines.

The designer's prototype was re-run under the 3 GB cap (tables (1) – (4) of §1.4 reproduce to the last
digit) and extended: `proto/sieve_review.py` (the reviewed sieve, each change behind a switch) and
`proto/run_review.py` (its measurements, about 15 min). Scratch scripts for the rest (half powers,
DIs, hue). Read for the facts: `gfx-polysynth/src/spectral.rs`, `gfx-rotary/src/`, `gfx-vibrato/src/`,
golem's catalogue (`library/catalog/effects.json`, block `organ`), setBfree's `tonegen.c`. Tag **[P]**
= measured in this review.

### Measured [P] (bank as §1.4; synthetic strings with noise at −100 dBFS; golem's two DIs)

| Question | Result |
|---|---|
| Are fractional powers of the band phase well defined? | Yes. u^½ and u^{3/2} from one tracked branch are as exact as the integer powers: over 150 static cases (a sine anywhere between band centres at 110, 200 and 600 Hz) the 16', 5⅓', 8' and 4' come out at the same level within 0.1 dB. The sign of √u is arbitrary per note and inaudible; 16' and 5⅓' share it, so their relation is fixed. gfx-polysynth's coded `sqrt_branch` is that tracker |
| 16' across bands, on a bend | No cancellation at crossovers in the model. But the draft handed over the branch only: the root's status was not handed over, and the 16' fell 22.9 dB for 116 ms on a 200 ¢ bend. With the status handed over: no drop under −6 dB |
| Does the draft's sieve hold in the attack? | No. Its inputs start from digital silence, which blanks the model's purity for ~90 ms. With a noise floor: ghost-root output −4.2 … −9.7 dB re the organ at 70 – 200 ms on six single notes of seven, −6.7 … −26 dB on the six chords, up to 10 roots on one A2 |
| Why | A clean band reads its partial 26 – 60 ¢ off at 4 τ_b (64 ms) and within 6 ¢ only after 120 – 150 ms; and k stopped at 16, below a low string's partials at 1.4 – 2 kHz |
| The reviewed sieve, first picks | Single notes: no ghost root, one root. Chords: none, or ≤ −39 dB in the attack; body as the draft (worst −26.8 dB, A minor) |
| When the registration arrives | 101 – 169 ms on single notes with either sieve; 141 – 181 ms in strummed chords (two notes at 251 and 342 ms). The draft's 64 ms was the rule's arithmetic, not a measurement |
| Chords where a note is a harmonic of another, struck together | Covered, in both sieves (open E: roots E2, B2, G♯3). Unknowable from the audio; the right outcome with upper drawbars, a loss of the top strings in Trio or 8' alone |
| The same notes played one after the other | Draft: E3, B3, E4 of an arpeggiated open E never sound in 88 8000 000; a held E4 loses its registration when a bass E2 enters. Reviewed (same-pick rule): 6 notes of 6; the held note kept in 4 seeds of 4 |
| Picked again | Ghost roots −13 dB (one note five times), −10 dB (open E three times) with the reviewed sieve; −5 … −15 dB with the draft. Not solved |
| golem's DIs | Reviewed without the same-pick rule: at most 5 roots at once (draft 8 and 7), harmonic root pairs 0.4 % and 13.5 % (draft 1.3 % and 23.6 %). With it: a median of 5 – 6 roots on `clean` (draft 3), over 6 for 1.3 – 35 % of the time by its threshold. No ground truth |
| Faster path (lowest candidate after 2 τ_b) | Root at 64 – 83 ms on six notes of seven; ghosts −10.9 dB (A2), −16.1 dB (A minor). Not adopted |
| setBfree's drawbar law | Linear, `setting / 8.0` (source read): −1.2, −2.5, −4.1, −6.0, −8.5, −12.0, −18.1 dB for steps 7 … 1 |
| golem's `organ` ids | As the draft says: `db16` `db513` `db8` `db4` `db223` `db2` `db135` `db113` `db1` (0 – 8), `leslie` stop · slow · fast, `vibrato` off · V1 · C1 · V2 · C2 · V3 · C3, `perc`, `perc_harm` 2nd · 3rd, `perc_decay` slow · fast; `drive` a bool there |
| Hue `#20226a` | ΔE\*ab 41 / 41 / 43 from its three nearest of the 26 (the draft's figures) |
| CPU | Not measurable without code. The measured parts sum to 8.9 – 9.6 % at 48 kHz before any saving; the Leslie's share doubles at 96 kHz and quadruples at 192 kHz |

### Changes made, and why

| # | Change | Why |
|---|---|---|
| 1 | **§2.3 rewritten**: k to the top of the bank; a settling allowance on the overtone test; confirmation = exact test for 4 τ_b and the widened test now; a confirmed root needs the exact test only | Rows 3 – 5 above. The draft's exact ±6 ¢ test ran on frequencies that are 26 – 60 ¢ off when it confirms |
| 2 | **Same-pick rule** (§2.3 step 5): a band is the overtone only of a band struck by the pick that brought it in; a confirmed root keeps its status across later picks | Rows 7 – 8. Covering by any lower sounding band, whenever, silences sequential octaves and twelfths in the default register and takes held notes away |
| 3 | **The root's status travels with the partial** (§2.2, §2.3 step 7): to the neighbouring band on a bend, and back after a gap of the selection | Row 2: −23 dB for 116 ms in the 16' on a bend |
| 4 | **Bloom restated everywhere as 100 – 170 ms** (status line, §1.4, §2.6, bench, Positioning); "what the bloom means" rewritten with its limit on fast lines; roadmap criterion stated as met for the first sound, not for the registration | Row 6 |
| 5 | Covered notes: the musical outcome written out (which strings, which registers, what brings them back), Applications warned; `touch` normalises to the loudest candidate, not the loudest root | The draft's "missed notes 0" counted covered notes as hits; a weak root standing alone was raised to full level |
| 6 | Sympathetic strings named as an open fault with its measured case; the parent depth left at 40 dB with the reason it cannot simply drop | An E4 over a sympathetic E2 sounded as an E2 organ note |
| 7 | Registers **Tutti** (was Full) and **Coupler** (was Octaves); `leslie` displayed **"Rotor"** | "Octaves" is the B9's sixth preset word for word, "Full" half of "Fat & Full"; "Leslie" is a brand, and the roadmap keeps brands out of parameters |
| 8 | Drawbar law: 3 dB kept; setBfree's law verified and the difference tabulated; the opening claim "a registration means the same on both organs" narrowed | It is true of which drawbars are out, not of their balance |
| 9 | gfx-dsp moves: what each is in the code (rotary a move, scanner an extraction of one `match` arm, spectral's shaper an inline loop); static dispatch; one commit and one null each; **CPU within 3 %** as a condition beside the null | gfx-polysynth runs at 7.1 – 7.8 % of its 8 %: a callback in its band loop can pass the null and miss the budget |
| 10 | CPU budget stated at 44.1 / 48 kHz, with the Leslie's share at 96 and 192 kHz | "The same at every host rate" holds for the banks only |
| 11 | Foldback: its meaning on a guitar stated (no sub-octave under C3); "≤ 12 roots" → 6 on a guitar; bench row "Bloom" no longer asks for the 16' of an E2, which is folded | Consistency |
| 12 | Bench: inputs with a noise floor as a rule; rows for single notes, sequences, picked again, sympathetic strings, annotated DIs; thresholds from the measured figures, the unmet ones marked; order of work (sieve first) and fallbacks | Each corrected claim has its check |

Checked and left as written: the survey and its tags (not re-fetched); the nine ratios and the tonewheel
offsets; the organ law's other terms; `hold`, gate, `swell`; percussion and click (setBfree's constants);
the Leslie inside the pedal (the four reasons hold: one selector recalls the sound, the foot mapping, the
swell pedal ahead of the rotor amp, 1.8 %) with `speaker` Off for gfx-rotary after it; `mod` as depth;
the shared ids and their options; block id and category; zero reported latency, allocation in
`prepare`, determinism, append-only enums, linear ranges, no register-dependent default; hue, motif,
face, glyph.

### What stays open

| Point | Owner |
|---|---|
| **The sieve on the coded core and real DIs**, before anything else of the pedal is coded: the rows of §5 from "Inputs have a noise floor" to "Annotated DIs"; the settling constants refitted; the "struck" test of step 5; ghosts on strings picked again; the parent depth | golem-fx, first task |
| **Annotated DIs**: takes whose notes are known, to a click: the open E arpeggiated and strummed, a melody over a ringing bass, octave dyads, one note picked repeatedly, a re-strummed chord, an E4 with the open low E unmuted; neck and bridge pickups; the bass takes. **Decided by Raph, 2026-11-26:** Raph records them (chords, arpeggios, fast lines, notes known); proving the sieve on the coded core on them is the first organ task | Raph |
| The bloom against the roadmap's latency criterion; whether fast lines get the fast path or a register that waits for nothing (open question 1). **Decided by Raph, 2026-11-26:** 100 – 170 ms (8' at 20 ms) accepted for now, judged by ear at coding | golem-fx at coding (by ear) |
| `mod` as depth; "Rotor" for `leslie`; gfx-rotary's "Vibratone"; foldback under C3; crosstalk. **Decided by Raph, 2026-11-26:** `mod` as depth, "Rotor", foldback kept, as the review recommends; "Vibratone" and crosstalk stay open | The maintainer ("Vibratone", crosstalk) |
| Drawbar law, 3 dB or linear: A / B on the bench against setBfree, then frozen. **Decided by Raph, 2026-11-26:** 3 dB, as the review recommends; settled by ear on the bench, then frozen | golem-fx (bench), the maintainer (ear) |
| The three gfx-dsp moves, each on its null and its CPU; polysynth's shaper as a generic | golem-fx, before the organ's own code |
| CPU measured per register and per rate; the rotary stage's rate above 48 kHz. **Decided by Raph, 2026-11-26:** the budget accepted per rate (rotary at the host rate), confirmed by the measurement | golem-fx |
| Block `guitar-organ`, category `pitch`, 1 / 1 first, the `roots` meter in the UI, the `leslie` footswitch mapping shared with `organ` | golem-rack |
| A B9 or C9 recording of golem's DIs, for the blind comparison Positioning calls for | The maintainer |
