# gfx-transient

> **Status: DESIGNED** (2026-11-14), **reviewed** the same day (Fable 5.2, see the Review section). Not
> coded. A transient shaper with no threshold: attack and sustain are set in dB, and the response is the same at
> any playing level. Its detector is a pair of differential envelopes, SPL's principle, computed as dB
> differences. The fast envelope is a sliding maximum over one period of the lowest note (sized an octave
> under the sidechain low cut), so a steady or decaying low note leaves the attack gain still: no envelope
> ripple and no distortion on bass. A noise floor keeps the
> sustain boost from lifting hiss and room. Options: attack on the highs or the lows only (a first-order
> shelf, zero latency, no crossover), a key input, stereo link, a parallel mix, and an output ceiling
> (soft clip or gfx-limiter's zero-latency engine). Zero latency by default; an optional lookahead (reported) lets the
> gain move before the transient, so the attack is boosted or cut without bending its first cycle.

golem's **transient shaper** for guitar, bass, drums and voice. A new golem block: there is no transient
block in golem's catalogue today. Proposed entry: type **`transient`**, display name **"Transient
Shaper"**, category **`dynamics`** (to be confirmed by golem-rack, Open question 8).

**Where it sits next to its siblings.**
- gfx-comp reacts to a **level** against a threshold. gfx-transient reacts to a **change of level**, at
  any level.
- gfx-swell **removes** attacks. It decides note by note, with onset detectors and a reset-aware state
  machine, and swells from silence over 5 ms – 4 s.
- gfx-transient **shapes** attacks continuously, both ways, from −24 dB to +24 dB. It makes no decisions,
  so there are no false triggers and no retrigger dips. It also shapes the tail (sustain).
- Rule of thumb for golem's AI: to make an attack **disappear** (violin, pads), use gfx-swell. To make it
  **softer, harder or shorter**, or to lengthen or shorten a tail, use gfx-transient. gfx-swell's "Soft
  attack" preset (`rise` 25 ms, `depth` 50 %) overlaps with `attack` −9 dB here; Open question 9.

## 1. State of the art

Survey of 2026-11-14. **[M]** manufacturer page or manual · **[R]** published review · **[F]** forum ·
**[V]** source code read · **[P]** paper · **[I]** inference · **[U]** not verified · **[A]** own calculation.

### Commercial references

| Reference | What it does | Strength | Weakness | What we take |
|---|---|---|---|---|
| **SPL Transient Designer** (1998 → TD4, TDx, Analog Code plug-in) [M] ([UA manual](https://help.uaudio.com/hc/en-us/articles/33538335681428-SPL-Transient-Designer-Manual), [9842 manual](https://images.static-thomann.de/pics/atg/atgdata/document/manual/130203_spl_9842bed.pdf)) | "Differential Envelope Technology". Attack section: an envelope that follows the input and one with a slower attack; the VCA's control voltage is their difference. Sustain section: the same with different releases. Attack ±15 dB, sustain ±24 dB. **No threshold**: "identical envelope processing from quiet to loud signals" | The reference. Two knobs, level-independent | Fixed time constants. A sustain boost lifts room and noise along with the note [I]. The patent is often cited but its number could not be found [U] | The differential principle, the ±dB ranges, the absence of a threshold |
| [Kilohearts Transient Shaper](https://kilohearts.com/products/transient_shaper) [M, R] | Attack, **Pump** (a dip after the transient), Sustain, Speed, Clip (output clipped at 0 dBFS) | Pump, an output clip | No mix: "the only thing I really miss" ([reasontalk](https://reasontalk.com/?p=769)) [R] | Clip stage, `mix` |
| [NI Transient Master](https://native-instruments.com/ni-tech-manuals/transient-master-manual/en/overview) [M] | Attack, Sustain, **Smooth** ("slightly smoother" attack shaping, which reduces distortion on heavily distorted guitars), **Limit** (a hard limiter at the output for loud boosted attacks) | Guitar-aware; the limit | Smooth is a switch [M]. A community thread on volume behaviour ([NI forum](https://community.native-instruments.com/discussion/35258/transient-master-volume-behaviour)), not read [U] | `smooth` as a continuous time; `ceil_mode` Limit |
| Sonnox Oxford TransMod [M, R] | Transient detection with a rise time and sensitivity; uses **lookahead** "so processed transients remain clear and sharp, whether boosted or reduced" ([SOS](https://www.soundonsound.com/node/4910537)) [R]. Successor: Envolution | Clean boosts and cuts thanks to lookahead | Latency | Optional lookahead, reported |
| [Waves Trans-X](https://www.manualslib.com/manual/188607/Waves-Trans-X.html) [M] | Wide and Multi (4 bands, phase-compensated crossovers, range and sensitivity per band). Sensitivity −10 … +10, Duration 0.01 – 500 ms, Range +18 / −24 dB, Release 0.5 – 500 ms | Multiband: attack per band | Four bands are a mastering tool; crossovers cost phase and CPU | Attack on one band, done as a shelf (§2.5) |
| [Softube Transient Shaper](https://softube.com/user-manuals/transient-shaper) [M] | Punch and Sustain, each switchable Hi / Wide / Lo around one crossover (100 Hz – 4 kHz); Punch Fast / Slow; Clip: "turn on or off output distortion" | A band per section | Clip as distortion | `att_band` / `att_freq`. A band for the sustain is not taken in v1 (Review) |
| [Eventide Physion Mk II](https://www.fullcompass.com/common/files/32104-EventidePhysionUserGuide.pdf) [M, R] | **Structural Split**: rapidly changing parts → Transient, the rest → Tonal; each goes to its own effect. Focus, Smoothing, Trans Decay | Process the click and the body differently (reverb on the tonal part only…) | Latency "necessary for the tonal/transient split" (Eventide staff) [F]; "heavy latency" ([MusicRadar](https://musicradar.com/reviews/eventide-physion-mkii)) [R]. The figure was not found [U] | Not a split mode (§2.8). Its cheapest use, hearing the transient part alone, becomes `delta` |

### Open source

| Project | What it does | What we take |
|---|---|---|
| **mda Transient** (Paul Kellett; GPL / MIT ports, e.g. [mod-audio/mda-lv2](https://github.com/mod-audio/mda-lv2) `src/mdaTransient.cpp`) [V] | Four one-pole followers on \|L + R\|: e1 / e2 (attack pair, fast and slow attack, shared release `Att Hold`), e3 / e4 (release pair, shared attack `Rel Hold`, different releases). `g = e1 − e2 + e3 − e4` in **linear amplitude**, output `a + e·g`, where e is the input through a one-pole tilt (`Filter`) | **Counter-example**: g is a difference of amplitudes, so the added term scales with level² and the effect doubles with every +6 dB [A]. Not level-independent. The `Filter` (the shaped part is tilted) is a good idea, which we take as `att_band` |
| [Airwindows Point](https://www.airwindows.com/point) (MIT) [V] `PointProc.cpp` | Gain = the **ratio** of a fast to a slow one-pole average of \|x\| (the "nib / nob" factor), with input trim | The ratio makes it level-independent. But there is no floor: in silence the ratio is of tiny numbers, and the author warns it can "blow up" on transport restart [M]. A one-pole average of \|x\| ripples on low notes [I] | A ratio, i.e. a dB difference, is the right quantity. Add a floor and a ripple-free fast envelope |
| [Envolvigo](https://github.com/johannes-mueller/envolvigo) (Rust LV2, GPL-2) [M] | SPL-style: a fast / slow pair for the attack and a pair for the sustain; boost / cut per part; a "smooth" knob per pair; dry / wet | Same family as ours; confirms the structure. Its gain law is not documented [U] |
| gfx-gate [V] | Close detector = **sliding maximum over one period of the lowest content**: window = clamp(1.2 / max(`hpf`, 30 Hz), 8 ms, 40 ms). gfx-dsp `envelope::SlidingMax` | The fast envelope here. The window rule is **halved** (§2.2): a gate's hysteresis hides what is left under a 12 dB / octave corner, a shaper's gain does not |
| gfx-limiter [V] | Zero-latency limiter: an envelope plus an ADAA soft clip on the residual (bit-transparent below the threshold); lookahead Off · 1 · 2 · 5 ms (Signalsmith moving minimum + box filters, no overs). golem's safety limiter, last on every route to the card | The output ceiling (its zero-latency modes), and the lookahead list and hold / box-filter engine for the attack gain |
| gfx-swell (designed, reviewed) | Phrase level (note level held 5 s, then −3 dB / s); `thresh` "Noise Floor"; `source` Main · Key; reported lookahead, setup only | The relative floor, `thresh`, the setup-only rule |
| gfx-autowah [V] | Envelope read relative to a 1.5 s long-term level | The idea of relative detection; here it comes for free from dB differences |
| gfx-comp [V] | `sc_hpf`, `link`, `mix`; static curve kept the same across characters | Ids and their meanings |

### Literature

- No paper standardises transient shaping; it lives in manuals and code. The useful theory is next to it:
  - Giannoulis, Massberg & Reiss, JAES 2012 (cited by gfx-comp): level detection and smoothing in dB.
  - [Bello et al., *A Tutorial on Onset Detection*, IEEE TSAP 2005](https://www.cs.bu.edu/fac/snyder/cs583/Literature%20and%20Resources/bello_onset_tutorial.pdf)
    [P] (read for gfx-swell): energy envelopes find percussive onsets.
  - Fitzgerald, *Harmonic/Percussive Separation using Median Filtering*, DAFx 2010 [I, not re-read]: the
    standard transient / tonal split. It works on an STFT, which means tens of ms of latency. That is
    Physion's latency, and the reason for §2.8.
- Our time-frequency argument [A]: a gain that changes within one period of a note modulates that note.
  At 41 Hz (bass E) a period is 24 ms. A full-band attack shorter than about two periods therefore cannot
  avoid colouring the fundamental; it can only move the boost away from it, which is what `att_band` Highs
  does.

### What users report, and our answer

| Complaint | Cause | Answer here |
|---|---|---|
| Distortion or "flutter" on bass and low notes [R] (Softube's Clip, [Computer Music](https://www.pressreader.com/australia/computer-music/20170419/282084866664293) on multiband bass shaping) | Followers fast enough for transients ripple at the note's period, so the gain follows the waveform; or the output clips | Fast envelope = one-period sliding maximum (a steady note gives exactly no gain change), its window sized an octave under `sc_hpf`; a 1.5 dB dead zone; `att_band` Highs; `ceil_mode` Off by default |
| Boosted attacks clip the output | +15 dB on a peak | `ceil_mode`: Soft (ADAA) or Limit (gfx-limiter), with `ceiling` |
| Sustain boost brings up room, hiss, amp noise [I] | The sustain gain keeps rising as the signal falls into the noise | **The boost can never exceed the signal's distance to the floor** (§2.4): noise at the floor gets 0 dB |
| Does little, or odd things, on material without clear transients ([Noel Borthwick](https://noelborthwick.com/cakewalk/?p=19322)) [R] | Fixed detector scale | `sens` sets how big a level jump counts as a full transient |
| No dry / wet [R] | — | `mix` |
| Latency (Physion, lookahead shapers) [R] | Lookahead or STFT | Zero latency by default; lookahead 1 / 2 / 5 ms optional, reported |
| Works differently at every input level (gain-staging dependent) [I] (mda, compressor-based "transient" tricks) | Linear differences or thresholds | dB differences: the same shaping at any level above the floor |

### What we conclude

1. **Differential envelopes, computed in dB.** A ratio of envelopes is level-independent (Point), a
   difference of amplitudes is not (mda). SPL's two pairs (attack, sustain) are the right structure.
2. **The fast envelope must not ripple.** A one-period sliding maximum, the gfx-gate fix, gives no gain
   change on a steady note of any pitch above the window's lowest frequency. That is the low-note
   distortion problem solved at its cause, not by smoothing the gain. The window must cover what the
   sidechain filter lets through, not its corner: one period of `sc_hpf` / 2 (§2.2).
3. **A floor bounds the sustain boost.** No surveyed design says how it keeps hiss down; ours makes it a
   law (§2.4).
4. **Lookahead is a quality option, not a necessity.** At zero latency the gain rises with the transient,
   so the first cycle is reshaped. With 1 – 5 ms of lookahead the gain arrives first: TransMod's
   "clear and sharp" boosts and cuts.
5. **Attack on one band, as a shelf, not a crossover.** One first-order shelf, multiplied by the attack
   gain, puts the boost on the pick's highs and leaves the bass fundamental alone. It is transparent at
   0 dB, has no crossover sum to null, and has no latency.
6. **No transient / tonal split.** It needs an STFT and latency (§2.8).

## 2. Design

### 2.1 Signal path

```
main in ─┬─▶ [lookahead delay L: 0 / 1 / 2 / 5 ms] ─▶ attack stage (full / shelf) ─▶ × g_sus ─▶ × level ─▶ mix ─▶ ceiling ─▶ out
         │                                              ▲ G_att                     ▲ G_sus           ▲ dry (same delay L)
key in ─┐│                                              │                           │
        └┴▶ source ▶ sc_hpf / sc_lpf ▶ |x| ▶ link ▶ sliding max (W) ─┬▶ attack pair:  E_f , E_s(att_time) ─▶ D_a ─▶ law ─▶ smooth / lookahead ramp
                                                                    └▶ sustain pair: E_rf, E_rs(sus_time) ─▶ D_s ─▶ law ─▶ floor bound
                                         phrase level P ─▶ floor F = max(thresh, P − 48 dB)
```

The detector reads the **undelayed** signal. With lookahead, the gain is computed L ahead of the audio
it is applied to.

### 2.2 Detector

All on the sidechain signal (§2.7 for stereo), after `sc_hpf` (Butterworth 2nd order, 0 = off) and
`sc_lpf` (Butterworth 2nd order, ≥ 20 kHz = off). gfx-dsp `svf`.

- **Peak level** `p(t)`: `envelope::SlidingMax` of \|x\| over
  **W = clamp(1.2 / max(`sc_hpf` / 2, 30 Hz), 8 ms, 40 ms)**: 40 ms up to `sc_hpf` 60 Hz, 30 ms at 80,
  20 ms at 120, 15 ms at 160, 12 ms at 200, 8 ms from 300 Hz.
  - **Why `sc_hpf` / 2, not gfx-gate's `sc_hpf`** [P]. A 12 dB / octave high-pass leaves a note under its
    corner only 7 – 10 dB down, with its period intact. With the gate's rule, a decaying low E (82 Hz)
    under `sc_hpf` 120 (W = 10 ms, period 12 ms) made p dip and jump once per period: D_a came back at
    every cycle, and G_att moved by 9.7 dB peak to peak at 82 Hz for `attack` ±24 dB (6 – 15 dB over the
    cases tried): the very flutter this detector exists to remove. With W sized for `sc_hpf` / 2, the same notes give **0.000 dB**
    (41 Hz under `sc_hpf` 80; 62, 73 and 82 Hz under 120; 82 Hz under 160).
  - The rule for the player and for golem's AI: **keep `sc_hpf` at or under twice the lowest note** (160 Hz
    for a guitar in E, 120 Hz down to a low B, 80 Hz for a bass in E, 60 Hz for a 5-string). Under that
    limit the attack gain ripples again.
  - It rises with the signal, sample by sample: an attack is seen at once.
  - On any periodic signal whose period is shorter than W it is constant; on a decaying note it steps down
    once per period by that period's decay (0.1 dB on a guitar note) [A].
  - Its cost is a hold of up to W after a peak, on top of the note's own tail. **A softer hit that does
    not rise above what the detector still holds is not seen** [P] (snare decaying at 200 dB / s, share of
    the first hit's attack gain given to the second):

    | Second hit | W | 50 ms later | 80 ms | 125 ms | 250 ms |
    |---|---|---|---|---|---|
    | −6 dB | 40 ms (`sc_hpf` ≤ 60) | 0 % | 10 % | 42 % | 86 % |
    | −6 dB | 12 ms (`sc_hpf` 200) | 0 % | 23 % | 57 % | 92 % |
    | −12 dB | 40 ms | 0 % | 0 % | 10 % | 79 % |
    | −12 dB | 12 ms | 0 % | 0 % | 37 % | 85 % |
    | −18 dB | 40 ms | 0 % | 0 % | 0 % | 69 % |
    | −18 dB | 12 ms | 0 % | 0 % | 11 % | 73 % |

    This is mostly the first hit's tail, not W: every broadband envelope shaper has it (SPL's too). What it
    means: an attack boost favours accents over ghost notes (it widens a drum part's dynamics), and a flam's
    second stroke is shaped only when it is the louder one (grace note first: both are seen [P]). For drums,
    `sc_hpf` 120 – 200 shortens the hold; a per-drum track does not have the problem.
- **Floor** `F = max(thresh, P − 48 dB)`. `thresh` is absolute (dBFS); P is the phrase level of gfx-swell
  §2.2 (p's peak held 5 s, then falling 3 dB / s, never below `thresh`). The states of the four envelopes
  are clamped to F (`E = max(E, F)` each sample), so nothing below the floor is ever shaped and a jump out
  of silence measures at most 48 dB.
- **Attack pair** (linear amplitude, f64):
  - `E_f = max(p, F)`, the fast envelope.
  - `E_s`: one-pole **attack** toward E_f with time constant `att_time`, instant release (E_s = E_f
    whenever E_f falls below it).
  - `D_a = 20 · log10(E_f / E_s)` ≥ 0 dB. Zero on steady or falling signals; it jumps at an onset, then
    decays as E_s catches up.
- **Sustain pair** (linear, f64):
  - `E_rf`: p through a one-pole release of 10 ms (instant attack), clamped to F. It follows the decay.
  - `E_rs`: p through a one-pole **release** with time constant `sus_time` (instant attack), clamped to
    F. Left alone it decays at 8.69 / `sus_time` dB / s, the **reference rate**: 29 dB / s at 300 ms,
    4.3 dB / s at 2 s [A].
  - `D_s = 20 · log10(E_rs / E_rf)` ≥ 0 dB. Zero at the onset (both rise together). On a note decaying at
    r times the reference rate it settles at −20 · log10(1 − r) for r < 1 (1.5 dB at r = 0.16, 6 dB at
    0.5, 12 dB at 0.75) and grows without limit, at the difference of the two rates, for r ≥ 1 [A, P].
- **Level independence.** D_a and D_s are ratios. Scaling the input by any gain scales E_f, E_s, E_rf,
  E_rs and P alike, so the shaping is identical as long as the signal stays above `thresh` (prototype:
  G(t) within 0.06 dB from 0 to −36 dB with `thresh` −96 [P]). `thresh` is the only absolute level in the
  block: near it, a quiet tail gets less sustain boost (§2.4), which is its purpose.

Why linear one-poles under the logarithm, not one-poles in dB: from silence, a dB-domain slow envelope
would make the attack's length depend on how far below the note the silence was (56 dB of gap would
boost for much longer than 12 dB). In amplitude, E_s reaches 63 % of the note in `att_time` whatever the
gap, so `att_time` means the same thing after silence as on a re-pick [A]: the table in §2.3.

### 2.3 Gain laws

One law for both sections. `A` = `attack` (dB), `S` = `sustain` (dB), `D_ref` = 22 − 20 · `sens` dB
(22 dB at 0 %, 12 dB at the default 50 %, 2 dB at 100 %), and a fixed dead zone `D_0` = 1.5 dB:

```
share(D) = 1 − exp(−max(0, D − D_0) / D_ref)                 0 … 1
G_att = A · share(D_a)
G_sus = S · share(D_s)                                      (then bounded, §2.4)
```

- **0 dB is exactly neutral.** With `attack` = `sustain` = 0 the gain is 1 and the path is bit-transparent
  (§2.9). Positive values boost, negative values cut, by up to the knob's value.
- **Saturating, not linear.** A full transient (D ≫ D_ref) gets the whole `attack`; a smaller one gets
  less: a re-pick rising 3 / 6 / 12 / 24 dB over a ringing note gets 11 / 30 / 57 / 82 % at the default
  `sens`, 51 / 88 / 99 / 100 % at `sens` 100 % [P]. A note out of silence always gets (nearly) the whole
  `attack`, whatever its level: that is level independence. A linear "dB per dB" law (an upward expander)
  would let a pick from silence (D ≈ 48 dB) run away.
- **Why a dead zone** [P]. The sliding maximum of anything noisy creeps: on steady white noise D_a reaches
  1.2 dB (99th percentile), on a ringing six-string chord 0.5 dB, on two strings beating at 1.5 Hz 2 dB.
  Without D_0 these gave random gain bursts of 13 % of `attack` at the default `sens` and 52 % at `sens`
  100 % (noise), and D_s made the sustain gain wander by 16 % and 65 %. With D_0 = 1.5 dB: 1.5 % and
  8.5 % for the attack on noise, nothing on the chord. A rise of less than 1.5 dB is not an attack. The
  law stays continuous (a corner in the slope only, and the gain is smoothed, §2.6), so this is not a
  yes / no detector.
- **Attack length.** After an onset from silence, at the default `sens` [A]:

  | t / `att_time` | 0.1 | 0.29 | 0.5 | 1 | 1.5 | 2 |
  |---|---|---|---|---|---|---|
  | D_a | 20.4 dB | 12.0 dB | 8.1 dB | 4.0 dB | 2.2 dB | 1.3 dB |
  | Share of `attack` applied | 79 % | 58 % | 42 % | 19 % | 6 % | 0 % |

  The boost or cut has mostly gone after one `att_time` and is over after two. A higher `sens` holds it
  longer (the law saturates on a smaller D). On a low note the count starts at the first peak, a quarter
  period after the note begins (D_a stays high while p is still climbing).
- **Sustain: what `sus_time` does.** It sets the reference rate, so it chooses **which notes count as
  decaying**, and a longer value shapes more of them, sooner (it is not "how long the cut lasts"):
  - a note decaying slower than 0.16 × the reference rate (4.6 dB / s at 300 ms, 0.7 dB / s at 2 s) is
    left alone (D_s under the dead zone);
  - between 0.16 × and 1 × it gets a steady part of `sustain` (r = 0.5: 31 %; r = 0.75: 58 %);
  - faster than the reference, D_s keeps growing and G_sus goes to the whole `sustain`.

  Prototype figures [P]: a snare decaying at 200 dB / s, `sus_time` 250 – 1,000 ms, reaches 55 – 59 % of
  `sustain` 100 ms after the hit and 85 – 88 % after 200 ms (`sc_hpf` 120); with `sus_time` 60 ms only
  40 % and 61 %, and it falls back. A snare at 60 dB / s with `sus_time` 1 s: 18 %, 43 % and 74 % at 100,
  200 and 400 ms. A guitar note decaying at 8 dB / s, `sustain` +10 dB, `sus_time` 2 s: +3.4 dB after
  0.5 s, +5.1 dB after 1 s, +6.9 dB after 2 s, +8.6 dB after 4 s; with `sus_time` 300 ms: +1.4 dB.
  So: drums and muted chops 200 – 500 ms; guitar and bass sustain 1 – 3 s; a short value (60 – 150 ms)
  only to catch very fast tails (a room after a stop) and leave everything else.
- The sustain gain starts W after the peak (the hold) and then moves at the pace of D_s: it is a tail
  shaper, not a gate. A faster response is a higher `sens`, not a shorter `sus_time`.
- Both gains add in dB: G = G_att + G_sus. No mode, no ratio: two gains, two times, one sensitivity.
- Not taken: an adaptive D_ref (normalising each onset to a typical one). It would give a soft re-pick the
  same boost as a hard one, and so flatten dynamics. Not taken in v1: a separate sensitivity for the
  sustain (to judge on the bench: the presets that cut tails ask for more `sens` than those that boost
  attacks).

### 2.4 Floor bound for the sustain boost

For a **boost**, `G_sus ≤ max(0, 20 · log10(E_rf / F))`: the tail is never lifted by more than its own
height above the floor.

- A tail far above the floor gets the whole `sustain`.
- As the tail approaches the floor, the lift shrinks to 0. Noise at or under F gets exactly 0 dB, so a
  +24 dB sustain does not raise hiss, room tone or amp noise between notes.
- Noise **above** F is lifted by at most its height over F (hiss 4 dB over the floor: +4 dB while E_rs
  comes down, then 0 [P]). The remedy is `thresh`, set just above the rig's hiss, as in gfx-swell.
- The law is continuous: no gate opens or closes, so nothing clicks.

A **cut** has no bound (cutting noise is harmless). The attack needs none: D_a is 0 on anything under F.

### 2.5 Attack band

`att_band` chooses where G_att acts. g_a = 10^(G_att / 20).

| `att_band` | Attack stage | Use |
|---|---|---|
| **Full** (default) | `y = g_a · x` | Drums, voice, full-range shaping |
| **Highs** | `y = x + (g_a − 1) · HP₁(x)`, a TPT one-pole high-pass with cutoff `att_freq` · √g_a | Pick and string click on bass and low-tuned guitar; the fundamental is hardly touched (41 Hz, +24 dB burst, `att_freq` 1 kHz: the added signal is 21 dB under the note, against 20 dB over it in Full [P]) |
| **Lows** | `y = x + (g_a − 1) · LP₁(x)`, cutoff `att_freq` / √g_a | Kick thump, bass "knock", without boosting the click |

- **This is an exact first-order shelf** whose midpoint stays at `att_freq` for any gain [A]:
  `1 + (g − 1) · s / (s + ω₀√g) = √g · (√g·s + ω₀) / (s + ω₀√g)`. Zero at ω₀ / √g, pole at ω₀√g.
  - Its magnitude is monotonic, from 1 to g: no dip and no bump at any gain (checked from −24 to +24 dB
    in 0.5 dB steps, Highs and Lows [P]). A fixed-cutoff second-order Butterworth high-pass in the same
    mix dips by 3.7 dB at +12 dB and 9.1 dB at +24 dB [P], so it was rejected.
  - With `att_freq` 1 kHz, Highs [A, P]; Lows is the mirror image about `att_freq`:

    | Attack gain | 80 Hz | 300 Hz | 1 kHz | 3 kHz | 8 kHz |
    |---|---|---|---|---|---|
    | +12 dB | +0.1 | +1.2 | +6.0 | +10.5 | +11.8 |
    | +24 dB | +0.4 | +3.8 | +12.0 | +19.6 | +23.0 |
    | −12 dB | −0.1 | −1.2 | −6.0 | −10.5 | −11.8 |

    The digital filter (bilinear, pole prewarped) is within 0.1 dB of this up to 3 kHz and 0.6 dB short at
    8 kHz at 48 kHz [P]. The cutoff is clamped to 0.45 fs (`att_freq` 8 kHz at +24 dB asks for 32 kHz):
    the shelf is then still first-order and monotonic, with its midpoint lower.

- At g_a = 1 the added term is exactly 0: bit-transparent, like Full.
- The cutoff moves with g_a sample by sample. A TPT one-pole stays stable under audio-rate cutoff
  modulation (random gains of ±24 dB per sample on noise stay bounded [P]); tan() comes from a polynomial
  (error < 10⁻⁵ up to 0.45 fs). The output is as continuous as g_a is: g_a is always smoothed (§2.6), and
  with that the added signal moves by at most 4 % of the input's amplitude per sample on a +24 dB burst
  in Highs (sines of 41 – 300 Hz) [P]. Lows steps as Full does: it is a gain on the lows.
- **Why not a crossover.** A 2-band Linkwitz-Riley split sums to an all-pass, not to the input, so the
  block would not be transparent at 0 dB. gfx-dsp also has no LR module yet (noted in gfx-swell's review).
  Waves' 4 bands are a mastering tool. One shelf gives what players ask for ("attack on the highs") at the
  cost of two multiplies.
- The sustain gain always acts on the full band.
- A change of `att_band` crossfades the two structures over 20 ms (gfx-dsp `smooth` crossfader). A change of
  `att_freq` is smoothed (20 ms).

### 2.6 Gain motion, zero latency and lookahead

The smoothing acts on the **share** (0 … 1), before it is multiplied by `attack` or `sustain`. The
engine then has no sign to handle, and an EXP sweep of `attack` through 0 changes nothing in it.

- **Zero latency** (`lookahead` Off).
  - The attack share reaches its target through a one-pole of time `smooth` (0.05 – 5 ms, default 0.5 ms)
    on its rises; its falls are already smooth (they follow E_s).
  - The sustain share passes through a one-pole of max(`smooth`, 0.5 ms), both ways. It needs it at a new
    onset: D_s falls to 0 while the new note climbs past the old tail, so a tail boosted by 10 dB (or cut
    by 21 dB) came back to 0 in 0.5 ms, with steps of 3.6 dB (7.2 dB) per sample; smoothed: 0.35 dB
    (0.7 dB) per sample, done in 1.4 ms [P].
  - A **boost** therefore rises *with* the transient: the gain changes inside the first cycle. At
    `smooth` 0.5 ms the first cycle of a 1 kHz burst peaks at +8.6 dB for `attack` +12 dB, at +11.4 dB
    with `smooth` 0.05 ms, at +1.6 dB with 5 ms [P]. That is the "snap" of zero-latency shapers;
    `smooth` near 5 ms gives NI's Smooth: less bite, less grit on distorted guitars.
  - A **cut** cannot catch the first ~`smooth` of the transient: at −24 dB the first cycle of the same
    burst is only 7.8 dB down (`smooth` 0.5 ms) [P]. A short tick leaks through, as on every zero-latency
    shaper.
- **Lookahead** (`lookahead` 1, 2 or 5 ms, reported). The audio is delayed by L and the gain is computed
  from the undelayed detector.
  - Ramp length R = clamp(`smooth`, 0.25 ms, L). The attack share goes through, in this order: a **moving
    maximum** over R, two cascaded box filters of total length R, and a **delay of L − R**
    (gfx-limiter's Signalsmith engine, run on a share instead of a reduction).
  - The gain therefore reaches its full value **at** the onset, having ramped in over R before it, and the
    first cycle is boosted or cut as a whole: for `attack` +12 dB the gain moves by 1.1 dB inside the
    first cycle (9.2 dB at zero latency), and at −24 dB the first cycle is 21.9 dB down [P]. This is
    TransMod's "clear and sharp". After the onset the gain decays as §2.3 says, about R / 2 late.
  - The sustain share is delayed by L − R and passes through the same two box filters (no maximum): it
    has finished moving when the onset arrives.
  - `smooth` keeps its meaning in both modes: how long the gain takes to move into an attack. **L only
    bounds it**: at the default `smooth`, 1 ms of lookahead does everything, and 2 or 5 ms give exactly
    the same gain [P]. The longer settings are for long ramps (`smooth` 2 – 5 ms), which keep a boost on
    a bass note from starting with a step.
  - Values: Off · 1 ms · 2 ms · 5 ms, **gfx-limiter's list** (one vocabulary for golem; the delay line
    is this block's own).
  - Setup only (gfx-limiter, gfx-swell): set at insert or by a preset, never automated, never on EXP.
    `set_latency_samples` is called only when the value changes, and the audio crossfades over 20 ms across
    the change.
- **Low notes, honestly.** The detector is ripple-free, so a steady note is never modulated. An *attack* on
  a bass note is a gain change shorter than a period, though, and it reshapes that note's first cycles
  (§1 Literature). On a 41 Hz note with `attack` +12 dB the gain is +6.2 dB at the first peak and gone
  after one period with `att_time` 15 ms: the boost is one louder half-cycle, a thump with a click's
  spectrum. With `att_time` 50 ms it is +9.3, +5.3 and +1.1 dB at the first peak, one period and two
  periods; with 100 ms, +10.3, +7.6 and +4.2 dB [P]. The remedies are `att_band` Highs (the usual one) or
  `att_time` ≥ 2 periods (50 – 100 ms on a low E). This is stated in the Applications, not hidden behind
  a rule.

### 2.7 Output, stereo, key

- **Order after the gain**: × `level` → `mix` → ceiling. The ceiling acts last, so it is truly the
  output's maximum.
- **`mix`**: a linear crossfade, computed as out = dry + m · (wet − dry), so that it is exact when the
  wet equals the dry. The dry comes from the same lookahead delay. The wet differs from the dry only by
  gains and a minimum-phase first-order shelf whose real part is always positive, so the blend cannot
  comb or dip. Its use is parallel shaping into the ceiling (dry body + clipped, boosted attack), and the
  "less of everything" a user expects from a mix knob.
- **`ceil_mode`** (the ceiling at `ceiling` dBFS; not `clip`, which is gfx-limiter's softness in %):
  - **Off** (default): peaks may exceed 0 dBFS. golem runs in float, so a boosted attack drives the next
    block (an amp, a NAM capture) harder: often the point of a shaper placed before an amp. At the end of
    a chain golem's safety limiter (gfx-limiter, last on every route to the card) catches the peaks.
  - **Soft**: gfx-limiter's Clip style as shipped (ADAA soft clip at 2×, softness 50 %; aliasing ≈ −57 dB
    measured on gfx-limiter). Distorts only the boosted peaks: Kilohearts' and Softube's Clip. As
    shipped, the knee starts about 4.7 dB under `ceiling` and peaks land 0 – 1.7 dB under it; the whole
    signal passes through the minimum-phase 2× pair (a few samples of group delay in the lows, not
    reported, and not bit-transparent).
  - **Limit**: gfx-limiter's Clean style (auto release, 50 ms) in its **zero-latency** mode, whatever
    `lookahead` is. No sample above `ceiling`; bit-transparent below it. NI's Limit, done properly.
  - **Why the limiter does not share the lookahead.** A lookahead limiter must see its own input L before
    it outputs it. Its input here is the shaped signal, which exists only once the shaper's gain has been
    applied, at output time: one delay cannot serve both. The causal forms are a second delay (latency
    2 L) or a split of L between the two (the limiter gets L − R, which moves with `smooth`). Neither is
    worth it in v1: the zero-latency Clean mode already guarantees the ceiling.
  - Reuse: gfx-limiter's `Limiter` is taken as a library dependency (the crate without its `plugin`
    feature). Its core moves to gfx-dsp `limiter` as a pure move, nulled bit for bit against the shipped
    plugin and announced to golem-rack first (it is golem's safety limiter).
- **Stereo.** Mono 1 / 1 first (a dynamics block before the amp must not make the lane stereo); 2 / 2
  offered. `link` (0 – 100 %, gfx-comp's id) blends the **rectified sidechain** before the sliding
  maximum: a_ch = link · max(\|L\|, \|R\|) + (1 − link) · \|x_ch\|.
  - 100 %: both channels read the same level, one detector, one G;
  - 0 %: each channel has its own detector;
  - in between, two detectors on blended levels. Never three.
- **Key** (`source` Key): a VST3 auxiliary input on its own bus, as gfx-gate and gfx-swell. Detection reads
  the clean DI while the shaper sits after the drive or amp: a high-gain signal has almost no level
  changes left to detect, but its DI does. Falls back to Main when nothing is connected.
- **`listen`** (on / off, as gfx-gate): the filtered sidechain, for setting `sc_hpf` / `sc_lpf`.
- **`delta`** (on / off): wet − dry, what the shaper adds or removes; a boosted attack heard alone is the
  "transient part" of a split, §2.8. `listen` wins when both are on. Both crossfade over 5 ms.

### 2.8 Transient / tonal split (Physion): not in v1

- A real split (Physion, median-filter HPSS) separates *spectral* components that change from those that
  persist. That needs frames of 20 – 90 ms, hence latency. The latency has been confirmed by Eventide's
  staff and criticised in reviews. A golem lane is live.
- A zero-latency time-domain "split" (x · w(t) and x · (1 − w(t)), w from D_a) is exactly what this block
  already computes: sending the two parts to different effects would need routing inside the block.
  golem's lanes are serial, so the useful case reduces to processing the transient part alone, which
  `delta` and `mix` give.
- So **no `split` mode**. If golem one day has parallel lanes, a "transient send" is a new block; Open
  question 4.

### 2.9 Transparency, latency, CPU

- **Bit-transparent** when `attack` = `sustain` = 0, `level` 0 dB, `ceil_mode` Off, or Limit with the
  signal below `ceiling`, at any `mix`, `att_band`, `link` and `sens`: the gain path is skipped exactly
  (not an exp(0) rounding), and `mix` is dry + m · (wet − dry). With `lookahead` ≠ Off it is the input
  delayed by L, bit for bit. `ceil_mode` Soft is not bit-transparent (§2.7).
- **Latency**: **0** at default. With `lookahead`: L, reported, whatever `ceil_mode` is. Soft adds the
  2× pair's group delay and the ADAA half-sample on the whole signal, as gfx-limiter's Clip style, which
  reports 0 and passes the bench's latency check.
- **CPU estimate** [A] (48 kHz, mono, release build; from gfx-gate / gfx-limiter's measured ~0.1 µs per
  sample for similar work):

  | Part | Cost |
  |---|---|
  | Sidechain filters (2 SVF), sliding maximum, 4 followers | ≈ 25 ns / sample |
  | 2 log, 3 exp (fast polynomial forms, ≤ 0.01 dB error) | ≈ 25 ns |
  | Shelf (Highs / Lows) with per-sample tan | ≈ 10 ns |
  | Lookahead maximum + box filters | ≈ 10 ns |
  | Ceiling: Soft (2×) or Limit | ≈ 30 – 50 ns |
  | **Total** | ≈ 60 – 120 ns / sample, ≤ 0.6 % of a core (budget 1 %) |

  Stereo below `link` 100 % doubles the detector. The sliding maximum is O(1) amortised at any W and
  sample rate (7,680 samples at 192 kHz).

### 2.10 What is reused, what is new

| From | Taken |
|---|---|
| gfx-dsp `envelope` | `SlidingMax` (fast envelope), `PeakFollower` (E_rf), one-pole helpers |
| gfx-dsp `svf`, `smooth`, `exp_smooth`, `delay` | Sidechain filters; crossfades and parameter smoothing; EXP; lookahead delay |
| gfx-gate | Window rule W(`sc_hpf`); `source` Main · Key with fallback; `listen` idea |
| gfx-limiter | The whole ceiling (Clean and Clip styles, zero latency), the `lookahead` list, and the moving-maximum / box-filter engine (re-used on the share) |
| gfx-swell (design) | Phrase level P, `thresh` as "Noise Floor", setup-only lookahead with crossfade, `bypass` / `level` ids |
| gfx-comp | `sc_hpf`, `link`, `mix` ids and meanings |
| **New in gfx-dsp** | `transient.rs`: the differential detector (both pairs, floor, laws), a shared module so gfx-comp or gfx-drum blocks can reuse it; a TPT one-pole with per-sample cutoff in `svf.rs` (the shelf) |
| **Shared with gfx-dsp `onset`** | The **level**: sidechain filters + `SlidingMax` + the window rule, one helper with the rule's divisor as an argument (gfx-gate and gfx-ducker's `onset::Pick` on an external level use `sc_hpf`; this block uses `sc_hpf` / 2), and the phrase level P. gfx-ducker's Trigger reads the same sliding maximum against a 150 ms average and fires above a threshold: the same quantity as D_a, with a decision added |
| **Not reused, on purpose** | gfx-dsp `onset` itself (gfx-swell's module: level gate, pick, flux, pitch jump, strum logic; gfx-killswitch's and gfx-ducker's `Pick`). A shaper should not decide "onset or not": a binary detector brings false triggers and missed notes, and its errors are audible as jumps. The differential envelope is continuous: a doubtful onset gets a small, proportional boost (and under 1.5 dB, none). gfx-ducker's design states the same boundary from its side |

## 3. Parameters

New block: no catalogue ids. Shared ids keep their contract or sibling meaning: `mix` (law stated
below), `gr` (read-only), `sc_hpf` / `link` (gfx-comp), `source` / `listen` (gfx-gate, a boolean as
there), `thresh` / `level` / `bypass` (gfx-swell), `lookahead` (gfx-limiter's list), `smooth` (ms, as
gfx-killswitch), `ceiling` (dB, as gfx-drive). Ranges are linear; enums only grow at the end; no default
depends on another parameter.

**Not the contract's `env_att` / `env_rel` / `env_sens`** (confirmed at review). Those name a follower
that *drives* an effect: a longer `env_att` makes the effect react later. Here the effect reacts at once;
a longer `att_time` makes the attack gain last longer, and a longer `sus_time` shapes more notes. Same
unit, opposite reading for golem's AI, so different ids. `sens` is kept: it is gfx-killswitch's and
gfx-swell's id for "how small a rise counts".

**Ids that differ from a sibling's on purpose.** `attack` and `sustain` are gains in dB here (the names
the whole field uses for a transient shaper), while `attack` is a time in ms in gfx-gate, gfx-comp and
gfx-ducker. Ids are per block and the descriptor carries the unit, so they stay; golem's AI notes say so
(Applications). `ceil_mode` is not called `clip` (gfx-limiter's `clip` is a softness in %), and the
shaping meter is not called `shape` (an enum in four siblings).

### Front panel

| id | Name | Type | Range | Default | EXP | Role |
|---|---|---|---|---|---|---|
| `attack` | Attack | f | −24 – +24 dB | 0 dB | ✓ | Gain on a full transient; + = more punch, − = softer. Fades over `att_time` |
| `sustain` | Sustain | f | −24 – +24 dB | 0 dB | ✓ | Gain on the tail once it has decayed; + = longer ring, − = tighter. Never lifts anything above its height over the floor |
| `att_time` | Attack Length | f | 1 – 100 ms | 15 ms | | How long the attack gain lasts (≈ 80 % gone after one length, over after two, §2.3) |
| `sus_time` | Sustain Length | f | 20 – 3,000 ms | 300 ms | | Which notes count as decaying: the reference falls at 8.69 / length dB / s. Longer = more notes shaped, and sooner (§2.3). 200 – 500 ms drums and chops, 1 – 3 s guitar sustain |
| `att_band` | Attack Band | e | Full · Highs · Lows | Full | | Where the attack gain acts (§2.5) |
| `ceil_mode` | Ceiling | e | Off · Soft · Limit | Off | | Output ceiling stage (§2.7) |

### Expert

| id | Name | Type | Range | Default | EXP | Role |
|---|---|---|---|---|---|---|
| `sens` | Sensitivity | f | 0 – 100 % | 50 % | | The level jump (attack) or fall under the reference (sustain) that counts as full: D_ref = 22 → 2 dB, above a fixed 1.5 dB dead zone |
| `smooth` | Smooth | f | 0.05 – 5 ms | 0.5 ms | | Time the gain takes to move into an attack (and the lookahead ramp) |
| `att_freq` | Attack Band Freq | f | 200 – 8,000 Hz | 1,500 Hz | | Shelf midpoint for Highs / Lows |
| `sc_hpf` | Sidechain Low Cut | f | 0 – 2,000 Hz | 0 (off) | | Detector high-pass; also sets W (40 ms up to 60 Hz, 20 ms at 120 Hz, 8 ms from 300 Hz): shorter hold, faster sustain response. Keep it at or under twice the lowest note (§2.2) |
| `sc_lpf` | Sidechain High Cut | f | 500 – 22,000 Hz | 22,000 Hz | | Detector low-pass; ≥ 20 kHz = off |
| `thresh` | Noise Floor | f | −96 – −30 dB | −66 dB | | Absolute floor: nothing below is shaped, sustain never lifts toward it (§2.4) |
| `source` | Detector Source | e | Main · Key | Main | | Key = auxiliary input (the clean DI after an amp) |
| `link` | Stereo Link | f | 0 – 100 % | 100 % | | Stereo layout only |
| `lookahead` | Lookahead | e | Off · 1 ms · 2 ms · 5 ms | Off | | Reported latency; the gain arrives before the transient. Setup only |
| `ceiling` | Ceiling Level | f | −24 – 0 dB | 0 dB | | Maximum output level for `ceil_mode` Soft / Limit |
| `mix` | Mix | f | 0 – 100 % | 100 % | ✓ | Linear crossfade dry / shaped (dry delay-aligned) |
| `level` | Level | f | −24 – +12 dB | 0 dB | ✓ | Output level, before the ceiling |
| `listen` | Listen | b | off / on | off | | Outputs the filtered sidechain (gfx-gate's `listen`) |
| `delta` | Delta | b | off / on | off | | Outputs only what the shaper adds or removes (wet − dry) |
| `bypass` | Bypass | b | off / on | off | | VST3 bypass parameter (`make_bypass`), as the coded effects |

Read-only: `shaping` (current G_att + G_sus applied, dB, −48 – +48; for golem's meter), `gr` (ceiling
reduction, dB, 0 – 90).

Smoothing: `attack`, `sustain`, `mix`, `level` through `exp_smooth` (the contract's EXP rule); `att_time`,
`sus_time`, `sens`, `smooth`, `thresh`, `ceiling`, `att_freq`, `link`, `sc_*` 20 ms (W follows the smoothed
`sc_hpf`: `SlidingMax::set_window` drops or keeps entries, it never steps p upward); enums by 20 ms
crossfade (`lookahead` §2.6); `listen` / `delta` 5 ms.

Negative minimums (`attack`, `sustain`, `thresh`, `ceiling`, `level`): linear in REAPER as measured in the
contract; checked once more on the bench with this block.

## 4. Applications

Block presets (generic names, no artists or trademarks). Times in ms.

| Use / preset | Settings to start from | Feature that matters |
|---|---|---|
| **Snap Pick** (clean / funk guitar, before the amp) | `attack` +8, `att_time` 12, `att_band` Highs, `att_freq` 1,500 | Shelf: pick click up, the low strings' fundamental untouched |
| **Soft Pick** (jazz, neck pickup, tame a harsh pick) | `attack` −9, `att_time` 20, `smooth` 2 | Cut at zero latency; `lookahead` 2 to remove the leading tick |
| **Funk Pop** (16th strums) | `attack` +10, `sustain` −8, `att_time` 8, `sus_time` 250, `sens` 70, `sc_hpf` 120 | W 20 ms (safe down to a low B); muted chops fall faster than the reference and are pulled down, ringing chords are not |
| **Chug Tight** (metal rhythm, before the amp) | `attack` +6, `sustain` −10, `att_time` 6, `sus_time` 250, `sens` 70, `sc_hpf` 120 (100 in drop A) | Palm-mute definition into the amp; complements gfx-gate |
| **Pick Definition after the amp** | `source` Key (DI), `attack` +6, `att_band` Highs, `att_freq` 2,500, `ceil_mode` Limit, `ceiling` −1 | Detection on the DI; restores what amp compression flattened |
| **Ring Out** (clean arpeggios, ambient) | `sustain` +10, `sus_time` 2,000, `thresh` −60 | Long reference; floor bound keeps hiss down |
| **Bass Click** | `attack` +8, `att_band` Highs, `att_freq` 1,200, `att_time` 15, `sc_hpf` 0 | Click up, fundamental not modulated (§2.6 low notes) |
| **Bass Thump** | `attack` +6, `att_band` Lows, `att_freq` 300, `att_time` 60, `smooth` 3 | Attack ≥ 2 periods of a low E: the boost covers whole cycles, not one half-cycle |
| **Bass Even** (fingerstyle) | `attack` −6, `sustain` +6, `att_time` 30, `sus_time` 1,000 | Pluck softened, notes held: comp-like evenness without a threshold |
| **Drum Punch** (drum bus / backing track) | `attack` +9, `sustain` −4, `att_time` 10, `sus_time` 250, `sc_hpf` 120, `ceil_mode` Soft | W 20 ms; soft clip on boosted peaks. Accents get more than ghost notes (§2.2) |
| **Gated Snare** (snare track) | `sustain` −24, `sus_time` 400, `sens` 80, `sc_hpf` 200 | Tail cut without a threshold, at any hit strength: on a snare decaying at 200 dB / s, half the cut 60 ms after the hit and all of it by 150 ms [P]. A tail shaper, not a gate: for a hard gate, gfx-gate |
| **Room Dry** (voice, live mic) | `sustain` −8, `sus_time` 150, `sc_hpf` 100 | Short reference (58 dB / s): only the room's fall after a phrase is faster, held notes are left alone; level-independent across a quiet and a loud singer |
| **Consonants** (voice) | `attack` +5, `att_band` Highs, `att_freq` 3,000, `att_time` 10 | Diction without a de-esser's job undone (pair with gfx-deesser after) |
| **Acoustic Body** | `attack` −4, `sustain` +6, `sus_time` 800 | Less pick, more body |
| **Studio Clean Punch** | any of the above + `lookahead` 1 (2 with `smooth` 2), `ceil_mode` Limit | Gain ramps in before the transient; no overs |

Placement advice for golem's AI and its docs:
- **Before the amp / drive**: an attack boost hits the amp harder (more pick bite and compression); an
  attack cut smooths the pick into the amp; a sustain boost acts like more compression on tails.
- **After the amp** with `source` Key (the DI): re-shapes a compressed high-gain sound.
- Before gfx-comp to keep attacks a compressor would dull; after gfx-swell is pointless (it removed them).
- Live presets keep `lookahead` Off or 1 ms; golem never switches a preset that changes `lookahead` inside
  a song.
- For golem's AI: `attack` and `sustain` are **dB gains** here, not times (the times are `att_time` and
  `sus_time`). `sc_hpf` stays at or under twice the lowest note played. A boost above +12 dB goes with
  `ceil_mode` Soft or Limit unless an amp follows. To soften picks, this block; to remove them, gfx-swell.

## 5. Bench suite (on top of gfx-bench's common checks)

Synthetic signals made in the test (tone bursts, exponentially decaying Karplus-Strong strings at 41,
82, 330 Hz, a synthetic kick and snare). DIs: clean.wav, funk.wav, crunch.wav, high-gain.wav; bass and
voice DIs when they exist (design-review §7).

| Check | Pass |
|---|---|
| **Transparency** | `attack` = `sustain` = 0, any `att_band` / `mix` / `link` / `sens`, `ceil_mode` Off and Limit: bit-identical; with `lookahead` 2 ms: bit-identical delayed by L |
| **Level independence** | funk.wav and the drum loop at 0, −12, −24, −36 dB (`thresh` −96): G(t) identical ± 0.1 dB |
| **No ripple on steady notes** | Sines at 31, 41, 82, 1,000 Hz and an asymmetric wave (fundamental + 2nd + 3rd harmonic) at 31, 41, 82 Hz, `attack` ±24, `sustain` ±24, `sens` 50 and 100, after 300 ms: gain within 0.05 dB peak to peak, added harmonics ≤ −80 dB re. the fundamental (`sc_hpf` 0; 41 Hz also at `sc_hpf` 80, 82 Hz at 160) |
| **No ripple under the sidechain filter** | Decaying strings at 41 Hz (`sc_hpf` 80), 62, 73 and 82 Hz (`sc_hpf` 120), 82 Hz (`sc_hpf` 160), `attack` ±24, `sens` 100, 0.3 – 0.8 s after the pick: G_att within 0.05 dB peak to peak (prototype 0.000; 6 – 15 dB with W = 1.2 / `sc_hpf`) |
| **Steady noise** | White noise, 3 s, `attack` ±12: \|G_att\| ≤ 0.3 dB at `sens` 50 and ≤ 1.5 dB at `sens` 100 (prototype 0.10 and 0.56 dB); a ringing six-string chord: 0 |
| Decaying bass note | 41 Hz string, `sustain` ±24, `sens` 100, `sus_time` 300 and 2,000: G_sus ripple inside one period ≤ 0.5 dB (prototype 0.37 dB at `sc_hpf` 80), added harmonics ≤ −60 dB at the default `sens` (target, first bench records the figure) |
| Attack law | 1 kHz burst from silence, `attack` +12, `smooth` 0.05: peak gain 12 × 0.79 … 12 dB within the first 0.1 `att_time`; the §2.3 table within ± 0.5 dB at 0.5, 1, 1.5 `att_time`; 0 from 2 `att_time` |
| Sustain law | Burst decaying at 60 dB / s, `sustain` +12, `sus_time` 1,000: G_sus matches §2.3 ± 0.3 dB |
| **Floor bound** | Notes with hiss at −70 dBFS in the gaps, `thresh` −66, `sustain` +24: gain on the hiss 0 ± 0.1 dB; tail never lifted above its height over F |
| Sensitivity | Re-picks over a ringing note, rises of 3 / 6 / 12 / 24 dB: boost monotonic in the rise (11 / 30 / 57 / 82 % ± 3 at the default); `sens` 100 % gives ≥ 85 % of `attack` at 6 dB; a 1 dB rise gives 0 |
| **Onset over a shaped tail** | A note 600 ms after one decaying at 40 dB / s, `sustain` +12 and −24: the gain never moves by more than 1 dB per sample at 48 kHz (prototype 0.35 and 0.7 dB) |
| Soft hit after a loud one | The §2.2 table within ± 10 points (documents the limit; guards against a regression of W) |
| Attack band | 80 Hz + 3 kHz burst, Highs, `att_freq` 1 kHz, `attack` +12: 80 Hz ≤ +0.2 dB, 3 kHz +10.5 ± 0.3 dB; monotonic response at ±24 dB (no dip below 0 or above the gain) |
| **Lookahead** | 1 kHz burst, `att_time` 15, `lookahead` 1, 2 and 5, `smooth` 0.5: `attack` −24: the first cycle ≥ 20 dB down (prototype 21.9; 7.8 at zero latency); `attack` +12: the gain is 0 until R before the onset, ≥ 11.5 dB at the onset, and moves by ≤ 1.5 dB inside the first cycle (shape kept; 9.2 dB at zero latency). The three lookaheads give the same gain. Zero latency: the leak of the same cut is recorded |
| Ceiling | Drum loop at −1 dBFS, `attack` +24: `ceil_mode` Limit, every `lookahead`: no sample above `ceiling`, reported latency = L; Soft: no sample above `ceiling` |
| Mix | 50 %, Highs, `lookahead` 2: no comb (response flat ± 0.1 dB where the gain is 0); with the gain held at ±24 dB the response stays between 0 dB and the gain at every frequency |
| Stereo link | Signal on L only: `link` 100 → same gain on R; `link` 0 → R bit-identical |
| Key | `source` Key unconnected: as Main |
| Clicks | `att_band` switch mid-note, `attack` sweep by EXP through 0, `sc_hpf` sweep 0 → 300 Hz on a held chord: within gfx-bench's click bound |
| Latency | 0 at default; 1 / 2 / 5 ms reported = measured ± 1 sample; a change crossfades without a click |
| Determinism, CPU | Strict null; ≤ 1 % of a core at 48 kHz, stereo unlinked included |

**gfx-bench options**: defaults. `param_steps` skips `lookahead` (it changes the reported latency). The
common checks start from silence with neutral defaults, so they also prove transparency.

**Listening**: Snap Pick, Soft Pick, Funk Pop, Chug Tight (before a NAM amp), Pick Definition after the
amp, Bass Click / Thump, Drum Punch, Room Dry. Blind, level-matched A / B against a commercial shaper Raph
owns (SPL-style), and Soft Pick against gfx-swell's "Soft attack".

## Visual

- **Category `dynamics`**, the family's **thrash** finish and slate hue `#4a5a6e`, **no crate hue**, the
  family's rune light. Thrash's diamond plate and hazard yellow say "attack", which is this pedal's job
  (gfx-swell, its opposite, went anodized for the same reason). The motif and the name set it apart from
  gate, comp and limiter.
- **Motif `spikes`** (new case in `motifParts`), **either side of the rune**, not along the bottom.
  Rendered at review in the real renderer: on thrash the name sits at y ≈ 145 – 160, white with a black
  outline, so a row on gfx-swell's baseline (H − 40) ran behind "TRANSIENT" and only its tips showed.
  - Baseline y0 = cy + ch / 2 + 3, top yTop = cy − ch / 2 − 6. Free width each side: (W − cw) / 2 − 3;
    two envelopes per side, each w = that width / 2 − 2 wide, 2 px apart.
  - Left side x0 = 2 + k · (w + 2), right side x0 = cx + cw / 2 + 3 + k · (w + 2), k = 0, 1.
  - In each slot, the **original** envelope as a ghost (opacity .35):
    `M x0 y0 L x0+1.5 y0−12 Q x0+.3w y0−6 x0+w y0`.
  - Over it, the **shaped** envelope (opacity 1): a taller, sharper spike, then a lifted tail:
    `M x0 y0 L x0+1.2 yTop Q x0+.25w y0−9 x0+w y0−t`, with t = 4 for k = 0 and 2 for k = 1, so the pair
    reads as two shaped notes, not a repeated stamp.
  - All `line`, so it is yellow lined in black on thrash and engraved on anodized.
  - Against its neighbours at tile size: gfx-gate's barbed wire and gfx-comp / gfx-limiter's hazard band
    are horizontal; gfx-ducker's `dips` hang from a line at the top of the rune band and come back in a
    curve; `spikes` stand on a baseline at its bottom and point up. gfx-swell's `swells` sit low on an
    anodized pedal. gfx-mbcomp's `tiers` are short horizontal bars with clamps in the same zone: bars
    against peaks.
- **Face** (`visuals/data/faces.json`):
  `"gfx-transient": {"name": "Transient", "blocks": ["transient"], "knobs": ["attack", "sustain", "att_time", "sus_time"], "selectors": ["att_band", "ceil_mode"], "category": "dynamics"}`.
  If "Transient" is too wide at tile size: "Snap".
- **Glyph** `transient` (24 × 24, single colour, stroke 1.8, round caps; title "Transient shaper"):
  ```
  <path d='M2.5 20.5h19' opacity='.4'/>                       baseline
  <path d='M4 20.5L6.5 13C10 16 14 18.5 21.5 19.5' opacity='.35'/>   the original envelope
  <path d='M4 20.5L6 3.5L8.5 12C12 15 16 16 21.5 16.5'/>     the shaped one: sharper attack, lifted tail
  ```
  Reads as "an envelope redrawn". Distinct from `gate` (square step), `comp` (arrows to a line),
  `limiter` (a trapezoid under a ceiling), `swell` (a late rise).

## Open questions: decisions (review of 2026-11-14)

1. **`attack` / `sustain` in dB, ±24 dB both.** Kept. dB is the contract's real unit and what the law
   delivers. The cut side uses the whole range (−24 dB softens a pick almost away); a boost above +12 dB
   is rarely musical and goes with a ceiling mode (Applications). Block presets stay within ±12 dB.
2. **Default `ceil_mode` Off.** Kept: transparent, and a boost into an amp is the point. golem's safety
   limiter already ends every route to the card.
3. **First-order shelf.** Kept: monotonic at every gain (checked), bit-transparent at 0 dB, two
   multiplies. A second-order gain-tracking shelf only if listening asks for a steeper split.
4. **No transient / tonal split.** Agreed. A latency-bound studio block only if golem gets parallel lanes.
5. **`sens` law.** D_ref 22 → 2 dB kept, with the 1.5 dB dead zone added (§2.3). The default and one
   shared `sens` for both sections are set by ear on the bench.
6. **Floor window.** Fixed at 48 dB; `thresh` is the user's control.
7. **W.** Follows `sc_hpf` / 2 (§2.2); no `low_note` control: one control, one rule ("at or under twice
   the lowest note"). The 40 ms hold with `sc_hpf` off is accepted: the note's own tail hides a soft
   second hit for longer than W does.
8. **Catalogue entry**: for golem-rack (below).
9. **gfx-swell's "Soft attack" preset**: both kept. golem's AI goes to this block to soften and to
   gfx-swell to remove; gfx-swell's README gets that one-line note (not edited here).
10. **Pump** (Kilohearts): no control. `attack` + with `sustain` − , `sus_time` 250 – 400 ms and `sens`
    70 – 80 % gets close; a true dip synced to the hit is gfx-ducker's Trigger keyed on the lane itself.
11. **gfx-limiter as a dependency**: agreed, zero-latency modes only; the move to gfx-dsp `limiter` is a
    pure move, nulled and announced (§2.7).

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good**, Strong once gfx-bench reproduces the prototype figures. The core is right and better
than the surveyed field: dB differences of linear envelopes (level-independent, `att_time` means the same
after silence and on a re-pick), a ripple-free fast envelope, a saturating law, a floor-bounded sustain
boost, an exact first-order shelf. A numpy prototype of the whole detector (bass E1, guitar strings,
snare bursts, noise, beating strings, soft hits after loud ones) confirmed those claims and found **one
error that defeated the main claim, one acausal sharing, and four smaller faults**. All are fixed above.
Figures marked **[P]** come from that prototype (scratchpad `proto-gfx-transient/`).

### Changes made, and why

| # | Change | Why |
|---|---|---|
| 1 | **W = 1.2 / max(`sc_hpf` / 2, 30 Hz)** instead of gfx-gate's 1.2 / `sc_hpf`; rule "`sc_hpf` at or under twice the lowest note"; presets and bench follow | A 12 dB / octave high-pass leaves a note under its corner 7 – 10 dB down with its period intact. Low E under `sc_hpf` 120 (the design's own Funk Pop and Chug Tight): W 10 ms < the 12 ms period, D_a re-fired every cycle, G_att rippled 9.7 dB peak to peak at 82 Hz for ±24 dB (6 – 15 dB over the cases tried). Halved rule: 0.000 dB [P] |
| 2 | **Dead zone D_0 = 1.5 dB** in the law, both sections | The sliding maximum creeps on noise, chords and beating strings (D_a 0.5 – 2 dB). At `sens` 100 % that was random bursts of half the `attack`, and a sustain gain wandering by 65 %. Now 8.5 % and bounded [P]. Also makes "a slow note is left alone" true for the sustain |
| 3 | **The ceiling limiter no longer shares the lookahead**; Limit runs zero-latency, latency = L always | Acausal as written: the limiter's input is the shaped signal, which does not exist L ahead of the output |
| 4 | **Lookahead engine specified**: on the share; maximum over R, boxes of R, then a delay of L − R; the sustain share through the boxes too. "2 ms covers a pick, 5 ms a kick" removed | Without the L − R delay the ramp ends R after the onset, not at it. The ramp is `smooth`, not L: 1, 2 and 5 ms give the same gain at the default [P] |
| 5 | **Sustain section rewritten**: D_s settles at −20 · log10(1 − r) under the reference rate; `sus_time` = which notes are shaped (longer = more); presets corrected | "A note slower than the reference gets nothing" was false (a one-pole lags any decay). The presets used short `sus_time` for tight tails, which does the opposite: Gated Snare at 60 ms reached 40 – 60 % and fell back, at 250 – 1,000 ms 85 % in 200 ms [P] |
| 6 | **Sustain share smoothed** (max(`smooth`, 0.5 ms), both ways) | "Needs no smoothing" failed at a new onset: 3.6 – 7.2 dB per sample while the note climbs past the old tail [P] |
| 7 | `mix` computed as dry + m · (wet − dry); bit-transparency claim limited to `ceil_mode` Off / Limit | (1 − m) · x + m · x is not x in floating point. gfx-limiter's Clip style passes everything through its 2× pair |
| 8 | `link` blends the rectified sidechain before the sliding maximum | The gain blend needed a third detector between 0 and 100 % |
| 9 | Ids: `clip` → **`ceil_mode`**; `listen` a boolean + **`delta`**; meter `shape` → **`shaping`**; note on `attack` / `sustain` in dB | `clip` is a % in gfx-limiter, `shape` an enum in four crates. A 3-option `listen` would put golem's "on" (1.0) on Delta |
| 10 | Bench: new checks (ripple under the sidechain filter, steady noise, onset over a shaped tail, soft hit after a loud one); lookahead check rewritten; attack-law and sensitivity figures | "First and third cycle within 0.5 dB" contradicts the law (11.7 vs 9.1 dB [P]); the old ripple check used pure sines, whose \|x\| has half the period |
| 11 | Shelf: the rejected second-order figures are 3.7 / 9.1 dB (Butterworth), not 2.4 / 6.7; cutoff clamp at 0.45 fs stated; digital deviation given | Checked [P]; `att_freq` 8 kHz at +24 dB asks for a 32 kHz pole |
| 12 | **Visual**: `spikes` moved beside the rune | Rendered in `golem-pedals.js`: on thrash the bottom row ran behind the name. Beside the rune it reads at tile size and is not `dips` upside down in the same place |
| 13 | Survey: Softube's controls corrected (Punch / Sustain each Hi / Wide / Lo, crossover 100 Hz – 4 kHz) [M]; SPL's patent number searched again, still not found [U] | Facts |

Checked and left as written: the dB-difference principle and the mda / Point counter-examples; the
attack table's arithmetic; level independence (0.06 dB over 36 dB [P]); zero gain on steady notes at
`sc_hpf` 0 down to 31 Hz, symmetric or not [P]; the floor bound (hiss under F: 0.00 dB with `sustain`
+24 [P]); the shelf identity, its table and its monotonicity; the TPT one-pole under per-sample cutoff
changes; the low-note honesty paragraph (now with figures); mono first, key on its own bus; linear
ranges; no mode-dependent default; `lookahead` setup-only; not using `env_att` / `env_rel` / `env_sens`;
not using a yes / no onset detector (consistent with gfx-ducker's reviewed §2.10: one `onset` module,
gfx-swell's, and this block outside it).

Coverage against the references: SPL (attack, sustain, no threshold), Kilohearts (clip; pump by
combination), NI (smooth, limit), TransMod (lookahead, rise time = `att_time`), Softube (band for the
attack, clip), Trans-X Wide (sensitivity, duration, range). Left out on purpose: a band for the sustain
(Softube), four bands (Trans-X Multi), TransMod's overdrive, the tonal split.

### Limits to know (drums)

- A softer hit inside the previous hit's tail gets little or nothing (§2.2 table): 125 ms after an
  accent, a ghost note 12 dB down gets 10 – 37 % of the accent's gain, one 18 dB down 0 – 11 %. Flams
  with the grace note first are fine. Acceptable for a drum bus or a backing track in a guitar rig, and
  shared with every broadband shaper; a detector per band would lift it and is not in v1.
- The sustain cut is a tail shaper (hold W, then tens of ms), not a gate.

### Still open

| Point | Owner |
|---|---|
| Reproduce the [P] figures in gfx-bench (Karplus-Strong strings and DIs, not only the prototype's synthetic notes) | golem-fx, first coding task |
| By ear: `sens` default; one `sens` or one per section; D_0 (1 – 2 dB); `smooth` default; whether Room Dry wobbles on a held voice; blind A / B against an SPL-style shaper | Raph, on the bench renders |
| A band for the sustain (`sus_band`, the same shelf on G_sus): cheap, asked by Softube users; not in v1 | Raph |
| gfx-gate and gfx-ducker keep W = 1.2 / `hpf`. The gate's hysteresis covers the ripple; gfx-ducker's "the level never ripples" and its Trigger (rise > R / 2) should be re-checked with a key note under its `hpf` corner | golem-fx (not edited here) |
| One level helper in gfx-dsp (filters + `SlidingMax` + window rule with its divisor) for gate, ducker, swell's `onset::Pick` and `transient.rs` | golem-fx, when the second of them is coded |
| gfx-limiter's core to gfx-dsp `limiter`: pure move, bit-for-bit null, announced first | golem-fx → golem-rack |
| One line in gfx-swell's README: "to soften a pick, gfx-transient" | golem-fx |
| Catalogue entry `transient` ("Transient Shaper", `dynamics`), glyph, face, block presets; key routing after the amp (as gfx-swell); a preset that changes `lookahead` re-syncs cleanly | golem-rack |
| `spikes` in `motifParts`, `transient` glyph; face name "Transient" (9 letters fit at 13 px; "Snap" if the board wants it shorter) | visuals |
| SPL's patent number [U] | nobody needs it |
