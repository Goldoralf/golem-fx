import os
import sys
p=os.path.expanduser('~/code/golem-fx/gfx-grit/README.md')
t=open(p).read()
R=[]
def r(a,b): R.append((a,b))
r("(a 500 Hz partial at 48 dB of clipping aliases below −70 dB at\n  2×)", "(a 500 Hz partial at 48 dB of clipping aliases at −58 dB at 2×,\n  −87 dB at 4×)")

# 3.1
r("module enums keep the catalogue's 23 options in order; Tube and Fuzz are appended.",
  "module enums keep the catalogue's 23 options in order; Tube and Fuzz are appended, and the catalogue's\noption lists gain them at the same time (§2.4).")
# 3.2
r("| Sine fold | **v′ = 20 log₁₀(35 v² + 1) / 48** (by intent: the curve itself changes, §2.4) | |",
  "| Sine fold | **v′ = min(5 v², 1) · 20 log₁₀(70 v² + 2) / 48** | Prisma's sin(π g x) peaks at g x = ½, ours at g x = 1: the same fold count needs twice the gain (the draft's 35 v² + 1 was 6 dB short). The first factor stands for Prisma's fade-in of the sine below v = 0.45. By intent: the curve itself changes |")
r("| Zero cross | **v′ = (200 log₁₀ v + 90) / 84** | Threshold v¹⁰ = 200 log₁₀ v dBFS onto −90 – −6 dBFS |",
  "| Zero cross | **v′ = (T(v) + 66) / 60**, T(v) in dBFS interpolated linearly in: v 0.05 → −67, 0.10 → −46, 0.20 → −34, 0.30 → −30, 0.40 → −27, 0.50 → −25, 0.60 → −22.5, 0.70 → −20, 0.80 → −16, 0.90 → −11, 0.95 → −8.5, 1 → −6 | T = the level where Prisma's whole curve has −6 dB of gain, minus 6 dB [P: computed from the law of §1.1]. The draft converted the v¹⁰ clip alone: Prisma's 23 % (audible: −6 dB at −27 dBFS) became a −90 dBFS dead zone, i.e. nothing |")
r("| Gain | **v′ = (40 log₁₀(v / 0.35) + 24) / 48** | dB linear, neutral at 50 % |",
  "| Gain | **v′ = (40 log₁₀(v / 0.35) + 24) / 48** | dB linear, neutral at 50 %. Prisma's v < 0.088 (below −24 dB, v = 0 a mute) clamps to −24 dB |")
r("| Stereo DC | **v′ = v⁵** | Linear offset |", "| Stereo DC | **v′ = v⁵** | Linear offset, ±0.5 at 100 % as Prisma's |")
r("""  conversions; the catalogue entry changes `*_gain` to dB (min −60, max 12, default 0) and its notes.""",
"""  conversions; the catalogue entry changes `*_gain` to dB (min −60, max 12, default 0), its option
  lists (25) and its notes ("Keep Quality OFF live" no longer applies: both settings report 0). The
  `*_v` unit stays %; the real value (dB, bits, Hz) is the plugin's formatted string, which golem may
  show.""")
# 3.3
r("lm Hard clip 42 % (20.4 dB); hm Heavy 55 % (e = 0.455), Peak 60 % (1.26 kHz), `hm_y2` **0 dB (faithful: inert today)**; lo −40 dB, hi −20.9 dB; 590 Hz / 2.52 kHz / 4.08 kHz. *Re-author*: `hm_y2` +9 dB, the peak the note describes |",
  "lm Hard clip 42 % (20.4 dB); hm Heavy 55 % (e = 0.455), Peak 60 % (1.26 kHz), `hm_y2` **+9 dB (the default: the peak the preset's note describes; in Prisma it was inert, a bug of the headless use)**; lo −40 dB, hi −20.9 dB; 590 Hz / 2.52 kHz / 4.08 kHz |")
# 3.4
r("| `drive` | Drive | f | 0 – 100 % | new (shared id) | 0 % | 20 ms | ✓ | +0 – 50 points on every nonlinear module's Amount (§2.5) |",
  "| `drive` | Drive | f | 0 – 100 % | new (shared id) | 0 % | 20 ms | ✓ | Up to +50 points (saturators: +24 dB) or +20 points (Bit crush, Sample divide, Zero cross) on every nonlinear module's Amount (§2.5) |")
