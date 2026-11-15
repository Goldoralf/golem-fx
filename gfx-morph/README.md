# gfx-morph

> **Status: CODED (v0.1.0, 2026-11-15).** Designed and reviewed by Fable 5.2 the same day (Review
> section; its decisions override the body where they differ). Bundle name "GoleM FX Morph", golem block
> type **`morph`**, display name **Morph**. Built as designed, in the review's order: gfx-grit's curves
> moved verbatim to gfx-dsp `shapers` (50 gfx-grit renders and its tables byte-identical), a new gfx-dsp
> `modal` (48 TPT modes in f64 lanes, rank morph, edge fades), then this crate: the guard's whole-pedal
> cap, the de-fizz drive duck and shelf, Single / Two / Three bands at 2× / 4× with 15 types A / B and the
> live ρ gain, vocal make-up and small-signal tables generated at build time, nine physical objects with
> their level, peak and voice tables, the comp, post filter, slap echo and three read-only meters; 45
> parameters, 17 presets, zero latency. Measured: group delay at 1 kHz 0.089 / 0.120 / 0.207 / 0.346 ms
> (Single 2×, 4×, Two, Three); the clean skeleton flat within 0.031 dB; the phrase held within 0.4 dB by
> every table; pink bleed under the guard at or below bypass in every third-octave band with every
> object, `reso`, `comp`, the echo and `tilt`; de-fizz −8.9 / −14.8 dB of hash at 50 / 100 %; the
> objects' pink level within 0.59 dB off the table's points and their voice level within 0.7 dB; 27
> behaviour tests, the bench 28 / 28 at every rate; CPU at the defaults on par with gfx-grit's measured
> beside it (As coded, at the end, with the deviations: Fuzz and Crush laws shortened, one-stage droop
> correction, S measured at −90 dBFS, and the CPU of the heavy rows on a loaded machine). Prototypes in
> `proto/` (numpy / scipy, plus a Rust CPU timing in `proto/rs/`) and the reviewer's scripts in
> `proto/review/`; every number marked [P] below is theirs, re-run in `proto/results.txt`.

golem's **live vocal mangler**: one pedal that turns a voice into a scream, a megaphone, a broken
transmission, a robot or a voice shouted into a can. It is built for a singer on stage, so its latency
is zero. It also plays guitar and drums. The chain is the one the "trash" family of plugins made
popular:

**pre filter → multiband dirt with two morphing types per band → an object (box) → compressor → post
filter → slap echo**

Everything runs at zero latency: minimum-phase crossovers and half-bands, ADAA shapers and a modal
object bank (IIR, no FFT). Mono 1 / 1 first, 2 / 2 second. **Reported latency 0.** The wet carries
0.06 – 0.10 ms of half-band group delay at 1 kHz [P] (0.09 ms through the default chain with its 100 Hz
low cut), plus the crossovers' all-pass dispersion when the dirt is split (§2.10). Nothing looks ahead.

What it does that no golem pedal does today, and that the commercial references do not do for a live
voice:

