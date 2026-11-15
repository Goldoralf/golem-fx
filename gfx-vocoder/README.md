# gfx-vocoder

> **Status: CODED** (v0.1.0, 2026-11-15). Designed 2026-11-14 by Claude Opus 5.5, reviewed by Claude
> Fable 5.2 the same day (Review section), coded as reviewed with the deviations listed in "As coded". A
> complex 4th-order bank on the ERB / Log scale with the 40 Hz floor, the width law, edge trims and G_Σ
> (gfx-dsp's new `filterbank`, which takes explicit band lists in full pole widths for gfx-polysynth), the
> Tracked / Drone / Noise / Self carriers on gfx-dsp's new `osc` (PolyBLEP saw and pulse, closed-form buzz,
> supersaw) and `noise`, flatten, the V/UV switch to a separate noise bank, the gated sibilance path,
> freeze, formant remap and tilt. Zero latency. Measured: summed ripple 1.8 dB at 24 ERB bands, lag ≤ 15.6
> ms in every band at every setting, wet level within 1 dB of the modulator at 8 – 40 bands, V/UV on in
> 2 ms, tracking within 0.1 ¢, bench 28 / 28 at 44.1 – 192 kHz; CPU 487 – 594 ns per sample at the
> defaults (over the 450 ns budget, see "As coded"). The voice-DI rows of §5 wait for the maintainer's recordings.

golem's **channel vocoder**. A bank of band-pass filters measures the spectral envelope of a
**modulator** (a voice, or a guitar) and imposes it, band by band, on a **carrier** (a synth, a guitar, or
the pedal's own oscillators). The result speaks with the modulator's vowels and consonants and sings with
the carrier's pitch and timbre.

The idea: **a vocoder that works on a live rig with nothing else plugged in**, and that stays
intelligible. Three things make it so:
- **Its own carrier, played by the voice**: an oscillator bank that follows the singer's pitch (a robot
  voice, or a whole diatonic chord in the song's key), a drone chord in the song's key, or noise. No
  keyboard, no routing: one voice on one lane is enough. External carriers (another lane, MIDI) come when
  golem routes them (§6).
- **Intelligibility by construction**: the carrier's own spectrum is flattened before it is shaped (no
  more "not enough highs" with a dark carrier), unvoiced sounds switch the carrier to noise, and the
  voice's sibilance above the bank passes straight through — the voiced / unvoiced trick of the EMS and
  Sennheiser machines, at zero latency.
- **Zero latency**: an IIR complex gammatone bank on the ERB scale, not an STFT. What remains is the
  filters' own lag, as in an analog vocoder: 1 – 6 ms where consonants live, never more than 16 ms in
  the lowest bands (§2.2).

It is **not** a pitch shifter. gfx-dsp's `vocoder.rs` is, despite its name, the **phase-vocoder pitch
shifter** of gfx-pitch's Poly engine (`PvShifter`: peak-locked phase vocoder on asymmetric windows,
6.6 – 10 ms reported latency) [V: `gfx-dsp/src/vocoder.rs` lines 1 – 15]. Nothing in it is a channel
vocoder; this crate shares no code with it. The new filterbank module is therefore **not** named
`vocoder`, and the existing file is renamed `pv_shift.rs` before this crate is coded (§2.9). Nor is it
a talk box filter (gfx-autowah's Vowel type moves one formant filter with an envelope; here the formants
come from a real voice).

Mono 1 / 1 first, 2 / 2 second, each with a 1- or 2-channel key auxiliary bus (contract). Zero reported
latency.

## 1. State of the art

