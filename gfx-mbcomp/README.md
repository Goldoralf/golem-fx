# gfx-mbcomp

> **Status: CODED** (v0.1.0, 2026-11-15). Designed and reviewed 2026-11-14 (Fable 5.2, last section).
> Built as designed: the gfx-dsp `crossover::Tree` (LR4 / LR8, compensation twins) for the audio, a new
> power-complementary `crossover::BwTree` for detection, gfx-comp's engine per band through its new gain
> API (`next_gain_db`, no audio through it) with the bass-safe detector (`set_detector_low_hz`: sliding
> maximum over 1.2 periods on both readings), Lift with the noise guard, Amount / Tilt / Time / static
> auto gain, solo / delta / mix against the aligned dry, signed `<b>_dyn` meters, and the Linear mode on
> the new `crossover::Linearizer` (one FIR for the whole tree, the fit rule, 2,048 / 4,096 / 8,192
> samples per rate family, reported). Measured: the flat sum nulls the f64 all-pass at ≤ −145 dB and 50
> random band-gain sets null Σ g_b band_b at ≤ −145 dB; the downward curve within 0.05 dB in every
> character; no harmonic added by a 31 / 41 Hz note (gain ripple side-bands ≤ −151 dBc) with 90 % of the
> reduction in ≤ 19 ms; the linearizer's residual equals the review's table (LR4 3-band −168.7 dB … LR8
> 40 Hz −65.2 dB) and Linear is flat within ±0.03 dB at every corner; gfx-comp's renders null bit for
> bit before / after. Bench 28 / 28; CPU ≈ 0.48 µs mono at the 4-band LR4 default (budget 0.6). One
> bench row cannot hold as written (Lift and pick transients, below).

golem's **multiband compressor**: 3 or 4 bands, each with downward compression and either upward
compression or expansion below it. It is for guitar, bass, voice and a bus. It is a **new golem block**
(no catalogue entry today). Type `mb-comp`, category `dynamics`, roles guitar, bass, voice, keys, pads
(§Open questions 1).

Built from parts the suite already has:

- **gfx-comp's engine and characters per band** (Clean, Bus, FET, Opto, Pedal; one static curve). Only
  its gain is used, so each band is a pure gain with no colour.
