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
3. **Warmth always applies.** It moves the colour around an *anchor*: the profile's, or the expert
   group's when it is Custom. Depth is an absolute excursion: the **peak** deviation of the delay around its
   centre (± ms); the profile and the mode only set its default.
4. **golem catalogue ids are kept.** gfx-chorus carries the ids of golem's `chorus` block (`rate`,
   `depth`, `mix`, see `library/catalog/effects.json`), in the catalogue's units, with ranges a little
   wider. A preset keeps its settings when its block switches to gfx-chorus. Every other id is new.
5. **Two kinds of parameters**, for golem's scene changes and EXP pedal:
   - **continuous**: smoothed internally, click-free, EXP-capable;
   - **structural** (profile, mode, engine, voice count…): a change triggers an internal ~20 ms
     crossfade between the old and the new state, so that a scene change never clicks.
6. **Musical units** (Hz, ms, dB, %), logarithmic curves for frequencies and rates.

Columns: **Type** `f` float, `e` enum, `b` bool · **Curve** lin / log · **Smooth** in ms (— = structural,
crossfaded) · **EXP** = drivable by the expression pedal (`exp: true` in the catalogue).

## Front panel — the golem block

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `profile` | Profile | e | Guitar Clean · Guitar Drive · Bass · Keys · Voice | Guitar Clean | — | — | | Picks the voicing: anchors of every group, internal scale of Depth and Warmth |
| `mode` | Mode | e | Classic · Dual I · Dual II · Dual I+II · Dimension 1 · Dimension 2 · Dimension 3 · Ensemble · Doubler | P | — | — | | Voice topology and its reference settings ([VOICING.md](VOICING.md) §2) |
| `rate` | Rate | f | 0.05 – 10 Hz | P | log | 50 | ✓ | LFO rate (catalogue `chorus.rate`: 0.1 – 3 Hz). Ignored when `sync` ≠ Off |
| `depth` | Depth | f | 0.1 – 10 ms | P | log | 50 | ✓ | Peak deviation of the delay, ± ms (catalogue `chorus.depth`: 1 – 8 ms) |
| `warmth` | Warmth | f | 0 – 100 % | 50 % | lin | 30 | ✓ | Colour macro: wet low-pass, input saturation, BBD bandwidth and noise, in the profile's proportions |
| `mix` | Mix | f | 0 – 1 (shown as %) | P | lin | 20 | ✓ | Dry / wet balance, law `mix_law` (catalogue `chorus.mix`: 0 – 1) |

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
| `interp` | Interpolation | e | Lagrange 3 · Lagrange 5 · Sinc | Lagrange 5 | — | — | | Clean engine only (`depends: engine=Clean`) |
| `drive` | Drive | f | 0 – 100 % | P | lin | 30 | | Soft saturation of the input stage (wet only) |
| `bbd_chip` | BBD Chip | e | 256 · 512 · 1024 · 2048 · 4096 stages | P | — | — | | Stage count: sets the clock frequency and so the aliasing (`depends: engine=BBD`) |
| `bbd_bw` | BBD Bandwidth | f | 2 – 15 kHz | P | log | 30 | | Cut-off of the anti-aliasing and reconstruction filters |
| `emphasis` | Emphasis | f | 0 – 100 % | P | lin | 30 | | BBD pre-emphasis before the line and matching de-emphasis after (CE-2: ≈ +15 dB treble shelf); lowers hiss, shapes the BBD's distortion |
| `compander` | Compander | f | 0 – 100 % | P | lin | 30 | | NE570-style compression/expansion (Dimension D): pumping, breathing noise |
| `noise` | Noise | f | 0 – 100 % | P | lin | 30 | | Hiss and clock bleed (BBD), tape hiss (Tape) |
| `wow` | Wow | f | 0 – 100 % | P | lin | 50 | | Slow speed drift (`depends: engine=Tape`) |
| `flutter` | Flutter | f | 0 – 100 % | P | lin | 50 | | Fast speed variation (`depends: engine=Tape`) |

### Modulation — voices and LFO

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `mod_custom` | Modulation Custom | b | off / on | off | — | — | | Off = follows the profile and the mode |
| `delay_base` | Base Delay | f | 1 – 30 ms | P | log | 100 | | Centre delay |
| `lfo_shape` | LFO Shape | e | Sine · Triangle · Rounded · Random · Log | P | — | — | | Triangle is RC-smoothed like the CE-2's (no corner in pitch); Rounded = softer; Random = smoothed random; Log = Strymon "dBucket"-style log LFO |
| `voices` | Voices | e | 1 – 6 | P | — | — | | Used by Ensemble and Doubler; fixed by the other modes |
| `voice_spread` | Voice Spread | f | 0 – 360° | P | lin | 50 | | LFO phase offset between successive voices |
| `imperfection` | Imperfection | f | 0 – 100 % | P | lin | 100 | | Analogue tolerances: phase and amplitude errors between voices, curved control voltage (string ensembles) |
| `detune_lock` | Detune Lock | b | off / on | off | — | — | | Holds the peak pitch deviation constant when Rate changes (depth shrinks as rate rises) |
| `drift` | Drift | f | 0 – 100 % | P | lin | 100 | | Slow random detuning of rate and depth between voices ("human") |
| `crossmix` | Cross-Mix | f | 0 – 100 % | P | lin | 30 | | L/R cross-mix of the complementary voices (Dimension mode) |
| `feedback` | Feedback | f | 0 – 50 % | 0 % | lin | 30 | | Feedback; above ~30 % it drifts towards a flanger |
| `sync` | Sync | e | Off · 4/1 · 2/1 · 1/1 · 1/2 · 1/4 · 1/8 · 1/16 (+ dotted / triplets) | Off | — | — | | Locks the LFO to the host tempo |

### Dynamics — envelope follower

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `env_depth` | Env → Depth | f | −100 – +100 % | 0 % | lin | 30 | | Playing harder adds (+) or removes (−) depth |
| `env_mix` | Env → Mix | f | −100 – +100 % | P | lin | 30 | | Playing harder raises (+) or ducks (−) the wet: chords stay clear on the attack, the chorus blooms on the sustain |

The envelope follower's times are set by the profile (fast for guitar, slower for voice and keys).

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
- **Log curves.** golem writes plain values; Ardour converts them through the plugin's controller, so a
  skewed range is fine as long as plain ↔ normalized is exact. Test it both ways for every parameter.
- **Enums and `map`.** The catalogue requires `map: norm` for JUCE VST3 enums; measure nih-plug before
  writing the catalogue entry.
- **Bypass.** golem disables a block through the host's bypass; nih-plug also exposes a VST3 bypass
  parameter. Pick one, and check that no click nor leftover delay-line content is heard on return.
- **Parameter count:** 6 on the front panel, 38 in expert mode (5 of them Custom switches). An order of
  magnitude, not a target.
