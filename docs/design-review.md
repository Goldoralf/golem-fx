# Design review

Review of the 22 effect designs, gfx-dsp, gfx-bench and the golem contract — 2026-11-12, by Claude
Fable 5.2, on the designs written by Claude Opus 5.5. The question asked: can each effect be the best in
its field, cover its field, and rival the best proprietary VST3s? Every finding below is already applied
to the design documents; this page records what changed and why, and what remains open.

## 1. Verdict

The designs are sound: each starts from a real survey, picks algorithms the literature supports, and
keeps golem's constraints (zero latency, determinism, catalogue ids). The review found **four errors**,
**one unsolved hard problem**, **several gaps in coverage**, and **one structural flaw** shared by many
documents. None requires starting a design again.

| Effect | Verdict | Rivals (in its use) | Main risk |
|---|---|---|---|
| gfx-gate | Strong after fixes | Fortin Zuul, ISP Decimator, Pro-G | Follow-decay voicing by ear |
| gfx-comp | Strong | Pro-C styles, Kotelnikov (Clean), pedal comps | Voicing five characters; feedback + soft knee solve |
| gfx-chorus | Strong, most ambitious | TAL-Chorus-LX, Arturia Dimension-D / JUN-6, Mobius | BBD clock-domain model; voicing time |
| gfx-eq | Strong | Live EQ blocks of Helix / Fractal; not a Pro-Q (by design: no latency) | Matched designs in an SVF (numerical check) |
| gfx-delay | Strong | EchoBoy / Valhalla Delay core uses, Timeline | Tape voicing |
| gfx-drive | Good | Fractal Drive block approach | Fuzz feel from a parametric engine |
| gfx-flanger | Strong | Mobius, BL-20, Instant Flanger | BBD engine shared with chorus |
| gfx-phaser | Strong | PhaseMistress core, Mobius, a real Vibe | Vibe lamp / LDR voicing |
| gfx-pitch | **At risk (Poly)** | Whammy (mono) yes; Helix / POG poly: unproven | See §3 |
| gfx-deesser | Strong | Pro-DS zero-latency mode, SuprEsser tracking | Vowel / sibilant discrimination |
| gfx-killswitch | Strong | Tremolator / ShaperBox core, SL-2 | golem UI for patterns |
| gfx-reverb | Strong | VintageVerb / BigSky core algorithms | Plate tank quality; CPU with 16 lines |
| gfx-shimmer | Strong | ValhallaShimmer, H9 Shimmer | Loop gain staging |
| gfx-tremolo | Strong | Flint, Fractal tremolo types | Bias / neon voicing |
| gfx-vibrato | Strong | VB-2, Magnatone, scanner | Scanner approximation |
| gfx-wah | Strong | Fractal / Helix wah sets | Table fit of 4th-order models |
| gfx-autowah | Strong | Mu-Tron III, Q-Tron+, MF-101 | Opto response voicing |
| gfx-rotary | Strong | Ventilator, Lex | Directivity tables by ear |
| gfx-octaver | Good; Poly at risk | OC-2 / OC-5 vintage, Octamizer; POG: unproven | Poly octave-down |
| gfx-limiter | Strong | Best zero-latency class; Pro-L only with lookahead | — |
| gfx-boost, gfx-gapmeter | Fine | — | — |

## 2. Errors corrected

1. **gfx-chorus — `depth` meant the wrong thing.** The design defined it as the peak deviation (±ms). golem's
   `chorus.depth` is Calf's mod depth, a **peak-to-peak** swing (delay = min + depth · (1 + lfo) / 2). Kept as
   written, every existing preset would have doubled its pitch wobble. Now peak-to-peak; all reference
   values converted (Juno 3.7 ms, CE-2 5 ms…).