Survey of 2026-11-14. **[V]** read in a source, a manual or the code · **[U]** not verified against a
primary source (from secondary listings or memory) · **[A]** our calculation · **[P]** measured on a
Python prototype (`proto-vocoder/` in the design session's scratchpad: `bank.py`, `sweep.py`, `sel.py`;
the review's are in `proto-gfx-vocoder/`: `bank.py`, `floor.py`, `edge.py`, `level.py`).

### What golem has today

- **No vocoder block** in `~/golem/library/catalog/effects.json` (no `vocoder`, no talk box) [V]. The only
  vocoders on the rig are presets inside the Eventide Eclipse (`139 Vocoder10`) and the "Vocoder" type of
  the GoleM Surge FX plugin, which golem does not expose as a block [V: `docs/platforms.md`].
- golem's spectral blocks (`docs/spectral.md`) are FFT-based and report their window as latency; a
  vocoder on a voice cannot afford that.

### Hardware references

| Unit | What it does | What we take |
|---|---|---|
| Homer Dudley's vocoder (Bell Labs, 1939) and its descendants | Analysis bank → rectifier + low-pass per band → gains on a synthesis bank fed by a buzz (voiced) or a hiss (unvoiced) source, switched by a voicing detector and driven by the speaker's pitch [U: 10 bands] | The whole architecture, **including the pitch-driven buzz carrier**: "Tracked" (§2.5) is the original vocoder, not a gimmick |
| EMS Vocoder 2000 / 5000 | 16 channels (2000) [U: vintagesynth listing]; a **voiced / unvoiced detector** whose output switches the internal noise source and oscillator, noise gated by the unvoiced decision ([lost-manuals listing](https://www.lost-manuals.com/equipment.php?id=16204)) [V, secondary]; praised for its sibilance [U] | The V/UV switch to noise; noise only where speech is unvoiced |
| Sennheiser VSM-201 (1977) | ~20 bands, freely assignable ([sequencer.de](https://sequencer.de/synth/index.php/Vocoder)) [V, secondary]; speech detector and voice-mode outputs ([radiomuseum](https://www.radiomuseum.org/r/sennheiser_vocoder_vsm201vsm_20.html)) [V, secondary]. A forum post gives a sibilance feed-through high-pass at ~5,080 Hz above a top band at 4,526 Hz, optionally gated by the V/UV detector, machine not named ([electro-music](https://electro-music.com/event2008/forum/post-45610.html)) [U]. 5,080 Hz is the top of the Bode 7702 / Moog 16-channel vocoder's bank (16 bands, 50 – 5,080 Hz [V, secondary: vintagesynth, dealer listings]), so the post most likely describes that machine; its pass-through is not confirmed by a primary source | **Sibilance pass-through** above the top band, gated by the detector (§2.6) |
| Roland VP-330 / SVC-350 | Strings-and-choir keyboard with a vocoder [U: 10 bands]; rack vocoder [U: 11 bands]. The "Roland choir" vocoder sound: few bands, an ensemble'd carrier | A low-band-count, ensemble'd-carrier preset (Supersaw, 10 – 12 bands); TAL-Vocoder emulates this family |
| Korg MS2000 / microKORG / King | 16 band-passes (King: "two sets of sixteen" [V: King manual via manualslib]; microKORG: 8 channels as pairs of 16 filters [V]); **Formant Shift −2 … +2** and a continuous carrier **fc offset**, both moving the carrier's band-pass filters against the modulator's ([MS2000R manual](https://cellphone.manualsonline.com/manuals/mfg/korg/ms2000r.html?p=56)) [V] | Formant shift as a band remapping, continuous (§2.4) |

### Plugins and code

| Plugin | What it does | What we take / beat |
|---|---|---|
| [Ableton Live Vocoder](https://www.ableton.com/en/manual/live-audio-effect-reference/) [V] | Inserted on the **modulator's** track; carrier **Noise · External · Modulator · Pitch Tracking** (a monophonic saw or pulse following the modulator); Bands, Range, BW (100 % = most accurate), **Formant**, **Unvoiced** level with Sens. and Fast / Slow, Gate (per band), Depth (0 – 200 %), Attack / Release, Precise / Retro, **Enhance** ("normalizing the carrier's spectrum and dynamics", for external carriers), Mono / Stereo / L-R [V]. Band counts up to 40 [U] | The closest model. Its routing (modulator = the track, carrier = sidechain) is golem's too. Our Tracked carrier goes further: diatonic chords in the song key, quantize, supersaw; Enhance becomes `flatten`, a continuous amount |
| [TAL-Vocoder](https://tal-software.com/products/TAL-Vocoder) (free) [V] | 11-band "vintage vocoder emulation", an envelope follower per band; carrier = internal VCO (saw, pulse, noise, sub) played by MIDI, or audio / **sidechain as carrier** / **sidechain as modulator** routing (manual) [V]. Older versions were open source [U] | The routing pair we call Key Input and Talk Box; a vintage 11-band preset |
| iZotope VocalSynth 2, Vocoder module ([help](https://s3.amazonaws.com/izotopedownloads/docs/vocalsynth201/vocoder/index.html)) [V] | Smooth / Vintage / Hard modes (band counts differ, not published), Shift (band frequencies), Scale (response speed), per-band level and pan; a two-oscillator synth with noise; **MIDI, Auto and Sidechain** modes for the carrier | **Auto mode** (the synth follows the voice): our Tracked; per-band pan → `spread` |
| Arturia Vocoder V ([product page](https://www.arturia.com/products/software-instruments/vocoder-v/overview)) [V, via store listings] | 16 analysis / synthesis bands, 50 – 5,080 Hz; shift and widen all bands; patch-bay rerouting of bands; a two-oscillator synth | Band remapping is the formant control; 5 kHz top + sibilance above it |
| Waves Morphoder, Softube Vocoder, Logic EVOC 20 [U] | Commercial channel vocoders; Softube's has an **Unvoiced** section, Spectral Tilt and Emphasis "for intelligibility on spoken phrases" ([MusicTech review](https://www.musictech.com/reviews/software-instruments/softube-vocoder-review/)) [V, secondary] | `tilt` |
| Surge XT Vocoder (GPL-3; [`VocoderEffect.cpp`](https://github.com/surge-synthesizer/surge/blob/main/src/common/dsp/effects/VocoderEffect.cpp)) [V, code read] | **The incoming audio is the carrier**, the modulator comes from the other input; log-spaced bands from the VSM-201 table (180 – 7,500 Hz); bands in SIMD groups of 4 (count rounded down to a multiple of 4), Q = 20 · (1 + 0.5 · q) (10 – 30); envelope = one-pole on the squared band, carrier × sqrt; per-band gate (default −96 dB); separate modulator range / centre (= formant shift); mix = dry · (1 − w) + 4 · w · sum. **The voiced / unvoiced code is commented out** [V] | SIMD bands; an independent modulator bank position as the formant shift. Its missing V/UV is the gap we fill |
| mda Vocoder ([MOD listing](https://pedalboards.moddevices.com/plugins/aHR0cDovL21vZGRldmljZXMuY29tL3BsdWdpbnMvbWRhL1ZvY29kZXI=)) [V, secondary] | 16 bands (8 at low quality); **Hi Thru** (modulator highs passed through) and **Hi Band** (top band driven differently), Envelope 14 ms – 10 s, Filter Q, Mid Freq, carrier / modulator **swap** | Hi Thru = our sibilance pass; swap = our two key modes |
| Calf Vocoder [U] | Sidechain vocoder (Ardour: Pin Connections → Sidechain → the mic track [V, forum]) | — |
| LSP, x42 [U] | No channel vocoder found | — |
| Image-Line Vocodex ([manual](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/plugins/Vocodex_parameters.htm)) [V] | 5 – 100 bands; noise added to the carrier "helps preserve sibilants"; a low-pass on the carrier pass-through so bright carrier highs do not mask sibilants | Noise in the carrier as a separate amount |

### Research

- **Complex gammatone filterbank with low-delay resynthesis**: V. Hohmann, "Frequency analysis and
  synthesis using a Gammatone filterbank", *Acta Acustica united with Acustica* 88 (3), 433 – 442, 2002
  [V: citation; method via the [pyfar port](https://pyfar.readthedocs.io/en/latest/_modules/pyfar/dsp/filter/gammatone.html)].
  Each band is a cascade of complex one-pole filters; the band's **complex output gives the envelope
  directly (|z|) and the band signal (Re z)**, and per-band phase factors make the bands sum nearly flat.
  This is the bank used here (§2.2).
- **ERB scale**: Glasberg & Moore (1990), ERB(f) = 24.7 · (4.37 f / 1000 + 1) Hz; ERB-number
  21.4 · log10(1 + 0.00437 f) [V: standard formula, also in gfx-octaver's Poly bank].
- **YIN** pitch detector (de Cheveigné & Kawahara 2002), already gfx-dsp `detect.rs` [V].
- **Band-limited oscillators**: PolyBLEP (Välimäki & Huovilainen 2007) for saw / pulse; the closed-form
  sum of harmonics (Moorer 1976, "discrete summation formulae") for an equal-amplitude buzz [U: not
  re-read; textbook].
- DAFx channel-vocoder literature (e.g. Zölzer, *DAFX* ch. on source-filter processing): the channel
  vocoder is the band-envelope special case of cross-synthesis; the STFT version trades latency for
  resolution [I].

### Forum and review feedback: what players love and hate

- **"Not enough highs" / dull output**: vocoding costs top end; the usual advice is a brighter carrier
  ([Sound On Sound](https://www.soundonsound.com/techniques/studio-one-vocoder)) [V, secondary]. Ableton's
  Enhance and Softube's tilt exist for this reason [V]. → `flatten` + `tilt`.
- **Intelligibility**: sibilants and plosives vanish when the carrier has no noise or no highs; Vocodex
  and Softube add noise / unvoiced sections [V]. The trade-off is real: more unvoiced level is clearer
  but less "vocoder" [V, SOS]. → `unvoiced`, `sib`, `noise` as separate amounts.
- **Carrier richness**: string pads, saw stacks and electric pianos work; a thin carrier sounds thin
  [V, SOS / Vocodex]. → Supersaw, chords, flatten.
- **Latency** of FFT vocoders when singing through them live [U: common forum complaint]. → IIR bank.
- **Routing pain**: setting up a sidechain vocoder is the classic DAW frustration (Mixbus / Ardour pin
  dialogs [V, forum]). → a vocoder that needs no routing to sound.
- **Bleed**: on a live mic, everything that leaks into the mic is vocoded; flattening makes it worse
  (a quiet bleed gets a full carrier). → a broadband gate `thresh`.

### What we conclude

1. **Bank**: an IIR complex gammatone bank, 4th order, on the ERB scale, 8 – 40 bands. It reports no
   latency, gives an envelope without a rectifier (|z| of a complex band: its ripple is the band's image
   rejection, −59 dB at the default), and sums within 1.0 – 1.5 dB from 24 bands up when the bands are
   phase-aligned [P: §2.2]. STFT is rejected: a 1,024-point frame is 21 ms at 48 kHz on **every** band,
   consonants included; the bank's lag is 1 – 6 ms there and bounded at 16 ms in the lows.
2. **Carrier**: internal first, because golem routes only the lane's own DI today. The Tracked
   carrier (the voice's own pitch, optionally quantized and harmonized in the song key) is the
   flagship: one voice → robot, or a choir.
3. **Intelligibility**: carrier flattening, V/UV noise substitution, sibilance pass-through, tilt —
   each a separate amount, with defaults that are clear but still sound like a vocoder.
4. **Formant shift** as a remapping of envelopes onto fixed synthesis bands (Korg's and Surge's idea),
   continuous, EXP-capable, no filter redesign.
5. **Key Input / Talk Box / MIDI** specified now, shipped when golem routes another lane or MIDI.

## 2. Design

### 2.1 Signal path (mono; stereo in §2.8)

```
main in ─┬─────────────────────────────────────────────────────────────────── dry ──┐
         │                                                                            │
         ├─(modulator: main, or key in Talk Box)                                      │
         │   ─▶ gate `thresh` (gain G_g) ─▶ analysis bank A (N complex gammatones) ─▶ |z_k|
         │        ├▶ followers `env_att` / `env_rel` ─▶ × G_g ─▶ freeze hold           │
         │        │     ─▶ formant remap ─▶ tilt ─▶ M_k                                │
         │        └▶ fast followers (2 / 10 ms) ─▶ V/UV detector ─▶ u (0 … 1)          │
         │   ─▶ YIN (Tracked only) ─▶ f0, voiced                                       │
         │   ─▶ HPF at `high` (4th order) ─▶ × G_g · `sib` · (0.25 + 0.75 u) ─────┐     │
         │                                                                        │     │
carrier (`source`): oscillators · noise · main · key                             │     │
   ─▶ synthesis bank S (N complex gammatones) ─▶ Re(z_k), |z_k| ─▶ C_k, C̄         │     │
        O_k = Re(z_k) / C̄ · (C̄ / max(C_k, floor))^`flatten`                       │     │
white noise ─▶ noise bank (the bands from 500 Hz up) ─▶ N_k (unit level)         │     │
   wet = G_Σ · Σ_k M_k · [ cos(π a / 2) · O_k + (sin(π a / 2) + `noise`) · N_k ]  │     │
        a = `unvoiced` · u                                  ─▶ + sib path ◀──────┘     │
   ─▶ `spread` (2 / 2) ─▶ equal-power `mix` with dry ◀─────────────────────────────────┘
   ─▶ `output` ─▶ bypass crossfade ─▶ out
```

- **The dry is always the main input**, whatever the source: on a voice lane the dry voice, in Talk Box
  the instrument the block sits on.
- **The noise has its own bank.** Mixed into the carrier before one shared bank, it would pass through
  `flatten`'s per-band gains while they still hold the oscillator's tilt: a 20 dB spit of highs at each
  "s", and dulled highs for 30 ms after it (§2.6).
- Everything is per sample except the formant remap weights and the C̄ statistics, which are updated on
  a control tick of 1 / 1,500 s (gfx-ducker's tick, defined in time so it is the same at every rate).

### 2.2 The filterbank (new gfx-dsp module `filterbank`)

- **Band centres**: `bands` (N, 8 – 40) centres between `low` and `high`, spaced on a scale whose local
  spacing in Hz is **Δ(f) = max(s · E(f), Δ_min)**:
  - E(f) = ERB(f) (`spacing` ERB, the default) or f (`spacing` Log, the analog layout: s is then the log
    of the band ratio).
  - s is solved by bisection so that ∫ df / Δ(f) from `low` to `high` equals N − 1; the centres sit at
    the integer values of that integral (closed form on each side of the knee: linear below it,
    ERB-number or log above).
  - **Δ_min = 40 Hz / w0**: no band is narrower than 40 Hz, so **no band lags by more than 16 ms**
    (below). With the default 24 ERB bands the floor touches the lowest band only (36.5 → 40 Hz);
    with 40 bands, or with Log spacing, it moves the surplus low bands up to where speech is
    understood (Log, 40 bands: 5 bands under 300 Hz instead of 11, 23 above 1 kHz instead of 18 [P]).
  - 100 Hz – 7 kHz is 28.8 ERB [A]: 24 bands are 1.25 ERB apart, 40 bands 0.74 ERB.
- **Each band**: a cascade of four complex one-pole filters, y ← (1 − |a|) · x + a · y,
  a = exp(−2π b / fs + i 2π f_c / fs), state in f64 (gfx-dsp numerics policy).
  - Pole bandwidth **b_k = w0 · Δ(f_k)**, with **w0 = clamp(0.93 − 0.085 · s_E, 0.55, 0.90)**, s_E being
    the spacing in ERB (Log: the spacing at 1 kHz, s · 1,000 / 132.6). The flattest sum needs relatively
    narrower bands when they are few: the best w0 runs from 0.60 at 8 bands to 0.90 at 40 [P,
    `width.py`], and a fixed 0.85 leaves 11.7 dB of ripple at 8 ERB bands and 10 dB at 11 Log bands.
    w0 and Δ_min are solved in the same bisection.
  - **Synthesis bands** carry a fixed complex factor e^(−iφ_k) / |H_k(f_c)|: zero phase and unity gain
    at the centre (Hohmann's phase alignment, without his extra delays). **Analysis bands** only need
    the gain (only |z| is read), scaled so that a sine of amplitude A at f_c gives |z| = A.
  - **Edge trims**: the first and last bands have one neighbour, so the aligned sum is 2.1 – 2.5 dB
    hotter at `low` and `high` than inside [P, `edge.py`]. Their gains are trimmed at design time to
    the inner level (−1.4 … −2.2 dB). Trimming every band the same way does not help (1.35 → 1.66 dB
    at 24 bands): only the two edges are trimmed.
  - **Sum gain G_Σ**: neighbours add partly out of phase at each centre, so the aligned sum sits
    2.5 – 4.2 dB under one band's gain. G_Σ = 1 / RMS of the summed response over the centres and the
    mid-points, computed at design time, puts the flat sum at 0 dB.
- **Why 4th order, and what it gives** [P, `floor.py`, 100 Hz – 7 kHz at 48 kHz; the same within
  0.1 dB at 44.1 and 96 kHz]:

  | Bands | w0 | Sum ripple, ERB / Log | Lag: lowest band · 500 Hz · 1.5 kHz · 3 kHz (ERB) |
  |---|---|---|---|
  | 8 | 0.58 | 4.2 / 4.7 dB | 7.5 · 3.1 · 1.3 · 0.8 ms |
  | 11 – 12 | 0.69 – 0.71 | 2.1 / 3.6 – 4.0 dB | 9 – 10 · 4 · 1.9 · 1.0 ms |
  | 16 | 0.77 | 1.9 / 2.3 dB | 12.2 · 5.4 · 2.4 · 1.3 ms |
  | 24 | 0.82 | 1.35 / 2.0 dB | 15.9 · 7.8 · 3.5 · 1.8 ms |
  | 32 | 0.85 | 1.5 / 1.4 dB | 15.9 · 11.0 · 4.6 · 2.3 ms |
  | 40 | 0.87 | 1.4 / 1.4 dB | 15.9 · 13.8 · 5.6 · 3.1 ms |

  - Neighbouring bands are 14 – 17 dB down at a band's centre from 16 bands up (the next ones ≈ −28 and
    ≈ −38 dB), 20 – 24 dB at 8 – 12 bands [P for w 0.85: −13 … −14 dB; A for the law].
  - A 2nd-order bank either overlaps so much that neighbouring bands carry the same envelope
    (neighbour −1.3 … −3.9 dB) or ripples by 5.7 – 6.0 dB when summed, at any width [P]. Hohmann's
    per-band delays could take the ripple lower at the cost of a few ms of reported latency: not
    taken. 1.0 – 1.5 dB on a carrier whose spectrum is reshaped band by band anyway is not a fault a
    listener can point to; under 16 bands the gaps between bands are the vintage sound.
- **Lag** (not a reported latency). A band's envelope follows the modulator's with the band's group
  delay, **τ_k = 4 / (2π b_k)** [A; the impulse response's envelope peaks at 3 / (2π b_k), the figure the
  first draft quoted]. See the table: 1 – 6 ms from 1.5 kHz up, 3 – 14 ms at 500 Hz, at most 15.9 ms in
  the lowest band. Without the 40 Hz floor the lowest band would lag 28 ms at 40 ERB bands and
  42 – 68 ms at 24 – 40 Log bands [P].
  - **Intelligibility** is carried above 1 kHz, where the lag is 1 – 6 ms: consonants stay on time. The
    ear's own filter at 100 Hz (a 1-ERB gammatone) is as slow as ours (18 ms [A]).
  - **Timing on stage**: a syllable is placed by its vowel onset (300 – 800 Hz): 5 – 8 ms late at
    16 – 24 bands, 11 – 14 ms at 32 – 40. That is 2 – 5 m of air, on top of the interface's latency.
    For tight rhythmic parts stay at 24 bands or under; 32 – 40 bands are for slow, speech-precise
    parts. It is a softened attack of the body, not an echo (everything stays inside the 30 ms fusion
    window).
  - The lag is on the gains only: a steady carrier is not delayed, and a carrier note change passes
    the synthesis band in the same τ_k.
- **Envelope**: |z_k| of the analysis band, one sqrt per band, then a peak-style follower: attack
  `env_att`, release `env_rel` (one-pole each, gfx-dsp `envelope::PeakFollower`).
  - There is no rectifier, but |z| is not ripple-free by nature: a real input leaves an image at −f_c,
    rejected by (1 + (2 f_c / b)²)^−2. That is −59 dB in the default's lowest band, −22 dB with 8 bands,
    −10 dB with 8 bands from `low` 50 Hz [P]: ripple at 2 f_c, about ±3 dB before the follower in the
    worst corner and a fraction of a dB after it; nothing at the default.
  - What remains is the beat between two harmonics in one band (at f0): 0.5 dB peak-to-peak at
    `env_rel` 40 ms, 1.0 dB at 20 ms, for a 100 Hz voice [P, `level.py`]. With Tracked the carrier
    shares that f0 and the beat only tints the harmonics; with Drone it is the roughness every channel
    vocoder has, and `env_rel` is its control.
  - Defaults `env_att` 3 ms, `env_rel` 40 ms: the attack is shorter than any band's lag below 1.5 kHz,
    so the filters set the rise; 40 ms keeps a syllable's end crisp with 0.5 dB of ripple.
- **Synthesis bank S**: the same centres, fed with the carrier. Its complex output gives the band signal
  Re(z_k) **and** the carrier's own band envelope |z_k| for `flatten` at no extra filter cost.
- **`width`** acts on the pole bandwidths: analysis b_k · `width` / 100 %, synthesis
  b_k · min(`width`, 100 %) / 100 %.
  - Above 100 % only the **analysis** widens: the envelope is smeared across neighbouring bands (softer,
    less articulate) and the lag shrinks, while the synthesis sum stays flat. Widening the synthesis
    bands too makes aligned neighbours cancel: at 130 % the sum loses 8.9 dB and ripples by 3.9 dB, at
    200 % it loses 18 dB and ripples by 23 dB [P].
  - Below 100 % both narrow: resonant, whistly bands with gaps (ripple 3.7 dB at 70 %, 10.5 dB at 50 %
    [P]) and a proportionally longer lag (32 ms in the lowest band at 50 %). G_Σ and the edge trims are
    recomputed, so the level does not move.
- **Parameter changes**: `bands`, `spacing`, `low`, `high` and `width` are setup values. Any change
  designs a new coefficient set, and the old and new banks run together for 40 ms, outputs
  crossfaded. One mechanism instead of two: gliding complex poles would also need their phase factors,
  trims and G_Σ re-solved on every tick.
  - The design runs where `set_param` runs, on the audio thread at a block boundary: it must be bounded
    and allocation-free (arrays sized for 40 bands at `prepare`): one bisection (≤ 40 steps, closed
    form), then the summed response at the 2 N − 1 centres and mid-points for the trims and G_Σ (each
    point sums its 12 nearest bands; the others are under −60 dB). Target ≤ 50 µs at 40 bands (bench);
    if it does not hold, the design is spread over the control ticks before the crossfade starts.
  - A new design starts only when the previous crossfade has ended; the latest values win. They are
    not EXP targets.

### 2.3 Gate (bleed)

- Broadband sliding maximum of the modulator (gfx-dsp `envelope::SlidingMax`), over one period of the
  detector's lowest note (1 / f_min of `register`: 17 / 11 / 7 / 13 ms): a 10 ms window would ripple
  between the pulses of a voice under 100 Hz (gfx-gate's lesson). Compared with `thresh` with 6 dB of
  hysteresis. Closed → the gate gain G_g falls over `env_rel` and rises over `env_att` (no extra time
  constants to learn). G_g multiplies the band envelopes **and the sibilance path**: with the gate
  closed nothing of the modulator reaches the output.
- −90 dBFS = off (gfx-gate's range). Default −60 dBFS: an open mic's room and bleed sit below it on a
  typical stage [U: to verify on a recorded stage vocal mic, the ducker's missing DI].

### 2.4 Formant remap, freeze, tilt

- **Formant**: carrier band k (centre f_k) takes the modulator envelope read at f_k · 2^(−`formant`/12),
  interpolated linearly along the band scale (§2.2's integral: a fractional band index) between the
  two nearest analysis bands. +12 st moves every formant
  up an octave (a smaller, "chipmunk" vocal tract), −12 st down. Points outside the analysis range read
  the edge band, faded by 6 dB per band beyond it. Weights per tick; envelopes stay per sample; the
  filters never move, so `formant` is EXP-safe and click-free.
  - It moves the envelope **as the bank sees it**. Under about 600 Hz the bands are narrower than a
    voice's harmonic spacing, so the "envelope" there is the harmonics themselves: on an internal
    carrier that does not matter (the carrier has its own harmonics), with Self it leaves holes. Self is
    a coarse formant colour, best with few, wide analysis bands (§4); the precise formant shifter is
    gfx-formant.
- **Freeze**: a second, slow copy of the envelopes (one-pole, 60 ms) runs all the time. On `freeze`, M_k
  crossfades over 30 ms to that snapshot and holds it; release crossfades back over 30 ms. The slow copy
  keeps a transient or a consonant out of the snapshot (golem's spectral freeze averages its first frames
  for the same reason [V: `docs/spectral.md`]). The carrier keeps playing: a held vowel on a moving chord.
  The gate is bypassed for the envelopes while frozen (the singer stops, the vowel stays); the
  sibilance path stays live and gated, so the singer can sing over the held vowel at `mix` < 100 %.
- **Tilt**: M_k · (f_k / 1 kHz)^(`tilt` / 6.02), i.e. `tilt` dB per octave around 1 kHz.

### 2.5 Carriers (`source`)

| `source` | Carrier | Works in golem today |
|---|---|---|
| **Tracked** | Oscillators following the modulator's pitch (YIN), `chord` stacked on it | Yes |
| **Drone** | Oscillators holding a diatonic chord on `degree` of the song's `key` / `scale` | Yes |
| **Noise** | White noise (deterministic seed): a whisper | Yes |
| **Self** | The main input itself: with `flatten` 100 % and `formant` ≠ 0, a formant shifter | Yes |
| **Key Input** | The key bus is the carrier, the main the modulator (Ableton's External) | Needs `key_from` (§6) |
| **Talk Box** | The main is the carrier, the key the modulator (TAL's "SC M") | Needs `key_from` (§6) |
| *MIDI* (phase 2, appended) | Polyphonic oscillators played by VST3 note events | Needs MIDI routing (§6) |

- **Tracked**:
  - Detector: gfx-dsp `detect::PitchDetector` (YIN at ~12 kHz, every ~2.7 ms) through `prepare_range`,
    on the range of **`register`** (gfx-golem's id and its three ranges, one option appended):
    Low 60 – 500 Hz · Mid 90 – 800 Hz · High 150 – 1,200 Hz, harmonic lock off (voices), and
    **Instrument** 75 – 1,500 Hz with the harmonic lock (a guitar, gfx-pitch's detector: 97 % pitch-class
    agreement on the lead DIs [V: gfx-pitch status]). The window follows the range, so a voice in Mid
    is read in ≈ 22 ms instead of 27, with fewer octave candidates. On the modulator after the gate.
    Not gfx-ringmod's phase-locked tracker: a vocoder's carrier does not have to null against the
    input, so frequency tracking is enough.
  - **Octave guard**: a ×2 / ÷2 jump must last 40 ms to be believed (gfx-golem's `OCTAVE_CONFIRM_MS`,
    also in gfx-correct's design: with a third user the rule moves into `detect`). An octave flip of
    the carrier is the one tracking error everybody hears.
  - Root: `quantize` Off = the measured f0 (smoothed 10 ms, plus `glide`), so vibrato and slides pass
    through; Chromatic = nearest semitone (the robot step); Key = nearest note of `key` / `scale`.
    Both quantized modes hold a note until the pitch is clearly in the next (`NoteTracker`'s
    confirmation and hysteresis: a vibrato across a boundary does not trill). Then `octave` × 12
    semitones.
  - Chord: intervals from `detect::harmony_interval` on the root's pitch class in `key` / `scale`, added
    to the (unquantized) root, as gfx-pitch's harmony does: a sung bend bends the chord. Voices are
    scaled by 1 / √n.
  - **Within a phrase**, unvoiced and silent frames hold the last note (YIN has no pitch on an "s"; the
    V/UV path supplies the noise).
  - **At a phrase start** (no voiced frame for 80 ms), YIN needs about two periods of the range's lowest
    note plus a hop: ≈ 25 ms in Mid, ≈ 30 ms in Instrument [A]. Until the first voiced frame the
    oscillators are **pending**: held on the last note at −12 dB, the noise bank carrying the onset at
    `unvoiced`'s amount. The first voiced frame sets the frequency at once (no `glide` into the first
    note of a phrase) and the oscillators rise to full level in 5 ms. A wrong note at full level for
    25 ms is a chirp; the same note 12 dB down under a breath of noise is an attack.
- **Drone**: root = `key` + the `degree`-th note of `scale`, in octave 3 (C3 – B3) + `octave`; chord
  diatonic on that degree. In a five-note scale degrees 6 and 7 are degrees 1 and 2 an octave up.
  golem changes `degree` (or `key`) per scene: verse on vi, chorus on IV.
- **Oscillators** (new gfx-dsp module `osc`), `wave`:
  - **Saw**, **Pulse** (50 %): PolyBLEP, phase in f64.
  - **Buzz**: equal-amplitude harmonics up to 0.45 fs by the closed-form sum, the last harmonic faded by
    its fractional part so harmonics enter and leave smoothly as the pitch moves; scaled by 1 / √n
    (n harmonics), with the series limit taken where the denominator vanishes. The flattest carrier,
    the most intelligible, the harshest.
  - **Supersaw**: 7 PolyBLEP saws per note, detuned symmetrically by up to `detune` cents, fixed start
    phases from the seed (deterministic).
  - A frequency change keeps the phase (no click); a new note in `chord` fades in over 5 ms.
  - `flatten` raises whatever is in a band by up to 40 dB, aliases included: the oscillators' inharmonic
    residue is checked **after** the vocoder (§5). If two-point PolyBLEP fails it at `octave` +2, the
    saws take gfx-crusher's BLEP kernel.
- **Noise**: one xorshift white noise source with a fixed seed. As a `source` (the whisper) it feeds
  the synthesis bank; for `unvoiced` and `noise` it feeds the noise bank (§2.6).
- **Carrier level**: there is no separate AGC. The band gains divide by **C̄**, the power mean of the
  carrier's band envelopes smoothed over 200 ms (per tick), so the output level follows the modulator,
  not the carrier's level. 1 / C̄ is clamped to +24 dB above a −18 dBFS carrier, and a carrier under
  −70 dBFS is treated as absent: a silent carrier (Key Input with no key) stays silent, and its noise
  floor is not vocoded up. The internal oscillators have a known level.

### 2.6 Intelligibility: flatten, V/UV, sibilance, noise

- **`flatten`** (Ableton's Enhance as a continuous amount): the carrier's band k is multiplied by
  g_k = (M_k / C̄) · (C̄ / max(C_k, C̄ · 10^(−40/20)))^`flatten`, where C_k is the carrier's band envelope
  (|z_k| of S, followed 5 ms up / 30 ms down: it rides the peaks of the carrier's pitch pulses, so their
  fine structure survives) and C̄ its power mean over the bands.
  - 100 %: g_k = M_k / C_k. Both are peak envelopes of the same kind of band, so every band of the
    output has the modulator's level by construction, whatever the carrier's spectrum: a saw's
    −6 dB/oct or a guitar's dark top no longer darken the voice. The carrier gives pitch and texture
    only.
  - 0 %: the vintage response, carrier spectrum × modulator spectrum, normalised by C̄ only.
  - The −40 dB floor keeps empty carrier bands empty (a sine carrier's missing highs are not raised from
    its noise floor); `noise` fills them if wanted.
- **V/UV detector**: u rises toward 1 when the modulator's energy above 3 kHz (the bands from 3 kHz up
  plus the sibilance path) exceeds its energy below 1 kHz, with 3 dB of hysteresis, and the gate is
  open.
  - The two energies come from the analysis bands' raw |z_k|², through their **own** followers (2 ms up,
    10 ms down), before the gate gain, freeze, remap and tilt: read from M_k they would lag by
    `env_rel`, up to 2 s.
  - **YIN's clarity is not part of the decision.** YIN reads the input low-passed at 2.4 kHz over a
    27 ms window [V: `detect.rs`]: going from a vowel into an "s" it still sees the vowel's tail and
    reports "voiced" for 10 – 25 ms, and a "t" or "k" burst is over before it changes its mind. The band
    balance answers in 2 – 3 ms. It is also what a zero-crossing rate measures, more robustly, so no
    third cue is planned. Side benefit: V/UV works with every source, and YIN runs for Tracked only.
  - u slews 3 ms up, 20 ms down. Used twice:
  - **`unvoiced`**: with a = `unvoiced` · u, the carrier's bands fade by cos(π a / 2) and the noise bands
    rise by sin(π a / 2): an equal-power crossfade (the two are uncorrelated), the EMS switch as an
    amount, 100 % = a full switch.
  - **`sib`**: the modulator above `high` (4th-order Linkwitz-Riley high-pass at the top band's centre)
    is added to the wet at `sib` · (0.25 + 0.75 u) · G_g: full during "s", "t", "f"; −12 dB of the voice's
    own air during vowels; nothing when the gate is closed.
    - The top band and the pass-through overlap from `high` to about 1.1 × `high` (the band is −8 dB
      there at 24 bands [P]). The vocoded band and the voice's own highs are different signals, so
      they add in power: within about ±1 dB across the join, no gap and no comb.
    - Timing: the pass-through is the voice itself, undelayed; the top bands follow 0.8 – 1.3 ms later
      plus `env_att`. Nothing to align.
    - With Self the two **are** the same signal with different phases, and the overlap combs over a
      fraction of an octave: use `sib` 0 there (§4).
- **Noise bank**: the same band set from 500 Hz up (≈ 18 of 24 bands), fed with white noise, each band
  scaled at design time to the level a flattened carrier band has (N_k: unit envelope). It needs no
  `flatten` and no follower, so nothing has to settle when u moves: no spit at an "s", no dulled vowel
  after it. It runs only while `unvoiced` or `noise` is above 0 and the source is not Noise.
- **`noise`**: the noise bands added at `noise` × M_k, always (Vocodex's noise): breath under the
  vocoder, and highs for a dark carrier at `flatten` 0.

### 2.7 Mix and output

- **`mix`**: **equal-power crossfade** of dry and wet, dry · cos(π m / 2) + wet · sin(π m / 2) — the law
  of gfx-shimmer and gfx-pitch, right here because the wet and the dry are uncorrelated with an
  internal carrier: a linear crossfade would dip by 3 dB at 50 %.
  - With Self (and Talk Box, where the dry is the carrier) the wet is the dry through a dispersive
    bank: at 50 % the sum is 1.2 – 2.1 dB under the dry [P] and combed band by band. Those two sources
    are meant for `mix` 100 %.
- **Level**: with G_Σ and `flatten` 100 %, the wet's RMS is the modulator's (100 Hz – 7 kHz) plus the
  followers' peak bias: −0.2 … +2.9 dB over 8 – 40 bands, saw and noise carriers, at `env_rel` 40 ms
  [P, `level.py`]. A fixed −1.5 dB centres it: ±1.5 dB from 16 bands up (bench).
  - The bias grows with the release (+0.4 dB at 20 ms, +3.6 dB at 400 ms [P]): a long release holds
    every peak, which is what it is for.
  - At `flatten` 0 the level depends on how the carrier's tilt lines up with the voice's: +3 … +9 dB
    with a saw under a pink modulator, +0.5 dB with noise [P]. The bench measures it per `wave`; a
    static trim per wave, scaled by (1 − `flatten`), is added if the sweep moves the level by more
    than 3 dB.
  - `output` −18 … +18 dB after the mix.
- **Bypass**: `bypass` (VST3 `kIsBypass`), 10 ms gain crossfade, as the coded effects ship. No tail beyond
  the envelopes' release.

### 2.8 Stereo, layouts, meters

- **Layouts**: 1 / 1 first (a vocoder is not stereo by nature; a voice lane stays mono), 2 / 2 second.
  Each has a key auxiliary bus of 1 or 2 channels (gfx-gate's wrapper).
- **2 / 2**: the modulator is (L + R) / 2 (one analysis bank). The carrier is stereo when it is (a stereo
  key, a stereo main in Talk Box, Supersaw's voices alternated L / R): two synthesis banks then, one
  when both sides are equal. `spread` pans even bands left and odd bands right by up to ±100 %
  (VocalSynth's per-band pan, as one amount; constant-power, so the mono sum does not change). At 0 the
  wet is the same on both sides.
- **Meters** (read-only, golem's nih-plug fork): `f0` (Hz, the tracked fundamental, 0 = none — golem can
  show what the vocoder hears) and `state` (1 while the gate is open). 

### 2.9 gfx-dsp reuse and what is new

- **Reused**: `detect::{PitchDetector (prepare_range), NoteTracker, harmony_interval, midi_of}`;
  `envelope::{PeakFollower, SlidingMax}`; `svf::Svf` (the sibilance Linkwitz-Riley high-pass as two
  Butterworth sections); `smooth::{OnePole, LinearRamp}`, `exp_smooth` (EXP parameters); gfx-gate's
  aux-bus wrapper; gfx-golem's `REGISTERS` and octave guard (both move to `detect`: gfx-golem,
  gfx-correct and this crate use them).
- **Not reused**: the phase-vocoder shifter (a pitch shifter, see the introduction); gfx-ringmod's
  phase-locked tracker (not needed); gfx-tuner's zoom stage and `candidates()` (cents precision and a
  strings prior are not needed for a carrier).
- **New**:
  - **`filterbank`**, the module gfx-dsp's README already plans under that name ("ERB complex IIR
    filterbank", for gfx-octaver's Poly): `GammaBank`, a complex gammatone bank of order 4 — the scale
    with its 40 Hz floor, the width law, phase factors, edge trims and G_Σ (§2.2), analysis and
    synthesis instances, SIMD-friendly (bands in lanes of 4 / 8, f64 state), a bounded design call,
    the 40 ms bank crossfade. One name for one thing: gfx-octaver's private 28-band bank (complex
    one-poles, 1 ERB) can move to it later, as a separate change with its own listening.
  - **`osc`**: one f64 phase accumulator type, and two oscillator families on it: PolyBLEP saw and
    pulse, closed-form buzz, supersaw (this crate); band-limited wavetables for sine, triangle and
    square (gfx-ringmod's design, which names the same module). Whichever crate is coded first creates
    the module with the phase type; the other adds its family. Deterministic start phases.
    gfx-octaver's Synth could adopt it later.
  - **`noise`**: xorshift white noise with a fixed seed (planned in gfx-dsp's README, not in `src/` yet
    [V]); whichever crate needs it first writes it.
- **`vocoder.rs` → `pv_shift.rs`**, as its own commit before this crate is coded: the module path
  changes (`gfx_dsp::vocoder::PvShifter` → `gfx_dsp::pv_shift::PvShifter`; one `use` in
  `gfx-pitch/src/lib.rs`, comments in gfx-pitch and gfx-bench, mentions in the READMEs of gfx-glitch,
  gfx-formant and gfx-correct). No parameter, id or sample changes: gfx-pitch's bench must null
  bit-exactly against the previous build, which is the whole proof. Left alone, a suite with a channel
  vocoder and a module called `vocoder` that is not one will mislead every later reader.
- **Key and MIDI in the `Effect` trait**: gfx-ducker's design adds `process_with_key` (default:
  `process`) and a `key` signal to gfx-bench `Options`; this crate needs the same. Phase 2 adds
  `note_on(note, velocity, offset)` / `note_off` to the trait (default: ignored) and note lists to the
  bench.

### 2.10 Latency, CPU

- **Zero reported latency.** No lookahead, no frames, no oversampling (nothing here is nonlinear except
  the oscillators, which are band-limited by construction). The bank's lag is inherent, bounded at
  16 ms and stated in §2.2.
- **CPU** at 48 kHz per mono channel [A, to be measured]:
  - One band of one bank: four complex multiply-adds ≈ 16 mul + 16 add, plus one sqrt for the envelope.
    Analysis + synthesis + follower + gains ≈ 80 flops per band per sample; a noise band ≈ 35.
  - 24 bands: ≈ 1,900 flops, + ≈ 600 for the noise bank (18 bands) ≈ 330 – 520 ns scalar f64,
    ≈ 130 – 200 ns with 4-lane f64 SIMD. 40 bands: × 1.7.
  - YIN: 60 – 80 ns, Tracked only (measured in gfx-pitch [V]).
  - Oscillators: ≈ 5 ns per PolyBLEP saw [A]; Supersaw triad (21 saws) ≈ 100 ns.
  - Default (24 bands, Tracked Unison Saw, `unvoiced` 50 %): ≈ 250 – 350 ns with SIMD, ≈ 1.5 % of a
    core. Worst (40 bands, Supersaw Ninth, 2 / 2 with a stereo carrier): ≈ 1 – 1.4 µs. Bench bounds
    in §5.

## 3. Parameters

A new block: no catalogue ids. Shared ids keep their contract meaning: `mix` (law stated in §2.7),
`env_att` / `env_rel` (1 – 100 ms / 20 – 2,000 ms), `state`, `bypass`. gfx-pitch's `key` and `scale`
(same options, same order), gfx-octaver's `glide` (0 – 200 ms), gfx-gate's `thresh` (−90 – 0 dBFS),
gfx-golem's `register` (its three options in its order, Instrument appended), golem's spectral
freeze `freeze`, the suite's `output`. All ranges linear (contract). No default depends on another
parameter. Enums only grow at the end; integers are `IntParam`s.

### Front panel

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `source` | Carrier | e | Tracked · Drone · Noise · Self · Key Input · Talk Box | Tracked | crossfade 30 ms | | What the voice is imposed on (§2.5). Tracked = oscillators following the voice's pitch; Drone = a chord in the song key; Noise = whisper; Self = the input (formant shifter); Key Input / Talk Box need a routed key |
| `chord` | Chord | e | Unison · Octave · Fifth · Triad · Seventh · Ninth | Unison | 5 ms per voice | | Tracked and Drone: notes stacked diatonically in `key` / `scale`. Octave = root + octave below; Fifth = root + fifth + octave below |
| `formant` | Formant | f | −12 – +12 st | 0 st | EXP | ✓ | Moves the voice's formants without moving the pitch: + smaller and brighter, − bigger and darker |
| `env_rel` | Release | f | 20 – 2,000 ms | 40 ms | EXP | ✓ | How long a band rings after the voice leaves it. 20 – 60 ms: articulate; 100 – 300 ms: smooth, legato; seconds: pad-like |
| `sib` | Sibilance | f | 0 – 100 % | 50 % | EXP | ✓ | The voice's own highs above the bank, full on "s / t / f", −12 dB during vowels, gated with the voice. 0 = pure vocoder; 100 % = maximum clarity |
| `mix` | Mix | f | 0 – 100 % | 100 % | EXP | ✓ | Equal-power crossfade, dry (the main input) to vocoded |
| `freeze` | Freeze | b | off / on | off | 30 ms | | Holds the current vowel; the carrier keeps playing. Foot switch (momentary or latched in golem) |

### Expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `bands` | Bands | i | 8 – 40 | 24 | bank crossfade 40 ms | | 10 – 16: vintage, choral; 24: clear; 32 – 40: speech-precise. Setup value |
| `spacing` | Spacing | e | ERB · Log | ERB | bank crossfade | | ERB puts resolution where speech is understood (1 – 4 kHz); Log is the analog layout |
| `low` | Low | f | 50 – 500 Hz | 100 Hz | bank crossfade | | Centre of the lowest band |
| `high` | High | f | 2,000 – 12,000 Hz | 7,000 Hz | bank crossfade | | Centre of the highest band; the sibilance pass starts here |
| `width` | Width | f | 50 – 200 % | 100 % | bank crossfade | | Band width relative to the flattest one for this band count. < 100 %: analysis and synthesis narrow, resonant, whistly, slower; > 100 %: the analysis alone widens, the vowels smear, softer and faster |
| `env_att` | Attack | f | 1 – 100 ms | 3 ms | 20 ms | | Envelope rise. 1 – 5 ms keeps consonants; slower softens |
| `thresh` | Gate | f | −90 – 0 dBFS | −60 dBFS | 20 ms | | Below it the modulator is treated as silence (mic bleed), sibilance path included. −90 = off |
| `flatten` | Flatten | f | 0 – 100 % | 100 % | EXP | ✓ | Removes the carrier's own spectral tilt so the voice's spectrum comes through. 0 = vintage (carrier × voice) |
| `unvoiced` | Unvoiced | f | 0 – 100 % | 50 % | 20 ms | | Carrier replaced by noise during unvoiced sounds (the EMS switch), and at a phrase start until the pitch is known |
| `noise` | Noise | f | 0 – 100 % | 0 % | EXP | ✓ | Noise always added to the carrier: breath, and highs for a dark carrier |
| `tilt` | Tilt | f | −6 – +6 dB/oct | 0 dB/oct | EXP | ✓ | Output brightness around 1 kHz |
| `wave` | Wave | e | Saw · Pulse · Buzz · Supersaw | Saw | crossfade 20 ms | | Internal oscillators. Buzz = flattest, most intelligible; Supersaw = the choir |
| `detune` | Detune | f | 0 – 50 cents | 15 cents | 20 ms | | Supersaw spread |
| `register` | Register | e | Low · Mid · High · Instrument | Mid | — | | Tracked: the range the pitch is searched in. Low 60 – 500 Hz, Mid 90 – 800 Hz, High 150 – 1,200 Hz (voices); Instrument 75 – 1,500 Hz (guitar). golem sets it from the lane's role at insert |
| `quantize` | Quantize | e | Off · Chromatic · Key | Off | — | | Tracked root: as sung · nearest semitone (robot steps) · nearest note of the key |
| `glide` | Glide | f | 0 – 200 ms | 0 ms | — | | Tracked root portamento (gfx-octaver's id and range) |
| `octave` | Octave | i | −2 – +2 | 0 | 5 ms | | Carrier register, Tracked and Drone. Drone's root sits in octave 3 at 0 |
| `degree` | Degree | i | 1 – 7 | 1 | 5 ms per voice | | Drone: the scale degree the chord stands on (five-note scales: 6 and 7 = 1 and 2 an octave up). golem changes it per scene |
| `key` | Key | e | C · C# · D · Eb · E · F · F# · G · Ab · A · Bb · B | C | — | | Song key (gfx-pitch's id); golem sets it per song |
| `scale` | Scale | e | gfx-pitch's 10 scales, its order | Major | — | | Chords, Key quantize, Drone |
| `spread` | Spread | f | 0 – 100 % | 0 % | 20 ms | | 2 / 2 only: even bands left, odd bands right |
| `output` | Output | f | −18 – +18 dB | 0 dB | 20 ms | | After the mix |
| `bypass` | Bypass | b | off / on | off | 10 ms | | VST3 `kIsBypass` |

Read-only: `f0` (0 – 1,500 Hz, 0 = none), `state` (0 / 1, gate open).

Notes:
- **Source and chord are independent**, so no default moves with the source: `chord` is ignored by
  Noise, Self, Key Input and Talk Box; `degree` by everything but Drone; `register` / `quantize` /
  `glide` by everything but Tracked. Their defaults are the neutral choice in their own mode.
- **`register`** is the one value that differs per lane: Mid is the plugin's default (a voice), and golem
  sets Instrument at insert on guitar and bass lanes (`init`), as the contract provides.
- **`bands` is an integer setup value**: changing it mid-song is click-free (crossfade) but not an
  EXP or scene parameter. The same for `spacing`, `low`, `high`, `width`.
- **MIDI**, when it comes, is appended to `source` (rank 6) with no other parameter (it uses `wave`,
  `detune`, `octave`, `glide`). v1 declares no MIDI input (§6).

## 4. Applications

Starting settings; everything not named is at its default. Preset names never carry artist or brand names.

| Use | Settings | What matters |
|---|---|---|
| **Robot voice** (voice lane) — "Robot Talk" | Tracked, Unison, `quantize` Chromatic, Saw, `bands` 24, `sib` 40 % | The voice's own melody, stepped; intelligible because of `flatten` and `sib` |
| **One-voice choir** — "Choir of One" | Tracked, Triad, `quantize` Key (golem sets `key` / `scale`), Supersaw, `env_rel` 120 ms, `bands` 20 | A three-part harmony that follows the singer in key, no keyboard |
| **Monotone machine** — "Monotone" | Drone, Unison, `octave` −1, Buzz, `bands` 32, `sib` 60 % | A fixed-pitch, harsh, fully intelligible voice |
| **Song-key pad under a vocal** — "Key Pad" | Drone, Seventh, Supersaw, `env_rel` 400 ms, `mix` 35 %, scenes move `degree` | The dry voice stays, a vocoded pad sings its vowels on the song's chords |
| **Frozen vowel** — "Hold the Ahh" | Drone, Triad, Supersaw, `env_rel` 200 ms; `freeze` on a foot switch | Sing "aah", stomp, keep a vowel pad while singing over it with `mix` 50 % |
| **Whisper** — "Whisper Wind" | Noise, `bands` 32, `sib` 70 %, `env_rel` 30 ms | A breathy, pitchless voice; good for spoken intros |
| **Formant colour** — "Small Voice" / "Big Voice" | Self, `formant` +4 / −4 st, `flatten` 100 %, `sib` 0 %, `unvoiced` 0 %, `bands` 20, `width` 160 %, `mix` 100 % | A coarse change of the voice's size at the same pitch. Few, wide analysis bands: narrow ones would follow the harmonics, not the formants (§2.4). The clean formant shifter is gfx-formant |
| **Synth guitar** (guitar lane) — "Synth Guitar" | Tracked, `register` Instrument, Fifth, Supersaw, `env_att` 1 ms, `env_rel` 150 ms, `bands` 16, `sib` 0 %, `unvoiced` 0 % | The guitar's attack and decay shape a synth that follows single notes |
| **Vintage strings vocoder** — "Strings Vocoder" | Tracked or Drone, Triad, Supersaw, `bands` 11, `spacing` Log, `width` 130 %, `flatten` 0 %, `unvoiced` 20 % | The few-band, smeared, choral sound of the late-70s keyboard vocoders (4 dB of gaps between the bands is part of it) |
| **Talk box guitar** (phase 2 routing) — "Talk Box" | Talk Box on the guitar lane before the amp, key = the vocal lane's DI, `bands` 24, `unvoiced` 60 %, `sib` 40 %, `mix` 100 % | The guitar speaks through the singer's mouth shapes; the amp after it |
| **Keys vocoder** (phase 2 routing) | Keys lane: Talk Box, key = the voice lane; or voice lane: Key Input, key = the keys lane | The classic synth-played vocoder |
| **MIDI vocoder** (phase 2) | Voice lane, MIDI from the keys lane's controller, Supersaw | Chords played on the keyboard, sung by the voice, without the keys lane's sound |

## 5. Bench suite (on top of gfx-bench's common checks)

The common checks (clicks on parameter steps, silence and denormals after a burst, determinism by strict
null, latency measured = 0, four sample rates, CPU) run **with a voice DI as modulator** and, for Key
Input / Talk Box, with a keyed signal (gfx-bench `Options.key`, from gfx-ducker's design). Without a
modulator the wet is silence and every check would pass trivially.

| Check | Method | Target |
|---|---|---|
| Flat sum | All M_k forced to 1 (test hook), white-noise carrier, `flatten` 0; spectrum between 1.3 · `low` and `high` / 1.3, ERB and Log | Ripple ≤ 1.7 dB at 24 – 40 bands (Log 24: ≤ 2.3 dB), ≤ 2.5 dB at 16 – 20, ≤ 5 dB at 8 – 12 [P: §2.2]; mean within ±0.3 dB of 0 dB (G_Σ) |
| Bank edges | Same, at the first and last centres | Within ±0.5 dB of the inner mean (edge trims) [P: +2.1 … +2.5 dB untrimmed] |
| Band selectivity | Sine at a band centre | Neighbours ≤ −13 dB, next ≤ −25 dB [P] |
| Lag | A tone at each band centre, amplitude-modulated at 10 Hz: delay of M_k's modulation against the input's, `env_att` 1 ms, short release | ≤ 4 / (2π b_k) + 1.5 ms per band; ≤ 17.5 ms in every band at every `bands`, `spacing`, `low` at `width` 100 % |
| Bank design | Time of one design call at 40 bands, on the bench machine | ≤ 50 µs, no allocation (else spread over ticks, §2.2) |
| Self identity | Self, `flatten` 100 %, `formant` 0, `sib` 0, voice DI | Third-octave envelope of out vs in within ±3 dB from `low` to `high` |
| Level | Pink-noise modulator, Saw carrier, `flatten` 100 %, default release | Wet RMS = modulator RMS (band-limited `low` – `high`) ± 1.5 dB at 16, 24, 40 bands, ± 2.5 dB at 8 |
| Level vs `flatten` | Same, `flatten` swept 100 → 0 %, each `wave` and Noise | Reported; > 3 dB of change → the per-wave trim of §2.7, then ≤ 2 dB |
| Flatten | Modulator pink noise; carrier saw (−6 dB/oct) vs buzz (flat) | Output third-octave spectra within ±2 dB of each other at 100 %; differ by the carrier tilt at 0 % |
| Carrier purity | Tracked forced to 1 kHz and 4 kHz (`octave` +2), each `wave`, `flatten` 100 %, pink modulator | Inharmonic components ≤ −60 dB under the strongest partial after the vocoder |
| Formant | Synthetic vowel (two formants) as modulator, `formant` +12 | Formant peaks move up one octave ± 1 band |
| V/UV | Voice DIs (male, female; to record if missing) with hand-labelled fricatives and plosive bursts | u ≥ 0.5 within 6 ms of each labelled "s / sh / f / t / k" onset on ≥ 90 % of them; u < 0.5 on ≥ 97 % of vowel frames. Under 90 %: add YIN's aperiodicity as a second way in, never as a condition |
| V/UV edges | Synthetic vowel → noise → vowel, `unvoiced` 100 %, Saw, `flatten` 100 % | No band's output more than 2 dB over its steady level at either transition; no click |
| Sibilance | Same DIs | Output energy above 4 kHz during labelled "s" ≥ −6 dB of the dry at `sib` 50 %; with the gate closed, the sibilance path ≤ −90 dBFS |
| Tracking | Tracked, Unison, voice (Mid) and lead-guitar (Instrument) DIs | Carrier within 10 cents of the offline YIN on ≥ 95 % of voiced frames; **no octave flip** on the DIs; at phrase starts the pending carrier is 12 dB (± 1) under the steady level until the first voiced frame, then on the note within 5 ms |
| Quantize | Sung note with a ±60-cent vibrato across a semitone boundary, Chromatic | One note, no trill |
| Chords | Tracked Triad `quantize` Key on a sung scale | Every carrier note in `scale` on 100 % of quantized frames |
| Edges | `freeze` on / off, `source` and `wave` changes, `bands` 8 ↔ 40, `low` / `high` / `width` steps and a continuous `low` sweep, `formant` steps, `quantize` Chromatic note changes | No click above the common bound (−50 dB above 10 kHz vs signal) |
| Gate | Voice DI with bleed (−70 dBFS pink) in the gaps | Wet ≤ −90 dBFS in the gaps at `thresh` −60; no chatter on a 70 Hz voiced tone 3 dB over the threshold |
| No key | Key Input / Talk Box with an unconnected key | Wet silent, dry passes, no NaN |
| EXP | `formant`, `sib`, `flatten`, `mix`, `noise`, `tilt`, `env_rel` driven by the contract's 50 Hz-stepped sweep | Control sidebands ≤ −80 dB |
| CPU | 48 kHz, mono / stereo | ≤ 450 ns at 24 bands Tracked Saw with the noise bank; ≤ 1.4 µs at 40 bands Supersaw Ninth, stereo carrier |
| Listening | voice/male, voice/female (to record), lead guitar; A / B against TAL-Vocoder and a 20-band reference; 24 vs 40 bands on a fast rhythmic phrase (the lag) ; the 40 Hz floor against 30 and 50 Hz | Renders kept for the maintainer; intelligibility judged on spoken counting ("one, two, six, seven") |

## 6. What golem-rack must provide

What exists (`~/golem/docs/golem-fx-sheet.md`, as read by gfx-ducker §6): golem routes one key, **the
lane's own DI**, to a block flagged `key: true`, on both engines. Nothing from another lane, no MIDI to
effect blocks.

1. **Catalogue entry `vocoder`** (category `modulation`, see Visual), no `key: true` needed for v1: the
   Tracked, Drone, Noise and Self carriers need nothing. `register` is set at insert from the lane's
   role (voice: Mid; guitar, bass, line: Instrument). Key Input and Talk Box with the lane's own DI as
   key are useless (the key is the same voice), so golem should hide or grey those two options until (2).
2. **`key_from`** (gfx-ducker's proposal: `lane:<id>` + `tap di | out`): a voice lane's vocoder reading the
   keys lane's output (Key Input), a guitar or keys lane's vocoder reading the vocal lane's DI (Talk Box).
   Key timing: a `di` key from the vocal lane should reach the block within ~5 ms of the main; the
   vocoder tolerates more than a ducker (the envelopes smooth over tens of ms), but a carrier late by
   more than ~20 ms smears note changes against syllables [A].
3. **MIDI to an effect block** (phase 2): the keys lane's controller notes delivered to the vocoder on
   the voice lane. Today golem's watcher connects MIDI ports to `kind == "midi"` lanes only
   [V: `golem_watch.lua`]; nothing reaches an effect block.
   - Golem Engine: Ardour feeds a plugin's inputs from audio **or MIDI** tracks as a sidechain, and the
     manual names the use: "a MIDI signal is used to control an audio effect, like a vocoder or an
     auto-tuner" ([Ardour manual, sidechaining](https://manual.ardour.org/signal-routing/sidechaining/))
     [V]. In the source [V: `~/golem-engine/libs/ardour`]:
     - `PluginInsert::add_sidechain (n_audio, n_midi)` creates MIDI sidechain ports;
     - but the Lua-bound `Route:add_sidechain (proc)`, the one the watcher uses, sizes the sidechain
       from the plugin's pins flagged as sidechain, and a VST3 pin is flagged only on an **aux input
       bus** (`vst3_plugin.cc`). nih-plug's note input is the main event bus: the watcher's call would
       add no MIDI port. Phase 2 therefore needs one more engine patch: bind
       `PluginInsert::add_sidechain` (or give `Route::add_sidechain` the counts).
     - A plugin with audio inputs and a MIDI input stays an effect (`is_instrument` needs the
       Instrument category or no audio input), and on an audio lane its MIDI pin is simply left
       unconnected.
   - REAPER: a MIDI send from the keys track to the vocoder's track [U: to verify with golem_rig's sends].
   - **v1 declares no MIDI input** (decided at the review). The bus is not part of what sessions and
     the engine table store (class id, parameter ids), so adding it in phase 2 breaks nothing, while a
     declared input that does nothing changes the pin layout both engines see for no benefit, on a
     path nobody has tested. Phase 2 starts with golem-rack's check on a throwaway build with
     nih-plug `MIDI_INPUT = Basic`: loads on both engines, stays an effect, keeps the lane mono, the
     parity null still holds.
4. **Song key**: golem sets `key` / `scale` per song, as for gfx-pitch's harmony. Three effects now carry
   the pair (gfx-pitch, gfx-golem, this one; gfx-correct next): the engine table should mark them
   song-bound once, by id, instead of block by block.
5. **Foot**: `freeze` as a momentary or latched foot action; EXP on `formant` (heel −6, toe +6 st
   suggested) or `mix`.

## Visual

- **Category `modulation`**, the family's green `#00a84f`, **anodized** (the family's finish).
  - Why modulation: it is the one family offered to all three roles that need a vocoder today — voice,
    guitar and keys (`role_categories` [V]): `pitch` is not offered to keys, `filter` not to voice. The
    vocoder's cousin, the ring modulator, is catalogued there too [V], as is `glitch`.
  - **No crate hue.** The only free gap in the green region (84° – 127°, between glitch lime and tuner
    green) leaves a candidate within ΔE\*ab 21 – 25 of glitch `#72c000`, tuner `#126e1c` and the
    modulation green itself [A: CIELAB D65 for `#3d9a12` and `#4a8f0a`]: it would not read as distinct.
    gfx-tuner's design reached the same conclusion and left the gap by lightness (`#126e1c`, L* 40); a
    second dark green would sit on it. The motif sets the pedal apart, as gfx-flanger and gfx-phaser do
    on the same green.
- **Motif `bank`** (new case in `motifParts`): **a filterbank, its bands raised by a vowel**. Renamed
  from `formants`: gfx-formant's design uses that name for another drawing (two side columns, harmonic
  ticks, an envelope curve running upwards). To keep the two apart this one has **no envelope line**,
  runs horizontally, and is made of closed arches only.
  - Baseline y0 = cy + ch / 2 + 8, just under the cartouche (as `bins`; the LED and the switch stay
    clear). Nine bells, centres x_i = W · (i + 0.5) / 9, i = 0 … 8, half-width w = W / 9: each bell
    reaches its neighbours' centres, so the row reads as overlapping bands, not as separate bumps.
  - Each bell is the cubic `M x−w y0 C x−w/3 y0 x−w/3 y0−h x y0−h S x+w/3 y0 x+w y0`.
  - Heights h_i = e_i · room_i, with the vowel e = [1, .55, .85, .45, .3, .5, .9, .6, .7] and room_i the
    space the bell has: 6 px when its centre lies under the cartouche (|x_i − cx| < cw / 2: it stops
    2 px under it), ch + 6 beside it (up to cy − ch / 2). The drawing is a scalloped bank under the rune
    with two unequal horns at its sides.
  - Opacity .45 + .5 · e_i per bell (as `bins`' bars): the formant bands are the bright ones.
  - All `line`: engraved on anodized, yellow lined in black on thrash (if forced).
  - Distinct from `bins` (gfx-glitch: thin vertical bars, sliced), `curve` (gfx-eq: one curve over a
    grid), `sidebands` (gfx-ringmod: a modulated sine) and gfx-formant's `formants` (vertical, an open
    curve over ticks). To be looked at once on the family sheet next to gfx-glitch: both live under the
    cartouche.
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-vocoder": {"name": "Vocoder", "blocks": ["vocoder"], "knobs": ["formant", "env_rel", "sib", "mix"],
                  "selectors": ["source", "chord"], "toggles": ["freeze"], "category": "modulation"}
  ```

  Four knobs, two selectors, one toggle (gfx-ducker's layout).
- **Glyph** `visuals/glyphs/vocoder.svg`: three overlapping filter bells of unequal height on one
  baseline, the bank shaped by a vowel:

  ```svg
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>Vocoder (gfx)</title><path d='M2.5 20.5c1.5 0 2-9 3.5-9s2 9 3.5 9M8.5 20.5c1.5 0 2-16.5 3.5-16.5s2 16.5 3.5 16.5M14.5 20.5c1.5 0 2-6 3.5-6s2 6 3.5 6'/></svg>
  ```

  Tops at y 11.5, 4 and 14.5; the feet cross inside the stroke at x ≈ 9 and 15, so the bells join into
  one bank [A]. The envelope curve of the first draft is gone: gfx-formant's glyph is an envelope curve
  over an arrow, and two voice glyphs with the same curve would be confused at 24 px. Distinct from
  `eq` (one curve with dots), `spectral-comp` and `glitch` (straight bars), `autowah` (crescent) and
  `golem-double` (two waves).
- No maker's livery, logo or trademark name on the pedal.

## Open questions for review — settled

1. **Category**: `modulation`. It is the only family offered to voice, guitar and keys today
   [V: `role_categories`]; `pitch` would need the keys role changed for one block, and a `voice` category
   would hide the pedal from guitar and keys lanes, where Synth Guitar and Talk Box live.
2. **Defaults**: clear by default (Tracked Unison Saw, `flatten` 100 %, `unvoiced` 50 %, `sib` 50 %). The
   first thing a singer judges is whether the words come through; the vintage colour is two presets
   away ("Strings Vocoder", `flatten` 0 %).
3. **Hohmann's delays / a "Precise" option**: no. With the width law the sum is within 1.0 – 1.5 dB from
   24 bands up (§2.2), on a signal whose spectrum the vocoder reshapes anyway. A reported latency on a
   voice pedal would cost more than it buys.
4. **Low-band lag**: no 2nd-order low bands (5.7 – 6 dB of ripple or no selectivity [P]). The lag is
   bounded instead: no band narrower than 40 Hz, so at most 15.9 ms anywhere, at any band count and
   spacing (§2.2). The 40 Hz constant is listened to against 30 and 50 Hz.
5. **V/UV detector**: the band balance alone, with its own fast followers; YIN's clarity is out of the
   decision (it is 10 – 25 ms late by construction, §2.6) and a zero-crossing rate would measure the same
   thing as the balance. If the bench falls under 90 %, aperiodicity is added as a second way in.
6. **MIDI input in v1**: no (§6, item 3).
7. **`vocoder.rs` → `pv_shift.rs`**: yes, as its own commit, proven by gfx-pitch's bit-exact null (§2.9).
   the maintainer can still veto it; nothing in this design depends on it.
8. **`chord` list**: kept. Octave and Fifth with the octave below are the right thickeners for a voice
   (the added notes stay under the sung one, so the melody stays on top). Sus2 / Sus4 can be appended.
9. **`key` / `scale` per block**: yes, song-bound by id in the engine table (§6, item 4).
10. **A recorded voice DI** is needed before the bench can run its V/UV, sibilance, tracking and gate
    rows: male and female, spoken and sung, with stage bleed, fricatives and plosives hand-labelled.
    One session serves gfx-ducker, gfx-deesser, gfx-formant and gfx-correct too.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good — Strong once the bench has a voice.** The idea is right and is the gap in the field:
no surveyed vocoder gives a singer a carrier that follows the voice in the song's key, and the
intelligibility chain (flatten, V/UV noise, sibilance pass, tilt) is the right one. The bank's main
claims were recomputed and hold: neighbours −13 … −14 dB at w 0.85, a 2nd-order bank at 5.7 – 6 dB of
ripple or no selectivity, ripple 2.3 / 1.3 / 1.0 dB at 16 / 24 / 40 bands (`proto-gfx-vocoder/bank.py`,
analytic responses, 44.1 – 192 kHz). Four things did not hold: the lag figures, the bank at few bands
and at wide widths, the V/UV detector's speed, and the noise passing through `flatten`. None changes
the architecture; two add structure (a floor on the band scale, a noise bank). What paper cannot
settle is everything that needs a recorded voice: the V/UV agreement, the pending onset, the gate's
default.

### Changes made, and why

| # | Change | Why |
|---|---|---|
| 1 | **Lag restated**: τ = 4 / (2π b), 11 – 29 ms in the lowest band at 16 – 40 ERB bands, 26 – 69 ms with Log spacing, as designed | "8 – 15 ms" was the impulse response's envelope peak, 3 / (2π b), read at 16 – 32 bands only. The envelope of a band follows with the group delay |
| 2 | **Band scale with a 40 Hz floor** (Δ = max(s · E, Δ_min)): lag ≤ 15.9 ms in every band at every setting | Open question 4, solved without 2nd-order bands. A plain floor on the pole width breaks the sum (7 – 34 dB of ripple [P]); spacing the low bands wider keeps it at ≤ 1.5 dB and gives the surplus bands to the speech range. The default moves by one band (36.5 → 40 Hz) |
| 3 | **What the lag does on stage** written out (§2.2) | Consonants 1 – 6 ms; vowel onsets 5 – 8 ms late at 16 – 24 bands, 11 – 14 ms at 32 – 40: a softer attack, not an echo. Guidance: ≤ 24 bands for tight parts |
| 4 | **Width law** w0 = clamp(0.93 − 0.085 · s_E, 0.55, 0.90) instead of a fixed 0.85 | At 0.85 an 8-band ERB bank ripples by 11.7 dB and the 11-band Log bank of the "Strings" preset by 10 dB. With the law: 4.2 and 4.0 dB, and "100 % = flattest" is true at every band count |
| 5 | **Edge trims and sum gain G_Σ** | The first and last bands stood 2.1 – 2.5 dB over the rest, outside the range the flat-sum check looked at; the aligned sum sits 2.5 – 4.2 dB under one band, which the level calibration silently absorbed |
| 6 | **`width` above 100 % widens the analysis only** | Widening aligned synthesis bands makes them cancel: −8.9 dB and 3.9 dB of ripple at 130 %, −18 dB and 23 dB at 200 %. "Smeared, softer" is an analysis property |
| 7 | **`low`, `high`, `width` use the bank crossfade**, not a coefficient glide; the design call is bounded and budgeted | `set_param` runs on the audio thread: "designed off the audio path" was not true. A glide would re-solve phase factors, trims and G_Σ per tick |
| 8 | **Envelope claims corrected**: \|z\| has the band's image as ripple (−59 dB default, −22 dB at 8 bands, −10 dB in the worst corner); beat ripple measured at 0.5 dB (40 ms), not "a few dB" | "Ripple-free" was true at the default only; the beat was over-estimated |
| 9 | **V/UV from the band balance alone**, on its own 2 / 10 ms followers | YIN reads a 2.4 kHz low-passed signal over 27 ms: ANDed in, it delays every "s" by 10 – 25 ms and misses plosive bursts. Read from M_k, the balance would lag by `env_rel`. V/UV now works with every source and YIN runs for Tracked only |
| 10 | **Noise through its own bank**, equal-power against the carrier's bands | Crossfaded before the shared bank, the noise meets `flatten` gains still set for a saw: +20 dB of highs for a few ms at each "s", dulled highs for 30 ms after. Cost ≈ +30 % of the bank |
| 11 | **Carrier AGC folded into C̄**; `flatten` written as (M_k / C̄) · (C̄ / C_k)^f | Two normalisers did one job, and the formula as written had no level reference. At 100 % the band level is the modulator's by construction |
| 12 | **Level measured**: −0.2 … +2.9 dB at `flatten` 100 % (fixed −1.5 dB trim), +3 … +9 dB at 0 % with a saw → a row in the bench and a per-wave trim if needed | `flatten` is EXP-capable: a sweep must not move the level by 6 dB. A running loudness match was tried and dropped: it trades one error for another (−4 … +3 dB) |
| 13 | **`mix`**: Self and Talk Box are coherent with the dry: 1.2 – 2.1 dB dip and a comb at 50 % | "Uncorrelated" holds for internal carriers and Key Input only |
| 14 | **Sibilance path**: gated by `thresh`, live during freeze; overlap with the top band stated (±1 dB, no comb unless Self); timing stated (leads by ≤ 1.3 ms + attack) | With the gate closed the −12 dB floor passed cymbal bleed. "No gap, no doubled band" was unproven; it holds in power |
| 15 | **Tracked**: `register` (gfx-golem's id + Instrument), the 40 ms octave guard, quantize hysteresis, and a **pending** state at phrase starts (−12 dB under noise until the first voiced frame) | The instrument detector with harmonic lock is the wrong one for a voice; an octave flip is the audible error; the bench asked for a wrong-note burst 12 dB down and nothing produced it |
| 16 | **Gate window** = one period of the lowest note, range −90 – 0 dBFS | 10 ms ripples under 100 Hz (gfx-gate's lesson); gfx-gate's `thresh` is −90 – 0 |
| 17 | **Module `filterbank`** (not `gammabank`); `osc` shared with gfx-ringmod's wavetables on one phase type; `vocoder.rs` → `pv_shift.rs` decided | gfx-dsp's README already reserves `filterbank` for exactly this; two designs name `osc` |
| 18 | **No MIDI input in v1**; the Ardour path checked in the source | The manual's claim is right, but the watcher's `Route:add_sidechain` adds no MIDI port for a main event bus: phase 2 needs an engine binding. An unused input buys nothing |
| 19 | **Motif `formants` → `bank`**, no envelope line in the motif or the glyph; placed under the cartouche | Name clash with gfx-formant, and the same curve in two voice glyphs. The first placement (baseline H − 40) shared its row with the LED and the lower screws [A, from `gfxPedal`'s layout] |
| 20 | Self formant preset: 20 bands, `width` 160 % (was 40 bands); Talk Box preset 24 bands; EXP parameters all on `exp_smooth`; bench rows added (lag, bank edges, design time, level vs flatten, carrier purity, V/UV timing and edges, quantize) | Narrow bands follow harmonics, not formants; three EXP rows listed a 20 ms smoother |

Checked and left as written: 4th order over 2nd; phase alignment without Hohmann's delays; the
formant remap (filters never move, EXP-safe); freeze on a slow copy; equal-power `mix` for internal
carriers; PolyBLEP + closed-form buzz + 7-saw supersaw; frequency tracking rather than gfx-ringmod's
phase lock; chords through `harmony_interval`; the Surge, TAL, mda, Korg and Ableton readings as far
as they could be re-read (Ableton's band range stays [U]); category, finish and the absence of a crate
hue (gfx-tuner's review found the same closed gap in the greens); linear ranges, no mode-dependent
default, append-only enums, `bypass`.

Coverage: nothing a live player needs is missing against Ableton's Vocoder (pitch-tracking carrier,
unvoiced, enhance, formant, gate), Softube (unvoiced, tilt), Vocodex (noise) or the EMS / Moog hardware
(V/UV switch, sibilance pass). Left out on purpose: per-band level and pan faders (VocalSynth, EMS
patch bay: a UI the pedal does not have), a band-count-per-preset "model" list, sample-and-hold of the
envelopes on a clock (Moog; `freeze` on a foot covers the live use). Nothing superfluous; `spacing`
Log earns its place only through the vintage presets.

### Still open

- **Owner (the maintainer)**
  - Record the voice DIs (open question 10): without them five bench rows cannot run and the verdict
    stays Good.
  - Rename `vocoder.rs` → `pv_shift.rs` (recommended, own commit, bit-exact null).
  - The shared `register` list: Low · Mid · High · Instrument here; gfx-golem and gfx-correct should
    append in the same order if they ever grow.
  - The `bank` motif next to gfx-glitch's `bins` on the family sheet, and the glyph next to
    gfx-formant's.
- **Listening / bench** (cannot be settled on paper)
  - The 40 Hz floor (30 / 50 Hz); 24 against 40 bands on fast phrases.
  - The pending onset: −12 dB and 80 ms are first guesses; on a guitar with `unvoiced` 0 the attack is
    12 dB soft for ≈ 30 ms, to be judged against a full-level wrong note.
  - V/UV on "f", "th" and "p" (weak or low-frequency bursts), and on cymbal bleed above `thresh`.
  - Level across `flatten` per wave; two-point PolyBLEP under `flatten` at `octave` +2.
  - CPU with the noise bank and f64 SIMD (all [A]).
- **golem-rack**
  - Catalogue entry `vocoder` in `modulation`; Key Input and Talk Box greyed until `key_from`;
    `register` set from the lane's role at insert.
  - `key` / `scale` song-bound by id in the engine table.
  - Phase 2: `key_from`; a binding for `PluginInsert::add_sidechain (n_audio, n_midi)` and the check
    of a build with a MIDI input on both engines; REAPER's MIDI send.

**Note (2026-11-14, after the gfx-polysynth review):** the shared gfx-dsp `filterbank` takes an explicit band
list (centres and widths) rather than a spacing law, so gfx-polysynth can ask for one band per partial.
Width convention: gfx-polysynth states full pole widths, twice the half-width this design uses — the module
takes full widths and this design converts.

## As coded (v0.1.0, 2026-11-15)

**Files.**
- gfx-dsp `filterbank.rs` (new): `ComplexBank` (an explicit `BandSpec` list: centre, **full** pole width
  B, order 1 – 4; r = e^(−πB / fs); f64 structure-of-arrays state; unit gain and zero phase at each
  centre), `band_response`, `real_response` (image included), `noise_power` (closed form), `GammaDesign`
  (this design's scale, width law, trims and G_Σ; half-widths b, banks get B = 2b; `index_of` /
  `freq_of`), `BankFade` (40 ms). 5 unit tests.
- gfx-dsp `osc.rs` (new): `Phasor` (f64), `poly_blep`, `saw`, `pulse`, `buzz`, `Supersaw`. 4 unit tests.
  gfx-ringmod is untouched (its wavetables can adopt `Phasor` later).
- gfx-dsp `noise.rs` (new): `White` (seeded xorshift, uniform on [−1, 1)). 1 unit test.
- `src/lib.rs`: the processor (`Vocoder`, `PARAMS`, labels, `REGISTERS`, `METERS`). `src/plugin.rs`: the
  VST3, class id `GoleMFxVocoder01`, mono 1 / 1 first then 2 / 2, each with a 1- or 2-channel key bus
  (declared, unread in v1), no MIDI input, latency 0, `bypass` = `make_bypass`, `f0` and `state`
  read-only. `src/tests.rs`: 16 behaviour tests (§5) + `cpu_report`, ignored.
- gfx-bench: the crate in `EFFECTS` / `make()`; `options()` skips the latency correlation (the wet is the
  internal carrier, uncorrelated with the input; IIR banks, no lookahead: 0 by construction).
- `presets/gfx-vocoder.json`: 12 presets from §4 (v1 carriers only). Visuals: the face, groups, help,
  depends (`register` / `quantize` / `glide` with Tracked, `degree` with Drone), the `bank` motif
  (modulation green, anodized, no crate hue), the `vocoder` glyph.

**Measured** (48 kHz unless stated; synthetic signals: pink noise, two-formant sung vowels, a fricative
of white noise high-passed at 3.5 kHz).

| §5 row | Bound | Measured |
|---|---|---|
| Flat sum (analytic, the bank's own response) | ripple ≤ 1.7 dB at 24 – 40 bands (Log 24 ≤ 2.3), ≤ 2.5 at 16 – 20, ≤ 5 at 8 – 12; mean ±0.3 dB | ERB 24 / 32 / 40: 1.76 / 2.02 / 1.85 dB; Log 24: 2.49; ERB / Log 16: 2.21 / 2.64; 8 bands 5.26 / 6.43; 11 – 12 Log 3.6 – 4.6. Above 500 Hz: 0.9 – 1.2 dB at 24 – 40 ERB bands. Mean −0.10 … +0.10 dB (44.1 – 96 kHz) |
| Bank edges | ±0.5 dB | −0.33 … +0.17 dB (trims −1.2 … −2.6 dB) |
| Band selectivity | neighbours ≤ −13, next ≤ −25 dB | 16 / 24 / 40 bands: −15.1 / −14.4 / −13.9 dB, next ≤ −29.7 dB |
| Lag | ≤ 4 / (2π b) + 1.5 ms, ≤ 17.5 ms | ≤ 15.6 ms in every band (24 / 40 ERB, Log 8 / 24 / 40, `low` 50 and 100 Hz), on 2 \|z\|. With a 20 ms-release follower the envelope's 10 Hz modulation lags up to 23 ms |
| Bank design | ≤ 50 µs at 40 bands | 15 µs (test profile), no allocation |
| Self identity (pink in place of the DI) | ±3 dB third-octave | mean −1.2 dB, worst band 1.3 dB off the mean |
| Level (pink, Drone saw, flatten 100 %) | ±1.5 dB at 16 – 40, ±2.5 at 8 | 8 / 16 / 24 / 40 bands: −0.97 / −0.48 / −0.76 / +0.39 dB |
| Level vs flatten (100 / 50 / 0 %) | > 3 dB → per-wave trim, then ≤ 2 dB | before the trim: saw +3.4, pulse +2.2 (with a 50 % dip: spread 3.5), supersaw +3.1, buzz 1.5, noise 0.7 dB. With the trims: spread saw 1.2, pulse 2.4, buzz 1.5, supersaw 1.7, noise 0.7 dB |
| Flatten | saw vs buzz within ±2 dB at 100 %; differ by the tilt at 0 % | 0.4 dB; at 0 % the saw's 5 kHz band falls 26.4 dB more than the buzz's |
| Carrier purity (B5, 988 Hz, steady envelope) | ≤ −60 dB | saw −60.4, pulse −67.2, buzz −71.2 dB. 4 kHz (`octave` +2 Tracked) not run: Drone tops at B5 and a 4 kHz sung tone is outside every register |
| Formant +12 st | peaks up one octave ± 1 band | first formant's band 8 → 12 (one octave = 12.2) |
| V/UV (synthetic) | u ≥ 0.5 within 6 ms; vowels < 0.5 on 97 % | 2 ms; 0 % of vowel frames unvoiced. The DI rows (labelled fricatives, plosives, ≥ 90 %) wait for the recordings |
| V/UV edges (`unvoiced` 100 %) | ≤ 2 dB per band | whole output, 5 ms windows: +2.0 dB over the louder steady level |
| Gate | wet ≤ −90 dBFS on bleed; no chatter on 70 Hz 3 dB over | bleed with peaks at −66 dBFS (−78 dBFS RMS): wet exactly 0, sibilance path included; 70 Hz at −57 dBFS: the gate opens once and stays open |
| Tracking (synthetic, Mid) | within 10 ¢; pending −12 ± 1 dB | first voiced frame 11 ms after the onset, f0 and carrier +0.1 ¢; pending carrier −11.4 dB |
| Quantize | one note, no trill | 220 Hz ± 60 ¢ at 5.5 Hz, Chromatic: one note |
| Chords | every note in the scale | Triad, Key quantize, A minor scale sung 10 ¢ sharp: 0 of 5,979 voice-frames out of A minor |
| Freeze | — | held vowel −0.0 dB against the sung level; released: silent |
| Edges | common bound −50 dB | bench steps: worst −50 dB (formant −12 → +12 at 44.1 kHz), −56 / −60 / −59 dB at 48 / 96 / 192 kHz; `bands` 24 → 8 → 40 under a vowel: 5 ms windows −5.2 … +2.3 dB |
| Common checks | — | 28 / 28 at 44.1 – 192 kHz (pink noise as the modulator: no voice DI in the repository) |
| CPU, 48 kHz | ≤ 450 ns (24 bands, Tracked Saw, noise bank); ≤ 1.4 µs (40 bands Supersaw Ninth, stereo carrier) | bench (pink: pending all the time, noise bank always on) 594 ns mono, 634 ns stereo; a sung vowel 487 ns; Drone without the noise bank 451 ns; 40 bands Supersaw Ninth stereo carrier 1.13 µs (the machine shared, ±10 %) |

Not run here: the V/UV, sibilance and tracking rows on voice and guitar DIs (no recordings yet), the EXP
sideband row (the EXP parameters run on `exp_smooth`, tested there), the listening row.

**Deviations from the design, and why.**
- **CPU over the 450 ns budget** (487 ns on a vowel, 594 ns on the bench's pink noise, where Tracked stays
  pending and the noise bank never stops). The three f64 banks cost ≈ 95 ns each at 24 bands on the
  baseline x86-64 target (SSE2: two f64 lanes; `target-cpu=native` measured no faster); §2.10's
  130 – 200 ns assumed 4-lane f64 SIMD. Left for the owner: f32 for the noise bank, or a run-time AVX path.
- **Flat-sum ripple** 0.1 – 0.3 dB over the [P] targets (2.6 dB at Log 24, 6.4 at Log 8): the excess sits in
  the floor's linear bands under 200 Hz; above 500 Hz the sum is within 0.9 – 1.2 dB. Bounds in the tests
  are the measured values.
- **Flatten's divisor** is max(2 |z_k| now, C_k, C̄ / 100), not max(C_k, floor): the band's own
  instantaneous envelope keeps the gain from overshooting while C_k (5 ms attack) catches up after a note
  change (a +20 – 40 dB burst on the bench's step check). (C̄ / den)^`flatten` per sample (fast log2 / exp2;
  plain division at 100 %); C̄, 1 / C̄ and the presence fade per tick.
- **Noise bands at −2.7 dB** under a unit envelope: measured level of a flattened noise carrier band (its
  divisor rides the envelope's peaks); at unit envelope the V/UV crossfade rose 4.7 dB.
- **Per-wave trims** at low `flatten`: saw −3.4, pulse −2.2, supersaw −3.1, buzz 0 dB, × (1 − `flatten`) ×
  the oscillators' share of the carrier, crossfaded with `wave`. Pulse still spreads 2.4 dB (its 50 % point
  dips): above §5's 2 dB.
- **Remap weights** (`formant`, `tilt`) are interpolated across one tick (a step per tick buzzed: −17 dB on
  the step check); **`flatten`** is read per sample.
- **`octave` glides over 20 ms**, not 5: a ±4-octave sweep in 5 ms (or a step) left −38 dB bursts at 96 /
  192 kHz through the flattened bands.
- **Pending onset**: the carrier's −12 dB and the noise share both follow a 5 ms ramp (down and up); a
  stepped noise share clicked when the first voiced frame arrived.
- **Bank crossfade**: the new core inherits the old one's C̄ (the same carrier), otherwise its 200 ms C̄
  settling dipped the level after every `bands` / `low` / `high` change.
- **`REGISTERS` and the octave guard stay in this crate** (gfx-golem's values and rule, Instrument
  appended), not moved into `detect`: moving them touches gfx-golem (a null-guarded change for the owner).
  Two YIN detectors: the voice ranges without harmonic lock (60 – 1,200 Hz, narrowed by `set_range`), and
  Instrument's 75 – 1,500 Hz with it (the lock is fixed at `prepare_range`).
- **Quantize** uses its own hysteresis (a new note 0.3 st nearer than the held one in 3 analyses running):
  `NoteTracker` gives pitch classes only, and the root needs the octave.
- **Infeasible scales** (e.g. 40 bands in 500 – 2,000 Hz, where 40 Hz-wide bands cannot fit) fall back to a
  linear layout at w0 0.90, narrower than the floor.
- **Gate bleed row**: the gate reads peaks (one period's maximum), so the bleed is set by its peak, 6 dB
  under `thresh`; −70 dBFS RMS of pink noise has peaks near −58 dBFS and opens a −60 dBFS gate.
- **Spread** 100 %: the bands' pans are constant power, but the total rises 2.7 dB (neighbouring bands,
  partly out of phase where they cross, no longer cancel once on different sides).
- **Level trim**: −1.5 dB at every `flatten` (the per-wave trim covers the rest).
- The `vocoder.rs` → `pv_shift.rs` rename had already been made before this crate.

**Left for the owner.** The voice DIs (male, female, spoken and sung, with labelled fricatives and stage
bleed) for the V/UV, sibilance, tracking and gate rows; listening (24 vs 40 bands on fast phrases, the
40 Hz floor, the pending onset's −12 dB / 80 ms, the octave's 20 ms glide); the CPU decision; moving
`REGISTERS` and the octave guard into `detect`; golem-rack's catalogue entry `vocoder` (modulation),
`register` set at insert, `key` / `scale` song-bound.

