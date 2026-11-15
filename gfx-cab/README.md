# gfx-cab

> **Status: CODED** (v0.1.0, 2026-11-15), with its sibling **gfx-irfx**. Designed and reviewed 2026-11-14
> ([Review](#review-fable-52-2026-11-14) at the end, applied in the text). Built as designed: the
> slot-and-index plumbing (`ir_a` / `ir_b`, golem's `ir-index.json` with sha256, the persisted fallback,
> `loaded_a` / `loaded_b` meters, loads on nih-plug's background thread, synchronous load in
> `initialize()` on a miss, a per-process kernel cache), the zero-latency engine (64-tap head, five
> partition levels to 8,192 with their work spread over the callbacks and the big transforms split,
> one FDL for the whole class layout, per-level 30 ms crossfades), the background preparation
> (resampling, onset trim, auto-polarity, length / fade, minimum phase, correlation alignment,
> Loudness at +14 dB for gfx-cab and 0 dB for gfx-irfx), the designed built-in IR, and the mix
> (blend, `delay_b`, cuts, `bass_keep`, `dry`, Split). **The engine is −132 … −139 dB from an f64
> convolution up to 20 s, bit-identical at every block size and exact for a Dirac; a swap never
> dips (−0.06 dB) and never starts cold; auto-polarity agrees on all 262 same-cab pairs of golem's
> library; the library's cab IRs play +14.1 dB raw (median, as the review measured) and within
> 0.26 dB of each other after Loudness.** A 200 ms cab costs 0.42 % of a core at 48 kHz. See
> [As coded](#as-coded-v010).

golem's **impulse-response loader**: a cabinet block that plays one or two IRs blended, with zero
latency. It stands for golem's `cab` block ("Cab (IR)", today x42 IR Convolver Mono) and keeps its
parameter id `level` ([golem contract](../docs/golem-contract.md)).

Its stereo sibling, **gfx-irfx**, plays golem's `ir-fx` block ("IR Effect", today x42 IR Convolver
Stereo: springs, echoes and reverb boxes captured as IRs) and keeps `wet` and `dry`. It is the same engine
in a second, thin crate (§2.8, decision 1), designed here because the two share everything except
the front panel and the channel layout.

What sets it apart:

- **Never blocks, never goes silent on a change.** Files are read, resampled and prepared on a background
  thread. The audio thread swaps the prepared kernel in and crossfades from the old one. The old and new
  kernels share the input's history, so the new IR is heard whole from its first sample, with no cold start.
  Today's convolver freezes the whole engine for about 540 ms when it is switched back on with its IR
  (`golem_rig.lua`, measured 2026-09-21).
- **True zero latency at any host block size.** The first 64 taps run as a direct-form FIR. The rest of
  the IR runs as non-uniform partitioned FFT convolution, with the work spread over the audio
  callbacks, the large FFTs included (§2.3). Everything runs on the audio thread, so a render does not
  depend on the host's block size or on a thread's timing. Between two machines it is the same to
  rounding: the FFT library picks its SIMD path per CPU.
- **Two IRs that do not fight.** Each IR's onset is found automatically and can be aligned by
  correlation, with its polarity set from the sign. There is also a manual delay in 0.01 ms steps, a
  polarity switch per IR, a level per IR and a blend that EXP can sweep. These are the controls that
  multi-mic users ask other loaders for.
- **The file reaches the plugin through a parameter, not a GUI.** A slot number per IR, and an
  index file that golem writes (§2.1). It works the same under REAPER and the Golem Engine, without
  rebuilding the chain, and a scene can switch it.

Reported latency **0**, and it is exact: h[0] multiplies x[n]. Mono 1 / 1 first, stereo 2 / 2 second
(gfx-irfx: 2 / 2 first).

## As coded (v0.1.0)

Files:
- gfx-dsp (new modules, no existing one changed): `convolve.rs` (engine), `ir.rs` (preparation),
  `cuts.rs` (the wet cuts), and behind the feature `ir-host` (hound, sha2, serde_json, rtrb; only
  these two crates enable it) `ir_host.rs` (index, WAV, hash, cache, loader, queues) and
  `ir_engine.rs` (`IrCore`, `Host`, `Worker`).
- `gfx-cab/`: `src/lib.rs` (`Cab`, `PARAMS`, the mix), `src/builtin.rs` (the slot-0 voicing),
  `src/plugin.rs` (VST3, class id `GoleMFxCab000001`, 1 / 1 then 2 / 2, meters read-only, `ir_files`
  persisted), `src/tests.rs`.
- `gfx-irfx/`: `src/lib.rs` (`IrFx`), `src/plugin.rs` (class id `GoleMFxIrFx00001`, 2 / 2 then 1 / 1),
  `src/tests.rs`, a README with its parameter table.

### Measured (crate tests and gfx-bench)

| §5 row | Measured | Target |
|---|---|---|
| Dirac | δ, Raw, Off: output = input sample for sample at 44.1 – 192 kHz; δ at 37: input delayed 37 exactly | exact |
| Accuracy | 0.2 s −133.7 dB, 2 s −132.4 dB, 20 s (gfx-irfx) −131.9 dB re output RMS; engine alone 300 – 70,000 taps −132.5 … −139.0 dB; a 44.1 kHz library cab through a 48 kHz instance −138.5 dB against its prepared IR; a 9.1 s stereo echo IR −132.0 dB | ≤ −110 dB |
| Block-size independence | Blocks 1, 7, 64, 1,000, 4,096 and a seeded random mix: bit-identical, two kernels and a swap at the same sample; gfx-bench −240 dB (all rates) | bit-identical |
| Schedule | `late_count` = 0 in every test (a level read before its cycle ended would count) | 0 |
| Zero latency | output[0] = h[0] from a first block of one sample | exact |
| Split transforms | forward and inverse vs whole realfft ≤ −130 dB (4,096 / 4 and 8,192 / 8) | ≤ −130 dB |
| No cold start | from 100 ms to 2 s at t: −132.6 dB against the full convolution once the last level has crossfaded | ≤ −110 dB |
| Swap | `ir_a` every 500 ms under a 1 kHz sine: lowest 5 ms window −0.06 dB re the quieter IR, HF burst −68.8 dB re peak; per-level fades: head + level 1 within one chunk, the last level within 2B + 30 ms | no step > −60 dB, ≤ 3 dB dip |
| Failures keep playing | unknown slot −1, hash mismatch / bad WAV −2, the previous IR plays; the previous slot again: meter back with no job; a slot written before its entry loads 0.50 s after the entry | as stated |
| Index plumbing | slot → `loaded_a` = slot through the host / worker path; 1,000 atomic index rewrites during 300 loads: 0 failures; slots 1 … 65,535 through the f32 normalised value exact; persisted path used when the index lacks the slot, the index wins otherwise | as stated |
| Activation | 20 × prepare / reset with the file and the index deleted: same kernel, meter kept (no I/O, no preparation); a second instance on the same IR gets the same `Arc` | as stated |
| Render determinism | slots set before `initialize()`: identical renders at blocks 64 and 1,000 from sample 0 | identical |
| No allocation in process | `assert_no_alloc` (nih-plug's allocator) around tick + process over 50 swaps with `align` changes | none |
| Resampling | complex error re level: 44.1 → 48 −106.7 dB to 20 kHz, 48 → 44.1 −107.7 dB to 19 kHz, 96 → 48 −99.7 dB, 48 → 192 −112.6 dB; 30 kHz alias 96 → 48 −119 dB; image 48 → 192 −110.7 dB | ±0.05 dB, ≤ −100 dB |
| Onset / polarity | onset keeps 0.5 ms, first peak bit-equal from 0.25 ms before it; B = −A late 1.37 ms: lag within 0.01 ms, blend within 0.012 dB of A to 5 kHz | as stated |
| Polarity on real files | 262 same-cab pairs (maestrium 1960TV and V30 sets, Palmer SM57 + Aston, Dauntless, Glockenklang): 0 left opposite, Correlation flips none | 0 |
| Minimum phase | 1/12-octave within 0.0001 dB; cumulative energy deficit 5 × 10⁻⁷ of the total | ≤ 0.1 dB, ≥ |
| Normalisation | 87 cab IRs of golem's library: raw +14.1 dB median (5 – 95 % spread 5.1 dB); after Loudness 0.26 dB spread on band-limited pink noise. 13 effect IRs: raw +17.1 … +28.0 dB (median +20.1), exactly 0 dB after Loudness | ≤ 1 LU, median 14 ± 0.5 |
| Length / fade | 200 ms cab-like IR cut at 40 ms: 1/12-octave curve rises ≤ 0.16 dB over the whole IR's with a 20 % fade (0.09 dB hard) | ≤ 0.5 dB |
| `delay_b` | half a sample: −0.45 dB at 10 kHz; ±10 ms in 2 s: HF −60 dB re peak; bench step −10 → +10 ms −82 dB | ≤ 0.5 dB, no click |
| `bass_keep` | IR = δ at 120 Hz: flat within 0.000 dB | ±0.1 dB |
| gfx-irfx channels | 2 ch L / R, 1 ch on both, 1 / 1 averaged, 4 ch plays 1 and 4: all −110 dB or better against references; predelay exact; reverse; width 0 % = L = R; trails ring +20 dB 125 ms after bypass, trails off = the input exactly after 10 ms | as stated |
| CPU (64-sample blocks, this machine) | 200 ms cab 0.42 % of a core at 48 kHz; with B 0.79 %; 2 s cab 0.80 %; two IRs stereo at 192 kHz 6.2 %; gfx-irfx 4 s stereo 1.0 – 1.7 %, 20 s stereo 1.7 % | 1 %, 1.7 %, 12 %, 3 % |
| CPU spikes | 99.9th-percentile block 3.7 – 4.7× the mean, 1.7 – 3.7 % of the period at 48 kHz; at 192 kHz stereo A + B 23 % of a 64-sample period (333 µs). Cost model: worst chunk 2.6× the mean | ≤ 4×, ≤ 5 % at 48 kHz |
| gfx-bench common checks | gfx-cab 28 / 28 (worst step −54 dB, `highcut` 1 → 22 kHz), CPU 75.6 ns / sample mono, 149 ns / frame stereo; gfx-irfx 28 / 28, 40 ns / 82 ns (no IR) | pass |

### Deviations from the text, and why

1. **Split transforms**: the 2B-point real transform is done as a B-point complex FFT of the packed
   signal (cut by 2 / 3 DIF passes into 4 / 8 sub-FFTs of 1,024), then a real post-pass that stores
   the spectrum in natural order; the inverse mirrors it. The text's "bins left in sub-transform
   order" assumed a DIF on the real sequence, which would need complex sub-transforms anyway. The
   pack, each pass and the post / pre-passes run in pieces: a first version with whole post-passes
   peaked at 100 – 170 µs per chunk (index arithmetic), now one piece ≈ one 1,024-point FFT.
2. **Cycle lengths**: level 4 spreads its cycle over 63 chunks and level 5 over 126 (B / 64 + 1 −
   stagger), so a staggered cycle still ends at its block boundary.
3. **Resampling precursor**: the sinc's precursor before the file's time 0 is kept (a lead of half
   the interpolator) when the onset trim or minimum phase follows, which then trims it; in Raw the
   file's time origin is kept and the precursor dropped (else the wet would be 1.3 ms late). The
   resampled IR is scaled by from / to so its response keeps its level. Both points were silent.
4. **`delay_b` smoothing**: each voice's delay (max(0, −d) for A, max(0, d) for B) runs through two
   25 ms one-poles instead of one 50 ms smoother of the signed value: crossing 0 at speed put a kink
   in the read head's speed (−41 dB HF on the bench's −10 → +10 ms step; now −82 dB). Under one
   sample the read is the cubic through taps 0 – 3, which meets the bypass at 0.
5. **Where the shared code lives**: `IrCore`, `Host`, `Worker` and the cuts are in gfx-dsp
   (`ir_engine`, `cuts`), not in gfx-cab: gfx-irfx linking gfx-cab exported two VST3 entry points
   when both were built together.
6. **No `ir_fixtures` bench option**: gfx-bench's common checks run on the built-in (gfx-cab) and on
   "none" (gfx-irfx); the IR rows are crate tests on synthetic IRs and on golem's library, read in
   place (`~/golem/assets/irs`, skipped when absent) rather than copied into the repository
   (licences). `load_ir(slot, channels, rate)` is the bench / test entry, as specified.
7. Silent points decided: multichannel cab IRs share one onset cut and take their polarity from the
   channel sum; the correlation uses each IR's first 0.5 s; `wet` −60 dB is off as `dry`; only −1 is
   retried (a −2 waits for a new slot); the debounce applies only when nothing but `length` / `fade`
   moved; the cache also keeps decoded sources (a settings change does not re-read the file).
8. **Spikes**: the measured 99.9th percentile is slightly over 4× the mean with two IRs (4.7×); the
   per-period bound at 48 kHz holds with margin (≤ 3.7 % vs 5 %).

Left for the owner and for golem-rack: the built-in voicing by ear (10 sections in `builtin.rs`, [I]);
the motifs `cone` and `impulse` and the teal on the family sheet; G_ref +14 dB or unity; R1 – R8
(index writer, engine table, waiting for `Loaded` before renders); the x42 null and blends by ear; a
bundled smoke test in REAPER / the Golem Engine (`check-block-stall`).

## 1. State of the art

Survey of 2026-11-14. **[V]** read in a source, manual or measurement · **[I]** inference, or a fact not
re-checked against a primary source · **[U]** could not verify · **[A]** our calculation · **[P]** the
review's prototype (numpy, 108 IR files of golem's `assets/`; see Review).

### What golem plays today

- **`cab`**: x42 IR Convolver Mono (zeroconvolv, `#CfgMono`). The only catalogue knob is `level` = "Wet
  Signal Level", −12 … +12 dB, mapped raw. The plugin's own range is −60 … +6 dB, with Dry −60 dB by default
  [V, MOD pedalboards page]. The IR is an **LV2 state property** (`zeroconvolv#ir`):
  - REAPER: written as a `P …#ir <path> 3` line into the FX chunk (`templates/cab-x42.RfxChain`).
  - Golem Engine: `ARDOUR.LuaAPI.set_plugin_insert_property` (`golem_watch.lua`). This call works for LV2
    only [V, `libs/ardour/lua_api.cc`: it calls `Plugin::set_property`, which only `LV2Plugin`
    implements].
- **`ir-fx`**: the same plugin in its stereo build (`#CfgStereo`), with `wet` (−30 … +6 dB, default −6,
  EXP) and `dry` (−60 … +6, default 0). golem writes both levels at every build ([library-tones.md
  §8](../../../golem/docs/library-tones.md)) [V].
- **Before the file reaches the plugin**, golem's compiler cuts a cab IR just before a "late impulse",
  with a 5 ms fade, in a cached copy (`compile_rig._ir_guard`). `ir-fx` IRs are copied whole. For a
  "pedal" IR, its measured length decides whether it plays as a cab or an effect (`ir_kind_of`: energy
  above −60 dB past 250 ms) [V].
- **Known pain points** [V, `golem_rig.lua` comments]:
  - Re-enabling the convolver with its IR stalls the engine for ~540 ms, so golem switches it through its
    `Enable` port and never bypasses it.
  - An asynchronous IR load racing a `TrackFX_Delete` has crashed REAPER (heap corruption, 2026-07-10).
  - Paths containing spaces break the unquoted LV2 state line, so every IR is copied to a safe name.
  - On Windows, the NAM VST3 that golem uses for IRs applies a fixed −18 dB to every IR; x42 applies no
    gain. Level parity between the two is therefore golem's job today.
- **zeroconvolv's engine** [V, README, except zita]:
  - "background processing … suitable to process long impulse-responses"; the kernel is
    zita-convolver's [I].
  - An optional **buffered** mode adds one cycle of latency for reliability and lower DSP load.
  - Configurations go up to true stereo (4 channels).
  - The README says presets are "normalized to yield approx equal loudness" and documents no
    normalisation for user IRs.
  - Its timing therefore depends on a thread meeting its deadline. That is the opposite of what golem's
    determinism rule asks for.

### Plugins and code

| Product | What it does well | Taken here |
|---|---|---|
| **x42 zeroconvolv** (GPL) [V] | zita-convolver partitions, background tail threads, true stereo | Configurations; **not** the threads |
| **LSP Impulse Responses** (GPL) [V, GStreamer port list] | Per-file FFT size (512 – 65,536), low and high cut with modes, dry, makeup. We could not confirm length, fade or pre-delay controls [U] | Wet cuts with slope choice |
| **KlangFalter** (GPL) [I] | HiFi-LoFi's FFTConvolver: a uniform head partition and a non-uniform tail on a background thread. Pre-delay, stretch, reverse, envelope, 2-band EQ | Reverse and pre-delay (gfx-irfx); the head / tail split, but without the thread |
| **HISE convolution** (GPL) [I] | The same family: a background thread option, pre-delay, damping | — |
| **NadIR** (free) [V, manual excerpt] | Dual IR, **output delay 0 – 20 ms per IR to shape phase**, polarity flip, **detects negative-polarity IRs on load and inverts them**, morph between IRs, per-IR level. Mono, dual mono and stereo. IRs up to 0.185 s, automatic resampling | Auto-polarity, per-IR delay and level, blend |
| **IRDX Studio** (free) [V, product news] | 5 IR slots, per-slot EQ and phase adjustment | Phase per IR |
| **Lancaster PULSE 2** [V, review] | One-click "Auto-Align" | `align` Correlation |
| **OwnHammer Studio Standard** [V, forum] | Mixes up to 8 IRs and **exports the mix as one IR** | The mix can be baked (decision 6) |
| **Two Notes DynIR / Wall of Sound** [V, vendor] | "Dynamic IR": 8 mics per cab, ~10,000 positions each, moved continuously. A forum reading says it is pre-captured IRs selected by position [I] | Blend under EXP is the cheap two-point version: a "mic move" between two captures |

### Hardware IR blocks

| Unit | IR handling | Note |
|---|---|---|
| **Line 6 Helix** [V, forum] | Imports a 48 kHz mono IR and truncates or pads it to 2,048 samples (42.7 ms), or 1,024 in a lighter block. Low cut, high cut and mix in the block | Users disagree on whether the truncation is audible |
| **Fractal Axe-Fx III** [V, forum] | "Normal" import gives ~2 K samples (40 ms) and "UltraRes" ~170 ms [I]. "FullRes" (firmware 17) processes IRs **up to 64 K points with zero latency** (1.37 s) [V, release notes]. The cab block can truncate the IR. Users report that RAW (non-minimum-phase) IRs carry so much delay they "cannot be aligned with the MPT factory cabs", and trim the files by hand | Minimum-phase transform as an option; a trim that also removes leading delay |
| **Neural DSP Quad Cortex** [V, forum] | IR loader blocks, "Phase Doctor" (a 180° flip plus an extra shift). Users ask for "a way to align phases just not 180 but in a variable manner". The simple delay moves only in 1 ms steps ("doesn't always work"); the lite IR block offers 0.01 ms | `delay_b` continuous, 0.01 ms display |

### Algorithms

- **Gardner**, *Efficient convolution without input-output delay*, JAES 43(3), 1995 [I, standard]:
  - a direct-form head, then partitions of doubling size;
  - a partition of size B can start at an IR offset ≥ B when it is computed at once.
- **García**, *Optimal filter partition for efficient convolution with short input/output delay*, AES 113th
  Convention, 2002 [I]: the cost-optimal mix of partition sizes, given a frequency-domain delay line (FDL)
  per size.
- **Wefers**, *Partitioned convolution algorithms for real-time auralization*, PhD thesis, RWTH Aachen,
  2015 [I]. It surveys uniformly partitioned (UPOLS) and non-uniformly partitioned (NUPOLS) overlap-save,
  and their scheduling. With time-distributed computation, a partition of size B needs an offset ≥ 2B.
- **Battenberg & Avizienis**, *Implementing real-time partitioned convolution algorithms on conventional
  operating systems*, DAFx 2011 [I]. Threads with deadlines work, but timing depends on the OS.
- **Hurchalla**, *A time distributed FFT for efficient low latency convolution*, AES 129th, 2010 [I]:
  spreading the large FFTs over sub-blocks evens out the CPU.
- **Minimum phase from the real cepstrum** (Oppenheim & Schafer, *Discrete-Time Signal Processing*,
  ch. 13) [I, standard]: fold the cepstrum of log|H|, then exponentiate. Time aliasing is limited by an FFT
  several times longer than the IR.

### Forum feedback: what players love and hate

- **Length** [V, Line 6 forum]: 1,024 versus 2,048 samples "barely audible", "you probably won't find
  any general consensus".
  - Our reading [A]: a truncated IR has a frequency resolution of about 1 / length. 2,048 samples at
    48 kHz resolve ~23 Hz, which is enough for a close-miked guitar cab. 512 samples (94 Hz) blur the low
    end and the cab's main resonance.
  - Hard truncation adds ripple; a fade-out removes it.
  - Room and distance IRs need hundreds of ms.
  - So: play the whole IR by default (it is cheap here), and offer a trim with a fade.
- **Low end**: close-mic proximity bumps and bass-cab IRs make the lows boomy. Players add a low cut. On
  bass, Parallax-style rigs keep the cab off the lows (gfx-bassdi's survey) [V].
- **Blending two IRs**: comb filtering when the onsets differ by a fraction of a millisecond (0.5 ms
  gives a first notch at 1 kHz [A]), and polarity flips. Requests: auto-align, continuous fine delay,
  auto-polarity (NadIR, PULSE, QC threads above).
- **Level jumps when swapping IRs**: vendors' IRs are peak-normalised (median peak −0.3 dBFS in golem's
  library), and peak says little about loudness. x42 plays user files raw. Measured on golem's files [P]:
  - 85 cab IRs: power gain on guitar-band pink noise **+14.1 dB median**, +12.2 … +17.4 dB (5 – 95 %),
    +11.4 … +20.8 dB overall. So raw swaps jump by up to 9 dB, and **a cab block adds about 14 dB**.
    (The NAM VST3's fixed −18 dB is the same fact seen from the other side.)
  - 6 "pedal" captures (EQ and DI boxes): −0.5 … +3.2 dB. They are unity-gain filters.
  - 13 whole effect IRs: +16.5 … +28.3 dB, median +20.9 dB.

### What we conclude

1. **Plumbing first**, because the DSP is known (§2.1). A slot parameter and an index file use the one
   channel golem already drives on both engines, the parameter. It lets a scene change the IR and lets
   golem verify the load. Plugin state is kept as a fallback.
2. **Deterministic zero-latency NUPOLS, with no thread**:
   - a direct-form head of 64 taps;
   - partitions growing ×4 from 64 to 8,192 samples;
   - the per-block work spread over the sub-blocks, with the large FFTs cut into sub-transforms;
   - CPU below 1 % of a core for a cab at 48 kHz (§2.6).
3. **Prepare IRs off the audio thread, swap with a crossfade, share the input FDL** between the old and new
   kernel. The FDL runs for the whole layout at all times, so a longer IR finds its history ready.
4. **Blend tools in the order players need them**: onset trim, auto-polarity, correlation align, fine
   delay, polarity, per-IR level, blend.
5. **Loudness normalisation** so that a swap does not jump, referenced to the level a typical cab IR
   plays at today. Raw stays available for parity.
6. **Not a cab model.** gfx-bassdi's IIR speaker voicings are a smoothed curve with no CPU cost. gfx-cab
   plays a real miked cab in full detail (§6).

## 2. Design

```
in ─┬─────────────────────────────────────────────────────────────── × dry ──────────────┐
    ├─ [bass_keep LR4: lows → dry path] ────────────────────────────────────────────────┤
    └─▶ input FDL (shared) ─┬─ kernel A ─ × level_a · (1 − blend) ─ delay_a ─┐            │
                            └─ kernel B ─ × level_b · blend · ±1 ─ delay_b ─┴▶ Σ ─ low / high cut ─ × level ─ + ─▶ out
```

Kernels A and B are prepared IRs: loaded, resampled, aligned, trimmed, normalised and partitioned.
Only `ir_*`, `align`, `normalize`, `min_phase`, `length` and `fade` rebuild a kernel. In `align`
Correlation, B is prepared against A, so a change of A rebuilds B as well. Every other control
acts on the outputs and is smoothed, so it is EXP-safe.

### 2.1 How the file reaches the plugin (decided)

**Constraints.**
- VST3 has no string parameters, and golem shows no plugin GUI.
- REAPER and the Golem Engine (Ardour 9.9, headless) must both work.
- golem already drives every golem-fx block through parameters on both engines (`apply_golem_fx`,
  `ardour_plan.plan_block`).
- Ardour's `set_plugin_insert_property` is LV2-only [V], so a VST3 state change has no runtime path in
  the Golem Engine without a new C++ binding in the fork. (`Plugin:load_preset` is bound in Lua, but a
  preset file per IR is the state route again.)
- REAPER can replace a VST3 state: golem already does `vst_chunk` surgery for the NAM VST3 on
  Windows / macOS, including REAPER's `<size><1><state><8-byte trailer>` wrapper.
- A nih-plug state load re-runs `initialize()` and `reset()` under the plugin lock [V, the fork's
  `set_state_inner`, `src/wrapper/vst3/inner.rs`]. That is acceptable at load time, but too heavy for a
  scene switch, and `reset()` cuts the tail.
- nih-plug cannot tell the plugin that a render is offline [V]: `ProcessMode` is read in
  `setupProcessing` only, and `BufferConfig.process_mode` holds the mode of the *previous* call
  (`wrapper.rs`, stored before it is updated). Ardour's export sets `kOffline` in each
  `ProcessData`, which nih-plug does not read, and never re-runs `setupProcessing` for it
  (`vst3_plugin.cc`; `set_non_realtime` is called for region effects only). So nothing here may depend
  on knowing the mode (point 6).

**Options weighed.**

| Mechanism | REAPER | Golem Engine | Scene switch | Verdict |
|---|---|---|---|---|
| Path in plugin state (`#[persist]`) | Proven (`vst_chunk`, as NAM) | Needs a fork binding, or a `.vstpreset` per IR | State reload → `initialize()` | **Fallback only** |
| Fixed IR folder + index parameter (rank in a directory listing) | Parameter | Parameter | Yes | Rejected: ranks move when a file is added |
| **Slot parameter + golem-written index (permanent numbers)** | Parameter (linear) | Parameter | Yes, crossfaded | **Chosen** |
| Content address in parameters (48 bits of the sha256 in three 16-bit integers, file found by name in a fixed folder) | 3 parameters | 3 parameters | Yes | Rejected: no index and no registry, but three writes that are not atomic and three meters to read back per IR |
| A side channel (OSC / socket to the plugin) | — | — | — | Rejected: a new channel, outside the contract |

**The mechanism.**

1. **Slot parameters** `ir_a`, `ir_b` (gfx-irfx: `ir_a`). Each is an nih-plug `IntParam` 0 – 65,535 with
   a linear range, so REAPER's (v − min) / (max − min) lands exactly.
   - In gfx-cab, slot 0 of `ir_a` is the **built-in** IR (§2.7), so an inserted cab is never silent.
     Slot 0 of `ir_b`, and of gfx-irfx's `ir_a`, is **none**.
   - A 1 / 65,535 step is 1.5 × 10⁻⁵ against an f32 ulp of 6 × 10⁻⁸ near 1, so the round trip through
     the normalised value is exact [A]. The range is permanent (REAPER stores the normalised value), so
     65,535 is final.
2. **The index**, a JSON file that golem writes atomically (write to a temporary file, then rename).
   - Path: `$GOLEM_FX_IR_INDEX` if that variable is set; otherwise `$XDG_DATA_HOME/golem-fx/ir-index.json`
     (default `~/.local/share/golem-fx/ir-index.json`).
   - Later on macOS: `~/Library/Application Support/golem-fx/ir-index.json`. On Windows:
     `%APPDATA%\golem-fx\ir-index.json`.

   ```json
   {"format": 1,
    "slots": {"17": {"path": "/home/…/assets/.cache/3fa1b2c4_412-v30-57.wav",
                     "sha256": "9c…", "name": "4x12 V30 dyn cap-edge", "kind": "cab"}}}
   ```

   - **Slot numbers are stable.** golem gives one number per file content (sha256), in increasing
     order. A number is never given to another content while golem runs; once all 65,535 are taken, golem
     may recycle the least recently used ones at start-up. A slot is a handle on one machine. golem's
     presets keep golem's own reference to the IR and compile it to a slot at every build, so a lost or
     rebuilt index heals at the next build.
   - **One writer per index.** The index is its own registry: golem reads it back, under a file lock.
     A test bridge or a second REAPER gets its own file through `GOLEM_FX_IR_INDEX`.
   - **The index wins.** The state fallback (point 5) is used only for a slot the index does not hold.
3. **Load.** The plugin's `process` sees a slot change and posts a background task through nih-plug
   `execute_background`, at once: a slot is not debounced (§2.4). That call sends a small `Copy` enum
   over a bounded queue (a `try_send`; on a full queue it is posted again at the next block) and does not
   allocate [V]. nih-plug runs one worker thread per plugin class and per process, shared by all
   instances: loads are serial, and a 20 s IR in one gfx-irfx delays the next load by its own time.
   The task:
   - looks the kernel up in the cache (point 7) and stops there on a hit;
   - reads the index, each time: it is small, and an mtime test can miss a rewrite within the clock's
     grain. The rename makes each read see a whole file, old or new;
   - checks the file's sha256;
   - decodes the WAV with `hound` (16 / 24 / 32-bit integer or 32-bit float; mono, 2 or 4 channels;
     WAVE_FORMAT_EXTENSIBLE);
   - prepares the kernel (§2.2, §2.3) and hands it to the audio thread through a lock-free single-slot
     mailbox.

   The audio thread takes the kernel at its next 64-sample chunk boundary and crossfades to it (§2.4).
   It then posts the old kernel back through a return queue, so the last reference is dropped on the
   background thread.
4. **Verification.** Read-only meters `loaded_a`, `loaded_b`:
   - **the requested slot**, from the moment its kernel is installed (the crossfade starts);
   - the previous value while the load is pending;
   - `−1`: the slot is not in the index, or the file is missing;
   - `−2`: the file is unreadable, or its hash does not match.

   So golem writes a slot and waits for equality; a negative value is a failure; anything else is
   "pending". This is golem's `loaded_output` idea (sfizz's "Number of regions"), compared for equality.
   - **On any failure the previous IR keeps playing.** A wrong or missing slot never mutes a lane.
     Writing the previous slot again restores the meter at once, with no load.
   - While a meter reads `−1`, the audio thread posts the lookup again every 0.5 s, so a slot written
     just before its index entry still loads.
   - An IR longer than the class limit is cut with the `fade` and plays; the meter reads the slot. golem
     knows the length when it registers the file (R1).
5. **Fallback in state.** `#[persist = "ir_files"]` keeps `{slot: (path, sha256)}` for the IRs that have
   loaded. A session opened where the index lacks the slot (a project copied to another machine) uses that
   path when the file exists and its hash matches. golem may also write it through `vst_chunk` if it ever
   needs to; the plugin's state is nih-plug JSON, so no binary surgery is needed.
6. **Renders are exact without knowing they are renders.** The audio thread never waits for a load: the
   plugin cannot know that a render is offline (constraints above).
   - **`initialize()` loads synchronously** the slots its parameters hold, when their kernels are not
     cached. This is nih-plug's own rule (`InitContext::execute` has no asynchronous form, for this
     reason). A session, a render or a bench case that starts with its slots set is right from the first
     sample. It costs 5 – 20 ms once per cab IR, at session load, off the audio thread.
   - **A slot written while running** loads in the background, in ~5 – 50 ms (§2.6). golem waits for
     `loaded_*` = slot before it starts a render or a parity capture (R4). The 2 s pre-roll of both
     engines covers it as well.
   - A slot automated inside a render changes when its load completes, so that case is not
     reproducible to the sample. golem does not do it; scenes are live.
