# gfx-acoustic

> **Status: CODED** (v0.1.0, 2026-11-15). Designed and reviewed 2026-11-14 (Fable 5.2, Review at the
> end; figures marked **[P]** come from its prototype). The whole chain is built: the DI back to the
> EMF through the shared gfx-dsp **`pickup`** module (extended for this pedal: band limit
> min(12 kHz, 3 × resonance), the analog prototype, the position shelf; gfx-fuzz's tests and bench stay
> green), the position / aperture shelves, `top`, the rewritten **decay** stage, the 12-string courses
> (new gfx-dsp **`filterbank`**, gfx-detune's shifter), the **body** (≤ 48 TPT modes, two gains per mode
> and mic, one blended 1,024-tap FIR per channel through the new gfx-dsp **`fir`**, AVX2), the piezo
> voice and quack dip, `low_cut`, the one-knob Opto compressor (gfx-comp, colour off), the room, K,
> `mix`. **Two bodies are measured**: Classical from the Mores archive (guitar g34, CC BY 4.0) and
> Concert from the IEM / DEGA recordings (Public Domain Mark), both licences confirmed on the record
> pages at download; the other four are the parametric voicings. Bench green (28 / 28 at 44.1 – 192
> kHz); **CPU 255 – 277 ns per sample mono at the defaults** (≈ 1.3 % of a core, budget 300 ns), 310 –
> 332 ns per stereo frame; worst case (2 / 2, twelve, room, comp, model crossfades with `mic` moving)
> 1.27 µs per frame. The decay stage runs its **fallback 1** (cap 12 dB, k halved): the soft-note row
> failed with the rule as written (−8.7 dB, bound 8). Deviations and numbers: "As coded" below.

## As coded (v0.1.0)

**Files.** gfx-dsp: `pickup.rs` extended (additions only: `resonance_hz`, `acoustic_pole_hz`,
`emf_prototype`, `emf_response`, `position_corner_hz`, `position_shelf`, `STRING_SPEED`; + 3 tests;
gfx-fuzz's 13 tests and its bench, 28 / 28, unchanged), new `fir.rs` (direct-form FIR on a doubled ring,
AVX2 + FMA dot product with runtime detection and a portable fallback, kernel blend, windowed-sinc and
causal kernel resampling; 6 tests), new `filterbank.rs` (gfx-octaver's ERB bank, octave up; 3 tests).
Here: `lib.rs` (the processor, `PARAMS`, the voicings, source paths, 12-string, room), `body.rs` (the
body engine), `decay.rs`, `bodies.rs` and `data/bodies.f32` (generated), `plugin.rs` (class id
`GoleMFxAcoustic1`, mono 1 / 1 then 2 / 2, latency 0, read-only `gr`), `tests.rs` (17 behaviour tests,
1 timing), `tools/body_fit.py` (the offline fit, §2.6), `NOTICE`.

**Data (§1.4, §2.6).** `tools/body_fit.py --fetch DIR` downloads the archive's
`qualified_selected_impulses.mat` and the two Phaidra zips by URL, checks their SHA-256 and fits; only
the derived tables are committed (`src/bodies.rs`, `data/bodies.f32`: 6 models × 3 mics × 1,024 f32 taps
at 48 kHz, 72 KB). Licences checked on the day: Zenodo's record says `cc-by-4.0` (creator R. Mores);
Phaidra's records say Public Domain Mark 1.0 and the IEM project page says "All the contents are in the
public domain". The notice of §1.4 is in `NOTICE`, in `bodies.rs`' header and below; the guitar is
**g34** (a classical Spanish guitar of 1971, measured in the anechoic room of HAW Hamburg: no reflection
to window out; not g07, g30 or g62).

> Classical body: fitted from guitar g34 of R. Mores, *Archive for the acoustical documentation of
> classical Spanish guitars, flamenco guitars and romantic guitars from private and public collections —
> bridge mobility*, Zenodo, 2021, doi:10.5281/zenodo.4604577, CC BY 4.0
> (https://creativecommons.org/licenses/by/4.0/). Modified: windowed, converted and reduced to a modal
> table and a short filter by golem-fx.
>
> Concert body: fitted from F. Zotter and D. Rudrich, IEM / DEGA guitar recordings (Phaidra o:68022 and
> o:69273), Public Domain Mark 1.0. Modified: reduced to a modal table and a short filter by golem-fx.

- **Fit.** Vector fitting (common poles, 60 starting pairs over 45 Hz – 1.5 kHz, relative weights),
  poles kept at 50 – 1,400 Hz with Q clamped to 2 – 300, the 48 strongest, the mics' residues refitted
  without a constant term; the residual (measured − modal) LR4-high-passed at 1 kHz, faded 15 → 21 ms,
  1,024 taps. Mode gains = the residue as (band-pass, low-pass) pair (§2.5).
- **Classical**: poles from the two bridge accelerometers (hammer → acceleration, the three hits
  averaged), gains from hammer → mic (Bridge = mean of the treble and bass mics, Neck = upper east,
  Sound hole = their mean). The near-field tilt toward the IEM classical guitar's C414s (§2.6): one slope,
  **+0.64 dB / octave** (pivot 1 kHz), applied minimum-phase. 48 modes, 76 – 1,330 Hz. Body vs its data
  (1/6 octave, 70 Hz – 10 kHz): worst **3.3 dB** (bridge, 1.6 kHz) and 2.0 dB (neck); the ±1.5 dB of §5
  is not reached at the crossover (1.2 – 1.8 kHz) and below 90 Hz.
- **Concert**: session 1's six Taylor takes (37 s), piezo DI → C414 bridge / neck. **Deviation**: the
  coherence of a piezo with a mic over playing is low (median **0.17** at 80 Hz – 8 kHz; ≥ 0.8 on 7 % of
  the bins): a piezo sums six strings, each through its own saddle path, so one input explains the mic
  only in part, and H1 is biased low by √coh (the first fit came out 14 dB dark at 4 – 8 kHz on the
  held-out take). The magnitude is the spectral ratio √(Syy / Sxx) (= |H1| / √coh, 1/12-octave
  smoothed), its phase the minimum phase (the bulk delay goes with it). Room check, C414 vs NTI at the
  same spot below 300 Hz, the cardioid's proximity slope removed: no band over 3 dB, so every mode is
  kept. 48 modes, 52 – 1,242 Hz. Body vs its data: worst 4.2 / 4.7 dB (bridge / neck). **Held out**
  (session 2, MelancholiA + B, never fitted): piezo through the fit vs the C414s, 1/3 octave 80 Hz –
  8 kHz, levels aligned: worst **8.2 dB** (bridge; 1 – 3 kHz: 6.2 dB) and 8.3 dB (neck; 1 – 3 kHz:
  2.9 dB); the ±2.5 / ±1.5 dB of §5 are not met: with one input and six strings the long-term ratio
  moves with what is played (the two takes are different pieces).
- **Parametric** (Dreadnought, Jumbo, Parlor, Round-back): §1.3's mode lists ([U] Q and levels), Jumbo
  and Parlor scaled by body area (f × area^−½), per-mic gains by rule (the bridge mic hears less of the
  air mode, the neck mic less of the top); the high band synthetic (decaying noise, T60 70 → 20 ms from 1
  to 16 kHz, seeded), its level re the modes taken from Concert's measured sound hole (−23.5 dB re the
  modal peak), plus a direct impulse at tap 0 so the high band's largest sample comes first (§5 onset
  row).
- `rho` per pair (FIR blend correction): Concert 0.98 / 0.99, Classical 0.66 / 0.90, parametric 0.1 –
  0.3.

**Source (§2.1 – §2.2).** The inverse is gfx-dsp `pickup`'s analog prototype, **matched** at the host
rate (gfx-dsp `matched`): within **0.07 dB** of the prototype to 0.45 fs, every pickup, 44.1 – 96 kHz.
The prototype reproduces §2.1's lift table within 0.3 dB at 4 – 10 kHz (the plateau is 0.8 dB above the
table's, which rounds it from the prototype's 24 kHz end) and the invariant (prototype × network = the
Butterworth at f_p within 0.2 dB, 20 Hz – 16 kHz). Position shelf: Bridge − Neck **+11.4 dB at 50 Hz**,
within 0.3 dB of 0 above 3 kHz. A parallel pair uses the mean of the two positions (the mean velocity),
**not** the mean of the corners. Piezo: D(s) on the input. The wrong-guess table and its `top` trim
(§5 row 2) are not benched (they need the bench's noise and guess renders; left for the owner).

**Decay (§2.3).** As written: 4-band LR4 tree, 25 ms sliding maximum, 60 ms release, the rule every 8
samples, gain ramped, re-open on a 3 dB rise, 2 ms smoothing (a re-open is not smoothed: the pick is
never attenuated). Measured (synthetic plucks, E2 and A3, partials to 8 kHz, T60 4 → 1 s):
- with the rule as written (cap 24 dB, the README's k): T60 ÷ 1.39 / 1.78 / 2.17 / 2.63 (targets 1.4 /
  1.8 / 2.2 / 2.6; measured from 150 ms after the pick, before the cap), attack 0.0 dB, equal note back
  to 0.0 dB within 20 ms, but **a note 12 dB softer, 0.6 s after a loud one, lost 1.3 / 8.7 / 5.7 /
  0.2 dB** over its first 20 ms (bound 8 dB; prototype 3 – 7);
- so **fallback 1 runs** (`decay::CAP_DB` 12 dB, `K_SCALE` 0.5): T60 ÷ **1.20 / 1.40 / 1.59 / 1.76**
  (targets 1 + k/2: 1.2 / 1.4 / 1.6 / 1.8), attack 0.0 dB in the first 5 ms, gain never above 0 dB, the
  soft note **0.6 / 4.4 / 2.9 / 0.1 dB**, the equal note 0.0 dB at 20 ms, steady E2 ripple at its period
  ≤ 0.08 dB, `decay` 0 bit-exact (the tree bypassed after a 20 ms crossfade). Restoring the rule as
  written is two constants, after the listening row.
- **Noise (set here, §2.3 left it to the bench)**: r is held below −70 dBFS (−76 to enter) and while the
  band is stationary (its detector moved < 10 dB over the last 3.2 s, counted from the reset). Pink
  noise alone at −80 and −60 dBFS: gain change **0.00 dB** over 10 s, measured from 4 s (the window must
  first forget the noise's own onset; r frozen at what it reached).
- The bench's T60 rows measure each band's own output (band × its gain): re-splitting the summed output
  mixes the neighbours' differently shortened skirts into each band (it read 1.80 for 2.2).

**Twelve (§2.4).** gfx-dsp `filterbank` (gfx-octaver's ERB bank, the 13 bands below 400 Hz; octave
within 1 cent on single notes), the octave voice low-passed at 800 Hz, the unison above 150 Hz, a
`twelve_delay` line, gfx-detune's `CenteredShifter` (3 ms centre, 1 ms search, 50 ms grains). Measured:
**+4.00 cents** for 4 asked (1.5 kHz tone), onset **5.52 ms** after the input for 2.5 + 3 ms. On a pure
steady tone below ≈ 500 Hz the long-term pitch stays (a 1 ms search cannot skip a period longer than
2 ms; on playing the splices carry it, as in gfx-detune). After a cold enable the voices run 25 ms
unheard before their 20 ms fade (no onset of a delayed tone mid-fade); they keep running 250 ms after
"off". gfx-octaver keeps its inline copy of the bank (not touched).

**Body (§2.5).** TPT modes in f64 (a SoA loop), two gains per mode and mic, gains and coefficients
recomputed every 32 samples and ramped; `size` measured at f · 100 / size within 0.5 %. `mic` 25 / 75 %
is the 50 / 50 blend of its neighbours' modal outputs to 1e−9 (exact). The FIR is warmed with the
outgoing body's input history on a model crossfade (a cold kernel rang its high band: −31 dB on the
step check). **Kernel resampling**: windowed sinc at 44.1 kHz; **zero-stuffing** for 2× / 4× (88.2 /
176.4 kHz via 44.1): a band-limited interpolation of a kernel whose energy starts at tap 0 loses its
pre-ringing, and an anti-image filter's phase turns the FIR against the modes (2 dB at 2.5 kHz,
measured); the images above the old Nyquist only filter what the input holds there. The whole chain at
44.1 / 88.2 / 96 / 192 kHz vs 48 kHz, every model, 100 Hz – 12 kHz: within **0.39 dB**. Onset: first
output at sample 0; the response above 1 kHz peaks at **0.021 ms** (Classical 0.125 ms), energy
centroids 0.06 – 1.07 ms.

