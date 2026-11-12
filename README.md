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

## Build

Rust stable (`~/.cargo/bin`).

```sh
cargo test --workspace --release              # unit tests + the bench's common checks at 48 kHz
cargo run --release -p gfx-bench -- check gfx-boost     # full common checks, four sample rates
cargo run --release -p gfx-bench -- render gfx-boost --in di.wav --out out.wav --set gain=6
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
| 5 | [gfx-delay](gfx-delay/) | `delay` | ReaDelay | design |
| 6 | [gfx-boost](gfx-boost/) | `clean-boost`, `volume` | JS Volume Adjustment | **coded**, passes golem's engine checks |
| 7 | [gfx-killswitch](gfx-killswitch/) | `killswitch` | golem_killswitch.jsfx | design |
| 8 | [gfx-deesser](gfx-deesser/) | `deesser` | golem_deesser.jsfx | **coded**, bench green, to test in golem |
| 9 | [gfx-drive](gfx-drive/) | `guitar-drive` | JS guitar/distortion | design |
| 10 | [gfx-flanger](gfx-flanger/) | `flanger-js` | JS flanger | **coded**, bench green, to test in golem |
| 11 | [gfx-phaser](gfx-phaser/) | `phaser-js` | JS phaser | **coded**, bench green, to test in golem |
| 12 | [gfx-pitch](gfx-pitch/) (+ gfx-pitch-classic, gfx-detune) | `pitch`, `pitch-shift-js`, `detune` | ReaPitch (quarantined), JS superpitch, GxDetune | design |
| 14 | [gfx-tremolo](gfx-tremolo/) | `tremolo` | Calf Pulsator | **coded**, bench green, to test in golem |
| 15 | [gfx-vibrato](gfx-vibrato/) | `vibrato` | GxTubeVibrato / golem_vibrato.jsfx | **coded**, bench green, to test in golem |
| 16 | [gfx-reverb](gfx-reverb/) | `reverb`, `reverb-plate`, `reverb-room` | Dragonfly Hall / Plate / Room | design |
| 17 | [gfx-shimmer](gfx-shimmer/) | `reverb-shimmer` | GxShimmiZita | design |
| 18 | [gfx-wah](gfx-wah/) | `wah`, `wah-color` | GxGCB-95, GxWahwah | **coded**, bench green, to test in golem |
| 19 | [gfx-autowah](gfx-autowah/) | `autowah` | GxAutoWah | **coded**, bench green, to test in golem |
| 20 | [gfx-rotary](gfx-rotary/) | `rotary` | Calf Rotary Speaker | **coded**, bench green, to test in golem |
| 21 | [gfx-octaver](gfx-octaver/) | `octaver` | Gx OC-2 | design |
| 22 | [gfx-limiter](gfx-limiter/) | `limiter` | x42-dpl | **coded**, passes golem's engine checks |
| 13 | [gfx-gapmeter](gfx-gapmeter/) | — (probe for `tools/check-scene-gap.py`) | golem_gapmeter.jsfx | design |

`chorus-js` is not covered by gfx-chorus: its parameters (`length`, `wet`/`dry` in dB) do not map onto
gfx-chorus's. The roadmap numbers are the order of design; the order of implementation is in the review.
