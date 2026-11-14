# gfx-geq

> **Status: DESIGNED** (2026-11-14) — not coded. Design by Claude Opus 5.5, reviewed and amended by
> Claude Fable 5.2 the same day (the Review section at the end lists every change and what is still open).

golem's **graphic equalizer**, in pedal form. It stands for golem's `eq-graphic` block (today guitarix's
GxGraphicEQ: 11 bands, `g1` … `g11`, ±12 dB) and keeps its eleven ids, its dB unit and its band order
([golem contract](../docs/golem-contract.md)). The aim is a graphic EQ whose **response is the curve the
faders draw**: every fader honoured at its frequency, plateaus flat, nothing sticking out between two
faders. It also has two pedal layouts (guitar 7-band, bass 7-band), a **Pedal** mode that interacts the
way the analog circuit does, an honest level control, and no hiss.

## 1. State of the art

Survey of 2026-11-14, re-checked at review. **[V]** read in the source · **[M]** manufacturer page or
manual · **[F]** forum opinion · **[I]** from memory or inferred, not re-checked · **[U]** unknown, to
measure · **[P]** measured on the review's numerical prototype (§3.3), to be confirmed by gfx-bench.

### The core problem: band interaction

A graphic EQ asks for gains at fixed **command frequencies**. Each band is a bell, and a bell leaks into
its neighbours. So the realised curve is not the fader curve:

- **Naive cascade** (every bell gets its fader's gain): adjacent boosts pile up. Errors reach 10 dB in
  selected cases (Rämö, Välimäki & Bank 2014, as cited by Välimäki & Rämö 2019 [V]).
- **Constant-Q vs proportional-Q** (Rane / Bohn, *Constant-Q Graphic Equalizers*, JAES 1986, and Rane
  notes [I]): a constant-Q bell keeps its width at every gain; a proportional-Q bell (API 560 [I], most
  analog pedals) widens at small gains, which sounds smooth but interacts more. **Neither fixes the
  interaction.** Each only trades ripple against overlap.
- **True-response hardware**: Rane's Perfect-Q (DEQ 60L) claims "the first graphic equalizer whose
  output response precisely matches the front panel slider settings" (Rane, *Equalization Methods with
  True Response* [V]). That is the class we are in.
- **Accurate Cascade Graphic EQ (ACGE)**, Välimäki & Liski, IEEE SPL 24(2), 2017. Details read in
  Välimäki & Rämö 2019 [V]:
  - The band filter is an Orfanidis bell whose **gain at the band edges is a fixed fraction c = 0.3 of
    its peak gain (dB)**. The **edges are the neighbouring centres** (bandwidth B = 1.5 ω_c for
    octaves), so 30 % of each band's dB gain leaks onto its neighbours' command points.
  - An **interaction matrix** gives each bell's normalised dB leakage, measured at a 17 dB prototype
    gain, at 2M − 1 design points (the centres and their geometric midpoints: 19 × 10 for octaves).
  - A least-squares solve gives the filter gains. **One iteration** rebuilds the matrix at those gains
    and solves again.
  - Result: about ±0.9 dB **at the centre frequencies** (the goal was ±1 dB). The papers do not bound
    the error between the centres.
  - The top three bandwidths were set by hand, because of bilinear warping near Nyquist (fs = 44.1 kHz).
- **High-precision parallel GEQ** (Rämö, Välimäki & Bank, IEEE/ACM TASLP 22(12), 2014 [V, reference]):
  parallel second-order sections fitted by least squares. Very accurate, but its phase differs from a
  cascade's, and a parallel bank is not bit-transparent when flat.
- **High-order band-shelving GEQs** (Holters & Zölzer, DAFx 2006; Rämö & Välimäki, IEEE SPL 2014 [V,
  references]; the idea behind "flat-top" graphic EQs): each band is a steep, flat-topped 4th-order (or
  higher) section. Little interaction, but the response is a **staircase**, not the fader curve, with
  more phase shift per band. Not taken.
- **Neurally controlled GEQ** (Välimäki & Rämö, IEEE/ACM TASLP 27(12), 2019 [V]): a 20-neuron network
  replaces the matrix solve: 99.6 % less computing time, same ±1 dB goal. Later Aalto work uses
  **symmetric band filters** near Nyquist (no hand-set bandwidths) and reports a third-octave GEQ within
  0.81 dB (Rämö, Liski & Välimäki, *Third-Octave and Bark Graphic-Equalizer Design with Symmetric Band
  Filters*, Appl. Sci. 2020 [V, abstract]).