2. **gfx-chorus — `mix` meant the wrong thing.** The design made it a dry / wet crossfade. In golem it is the
   **wet level with the dry at unity** (golem forces Calf's dry to 1.0). Now the default law (`mix_law` Add);
   crossfade and equal-power stay available.
3. **gfx-gate — the close detector would have chattered.** A ~10 ms RMS (and a 5 ms peak fall) ripples
   between the peaks of a low note: a bass B's peaks are 16 ms apart. Replaced by a **sliding maximum over
   one period of the lowest content**, sized from the sidechain high-pass (40 ms full range, 10 ms at
   120 Hz): ripple-free by construction, and it explains to the player why raising the low cut tightens
   the gate.
4. **gfx-tremolo — square edges at zero crossings.** Rejected for the gate (edges wander by up to half a
   period) but adopted for the tremolo: inconsistent, and it blurs the rhythm on low notes. Now fixed 2 ms
   edges, as in gfx-killswitch.

Also tightened: gfx-flanger's through-zero offset was a parameter that changed the reported latency
(the host would re-sync on every move) — now a fixed 1 ms; its feedback limit "to settle" is settled
(−1 dB, soft limiter in the loop); gfx-drive's blend would have combed against the oversampled path — the
clean path now runs through the same filters; gfx-comp's bit-transparency claim needed makeup at 0 dB.

## 3. The unsolved hard problem: polyphonic pitch

gfx-pitch planned a phase vocoder with a 512-point frame to reach the ~10 ms of the best pedals. That
cannot work on guitar: such a frame has 94 Hz bins, a low E sits at 82 Hz, and a chord's notes share
bins. The design now names two candidates — **band-split time-domain shifters** (window per band: long
for lows, short for highs) and a **multi-resolution phase vocoder** — and requires both to be prototyped
in gfx-bench on chords before one is chosen. The same honesty applies to gfx-octaver's Poly octave-down
(per-band square root with branch tracking), which keeps its phase-vocoder fallback.

**Consequence for planning**: Whammy (mono), Classic and Detune are safe. Poly quality "as good as a POG /
Helix Poly" is a research item, not a given; nothing else depends on it.

## 4. Structural flaw corrected: mode-dependent defaults

Seven designs gave parameters a default "per model / per voice / per character", applied "while the
parameter is still at its default". A host stores every value, so that state cannot be detected: the
rule would have failed on the first preset recall. Replaced everywhere by one convention, now in the
[golem contract](golem-contract.md):

- a model owns its intrinsic voicing; user parameters act on top and default to neutral;
- parameters that must follow the model are relative (50 % = the model's own) or have a `Model` / `Auto`
  option;
- per-block values are pinned by golem at insert; reference settings are block presets.

Applied to: comp `knee` (now absolute, the same in every character, consistent with "one static curve");
delay `high_cut` / `drive` / `mod`; drive `tight`; flanger `shape`; phaser `color`; reverb `size` /
`predelay` / `mod` / `early`; chorus front panel (fixed catalogue defaults; the mode × profile settings
became a preset list).

## 5. Coverage added

| Effect | Added | Why |
|---|---|---|
| gfx-gate | **Tail damp** (highs close before the level) | The HUSH / Decimator-rack way of removing hiss under a decaying high-gain note; concrete **follow-decay** algorithm (release 0.25× – 4× from the note's ending slope) |
| gfx-comp | Stereo link, sidechain listen; explicit time-constant semantics per character; feedback soft-knee solve | Needed to implement and to set by ear |
| gfx-eq | **Second bell**, high-cut slope 12 / 24 | The commonest guitar move is two bells at once; 24 dB / oct after a cab |
| gfx-delay | **Diffusion**, **multi-head** (Space Echo), feedback to 110 % (bounded self-oscillation), reverse planned | Ambient washes, RE-201 rhythms, the dub pedal trick |
| gfx-drive | Overdrive = clean + clipped sum (the real TS structure); Fuzz = two cascaded stages | A single shaper cannot do either |
| gfx-phaser | Bi-Phase second LFO (same / reverse / free), series / parallel | The Bi-Phase is its routing |
| gfx-pitch | Fine tune; **scale-aware harmony** specified as phase 2 (golem knows the song key) | Twin leads in key are a marquee feature golem is uniquely placed to do |
| gfx-reverb | **Spring** planned first after v1 (guitar's own reverb), then Bloom, Gated, Reverse; plate tank to be benched against an FDN plate | A guitar rack without its own spring is incomplete |
| gfx-autowah | **Vowel** filter type | Talk-box / Humanizer sounds |
| gfx-octaver | Clean mode's tracking low-pass made explicit | The half-angle identity only holds on a near-sinusoid |
| All | **Applications** section: uses → settings → the feature that matters | So the coding agent builds features for their uses, and the bench listens to each one |

Shared vocabulary fixed in the contract: `mix` laws, `sync` list, `stereo_phase`, `bass_keep`, `drive`,
envelope ids, meters. gfx-dsp gained the thirteen modules the designs actually need, and a numerics
policy (f32 audio, f64 recursive state).

## 6. What "rival the best proprietary plugins" means here

Honest boundaries, so expectations are right:

- **Where we can match or beat them**: everything a live rack needs with zero latency — gate, comp
  characters, modulation (chorus, flanger, phaser, tremolo, vibrato, rotary), delay, reverb, shimmer, wah,
  limiter. The algorithms are the published state of the art; several details (one-period gate detector,
  one static curve across comp characters, deterministic modulation, EXP smoothing, relative de-ess
  detection) are better than common practice.
- **Where the proprietary leaders keep an edge**: years of **voicing by ear** (Valhalla, Strymon, Fractal)
  — algorithms get us to the door, listening gets us through; **latency-bound features** we refuse by
  design (linear-phase / spectral EQ, lookahead-everything, FFT de-noising); **polyphonic pitch** (§3).
- **The lever that decides the result** is therefore the bench and the listening loop, more than any
  further design: reference recordings, blind level-matched comparisons, and the missing DIs (bass,
  voice, keys renders).

### Priority effects: reverb, delay, shimmer

The maintainer's priority (2026-11-12): these three must be as complete as possible. Extended after the review:

- **gfx-reverb**: 11 algorithms (Hall, Plate, Room, Chamber, Cathedral, Ambience, Spring, Bloom, Cloud, Gated,
  Reverse), `era` (Modern / 80s / 70s), three-band decay, wet shelves, swell, drive / dwell, modulation rate,
  sync, decay to 60 s.
- **gfx-delay**: 4 models (Digital, Tape, BBD, Lo-Fi), reverse, 8 tap patterns, pitch and frequency shifters
  in the loop, age, swell, filter sweep, diffusion, 1 ms – 4 s. All combinable.
- **Shimmer in all three**: gfx-reverb has a one-voice shimmer on any algorithm; gfx-delay a shifter in its
  loop; gfx-shimmer is the full instrument (same code).
- **gfx-shimmer**: choice of space, two voices, freeze / infinite, swell, pitch glide, voice spread; an explicit
  acceptance test against ValhallaShimmer (blind A / B).

## 7. Open points

| Point | Owner |
|---|---|
| Poly pitch / poly octave: prototype two engines, choose on the bench | golem-fx (first research task) |
| Read-only VST3 parameters in nih-plug (meters) | golem-fx wrapper; before the first dynamics effect ships |
| Values marked [I] / [U] in the documents (Dimension D absolute delays, GxDetune range, ReaEQ shelf frequency, Solina rates, 105Q voicing) | Bench / measurement |
| Real EXP update intervals per engine | golem rack session (measurement promised) |
| golem UI for step patterns (killswitch), learn-threshold (gate), key routing | golem rack session |
| DIs: bass (fingers, pick), voice (male, female); keys rendered from MIDI | The maintainer / golem rack session |
| Expert parameter counts (15 – 44): which ones golem's Designer exposes | golem catalogue, per effect, when it ships |

## 8. Suggested implementation order

1. Workspace, gfx-dsp core (`smooth`, `exp_smooth`, `svf`, `envelope`, `delay`, `lfo`, `transport`), gfx-bench
   with the common checks, the read-only-parameter wrapper.
2. **gfx-boost** (proves the whole chain to golem in a day), then **gfx-gate**, **gfx-comp**, **gfx-limiter**
   (dynamics share `envelope`).
3. **gfx-eq**, **gfx-deesser**, **gfx-wah**, **gfx-autowah** (share `svf`).
4. **gfx-chorus**, **gfx-flanger**, **gfx-vibrato**, **gfx-tremolo**, **gfx-phaser**, **gfx-rotary** (share `delay`,
   `lfo`, `bbd`).
5. **gfx-delay**, **gfx-reverb**, **gfx-shimmer** (share `fdn`, trails).
6. **gfx-drive**, **gfx-killswitch**, **gfx-octaver** (Vintage, Clean, Synth).
7. **gfx-pitch** family, with the Poly research running from step 1 in gfx-bench.
8. gfx-gapmeter whenever the rack session needs it on the Golem Engine.
