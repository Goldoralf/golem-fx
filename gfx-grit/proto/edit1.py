import os
import sys
p=os.path.expanduser('~/code/golem-fx/gfx-grit/README.md')
t=open(p).read()
R=[]
def r(a,b): R.append((a,b))

r("> **Status: DESIGNED** (2026-11-14) — design only, no code. Not reviewed yet.",
  "> **Status: DESIGNED** (2026-11-14) — design only, no code. Reviewed 2026-11-14 (Fable 5.2): see the\n> Review section at the end; its changes are applied below.")
r("""oversampling with ADAA). Every dirt module is level-matched, so the Amount knob changes the tone and
leaves the volume where it was.""",
"""oversampling with ADAA). The saturating modules are level-matched at a reference level, so the Amount
knob changes the tone far more than the volume.""")

# 1.1
r("""  octaves. 25 % = 112 Hz, 50 % = 632 Hz, 75 % = 3,557 Hz [A]. `calccross` keeps each crossover ≥ 0.02
  (0.2 octave) above the previous one.""",
"""  octaves. 25 % = 112 Hz, 50 % = 632 Hz, 75 % = 3,557 Hz [A]. `calccross` keeps each crossover ≥ 0.02
  (0.2 octave) above the previous one and inside its own window: x1 in 2 – 94 %, x2 in 4 – 96 %, x3 in
  6 – 98 % (23 Hz – 17.4 kHz).""")
r("| 8 | Zero cross | dead zone T = v¹⁰ then a shaping power | No: v¹⁰ (T = 200 log₁₀ v dBFS) |",
  "| 8 | Zero cross | y₀ = (x − clamp(x, ±v¹⁰)) / (1 − v¹⁰), then y = y₀ · (1 − (1 − \\|y₀\\|)^12.5)^(3 / (1 − 0.98 v) − 3); v = 1 → silence | No. The v¹⁰ clip is inaudible below v ≈ 0.6; **the power term is the audible dead zone**. Its −6 dB point [P]: −40 dBFS at 10 %, −27 at 23 %, −19 at 50 %, −11 at 77.5 %, −5 at 90 % |")
r("| 10 | Sample divide | hold rate R = 40 · 1000^(1 − v) Hz (fractional counter, rate in Hz independent of oversampling) | **Yes**: linear in octaves |",
  "| 10 | Sample divide | hold rate R = 40 · 1000^(1 − v) Hz (fractional counter, rate in Hz independent of oversampling; the sample at each new hold is a linear blend of old and new) | **Yes**: linear in octaves |")
r("""3. **Several laws are steep or discontinuous**: Bit crush jumps from bypass to 7.6 bits as v leaves 0;
   Zero cross spends 50 % of its travel below −60 dBFS (v¹⁰); Soft clip spends its first 10 % on 0 → 9 dB.""",
"""3. **Several laws are steep or discontinuous**: Bit crush jumps from bypass to 7.6 bits as v leaves 0;
   Zero cross is two laws multiplied, and silences a −18 dBFS signal from v ≈ 0.75; Soft clip spends its
   first 10 % on 0 → 9 dB.""")
r("""5. Level: every module carries an ad-hoc make-up (the 0.92 factors), so loudness moves with Amount.""",
"""5. Level: every module carries an ad-hoc make-up (the 0.92 factors), so loudness moves with Amount.
6. Prisma's own "Tight kick" preset, read as (value, type) pairs [V]: Width 0 → Gain 0.615 (+9.8 dB) →
   Zero cross 0.775 → Gain 0.23 (−7.3 dB), on the low band. It needs four modules; §4 gives the two-slot
   equivalent.""")

# 1.5
r("""2. **One oversampling domain for the whole split**, as Prisma does, but with ADAA1 on every nonlinear
   module and 2× even with Quality off. All bands then share one group delay: coherent by construction.""",
"""2. **One oversampling domain for the whole split**, as Prisma does, but with ADAA1 on every nonlinear
   module and 2× even with Quality off. All bands then share one group delay: coherent by construction.
   "Antialiased" means 15 – 20 dB less aliasing than the same shaper without ADAA, not alias-free: §2.1
   gives the figures.""")
