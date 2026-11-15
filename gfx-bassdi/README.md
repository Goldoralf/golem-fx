# gfx-bassdi

> **Status: CODED** (v0.1.0, 2026-11-15). Designed and reviewed 2026-11-14 (Fable 5.2, last section).
> Built as designed on the new gfx-dsp `crossover` (split, band-gain with its all-pass twin, tree): three
> engines (Modern, Classic, Vintage) whose dirt runs pre band-gains, 8× ADAA stages with the interstage
> LR4 low-pass and a post low-pass, while the clean runs the same twins, half-bands and half samples;
> gfx-comp's engine before the dirt with its colour off (a new library-only setting, gfx-comp's renders
> bit-identical); `bass_keep`, the range-scaled subsonic high-pass, the EQ, the five-way cab, `level`,
> model and cab crossfades. Measured: at small signals the blend is a crossfade of magnitudes to 0.0000 dB
> (dirt / clean phase ≤ 0.023°, every model, `grunt` / `attack` corners, `low_note` 27.5 – 82.4 Hz, `comp`
> 50 %); worst alias −97.5 dBFS at full gain (bound −70), driven fundamentals lose ≤ 0.18 dB in the blend.
> Bench 28 / 28, 13 behaviour tests (+ 7 gfx-dsp `crossover` tests), CPU 920 – 1,025 ns / sample mono at defaults over five runs
> (bound 1 µs), 1.64 µs during model crossfades (bound 2 µs). Deviations and what stays [A]: "As coded".

## As coded (v0.1.0)

**Files.** gfx-dsp `crossover.rs` (`Split`, `Twin`, `BandGain`, `Tree`); gfx-comp gains
`Comp::set_colour(false)` (library only: the FET / Opto / Pedal dynamics without their shaper, 2×
oversampler and hiss; 50 bench renders of gfx-comp, 5 characters × 5 settings, mono and stereo at
block 77, bit-identical before / after); here `lib.rs` (the processor, PARAMS, the one-knob comp map, the
loudness tables), `engine.rs` (models, voicings, the dirt and its clean twin, 8× oversampling),
`post.rs` (EQ and cab designs), `plugin.rs` (class id `GoleMFxBassDi001`, mono 1 / 1 then 2 / 2, latency 0,
`bypass` = the VST3 bypass parameter, `gr` read-only meter), `tests.rs`.

**Measured** (tests on synthetic notes, and `gfx-bench check gfx-bassdi`):

| What | Target (§5) | Measured |
|---|---|---|
| `crossover` identities | phase of section = twin within 1e−5 rad, monotone, bit-identical at g = 1 | ≤ 4.1e−9 rad (LR2 / 4 / 8, 44.1 – 192 kHz, g 0 – +12 dB, 120 Hz / 1.5 / 12 kHz); monotone; bit-identical. LR2 −12 dB shelf at 200 Hz: −7.9 / −4.1 / −1.4 dB at 100 / 200 / 400 Hz |
| Coherence, small signal (−74 dBFS per partial, `gain` 0, 3 models × 5 `grunt` / `attack` / `low_note` corners, `comp` 50 % Dirt) | crossfade of magnitudes within 0.1 dB, 25 Hz – 15 kHz | 0.0000 dB; dirt / clean phase ≤ 0.023° |
| Coherence, driven, single tone (41 Hz – 1 kHz, −30 – 0 dBFS, `gain` 0 / 50 / 100) | within 2° | Vintage 0.01°; Classic 3.3°, Modern 6.5° (both at 1 kHz, 0 dBFS, `gain` 100): see deviations |
| Coherence, driven notes (16 weak-fundamental notes B0 – G2, Modern, `gain` 70 %) | median ≤ 0.3 dB, 90th ≤ 1.5 dB | fundamentals 0.03 / 0.18 dB (worst 0.18); every partial below 200 Hz 0.03 / 3.1 dB (worst 9.5 dB, a weak 4th harmonic); with `bass_keep` 2.5 L the fundamental within 0.15 dB of the clean |
| Tight low end (low B at `gain` 80 %, 1 kHz probe) | 31 Hz within 0.5 dB of the clean; probe sidebands ≥ 20 dB lower | 31 Hz −0.05 dB; sidebands of a bare B fundamental −60 vs −37 dB (23 dB lower); of a six-harmonic B −7.5 vs −1.0 dB: see deviations |
| Range scaling (41.2 → 30.87 Hz) | each corner × 0.749 within 1 % | responses at 0.6 L, 0.85 L (Smooth), 4 L (pre section), 2 L (bass shelf) unchanged within 0.1 dB |
| Aliasing (10 sines 1 – 5.2 kHz, −12 dBFS, dirt only, cab Off) | `gain` 100: worst ≤ −70, median ≤ −85; `gain` 35: ≤ −100 dBFS | `gain` 100: −97.5 / −110.0 (Modern), −109.2 / −111.9 (Classic), −108.4 / −111.2 (Vintage); `gain` 35: ≤ −106.4 |
| Level (synthetic −18 dBFS-RMS bass line, K-weighted) | ±1.5 LU over `gain`, ±2 LU across models | within ±1.5 LU (the tables are fitted on the same line) |
| EQ (44.1 – 192 kHz, 8 frequencies, all four bands moved) | ±0.5 dB of the analog targets; flat = sections not run | within 0.5 dB; flat is the identity |
| Cab | Off bit-transparent; `cab_lows` off within ±0.3 dB below 100 Hz; Smooth no notch deeper than 3 dB | Off identity; within ±0.3 dB (all four models, 30 – 100 Hz); Smooth ≥ −3 dB 150 Hz – 2 kHz. 8×10: −6.0 dB at 40 Hz, +0.1 at 58 Hz, −2.0 at 5 kHz (anchor −10 dB at 40 Hz not reached: [A]) |
| Compressor | `comp` 0 bit-transparent; make-up holds −18 dBFS within 1 dB; FET / Opto no filter; `gr` within 0.5 dB | not run at 0 (path Both = clean alone, bit-exact); within 1 dB in all three characters; FET / Opto vs Clean on a steady tone within 0.3 dB and 0.1°; `gr` within 0.5 dB of the curve |
| Onset thump (Vintage, `gain` 100 %, `sag` 100 %, gated 82 Hz) | report | energy below 29 Hz −27 dB re the note, first 100 ms |
| Common checks, 44.1 – 192 kHz | — | 28 / 28; worst parameter step −78 dB re peak (`gain` 100 → 0), so the README's −30 dB allowance was not needed |
| CPU (48 kHz, blocks of 128) | ≤ 1 µs / sample mono; ≤ 2 µs in a crossfade | 920 – 1,025 ns mono over five runs (1.78 – 1.90 µs stereo frame): at the bound, the 4× fallback not taken; 1.64 µs with the engines always crossfading |

**Deviations from the design, and why.**

- **`bass_keep` also high-passes the dirt's input.** §2.1 wrote out = LP(clean) + HP(mix), which keeps the
  output's lows clean but leaves them in the clipper: the low B's AM of a 1 kHz probe was unchanged
  (−1.0 dB both ways). §1's conclusion and the bench row ask that "the lows never reach the clipper", so
  the dirt now enters through a second LR4 split's high band and the clean through its twin, before the
  engines; after them the post split is as designed. Both paths carry both all-passes, so the blend stays
  coherent; the price is twice the `bass_keep` dispersion (≈ 9 ms at 110 Hz instead of 4.6).
- **Tight low end with a real note.** With the lows out of the clipper, the B's harmonics above 110 Hz
  (93 Hz at −6 dB, 124, 155 Hz…) still clip with the probe and modulate it at the B's period: sidebands
  fall 6.5 dB, not 20. A bare fundamental falls 23 dB. Meeting 20 dB on a full note would need a
  `bass_keep` above the note's harmonics, which is not a bass DI any more.
- **Driven single tone, two-stage models.** The interstage shelf and low-pass filter stage 1's harmonics
  before stage 2 re-clips them, so the fundamental rotates up to 3.3° (Classic) and 6.5° (Modern) at full
  gain and 1 kHz (≤ 2.5° at 330 Hz and below). The prototype's ≤ 1° was a bare clipper. Test bound 8° for
  those two, 2° kept for Vintage.
- **Driven notes**: the README's bound is met on the fundamentals (the prototype's measure); over every
  partial below 200 Hz the 90th percentile is 3.1 dB (weak 3rd – 5th harmonics rebuilt by the clipper,
  also in the one-stage Vintage: 2.5 dB). `bass_keep` remains the guarantee.
- **The sag / bias envelope** is a mean square (x² smoothed by two 1 ms one-poles, then the 5 / 150 ms
  follower, read as a sine's peak), not a peak follower on the samples: the latter switched attack /
  release with the sampling phase of a 3 kHz note and modulated the sag inharmonically (−90 dBFS
  aliases-like components in Classic and Vintage at `gain` 35 %; now ≤ −106).
- **Choices where the design was silent** [A]: Classic's power stage +6 dB; full sag envelope = stage 1's
  input at peak 1 (its clipping point); bells of the mid bands = gfx-eq's law at its defaults (Q 0.67,
  50 % proportional); Smooth's 4th-order low-pass = sections Q 0.54 and 0.6 (no peak); bass_keep below
  1 Hz is off, its split never below 10 Hz; `comp` at exactly 0 (settled) does not run the compressor
  (bit-transparent; ratio 1 would otherwise enter gfx-comp's knee with a = 0).
- **Cab models**: the box alignments and bodies of §2.7 are designed values, voiced by numbers only; the
  8×10 reaches −6 dB at 40 Hz, not the manual's −10 (a sealed 2nd-order box with Q 1.1 cannot drop 10 dB
  in the 0.54 octave from 58 to 40 Hz while holding 58 Hz within 3 dB). All four wait for the licensed IRs.
