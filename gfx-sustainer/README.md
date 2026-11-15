# gfx-sustainer

> **Status: CODED** (v0.1.0, 2026-11-15). Designed and reviewed by Fable 5.2 on 2026-11-14 (Review
> section), then coded on the shared gfx-dsp `track` (gfx-ringmod's, extended with opt-in modes only:
> gfx-ringmod's and gfx-swell's renders byte-identical) and the new gfx-dsp `multipitch` (the API agreed
> with gfx-polysynth: `prepare(sample_rate, f_min, max_notes)`, this pedal passes 4).
>
> - **Feedback**: the string's own partial, phase-locked, adds only what the string has lost: the output
>   partial follows its command within 0.06 – 0.21 dB at E4 and 0.07 – 0.45 dB at E2 through a decaying
>   handover (B 0 – 3·10⁻⁴); every fixed `harmonic` (Sub … Two Octaves + Third) sits within 2 ¢ of the
>   string's own (inharmonic) partial at E2, E4, E6. The competition model measures c = 1.202, a 2×
>   challenger takes over in 0.49 s (dip 2.9 dB), `rise` lands within 1 ms (50 ms – 6 s).
> - **Sustain**: a 16-partial bank holds each partial within 0.7 dB of its target for 30 s, no AM, the
>   generated part's noise 51 dB under a −60 dBFS hiss; with B = 3·10⁻⁴ every held partial up to k = 16 is
>   within 1.7 ¢ of the string's own (the tracker now fits the string's inharmonicity).
> - **Chords**: `multipitch` names the lowest note of 200 random voicings 200 / 200 and the highest
>   183 / 200; a power chord never blooms on the sub-octave, with `delay` 0 it blooms 231 – 315 ms after
>   the strum.
> - **Bench**: 28 / 28 common checks; 23 behaviour tests. **CPU over budget** for Sustain (2.2 – 2.5 % of a
>   core against 1.5 %) and `chord` All (4.0 % against 3 %); Feedback 1.0 – 1.1 %, idle 0.9 %. See "As
>   coded" for every measurement and deviation.
> - Design by Claude Opus 5.5, reviewed by Claude Fable 5.2 on 2026-11-14. It brings two new gfx-dsp
>   modules, `track` (the phase-locked partial tracker, shared with gfx-ringmod) and `multipitch` (chord
>   analysis on the side chain), and reuses `detect` with the extensions gfx-tuner asks for.

golem's **sustainer and feedback generator**. Hold a note and it **blooms into "feedback"**: after a
delay its fundamental, or a chosen harmonic (octave, octave + fifth, two octaves), grows until it sings,
in tune with the note and following its bends. It plays the role of the DigiTech FreqOut, the EBow and
the magnetic sustainers (Fernandes, Sustainiac), but in the signal only: no string is driven. A second
engine, **Sustain**, keeps the note itself alive at a steady level, partial by partial, like an infinite
compressor that does not pump and does not raise the hiss.

It is a **new golem block** (`sustainer`). Checked on 2026-11-14 in `~/golem/library/catalog/effects.json`:
no block sustains a note by its pitch or simulates feedback. The nearest ones hold *a sound*, not *a note*:

| Block / pedal | What it holds | Pitch-aware | Dry | Latency |
|---|---|---|---|---|
| `freeze` (GoleM Freeze CLAP) | the whole spectrum at the gesture, a pad | no | untouched | frame |
| `glitch` (gfx-glitch) `sustain` | spectral bins, a pad | no | untouched | wet 53 ms |
| gfx-reverb freeze | an infinite diffuse tail | no | untouched | 0 |
| `comp` (gfx-comp) | the input, raised by up to `range` | no | compressed | 0 |
| gfx-swell | nothing: it removes attacks | onsets only | gated | 0 |
| gfx-grains (being designed) | grains of the input, looped | no | — | — |
| **gfx-sustainer** | **the note: its own partials, each phase-locked**, or one harmonic of it | yes | untouched by default | **0** |

## 1. State of the art

Survey of 2026-11-14. **[V]** read in a manual, a patent, a paper or source · **[M]** maker's page or
listing · **[F]** forum post or review · **[A]** computed here · **[I]** inference or not re-checked ·
**[U]** could not verify.

### 1.1 Pedals and devices

| Reference | How it works | What players say | What we take |
|---|---|---|---|
| **DigiTech FreqOut** (2016) | Pitch detection on the input [M], then a generated feedback tone. Knobs **Gain** (feedback level against the guitar) and **Onset** (delay / rate of the build-up) [V, manual mirrors]; **Type**: **Sub** (an octave below), 1st, 2nd, 3rd, 5th harmonic ("a multiple of your note's frequency": the 5th is two octaves and a major third), **Nat Low** (nothing at the 5th harmonic or higher), **Nat Hi** [V, manual]; **Dry** switch (kill the guitar); **Momentary** (feedback while the switch is held) vs latching (each picked note feeds back once the pedal is on) [V] | "near identical in tone and onset" to real feedback; sustains "about as long as you can keep a vibrato going", not forever [F, Fractal forum]; Nat Low / Hi "act more like natural feedback… may produce different feedback harmonics every time you play the same note" [V, Sound On Sound]; chords "less predictable", unwanted strings must be damped [V, SOS]; weak on anything but single notes [F, Thomann]; attack and decay a little too slow to replace an EBow [F, Thomann]; a "chirp or whistle" at the end of the feedback [F, Fractal forum]; hiss [F, Line 6 forum] | The control set (delay, rise, level, type, dry, momentary); Natural types; a fast minimum rise; an ending without chirp |
| **EBow** (Plus) | A hand-held driver: pickup coil → amp → drive coil over one string. **Standard** favours the fundamental; **Harmonic** "rich upper harmonics" [V, players' guide], usually the octave [M]; harmonic mode attributed to a reversed drive polarity [I, wiki] | Violin / cello swells; only one string | The bowed swell (dry off, short rise); harmonic mode = the octave |
| **Fernandes Sustainer**, **Sustainiac** | A driver pickup fed by the bridge pickup's amplified signal [V, Sustainiac manual]. Fernandes modes: **Natural** (the fundamental, as long as the system is on), **Harmonic** ("within a second or two the ringing note rises to a higher harmonic"), **Mix** (both) [V, fernandesguitars.com]; chords "may develop at different rates" [V]. Fernandes closed in 2024 [M] | Infinite sustain at any volume; the harmonic jump is the signature | `Climb` (fundamental, then the octave takes over); Sustain mode; chords: each note on its own (`chord` All) |
| **Gamechanger Audio PLUS** | Piano-style sustain by **sampling**: short slices of the instrument looped, no oscillator or filter [V, manual]; Rise and Tail knobs; up to 5 layers; FX loop on the sustained part [V] | Keeps the instrument's tone, works on chords | Not our method (sampling = gfx-grains / gfx-glitch territory); its Rise / Tail law is a reference for Sustain |
| **Boss DF-2** (1984) | Distortion + "Super Feedbacker": hold the switch, a harmonic of the detected note sustains [V, Roland review] | "very synthetic", "even vibrato wobble", "click-thunk" when stepped at the wrong moment [F] | What to avoid: a free oscillator, an automatic vibrato, a hard retrigger |
| **Boss FB-2** (2011) | Booster; hold the switch: feedback; FEEDBACK knob "adjusts the time taken… to feedback" [V, Roland press release]; the feedback follows bends, vibrato, hammer-ons [M]; cannot feed back without the boost [F, MusicRadar] | Low settings "amp-like", high settings "near a magnetic sustainer" [M] | Bends must be followed; momentary use |
| **Boss GT-6 "FB Feedbacker"** | Mode **OSC / Natural**, Rise Time, F.B. Level, Vibrato Rate / Depth [V, partial manual page] | — | Two families (oscillator vs natural) |
| **Fractal Axe-Fx III / FM** | No feedback block [I]. Players build it from the **Synth** block (3-voice mono tracking synth, sine voices at +12 and +19 st) or the Pitch Follower; Synth tracking stops near 47 Hz [F, Fractal forum] | "You can't vary the frequency by moving around" [F] | Natural mode with a `distance` the foot can move |
| **Line 6 Helix** | No feedback simulator; players put a FreqOut in a loop [F, Line 6 forum threads] | — | Room for a block that does it right |

### 1.2 Research and patents

- **Sullivan, "Extending the Karplus-Strong algorithm to synthesize electric guitar timbres with
  distortion and feedback"**, Computer Music Journal 14 (1990) [I: cited by Karjalainen, Välimäki et al.;
  not read]. A synthesised string whose output, distorted and delayed, is fed back into the string loop;
  the delay sets which harmonic feeds back.
- **Softube patent US 7,572,972 B2** (F. Gustafsson, filed 2005, granted 2009, **expired 2021** for unpaid
  fees) [V, Google Patents]. A pedal-style feedback emulator: input + loop → **nonlinear amplifier**
  (arctan) → **room** (delay T and attenuation α; a volume stands for the distance) → **string** (band-pass
  filters at harmonics of the detected fundamental) → back to the sum. Describing-function theory: a static
  nonlinearity in a linear loop gives a stable oscillation. It notes that which harmonic dominates is
  "unpredictable" and keeps a table per fundamental. Pitch: harmonic-summed DFT energy, coarse then fine
  stage; feedback only above an energy gate. Free to use now.
- **Gabrielli, Giobbi, Squartini, Välimäki**: "A nonlinear second-order digital oscillator for virtual
  acoustic feedback", ICASSP 2014, pp. 7485 – 7489; and "Adaptive digital oscillator for virtual acoustic
  feedback", AES 136th Convention 2014, paper 9066: a nonlinear oscillator tuned by real-time pitch
  detection, stable amplitude, low cost [V: records and abstracts; equations not read].
- **Costello**, musicdsp.org "Guitar feedback" (2002): feed the output delay line back into the
  Karplus-Strong lines; feedback amount and delay length set intensity and pitch [V].
- **Klapuri, "Multiple fundamental frequency estimation by summing harmonic amplitudes"**, ISMIR 2006
  [I]: iterative salience and cancellation, the reference for chord analysis.
- **Välimäki, Laakso, Mackenzie** (ICMC 1995) on transients in time-varying all-pass fractional delays
  [I]: how to move a lossless loop's length without clicks. Not needed since the review: Sustain has no
  loop (§2.5).

