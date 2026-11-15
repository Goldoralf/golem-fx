# gfx-spectralcomp

> **Status: DESIGNED (2026-11-15), reviewed by Fable 5.2** (Review section at the end; its decisions
> override the body where they differ). Design only, no plugin code. Prototypes in `proto/` (numpy, plus
> a Rust CPU timing in `proto/rs/`); every number marked [P] below is theirs, re-run in
> `proto/results.txt`.

golem's own **spectral compressor**. It compresses each part of the spectrum on its own, downward above
a threshold curve and upward below it, so a voice, a pad or a guitar is evened out without the pumping
of a broadband compressor. It replaces golem's block **`spectral-comp`** (Spectral Comp, category
`dynamics`), which today hosts Robbert van der Helm's Spectral Compressor CLAP. It keeps the block's ten
catalogue ids: `thresh`, `down`, `up`, `center`, `slope`, `attack`, `release`, `window`, `mix`, `out`
([golem contract](../docs/golem-contract.md)). Three of them change meaning slightly, with the conversion
in §3.4.

What it does that the block it replaces does not:

1. **A zero-latency Live mode.** The legacy block always delays the lane by its window: 2,048 samples,
   42.7 ms. golem's catalogue therefore keeps it out of guitar lanes. **Live** computes the gains in an
   STFT, as before. It applies them through a minimum-phase cascade of third-octave bells, solved by
   gfx-geq's accurate graphic-EQ engine: reported latency 0, no pre-echo. **Studio** applies the gains
   per bin inside the STFT, with the latency of the window reported (21.3 ms at the default).
2. **Thresholds that mean dBFS.** The legacy detector reads one bin's magnitude, scaled by the window and
   the overlap. The same sine reads 6 dB higher at 4,096 than at 1,024, and the "dB / oct" slope is
   applied per neper. Here the detector reads the **power in a constant-Q band** around each bin. A sine
   reads its own level whatever the window (within 0.05 dB [P]), and slope 0 is flat for pink noise.
3. **Gains smoothed across frequency** (`res`, 1/6 octave by default). This removes most of the musical
   noise and the per-bin gain jitter of the per-bin design, which is 3 to 8 times larger [P].
4. **An upward stage that cannot lift hiss**, with gfx-mbcomp's noise guard and a lift range. **Auto
   gain** level-matches pink noise at −18 dBFS RMS, so an A / B against bypass is fair.
5. **A hop that keeps spurs low**: Studio uses a hop of N / 8. At the stft module's N / 4 the gain steps
   left side-bands at −43 dBc [P]; at N / 8 they are under −70 dBc on moving gains, −58 dBc worst on
   steady low tones (Review, change 10).
