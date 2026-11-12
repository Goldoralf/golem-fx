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
| 0 | gfx-dsp, gfx-bench | — | — | design |
| 1 | gfx-gate | `gate` | ReaGate | — |
| 2 | gfx-comp | `comp` | ReaComp | — |
| 3 | [gfx-chorus](gfx-chorus/) | `chorus` | Calf Multi Chorus / golem_chorus.jsfx | design |
| 4 | gfx-eq | `eq` | ReaEQ | — |
| 5 | gfx-delay | `delay` | ReaDelay | — |
| 6 | gfx-boost | `clean-boost`, `volume` | JS Volume Adjustment | — |
| 7 | gfx-killswitch | `killswitch` | golem_killswitch.jsfx | — |
| 8 | gfx-deesser | `deesser` | golem_deesser.jsfx | — |
| 9 | gfx-drive | `guitar-drive` | JS guitar/distortion | — |
| 10 | gfx-flanger | `flanger-js` | JS flanger | — |
| 11 | gfx-phaser | `phaser-js` | JS phaser | — |
| 12 | gfx-pitch | `pitch-shift-js`, `pitch`, `detune` | JS pitch shift, ReaPitch (quarantined) | — |
| 13 | gfx-gapmeter | — (probe for `tools/check-scene-gap.py`) | golem_gapmeter.jsfx | contract to agree |

`chorus-js` is not covered by gfx-chorus: its parameters (`length`, `wet`/`dry` in dB) do not map onto
gfx-chorus's.