r("""4. **Level-matched modules** (static compensation), so Amount is a tone control.""",
"""4. **Level-matched saturators** (static compensation at a reference level), so Amount is mostly a tone
   control.""")

# diagram
r("""          │   band b ─┬─▶ slot 1 ─▶ slot 2 ─▶ DC block ─▶ (wet_b)                                    │
          │           └──────────────────────────────▶ (dry_b, aligned) ─▶ mix m_b ─▶ × gain_b ─▶ Σ ─┤
          └──────────── solo / mute / bypass ramps ─────────────── down 2× / 4× ───────────────────┘─▶ × level ─▶ out
   m_b = wet · <b>_mix ; slot amounts = <b>_v + drive / 2 + 25 · focus · w_b (clamped 0 – 100)""",
"""          │   band b ─┬─▶ slot 1 ─▶ slot 2 ─▶ DC block ─▶ (wet_b)                                    │
          │           └──────────────────────────────▶ (dry_b, aligned) ─▶ mix m_b ─▶ × gain_b ─▶ Σ ─┤
          └──────── solo / bypass ramps ──── droop correction ──── down 2× / 4× ───────────────────┘─▶ × level ─▶ out
   m_b = wet · <b>_mix ; slot amount = <b>_v + k_type · (drive + focus · w_b / 2) / 100 (clamped 0 – 100, §2.5)""")

# 2.1
r("""- **ADAA1 on every nonlinear module**, inside the oversampling. ADAA1 adds half a sample (at the inner
  rate); a slot that holds a linear module or Off passes a `HalfSample` instead, and the band's dry gets
  two of them. Every path in every band is then delayed by exactly one inner sample: the per-band mix and
  the band sum stay coherent (gfx-drive's blend lesson). When x[n] ≈ x[n−1] (|Δ| < 1e-5) ADAA falls back
  to f at the midpoint (standard).""",
"""- **ADAA1 on every nonlinear module**, inside the oversampling. At small signals ADAA1 is the two-tap
  average (x[n] + x[n−1]) / 2: half an inner sample of delay **and a cos(π f / fs_inner) roll-off**. A slot
  that holds a linear module or Off passes gfx-dsp's `HalfSample` (the same average) instead, and the
  band's dry gets two of them. Every path in every band then carries exactly the same response, one inner
  sample of delay with a cos² magnitude: the per-band mix and the band sum stay coherent (gfx-drive's
  blend lesson). When x[n] ≈ x[n−1] ADAA falls back to f at the midpoint (gfx-dsp `Adaa1`'s threshold).
- **Droop correction** (added at review). Two averages cost −0.95 dB at 10 kHz and **−4.0 dB at 20 kHz**
  at 2× (−0.23 / −0.95 dB at 4×) [P], on every band, even with every slot Off. Because the droop is common
  to all paths, one filter after the band sum, before the downsampler, removes it:
  C(z) = ((1 + a) / (1 + a z⁻¹))², a = 0.8. Residual ±0.06 dB to 20 kHz at 2×, ±0.012 dB at 4× [P]; it
  also gives the shaped signal back the highs ADAA1 takes. It is a constant, minimum-phase, two-pole
  filter: no coherence issue. (The exact inverse has its poles at z = −1; a = 0.8 leaves the zero at the
  inner Nyquist in place, where the downsampler's stop band is.)
- **What the alignment is worth** [P]: two equal paths half an inner sample apart lose 0.5 dB at 20 kHz
  when summed at 2× (one sample: 2.0 dB), nothing audible at the crossovers. So the alignment is a
  refinement, and the one exception is accepted: **Fuzz** holds two ADAA stages (one inner sample), half
  a sample more than its slot's allowance; a Fuzz band blended with its dry dips 0.5 dB at 20 kHz at 2×.
- **How much antialiasing** [P: numpy, ideal downsampler, sine at −6 dBFS, strongest aliased component
  below 20 kHz relative to the fundamental]:

  | Shaper, drive | 2× + ADAA1, sine at 2 / 5 / 9 kHz | 4× + ADAA1 |
  |---|---|---|
  | Hard clip, 24 dB (Amount 50 %) | −59 / −52 / −37 dB | −79 / −65 / −69 dB |
  | Hard clip, 48 dB (Amount 100 %) | −45 / −43 / −35 dB | −59 / −51 / −50 dB |
  | Hard clip, 24 dB, 5 kHz, **no ADAA** | −33 dB | −44 dB |
  | Triangle fold, 24 dB | −43 / −36 / −11 dB | −57 / −36 / −44 dB |

  ADAA1 buys about 20 dB at equal oversampling; 2× + ADAA1 is better than Prisma's Quality On (2×, no
  ADAA) and far better than its Off. It is not alias-free at full drive, and no folder is on a 9 kHz
  sine. What keeps the pedal clean in practice is the split itself: a shaper in the low or low-mid band
  only sees content below its crossover (a 500 Hz partial at 48 dB of clipping aliases below −70 dB at
  2×), and the top band is the one presets leave clean or light. If the bench hears fizz on a driven top
  band, the upgrade path is ADAA2 on the clip / tanh family or 8× on Quality On (open, bench).""")