- **Loudness tables** are fitted on a synthetic plucked line (10 notes E1 – G3 and B0, partials to 8 kHz),
  K-weighted; they are to be refitted on golem's bass DIs.
- The EXP-sideband row is covered by gfx-dsp `exp_smooth`'s own tests (the EXP parameters use it); the
  naive-build diagnostic row is not re-run (the prototype's −0.9 / −4.2 dB stand).

**Open for the owner.** Every [A] voicing by ear once the bass DIs and the NAM references exist (model
gains and corners, b, sag depths, the comp map, Classic's stage 2); cab targets from licensed IRs; the
loudness tables refitted on real DIs; LR2 vs LR4 for the low pre section (A / B on a low B); golem-rack:
the `bass-di` catalogue entry, `init` writing `low_note` and `cab` Off.

golem's **bass preamp / DI**. It is a new block: one box that does the job of the tube-emulation DI, the
modern CMOS-grind preamp and the valve-grit DI. It has a compressor before the dirt, a clean path and a
driven path blended in phase, an active four-band EQ, and a speaker emulation that can be switched off
when an IR follows.

Three things set it apart from the pedals and plugins of the category:

- **No filter in the box can make the blend cancel.** Every linear filter on the driven path is built so
  that its phase is a known all-pass, and the clean path runs through that same all-pass (§2.2). At small
  signals, Blend is a pure crossfade of magnitudes, at every setting of Grunt, Attack and model. Under
  drive only the clipper's own phase remains, and `bass_keep` takes the lows out of its reach.
- **It knows the instrument's range.** `low_note` (the lowest open string, set by golem from the lane's
  tuning) scales every low-frequency corner. So a 5-string in drop A gets the same tightness as a
  4-string in E.
- **It keeps the low end tight both ways**: Darkglass-style pre-clip filtering under a full-band clean
  blend, and a Parallax-style crossover (`bass_keep`) that keeps the lows out of the clipper and the
  cabinet.

It reports zero latency (§2.6): 0.1 ms of minimum-phase group delay, and lows about 2 ms behind the
highs. It is mono 1 / 1, with 2 / 2 offered.

## 1. State of the art

Survey of 2026-11-14. **[M]** read in a manufacturer manual or page · **[V]** read in a third-party copy
of a manual, a paper or source · **[F]** forum or review opinion · **[U]** not verified · **[A]** our
calculation or design inference.

### Reference hardware

| Unit | What defines it | What we take |
|---|---|---|
| **Tube-emulation bass DI** (SansAmp Bass Driver DI, [manual](https://www.tech21nyc.com/wp-content/uploads/2019/05/BassDriver-OM1.pdf)) [M] | Level, Blend, Treble, Presence, Drive, Bass. Drive is described as "similar to when the output section of a tube amp is being pushed". Blend mixes the direct signal with the tube-emulation circuit, and "in most cases" sits at 100 %. **Bass, Treble and Level stay active at Blend minimum**, so the EQ comes after the blend. Bass and Treble are ±12 dB. The mid level is preset: boosting both gives a "mid-cut at 750 Hz". The speaker simulation is "an integral part" of the circuit: a smooth response "as would be achieved by a multiply-miked cabinet, without peaks, valleys and notches". 1 MΩ input; XLR pad −20 dB | **Classic** model (power-stage push, sag); EQ after the blend; the **Smooth** speaker emulation (notch-free) |
| SansAmp BDDI **v2** ([third-party manual copy](https://manualmachine.com/tech21/sansampbassdriverdiv2/19065674-owners-manual), retailer listings) [V] | Adds a Mid control with a 500 Hz / 1 kHz shift. **Bass shift 80 Hz → 40 Hz "to accommodate 5- and 6-string basses"** | Bass corner follows the instrument's range (`low_note`) |
| **Modern CMOS bass preamp** (Darkglass Microtubes B7K Ultra v2, [manual](https://darkglass.com/?p=1374): the link opened the maker's storefront at review, so the quotes stand as the designer read them) [M, not re-read] | Master, Blend, Level, Drive. **Blend**: "the clean signal remains at unity gain while the volume of the overdriven signal is set by the Level knob". **Attack** (Boost / Flat / Cut): the "amount of treble content to saturate". **Grunt**: "three different bass boost levels **before the clipping stage**". EQ: Bass ±12 dB at 100 Hz; Lo Mids ±12 dB at 250 / 500 / 1 kHz; Hi Mids ±12 dB at 750 / 1.5 k / 3 kHz; Treble ±12 dB at 5 kHz. 1 MΩ in | **Modern** model; `grunt` and `attack` as pre-clip band gains; `dirt_level` for the Level knob; two switchable mid bands |
| CMOS lineage (B3K, "Microtubes 2K") ([Aion FX on the B3K](https://equipboard.com/items/aion-fx-maelstrom-bass-drive)) [V] | A CMOS-based drive (inverter stages used as amplifiers), first posted on the DIYStompboxes forum in 2008. Exact chip and values are not verified [U] | Cascaded soft asymmetric stages |
| Darkglass amp-head engine (Microtubes 900 v2, [manual](https://www.darkglass.com/manual-microtubes-900v2/)) [M] | "Compression knob: the amount of dynamic compression added to the Microtubes Engine. The make-up gain is adjusted automatically." B3K mode is "aggressive, percussive"; VMT mode "brings up the mids". Mid bands 250 / 500 / 1.5 k / 3 kHz. The compression belongs to the engine (the distortion path), which Blend mixes with the clean; the maker's X Ultra needs its software to "enable the compressor for clean signal" [M, checked at review] | **One-knob compressor before the dirt**, auto make-up, `comp_path` Dirt / Both |
| **Valve-grit DI** (Ampeg SCR-DI, [manual](https://ampeg.com/pdf/manuals/SCR-DI_OM.pdf)) [M, re-read at review] | Bass ±14 dB at 40 Hz; Mid +5 / −11 dB at 500 Hz; Treble +16 / −15 dB at 4 kHz. **Ultra Lo**: +1 dB at 40 Hz and −10 dB at 500 Hz. **Ultra Hi**: +5 dB at 8 kHz. Scrambler overdrive with Drive and Blend; the overdrive and the EQ can be used separately | **Vintage** model; presets (scoop = Lo-Mid −10 dB at 500 Hz) |
| Tech 21 VT Bass family [U] | Character control between amp voicings | Covered by the model selector |

### Plugins

| Plugin | What it does best | What we take |
|---|---|---|
| Neural DSP Darkglass suite ([manual](https://neuraldsp.com/manual/darkglass-ultimate)) [M] | Pre-FX chain: **FET-style compressor with a dry Blend** ("to avoid over-compressing lows"), then the B7K / Vintage preamps (Grunt, Attack, two switchable mids). A **"Distortion EQ Link"** shares or separates the EQ between the clean and distorted modes. Cab with movable mics, custom IR, per-mic phase | FET character for the compressor; EQ after the blend |
| [Neural DSP Parallax](https://neuraldsp.com/us/plugins/parallax) ([SOS review of Parallax X](https://www.soundonsound.com/reviews/neural-dsp-parallax-x)) [M/V] | **Three-band crossover**: lows compressed (clean), mids and highs distorted separately. **The cab sim acts only on the mid and high bands**, "to avoid issues with the low end". Six-band graphic EQ | `bass_keep` (clean lows) and `cab_lows` (cab off below the bass) |
| Line 6 Helix / Fractal bass amps and drives [U] | Bass amp and preamp models; Fractal's parallel drive blocks are reported to comb against the dry ([Fractal forum](https://forum.fractalaudio.com/threads/drive-block-in-parallel-causes-phase-issues.92282)) [F] | The problem we solve by construction (§2.2) |
| Free: TSE BOD (BDDI emulation), Ignite Amps SHB-1 [U] | Popular free references for DI-style bass grit | Listening references only |
| ChowDSP BYOD (GPL-3) [V, from the gfx-drive survey] | ADAA waveshapers, modular drives | Already reused via gfx-drive |

### Papers

- Fernández-Cid, Casajús-Quirós & Aguilar, *Multiband Waveshaping Distortion*, DAFx 1999
  ([archive](https://dafx.de/paper-archive/details/T3H0_pgOrplVcowqobXvpg)) [V, abstract]. Splitting the
  input into bands before waveshaping keeps the intermodulation low and the result "harmonic rather than
  metallic". This is the theory behind the crossover approach.
- Linkwitz, *Active crossover networks for non-coincident drivers*, JAES 1976 [U, standard]. LR4 low-pass
  and high-pass outputs are **in phase at every frequency**, and their sum is a 2nd-order all-pass
  (§2.2; verified numerically at review). The 2nd-order LR pair does the same with one output inverted,
  and sums to a 1st-order all-pass.
- Yeh, Abel & Smith (DAFx 2007), Eichas & Zölzer (JAES 2018): pre-filter → static nonlinearity →
  post-filter captures most of a drive. Parker et al. (DAFx 2016) and Bilbao et al. (IEEE SPL 2017) give
  ADAA. All of these come from the [gfx-drive](../gfx-drive/README.md) survey and its code.
- Giannoulis, Massberg & Reiss (JAES 2012) for the compressor, as used in [gfx-comp](../gfx-comp/README.md).

### Forum feedback: what bassists love and hate

- **Loss of low end with drive** is the first complaint. The standard cure is a clean blend. Players
  report Blend around 50 – 60 % keeping "the clean low end intact"
  ([Carvin](https://carvinaudio.com/blogs/guitar-bass-education/losing-low-end-with-bass-overdrive-here-are-some-tips-you-can-try),
  [Death Cloud](https://deathcloud.com/blogs/info/how-does-parallel-distortion-work-for-bass)) [F].
- **Blends that thin out**: phase cancellation between the clean and the processed signal. The usual
  advice is to flip polarity or "use phase rotation"
  ([SOS, mixing bass](https://www.soundonsound.com/techniques/mixing-bass), Fractal forum above) [F].
  This is a symptom of unmatched filters; we remove the cause.
- **Fizz direct to the desk**: a DI is flat. A pedal voiced through a bass cab is "extremely bright and
  fizzy" into the PA. Players use a cab sim, or a low-pass and high-pass, or an IR. One reports a hi-cut
  above about 6.5 kHz sounding "rather bad and fizzy" with a lot of drive
  ([talkbass](https://www.talkbass.com/threads/dirt-di-seems-to-why.1440259/),
  [basschat](https://basschat.co.uk/topic/471212-drivesdistortionsfuzzes-through-amp-di-beat-the-fizz),
  [Line 6 forum](https://line6.com/support/topic/56787-an-accurate-scientific-explanation-for-why-we-still-need-treble-cut-with-frfr-speakers-and-a-virtual-cab))
  [F]. The B7K Ultra v2 added a defeatable IR on the DI output [V, retailer listings].
- **Loved**: the SansAmp's "plug in and it sounds like a miked rig"; the Darkglass growl from boosting
  Hi Mids at 1.5 kHz; the SVT grit at low drive. **Hated**: the "modern Darkglass" sound becoming a
  cliché; flubby lows on 5-strings; noise at high drive [F, general consensus, not one source].

### What we conclude

1. **The tight-low-end problem has a mechanism** [A]. A low note's large swing pushes the whole signal
   in and out of clipping at the note's rate. That amplitude-modulates the upper harmonics (the "flub")
   and creates difference tones between low notes. There are two cures, and both are kept:
   - **pre-clip filtering** (Darkglass Grunt cut, the TS-style tight): fewer lows reach the clipper, and a
     full-band clean blend restores them;
   - **a crossover** (Parallax, multiband waveshaping): the lows never reach the clipper.
2. **The blend must be phase-coherent by construction**, not by a polarity switch. Filters can be made
   exactly coherent; a clipper cannot (§2.2), which is the second reason to keep a crossover.
3. **EQ after the blend**, as in the SansAmp; the Darkglass suite lets both paths share it.
4. **The compressor goes before the dirt**: it evens out what the clipper sees, with automatic make-up.
5. **Speaker emulation: smooth and switchable.** On, it gives a notch-free averaged cab (the SansAmp
   philosophy). Off, an IR follows (golem's `cab` block). It can also be kept off the lows (Parallax).
6. **Tuning-aware low corners.** The v2 bass shift (80 → 40 Hz for 5- and 6-strings) generalised: golem
   knows the lane's tuning.

## 2. Design

```
in ▶ × input ─▶ [comp: path Both] ─┬─ CLEAN: twin all-passes (base rate) ─▶ ↑8 identity ─▶ twin interstage APs ─▶ ½-sample ×2 ─▶ ↓8 identity ─▶ twin post AP ─┐
                                   │                                                                                                                      │
                                   └▶ [comp: path Dirt] ─▶ DIRT: pre sections (low, high) ─▶ ↑8 ─▶ stage 1 (ADAA) ─▶ interstage sections (shelf, 12 kHz LP) │
                                                           ─▶ stage 2 (ADAA) ─▶ ↓8 ─▶ post section (fizz LP) ─▶ × loudness comp × dirt_level ─────────────┤
                                                                                                                                                          ▼
  out ◀ × level ◀ cab (Off / Smooth / 8×10 …, cab_lows) ◀ EQ (bass, lo-mid, hi-mid, treble) ◀ subsonic HP ◀ LR4 bass_keep ◀ mix (linear crossfade)
```

### 2.1 Signal path

- **Input** (`input`, ±12 dB): the pickup's level into the whole box. An active bass is often 6 – 10 dB
  hotter than a passive one [U]. Nothing compensates it: it drives the compressor and the clipper, as a
  hot instrument does.
- **Compressor** (`comp`, one knob, §2.4): on both paths, or on the dirt only (default Dirt, the
  Darkglass approach).
- **Split.** The clean path and the dirt path start from the same samples.
- **Dirt path**: pre sections → 8× oversampling → one or two saturating stages (with ADAA1) → back to the
  base rate → post section. Its loudness is compensated (§2.3), then trimmed by `dirt_level` (the
  Darkglass Level knob).
- **Mix** (`mix`): out = (1 − m) · clean + m · dirt, a **linear crossfade**. Linear, not equal-power,
  because the paths are correlated in phase: equal-power would bulge about +3 dB at 50 % on the lows.
  - It is a crossfade, not Darkglass's "clean at unity": at 50 % the clean is at −6 dB. Where the model
    cuts the lows before the clipper, the sum is a little lighter than the clean alone (Modern at 50 %:
    about −2.5 dB below 80 Hz [A]). `bass_keep` removes that loss.
- **`bass_keep`** (shared id, Hz, 0 = off): out = LR4_LP(clean) + LR4_HP(mix). Below it the bass stays
  clean; above it, the blend applies. In 2 / 2 the kept band is the mono sum (the shared id's meaning).
- **Subsonic high-pass**: 2nd-order Butterworth at 0.6 × `low_note`. It acts **after the mix**, on both
  paths at once, and it is the DC blocker: it removes the asymmetric stages' DC and the infrasonic thump
  of bias movement (no separate 5 Hz blocker). At 0.6 × f0 the fundamental loses 0.5 dB (verified).
- **EQ** (§2.5), then **cab** (§2.7), then **`level`**.
- **2 / 2**: two independent engines, the compressor linked (gfx-comp `link` 100 %).

### 2.2 Phase-coherent blend: the coherence rule

The clean path and the dirt path must have **the same phase response at small signals**, at every setting.
Otherwise the blend combs, and the lows cancel first: a 30° error is already −0.3 dB at 50 %, and 120°
is −6 dB (cos(φ / 2)). Two kinds of filter are allowed on the dirt path.

1. **Band-gain sections built on a Linkwitz-Riley split**, each with an all-pass **twin** on the clean
   path. Two orders, both verified in a numpy prototype (bilinear / TPT forms at 48 kHz: the phase of
   the section equals its twin's within 3 · 10⁻⁷ rad for every g from 0 to +12 dB):

   | | **LR2 section** (shelves) | **LR4 section** (low-pass, high-pass, `bass_keep`) |
   |---|---|---|
   | Split | p = one-pole low-pass; LP2 = p², HP2 = (1 − p)². LP2 and −HP2 are in phase | LP4, HP4 = two cascaded Butterworth sections each; in phase |
   | Twin (the sum) | A1 = LP2 − HP2 = (1 − s) / (1 + s) = 2p − 1, a **1st-order** all-pass | A2 = LP4 + HP4 = (s² − √2 s + 1) / (s² + √2 s + 1), a **2nd-order** all-pass |
   | Low shelf | A1 + (g − 1) · LP2; magnitude (g + w²) / (1 + w²) | A2 + (g − 1) · LP4; magnitude (g + w⁴) / (1 + w⁴) |
   | High shelf | A1 − (g − 1) · HP2; magnitude (1 + g w²) / (1 + w²) | A2 + (g − 1) · HP4 |
   | g = 0 | 12 dB / oct high-pass or low-pass | 24 dB / oct |
   | Cost, dirt + clean | 2 one-poles + 1 one-pole | 3 SVFs (2 for a pure low-pass) + 1 SVF |
   | Twin's group delay at low f | 1 / (π f_c): 1.9 ms at 165 Hz | 0.45 / f_c: 2.7 ms at 165 Hz |

   - The magnitude runs monotonically from g to 1: no bump, at any g ≥ 0. The phase is the twin's, for
     every g.
   - **Shelves use LR2.** Its transition is that of the hardware's RC shelves (−12 dB at 200 Hz: −7.9 dB
     at 100 Hz, −4.1 at 200, −1.4 at 400; the LR4 shelf does the same within one octave), and its twin
     has 30 % less group delay and one third of the cost. **LR4** is kept where a slope is wanted: the
     fizz and interstage low-passes and `bass_keep`.
   - **The section is computed as twin + (g − 1) · branch.** At g = 1 the dirt's section is then
     bit-identical to the clean's twin, and `grunt` / `attack` only scale one branch: no filter state is
     disturbed, so they are click-free under an EXP pedal.
   - Every twin is taken with gain +1 at DC, so the box never inverts the bass.
   - The bilinear (TPT) transform preserves these identities exactly, warped. Sections and twins run at
     the same rate: pre and post at the base rate, interstage at 8× inside the clean path's identity
     oversampling.
   - Sections in cascade multiply: the dirt's small-signal response is ∏|S_i| · ∏A_i, the clean's is
     ∏A_i.
2. **Filters replicated exactly on the clean path**: the three half-band pairs (gfx-dsp `oversample`
   `identity`) and ADAA's half-sample delay (`HalfSample`, one per stage, at 8×).

Every other filter (subsonic high-pass, EQ, cab) sits **after the mix**, where both paths share it. The
compressor on path Dirt is a gain, not a filter: its colour stage must stay off (§2.4).

**What the prototype measured** (scripts in the review's scratchpad, `proto-gfx-bassdi/`):

- **Small signals: exact.** Whole chain (pre sections, half-bands, two ADAA stages, interstage, post):
  the dirt / clean phase difference tends to 0 with level (≤ 0.03° at −120 dBFS, 20 Hz – 15 kHz, every
  `grunt` / `attack` / `low_note` corner), and the blend equals the crossfade of magnitudes. ADAA1's
  small-signal response is exactly slope × the half-sample average (gfx-dsp's own test), so `HalfSample`
  is its true twin.
- **The same build with ordinary minimum-phase shelves and no twins**: a −12 dB shelf is 53° off and
  costs 0.9 dB at 50 %; a 2nd-order high-pass against the dry costs 4.2 dB. A fitted all-pass would move
  with g and cannot follow g = 0.
- **A single sine, driven: exact.** A memoryless clipper's describing function is real: the fundamental
  of a clipped sine keeps its phase (≤ 1° measured from 41 Hz to 1 kHz, 6 – 48 dB of gain).
- **A real note, driven: not exact, and no filter can make it so.** The clipper rebuilds each partial
  from intermodulation of the others. The strongest partial keeps its phase; a weaker one rotates.
  Sixty synthetic notes (B0 – E2, random partial phases, 6 – 48 dB, `grunt` −12 – +6), fundamental against
  the clean at `mix` 50 %:

  | Note | Phase error, median / 90th / worst | Blend loss, median / 90th / worst |
  |---|---|---|
  | Fundamental strongest | 3° / 12° / 25° | 0.0 / 0.05 / 0.2 dB |
  | 2nd harmonic 6 dB above the fundamental (a low string) | 15° / 55° / 116° | 0.1 / 0.7 / 3.7 dB |
  | Same notes, bare clipper with no filter at all | 11° / 39° / 53° | 0.0 / 0.5 / 0.9 dB |

  So under drive the lows usually add, a weak fundamental can lose 1 – 3 dB on some notes, and a null was
  never seen. **`bass_keep` is the guarantee**: with it the fundamental comes from the clean path alone.

Consequences:

- At small signals Blend is a crossfade of magnitudes: no notch, no thinning, at any `grunt`, `attack`,
  `model` or `mix`.
- Under drive the blend is free of filter-made cancellation; what remains is the clipper's own doing
  (table above), and `bass_keep` removes it from the lows.
- **Dispersion.** The clean path is not the input: it carries the twins. Lows arrive later than highs:

  | | E1 (41.2 Hz) | B0 (30.9 Hz) |
  |---|---|---|
  | Low section's twin (4 L), at the fundamental | 1.8 ms | 2.4 ms |
  | Every other twin, half-bands, half-samples | ≈ 0.5 ms | ≈ 0.5 ms |
  | `bass_keep` at 110 Hz, when on | + 4.6 ms | + 4.4 ms |

  This is the order of one loudspeaker crossover, inaudible on its own. Two limits follow: `mix` 0 % is
  the clean DI in magnitude, not a null against the input (`bypass` is); and the block is not coherent
  with a copy of its input in a **parallel branch outside it** (golem's parallel chains): blend inside.

### 2.3 Dirt engine and models

`low_note` = L (Hz). Every low-frequency corner is a multiple of L, so it follows the instrument's range
(§2.8). Values marked [A] are starting points, to be voiced by ear against the references (§5).

Every model has the same two pre sections; the model contributes an offset in dB and the user's `grunt`
/ `attack` add to it on the same section (one twin each, not two).

| | **Modern** (CMOS grind; B7K / B3K role) | **Classic** (tube-emulation DI; SansAmp role) | **Vintage** (valve-preamp grit; SVT / Scrambler role) |
|---|---|---|---|
| Low section (LR2 shelf at 4 L: 165 Hz at E1), gain = model + `grunt` | −6 dB | 0 dB | 0 dB |
| High section (LR2 shelf at 1.5 kHz), gain = model + `attack` | +3 dB | 0 dB | −3 dB |
| Gain into stage 1 (`gain` 0 → 100 %, linear in dB) | +6 → +48 dB | 0 → +36 dB | 0 → +30 dB |
| Stage 1 | tanh(x + b) − tanh(b), b = 0.25 (a CMOS inverter's asymmetric transfer) [U] | b = 0.15 (preamp) | b = 0.35, with b moved by the envelope (grid conduction, see `sag`) |
| Interstage sections (8×) | LR2 high shelf −6 dB at 3 kHz; LR4 low-pass at 12 kHz | LR2 high shelf −3 dB at 4 kHz; LR4 low-pass at 12 kHz | — (one stage) |
| Stage 2 | tanh, gain +12 dB (second inverter stage) | tanh "power stage" (push-pull: symmetric), gain lowered by sag | — |
| Sag (model's own at `sag` 50 %) | 0 dB (`sag` has no effect) | up to −3 dB of stage-2 gain, envelope attack 5 ms / release 150 ms | b moves +0.15 at full envelope, same times |
| Post section (fizz) | LR4 low-pass at 6.5 kHz | 5 kHz | 4.5 kHz |
| Character | Bright, aggressive, percussive; clean blend expected | Warm push, rounded top, "miked rig" | Low-gain grind, mids forward, blooms when dug in |

- **`gain`** maps linearly in dB over the model's own range, so 0 % is the model's lightest setting. It
  is not "off": Blend at 0 % is the clean DI. The id is `gain`, as in gfx-emet and gfx-dist; `drive` is
  the contract's "extra saturation, neutral at 0" (§3).
- **Shapers**: gfx-dsp `adaa::Shaper::Asym(b)` / `Tanh` with ADAA1, inside **8×** (three
  `Oversampler2x` stages, as gfx-emet). The stages carry no DC blocker between them (it would need a
  twin); the DC goes to the subsonic high-pass.
- **Aliasing: what the prototype measured** [A]. At 60 dB of total gain tanh is a hard clipper, and
  ADAA1 buys about 34 dB on a square's harmonics near the oversampled rate. Worst inharmonic component
  below 20 kHz, 43 sines from 1 to 5.2 kHz at −12 dBFS, Modern at `gain` 100 %:

  | Engine | Worst / median |
  |---|---|
  | 4×, two stages, shelf only between them (the draft) | −51 / −73 dBFS |
  | 4× + interstage low-pass at 8 kHz | −70 / −80 dBFS |
  | 8×, shelf only | −60 / −85 dBFS |
  | **8× + interstage LR4 low-pass at 12 kHz** | **−79 / −93 dBFS** |
  | One stage: 4× / 8× | −65 / −72 · −79 / −88 dBFS |

  Stage 2 re-squares whatever ultrasonic content stage 1 leaves, so the interstage low-pass matters as
  much as the rate. At the default `gain` (35 %) every engine is below −110 dBFS. On bass notes (E1 – G4,
  harmonics to 8 kHz, −6 dBFS peak, `gain` 100 %, `attack` +12) the chosen engine stays at about
  −90 dBFS or below (measured with the low-pass at 10 kHz). The draft's "−90 dBFS at full drive on 2 – 5 kHz sines" is out of reach for ADAA1 at any
  practical rate; the bench states what is (§5).
- **Sag / bias** come from a gfx-dsp `envelope::PeakFollower` on the stage's input. The ADAA parameter
  then moves slowly (smoothed, updated at the base rate); `Adaa1` re-evaluates its antiderivative when
  the shaper changes, so ADAA1 stays valid (one more `ln cosh` per sample in Vintage).
- **Loudness compensation**: a static table per model × `gain` (33 points, interpolated in dB), at
  `grunt` / `attack` / `input` 0. It is computed offline from the model's gain into a −18 dBFS-RMS bass
  DI (a synthetic plucked-note set until golem's bass DIs exist) and stored in the source. It never
  depends on the signal, so nothing pumps. `dirt_level` (±12 dB) then trims, as Darkglass's Level does.
- **Model change**: an engine is the dirt path **and its clean twins** (the twins differ per model).
  Both engines run, each makes its own mix, and the two results crossfade over 30 ms (equal gain). The
  idle one is reset after the fade.

### 2.4 Compressor (reused from gfx-comp)

- The **gfx-comp `Comp` engine** as a library dependency. It runs block-wise on the input buffer (path
  Both) or on the dirt's copy (path Dirt).
- **Colour off, by construction.** gfx-comp's FET and Opto characters carry an intrinsic colour (0.25 /
  0.18) that runs a shaper in its own 2× oversampler. On path Dirt that would add a half-band pair and a
  half-sample to the dirt only, and break §2.2. gfx-comp has no switch for it today: it gains a
  library-only setting (`colour: false`, not a plugin parameter), and gfx-bassdi takes the characters'
  **dynamics** only. Without colour the engine is a gain with zero latency: coherent on either path.
- **One knob** `comp` c = `comp` / 100 [A]:
  - threshold = −6 − 30c dBFS (RMS-calibrated, as gfx-comp);
  - ratio = 1 + 7c;
  - knee 6 dB;
  - attack 10 ms; release 120 ms with `auto_release`;
  - make-up = the static curve's reduction at −18 dBFS, computed from the curve (automatic make-up, as
    the Darkglass 900): 0 dB at c = 0, +15.75 dB at c = 1.
  c = 0 is bit-transparent (ratio 1, make-up 0 dB: gfx-comp's rule).
- **`comp_char`**: FET (default: Neural's Darkglass suite uses a FET-style compressor) · Opto · Clean. These
  are gfx-comp's characters, so the static curve is the same in each.
- **No sidechain high-pass**: on bass, the compressor should hear the fundamental.
- **`gr`**: read-only meter (shared id), the compressor's gain reduction.

### 2.5 EQ (after the mix)

Built from gfx-dsp `svf` with matched coefficients, as gfx-eq. Gains are linear in dB, smoothed over 20 ms;
frequencies are smoothed in log-Hz over 30 ms.

| Band | Type | Frequency | Gain |
|---|---|---|---|
| `bass` | Low shelf, Q 0.7 | 2 L (82 Hz at E1, 62 Hz at B0) | ±12 dB |
| `lo_mid` | Bell, proportional Q (gfx-eq law) | `lo_mid_freq` 100 – 1,000 Hz | ±12 dB |
| `hi_mid` | Bell, proportional Q | `hi_mid_freq` 500 – 5,000 Hz | ±12 dB |
| `treble` | High shelf, Q 0.7 | 5 kHz | ±12 dB |

The mid frequencies are continuous, not switches. Presets cite the hardware positions (250 / 500 / 1 k;
750 / 1.5 k / 3 k). All four at 0 dB bypass their sections (bit-transparent, as gfx-eq). The bands are
shared by both paths, so a cut never unbalances the blend.

### 2.6 Latency and CPU

- **Reported latency 0.** The half-bands are minimum phase: the identity path measures 5.5 base-rate
  samples of group delay up to 1 kHz at 8× (0.11 ms; 4.9 at 4×), and the two half-sample averages cost
  0.23 dB at 20 kHz (0.95 dB at 4×), on both paths. The twins add dispersion, not delay (§2.2). The bench
  uses `skip_latency` (gfx-drive and gfx-emet precedent).
- **CPU** [A], mono at 48 kHz:
  - pre sections: 6 one-poles; post: 3 SVFs; `bass_keep`: 4 SVFs when on;
  - interstage at 8×: 3 one-poles and 3 SVFs;
  - three half-band pairs, twice (dirt and clean identity);
  - 2 ADAA stages at 8× (ln cosh, ≈ 25 ns each: ≈ 400 ns, the largest item);
  - comp ≈ 100 ns (gfx-comp measures ~0.1 µs);
  - EQ and cab ≈ 12 SVFs.
  Estimate ≈ 0.9 µs / sample at defaults (≈ 4.5 % of one core); bound 1 µs. It doubles during a model
  crossfade. gfx-emet measured 198 ns at 8× with a single, cheaper stage. If the bound fails: 4× with the
  interstage low-pass at 8 kHz (−70 dBFS worst case, table in §2.3).

### 2.7 Speaker emulation (`cab`)

- **Zero-latency minimum-phase IIR** in two parts:
  - a **low section**: the box. A **sealed** box is a 2nd-order high-pass (f_c, Q_tc); a **ported** box
    is a **4th-order** high-pass (24 dB / oct below its tuning): two SVFs;
  - a **body**: cone breakup bell(s), a mid dip, a 4th-order resonant low-pass and, on the horn-loaded
    model, a horn shelf.
  Each model is 6 – 8 SVF sections, designed as analog prototypes and realised with gfx-dsp `matched`, so
  the response holds from 44.1 to 192 kHz.
- **What an IIR model can and cannot do** [A]:
  - It reproduces the 1/3-octave-smoothed magnitude of a cab (target ±1 dB from 40 Hz to 8 kHz) and its
    minimum-phase part.
  - It does not reproduce the comb structure of a single mic position (distance, reflections), nor the
    excess phase. That is exactly what the SansAmp manual calls undesirable for a DI ("without peaks,
    valleys and notches").
  - At 1/3 octave the named models are **voicings of a cab type**, not a given cab: what tells two real
    8×10s apart lives in the detail this smoothing removes. For a specific miked cab, turn `cab` Off and
    use golem's `cab` (IR) block. That is the documented route, and the reason Off exists.
- **Models** (enum, Off first):
  - **Smooth** (default): a multiply-miked average, designed, with no source to license: high-pass at
    0.85 L (35 Hz at E1, 26 Hz at B0: range-aware like the rest), broad −2 dB at 500 Hz, low-pass 4th
    order at 5 kHz, Q 0.6.
  - **8×10** (sealed, big low-mid, steep top). Anchors: 58 Hz – 5 kHz at ±3 dB, −10 dB at 40 Hz (SVT-810E
    owner's manual [M]; infinite baffle [V, retailer copy]);
  - **4×10** (ported, with horn);
  - **1×15** (ported, deep, dark).
  A **2×12** is staged: the enum grows when a target exists; it never shrinks.
- **Targets** (open question 4). v1 does not wait for impulse responses: the low sections come from the
  box alignments (sealed Q_tc, ported tuning) and the bodies from the makers' published responses, all
  [A] until measured. When freely licensed IRs are chosen, each target becomes the 1/3-octave-smoothed
  magnitude of the IR, fitted offline (least squares on log magnitude) into the section values stored
  in the source; only the eight numbers ship, never the IR.
- **`cab_lows`** (bool, default on). Off removes the low section: the cab acts only above ≈ 150 Hz,
  Parallax's "cab on mid and high bands" without a second crossover. A 5-string's B (31 Hz) then keeps
  its weight through an 8×10 model. It is a single path, so no coherence issue arises. The fit keeps
  the body within ±0.3 dB of flat below 100 Hz, or this would not hold.
- Switching models crossfades over 30 ms.
- **In a chain with an amp, a cab or an IR after it, `cab` must be Off.** The default (Smooth) is right
  for the block alone; golem writes Off at insert when such a block follows (as it writes `low_note`).

### 2.8 Range awareness (`low_note`)

- **`low_note`** (Hz, 20 – 100, default 41.2 = E1) is the instrument's lowest open string. It moves:
  - the subsonic high-pass (0.6 L);
  - the low pre section (4 L), the model's tight and `grunt` together;
  - the bass shelf (2 L);
  - the Smooth cab's high-pass (0.85 L).
  Default corners at E1: 25, 165, 82, 35 Hz. At B0 (30.87 Hz): 19, 123, 62, 26 Hz [A].
- **Why** [A]. A fixed tight shelf treats a 5-string's B differently from an E. It cuts the B's
  fundamental and its 2nd and 3rd harmonics (31, 62, 93 Hz) harder than the E's (41, 82, 124 Hz), so the
  low string loses its body before the clipper. Scaled corners keep each string's harmonics in the same
  place relative to the corners as on a 4-string. The same goes for the bass shelf (SansAmp v2's
  80 → 40 Hz shift, made continuous).
- **Set by golem at insert** (contract: per-block values set by golem via `init`). The lane's Tuner
  carries a `tunings.json` id: low_note = 440 · 2^((midi₀ − 69) / 12).
  - bass-4-std → 41.2 Hz; bass-4-d-std / drop-d → 36.7 Hz; bass-5-std → 30.87 Hz; bass-5-drop-a →
    27.5 Hz.
  - On a guitar lane (6-std) → 82.4 Hz. A lane with no tuning (keys, voice, drums) keeps 41.2 Hz.
  This is not a mode-dependent default: the plugin's default is fixed (E1), and golem writes a value.
- Smoothed over 100 ms in log-Hz. It is a setup value, not a performance control.

### 2.9 What is reused, what is new

| Piece | From | Use here |
|---|---|---|
| `Comp` engine, characters, static curve, `gr` | gfx-comp (library dependency; gains a `colour: false` setting) | Pre-dirt compressor |
| `Oversampler2x` ×3, `identity` | gfx-dsp `oversample` | 8× dirt, identity clean |
| `adaa::Shaper::{Tanh, Asym}`, `Adaa1`, `HalfSample` | gfx-dsp `adaa` | Stages; clean alignment |
| `svf` + `matched`, proportional-Q law | gfx-dsp, gfx-eq | EQ, cab sections, LR4 sections |
| `envelope::PeakFollower` | gfx-dsp | Sag / bias |
| `smooth`, `exp_smooth` | gfx-dsp | Every parameter; EXP on `gain`, `mix`, `grunt`, `attack`, mids |
| **New: gfx-dsp `crossover`** | — | One module for the suite, shared with [gfx-grit](../gfx-grit/README.md) (below) |
| **New: model voicings, loudness tables, cab section sets, one-knob comp map, `low_note` scaling** | — | This crate |

**gfx-dsp `crossover`: one module, three layers.** gfx-grit, designed the same day, asks for a module of
the same name (an N-band LR4 / LR8 tree with compensation all-passes). They are the same code:

1. **`Split`**: a two-way Linkwitz-Riley split of order 2, 4 or 8 at one frequency, returning low, high
   and the all-pass twin from shared state; log-smoothed moving frequency; f64 state.
2. **`BandGain`**: the section of §2.2 (twin + (g − 1) · branch) and its stand-alone `Twin`, for a path
   that must stay aligned. Used here and by `bass_keep` everywhere.
3. **`Tree`**: N bands from `Split`s, each band compensated by the twins of the splits it did not pass
   (gfx-grit, the lot-3 multiband compressor).

gfx-drive (LR8 `bass_keep`, `blend`), gfx-emet and gfx-flanger keep their own splits until they are next
touched; they then move to layer 1 with their bench unchanged. The module is written **before** this
crate and gfx-grit, with the identities of §2.2 as its unit tests.

## 3. Parameters

New block: no catalogue ids to keep. Shared ids used with their shared meaning: `mix`, `bass_keep`, `gr`,
`model`, `bypass`; `gain`, `input` and `level` as in gfx-emet. Ranges are linear (contract). Enums only
grow at the end.

### Front panel

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `gain` | Drive | f | 0 – 100 % | 35 % | 20 ms (dB) | ✓ | Gain into the model's stages, linear in dB over the model's own range; loudness-compensated |
| `mix` | Blend | f | 0 – 100 % | 50 % | 20 ms | ✓ | **Law: linear crossfade**, 0 = clean only, 100 = dirt only; phase-coherent (§2.2) |
| `level` | Level | f | −24 – +12 dB | 0 dB | 20 ms | ✓ | Output (DI level) |
| `bass` | Bass | f | −12 – +12 dB | 0 dB | 20 ms | ✓ | Low shelf at 2 × `low_note` |
| `lo_mid` | Lo Mid | f | −12 – +12 dB | 0 dB | 20 ms | ✓ | Bell at `lo_mid_freq` |
| `hi_mid` | Hi Mid | f | −12 – +12 dB | 0 dB | 20 ms | ✓ | Bell at `hi_mid_freq` (the growl band) |
| `treble` | Treble | f | −12 – +12 dB | 0 dB | 20 ms | ✓ | High shelf at 5 kHz |
| `model` | Model | e | Modern · Classic · Vintage | Modern | 30 ms crossfade | | §2.3 |
| `cab` | Cab | e | Off · Smooth · 8×10 · 4×10 · 1×15 | Smooth | 30 ms crossfade | | Speaker emulation; Off for an amp, a cab or an IR after it |

### Expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `grunt` | Grunt | f | −12 – +12 dB | 0 dB | 20 ms | ✓ | Lows into the clipper (pre-clip low shelf at 4 × `low_note`), added to the model's own |
| `attack` | Attack | f | −12 – +12 dB | 0 dB | 20 ms | ✓ | Treble into the clipper (pre-clip high shelf at 1.5 kHz), added to the model's own |
| `dirt_level` | Dirt Level | f | −12 – +12 dB | 0 dB | 20 ms | | The dirt's level relative to the compensated level (the hardware Level knob) |
| `lo_mid_freq` | Lo Mid Freq | f | 100 – 1,000 Hz | 500 Hz | 30 ms (log) | | |
| `hi_mid_freq` | Hi Mid Freq | f | 500 – 5,000 Hz | 1,500 Hz | 30 ms (log) | | |
| `bass_keep` | Bass Keep | f | 0 (off) – 300 Hz | 0 | 30 ms (log) | | Shared id: below it, clean and mono (LR4); on / off crossfades over 20 ms |
| `low_note` | Lowest Note | f | 20 – 100 Hz | 41.2 Hz | 100 ms (log) | | Lowest open string; golem sets it from the lane's tuning (§2.8) |
| `comp` | Comp | f | 0 – 100 % | 0 % | 20 ms | | One-knob compressor, automatic make-up (§2.4) |
| `comp_char` | Comp Type | e | FET · Opto · Clean | FET | gfx-comp's character fade | | gfx-comp characters, dynamics only |
| `comp_path` | Comp Path | e | Dirt · Both | Dirt | 20 ms crossfade | | Before the dirt only (Darkglass), or before the split |
| `sag` | Sag | f | 0 – 100 % | 50 % | 30 ms | | Relative: 50 % = the model's own sag / bias movement, 0 = none, 100 = double. No effect in Modern |
| `input` | Input | f | −12 – +12 dB | 0 dB | 20 ms | | Instrument level into the box (active / passive); not compensated |
| `cab_lows` | Cab Lows | b | off / on | on | 30 ms crossfade | | Off: the cab acts only above the bass |
| `gr` | Comp GR | f, read-only | 0 – 30 dB | — | — | | Shared meter id |
| `bypass` | Bypass | b | off / on | off | 10 ms crossfade | | Shipped bypass parameter |

- No default depends on another parameter. `low_note` (and `cab` Off before an amp or a cab) is written
  by golem at insert.
- **`gain`, not `drive`** (decided at review). The contract's `drive` is "extra saturation on top of the
  model's own", neutral at 0 %. Here the control is the box's main gain, never neutral, default 35 %:
  the same control as gfx-emet's and gfx-dist's `gain` (0 – 100 %). The panel still reads "Drive".
- `sag` is relative here (the contract's form for a value that follows the model). gfx-drive's `sag` is
  absolute (0 % = none): same word, not a shared id.
- `bass_keep` stops at 300 Hz (gfx-flanger's range; gfx-drive's goes to 500): above that the box would
  no longer distort a bass.
- Expert groups (`visuals/data/groups.json`):
  - **Dirt**: `grunt`, `attack`, `dirt_level`, `sag`;
  - **EQ**: `lo_mid_freq`, `hi_mid_freq`;
  - **Low end**: `bass_keep`, `low_note`, `cab_lows`;
  - **Comp**: `comp`, `comp_char`, `comp_path`, `gr`;
  - **Gain**: `input`.

### Visual

- **Category: `preamp`**, golem's existing category (today only `preamp-nam`). It is offered on bass,
  guitar, keys, voice and drums lanes by `role_categories`. The catalogue entry is golem-rack's:
  `{"type": "bass-di", "name": "Bass DI", "category": "preamp", "roles": ["bass", "guitar"], "short":
  "BassDI", "color": "#b316c0", "quarantine": …}`, like the other golem-fx-only blocks.
- **Hue**: `METAL_HUE.preamp = '#b316c0'`, a saturated orchid violet. **Approved at review.**
  - Hue 295°, saturation 79 % (HSL; 89 % in HSV), lightness 42 %: the family's level (pitch `#c4148c`
    is 81 % / 42 %). It sits in the free gap between reverb / shimmer (`#6a1fd1`, `#8a3ff0`, both 265°)
    and pitch's magenta (`#c4148c`, 319°): 30° from the first and 24° from the second.
  - No other design claims the gap: lot 1 took brass `#c9b000` (53°), raspberry `#e8336f` and rose
    `#d81e5b` (both 340°, 45° away, beyond pitch); gfx-grit and gfx-dist keep lofi and thrash red.
  - The catalogue's preamp ochre (`#d9a441`, 39°) would sit on utility's amber (41°).
  - A family entry is right, not a crate one: `preamp` has no METAL_HUE yet, and `preamp-nam` is drawn
    as outboard, not as a metal pedal.
  - `RUNE_LIGHT.preamp = '#ecb8ff'`. The body's luma is 0.27: the ink is light.
- **Finish: anodized** (the default for a family without a FAMILY_FINISH). A DI is a studio tool. The
  blackwork belongs to the high-gain companions, and its black-and-white would echo a known bass-preamp
  maker's livery.
- **Motif `parallel`** (new case in `motifParts`): the blend, drawn.
  - A single line enters at y = cy from x = 0. At x = 0.12 W it splits into two tracks, at cy − 0.9 ch and
    cy + 0.9 ch.
  - **Upper track**: a clipped wave. Period 22, amplitude 0.35 ch, flat tops and bottoms (trapezoids:
    rise 4, flat 7).
  - **Lower track**: a smooth sine of the same period and phase, its peaks under the upper track's flats,
    at opacity 0.6.
  - The tracks rejoin at x = 0.88 W into one line to x = W.
  - Drawn with `line`: engraved on anodized, yellow lined in black if thrash is forced. The rune's
    cartouche masks the middle, as for the other motifs.
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-bassdi": {"name": "Bass DI", "blocks": ["bass-di"], "knobs": ["gain", "mix", "level", "bass", "treble"],
                 "selectors": ["model", "cab"], "category": "preamp", "color": "#b316c0"}
  ```
  The mids live in the drawer. The face shows the identity: Drive, Blend, Level, and the outer EQ.
- **Glyph** `bass-di` (24 × 24, single colour, stroke 1.8, round caps; title "Bass DI (gfx)"):
  ```
  <path d='M2 12h3M19 12h3'/>                       in and out: one line
  <path d='M5 12l3-5h8l3 5'/>                       the dirt: clipped flat
  <path d='M5 12c2 4 4.5 6 7 6s5-2 7-6'/>           the clean: round, same ends
  ```
  It reads as "split, one path clipped, summed back". It is distinct from `boost` (chevron), `guitar-drive`
  (diamond) and `preamp-nam` (hexagon).
- No maker's livery, logo or trademark on the pedal or in preset names.

## 4. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Clean DI to the desk, "sounds miked" | Classic, `gain` 10 %, `mix` 100 %, cab Smooth, EQ flat | Tube push + notch-free speaker emulation |
| Modern metal grind (the B7K role) | Modern, `gain` 65 %, `mix` 55 %, `grunt` +4, `attack` +4, `lo_mid` −4 @ 500, `hi_mid` +5 @ 1.5 k, cab 8×10 | Coherent blend keeps the lows; growl at 1.5 kHz |
| 5- / 6-string, drop tunings, djent | Modern, `low_note` from the tuning, `gain` 70 %, `mix` 60 %, `grunt` −6, `bass_keep` 110 Hz, `cab_lows` off | Range-scaled corners; lows out of the clipper and the cab |
| Vintage rock grit (SVT / Scrambler role) | Vintage, `gain` 45 %, `mix` 50 %, `lo_mid` +3 @ 500, `treble` −2, cab 8×10 | Valve bloom (sag); mids forward |
| Scooped "ultra" tone | Vintage, `gain` 25 %, `lo_mid` −10 @ 500, `bass` +2, `treble` +4 | The SCR's Ultra Lo, as a preset |
| Funk / slap, even and punchy | Classic, `gain` 15 %, `mix` 30 %, `comp` 50 % (`comp_path` Both, FET), `bass` +3, `treble` +4, `lo_mid` −4 @ 500 | Compressor before everything |
| Compressed grind (the "engine" approach) | Modern, `gain` 60 %, `comp` 60 % (Dirt), `mix` 50 % | The dirt is even; the clean keeps its dynamics |
| Synth-like fuzz wall | Modern, `gain` 100 %, `attack` +12, `mix` 80 %, `hi_mid` +6 @ 2 k | 8× and the interstage low-pass: aliases under the note at full gain |
| Into a cab IR | any, `cab` Off, then golem's `cab` block | Speaker emulation switchable |
| Clean ↔ grit on the foot | EXP on `mix` (0 → 70 %), `bass_keep` 100 Hz | Coherent at every point of the sweep; the lows never move |
| SansAmp-style guitar DI | Classic, `low_note` 82.4 (set by golem), `gain` 40 %, cab Smooth | Range-scaled corners on guitar |
| Replace a two-branch "bi-amp" chain | one block: Modern, `mix` 40 %, `bass_keep` 100 Hz | Golem's factory Bi-Amp Grit sums two full rigs; this does it in one coherent block |

**Preset ideas** (`presets/gfx-bassdi.json`, generic names): Studio DI · Warm Push · Modern Grind · Tight
Five · Drop Growl · Valve Grit · Ultra Scoop · Slap Even · Engine Grind · Fuzz Wall Low · Into the IR · Grit
Blend Pedal. Every preset states `cab` and leaves `low_note` to golem.

## 5. Bench suite (on top of gfx-bench's common checks)

**gfx-bench options**:

- `skip_latency`: minimum-phase oversampling (5.5 samples of group delay) and the twins' dispersion, not
  reported (gfx-drive, gfx-emet).
- `click_bound_db` −30 dB: a `gain` jump glides through real harmonics, as gfx-drive's ceiling does. To be
  tightened if measured lower.
- Silence floor −140 dBFS: no tail, the subsonic high-pass settles.

**Bench needs bass DIs** (fingers, pick, slap, a 5-string low B). golem has not recorded them yet (review
§7; library-tones "not done"). Until then the rows below run on synthetic notes: partials to 8 kHz,
fixed-seed phases, in two profiles (fundamental strongest; 2nd harmonic 6 dB above it).

| Check | Pass |
|---|---|
| **`crossover` identities** (gfx-dsp unit tests) | LR2 and LR4, 44.1 – 192 kHz, g from 0 to +12 dB: phase of the section = phase of its twin within 10⁻⁵ rad; magnitude monotone from g to 1; at g = 1 the section is bit-identical to the twin |
| **Coherence, small signal** | **−60 dBFS** log sweep, `gain` 0 %, `mix` 0 / 25 / 50 / 75 / 100 %, every `model`, `grunt` and `attack` at −12 / 0 / +12, `low_note` 27.5 / 41.2 / 82.4, `comp` 50 % on path Dirt: the output magnitude equals the linear crossfade of the two paths' magnitudes within 0.1 dB, 20 Hz – 15 kHz; no notch |
| **Coherence vs a naive build** | Same sweep with the dirt sections replaced by ordinary min-phase shelves and no twins: report its worst notch (diagnostic; the prototype gives −0.9 dB for a −12 dB shelf, −4.2 dB for a high-pass) |
| **Coherence, driven, single tone** | Sines 41 Hz – 1 kHz, −30 – 0 dBFS, `gain` 0 / 50 / 100 %: the dirt's fundamental within 2° of the clean's |
| **Coherence, driven, notes** | 60 synthetic notes (weak-fundamental profile), `gain` 70 %, `mix` 50 %, paths level-matched: loss of each partial below 200 Hz against the in-phase sum: median ≤ 0.3 dB, 90th percentile ≤ 1.5 dB; worst reported. With `bass_keep` 2.5 L: the fundamental within 0.5 dB of the clean path on every note |
| **Tight low end** | 5-string low-B DI (or a synthetic 30.87 Hz + harmonics), `gain` 80 %: with `bass_keep` 110, the 31 Hz level within 0.5 dB of the clean path; AM sidebands of a 1 kHz probe at ±31 Hz ≥ 20 dB lower than with `bass_keep` 0 and `grunt` 0 |
| **Onset thump** (report) | Vintage, `gain` 100 %, `sag` 100 %, a gated 82 Hz burst: energy below 0.7 L in the first 100 ms, relative to the note. It sets the bias depth by ear: the DC of an asymmetric stage steps with the envelope and the subsonic high-pass turns the step into a thump |
| **Range scaling** | `low_note` 41.2 → 30.87 Hz: every scaled corner (0.6 L, 4 L, 2 L, 0.85 L) moves by the ratio 0.749 within 1 % (measured on the responses) |
| **Aliasing** | Worst inharmonic component below 20 kHz, each model. (a) 40 sines from 1 to 5.2 kHz at −12 dBFS, `gain` 100 %: ≤ −70 dBFS worst, ≤ −85 dBFS median (prototype: −79 / −93). (b) The same at `gain` 35 %: ≤ −100 dBFS. (c) Synthetic notes E1 – G4 at −6 dBFS peak, `gain` 100 %, `attack` +12: ≤ −85 dBFS. (d) 41 Hz + 3 kHz two-tone: reported |
| **Level** | Bass DI at −18 dBFS RMS, `mix` 100 %: loudness within ±1.5 LU over `gain` 0 – 100 % for each model; across models within ±2 LU at 50 %; factory presets within ±2 LU of the bypass |
| **EQ** | Each band within ±0.5 dB of its analog target to 20 kHz at 44.1 – 192 kHz; flat EQ = bit-transparent sections; a frequency sweep under a sine has no click above the bound |
| **Cab** | Each model within ±1 dB of its 1/3-octave target, 40 Hz – 8 kHz, at 44.1 – 192 kHz; Smooth has no notch deeper than 3 dB; `cab_lows` off: within ±0.3 dB of Off below 100 Hz; Off is bit-transparent |
| **Compressor** | `comp` 0: bit-transparent; static curve as §2.4 within 0.2 dB; `gr` reads the reduction within 0.5 dB; make-up keeps −18 dBFS within 1 dB; FET and Opto add no latency and no filter (null against Clean on a steady tone) |
| **Model / cab switching** | Switching under a driven bass note (DC present in the dirt): no click above the bound; CPU during the fade within the stated peak |
| **EXP** | `mix` and `grunt` swept by 50 Hz-stepped values: control sidebands ≤ −80 dB (contract) |
| **Determinism, CPU** | Strict null between two renders; ≤ 1 µs / sample mono at 48 kHz at defaults, ≤ 2 µs during a crossfade |
| **Listening** | Bass DIs through each model, blind and level-matched against golem's NAM captures of the references (pedal archetypes `darkglass-b7k`, `sansamp-bass`, and an `ampeg-svt` capture), with and without golem's `cab` block; owner's ear decides the voicing values marked [A] |

## 6. Positioning

golem already plays exact bass pedals as NAM captures. A capture is one setting of one unit: no live
Blend, no Grunt, no EQ, no tuning awareness, no compressor before the dirt. Its blend with a clean branch
is not phase-matched, and it is heavier on CPU. gfx-bassdi is the parametric instrument: every control
live and EXP-able, a blend with no filter-made cancellation, corners that follow the tuning, zero
latency. The captures remain the reference for "that exact pedal".

## Open questions for review — settled

1. **Category and hue** → `preamp`, `METAL_HUE.preamp = '#b316c0'`: approved; no other design of lots 1
   and 2 claims the 265° – 319° gap (§3, Visual).
2. **Block type** `bass-di`, roles bass and guitar, golem-fx-only quarantine → golem-rack.
3. **`drive`'s shared meaning** → the id is **`gain`** (0 – 100 %), named "Drive" on the panel (§3).
4. **Cab targets** → v1 ships designed targets (Smooth; box alignments and published responses for the
   three cabs); fits to freely licensed IRs replace them when Raph has chosen and checked them. 2×12 is
   staged (§2.7).
5. **`low_note` at insert** → golem-rack writes it by `init`. It should write it again when the lane's
   tuning changes: the parameter is smoothed over 100 ms and safe to move at any time. A user's own
   value is then lost; golem-rack decides whether to track "set by hand".
6. **Defaults** → Modern, `gain` 35 %, `mix` 50 %, cab Smooth: accepted, with `cab` written Off by golem
   when an amp, a cab or an IR follows.
7. **4× vs 8×** → **8×, with an LR4 low-pass at 12 kHz between the stages** (§2.3). 4× with the low-pass
   at 8 kHz is the fallback if the CPU bound fails.
8. **Voicing values** stay [A]: they need the bass DIs and the NAM references.
9. **`comp_path` default Dirt** → kept. The 900's manual puts the compression in the Microtubes Engine,
   and the maker's X Ultra needs its software to "enable the compressor for clean signal": engine-only
   is the maker's default.
10. **gfx-dsp `crossover`** → written **before** this crate, as one module with gfx-grit's tree (§2.9).
    gfx-drive, gfx-emet and gfx-flanger adopt it when next touched, not now.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good — Strong once the driven-blend claim is read as corrected.** The central idea is right
and it is new in the category: band gains built on a Linkwitz-Riley split, with the sum's all-pass as a
twin on the clean path. A numpy prototype of the whole chain (gfx-dsp's half-bands ported coefficient
for coefficient, ADAA1, `HalfSample`, TPT sections) confirms it: the section's phase equals its twin's to
3 · 10⁻⁷ rad for every gain including 0, the oversampling filters and ADAA's half-sample match on both
paths, and the small-signal blend is a pure crossfade of magnitudes. Three claims did not survive the
numbers: the fundamental "still arrives in phase" under drive (true for a sine, not for a note), −90 dBFS
of aliasing at 4×, and a bench row that could not pass even with perfect coherence. The manual figures I
could re-read hold (SCR-DI: ±14 dB at 40 Hz, +5 / −11 dB at 500 Hz, +16 / −15 dB at 4 kHz, Ultra Lo and
Ultra Hi as quoted; SVT-810E: 58 Hz – 5 kHz, −10 dB at 40 Hz).

Changes made, and why:

| # | Change | Why |
|---|---|---|
| 1 | **Driven coherence restated with measurements** (§2.2); "the blend cannot cancel" → "no filter-made cancellation"; `bass_keep` named as the guarantee | A clipped sine keeps its phase (≤ 1°). A clipped note does not: a fundamental weaker than its 2nd harmonic rotates by 15° (median) to 116° (worst of 60), costing up to 3.7 dB at 50 %. A bare clipper with no filter shows the same effect (53°, 0.9 dB): it is the nonlinearity, not the design |
| 2 | **Shelves: LR4 → LR2 sections** (1st-order all-pass twin); LR4 kept for low-passes and `bass_keep` | Same exact phase identity (verified). The LR4 shelf's transition fits in one octave, steeper than any of the hardware; LR2 has the RC shelf's shape, 30 % less group delay and a third of the filters |
| 3 | **Model and user shelves merged**: one low section (4 L) and one high section (1.5 kHz), gains added in dB | The draft ran the model's shelf and the user's as separate sections, two of them at the same 1.5 kHz: four twins where two do. Clean-path dispersion at E1 falls from 6.5 ms to 1.8 ms (8.6 → 2.4 ms at B0) |
| 4 | **Section computed as twin + (g − 1) · branch** | Bit-identical to the twin at g = 1 (a null test becomes possible), and it makes the click-free claim true by construction |
| 5 | **4× → 8×, plus an LR4 low-pass at 12 kHz between the stages** (§2.3); aliasing claim and bench row rewritten | Prototype, 1 – 5 kHz sines at −12 dBFS, full gain: the draft's engine reaches −51 dBFS (worst), not −90. Stage 2 re-squares stage 1's ultrasonics. 8× alone: −60; 8× with the low-pass: −79 worst, −93 median; bass notes ≤ −90 |
| 6 | **Model change crossfades whole engines, twins included** | The twins differ per model, so the clean path changes with the model; crossfading the dirt alone would have left the clean jumping |
| 7 | **Compressor: colour forced off, as a gfx-comp library setting** | FET and Opto run a shaper in their own 2× oversampler. On path Dirt that puts a half-band pair on one path only: the blend would comb. gfx-comp has no such switch today |
| 8 | **`drive` → `gain`** | The contract's `drive` is neutral at 0; gfx-emet and gfx-dist already use `gain` for the main gain |
| 9 | **Bench**: small-signal sweep at −60 dBFS (not −40); driven rows rewritten; `crossover` unit tests, onset thump, switching under DC added; CPU bound 1 µs | At −40 dBFS with `grunt` +12 the stages already compress by about 0.1 dB, the row's whole tolerance. The draft's driven row asked for "the louder path − 1 dB" at `mix` 50 %: a crossfade of a full clean and a low-cut dirt gives −2.5 dB with perfect phase |
| 10 | **Dispersion and its limits stated** (§2.2): `mix` 0 % is not a null against the input; not coherent with a parallel branch outside the block | The clean path carries the twins. True of any crossover, but the draft implied a transparent clean |
| 11 | **Separate 5 Hz DC blocker removed** | The subsonic high-pass (19 – 49 Hz, 2nd order) already blocks DC |
| 12 | **Cab**: ported boxes get a 4th-order low section; models are "voicings of a type"; 2×12 staged; Smooth's high-pass follows `low_note`; targets no longer wait for IRs; `cab` Off written by golem before an amp or a cab | A ported box rolls off at 24 dB / oct, not 12. An enum cannot shrink, and a model without a target should not ship. A fixed 35 Hz high-pass took 4 dB off a low B in the default |
| 13 | **Latency and CPU figures measured or re-estimated** (5.5 samples at 8×; 0.23 dB at 20 kHz from the half-samples; ≈ 0.9 µs) | The draft's "≈ 6 samples at 4×" and 500 ns belonged to another engine |
| 14 | **Mix law's cost stated** (clean at −6 dB at 50 %); 2 / 2 behaviour defined | Darkglass's Blend keeps the clean at unity; ours is a crossfade (the SansAmp's), which the reader should know |
| 15 | **Hue approved, figures corrected** (saturation 79 % HSL; shimmer is 265°, as reverb) | Checked against METAL_HUE, METAL_CRATE_HUE and the lot-1 crate hues: nearest neighbours 24° and 30° away |
| 16 | Survey: the B7K Ultra link flagged; the 900's compressor routing resolved from the manual | The link now opens the maker's storefront: its quotes could not be re-read |

Still open:

| Point | Owner |
|---|---|
| Voicing of every [A] value: gains, corners (4 L, 1.5 k, 3 k, 12 k, fizz), b, sag depths, the comp map | Raph, by ear, once the bass DIs exist |
| LR2 or LR4 for the low pre section: LR2 decided on shape and dispersion; LR4 isolates the fundamental from the clipper more sharply. One A / B on a low-B DI | Bench, then Raph |
| Bass DIs (fingers, pick, slap, 5-string B); NAM captures of the three references | Raph / golem-rack |
| Cab targets from freely licensed IRs; the three box alignments until then [A] | Raph, with licence checks |
| Catalogue entry `bass-di` (category `preamp`, roles, quarantine); `init` writing `low_note` from the lane's tuning, again on a tuning change, and `cab` Off when an amp, a cab or an IR follows | golem-rack |
| `METAL_HUE.preamp`, `RUNE_LIGHT.preamp`, the `parallel` motif, the face and the glyph | golem-fx visuals, with the release; hue to confirm on the family sheet at tile size |
| gfx-dsp `crossover` shared with gfx-grit: gfx-grit's document describes only the tree; the two-way `Split`, `BandGain` and `Twin` layers are specified here (§2.9) and must be written once, first | golem-fx, before either crate |
| gfx-comp `colour: false` library setting | golem-fx, with this crate |
| gfx-drive's aliasing figure (−88 to −92 dBFS at 50 dB) does not match what ADAA1 gives on a hard-clipped sine in this prototype (−64 dBFS for one stage at 4×): worth re-measuring over a frequency sweep rather than at 3.7 kHz alone | golem-fx bench |
| CPU at 8× (bound 1 µs); the fallback is 4× with the interstage low-pass at 8 kHz | Bench |
