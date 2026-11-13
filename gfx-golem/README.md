# gfx-golem

> **Status: DESIGNED, reviewed** (2026-11-13) — Fable 5.2's design review is folded in. **Gate before any
> plugin code: an offline prototype on a real sung phrase** (§6), listened against the reference.

golem's **live vocal doubler**: the voice of clay. It adds one or two pitch-shifted copies of a singer's
voice — first and foremost **a double one octave below**, tight under the lead, the sound of Sleep Token's
*Emergence* chorus — with a timbre that stays a voice (formants kept, or moved on purpose), at a latency a
singer in in-ear monitors does not notice. It is a new block for golem's voice lanes, set per scene.

It is not a guitar pitch shifter: gfx-pitch stays golem's Whammy / harmonizer / polyphonic shifter for
instruments. gfx-golem reuses gfx-dsp's pitch detector (with a voice range, §3) and gfx-pitch's key /
scale harmony, and its parameter ids where the meaning is the same.

## 1. The sound it is built for

Measured on the reference (Sleep Token, *Emergence*, the sung passage 1:09 – 1:30; mid / side STFT,
8,192-point frames every 50 ms, this repo's analysis of 2026-11-13):

- the sung line sits around **387 Hz** (median, ≈ G4);
- an **octave component** follows it: the ratio of the sung fundamental to the low peak has a median of
  **2.000** and stays within 2 % of 2 in 64 % of the frames, following the melody note for note;
- its fundamental peak sits **≈ 9 dB under the lead's fundamental peak** (peak against peak, not an RMS
  level);
- a fifth below is far weaker (−19 dB): an octave, not a harmony;
- it is **centred**: side / mid −4 dB against ≈ 0 dB for the lead's band.

What the mix cannot tell: whether this octave is a pitch-shifted double, a sung or synthesised low octave,
and whether its formants were kept or lowered. The pedal offers both timbres; the listening test decides.

## 2. State of the art

Survey of 2026-11-13. **[V]** read in a paper or a source · **[I]** inference or not re-checked.