r("""- Why not oversample per band: four up / down pairs cost what one pair plus the crossovers at the higher
  rate cost [A, §2.8], and a per-band choice would give bands different group delays (combs at the
  crossovers) unless the clean bands paid for an identity path anyway.""",
"""- Why not oversample per band: four up / down pairs cost what one pair plus the crossovers at the higher
  rate cost [A, §2.7], and a per-band choice would give bands different group delays (combs at the
  crossovers) unless the clean bands paid for an identity path anyway. For the same reason Quality Off
  never drops to 1× for a low band (open question 6).
- **`quality` and `slope` are set-up switches, not performance gestures**: the other engine is started,
  run for 30 ms beside the playing one (its crossovers and DC blocks settle; Ring mod and Sample divide
  phases are copied), then crossfaded over 10 ms. A cold crossfade would let a 112 Hz crossover ring in
  under the fade.""")

# 2.2
r("""  magnitude flat, phases of adjacent bands equal at every frequency.""",
"""  magnitude flat, phases of all four bands equal at every frequency. Checked [P] on the analog
  prototypes and on the TPT forms at 96 kHz, at the default crossovers, at minimum spacing and at both
  ends of the range, LR4 and LR8: sum flat within 1e-6 dB, band phases equal within 1e-15 rad. The
  identity needs every section of one crossover to share one coefficient set, updated on the same
  sample; it is exact in steady state and approximate while a crossover moves.""")
r("""  Butterworth TPT SVF sections per LP / HP, its all-pass = one 2nd-order section. LR8 = four sections,
  all-pass = two. LR8 halves the region where a partial is in two bands at once (less "double distortion"
  at the crossover, below) at twice the dispersion.""",
"""  Butterworth TPT SVF sections per LP / HP, its all-pass = one 2nd-order section (Q = 1/√2). LR8 = four
  sections (Q 0.541, 1.307, twice), all-pass = two (Q 0.541, 1.307). The first section of a split gives
  its low-pass and its high-pass from one SVF tick: 3 ticks per LR4 split, 7 per LR8 split. LR8 halves
  the region where a partial is in two bands at once (less "double distortion" at the crossover, below)
  at about twice the dispersion.""")
r("""- **Laws**: x_k = 0 – 100 %, **f = 20 · 1000^(x/100) Hz** (Prisma's, kept: §3.2). Ordering as Prisma:
  x2 ≥ x1 + 2 %, x3 ≥ x2 + 2 % (0.2 octave), the later crossover yields. Smoothed in the log-frequency""",
"""- **Laws**: x_k = 0 – 100 %, **f = 20 · 1000^(x/100) Hz** (Prisma's, kept: §3.2). Ordering as Prisma:
  x2 ≥ x1 + 2 %, x3 ≥ x2 + 2 % (0.2 octave), the later crossover yields; effective values are held in
  Prisma's windows (x1 2 – 94 %, x2 4 – 96 %, x3 6 – 98 %: 23 Hz – 17.4 kHz), so three crossovers always
  fit. The parameters keep 0 – 100 %; the effective frequency is what is displayed. Smoothed in the log-frequency""")
