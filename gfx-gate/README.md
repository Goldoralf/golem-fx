# gfx-gate

> **Status: CODED** (v0.1.0, 2026-11-12) — processor, VST3 wrapper with key input, 13 behaviour tests, bench green at 44.1 – 192 kHz. Read-only meters (`state`, `gr`) pending nih-plug output parameters.

golem's noise gate. Stands for golem's `gate` block (today ReaGate), keeping its parameter ids and units
([golem contract](../docs/golem-contract.md)). The aim is the best gate for a live rack: metal-tight stops
on chugs, natural decays on clean notes, no chatter, no clicks, zero latency.

## 1. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference or not re-checked.

### Commercial references

| Product | What it does best | What we take |
|---|---|---|
| [ISP Decimator](https://www.effectsdatabase.com/model/isp/decimator) [V] | "Time Vector Processing": the release follows the input's envelope (program-dependent); Decimator II keeps the decay smooth on sustained notes. Algorithm undisclosed | **Follow decay**: release that tracks the note's own decay |
| [Fortin Zuul](https://www.thomannmusic.com/fortin_zuul_plus_blackout_noise_gate.htm) / [Boss NS-2](https://www.boss.info/global/products/ns-2/) [V] | **Key input**: detection on the clean guitar, gating after the drive / amp; the threshold stays valid from clean to high gain | **Key input** |
| [Fractal Axe-Fx III](https://www.fractalaudio.com/downloads/manuals/fas-guides/Fractal-Audio-Blocks-Guide.pdf) [V] | Gate and expander types, peak / RMS detector, sidechain filters, sidechain source (input, row, physical input); "Intelligent" types filter AC hum out of detection | Expander ratio, hum-aware detection, key source |
| [FabFilter Pro-G](https://fabfilter.com/help/pro-g/using/timecontrols) [V] | Styles (Classic, Clean, Guitar "following the guitar's natural decay", Vocal…), program-dependent attack / release, built-in hysteresis, soft knee, optional lookahead, zero latency without it | Gate ↔ expander, knee, program dependence |
| [Kemper](https://www.fullcompass.com/common/files/37891-PROFILERSTAGEQuickStartGuide.pdf) [V] | "Noise Gate 2:1 / 4:1": expanders before the amp for metal | Expander mode before the amp |
| [Boss NS-1X](https://www.boss.info/global/products/ns-1x/) [V] | Reduction (gentle) / Gate / Mute modes | Range: a gate does not have to close fully |
| [Steinberg Gate](https://archive.steinberg.help/cubase_pro_artist/v9/en/cubase_nuendo/topics/audio_processing_and_functions/audio_processing_noise_gate_r.html) [V] | Minimum opening time (anti-chatter) | Hold covers it |
| [ReaGate](https://www.soundonsound.com/techniques/reagate-noise-reduction) [V] | Hysteresis, pre-open (lookahead), wet / dry / noise mix | Hysteresis (golem's `hyst`) |

### Open source

| Project | Idea |
|---|---|
| [LSP Gate](https://github.com/lsp-plugins/lsp-dsp-units/blob/master/src/main/dynamics/Gate.cpp) [V] | Two curves (open, close) each with a threshold and a **zone**: a cubic Hermite knee in log / log with zero slope at both ends; a state bit picks the active curve. Peak / RMS / LPF / SMA detectors, reactivity, listen, dry / wet. The best reference |
| [CAPS Noisegate](http://quitte.de/dsp/caps.html) [V] | **Opens on the instantaneous peak, closes on RMS** below a separate threshold; a **50 / 60 Hz notch in the detector only**, so hum never holds the gate open |
| [Airwindows Gatelope](https://github.com/airwindows/airwindows/blob/master/plugins/LinuxVST/src/Gatelope/GatelopeProc.cpp) [V] | Spectral release: lows and highs close at different rates; louder hits open faster |
| [ZamGate](https://github.com/zamaudio/zam-plugins/blob/master/plugins/ZamGate/ZamGatePlugin.cpp) [V] | Windowed RMS, linear ramps, no hysteresis: the counter-example |

### Papers

- Giannoulis, Massberg & Reiss, JAES 2012: gain computer and smoothing in dB, smooth peak detectors; the
  expander is the compressor's curve mirrored [I].
- Terrell & Reiss, *Automatic Noise Gate Settings for Multitrack Drum Recordings*, DAFx 2009
  ([PDF](https://www.dafx.de/paper-archive/2009/papers/paper_60.pdf)) [V]: the best threshold sits just above
  the noise's peak level → a "learn threshold" (strings muted for a second, threshold = noise peak + 3 dB).

### What we conclude

1. **Key input** is the biggest win for metal: the gate after the amp, listening to the clean DI.
2. **Open fast, close smart**: open on the peak, close on a slower envelope, with hysteresis and hold —
   fast stops without chatter.
3. **Gate and expander are one curve**: a ratio and a knee below the threshold; a 2:1 – 4:1 expander before
   the amp sounds more natural than a hard gate.
4. **Follow decay**: a release that lets a ringing note decay naturally and clamps only once it reaches
   the noise.
5. **Hum notch in the detector**: costs nothing, lets the threshold go lower.
6. Not worth it: zero-crossing switching (variable delay of up to 12 ms on low notes), RMS-only opening
   (too slow for chugs), lookahead (latency).
7. **Tail damp** (HUSH / Decimator rack units, Gatelope): close the highs before the level, so hiss
   disappears under a decaying note instead of being chopped with it.

## 2. Our design

### Signal path

```
main in ───────────────────────────────────────────────────────▶ × gain ─▶ out
key in (optional) ─┐                                                  ▲
main in ───────────┴▶ source ▶ hum notch ▶ HPF / LPF ▶ detector ▶ gain computer ▶ ballistics ─┘
                                                     (open: peak ·  (thresh, hyst,  (attack S-curve,
                                                      close: env)    ratio, knee,    hold, release /
                                                                     range)          follow decay)
```

### Behaviour

- **Detection.** Two detectors on the filtered sidechain:
  - **open**: the instantaneous peak |x| — as fast as the pick;
  - **close**: a **sliding maximum** of |x| over one period of the lowest content, then a short smoother.
    Window = clamp(1.2 / max(`hpf`, 30 Hz), 8 ms, 40 ms): 40 ms full range (a low B at 31 Hz), 10 ms with
    `hpf` at 120 Hz (golem's Chug-tight). A fixed 5 – 10 ms follower would ripple between the peaks of a
    low note (a low E's peaks are 6 ms apart, a bass B's 16 ms) and chatter at the threshold; a
    one-period maximum is ripple-free by construction, and its only cost — closing up to one window late —
    is what Hold would add anyway. This is why raising `hpf` makes the gate **tighter**, not just less
    sensitive to rumble.
- **Curve.** Above `thresh`: unity. Below: downward expansion with `ratio` (∞ = gate) and a soft `knee`,
  floored at `range`. The close threshold is `thresh + hyst` (golem's convention, `hyst` ≤ 0).
- **States.** CLOSED → ATTACK (peak > `thresh`) → OPEN → HOLD (envelope < `thresh + hyst`) → RELEASE → CLOSED.
  A new crossing during HOLD or RELEASE resumes from the current gain; nothing ever jumps.
- **Attack**: S-curve (raised cosine), minimum 50 µs. **Release**: linear in dB.
- **Follow decay** (`follow` on): the release time adapts to how the note ended. The slope of the close
  detector (dB / s over the last 50 ms) is read when the gate starts closing: an abrupt stop (a palm mute,
  steeper than −300 dB / s) releases in `release` × 0.25; a natural decay (−20 dB / s or slower) in
  `release` × 4; log-interpolated between. Staccato is cut tight and a ringing chord fades out, with one
  setting. (Distinct from the expander: `ratio` shapes *how much*, `follow` shapes *how fast*.)
- **Tail damp** (`damp`): while closing, the first part of the gain reduction is taken from the highs: a
  one-pole low-pass slides from 20 kHz down to 1.5 kHz over the first `damp` × 12 dB of reduction, the
  broadband gain doing the rest. Hiss fades under the note before the note itself is touched.
- **Hum notch**: 50 or 60 Hz and its first three harmonics, notched in the detector only.
- **Key**: VST3 auxiliary input, on its **own bus**: the main layout stays 1 / 1 (otherwise the Golem
  Engine reads a mono + key plugin as stereo). With `source` = Key and nothing connected, the gate falls
  back to the main input (never stays closed). golem will route the DI to it: Ardour through
  `Route:add_sidechain`, REAPER through a 4-channel track and pin mappings (golem-side work, scheduled
  once gfx-gate exists).
- **Meters**: gate state (open 0 / 1) and gain reduction (dB) as **read-only VST3 parameters**; golem reads
  them through Lua (Ardour) and `TrackFX_GetParam` (REAPER). nih-plug does not expose output parameters
  today: to solve in the wrapper (see the contract).
- **Stereo**: linked detection (max of both channels), same gain on both.
- **Zero latency.** No lookahead.

## 3. Parameters

Catalogue ids and units kept; ranges a little wider. Defaults = the catalogue's.

| id | Name | Type | Range | Catalogue | Default | Curve | Smooth | Role |
|---|---|---|---|---|---|---|---|---|
| `thresh` | Threshold | f | −90 – 0 dB | −70 – −10 | −50 dB | lin | 20 ms | Opening level, peak, after the sidechain filters |
| `attack` | Attack | f | 0 – 100 ms | 0 – 50 | 3 ms | log | — | Time to open fully (S-curve) |
| `hold` | Hold | f | 0 – 1,000 ms | 0 – 500 | 20 ms | log | — | Stays open this long after the envelope falls below the close threshold |
| `release` | Release | f | 5 – 2,000 ms | 20 – 1,000 | 150 ms | log | — | Time from open to `range`, linear in dB |
| `hpf` | Sidechain Low Cut | f | 0 – 2,000 Hz | 0 – 1,000 | 0 (off) | log | 20 ms | Butterworth high-pass on detection; 0 = off |
| `lpf` | Sidechain High Cut | f | 500 – 22,000 Hz | 1,000 – 20,000 | 20,000 Hz | log | 20 ms | Butterworth low-pass on detection; ≥ 20 kHz = off |
| `hyst` | Hysteresis | f | −24 – 0 dB | −12 – 0 | 0 dB | lin | 20 ms | Close threshold = `thresh` + `hyst` |
| `range` | Range | f | −90 – 0 dB | new | −90 dB (silent) | lin | 20 ms | Level when closed |
| `ratio` | Ratio | f | 1 – 100 :1 (100 = ∞) | new | 100 (gate) | log | 20 ms | Expansion below the threshold; 2 – 4 = expander |
| `knee` | Knee | f | 0 – 24 dB | new | 0 dB | lin | 20 ms | Soft knee below the threshold |
| `follow` | Follow Decay | b | off / on | new | off | — | — | Release adapts to how the note ended (0.25× – 4×) |
| `damp` | Tail Damp | f | 0 – 100 % | new | 0 % | lin | 20 ms | Highs close before the level (hiss under decaying notes) |
| `hum` | Hum Filter | e | Off · 50 Hz · 60 Hz | new | Off | — | — | Notches mains hum out of detection |
| `source` | Detector Source | e | Main · Key | new | Main | — | — | Key = the auxiliary input |
| `listen` | Listen | b | off / on | new | off | — | — | Outputs the filtered sidechain, for setting `hpf` / `lpf` / `hum` |

Defaults keep golem's presets sounding like a gate (ratio ∞, knee 0, follow off).

Read-only (meters, see §2): `state` (open 0 / 1) and `gr` (gain reduction, dB).

**On golem's side** (agreed with the rack session, scheduled once gfx-gate exists): key routing on both
engines; a "learn threshold" UI action (mute the strings, measure the noise peak for 1 s, write
`thresh` = peak + 3 dB); the gate-state and gain-reduction display, from the read-only parameters.

**To check on the bench (contract):** `thresh`, `hyst` and `range` have negative minimums; check that
REAPER and Ardour write them exactly (plain ↔ normalized, both ways).

## 4. Applications

What each feature is for. The bench renders one file per row.

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Metal rhythm, gate before the amp | thresh −38 dB, attack 0.5 ms, hold 8 ms, release 60 ms, `hpf` 120 Hz, hyst −6 dB (golem's Chug-tight), `follow` on | One-period close detector + `hpf` = tight stops; follow keeps held chords |
| Metal, gate **after** the amp keyed from the DI | `source` Key, thresh −45 dB, release 40 ms, `range` −90 dB | Key input: the amp's hiss is gated on the clean guitar's timing; the threshold does not move with the gain |
| High-gain lead, long sustain | ratio 3:1, knee 12 dB, release 300 ms, `follow` on, `damp` 60 % | Expander + tail damp: the note's end is never chopped, hiss fades first |
| Single coils, mains hum | `hum` 50 or 60 Hz, thresh 6 dB lower than without | Hum notch in the detector |
| Clean / funk, natural | ratio 2:1, `range` −20 dB, attack 3 ms, release 250 ms | Expander and range: never fully closed |
| Bass | thresh −55 dB, attack 5 ms, hold 50 ms, release 250 ms, `hpf` 30 Hz, `lpf` 8 kHz | 40 ms close window: no chatter on a low B |
| Voice, live mic | thresh −48 dB, hold 100 ms, release 300 ms, `hpf` 100 Hz, `lpf` 12 kHz, `range` −15 dB | Range: bleed reduced, not removed (no pumping ambience) |
| Setting the detector by ear | `listen` on | Hear what opens the gate |

## 5. Bench suite (on top of gfx-bench's common checks)

| Check | Signal | Pass |
|---|---|---|
| Opening time | sine step, below → above threshold | reaches −0.1 dB of the input in Attack ± 1 sample |
| Hold + Release time | sine step, above → silence | starts closing after Hold; reaches `range` after Release ± 1 sample |
| Hysteresis | slow level ramp up then down | opens at `thresh`, closes at `thresh + hyst` (± 0.1 dB) |
| No chatter | decaying low E (82 Hz) and low B (31 Hz), hold 0, hyst 0 | a single open → close sequence |
| Close delay | sine burst ending abruptly, `hpf` 0 and 120 Hz | closing starts ≤ one detector window after the last peak (40 ms / 10 ms) |
| Tail damp | hiss at −50 dBFS under a decaying note, `damp` 100 % | hiss above 4 kHz falls ≥ 10 dB before the note's level falls 3 dB |
| Clicks | sine bursts at Attack 0 and Release 5 ms | no energy above 10 kHz beyond the sine's own spectrum + a bound |
| Expander curve | slow ramp, ratio 2 and 4, knee 0 and 12 | static curve ± 0.1 dB |
| Follow decay | clean.wav held last chord | the decay is not cut before it reaches the noise |
| Hum | 50 Hz hum at −45 dBFS + silence, `hum` 50 Hz, threshold −50 | stays closed |
| Sidechain filters | sine steps at 40 Hz with `hpf` 120 Hz | does not open |
| Key | key = DI, main = DI through a distortion | opens and closes on the DI's timing |
| Range | above, then below threshold, `range` −30 dB | closed level = −30 dB ± 0.1 |
| Stereo link | signal on L only | same gain on L and R |
| Chug test | high-gain.wav and djent.wav, golem preset Chug-tight | no false openings in rests; renders kept for listening |

Listening is done in context, gate before a high-gain amp (and after it, keyed from the DI), through
golem; with golem's five gate presets: Standard, Soft, Chug-tight, Bass gate, Vocal gate.