r("| `focus` | Focus | f | −100 – +100 % | new | 0 % | 20 ms | ✓ | Tilts the dirt toward the lows (−) or the highs (+), ±25 points at the outer bands |",
  "| `focus` | Focus | f | −100 – +100 % | new | 0 % | 20 ms | ✓ | Tilts the dirt toward the lows (−) or the highs (+), ±25 points (saturators) at the outer bands |")
r("| `quality` | Quality | b | off / on | same | off | 10 ms crossfade | | Off = 2× + ADAA, On = 4× + ADAA; both reported latency 0 |",
  "| `quality` | Quality | b | off / on | same | off | 30 ms pre-roll, 10 ms crossfade | | Off = 2× + ADAA, On = 4× + ADAA; both reported latency 0 |")
# 3.6
r("| `<b>_y1`, `<b>_y2` | \\<Band\\> Module 1 / 2 Peak Gain | f | −30 – +30 dB | 0 dB | 20 ms | Gain of a Peak module in that slot (Prisma's GUI-only Y, now reachable); inert for other types |",
  "| `<b>_y1`, `<b>_y2` | \\<Band\\> Module 1 / 2 Peak Gain | f | −30 – +30 dB | **+9 dB** | 20 ms | Gain of a Peak module in that slot (Prisma's GUI-only Y, now reachable, and signed: a cut before a clipper is a scoop); inert for other types. A constant default, so a Peak is heard as soon as it is chosen |")
r("| `<b>_solo` | \\<Band\\> Solo | b | off / on | off | 5 ms | Hear one band (set-up); several soloed = their sum |\n| `<b>_mute` | \\<Band\\> Mute | b | off / on | off | 5 ms | Band off |\n",
  "| `<b>_solo` | \\<Band\\> Solo | b | off / on | off | 5 ms | Hear one band (set-up); several soloed = their sum |\n")
r("| `slope` | Crossover Slope | e | 24 dB/oct · 48 dB/oct | 24 dB/oct | 20 ms crossfade | LR4 (Prisma's) or LR8 (§2.2) |",
  "| `slope` | Crossover Slope | e | 24 dB/oct · 48 dB/oct | 24 dB/oct | 30 ms pre-roll, 10 ms crossfade | LR4 (Prisma's) or LR8 (§2.2) |")
r("""No default depends on another parameter. Values that differ per golem use are presets (§4). 54
parameters in all (25 catalogue, 29 new).""",
"""No default depends on another parameter. Values that differ per golem use are presets (§4). **50
parameters in all (25 catalogue, 25 new)**: the largest set in the suite, but 40 of them are the 4 × 10
of the bands, and golem's Designer shows the catalogue's 25 plus the face. A per-band mute can be added
later (ids are permanent, additions are free).""")
r("""`m`, `v`, `gain`, `mix`, `y`, `solo`, `mute`, `bypass`), **Crossovers** (`x1` – `x3`, `slope`).""",
"""`m`, `v`, `gain`, `mix`, `y`, `solo`, `bypass`), **Crossovers** (`x1` – `x3`, `slope`).""")
# 4
r("| **Megaphone** (voice) | as §3.3, `hm_y2` +9 dB | Hard clip on the low mids, highs and lows pulled down |",
  "| **Megaphone** (voice) | as §3.3 (`hm_y2` +9 dB) | Hard clip on the low mids, a peak at 1.26 kHz, highs and lows pulled down |")
r("| **Tight Kick** (Prisma's own idea, in golem terms) | x1 30 %; lo Zero cross 45 % (dead zone −52 dBFS) → Gain 60 % (+4.8 dB) | Crossover distortion on the kick's body only: punch, short tail |",
  "| **Tight Kick** (Prisma's own preset, folded into two slots) | x1 30 %; lo Zero cross 65 % (dead zone −27 dBFS) → Gain 55 % (+2.4 dB) [A from §1.1 finding 6; by ear] | A dead zone on the kick's band only: the tail falls into it, the hit passes. Punch, short tail |")
r("| **Drum Bus Crunch** | lo Off; lm Heavy 30 %; hm Soft clip 35 %; hi Bit crush 50 % (8.5 bits); `wet` 60 % | Parallel crunch, kick untouched |",
  "| **Drum Bus Crunch** | lo Off; lm Heavy 30 %; hm Soft clip 35 %; hi Bit crush 50 % (8.5 bits); `wet` 60 % | Parallel crunch, kick untouched (all three are memoryless: the blend has no notch) |")