r("""- **Dispersion** [A]: the sum delays low frequencies by the three all-passes' group delays,
  2√2 / (2π f_c) each at DC for LR4: **4.0 + 0.7 + 0.1 = 4.9 ms** at the default crossovers (112 / 632 /
  3,557 Hz), under 0.2 ms above 4 kHz. That is the price of zero latency (Multipass and Saturn's default""",
"""- **Dispersion** [A, confirmed P]: the sum delays low frequencies by the three all-passes' group delays,
  2√2 / (2π f_c) each at DC for LR4: **4.0 + 0.7 + 0.1 = 4.9 ms** at the default crossovers (112 / 632 /
  3,557 Hz), peaking at 5.7 ms near 73 Hz; 0.53 ms at 1 kHz, 0.13 ms at 4 kHz. LR8: 9.0 ms at DC, a
  12.6 ms peak at 100 Hz, 0.32 ms at 4 kHz. Crossovers packed at minimum spacing around 112 Hz: 12.6 ms
  (LR4), 28 ms (LR8). That is the price of zero latency (Multipass and Saturn's default""")
r("""  benefit (a null instead of an all-pass when every band is clean) does not survive the first driven
  band. Open question 4 asks the reviewer to confirm.""",
"""  benefit (a null instead of an all-pass when every band is clean) does not survive the first driven
  band. Confirmed at review (open question 4).""")

# 2.3
r("""- **DC block** on every band's wet, always on (2nd-order high-pass at 5 Hz): Prisma switched it with the
  module list and reset its state, a click source. Always on costs 4 SVF ticks and is inaudible.
- **Per-band mix**: out_b = gain_b · (dry_b · (1 − m_b) + wet_b · m_b), m_b = `wet` · `<b>_mix`. The
  dry is the band after the crossover, aligned (§2.1): the blend never combs.""",
"""- **DC block** on every band's wet, always on (2nd-order high-pass at 5 Hz): Prisma switched it with the
  module list and reset its state, a click source. Always on costs 4 SVF ticks. It sits on the wet, before
  the mix and the gain, so a band's DC (Rectify at 100 %: 0.64 × peak) never meets a moving gain or mix
  (no thump on a solo, a mute or an EXP sweep). Its phase lead (10° at 40 Hz) is the same on every band's
  wet; against a band's own dry it costs under 0.1 dB at 20 Hz. With every slot Off the pedal is the
  all-pass of §2.2 times this high-pass (the bench reference includes it).
- **Per-band mix**: out_b = gain_b · (dry_b · (1 − m_b) + wet_b · m_b), m_b = `wet` · `<b>_mix`. The
  dry is the band after the crossover, aligned (§2.1): **the blend has no latency comb**. What a module
  does to the phase is its own sound and stays: a Lowpass, Highpass, Peak or Cabinet shifts the wet
  against the dry; Sample divide delays it by half a hold period (first notch at the hold rate when
  blended 50 / 50); a folder's fundamental changes sign as the drive rises. The memoryless saturators
  (Soft, Hard, Heavy, Tube, Rectify, Zero cross, Bit crush) blend without notches.""")
r("""- **Solo / mute / bypass** per band (expert, Prisma's own set): solo any band → the others are muted;
  mute → band gain 0; bypass → the band passes its dry, gain 0 dB. 5 ms raised-cosine ramps.""",
"""- **Solo / bypass** per band (expert): solo any band → the others are muted; bypass → the band passes
  its dry, gain 0 dB. 5 ms raised-cosine ramps. Prisma's per-band mute is not carried: `<b>_gain` at
  −60 dB is a mute, and solo covers the set-up use.""")

# 2.4 table and notes
r("**Tube** and **Fuzz** are appended (enums only grow at the end).",
  "**Tube** and **Fuzz** are appended (enums only grow at the end). Every law is continuous from 0 %: no\njump when a value leaves 0 (Prisma skips a module at v = 0).")
