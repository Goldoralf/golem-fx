# gfx-flanger

> **Status: PROPOSED** — design, no code yet.

golem's flanger. Stands for golem's `flanger-js` block (today REAPER's JS `guitar/flanger`), keeping its
parameter ids and units ([golem contract](../docs/golem-contract.md)). Built on the same modulated-delay
and BBD core as gfx-chorus (gfx-dsp). The aim: every classic flanger voice — Electric Mistress, MXR,
A/DA's huge sweep, tape through-zero, barberpole — smooth under an EXP pedal, zero latency by default.

## 1. The block being replaced

REAPER's JS `guitar/flanger` (LGPL, read on this machine): the delay line is read at **whole samples**
(the sweep steps: zipper), the delay follows `(sin + 1) · (length / 2 − 1)` so the sweep always runs from
~0 to `length`, feedback is positive only, one mono LFO.

## 2. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference; **[U]** not checked.

### References

| Reference | What it does best | What we take |
|---|---|---|
| [Strymon Mobius](https://www.strymon.net/manuals/Mobius_UserManual_REVF.pdf) [V] | Silver (Electric Mistress), Grey (MXR, **log LFO** that lingers high), Black ± (very wide, ± regen), Zero ± (through-zero, ± regen; MANUAL sets how far past zero) — all on a BBD model | Mode list, regen polarity, manual |
| [UA / Brainworx A/DA Flanger](https://help.uaudio.com/hc/en-us/articles/33531569312788) [V] | 0.4 – 14 ms: a **35:1** sweep | A huge-sweep mode |
| [Arturia Flanger BL-20](https://www.arturia.com/products/software-effects/flanger-bl-20/overview) [V] | BBD; through-zero by delaying the dry; LFO, envelope and manual sweep sources **usable together**; input HPF; stereo offset | Summed sweep sources |
| [Eventide Instant Flanger Mk II](https://www.musicradar.com/news/eventide-brings-back-the-instant-flanger-with-mk-ii-plugin) [V] | LFO + envelope + manual; "pseudo through-zero" by phase-shifting the dry ([Eventide](https://www.eventideaudio.com/?p=184730)) | A zero-latency pseudo-TZ |
| [Softube Fix Flanger](https://softube.com/manuals/fix-flanger) [V] | TZ recipe: mix 50 %, delay polarity negative, regen off → full cancellation at 0 ms | TZ default voicing |
| [LSP Flanger](https://lsp-plug.in/?page=manuals&section=flanger_stereo) [V] | Min depth, depth, phase invert, feedback drive, extra feedback delay, half-period LFO modes with crossfade ("endlessly receding") | Loop drive, barberpole by dual ramps |
| [Airwindows Flanger](https://www.airwindows.com/flanger/) [V] | Through-zero, brightening to offset interpolation dulling | HF compensation on the Clean engine |
| [Surge Flanger](https://surge-synthesizer.github.io/manual-xt/) [V] (GPL-3) | 1 – 4 combs tuned in pitch (Base Pitch, Spacing), LF damping | Comb tuning in pitch |
| Tycobrahe Pedalflanger [U] | A rocker pedal drives the sweep | Manual sweep for the EXP pedal |

### Papers

- Esqueda, Välimäki & Parker, *Barberpole Phasing and Flanging Illusions*, DAFx 2015
  ([DAFx](https://dafx.de/paper-archive/details/ZoTfxK0W97ZkYnrb5Pjdog)) [V]: notch cascade, **synchronised
  dual flanger** (crossfaded sawtooth ramps), or SSB modulation.
- Huovilainen, *Enhanced Digital Models for Analog Modulation Effects*, DAFx 2005 [V, abstract].
- Comb maths [V]: delay τ, same polarity → notches at (2k + 1) / 2τ; inverted → at k / τ. 1 ms puts the first
  notch at 500 Hz, 0.1 ms at 5 kHz. Sweeping τ **exponentially** moves the notches evenly in pitch.

### What we conclude

1. Fractional delay (Lagrange 5 / sinc, from gfx-chorus), exponential sweep between a **manual** minimum
   and `length`.
2. **Through-zero** needs the dry delayed by d0 (0.3 – 2 ms): opt-in and **reported** as latency. A
   zero-latency **pseudo-TZ** (allpass on the dry) for the default path.
3. **Negative feedback** as a polarity switch; **saturation and filters in the loop** so high regen stays
   safe and voiced.
4. **Sweep sources** LFO / envelope / manual, combinable; manual = the EXP pedal's target.
5. **Barberpole** by the synchronised dual flanger.

## 3. Design

- Engines: **Clean** (interpolated, HF-compensated), **BBD** (clock-domain model + BLEP, companding and
  clipping in the loop, from gfx-chorus), **Tape TZ** (two lines, dry delayed by `tz_offset`, reported
  latency), **Pseudo TZ** (allpass on the dry, zero latency).
- Sweep: delay = manual · (length / manual)^u, u ∈ [0, 1] from the summed sources, × depth.
- The LFO runs freely: parameter changes never reset its phase (the JS resets it on any change — not
  copied); phase 0 at reset; synced from the host's position when `sync` is on.

| `mode` | Engine | LFO | Sweep | Note |
|---|---|---|---|---|
| Mistress | BBD | smoothed triangle | 0.5 – 8 ms | Electric Mistress, Mobius Silver |
| Grey | BBD | log | 0.3 – 6 ms | MXR M117, Mobius Grey |
| A/DA | BBD | triangle | 0.4 – 14 ms (35:1) | Huge sweep |
| Tape TZ+ / TZ− | Tape TZ | sine | through zero | regen +/−; reported latency `tz_offset` |
| Pseudo TZ | Pseudo TZ | sine | near zero | zero latency |
| Barberpole ↑ / ↓ | Clean | dual ramps | endless | Esqueda dual flanger |
| Pedal | any | none | manual only | Tycobrahe: the EXP pedal sweeps |

## 4. Parameters

Catalogue ids kept (`feedback` stays a magnitude in dB; its polarity is a separate switch).

| id | Name | Type | Range | Catalogue | Default | EXP | Role |
|---|---|---|---|---|---|---|---|
| `length` | Length | f | 0.1 – 30 ms | 0 – 30 | 6 ms | | Maximum delay of the sweep |
| `feedback` | Feedback | f | −60 – −1 dB | −60 – −6 | −24 dB | ✓ | Above −6 dB only with `drive` > 0 [to settle] |
| `wet` | Wet | f | −60 – 0 dB | same | −10 dB | | |
| `dry` | Dry | f | −60 – 0 dB | same | 0 dB | | |
| `rate` | Rate | f | 0.05 – 5 Hz | 0.05 – 2 | 0.3 Hz | ✓ | |
| `mode` | Mode | e | Mistress · Grey · A/DA · Tape TZ+ · Tape TZ− · Pseudo TZ · Barberpole ↑ · Barberpole ↓ · Pedal | new | Mistress | | |
| `manual` | Manual | f | 0.05 – 30 ms | new | 0.1 ms | ✓ | Minimum delay; the EXP target |
| `depth` | Depth | f | 0 – 100 % | new | 100 % | ✓ | Exponential span between manual and length |
| `fb_polarity` | Feedback Polarity | e | + · − | new | + | | |
| `shape` | LFO Shape | e | Sine · Triangle · Log · Exp · Ramp Up · Ramp Down · Random | new | per mode | | Random = smoothed S&H, fixed seed |
| `sweep_src` | Sweep Source | e | LFO · Envelope · Manual · LFO + Envelope | new | LFO | | |
| `env_sens` | Envelope Sensitivity | f | −40 – 0 dB | new | −20 dB | | |
| `env_att` | Envelope Attack | f | 1 – 100 ms | new | 10 ms | | |
| `env_rel` | Envelope Release | f | 20 – 2,000 ms | new | 300 ms | | |
| `stereo_phase` | Stereo Phase | f | 0 – 180° | new | 90° | | |
| `sync` | Sync | e | Off · 4 bars … 1/16 (+ dotted / triplets) | new | Off | | Phase from the host's position |
| `drive` | Loop Drive | f | 0 – 100 % | new | 0 % | | Saturation in the feedback loop |
| `fb_hpf` | Loop Low Cut | f | 20 – 500 Hz | new | 20 Hz | | |
| `fb_lpf` | Loop High Cut | f | 1 – 20 kHz | new | 20 kHz | | |
| `bass_mono` | Bass Mono Below | f | 0 (off) – 300 Hz | new | 0 | | Lows stay dry and mono |
| `tz_offset` | TZ Offset | f | 0.3 – 2 ms | new | 0.5 ms | | Dry delay in Tape TZ modes = reported latency |

## 5. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| No zipper | slow sweep on a sine: no stepping (the JS defect) |
| Notch positions | at a fixed delay τ, notches at (2k + 1) / 2τ (+) or k / τ (−) within 1 % |
| Through-zero | Tape TZ−, mix 50 %, regen off: full cancellation at the zero crossing (≤ −60 dB) |
| Latency | 0 except Tape TZ = `tz_offset` exactly |
| LFO continuity | moving any parameter never jumps the LFO phase |
| Loop | feedback −1 dB with drive: bounded, no runaway |
| Barberpole | notches move in one direction for 30 s, no audible reset |
