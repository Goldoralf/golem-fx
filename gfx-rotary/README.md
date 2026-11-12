# gfx-rotary

> **Status: PROPOSED** — design, no code yet.

golem's rotary speaker. Stands for golem's `rotary` block (today Calf Rotary Speaker;
`golem_rotary.jsfx` on Windows / macOS), keeping its parameter ids and units
([golem contract](../docs/golem-contract.md)). For guitar and keys; golem's organ keeps setBfree's own
Leslie. The aim: the slow ↔ fast transition that *is* the sound — horn and drum on their own inertia —
plus directivity, cabinet reflections, mics and tube drive, zero latency, mono in / stereo out.

## 1. What golem has today

`golem_rotary.jsfx` copies Calf's design [V]: horn = 3 taps with AM and a 2 kHz band-pass; drum = 1 tap,
¼ of the Doppler, AM, 800 Hz low-pass; an `atan` input stage. Speeds horn 48 → 400 rpm, drum 40 → 342 rpm.
Ramps are **linear**, computed once per block: horn ~2 s up / 1 s down, drum ~7 s / 5 s. Presets: Leslie
slow (mic 70, spacing 50, depth 60), Leslie fast (80, 60, 75).

## 2. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference; **[U]** forum, retailer or not
checked.

### References

| Reference | Facts | What we take |
|---|---|---|
| Leslie 122 / 147 ([measured](https://www.diyaudio.com/community/threads/leslie-speaker-spin-speed.277275/)) [V] | Horn 48 rpm (0.80 Hz) / 400 rpm (6.67 Hz); drum 40 rpm (0.67 Hz) / 342 rpm (5.7 Hz); spread between units; ~800 Hz crossover; the amp growls when pushed ([DAFx 2002](https://ccrma.stanford.edu/~jos/doppler/dafx02.pdf)) | Default speeds, drive |
| [setBfree whirl.c](https://github.com/pantherb/setBfree/blob/master/b_whirl/whirl.c) (GPL-2+) [V] | **Exponential** speed changes: horn τ 0.16 s up / 0.32 s down, drum τ 4.1 s up / 1.4 s down (the drum brakes faster than it speeds up); path-length Doppler; **angle-indexed short FIRs** for directivity; 6 reflection taps per rotor; geometry (mic 42 cm, horn radius 19.2 cm, drum 22 cm) | Inertia constants, directivity tables, reflections, geometry |
| Herrera, Hanson & Abel, *Discrete Time Emulation of the Leslie Speaker*, AES 127 (2009) ([CCRMA](https://ccrma.stanford.edu/papers/view/2568)) [V, abstract] | Rotor speed as first-order systems with **different accelerate and decelerate constants**; responses tabulated per angle | Confirms the motor model |
| Smith, Serafin, Abel & Berners, *Doppler Simulation and the Leslie*, DAFx 2002 ([PDF](https://ccrma.stanford.edu/~jos/doppler/dafx02.pdf)) [V] | Doppler as a variable delay; each path (direct, reflections) its own time-varying filter; horn nearly omnidirectional thanks to its diffuser | Per-path filters |
| [Strymon Lex](https://www.strymon.net/manuals/Lex_UserManual_RevD.pdf) [V] | Fast speed, mic distance (stereo mics on the horn, one on the drum), preamp drive, horn level; slow speed, cab direction (front / rear), acceleration; **hold the switch = brake** | Brake on hold, cab direction |
| [Neo Ventilator II](https://www.soundonsound.com/reviews/neo-instruments-ventilator-ii-mini-vent) [V/U] | Fast speeds adjustable (drum 2.6 – 7.8 Hz, horn 2.7 – 8.2 Hz); balance, acceleration, drive, mic distance | Speed ranges |
| GSi VB3 / GSiRotary [U] | 122 / 147 / 142 / 145 / 251; tube overdrive and feedback; **"Front Stop" brake with an adjustable stop position** | Stop angle |
| Fender Vibratone [U] | One rotating drum in front of a 10" speaker, no horn, ~40 / 340 rpm: the guitar rotary | A Vibratone cabinet |
| [Surge XT RotarySpeaker](https://github.com/surge-synthesizer/sst-effects/blob/main/include/sst/effects/RotarySpeaker.h) (GPL-3) [V] | Drive before the crossover; **sub-bass under 200 Hz mono and unmodulated** | Mono sub-bass |

### What we conclude

1. **Inertia**: exponential per rotor, separate accelerate / decelerate, drum much slower. The horn
   reaching full speed while the drum still lags *is* the signature; linear ramps sound mechanical.
2. **Brake** that coasts, with an optional stop angle (a timbre choice).
3. **Life**: slight deterministic speed drift; rotors never phase-locked.
4. **Directivity, not just AM**: angle-dependent filtering per path.
5. **Cabinet** reflections, **mics** (distance, angle), **drive**, **mono sub-bass**.
6. Skipped: long IR convolution and dense measured FIRs (latency, CPU), tempo sync (a rotor is a motor),
   organ-specific Leslie (setBfree), bi-amp output.

## 3. Design

```
mono in ▶ trim ▶ tube stage (ADAA, sag) ▶ crossover 800 Hz ─┬▶ horn: delay line ▶ direct + 3 reflections per mic, each with an angle-indexed filter ─┐
                                    (sub-bass < 150 Hz mono, unmodulated) └▶ drum: delay line ▶ taps, stronger AM, low-pass ─────────────────┴▶ cabinet EQ ▶ level / mix ▶ L R
```

- **Motor**: ω += (ω_target − ω) · k, with k_up / k_down per rotor, per sample; angle integrated in f64.
  Counter-rotation option. Drift: filtered noise from a fixed seed.
- **Reset**: horn at 0, drum at 0.37 turn, both at the slow speed, fixed seed — deterministic.
- **Paths**: tap delays from the geometry (radius, mic distance and angle) / speed of sound; per-tap
  tilt / gain interpolated from a 64-entry angle table fitted to the DAFx 2002 / setBfree curves.
- **Cabinets**: 122, 147, 145, Vibratone (drum only, full range), 122 rear — voicing, radii, EQ (the 122 rolls
  off at 6 kHz), drive curve.
- **Brake**: `speed` = stop cuts the motor; the rotor coasts with its decelerate constant; below 0.1 Hz the
  horn settles at `stop_angle`.
- `depth` and `spacing` are smoothed over ~50 ms (their jumps move read pointers).
- Buffers sized at initialisation for 192 kHz. Zero latency.

## 4. Parameters

Catalogue ids kept.

| id | Name | Type | Range | Catalogue | Default | EXP | Role |
|---|---|---|---|---|---|---|---|
| `speed` | Speed | e | Stop · Slow · Fast | same order | Slow | | The footswitch; the ramp is the plugin's, never smoothed by the host |
| `mic` | Mic Distance | f | 0 – 100 % | same | 70 % | | Close = dry, aggressive, more AM; far = more reflections |
| `spacing` | Mic Spacing | f | 0 – 100 % | same | 50 % | | Horn mic angle 0 – 180°; 0 = exactly mono |
| `depth` | Depth | f | 0 – 100 % | same | 60 % | | Doppler and AM together (100 % ≈ 1.5 × real geometry) |
| `cabinet` | Cabinet | e | 122 · 147 · 145 · Vibratone · 122 Rear | new | 122 | | |
| `ramp` | Ramp | f | 0 – 400 % | new | 100 % | ✓ | Scales all four time constants |
| `drive` | Drive | f | 0 – 100 % | new | 20 % | ✓ | Tube preamp |
| `balance` | Horn / Drum | f | 0 – 100 % | new | 50 % | | |
| `horn_slow` | Horn Slow | f | 0.3 – 2 Hz | new | 0.80 Hz | | |
| `horn_fast` | Horn Fast | f | 3 – 9 Hz | new | 6.7 Hz | | |
| `drum_slow` | Drum Slow | f | 0.3 – 2 Hz | new | 0.67 Hz | | |
| `drum_fast` | Drum Fast | f | 2.5 – 8 Hz | new | 5.7 Hz | | |
| `horn_accel` | Horn Accel | f | 0.05 – 3 s | new | 0.16 s | | Time constant |
| `horn_decel` | Horn Decel | f | 0.05 – 3 s | new | 0.32 s | | |
| `drum_accel` | Drum Accel | f | 0.2 – 12 s | new | 4.1 s | | |
| `drum_decel` | Drum Decel | f | 0.2 – 12 s | new | 1.4 s | | |
| `brake` | Brake | f | 0 – 100 % | new | 50 % | | Coast time on stop; 0 = abrupt |
| `stop_angle` | Stop Angle | f | 0 – 360° | new | 0° | | Where the horn parks |
| `counter` | Counter-rotate | b | off / on | new | on | | Horn and drum in opposite directions |
| `wobble` | Wobble | f | 0 – 100 % | new | 30 % | | Speed drift and flutter |
| `sub_mono` | Sub Mono | f | 0 (off) – 200 Hz | new | 150 Hz | | Unmodulated mono sub-bass |
| `level` | Level | f | −24 – +12 dB | new | 0 dB | | Check plain ↔ normalized on REAPER (negative minimum) |
| `mix` | Mix | f | 0 – 100 % | new | 100 % | ✓ | Parallel use on guitar |

## 5. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| Speeds | settled horn / drum rates within 1 % of the setting (from the AM period) |
| Inertia | slow → fast: horn reaches 63 % in `horn_accel` ± 5 %, drum in `drum_accel` ± 5 % |
| Brake | stop: coasts with the decelerate constant, parks at `stop_angle` |
| Mono | `spacing` 0: L = R; sub-bass below `sub_mono` unmodulated |
| Determinism | strict null between two renders and between hosts |
| No jumps | `depth` / `spacing` swept: no click |
| CPU | < 1 % of one core at 48 kHz |
| Listening | clean.wav, lead-clean.wav and keys (rendered) with golem's two presets, against Calf and setBfree's Leslie |
