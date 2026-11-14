# gfx-swell

> **Status: DESIGNED** (2026-11-14, design only; reviewed 2026-11-14, see the Review section at the end).
> Zero-latency auto-swell: each note, chord or legato change fades in on its own. It uses a level gate
> after silence and three onset detectors, each for a different kind of note change:
> - picks: a high-passed transient detector, from gfx-killswitch;
> - new energy without a pick (soft attacks, taps, new chords): a coarse "shape flux" that ignores vibrato,
>   bends and tremolo;
> - mono legato (hammer-ons, pull-offs, slides): a pitch-jump detector built on gfx-dsp's YIN.
>
> One swell per strum. The gain engine is reset-aware, so after a stopped note the attack is removed
> perfectly even at zero latency. Five rise curves, rise synced to tempo, a bow (attack + decay) mode, a key
> input, and an optional reported lookahead.

golem's **auto-swell** ("slow gear", auto volume pedal). A new golem block: there is no swell block in
golem's catalogue today. Checked on 2026-11-14 in `~/golem/library/catalog/effects.json` and
`visuals/data/golem-blocks.json`:
- "Swell" appears only as the display name of `mix` on the Echo Cat block (GxEchoCat).
- No plugin plays an auto-swell in golem.
- The closest features are golem-fx's own `swell` parameter in gfx-delay, gfx-reverb and gfx-shimmer
  (swell into the wet only), and gfx-killswitch's Ramp Up shape (tempo-locked, not note-locked).

The aim is the violin / bowed sound, played the way you play. Each picked note swells. So does each
hammer-on or new chord with no pick. A strum gives one swell, not six. A decaying note, vibrato, a bend or
hiss never re-trigger. It is the same at any input level, before or after the drive. No latency unless
asked.

**Rule for every threshold below: a false retrigger costs more than a miss.** A missed legato note simply
does not swell; a false onset at `dip` 100 % punches a hole in a ringing note. Detectors are tuned for
zero false onsets on the negative bench tests first, and for recall second.

## 1. State of the art

Survey of 2026-11-14. **[M]** manufacturer page or manual · **[R]** published review · **[F]** forum ·
**[V]** source code read · **[P]** paper · **[I]** inference.

### Hardware and multi-effects

