# The golem contract

What every golem-fx effect must satisfy to run as a golem block, on both of golem's engines: REAPER and
the Golem Engine (Ardour 9.9, headless). Agreed with golem's rack session on 2026-11-12.

## Sound

- golem-fx effects are **new sounds**, voiced on their own merits. They are not clones of the REAPER
  blocks they will replace; presets will sound different, and that is accepted.
- They are added to golem's catalogue now and become golem's default blocks later.

## Parameters

- **Catalogue ids are kept.** When an effect stands for an existing golem block, each parameter of that
  block keeps its catalogue `id` (`~/golem/library/catalog/effects.json`) as its nih-plug `#[id]`, and
  the catalogue's **unit**. Ranges may be a little wider than the catalogue's. The display name is free;
  golem maps catalogue id → parameter on its side (engine table).
- **Real units**: dB, ms, Hz, %, semitones, or the catalogue's own scale (e.g. 0 – 1).
- **golem writes plain values.** Ardour converts them through the plugin's controller, so a skewed range
  is fine as long as plain ↔ normalized is exact both ways.
- **Enums in the catalogue's option order.**
- **One plugin may stand for several blocks** when they share ids: `volume` (`gain` −60 … +6 dB) and
  `clean-boost` (`gain` 0 … +12 dB) are both gfx-boost, with `gain` −60 … +12 dB.
- **Meters are read-only VST3 parameters** (gain reduction, gate state…): Ardour reads them from Lua,
  REAPER through `TrackFX_GetParam`; no other channel. nih-plug does not expose VST3 output parameters
  today: the wrapper has to add it (to solve before the first dynamics effect ships).
- **Sidechain (key) inputs are a separate auxiliary bus**; the main layout stays 1 / 1 or 2 / 2.
- **Ids are permanent** once shipped: VST3 class id and parameter ids are stored by sessions and by
  golem's engine table.

## Audio

- **VST3 only.**
- **Mono layout first** (1 in / 1 out), stereo 2 / 2 second. The Golem Engine keeps a lane mono until its
  first plugin whose default layout is stereo, so a gate before the amp must not make the lane stereo.
  Stereo-by-nature effects (chorus, delay, phaser…) may default to stereo; the mono DI then arrives on
  both inputs.
- **Zero latency**, unless reported to the host.
- **No allocation, lock or I/O in `process`.**
- **No modal window, no blocking work when the plugin turns on.** A plugin that freezes the engine on
  enable is quarantined by golem.

## Trails

Effects with a tail (delay, and any later reverb) switch on / off through their own bypass parameter
(nih-plug `make_bypass`, VST3 `kIsBypass`), so their tail rings out:

- Ardour keeps such a plugin running and drives that parameter instead of deactivating it.
- REAPER stops processing a host-bypassed plugin, so golem switches these effects by that parameter, as it
  does for GoleM Freeze (`switch_param_of` in `golem_rig.lua`). golem-side change.

## Modulation and tempo

- **Deterministic modulation.** Every LFO, random source or grain scheduler starts from a fixed state on
  initialize and reset: phase 0, fixed seed, never time- or address-based. Random still means random
  (drift, Doubler paths…), only reproducible. Then a modulated effect renders the same in both engines
  and passes the strict null (≤ −50 dB) like any other. Ardour pre-rolls 2 s before an export and the
  rack's bench gives REAPER the same pre-roll, so a reset at activation is enough.
- **Tempo comes from the host transport** (nih-plug `Transport::tempo`), never from a parameter. REAPER
  and the Golem Engine both provide it. No tempo from the host → 120 BPM, never silence.

## Expression pedal

golem's EXP values arrive every 10 – 20 ms (OSC / MIDI). A fixed short one-pole leaves a staircase that
buzzes on resonant filters; a long one lags. EXP-capable parameters therefore ramp linearly to each new
value over the **measured** update interval (median of the last few), then pass a short critically damped
smoother (3 – 5 ms) and a slew limit. Shared in gfx-dsp (`smooth`). Bench: a 50 Hz-stepped sweep leaves
control-rate sidebands ≤ −80 dB, and a step reaches 90 % within one interval + 5 ms.

## Known host behaviour

- Ardour delivers a parameter value to an **active** VST3 through the next audio cycle's parameter
  queue, and nih-plug ignores the controller's `setParamNormalized` while processing. Live, this is
  invisible; an offline bench must let the engine run one cycle before saving.
- golem_rig sets REAPER's displayed-value ("norm") knobs one display step below the target. Our own
  renders write raw values.

## Proof

Run on golem's side by the rack session:

- `tools/check-engine-blocks.py`: load, every parameter resolved and read back, on/off toggles, xruns.
- `tools/check-engine-parity.py`: the same DI through REAPER and through the Golem Engine; with the same
  binary, null ≤ −50 dB.

When an effect is ready, its **VST3 class id** is sent to the rack session.