6. A **key** input (the lane's DI today), **meters** for the gain-change curve, and a `delta` listen.

Mono 1 / 1 first, 2 / 2 second (linked detection). Reported latency: 0 in Live, N − 1 samples in Studio
(plus one hop where §2.9's worst-block rule cuts a frame over the hop).

## 1. State of the art

Survey of 2026-11-15. **[V]** read on a primary page (manual, maker page, source code) · **[I]**
secondary source, forum or search summary · **[U]** could not be verified · **[P]** measured on this
design's prototype (`proto/`) · **[K]** standard literature, not re-checked.

### 1.1 The block we replace: Robbert van der Helm's Spectral Compressor [V: source]

Read in `nih-plug/plugins/spectral_compressor` (GPL-3), for its formulas only. The reference prototype
`proto/ref.py` re-derives them from scratch for the comparisons of §3.4.

| Part | What it does | Consequence |
|---|---|---|
| STFT | Hann analysis and synthesis, window 64 – 32,768 (default 2,048), overlap 4× – 32× (**default 16×**; golem never sets it). Gain compensation gc = 1 / ((ov / 4) · 1.5) / N, √gc applied before the FFT and √gc after it | Latency = the window, reported (VST3; wrong in CLAP, issue #200 [V]) |
| Detector | **One bin's magnitude** \|X_k\| · √gc, a linear one-pole per bin at the hop rate (attack when rising) | A sine on a bin reads 20 log A + 10 log(N / (6 · ov)) dB: **+10.3 / +13.3 / +16.3 dB** at 1,024 / 2,048 / 4,096 with 16× overlap. Hann scalloping takes up to −1.4 dB more between bins. Noise reads per bin, so its level depends on the overlap, not on N. **A threshold means something different at every window size** |
| Threshold curve | T_k = thresh + (slope − 3) · ln(f_k / center) + curve · ln²(…). In "Pink Noise" mode it adds −3 to the slope | The slope multiplies a **natural log**: "dB / oct" is per neper, so the real per-octave slope is 0.693 × the displayed value. The −3 meant to flatten pink noise per bin gives −2.08 dB / oct, not −3.01. Slope 0 therefore leaves a +0.93 dB / oct tilt against pink noise |
| Curve | Downward and upward from one threshold (offsets −50 … +50 dB, default 0), ratios 1 – 500, quadratic knee (default 6 dB; Giannoulis et al.). The upward ratio has a **hi-freq rolloff** (default 0.75): 1 / r_k = (1 / r)(1 − t) + t, t = 0.75 · ln f / ln 22,050 | At 420 Hz an `up` of 3 acts as 1.57. **The catalogue's `up` is far weaker than it reads** |
| Upward guard | Only "envelope above −100 dB" and bins above 20 Hz. No cap on the boost | Upward ratios "squash anything into pink noise" (its README): a feature, and the source of lifted hiss |
| Sidechain | Three modes: Pink Noise (internal); Sidechain Matching (thresholds + key level per bin); Sidechain Compression (envelopes from the key) | We take Compression as `source` Key (§2.7). Matching is deferred |
| Output | Output gain on the wet, then a linear dry / wet mix with the dry delayed. Auto make-up is a TODO in the source | — |
| Reset | Timings faded in over 150 ms after a reset or a window change, "to avoid huge spikes" (changelog 0.4.2) | First frame instant here (§2.4) |

Its users ([issues](https://github.com/robbert-vdh/nih-plug/issues?q=spectral+compressor) [V]): the
tracker is mostly crashes (two instances, mono tracks in Ardour and Studio One: #144, #145, #157, #166,
#196, #250, #259), the CLAP latency report (#200), and requests for presets because "rebuilding settings
each time is tedious" (#183, which also calls it "maybe even the best resonance suppressor" that "sounds
good at low window sizes"). #146 asks that its sound stay the same across releases. On KVR: "a nice one,
but the threshold is fixed" ([KVR](https://www.kvraudio.com/forum/viewtopic.php?p=9154917) [V]).

golem's own record ([`effects.json`](~/golem/library/catalog/effects.json), `spectral.md` [V]):
- validated on the bench (2026-09-25), **not yet judged by ear**;
- "never in the base path of a guitar lane" because of the 42.7 ms;
- seven factory presets (`de-harsh-vocal`, `gentle-glue`, `keys-even`, `pad-hold`, `pink-noise-wash`,
  `vocal-smooth`, `vocal-upfront`), converted in §3.4.

### 1.2 Commercial references

| Product | What it does | Latency | What we take, or not |
|---|---|---|---|
| oeksound **soothe2** ([manual](https://oeksound.com/manuals/soothe2/) [V]) | Dynamic resonance suppressor. Depth, sharpness ("deeper and narrower cuts"), selectivity, attack (frequency-dependent), release. Soft mode is less level-dependent. Resolution eco / high / ultra. Delta, external sidechain | Manual gives none. About 45 ms ([SOS](https://www.soundonsound.com/reviews/oeksound-soothe-2) [I]); a user measured 2,048 samples at 48 kHz, also when bypassed ([Gearspace](https://gearspace.com/threads/oeksound-soothe2.1378685/) [I]) | Delta; latency kept when bypassed (our `bypass`); a cut cap |
| oeksound **soothe3** ([manual](https://oeksound.com/manuals/soothe3) [V]) | As soothe2, minimum-phase filters by default, linear phase optional | **Regular 2,304, Low latency 0, Linear 3,456** samples at 44.1 / 48 kHz. Low latency forces minimum phase and loses "lookahead", so it reacts later (vibrato) | **Exactly our split**: a zero-latency minimum-phase mode that reacts later, and a lookahead mode |
| oeksound **soothe live** ([manual](https://oeksound.com/manuals/soothe-live) [V]) | For live consoles. Level-dependent, "can also even out dynamics much like a multiband compressor". High "detail" makes artefacts on vocals. Mono key. Delta kept off the physical surface | 64 samples algorithmic (the page's table says 32 at 48 kHz) | Live resolution kept coarse; `delta` expert only |
| Soundtheory **Gullfoss** ([manual](https://www.soundtheory.com/static/Gullfoss%20Operation%20Manual.pdf) [V]) | Perceptual auto-EQ: Recover (masked parts) and Tame (dominant parts). About 1,000 model and 300 EQ updates per second | About 21 ms ("a 20 milliseconds time-frame"); **Gullfoss Live about 2 ms**, which "has to try to predict the future" and affects quick changes | Same split again: analysis on a window, applied by a short filter that lags |
| FabFilter **Pro-Q 4 Spectral Dynamics** ([manual](https://www.fabfilter.com/help/pro-q/using/spectral-dynamics) [V]) | Inside a band, only the frequencies over the threshold are cut. Spectral Density = selectivity. **Spectral Tilt adds 3 dB / oct to the trigger, on by default**. Automatic threshold and times | Spectral bands are always linear phase: Low 3,072 samples (70 ms) at 44.1 kHz ([manual](https://www.fabfilter.com/help/pro-q/using/processingmode) [V]). SOS: "significant latency, even … in 'zero latency' mode" [V] | The pink-flat reference of our slope 0 (their tilt, our band power) |
| Baby Audio **Smooth Operator** ([page](https://babyaud.io/smooth-operator-plugin) [V]) | FFT spectral balancer, linear phase, sidechain, auto gain | Classic (lower) / High-Res; figures not published [I] | Auto gain |
| Wavesfactory **Trackspacer** ([page](https://wavesfactory.com/trackspacer) [V]) | "Internal 32 band EQ" driven by the inverted key spectrum. Attack / release in ms, low / high cut, stereo / mid / side | Not stated; users say "latencyfree" [I]. Users report LF "interrupts" in v2.5 ([KVR](https://www.kvraudio.com/forum/viewtopic.php?p=7082443) [I]) | `source` Key with downward = this, with finer bands in Studio (§2.7, against gfx-ducker Spectral) |
| MeldaProduction **MSpectralDynamics** ([page](https://www.meldaproduction.com/MSpectralDynamics) [V]) | Per-frequency compressor / expander / gate on a drawn shape; look-ahead, RMS length, sidechain | Not published | A threshold curve, not a drawn shape: the face stays two numbers (thresh, slope) |
| Sonible **smart:comp** ([KVR](https://kvraudio.com/product/smart-comp-by-sonible) [I]) | "Spectro-dynamic" compression | About 100 ms (v1), "tracking not possible" [I] | What a live rig cannot have |
| Waves Curves Equator Live [I] | Marketed "zero latency" resonance control, auto make-up | — | Confirms the stage demand |

### 1.3 Open source and literature

- **polarity-sc-dark** ([GitHub](https://github.com/polarity/plugin.nih.polarity-sc-dark) [V], GPL-3,
  Rust on a nih-plug snapshot): per-bin up / down with the same three modes as the reference, freeze and
  IR export. Sizes and latency not stated. A fork of the reference's ideas, not a different method.
- **ZL Equalizer** ([manual](https://zl-audio.github.io/plugins/zlequalizer/manual) [I], AGPL-3): a
  minimum-phase dynamic EQ with a true zero-latency mode; other phase modes about 11 / 21 / 171 ms.
- **noise-repellent / libspecbleach** ([GitHub](https://github.com/lucianodato/noise-repellent) [I],
  LGPL-3): STFT spectral gating, "not appropriate to be used while recording (35 ms for 44.1 kHz)".
- **LSP** multiband dynamics (crossover-based, linear phase since 1.2.10) [I]; no other open-source
  spectral compressor found (Faust, JUCE, x42, Airwindows, Calf) [U].
- **McCormack & Välimäki**, *FFT-based Dynamic Range Compression*, SMC 2017
  ([Zenodo](https://zenodo.org/record/1401877) [I]): per-bin compression with frequency-dependent
  control, the reference's family.
- **Time aliasing** (J. O. Smith, *Spectral Audio Signal Processing*,
  [CCRMA](https://ccrma.stanford.edu/~jos/sasp/) [K]): a per-bin gain is a circular convolution. Without
  zero-padding its impulse response wraps around, and changes between frames "disturb the aliasing
  cancellation"; smaller hops help. That is §2.5's pre-echo and hop spurs.
- **Musical noise**: Cappé, IEEE Trans. SAP 1994 ([PDF](https://www.di.ens.fr/olivier.cappe/Publications/Self-archive/ephma94.pdf)
  [I]), and Breithaupt, Gerkmann & Martin, *Cepstral smoothing of spectral filter gains*, IEEE SPL 14(12),
  2007 [I]. Per-bin gains that fluctuate independently make tonal "bubbles"; smoothing the gains over time
  and frequency removes them. Our `res` (§2.2) is the frequency smoothing.
- **Gains computed in a filterbank, applied as one time-domain filter**: Löllmann & Vary, *Uniform and
  warped low delay filter-banks for speech enhancement*, Speech Communication 49, 2007 [I] (the
  "filter-bank equalizer"). Kates & Arehart, EURASIP JASP 2005 [I]: hearing-aid compression with a warped
  filter, group delay independent of the gains. **This is our Live mode** (§2.6).
- **Low-delay STFT with asymmetric windows**: Mauler & Martin, EUSIPCO 2007 [I]. A long analysis window
  and a short synthesis window, about 10 ms in total. Rejected in §2.6: the applied filter cannot be
  longer than the synthesis window, so its resolution is no better than a short FFT's.
- **Compressor curves**: Giannoulis, Massberg & Reiss, JAES 60(6), 2012 [K]: quadratic knee and
  smoothed detectors, as gfx-comp.

### 1.4 Forum feedback: what people love and hate

- **Love**: smooth control without pumping, "the best resonance suppressor" (#183 above), bringing out
  low-level detail with upward compression (the OTT crowd), and on live consoles replacing manual notch
  EQ ([Mix](https://www.mixonline.com/blog/innovations-oeksound-soothe-live-plug-in) [I]).
- **Transients**: artefacts that "make transients sound almost washy"; fast times "mess with the
  transients… especially on vocals" ([Gearspace](https://gearspace.com/threads/soothe-2-vs-standard-multiband-compressor.1368827/post-15783231)
  [I]). FFT dynamics have "poor time resolution"
  ([KVR](https://kvraudio.com/forum/viewtopic.php?p=7524566) [V]). → §2.5 measures the pre-echo; Live has
  none.
- **Overuse and dullness**: a vocal "wrapped in a wet blanket"
  ([kits.ai](https://www.kits.ai/blog/using-soothe2-by-oeksound-ai-vocals) [I]), a mix that "loses air
  and life" ([Dubspot](https://blog.dubspot.com/plugins/soothe2) [I]); soothe's own manual says hard mode
  is easy to overuse [V]. → `range`, auto gain, `delta`, `mix`.
- **Latency rules out tracking and live** (soothe2, smart:comp [I]; Pro-Q 4 users ask for a zero-latency
  spectral mode, [FabFilter forum](https://prod.fabfilter.com/forum/topic/7983/pro-q-4-spectral-dynamics-zerolow-latency-mode)
  [V]). → Live.
- **Spectral gates pump and sound non-musical** ([KVR](https://www.kvraudio.com/forum/viewtopic.php?p=9154917)
  [V]). → No spectral expansion in v1; upward compression only.
- **Hard to set** (#183; "threshold is fixed"). → Thresholds in dBFS whatever the window, a face of
  six controls, and presets.
- No user report found of "underwater" artefacts, pre-echo or level jumps on toggle [U]; every product
  above ships delta or auto gain, which says the need is real.

### 1.5 What we conclude

1. **The market has settled on two modes**: a zero-latency minimum-phase mode that reacts later
   (soothe3 0 samples, Gullfoss Live 2 ms, soothe live 64 samples) and a windowed mode with lookahead.
   golem needs both: Live for any lane on stage, Studio for pads, sound design and a voice whose
   monitoring tolerates the window.
2. **Analyse in the FFT, apply in the time domain** for Live. Zero latency with fine low-frequency
   resolution is only possible with minimum-phase filters, whose group delay plays the part of the
   window. gfx-geq's accurate graphic EQ already realises any smooth curve with zero latency, so Live
   needs no new filter engine.
3. **The legacy detector is per bin and uncalibrated.** Calibrated band power fixes the threshold's
   meaning and the musical noise in one step.
4. **Upward compression needs a guard and a cap**, as in gfx-mbcomp; the legacy block has neither.
5. **The face must be few controls**, and the levels honest (auto gain, delta).

## 2. Design

```
key (aux, mono) ──┐  source = Key
in ─┬─────────────┴─▶ DETECTOR (shared): periodic Hann STFT, N = window, hop N/8 (both modes)
    │                  P_k = c |X_k|²  →  band power E_k over ±res/2 oct (≥ ±2 bins), prefix sums
    │                  L_k = 10 log10(E_k / 0.5)  →  dB follower (attack / release per hop)
    │                  g_k = static curve(L_k, T_k = thresh + slope · log2(f_k / center)), range, lift_range, guard
    │
    ├─ Studio ─▶ same STFT: Y_k = X_k · 10^(g_k/20) → IFFT → Hann → OLA (/ 3.0) ─▶ wet   (latency N − 1)
    │            dry = in delayed by N − 1
    │
    └─ Live ───▶ G_p = power-weighted mean of g_k over ±1/3 oct at 55 design points, then [¼ ½ ¼]
                 → targets T_p, ramped over the hop
                 → ACGE solver (gfx-dsp geq): one warm-started correction per 1.33 ms tick
                 → 27 matched bells in cascade (TPT, coefficients ramped) ─▶ wet   (latency 0)
                 dry = in
wet × auto make-up ─▶ mix with dry ─▶ (delta: wet − dry) ─▶ × out ─▶ out
```

### 2.1 What is shared, what differs

- **One detector and one gain law for both modes**, at `res` as set in both. A preset sounds like the same
  compressor in Live and Studio: same detector, same make-up; measured by the review (`r6`), pink noise
  0.00 dB apart and four DIs within 0.13 dB in total. Only the resolution with which the gains are
  applied, and the timing, differ:

  | | Live | Studio |
  |---|---|---|
  | Gains applied by | 27 minimum-phase third-octave bells (ACGE), zero latency | per bin, in the STFT |
  | Reported latency | **0** | **N − 1 samples**: 10.6 / 21.3 / 42.7 / 85.3 ms at `window` 512 / 1,024 / 2,048 / 4,096, 48 kHz |
  | Frequency resolution of the gains | two-thirds of an octave (width at half depth 0.72 oct, §2.6) | `res` (default 1/6 octave), down to the main lobe |
  | Timing | the gains act from the frame's **end**: a detection lag of about N / 2 | the gains act on the frame itself: up to N / 2 of **lookahead** |
  | Artefacts | none of the STFT's (no pre-echo, no hop spurs); minimum-phase dispersion of the bells | pre-echo under onsets (§2.5), smearing at long windows |
  | Hop | N / 8 (detection; Review change 9) | N / 8 (§2.5) |

- **`mode` is a set-up switch**: it changes the reported latency. It is an enum, never automated, set by
  the preset or at insert. The switch fades the output out over 10 ms, changes engine, reports the
  latency and fades in (gfx-mbcomp's `phase` rule).

### 2.2 Detector: calibrated band power

- **STFT** (gfx-dsp `stft`, reused): **periodic** Hann (the module's form; w[0] = 0), sizes from its
  half-octave grid. The brief says "symmetric Hann"; the module's periodic Hann is what makes the
  overlap-add constant, so it is kept. Frames are processed **in one piece** when the hop completes
  (N ≤ 4,096 at 48 kHz, 16,384 at 192 kHz: §2.9). Spreading the work over the hop, as gfx-stretch and
  gfx-glitch do, would add H to the latency for no gain at these sizes.
- **`window`** is given in samples at 48 kHz (the catalogue's 1,024 / 2,048 / 4,096, plus 512, §3.3).
  The size follows the rate family: × 1 at 44.1 / 48 kHz, × 2 at 88.2 / 96, × 4 at 176.4 / 192. The time
  and frequency resolution are then the same at every rate, within 9 % at 44.1 kHz.
- **Power per bin**: P_k = c · |X_k|², c = 2 / (N Σ w²). A sine of amplitude A then gives Σ P = A² / 2 over
  its main lobe, and white noise of variance σ² gives P = 2σ² / N per bin (Parseval).
- **Band power**: E_k = Σ P_j for j in [k − d_k, k + u_k], with u_k = max(2, k (2^(r/2) − 1)) and
  d_k = max(2, k (1 − 2^(−r/2))) bins, r = `res` / 12 octaves. It is computed by **prefix sums** in f64
  with fractional edges: O(N), no per-bin loop over the band.
  - A **tone** reads its own power wherever it sits: the flat window always holds the Hann main lobe
    (±2 bins). Measured [P]: −20.00 ± 0.05 dB for a −20 dBFS sine at 100 Hz – 5 kHz, N = 1,024 – 4,096
    (−20.47 at 100 Hz, N = 512, where the lobe is 375 Hz wide).
  - **Noise** reads the power of the band. Pink noise reads the same in every constant-Q band, so
    **slope 0 is flat for pink noise**, as Pro-Q 4's tilt intends and the legacy block's −3 tried to.
    Below the frequency where ±2 bins are wider than the band (1.6 kHz at N = 1,024, 800 Hz at 2,048 with
    1/6 octave), the band holds more noise. Measured [P]: pink noise reads −0.92 dB / oct (N = 1,024) and
    −0.33 dB / oct (2,048) over 100 Hz – 10 kHz, about −35 dB (predicted −35.8 for 1/6 octave at −18 dBFS
    RMS). Tones are not affected. Stated, not corrected: the resolution floor is the analysis's.
  - **0 dB = a full-scale sine** in its band: L_k = 10 log10(E_k / 0.5).
  - **Lowest clean fundamental: 1.5 bins** (Review change 11): 141 Hz at `window` 512, 70 Hz at 1,024,
    35 Hz at 2,048. Under it the reading wobbles from frame to frame (a −20 dBFS sine at 0.88 bin: −1.3 /
    +2.4 dB; at 1.17 bins −0.3 / +1.0 dB; from 1.57 bins steady within 0.02 dB). Bass presets use 2,048.
- **`res`** (0 – 12 st, default 2 = 1/6 octave) is the width of that band. It smooths the gains across
  frequency and lowers their variance (B bins averaged, variance / B).
  - Measured [P], pink noise at −18 dBFS RMS, heavy setting (down 4, up 4, attack 10, release 50,
    N = 2,048):

    | Detector | Gain std over time | Bin to bin | Hop to hop |
    |---|---|---|---|
    | One bin (the legacy detector) | 1.96 dB | 2.02 dB | 0.66 dB |
    | Main lobe only (±2 bins, `res` 0) | 1.19 | 0.58 | 0.36 |
    | 1/12 octave (`res` 1) | 0.87 | 0.36 | 0.27 |
    | **1/6 octave (default)** | **0.67** | **0.25** | **0.22** |
    | 1/3 octave | 0.49 | 0.17 | 0.17 |

    The default cuts the bin-to-bin roughness by 8× and the fluctuation over time by 3×. That
    roughness is the musical noise.
  - All bins of one partial read nearly the same level (the flat window holds its main lobe), so the
    partial gets one gain and is not distorted. Between partials, `res` decides: 1/6 octave resolves the
    first 8 harmonics of a note; above them the band holds several harmonics, as a third-octave
    multiband would.

### 2.3 Static curve

Per bin, in dB, with T_k = `thresh` + `slope` · log2(f_k / `center`) and o = L_k − T_k:

- **Downward**: g_d = −(1 − 1 / `down`) · knee(o), held ≥ −`range`.
- **Upward**: g_u = (1 − 1 / `up`) · knee(−o), then **held ≤ max(0, L_k − `guard`)** (a boost never
  exceeds the level's height above the guard: nothing at or under it is lifted, gfx-mbcomp's law), then
  ≤ `lift_range`. No upward gain under 20 Hz.
- knee(x) = 0 for x ≤ −W / 2, (x + W / 2)² / (2W) inside, x above; W = `knee` (Giannoulis et al.).
- g_k = g_d + g_u. One threshold for both directions, as the catalogue block: above it the band is
  pulled down toward it, below it up toward it. A separate upward threshold is deferred (§2.12).
- `slope` turns the curve about `center` in **dB per octave** (log2, not the legacy neper). Negative:
  the highs are held down sooner (de-harsh). Positive: the lows are.
- Measured [P], a 1 kHz sine swept −80 → 0 dBFS, down 4, up 3, thresh −36, knee 0 and 6: **Studio
  within 0.10 dB of the formula** at every level. Live: §2.6.

### 2.4 Ballistics

- One follower per bin **in dB**: L̄ ← L̄ + α (L − L̄), α = 1 − exp(−H / (τ fs)), with τ = `attack` when
  L > L̄ and `release` otherwise. The times are real milliseconds at every hop and rate. The dB domain
  makes the gain move at a steady pace whatever its depth (Giannoulis's smooth decoupled form).
- **Reset**: the follower is instant (α = 1) for the first N samples after a reset, then normal (Review
  change 15: the first frame alone holds only H samples and reads 20 – 30 dB low). No slow start-up lets
  a loud first note through, and there is none of the legacy block's 150 ms timing fade.
- **What the window adds**: a frame sees an onset as soon as it enters, and the follower reads it at the
  hop rate. Measured on the gain at the tone's bin, timed at the frame's end (when Live can apply it),
  for a +30 dB step through down 4 (−19.5 dB) [P]:

  | `window` (N / 2) | attack 1 ms: 10 / 63 / 90 % | 10 ms | 50 ms | 150 ms | release 300 ms: 63 / 90 % |
  |---|---|---|---|---|---|
  | 512 (5.3 ms) | 3 / 5 / 8 ms | 5 / 16 / 27 | 16 / 61 / 125 | 40 / 173 / 371 | 244 / 460 |
  | 1,024 (10.7 ms) | 8 / 8 / 13 | 8 / 19 / 29 | 19 / 61 / 131 | 45 / 179 / 376 | 252 / 471 |
  | 2,048 (21.3 ms) | 13 / 24 / 24 | 13 / 24 / 45 | 24 / 67 / 131 | 45 / 184 / 376 | 263 / 487 |
  | 4,096 (42.7 ms) | 24 / 45 / 45 | 24 / 45 / 67 | 45 / 88 / 152 | 67 / 195 / 387 | 295 / 508 |

  So the effective attack is about τ plus N / 4 to N / 2: **the window is the fastest attack.** In Live
  the output is later than this table (targets ramped over a hop, solve converging over ticks). Measured
  at the output by the review (`r8`), +30 dB step, attack 1 ms, 10 / 63 / 90 %: `window` 512: 3.2 /
  6.7 / 8.8 ms; 1,024: 6.2 / 10.8 / 15.3 ms at hop N / 8 (8.7 / 13.7 / 18.2 at N / 4); 2,048: 13.4 /
  22.8 / 33.8 ms. Until then the onset passes uncut. In Studio the gain
  applies to the frame that contains the onset: it is up to N / 2 **earlier** than the table, and an
  onset is caught as it starts (lookahead), with the pre-echo of §2.5 as its cost.
- A spectral compressor is a levelling tool (catalogue default attack 150 ms). It is not a transient
  catcher: put gfx-comp or gfx-limiter in front for that (§4).

### 2.5 Studio: per-bin gains in the STFT

- Y_k = X_k · 10^(g_k / 20), inverse FFT, Hann synthesis window, overlap-add, / (1.5 · 8 / 4) = / 3.0.
  The dry for `mix` is the input delayed by the same N − 1 samples. The DC and Nyquist bins take their
  neighbour's gain.
- **Latency N − 1**: a frame is processed **inside the host block, at the sample that completes the
  hop**, and its first sample is complete then (a frame deferred to the block's end would make the
  latency depend on the block size). The review streamed blocks of 1 – 699 samples: null at −314 dB,
  no sample read early; N − 2 also works (w[0] = 0), N − 1 is kept as one sample of margin. Measured [P]: an impulse comes out exactly N − 1 samples late
  at N = 512 – 4,096. At unity gains the output nulls the delayed input at −312 dB (f64).
- **Hop N / 8, not the module's N / 4** [P]. A sine with a 6 Hz ±6 dB tremolo, compressed with attack
  10 ms, release 50 ms: at N / 4 the frame-to-frame gain steps leave side-bands at k · fs / H of
  **−44 dBc (N = 1,024) and −42 dBc (2,048)**; at N / 8 they are **−78 / −79 dBc** (worst component
  more than 40 Hz from the carrier: −70 / −79 dBc). On **steady** low tones under an 18 dB cut images
  remain at f ± k · fs / H: −58 dBc at 110 Hz and −62 dBc at 82 Hz (N = 1,024), median −77 (review,
  `r2`); at N / 4 the same tones give −35 to −58 dBc, so interpolating gains cannot rescue N / 4. Hop
  N / 16 (−77 dBc worst, twice the frames) is the named upgrade if the low images are heard. The legacy block defaults to 16× for this reason
  (changelog 0.4.0). Hann² at hop N / 8 sums to 3.0. The stft module gains an `OLA` constant per hop
  divisor (§2.11). Cost: twice the frames of N / 4 (§2.9).
- **Pre-echo** [P]. An E4 pluck at −6 dBFS after 0.6 s of −80 dBFS noise, compressed hard (down 8, attack
  5 ms), N = 2,048. Output RMS before the onset, re the first 10 ms of the note (input: −67 dB):

  | Setting | 0 – 5 ms before | 5 – 20 ms | 20 – 45 ms |
  |---|---|---|---|
  | down 8, one bin (legacy-like) | −19 dB | −37 | −65 |
  | down 8, 1/6 octave | −21 | −48 | −58 |
  | down 8 + up 8, one bin | −9 | −34 | −64 |
  | down 8 + up 8, 1/6 octave | −15 | −41 | −59 |
  | down 8 + up 8, 1/6 octave, N = 1,024 | −24 | −56 | −64 |
  | **Live**, down 8 + up 8 | **−72** (no pre-echo) | −72 | −72 |

  The gain of a frame that holds an onset also falls on the samples before it. Smoothing across
  frequency shortens the gain's impulse response, which takes 7 – 11 dB off the 5 – 20 ms part (the part
  that escapes backward masking). The last 5 ms stay at −15 to −24 dB under the note, inside the
  ear's pre-masking. A smaller window halves the span. This is why fast attacks on plucked sources are
  a Live job (§4).
- Time aliasing of the gain itself: with `res` ≥ 1/12 octave the gain curve's impulse response is
  short against the window, and no wrap-around was measurable above the pre-echo above. With `res` 0
  (one bin), sharp gain curves can wrap. `res` 0 means the main lobe (±2 bins), never one bin.

### 2.6 Live: the same gains, applied with zero latency

- **Why not a short FFT.** At N = 256 (5.3 ms) the bins are 187.5 Hz apart: everything under 400 Hz is
  one band, and the guitar's whole fundamental range one gain. **Why not a low-delay asymmetric STFT**
  (Mauler & Martin): the applied filter is cut to the short synthesis window, so its resolution is no
  better than a short FFT's. **Why not a minimum-phase FIR rebuilt each hop** (cepstrum, then a
  zero-latency partitioned convolution): four more FFTs per hop, a kernel crossfade on every hop, and
  gfx-dsp `convolve`'s kernel switch takes 30 ms. Possible, but heavier and less exact than reusing a
  solved EQ.
- **The gain curve as a graphic EQ.** gfx-geq's Accurate engine (gfx-dsp `geq`: ACGE, Välimäki & Liski
  2017, matched bells, an interaction matrix and a warm-started solve; coded and measured there) is
  used as is, with a new band list:
  - **27 bells on the ISO third-octave centres 40 Hz – 16 kHz** (any centre above 0.45 fs dropped:
    none from 44.1 kHz up), edges at the neighbours, no fill-ins (the gaps
    are a third of an octave), **c = 0.5 for this plan** (Review change 8; gfx-geq keeps 0.4). Design
    points: the 27 centres (weight 1), the 26 midpoints (0.5), two outer points (0.25). Filter gains
    clamped to ±40 dB (gfx-geq: ±30); with the target rule no section exceeds 23 dB.
  - **Targets** (Review change 1, the rule that makes Live work): at each of the 55 design points p,
    G_p = 10 log10(Σ P_k · 10^(g_k / 10) / Σ P_k) over the bins with |log2(f_k / f_p)| ≤ 1/3 (at least
    the nearest bin), P_k this frame's bin power of the detected signal plus a 1e-20 floor; then
    T_p = ¼ G_(p−1) + ½ G_p + ¼ G_(p+1), ends repeated. Two more prefix sums per hop. They are ramped
    linearly from one frame to the next over the hop, and the solver makes **one warm-started correction
    per control tick** (1.33 ms, gfx-geq's tick in time). Coefficients ramp across the tick
    (`set_biquad_ramped`). The cascade is bypassed bit-exactly while every gain and target is 0 dB
    (gfx-geq's rule), so a Live instance under its threshold is transparent.
- **Resolution in Live: two-thirds of an octave.** The detector runs at `res` as set; the target rule
  above widens what Live applies to a curve 2nd-order third-octave bells can draw. The draft held `res`
  ≥ 6 st and sampled g_k at the points: a lone tone then asked for a half-octave rectangle, the solve
  answered with opposing neighbours (−24.5 / +24.2 dB sections for a −24 dB request) and missed by −5.0
  to +4.2 dB, and Live cut pink noise 2.1 dB more than Studio (Review, changes 1 – 2).
- **Accuracy** (the draft's figures [P] below are superseded by the review's with the target rule: lone
  tones within **±1.0 dB down to −18 dB and ±2.1 dB at −24 dB** at 150 Hz – 7.1 kHz, upward region within
  +0.5 dB; program **0.44 – 0.46 dB** worst at the centres with c 0.4, 0.35 – 0.37 dB with c = 0.5):
  - **On program** (the clean and lead-clean DIs and a synthetic phrase; thresh −40, down 3, up 2, attack
    30 ms): the realised response follows the solver's targets within **1.15 – 1.38 dB worst, 1.0 – 1.2
    dB p95** at the centres, one correction per tick.
  - **On a lone sine** (the worst case: the narrowest curve), 1 kHz, down 4, up 3: within ±1.4 dB down to
    −15 dB of reduction, then up to 3.9 dB too deep at −27 dB; at 150 Hz within 0.6 dB everywhere; at
    5 kHz up to 5.4 dB too shallow at −22 dB and 7.4 dB at −27 dB; at 7.1 kHz (between centres) within
    1.5 dB to −15 dB, 5.4 dB short at −27 dB. In the upward region (−68 … −52 dBFS) up to +2.8 dB.
  - So, with the target rule, **Live is a two-thirds-octave spectral compressor, accurate to about ±1 dB
    to −18 dB** and ±2 dB at −24 dB. That is the cost of zero latency. Studio is the accurate one
    (±0.1 dB).
- **Detection lag**: as measured at the output in §2.4 (10.8 ms to 63 % at the default `window` 1,024). Like
  soothe3's low-latency mode, Live reacts later; it never anticipates.
- **Hop spurs**: the prototype holds the coefficients for each tick (Rust ramps them): side-bands at the
  tick rate −57 dBc (N = 1,024) and −61 dBc (2,048), worst component −57 dBc [P]. The bench bound (§5)
  is −70 dBc with ramped coefficients.
- **Phase**: the bells are minimum phase, so a cut turns the phase near it (42° at −12 dB, 72° at
  −24 dB [P]). With `mix` at 50 % the blend dips below the magnitude blend by 0.15 / 0.56 / 1.64 dB at
  −6 / −12 / −24 dB cuts, and 0.33 dB at a +9 dB boost [P]. Stated: parallel compression in Live is
  slightly deeper than the magnitudes say.
- **Detection hop N / 8** in Live too (Review change 9): the gains are ramped between frames, so the hop
  does not step the audio, but N / 8 takes 2 – 3 ms off the lag. Cost about +61 ns per sample.

### 2.7 Key (sidechain)

- **`source`**: Main · Key. In Key mode the detector runs on the key bus (mono aux input; contract:
  "sidechain inputs are a separate auxiliary bus"). The gains apply to the main. This is the legacy
  block's "Sidechain Compression":
  - with **downward**, the main is cut where the key is loud: spectral ducking ("make room", a
    Trackspacer-like use) at third-octave (Live) or bin (Studio) resolution;
  - with **upward**, the main is lifted where the key is quiet (fills the key's gaps).
- Measured [P]: a steady pink-noise main at −24 dBFS, key = an E4 pluck train, down 8, thresh −50.
  During a note, Studio cuts the main by **−12.9 / −11.6 / −6.7 dB at 330 / 660 / 990 Hz and 0.0 dB at
  200 Hz**. Live cuts −13.7 / −19.0 / −18.0 dB at the harmonics, and also −6.5 dB at 200 Hz and −4.6 dB at
  8 kHz: a wide duck (draft figures; with the target rule it is about two-thirds of an octave wide).
- golem routes the key from the **lane's DI** today (gfx-ducker §6). Keyed from the DI, the compressor
  reads the clean guitar while it compresses the drive's output, but only in the bands the two share
  (fundamentals and low harmonics): detection is band by band, and the drive's fizz above 3 kHz has no
  counterpart in the DI, so a DI key never reads it over the threshold (Review change 5). Another lane's key needs gfx-ducker's `key_from` (§6).
- **No key = no change**: a silent key is under every threshold, so the downward stage does nothing,
  the guard stops the upward stage, and **auto gain is 0 dB while `source` is Key** (Review change 3:
  otherwise a silent key left +16.9 dB of make-up on `make-room-for-voice`). Unlike gfx-gate, which
  falls back to Main, a missing key here does nothing. No connected-or-not logic (gfx-ducker's rule).
- **Against gfx-ducker's Spectral shape**: that is 16 half-octave cut-only bells with a fixed depth.
  This is a ratio-based compressor with both directions and per-bin resolution in Studio. The ducker
  stays the tool for "duck by a fixed amount"; this one is the tool for "keep the main out of the
  key's spectrum by a ratio".
- **Sidechain Matching** (thresholds follow the key's spectrum, a morph): deferred (§2.12).

### 2.8 Output, mix, level, meters

- **Auto gain** (`auto_gain`, default on): a static make-up, computed from the parameters only. Pink
  noise at −18 dBFS RMS (20 Hz – 20 kHz) is read through the static curve as the detector would read it
  at `res` as set (the same value in both modes),
  and the make-up returns its power to bypass. It is deterministic, does not pump, and is recomputed
  when a curve parameter changes (smoothed 50 ms). It applies to the wet only. Measured [P]: at the
  defaults the clean DI (−20.5 dBFS RMS) comes out within **+0.6 dB (Live) / −0.6 dB (Studio)** of bypass.
  Quieter program comes out louder (lead-clean DI at −33 dBFS RMS: +6.4 / +3.6 dB), which is what a
  compressor does. The reference level is a bench constant to set from the DIs (open question 5).
- **`mix`**: linear dry / wet, with the dry aligned (delayed by N − 1 in Studio). The catalogue's law.
- **`out`**: −24 … +24 dB **after** the mix (gfx-mbcomp's `level`). The legacy block put its output
  gain on the wet only; the two differ only when `mix` < 100 % (2 of the 7 legacy presets, §3.4).
- **`delta`**: hear wet − dry (the change), expert only, as soothe live keeps it off the surface.
- **`bypass`** (nih-plug `make_bypass`): the output becomes the dry, **still delayed by N − 1 in
  Studio**, so a bypassed Studio instance keeps its latency (soothe2 does the same) and REAPER's PDC does
  not jump. 10 ms crossfade.
- **Meters** (read-only parameters, contract):
  - `gr`: the largest, over the eight octaves, of the power-weighted mean of the cut part −min(g_k, 0)
    (dB ≥ 0), so a cut and a lift in one octave do not cancel (Review change 16);
  - `lift_gr`: the same for the boost part max(g_k, 0);
  - **`dyn_63` … `dyn_8k`**: the signed gain change (dB) at the eight octave centres 63 Hz – 8 kHz, each
    the power-weighted mean of g_k over its octave. That is the curve summary golem draws as eight
    bars. Updated once per hop, held 50 ms peak for display, as gfx-mbcomp's `<b>_dyn`.

### 2.9 Latency and CPU

- **Latency**: Live **0**. Studio **N − 1** samples, reported with `set_latency_samples` and changed
  only by `mode` and `window` (both set-up enums, "set before playing"): 10.6 / 21.3 / 42.7 / 85.3 ms at
  48 kHz for 512 / 1,024 / 2,048 / 4,096, and the same in ms in each rate family (44.1 kHz: 11.6 / 23.2 /
  46.4 / 92.9 ms).
- **Stage budget**: golem's own pedals keep a lane within a few ms (gfx-correct 4.3 – 6.6 ms is the
  largest live latency in the suite). Studio at 21 ms is for a lane whose monitoring is compensated
  (pads, a backing voice, FOH-only) or for sound design. Its use on a sung or played lane on stage is
  the player's choice; the default is Live.
- **CPU** [P: `proto/rs`, release build, this machine under load (load average 5), std `log10` / `powf`
  per bin, a f64 prefix sum, realfft 3.5]:

  | Part | 512 | 1,024 | 2,048 | 4,096 | per sample at 48 kHz |
  |---|---|---|---|---|---|
  | Live detector, hop N / 4 (FFT, band power, follower, curve) | 8 – 10 µs / hop | 15 – 19 µs | 31 – 33 µs | 62 – 68 µs | **60 – 77 ns** |
  | Studio, hop N / 8 (the above + gains, inverse FFT, window, OLA) | 9 µs | 17 – 21 µs | 34 – 37 µs | 70 – 91 µs | **131 – 177 ns** |

  - The per-bin math dominates (about 35 ns per bin with std `log10` and `powf`). gfx-dsp's fast
    exp / log would halve it; not counted.
  - **Live's bells** [A, re-scaled by the review from gfx-geq's measurements, gains always moving]:
    cascade 138 × 27 / 20 = 186 ns; solve 7.8 µs × (27 × 55) / (20 × 39) = 14.9 µs per tick = 232 ns;
    detector at N / 8 about 122 ns; the target rule about 15 ns.
  - **Totals, 48 kHz**: **Live ≈ 0.55 µs mono**, ≈ 0.9 µs stereo linked (one solve), 1.1 µs unlinked;
    **Studio ≈ 0.15 µs mono**. Inside the suite's 1 – 1.5 µs norm. Budgets for the bench: Live ≤ 0.7 µs
    mono, ≤ 1.2 µs stereo; Studio ≤ 0.3 / 0.55 µs. A solve every second tick is the lever if needed.
  - **Worst block** (Review change 7, both modes): linked channels complete their frames on the same
    sample. A frame runs in one piece while **channels × N ≤ 16,384 at 44.1 / 48 kHz (always), ≤ 8,192
    at 88.2 / 96 kHz, ≤ 4,096 at 176.4 / 192 kHz**. Above, the frame is cut into the stft module's units
    over the hop (`due` schedule): +H of reported latency in Studio, +H of detection lag in Live. (At
    192 kHz, default `window`, stereo: 2 × 72 µs against a 333 µs block, 43 %.) Bench bound: worst
    64-sample block ≤ 25 %.

### 2.10 Sample rates

- `window` scales with the rate family (§2.2); the third-octave bells are designed per rate (matched
  bells, gfx-geq) and all 27 exist from 44.1 kHz up.
- Measured [P], 1 kHz and 6 kHz sines at −12 dBFS, down 4 (formula −18.0 dB), at 44.1 / 48 / 96 /
  192 kHz: **Studio −17.99 / −18.00 dB at every rate**; Live −20.5 to −20.7 dB at 1 kHz (the lone-tone
  over-cut of §2.6) and −18.0 to −18.1 dB at 6 kHz, identical at 48 / 96 / 192 kHz and within 0.2 dB at
  44.1 kHz.
- Times are in ms and the hop is a fixed fraction of a window fixed in ms, so ballistics are the same at
  every rate.

### 2.11 Reuse from gfx-dsp, and what is new

- **Reused**: `stft` (sizes, periodic Hann, the FFT units and `due` schedule for 192 kHz, `SizeFade`
  for a `window` change); `geq` (`Plan`, matched `FixedBell`, the warm-started `correct`, the flat
  bypass) for Live; `svf` (`set_biquad_ramped`); `smooth` and `exp_smooth` (thresh, slope, center,
  mix, out); `stereo` link conventions from gfx-mbcomp; gfx-gate's aux-bus wrapper and
  `Effect::process_with_key` (gfx-ducker) for the key; `delay::DelayLine` for the aligned dry.
- **Changed in gfx-dsp** (each change keeps existing users bit-exact):
  - `geq`: `MAX_BANDS` 11 → 27 and `MAX_SECTIONS` 20 → 27, a band list with no fill-ins, `GAIN_CLAMP_DB`
    per plan (40 here, 30 for gfx-geq), and targets given at the design points directly (gfx-geq builds
    them from its PCHIP). gfx-geq's renders must null before / after.
  - `stft`: the OLA constant for hop N / 8 (3.0) beside N / 4's 1.5.
- **New, in the crate** (move to gfx-dsp when a second user appears): the calibrated band-power
  detector (prefix sums), the per-bin dB follower, the static curve with guard and ranges (vectorised
  form of gfx-mbcomp's Lift law), the auto-gain integral, the octave meters.

### 2.12 v1 and deferred

| v1 | Deferred (appended later, ids free) |
|---|---|
| Live and Studio; `window` 512 – 4,096; downward + upward with guard and ranges; `res`; Main / Key; auto gain, mix, out, delta, bypass; 10 meters; mono and stereo (linked) | **Sidechain Matching** (thresholds = key spectrum + offset: a spectral morph); a **separate upward threshold** (`lift_thresh`, the OTT layout); the legacy **quadratic `curve`**; **spectral expansion / gating** (users find it unmusical); Mid / Side; a **hybrid** mode (Live under a split frequency, Studio above) if the bench shows Live's highs too coarse; `res` per mode |

## 3. Parameters

All ranges linear in real units (contract). No default depends on another parameter. The ten catalogue
ids keep their unit; three are converted once for golem's presets (§3.4). Enums only grow at the end.

### 3.1 Front panel (6)

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `thresh` | Threshold | f | −80 – 0 dBFS | −36 dBFS | EXP rule, applied per hop | ✓ | Band level (a sine's dBFS) at `center` where the work starts. Catalogue −60 … 0, widened for the conversion |
| `down` | Down | f | 1 – 20 :1 | 4 | per hop | | Downward ratio above the curve; 1 = off |
| `up` | Up | f | 1 – 20 :1 | 1 | per hop | | Upward ratio below the curve, guarded; 1 = off. Full strength at every frequency (no legacy rolloff) |
| `slope` | Slope | f | −24 – +24 dB/oct | 0 | per hop | ✓ | Tilts the threshold about `center`; 0 = flat for pink noise; − = highs held sooner |
| `attack` | Attack | f | 1 – 1,000 ms | 150 ms | per hop | | Level rising; the window adds N / 4 – N / 2 (§2.4) |
| `mode` | Mode | e | Live · Studio | Live | set-up: 10 ms out, switch, 10 ms in | | Live: zero latency, two-thirds of an octave. Studio: per bin, latency N − 1 |

### 3.2 Expert

| id | Name | Type | Range | Default | Smooth | Role |
|---|---|---|---|---|---|---|
| `center` | Center | f | 100 – 8,000 Hz | 420 Hz | per hop | Pivot of `slope` (catalogue) |
| `release` | Release | f | 5 – 2,000 ms | 300 ms | per hop | Level falling (catalogue) |
| `window` | Window | e | 1024 · 2048 · 4096 · 512 | 1024 | set-up (Studio latency); `SizeFade` | Samples at 48 kHz, × rate family. Catalogue order kept; 512 appended (open question 3) |
| `res` | Resolution | f | 0 – 12 st | 2 st | per hop | Band width of the detector, both modes; 0 = the main lobe |
| `knee` | Knee | f | 0 – 24 dB | 6 dB | per hop | Quadratic, both directions (gfx-mbcomp's id) |
| `range` | Range | f | 0 – 60 dB | 24 dB | per hop | Deepest cut per bin (gfx-mbcomp's id) |
| `lift_range` | Lift Range | f | 0 – 24 dB | 12 dB | per hop | Largest boost per bin (gfx-mbcomp's id) |
| `guard` | Noise Guard | f | −96 – −36 dBFS | −72 dBFS | per hop | Nothing under it is lifted (gfx-mbcomp's id and law) |
| `source` | Source | e | Main · Key | Main | 20 ms crossfade of the gains | §2.7 (gfx-gate's id and options) |
| `auto_gain` | Auto Gain | b | off / on | on | 50 ms | §2.8 |
| `mix` | Mix | f | 0 – 100 % | 100 % | 20 ms | ✓ Linear, dry aligned (catalogue) |
| `out` | Output | f | −24 – +24 dB | 0 dB | 20 ms | ✓ After the mix (catalogue id) |
| `link` | Stereo Link | f | 0 – 100 % | 100 % | 20 ms | 2 / 2 only: each channel's band power mixed toward the louder |
| `delta` | Delta | b | off / on | off | 10 ms | Hear wet − dry |
| `bypass` | Bypass | b | off / on | off | 10 ms | Keeps the latency (§2.8) |

**Meters** (read-only): `gr`, `lift_gr`, `dyn_63`, `dyn_125`, `dyn_250`, `dyn_500`, `dyn_1k`, `dyn_2k`,
`dyn_4k`, `dyn_8k` (dB). **21 parameters + 10 meters.** Groups for golem's Designer
(`visuals/data/groups.json`): **Curve** (`center`, `knee`, `range`, `lift_range`, `guard`, `res`),
**Timing** (`release`, `window`), **Routing** (`source`, `link`, `mix`, `out`, `auto_gain`, `delta`,
`bypass`).

### 3.3 Why these choices

- **The front panel** is the catalogue's six main knobs minus `center` (irrelevant at slope 0, a
  set-and-forget) plus `mode`, which decides whether the block may sit in a guitar lane at all.
- **`window` default 1,024**, not the catalogue's 2,048: Live's detection lag and Studio's latency halve
  (10.7 / 21.3 ms), and 1/6 octave is still resolved above 1.6 kHz (800 Hz at 2,048, §2.2); the guitar's low E sits at
  1.76 bins, above the 1.5-bin floor. Every legacy preset sets `window`,
  so none changes.
- **`512` appended** to `window`: Studio at 10.6 ms for keys and a backing voice. Its detector merges
  everything under about 400 Hz (±2 bins = ±188 Hz). Option rank 3, after 4096 (append-only rule).
- **No `overlap` parameter**: the hop is fixed per mode (§2.5, §2.6).
- **`thresh` widened to −80 dBFS** so the converted legacy values (§3.4, down to −76) fit.

### 3.4 Conversion from the `spectral-comp` catalogue (golem engine table)

The legacy values mean other things (§1.1). golem's seven presets and saved chains are converted once
(contract: "switch to real units… the design gives the conversion formula"), with N the preset's
`window`:

- `thresh` ← thresh − 10 log10(N / 96) (the legacy tone reading at 16× overlap: −10.3 / −13.3 / −16.3 dB);
- `slope` ← 0.693 · (slope − 3) (natural log → octaves, and the legacy −3 "pink" offset made explicit);
- `up` ← 1 / ((1 / up)(1 − t) + t), t = 0.75 · ln(center) / ln 22,050 (the legacy upward rolloff, read
  at `center`);
- `attack` ← 0.5 · attack (the legacy follower is linear in magnitude and covers 87 % of a 30 dB rise in
  τ, a dB follower 63 %; Review change 12, which improves the fit on all three DIs);
- for `mix` < 100 % (m the legacy mix, g its linear output gain): `out` ← 20 log10(1 − m + m · g), `mix` ←
  100 · m · g / (1 − m + m · g), exact with auto gain off;
- `down`, `center`, `release`, `window`: as they are. Converted presets also set
  `range` 60, `lift_range` 24, `guard` −96, `auto_gain` off (the legacy block had no caps and no
  make-up), `res` 2.

**Fitted, not assumed** [P]. Mean gain per octave (125 Hz – 8 kHz) of the reference (`proto/ref.py`)
against ours (Studio) on the clean DI, over the seven presets. The tone conversion above fits best: **RMS
difference 1.7 dB, worst 7.0 dB** (pink-noise-wash). The alternative, "pink-flat" slope conversion
(0.693 · slope + 0.93) gives 2.0 dB RMS. With `attack` halved the review measured clean 1.45 dB RMS
(worst 3.2), lead-clean 1.52 (4.7), crunch 1.66 (3.9). On pink noise the two families differ by design
(one bin against a band): with this conversion ours cuts 6 – 11 dB more at 8 kHz (pad-hold 10.9) and
2 – 3 dB more at 125 Hz.

| Preset | `thresh` | `slope` | `up` |
|---|---|---|---|
| gentle-glue (2048) | −24 → −37.3 | 0 → −2.08 | 1.2 → 1.10 |
| de-harsh-vocal (1024) | −30 → −40.3 | −6 → −6.24 | 1 → 1 |
| keys-even (2048) | −22 → −35.3 | −1.5 → −3.12 | 1.3 → 1.14 |
| pad-hold (4096) | −30 → −46.3 | 0 → −2.08 | 3 → 1.55 |
| pink-noise-wash (4096) | −40 → −56.3 | 0 → −2.08 | 12 → 2.01 |
| vocal-smooth (2048) | −26 → −39.3 | −3 → −4.16 | 1.5 → 1.19 |
| vocal-upfront (2048) | −32 → −45.3 | 0 → −2.08 | 3 → 1.50 |

`mode` for converted presets: **Studio** keeps them closest to what was voiced (they already paid the
latency through PDC). Whether golem moves them to Live is open question 1. The two presets with `mix`
< 100 % become pink-noise-wash **mix 42.9 %, out −3.09 dB** and vocal-upfront **65.0 %, −1.35 dB**
(exact, by the formula above). Every converted `attack` is halved: 75 / 10 / 25 / 75 / 150 / 30 / 40 ms
in the table's order.

## 4. Applications

Reference settings become block presets (`presets/gfx-spectralcomp.json`, presets/README format). The
seven legacy ids (`gentle-glue`, `de-harsh-vocal`, `keys-even`, `pad-hold`, `pink-noise-wash`,
`vocal-smooth`, `vocal-upfront`) carry exactly the §3.4 conversion and are not redefined here (one
definition per id: Review change 6). No
artist or trademark names. Unlisted values are the defaults. Placement: after a gate and EQ, before
modulation, delay and reverb. On a guitar lane, Live, after the amp / cab when the aim is the amp's
tone, or before the drive (keyed from the DI) to even what feeds it.

| Preset id | Settings to start from | What it is for |
|---|---|---|
| `even-clean-strum` | Live, thresh −40, down 3, up 1.5, slope 0, attack 30, release 200, window 1024 | Strummed clean chords: the low strings stop masking the top ones; no broadband pumping |
| `glassy-arpeggio` | Live, thresh −44, down 2, up 2.5, attack 50, release 400, lift_range 9 | Arpeggios whose quiet notes ring as loud as the picked ones |
| `crunch-de-fizz` | Live, `source` Main, thresh −42, down 4, up 1, slope −4, center 2000, attack 20, release 120 | After a drive: the fizz above 3 kHz is held as the gain rises (a DI key would not see the fizz: §2.7) |
| `bass-even-strings` | Live, thresh −38, down 3, up 1.5, slope +2, center 200, attack 60, release 300, window 2048 | Bass whose strings read at different levels: lows held, fingers' detail kept |
| `bass-growl-lift` | Live, window 2048, thresh −46, down 2, up 3, slope −2, center 800, lift_range 9, guard −66 | Brings up the growl and finger noise without lifting amp hiss |
| `vocal-smooth-live` | Live, thresh −40, down 3, up 1.5, slope −3, center 1000, attack 40, release 200 | A live voice: ringing resonances and harsh vowels held, zero latency in the in-ears |
| `vocal-de-harsh` | Live, thresh −44, down 4, up 1, slope −6, center 3000, attack 15, release 100 | Harshness and sibilant edges of a bright mic; with gfx-deesser for the sibilants themselves |
| `spectral-wash` (a new sound beside the converted `pink-noise-wash`: guarded, `res` default) | Studio, window 4096, thresh −56, down 8, up 6, attack 300, release 1000, lift_range 24, guard −84, mix 60, out −6 | Anything squashed toward pink noise: an ambient bed. The legacy "pink-noise wash", guarded |
| `make-room-for-voice` | Live, `source` Key, thresh −50, down 6, up 1, attack 10, release 150 | On a keys or guitar lane keyed by the singer's lane (needs `key_from`, §6): the instrument steps out of the voice's harmonics only |
| `fill-the-gaps` | Studio, `source` Key, thresh −50, down 1, up 3, lift_range 9, guard −60 | A pad keyed by the lead: rises where the lead leaves room |
| `tame-ringing-room` | Live, thresh −30, down 6, up 1, res 2, attack 5, release 80, window 1024 | An acoustic or a mic that rings: the ring is held over two-thirds of an octave as it builds (63 % at the output in about 11 ms). It does not replace a notch |

What it is not: a transient shaper (gfx-transient), a fixed-depth duck (gfx-ducker), a de-esser
(gfx-deesser: wideband or split, at zero latency and with a sibilance detector), a multiband with
crossovers to set (gfx-mbcomp). A spectral comp is a levelling tool for the spectrum.

## 5. Bench suite (on top of gfx-bench's common checks, run with and without a key)

Targets for the coded plugin, and what the prototype measured [P]:

| Check | Target | Prototype |
|---|---|---|
| Latency | Live 0; Studio N − 1, at every rate family and `window` | N − 1 exactly at 512 – 4,096 |
| Transparency | Studio at ratios 1: null vs delayed input ≤ −120 dB (f32); Live under threshold: bit-exact (bypassed cascade) | −312 dB (f64) |
| Calibration | sine 100 Hz – 10 kHz reads its dBFS ±0.1 dB, N 1,024 – 4,096; pink slope over 100 Hz – 10 kHz within ±1 dB / oct | ±0.05 dB; −0.92 / −0.33 dB / oct |
| Static curve | Studio ±0.15 dB (knee 0 / 6, down and up); Live lone tone ±1 dB to −18 dB, ±2.5 dB to −24 dB; Live program ≤ 0.6 dB at the centres | Studio ±0.10; Live with the target rule (review): ±1.0 / ±2.1 dB, program 0.44 – 0.46 dB |
| Live against Studio | same settings: pink noise ≤ 0.3 dB, DIs ≤ 0.3 dB in total | review `r6`: 0.00 dB, 0.13 dB |
| Rates | 44.1 – 192 kHz: Studio ±0.1 dB of the 48 kHz figure; times within 5 % | Studio −17.99 / −18.00 dB at all four |
| Ballistics | Studio: 63 % within τ + N / 2 ± 10 %; Live measured at the output (§2.4) | §2.4 |
| Hop spurs | Studio: tremolo ≤ −70 dBc, steady tones ≤ −55 dBc at 82 – 110 Hz and ≤ −65 dBc above; Live ≤ −70 dBc with ramped coefficients | −78 / −79 dBc; Live −57 to −61 dBc held per tick (ramping not prototyped) |
| Musical noise | pink-noise gain bin-to-bin ≤ 0.3 dB, over time ≤ 0.8 dB at the default `res` | 0.25 / 0.67 dB |
| Pre-echo | Studio, 1/6 oct, N 2,048: ≤ −15 dB in the last 5 ms, ≤ −40 dB 5 – 20 ms; Live: none above the floor | −14.8 / −41.4 dB; Live −72 (floor) |
| Guard | −80 dBFS noise, up 20: no band lifted; a decaying note's boost ≤ level − guard + 1 dB | law in the prototype (`static_gain`) |
| Auto gain | pink −18 dBFS RMS = bypass ±0.5 dB at every setting; clean DI at defaults ±1 dB | clean DI +0.6 / −0.6 dB |
| Key | silent key: output = input, no lift, auto gain 0 dB; key pluck train: the main's harmonics cut, 200 Hz untouched in Studio | §2.7 figures |
| Mix | Studio 50 %: no notch > 0.2 dB; Live: stated dip ≤ 0.6 dB at −12 dB cuts | Live 0.15 / 0.56 / 1.64 dB at −6 / −12 / −24 |
| Switches | `mode`, `window`, `source`, `bypass` under a held 82 Hz note: clicks ≤ −60 dB re peak | to bench |
| Legacy conversion | the seven converted presets within 2 dB RMS of the reference's per-octave gain on the clean, lead-clean and crunch DIs | 1.7 dB (clean); review with `attack` halved: 1.45 / 1.52 / 1.66 dB |
| CPU (48 kHz) | Live ≤ 0.7 µs mono / 1.2 µs stereo, Studio ≤ 0.3 / 0.55 µs; worst 64-sample block ≤ 25 % | detector / Studio timed 60 – 177 ns; ACGE scaled |
| Listening | the DIs through every preset, Live vs Studio level-matched; Studio's pre-echo on pick attacks; Live's two-thirds octave against Studio's bins | Raph |

## 6. What golem-rack must provide

1. **The catalogue entry `spectral-comp` moves to golem-fx**, keeping the ten ids, with:
   - `thresh` −80 … 0 dBFS, `window` options 1024 · 2048 · 4096 · 512 with **default 1024** (golem writes
     catalogue defaults at insert), no `values` / `map.mode: raw`, the new expert ids (`source`, not
     `detect`);
   - **plain linear values** instead of `map.mode: "pow"` (the legacy plugin's tapers) and no
     `defaults: "compile"` workaround;
   - the seven presets **converted once** (§3.4), and `mode` chosen for them (open question 1).
2. **The catalogue note changes**: Live has no latency, so the rule "never in the base path of a guitar
   lane" applies to Studio only. Studio's latency changes with `mode` and `window`: "set before playing"
   (gfx-mbcomp `phase`, gfx-limiter `lookahead`).
3. **`key: true`** for `spectral-comp` in the engine table (the lane's DI, as for gfx-gate and
   gfx-ducker); `key_from` (another lane's voice) is gfx-ducker §6 item 2, for the `make-room-for-voice`
   preset. On Windows / macOS, where golem routes no key, `source` Key does nothing: say so on the block.
4. **Meters**: `gr`, `lift_gr` and the eight signed `dyn_*`, drawn as a bar curve (gfx-mbcomp's signed
   meters are the precedent).
5. **Latency at preset level**: a preset that switches Live ↔ Studio changes the lane's latency; both
   engines must re-sync cleanly (gfx-ducker §6 item 6).

## Visual

- **Category `dynamics`**, family slate `#4a5a6e`, **Thrash** finish (`FAMILY_FINISH.dynamics`). **No
  crate hue**, as gate, comp, limiter, de-esser, gfx-transient, gfx-mbcomp and gfx-ducker: the motif
  tells them apart.
  - ΔE*ab [A, CIELAB D65, ΔE76, against every body hue of `METAL_HUE` and `METAL_CRATE_HUE`]: the slate
    is ΔE 12 from gfx-gapmeter's grey `#6b7480` (an existing pair, told apart by finish: gapmeter is
    anodized), then filter cyan 38, gfx-radio 43, gfx-acoustic 47. It adds no new near pair.
  - Weighed and rejected as crate hues:
    - golem's legacy tile `#8a94a3` (L* 61): ΔE 13 from gfx-gapmeter, 24 from the slate. A pale grey
      utility on the board;
    - a "spectral" teal-steel `#3f7f96`: ΔE 17 from gapmeter, 18 from the slate, 20 from filter cyan. It
      would read as a filter;
    - an indigo slate `#4a4f8e`: ΔE 28 from the slate, 35 from gapmeter, 36 from gfx-stretch's
      periwinkle. The only distinct one, but a fourth blue-violet among stretch, delay and eq, and it
      breaks the family the other seven dynamics pedals keep.
  - None says "dynamics" better than the family's slate.
  - Rune light: the family's `#5fb0ff` (`RUNE_LIGHT.dynamics`).
- **Motif `skyline`** (new case in `motifParts`): **a spectrum evened by a tilted line**.
  - In the band of `tiers` and `bins`, either side of the rune (clear of the cartouche: |x − cx| ≥ cw / 2
    + 2): bars 2 px wide every 5 px on a baseline y0 = cy + ch / 2 + 6. Their "input" heights are a fixed
    irregular list, as `bins` (`[.35, .9, .5, 1, .25, .7, .95, .4, .6, .3, .85, .55]`).
  - A **threshold line** from (0, cy − 0.15 ch) to (W, cy − 0.05 ch), falling to the right: a gentle
    slope, the `slope` control. It is broken at the cartouche like the bars (|x − cx| ≥ cw / 2 + 2).
  - A bar whose input top is above the line is **cut at the line**, and a short faint stub (opacity .3)
    above it shows how much was taken; a bar under the line is drawn to its height and **extended
    upward by a third of its gap** to the line at opacity .55: the lift. So the row reads uneven below,
    flattened along the line.
  - The line itself at opacity .45, 1 px. All `line()`: yellow lined in black on thrash, engraved on
    anodized.
  - Told apart from `bins` (glitch: bars sliced and slipped sideways), `tiers` (mbcomp: four horizontal
    meter bars with clamps), `bank` (vocoder: bells) and `scale` (geq: fader slots).
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-spectralcomp": {"name": "Spectral", "blocks": ["spectral-comp"],
                       "knobs": ["thresh", "down", "up", "slope", "attack"],
                       "selectors": ["mode"], "category": "dynamics"}
  ```
- **Glyph**: `visuals/glyphs/spectral-comp.svg` exists (five bars and a down arrow). Proposed refinement
  in the motif's language, five bars evened by a tilted line, uneven below it:

  ```svg
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>Spectral compressor</title><path d='M2.5 6.5 21.5 8.5'/><path d='M4.5 21.5V8.5M8.5 21.5V15.5M12.5 21.5V9.4M16.5 21.5V16M20.5 21.5V10.2'/><path d='M8.5 12.6V11M16.5 13.1V11.5' opacity='.5'/></svg>
  ```
  Tall bars stop just under the line, which falls to the right as in the motif; short ones carry a faint
  lift stub with a real gap past the round caps (the review's geometry; to check at 16 px).
- No maker's livery, logo or trademark.

## Open questions for review

Settled at the review of 2026-11-15, except those marked open (the Review's "Still open" table).

1. **`mode` of the converted legacy presets**: **Studio** (closest to their voicing, latency already paid
   through PDC). The block's default is **Live**. Open for Raph: whether golem later moves them to Live.
2. **Live's resolution and accuracy**: **ship it, with the review's target rule** (power-weighted ±1/3
   octave, then [¼ ½ ¼]) and c = 0.5: a two-thirds-octave compressor, ±1 dB to −18 dB on lone tones,
   ≤ 0.5 dB on program. Sixth-octave bells (about 0.9 µs mono) and the hybrid stay deferred.
3. **`window` 512 appended**: **kept** (rank 3). Its lowest clean fundamental is 141 Hz (§2.2).
4. **Studio's pre-echo**: **accepted** in v1, no transient guard; `window` 1024 for plucked sources in
   Studio, and Live for fast attacks.
5. **Auto gain's reference** (pink at −18 dBFS RMS): **kept** as a fixed constant; 0 dB while `source`
   is Key.
6. **The detector's floor at low frequencies**: **accepted and stated** (§2.2).
7. **One threshold** for both directions: **kept** (catalogue); `lift_thresh` is a later append.
8. **Hop N / 8 in Studio**: **kept**; N / 4 cannot be rescued by interpolation (steady-tone images); N / 16
   is the upgrade if the −58 dBc low images are heard. Live detects at N / 8 too.

## Review (Fable 5.2, 2026-11-15)

> **How this section relates to the body.** Every change below, blocking (B) and recommended (R), has
> been applied in the body by the author on 2026-11-15: the Live target rule and c = 0.5 (§2.6), `res` as
> set in both modes and the shared make-up (§2.1, §2.8), auto gain 0 dB under a key and `source` Main ·
> Key (§2.7, §3.2), the narrowed DI-key claim and `crunch-de-fizz` on Main (§2.7, §4), one definition per
> preset id (§4), the worst-block rule (§2.9), hop N / 8 in Live, ballistics at the output, the
> low-fundamental floor, the reset rule, the meter definitions, the conversion (`attack` halved, exact
> `mix` / `out`), the CPU re-scaling, the facts and the visual. **Where the body and this section
> differ, this section's decisions override the body.** The prototypes in `proto/` (`sc.py`) still
> implement the draft's Live targets. The reviewer's scripts, which implement the rule, are copied to
> `proto/review/`; run them from `proto/` with `PYTHONPATH=. python3 review/<script>.py`.

**Verdict: Good**, Strong once the Live target rule below is applied and the bench confirms it in Rust.
The structure is right for a stage rig: one calibrated band-power detector, a zero-latency
minimum-phase applier as the default, and a windowed per-bin applier when the latency can be paid. I
found no flaw that forces a different engine. These survive a re-run: the detector's calibration, the
N − 1 latency, the hop-rate argument, the legacy formulas and the CPU timing of the STFT part. Six
points did not survive the numbers:

- Live "half an octave, ±1.5 dB": 27 third-octave bells cannot draw a half-octave plateau. Neighbours
  fight, and one hits the ±40 dB clamp.
- "A preset sounds like the same compressor in Live and Studio" is false on dense material, and the
  auto gain differs by 2.1 – 2.5 dB between modes.
- Auto gain in Key mode: a silent key leaves the make-up on the lane (+16.9 dB on
  `make-room-for-voice`).
- The §2.4 table as "the delay seen at the output" in Live: the output is about 5 ms later.
- "Hop rate free of spurs" holds for moving gains at 1 kHz, not for steady low tones.
- The worst-block analysis misses stereo and Live.

One rule fixes the first two. None of the six touches the structure.

**What I checked.** My scripts and copies of the prototypes are in the session scratchpad
(`review-spectralcomp/`; the scripts are copied to `proto/review/`): `r1_lowfreq.py`,
`r2_static_spurs.py`, `r2b.py`, `r3_gain_smooth.py`, `r4_live_static.py`, `r5_live_targets.py`,
`r5b.py`, `r6_mode_consistency.py`, `live2.py`, `r7_live_dyn.py`, `r8_live_program.py`,
`r9_stream_latency.py`, `r10_asym.py`, `r11_fit_times.py`. Nothing in the repo was modified.

- **Legacy source** (`curve.rs`, `compressor_bank.rs`, `lib.rs`):
  - `evaluate_ln` computes `intercept + slope · (ln f − ln center)`, so the slope is per neper.
    Confirmed.
  - `curve_params` uses `curve_slope − 3.0` in Internal mode. Confirmed.
  - Confirmed: gc = 1 / ((ov / 4) · 1.5) / N, applied as √gc before and after the FFT; default overlap
    order 4 (16×); upward rolloff 0.75 and downward 0 by default; knee 6 dB.
  - The follower is a one-pole on the **linear magnitude** (see change 12).
- **Prototypes re-run**:
  - `m5.py` and `m5fit.py`: 1.71 dB RMS, worst 6.97, as stated;
  - the Rust timing: detector 60 – 77 ns, Studio 132 – 140 ns per sample; N = 8,192: 129 / 144 µs per
    frame;
  - the ΔE figures: 12.2 / 23.7 / 18.4 / 27.9.
- **New measurements**:
  - Studio as a stream with random host blocks;
  - steady-tone images by hop;
  - readings of low notes;
  - ACGE section gains on lone tones;
  - five Live target shapes;
  - Live against Studio on pink noise and four DIs;
  - Live end to end: lone tones, two DIs, a step, tick spurs;
  - an asymmetric analysis window;
  - the time constants of the conversion.

**Limits of my measurements.** The Live rule of change 1 was measured in the Python prototype at 48 kHz,
in Self mode, with coefficients held per tick. `r6` compares target curves, not realised responses.
`r8` runs end to end on 4 s of two DIs. Key mode, other rates and ramped coefficients are left to the
bench.

### Changes made, and why

B = blocking: apply before coding. R = recommended.

| # | | Change | Why |
|---|---|---|---|
| 1 | B | **Live target rule** (§2.1, §2.6, §3.2). The detector runs at `res` **as set, in both modes**; "Live holds `res` ≥ 6 st" is removed. For each of the 55 design points p, G_p = 10 log10(Σ P_k · 10^(g_k / 10) / Σ P_k) over the bins with \|log2(f_k / f_p)\| ≤ 1/3, and at least the nearest bin. P_k is this frame's bin power of the detected signal plus a 1e-20 floor. Then T_p = ¼ G_(p−1) + ½ G_p + ¼ G_(p+1), with the ends repeated. This costs two more prefix sums per hop. Live is described as a **two-thirds-octave** compressor (width at half depth 0.72 octave) | As written, a lone tone asks for a half-octave rectangle. The solve answers with opposing neighbours: −24.5 and +24.2 dB of section gain for a −24 dB request at 1 kHz, and one section at the −40 dB clamp at 5 kHz. It misses by −5.0 to +4.2 dB end to end. With the rule (`r7`): within ±1.0 dB down to −18 dB, and ±2.1 dB at −24 dB, at 150 Hz, 1 kHz, 1,122 Hz, 5 kHz and 7.1 kHz; sections ≤ 23 dB; upward region within +0.5 dB (was +2.4). On program (`r8`, clean and high-gain DIs): worst error at the centres 0.44 – 0.46 dB (was 0.98 – 1.08), largest section 9 dB (was 12 – 14). Changing λ from 1e-3 to 3e-2 changes nothing (`r5`) |
| 2 | B | Same rule. **"Same compressor in both modes" becomes true**, and the auto make-up is computed from `res` as set, the same value in both modes | The held `res` raises a noise-like band reading by 4.8 dB. As written, Live cuts pink noise 2.1 – 2.2 dB more than Studio (3.0 – 3.2 dB per octave above 1 kHz), and its make-up is 2.1 – 2.5 dB higher: 7.31 against 4.82 dB at the defaults. The DIs' wet levels differ by only 0.0 – 0.7 dB, so a mode switch moved a guitar by about 2 dB. With the rule (`r6`): pink 0.00 dB apart, the four DIs within 0.13 dB in total, 1.4 dB worst per octave |
| 3 | B | **Auto gain is 0 dB while the detector is keyed** (§2.7, §2.8) | The make-up is computed from the parameters and applied to the wet. With a silent key the lane would come out at +16.9 dB on `make-room-for-voice` (thresh −50, down 6; `r10`). "No key = no change" could not hold |
| 4 | B | **`detect` Self · Key → `source` Main · Key**, gfx-gate's and gfx-swell's id and options | The contract's shared ids have one meaning, and `detect` is already Absolute · Relative in gfx-deesser and gfx-ducker. State which no-key rule applies: gfx-gate falls back to Main, this design does nothing. Keep "does nothing", and say so |
| 5 | B | **Key from the DI: claim narrowed** (§2.7, §4). `crunch-de-fizz` uses `source` Main. "No detector chasing distortion hash" is limited to the bands the DI and the processed signal share (fundamentals and low harmonics) | Detection is band by band. The drive makes the fizz above 3 kHz from lower bands. The DI has almost nothing there, so a DI key never reads over the threshold in those bands |
| 6 | B | **One definition per preset id** (§3.4, §4). The seven legacy ids carry the §3.4 conversion exactly. §4 drops its own `gentle-glue`, `keys-even` and `pad-hold` rows, which gave different values under the same ids. It drops or justifies the near-duplicates `vocal-upfront-studio` and `spectral-wash` | golem picks presets by id. §4's `keys-even` (thresh −36, up 1.3) and `pad-hold` (up 2) are not §3.4's (−35.3 / 1.14; 1.55). §4's `gentle-glue` has auto gain on where §3.4 says off |
| 7 | B | **Worst-block rule by size and channel count** (§2.9), for both modes. A frame runs in one piece only while channels × N ≤ 16,384 at 44.1 / 48 kHz (always true), ≤ 8,192 at 88.2 / 96 kHz, and ≤ 4,096 at 176.4 / 192 kHz. Above that, the `due` schedule applies: +H of reported latency in Studio, +H of detection lag in Live | Two linked channels complete their frames on the same sample. At 192 kHz the default `window` in stereo costs 2 × 72 µs against a 333 µs block (43 %), and Live's detector 2 × 61 µs (37 %). The draft treats only Studio at 16,384. At 96 kHz, `window` 4096, stereo: 288 µs of 667 |
| 8 | R | **Bell width constant c = 0.5 for this plan**, set per plan (gfx-geq keeps 0.4) | `r5b`, same target shape. Error at the tone: ±0.5 dB at −12 dB (c = 0.4: −0.6 … +0.9), and −1.6 … +2.5 dB at −24 dB (c = 0.4: −1.9 … +3.6). Ripple on a two-octave plateau: 0.27 / 1.32 dB p-p at −12 / −24 dB (c = 0.4: 0.58 / 2.11). On program: 0.35 – 0.37 dB worst. c = 0.3 and 0.2 are worse on both counts |
| 9 | R | **Live's lag is stated at the output, and Live detects at hop N / 8** (§2.4, §2.6), one hop for both modes | The targets ramp over a hop and the solve converges over ticks. A +30 dB step with attack 1 ms, measured at the output (`r8`), 10 / 63 / 90 %: at `window` 1024, 8.7 / 13.7 / 18.2 ms at hop N / 4 against the table's 8 / 8 / 13; 6.2 / 10.8 / 15.3 ms at N / 8; 5.7 / 9.8 / 14.4 ms at N / 16. `window` 512: 3.2 / 6.7 / 8.8 ms. `window` 2048: 15.4 / 29.4 / 40.3 ms at N / 4, 13.4 / 22.8 / 33.8 ms at N / 8. Until then the onset passes uncut, +19.9 dB over the settled level: Live never anticipates. Cost: +61 ns per sample |
| 10 | R | **Studio keeps hop N / 8; the claim and the bench row are corrected** (§2.5, §5). Open question 8 is closed | Steady sines under an 18 dB cut (`r2`) leave images at f ± k · fs / H: **−58 dBc at 110 Hz and −62 dBc at 82 Hz (N = 1,024)**, −67 to −68 dBc at a few other frequencies, median −77. "Free of spurs" holds for the tremolo test only. At **N / 4 the same steady tones give −35 to −58 dBc**. The cause is a gain that is not flat over the lobe, not only gain steps, so interpolating gains across the frame cannot rescue N / 4. At N / 16: −77 dBc worst, most −95 to −117 (`r2b`), for twice the frames (about 0.28 µs). Bench bounds: tremolo ≤ −70 dBc; steady tones ≤ −55 dBc at 82 – 110 Hz and ≤ −65 dBc above. N / 16 is the named upgrade if the low images are heard. A wider minimum band (±3 or ±4 bins) or smoothing the gain across bins moves the worst case but does not remove it, and costs resolution or 0.3 – 1.2 dB of tone accuracy (`r2b`, `r3`) |
| 11 | R | **Lowest clean fundamental stated** (§2.2, §3.3): 1.5 bins = 141 Hz at `window` 512, 70 Hz at 1024, 35 Hz at 2048. Bass presets set `window` 2048; `bass-growl-lift` did not. `tame-ringing-room` moves to 1024 or states the limit | The calibration was checked from 100 Hz only. A −20 dBFS sine at 0.88 bin (41 Hz at 1,024; 82 Hz at 512) reads −1.3 / +2.4 dB from frame to frame; at 1.17 bins, −0.3 / +1.0 dB (`r1`). From 1.57 bins it is steady within 0.02 dB. Effect at 41 Hz, N = 1,024: the gain is 0.8 – 1.4 dB too deep, with a spur at −53 dBc for attack 1 / release 20 ms and −69 dBc for 10 / 50 |
| 12 | R | **Conversion** (§3.4): (a) `attack` ← 0.5 · attack. (b) For the two presets with `mix` < 100 %, exactly: `out` ← 20 log10(1 − m + m · g) and `mix` ← 100 · m · g / (1 − m + m · g), with m the legacy mix and g the legacy linear output gain. That gives pink-noise-wash **42.9 %, −3.09 dB** and vocal-upfront **65.0 %, −1.35 dB**. (c) The pink-noise sentence is corrected. (d) `results.txt` is re-run for `m5.py` | (a) The legacy follower is linear in magnitude: it covers 87 % of a 30 dB rise in τ, where a dB follower covers 63 %. With attack × 0.5 the fit improves on all three DIs (`r11`), RMS (worst): clean 1.72 → 1.45 dB (7.0 → 3.2), lead-clean 2.45 → 1.52 (7.9 → 4.7), crunch 1.90 → 1.66 (4.9 → 3.9). As written, lead-clean misses the bench's 2 dB bound. Scaling the release did not help consistently. (b) "No conversion proposed" was not needed: with auto gain off the formula is exact. (c) With the chosen tone conversion, ours cuts pink noise 6 – 11 dB more than the legacy at 8 kHz (pad-hold 10.9) and 2 – 3 dB more at 125 Hz. The README's "6 – 7 dB more at 125 Hz" belongs to the rejected pink-flat conversion. (d) The `m5` block of `results.txt` still printed the pink-flat slopes (+0.93) |
| 13 | R | **Live CPU re-scaled** (§2.9): cascade 138 × 27 / 20 = 186 ns; solve 7.8 µs × (27 × 55) / (20 × 39) = 14.9 µs per tick = 232 ns; detector at N / 8 about 122 ns; rule 1 about 15 ns. **Live ≈ 0.55 µs mono**; stereo ≈ 0.9 µs linked (one solve), 1.1 µs unlinked. Budgets: Live ≤ 0.7 µs mono and ≤ 1.2 µs stereo; Studio unchanged | "250 – 350 ns" under-scales: gfx-geq measures 270 ns with 20 sections while a fader moves, and here the gains always move. Still inside the suite's norm. Not timed: the upward branch, the meters, and Studio's second forward FFT with a key. A solve every second tick is the lever if the bench needs one (not measured) |
| 14 | R | **Latency N − 1 kept, with its condition written** (§2.5): the frame runs inside the host block, at the sample that completes the hop | `r9`: a ring-buffer stream fed blocks of 1 – 699 samples nulls against the input delayed by N − 1 at −314 dB, with no sample read early. N − 2 also works (w[0] = 0); N − 3 does not. N − 1 is correct and one sample conservative. A frame deferred to the end of the block would make the latency depend on the block size |
| 15 | R | **Reset** (§2.4): the follower is instant (α = 1) for the first N samples after a reset, then normal | "First frame sets L̄ = L" reads a window that holds only H samples under its tail, 20 – 30 dB low, so the claim "no slow start-up" does not hold as written |
| 16 | R | **Meters defined** (§2.8). `gr` = the largest, over the eight octaves, of the power-weighted mean of the cut part (−min(g, 0)). `lift_gr` = the same for max(g, 0). `dyn_*` unchanged | As written, `gr` is taken from the signed octave means, so a cut and a lift in one octave cancel, and `lift_gr` has no stated domain |
| 17 | R | **Facts**: §3.3's "1/6 octave is still resolved above 800 Hz" becomes 1.6 kHz at 1,024 (800 Hz is 2,048, as §2.2 says). §6 item 1 adds the catalogue's `window` default → 1024 and drops `values` / `map.mode: raw`. `tame-ringing-room`'s "detection lag 5.3 ms" becomes 3 / 7 / 9 ms at the output; it holds a ring over two-thirds of an octave and does not replace a notch. Preset rows that set `res` in Live now mean something (rule 1) | Accuracy. golem writes catalogue defaults at insert, so the plugin's 1024 would never be seen |
| 18 | R | **Visual**: the threshold line is broken at the cartouche like the bars (\|x − cx\| ≥ cw / 2 + 2); the glyph's line tilts the same way as the motif's (down to the right); the stubs get a real gap: `<path d='M2.5 6.5 21.5 8.5'/><path d='M4.5 21.5V8.5M8.5 21.5V15.5M12.5 21.5V9.4M16.5 21.5V16M20.5 21.5V10.2'/><path d='M8.5 12.6V11M16.5 13.1V11.5' opacity='.5'/>` | The line as specified crosses the rune and the name. The motif's line falls to the right, the glyph's rises. With round caps (0.9 px) the draft's stub at 12.5 overlaps its bar's cap at 13.1, so there is no gap at any size. Computed, not rendered |

### Checked and left as written

- **The Live / Studio split, with Live as the default.** Right for a stage rig. `mode` as the sixth
  face control is justified: it decides whether the block may sit in a played lane.
- **ACGE as Live's engine.** With rule 1 it is accurate where it matters.
  - Sixth-octave bells (54 sections) would cost about 0.9 µs mono and need their own conditioning
    study. Not for v1.
  - A minimum-phase FIR per hop buys bin resolution that a zero-latency filter cannot use in time.
    Rightly rejected.
  - The hybrid stays deferred.
- **Band-power calibration**: tones within 0.05 dB from 1.5 bins up; pink slope −0.92 / −0.33 dB per
  octave. The low-frequency floor is accepted and stated (open question 6).
- **Legacy formulas**: `thresh` − 10 log10(N / 96), `slope` ← 0.693 · (slope − 3), `up` read at
  `center`. The 1 kHz reading in `results.txt`, 0.63 dB under the formula, is Hann scalloping at bin
  21.33, not an error. Converted presets stay in **Studio** (open question 1).
- **Pre-echo** figures, and no transient guard in v1 (open question 4). `window` 1024 for plucked
  sources in Studio.
- **Min-phase mix dips**: stated; not re-measured with rule 1.
- **Parameters**:
  - the ten catalogue ids and units; linear ranges; no default that depends on another parameter (and,
    with rule 1, no hidden hold either);
  - `window` order 1024 · 2048 · 4096 · 512 (append-only), with `512` kept (open question 3);
  - `window` default 1024: guitar's 82 Hz is clean at 1.76 bins, and Live's lag is half that of 2048;
  - `thresh` down to −80 dBFS: wider than "a little", justified by the conversion (−76.3 for a saved
    chain at −60 with 4096);
  - `out` after the mix, one threshold for both directions (open question 7), the front panel;
  - auto gain's pink reference at −18 dBFS as a fixed constant (open question 5).
- **Slate kept, no crate hue**; ΔE figures confirmed. No artist or trademark name in the preset ids.

### Still open

| Point | Owner |
|---|---|
| Rule 1 in Rust with ramped coefficients: lone tones, program, Key mode (weights from the key's power), 44.1 – 192 kHz; tick spurs ≤ −70 dBc (the prototype, held per tick, gives −57 dBc at a 64-sample tick, −65.5 dBc at 16) | golem-fx, at coding; bench |
| Live's two-thirds-octave duck in Key mode (the draft's −6.5 dB at 200 Hz for a 330 Hz key will be wider): acceptable against gfx-ducker's Spectral shape, or keep Key for Studio in the presets | Bench, then Raph |
| An asymmetric analysis window for Live, where nothing is resynthesised. With a fall of N / 4, the gain reaches 63 % at 5.3 ms instead of 8.0 at the frame's end, and the tone reading is ±0.4 dB (`r10`) | golem-fx, after v1 |
| Studio hop N / 16 if the −58 dBc images are heard on bass or low guitar notes | Bench, by ear |
| Bass and voice DIs: every bass and vocal preset, and the conversion fit on voice and keys (the presets' real roles; the fit rests on guitar DIs only) | Raph / golem-rack |
| Catalogue entry: `source` instead of `detect`, `window` default 1024, `thresh` −80, the converted presets with `attack` halved and the two `mix` / `out` pairs, the keyless display on Windows / macOS | golem-rack |
| Latency at 96 / 192 kHz changing with the channel count (change 7), and with `mode` at preset recall | golem-rack, to measure |
| `skyline` against `bins` at tile size (same bar grid and baseline; only the line and the clipping differ); the new glyph at 16 px | Raph, on the family sheet |
| `geq` changes (27 sections, per-plan clamp and c, targets at the points), with gfx-geq's renders nulling before and after | golem-fx, with this crate |
| Converted presets on dense sources: 6 – 11 dB more cut at 8 kHz than the legacy on pink noise, by design of the band detector | Raph, by ear |
