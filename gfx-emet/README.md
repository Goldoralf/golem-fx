# gfx-emet

> **Status: CODED** (v0.1.0, 2026-11-13) — 808 identity at 8× with an exact-ADAA soft clipper (knee 2.7 V, `headroom`, `asym`), the 808 tone network simulated from its netlist (gfx-dsp `circuit`), phase-aligned `bass_keep`; gain law +8.3 / +44.4 dB at 1 kHz as computed, aliases −120 dB in the audible band; 7 behaviour tests, bench green (28 checks; latency and click bound exempted as gfx-drive); CPU 198 ns / sample. Design reviewed by Fable 5.2 (folded in). The A / B against the Nameless Suite waits for the Windows renders.

golem's **high-headroom overdrive**. *Emet* is the word that brings a golem to life. It is a new block:
an 808-family overdrive whose clipping sits about four times higher than a Tube Screamer's. At low gain it
is a mid-voiced clean boost; turned up, it gives a "crushing mid-range push" in front of a high-gain amp,
louder and more open than a TS. gfx-drive remains golem's general drive engine; gfx-emet is one voicing
done thoroughly, with three knobs.

## 1. State of the art

Survey of 2026-11-13. **[M]** manufacturer page or manual · **[C]** DIY "compare to" clone schematic ·
**[A]** calculation from it (checked by the design review) · **[F]** forum opinion or review.

### Reference: the Fortin / Neural DSP HEXDRIVE

- **Origin** [M]: Neural DSP and Fortin designed it for the Fortin Nameless Suite. Fortin later made it
  an analog pedal.
- **Controls** [M]: LEVEL, GAIN (DRIVE in the plugin) and TONE.
  - Gain is "pretty clean" up to about 10 – 11 o'clock, then gives a "crushing mid-range push".
  - Fortin calls Tone "not a regular tone control… more like a mid-range punch control": flattest when
    off, a regular mid-boosted overdrive at half, and above that more mids and less "flabby bass".
    Neural DSP calls it "high-frequency shaping".
- **Fortin's settings** [M]: "Clean hump", "800 mod", "Tighten up the gain" (gain low, tone a little
  low: "that classic TS thang") and "Front end smash".
- **Circuit** [C]: a **TS808 topology**.
  - Input buffer, then an RC4558 clipping stage: a 1 MΩ audio-taper drive pot plus 10 kΩ in the
    feedback, and 4.7 kΩ + 47 nF to Vref, so a 720 Hz high-pass in the gain leg, exactly as in a TS.
    50 pF across the feedback.
  - Then the TS tone stage (two 220 nF, a W-taper 20 kΩ pot, 220 Ω, 1 kΩ, and a 10 kΩ bias that moves
    the first low-pass to ≈ 796 Hz), the level pot (unity at full) and an output buffer.
  - A charge pump gives ≈ 16 V; with Vref at 20/35 of the supply, the stage swings about +5.5 / −7.5 V.
- **The clipping** [C, A]: two BS170 MOSFETs back to back in the op-amp's feedback, each wired as a
  diode. Each polarity conducts through one channel (threshold about 2 V) plus the other device's body
  diode (about 0.65 V).
  - The clip sits at **about ±2.7 V**, against ±0.6 V for a TS's silicon diodes. That is the "higher
    output, additional headroom" of the description.
  - It is symmetric for matched parts. The BS170's threshold spreads from 0.8 to 3 V between devices,
    which is the likely meaning of "irregular style of clipping".
  - The small-signal gain at 1 kHz is ≈ +8.3 dB at minimum drive and ≈ +44.4 dB at maximum, peaking near
    45 dB around 1.5 kHz (47 dB is only the asymptote). A TS808 reaches ≈ 41 dB.
- **Reception** [F]: "a good Tube Screamer with more output and headroom, slightly more aggressive".
  Some builders find the analog clone "just an 808 with a poor choice of clipping diodes". The headroom
  is the point: it makes the pedal a boost-plus-drive.

### What we conclude

1. **Keep the 808 structure**: a high-pass in the gain leg, the input summed back, and a
   drive-dependent low-pass (the 50 pF). This is where a TS's tight lows and mid hump come from.
2. **Change the clipper, not the circuit**: a soft symmetric knee ≈ 13 dB above the TS diodes, exposed as
   `headroom` so the same pedal also plays a true 808 (−13 dB) or an unmatched pair (`asym`).
