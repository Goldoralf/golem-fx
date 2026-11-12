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
- **Linear ranges only.** Ardour converts plain values through the plugin's controller, but REAPER's path
  (golem_rig) writes the normalized value as (v − min) / (max − min) — measured on gfx-boost 2026-11-12. A
  skewed range would land on the wrong value in REAPER. Knob feel is golem's UI's business, not the
  plugin's.
- **Enums in the catalogue's option order.**
- **One plugin may stand for several blocks** when they share ids: `volume` (`gain` −60 … +6 dB) and
  `clean-boost` (`gain` 0 … +12 dB) are both gfx-boost, with `gain` −60 … +12 dB.
- **Meters are read-only VST3 parameters** (gain reduction, gate state…): Ardour reads them from Lua,
  REAPER through `TrackFX_GetParam`; no other channel. golem-fx builds on **golem's nih-plug fork**
  ([Goldoralf/nih-plug](https://github.com/Goldoralf/nih-plug), branch `golem-fx`): `FloatParam::read_only()`
  + `set_output_value()`, reported as `kIsReadOnly` and through the output parameter changes. The fork also
  applies controller writes while the plugin is processing (upstream drops them), so a host that saves
  its state before a cycle keeps the value.
- **Sidechain (key) inputs are a separate auxiliary bus**; the main layout stays 1 / 1 or 2 / 2.
- **Ids are permanent** once shipped: VST3 class id and parameter ids are stored by sessions and by
  golem's engine table.

## Parameter conventions (all effects)

Decided at the design review of 2026-11-12, to keep 22 effects coherent.

- **No mode-dependent defaults.** A parameter's default never depends on another parameter (a host stores
  every value, so "still at its default" cannot be told from "set by the user"). Instead:
  - a **model owns its intrinsic voicing** (its filters, its saturation, its modulation), and user
    parameters apply **on top**, defaulting to neutral (0 %, filter off, 0 dB);
  - where a parameter must follow the model, it is **relative**: a percentage where 50 % (or 100 %,
    stated in the table) = the model's own value, or an enum whose first option is `Model` / `Auto`;
  - values that differ per golem block are set by golem at insert (`init` / `FX_INIT`), not by the
    plugin's defaults.
- **Reference settings are presets.** "CE-2", "Juno I", "Phase 90 script"… are golem block presets
  (`library/block-presets/<type>/`) carrying the mode and the settings; the design documents list them so
  they can be authored.
- **Shared ids**, same meaning and unit everywhere they appear:

  | id | Meaning |
  |---|---|
  | `mix` | Keeps the **catalogue's law** for its block, stated in each document (wet level with the dry at unity for delay, reverb, chorus; equal-power crossfade for shimmer, pitch) |
  | `on`, `trails` | Bypass parameter with tail (§Trails) |
  | `sync` | Off · 4/1 · 2/1 · 1/1 · 1/2 · 1/2• · 1/2T · 1/4 · 1/4• · 1/4T · 1/8 · 1/8• · 1/8T · 1/16 · 1/16• · 1/16T · 1/32 — phase from the host's position. Catalogue lists (`delay.time`, `killswitch.sync`) keep their own options |
  | `stereo_phase` | L / R LFO offset, 0 – 180° |
  | `bass_keep` | Hz, 0 = off: below it the signal stays dry, mono and unmodulated (Linkwitz-Riley) |
  | `drive` | 0 – 100 %, extra saturation on top of the model's own |
  | `env_sens`, `env_att`, `env_rel` | Envelope follower: 0 – 100 % sensitivity (level-independent, dB window), 1 – 100 ms, 20 – 2,000 ms |
  | `gr`, `state` | Read-only meters |
  | `model` / `mode` / `character` | `model` = a reference unit's voicing; `mode` = a behaviour or topology; `character` = gfx-comp's dynamics families |
- **Enums only grow at the end.** Integers (patterns, step counts) are nih-plug `IntParam`s.
- **Every document ends with an Applications section**: what each feature is for, with starting settings.
  The coding agent implements features for those uses, and the bench listens to them.

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

How EXP values arrive (golem's `bridge/foot.py`, 2026-11-12; the rate itself not measured yet):

- The foot controller sends a **7-bit** MIDI CC (0 – 127) each time the pedal moves; the bridge drops
  repeats and forwards each CC at once, with no throttle and no smoothing.
- REAPER: one OSC message per CC, applied by its control surface on the main thread — so values land in
  **batches**, several steps at once. Golem Engine: the same OSC message, applied at the next audio cycle.
- So: at most 128 steps, **irregular** intervals (a few ms during a fast sweep, nothing while the foot
  rests), bursts on REAPER.

Rule for EXP-capable parameters (shared in gfx-dsp `smooth`): on each new value, ramp linearly from the
current value to the target over the recent update interval (median of the last few, clamped to
5 – 30 ms; in a burst only the last value counts), then a short critically damped smoother (3 – 5 ms) and a
slew limit. The 7-bit steps are the smoother's job, not the plugin's resolution: musical parameters
driven by EXP must not sound stepped at 128 values. Bench: a 50 Hz-stepped sweep leaves control-rate
sidebands ≤ −80 dB, a step reaches 90 % within one interval + 5 ms, a 128-step sweep has no audible steps.

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
