# gfx-drive

> **Status: CODED** (v0.1.0, 2026-11-12) — Overdrive (TS: first-order 720 Hz gain-leg high-pass, unclipped signal summed back), Distortion (level-dependent low-pass standing for the slew limit), Fuzz (two stages, inter-stage high-pass), Legacy (the JS curve, alias-free); tanh → hard-clip family with asymmetry and sag; phase-matched clean path for `blend` and `bass_keep` (Linkwitz-Riley 8 split); 5 behaviour tests, bench green. Choices made in code: 4× oversampling (two half-band stages: 2× left aliases near −77 dBFS; 4× measures −88 to −92 dBFS at 50 dB of gain); the slew limit is a level-dependent low-pass (a hard slew limiter aliased at −34 dBFS); ~6 samples of minimum-phase group delay, not reported.

golem's drive. Stands for golem's `guitar-drive` block (today REAPER's JS `guitar/distortion`), keeping
its parameter ids and units ([golem contract](../docs/golem-contract.md)). golem already plays exact
pedals through NAM captures; gfx-drive is something else: **one parametric drive engine** that covers
the pedal families (overdrive, distortion, fuzz), morphs continuously under an EXP pedal, stays clean of
aliasing at 50 dB of gain, and keeps a bass's fundamental.

## 1. The block being replaced

REAPER's JS `guitar/distortion` (LGPL, read on this machine):

- `preamp = 2^(gain / 6)`, `soft = 2^hardness`, `maxv = 2^(ceiling / 6)`;
- per sample, above `maxv`: `y = sign · (maxv + d / (soft + d))` with `d = |x| − maxv`.

So `ceiling` is the knee (and the driven output level), `hardness` how flat the curve is above it.
Below the ceiling it is perfectly linear; no EQ, no oversampling: harsh and fizzy at high gain. Its curve
has a closed-form antiderivative, so it can be reproduced alias-free (`voice` Legacy below).

## 2. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference or not re-checked.

### References

| Reference | What makes it special | What we take |
|---|---|---|
| Ibanez TS808 / TS9 ([ElectroSmash](https://electrosmash.com/tube-screamer-analysis)) [V] | Diodes in the op-amp's feedback loop; a high-pass in the gain leg so only the mids clip: the mid hump, tight lows | Pre-clip high-pass (`tight`) + post low-pass |
| Klon Centaur ([Coda Effects](https://www.coda-effects.com/p/klon-centaur-circuit-analysis.html)) [V] | Clean blended back as the gain rises; germanium diodes; 18 V headroom | Phase-coherent clean blend |
| ProCo RAT ([ElectroSmash](https://electrosmash.com/proco-rat)) [V] | LM308 slew limiting rounds the highs at full swing; hard diodes to ground | A slew limiter before the clipper |
| Fuzz Face / Big Muff ([ElectroSmash](https://electrosmash.com/big-muff-pi-analysis)) [V/I] | Bias sets asymmetry and sputter; cleans up with the guitar's volume | Asymmetry, sag |
| [Fractal Axe-Fx Drive block](https://forum.fractalaudio.com/threads/yeks-guide-to-the-fractal-audio-drive-models-final-ed-july-2022.148573/page-4) [V] | Every model shares generic controls: Clip Type, Bias, Slew Rate | **One parametric engine can cover dozens of pedals** |
| [Line 6 Helix](https://helixhelp.netlify.com/models/) [V] | Individual pedal models (Scream 808, Minotaur…) | golem has those through NAM |

### Open source

| Project | Idea | Licence |
|---|---|---|
| [BYOD](https://github.com/Chowdhury-DSP/BYOD) [V] | Many drive modules (TS, Centaur, RAT, Muff…), ADAA waveshapers | **GPL-3**: portable |
| [ChowCentaur](https://github.com/jatinchowdhury18/KlonCentaur), [chowdsp_wdf](https://github.com/Chowdhury-DSP/chowdsp_wdf) [V] | Klon by wave digital filters and by RNN; diode pairs | BSD-3 |
| [Jatin Chowdhury's ADAA notebook](https://ccrma.stanford.edu/~jatin/Notebooks/adaa.html) [V] | ADAA for hard clip and tanh; ADAA1 alone is not enough, ADAA1 + 2× oversampling works well | — |
| [Airwindows](https://github.com/airwindows/airwindows) Density2, Drive2 [V] | Cheap, smooth, antialiased saturators | MIT |
| [HIIR](http://ldesoras.free.fr/prod.html) (de Soras) [I] | Polyphase IIR half-band filters: minimum-phase 2× / 4× oversampling | WTFPL |

### Papers

- Yeh, Abel & Smith, *Simplified, Physically-Informed Models of Distortion and Overdrive Pedals*, DAFx 2007
  ([DAFx](https://dafx.de/paper-archive/details/mIq7vlQKu6pUlF467vvhgw)) [V]: the TS and DS-1 core is a diode
  clipper with an embedded low-pass — pre-emphasis, then a static clip, is a faithful simplification.
- Yeh, Abel & Smith, K-method, IEEE TASLP 2010; Werner et al., WDFs with multiple nonlinearities, DAFx 2015.
- Parker, Zavalishin & Le Bivic, DAFx 2016; Bilbao, Esqueda, Parker & Välimäki, *Antiderivative
  Antialiasing for Memoryless Nonlinearities*, IEEE SPL 2017 [V].
- Holters, *Antiderivative Antialiasing for Stateful Systems*, DAFx 2019 [V]; La Pastina et al., arbitrary-order
  IIR ADAA, DAFx 2021 (stability limits) [V].
- Eichas & Zölzer, *Gray-Box Modeling of Guitar Amplifiers*, JAES 2018 [V]: filter → static nonlinearity →
  filter (Wiener–Hammerstein) captures most of a pedal.

### What we conclude

1. **Filter → shaper → filter** captures a pedal's character; the EQ around the clipper matters more than
   the clip curve.
2. **Aliasing** is the legacy block's main defect: ADAA1 inside 2× minimum-phase oversampling, plus
   band-limiting before the clipper. Zero reported latency.
3. **Touch**: soft knee, sag, asymmetry. **Clean blend** with a low crossover for bass.
4. **Level matching** across the gain sweep (the legacy ceiling already does it once driven).
5. Skipped: per-pedal circuit models and neural captures (NAM covers them), a built-in gate (gfx-gate with
   the key on the DI does it better), amp / cab stages, supply simulation beyond sag.

## 3. Design

```
in ─┬▶ tight (HPF) ▶ pre-LPF ▶ × gain (− sag) ▶ slew ▶ [2× min-phase OS: shaper (hardness, asym), ADAA1] ▶ DC ▶ tone ▶ ceiling ─┬▶ out
    └▶ blend path (below bass_keep only, when bass_keep > 0) ─────────────────────────────────────────────────────────────────────┘
```

- **Shaper family** along `hardness`: 1 ≈ tanh, ~5 ≈ silicon diode, 10 ≈ the legacy hard knee. Every curve
  has a closed-form antiderivative; ADAA of a crossfade is the crossfade of the ADAAs.
- **Voices** preset the filters and the slew, they are not separate circuits:

| `voice` | `tight` | pre-LPF | slew | Character |
|---|---|---|---|---|
| Legacy | off | off | off | The JS curve exactly, now alias-free |
| Overdrive | 720 Hz [I] | 6 kHz | off | TS-style: out = in + clip(gain · HP(in)) — the unclipped signal is always summed back, which is where a Tube Screamer's dynamics and tight lows come from |
| Distortion | 100 Hz | 5 kHz | on | RAT / DS-1: harder, slew-rounded highs |
| Fuzz | 40 Hz | 8 kHz | off | **Two cascaded clip stages** with an inter-stage high-pass (Big Muff / Fuzz Face): one shaper cannot make a wall of fuzz. Biased; sputters and gates with `asym` and `sag`; cleans up when the input level drops (guitar volume) |

- **Blend coherence**: the clean path of `blend` / `bass_keep` runs through the same oversampling filters and
  the identity ADAA (a 2-point average), so it stays phase-aligned with the driven path: no comb at any blend.
- Antialiasing: ADAA1 on the memoryless shaper (it sits after the stateful filters and slew), inside 2×
  polyphase IIR half-band oversampling. ADAA1 adds a fixed half-sample group delay, not reported as
  latency (a quarter sample at the base rate; matched on the clean path, see above). 4× only if the bench finds aliases
  above −90 dBFS.

## 4. Parameters

Catalogue ids kept (`channel` is dropped: not needed). Defaults = the catalogue's.

| id | Name | Type | Range | Catalogue | Default | EXP | Role |
|---|---|---|---|---|---|---|---|
| `gain` | Gain | f | 0 – 50 dB | same | 20 dB | ✓ | Into the shaper, exact dB |
| `hardness` | Hardness | f | 1 – 10 | same | 6 | ✓ | Knee shape: tanh → diode → hard |
| `ceiling` | Ceiling | f | −40 – 0 dB | same | −12 dB | | Clip threshold and driven output level |
| `voice` | Voice | e | Overdrive · Distortion · Fuzz · Legacy | new | Overdrive | | §3 |
| `tight` | Tight | f | 20 – 1,000 Hz | new | 20 Hz (off) | | Extra pre-clip high-pass on top of the voice's own (metal: 150 – 300 Hz) |
| `tone` | Tone | f | −100 – +100 % | new | 0 % | ✓ | Post tilt / low-pass, 1 – 10 kHz |
| `asym` | Asymmetry | f | 0 – 100 % | new | 0 % | | Bias into the shaper (even harmonics, sputter), DC-blocked after |
| `sag` | Sag | f | 0 – 100 % | new | 0 % | | Hard hits lower the gain and move the bias |
| `blend` | Blend | f | 0 – 100 % | new | 0 % | ✓ | Phase-coherent clean mix (Klon) |
| `bass_keep` | Bass Keep | f | 0 (off) – 500 Hz | new | 0 | | Below it: clean only (a bass keeps its fundamental) |

Voices own their filters and slew; `tight` and `tone` act on top (contract's parameter conventions).

## 5. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Tighten a high-gain amp (the metal boost) | Overdrive, gain 6 dB, hardness 3, ceiling −6 dB, `tight` 200 Hz, `tone` +20 % | Pre-clip high-pass + level: the amp does the distortion |
| Edge-of-breakup into a clean amp | Overdrive, gain 18 dB, hardness 2, `blend` 30 % | Clean sum: dynamics follow the pick |
| Classic rock crunch | Distortion, gain 28 dB, hardness 5, `tone` 0 | Slew-rounded highs, no fizz |
| Wall of fuzz | Fuzz, gain 40 dB, hardness 7, `tone` −30 % | Two cascaded stages |
| Sputtery, gated fuzz | Fuzz, gain 35 dB, `asym` 60 %, `sag` 50 % | Bias + sag |
| Bass overdrive | Overdrive or Distortion, gain 20 dB, `bass_keep` 150 Hz, `blend` 40 % | Lows stay clean and mono-solid |
| Solo lift | any voice, EXP on `gain` | Level-matched across the sweep, alias-free at 50 dB |
| Old presets unchanged | Legacy | The JS curve, alias-free |

## 6. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| Legacy | `voice` Legacy nulls against the JS output except for the aliasing difference |
| Aliasing | 2 – 5 kHz sines at gain 50 dB: aliased components ≤ −90 dBFS |
| Level | loudness within ±2 dB across the gain sweep once driven |
| Blend | `blend` 50 %: no comb filtering against the dry |
| Bass | `bass_keep` 120 Hz on a low E: the fundamental passes clean |
| Listening | crunch.wav, high-gain.wav into a NAM amp (rendered by golem), against the JS block and against NAM pedal captures |
