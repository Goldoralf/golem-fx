# gfx-stretch

> **Status: CODED** (v0.1.0, 2026-11-15). Designed 2026-11-14 by Claude Opus 5.5, reviewed by Claude
> Fable 5.2 the same day (Review), coded as reviewed with the deviations listed in "As coded" (at the end).
> gfx-dsp's new `stft` (written from gfx-glitch's engine, which is not touched) cuts every pass of a frame
> into units of ≤ 1,024 elements spread over the hop on sample counts, with a four-step FFT above 16,384
> points; the crate adds f16 takes A / B with a pre-roll loop seam, the Rolling ghost with its resyncs,
> freeze, power-domain (random) and complex (coherent) head crossfades, fshift / pitch / constant-Q spread /
> frame compressor, and the `cloud` law. Measured: identity at ×1 to −90 dB (f32 pool) and −73.2 dB (f16)
> at all 21 sizes; random-mode level within ±0.35 dB of the region at ×1 – ×100; spread σ within 3 %,
> energy within 0.35 dB; fshift 540.15 / 540.00 Hz; compress steps exact to 0.01 dB; resync at 5 s ± 1
> sample, fades of 0.50 s at ×1,024; bench 28 / 28 at 44.1 – 192 kHz; CPU 179 ns per stereo frame at the
> defaults, ≤ 330 ns in every case, worst 64-sample block ≤ 8 % at 48 kHz. Coherent mode at ×8 – ×100 is
> 1 – 3 dB under the `cloud` law, and memory is 57.7 / 128 MB (see "As coded").

golem's **time stretcher**. It plays what you played slower than you played it, from twice as slow to a
thousand times slower:
- **a chord becomes a pad** (×4 – ×10);
- **a note becomes a drone** that lasts minutes (×50 – ×1,000);
- **freeze** holds one instant forever;
- pitch, frequency shift, spread and a frame compressor colour it.

It stands for golem's catalogue block **`stretch`** (Stretch, today CLAP PaulXStretch, category `reverb`). It
keeps the block's thirteen ids: `capture`, `freeze`, `thru`, `amount`, `pitch`, `fshift`, `spread`, `compress`,
`maxlen`, `fft`, `vol`, `spread_on`, `comp_on` ([golem contract](../docs/golem-contract.md)). Two of them
change to real units under the contract's rule for third-party 0 – 1 scales (§3.1).

**What it is not.** gfx-glitch is a spectral processor of the *present*: its wet is the guitar of the last
window, held and bent. gfx-stretch is a **time machine**: its wet is the guitar of seconds or minutes ago,
slowed down (§2.8).

## 1. State of the art

Survey of 2026-11-14. **[V]** read in source code, a paper or a manual · **[M]** manufacturer page or
listing · **[F]** forum post or review · **[A]** computed here (scripts reproducible from the formulas
given) · **[I]** inference, or not re-checked at the source.

### 1.1 The block we replace: PaulXStretch [V]

