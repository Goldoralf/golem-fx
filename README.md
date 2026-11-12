# golem-fx

golem's in-house effects: one folder per effect (`gfx-<name>`), one format (VST3), loaded by REAPER and
the Golem Engine (Ardour) on Linux, Windows and macOS. Rust / nih-plug. Licence: GPL-3.0-or-later.

These are new sounds, not clones. They are added to golem's catalogue first and become golem's default
blocks later, keeping the catalogue's parameter ids so presets keep their settings. The rules are in
[docs/golem-contract.md](docs/golem-contract.md).

Shared code: [gfx-dsp](gfx-dsp/), the DSP library every effect is built on (delay lines, LFOs, filters,
dynamics, saturation, BBD model…), and `gfx-bench`, the offline render and measurement tool.

## Roadmap

In order. The golem block each effect stands for is its catalogue `type`.

| # | Effect | golem block | Replaces | Status |
|---|---|---|---|---|
| 0 | [gfx-dsp](gfx-dsp/), [gfx-bench](gfx-bench/) | — | — | design |
| 1 | [gfx-gate](gfx-gate/) | `gate` | ReaGate | design |
| 2 | [gfx-comp](gfx-comp/) | `comp` | ReaComp | design |
| 3 | [gfx-chorus](gfx-chorus/) | `chorus` | Calf Multi Chorus / golem_chorus.jsfx | design |
| 4 | [gfx-eq](gfx-eq/) | `eq` | ReaEQ | design |
| 5 | [gfx-delay](gfx-delay/) | `delay` | ReaDelay | design |
| 6 | [gfx-boost](gfx-boost/) | `clean-boost`, `volume` | JS Volume Adjustment | design |
| 7 | [gfx-killswitch](gfx-killswitch/) | `killswitch` | golem_killswitch.jsfx | design |
| 8 | [gfx-deesser](gfx-deesser/) | `deesser` | golem_deesser.jsfx | design |
| 9 | [gfx-drive](gfx-drive/) | `guitar-drive` | JS guitar/distortion | design |
| 10 | [gfx-flanger](gfx-flanger/) | `flanger-js` | JS flanger | design |
| 11 | [gfx-phaser](gfx-phaser/) | `phaser-js` | JS phaser | design |
| 12 | [gfx-pitch](gfx-pitch/) (+ gfx-pitch-classic, gfx-detune) | `pitch`, `pitch-shift-js`, `detune` | ReaPitch (quarantined), JS superpitch, GxDetune | design |
| 13 | [gfx-gapmeter](gfx-gapmeter/) | — (probe for `tools/check-scene-gap.py`) | golem_gapmeter.jsfx | design |

`chorus-js` is not covered by gfx-chorus: its parameters (`length`, `wet`/`dry` in dB) do not map onto
gfx-chorus's.