### 1.3 How real guitar feedback behaves [A, I]

The loop is string → pickup → amp → speaker → air → string. The string is the high-Q element, so:

1. **The oscillation is a string mode.** The loop can only sustain where the string resonates: the
   fundamental or one of its harmonics, at the string's real (slightly inharmonic) frequency. So real
   feedback is always in tune and follows bends: the player bends the oscillator itself.
2. **Which mode wins is a phase question.** A mode builds when the loop phase at its frequency is near
   0 (mod 2π). The air path alone is τ = d / 343 m/s; for harmonic k of f0 the phase repeats every
   **λ_k = 343 / (k · f0)**: 52 cm for the octave of E4 (330 Hz) [A]. That is why taking a step changes
   the harmonic, and why the sweet spot moves with the note.
3. **Bends move the winner.** At 1.5 m (τ ≈ 5.4 ms with 1 ms of electronics), a whole-tone bend of E4 moves
   the octave's loop phase by 2.7 rad [A]: another mode can take over mid-bend. Players know it as "the
   feedback jumped".
4. **Gain sets speed and reach.** More gain: faster build-up, and higher modes can pass unity too (higher
   squeals at high gain).
5. **Mode competition.** All modes share one saturating amplifier: the first mode to grow takes the gain,
   the others die (winner-take-all). A change of winner is a cross-fade over a few hundred ms, not a cut.
6. **Onset is exponential.** The mode grows from the string's own level by a fixed number of dB per second
   (loop gain), then saturates softly. A quiet harmonic needs longer to become audible.
7. **Muting kills it**; a new pick restarts the process on the new note; a hammer-on while feeding back
   moves the oscillation to the new note at once (the string is still driven).

### 1.4 What we conclude

- **A free oscillator is the DF-2's mistake** (synthetic, beats against the string). The generated tone
  must be **the string's own partial, phase-locked**, so that the bloom grows *out of* the note.
- **A literal feedback loop is not needed.** Real feedback needs the loop because the string is the
  oscillator. We cannot drive the string, so the loop's role (growth law, mode choice, saturation) is
  modelled on the control side, and the audio is a phase-locked partial whose amplitude follows that
  model. Benefits: the detector always hears the **clean input** (no self-contamination), the level is
  bounded by construction, and the pitch is the string's.
- **The musical model of feedback** (§1.3) is cheap: a handful of mode amplitudes with a shared
  saturation and a phase-dependent gain per mode. That gives FreqOut's Natural types a physical meaning,
  and a `distance` the foot can move.
- **Sustain** must not amplify the input (that raises the hiss and pumps). It is the same idea applied
  to **every partial**: each one is held on its own frequency, by as much as it has lost. (A
  pitch-synchronous loop, Karplus-Strong style, was the first draft: a loop is periodic and a string is
  not, §2.5.)
- **Chords**: the onset delay (usually ≥ 300 ms) hides the chord analysis on the side chain, so windows
  long enough to separate the notes cost no audio latency; with a shorter `delay` the bloom waits for
  the chord to be named (§2.2). One note feeds back (Lowest / Highest / Strongest), or every note on its
  own.
- **Honest limit**: the real string keeps decaying. Bends are followed while the tracked partial is
  above about −72 dBFS (`thresh` − 6 dB), which on a clean DI is most of the string's own sustain ([U] in
  seconds, to be measured); after that the feedback holds its pitch. The FreqOut has the same limit
  ("as long as you can keep a vibrato going").

## 2. Design

### 2.1 Signal path (mono; the 2 / 2 layout runs the same voice on both channels from the mono sum)

```
in ──┬──────────────────────────────── dry gain (`dry`, `duck`) ──────────────┐
     │                                                                         ├─ sum ─ out
     └─ detection (§2.2) ─ note state (§2.3) ─┬─ Feedback voice (§2.4) ────────┤
        (or the Key input, `source`)          └─ Sustain bank (§2.5) ──────────┘
```

Nothing on the audio path waits for an analysis: **zero latency**, nothing reported. The generated parts
start at zero and are added on the string's own partials.

### 2.2 Detection (side chain)