r("| 0 | Off | — | — | | HalfSample (alignment) |", "| 0 | Off | — | — | | HalfSample (alignment). The linear modules below (11 – 18, 20 – 22) pass a HalfSample too |")
r("| 7 | Sine fold | d = 0.48 v dB; y = sin(π/2 · g x) | dB | ✓ | ADAA1 (F = −2 cos(π g x / 2) / (π g)); soft saturation at 0 dB, folds above |",
  "| 7 | Sine fold | d = 0.48 v dB; y = sin(π/2 · g x) | dB | ✓ | ADAA1 (F = −2 cos(π g x / 2) / (π g)); soft saturation at 0 dB, folds above. Same fold count as Fold at the same dB (peaks at g x = 1) |")
r("| 8 | Zero cross | dead zone T = −90 + 0.84 v dBFS (−90 – −6 dBFS); y = sgn x · max(\\|x\\| − T, 0) with a 6 dB soft knee | dBFS | ✓ | ADAA1. Class-B crossover distortion |",
  "| 8 | Zero cross | dead zone T = −66 + 0.60 v dBFS (−66 – −6 dBFS); y = sgn x · max(\\|x\\| − T, 0) with a 6 dB soft knee (gain −6 dB at T + 6 dB) | dBFS | ✓ capped | ADAA1. Class-B crossover distortion; above T ≈ the signal's peaks it is a gate (a −18 dBFS sine is silent from 80 %) |")
r("| 9 | Bit crush | bits = 16 − 0.15 v (16 – 1, fractional: step 2^(1 − bits)); Tread rounding (silence stays 0) | bits | ✓ | ADAA1 on the staircase (exact F). gfx-crusher's continuous-bits law |",
  "| 9 | Bit crush | bits = 16 − 0.15 v (16 – 1, fractional: step 2^(1 − bits)); Tread rounding (silence stays 0) | bits | | ADAA1 on the staircase (exact F). gfx-crusher's continuous-bits law. Within 1.2 dB of unity down to 4 bits at the reference level; below 3 bits a −18 dBFS signal rounds to silence (the gate-crush sound, as in Prisma): a Gain in slot 1 pushes the band into the bits |")
r("| 10 | Sample divide | hold rate R = 40 · 1000^(1 − v/100) Hz (kept; 40 Hz – 40 kHz) | Hz | ✓ | gfx-crusher's fractional phase accumulator (f64), naive hold at the inner rate: the downsampler removes the images above fs / 2, the aliases below are the effect |",
  "| 10 | Sample divide | hold rate R = 40 · 1000^(1 − v/100) Hz (kept; 40 Hz – 40 kHz) | Hz | | gfx-crusher's fractional phase accumulator (f64). The held value is read at the fractional hold instant (linear interpolation); the inner sample that contains the edge takes the area mean of the old and new levels. That is ADAA1 of the staircase: the same half-sample delay as the other modules, and no edge jitter at the inner rate. The downsampler removes the images above fs / 2; the aliases below are the effect |")
r("| 11 | DC | offset = v/100 − 0.5 in full-scale units, ±0.5 (neutral 50 %) | % | | Biases the next slot; DC block after |",
  "| 11 | DC | offset = v/100 − 0.5 in full-scale units, ±0.5 (neutral 50 %) | % | | Biases the next slot: slot 1 only. In slot 2 the DC block removes it (inert) |")
r("| 13 | Gain | −24 + 0.48 v dB (−24 – +24, neutral 50 %) | dB | | |",
  "| 13 | Gain | −24 + 0.48 v dB (−24 – +24, neutral 50 %) | dB | | Not a mute: `<b>_gain` does that |")
r("| 17 | Peak | f = 20 · 1000^(v/100) Hz (kept), Q 1.5, gain = `<b>_y<n>` (−30 – +30 dB, new, §3) | Hz | | SVF bell |",
  "| 17 | Peak | f = 20 · 1000^(v/100) Hz (kept), Q 1.5, gain = `<b>_y<n>` (−30 – +30 dB, default +9 dB, new, §3) | Hz | | SVF bell |")
