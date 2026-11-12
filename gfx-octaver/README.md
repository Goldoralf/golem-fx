# gfx-octaver

> **Status: CODED** (v0.1.0, 2026-11-12) — Vintage (OC-2: adaptive Schmitt trigger, ÷2 / ÷4, the filtered signal's polarity switched), Clean (half-angle identity on a fundamental-tracking low-pass: 3rd harmonic under −50 dBc on a sine), Synth (oscillator locked to the divider, sine → square, glide, stability gate), Poly (28-band ERB complex filterbank: z² / |z| up, branch-tracked √z down below `range`), octave up (oversampled rectifier; analytic z² / |z| in Clean); all zero reported latency; 5 behaviour tests, bench green. Choices made in code: Clean's envelope is the last cycle's peak, held (a decaying follower rippled ~10 % per period and put the 3rd harmonic at −35 dBc); the dry's alignment all-passes sit at 550 Hz for both instruments (switching must not move the dry); `direct` = 1 is unity, so golem's presets may need a level check against the JSFX (pregain 4.7).

golem's octaver. Stands for golem's `octaver` block (today guitarix gx_oc_2, a Boss OC-2 model;
`golem_octaver.jsfx` on Windows / macOS), keeping its parameter ids and units
([golem contract](../docs/golem-contract.md)). The aim: the OC-2's instant analog-style sub, a clean sine
sub that keeps the player's dynamics (the bass player's octaver), a synth sub, an octave up, and a
polyphonic mode — all with zero reported latency. Shifting by arbitrary intervals belongs to
[gfx-pitch](../gfx-pitch/).

## 1. What golem has today

- Catalogue: `direct`, `octave1` (−1 oct), `octave2` (−2 oct), each 0 – 1. Presets: Bass double
  (1 / 0.5 / 0), OC-2 classic (1 / 0.7 / 0), Organ stack (0.8 / 0.8 / 0.5), Sub synth (0.4 / 1 / 0.6).
- `golem_octaver.jsfx` [V]: input low-pass 830 Hz → hysteresis comparator → flip-flop → the filtered signal
  × ±1, −1 oct low-passed at 470 Hz; −2 oct switches the −1 oct voice on its own zero crossings (clocking it
  from the input gave a dominant 3f / 4), low-pass 210 Hz. A pregain of 4.7 applies to the direct too.
- **Gain convention to settle**: with that pregain, `direct` = 1 is ≈ +13.4 dB in the JSFX. gfx-octaver
  uses unity (`direct` = 1 = 0 dB); golem's presets may need a level check when the block switches
  (golem side).

## 2. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference.

