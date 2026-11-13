# gfx-met

> **Status: CODED** (v0.1.0, 2026-11-13) — keyed mean-square detector with the circuit's adaptive timing (6.5 dB clamp, fast τ = slow / 13), hard-knee 1 : 2.8 expander, hold / release with a 5 ms minimum fall, ripple-aware gain smoother, key latch fallback, meters; 7 behaviour tests, bench green (28 checks); CPU 96 ns / sample. Design reviewed by Fable 5.2 (folded in). The A / B against the Nameless Suite waits for the Windows renders.

golem's **keyed expander-gate** for high gain. *Met* is the word that switches a golem off: erase one
letter of *emet* on its forehead and it falls silent. It is a new block (no golem block to replace),
built for one job: silence between chugs with no audible gate, from a clean key signal, with one knob.
gfx-gate stays golem's full-featured gate; gfx-met is the high-gain specialist.

## 1. State of the art

Survey of 2026-11-13. **[M]** manufacturer page or manual · **[C]** read from a DIY "compare to" clone
schematic (close, not guaranteed) · **[A]** calculation from those schematics (ours, checked by the design
review) · **[F]** forum opinion · **[I]** inference, not re-checked · **[V]** read in a datasheet.

### Reference: the Fortin ZUUL family

- **Controls** [M]: the Zuul has one GATE (threshold) knob, "which also controls the dynamics of the
  gate", a threshold-range switch for hot or weak signals, and a KEY input normalled to the main input.
  The Zuul+ adds HOLD (keeps the gate open through short pauses) and RELEASE (the fade-out time once the
  hold is over), plus KEY IN / KEY THRU for the four-cable method. Fortin publishes no ranges. There is no
  attack control and no hard/soft switch. Neural DSP's Nameless Suite Zuul has a threshold knob only.
- **Keying** [M]: guitar → KEY IN → KEY THRU → drives → amp; the amp's FX loop runs through the Zuul. The
  gate **listens to the clean guitar** but **acts on the post-preamp signal**, before the time effects.
  Fortin: "you can go from clean to high gain without changing the threshold", and the gate ignores the
  hiss of the gain stages. It needs a series loop.
- **Circuit** [C]: a THAT4301-type analog engine with a log-domain **true-RMS** detector and a dB-linear
  VCA. The audio path runs at unity gain; the key is coupled through a ≈ 32 Hz high-pass.
- **It is an expander, not a gate** [A]: the control path multiplies to about 1.8, so each dB under the
  threshold adds about 1.8 dB of attenuation. That is a **1 : 2.8 downward expansion, hard knee,
  unlimited depth**. No hysteresis network is visible; the gain is continuous, so it cannot chatter
  (it can still flutter near the threshold, see §2).
- **The timing** [A, medium confidence]: the RMS timing capacitor is Miller-multiplied by an
  integrator (≈ 9.1 µF effective, τ ≈ 58 ms). Two diodes clamp that integrator when the detector moves
  more than ≈ 6.5 dB away from a leaky reference (10 MΩ × 10 nF, ≈ 100 ms); the capacitor then drops to
  ≈ 0.71 µF (τ ≈ 4.5 ms).
  - An attack is instant anyway: a mean-square detector rises in microseconds on a step up.
  - A fall runs at the slow rate (≈ 75 dB/s) for its first ≈ 6.5 dB, about 85 ms, then plunges at
    ≈ 965 dB/s. That slow-then-plunge shape is probably the "natural" feel, and it works as a built-in
    hold.

### Others

| Reference | What makes it special | What we take |
|---|---|---|
| ISP Decimator [I] | Time-vector tracking; key loop (G-String) | Confirms the key-before-gain topology |
| gfx-gate (ours) | Peak open, one-period sliding-max close, hysteresis, `follow` decay | Its key bus, its meters |
| THAT4301 datasheet [V] | RMS detector τ = 0.026 · Ct / IT; attack and release in a fixed ratio | The detector model |

### What we conclude

1. **Expansion, not switching.** A fixed slope under the threshold fades notes out at a rate that
   follows the playing.
2. **A true mean-square detector, read in dB**, followed by a dB-linear gain: the detector's time
   constant is the gate's ballistics, and attacks are never clipped.