- **gfx-dsp `crossover`** (the three layers of [gfx-bassdi §2.9](../gfx-bassdi/README.md); the tree is
  [gfx-grit §2.2](../gfx-grit/README.md)'s). It is a Linkwitz-Riley tree with all-pass compensation.

What it does that the field does not do together:

1. **The phase never moves with the gains.** Every band of an LR tree has the same all-pass phase, so any
   set of band gains changes only the magnitude: the output stays the input × one fixed all-pass. That
   holds whether a band is compressing, boosting or soloed. When nothing compresses, the output is that
   all-pass, flat in magnitude. This is the promise of a "dynamic phase" mode, kept at every gain.
2. **An optional Linear mode** with 42.7 ms of reported latency. It uses one FIR for the whole tree, not
   one per band: it removes the all-pass, so the output is a pure delay times zero-phase bands.
3. **OTT's upward compression with a noise guard.** A boost never exceeds the signal's height above the
   guard, so the hiss of an amp is not lifted with the quiet notes (gfx-transient's floor bound).
4. **A face of Amount + Tilt with auto gain.** Amount scales every band's slope (0 = flat, 100 % = the
   bands as set, 200 % = harder). Tilt moves the work toward the lows or the highs. Auto gain keeps the
   playing level, so an A / B is fair and "louder" does not win by accident.
5. **Bass-safe detection.** Each band's detector takes the maximum of its readings over 1.2 periods of
   its lowest content (gfx-gate's rule), so a low E in the low band does not ripple the gain, and the
   attack stays fast. That ripple is distortion, the "mud" that players blame on multiband compression.

Mono 1 / 1 first, 2 / 2 second. Reported latency 0 in Minimum mode.

## As coded (v0.1.0)

**Files.** `src/lib.rs` (the processor: `PARAMS`, the effective values of Amount / Tilt / Time and auto
gain as pure functions, Lift's static law, the two audio engines and their switch, detection, meters),
`src/plugin.rs` (class id `GoleMFxMbComp001`, mono 1 / 1 then 2 / 2, latency 0 or L reported per
`phase`, read-only `gr` and `<b>_dyn`), `src/tests.rs` (the §5 rows on synthetic signals).
**Shared code**: gfx-dsp `crossover` gains its fourth layer, `BwTree` and `Linearizer`, with unit tests
(layers 1 – 3 untouched: gfx-bassdi's and gfx-grit's tests pass unchanged). **gfx-comp** gains
`next_gain_db(&[sc]) -> [g; 2]` (the clock, the smoothers and the detector for one sample, `link`
applied; `process_block` is rebuilt on it), `set_detector_low_hz(Option<f32>)` (`None` = today's
detector) and `level_db(ch)` (the detector's reading, which Lift uses). Null against the build before:
102 renders bit-identical (gfx-comp: 12 settings covering every character, colour, `drive`, `sc_hpf`,
`sc_listen`, `mix`, `link`, `range`, × 4 inputs mono / stereo at 44.1 – 96 kHz × blocks 512 and 37;
gfx-bassdi: 3 compressor settings × 2 inputs), and gfx-comp's tests pass unchanged (2 added).

**Measured** (tests at 48 kHz unless stated, and `gfx-bench check gfx-mbcomp`):

| What | Target (§5) | Measured |
|---|---|---|
| Flat sum (Amount 0, and all ratios 1; LR4 / LR8, 3 / 4 bands, defaults, minimum spacing, both ends) | null ≤ −90 dB vs the f64 all-pass; flat ±0.001 dB | null −145.5 to −151.2 dB (32 cases); flat within 0.001 dB |
| Phase independent of gain (50 random `<b>_gain` sets) | null ≤ −90 dB vs Σ g_b band_b; magnitude in [min g, max g] | null ≤ −145.4 dB; inside the bounds within 0.001 dB |
| Static curve, High Mid band, 1 kHz | downward ±0.1 dB every character; Lift +75 % / −100 % ±0.1; caps exact; guard ±0.2; monotone | downward within 0.051 dB (all five); Lift slopes and caps within 0.1 dB; guard law within 0.2 dB; output monotone |
| Threshold sag at a crossover (+6 dB over threshold, 4:1) | (6 − 3) · 0.75 = 2.25 dB ±0.3 | 2.45 dB at 120 Hz, 2.25 dB at 2.5 kHz (both bands) |
| Neighbour leakage (41 Hz at −10 dBFS) | `lm` ≤ 0.1 dB | 0.00 dB |
| Bass ripple (41.2 / 30.9 Hz sines and the asymmetric note; 4:1, attack 1 ms, release 20 / 250 ms; Clean, Bus, FET) | added harmonics ≤ −90 dBc; 90 % in 20 ms; reading ≤ +1 dB (+1.5 Bus) | gain-ripple side-bands ≤ −151 dBc, output harmonics of the sines ≤ −90 dBc; 90 % after 19.0 / 17.4 / 5.5 ms; reading +0.75 / +1.34 / 0.00 dB |
| Noise guard (−80 dBFS noise, Hyper, guard −72) | every `<b>_dyn` 0 dB; output = input + make-up ±1 dB | `<b>_dyn` 0.00 dB; output +11.2 dB (make-ups 9.7 / 5.8 / 11.6 dB); with guard −96: +29.0 dB. Decaying note: boost ≤ level − guard + 1 dB at every block |
| Amount / Tilt / auto gain (a sine at R in each band) | net gain at R ±0.5 dB; Amount 100 % / Tilt 0 = the bands; ratio ≤ 30:1; monotone | worst 0.42 dB (Amount 0 – 200 %, Tilt ±100 %); identical values; 2.5:1 at 161 % → 30:1; monotone in all five characters at 200 % |
| Linear mode | L = 2,048 / 4,096 / 8,192; impulse peak at L; flat ±0.01 dB (4-band defaults), ±0.05 dB (corners); residual ≤ table + 6 dB; gains = Minimum's, L later | as specified at 44.1 / 48 / 96 / 192 kHz; peak at L; ±0.0006 dB at the defaults (LR8), ±0.029 dB worst corner; residual (gfx-dsp test): LR4 −168.7 / −165.5 / −138.3 / −108.0 / −80.3 / −52.9 / −49.2 dB, LR8 −82.3 / −80.1 / −67.3 / −47.4 dB, LR8 under 80 Hz (rest) −101.4 / −65.2 / −59.6 dB: the review's table; per-sample `<b>_dyn` in Linear = Minimum's L samples later, bit for bit |
| Mix / delta | `delta` at Amount 0 ≤ −120 dBFS; `mix` 50 % no notch > 0.2 dB | exactly 0; never under the blend's own gain − 0.2 dB, both phases |
| Clicks under a held 82 Hz note (HF above 10 kHz, re peak) | below the default bound (−50 dB) | `bands` 3 → 4 −77, 4 → 3 −72, `slope` −72, `phase` → Linear −116, → Minimum −111, `x_lo` across 80 Hz in Linear LR8 −128, `character` −127, `solo` −120, `bypass` −114, Amount 0 → 200 −82 dB |
| Attack splatter (82 Hz bursts, about 6 dB in the low band; HF above 1 kHz added by the dynamics, re output) | reported | attack 30 ms −70 dB, 1 ms −70 dB, 0.1 ms −67 dB |
| Bypass null | bit-exact (Minimum), delayed by L (Linear) | both exact |
| Stereo | L = R bit-exact; `link` 0 leaves R alone | both exact (R equals its mono render) |
| Meters | `<b>_dyn` signed = applied gain ±0.1; `gr` ≥ 0 | output gain within 0.15 dB of the meter (the neighbours' 0.13 % at unity: +0.09 dB at 18 dB down); signs and `gr` as specified |
| gfx-bench check | all pass | 28 / 28 at 44.1 – 192 kHz (latency skipped in Minimum, `phase` out of the step check as a set-up switch); worst parameter step −66 dB (`lm_gain` ±24 dB) |
| CPU, mono, 48 kHz (machine under load from other builds, ±20 %) | ≤ 0.6 µs 4-band LR4 Minimum; ≤ 0.8 µs Linear | 3-band LR4 0.37 – 0.48 µs (bench line 0.48 – 0.51); 4-band LR4 0.48 – 0.50; LR8 0.50 – 0.52; Linear LR4 0.52 – 0.69, LR8 0.54 µs |

**Deviations and choices where the spec was silent.**

- **Lift and pick transients: the §5 row "with attack ≤ 2 ms the peak rises ≤ 1 dB" does not hold**
  with the specified ballistics (the boost leaves through the attack's one-pole). Lift +75 % with
  15.8 dB of boost before the pick, peak rise: instant onset 3.4 / 11.7 / 12.9 / 14.0 dB at attack
  0.1 / 1 / 2 / 8 ms; a 2 ms onset 0.0 / 2.5 / 6.3 / 12.2 dB. The law is kept as written (it is the
  snap of the OTT sound); the review's open item "a fixed fast fall for boosts" is the fix if the
  pick-attack DI asks for it. The test asserts the ordering and 0.1 ms on a 2 ms onset.
- **`phase` switch**: the output fades out (10 ms), the path changes at silence (trees and linearizer
  from rest, latency switched), then the path's **input** fades in (10 ms). Fading the output in
  instead let the Linear FIR play the abrupt onset of its history L samples later (−9 dB click).
  Switching to Linear leaves about L of silence.
- **The dry of `mix` and `delta` is the tree's unity sum in both modes** (in Linear it equals the input
  delayed by L within the §2.3 residual). It stays coherent with the wet when LR8's Q 1.307 section is
  left minimum-phase under 80 Hz. `bypass` goes to the input (delayed by L in Linear), as specified.
- **`bands` switch**: detection and the band engines follow the new layout at once; a slot the new
  layout drops keeps its last gain while the old engine fades out, and High Mid's engine restarts from
  rest when 4 bands come back (without the hold, 4 → 3 clicked at −21 dB).
- **Kernel renders**: the impulse response B samples per block, then 4 partitions per block, then the
  20 ms crossfade; one render at a time, the latest request queued (≈ 70 ms per change at 48 kHz, which
  also keeps renders under one per 20 ms).
- **Smoothing**: Amount, Tilt and Time reach the band engines as effective values when a parameter
  changes; gfx-comp's own smoothers glide them (threshold on its EXP smoother, ratio 20 ms, times 50 ms
  in log). Lift's slope and threshold glide 20 ms, the make-up 50 ms, `<b>_gain` 20 ms.
- **Lift**: the global `knee` applies to the Lift threshold too (quadratic, as the downward one); `link`
  applies to Lift as to the downward gain (toward the channel with less boost / more cut).
- **Crossover ordering**: in 3 bands `x_hi` ≥ 1.5 × `x_lo` (`x_mid` is not used); in 4 bands `x_mid` ≥
  1.5 × `x_lo` and `x_hi` ≥ 1.5 × `x_mid`.
- **Auto gain on program**: on pink noise at −18 dBFS RMS the factory presets come out 0 to −5 dB from
  bypass (the detectors read the peak-held levels, above the static reference R). Their `level_db`
  says so; R stays the bench constant to set from the DIs.
- Plugin name `GoleM FX MB Comp`; face word "Bands".

**Left for the owner.** R, the per-band default times, Tilt's ±12 dB and the 80 Hz fit limit by ear on
the DIs; the Lift-transient law on the pick-attack DI; the Listening row; dispersion and pre-ringing
audibility (Minimum vs Linear); golem-rack's `mb-comp` catalogue entry, the "set before playing" note
for `phase`, and the signed `<b>_dyn` meters in the UI.

## 1. State of the art

Survey of 2026-11-14. **[V]** read in a manual or primary page · **[I]** secondary source or inference ·
**[U]** could not be verified · **[A]** our calculation (numpy prototype,
`scratchpad/proto-mbcomp/xo.py`, `lin.py`: bilinear / TPT forms at 48 kHz; re-run and extended at review
in `scratchpad/proto-gfx-mbcomp/`) · **[K]** standard literature, not re-checked.

### 1.1 Commercial references

| Product | What it does best | What we take, or not |
|---|---|---|
| [FabFilter Pro-MB](https://www.fabfilter.com/help/pro-mb/using/processingmode) [V] | Three modes. **Dynamic Phase** does not split: it uses dynamic filters, with "a flat / linear phase response when no gain processing is applied", and "minor phase effects will only be introduced when the gain of a band is actually changed". **Minimum Phase** has no latency but "static phase changes at the crossover frequencies", "very apparent and often unpleasant" at high slopes. **Linear Phase** costs "quite a bit of extra latency and possible pre-ringing". FabFilter staff on the forum: Dynamic Phase cannot null with every band muted, because it does not split [I] | Our tree's phase is fixed by the crossovers alone, whatever the gains: §2.2. Linear as an option with reported latency |
| Xfer OTT (free) / Ableton Multiband Dynamics "OTT" preset ([SOS on Multiband Dynamics](https://www.soundonsound.com/techniques/multiband-dynamics-plug) [V] for the device, [EDMProd](https://www.edmprod.com/ott-plugin/) [I] for the preset's values) | Ableton's device [V]: two thresholds per band, Above and Below, each compressing or expanding; defaults 120 Hz and 2.5 kHz; "the Amount knob scales down all ratio settings", "the Time knob … scales all attack and release settings". The OTT preset [I]: three bands at 88.3 Hz and 2.5 kHz, fixed in OTT. "Above" = downward (highs ∞:1, mids and lows 66.7:1), "Below" = upward 4:1, very low thresholds, +5.2 dB input, +10.3 / +5.7 / +10.3 dB output. One **Depth** (dry / wet) and one **Time** knob | Three of the four quadrants: compression above, compression or expansion below. Expansion **above** the threshold (Ableton's "Restore Punch") is gfx-transient's job, and its curve cannot be inverted by the feedback characters. Amount scales slopes; Time scales times; upward compression; Ableton's default crossovers |
| [Slate MO-TT](https://docs.slatedigital.com/MO-TT/MO-TT.html) [V] | An "OTT" quick-set, "an exact recreation of the original OTT preset in Ableton Live"; Amount scales "all of the ratios, input, and output gain"; Timings 10 – 1000 %; per-band sidechain from the band or Wide; ratios "past inf:1"; clip per band. No noise floor control for the upward stage | Amount as the main knob; Band / Wide sidechain. Not taken: ratios past ∞ (non-monotonic curve, and our feedback characters need an invertible one) |
| Waves C4 / C6 ([manual mirror](https://manualmachine.com/wavesaudio/c4multibandcompressor/6482609-user-guide) [I]) | Threshold at the middle of a **Range** (gain change up or down, capped); behaviour Opto / Electro; ARC auto release | Range caps; program-dependent release (gfx-comp's `auto_release`) |
| [TDR Nova](https://docs.tokyodawn.net/nova-manual/) [V] | Parallel dynamic EQ (4 bands + a "W-band" for what the bands leave), compress or expand; the dry mix skips the HP / LP because their phase differs from the wet | The dynamic-EQ use (a band that works only when loud) is a 4-band preset here (§4); narrow bells stay with gfx-eq's de-harsh and gfx-deesser |
| [LSP Multiband Compressor](https://lsp-plug.in/?page=manuals&section=mb_compressor_mono) (GPL-3) [V] | Up to 8 bands; **Classic** = IIR with all-pass compensation, **Modern** = dynamic shelving pairs, **Linear Phase** = FFT FIR with latency; slopes 6 – 72 dB / oct; downward, upward, boosting; per-band sidechain filters, lookahead (reported, other bands delayed); solo / mute at −36 dB | Classic is our tree; the three-mode split confirms the trade-off. Not taken: more than 4 bands, lookahead per band (latency) |
| Line 6 Helix 3-Band Comp ([Sweetwater](https://www.sweetwater.com/insync/?p=232640) [I]) | Three bands, a band gain each; players use it as a crossover with the threshold at 0 dB | A guitar rack expects 3 bands; a band gain that works with no compression |
| Fractal Axe-Fx III Multiband Comp (release notes [I]) | 3 bands; mid-band gain "automatically compensated for crossover response"; Auto Attack / Release added in 12.07 | Confirms the crossover-sum problem and auto timing |
| Boss BC-1X (bass) ([OvniLab review](https://ovnilab.com/reviews/bc1x.shtml) [V]) | Multiband (band count not published), four knobs; low peaks no longer squash mids and highs: "brighter, more articulate". Downsides: "low end can sound tighter and less powerful", some low spikes pass with no lookahead | A four-knob face hiding the bands; `tilt` and auto gain against the "thin lows" complaint |
| EBS MultiComp ([SOS](https://www.soundonsound.com/node/4905536) [I]), Trace Elliot / Ashdown dual-band [I] | Analog dual-band bass comps; the EBS has internal trims for each band's sensitivity; the Trace low band's fixed slow attack lets slap peaks through | 2 bands are a 3-band preset with a band at ratio 1 |
| TC Electronic SpectraComp (bass) [I] | Multiband compressor in a one-knob pedal, voiced by downloadable presets | A preset bank under one knob is the pedal's whole interface |

### 1.2 Open source and papers

- **LSP** (above): Classic / Modern / Linear crossovers in one plugin, GPL-3, readable for ideas.
- **gfx-grit's prototype** (reviewed): the all-pass-compensated LR tree sums flat within 1e-6 dB, with band
  phases equal within 1e-15 rad. LR4 and LR8, analog and TPT.
- Linkwitz, *Active crossover networks for non-coincident drivers*, JAES 1976 [K]: LR low-pass and high-pass
  in phase, sum all-pass.
- Lipshitz & Vanderkooy, *A family of linear-phase crossover networks of high slope derived by time delay*,
  JAES 1983 [K], and Powell & Chau, *A technique for realizing linear phase IIR filters*, IEEE Trans. SP
  1991 [K]. Time-reversed IIR: an all-pass is undone by its time-reversed copy, at the cost of latency.
  Our Linear mode (§2.3) is this idea, applied once to the whole tree.
- Giannoulis, Massberg & Reiss, JAES 2012 [K]: the static curve and soft knee, as gfx-comp.

### 1.3 Forum feedback: what players love and hate

- **OTT abuse** ([KVR threads](https://www.kvraudio.com/forum/viewtopic.php?p=8354565),
  [review](https://www.kvraudio.com/product/ott-by-xfer-records/reviews/8457) [I]). It strips short-term
  dynamics and **raises the noise floor**. It is harsh in the high mids and "squeezed the bass out". The
  bands are pushed to extremes because the preset is extreme. Its defenders say it brings out low-level
  detail. Our answers: a noise guard on upward gain (none of the references surveyed has one), auto gain
  (no free loudness), Amount from 0 % up, and `delta` to hear what is being done.
- **Pumping and mud on bass** (OvniLab, mixing guides [I]). Fast low-band attacks pump; a tighter low band
  sounds "less powerful". Our answers: slow low-band defaults, ripple-free low-band detectors (§2.5),
  program-dependent release, and Tilt to give the lows back.
- **Phase and complexity** (FabFilter's own text, mixing guides [I]). Minimum-phase crossovers smear, and
  "use broadband unless you have a problem". Our answers: the phase cost is fixed and stated (§2.2), Linear
  is there when latency is allowed, and the face is two knobs.

### 1.4 What we conclude

1. The **LR tree with all-pass compensation** is the right split for zero latency. Its band gains act on
   magnitude only, so compression never adds phase modulation. That is better than "dynamic phase",
   which is flat at rest but moves its phase with the gain.
2. The fixed cost of zero latency is **low-frequency dispersion**, 4 – 5 ms at the defaults. Linear phase
   is the only way to remove it. It needs one FIR, not one per band, because every band carries the same
   all-pass.
3. **Upward compression is the sound of the category** (OTT), and the noise floor is its known flaw.
4. **The face must be two macros**, with auto gain making them safe to turn.
5. **Detection is where multiband goes wrong on bass**: detector ripple at the fundamental (with short
   releases) and threshold sag at the crossovers. Both are design items here, not tuning.

## 2. Design

```
in ─┬─▶ (Linear only) Linearizer: FIR = reversed all-pass of the tree, latency L ─┐
    │                                                                            ▼
    │   AUDIO: LR tree (LR4 / LR8, all-pass compensated)
    │     band b ─▶ × G_b(t) ─▶ × makeup_b · gain_b ─▶ (solo ramp) ─▶ Σ ─▶ wet
    │     Σ bands at unity (Minimum) or in · z^−L (Linear) ───────────────▶ dry (aligned)
    │
    └─▶ (Linear only: delay L) ─▶ DETECTION: Butterworth-4 tree (power complementary)
        band b ─▶ (or Wide: the whole signal) ─▶ gfx-comp detector + ballistics (character; gain only)
                                                    ─▶ g_dn,b  (downward, dB ≤ 0)
                                                 ─▶ Lift: upward / expansion below lift_thresh, guard ─▶ g_up,b
        G_b = 10^((g_dn,b + g_up,b) / 20)
wet, dry ─▶ mix ─▶ (delta: wet − dry) ─▶ × level ─▶ out
```

### 2.1 Bands and crossovers

- **`bands`**: 3 or 4. The ids are positional, as in gfx-grit: `lo_`, `lm_`, `hm_`, `hi_`, crossovers
  `x_lo`, `x_mid`, `x_hi`. With **3 bands**, `x_mid` and the `hm_` parameters are not used, and the bands
  are `lo` (below `x_lo`), `lm` (from `x_lo` to `x_hi`, shown as "Mid") and `hi`. So a 3-band preset is a
  4-band preset with one crossover removed, and its ids never change meaning.
- **Ranges** (linear Hz, contract): `x_lo` 40 – 500 Hz, `x_mid` 150 – 3,000 Hz, `x_hi` 800 – 12,000 Hz.
  Defaults are 120 Hz, 600 Hz and 2.5 kHz. The 3-band default is then 120 Hz / 2.5 kHz, near OTT's
  88 Hz / 2.5 kHz with the low band above the bass's fundamental zone. **Ordering**: each crossover is at
  least ×1.5 (0.58 octave) above the one below; the upper one yields. Shown values are the effective
  ones.
- **Smoothing**: crossovers in log-frequency over 20 ms, recomputed per sample. TPT sections stay stable
  under motion (gfx-dsp `svf`). The tree's identities need every section of one split to share a single
  coefficient set updated on the same sample (gfx-grit §2.2).
- **`slope`**: 24 dB / oct (LR4, default) or 48 dB / oct (LR8). This is a set-up switch: the other engine
  is pre-rolled for 30 ms, then crossfaded over 10 ms (gfx-grit's rule). **`bands`** switches the same
  way.
- **No oversampling, no ADAA, no colour**: every path is linear filters and gains at the base rate. The
  alignment problems the reviews found in gfx-grit (half-sample droop) and gfx-bassdi (colour's
  half-band pair on one path) **cannot occur here**. Only the engine's gain is used (§2.5), so FET, Opto
  and Pedal keep their dynamics and lose their shaper and Pedal its hiss. That is the price of
  coherence; a drive goes before or after the block.
- **The `crossover` module, as specified**: layer 1, the two-way split (gfx-bassdi's `Split`,
  gfx-grit's `LrSplit` with its all-pass twin: order 4 or 8, low, high and twin from shared f64 state,
  one coefficient set per split, log-smoothed frequency), and layer 3, the tree (`Tree` / `LrTree<N>`:
  band 0 = LP(x1) · AP(x2) · AP(x3), band 1 = HP(x1) · LP(x2) · AP(x3), band 2 = HP(x1) · HP(x2) ·
  LP(x3), band 3 = HP(x1) · HP(x2) · HP(x3)). Layer 2 (`BandGain`) is not used: a tree band takes a
  plain gain. The two documents name the layers differently; the module has one set of names, fixed
  by whichever crate is coded first. This crate adds two pieces to it (§2.9) and changes none.

### 2.2 What the tree guarantees, and what it costs

- **Phase is independent of the gains** [A, verified at review]. Band b = M_b(f) · A(f), with M_b real
  ≥ 0 and A = A(x_lo) A(x_mid) A(x_hi) the common all-pass. LR low-pass and high-pass are in phase, and
  the compensation all-passes make every band carry the same A. So for any gains g_b, Σ g_b · band_b
  = (Σ g_b M_b) · A.
  - Why: for one LR4 split, LP / A = 1 / (1 + ω⁴) and HP / A = ω⁴ / (1 + ω⁴) (ω⁸ for LR8): real,
    positive, summing to 1. A tree band's M_b is a product of such terms, and Σ M_b = 1. The output's
    magnitude is therefore a **weighted mean of the band gains**, with weights M_b: it lies **between
    the smallest and the largest band gain** at every frequency. No bump and no notch can form at a
    crossover, at any gains.
  - Prototype: 300 random gain sets per case, −30 to +12 dB, 3- and 4-band, LR4 and LR8, six crossover
    sets from (40, 60, 800 Hz) to (500, 3,000, 12,000 Hz), TPT forms at 48 kHz: M_b real within 1e-11
    and never negative; the magnitude inside [min g, max g] within 1e-7 dB; unity sum flat within
    1e-10 dB (LR4) and 1e-6 dB (LR8).
- **What this means for a compressor**: a band reducing by 6 dB makes a smooth shelf or a band-shaped dip
  whose shape is set by the crossover slope. It adds no phase shift and no phase modulation.
- **Gains that move** [review]. The gains multiply the bands **after** the filters, so no filter state
  is modulated and the identity above holds at every sample: a partial shared by two bands at a
  crossover is always the sum of two in-phase parts, whatever each gain does. What a moving gain adds
  is the side-bands of its own envelope, as in any compressor. Two things are specific to a split:
  - the side-bands are **not filtered back into the band**: a low band's gain stepping in 0.1 ms
    spreads a click's worth of spectrum over the kHz range, where the low band no longer masks it. The
    low band's default attack is 30 ms; the bench reports the splatter of a fast one (§5);
  - gain ripple at a low note's fundamental is the usual cause of "multiband mud": removed at the
    detector (§2.4).
- **The cost: low-frequency dispersion** [A], the group delay of A:

  | Crossovers | LR4: at 20 Hz / peak / at 1 kHz | LR8: at 20 Hz / peak / at 1 kHz |
  |---|---|---|
  | 3-band default (120 Hz, 2.5 kHz) | 4.0 / 4.7 ms at 77 Hz / 0.3 ms | 7.3 / 10.7 ms at 107 Hz / 0.5 ms |
  | 4-band default (120, 600, 2,500 Hz) | 4.8 / 5.5 ms at 78 Hz / 0.6 ms | 8.7 / 12.1 ms at 107 Hz / 1.1 ms |
  | Bass preset (100 Hz, 1.5 kHz) | 5.0 / 5.7 ms at 64 Hz / 0.4 ms | 9.0 / 13.0 ms at 89 Hz / 0.8 ms |
  | Lowest reachable (40, 150, 800 Hz) | 16.9 / 17.2 ms at 26 Hz / 0.5 ms | 30.4 / 37.8 ms at 36 Hz / 1.1 ms |

  At the defaults this is one loudspeaker crossover's worth, rarely audible through a cab [I: bench, as
  gfx-grit]. Two consequences are stated, not hidden:
  - at Amount 0 the output is the all-pass, **not a null against the input** (`bypass` is the null:
    §2.7);
  - in a **golem split lane** summed against a dry branch, it combs at the crossovers. Use `mix` inside
    the block, or Linear mode.
- **Band interaction** (inherent, measured on the bench §5):
  - *Leakage into a neighbour's detector*: a partial one octave from a crossover reaches the other band's
    detector at −24 dB (both LR4 audio and the BW4 detection tree; with LR8 the detector stays at
    −24 dB, §2.4). A loud bass note still triggers the low-mid band, 24 dB less.
  - *Threshold sag at a crossover*: a partial exactly at f_c is split between two bands (§2.4).
  - *A band's makeup or boost* raises the shoulders of its neighbours' slopes. That is the shelf shape
    above, monotone.

### 2.3 Linear mode (`phase` = Linear)

- **One FIR for the whole tree.** Every band carries the same A, so filtering the input once by
  z^−K · A^−1 makes every band zero-phase: out = z^−L · Σ G_b M_b x, at any band gains. A^−1 is
  anti-causal (A's poles mirrored). Its truncation is the **time-reversed impulse response of A**, cut at
  K samples, with a 10 % half-Hann fade on the far tail (Powell–Chau's idea, done once for all bands).
- **Latency L = 2,048 samples at 44.1 and 48 kHz** (42.7 ms at 48 kHz, 46.4 ms at 44.1 kHz), 4,096 at
  88.2 and 96 kHz, 8,192 at 176.4 and 192 kHz: one size per rate family, so the convolution keeps its
  partition count. L = K + B, the kernel's length plus the convolution block (B = 64, 128, 256). This is
  the same order as the catalogue's Spectral Comp window (43 ms). Reported with `set_latency_samples`.
  Changing `phase` changes the reported latency, so it is a **set-up switch**: the host re-syncs once,
  and golem's note says "set before playing" (gfx-deesser's and gfx-limiter's lookahead precedent).
- **A section is in the kernel only if its tail fits** [A, review]. A truncated all-pass is not an
  all-pass: its magnitude ripples. LR4's section (Q 0.707) fits at every crossover of the range. LR8's
  second section (Q 1.307) rings 1.8 × longer: with `x_lo` at 40 Hz the draft's kernel measured
  +0.8 / −2.3 dB in the lowest octaves, worse than Minimum mode, which is flat. Rule: **LR8's Q 1.307
  section of a crossover below 80 Hz stays out of the kernel** and remains a minimum-phase all-pass.
  The magnitude is then flat within ±0.04 dB at every setting, and that one section's dispersion
  remains (below). The kernel is crossfaded as usual when `x_lo` crosses 80 Hz.
- **Residual** [A, re-run at review] (max |FIR · A − z^−K|, 20 Hz – 20 kHz, 48 kHz, K = 1,984), and the
  FIR's magnitude error:

  | Crossovers | LR4 | LR8 |
  |---|---|---|
  | 3-band default (120, 2,500) | −169 dB | −82 dB |
  | 4-band default (120, 600, 2,500) | −165 dB | −80 dB |
  | 100, 1,500 (bass preset) | −138 dB | −67 dB |
  | 80, 600, 2,500 | −108 dB | −47 dB (±0.04 dB) |
  | `x_lo` 60 Hz | −80 dB | Q 1.307 left minimum-phase: 5 ms at 20 Hz, 14 ms peak at 55 Hz remain; the rest −101 dB |
  | `x_lo` 40 Hz | −53 dB (±0.02 dB) | Q 1.307 left: 11 ms at 20 Hz, 22 ms peak at 37 Hz remain (Minimum: 25 / 32 ms); the rest −65 dB |
  | Lowest reachable (40, 150, 800) | −49 dB (±0.03 dB) | as above; the rest −60 dB (±0.01 dB) |

  So Linear is exact for LR4 everywhere and for LR8 from 80 Hz up; below, LR8 keeps part of its lowest
  crossover's dispersion and a flat magnitude. It is never worse than Minimum mode. A kernel twice as
  long (85 ms) would correct LR8 down to 40 Hz at −47 dB; it is not taken (open question 2), and `phase`
  can grow a third option later.
- **Convolution**: a uniformly partitioned FFT convolution (block B, 32 partitions), using `realfft`,
  which gfx-dsp already depends on. When a crossover moves, the new kernel is rendered (run the kernel's
  sections on an impulse for K samples, reverse, fade, transform the partitions) **across the following
  audio blocks**, at most once per 20 ms. Old and new convolutions are then crossfaded over 20 ms.
  Everything is preallocated for 192 kHz in `prepare`.
- **Detection is not linearized** [review]. The detection tree reads the **input delayed by L**, not
  the FIR's output. The FIR is an all-pass: it changes the waveform's crest, so a detector behind it
  would read other peaks, and the two modes would not compress alike. With the plain delay, the band
  gains are the same sequences as in Minimum mode, L samples later. The only difference left is timing:
  in Minimum the audio lags its gain by A's group delay (0.2 – 5 ms); in Linear they are aligned.
- **What Linear changes, and not**: the dynamics are identical; only the phase changes. The dry of
  `mix` and `bypass` is the input delayed by L. The known trade-off of linear-phase multiband is that
  **a compressing band pre-rings**: its zero-phase band shape is symmetric in time. It is audible on
  transients when bands differ by many dB (bench report). That is why Minimum stays the default,
  besides latency.

### 2.4 Detection

- **A separate, cheap detection tree**: a Butterworth-4 split at each crossover (two TPT SVF sections,
  Q 0.541 and 1.307), for both slopes. It is **power complementary**, |LP|² + |HP|² = 1, and so is the
  whole tree (band powers sum to the input's power within 1e-7 [A]); LR4 is amplitude complementary.
  Cost: 3 SVF ticks per split (the first section gives both LP and HP), so 9 for 4 bands [A].
  - A partial at a crossover reads **−3.0 dB** in each band, against −6.0 dB with the LR bands. The
    threshold sag at the crossovers is halved, and the band powers of broadband material add up to its
    power.
  - Selectivity matches the LR4 audio tree: −24.1 dB one octave away (LR4 −24.6 dB). The band edges
    differ by 3 dB at the crossover and by 0.5 dB an octave away: the detector hears a little more of
    its edges than the band carries, which is the intent.
  - With **LR8** the detector is wider than its band: one octave away it reads −24 dB where the band
    carries −48 dB. A band can then react to a neighbour's note that it barely passes, 24 dB down.
    Accepted: a Butterworth-8 tree (21 ticks) would match it, and is the upgrade if the bench hears it.
- **`<b>_sc`**: **Band** (default: the band's own detection signal) or **Wide** (the whole input, before
  the split). Wide is MO-TT's mode ("the detection signal is the wideband input signal") [V]: a band
  ducks when the whole signal is loud, e.g. the low band of a bus under a kick.
- **Ripple-free on bass, without slowing the detector** [A, review: `det.py`]. gfx-comp's detector is
  calibrated to RMS: a 5 ms sliding peak (−3 dB) and a one-pole RMS (10 ms; 5 ms in Bus). In a low band
  both ripple at the fundamental. The damage depends on the release: on a 41 Hz E at ratio 4, 6 dB of
  reduction and attack 1 ms, the gain ripples by 0.03 – 0.05 dB at release 250 ms (3rd harmonic
  −61 to −55 dBc) and by 0.27 – 0.5 dB at release 20 ms (**−41 to −35 dBc**: the "mud"). A longer RMS
  cures the ripple and breaks the attack: with a 67 ms RMS the reduction takes 113 ms to reach 90 %,
  against 16 ms. So each band applies **gfx-gate's rule to both readings**:
  - W_b = clamp(1.2 / f_low,b, 5 ms, 40 ms): the window of gfx-gate and gfx-transient;
  - **peak** = sliding maximum of |x| over W_b;
  - **RMS** = the engine's one-pole RMS, unchanged, then a **sliding maximum over W_b**. Over one
    period the maximum of a periodic reading is constant;
  - f_low = 30 Hz for `lo` (W = 40 ms), `x_lo` for `lm` (10 ms at 120 Hz), `x_mid` or `x_lo` for `hm`,
    `x_hi` for `hi` (5 ms, gfx-comp's own).

  Measured: gain ripple 0.000 dB and no added harmonic (≤ −160 dBc) at any attack and release, on a
  sine and on an asymmetric note; 90 % of the reduction in 16 ms (14 ms in Bus), as today.
  The costs, stated:
  - a **hold of W** before the release starts (40 ms in the low band);
  - the RMS reads the top of its own ripple: **+0.8 dB at 41 Hz, +1.0 dB at 31 Hz** (+1.3 / +1.6 dB in
    Bus), under 0.25 dB above 150 Hz. Today's engine with a fast attack reads the same;
  - a second hit within W of a louder one is not seen as new (gfx-transient §2.2).

  This is a gfx-comp library setting, `detector_low_hz: Option<f32>` (`None` = today's detector, bit
  for bit; gfx-comp itself does not change). Attack on the low band is bounded by physics at about half
  a period; the low band's default attack is 30 ms anyway.
- **Stereo**: per-channel detection, `link` 0 – 100 % as gfx-comp, per band.

### 2.5 Gain per band: gfx-comp downward + Lift

- **Downward (above `<b>_thresh`)**: the **gfx-comp engine** per band, character from the global
  `character`. It has gfx-comp's static curve (thresh, ratio, knee), RMS calibration, ballistics in dB,
  `auto_release`, and the feedback characters' implicit solve. Two library changes are needed:
  1. a public **gain API**: `next_gain_db(sc: &[f32]) → [g; channels]` (dB ≤ 0). It advances the
     engine's clock and smoothers by one sample, runs the detector on each channel's sidechain sample
     and applies `link`. No audio passes through the engine, so there is **no colour by
     construction**: gfx-bassdi's `colour: false` is for callers of `process_block`, and is not needed
     here. `process_block` is rewritten on top of this call, so there is one detector code path;
  2. `detector_low_hz` (§2.4).
  gfx-comp's own tests must still pass unchanged, and its renders must null bit-exact against today's.
  The engine receives effective values: thresholds after Tilt (held in −60 – 0 dB), ratio after Amount
  (≤ 30:1), times after `time` (attack 0.02 – 1,200 ms, release 5 – 12,000 ms).
- **Lift (below `<b>_lift_thresh`)**: new, feed-forward in every character. It reads the larger of the
  band's two readings of §2.4 (peak − 3 dB, RMS), so it sees an onset at once.
  - **`<b>_lift`** is the slope k, in % (dB of gain per dB below the threshold T_l); 0 = off:
    - k > 0: **upward compression**, a boost. +75 % is OTT's 4:1 (k = 1 − 1 / ratio); +100 % brings
      everything under T_l up to T_l, within the range.
    - k < 0: **downward expansion**, a cut. −100 % is 1:2 (−1 dB per dB below T_l).
  - Static gain, x the level in dB:
    - boost: g_up = max(0, min(k · (T_l − x), `lift_range`, x − `guard`));
    - cut: g_up = max(k · (T_l − x), −`range`), no guard (cutting noise is harmless).
  - **Noise guard**: the third term is gfx-transient's floor bound (§2.4 there): **a boost never exceeds
    the signal's height above `guard`**. Noise at or under `guard` gets exactly 0 dB, whatever
    `lift_range` is; a tail is lifted less and less as it sinks toward the guard. The law is continuous
    (a minimum of three lines), and the output level stays non-decreasing in the input level: slopes
    1 − k (lifting), 1 (at the range), 2 (the guard taking the boost back), 1 (under the guard).
  - What the guard does **not** do: the make-up still raises the noise with everything else. In the
    Hyper preset the make-up is +10 dB, so hiss comes out 10 dB up; without the guard it would be 28 dB
    up. To push noise back under its input level, use a negative `lift` on that band or gfx-gate
    before the block.
  - The guard is an absolute level, one for all bands, as gfx-gate's threshold: a preset is set for a
    lane's noise. gfx-transient's relative floor (phrase level − 48 dB) is the alternative if presets
    prove too level-dependent (bench).
  - **Ballistics**: Lift has its own state and the band's times: **attack moves the gain down, release
    moves it up** (no program-dependent release). A boost leaves with the attack time when a note
    hits. Within that time the onset keeps part of the boost: at the default attacks (8 – 30 ms) a
    pick transient after a quiet passage comes out lifted. That is the snap of the OTT sound, whose
    attacks are tens of milliseconds [U]. With attack ≤ 2 ms the transient is not inflated (§5).
  - `lift_thresh` is held ≤ `thresh` (pushed down with it).
- **G_b** = 10^((g_dn + g_up) / 20) · makeup_b · gain_b, per sample (gfx-comp's `gain()`: exactly 1.0 at
  0 dB). g_dn + g_up is held ≥ −`range`.

### 2.6 The face: Amount, Tilt, Time, auto gain

- **`amount`** (0 – 200 %, 100 % = the bands as set) scales every band's **slopes**, as Ableton's Amount
  scales ratios (Ableton's stops at 100 %; MO-TT's goes past) [V]:
  - downward: (1 − 1/r′) = a · (1 − 1/r), with a = amount / 100, held ≤ 1 − 1/30. The engine simply
    gets the ratio r′ ≤ 30:1, gfx-comp's own maximum, so no engine change is needed. A true ∞:1 is not
    given to the engine: its feedback characters compute (1 − r) · (y − T), which has no value at ∞.
    30:1 differs from ∞:1 by 0.7 dB, 20 dB above the threshold. A band at the default 2.5:1 reaches
    30:1 at Amount 161 %.
  - Lift: k′ = a · k, held in −200 – +100 %.
  - 0 % = every band at unity: the flat all-pass, at any band settings. EXP target.
- **`tilt`** (−100 – +100 %) shifts both thresholds of band b by 12 dB · (tilt / 100) · w_b, with
  w = (−1, −⅓, +⅓, +1) for (lo, lm, hm, hi). In 3-band, w = (−1, 0, +1). It is the catalogue Spectral
  Comp's threshold slope with the same sign, as a macro.
  - **Positive = the lows are held, the highs are free**: the lows' thresholds go down (more
    compression there) and the highs' go up.
  - **Negative = the highs are held, the lows are free**: tame fizz and pick click, give the low end its
    dynamics back (the BC-1X complaint).
  - **It is not a tone control** [review]. With auto gain on, each band's make-up is computed after
    Tilt, so at the playing level every band still comes out at unity. Tilt +100 % at the defaults:
    the low band is +6.8 dB at 12 dB under the playing level and −7.2 dB at 12 dB over; the high band
    is within 0.5 dB of unity over that span. What moves is **where the squeeze is**: at positive Tilt
    loud playing sounds brighter and quiet playing fuller. For a static tone tilt, use the
    `<b>_gain`s. EXP target.
- **`time`** (10 – 400 %) scales every attack and release (MO-TT, Ableton). Pedal's 5 ms attack cap
  still applies.
- **`auto_gain`** (on by default). Each band's makeup is the opposite of its static gain change at a
  reference level R:
  - makeup_b = −(g_dn,b(R) + g_up,b(R)), after Amount and Tilt;
  - R = −24 dBFS RMS per band [A]: pink noise at −18 dBFS (gfx-comp's calibration) puts −22 to −25 dBFS
    in each band at the default crossovers, 3 or 4 bands. A band on **Wide** uses R = −18 dBFS, the
    level its detector reads;
  - clamped to ±24 dB and smoothed over 50 ms in dB.

  It is **static**: no breathing, no pumping, no gain chasing silence. At playing level the loudness
  holds while Amount, Tilt or a ratio moves; quieter and louder playing differ less, which is the
  effect. R is a bench constant, set from the DIs (as gfx-grit's reference level). A band that plays
  far from R (a bass DI's top band, 15 dB lower) is matched less well; `<b>_gain` corrects it in the
  preset.
  - Why here and not in gfx-comp: gfx-comp stands for a catalogue block whose presets set `makeup` by
    ear. gfx-mbcomp is new, its presets are written with auto gain, and the face cannot be safe
    without it.
- **`<b>_gain`** (−24 – +24 dB) is a per-band output trim on top: OTT's per-band output, and the Helix
  "band gain as a crossover EQ". It is a static band level, so it is phase-free (§2.2).

### 2.7 Output, listen, meters

- **`mix`** (shared id; law: **crossfade**, dry and wet in phase). The dry is the tree's unity sum
  (Minimum) or the input delayed by L (Linear), so it is coherent with the wet at any setting. OTT's
  Depth.
- **`delta`**: hear wet − dry, what the dynamics add or remove, at mix 100 %. This is the diagnostic
  against OTT abuse.
- **`solo`**: Off · Low · Low Mid · High Mid · High. One band is heard, after its gain, with a 5 ms
  raised-cosine ramp. In 3-band, Low Mid and High Mid both solo the Mid band. A separate sidechain
  listen is not added: Band detection is that same band (BW4 instead of LR).
- **No per-band bypass.** A band at ratio 1 and Lift 0 % passes at unity, and `delta` shows what a
  band does. Parameters can be added later, never removed (gfx-grit's per-band mute, same decision).
- **`level`**: −24 – +12 dB after the mix.
- **Meters** (read-only, contract):
  - `gr`: the largest band reduction, ≥ 0 dB (shared id, golem's meter: its meaning is unchanged);
  - `lo_dyn`, `lm_dyn`, `hm_dyn`, `hi_dyn`: each band's **dynamic gain** g_dn + g_up in dB, with the
    sign of a gain: negative = reduction, positive = Lift's boost. −60 – +24 dB. Make-up and
    `<b>_gain` are not in it. In 3-band, `hm_dyn` reads 0.

  Per block, each reports the value farthest from 0 (as gfx-comp's peak-held `gr`).
- **`bypass`**: nih-plug `make_bypass`, 10 ms crossfade to the **input** (delayed by L in Linear), not
  to the tree's unity sum: bypass is a true null. During the 10 ms the input and the all-passed wet
  overlap; it is too short to hear as a comb (bench: Clicks). There is no tail, so no trails.

### 2.8 Latency and CPU

- **Latency**:
  - Minimum: **reported 0**. No delay line; the output carries the all-pass dispersion (§2.2). gfx-bench
    gets `skip_latency` (gfx-grit's and gfx-octaver's precedent for all-passes).
  - Linear: **reported L** (2,048 samples at 44.1 / 48 kHz, 4,096 at 88.2 / 96 kHz, 8,192 at 176.4 /
    192 kHz), and the impulse peaks at L.
- **CPU** [A, mono, 48 kHz; an SVF tick ≈ 4 ns, gfx-comp ≈ 0.1 µs measured for a whole instance]:

  | Case | Audio tree | Detection tree | Band engines (gfx-comp + Lift) | Mix, makeup, meters | Total |
  |---|---|---|---|---|---|
  | 3-band, LR4, Minimum | 7 ticks, 28 ns | 6 ticks, 24 ns | 3 × 90 ns | 20 ns | **≈ 350 ns** |
  | 4-band, LR4, Minimum | 12 ticks, 48 ns | 9 ticks, 36 ns | 4 × 90 ns | 20 ns | **≈ 460 ns** |
  | 4-band, LR8, Minimum | 27 ticks, 108 ns | 36 ns | 360 ns | 20 ns | **≈ 520 ns** |
  | + Linear | | | | FFT convolution ≈ 80 ns (two 128-point FFTs and 32 × 65 complex products per 64 samples); kernel render spread over blocks | **≈ 600 ns** |

  The band engines include the second sliding maximum of §2.4 (amortised O(1), ≈ 5 ns). Stereo doubles
  it. Bound for the bench: 0.6 µs mono (4-band LR4 Minimum), 0.8 µs Linear. That is 3 – 4 % of a core,
  the order of gfx-drive.

### 2.9 Reuse and what is new

| Piece | From | Use here |
|---|---|---|
| `Comp` engine, characters, static curve, `auto_release`, RMS calibration | gfx-comp (library) | Downward per band. **Changes**: public `next_gain_db` (§2.5; `process_block` rebuilt on it) and `detector_low_hz: Option<f32>` (§2.4). `colour: false` (gfx-bassdi, gfx-acoustic) is not needed here. gfx-comp's tests unchanged, renders bit-identical |
| `crossover` layers 1 and 3 (the split with its twin, the tree: §2.1) | gfx-dsp (new, shared with gfx-bassdi and gfx-grit; written before either, with the identities as unit tests: gfx-bassdi §2.9) | Audio tree, LR4 / LR8, unity sum = dry |
| **New in `crossover`**, layer 4: `BwTree<N>` (power-complementary detection tree) and `Linearizer` (kernel from the tree's all-pass sections with the fit rule of §2.3, partitioned convolution) | — | Detection; Linear mode. Unit tests: band powers sum to 1; FIR · A = z^−K within the §2.3 table. The `Linearizer` also serves gfx-grit and gfx-bassdi if they ever want a linear option |
| `svf`, `smooth`, `exp_smooth`, `envelope::SlidingMax` | gfx-dsp | Sections, parameter smoothing (EXP on `amount`, `tilt`, `mix`, `level`), peak and RMS windows |
| `realfft` | gfx-dsp dependency | Linear-mode convolution |
| **New in the crate** | — | Lift (upward / expansion with guard), Amount / Tilt / Time mapping, auto gain, solo / delta, meters |

## 3. Parameters

All ranges linear (contract). No default depends on another parameter. The fresh plugin is a gentle
3-band leveller: Clean character, every band at −24 dB and 2.5:1, Lift off.

### 3.1 Front panel

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `amount` | Amount | f | 0 – 200 % | 100 % | 20 ms | ✓ | Scales every band's slope (down and Lift); 0 = flat; held at 30:1 (§2.6) |
| `tilt` | Tilt | f | −100 – +100 % | 0 % | 20 ms | ✓ | ±12 dB of threshold on the outer bands; + = the lows are held and the highs free, − = the reverse (§2.6) |
| `time` | Time | f | 10 – 400 % | 100 % | 50 ms (log) | | Scales every attack and release |
| `mix` | Mix | f | 0 – 100 % | 100 % | 20 ms | ✓ | Crossfade with the aligned dry (OTT's Depth) |
| `level` | Level | f | −24 – +12 dB | 0 dB | 20 ms | ✓ | Output |
| `character` | Character | e | Clean · Bus · FET · Opto · Pedal | Clean | gfx-comp's crossfade | | gfx-comp's dynamics families, no colour, all bands |

### 3.2 Bands, × 4: `lo_`, `lm_`, `hm_`, `hi_`

Names "Low …", "Low Mid …" (shown "Mid" in 3-band), "High Mid …", "High …".

| id | Name | Type | Range | Default (lo / lm / hm / hi) | Smooth | Role |
|---|---|---|---|---|---|---|
| `<b>_thresh` | \<Band\> Threshold | f | −60 – 0 dB | −24 dB | 20 ms | Downward threshold, RMS-calibrated |
| `<b>_ratio` | \<Band\> Ratio | f | 1 – 30 :1 | 2.5 | 20 ms | Downward, gfx-comp's meaning |
| `<b>_lift_thresh` | \<Band\> Lift Threshold | f | −80 – 0 dB | −48 dB | 20 ms | Below it, Lift works; held ≤ `<b>_thresh` |
| `<b>_lift` | \<Band\> Lift | f | −100 – +100 % | 0 % (off) | 20 ms | Slope below the Lift threshold: + = upward compression (+75 % = 4:1), − = downward expansion (−100 % = 1:2) (§2.5) |
| `<b>_attack` | \<Band\> Attack | f | 0.1 – 300 ms | 30 / 20 / 12 / 8 ms | 50 ms (log) | Gain moving down; × `time` |
| `<b>_release` | \<Band\> Release | f | 10 – 3,000 ms | 250 / 180 / 150 / 120 ms | 50 ms (log) | Gain moving up; × `time`; fast stage under `auto_release` |
| `<b>_gain` | \<Band\> Gain | f | −24 – +24 dB | 0 dB | 20 ms | Band trim on top of the makeup |
| `<b>_sc` | \<Band\> Sidechain | e | Band · Wide | Band | — | §2.4 |

The defaults differ per band but are constants: this is not a mode-dependent default.

### 3.3 Expert (global)

| id | Name | Type | Range | Default | Smooth | Role |
|---|---|---|---|---|---|---|
| `bands` | Bands | e | 3 · 4 | 3 | pre-roll 30 ms + 10 ms crossfade | §2.1 |
| `x_lo` | Low Crossover | f | 40 – 500 Hz | 120 Hz | 20 ms, log | lo / next |
| `x_mid` | Mid Crossover | f | 150 – 3,000 Hz | 600 Hz | 20 ms, log | lm / hm (4-band only); ≥ 1.5 × `x_lo` |
| `x_hi` | High Crossover | f | 800 – 12,000 Hz | 2,500 Hz | 20 ms, log | last split; ≥ 1.5 × the one below |
| `slope` | Crossover Slope | e | 24 dB/oct · 48 dB/oct | 24 | pre-roll + crossfade | LR4 / LR8 |
| `phase` | Phase | e | Minimum · Linear | Minimum | latency change, 20 ms crossfade | Linear: reported 42.7 ms at 48 kHz (§2.3) |
| `auto_gain` | Auto Gain | b | off / on | on | 50 ms | Static makeup at the reference level (§2.6) |
| `auto_release` | Auto Release | b | off / on | on | — | gfx-comp's program-dependent release |
| `knee` | Knee | f | 0 – 24 dB | 6 dB | 20 ms | gfx-comp's quadratic knee, every band, both thresholds |
| `range` | Range | f | 0 – 60 dB | 24 dB | 20 ms | Maximum cut per band (downward + expansion) |
| `lift_range` | Lift Range | f | 0 – 24 dB | 12 dB | 20 ms | Maximum boost per band |
| `guard` | Noise Guard | f | −96 – −36 dBFS | −72 dBFS | 20 ms | Nothing at or under it is lifted; a boost never exceeds the level's height above it (§2.5) |
| `link` | Stereo Link | f | 0 – 100 % | 100 % | 20 ms | Stereo layout only |
| `solo` | Solo | e | Off · Low · Low Mid · High Mid · High | Off | 5 ms | Hear one band (§2.7) |
| `delta` | Delta | b | off / on | off | 10 ms | Hear wet − dry |
| `bypass` | Bypass | b | off / on | off | 10 ms | nih-plug `make_bypass` |

Read-only meters: `gr`, `lo_dyn`, `lm_dyn`, `hm_dyn`, `hi_dyn` (§2.7).

**54 parameters + 5 meters** (61 before the review: per-band solo and bypass became one `solo`). 32 of
them are the 4 × 8 band parameters. golem's Designer shows the face, then the expert groups
(`visuals/data/groups.json`):

- **Low**, **Low Mid**, **High Mid**, **High**: each band's 8 parameters;
- **Crossovers**: `bands`, `x_*`, `slope`, `phase`;
- **Dynamics**: `auto_gain`, `auto_release`, `knee`, `range`, `lift_range`, `guard`, `link`, `solo`,
  `delta`.

Ids are permanent once shipped; enums only grow at the end. Room kept for later, by appending:
`<b>_char` (first option Global), a third `phase` option, per-band bypass.

## 4. Applications

Reference settings become block presets (`presets/gfx-mbcomp.json`, the presets/README format),
level-matched by auto gain. No artist or trademark names. Unlisted values are the defaults.

| Use | Settings to start from | Feature that matters |
|---|---|---|
| **Even clean guitar** (default) | 3-band, Clean, all −24 dB 2.5:1, Amount 100 % | Strums do not pump the highs; the lows do not dull the attack |
| **Tight chug** (high gain, after amp and cab) | 3-band, `x_lo` 150 Hz, lo −28 dB 4:1, attack 8 ms, release 120 ms; other bands 1.5:1; Tilt +20 % | Palm-mute boom held; the riff's top stays open. Ripple-free low detector: no added fuzz on the boom |
| **Acoustic piezo boom** | 4-band, `x_lo` 180 Hz, `x_mid` 700 Hz; lm −30 dB 3:1 (only when it booms); hi Lift +50 % below −42 dB | Dynamic-EQ use: the body resonance is cut only when it blooms |
| **Bass, even strings** (BC-1X use) | 3-band, `x_lo` 100 Hz, `x_hi` 1.5 kHz; FET; lo −26 dB 4:1 attack 30 ms release 250 ms; Mid −24 dB 3:1; hi 2:1 attack 2 ms; Tilt −15 % | Low peaks no longer squash the mids; negative Tilt leaves the lows more of their dynamics, the answer to the "tighter, less powerful" complaint |
| **Slap tamer** | Bass preset, hi −30 dB 6:1, attack 0.5 ms, release 80 ms | Pops caught without dulling the fingers |
| **Low anchor, free top** | 3-band, `x_lo` 120 Hz, lo 6:1 Opto, Mid and hi ratio 1 | The Parallax-style clean, steady bottom; the rest untouched |
| **Vocal, levelled** | 4-band, `x_lo` 150, `x_mid` 600, `x_hi` 4.5 kHz; Opto; lm −26 dB 3:1 (de-mud when loud); hm 2:1; hi −30 dB 4:1 attack 1 ms (de-harsh); Tilt 0 | Mud and harshness handled only when they occur; gfx-deesser still does the "s" |
| **Vocal presence lift** | as above, hm Lift +67 % below −40 dB, `guard` −66 dBFS | Quiet syllables come forward; breaths and room noise are not raised |
| **Keys / backing bus glue** | 3-band, Bus, all −20 dB 2:1, attack 30 ms, release 300 ms; `phase` Linear | Linear: no low-end smear on a non-live lane (latency reported) |
| **Hyper** (OTT-style, sound design) | 3-band, `x_lo` 90 Hz, `x_hi` 2.5 kHz; ratio 30 on all; thresholds −34 / −30 / −36 dB; Lift +75 % below −42 dB; `lift_range` 18 dB; Time 50 %; `mix` 100 % | The category's signature; auto gain keeps it level-matched, the guard keeps the Lift off the hiss (the make-up still raises it by about 10 dB: §2.5) |
| **Hyper, tasteful** | Hyper with Amount 40 % or `mix` 30 % | The usual way OTT is used (20 – 40 % Depth) |
| **Pad swell / breath** | 4-band, ratio 1 everywhere, Lift +67 % below −30 dB, attack 60 ms, release 800 ms, `guard` −76 | Upward only: decays and reverb tails bloom, every band evenly |
| **De-bleed** (live vocal mic) | 4-band, lo and hi Lift −100 % (expansion 1:2) below −45 dB, `range` 12 dB | Downward expansion: cymbal and stage rumble recede between phrases |
| **Weight shift by foot** | any preset, EXP on `tilt` (−60 → +60 %) | Heel: the highs are held and the lows breathe; toe: the lows are pinned and the highs open up. The playing level does not move (auto gain); it is not a tone sweep (§2.6) |

## 5. Bench suite (on top of gfx-bench's common checks)

**gfx-bench options**: `skip_latency` in Minimum (all-pass dispersion, reported 0). Linear is checked by its
own row. `click_bound_db` default. The [A] bounds come from the prototype; the bench re-measures them
and then fixes them.

| Check | Pass |
|---|---|
| **Flat sum** | Amount 0 %, and separately all ratios 1 with Lift off. Every slope, 3 and 4 bands, crossovers at defaults, at minimum spacing (× 1.5) and at both ends: magnitude 20 Hz – 20 kHz flat ±0.001 dB; output nulls ≤ −90 dB against the f64 all-pass reference A(x)·in |
| **Phase independent of gain** | `<b>_gain` set to 50 random sets (−24 – +24 dB), no dynamics: output nulls ≤ −90 dB against Σ g_b band_b (f64); magnitude between min and max g_b at every frequency |
| **Static curve per band** | 1 kHz sine in a 4-band mid band, level ramp, every character: downward as gfx-comp ±0.1 dB; Lift slopes +75 % and −100 % ±0.1 dB; `lift_range` and `range` caps exact; guard: boost = level − `guard` ±0.2 dB where that is the smallest term, 0 dB at and under `guard`; output level non-decreasing in input level |
| **Threshold sag** | Sine sweep 20 Hz – 20 kHz at a band threshold + 6 dB, ratio 4: gain reduction vs frequency; at each crossover ≥ (6 − 3.0) · 0.75 dB ±0.3 dB (the BW4 −3 dB reading); report the LR4-detection figure for comparison |
| **Neighbour leakage** | 41 Hz sine at −10 dBFS: `lm` reduction ≤ 0.1 dB with `lm` threshold −24 dB |
| **Bass ripple** | 41.2 Hz and 30.9 Hz sines and an asymmetric note (f + 0.6 · 2f + 0.3 · 3f), lo band ratio 4, attack 1 ms, release 20 ms and 250 ms, reduction 6 dB, Clean, Bus and FET: harmonics added ≤ −90 dBc (prototype ≤ −160; gfx-comp's own windows give −35 to −61 dBc [A]); 90 % of the reduction within 20 ms of a +12 dB step; reading of a 41 Hz sine within +1 dB of its RMS (+1.5 dB in Bus) |
| **Upward vs transient** | Pick-attack DI after 1 s at −60 dBFS, into hi Lift +75 %: with attack ≤ 2 ms the attack's peak rises ≤ 1 dB above its unprocessed peak + makeup; at the default 8 ms: reported (the boost leaves with the attack time) |
| **Noise guard** | −80 dBFS pink noise, Hyper preset, `guard` −72: every band's `<b>_dyn` = 0 dB and output noise = input + make-up ±1 dB (Lift adds nothing); with `guard` −96: reported (about 18 dB more). Decaying note: boost ≤ level − `guard` + 1 dB at every instant |
| **Amount / Tilt / auto gain** | Amount 0 → 200 % and Tilt −100 → +100 % on pink noise whose bands sit at R: loudness within ±1.5 LU; Tilt ±100 %: every band's net gain at R within ±0.5 dB of 0; Amount 100 % and Tilt 0 bit-identical to the bands alone; effective ratio never past 30:1 (output monotone in input level, every character) |
| **Linear mode** | Reported latency = L (2,048 at 44.1 / 48 kHz, 4,096 at 88.2 / 96 kHz, 8,192 at 176.4 / 192 kHz); impulse peak at L; 4-band default LR4 and LR8: magnitude flat ±0.01 dB, group delay L ± 0.1 sample from 30 Hz; **every corner of the crossover ranges, both slopes: magnitude flat ±0.05 dB** (the fit rule); residual vs z^−L ≤ the §2.3 table + 6 dB; dynamics identical to Minimum (the band gains are Minimum's, L samples later: null ≤ −80 dB after the shift); pre-ringing of a compressed band on a snare: reported |
| **Mix / delta** | `mix` 50 % at every setting: no notch > 0.2 dB (dry aligned); `delta` at Amount 0 → silence ≤ −120 dBFS |
| **Clicks** | Crossover sweeps by EXP 50 Hz steps (sidebands ≤ −80 dB, contract), `x_lo` across 80 Hz in Linear LR8, `bands` 3 ↔ 4, `slope`, `phase`, `character`, `solo`, `bypass`, Amount 0 ↔ 200 % under a held 82 Hz note: HF burst below the default click bound; no gap |
| **Attack splatter** | 82 Hz tone bursts, lo band 6 dB of reduction, attack 0.1 ms, 1 ms and 30 ms: energy above 1 kHz reported for each (the side-bands are not filtered back into the band, §2.2) |
| **Bypass null** | `bypass` on: output = input bit-exact (Minimum), = input delayed by L (Linear) |
| **gfx-comp unchanged** | gfx-comp's tests and bench renders null bit-exact against the build before `next_gain_db` and `detector_low_hz` |
| **Stereo** | 1 / 1 and 2 / 2 with equal inputs: L = R bit-exact; `link` 0 % on L-only content leaves R unprocessed |
| **Meters** | `<b>_dyn` equals the applied g_dn + g_up ±0.1 dB, negative for a reduction, positive for a boost; `gr` = the largest band reduction, ≥ 0 |
| **Determinism** | Two renders null bit-exact (no random source) |
| **CPU** | ≤ 0.6 µs / sample mono, 4-band LR4 Minimum; ≤ 0.8 µs Linear, 48 kHz |
| **Listening** | clean.wav, crunch through amp + cab, bass DI (fingers, pick, slap), voice: §4 presets blind against gfx-comp (same intent, single band) and against the catalogue's Spectral Comp; Hyper against an OTT-like reference at matched loudness; dispersion audibility on bass and kick (Minimum vs Linear, blind) |

## 6. Positioning

| Pedal | Its job | Difference |
|---|---|---|
| **gfx-comp** | One band, five characters; `sc_hpf` keeps the lows from pumping | gfx-mbcomp runs the same engine per band. For one even level, gfx-comp is simpler and phase-free |
| **gfx-glitch `density`** | A compressor / expander **per STFT bin**, toward the frame's mean level, inside a spectral effect with latency on the wet | gfx-glitch flattens a spectrum into a texture (an effect of the spectral pedal). gfx-mbcomp is a time-domain, zero-latency dynamics tool with 3 – 4 musical bands, real attack / release and characters. It keeps transients and pitch intact |
| **Spectral Comp** (catalogue, CLAP) | Thousands of per-bin compressors, up and down, a threshold curve with tilt; 43 ms latency at 2,048 | The same use (even out voice and pads without pumping) at bin resolution, with latency. gfx-mbcomp is the zero-latency, guitar-lane version; its Tilt is that block's slope, made a macro |
| **gfx-deesser** | One dynamic band, relative detection, sibilant-aware | Narrow, voice-specific; keep it before gfx-mbcomp on a voice |
| **gfx-eq** de-harsh | One cut-only dynamic band | One problem frequency; gfx-mbcomp works on whole regions |
| **gfx-bassdi** | Single-band comp before the dirt, `bass_keep` | Its compressor is part of a preamp; gfx-mbcomp after it handles string-to-string balance |
| **gfx-grit** | Per-band distortion on the same `crossover` tree | Same split, other job |
| **gfx-transient** | Shapes attacks and tails by level-independent envelope ratios | It does the fourth quadrant (expansion above); its floor bound is this block's noise guard |
| **gfx-ducker** | Gain driven by a key from another lane | Keyed; gfx-mbcomp has no key input in v1 |
| **gfx-limiter** | Never a sample past the threshold | Put it after gfx-mbcomp when Hyper is pushed |

Against the field: Pro-MB's Minimum phase with a phase that provably does not move with gain; its Linear
mode at one FIR's cost; OTT's sound with a noise guard and loudness-matched macros; MO-TT's per-band
Band / Wide sidechain; gfx-comp's five characters per band; and detectors sized for bass. Not attempted:
more than 4 bands, per-band lookahead, external key input (golem does not route aux buses today; the
contract allows one later as a separate bus), dynamic-phase filtering (§1.4), mid / side.

## Visual

- **Category: `dynamics`**. Body hue is the family's slate, `METAL_HUE.dynamics` `#4a5a6e`, with the
  family's blue rune light (`RUNE_LIGHT.dynamics` `#5fb0ff`). **No crate hue**: the motif sets it apart
  from gfx-comp, gfx-limiter, gfx-deesser and gfx-gate, as gfx-grit did inside lofi. Free gaps on the
  wheel are scarce after lots 1 – 2 (gfx-grit review), and a multiband comp is a compressor.
- **Finish: Thrash**, the family's (`FAMILY_FINISH.dynamics`): gloss slate, the motif in yellow lined in
  black.
- **Motif: `tiers`** (new case in `motifParts`, drawn with `shape()` so both finishes work). It is
  four short meter bars on each side of the rune, each pressed by its own small clamp. Geometry fixed
  at review, inside the rune zone (the cartouche's height), clear of the knob grid:
  - The side space is s = (W − cw) / 2 − 4: about 17 – 33 px, since W = 112 or 124 and cw ≤ 70.
  - On each side, four bars at y = cy − 0.375 ch, cy − 0.125 ch, cy + 0.125 ch, cy + 0.375 ch (9 – 13 px
    apart). Height 3, rounded ends. Each starts at the outer edge (x = 4, or W − 4) and runs inward over
    (0.7, 0.4, 0.55, 0.3) of s − 8 on the left and (0.35, 0.65, 0.3, 0.5) on the right, so the two
    sides read as different bands working. The 8 px keep room for the clamp.
  - At each bar's inner end, a small clamp triangle pointing at it: gfx-comp's `clamp` triangle at
    0.45 scale (4 × 6.3 px), 2 px clear of the bar. If s < 16 px, the triangles are left out.
  - Opacity 1, 0.85, 0.7, 0.55 from top to bottom.
  - Thrash: yellow, lined in black; anodized (if forced): engraved.
  - It reads as gfx-comp's clamp, split into bands. Against its neighbours on the slate: `clamp`
    (gfx-comp, gfx-limiter, gfx-deesser) is two large triangles and a band across the rune; `dips`
    (gfx-ducker) is one curve that drops and recovers; `spikes` (gfx-transient) is a row of envelopes
    on a baseline under the rune; `barbed` (gfx-gate) is wire. `tiers` is the only one made of
    stacked horizontal bars.
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-mbcomp": {"name": "Bands", "blocks": ["mb-comp"], "knobs": ["amount", "tilt", "time", "mix", "level"],
                 "selectors": ["character"], "category": "dynamics"}
  ```

  Five knobs and one selector. The printed word is "Bands" (open question 1).
- **Glyph** (`visuals/glyphs/mb-comp.svg`, style of `glitch.svg`): three band lines, each pressed by an
  arrow at a different place, i.e. the comp glyph split into bands:

  ```svg
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>Multiband compressor (gfx)</title><path d='M3 7.5h18M3 13.5h18M3 19.5h18'/><path d='M4.5 2.5 7 5l2.5-2.5M10.5 8.5 13 11l2.5-2.5M14.5 14.5 17 17l2.5-2.5'/></svg>
  ```
- No maker's livery, logo or trademark on the pedal. Preset names carry no artist or product names
  ("Hyper", not "OTT").

## Open questions for review — settled

1. **Block type and name** → `mb-comp`, category `dynamics`, a golem-fx-only catalogue entry
   (golem-rack). Roles from the catalogue's list: guitar, bass, voice, keys, pads ("bus" is not a role
   there). Printed word **"Bands"**: "Multi" says nothing at tile size.
2. **Linear latency** → **2,048 samples (42.7 ms at 48 kHz) kept.** LR4 is corrected at every crossover
   (−49 dB or better); LR8 from 80 Hz up, and below that one section is left minimum-phase so the
   magnitude stays flat (§2.3). 85 ms would only help LR8 with `x_lo` under 80 Hz. `phase` can grow a
   third option if a use asks for it.
3. **Auto gain** → **static, per band, R = −24 dBFS (−18 for a Wide band), on by default.** A loudness
   follower breathes, and breathing is what the block is accused of. The static reference is exact for
   pink-like material and approximate for a band far from R; `<b>_gain` in the preset does the rest.
   R is fixed on the bench from the DIs.
4. **Detection tree** → **Butterworth-4, power complementary, for both slopes** (§2.4). 36 ns buys half
   the crossover sag. Its edges differ from the audio bands' by 3 dB at the crossover, by design.
5. **3 / 4 bands** → **`bands` kept**, positional ids. Always-4 would put a fourth all-pass and a
   fourth engine in every guitar preset; a rack expects 3 bands (Helix, Fractal, OTT).
6. **Character** → **one global `character`.** Per-band attack and release already give "slow lows,
   fast highs". `<b>_char` can be appended later with a first option Global.
7. **gfx-comp changes** → **`next_gain_db` and `detector_low_hz`** (§2.4, §2.5). `colour: false` is
   gfx-bassdi's and gfx-acoustic's need, not this crate's: the gain API passes no audio. All three
   leave gfx-comp's renders bit-identical, which its tests and a null against today's build must show.
8. **Meters** → **`<b>_dyn`, a signed gain** (negative = reduction, positive = boost); `gr` keeps the
   contract's meaning (≥ 0, the largest reduction). A signed value under a `_gr` name would have
   contradicted the shared id.
9. **Expansion** → **one signed slope `<b>_lift`** (−100 – +100 %, 0 = off at the centre) replaces
   `up_ratio` 0.5 – 10, whose "off" sat at 5 % of the range and whose expansion half was 5 % wide.
   Amount already scales slopes, so the unit is the engine's own.
10. **No key input** in v1 → confirmed, as gfx-comp. gfx-ducker is the keyed block; the contract
    allows an auxiliary bus later.
11. **Hue** → **no crate hue**, the family slate and the `tiers` motif. To check on the family sheet
    against gfx-comp, gfx-ducker and gfx-transient at tile size.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good**, Strong once the bench confirms the detector and the guard on real DIs. The central
claim is true and it is the right foundation: on the all-pass-compensated Linkwitz-Riley tree every
band is a real, non-negative weight times one all-pass, the weights sum to 1, so any band gains give
that all-pass times a weighted mean of the gains. The phase cannot move and no bump or notch can form
at a crossover. It is now proved in two lines (§2.2) and re-measured (TPT forms, 48 kHz, 300 gain sets
per case). The one-FIR Linear mode, the power-complementary detection tree and the guard on upward gain
are all sound ideas, and none of the surveyed references has the last one. Four claims did not survive
the numbers: Tilt as a tone control, Linear mode "never worse than Minimum", "dynamics identical" in
Linear, and the detector's ripple figure with its cure. None touches the structure. Pages re-read:
FabFilter's processing modes, Slate's MO-TT manual, Sound On Sound on Ableton's device. Scripts:
`scratchpad/proto-gfx-mbcomp/` (`tree.py`, `lin.py`, `lin2.py`, `det.py`, `det2.py`).

Changes made, and why:

| # | Change | Why |
|---|---|---|
| 1 | **Tilt is described as what it is** (§2.6, §3, §4): where the squeeze is, not a tone control; "brighter / darker" removed; "Tone tilt by foot" became "Weight shift by foot" | The make-up is computed after Tilt, so at the playing level every band stays at unity. Tilt +100 %: low band +6.8 dB twelve dB under the playing level and −7.2 dB twelve over, high band within 0.5 dB. The draft's "tone control at playing level" was cancelled by its own auto gain |
| 2 | **Linear kernel: fit rule** (§2.3). LR8's Q 1.307 section of a crossover below 80 Hz stays minimum-phase | A truncated all-pass is not flat. With `x_lo` 40 Hz and LR8 the draft's kernel measured +0.8 / −2.3 dB in the lows (−1.2 dB in 3-band), while Minimum is flat: "never worse" was false. With the rule: ±0.04 dB at every reachable setting |
| 3 | **Linear: detection reads the input delayed by L**, not the FIR's output (§2.3, diagram) | The FIR is an all-pass and changes the waveform's crest, so peak readings differ: the draft's "dynamics identical" and its bench null could not hold. With the delay, the gains are Minimum's, L later |
| 4 | **Detector: sliding maximum over 1.2 periods on both readings**, RMS time unchanged (§2.4); `detector_low_hz` is an `Option` | The draft's 67 ms RMS removes the ripple but takes 113 ms to reach 90 % of the reduction (16 ms today): attack on the low band stopped meaning anything, and Bus and Opto read RMS only. The maximum of the 10 ms RMS over a period is ripple-free by construction (0.000 dB, no added harmonic) and as fast as today. Window 1.2 / f, not 1 / f: gfx-gate's and gfx-transient's rule. Cost: +0.8 to +1.6 dB of reading on the lowest octave, a hold of W |
| 5 | **Ripple figures corrected** (§2.4, §5) | −29 dBc assumed the gain follows the ripple both ways. With attack 1 ms the gain holds the trough: −55 to −61 dBc at release 250 ms, −35 to −41 dBc at release 20 ms. The problem is real at short releases, not at the defaults |
| 6 | **Noise guard = gfx-transient's floor bound**: boost ≤ level − `guard`; default −72 dBFS, range −96 – −36 (§2.5) | The draft's guard reached 0 dB at guard − `up_range`, so raising the range raised the noise that got lifted. Now nothing at or under `guard` is lifted, whatever the range; same law as gfx-transient §2.4. Close to the draft at its defaults (both reach 0 dB at −72 dBFS) |
| 7 | **Guard's limits stated**; bench row rewritten | The make-up raises noise too (+10 dB in Hyper): "output noise ≤ input + 3 dB" could not pass. The guard removes Lift's share (18 dB of 28) |
| 8 | **Lift's transient claim corrected** (§2.5, §5); Lift reads max(peak − 3 dB, RMS) | "Keeps upward compression from inflating pick transients" holds only for short attacks: the onset keeps the boost for the attack time. Stated, bench row split by attack |
| 9 | **`<b>_up_ratio` 0.5 – 10 → `<b>_lift`, a signed slope −100 – +100 %**; ids `lift_thresh`, `lift_range` | Open question 9. Off at the centre, linear in the unit Amount scales; +100 % (slope 1) is safe because Lift is feed-forward and capped |
| 10 | **Amount holds the ratio at 30:1, not ∞** (§2.6); Lift's scaled slope held in −200 – +100 % | gfx-comp's feedback characters compute (1 − r) · (y − T): no value at ∞. The draft's "no engine change" was true only below ∞. Lift's k′ could exceed 1 (a non-monotone curve) |
| 11 | **gfx-comp API made precise** (§2.5, §2.9): `next_gain_db` advances the clock and smoothers, `process_block` is rebuilt on it, effective values stay inside the engine's ranges; `colour: false` dropped from this crate's needs | The engine's control clock and smoothers live in `process_block` today. A gain-only API passes no audio, so colour cannot occur; the setting stays gfx-bassdi's and gfx-acoustic's |
| 12 | **`crossover` spec pinned** (§2.1, §2.9): layers 1 and 3 of gfx-bassdi §2.9, tree as gfx-grit §2.2; `BwTree` and `Linearizer` are a fourth layer with unit tests | gfx-bassdi and gfx-grit name the layers differently (`Split` / `LrSplit`, `Tree` / `LrTree<N>`); this document now cites both and adds nothing to the shared layers |
| 13 | **61 → 54 parameters**: four `<b>_solo` → one `solo` enum; four `<b>_bypass` dropped | Solo is an audition tool, one band at a time is enough. A band at ratio 1 and Lift 0 % is bypassed. Parameters can be added later, never removed (gfx-grit's decision on mutes) |
| 14 | **Meters: `<b>_gr` signed → `<b>_dyn`**, a gain with its natural sign; `gr` unchanged | The contract's `gr` is a reduction ≥ 0; a signed value under that name would contradict it |
| 15 | **`bypass` goes to the input**, not to the tree's sum (§2.7) | §2.2 called bypass "the null" and §2.7 sent it to the all-passed dry: one of them was wrong |
| 16 | **Auto gain**: R explained (pink noise at −18 dBFS gives −22 to −25 dBFS per band), R = −18 dBFS for a Wide band, limits stated | A Wide band's detector reads the whole signal, 6 dB above a band's |
| 17 | **Latency per rate family** (2,048 / 4,096 / 8,192) instead of "the same time at every rate" | A partitioned convolution wants whole blocks; 44.1 kHz would have needed 1,881.6 samples |
| 18 | **Moving gains** paragraph (§2.2), **Attack splatter** bench row | The identity holds sample by sample because the gains follow the filters. A fast low-band gain's side-bands are not filtered back into the band: true of every split-gain-sum design, now stated and measured |
| 19 | **Detection tree vs audio bands** stated for both slopes (§2.4) | With LR4 the edges agree within 0.5 dB an octave away and differ by the intended 3 dB at the crossover. With LR8 the detector is 24 dB less selective than its band: accepted, upgrade named |
| 20 | Facts: dispersion table re-measured, bass row at the preset's 1.5 kHz, "worst packing (40, 60, 90)" replaced by the lowest reachable set (40, 150, 800: `x_mid` starts at 150 Hz); residual table re-run; 3-band LR4 tree = 7 ticks; OTT's values are EDMProd's [I], Sound On Sound covers the device; "four quadrants" → three; MO-TT's OTT is a quick-set; roles from the catalogue's list (pads, no "bus") | Accuracy |
| 21 | **Visual**: `tiers` bars moved inside the cartouche's height (± 0.375 ch, were ± 0.75 ch), lengths as fractions of the side space less the clamp, triangles dropped under 16 px; distinctions from `clamp`, `dips`, `spikes`, `barbed` written out | At ± 0.75 ch the outer bars left the rune zone (the cartouche nearly fills it); a 0.9 bar left no room for its triangle in a 17 – 33 px side space |

Still open:

| Point | Owner |
|---|---|
| Catalogue entry `mb-comp` (category `dynamics`, roles, golem-fx only); the note "Phase: set before playing"; the split-lane note of §2.2 | golem-rack |
| Both engines following a latency change made by a parameter (`phase`) while running | golem-rack, to measure |
| Per-band meters `<b>_dyn` (signed) in golem's UI; a band view in the Designer for 4 × 8 parameters; EXP on `amount` and `tilt` (non-catalogue ids) | golem-rack |
| R (−24 dBFS), the per-band default times, Tilt's ±12 dB, the 80 Hz limit of the fit rule | Bench, by ear |
| Guard: absolute level kept. If presets prove too level-dependent, gfx-transient's relative floor (phrase level − 48 dB). Whether the guard should also take the make-up back under it (a soft expander below the guard, so that hiss never comes out above its input level) | Bench, then the maintainer |
| Lift and transients: attack as the only time (OTT's way) or a fixed fast fall for boosts | Bench, on the pick-attack DI |
| LR8: Butterworth-8 detection if the 24 dB selectivity gap is heard; a third `phase` option at 85 ms if LR8 below 80 Hz needs full correction | Bench |
| Audibility of 4 – 5.7 ms (LR4) and 9 – 13 ms (LR8) of low-frequency dispersion on bass and kick, Minimum against Linear; pre-ringing of a compressing band in Linear | Bench, by ear (shared with gfx-grit) |
| `crossover`: one set of names for layers 1 – 3 across gfx-bassdi, gfx-grit and this crate; who writes it first | golem-fx, at coding order |
| gfx-comp: `next_gain_db`, `detector_low_hz`, with a bit-exact null of gfx-comp before and after | golem-fx, with this crate |
| `tiers` against gfx-comp, gfx-ducker and gfx-transient at tile size; the printed word "Bands" | The maintainer, on the family sheet |
| Bass DIs (fingers, pick, slap), voice DIs: every §4 preset and the Listening row wait for them | The maintainer / golem-rack |
| OTT's attack times could not be re-read [U]; the Waves C4 and Fractal rows rest on secondary sources [I] | Not blocking |