r("| 20 | Stereo DC | offset = 0.5 · (v/100) · (c − ½) (linear) | % | | Stereo only |",
  "| 20 | Stereo DC | offset = (v/100) · (c − ½), c = 0 left, 1 right: ±0.5 at 100 % (linear) | % | | Stereo only |")
r("| 22 | Cabinet | 6 voicings, index round(5 v/100) (kept stepping): 4×12 closed, 4×12 bright, 2×12 open, 1×12 combo, 1×15 bass, 8″ small | name | normalised | **Minimum-phase biquad voicings** (low resonance, presence, high roll-off), no convolution, zero latency; Prisma's IRs are not used (§7.7) |",
  "| 22 | Cabinet | 6 voicings, index round(5 v/100) (kept stepping), in the order of Prisma's roles: 0 4×12 closed (dark), 1 1×15 bass, 2 4×12 bright, 3 8″ small, 4 2×12 open, 5 1×12 combo | name | normalised | **Minimum-phase biquad voicings** (low resonance, presence, high roll-off), no convolution, zero latency; Prisma's IRs are not used (open question 7) |")
r("| 24 | **Fuzz** (new) | d = 0.48 v dB into gfx-drive's Fuzz core: two biased clip stages, a 40 Hz high-pass between them | dB | ✓ | ADAA1 on each stage |",
  "| 24 | **Fuzz** (new) | d = 0.48 v dB into gfx-drive's Fuzz core: two biased clip stages, a 40 Hz high-pass between them | dB | ✓ | ADAA1 on each stage: one inner sample, half a sample over the slot's allowance (accepted, §2.1); DC block after |")
r("""- **Tape** is not in the list: it is appended when gfx-tape (lot 2) has its engine, as a module calling
  that engine. Enum order makes that free.""",
"""- **Tape** is not in the list: it is appended when gfx-tape (lot 2) has its engine, as a module calling
  that engine. An append is free for the plugin, not for golem: golem writes an enum as index / (n − 1)
  from the catalogue's option list (contract, REAPER normalization), so **the catalogue's eight option
  lists must hold exactly the plugin's options**. 25 at the first release; each later append is a
  catalogue change made with the plugin release.
- **Soft clip and Hard clip at 0 % are not Off**: unity gain into the shaper (tanh compresses a peak at
  −3 dBFS by 1.3 dB; Hard clip limits at 0 dBFS in the band).
- **Licence hygiene**. Prisma is AGPL-3.0-or-later; golem-fx is GPL-3. What is taken from it is its
  parameter laws, restated as formulas in §1.1 (facts needed to convert golem's presets). The modules
  are written from this document, not from Prisma's source: no source file, table, GUI asset or impulse
  response of Prisma enters the repository, its make-up factors (0.92, 0.86) are replaced by §2.6, and
  the bench's "Prisma" reference formulas are typed from §1.1. The designer's clone stays in a scratch
  directory.""")