- **What these papers do not do, and we do**:
  - put **sections between the command frequencies** (one bell per band cannot hold both the centres
    and the plateaus: ±0.8 dB ripple with all faders up, 1 dB outside the faders' envelope [P]);
  - iterate to convergence, which a warm start makes free (one correction per control tick);
  - use Vicanek matched bells, already in gfx-dsp, for the symmetric top bands.

### How an analog graphic EQ really interacts

The 7- and 10-band pedals, and the API 560, are **not a cascade of bells**. They are one gain stage with
a series-resonant branch (gyrator or inductor) per band, each hung on the wiper of a pot between the
op-amp's two inputs [I: the classic circuit; the exact parts of each pedal are [U]]. With x the pot
position (0 = full cut, 1 = full boost), ideal op-amp, input and feedback resistors R:

```
H(s) = (1 + R · Σ xᵢ Yᵢ(s)) / (1 + R · Σ (1 − xᵢ) Yᵢ(s)),   Yᵢ = 1 / (Zᵢ(s) + xᵢ (1 − xᵢ) R_pot)
```

- The pot's own resistance, x (1 − x) R_pot, is in series with the branch: that is the
  **proportional Q** (broad near the centre detent, narrow at the ends of travel).
- The bands add **as currents into one node**, not in dB. With all faders at full boost the response
  is bounded (about +16 to +17.5 dB for ±15 dB bands of Q 1.5 [P]); a dB-sum of the same lone curves
  gives +14 to +24 dB depending on the parts. A naive cascade is therefore **not** "the way an analog
  graphic EQ behaves".

### Pedals and hardware

| Product | Bands | What players love / hate | What we take |
|---|---|---|---|
| Boss GE-7 [M] | 100 · 200 · 400 · 800 · 1.6k · 3.2k · 6.4k, ±15 dB, level ±15 dB. Boss describes six peak bands and, at 6.4 kHz, "a filter for smoothly adjusting treble" [M]; its exact shape (shelf or broad peak) is [U] | Loved: the solo mid lift with the level fader, "tighten before a high-gain amp", fixing a scooped amp [F]. Hated: **hiss** (an industry of noise mods: Analog Man, JHS, Sniper, Silencio [F]), bands that sound "peaky" at full boost [I] | The **Guitar 7** layout; level; no noise by construction; its circuit law for Pedal mode |
| Boss GEB-7 [M] | 50 · 120 · 400 · 500 · 800 · 4.5k · 10k, ±15 dB, level | The bass slap / growl centres; 400 and 500 Hz side by side for fine low-mid work | The **Bass 7** layout |
| MXR M108S Ten Band [M] | 31.25 Hz – 16 kHz in octaves, ±12 dB, plus volume and gain sliders | "After ten bands I couldn't go back to seven" [F] | Our Octave 11 is this grid plus 12 kHz |
| Source Audio EQ2 [M] | 10 bands 31 Hz – 16 kHz, ±18 dB, adjustable frequencies, 128 presets, up to 12 dB of clean boost, stereo, MIDI | Preset recall per song is the reason to buy it [F] | golem already recalls presets and scenes; ±15 dB is enough (§4) |
| API 560 [I] | 10-band octave, proportional Q | Musical, "can't sound bad" | Proportional Q, in Pedal mode |
| Rane DEQ 60L (Perfect-Q) [V] | 30-band, DSP | The response is the slider curve | The goal of Accurate mode |
| Line 6 Helix / Fractal GEQ blocks [I] | Several types: 10-band constant / variable Q, 7-band pedal style, 5-band Mark style… | Players choose by familiarity, not accuracy | Layouts, not "types" that silently change band meaning |

### Open source

| Project | Idea | Weakness |
|---|---|---|
| guitarix GxGraphicEQ [V: `graphiceq.dsp`, installed `gx_graphiceq.ttl`] | Today's `eq-graphic`. **Not bells**: a 3rd-order Faust filterbank (`fi.filterbank(3, …)`) split at 44 · 88 · 177 · 354 · 707 · 1,414 · 2,828 · 5,657 · 11,384 · 18,110 Hz, one gain per band, summed. Bands 1 – 9 are the octaves around 31.25 Hz … 8 kHz; band 10 is 11.4 – 18.1 kHz; band 11 is everything above 18.1 kHz. Installed ports −30 / +20 dB (upstream source now −30 / +5.2), clamped to ±12 in golem's catalogue | 3rd-order crossover phase on the whole signal even when flat-ish; `g11` is almost inaudible; a band, not a point, per fader |
| LSP Graphic EQ x16 / x32 [I] | IIR / FIR / FFT modes, slope choice, gains as linear multipliers | FIR / FFT modes add latency; linear gain ports (why golem did not choose it) |
| Calf [I], x42 [I] | Calf's 30-band uses high-order band-shelving filters (the Holters – Zölzer family); x42 has no GEQ (fil4 is parametric) | Staircase response |

Consequence for golem's presets: on GxGraphicEQ `g10` / `g11` act on 11 – 18 kHz and above 18 kHz. Here
they are points at 12 kHz and 16 kHz (the catalogue's labels), so the top of "Hygiaphone" or "Lo-fi
radio" is cut a little lower and more audibly. That is accepted by the contract (new sounds).

### What we conclude

1. **Accurate by default.** Every fader is honoured at its frequency within 0.25 dB, plateaus are flat
   within 0.5 – 0.75 dB, and between two faders the response stays between their two values. The
   engine is ACGE with **fill-in sections**, solved to convergence on exact matched-bell responses, at
   control rate, with zero latency.
2. **A Pedal mode** that is the analog circuit's law (§3.4), not a naive cascade: proportional Q, bands
   that add into one node, a lone fader that is broad at small moves.
3. **Layouts as separate fader sets, never as remapped ids** (§2).
4. **No hiss, no headroom problem**: float processing, f64 filter state.
5. **Level** (the solo boost), **amount** (the whole curve from flat to full, for the EXP pedal),
   optional **static auto gain** (so a boost does not pass for an improvement), and **low / high cut**
   (things the faders cannot do: a 30 Hz bell at −15 dB is not a high-pass).
6. Skipped: linear phase and FFT modes (latency), third-octave bands (not a pedal), flat-top band
   filters (staircase), adjustable centres (the EQ2 way: they would change what `g6` means; a parametric
   EQ already exists in gfx-eq), per-band Q.

## 2. Layout decision

The owner asked for "an EQ with 7 to 10 bands, pedal style". The catalogue block has 11 octave bands.
Four options were weighed:

| Option | Verdict |
|---|---|
| **A. `layout` enum that remaps `g1` … `g7` onto other centres** (e.g. `g3` = 125 Hz in Octave, 400 Hz in Guitar 7) | **Rejected.** A preset's `g3: +4` would mean two different sounds depending on another parameter. That is the mode-dependent meaning the contract forbids, and golem's preset seeding (reset to defaults, then apply the preset) would play an old preset on the wrong grid. |
| **B. Adjustable centre frequencies** (`f1` … `f11`) | Rejected for the same reason, and because it duplicates gfx-eq. |
| **C. Octave 11 only; pedal grids as presets** | Viable and minimal. But a player cannot put a fader at 800 Hz, and an 11-fader face does not fit a stomp box. |
| **D. `layout` enum + one dedicated id set per layout** | **Chosen, confirmed at review.** Each id always means "dB at this fixed frequency". `layout` only picks which set drives the engine. |

Option D consequences:

- `layout` defaults to **Octave 11**. golem's existing presets set only `g1` … `g11` and still play on
  the grid they were written for.
- A host stores all 25 fader values. The two inactive sets are stored and restored like any parameter,
  and are inert: no default depends on `layout`, no value is rewritten when `layout` changes.
- The pedal sets are named by frequency (`gt_800`, `gb_4k5`), so a preset reads unambiguously. A preset
  for a pedal layout carries `layout` and its own set; seeding resets the other sets to 0.
- A 10-band layout is not needed: the octave grid already is the 10-band pedal's grid (31.25 Hz –
  16 kHz), plus 12 kHz. Leave `g10` at 0 and it is that pedal.
- Enums are append-only, so a later layout (e.g. "Mark 5") is one more option at the end with its own
  ids. Option ranks and ids are permanent from the first release.
- **How golem shows it** (golem's choice, both work with this plugin):
  1. one `eq-graphic` block with 25 faders, the inactive sets hidden by the catalogue's `depends`
     (`"depends": {"layout": "Guitar 7"}`, the form already used in `effects.json`); or
  2. **recommended**: three catalogue blocks on the one plugin (the contract's "one plugin may stand
     for several blocks"): `eq-graphic` (11 faders) plus a guitar-7 and a bass-7 block, each pinning
     `layout` at insert (`init`) and listing only its own seven faders and the common parameters. No
     dead fader anywhere, and each block has a face of fixed width (§5).

## 3. Design

```
in ▶ low cut ▶ [ Accurate: cascade of matched bells in TPT SVFs | Pedal: one-node band-pass network ]
   ▶ high cut ▶ auto gain ▶ level ▶ out
             ▲
   control path (every 1.33 ms = 64 samples at 48 kHz, only while a target moves):
   faders (smoothed in dB) × amount ▶ target curve ▶ one solver correction (Accurate)
                                                   or the pot law (Pedal) ▶ coefficients, ramped
```

### 3.1 Band centres and sections

| Layout | Fader centres (Hz) | Ids | Accurate sections |
|---|---|---|---|
| Octave 11 | 31.25 · 62.5 · 125 · 250 · 500 · 1k · 2k · 4k · 8k · 12k · 16k (catalogue labels: 30, 60, 125, …) | `g1` … `g11` | 20 |
| Guitar 7 | 100 · 200 · 400 · 800 · 1.6k · 3.2k · 6.4k | `gt_100` … `gt_6k4` | 13 |
| Bass 7 | 50 · 120 · 400 · 500 · 800 · 4.5k · 10k | `gb_50` … `gb_10k` | 15 |

**Fill-in rule.** In Accurate mode the cascade has one bell per fader **plus fill-in bells between
them**: a gap of g octaves between two faders is cut into k = ⌈g / 0.7⌉ equal parts in log-frequency,
with k ≥ 2 as soon as g ≥ 0.5. Octave gaps get their midpoint; 12k – 16k and 400 – 500 Hz get none;
Bass 7's 800 Hz – 4.5 kHz gap gets three. Fill-in sections have no fader: their target is read on the
fader curve (§3.2).

Why: a zigzag on the fader grid is a smooth curve on a grid twice as fine. The solver then never asks
for more than about 1.5 × the fader range (on an octave grid, 17 dB of filter gain for a ±12 dB
zigzag, against 24 dB with one bell per band [P]), the centres and the plateaus hold together, and an
irregular layout (Bass 7) is no longer a special case.

Any section above 0.45 · fs is dropped. That never happens at 44.1 kHz or above.

### 3.2 The fader curve (the target)

The target T(f) is a **monotone cubic (PCHIP) in log-frequency through the fader values**, each
multiplied by `amount`. That is the line the fader caps draw, with rounded corners and no overshoot.
Two faders at +6 give a plateau between them, and a lone fader gives a bump that is back to 0 dB at its
neighbours (narrower than an analog pedal's lone band: that broader shape is Pedal mode's).

T is defined **between the first and the last fader only**. Outside, there is no target and no design
point: the end bells fall back to 0 dB on their own skirt (a lone −12 at 30 Hz is −7 dB at 25 Hz and
−3.5 dB at 20 Hz [P]). Removing the lows is `lowcut`'s job. Outer design points were tried and cost
0.7 dB at the end faders [P].

The realised curve is not required to follow the PCHIP on a steep slope (a ±2 dB gap on a 24 dB / oct
slope is a 1/12-octave shift that nobody hears). What is required, and benched (§7): the fader points,
the plateaus, and no excursion outside the two neighbouring fader values.

### 3.3 Accurate mode: the solver

The engine is ACGE on the section grid of §3.1:

1. **Band filters.** Each section is a symmetric bell (`matched::Analog::bell`) realised as a
   **Vicanek matched biquad**, symmetric up to Nyquist. Its **edges are at its neighbouring sections**
   (as in ACGE): the edge distance d is the mean of the log-distances to the two neighbours (the one
   neighbour for an end section), and the **gain at the edge is c · G**, with **c = 0.4** to start
   (0.3 is ACGE's value for one bell per band; 0.35 – 0.5 to be tuned on the bench: a larger c gives
   flatter plateaus and asks for larger filter gains). Q follows from G in closed form. Let
   Ω_e = 2^d, x = (1 − Ω_e²)² / Ω_e², α = 10^(|G|/20) and K = α^(2c). Then:
   - 1/Q² = x (K − 1) / (α − K / α);
   - at G → 0 the limit is x · c / (1 − c).

   With this law each bell's dB shape scales almost linearly with its gain, so the linear model behind
   the solver is good from the first step.
2. **Design points.** The sections' centres and the geometric midpoints between adjacent sections
   (39 for Octave 11, 25 for Guitar 7, 29 for Bass 7). Weights: **1 at the fader centres, 0.15 at the
   fill-in sections, 0.05 at the midpoints**. The fader points dominate; the rest keeps the curve
   between them.
3. **Precomputed at `initialize`, per layout and sample rate.** First the interaction matrix B₀, the
   normalised dB response of each section at a 15 dB prototype gain, using the **digital** matched
   magnitude (`Biquad::mag2`). Then its weighted, Tikhonov-regularised pseudo-inverse
   P = (BᵀWB + λI)⁻¹ BᵀW, with λ = 10⁻³. Fixed-size arrays (at most 20 × 39), a few tens of
   microseconds: no blocking work when the plugin turns on.
4. **Per control tick, only while a target moves (and 4 ticks after it stops):** one correction,
   **warm-started from the previous tick's gains**:

   g ← clamp(g + P · (T − R(g)), ±30 dB)

   where R(g) is the **exact** dB response of the cascade of matched designs at the design points.
   This is a chord (fixed-Jacobian Gauss–Newton) iteration: ACGE's "second matrix", carried on to
   convergence across ticks. From a cold start (`initialize`, `reset`, preset recall with the plugin
   stopped) it runs to convergence at once: g₀ = P · T, then corrections until the change is below
   0.005 dB (3 on average, 4 at most [P]; capped at 8). Deterministic, f64, no allocation.

   Measured [P]: during a 30 dB sweep of one fader in 50 ms the warm-started gains stay within
   0.02 dB of the converged ones (0.17 dB for the same sweep in 7 ms), and are converged 3 ticks after
   the fader stops.
5. **Flat is flat.** T ≡ 0 gives g ≡ 0. When **every** solved gain has stayed below 10⁻⁴ dB for 50 ms,
   the whole cascade fades out over 5 ms and is bypassed, so all faders at 0 (or `amount` at 0) is
   bit-transparent. Single sections are **never** bypassed on their own: a gain that crosses 0 dB in
   the middle of a sweep must not trigger a fade.

**Accuracy of this specification** [P] (numpy port of `matched::design`, 250 random settings per
case, same figures at 44.1, 48 and 192 kHz):

| Layout | Faders ±12 dB: worst error at a fader | Outside the two neighbouring fader values | All faders +12 | Zigzag ±12, at the faders | Largest filter gain | Faders ±15 dB: worst at a fader |
|---|---|---|---|---|---|---|
| Octave 11 | 0.16 dB | ≤ 0.7 dB | +12 ± 0.65 dB (worst around 12 kHz) | 0.21 dB | 23 dB | 0.22 dB (zigzag 0.29) |
| Guitar 7 | 0.09 dB | ≤ 0.45 dB | +12 ± 0.35 dB | 0.11 dB | 17 dB | 0.13 dB |
| Bass 7 | 0.10 dB | ≤ 1.1 dB | +12 ± 0.7 dB | 0.13 dB | 25 dB | 0.16 dB (zigzag ±15: 1.1, at the ±30 dB clamp) |

For comparison, the design as first written (one bell per band, edges half-way to the neighbours,
c = 0.3, at most 4 corrections) gave 1.3 – 1.5 dB at the faders and 4.6 – 5.4 dB of ripple with all
faders up [P]; plain ACGE (one bell per band, edges at the neighbours) 0.3 – 1.4 dB at the faders and
0.8 dB of ripple, and it failed on Bass 7 (about 5 dB at the faders, around the 400 / 500 Hz pair).

**Smoothing and stability.**

- Faders, `amount` and `level` are smoothed by the EXP rule (`smooth`). The solve is continuous in
  the targets, so smoothed targets give smoothly moving gains.
- Coefficients ramp linearly over the tick (`Svf::set_biquad_ramped`). Each section keeps its centre
  frequency, so only its damping and output mix move: with g fixed, the SVF's a1, a2, a3 stay
  proportional to each other and every interpolated point is a valid, stable SVF.
- A gain crossing 0 dB is continuous: the matched design of a cut is the inverse of the mirror boost,
  and both tend to the identity with the same poles.

### 3.4 Pedal mode

No solver and no cascade: the circuit's law of §1, with the fader in dB.

- **Network.** H = (1 + Σ aᵢ Bᵢ) / (1 + Σ bᵢ Bᵢ), one branch per fader of the active layout. Bᵢ is a
  band-pass of unity peak gain at the fader's frequency, with Qᵢ = Q_max / (1 + ρ xᵢ (1 − xᵢ));
  aᵢ = β xᵢ / (1 + ρ xᵢ (1 − xᵢ)) and bᵢ = β (1 − xᵢ) / (1 + ρ xᵢ (1 − xᵢ)).
- **Three constants per layout**: β = R / r = 10^(15/20) − 1 = 4.62 (a lone band at the end of travel
  is ±15 dB), ρ = R_pot / r (how far Q falls near the centre; start 20) and Q_max (start 1.8). ρ and
  Q_max are to be read from the 7-band guitar and bass pedals' schematics and checked against a SPICE
  run [U]. Guitar 7's 6.4 kHz branch is a first-order (series RC) high-shelving branch if the schematic
  says so [U]; otherwise a band-pass like the others.
- **Fader to pot.** xᵢ is the pot position at which a **lone** band gives the fader's dB at its
  centre: the root in [0, 1] of
  −(G − 1) ρ x² + ((G − 1) ρ − (G + 1) β) x + (G − 1) + G β = 0, with G = 10^(dB/20). So a lone fader
  reads true, 0 dB is x = 0.5, and the width and the interaction are the circuit's. `amount` scales
  the fader's dB before this.
- **Realisation, zero latency.** One bank of TPT band-passes (plain trapezoidal SVF: its band output
  is b(w) = γᵢ w + sᵢ, with γᵢ the instantaneous gain and sᵢ the state term) fed by the internal node
  w, shared by the numerator and the denominator:

  w = (x − Σ bᵢ sᵢ) / (1 + Σ bᵢ γᵢ),   y = w + Σ aᵢ Bᵢ(w)

  The delay-free loop is solved exactly, with one division per sample. The branches are positive-real
  and bᵢ ≥ 0, so the loop is stable for every setting and under modulation. aᵢ, bᵢ and Qᵢ come from
  closed forms and ramp over the tick: nothing to solve, nothing to zipper.
- All faders at 0 gives aᵢ = bᵢ, so H = 1 analytically; the network then fades out (5 ms) and is
  bypassed, bit-transparent.
- The top branch narrows a little near Nyquist at 44.1 kHz (bilinear band-pass). Matched band-passes
  are not used here: the loop needs positive-real branches. The bench reports the deviation.

### 3.5 Around the bands

- **Low cut / high cut**: matched Butterworth sections (gfx-eq's designs), 12 or 24 dB / oct each. A
  cut is off at its end stop (10 Hz, 22,000 Hz, as in gfx-eq) and bypassed there (bit-transparent).
- **Auto gain**: the pink-weighted power average of the realised curve (40 Hz – 18 kHz), computed on
  the control path, never from the signal, so nothing pumps (gfx-eq's `curve_average_db`; in Pedal
  mode the same average over the network's closed-form magnitude).
- **Level**: dB, after everything, smoothed by the EXP rule.
- **Layout / mode change**: the sections of two layouts sit at different frequencies, and the two
  modes are different structures, so coefficients are **not** ramped across a change. The plugin holds
  two engine slots: the new one starts from rest on the live input and the outputs crossfade over
  20 ms. Both are preallocated; the extra CPU lasts 20 ms.
- **Stereo**: 2 / 2 runs the same curve on both channels. Mono 1 / 1 is the first layout (contract).

### 3.6 Reuse vs new

| Reused (gfx-dsp / gfx-eq) | New (gfx-dsp `geq` module, shared with any later GEQ) |
|---|---|
| `matched::{Analog::bell, low_pass, high_pass, design, Biquad::mag2}` | Fill-in rule, PCHIP target in log-f |
| `svf::Svf` + `set_biquad`, `set_biquad_ramped` (TPT realisation, f64 state) | Q-from-edge-gain law (§3.3.1) |
| `smooth` (EXP rule) for faders, amount and level | Interaction matrix, regularised pseudo-inverse, warm-started chord iteration on fixed arrays |
| gfx-eq's fade-to-bypass and `curve_average_db` (to lift into gfx-dsp; gfx-eq then calls the shared copy, with no behaviour change) | The one-node network (§3.4): `Svf` gains a band-pass "instantaneous gain + state term" accessor |
| | A bell designer with a **fixed centre**: `matched::design` recomputes its 25-point fitting grid (25 `powf`) on every call; for a section whose frequency never moves the grid is a constant, cached at `initialize` |

### 3.7 Latency and CPU

- **Latency: zero** (minimum-phase IIR in both modes; nothing reported to the host).
- **Audio path** (estimate): Accurate, 13 – 20 TPT sections + 2 – 4 cut sections ≈ 100 – 150 ns per
  mono sample, about 0.5 – 0.7 % of a core at 48 kHz. Pedal, 7 – 11 band-passes and one division:
  less.
- **Solver** (estimate, Octave 11): per tick, 20 matched designs (≈ 1 µs each with the cached grid),
  20 × 39 magnitude evaluations and one 20 × 39 product: 20 – 40 µs. At one tick per 1.33 ms that is
  1.5 – 3 % of a core **while a fader moves**, 0 at rest. The tick is defined in time, so the load
  does not grow with the sample rate. If the bench measures more: evaluate each section only at the
  design points within two octaves of it (its leakage beyond is at most 0.05 dB per dB [P]; re-run the
  accuracy table if this is used).
  The neural-network shortcut (Välimäki & Rämö) is not needed.

## 4. Parameters

Catalogue ids kept (unit dB, band order). The range is widened to ±15 dB, which the contract allows
("a little wider") and which is the 7-band pedals' range. All ranges are linear. New parameters default
to neutral. The display names of `g1` … `g11` stay **G1 … G11**, because the name is golem's join key;
the frequency is the catalogue `label`. `lowcut`, `highcut` and their slopes have gfx-eq's ids, ranges
and "off" stops.

| id | Name | Type | Range | Unit | Default | EXP | Role |
|---|---|---|---|---|---|---|---|
| `g1` … `g11` | G1 … G11 | f | −15 – +15 | dB | 0 | ✓ | Octave 11 faders: 30, 60, 125, 250, 500, 1k, 2k, 4k, 8k, 12k, 16k (catalogue −12 – +12) (depends: `layout` = Octave 11) |
| `layout` | Layout | e | Octave 11 · Guitar 7 · Bass 7 | — | Octave 11 | | Which fader set drives the EQ (§2) |
| `mode` | Mode | e | Accurate · Pedal | — | Accurate | | Accurate: the response is the fader curve. Pedal: the analog circuit's law, proportional Q and bands that interact |
| `level` | Level | f | −15 – +15 | dB | 0 | ✓ | Output level: the solo boost |
| `amount` | Amount | f | 0 – 100 | % | 100 | ✓ | Scales every fader of the active set: 0 = flat (bit-transparent), 100 = the curve as drawn |
| `gt_100`, `gt_200`, `gt_400`, `gt_800`, `gt_1k6`, `gt_3k2`, `gt_6k4` | 100 … 6.4k | f | −15 – +15 | dB | 0 | ✓ | Guitar 7 faders (depends: `layout` = Guitar 7) |
| `gb_50`, `gb_120`, `gb_400`, `gb_500`, `gb_800`, `gb_4k5`, `gb_10k` | 50 … 10k | f | −15 – +15 | dB | 0 | ✓ | Bass 7 faders (depends: `layout` = Bass 7) |
| `lowcut` | Low Cut | f | 10 – 500 | Hz | 10 (off) | | High-pass; 10 = off, bypassed |
| `lowcut_slope` | Low Cut Slope | e | 12 · 24 dB / oct | — | 12 | | Butterworth |
| `highcut` | High Cut | f | 1,000 – 22,000 | Hz | 22,000 (off) | | Low-pass; 22,000 = off, bypassed |
| `highcut_slope` | High Cut Slope | e | 12 · 24 dB / oct | — | 12 | | 24 after a cab simulation |
| `auto_gain` | Auto Gain | b | off / on | — | off | | Static loudness compensation from the curve |

31 parameters. Front panel (golem Settings tab): the active fader set and `level`. Expert: `layout`,
`mode`, `amount`, the cuts and `auto_gain`.

## 5. Visual

| Item | Choice |
|---|---|
| Category | `eq` |
| Hue | the family's `METAL_HUE.eq` `#0058e0`; **no crate hue**. It sits beside gfx-eq as family, and the faders tell the two apart |
| Finish | **anodized** (the eq family default; no `CRATE_FINISH` entry) |
| Motif | new **`scale`**: fine horizontal engraved rules at 0 (stronger), ±5, ±10 and ±15 dB running across the face behind the slots, like a screen-printed fader scale. When the pedal is on, a thin line in the family light joins the fader caps |
| Name word | **Graphic** |
| Glyph | the block's glyph `eq-graphic.svg` stays the face's glyph. Proposed refresh below |

**Proposed face field.** `faders`, an ordered list of parameter ids drawn as vertical sliders, plus an
optional `fader_sets` keyed by an enum parameter, with one list per option **by rank** (the
descriptor rule), and `shape: "wide"` beside the existing `"treadle"`:

```json
"gfx-geq": {
  "name": "Graphic", "blocks": ["eq-graphic"], "category": "eq", "shape": "wide",
  "faders": ["g1", "g2", "g3", "g4", "g5", "g6", "g7", "g8", "g9", "g10", "g11"],
  "fader_sets": {"param": "layout", "sets": [
    ["g1", "g2", "g3", "g4", "g5", "g6", "g7", "g8", "g9", "g10", "g11"],
    ["gt_100", "gt_200", "gt_400", "gt_800", "gt_1k6", "gt_3k2", "gt_6k4"],
    ["gb_50", "gb_120", "gb_400", "gb_500", "gb_800", "gb_4k5", "gb_10k"]]},
  "level_fader": "level"
}
```

**How to draw it** (renderer change later, not now):

- **Shape**: the box's width is **fixed per face, never driven by a parameter** (a pedal that changes
  width when `layout` changes would reflow the pedalboard). The `eq-graphic` face is the **wide**
  stomp, W 168 × H 200, like the 10-band pedals' wider box: 11 bands + level at a 12.5 px pitch with
  6 px margins. On it a 7-band layout draws its 7 sliders + level centred, at an 18 px pitch. If
  golem adds the two 7-band blocks (§2), their faces are the standard stomp (W 112): 7 bands + level
  at a 12 px pitch.
- **Slot**: a 1.6 px dark groove, 84 px long (y 16 – 100), with the motif's rules crossing it.
- **Cap**: 9 × 6 px machined, with an engraved centre line in the family light. Its position is the
  parameter's normalised value, so 0 dB sits at the slot's middle.
- **Labels**: 4.5 px, under each slot, from the catalogue labels or the frequency-named ids (30 … 16k,
  100 … 6.4k).
- **Level**: set apart by a 6 px gap, with a cap in the accent colour (gold on thrash, light on
  anodized) and the label `LVL`.
- **Curve line**: through the caps, as a monotone cubic (the Accurate target). In Pedal mode the played
  curve is broader than that line; the line is still drawn through the caps (a face is not a plot).
- **Below the sliders**: the usual footswitch, LED and name. No knobs, no selectors on the face.

**Glyph refresh** (24 × 24, the line style of `glitch.svg`: `fill="none" stroke="currentColor"
stroke-width="1.8" stroke-linecap="round"`):

- five slots at x = 4, 8, 12, 16, 20, from y 3 to 21, opacity .4: `M4 3v18M8 3v18M12 3v18M16 3v18M20 3v18`;
- caps as 3 px bars, stroke-width 2.6, forming a lift: `M2.5 14h3M6.5 15h3M10.5 9h3M14.5 7h3M18.5 12h3`;
- a 1.1 px line through the cap centres (the curve the EQ plays): `M4 14C6 14 6 15 8 15S10 9 12 9S14 7 16 7S18 12 20 12`.

No maker's livery, logo or trademark on the face.

## 6. Applications

Preset names carry no artist or trademark names. A `presets/gfx-geq.json` is to be written with the
code. golem's seven `eq-graphic` presets (Standard, Bass smiley, Hygiaphone, Lead boost, Lo-fi radio,
Vocal shape, V shape) play on Octave 11 with their values as written (a new sound, see §1: the top two
faders now sit at 12 and 16 kHz).

| Use | Settings to start from | Feature that matters |
|---|---|---|
| **Solo lift** on a footswitch / scene | Guitar 7: `gt_800` +4, `gt_1k6` +3, `gt_400` +1, `level` +3 dB | Accurate: +4 at 800 Hz means +4, with no pile-up; level |
| Tighten before a high-gain amp | Octave: `g1` −15, `g2` −10, `g3` −4, `g6` +3, `g7` +2 | Pre-amp low cut by faders, with no peaking at 125 Hz |
| Scooped rhythm, post-amp | Octave: `g4` +2, `g5` −4, `g6` −5, `g7` −2, `g9` +2 | |
| Fuzz in a band mix (restore the mids) | Guitar 7: `gt_400` +2, `gt_800` +5, `gt_1k6` +3 | |
| Post-cab cleanup | `lowcut` 80 Hz, `highcut` 8,000 Hz, `highcut_slope` 24, `g8` −2 | The cuts the faders cannot do |
| Bass slap smile | Bass 7: `gb_50` +4, `gb_400` −4, `gb_500` −3, `gb_4k5` +3, `gb_10k` +4 | Two low-mid faders a third of an octave apart, solved without a hole between them |
| Bass growl | Bass 7: `gb_120` +2, `gb_800` +5, `gb_4k5` −2 | |
| Piezo acoustic | Octave: `lowcut` 70 Hz, `g5` −2, `g6` −4 (the quack), `g10` +2 | |
| Telephone / lo-fi voice | golem's Hygiaphone, or `lowcut` 300, `highcut` 3,400, `g6` +4 | Cuts plus a mid bump |
| Honest A / B | any curve with `auto_gain` on | A boost does not win by being louder |
| Old-school pedal feel | `mode` Pedal, Guitar 7, faders by ear | Broad small moves, bands that lean on each other, as on the analog pedal |
| EQ depth by foot | any curve, EXP on `amount` 0 → 100 % | Heel = flat, toe = the full curve: a solo voicing that fades in; one parameter moves the whole curve |
| Cocked mid sweep | EXP on `g6` (or `gt_800`), −6 → +12 dB | Click-free solver under EXP |
| Volume swell / level by foot | EXP on `level` | |

Preset ideas: Solo lift, Mid push for fuzz, Tight front end, Scooped rhythm, Post-cab tame, Slap smile,
Bass growl, Piezo de-quack, Telephone, Bright clean sparkle (`g9` +3, `g10` +2, `g3` −2), Dark jazz
(`g8` −4, `g9` −6, `g10` −8, `g3` +2), Bedroom to stage (`g2` −3, `g7` +2), Flat (default).

## 7. Bench suite (on top of gfx-bench's common checks)

Thresholds are the prototype's figures (§3.3) plus a margin. A check that fails is a finding to
report, not a threshold to loosen.

| Check | Pass |
|---|---|
| Flat at neutral | every layout and both modes, all faders 0 (or `amount` 0 with any faders), level 0, cuts off: **bit-identical** output |
| Accuracy at the faders | Accurate, each layout, 10,000 random settings uniform in ±12 dB, at 44.1 / 48 / 96 / 192 kHz: **max error ≤ 0.25 dB** at every fader frequency. At ±15 dB: ≤ 0.35 dB |
| Between the faders | same draws, 1/48-octave grid from the first to the last fader (up to 0.45 fs): the response never leaves the interval of the two neighbouring fader values by more than **0.75 dB** (Octave 11, Guitar 7) or **1.25 dB** (Bass 7) |
| All faders up | +12 everywhere, between the first and the last fader: +12 ± 0.5 dB on Guitar 7, ± 0.75 dB on Octave 11 and Bass 7 |
| Lone fader | `g6` +12 alone: +12.0 ± 0.1 at 1 kHz, 0 ± 0.1 dB at 500 Hz and 2 kHz, never below −0.5 dB anywhere |
| Zigzag | alternating ±12: every fader within 0.3 dB; alternating ±15: within 0.4 dB (Bass 7: 1.5 dB) |
| Solver | cold start converges in ≤ 8 corrections on every draw; no draw hits the ±30 dB clamp at ±12 dB |
| Realisation | impulse response of the running plugin (FFT) vs the designed cascade: within 0.05 dB, including a 30 dB section at 31.25 Hz at 192 kHz (the SVF realises the matched designs) |
| Top bands | 16 kHz band at 44.1 kHz, ±12 dB: symmetric shape, fader within 0.25 dB |
| Pedal mode | matches the analog network formula of §1 within 0.1 dB up to 0.2 fs, deviation above reported; a lone fader gives its dB ± 0.05 at its centre; all faders at +15: bounded as the formula says |
| Modulation | EXP-stepped sweep of `g6` −15 → +15 dB in 50 ms under a sine, and of `amount` 0 → 100 % on a zigzag: no click, control-rate sidebands ≤ −80 dB, no section fade triggered when a gain crosses 0 dB; warm-started gains within 0.1 dB of the converged ones during the sweep. Both modes |
| Switches | `layout` and `mode` changes under a sine and under noise: click-free (20 ms crossfade) |
| Solver cost | worst-case tick ≤ 50 µs; CPU at rest and during a sweep reported, at 48 and 192 kHz |
| Auto gain | pink noise: loudness ±0.5 dB over random curves, both modes |
| Presets | golem's seven `eq-graphic` presets on clean, funk, crunch and bass DIs |

## 8. Coding notes

- gfx-dsp gets a `geq` module (§3.6). gfx-eq's fade and auto-gain helpers move into gfx-dsp with no
  change in behaviour, which its existing tests prove.
- Order of work: the solver and its accuracy tests first (pure functions on `Biquad::mag2`, no audio),
  then the cascade, then Pedal mode. If time is short, Pedal mode is the part to ship second: `mode`
  can be added later with Accurate as its first option and default.
- The numbers marked [P] come from a numpy prototype written for the review (a port of
  `matched::design`, not kept in the repository). The first task is to reproduce its table in Rust.
- Descriptors: set `depends` on every band set; `layout` and `mode` option ranks are permanent; announce
  the new crate and its ids to golem-rack before shipping (release flow).

## 9. Questions settled at review

1. **Layout as separate id sets (D) vs Octave 11 only (C).** **D.** It is contract-clean (no id
   changes meaning, no default depends on `layout`, enums and ids only grow), and the 800 Hz fader and
   the 7-fader stomp are what "pedal style" means. golem should expose it as three blocks pinning
   `layout` (§2).
2. **Bass 7's 400 / 500 Hz pair.** **Keep the reference grid.** With fill-in sections the layout holds
   0.10 dB at the faders [P]; evenly spaced centres would be a different instrument. Its looser bound
   between faders (1.1 dB) comes from the 2.5-octave gap above 800 Hz.
3. **Range.** **±15 dB.** It is the 7-band pedals' value, the accuracy table holds there, and ±18
   would push zigzags into the gain clamp for no musical use.
4. **Target curve.** **PCHIP in log-f**, between the first and last fader only; no target outside, no
   shelf (§3.2). Piecewise linear has corners that no minimum-phase cascade draws; "each fader a bump"
   is Pedal mode.
5. **Pedal mode.** **Kept, re-specified** as the analog circuit's one-node network (§3.4) instead of a
   naive cascade, which is not how those pedals interact. It earns its place: it is the broad,
   forgiving feel players know, Accurate cannot give it (its lone fader is back to 0 dB at the
   neighbours), and it costs less CPU than the cascade. Its two voicing constants per layout remain
   [U].
6. **Octave centres.** **31.25 · 2ᵏ**, then 12k and 16k. GxGraphicEQ's bands are now read (§1): they
   are the octave bands around these same centres up to 8 kHz. Labels stay the catalogue's.
7. **`level` vs `output`.** **`level`.** Nine golem-fx designs use `level` for a post-effect gain;
   gfx-eq's `output` is the one exception and is shipped (ids are permanent). `level` should be added
   to the contract's shared-ids table.
8. **Solver at control rate.** One warm-started correction per 1.33 ms tick (§3.3): 1.5 – 3 % of a
   core while a fader moves [estimate], 0 at rest. Accepted; the fallback is written in §3.7.
9. **Face field.** `faders` + `fader_sets` (by rank) + `level_fader` + `shape: "wide"` fit the
   existing `faces.json` schema (which already has `shape`). The width is fixed per face (§5).

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good**, and Strong once gfx-bench reproduces the prototype's table. The concept (true
response by default, layouts as separate id sets, zero latency, static auto gain) is right and
contract-clean. As first written the engine would not have met its own targets; the amended one does on
the prototype.

Changes made, and why:

1. **Bell bandwidths corrected.** §3.3 put the edges half-way to the neighbours with c = 0.3; ACGE puts
   them **at** the neighbours (B = 1.5 ω_c, read in Välimäki & Rämö 2019), as the survey itself said.
   Half-way edges give 4.6 – 5.4 dB of ripple with all faders up [P].
2. **Accuracy targets were unreachable; the engine is changed so they hold.** One bell per band gives
   0.3 – 1.4 dB at the faders and 0.8 dB of plateau ripple whatever the solver does (ACGE's own ±0.9 dB
   is at the centres only, not "on the curve" as the bench assumed). Added **fill-in sections** (§3.1):
   0.09 – 0.16 dB at the faders, plateaus within 0.35 – 0.7 dB, filter gains a third lower. Cost: 13 –
   20 sections instead of 7 – 11.
3. **Bench rewritten** around what can be heard and met: the fader points, plateaus, and "never outside
   the two neighbouring faders" replace "≤ 1 dB from the PCHIP on a 1/24-octave grid" (5 dB on steep
   slopes for any realisable filter, and inaudible). Thresholds now come from measured figures.
4. **Solver made cheap and safe under EXP**: one warm-started correction per tick instead of up to 5
   from scratch (tracks within 0.02 dB in a 50 ms sweep [P]). The cost estimate had left out the 11
   matched designs per iteration (each a 25-`powf` fit), the dominant term; added a fixed-centre
   designer. Tick defined in time (1.33 ms), not samples. Clamp ±30 dB (±27 was hit by ±15 zigzags).
5. **Per-section bypass removed.** A solved gain crossing 0 dB mid-sweep would have triggered a 5 ms
   fade out and in. The cascade is bypassed as a whole, after 50 ms at zero.
6. **Layout / mode switch**: ramping coefficients between layouts would have swept filters across the
   spectrum (sections sit at different frequencies). Now two engine slots and a 20 ms crossfade.
7. **Target ends**: the outer design points and the "hold, then return to 0 dB" rule are dropped (they
   cost 0.7 dB at the end faders [P]); nothing is specified outside the faders.
8. **Pedal mode re-specified.** A naive cascade with Q = Q₀ · 10^(|G|/40) is not the analog behaviour:
   those pedals sum band currents into one op-amp node, with the pot's resistance making the
   proportional Q. Replaced by that network, in closed form, with a zero-delay realisation on one bank
   of band-passes. The unknown is reduced from "a voicing to find" to two constants per layout.
9. **Facts corrected**: GxGraphicEQ is a 3rd-order filterbank split at 44 … 18,110 Hz, not a naive
   bell cascade, and its centres are now read (the [U] is resolved); Rämö / Välimäki / Bank 2014 is
   IEEE/ACM TASLP, not SPL; the 2020 Aalto paper does use symmetric band filters, so "none of these
   papers" was wrong on that point; Calf's graphic EQ is the high-order band-shelving kind [I]; the
   GE-7's top band is described by Boss as a treble filter, its shelf shape is [U] not [M]. Added Rane
   Perfect-Q and the high-order band-shelving family to the survey.
10. **Contract and ids**: `lowcut` 10 – 500 Hz with 10 = off (was 0 – 400 with 0 = off), and
    `lowcut_slope` / `highcut_slope` (was one `cut_slope`): the same ids, ranges and stops as gfx-eq,
    and the common post-cab setting is 12 low / 24 high. `depends` written in the catalogue's real form.
    Stated what a host storing all 25 faders implies. Recommended three golem blocks pinning `layout`.
11. **Added `amount`** (0 – 100 %, EXP): the one move a graphic EQ under a foot lacks, and nearly free
    (it scales the target).
12. **Visual**: the face's width no longer follows `layout` (it would reflow the pedalboard); scale
    rules at 0, ±5, ±10, ±15 to match the ±15 dB slot.
13. **Preset claim softened**: golem's presets keep their values, but do not "play unchanged": the top
    two faders act lower than GxGraphicEQ's top two bands.

Still open for the owner:

| Point | Owner |
|---|---|
| Reproduce the [P] table in gfx-bench; tune c (0.35 – 0.5) and the weights on it | golem-fx, first coding task |
| Pedal mode: ρ, Q_max and the 6.4 kHz branch from the 7-band pedals' schematics + a SPICE check [U]; whether Pedal ships in v1 or second | golem-fx / Raph |
| `amount`: keep (reviewer's addition) or drop | Raph |
| 20 sections on Octave 11 (≈ 0.7 % of a core, 1.5 – 3 % while a fader moves): accepted, or cap the tick cost lower | Raph, after the bench |
| One `eq-graphic` block with `depends`, or three blocks pinning `layout`; catalogue range ±12 → ±15 | golem rack session |
| Add `level` to the contract's shared-ids table | Raph (contract) |
| `faders` / `fader_sets` / `shape: "wide"` in the renderer; the wide stomp on golem's pedalboard | visuals / golem rack session |
| Upstream guitarix now limits its ports to +5.2 dB (installed build: +20); no effect on us, noted for golem's catalogue | golem rack session |