r("| **EXP: edge to wreck** | any preset, EXP on `drive` 0 → 100 % | One sweep moves every module along its own law, level-matched |",
  "| **EXP: edge to wreck** | any preset, EXP on `drive` 0 → 100 % | One sweep moves every module along its own law, each by its weight (§2.5); saturators stay level-matched |")
# 5
r("silence floor −140 dBFS (Tread rounding, the always-on DC block and Heavy's knee\nmake silence decay to 0).",
  "silence floor −140 dBFS (Tread rounding, the always-on DC block and Heavy's knee\nmake silence decay to 0). Bounds marked [P] come from the review's numpy prototype with an ideal\ndownsampler; the bench re-measures them with the real half-bands and then fixes them.")
r("| Clean sum | Every slot Off, gains 0 dB, `wet` 100 %, both slopes, crossovers at defaults, at min spacing (2 %) and at 20 Hz / 20 kHz: magnitude 20 Hz – 20 kHz flat ±0.02 dB; output nulls ≤ −80 dB against the analytic all-pass cascade A(x1)A(x2)A(x3) at the base rate (half-band group delay compensated in the reference) |",
  "| Clean sum | Every slot Off, gains 0 dB, `wet` 100 %, both slopes, both qualities, crossovers at defaults, at min spacing (2 %) and at both ends of their windows (2 / 4 / 6 % and 94 / 96 / 98 %): magnitude 20 Hz – 20 kHz flat ±0.1 dB at 2×, ±0.05 dB at 4× (droop correction residual, §2.1 [P]; the half-bands' own pass-band added); output nulls ≤ −60 dB against the reference = all-pass cascade A(x1)A(x2)A(x3) × the 5 Hz DC block × the measured response of the same engine with crossovers removed (half-bands, averages, correction) |\n| Droop | The same with the correction switched off in a test build: −0.95 dB at 10 kHz and −4.0 dB at 20 kHz at 2× ±0.1 dB (proves the correction is doing the work, not the tolerance) |")
r("| Coherence | One band driven (Soft clip 0 %), the others Off: the sum still flat ±0.1 dB at −40 dBFS (tanh linear there); `wet` 50 %, `<b>_mix` 50 %: no notch deeper than 0.5 dB anywhere |",
  "| Coherence | One band on Soft clip 0 %, the others Off: the sum still flat ±0.1 dB at −40 dBFS (tanh linear there); `wet` 50 %, `<b>_mix` 50 %: no notch deeper than 0.2 dB anywhere. Same with Fuzz 0 % in one band: ≤ 0.6 dB at 20 kHz at 2× (the accepted half sample) |")
r("| Conversions | The test embeds Prisma's formulas: for each converted type, 101 values of v → v′ → our law gives Prisma's dB / bits / dBFS within 0.01; the four catalogue presets convert to the §3.3 table |",
  "| Conversions | The test holds Prisma's formulas, typed from §1.1: for each converted type, 101 values of v → v′ → our law gives Prisma's dB / bits within 0.01 (where v′ is not clamped); Zero cross: our −6 dB level within 2 dB of Prisma's curve for v in 0.1 – 0.95; Sine fold: same fold count (zero crossings of a ramp) for v ≥ 0.45; the four catalogue presets convert to the §3.3 table |")
r("| Aliasing | 2, 5, 9 kHz sines at −6 dBFS in the band that holds them, Amount 100 %, 48 kHz: Quality Off — aliased components ≤ −80 dBFS for Soft / Hard / Tube / Heavy / Asym / Rectify / Zero cross, ≤ −65 dBFS for Fold / Sine fold / Fuzz; Quality On: ≤ −90 / −78 dBFS [A: from gfx-drive's measurements, to confirm]. Bit crush and Sample divide: excess over their ideal model ≤ −70 dB (gfx-crusher's method, simplified) |",
  "| Aliasing | 2 / 5 / 9 kHz sines at −6 dBFS in the band that holds them, 48 kHz, strongest aliased component below 20 kHz relative to the output fundamental [P, 4 dB of margin]. Soft / Hard / Tube at Amount 50 %: Off ≤ −55 / −48 / −33 dB, On ≤ −75 / −60 / −63 dB; at 100 %: Off ≤ −40 / −38 / −30 dB, On ≤ −55 / −46 / −44 dB. A 500 Hz sine in the low-mid band, Hard clip 100 %: Off ≤ −54 dB, On ≤ −80 dB. Every ADAA module: ≥ 15 dB better than the same curve without ADAA at the same factor (5 kHz, Amount 50 %). Heavy / Asym / Rectify / Zero cross / Fold / Sine fold / Fuzz: reported, then bounded from the first measurement. Bit crush and Sample divide: excess over their ideal model ≤ −70 dB (gfx-crusher's method, simplified). The draft's −80 / −90 dBFS at Amount 100 % was 40 – 50 dB beyond what 2× / 4× + ADAA1 can do |")
