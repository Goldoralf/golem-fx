# gfx-eq

> **Status: CODED** (v0.1.0, 2026-11-12) — every band a matched section in the TPT SVF (within 1 dB of analog to 20 kHz at 48 kHz), proportional Q, de-harsh with relative detection, static auto gain, bands fade in / out (bit-transparent when flat), `harsh_gr` meter; 6 behaviour tests, bench green at 44.1 – 192 kHz.

golem's EQ. Stands for golem's `eq` block (today ReaEQ: high-pass, one bell, a high shelf), keeping its
four parameter ids and units ([golem contract](../docs/golem-contract.md)). The aim is the best live EQ:
analog-matched curves up to Nyquist with zero latency, click-free under an EXP pedal, musical widths,
and the three moves a live rack actually needs beyond the simple face: high cut, de-harsh, air.

## 1. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference or not re-checked.

### Commercial references

| Product | What it does best | What we take |
|---|---|---|
| [FabFilter Pro-Q 4](https://www.fabfilter.com/products/pro-q-4-equalizer-plug-in) [V] | Zero-latency, natural and linear phase; dynamic EQ on every band; spectral dynamics; EQ Match; tilt shapes; auto gain | Dynamic band, tilt, auto gain — not linear phase or spectral (latency) |
| [TDR Nova](https://www.tokyodawn.net/tdr-nova/) [V] | Parallel dynamic EQ, sidechain with gentle internal filters | A cut-only dynamic band |
| [API 550A / 560](https://apiaudio.com/product/550a-discrete-3-band-eq/) [V] | **Proportional Q**: wide for small moves, narrow for big ones | Proportional Q on the bell |
| [Sonnox Oxford EQ](https://www.sonnox.com/plugin/oxford-eq) [I] | EQ "types" that change how gain and Q interact | Gain–Q coupling as a parameter |
| [Maag EQ4](https://www.kmraudio.com/products/maag-audio-eq4) [V] | **Air band**: boost-only, very broad shelf cornered high (up to 40 kHz), heard as its tail | `air` |
| [Pultec EQP-1A](https://help.uaudio.com/hc/en-us/articles/7183176266260-Pultec-Passive-EQ-Collection-Manual) [V] | Boost and cut at the same low frequency, different curves | A preset (low shelf + broad cut above), no mode |
| Neve 1073, SSL E / G [I] | Broad musical curves; constant vs proportional Q | Broad defaults |
| [Line 6 Helix EQs](https://www.guitarchalk.com/line-6-helix-effects-list/) [V] | Simple EQ, Low / High Cut, Tilt, Parametric, Graphic, Cali Q (Mesa 5-band); users find its Simple EQ's high shelf too low | Simple face + cuts + tilt is what players use; a well-placed shelf |

### Open source

| Project | Idea |
|---|---|
| [x42 fil4](https://github.com/x42/fil4.lv2) [V] | **The closest to our target**: four bells as Regalia-Mitra lattices (stable under parameter changes), shelves, HP / LP with a resonance control, "correct equivalent analog gain at Nyquist", every control smoothed, zero latency |
| [LSP Para EQ](https://lsp-plug.in/?page=manuals&section=para_equalizer_x8_mono) [V] | The catalogue of filter design methods: bilinear (cramps), matched-Z (can alias), direct digital design |
| [Cytomic technical papers](https://cytomic.com/technical-papers/) [V] | Trapezoidal SVF: bell and shelves from one structure, well-behaved under fast modulation; Nyquist-matched one-pole for tilt |
| [Airwindows Baxandall](https://www.airwindows.com/baxandall/) [V] | Broad shelves as interleaved biquads; a Baxandall is not "just a Q" |
| Calf, ZamEQ2 [I] | Plain RBJ biquads: the counter-example (cramping near Nyquist, zipper under modulation) |

### Papers

- RBJ, *Audio EQ Cookbook* ([W3C](https://www.w3.org/TR/audio-eq-cookbook/)): bilinear biquads **cramp** —
  an 8 kHz bell at 48 kHz comes out narrow and lopsided.
- Orfanidis, *Digital Parametric Equalizer Design With Prescribed Nyquist-Frequency Gain*, JAES 1997
  ([AES](https://aes.org/publications/elibrary-page/?id=7418)) [V]: bells without most of the cramping.
- Vicanek, *Matched Second Order Digital Filters*, 2016 ([PDF](https://www.vicanek.de/articles/BiquadFits.pdf))
  [V]: low-pass, high-pass, band-pass and peaking "without the notorious cramping", closed form, "suitable
  for modulation".
- Simper (Cytomic), *SvfLinearTrapOptimised2*: the structure that stays clean when its coefficients move.

### What we conclude

1. **Matched curves** (Vicanek / Orfanidis) realised in a **trapezoidal SVF** (Simper): analog-like up to
   Nyquist without oversampling, and click-free under modulation.
2. **Proportional Q** on the bell; broad shelves; a **tilt**.
3. **High cut** (missing from the current four), **de-harsh** (one cut-only dynamic band, relative
   detection) and **air** are the live wins.
4. **Static auto gain** (computed from the curve, never from the signal): an EQ change does not change the
   loudness between scenes, and nothing pumps.
5. Skipped: linear / natural phase, spectral dynamics, EQ Match (latency or FFT); slopes beyond
   24 dB / oct; graphic modes (presets cover them); tone stacks (they belong to the amp); saturation (drive
   blocks exist).

## 2. Design

```
in ▶ low cut ▶ low shelf ▶ bell (proportional Q) ▶ bell 2 ▶ de-harsh (dynamic, cut only) ▶ high shelf ▶ air
   ▶ tilt ▶ high cut ▶ output gain / auto gain ▶ out
```

- One filter core for every band: 2-pole trapezoidal SVF with coefficients from matched designs; tilt on
  a matched one-pole. Shared in gfx-dsp.
- Smoothing: frequencies in log-Hz, gains in dB, ~20 ms; SVF coefficients updated every few samples and
  interpolated (never direct-form coefficients).
- **De-harsh**: a bell whose cut follows the band's envelope **relative to the full-band envelope**, so the
  threshold holds from a clean to a driven signal. Attack 1 ms, release 80 ms. Same detector as
  gfx-deesser.
- **Auto gain**: the curve's average dB over 50 Hz – 10 kHz, pink-weighted, computed on the control path.

## 3. Parameters

Catalogue ids kept; defaults = the catalogue's; new parameters default to neutral.

| id | Name | Type | Range | Catalogue | Default | Curve | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `lowcut` | Low Cut | f | 10 – 500 Hz | 20 – 200 | 80 Hz | log | | High-pass |
| `midgain` | Mid Gain | f | −18 – +18 dB | −12 – +12 | 0 dB | lin | ✓ | Bell gain |
| `midfreq` | Mid Freq | f | 100 – 8,000 Hz | 300 – 2,500 | 800 Hz | log | ✓ | Bell frequency (an EXP sweep = a fixed wah) |
| `highs` | Highs | f | −18 – +18 dB | −12 – +12 | 0 dB | lin | ✓ | High shelf gain |
| `lowcut_slope` | Low Cut Slope | e | 12 · 24 dB / oct | new | 12 | — | | Butterworth |
| `lows` | Lows | f | −18 – +18 dB | new | 0 dB | lin | | Low shelf gain |
| `lowfreq` | Low Freq | f | 40 – 500 Hz | new | 120 Hz | log | | Low shelf corner |
| `midq` | Mid Q | f | 0.3 – 6 | new | 0.67 (ReaEQ's 2 octaves) | log | | Bell width at small gains |
| `midprop` | Proportional Q | f | 0 – 100 % | new | 50 % | lin | | 0 = constant Q; 100 = API-like narrowing with gain |
| `mid2gain` | Mid 2 Gain | f | −18 – +18 dB | new | 0 dB | lin | ✓ | Second bell: a guitarist's commonest move is two at once (cut the mud, lift the presence) |
| `mid2freq` | Mid 2 Freq | f | 100 – 8,000 Hz | new | 2,500 Hz | log | ✓ | |
| `mid2q` | Mid 2 Q | f | 0.3 – 6 | new | 0.7 | log | | Same proportional-Q amount as the first bell |
| `highfreq` | High Freq | f | 1 – 16 kHz | new | 5 kHz | log | | High shelf corner (ReaEQ's band 4 value to read on the bench) |
| `air` | Air | f | 0 – 10 dB | new | 0 dB | lin | | Boost-only broad shelf |
| `airfreq` | Air Freq | f | 5 – 20 kHz | new | 12 kHz | log | | |
| `tilt` | Tilt | f | −6 – +6 dB | new | 0 dB | lin | ✓ | Around 1 kHz: + = brighter, − = darker |
| `highcut` | High Cut | f | 1 – 22 kHz | new | 22 kHz (off) | log | ✓ | Low-pass, 12 dB / oct, matched (no forced zero at Nyquist) |
| `highcut_slope` | High Cut Slope | e | 12 · 24 dB / oct | new | 12 | — | | 24 after a cab simulation |
| `harsh_freq` | De-harsh Freq | f | 1.5 – 8 kHz | new | 3.5 kHz | log | | |
| `harsh_thresh` | De-harsh Threshold | f | −40 – 0 dB | new | 0 dB (off) | lin | | Band level relative to the full band |
| `harsh_range` | De-harsh Range | f | 0 – 12 dB | new | 6 dB | lin | | Maximum cut |
| `output` | Output | f | −18 – +18 dB | new | 0 dB | lin | ✓ | |
| `auto_gain` | Auto Gain | b | off / on | new | off | — | | Static loudness compensation |

Read-only (meter): `harsh_gr`, de-harsh gain reduction in dB.

Presets to add to golem's five (Standard, Guitar polish, Mid scoop, Bass cleanup, Vocal clarity):
Guitar post-cab (low cut 90 Hz, high cut 7 kHz, de-harsh 3.5 kHz), Voice air (low cut 100 Hz, air +3 dB),
Pultec low (lows +4 dB at 60 Hz, broad −2 dB at 200 Hz), Mesa V.

## 4. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Guitar after amp and cab | `lowcut` 90 Hz, `highcut` 7 kHz at 24 dB / oct, `mid2gain` −3 dB at 350 Hz | The two cuts every mixed guitar gets; matched high cut (no cramping at 7 kHz) |
| High-gain fizz | `harsh_freq` 3.5 kHz, `harsh_thresh` −18 dB, `harsh_range` 6 dB | Dynamic de-harsh: cuts only when the fizz is there, palm mutes keep their bite |
| Solo boost on a footswitch / scene | `midgain` +4 dB at 1.2 kHz, `output` +2 dB | Click-free scene change |
| Fixed wah / EXP sweep | `midgain` +12 dB, `midq` 4, `midprop` 0, EXP on `midfreq` | SVF core: sweep without zipper |
| Mid scoop | `midgain` −5 dB at 550 Hz, `highs` +2 dB (golem's preset) | Proportional Q: the deeper the cut, the narrower |
| Bass, tight and present | `lowcut` 35 Hz at 24 dB / oct, `lows` +2 dB at 80 Hz, `midgain` −3 dB at 500 Hz, `mid2gain` +3 dB at 1.5 kHz, `highcut` 5 kHz | |
| Voice | `lowcut` 100 Hz, `midgain` −2.5 dB at 400 Hz, `air` +3 dB, de-harsh at 6 kHz | Air band: presence without harshness |
| Acoustic guitar (piezo) | `lowcut` 80 Hz, `midgain` −4 dB at 1 kHz ("quack"), `air` +2 dB | |
| Brighter / darker in one move | `tilt` ±2 dB with `auto_gain` on | Tilt + static auto gain: tone changes, loudness does not |
| Pultec low end | `lows` +4 dB at 60 Hz, `midgain` −2 dB at 200 Hz, `midq` 0.5 | |

## 5. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| Matched response | every band at 1, 5, 10, 15 kHz and 48 kHz: magnitude within 0.5 dB of the analog prototype up to 0.9 × Nyquist |
| Flat at neutral | all gains 0, cuts off: bit-identical output |
| Modulation | `midfreq` swept 100 Hz → 8 kHz in 50 ms under a sine: no click (HF burst bound), no instability |
| Proportional Q | bandwidth at ±3 dB vs ±12 dB follows `midprop` |
| De-harsh | a 3.5 kHz burst inside a full-band signal is cut by `harsh_range`; the same burst 20 dB quieter overall is cut the same (relative detection) |
| Auto gain | pink noise: output loudness ±0.5 dB whatever the curve |
| Presets | golem's five eq presets on clean, funk and crunch |
