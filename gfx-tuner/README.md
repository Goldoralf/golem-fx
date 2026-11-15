# gfx-tuner

> **Status: CODED** (v0.1.0, 2026-11-15). Designed 2026-11-14 by Claude Opus 5.5, reviewed the same day
> (Review section), coded as reviewed with the deviations listed in "As coded". A coarse YIN names the
> note, a digital strobe measures it, the tuning is the prior, and everything leaves as 23 read-only
> meters; the audio is a bit-exact copy or a 10 ms mute ramp, zero latency.
>
> - **gfx-dsp**: the new `zoom` module (demodulator, wide low-pass, ring; nine Hann frames and a weighted
>   phase line fit with error bar and capture range; a Blackman-Harris grid and peak for Poly) and the
>   five `detect` additions (`prepare_decimated`, `push_decimated`, `set_hop`, `set_floor`,
>   `candidates()`). gfx-pitch, gfx-swell, gfx-ringmod, gfx-crusher, gfx-golem and gfx-octaver render
>   **bit-identically** to before; their tests are green.
> - **The prototype's figures reproduce in Rust**: coarse reading E4 20 ms, E2 30 ms, B1 40 ms, B0 80 ms;
>   first strobe reading E4 0.06 s, E2 0.16 s, B0 0.43 s; settled (Fast) 0.21 / 0.31 / 0.73 s and
>   (Strobe) 0.36 / 0.46 / 1.03 s; worst error while settled 0.038 ¢ over 138 notes in Fast (every string
>   of every tuning, A0 – F6, offsets 0 to −100 ¢), 0.006 ¢ in Strobe (A0 – E6); 0.26 ¢ / 0.033 ¢ after a
>   20 ¢ glide. A D3 with a whole E2 ringing 10 dB under: 0.30 ¢ (Fast), 0.011 ¢ settled at 0.55 s (Strobe).
> - **Strum**: clean strings within 0.08 ¢ on all 20 tunings (3 random strums each, strings ±30 ¢ out),
>   shared strings flagged (≤ 0.5) and right in sign; shown 0.36 – 0.59 s after the strum on a 6- or
>   7-string, 0.55 – 0.84 s on an 8-string, 0.66 – 1.07 s on a bass (slower than the design's 0.35 / 0.75 /
>   0.95 s: windows of 5 / Δ instead of 4 / Δ, which left 1.1 – 1.5 ¢ of pull).
> - **Bench 28 / 28** at 44.1 – 192 kHz with the default options. CPU 30 ns / sample (0.15 % of a core)
>   at the defaults, 94 ns (0.45 %) with eight poly tracks, 1.4 ns bypassed; worst block 0.095 ms.

golem's **tuner**, measured inside the audio engine. It stands for golem's `tuner` block (category
`utility`, today a marker whose runtime is the system JSFX `golem_tuner`) and keeps its parameter id
`tuning` ([golem contract](../docs/golem-contract.md)).

The idea: **a coarse detector names the note; a strobe measures it.** A two-window YIN (gfx-dsp `detect`,
extended down to 25 Hz) says which note or which string is playing within a few cents in a few periods.
Then a **digital strobe** takes over. It heterodynes the signal by the detected frequency and reads the
**rotation speed of the phase** that is left, which is exactly what the disc of a mechanical strobe
tuner shows. That gives ±0.1 cent when the string allows it, the same way on a 1.3 kHz harmonic and on a
5-string bass's low B. With golem's **tuning** (6 to 8 strings, 4 or 5 bass strings), it also reads a
**strum of all the strings at once**: one narrow zoom band per open string, so a string's reading is not
disturbed by the other strings' partials (§2.5).

Everything golem shows is **read-only VST3 parameters** (frequency, note, cents, confidence, string, state,
and cents and confidence for each string). No gmem, no file, no relay of its own: golem reads them the
way it reads every other meter (`gr`, `state`). The same binary runs on REAPER and on the
Golem Engine, which today has **no tuner at all** (its plan lists "x42-tuner LV2 ❓", `docs/ardour-engine.md`).

The audio path is **zero latency**: the signal passes bit-exact, or is muted with a 10 ms ramp while
tuning. Layout: mono 1 / 1 first, 2 / 2 second (the analysis reads (L + R) / 2).

## 1. State of the art

Survey of 2026-11-14. Tags:
- **[V]**: read in a source, a manual or the code;
- **[F]**: forum or user report;
- **[I]**: our inference, or a fact not checked against a primary source;
- **[U]**: a claim we could not verify;
- **[A]**: our own calculation.

### What golem plays today

Read in `~/golem` [V]:

| Piece | What it does |
|---|---|
| Catalogue `tuner` (`library/catalog/effects.json`) | `engine: virtual`, `unique`, `scene_invariant`, `foot_scene_claim` (footswitch 1 – 5, `foot_suggest` 5). One param **`tuning`: a dynamic enum of string ids** from `library/catalog/tunings.json` (default `""`). It is **not a reference pitch**: it has no unit, it names a set of open strings |
| `tunings.json` (a copy of golem.fm's `TUNING_PRESETS`) | 20 tunings: `6-std` … `8-dbdadgbe`, `bass-4-std` … `bass-5-drop-a`; MIDI notes from low to high (D1 = 26 to E4 = 64; lowest A0 = 21 in `bass-5-drop-a`). Defaults: `6-std` for guitar, `bass-4-std` for bass. `bridge/tunings.py` filters them by the lane's role |
| `reaper/Effects/golem_tuner` (JSFX, MIT) | Mono YIN in the lane's **input FX**. It never touches the audio, so the **tuner does not mute** (`docs/devices-people.md`). Steps: decimation to 8 kHz, 4th-order low-pass at 1.6 kHz, full-rate refinement above 125 Hz. Range 28 – 1,320 Hz (lowered on 2026-10-06 for the 8-string and the 5-string bass). **A4 = 440 hard-coded**. Result in `gmem` `golem_tuner` (freq, fractional MIDI, conf, seq). The work is spread over several audio blocks by a budget, giving about 8 analyses / s since the floor was lowered |
| `golem_tuner.lua` → `presets/.tuner.json` | A Lua defer loop reads gmem and writes the file **at most 10 Hz** |
| `bridge/monitors.py` → `/live` | The bridge reads the file at about 20 Hz. `app/live-tuner.js` then needs **3 confirmations** before switching note and smooths the cents with k = 0.25. The arc has 11 segments over ±50 ¢; "in tune" is ±4 ¢. The strings come from the lane's `tuning`, and flat tunings are named with flats |

What is wrong with it, by design:
- **Four hops** before the eye: JSFX → gmem → Lua at 10 Hz → file → bridge at 20 Hz → UI with 3 confirmations.
  A new note shows about 0.3 – 0.5 s after it is played [A].
- **Nothing on the Golem Engine**: no JSFX runtime there.
- **No mute**: the audience hears the tuning.
- **No reference** other than 440 Hz, **no precision mode**, and **no strum**.
- YIN with parabolic refinement is a ~1-cent detector at best [I]. The JSFX source records two
  of its own measured bugs, a ~450 ms-old window and a +70 ¢ bias: proof that this is fragile to get right.

### Hardware and software tuners

| Reference | What defines it | What we take |
|---|---|---|
| TC Electronic PolyTune 3 ([MusicRadar](https://www.musicradar.com/reviews/tc-electronic-polytune-3-review), [manual on ManualsLib](https://manualslib.com/manual/1273592/Tc-Electronic-Polytune-3.html?page=5)) | **Strum all the strings**: every string's state is shown at once. A single plucked string switches to chromatic or strobe [F]. Accuracy ±0.5 ¢ chromatic; strobe ±0.02 ¢ in most listings and reviews, ±0.05 or ±0.1 ¢ in others, no maker's sheet found [U]. No accuracy is published for the poly display. Reference adjustable around 435 – 445 Hz [U]. Output **muted while tuning** [V]. "Display Always On" in buffered mode [V]. The manual advises **neck pickup + thumb strum** for poly [V] | Auto Mono / Poly switching; `mute`; `always_on`. The thumb / neck advice is a clue: poly is spoiled by **upper partials**, which is our §2.5 analysis |
| PolyTune reviews ([Premier Guitar](https://www.premierguitar.com/gear/tc-electronic-polytune-poly-chromatic-tuner-review)) [F] | Poly only reads tunings it knows (standard, transposed, down to B). "A dropped tuning on the lowest string" made poly "useless", and Open E does not work | We know the tuning: golem gives it (`tuning`), so drop and open tunings are first-class |
| Boss TU-3 ([boss.info](https://www.boss.info/id/products/tu-3/)) [V] | ±1 ¢; range C0 (16.35 Hz) – C8; reference 436 – 445 Hz in 1 Hz steps; Chromatic / Guitar / Bass modes with **flat tuning** (×2, ×3 semitones); mute or thru output. A forum user says it "falls apart" on a low B, where a PolyTune 3 is smooth [F] ([rig-talk](https://www.rig-talk.com/forum/threads/tu3-goes-out-of-tune-vs-polytune.332086/post-4450309)) | String-aware mode (Guitar / Bass) = our tunings; `transpose` = flat / capo; the low B as a bench row |
| Peterson StroboStomp HD ([Peterson](https://www.petersontuners.com/support/stroboStompHD)) [V] | Strobe, **±0.1 ¢ detection and display**; range C0 – A#8; **Sweeteners**: over 100 – 135 sets of per-string cent offsets away from equal temperament [V]. StroboSoft is the same idea in software [I] | Precision Strobe; sweetening as **per-string offsets** (`offset_n`). The values of Peterson's sweeteners are not public; we do not copy them |
| Sonic Research Turbo Tuner ST-300 ([MusicRadar](https://www.musicradar.com/reviews/guitars/sonic-research-turbo-tuner-st-300-mini-646201)) | Rotary strobe LED display, **±0.02 ¢ claimed**. Only the maker's figure exists; no independent measurement found [U] | The precision of a claim depends on the string, not the tuner: §2.4 says what 0.1 ¢ means |
| Line 6 Helix / HX ([Helix Stadium manual](https://manuals.line6.com/en/helix-stadium/live/tuner)) [V] | Reference 425 – 455 Hz; **per-string offsets** in cents behind an on / off switch. With offsets on, a 12th-fret harmonic picks the wrong string's offset [F] ([Line 6 forum](https://line6.com/support/topic/70041-helix-floor-382-tuner-offsets-not-working)) | Offsets applied by **string attribution**, and a 12th-fret harmonic is attributed to its string (§2.3) |
| Fractal Axe-Fx III / FM9 ([forum](https://forum.fractalaudio.com/threads/axe-fx-iii-tuner-accuracy.158385/page-4)) [F] | Bar and strobe tuners using different algorithms, which can disagree by "several cents". Some find the strobe "too jumpy". Offsets per string; users share sweetened offset tables | One estimator for every display. Jitter is the estimator's job, not the display's (§2.6) |
| GVST GTune ([manual](https://www.gvst.co.uk/gtune_manual.htm)) [V] | Free chromatic VST: note, frequency, coarse LED indicators, reference control | The floor of the plug-in field |
| **x42 Tuna** ([tuna.lv2](https://github.com/x42/tuna.lv2), GPL-2+; code read in `src/tuna.c`) [V] | **Phase tracking**: a coarse DFT (8,192 points at 48 kHz) finds the octave; the frequency is rounded to the nearest note at a reference of 220 – 880 Hz; then a 4th-order Butterworth **band-pass ±5 % around the note** (at least 10 Hz wide), **zero crossings**, and a 2nd-order **delay-locked loop** give the frequency. Its UI draws the strobe by integrating the cents. Outputs as LV2 control ports: `freq_out`, `octave`, `note`, `cent`, `rms`, `accuracy`, `strobetoui`; RMS threshold −75 dB | The architecture (coarse → narrow band → phase), and the meter ports as read-only params. We replace the IIR band-pass + zero crossings + DLL with **complex demodulation + a finite window + phase regression**: it uses every sample instead of 2 events per period, forgets the attack completely after one window, and gives its own error bar (§2.4) |

### Research

| Paper | Idea | Use here |
|---|---|---|
| de Cheveigné & Kawahara, *YIN*, JASA 111(4), 2002 [V] | Cumulative mean normalised difference function: few octave errors | Coarse stage (gfx-dsp `detect`) |
| McLeod & Wyvill, *A Smarter Way to Find Pitch* (MPM), ICMC 2005 ([pdf](http://miracle.otago.ac.nz/tartini/papers/A_Smarter_Way_to_Find_Pitch.pdf)) [V] | Normalised square difference function with "key maxima"; works with about **two periods** of signal | Fallback if YIN's octave errors stay on the bench; same cost, same buffers |
| Mauch & Dixon, *pYIN*, ICASSP 2014 [V] | Several YIN thresholds turned into pitch candidates with probabilities, then an HMM / Viterbi | One idea only: **keep several candidates** and let a prior choose. Our prior is the tuning's strings (§2.3). No Viterbi: it needs look-ahead |
| Camacho & Harris, *SWIPE*, JASA 124(3), 2008 [V] | Sawtooth-waveform spectral matching, robust to missing fundamentals | Not used: spectral, needs long frames. The strings prior does the same job more cheaply for us |
| Rife & Boorstyn 1974; Kay, *A fast and accurate single frequency estimator*, IEEE TASSP 1989 [V] | The Cramér–Rao bound for one sinusoid's frequency; weighted phase-difference averaging reaches it at high SNR | The fine stage (§2.4): a phase line fit, on windowed frames so that neighbours are rejected too (the bound is for one sinusoid alone) |
| Lee, Smith, Abel, Berners, *Pitch glide analysis and synthesis from recorded tones*, DAFx 2009 ([archive](https://dafx.de/paper-archive/details/hCfsdc26bZYdrYYFI2CHnw)) [V] | A large pluck raises the string's tension: the pitch starts **sharp** and relaxes exponentially to its nominal value | Attack hold-off; the "settled" state; why a hard pluck reads sharp (§2.4) |

### Forum feedback: what players love and hate

| Complaint or wish [F] | Cause | Our answer |
|---|---|---|
| Low B (7-string, 5-string bass) slow or wandering | The window must hold several periods (32 ms each at B0). The fundamental is weak, so the detector jumps an octave | Long-window coarse YIN down to 25 Hz plus the **strings prior**; the strobe reads the fundamental's own phase (§2.2 – 2.4) |
| Jittery needle | Every analysis is independent; quantised YIN lags; **another string still ringing** beats inside the analysis band | One estimator with an error bar; a window long enough to reject the neighbour (§2.4); the reading moves only by more than its own uncertainty (§2.6) |
| "Reads sharp, then drifts flat" | Real: pitch glide of the pluck (DAFx 2009) | `state` = settling until the glide is over. Strobe shows the glide honestly |
| Tuning with the 12th-fret harmonic: cleaner, but offsets go wrong (Helix) | The harmonic is attributed to another note | A harmonic at 2× a string target is attributed to **that string**, with that string's offset (§2.3) |
| Poly fails in drop / open tunings (PolyTune) | Poly assumes standard tuning | golem gives the tuning: every tuning is known |
| Tuner muting or not | Personal and situational | `mute` toggle; golem chooses per rig |

### What we conclude

1. Detection speed and precision are **two problems**. Solve them with two stages: a coarse detector
   names the note, a phase-based strobe measures it. All the good tuners split them this way
   (x42, strobes).
2. **The tuning is the most valuable input.** It removes octave errors on low strings and makes strum
   reading possible. It also lets a harmonic count as its string. golem already knows it per lane.
3. Precision of ±0.1 ¢ is not limited by the estimator but by **the string** (glide, sympathetic
   strings, inharmonicity). Measure it, report an error bar, and say "settled" only when the reading
   has stopped moving.
4. Rejecting a neighbour Δf away takes about 2 / Δf to 4 / Δf of signal, whatever the filter. **Time is
   the only currency**: a finite window spends it once; a narrow IIR filter spends it twice (its own
   settling, then the estimator's window) and never quite forgets the attack (§2.4).
5. Polyphonic reading must be **narrow-band per string**, not a generic filterbank: the partials to
   separate are a few hertz apart (§2.5).

## 2. Design

### 2.1 Signal path

```
in (L+R)/2 ─┬───────────────────────────────────────────────── × g (mute ramp) ──▶ out (L, R pass unchanged)
            │
            └▶ HP 18 Hz (2nd) ─▶ AA LP + ÷D decimator (fs → fd ≈ 12 kHz, shared) ─┬▶ coarse: YIN short; ÷4 ─▶ YIN long (§2.2)
                                                                                    ├▶ onset / level (gfx-dsp envelope)
                                                                                    └▶ zoom tracks: mono 1, poly ≤ 8 (§2.4, 2.5)
                                                                     ▼
                                     note / string attribution (§2.3) ─▶ read-only params, once per hop (10 ms)
```

- **Audio**: out = in × g. When g = 1 the multiply is skipped: the output is a copy, bit-exact, zero latency.
  g moves only by a 10 ms raised cosine: to 0 when `mute` is on **and** the tuner is engaged, back to 1
  when either ends (so putting the tuner away un-mutes by the same ramp, never by a step).
- **Analysis**: runs when the tuner is engaged (`bypass` off). When bypassed it runs only if `always_on` is on;
  otherwise only the copy runs, the analysis state is reset and `seq` stops (§2.6).
- **Hop**: 10 ms for publishing. The demodulators run at fd, every sample; the estimators once per hop.

### 2.2 Coarse stage: two YIN windows (gfx-dsp `detect`, extended)

`detect::PitchDetector` today [V]:
- runs at about 12 kHz, analyses every 32 samples, range **75 Hz – 1.5 kHz**;
- has a **window tied to f_min** (W = τmax = 1 / f_min);
- has harmonic lock (a 2× / 3× lag wins when its dip is clearly deeper);
- does a parabolic refinement on the raw difference function.

The gfx-swell review found two weaknesses. The fixed window is slow everywhere when f_min is low. Two
ringing strings make it flip.

For a tuner:

| Detector | Rate | Range | Window | First reading after the onset | Use |
|---|---|---|---|---|---|
| **Short** | fd ≈ 12 kHz | 60 Hz – 1.5 kHz | 16.7 ms | 16.7 ms + one period + up to one hop: E4 20 – 30 ms, E2 29 – 39 ms | Everything above B1 |
| **Long** | fd / 4 ≈ 3 kHz | 25 Hz – 150 Hz | 40 ms | 40 ms + one period + up to one hop: B0 72 – 82 ms, A0 76 – 86 ms | A0 (27.5 Hz) – B1 (61.7 Hz), and as a check when the short detector's best lag is at its range edge |

The window must lie wholly inside the note together with the lag being tested, hence "window + one
period": the first reading does not come in "2 periods" for a high note, the window is the floor [A].

The long detector runs **four times slower on purpose**. Its notes need no lag finer than 1 / 3,000 s
(B1 is 49 samples, and the parabola refines to well under 1 ¢ there), and the cost of a YIN analysis goes
with the square of the rate. It gets its own ÷4 decimator after a 4th-order low-pass at 600 Hz (two `svf`
sections): a low B still has 19 partials below that.

How the result is chosen, every hop:
- the short detector's result is used when it is voiced (CMNDF minimum ≤ 0.35) and its lag is not at τmax;
- otherwise the long detector's result is used.

Both detectors return their **two best candidates**: lag, CMNDF value and harmonic-lock decision. The
candidates go to the attribution step (§2.3), which uses the tuning as a prior. Coarse precision is ±5 ¢
(±10 ¢ above 1 kHz, where the lag is under 12 samples; gfx-dsp's own test bound) [V]. It only has to pick
the target and centre the strobe. The cents shown before the strobe reads carry `state` = 1.

Changes to gfx-dsp `detect`, small and backward compatible:
1. `prepare_decimated(rate, f_min, f_max, harmonic_lock)`: a detector fed at `rate` with no decimator of
   its own (today `prepare_range` always derives one from the host rate).
2. `push_decimated(y)`: feed an already decimated sample. The tuner owns the decimators for both
   detectors and the zoom tracks.
3. `set_hop(n)`: the hop is a field instead of the `HOP` constant (32 stays the default; the tuner uses
   one 10 ms hop).
4. `candidates() -> [(hz, cmndf); 2]`: the two deepest **distinct** dips (local minima of the CMNDF at
   least 20 % apart in lag), harmonic lock applied to the first.
5. `set_floor(dbfs)`: the silence floor is fixed at −70 dBFS today (`energy < 1e-7`); the tuner sets it
   from `threshold`, which goes lower.

Cost per hop [A]: short τmax = 200, W = 200: 40 k MAC; long τmax = 120, W = 120: **14 k MAC** (it was
230 k at 12 kHz, the one CPU spike of the first draft). Together 5.4 M MAC / s with the existing 8-lane
sums: no block carries more than about 25 µs of YIN, and nothing needs spreading over blocks.

### 2.3 Targets and attribution

The **target** of a reading is what the cents are measured against:

- **Chromatic** (`tuning` = Chromatic): the nearest equal-tempered note, with
  f(n) = `reference` · 2^((n − 69) / 12).
- **A string tuning**:
  - string i has target f_i = f(m_i + `transpose`) · 2^(`offset_i` / 1200), where m_i is the tuning's MIDI
    note for string i (string 1 = the **lowest**, the order of `tunings.json`);
  - a coarse candidate f is attributed to the string with the smallest |cents(f / f_i)|, or to that
    string's **2nd partial** (12th-fret harmonic) when |cents(f / 2f_i)| is smaller;
  - the cents shown are relative to that string's target (or twice it, for the harmonic), clamped to
    ±100 ¢: beyond, the display says "far" and which way;
  - when no string target or 2× target lies within ±100 ¢, the reading falls back to chromatic and
    `string` = 0. That case is a fretted note, or a string more than a semitone out (new strings): the
    chromatic reading is then the useful one.

  The ±100 ¢ is only what is **shown**. The strobe is centred on the frequency heard, not on the target
  (§2.4), so it measures a string 80 ¢ flat exactly as well as one in tune.

Rules:
- **Ties**: when a string's fundamental and another string's 2nd partial are within 30 ¢ of each other
  (DADGAD: D2's harmonic = the D3 string), the **fundamental wins**. With different offsets on the two
  strings, a harmonic read this way is off by the offset difference. That is stated in the UI hint.
- **Why only the 2nd partial**: the 3rd and 4th (7th- and 5th-fret harmonics) fall on open strings in most
  tunings [A]:
  - 6-std: E2 × 3 is 2 ¢ from B3, A2 × 3 is 2 ¢ from E4, E2 × 4 is exactly E4;
  - the 7- and 8-string add B1 × 4 = B3, F#1 × 4 ≈ B1 × 3.

  They cannot be told apart by frequency.
- **Octave errors**: when the two coarse candidates are an octave apart and only one of them is within
  ±100 ¢ of a string or its 2× target, that one wins. This is pYIN's "several candidates + a prior", with
  the tuning as the prior. It is what makes a weak-fundamental low B on a bridge pickup safe.
- **Open string or its harmonic** (the case the prior cannot decide: f_i and 2 f_i both fit string i). The
  strobe decides by looking: a second zoom track at f_i runs beside the one at 2 f_i for the first
  window. A real 12th-fret harmonic has **no fundamental** (its odd partials are damped by the finger);
  an open string read an octave high by YIN has one. Harmonic when the band at f_i is ≥ 20 dB under the
  band at 2 f_i [I, to set on DIs]; otherwise the open string, and the strobe moves down to f_i.
- **Note hysteresis**: the target changes only when the new candidate wins 2 hops running (20 ms). In
  chromatic mode it must also be more than 60 ¢ from the current target, so a bend between two notes
  keeps the old note (cents then run to ±60, inside the ±100 range). This replaces live-tuner.js's
  3-confirmation rule, which became 3 file updates (~300 ms).

### 2.4 Fine stage: the digital strobe (new gfx-dsp module `zoom`)

The mechanical strobe principle: a disc lit at a known frequency stands still when the note is at that
frequency. Its drift speed is the frequency difference. Here the disc is a complex oscillator and its
angle is the phase of the demodulated signal.

The first draft put a narrow 4th-order Butterworth (0.08 f Fast, 0.04 f Strobe) after the demodulator and
fitted a line over 8 or 30 periods. The review prototype (numpy, `proto-gfx-tuner/` in the review
scratchpad: synthetic plucks with 1 / k partials, inharmonicity, decay, +8 ¢ glide, −60 dBFS noise) showed
four faults [A, measured]:
- **Slow where it matters.** Filter settling and window add up: low B first read at 0.9 s in Fast and
  1.9 s in Strobe (the draft said 1.4 s for Strobe, and 0.3 s for Fast at E2).
- **Biased while "settled".** 1 % of filter transient left is 0.13 – 0.35 ¢ at A0 and B0 when the note is
  off centre; an IIR filter never quite forgets the attack.
- **The window added nothing.** T · B = 0.64 in Fast: the fit ran over 4 – 5 samples of an already smooth
  signal, so its residual said nothing. The error bar read up to 7 times too small and called a note
  settled while it was still gliding (up to 5.7 ¢ wrong at E6).
- **Fast did not reject a neighbour.** A string 200 ¢ away at −10 dB (the low E's 2nd partial under the
  D string: the everyday case) made the reading swing ±6 ¢.

The design below keeps the principle (demodulate, read the phase slope) and changes the filter: **the
window is the filter**, and it is finite.

1. **Demodulate at the coarse frequency**: z[n] = x_d[n] · e^(−j·2π·f_c·n / fd), f_c = the coarse reading
   when the track starts (not the target). x_d is the decimated stream; the oscillator phase is f64 with
   an integer sample counter, so it does not drift over minutes. Centring on what is heard makes the
   capture range a non-issue: nothing depends on how far the note is from its target.
2. **Wide baseband low-pass**: 4th-order Butterworth on Re and Im at **0.5 f_c**, then decimation to
   fb ≈ 4 f_c (fd / ⌊fd / 4 f_c⌋), into a ring of 4,096 complex samples (allocated in `prepare`; it holds
   the longest window at any note). This filter only removes the image and the upper partials before
   decimation; it settles in 2 periods and selects nothing. Selectivity comes next.
3. **Estimate**, every hop, over a window T that ends now:
   - **nine Hann frames** of length L = T / 2, spaced L / 8, each a windowed sum X_m of the ring rotated
     by the last estimate (so the frames are centred on the signal);
   - a **weighted line fit of the unwrapped phase** of X_1 … X_9, weights |X_m|², so the quiet end of a
     note counts less. Δf = slope / 2π; the reading is f = f_c + Δf;
   - every frame is a finite sum: a sample older than T has **no weight at all**. A neighbour Δ away
     falls in the Hann frame's side lobes once L ≥ 2 / Δ, and the fit averages what is left;
   - cost: 9 × L · fb ≤ 9 × 900 complex MACs per hop, under 1 M / s.
4. **The window grows.** T starts at 8 periods and grows with the note up to T_max:
   - **Fast**: T_max = 24 periods, clamped to 80 ms – 0.6 s (E2 0.29 s, B0 0.6 s);
   - **Strobe**: T_max = 48 periods, clamped to 0.3 – 1.2 s (E2 0.58 s, B0 1.2 s).

   The first strobe reading comes early and tightens by itself; `precision` only says how long the window
   may get and how still the reading must be before it is called settled.
5. **Attack hold-off**: an onset (fast − slow envelope rise ≥ 6 dB, gfx-dsp `envelope`) restarts the
   track. The window never reaches back before the onset + max(3 periods, 25 ms) + 2 periods (the wide
   filter). Until 8 periods are in, the coarse reading is published with `state` = 1. First strobe
   reading: E4 0.06 s, E2 0.16 s, B0 0.43 s after the onset [A, measured].
6. **Error bar** σ from the fit's residual: σ_Δf = σ_res / (2π · √(3 · Σw(t − t̄)² / Σw)), in cents. The 3
   is the number of independent frames in the span (the nine overlap by 7 / 8); the first draft's formula
   took every sample as independent. σ drives `confidence`. It rises at once when a neighbour beats in
   the band (0.5 – 6 ¢ in the sympathetic rows below, against 0.002 – 0.015 ¢ on a clean note) [A].
7. **Settled** (`state` = 2) is a rule about the readings, not about σ: the readings of the last
   max(150 ms, T / 2) span ≤ **0.2 ¢** (Fast), or those of the last max(300 ms, T / 2) span ≤ **0.07 ¢**
   (Strobe). A glide, a beat or noise all fail it the same way, without a model of any of them.
8. **Capture guard**: the estimate may not leave ±35 ¢ around the coarse reading. If it does (a louder
   neighbour pulled it), or when the coarse reading itself moves more than 170 ¢ from f_c (0.1 f_c: the
   wide filter's flat part), the track restarts on the coarse reading. A peg being turned is followed
   without a restart: rotating the ring costs nothing and has no transient.

**Measured on the prototype** [A] (time after the onset; error against the true fundamental):

| | First strobe reading | Settled, clean note | Settled, +8 ¢ glide (τ 150 ms) | Worst error while settled |
|---|---|---|---|---|
| Fast, E4 / E2 / B0 | 0.06 / 0.16 / 0.43 s | 0.21 / 0.32 / 0.74 s | 0.7 / 0.8 / 1.2 s | 0.13 ¢ (0.30 ¢ after a 20 ¢ glide with τ 250 ms) |
| Strobe, E4 / E2 / B0 | same | 0.37 / 0.47 / 1.03 s | 1.2 / 1.3 / 2.05 s | 0.04 ¢ |

- A note 100 ¢ from its target reads as well as one on it (same rows, −100 ¢).
- **The glide sets the settle time, not the tuner**: an 8 ¢ glide is still 0.3 ¢ sharp after 0.5 s and
  0.1 ¢ after 0.66 s, and the window then has to slide past it. The needle shows the glide meanwhile.
- **Neighbour 200 ¢ below** (reading A2; in practice the low E's 2nd partial under the D string):

  | Neighbour level | Fast (24 periods) | Strobe (48 periods) |
  |---|---|---|
  | −20 dB | 0.11 ¢, settles | 0.04 ¢, settles |
  | −10 dB | swings 0.2 – 0.4 ¢, not called settled | 0.06 ¢, at the edge of the settled rule |
  | 0 dB | swings 1.7 ¢ | 0.16 ¢, not called settled |

  A whole low E ringing 10 dB under the D string (all its partials): Fast swings 0.3 ¢, Strobe reads
  within 0.03 ¢ and settles in 0.55 s. A neighbour **98 ¢** away (low B's 3rd partial under the G string of a 5-string bass or a 7-string) at
  −10 dB: 2.2 ¢ swing in Fast, 0.04 ¢ in Strobe. These are physics (about 2 / Δf of signal per neighbour),
  not tuning constants: Fast shows a slightly breathing needle when other strings ring loud, and Strobe
  is the mode that does not care. Bench option to try: let Fast's window keep growing to Strobe's
  length **while the reading is not settled** [I].
- With −60 dBFS white noise under a note decaying from −12 dBFS: jitter ≤ 0.17 ¢ peak in Fast, ≤ 0.03 ¢ in
  Strobe.

**What ±0.1 ¢ means** [A]:
- **The estimator is not the limit.** The Cramér–Rao bound for one sinusoid at 48 kHz, 30 dB SNR and
  T = 0.25 s is σ = 0.64 mHz: 0.036 ¢ at B0 (30.87 Hz) and 0.013 ¢ at E2 (checked: 12 fs² / (4π² · SNR · N³)).
  A clean DI is nearer 50 dB SNR, which divides those by 10. The frames-and-fit estimator sits about 2 – 3
  times above the bound [A, one case]: the price of rejecting neighbours, which the bound knows nothing
  about.
- **What a 0.1 ¢ step is**, in hertz: 1.8 mHz at B0, 4.8 mHz at E2, 19 mHz at E4.
- **The string is the limit**:
  - pitch glide (sharp at the attack, settling over a fraction of a second; amplitude-dependent [U]);
  - sympathetic strings;
  - **inharmonicity**: the k-th partial of a stiff string is sharp by about 1731 · B (k² − 1) / 2 ¢. With
    B = 1e-4 to 5e-4 [U for guitar strings] a 12th-fret harmonic reads **0.26 – 1.3 ¢ sharp** of the open
    string's fundamental.

  Strobe mode therefore reads the fundamental only. It publishes `state` = 2 (settled) only when the
  readings have held within 0.07 ¢, and never claims a figure the string did not hold. No harmonic
  correction is made: B is unknown per string (question 7).
- **The reference** is exact to f64. `reference` is a linear 0.1 Hz-step parameter.

### 2.5 Polyphonic: strum all the strings

**Feasibility.** Strum reading needs the tuning: we must know which few hertz to look at. The problem is
the other strings' partials. For each open string of every tuning we computed the nearest partial
(orders 1 – 8) of every other string [A; the designer's table, redone at review for all 20 tunings and
with the strings allowed to be out of tune]. A string is **clean** when that partial is at least 150 ¢
away: with both strings up to 30 ¢ off it is still 90 ¢ away, and a window can split them. Otherwise it
is **shared**.

| Tuning | Clean (longest window needed) | **Shared** with a lower string's partial |
|---|---|---|
| 6-std, eb-std, d-std | the four low strings (0.36 s) | 2nd string = 6th × 3 (2 ¢); 1st string = 6th × 4 (exact) |
| drop-d, drop-c | 6th, 5th, 3rd, 2nd (0.27 s) | 4th = 6th × 2 (exact); 1st = 5th × 3 (2 ¢) |
| dadgad | D2, A2, G3 (0.25 s) | D3, A3, D4: octaves of lower strings (exact) |
| 7-std | B1, E2, A2, D3 (0.43 s) | **G3 = B1 × 3 (98 ¢)**; B3 = B1 × 4, E4 = E2 × 4 (exact) |
| 7-drop-a, 7-gdgcfad | four (0.36 s) | the octave of the 7th (exact); 2nd and 1st as in standard |
| 8-std | F#1, B1, E2, A2 (0.58 s) | **D3 = F#1 × 3 and G3 = B1 × 3 (98 ¢)**; B3, E4 (exact) |
| 8-e, 8-drop-e | four / three (0.43 s) | E2 (and A2) = octaves (exact); G3 = E1 × 5 (86 ¢); B3, E4 |
| 8-dbdadgbe | D1, B1 only (0.44 s) | the six others |
| bass-4-std, -eb, -d-std | all four (0.65 – 0.73 s: D2 is 200 ¢ from E1 × 2, 9 Hz) | none |
| bass-4-drop-d | D1, A1, G2 (0.48 s) | D2 = D1 × 2 (exact) |
| bass-5-std | B0, E1, A1, D2 (0.86 s: A1 is 200 ¢ from B0 × 2, 6.7 Hz) | **G2 = B0 × 3 (98 ¢, 5.7 Hz)** |
| bass-5-high-c | E1, A1, D2, G2 (0.65 s) | C3 = E1 × 3 (98 ¢) |
| bass-5-drop-a | A0, E1, D2, G2 (0.65 s) | A1 = A0 × 2 (exact) |

(The first draft listed the 98 ¢ cases as clean. They are not once the strings are out of tune, which is
when a tuner is used: the prototype read G3 on a 7-string up to 7 ¢ wrong.)

Consequences:
- An **ERB filterbank (gfx-octaver's Poly) cannot do this**: one ERB is 33.6 Hz wide at 82 Hz and 60 Hz at
  330 Hz [A]. It holds several strings' partials in one band, and its instantaneous frequency is then a
  mixture.
- What works is **one narrow zoom band per string**, centred on the string's target, with a window long
  enough for that string's nearest neighbour: **T_i = clamp(4 / Δ_i, 0.25 s, 1 s)**, Δ_i = the distance in
  hertz to the nearest foreign partial once 60 ¢ closer than nominal. That is 0.25 – 0.36 s on a 6-string,
  up to 0.58 s on an 8-string, 0.65 – 0.86 s on a bass. Measuring a lone partial to 0.1 ¢ takes far less
  than 1 / (0.1 ¢): only **separating** two partials costs 1 / Δf. So a PolyTune-like response is within
  reach on guitar (first full reading about 0.3 s after the strum) and physically not on a bass's low
  strings, by anyone.
- A **shared** string cannot be separated by frequency when both strings are in tune: 1 ¢ of mistuning
  at E4 moves it 0.19 Hz, about 5 s of resolution. It is read anyway, and **v1 flags it**:
  - what the band shows is the string's fundamental pulled toward the lower string's partial, by about
    their amplitude ratio. Prototype, six strings each ±30 ¢ off, partials 1 / k: B3 and E4 read with a
    median error of 1 – 4 ¢ and a worst case of 6 ¢; 2 – 6 ¢ and 14 ¢ with bright partials (1 / √k) [A].
    The sign is right, the value is not a tuning reading;
  - the lower string's partial is weak with the neck pickup and a thumb strum. That is TC's own
    advice, and our explanation for it [I];
  - those strings get a **lower confidence** (`sN_conf` ≤ 0.5), and golem shows them dimmed with
    "pluck alone", which switches to Mono. PolyTune behaves the same way [F];
  - one case earns full confidence back: the lower string has just been read, so its partial's place is
    known (k × its reading). When the shared string's peak lies **more than one main lobe (4 / T_i) from
    it**, the peak is the string's own [I, bench]. Two strings both in tune stay flagged: reading 0 ¢
    there is right anyway.

**Algorithm**, per string i of the tuning (up to 8):
1. A `zoom` track like §2.4's, with its oscillator **at the target f_i** (there is no coarse reading per
   string): wide baseband low-pass at 0.5 f_i, fb ≈ 4 f_i, the same ring.
2. **Peak search**, once per strum and again whenever the peak is lost: a windowed DFT of the last T_i
   (4-term Blackman-Harris: −92 dB side lobes, main lobe ±4 / T_i) on a grid of ±60 ¢ in 2.5 ¢ steps.
   Cost: 49 points × T_i · fb ≤ 49 × 500 complex MACs per string, once.
3. **Refinement**, every 50 ms: the **phase difference between two half-overlapping Blackman-Harris
   frames** (length 2 T_i / 3) at the peak, the phase-vocoder estimator, iterated twice. The window's
   side lobes reject the neighbours; there is no IIR in the band.
4. A peak on the edge of the ±60 ¢ grid is not a reading: `sN_conf` = 0 and `sN_cents` = ±100 on the side
   it left by ("far").
5. The **clean / shared table** and the T_i are computed when `tuning`, `transpose`, a `string_n` or an
   offset changes. It is bounded (8 strings × 7 others × 8 partials = 448 comparisons) and needs no
   allocation.

**Mode switching** (`mode` = Auto). A strum cannot be told from a pluck in 50 ms: nothing is resolved
yet, and one low E alone lights the B3 and E4 bands with its own 3rd and 4th partials. So:
- after every onset the Mono path runs as usual; a strum gives the coarse stage a poor CMNDF, so
  `confidence` stays low and golem shows "listening…";
- at the **first band reading** (the longest T_i of the clean strings), **Poly** when at least
  min(3, the number of clean strings) **clean** strings show a peak within 30 dB of the loudest. Shared
  strings never count: they light up without being played;
- otherwise **Mono** stays (one string, or a fretted note);
- Poly holds until the next onset or until the level falls 40 dB below the strum's peak. `state` says
  which mode is showing. Mono and Poly force the mode;
- a strum of the high strings only (no three clean strings in it) is read as Mono in Auto: force Poly
  for that;
- Chromatic has no strings, so Poly is unavailable there. `mode` = Poly then behaves as Mono, and `state`
  never reaches 3.

**Timing** [A, prototype: 12 random strums per tuning, 20 ms strum spread, strings ±30 ¢, windows of
40 / f_i as in the first draft; the 4 / Δ_i rule is 0.1 s longer on two bass strings]:
- clean strings read within **±1 ¢** (median 0.02 ¢) at 0.3 s on guitar, 0.5 s on a 4-string bass, 0.75 s
  on a 5-string;
- with an 8 ¢ glide on every string they read 2.8 ¢ sharp at 0.3 s (the strings are sharp then), 0.7 – 1.5 ¢
  at 0.5 s and within 0.3 ¢ at 0.75 s;
- then refreshed every 50 ms.

Later (v2, not in v1): cancel a shared partial whose frequency is known from the lower string's own
measurement (k · f_low with an estimated inharmonicity) by a two-component fit. It is a research item, to
try on the bench with the maintainer's strum DIs.

### 2.6 Outputs, smoothing, and what golem-rack needs

**What the plugin publishes**: every hop (10 ms), the read-only parameters of §3.4 through
`set_output_value`.
- The fork reports them at the end of each process call: one value per block, which is ample.
- All read-only parameters are `FloatParam`s; the fork only has `read_only()` there. Integer values
  (note, string, state) are whole-number floats.
- **No display smoothing in the plugin.** The estimator's window is the smoothing, and the error bar
  is published in `confidence`: the product of the coarse clarity, the level above `threshold` (0 at the
  threshold, 1 from 12 dB above), and a strobe term that is 1 for σ ≤ 0.1 ¢ and falls linearly in log σ to
  0 at 3 ¢ (1 while only the coarse reading exists).
- **Note hysteresis** is in the plugin (§2.3).
- **Dead band**: `cents` changes only when it moves by more than σ / 2 or 0.05 ¢. That stops jitter at the
  source. `freq` is always exact.

**What golem-rack needs to do**, the replacement for the gmem → Lua → file → bridge chain:

| Topic | Rule |
|---|---|
| Placement | One gfx-tuner **first in each instrument lane**, in the lane's own chain on both engines (not REAPER's input FX: the Golem Engine has none, and one placement means one code path and one parity check), inserted with `bypass` on. A bypassed instance copies its input and costs nothing measurable (§2.8). The `tuner` marker then only picks **which lane's instance to engage**. That replaces the `tuner_in` ExtState juggling |
| Engage | Footswitch or `/api/tuner` → `bypass` off on that instance. On REAPER switch by the **parameter**, never by host bypass: REAPER stops processing a host-bypassed plugin, and nothing would be measured. That is the same switch path as GoleM Freeze / gfx-delay trails, `switch_param_of` |
| Insert values | `tuning` = Chromatic for `""`, else **Custom with `string_1…8` = the id's `midi` list** from `tunings.json`, zeros after (§3.1); `reference` from one golem-wide setting, written to every instance; `mute` per golem's policy (question 2) |
| Read rate | Poll the engaged instance at **30 Hz** (REAPER `TrackFX_GetParam`; Golem Engine Lua), on the channel that already carries `gr` and `state`. Read the 7 mono parameters always, and the strings' 2 × N only when `state` = 3: 7 to 23 reads per poll, under 700 a second |
| One poll, one hop | The reads of a poll are separate calls and a hop may land between them. Read `seq` **first and last**; if it changed, drop the poll (or read again). Otherwise a new `note` can arrive with the old `cents` for one frame |
| Freshness | `seq` adds 1 every hop **while the analysis runs**, in silence too. Unchanged for 300 ms: stale (engine stopped, plugin bypassed without `always_on`, plugin host-bypassed) |
| Showing | Show a reading when `confidence` ≥ 0.5, dimmed at 0.3 – 0.5, "listening…" below 0.3 or when `state` = 0. `state` 1 = settling: needle shown, "in tune" lamp never lit. `state` 2 = settled |
| Needle | Interpolate between reads at the display's frame rate. Use a critically damped follower with **τ ≈ 40 ms**, only to hide the 30 Hz steps. Drop live-tuner.js's 3-confirmation rule and its k = 0.25 smoothing: they doubled the plugin's own averaging |
| In tune | The lamp is golem's: \|cents\| inside the band for 150 ms running, `confidence` ≥ 0.5, `state` 1 or 2. Band: **±2 ¢ in Fast** (today's ±4 ¢ is too wide, ±1.5 ¢ is under what a string holds through its decay), **±0.3 ¢ in Strobe** with the cents shown to 0.1 ¢. `state` 2 adds a "settled" mark; it is not a condition of the lamp, or a loud neighbour string would keep it dark |
| Strobe display | **Draw it in golem by integrating the cents**: φ += k · cents · dt, three bands drawn at φ, 2φ, 4φ. Reading a phase from the plugin at 30 Hz would alias. Choose k in **segments per second per cent** so the picture is useful where it is read: k = 0.5 on the slow band (0.1 ¢ creeps one segment in 20 s; the 4φ band shows it in 5 s). A band moving faster than a quarter of the frame rate in segments is drawn as a blur, as a real disc looks |
| Note names | `note` is MIDI at **concert pitch**: octave = ⌊note / 12⌋ − 1. Use flats for flat tunings (as live-tuner.js does with `eb-*`). In string tunings, name the **string's** note from the tuning (`notes`), not `note`'s sharp spelling |
| String numbers | `string` and `s1…s8` count **from the lowest string** (the order of `tunings.json`, so the same index works for 4 to 8 strings). golem shows conventional numbers (the highest = 1): shown = N + 1 − string |
| Poly | Show one column per string from `sN_cents` and `sN_conf`. Columns with `sN_conf` ≤ 0.5 are dimmed with "pluck alone" |
| Drift monitor | With `always_on` on, a bypassed instance keeps publishing. golem can show a small sharp / flat badge during a song |

### 2.7 gfx-dsp reuse and new code

| Piece | Status |
|---|---|
| `detect::PitchDetector` (YIN, harmonic lock, `prepare_range`, `set_range`) | **Reused**, with `prepare_decimated`, `push_decimated`, `set_hop`, `candidates()`, `set_floor` added (§2.2) |
| `svf` (anti-alias low-pass, 18 Hz high-pass) | Reused; the decimator copies `detect`'s filter design, run once and shared; a second ÷4 stage (600 Hz low-pass) feeds the long detector |
| `envelope` (fast / slow followers) | Reused for onsets and level |
| `smooth` | Mute ramp |
| **`zoom`** (new) | A track: complex demodulator (f64 phase) + wide 4th-order baseband low-pass + decimator + ring of 4,096. Two estimators on the ring, both finite windows: Hann frames + weighted phase line fit with error bar (Mono); Blackman-Harris DFT on a cents grid + two-frame phase difference (Poly). About 300 lines. Later users: gfx-crusher's `track`, gfx-pitch's correction, any sustainer |
| `filterbank` (ERB, gfx-octaver) | **Not used**: bands too wide (§2.5) |
| x42 Tuna (GPL-2+, compatible with GPL-3) | Architecture studied; no code copied. Checked at review against `src/tuna.c`: nothing specified here is Tuna's (its FFT overtone scan, ±5 % IIR band-pass, zero-crossing counter and DLL constants are all absent; we share only the published idea of coarse → narrow → phase, and of drawing a strobe by integrating cents) |

### 2.8 Latency and CPU

- **Audio latency: 0** (reported 0, measured 0: a copy or a gain).
- **Reading latency** (onset → first reading, `state` 1, coarse): the detector's window + one period +
  up to one hop. E4 20 – 30 ms, E2 29 – 39 ms; A0 – B1 72 – 86 ms (the long window) [A].
- **First strobe reading** (still `state` 1): E4 0.06 s, E2 0.16 s, B0 0.43 s [A, prototype].
- **Settled** (`state` 2), clean note → with an 8 ¢ glide [A, prototype]:
  - Fast: E4 0.21 → 0.7 s, E2 0.32 → 0.8 s, B0 0.74 → 1.2 s;
  - Strobe: E4 0.37 → 1.2 s, E2 0.47 → 1.3 s, B0 1.03 → 2.05 s;
  - Poly: clean strings within ±1 ¢ at 0.3 s (guitar), 0.5 s (4-string bass), 0.75 s (5-string).

  To confirm on DIs: real glides are the unknown.

CPU at 48 kHz, mono instance [A]:

| State | Cost |
|---|---|
| Bypassed, `always_on` off | A copy |
| Mono | Decimators ~12 flops per input sample; short YIN 40 k MAC / hop; long YIN 14 k MAC / hop; one `zoom` track ~30 flops per decimated sample + ≤ 8 k complex MACs / hop. About **8 M ops / s, well under 1 % of a core** |
| Poly | + up to 8 tracks (≈ 3 M ops / s) and 8 refinements every 50 ms (< 0.5 M ops / s); the grid search (≤ 25 k complex MACs per string) once per strum, spread over the strings' own hops |

The heaviest block carries both YIN analyses and one estimate: about 60 k MACs, a few tens of
microseconds [A]. The first draft's 230 k-MAC long YIN (≈ 0.1 ms in one block every 10 ms) is gone with
the ÷4 (§2.2), so nothing is split over blocks. Nothing allocates in `process`.

## 3. Parameters

### 3.1 Ids and units: the catalogue's `tuning`

The catalogue's `tuning` is a **dynamic string enum** (ids from `tunings.json`; default `""` = no tuning).
It is not a reference pitch, and REAPER's JSFX hard-codes A = 440. A VST3 parameter cannot hold a string,
and `tunings.json` is a copy of golem.fm's list, where ids are born and may be inserted anywhere. An enum
here can only grow at the end. So the two lists cannot stay in step by position, and must not try.

Decided at review:

- **golem always sends the notes, never a rank.** Its engine table maps the catalogue value to
  `tuning` = Chromatic for `""`, otherwise `tuning` = **Custom** and `string_1 … string_8` = the id's
  `midi` list (zeros after the last string). One path for every tuning, today's and tomorrow's; golem.fm
  stays the only owner of the list; no plugin release is ever waited for, and nothing in golem depends on
  the plugin's own table.
- **The plugin keeps a named list as a convenience** for its own presets and for hosts without golem:
  option 0 = **Chromatic**, option 1 = **Custom**, options 2 – 21 = the 20 ids of today, named by their
  ids (`6-std` …). New ones are appended. The two special values lead, so the named run never has a
  foreign option in its middle. Enum order is permanent (descriptor rules); golem does not rely on it.
- If golem-rack would rather send a named option when one exists, it joins **by option name**
  (the name is the catalogue id), never by position in `tunings.json`.
- A named option and Custom with the same notes behave identically: the plugin turns both into the same
  list of MIDI notes before anything else.
- The **reference pitch** is a new parameter, `reference` (Hz). The `tuning` name is not reused for it
  (x42 calls its reference `tuning`; we cannot).

### 3.2 `tuning` options (rank = value; append-only)

| Rank | Option | Rank | Option | Rank | Option |
|---|---|---|---|---|---|
| 0 | Chromatic (`""`) | 8 | 7-std | 15 | bass-4-std |
| 1 | Custom | 9 | 7-drop-a | 16 | bass-4-eb |
| 2 | 6-std | 10 | 7-gdgcfad | 17 | bass-4-d-std |
| 3 | eb-std | 11 | 8-std | 18 | bass-4-drop-d |
| 4 | d-std | 12 | 8-e | 19 | bass-5-std |
| 5 | drop-d | 13 | 8-drop-e | 20 | bass-5-high-c |
| 6 | dadgad | 14 | 8-dbdadgbe | 21 | bass-5-drop-a |
| 7 | drop-c | | | | |

The plugin embeds the MIDI notes of ranks 2 – 21 exactly as in `tunings.json` on 2026-11-14 (§1). A bench
test compares them with the file when it is present.

### 3.3 Front panel

| id | Name | Type | Range | Catalogue | Default | Role |
|---|---|---|---|---|---|---|
| `tuning` | Tuning | e | §3.2 | dynamic enum (string id); golem sends Chromatic, or Custom + the notes (§3.1) | Chromatic | The open strings: string-aware attribution, harmonics, poly. Chromatic = nearest note |
| `mode` | Mode | e | Auto · Mono · Poly | new | Auto | Auto switches to Poly on a strum (§2.5) |
| `precision` | Precision | e | Fast · Strobe | new | Fast | Fast: window up to 24 periods, settled when the readings hold within 0.2 ¢ (error ≤ 0.3 ¢). Strobe: up to 48 periods, hold within 0.07 ¢ (error ≤ 0.1 ¢); slower, and indifferent to other strings ringing (§2.4) |
| `reference` | Reference A | f | 410 – 470 Hz, 0.1 Hz steps | new | 440 Hz | A4. Covers 415 (baroque), 432, 435 – 445 (orchestras), 466 |
| `mute` | Mute | b | off / on | new | off | Output muted (10 ms ramp) while the tuner is engaged. No effect while bypassed, with or without `always_on` |
| `bypass` | Bypass | b (`make_bypass`, `kIsBypass`) | off / on | — | off | **On = the tuner is put away**: audio passes bit-exact; analysis stops unless `always_on`. golem engages the tuner by turning this off. A **host** bypass that stops processing (REAPER's) measures nothing and freezes `seq`: golem never uses it here (§2.6) |

**Defaults are the safe ones.** A freshly inserted gfx-tuner is engaged (bypass off, the VST3 norm) but
never silent (`mute` off). golem sets `bypass` on and `mute` at insert (contract: per-block values at
insert). Nothing depends on another parameter.

**Two switches, two meanings.** `bypass` says whether the tuner is out (engaged); `mute` says what the
audience hears while it is. Engaged + `mute` on is the pedal tuner; engaged + `mute` off is tuning while
heard (golem's behaviour today); bypassed + `always_on` is the drift watch, never silent. On the Golem
Engine, deactivating the plugin drives `bypass` (the contract's Trails rule), so it behaves as on REAPER.

### 3.4 Expert

| id | Name | Type | Range | Default | Role |
|---|---|---|---|---|---|
| `transpose` | Transpose | i | −12 – +12 st | 0 | Shifts every string target: capo (+), down-tuned or "flat ×n" (−). String tunings only |
| `offset_1` … `offset_8` | Offset 1 … 8 | f | −50 – +50 ¢, 0.1 ¢ steps | 0 ¢ | Per-string sweetening, string 1 = lowest. Applied to any string tuning, including the 12th-fret harmonic of that string |
| `string_1` … `string_8` | String 1 … 8 | i | 0 – 96 (MIDI; 0 = no string) | 40, 45, 50, 55, 59, 64, 0, 0 | Custom tuning's open strings, low to high; **the list ends at the first 0**. Read only when `tuning` = Custom. The fixed default is standard 6-string. This is the path golem uses for every tuning (§3.1) |
| `threshold` | Threshold | f | −90 – −30 dBFS | −65 dBFS | Level below which nothing is read (noisy rigs, hum). The JSFX gate was −62 dBFS |
| `always_on` | Always On | b | off / on | off | Keep measuring while bypassed: drift badge during a song (PolyTune's "Display Always On") |

Expert groups (`visuals/data/groups.json`): **Strings** (`transpose`, `string_1…8`, `offset_1…8`),
**Detection** (`threshold`, `always_on`).

### 3.5 Read-only meters

| id | Name | Range | Meaning |
|---|---|---|---|
| `freq` | Frequency | 0 – 5,000 Hz | Fine (or coarse while settling) frequency; 0 = no reading. f32 resolution at A0 is far below 0.01 ¢ [A] |
| `note` | Note | 0 – 127 | MIDI of the target, concert pitch; 0 = none. In string tunings the string's note (a 12th-fret harmonic gives its string's note + 12) |
| `cents` | Cents | −100 – +100 ¢ | Deviation from the target (or twice it for a 12th-fret harmonic, §2.3), with the offset included. Clamped: ±100 = "far" |
| `confidence` | Confidence | 0 – 1 | Product of the coarse clarity, the strobe's error bar mapped to 0 – 1, and the level above `threshold` (§2.6) |
| `string` | String | 0 – 8 | Attributed string (1 = lowest); 0 = chromatic or a fretted note |
| `state` | State | 0 – 3 | 0 silence · 1 note, settling · 2 note, settled · 3 strum (poly readings valid). Shared id `state` |
| `seq` | Update | 0 – 1,023 | +1 every hop while the analysis runs, wraps: freshness, and the guard of a poll (§2.6) |
| `s1_cents` … `s8_cents` | String n Cents | −100 – +100 ¢ | Poly reading per string, strings the tuning has. 0 when unread; ±100 = far (§2.5) |
| `s1_conf` … `s8_conf` | String n Confidence | 0 – 1 | 0 = not heard; ≤ 0.5 for shared strings (§2.5) |

That makes 23 read-only parameters: 7 always read, 16 only during a strum. They are plain meters like
`gr`; `state` is the shared id, with this effect's own values as in gfx-gate and gfx-swell.

## 4. Applications

Reference settings become block presets (`presets/gfx-tuner.json`); no artist or trademark names.

| Use / preset idea | Settings to start from | Feature that matters |
|---|---|---|
| **Stage Silent** (between songs) | `mode` Auto, `precision` Fast, `mute` on, `tuning` = the lane's | Mute ramp; readings in tens of ms; string chips |
| **Strum Check** (is anything off?) | `mode` Auto (or Poly), Fast, `mute` on. Neck pickup, thumb strum | Per-string columns 0.3 s after the strum on guitar, 0.5 – 0.9 s on bass; pluck the dimmed (shared) strings alone |
| **Strobe Setup** (intonation at the bench) | `tuning` Chromatic, `precision` Strobe, `mode` Mono, `mute` off. Compare the open string with the 12th fret **fretted** (not the harmonic, which is sharp by the string's inharmonicity) | ±0.1 ¢ with an honest "settled" |
| **Bass Low B** | `tuning` bass-5-std, Fast (B0 settles in 0.75 s); Strobe for a final check (1 s on a soft pluck, 2 s on a hard one: the glide) | Long coarse window down to 25 Hz; the strings prior stops octave errors |
| **Drop A 7-string** | `tuning` 7-drop-a, Fast | A1 at 55 Hz on the long window; string attribution |
| **Capo 2** | `tuning` 6-std, `transpose` +2 | Targets follow the capo |
| **Half-Step Down** | `tuning` eb-std, or 6-std with `transpose` −1 | Flat names (golem) |
| **Sweetened Electric** | 6-std, `offset_n` set by ear per string (typically within ±3 ¢ [U]); golem may store sweetened sets as Custom-like presets | Offsets also apply to that string's 12th-fret harmonic |
| **A = 432** / **Baroque 415** | `reference` 432 / 415 | Exact reference |
| **Harmonic Tuning** (quiet stage, dead strings) | Any string tuning, Fast; play 12th-fret harmonics | 2× attribution to the right string |
| **Drift Watch** (long sets, temperature) | `bypass` on, `always_on` on, `mute` off | golem's small badge while playing |
| **Voice / Violin Practice** | Chromatic, Fast, `mute` off | Chromatic attribution with hysteresis |

## 5. Bench suite (on top of gfx-bench's common checks)

**gfx-bench options**: defaults. The audio is a copy or a gain, so silence floor −140 dBFS, latency 0,
click bound −50 dB (the mute ramp). gfx-bench's `Effect` trait has no output parameters. It needs a
`meters()` accessor returning the read-only values, so these checks can read them (small bench change).

Synthetic notes for the bench:
- partials 1 / k up to 5 kHz;
- inharmonicity B ∈ {0, 2e-4, 5e-4};
- exponential decay;
- optional pitch glide: +8 ¢ relaxing with τ = 150 ms, and +20 ¢ with τ = 250 ms [U, test values];
- noise at −60 dBFS under a note starting at −12 dBFS;
- a 12th-fret harmonic is the even partials only; a "bridge pickup" open string has its fundamental
  20 dB under its 2nd partial.

The review prototype (`proto-gfx-tuner/zoom3.py`, `t4.py`, `poly.py` in the review scratchpad) passes the
strobe and poly rows below on these signals; its figures are the bounds, with a margin.

| Check | Pass |
|---|---|
| Accuracy, Fast | Every string of every tuning in §3.2, and chromatic A0 – F6 (the coarse stage stops at 1.5 kHz), at 0, ±3, ±20, ±49 ¢, and −80 / −100 ¢ on strings: after `state` 2, \|error\| ≤ 0.3 ¢ (B = 0, no glide) |
| Accuracy, Strobe | Same notes: \|error\| ≤ 0.1 ¢ and σ ≤ 0.03 ¢ over 2 s, including A0 (27.5 Hz) and B0, settled within 1.2 s |
| Reference | `reference` 415 / 432 / 466: a tone at the reference reads 0.00 ± 0.05 ¢; every sample rate 44.1 – 192 kHz agrees ± 0.05 ¢ |
| Speed, coarse | Onset → `state` ≥ 1 within ±5 ¢: E4 ≤ 35 ms, E2 ≤ 45 ms, B1 ≤ 50 ms, B0 ≤ 95 ms |
| Speed, strobe | First strobe reading: E4 ≤ 80 ms, E2 ≤ 0.18 s, B0 ≤ 0.45 s. `state` 2 on a clean note, Fast: E4 ≤ 0.25 s, E2 ≤ 0.35 s, B0 ≤ 0.8 s; Strobe: E4 ≤ 0.4 s, E2 ≤ 0.5 s, B0 ≤ 1.1 s |
| Octave errors | 200 plucks per string from DIs (neck and bridge pickups, guitar and bass; `clean.wav` + the maintainer's bass DI): 0 octave errors in string tunings, ≤ 1 % in chromatic |
| Open string or harmonic | The "bridge pickup" open E2 reads `note` 40, `string` 1; the synthetic 12th-fret harmonic reads `note` 52, `string` 1 (§2.3) |
| Needle jitter | Steady tone, noise −60 dBFS: `cents` σ ≤ 0.1 ¢ (Fast), ≤ 0.02 ¢ (Strobe) after settling; no published step > 0.5 ¢ while the input is constant |
| Error bar | Clean notes: σ within a factor 3 of the measured standard deviation of the readings. With a neighbour 200 ¢ away at 0 dB: σ ≥ 0.5 ¢ |
| Neighbour strings | D3 read while a full E2 rings 10 dB under it: swing ≤ 0.4 ¢ (Fast); ≤ 0.1 ¢ and `state` 2 within 0.7 s (Strobe). One partial 200 ¢ away at −20 dB: ≤ 0.2 ¢ and settled in both. At 0 dB: Strobe ≤ 0.2 ¢ and `state` never 2. G2 with the low B ringing 10 dB under (98 ¢): Strobe ≤ 0.1 ¢ |
| Capture guard | A2 read while E2 rings 10 dB **louder**, track forced on A2: the reading never leaves ±35 ¢ of the coarse value; Strobe ≤ 0.1 ¢ |
| Glide | Both glide tests: `state` stays 1 while the pitch moves; error at the first `state` 2 ≤ 0.3 ¢ (Fast), ≤ 0.1 ¢ (Strobe); the settled value is the nominal ± 0.1 ¢ |
| Peg turn | E2 sliding from −80 ¢ to 0 in 2 s, Fast: the reading follows within 8 ¢ (half a window behind) with no gap and no restart; `state` 2 within 0.45 s of the stop |
| 12th-fret harmonic | 2 × f_string with B = 2e-4: attributed to the right `string`; reads +0.5 ¢ ± 0.1 (the physics, not corrected) |
| Coincidence (DADGAD) | D2's harmonic → the D3 string (the fundamental wins); documented behaviour |
| Poly | Synthetic strums of every tuning: each string at a random offset ±30 ¢, 20 ms strum spread. Clean strings ±1 ¢ at their T_i + 0.1 s (0.35 s on 6-std, 0.75 s on bass-4, 0.95 s on bass-5-std); shared strings flagged `sN_conf` ≤ 0.5 and their sign right when more than 10 ¢ off; `state` 3 at the first band reading. A single pluck after a strum returns to Mono |
| Poly, false strum | One open low string alone, in every tuning: `state` never 3 (its partials light the shared bands) |
| Poly on DIs | The maintainer's strum recordings (neck / thumb and bridge / pick): readings against the same strings plucked alone and read in Strobe. Report the error per string, clean and shared; no pass bound until measured |
| Mute | `mute` toggled on a 1 kHz sine, and `bypass` turned on while muted: no HF burst > −50 dB re peak (the ramp). Unmuted and bypassed: output bit-exact to the input |
| Freshness | `seq` adds exactly 1 per 10 ms at block sizes 32 – 2,048 and at every sample rate, in silence too; it stops when bypassed without `always_on` |
| Bypassed CPU | `bypass` on, `always_on` off: cost of a copy (≤ 2 ns / sample) |
| CPU | Mono ≤ 1 % of a core, Poly 8 strings ≤ 1.5 %, 48 kHz, 128-sample blocks; worst block ≤ 0.1 ms |
| Tuning table | The 20 embedded tunings equal `~/golem/library/catalog/tunings.json` (MIDI notes) when the file is present; every named option and Custom with the same notes give identical meters |
| Against references | Same DIs through x42 Tuna and golem's JSFX: report the differences (no bound). A hardware strobe reading if the maintainer has one |

## 6. Positioning

| | golem's JSFX today | gfx-tuner |
|---|---|---|
| Engines | REAPER only | REAPER and the Golem Engine (VST3) |
| Path to the UI | gmem → Lua 10 Hz → file → bridge 20 Hz → 3 confirmations | Read-only params, 30 Hz polls, no confirmations |
| Precision | ~1 ¢ [I] | 0.3 ¢ Fast, 0.1 ¢ Strobe, with an error bar and a "settled" that waits for the string |
| Low range | 28 Hz, single long window (slow for everything) | 25 Hz on a separate long window at a quarter of the rate; the short one stays fast |
| Tuning-aware | UI only (chips) | Attribution, octave errors, harmonics, sweetening, poly |
| Mute, reference, capo | none, 440, none | `mute`, 410 – 470 Hz, `transpose` |

Against the field: PolyTune's strum (here in any tuning golem knows), Peterson's precision and sweetening
(as per-string offsets), the TU-3's string modes and flat tuning. x42's phase principle comes with an
estimator that reports its own uncertainty. Not attempted: a calibrated stretch / inharmonicity
correction (needs B per string), and temperaments other than equal (a later `temperament` enum,
appended, if asked).

## Visual

- **Category: `utility`** (golem's own category for `tuner`; no catalogue change).
- **Hue**: `METAL_CRATE_HUE['gfx-tuner'] = '#126e1c'`, **deep scope green** (hue 127°, L* 40), with the
  lamp as the rune light: **`RUNE_LIGHT['gfx-tuner'] = '#3ddc2e'`**. The body is the dark glass of an
  instrument; the light is the "in tune" lamp, which is where that green belongs.
  - The first proposal, `#23bf18` (hue 116°), sat in the 84° → 148° gap by hue only. At equal lightness and
    the same anodized finish it is ΔE*ab 19 (ΔE00 7) from gfx-glitch's lime `#72c000`: the same pedal at
    tile size. The gap between lime and modulation's green `#00a84f` is itself only ΔE00 15: there is
    no room for a third bright green.
  - `#126e1c` separates by **lightness**: ΔE*ab 46 (ΔE00 30) from the lime, 25 (21) from modulation's
    green, ≥ 58 from everything else in use or proposed, the lot-3 hues included (gfx-widener's ice
    `#c0e0f8`, gfx-tape's champagne `#d8c8a0`, gfx-ringmod's brass `#c9b000`) [A, CIELAB D65].
  - To confirm on the family sheet next to gfx-glitch and a modulation pedal.
- **Finish: Anodized** (`CRATE_FINISH['gfx-tuner'] = 'anodized'`): a measuring instrument, like
  gfx-gapmeter. Utility's family finish is Thrash, so this is a crate exception, as for gapmeter.
- **Motif: `strobe`** (new). Three concentric arcs round the rune, the bands of a strobe disc:
  - radii W·0.40, W·0.47 and W·0.54;
  - each is a run of short arc segments with equal gaps: **12, 24 and 48** segments per turn (96 on the
    outer ring is under a pixel per segment at tile size). The doubling stands for the octave bands of
    a strobe disc;
  - drawn on two spans, 200° – 340° and 20° – 160°, like `crescent`, so they stay clear of the knobs;
  - the inner ring at full opacity, the outer two at 0.7 and 0.45;
  - the middle ring is offset by half a segment, so the eye reads motion.

  Anodized: engraved segments, lit by the rune light. Thrash (if forced): yellow segments lined in black. Drawn with
  `arcPath` per segment.
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-tuner": {"name": "Tune", "blocks": ["tuner"], "knobs": ["reference"],
                "selectors": ["mode", "precision"], "toggles": ["mute"], "category": "utility", "color": "#126e1c"}
  ```

  `tuning` (22 options) is not on the face: it comes from the lane and belongs in golem's own picker.
- **Glyph: keep golem's existing `tuner` glyph** (the needle arc). It is the link between the block tile
  and the pedal (gfx-crusher's precedent). If a pedal-only glyph is wanted, the strobe variant
  (24 × 24, single colour, line style of `glitch.svg`):

  ```svg
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>Strobe tuner (gfx)</title><path d='M3 17a9 9 0 0 1 18 0'/><path d='M6.5 17a5.5 5.5 0 0 1 11 0' stroke-dasharray='1.6 2.2'/><path d='M12 17l2.8-7.5'/><circle cx='12' cy='17' r='1.4' fill='currentColor' stroke='none'/></svg>
  ```
- No maker's livery, logo or trademark on the pedal.

## Questions settled at review

1. **`tuning` as a ranked enum + Custom, or notes only?** Both, with the roles fixed (§3.1): golem always
   sends Custom + `string_1…8`, so the catalogue's dynamic list and the plugin's enum never have to
   agree; the named options stay for presets and for hosts without golem, Chromatic and Custom first.
   → golem-rack confirms its engine table takes one catalogue value to nine parameters.
2. **Mute by default.** The plugin's default stays off (a safe insert). For golem's insert value the
   review recommends **on**: an engaged tuner on a stage is a silent one, and `always_on` covers
   watching the pitch while playing. → the maintainer.
3. **Placement.** One instance per instrument lane, first in the lane's own chain on both engines,
   inserted bypassed; the marker picks which one to engage. Not REAPER's input FX (§2.6). A bypassed
   instance is a copy. → golem-rack confirms.
4. **23 read-only parameters.** Kept, unpacked. 7 are read at 30 Hz, 16 only during a strum; packing two
   values in one float saves nothing that matters and costs a decoder on each engine. What the count
   does require is the `seq` guard around each poll (§2.6). → golem-rack: Designer hides `read_only`
   parameters as it does `gr`; the Golem Engine's Lua can read 7 – 23 values at 30 Hz.
5. **Poly on shared strings.** v1 flags them: accepted. The limit is physical (the two partials are less
   than 1 Hz apart) and every strum tuner has it; the flagged reading is still right in sign (§2.5). The
   two-component fit stays v2 and needs the maintainer's strum DIs first.
6. **In-tune band and settle times.** Lamp ±2 ¢ in Fast, ±0.3 ¢ in Strobe, golem-side (§2.6). Fast is the
   stage mode: settled in 0.2 – 0.75 s on a clean note. Strobe is the bench mode: 0.4 – 1 s, and up to 2 s
   on a hard-plucked low B, which is the string settling. → the maintainer, by feel, on the pedal.
7. **Inharmonicity.** Reported raw. Estimating B per string is possible later (Strobe, Mono, partials 1
   and 2 on two tracks, which §2.3's harmonic test already runs); not in v1.
8. **Reference.** 410 – 470 Hz is enough: it covers every pitch standard in use; x42's 220 – 880 Hz is an
   octave choice, not a need. `reference` is **one golem-wide setting written to every instance at
   insert**, not a parameter of the `tuner` block: a band has one A. → golem-rack.
9. **Sweetener sets.** The plugin is ready (Custom + `offset_n`). The values are data for golem.fm,
   later. → the maintainer.
10. **Hue and motif.** `#23bf18` is too close to gfx-glitch's lime; replaced by deep green `#126e1c` with
    a lamp-green rune light. Segment counts halved (Visual). → the maintainer, on the family sheet.
11. **CPU spike.** Removed at the source: the long YIN runs at a quarter of the rate (§2.2).

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good; Strong once the strobe is benched on DIs.** The architecture is the right one and the
best in the field: a coarse detector names, a phase measurement reads, the tuning is a prior, the
reading carries its own state, and everything leaves as read-only parameters. The survey holds: x42
Tuna's description was checked against `src/tuna.c` and `tuna.lv2.ttl.in` (8,192-point FFT at 48 kHz,
±5 % 4th-order band-pass, rising zero crossings, 2nd-order DLL, 220 – 880 Hz, −75 dB: all true), the
collision script was rerun, the Cramér–Rao figures recomputed. What did not hold was **the strobe as
specified**: a numpy prototype on synthetic plucks showed it slower than claimed, biased, blind to its
own error, and shaken by a neighbour string in Fast. The principle stays; the filter changed. The
prototype of the corrected design meets the precision claims, and its figures are now the document's.
Scripts: `proto-gfx-tuner/` in the review scratchpad (`zoom.py` + `t1.py` the first draft, `zoom3.py` +
`t4.py` the corrected strobe, `t3.py` the neighbour sweep, `poly.py` the strum, `coll2.py` the table).

Changes made, and why:

| # | Change | Why |
|---|---|---|
| 1 | **Strobe: narrow Butterworth + line fit → wide low-pass + nine Hann frames + line fit, on a finite, growing window** (§2.4) | As drafted, measured: low B first read at 0.9 s (Fast) and 1.9 s (Strobe) against 1.4 s claimed for Strobe, since filter settling and window add up; 0.13 – 0.35 ¢ of bias at A0 / B0 from the filter's last 1 % of transient; Fast's fit ran on 4 – 5 samples (fb = max(4B, 50 Hz), T · B = 0.64). A finite window forgets the attack exactly. Now: B0 settled in 0.74 s (Fast) and 1.03 s (Strobe), E2 in 0.32 / 0.47 s |
| 2 | **Error bar corrected; "settled" is a stability rule** (span of the last readings), not σ under a threshold | The residual formula counted correlated samples as independent: σ up to 7 times too small, and `state` 2 raised during a glide with the reading up to 5.7 ¢ off. The design's own Glide row would have failed. Worst error while settled is now 0.13 ¢ (Fast) and 0.04 ¢ (Strobe) |
| 3 | **Demodulation at the coarse frequency, not the target; capture guard ±35 ¢ around the coarse reading** | Strobe's band (0.04 f) cut a string 80 – 100 ¢ flat by 5 – 12 dB while the text kept it on its string; "re-target beyond ±50 ¢" contradicted the ±100 ¢ clamp. Now the clamp is display only, and a peg turn is followed without a restart (checked: half a window behind, settled 0.36 s after the stop) |
| 4 | **Neighbour strings stated with numbers; Fast's window 8 → 24 periods, Strobe's 30 → 48** | The drafted Fast swung ±6 ¢ with a 200 ¢ neighbour at −10 dB (the low E's octave under the D string), ±20 ¢ at 0 dB. Rejecting Δf takes about 2 / Δf of signal: 24 periods hold 0.3 ¢ at −10 dB, 48 periods 0.06 ¢. The bench row "E2 10 dB louder than the note read" was replaced by the cases that occur |
| 5 | **Long YIN at fd / 4** (25 – 150 Hz, 14 k MAC instead of 230 k); `prepare_decimated` and `set_floor` added to `detect`; `candidates()` defined | Removes the CPU spike instead of spreading it (question 11). `prepare_range` cannot take a pre-decimated stream; `detect` has a fixed −70 dBFS floor above `threshold`'s range |
| 6 | **Coarse latency restated**: window + one period + a hop (E4 20 – 30 ms, not 15) | A YIN window must lie inside the note; "2 periods" ignored the 16.7 ms window |
| 7 | **Open string or 12th-fret harmonic decided by the fundamental's presence** (§2.3) | The tuning prior accepts both f_i and 2 f_i, so it could not fix the very octave error it was meant to: an open low B read an octave up became "its harmonic", with the wrong `note` and the partial's inharmonicity |
| 8 | **Poly: clean / shared table redone for the 20 tunings with strings out of tune; windows from the table (4 / Δ)** | The 98 ¢ cases (G3 on 7- and 8-strings, D3 on the 8, G2 on a 5-string bass) were listed clean; out of tune they are not (7 ¢ errors). "Bass: none" was true of standard only. T = 40 / f gave the 5-string's G 0.41 s to split 5.7 Hz |
| 9 | **Poly decision at the first band reading, counting clean strings only**; refresh 50 ms; grid search once per strum | At 50 ms nothing is resolved, and one low E alone lights the B3 and E4 bands: it would have been called a strum |
| 10 | **Poly timing restated from the prototype**: clean strings ±1 ¢ at 0.3 s (guitar), 0.5 s (bass-4), 0.75 s (bass-5); shared strings 1 – 4 ¢ median, 6 – 14 ¢ worst | The bandwidth–time answer: PolyTune-like on guitar, physically slower on a bass |
| 11 | **`tuning`: golem sends Custom + notes always; enum reordered Chromatic, Custom, ids** (§3.1 – 3.2) | A dynamic catalogue list and an append-only enum cannot stay in step by rank; one path is simpler than two and needs no plugin release for a new tuning |
| 12 | **Poll guard** (`seq` read first and last), `seq` defined as running in silence, lamp rule moved to golem without requiring `state` 2, strobe drawing constant given in segments | 7 separate reads can straddle a hop; a lamp tied to "settled" stays dark when a neighbour rings; k = 1 turn / s per cent aliases on a 60 Hz screen at 3 ¢ |
| 13 | **Mute semantics written out** (ramp on bypass while muted, no effect when bypassed, the multiply skipped at g = 1); placement fixed to the lane's chain | Unspecified corners; one placement for both engines |
| 14 | **Bench**: top note C7 → F6; speed rows per stage; rows for error bar, neighbours, capture guard, glide at `state` 2, peg turn, open string vs harmonic, false strum, freshness, Custom = named; CPU bound lowered | C7 is above the coarse stage; the new mechanisms need their checks |
| 15 | **Hue `#23bf18` → `#126e1c` + rune light `#3ddc2e`; motif 24 / 48 / 96 → 12 / 24 / 48 segments** | ΔE*ab 19 (ΔE00 7) from gfx-glitch's lime at the same lightness and finish; 96 segments are sub-pixel on a tile |
| 16 | Facts: PolyTune strobe accuracy is quoted as ±0.02, ±0.05 or ±0.1 ¢ depending on the seller; "no Lua relay" softened (REAPER still needs a reader, the one that reads `gr`); Tuna's band-pass has a 10 Hz floor | Accuracy |

Still open:

| Point | Owner |
|---|---|
| Engine table: one catalogue `tuning` → `tuning` + `string_1…8`; `reference` as a golem-wide setting; placement first in the lane; the 30 Hz reader on each engine with the `seq` guard; Designer hiding the 23 meters | golem-rack |
| golem's `mute` insert value; the lamp bands (±2 ¢ / ±0.3 ¢); sweetener data; hue on the family sheet | The maintainer |
| Real glides and real neighbours: every [A] figure of §2.4 – 2.5 comes from synthetic plucks. The settle spans (150 / 300 ms, 0.2 / 0.07 ¢), the 20 dB harmonic test and the capture guard are to set on DIs | Bench, with the maintainer's guitar, bass and strum DIs |
| Fast growing to Strobe's window while unsettled; full confidence for a shared string whose peak is far from the lower string's partial; the two-component fit (v2) | Bench |
| The strobe above 1.5 kHz (violin, voice harmonics): needs a third coarse range; not in v1 | The maintainer, if asked |
| `zoom`'s frames estimator sits 2 – 3 times above the Cramér–Rao bound (one case): fine for a tuner, to remeasure before gfx-pitch or gfx-crusher reuse it | Bench |

## As coded (v0.1.0, 2026-11-15)

**Files.**
- gfx-dsp `zoom.rs` (new): `Zoom` (`start`, `push`, `estimate`, `set_capture`, `frame_power`,
  `grid_peak`, `refine`, `refine_pv`), `Estimate`, `cents`. 6 unit tests.
- gfx-dsp `detect.rs`: `prepare_decimated`, `push_decimated` (`push` now ends in it: same operations),
  `set_hop`, `set_floor`, `rate`, `window_s`, `candidates()`. One unit test. Bit-exact null: renders of
  gfx-pitch (default, Harmony, Tracking), gfx-pitch Poly, gfx-swell (two sources), gfx-ringmod,
  gfx-crusher, gfx-golem and gfx-octaver are byte-identical to the previous commit's.
- gfx-dsp `effect.rs`: `Effect::meter(id)` with a default `None` (§5's "meters() accessor"), so the bench
  and the tests read the read-only values. No other effect changes.
- `src/lib.rs`: the processor (`Tuner`, `Settings`, `Target`, `Counters`, `PARAMS`, `TUNINGS`,
  `TUNING_NAMES`, `METERS`). `src/plugin.rs`: the VST3, class id `GoleMFxTuner0001`, mono 1 / 1 first then
  2 / 2, latency 0, `bypass` = `make_bypass`, the 23 meters `read_only()` and published with
  `set_output_value` after each block. `src/tests.rs`: 21 behaviour tests (§5) + `survey` (§2.4's table)
  and `cpu`, ignored.
- `presets/gfx-tuner.json`: 14 presets from §4.

**Measured** (synthetic plucks as the review's prototype: partials 1 / k to 5 kHz, decay, −60 dBFS noise;
48 kHz unless stated).

| §5 row | Bound | Measured |
|---|---|---|
| Accuracy, Fast | ≤ 0.3 ¢ after `state` 2 | 0.038 ¢ worst over 138 notes (120 strings of the 20 tunings + 18 chromatic A0 – F6; offsets 0, ±3, ±20, ±49, −80, −100 ¢); slowest settle 0.78 s (A0) |
| Accuracy, Strobe | ≤ 0.1 ¢, σ ≤ 0.03 ¢, settled ≤ 1.2 s | 0.006 ¢, shown σ 0.000 ¢, slowest settle 1.09 s (A0 – E6 × 3 offsets) |
| Reference | 0.00 ± 0.05 ¢, every rate | 415 / 432 / 466 Hz at 44.1 / 48 / 96 / 192 kHz: within ±0.05 ¢, note 69 |
| Speed, coarse | E4 35, E2 45, B1 50, B0 95 ms | 20, 30, 40, 80 ms |
| Speed, strobe | first E4 0.08, E2 0.18, B0 0.45 s; settled Fast 0.25 / 0.35 / 0.8, Strobe 0.4 / 0.5 / 1.1 s | 0.06 / 0.16 / 0.43 s; 0.21 / 0.31 / 0.73 s; 0.36 / 0.46 / 1.03 s |
| Octave errors (synthetic) | no wrong note | low B, A0, A1 and E2 with the fundamental 20 – 34 dB under: their note and string from the first reading, nothing flashed, within 0.5 ¢. The DI rows (200 plucks) need the maintainer's recordings |
| Open string or harmonic | E2 open, harmonic 52 | bridge-pickup E2 (fundamental −20 dB): note 40, string 1. Even partials only: note 52. Even partials with the fundamental at −17 dB: YIN reads 2 f, the probe finds the fundamental, the strobe moves down (note 40, within 0.1 ¢); at −23 dB and under: the harmonic |
| 12th-fret harmonic, B = 2e-4 | +0.5 ± 0.1 ¢ | +0.518 ¢ over the open string's fundamental |
| Coincidence (DADGAD) | D2's harmonic → D3 | string 3, not a harmonic |
| Needle jitter | σ ≤ 0.1 / 0.02 ¢, no step > 0.5 ¢ | 0.000 / 0.000 ¢ (the dead band), no step |
| Error bar | within × 3; ≥ 0.5 ¢ with a 0 dB neighbour | σ 0.070 ¢ against a measured spread of 0.171 ¢ (× 2.4, noise at −40 dBFS); ≥ 5.4 ¢ |
| Neighbour strings | D3 + E2 −10 dB: 0.4 ¢ Fast, 0.1 ¢ and settled ≤ 0.7 s Strobe; −20 dB partial: 0.2 ¢ and settled; 0 dB: Strobe 0.2 ¢, never settled; G2 + B0 −10 dB: 0.1 ¢ | 0.295 ¢; 0.011 ¢ settled at 0.55 s; 0.071 ¢ (settled 0.42 s) and 0.018 ¢ (0.65 s); 0.187 ¢, never settled; 0.034 ¢ |
| Capture guard | within ±35 ¢; Strobe 0.1 ¢ | held at the edge for one hop, then 0.006 ¢ |
| Glide (8 ¢ / 150 ms, 20 ¢ / 250 ms; E1, E2, E4) | ≤ 0.3 / 0.1 ¢ at the first `state` 2; nominal ± 0.1 | ≤ 0.26 ¢ (Fast), ≤ 0.033 ¢ (Strobe); all end within 0.1 ¢ |
| Peg turn | within 8 ¢, no gap, no restart, settled ≤ 0.45 s after | 6.6 ¢ behind, no gap, no restart, settled 0.36 s after the stop |
| Poly | clean ±1 ¢ at T_i + 0.1 s; shared ≤ 0.5 and right sign | clean ≤ 0.08 ¢ (every tuning, 3 strums); shared ≤ 12 ¢, flagged unless separated, sign right beyond 10 ¢ |
| Poly, false strum; back to Mono | never 3; Mono after a pluck | a lone low string never reads as a strum in any tuning; a pluck after a strum shows its string |
| Mute | no burst > −50 dB; bit-exact | −124 dB (bench, every rate); bit-exact unmuted, bypassed, bypassed + `always_on`, bypassed + `mute` |
| Freshness | +1 per 10 ms | exactly 100 per second at 44.1 – 192 kHz, blocks of 32 / 333 / 2,048; frozen when bypassed, running with `always_on` |
| Tuning table | = golem's file; named = Custom | the 20 lists equal `~/golem/library/catalog/tunings.json`; DADGAD named and Custom give identical meters |
| CPU | Mono ≤ 1 %, Poly 8 ≤ 1.5 %, worst block ≤ 0.1 ms, bypassed ≤ 2 ns | 33 – 35 ns (0.17 %); 89 – 94 ns (0.45 %); 0.095 ms; 1.4 ns |

Strum shown (first `state` 3 after the strum, worst of 3): 6-std 0.51 s, eb-std 0.53, d-std 0.56, drop-d
0.36, dadgad 0.40, drop-c 0.40, 7-std 0.59, 7-drop-a 0.45, 7-gdgcfad 0.50, 8-std 0.84, 8-e 0.68, 8-drop-e
0.55, 8-dbdadgbe 0.61, bass-4-std 0.87, bass-4-eb 0.99, bass-4-d-std 1.04, bass-4-drop-d 0.66, bass-5-std
1.07, bass-5-high-c 0.87, bass-5-drop-a 0.94.

**Deviations from the spec, and why.**
1. **Poly windows T_i = clamp(5 / Δ_i, 0.25, 1 s)**, not 4 / Δ_i, and Δ_i taken on the side where the
   foreign partial is (the review's script took the upward distance for partials below too, 10 – 20 %
   too large for a neighbour 500 ¢ under). At 4 / Δ a neighbour 60 ¢ closer than nominal sits on the
   Blackman-Harris main lobe's edge: 1.5 ¢ of pull measured (A1 beside E1), 1.1 – 1.3 ¢ on string 2 of the
   basses and the 8-string. At 5 / Δ: 0.003 ¢. The strum reads later than §2.5's figures (list above).
2. **Poly refinement**: the full window's magnitude peak (Gaussian interpolation of three points a bin
   apart, recentred three times), not the phase difference of two 2T / 3 frames. The shorter frames have a
   main lobe of ±6 / T, so the neighbour the window was sized for is inside it. The magnitude of a real
   envelope is symmetric about its frequency, so a decay does not bias it. The design's estimator is kept
   as `zoom::refine_pv`; a unit test shows it pulled where `refine` is not.
3. **Capture guard as a clamp**, as in the prototype: the estimate is held at ±35 ¢ around the track's
   coarse reading and that hop shows the coarse reading only. A restart there would reset the window to 8
   periods, which can never split the louder neighbour: the track would restart forever. The track still
   restarts when the coarse reading leaves the wide filter (170 ¢, two hops running).
4. **The short YIN runs without harmonic lock, over 60 – 1,800 Hz.** With the lock, a D3 over a ringing
   E2 was read as D2 (the doubled lag is deeper with two notes) and the strobe had nothing to read. With
   1.5 kHz as the top, F6 + 49 ¢ (1,437 Hz) had its lag on τmin, went unvoiced, and the long window
   answered a sub-harmonic (143 Hz). Octave-high errors are the tuning prior's and the probe's job, as
   designed.
5. **Choosing between the windows** (the spec: short when voiced, else long). Two additions, both found on
   a weak-fundamental low B. The long window is used only when the short one's period estimate is under
   200 Hz. A barely voiced short reading (CMNDF over 0.1) that is no harmonic of the long window's clear
   reading (twice as deep) is a partial cluster: the long window wins. Before the long window can answer
   (40 ms + one 33 ms period), a barely voiced short reading waits, so no wrong note is flashed.
6. **The octave rule needs a 20 ¢ margin**: the other candidate wins only within 80 ¢ of a string, so a
   string 100 ¢ out does not flip between two strings an octave apart.
7. **Strum decision**: at least min(3, clean strings) and never fewer than 2 lit clean strings, so a
   one-string Custom tuning does not show every pluck as a strum.
8. **Full confidence back for a shared string**: its peak must lie 1.5 main lobes (6 / T) from **every**
   foreign partial within 150 ¢ whose string is lit. One lobe from the nearest one only left 2.2 ¢ on the
   8-e's G3, which sits between E1 × 5 and B1 × 3.
9. **No block carries the whole poly work**: one grid search per hop, each string's as soon as its own
   window is full, and the 50 ms refreshes staggered (string i on hops ≡ −i mod 5). The grid sums the
   windowed samples in fours (the grid spans ±0.04 fc at fb ≈ 4 fc). Worst block 0.095 ms.

**Choices where the spec was silent.**
- Onsets: peak followers on the decimated signal, fast 0.5 / 20 ms, slow 10 / 400 ms. An onset is fast
  over slow by 6 dB and above `threshold`, re-armed under 3.5 dB, at most one per 50 ms while sounding. A
  note that grows in without an onset starts where the level crosses `threshold`.
- The mono track starts at the first coarse reading after the onset and **replays the input since the
  onset** from a 4,096-sample history at fd, so a late coarse reading costs no strobe time.
- The coarse reading waits until the window lies inside the note: since the onset ≥ the window + one
  period of the reading.
- Note hysteresis: the first target after an onset or silence is taken at once; changes need 2 hops (and
  60 ¢ in chromatic).
- Settled: the span rule uses T_max (as the prototype), and needs a strobe reading at every hop of it.
- Level and `threshold`: the slow follower's peak in dBFS. The detectors' floors are `threshold` − 10 dB
  (they measure a mean square).
- The harmonic probe compares one Hann frame of 4 fundamental periods in both bands, once, past the
  hold-off: `> −20 dB` = the open string (the probe's track becomes the strobe), else the harmonic.
- Poly windows start 30 ms after the strum's first onset + 2 periods (the prototype's hold-off); a
  shared string's window is the longest clean one.
- Meters are published at each 10 ms hop and handed to the host once per block. `freq` is the strobe's
  reading, or the coarse one while settling; the strings' columns are 0 when no strum is shown.
- Expert groups: `tuning` joins Strings and `bypass` joins Detection (every parameter must be on the face
  or in one group).
- Presets: no "Sweetened Electric": its offsets are data for golem.fm (question 9), not values to invent.
  Presets about the lane's own tuning leave `tuning` out, so the export completes it as Chromatic. golem
  should re-apply the lane's tuning (Custom + `string_1…8`) after loading a tuner preset (golem-rack).

**Left for the owner.**
- The DI rows of §5: octave errors on 200 plucks per string, Poly on the maintainer's strum DIs, against x42 Tuna
  and the JSFX. Real glides, the settle spans (0.2 / 0.07 ¢), the 20 dB harmonic test and the onset
  constants are to set on those recordings.
- golem-rack: the engine table (`tuning` → Custom + strings), `reference` written to every instance,
  placement first in the lane, the 30 Hz reader with the `seq` guard, Designer hiding the 23 meters, and
  re-applying the lane's tuning after a tuner preset.
- The maintainer: `mute` at insert, the lamp bands, the hue on the family sheet.

