# golem-fx

golem's in-house effects: one folder per effect (`gfx-<name>`), one format (VST3), loaded by REAPER and
the Golem Engine (Ardour) on Linux, Windows and macOS. Rust / nih-plug. Licence: GPL-3.0-or-later.

These are new sounds, not clones. They are added to golem's catalogue first and become golem's default
blocks later, keeping the catalogue's parameter ids so presets keep their settings. The rules are in
[docs/golem-contract.md](docs/golem-contract.md).

All designs were reviewed on 2026-11-12: [docs/design-review.md](docs/design-review.md) (verdict per effect,
corrections, open points, implementation order). Every design ends with an Applications table: what each
feature is for.

Shared code: [gfx-dsp](gfx-dsp/), the DSP library every effect is built on (delay lines, LFOs, filters,
dynamics, saturation, BBD model…), and `gfx-bench`, the offline render and measurement tool.

Visuals: [visuals/](visuals/) holds golem's glyph set and the pedal renderers (golem-fx pedals, NAM
captures and amps, a generic pedal for every other block, a lane drawn on a stage floor). Open
[visuals/index.html](visuals/index.html) to see them all.

## Build

Rust stable (`~/.cargo/bin`).

```sh
cargo test --workspace --release              # unit tests + the bench's common checks at 48 kHz
cargo run --release -p gfx-bench -- check gfx-boost     # full common checks, four sample rates
cargo run --release -p gfx-bench -- render gfx-boost --in di.wav --out out.wav --set gain=6
cargo run --release -p gfx-bench -- manifest  # every effect's parameters as JSON (visuals/data/manifest.json)
cargo xtask bundle gfx-boost --release        # → target/bundled/GoleM FX Boost.vst3 (same for gfx-gate)
```

Each effect crate has a host-independent processor (`gfx_dsp::Effect`) and a nih-plug wrapper behind the
`plugin` feature; gfx-bench links the processors.

## Roadmap

In order. The golem block each effect stands for is its catalogue `type`.