# 2.5
r("""- **`drive`** (shared id, 0 – 100 %, extra saturation on top of the modules' own): adds **drive / 2
  points** to the Amount of every nonlinear module (✓ in the table, plus Sample divide and Bit crush),
  clamped at 100. Each module then moves along its own law: more dB, fewer bits, a lower hold rate, a
  wider dead zone. Linear modules are untouched, so a band holding only a filter never changes.
- **`focus`** (−100 – +100 %): adds **25 · focus · w_b points**, w = (−1, −⅓, +⅓, +1) for (lo, lm, hm, hi):
  −100 % moves the dirt toward the lows, +100 % toward the highs. Same clamp, same modules.""",
"""- **`drive`** (shared id, 0 – 100 %, extra saturation on top of the modules' own): adds
  **k_type · drive / 100 points** to the Amount of every nonlinear module, clamped at 100. Each module
  then moves along its own law. Linear modules are untouched, so a band holding only a filter never
  changes.

  | Module | k_type | Full Drive adds |
  |---|---|---|
  | Soft clip, Hard clip, Fold, Sine fold, Tube, Fuzz | 50 | +24 dB |
  | Heavy, Asym | 50 | exponent −0.495 |
  | Rectify | 50 | +50 % |
  | Zero cross | 20 | dead zone +12 dB |
  | Bit crush | 20 | −3 bits |
  | Sample divide | 20 | hold rate −2 octaves |

  One weight for all (the first draft's 50 points) would have taken a Bit crush from 8 bits to 0.5, a
  Sample divide down five octaves and a Zero cross up 30 dB: on three of the four catalogue presets a
  Drive sweep ended in silence or in a 240 Hz hold. The weights are constants [I: to trim by ear], not
  parameters.
- **`focus`** (−100 – +100 %): adds **k_type · focus · w_b / 200 points**, w = (−1, −⅓, +⅓, +1) for
  (lo, lm, hm, hi): −100 % moves the dirt toward the lows, +100 % toward the highs (±12 dB on a
  saturator in the outer bands). Same clamp, same modules.""")

# 2.6
r("""- Each compensated module carries a **static make-up c(type, v)**: the RMS ratio input / output of a
  −18 dBFS, 1 kHz sine through the shaper, tabulated at 65 points of v per type and interpolated. The
  tables are generated at build time (const data): nothing is computed when the plugin turns on.
- Static, not a follower: no pumping, no breathing on decays; a module at −18 dBFS sounds as loud at 0 %
  and 100 %, and louder playing is still louder. Always on (no toggle): band gains and `level` set the
  levels; Prisma's 0.92 factors were the same idea done by hand.
- Bit crush and Sample divide come out near 1 by nature; the table makes it exact.""",
"""- Each compensated module carries a **static make-up c(type, v)**: the RMS ratio input / output of a
  1 kHz sine peaking at −18 dBFS through the shaper, tabulated at 65 points of v per type and
  interpolated, **limited to −30 – +12 dB**. The tables are generated at build time (const data): nothing
  is computed when the plugin turns on. The make-up follows the Amount after Drive and Focus.
- Static, not a follower: no pumping, no breathing on decays. What it does and does not do [P, tanh]:

  | Drive | Make-up | Small-signal gain (noise floor) | A −6 dBFS input comes out |
  |---|---|---|---|
  | 0 dB | 0 dB | 0 dB | −0.5 dB re input |
  | 12 dB | −11.5 dB | +0.5 dB | −4.4 dB |
  | 24 dB | −19.1 dB | +4.9 dB | −10.5 dB |
  | 48 dB | −20.9 dB | +27 dB | −12.0 dB (= the reference level) |

  At the reference level the loudness holds across Amount. Above about 24 dB of drive every input comes
  out near the reference level (that is saturation: the band is a square wave), and the quiet parts rise
  by the small-signal gain, as in any distortion. So "Amount is a tone control" is true at playing level;
  dynamics shrink with Amount, by nature. A square has more highs than a sine at equal RMS: expect + 2 to
  3 LU at full Amount (bench).
- **The reference level** is one constant for all modules. −18 dBFS peak is a first value [I]: the
  bench sets it from the per-band peak levels of the DIs (a band holds less than the full signal).
- **Not compensated**: Bit crush and Sample divide (unity by nature; a table measured where the
  quantiser gates would only add gain to silence). **Capped**: Zero cross, whose output vanishes when T
  reaches the signal (the +12 dB limit stops the table from chasing it).
- Always on (no toggle, open question 11): band gains and `level` set the levels; Prisma's 0.92 factors
  were the same idea done by hand.""")

# 2.7
r("""  (a few samples at the base rate: gfx-drive measured ~6 at 4×), one inner sample of ADAA alignment and
  the crossover dispersion (§2.2). gfx-bench: `skip_latency` (gfx-drive's precedent).""",
"""  (a few samples at the base rate: gfx-drive measured ~6 at 4×), one inner sample of ADAA alignment
  (half a base sample at 2×) and the crossover dispersion (§2.2). gfx-bench: `skip_latency` (gfx-drive's
  precedent).""")