**After the body (§2.8).** Piezo voice: F̂ → D⁻¹ → +4 dB bell at 2 kHz (or the input for a piezo
source, crossfaded with the source paths), added as (mic + p · piezo) / √(1 + p²): `piezo` 0 → 100 %
moves the loudness by +0.46 dB (the level match constant is 0 dB). Quack dip: gfx-dsp `onset::Pick` on
the input, a −9 dB × `quack` bell at 2.2 kHz (Q 1.2) decaying over 30 ms. Compressor as §2.8 (gfx-comp's
engine, colour off, Opto; not run at 0: bit-transparent). Room: 12 taps (3.1 – 24.4 ms) per channel and
gfx-dsp's 16-line FDN at scale 0.3 (RT 0.35 s), input scaled by `room`, running 1 s after it reaches 0;
L / R decorrelation follows `width` (full from 40 %), so `width` 0 is L = R = the mono output bit for
bit (with room and comp on). `width` 80: low-band (< 500 Hz) correlation 0.98.

**Level.** K(model) on golem's DI proxy (gfx-fuzz's plucked notes, loudness after a 100 Hz high-pass):
Dreadnought −5.0, Jumbo −0.7, Concert −6.4, Parlor −0.7, Classical −5.2, Round-back −6.0 dB: every
model within 1 dB of the bypass at the defaults. **Deviation**: `body` 0 / 100 % moves the loudness by
−11.2 – −11.9 / +12.2 – +12.6 dB, not ±4 LU: the modal band carries most of the loudness (the row's
bound assumed a larger high band).

**Bench.** `skip_latency` (§5) and **`stereo_by_nature`** (2 / 2 is the two-mic pair: L ≠ R at the default
`width` 40 %; `width` 0 = mono is its own test). Click bound at the default −50 dB: worst −57 dB (`decay`
0 → 100), K glided over 10 ms on a model change (a 1 dB step of K was the first failure). Silence floor
−240 dBFS.

**Not done (for the owner).** Listening rows (§5): the decay fallback against the rule as written on
real arpeggios and melodies; the paired electric / acoustic phrases and the measurement campaign (the
four steel-string bodies stay parametric); the wrong-guess / `top` trim row; the chord rows of the
12-string (sum tones) and of the decay; the IEM room question (the C414 / NTI check found no band, but a
better reference is a dry steel-string measurement); voicings marked [U] (k_b, Q, the piezo bell, the
room). golem-rack: the `acoustic-sim` catalogue entry (category `amp`), `stacked_cab`, `pickup` /
`pickup_pos` at insert, `depends` (`quack` on Piezo is written in groups.json), the NOTICE on the about
page and in the bundle.

golem's **acoustic simulator**. An electric guitar's DI goes in; what comes out sounds like an acoustic
guitar recorded with a microphone. It also has a second job: an acoustic-electric's **under-saddle piezo**
goes in, and the mic sound comes out with the "quack" taken off. It is a new block with no catalogue
entry, so it has no ids to keep.

The idea is to **simulate the physics in the order it happens, and measure what cannot be computed**:

1. **Rebuild what the strings did.** The DI goes back to the pickup's EMF, through gfx-fuzz's source model.
   Then a filter set by the pickup's position estimates the force the strings would put on an acoustic's
   bridge (§2.2).
2. **Make the strings decay like an acoustic's.** A solid body keeps the strings' energy and an acoustic
   body drains it, so the strings' decay is shortened per band and per note, and a pick is never
   attenuated (§2.3). The 1999 study found long electric sustain to be the main thing that gives the trick
   away. A mono DI sets the limit: notes that share a band share one gain.
3. **The body is a measured modal bank plus a short high-band FIR.** Below about 1 kHz there are 30 – 60
   resonators. They are tunable, ring for up to a second, and cost little. Above 1 kHz, a short measured
   "noise-like" tail. Moving the **microphone** only changes the weights on the same resonators (§2.5).
4. **The body data come from sources we are allowed to use.** We measure the steel-string bodies
   ourselves, with the deconvolution method of the 1999 study (§2.6). The classical body comes from an
   open CC BY 4.0 archive of 65 classical guitars. Public-domain studio recordings of a piezo-equipped
   steel-string give the first measured steel-string body (Concert) and the piezo source's test.

Around the core sit a 12-string course generator (gfx-octaver's poly octave-up and gfx-detune's shifter),
a light compressor, a small room and a blend with the DI. **Zero reported latency.** Mono 1 / 1, with
2 / 2 offered as a two-microphone pair.

## 1. State of the art

Survey of 2026-11-14. **[M]** manufacturer manual or page · **[V]** read in a paper, dataset or source ·
**[F]** forum or review opinion · **[I]** inference, or not re-checked against a primary source ·
**[U]** not verified · **[A]** our calculation or design inference.

### 1.1 Hardware and plugins