3. **The circuit's slow-then-plunge timing** is the character: modelled, not replaced by a faster
   heuristic. It stays switchable (`adaptive`), so the two can be compared by ear.
4. **Key from the DI**, on gfx-gate's key bus.

## 2. Design

### Signal path

```
main in ───────────────────────────────────────────────────────────────────────────▶ × gain ─▶ out
key in (aux bus) ─┐                                                                       ▲
main in ──────────┴▶ source ▶ HPF 32 Hz (+ `hpf`) ▶ LPF `lpf` ▶ mean square (τ adaptive) ▶ dB ▶ expander ▶ hold / release ▶ smoother ┘
                                                                     ▲
                                                leaky reference (1 s) and 6.5 dB clamp
```

- **Detector.** `ms += a · (x² − ms)`, level = 10 · log10(ms), where x is the filtered key.
  - The coefficient a comes from τ, which is `smooth` (default 58 ms) in normal running.
  - A **reference** r follows the level in dB: it jumps up with a rising level and leaks down with a
    1 s one-pole. The clone's 10 MΩ × 10 nF suggests 100 ms, but with 100 ms the clamp would only
    engage ≈ 200 ms into a fall (a slow detector falls at 75 dB/s, barely faster than the leak). A 1 s
    leak gives the review's reading: the first 6.5 dB at the slow rate (≈ 85 ms), then the plunge. To
    be settled by ear against the Nameless Suite Zuul.
  - When the level falls more than 6.5 dB below r (`adaptive` on), τ switches to `smooth` / 13
    (≈ 4.5 ms) until the level is back within 6.5 dB of r.
  - Rises are never slowed: mean-square averaging already rises fast on a step up.
- **Gain computer.** At or above `thresh` the gain is 0 dB. Below it:
  `gain_dB = (level − thresh) · (ratio − 1)`, floored at `range`. The knee is hard, as in the circuit.
  The threshold is an **RMS** level: a sine reads 3 dB below its peak, unlike gfx-gate's peak threshold.
- **Hold / release** (Zuul+), applied to the computed gain:
  - a fall in gain waits `hold` ms after the level drops below the threshold;
  - the gain then falls no faster than |`range`| / `release` dB per second, linear in dB. When `range`
    is 0 dB there is nothing to fall, and the rate is not computed;
  - `release` 0 means no limit, the detector alone. With `hold` > 0 and `release` 0 there is still a
    5 ms minimum fall, so the step after the hold never clicks;
- **Smoother**: a rise of more than 3 dB (an attack) runs through two cascaded 0.25 ms one-poles in
  linear gain, an S-shaped onset that opens a note within 1 dB in ≈ 1 ms without a click; a smaller rise is smoothed over 40 ms; falls take 1.5 ms. It removes the mean-square ripple of low notes:
  ±0.9 dB on a low E would otherwise become ±8 dB of gain wobble at `ratio` 10.
- **Key fallback.** If no non-zero key sample has been seen since reset, the detector reads the main
  input. Once the key has been seen, Met stays on it, even if it falls silent: a muted DI keeps the gate
  closed instead of opening on the amp's hiss.
- **Layouts**: main 1 / 1 first, 2 / 2 second, each with a key **auxiliary bus** (1 or 2 channels),
  exactly as gfx-gate. Stereo: one detector, the same gain on both channels.
- **Meters**: `state` (0 / 1; open when the gain is above −1 dB) and `gr` (dB), read-only, as in gfx-gate.
- **Zero latency**, no lookahead. An amp or cab with latency makes the key arrive early, so tails close
  early by that latency. golem compensates it when it routes the key.

## 3. Parameters

New block: no catalogue ids to keep. The ids are gfx-gate's wherever the meaning is the same (`thresh`,
`hold`, `release`, `range`, `ratio`, `hpf`, `lpf`, `source`, `listen`, `state`, `gr`).

Columns: **Curve** (display only, every range is linear) · **Smooth** in ms (— = structural) · **EXP**.

### Front panel

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `thresh` | Threshold | f | −90 – 0 dB | −45 dB | lin | 20 | | RMS level of the key under which the expansion starts (a sine reads 3 dB below its peak) |
| `hold` | Hold | f | 0 – 500 ms | 0 ms | log | — | | Keeps the gate open this long through short pauses before it fades |
| `release` | Release | f | 0 – 1,000 ms | 0 ms | log | — | | Shortest fade from open to fully closed; 0 = the detector alone (the original Zuul) |