| Reference | What makes it special | What we take |
|---|---|---|
| TD-PSOLA (Moulines & Charpentier, *Speech Communication* 9, 1990) [V] | Pitch-synchronous overlap-add: grains of two pitch periods around the glottal epochs, re-spaced at the new period. The grains carry the spectral envelope as sampled at the voice's harmonics | The engine |
| x42-autotune (GPL; gfx-pitch's README calls it the PSOLA model) [I] | The nearest open-source live PSOLA | Engineering reference |
| Formant shift by grain resampling [I] | Resampling each grain moves the envelope, re-spacing moves the pitch. (Praat's "Change gender" differs: it resamples the whole signal, then restores pitch and duration by PSOLA) | `formant` |
| Zynaptiq PITCHSHIFT (studio plug-in) [V, product pages] | ±12 st pitch and ±12 (PRO ±24) st formant, linked or independent; several engines; no latency published | Formant as a first-class knob; formant linked to pitch as an option |
| TC-Helicon VoiceLive, Boss VE-500, Eventide harmonizers [I] | Live vocal doubling and harmony, key-aware | The live use: scenes, a clean dry path |
| gfx-pitch Poly (ours, a phase vocoder, 6.6 / 10 ms) [V] | Polyphonic, low latency, no formant handling | Baseline for the listening gate; not the engine: its formants follow the pitch |
| gfx-dsp `detect` (ours) [V] | YIN on the decimated input, every ≈ 2.7 ms, 75 Hz – 1.5 kHz, clarity | Pitch and voicing, extended with a voice range (§3) |

gfx-pitch's README quotes 20+ ms for formant preservation (a vocoder with envelope estimation; Little
AlterBoy ≈ 60 ms). PSOLA needs no envelope estimate: its cost is the epoch tracking, hence the lower
figures of §3.

### What we conclude

1. **PSOLA on a monophonic voice**: the formant behaviour comes with the method, at about one period of
   delay. Its known weak point is a large downward shift (r = ½: isolated grains, a buzzy tendency): the
   prototype gate (§6) exists for that.
2. **The double is phase-locked to the lead.** Its synthesis clock comes from the lead's own epochs, not
   from a pitch estimate: no slow beating between the double's harmonics and the lead's, no comb that
   changes note by note.
3. **The dry voice is never delayed.** Only the double is late, by about one period of the sung note.
4. **Unvoiced sounds and stage bleed are left alone**: consonants, breath and between-phrase bleed never
   reach the double.
5. **Formant is its own knob**, independent of the interval.

## 3. Design

```
in ─┬──────────────────────────────────────────────────────────────────────────────────────────▶ (+) ─▶ out
    │                                                                                              ▲
    ├▶ voice detector (YIN, register) ─▶ voice tracker ─▶ voicing gate (hysteresis, hold, thresh) ──┐
    │                                         │                                                     │ × (gate)
    └▶ epoch tracker (predicted, polarity-held, sub-sample) ─▶ grain buffer ─┬▶ PSOLA voice 1 ─ × voice1 ─┤
                                                                              └▶ PSOLA voice 2 ─ × voice2 ─┘
                                                                         doubles: HP 60 Hz ─▶ LP `lpf` ─▶ pan
```

### Pitch and voicing

- **Detector.** gfx-dsp's YIN gains a ranged constructor, `prepare_range(sr, f_min, f_max)`: the window is
  tied to `f_min`, harmonic locking is optional and **off** here (it prefers 2× / 3× lags, which suits
  guitar pickups but invites octave-down errors on a rough voice), raw period and clarity are always
  exposed. gfx-pitch keeps its `prepare` untouched. `register` sets the range:

  | `register` | Range | For |
  |---|---|---|
  | Low | 60 – 500 Hz | Baritone, bass |
  | Mid (default) | 90 – 800 Hz | Tenor, alto |
  | High | 150 – 1,200 Hz | Soprano, head voice |

- **Voice tracker.** A ×2 or ÷2 jump shorter than 40 ms is rejected as an octave error; voicing opens
  after three agreeing frames (≈ 8 ms).
- **Voicing gate**, driving the doubles' gain:
  - opens at clarity 0.8, closes under 0.65 (hysteresis), with a 30 ms hold: a breathy note does not
    flutter, a detector dropout mid-vowel freewheels through the hold;
  - `sens` moves both thresholds: 0 % = open 0.9 / close 0.8 (only clean notes), 50 % = 0.8 / 0.65,
    100 % = 0.7 / 0.5 (rough notes followed too, more risk of artefacts);
  - **level gate** `thresh`: below it (input RMS), the doubles close whatever the pitch. Stage bleed
    between phrases (guitars, bass, wedges) is pitched and would otherwise be doubled;
  - ramps 10 ms open, 30 ms close. The gate is driven by the undelayed detector and applied to the delayed
    double: free lookahead, so a consonant closes the double before it reaches it.

### Epochs (the quality-critical part)

- **Marking**: on a low-passed copy (1 kHz, second order; its group delay is compensated), each mark is
  **predicted** at m(k−1) + T, with T the smoothed mark-to-mark period, and searched within ±25 % of the
  prediction with a penalty on deviation.
- **Polarity** is chosen once per voiced segment (from the asymmetry of the peaks) and held: mic and preamp
  polarity are not guaranteed, and a mark must not jump half a period when two peaks compete.
- **Sub-sample refinement**: each mark is refined by cross-correlating the new period with the previous
  one; grains are read and placed at fractional positions (cubic interpolation).
- **Unvoiced or dropout**: the tracker freewheels at the last period (the doubles are gated there).

### Synthesis

- **Grains** span the previous and the next period around their mark, windowed by **asymmetric Hann
  halves** (left half = previous period, right half = next period), so overlap-adding them at the original
  marks gives back the input even under vibrato.
- **Phase-locked clock.** Synthesis marks derive from the analysis marks.
  - For an octave interval r = 2^n (−12, −24, +12 st), each synthesis mark falls on a predicted input epoch
    (every second one for −12, every fourth for −24, and two marks per period for +12), each using the grain
    |n| periods back. The double is drift-free and coherent with the lead: it thickens by a constant amount
    at every pitch instead of flanging.
  - For other intervals, synthesis marks are spaced at T / r (T the smoothed mark-to-mark period, not the
    detector's estimate), each taking the analysis grain nearest in time; at r < 1 grains are skipped, at
    r > 1 repeated.
  - An interval change slews r per mark (1 st / ms, as gfx-pitch), extended by `glide`: no crossfade, no
    click.
- **Formant**: each grain is read at a rate 2^(`formant` / 12) (its length scaled accordingly).
  - Above 0 the read decimates: the grain is read through a 4× oversampled interpolator, so nothing
    aliases.
  - With `formant_link` on, the formant ratio is the interval's, and `formant` adds to it as an offset.
- **Level normalisation.** At r = ½ the grains no longer overlap and the double loses ≈ 3.7 dB of RMS; a
  formant ratio α scales it by ≈ r / α. A gain law g(r, α) — computed from the grain overlap and the
  window energy, checked by the bench — makes the double's RMS equal to the dry's at `voice1` 0 dB, for
  every interval and formant.
- **Delay of the double**: about one current period plus epoch confirmation (a quarter period and the
  low-pass delay): ≈ 4 ms at the reference's 387 Hz, ≈ 13 ms at 90 Hz, worst ≈ 19 ms at 60 Hz. The
  double's **first sound** comes 20 – 27 ms after a sung onset (detector window and voicing frames), plus
  the 10 ms ramp.
- **Second voice**: same engine, its own interval (`shift2`) and level (`voice2`, −60 dB = off, default).
  Formant is shared.
- **Harmony**: `harmony` Scale makes both intervals diatonic in `key` / `scale` (gfx-pitch's tables and
  `NoteTracker`). Known limits: the interval steps while the lead scoops into a note, and a singer more than
  40 cents off holds the previous note.

### Output

- **Doubles' tone**: a fixed 60 Hz high-pass and a low-pass `lpf` (2 – 16 kHz, default 6 kHz). Note that
  `formant` −12 already halves their bandwidth.
- **Layouts**: mono 1 / 1 first. In 2 / 2, voice 1 stays centred and voice 2 is placed by `pan`
  (equal-power, −100 % left … +100 % right); the dry is untouched. Mono sum stays within 1 dB of the 1 / 1
  output (no anti-phase anywhere).
- **Latency reporting**: the dry has zero latency and the plugin reports 0 (precedents: gfx-detune,
  gfx-pitch-classic; 4 – 19 ms is in the doubling range, not an echo). `align` delays the dry by the
  worst-case double delay of the `register` and reports it, for studio renders. `register` and `align` are
  **setup parameters**: latency is re-reported at initialisation or on their change only, never under
  scene automation (the host re-syncs: the gfx-flanger finding of the design review).
- **Meter**: a read-only `state` (0 = idle, 1 = tracking, 2 = gated by level), for setting `sens` and
  `thresh` at soundcheck.
- **Real-time**: buffers sized in `prepare` for the Low register at the highest sample rate; no allocation
  in `process`; deterministic.

## 4. Parameters

New block: no catalogue ids. Ids keep their suite-wide meaning: `shift`, `shift2`, `voice2`, `harmony`,
`key`, `scale`, `glide`, `fine` as in gfx-pitch; `lpf` (a low-pass in Hz) and `thresh` (dB) as elsewhere;
`state` as the contract's meters.

Columns: **Curve** (display only, every range is linear) · **Smooth** in ms (— = structural / setup) · **EXP**.

### Front panel

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `shift` | Interval | f | −24 – +24 st | −12 st | lin | per mark | ✓ | Interval of the main double (−12 = one octave below) |
| `formant` | Formant | f | −12 – +12 st | 0 st | lin | 50 | ✓ | The double's timbre: 0 = the voice's own, below = darker and bigger, above = lighter |
| `voice1` | Double Level | f | −60 – +6 dB | 0 dB | lin | 30 | ✓ | Level of the main double; 0 dB = as loud (RMS) as the dry voice |
| `lpf` | Tone | f | 2,000 – 16,000 Hz | 6,000 Hz | log | 30 | | Low-pass on the doubles |

### Expert

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `shift2` | Interval 2 | f | −24 – +24 st | +12 st | lin | per mark | | Interval of the second voice |
| `voice2` | Voice 2 Level | f | −60 – 0 dB | −60 dB (off) | lin | 30 | | Level of the second voice |
| `pan` | Voice 2 Pan | f | −100 – +100 % | +50 % | lin | 30 | | Equal-power position of voice 2 (voice 1 stays centred) |
| `fine` | Fine | f | −50 – +50 ct | 0 ct | lin | 30 | | Detunes both doubles: with `shift` 0, a unison double |
| `humanize` | Humanize | f | 0 – 100 % | 0 % | lin | 50 | | Slow seeded drift of the doubles' pitch (±8 ct) and timing (±10 ms): a sung double rather than a coherent one |
| `formant_link` | Formant Follows Interval | b | off / on | off | — | — | | On: formants move with the interval; `formant` becomes an offset |
| `glide` | Glide | f | 0 – 1,000 ms | 0 ms | log | — | | Slows an interval change (scene), on top of the 1 st / ms slew |
| `sens` | Sensitivity | f | 0 – 100 % | 50 % | lin | 30 | | Voicing thresholds (§3): higher follows rougher notes |
| `thresh` | Threshold | f | −90 – 0 dB | −50 dB | lin | 20 | | Level gate: under it the doubles close (stage bleed between phrases) |
| `register` | Register | e | Low · Mid · High | Mid | — | setup | | The voice's range; sets detection and the worst-case delay |
| `harmony` | Harmony | e | Off · Scale | Off | — | — | | Scale: intervals follow `key` / `scale` |
| `key` | Key | e | C – B | C | — | — | | For `harmony` |
| `scale` | Scale | e | gfx-pitch's scales | Major | — | — | | For `harmony` |
| `align` | Align Dry | b | off / on | off | — | setup | | Delays the dry by the double's worst-case delay and reports it (studio) |

### Meter (read-only)

| id | Name | Range | Role |
|---|---|---|---|
| `state` | Voice State | 0 – 2 | 0 idle, 1 tracking, 2 gated by level |

The pedal face shows Interval, Formant, Double Level and Tone.

## 5. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| *Emergence*-style octave double (the reason it exists) | `shift` −12, `formant` 0, `voice1` ≈ 0 dB (to set by the listening test: the measured −9 dB is fundamental against fundamental), `lpf` 6 kHz, mono | Phase-locked octave, formants kept, centred |
| Heavier, inhuman double | `shift` −12, `formant` −4 | Formants lowered |
| Choir-like thickening | `shift` −12, `shift2` +12, `voice2` −10 dB, `pan` +60 %, `humanize` 40 % | Two voices; voice 2 on one side (or two instances mirrored) |
| Unison double-tracking | `shift` 0, `fine` +8 ct, `humanize` 60 % | A sung-like double, no comb |
| Harmony in the key | `harmony` Scale, `shift` +4 (a third), `key` / `scale` of the song | Diatonic intervals |
| Chorus only, per scene | verse `voice1` −60 dB, chorus `voice1` 0 dB | Smoothed changes, no click |
| Deep, low voices | `register` Low | Detection down to 60 Hz (worst-case delay ≈ 19 ms) |
| Loud stage | `thresh` above the bleed level (read `state`) | Bleed between phrases never doubled |
| Studio | `align` On | Double under the lead, latency reported |

golem should place gfx-deesser and any gate before gfx-golem on a voice lane.

## 6. Prototype gate and bench suite

**Prototype gate (before plugin code).** An offline Python prototype of §3 renders a real sung phrase
(dry, the singer of the rig) at `shift` −12 with `formant` 0 and −4, against gfx-pitch's Whammy at −12 as
a baseline. Raph listens against the *Emergence* passage. If the octave-down PSOLA sounds buzzy or hoarse
beyond repair, the fallback is an LPC source-filter engine: a different design, to write then.

**Bench** (on top of gfx-bench's common checks). Test signals: a synthetic voice (Rosenberg glottal pulses
through /a/ formants: 700, 1,220, 2,600 Hz), with vibrato, plus noise bursts for consonants; delays are
compensated (the double is measured against the delayed dry).

| Check | Pass |
|---|---|
| Interval | The double's pitch is the input's × 2^(shift / 12) within ±5 cents, steady vowel, 100 – 500 Hz |
| Phase lock | Octave intervals: the lead's harmonics change by a constant amount across sung pitches 150 – 500 Hz (spread ≤ 0.5 dB) |
| Level | Double RMS within ±1 dB of the dry at `voice1` 0 dB, over `shift` × `formant` |
| Identity | `shift` 0, `formant` 0: double nulls against the delayed dry to −30 dB |
| Purity | Steady vowel, octave down: inharmonic residue of the double ≤ −30 dB (catches mark jitter) |
| Formants | At 100 – 120 Hz, octave down, `formant` 0: the double's harmonic levels follow the input's interpolated envelope within ±2 dB; `formant` −4: the envelope moves by 2^(−4/12) |
| Octave errors | A vowel with a strong 2nd harmonic and a rough vowel: no ÷2 / ×2 jump of the double |
| Unvoiced | From 50 ms after a noise burst ("s", "f"): double ≤ −40 dB re the dry; vowel → /s/ → vowel without a click |
| Bleed | Pitched input under `thresh`: doubles ≤ −60 dB |
| Onset | First sound of the double ≤ 40 ms after a sung onset |
| Vibrato | ±50 cents at 6 Hz: the double follows within ±10 cents (delay-compensated) |
| Clicks | A scene changing `shift`, `voice1` or `register`: no click above the common bound |
| Mono | 2 / 2 summed to mono within 1 dB of 1 / 1 |
| Latency | Dry 0 samples, reported 0; `align` On reports the `register` worst case and matches it |
| Determinism, CPU | Strict null (humanize seeded); ≤ 2 % of one core at 48 kHz, mono, two voices, Low register |