r("| Level | Each compensated module, Amount 0 → 100 %, −18 dBFS sine and guitar DI at −18 dBFS RMS: loudness within ±1 dB (sine) / ±3 dB (DI); `drive` 0 → 100 % on the Grit preset within ±3 dB |",
  "| Level | Each compensated module (Zero cross up to T = −30 dBFS only), Amount 0 → 100 %: RMS within ±0.5 dB on the reference sine (the table itself), loudness within ±3 LU on the sine and on a guitar DI whose band peaks sit at the reference; `drive` 0 → 100 % on Megaphone within ±3 LU. Make-up never above +12 dB (Zero cross 100 %, Bit crush 1 bit: output silent, no gain on the noise) |\n| Drive weights | `drive` 100 % on Grit, Telephone and Broken: output still above −40 dBFS RMS on the drum loop (no preset driven to silence); Sample divide moves 2 octaves, Bit crush 3 bits ±1 % |")
r("| Clicks | Slot type changes under a held note, Cabinet voicing steps, crossover sweep by EXP 50 Hz steps, solo / mute / bypass toggles: HF burst ≤ −30 dB re peak; crossover EXP sidebands ≤ −80 dB (contract) |",
  "| Clicks | Slot type changes under a held note, Cabinet voicing steps, crossover sweep by EXP 50 Hz steps, solo / bypass toggles and `<b>_gain` 0 → −60 dB with Rectify 100 % in the band (DC must not thump): HF burst ≤ −30 dB re peak; crossover EXP sidebands ≤ −80 dB (contract) |")
r("| Quality switch | Off ↔ On under a note: no gap, crossfade only; reported latency 0 both ways |",
  "| Quality and slope switches | Off ↔ On and 24 ↔ 48 under a held 82 Hz note: no gap, no burst above −30 dB re peak (pre-rolled engine, §2.1); reported latency 0 both ways |")
r("| Intermodulation (report) | Power chord (E2 + B2) and an open E major chord: one band Soft clip 30 dB (all crossovers at 20 kHz) vs",
  "| Intermodulation (report) | Power chord (E2 + B2) and an open E major chord: one band Soft clip 30 dB (crossovers at 94 / 96 / 98 %) vs")
# visual
r("""- **Category: `lofi`** (the catalogue's). Body hue: the family's — `METAL_HUE.lofi` `#e8336f`
  (raspberry), as proposed by gfx-crusher; **no crate hue**: the motif sets it apart from gfx-crusher, as
  gfx-freqshift is set apart in the pitch family.""",
"""- **Category: `lofi`** (the catalogue's). Body hue: the family's — `METAL_HUE.lofi` `#e8336f`
  (raspberry), as proposed by gfx-crusher; **no crate hue**: the motif sets it apart from gfx-crusher, as
  gfx-freqshift is set apart in the pitch family.
- **Hue clash with gfx-swell, resolved at review (proposal for the maintainer)**. `#e8336f` and gfx-swell's crate
  hue `#d81e5b` are the same hue (340.1° and 340.3°). The gap they both chose, pitch's magenta (319°) to
  the thrash drive red (3°), is 44° wide: it holds one hue 20° from its neighbours, not two.
  - **lofi keeps raspberry**: a family hue carried by two pedals (gfx-crusher, gfx-grit; gfx-tape may
    join) outranks one crate's hue, and its thrash finish needs the distance from drive's red.
  - **gfx-swell moves to orchid `#bf1dd7`** (292°, saturation 76 % as the other metal hues): the middle of
    the next free gap, 27° from reverb's violet (265°) and from pitch's magenta (319°), more room than
    raspberry has. It stays anodized, with its `swells` motif. To note in gfx-swell's README (not edited
    here: `METAL_CRATE_HUE['gfx-swell']` and its face `color`).
  - Fallback if orchid reads as "pitch" at tile size: gfx-swell drops its crate hue for the dynamics
    slate and keeps a rose `RUNE_LIGHT`. Decided on the family sheet.""")
t0=t
for a,b in R:
    n=t.count(a)
    if n!=1: print("COUNT",n,"for:",a[:90]); sys.exit(1)
    t=t.replace(a,b)
open(p,'w').write(t); print("ok",len(R))