| Reference | Mechanism / facts | What we take |
|---|---|---|
| Boss OC-2 ([guitarix oc_2.dsp](https://github.com/brummer10/guitarix/blob/master/trunk/src/LV2/faust/oc_2.dsp)) [V] | Low-pass → adaptive Schmitt trigger → flip-flops (÷2, ÷4) → a CMOS switch flips the **polarity of the filtered signal**: the sub keeps the input's envelope and some of its timbre (not a square × envelope) | Vintage mode |
| Esqueda, Välimäki et al., SMC 2018 ([doi](https://doi.org/10.5281/zenodo.1422641)) [V] | Full recipe: 4-pole smoothing at ~1 kHz, adaptive Schmitt trigger (Matlab code), latch, flip-flop, **allpass alignment of the dry** to the clock edges, then a processor and a resonant filter; switching at zero crossings avoids steps and aliasing | The digital divider |
| [Boss OC-5](https://www.boss.info/global/products/oc-5/) [V] | Vintage (≈ 1 ms) and Poly modes; Range ("Lowest" octaves only the bottom note of a chord); guitar / bass switch; +1 octave (poly, ≈ 10 ms, anecdotal) | Range, instrument switch |
| [EHX POG3](https://ehx.com/products/pog3) [V] | Polyphonic −2, −1, +5th, +1, +2; low-latency "original POG" algorithm | Poly mode |
| [MR-DOS/guitar_down_octave](https://github.com/MR-DOS/guitar_down_octave) (GPL-3) [V] | `sqrt` of the DC-shifted normalised signal, every second half-cycle flipped: the **half-angle identity** cos(θ/2) = ±√((1 + cos θ) / 2) — a near-pure sine sub (3rd harmonic −62 dBc) made from the player's own waveform | **Clean mode**, sample by sample, zero latency |
| Aalto MSc thesis, *Real-Time Polyphonic Octave Doubling for the Guitar* ([aaltodoc](https://aaltodoc.aalto.fi/items/6fd2cfc9-db61-4b0a-b11f-ea8c2925509f)) [V] | Doubling the instantaneous phase of sub-bands from a non-decimated ERB IIR filterbank beats the commercial pedals on latency at comparable quality | Poly mode |
| Octavia ([DIYSB](https://www.diystompboxes.com/smfforum/index.php?msg=792057)) [V] | Octave up by full-wave rectification | Vintage octave up |
| [Aguilar Octamizer](https://www.aguilaramp.com/effect-pedals-2/octamizer-ii/) [V] | Separate dry and octave paths, 3-pole low-pass 80 – 800 Hz on the octave | Sub tone |
| YIN (de Cheveigné & Kawahara 2002), MPM (McLeod 2005) [I] | Trackers need ~2 periods (≈ 50 ms on a bass's low E) | Only as a confidence gate, never in the audio path |

### What we conclude

1. **Vintage**: the OC-2 polarity switch, with an adaptive trigger and the dry aligned to the clock.
2. **Clean**: the half-angle sub — near-pure sine, the player's own envelope, zero latency. The best bass
   mode.
3. **Synth**: an oscillator whose phase is locked to the divider's edges (zero latency); a tracker only
   gates uncertain input instead of glitching.
4. **Poly**: ERB filterbank, per-band phase doubling (up) and per-band square root with branch tracking
   (down), only below `range`. Fallback if the bench fails it: gfx-pitch's phase vocoder (reported latency).
5. Skipped: +2 octaves, fifths, detune, freeze (gfx-pitch), formants, resonant envelope filter (gfx-autowah).

## 3. Design

| `mode` | Algorithm | Reported latency |
|---|---|---|
| **Vintage** (default) | Smoothing low-pass → adaptive Schmitt trigger (peak follower) → ÷2; ÷4 clocked from the −1 oct voice's zero crossings; the filtered signal's polarity switched; dry aligned by two allpasses | 0 |
| **Clean** | Same trigger. The half-angle identity needs a near-sinusoid: `x` is the input through a **tracking low-pass** that follows the divider's own period (cut-off ≈ 1.5 × the detected fundamental, slewed), normalised by its envelope; sub = sign(flip-flop) · env · √((1 + x / env) / 2), clamped. On a note change the filter and the flip-flop re-settle within a period | 0 |
| **Synth** | Oscillator phase-locked to the divider edges, sine ↔ square (`shape`), envelope follower, `glide`; a YIN confidence gate mutes on uncertain input | 0 |
| **Poly** | ERB IIR filterbank (24 – 32 complex bands); up = z² / \|z\| per band; down = per-band square root with branch tracking below `range` | 0 (the low bands' group delay is inherent, as in analog pedals); vocoder fallback: one frame, reported, dry delayed to match |

- Octave up: full-wave rectifier + DC blocker (Vintage, Synth); analytic signal by an IIR Hilbert pair then
  z² / |z| (Clean); per band (Poly).
- The rectifier and switching stages run 2× oversampled (minimum-phase). Every state zeroed on `reset()`.

## 4. Parameters

Catalogue ids kept.

| id | Name | Type | Range | Catalogue | Default | EXP | Role |
|---|---|---|---|---|---|---|---|
| `direct` | Direct | f | 0 – 1 | same | 1 | ✓ | Unity gain at 1 |
| `octave1` | Octave −1 | f | 0 – 1 | same | 0.6 | ✓ | |
| `octave2` | Octave −2 | f | 0 – 1 | same | 0 | ✓ | |
| `octave_up` | Octave +1 | f | 0 – 1 | new | 0 | ✓ | |
| `mode` | Mode | e | Vintage · Clean · Synth · Poly | new | Vintage | | |
| `instrument` | Instrument | e | Guitar · Bass | new | Guitar | | Input filter (800 / 300 Hz) and trigger time constants |
| `sub_tone` | Sub Tone | f | 60 – 2,000 Hz | new | 470 Hz | | Low-pass on −1 oct; −2 oct follows at ~0.45× |
| `up_tone` | Up Tone | f | 500 – 12,000 Hz | new | 4,000 Hz | | |
| `sensitivity` | Sensitivity | f | 0 – 100 % | new | 50 % | | Hysteresis and gate relative to the envelope |
| `glide` | Glide | f | 0 – 200 ms | new | 0 ms | | Synth |
| `shape` | Shape | f | 0 – 100 % | new | 0 % | | Synth: sine → square |
| `range` | Range | f | 40 – 1,000 Hz | new | 1,000 Hz | | Poly: only notes below this get octaved |

## 5. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| OC-2 synth-bass line on guitar | Vintage, direct 0.4, octave1 1, octave2 0.6 (golem's Sub synth), neck pickup, single notes | The OC-2's glitchy-warm character |
| Fatter single-note riffs (Jack White, QOTSA) | Vintage, direct 1, octave1 0.7 | Instant, zero latency |
| Bass: solid sub under the note | Clean, Bass, direct 1, octave1 0.5, `sub_tone` 250 Hz | Near-sine sub that follows the playing dynamics, no tracking glitches on decays |
| Organ stack | Vintage, direct 0.8, octave1 0.8, octave2 0.5, `octave_up` 0.3 | |
| Synth bass with glide | Synth, `shape` 70 %, `glide` 40 ms, octave1 1, direct 0 | Oscillator locked to the divider |
| Octavia lead (Hendrix "Purple Haze") | Vintage, `octave_up` 1, direct 0.5, before a fuzz, neck pickup above the 12th fret | Rectifier octave-up |
| Chords with a sub on the bass note only | Poly, octave1 0.6, `range` 150 Hz | Range (OC-5 "Lowest") |
| 12-string / organ shimmer on chords | Poly, `octave_up` 0.5 | Per-band octave up |
| Fewer false triggers on a noisy rig | `sensitivity` 30 % | Gate relative to the envelope |

## 6. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| Tracking | bass (to record) low E to the 12th fret: sub level steady within 3 dB |
| No flips | legato phrases: no octave flips (sub phase continuous across notes) |
| False triggers | decaying notes: count below a bound; gate floor silent |
| Clean purity | sine in → sub 3rd harmonic ≤ −50 dBc |
| Latency | 0 in every mode (vocoder fallback: reported = measured) |
| Poly | open E chord: roughness against the vocoder fallback |
| Listening | lead-clean.wav, funk.wav and bass with golem's four presets, against gx_oc_2 |