3. **The tone is the 808's exact network**, derived by nodal analysis, not approximated by shelves.
4. **The level reference is the hardware's**: 0 dB = the level pot fully up.

## 2. Design

```
in ─▶ × input ─▶ ↑8 ─┬─▶ gain leg: Zf / Zs (HPF 720 Hz, × Rf / 4.7 kΩ, LPF fc(gain)) (+ `tight`) ─▶ soft clip (knee = `headroom`, `asym`) ─┐
                     └──────────────────────────────── in (summed back) ──────────────────────────────────────────────────── + ◀────────┘
                     ─▶ ↓8 ─▶ tone network ─▶ × level ─▶ out
```

- **Scale.** 0 dBFS = a 1 V peak at the pedal's input. golem's reference DIs peak at −3 to −8 dBFS
  (0.4 – 0.7 V, real pickups). `input` (−12 to +12 dB) trims a hotter or weaker source.
- **Gain leg.** The 808 identity: out = in + clip(in · Zf / Zs).
  - Zs = 4.7 kΩ + 47 nF, so a 720 Hz high-pass; the gain is **Rf / 4.7 kΩ**, with no "1 +", since the
    summed input already provides it.
  - Rf = 1 MΩ · taper(`gain`) + 10 kΩ, in parallel with 50 pF: fc = 1 / (2π · Rf · 50 pF), from
    ≈ 318 kHz down to ≈ 3.15 kHz.
  - The audio taper is the pot's A-law: 10 % of the resistance at half travel.
  - The low-pass is applied before a static clipper (the Yeh simplification, as in gfx-drive). In the
    circuit the 50 pF sits across the clipper, so the corner rises while it conducts; the bench compares
    against a nonlinear one-pole reference at high gain, and the pre-low-pass is retuned if it comes out
    too dark.
- **Clipper.** `f(x) = x / sqrt(1 + (x / Vc)²)` (p = 2), with Vc = 2.7 V · 10^(`headroom` / 20).
  - It is symmetric, monotonic and soft. Its ADAA1 antiderivative is exact: F(x) = Vc · sqrt(Vc² + x²).
    The difference quotient is evaluated in f64, falling back to f((x₁ + x₀) / 2) when |x₁ − x₀| < 1e-6.
  - The first bench measures it against a BS170 + body-diode model; p is refitted if the transition is
    sharper.
  - `asym` (0 – 100 %) lowers the negative knee by up to 50 %: an unmatched MOSFET pair.
- **Oversampling: 8×.** The gfx-dsp half-band stages, cascaded three times, minimum phase. Measured:
  aliases in the audible band at −120 dB at full gain (§5).
  - The design review's simulation gives aliases of about −50 dB relative to the knee at 4×, and −63 dB
    at 8×, at full gain and full-scale input.
  - The alias bound is stated at a realistic input, −12 dBFS (§5).
- **Rails**: the doubled supply never clips with input ≤ 0 dBFS (the sum peaks near 3.7 V), so it is
  not modelled. The path is float.
- **Tone**: the exact TS tone network (component values in the source, from the clone).
  - Its transfer function H(s; `tone_stack`) comes from nodal analysis of the circuit with an ideal
    op-amp. It is recomputed at control rate on a change, and runs as a trapezoidal state-space whose
    states are the capacitor voltages, so a moving pot never jumps the state and there is no zipper.
  - The W taper is approximated by a linear law for now (to refine against the Nameless renders).
    0 % is the darkest setting; half travel gives the mid hump.
- **Level**: 0 dB = the hardware level pot fully up (unity after the tone), −24 to +12 dB. With the stage's
  own gain, `gain` 0 and `level` 0 dB give about +8 dB of mid boost.
- **Bass keep** (`bass_keep`, the shared id): below the frequency, the signal stays clean. The clean band
  is split with a Linkwitz-Riley crossover and runs through the **same oversampler and the ADAA identity
  path** as the driven band, so the sum has no ripple (the bench checks it).
- **Mono 1 / 1**, stereo 2 / 2 offered, as two independent channels.
- **Latency.** The minimum-phase oversampling adds a few samples of group delay, which is not reported:
  the gfx-drive precedent, noted against the contract's zero-latency rule.

## 3. Parameters

New block: no catalogue ids. Shared ids keep their contract meaning (`tight`, `bass_keep`). Two ids are
deliberately not the shared ones:
- `gain` is the main gain, as in gfx-drive; not `drive`, which the contract reserves for "extra
  saturation, neutral at 0".