Read in [essej/paulxstretch](https://github.com/essej/paulxstretch) (GPL-3; its DSP core `PS_Source/` is Paul
Nasca's Paulstretch, GPL-2), the commit golem builds with FFTW on Linux (`provision/install-spectral.sh`).
We study ideas only; no code is copied.

**The algorithm** (`Stretch.cpp`):
1. An input frame of **2B samples** is windowed (Hamming by default, `setFFTWindowingType(1)`) and
   transformed. Only the **magnitudes** are kept.
2. The spectral modules act on the magnitudes (§1.1, modules).
3. The inverse FFT runs with a **fresh uniform random phase** on every bin (a seeded `mt19937`, 2^15 phase
   steps).
4. Output frames have a **hop of B**, so they overlap by half. There is **no synthesis window**: the second
   half of the new frame is joined to the first half of the previous one by a raised-cosine crossfade, then
   multiplied by `(0.854 − 0.146 · cos(2πi / B)) · 2`. That curve lifts the middle of the fade by √2 against
   its ends: a power correction, since the two frames are uncorrelated.
   - The correction is not exact [A]: the expected power ripples by 0.4 dB peak to peak, at twice the frame
     rate.
   - Level [A, simulated from the code]: with the Hamming window and the factor 2, the stretched sound comes
     out **0.8 dB under** the region it reads (noise, a chord, a sine alike).
5. The read position moves by **B / amount** per output hop (`remained_samples += 1 / rap`). With `freeze`
   it stops, and the same magnitudes get new phases on every hop.
6. **Onset detection** (`onsetdetect`, default 0) measures the spectral flux in bands of 500 Hz. Over a
   threshold `20^(1 − s) − 1`, it lets the read position run through the attack at a faster rate, then pays
   that time back.

**Parameter laws** (`PluginProcessor.cpp`):

| Param (ours) | PaulXStretch | Law | Default |
|---|---|---|---|
| `amount` | `stretchamount0` 0.1 – 1,024 | ×, JUCE skew 0.25 (the catalogue already stores ×) | 2 (catalogue 4) |
| `fft` | `fftsize0` 0 – 1 | B = 2^(7 + 10 n) samples, rounded to the nearest 2^a 3^b 5^c 7^d 11^e (FFTW). 2^(7 + 14.5 n) only at the largest prebuffer setting. Window = 2B. **Exponential, and in samples, so it depends on the rate** | 0.7 → B = 16,384, window 682.7 ms at 48 kHz |
| `pitch` | `pitchshift0` ±24 st | Linear. Nearest-bin magnitude resampling | 0 |
| `fshift` | `freqshift0` ±1,000 Hz | Linear. Integer-bin shift (`(int)(Hz / (fs/2) · nfreq)`) | 0 |
| `spread` | `spread0` 0 – 1 | Below. **Strongly non-linear** | 0 |
| `compress` | `compress0` 0 – 1 | Gain = (0.1 · R)^(−p), R = the RMS of the frame's unnormalised magnitudes, with a floor of 10^−3 on 0.1 · R. **One gain per frame**, not per bin. Unity gain sits at 10 / √(0.397 · 2B) [A]: **−21.1 dBFS RMS at B = 16,384**, 3 dB lower per octave of window (−0.2 dBFS at B = 128, −30.2 dBFS at B = 2^17). The floor allows a lift of **+60 dB** at p = 1 | 0 |
| `maxlen` | `maxcapturelen_0` 1 – 120 s | Linear | 10 s |
| `vol` | `mainvolume0` −24 – +12 dB | dB, **on the stretched sound only**; the pass-through is added at unity after it | −3 dB |
| `thru` | `passthrough0` | Bool, 1-block ramps | **off** (golem forces on) |
| `capture`, `freeze` | `capture_enabled0`, `freeze0` | Bools | off |
| `spread_on`, `comp_on` | `enab_specmodule5`, `…8` | Module switches | off (golem forces on) |

- **Spread** blurs the magnitudes along a **log-frequency** axis. The spectrum is resampled onto nfreq points
  spaced logarithmically from 20 Hz to Nyquist. A one-pole runs forward and backward over it, twice, with
  pole a = (1 − 2^(−10 n²))^(16384 / nfreq). Then it is resampled back.
  - Width [A]: four one-poles in cascade make a near-Gaussian with
    **σ = 2,400 · log₂(fs / 40) / (16,384 · (−ln(1 − 2^(−10 n²)))) cents**. Remarkably, it does not depend
    on nfreq.
  - Checked by simulating the code [A]: 0.3 → 1.4 ¢, 0.5 → 8 ¢, 0.7 → 44 ¢, 0.8 → 126 ¢, 0.9 → 410 ¢,
    0.95 → 780 ¢, 1.0 → 1,530 ¢ at 48 kHz. **Half the knob does nothing audible; everything happens
    between 0.7 and 1.**
  - It smooths **amplitudes**, not powers. A spread partial therefore loses power as it widens [A].
  - For the same reason, **what is heard of one partial is the kernel squared**: its power profile has a
    standard deviation of **0.59 σ** [A, computed for this kernel]: 26 ¢ at 0.7, 907 ¢ at 1.0. The conversion
    (§3.4) uses that width.
- **Module order** (`ProcessedStretch.cpp`): harmonics, tonal / noise, **frequency shift, pitch shift**,
  ratios, **spread**, filter, free filter, **compressor**. So f′ = (f + fshift) · 2^(pitch / 12).

**The live model** (`processBlock`, `timerCallback`) [V]:
- `capture` on records into a ring of `maxlen` with a one-block fade. At `maxlen` it finishes by itself
  (`m_auto_finish_record`).
- `capture` off commits the recording. It is then played **looped** from its start; the loop seam is a
  linear crossfade of `loopxfadelen` (0.01 s).
- **The previous take keeps playing while the next one records.**
- `capture` is polled on a **GUI timer**, not on the audio thread, so a stomp lands with timer jitter.
- The stretcher runs on a **background buffering thread** with a prebuffer of 4,096 – 262,144 samples. The
  host sees latency 0, but the stretched sound is late by the prebuffer, and a busy machine underruns it.
- `vol` scales the stretched sound only.

**Forum feedback** [F]: on KVR, users found that on the instrument's own track nothing is heard until capture
starts. The workaround is a bus with input monitoring, and this is why golem forces `thru` on. They use it
for radical ambient and sound-design transformations, not for subtle timing work
([KVR thread](https://www.kvraudio.com/forum/viewtopic.php?p=6957716)). Nothing found measures its live
latency.

### 1.2 Other stretchers and hold machines

| Reference | What makes it special | What we take |
|---|---|---|
| Paulstretch, the method [V: source; I: [arXiv 2206.04279](https://arxiv.org/pdf/2206.04279)] | A phase vocoder whose phases are all randomised. It is smoother than a phase vocoder at extreme factors, and not meant for factors under 1 | The core, at `cloud` 100 % |
| Phase vocoder with identity phase locking (Laroche & Dolson 1999) [V: our `gfx-dsp/pv_shift.rs`] | Peak regions keep their bins' relative phases. Clean at moderate factors, phasey at extreme ones | `cloud` 0 %: the coherent end (§2.5) |
| [Phase Vocoder Done Right](https://arxiv.org/abs/2202.07382) (Průša & Holighaus, EUSIPCO 2017) [V: abstract] | Phase derivatives in time and frequency, integrated by RTPGHI. No peak picking, no transient detection, and few artefacts claimed even at extreme factors | A candidate for the coherent end, to be prototyped against peak locking (Open question 5) |
| Röbel, transient detection and preservation in the phase vocoder (ICMC / DAFx 2003) [I: abstracts] | Transient bins found by time reassignment, and their phases re-initialised | Not for v1: random phases smear attacks by design (and Paulstretch's own time credit is left out too, §2.7) |
| Ableton Live, Texture warp mode [F: secondary sources] | Grain size (2 – 263) and Flux (randomness): granular stretching for pads and noise | `cloud` between 0 and 100 % sits in the same space |
| Hologram Infinite Jets (pedal) [M] | Two-channel capture with infinite sustain, by footswitch or **auto-triggered from the playing**; it is calibrated per instrument | Two takes (A / B) and a crossfade on commit. **Rolling** mode is our hands-free answer |
| golem's own `freeze` block (GoleM Freeze) [V: `golem/docs/spectral.md`] | It averages 4 frames' magnitudes and advances phases at the measured frequency. **Quarantined** since 2026-11-12 ("does not work well") | Freeze holds the magnitudes and lets the phases run. Open question 4: should this pedal also stand for `freeze`? |
| gfx-glitch (ours) [V] | Symmetric STFT of 512 – 32,768 points, hop N / 4, 16 steps spread over the hop, crossfaded size changes, `cloud`, peak-region frequency map | The engine, shared through gfx-dsp (§2.7) |
| Signalsmith Stretch (MIT), Rubber Band R3 (GPL) [I] | Live stretchers aimed at ×0.5 – ×2 with transient care | Out of scope: gfx-pitch's world |

### 1.3 What we conclude

1. **Keep Paulstretch's core**: magnitude only, random phases, a long window. Nothing else makes ×100 sound
   smooth.
2. **Beat it on five points**:
   - **overlap 4 with power-complementary windows**, instead of overlap 2 with a corrected crossfade. The
     expected power is then **exactly constant** (Σ w_s² = 1.5 at every sample), where Paulstretch's ripples by
     0.4 dB at twice the frame rate;
     - This does **not** make a partial fade less [A, both simulated, ×8, 440 Hz]. Random phases make each
       partial fade like a narrow-band noise, and that slow "breathing" is Paulstretch's character. Ours keeps
       it: the same line width (90 % of the power in 2.6 bins against 2.5), the same fading rate (envelope
       correlation 0.18 N against 0.16 N), slightly deeper fades (power variance 0.80 against 0.54; under
       −20 dB 1 % of the time against 0.3 %). On a chord or on noise the 50 ms level moves the same in both
       (standard deviation 1.4 against 1.5 dB at 683 ms on a chord).
     - `cloud` under 100 % is the control that reduces the fading, not the overlap;
   - **no background thread, no prebuffer**: the frame work is spread over the hop on the audio thread by
     sample count. Even 2^18-point FFTs are cut into small units by a four-step decomposition, so CPU is flat,
     results do not depend on the host's blocks, and nothing underruns;
   - **sample-accurate capture** (audio thread, not a GUI timer) and a **seamless loop seam** from a
     pre-roll;
   - **real units with laws that mean something**: spread as a width in cents, the window in ms, compress as
     a level slope;
   - **calibrated loudness**: the wet's power equals the power of the stretched region (Parseval, §2.6),
     whatever the window, the amount or `cloud`.
3. **Add what a live player needs**:
   - a **hands-free Rolling mode** (the maintainer's no-footswitch rule for gfx-glitch, 2026-11-14), next to the
     catalogue's capture gesture;
   - `cloud`, to move from a Paulstretch wash to a coherent slow-motion;
   - stereo `width`;
   - crossfaded commits and resyncs.

## 2. Design

```
in ─┬──────────────────────────────────────────────────────────────── thru (unity, never delayed) ─▶ (+) ─▶ out
    │                                                                                                  ▲
    ├▶ pre-roll ring (250 ms, always) ──┐                                                               │
    └▶ MEMORY (takes A / B, or live ring) ─▶ HEAD(s) ─▶ STFT analysis ─▶ |X| ─▶ FSHIFT ─▶ PITCH ─▶ SPREAD
                                            amount, freeze      (window `fft`)                         │
          ┌───────────────────────────────────────────────────────────────────────────────────────────┘
          └▶ COMPRESS ─▶ phases (cloud, width) ─▶ ISTFT + OLA ─▶ HP ─▶ LP ─▶ vol ─▶ soft limiter ─▶ wet
```

### 2.1 The live model: what a stretcher of a live input can do

A stretcher **outputs slower than its input**. A read head moving at 1 / amount of real time falls behind
the present by (1 − 1 / amount) seconds every second. It can never follow the input, so there are only
three honest behaviours, and the pedal has all three.

| Behaviour | Where the head reads | What you hear |
|---|---|---|
| **Capture** (`mode` Capture; the catalogue's block) | A committed **take**, looped | `capture` on records, off commits; the take plays looped and stretched until the next take replaces it. The catalogue's "spectral looper" |
| **Rolling** (`mode` Rolling; hands-free) | The **live ring**. The head drifts behind the present and **resyncs** to it when the lag reaches `maxlen` | A slow ghost of what you just played, which follows the song in snapshots. At `amount` ×1 it is a pure **smear** of the present |
| **Freeze** (`freeze`, in both modes) | Nowhere: the last magnitudes are held | One instant forever, with new phases on every hop |

Units: H = hop = N / 4 samples, N = window size (§2.4). Positions are f64 samples.

**Capture mode.**
- **`capture` off → on** (sample-accurate, edge-triggered):
  - the **pre-roll ring's last 250 ms are copied aside** (the seam's material), and recording starts into
    the **idle take buffer** (A or B) at t₀;
  - S = seam length = min(250 ms, 1/4 of the take), settled at commit;
  - the playing take, if any, keeps playing.
  - **During a commit crossfade** the idle buffer is the one the fading head still reads. That head then
    **stops reading and holds its last magnitudes** (the freeze mechanism), and its fade completes on the
    held spectrum. So a new take can always start at once, and no third buffer is needed.
  - The value of `capture` at start-up or after `reset` is the baseline: a session saved with `capture` on
    does not start recording when it loads.
- **`capture` on → off**, or **`maxlen` reached** (auto-commit; the next take then needs a new off → on):
  - The take is [t₀, t₁). If it is shorter than 50 ms, it is discarded as a bounce.
  - **Loop seam**: the take's last S samples are crossfaded, equal power, with the S pre-roll samples that
    came before t₀. Reading on from the end of the take into its start then continues exactly as the input
    did. This is the looper's trick: no click, and no step in the window for the analysis to smear into a
    noise burst.
    - Inside the fade, two different moments of the playing are mixed. On a chord or noise the level holds;
      on one sustained pure tone the two can cancel or add (any looper's limit). With a window longer than
      the seam, the frame averages it out.
    - The copy of the seam is S samples in one block (at most 48,000 at 192 kHz): no spike.
  - A **new head** starts at the take's start.
  - It **crossfades** from the old head over `xfade` (§2.3).
  - The two buffers then swap roles.
- **Head motion**:
  - per output hop, p ← p + H / amount, modulo the take length. `amount` < 1 plays faster than real time
    (allowed: a texture, as in Paulstretch).
  - The analysis window centred on p **reads the take circularly**: a take shorter than the window still
    works, as a seamless loop.
- **No take yet**: the wet is silent and only `thru` sounds.
- **While a first take records**: the wet is silent. Later takes record under the playing one.

**Rolling mode.**
- The live ring always records. Its write position is w (now).
- **Minimum lag** L₀ = N / 2: at a hop boundary (time w), the frame centred on p must be fully written. It
  is then read and worked during the hop, and sounds from w + H on. Its centre comes out N / 2 later: N + H
  after it was played.
- Per hop: p ← p + H / amount, then p ← min(p, w − L₀).
  - `amount` < 1 is therefore a **catch-up speed**: after a freeze, the head races towards the present, then
    holds at L₀.
  - `amount` ×1 keeps the head at L₀: the wet is the present, smeared, late by N + H (§2.9).
- **Resync**: when the lag w − p exceeds `maxlen`, a new head starts at w − L₀ and the old one crossfades
  out over `xfade`.
  - There are never more than two heads. The crossfade used is **min(`xfade`, half the time to the next
    resync)**, that is half of `maxlen` · amount / (amount − 1). At ×1,024 and `maxlen` 1 s, a resync comes
    every second and the fade is 0.5 s, whatever `xfade` says.
  - The lag limit is **max(`maxlen`, 2 N)**: with a 5.46 s window and `maxlen` 1 s, the minimum lag alone
    (2.73 s) would otherwise trigger a resync on every hop.
  - The old head goes on reading during the fade, so the lag reaches `maxlen` + the fade + one window. The
    ring is **the whole pool** (120.5 s, §2.10), which covers the worst case (60 + 10 + 5.5 s).
  - Example [A]: ×8 and `maxlen` 10 s. The lag grows by 0.875 s per second, so a resync comes every 11.4 s,
    and each segment stretches 1.43 s of playing into 11.4 s.
  - **Musically: every few seconds the drone takes a new snapshot of what you are playing.** Long `maxlen`
    gives rarer, longer snapshots.
  - When you stop playing, the next snapshot is of silence: the ghost ends within one resync period plus the
    fade. It does not play out what it had not reached yet (Open question 3).
- `capture` is ignored in Rolling (the `state` meter shows Rolling).

**Freeze** (both modes):
- **On**: the head stops, and the **last analysed magnitudes are held**. Each hop resynthesises them with
  new phases.
  - Nothing is re-read, so in Rolling the ring may overwrite the frozen moment: freeze holds a spectrum,
    not a position.
  - In coherent mode (`cloud` < 100 %) the held peaks keep advancing at their own frequencies, as
    gfx-glitch's sustain does.
  - **No click by construction**: the next frame overlaps the last four.
- **Off**:
  - Capture: the head resumes from where it stopped.
  - Rolling: it resumes if the lag is still under `maxlen`; otherwise a resync crossfade starts at once.
- Freeze on during a crossfade: both heads stop, and the fade completes on the held spectra.

**Mode change** (`mode` is a **setup** value, never a scene value): the wet fades out over 50 ms, the
memory pool is re-partitioned (§2.10), takes are discarded, and the new mode starts empty.

**Bypass and trails** (`bypass`, `trails`, the contract's §Trails):
- **Trails on**: on bypass, the memory stops recording and the dry passes. The wet keeps playing what it
  plays (a frozen spectrum stays frozen) and fades out over max(`xfade`, one window).
- On re-engage in Capture, **the take is still there**: it fades back in from the take's start. A drone
  therefore returns with its scene.
- **Trails off**: the wet fades out in 20 ms.

**`state`** (read-only meter, contract shared id): 0 Empty · 1 Capturing · 2 Playing · 3 Frozen · 4 Rolling.
It lets golem light the capture footswitch honestly, for example while an auto-commit fires at `maxlen` and
the host still shows `capture` on.

### 2.2 Analysis

- **Window**: Hann, symmetric (periodic form), analysis = synthesis, **hop N / 4**: gfx-glitch's Smooth
  window.
- Frames are rotated by N / 2, so phases refer to the frame's centre (gfx-glitch, needed by the coherent
  path).
- **Magnitudes only** in random mode. Bins below 20 Hz are zeroed, so the wet has no DC and no thump.
- **Fractional head**: the frame starts at round(p − N / 2). In random mode a sub-sample position is
  meaningless. In coherent mode the frequencies come from a pair of frames exactly H_a samples apart
  (§2.5), so the rounding does not matter there either.
- **Layout**: 2 / 2 first, 1 / 1 second (stereo by nature: golem turns the lane stereo here).
- **Stereo**: each channel is analysed separately. **When L and R are bit-identical over the frame** (a
  mono DI on a 2 / 2 lane), it is analysed once and copied, which halves the analysis cost (gfx-freqshift's
  rule).
  - The test costs nothing per frame: the recorder keeps **the position of the last sample where L ≠ R**
    (per take, and for the ring). A frame is mono when that position is older than its window.

### 2.3 Crossfades between heads (commit, resync, unfreeze-resync)

While a crossfade runs, two heads are analysed and **one** synthesis runs.

- **Random mode**: the frame's magnitudes are mixed in the **power** domain:
  |Y|² = (1 − g) |A|² + g |B|², with g a raised-cosine from 0 to 1 over `xfade`.
  - For uncorrelated signals, this is exactly an equal-power crossfade, with no dip and no bump.
  - It costs one extra analysis, not one extra synthesis.
- **Coherent mode** (`cloud` < 100 %): each head keeps **its own phases** (its own accumulators), and the
  two output spectra are summed as complex numbers, √(1 − g) · A + √g · B, before the one synthesis.
  - The inverse STFT is linear, so this **is** two syntheses crossfaded with equal power on the frame grid,
    for the price of one. The cost is one more set of phase arrays per engine, not an inverse FFT.
  - "Keep the louder bin's phase" is dropped: it breaks the phase locking of every peak region the two
    heads share.
- `xfade` is applied on the frame grid. The real fade is never shorter than one window: the overlap-add
  smooths it.

### 2.4 Window size (`fft`) and the engine

- `fft` is the **window length in ms** (§3.1). It is quantised to the nearest size in log of the grid
  **2^k and 3 · 2^(k−1)**, from 256 to 2^18 samples (21 sizes, half-octave steps).
  - At 48 kHz that is 5.3 ms – 5.46 s.
  - **The top is 2^18 samples at every rate**: at 96 kHz the longest window is 2.73 s, at 192 kHz 1.37 s.
    This caps the memory (§2.10; Open question 8).
- **Size change**: a second engine starts on the same heads. The wet then crossfades, equal power, over
  max(50 ms, the new window), as in gfx-glitch. Both engines run during the change.
  - **Nothing of O(N) happens at the change.** The Hann windows and the four-step twiddles are read with a
    stride from **two master tables** built in `prepare` (2^18 and 3 · 2^16 points), and every FFT plan is
    made there.
  - An engine that has faded out is **cleared in units** on the same schedule (6 N_max values: a `fill` in
    one piece would be the spike we are avoiding). A second size change waits until that is done.
- **Schedule** (gfx-glitch's, generalised): each frame's work is cut into **units**, each of at most about
  4,096 element operations. The units are spread evenly over the hop **on sample counts**, never on host
  blocks: unit i runs at offset ⌊(i + 1) · H / (U + 1)⌋. So the output is identical for any block size.
  - **Every pass over the frame is resumable**, not only the FFT: reading the memory (f16 decoding) and
    windowing, the magnitudes, each module and each of the blur's four passes, the peak regions, the
    phasors, the windowing and the overlap-add.
  - This is the lesson of gfx-glitch's profile [A, measured at 32,768, stereo]: its FFT step takes 85 µs,
    its frequency-map and density steps 590 µs each. **The one-piece bin passes are its spike, not the FFT.**
  - At the smallest sizes U exceeds H in the heaviest case (two heads, coherent, a size change): units that
    fall on the same sample run one after the other.
- **Four-step FFT** (new in gfx-dsp `stft`; Bailey 1990 [I]):
  - A real FFT of N is a complex FFT of N / 2 plus an O(N) post-twiddle (realfft's packing).
  - The complex N / 2 = N₁ × N₂ is done as N₂ column FFTs of N₁, a twiddle pass, then N₁ row FFTs of N₂,
    with N₁, N₂ ≤ 1,024.
  - Each sub-FFT, each twiddle chunk and each post-twiddle chunk is one schedulable unit.
  - **Used above N = 16,384 only.** Up to there a one-piece realfft is one unit (13 µs at 4,096, 45 µs at
    16,384 [A, measured, rustfft 6.4 with AVX2, an i9-13900H]).
  - Measured in one piece above that [A]: 60 – 90 µs at 32,768, 200 µs at 2^17, **435 µs at 2^18**. That is a
    third of a 64-sample block at 48 kHz, and more than a whole one at 192 kHz.
  - Prototype of the four-step [A, measured: sub-FFTs by rustfft, columns gathered and scattered one at a
    time]: the complex 2^17 takes 544 units of **7 – 17 µs** each. It costs **3.4 times** a one-piece FFT in
    total (0.96 ms against 0.29 ms), which the estimate of §2.11 counts. Transposing first (the six-step
    form) should win part of it back: to try, not required.
  - At N = 2^18 per channel and per frame, in random mode: about **1,600 units** (two four-steps of 544,
    64 units each for the read and for the overlap-add, 32 per bin pass). In stereo that is one unit every
    20 samples, so **a 64-sample block holds three or four units: 30 – 60 µs, 2 – 5 % of its duration**.
    Coherent mode with a head crossfade triples the analysis: still under 15 %.
  - Sizes 3 · 2^(k−1) use N₁ = 3 · 2^j. rustfft plans those mixed radices in `prepare` (384 × 256 and
    768 × 256 measured: same unit cost).
  - **Exactness**: it is an FFT, not an approximation. Measured in f32 [A]: 1.5e-7 relative RMS against an
    f64 FFT at every size (a one-piece f32 FFT: 1.4e-7), 1.8e-7 between the two. Bench: within 1e-6.
- **Random phasors**: a seeded xorshift picks entries from a 4,096-entry table of unit phasors (12-bit
  phase), so there is no sin / cos per bin. Paulstretch's own resolution is 2^15 steps; 2^12 is far below
  any audible phase quantisation on a random phase [A].

### 2.5 Phases: `cloud` and `width`

- **`cloud` 100 %** (the default): Paulstretch. Every bin, every hop, a fresh uniform phase.
- **`cloud` 0 %**: **coherent stretch**, a peak-locked phase vocoder.
  - The instantaneous frequency comes from a **second analysis** at p − H_a, with H_a = H / 4. The phase
    difference of the two frames gives each peak's frequency exactly, with no ambiguity at any `amount`.
  - Each peak's phase advances by ω · H per output hop. Its region's bins keep their measured phase
    relative to the peak (`pv_shift.rs` and gfx-glitch's `regions`).
  - At `amount` exactly ×1, with pitch, fshift and spread at 0, the analysis phases are used as they are:
    **identity**, so the wet equals the input delayed (bench §5; the f16 pool limits the null, §2.10).
    The accumulators are **re-seeded from the analysis phases on every such frame**, so leaving ×1 starts
    from the true phases, with no jump.
  - This end is for ×1.5 – ×4 on keys and voice (the Texture-warp ground). At ×100 it rings metallic; that
    is what `cloud` is for.
- **In between**: coherent phase + a fresh offset, uniform in ±π · cloud, per bin and per hop. The offset
  does not accumulate (gfx-glitch's `cloud`).
- **Level across `cloud`** [A]:
  - for hop N / 4 and Hann / Hann, the overlap-add sums Σ w_s² = 1.5 for uncorrelated frames and
    (Σ w_a w_s)² = 2.25 for coherent ones;
  - a bin whose phase gets an offset uniform in ±π · cloud keeps a coherent part of amplitude
    s = sin(π · cloud) / (π · cloud), so of **power s²**; the rest, 1 − s², is incoherent;
  - the output gain is therefore g = 1 / √(0.5625 + s² · (2.25 − 0.5625)) (§2.6).
  - Checked by simulation at ×1 [A]: within 0.03 dB of the measured level at `cloud` 0, 10, 25, 50, 75 and
    100 %, on noise and on a chord. (With s in place of s², the draft's law, the wet was 1.2 dB too quiet at
    50 % and 1.75 dB at 75 %.)
  - Bench: level within ±0.5 dB across `cloud` at ×1; ±1 dB at other amounts, where a phase vocoder's frames
    do not add up fully coherently.
- **`width`** (0 – 100 %, default 100 %), gfx-glitch's id:
  - the right channel's random phase is φ_R = φ_L + width · δ, with δ uniform in ±π per bin;
  - 0 % gives identical phases: a mono DI gives L = R exactly, and a stereo source keeps its image;
  - 100 % gives independent channels: the wide wash Paulstretch is loved for, from a mono DI.
  - In coherent mode, `width` only scales the `cloud` offsets.

### 2.6 Loudness

- **Calibration** [A, Parseval]:
  - a random-phase frame has the energy of the windowed analysis frame, spread evenly over N samples, so its
    power is P · mean(w_a²) = 0.375 P, where P is the power of the region read;
  - the synthesis overlap-add of power-complementary Hann windows multiplies that by Σ w_s² = 1.5;
  - the fixed gain 1 / √0.5625 = 1.333 (at `cloud` 100 %) therefore makes **the wet's expected power
    equal the region's power**: the same for a sine, a chord or noise, at any window and any `amount`.
- **Spectral modules preserve power**: pitch, fshift and spread work on |X|², with the remap normalised
  (§2.7). PaulXStretch's amplitude spread loses level as it widens; ours does not.
- **`vol`** (−24 – +12 dB, default −3 dB as the catalogue's) is applied after that, on the wet only.
- **Soft limiter** on the wet: tanh knee from −6 dBFS, ceiling −1 dBFS (gfx-glitch's). Random-phase sums
  are near-Gaussian, with peaks 13 – 15 dB over the RMS on a chord or noise and 9 – 10 dB on one sine [A,
  simulated], so a loud chord at `compress` 100 % needs it.
- **Level against PaulXStretch** [A, both simulated]: ours comes out at the region's level (0.0 dB on noise,
  −0.1 dB on a chord), Paulstretch's algorithm 0.8 dB under it. With `spread` at 0 a converted rig is thus
  **0.8 dB louder**, and golem's `vol` values are kept as they are. With `spread` up, PaulXStretch loses
  level and we do not. The bench records the offset on the real plugin (§5).

### 2.7 Spectral modules (per frame, after the analysis)

Their order follows PaulXStretch, so converted settings mean the same: **fshift → pitch → spread →
compress**. So f′ = (f + fshift) · 2^(pitch / 12).

| Module | Ours | PaulXStretch |
|---|---|---|
| `fshift` (±1,000 Hz) | Fractional bin shift, linear interpolation of \|X\|²; what leaves 20 Hz – Nyquist is dropped | Integer bins: 1.46 Hz steps at B = 16,384, 46.9 Hz at B = 512 |
| `pitch` (±24 st) | \|X\|² remapped by r = 2^(st / 12). Down: accumulated; up: interpolated and scaled by 1 / r, so the power is kept. In coherent mode, **peak regions move as rigid units** (gfx-glitch's `map_freq`) | Nearest bin |
| `spread` (0 – 2,400 ¢) | A **constant-Q blur** of \|X\|² with standard deviation σ in cents: a forward and backward one-pole, twice, along the linear bin axis, whose pole varies per bin: a_k = exp(−1 / τ_k), **τ_k = k · ln 2 · σ / 2,400** bins. Each pass is in **scatter form** (below). It is O(N) with no resampling, and the energy is kept | Log resampling (too fine at the bottom, decimated without filtering at the top), then the same recursive smoother, on amplitudes |
| `compress` (0 – 100 %) | **One gain per frame**, as PaulXStretch: g = (L_ref / L)^c, where L = the RMS level of the region the frame reads (its power over the window's), **L_ref = −21 dBFS**, and c = `compress` / 100. The output level then moves by (1 − c) dB per input dB. The lift is **+24 dB at most**, and it **falls linearly in dB to 0 between −60 and −72 dBFS**: no noise pumped up in a gap, and no step between two frames either side of a threshold. Applied per frame: the 4× overlap is its smoothing | Same law. Its reference is tied to its unnormalised FFT scale, so it moves with the window: −21.1 dBFS at golem's default (§1.1), which is where ours is fixed. Its lift reaches +60 dB |

- **The blur's scatter form** [A, prototyped]. Per pass, with a running state s: s ← s + P_k, then
  P_k ← (1 − a_k) · s and s ← a_k · s. Each bin's energy is handed on, never copied, so the sum is kept
  exactly (what is lost is what leaves the band at Nyquist).
  - Measured on one partial at 110 Hz, 440 Hz, 1 kHz and 3 kHz: σ within 1 % of the setting from 50 to
    1,200 ¢, energy within 0.05 dB, centroid within 6 ¢. At 2,400 ¢ the top of the band cuts the blur short
    (σ 0.82 – 0.97 of the setting).
  - The usual form, y_k = a_k · y_(k−1) + (1 − a_k) · x_k with a varying pole, does **not** keep the energy:
    +1.6 dB at 1,200 ¢, +7.8 dB at 2,400 ¢, and the centroid moves up by more than an octave. And the pole
    2^(σ / 2,400) − 1 of the draft is 19 % too wide at 1,200 ¢.
  - Under one bin of width (τ_k < 1: low partials, small σ, short windows) the blur can do nothing: the
    window's own lobe is wider.
- `spread_on` and `comp_on` (catalogue switches): off forces spread or compress to 0. They are crossfaded,
  since they act per frame.
- **No `onset` in v1.** Paulstretch's time credit plays an attack less stretched, but still through random
  phases and a 683 ms window: nothing crisp comes out. PaulXStretch ships it at 0 and golem never exposed
  it. In Rolling it would also fight the lag rule. An id is permanent once shipped, so it waits for a proof
  by ear. gfx-grains brings gfx-dsp an `onset` detector; its first use here would be a **resync on the
  attack** (v2, Open question 3).
- **Wet filters**: `hpf` (20 – 500 Hz, default 20 Hz = off) and `lpf` (1 – 20 kHz, default 20 kHz = off),
  4th-order TPT, per sample (gfx-dsp `svf`). Drones on a guitar lane want `hpf` 80 – 150 Hz; presets set it.

### 2.8 gfx-stretch and gfx-glitch: who does what

| | gfx-glitch | gfx-stretch |
|---|---|---|
| Time | The present (late by N + H) | Seconds to minutes in the past, slowed down (Rolling at ×1: the present too, late by N + H) |
| Sustain | A decaying max-hold of the input's partials (T60) | The input itself, read slower. Freeze holds a moment |
| Memory | One window | Up to 60 s (takes or ring) |
| Gesture | None: set it and play | Capture / Freeze (the catalogue's) **or** Rolling (none) |
| Spectral palette | 29 bin effects, density, LFO, envelope | Paulstretch's four modules: pitch, shift, spread, compress |
| Shared (gfx-dsp `stft`) | Schedule, windows and their normalisation, frame rotation, seeded randoms with `width`, `regions` / `map_freq`, size crossfade, and now the four-step FFT | Same |

A pad *from what you are playing now* is gfx-glitch's job (Sustain, Ring Hold). A pad *from what you played
a moment ago, slowed down* is this pedal's.

### 2.9 Latency

- **Reported 0.** The dry (`thru`) is never delayed. No `align`: a stretched wet is decorrelated by nature,
  so there is no comb to fix.
  - One corner is not decorrelated: **Rolling at ×1 with `cloud` near 0** is the input itself, late by
    N + H, next to the dry. That is an echo (a comb under about 20 ms of window). It is what the setting
    asks for; `thru` off, or `cloud` up, removes it.
- golem-rack accepted a late wet with a reported latency of 0 for gfx-glitch (2026-11-13); the same holds
  here.
- **The wet is late by design**:
  - Rolling at ×1: N + H (gfx-glitch's figure). At 48 kHz: 107 ms at 85 ms, 213 ms at 171 ms, 853 ms at the
    683 ms default;
  - Rolling above ×1: that, plus the growing lag;
  - Capture: a commit sounds after **one hop** (the first frame) and reaches full level after `xfade`. The
    first take fades in the same way (to judge by ear: a fade of one window may suit a looper gesture
    better);
  - at the longest window (5.46 s) all these figures are in seconds: 6.8 s in Rolling at ×1.
- The bench's latency check finds the dry at sample 0.

### 2.10 Memory

Allocated in `prepare` for the worst case and **written once there** (as gfx-glitch), so no page is first
touched in `process`.

- **Pool** per channel: 2 × (60 s + 0.25 s) at the host rate, stored as **f16**.
  - Half precision keeps a relative error under 2^−11 per sample. Measured [A]: the error sits **73.5 dB
    under the signal** (RMS; a sine, noise, a decaying note alike), at every level down to −84 dBFS, and
    62 dB under it at −90 dBFS (subnormals). It follows the signal, so there is no noise floor in a gap.
    That is far under the random-phase texture's own noise, and halves the memory.
  - Conversion by the `half` crate, which uses F16C where the CPU has it (found at run time). The pool type
    is generic over its sample, so the tests also run it in f32.
  - Non-finite inputs are replaced by 0 before they are stored.
  - **Capture**: two take buffers, A and B, each 60.25 s.
  - **Rolling**: one ring over the whole pool, 120.5 s (the lag can reach `maxlen` + a fade + a window,
    §2.1).
  - Buffers carry their **valid lengths**: nothing in `process` ever clears a pool (a mode change or a
    `reset` only moves counters).
- **Engines**: 2 engines × 2 channels × about 6 N_max f32 (frame, spectrum, magnitudes, held magnitudes,
  overlap-add accumulator, coherent phases and second frame), N_max = 2^18.
- **Tables**: the two master Hann tables and the two twiddle tables (§2.4), and the fading head's own
  phase arrays (§2.3): about 3 MB.
- **Pre-roll ring**: 250 ms, and its copy for the seam.

| Rate | Pool (stereo) | Engines and tables | Total [A] | First write in `prepare` [A, measured on an i9-13900H] |
|---|---|---|---|---|
| 44.1 / 48 kHz | 21 / 23 MB | 28 MB | ≈ 51 MB | ≈ 7 ms |
| 96 kHz | 46 MB | 28 MB | ≈ 74 MB | ≈ 20 ms |
| 192 kHz | 93 MB | 28 MB | ≈ 121 MB | ≈ 40 ms |

- The first write is slower than a copy (the kernel hands the pages over one by one): 40 ms at 192 kHz, not
  12. It happens in `prepare`, never in `process`. golem-rack's stall check decides whether that is short
  enough at switch-on (Review, open points).
- This is the largest pedal of the suite, and it is used once per rig. PaulXStretch holds 120 s in f32 plus
  its prebuffer: 46 MB at 48 kHz before its FFT buffers. **Accepted as it is** (Open questions 6 – 8).

### 2.11 CPU estimate [A]

Per stereo frame at 48 kHz, default settings (683 ms window, N = 32,768, H = 8,192, random mode):

| Part | Cost |
|---|---|
| Analysis FFT + magnitudes (per hop per channel ≈ 200 + 35 µs; the FFT is a four-step at this size, 3.4 × the 60 µs of a one-piece one) | ≈ 60 ns |
| fshift, pitch, spread (2 × 2 recursive passes), compress | ≈ 25 ns |
| Random phasors, inverse FFT (four-step), window and overlap-add | ≈ 65 ns |
| Per sample: f16 write and read, wet filters, `vol`, limiter, crossfades | ≈ 35 ns |

- Total **≈ 185 ns per stereo frame, about 0.9 % of a core**. The FFT figures are measured (§2.4), the
  others estimated.
- Variations:
  - coherent mode: +50 % (the second analysis, the regions);
  - a head crossfade: +30 %;
  - a size change: ×2 for its duration;
  - N = 2^18: +20 % (log N);
  - N ≤ 16,384: −40 % (one-piece FFTs).
- **Budget for the bench**: mean ≤ 400 ns per stereo frame in every case, and **worst 64-sample block ≤ 25 %
  of its duration at every size**, 2^18 included.

### 2.12 Safety and determinism

- A non-finite frame resets its channel's spectral state. A non-finite output resets the engines and fades
  the wet back in over 10 ms (gfx-glitch).
- Seeded randoms; `reset` reseeds and clears the takes and the ring. Two renders are identical bit for bit.
- No allocation, lock, thread or I/O in `process`, and no pass of O(N) or O(pool) in one piece (§2.4,
  §2.10). `capture` is read on the audio thread, sample-accurate within nih-plug's parameter events.

## 3. Parameters

### 3.1 Ids and units: decision

- **The thirteen catalogue ids are kept**, with their display names (golem joins on `name`: descriptor
  rules).
- **Kept as they are** (already real units, linear): `amount` ×, `pitch` st, `fshift` Hz, `vol` dB,
  `maxlen` s, and the bools `capture`, `freeze`, `thru`, `spread_on`, `comp_on`.
- **`compress` keeps 0 – 100 %**. PaulXStretch's law is already linear in a meaning: the exponent p is "the
  fraction of each dB of level change removed". That is the gfx-ringmod case of the 2026-11-14 rule. Only its
  reference level changes (§2.7).
- **`spread` switches to cents** (σ of the blur, 0 – 2,400 ¢). The law is the case the rule was written for:
  half the knob does nothing, and the meaning depends on the rate. The top of the range is useful up to
  about 1,200 ¢; above, the band's edges cut the blur (§2.7).
- **`fft` switches to ms** (window length, 5 – 5,500 ms). PaulXStretch's law is exponential and in samples,
  so a stored value means a different window at 96 kHz. The display name stays "FFT size" (the join key),
  with the unit ms.
- **`maxlen`: 1 – 60 s**, the catalogue's range, not the plugin's 120 s. It halves the memory (§2.10); a
  stored value above 60 is clamped.
- **`amount`: 0.1 – 1,024 ×, linear**, the plugin's full range (the catalogue's knob stops at 100, and may
  stay there: only presets above ×100, such as `glacier`, need its maximum raised).
  - Fine resolution at the bottom is golem's UI's business: half the knob's travel for ×0.1 – ×20 is
    advised.
  - **EXP is mapped to a sub-range** (heel ×1, toe ×20): over 1,024 ×, 7 bits are steps of ×8.
- **New ids** (golem-fx's own): `mode`, `cloud`, `width`, `xfade`, `hpf`, `lpf`, `trails`, `bypass`,
  `state` (read-only). `cloud`, `width`, `hpf`, `lpf`, `trails` and `bypass` are the suite's shared or
  gfx-glitch's ids, with the same meaning. `state` is a read-only `FloatParam`, as gfx-gate's (the
  contract's meters).
- **`freeze` is a bool here**, the catalogue's. gfx-reverb and gfx-shimmer have a `freeze` enum (Off ·
  Freeze · Infinite) for their own blocks; gfx-grains a bool. The id is not in the contract's shared table,
  and each block's catalogue decides.
- **`spread_on` and `comp_on` are kept** although `spread` 0 and `compress` 0 already mean off: they are
  catalogue ids, golem writes them (on) at compile time, and saved chains carry them.

### 3.2 Front panel

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `amount` | Stretch amount | f | 0.1 – 1,024 × | 4 × | per hop (head speed) | ✓ (sub-range) | How much slower the sound plays: ×1 = real time (a smear in Rolling), ×4 = a pad, ×50 + = a drone |
| `fft` | FFT size | f | 5 – 5,500 ms | 683 ms | size crossfade | | Window length: short = tight, grainy, rhythmic; long = smooth, every attack melted. Quantised to half-octave sizes, ≤ 2^18 samples |
| `spread` | Frequency spread | f | 0 – 2,400 ¢ | 0 ¢ | per frame | ✓ | Blurs each partial over ± this many cents: 20 – 60 ¢ chorus-like, 100 – 300 ¢ a wash, 1,200 ¢ + noise bands |
| `vol` | Main volume | f | −24 – +12 dB | −3 dB | EXP | ✓ | Level of the stretched sound (the dry is untouched) |
| `mode` | Mode | e | Capture · Rolling | Rolling | 50 ms fade, setup only | | Capture = takes on the footswitch (the catalogue's gesture); Rolling = hands-free, the head follows the playing (§2.1) |
| `capture` | Capture | b | off / on | off | edge | | Capture mode: on records (up to `maxlen`), off commits and plays the take looped and stretched |
| `freeze` | Freeze | b | off / on | off | frame | | Holds the current instant forever (both modes) |
| `thru` | Pass input through | b | off / on | on | 20 ms | | The dry at unity, never delayed. Off = the stretched sound only |

**Default mode.** Rolling, so the bare pedal does something with no footswitch: The maintainer's rule since gfx-glitch.
golem's `stretch` block **pins `mode` Capture** (contract: per-block values are golem's pins), so its foot
gesture keeps working (§3.4; Open question 1).
- The pin must also reach **saved rig chains**, which have no `mode`: the block's entry is `defaults:
  compile`, so golem writes its catalogue defaults when a chain omits them. `mode` Capture has to be one of
  them (a hidden catalogue parameter or an `init` value: golem-rack's choice).
- This is not a mode-dependent default: no parameter's default depends on `mode`.

### 3.3 Expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `pitch` | Pitch shift | f | −24 – +24 st | 0 st | per frame | ✓ | Moves the stretched spectrum by semitones: octave drones, sub layers |
| `fshift` | Frequency shift | f | −1,000 – +1,000 Hz | 0 Hz | per frame | ✓ | Moves every partial by the same Hz: inharmonic, bell-like (applied before `pitch`, as PaulXStretch) |
| `compress` | Compress | f | 0 – 100 % | 0 % | per frame | ✓ | Evens the frame level: 50 % halves the level swings, 100 % keeps the drone at a constant level through gaps |
| `maxlen` | Max capture length | f | 1 – 60 s | 10 s | next take / at once (Rolling) | | Capture: the longest take (auto-commit). Rolling: the longest lag before a resync |
| `cloud` | Cloud | f | 0 – 100 % | 100 % | per frame | ✓ | 100 % = random phases (the Paulstretch wash); 0 % = coherent slow-motion (clean at ×1.5 – ×4, metallic at ×100) |
| `width` | Width | f | 0 – 100 % | 100 % | per frame | | How independent the two channels' phases are: 0 = mono-identical, 100 = a wide wash from a mono DI |
| `xfade` | Crossfade | f | 50 – 10,000 ms | 2,000 ms | — | | Length of the crossfades on commit, resync, unfreeze-resync and the trails fade (a resync never takes more than half its period, §2.1) |
| `hpf` | Low Cut | f | 20 – 500 Hz | 20 Hz (off) | 30 ms | | High-pass on the wet: keeps a drone from muddying the lane |
| `lpf` | Tone | f | 1,000 – 20,000 Hz | 20,000 Hz (off) | 30 ms | ✓ | Low-pass on the wet: darker, further away |
| `spread_on` | Enable spread | b | off / on | on | frame | | The catalogue's module switch: off forces spread to 0 |
| `comp_on` | Enable compressor | b | off / on | on | frame | | The catalogue's module switch: off forces compress to 0 |
| `trails` | Trails | b | off / on | on | — | | On bypass the wet keeps playing and fades over `xfade` |
| `bypass` | Bypass | b | off / on | off | §2.1 | | The VST3 bypass (nih-plug `make_bypass`), the id every coded effect ships |
| `state` | State | meter (read-only) | 0 – 4 | — | — | | Empty · Capturing · Playing · Frozen · Rolling |

- No mode-dependent defaults. Spectral parameters are **latched per frame**, so they move at the hop rate
  (171 ms at the default window) and the 4× overlap smooths them. An EXP sweep on `pitch` is therefore as
  slow as the window: a property of the method, stated for golem's UI.
- The contract's "control-rate sidebands" check applies to `vol` and `lpf`, which act per sample.

### 3.4 Conversion from the `stretch` catalogue (golem engine table)

Values as golem stores them (real units through `map.mode pow`). n = the stored % / 100.

| id | Formula | Catalogue default → |
|---|---|---|
| `capture`, `freeze`, `thru`, `spread_on`, `comp_on`, `amount`, `pitch`, `fshift`, `compress`, `vol` | Unchanged | Unchanged |
| `maxlen` | min(60, s) | 10 s |
| `spread` | ¢ = **0.59** · 2,400 · log₂(48,000 / 40) / (16,384 · (−ln(1 − 2^(−10 n²)))), 0 when n = 0. The factor: PaulXStretch blurs amplitudes, so a partial's power profile is its kernel squared (§1.1); ours blurs the power itself | 0 % → 0 ¢ |
| `fft` | ms = 2,000 · B / 48,000, B = the nearest 2^a 3^b 5^c 7^d 11^e to ⌊2^(7 + 10 n)⌋ | 70 % → 682.7 ms |
| `mode` | **pinned Capture** at insert of the `stretch` block | — |

The formulas are written at 48 kHz, golem's rate; they are what PaulXStretch did there.

| `spread` % | 30 | 50 | 60 | 70 | 80 | 85 | 90 | 95 | 100 |
|---|---|---|---|---|---|---|---|---|---|
| PaulXStretch's amplitude σ, ¢ [A] | 2 | 8 | 17 | 44 | 126 | 223 | 410 | 780 | 1,534 |
| **Ours, ¢** [A] | 1 | 5 | 10 | 26 | 74 | 132 | 243 | 461 | 907 |

| `fft` % | 0 | 10 | 20 | 30 | 40 | 50 | 60 | 70 | 80 | 90 | 100 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| ms [A] | 5.3 | 10.7 | 21.3 | 42.7 | 85.3 | 170.7 | 341.3 | 682.7 | 1,365 | 2,731 | 5,461 |

- No block presets exist for `stretch` (`library/block-presets/` has none), so only saved rig chains are
  converted.
- `compress` is unchanged, and means the same at the default window (§2.7). A chain with another `fft`
  and `compress` up changes level by 3 dB per octave of window: accepted (new sounds).
- The catalogue entry changes: `spread` in ¢, `fft` in ms, `maxlen` max 60. This needs golem-rack's
  agreement, as for gfx-crusher and gfx-freqshift.

## 4. Applications

| Use | Settings to start from | Feature that matters |
|---|---|---|
| **Intro pad from one chord** (guitar, keys) | Capture; stomp on, strum, stomp off. `amount` ×8, `fft` 683 ms, `thru` on, `vol` −6 dB, `hpf` 100 Hz | Seamless loop seam (pre-roll); commit crossfade |
| **Drone under a verse** | Capture one low note (2 s); `amount` ×60, `spread` 120 ¢, `compress` 60 %, `hpf` 80 Hz, `lpf` 6 kHz, `vol` −10 dB | `compress` keeps it level through the take's decay; spread widens the partials |
| **Infinite hold** (the hold-machine gesture) | Capture a chord; then `freeze` on; play over it. `xfade` 4 s | Freeze holds a spectrum, with no click |
| **Voice stretched into the outro** | Capture the last sung line; `amount` ×20, `fft` 341 ms, `width` 100 %, `lpf` 9 kHz | Long takes (up to 60 s); the voice keeps its vowels at medium windows |
| **Hands-free ambient shadow** (no footswitch) | Rolling; `amount` ×6, `maxlen` 12 s, `fft` 683 ms, `vol` −10 dB, `hpf` 120 Hz | Resync crossfades: a slow ghost that follows the song |
| **Live spectral smear** | Rolling; `amount` ×1, `fft` 171 ms, `cloud` 100 %, `vol` −6 dB | The present smeared, late by N + H = 213 ms: a diffuse, reverb-like halo with no tail model |
| **Slow-motion keys** | Capture an arpeggio; `amount` ×2.5, `cloud` 15 %, `fft` 85 ms | The coherent end: clear notes, no wash |
| **Octave-up glass drone** | Capture; `amount` ×40, `pitch` +12, `spread` 40 ¢, `lpf` 10 kHz, `vol` −12 dB | Power-preserving remap (and rigid peak regions once `cloud` comes down) |
| **Sub bed for bass** | Capture a bass note; `amount` ×30, `pitch` −12, `lpf` 1.5 kHz, `width` 0 % | `width` 0 keeps the low end mono |
| **Bells from a chord** | Capture; `amount` ×12, `fshift` +70 Hz, `spread` 0 | Fractional-bin frequency shift |
| **Noise-band wash** | Rolling or Capture; `amount` ×100, `spread` 1,200 ¢, `compress` 80 % | A constant-Q blur, energy kept |
| **Tight grain** (rhythmic) | Capture a riff; `amount` ×3, `fft` 21 ms, `cloud` 100 % | Short windows keep a rhythmic flutter (PaulXStretch's low FFT) |
| **Scene swell** | Capture beforehand; bypass with `trails`; re-engage at the chorus with `xfade` 6 s | The take survives bypass; trails fade |

**Preset ideas** (ids for `presets/gfx-stretch.json`, no artist names): `chord-pad`, `verse-drone`,
`infinite-hold`, `vocal-outro`, `ambient-shadow`, `live-smear`, `slow-keys`, `glass-octave`, `sub-bed`,
`chord-bells`, `noise-wash`, `tight-grain`, `scene-swell`, `glacier` (×500, `spread` 300 ¢, `compress` 70 %),
`ghost-voice` (Rolling ×3, `lpf` 5 kHz, `vol` −14 dB). Presets of the `stretch` block's lineage set
`mode` Capture.

## 5. Bench suite (on top of gfx-bench's common checks)

Every check runs at 44.1, 48, 96 and 192 kHz. Targets are computed in the test.

| Check | Pass |
|---|---|
| Four-step FFT | Every size above 16,384, forward and inverse: within 1e-6 (relative RMS) of a direct realfft (prototype: 1.8e-7). Median unit ≤ 25 µs (prototype: 7 – 17 µs) |
| Identity | `cloud` 0, `amount` ×1, modules neutral, Rolling: wet = input delayed by N + H, within −90 dB with the f32 pool (test build) and **−66 dB with the f16 pool** (shipped; 73.5 dB expected), at every size |
| Level | White noise, a chord; `cloud` 100 / 75 / 50 / 25 / 0 %; `amount` ×1, ×8, ×100; every size: long-term wet RMS within ±0.5 dB of the region's (±1 dB at `cloud` < 100 % and `amount` ≠ 1). One sine: ±0.5 dB over at least 2,000 windows (its level fades by nature) |
| Stretch accuracy | Tone pips 100 ms apart; `amount` ×10 (Capture): pip centroids 1 s ± 2 % apart; ×0.5: 50 ms ± 5 % |
| Texture | Against a Paulstretch reference built in the test (Hamming, hop B, its crossfade), same input, ×8, `cloud` 100 %. Noise: no line at the frame rate or its multiples in the wet's power envelope (the reference shows one at twice its frame rate, 20 – 30 dB over the floor). One sine: 90 % power bandwidth within ±15 % of the reference's, envelope correlation time within ±25 %. The fading itself is not asked to be lower: it is not (§1.3) |
| Capture | Take of 2.000 s: loop period = 2 s × `amount` ± 1 hop. A sustained chord and noise captured at an arbitrary length: the wet's level at the seam within ±1 dB, and no broadband burst (the frames over the seam within 3 dB of their neighbours above 5 kHz). One sine: reported, not gated (§2.1). Auto-commit at `maxlen` ± 1 sample. A take under 50 ms is ignored. `capture` on during a commit crossfade: recording starts at that sample, and the fading head's level follows its fade within ±1.5 dB. `capture` on at load: no recording |
| Commit / resync | Noise and a chord, `cloud` 100 % and 0 %: no level dip below −1.5 dB or bump above +1.5 dB during any crossfade. Rolling ×8, `maxlen` 5 s: resync when the lag reaches 5 s ± 1 hop. Rolling ×1,024, `maxlen` 1 s, `xfade` 10 s: never more than two heads, fades of 0.5 s |
| Freeze | After freeze on: long-term spectrum stationary (1/3-octave bands within ±0.5 dB over 60 s); level constant; no step at on or off (common click check) |
| Rolling ×1 | Wet late by N + H ± 1 sample (envelope cross-correlation on a pip) |
| Modules | `pitch` +12 on 440 Hz → 880 Hz ± 1 bin; `fshift` +100 Hz → 540 Hz ± 0.2 Hz (fractional); `spread` σ measured on a 1 kHz sine within ±10 % of the setting (50, 200, 1,200 ¢), energy within ±0.5 dB, centroid within ±10 ¢; `compress` c: a −40 → −10 dBFS input step moves the output by (1 − c) · 30 dB ± 1 dB; at 100 % a −21 dBFS input keeps its level; no lift under −72 dBFS, and no step over 2 dB between two frames 1 dB apart in level |
| Stereo | `width` 0, L = R in: L = R bit-exact. `width` 100: inter-channel coherence < 0.2 above 200 Hz. Mono-DI detection: CPU of L = R input ≤ 60 % of independent L / R |
| CPU | Mean ≤ 400 ns per stereo frame (48 kHz) in random, coherent, crossfade and size-change cases; **worst 64-sample block ≤ 25 %** at every size including 2^18, measured **through a size change, a mode change, a commit and a resync** (where the one-piece work hides), at 48 and 192 kHz |
| Memory and start | `prepare` ≤ 15 ms at 48 kHz and ≤ 60 ms at 192 kHz (first write of 51 and 121 MB); no page fault in `process` (counted by `getrusage` in the test) |
| Conversion | §3.4 tables reproduced. PaulXStretch renders (golem-rack, REAPER) and ours on the same takes and converted settings: 1/3-octave long-term spectra within ±2 dB; level offset measured and recorded (expected +0.8 dB with `spread` 0). `spread` 70, 85 and 100 %: the −10 dB width of a 1 kHz partial within ±20 % of PaulXStretch's |
| Latency / determinism / block size | Reported 0, dry at sample 0; strict null; block sizes 1 – 4,096 identical to −240 dB (sample-count schedule) |
| Listening | `gfx-bench apps gfx-stretch` (one render per Applications row) on clean, lead, bass and voice DIs. A blind A / B against PaulXStretch on the intro pad, the drone and the vocal outro |

gfx-bench options:
- `stereo_by_nature` (2 / 2 first, decorrelated channels).
- Silence floor: the default holds at the default settings (Rolling ×4: 3 s after a burst the head reads
  silence). Freeze and Capture are tested by the suite above, not by the common floor.
- Click bound: −50 dB; −40 dB if the random-phase texture's own high band trips the detector (gfx-glitch's
  precedent), to be declared only if measured.

## 6. Reuse from gfx-dsp, and what is new

| Reused | Moved into gfx-dsp `stft` (from gfx-glitch) | New |
|---|---|---|
| `svf` (wet filters), `smooth` / `exp_smooth` (`vol`, `lpf`), gfx-glitch's soft limiter, `pv_shift.rs`'s peak locking | Sample-count **schedule** (now of variable-size units); Hann pair and its **amplitude and power** normalisations; frame rotation; seeded `Rand` with `width`; `regions` and `map_freq`; the dual-engine **size crossfade** | **Four-step FFT** (resumable units, sizes 2^k and 3 · 2^(k−1) up to 2^18); random phasor table; constant-Q recursive blur (`spread`), also usable by gfx-glitch's Blur |

- In the gfx-stretch crate: heads, takes, rings and seams; the power-domain head crossfade; the frame
  compressor; the `cloud` gain law.
- **gfx-glitch is not migrated in this crate's work.** gfx-dsp `stft` is written from gfx-glitch's code,
  and gfx-glitch keeps its own copy until gfx-stretch has passed its bench.
- It then moves onto the shared module **behind a bit-exact null** (identical output at every size and
  window), which requires that it **keeps its one-piece FFT**: a four-step FFT rounds differently (1.8e-7,
  about −135 dB), and its peak picking can turn that into audible differences on a shipped, preset-bearing
  pedal.
- **The four-step FFT would not cure its worst block anyway** [A, measured here on gfx-glitch as shipped,
  everything on, 64-sample blocks, an i9-13900H: 49 – 56 % at 32,768]. Per step at that size: FFT 85 µs,
  frequency map 590 µs, density 586 µs, sustain and slots 200 – 300 µs each. Its README blames "one FFT
  step"; the spike is the bin passes done in one piece. Cutting those into resumable units (same
  arithmetic, same order, so the bit-exact null still holds) is what removes it: gfx-glitch's own item.

## Visual

- **Category `reverb`** (the catalogue's), anodized (the family's `auto` finish).
- **Crate hue: periwinkle dusk `#a0a8ff`** (`METAL_CRATE_HUE['gfx-stretch']`).
  - Reverb violet (`#6a1fd1`) and shimmer violet (`#8a3ff0`) already wear the family's colour. A third violet
    could not be told apart at tile size, and the catalogue's own `#a86cf0` is only ΔE 27 from shimmer.
  - Periwinkle reads as slow, cold, glacial time. Its ΔE*ab [A, CIELAB D65, ΔE76] to the nearest hues in use or
    proposed: widener ice `#c0e0f8` 40, eq blue 45, gapmeter 47, dynamics slate 49; every other hue is farther.
  - HSL saturation is 100 % (no grey mixed, the family rule); lightness is high (L\* 71).
  - **Legibility, computed from `metalColors`** [A, WCAG contrast ratios]. The renderer's luma is .68, over
    its .38 threshold, so labels take the **dark ink** (`#181926`): 6.2 : 1 on the top of the face, 3.3 : 1
    at the bottom of its gradient (gfx-widener's ice: 8.9 and 4.5). The engraving's dark stroke gives 4.4
    to 2.4 : 1 and its light stroke 2.2 to 4.1 : 1: one of the two always shows. Fine at the top, thin at
    the bottom: to check at tile size, where the lowest labels sit on the darkest metal.
  - **The lit rune needs a darker light**, as on gfx-widener and gfx-tape: the default, mix(hue, white, .3)
    = `#bcc2ff`, is 1.65 : 1 on the body. **`RUNE_LIGHT['gfx-stretch'] = '#3040c8'`** (2.8 : 1; ΔE 34 from
    delay's blue, 55 from the widener's light). In the cartouche mode the rune sits on its dark pill and
    needs nothing; this is for the engraved and clearing modes, where the core, mix(light, white, .75), is
    close to the body and the glow carries the rune (the widener's open point, same answer for both).
- **Motif `chirp`** (new, in `motifParts`): **one wave whose period grows ×8 from left to right**, time
  being stretched.
  - y = cy + a · sin(φ(x)), with the instantaneous period p(x) = p₀ · 8^(x / W), p₀ = 5 px, so
    φ(x) = 2π · W / (p₀ · ln 8) · (1 − 8^(−x / W)). That is ≈ 6.7 cycles over an 80 px face.
  - a = 0.7 · ch; drawn with `line()` from x = 0 to W in 1 px steps, like `waves`. A second copy at
    opacity .45, offset by +4 px in y and by a quarter of the local period, suggests the stereo wash.
  - It lies in the rune band like `waves` (clear of the knob grid), and is told apart from `waves` (two equal
    sines) by its glide from dense to wide.
- **Face** (`visuals/data/faces.json`):
  `"gfx-stretch": {"name": "Stretch", "blocks": ["stretch"], "knobs": ["amount", "fft", "spread", "vol"], "selectors": ["mode"], "toggles": ["capture", "freeze", "thru"], "category": "reverb", "color": "#a0a8ff"}`.
  "Stretch" is the catalogue's plain word, not a maker's name.
- **Glyph**: a pedal's rune is its first block's glyph, and `visuals/glyphs/stretch.svg` exists (a sine of
  constant period over a double arrow). Proposed refinement, the same idea in chirp form: half-cycles
  widening 2.6 → 7.3 px over the same double arrow (24 × 24, `stroke-width="1.8"`, round caps and joins, as
  `glitch.svg`):

  ```
  <path d='M2.5 9.5q1.3-6 2.6 0q1.9 6 3.8 0q2.65-6 5.3 0q3.65 6 7.3 0'/>
  <path d='M6 19.5h12M15.5 17l2.5 2.5-2.5 2.5M8.5 17 6 19.5 8.5 22'/>
  ```

  The first half-cycle is 2.6 px wide with peaks ±3 px. That is the smallest that stays a wave and not a
  blob at a stroke of 1.8 (to check at 16 px).

## Open questions for review

All answered at the review of 2026-11-14 (details in the Review below).

1. **Default `mode`** → **Rolling**, and golem pins Capture for the `stretch` block, saved chains included
   (§3.2). A bare pedal that is silent until a stomp breaks the maintainer's rule; the block's foot gesture is kept by
   the pin.
2. **Units** → `spread` in ¢ (with the 0.59 factor, §3.4), `fft` in ms, `maxlen` 1 – 60 s, `compress` kept.
   A correct use of the 2026-11-14 rule. golem-rack must accept the catalogue change (open point).
3. **Rolling's resync** → the **hard rule and the jump to the present** in v1, with the fade capped at half
   the resync period (§2.1). It is predictable: the ghost ends within one period of the last note. v2
   candidates, to choose by ear once v1 plays: resync on an attack (gfx-dsp `onset`, from gfx-grains), on
   the host's bar (`sync`), and "finish the phrase" (no jump while the present is silent).
4. **Stand for `freeze` too?** → **No.** Only one id of six matches, and its gesture is different: an
   instant hold of the present under the dry, with `mix`, a release `decay` and a `tone` tilt. Adding those
   here would give this pedal two identities. gfx-grains' design also keeps GoleM Freeze as its own block.
   - What is true: Rolling at ×1 + `freeze` **is** a spectral freeze of the present, with overlap 4 and a
     calibrated level. So the rework of the quarantined block should be a **thin crate on gfx-dsp `stft`**
     (held magnitudes, `cloud` as `smear`, the six catalogue ids), after this one is benched. The maintainer's call.
5. **Coherent end** → **peak-locked phase vocoder** in v1. The code exists (`pv_shift.rs`, `regions`,
   `map_freq`), it gives the exact identity at ×1, and every pass of it can be cut into units. RTPGHI's
   heap over 131,073 bins cannot be cut into bounded units, which the schedule requires. It stays a
   research item for gfx-bench, with no id at stake.
6. **Memory** → **accepted**: 51 MB at 48 kHz, 121 MB at 192 kHz, one instance per rig. No shorter pool at
   high rates: `maxlen` would then mean something else at 96 kHz.
7. **f16 takes** → **kept.** The error is 73.5 dB under the signal and follows it. The identity is a test,
   not a use: −66 dB with the shipped pool, −90 dB with the f32 pool in the test build (the pool is generic).
8. **Window cap** → **2^18 samples at every rate.** At 48 kHz, golem's rate, it is the catalogue's own top
   (5.46 s). Scaling it with the rate would quadruple the engines for a window nobody asked for.
9. **Coherent head crossfade** → **complex equal-power sum of the two heads' spectra, each with its own
   phases, one synthesis** (§2.3). It equals two syntheses, by linearity.
10. **Hue** → `#a0a8ff` **kept**, with dark ink (the renderer picks it) and `RUNE_LIGHT` `#3040c8` (Visual).
    The lowest labels, on the darkest part of the face, are to be checked on the family sheet.
11. **gfx-glitch migration** → **later, in its own commit, bit-exact, with its FFT unchanged** (§6). And its
    worst block is not an FFT problem.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Strong**, after the fixes below. The live model is the right one: a stretcher cannot follow its
input, and the three behaviours offered (takes, a drifting head that resyncs, a held spectrum) are the
honest ones. The PaulXStretch reading holds line by line. The level calibration holds. The four-step FFT
is correct, accurate and worth its cost. What did not hold: one claim about the texture, one level law,
the blur as written, the conversion of `spread`, and the story told about gfx-glitch's CPU spike. None
touches the architecture. The file was checked for coherence after the designer's last edit: structure,
numbering and cross-references are intact.

### Checked against PaulXStretch's source (essej/paulxstretch, `main`)

`Stretch.cpp`, `ProcessedStretch.h` / `.cpp`, `StretchSource.cpp`, `PluginProcessor.cpp`:
- B = 2^(7 + 10 n) (14.5 only at prebuffer setting 5), window 2B, Hamming (`setFFTWindowingType(1)`),
  hop B, magnitudes only, `mt19937` phases in 2^15 steps, read step B / amount: **as written**.
- Module order = the enum's order (frequency shift, then pitch, then spread, then compressor); frequency
  and pitch shift on by default, spread and compressor off: **as written**.
- Spread: pole (1 − 2^(−10 n²))^(16,384 / nfreq), forward and backward twice, on a log axis from 20 Hz to
  Nyquist. σ = 44 ¢ at 0.7 and 1,534 ¢ at 1.0: **reproduced**.
- Compressor: (0.1 · R)^(−p): **as written**; the floor is on 0.1 · R, and the reference level was computed
  (−21.1 dBFS at the default window).
- All laws, ranges and defaults of the thirteen parameters, `loopxfadelen` 0.01, auto-finish: **as written**.
- Not re-read: the GUI-timer polling of `capture`, and the list of smooth sizes (`get_optimized_updown`).
  Neither matters to the design.

### Prototypes

In `scratchpad/proto-gfx-stretch/` of the review session (numpy; Rust with rustfft 6.4 for the timings):
Paulstretch against ours on a sine, a chord and noise (`texture.py`, `texture2.py`), the `cloud` level law,
the blur in both forms (`spread.py`, `spread2.py`), f16, PaulXStretch's compressor reference and the first
write of the pool (`misc.py`), the four-step FFT (`fourstep/`), and gfx-glitch as shipped with a timer on
each of its sixteen steps (`glitch-prof/`, a copy: the crate itself was not touched).

### Changes made, and why

| # | Change | Why |
|---|---|---|
| 1 | **"Overlap 4 fluctuates less" withdrawn** (§1.3); the Texture check rewritten | Simulated: a partial fades as much, slightly deeper (power variance 0.80 against Paulstretch's 0.54). What overlap 4 buys is an exactly constant expected power, where Paulstretch ripples by 0.4 dB. The good news is the other half: line width and fading rate match, so **Paulstretch's character is reproduced**. The draft's check (lower level deviation than overlap 2) would have failed on tonal input |
| 2 | **`cloud` level law: s², not s** (§2.5) | The coherent part of a bin jittered by ±π · cloud has amplitude s, so power s². Simulated: the corrected law is within 0.03 dB; the draft's was 1.2 dB off at 50 % and 1.75 dB at 75 %, outside its own ±1 dB check |
| 3 | **Blur in scatter form, τ_k = k · ln 2 · σ / 2,400** (§2.7) | As written (a varying pole in the usual one-pole) it gains 1.6 dB at 1,200 ¢ and 7.8 dB at 2,400 ¢, and moves a partial's centroid up by more than an octave: "the energy is kept" was false. The scatter form keeps it exactly, and the corrected pole gives σ within 1 % |
| 4 | **`spread` conversion × 0.59** (§3.4) | PaulXStretch blurs amplitudes, so a partial's power profile is the kernel squared: 26 ¢ at 70 %, not 44. Without the factor every converted chain would be 1.7 times too wide |
| 5 | **`compress` reference −21 dBFS**, continuous floor (§2.7) | −21.1 dBFS is PaulXStretch's own unity point at golem's default window, so converted chains keep their level; −18 was arbitrary. "No lift below −60 dBFS" as a threshold was a 24 dB step between two frames of a decaying take |
| 6 | **Minimum lag L₀ = N / 2**, not N / 2 + H (§2.1) | With N / 2 + H the wet at ×1 would be late by N + 2H, against the N + H stated everywhere else and benched |
| 7 | **Resync fade capped at half the period; never more than two heads; lag limit max(`maxlen`, 2 N); the ring is the whole pool** (§2.1, §2.10) | At ×1,024 and `maxlen` 1 s a resync comes every second: a 2 s fade needed a third head. And the old head reads on during the fade: 60 + 10 + 5.5 s does not fit in "`maxlen` + one window" |
| 8 | **`capture` during a commit crossfade**: the fading head holds its spectrum (§2.1) | The buffer to record into was the one that head was still reading |
| 9 | Pre-roll copied aside at capture-on; the seam's limit on a pure tone stated; the bench's seam check moved to a chord and noise | An equal-power fade between two moments of one sine can cancel: "within ±1 dB on a sustained sine at an arbitrary length" could not pass, in any looper |
| 10 | `capture` on at load does not record (§2.1) | An edge-triggered parameter restored by the host |
| 11 | **Four-step FFT: measured, and used above 16,384 only** (§2.4) | A one-piece 2^18 FFT takes 435 µs here, not 2 – 8 ms: still a third of a 64-sample block, so the idea stands. The four-step costs 3.4 times a one-piece FFT in total, so small sizes keep realfft. Units of 7 – 17 µs; accuracy 1.5e-7 |
| 12 | **Every O(N) pass resumable; nothing O(N) at a size change; engines cleared in units; pools never cleared** (§2.4, §2.10) | gfx-glitch's profile shows where spikes come from: bin passes in one piece, not the FFT. At 2^18 a window table, a `fill` of an engine or the read of one frame from the pool is each a spike of its own |
| 13 | **The gfx-glitch claim corrected; migration deferred, bit-exact, FFT unchanged** (header, §6) | Measured on gfx-glitch as shipped: FFT step 85 µs, map 590 µs, density 586 µs. The four-step FFT would not remove its worst block, and would break the −240 dB null the same paragraph required |
| 14 | Coherent head crossfade settled: complex equal-power sum (§2.3) | Linear, exact, one synthesis. "Keep the louder phase" breaks phase locking |
| 15 | Mono detection by the last L ≠ R position (§2.2) | Comparing 2^18 samples per frame is one more O(N) pass |
| 16 | **`onset` removed from v1** (§2.7, §3) | Not in the catalogue, off in PaulXStretch, no crisp attack can come through random phases and a long window, and an id is permanent |
| 17 | 21 sizes, not 27; CPU estimate redone with the measured FFTs (185 ns, 0.9 %); first write of the pool measured (40 ms at 192 kHz, not 12) | Arithmetic and measurements |
| 18 | f16: 73.5 dB measured (66 dB is the per-sample bound); `half` with F16C; generic pool; identity gate −66 dB | The figure, and how the −90 dB test stays possible |
| 19 | Latency: the ×1 / low-`cloud` echo stated; golem-rack's precedent cited; first-take fade noted (§2.9) | "No comb to fix" has one exception, and it should be written |
| 20 | Level against PaulXStretch computed: +0.8 dB (§2.6) | So the conversion bench has an expected value |
| 21 | `freeze` (bool here, enum in gfx-reverb / gfx-shimmer), `state` (float meter), `spread_on` / `comp_on`, the pin on saved chains (§3.1, §3.2) | Id hygiene made explicit |
| 22 | Visual: ink and engraving contrasts computed, `RUNE_LIGHT` `#3040c8` | A light body: the renderer already switches to dark ink; the lit rune was 1.65 : 1 |
| 23 | Bench: unit times, `capture` edge cases, two-head bound, CPU through size / mode changes and at 192 kHz, `compress` floor, blur centroid | Each new rule has its check |

Checked and left as written: the three behaviours and their table; the take model (double buffer, 50 ms
bounce, auto-commit); the power-domain crossfade in random mode; the Parseval calibration (simulated: 0.0 dB
on noise, −0.1 dB on a chord, at 683 ms and 85 ms); Σ w² = 1.5 and 2.25; the resync example (11.4 s,
1.43 s); the second analysis at H / 4 for the frequencies; the identity at ×1; `width`; the `fft` table;
the 12-bit phasor table; linear ranges; no mode-dependent default; 2 / 2 first; `bypass` and trails; the
ΔE values of the hue (40, 45, 47, 49; 27 for the catalogue's colour against shimmer).

### Coverage

- Against PaulXStretch: the block's thirteen parameters, and four improvements that are real (sample-
  accurate capture, no prebuffer thread, units that mean something, a calibrated level). Its other modules
  (harmonics, tonal / noise, ratios, filters) are not in golem's block and are not missed.
- Against the hold pedals: two takes with a crossfade, a freeze with no click, and Rolling, which none of
  them has in this form.
- **gfx-grains** (designed the same day, `../gfx-grains/README.md`): no real overlap. It is time-domain,
  holds 10 s, and stretches a few times by scanning; this one is spectral, holds 60 s, and goes to ×1,024.
  Its "Time-Stretch Freeze" and our "Tight grain" meet in the middle, from two sides. Nothing forces shared
  code: its buffer is f32 with a decimation pyramid, ours f16 takes. Two things to align when both are
  coded: its `state` is drawn as a read-only **enum**, where the contract and gfx-gate use a read-only
  `FloatParam` (as here); and its gfx-dsp `onset` detector is what a v2 resync here would use.
- Missing, on purpose: a way to stop a take other than `bypass` (with trails) or `vol` on EXP. If the
  footswitch needs it, a `clear` gesture is a golem foot-mode question, not a new DSP.

### What stays open

| Point | Owner |
|---|---|
| Catalogue change: `spread` in ¢, `fft` in ms, `maxlen` max 60; conversion of saved chains (§3.4); `mode` Capture pinned for the `stretch` block, saved chains included | golem-rack |
| Switch-on: `prepare` writes 51 MB (7 ms) to 121 MB (40 ms). Is that inside the stall check's tolerance (PaulXStretch passed it at 2 ms)? If not, a fallback has to be designed (for instance the pool touched in small units over the first seconds, `maxlen` limited meanwhile) | golem-rack to measure; then golem-fx |
| `amount` above ×100 in the catalogue (only for presets such as `glacier`) | golem-rack |
| gfx-glitch: its worst block (cut the map, density, sustain and slot steps into units, bit-exact), the wrong cause in its README, then its move onto gfx-dsp `stft` | golem-fx, after gfx-stretch's bench; the maintainer's go, as for any change to a shipped pedal |
| The `freeze` block's rework as a thin crate on gfx-dsp `stft` | The maintainer |
| By ear: the fading of a single note at long windows against PaulXStretch (ours fades slightly deeper); `cloud` 0 – 30 % at ×1.5 – ×4 on keys and voice; the first take's fade-in; Rolling's "ghost ends when you stop" against "finishes its phrase" | The maintainer, on `gfx-bench apps gfx-stretch` |
| Six-step (transposed) form of the four-step FFT, if the 3.4 × is worth winning back | golem-fx, at coding |
| The lowest labels and the lit rune on the periwinkle body; `chirp` against `waves` at tile size | The maintainer, on the family sheet |

## As coded (v0.1.0, 2026-11-15)

**Files.**
- gfx-dsp `stft.rs` (new; gfx-glitch untouched): the 21-size grid (`size`, `nearest_size`), periodic Hann
  read from two master tables of e^(−2πi m / T) (T = 2^18 and 3 · 2^16, built in `Tables::new` with every
  rustfft plan; a size change is O(1)), `OLA_AMP` / `OLA_POW` / `cloud_gain`, the real FFT as a complex
  FFT of N / 2 plus resumable twiddle passes (`post_twiddle`, `pre_twiddle`, `read_time`), the complex FFT
  in one unit up to 16,384 and as a four-step above (`cfft_unit`: N₂ column FFTs of N₁ gathered a few
  columns at a time with their twiddles, then N₁ row FFTs; its output stays transposed and the twiddle
  passes read it in place, so there is no transpose pass), the schedule `due`, `XorShift` and the
  4,096-entry `Phasors`, branch-free `fast_atan2` / `fast_sin_cos` / `wrap_pi`, gfx-glitch's `regions` and
  its resumable two-sweep form `RegionSweep`, the scatter-form constant-Q `blur_pass`, `SizeFade`.
  10 unit tests.
- `src/memory.rs`: the pool (generic `Sample`: f16 by the `half` crate, f32 for the tests), takes and ring
  with valid lengths, `touched` (every page written in `prepare`). `src/engine.rs`: the per-size engine
  (passes and units, heads' analyses, map, mix, modules, phases, overlap-add). `src/lib.rs`:
  `StretchCore<S>` / `Stretch` (the f16 processor), `PARAMS`, `MODES`, `METERS`, the catalogue conversions
  `spread_from_catalogue` / `fft_from_catalogue` (§3.4). `src/plugin.rs`: the VST3, class id
  `GoleMFxStretch01`, 2 / 2 first then 1 / 1, latency 0, sample-accurate automation (so `capture` lands on
  its sample), `bypass` = `make_bypass`, `state` read-only. `src/tests.rs`: 22 behaviour tests (§5), and
  ignored reports (`cpu_report`, `prepare_report`, `fault_probe`, `prof_coherent`).
- gfx-bench: the crate in `EFFECTS` / `make()`; `options()`: `stereo_by_nature`, and the silence floor read
  after 7 s instead of 3 (a new `Options::silence_tail_s`, default 3 s: every other effect unchanged).
- `presets/gfx-stretch.json`: the 15 preset ideas of §4 (Capture-lineage presets set `mode` Capture).
  Visuals: the face, groups (Spectrum, Modules, Takes, Output; `capture` depends on `mode` = Capture,
  `compress` on `comp_on`), the `chirp` motif, hue `#a0a8ff` (family finish, anodized), `RUNE_LIGHT`
  `#3040c8`, and the chirp form of the `stretch` glyph.

**Measured** (48 kHz unless stated; synthetic signals; the f32 pool unless "f16").

| §5 row | Bound | Measured |
|---|---|---|
| Four-step FFT | ≤ 1e-6 relative RMS of realfft, forward and inverse; median unit ≤ 25 µs | every size (the one-piece ones too) within 1e-6 both ways; 2^18: 256 units, median 4.3 – 5.6 µs, 99th percentile 10 – 26 µs |
| Identity (`cloud` 0, ×1, Rolling) | −90 dB (f32 pool), −66 dB (f16) | every size 256 – 2^18: under −90 dB (peak and RMS); f16 worst −73.2 dB RMS under the signal |
| Level, random (`cloud` 100 %) | ±0.5 dB | noise −0.07 … 0.00 dB, chord −0.19 … +0.12 dB (N = 2,048 / 8,192 / 32,768; ×1, ×8, ×100) |
| Level, ×1, any `cloud` | ±0.5 dB | noise ±0.02 dB, chord −0.18 … +0.04 dB |
| Level, coherent at ×8 / ×100 | ±1 dB | `cloud` 75 %: −0.36 … −0.76 dB; `cloud` ≤ 50 %: noise −0.98 … −1.51 dB, chord with resolved partials (N ≥ 8,192) −0.13 … +0.02 dB, chord at N = 2,048 (unresolved) −1.86 … −3.06 dB. **Not met** (deviations) |
| One sine | ±0.5 dB over ≥ 2,000 windows | Rolling ×8, N = 512: −0.02 / +0.01 dB (L / R). Captured with its loop in phase: +0.50 dB (the equal-power seam adds up, §2.1), reported |
| Stretch accuracy | ×10: 1 s ± 2 %; ×0.5: 50 ms ± 5 % | ×10: 27 intervals 0.994 – 1.004 s; ×0.5 (N = 256): 554 intervals 48.8 – 51.2 ms |
| Capture | period 2 s × `amount` ± 1 hop; auto-commit ± 1 sample; < 50 ms ignored; on at load ignored | 192,000 samples ± 128 (×2); auto-commit at exactly 48,000; 40 ms ignored; `capture` on at load and in Rolling: no take |
| Seam (77,777-sample take, ×1, `cloud` 0) | ±1 dB; HF ≤ 3 dB over the neighbours | chord 0.69 dB, noise 0.27 dB (50 ms windows); HF +0.00 / +0.05 dB |
| Commit crossfade | ±1.5 dB | 400 ms levels (L / R power) through a 2 s commit: noise 0.10 / 0.12 dB, chord 0.77 / 0.44 dB (`cloud` 100 / 0 %) |
| Resync | at `maxlen` ± 1 hop; ≤ 2 heads; ×1,024 fades 0.5 s | ×8, `maxlen` 5 s: on the first sample past 240,000 (checked per sample; the block before reads 239,992); ×1,024, `maxlen` 1 s, `xfade` 10 s: never 3 heads, every fade 0.50 s (± 1 %) |
| Freeze | 1/3-octave ±0.5 dB over 60 s; level constant | noise frozen: bands within 0.45 dB over four 15 s segments; 1 s levels within 1 dB; the input stopping after the freeze changes nothing |
| Rolling ×1 | late by N + H ± 1 | coherent: exact (identity); random, 16 pips: centroids 5,143 samples late for N + H = 5,120 (0.6 % of N) |
| Modules | pitch +12: 880 Hz ± 1 bin; fshift +100: 540 ± 0.2 Hz; spread σ ±10 %, energy ±0.5 dB, centroid ±10 ¢; compress (1 − c) · 30 ± 1 dB, −21 dBFS kept, no lift under −72 dBFS, ≤ 2 dB per dB | pitch within a bin (random and coherent); fshift 540.15 Hz (random), 540.00 (coherent); spread 50 / 200 / 1,200 ¢ → σ 50.1 / 195.4 / 1,188.5 ¢, energy +0.34 / +0.19 / +0.03 dB, centroid +0.4 / +1.9 / −5.5 ¢; compress 0 / 50 / 100 %: 30.00 / 15.00 / 0.00 dB, −21 dBFS at 100 % within 0.5 dB, −80 dBFS not lifted; the law ≤ 2 dB per dB everywhere |
| Stereo | `width` 0: L = R bit-exact; `width` 100: coherence < 0.2; mono DI ≤ 60 % CPU | bit-exact (f16 pool, chord); coherence 0.009 (`cloud` 100 %, 200 Hz – 10 kHz); mono DI 158 against 179 ns (88 %, **not met**: deviations) |
| CPU, 48 kHz (mean / worst 64-sample block / 99.9th percentile) | mean ≤ 400 ns; worst ≤ 25 % | defaults (Rolling ×4, 683 ms, random) 179 ns / 4.0 % / 2.4 %; coherent 30 % 320 ns / 7.7 %; 5.46 s random 184 ns / 5.5 %; 5.46 s coherent 291 ns / 5.3 %; resyncs 206 ns / 3.8 %; size changes 683 → 5,461 → 21 ms 291 ns / 5.1 %; mode change + commits 107 ns / 3.0 %; coherent commits at 5.46 s 283 ns / 8.1 % |
| CPU, 192 kHz | worst ≤ 25 % | means 163 – 318 ns. 99.9th percentile 11 – 25 % in every case; worst block 17 % (defaults), 19 – 27 % (resyncs, size and mode changes, commits), 24 – 45 % (coherent at 683 ms / 5.46 s): see deviations |
| Memory and start | `prepare` ≤ 15 / 60 ms; no page fault in `process` | 57.7 MB at 48 kHz, 127.9 MB at 192 kHz; `prepare` 19 – 20 ms at 48 kHz, 42 ms at 192 kHz (a loaded machine); 0 page faults in `process` over 12 s (`/proc/self/stat`) |
| Latency / determinism / block size | reported 0, dry at 0; strict null; blocks 1 – 4,096 | bench: measured 0; bit-identical; −240 dB. The behaviour test also renders captures and a freeze at blocks 1, 64, 512, 4,096: identical |
| Conversion (§3.4 tables) | reproduced | every `spread` and `fft` entry within rounding |
| Common checks | — | 28 / 28 at 44.1 – 192 kHz; worst parameter step −118 dB (`lpf` 20 → 1 kHz at 44.1 kHz). CPU line: 100.6 ns mono, 159.7 ns stereo |

Not run here: the Texture row (no Paulstretch reference built), the Conversion renders against
PaulXStretch (golem-rack / REAPER), the level of a fading head when `capture` starts during a commit
crossfade (the hold is coded, its level not measured), and the Listening row.

**Deviations from the design, and why.**
- **Coherent level at ×8 – ×100** (`cloud` ≤ 50 %): the s² law holds at ×1 (±0.2 dB) and in random mode,
  but a peak-locked phase vocoder's frames do not add up fully coherently once the head moves at another
  speed than the output: −1.0 … −1.5 dB on noise, −1.9 … −3.1 dB on a chord whose partials the window does
  not resolve (N = 2,048), 0 dB on resolved partials. §5's ±1 dB is not met there; the tests bound the
  measured values. A cure would be signal-dependent (an estimate of each frame's coherence, or a slow
  level loop): left for the owner, by ear first.
- **Peak frequencies from the previous frame** when the head's last frame lies 0 < Δ ≤ N / 4 behind (amount
  ≥ 1: the standard phase vocoder's case, unambiguous within ±2 bins of each peak, which is where a peak
  is); the second analysis H / 4 earlier only for `amount` < 1, a new head and after a freeze. It halves the
  coherent analysis (434 → 320 ns per stereo frame at the defaults). Frequencies are computed at the peaks
  only, not per bin. Identity frames (×1, neutral) skip both: no frequency is needed.
- **Fast trigonometry**: polynomial atan2 and sin / cos (≤ 1e-6, no libm call, branch-free) for the
  coherent path; the identity still nulls to −90 dB.
- **Units of ≤ 1,024 elements** (512 for the coherent passes with a transcendental per bin), not ~4,096:
  at 192 kHz a 64-sample block lasts 333 µs and 4,096-element units of transposed reads cost 50 – 70 µs.
- **192 kHz worst block**: the 99.9th percentile stays ≤ 25 % in every case, but single blocks reach 24 –
  45 % in coherent mode (two units of the heaviest passes in one block, on a machine shared with other
  builds: load 6 – 9, best of three runs). The six-step (transposed) FFT and finer coherent units are the
  owner's options.
- **Memory 57.7 MB at 48 kHz, 128 MB at 192 kHz** (§2.10: ≈ 51 / 121). Per engine and channel: the frame
  buffer (N), the overlap-add ring (1.25 N), the mixed spectrum (2 × N / 2) and four arrays per head (2 × 4
  × N / 2): 7.25 N against the ~6 N counted. The pool, tables and pre-roll are as designed (the pre-roll
  ring is 2 × 250 ms, below).
- **`prepare`** 19 – 20 ms at 48 kHz (bound 15): the first write of 57.7 MB on a loaded machine (the tables
  are built from √T sines and cosines, ~1 ms). 42 ms at 192 kHz (bound 60).
- **Mono-DI analysis** saves 12 % at `width` 100 %, not 40 %: each channel still synthesises its own phases
  (the copy skips reading, the FFT and the twiddle pass). Mono is detected per take (no L ≠ R sample in it)
  in Capture, by the last L ≠ R position in Rolling.
- **The seam is never one block**: the pre-roll is copied aside one sample per recorded sample (its ring
  holds 500 ms, so the 250 ms before a take stay intact for 250 ms more), and the seam is crossfaded 32
  samples per sample after the commit (8 ms at any rate); the new head starts when it is done. `state`
  shows Capturing meanwhile.
- **Silence check**: §5's "the default holds" was wrong. Rolling ×4 plays the bench's 0.5 s burst four
  times slower: its head leaves the burst at 4 × (0.5 s + N) ≈ 5 s, so the wet is −3 dBFS at 3 s. gfx-bench
  reads the floor after 7 s for this pedal (a new option, default 3 s); the −140 dBFS bound holds (−240).
- **Coherent modules**: fshift is part of the rigid region move, f′ = (f + fshift) · 2^(pitch / 12) per
  peak; spread rescales each bin to the blurred power and keeps its phase (a bin with no coherent energy
  gets a random phase).
- **Edges the design left open**: a bypass while recording commits the take; re-engaging in Rolling goes
  on from where the head is (Capture restarts the take, as designed); a commit during a commit crossfade
  drops the oldest head at once (never three heads); `maxlen` is latched when a take starts; the Rolling
  minimum lag is half the larger window during a size change; `hpf` at 20 Hz is a filter at 20 Hz, as
  gfx-glitch's; `map_freq` stays in gfx-glitch (its form is slot-specific), the stretch's rigid move is in
  its engine.

**Left for the owner.** By ear (`gfx-bench apps gfx-stretch` is not written yet): the coherent end at ×1.5
– ×4 and its level, the first take's fade, the fading of one note against PaulXStretch; the Texture and
Conversion rows against PaulXStretch renders (golem-rack); golem-rack's catalogue change (`spread` in ¢,
`fft` in ms, `maxlen` ≤ 60 s, `mode` Capture pinned for `stretch`, `amount` > 100 for `glacier`) and the
switch-on stall check (`prepare` writes 58 – 128 MB).