| Reference | How it works | What we take |
|---|---|---|
| Boss AC-3 ([manual](https://cdn.roland.com/assets/media/pdf/AC-3_OM.pdf)) [M] | "COSM" modelling. Four modes: **Standard** (dreadnought), **Jumbo** (rich bass, "beautiful high end"), **Enhance** (sharper attack, not masked in a band), **Piezo** ("round-back acoustic guitar, with subdued body resonance and a unique attack"). Knobs: **Body** ("resonance of the sound produced by the guitar body… mellowness and breadth"), **Top** ("sense of attack and the harmonic content of the upper range"), Level, Reverb. The manual's sample settings use a **single coil, front pickup** and say: if it distorts, turn Top down | `body` and `top` as the two main tone knobs; a round-back model; the neck pickup as the reference (§2.2) |
| Boss AC-2; Line 6 Helix "Acoustic Sim" ([Helix 3.0 notes](https://kb.line6.com/helix-3-0-release-notes)) [M] | Helix's EQ > Acoustic Sim is "based on the BOSS AC-2", meant to be used alone or with an acoustic IR | Even the big modellers ship a small parametric filter set and leave the body to an IR |
| Fishman Aura Imaging pedals ([SOS review](https://www.soundonsound.com/reviews/fishman-aura-imaging-pedals)) [V] | **Piezo in, studio mic out.** Fishman records each instrument with condenser mics in a quiet studio and with its under-saddle pickup, then derives an "image": "more akin to a high-resolution EQ", "around two thousand bands". Six pedals (Concert, Orchestra, Dreadnought, Jumbo, 12-string, Nylon) with 16 images each; a Blend (reviewer: 40 – 50 % best); about 3 ms through the system, the dry delayed to match. "Imaging, not modelling" ([Fishman](https://www.fishman.com/?p=25150)) [M] | Piezo restoration as the second source (§2.7); a blend with the pickup; body families. The 3 ms we do not take |
| LR Baggs Align Session ([product](https://www.lrbaggs.com/align-series-session-acoustic-pedal)) [M] | Acoustic saturation, a three-band compressor, EQ | A light comp belongs in the box |
| LR Baggs VoicePrint DI [U] | A phone app records the player's guitar through its pickup and a mic and computes a custom pickup-to-mic filter | The same measurement idea as §2.6 |
| IR-based acoustic simulator pedals (Hotone Omni AC, NUX Optima Air, Mooer Acoustikar + 3Sigma IRs) [F, [roundup](https://progressiveradionetwork.com/best-acoustic-simulator-pedals/)] | A body IR after an EQ. Reviews: "remarkably convincing in a live mix"; the analog-modelled ones are "less realistic in isolation" | IR = the measured high band; still missing: pickup inversion, decay, tunability |
| Fractal Axe-Fx ([forum](https://forum.fractalaudio.com/threads/an-acoustic-ir-odyssey.121093/)) [F] | No acoustic block. Users Tone-Match a recording of an acoustic (a minimum-phase IR) or load acoustic IRs. One warning: "keep that low end out… it really clutters things up. Likes to feed back" | `low_cut`; the low modes need their own control (`body`) |
| Line 6 Variax acoustic models [I] | Hex piezo plus per-model processing inside the guitar (Martin, Gibson J-200 and others) | Out of reach: it needs a hex pickup |
| NAM capture "Acoustic Guitar" (tone3000 #27688, in golem's tones index) [V] | A WaveNet trained on one acoustic simulator chain | One setting, no controls. Its receptive field cannot hold a body mode that rings for 0.5 s (§1.3) |
| [Airwindows WoodenBox](https://www.airwindows.com/woodenbox/) (MIT) [V] | "A miniature reverb for converting DI to acoustic". The author **rejects resonances** ("wolf tones… always bad") and treats the body as "a miniature room made out of wood" | The noise-like high band (§2.5). We keep the low modes anyway: the 102 Hz mode is what radiates the bass (§1.3) |

No open-source acoustic simulator with a body model was found. The MOD forum's answer is "an acoustic IR
in an IR loader, plus EQ, comp and reverb" [F].

### 1.2 Research

- **Karjalainen, Penttinen, Välimäki, "More acoustic sounding timbre from guitar pickups", DAFx-99**
  ([pdf](https://dafx.de/paper-archive/1999/karjalainen.pdf)) [V]. This is the founding study, and the
  only one that tries a **magnetic** pickup.
  - Method 1: a hammer on an acoustic's bridge with the strings muted gives a body IR. It was applied to
    the electric pickup with an HF boost to undo the pickup's low-pass. Result: "nasal, lacks clarity".
  - Method 2: **a single-coil magnetic pickup in an acoustic's sound hole, a condenser mic 1 m in front,
    rich playing, and the EQ is the deconvolution P(ω) / X(ω)**. Result: "much richer and clearer", and
    "not nasal". The best FIR was of **order 750**: a higher order adds "an undesired impression of a
    room". (The paper's text does not give the sampling rate, so the length in ms is [U].)
  - Above 1 kHz the body's response "resembles exponentially decaying white noise". Replacing it with
    synthetic noise above a 500 Hz – 2 kHz crossover worked. A lower crossover gave a muffled sound.
  - What was still missing: "some of the clarity", a different attack, and above all **"the long sustain
    of the electric guitar makes the sound approach a pure electric guitar sound when playing long
    notes"**.
- **Karjalainen, Välimäki, Penttinen, Saastamoinen, "DSP equalization of electret film pickup for the
  acoustic guitar", JAES 48(12), 2000** ([AES](https://www.aes.org/e-lib/browse.cfm?elib=12032))
  [V, abstract]. The best results came from long FIRs, "possibly with one or more body resonances modelled
  with separate digital resonators", or from warped IIRs.
- **Rau, Abel, Smith, "Contact sensor processing for acoustic instrument recording using a modal
  architecture", DAFx-18** ([pdf](https://ccrma.stanford.edu/~mrau/papers/DAFx2018_Rau_Abel_Smith.pdf))
  [V, re-read at review]. Mode frequencies and dampings come from the contact sensor's response (Complex
  Exponential / Prony fit in 9 bands up to 6 kHz, then a constrained optimization that shares the poles
  between sensor and microphone). The mode shapes are fitted by least squares, and their ratio gives a
  **parallel bank of biquads** (88 modes on an upright bass). The architecture "provides the option to
  choose from or mix between different microphone responses by tuning only the relative mode gains", and
  allows "simple switching between or interpolating between microphone responses". This is our mic
  control. Note what it is: interpolating gains is exactly a **crossfade of two microphones' signals**
  (the system is linear), not a microphone that travels (§2.5).
- **Rau, Abel, Smith, "Electric-to-acoustic pickup processing for string instruments… hexaphonic
  pickup", JASA 2021** ([project](https://ccrma.stanford.edu/~mrau/projects/PickupProcessingWithDamping))
  [V, abstract and demo page]. The filters are fitted to an acoustic's bridge admittance. **"Unlike
  previous methods which typically only perform equalization, our method includes detailed physics-based
  string damping changes"**, run in real time from onset and pitch detection on each string. It needs a
  hex pickup. Ours is a mono approximation (§2.3).
- **Ducceschi, Russo, Webb, "Measurement-informed nonlinear modal synthesis of 65 classical guitars",
  DAFx-26** ([pdf](https://www.dafx.de/paper-archive/2026/papers/DAFx26_paper_40.pdf)) [V]. They extract
  50 – 200 bridge modes and 80 – 300 radiation poles per guitar from the Mores archive (§1.4). That shows
  the archive fits to modal form. Their code repository has no licence, so it is not used.
- **Penttinen, Karjalainen, Härmä, "Morphing instrument body models", DAFx-01**
  ([archive](https://dafx.de/paper-archive/details/PwdmfNB8tJwCtEV3wX0hUw)) [V, abstract]. Body models can
  be morphed stably. That supports crossfading between models and continuous `size` (§2.5).
- **Commuted synthesis** ([Smith, PASP](https://www.dsprelated.com/freebooks/pasp/Commuted_Synthesis.html))
  [V]: the least-damped body modes are best pulled out as parallel resonators and the rest kept as a
  table. That is the same split as §2.5.

### 1.3 Guitar body physics (the numbers the voicing starts from)

From Rossing & Caldersmith, *The Science of String Instruments* ch. 3, which reproduces Fletcher &
Rossing ([pdf](https://logosfoundation.org/kursus/The%20Science%20of%20String%20Instruments.pdf)) [V]:

- **Three coupled resonances between 100 and 250 Hz**: the A0 air mode ("Helmholtz") coupled with the
  top's and back's (0,0) modes.
  - At the lowest one the plates move in opposite directions and the guitar "breathes" through the sound
    hole.
  - D-28-class dreadnought: **102 Hz** ("radiates efficiently through the sound hole") and **193 – 204 Hz**
    (169 Hz with the ribs held: the player's grip shifts it).
- **Seesaw (1,0) modes**: near **400 Hz** on cross-braced steel-string guitars (376 and 436 Hz on the
  D-28, which barely radiate forward), and 250 – 300 Hz on fan-braced classicals.
- **Top-plate modes (Table 3.1)**: D-28 163 / 326 / 390 / 431 / 643 / 733 / 756 Hz; D-35 135 / 219 / 313 /
  397 / 576 / 626 / 648 / 777 Hz; classicals 183 – 660 Hz. A "(2,0) around 550 Hz" is prominent in
  classicals and less so in folk guitars.
- **Classical**: A0 ≈ 98 Hz, T1 ≈ 200 Hz, T2 ≈ 240 Hz on one measured Conde
  ([DAGA 2021](https://pub.dega-akustik.de/DAGA_2021/data/articles/000093.pdf)) [I, from a search
  summary].
- **Decay [A]**: τ = Q / (π f). Low body modes with Q 30 – 50 at 100 Hz [U] give T60 = 0.7 – 1.1 s; at
  400 Hz, about 0.2 s. A body IR therefore needs tens of thousands of taps to hold its lows, but only
  1 – 2 k taps for the band above 1 kHz. **Modal below, FIR above.**

### 1.4 Data we may use (and what we may not)

| Source | Content | Licence | Use |
|---|---|---|---|
| **Mores archive** ([Zenodo 4604577](https://zenodo.org/records/4604577), R. Mores, HAW Hamburg, 2021) [V: record metadata, `Method.pdf`, `Comments.pdf` read at review] | 65 numbered classical, flamenco and romantic guitars. A 9 g impulse hammer at the bridge (bass / centre / treble); **two bridge accelerometers**; **three near-field mics 10 cm above the top** ("treble" and "bass", 10 cm either side of the bridge centre, and "upper east", 20 cm from the treble one, "where flamenco guitarists usually place their stage microphone"). 48 kHz, 1 s per impulse, reflection-free for 12 – 36 ms (walls at 2 – 6 m). `qualified_selected_impulses.mat`, 32.7 MB, not yet normalized (the archive's `.m` files give the calibration). g07 and g30 have poor raw data and the author doubts g62: not used | **CC BY 4.0** (the record's licence field) | The **Classical** model (bridge force → mic): fit and attribution (§2.6) |
| **IEM / DEGA guitar recordings** ([project](https://opendata.iem.at/projects/dega_guitar_recordings/), [session 1](https://phaidra.kug.ac.at/o:68022), [session 2](https://phaidra.kug.ac.at/o:69273), [paper](https://pub.dega-akustik.de/DAGA_2018/data/articles/000295.pdf); F. Zotter, D. Rudrich) [V: project page, Phaidra record and paper read at review] | A Hanika 59PF classical and a **Taylor 612ce 12-fret steel-string "equipped with a piezo pickup behind the saddle"**. **Piezo DI** (BSS AR133) recorded together with **two AKG C414 B/ULS at bridge and neck**, two NTI M2230 reference mics and a room mic at 3 m. IEM CUBE (RT 0.5 s, 22 dBA), with room transfer functions, scores and documentation | **Public Domain Mark 1.0** on Phaidra; "All the contents are in the public domain" on the project page | The **Concert** model's first measured data and the **piezo source** (§2.6, §2.7): piezo → mic over real playing; session 1 to fit, session 2 to test. Its classical guitar gives the far-field tilt of the Classical model |
| Rossing / Fletcher mode tables [V] | Mode frequencies | Facts | Starting values for the parametric voicings (§2.6) |
| Our own measurements (§2.6) | Steel-string bodies | Ours | Dreadnought, Jumbo, Concert, Parlor, Round-back |
| Ducceschi et al. code; commercial IR packs; tone3000 captures | — | None given / no-redistribution EULAs [I] / unknown | **Not used** |

No open steel-string **impulse** dataset was found (Zenodo, DAGA, AES searches). The thesis record
[Zenodo 3407272](https://zenodo.org/records/3407272) is CC BY 4.0 but holds only a PDF. The IEM recordings
are the one open steel-string source: a piezo and two studio mics on the same takes.

**Licences: the plan is clean** (checked at review).

- **CC BY 4.0 → GPL-3.** CC BY 4.0 has no ShareAlike and forbids only *additional restrictions on the
  licensed material itself*; the FSF's licence list gives it as compatible with every GPL version (and
  advises against using it for software, which we do not: it covers data). Read through two secondary
  sources (Trisquel's mirror of the list, and a report of the gnu.org change); **gnu.org itself refused
  our requests (403 / 429) on the review day**, so the entry `#ccby` of
  [the list](https://www.gnu.org/licenses/license-list.html) is one click for the owner to confirm [I].
- **What we ship** from the archive is a table of mode frequencies, dampings and gains and a 21 ms
  residual, fitted from one guitar: adapted material. The attribution duty (CC BY 4.0 §3(a)) is met by one
  notice, kept wherever the table goes: creator, title, DOI, licence and its URI, and that the data were
  modified. GPL-3 §7(b) lets such a notice be required downstream.

  > Classical body: fitted from guitar g34 of R. Mores, *Archive for the acoustical documentation of
  > classical Spanish guitars, flamenco guitars and romantic guitars from private and public collections —
  > bridge mobility*, Zenodo, 2021, doi:10.5281/zenodo.4604577, CC BY 4.0
  > (https://creativecommons.org/licenses/by/4.0/). Modified: windowed, converted and reduced to a modal
  > table and a short filter by golem-fx.

  It goes in this README, in the table's source file, in a `NOTICE` file shipped with the bundle and on
  golem's about page. Nothing may suggest that the author endorses the pedal; the maker's name of the
  chosen guitar appears in the notice only, never on the pedal or in a preset name.
- **IEM / DEGA.** The authors publish their own recordings under the Public Domain Mark and say so in
  words on the project page and in the paper's title. The mark is a label, not a waiver like CC0, but the
  intent is explicit, and we ship no audio: only a fitted table. No duty; we credit them anyway (same
  `NOTICE`), which is the academic norm.
- The raw files of both sources stay out of the repository. `body-fit` downloads them by DOI / URL and
  checks a SHA-256, so the tables can be rebuilt.

### 1.5 Forum feedback: why acoustic simulators sound "quacky" or thin

- **"An IR can't produce sounds that aren't there"**, "magnetic pickups lack high-frequency content"
  ([Fractal forum](https://forum.fractalaudio.com/threads/is-the-a-preset-to-make-an-electric-guitar-sound-like-an-%E2%80%9Cacoustic%E2%80%9D.182589/))
  [F]. It is half true [A].
  - The highs are in the EMF. The pickup's resonance and its load hide them: −14 dB at 10 kHz for a
    single coil, −20 dB for a humbucker (gfx-fuzz §2.2). The inverse filter brings them back (§2.1).
  - What really is missing: the comb of the pickup position (§2.2), the humbucker's two-coil null near
    3 kHz on wound strings [A], the string material, and finger and pick noise.
- **Fizz and harshness** ([Gear Page on the AC-3](https://www.thegearpage.net/board/index.php?threads/anyone-use-the-boss-ac-3-acoustic-simulator-pedal.1904354/))
  [F]: "the fizz… if you turn [Top] down the fizz goes away, but so does the acoustic tone". "A hot Les
  Paul was far too hot for this pedal"; it worked "with my neck pickup, rolled off a little of my guitar's
  tone". Our reading: an analog input clipping, plus a bridge pickup's treble that nobody corrected
  (§2.2).
- **Thin / boxy**: few resonances, and a generic EQ that is not a body. Mooer Acoustikar buyers: "makes
  it sound thin and unpleasant"; "works best on single coil pickups" ([Thomann reviews](https://www.thomann.de/be/mooer_acoustikar_reviews.htm))
  [F].
- **Sustain gives it away** (the 1999 study, above) [V]. Players work around it by palm-muting [F].
- **"Wound strings sound markedly different from plain"** through one IR [F]. One average filter cannot
  fit both.
- **Low end feeds back and clutters a band mix** (Fractal, above) [F]. Hence `low_cut` and the blend of a
  piezo voice (§2.8).
- **What players want**: convincing in a band mix, not necessarily solo [F]. A blend of "piezo and sound
  hole" sounds for live use [F]. A preset that works on the neck pickup [M, AC-3].

### 1.6 What we conclude

1. **Invert the pickup, do not just boost highs.** The DI → EMF step is gfx-fuzz's second-order inverse,
   band-limited. The EMF → bridge-force step is the string's own physics (a shelf set by the pickup's
   position).
2. **Hybrid body**: a modal bank for what rings (tunable and cheap) and a short FIR for the dense high band.
   Both are fitted from measurements, with zero latency and no FFT.
3. **Microphone position = mode gains** (Rau 2018): a continuous blend of fitted microphones, EXP-able,
   and stereo for the price of a second set of gains.
4. **Shorten the electric sustain** per band, keeping the attack: the one thing no EQ or IR product does.
5. **Measure the steel-string bodies ourselves** with the 1999 deconvolution protocol. Use the CC BY
   archive for nylon and the public-domain recordings for the first steel-string body and the piezo.
6. **Honest limits**: the comb notches cannot be inverted from a mono DI, and finger noise is absent. A
   solo acoustic passage will stay "very close", not "the same" (§6).

## 2. Design

```
DI ─┬─▶ SOURCE (§2.1–2.2, by `pickup`) ─▶ TOP shelf ─▶ DECAY (§2.3) ─▶ 12-STRING (§2.4, if on) ─▶ F̂ (bridge force)
    │     Magnetic: EMF inverse ▶ position shelf R(s) ▶ aperture shelf                     │
    │     Piezo:    force → reference D(s) ▶ quack tamer                                   │
    │                                                                                       ▼
    │                     BODY (§2.5) : modal bank (SVF modes) ─┬─ gains(mic) ─┐          F̂ ▶ piezo voice (§2.8)
    │                                   high-band FIR(mic)  ───┴──────────────┴─▶ Σ ─▶ + piezo · ─▶ low_cut
    │                                                                                       ▼
    │                                                       COMP (§2.8) ─▶ ROOM ─▶ × K(model) × level
    └──────────────────────────────── dry (× 1) ───────────────────────────────▶ MIX (crossfade) ─▶ out
```

All stages run at the host rate. No oversampling: the only nonlinear stages are slow gains (decay,
compressor) and the 12-string's octave voice, which is band-limited to 400 Hz in and 800 Hz out.

**F̂ is defined in one reference for every source and every model**: the EMF of a single-coil pickup in
the sound hole of the modelled guitar, at x_s from the saddle (§2.2). It is what the steel-string bodies
are measured from (§2.6). Force-like data (the archive's hammer, a piezo) are converted to it by D(s)
(§2.7). "Bridge force" in this document is shorthand for that reference.

### 2.1 Source: back to the EMF (shared with gfx-fuzz)

- A DI recorded on a Hi-Z input is the EMF filtered by the pickup's resonance against its load (gfx-fuzz
  §2.2). With Y_rec = the admittance at the pickup node at recording, **e = DI · (1 + (R_pu + s·L_pu)·Y_rec)**.
- The `pickup` values are gfx-fuzz's (Single Coil 2.5 H / 6 kΩ / 100 pF; Humbucker 4.5 H / 8 kΩ / 150 pF;
  P-90 7 H / 8 kΩ / 120 pF; 500 pF cable; Active = resistive source, no inverse).
- **Two differences from gfx-fuzz.**
  1. **The band limit.** gfx-fuzz makes the inverse proper with two poles at 20 kHz, inside its
     oversampler. Here the two poles (Butterworth, matched design from gfx-dsp `matched`, at the host
     rate) sit at **f_p = min(12 kHz, 3 × the pickup's resonance)**: 12 kHz for Single Coil, 8.7 kHz for
     Humbucker, 7.2 kHz for P-90. The inverse then levels off at about +19 dB for every pickup.
     - Lift re DI [P]:

       | `pickup` | 4 kHz | 6 kHz | 8 kHz | 10 kHz | Plateau | Interface hiss, A-weighted |
       |---|---|---|---|---|---|---|
       | Single Coil (12 kHz) | −3.6 dB | +3.0 | +8.9 | +12.5 (unlimited: +14.2) | +17.8 dB | +10 dB |
       | Humbucker (8.7 kHz) | +0.4 dB | +9.5 | +13.9 | +16.2 (unlimited: +20.5) | +18.5 dB | +13 dB |
       | P-90 (7.2 kHz) | +5.5 dB | +12.8 | +16.1 | +17.5 (unlimited: +24.2) | +18.8 dB | +14 dB |

     - With 12 kHz for all three (the first draft), the humbucker's hiss rose by 16 dB and the P-90's by
       20 dB [P]. Above f_p the acoustic's own radiation falls, the 1999 study's spectra are noise above
       7 kHz, and the inverse would only lift the interface's hiss. The decay stage (§2.3) is a downward
       expander on tails, so what hiss is left falls with the note.
  2. **Nothing takes the inverse back.** In gfx-fuzz the simulated guitar network follows the inverse, so
     a wrong guess of L_pu or of the cable cancels (its invariant). Here the EMF is the product: **a wrong
     guess is heard**, as a treble error above 3 kHz [P, guess = Single Coil]:

     | The real guitar | 3 kHz | 4 kHz | 6 kHz | 10 kHz |
     |---|---|---|---|---|
     | Cable 250 pF (guess 500 pF) | −1.7 dB | −1.6 | +3.8 | +5.1 |
     | Cable 900 pF | +2.1 dB | −2.5 | −5.4 | −4.9 |
     | A hot single coil, 3.2 H / 7 kΩ | −0.2 dB | −2.1 | −3.1 | −2.5 |
     | A humbucker | +2.7 dB | −4.1 | −7.1 | −6.3 |
     | Tone knob rolled to about 100 kΩ | −2.5 dB | −3.9 | −2.1 | −0.6 |

     So `pickup` must be right (golem sets it at insert), and **`top` is the trim** for the rest: its
     ±10 dB shelf at 2.5 kHz covers every row. This is why `top` sits on the front panel.
- **Code**: a new gfx-dsp module **`pickup`**: the values, Y_rec, the inverse as an **analog prototype**
  (s-domain coefficients and the band-limit poles as an argument), and the position shelf of §2.2. Each
  client discretizes it its own way: gfx-fuzz bilinear at its oversampled rate (it must match its
  netlist's trapezoidal rule), gfx-acoustic matched at the host rate. gfx-fuzz is designed but not coded
  yet, so the module is written once for both. The shared bench invariant is on the prototype (§5).

### 2.2 From string velocity to bridge force

A magnetic pickup's EMF is about the string's **velocity** at the pickup, x_p from the bridge [I, Faraday's law;
Paiva, Pakarinen, Välimäki, JAES 2012]. An acoustic's top is driven by the **force** at the bridge,
F = T · ∂y/∂x at x = 0.

- **Per partial n [A, checked at review]**: F_n / v_n = Z₀ / sin(k_n x_p), where Z₀ = T / c is the
  string's impedance and k = ω / c.
  - Below the first notch, 1 / sin(k x_p) ≈ c / (ω x_p): the force follows the string's **displacement**
    at the pickup, divided by x_p.
  - Above it, averaged over the comb in power, ⟨sin²⟩ = ½: the force is **√2 · Z₀ times the rms
    velocity** (+3 dB, not 0 dB; the first draft left the √2 out) [P].
  - So the shape is a shelf, (s + ω_x) / s with **ω_x = c / (√2 · x_p)**: an integrator below the corner,
    flat above it. Against the exact six-string average it is within ±1 dB [P].
- **The comb cannot be inverted.** Its notches sit at f = m · c / (2 x_p). Strat neck pickup, low E:
  334, 668 Hz… [A]. They are fixed per string (c = 2 · scale · f_open does not change with the fret) and
  different on each of the six strings, which a mono DI mixes. We correct only the average.
- **The measured reference already holds an average comb.** The bodies are measured through a magnetic
  pickup in the sound hole (§2.6), at x_s ≈ 200 mm on a 645 mm scale [U]. So the correction is the
  **ratio** of the two positions' shelves:
  **R(s) = (s + ω_e) / (s + ω_s)**, normalized to 0 dB above both corners.
  - Low-frequency gain of R = x_s / x_e [A], whatever the corners.
  - Corners, f = c / (2√2 π x_p), geometric mean over the six open strings (f̄_open = 166 Hz, 648 mm
    scale) [P]:

  | `pickup_pos` | x_p [U] (Strat / LP within 3 %) | Corner f_x | R(s) relative to the sound-hole reference (x_s = 200 mm, 121 Hz) |
  |---|---|---|---|
  | Neck | 160 mm | 152 Hz | +1.9 dB at DC, +1.1 dB at 120 Hz: nearly the reference |
  | Middle | 100 mm | 243 Hz | +6.0 dB at DC, +4.0 dB at 120 Hz |
  | Bridge | 41 mm | 592 Hz | **+13.8 dB at DC**, +11.0 dB at 120 Hz, +6.2 dB at 300 Hz, +0.6 dB at 1.5 kHz (equivalently, the bridge pickup is up to 14 dB too bright above 600 Hz) |
  | Neck + Middle, Middle + Bridge | Parallel pairs: the mean of the two velocities [A] | Shelf with the mean corner | Out-of-phase combs: a "quacky" notch set no filter can undo; presets avoid them |

  - **x_s is measured, not assumed.** On a 14-fret dreadnought the sound hole spans roughly 100 – 200 mm
    from the saddle [A, from the fret positions], so a sound-hole pickup sits at 150 – 200 mm. The
    protocol records it (§2.6, step 2) and the table is recomputed per measured guitar. At
    x_s = 165 mm the Neck row is +0.3 dB (the reference itself) and Bridge +12.1 dB [P].
  - **What the average hides.** The corner belongs to each string: for the Bridge position it runs from
    293 Hz (low E) to 1.17 kHz (high e). One shelf is right for the set and wrong by up to ±6 dB per
    string in the two octaves around the corner. A hex pickup would fix it; a mono DI cannot.
  - Electric and acoustic string sets also differ in impedance (plain against wound G, lighter gauges):
    the balance between strings stays the electric's.

  This is the physics behind the AC-3 manual's front-pickup advice and the forum's "neck pickup, tone
  rolled off" [A].
- **Aperture shelf (Humbucker)**: two coils 18 mm apart sum as cos(k d / 2). The first null is at
  c / (2d): 2.9 kHz on the low E, 11.5 kHz on the high e [A]. A fixed +3 dB high shelf at 4 kHz restores
  the average; the nulls stay [A, voicing].
- **Active** pickups: no EMF inverse (their buffer already gives a flat source), the same shelves.

### 2.3 Decay: making the strings sustain like an acoustic's

The body drains the strings through the bridge (the real part of the bridge admittance), most at its
resonances and in the highs. A solid body hardly does. Rau 2021 does this per string with a hex pickup.
With a mono DI we do it **per band and per note, from the envelope**. It is causal, with no look-ahead.

**Rule: this stage never does worse than the DI.** Its gain is ≤ 0 dB, a pick is never attenuated, and
what it cannot shorten it leaves alone.

- **Bands**: gfx-dsp `crossover` `Tree` (designed by gfx-bassdi / gfx-grit), 4 bands, LR4 at 250 Hz,
  1 kHz and 4 kHz, with all-pass compensation so the sum is an all-pass when nothing acts.
- **Per band b**:
  - **Detector** e_b (dB): a sliding maximum over **25 ms in every band**, then a release smoother of
    60 ms (instant attack).
    - The window must hold one period of the lowest **note**, not of the band's lowest frequency: a
      low E's harmonics in the 1 – 4 kHz band still pulse every 12 ms. The first draft's windows (25, 4,
      1, 0.25 ms) gave 1.5 – 6.7 dB of gain ripple at the note's period in the three upper bands, and an
      attack cut by 1 – 10 dB in the top two [P].
    - The 60 ms smoother keeps beats faster than about 5 Hz out of the gain.
  - **Reduction** r_b (dB, ≥ 0), g_b = −r_b:
    - while e_b falls, r_b grows by **k_b dB per dB of fall**: a partial that decays as e^(−αt) comes out
      as e^(−α(1 + k_b) t), so **T60 is divided by (1 + k_b)**;
    - when e_b rises by Δ above its last minimum, r_b drops to r_min · max(0, 1 − Δ / 3 dB), within the
      same sample: **a new pick re-opens the band**, fully from a 3 dB rise;
    - r_b is capped at **24 dB** and smoothed over 2 ms.
  - Not a peak memory: the first draft held the last peak for seconds (3 dB/s release), so a note played
    12 dB softer than the one before it, 0.6 s later, lost 10 – 39 dB of its attack in the three upper
    bands [P]. With the rule above the same note loses 3 – 7 dB in the two bands where it does not rise
    3 dB above the older note's tail, and nothing elsewhere [P].
  - The log, the rule and the exp run every 8 samples; the gain is ramped in between.
- **k_b belongs to the model** (its intrinsic voicing). For example, Dreadnought k = 0.4 / 0.8 / 1.2 / 1.6
  (low → high); Classical higher in the highs (nylon's fast HF decay); Round-back lower ("subdued body
  resonance") [U, voicing on the bench against real recordings]. `decay` scales all k_b
  (50 % = the model's own, 0 % = off, 100 % = double).
- **Measured on the prototype** [P] (`decay2.py`; synthetic plucks, T60 4 s at 100 Hz to 1 s at 4 kHz):

  | Test | Result |
  |---|---|
  | Single notes E2, A3 | T60 divided by 1.37 – 1.39 / 1.74 – 1.77 / 2.11 – 2.14 / 2.41 – 2.66 for targets 1.4 / 1.8 / 2.2 / 2.6; first 5 ms at 0.0 dB in every band; gain ripple ≤ 0.2 dB (0.4 dB in the top band) |
  | A second note of equal level in the same bands, 0.6 s later | every band back to 0.0 dB within 20 ms: the new note is whole, and the older note's tail comes back to the level it has on the DI |
  | Dyad E2 + B2 | energy off the two harmonic series: −56.6 dB in, −53.6 dB out (the first draft: −45.1 dB) |
  | Two partials 2 Hz apart, 2 : 1 | beat depth 9.7 dB in, 17.7 dB out: **slow beats are deepened by (1 + k)** |
  | The same, 8 Hz apart | 9.6 dB in, 10.4 dB out |

- **What stays, and how it sounds**:
  - **Arpeggios and fingerstyle.** A new note re-opens its bands, and the older notes ringing there
    return to their DI level (a step of up to r_b, under the new note's attack). The shortening then
    acts between picks only. On single lines and long notes, where the 1999 study heard the problem, it
    acts fully. Under steady strumming it does almost nothing, as on a real guitar that is struck again.
  - **A much softer note under a ringing one** is partly attenuated (above).
  - **Slow beats** (two strings a few cents apart, a 12-string) are deepened by (1 + k). k ≤ 1 in the two
    low bands, where they live.
  - **Noise.** Below −70 dBFS in a band r_b is held, with 6 dB of hysteresis. On hiss alone at −60 dBFS
    the gain wanders by 0.4 – 5.6 dB [P]: the floor and the hysteresis are set on the bench with the
    real DIs.
- **At `decay` 0 %** the Tree is bypassed, crossfaded over 20 ms: bit-exact pass-through.
- **Fallback if the bench or the listening rejects it** (§5), in this order:
  1. the cap from 24 dB to 12 dB and the model k_b halved (less shortening, steps of 12 dB at most);
  2. a release-only sustain shaper as in gfx-transient: the difference of a 30 ms and a 400 ms release
     envelope per band, at most 6 dB, which shortens the first half second and cannot re-open audibly.
  There is no static fallback: a fixed filter cannot change a decay time.
- **What it cannot do**: per-string decay differences, or sympathetic ringing. Two notes in one band
  share one gain.

### 2.4 Twelve-string courses

A 12-string doubles every course. The four low courses get an **octave** string, struck first on a
downstroke because it lies above. The B and e courses get a **unison** string. Built on F̂ (before the
body: all courses excite the same top):

- **Octave voice**: gfx-octaver's Poly octave-up, z² / |z| per band of the 28-band ERB complex filterbank
  (zero latency; one complex pole per band, 1 ERB wide, 40 Hz – 5 kHz, read in gfx-octaver's code). It is
  limited to bands below `twelve_range` (fixed at 400 Hz [A]).
  - Doubling every partial in a band keeps each note harmonic: 2 · (n f₀) is a harmonic of f₀.
  - The octave strings sound E3 – G4 from the low E, A, D and G courses.
  - In a mono DI the B string's fundamental (247 Hz) is doubled too. That is accepted (a real 12-string's
    B course is a unison, so this is a slight excess of octave on B-string notes below 400 Hz).
  - **On chords it is not clean, and that is accepted.** One-pole bands overlap, so two partials closer
    than about one ERB (35 Hz at 100 Hz, 68 Hz at 400 Hz) share a band, and z² adds their **sum tone**
    f₁ + f₂ to the two octaves. In an open E chord, E2 + B2 gives 205.9 Hz against the chord's G♯3 at
    207.7 Hz: a 1.8 Hz beat [A]. Sum tones of chord notes fall on or near chord notes, which is why Poly
    octaves pass as "12-string" at all. The voice is mixed at `twelve_level` under the main courses,
    detuned and delayed, so they read as course shimmer. The bench reports them (§5).
  - The voice stops at 800 Hz. The brightness of the thin octave strings comes from the unison voice:
    their harmonics are the even harmonics of the main string, which the unison voice doubles.
  - The low bands lag by their group delay, about 9 ms at 100 Hz and 5 ms at 400 Hz [A, 1 / (π · ERB)].
    With the shifter's 3 ms and `twelve_delay` the octave voice lands 10 – 15 ms after the main course at
    the defaults: inside a strum's spread, and benched as a number.
  - The filterbank moves from gfx-octaver into gfx-dsp as `filterbank` (the gfx-dsp README already plans
    that module), with gfx-octaver's tests.
- **Unison voice**: the whole F̂ above 150 Hz.
- **Course detune and strum offset**: octave + unison voices are summed and passed through **one gfx-dsp
  `CenteredShifter`** (gfx-detune's: 3 ms centre, search window, reported latency 0 because the centre is
  part of the doubled voice). It runs at +`twelve_detune` cents, delayed by `twelve_delay` (strum offset)
  on top of the 3 ms centre.
- Level: `twelve_level` relative to the main courses. Mixed into F̂.

### 2.5 Body engine

One engine for every model and every mic position:

- **Modal bank**: N = 30 – 60 modes below about 1.2 kHz (per model, from the fit in §2.6).
  - Each mode is a **TPT state-variable band-pass** (gfx-dsp `svf`, f64 state), with g = tan(π f / fs)
    and k = 1 / Q.
  - It stays stable and click-free when f moves, which `size` and model crossfades need. A direct-form
    biquad bank does not.
  - Output = Σ_m [a_m(mic) · bp_m(F̂) + b_m(mic) · lp_m(F̂)]: **two real gains per mode and per mic**,
    which is a complex residue. One real gain is not enough: after the bulk delay is removed, paths that
    differ by ±10 cm still turn the phase by ±105° at 1 kHz [A]. Rau's fit uses complex mode shapes too.
  - The gains **interpolate piecewise-linearly** between the fitted positions **Bridge (0 %), Sound hole
    (50 %), Neck (100 %)**. This is exactly a crossfade of the two neighbouring microphones' signals
    (§1.2): between two positions the response is that of two blended mics, with their partial
    cancellations, not of a mic half-way. A mode whose gains have opposite signs passes through zero.
  - `size` multiplies every f_m by 100 / `size` (a body larger by s has its modes lower by 1 / s; Q kept).
    The FIR does not move. While `size` moves, g = tan(π f / fs) is recomputed every 32 samples and
    ramped (48 `tan` per sample would cost more than the whole pedal).
  - `body` scales the whole bank by ±12 dB.
- **High-band FIR**: the residual above the modal band. It is the measured IR minus the modal part,
  high-passed at the crossover (LR4 at about 1 kHz), with its propagation delay removed (§2.6) and a
  half-Hann fade from 15 ms to 21 ms. The modes' own skirts above 1 kHz are inside the subtraction, so
  the sum equals the measurement there.
  - **1,024 taps at 48 kHz** (21 ms; the 1999 study's best was order 750; longer "adds a room").
  - A mode at 1.5 kHz with Q 50 rings for 73 ms [A]: the fade cuts such tails. If the T60 row of the
    bench shows it, the modal band rises to 2 kHz at the fit (more modes, same FIR).
  - It is resampled at `initialize` for 44.1 / 88.2 / 96 / 192 kHz (2,048 taps at 96 kHz). That is a few
    ms of work outside `process`, done once.
  - **Direct-form convolution**, SIMD over taps: output sample 0 depends on input sample 0, so there is no
    latency and no partitioning. Measured: **36 – 53 ns per sample for one 1,024-tap kernel** (f32, AVX2
    FMA, four accumulators, i9-13900H; `fir.c`) [P]; 73 ns for 2,048 taps, 148 ns for 4,096.
  - Mic position: three FIRs per model. A linear crossfade of two FIRs fed the same input **is one FIR**
    with blended taps. So **one kernel runs per output channel**: the blend is rebuilt every 32 samples
    while `mic` moves (1,024 multiply-adds per block), and the old and new kernels are crossfaded over
    that block (two convolutions only while `mic` moves or a model fades).
  - The two tails are only partly correlated, so a linear blend dips in level half-way (−3 dB if fully
    uncorrelated). The blend is scaled by 1 / √(w² + (1 − w)² + 2 w (1 − w) ρ), with ρ the two FIRs'
    measured correlation, one number per pair and per model in the table.
- **Why not one long IR?** The low modes need 0.7 – 1 s (§1.3): 48 k taps, so partitioned convolution
  with a direct first block, about 10× the CPU. And no `size`, `body`, continuous `mic` or model
  morphing. The modal low band gives all four.
- **Why not purely parametric?** Above about 1 kHz a body is dense and noise-like. Hand-placed resonators
  there sound like "wolf tones" (WoodenBox's objection) or "nasal" (the 1999 hammer method). The
  measured or noise-like FIR handles it.

### 2.6 Where each model's data come from

**`model`**: Dreadnought · Jumbo · Concert · Parlor · Classical · Round-back.

| Model | v1 data (ships first) | Final data |
|---|---|---|
| Dreadnought, Jumbo, Parlor | **Parametric**: mode frequencies from §1.3 (D-28-class 102 / 193 / 376 / 436 Hz and Table 3.1). Jumbo and Parlor scaled by body area [A], Q 15 – 50 [U]. The high band is **synthetic**: exponentially decaying noise (the 1999 study's finding), T60 tilted with frequency, seeded | **Our measurement** (below) |
| Concert (OM / grand concert) | **IEM / DEGA fit** (public domain): piezo → C414 at bridge and at neck on session 1's playing, as steps 5 – 6 below, through D(s) (§2.7). A 12-fret grand-concert maple body. "Sound hole" = the mean of the two mics. Caveat: the room (RT 0.5 s) is in the estimate; the 21 ms fade removes it above 1 kHz, and below it the modes are kept only where the two reference mics agree [U, on the bench] | Our measurement, if it beats the fit on the held-out row |
| Classical | **Mores archive** fit (CC BY 4.0) on one mid-century Spanish concert guitar (not g07, g30, g62). Poles from the two **bridge accelerometers** (no room in them), gains per mic from the hammer → mic responses, windowed before the first reflection; through D(s) (§2.7). "Bridge" = mean of the treble and bass mics; "Neck" = the upper-east mic; "Sound hole" = their mean. These are near-field mics at 10 cm: a fixed 1/3-octave tilt brings their long-term spectrum to that of the IEM classical guitar's C414s (public domain), so the model reads as a studio mic | Same; more guitars from the archive as presets of `size` / `body`, if blind tests ask for it |
| Round-back | Parametric: few, low-Q modes and a short tail ("subdued body resonance", AC-3) | Measurement if a guitar is available |

Model names are generic. No maker's name appears on the pedal or in preset names.

**Measurement protocol (steel strings)**, after the 1999 study's method 2:

1. **Guitars**: one good example each of a dreadnought, a jumbo, an OM / grand concert and a parlor
   (owned or borrowed: the data are ours). New phosphor-bronze strings, tuned to 440.
2. **Sensors, recorded together at 48 kHz / 24-bit**:
   - a passive single-coil **sound-hole magnetic pickup**, its L and R measured with an LCR meter, into
     1 MΩ through a known cable, so its EMF can be rebuilt as in §2.1; **its distance x_s from the saddle
     is measured and stored with the table** (§2.2);
   - three small-diaphragm omni measurement mics at **30 cm**: bridge (lower bout), sound hole edge,
     and 12th – 14th fret;
   - a reference piezo where the guitar has one (for §2.7).
3. **Room**: as dead as available (blankets, a large room, the mics closer to the guitar than to any
   surface). The first reflection must arrive later than 4 ms after the direct sound.
4. **Playing**: 5 – 8 minutes per guitar (open chords, barre chords, single notes on every string to the
   12th fret, fingerstyle and pick, soft and hard). Two takes: one to fit, one to test.
5. **Estimate**: H1 = S_xy / S_xx (Welch, 16 k FFT, 75 % overlap) from the rebuilt EMF to each mic.
   - Bins with coherence < 0.8 are regularized toward the smoothed response.
   - Onset delay (sound travel, about 0.9 ms at 30 cm) removed, so the bulk delay is zero.
   - This averages the string-dependent comb by playing energy, which is the correction a mono DI can
     use.
6. **Fit** (offline tool, gfx-bench `body-fit`):
   - peak picking plus complex-exponential initial modes (Rau 2018), then constrained pole optimization
     (frequencies and dampings shared across the three mics, two real gains per mode and per mic, by
     least squares at each step);
   - the residual above about 1 kHz goes to the FIR (above).
   - Output: a data table compiled into the binary (§2.11). No file is read at run time.
7. **Hammer check** (optional): a small impact hammer on the bridge, strings muted, compared with the
   played estimate. It is a diagnostic, not the data (the 1999 study found the played estimate better).

A bass → upright-bass model needs the same protocol on an upright with a magnetic bass pickup (Rau 2018
used an upright with contact sensors). It is **phase 2**: `Upright` will be appended to `model` when
voiced (§7).

### 2.7 Piezo source: restoring the body (`pickup` = Piezo)

An under-saddle piezo senses roughly the vertical force at the saddle. It misses most of what the body
does to that force, and it adds its own colour: the saddle's compliance, its mounting, and the
transducer's mid peak that players call "quack" [I]. D(s) is fixed physics in v1, not a measured
de-quack curve: the first draft divided the IEM response by a Concert model that was itself parametric,
so its "D" would have held the difference between a guess and a guitar.

- **Force → reference, D(s)**. A force-like signal (a piezo, the archive's hammer) is brought to the
  reference of §2 by the inverse of §2.2's shelf at x_s:
  **D(s) = (s + ω₀) / (s + ω_s)**, ω_s = c / (√2 · x_s) (121 Hz at 200 mm) and ω₀ = 2π · 30 Hz (it bounds
  the integrator of D⁻¹, which the piezo voice uses, §2.8). One first-order section.
  - The measured force-like models are stored through it (table = measured response / D), so
    **Concert + Piezo is the IEM guitar's own piezo → mic response, exactly** (bench: on session 2, never
    fitted), and Classical + Piezo is the archive's force → mic response.
  - **The sensor's own colour is not known yet.** What a piezo adds to the true force (its mid peak) cannot
    be separated from the body with one guitar and no force sensor. In v1 it stays inside Concert, where
    it is right for a Concert-like piezo guitar. With another model it remains, and `quack` and `top`
    trim it.
  - When a measurement guitar carries both a piezo and the sound-hole pickup (§2.6, step 2), the ratio of
    the two, smoothed to **1/3 octave** (only the broad sensor colour carries across guitars), replaces
    D as a ≤ 12-section minimum-phase IIR (gfx-dsp `matched` sections), and Concert is re-stored through
    it.
- Then F̂ = D(piezo). It feeds the same strings (`decay`) and body path as the magnetic source, so every
  `model` works with a piezo guitar. Pick the body nearest the real guitar's: Fishman's "a dreadnought
  image is for a dreadnought".
- **Quack tamer (`quack`)**: a dynamic dip at 1.5 – 3 kHz (bell, Q 1.2, depth up to −9 dB). It is
  triggered by gfx-dsp `onset` and decays over 30 ms, so only the attack's quack is tamed [A, voicing].

### 2.8 After the body

- **Piezo voice (`piezo`)**: the live "pickup + mic" blend.
  - From a magnetic source the simulated piezo is F̂ through D⁻¹ (the force estimate), plus a fixed bell
    of +4 dB at 2 kHz, Q 1.2, for the sensor's colour [U, voicing; replaced by the measured curve, §2.7];
  - from a piezo source it is the input itself;
  - added to the mic signal at `piezo` (0 – 100 %, level-matched).
  - It gives the cut-through, low-feedback stage sound and the AC-3's "Piezo" character.
- **`low_cut`**: 2-pole SVF high-pass, 20 Hz (off) – 300 Hz.
- **Compressor**: the gfx-comp `Comp` engine as a library, colour off (gfx-bassdi's `colour: false`
  setting), **Opto** dynamics. One knob `comp` = c:
  - threshold −6 − 24c dBFS, ratio 1 + 3c, knee 6 dB, attack 15 ms, release 150 ms auto;
  - automatic make-up from the static curve at −18 dBFS [A];
  - c = 0 is bit-transparent. `gr` is the shared read-only meter.
- **Room (`room`)**: gfx-dsp `reverb` `Taps` (12 early reflections of a 4 × 5 m room, 1.5 m from the
  walls) plus a 0.35 s 8-line `Fdn` tail, wet added at `room`. The air around a mic at 30 cm, not a
  reverb (golem's reverb blocks do that). In 2 / 2 the taps are decorrelated L / R; seeded.
- **Level**: × K(model), fixed per model so that the defaults come out at the bypass loudness (±1 LU on
  golem's clean DI), measured once on the bench and hard-coded (gfx-fuzz's rule), then × `level`.
- **Mix**: **law: linear crossfade**, out = (1 − m) · DI + m · sim. The dry is not delayed: the wet has no
  bulk delay (onsets trimmed, §2.6), only the dispersion of the modes and of the decay stage's
  crossover (an all-pass turning 360° at each of 250 Hz, 1 kHz and 4 kHz). So partial mixes colour like a
  blended pickup and mic (as on Aura's Blend), without the regular comb of a delay; they are not flat,
  and the bench reports them.

### 2.9 Stereo (2 / 2, second layout)

The input is the mean of L and R (a mono DI on both inputs). Everything up to F̂ runs once. The body
yields **two mics at once**, at mic ± `width` / 2 (clamped to 0 – 100 %): the modal bank's states are
shared and only the gain vectors and the FIR pair differ. That is Rau's "multiple simultaneous mic
responses… efficiently computed". `width` 0 = mono. The room taps are decorrelated. In the 1 / 1 layout
`width` does nothing.

### 2.10 Switching, smoothing

- **`model`**: two bodies run during a **30 ms** crossfade. The incoming bank is driven from the moment
  of the switch, so its lowest modes reach full level within one or two picks.
- **`pickup`**, **`pickup_pos`**: new source filters, a 30 ms crossfade of two source paths.
- **`twelve`**: the course voices fade in or out over 20 ms (the filterbank and shifter keep running for
  250 ms after "off", so a re-enable does not restart them cold).
- **`mic`, `size`, `body`, `top`, `mix`**: smoothed per sample (gfx-dsp `exp_smooth`, EXP-capable). SVF
  modes take moving f without clicks (coefficients every 32 samples, ramped, §2.5). The FIR kernel is
  re-blended every 32 samples and crossfaded over the block (§2.5); the neighbours change only at 0, 50
  and 100 %, where the outgoing FIR's weight is zero.
- **`bypass`**: nih-plug's bypass parameter, 10 ms crossfade. The body's tail is short (< 1 s), so no
  trails mode is needed.

### 2.11 Latency and CPU

- **Reported latency 0.**
  - Every stage is causal and starts at sample 0. SVF modes, the IIR source filters, the crossover Tree
    and the filterbank have only dispersion. The FIR is direct-form with the onset trimmed.
  - The shifter's 3 ms centre is part of the doubled voice (gfx-detune's precedent).
  - The bench skips the correlation-peak latency check (a body has no single peak) and runs its own onset
    test (§5).
- **CPU per mono sample at 48 kHz [A; the FIR row is measured, P]**:

  | Stage | Cost |
  |---|---|
  | Source (3 biquads) + top shelf | ≈ 10 ns |
  | Decay (crossover Tree, 4 sliding maxima, gain rule every 8 samples) | ≈ 30 – 40 ns |
  | Modal bank, 48 SVFs with two taps, f64, SIMD × 4 | ≈ 40 – 50 ns |
  | FIR, one blended 1,024-tap kernel, f32 AVX2 FMA | **36 – 53 ns [P]** (twice that while `mic` moves) |
  | Comp, low_cut, level, mix | ≈ 20 ns |
  | **Defaults (twelve off, room 0)** | **≈ 0.15 – 0.2 µs ≈ 0.7 – 1 % of a core** |
  | + twelve (28 complex bands + shifter) | + 0.15 – 0.25 µs |
  | + room (taps + 8-line FDN) | + 0.08 µs |
  | Worst: 2 / 2, twelve, room, during a model crossfade with `mic` moving | ≈ 0.9 µs ≈ 4.5 % |

  - **High rates.** The FIR's cost per second grows with the square of the rate (taps × samples): 4× at
    96 kHz, 16× at 192 kHz, i.e. 148 ns × 192,000 ≈ 2.8 % of a core per kernel and about 6 % for 2 / 2 at
    192 kHz [P]. Accepted: golem runs at 48 kHz. Without AVX2 the kernel falls back to SSE (about twice
    the time).
- **Memory**: data tables of about 80 KB (6 models × 3 FIRs × 1,024 × f32, plus mode tables). Allocated
  in `initialize` only.
- **Denormals**: the 48 modes ring down for seconds. FTZ / DAZ are set in `process` (nih-plug's guard),
  and the bank is flushed to zero once its energy passes −200 dB.

### 2.12 What is reused, what is new

| Piece | From | Use here |
|---|---|---|
| Source network values, EMF inverse | gfx-fuzz §2.2 → **new gfx-dsp `pickup`** (shared) | §2.1, §2.2 |
| `svf` (TPT band-pass, high-pass), `matched` | gfx-dsp | Modes, `low_cut`, D(s), shelves |
| `crossover` `Tree` | gfx-dsp (designed by gfx-bassdi / gfx-grit) | Decay bands |
| `envelope::SlidingMax` | gfx-dsp | Decay detectors |
| ERB complex filterbank, z² / \|z\| | gfx-octaver → **gfx-dsp `filterbank`** (planned there) | 12-string octaves |
| `pitch::CenteredShifter` | gfx-dsp (gfx-detune) | 12-string detune / strum |
| `onset` | gfx-dsp | Quack tamer |
| `Comp` engine, `colour: false` | gfx-comp (library), as gfx-bassdi | One-knob comp |
| `reverb::Taps`, `Fdn` | gfx-dsp (gfx-reverb) | Room |
| `smooth`, `exp_smooth` | gfx-dsp | Parameters |
| **New: gfx-dsp `fir`** | — | Direct-form SIMD FIR, kernel blend and block crossfade; resampling of tables at `initialize` |
| **New: gfx-bench `body-fit`** | — | Offline: H1 estimate, mode fit, residual FIR, table generation (§2.6) |
| **New: model tables, decay rule and voicings k_b, D(s), K table** | — | This crate |

## 3. Parameters

New block: no catalogue ids to keep. Shared ids used with their shared meaning: `model`, `mix` (law
stated), `gr`, `bypass`, `level` (as gfx-emet / gfx-bassdi), `low_cut` (gfx-delay's id). **`pickup` is
gfx-fuzz's id with the same first four options in the same order, and `Piezo` appended here only**
(decided at review: gfx-fuzz's list does not change; golem writes the rank, and a piezo lane sets
gfx-fuzz to Active). golem sets it at insert from the lane's instrument, for both pedals. Ranges are
linear (contract); enums only grow at the end.

### Front panel

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `model` | Model | e | Dreadnought · Jumbo · Concert · Parlor · Classical · Round-back | Dreadnought | 30 ms crossfade | | The body (§2.6); owns its modes, high band, decay and K |
| `body` | Body | f | 0 – 100 % | 50 % | 20 ms (dB) | ✓ | **Relative: 50 % = the model's own**; the modal bank (the boom, the breath) at (body − 50) × 0.24 dB, ±12 dB |
| `top` | Top | f | 0 – 100 % | 50 % | 20 ms (dB) | ✓ | **Relative: 50 % = the model's own**; high shelf at 2.5 kHz on F̂ (before the body), ±10 dB: attack and sparkle (AC-3's Top), and the trim for a guitar that is not the `pickup` guess (§2.1) |
| `mic` | Mic | f | 0 – 100 % | 75 % | 20 ms | ✓ | Microphone blend: 0 Bridge · 50 Sound hole · 100 Neck (12th – 14th fret), crossfading the two neighbours (§2.5). The default is the studio's usual spot, between sound hole and neck |
| `mix` | Mix | f | 0 – 100 % | 100 % | 20 ms | ✓ | **Law: linear crossfade**, 0 = the DI, 100 = the acoustic. EXP: morph from electric to acoustic mid-song |
| `twelve` | 12-String | b | off / on | off | 20 ms fade | | Adds the octave and unison courses (§2.4) |

### Expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `pickup` | Pickup | e | Single Coil · Humbucker · P-90 · Active · Piezo | Single Coil | 30 ms crossfade | | The source (§2.1, §2.7). gfx-fuzz's id and order; golem sets it at insert |
| `pickup_pos` | Pickup Position | e | Neck · Middle · Bridge · Neck + Middle · Middle + Bridge | Neck | 30 ms crossfade | | Position shelf R(s) (§2.2). Inert for Piezo |
| `size` | Size | f | 70 – 140 % | 100 % | 30 ms | ✓ | Body scale: every mode at f · 100 / size. 100 % = the model |
| `decay` | Decay | f | 0 – 100 % | 50 % | 30 ms | | **Relative: 50 % = the model's own** string-sustain shortening (k_b × `decay` / 50); 0 = the electric's sustain, bit-exact; 100 = double |
| `piezo` | Piezo Blend | f | 0 – 100 % | 0 % | 20 ms | ✓ | Adds the simulated (or real) piezo voice to the mic (§2.8) |
| `quack` | Quack Cut | f | 0 – 100 % | 0 % | 30 ms | | Piezo source only: the dynamic dip on attacks (§2.7); inert otherwise (golem greys it through `depends`, as `pickup_pos` for Piezo and `width` in 1 / 1) |
| `low_cut` | Low Cut | f | 20 – 300 Hz | 20 Hz (off) | 30 ms | | 2-pole high-pass after the body: band mix, feedback |
| `comp` | Comp | f | 0 – 100 % | 0 % | 20 ms | | One-knob Opto compressor, auto make-up (§2.8) |
| `room` | Room | f | 0 – 100 % | 0 % | 20 ms | ✓ | The small room around the mic (§2.8) |
| `width` | Width | f | 0 – 100 % | 40 % | 20 ms | | 2 / 2 only: the two mics' spacing (§2.9); inert in 1 / 1 |
| `twelve_level` | Courses Level | f | −12 – +6 dB | 0 dB | 20 ms | | Octave and unison strings relative to the main courses |
| `twelve_detune` | Courses Detune | f | 0 – 20 cents | 4 cents | 30 ms | | Course detune (shifter ratio) |
| `twelve_delay` | Strum Offset | f | 0 – 10 ms | 2.5 ms | 30 ms | | Delay of the course voices on top of the shifter's 3 ms centre and the octave bands' own 5 – 9 ms (§2.4) |
| `level` | Level | f | −24 – +12 dB | 0 dB | 20 ms | ✓ | 0 dB = the bypass loudness on every model (§2.8) |
| `gr` | Comp GR | f, read-only | 0 – 30 dB | — | — | | Shared meter id |
| `bypass` | Bypass | b | off / on | off | 10 ms crossfade | | Shipped bypass parameter |

- No default depends on another parameter. `pickup` and `pickup_pos` are written by golem at insert when
  the lane's guitar is known (the DI index does not name pickups yet: gfx-fuzz open question 2).
- `twelve_*` defaults are not neutral, but they act only when `twelve` is on, and never depend on it.
- `Neck + Bridge` is not offered: on most guitars it is a Telecaster-style series / parallel pair whose
  comb cannot be averaged well [A]. It can be appended.
- Expert groups (`visuals/data/groups.json`):
  - **Guitar**: `pickup`, `pickup_pos`;
  - **Body**: `size`, `decay`, `piezo`, `quack`;
  - **12-String**: `twelve_level`, `twelve_detune`, `twelve_delay`;
  - **Output**: `low_cut`, `comp`, `gr`, `room`, `width`, `level`.

### Visual

- **Block and category (proposal for golem-rack)**: type `acoustic-sim`, **category `amp`**:
  `{"type": "acoustic-sim", "name": "Acoustic Sim", "category": "amp", "roles": ["guitar"], "short":
  "Acoustic", "color": "#552210", "note": "Full rig: DI → miked acoustic guitar. No amp or cab after it
  (like Head+Cab). Put it first after the gate; comp / EQ / reverb after"}`.
  - It takes the amp's place in the lane, as "Head+Cab (NAM)" does ("no cab block needed after it").
  - `amp` is offered to guitar, line and bass lanes, never to voice, keys or drums, which is right for
    this block.
  - `preamp` would be offered on voice and drum lanes; `cab` would invite an amp in front of it.
  - "roles" gains "bass" with the Upright model (phase 2).
- **Crate hue: rosewood `#552210`** (`METAL_CRATE_HUE['gfx-acoustic']`). The `amp` family has no
  METAL_HUE (its blocks are drawn as amps, not metal pedals), so the hue is the crate's.
  - Hue 16°, saturation 68 % (HSL), lightness 20 %: a dark, saturated wood, with no grey mixed in.
  - ΔE*ab [A] to the nearest hues in use: gfx-fuzz copper `#b0602a` **36**, dynamics slate 47, gapmeter
    grey 48, thrash red 55. gfx-tape's review treated ΔE 26 to copper as too close; 36 sits apart.
  - Alternatives that were rejected: a spruce or "natural" top sits on tape's champagne `#d8c8a0`;
    mahogany `#8b4513` is ΔE 12 from copper; abalone teal `#0f9f8f` is ΔE 24 from the filter cyan.
  - `RUNE_LIGHT['gfx-acoustic'] = '#ffc46b'`: a warm amber glow, as a lit sound hole. The body's relative
    luminance is 0.03 (L* 20) [P], so the ink must be light.
  - **Decided at review: rosewood `#552210`.** Re-computed [P]: ΔE*ab 36 to gfx-fuzz's copper, 65 to
    gfx-tape's champagne, 62 to the drive orange `#e04a00`, 55 to the thrash red, 62 to Head+Cab's
    `#ff5a3c` in the same drawer section. A lighter rosewood (`#6b2a12`, `#74300f`) falls to 27 and 22 from
    copper, inside the 26 that gfx-tape's review refused: the wood must stay dark. Its one close
    neighbour is black (ΔE 36 to `#101010`): to check on the family sheet next to the blackwork
    companions, where the amber rune and the rosette do the telling apart.
- **Finish: anodized** (the default; an acoustic is not a thrash pedal; blackwork belongs to the
  high-gain companions). The engraving reads light on the dark wood.
- **Motif `rosette`** (a new case in `motifParts`): the sound-hole rosette, drawn **around** the rune,
  not on both sides of it (unlike every other motif). With r₀ = ½ · hypot(cw, ch) + 3, centred on
  (cx, cy):
  - `line` circles at r₀ and r₀ + 2.5 (opacity 1 and 0.6);
  - a **herringbone ring** between r₀ + 3.3 and r₀ + 7.2: 36 short strokes every 10°, alternately slanted
    ±25° from the radius, joined as one zig-zag `line` (opacity 0.8);
  - `line` circles at r₀ + 8 and r₀ + 10.5 (opacity 0.6 and 1);
  - full circles via `arcPath(cx, cy, r, 0, 359.9)`; the face's clip cuts what leaves the panel;
  - the rings pass **behind** the knobs and the selector (the motif layer is drawn first); if r₀ + 10.5
    reaches a knob's skirt on the four-knob face, the two outer circles are dropped, not the herringbone;
  - engraved on anodized; yellow lined in black if thrash is forced.
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-acoustic": {"name": "Acoustic", "blocks": ["acoustic-sim"], "knobs": ["body", "top", "mic", "mix"],
                   "selectors": ["model"], "toggles": ["twelve"], "category": "amp", "color": "#552210"}
  ```
- **Glyph** (`visuals/glyphs/acoustic-sim.svg`): a guitar body (two bouts), neck, sound hole and bridge.
  Rendered and checked at 240 px and 24 px:

  ```svg
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>Acoustic simulator (gfx)</title><path d='M12 1.5V5'/><path d='M8.86 11.48A4 4 0 1 1 15.14 11.48A5.5 5.5 0 1 1 8.86 11.48Z'/><circle cx='12' cy='9.4' r='1.6'/><path d='M10 17.2h4'/></svg>
  ```
- No maker's livery, logo or trademark on the pedal; model and preset names are generic.

## 4. Applications

Reference settings become block presets (`presets/gfx-acoustic.json`), level-matched to the bypass
(±2 LU). Unless stated: `pickup` Single Coil, `pickup_pos` Neck, `mix` 100 %.

| Use / preset idea | Settings to start from | Feature that matters |
|---|---|---|
| **Studio Dreadnought** (strummed chords in a band) | Dreadnought, `mic` 75, `comp` 25, `room` 15 | Modal lows, measured high band, decay |
| **Big Jumbo Strum** | Jumbo, `body` 60, `mic` 65, `comp` 30 | Deeper triplet, more boom |
| **Fingerstyle Concert** | Concert, `mic` 85, `decay` 60, `top` 55, `comp` 20 | Decay shortening between picks; the measured (IEM) body |
| **Single-Note Lead** (melody, long notes) | Dreadnought, `decay` 70, `mic` 70, `room` 15 | Where the decay stage acts fully: the long electric sustain is gone |
| **Parlor Blues** | Parlor, `top` 60, `body` 40, `room` 20 | Small-body midrange |
| **Nylon Bossa** | Classical, `top` 35, `mic` 80, `room` 20 | Archive-fitted nylon body; nylon decay |
| **Twelve-String Jangle** | Dreadnought, `twelve` on, `twelve_detune` 4, `twelve_delay` 2.5, `comp` 35 | Poly octave courses, strum offset |
| **Stage Acoustic** (loud band, no feedback) | Dreadnought, `piezo` 40, `mic` 60, `low_cut` 100, `comp` 35 | Piezo + mic blend, low cut |
| **Round-back Stage** (the 80s stage sound) | Round-back, `piezo` 60, `top` 60 | Subdued body, piezo cut |
| **Piezo Restore** (an acoustic-electric's DI) | `pickup` Piezo, the matching body (Concert is the measured one), `mic` 75, `mix` 70, `quack` 40 | Force → reference D(s), body, blend like Aura |
| **Electric → Acoustic** (EXP) | Dreadnought, EXP on `mix` 0 → 100 % | Crossfade with no bulk delay: no comb mid-sweep |
| **Wide Doubled Acoustic** (2 / 2) | Concert, `width` 80, `room` 25 | Two mics from one modal bank |
| **Bridge-Pickup Rescue** | `pickup` Humbucker, `pickup_pos` Bridge, Dreadnought, `top` 45 | Position shelf (up to +13.8 dB of lows re the treble), aperture shelf |
| **Small-Body Morph** | Dreadnought, EXP on `size` 140 → 80 % | Modes glide; no click |
| **Ringing Sustain** (ambient, keep the electric's sustain) | Concert, `decay` 0, `room` 30 | Decay is a choice |

**Playing advice for the preset notes** (from §1.5): neck pickup, guitar tone a little rolled off for
humbuckers, a lighter touch than on an amp, palm-mute long notes when `decay` is low.

## 5. Bench suite (on top of gfx-bench's common checks)

**gfx-bench options**: `skip_latency` (a body has no correlation peak; the onset row measures 0).
`click_bound_db` at the default (−40 dB): every stage is linear or slow. Silence floor −140 dBFS: with
F̂ = 0 the modes ring down to exactly 0 after the f64 flush.

**DIs needed**: golem's clean.wav (6-string; pickup to be named) and, for the real test, **phrases played
twice by the maintainer, on an electric (neck single coil, DI) and on a steel-string acoustic (miked as in §2.6)**.
Until then the rows run on clean.wav and synthetic plucks.

| Check | Pass |
|---|---|
| **Source invariant** (gfx-dsp `pickup`, shared with gfx-fuzz) | Each `pickup`, on the analog prototype without the band limit: the EMF inverse followed by the recording network = unity within 0.01 dB. As run here: inverse × network = the two poles' Butterworth response at f_p within 0.2 dB, 20 Hz – 16 kHz; the lifts of §2.1's table within 0.3 dB |
| **Source noise and guess** | −100 dBFS white noise in, defaults: output hiss reported per `pickup` (A-weighted lift within 1 dB of §2.1's column). §2.1's wrong-guess table reproduced, and each row brought within ±2 dB at 3 – 10 kHz by a `top` setting (report it) |
| **Position shelf** | `pickup_pos` responses equal R(s) within 0.2 dB; Bridge − Neck = +11.4 dB at 50 Hz, 0 dB ± 0.3 above 3 kHz |
| **Onset / latency** | Impulse in, `mix` 100: first non-zero output at sample 0; the largest sample of the response above 1 kHz within the first 0.25 ms; its energy centroid reported (a 21 ms noise-like tail has one of a few ms by nature); `mix` 0 bit-exact dry |
| **Body vs its data** | Each model and each fitted mic: modal + FIR response vs the measured target, 1/6-octave smoothed, within ±1.5 dB at 70 Hz – 10 kHz; mode frequencies within 1 %; T60 per 1/3 octave below 1 kHz within 15 % |
| **Held-out playing** (measured models) | Test take (§2.6, never fitted): the sound-hole pickup's EMF through the model vs the mic, long-term 1/3-octave spectra within ±2 dB at 80 Hz – 8 kHz, per mic |
| **Piezo cross-validation** | IEM session 2 (never fitted): piezo through Concert + D vs the bridge and neck C414s, 1/3-octave within ±2.5 dB at 80 Hz – 8 kHz; the 1 – 3 kHz quack region within ±1.5 dB. The two NTI reference mics against the C414s: bands where they disagree by more than 3 dB below 300 Hz are listed as room, not body |
| **Mic interpolation** | `mic` at 0, 50, 100 %: the fitted responses within 0.1 dB. At 25 and 75 %: equal to the 50 / 50 sum of the two neighbours' outputs within 0.1 dB (it is a blend), and its 1/3-octave level above 1 kHz within ±1 dB of the neighbours' mean (the ρ correction). Swept 0 → 100 % in 2 s under pink noise: no zipper sidebands > −80 dB (EXP rule, 50 Hz steps) |
| **Size** | `size` 70 / 100 / 140 %: lowest modes at f · 100 / size within 0.5 %; sweep under a strum: no click above the bound |
| **Decay: rate and attack** | Synthetic plucks (82 – 660 Hz, partials to 8 kHz, exponential decays as an electric's, T60 4 s at 100 Hz to 1 s at 4 kHz): per band T60 out = T60 in / (1 + k_b) within 10 %; first 5 ms of each pluck within 0.2 dB of `decay` 0 (prototype: 0.0) |
| **Decay: never worse than the DI** | Gain ≤ 0 dB always. A note 12 dB softer, 0.6 s after a loud one in the same bands: its first 20 ms within 8 dB of `decay` 0 in every band (prototype: 3 – 7 dB in two bands, 0 elsewhere; the first draft: 10 – 39 dB). A note of equal level: within 0.5 dB |
| **Decay: ripple, beats, noise** | Sustained 82 Hz and a dyad E2 + B2: gain ripple at the notes' periods ≤ 0.3 dB in every band; energy off the harmonic series raised by ≤ 4 dB (prototype: 3 dB). Two partials 2 Hz apart in one band: beat depth ≤ (1 + k)× the input's + 1 dB; 8 Hz apart: ≤ the input's + 2 dB. Pink noise alone at −80 and −60 dBFS: gain change ≤ 1 dB over 10 s (the prototype fails this at −60 dBFS: the hysteresis is set here). `decay` 0: bit-exact pass-through |
| **Decay: chords** | Strummed E chord, then a single note in the same band at +0.5 s: the new note's attack within 0.5 dB of `decay` 0; the step on the older note's partials at that onset reported, and ≤ the 24 dB cap. An arpeggio at 120 BPM in eighths against the same notes played alone: the shortening left between picks reported per band |
| **Twelve: tuning** | Single notes E2 – G3: octave voice at 2 f₀ ± 1 cent before the shifter; after it, the detune within ±0.5 cent of `twelve_detune`; strum offset = `twelve_delay` + 3 ms ± 0.1 ms |
| **Twelve: chords** | Open E and G chords: every component of the octave voice listed; those that are neither an octave of a chord partial nor a sum of two of them ≤ −40 dB re the voice. The sum tones' level reported re the octaves (expected within 6 – 12 dB of them on close partials), with the B-string excess. The octave voice's delay re the main courses reported per band (expected 10 – 15 ms at the defaults) |
| **Level** | Defaults, clean.wav: every model within ±1 LU of the bypass; `body` 0 → 100 % within ±4 LU; factory presets within ±2 LU |
| **Compressor** | `comp` 0 bit-transparent; static curve within 0.2 dB of §2.8; `gr` within 0.5 dB |
| **Switching** | `model`, `pickup`, `pickup_pos` and `twelve` switched under a ringing chord: no click above the bound; CPU during the fade within the stated worst case |
| **Stereo** | 2 / 2, `width` 0: L = R bit-exact = the 1 / 1 output; `width` 80: inter-channel coherence below 500 Hz ≥ 0.9 (mono-safe lows), mono sum within ±1.5 dB of the 1 / 1 output (1/3 octave) |
| **Sample rate** | 44.1 / 48 / 96 / 192 kHz: body responses within 0.5 dB below 15 kHz; mode frequencies identical |
| **Determinism, CPU** | Strict null between two renders (room seeded); ≤ 0.3 µs / sample mono at defaults, ≤ 1 µs worst (§2.11), 48 kHz; the cost at 96 kHz reported. `size` swept by EXP: no more than +20 % over the defaults |
| **Listening** | Blind, level-matched (ABX and preference) on the paired electric / acoustic phrases: the sim against the real acoustic at the same mic, **in a band mix and solo**; against an IR-only chain (the same FIR without the modal bank, decay or position shelf) to show each feature's worth; against an AC-style parametric reference (owner's choice). **`decay` on and off on an arpeggio, a fingerstyle piece and a single-note melody**: if the re-opened tails are heard as breathing, the fallback of §2.3 applies. `twelve` on open chords against a real 12-string recording. Voicing values marked [U] / [A] are set here |

## 6. Positioning

| Product | What it lacks that gfx-acoustic has | What it has that we do not |
|---|---|---|
| Boss AC-3 / AC-2 / Helix Acoustic Sim | Measured bodies, pickup inversion, decay, mic position, stereo, 12-string | Nothing structural; a small CPU footprint |
| IR acoustic pedals, Fractal Tone Match | Pickup inversion and position, decay, tunable lows, mic morph | Any user IR (a later `model` "User" with an IR file is possible, but would need file I/O outside `process`) |
| Fishman Aura (piezo) | A body choice for any piezo guitar, decay, zero latency (Aura: about 3 ms) | 96 images of real instruments, each measured with its own pickup; ours has one measured piezo guitar (Concert) and a physical conversion for the rest |
| Rau 2021 (hex) | Works on any guitar's mono DI | Per-string damping and per-string comb inversion (hex pickup) |
| Real acoustic, miked | — | Finger noise, string material, the comb-free force, the player's grip on the body |

What "best" means here: in a band mix it should pass for an acoustic. Solo, on long notes and fingerstyle,
it should beat every product above. It will not be a real guitar: the comb notches and the missing finger
noise are physics a mono DI does not carry (§1.6).

## 7. Open questions for review — answered

1. **Measurement campaign** (§2.6). Still the owner's: which four steel-string guitars, which sound-hole
   pickup and mics, which room. Decided here: it does not block v1. Concert ships measured from the IEM
   recordings, Classical from the archive, the others parametric, and the held-out rows wait for the
   campaign. The protocol now records x_s.
2. **Category `amp`**: yes. golem's role table already keeps `amp` away from voice, keys, pads and drums,
   and Replace offers the same category, so "swap the amp for an acoustic" is one gesture. For golem-rack:
   `acoustic-sim` is not a capture type (it must not join `CAPTURE_TYPES`), and `rig.stacked_cab` should
   warn on a cab after it, as after Head+Cab. An amp after it is a real use (an acoustic amp) and is
   left alone.
3. **`pickup` shared with gfx-fuzz**: one value table in gfx-dsp `pickup`, the same first four options in
   both pedals, `Piezo` appended here only. gfx-fuzz's README does not change.
4. **Decay on chords and beats**: prototyped. As first written it failed (ripple, eaten soft notes); the
   revised rule passes on synthetic plucks and has known, bounded limits (§2.3). The fallback is
   specified there. A static damping is not a fallback.
5. **12-string octave range**: fixed at 400 Hz, no parameter. The B-string excess and the chords' sum
   tones are accepted and benched (§2.4).
6. **CC BY 4.0**: compatible; notice text and places given in §1.4. gnu.org's entry to be opened once by
   the owner (the site refused the review's requests).
7. **Mores near-field mics**: a fixed tilt toward the IEM classical guitar's studio-mic spectrum (§2.6).
   The Classical model's `mic` keeps the same three names.
8. **Upright bass**: phase 2, as proposed. Rau 2018's own material is not licensed for reuse [U]; it needs
   our measurement.
9. **Pick / finger-noise layer**: not in v1. Prototype only if the solo listening row names the missing
   noise before anything else.
10. **Hue**: rosewood `#552210`, anodized, motif `rosette` (§3, Visual).

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good**, and Strong on single lines and in a band mix once the decay stage passes its listening
row and two bodies are measured. The architecture is right, and nobody ships it: pickup inversion, the
string's own physics for the pickup position, a tunable modal low band under a short measured FIR, and
the electric sustain shortened. The survey's key sources check out (the 1999 study, Rau 2018, the archive's
method file, the IEM paper were re-read). The licences are clean. What did not hold: the decay detector
and its peak memory, the √2 in the position shelf, one band limit for three pickups, the claim that a
wrong pickup guess is harmless here as in gfx-fuzz, real mode gains, "the mic moves", and a D(ω) measured
against a model that was itself a guess. All are fixed above; none changes the architecture.

**The file on disk was coherent** when reviewed: one status line, twelve design subsections, no
duplicated or truncated part, no foreign edit visible (663 lines, untracked in git, so no diff to
compare with).

Prototype (`proto-gfx-acoustic/` in the review scratchpad: `pickup.py`, `pickup2.py` the inverse, its
noise, the wrong-guess table and the position shelf against the exact six-string average; `decay.py`,
`decay2.py` the decay stage as written and as revised; `fir.c` the direct FIR; colour distances inline).
Figures marked [P] come from it. The plucks are synthetic: they size the decay stage's faults, the real
DIs and the ear settle it.

| # | Change | Why |
|---|---|---|
| 1 | **Decay stage rewritten** (§2.3): 25 ms sliding maximum in every band + 60 ms release smoothing; a reduction that grows k dB per dB of fall and re-opens on a 3 dB rise, instead of a peak memory; cap 24 dB (was a −40 dB floor); rule "never worse than the DI"; a real fallback | As written: 1.5 – 6.7 dB of gain ripple at the note's period in the three upper bands (windows shorter than the note's period), the attack cut by 1 – 10 dB in the top two, a note 12 dB softer than the previous one cut by 10 – 39 dB, intermodulation on a dyad 11 dB up [P]. Revised: T60 ratio on target, attack at 0.0 dB, ripple ≤ 0.2 dB, 3 dB of intermodulation [P]. It is realisable causally at zero latency. On polyphonic material it cannot be artefact-free in mono: a new note brings the older notes' tails in its bands back to their DI level, and slow beats are deepened by (1 + k). Both are bounded and stated |
| 2 | **Position shelf: corner ω_x = c / (√2 · x_p)**; table recomputed (152 / 243 / 592 Hz, reference 121 Hz); x_s measured, not assumed; per-string spread stated | The comb's power average is ½: force = √2 · Z₀ · rms velocity above the notches. The +13.8 dB low-frequency figure for the bridge pickup is right (x_s / x_e = 200 / 41), and the first-order shelf is within ±1 dB of the exact six-string average [P]. Per string the corner runs over two octaves |
| 3 | **Band limit per pickup**: f_p = min(12 kHz, 3 × resonance); lift table re-computed | 12 kHz is right for a single coil. For a humbucker and a P-90 it lets the inverse climb to +24 and +27 dB and lifts the interface's hiss by 16 and 20 dB(A) [P]. The draft's figures were also 0.4 – 0.5 dB off gfx-fuzz's (which the prototype reproduces: +3.5 dB at 4 kHz, −14.2 dB at 10 kHz) |
| 4 | **The wrong-guess table and `top` as its trim** (§2.1); bench row | gfx-fuzz's invariant ("a wrong guess cannot colour the sound") holds there because its netlist takes the inverse back. Here nothing does: a 250 or 900 pF cable is ±5 dB at 6 – 10 kHz, a humbucker read as a single coil −7 dB [P]. Consistent with gfx-fuzz's review in method, different in consequence |
| 5 | **gfx-dsp `pickup` = analog prototype**, discretized by each client; the invariant row rewritten | gfx-fuzz needs bilinear at its oversampled rate with 20 kHz poles, this pedal a matched design at the host rate. "Within 0.1 dB to 10 kHz" could not pass with poles at 12 kHz (−1.7 dB at 10 kHz) |
| 6 | **One reference for F̂** (the sound-hole EMF), and **D(s) = force → reference** as fixed physics | The steel-string bodies are measured from a pickup's EMF and the Classical one from a hammer's force: the same F̂ could not feed both. The piezo's "D(ω)" was the IEM response divided by a parametric Concert |
| 7 | **Concert v1 fitted from the IEM recordings** (was parametric) | A public-domain piezo + two studio mics on a steel-string is a measured body. Concert + Piezo is then exact by construction, and the cross-validation row tests a fit, not a guess |
| 8 | **Classical fit specified**: poles from the bridge accelerometers, gains from the mics before the first reflection, a tilt from the IEM classical guitar; guitars g07, g30, g62 excluded | The archive's method file: 12 – 36 ms reflection-free, raw data not normalized, three doubtful instruments. The archive's contents were corrected in §1.4 (65 numbered guitars, mic geometry, 32.7 MB) |
| 9 | **Two gains per mode and mic** (a complex residue) | Real gains cannot hold the phase a 10 cm path difference makes near 1 kHz |
| 10 | **`mic` is a blend of fitted microphones**, said as such; FIR blend level-corrected by ρ; the "monotone" bench row replaced | Interpolating gains is a crossfade of two mic signals (Rau's own words: "mix between"). A linear blend of two partly uncorrelated tails loses up to 3 dB half-way |
| 11 | **One blended FIR kernel per channel** instead of two; CPU table with the measured figure; high-rate cost stated; `size` coefficients every 32 samples; decay gain every 8 | A crossfade of two FIRs is one FIR. Measured 36 – 53 ns per 1,024-tap kernel [P]: the draft's 80 – 130 ns for two was honest, and the 0.25 µs total holds (now ≈ 0.15 – 0.2 µs). 48 `tan` per sample under an EXP sweep of `size` would not |
| 12 | **Onset bench row**: peak within 0.25 ms, centroid reported | "Energy-weighted group delay ≤ 1 ms" cannot pass: a noise-like 21 ms tail has a centroid of a few ms by nature |
| 13 | **12-string: sum tones, band delay and brightness stated**; bench row rewritten | gfx-octaver's bands are one complex pole each (read in its code): on chords z² adds f₁ + f₂. "No pair of partials sharing a band" was not true. Accepted: sum tones of chord notes sit on or near chord notes, under a detuned voice |
| 14 | **Licences settled** (§1.4), notice text written, raw data kept out of the repository | Zenodo's record says cc-by-4.0; Phaidra says Public Domain Mark 1.0 and the project page says it in words |
| 15 | Facts: the 1999 FIR is "order 750" with no sampling rate in the text; Rau 2018's method described as read; Taylor 612ce and Hanika 59PF named in the data table only; luminance 0.03, not 0.08 | Re-read |
| 16 | **Visual decided**: rosewood `#552210`, anodized, `rosette`; ΔE re-computed | 36 from copper, 65 from champagne, 62 from the drive orange; lighter woods fall inside the distance gfx-tape's review refused |
| 17 | Presets: Single-Note Lead added; Bridge-Pickup Rescue trims `top` | The decay stage's best use had no preset |

Checked and left as written: the EMF inverse and gfx-fuzz's values; F_n / v_n = Z₀ / sin(k x_p) and the
comb's notches (334 Hz on a Strat's low E); the aperture shelf (nulls at 2.9 and 11.5 kHz); modal below /
FIR above, with the residual defined by subtraction; TPT SVF modes; direct-form FIR at zero latency;
reported latency 0 with `skip_latency`; the measurement protocol (H1, coherence, held-out take); the
compressor and room; ids, linear ranges, relative `body` / `top` / `decay`, no mode-dependent default;
mono first; `mix` law; generic model names; the glyph.

Coverage: against the AC-3, the IR pedals, Aura and Helix, nothing a player uses is missing. Left out on
purpose, first candidates to append: a `User` body (IR file), `Upright`, a finger-noise layer, a second
measured piezo curve.

Not re-verified at review (as the survey marks them): the Fishman, LR Baggs, Helix, Fractal and forum
quotes; Rau 2021; the DAFx-26 paper; the Rossing mode tables.

Still open:

| Point | Owner |
|---|---|
| The decay stage on real playing: the listening row of §5 (arpeggio, fingerstyle, melody). First coding task, before any voicing; fallback in §2.3 | golem-fx, then the maintainer by ear |
| Measurement campaign: four steel-string guitars, a sound-hole single coil (L, R, x_s), three omni mics, the room; the paired electric / acoustic phrases | The maintainer |
| Open gnu.org's licence list at `#ccby` once; choose the archive's guitar (gNN) and put it in the notice; add the `NOTICE` file to the bundle and the credit to golem's about page | The maintainer; golem-rack |
| The IEM fit: how much of the CUBE's room sits in the low modes; if too much, Concert stays parametric below 300 Hz until the campaign | golem-fx, on the bench |
| x_p per `pickup_pos` [U], the pickup values [U], k_b per model, the piezo voice's bell, the noise floor of the decay: on the bench with real DIs | golem-fx |
| Catalogue entry `acoustic-sim` in category `amp`; `stacked_cab` warning; `pickup` and `pickup_pos` written at insert from the lane's instrument (gfx-fuzz asks the same); `depends` for `quack`, `pickup_pos`, `width`, `twelve_*` | golem-rack |
| gfx-dsp: who creates `pickup` (with gfx-fuzz), `crossover` (with gfx-bassdi / gfx-grit), `filterbank` (moved out of gfx-octaver), `fir` | golem-fx |
| Rosewood next to the blackwork companions at tile size; the rosette as the first motif drawn around the rune | The maintainer, on the family sheet |
| Add `level` to the contract's shared-id table (asked by gfx-fuzz, gfx-dist, gfx-geq too) | The maintainer (contract) |