r("""  Stereo doubles it. For scale: gfx-drive 736 ns stereo, gfx-reverb 315 ns. The worst case is 6 % of a
  core: accepted for a pedal whose presets use two to four slots.""",
"""  Stereo doubles it. For scale: gfx-drive 736 ns stereo, gfx-reverb 315 ns. The worst case is 6 % of a
  core: accepted for a pedal whose presets use two to four slots. The crossover column is an upper
  bound: with the shared first section the tree is 12 ticks (LR4) or 27 (LR8) per inner sample, and the
  three `tan()` of the coefficients are computed only while a crossover moves. Heavy and Asym cost one
  `powf` per inner sample (≈ 30 ns [I]), more than tanh.""")

# 2.8
r("| `oversample` | The one up / down pair (2× and 4×), `identity` not needed (every band is inside) |",
  "| `oversample` | The one up / down pair (2× and 4×), `identity` not needed (every band is inside); the droop correction (§2.1) lives beside `HalfSample` in `adaa` |")
r("| gfx-drive's shapers | Hard clip and Fuzz antiderivatives (`clip_f` / `clip_af`, the two-stage Fuzz core) — **moved to gfx-dsp as `shapers`**, then used by both crates |",
  "| gfx-drive's shapers | Hard clip and Fuzz antiderivatives (`clip_f` / `clip_af`, the two-stage Fuzz core, the `Adaa` whose shaper may change every sample) — **moved into gfx-dsp `adaa`** as new `Shaper` variants (Clip, the tanh → clip blend) and a `FuzzCore`, with no change of sound for gfx-drive (its tests must still null); gfx-bassdi and gfx-grit then use the same code. The grit-only curves (Heavy, Asym, Rectify, Fold, Sine fold, Zero cross, staircase) stay in the crate |")
r("| gfx-crusher's `converter` (designed, not coded) | Continuous-bits Tread quantiser, fractional clock. If gfx-grit is coded first, these two small functions live in gfx-grit and move to gfx-dsp with gfx-crusher |",
  "| gfx-crusher's `converter` (designed, not coded) | Only its two laws: continuous bits with Tread rounding (step 2^(1 − bits)) and the f64 fractional clock. Not its engine: gfx-crusher samples and renders through a minimum-phase BLEP kernel, which a band-local module does not need inside 2× / 4× + ADAA. The two functions are a few lines; they live in gfx-grit and are not a shared module |")
r("| **New: `crossover`** (gfx-dsp) | N-band Linkwitz-Riley tree (LR4 / LR8) with compensation all-passes, log-smoothed moving crossovers. Replaces the ad-hoc LR splits in gfx-drive, gfx-chorus, gfx-emet, gfx-flanger over time; next user: the multiband compressor (lot 3) |",
  "| **New: `crossover`** (gfx-dsp) | **One module, shared with gfx-bassdi** (which proposes the same name), in three layers. (1) `LrSplit`: one crossover point, order 4 or 8, low-pass and high-pass from shared SVF sections, and `LrAllpass`, its twin (same frequency, same rate, same coefficient update), with the log-domain frequency smoother. (2) `LrTree<N>`: the N-band tree with compensation all-passes (gfx-grit, then the lot-3 multiband compressor). (3) gfx-bassdi's band-gain sections S(g, f) = g · LP + HP and the `bass_keep` sum, built on (1). One test file holds the identities (LP + HP = AP, equal phase, flat tree sum). Whichever crate is coded first creates layer (1); gfx-drive, gfx-emet, gfx-chorus and gfx-flanger adopt it later |")
r("| **New in the crate** | The 25 modules, the compensation tables (build-time generator), slot crossfades |",
  "| **New in the crate** | The 25 modules, the compensation tables (build-time generator), slot crossfades, the Drive / Focus weights |")
t0=t
for a,b in R:
    n=t.count(a)
    if n!=1: print("COUNT",n,"for:",a[:90]); sys.exit(1)
    t=t.replace(a,b)
open(p,'w').write(t); print("ok",len(R))
