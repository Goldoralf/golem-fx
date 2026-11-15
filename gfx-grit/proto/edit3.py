import os
p=os.path.expanduser('~/code/golem-fx/gfx-grit/README.md')
t=open(p).read()
i=t.index("## Open questions for review")
new = """## Open questions for review

Each answered at the review of 2026-11-14.

1. **Units and conversion** (§3.2). **Decided: band gains in dB, module values converted once, keyed by
   the slot's type**; three conversions were corrected (Zero cross, Sine fold, Stereo DC's scale).
   Whether golem-rack's engine table can key a conversion on another parameter stays its call; the
   fallback (Prisma's laws kept for the eight types) is written out.
2. **Peak**. **Decided: `<b>_y1/_y2` kept (signed, −30 – +30 dB), default +9 dB.** A constant default
   breaks no rule, a Peak is heard when chosen, and Megaphone gets the peak its note describes. A fixed
   gain would lose the cut (a scoop before a clipper).
3. **Band gain on the whole band**. **Accepted**: identical on the four presets, and a muted band that
   stays muted at any `wet` is what a player expects.
4. **Crossover dispersion**. **Accepted, no linear-phase mode.** 4.9 ms at DC (5.7 ms peak at 73 Hz) is
   below what a bass or a kick shows as a softened attack through a cab [I, bench]. The one use that
   needs care is a golem split lane summing gfx-grit against a dry branch: it combs at the crossovers
   whatever the settings. Catalogue note: "in a split, put gfx-grit on the summed lane or use `wet`".
5. **Slope default**. **24 dB / oct**: Prisma's, half the dispersion; 48 is the bass-split presets'.
6. **Quality Off = 2×**. **Kept, never 1× per band**: one rate for the whole split is what keeps the
   bands coherent, and the split already protects the low bands (§2.1).
7. **Cabinet**. **Biquad voicings confirmed**, in the order of Prisma's roles; no IR of Prisma is used
   (§2.4, licence hygiene). The slot may call the lot-3 cab engine later, under the same six names.
8. **Drive / Focus**. **Additive points kept, with a weight per module family** (§2.5): one weight
   drove three of the four catalogue presets to silence or to a 240 Hz hold.
9. **Hue**. **lofi keeps raspberry; gfx-swell is proposed orchid `#bf1dd7`** (Visual). The maintainer decides on
   the family sheet.
10. **Tube / Fuzz now, Tape later**. **Agreed.** The list need not be closed before release, but every
    append is a joint plugin + catalogue change (§2.4).
11. **Static compensation always on**. **Yes, no toggle**: band gains and `level` do the rest. Limited
    to −30 – +12 dB; Bit crush and Sample divide are left alone (§2.6).

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good**, Strong once the bench confirms the prototype's figures. The architecture is right
and holds under the numbers: the all-pass-compensated Linkwitz-Riley tree sums flat and equiphase for
LR4 and LR8 (prototype: 1e-6 dB, 1e-15 rad, analog and TPT, at every crossover placement), the 4.9 ms
is exact, one oversampling stage around the split is the correct way to keep bands coherent, and the
per-type unit work is the right reading of the 2026-11-14 rule. Prisma's source was re-read at
`5062b59`: the tree, the crossover law, the band-gain and wet laws, and the Soft / Hard / Heavy / Asym /
Rectify / Fold / Bit crush / Sample divide / DC / Gain / Ring mod laws are as the survey says, and the
§3.3 table reproduces. The design had one mechanism that defeated its own bench (the alignment's
droop), antialiasing bounds 40 – 50 dB out of reach, three wrong conversions, and two rules (Drive,
compensation) that broke on the gating modules. None touches the structure. Prototype scripts:
`scratchpad/proto-gfx-grit/` (`xo.py`, `adaa.py`, `zc.py`, `zcfit.py`).

Changes made, and why:

| # | Change | Why |
|---|---|---|
| 1 | **Droop correction after the band sum** (§2.1), bench rows Clean sum / Droop | `HalfSample` is a two-tap average, as ADAA1 is at small signals: two per path cost 4.0 dB at 20 kHz at 2× (0.95 dB at 10 kHz) on every band, every slot Off. The "flat ±0.02 dB" row could not pass. The droop is common to all paths, so one two-pole filter (a = 0.8) restores ±0.06 dB |
| 2 | **Alignment put in proportion; Fuzz's extra half sample accepted** | A half-sample mismatch costs 0.5 dB at 20 kHz in a 50 / 50 blend at 2×. Fuzz has two ADAA stages and cannot fit a one-average slot |
| 3 | **Antialiasing stated with figures; bench bounds reset** (§2.1, §5) | −80 / −90 dBFS at Amount 100 % is not reachable: a 9 kHz sine clipped by 48 dB aliases at −35 dB (2× + ADAA1) and −50 dB (4×). ADAA1 buys ≈ 20 dB over naive; the split protects the low bands. Upgrade path named (ADAA2 or 8×) |
| 4 | **"The blend never combs" → "no latency comb"** (§2.3) | True for the memoryless saturators only: filters, Cabinet, Sample divide (half a hold period) and folders (sign of the fundamental) shift the wet against the dry. That is their sound, but the claim was wrong |
| 5 | **Zero cross: law −66 – −6 dBFS, conversion by table** (§1.1, §2.4, §3.2) | Prisma's audible dead zone is its power term, not the v¹⁰ clip the draft converted: Prisma's 23 % became −90 dBFS (nothing). Its −6 dB point runs −40 dBFS at 10 % to −5 at 90 % |
| 6 | **Sine fold conversion: 70 v² + 2, with the fade-in factor** | sin(π g x) folds twice as fast as sin(π/2 · g x): the draft was 6 dB short |
| 7 | **Stereo DC scale**: ±0.5 at 100 % | The draft's 0.5 · v · (c − ½) was half Prisma's v⁵ · (c − ½) under v′ = v⁵ |
| 8 | **Drive / Focus weights per family** (§2.5), bench row | 50 points for all: Bit crush 8 → 0.5 bits, Sample divide −5 octaves, Zero cross +30 dB. Now +24 dB / −3 bits / −2 octaves / +12 dB |
| 9 | **Compensation**: limited to −30 – +12 dB, Bit crush and Sample divide uncompensated, Zero cross capped; behaviour table; reference level made a bench constant | At −18 dBFS a Tread quantiser below 3 bits and a dead zone above −18 dBFS output silence: the table divided by zero. "Louder playing is still louder" is false above ≈ 24 dB of drive |
| 10 | **Sample divide: interpolated read, area-sampled edge** | A naive hold at the inner rate jitters its edges by up to one inner sample (inharmonic grit that is not the effect) and has no half-sample delay; the area mean is ADAA1 of the staircase, at the cost of one multiply |
| 11 | **DC block kept on the wet, reason and cost stated; bench reference includes it** | Before the mix and gain it prevents thumps on ramps; the all-Off null against the bare all-pass would have failed at 40 Hz (10° of lead) |
| 12 | **`quality` / `slope`: pre-rolled engine, then crossfade** | A cold engine crossfaded in 10 – 20 ms lets a 112 Hz crossover settle under the fade |
| 13 | **Crossover windows** (2 – 94 / 4 – 96 / 6 – 98 %) taken from Prisma's `calccross` | Without them x1 = 100 % leaves no room for x2, x3; bench rows that put "all crossovers at 20 kHz" corrected |
| 14 | **Peak gain default +9 dB**; **per-band mute dropped**: 54 → 50 parameters | Open question 2; `<b>_gain` −60 dB is a mute, and parameters can be added later, never removed |
| 15 | **Cabinet voicings ordered by Prisma's roles** (1 = bass, 3 = the small one) | A saved chain keeps the kind of cabinet it had |
| 16 | **gfx-dsp sharing settled** (§2.8): one `crossover` in three layers with gfx-bassdi; gfx-drive's shapers into `adaa`; gfx-crusher: laws only | Two designs proposed a `crossover` module with different contents; gfx-crusher's engine (BLEP converter) is not what a band-local module needs |
| 17 | **Enum appends need the catalogue's option lists in step** (§2.4, §3.1) | golem writes enums as index / (n − 1) from its own list: 23 options against the plugin's 25 would select the wrong module |
| 18 | **Licence hygiene paragraph** (§2.4) | AGPL-3 source read for its laws only; nothing of Prisma (code, tables, make-up factors, the six bundled IRs) is reused. Checked: the design asks for none of it |
| 19 | Facts: Tight kick read from Prisma's source and re-authored (the draft's −52 dBFS dead zone does nothing to a kick); dispersion peaks and LR8 figures; SVF tick counts; LR8 "twice" → 1.85 × at DC, 2.2 × at the peak; Megaphone converted with its peak; Gain's clamp; dangling §7.7 reference | Accuracy |
| 20 | **Visual**: hue clash resolved by proposal (lofi raspberry stays, gfx-swell → orchid `#bf1dd7`) | Both sit at 340°; the gap holds one hue |

Still open:

| Point | Owner |
|---|---|
| Type-keyed conversion of `*_v1/_v2` in the engine table (else the §3.2 fallback); `*_gain` in dB; option lists at 25; the note's "Keep Quality OFF live" removed; count of saved chains using `grit` | golem-rack |
| Showing the plugin's formatted value (dB, bits, Hz) for `*_v*`; EXP on `drive` / `focus` (non-catalogue ids) | golem-rack |
| gfx-swell's hue (`#d81e5b` → `#bf1dd7`), to carry into gfx-swell's README if accepted; raspberry against drive's thrash red at tile size | The maintainer, on the family sheet |
| Which of gfx-bassdi / gfx-grit creates gfx-dsp `crossover` layer (1); moving gfx-drive's shapers without changing its sound | golem-fx, at coding order |
| Aliasing bounds with the real half-bands; fizz on a driven top band → ADAA2 or 8× | Bench |
| Reference level of the compensation; Drive weights; loudness of a square against a sine (+2 – 3 LU expected) | Bench, by ear |
| Cabinet voicings, Tight Kick, the Zero cross knee; audibility of 4.9 ms (LR4) and 9 – 12.6 ms (LR8) of low-frequency dispersion on bass and kick | Bench, by ear |
| Meaning of Prisma's IR names (roles inferred [I]); US 7206419 read only as an abstract [I] | Not blocking |
"""
open(p,'w').write(t[:i]+new)
