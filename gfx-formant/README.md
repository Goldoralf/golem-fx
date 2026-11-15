# gfx-formant

> **Status: DESIGNED** (2026-11-14), **reviewed** (Fable 5.2, 2026-11-14: see the Review at the end). Not
> coded. Design by Claude Opus 5.5. It brings two new gfx-dsp modules: `specenv`, the spectral envelope
> and its filters, and `psola`, gfx-golem's epoch tracker moved into the library with a new fixed-delay
> reader. **A listening prototype of the Live engine gates the coding** (§5), as it did for gfx-golem.

golem's **formant shifter** moves a sound's spectral envelope (on a voice, the vocal tract) without
changing its pitch.
- **On a voice**: a bigger or smaller singer, a gender or age change, the chipmunk or the giant, with
  the same notes.
- **On guitar, bass, keys**: a tone effect. The "body" of the instrument gets smaller or bigger, and
  vowels can be put on a chord (talk-box without the tube).
- **Throat**: growl (period doubling), breath, and source tilt, on top of the shift.

It is a **new block** (`formant`). The catalogue has no formant block: `pitch-spectral` (Signalsmith
Stretch) only has a `formant_comp` bool, and `golem-double` has a `formant` knob on its doubles only. Its
ids follow the suite: `formant` and `register` as in gfx-golem, `shift` as in gfx-pitch, `mix`,
`output`, `bypass`, `state` as in the contract.

## 1. State of the art

Survey of 2026-11-14. **[V]** read in source code, a paper or a manual · **[M]** maker's page ·
**[F]** forum post, review or comparison site · **[A]** computed here · **[I]** inference, or not
re-checked at the source · **[U]** could not be verified.

### Products

| Reference | What it does best | Latency | What we take |
|---|---|---|---|
| Soundtoys Little AlterBoy [M, F] | Pitch and Formant knobs, independent or linked, a Robot mode, drive. The reference "voice changer" plug-in | Not published by the maker. gfx-pitch's README quotes ≈ 60 ms from a KVR thread [F]. Users complain of latency [F] | Formant as the main knob, pitch as an independent second control |
| Waves Vocal Bender [M, F] | Pitch and formant, sold as "zero latency" for live use, with LFO and sequencer modulators | A KVR user measured a 512-sample offset in a render [F, U] | A live voice changer must be usable in monitors: our Live engine never delays the audio |
| Roland VT-4 / Boss VT-1 voice transformers [M, F] | Hardware with a pitch slider and a formant slider, both "instant", praised for having no glitches when the sliders move | "Very low" (MusicTech review), no figure [F] | Sliders that can move during a note without clicks: EXP on `formant` |
| zplane TIMBRE [M, F] | A dedicated formant-shift plug-in | 23.2 ms at 44.1 kHz (1,024 samples), from a comparison site [F] | The latency a spectral formant shifter costs |
| Auburn Sounds Graillon 3 [M] | Live voice changer: pitch correction, three pitch engines, formant shifting with a scalable "formant contour" (0 – 100 %) | Not zero since Graillon 2 (changelog); engine not documented [M, U] | A formant amount rather than a fixed on/off |
| Antares THROAT Evo [M] | Physical-model vocal designer: "neutralises" the singer's tract, then imposes a new one (length, five widths), edits the glottal waveform, and adds breathiness from breath to whisper | — | The same split: estimate the envelope, divide it out, impose a new one. Our throat options are `growl`, `breath` and `tilt` |
| Melodyne, élastique, Rubber Band (GPL) [I] | Studio formant correction or preservation: a spectral (often cepstral) envelope is divided out, the remainder shifted, the envelope re-applied | Offline or tens of ms | Cepstral envelope: confirms the spectral route for quality |
| Signalsmith Stretch (MIT, the `pitch-spectral` block) [V] | Formant shift inside the STFT. The envelope is a peak-hold then min-hold smoothing of the bin energies, with its decay tied to a rough f0 (no cepstrum, no LPC). The warp is linear below a "tonality limit" and an offset above it. A per-bin gain E(warped f) / E(f) is applied before phase prediction. Its changelog says the correction "is not as sharp as monophonic algorithms (PSOLA)" | One STFT frame | Envelope smoothing tied to f0. The ratio E(f / α) / E(f) as a gain. It confirms PSOLA's edge on monophonic voice |
| Kilohearts Formant Filter, gfx-autowah Vowel [I, V] | Vowel filters: they **impose** formants (a → e → i → o → u) rather than move them. Zero latency, polyphonic | 0 | Vowel templates (`vowel`), applied in a way that works on chords |
| Eventide H910 / H949 harmonizers [I] | No formant control: their sound *is* the moved formants of a pitch shifter | — | Nothing. Listed because the brief asked |

### Methods

| Method | Latency | Polyphonic | Weak points | Verdict |
|---|---|---|---|---|
| **TD-PSOLA with grain resampling** (Moulines & Charpentier 1990; Valbret, Moulines & Tubach 1992 for voice conversion) [V, I]. gfx-golem already ships it | About one period of the lowest note (§2.3) | No | Needs epochs. Buzzy or hoarse at large shifts down. Unvoiced sounds need another path | **Kept: Grain engine.** It is the most natural on a solo voice, needs no envelope estimate, and its code exists |
| **LPC source-filter**: whiten with A(z), re-colour with a warped 1/A′(z) | 0 (causal analysis) | Yes | The all-pole fit follows the harmonics on high voices (biased formants, a metallic shift). Warping needs a re-fit or root moves | Rejected as the model, kept as the structure (whiten, then re-colour) |
| **Warped all-pass chain**: replace z⁻¹ in 1/A(z) by a first-order all-pass (Strube 1980; Härmä et al., JAES 2000) [I] | 0 | Yes | Still an LPC envelope. A delay-free loop must be solved. The warp is bilinear, but at 48 kHz it is almost uniform in the voice band: for +4 st (λ = 0.115) the ratio is 1.260 at 500 Hz, 1.253 at 2.5 kHz, 1.247 at 3.5 kHz, 1.20 at 8 kHz [A] | Rejected for its envelope model, not its warp. Vowels, clamps and tilt need a log-spectral domain |
| **STFT envelope warping** (true envelope: Röbel & Rodet, DAFx 2005) [V] | One frame plus a hop: 20 – 60 ms | Yes | Latency. Phasiness if the phases are re-synthesised | Its **estimator** is kept. Its STFT synthesis is not: the envelope ratio becomes a filter (§2.2) |
| **Vowel filter bank** | 0 | Yes | Imposes vowels and does not move the input's own formants | Kept as a feature (`vowel`), on the same filter |

**True envelope (TE)** [V, I]: the log-amplitude spectrum is smoothed by cepstral liftering, then the
smoothing input is replaced by max(original, current envelope), and this repeats. It converges to an
envelope that passes through the harmonic peaks. Its cepstral order is bounded by the harmonic sampling,
Pc ≤ fs / (2 · F0) (Röbel, Villavicencio & Rodet). The 2005 paper reports a few times LPC's cost for
clearly better control; a later real-time version is 2 – 9× faster. A Δ ≈ 2 dB stop criterion is
reported [I]. The iteration only ever **raises** the envelope, so it must start from the frame's own
spectrum (§2.2). **Pitch-adaptive windows**: WORLD's CheapTrick uses a **symmetric Hann** window of three
periods for its envelope [I]: the symmetry is what makes the estimate independent of where the glottal
pulses fall in the window.

### Forum feedback (what players hate) [F]

- **Metallic, phasey, "underwater" timbre** at large shifts: phase vocoders without phase coherence, or
  envelopes that follow the harmonics instead of the formants.
