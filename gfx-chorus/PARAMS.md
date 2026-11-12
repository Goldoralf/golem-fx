# gfx-chorus — parameters

> **Status: PROPOSED** — design, no code yet. Values marked **P** depend on the profile or the mode; they
> are set in [VOICING.md](VOICING.md).

## Rules

1. **The identifier is a contract.** nih-plug derives the VST3 parameter id from the `id` string: it never
   changes once published. golem presets and the catalogue (`effects.json`) refer to it. Parameters are
   added, never renamed nor removed.
2. **No parameter writes another.** VST3 hosts handle badly a plugin that changes its own parameters
   (automation, state, undo). The profile therefore does not copy its values into the expert parameters:
   each expert group has a **Custom** switch.
   - Custom off: the group follows the profile (its controls are greyed, `depends` in the catalogue).
   - Custom on: the group uses its own controls.
3. **Front-panel macros always apply.** Depth and Warmth move the sound around an *anchor*: the profile's,
   or the expert group's when it is Custom.
4. **Two kinds of parameters**, for golem's scene changes and EXP pedal:
   - **continuous**: smoothed internally, click-free, EXP-capable;
   - **structural** (profile, mode, engine, voice count…): a change triggers an internal ~20 ms
     crossfade between the old and the new state, so that a scene change never clicks.
5. **Musical units** (Hz, ms, dB, %), logarithmic curves for frequencies and rates.

Columns: **Type** `f` float, `e` enum, `b` bool · **Curve** lin / log · **Smooth** in ms (— = structural,
crossfaded) · **EXP** = drivable by the expression pedal (`exp: true` in the catalogue).

## Front panel — the golem block

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `profile` | Profile | e | Guitar Clean · Guitar Drive · Bass · Keys · Voice | Guitar Clean | — | — | | Picks the voicing: anchors of every group, internal scale of Depth and Warmth |
| `mode` | Mode | e | Classic · Dual · Dimension · Ensemble · Doubler | P | — | — | | Voice topology (see Modulation group) |
| `rate` | Rate | f | 0.05 – 10 Hz | P | log | 50 | ✓ | LFO rate. Ignored when `sync` ≠ Off |
| `depth` | Depth | f | 0 – 100 % | 50 % | lin | 50 | ✓ | Delay excursion; 100 % = `mod_range` (profile anchor or Custom value) |
| `warmth` | Warmth | f | 0 – 100 % | 50 % | lin | 30 | ✓ | Colour macro: wet low-pass, input saturation, compander and BBD bandwidth, in the profile's proportions |
| `mix` | Mix | f | 0 – 100 % | P | lin | 20 | ✓ | Dry / wet balance (law: `mix_law`) |

## Expert

### Crossover — dry, mono lows

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `xover_custom` | Crossover Custom | b | off / on | off | — | — | | Off = follows the profile |
| `xover_on` | Crossover | b | off / on | P | — | — | | Enables the low split |
| `xover_freq` | Crossover Freq | f | 40 – 500 Hz | P | log | 30 | | Below this frequency: dry signal, summed to mono |
| `xover_slope` | Crossover Slope | e | 12 dB · 24 dB | 24 dB | — | — | | Linkwitz-Riley 2 or 4 (flat sum, coherent phase) |

### Color — engine and character

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `color_custom` | Color Custom | b | off / on | off | — | — | | Off = follows the profile |
| `engine` | Engine | e | Clean · BBD · Tape | P | — | — | | Delay-line engine |
| `interp` | Interpolation | e | Hermite · Lagrange 5 · Allpass · Sinc | Lagrange 5 | — | — | | Clean engine only (`depends: engine=Clean`) |
| `drive` | Drive | f | 0 – 100 % | P | lin | 30 | | Soft saturation of the input stage (wet only) |
| `bbd_chip` | BBD Chip | e | 512 · 1024 · 2048 · 4096 stages | 1024 | — | — | | Stage count: sets the clock frequency and so the aliasing (`depends: engine=BBD`) |
| `bbd_bw` | BBD Bandwidth | f | 2 – 15 kHz | P | log | 30 | | Cut-off of the anti-aliasing and reconstruction filters |
| `compander` | Compander | f | 0 – 100 % | P | lin | 30 | | NE570-style compression/expansion: pumping, breathing noise |
| `noise` | Noise | f | 0 – 100 % | P | lin | 30 | | Hiss and clock bleed (BBD), tape hiss (Tape) |
| `wow` | Wow | f | 0 – 100 % | P | lin | 50 | | Slow speed drift (`depends: engine=Tape`) |
| `flutter` | Flutter | f | 0 – 100 % | P | lin | 50 | | Fast speed variation (`depends: engine=Tape`) |