| Reference | What it does | Strength | Weakness | What we take |
|---|---|---|---|---|
| [Boss SG-1 Slow Gear](https://support.roland.com/hc/en-us/articles/201958569-SG-1-Specifications) (1979–82) [M], [circuit notes](https://generalguitargadgets.com/wp-content/uploads/sgo_notes.txt) [F] | NF-1 gate circuit with its envelope reversed: the attack is cut below a threshold (Sens), then a VCA rises (Attack) | The reference sound | Boss's own GT manuals: low Sens needs "extremely hard" picking, max Sens "everything will trigger… noise" ([GT-X](https://www.manualslib.com/manual/604932/Boss-Gt-X.html?page=86)) [M]. Absolute threshold | Two knobs at the front: rise and sensitivity |
| [Mooer Slow Engine, Behringer SM200](https://equipboard.com/items/behringer-sm200-slow-motion) [M] | SG-1 clones: Sens + Attack | Cheap, simple | The same threshold problem | — |
| [EHX Attack Decay](https://www.guitarworld.com/gear/review-electro-harmonixs-attack-decay-tape-reverse-simulator-delivers-a-vast-universe-of-exciting-new-sounds-to-explore) (2019 reissue) [R] | Attack and decay envelope (a "tape reverse simulator"); mono or poly; linear, parabolic and S curves for each segment; sensitivity; EXP | Curves, decay ("reverse"), poly | "Fiddly", menus behind footswitches ([Premier Guitar](https://www.premierguitar.com/gear/electro-harmonix-attack-decay-review)) [R]; poly leaves artifacts [R] | Curve set, bow mode (attack + decay) |
| [EHX Pico Swello](https://www.gearnews.com/ehx-pico-swello-pog2-style-mini-auto-swell/) [R] | Polyphonic swell from the POG2's attack: each note in a chord or arpeggio swells separately | Chords keep ringing under new notes | Needs the POG2's frequency analysis (latency and cost) [I] | Not in v1: a per-note swell is a research item (§2.6) |
| [Malekko Sneak Attack](https://www.effectsdatabase.com/model/malekko/sneakattack) [M] | VCA attack / decay with length and **curve** per segment (decay 10 ms – 6 s); auto threshold, manual trigger from a footswitch, tap tempo | Manual trigger, tempo | Threshold-only detection | Manual `fire`, tempo sync |
| [Pigtronix Attack Sustain](https://www.premierguitar.com/gear/pigtronix-attack-sustain-pedal-review) [R] | Attack envelope + compressor-sustainer, external trigger input | Sustain before the swell | — | Key input (an external trigger, done the golem way) |
| [Fractal Volume/Pan, Auto-Swell](https://www.fractalaudio.com/downloads/manuals/fas-guides/Fractal-Audio-Blocks-Guide.pdf) [M, read] | Threshold, Attack, Release; **Hysteresis**: "how much your signal has to drop before the block resets"; tapers Linear, Log 30A … 5A, and an "S-taper designed especially for volume swells" | Pot tapers; explicit reset | Resets only when the level drops: legato and re-picks over a ringing note never re-swell [I] | Pedal taper and S-curve; reset depth (`reset`) |
| Helix Auto Swell ([thegearforum](https://thegearforum.com/threads/helix-autoswell-could-someone-please-explain-a-bit-the-rel-offset-parameter.5650)) [F] | Attack, Decay, Threshold (dBFS), ROffset (the reset threshold relative to the start one) | — | A new guitar's low E did not trigger at −56 dB (needed −70); ROffset "no audible difference"; users must "mute your sound to get the Autoswell to reset" ([Line 6](https://line6.com/support/topic/21265-some-advice-to-replicate-boss-slow-gear)) [F] | Thresholds relative to the playing level, not dBFS |
| [Strymon TimeLine Swell / BigSky Swell](https://www.manualslib.com/manual/963033/Strymon-Bigsky.html?page=12) [M] | Swell into a delay or a reverb; rise time; "improved Swell triggering" in TimeLine firmware 1.33 | Swell feeding an ambience | Wet only | Already in gfx-delay, gfx-reverb, gfx-shimmer (`swell`) |

### Open source

| Project | What it does | What we take |
|---|---|---|
| [GxSlowGear.lv2](https://github.com/brummer10/GxSlowGear.lv2) (GPL-3) [V], `dsp/slowgear.cc` | 8-sample peak > threshold → gain rises linearly (uptime), else falls (downtime). A **slow gate**: no onset detection, so a note only swells after the level has fallen below the threshold | The counter-example: re-swelling needs silence |
| [slowmo.lv2](https://github.com/brummer10/slowmo.lv2) [V] | An 18-band "slow gate" with echo: one slow gate per band (Faust band-split) | A band-split swell is cheap and has no latency; kept for a later Poly mode (§2.6) |
| golem-fx `swell` (gfx-delay, gfx-shimmer `lib.rs`) [V] | Fast one-pole (k 0.05) above twice a slow one (k 0.0005) restarts the gain from 0; gain² rise | Its law (gain², ms) as the `Square` curve. Its defects are fixed here: no high-pass (low-string ripple and beating re-trigger it), no re-arm or refractory period (each string of a strum restarts it), the gain steps to 0 in one sample, and the coefficients do not scale with the sample rate |
| gfx-killswitch pick detector [V] | High-pass 1.5 kHz ×2, fast (0.2 / 5 ms) − slow (150 ms) envelopes in dB, `sens` → rise 12 … 2 dB, hysteresis re-arm, 40 ms refractory; ≤ 2 ms after the pick | Detector A, extended |
| gfx-autowah follower [V] | dB follower read relative to the playing's long-term level (a 1.5 s one-pole) | The idea of relative floors: the same response at any level. The reference itself is different here (§2.2) |
| gfx-gate [V] | Key input on its own bus, falls back to main; S-curve attack; hysteresis | `source`, fallback, read-only `state` / `gr` |
| gfx-dsp `detect` [V] | YIN pitch detector (2.7 ms hop, 75 Hz – 1.5 kHz, harmonic lock, window = 1 / lowest frequency = 13.3 ms; `set_range` narrows it without allocating) + `NoteTracker` | Detector C |

### Literature

- [Bello et al., *A Tutorial on Onset Detection in Music Signals*, IEEE TSAP 2005](https://www.cs.bu.edu/fac/snyder/cs583/Literature%20and%20Resources/bello_onset_tutorial.pdf) [P]:
  - onset = detection function + peak picking;
  - energy and high-frequency content suit percussive (picked) onsets;
  - spectral flux and phase / complex-domain methods suit soft, pitched onsets;
  - adaptive thresholds (a moving median plus δ) beat fixed ones.
- [Böck & Widmer, *Maximum Filter Vibrato Suppression for Onset Detection* (SuperFlux), DAFx 2013](https://dafx.de/paper-archive/details/0oee-99Z88WL7pSo749gcA) [P]:
  - spectral flux against a **maximum filter over neighbouring frequency bins** of an earlier frame, on a
    log-frequency spectrogram of 24 bands per octave with a filter 3 bins wide (± a quarter tone);
  - up to 60 % fewer false positives on vibrato-heavy music, with no extra misses;
  - causal, so usable online;
  - detector B borrows the idea, not the resolution: its bands are ten times wider (§2.3).
- [Collins, *Using a Pitch Detector for Onset Detection*, ISMIR 2005](https://archives.ismir.net/ismir2005/paper/000008.pdf) [P]:
  - for pitched, non-percussive monophonic material (legato), note changes read in a pitch track (a
    constant-Q pitch detector, vibrato suppressed) improve on the best detector of an earlier comparison
    study.
- Time-frequency limit [I]: telling apart a semitone at 82 Hz (5 Hz apart) in the spectrum needs about
  200 ms of signal. No zero-latency filter bank can see a low-string legato semitone quickly. A
  time-domain period detector can: YIN sees a period change once its window (plus one period) holds the
  new note, about 27 ms on a low E. This is why detector C exists.

### What players report (forums, reviews) and our answer

| Complaint | Cause | Answer here |
|---|---|---|
| Sensitivity finicky; quiet notes missed or noise triggers | One absolute threshold | Detection **relative to the playing level** (§2.2), plus an absolute `thresh` that only rejects noise |
| Legato, hammer-ons and slides don't re-swell | Level-only detection | Detectors C (pitch jump) and B (shape flux), under `legato` |
| Re-swells only after muting (Helix, Fractal) | Hysteresis reset only | Onset-driven re-trigger; `reset` remains for the after-silence case |
| A chord strum swells six times, or stutters | No refractory period | `strum` window: one swell per strum |
| Re-picking cuts the note still ringing | Every trigger drops to silence | `dip`: how far a retrigger pulls a ringing note down (100 % = classic, 30 % = a gentle bow change) |
| The attack still clicks through | Detection lag | Gain already at 0 after a stopped note (`reset`), so it is perfect there. A re-pick over a ringing note still leaks about 1 ms of pick (an audible tick, as on every zero-latency slow gear); `lookahead` 3 ms removes it at a reported latency |
| Before or after the drive? | Before: the drive compresses the rise, which sounds like it jumps late. After: detection reads a compressed, noisy signal | Place it **after** the drive / amp with `source` Key reading the clean DI, the gfx-gate way; `curve` dB if it must sit before a drive |
| No tempo sync | — | `sync` (contract list) |
| Fiddly menus | — | Face: four knobs and one selector; everything else in Expert with neutral defaults |
| A soft note never sounds (no transient to detect) | Onset-only triggering | After a reset, the level alone opens the swell (§2.3 L): the worst case is a classic slow gear, never silence |

### What we conclude

1. **Detection decides everything.** Use a level gate after silence and three cheap, causal detectors,
   each for one kind of note change, combined under one arming / strum logic. All levels are relative to
   the playing's level. Precision before recall.
2. **Reset-aware gain.** When a note has been stopped or has died away, the gain is already back at zero,
   so the next pick is removed perfectly at zero latency. A note left ringing does not reset (that would
   cut its sustain): a re-pick over it leaks about 1 ms of pick, and `lookahead` closes that gap for those
   who accept latency.
3. **The swell's shape**: five curves, rise in ms or note values, an optional decay (bow), a start depth,
   and a retrigger dip.
4. **Poly later.** A four-band split is not polyphony (every note's partials cross every band); a true
   per-note swell is a research item next to poly pitch, not a v1 feature.

## 2. Design

### 2.1 Signal path

```
main in ──▶ [lookahead delay: 0 / 3 / 10 / 25 ms] ──▶ × g(t) ──▶ × level ──▶ out
                                                        ▲
key in (opt.) ─┐                                        │
main in ───────┴▶ source ▶ mono sum ─┬▶ L  level: full-band env, phrase level, gate rule ────┐
                                     ├▶ A  pick:   HPF 1.5 kHz ▶ fast / slow dB, slope ──────┤▶ onset logic ▶ gain engine
                                     ├▶ B  shape flux: 16-band bank ▶ dB env ▶ max filter ───┤   (arm, strum,  (curve, rise / sync,
                                     └▶ C  pitch jump: YIN (gfx-dsp detect) ─────────────────┘    thresh)       depth, dip, decay, reset)
```

The detectors read the **undelayed** input, so with lookahead the decision arrives before the audio does.

### 2.2 Levels (L)

- **Note level** `e(t)`: a full-band peak follower (0.2 ms attack, 30 ms release), in dB. gfx-dsp
  `envelope::PeakFollower`.
- **Phrase level** `P(t)`: the note level's peak, held 5 s, then falling 3 dB / s, never below `thresh`.
  It plays the role of gfx-autowah's long-term level, with a peak hold so that a pause does not open the
  window to the noise.
- **Floor** for any onset: `e > max(thresh, P − W)`, with the window W = 18 + 30 · sens dB (18 – 48 dB).
  The same phrase played 24 dB quieter triggers the same notes.
- **Honest limit.** Hiss under the window is rejected while P holds. After a long silence P falls to
  `thresh`, and hiss above `thresh` then opens the swell like a note. The cost is small (the next pick is
  a retrigger instead of a perfect removal), and the remedy is the user's: set `thresh` (Noise Floor) just
  above the rig's hiss.
- **Note body** `N`: the maximum of `e` from 20 ms after the onset (so the pick's spike does not count).
  The reference for `reset` (§2.5).

### 2.3 Onset detectors

**L: level gate (after a reset only).** While the gain engine is Closed (§2.5), the note level crossing
the floor (§2.2) for 1 ms is an onset. This is the classic slow gear's rule, and the safety net: a soft,
thumb-plucked or faded-in note with no transient still swells, and nothing can leave the block silent.
Detection ≈ 0.2 ms; the gain is at its start level anyway.

**A: picks.** gfx-killswitch's detector, sample by sample, with two changes (the slow reference and the
slope test):
- 2nd-order high-pass at 1.5 kHz, then a fast envelope (0.2 / 5 ms) in dB.
- The slow reference is a 150 ms average **of the fast envelope in dB**, not of |x| as in gfx-killswitch.
  On stationary noise a peak follower sits 8 – 10 dB above the mean of |x|, which alone would pass the
  rise test; against its own average the difference stays within ± 3 dB.
- It fires when all of these hold:
  - fast − slow > R, with R = 12 − 9 · sens dB (12 → 3 dB);
  - the fast envelope rose by more than R / 2 in the last 3 ms (**slope test**: a transient, not the slow
    beating of a chord's detuned strings, nor an upstream tremolo);
  - the note level is above the floor (§2.2);
  - the detector is armed.
- It re-arms when fast − slow < R / 2.
- Detection ≈ 0.3 – 1 ms after the pick.

**B: shape flux (new energy without a pick).**
- **Role.** Soft or fingered attacks that A misses, taps, hammer-ons "from nowhere", a new chord over a
  ringing one, pads and keys. B does **not** see small-interval legato on one string (the partials stay in
  their bands): that is C's job.
- **Bank.** 16 band-passes (gfx-dsp `svf`, Q ≈ 3.4), 5 semitones apart from 90 Hz to 7 kHz. Each band's
  level `L_b = max(20 · log10(env_b), P − 60 dB)` is read every hop of 1 ms (one-pole envelopes,
  τ = max(2 periods of the band centre, 2 ms)). Levels are in dB under a floor tied to the phrase level, so
  the detector is the same at any input level.
- **Increase against a maximum filter** (SuperFlux's idea). `d_b(n) = L_b(n) − max(L_{b−1}, L_b,
  L_{b+1})(n − μ)`, with μ = 8 ms. A partial moving into the next band (wide vibrato, a bend, a slide) does
  not count as new. With bands this wide the filter also hides any move smaller than a band, which is why
  B is a detector of new energy and not of pitch changes.
- **Level-rise correction (tremolo-proof).** Let the *ringing* bands be those that were more than 6 dB
  above the floor at n − μ, and r the median of `d_b` over them (r = 0 with fewer than three).
  `D(n) = Σ_b max(0, d_b(n) − max(0, r))`.
  - A tremolo, a volume knob or a compressor raises every ringing band by the same number of dB: D ≈ 0.
  - A decay lowers every band: D = 0. The positive part counts only bands that rise.
  - A new note raises a few bands more than the others, or bands that were silent: D > 0. A band rising
    from the floor is always counted in full.
- **Adaptive threshold** (Bello): fires when `D(n) > δ + 1.5 · median(D over the last 150 ms)`, on a rising
  crossing, with re-arm below half the threshold. δ = 36 − 24 · legato dB, summed over the bands (starting
  values, tuned on the bench).
- Detection: 5 – 10 ms from the partials above 300 Hz, which every guitar note has; the bands under 200 Hz
  answer in 25 – 35 ms and only confirm. Strong hammer-ons often also fire A (the fret strike is a
  transient).

**C: pitch jump (mono legato).**
- Its own gfx-dsp `PitchDetector` (YIN, every 2.7 ms), computed when `legato` > 0.
- **Adaptive window.** The detector's window is one period of its lowest frequency (13.3 ms at 75 Hz), at
  any pitch. C therefore calls `set_range` (no allocation) to follow the note: lowest frequency =
  max(75 Hz, 0.45 × the reference pitch), so the window shrinks to 2.2 periods of the note, and the cost
  with it. A pull-off of more than about 13 semitones, or 30 ms without a voiced frame, reopens the full
  range.
- **Reference pitch.** The last *stable* pitch: an analysis within 25 cents of the one before it, with
  clarity ≥ 0.92 − 0.12 · legato. Anything that glides slower than 25 cents per analysis (9 cents / ms)
  drags the reference along:
  - vibrato of ± 100 cents at 7 Hz moves 12 cents per analysis;
  - a 200-cent bend in 40 ms moves 13.5 cents per analysis.

  Neither can ever stand 70 cents from the reference.
- **Jump.** It fires when a new stable pair of analyses lands ≥ 70 cents from the reference, the reference
  having been stable for at least 30 ms, and the change having taken ≤ 40 ms. The unvoiced or scattered
  analyses in between (the window holds both notes) are ignored.
- **Octave errors.** A jump within 30 cents of an octave, a twelfth or two octaves (YIN's own errors, and
  the pitch of two strings ringing together) must hold for three more analyses.
- **Let-ring guard.** Two strings ringing together give a low clarity or a pitch that flips as one decays
  faster. The clarity bound on both sides of the jump and the 30 ms of stable reference reject them; the
  bench's let-ring test decides the clarity bound.
- **Slides.** A jump in the same direction as the previous one, inside `strum`, continues the swell instead
  of restarting it: a slide swells once. A slide slower than one fret per `strum` restarts at each fret
  (no detector can tell it from a hammer-on run): raise `strum`.
- Detection: window + one period + up to one hop + the confirming analysis ≈ 3.2 periods + 3 – 5 ms. About
  14 ms at 330 Hz, 9 ms at 660 Hz, 30 ms on a low E (where the window stays at 13.3 ms).
- Polyphony: unvoiced or ambiguous frames are ignored; chords are A's and B's job.

### 2.4 Onset logic

- **onset** = (Closed ∧ L) ∨ A ∨ (`legato` > 0 ∧ (B ∨ C)), gated by the floor (§2.2).
- **Strum window** (`strum`, default 60 ms), counted from the first onset and not extended by later ones:
  onsets inside it are absorbed (no dip, no restart; the rise keeps its origin).
  - A strum (6 strings in 10 – 60 ms, either direction) gives **one** swell, timed from its first string.
    The later strings arrive while the gain is still near its start, so their attacks are removed too.
  - It is also the fastest retrigger rate (60 ms = 16 notes / s).
- **Same-note hold-off.** B and C answer 5 – 30 ms after A or L for the same note. Their onsets are
  absorbed for max(`strum`, 40 ms) after any onset, whatever `strum` is set to.
- **Manual `fire`**: an off → on edge is an onset, from a footswitch. It re-swells a held chord.
- **Engage**: the block has its own bypass parameter (`bypass`, nih-plug `make_bypass`, as gfx-killswitch).
  Switching on, like `reset()`, starts Closed: a chord already ringing swells in at once through L, and
  silence stays closed.

### 2.5 Gain engine

States: **Idle** (gain at the start level s₀ = 1 − `depth`) → **Rise** → **Hold** (gain 1) → [**Decay**
when `decay` > 0] → Idle. With `decay` > 0 the Hold has no length: the decay starts where the rise ends.

Idle has two flavours, both reported as `state` 0:
- **Closed**: the note has reset (below). The level gate L is armed.
- **Spent**: a bow's decay has ended while the note still rings. Only A, B, C and `fire` can start a swell
  (otherwise L would bow the same note again and again); it becomes Closed once the reset condition holds.

- **Curve.** The gain follows `g = s₀ + (1 − s₀) · c(t)`, with t from 0 to 1 over the rise time:

  | `curve` | c(t) | At half the rise | Use |
  |---|---|---|---|
  | Square | t² | −12 dB | Default; the law of golem-fx's existing `swell` (delay, reverb, shimmer), so a swell sounds the same everywhere |
  | Linear | t | −6 dB | The VCA of the clones; "fast" swell |
  | S-Curve | (1 − cos πt) / 2 | −6 dB, flat ends | Smooth start and landing (Fractal's swell taper) |
  | dB | (10^(−2.4(1−t)) − 10^(−2.4)) / (1 − 10^(−2.4)) | ≈ −24 dB | Even in loudness over 48 dB, arrives late; before a drive |
  | Pedal | (32^t − 1) / 31: an A-taper pot (15 % at half travel) swept at constant speed | ≈ −16 dB | A volume pedal rocked by the foot |

- **Rise time**: `rise` ms, or with `sync` ≠ Off a note value at the host tempo (gfx-dsp `transport`;
  120 BPM without a host tempo). `sync` sets a **duration** only: the swell starts at the onset, not on
  the grid. A synced rise is not bound by `rise`'s range (4/1 at 60 BPM = 16 s).
- **Retrigger.** Over a ringing note (Rise, Hold, Decay or Spent), an onset pulls the gain down to
  `g_dip = g − dip · (g − s₀)` along a raised cosine, then continues the rise from `g_dip`: the curve's
  parameter restarts at t = c⁻¹(normalised g_dip), so the remaining time is proportional and nothing
  jumps.
  - `dip` 100 % is the classic slow gear;
  - 0 % ignores retriggers, so a note only swells after silence (the Fractal / GxSlowGear behaviour).
- **Dip time**, by the onset's source (zero latency):
  - A: 0.5 ms. The pick's own transient masks the fade.
  - B, C and `fire`: 8 ms. There is no transient to hide a fast fade, and it would click on a sustained
    note.
- **What leaks at zero latency.**
  - Pick over a ringing note: the first 0.8 – 1.5 ms of the pick pass at the old gain, then silence. This
    is the loudest, brightest millisecond of the note, so it is heard as a tick (the SG-1's own blip).
    `dip` < 100 % makes it less exposed; only `lookahead` removes it.
  - Legato (B, C): the new note sounds at the old gain for its detection time (10 – 30 ms), then fades and
    swells: a short "blip, then swell". Only `lookahead` 10 / 25 ms removes it.
- **With lookahead** the audio is late, so the dip can end where the onset is. Each detector reports its
  own lag (A 0.5 ms, B μ = 8 ms, C its window + one period + half a hop). The dip is a raised cosine of
  min(3 ms, the time left) for A and 8 ms for B, C and `fire`, ending at the estimated onset, or as early
  as possible when the lag exceeds the lookahead.
- **Reset.** Once the note level has fallen `reset` dB below the note body N (§2.2), or under `thresh`,
  the gain returns to s₀ over `release` ms (the curve played backwards) and the engine is Closed. The
  tail is already that quiet, so the fade is inaudible. The next pick then meets a closed gain: **perfect
  attack removal at zero latency**.
  - This is the case of stopped or muted notes (the slow-gear technique) and of notes left to die.
  - A note left ringing under the next one does not reset: a clean string loses 30 dB in several seconds,
    and a shallower reset would cut its sustain. That case is a retrigger.
  - `reset` at its maximum (60 dB) leaves only the `thresh` rule.
- **Bow** (`decay` > 0): after the rise, the gain falls back to s₀ over `decay` ms, the same curve
  mirrored. Each note becomes an attack-decay bow; a long rise with a short decay sounds like reversed
  tape (the EHX Attack Decay's purpose).
- **Smoothing.** The gain is computed in f64 and never steps. Parameter changes act from the next onset
  (so an EXP pedal on `rise` or `decay` sets the next swell, with no smoothing needed), except `level` and
  `depth` (EXP smoother, then 20 ms).

### 2.6 Layouts, key, latency

- **One gain for the whole signal.** No band-split mode in v1 (decision 5, §6): four bands are not
  polyphony. A later **Poly** mode would put one swell per band of gfx-octaver's 28-band ERB filterbank
  (zero latency, already written), with onsets per band: slowmo's idea at a useful resolution. It is a
  research item with its own bench, and would arrive as a new parameter.
- **Stereo.** Mono 1 / 1 first: a slow gear sits before the amp or after it on a mono lane, and must not
  make the lane stereo. 2 / 2 offered. Detection runs on the mean of the channels; one gain for both, so
  the image never shifts.
- **Key** (`source` Key): a VST3 auxiliary input on its own bus, as in gfx-gate.
  - Detection reads the clean DI while the swell sits after the drive / amp. A distorted signal has
    almost no transients left, and it amplifies hiss that could trigger.
  - `reset` and `thresh` then also read the key: the DI's decay is the note's, the amp's compressed tail
    is not.
  - Falls back to main when nothing is connected.
- **Latency**: **zero** at default. `lookahead` 3, 10 or 25 ms delays the audio only and is **reported** to
  the host (nih-plug `set_latency_samples`, called only when the value changes).
  - 3 ms hides every pick dip. Playable live.
  - 10 ms also hides B's dips and C's above about 430 Hz.
  - 25 ms hides C's down to about 145 Hz. For recording and re-amping.
  - Changing it re-syncs the host. It is a setup choice, as gfx-limiter's `lookahead`: set at insert or by a
    preset, never automated, never on EXP, never switched inside a song. The audio is crossfaded over 20 ms
    across the change.
- **CPU estimate** (48 kHz, mono, release build):

  | Part | Cost |
  |---|---|
  | A and L | ≈ 15 ns / sample |
  | B (16 SVFs + envelopes; flux per hop) | ≈ 60 ns |
  | C (YIN, adaptive window: full range only on low notes) | ≈ 20 – 80 ns |
  | Gain engine | < 5 ns |
  | Total with all detectors on | ≈ 100 – 160 ns / sample, ≤ 1 % of a core (budget 2 %) |

  With `legato` 0, B and C are not computed: about 20 ns.

### 2.7 What is reused

| From | Taken |
|---|---|
| gfx-killswitch | Detector A (high-pass, fast envelope in dB, `sens` direction, hysteresis re-arm; the slow reference and the slope test are new), the engage-through-bypass restart, the `depth` id and meaning, the read-only counter pattern |
| gfx-autowah | The idea of detection relative to the long-term playing level |
| gfx-gate | `source` Main · Key with fallback, the S-curve attack, read-only `state` and `gr` |
| gfx-delay / gfx-reverb / gfx-shimmer | The `swell` law (ms, gain²) as the default `Square` curve |
| gfx-dsp | `svf` (detector bank, high-pass), `envelope::PeakFollower`, `detect::PitchDetector` (+ `set_range`), `transport` (sync), `delay` (lookahead), `smooth`, `exp_smooth`, `util` |
| New in gfx-dsp | `onset.rs` (L + A + B + C + logic) and `swell.rs` (gain engine). Both are written as shared modules, so gfx-delay, gfx-reverb and gfx-shimmer replace their inline `swell` detector afterwards (decision 1, §6) |

## 3. Parameters

New block: no catalogue ids. Shared ids keep their contract meaning: `sync` (the contract's list), `gr`
and `state` (meters). `sens` and `depth` keep gfx-killswitch's meaning; `source` keeps gfx-gate's. Ranges
are linear; enums only grow at the end.

### Front panel

| id | Name | Type | Range | Default | EXP | Role |
|---|---|---|---|---|---|---|
| `rise` | Rise | f | 5 – 4,000 ms | 300 ms | ✓ | Swell time from start to full level (when `sync` = Off) |
| `sync` | Sync | e | contract's list (Off · 4/1 … 1/32) | Off | | Rise as a note value at the host tempo |
| `sens` | Sensitivity | f | 0 – 100 % | 50 % | | Pick threshold (rise 12 → 3 dB) and the window below the playing level that may trigger (18 → 48 dB) |
| `legato` | Legato | f | 0 – 100 % | 50 % | | Sensitivity of the pickless detectors (shape flux, pitch jump); 0 = picks only (and cheaper) |
| `curve` | Curve | e | Square · Linear · S-Curve · dB · Pedal | Square | | Rise law (§2.5) |
| `depth` | Depth | f | 0 – 100 % | 100 % | ✓ | How far below unity a swell starts: 100 % = from silence, 50 % = attack softened by 6 dB |

### Expert

| id | Name | Type | Range | Default | EXP | Role |
|---|---|---|---|---|---|---|
| `dip` | Retrigger Dip | f | 0 – 100 % | 100 % | | How far a new note pulls a ringing note down before swelling; 0 = swell only after silence |
| `strum` | Strum Window | f | 20 – 300 ms | 60 ms | | Onsets this close to the first one merge into one swell (strums, slides); also the fastest retrigger |
| `decay` | Decay (Bow) | f | 0 – 4,000 ms | 0 (off) | ✓ | After the rise, fall back to the start level: attack-decay bows, reverse-tape |
| `reset` | Reset | f | 6 – 60 dB | 30 dB | | The swell resets (gain back to the start) once the note has decayed this far below its body level; falling under `thresh` always resets |
| `release` | Reset Fade | f | 10 – 2,000 ms | 60 ms | | Fade to the start level on reset |
| `thresh` | Noise Floor | f | −96 – −30 dB | −66 dB | | Absolute floor: nothing below it triggers or holds a note (hiss, hum) |
| `source` | Detector Source | e | Main · Key | Main | | Key = the auxiliary input (the clean DI when the swell sits after the amp) |
| `lookahead` | Lookahead | e | Off · 3 ms · 10 ms · 25 ms | Off | | Delays the audio, reported to the host: perfect removal of retriggered attacks. Setup only (§2.6) |
| `fire` | Fire | b | off / on | off | | Each off → on edge starts a swell (footswitch) |
| `level` | Level | f | −24 – +12 dB | 0 dB | ✓ | Output level |
| `bypass` | Bypass | b | off / on | off | | The VST3 bypass parameter (`make_bypass`): golem switches the block with it, so engaging is seen (§2.4) |

Read-only: `state` (0 Idle · 1 Rise · 2 Hold · 3 Decay), `gr` (current attenuation, 0 – 90 dB),
`onsets_count` (debug witness, as killswitch's `picks_count`).

No default depends on another parameter. `rise` is ignored while `sync` ≠ Off; `release` and `reset` keep
their meaning with `decay` > 0. `reset` has no "0 = off" step (its old 0 = never sat next to 1 = the most
eager): the maximum is the neutral end.

`rise` reaches 5 ms so that, with `depth` < 100 %, the block also works as a transient softener. The UI
needs a log-feeling knob there: the range itself stays linear (contract).

### Visual

- **Category: `dynamics`.**
  - The SG-1 is literally the NF-1 gate reversed, and the swell shapes each note's envelope from the
    signal's own level, as the gate and compressor do. The add-block drawer then shows it next to its
    sibling.
  - `utility` is for static level tools (Volume, Clean Boost); `modulation` implies a periodic source.
- **Finish: anodized, against the dynamics family's thrash.** Thrash's diamond plate and hazard yellow
  say "attack" and "aggression". This pedal removes attacks to make violins and pads. The crate overrides
  its family, as gfx-killswitch (thrash) and gfx-gapmeter (anodized) already do:
  `CRATE_FINISH['gfx-swell'] = 'anodized'`.
- **Crate hue**: `METAL_CRATE_HUE['gfx-swell'] = '#d81e5b'`, a saturated dusk rose (bow rosin, a slow
  sunrise).
  - Hue 340°, in the middle of the only wide gap left on the wheel: pitch's magenta (`#c4148c`, 319°) on
    one side, the drive's thrash red (`#c8160e`, 3°) and orange (`#e04a00`) on the other.
  - Saturation 76 %, as the other metal hues ("no grey mixed into the hue"); the first proposal
    (`#c2365f`, 56 %) would have looked dusty beside them.
  - The dynamics slate (`#4a5a6e`) reads grey once anodized.
  - `RUNE_LIGHT['gfx-swell'] = '#ffd0dc'`: without a crate entry the rune would take the dynamics
    family's blue (`#5fb0ff`).
- **Motif `swells`** (new case in `motifParts`):
  - A baseline at y0 = H − 40, across the full width.
  - On it, four envelopes in slots of width W / 4, each w = W / 4 − 4 wide (a 4 px gap, so a drop never
    lies on the next tick). Each starts with a short pick tick `M x0 y0 V y0−8` (opacity .35: the attack
    that was removed), then, from the baseline again, rises concave-up (the Square curve)
    `M x0 y0 Q x0+.75w y0 x0+.92w yTop` to a plateau at yTop = cy + ch / 2 + 8, `H x0+w`, then drops
    vertically to y0: the next note's dip.
  - The plateaus rise slightly left to right (yTop − 3k) and the opacity goes .45 → 1: a crescendo.
  - All drawn with `line`, so it is engraved on anodized and yellow-on-black on thrash if forced.
- **Face** (`visuals/data/faces.json`):
  `"gfx-swell": {"name": "Swell", "blocks": ["swell"], "knobs": ["rise", "sens", "legato", "depth"], "selectors": ["curve"], "category": "dynamics", "color": "#d81e5b"}`.
- **Glyph** `swell` (24 × 24, single colour, stroke 1.8, round caps; title "Auto swell"):
  ```
  <path d='M2.5 20h19' opacity='.4'/>            baseline
  <path d='M4 20V12.5' opacity='.35'/>           the pick attack, removed
  <path d='M4 20C10 20 11.5 7 16 7h5.5'/>        the fade-in to a held plateau
  ```
  It reads as "a note that arrives late and stays". Distinct from `gate` (square step), `volume`
  (fader) and `tremolo` (periodic).

## 4. Applications

Block presets (generic names, no artists or trademarks):

| Use / preset | Settings to start from | Feature that matters |
|---|---|---|
| **Violin** (clean single notes) | `rise` 250 ms, Square, `sens` 50 %, `legato` 50 %; stop each note before the next | Pick removal at zero latency after each note's reset |
| **Violin after the amp** | Same, `source` Key (clean DI), after the amp / drive | Detection on the clean signal; the drive does not flatten the rise |
| **Cello pad** (chords) | `rise` 700 ms, S-Curve, `strum` 80 ms, into a reverb | One swell per strum |
| **Legato lead** (hammer-ons, slides) | `rise` 120 ms, dB, `legato` 75 %, `dip` 70 % | Detector C (and B); slides swell once |
| **Gentle bow changes** | `rise` 300 ms, `dip` 35 % | The ringing note is only pulled down a little |
| **Reverse tape** | `sync` 1/4, dB, `decay` 40 ms, `depth` 100 % | Bow: long rise, quick release, on the beat |
| **Bowed stabs** | `rise` 80 ms, `decay` 400 ms, S-Curve | Attack-decay envelope per note |
| **Tempo swells** (ambient) | `sync` 1/2, Pedal, then delay 1/4• and a long reverb | Rise locked to the song |
| **Soft attack** (tame picking) | `rise` 25 ms, Linear, `depth` 50 % | Transient softener; nothing else changes |
| **Foot swell** (re-swell a held chord) | `fire` on a momentary footswitch, `legato` 0 %, `sens` 0 % | Manual trigger |
| **Live clean retriggers** | Any preset + `lookahead` 3 ms | Re-picked attacks fully removed for 3 ms of reported latency |
| **Studio perfect swell** (re-amp, recording) | Any preset + `lookahead` 25 ms | Picked and legato retriggers start at the note, at a reported latency |
| **Bowed bass** | `rise` 180 ms, Square, `sens` 60 %, `legato` 40 % | Relative floor; L, A and B at any pitch. C only reads notes above 75 Hz (from the D string's first frets up): below, legato swells through B or not at all (decision 4, §6) |
| **Pads and keys attack removal** | `legato` 70 %, `rise` 400 ms | L after silence, B for new chords with no transient |

Placement advice for golem's AI and its docs:
- **after** drive / amp with `source` Key, **before** delay and reverb;
- or first in the chain with `curve` dB;
- a compressor (gfx-comp) **before** the swell lengthens the bowed note, as the Attack Sustain does; it
  also lifts the hiss, so raise `thresh`;
- presets used live keep `lookahead` Off or 3 ms; golem does not switch a preset that changes `lookahead`
  inside a song.

## 5. Bench suite (on top of gfx-bench's common checks)

Synthetic signals are Karplus-Strong strings, made in the test, with known onsets. The DIs (clean.wav,
lead-clean.wav, funk.wav, crunch.wav) get hand-annotated onset lists stored next to the tests: a task for
Raph or the coding agent.

| Check | Pass |
|---|---|
| Curve | A single note after silence: g(t) matches the curve within ±0.3 dB over 10 – 100 % of the rise, for all five curves; `sync` 1/4 at 120 BPM: rise = 500 ms ± 1 sample |
| Attack removed after silence | Energy of the first 10 ms after the pick ≤ −40 dB relative to the dry (`depth` 100 %, zero latency) |
| **Never silent** | A sine faded in over 50 ms with no transient, `legato` 0: one onset (L), the swell plays. Engage on a ringing chord: it swells in |
| **Retrigger on pick** | Re-picks every 400 ms on a ringing string: one onset each, ≤ 1.5 ms late; leaked attack (first 10 ms) ≤ −12 dB at zero latency (a target: the first bench records the real figure), ≤ −40 dB with `lookahead` 3 ms |
| **Retrigger on legato** | Instantaneous pitch change of the string with no excitation: a semitone at 330 Hz fires within 18 ms, at 110 Hz within 30 ms, at 82 Hz within 35 ms (C); a new three-note chord with no pick over a ringing one fires within 15 ms (B); with `lookahead` 25 ms the dips at 330 and 110 Hz and the chord's end within ± 3 ms of the change; F-measure ≥ 0.85 on annotated legato in lead-clean.wav, with precision ≥ 0.95 |
| **No retrigger on decay** | 8 s decaying note; a 6-string chord with ±3 cent detuned strings beating at 1 – 3 Hz: 0 onsets after the first |
| **No retrigger on vibrato / bends** | ±50 cents at 6 Hz; ±100 cents at 7 Hz; 200-cent bend over 100 ms and back, and over 40 ms; a 12-semitone dive in 300 ms; tremolo of 6 dB at 5 Hz upstream (sine): 0 onsets. Chromatic slide of 5 frets in 150 ms: 1 onset |
| **No retrigger on let-ring** | Two and three strings ringing together (fifth, fourth, third, octave) and decaying at different rates; an arpeggio left ringing, each note picked: 0 onsets from C, one per pick from A |
| **One swell per strum** | 6 strings 12 ms apart (down) and 6 ms apart (up): 1 onset each; two strums 250 ms apart: 2. `strum` 20 ms, a picked note change on a low E: still 1 onset (same-note hold-off) |
| Noise and level independence | Pink noise at −50 and −20 dBFS, 50 Hz hum with harmonics at −50 dBFS, `sens` and `legato` ≤ 75 %: 1 onset (L, at the start), then 0; hiss 45 dB under a phrase with 3 s pauses: 0 onsets in the pauses; the same phrase at 0, −12 and −24 dB: identical onsets ± 1 ms |
| Picks on real DIs | F-measure ≥ 0.95 (±5 ms) on annotated funk.wav and clean.wav; crunch.wav with `source` Key = clean DI ≥ 0.95 |
| **Reset** | A note stopped by hand (level −40 dB in 60 ms): Closed within 150 ms, so a pick 200 ms later is fully removed. A note left ringing 4 s on clean.wav: no reset, no audible cut |
| **No clicks** | The gain never moves by more than its raised cosine allows (per-sample step bound per segment). The 8 ms dip (B, C, `fire`) on a sustained 1 kHz sine: HF burst within gfx-bench's click bound (−50 dB). The 0.5 ms dip of A is measured, not bounded: it is a deliberate fast fade under a pick. Reset fade and bow decay are smooth (no step > −80 dB) |
| Bow | `decay` 400 ms on an 8 s note: one attack-decay, then silence (Spent): 0 further onsets until a pick |
| Latency | Default: 0 (common check); `lookahead` 3 / 10 / 25 ms: reported = measured ± 1 sample; a change of `lookahead` crossfades without a click |
| Key | `source` Key, key unconnected: behaves as Main |
| Determinism, CPU | Strict null; ≤ 2 % of one core at 48 kHz (target 1 %) with every detector on |

**gfx-bench options**:
- Default bounds.
- `param_steps` must skip `lookahead` (it changes the reported latency) and `fire` (a trigger, not a level
  change).
- The common checks feed steady signals (pink noise, a 1 kHz sine) from a reset. They only see audio
  because L opens the swell at the start, the level never resets, and steady noise never retriggers: the
  second half of each render is at gain 1. So the common latency, null and parameter-step checks double
  as a noise-immunity test. No pre-roll or `depth` override is needed; if one of them reads silence, a
  detector is wrong, not the bench.

**Listening**: violin, cello pad, legato lead and reverse tape on clean.wav and lead-clean.wav, with the
swell placed before and after a NAM amp. Compared with the gfx-delay `swell` (same law) and a manual
volume-pedal swell recorded by Raph.

## 6. Open questions for review

All settled at the review of 2026-11-14, except what needs ears or golem-rack (listed at the end of the
Review section).

1. **Share with gfx-delay, gfx-reverb, gfx-shimmer?** **Yes, afterwards.** Once gfx-swell's bench is green,
   their `swell` takes gfx-dsp `onset` (L + A, strum window) and `swell` (Square, `dip` 100 %, 0.5 ms dip).
   The id and its ms meaning stay. The change is accepted: the inline detector depends on the sample rate
   (a bug, a 96 kHz session does not swell like a 48 kHz one), steps the gain to 0 in one sample, and
   restarts on every string. The presets that use `swell` are listened to again then.
2. **Legato dip audibility.** **A slower dip (8 ms) for B, C and `fire`**, at the depth the user set. No
   automatic halving of `dip`: a hidden rule on a user parameter. The "blip, then swell" at zero latency is
   stated in §2.5, and `lookahead` 10 / 25 ms is the answer for those who accept latency.
3. **Lookahead values.** **An enum, Off · 3 ms · 10 ms · 25 ms**, reported, setup only. This is
   gfx-limiter's `lookahead` (Off · 1 · 2 · 5 ms), already shipped; gfx-flanger's case was different (a
   continuous knob that moved the latency on every turn). 25 ms was added because C answers in 14 – 30 ms:
   10 ms cannot place a legato dip under about 430 Hz.
4. **YIN range on bass.** **Picks, level and flux only below 75 Hz in v1.** A 30 Hz range costs six times
   the CPU and answers in about 70 ms, too late to place a dip. The lowest octave and a half of a bass
   (B0 to D2) therefore has no pitch-jump detection. If the bass bench asks for it, a `range` parameter
   (Guitar · Bass) is appended later; no hidden `init` value.
5. **Bands mode value.** **Dropped from v1**, and `mode` with it. Every note's partials cross all four
   bands: a high note over a ringing low E would dip the E's upper partials and swell them back, an
   audible filter sweep on the bass note, not a polyphonic swell. Detector A lives above 1.5 kHz, so three
   of the four bands had no pick detector. The real thing is a per-band swell on gfx-octaver's ERB bank
   (§2.6), as research.
6. **Default curve.** **Square.** It sits between Linear (the start is heard as an attack) and dB (arrives
   late), and matches the `swell` of delay, reverb and shimmer. Presets carry their own curve, so the
   listening decides preset by preset, not the default.
7. **Reset default.** **30 dB below the note body** (the body excludes the pick's spike, which the old
   "36 dB below the peak" included), with a 60 ms fade. `reset` cannot serve notes left ringing without
   cutting them; it serves stopped notes, and for those the fade should be short so the gain is closed
   before the next pick. The bench's Reset check holds both sides.
8. **Visual.** **Rose, anodized, `swells` motif: approved**, with the hue saturated to `#d81e5b` and two
   fixes to the motif's path (§3 Visual).
9. **Catalogue entry.** **Type `swell`, display name "Auto Swell", category `dynamics`.** The block type
   and the `swell` parameter of delay / reverb / shimmer live in different namespaces. To be confirmed by
   golem-rack once the code exists (glyph, faces entry, descriptors, block presets).

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good.** Strong on what makes a slow gear good for most playing: pick detection relative to the
playing, one swell per strum, the reset-aware gain, the curves, the key input. **At risk until benched**:
pickless detection (B and C) at zero latency. It is late by nature (10 – 30 ms), and a false onset is
worse than a miss; the design now says so and the bench tests the negatives first. No hardware or plugin
surveyed attempts legato re-swelling at all, so even a careful C alone would put this block ahead.

### Changes made, and why

| # | Change | Why |
|---|---|---|
| 1 | **Level gate L** added: after a reset, the level alone starts a swell. Idle split into Closed / Spent | With onset = A ∨ B ∨ C only, a soft note with no transient (thumb, neck pickup, a pad) and `legato` 0 never opened the gain: silence. Spent stops L from bowing a ringing note again and again after a decay |
| 2 | **A's slow reference** is the 150 ms average of the fast envelope in dB | Against the mean of \|x\| (gfx-killswitch), a peak follower on stationary noise already stands 8 – 10 dB up: over R at the default. killswitch hides this behind an absolute floor; the swell's floor is relative |
| 3 | **B rewritten**: levels in dB over a floor tied to the phrase level; the level-rise correction is the median over the ringing bands; bands rising from the floor count in full | `log(1 + γ · env)` is not level-independent and is not a log at low levels, so a tremolo did not raise all bands "together" and the tremolo-proof claim failed. The mean was pulled by the new note itself. A note after silence looked like a volume knob |
| 4 | **B's role stated**: new energy, not pitch changes. Band spacing corrected | SuperFlux filters ± a quarter tone (24 bands / octave, 3 bins); here the bands are 5 semitones wide, so the maximum filter also hides every small-interval legato. 16 bands half an octave apart from 90 Hz would reach 16 kHz, not 7 kHz: the spacing is 0.42 octave |
| 5 | **C rewritten**: adaptive window through `set_range`; reference = last stable pitch (no smoother); 30 ms of stable reference; integer-ratio jumps confirmed; let-ring guard; honest timings | gfx-dsp's window is 13.3 ms at any pitch (1 / 75 Hz), so "two periods + a hop, 9 ms at 330 Hz" was really about 19 ms. The "70 cents from a 25 ms-smoothed reference" test would fire on a fast bend; the stable-pair rule cannot. Two ringing strings make YIN flip pitch |
| 6 | **Strum window** specified (fixed from the first onset, absorbs, does not extend); **same-note hold-off** of max(`strum`, 40 ms) for B and C; `strum` minimum 10 → 20 ms | "Extend" was undefined. With `strum` under C's lag, a picked note fired A, then C 27 ms later: two dips for one note |
| 7 | **Dip time by source** (0.5 ms for A, 8 ms for B / C / `fire`); the zero-latency leak described as an audible tick; the lookahead dip placed from each detector's own lag | A 0.5 ms full-depth fade with no pick over it is a click. "Leaks about 1 ms" was right in duration but read as negligible: it is the loudest millisecond of the note, alone before silence |
| 8 | **Reset** measured from the note body, default 36 → 30 dB, range 0 – 60 → 6 – 60 dB (no "0 = never" next to 1 = most eager), fade 200 → 60 ms; "the common case in slow playing" corrected | The peak included the pick spike (10 – 20 dB). A ringing clean note takes seconds to lose 30 dB: the reset serves stopped notes only, and for them a short fade closes the gain before the next pick |
| 9 | **Phrase level** holds 5 s then falls 3 dB / s; the hiss claim replaced by its limit | Falling 6 dB / s at once, the window reached hiss 50 dB down after 3 s of silence: "never" was false |
| 10 | **Bands mode and `mode` removed** from v1 | Decision 5. Also: gfx-dsp has no Linkwitz-Riley crossover module (the flanger's split is inline), and a 4-way LR sum is only flat with compensating all-passes |
| 11 | **`lookahead`** Off · 3 · 10 · 25 ms, setup only, crossfaded on change; presets and golem rules stated | Decision 3; the 10 ms claims for legato did not hold with C's real lag |
| 12 | **Engage** = Closed + L: a ringing chord swells in. `bypass` listed in the table | "Starts from Idle" muted a ringing chord until the next pick. The parameter existed in the text only |
| 13 | `sync` stated as a duration, not a phase; synced rise not bound by `rise`'s range; EXP on `rise` / `decay` latched at the onset; key also feeds `reset` / `thresh` | Contract's `sync` speaks of phase; ambiguity for the coder. After an amp the main signal's tail is compressed, so the reset must read the DI |
| 14 | **Bench**: new checks Never silent, No retrigger on let-ring, Reset, Bow; harder vibrato / bend / noise cases; legato timings 10 / 25 / 30 → 18 / 30 / 35 ms; precision bound; the click check restated; the common-check note corrected | The old noise test sat exactly at `thresh`. The −50 dB click bound on a 0.5 ms dip tested the wrong thing. The latency check does not need a pre-roll: steady noise opens through L and must then stay open |
| 15 | **Visual**: hue `#c2365f` → `#d81e5b`; RUNE_LIGHT's reason corrected; motif path fixed (missing `M` before the `Q`, 4 px gap) | Saturation 56 % against the family's rule; without the entry the rune is the dynamics blue, not a dull rose |
| 16 | Facts: gfx-autowah's reference is a 1.5 s one-pole, not a held peak; Collins's result reworded; SuperFlux's resolution added; Pedal curve given a formula; bass range (C starts at 75 Hz, above the open E, A and D strings); CPU table updated | Errors or gaps against the code and the papers |

Checked and left as written: the SG-1 / Fractal / Helix / Attack Decay / Sneak Attack survey; Bello's and
Böck & Widmer's claims (60 % fewer false positives on vibrato-heavy music, causal); the 200 ms
time-frequency limit; the five curves and their half-rise levels; retrigger continuation through c⁻¹;
`depth`, `dip`, `decay`, `fire`, `thresh`, `source`; mono first with the key on its own bus; linear ranges;
no mode-dependent default.

Coverage against the references: SG-1 (Sens, Attack), Helix (Attack, Decay, Threshold, reset offset),
Fractal (Threshold, Attack, Release, Hysteresis, tapers), Attack Decay (attack, decay, curves, blend =
`depth`) are all covered. Left out on purpose: a curve per segment (the decay mirrors the rise), a hold
time before the decay, the Attack Decay's fuzz and loop, the Pico Swello's filter (other blocks do them).

### Still open for the owner

- **Listening** (cannot be settled on paper): B's δ and C's clarity bound against the negative tests;
  whether the zero-latency legato "blip" is acceptable at the default `legato` 50 %, or whether the default
  should fall to 0 with legato as an opt-in; the `reset` default on clean.wav; the pick leak figure.
- **Onset annotations** for clean.wav, lead-clean.wav, funk.wav and crunch.wav (Raph or the coding agent),
  and a let-ring and a bass DI.
- **golem-rack**: the catalogue entry (decision 9); key routing for a block after the amp; confirmation
  that a preset-level latency change (`lookahead` here, as in gfx-limiter and gfx-glitch `align`) re-syncs
  cleanly on both engines.
- **Follow-ups**: the shared `swell` of delay / reverb / shimmer (decision 1); a Poly mode on the ERB bank
  and a bass range for C, both only if the bench asks.