| # | Effect | golem block | Replaces | Status |
|---|---|---|---|---|
| 0 | [gfx-dsp](gfx-dsp/), [gfx-bench](gfx-bench/) | — | — | core coded (+ oversampling, ADAA) |
| 1 | [gfx-gate](gfx-gate/) | `gate` | ReaGate | **coded**, passes golem's engine checks |
| 2 | [gfx-comp](gfx-comp/) | `comp` | ReaComp | **coded**, passes golem's engine checks |
| 3 | [gfx-chorus](gfx-chorus/) | `chorus` | Calf Multi Chorus / golem_chorus.jsfx | **coded**, bench green, to test in golem |
| 4 | [gfx-eq](gfx-eq/) | `eq` | ReaEQ | **coded**, passes golem's engine checks |
| 5 | [gfx-delay](gfx-delay/) | `delay` | ReaDelay | **coded**, bench green, to test in golem |
| 6 | [gfx-boost](gfx-boost/) | `clean-boost`, `volume` | JS Volume Adjustment | **coded**, passes golem's engine checks |
| 7 | [gfx-killswitch](gfx-killswitch/) | `killswitch` | golem_killswitch.jsfx | **coded**, bench green, to test in golem |
| 8 | [gfx-deesser](gfx-deesser/) | `deesser` | golem_deesser.jsfx | **coded**, bench green, to test in golem |
| 9 | [gfx-drive](gfx-drive/) | `guitar-drive` | JS guitar/distortion | **coded**, bench green, to test in golem |
| 10 | [gfx-flanger](gfx-flanger/) | `flanger-js` | JS flanger | **coded**, bench green, to test in golem |
| 11 | [gfx-phaser](gfx-phaser/) | `phaser-js` | JS phaser | **coded**, bench green, to test in golem |
| 12 | [gfx-pitch](gfx-pitch/) (+ gfx-pitch-classic, gfx-detune) | `pitch`, `pitch-shift-js`, `detune` | ReaPitch (quarantined), JS superpitch, GxDetune | **coded**, bench green, to test in golem |
| 14 | [gfx-tremolo](gfx-tremolo/) | `tremolo` | Calf Pulsator | **coded**, bench green, to test in golem |
| 15 | [gfx-vibrato](gfx-vibrato/) | `vibrato` | GxTubeVibrato / golem_vibrato.jsfx | **coded**, bench green, to test in golem |
| 16 | [gfx-reverb](gfx-reverb/) | `reverb`, `reverb-plate`, `reverb-room` | Dragonfly Hall / Plate / Room | **coded**, bench green, to test in golem |
| 17 | [gfx-shimmer](gfx-shimmer/) | `reverb-shimmer` | GxShimmiZita | **coded**, bench green, to test in golem |
| 18 | [gfx-wah](gfx-wah/) | `wah`, `wah-color` | GxGCB-95, GxWahwah | **coded**, bench green, to test in golem |
| 19 | [gfx-autowah](gfx-autowah/) | `autowah` | GxAutoWah | **coded**, bench green, to test in golem |
| 20 | [gfx-rotary](gfx-rotary/) | `rotary` | Calf Rotary Speaker | **coded**, bench green, to test in golem |
| 21 | [gfx-octaver](gfx-octaver/) | `octaver` | Gx OC-2 | **coded**, bench green, to test in golem |
| 22 | [gfx-limiter](gfx-limiter/) | `limiter` | x42-dpl | **coded**, passes golem's engine checks |
| 13 | [gfx-gapmeter](gfx-gapmeter/) | — (probe for `tools/check-scene-gap.py`) | golem_gapmeter.jsfx | **coded**, bench green, to wire in check-scene-gap.py |
| 23 | [gfx-met](gfx-met/) | new block (keyed expander-gate) | — (Fortin Zuul's role) | **coded**, bench green, to test in golem |
| 24 | [gfx-emet](gfx-emet/) | new block (high-headroom 808 overdrive) | — (Hexdrive's role) | **coded**, bench green, to test in golem |
| 25 | [gfx-shem](gfx-shem/) | new block (clean shaped tight boost) | — (Fortin Grind's role) | **coded**, bench green, to test in golem |
| 26 | [gfx-golem](gfx-golem/) | new block (live vocal octave doubler) | — (the Sleep Token *Emergence* vocal) | **coded**, bench green, prototype approved by ear, to test in golem |
| 27 | [gfx-glitch](gfx-glitch/) | new block (spectral freeze / glitch) | — | **coded**, bench green, to test in golem |
| 28 | [gfx-ringmod](gfx-ringmod/) | `ringmod`, `surge-treemonster` | mda RingMod, Surge Treemonster | **designed**, reviewed by Fable 5.2 |
| 29 | [gfx-freqshift](gfx-freqshift/) | `surge-freqshift` | Surge Frequency Shifter | **designed**, reviewed by Fable 5.2 |
| 30 | [gfx-crusher](gfx-crusher/) | `bitcrusher` | mda Degrade | **designed**, reviewed by Fable 5.2 |
| 31 | [gfx-geq](gfx-geq/) | `eq-graphic` | GxGraphicEQ | **designed**, reviewed by Fable 5.2 |
| 32 | [gfx-swell](gfx-swell/) | new block `swell` (auto-swell) | — | **designed**, reviewed by Fable 5.2 |
| 33 | [gfx-fuzz](gfx-fuzz/) | new block `fuzz` (circuit fuzzes, guitar in the netlist) | — | **designed**, reviewed by Fable 5.2 |
| 34 | [gfx-dist](gfx-dist/) | new block `dist` (circuit distortions and overdrives) | — | **designed**, reviewed by Fable 5.2 |
| 35 | [gfx-bassdi](gfx-bassdi/) | new block `bass-di` (bass preamp / DI) | — | **designed**, reviewed by Fable 5.2 |
| 36 | [gfx-tape](gfx-tape/) | new block `tape` (tape and media saturation) | — | **designed**, reviewed by Fable 5.2 |
| 37 | [gfx-grit](gfx-grit/) | `grit` | Prisma | **designed**, reviewed by Fable 5.2 |
| 38 | [gfx-transient](gfx-transient/) | new block `transient` (transient shaper) | — | **designed**, reviewed by Fable 5.2 |
| 39 | [gfx-mbcomp](gfx-mbcomp/) | new block `mb-comp` (multiband compressor) | — | **designed**, reviewed by Fable 5.2 |
| 40 | [gfx-ducker](gfx-ducker/) | new block `ducker` (key / self / pump ducking) | — | **designed**, reviewed by Fable 5.2 |
| 41 | [gfx-cab](gfx-cab/) | `cab` (+ `ir-fx` as sibling crate gfx-irfx) | x42 IR Convolver Mono / Stereo | **designed**, reviewed by Fable 5.2 |
| 42 | [gfx-acoustic](gfx-acoustic/) | new block `acoustic-sim` (electric to acoustic) | — | **designed**, reviewed by Fable 5.2 |
| 43 | [gfx-widener](gfx-widener/) | new block `widener` (mono-safe stereo width) | — | **designed**, reviewed by Fable 5.2 |
| 44 | [gfx-tuner](gfx-tuner/) | `tuner` | golem_tuner.jsfx | **designed**, reviewed by Fable 5.2 |
| 45 | [gfx-correct](gfx-correct/) | `pitch-correct` | x42-Autotune (Zita-AT1) | **designed**, reviewed by Fable 5.2 |
| 46 | [gfx-formant](gfx-formant/) | new block `formant` (formant shifter) | — | **designed**, reviewed by Fable 5.2 |
| 47 | [gfx-vocoder](gfx-vocoder/) | new block `vocoder` (channel vocoder) | — | **designed**, reviewed by Fable 5.2 |
| 48 | [gfx-radio](gfx-radio/) | new block `radio` (telephone, megaphone, AM, walkie…) | — | **designed**, reviewed by Fable 5.2 |

gfx-met, gfx-emet and gfx-shem (2026-11-13) are high-gain companions designed from the function of three
Fortin / Neural DSP pedals, as new sounds; their A / B against the Nameless Suite waits for the Windows
renders.

`chorus-js` is not covered by gfx-chorus: its parameters (`length`, `wet`/`dry` in dB) do not map onto
gfx-chorus's. The roadmap numbers are the order of design; the order of implementation is in the review.