### Modulation — voices and LFO

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `mod_custom` | Modulation Custom | b | off / on | off | — | — | | Off = follows the profile and the mode |
| `delay_base` | Base Delay | f | 1 – 30 ms | P | log | 100 | | Centre delay |
| `mod_range` | Mod Range | f | 0.1 – 10 ms | P | log | 100 | | Excursion at Depth = 100 % |
| `lfo_shape` | LFO Shape | e | Sine · Triangle · Rounded · Random · Juno | P | — | — | | Rounded = softened triangle; Random = smoothed random; Juno = Juno-60 triangle |
| `voices` | Voices | e | 1 – 6 | P | — | — | | Used by Ensemble and Doubler; fixed by the other modes |
| `voice_spread` | Voice Spread | f | 0 – 360° | P | lin | 50 | | LFO phase offset between successive voices |
| `drift` | Drift | f | 0 – 100 % | P | lin | 100 | | Slow random detuning of rate and depth between voices ("human") |
| `crossmix` | Cross-Mix | f | 0 – 100 % | P | lin | 30 | | L/R cross-mix of the complementary voices (Dimension mode) |
| `feedback` | Feedback | f | 0 – 50 % | 0 % | lin | 30 | | Feedback; above ~30 % it drifts towards a flanger |
| `sync` | Sync | e | Off · 4/1 · 2/1 · 1/1 · 1/2 · 1/4 · 1/8 · 1/16 (+ dotted / triplets) | Off | — | — | | Locks the LFO to the host tempo |

### Stereo

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `stereo_custom` | Stereo Custom | b | off / on | off | — | — | | Off = follows the profile |
| `stereo_phase` | Stereo Phase | f | 0 – 180° | P | lin | 50 | | LFO offset between left and right |
| `width` | Width | f | 0 – 150 % | P | lin | 30 | | Wet width (M/S); 0 % = mono wet |
| `mono_safe` | Mono Safe | b | off / on | P | — | — | | Complementary voices built so that the mono sum does not hollow out |

### Wet filters

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `filter_custom` | Filter Custom | b | off / on | off | — | — | | Off = follows the profile |
| `wet_hpf` | Wet Low Cut | f | 20 – 1,000 Hz | P | log | 30 | | High-pass on the wet only |
| `wet_lpf` | Wet High Cut | f | 1 – 20 kHz | P | log | 30 | | Low-pass on the wet; Warmth moves it around this anchor |

### Output

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `mix_law` | Mix Law | e | Linear · Equal Power | P | — | — | | Linear = classic chorus (marked phase notches); Equal Power = constant level |
| `level_comp` | Level Comp | b | off / on | on | — | — | | Compensates level so the effect does not seem better only because it is louder |
| `output` | Output | f | −12 – +12 dB | 0 dB | lin | 20 | | Final output level |

## To check on the bench

- **`output` has a negative minimum.** REAPER's "bipolar slider" trap is documented for JSFX
  (`~/golem/docs/pitfalls/jsfx.md`); check it for an nih-plug VST3 with `fxset` (write, read back).
  Fallback: a 0 – 400 % linear gain range.
- **Enums and `map`.** The catalogue requires `map: norm` for JUCE VST3 enums; measure nih-plug before
  writing the catalogue entry.
- **Bypass.** golem disables a block through the host's bypass; nih-plug also exposes a VST3 bypass
  parameter. Pick one, and check that no click nor leftover delay-line content is heard on return.
- **Parameter count:** 6 on the front panel, 34 in expert mode (5 of them Custom switches). An order of
  magnitude, not a target.
