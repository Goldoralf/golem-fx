# golem's legacy blocks: what replaces them

> **Status: DECIDED** (opened 2026-11-15 with the maintainer). This file records the decisions as they are
> taken. golem-rack is told once the list is settled.

golem's catalogue (v46 snapshot in `visuals/data/golem-blocks.json`) still holds 14 blocks that no golem-fx pedal
stands for. They draw as the generic enamel pedal ("Other golem blocks" on `visuals/index.html`). The
maintainer's position: golem's old third-party pedals are obsolete. Each one is either replaced by a golem-fx
pedal, rebuilt as one, or dropped.

## 1. Blocks with a golem-fx equivalent: removed

**Decided.** These old blocks disappear from golem's catalogue. Their presets move to the golem-fx pedal, and the
pedal's `blocks` list names the old type so that existing songs load.

| Old block | Plugin today | Replaced by |
|---|---|---|
| `chorus-js` | JS sstillwell/chorus_stereo | `gfx-chorus` |
| `phaser` | Calf Phaser | `gfx-phaser` |
| `delay-tape` | Calf Vintage Delay | `gfx-delay`, model Tape |
| `delay-tape-aw` | Airwindows TapeDelay2 | `gfx-delay`, model Tape |
| `delay-echo` | GxEchoCat | `gfx-delay`, model Tape |
| `eq-para` | x42-eq parametric | `gfx-eq` |
| `surge-spring` | Surge XT Effects (spring) | `gfx-reverb`, Spring voice |

## 2. Blocks with only a close golem-fx match

| Old block | Plugin today | Closest golem-fx pedal | Decision |
|---|---|---|---|
| `freeze` | GoleM Freeze (CLAP) | `gfx-stretch` Freeze, `gfx-grains` hold | **Decided: removed, no replacement.** The maintainer finds the effect poor; the freeze modes of gfx-stretch and gfx-grains stay. |
| `boost` | Proteus (neural capture, "Overdrive / Boost") | `gfx-drive`, `gfx-emet`, `gfx-shem`, `gfx-boost`; captures go to `pedal-nam` | **Decided: removed.** Overdrive presets go to `gfx-drive` / `gfx-emet`, boost presets to `gfx-boost` / `gfx-shem`; a real capture uses golem's NAM pedal block. |
| `pitch-spectral` | Spectral Shift ("Digital Detuner") | `gfx-pitch` Poly (phase vocoder) or `gfx-detune` | **Decided: removed.** Shift presets go to `gfx-pitch` (Poly), detune presets to `gfx-detune`. |
| `rezfilter` | MDA RezFilter (resonant filter, envelope + LFO) | `gfx-autowah` (envelope, LFO / S&H sources, resonance to self-oscillation) | **Decided: removed**, presets to `gfx-autowah`. |

## 3. Blocks with no golem-fx equivalent

| Old block | What it is | Decision |
|---|---|---|
| `spectral-comp` | Spectral compressor (Robbert van der Helm's CLAP) | **Decided: we build our own** (new crate, design round as usual: state of the art, users' feedback, review by Fable 5.2, then code). Working name `gfx-spectralcomp`. |
| `whammy-dt` | Not an effect: golem drives a real hardware pitch pedal over MIDI | **Decided: kept**, drawn from a generic photograph (§4) |
| `midi-pedal` | Not an effect: a hardware MIDI-controlled pedal | **Decided: kept**, drawn from a generic photograph of its kind (§4) |

## 4. Hardware pedals: showing the real unit

**Wish (maintainer):** golem shows the real pedal for hardware it drives (e.g. the Whammy DT), from a photograph.

Constraint: golem's repository is public (GPL). A retailer's or maker's product photograph is someone else's
copyright, and the maker's logo and name are trademarks; neither can be committed (the project's rule: no maker
livery, logos or trademark names in shipped assets). The options weighed:

**Decided (2026-11-15): generic renders, drawn as vectors.** Instead of generated photographs, one realistic
vector render per *kind* of MIDI hardware, validated by the maintainer: `hardwareUnit(kind)` in
`visuals/js/golem-pedals.js`, kinds `pitch-treadle`, `big-box`, `mid-box`, `compact`, `floor-multi`,
`loop-switcher`, `foot-controller`, `rack-1u` (no livery, logo or lettering; LEDs and screens light when on).
`whammy-dt` draws as `pitch-treadle`; each pedal of golem's registry (`library/pedals/*.json`) names its kind for
`midi-pedal`. A user's own photo (option A below) stays possible later.

- **A. The user's own photo** of the user's own unit, stored in the user's golem library (not in the repo) and
  shown on that block. Legal, and it is literally the real pedal.
- **B. A generic photoreal image** per kind of hardware (a pitch treadle with two footswitches, a MIDI
  foot controller…), generated for golem without logo or lettering, like the drum kits (`assets/kits/`).
  Shipped as the default.
- A and B together: B by default, A when the user has added a photo.

## Log

- 2026-11-15: list drawn from the catalogue snapshot; §1 decided (removed), `freeze` removed, `spectral-comp`
  to be built; hardware photos wished.
- 2026-11-15: boost, pitch-spectral and rezfilter removed (presets redirected); hardware drawn from one generic
  photograph per kind (prompts written).
- 2026-11-15: hardware drawn as vector renders (validated) instead of generated photographs; prompts dropped.
- 2026-11-15: applied by golem-rack (catalogue v48): the rack rewrites old documents on load (boost → emet, or shem when drive ≤ 15; rezfilter → autowah Low-pass; freeze → granular, so an old song still loads), so the gfx descriptors' `blocks` need not list the old types.
