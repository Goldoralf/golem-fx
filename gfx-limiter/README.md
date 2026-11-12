# gfx-limiter

> **Status: CODED** (v0.1.0, 2026-11-12) — zero-latency and lookahead (1 / 2 / 5 ms, true peak) modes, three styles, `gr` meter; 6 behaviour tests (never above the threshold in any mode, bit-transparent below it, latency = delay, no bass riding, true peak ≤ +0.1 dB, Clip-style aliasing ≈ −57 dB), bench green at 44.1 – 192 kHz. Clip style clips at 2× with 1.7 dB of headroom for the down-sampler's overshoot, so its peaks land up to 1.7 dB under the threshold.

golem's peak limiter. Stands for golem's `limiter` block (today x42-dpl; `golem_limiter.jsfx` on
Windows / macOS), keeping its parameter ids and units ([golem contract](../docs/golem-contract.md)). The
aim: a limiter that never lets a sample past its threshold, **zero latency by default** — with the
unavoidable first-cycle shaping done by a band-limited clipper — and an optional lookahead mode that is
fully transparent and true-peak capable.

## 1. What golem has today

- **The current block is not zero-latency**: x42-dpl uses a 1.2 ms lookahead rounded up to 8-sample
  blocks (64 samples at 48 kHz) and reports it [V, [peaklim.cc](https://github.com/x42/dpl.lv2/blob/master/src/peaklim.cc)].
  `golem_limiter.jsfx` copies it: 64 samples reported, linear gain ramp, final clamp.
- **`threshold` is the output ceiling, no makeup** (x42's README: no sample exceeds it) [V]. The Standard
  preset (−6 dB, 50 ms) means "peaks capped at −6 dBFS". This meaning is kept: an L1-style threshold with
  automatic makeup would make every preset 6 dB louder.
- x42's internals [V]: two targets (a fast block peak; a slow 500 Hz low-passed path held ~16 ms — the
  "longer hold on loud lows"), exponential smoothing, true peak by 4× oversampled detection.
- `golem_safety.jsfx` is already golem's zero-latency last-resort safety (instant attack, −1 dBFS clamp,
  NaN scrub). gfx-limiter's job is to **sound good** as a block.

## 2. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[K]** known, not re-checked; **[I]** inference.

| Reference | What it does best | What we take |
|---|---|---|
| [FabFilter Pro-L 2](https://www.fabfilter.com/help/pro-l/using/advancedsettings) [V] | 8 styles; the newer ones need more latency; true peak adds ~5 ms; lookahead below 0.1 ms ≈ hard clipping; separate transient and release stereo link | Styles; lookahead and true peak go together |
| [Waves L2](https://josephnilo.com/blog/waves-l2-explained/) [V] | ARC: release becomes a scale on an adaptive release (a fast fixed release pumps or distorts) | Auto release |
| [Ozone Maximizer](https://downloads.izotope.com/docs/ozone9/en/maximizer/index.html) [V] | IRC LL = lowest-latency mode | A low-latency style is standard practice |
| [TDR Limiter 6 GE](https://docs.tokyodawn.net/limiter-6-ge-manual/) [V] | Serial modules: compressor, peak limiter, HF limiter, **clipper**, output protection; a short lookahead distorts lows sooner | Clipper stage, LF care |
| [LSP Limiter](https://lsp-plug.in/?page=manuals&section=limiter_mono) [V] | Herm / Exp / Line shapes, ALR (a slow levelling stage before the peak cutter), oversampling, true-peak modes | Two-stage reduction |
| [Calf Limiter](https://calf-studio-gear.org/doc/Limiter.html) [V] | ASC: under sustained reduction, release returns to the average, not to 0 dB | Program-dependent release |
| [Airwindows ClipOnly2 / ClipSoftly](https://www.airwindows.com/cliponly2/) (MIT) [V] | A safety clipper that leaves unclipped samples untouched; a zero-latency soft clipper | Untouched below the knee |
| [Signalsmith, *Limiter*](https://signalsmith-audio.co.uk/writing/2022/limiter/) [V], [Rudrich](https://github.com/DanielRudrich/SimpleCompressor/blob/master/docs/lookAheadLimiter.md) [V] | Lookahead done right: moving minimum over N samples, then cascaded box filters of total length ≤ N, audio delayed by N — **no sample overs, guaranteed**; latency = attack | The lookahead engine |
| ADAA — Parker, Zavalishin & Le Bivic (DAFx 2016), Bilbao, Esqueda et al. (IEEE SPL 2017), [Vicanek](https://www.vicanek.de/articles/AADistortion.pdf) [V] | Antiderivative antialiasing for clippers; with 4× oversampling even hard clipping is nearly alias-free | The zero-latency clipper |
| ITU-R BS.1770 true peak ([Essentia](https://essentia.upf.edu/tutorial_audioproblems_truepeakdetector.html)) [V] | 4× polyphase FIR, 48 taps: needs ~6 future samples | True peak only with lookahead |

### What we conclude

1. **Zero latency cannot be transparent on the first cycle**: without lookahead the gain must drop at the
   over sample itself. The best possible is a **band-limited soft clipper** that handles the first few
   cycles, while an envelope takes over within ~1 ms.
2. **ADAA on the residual only** (`clip(x) − x`, exactly 0 below the knee): bit-transparent when nothing is
   limited (plain ADAA on the whole signal would delay by half a sample and dull the top), then a hard
   clamp — the promise.
3. **Lookahead** (Signalsmith) for full transparency and **true peak**, reported, in discrete steps.
4. **Two-stage auto release** and an **LF hold** (one period of the lowest content) against bass distortion.
5. Skipped: multiband / auditory-band modes (latency, CPU), dither, LUFS, auto makeup (would move preset
   levels), true peak at zero latency (cannot be done honestly), a continuous lookahead knob (latency
   churn), a DC blocker (breaks transparency).

## 3. Design

```
in ▶ NaN / inf scrub ▶ × gain ▶ detector (sample peak; 4× true peak with lookahead)
   ▶ targets: fast peak + LF path (500 Hz low-pass, auto hold of one LF period)
   ▶ gain envelope: zero latency (attack at the over sample, toward threshold + knee)
                    or lookahead (moving minimum + cascaded box filters, audio delayed)
   ▶ two-stage auto release ▶ apply gain ▶ ADAA soft clip on the residual ▶ hard clamp at threshold ▶ out
```

| `style` | What does the work | For |
|---|---|---|
| **Clean** (default) | The envelope; small knee; the clipper only catches the first cycle | Transparent end of chain |
| **Punchy** | A later envelope, more clipper | Keeps attacks |
| **Clip** | No envelope: pure ADAA soft clip | Loud live leads |

- Lookahead: reported with `set_latency_samples`, changed only at preset level (it retriggers the host's
  delay compensation).
- Stereo: one linked gain (`link` for partial linking).

## 4. Parameters

Catalogue ids kept with x42's meanings.

| id | Name | Type | Range | Catalogue | Default | Role |
|---|---|---|---|---|---|---|
| `threshold` | Threshold | f | −24 – 0 dB | −10 – 0 | −6 dB | Maximum output sample level, no makeup. Negative minimum: check plain ↔ normalized on REAPER |
| `release` | Release | f | 1 – 1,000 ms | same (x42 takes seconds; golem's `scale` goes away) | 50 ms | With `auto_release`: scales the adaptive release |
| `gain` | Input Gain | f | 0 – +24 dB | new | 0 dB | Drive into the limiter (x42's hidden input gain) |
| `style` | Style | e | Clean · Punchy · Clip | new | Clean | §3 |
| `lookahead` | Lookahead | e | Off · 1 ms · 2 ms · 5 ms | new | Off | Reported latency |
| `true_peak` | True Peak | b | off / on | new | off | Only with lookahead (ignored otherwise, documented) |
| `auto_release` | Auto Release | b | off / on | new | on | Two-stage release + LF hold |
| `clip` | Clip Softness | f | 0 – 100 % | new | 50 % | 0 = hard (ADAA); 100 = soft from 6 dB under the threshold |
| `link` | Stereo Link | f | 0 – 100 % | new | 100 % | Stereo layout only |

Read-only (meter): `gr`, gain reduction in dB.

No `ceiling`: under x42's meaning `threshold` already is the ceiling; a second one would be redundant or
would silently change preset levels.

**golem side**: the catalogue's `release` `scale: 0.001` (x42 takes seconds) must go when the block
switches to gfx-limiter (ms).

## 5. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| End-of-lane safety, inaudible | Clean, threshold −3 dB, release 50 ms, `auto_release` on | Zero latency; untouched below the threshold |
| Louder lead without clipping the PA | Punchy, `gain` +6 dB, threshold −3 dB | Envelope + soft clip share the work, attacks kept |
| Loud and aggressive | Clip, `gain` +9 dB, `clip` 30 % | Alias-free clipper |
| Bass, no distortion on the low E | Clean, release 100 ms, `auto_release` on | LF hold: the gain does not ride the waveform |
| Slap bass / percussive peaks | Punchy, threshold −6 dB | |
| Vocal peaks | Clean, threshold −4 dB, release 120 ms | |
| Backing tracks / keys, fully transparent | `lookahead` 2 ms, `true_peak` on | Reported latency; no inter-sample overs |
| Level a whole patch before the output | `gain` to taste, threshold −1 dB | `threshold` is the ceiling: what you set is the peak you get |

## 6. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| The promise | random signals with `gain` +30 dB: no sample above `threshold`, every style and lookahead |
| Transparency | below threshold: bit-identical output |
| Aliasing | 5 kHz bursts, Clip style: aliased components ≤ −90 dBFS |
| Bass | 41 Hz, release 1 ms: distortion with `auto_release` on ≪ off |
| True peak | lookahead + `true_peak`: dBTP ≤ threshold + 0.1 dB |
| Latency | 0 with lookahead Off; reported = measured otherwise |
| Listening | crunch.wav and high-gain.wav after an amp, bass (to record), against x42-dpl |
