# gfx-irfx

> **Status: CODED** (v0.1.0, 2026-11-15). Designed with gfx-cab: the design, its review and the measured
> figures are in [gfx-cab/README.md](../gfx-cab/README.md) (§2.8 for this crate, "As coded" for the
> numbers). This page only lists the parameters.

golem's **stereo IR effect**: it stands for golem's `ir-fx` block (springs, echoes and reverb boxes
captured as impulse responses) and keeps its ids `wet` and `dry`. Same engine as gfx-cab (gfx-dsp
`convolve`, `ir`, `ir_host`, `ir_engine`, `cuts`), one IR per instance, class limit 20 s, 2 / 2 first,
zero latency. The IR comes through the slot parameter `ir_a` and golem's index (gfx-cab §2.1).

| id | Name | Type | Range | Default | Role |
|---|---|---|---|---|---|
| `wet` | Wet | f | −60 – +6 dB (−60 = off) | −6 dB | Effect level (catalogue id); with Loudness, the effect's level against the dry |
| `dry` | Dry | f | −60 – +6 dB (−60 = off) | 0 dB | Insert dry, at unity (catalogue id) |
| `lowcut` | Low Cut | f | 10 – 1,000 Hz | 10 Hz (off) | Thin a spring's boom |
| `highcut` | High Cut | f | 1,000 – 22,000 Hz | 22,000 Hz (off) | Darken the tail |
| `ir_a` | IR | int | 0 – 65,535 (0 = none) | 0 | Slot of the IR in golem's index. Set by golem's picker, not a knob |
| `predelay` | Predelay | f | 0 – 250 ms | 0 ms | Gap before the effect |
| `length` | Length | f | 10 – 20,000 ms | 20,000 ms (whole) | Shorten a tail; ≥ the IR's length = untouched |
| `fade` | Fade | f | 0 – 100 % | 10 % | Half-Hann fade over the end of a shortened IR |
| `normalize` | Normalize | e | Off · Peak · Loudness | Loudness | Level match: pink noise 60 Hz – 12 kHz at 0 dB; Off = the file as it is |
| `reverse` | Reverse | b | off / on | off | Reversed IR: swells |
| `width` | Width | f | 0 – 200 % | 100 % | Mid / side on the wet; 100 % = untouched |
| `lowcut_slope` | Low Cut Slope | e | 12 dB · 24 dB | 12 dB | As gfx-eq |
| `highcut_slope` | High Cut Slope | e | 12 dB · 24 dB | 12 dB | As gfx-eq |
| `trails` | Trails | b | off / on | on | Bypass lets the tail ring |
| `bypass` | Bypass | b | off / on | off | `kIsBypass` |
| `loaded_a` | Loaded | f, read-only | −2 – 65,535 | — | The slot once installed; −1 missing, −2 unreadable or hash mismatch |