1. **A guard on the pedal's loop gain.** A distortion lifts small signals: +8 dB of stage bleed at
   24 dB of drive, +27 dB at 48 dB [P]. That is why the hardware vocal distortion pedals feed back on
   stage (§1.5). The guard is a downward gate before the dirt whose depth is the pedal's whole
   small-signal gain at its worst frequency (the dirt at the current `mix`, the object's strongest peak,
   the filter's resonance, the comp's make-up, the echo's comb) plus 3 dB. Below its threshold, the
   pedal is quieter than bypass at any drive and at every frequency (−3 dB [P]), and a voice above it
   passes untouched. It covers the gaps between phrases. It is not a feedback suppressor: while the
   singer sings, bleed still rides 2 – 11 dB up [P], and a howl louder than the threshold holds the
   gate open (§2.2).
2. **De-fizz that works in front of a distortion.** A de-esser placed before or after a heavy clipper
   barely helps: the clipper levels the "s" back up, and its hash in 1 – 4 kHz stays (p4 [P]). The
   design's law instead lowers the drive while an "s" passes, so the "s" goes through nearly clean, and
   then shelves its band down. Hash in 1 – 4 kHz re the vowel: −4.0 dB without it, −8.3 dB at 50 %,
   −11.3 dB at 100 % [P]; on a real sung take, 5.7 and 9.7 dB less hash on the sibilant frames [P]. The
   vowels move 0.15 dB.
3. **Level that holds for a voice.** The make-up tables are measured on a sung vowel, not on gfx-grit's
   1 kHz sine. With gfx-grit's tables, a voice would drop 3 dB at high drive [P]; with the vocal tables,
   the level stays within ±0.8 dB over the whole drive range on the reference vowels, and within
   0 … −2.0 dB on a real sung phrase, where gfx-grit's tables lose 4.5 – 4.8 dB [P].
4. **Objects made from physics, not from IRs.** The objects are a tin can, a bucket, a pipe, a bottle, a
   cardboard box, an oil drum, a glass jar, a cone and cupped hands. Each is a modal bank built from
   textbook acoustics (tube, cavity, Helmholtz and shell modes), so its `size`, `decay` and `position`
   move continuously under an EXP pedal at a held level, and two objects morph into each other. No third-party IR is
   used, and nothing is recorded.
5. **One morph gesture**: `morph` slides every band from its type A to its type B and the object from A
   to B at once. It is level-held (correlation-aware), and the envelope can drive it.

It is not gfx-grit (four-band Prisma-compatible dirt, no object, no vocal guard), not gfx-radio (a
device chain: mic, channel, speaker, room), not gfx-irfx / gfx-cab (a user IR player). §1.6 draws the
lines.

## 1. State of the art

Survey of 2026-11-15. **[V]** read on a primary page (maker's page, manual, source code) · **[M]**
maker's claim (marketing) not checked · **[I]** secondary source: review, forum, search summary · **[U]**
could not be verified · **[K]** standard literature, not re-checked · **[P]** measured on this design's
prototype (`proto/`).

### 1.1 The reference: iZotope Trash (2024)

Sources: the product page ([izotope.com/products/trash](https://www.izotope.com/products/trash), read
2026-11-15 [V]) and the user manual
([Trash_Manual.pdf](https://docs.native-instruments.com/pdf-guides/trash/Trash_Manual.pdf) [V]). Also
the reviews in [Sound On Sound](https://www.soundonsound.com/reviews/izotope-trash-2024) [I],
[Mix](https://www.mixonline.com/technology/reviews/plug-ins/izotope-trash-a-mix-real-world-review) [I]
and [MusicRadar](https://musicradar.com/reviews/izotope-trash-review) [I], the launch blog
([izotope.com](https://www.izotope.com/community/blog/trash-is-back) [V]) and
[KVR user reviews](https://www.kvraudio.com/product/trash-by-izotope) [I].

| Part | What it is | Source | What we take, or not |
|---|---|---|---|
| Structure | Two modules, **Trash** (distortion) and **Convolve** (IRs); **Swap** reverses their order, per band | manual §6 item 8, product page [V] | `order` (Dirt → Box · Box → Dirt), §2.1 |
| Distortion types | "**69** different distortion types" in categories (manual) [V]; "over 60" (page) [M]; "nearly 70", seven categories Distort, Drive, Faulty, Fuzz, Heavy, Retro, Saturate (SOS) [I] | manual, page | 15 types chosen for voice (§2.4), each a formula with a real unit |
| XY pad | Blends **up to four** distortion types; the pad shows the blended transfer curve. A "Modulation node" sets a path for the envelope | manual §6 [V] | Two types per band (A / B) and one `morph` axis: a pedal has one EXP. Four corners need a 2-D control golem's face has no place for |
| Drive, Mix, Tilt | Drive into the distortion; module Mix; **Tilt EQ before the distortion** with its centre frequency | manual §6 items 10 – 12 [V] | `drive`, `mix`, `tilt` / `tilt_freq` |
| Style | Per-type extra control, "increase the intensity", greyed when unsupported | manual [V] | Not taken: each type's own unit follows `drive` (§2.4); a greyed knob breaks "every parameter audible" |
| Convolve | XY pad of four IRs; "**over 300**" IRs (manual welcome page) [V] vs "**over 600**" (page, blog) [M]; categories incl. Amps, FX, Textures, Tones, Vowels, Reverbs, Reverse, Animals, Devices, Water (SOS) [I]; examples "reverse reverbs, piano cabinets, and plexiglass tubes" [V]; a mic selector Condenser / Dynamic / Ribbon on some IRs [V]; **one custom IR per instance, ≤ 1 s**, wav / aif / mp3 / m4a / flac [V] | manual §7, page | An object bank synthesised from physics (§2.7). No IR library to license or record; the custom-IR use is gfx-irfx's |
| Multiband | **Up to three bands** (low, mid, high) dragged from the edges; per band its own Trash and Convolve, Solo, **Band Gain −40 … +10 dB** [V] | manual §5 | `split` Single · Two · Three, per-band types, trim and level; `bands` as voice names: low / body / air |
| Envelope | Envelope follower (Attack, Release, global Modulation) to **Drive**, the **XY pad** and the **Filter frequency** [V] | manual §8 | `env_drive`, `env_morph`, `env_filter` (§2.8), fixed times (a vocal follower) |
| Filter | One resonant low-pass: Frequency, **Scream** (resonance, self-oscillates), **Heat** (drive into the filter) [V] | manual §9 | `post_lp` + `reso` (no self-oscillation on a live voice: §2.8) |
| In / Out | In Gain, **Auto Gain** ("match the loudness of the input"), **Limiter**, **Oversample**, Out Gain [V]; the page: "switch **4x oversampling** on for the Trash module" [V] | manual §10, page | Static vocal make-up (§2.5); the safety limiter is golem's (gfx-limiter, last on every route); 2× always, 4× on `quality` |
| Latency, CPU | **Not stated** on the page or in the manual [V: absent]. "Oversample increases the plugin's CPU load" [V]. One user reports zero latency in Pro Tools ([Gearspace](https://gearspace.com/board/reviews/1425407-izotope-trash.html) [I]) | | Ours: 0, by construction and benched |
| Presets, randomise | Presets by category (the Mix review names a Vocals category [I]); random preset / types / IRs [V] | | 17 annotated presets (§4); no randomiser (golem's AI picks presets by id) |
| What reviewers say | "lots of fun and very accessible" (SOS) [I]. Trash 2 users miss "Dynamics, EQ and Delay" (Mix [I]), the custom waveshaper and "doesn't distort as hard as Trash 2" (KVR [I]). Type names are non-descriptive ("Citrus Pulp", "Soft Taco": Mix [I]). The IR mic options differ "much more subtle[ly]" than expected (Mix [I]). One custom IR only (SOS [I]) | | Dynamics and a delay are back (§2.8, §2.9); type names say what they do; no mic matrix |

### 1.2 Trash 2 (legacy) and its "box modeler"

Trash 2 is now a legacy product, still available to its owners (product page FAQ [V]). Its chain
(MusicTech review [I], [musictech.com](https://musictech.com/reviews/trash-2-review/)): two **Filter**
modules (six bands, "over 20 different filters", before and after the distortion); **Trash** (up to
**four bands, two distortions in series per band**, "over 60" waveshapes, a **custom drawn
waveshape**); **Convolve** ("over 100" amp, speaker and object IRs, user files); **Dynamics** (a
compressor and a noise gate, up to four bands); **Delay** (six types, with a post filter); a final
**limiter**. A Trash 2 help file (Scribd copy, [I]) gives the default order Filter 1 → Trash → Filter 2 →
Convolve → Dynamics → Delay. Sweetwater's support note says some of the original Trash's **Box Model**
settings "can significantly raise CPU and latency"
([sweetwater.com](https://www.sweetwater.com/sweetcare/articles/lower-izotope-trash-latency-cpu-usage/) [I],
page not readable at survey: search summary only).

What we take: the **whole chain** (pre filter, dirt, box, dynamics, post filter, delay). The 2024
redesign cut half of it, and its users miss it. What we leave: the drawn waveshaper (a drawn curve
cannot carry a closed-form antiderivative, so no ADAA; it is also not a linear parameter) and the
four-band dynamics (gfx-mbcomp exists).

### 1.3 Other commercial references

| Product | What it does best | Latency | What we take, or not |
|---|---|---|---|
| Soundtoys **Decapitator** ([manual](https://www.soundtoys.com/wp-content/uploads/Decapitator-Manual.pdf), via search summary [I]) | Five analog styles. **Punish** (+20 dB into the drive), **Thump** (a low boost at the low-cut frequency, before the saturation), **Steep** (high cut 6 → 30 dB / oct, "a speaker cabinet"), **Auto** output follows Drive, Mix | not stated | Pre-dirt emphasis (`tilt`), a steep post low-pass, auto level, mix |
| Soundtoys **Devil-Loc Deluxe** ([soundtoys](https://store.soundtoys.com/product/devil-loc-deluxe/) [M]; [MusicRadar](https://musicradar.com/reviews/tech/soundtoys-devil-loc-deluxe-469967) [I]) | A crushing level-flattening compressor and distortion after the 1968 Shure Level-Loc. Darkness (tone), release speeds, mix. "In small amounts can add sizzle to … vocals" [M] | — | The compressor after the dirt: crush and evenness are part of the sound (`comp`) |
| FabFilter **Saturn 2** ([help](https://fabfilter.com/help/saturn/using/inputoutput) [V via search excerpt]) | Up to 6 bands, 28 styles, per-band dynamics. **Linear Phase** applies to the crossovers and to the High Quality oversampling, and adds latency; a **32×** "Superb" mode | linear phase: latency, compensated by its own bypass [V] | Confirms the split: zero latency means minimum phase. We give the numbers (§2.10) |
| Output **Thermal** ([SOS](https://www.soundonsound.com/reviews/output-thermal) [I], [MusicRadar](https://www.musicradar.com/reviews/output-thermal) [I]) | Three serial or parallel **stages**, each a band split, one of 19 distortions and a filter / effect; two multistage envelopes; master high / low cut and a compressor | — | A compressor and filters around the dirt; modulation stays out of v1 (golem has gfx-tremolo / EXP) |
| Sugar Bytes **Effectrix 2** ([sugar-bytes.de](https://sugar-bytes.de/effectrix2) [M]; [Bonedo](https://www.bonedo.de/artikel/sugar-bytes-effectric-2-test) [I]) | Up to 14 effects on a 32-step sequencer: loopers, stutter, vinyl stop, crush | — | Out of scope: sequenced glitch is gfx-glitch's and gfx-killswitch's. Named because the brief cites it |
| Ableton **Roar** (gfx-grit §1.2 [V]) | Six routings, 12 shapers, each stage only Amount, Bias, Filter | — | A small face over a deep engine |
| UnplugRed **Prisma** (gfx-grit §1.1 [V]) | golem's current multiband dirt, 4 bands × 2 slots | 0 (golem measured) | Our types reuse gfx-grit's curves and laws |
| iZotope **VocalSynth 2**, Distort module ([izotope.com](https://izotope.com/en/products/vocalsynth/features/distort.html), page rate-limited at survey [U]) | A vocal multi-effect with a distortion stage | — | Not studied further |
| BLEASS **Fusion** (named by a KVR reviewer as doing Trash's morphing "more fully for less money" [I]) | Morphing multi-effect | — | Not studied further [U] |
| TC-Helicon **VoiceTone X1** ([zZounds](https://zzounds.com/item--TCEVOICETONEX1) [M]; [Thomann reviews](https://www.thomann.co.uk/tc_helicon_voicetone_x1_reviews.htm) [I]) | Hardware vocal pedal: three megaphone / telephone effects, two distortions, three amp effects, drive and filter | hardware | **The live problem** in its reviews (§1.5): feedback. Our guard (§2.2) |
| McDSP FutzBox, Audio Ease Speakerphone (gfx-radio §1.1 [V]) | Device simulators: filters, distortion, speaker models, a dropout gate, noise ducked by the input | 0 / some codecs add latency | Device chains are gfx-radio's; we keep its lesson that noise and gates must follow the voice |

### 1.4 Open source and literature

| Source | What it is | Licence | Use here |
|---|---|---|---|
| [GnomeDistort 2](https://soundshockaudio.com/gnomedistort) (Crowbait) [I] | Multiband waveshaping distortion, JUCE | GPL-3.0 [I] | Confirms the type set; not read further |
| [Fire](https://awesome.ecosyste.ms/projects/github.com%2Fjerryuhoo%2FFire) (jerryuhoo) [I] | Up to four bands of distortion, JUCE | AGPL-3.0 [I] | Ideas only (AGPL) |
| [WSTD M3NGLR](https://wasted-audio.itch.io/wstd-m3nglr) [I] | Multiband mangler, LV2 / VST3 / CLAP | GPL-3.0 [I] | Not read |
| [Wolf Shaper](https://github.com/wolf-plugins/wolf-shaper) [I] | Spline-drawn waveshaper (DPF) | GPL-3.0 [I] | The drawn curve we decline (§1.2) |
| No open-source **box / object convolver or modal object effect** for audio input was found [U] | | | The Box stage is new (§2.7) |
| gfx-grit, gfx-radio, gfx-drive, gfx-acoustic (ours) [V] | ADAA shapers and the Prisma laws (grit); carbon mic, rail sag (radio); Fuzz core (drive); a TPT modal body ≤ 48 modes, `size`, model crossfades (acoustic §2.5) | GPL-3.0 | Reused (§2.11) |
| Parker, Zavalishin & Le Bivic, DAFx 2016; Bilbao, Esqueda, Parker & Välimäki, IEEE SPL 2017 [K] | First-order antiderivative antialiasing | | Every memoryless type |
| J.-M. Adrien, *The missing link: modal synthesis* (1991); K. van den Doel & D. Pai, *The sounds of physical shapes* (Presence, 1998); P. Cook, *Physically Informed Sonic Modeling* (1997) [K] | An object as a sum of damped modes; gains from the excitation and listening points; size scales frequencies | | The Box stage's model (§2.7) |
| Kinsler, Frey et al., *Fundamentals of Acoustics*; Fletcher & Rossing, *The Physics of Musical Instruments* [K] | Tube, cavity and Helmholtz modes, end corrections; ring and plate bending modes | | Every object's mode list (`proto/objects.py`) |
| Penttinen, Karjalainen, Härmä, *Morphing instrument body models*, DAFx-01 (cited by gfx-acoustic [V, abstract]) | Body models morph stably | | Object morph (§2.7) |
| Giannoulis, Massberg & Reiss, JAES 60(6) 2012 [K] | Compressor curves and detectors | | `comp` (§2.8), as gfx-comp |

### 1.5 Forum feedback: voice, live, and these plugins

- **Feedback is the live killer of vocal distortion.** VoiceTone X1 buyers: they "loved the sound but
  could never use it live because of feedback problems"; distortion settings give "many squeals in
  larger rooms past a certain threshold" ([Thomann](https://www.thomann.co.uk/tc_helicon_voicetone_x1_reviews.htm)
  [I]). Live engineers: "distortion is compression, so watch out for those together"; keep the effect out
  of the wedges; in-ears help
  ([r/sound](https://lr.ggtyler.dev/r/sound/comments/13r6rny/live_vocal_distortion/jmjdaxa/?context=3) [I],
  [r/livesound](https://redlib.astrial.org/r/livesound/comments/1ktwid5/struggling_with_feedback_using_tc_helicon/mtx2lah/?context=3)
  [I]). A Harmony Central thread asks exactly "how can I distort my voice live without having the PA
  feedback" ([harmonycentral](https://www.harmonycentral.com/forums/topic/200228-how-can-i-distort-my-voice-live-without-having-the-pa-feedback/),
  refused at survey [U]). The physics, measured: distortion lifts small signals by its drive minus its
  make-up, +8 dB at 24 dB of drive and +27 dB at 48 dB [P]. Every dB of that is loop gain toward the
  wedges. → The guard (§2.2).
- **Latency in the ears.** A singer hears their own voice through bone conduction at once, so any
  monitored delay makes a comb, and "the lowest possible latency is the only tool"
  ([ProSoundWeb](https://www.prosoundweb.com/12-a-matter-of-timing-clarifying-latency-and-putting-it-into-context/)
  [I]). Rules of thumb run from 5 ms for the whole chain (ProSoundWeb) to 15 – 20 ms before it gets
  "difficult to contend with" ([Whirlwind](https://www.whirlwindusa.com/tech-articles/opening-pandoras-box/)
  [I]). golem's own suite keeps a lane within a few ms (gfx-spectralcomp §2.9). → 0 reported, the
  dispersion measured (§2.10).
- **Sibilance turns into fizz.** iZotope's own guide: a bright vocal "can overwhelm a distortion stage";
  de-ess before the effect ([izotope.com](https://izotope.com/en/learn/the-dos-and-donts-of-de-essing.html)
  [I]). A vendor blog advises the opposite, de-ess after
  ([RysUp](https://rysupaudio.com/blogs/news/best-distortion-plugins-for-vocals-2026) [I]). Metal
  mixers de-ess hard before distortion ([Nail The Mix](https://nailthemix.com/deesser) [I]). → We
  measured both, and both fail on heavy dirt (p4 [P], §2.3).
- **Level jumps.** Trash's manual warns that types give "hugely varying output levels"; use Auto Gain
  and the Limiter [V]. The Mix reviewer turns them on whenever Heat is high [I]. → Static vocal make-up
  per type and drive (§2.5); golem's safety limiter at the end of the route.
- **Simplicity against depth.** The 2024 Trash is praised for its single page (SOS, Mix [I]) and blamed
  for losing Trash 2's modules (KVR [I]). → A four-knob face over a full expert engine (§3), as
  gfx-grit.
- **Distortion on a lead voice "can be excessive"; parallel layers are the point** (RysUp [I]);
  Decapitator's Mix "makes parallel distortion easy" [I]. → `mix` with the dry phase-aligned (§2.6).
- **What singers do instead**: a ring modulator "can add growl without creating a feedback nightmare"
  (r/sound [I]); megaphone and telephone voicings (VoiceTone X1). → Ring and Carbon types, the Cone and
  Hands objects.

### 1.6 Why a new pedal, and not gfx-grit, gfx-radio or gfx-irfx

| Pedal | Its job | Why it is not the live vocal mangler |
|---|---|---|
| **gfx-grit** | golem's `grit` block, Prisma-compatible: 4 bands, 2 slots of 25 modules in series, 25 catalogue ids, Drive / Focus | Bound to Prisma's catalogue and laws: its slots are in series and polymorphic. It has no A / B morph, no object, no guard, no de-fizz, no dynamics or delay. Its make-up tables are measured on a sine, 3 dB off on a voice [P]. It stays the multiband dirt for guitar, bass and drums |
| **gfx-radio** | A device chain: mic, channel (AM / FM / codec), amp, Klippel speaker, room | Recognisable devices, not a voice destroyer. Its Megaphone is a whole electronic megaphone. Here the Cone is the acoustic horn with no amp, and any dirt can go through it. They chain: gfx-morph for the scream, gfx-radio for "through a walkie" |
| **gfx-irfx / gfx-cab** | The user's IR, any length, golem's IR index, partitioned convolution | An IR player with no dirt. A fixed IR cannot change `size`, `decay` or `position` under EXP, and cannot morph without a kernel crossfade (30 ms, `convolve`). For one particular real object, gfx-irfx stays the tool |
| gfx-crusher / gfx-drive / gfx-fuzz / gfx-dist | Converters, pedal circuits | Single-band and guitar-voiced; no vocal guard |
| gfx-formant, gfx-golem, gfx-correct | Voice pitch and formants | They change who sings; this changes what the voice is put through. Chain them before it (a growl an octave down, then mangled) |

The integrated pedal earns its place where the stages must know about each other on a live voice. The
guard needs the dirt's small-signal gain. The de-fizz acts on the drive. The make-up must follow the
morph. The object's level must match the voice. One EXP sweep must move the dirt and the object
together. Built from four separate golem blocks, a lane gets none of this. It also gets four latencies
of dispersion and four level stages to balance by hand.

### 1.7 What we conclude

1. **The whole Trash 2 chain, at zero latency**: pre filter, multiband dirt, object, dynamics, post
   filter, delay. The 2024 redesign's users miss half of it; a vocal rig needs all of it.
2. **Minimum phase everywhere**, as Saturn shows: linear phase costs latency. Numbers in §2.10: a
   linear-phase crossover at 250 Hz is about 8.8 ms; we pay 1.8 ms of low-frequency dispersion instead,
   and only when the dirt is split.
3. **The voice's own problems are design problems**: loop gain (the guard), sibilance (de-fizz by drive
   ducking), level (vocal tables), parallel blends (aligned `mix`).
4. **Objects as physics** rather than an IR library: no third-party IP, continuous controls and morph,
   48 modes of IIR at about 72 ns per sample in f64 lanes [P].
5. **Few controls on the face, every one audible**, as gfx-grit: `drive`, `morph`, `box`, `mix` and the
   `object` selector.

## 2. Design

```
in ─▶ GUARD ─▶ PRE: HP · LP ─▶ DE-FIZZ detector ─▶ tilt ─┐ (drive duck, shelf cut)
      cap law     (min-phase)      HF share               │
                                                         ▼
      ┌─────────── up 2× / 4× (min-phase half-bands) ───────────────────────────────────────┐
      │ split: Single · Two · Three (LR4, all-pass compensated)                               │
      │   band b ─┬─▶ type A ─┐                                                               │
      │           ├─▶ type B ─┴─ morph blend (ρ-held) ─ × make-up ─▶ wet_b                    │
      │           └─▶ dry_b (aligned) ──────────── mix ─▶ × level_b ─▶ Σ                       │
      └──────────── droop correction ─────────── down ───────────────────────────────────────┘
          ─▶ de-fizz high shelf (after) ─▶ BOX: object A ⇄ B (48-mode TPT bank + skirt), `box` amount
          ─▶ COMP (one knob) ─▶ POST: HP · resonant LP (envelope) ─▶ ECHO (slap, feedback) ─▶ × level ─▶ out
   `order` Box → Dirt moves the BOX before the dirt (after the de-fizz detector).
   drive_b = drive + trim_b + env_drive · e(t) − duck(t);   morph(t) = morph + env_morph · e(t)
```

### 2.1 Order

- **Dirt → Box** (default): a distorted voice heard through an object (a scream inside a drum).
  **Box → Dirt**: the object's resonances are driven, as when you shout into a can taped to a cheap mic.
  The resonant peaks clip first, which is the megaphone growl. Trash's Swap, as one enum `order`.
- The compressor, the post filter and the echo stay after both: they shape what the audience hears.
- `order` is a set-up switch: 10 ms fade out, swap, 10 ms fade in (gfx-mbcomp's `phase` rule). The box
  rings across the swap, so its state is kept.

### 2.2 Guard: the loop-gain cap

- **Why**: at drive d, a distortion with static make-up m(d) has a small-signal gain G_ss = d + m(d):
  +1.6 / +8.0 / +18.5 / +30.2 dB at 12 / 24 / 36 / 48 dB (Soft, vocal table [P]). Stage bleed comes out
  that much louder than in bypass: measured on pink noise at −50 dBFS RMS, +1.5 / +7.9 / +18.0 /
  +27.0 dB [P]. That is the feedback the hardware pedals' users report.
- **Law** (`guard` = threshold T, dBFS RMS): a detector reads the input's RMS over 5 ms (one-pole on x²).
  Below T the pedal's input is cut by **(G_cap + 3 dB) · clamp((T − L) / 6 dB, 0, 1)**: the pedal's
  small-signal gain plus 3 dB, reached 6 dB under the threshold. Attack 1 ms, release 80 ms, both on
  the gain in dB. Hysteresis is 3 dB and acts on closing only: an open gate closes under T − 3, and a
  closed one already cuts nothing from T up.
- **G_cap is the largest gain the pedal gives a small signal, at any frequency** (a loop rings at its
  worst frequency, not on average): G_cap = max(0, G_dirt + G_post), recomputed at the control rate.
  - **G_dirt** = the largest, over the active bands, of 20 log10(1 − m + m · 10^(g_b / 20)) +
    `<b>_level`, with m the `mix` and g_b the band's small-signal gain: the blend (1 − μ) · S_A + μ · S_B
    of its two types, times the morph's ρ gain. S(type, d) is **its own build-time table**, measured
    with pink noise at −60 dBFS through the type and its make-up at each drive. It equals d + m(d) for
    Soft, Hard and Fold (+1.6 / +8.0 / +18.5 / +30.2 dB above), and it is 0 dB for the types that do not
    lift a small signal (Off, Rectify, Crush, Decimate, Sputter, Ring). It is read at the band's
    current drive, so the guard tracks `drive`, the trims, the envelope and the duck.
  - **G_post** = the sum of what the later stages add at their peak [P, review r1]:

    | Stage | Gain counted | Size |
    |---|---|---|
    | `tilt` | \|tilt\| / 2 | up to 6 dB |
    | Box | 20 log10(1 − b + b · 10^(P / 20)), b = `box`, P the object's largest peak re bypass at the current `size` and `decay` (a table beside the level table, §2.7) | at 100 %: Hands +3.9, Cone +8.1, Bucket +9.6, Cardboard Box +10.6, Oil Drum +11.7, Tin Can +12.0, Pipe +13.9, Glass Jar +14.3, Bottle +19.2 dB |
    | `reso` | the low-pass's peak, 20 log10(Q / √(1 − 1 / (4 Q²))), when the filter is engaged | +6.4 / +10.6 / +15.6 dB at 25 / 50 / 100 % |
    | `comp` | its static make-up | 0 / +2.0 / +5.2 dB at 40 / 70 / 100 % |
    | Echo | the comb's peak, 20 log10(1 + `echo_mix` / (1 − `echo_fb`)) | +1.8 dB at 20 % / 15 %; +7.0 dB at 50 % / 60 %; +20.8 dB at 100 % / 90 % |

    `level` is not counted: it is the lane's volume, and a preset that trims with it is level-matched
    by definition.
  - With `mix` under 100 % the cap follows the real gain: at 36 dB of drive the pedal lifts bleed by
    +11.1 dB at `mix` 35 % and +18.5 dB at 100 % [P]. A cut of G_ss + 3 at every mix would take 10 dB
    too much from the dry voice's tails.
- **A fixed-ratio expander does not do it** [P]: 1:4 at T = −45 dBFS still leaves +9 dB of bleed at 36 dB
  of drive, and +20 dB at 48 dB. The cap law at T = −40 dBFS gives **−2.9 to −3.1 dB at every drive**
  [P] (dirt alone, `mix` 100 %; the bench repeats it with an object, `reso`, `comp` and the echo on).
- **What the guard does not do** [P, review r1]:
  - **While the singer sings, the gate is open.** Bleed riding on a vowel at −18 dBFS RMS still gains
    +1.8 / +6.3 / +10.9 dB at 24 / 36 / 48 dB of drive (against +7.8 / +18.0 / +27.0 dB alone: the
    saturated voice compresses it), plus the object's and the filter's peaks.
  - **A howl louder than T holds the gate open**, at the full gain. The guard lowers the gain in the
    gaps, where a ring starts. It cannot stop one that has started.
  - So the guard buys back the gaps, not the monitor mix: the wedges are still set with the pedal on,
    at the loudest drive of the set, as for any distortion.
- **Rule for the threshold**: about 10 dB above the bleed at the mic. With T only 5 dB above it, the
  detector's noise re-opens the gate (cap law at −45 over −50 bleed: +2 to +15 dB [P]). The `state` meter
  shows the gate (1 = open), so the threshold is set at soundcheck by watching it stay at 0 between
  phrases.
- **What a high threshold costs** [P, review r1, a real sung take at −18 dBFS RMS]: the sung notes are
  untouched (0.0 / 0.1 / 0.4 – 0.5 % of the active time cut by more than 3 dB at T = −55 / −45 / −40;
  no energy lost), but that take's breaths and sibilants sit at −50 … −40 dBFS, so a threshold of −40
  gates them, and the gate closes and re-opens inside phrases 16 – 21 times in 27 s (6 – 9 times at
  −55). The default (−55 dBFS) keeps the consonants; it only protects when the bleed is under about
  −65 dBFS. On a loud stage the threshold must be raised at soundcheck, and consonants are the price.
- **Cost at an onset** (no lookahead) [P], a vowel starting over bleed, T −45 dBFS, drive 36 dB:
  −6.5 dB over the first 1 ms, −4.0 dB over 2 ms, −2.8 dB over 5 ms, −0.9 dB over 10 ms, −0.3 dB over
  20 ms. The onset is softened for a few ms, inside the attack of any sung or screamed note. A quiet
  vowel at −40 dBFS RMS passes untouched.
- It acts **before** the dirt (and before the box in Box → Dirt), so the box and the echo also stay
  quiet between phrases. Every gain the later stages add on silence is in G_cap.
- The detector's dB law runs at the 16-sample control rate (§2.10) with the gain ramped per sample
  between ticks: up to 0.33 ms more on the onset figures above.
- `guard` at its minimum (−90 dBFS) turns it off (bypassed, no detector).

### 2.3 Pre filter and de-fizz

- **Pre filter** (before the dirt, so it decides what distorts): `pre_hp` (12 dB / oct, 20 Hz = off),
  `pre_lp` (12 dB / oct, 20 kHz = off) and `tilt` around `tilt_freq`. The tilt is a pair of first-order
  shelves, ±`tilt`/2 dB at the extremes, 0 dB at `tilt_freq`, as Trash's Tilt and Decapitator's
  Thump / Steep do. A high-pass before the dirt is the cheapest de-mud of a screamed voice: the chest
  boom never reaches the clipper.
- **De-fizz, measured first** (p4 [P]). The test: a phrase with sibilants (4 – 10 kHz noise at
  −24 dBFS RMS) and vowels (−18 dBFS RMS) into Soft at 30 dB, 2× + ADAA. Results are the output 's' in
  1 – 4 kHz re the vowel in the same band (the hash the ear hears), and above 4.5 kHz:

  | Case | 's' 1 – 4 kHz | 's' above 4.5 kHz | Vowels touched |
  |---|---|---|---|
  | Bypass (no dirt) | −10.4 dB | +47.6 dB | — |
  | Dirt, nothing else | −5.0 | +17.5 | — |
  | De-esser **after** the dirt (18 dB) | −7.9 | +6.6 | −0.3 dB above 4.5 kHz |
  | De-esser **before** the dirt (18 dB band cut) | **−4.3** (worse) | +12.3 | 0 |
  | Before + after | −7.7 | +4.7 | −0.3 dB |
  | **Drive duck + cut after (the design), 50 %** | **−9.9** | +9.4 | +0.15 dB level, −0.2 dB above 4.5 kHz |
  | **Same, 100 %** | **−13.8** | +2.9 | +0.15 dB, −0.4 dB |

  The two design rows are p4's, with an LR4 band gain as the cut. With the high shelf this document
  specifies (below), the same metric moves by +0.6 dB at 50 % and +1.5 dB at 100 % [P, review r2b].

  A heavy clipper is a leveller: an 's' cut by 18 dB before it comes out nearly as loud and just as
  hashy. Cutting after the dirt lowers its top but leaves its 1 – 4 kHz hash. What works is to let
  the 's' pass the shaper **at low drive**, so it stays noise instead of hash, then cut its band.
- **Law**: the HF share s = P(> 4.5 kHz) / P(full), both over 5 ms RMS, read **after `pre_hp` and
  `pre_lp` and before the tilt**. It is −55 to −64 dB on the synthetic vowels and −1.7 dB on an 's' [P];
  on a real sung take its median is −35 dB, and 2 % of the active time is above −14 dB [P, review r2].
  The cut is c = −0.18 · `defizz` · w · clamp((s + 14 dB) / 8 dB, 0, 1) dB, with attack 0.5 ms, release
  30 ms (at most −18 dB at 100 %). **w is a floor on the HF band's own level**: 0 under −66 dBFS RMS, 1
  from −60 dBFS, linear in dB between.
  1. **Drive duck**: every band's drive is lowered by |c|, and its make-up follows the table at the
     lowered drive (§2.5), so the level is held while the hash goes;
  2. **Shelf cut**: the summed dirt output is shelved down by c above 4.5 kHz: a TPT SVF high shelf
     (low + √g · k · band + g · high, k = √2; gfx-dsp `svf`). It is the identity at c = 0, so it adds
     no dispersion to a vowel.
- **Why these three details** [P, review r2, r2b, r5]:
  - **Before the tilt**: a +12 dB `tilt` raises a vowel's share by 2 – 6 dB, and a breathy vowel
    (aspiration at −12 dB) then reads −10 to −12 dB: the dirt would let go on vowels.
  - **The floor w**: a preamp's hiss is white, share −1.2 dB. Without w the duck sits at its maximum in
    every silence, and each phrase starts with the drive 9 dB down at `defizz` 50 % (18 dB at 100 %),
    still −3.3 dB (−6.7 dB) 30 ms into the note. With w: 0 dB.
  - **A shelf, not an LR4 band gain**: the band gain's all-pass stays in the path at 0 dB and adds
    0.10 ms at 1 kHz to every vowel (default chain 0.189 ms instead of 0.087 ms). The shelf gives
    −8.3 / −11.3 dB of hash at 50 / 100 % against −8.9 / −12.8 dB, and lisps less (next point).
- **How much is too much** [P, review r2, r2b]: full band, the 's' sits −6.4 dB under the vowel in
  bypass and −1.5 dB under it through Soft at 30 dB. `defizz` 25 – 30 % restores the bypass ratio,
  50 % (the default) leaves the 's' 2 dB under it, and 100 % 5.9 dB under it: a soft lisp, for the
  heaviest dirt only. The law holds from 12 to 42 dB of drive (hash at 50 %: −10.2 / −10.1 / −8.9 /
  −6.6 dB at 12 / 20 / 30 / 42 dB).
- **Not for drums or guitar**: a hi-hat's share is −0.7 dB, so the duck would sit under −3 dB 94 % of
  the time on a drum loop [P]. Non-vocal presets and lanes set `defizz` 0 (§4, §6).
- Zero latency. The detector reacts in 0.5 ms; an 's' rises over 10 – 30 ms. `defizz_gr` is a meter.

### 2.4 Dirt

- **Bands** (`split`): **Single** (one band: no crossover, no dispersion; the default), **Two** (`low` |
  `body` at `x_low`) or **Three** (`low` | `body` | `air` at `x_low`, `x_air`). gfx-dsp `crossover::Tree`,
  LR4, all-pass compensated: the clean sum is the cascade of the twins, flat in magnitude (gfx-grit
  measured ±0.063 dB at 2×). In Single, the band is `body`; the `low_*` / `air_*` parameters are greyed
  (`depends`). For a voice, `low` is the chest (default 250 Hz), `body` the vowel and `air` the
  sibilance and breath (default 3.5 kHz).
- **One oversampling domain** around the split (gfx-grit §2.1, verbatim): 2× (`quality` off) or 4× (on),
  halved at ≥ 88.2 kHz, HIIR half-bands (gfx-dsp `oversample`). Every path carries one inner sample of
  ADAA alignment (HalfSample on linear paths), and the droop correction ((1 + a) / (1 + a z⁻¹))², a = 0.8,
  sits after the band sum (gfx-grit, coded).
- **Two types per band, A and B, in parallel**, fed the same band at the same drive. Each output is
  level-matched by its own table, then blended: y = (1 − μ) A + μ B, μ = the band's morph. Parallel
  rather than series (gfx-grit's slots are in series): a morph is then a blend of two finished sounds,
  continuous from one type to the other. Two memoryless shapers with the same ADAA delay have no comb
  between them. The types with memory do not share that property: Decimate (half a hold of delay),
  Fuzz (its 40 Hz inter-stage high-pass), Starve (its envelope) and Ring. Blended with another type
  they comb or beat, and that is their morph's sound; the ρ gain (§2.5) still holds the level.
- **A slot whose weight is zero is not computed**: at `morph` 0 % (and no `env_morph`) only type A
  runs, at 100 % only B, and an Off slot costs a HalfSample. A slot wakes when its weight leaves zero;
  the cheap states of the types with memory (Fuzz's high-pass, Decimate's clock, Starve's envelope,
  Ring's phase) keep running so nothing jumps. A static setting therefore costs one shaper per band
  (§2.10).
- **Types** (the enum, permanent order). The amount is the band's drive d_b in dB (0 – 48). Each type
  uses it in its own real unit:

  | # | Type | Law of d (dB) | Make-up | Notes |
  |---|---|---|---|---|
  | 0 | Off | the band, clean (HalfSample) | — | |
  | 1 | Soft | tanh(g x), g = 10^(d/20) | vocal table | ADAA (`Shaper::Tanh`) |
  | 2 | Hard | clip(g x, ±1) | vocal table | ADAA (`Shaper::Hard`) |
  | 3 | Tube | tanh(g x + 0.3) − tanh(0.3) | vocal table | even harmonics; DC block after (gfx-grit's Tube) |
  | 4 | Fuzz | gfx-grit's Fuzz (two biased clip stages, 40 Hz between) | vocal table | one inner sample, as in gfx-grit |
  | 5 | Rectify | (1 − r) x + r \|x\|, r = d / 48 | vocal table | the octave-up, "broken" voice; ADAA |
  | 6 | Fold | triangle fold of g x (period 4), **g = 10^(d / 40)** (0 – 24 dB) | vocal table | ADAA (closed-form F); the widest-band type (§2.4 aliasing) |
  | 7 | Sine Fold | sin(π/2 · g x), **g = 10^(d / 40)** (0 – 24 dB) | vocal table | ADAA |
  | 8 | Crush | bits = 16 − 14 · d / 48 (16 → 2), Tread rounding | none (unity) | gfx-grit's Bit crush engine (ADAA on the staircase) under this pedal's own law (gfx-grit's goes to 1 bit) |
  | 9 | Decimate | hold rate 20 kHz · 2^(−6 d / 48) (20 kHz → 312 Hz) | none | gfx-grit's Sample divide engine (fractional clock, area-sampled edge) under this pedal's own law (gfx-grit's: 40 kHz → 40 Hz) |
  | 10 | Sputter | dead zone T = −66 + 1.25 · d dBFS (−66 → −6), 6 dB knee | vocal table, capped | gfx-grit's Zero cross: the broken, gated voice. From about 36 dB (T = −21 dBFS) only the loudest syllables pass |
  | 11 | Ring | x · cos(2π `ring_hz` t), depth r = d / 48 blended with the band | none | the robot (`ring_hz` 30 – 100 Hz); phase 0 on reset |
  | 12 | Carbon | the carbon-button curve of gfx-radio §2.2 (i ∝ 1 / (1 − h·p) − 1, gfx-dsp `adaa::Shaper::Asym`), h·p = 0.1 · 10^(d / 40), guarded at 0.6 | vocal table | 2nd harmonic, the old-mic / phone grain |
  | 13 | Starve | Hard clip at a rail that sags with the 20 ms envelope (gfx-radio's battery sag; gfx-dsp `adaa::HardClip`), depth d / 48 | vocal table | the dying-battery sputter on loud syllables |
  | 14 | Rasp | Tube, plus seeded noise (2 – 6 kHz) × the band's 5 ms envelope × r, r = d / 48 at −12 dB re the band | vocal table | aspiration hiss that follows the voice: the false-cord scream; silence stays silent |

  Every law is continuous from d = 0, and nothing jumps when a type is chosen (10 ms type crossfade,
  gfx-grit's queue). Crush, Decimate and Ring are unity by nature (no table), as in gfx-grit. Every
  band ends in a DC block (Rectify, Tube, Carbon and Fuzz make DC), and the vocal tables are measured
  after it.
- **Drive per band**: d_b = clamp(0.48 · `drive` + `<b>_trim` + `env_drive` · e(t) − |c(t)|, 0, 48) dB,
  where e(t) is the envelope (§2.8) and c(t) the de-fizz cut.
- **Aliasing on a voice** (p2 [P]). A steady sung 'a' (f0 187.3 Hz, a band-limited glottal source
  through four formants), so every non-aliased product is a harmonic. The metric is the energy off the
  harmonics re the energy on them, below 20 kHz; the input's own floor is −94.9 dB:

  | Type, drive, input RMS | 1× | 2× | 2× + ADAA | 4× + ADAA | 32× reference |
  |---|---|---|---|---|---|
  | Soft 24 dB, −18 dBFS | −95 | −95 | −95 | −95 | −95 |
  | Soft 48 dB, −18 | −27 | −42 | **−59** | **−89** | −95 |
  | Soft 48 dB, −6 (a scream into a hot preamp) | −22 | −31 | **−49** | **−66** | −94 |
  | Hard 24 dB, −18 | −49 | −63 | **−80** | −94 | −95 |
  | Hard 48 dB, −6 | −22 | −30 | **−48** | **−64** | −77 |
  | Rectify (any drive) | −48 | −62 | **−80** | −92 | −93 |
  | Fold 24 dB, −18 | −38 | −52 | **−70** | −87 | −93 |
  | Fold 48 dB, −6 | +11 | +7 | **−12** | **−25** | −35 |

  So 2× + ADAA keeps a voice's aliasing at −48 to −95 dB except deep folding, and 4× (`quality`) gains
  10 – 30 dB more. A smooth curve on a voice is clean even at 1× up to 24 dB: a voice has little energy
  above 4 kHz to fold. The figures hold across the voice's range: at 294, 524 and 881 Hz, Soft at
  48 dB reads −46 … −59 dB at 2× + ADAA and −62 … −93 dB at 4× [P, review r4].
- **Why Fold and Sine Fold stop at 24 dB** [P, review r4]. A memoryless curve on a periodic input gives
  a periodic output, so every inharmonic dB in this table is an alias, in the 32× column too. With g up
  to 48 dB a folder reads −24 dB at 2× + ADAA on the reference vowel (−18 … −34 dB at other pitches)
  and −12 dB on a scream, and −35 … −61 dB at 32×: no oversampling a live pedal can afford makes it a
  folder, it is a noise source. The rows "Fold 48 dB" above are that rejected law. With g = 10^(d / 40)
  the type's whole range is the "Fold 24 dB" rows: −70 dB at −18 dBFS and −52 dB at −6 dBFS at 2× +
  ADAA (−68 … −70 and −50 … −54 dB at the other pitches), −87 / −70 dB at 4×.

### 2.5 Level

- **Vocal make-up tables**: for each compensated type, the RMS ratio input / output of the **reference
  vowel** (the sung 'a', −18 dBFS RMS) through the type at 2× + ADAA. They are tabulated at 1 dB steps of
  drive and interpolated, limited to −30 … +12 dB, and generated at build time (const data), as gfx-grit.
- **Why not gfx-grit's tables** (p3 [P]). On the sung phrase at −18 dBFS RMS, from drive 0 → 48 dB, the
  output re input is:
  - Soft: grit's tables −0.1 → −3.0 dB (spread 3.1 dB); vocal tables 0.0 dB everywhere;
  - Hard: grit's −3.3 dB spread; vocal tables 0.1 dB;
  - Fold: grit's 4.8 dB spread; vocal tables 0.8 dB.

  A steady vowel has a crest factor of about 8 dB [P: peak −9.7 dBFS at −18 dBFS RMS] against a sine's
  3 dB, so a sine-referenced table under-compensates it by 3 dB once it saturates.
- **On a real sung take** [P, review r3: 27 s, set to −18 dBFS RMS over its active parts; its peak is
  then +1.3 dBFS, a crest of 19 dB, and its 20 ms levels run from −26 to −19 dBFS between quartiles],
  drive 0 → 48 dB, output re input:
  - Soft: vocal table 0 … −1.8 dB (the dip is at 18 dB of drive), gfx-grit's table −0.5 … −4.5 dB;
  - Hard: vocal 0 … −2.0 dB, gfx-grit's 0 … −4.8 dB;
  - the loudest quarter of the frames comes out 4.9 dB under its input at high drive and the quietest
    quarter 11.5 dB over it: the leveller, below.

  So the vocal tables are 2.7 – 3 dB closer than the sine's on real singing, and a phrase is held within
  +0.5 / −2.5 dB, not ±0.8 dB: that figure belongs to the steady reference vowel.
- **The reference level is the lane's level**: the tables assume the voice arrives near −18 dBFS RMS on
  its sung notes. A take 10 dB under it comes out up to 9 – 10 dB over its input at high drive (and one
  10 dB over it, as much under: next point). golem's voice lanes must feed the block near that level
  (§6), and presets are level-matched on the bench's voice DI at it.
- **What a static table does not do** [P]: 10 dB louder singing (−8 dBFS RMS) comes out 9 – 10 dB under
  its input at high drive, at the same output level as the reference. Saturation is a leveller. Live,
  this is wanted: a scream does not jump out of the mix. At low drive the voice keeps its dynamics.
- **Morph level** (p6 [P]): two outputs at equal RMS blended linearly dip when they are not correlated.
  At 50 %: Soft → Hard 0.0 dB (ρ 1.00), Soft → Fold −1.8 dB (ρ 0.31), Tube → Rectify −4.3 dB (ρ −0.26).
  The blend is scaled by 1 / √((1 − μ)² + μ² + 2μ(1 − μ)ρ) (gfx-acoustic's FIR-blend rule). ρ is
  measured live: exponential averages of A·B, A² and B² over 30 ms, clamped to −0.5 … 1, the gain
  smoothed 20 ms. With ρ known exactly the blend holds within 0.00 dB at every μ [P]. With the live
  estimate and the static tables, the blend at 50 % sits within 0.10 dB of the mean of its two ends on
  the reference phrase and within 0.26 dB on a real take, for Soft → Hard, Soft → Fold, Tube → Rectify
  and Soft → Rectify [P, review r8]; the bench bound stays ±0.5 dB. It is a slow gain (30 ms), not a
  per-sample one: it cannot make distortion of its own. While only one slot runs (§2.4) the gain is 1.
- The **box** carries its own level table (§2.7), the **comp** its static make-up (§2.8). `level`
  (−24 … +12 dB) trims at the end.

### 2.6 Mix

- **`mix`** (0 – 100 %) is the dirt's wet per band, against the band's aligned dry: the dry is the band
  after the crossover, through the same HalfSample as a linear slot. It is gfx-grit's per-band blend,
  with no latency comb by construction. A parallel scream under a clean voice is the most common studio
  use (§1.5), and live it keeps the words intelligible.
- The box, comp, post filter and echo come after the mix, so they treat the blend as one voice.
- What a type does to the phase stays its sound: Ring and Rectify change the fundamental, and Decimate
  delays by half a hold. Presets that blend those state it (gfx-grit §2.3's rule).

### 2.7 Box: objects as modal banks

- **Model**: an object is up to **48 modes** (f_m, Q_m, g_m) plus a **skirt**. Each mode is a TPT SVF
  band-pass normalised to unity at its peak (k · v1), summed with gain g_m: the mode of gfx-acoustic's body engine
  (≤ 48 TPT modes, `size`, §2.5 there), written as a new gfx-dsp module `modal`, in f64 (§2.11). The skirt is the
  non-resonant transmission where modes are too dense to list: a 2nd-order high-pass and low-pass at
  the object's edges, at `skirt` dB. Output = `box` · bank + (1 − `box`) · direct, so `box` 0 % is
  bit-exact bypass (the bank is skipped).
- **Objects** (`object`, permanent order), from `proto/objects.py`. The formulas are in its docstring:
  closed-open tube with end correction, open-open tube, closed-cylinder transverse modes (zeros of J′),
  rectangular cavity, Helmholtz, thin-ring wall modes, clamped-plate lid modes. Q comes from the
  material, with a T60 cap per object: a held can does not ring for 7 s. Measured at `position` 50 %,
  `size` 100 % [P]:

  | # | Object | Physics (dimensions) | Modes | Mode T60 | IR to −60 dB | Strongest peak | Voice level re bypass, pink-normalised |
  |---|---|---|---|---|---|---|---|
  | 0 | None | — | — | — | — | — | — |
  | 1 | Tin Can | closed-open tube 11 cm × ⌀ 7.4 cm, transverse modes, steel wall rings (0.22 mm) | 28, 100 Hz – 7.9 kHz | 6 – 600 ms | 264 ms | 646 Hz | −0.1 dB |
  | 2 | Bucket | closed-open 27 cm × ⌀ 26 cm, plastic wall | 36, 89 Hz – 3.0 kHz | 8 – 400 ms | 196 ms | 245 Hz | −0.5 |
  | 3 | Pipe | open-open 60 cm × ⌀ 5 cm | 22, 272 Hz – 6.0 kHz | 6 – 364 ms | 160 ms | 5.5 kHz | −4.4 |
  | 4 | Bottle | Helmholtz (0.75 L, 9.5 mm neck) + body + glass rings | 25, 108 Hz – 6.7 kHz | 6 – 1,000 ms | 546 ms | 108 Hz | −4.6 |
  | 5 | Cardboard Box | rectangular 40 × 30 × 30 cm, lossy walls (Q 7) | 48, 572 Hz – 2.5 kHz | 6 – 27 ms | 19 ms | 1.7 kHz | −1.2 |
  | 6 | Oil Drum | closed cylinder 88 cm × ⌀ 58 cm, steel lid modes | 37, 61 Hz – 2.3 kHz | 34 – 1,500 ms | 918 ms | 781 Hz | −0.5 |
  | 7 | Glass Jar | closed-open 15 cm × ⌀ 9 cm, glass wall rings (3 mm) | 32, 484 Hz – 8.5 kHz | 7 – 1,200 ms | 298 ms | 484 Hz | −3.6 |
  | 8 | Cone | the acoustic megaphone: a truncated cone 62 cm (the complete-cone series), wide mouth (Q 14) | 27, 214 Hz – 5.8 kHz | 2 – 144 ms | 49 ms | 5.3 kHz | −2.2 |
  | 9 | Hands | cupped hands round the mic: Helmholtz 150 cm³, 2 cm² opening (Q 3) + one overtone | 2, 515 Hz, 1.3 kHz | 7 – 13 ms | 11 ms | 507 Hz | +2.1 |

  Every value is a voicing starting point [I], to be confirmed by measuring a real object (open
  question 3). The mode counts and ranges are the generator's at `position` 50 %; they change with the
  fixed-set rule below (the Cardboard Box has 92 candidate modes, and 12 of the Oil Drum's 37 are
  silent at 50 %). The last column is p5's two notes; the shipped constant is the pitch mean below.
- **A fixed mode set per object** [P, review r6]. `proto/objects.py` keeps the 48 strongest modes at the
  current `position` and drops the modes above 0.45 fs or under 30 Hz at the current `size`. Moving
  either control then changes the set: the Cardboard Box takes 11 different sets over a position sweep,
  and the Tin Can falls from 28 to 17 modes at `size` 25 %. The generator therefore:
  - chooses each object's ≤ 48 modes **once**, by the mean over `position` of g² / f, so `position`
    only moves gains;
  - **fades** a mode's gain to zero between 0.35 fs and 0.45 fs and between 40 and 30 Hz instead of
    dropping it (the coefficient is held at the limit), so `size` only moves frequencies.
- **Level** [P, review r6, r7]: each object is normalised so its pink-weighted power (100 Hz – 8 kHz)
  is 0 dB **at every setting**, then offset by one voice constant.
  - The pink power moves with all three controls: up to 3.6 dB with `size`, **+3.4 … +8.9 dB at
    `position` 0 %** and −3.0 dB elsewhere, **+3.2 … +7.6 dB at `decay` 25 %** and −2.1 … −5.4 dB at
    400 %. The three do not separate (4.4 dB of cross-term), and a closed-form sum of the modes' powers
    misses by up to 9.6 dB where modes overlap (Cardboard Box, Cone).
  - So the constant is a **build-time table per object over `position` (65 points) × `decay` (5,
    log) × `size` (9, log)**, from the numerical pink power of the generated bank, read trilinearly:
    26,325 values. Interpolation error at the mid-points: ≤ 0.9 dB over `position` (it needs the 65
    points: 1.6 dB with 33, 3.5 dB with 17), ≤ 0.1 dB over `decay`, 0.7 dB over `size` with 5 points
    (hence 9).
    The same generator writes the object's **largest peak** re bypass over `decay` × `size`, for the
    guard (§2.2).
  - The **voice constant** is one value per object: the power mean of the level of a sung vowel over
    98 – 392 Hz in semitones, 'a' and 'o' (Tin Can +1.3, Bucket −0.2, Pipe −3.1, Bottle −3.9, Cardboard
    Box −1.8, Oil Drum +3.0, Glass Jar −3.0, Cone −2.0, Hands +2.3 dB re bypass before the constant
    removes it). p5's two notes
    (last column above) were off by up to 3.5 dB from it.
  - **A single note is not held, and should not be**: its harmonics meet the modes or miss them. Over
    that range a note comes out −4.3 … +7.3 dB from the mean through the Tin Can, −4.4 … +10.5 dB
    through the Oil Drum, −7.7 … +1.9 dB through the Pipe. That is the object; `comp` after it is
    there to hold those peaks.
- **`size`** (25 – 400 %): every f_m × 100 / `size`, Q kept (a bigger object of the same material). The
  skirt corners move with it.
- **`decay`** (25 – 400 %): every Q_m × `decay` / 100 (T60 scales with it), capped so no mode exceeds 4 s.
- **`position`** (0 – 100 %): the listening point along the object's main axis. Each axial mode's gain is
  its shape there: cos((2n − 1)π x / 2) in a closed-open tube, cos(nπ x) in an open tube or a box. Moving
  it sweeps notches through the mode series, a formant-like gesture under EXP.
- **Object morph** (`object_b`, driven by `morph`): the two objects' modes are paired by rank (sorted by
  frequency), and each pair is interpolated: log f, log Q, gain linear (gains pre-scaled by each
  object's level constant). **A slot with no partner takes its partner's f and Q and only fades**: p5
  parked the silent slots at the shorter object's last frequency, so Hands → Tin Can swept 26 modes
  down from 1.3 kHz and bumped the level by +6.0 dB at 75 %; with the rule the level runs +1.0 / −0.8 /
  −1.7 / −2.4 / −3.4 dB from one object to the other [P, review r6]. The skirt's corners interpolate in
  log f and its gain linearly. This is the stable body morph of Penttinen et al. (DAFx-01).
  `object_b` = Same disables it. Mid-morph the level may leave the two ends' line by 1 – 3 dB on a held
  note (Bottle → Cone: −6.0 dB at 50 % between −4.4 and −2.2 dB): the modes move off the note's
  harmonics.
- **Motion is clean** [P]. The per-sample TPT bank stays finite and bounded while `size` sweeps
  50 → 200 % in 0.5 s, `position` 0 → 100 %, and Tin Can morphs into Bucket. The largest sample step is
  1.36 / 1.00 / 0.87 × the static renders' own. In the plugin, coefficients are recomputed every 32
  samples (48 `tan`: 16 ns per sample [P]) and ramped in between (gfx-acoustic's rule).
- **Why not convolution** (gfx-dsp `convolve`, what Trash's Convolve and gfx-irfx do):
  - the same objects as IRs need 11 – 918 ms (514 – 44,078 taps) [P]. That is affordable: gfx-cab
    measures 75.6 ns per sample for 200 ms and 0.8 % of a core for 2 s;
  - but a fixed kernel has no `size`, `decay`, `position` or morph. Each change is a new kernel with a
    30 ms crossfade (`convolve`), and an EXP sweep would re-render kernels continuously;
  - the bank costs **72 ns per sample in f64 lanes** (114 ns scalar; 32 ns in f32, not used: §2.10)
    [P] and moves every sample;
  - a user IR is gfx-irfx's job, after this pedal.
- **Why no high-band FIR** (gfx-acoustic adds one): its use is a measured body's dense top. Here the
  objects are synthetic, so their dense region is the skirt; a decaying-noise FIR (gfx-dsp `fir`, 2,048
  taps, ≈ 73 ns) is the named upgrade if the metal objects sound too "clean" (open question 4).
- **Zero latency**: the bank is IIR. Its ringing is the object's sound, not a delay.

### 2.8 Envelope, compressor, post filter

- **Envelope** e(t) ∈ [0, 1]: the input's RMS over 10 ms in dB (L), with e = clamp((L + 48) / 36, 0, 1)
  (−48 dBFS → 0, −12 dBFS → 1), attack 5 ms, release 120 ms. These are fixed times, chosen for syllables:
  a vocal follower, not a synth one. It drives:
  - `env_drive` (−24 … +24 dB): positive = louder singing distorts more (a scream that breaks up as it
    grows); negative = the dirt backs off on loud notes, so the level stays even.
  - `env_morph` (−100 … +100 %): added to `morph`, clamped 0 – 100 %. Louder = type B and object B:
    Trash's modulation node, on one axis.
  - `env_filter` (−4 … +4 oct): moves `post_lp`, an envelope "talk" filter.
- **`comp`** (0 – 100 %, 0 = off): one knob, after the box. It is gfx-comp's feed-forward dB-domain
  compressor (Giannoulis smooth decoupled detector, soft knee 6 dB). Threshold −6 − 0.24 · `comp` dBFS,
  ratio 1 + 0.07 · `comp` (1 → 8:1), attack 3 ms, release 120 ms, static make-up (half the gain
  reduction at the reference). It levels a screamed take and holds the box's resonant peaks. Trash 2's
  Dynamics module, which its users miss, and Devil-Loc's crush at the top of the range. The `gr` meter
  shows it.
- **Post filter**:
  - `post_hp` (12 dB / oct);
  - `post_lp` with `reso`: an SVF low-pass, Q 0.707 → 6 at 100 %, **no self-oscillation**. Trash's
    Scream self-oscillates, which on a live vocal mic is a feedback tone on demand: not offered. Even
    so, Q 6 is +15.6 dB at the cut-off: the guard counts it (§2.2);
  - a −12 dB / oct `post_lp` at 4 – 6 kHz is the "steep" speaker roll-off that takes the remaining fizz.

### 2.9 Echo

- One slap delay with feedback, after the post filter: `echo_time` 20 – 500 ms, `echo_fb` 0 – 90 %,
  `echo_mix` 0 – 100 % (0 = off; added to the dry, the shared `mix` law for delays). A one-pole low-pass
  at 4 kHz in the loop, gfx-dsp `delay::DelayLine` with Hermite reads, and `time` smoothed (a tape-like
  glide when moved).
- Why it is here: the megaphone in the street, the stadium shout and the robot's repeat are part of the
  sound (Trash 2 had six delay types; the 2024 users miss it, §1.1). It is kept to one tap: anything
  more is gfx-delay, after it.
- The echo is keyed by the guard: in silence it decays, it does not re-feed bleed. Its comb's peak
  (1 + `echo_mix` / (1 − `echo_fb`)) is part of the guard's cap (§2.2).

### 2.10 Latency, stereo, CPU

- **Latency: reported 0** (gfx-bench `skip_latency`, gfx-grit's precedent). The first output sample of an
  impulse is sample 0 through every part [P]. Group delay [P]:

  | Part | 150 Hz | 300 Hz | 1 kHz | 3 kHz |
  |---|---|---|---|---|
  | 2× half-band pair (8 coefficients, transition 0.04) | 3.07 samples | | 3.07 samples = **0.064 ms** | 3.13 samples |
  | 4× | | | 4.61 samples = **0.096 ms** | |
  | LR4 all-pass at 250 Hz (Two / Three) | ≈ 1.9 ms | | 0.119 ms | 0.013 ms |
  | LR4 all-pass at 3 kHz (Three) | | | 0.163 ms | 0.154 ms |
  | **Default chain** (Single, `pre_hp` 100 Hz, 2×) | 1.27 ms | 0.34 ms | **0.087 ms** | 0.068 ms |
  | Default chain, `quality` on (4×) | 1.31 ms | 0.37 ms | **0.119 ms** | 0.100 ms |
  | **Two bands** 250 Hz + `pre_hp` 100 + 2× | 3.44 ms | 1.77 ms | **0.206 ms** | 0.081 ms |
  | **Three bands** 250 / 3,500 Hz + `pre_hp` 100 + 2× | 3.57 ms | 1.90 ms | **0.342 ms** | 0.229 ms |

  The four chain rows are the document's own defaults [P, review r5]. p1's "default chain" was pre HP
  80 Hz + a 12 kHz low-pass, and its Three used 3,000 Hz. Below 300 Hz the default chain's figure is
  the 100 Hz low cut's, which is a phase **lead** (+56° at 150 Hz), not a delay. "First sample 0" is
  exact but small: y[0] is 3 · 10⁻⁴ of the impulse, and half its energy has arrived by sample 4 (2×)
  or 6 (4×). The ADAA half-sample and the droop correction add about ±0.2 sample, not counted.

- **What the singer hears** [A; phases P, review r5]: a monitored wet combs against bone conduction
  where its **phase** passes 180°. The default chain reaches it near 7.6 kHz (−15° at 1 kHz, −42° at
  2 kHz, −91° at 4 kHz), where bone conduction is weak; under 300 Hz the low cut leads by 28 – 88°,
  which thins the monitored chest a little and makes no notch. A crossover's twin turns the phase by
  180° at its frequency: Two or Three bands read −163° at 250 Hz, a notch at the chest in in-ears,
  heard as a hollow low end. **Single is the default for that reason**, and the vocal presets split
  only when the bands earn it (§4). The interface's own round trip (1 – 3 ms at best) is outside the
  pedal and usually larger than all of this.
- **The linear-phase alternatives**, in numbers [P]: a crossover at 250 Hz with LR4's selectivity needs
  about 842 taps, so 421 samples = **8.8 ms** of reported latency. Linear-phase half-bands with the IIR's
  100 dB would add about 0.84 ms (2×) or 1.27 ms (4×). The first alone is beyond what a singer tolerates
  in-ear by the strict rule (5 ms for the whole chain, §1.5). No linear-phase mode, and no exception to
  zero latency.
- **Stereo**: 1 / 1 first. 2 / 2 runs two identical chains with linked detectors (guard, de-fizz,
  envelope, comp, ρ): one voice, two outputs. Rasp's noise is the same seed on both (no fake width).
  `width` is not offered: golem's widener is a block.
- **CPU** [P: `proto/rs`, the whole chain re-typed in Rust (std only, f64, HIIR half-bands, ADAA with
  ln cosh, TPT SVFs, the 48-mode bank), 10 s at 48 kHz mono, median of 7, i9-13900H under load
  (load average 7 – 9): noisy, ±20 %]:

  | Configuration (A = Soft, B = Hard: one ADAA stage per slot) | Measured (three runs) | After the coding rules (below), estimate |
  |---|---|---|
  | Single band, A + B, 2× (dirt and detectors only) | 239 – 272 ns | ≈ 190 ns |
  | **Default-like**: Single, A + B, 2×, box 48 modes, comp, echo | **448 – 496 ns** | **≈ 350 ns** |
  | Three bands, A + B, 2×, box, comp, echo | 646 – 693 ns | ≈ 550 ns |
  | Three bands, A + B, 4×, box, comp, echo | 855 – 1,069 ns | ≈ 800 ns |
  | The same with the box's `size` moving | 866 – 1,324 ns | ≈ 850 ns |
  | **Worst** [review]: Three, 4×, a two-stage type (Fuzz) in all six slots, morph moving, box, comp, echo | **2.1 – 2.2 µs** (Three at 2×: 1.34 µs; Single at 2×: 0.64 µs) | ≈ 2.0 µs |
  | Parts: 48-mode bank f64 scalar / **f64 × 4 lanes** / f32 × 8 lanes | 114 / **72** / 32 ns | |
  | Parts: one shaper in a 2× pair (two ADAA tanh) | 59 ns | |
  | Parts: one log10 + exp per sample (a detector's dB law) | 25 ns | |

  - The coding rules:
    - the modal bank in **f64 structure-of-arrays lanes** (the states in one plain loop over the
      modes, then the sum in four accumulators): 72 ns [P]. f32 lanes would save 40 ns more, but
      gfx-dsp keeps recursive states in f64, and an f32 mode at 30 Hz, Q 160 (`decay` 400 %) at
      192 kHz is 49 dB under its own output in error against 60 – 80 dB at 48 kHz [P, review r6];
    - the detectors' **dB laws at a 16-sample control rate** (guard, de-fizz, envelope: three log /
      exp per sample cost about 75 ns);
    - **zero-weight slots are skipped** (§2.4): at a static `morph` 0 or 100 % a band runs one shaper,
      not two (about 60 ns per band at 2× for a one-stage type).
  - Not in the timing: the ρ estimator, the tilt shelves, the DC blocks, the type tables' lookups
    (a few tens of ns), and the cost of each type beyond Soft and Hard. **Fuzz is two ADAA stages per
    inner sample**: six Fuzz slots at 4× are 48 tanh-ADAA calls per sample, which is the 2.1 – 2.2 µs
    row (10 % of a core at 48 kHz, mono). The draft's "worst ≤ 1.2 µs with Fuzz / Fold in every
    slot" was not measured and does not hold.
  - Against siblings (mono, 48 kHz): gfx-grit 236 ns at its defaults (1.36 µs at its worst),
    gfx-acoustic 255 – 277 ns, gfx-radio 568 ns, gfx-spectralcomp Live ≈ 0.55 µs, the suite's norm
    1 – 1.5 µs.
  - **Bench budgets** (mono): default settings ≤ 350 ns; default-like with box, comp and echo
    ≤ 450 ns; Three bands 2×, one-stage types in A and B, ≤ 750 ns; **worst (Three, 4×, Fuzz in all
    six slots, morph and box moving, comp, echo) ≤ 2.2 µs**; stereo ≤ 1.8 × mono (linked detectors).
    The worst row is above the suite's norm: accepted here as a corner no preset uses (the heaviest
    presets run Three bands at 2× with two or three active slots), to be confirmed by the maintainer.

### 2.11 Reuse from gfx-dsp, and what is new

| Module | Use |
|---|---|
| `oversample` | The one 2× / 4× up / down pair around the dirt; `identity` not needed (every band is inside) |
| `adaa` | `Adaa1`, `Shaper::Tanh` / `Asym` / `Hard`, `HardClip` (Starve), `HalfSample`; the droop correction as gfx-grit |
| `crossover` | `Tree` (Two / Three, LR4, twins) |
| `svf`, `matched` | Pre / post filters, tilt shelves, the de-fizz high shelf, the skirt, the comp's detector filters |
| `envelope` | RMS / dB followers (guard, de-fizz, envelope, comp) |
| `smooth`, `exp_smooth` | Every float; the EXP rule on `drive`, `morph`, `box`, `mix`, `size`, `position`, `post_lp` |
| `delay` | The echo (`DelayLine`, Hermite) |
| `noise` | `White`, seeded, for Rasp |
| gfx-grit (crate) | Curves and laws of Soft, Hard, Tube, Fuzz, Rectify, Fold, Sine fold, Bit crush, Sample divide, Zero cross (`curves.rs`, plain `std`, already included by gfx-grit's `build.rs`): **moved verbatim to gfx-dsp `shapers`** in a commit of its own, before this crate is coded, with gfx-grit's renders and generated tables **byte-identical** before and after (gfx-golem → `psola`'s method). This pedal's own laws (Fold and Sine Fold to 24 dB, Crush to 2 bits, Decimate 20 kHz → 312 Hz, Sputter per dB) are functions of this crate that call those curves; gfx-grit's Amount laws are not changed |
| gfx-radio (crate) | **Nothing moves.** Its carbon mic is already gfx-dsp `adaa::Shaper::Asym` with gfx-radio's constants, and its rail is gfx-dsp `adaa::HardClip`. Carbon and Starve use those two with this crate's own constants (§2.4's formulas) |
| gfx-acoustic (crate) | **Nothing moves now.** Its body (`body.rs`) is an f64 scalar loop with a band-pass and a low-pass gain per mode and per mic, tied to its FIR, its D⁻¹ filter and a 50 – 200 % `size`; a bank "generalised to N modes and lanes" sums in another order and cannot stay bit-exact. gfx-acoustic keeps its copy until it is next touched, then migrates with byte-identical renders or stays (gfx-octaver / `erb_bank`'s and gfx-glitch / `stft`'s precedent) |
| gfx-comp | Its feed-forward dB-domain core for `comp` (already shared through `envelope`) |
| **New in gfx-dsp** | `shapers` (the move above: no new code) and `modal` (new code, written from the same TPT mode: f64 structure-of-arrays lanes, N ≤ 48 band-pass modes with one gain each, per-tick coefficient ramps, a rank morph of two mode lists, the gain fades at the band edges) |
| **New in the crate** | Guard cap law; de-fizz HF-share detector and drive duck; the A / B blend with live ρ; the 15-type enum with vocal make-up tables (build-time generator on the reference vowel, `proto/common.py`'s source); the object generator (`objects.rs`, from `proto/objects.py`) with voice-referenced level tables per object and size; Rasp; the echo wiring |

### 2.12 v1 and deferred

| v1 | Deferred (ids free, appended later) |
|---|---|
| Single / Two / Three bands; 15 types A / B per band; guard; de-fizz; pre HP / LP / tilt; 9 objects A / B with size, decay, position; order; comp; post HP / LP / reso; envelope to drive, morph and filter; slap echo; mono and stereo | A four-corner morph (Trash's XY): needs a 2-D control golem's face lacks; per-band objects (Trash runs Convolve per band); a drawn waveshaper (no ADAA); the high-band FIR for metal objects (§2.7); more objects (a helmet, a mailbox, a car, a phone booth: rooms are gfx-reverb's); LFO modulation of `morph` (golem's EXP and gfx-tremolo cover it) |

## 3. Parameters

New golem block: **type `morph`**, category **`lofi`**, golem-fx only (no catalogue ids to keep). Roles:
voice first, guitar, drums, keys. All ranges are linear. Enums only grow at the end. No default depends
on another parameter. Ids and names are join keys from the first ship (descriptor rules).

### 3.1 Front panel (5)

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `drive` | Drive | f | 0 – 100 % | 40 % | 20 ms | ✓ | Shared id: 0.48 dB per % into the dirt (0 – 48 dB), displayed "% (dB)"; every band, under its trim |
| `morph` | Morph | f | 0 – 100 % | 0 % | 20 ms | ✓ | Type A → type B in every band, and object → `object_b`, level-held (§2.5, §2.7) |
| `box` | Box | f | 0 – 100 % | 0 % | 20 ms | ✓ | Direct → inside the object; 0 = the bank is skipped |
| `mix` | Mix | f | 0 – 100 % | 100 % | 20 ms | ✓ | The dirt's wet per band against the aligned dry (§2.6) |
| `object` | Object | e | None · Tin Can · Bucket · Pipe · Bottle · Cardboard Box · Oil Drum · Glass Jar · Cone · Hands | Tin Can | 20 ms crossfade of the banks | | The object (§2.7). Tin Can so that `box` is heard at once |

### 3.2 Expert

| id | Name | Type | Range | Default | Smooth | Role |
|---|---|---|---|---|---|---|
| **Input** | | | | | | |
| `guard` | Guard | f | −90 – −20 dBFS | −55 dBFS | per block | Loop-gain cap threshold (§2.2); −90 = off. Set at soundcheck, about 10 dB over the bleed |
| `defizz` | De-fizz | f | 0 – 100 % | 50 % | 20 ms | Drive duck and shelf cut on sibilants, up to 18 dB (§2.3); 0 = off, for voice only |
| `env_drive` | Env → Drive | f | −24 – +24 dB | 0 dB | 20 ms | §2.8 |
| `env_morph` | Env → Morph | f | −100 – +100 % | 0 % | 20 ms | §2.8 |
| **Pre** | | | | | | |
| `pre_hp` | Pre Low Cut | f | 20 – 1,000 Hz | 100 Hz | 20 ms, log | Before the dirt; 20 = off |
| `pre_lp` | Pre High Cut | f | 1,000 – 20,000 Hz | 20,000 Hz | 20 ms, log | Before the dirt; 20,000 = off |
| `tilt` | Tilt | f | −12 – +12 dB | 0 dB | 20 ms | + = brighter into the dirt (more bite), − = darker (more body) |
| `tilt_freq` | Tilt Frequency | f | 200 – 5,000 Hz | 1,000 Hz | 20 ms, log | Pivot of `tilt` |
| **Dirt** | | | | | | |
| `split` | Bands | e | Single · Two · Three | Single | 30 ms pre-roll, 10 ms crossfade | §2.4; set-up |
| `x_low` | Low Crossover | f | 60 – 1,000 Hz | 250 Hz | 20 ms, log | `low` \| `body` |
| `x_air` | Air Crossover | f | 1,000 – 10,000 Hz | 3,500 Hz | 20 ms, log | `body` \| `air`; held ≥ 2 × `x_low` |
| `low_a`, `body_a`, `air_a` | Low / Body / Air Type A | e | Off · Soft · Hard · Tube · Fuzz · Rectify · Fold · Sine Fold · Crush · Decimate · Sputter · Ring · Carbon · Starve · Rasp | Off / Soft / Off | 10 ms crossfade | §2.4 |
| `low_b`, `body_b`, `air_b` | Low / Body / Air Type B | e | same list | Off / Hard / Off | 10 ms crossfade | The `morph` target |
| `low_trim`, `body_trim`, `air_trim` | … Drive Trim | f | −24 – +24 dB | 0 dB | 20 ms | Added to the band's drive |
| `low_level`, `body_level`, `air_level` | … Level | f | −60 – +12 dB | 0 dB | 20 ms | Whole band after the mix; −60 = mute |
| `ring_hz` | Ring Frequency | f | 20 – 2,000 Hz | 60 Hz | 20 ms, log | Carrier of the Ring type |
| `quality` | Quality | b | off / on | off | 30 ms pre-roll, 10 ms crossfade | 2× / 4×, both latency 0 |
| **Box** | | | | | | |
| `object_b` | Object B | e | Same · None · Tin Can · Bucket · Pipe · Bottle · Cardboard Box · Oil Drum · Glass Jar · Cone · Hands | Same | 20 ms | The `morph` target; Same = no object morph |
| `size` | Size | f | 25 – 400 % | 100 % | 20 ms + 32-sample coefficient ramp | ✓ (EXP) Modes × 100 / size |
| `decay` | Decay | f | 25 – 400 % | 100 % | 20 ms | T60 × decay / 100 |
| `position` | Position | f | 0 – 100 % | 50 % | 20 ms | ✓ (EXP) Listening point along the object |
| `order` | Order | e | Dirt → Box · Box → Dirt | Dirt → Box | 10 ms out / in | §2.1 |
| **Dynamics and post** | | | | | | |
| `comp` | Compress | f | 0 – 100 % | 0 % | 20 ms | One knob (§2.8) |
| `post_hp` | Post Low Cut | f | 20 – 1,000 Hz | 20 Hz | 20 ms, log | 20 = off |
| `post_lp` | Post High Cut | f | 1,000 – 20,000 Hz | 20,000 Hz | 20 ms, log | ✓ (EXP); 20,000 = off unless `reso` > 0 |
| `reso` | Resonance | f | 0 – 100 % | 0 % | 20 ms | Q 0.707 → 6 at `post_lp`; no self-oscillation |
| `env_filter` | Env → Filter | f | −4 – +4 oct | 0 oct | 20 ms | §2.8 |
| **Echo** | | | | | | |
| `echo_time` | Echo Time | f | 20 – 500 ms | 110 ms | 50 ms glide | Slap |
| `echo_fb` | Echo Feedback | f | 0 – 90 % | 30 % | 20 ms | |
| `echo_mix` | Echo Mix | f | 0 – 100 % | 0 % | 20 ms | 0 = off |
| **Output** | | | | | | |
| `level` | Level | f | −24 – +12 dB | 0 dB | 20 ms | ✓ |
| `bypass` | Bypass | b | off / on | off | 10 ms | nih-plug `make_bypass`, with the box and echo tails |

**Meters** (read-only): `gr` (comp, dB), `state` (guard: 1 = open), `defizz_gr` (dB). **45 parameters
+ 3 meters.** Every parameter is audible in its context. The `depends` greying lists what is not, or
what repeats another control:
`low_*` and `x_low` in Single, `air_*` and `x_air` in Single and Two, `body_trim` in Single (there it
is `drive` again), `<b>_b` at `morph` 0 % with `env_morph` 0, `tilt_freq` at `tilt` 0, `ring_hz` without
a Ring type, `size` / `decay` / `position` / `object_b` with `object` None or `box` 0 %, `reso` without
`post_lp` or `env_filter`, `echo_time` / `echo_fb` at `echo_mix` 0. `body_level` stays live in Single:
it sets the level into the box and the comp, which `level` does not.

Groups (`visuals/data/groups.json`): **Input** (`guard`, `defizz`, `env_drive`, `env_morph`), **Pre**
(`pre_hp`, `pre_lp`, `tilt`, `tilt_freq`), **Bands** (`split`, `x_low`, `x_air`, `ring_hz`,
`quality`), **Low** / **Body** / **Air** (`<b>_a`, `<b>_b`, `<b>_trim`, `<b>_level`), **Box**
(`object_b`, `size`, `decay`, `position`, `order`), **Post** (`comp`, `post_hp`, `post_lp`, `reso`,
`env_filter`), **Echo** (`echo_time`, `echo_fb`, `echo_mix`), **Output** (`level`).

### 3.3 Why these choices

- **`drive` in % (shared id)**, as the contract's table says; 48 dB at 100 % as gfx-grit's saturators.
- **Defaults are safe on a live voice**: Single band (no dispersion), Soft → Hard, 19 dB of drive, the
  guard at −55 dBFS, de-fizz 50 %, box and echo off. Inserted on a voice lane, it is an edgy distortion
  that does not fizz and adds 5 dB of gain to small signals at most. The guard's default removes that
  gain only under −55 dBFS: on a loud stage it is raised at soundcheck (§2.2).
- **Bands named for a voice** (`low`, `body`, `air`), not lo / mid / hi: the presets read as what they do
  (clean chest, dirty body). On guitar the same names hold well enough.
- **No `style` per type** (Trash's) and **no four-corner pad**: a greyed knob is an inaudible parameter,
  and a pedal has one EXP axis.

## 4. Applications

Reference settings become block presets (`presets/gfx-morph.json`, presets/README format; ids
permanent, no artist or trademark name, in the ids or in the texts). Unlisted values are the defaults;
every preset is level-matched to the bypass with `level` (±2 LU on the bench's voice DI at −18 dBFS RMS,
§2.5). Voice presets with an object at `box` ≥ 70 % raise the guard's cap by 8 – 19 dB (§2.2): under the
threshold they gate harder, by design. **Placement on a voice lane**: after gfx-gate
and gfx-correct / gfx-formant if used, before gfx-delay and gfx-reverb. On guitar: after the drive and
before the amp, or after the cab for the "through an object" sound. On drums: on the bus or one mic.

| Preset id | inst / gain / use | Settings to start from | What it is for |
|---|---|---|---|
| `scream-edge` (default for voice) | voice / any / lead | drive 45, body Tube → Fuzz, morph 0, defizz 70, guard −55, comp 40, pre_hp 150 | A screamed lead that stays intelligible: chest boom out before the dirt, sibilants clean, level even |
| `scream-swell` | voice / any / lead | drive 30, body Soft → Fuzz, env_drive +18, env_morph +60, comp 30 | The louder the scream, the more it breaks: dynamics drive the dirt |
| `growl-split` | voice / high / lead | split Three, x_low 220, x_air 3500, low Off, body Tube → Rasp, morph 50, air Off, air_level −6, drive 55, defizz 60 | Metal growl: chest clean and solid, a rasped body, no fizz on top (the clean-lows split of bass rigs, for voice) |
| `parallel-grit` | voice / any / rhythm | drive 60, body Hard → Fuzz, mix 35, defizz 60 | A distorted layer under the clean voice, phase-aligned |
| `acoustic-megaphone` | voice / any / texture | object Cone, box 85, body Hard, drive 35, pre_hp 400, post_lp 5000, echo_mix 20, echo_time 140, echo_fb 15 | The shout through a cone, a slap off the street; gfx-radio's Megaphone is the electronic one |
| `shout-in-a-can` | voice / any / texture | order Box → Dirt, object Tin Can, box 100, body Hard, drive 40, size 100 | Shouting into a can taped to a cheap mic: the can's peaks clip first |
| `string-phone` | voice / clean / texture | object Tin Can, box 100, drive 15, body Soft, position 30, post_hp 300 | The child's can telephone: thin, ringing, barely dirty |
| `bucket-head` | voice / any / texture | object Bucket, box 70, drive 25, body Soft | Singing with a bucket on the head: boxy low resonance |
| `bottle-ghost` | voice / clean / ambient | object Bottle, box 100, decay 150, drive 5, mix 50, echo_mix 25, echo_time 280, echo_fb 40 | Hollow, haunted whisper inside a bottle |
| `oil-drum-boom` | voice / any / texture | object Oil Drum, box 75, body Tube, drive 35, comp 30 | A voice from inside a steel drum: long metallic boom |
| `glass-jar-sweep` | voice / any / texture | object Glass Jar, box 90, drive 20, position EXP 0 → 100 % | EXP moves the ear inside the jar: a glassy formant sweep |
| `cupped-hands-blues` | voice / crunch / lead | object Hands, box 100, body Carbon, drive 45, pre_hp 200, post_lp 4500 | The cupped bullet-mic voice of blues harp players, on a singer |
| `robot-ring` | voice / any / texture | body Ring → Decimate, ring_hz 55, drive 70, morph 30, object Pipe, box 40 | The robot: ring-modulated voice, a sampled edge as morph rises (Decimate's half-hold delay combs against the ring: part of the sound, §2.4) |
| `broken-transmission` | voice / any / texture | body Sputter → Starve, drive 50, env_morph +70, object Cardboard Box, box 40, post_hp 400, post_lp 3500 | A voice that drops out and sputters on loud syllables |
| `object-morph-exp` | voice / any / texture | body Soft → Fold, object Tin Can, object_b Oil Drum, box 80, morph EXP 0 → 100 % | One pedal sweep turns a small can into a drum and soft grit into folding (Fold's own law stops at 24 dB, §2.4) |
| `drum-bus-trash` | drums / any / texture | split Three, low Soft, body Crush → Decimate, air Hard −6 dB, drive 55, object Cardboard Box, box 35, comp 70, mix 60, guard −90, defizz 0 | Parallel crushed drums in a box, kick body kept |
| `can-lead-guitar` | guitar / crunch / solo | split Two, x_low 180, low Off, body Fuzz → Sine Fold, drive 50, object Tin Can, box 50, guard −90, defizz 0 | A fuzz solo through a can, low strings clean |

What it is not: a pitch or formant effect (gfx-formant, gfx-golem), a device simulator (gfx-radio), a
user IR player (gfx-irfx), a spectral wrecker with latency (gfx-glitch), or a 25-module multiband box
(gfx-grit).

## 5. Bench suite (on top of gfx-bench's common checks)

**gfx-bench options**: `skip_latency` (minimum-phase group delay and crossover dispersion; reported 0);
`click_bound_db` −30 dB (type changes at high drive make real HF content, gfx-grit's bound); silence floor
−140 dBFS (the guard and the always-on DC blocks make silence decay to 0; Rasp's noise follows the
envelope).

| Check | Target | Prototype |
|---|---|---|
| Latency | first output sample of an impulse = sample 0, every configuration and rate; group delay at 1 kHz, at the defaults (`pre_hp` 100 Hz, `defizz` 50 %): ≤ 0.10 ms (Single, 2×), ≤ 0.13 ms (Single, 4×), ≤ 0.22 ms (Two), ≤ 0.36 ms (Three, 250 / 3,500 Hz); the de-fizz shelf at rest nulls against its input (≤ −120 dB) | 0; 0.087 / 0.119 / 0.206 / 0.342 ms (review r5) |
| Clean skeleton | every type Off, Single / Two / Three, 2× / 4×: magnitude flat ±0.1 dB 20 Hz – 20 kHz; null ≤ −60 dB against the twins' cascade (gfx-grit's row) | gfx-grit measured ±0.063 dB |
| Aliasing on voice | the reference 'a' at 187.3, 293.7, 523.9 and 881.3 Hz, −18 / −6 dBFS RMS: inharmonic re harmonic ≤ the 2× + ADAA figures + 4 dB (Soft at `drive` 100 %: −53 / −42; Hard at −6: −41; Fold at `drive` 100 % = 24 dB: −64 / −46); 4× at least 10 dB better on Soft / Hard. The types the prototype does not run (Sine Fold, Fuzz, Crush, Decimate, Sputter, Carbon, Starve, Rasp): measured and bounded at coding, ≤ −45 dB at `drive` 100 % on the −18 dBFS vowel, or the type's law is shortened as Fold's was | §2.4 table; review r4 |
| Level, vocal tables | each compensated type, drive 0 → 100 %: the reference phrase (p3) at −18 dBFS RMS within ±1 dB of the input; the bench's real voice DI at −18 dBFS RMS (active parts) within +0.5 / −2.5 dB | spread 0.0 – 0.8 dB; real take 0 … −2.0 dB (review r3) |
| Morph level | every pair of memoryless types at 25 / 50 / 75 %: within ±0.5 dB of the line between the ends (live ρ), on the reference phrase and on the voice DI; pairs with Decimate, Ring, Fuzz or Starve: within ±1.5 dB | 0.00 dB with exact ρ; ≤ 0.26 dB live (review r8) |
| Guard | pink bleed at −50 dBFS RMS, guard −40: output ≤ bypass − 2 dB at drive 0 → 100 %, at `mix` 35 and 100 %, every type; **the same in third-octave bands (no band above bypass)** with each object at `box` 100 %, with `reso` 100 %, `comp` 100 %, the echo at 50 % / 60 %, `tilt` ±12, alone and together; S(type, d) within 1 dB of the measured small-signal gain of each type; vowel onset loss ≤ 3 dB over 5 ms, ≤ 1 dB over 10 ms; a −40 dBFS vowel untouched (±0.1 dB) | dirt alone: −2.9 to −3.1 dB; −2.8 / −0.9 dB; 0.0 dB. Post stages: +3.9 … +20.8 dB uncovered in the draft (review r1) |
| De-fizz | the p4 phrase at drive 30 dB Soft, r2b's metric: 's' in 1 – 4 kHz re the vowel in 1 – 4 kHz ≤ −7.5 dB at 50 %, ≤ −10.5 dB at 100 % (−4.0 at 0 %); vowels within ±0.3 dB; white hiss at −70 dBFS then a vowel: no duck at the onset (≤ 0.5 dB); `tilt` +12 on a breathy vowel: duck ≤ 1 dB; a hi-hat loop at `defizz` 0: `defizz_gr` 0 | −8.3 / −11.3 with the shelf; +0.15 dB (review r2, r2b) |
| Objects | each object's mode list equals the generator's (frequencies ±0.5 %) and does not change with `position` or `size`; pink power within ±1 dB of 0 dB at `box` 100 % on a grid of `position` × `decay` × `size` off the table's points, every object; the voice level (power mean over 98 – 392 Hz, 'a' and 'o') within ±1 dB of bypass at the defaults; single notes reported, not bounded; `box` 0 = bit-exact bypass; f64 bank at 44.1 – 192 kHz | p5 tables; review r6, r7 |
| Box motion | `size` 25 → 400 % in 0.5 s (through the edge fades), `position` 0 → 100 % on every object (the Cardboard Box included), morph between every object pair (Hands ↔ Tin Can: no level bump over +1 dB above the louder end): finite, largest sample step ≤ 1.5 × the static renders'; EXP-stepped 50 Hz sweeps: control-rate sidebands ≤ −80 dB (contract) | 1.36 / 1.00 / 0.87 × (Tin Can, 50 → 200 % only) |
| Comp | 1 kHz at −6 dBFS, `comp` 100 %: GR within 1 dB of the law; `gr` meter matches | to bench |
| Echo | time ±1 sample; feedback 90 %: bounded (loop gain < 1 with the low-pass); guard closed: no re-feed of bleed | to bench |
| Switches | `split`, `quality`, `order`, `object`, type changes under a held sung note: no burst > −30 dB re peak | to bench |
| Stereo | 2 / 2 with equal inputs: L = R bit-exact; linked detectors | — |
| Determinism | Rasp noise, Ring phase, Decimate clock from fixed states: two renders null | — |
| CPU (48 kHz, mono, blocks of 128) | defaults ≤ 350 ns; with box, comp, echo ≤ 450 ns; Three 2×, one-stage types ≤ 750 ns; worst (Three, 4×, Fuzz in six slots, morph and box moving) ≤ 2.2 µs; stereo ≤ 1.8 × mono | 239 – 272 / 448 – 496 / 646 – 693 ns / 2.1 – 2.2 µs before the coding rules (§2.10) |
| Listening | Raph's sung phrase (gfx-golem's), a scream take, a spoken phrase, guitar DIs, a drum loop, through every preset; in-ears with the default and with Three bands (the low-frequency dispersion, §2.10); a stage test of the guard against a wedge | Raph |

## 6. What golem-rack must provide

1. **A new block type `morph`**, category `lofi`, golem-fx only, roles voice, guitar, drums, keys. golem's
   voice lane takes `lofi` as its only saturation (gfx-radio §3), so this is the voice's distortion
   pedal beside gfx-radio. For "scream / distorted vocal / megaphone / robot / in a can" intents, golem's
   AI vocabulary should prefer `morph`; for "phone / radio / walkie", `radio`.
2. **Descriptor**: the 45 parameters and 3 meters of §3, the `depends` list of §3.2, the groups, enums by
   rank, the face below. EXP targets: `drive`, `morph`, `box`, `mix`, `size`, `position`, `post_lp`,
   `level`.
3. **Insert pins** for a voice lane: none needed (the defaults are the voice's). For a guitar or drum lane:
   `guard` −90 (off), since a guitar DI has no stage bleed through this block, and `defizz` 0 (cymbals
   and pick noise read as sibilants, §2.3). **Level**: the block expects the sung notes near −18 dBFS RMS
   (§2.5); golem states the voice lane's nominal level, or trims before the block.
4. **Meters**: `state` as a lamp on the block (guard open), `gr` and `defizz_gr` as bars. They are how
   `guard` is set at soundcheck: golem's soundcheck flow should ask for it on a voice lane that carries
   this block (silence on stage, raise `guard` until the lamp stays off, then about 5 dB more).
5. **Presets**: the 17 ids of §4 seeded as factory presets (`library/block-presets/morph/`), default on
   `scream-edge` for voice.
6. **Latency**: 0 in every setting; no re-sync on `split` / `quality` / `order` changes (pre-rolled engine
   switches inside the plugin).
7. **The safety limiter**: golem's gfx-limiter stays last on the route. Trash's own limiter is not
   duplicated.

## Visual

- **Category `lofi`**, **Thrash** finish (`FAMILY_FINISH.lofi`): gloss lacquer, diamond plate, the motif
  yellow lined in black.
- **Crate hue: deep crimson `#9a0d2a`** (`METAL_CRATE_HUE['gfx-morph']`): a raw throat, a shouted red, the
  darker sister of the lofi family's raspberry. gfx-correct is the same kind of sister to pitch's magenta.
  - ΔE*ab [P: `proto/hue.py`, CIELAB D65, ΔE76, against every metal and crate hue]: lofi raspberry 29,
    gfx-acoustic rosewood 34, gfx-correct plum 35; everything else is further. It sits in the class
    gfx-radio accepted (33) and gfx-tuner's review asked for. It is the best of 15 candidates; the
    closer ones were rejected: oxblood (rosewood 22), rust (gfx-fuzz 18), blood orange (drive 14),
    gunmetal (dynamics slate 19), acid chartreuse (gfx-glitch 22).
  - Why a crate hue and not raspberry: lofi already has two raspberry thrash pedals (gfx-crusher,
    gfx-grit); a third would be told apart by motif alone (gfx-radio's reason for olive).
  - **`RUNE_LIGHT['gfx-morph'] = '#ff6a4d'`** (an ember): the default `mix(hue, '#fff', .3)` gives a
    dull rose.
- **Motif `canned`** (new case in `motifParts`, drawn with `line()` and `shape()` so both finishes work),
  in the rune zone under the knob grid:
  - left of the rune: a smooth sine (3 periods, amplitude 0.25 ch) from x = 6 to the cartouche's left
    edge − 2: the voice going in;
  - right of the rune: the **outline of a can**, a rounded rectangle of 0.9 ch × 1.3 ch with an ellipse
    rim on its open left side, at the cartouche's right edge + 3. Out of it, to x = W − 6, the same wave
    **squared off and broken**: three flat-topped pulses, then a gap, then a stair step, at the same
    amplitude. The voice comes out clipped, broken and sampled;
  - thrash: yellow strokes lined in black; anodized (if forced): engraved.
  - It is told apart from `prism` (gfx-grit: one beam fanned into four dirty rays), `grille` (gfx-radio:
    speaker slots and a dial) and `impulse` (gfx-irfx).
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-morph": {"name": "Morph", "blocks": ["morph"], "knobs": ["drive", "morph", "box", "mix"],
                "selectors": ["object"], "category": "lofi", "color": "#9a0d2a"}
  ```
  Four knobs, as the largest faces. `guard` is expert: its default is right for a vocal mic, and it is
  set once at soundcheck with the `state` lamp.
- **Glyph** (`visuals/glyphs/morph.svg`, new block type): an open can with a jagged wave leaving it,
  distinct from `radio` (a bullhorn) and `grit` (bands of zigzags):

  ```svg
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>Morph (gfx)</title><path d='M3 8.5h8v9.5a1.5 1.5 0 0 1-1.5 1.5h-5A1.5 1.5 0 0 1 3 18z'/><ellipse cx='7' cy='8.5' rx='4' ry='1.5'/><path d='M13 12.5h2v-3h2.5v6H20v-3h1.5' /></svg>
  ```
  To check at 16 px on the family sheet.
- No maker's livery, logo or trademark; object names are generic.

## Open questions — answered at review

Each question is the draft's; the decision is the review's (details in the Review section).

1. **Modal objects instead of convolution** (§2.7). The bank is continuous, morphs, and costs 72 ns in
   f64 lanes [P]; IRs would be measured objects. Should v1 also ship a few IRs that the owner measures
   himself?
   → **Modal only in v1, no IR shipped.** A second engine would give objects that ignore `size`, `decay`,
   `position` and `morph`, which breaks "every parameter audible" for those objects. The owner's
   measurements serve question 3 instead (they fit the generator), and a real object's IR is played by
   gfx-irfx after this pedal.
2. **The de-fizz law** (§2.3): drive ducking plus a cut. Is ducking the drive on an 's' heard as the
   dirt "letting go"?
   → **Kept, with three corrections** (detector before the tilt, an HF-level floor, a shelf instead of
   the LR4 band gain). On a real take the duck is under −3 dB for 1.4 % of the loud frames at 50 %, and
   the sibilant frames lose 5.7 dB of hash [P]. Default 50 %. The release (30 ms) and whether 50 % is
   one notch too much (the 's' ends 2 dB under its bypass ratio) are set by ear.
3. **Object voicings [I]**: the dimensions, Q values and T60 caps are textbook physics and guesses.
   → **Method accepted** (the owner sweeps his own can, bucket, jar, bottle and pipe; the generator's
   constants are fitted). First thing to check on the real can: its lowest wall mode. The free-ring
   formula puts it at 100 Hz; a can with a bottom is much stiffer, so that mode is probably wrong by
   an octave or more.
4. **High-band FIR for the metal and glass objects**: not in v1.
   → **Not in v1.** Decided after question 3's measurements, by ear.
5. **The guard's default** (−55 dBFS) and its 6 dB knee.
   → **−55 dBFS and 6 dB kept.** It is the value that leaves a real take's breaths and consonants alone
   (they sit at −50 … −40 dBFS for sung notes at −18). It protects only when the bleed is under about
   −65 dBFS, so the documents no longer say the default "does not feed back": the threshold is a
   soundcheck setting (§6 item 4).
6. **Single band by default for voice.**
   → **Accepted.** Two or Three bands turn the phase by 163° at 250 Hz; presets split only where the
   bands earn it (`growl-split`).
7. **`morph` moves both the dirt and the object.**
   → **One knob.** `object_b` = Same gives the dirt-only morph, and equal A / B types give the
   object-only one.
8. **The echo inside the pedal** (one slap tap).
   → **Kept**, three parameters, one tap. Its comb peak is now in the guard's cap.
9. **No self-oscillating Scream** on `reso` (Q ≤ 6).
   → **Accepted.** Q 6 is already +15.6 dB of loop gain at the cut-off; the guard counts it.
10. **Moves into gfx-dsp.** Who does it, and in which order?
    → (1) gfx-grit's `curves.rs` → gfx-dsp `shapers`, verbatim, its own commit, gfx-grit's renders and
    tables byte-identical; (2) `modal` written new in gfx-dsp (f64 lanes); (3) this crate. gfx-radio:
    nothing to move. gfx-acoustic: not touched now. All by the agent that codes this crate (§2.11).
11. **Hue**: deep crimson `#9a0d2a` thrash, ember rune light, against raspberry (ΔE 29) at tile size.
    → ΔE figures confirmed (29 / 34 / 35, `proto/hue.py` re-run). **Raph, on the family sheet.**

## Review (Fable 5.2, 2026-11-15)

> **How this section relates to the body.** Every change below, blocking (B) and recommended (R), has
> been applied in the body by the reviewer on 2026-11-15: the guard's cap (§2.2), the de-fizz detector,
> floor and shelf (§2.3), Fold's law, the skipped slots and the morph's limits (§2.4), the level on a
> real take (§2.5), the object's fixed mode set, level table and morph (§2.7), the latency table and the
> CPU budgets (§2.10), the gfx-dsp moves (§2.11), `depends` (§3.2), the presets (§4), the bench rows (§5),
> the pins (§6) and the answers to the open questions. **Where the body and this section differ, this
> section's decisions override the body.** The prototypes `p1` – `p6` still implement the draft (LR4
> de-fizz cut, the dirt-only cap, p5's mode selection); they re-run byte-identical to the draft's
> `results.txt`. The reviewer's scripts are in `proto/review/`; run them from `proto/` with
> `PYTHONPATH=.:review python3 review/<script>.py`. Their output is appended to `proto/results.txt`.

**Verdict: Good**, Strong once the guard's cap is complete and the bench confirms it on a stage. The
chain is right for a live voice, and the three ideas that carry the pedal survive my numbers: a cap on
the small-signal gain instead of a ratio gate, a drive duck instead of a de-esser, and make-up tables
measured on a voice. The live ρ gain, the aliasing figures and the zero-latency construction also hold.
Nothing forces a different engine. Seven statements did not survive:

- "Below its threshold the pedal is quieter than bypass": true for the dirt alone at `mix` 100 %. The
  object, the resonant filter, the comp's make-up and the echo add +3.9 to +20.8 dB at their peaks,
  after the guard and outside its cap.
- "It does not feed back": the gate is open while the singer sings, and a howl holds it open.
- "0.097 ms at 1 kHz for the default chain": 0.189 ms at the document's own defaults, because the
  de-fizz band gain keeps an all-pass in the path.
- "The level stays within ±0.8 dB": on the steady reference vowel. A real phrase is held within 2 dB.
- "One level constant per object and size": `position` and `decay` move the level by up to 8.9 and
  7.6 dB.
- "Worst case ≤ 1.2 µs with Fuzz in every slot": 2.1 – 2.2 µs in the prototype's own code.
- "`modal` is gfx-acoustic's body, moved, in f32 lanes": that move cannot keep gfx-acoustic bit-exact,
  and f32 states are against gfx-dsp's own rule.

**What I checked.** Scripts in `proto/review/`: `r1_guard.py`, `r2_defizz.py` (+ `r2_defizz_lib.py`),
`r2b_shelf.py`, `r3_level_real.py`, `r4_alias.py`, `r5_latency.py`, `r6_box.py`, `r7_boxlevel.py`,
`r8_morph_live.py`, helpers `rvoice.py` and `p5_box_lib.py`; one line added to `proto/rs` (the bank in
f64 lanes). Nothing outside `gfx-morph/` was modified.

- **Prototypes re-run**: `p1` – `p6` and `hue.py` byte-identical to `results.txt`; the Rust timing three
  times (239 – 272 / 448 – 496 / 646 – 693 / 855 – 1,069 / 866 – 1,324 ns under load 3 – 6).
- **Sources read**: gfx-grit `curves.rs` (laws, the 1 kHz reference at −18 dBFS peak, 20 harmonics),
  gfx-acoustic `body.rs`, gfx-radio `chain.rs`, gfx-dsp's module list and numerics rule, the contract.
  The gfx-dsp names the design cites exist (`Adaa1`, `Shaper`, `HardClip`, `HalfSample`,
  `crossover::Tree`, `delay::DelayLine`, `noise::White`), and the siblings' CPU figures are as quoted.
- **New measurements**: the largest small-signal gain of each stage after the dirt; the cap against
  `mix`; bleed under a sung vowel; the cap, the HF share, the de-fizz, the tables and the live ρ on a
  **real sung take** (27 s, the maintainer's phrase, set to −18 dBFS RMS over its active parts);
  de-fizz on hiss, a hi-hat loop, a breathy vowel and a tilted one; a shelf against the LR4 band gain;
  aliasing at four pitches; group delay and phase at the document's defaults; the box's mode sets,
  its level against pitch, `position`, `decay` and `size`, three ways to hold it, the morph's padding,
  an f32 mode at 192 kHz; a two-stage type in every slot.

**Limits of my measurements.** Everything is the Python prototype at 48 kHz, f64. Only five of the
fourteen types exist in it (Soft, Hard, Tube, Fold, Rectify): the aliasing, level and small-signal
gain of Sine Fold, Fuzz, Crush, Decimate, Sputter, Carbon, Starve and Rasp are untested and go to the
bench. The real take is one singer, one microphone, dark (median HF share −35 dB) and already quiet
in its sibilants; no scream and no stage bleed were available. The guard was never run in a loop: its
behaviour against a real wedge is a stage test. The cap's new terms (G_post, the per-type tables) are
specified and bench-bounded, not prototyped end to end. The "two-stage type" timing uses two tanh
stages as a stand-in for Fuzz.

### Changes made, and why

B = blocking: apply before coding. R = recommended.

| # | | Change | Why |
|---|---|---|---|
| 1 | B | **The guard caps the whole pedal at its worst frequency** (§2.2). The cut is (G_cap + 3 dB) · clamp((T − L) / 6), G_cap = max(0, G_dirt + G_post). G_dirt is the largest over the bands of 20 log10(1 − m + m · 10^(g_b / 20)) + `<b>_level`, with g_b the morph blend of **per-type small-signal tables S(type, d)**, measured at build time (0 dB for Off, Rectify, Crush, Decimate, Sputter, Ring). G_post adds \|tilt\| / 2, the object's largest peak scaled by `box`, the low-pass's resonant peak, the comp's static make-up and the echo's comb peak. Claim 1 and §3.3 no longer say the pedal "does not feed back"; §2.2 says what the guard does not do | `r1`. The draft's cap is the dirt's d + m(d). After it, at 100 %: Hands +3.9, Cone +8.1, Bucket +9.6, Cardboard Box +10.6, Oil Drum +11.7, Tin Can +12.0, Pipe +13.9, Glass Jar +14.3, **Bottle +19.2 dB** (at 108 Hz); `reso` 100 % +15.6 dB; `comp` 100 % +5.2 dB; echo 50 % / 60 % +7.0 dB, 100 % / 90 % +20.8 dB. A loop rings where the gain is largest, so `bottle-ghost` in a gap was 16 dB over bypass at 108 Hz with the guard closed. With `mix` 35 % the real gain at 36 dB of drive is +11.1 dB, and the draft cut 21.5 dB: 10 dB too much off the dry tails. d + m(d) is wrong for the types that are not a gain at small level (a dead zone, a quantiser, a biased two-stage fuzz). Gate open: bleed under a vowel still gains +1.8 / +6.3 / +10.9 dB at 24 / 36 / 48 dB |
| 2 | B | **Box level: a table over `position` × `decay` × `size`, and a pitch-mean voice constant** (§2.7). 65 × 5 × 9 points per object from the numerical pink power, trilinear; the same generator writes the peak table B1 needs. The voice constant is the power mean over 98 – 392 Hz, 'a' and 'o'. Single notes are reported, not bounded | `r6`, `r7`. With one constant per object and size, the pink power moves by +3.4 … +8.9 dB at `position` 0 % (−3.0 dB at 75 %) and by +7.6 … −5.4 dB over `decay`: `glass-jar-sweep`, whose EXP is `position`, swings 6.9 dB. The controls do not separate (4.4 dB of cross-term), and a closed-form incoherent sum misses by up to 9.6 dB. `position` needs 65 points (0.9 dB; 1.6 dB with 33). p5's "voice-referenced" constant is two notes on a curve that spans −4.4 … +10.5 dB with pitch (Oil Drum), 3.5 dB off the pitch mean |
| 3 | B | **gfx-dsp moves** (§2.11). `shapers` = gfx-grit's `curves.rs` moved verbatim, in its own commit before this crate, byte-identical renders and tables. `modal` = **new** code in f64 lanes. gfx-acoustic's `body.rs` is not touched. Nothing moves from gfx-radio | gfx-acoustic's bank is an f64 scalar loop with two gains per mode and per mic, tied to its FIR, its D⁻¹ and a 50 – 200 % size: "generalised to N modes and f32 lanes" changes the precision and the summation order, so its renders cannot stay bit-exact. gfx-dsp's numerics rule keeps recursive states in f64, and `filterbank` already is f64 structure-of-arrays. An f32 mode at 30 Hz, Q 160, 192 kHz errs at −49 dB re its own output (`r6`; −60 to −80 dB at 48 kHz). f64 lanes cost 72 ns against 32 (f32) and 114 (scalar). gfx-radio's Carbon is gfx-dsp `Shaper::Asym` plus constants and its rail is `HardClip`: both are in gfx-dsp already. The draft's type table called Crush and Decimate "gfx-grit's law" while giving other laws (16 → 2 bits against 16 → 1; 20 kHz → 312 Hz against 40 kHz → 40 Hz): the engines are shared, the laws are this crate's |
| 4 | B | **CPU: the worst case is measured, and zero-weight slots are skipped** (§2.4, §2.10, §5). Budgets: defaults ≤ 350 ns; with box, comp, echo ≤ 450 ns; Three 2× with one-stage types ≤ 750 ns; **worst ≤ 2.2 µs**; stereo ≤ 1.8 × | The draft's worst row (≤ 1.2 µs "with Fuzz / Fold in every slot") was timed with Soft and Hard, one ADAA stage each. With a two-stage type in the six slots: 0.64 µs Single 2×, **1.34 µs Three 2×, 2.1 – 2.2 µs Three 4×**, 10 % of a core. Skipping a slot at weight 0 halves the dirt at a static `morph` of 0 or 100 %, which is the default and twelve of the seventeen presets. The timing also omits the ρ estimator, the tilt, the DC blocks and the table lookups |
| 5 | B | **Latency table at the document's own defaults, and the de-fizz cut as a shelf** (§2.3, §2.10, §5). Default chain 0.087 ms at 1 kHz (0.119 ms at 4×), Two 0.206 ms, Three 0.342 ms; "what the singer hears" is argued on phase | `r5`, `r2b`. p1's default chain is pre HP 80 Hz + LP 12 kHz, not the defaults, and leaves out the de-fizz LR4 band gain, whose all-pass at 4.5 kHz is in the path at 0 dB: **0.189 ms at 1 kHz** (0.221 ms at 4×), Three **0.444 ms**, over the draft's own bench bound (0.4). A TPT high shelf is the identity at rest (null −325 dB) and gives −8.3 / −11.3 dB of hash at 50 / 100 % against −8.9 / −12.8 dB. The comb against bone conduction follows the phase: the default reaches 180° near 7.6 kHz; the 100 Hz low cut is a lead of 28 – 88°, not 1.3 ms of delay; a split reads −163° at 250 Hz |
| 6 | R | **De-fizz detector** (§2.3): read after `pre_hp` / `pre_lp` and **before the tilt**; weighted by a **floor on the HF band's level** (0 under −66 dBFS, 1 from −60) | `r2`. `tilt` +12 raises a vowel's share by 2 – 6 dB; a breathy vowel then reads −10 … −12 dB against the −14 dB threshold. White hiss reads −1.2 dB: without the floor the duck is at its maximum in every silence, and a phrase starts 9 dB (50 %) or 18 dB (100 %) under its drive, still −3.3 / −6.7 dB after 30 ms |
| 7 | R | **De-fizz amount stated** (§2.3): about 25 – 30 % restores the bypass ratio of 's' to vowel, 50 % is 2 dB under it, 100 % is 5.9 dB under it (8.6 dB with the LR4 cut). Default 50 % kept. **`defizz` 0 on non-vocal presets and pins** (§4, §6) | `r2`, `r2b`. Full band, the 's' is −6.4 dB re the vowel in bypass, −1.5 dB through Soft at 30 dB, −5.5 / −9.2 / −15.0 dB at 25 / 50 / 100 % (LR4). On the real take the flagged frames go from +0.9 to −4.8 / −8.8 dB in 1 – 4 kHz, and the unflagged ones move 0.03 dB: the law works on a real voice. On a kick + hi-hat loop the duck is under −3 dB 94 % of the time; `drum-bus-trash` and `can-lead-guitar` did not turn it off |
| 8 | R | **Level on a real take stated; the −18 dBFS RMS reference written as a lane requirement** (§2.5, §5, §6) | `r3`. On 27 s of singing at −18 dBFS RMS (crest 19 dB, not the steady vowel's 8), Soft is held within 0 … −1.8 dB and Hard within 0 … −2.0 dB (gfx-grit's tables: −4.5 / −4.8 dB), so the direction of claim 3 holds and its ±0.8 dB does not. The loudest quarter of frames ends 4.9 dB under, the quietest 11.5 dB over. A lane 10 dB low comes out about 10 dB hot at high drive |
| 9 | R | **Fold and Sine Fold: g = 10^(d / 40)**, 0 – 24 dB over the drive's range (§2.4). "Inharmonic by nature" removed. `object-morph-exp` loses its trim | `r4`. A memoryless curve on a periodic input has a periodic output: the 32× column's −35 … −61 dB is aliasing too. At 48 dB Fold reads −24 dB at 2× + ADAA on the reference vowel (−18 … −34 dB at other pitches) and −12 dB on a scream: half the type's range was noise, and a preset needed a trim to stay out of it. At 24 dB: −68 … −70 dB (−50 … −54 dB at −6 dBFS). The other figures of the table hold at 294, 524 and 881 Hz (Soft 48 dB: −46 … −59 dB) |
| 10 | R | **A fixed mode set per object, and faded band edges** (§2.7): the ≤ 48 modes are chosen once (mean over `position` of g² / f); a mode fades between 0.35 and 0.45 fs and between 40 and 30 Hz | `r6`. `objects.bank()` selects at the current position and drops at the current size. The Cardboard Box (92 candidates) takes 11 different sets over a position sweep, so `position` is not a gain move there; the Tin Can drops from 28 to 17 modes at `size` 25 %; 12 of the Oil Drum's 37 kept modes are silent at `position` 50 %. p5's motion test ran the Tin Can from 50 to 200 % only, where nothing drops |
| 11 | R | **Object morph: a slot with no partner takes its partner's f and Q** and only fades; the skirt interpolates (log corners, linear gain) (§2.7) | `r6`. With p5's padding, Hands → Tin Can glides 26 modes from 1.3 kHz and reads +1.0 / +0.8 / +1.7 / **+6.0** / −3.4 dB along the morph; with the rule +1.0 / −0.8 / −1.7 / −2.4 / −3.4 dB. p5's morph kept object A's skirt to the end |
| 12 | R | **`depends`** (§3.2): `x_low` in Single; `x_air` in Single and Two; `body_trim` in Single; `<b>_b` at `morph` 0 with `env_morph` 0; `tilt_freq` at `tilt` 0; the box's parameters at `box` 0 % | The draft greyed `low_*` and `air_*` but not their crossovers. In Single, `body_trim` adds to `drive` and nothing else: a second drive knob. 45 parameters and 3 meters stand |
| 13 | R | **Presets** (§4): the product name is out of `growl-split`'s text (and its settings are spelled as parameters); `defizz` 0 on the drum and guitar presets; `robot-ring` states its comb | Preset texts go to golem's AI with the ids: no trademark in either. "body Tube 50 % → Rasp, air Off at −6 dB" was not a parameter setting |
| 14 | R | **The morph's limits written** (§2.4, §2.5): types with memory (Decimate, Fuzz, Starve, Ring) comb or beat in a blend; every band ends in a DC block and the tables are measured after it; the ρ gain is 1 while one slot runs | "Two memoryless shapers have no comb" was stated for all pairs. Rectify, Tube, Carbon and Fuzz make DC, and the draft named a DC block for Tube only |
| 15 | R | **Guard: what a high threshold costs, and the default's limit** (§2.2, §3.3, §6) | `r1`. On the real take the sung notes are untouched at T = −55 … −40 (≤ 0.5 % of the active time cut by 3 dB), but its breaths and sibilants sit at −50 … −40 dBFS: at −40 they are gated, with 16 – 21 closings that re-open within 0.3 s in 27 s. The default −55 only acts when the bleed is under about −65 dBFS, which a loud stage is not. The hysteresis acts on closing only (a closed gate already cuts nothing from T up) |
| 16 | R | **Facts**: the crest factor is the steady vowel's; "first sample 0" carries 3 · 10⁻⁴ of the impulse, half its energy by sample 4 (2×) or 6 (4×); 72 ns replaces 32 ns in §1.7 and §2.7; `results.txt` regenerated (the Rust block with the f64 lanes, the review's output appended) | Accuracy |

### Checked and left as written

- **The chain and its order**, `order` as one enum, the comp, post filter and echo after both. The whole
  Trash 2 chain at zero latency is a real gap in the suite, and the stages do need each other: the
  guard needs every stage's gain (more than the draft thought), the de-fizz acts on the drive, the
  make-up follows the morph.
- **Against the siblings.** gfx-grit keeps Prisma's 25 serial modules and four bands; this pedal shares
  its curves (through `shapers`) and nothing of its face. gfx-radio's devices and gfx-irfx's IR player
  stay distinct: no object here is one of gfx-radio's speakers, and no IR is shipped. The overlap that
  remains is the slap echo (three parameters, question 8) and the one-knob comp, both justified by the
  guard and the level chain.
- **Zero latency by construction**: HIIR half-bands, ADAA, LR4 trees, TPT banks; `skip_latency` with
  gfx-grit's precedent. Group delay of the 2× / 4× pairs confirmed (0.064 / 0.096 ms at 1 kHz).
- **The cap law's shape** (6 dB knee, 1 ms / 80 ms, RMS 5 ms) and its advantage over a 1:4 expander:
  confirmed (−2.9 … −3.1 dB against +9 / +20 dB of bleed left). Onset cost confirmed.
- **The drive duck as the de-fizz principle**: confirmed on p4's phrase from 12 to 42 dB of drive, and
  on the real take.
- **Aliasing at 2× + ADAA** for Soft, Hard, Tube and Rectify: the table's figures re-run and hold at
  four pitches. 2× by default, 4× on `quality`.
- **The live ρ gain**: within 0.10 dB at 50 % on the reference phrase and 0.26 dB on the real take
  (`r8`), with the static tables and a 30 ms average. The ±0.5 dB bench bound is safe.
- **Objects from physics**: the formulas in `objects.py` are textbook, and the numbers I recomputed
  agree (Helmholtz 108 Hz for the bottle and 515 Hz for the hands, the cone's 214 Hz series, the ring
  constant). No recording and no third-party IR. The voicing itself is question 3.
- **Parameters**: linear ranges, real units, enums that only grow at the end (`object_b` = `object`
  shifted by its leading `Same`, which is fine as long as both grow together), no default that
  depends on another parameter, `drive` as the shared id at 0.48 dB per %, `bypass` as in the
  siblings, `gr` and `state` as the shared meters. `x_air` held ≥ 2 × `x_low` follows gfx-mbcomp's
  "held" precedent. `object` None and `box` 0 % overlap, and None is kept because a morph from
  nothing into an object needs it.
- **Preset ids**: 17, none an artist or a trademark.
- **IP**: formulas and public facts only. The survey names products to describe them; no code, table,
  IR, preset or type name comes from them.
- **Hue**: ΔE 29 / 34 / 35 confirmed by `hue.py`.

### Still open

| Point | Owner |
|---|---|
| The guard against a real wedge: the cap with an object and `reso` on, the latch when a howl exceeds T, the threshold procedure at soundcheck | Raph, stage test |
| **Worst-case CPU 2.2 µs** (Fuzz in six slots, Three, 4×, morph moving): accept as a corner, or forbid 4× with more than three two-stage slots | Raph |
| golem's voice lane level: the block expects sung notes near −18 dBFS RMS | golem-rack |
| S(type, d) and the make-up tables for the nine types the prototype does not run; their aliasing at `drive` 100 % (shorten the law, as Fold's, if a type fails −45 dB) | golem-fx, at coding; bench |
| The cap end to end in Rust: third-octave bleed test with each object, `reso`, `comp`, echo, `tilt`; the peak table against `decay` and `size` | golem-fx, at coding; bench |
| De-fizz by ear on screams and on a brighter microphone than the test take: the −14 dB threshold, the 30 ms release, 50 % or 30 % as the default | Raph |
| Object voicings from the owner's measurements (question 3); the Tin Can's 100 Hz wall mode first | Raph, then golem-fx |
| The level table's 26,325 values: generated at build time by the Rust port of `objects.py`, checked against the Python on a few hundred points | golem-fx, at coding |
| `shapers` move: gfx-grit's renders and tables byte-identical before and after, in its own commit | golem-fx, before this crate |
| Catalogue entry: block `morph`, 45 parameters, `depends` as §3.2, pins `guard` −90 and `defizz` 0 on non-vocal lanes, the soundcheck step for `guard` | golem-rack |
| Hue at tile size against raspberry; the glyph at 16 px | Raph, on the family sheet |

## As coded (v0.1.0, 2026-11-15)

**Order** (open question 10). (1) gfx-grit's `curves.rs` → gfx-dsp `shapers`, verbatim, in a commit of
its own: 50 gfx-bench renders of gfx-grit (each of the 25 modules in two slots and two bands with
`drive`; and in the low band at 4×, LR8, `focus`, stereo) byte-identical before and after (md5 of every
WAV), and the generated make-up tables identical apart from their "generated from" comment line.
(2) gfx-dsp `modal`, new code, 8 unit tests (unity peak and skirt at 44.1 – 192 kHz, equality with gfx-dsp
`Svf` band-passes, ramps landing on the static set, rank morph and the partner rule, edge fades, a 30 Hz
Q 160 mode at 192 kHz within 0.05 dB, the analytic response within 0.05 dB of the filter, decay after a
burst). gfx-acoustic is not touched; nothing moves from gfx-radio. (3) This crate.

**Files.** `src/lib.rs` (`Morph`: `PARAMS`, the control tick, the guard, the de-fizz detector and
shelf, the tilt, the box stage, the comp, the post filters, the echo, the `order`, `split` and `quality`
switches, the meters), `src/dirt.rs` (the oversampled engine: LR4 tree, two parallel slots per band with
the type crossfade queue, skipped zero-weight slots, the ρ estimator, aligned dry, DC block, droop
correction), `src/types.rs` (the 15 types: laws, curves with antiderivatives, per-slot state; plain enough
for build.rs), `src/objects.rs` (the port of `proto/objects.py` with the fixed mode set, and the level,
peak and voice-constant generators), `src/voice.rs` (the reference vowel, pink noise, sibilants),
`build.rs` (the tables: make-up and S per type and dB of drive, 26,325 level values, 405 peaks, 9 voice
constants; about 25 s in cargo's unoptimised build-script profile), `src/plugin.rs` (VST3 class id
`GoleMFxMorph0001`, 1 / 1 then 2 / 2, latency 0, meters `gr`, `state`, `defizz_gr`), `src/tests.rs`.

**Measured** (tests on the synthetic signals of `src/voice.rs`; the real-take rows through gfx-bench
render on the maintainer's 27 s sung phrase, set to −18 dBFS RMS over its active parts, not in the repo):

| §5 row | Target | Coded |
|---|---|---|
| Latency | first sample 0; GD at 1 kHz ≤ 0.10 / 0.13 / 0.22 / 0.36 ms | first sample ≠ 0 at 44.1 – 192 kHz in every split and factor; **0.089 / 0.120 / 0.207 / 0.346 ms**; the de-fizz shelf at rest is the exact identity (and does not run) |
| Clean skeleton | ±0.1 dB 20 Hz – 20 kHz | **±0.031 dB** (Single / Two / Three × 2× / 4×) |
| Aliasing, prototype's types | Soft −53 / −42, Hard −41, Fold −64 / −46 (worst of four pitches) | Soft **−55.6 / −45.0**, Hard **−44.3**, Fold **−66.8 / −49.3**; 4× better by 30 / 17 / 15 dB on Soft / Soft −6 / Hard |
| Aliasing, the nine others (drive 100 %, −18 dBFS vowel, 2× / 4×) | ≤ −45 dB or shorten the law | Tube −55.5 / −87.7, Rectify −74.3 / −91.5, Sine Fold −93.5, **Fuzz −50.2 / −71.6** (law shortened, below), **Crush −46.3 / −59.4** (law shortened), Sputter silent (below), Carbon −68.5 / −87.9, Starve −75.7 / −89.4. Inharmonic by design, reported: Decimate +24.3 (its clock is not the pitch's), Ring (every product is f0 ± carrier), Rasp −12.9 (noise) |
| Level, vocal tables (phrase) | ±1 dB | every compensated type within **−0.30 … +0.38 dB** at drive 0 / 25 / 50 / 75 / 100 %, Sputter apart (silent at 100 %) |
| Level, real take | +0.5 / −2.5 dB | Soft 0 … −1.6, Hard 0 … −1.8, Tube −0.2 … −1.5, Fuzz −0.1 … −1.9, Rectify 0 … −0.2, Fold 0 … −2.3, Sine Fold −0.5 … −2.2, Carbon +0.9 … −2.4, Rasp +0.1 … −1.5; **Starve +2.4 dB at 100 %** (outside) |
| Morph level | ±0.5 dB memoryless, ±1.5 with memory | memoryless pairs ≤ **0.01 dB** off the line at 25 / 50 / 75 % (phrase), ≤ 0.25 dB at 50 % on the real take; Ring → Decimate +0.43, Soft → Fuzz, Sputter → Starve, Tube → Rasp 0.00 |
| Guard, bleed | ≤ bypass − 2 dB, every type, drive 0 → 100 %, mix 35 / 100; no third-octave band over bypass with each object, `reso`, `comp`, echo, `tilt`, together | all broadband ≤ −2 dB; worst band, dirt alone **−0.5 dB** (Fuzz 100 %); each object at `box` 100 % −2.4 (Hands) … −11.5 dB (Bottle); `reso` 100 % −2.8, `comp` 100 % −1.5, echo 50 / 60 % −6.3, `tilt` ±12 −1.8 / −2.2, all together −27.9 dB |
| Guard, S(type, d) | within 1 dB of the measured small-signal gain | within **0.27 dB** through the whole pedal (Decimate and Ring hold their worst band by design, below) |
| Guard, onset; quiet vowel | ≤ 3 dB over 5 ms, ≤ 1 dB over 10 ms; ±0.1 dB | −1.24 / −0.53 / **−0.26** / **−0.12** / −0.06 dB over 1 / 2 / 5 / 10 / 20 ms (T −45, drive 36 dB); a −40 dBFS vowel **±0.000 dB**, `state` 1 |
| De-fizz | ≤ −7.5 / −10.5 dB at 50 / 100 % (−4.0 at 0 %); vowels ±0.3 dB; hiss onset ≤ 0.5 dB; tilted breathy vowel ≤ 1 dB; hats at 0 %: meter 0 | **−3.4 / −8.9 / −14.8 dB**; vowels **0.00 dB**; hiss then a vowel: no duck (0.00 dB); breathy vowel (2 – 8 kHz aspiration 12 dB under) with `tilt` +12: **mean 0.18 dB, largest 2.0 dB**; `defizz_gr` 0 at 0 %, the hats read as sibilants at 50 % |
| Objects | mode list fixed; pink ±1 dB off the grid; voice level ±1 dB; `box` 0 bit-exact; 44.1 – 192 kHz | counts 28 / 36 / 22 / 25 / 48 / 37 / 32 / 27 / 2 (Tin Can … Hands), unchanged by `position` and `size`; pink **≤ 0.59 dB** off the grid (5 × 4 × 5 points per object); voice level **−0.52 … +0.69 dB**, single notes −7.4 … +7.4 dB (Oil Drum); `box` 0 bit-exact; finite at every rate with `size` 25 %, `decay` 400 % |
| Box motion | finite; largest step ≤ 1.5 × the static renders' | **1.41 ×** (Cardboard Box, `size` 25 → 400 % in 0.5 s), the reference being the static renders along the path; Hands ↔ Tin Can: −1.3 → −4.7 dB, no bump |
| Comp | GR within 1 dB of the law; meter | **20.99 dB** against 21.00 dB; `gr` 20.99; make-up 0 / 2.0 / 5.2 dB at 40 / 70 / 100 % |
| Echo | ±1 sample; bounded at 90 %; no re-feed with the guard closed | 5,280 samples exactly (re the dry's own peak); bounded and decaying; bleed stays ≥ 2 dB under bypass |
| Switches | ≤ −30 dB re peak | worst **−51.3 dB** (`split`, `quality`, `order`, `object`, `object_b`, types under a held note) |
| Stereo, determinism | L = R bit-exact; renders null | L = R bit-exact and equal to mono within 1e-6; Rasp, Ring, Decimate renders null after reset |
| Common checks | | gfx-bench **28 / 28** at 44.1 / 48 / 96 / 192 kHz (`skip_latency`, `click_bound_db` −30, both as §5; worst step −36 dB at 192 kHz) |

Tables (dB; d = 0 / 12 / 24 / 36 / 48): vocal make-up Soft 0.1 / −10.4 / −15.9 / −17.5 / −17.8 (the
prototype's r2 table to 0.1 dB), Fuzz −13.9 … −17.9, Rectify 0 … +6.6, Fold 0 … −13.2, Sine Fold
−3.8 … −14.9, Sputter 0 … +6.0 (capped), Carbon +0.1 … +2.8, Starve 0 … +8.9. S: Soft 0.1 / 1.6 / 8.0 /
18.5 / 30.1 (the review's +8.0 / +18.5 / +30.2), Hard to 30.1, Tube 29.4, Fuzz 30.0, Fold 10.8, Sine
Fold 12.9, Rectify ≤ 0.9, Ring ≤ 1.8, Decimate ≤ 5.9 (worst band), Carbon 2.8, Starve 8.8, Crush and
Sputter −60 (a dead zone). Objects' voice constants (dB): Tin Can −1.3, Bucket +0.2, Pipe +3.1, Bottle
+3.9, Cardboard Box +0.1, Oil Drum −3.0, Glass Jar +3.0, Cone +2.0, Hands −2.3 (the review's figures
to 0.1 dB except the Cardboard Box, whose fixed mode set differs); their largest peaks at the defaults,
re bypass with the constant: 11.8 / 10.4 / 12.9 / 20.0 / 9.6 / 8.2 / 16.0 / 9.6 / 3.8 dB.

**CPU** (48 kHz, blocks of 128). The machine was loaded (load average 6 – 15, other sessions building)
and the readings scatter by ±30 %; gfx-grit, documented at 236 ns at its defaults, read 383 – 635 ns
beside it, so the figures are given raw and against gfx-grit read in the same minutes:

| Configuration | Budget | Raw (fastest of 5) | × 236 / 383 (gfx-grit's ratio) |
|---|---|---|---|
| Defaults (gfx-bench's CPU line) | ≤ 350 ns | 392 ns mono, 429 ns stereo | ≈ 240 ns |
| Default-like: `box` 60, `comp` 60, `echo_mix` 30 | ≤ 450 ns | 524 ns | ≈ 325 ns |
| Three 2×, one-stage types in A and B, `morph` 50, box, comp, echo | ≤ 750 ns | 1,034 ns | ≈ 640 ns |
| Worst: Three 4×, Fuzz in six slots, morph and box moving, comp, echo | ≤ 2.2 µs | 1,865 ns | ≈ 1,150 ns |
| Stereo / mono | ≤ 1.8 × | 1.1 – 1.6 × | |

Parts at the defaults (raw): the Soft slot's two ADAA tanh 110 – 130 ns, the de-fizz detector 45 ns,
the 48-mode bank at `box` > 0 about 130 ns, the comp 20 ns (its gain computer at the control rate),
the skeleton (half-bands, detectors, ramps) about 230 ns. `cargo test --release -p gfx-morph
cpu_per_configuration -- --ignored --nocapture` re-runs the table; a quiet machine should re-read it.

**Deviations from the spec, and why**

1. **One-stage droop correction**, (1 + a) / (1 + a z⁻¹), not gfx-grit's square. The types are parallel,
   so every path carries one inner average, where gfx-grit's two slots in series carry two; the square
   overshot by +0.11 dB at 4.9 kHz (the clean-skeleton bound is 0.1). Flat within 0.031 dB with one.
2. **Fuzz's law shortened** to 0.7 d (0 – 33.6 dB into its first stage, then gfx-grit's two biased clip
   stages): at the README's 48 dB it aliased at −38.1 dB on the reference vowel (−45.0 at 36 dB). Now
   −50.2 dB at `drive` 100 %.
3. **Crush 16 → 4 bits** (the README's 16 → 2): from about 3 bits down the −18 dBFS vowel falls inside
   the mid-tread's dead zone (−121 dBFS out at 2 bits, the inharmonic share then +32 dB re the
   harmonics); at 4 bits it reads −46.3 dB.
4. **S(type, d) is measured at −90 dBFS** (the README says −60): the guard's cut takes the bleed far below
   −60 dBFS, where the high-gain types gain more than their −60 reading (Fuzz at the old law: +1.5 dB
   over bypass in the bleed test). **Decimate and Ring take their worst third-octave band** (images and
   sidebands lift a band while the whole loses): with the broadband value, Decimate at 100 % left a band
   +2.4 dB over bypass. Both make the cap exact where it matters (README §2.2: "the worst frequency").
5. **Carbon is coded in the crate**, not as gfx-dsp `adaa::Shaper::Asym` (the README's §2.4 / §2.11):
   gfx-radio's carbon curve is 1 / (1 − u) − 1 with a soft tanh guard and no antiderivative. Here the
   same curve with a hard guard at |u| = 0.6 (linear beyond) and its closed-form antiderivative
   (−ln(1 − u) − u), under ADAA, u = h x, h = 0.1 · 10^(d / 40) / (peak of a −18 dBFS RMS sine).
6. **Starve's constants** (the spec gives the law, not the numbers): a rail at −6 dBFS at rest that sags
   by up to 90 % × depth with a 20 ms envelope of |x| normalised to the reference vowel's mean |x|,
   floor 3 %. On the real take it ends +2.4 dB over its input at `drive` 100 % (the loud frames sag the
   rail more than the reference vowel did); by ear.
7. **Sputter**: its make-up is capped at +6 dB (the README's "capped", value unstated). Its dead zone
   reaches −6 dBFS at 48 dB as specified, so from about 90 % a −18 dBFS voice is entirely gated: the
   "broken voice" law taken to its end. `broken-transmission` sits at 50 %.
8. **Rasp's make-up is Tube's table** (the noise rides on top at −12 dB re the band, 0.3 dB at most).
9. **Control rate**: the bands' drives and laws, the detectors' dB laws, ρ and the cap are rebuilt every
   16 samples; the gains that multiply audio (guard, mix, levels, ρ, morph weights, box, comp, echo,
   level) ramp per sample between ticks; the EXP smoothers (gfx-dsp `exp_smooth`) run at the control
   rate. The comp's peak detector runs per sample on the linked linear level and its dB gain computer at
   the tick (a 3 ms attack against a 0.33 ms tick).
10. **The ρ estimator removes the means** (exponential means of A and B over 30 ms): Rectify, Tube,
    Carbon and Fuzz carry DC before the band's DC block, which inflated ρ.
11. **Object morph's peak term** for the guard: the larger of the two ends while 0 < μ < 1 (conservative).
    **Object None** is a flat skirt (10 Hz / 0.45 fs, 0 dB) with no mode, so a morph from None fades the
    partner's modes in.
12. **The level and peak tables are generated at 48 kHz** and read at every rate (they weigh 100 Hz –
    8 kHz; only the edge fades differ with the rate).
13. **De-fizz on a tilted breathy vowel**: the mean duck is 0.18 dB, the largest instantaneous duck 2.0 dB
    (the bound "≤ 1 dB" is met as a mean, not as a peak).
14. **Clean-skeleton null against the twins' cascade** is not a separate test: the skeleton is that
    cascade (the same gfx-dsp tree, half-bands and averages); its flatness is tested.
15. **`body_trim` stays live in Single** (greyed by `depends`, it still adds to `drive`).
16. **`depends`**: golem's descriptor holds `param = option` only, so `x_air` and `air_*` carry
    `split = Three`; the rest of §3.2's list (`low_*` and `x_low` in Single, `body_trim` in Single,
    `<b>_b` at `morph` 0 with `env_morph` 0, `tilt_freq` at `tilt` 0, `ring_hz` without a Ring type, the
    box parameters at `box` 0 or object None, `reso` without the low-pass or `env_filter`, the echo's at
    `echo_mix` 0) needs conditions the format lacks: for golem-rack.
17. **Presets are level-matched on synthetic signals** (the p4-style phrase with sibilants for voice, a
    plucked saw for guitar, a kick + hat loop for drums) through gfx-bench render, not on the bench's voice
    DI: all within ±0.25 dB except `drum-bus-trash` (−1.3 dB at `level` +12, its comp at 70 % squashes
    the loop's peaks). Glass Jar's and Object morph's EXP targets are in the presets' notes (the preset
    format has no EXP field).
18. **Bypass** fades the input out of the chain over 10 ms while the object and the echo ring out (the
    output is the dry plus those tails).

**Left for the owner.** By ear: the de-fizz threshold and 50 % default on a brighter mic, Starve's and
Sputter's top of range, the object voicings (open question 3: the Tin Can's 100 Hz wall mode first),
the guard on a stage against a wedge, the presets on the voice DI. Accept or not the worst-case CPU row
and the heavy rows' raw figures (re-time on a quiet machine). Golem-rack: the block `morph`, the
`depends` conditions above, the voice lane's −18 dBFS RMS level, the soundcheck step for `guard`.
