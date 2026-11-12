# gfx-boost

> **Status: PROPOSED** — design, no code yet.

golem's gain utility. Stands for two golem blocks that share the id `gain`
([golem contract](../docs/golem-contract.md)): `volume` (ReaPlugs Volume, −60 … +6 dB) and `clean-boost`
(JS Volume Adjustment, 0 … +12 dB). One plugin, `gain` −60 … +12 dB.

## 1. State of the art

A gain stage has no sound of its own to design; "best in class" means it never makes a sound of its own
either. What the good ones get right ([I], common practice in DAW utility plugins: REAPER's JS Volume,
Ardour's a-Amplifier, FabFilter / TDR utilities):

- **Bit-transparent at 0 dB**: gain exactly 1.0, the output equals the input.
- **No zipper noise** under an EXP pedal: the gain is smoothed, in dB, over a few ms.
- **A real silence** at the bottom of the range when used as a volume pedal.
- **A useful taper** for a volume pedal (most of the travel in the musical range, not in −60 … −30 dB).

Colour (a "clean boost" that adds some drive, like an EP Booster) belongs to gfx-drive, not here.

## 2. Design

| id | Name | Type | Range | Catalogue | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|---|
| `gain` | Gain | f | −60 – +12 dB | volume: −60 – +6 · clean-boost: 0 – +12 | 0 dB | lin (dB) | 10 ms, in dB | ✓ | −60 dB = silence (−∞) |
| `polarity` | Polarity | b | normal / inverted | new | normal | — | 5 ms crossfade | | Fixes a phase problem between two lanes |

- Gain = 1.0 exactly at 0 dB, and exact silence at −60 dB (the bottom of the range is −∞, not −60).
- Smoothing in dB, 10 ms: an EXP sweep never steps, a scene change never clicks.
- The volume-pedal taper is golem's (the EXP mapping lives in golem's pedal settings); the plugin's
  curve stays linear in dB so a written value lands exactly.
- Mono first, stereo second; the same gain on both channels.

## 3. Applications

| Use | Settings | Feature that matters |
|---|---|---|
| Volume pedal | EXP on `gain`, −60 → 0 dB (golem's taper) | Smoothed in dB: no zipper; true silence at the heel |
| Solo boost on a footswitch / scene | `gain` +4 dB after the amp | Click-free |
| Push the front of an amp | `gain` +6 … +12 dB before the amp | (For a coloured boost, gfx-drive) |
| Level-match two scenes or lanes | `gain` ± a few dB | Exact dB |
| Fix a phase cancellation between two lanes | `polarity` inverted | |
| Swells | EXP on `gain`, before delay / reverb | |

## 4. Bench suite

| Check | Pass |
|---|---|
| 0 dB | bit-identical output |
| −60 dB | exact zeros |
| Any value | output / input = 10^(gain / 20) ± 0.001 dB once settled |
| EXP sweep −60 → +12 dB in 200 ms on a sine | no step: HF energy below a bound |