- **Latency** for live monitoring (Little AlterBoy, Graillon after v2, Vocal Bender's hidden 512 samples).
- **Sibilants and breath**: moved up they turn thin and harsh, moved down they lisp. Consonants
  "vanish" at large shifts.
- **Warble** from frame-to-frame jitter in the envelope estimate.
- **Gender changes sound wrong with formant alone**: a man's voice with female formants sounds like a
  small man. A real change needs pitch too.

### What we conclude

1. **Two engines, one setup choice.**
   - **Live** (default): zero audio latency, polyphonic. An envelope ratio filter: the input's
     envelope is estimated by TE from the past, and E(f / α) / E(f) is applied as a **minimum-phase FIR
     filter**. The source's fine structure (pitch, phases, noise) is never re-synthesised, so there is no
     phase-vocoder phasiness and no pitch error. Its price: the **filter** follows the voice about 12 –
     22 ms late (§2.2), so sustained notes are right and fast articulation is blurred. `lookahead` buys
     that back.
   - **Grain**: gfx-golem's PSOLA at a fixed reported latency of 8.6 – 21 ms (by `register`). It is the
     most natural sound for a solo voice, it follows articulation exactly (the grains carry their own
     formants: nothing is estimated), and it alone allows a **pitch** change (`shift`) for real gender
     or age changes.
2. **Fix the known faults.**
   - An envelope that cannot follow harmonics: TE with an order from f0.
   - Unvoiced sounds handled on their own (`unvoiced`).
   - A boost clamp and a noise-floor guard, so no hiss is lifted.
   - Level normalisation.
   - Steady envelope updates (a window of whole periods, averaged over the pitch phase; filters
     crossfaded over each hop).
3. **One log-spectral domain for everything the envelope can do**: shift, vowel templates, tilt, clamps.
   Every option costs one addition in the log domain, not a new filter.

## 2. Design

### 2.1 Signal path

```
                 ┌──────────── analysis (every hop, scheduled on sample counts) ─────────────┐
in ─┬─▶ YIN ─────┤ Hann window (3 periods × 3 / fixed) ─▶ FFT ─▶ log|X| ─▶ TE ─▶ log E(f)     │
    │            │ L(f) = [log E(f/α) or vowel] − log E(f) + tilt, clamped ─▶ min-phase h[n]  │
    │            └────────────────────────────────────────────────────────────────────────────┘
    │
    │  Live:   x ─▶ (lookahead) ─▶ FIR h (crossfaded per hop) ─▶ × growl AM ─▶ + breath ─▶ mix ─▶ output
    │
    │  Grain:  x ─▶ epochs ─▶ fixed-delay PSOLA reader (α, r) ─┐
    │          x ─▶ delay D ─▶ FIR h (unvoiced amount) ────────┴─ voicing crossfade ─▶ growl, breath ─▶ mix ─▶ output
    └─ dry (delayed by the reported latency) ──────────────────────────────────────────────────────────▶ mix
```

α = 2^(`formant` / 12) is the envelope ratio (α > 1: formants up). r = 2^(`shift` / 12) is the pitch ratio
(Grain only).

### 2.2 Live engine: the envelope ratio filter (new gfx-dsp module `specenv`)

Sizes at 44.1 / 48 kHz. They double at 88.2 / 96 kHz and double again at 176.4 / 192 kHz, so every time
constant stays the same.

**Analysis** (every hop of 256 samples, 5.3 ms):

- **Window**, zero-padded into a 2,048-point FFT. Three frame classes:
  - **Voiced** (`source` Voice, pitch tracked): a **symmetric Hann of exactly k periods** of the detected
    f0, k = max(3, ⌈8 ms / T⌉), at most 2,048 samples (CheapTrick's rule). Its length is fractional: the
    window is sampled on its own continuous length, never rounded to whole samples. **Three power
    spectra are averaged**, from windows ending now, T / 3 and 2T / 3 earlier, before one TE.
    - Why not the asymmetric window of gfx-dsp `vocoder` (a short falling edge): the estimate then
      depends on where the glottal pulses fall under that edge. Prototype, steady /a/, `formant` ±4 [A]:

      | Window | Harmonic levels, hop to hop | Strongest non-harmonic component |
      |---|---|---|
      | Asymmetric, 3 periods, fall ≤ L / 4 (first design) | 2.3 – 7 dB | **−20 dBc** (98 – 400 Hz) |
      | Hann, 3 periods | 0.4 – 0.9 dB | −51 … −30 dBc |
      | Hann, 3 periods, 3 spectra T / 3 apart | — | **−71 … −52 dBc** up to 220 Hz; −42 … −33 dBc at 311 – 397 Hz |

      What is left is a ripple at three times the period's rate: the T / 3 spacing cancels it. Above
      ≈ 250 Hz a period is under 200 samples and the cancellation is poorer: open (§ Review).
  - **Tonal, not tracked** (`source` Instrument, or Voice with no pitch and a peaky spectrum: a chord):
    L = 2,048 (42.7 ms), Hann; the envelope is then **smoothed over time** (one pole, 150 ms, in dB).
    An instrument's body does not move, and without this the partials that beat inside one window
    modulate the filter: E2 B2 E3 G♯3 at +4 st gave new partials at −25 dBc; with O = 60 and 150 ms,
    −47 dBc (a fifth: −52 dBc) [A].
  - **Noise-like** (Voice, no pitch, spectral flatness over 300 Hz – 8 kHz above ≈ 0.3, with
    hysteresis [to tune]): L = 512 (10.7 ms), asymmetric (rise 384, fall 128): consonants caught
    quickly, and noise has no pitch phase to disturb.
- **Envelope lag.** Audio is never delayed, the **filter** is. The Hann window's centre is 1.5 T old,
  the three spectra add T / 3, the frame's work 1.3 ms, and the crossfade completes 5.3 ms later
  (2.7 ms on average): about **1.8 T + 4 ms**, i.e. 22 ms at 100 Hz, 16 ms at 150 Hz, 12 ms at 220 Hz.
  - What it costs, measured on /a/ → /i/ glides at 150 Hz (F2 moves an octave) [A]: while the vowel
    moves, the output's harmonics miss their target by ≈ 10 dB rms (peaks of 25 – 35 dB) at `formant`
    −4 and +7, against 1.4 dB (−4) and 2.9 dB (+7) on the steady vowel. The stale ratio removes the old
    vowel's formants from a sound that no longer has them. No click, no pitch error: a blurred vowel
    change, for the length of the lag.
  - `lookahead` (setup only) delays the audio instead, to centre the analysis: on the same glides,
    10 ms brings the error to 5 – 7 dB rms and **20 ms to ≈ 2 dB**, the steady figure. 20 ms covers
    voices above ≈ 115 Hz.
  - So: Live at 0 ms for sustained singing, pads and instruments; `lookahead` or Grain for speech and
    fast lines.
- **Pitch**: gfx-dsp `detect` (YIN) with gfx-golem's ranged constructor and `register` ranges, and
  gfx-golem's voicing gate (clarity hysteresis at gfx-golem's `sens` 50 % values, 0.8 / 0.65; `thresh`;
  30 ms hold).
- **TE** on the 1,025-bin log spectrum, max-pooled to 513 bins: bin j takes the maximum of bins 2j − 1,
  2j and 2j + 1 (centred, so no half-bin bias). Max-pooling keeps the harmonic peaks that TE must pass
  through, even at 60 Hz (2.6 bins apart before pooling).
  - Cepstral order O = ⌊0.5 · fs / f0⌋, clamped 24 – 400 (voiced). O = 60 for tonal untracked frames
    (≈ 400 Hz detail: a body, not partials) and for noise-like frames (two iterations).
  - **Cold start**: every frame starts from its own spectrum. At most 8 iterations (16 for tonal
    untracked frames); stop when the spectrum exceeds the envelope by less than 2 dB everywhere.
    Measured on the k-period windows: 2 – 3 iterations at 100 Hz, 4 – 6 at 220 Hz, 6 – 7 at 400 Hz; a
    1 dB criterion and 16 iterations change nothing audible (same errors, same ripple) [A].
    - **No warm start.** The iteration replaces the spectrum by max(spectrum, envelope): started from
      the previous envelope it can only rise, and it becomes a peak-hold. Prototype: 400 ms after an
      /a/ → /i/ change the warm-started envelope still stood 36 dB too high at 1 kHz (14 dB rms over
      200 Hz – 5 kHz); the cold start was within 1 dB of the reference [A].
  - **Hold**: below `thresh` (RMS over the window), the envelope and the filter are held. A note starting
    from silence is filtered with the last real envelope, never with one estimated on the noise floor.

**The log ratio** L(f) in dB, on the 1,025-bin grid:

- **Shift**: L₀(f) = log E(f / α) − log E(f), with log E read with linear interpolation in bins.
  - Above Nyquist (α < 1 reads past fs / 2), E continues with its last octave's mean slope, limited to
    0 dB / oct or less.
- **Vowel** (`vowel_amt` v): the target becomes (1 − v) · log E(f / α) + v · (log V(f / α) + c).
  - V is a vowel template: the five-formant cascade of the table below.
  - c matches V's energy to E's, weighted by the input spectrum, so the level holds.
  - At v = 100 % the input's own envelope is replaced: a talk-box on any instrument.
- **Tilt**: + `tilt` · log₂(f / 1 kHz), clamped to ±12 dB.
- **Clamps**:
  - L ≤ **+24 dB** anywhere. Moving a formant up means lifting the valley it lands in to a peak. On the
    bench vowels (80 – 120 Hz bandwidths) a +18 dB clamp caught 14 – 19 % of the bins under 5 kHz at
    +7 st and left the new peaks up to 10 dB short (2.2 – 2.8 dB rms); +24 dB: 5 %, 5 – 6 dB short,
    1.2 – 1.7 dB rms [A, f0 100 Hz, /a/ and /i/]. To voice between 18 and 24 on the sung phrase (breath
    between the harmonics rises by as much).
  - Where the smoothed input spectrum lies more than 60 dB under its peak, L ≤ 0 dB (noise-floor
    guard: nothing lifts hiss).
  - L is faded to 0 dB under 40 Hz and over 0.45 · fs.
- **Unvoiced amount**: on **noise-like** frames only, α is replaced by α^(`unvoiced` / 100), crossfaded
  with the voicing gate (10 ms open, 30 ms close). A chord is not "unvoiced": tonal untracked frames keep
  the full α.
- **Level**: g = √(Σ P(f)) / √(Σ P(f) · 10^(L / 10)) over the analysis spectrum P, smoothed (30 ms) and
  clamped to ±12 dB. It is folded into L as a constant, so moving formants does not change the loudness.
- **Identity**: with α = 1, v = 0 and `tilt` 0, L ≡ 0 and the FIR is bypassed. The output is then the
  input, bit for bit.

**Vowel templates** (Peterson & Barney 1952, men's averages [I]). Order a e i o u, as gfx-autowah, and
interpolated in log frequency along `vowel`:

| Vowel | F1 | F2 | F3 | F4, F5 (fixed) |
|---|---|---|---|---|
| a (ɑ) | 730 | 1,090 | 2,440 | 3,300 · 4,500 |
| e (ɛ) | 530 | 1,840 | 2,480 | |
| i | 270 | 2,290 | 3,010 | |
| o (ɔ) | 570 | 840 | 2,410 | |
| u | 300 | 870 | 2,240 | |

Bandwidths are 80, 90, 120, 150, 200 Hz [I]. They are templates for an adult male tract. `formant`
moves them like any envelope. gfx-autowah keeps its own two-formant table: no change there.

**Filter design** (per hop):

1. Real cepstrum of L (2,048-point IFFT).
2. Fold to minimum phase: c₀, 2cₙ for 1 ≤ n < 1,024, c₁₀₂₄.
3. FFT, complex exponential, IFFT. This gives h.
4. Truncate h to 512 taps (10.7 ms) with a 64-tap half-Hann taper.

L is smooth (a difference of two envelopes of order ≤ O), so cepstral aliasing is negligible. Truncation:
on the bench vowels at ±4, ±7 and +12 st, the output harmonics are the same within 0.1 dB with 512 and
with 2,048 taps [A]. The bench keeps the check (§5).

**Filtering**: y = (1 − λ) · (h_prev ∗ x) + λ · (h_new ∗ x), with λ ramping linearly over the hop: two
512-tap direct convolutions (SIMD), never a block FFT, so the latency stays 0. The crossfade is
continuous (no click). Whatever differs between two successive filters modulates the sound at rates
tied to the hop and the pitch: that is the −71 … −33 dBc measured above, and why the window is what it
is.

**Latency: 0 samples, reported 0** (or `lookahead`). The minimum-phase FIR has the group delay of the
resonances it adds or removes, as any analog filter does. "Zero latency" is a claim about the audio
path only: the envelope lag above is the engine's real cost and the manual must say so.

**Scheduling**: a frame's FFTs and TE iterations are spread over the next **64 samples** (1.3 ms) on
sample counts, as gfx-glitch's engine does: the output does not depend on the block size. A whole hop
(the first design) added 4 ms to the lag to flatten a load of ≈ 0.1 ms per frame.

### 2.3 Grain engine: fixed-delay PSOLA (gfx-dsp `psola`, from gfx-golem)

- **Moved from gfx-golem** without behaviour change into gfx-dsp `psola`:
  - the epoch tracker: predicted marks, polarity held per segment, sub-sample refinement, low-pass
    group-delay compensation;
  - the voicing gate and the octave-error rejection;
  - `level_norm`, the Hermite read, the register table.
  - **Guard: a bit-exact null**, not the −50 dB of the contract. A move changes no arithmetic, so
    gfx-golem's output after it must equal its output before it, sample for sample. The reference
    renders are made **before** the move (bench signals and the sung phrase; the three registers; a
    locked octave, a free interval, `formant` ±4, harmony on; 44.1, 48 and 96 kHz) and kept. Any
    difference is a behaviour change and stops the move. gfx-golem's VST3 class id, parameter ids and
    tests are untouched; nothing to announce to golem-rack if the null is exact.
  - The move is the tracker, the gate and the helpers. gfx-golem's overlap-add writer and its
    phase-locked scheduler stay in gfx-golem.
- **New: a streaming fixed-delay reader.** gfx-golem writes a whole grain into an overlap-add buffer once
  the next mark is known, which costs about 2.25 periods. Here every output sample reads the active
  grains directly: two for α ≥ 1, up to three for α down to 0.5 (a right half lasts T / α). Output time t
  reads grain k at input position m_k + (t − m_k − D) · α, windowed by gfx-golem's asymmetric Hann
  halves (left half = the previous period, right half = the next). Formants down (α < 1) stretch the
  right half only, as in gfx-golem.
- **Fixed delay D.**
  - Read in gfx-golem's tracker: mark k is searched within ±0.25 T̄ of its prediction m_{k−1} + T̄ (T̄ the
    smoothed period) and is known at m_{k−1} + 1.25 T̄ + gd + 2, wherever it falls in that window.
    Grain k − 1's right half and grain k's left half both start at output time m_{k−1} + D and both need
    m_k. So the condition is **D ≥ 1.25 T̄ + gd + 2**, exactly. With α > 1 the read must not pass the
    input: D ≥ T (α − 1) / α, always below it.
  - So D = ⌈1.25 · T_max + gd + 2⌉ samples, with T_max the register's lowest period and gd = √2 /
    (2π · 1 kHz) the marker low-pass delay [A, checked]:

    | `register` | Lowest f0 | D at 48 kHz | Reported latency |
    |---|---|---|---|
    | Low | 60 Hz | 1,013 samples | 21.1 ms |
    | Mid (default) | 90 Hz | 680 samples | 14.2 ms |
    | High | 150 Hz | 413 samples | 8.6 ms |

  - The bound holds while T̄ ≤ T_max. The reader therefore clamps the period it predicts with to T_max,
    and when a mark is not confirmed in time (a note at the register's floor with vibrato, a tracking
    slip) it **commits the predicted mark** as the grain boundary. Both halves that meet there use the
    same value, so they still sum to one: the identity holds, only that boundary is off the epoch.
  - A note under the register's floor freewheels at T_max (the doubles of gfx-golem do the same).
- **Identity**: at α = 1 and r = 1 the read position is m_k + (t − m_k − D) = t − D, an integer. The
  asymmetric halves sum to one, so voiced output = input delayed by D, exactly.
- **Pitch** (`shift`, Grain only). Synthesis marks are spaced T / r. Each takes the **latest** analysis
  mark at or before t_syn − D, never the nearest: the nearest could lie up to T / 2 later and break the
  bound on D. At r < 1 grains are skipped, at r > 1 repeated. The slew is 1 st / ms, as gfx-golem.
- **Unvoiced, silence, polyphony**: the voicing gate crossfades (10 ms open, 30 ms close) to the input
  delayed by D through the Live filter: at α^(`unvoiced` / 100) on noise-like frames, at the full α on
  tonal ones (§2.2). The gate is driven by the undelayed detector, so it acts D ahead of the audio it
  switches: a consonant never reaches the grain path. A chord closes the gate, so Grain on a guitar falls
  back to Live's filter by itself, fully shifted.
  - The fallback filter analyses the undelayed input and filters the input delayed by D: it has D of
    lookahead for free (8.6 – 21 ms), so its envelope lag is mostly gone.
  - A voiced onset runs through the fallback until the tracker has three marks (gfx-golem: first sound
    20 – 27 ms after the onset), i.e. for about 20 – 27 ms − D, then crossfades to the grains. The two
    paths agree on the envelope and differ in phase: the 10 ms crossfade may dip. Bench: Grain fallback.
- **Level**: gfx-golem's level_norm(r, α), checked by the bench.
- **One `psola` API with gfx-correct** (its §2.8; the first of the two to be coded makes the move):
  - **Tracker**: returns the marks, the smoothed period, the clarity and the gate (`sens`, `thresh`),
    **and the predicted next mark** (Live's growl needs it). Backward seeding at a voicing onset
    (gfx-correct §2.2) is an option: off for gfx-golem (its null is bit-exact), **on here**, so the
    grains start with YIN's first reading and the onset spends less time in the fallback.
  - **Reader**: delay D given by the caller (≥ 1.25 · T_max + gd + 2 samples, the "+ 0.3 ms" of
    gfx-correct: here exactly that minimum, gfx-correct's Studio 2 · T_max + 4 ms); per-grain r, α and
    **gain** (growl); one or two voices; identity at r = α = 1; the period clamp and the committed
    predicted mark above. The caller owns the gate: gfx-correct runs the reader ungated (unvoiced sound
    passes at r = 1), gfx-formant crossfades to its filter.

### 2.4 Throat: growl, breath, tilt

- **`growl`** (period doubling, the vocal-fry / false-fold growl). Alternate glottal periods are
  attenuated by d = 0.8 · `growl`, and every period also gets a seeded random gain (± 0.3 · `growl`²,
  roughness).
  - **Grain**: per grain, exactly epoch-synchronous.
  - **Live**: a gain switched at the **predicted epochs** of the `psola` tracker, which runs in Live too
    (≈ 0.1 % CPU). The tracker knows mark k a quarter period after it and predicts mark k + 1 at
    m_k + T̄: the gain switches 0.2 T̄ before that prediction, in the quiet end of the period, with 1 ms
    raised-cosine edges. Zero latency.
    - Why not gfx-octaver's divider (the first design): a zero crossing of the fundamental falls
      anywhere in the glottal cycle, so the switch would cut a pulse's formant ringing in two; switched
      between pulses, each pulse and its ringing are scaled as one, which is what a throat does when
      alternate pulses are weaker. The tracker also cannot double-trigger on a strong second harmonic
      as a Schmitt trigger does, both engines growl on the same marks, and gfx-octaver is left alone.
  - Active only while voiced.
  - **Roughness, not tremolo**: the modulation runs at f0 / 2, 30 Hz for the lowest note of `register`
    Low and 45 Hz and more elsewhere, above the rate where a level change is heard as one.
  - Each harmonic gains sidebands at ±f0 / 2. At `growl` 100 % (d = 0.8), relative to the harmonics of
    the **output** (whose mean gain is 1 − d / 2): Live (square gain) d / π / (1 − d / 2) = −7.5 dB;
    Grain (Hann halves: a sine gain) d / 4 / (1 − d / 2) = −9.5 dB [A].
- **`breath`**: seeded white noise, times the input's amplitude follower (10 ms), times the voicing gate,
  high-passed at 1 kHz.
  - It is shaped by a second minimum-phase FIR built from the **target** envelope (same cepstral fold,
    one more FFT per hop). Only computed when `breath` > 0.
  - 100 % puts the noise 12 dB under the voice's RMS.
- **`tilt`**: a log-linear term in L (§2.2). Negative = darker, breathier source; positive = pressed,
  brighter.

### 2.5 Layouts, latency, CPU

- **Mono 1 / 1 first**, as the contract asks. **2 / 2**:
  - Live analyses the mid and applies the same filter to both channels, so the image holds.
  - Grain processes the mid and sends it to both outputs (a voice is mono).
- **Latency**: Live 0 (or `lookahead`); Grain D by `register` (table above).
  - `engine`, `register` and `lookahead` are **setup parameters**, never scene values: a reported latency
    that changes at runtime makes REAPER and Ardour re-sync (gfx-glitch's `align` rule).
  - The dry path of `mix` is delayed by the same amount, so the blend never combs.
- **CPU** estimate [A, to measure], mono, 48 kHz:

  | Part | Cost |
  |---|---|
  | YIN detector (gfx-golem's ranged instance) | ≈ 0.3 % |
  | Per hop (5.3 ms): 3 FFTs of 2,048 (the three spectra); TE, 2 – 7 iterations × 2 FFTs of 1,024 (cap 8); fold: 3 FFTs of 2,048; O(N) loops: ≈ 80 – 110 µs | ≈ 1.5 – 2 % |
  | Two 512-tap convolutions per sample (≈ 49 M MAC / s). Measured in C on an i9-13900H: 107 ns / sample with SSE2, 75 ns with AVX2 [A] | 0.5 % (0.36 %) |
  | Breath FIR (when on): one more FFT per hop, one more convolution | + 0.4 % |
  | Epoch tracker (Live growl), Grain reader (gfx-golem measured 50 ns / sample for two voices) | + 0.1 – 0.2 % |

  Total ≈ 2.5 – 3 % (Live), a little more in Grain, ≈ 3.5 % with breath. Bench budget: **5 %** of one
  core at 48 kHz.
  - **High rates**: taps and rate both scale, so the convolutions cost 16× at 192 kHz: measured 4.7 %
    with AVX2, **9.8 % with SSE2** (1.2 % at 96 kHz with AVX2) [A]. The convolution is built with runtime
    AVX2 dispatch, and the bench reports the 96 and 192 kHz figures; budget 10 % at 192 kHz.
- Real-time: buffers and FFT plans sized in `prepare` for 192 kHz; no allocation in `process`.
  Deterministic: seeded noise and roughness, reset on `reset`.

## 3. Parameters

New block: no catalogue ids to keep. Every range is linear. **Smooth**: EXP = the contract's EXP smoother;
hop = taken at the next analysis hop and crossfaded over it; setup = latency or engine change, applied on
re-prepare.

### 3.1 Front panel

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `formant` | Formant | f | −12 – +12 st | −3 st | EXP, hop | ✓ | Moves the envelope: below 0 bigger and darker (giant), above smaller and lighter (child, chipmunk). Pitch unchanged. gfx-golem's id and unit |
| `growl` | Growl | f | 0 – 100 % | 0 % | EXP | ✓ | Alternate periods attenuated, plus roughness: a growl at f0 / 2 |
| `vowel` | Vowel | f | 0 – 100 % | 0 % | EXP, hop | ✓ | Vowel template position a · e · i · o · u (0, 25, 50, 75, 100 %). Acts through `vowel_amt` |
| `vowel_amt` | Vowel Amount | f | 0 – 100 % | 0 % | EXP, hop | ✓ | 0 = the input's own formants (moved); 100 % = replaced by the vowel (talk-box) |

### 3.2 Expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `engine` | Engine | e | Live · Grain | Live | setup | | Live: zero latency, any source. Grain: PSOLA for a solo voice, latency by `register`, allows `shift` |
| `shift` | Pitch | f | −12 – +12 st | 0 st | per mark | ✓ | Pitch change, **Grain only**: the descriptor carries `depends: engine = Grain`, so golem greys it in Live, and Live ignores it. With `formant` it makes real gender and age changes |
| `source` | Source | e | Voice · Instrument | Voice | hop | | Voice: window and envelope detail from the sung pitch, unvoiced handling. Instrument: fixed window and detail (chords, bass, synth) |
| `register` | Register | e | Low · Mid · High | Mid | setup | | gfx-golem's ranges (60 – 500, 90 – 800, 150 – 1,200 Hz). Grain latency 21.1 / 14.2 / 8.6 ms |
| `unvoiced` | Unvoiced Shift | f | 0 – 100 % | 100 % | 30 ms | | Share of the formant shift applied to consonants and breath (noise-like frames; never chords). 100 % = everything moves (physical); 50 % = softer sibilants on big shifts |
| `tilt` | Tilt | f | −6 – +6 dB/oct | 0 | hop | | Source tilt around 1 kHz, clamped ±12 dB |
| `breath` | Breath | f | 0 – 100 % | 0 % | 30 ms | ✓ | Aspiration noise shaped by the new envelope |
| `thresh` | Threshold | f | −90 – 0 dB | −60 dB | 20 ms | | Below it the envelope is held and Grain's gate closes (gfx-golem's id) |
| `lookahead` | Lookahead | f | 0 – 20 ms | 0 ms | setup | | Live only (`depends: engine = Live`): delays the audio to centre the analysis; reported as latency, rounded to whole samples. 10 ms halves the envelope lag, 20 ms removes it above ≈ 115 Hz: the setting for speech and fast lines |
| `mix` | Mix | f | 0 – 100 % | 100 % | EXP | ✓ | **Linear** crossfade with the dry delayed to match. Dry and wet are coherent (same pitch, aligned), so equal power would add +3 dB at 50 %. In Live the blend is itself a minimum-phase filter, (1 − m) + m · H: a shallower shift, no comb |
| `output` | Output | f | −12 – +12 dB | 0 dB | 30 ms | | Output level |
| `bypass` | Bypass | b | off / on | off | 10 ms crossfade | | Shared id (no tail, so no `trails`) |

### 3.3 Meter (read-only)

| id | Name | Range | Role |
|---|---|---|---|
| `state` | Voice State | 0 – 2 | 0 below `thresh` (envelope held), 1 not tracked (noise-like or polyphonic: Grain runs its fallback), 2 voiced (pitch tracked) |

No mode-dependent defaults: every throat and vowel parameter is neutral at its default. `formant` −3 st
makes the bare pedal audible (a slightly bigger voice); golem pins its own value at insert. A default
that is not neutral is the suite's habit for an effect's main knob (gfx-freqshift's `shift` +2 Hz,
gfx-pitch's +12 st), and the shared id fixes the meaning and the unit, not the default (0 st in
gfx-golem). Enums only grow at the end.

`engine`, `register` and `lookahead` are **structural** in the descriptor: they change the reported
latency, so golem never puts them in a scene or on EXP. `engine` is on the face as the pedal's first
choice, made once per song, not a foot switch.

## 4. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Bigger voice, same singer | Live, `formant` −2 … −4 | Zero latency in the in-ears; TE envelope (no metallic shift) |
| Giant, monster | Grain, `shift` −5, `formant` −7, `growl` 40 %, `tilt` −2 | Pitch + formant together; epoch-synchronous growl |
| Child, chipmunk without the pitch | Live, `formant` +6 … +10, `unvoiced` 50 % | Melody kept; sibilants not thinned |
| Lighter voice, same notes (singing) | Live, `formant` +3 | Female formants are ≈ 15 – 20 % higher than male (Peterson & Barney): +2.4 … +3.2 st [A] |
| Gender / age change (speech, FX) | Grain, `register` Low, `shift` +7, `formant` +3; the reverse: `shift` −7, `formant` −3 | Pitch and formant both move: formant alone gives "a small man" |
| Demon voice | Grain, `shift` −12, `formant` −5, `growl` 60 %, `breath` 20 %, `mix` 100 % | All throat options |
| Growl on a sung line | Live, `formant` −1, `growl` 30 – 50 % | Growl on the predicted epochs, zero latency |
| Speech, rap, fast lines (live) | Live, `lookahead` 10 – 20 ms; or Grain | The filter keeps up with the articulation |
| Whisper ghost | Live, `formant` +2, `breath` 80 %, `tilt` −3 | Shaped breath noise |
| Guitar: smaller box / bigger box | `source` Instrument, `formant` ±4 | A tone effect that leaves pitch and attack alone (polyphonic, zero latency) |
| Guitar talk-box sweep | Instrument, `vowel_amt` 85 %, `vowel` on EXP (a → u) | Vowels imposed on chords |
| Bass giant | Instrument, `formant` −5, `register` Low | Low-end detail; noise-floor guard |
| Synth vowel pad | Instrument, `vowel` 50 % (i), `vowel_amt` 100 % | Talk-box without the tube |
| Studio double that sounds like another singer | Grain, `formant` +2, `shift` 0, `mix` 50 % on a parallel lane | Grain identity is exact: only the formants differ |

golem should place gfx-deesser and any gate **before** gfx-formant on a voice lane, and gfx-golem
**after** it if both are used.

**Preset ideas** (`presets/gfx-formant.json`, no artist names): `bigger-voice`, `giant`, `child`,
`chipmunk-same-notes`, `lighter-voice`, `gender-up`, `gender-down`, `demon`, `sung-growl`,
`whisper-ghost`, `guitar-small-box`, `guitar-big-box`, `guitar-talkbox`, `bass-giant`, `synth-vowel-pad`,
`other-singer-double`.

### Visual

- **Category `pitch`**, body hue `METAL_HUE.pitch` `#c4148c`, **anodized** (the family's finish).
  - No crate hue: the motif sets it apart from gfx-pitch (stairs) and gfx-freqshift (barber).
  - gfx-golem is blackwork violet, so the two voice pedals never look alike.
- **Motif `formants`** (new case in `motifParts`): *the envelope moves, the harmonics stay*.
  - Two columns, x ∈ [4, 16] and [W − 16, W − 4], over the rune zone only (y from cy − ch to cy + ch, as
    `barber`), clear of the knob grid.
  - **Harmonic ticks**: in both columns, horizontal 6 px lines every 5 px (opacity .35), identical left
    and right: the pitch does not change.
  - **Envelope**: a polyline sampled every 1 px. Frequency runs upwards. It is the sum of three
    raised-cosine bumps 6 px deep, pointing towards the rune.
    - Left column: bumps at 22, 42 and 62 % of the column height (the giant).
    - Right column: the same curve stretched ×1.3, bumps at 29, 55 and 81 % (the child).
    - Opacity 1.
  - Drawn with `line()`, so both finishes work.
- **Face** (`visuals/data/faces.json`):
  `"gfx-formant": {"name": "Formant", "blocks": ["formant"], "knobs": ["formant", "growl", "vowel", "vowel_amt"], "selectors": ["engine"], "category": "pitch"}`.
  "Formant" is a generic word. "Throat" is avoided (a product name).
- **Glyph** `formant.svg` (24 × 24, as `glitch.svg`): a three-formant envelope above a double arrow,
  "the envelope moves both ways". The envelope's lowest point (y 16) clears the arrowheads' top (y 19) by
  1.2 px after the strokes.

  ```
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>Formant shifter (gfx)</title><path d='M2.5 16C4.5 16 5 5.5 7 5.5s2.5 7 4.5 7 2.5-4.5 4.5-4.5 3 8 5 8'/><path d='M5 21h14M7 19l-2 2 2 2M17 19l2 2-2 2' opacity='.6'/></svg>
  ```

- **Catalogue entry** (for golem-rack): type `formant`, category `pitch`, plugin `VST3: GoleM FX Formant
  (GoleM)`, roles voice, guitar, bass, keys. It carries the golem-fx-only `quarantine` note, as
  `golem-double`.

## 5. Bench suite (on top of gfx-bench's common checks)

Test signals:
- a **synthetic voice**: Rosenberg pulses through /a/ formants 700 / 1,220 / 2,600 Hz (gfx-golem's), at
  f0 100, 220 and 400 Hz, with and without 6 Hz ±50 ct vibrato;
- noise bursts ("s", "f");
- a guitar chord DI and a bass DI;
- The maintainer's sung phrase (gfx-golem's listening voice).

Every check runs at 44.1, 48, 96 and 192 kHz.

| Check | Pass |
|---|---|
| Identity | `formant` 0, `vowel_amt` 0, `tilt` 0, throat 0: Live output = input bit for bit; Grain output = input delayed by D, ≤ −90 dB residual |
| Pitch kept | Live, `formant` ±7: every harmonic's frequency unchanged within ±0.5 ct (it is a filter); Grain within ±5 ct |
| Envelope moved | Steady vowel, `formant` ±4, harmonics from 200 Hz to 5 kHz against the synthesis envelope read at f / α, bins at the boost clamp left out. Live: ≤ 1.5 dB rms and ≤ 4 dB at 100 Hz, ≤ 2.5 dB rms and ≤ 6 dB at 220 Hz, ≤ 4 dB rms at 400 Hz (prototype: 0.9 – 1.1 / 3 – 4, 1.5 – 2.3 / 4 – 5, 2.9 – 4.0 / 6 – 9: at 400 Hz five harmonics sample three formants, for any method). Grain: the same bounds |
| TE start | /a/ → /i/ in 20 ms: 100 ms later the envelope is within 1 dB of an offline cold-start TE (20 iterations) from 200 Hz to 5 kHz: nothing is held |
| Glide | /a/ → /i/ in 60 ms at 150 Hz, `formant` −4 and +7: harmonic error against the moving target. Reported at `lookahead` 0 (prototype ≈ 10 dB rms); ≤ 3 dB rms at `lookahead` 20 ms and in Grain |
| High voice | f0 400 Hz, `formant` +4: no harmonic-peak artefact. The output's TE has no peak at a harmonic that the reference envelope lacks (> 3 dB) |
| Level | Pink noise and the vowel, `formant` −12 … +12: RMS within ±1 dB of the input (normalisation) |
| FIR truncation | The designed h against its target L(f): within ±0.5 dB from 100 Hz to 0.45 fs, for the bench vowels at ±12 st |
| Smoothness | Steady vowel, steady settings, `formant` ±4 and ±7, f0 off the hop grid (98.3, 147.1, 221.7, 311, 397 Hz): strongest non-harmonic component ≤ −50 dBc up to 220 Hz, ≤ −35 dBc above (prototype: −71 … −52, −42 … −33). A voice's own noise sits at −20 … −30 dBc between its harmonics; a synth pad's does not: Instrument's 150 ms smoothing covers that case, ≤ −60 dBc on a steady sawtooth |
| Sweep | `formant` swept −6 → +6 st in 1 s on a sustained vowel: non-harmonic components ≤ −45 dBc; EXP rule (50 Hz-stepped sweep, control-rate sidebands ≤ −80 dB) |
| Lag | Vowel /a/ → /i/ step: the filter reaches 90 % of the new ratio within 1.83 T + 64 samples + 1 hop (Live, lookahead 0); within 1 hop with `lookahead` 20 ms above 115 Hz |
| Unvoiced | Noise burst, `formant` +7: its spectral centroid moves by α^(`unvoiced`) within 10 %, at 100 % and 50 % |
| Noise floor | Vowel at −20 dBFS over −80 dBFS noise, `formant` −12: the floor rises ≤ 1 dB in silent gaps; L never exceeds +24 dB |
| Onset | A vowel from silence: no burst above the vowel's level in the first 20 ms (envelope held under `thresh`) |
| Polyphony | E2 B2 E3 G♯3 (synthetic, then the guitar DI), Instrument, `formant` +4: no new partial above −45 dBc (prototype −47 dBc; −25 dBc without the 150 ms smoothing). Voice mode on the same chord: classed tonal, full α, same bound |
| Frame class | Noise bursts classed noise-like, chords and a sustained synth classed tonal, within 2 frames; no flapping on a breathy vowel (`state` steady) |
| Vowel | `vowel_amt` 100 % on pink noise: output TE peaks within 5 % of the template's F1 – F3 at the five positions |
| Growl | Voice at 150 Hz, `growl` 100 %: components at f0 / 2 ± harmonics, relative to the output's harmonics, at −7.5 ± 2 dB (Live) and −9.5 ± 2 dB (Grain); 0 % → none above −90 dB. Live's switch points fall in the last 40 % of each period (between pulses) |
| Breath | `breath` 100 %: noise 12 ± 2 dB under the voice RMS, its TE within ±3 dB of the target envelope above 1 kHz; 0 when unvoiced gate closed |
| Grain pitch | `shift` ±7: pitch within ±5 ct; level within ±1 dB; no click on a scene change of `shift` |
| Grain fallback | Chord into Grain: gate closed, output = delayed Live filter path at the full α, no grain buzz. Voiced onset: no level dip above 3 dB while the fallback crossfades to the grains |
| Grain bound | A note at the register's floor with ±50 ct vibrato, and one 20 % under it: no click, identity still ≤ −90 dB (committed predicted marks) |
| `psola` move | gfx-golem before and after the move: **bit-exact** on the stored renders (§2.3) |
| Latency | Live reports 0 (and `lookahead`); Grain reports D per register; measured with an identity setting: the impulse lands at the reported sample |
| Clicks | Scene changes of every non-setup parameter: no click above the common bound |
| Mono / stereo | 2 / 2 with equal inputs: L = R bit-exact, and equal to 1 / 1 |
| Determinism, CPU | Strict null (seeded noise and roughness); ≤ 5 % of one core per mono instance at 48 kHz, ≤ 10 % at 192 kHz, both reported |
| Listening | `gfx-bench apps gfx-formant` (one render per Applications row) on the sung phrase, guitar and bass DIs; blind A / B Live vs Grain at `formant` ±3 / ±7 on voice |

**Prototype gate, before any Rust** (as for gfx-golem): the Live engine offline, on the maintainer's sung phrase
and a spoken sentence, at `formant` ±3 and ±7, with `lookahead` 0, 10 and 20 ms, against Grain's reader
with ideal marks. Three questions for the ear: is the blur on fast articulation at 0 ms acceptable for
singing; is the steady-state residue (−50 dBc and better up to 220 Hz, −35 dBc above) inaudible; +18 or
+24 dB for the clamp. The numpy prototype of this review is the starting point (`proto-gfx-formant/`
in the review's scratchpad: to be copied into the crate if kept).

gfx-bench options:
- latency check on the identity rows only; the minimum-phase group delay is expected otherwise
  (gfx-crusher's precedent);
- step-click bound: default.

## 6. Reuse from gfx-dsp and the suite

| Reused | New or improved |
|---|---|
| gfx-golem: epoch tracker, voicing gate, octave rejection, asymmetric Hann grains, Hermite read, `level_norm`, `register` table → moved to gfx-dsp **`psola`** unchanged (bit-exact guard, §2.3). Shared with gfx-correct: one API (§2.3) | **Fixed-delay streaming reader** (D = 1.25 T_max + gd + 2, against gfx-golem's 2.25 T); latest-mark selection for `shift`; committed predicted marks; the tracker's predicted next mark (Live growl) |
| gfx-dsp `detect` (YIN, `prepare_range`, `set_range`) | — |
| gfx-glitch `engine`: frame work scheduled on sample counts; `realfft` plans | **`specenv`**, in three parts a caller can take separately: (1) **envelope**: frame class, k-period Hann window with three spectra, cold-start TE → log E; (2) **log-ratio composition** (shift, vowel, tilt, clamps, noise-floor guard, level, a weight 0 – 1); (3) **cepstral minimum-phase FIR** and the crossfaded convolution. f0, the voicing and α come from the caller: `specenv` owns no detector |
| gfx-octaver's divider | **Not moved, not used** (growl runs on predicted epochs, §2.4). gfx-dsp's planned `divider` waits for a second user |

**Other users of `specenv`.**
- **gfx-correct, Live formants (its §2.6, stage 2)**: served as is. It runs parts 1 – 3 on its shifter's
  **output**, with the output's pitch (f0 · r) as f0, α = 1 / r per hop and its own weight. What it
  inherits: the envelope lag (1.8 T + 4 ms; harmless there, the vowel is the same before and after the
  shifter, only r moves), the steady residue above 250 Hz, and a cost nearer 2.5 % of a core than its
  3 % estimate. At ±2 st the ratio stays within a few dB, far from the clamps. Its gate (blind
  preference on the sung phrase) stands; `specenv` must first pass this document's prototype gate.
- gfx-pitch Poly formant preservation (deferred there for latency) and gfx-golem's large shifts:
  possible later, tonal frame class.
- A vocoder's modulator envelope: part 1 alone.
| gfx-autowah's vowel order and log-frequency interpolation | Three-formant Peterson & Barney templates + F4 / F5, applied as an envelope (moved by `formant`) |
| `svf`, `envelope`, `exp_smooth`, `smooth` | — |

## Open questions for review

Settled by the review of 2026-11-14 (reasons in the Review below).

1. **A third, STFT "Studio" engine?** → **No.** Grain is the studio engine for a solo voice (nothing
   estimated, exact articulation); Live with `lookahead` 20 ms measures at its steady-state error on a
   moving vowel. An STFT engine would add a third sound to maintain for the polyphonic studio case only.
2. **`shift` in Grain only** → **kept**, with `depends: engine = Grain` in the descriptor (golem greys
   it). No label trick in the plugin.
3. **`formant` range ±12 st** → **kept** (gfx-golem's id, unit and range). Beyond +7 the boost clamp
   already limits what is reached.
4. **Default `formant` −3 st** → **kept**. Not a mode-dependent default; the suite's main knobs are
   audible at insert; golem pins its own value.
5. **Hop 256 against 128** → **256.** The lag is the window's (1.8 T), not the hop's: 128 would save
   2.7 ms of ≈ 16 – 22 for twice the analysis. The 4 ms that were free came from the scheduling.
6. **TE warm start** → **removed**: it turns TE into a peak-hold (measured). Cold start, 8 iterations,
   Δ 2 dB confirmed.
7. **Growl in Live** → **predicted epochs** of the `psola` tracker, zero latency; the divider is not
   used. By ear at the prototype gate: `growl` depth law.
8. **Moving gfx-golem's engine into `psola`** → **agreed**, guarded by a bit-exact null on renders
   stored before the move; one API with gfx-correct.
9. **Vowel bandwidths and F4 / F5** → by ear on guitar, as proposed. Stays [I].
10. **Catalogue** → new type `formant` with the quarantine note; `pitch-spectral`'s `formant_comp` is
    never routed here.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good; the Live engine is at risk until a listening prototype passes.** The architecture is
right: two engines with opposite strengths, one log-spectral domain, PSOLA reused instead of rewritten,
and the envelope ratio as a minimum-phase FIR is a sound idea (it does keep pitch and phase, and the
512-tap truncation is harmless). Grain is solid: its delay formula is exact. But Live as written did not
work: prototyped in numpy on source-filter vowels (additive source, formant cascade with higher poles;
`proto-gfx-formant/` in the review's scratchpad), it gave −20 dBc of inharmonic residue on a steady
vowel, an envelope that could never come down, and −25 dBc of intermodulation on a chord. All three are
fixed below, by measurement. What cannot be fixed is the lag of a causal envelope: "zero latency" is
true of the audio and false of the filter, and only the ear can say how much that matters on singing.

### Measured (48 kHz, synthetic /a/ and /i/, f0 98 – 400 Hz)

| Question | Finding |
|---|---|
| Is Live zero-latency? | The audio path, yes (causal minimum-phase FIR, reported 0). The filter lags ≈ 1.8 T + 4 ms after the fixes (the design said "one period"; its own figures gave 0.3 L + 2 hops ≈ 20 ms at 100 Hz) |
| Fast vowel changes | /a/ → /i/ glides of 20 and 60 ms at 150 Hz: ≈ 10 dB rms harmonic error (peaks 25 – 35 dB) during the glide plus the lag, against 1.4 – 2.9 dB steady. `lookahead` 10 ms: 5 – 7 dB; 20 ms: ≈ 2 dB. No click: the per-hop crossfade is continuous |
| Does E(f / α) / E(f) move the formants cleanly? | Steady vowel, ±4 st: harmonics within 0.9 – 1.1 dB rms of the true envelope at 100 Hz, 1.5 – 2.3 dB at 220 Hz, 2.9 – 4.0 dB at 400 Hz. Harmonics stay put (it is a filter). The metallic risk is not the ratio, it is its **time variation** (next rows) |
| Asymmetric 3-period window | Harmonic levels move 2.3 – 7 dB from hop to hop; strongest non-harmonic component −20 dBc at every pitch. Symmetric Hann: −51 … −30 dBc. Hann + three spectra T / 3 apart: −71 … −52 dBc up to 220 Hz, −42 … −33 dBc at 311 – 397 Hz. An f0-guided harmonic-peak envelope was tried and is no better |
| TE warm start | A peak-hold: +36 dB at 1 kHz 400 ms after a vowel change. Cold start converges in 2 – 7 iterations on these windows |
| Boost clamp | +18 dB leaves upward-moved formants up to 10 dB short at +7 st; +24 dB halves the error |
| FIR | 512 taps against 2,048: same harmonics within 0.1 dB |
| Chords (Instrument) | New partials at −25 dBc on E2 B2 E3 G♯3 (beating partials modulate the estimate); −47 dBc with O = 60 and 150 ms of envelope smoothing |
| CPU | Two 512-tap convolutions: 0.36 % (AVX2) to 0.51 % (SSE2) at 48 kHz, as claimed; 4.7 – 9.8 % at 192 kHz, not stated. Per-hop work ≈ 80 – 110 µs with the three spectra: Live ≈ 2.5 – 3 %, not 2 % |

### Changes made, and why

| # | Change | Why |
|---|---|---|
| 1 | **Voiced window: symmetric Hann of exactly k periods, three power spectra T / 3 apart** | The first design's short falling edge made the estimate depend on the pitch phase: −20 dBc of residue, the "metallic" fault the document set out to avoid |
| 2 | **TE: cold start**, warm start removed | Correctness: the iteration only raises the envelope |
| 3 | **Envelope lag restated** (1.8 T + 4 ms), its cost measured, `lookahead` re-documented (20 ms, not 10, removes it); "truly zero-latency" reworded; a speech row in Applications | "One period late" was wrong by a factor of two, and the cost on moving vowels was not stated anywhere |
| 4 | **Scheduling over 64 samples**, not a hop | 4 ms of lag for nothing |
| 5 | **Three frame classes**: voiced, tonal untracked, noise-like (spectral flatness). `unvoiced` acts on noise-like frames only | "Voice without a pitch" and "unvoiced" were the same test with two different windows; and Grain's fallback applied α^(`unvoiced`) to chords: a guitar at `unvoiced` 50 % got half the shift |
| 6 | **Tonal frames: O = 60, envelope smoothed 150 ms** | Chord intermodulation −25 → −47 dBc |
| 7 | **Clamp +24 dB** (to voice against +18) | Measured shortfall of the moved peaks |
| 8 | Max-pooling centred on 3 bins | Pairs shift the envelope by half a bin |
| 9 | **Grain: D's condition written from the tracker's code** (1.25 T̄ + gd + 2, exact); period clamp and committed predicted marks; "two active grains" → up to three for α < 1 | The stated confirmation time ignored where the mark falls in its window (the result was right by luck: the two terms cancel). The bound had no margin at the register's floor |
| 10 | **Grain fallback**: full α on tonal frames; its free look-ahead D noted; onset crossfade given a bench row | See 5 |
| 11 | **Growl in Live on predicted epochs; `divider` not moved** | Switching between pulses scales a pulse with its ringing (a throat); a zero crossing of the fundamental does not. One refactor of a shipped effect fewer. Sideband levels restated relative to the output (−7.5 / −9.5 dB): "−12 dB" was relative to the unmodulated harmonics and matched neither engine at the output |
| 12 | **`psola` move guarded by a bit-exact null** on renders stored first; scope of the move stated; **one API with gfx-correct** (predicted mark, backward seeding as an option, D and gate owned by the caller, per-grain gain) | A −50 dB null would let a behaviour change through. gfx-correct's §2.8 is accepted with three additions it does not need itself: the predicted mark, the per-grain gain, the committed boundary |
| 13 | `specenv` split in three parts with f0, voicing and α from the caller | So gfx-correct's stage 2 and a vocoder can use it; confirmed for gfx-correct (§6) |
| 14 | Parameters: `shift` and `lookahead` carry `depends`; `engine`, `register`, `lookahead` structural; `state` 1 = not tracked; the non-neutral default justified; gate thresholds fixed at gfx-golem's `sens` 50 % | golem's descriptor already has these fields; "shows Grain only" was not something a VST3 float can do |
| 15 | CPU table and budget (5 % at 48 kHz, 10 % at 192 kHz, AVX2 dispatch) | Measured |
| 16 | **Bench**: Envelope moved with realistic bounds and clamped bins left out; TE start, Glide, Frame class, Grain bound, `psola` move added; Smoothness and Polyphony restated in dBc with achievable targets; Growl levels; the **prototype gate** | "≤ 0.3 dB hop to hop" and "−70 dB" were out of reach for any per-hop estimator; −60 dBc on chords likewise |

Differences left with gfx-correct's text: it says "the two active grains" (true for its α ≥ 0.89) and
"+ 0.3 ms" for gd + 2 samples (0.27 ms at 48 kHz): same thing. Its 3 % for the stage-2 filter is on the
safe side.

Checked and left as written: the survey's method table and the rejection of LPC (gfx-correct's
prototype agrees: 3.2 – 4.2 dB at order 20); TE's order fs / (2 f0) and Δ 2 dB; the cepstral fold; the
D table (1,013 / 680 / 413 samples recomputed); the identity argument in both engines; the latest-mark
rule for `shift`; the +2.4 … +3.2 st figure; `mix` linear; linear ranges; no mode-dependent default;
enums; mono first. Not re-checked at the source: the products' latency figures ([F], [U] stay).

Coverage: pitch + formant independent (AlterBoy, Vocal Bender, VT-4), formant amount, throat options
(THROAT), vowels on chords: nothing missing for the block. Not taken, rightly: robot mode (gfx-correct at
`filter` 0 does it), modulators (EXP and scenes), per-formant editing.

**Visual: accepted.** Pitch magenta anodized, no crate hue: gfx-correct takes its own hue precisely
because this pedal keeps the family's. `formants` (two side columns, ticks and a three-bump envelope)
cannot be mistaken for `stairs` (one full-width staircase) or `barber`. The ×1.3 stretch is right
(28.6, 54.6, 80.6 %). Glyph geometry checked: 1.2 px between envelope and arrowheads; the arrowheads end
at y 23.9 of 24, tight but inside.

### What stays open

| Item | For |
|---|---|
| **Prototype gate** (§5): Live at `lookahead` 0 / 10 / 20 ms against Grain on the sung phrase and on speech. If 0 ms is not good enough on singing, the default `lookahead` becomes 10 ms and Live is no longer "zero latency" by default | The maintainer |
| Residue above 250 Hz (−42 … −33 dBc): a period is too short for T / 3 offsets on whole samples. Try fractional offsets (phase ramps on the spectra), five spectra, or 5 – 10 ms of envelope smoothing (measured: about 5 dB) | golem-fx, at the prototype |
| Reducing the blur at 0 ms: scale L down while the envelope moves fast (spectral flux), so a stale ratio is applied less. Not tried | golem-fx, at the prototype |
| Clamp +18 or +24 dB; `growl` depth law; vowel bandwidths, F4 / F5; flatness threshold of the frame class | By ear |
| A `sens` parameter (gfx-golem's id) if breathy voices drop out of Grain: can be appended later | The maintainer |
| 192 kHz cost (≈ 5 – 10 %): accepted, or a shorter FIR there | The maintainer |
| Who makes the `psola` move (the first of gfx-formant and gfx-correct to be coded) | golem-fx |
| New catalogue type `formant` (category `pitch`, quarantine note); `depends` and structural flags honoured; `engine`, `register`, `lookahead` never in scenes; voice lane order (deesser and gate before, gfx-golem after) | golem-rack |