- **Pitch, single notes**: gfx-dsp `detect::PitchDetector` (YIN at ~12 kHz, hop ~2.7 ms, harmonic lock
  on), f_min = `lowest` (30 – 150 Hz, default 70 Hz), with gfx-tuner's extensions (`candidates()`,
  `set_floor()` from `thresh`).
  - First reading = the window (1 / f_min) + one period of the note + up to one hop (the gfx-swell and
    gfx-tuner reviews' rule): 29 ms for E2 at 70 Hz, 55 ms for E1 at 35 Hz [A].
  - **`lowest` under 60 Hz** uses gfx-tuner's long detector (fed at fd / 4 through `prepare_decimated`):
    a 30 Hz window at 12 kHz costs about four times the guitar setting [A, from gfx-tuner §2.2].
  - YIN names the note and feeds the tracker forward. It is never the generator's frequency: its
    ±5 ¢ would beat against the string.
- **Phase lock: gfx-dsp `track`** (new module, shared with gfx-ringmod; API in §2.7). The review's
  prototype changed the phase detector of gfx-ringmod's design: the loop, its feed-forward and its engage
  rules are kept.
  - **Reference (the fundamental).** The input is multiplied by e^(−j·2π·ψ) (ψ = the NCO's phase, f64) and
    low-passed by two one-poles at f0 / 3: the result z = d₁ · e^(j·e) gives the partial's level d₁ and
    the phase error e at once. A PI loop (damping 0.707, bandwidth **clamp(f0 / 5, 14, 50) Hz**,
    correction ±1 st) steers the NCO; its feed-forward is YIN's f0 smoothed over 30 ms.
  - **Why not the band-pass pair in front of the loop** (gfx-ringmod's draft): two SVFs of Q 4 delay the
    string's phase modulation by 2.55 periods before the loop sees it, and retuning them from YIN
    disturbs them. Measured on a 6 Hz, ±30 ¢ vibrato at E4: generator against string 0.9 rad
    peak-to-peak with the pair, 0.2 – 0.3 rad with the demodulator, whose filter is inside the loop at
    baseband and is never retuned [A, prototype]. With the fundamental 12 dB under the 2nd partial the
    loop still holds (0.2 – 0.5 rad).
  - **Partials k = 2 … K**: oscillator k runs at **k · ψ + φ_k** (accumulated: its increment is k times
    the reference's, plus the trim; never k times a wrapped phase). The same demodulator at that phase
    (four one-poles at 0.4 · f0: the neighbours, f0 away, 34 dB down) gives d_k and its error; a PI loop
    of **4 Hz** moves the trim, stored as a **ratio** (±15 ¢), so a bend followed by the reference
    carries every partial with it.
  - Why the trim: string inharmonicity puts partial k at k · f0 · √(1 + B k²): with B = 10⁻⁴, partial 4
    is 1.4 ¢ sharp, partial 6 3.1 ¢; with B = 3·10⁻⁴ (wound strings, [I]) 4.2 and 9.3 ¢ [A]. At
    1.3 kHz, 1.4 ¢ is a full cycle of phase per second: without the trim, a "seamless" bloom would beat.
  - **Engage.** Feed-forward valid, d above the hold level, |e| settled (about 5 periods after the first
    YIN reading). A generator that is still silent **snaps** ψ onto the input's phase (nothing is heard);
    one that is sounding (legato, re-lock) keeps ψ and pulls the offset in through the frequency, at most
    5 ¢. gfx-ringmod always uses the second rule.
  - **Hold.** A partial is tracked while d_k > `thresh` − 6 dB (the prototype holds lock down to about
    −73 dBFS under hiss of −70 to −60 dBFS RMS; at −77 dBFS it is lost) and the variance of e over 50 ms
    stays under (0.5 rad)². Below, its trim freezes. When the reference itself is lost, the NCO runs at
    the **mean of its last 150 ms** (one vibrato cycle: the centre of a vibrato, not the point where it
    stopped).
  - **Weak fundamental** (d₁ more than 20 dB under the strongest of partials 2 – 3 at the body, some
    bridge-pickup low notes): the reference loop runs on that partial k_r and ψ advances by 1 / k_r of
    its increment [bench].
- **What the tracker achieves, and its limit** [A, prototype: decaying strings with inharmonic partials,
  hiss, YIN emulated with 25 ms of lag and ±3 ¢ of error]. Ripple = the swing of partial k in the
  **output** around its commanded level, the generator added to the string:

  | Case | E4 (330 Hz) | E2 (82 Hz) |
  |---|---|---|
  | Decay alone, B up to 3·10⁻⁴, k ≤ 6 | ≤ 0.1 dB | ≤ 0.5 dB |
  | 200 ¢ bend in 150 ms and back, k = 2 – 4 | 0.1 – 0.2 dB | 0.75 dB |
  | Vibrato ±30 ¢, 6 Hz, k ≤ 2 | 0.1 dB | 0.2 – 0.9 dB |
  | Vibrato ±50 ¢, 6 Hz, k = 3 – 4 (E2: ±30 ¢, k = 4) | 0.2 – 0.4 dB | 1.2 dB |
  | Vibrato ±30 ¢, k = 6 – 8 | — | 2.7 – 4 dB |

  A static phase error is harmless (30° costs 0.02 dB, 90° 0.4 dB): what must be exact is the frequency.
  The limit is the phase error under fast pitch motion, which grows with k and with the period: on low
  strings, a wide vibrato makes the upper partials breathe by a few dB while string and generator have
  comparable levels. It is an amplitude wobble at the vibrato's rate, as a real pickup gives; it is
  never a beat, and it ends when the string has faded.
- **Body level** b: the note's level once the pick transient is over: a sliding maximum over one tracked
  period (`envelope::SlidingMax`, the gate review's lesson: no ripple on low notes), taken 80 ms after the
  onset. Each partial's body level b_k is its d_k at the same instant.
- **Onsets**: gfx-dsp `onset` (gfx-swell's reviewed module): its pick detector A; **legato** note changes
  by its pitch-jump detector C (a stable pair ≥ 70 ¢ from a stable reference: bends and vibrato never
  count).
- **Mute**: the period-synchronous level falls by more than D dB within 60 ms, D from `mute_sens`
  (30 dB at 0 % → 10 dB at 100 %); or it reaches `thresh` having fallen more than 6 dB in the last
  60 ms (a mute of an already quiet string has less than D left to fall). A string's natural decay is
  1 – 3 dB in 60 ms; a palm or hand mute is 20 – 40 dB [I, to be measured on the DIs].
- **Touch**: once the string is below `thresh` a mute cannot be seen, so an onset (the hand's noise) with
  no new pitch confirmed within 60 ms ends the note. [U: touch noise level on a quiet DI; bench.]
  A hand laid slowly on a dead string gives neither: `max_time` is the exit (§2.3).
- **Chords: gfx-dsp `multipitch`** (new module, side chain only).
  - The input decimated to 6 kHz (the decimator design of `detect`), Hann FFTs of **512, 1,024 and
    2,048 points (85, 171, 341 ms)** every 256 samples (43 ms), each started at the onset;
    Klapuri-style harmonic summation with iterative cancellation, up to 4 notes from `lowest` to 1.2 kHz,
    each with a salience and a parabolic-interpolated f0.
  - The windows follow gfx-tuner's rule (T = 4 / Δf): 85 ms separates notes 47 Hz apart, enough for the
    root and fifth of every power chord above G2 (49 Hz apart; E2's are 41 Hz apart and wait for the
    171 ms window); 341 ms separates 12 Hz, and a minor third at E2 is 16 Hz [A].
  - It differs from gfx-tuner's strum reading on purpose: the tuner knows which few hertz to look at
    (its tuning) and uses one `zoom` band per string; here the notes are unknown.
  - It runs whenever YIN's clarity is poor (CMNDF > 0.2), whatever the note state, so `feed` pressed on a
    ringing chord has its answer already.
  - A reading is **accepted** when two successive analyses agree on the notes (±50 ¢), from any window.
  - YIN alone would often name the **sub-octave** of a power chord (root, fifth and octave share a period
    of 2 / f) [A]: that is the error `chord` avoids.
- **When `delay` is shorter than the analysis.** A note with a clear YIN reading (single notes) never
  waits. When the clarity is poor at the end of `delay`, **Wait is extended until `multipitch` accepts a
  reading, at most 400 ms after the onset**; past that, YIN's candidate is used. So a chord never blooms
  on YIN's sub-octave and then jumps; the cost is a bloom up to 0.1 – 0.4 s late on chords, with `delay`
  under 400 ms only. The audio is never delayed.
- **`chord`**: Lowest · Highest · Strongest (largest salience) · All (up to 4 notes, each with its own
  voice and its own tracker, fed forward by the `multipitch` f0s). In All, a partial of one note within
  0.4 · f0 of a partial of a lower note (E2 × 3 and B2 × 2) belongs to the lower note only: two trackers on
  one frequency would both supply the same loss.

### 2.3 Note state

Per voice: **Idle → Wait (`delay`) → Rise → Hold → Release**, exposed as the `state` meter.

- **Start**: `trigger` Auto: a confirmed note (pitch voiced, level above `thresh`) starts Wait. Switch:
  only while `feed` is on; turning `feed` on with a note ringing starts Wait at once (the FreqOut's
  momentary use, golem's footswitch on `feed`).
- **New pick** on another note, or the same note re-picked: the old voice Releases, a new one starts
  (two voices alternate, so the release and the new wait overlap).
- **Legato** change (hammer-on, pull-off, slide): `carry` on: the voice keeps its state and level; its
  NCO glides to the new note within 30 ms and re-locks without a phase jump (§2.2, Engage); its partial
  levels d_k are read again on the new note (the sustainer / EBow behaviour). `carry` off: as a new pick.
- **Mute / touch / `feed` off / `max_time` reached**: Release.
- **`max_time`** counts from the start of Rise. In Auto, nothing else ends a note whose string has died
  under a hand laid on it without a sound, so its default is 15 s, not "no limit".
- **Release**: the generated amplitude fades over `release` with a raised cosine; **the frequency is held**
  (the loop stops correcting; the proportional term is dropped, the integrator kept), so the end can
  never chirp (the FreqOut complaint).
- **Bypass**: `make_bypass`; with `trails` on, bypass acts as a Release of every voice and the dry stays
  at unity; with `trails` off, a 10 ms fade.

### 2.4 Feedback voice

**Output** = Σ_k g_k · C(k · ψ + φ_k), k ∈ the modes of `harmonic` (below), where g_k ≥ 0 is the
generated amplitude and C a phasor with **colour harmonics**:
- C(φ) = cos φ + c₂ · cos 2φ + c₃ · cos 3φ, with c₂, c₃ growing with the level as a soft saturating
  pickup would (Chebyshev-style: c_n ∝ (g / b)^(n−1)), at the model's own −30 dBc at full bloom when
  `color` = 50 % [voicing]; 0 % = pure sine, 100 % = twice. Because the harmonics are computed, not
  produced by a waveshaper, **nothing aliases** (terms above 0.45 fs are dropped).
- The voice is designed to sit **before** a drive or amp, as the FreqOut's manual asks: the amp gives
  the squeal its grit. Placed after the amp, use `source` Key with the clean DI on the auxiliary bus.

**Seamless bloom.** Let a_k(t) be the commanded total level of partial k (string + generated), d_k(t)
the dry partial's measured level and γ the dry gain actually applied (`dry`, `duck`). Then
**g_k = max(0, a_k − γ · d_k)**: the generated partial supplies only what the string has lost, on the
string's own frequency. At the start a_k = d_k, so nothing changes at the instant the bloom starts (no
doubling, no step); then the partial grows out of the note. Measured: §2.2's table.

**Growth law and mode competition** (control rate 1 ms, f64). For each mode k of the voice, with
u_k = (a_k / A)²:

  d(ln a_k)/dt = β · [ G_k / G_max − u_k − c · Σ_(j ≠ k) u_j ],  c = 1.2

- A = b · `bloom` (the target level); G_k the mode's loop gain; G_max = max over the voice's modes.
- **Rise.** Alone, a mode follows a logistic in u: it reaches A − 1 dB at `rise` when
  **β = ln(3.86 · (1 / u₀ − 1)) / (2 · `rise`)**, u₀ = (a_k(0) / A)² (closed form, checked: 1.199 s for
  1.2 s). The curve is dB-linear, then a soft knee: the shape of real feedback (§1.3, 6). A partial that
  starts at or above A (the fundamental with `bloom` ≤ 0 dB) has no rise: it falls to A and stays.
- The start: a_k(0) = max(d_k, A · 10^(−40/20)) (a missing partial starts 40 dB under the target).
- **Winner-take-all, with hysteresis** (Lotka-Volterra competition, c > 1). From equal seeds the mode with
  the highest G_k wins. A mode that already sings **keeps the voice until another's G exceeds its own
  by c** (1.6 dB): between the two both states are stable [A, simulated]. That is real feedback's
  behaviour (a squeal holds through small moves) and it stops a vibrato from flipping the winner. The
  string's own partial levels are the seeds, so a brighter attack favours a higher mode, as the FreqOut
  manual describes for its Natural types.
- **Jumps.** Losers rest at A · 10^(−30/20), and once a mode has reached Hold the competition runs at
  β_j = max(β, 17 s⁻¹). A challenger with twice the holder's gain then takes over in ≈ 0.5 s, four times
  in ≈ 0.3 s, 1.3 times in ≈ 2 s; the total level dips 3 – 5 dB on the way [A, simulated; voicing]. (As
  drafted, with c = 1.5, a −90 dB floor and β from `rise`, the same jump took 9 s at the default `rise`,
  and a challenger under 1.5 times the gain never won.)
- **`duck`**: dry gain γ = 1 − `duck` · p(t), p = the bloom's progress (0 → 1): at 100 % the note turns
  into its harmonic (the Fernandes / EBow harmonic sound), at 0 the dry is untouched.

**`harmonic`** sets the modes and their G_k:

| Option | Modes | G_k |
|---|---|---|
| Sub | k = ½ (its own accumulator, at half the reference's increment) | 1; seed −40 dB (no string partial to grow from) |
| Fundamental | k = 1 | 1 |
| Octave | k = 2 | 1 |
| Octave + Fifth | k = 3 (the twelfth) | 1 |
| Two Octaves | k = 4 | 1 |
| Two Octaves + Third | k = 5 | 1 |
| Natural Low | k = 1 … 4 | w_k · (1 + cos Φ_k) / 2, w = [1, .9, .7, .5] [voicing] |
| Natural High | k = 1 … 6 | same, w = [.3, .8, 1, .9, .7, .5] [voicing] |
| Climb | k = 1, then 2 | G₁ = 1 until `rise` + 1 s after the start, then G₁ = .6, G₂ = 1: the note sings, then rises to the octave (Fernandes Harmonic) |

- These are the FreqOut's seven types [V, manual: "harmonic is defined as a multiple of your note's
  frequency"; its 3RD is the twelfth, its **5TH is the 5th harmonic**, two octaves and a major third],
  plus Two Octaves and Climb.
- **Natural Low stops at k = 4**, as the FreqOut's: "doesn't allow feedback at the 5th harmonic or
  higher… you don't get feedback that sounds 'major' on minor chords" [V, manual]. Two Octaves + Third
  carries the same warning.
- **Natural loop phase**: Φ_k = 2π · f_k · τ + φ_s, with f_k = partial k's tracked frequency,
  τ = `distance` / 343 m/s + 1 ms (pickup, amp, speaker [voicing]), φ_s = 0. So: the harmonic that sings
  depends on the note, the distance and the bend, as in §1.3. Natural is deterministic: the same
  note, attack and distance always give the same harmonic; `drift` (0 – 100 %) adds a slow seeded random
  walk of the distance (±0.3 m at 100 %, 0.1 Hz) so long holds can wander between harmonics, as a player
  swaying in front of the amp.
- A plain fifth (3 : 2) is not a string mode; "Octave + Fifth" is the twelfth.
- **Changing `harmonic` while a note blooms** changes the G_k: the competition cross-fades. No click by
  construction.

### 2.5 Sustain: the partial bank

The **Sustain** engine keeps the note's own sound, not one partial. It must not amplify the input (hiss,
pumping). It is the Feedback voice's law applied to **every partial at once**:

- **Bank**: partials k = 1 … K of the tracker, K = min(16, ⌊min(5 kHz, 0.45 fs) / f0⌋) (8 per note in
  `chord` All). Each has its target **A_k = `bloom` · b_k · k^(`tone`)** (b_k = its level at the body;
  `tone` ±100 % = ±6 dB per octave of partial number, 0 = the body's own spectrum [voicing]) and
  **g_k = max(0, a_k − γ · d_k)**, with a_k following §2.4's growth law without competition (c = 0,
  G = 1) from the end of `delay`.
- **What is heard**: each partial decays from its body level to A_k, then **stays there**, on its own
  frequency. With `bloom` under 0 dB there is no rise at all (the partial simply stops falling); `rise`
  matters when `bloom` asks for more than the body.
- **No hiss, no pumping, by construction**: the dry is never amplified and the added part is a sum of
  sines; no gain rides the output. A compressor at infinite ratio would instead raise the input by
  40 – 60 dB, hiss and pick noise included, and its release pumps on each re-pick.
- **Bends** are followed as in Feedback: the trims are ratios, so the whole bank follows the reference;
  when the string is lost the bank holds its pitch (§2.2).
- **The held note stays alive**: each oscillator keeps its partial's real, slightly inharmonic
  frequency, so the partials drift slowly against one another, as on the string. A looped period
  cannot do that.
- **Chords**: `chord` All gives one bank per note (up to 4 × 8 partials; shared partials belong to the
  lower note, §2.2). Any other `chord` holds one note and lets the rest decay.

**Why not a period-tuned loop** (the first draft: a lossless comb s = (1 − g) · x + g · s[n − P], frozen
at g = 1, its length following the pitch through a Thiran all-pass). Prototype, E2 with B = 10⁻⁴, g = 0.95
[A]:
- a loop is exactly periodic and a string is not. Before the freeze the comb holds partial 8 12 dB low
  and partial 10 18 dB low (their phase turns by 2π · k · δ_k per pass); with B = 3·10⁻⁴ partial 6 is
  already 10 dB down;
- after the freeze those partials are on k / P while the string's are 1 – 12 Hz away: they **beat**, by
  4 – 10 dB peak-to-peak from partial 6 up (15 – 37 dB with B = 3·10⁻⁴), for the seconds the handover
  lasts. "It supplies exactly what the note has lost" held for the first four partials only. The bank,
  same string: ≤ 1.5 dB on every partial, measurement leakage included;
- a tracking error of 5 ¢ on P costs partial 6 another 7.5 dB (comb detuned by 2π · k · ε per pass), so
  the loop is no better than the bank under vibrato;
- its other parts go away with it: the all-pass in a moving loop (gfx-dsp `delay` forbids it for a moving
  read, for its transients), the per-pass `tone` (a per-pass tilt in a loop that turns 82 to 1,300 times
  a second has no pitch-independent meaning), the loop limiter, the excitation gate.
- The draft's figure for the alternative interpolator was right: a 3rd-order Lagrange at fraction ½
  loses 0.036 dB per pass at 5 kHz (48 kHz), 36 dB / s in a 1 kHz loop [A, recomputed].

**`mode`**: Feedback · Sustain · Both (the note held at A_k, and its harmonic blooming on it: the
Sustainiac / Fernandes Mix sound). One bank serves both: in Both, a partial that is also a Feedback mode
takes the larger of its two targets. `duck` lowers the dry and the held partials together.

### 2.6 Latency, CPU, real-time

- **Latency: 0**, nothing reported. The dry is untouched (one gain). All analyses are side chain: they
  delay *when* the bloom may start (YIN 29 ms at 70 Hz, lock ≈ 5 periods, chords 0.1 – 0.4 s), never the
  audio. Earliest bloom on a low E2: ≈ 90 ms after the pick [A]; the minimum `rise` is 50 ms.
- **CPU** [A, estimate, to measure]: YIN as in gfx-ringmod (≈ 11 M MAC / s, 8-lane sums); per partial one
  complex multiply, four complex one-poles, a PI and a phasor ≈ 40 flops / sample; `multipitch`
  ≈ 1.5 M flops / s while active. Feedback, one voice (6 partials + colour): **≈ 1 % of a core** at 48 kHz.
  Sustain, one voice (16 partials): ≈ 1.5 %. `chord` All (4 × 8 partials): ≈ 3 %. The baseband
  filters and loops may run at fs / 8 (their signals are under f0 / 2) if the bench asks; the
  multiplies and phasors stay at fs. Target to measure: ≤ 3 %.
- Allocation in `prepare` only (FFT buffers and plans, YIN sized for 30 Hz at 192 kHz). Phases and
  recursive state f64. Flush-to-zero.
- **Level bound**: the generated part never exceeds Σ a_k ≤ `bloom` · Σ b_k · (1 + c₂ + c₃) per voice:
  no loop, nothing to limit.
- **Determinism**: `drift`'s random walk has a fixed seed, reset on `reset()`; every state zeroed.

### 2.7 gfx-dsp reuse and new code

| Reused | For |
|---|---|
| `detect` (+ gfx-tuner's `candidates`, `set_floor`, `set_hop`, `prepare_decimated`) | Pitch of single notes; the long detector under 60 Hz |
| `envelope::SlidingMax` | Body and mute levels, one period |
| `svf` | Detector filters, `multipitch`'s decimator |
| `smooth`, `exp_smooth` | All user parameters; EXP on `rise`, `bloom`, `distance`, `dry`, `duck` |
| `onset` (gfx-swell's module) | Picks and touches (detector A), legato (detector C) |

| New | Content |
|---|---|
| **`track`** | The phase-locked partial tracker, **one module for gfx-ringmod and gfx-sustainer** |
| **`multipitch`** | Decimated FFTs (realfft, as `pv_shift`), harmonic summation with cancellation, ≤ 4 notes |
| in the crate | Note state machine, mode-competition model, colour phasor, partial-bank targets |

**The tracker family in gfx-dsp**, one job each: `detect` names a note (YIN, ±5 ¢, 20 – 60 ms); `zoom`
(gfx-tuner) reads it to 0.1 ¢ over 8 – 48 periods; `psola` (gfx-golem, gfx-correct, gfx-formant) marks a
voice's cycles; `multipitch` names the notes of a chord; **`track` follows a partial's phase**.

```
track::Tracker                               // one harmonic series: a reference partial and K trims
  prepare(sample_rate, max_partials)         // allocation here only
  reset()
  set_f0(hz, confident)                      // feed-forward, control rate: from detect, multipitch or zoom
  set_floor(dbfs)                            // hold level (`thresh` − 6 dB here, `thr` in gfx-ringmod)
  engage(Engage::Snap | Engage::Keep)        // Snap: silent generator, ψ := the input's phase. Keep: ψ continuous
  process(x) -> Lock { psi, inc, level, locked }       // per sample; psi in turns (f64), inc in turns / sample
  partial(k) -> Lock                         // k · ψ + φ_k, its increment, d_k, its own lock flag
  hold()                                     // every frequency frozen at its 150 ms mean (Release, lock lost)

multipitch::Analyzer
  prepare(sample_rate, f_min)   reset()   push(x)
  notes() -> &[Note { hz, salience }]        // ≤ 4, sorted by pitch; empty until a reading is accepted
  age_ms()                                   // time since the onset the reading belongs to
```

gfx-ringmod uses `Tracker` with no trims (K = 0) and `Engage::Keep`; its carrier is φ_c += r · `inc`, and
its `phase` parameter reads `psi`. Its README still describes the band-pass pair of its own review: to
be updated when `track` is coded (it is not edited here).

## 3. Parameters

### 3.1 Decisions

- New block, new ids: no catalogue ids to keep. Linear ranges; enums append-only from the first release
  (the order below is free until then); no mode-dependent defaults (`harmonic` is ignored in Sustain
  mode, `tone` in Feedback mode, `distance` and `drift` outside the Natural types; none has a default
  that depends on another).
- **`rise`** keeps gfx-swell's meaning (time from the start of growth to full level); its range is this
  block's own. `thresh`, `source` and `release` keep gfx-swell's ids and meanings.
- **`carry`**, not `legato`: gfx-swell's `legato` is a 0 – 100 % sensitivity; one id cannot be a float
  there and a choice here. **`mute_sens`**, not `mute`: gfx-tuner's `mute` is a switch that silences the
  output.
- **`bloom`** is a level *relative to the note's body*, so a soft note blooms softly. (gfx-shimmer has a
  `bloom` in %, its diffusion: ids are per block, and only the contract's shared table promises one
  meaning; noted for golem-rack.) `duck` likewise: here the dry gives way to the bloom.
- `color` is relative (50 % = the model), `tone` bipolar around 0: the contract's two forms, as in
  gfx-phaser and gfx-drive.
- `feed` is the footswitch gesture in Switch mode (as Freeze's `freeze`): golem's footswitch drives it,
  never the bypass, so the bloom can ring out. In Auto, the footswitch is the `bypass` parameter, and
  `trails` lets the bloom ring out too. Its name is "Feed": "Feedback" is the word on the pedal.

### 3.2 Front panel

| id | Name | Type | Range | Default | EXP | Role |
|---|---|---|---|---|---|---|
| `harmonic` | Harmonic | e | Sub · Fundamental · Octave · Octave + Fifth · Two Octaves · Two Octaves + Third · Natural Low · Natural High · Climb | Octave | | Which mode feeds back (§2.4) |
| `delay` | Onset Delay | f | 0 – 3,000 ms | 300 ms | | Time the note is held before the feedback starts to grow (chords: at least until they are named, §2.2) |
| `rise` | Rise | f | 50 – 6,000 ms | 1,200 ms | ✓ | From the start of growth to full level: the virtual amp's gain (short = loud amp, close) |
| `bloom` | Bloom | f | −24 – +6 dB | −6 dB | ✓ | Where the feedback (and each held partial in Sustain) settles, relative to the note's body level |
| `dry` | Dry | f | −60 – 0 dB | 0 dB | ✓ | Dry level; −60 = off (only the feedback: bowed / EBow swells) |
| `mode` | Mode | e | Feedback · Sustain · Both | Feedback | | §2.5 |

### 3.3 Expert

| id | Name | Type | Range | Default | EXP | Role |
|---|---|---|---|---|---|---|
| `trigger` | Trigger | e | Auto · Switch | Auto | | Auto: every held note blooms. Switch: only while `feed` is on |
| `feed` | Feed | b | off / on | off | | The footswitch gesture in Switch mode |
| `distance` | Distance | f | 0.2 – 6 m | 1.5 m | ✓ | Natural types: the virtual path to the amp; which harmonic sings. On EXP: walk to the amp |
| `drift` | Drift | f | 0 – 100 % | 0 % | | Natural types: slow seeded wander of the distance during long holds |
| `chord` | Chord | e | Lowest · Highest · Strongest · All | Strongest | | Which note of a chord feeds back; All = each note (≤ 4) on its own |
| `carry` | Legato Carry | b | off / on | on | | A hammer-on / slide keeps the feedback and moves it to the new note; off: it restarts |
| `release` | Release | f | 20 – 2,000 ms | 150 ms | | Fade when the note ends (mute, touch, new pick, `feed` off). Pitch held, no chirp |
| `mute_sens` | Mute Sensitivity | f | 0 – 100 % | 50 % | | How small a sudden drop ends the note (30 → 10 dB in 60 ms) |
| `max_time` | Max Hold | f | 1 – 60 s | 15 s | | Longest bloom or hold, from the start of Rise; 60 = no limit |
| `duck` | Duck | f | 0 – 100 % | 0 % | ✓ | The dry (and the held partials in Both) gives way as the feedback blooms: the note turns into its harmonic |
| `color` | Color | f | 0 – 100 % | 50 % | | Harmonics of the feedback tone; 50 % = the model's pickup voicing, 0 = pure sine |
| `tone` | Tone | f | −100 – +100 % | 0 % | | Sustain: tilt of the held partials, ±6 dB per octave of partial number; 0 = the note's own body spectrum, − = mellower (a magnetic sustainer's sound) |
| `lowest` | Lowest Note | f | 30 – 150 Hz | 70 Hz | | Detector floor: 70 Hz guitar, 35 Hz bass. Higher = faster tracking |
| `thresh` | Noise Floor | f | −96 – −30 dB | −66 dB | | Nothing below it starts a note; a partial 6 dB below it is no longer followed (its pitch is held) |
| `source` | Detector Source | e | Main · Key | Main | | Key = the auxiliary input (the clean DI when placed after the amp). Key unconnected → Main |
| `bypass` | Bypass | b | off / on | off | | `make_bypass` (§Trails of the contract) |
| `trails` | Trails | b | off / on | on | | On: bypass releases the bloom over `release` |

Read-only meters: `state` (0 Idle · 1 Wait · 2 Rise · 3 Hold · 4 Release, of the loudest voice), `note`
(tracked f0, Hz; 0 = none), `harm` (the sounding mode: 0.5, 1 … 6; 1 in Sustain), `lock` (0 = the
string is no longer followed, the pitch is held; 1 = followed).

## 4. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Octave squeal on a held lead note (before a high-gain amp) | Octave, `delay` 400 ms, `rise` 1,000 ms, `bloom` 0 dB | Phase-locked partial grows out of the note; follows the bend |
| Singing fundamental, endless solo note | Fundamental, `delay` 200 ms, `rise` 800 ms, `bloom` −3 dB | Seamless bloom (g = a − γ d): sounds like the note just never dies |
| Scream at the end of a riff, on the switch | `trigger` Switch, Octave + Fifth, `delay` 0, `rise` 300 ms, `bloom` +3 dB | `feed` on a footswitch; release holds the pitch |
| Whistle over a major chord | Two Octaves + Third, `delay` 500 ms, `rise` 1,500 ms, `bloom` −6 dB | The FreqOut's 5TH: a major third, two octaves up (not over minor chords) |
| Real-amp feedback, unpredictable | Natural Low, `distance` 1.5 m, `drift` 30 %, `rise` 1,500 ms | Mode competition with hysteresis, harmonic jumps on bends |
| Walk to the amp | Natural High, EXP on `distance` 0.5 – 3 m | λ_k = 343 / (k f0): the harmonic changes every few tens of cm |
| Sustainer harmonic mode | Climb, `delay` 0, `rise` 600 ms, `duck` 60 % | Fundamental, then the note rises to the octave |
| Bowed / EBow lines | Fundamental, `dry` −60 dB, `delay` 0, `rise` 400 ms, `release` 300 ms, `carry` on | Swell from nothing; legato carries the bow across notes |
| Infinite note, clean or ambient | `mode` Sustain, `bloom` −6 dB, `release` 800 ms, `max_time` 60, then reverb | Partial bank: held without pumping or hiss, each partial on its own frequency |
| Chord drone | `mode` Sustain, `chord` All, `bloom` −9 dB, `tone` −30 %, `max_time` 60 | One partial bank per note |
| Sustainer "Mix" | `mode` Both, Octave, `bloom` −6 dB, `duck` 40 % | Note held, its octave blooming on it |
| Bass: held root under a chorus | `mode` Sustain, `lowest` 35 Hz, `bloom` −6 dB | Detector to 35 Hz (55 ms response) |
| Sub swell | Sub, `dry` −12 dB, `rise` 1,500 ms | Octave-down partial locked at half the reference's rate |
| Low-volume / in-ear feedback | Natural Low, `rise` 1,200 ms, before the amp sim | Feedback without volume, the FreqOut's market |

**Preset ideas** (`presets/gfx-sustainer.json`, no artist or trademark names): `octave-squeal`,
`fundamental-sing`, `twelfth-scream`, `two-octave-whistle`, `major-third-whistle`, `amp-close`, `amp-far`, `walk-to-amp`,
`string-climb`, `bowed-lead`, `infinite-note`, `chord-drone`, `sustainer-mix`, `bass-hold`, `sub-swell`,
`switch-scream`, `quiet-room-feedback`.

## 5. Bench suite (on top of gfx-bench's common checks)

Synthetic notes are Karplus-Strong strings with known f0, inharmonicity B and T60, made in the test. DIs:
clean.wav, lead-clean.wav (single notes, bends), crunch.wav; plus reference recordings of real amp
feedback by Raph (same guitar, three distances) for listening.

| Check | Pass |
|---|---|
| Latency | 0 (common check, not skipped) |
| **Tuning** | E2 … E6, every fixed `harmonic`: generated partial within ±2 ¢ of the string's partial k (mean over 200 ms); with B = 3·10⁻⁴, locked to the real partial (≤ 9 ¢ from k · f0), not to k · f0 |
| **Seamless start** | Partial k of the output has no step > 0.5 dB in the 20 ms after growth starts; no other partial changes by > 0.5 dB |
| **Seamless handover** | Decaying string (B 0 – 3·10⁻⁴, T60 3 – 12 s), from the start of growth until the string is 40 dB under the generator: partial k of the output within ±0.5 dB of its commanded level a_k (E4), ±1 dB (E2); no beat |
| Rise | Winner at A − 1 dB at `delay` + `rise` ± 5 %, `rise` 50 ms and 6 s |
| **Bends** | 200 ¢ bend in 150 ms and back; vibrato ±30 ¢ and ±50 ¢ at 6 Hz, while the string is followed (`lock` = 1): generated partial within 5 ¢ RMS of the string's (5 ms mean), and the output partial's ripple ≤ 0.5 dB at E4 (k ≤ 4), ≤ 1.5 dB at E2 (k ≤ 4), reported for k = 6 – 8 |
| **Lock hold** | Hiss at −70 and −60 dBFS RMS, a partial decaying through the floor: no cycle slip while it is above `thresh` − 6 dB; below, pitch held at the 150 ms mean ±2 ¢, no glide, `lock` = 0. A vibrato stopped mid-cycle by the decay holds its centre ±10 ¢ |
| No chirp | Release: instantaneous frequency of the generated tone constant ±1 ¢ until −60 dB |
| **Natural** | From equal seeds, winner = argmax G_k for 20 notes × 5 distances. Hysteresis: a holder keeps the voice until a challenger's G exceeds its own by 1.2 ± 0.05. Jump: 0.5 s ± 20 % at twice the gain, total level dip ≤ 6 dB. A `distance` sweep 0.5 → 3 m on E4 gives ≥ 3 distinct harmonics; each change a cross-fade (no click), deterministic (strict null). Natural Low never sounds k ≥ 5. A ±30 ¢ vibrato alone never changes the winner |
| **Chords** | Synthetic E2-B2-E3, A2-E3-A3-C#4, open G, Em: `chord` Lowest / Highest / Strongest name the right note in ≥ 95 % of 200 renders (random voicings, ±10 ¢ strings) within 0.4 s; a power chord never feeds back at the sub-octave, **with `delay` 0 as well** (Wait is extended, the bloom starts ≤ 400 ms after the strum); a single note with `delay` 0 is not held back; `chord` All: a partial shared by two notes is supplied once (its level within 1 dB of the target) |
| **Gating** | Hand mute (−40 dB in 40 ms): Release within 20 ms. Natural decay (T60 3 – 12 s): no Release. Mute of a string 8 dB above `thresh`: Release (second rule). Touch transient after the string fell below `thresh`: Release. No touch at all: Release at `max_time`. Re-pick: Restart. Hammer-on (pitch jump, no pick): Carry within 30 ms, no gap, no phase jump |
| **Sustain** | Note with T60 3 s, `max_time` 60: each partial k ≤ K within ±1 dB of A_k for 30 s; AM sidebands of the held note ≤ −60 dBc once the string is gone (no pumping); hiss at −60 dBFS on the input: output noise = the dry's, to 0.1 dB, at any time |
| **Sustain, inharmonic** | E2 with B = 10⁻⁴ and 3·10⁻⁴: during the handover no partial k ≤ K ripples by more than 1.5 dB; each held partial within ±2 ¢ of the string's own partial, not of k · f0 |
| Bank stability | 60 s hold, 30 Hz – 1.5 kHz: level drift ≤ 0.1 dB, pitch drift 0, no denormals |
| No clicks | Every parameter step (gfx-bench) and `harmonic` / `mode` changes while sounding: within the common click bound |
| Silence | Burst then silence (an abrupt end = a mute): silent after `release` (common check, default bound) |
| Noise immunity | Pink noise, hum at −50 dBFS: no bloom (unvoiced) |
| CPU | ≤ 3 % of a core at 48 kHz, `chord` All; ≤ 1.5 % one voice (Feedback ≤ 1 %) |

**gfx-bench options**: defaults. The common checks see: a 1 kHz sine from reset blooms an octave at
2 kHz (deterministic, so the null passes); the burst's abrupt end is a mute, so the silence check passes
with the default floor. `param_steps` skips `feed`, `trigger` and `carry` (gestures and rules, not levels). Mono 1 / 1, not
stereo by nature.

**Listening**: on clean.wav and lead-clean.wav, into a high-gain NAM amp, against Raph's real-feedback
recordings, blind and level-matched: does it sound like the amp, or like a synth?

## Visual

- **Category**: `pitch` (**confirmed at the review**). The pedal hears the pitch and adds a harmonic of
  it, and its selector speaks in intervals; gfx-octaver and gfx-ringmod's Track carrier are its
  relatives. `dynamics` would put it among the compressors and `drive` would give it the thrash finish.
- **Hue**: `METAL_HUE.pitch` `#c4148c`, **no crate hue** (gfx-freqshift and gfx-formant precedent: the
  motif sets it apart). **Finish**: anodized (the family's). **Approved.** The magenta pedals it can sit
  beside are told apart by their motifs: gfx-pitch stairs, gfx-octaver diamonds, gfx-freqshift barber
  (two columns), gfx-formant (two columns), here a horizontal string. gfx-formant and gfx-correct live
  on voice lanes, this pedal on guitar lanes; gfx-correct has the dark plum, gfx-ringmod a brass crate
  hue. A crate hue here would only spend one of the few gaps left.
- **Motif `modes`** (new case in `motifParts`, drawn with `line()` so both finishes work): *a string's
  standing waves, the note blooming into its harmonic*.
  - Left of the cartouche (x from 6 to cx − cw / 2 − 6): the **fundamental**: one loop between two nodes,
    drawn as three nested lenses `M xa cy Q xm cy−2a xb cy Q xm cy+2a xa cy` with a = (.35, .65, 1) · .55 ch,
    opacity .35 / .65 / 1: the bloom.
  - Right of the cartouche (x from cx + cw / 2 + 6 to W − 6): the **octave**: two loops (the second mode),
    each half drawn as one lens with a = .55 ch, opacity 1, and a faint one-loop lens behind it (opacity .3):
    the harmonic taking over.
  - A string line through both regions at y = cy (opacity .4). It stays inside the rune's band, clear of
    the knobs.
- **Face** (`visuals/data/faces.json`):
  `"gfx-sustainer": {"name": "Feedback", "blocks": ["sustainer"], "knobs": ["delay", "rise", "bloom", "dry"], "selectors": ["harmonic", "mode"], "category": "pitch"}`.
  No toggles (`feed` belongs to golem's footswitch). "Feedback" is a plain word, kept: it names what
  the default mode does and what players will look for; no parameter carries that name any more
  (`feed` is "Feed").
- **Glyph** `sustainer` (24 × 24, `fill="none" stroke="currentColor" stroke-width="1.8"`, round caps and
  joins, title "Sustainer / feedback (gfx)"): a string, its fundamental faint, its octave mode strong.
  ```
  <path d='M2 12h20' opacity='.35'/>
  <path d='M2 12Q12 2 22 12Q12 22 2 12' opacity='.4'/>
  <path d='M2 12Q7 4.5 12 12T22 12'/>
  ```
  The fundamental's lens peaks at y 7 / 17, the octave's S at 8.25 / 15.75 [A]. Distinct from `vibrato`
  (many cycles), `chorus` (two waves), `octaver` (arrow), `freeze` (snowflake).

## Questions settled at review

1. **Feed-forward vs a literal loop: feed-forward, for every type.** A loop built around a band-pass
   "string" is a second resonator: its frequency is pulled off the real string's by the loop phase
   (Δf ≈ −Φ · f / 2Q), so it beats against the dry, which is the DF-2's fault by another road; and its
   detector hears its own output. What the loop gives for free is kept in the control model: seeds from
   the string's partials, hysteresis, jumps that dip and cross-fade, `drift`. The model was corrected
   (§2.4: c, floor, jump rate, closed-form β). No second engine; the reference recordings judge the
   Natural types by ear.
2. **FreqOut's 5TH is the 5th harmonic** (two octaves + a major third), and its 3RD is the twelfth [V,
   manual]. "Two Octaves + Third" (k = 5) is added; Natural Low stops at k = 4 as the FreqOut's does.
3. **Default `harmonic`: Octave.** It is the squeal players expect and it fits any chord. (The FreqOut's
   knob at noon is its 3RD, the twelfth.)
4. **Chords.** Three windows (85 / 171 / 341 ms), a reading accepted when two analyses agree; when the
   clarity is poor, Wait lasts until a reading is accepted (≤ 400 ms). Single notes never wait. Natural
   competing across the notes of a chord: v2.
5. **Touch detection: kept, not relied on.** A second mute rule covers the quiet string; `max_time`
   (default 15 s) is the exit a silent hand needs. The touch level stays [U] until the DIs.
6. **Category: `pitch`.** The pedal is defined by a tracked pitch and a harmonic selector; `dynamics`
   would file it with compressors, which is the one thing it is not, and `drive` would give it the
   thrash finish.
7. **Placement: before the amp; no saturation of its own.** After a high-gain amp a clean partial over
   a distorted dry is the wrong sound whatever its `color`; an amp model inside a feedback pedal is
   another pedal. golem inserts the block before the amp, and `source` Key covers the other case.
8. **Catalogue** (for golem-rack): block `sustainer`, category `pitch`, `exp` on `rise`, `bloom`, `dry`,
   `distance`, `duck`; golem-fx only. Proposed: the block's footswitch is the bypass with `trigger` Auto
   (the FreqOut latched); a block preset "momentary" pins `trigger` Switch and puts the footswitch on
   `feed`.

## Sources

- DigiTech FreqOut: [manual mirror](https://manualmachine.com/digitech/freqout/19065384-owners-manual),
  [Sound On Sound review](https://www.soundonsound.com/reviews/digitech-freqout),
  [Fractal forum](https://forum.fractalaudio.com/threads/digitech-freqout-pedal-is-legit.127354),
  [Thomann reviews](https://www.thomann.co.uk/digitech_freqout_reviews.htm?rating=4),
  [Line 6 forum](https://line6.com/support/topic/57083-helix-freqout)
- [EBow players' guide](https://ebow.com/players-guide); [Fernandes Sustainer](https://fernandesguitars.com/sustainers/);
  [Sustainiac Stealth Pro manual](https://manuals.online/sustainiac/industrial-equipment/stealth-pro/user-manual/t9rp)
- [Gamechanger PLUS support](https://gamechangeraudio.com/support/plus-pedal/)
- Boss: [FB-2 press release](https://www.roland.com/us/company/press_releases/2011/1409/),
  [DF-2 vs FB-2](https://rolandindonesia.com/i-heart-guitar-review-boss-df-2-vs-fb-2/),
  [MusicRadar FB-2](https://www.musicradar.com/reviews/guitars/boss-fb-2-feedbacker-booster-567385),
  [GT-6 manual](https://www.manualslib.com/manual/19662/Boss-Gt-6-Gt-6.html?page=33)
- Fractal: [Feedback Simulator thread](https://forum.fractalaudio.com/threads/feedback-simulator.160001/post-1916110),
  [Blocks Guide](https://www.fractalaudio.com/downloads/manuals/fas-guides/Fractal-Audio-Blocks-Guide.pdf)
- [Softube patent US20080091393 / US7572972](https://patents.google.com/patent/US20080091393)
- Gabrielli et al.: [AES 136, paper 9066](https://aes.org/publications/elibrary-page/?id=17213); ICASSP 2014,
  DOI 10.1109/ICASSP.2014.6855055
- [musicdsp.org, Guitar feedback](https://www.musicdsp.org/en/latest/Effects/47-guitar-feedback.html)

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good; Strong on the idea, at risk on the tracker until it is benched on DIs.** The thesis
is the right one and nothing on the market does it: the feedback is the string's own partial,
phase-locked, added only by what the string has lost, with the loop's behaviour modelled on the control
side. The survey holds (the FreqOut manual was read: types, Natural Low's limit, placement; the Softube
patent's structure; the λ_k and bend-phase figures recomputed). Three things did not hold as drafted,
each shown by a numpy prototype: the **tracker lagged a vibrato**, the **Sustain loop cannot be
"exactly what the note has lost"** on a real, inharmonic string, and the **mode competition** did not
behave as described. All three are corrected in the text; the first remains the risk. Scripts:
`proto-gfx-sustainer/` in the review scratchpad (`bloom.py`, `bloom2.py` the drafted tracker; `het.py`,
`het2.py`, `het3.py` the corrected one; `image.py`, `image2.py` the loop; `lv.py` the competition).

### Measured (48 kHz, synthetic strings: 10 partials, B 0 – 3·10⁻⁴, T60 1.5 – 8 s, hiss −70 dBFS)

| Question | Result |
|---|---|
| Does "add what the string has lost" bloom without a seam? | Yes when the frequency is locked: partial ripple ≤ 0.1 dB at E4, ≤ 0.5 dB at E2 on a decay. A static phase error is harmless (30°: +0.02 dB of ripple; 90°: +0.3 dB) |
| Drafted tracker (band-pass pair, then PLL) on a ±30 ¢, 6 Hz vibrato | Generator against string 0.9 – 1.8 rad peak-to-peak at k = 1 – 2, output ripple 3 – 5 dB at k = 2; at ±50 ¢ on k = 3 the lock is lost. Compensating the filters' phase at the NCO's frequency halves the loop's damping (a vibrato on E2 then reads 57 ¢ RMS of error) |
| Corrected tracker (demodulator inside the loop, partials at k · ψ + trim) | §2.2's table: ≤ 0.4 dB at E4 up to ±50 ¢ and k = 4; ≤ 1.2 dB at E2 for k ≤ 4; 2.7 – 4 dB at E2 for k = 6 – 8 |
| Lock on a dying partial | Held to about −73 dBFS under −70 and −60 dBFS of hiss; lost by −77 dBFS. Hence the hold level `thresh` − 6 dB |
| Sustain loop, E2, B = 10⁻⁴ | Partials 8 and 10 held 12 and 18 dB low; after the freeze, 4 – 10 dB of beating from partial 6 up (15 – 37 dB at B = 3·10⁻⁴) |
| Partial bank, same strings | ≤ 1.5 dB on every partial (leakage of the measurement included) |
| Competition as drafted (c = 1.5, floor −90 dB) | A challenger under 1.5 × the holder's gain never wins; at 2 × it takes 9 s (default `rise`) |
| Corrected (c = 1.2, floor −30 dB, β_j ≥ 17 s⁻¹) | 0.5 s at 2 ×, 0.3 s at 4 ×, 1.6 dB of hysteresis; closed-form β gives `rise` within 1 ms |

### Changes made, and why

| # | Change | Why |
|---|---|---|
| 1 | **`track`'s phase detector: complex demodulation at the NCO's phase, filter inside the loop**; loop bandwidth f0 / 5 (14 – 50 Hz); partials at k · ψ + a ratio trim with their own 4 Hz loop; engage Snap / Keep; hold rules; weak-fundamental rule | Rows 2 – 4 above. The band-pass pair delayed the string's modulation by 2.55 periods ahead of the loop; "engage at zero error" (right for gfx-ringmod) would have left the generator at a random phase against the string; a first-order 3 Hz phase trim cannot follow a 3 Hz frequency offset; "frequency held" had to say which frequency (the 150 ms mean) |
| 2 | **Sustain is a partial bank, not a period-tuned loop** (§2.5); Thiran read, loop limiter, excitation gate and "Image stability" check removed | Rows 5 – 6. A loop is periodic, a string is not; the bank is the design's own thesis applied to every partial, has no hiss by construction, and deletes the one place where gfx-dsp's own rule (no all-pass on a moving read) was broken |
| 3 | **`tone` redefined**: a tilt of the held partials | "Per pass" in a loop that turns 82 – 1,300 times a second has no pitch-independent meaning |
| 4 | **Competition law**: c 1.5 → 1.2, hysteresis stated, loser floor −90 → −30 dB, jump rate β_j, closed-form β; bench row "winner = argmax G" restated | Rows 7 – 8. Hysteresis is real feedback's behaviour and stops a vibrato from flipping the winner, but it must be said and bounded |
| 5 | **g_k = max(0, a_k − γ · d_k)** with the applied dry gain | With `dry` or `duck` under unity the string's share of the output is smaller: the old law left a hole |
| 6 | **`harmonic`**: "Two Octaves + Third" added, Natural Low limited to k ≤ 4, options in harmonic order, Sub on its own half-rate accumulator | The FreqOut manual [V]; the 5th harmonic is a major third (wrong over minor chords); ψ / 2 of a wrapped phase jumps by π |
| 7 | **Chords**: three windows (85 / 171 / 341 ms), acceptance rule, **Wait extended until the chord is named (≤ 400 ms)**; shared partials owned by the lower note | "Costs no latency" was true of the audio but not of a short `delay`: the bloom would start on YIN's sub-octave and jump 0.3 s later, the very error the bench forbids |
| 8 | **End of note**: second mute rule for a quiet string; `max_time` default 60 s (no limit) → 15 s, counted from Rise | A string muted near the floor has less than D dB left to fall; a hand laid silently on a dead string left an endless tone with no exit but the bypass |
| 9 | **Ids**: `legato` (enum) → `carry` (bool); `mute` → `mute_sens`; `feed` named "Feed"; meter `lock` added | gfx-swell's `legato` is a float, gfx-tuner's `mute` a switch that silences; the pedal's face already says "Feedback" |
| 10 | **Detection facts**: YIN's first reading = window + a period + a hop; long detector under 60 Hz; `onset` is gfx-swell's module (pick A, jump C at 70 ¢) | The suite's own reviewed rules; a 30 Hz YIN window at 12 kHz is the CPU spike gfx-tuner removed |
| 11 | **`track` and `multipitch` APIs written** (§2.7), with the tracker family's division of work | One module for gfx-ringmod and this pedal needs one interface before either is coded |
| 12 | Bench: rows for vibrato ripple, lock hold, hysteresis and jump time, chord wait, partial-bank hold, inharmonic hold, shared partials, silent-hand exit | Each corrected claim has its check |
| 13 | CPU restated per engine; level bound stated | The bank costs more than the loop did (≈ 1.5 % against ≈ 1 %) |

Checked and left as written: the survey of §1.1 – 1.2 (Softube patent number, dates and structure;
Gabrielli et al.; Costello); §1.3's physics and its two figures (52 cm, 2.7 rad); the inharmonicity
figures; the Lagrange loss figure; zero latency; `rise` (gfx-swell's meaning), `thresh`, `source`,
`release`; `color` at 50 % = the model; `feed` against `bypass`; linear ranges; no mode-dependent
default; mono first; the glyph's geometry (7 / 17 and 8.25 / 15.75).

Coverage against the references: FreqOut (Gain = `bloom`, Onset = `delay` + `rise`, seven types, Dry,
Momentary) is covered and passed on four points: bends followed in phase, a pitch-held ending, chords
named before the bloom, a distance the foot can move. EBow and the magnetic sustainers: Fundamental,
Climb, Sustain, Both. Left out on purpose: the PLUS pedal's sampling (gfx-grains / gfx-glitch), an
automatic vibrato (the DF-2's fault), an amp saturation inside the pedal (question 7).

### Still open

| Point | Owner |
|---|---|
| **Tracker prototype in gfx-bench on the DIs** (lead-clean.wav bends and vibrato, a bass DI), before `lock`, `harm` and gfx-ringmod's `phase` are frozen: every figure of §2.2 is from synthetic strings with an emulated YIN | golem-fx, first task |
| gfx-ringmod's README still specifies the band-pass pair; to be aligned on `track` when it is coded | golem-fx |
| Vibrato ripple on low strings' upper partials (2.7 – 4 dB at E2, k ≥ 6): acceptable, or should Sustain's K fall to 6 below 110 Hz? | Raph's ear |
| Sustain's held spectrum (the body's, 80 ms after the pick, may be bright): `tone` default 0 or negative | Raph's ear |
| `max_time` default (15 s); Natural weights w_k, c and β_j; `color`'s −30 dBc | Raph's ear, with the reference recordings |
| Touch-noise and mute-drop levels on quiet DIs; the weak-fundamental rule; CPU of 32 partials | Bench |
| Catalogue entry, footswitch rule (question 8), glyph and motif `modes`; `bloom` and `duck` also exist in gfx-shimmer / gfx-delay with other meanings | golem-rack |
| Natural competing across a chord's notes; an external (key) pitch reference from another lane | v2 |

**Note (2026-11-14, after the gfx-polysynth review):** `multipitch` is shared with gfx-polysynth, whose
review fixed its API (gfx-polysynth/README.md §2.3): `prepare(sample_rate, f_min, max_notes)` — this pedal
passes `max_notes` 4 and its results are unchanged; `notes()` returns confirmed readings only; `early()` is
reserved for research and never starts a sound. `track` is unchanged.

## As coded (v0.1.0, 2026-11-15)

Files: `src/lib.rs` (the processor, `PARAMS`, the competition `compete()`, `natural_gains()`,
`chord_comb()`), `src/plugin.rs` (VST3 `GoleMFxSustain01`, "GoleM FX Sustainer", mono 1 / 1 first then
2 / 2, each with a one- or two-channel key bus, latency 0, read-only meters `state`, `note`, `harm`,
`lock`), `src/tests.rs` (23 behaviour tests on synthetic strings: inharmonic partials with known phases,
T60 shortening with k, pitch motion, white hiss; 2 ignored CPU reports). Shared: gfx-dsp `multipitch.rs`
(new), `track.rs` (opt-in additions), `onset.rs` (`PitchJump::feed`, additive).

### Shared code and the null proof

- **`multipitch`** as agreed: `prepare(sample_rate, f_min, max_notes)`, `reset`, `push`, `notes()`
  (confirmed readings only, sorted by pitch, ≤ `max_notes`), `age_ms()`, `early()` (empty unless
  `set_early(true)`; never used here). Added: `onset()` (the windows restart at a pick), `set_active()`
  (analyses only while YIN is unclear; the history always fills), `set_f_min()` (`lowest`, no
  allocation), `analyses()`. Decimation to ~6 kHz (4th-order Butterworth at 0.35 × the rate), Hann 512 /
  1,024 / 2,048 points, one analysis every 256 samples on the longest window after the onset;
  harmonic summation on 1/8-semitone cells (Klapuri's weights, m ≤ 10, bands widened upwards for
  B = 3·10⁻⁴), f0 refined from partials 1 – 3 (parabolic, log magnitude), cancellation by the geometric
  mean of the neighbours **in the original spectrum** (the fundamental whole). Measured (unit tests):
  single notes E2 – E5 confirmed by 128 – 171 ms at 44.1 / 48 / 96 kHz; E2-B2-E3, A2-E3-A3-C#4 (A3, an
  exact octave, is not named: its partials are A2's, owned by A2 anyway), a D triad, E2-B2-E3-G3; 200
  random voicings (E2 – A3 roots, power chords ± third, triads, ±10 ¢ strings): lowest 200 / 200,
  highest 183 / 200, none later than 0.4 s. A 4-note build's notes are always within a 6-note build's.
- **`track`**, every addition opt-in (gfx-ringmod's default path unchanged):
  - `set_partial_rate(n)`: the partials' loops every n ≤ 16 samples (fs / n ≥ 4 f0): mixer and the first
    two one-poles at fs, the last two, the level / error, the variance averages and the PI at the update.
    The mixer's phasor runs on a rotator re-anchored on φ at each update (`partial_cos(k)` gives it to the
    synth). This pedal uses 8 (16 broke the 2 ¢ tuning row).
  - Frequency-aided acquisition: before engaging, the error's rotation per update gives the partial's
    offset; the trim starts there, and the engage test also accepts a steady rotation (a 4 Hz loop alone
    never caught partial 5 of a wound E4, 6 Hz away).
  - The decimated Snap turns the demodulator's states with the phase jump. (The per-sample path's
    partial Snap clears them and reads as lost on the next sample: unused by gfx-ringmod, left as is.)
  - An **inharmonicity fit**: B from the locked partials' whole ratios (weights k² − 1), every partial's
    trim (±15 ¢) centred on √((1 + B k²) / (1 + B)). Partial 8 of a wound E2 is 16 ¢ sharp, partial 16
    64 ¢: beyond the specified ±15 ¢ trim alone.
  - `set_hold_margin(Some(dB))`: the 150 ms history and the trims learn only that far above the hold
    level. `set_narrow(n)` / `prepare_narrow(max_n, f_min)`: a comb of n periods, bandwidths ÷ n, variance
    windows × n (chord voices). Diagnostics: `partial_trim`, `stretch_b`.
- **`onset::PitchJump::feed(hz, clarity, elapsed, clarity_min)`**: the jump rules on the caller's YIN
  readings (this pedal's detector reaches `lowest`; the detector's own stops at 75 Hz, and costs 33 ns).
- **Null proof**: gfx-ringmod (Track + lock; Track + lock + snap + `pitch` 0.6) and gfx-swell (`legato`
  100 %; 40 % in blocks of 64) rendered on a synthetic E2 bend / A3 vibrato / E4 file with the original
  and the new gfx-dsp: byte-identical WAVs. gfx-ringmod 23 / 23 and gfx-swell 18 / 18 tests, both
  benches 28 / 28.

### Deviations from the spec

| Spec | Coded | Why (numbers) |
|---|---|---|
| Partials' loops at fs, "fs / 8 if the bench asks" | Decimated (8 samples), the mixer by rotator | CPU: 65 → ≈ 20 ns per partial (Sustain at E4 1,371 → 467 ns) |
| Trim ±15 ¢ (§2.2) | ±15 ¢ around the fitted stretch | B = 3·10⁻⁴: partial 8 of E2 held 16 ¢ off before (5 dB of beating in the handover), now 0.55 ¢; partial 16 1.64 ¢ |
| The voice follows the string down to `thresh` − 6 dB | The tracker locks down to `thresh` − 6 dB, but the **voice holds its pitch 12 dB above** (`thresh` + 6 dB), at the 150 ms mean learned there | The 44 Hz reference loop wanders ±0.3 Hz (±5 ¢) at 110 Hz once the SNR in its band falls to 20 – 30 dB (−60 dBFS white hiss); the plain mean then held a fading note 10 – 16 ¢ off. Now: the vibrato's centre ±2.6 ¢ (−70 dBFS hiss) and ±4.0 ¢ (−60 dBFS), no glide |
| g_k = max(0, a_k − γ d_k) | d_k faded out of the subtraction near the hold level h: d · clamp((d − h) / h, 0, 1), h gliding 30 ms | Under h the level is noise; the generated part's noise is now −51 dB re a −60 dBFS hiss (−19.5 dB with the plain law). Soft, so a `thresh` step makes no click (−45 dB HF burst with a hard gate) |
| Seed a_k(0) = max(d_k, A · 10^−2) | max(γ d_k, A · 10^−2), ≤ A | With `dry` −60 the bloom starts from what is heard: the bowed swell rises from silence |
| One-period comb for every voice | A note named by `multipitch` gets a comb of n ≤ 6 periods whose zeros fall on the other notes' first three partials (`chord_comb`) | A fifth's partials leak −13 dB, a major third's ≈ 0 dB through one period: chord voices never locked |
| A reading = two agreeing analyses | Same, and a note needs its own fundamental (an interior peak ≥ −20 dB re its strongest of partials 1 – 4); notes within 70 ¢ merge | The 85 ms window named a 92 Hz "missing fundamental" (C#4's 3rd and 6th) in an A chord |
| Wait extended until named | Same, and a waiting chord voice follows later readings (non-All: retargets; All: a note no longer named gives way) | The first reading may be revised. A2-E3-A3-C#4's top note blooms 520 ms after the strum with `delay` 0 (the power chord row, ≤ 400 ms, holds: 231 – 315 ms) |
| Past 400 ms, YIN's candidate | Only after a detected onset; a voice not locked within max(`delay`, 0.4 s) + 1 s gives up until the next onset | Pink noise kept a never-locking voice waiting (CPU); nothing bloomed either way |
| YIN feeds the tracker | Only clear readings, the input above `thresh`, within 45 ¢ of what the tracker follows | A fading string's readings walked the feed-forward away (each > 50 ¢ step re-armed the loop) |
| Legato: gfx-swell's `PitchJump` on its own YIN | `PitchJump::feed` on this pedal's YIN | 33 ns, and bass legato under 75 Hz |
| Mute (D in 60 ms) | Not in the first 80 ms after an onset | The pick's own decay |
| — (unspecified) | `duck`'s progress p = max u of the Feedback modes (0 in Sustain); body b at 80 ms after the onset or at lock, whichever later; bypass without trails: 10 ms fade, dry to unity in 10 ms; the colour on each partial's Feedback share (no switch when a mode leaves the set) | — |

### Bench rows (§5) as measured (synthetic strings, 48 kHz)

| Row | Result |
|---|---|
| Latency | 0, measured 0 at 44.1 – 192 kHz |
| Tuning | every fixed `harmonic` at E2 / E4 / E6, B 10⁻⁴ and 3·10⁻⁴: within 2 ¢ of the string's partial (6.5 ¢ from k f0 for k = 5, B = 3·10⁻⁴) |
| Seamless start | no step at the start of growth; the other partials do not move (≤ 0.5 dB) |
| Seamless handover | output partial vs a_k: E4 k = 1: 0.06 dB, k = 2: 0.13 – 0.21 dB; E2 k = 1: 0.07 – 0.14 dB, k = 2: 0.20 – 0.45 dB (bounds 0.5 / 1) |
| Rise | A − 1 dB at 49 ms (`rise` 50), 1,199 ms (1,200), 6,000 ms (6,000); Rise starts at `delay` |
| Bends | E4 k = 1 / 2 / 4: ±30 ¢ 2.0 – 2.2 ¢ RMS, ±50 ¢ 3.4 – 3.6 ¢, bend 1.0 – 1.1 ¢; ripple 0.39 – 0.63 dB, **bend at k = 1 0.73 dB** (bound 0.5; 0.30 – 0.49 for k = 2 – 4). E2: ±30 ¢ 3.5 – 5.2 ¢ RMS, **±50 ¢ 5.7 – 8.6 ¢** (bound 5: the tracker as coded, gfx-ringmod measured 7.8 ¢), ripple ≤ 0.73 dB (bound 1.5) |
| Lock hold | lost at 0.78 – 0.80 s on a fading E4 with vibrato; held at the centre +2.6 ¢ (−70 dBFS hiss), +4.0 ¢ (−60 dBFS), no glide (0.0000 ¢), `lock` 0 |
| No chirp | release frequency constant (< 1 ¢) |
| Natural | from equal seeds winner = argmax G (20 notes × 5 distances); hysteresis threshold 1.202; jump 0.494 s at 2× (dip 2.9 dB), 0.296 s at 4×, 2.07 s at 1.3×; a 0.5 → 3 m sweep on E4 sounds harmonics 1, 5, 2, 4, 3; deterministic; Natural Low never above k = 4; a ±30 ¢ vibrato never changes the winner |
| Chords | Lowest / Highest on E2-B2-E3 and A2-E3-A3-C#4, `delay` 0 and 300; never the sub-octave; a single E4 with `delay` 0 blooms before 150 ms; All: B2's partial 2 (= E2 × 3) held by E2 only |
| Gating | hand mute (−40 dB in 40 ms): Release 24 ms after it starts; natural decays (T60 3, 12 s): none; a string 8 dB above `thresh` muted: Release (second rule); touch after the floor: Release; `max_time` 2 s: Release at 2.00 s; re-pick: Release and a new Rise; hammer-on E4 → G4: carried in 19 ms, no Release, no gap |
| Sustain | T60 3 s, `max_time` 60: partials 1 – 8 within 0.7 dB of A_k for 30 s; AM ≤ 0.000 dB p-p; generated noise −51 dB re a −60 dBFS hiss (≪ 0.1 dB on the output) |
| Sustain, inharmonic | E2, B 10⁻⁴ / 3·10⁻⁴, partials 1 – 16: held within 1.64 ¢ of the string's own; handover ripple ≤ 0.98 dB |
| Bank stability | 60 s holds of E1 (`lowest` 35, −31.9 dBFS RMS at 60 s) and 1.2 kHz: level drift ≤ 0.1 dB, pitch drift 0, finite |
| No clicks | `harmonic` / `mode` changes while sounding: HF bursts ≤ −87 dB re peak; bench parameter steps ≤ −62 dB (bound −50) |
| Silence, noise | silent after a burst (−240 dBFS); pink noise and −50 dBFS hum never bloom |
| Common checks | 28 / 28 (44.1 – 192 kHz) |

**CPU** (48 kHz, mono, a sounding note, `cpu_report`; the bench's line, pink noise, defaults: 188 – 196 ns):

| Case | ns / sample | % of a core | Budget |
|---|---|---|---|
| No note (pink noise) | 194 | 0.93 | — |
| E4 / E2, Octave (Feedback) | 211 – 214 | 1.0 | ≤ 1 % |
| E2, Natural High | 289 | 1.4 | ≤ 1.5 % |
| E4 / E2, Sustain (K 15 / 16) | 467 – 512 | **2.2 – 2.5** | ≤ 1.5 % |
| E2, Both | 521 | **2.5** | ≤ 1.5 % |
| E2-B2-E3-G3, Sustain, `chord` All | 836 | **4.0** | ≤ 3 % |

What costs: detection ≈ 100 ns on a note (YIN 35, the pick detector 32, the side chain's decimators),
one reference loop per voice ≈ 85 ns (gfx-dsp `track`'s per-sample path, kept bit-exact for gfx-ringmod),
≈ 20 ns per partial. The ways down: an opt-in decimated reference loop in `track` (4 references in
`chord` All), Sustain's K to 8 under 110 Hz (an open question already).

Not run (need recordings or golem's rig): the DI rows (clean.wav, lead-clean.wav, crunch.wav), Raph's
real-feedback recordings, the open-chord renders (six strings: `notes()` keeps the four most salient), the
touch-noise level on quiet DIs, the listening A / B.

### Left for the owner

- By ear, against the reference recordings: Natural weights w_k, c and β_j; `color`'s −30 dBc; `tone`'s
  default; `max_time` 15 s.
- Whether the voice's follow margin (12 dB above `thresh` − 6 dB) should be a parameter or follow
  `thresh` differently on real DIs; the weak-fundamental rule and the mute / touch levels on the DIs.
- CPU: accept 2.5 % for Sustain / 4 % for All, or ask for the decimated reference loop.
- golem-rack: the catalogue entry (block `sustainer`, category `pitch`, `exp` on `rise`, `bloom`, `dry`,
  `distance`, `duck`), the footswitch rule (question 8), the new glyph and motif `modes`.
