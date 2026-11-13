# gfx-shem

> **Status: CODED** (v0.1.0, 2026-11-13) — the Grind network simulated from its netlist (gfx-dsp `circuit`), within 0.25 dB of the analog response, Grind = the plateau gain, Focused / Full crossfaded, coupling capacitors replaced by a 5 Hz DC blocker (their 2.5 Hz resonance rang); 7 behaviour tests, bench green (28 checks); CPU 116 ns / sample. Design reviewed by Fable 5.2 (folded in). The A / B against the Nameless Suite waits for the Windows renders.

golem's **tight boost** for high-gain amps. *Shem* is the sacred name slipped into a golem's mouth to set
it moving. It is a new block: a clean, frequency-shaped boost of up to +20 dB, with no clipping of its
own. It cuts the bass the amp would turn to mud and lifts the presence the amp would bury, so that every
note stays defined under heavy distortion. All the grind is the amp's; Shem decides what reaches it.
gfx-boost stays the neutral volume and boost.

## 1. State of the art

Survey of 2026-11-13. **[M]** manufacturer page or manual · **[C]** DIY "compare to" clone schematic ·
**[A]** nodal analysis of that schematic (the design review's, ideal op-amp; the test recomputes it) ·
**[F]** forum opinion.

### Reference: the Fortin GRIND (and its sibling, the 33)

- **Controls** [M]: one GRIND knob, "the EQ shaper". Fortin's manual: "the overall level control gives
  you up to +20 dB of boost". A side jack switches the amp's channel. Fortin describes the travel:
  - low: "pushed mids feel, tight bass… almost feels like the gain has reduced";
  - middle: "slightly flatter, tight bass, a little more scooped";
  - high: "more scooped, but tightening up the bass… feels like there is more gain".
  Neural DSP's Nameless Suite calls the knob the booster's output level.
- **Placement** [M]: in front of an amp that already has gain. Set the amp first; you "need much less
  gain than you thought".
- **Circuit** [C]: a clean op-amp boost (a transistor-input composite non-inverting stage) on ≈ 30 V
  rails. There are **no clipping diodes**: it is a fixed filter with gain, followed by a **passive level
  pot** (A5K between two 220 Ω pads). **That pot is the Grind knob.** The described travel is the amp
  reacting to more level through a fixed EQ.
  - A switch picks one of two shunt arms: 2.7 kΩ + 100 nF, or (9.1 kΩ + 2.7 kΩ) + 100 nF.
  - The feedback is 220 kΩ in parallel with (16 kΩ + 10 nF) and a T-network (2.2 kΩ, 160 Ω ∥ 47 nF,
    3.9 kΩ, with 20 kΩ ∥ 470 nF to ground). The T-network is three-terminal: it loads the shunt node, so
    the simple 1 + Zf / Zs form does not hold. The network needs nodal analysis.
- **Response** [A], before the level pot:

  | Arm | 100 Hz | 300 Hz | 1 kHz | 4 kHz and up |
  |---|---|---|---|---|
  | 2.7 kΩ | +3.7 dB | +12.3 dB | +22.5 dB | +20.5 dB |
  | 11.8 kΩ | +4.1 dB | +10.8 dB | +18.5 dB | +15.9 dB |

  - The +20 dB plateau matches Fortin's "+20 dB".
  - In the audio band the network is order 3: a real pole at 588 Hz, a complex pair at 754 Hz
    (Q ≈ 0.86), and zeros near 112, 334 and 948 Hz. It gives a ≈ 2 dB hump around 1 kHz.
  - The coupling capacitors add two poles at 2.45 Hz.
- **Lineage** [F]: the 33 (Fredrik Thordendal's signature) and the Grind derive from the TC Electronic
  Integrated Preamp with its EQ fixed. They reportedly differ by one component. Which arm is the Grind and
  which the 33 is disputed among builders.

### What we conclude

1. **No saturation inside.** A clean float boost through the fixed network; the amp does the rest.
2. **The knob is the level.** Faithful to the circuit, and so the travel feels the way Fortin describes
   it, because the amp reacts to it.
3. **The network is computed from the circuit** (nodal analysis), not fitted: its two arms are the two
   voices.

## 2. Design

```
in ─▶ network (voice) ─▶ × grind ─▶ × bright (presence trim) ─▶ out
```

- **The network, from the circuit.** The schematic is entered as a small netlist (component values in the
  source) and solved by modified nodal analysis with an ideal op-amp. The capacitors are discretized with
  the trapezoidal rule (companion models), which gives a discrete state-space x[n+1] = A x + B u,
  y = C x + D u, with the capacitor voltages as states.
  - The matrices are computed in `initialize` (they depend only on the sample rate) for both arms.
  - The two coupling capacitors (1 µF, 4.7 µF) are modelled as shorts, and a first-order DC blocker at
    5 Hz stands for them. With the feedback they form a +7 dB resonance at ≈ 2.5 Hz that rang for
    seconds after every note (measured in the first build); the design review had advised not to model
    it. Above 20 Hz the response is unchanged.
  - Trapezoidal integration is within 0.1 dB of the analog response up to 20 kHz at 44.1 kHz, since every
    feature sits below 1 kHz and the response is flat above 3 kHz.
  - A small gfx-dsp module (`circuit.rs`) does this; gfx-emet's tone network uses it too.
- **Voice.** Both arms run all the time; `voice` crossfades their outputs over 30 ms, so switching never
  clicks. The two cost a few dozen multiplies per sample.
- **Grind** (`grind`): the level pot, as gain at the network's plateau, in dB, from −8 to +20 dB.
  - The A5K pot between its 220 Ω pads spans ≈ −28 to −0.4 dB. We keep the top part of that travel, the
    range players use, and express it in real units.
  - The default, +10 dB, is a deliberate insert level: loud enough to tighten an amp, not a jump.
  - With Full, the same knob gives ≈ 4.6 dB less at the plateau, as the circuit does.
- **Bright** (`bright`): a first-order high shelf at 3 kHz, ±6 dB, on top of the network (0 = the
  circuit's own response).
- **No clipping, no oversampling**: the plugin is linear. Its output can exceed 0 dBFS, as a boost's
  should; the amp after it handles that.
- **Mono 1 / 1**, stereo 2 / 2 offered. Zero latency.

## 3. Parameters

New block: no catalogue ids. `voice` follows the shared `model` / `mode` / `voice` naming (a voicing).

### Front panel

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `grind` | Grind | f | −8 – +20 dB | +10 dB | lin | 20 | ✓ | The level pot: boost at the network's plateau. More of it pushes the amp harder: tighter, "more gain" |

### Expert

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `voice` | Voice | e | Focused · Full | Focused | — | 30 (crossfade) | | The shunt arm: Focused (2.7 kΩ) cuts more bass and boosts more; Full (11.8 kΩ) is ≈ 4.6 dB lower at the plateau |
| `bright` | Bright | f | −6 – +6 dB | 0 dB | lin | 30 | | High shelf at 3 kHz, on top of the network |

The pedal face shows Grind as a single large knob.

## 4. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Tight boost in front of a high-gain amp (the reason it exists) | `grind` +10 dB, amp gain lowered | Bass cut and presence lift: every note defined |
| Drop tunings, 7 and 8 strings | `grind` +14 dB, `voice` Focused | More level through the network: the low strings stay tight |
| Pushed mids, less apparent gain | `grind` +2 dB | Fortin's "low" travel: the EQ is heard, the amp is barely pushed |
| Lead boost on the same channel | `grind` +6 dB on a footswitch | Louder and more present without more mud |
| Too fizzy amp | `bright` −3 dB | Presence trimmed, tightness kept |

## 5. Bench suite (on top of gfx-bench's common checks)

Targets come from the analog network, computed in the test by nodal analysis (no hand-copied numbers).

| Check | Pass |
|---|---|
| Response | Both voices, `grind` at its plateau-relative 0: within ±0.25 dB of the analog network, 20 Hz – 20 kHz |
| Analysis anchors | Focused, before level: +3.7 / +12.3 / +22.5 / +20.5 dB at 100 Hz / 300 Hz / 1 kHz / 4 kHz, ±0.5 dB |
| Grind law | Plateau gain equals `grind` (Focused) within 0.1 dB at −8, 0, +10 and +20 dB; the EQ shape does not change with `grind` |
| Voice switch | Switching under a 200 Hz sine: no click above the common bound |
| Subsonic | No resonance below 20 Hz (the 5 Hz DC blocker); no DC at the output; silence floor ≤ −140 dBFS 3 s after a burst |
| Linearity | 1 kHz sine at −6 dBFS in: THD < −120 dB (float path, no clipping) |
| Zipper | A `grind` sweep under a sine: no click above the common bound |
| Determinism, CPU | Strict null; ≤ 0.3 % of one core at 48 kHz, mono |
| Listening | djent.wav and high-gain.wav into a NAM high-gain amp, against the Nameless Suite Grind (Windows renders) and gfx-boost at the same loudness |