- `tone_stack` is the 808 network; not `tone`, which is gfx-drive's −100 to +100 % tilt.

### Front panel

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `gain` | Gain | f | 0 – 100 % | 35 % | lin | 30 | ✓ | The drive pot (audio taper): clean mid boost at the bottom, a mid-range push above ~40 % |
| `tone_stack` | Tone | f | 0 – 100 % | 50 % | lin | 30 | ✓ | The 808 tone network: darkest at 0, the mid hump at half, presence above |
| `level` | Level | f | −24 – +12 dB | 0 dB | lin | 20 | ✓ | Output level; 0 dB = the hardware level pot fully up |

### Expert

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `headroom` | Headroom | f | −18 – +6 dB | 0 dB | lin | 30 | | Clip knee relative to the MOSFET pair: −13 dB ≈ a Tube Screamer's diodes |
| `asym` | Asymmetry | f | 0 – 100 % | 0 % | lin | 30 | | Lowers the negative knee (up to −50 %): an unmatched pair, even harmonics |
| `tight` | Tight | f | 20 – 1,000 Hz | 20 Hz (off) | log | 30 | | Extra high-pass in the gain leg, on top of the 720 Hz; bypassed at 20 Hz |
| `input` | Input | f | −12 – +12 dB | 0 dB | lin | 20 | | Input trim, for a source hotter or weaker than a pickup at 0 dBFS = 1 V |
| `bass_keep` | Bass Keep | f | 0 (off) – 500 Hz | 0 | log | 30 | | Below it: clean, so a bass keeps its fundamental |

The pedal face shows Gain, Tone and Level in a triangle.

## 4. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Tighten a high-gain amp (the main use) | `gain` 15 %, `tone_stack` 45 %, `level` 0 dB | Mid-voiced boost with headroom: the amp clips, not the pedal |
| Fortin "Clean hump" | `gain` 0 %, `tone_stack` 50 %, `level` 0 dB | A clean mid boost (≈ +8 dB in the mids) |
| Fortin "Tighten up the gain" | `gain` 10 %, `tone_stack` 40 %, `level` −3 dB | "That classic TS thang", with more room |
| Fortin "Front end smash" | `gain` 70 %, `tone_stack` 60 %, `level` −6 dB | Crushing mids in front of the amp |
| Classic 808 into a clean amp | `headroom` −13 dB, `gain` 50 %, `tone_stack` 50 % | The TS knee |
| Lead with presence | `gain` 55 %, `tone_stack` 75 % | Tone above half: presence back |
| Bass overdrive | `gain` 40 %, `bass_keep` 120 Hz | Fundamental kept clean, no crossover ripple |

The three Fortin-style settings become golem block presets.

## 5. Bench suite (on top of gfx-bench's common checks)

Targets are computed in the test from the circuit equations, not copied by hand.

| Check | Pass |
|---|---|
| Gain law | Small 1 kHz sine, before tone and level: +8.3 ± 0.3 dB at `gain` 0, +44.4 ± 0.5 dB at 100 % (analytic) |
| Gain-leg high-pass | At `gain` 100 %, small signal: the boosted part's response matches Zf / Zs within 0.3 dB, 50 Hz – 10 kHz |
| Knee | 1 kHz sine whose gain-leg peak equals Vc: 3rd harmonic matches the p = 2 curve's analytic value within 1 dB; 2nd < −80 dBc at `asym` 0 |
| Tone network | `tone_stack` 0 / 50 / 100 %: within 0.25 dB of the analog network (nodal analysis in the test), 20 Hz – 20 kHz |
| Aliasing | −12 dBFS 5 kHz sine at full gain: aliases folding into the audible band below −90 dB relative to the fundamental (measured −120 dB). The 5th harmonic (25 kHz) leaks to 23 kHz through the decimator's transition band at −53 dB: above 20 kHz, inaudible |
| Bass keep | `gain` 0, `bass_keep` 120 Hz, small signal: the sum is flat within 0.1 dB across the crossover |
| Nonlinear pre-LPF | Full gain: spectral centroid within 10 % of a nonlinear one-pole reference (Rf ∥ 50 pF across the clipper) |
| Determinism, CPU | Strict null; ≤ 3 % of one core at 48 kHz, mono (8×) |
| Listening | lead.wav, high-gain.wav and djent.wav into a NAM high-gain amp, against the Nameless Suite Hexdrive (Windows renders) and gfx-drive Overdrive |