### Expert

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `ratio` | Ratio | f | 1.5 – 10 :1 | 2.8 :1 | log | 20 | | Expansion slope under the threshold: 2.8 = the Zuul; 10 ≈ a hard gate |
| `range` | Range | f | −90 – 0 dB | −90 dB | lin | 20 | | Floor of the attenuation |
| `smooth` | Smoothing | f | 10 – 200 ms | 58 ms | log | 20 | | Slow time constant of the detector; the fast one is 13 times shorter |
| `adaptive` | Adaptive | b | off / on | on | — | — | | The circuit's timing: a fall beyond 6.5 dB switches to the fast time constant |
| `hpf` | Key Low Cut | f | 0 – 500 Hz | 0 (off) | log | 20 | | Extra high-pass on the key, on top of the fixed 32 Hz: ignores low-string thump |
| `lpf` | Key High Cut | f | 500 – 22,000 Hz | 22,000 Hz (off) | log | 20 | | Low-pass on the key: ignores pick noise and single-coil hiss |
| `source` | Detector Source | e | Main · Key | Key | — | — | | Key = the auxiliary input; Main until a key signal has been seen |
| `listen` | Listen | b | off / on | off | — | 5 (crossfade) | | Outputs the filtered key, for setting `hpf` and `lpf` |

### Meters (read-only)

| id | Name | Range | Role |
|---|---|---|---|
| `state` | Gate State | 0 / 1 | Open when the gain is above −1 dB |
| `gr` | Gain Reduction | 0 – 90 dB | Current attenuation |

The pedal face shows `thresh` as a large knob, with `hold` and `release` as two small ones.

## 4. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Djent / modern metal chugs (the reason it exists) | key = lane DI, `thresh` −45 dB, rest default | Keyed RMS expansion with the circuit's timing: silent between chugs, attacks untouched |
| Original Zuul feel | `hold` 0, `release` 0, `ratio` 2.8 | The detector alone |
| Zuul+ for staccato riffs | `hold` 20 ms, `release` 60 ms | Short pauses stay open, then a clean fade |
| Sustained chords that must ring out | `ratio` 1.8, `smooth` 120 ms | A gentle slope, slow smoothing |
| Hard gate for stop-start breakdowns | `ratio` 10, `release` 30 ms | Close to switching, still click-free |
| Noisy pickups, low tunings | `hpf` 80 Hz, `lpf` 5 kHz | The key ignores the low string's thump and the hiss |
| Without a key (simple use) | `source` Main | Works as a regular expander on the lane |

## 5. Bench suite (on top of gfx-bench's common checks)

Figures come from the model, computed in the test (no hand-copied constants).

| Check | Pass |
|---|---|
| Expansion slope | Steady sine at T − 10 dB (RMS) → attenuation 18 ± 0.5 dB at `ratio` 2.8 |
| Attack | From silence, a sine 20 dB above the threshold is within 1 dB of open after 1.5 ms |
| Adaptive timing | After a −30 dB step, with `adaptive` on: the first 6.5 dB of the level fall at the slow rate, the rest at the fast rate (within 10 % of the model); with it off, at the slow rate throughout |
| No flutter | A sine at the threshold with ±1 dB beating at 5 Hz: gain modulation ≤ ±2 dB at `ratio` 2.8 |
| No chatter | A synthetic exponentially decaying sine (82 Hz) through the threshold: no gain rise above 0.2 dB while it falls |
| Hold / release | `hold` 50 ms: no attenuation for 50 ms after the drop; `release` sets the fade rate within 5 %; `hold` 50 / `release` 0: fall no steeper than the 5 ms minimum |
| Key fallback | Before any key: `source` Key = Main. After a key burst then a silent key: the gate stays closed on a hissing main input |
| Clicks | Chugs at 160 BPM keyed from the DI: no click above the common bound |
| Determinism | Strict null between two renders |
| CPU | ≤ 0.5 % of one core at 48 kHz, stereo |
| Listening | djent.wav and high-gain.wav (di-refs) through a NAM high-gain amp, keyed from the DI, `adaptive` on against off, and against gfx-gate and the Nameless Suite Zuul (Windows renders) |