7. **Cache.**
   - Prepared kernels are immutable and shared (`Arc`) through **one cache per process**, keyed by
     (sha256, sample rate, kernel settings). Two lanes on the same IR share one kernel. Kernels in use
     are never evicted; unused ones are kept up to 64 MB, least recently used first.
   - Each instance also remembers the kernels of its current slots. After a scene has used an IR once,
     switching back to it is a mailbox swap.
   - `initialize()` after a deactivate / activate, or after a state load with the same slots, reuses
     them: no IO, no FFT. That avoids the x42 stall, and keeps the contract's "no blocking work when the
     plugin turns on".
   - `reset()` clears the input history and the FDL (a fixed state, as the contract asks) and keeps the
     kernels.

**What golem-rack must implement** (decisions at the end):

| # | Where | What |
|---|---|---|
| R1 | bridge | **Slot registry and index writer**: assign the slot per sha256 and write `ir-index.json` atomically at the contract path, **before** a rig that uses the slot is applied. One writer per index, under a file lock; isolated benches set `GOLEM_FX_IR_INDEX`. Register the file golem would have played (the cab IR guard's cached copy), and note its length against the class limit (2 s, 20 s) |
| R2 | engine table | `cab` → gfx-cab VST3, `ir-fx` → gfx-irfx VST3, `params` `level` → "Level", `wet` → "Wet", `dry` → "Dry". A new key mapping the block's file keys to slot parameters, e.g. `"file_slots": {"file": "IR A", "file_b": "IR B"}`. `loaded_output` "Loaded A" (and B), **equal to the slot written** |
| R3 | `golem_rig.lua` | Write the slot like any linear knob. **No** chunk line and no `Enable` trick: the plugin is never stalled by activation, and is switched by its `bypass` parameter (`kIsBypass`; for gfx-irfx with trails, as the contract's §Trails) |
| R4 | `golem_watch.lua` (Golem Engine), render tools | The slot through the parameter path. `verify_files` reads `Loaded A / B` instead of the LV2 property echo. **Before a render, an export or a parity capture, wait for `Loaded` = slot** on both engines |
| R5 | Designer / catalogue | A second IR picker for `cab` (`file_b`) once the blend is offered. Optionally widen `level` to −24 … +12 dB and `wet` to −60 … +6. The descriptor's expert knobs |
| R6 | Audition / scenes | Library audition and per-scene IR changes can become a slot write (crossfaded) instead of a rebuild |
| R7 | Conversion | `normalize` Off pinned for converted chains and for "pedal" captures (§2.2.7, decision 3); x42's Dry −60 dB = our `dry` default. The gain Loudness applies to a file comes from the `ir` module's own tool (`cargo xtask ir-info <wav>`), not from a second implementation |
| R8 | Tools | `check-engine-blocks` and `check-engine-parity` with IR fixtures in a test index (`GOLEM_FX_IR_INDEX`) |

### 2.2 Preparing an IR (background thread)

In this order, all in f64, deterministic:

1. **Decode** and convert to f64. A file with more channels than the class uses:
   - gfx-cab averages the channels to mono, except in `stereo` Linked on 2 / 2, which uses ch 1 → L and
     ch 2 → R;
   - gfx-irfx: §2.8.
2. **Resample to the host rate** whenever the rates differ (44.1 / 48 / 88.2 / 96 / 176.4 / 192 kHz and
   others).
   - Offline windowed sinc: Kaiser window, β = 10, 64 zero crossings per side.
   - Cut-off (−6 dB) at half the input rate when going up, and at 0.45 × the output rate when going
     down.
   - Computed [P]: ripple < 0.01 dB in the passband, stop band −104 … −106 dB.
     - 44.1 → 48: flat to 21 kHz, images below −99 dB from 23.1 kHz.
     - 96 → 48: flat to 20.5 kHz, aliases below −106 dB.
     - 48 → 44.1: flat to 19 kHz, −10 dB at 20 kHz.
   - New gfx-dsp function `ir::resample` (about 100 lines). Not a dependency.
3. **Onset and polarity** (`align` ≠ Raw):
   - **Onset** = the first sample with |h| ≥ peak − 20 dB, moved back by **0.5 ms**. The samples before
     it are dropped, and a raised-cosine fade-in covers the first 0.25 ms kept. The fade therefore ends
     0.25 ms before the onset and never touches it. (As first written, a 16-sample fade inside a 12-sample
     back-off was still rising on the first peak.)
     - When less than 0.5 ms precedes the onset, nothing is dropped and **nothing is faded**: most vendor
       files start at their onset, with their peak 0.04 – 0.19 ms in [P].
     - On golem's cab files, most start at their onset and lose nothing. Those with 0.6 – 1.6 ms of lead
       lose less than −60 dB of their energy [P].
   - **Auto-polarity**: the sign of the **first local extremum that reaches −12 dB under the peak**. A
     negative sign flips the IR.
     - On 209 pairs of captures of the same cab (58 files, 8 vendors' sets), this rule gives the two
       files of every pair the polarity their correlation asks for: 209 / 209 [P].
     - The sum of the first 2 ms, as first designed, fails on 37 of them: after the first spike a cab
       swings the other way, and the sum follows the swing. It flipped one of two SM57 captures of the same
       speaker. The sign of the largest peak fails on 23.
4. **Length and fade.**
   - `length` cuts the IR. When the cut falls inside the file, a half-Hann fade covers the last `fade` %
     of the kept part. When `length` is longer than the IR, nothing changes.
   - Trailing samples below −120 dB re the peak are dropped. They are inaudible and cost CPU.
   - Many vendor files carry a noise floor to their end (1.6 s and 4 s files with a 100 – 150 ms cab
     [P]). It is played as it is; `length` is the user's tool. A noise-floor detector is not in v1.
5. **Minimum phase** (`min_phase`, gfx-cab only).
   - Real cepstrum with an FFT of 8× the IR length (a power of two, at least 2¹⁴); log-magnitude floor at
     −140 dB re the peak.
   - Fold, exponentiate, inverse FFT, then keep the original length.
   - Computed on four library cabs [P]: 1/12-octave magnitude within 0.05 dB, no energy lost by the
     truncation, cumulative energy never under the original's.
   - The transform removes the leading delay and the polarity (its first lobe is positive), so it
     replaces step 3 when it is on.
6. **Correlation align** (`align` Correlation, B against A, both already through steps 1 – 5; it must
   come after the minimum-phase step, which would undo it):
   - band-pass both 80 Hz – 3 kHz;
   - cross-correlate over a lag of ±2 ms;
   - take the positive peak of r. Take the negative peak, and flip B, only when it exceeds the positive
     one by more than 1.5 dB: 1 library pair in 209 has two peaks that close, and none is negative [P];
   - parabolic interpolation gives the sub-sample lag;
   - **the IR that leads is delayed** by that lag (offline sinc interpolation in the kernel), and the other
     is left alone. An IR cannot be advanced. After the onset trim the lag is a fraction of a millisecond
     (0.02 – 0.04 ms on library pairs [P]).

   A user's `polarity_b` then acts on top (XOR).
7. **Normalise** (`normalize`), as one gain on the kernel:
   - **Off**: the file as it is (x42 parity).
   - **Peak**: peak = 1.
   - **Loudness**: the IR's power gain, weighted by a reference spectrum, is set to a reference gain.
     g = G_ref · (Σ_f W(f) / Σ_f W(f) · |H(f)|²)^½, computed on a 2¹⁶-point spectrum.
     - gfx-cab: pink noise band-limited 80 Hz – 6 kHz, **G_ref = +14 dB**. That is the median of golem's
       85 cab IRs played raw [P]. Every IR then plays as loud as a typical vendor cab does today: a
       typical cab keeps its level within ±2 dB, the outliers move by up to 7 dB, and swaps match.
       With G_ref = 0 dB every existing rig would have dropped by 14 dB.
     - gfx-irfx: pink noise 60 Hz – 12 kHz, **G_ref = 0 dB**. `wet` then reads as the effect's level
       against the dry on a sustained signal, which is what the knob claims. Today's raw files measure
       +16 … +28 dB [P], so this moves every existing `ir-fx` setting: converted chains pin Off (R7).
     - A unity-gain "pedal" capture (an EQ, a DI box) played in gfx-cab would be raised by 12 – 14 dB:
       golem sets `normalize` Off for those at insert (R7).
   - **Per IR, before the blend**, so that A and B arrive matched and `level_a` / `level_b` stay trims.
8. **Partition** into the engine's layout (§2.3), which means the FFT of each partition. This is the only
   step that depends on the convolution engine.

On a 200 ms, 96 kHz IR played at 48 kHz, steps 1 – 8 take a few ms; the cepstrum dominates when it is on
[A, order of magnitude]. A 20 s, 96 kHz stereo IR takes a few hundred ms (hash, resampling, 2 × 4 M taps).

### 2.3 Convolution engine: new gfx-dsp module `convolve`

**Layout (fixed per sample rate, any IR).**
- **Head**: direct-form FIR over taps [0, 64), SIMD (8 lanes), per sample. This is what makes the
  latency 0 at every host block size.
- **Partitioned levels**, overlap-save with real FFTs of size 2B (`realfft`, already a gfx-dsp
  dependency):

  | Level | B | Starts at tap | Partitions | Rule |
  |---|---|---|---|---|
  | 1 | 64 | 64 | 7 | Computed at once when its input chunk completes: offset ≥ B |
  | 2 | 256 | 512 | 6 | Time-distributed over the next 4 chunks: offset ≥ 2B |
  | 3 | 1,024 | 2,048 | 6 | idem |
  | 4 | 4,096 | 8,192 | 6 | idem |
  | 5 | 8,192 | 32,768 | as many as needed | idem; cap B_max = 8,192. Its offset is 4B: it is run as offset 2B with two empty leading partitions |

  The schedule was run sample by sample in the prototype, each step reading only the input already
  received [P]: no result is ever late, and the output equals a direct convolution (−304 dB in f64) for
  IRs of 300 to 70,000 taps.

  B₁ = 64 at **every** sample rate. Measured on paper, a B₁ that scales with the rate costs more at
  192 kHz, because a 256-tap head is the most expensive part [A, flop model of §2.6].
  B_max = 8,192 bounds the largest FFT (16,384 points) and its burst of work.
- **Internal chunk of 64 samples**, counted from `reset()`. Input samples go into the head FIR and a
  64-sample FIFO. At each chunk boundary, level 1 runs and the levels above advance their scheduled
  share. Output never waits for a chunk.
  - Results therefore do not depend on the host's block size: with the parameters still, blocks of 1,
    7, 64, 1,000 or a random mix give **bit-identical** output (bench). A parameter change lands at a
    block start, as in every plugin.
  - Everything that changes the engine (a new kernel from the mailbox, the start of a crossfade) is
    taken at a chunk boundary, never at a block boundary.
- **Time distribution.**
  - A level-k block has B / 64 chunks (4, 16, 64, 128) to do its work: forward transform, the complex
    multiply-accumulates (MACs) of its partitions in equal slices, then one inverse transform per kernel.
  - Levels 3, 4 and 5 start their cycle 1, 2 and 3 chunks late, so that their transforms fall in
    different chunks.
  - **No chunk carries a transform larger than a 2,048-point real FFT.** Level 4's transforms are
    cut in 4 units and level 5's in 8, one unit per chunk:
    - forward: two (level 4) or three (level 5) radix-2 decimation-in-frequency passes, written by hand,
      then the 4 or 8 independent sub-FFTs;
    - inverse: the mirror image;
    - the bins stay in the sub-transforms' order. The kernel's spectra are stored in the same order,
      so the MACs need no reordering.
  - Why from v1: with whole FFTs, the flop model gives a worst chunk of **8 – 16× the mean** for a cab
    (0.6 – 0.9 Mflop in one chunk, the 16,384-point FFT alone 0.57). Cut as above: **2 – 4× the mean**,
    0.17 – 0.2 Mflop [P]. The first draft's "≤ 3×" could not hold with whole FFTs.
  - Bound: the worst chunk costs ≤ 4× the mean chunk, and ≤ 5 % of a 64-sample period at 48 kHz on the
    bench machine.
  - What a host block size changes is only how many chunk boundaries fall in one callback. A host that
    cuts its period into small blocks (automation, loop points) keeps the period as its deadline. Only a
    device period under 64 samples sees the chunk peaks at full height: at 32 samples and 192 kHz
    (167 µs) the worst chunk is about 20 % of the period [A, 5 Gflop/s].
- **One input FDL, several kernels.**
  - The spectra of past input chunks are computed once per level and stored in the frequency-domain delay
    line.
  - A kernel is just its partition spectra. Kernels A and B, and an old and a new kernel during a
    crossfade, all read the same FDL. So a new kernel's first block at each level is already the full
    convolution of past input (no cold start; −303 dB against a direct convolution, every level [P]).
  - **The FDL runs for the whole class layout, always**, whatever the kernels in place use. Otherwise a
    200 ms cab would leave level 5 unfed, and a longer IR swapped in would start its tail cold. Cost:
    one forward transform per unused level, +70 flop / sample for a cab (10 %, in the table of §2.6).
  - Each kernel adds only MACs and one inverse FFT per level.
- **Empty partitions are skipped**: MACs and inverse transforms follow the IR's length, not the layout's.
- **Numerics**: f32 data, f32 FFTs, MACs accumulated in f32 per partition sum. Target error ≤ −110 dB re
  the output's RMS against an f64 direct convolution, for IRs up to 20 s (bench). f64 accumulation only if
  that fails. FTZ on the audio thread.

### 2.4 Swaps, crossfades and structural changes

- **A slot, an enum or a switch rebuilds at once.** Only the continuous rebuilding controls (`length`,
  `fade`) are debounced, 100 ms while they move. A scene's IR change must not wait.
- **The crossfade is per level.** A new kernel cannot give all its levels at once: a time-distributed
  level computes a block one cycle ahead, so the new kernel's first block at a level of size B is ready
  B to 2B samples after it arrives. Computing it at once instead would be a burst of several large FFTs.
  - The head and level 1 switch from the next chunk (≤ 64 samples), level 2 within 512, level 3
    within 2,048, level 4 within 8,192, level 5 within 16,384.
  - Each level then crossfades old → new linearly over **30 ms**, starting at its first new block.
  - At 48 kHz: the first 43 ms of the IR (head to level 2) have changed 11 ms + 30 ms after the load;
    43 – 171 ms of the IR follow within 43 ms; the part beyond 171 ms within 171 ms; beyond 683 ms within
    341 ms. Each part is exact when it arrives. The late parts of an IR switching a little later than
    its attack is not audible as such.
  - Measured in the prototype for one arrival phase: 0, 47, 431, 1,199, 7,343 and 15,535 samples [P].
- The two outputs are highly correlated (the same input, often similar IRs), so a linear, equal-gain
  law is right. Between two unlike cabs the uncorrelated part dips by up to 3 dB for a few ms.
- CPU: the second kernel's MACs and inverse transforms, only while a level still runs both.
- Requests that arrive during a crossfade queue up. Only the latest request is kept.
- `bypass`: gfx-cab crossfades wet → dry in 10 ms. gfx-irfx has `trails`: with trails on, the input to
  the kernel is muted and the tail rings out (§Trails of the contract).

### 2.5 Blend, alignment, outputs

- **Blend.** Gains are g_A = level_a · (1 − blend) and g_B = level_b · blend · (±1).
  - Linear law, because aligned IRs of one cab are correlated.
  - When `ir_b` is none, A plays at level_a whatever `blend` is (a rule, not a default).
  - Smoothed by gfx-dsp `exp_smooth`, since EXP can sweep it.
  - B runs whenever it is loaded, also at `blend` 0: a level stopped would need up to 2B samples to
    come back (§2.4), and the pedal would sweep into a late tail.
- **`delay_b`**, −10 … +10 ms: a positive value delays B's output, a negative value delays A's.
  - gfx-dsp `delay` with Lagrange-3 fractional reads; 50 ms smoothing. At exactly 0 the line is
    bypassed.
  - At a half-sample position and 48 kHz the read loses 0.2 dB at 8 kHz, 0.5 dB at 10 kHz and 2.5 dB at
    15 kHz [P]: above a guitar cab's band, and it depends on the setting. Correlation's own shift is
    exact (sinc, in the kernel).
  - This delays the wet as a mic moved back would. It is a setting, not a latency, and is not reported.
- **Cuts** on the summed wet: `lowcut` 10 – 500 Hz and `highcut` 1 – 22 kHz, with `lowcut_slope` and
  `highcut_slope`. These are gfx-eq's four ids, ranges and stops (gfx-geq took them too): the common
  post-cab setting is 12 dB low and 24 dB high, which one shared slope could not give.
  - Butterworth, 12 or 24 dB / oct, from gfx-dsp `svf` with f64 state.
  - At the end stops (10 Hz, 22 kHz) the section is bypassed exactly.
- **`bass_keep`** (shared id), 0 – 300 Hz.
  - Below it, the output is the **dry** input instead of the cab: LR4 from the gfx-dsp `crossover`
    `Split` (gfx-bassdi's module).
  - The wet is high-passed and the dry low-passed by the same split, at unity.
  - The sum is flat only for IR = δ. With a real cab, the dry lows and the cab's highs meet at the
    crossover with the cab's own phase, as in the hardware it copies; `align` Onset keeps the wet's
    delay out of it.
  - Uses: bass lanes (the cab acts only above the lows, as in Parallax) and 7-string chugs.
- **`dry`**, −60 … +6 dB, −60 = off: a parallel DI (bass DI plus cab, an acoustic body IR under the
  piezo).
  - The dry is not delayed. The wet starts at h[0], so they stay aligned in Raw.
  - In Onset mode the wet's leading delay is removed, which aligns it best.
- **`level`**: the wet output, as x42's "Wet Signal Level".
- **2 / 2 layout**, `stereo`:
  - **Linked**: the same blend on both channels. A 2-channel file plays L / R.
  - **Split**: the left input through A to the left output, the right input through B to the right
    output: the classic two-cab wide sound. `blend` becomes the balance: g_A = min(1, 2 · (1 − blend)),
    g_B = min(1, 2 · blend), so both are at unity at 50 %. With `ir_b` none, Split plays as Linked.

### 2.6 Latency, CPU, memory

- **Latency**: 0 reported, 0 true. The head FIR's tap 0 multiplies the current input sample.
- **CPU** [A, flop model: real FFT ≈ 2.5 N log₂ N flops, complex MAC 8 flops, head 2 flops per tap].
  Per mono kernel:

  | IR | 44.1 kHz | 48 kHz | 96 kHz | 192 kHz |
  |---|---|---|---|---|
  | Cab 200 ms | 759 flop / sample · 33 Mflop/s | 759 · 36 | 775 · 74 | 877 · 168 |
  | Cab 500 ms | 783 · 35 | 783 · 38 | 885 · 85 | 933 · 179 |
  | Cab 2 s (class limit) | 925 · 41 | 933 · 45 | 1,029 · 99 | 1,213 · 233 |
  | Spring 4 s (gfx-irfx) | 1,013 · 45 | 1,029 · 49 | 1,213 · 116 | 1,589 · 305 |
  | 20 s (class limit of gfx-irfx) | 1,701 · 75 | 1,781 · 85 | 2,717 · 261 | 4,590 · 881 |

  - The figures include the forward transforms of the levels the IR does not reach (§2.3): 689 → 759
    for the 200 ms cab. The first draft's figures were reproduced exactly by the review's model without
    them [P].
  - At ~5 Gflop/s of sustained SIMD FFT / MAC throughput on one core [I], one 200 ms cab is ~0.7 % of a
    core at 48 kHz and ~3.4 % at 192 kHz.
  - A second IR (B) shares the head's input, the forward transforms and the FDL, and adds 64 – 71 % for
    a cab [P].
  - Stereo doubles everything.
  - A two-IR stereo cab at 192 kHz stays near 11 %. A 4 s stereo spring at 48 kHz is ~2 %.
  - Worst chunk: 2 – 4× the mean (§2.3).
  - Bench targets are in §5.
- **Memory.**
  - A kernel holds ≈ one complex value per IR sample: 8 bytes · L. A 200 ms cab at 48 kHz is 77 kB; a 4 s
    spring is 1.5 MB per channel.
  - The FDL is the same size, preallocated at `initialize()` for the class limit:
    - gfx-cab: 2 s, 0.8 MB per channel at 48 kHz;
    - gfx-irfx: 20 s, 7.7 MB per input channel; 61 MB at 192 kHz [A].
  - Kernel cache: kernels in use, plus up to 64 MB of unused ones, per process (§2.1).
- **Load time** for a 200 ms cab: ~5 – 20 ms on the background thread [A, order of magnitude], then the
  per-level crossfades of §2.4.

### 2.7 The built-in IR

gfx-cab's `ir_a` slot 0 is a **generic closed-back 4×12 voicing of our own, designed, not derived from
files** (decision 5).

- It is a parametric curve: about ten gfx-dsp `matched` sections (a 2nd-order high-pass near 80 Hz, the
  cab resonance, the low-mid body, the 2 – 4 kHz presence, the cone break-up dip, a 4th-order roll-off
  from 5 kHz). The same method as gfx-bassdi's `cab` voicings.
- It is rendered at `initialize()`, at the host's rate, into a 100 ms impulse response: the sections are
  minimum-phase already. No table, no resampling, no file, and nothing in the repository that comes from
  somebody's capture.
- Its level follows Loudness's reference (+14 dB), so slot 0 and a library cab match.
- Library cabs serve as a listening reference only. Averaging their magnitudes into a shipped table was
  dropped: the files come from vendors' and community packs whose terms golem-fx has not cleared, and a
  GPL repository should not have to argue that a smoothed average is not a derivative.
- What it is for: an inserted Cab is never a raw amp. It is a smooth curve, without a real cab's comb
  and resonances; the miked cab is slots 1 and up.
- For bass, a preset picks a bass cab IR, or gfx-bassdi's IIR cab is used.
- Its voicing is [I] until heard: The maintainer's ear.

### 2.8 gfx-irfx: the stereo IR effect

- **Same engine**, class limit 20 s, layout 2 / 2 first, 1 / 1 second. golem turns the lane stereo
  there, as the catalogue note asks ("put it on a stereo lane").
- **Channel use, by the file's channel count**:
  - 1 ch: the same IR on each channel;
  - 2 ch: in L → ch 1 → out L, in R → ch 2 → out R. A mono DI on both inputs therefore gives a true
    mono-to-stereo spring or echo;
  - 4 ch: true stereo, **not in v1**. The 108 files at hand have 1 or 2 channels [P], and the channel
    order (LL, LR, RL, RR is the common convention [I]) cannot be checked without a file. Until a pack
    needs it, a 4-channel file plays its channels 1 and 4 (L → L, R → R), and the engine keeps room for
    four kernels per instance.
  - On 1 / 1, the outputs are averaged.
- **No echo cut, ever.** golem copies `ir-fx` files whole. `length` (default 20 s = whole) and `fade`
  are the user's choice. The longest effect IR in golem's packs today is 9.1 s [P].
- **Extras** for effect IRs:
  - `predelay` 0 – 250 ms, a delay line on the kernel input;
  - `reverse` (reversed IR: swells; the kernel is rebuilt);
  - `width` 0 – 200 %, mid / side on the wet;
  - `lowcut`, `highcut` and their slopes, as gfx-cab;
  - `trails`.
- No `ir_b`, no `align`, no `min_phase`: a reverb's or an echo's phase and pre-delay are the sound.

## 3. Parameters

### Ids and units

- `level` (gfx-cab), `wet` and `dry` (gfx-irfx) keep their catalogue ids, **units (dB) and meaning**.
  They are wet and dry levels, with linear dB ranges, a little wider than the catalogue's (contract).
- Names: "Level", "Wet", "Dry". golem joins them through the engine table's `params` (R2).
- Slots are `IntParam`s. Enums are append-only.
- No default depends on another parameter.
- `lowcut`, `highcut`, `lowcut_slope`, `highcut_slope` are gfx-eq's ids, with its ranges and stops.
- What differs per golem block or per file (`normalize` Off for a "pedal" capture or a converted chain)
  is set by golem at insert, as the contract says.

### gfx-cab: front panel

| id | Name | Type | Range | Catalogue | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `level` | Level | f | −24 – +12 dB | −12 – +12 dB | 0 dB | 20 ms | ✓ | Wet output level (x42's "Wet Signal Level") |
| `blend` | Blend | f | 0 – 100 % | new | 0 % | 20 ms (EXP rule) | ✓ | A ↔ B, linear. The "mic move" pedal. Ignored when B is none |
| `lowcut` | Low Cut | f | 10 – 500 Hz | new | 10 Hz (off) | 30 ms | | Tighten proximity boom |
| `highcut` | High Cut | f | 1,000 – 22,000 Hz | new | 22,000 Hz (off) | 30 ms | ✓ | Fizz control; a dark lead sweep |
| `align` | Align | e | Raw · Onset · Correlation | new | Onset | rebuild | | Leading-delay trim and auto-polarity; B aligned to A by correlation. With one IR, Correlation = Onset |
| `ir_a` | IR A | int | 0 – 65,535 (0 = Built-in) | (file) | 0 | rebuild, at once | | Slot of the main IR (§2.1). Set by golem's picker, not a knob |
| `ir_b` | IR B | int | 0 – 65,535 (0 = none) | new | 0 | rebuild, at once | | Slot of the second IR |

### gfx-cab: expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `level_a` | Level A | f | −24 – +12 dB | 0 dB | 20 ms | | Trim of A after normalisation |
| `level_b` | Level B | f | −24 – +12 dB | 0 dB | 20 ms | | Trim of B |
| `polarity_b` | Polarity B | b | off / on | off | 5 ms crossfade | | Invert B (on top of auto-polarity) |
| `delay_b` | Delay B | f | −10 – +10 ms (0.01 ms display) | 0 ms | 50 ms | | Fine phase: + delays B, − delays A |
| `length` | Length | f | 5 – 2,000 ms | 2,000 ms (whole) | rebuild, debounced 100 ms | | IR trim; ≥ the IR's length = untouched |
| `fade` | Fade | f | 0 – 100 % | 10 % | rebuild, debounced 100 ms | | Half-Hann fade over the end of a trimmed IR |
| `normalize` | Normalize | e | Off · Peak · Loudness | Loudness | rebuild | | Level match between IRs, at the level of a typical cab IR played raw (§2.2.7) |
| `min_phase` | Min Phase | b | off / on | off | rebuild | | Minimum-phase transform (tight, no pre-delay, easy blends). Replaces the onset trim and the auto-polarity |
| `lowcut_slope` | Low Cut Slope | e | 12 dB · 24 dB | 12 dB | 20 ms crossfade | | As gfx-eq |
| `highcut_slope` | High Cut Slope | e | 12 dB · 24 dB | 12 dB | 20 ms crossfade | | As gfx-eq |
| `dry` | Dry | f | −60 – +6 dB (−60 = off) | −60 dB | 20 ms | | Parallel DI |
| `bass_keep` | Bass Keep | f | 0 – 300 Hz (0 = off) | 0 Hz | 30 ms | | Shared id: below it the dry plays instead of the cab |
| `stereo` | Stereo | e | Linked · Split | Linked | 20 ms crossfade | | 2 / 2 only: same blend both sides, or A left / B right (as Linked while B is none) |
| `bypass` | Bypass | b | off / on | off | 10 ms | | `kIsBypass` |
| `loaded_a`, `loaded_b` | Loaded A / B | f, read-only | −2 – 65,535 | — | — | | The requested slot once installed; −1 missing, −2 unreadable or hash mismatch; the previous value while pending (§2.1) |

Expert groups: **IRs** (`level_a`, `level_b`, `polarity_b`, `delay_b`), **Shape** (`length`, `fade`,
`normalize`, `min_phase`, `lowcut_slope`, `highcut_slope`), **Output** (`dry`, `bass_keep`, `stereo`).

### gfx-irfx

| id | Name | Type | Range | Catalogue | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `wet` | Wet | f | −60 – +6 dB | −30 – +6 dB | −6 dB | 20 ms | ✓ | Effect level |
| `dry` | Dry | f | −60 – +6 dB | −60 – +6 dB | 0 dB | 20 ms | | Insert dry, at unity |
| `lowcut` | Low Cut | f | 10 – 1,000 Hz | new | 10 Hz (off) | 30 ms | | Thin a spring's boom |
| `highcut` | High Cut | f | 1,000 – 22,000 Hz | new | 22,000 Hz (off) | 30 ms | ✓ | Darken the tail |
| `ir_a` | IR | int | 0 – 65,535 (0 = none) | (file) | 0 | rebuild, at once | | Slot |
| `predelay` | Predelay | f | 0 – 250 ms | new | 0 ms | 50 ms | | Gap before the effect |
| `length`, `fade` | Length, Fade | f | 10 – 20,000 ms; 0 – 100 % | new | 20,000 ms; 10 % | rebuild | | Shorten a tail with a fade |
| `normalize` | Normalize | e | Off · Peak · Loudness | new | Loudness | rebuild | | Same law, 60 Hz – 12 kHz, reference 0 dB: `wet` is the effect's level against the dry (§2.2.7) |
| `reverse` | Reverse | b | off / on | new | off | rebuild | | Reversed IR |
| `width` | Width | f | 0 – 200 % | new | 100 % | 20 ms | | Mid / side on the wet |
| `lowcut_slope`, `highcut_slope` | Low / High Cut Slope | e | 12 dB · 24 dB | new | 12 dB | 20 ms crossfade | | As gfx-eq |
| `trails` | Trails | b | off / on | new | on | — | | Bypass lets the tail ring |
| `bypass` | Bypass | b | off / on | — | off | — | | `kIsBypass` |
| `loaded_a` | Loaded | f, read-only | −2 – 65,535 | — | — | — | | As gfx-cab |

### Visual

- **gfx-cab: category `cab`** (the catalogue's). `METAL_HUE` has no `cab` entry yet, so it gains
  **`METAL_HUE.cab = '#00a898'`**, a teal (hue 174°), decided at the review.
  - CIEDE2000 distances [P]: 14.9 to filter's cyan `#00a8c0`, 17.9 to modulation's green `#00a84f`, 25.6
    to gfx-tuner's `#23bf18`, 5.7 to golem's own cab tile (`CATEGORY_HUE.cab` `#1ab7b0`).
  - The first proposal, `#00a88a` (169°), sat at 13.4 from modulation and 19.8 from filter. Five degrees
    towards cyan balance the two and bring the pedal closer to its tile.
  - The family already lives with closer pairs (eq / delay 12.5, delay / reverb 5.9), and the motif
    differs from gfx-wah's and gfx-autowah's. The maintainer confirms on the family sheet.
- **Finish**: anodized (the family default).
- **Motif `cone`** (new, in `motifParts`): a loudspeaker seen from the front, **with the rune as its dust
  cap**.
  - The first drawing put quarter-circles on the lower corners. That is the switch plate (y from H − 40,
    with its four screws and the "golem fx" lettering): motifs live in the rune zone, as `crescent` and
    `bins` do.
  - Centred on the rune (`g.x`, `g.y`), on each side: arcs of radius 34, 40, 46 and 52 over ±28° about
    the horizontal (`arcPath`). The outermost is dashed (2 on, 2 off) as the surround; the others at
    opacity 1, .8, .6.
  - Three radial ribs per side, from radius 34 to 52 at 0° and ±18°, at 40 % opacity.
  - Extent on the 112 × 200 face: x 4 … 108, y 88 … 136. It clears the cartouche (56 wide), the knobs
    (they end by y 72), the single selector at the centre, and the name at y 155.
  - Drawn with `line()` and `arcPath`, so it works engraved in anodized and in thrash.
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-cab": {"name": "Cab", "blocks": ["cab"], "knobs": ["level", "blend", "lowcut", "highcut"],
              "selectors": ["align"], "category": "cab", "color": "#00a898"}
  ```
- **Glyph: keep golem's `cab` glyph** (a square cab with one speaker). It is the block's tile, and the
  rune links the two.
- **gfx-irfx**: category `reverb` (the catalogue's `ir-fx`), body `METAL_HUE.reverb` `#6a1fd1`,
  anodized, no crate hue.
  - **Motif `impulse`** (new): on each side of the rune, an impulse response drawn as bars standing on
    the line `g.y + g.ch / 2 + 6` (the baseline of `bins`), outside the cartouche. One tall spike at the
    inner end, then bars every 4 units whose heights decay exponentially (×0.72 per bar), with a fainter
    echo cluster after a gap, as many as fit to x = 7 and W − 7. It reads as an "IR plot" and differs
    from delay's `echo` and reverb's `rays`. Not on the bottom edge: that is the switch plate.
  - The catalogue's own colour for `ir-fx` (`#3d9be9`) is not used: the pedal follows its category.
  - Face: `"gfx-irfx": {"name": "IR", "blocks": ["ir-fx"], "knobs": ["wet", "dry", "predelay",
    "highcut"], "toggles": ["trails"], "category": "reverb"}`.
  - **Glyph `ir-fx.svg`** (new; golem has no SVG for it). A spike, then decaying taps, then a late echo:

    ```
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>IR effect (gfx)</title><path d='M2.5 20.5h19'/><path d='M5 20.5V3.5'/><path d='M8.5 20.5v-9M12 20.5v-5.5M15.5 20.5v-3' opacity='.7'/><path d='M19.5 20.5v-6' opacity='.45'/></svg>
    ```
- No maker's livery, logos or trademark names on either pedal.

## 4. Applications

Presets carry **settings, not IRs**: a slot is local to one machine, and the IR is golem's choice
(picker, or golem.fm's AI choosing a library IR). The presets below set the shape around the IR. They
go in `presets/gfx-cab.json` and `presets/gfx-irfx.json`, with no artist or trademark names. The
gfx-irfx levels below assume `normalize` Loudness (0 dB reference).

| Use / preset idea | Settings to start from | Feature that matters |
|---|---|---|
| **Single close mic** (the default) | `align` Onset, `normalize` Loudness, everything else neutral | Zero latency, onset trim, level-matched swaps |
| **Two-mic blend** (dynamic + ribbon of one cab) | `ir_b` set, `blend` 35 %, `align` Correlation | Correlation align + auto-polarity: no comb |
| **Session-true blend** (two mics captured together, keep their real distance) | `align` Raw, `blend` 50 %, tweak `delay_b` ±0.1 ms by ear | Raw keeps the recorded phase; 0.01 ms fine delay |
| **Mic sweep on EXP** | `ir_b` = the same speaker off-axis, EXP on `blend` 0 → 100 % | Smoothed blend: a two-point "mic move" |
| **Tight metal rhythm** | `lowcut` 90 Hz, `highcut` 7.5 kHz, `highcut_slope` 24 dB | Proximity boom and fizz cut after the IR |
| **Wide double cab** (2 / 2) | `stereo` Split, A and B = two different cabs, `blend` 50 % | Two cabs left / right from one amp |
| **Bass cab with DI** | `bass_keep` 120 Hz, or `dry` −6 dB with `level` −3 dB | Lows stay DI, the cab only above |
| **Acoustic body** (piezo → body IR) | `dry` −10 dB, `lowcut` 70 Hz, `length` whole | Parallel dry, long IR |
| **Small speaker / phone** | A small-speaker IR, `lowcut` 300 Hz, `highcut` 4 kHz | Cuts on the wet |
| **Room cab** (far mic, long IR) | `length` whole (500 ms+), `normalize` Loudness | No forced truncation (Helix-style 42 ms would lose the room) |
| **Tight MPT cab** (blend IRs from different vendors) | `min_phase` on (it applies to both), `align` Correlation | Minimum phase: blends without combs, as Fractal's MPT |
| **Short IR** (save CPU on a small board) | `length` 40 ms, `fade` 20 % | Trim with a fade, no ripple |
| **Scene cab change** (clean 2×12 → lead 4×12) | A different `ir_a` per scene | Cached kernels, per-level crossfade: the attack has changed within ~40 ms, no gap |
| gfx-irfx **Spring tank** | Spring IR, `wet` −6 dB, `dry` 0 dB, `lowcut` 150 Hz, `trails` on | Whole IR, stereo, trails |
| gfx-irfx **Tape echo capture** | Echo IR, `wet` −8 dB, `highcut` 6 kHz | Whole IR (echo = the sound); no feedback control (as golem's note) |
| gfx-irfx **Reverse swell** | Plate or hall IR, `reverse` on, `predelay` 0, `wet` −10 dB | Reversed IR |
| gfx-irfx **Short room for voice** | Room IR, `length` 600 ms, `fade` 30 %, `wet` −14 dB, `width` 80 % | Trimmed tail, width |

## 5. Bench suite (on top of gfx-bench's common checks)

**gfx-bench needs one addition.** The `Effect` trait has no file input. gfx-cab's DSP struct exposes
`load_ir(slot, channels: &[Vec<f32>], rate)`, which prepares synchronously and is for the bench only.
A bench option `ir_fixtures` names WAVs in `gfx-bench/fixtures/ir/` (our own synthetic IRs and a few
freely licensed cabs). The common checks run on the **built-in** IR. The plumbing rows (index, meters,
background loads, activation) are integration tests of the plugin crate: gfx-bench's `Effect` has no
meters and no thread.

- **Options**:
  - `skip_latency`: the built-in IR is minimum-phase with a few samples of group delay, so the correlation
    peak is not at 0. The "Dirac" row below measures the true 0. This is the gfx-drive precedent.
  - gfx-irfx: `stereo_by_nature`, and `silence_floor_db` −60 with the fixture spring. Its tail is the
    effect, the gfx-delay precedent.

| Check | Pass |
|---|---|
| Dirac | IR = δ, `align` Raw, `normalize` Off, everything else at its default: output **equal to the input, sample for sample** (head tap 0 = 1, empty partitions skipped, cuts and delay bypassed at their stops). δ delayed by 37 samples: output = input delayed by 37 exactly |
| Accuracy | Random IRs 0.2 s, 2 s, 20 s (gfx-irfx) against an f64 direct / FFT reference: error ≤ −110 dB re output RMS |
| Block-size independence | Parameters still. Host blocks 1, 7, 64, 1,000, 4,096 and a seeded random mix: outputs bit-identical, with one and with two kernels, and across a swap injected at the same chunk |
| Schedule | A debug counter: no level's block is ever read before it is complete, over 60 s, every class length and rate |
| Zero latency | Impulse in at sample 0 → output[0] = h[0]; first block of 1 sample already correct |
| CPU (48 kHz, mono, one IR) | 200 ms cab ≤ 1 % of a core; with B ≤ 1.7 %; 192 kHz two IRs stereo ≤ 12 %; 4 s spring stereo 48 kHz ≤ 3 % |
| CPU spikes | 64-sample host blocks; 200 ms cab, 2 s cab, 4 s and 20 s IRs; 48 and 192 kHz: worst block ≤ 4× the mean and ≤ 5 % of the block's duration at 48 kHz. Reported with whole FFTs too (model: 8 – 16×) |
| Split transforms | Levels 4 and 5: forward then inverse = identity to f32 rounding; spectra × kernel = the whole-FFT result (≤ −130 dB) |
| No allocation in process | nih-plug `assert_process_allocs` over a session with 50 IR swaps, scene changes and a sample-rate change |
| Swap | 1 kHz sine and a guitar DI, `ir_a` changed every 500 ms between two fixtures: no step > −60 dB re signal; each level's crossfade lasts 30 ms ± 1 ms and starts within 2B of the kernel's arrival; output never drops more than 3 dB below both IRs' levels (1 dB between two captures of one cab) |
| No cold start | A swap at t, from a 100 ms IR to a 2 s one (20 s in gfx-irfx): at every level, the new kernel's output from its first block on = a full convolution of the whole input history (error ≤ −110 dB) |
| Failures keep playing | Unknown slot, missing file, hash mismatch, corrupt WAV: previous IR keeps playing, `loaded_*` = −1 / −2 in ≤ 100 ms; no panic. The previous slot written again: meter back at once, no load. A slot written 1 s before its index entry: loads within 0.5 s of the entry |
| Load time | 1 s 96 kHz stereo IR at 48 kHz host: kernel ready ≤ 50 ms on the bench machine; built-in ready at `initialize()` |
| Activation | Deactivate / activate × 20, and a state load with the same slots: no IO, no FFT (cache), processing within the first block. golem's `check-block-stall.py --rig`: no stall |
| Render determinism | Slots set before `initialize()`: two renders identical from sample 0, at two block sizes. A slot written while running: identical from the sample `loaded_*` = slot is read |
| Shared cache | Two instances on one IR: one preparation, one kernel in memory; removing one instance leaves the other playing |
| Resampling | 44.1 → 48: magnitude ±0.05 dB to 20 kHz against the analytic response of a synthetic IR. 96 → 48: aliases ≤ −100 dB. 48 → 192: no image above 24 kHz > −100 dB. 48 → 44.1: ±0.05 dB to 19 kHz |
| Onset / polarity | Synthetic B = −A delayed 1.37 ms: Correlation recovers the lag within 0.01 ms and the sign; 50 / 50 blend within 0.5 dB of 2·A up to 5 kHz. Onset keeps 0.5 ms before the −20 dB point and leaves the first peak untouched (bit-equal from 0.25 ms before it) |
| Polarity on real files | Captures of one cab from golem's library, in pairs: auto-polarity never gives the two files of a pair opposite signs (prototype: 209 / 209); Correlation never flips one |
| Minimum phase | 1/12-octave magnitude within 0.1 dB of the original; cumulative energy ≥ the original's at every sample |
| Normalisation | The cab IRs of golem's library, Loudness: pink-noise loudness spread ≤ 1 LU; the median gain applied is 0 ± 0.5 dB (it re-measures G_ref = +14 dB; prototype: raw spread 9.4 dB, median +14.1 dB over 85 files). gfx-irfx: the same on the effect IRs, G_ref 0 dB |
| Length / fade | A 200 ms IR cut at 40 ms with a 20 % fade: no ripple above the smoothed magnitude by more than 0.5 dB above 200 Hz |
| `delay_b` sweep | ±10 ms in 2 s under a sine: no click (common click bound), smooth comb motion. Static, at any setting: B's magnitude within 0.5 dB to 10 kHz at 48 kHz |
| `bass_keep` | Sum of dry-low + wet-high flat to ±0.1 dB when the IR = δ; LR4 phase as gfx-bassdi |
| Index plumbing | The test index via `GOLEM_FX_IR_INDEX`: slot write → `loaded_a` = slot. Slots 1, 2, 255, 256, 32,767, 32,768, 65,534, 65,535 written through the normalised value on both engines: each read back exact. Index rewritten 1,000 times (rename) during loads: every load sees a whole file. Copied session without the index → persisted path loads; index and state disagree → the index wins |
| gfx-irfx channels | 1 and 2-channel fixtures: L / R mapping matches a reference; 1 / 1 averages; a 4-channel file plays channels 1 and 4 |
| Listening | Same IR through x42 (raw, `normalize` Off) and gfx-cab: null ≤ −80 dB (engine parity). Blends by ear on the bench DIs; a scene change mid-phrase |

## 6. Positioning

| Pedal | Its cab | Difference |
|---|---|---|
| gfx-bassdi `cab` (Smooth, 8×10…) | IIR, 1/3-octave magnitude of a cab type, no CPU, part of the DI | gfx-cab plays a **specific miked cab** in full detail (comb, resonances, room). Rule: gfx-bassdi `cab` Off when gfx-cab follows (bassdi's own doc says so) |
| gfx-eq | Tone after the cab | gfx-cab's cuts are the cab's own; a full EQ goes after |
| gfx-reverb Spring | Algorithmic spring, playable, modulated | gfx-irfx plays a **capture** of a real tank: static, exact |

Against the field: Helix's IR length limit, QC's 1 ms phase steps and x42's thread timing are avoided.
NadIR's and PULSE's blend tools are matched (auto-polarity, auto-align, fine delay, per-IR level), and
so is Fractal's FullRes zero-latency length (64 K points ≈ 1.37 s at 48 kHz; ours 2 s, 20 s for effects).
What no surveyed loader states: a swap that neither clicks nor starts cold, and a level law measured on a
library.

Not attempted:
- DynIR-style many-position morphing; it needs many captures per cab.
- Per-IR EQ; gfx-eq is the rack's job.
- Exporting a blended IR (decision 6).
- Linear-phase cuts, which would add latency.

## 7. Reuse from gfx-dsp

| Piece | From | Use |
|---|---|---|
| `realfft` dependency | gfx-dsp | All FFTs |
| `svf` (+ f64 state) | gfx-dsp | Cuts |
| `delay::DelayLine`, `Interp::Lagrange3` | gfx-dsp | `delay_b`, `predelay` |
| `smooth`, `exp_smooth` | gfx-dsp | Gains, blend (EXP), cuts |
| `crossover::Split` (LR4) | **Not in gfx-dsp yet**: gfx-bassdi and gfx-grit are designs, and the split lives inline in `flanger.rs`. The first of the three to be coded extracts it | `bass_keep` |
| `matched` | gfx-dsp | The built-in voicing's sections (§2.7) |
| `util::FtzGuard`, `db_to_gain` | gfx-dsp | — |
| **New `convolve`** | — | Head FIR + non-uniform staggered levels, split transforms, shared FDL, N kernels per FDL, per-level crossfade. Users: gfx-cab, gfx-irfx. (gfx-reverb's design skips convolution, and gfx-bassdi sends its user to this block.) |
| **New `ir`** | — | Offline: `resample`, `onset`, `align_correlation`, `min_phase`, `normalize`, `trim_fade`, `partition`. Pure f64, testable without a host |
| New crate dependencies | — | `hound` (WAV, Apache-2.0), `rtrb` (lock-free SPSC, MIT / Apache-2.0), `sha2` (MIT / Apache-2.0), `serde_json` (index; already in nih-plug's tree). All GPL-3 compatible. `hound` reads no AIFF, RF64 or 64-bit float: golem's cached copy is always a WAV it can read (R1) |

## Open questions: decisions of the review

1. **One crate or two: two.** `gfx-cab` (1 / 1 first) and a thin `gfx-irfx` (2 / 2 first), both on gfx-dsp
   `convolve` and `ir`. The descriptor, the faces, the bench and golem's engine table are one class per
   crate, and the two blocks differ in layout, limits, panel, category and normalisation reference.
   Everything shared is in gfx-dsp; a third crate for the plumbing (index, cache, mailbox) is not worth
   it: it goes in a `gfx-dsp` feature `ir-host` that only these two enable, so the other effects do not
   pull `hound`, `sha2` and `serde_json`.
2. **The plumbing (§2.1): sound on both engines, kept.** Slot parameter + index is the only route that
   needs no fork binding in the Golem Engine and no state reload in REAPER; both claims it rests on were
   read in the sources. Amended: single writer, the index wins over the state, immediate loads, a
   process-wide cache, synchronous load in `initialize()`, no offline wait, meter semantics. For
   golem-rack to accept: R1 – R8. The persisted-path fallback is not written through `vst_chunk`.
3. **`normalize` default: Loudness, at the library's own level.** G_ref = +14 dB for gfx-cab (a typical
   cab does not move), 0 dB for gfx-irfx (`wet` means what it says). golem pins Off for converted chains
   and for "pedal" captures. The owner may prefer unity (0 dB) for gfx-cab: it is the purer law, and it
   costs 14 dB of make-up everywhere after the cab.
4. **Class limits: 2 s and 20 s, kept.** The longest effect IR at hand is 9.1 s; 20 s costs 15 MB of FDL
   per stereo instance at 48 kHz, 61 MB at 192 kHz, which golem does not run. Two library cab files
   exceed 2 s (noise floor): they are cut with the fade.
5. **The built-in IR: designed, not averaged** (§2.7). No file-derived data in the repository.
6. **Bake the blend: not in v1.** It saves about 40 % with two IRs, on a block that costs 1 %.
7. **Offline wait: dropped** (§2.1, point 6). The mode cannot be known under nih-plug on either engine.
8. **4-channel order: deferred with 4-channel support** (§2.8).
9. **Hue: `#00a898`.** Motifs `cone` and `impulse` moved to the rune zone. The `ir-fx` glyph is kept as
   drawn.
10. **Time-distributed big FFTs: in v1** (§2.3). B_max stays 8,192.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good**, and Strong once gfx-bench reproduces the schedule and the split transforms. The two
ideas that carry the design hold: the slot-and-index plumbing is the right answer to "no string
parameter, two hosts, no GUI", and the partition schedule is feasible at any block size with true zero
latency. The engine as first written would have missed three of its own claims (CPU peaks, no cold
start, offline determinism), and the IR preparation had one rule that breaks the blends it exists for.
All are fixed in the text.

Verified in sources: Ardour's `set_plugin_insert_property` is LV2-only (`lua_api.cc`, `plugin.h`);
nih-plug's state load re-runs `initialize()` and `reset()`; `execute_background` is a `try_send` on a
bounded queue to one worker per class; the catalogue's ranges and defaults for `cab` and `ir-fx`;
zeroconvolv's README; Fractal's FullRes figures.

Prototype (numpy; scripts in the session scratchpad, `proto-gfx-cab/`: `nupols.py`, `cost.py`,
`irstats.py`, `pol.py`): the schedule simulated sample by sample; the flop model per chunk; 108 distinct
IR files of golem's `assets/` (85 cabs, 6 pedal captures, 13 whole effects, 4 cut effects) for levels,
onsets, polarity and correlation; the resampler's response; the cepstral transform on four cabs.

Changes made, and why:

| # | Change | Why |
|---|---|---|
| 1 | **Large FFTs cut into sub-transforms from v1**; bound restated as ≤ 4× the mean and ≤ 5 % of a period (§2.3) | With whole FFTs the worst chunk is 8 – 16× the mean, not ≤ 3× [P]. Cut in 4 and 8: 2 – 4×. The method is written out, with bins left in sub-transform order so the MACs need no reordering |
| 2 | **The FDL runs for the whole class layout, always** | "CPU follows the IR's length" left the upper levels unfed under a short cab, so a longer IR swapped in started its tail cold: the headline claim was false in that case. Cost +10 %; the CPU table is recomputed (the draft's figures reproduce exactly without it) |
| 3 | **Crossfade per level**, each starting at the new kernel's first block (≤ 2B later) (§2.4) | A time-distributed level cannot produce a new kernel's block at once; "30 ms" for the whole IR needed a burst of large FFTs. Staggered, the attack changes in ~40 ms and each part is exact |
| 4 | Level 5's offset (4B) run as 2B with two empty partitions; stagger of 1, 2, 3 chunks stated; engine changes taken at chunk boundaries | The table's rule "offset ≥ 2B" did not say what an offset of 4B does; block-size independence needs the mailbox read at a chunk boundary |
| 5 | **Offline wait removed**; `initialize()` loads synchronously; golem waits for `loaded_*` before a render | nih-plug gives the mode of the previous `setupProcessing`, and Ardour's export never passes `kOffline` there. The audio thread would have waited in neither host |
| 6 | **Auto-polarity = sign of the first extremum at −12 dB**, not the sum of the first 2 ms | The draft's rule gave opposite signs to two captures of the same cab in 37 pairs of 209; the new one in none [P] |
| 7 | Onset: back-off 0.5 ms, fade over its first 0.25 ms | A 16-sample fade inside a 12-sample back-off attenuated the first peak |
| 8 | Preparation reordered: trim, minimum phase, **then** correlation; the leading IR is delayed; negative peak only above +1.5 dB; B's kernel depends on A | Minimum phase after the alignment undid it; an IR cannot be advanced; max \|r\| alone can pick an inverted alignment |
| 9 | **Loudness referenced to +14 dB** (gfx-cab) and 0 dB (gfx-irfx); pedal captures and converted chains pinned Off | Library cabs play 14 dB hot when raw [P]: a 0 dB target would have dropped every rig by 14 dB. Effect IRs play +16 … +28 dB raw, which is why `wet` −6 dB means little today |
| 10 | Slots load at once (debounce only for `length`, `fade`); cache per process, shared; index read at each load, single writer, index wins; retry on −1; slot recycling rule | A scene switch waited 100 ms for nothing; two lanes on one IR prepared and stored it twice; mtime tests miss rewrites; "permanent" had no answer to a full or lost registry |
| 11 | `loaded_*`: the requested slot once installed; −3 removed | "Compared for equality" and "−3 while it plays" contradicted each other |
| 12 | Cut ids → `lowcut`, `highcut`, `lowcut_slope`, `highcut_slope` with gfx-eq's ranges | Shared ids; one slope for both cuts cannot give 12 low / 24 high |
| 13 | Built-in IR designed from sections, rendered at `initialize()` | Licence, and no table to resample |
| 14 | 4-channel IRs deferred; Split law and inputs defined; `delay_b` high-frequency loss stated; `bass_keep` phase note; B always runs | Unverifiable order with no file; "blend becomes the balance" was not a law |
| 15 | **Visual**: hue `#00a898`; `cone` and `impulse` moved off the switch plate to the rune zone | The lower corners hold the plate, its screws and the lettering (`golem-pedals.js`) |
| 16 | Facts: "bit for bit on any machine" → block size and timing only; `crossover::Split` does not exist yet; gfx-reverb and gfx-bassdi are not later users; second IR adds 64 – 71 %, not ~60 %; zeroconvolv's zita kernel tagged [I]; content-address option added to the table | Accuracy |
| 17 | Bench: Schedule, Split transforms, Shared cache, Polarity on real files, slot round trip, index race rows; swap dip 1 → 3 dB; Dirac conditions | Each new mechanism has its check; a linear crossfade between unlike cabs dips more than 1 dB |

Checked and left as written: the head / level layout and B₁ = 64 at every rate; the shared-FDL idea;
the slot arithmetic in f32; the resampler (−104 dB, < 0.01 dB ripple [P]); the cepstral minimum phase
(≤ 0.05 dB [P]); linear blend; `level`, `wet`, `dry` ids, units and ranges; linear ranges; no
mode-dependent default; 1 / 1 first for gfx-cab and 2 / 2 first for gfx-irfx; `bypass` and `trails`; the
positioning against gfx-bassdi's IIR cab (Off when gfx-cab follows) and gfx-reverb's spring.

Coverage: NadIR, PULSE, IRDX and the hardware blocks are matched on what players ask for (two IRs,
auto-align, auto-polarity, fine delay, cuts, trim). Left out on purpose and rightly: per-IR EQ, more
than two IRs, DynIR-style morphing, baking. Nothing superfluous; `min_phase` and `length` are the narrow
ones and both answer a named use.

Still open:

| Point | Owner |
|---|---|
| Accept R1 – R8; the index path; who holds the file lock; isolation of test bridges by `GOLEM_FX_IR_INDEX` | golem-rack |
| Re-measure G_ref on the whole library (cab and effect sets), and list the "pedal" captures to pin Off | golem-rack |
| Unity (0 dB) or house level (+14 dB) for gfx-cab's Loudness | The maintainer |
| Converted `ir-fx` chains: pin Off, or convert `wet` by each file's measured gain (`ir-info`) | golem-rack |
| Fix in golem's nih-plug fork: store the new `process_mode` before `BufferConfig`, and read `ProcessData.processMode`. Not needed by this design; useful to others | The maintainer |
| Throughput (5 Gflop/s is [I]), f32 accuracy on 20 s IRs, the 4× bound on real hardware, split-transform cost against `realfft`'s own | Bench, first coding task |
| Longest outboard pack; a pack with 4 channels | golem-rack |
| The built-in voicing; hue and the two motifs on the family sheet | The maintainer |
| A noise-floor trim for vendor files that carry 1 – 4 s of floor | Later, if the CPU matters |
