# gfx-radio

> **Status: DESIGNED** (2026-11-14), **reviewed** (Fable 5.2, 2026-11-14: see the Review section at the end).
> No code yet.

golem's **small-speaker and transmission pedal**. It covers the telephone, the cell phone, the megaphone, the
walkie-talkie, the AM radio, the station PA, the intercom, the pocket transistor radio, the gramophone horn
and the laptop speaker. It is for voice first: golem's `voice` lane takes the `lofi` category as its only
saturation, so this pedal is the voice's distortion box. It also serves guitar intros, "through the radio"
breakdowns, keys and drum loops.

The idea is to **simulate the device chain, not draw a band-pass**. Each model runs the same chain:
a microphone, then a transmitter and channel, then a receiver amplifier, then a loudspeaker, then the room
where the listener stands. Each stage is physical and has zero latency:

- **Microphone**: a carbon-button mic (the sandwich model), a close dynamic mic or a crystal element.
- **Channel**: the channels with radio frequency (RF) carriers (AM, narrowband FM) are simulated in
  **complex baseband**. Tuning, detuning distortion, heterodyne whistles, selective fading, FM threshold clicks
  and the squelch tail therefore come out of the simulation. They are not samples or envelopes laid on top.
  The cell phone channel is a **backward-adaptive speech coder** on 8 kHz samples: a real LPC analysis /
  synthesis pair whose coefficients come from the frame that just ended, with packet-loss concealment that
  repeats what has already been played. Nothing needs lookahead.
- **Loudspeaker**: a Thiele-Small small-signal response, plus a **Klippel-style large-signal motor**
  (Bl(x), K(x) and a rattle at maximum excursion), plus a horn throat's air nonlinearity. The motor is a
  2-state ODE, and its linear limit is the speaker's box high-pass (within 0.002 dB at 2× [A, prototype]).

Clichés players hate are avoided by design (§1.4). The band edges are not static brick-walls. The noise does
not hiss on during silence. The level does not drop when the band narrows. The level stays matched to the
bypass, and `band` and `mix` keep the sound intelligible in a mix.

It is **not** a converter (gfx-crusher), not a medium (gfx-tape), not band-local dirt (gfx-grit), not a
static EQ curve (golem's eq-graphic "Hygiaphone" / "Lo-fi radio" presets) and not an impulse-response player
(gfx-cab). §6 places it among them.

Mono 1 / 1 first, 2 / 2 offered (§2.9). **Reported latency 0.** The wet carries some minimum-phase group delay
from the IIR filters and the oversamplers' half-bands (gfx-dsp's are polyphase all-pass IIR: no pure delay;
a few samples, plus about 1 ms near the 300 Hz edge [A]). Nothing looks ahead.

## 1. State of the art

Survey of 2026-11-14. **[V]** = read in a source, a manual or code. **[I]** = inference, or a fact not
re-checked against a primary source. **[U]** = could not be verified (forum, vendor claim). **[A]** = our
calculation.

### 1.1 Commercial references

| Reference | What it does best | What we take / beat |
|---|---|---|
| **Audio Ease Speakerphone 3** ([manual](https://audioease.com/speakerphone/files/speakerphone-3-manual.pdf)) [V] | Convolution with "hundreds" of sampled speakers (Altiverb engine). Chain: input gain → phono → **radio tuning** → **telecom** → speaker (pre) → crush → gate → compressor → distortion → mod → delay → **mic** → speaker (post) → Leslie → **cover** → **room** → EQ → limiter. Telecom = real GSM-type LPC codec with dropped data ("repetitions, underwater effects"); some codec types add latency. The radio tuner's horizontal axis worsens reception (distortion + noise) and its vertical axis sets the inter-modulation side-tone pitch, with AM / FM band noise. Mic module has a carbon "DEGRADE" knob. Its **Wet / Dry opens each module progressively** rather than crossfading | The **chain order** (channel → speaker → room, the speaker before or after the dirt), the **tuning axis**, the carbon degrade, a progressive "open up" (our `band`, §2.6). Not copied: its IR library (proprietary, and IRs cannot model level-dependent breakup) and its latency-bearing codecs |
| **McDSP FutzBox** ([vendor](https://mcdsp.com/plugin-index/futzbox/), [Mix review](https://www.mixonline.com/?p=72214)) [V vendor / review] | "Synthetic Impulse Models" (SIMs: compact parametric speaker / transmission models); filter → distortion (fuzz, nuke, clip, rectify) → EQ / filter → SIM → **gate made for dropouts** → lo-fi (bits / rate); **noise generator ducked by the input**; dry / wet; zero latency | Parametric (not sampled) models at zero latency, **noise that follows the signal**, a dropout gate. Beyond it: dropouts and gating that the channel physics produces (squelch, packet loss), not a gate tuned by hand |
| iZotope Trash 2 (Convolve module) [I] | Speaker / phone IRs + filters + distortion in one multiband engine | The IR route belongs in gfx-cab (§6) |
| TC-Helicon VoiceTone X1, VoiceWorks "Transducer" block ([manual excerpt](https://libble.de/tc-helicon-voiceworks-plus/p/808914/?page=0037)) [V excerpt] | Hardware vocal megaphone / telephone (band shape + distortion) for live singers | The live-voice use, one footswitch |
| Line 6 Helix "bullhorn" / megaphone in practice ([Line 6 forum](https://line6.com/support/topic/30798-helix-native-for-vocals-and-non-guitars), [thread](https://line6.com/support/topic/55404-lo-fi-modern-metal-sound/)) [U] | Users praise the scratchy phone-like vocal. One finds it "too heavy on distortion" for guitar because it is voiced for vocals | `drive` defaults to 0 % on top of a lightly driven model, so the same model serves voice and guitar |
| Soundtoys Little Radiator, Waves UM, Decapitator (named in the brief) [I] | Respectively a tube-mixer preamp, an upmixer and a saturator: none is a speaker or channel simulator | Out of scope |

### 1.2 Physics and papers

- **Loudspeaker large-signal behaviour** ([Klippel, transducer nonlinearities](https://www.klippel.de/know-how/measurements/transducer-parameters/transducer-nonlinearities-curve-shape.html),
  [SIM-AUR](https://klippel.de/manuals/simulation-auralisation/simaur/simaur.html)) [V]. The lumped-parameter model
  makes the force factor Bl(x), the stiffness Kms(x) and the inductance Le(x) functions of the excursion x. They
  are given as curves or power series, and simulated in real time for auralisation. The physics follows from
  this [I, standard]:
  - Excursion is largest at and below the resonance fs. A small speaker therefore distorts on **bass content**.
  - The distortion also intermodulates the mids: this is the "breakup" of a pocket radio played loud.
  - A stiffening suspension (K rising with x) compresses the output.
  - Bl falling with x reduces the motor's force.
  - An offset rest position gives even harmonics and a DC excursion.
- **Thiele-Small small-signal model** [I, standard]: in a sealed box the cone acceleration (the radiated pressure)
  is a 2nd-order high-pass at fs with total Q Qts. The nonlinear motor of §2.5 reduces to that high-pass at low
  level.
- **Horn and compression-driver air nonlinearity** ([JBL tech note excerpt via diyAudio](https://www.diyaudio.com/community/goto/post?id=2949663),
  [Robineau & Vaucher, Nexo, AES 1995](https://www.nexo-sa.com/wp-content/uploads/Robineau-Vaucher-1995-Analysis-of-Non-Linear-Distrortion-in-Compression-Drivers.pdf)) [V, abstract level]:
  the main distortion in compression drivers is the **air's own nonlinearity in the throat**, mostly 2nd
  harmonic. For a given level and frequency it falls as the horn's cutoff rises, and a wider throat lowers it.
  The Thuras-Jenkins-O'Neil formula as textbooks give it (Beranek, *Acoustics*: 2nd harmonic in % ≈
  1.73 · (f / f_c) · √I_throat · 10⁻², I in W / m²) is quoted from memory, not re-read [I].
  We use only its shape: 2nd harmonic rising 6 dB / oct with frequency and in proportion to the pressure.
- **Carbon microphone** ([Oksanen & Välimäki, DAFx-2011](https://dafx.de/paper-archive/details/3d2KWaoXozGA8TlhRn9Byg)) [V, abstract]:
  a model built from measurements of a real carbon mic, using the **sandwich model** (linear filter → memoryless
  nonlinearity → linear filter) that MIT Lincoln Lab used for handset distortion
  ([Quatieri et al.](https://www.ll.mit.edu/r-d/publications/estimation-handset-nonlinearity-application-speaker-recognition):
  sigmoidal nonlinearities from a carbon-button model) [V, abstract]. It adds a **filtered self-noise source** and
  an **input-level-dependent noise**. Carbon mics produce strong 2nd harmonics ([US 4476348](https://patents.google.com/patent/US4476348)) [V]
  and "frying" noise while current flows ([Ericsson bulletin, 1949](https://www.britishtelephones.com/ericsson/bulletin/18_16.htm)) [V, excerpt].
  The paper's equations and values were not readable (the PDF link was dead): the curve and noise levels are
  voiced [I].
- **Packet-loss concealment** (ITU-T G.711 Appendix I, described in
  [Opal's g711a1_plc.h](https://archive.reu.dimacs.rutgers.edu/pre2008/reu/2011/janvolec/src/opal-3.6.2/include/codec/g711a1_plc.h)
  and patents) [V, secondary sources]:
  - the lost frame is filled by repeating the last pitch period(s), overlap-added over ¼ period;
  - the first 10 ms are unattenuated, then the level falls 20 % per 10 ms, reaching **silence after 60 ms**;
  - App. I delays the output by 3.75 ms for its overlap-add. We do not (§2.3).
- **FM threshold and clicks** ([All About Circuits](https://www.allaboutcircuits.com/technical-articles/introduction-to-the-threshold-effect-in-fm-systems/)) [V]:
  - below a carrier-to-noise ratio (CNR) of about **10 dB** the discriminator output breaks into **clicks**,
    phase slips of 2π [V];
  - Rice's click theory gives the rate for an unmodulated carrier: N ≈ r · erfc(√ρ), with ρ the CNR in the IF
    noise bandwidth and r = B / (2√3) for an IF of total width B [I, standard: Rice 1963]. For B = 12 kHz that
    is 0.03 clicks / s at 10 dB, 1.3 at 8 dB, 17 at 6 dB and 87 at 4 dB [A];
  - a limiter-discriminator simulated in discrete-time baseband **does** produce these clicks by itself.
    Prototype (§2.3 NBFM as specified, 60 s per point) [A]: 0.0 – 0.1 / 1.0 / 14 / 63 clicks / s at
    10 / 8 / 6 / 4 dB at 48 kHz, on Rice's curve down to 6 dB, and the same within 10 % at 44.1, 96 and
    192 kHz. So "10 dB" is where clicks *begin* (one every 30 s); they are a texture at 6 – 8 dB.
- **Squelch tail** ([repeater-builder](https://repeater-builder.com/tech-info/reverse-burst/reverse-burst.html), [Urgent Comms](https://urgentcomm.com/?p=8502)) [V]:
  when the carrier drops, the receiver's squelch stays open briefly and passes a loud noise burst. CTCSS
  (sub-audible tone squelch) with "reverse burst" removes it. The tail is the noise squelch's own detector time.
- **AM broadcast** ([NRSC-1-C](https://www.nrscstandards.org/standards-and-guidelines/documents/standards/nrsc-1-c.pdf),
  [Canadian study 1980](https://publications.gc.ca/collections/collection_2020/isde-ised/Co24/Co24-449-1980-eng.pdf),
  [Columbia AM receiver overview](https://www.ee.columbia.edu/~kinget/EE6350_S24/AMRADIO/system_overview.html)) [V, abstract level]:
  - **channel spacing**: 10 kHz in the Americas; 9 kHz in ITU Regions 1 and 3 (the Geneva 1975 plan) [I, standard];
  - **audio bandwidth**: the NRSC sets 10 kHz, and some stations run 5 – 6 kHz;
  - **receivers**: cheap ones are narrower, about 2.5 – 3 kHz audio [I];
  - **night-time whistle**: adjacent carriers produce a 9 / 10 kHz whistle, and receivers include filters for it.
  - **Not verified, standard radio theory** [I]:
    - selective fading: a notch from multipath that sweeps across the carrier, so the envelope detector distorts;
    - diagonal clipping: the diode detector's RC is too slow for deep, high-frequency modulation.

### 1.3 Open source

There is no open-source small-speaker / radio simulator of note [I: searched, none found]. The DAFx carbon-mic
paper and Klippel's published model are the method sources. The rest comes from standard communications DSP:
complex baseband, IF filters, envelope and phase detectors, and squelch.

### 1.4 What players and engineers love and hate

- **"A band-pass is not a phone."** Band-limiting plus compression "works, sort of, but not really". Realism
  comes from **dropping to 8 kHz and clipping the peaks**, then automating the cuts for distance
  ([Production Expert](https://www.production-expert.com/production-expert-1/the-secret-to-great-phone-futzing)) [V].
  Free phone plugins stop at steep 6-pole 350 Hz – 3 kHz filters plus bit reduction ([KVR / Rocksonics listing](https://www.kvraudio.com/product/futzbox-by-mcdsp)) [U].
- **Noise must duck under the voice** (FutzBox's noise duck) [V]. A constant hiss under a live rig is hated:
  the same complaint as gfx-crusher's fizz on silence.
- **Intelligibility versus cheese.** The cartoon "telephone" is a static 300 Hz – 3 kHz box with heavy
  distortion. Real phones are cleaner and more dynamic (AGC), and change over time (codec, losses). Mixers
  want the sound **readable** and to "open up" progressively (Speakerphone's progressive wet / dry) [V / I].
- **Megaphone voicings are often too distorted for guitar** (Helix thread) [U]. They also need a mid peak
  around 1.5 – 2.5 kHz ([VoxBooster](https://voxbooster.com/blog/megaphone-voice-effect)) [U, vendor blog: a starting point only].
- **Level drop**: band-limiting removes 6 – 12 dB of perceived loudness on a full-range source. An unmatched
  insert sounds broken [A, by the loudness weighting of the removed bands; bench verifies].

### 1.5 What we conclude

1. **Simulate the chain** (mic → channel → amp → speaker → room), each stage physical. Artefacts must
   *emerge* (FM clicks, the squelch tail, AM detuning distortion and whistles, small-speaker bass breakup)
   rather than be pasted on. That is the step beyond FutzBox's filter / distortion / gate chain.
2. **No IRs.** Speakerphone's library is proprietary, and an IR cannot break up with level. For a specific
   device's IR there is gfx-cab, after this pedal with `speaker` None (§6).
3. **Zero latency everywhere**: codec effects are made from past audio only (a backward-adaptive predictor,
   PLC repeats), with a 20 ms frame clock: the envelope lags the voice by up to one frame, where a real codec
   delays the voice instead.
4. **Musical by default**:
   - loudness matched to the bypass;
   - noise that follows keying (squelch, §2.3);
   - a `band` control that opens or narrows the model around its own voicing;
   - `mix`;
   - `drive` at 0 % on top of each model's own light saturation.
5. **Matrix behind the models**: `model` is a bundle. Expert overrides (`mic`, `channel`, `speaker`, `room`,
   each with `Model` first) let a megaphone play an AM broadcast, or a walkie play into a station hall, as
   Speakerphone's module chain allows.

## 2. Design

```
in ─┬──────────────────────────────────────────────────────────────────── dry ───────────────┐
    └▶ MIC (2×) ▶ TX / CHANNEL ─────────────▶ AMP + SPEAKER (2×) ─────────▶ ROOM / DISTANCE ──┤
       carbon      phone line · G.711 ·       battery rail, crossover,     direct + reflections│
       dynamic     cell codec (frames, PLC) · clip (ADAA) ▶ motor ODE      + FDN tail           │
       crystal     AM (baseband) · NBFM       Bl(x) K(x) rattle ▶ throat   air / HF shelf       │
                   (baseband) · keying /      ▶ body IIR (box, bells,      ▶ level (auto-match) │
                   squelch                    horn, LP)                    ▶ mix (crossfade) ───┴▶ out
```

### 2.1 Models

A model is a bundle of (mic, channel, amp, speaker, room) plus its voicing constants. **Every value is a
voicing starting point [I]**, to be fixed against measured devices (§7.7).

| `model` | Mic | Channel | Amp | Speaker (band at −10 dB, peaks) | Room | Model's own noise |
|---|---|---|---|---|---|---|
| **Telephone** (default) | Carbon | Phone Line | line-driven (none) | Handset earpiece: 300 – 3,400 Hz, peaks ≈ 1 kHz and 2.6 kHz | Small Room, at the ear | Line hiss −50 dB, hum, faint crosstalk |
| **Cell Phone** | Clean | Cell Codec | smart-amp limiter | Phone receiver: 300 – 3,400 Hz (narrowband), smooth | Small Room, at the ear | Codec noise floor, comfort noise |
| **Megaphone** | Dynamic (close) | — | battery class-B, 15 W, hard clip | Re-entrant horn: 500 – 4,500 Hz, peaks ≈ 1.2 kHz and 2.4 kHz, throat nonlinearity | Street | Amp hiss −60 dB |
| **Walkie-Talkie** | Dynamic | NBFM (narrowband FM) | class-AB 0.5 W | 40 mm speaker: 450 – 4,000 Hz, box resonance ≈ 650 Hz | Small Room | CNR 30 dB, squelch tail |
| **AM Radio** | Clean | AM | tube, single-ended | Table radio, 5″ in open-back cabinet: 150 – 4,500 Hz, cabinet bump ≈ 200 Hz | Small Room | Static, CNR 35 dB |
| **PA Horn** | Dynamic | — | tube PA amp + 100 V line transformer (saturates on lows, §2.4) | Reflex trumpet: 400 – 6,000 Hz, strong horn peaks | Hall + distant horns | Hum, amp hiss |
| **Intercom** | Electret | Phone Line (short: hum, AGC, no crosstalk) | small IC amp | 50 mm speaker in a metal box: 500 – 4,000 Hz, box ring ≈ 1.5 kHz | None | Hum 50 Hz −45 dB |
| **Pocket Radio** | Clean | — | class-B transistor, 9 V, crossover distortion | 57 mm speaker: 250 – 6,000 Hz, resonance ≈ 350 Hz | None | Amp hiss −65 dB |
| **Gramophone** | — | — | soundbox overload (asymmetric) | Horn + mica soundbox: 250 – 4,000 Hz, soundbox resonance ≈ 2.8 kHz, horn ripples | Small Room | Surface noise, crackle |
| **Laptop** | Clean | — | smart amp (limiter + bass enhancer) | Micro-speaker: 4th-order HP at 700 Hz, 700 Hz – 14 kHz, peak ≈ 1 kHz | Small Room (desk) | none |

- **"—"** in the Mic or Channel column = that stage is bypassed (the `channel` option None; a model whose
  mic is "—" resolves `mic` Model to no mic stage at all).
- **Why these ten.** They are the brief's list, ordered by expected use. The order is permanent once shipped,
  since enum options are matched by rank. "Vintage PA / tannoy" is **PA Horn**. "Old gramophone horn" is
  **Gramophone**: only the acoustic reproducer. Its record medium (scratches, wow) is gfx-tape's Shellac 78,
  so the two chain (§6). Its own noise is a light surface hiss and crackle, so the model stands alone.
- **AM Radio vs Pocket Radio.**
  - **AM Radio** is the broadcast heard on a table set: the channel's artefacts (tuning, fading, whistles,
    static).
  - **Pocket Radio** is only the device: a clean signal played on a 9 V transistor set (the sound of the
    eq-graphic "Lo-fi radio" preset, done physically).
  - `channel` AM with `speaker` Pocket Radio gives the beach radio.
- **Candidates to append later** (§7.2):
  - **Shortwave**: an SSB (single-sideband) channel. Mistuned SSB is a frequency shift (the "Donald Duck"
    voice) and needs gfx-dsp `ssb`.
  - **Bullet Mic**: a crystal harmonica mic into a small tube amp.
  - **Smartphone Speaker**.
  - **TV**: a CRT set with its 15.7 kHz line whistle.

### 2.2 Microphone (transmitter side)

`mic`: Model · Clean · Carbon · Dynamic · Crystal · Electret. Each mic is a sandwich: pre-filter, then a
memoryless curve, then a post-filter.

- **Clean**: a high-pass at 80 Hz only, as for a phone's MEMS mic. No nonlinearity.
- **Carbon** (the telephone's defining stage, after Oksanen & Välimäki):
  - **pre-filter**: the diaphragm, a 2nd-order resonant low-pass at 1.4 kHz with Q 1.5, then a high-pass at 250 Hz [I];
  - **nonlinearity**: the granule resistance R = R0 · (1 − h·p), driven at constant voltage, so the current
    is i ∝ 1 / (1 − h·p) − 1. The curve is rational and asymmetric, so 2nd harmonic dominates. The
    modulation h is set so that the reference level (−18 dBFS RMS) gives about 5 % THD (total harmonic
    distortion) at 1 kHz: a peak h·p ≈ 0.1 [A]. `drive` raises h·p by 0.4 dB per % only (+12 dB at 100 %,
    peak h·p ≈ 0.4): the full +30 dB would only flatten the voice against the guard [I, voiced];
  - **noise**: a "frying" noise (white noise through a 1 – 4 kHz band-pass) at the model's level, plus a
    level-dependent noise: the same noise amplitude-modulated by the slow envelope of h·p, so loud syllables
    crackle;
  - **guard**: the operating range is held to |h·p| ≤ 0.6 by a soft limit before the curve
    (0.6 · tanh(h·p / 0.6)), so it never reaches the pole;
  - **DC**: the curve has a level-dependent DC term (the mean of 1 / (1 − h·p)); the post-filter starts
    with a 60 Hz high-pass so that syllables do not thump.
- **Dynamic**: close-talk proximity (a +6 dB shelf below 200 Hz), presence peak at 4 kHz (+4 dB) and gentle
  tanh only when driven [I].
- **Crystal**: a 3 kHz resonance (+8 dB, Q 2), roll-off below 150 Hz and a high source impedance against the
  amp's input (a 1-pole low-pass at 5 kHz) [I].
- **Electret**: flat 100 Hz – 8 kHz. Bias-starved asymmetric clip at +6 dBFS-equivalent [I].
- **Domain**: the curves run at 2× (gfx-dsp `Oversampler2x`). For a sine of peak a = h·p, the rational
  curve's harmonic n is r^(n−1) below the fundamental, with r = (1 − √(1 − a²)) / a [A, exact]: r = 0.05 at
  the reference (2nd harmonic 5 %), 0.33 at the guarded maximum (9th harmonic −76 dB). Prototype at 2×,
  1.2 kHz, guard included: worst alias −126 dB at `drive` 100 % and −103 dB with +30 dB forced into the
  guard [A]. 2× is ample; the bench confirms (§5).

### 2.3 Transmitter and channel

`channel`: Model · None · Phone Line · Digital Line · Cell Codec · AM · NBFM.

**Keying and squelch (all channels).** The keying detector is a level detector on the transmitter input: a
`SlidingMax` over 20 ms, so it cannot ripple on a low note (the gfx-gate lesson). It is compared with the
`squelch` threshold, with 4 dB of hysteresis and a hang of 300 ms × `squelch_tail`. **`squelch` 0 % keeps
every channel open forever** (constant noise, as the real line). Above 0 % the channel **unkeys** in silence,
and each channel closes its own way:

| Channel | On unkey | On key-up |
|---|---|---|
| NBFM | The carrier stops (A ramps to 0 over 2 ms). The receiver now hears pure noise: the **squelch tail** lasts until the noise-squelch detector (time constant 150 ms × `squelch_tail` [I]) closes. Optional **roger** beep (two tones, 1,000 → 1,500 Hz, 2 × 80 ms [I]) before the carrier drops | The carrier rises over 2 ms and captures the discriminator (the noise quiets). The squelch opens after its 15 ms attack, so the first syllable loses a few ms (true to life; `squelch` 0 avoids it) |
| Cell Codec | DTX: the line noise is replaced by smoothed comfort noise for the hang, then silence | The noise suppressor re-learns over 2 frames |
| Phone Line, Digital Line, AM, None | Channel noise, hum and crosstalk fade out over 200 ms × `squelch_tail` | Noise fades in over 10 ms |

After the hang and the tail, the channel's noise generators **stop**: silence in gives exactly 0 out once the
room tail has gone. That is the "noise follows the voice" behaviour players ask for (§1.4). `state` (read-only)
shows keyed / unkeyed. The default `squelch` is 25 % (threshold −65 dBFS: closes only in true silence).

- **Squelch-tail level.** With no carrier the discriminator's output is far louder than speech: −10 dBFS RMS
  after the de-emphasis and the audio band, 4 dB above a full-deviation tone and peaking over full scale
  [A, prototype]. A real set does that too (the "squelch crash"), but a pedal must not: the receiver's audio
  stage ends with a limiter that holds the open-squelch noise at **6 LU below the reference programme
  level** [I, voiced]. The burst stays the loudest noise of the pedal, on purpose, and is bounded.
- **A source that never goes silent** (a hot mic in a room, a humming pickup above −65 dBFS) never unkeys:
  the pedal then behaves as at `squelch` 0. That is the safe side.

**Phone Line** (analog POTS, plain old telephone service):

- **Band**: a 4th-order Butterworth high-pass at 280 Hz plus a 6th-order Chebyshev (0.5 dB) low-pass at
  3,500 Hz. This is the loaded-cable cutoff, a "G.712-like" mask, not a measured one [I].
- **Line**: a −2 dB tilt from 1 to 3 kHz (cable loss) and a slow AGC (attack 20 ms, release 400 ms, ±10 dB).
- **Hum**: at `mains` (50 / 60 Hz) plus 3rd and 5th harmonics.
- **Crosstalk**: a seeded **babble** (band-passed noise with a 3 – 6 Hz syllabic envelope, spectrally like
  distant speech) at −45 dB re the reference.
- **Dropouts**: `dropouts` adds line clicks and crackle (a Poisson process, band-limited impulses).

**Digital Line** (G.711 µ-law, 8 kHz): gfx-crusher's virtual converter (`converter`: µ-law, 8 bits,
8,000 Hz, Clean model) after the Phone Line band. **It depends on gfx-crusher's `converter` moving to
gfx-dsp** (§2.8). It brings the real 8 kHz sampling and the companded grain that Production Expert calls the
key to realism [V].

**Cell Codec** (narrowband; **not** a bit-exact AMR / GSM coder, since those need 20 ms frames + lookahead,
which is latency) [I]. The draft's coder was a bank of 7 bells whose gains stepped once per frame: a
graphic EQ that wobbles by ± Δ / 2, with none of a speech coder's two signatures (an LPC envelope that is
quantised, an excitation that is coarse). It is replaced by a **real predictor made backward-adaptive**: the
short-term stage of GSM 06.10 (order-8 LPC, reflection coefficients quantised as log-area ratios, a
regular-pulse excitation grid) with its coefficients taken from the *previous* frame, the way G.728 reaches
its low delay [I, standard]. The whole coder runs on the converter's **8 kHz virtual samples** (between its
ADC and DAC halves, §2.8), so it is the same at every host rate.

1. **Band and rate**: the 8 kHz converter at 13 bits linear, with the Phone Line band.
2. **Frame clock**: 160 virtual samples (20 ms), phase 0 at reset (deterministic).
3. **Analysis**: at each frame boundary, an order-8 autocorrelation LPC of the frame that just ended
   (Hamming window, 60 Hz lag window, Levinson-Durbin) gives 8 reflection coefficients k, clamped to
   |k| ≤ 0.98. The work is 160 × 9 multiply-adds, once per 20 ms: no CPU spike.
4. **Predictor** (the warble and the hoarseness), over the next frame:
   - an **analysis lattice** with k turns the live signal into its residual e;
   - a **synthesis lattice** with the quantised k̂ rebuilds the voice from e;
   - **envelope quantisation**: k̂ = k quantised uniformly in the log-area-ratio domain
     (LAR = ln((1 + k) / (1 − k))) with step q = 0.01 · `codec` (50 % = the model's 0.5 [I]). The formants are
     rebuilt slightly wrong, differently on each frame;
   - **excitation coarsening**: e′ = e + α · (RPE(e) − e), where RPE keeps one residual sample in three
     (× 3), GSM full rate's pulse grid without its long-term predictor. α = 0.005 · `codec` (50 % = the
     model's 0.25, at most 0.5). Its error is shaped by the synthesis filter, so it follows the voice's own
     spectrum, as coding noise does;
   - both lattices move from the old to the new coefficients over 3 ms (24 samples), together.
   - **At `codec` 0 the stage is an exact identity**: with k̂ = k and α = 0 the two lattices are inverses
     sample by sample, ramps included (prototype: residue −214 dB re the signal in f64 [A]).
   - Because the coefficients come from the previous frame, the envelope lags the voice by up to one frame.
     That lag is the smeared, "underwater" movement, and it is strongest on onsets.
   - **Bounds** (prototype, synthetic voiced signal with gliding formants and unvoiced gaps [A]): q = 0.5
     alone leaves an error 7 dB below the signal, with frame levels within −5.5 … +4 dB of the input;
     q = 0.5 with α = 0.5 leaves −3 dB and −7 … +8 dB; q = 1.0 with α = 1.0 bursts to +19 dB on single
     frames, which is why α stops at 0.5 and the AGC of step 6 ends with the smart amp's limiter. No setting
     is unstable (|k̂| ≤ 0.98). The values of q and α are voicings, to set by ear on real voices (§7.7).
5. **Noise suppressor**: 7 band-pass detectors at half-octave centres 350, 495, 700, 990, 1,400, 1,980 and
   2,800 Hz measure each band's energy over the frame that just ended; per band, a minimum-statistics floor
   (the 1.5 s minimum of frame levels). Bands within 6 dB of the floor are cut on the next frame by up to
   0.3 · `codec` dB (15 dB at 50 %), through a **cascade of 7 SVF bells** at the same centres (Q 2), ramped
   over 3 ms. At 0 dB a TPT bell is an identity. The result is the swirly musical noise of phone noise
   suppression, which acts on the channel noise and on room sound.
6. **AGC**: target −20 dBFS, attack 50 ms, release 1 s, ±12 dB; it pumps on dynamic material by design. It
   does not scale with `codec`: it belongs to the handset.
7. **Packet loss** (`dropouts`): a seeded **Gilbert–Elliott** two-state loss process at frame granularity,
   because real network losses come in bursts (mean burst 2 frames [I]; loss rate 0.3 % per % of
   `dropouts`, so 30 % of frames at 100 %). In a lost frame the output is built from a history of the
   **decoder's own output** (1,600 virtual samples, 200 ms): what the listener has already heard, which is
   what a decoder repeats.
   - the last pitch period is repeated (gfx-dsp `PitchDetector` on the decoder output, its last voiced
     estimate; unvoiced → a 10 ms segment), overlap-added over ¼ period at each wrap. The overlap reads the
     quarter period *before* the repeated one, which is in the history: causal;
   - attenuation is 0 for the first 10 ms, then −20 % per 10 ms, and silence after 60 ms (the G.711
     App. I schedule [V, secondary]);
   - on return, the live signal crossfades back over ¼ period, and the predictor's lattices keep running
     through the loss (their states never jump).
   - **Zero latency.** App. I delays the output by 3.75 ms so that the first overlap-add can use future
     samples. We instead crossfade *from the live signal* into the repeat over 2 ms at the loss instant.
     That uses no future sample and costs only a 2 ms fade-in of the repeat, inaudible next to the repeat
     itself [I]. The repeat y[n − P] continues a periodic signal in phase, so the crossfade joins two nearly
     equal signals: click-free on voiced sound by construction, and a 2 ms equal-gain fade on the rest.
   - Everything above is on virtual samples: the converter's kernel renders the repeats, fades and the
     pulse grid band-limited.

**AM** (complex baseband at the **host rate**: no carrier is synthesised).

- **The host rate is enough** [A]. With the dial at ±9 kHz and a neighbour's sideband 4.5 kHz beyond its
  carrier, the widest content sits at ±22.5 kHz. At 44.1 kHz the last 450 Hz wraps to the other side of the
  complex band, 18 kHz away from the 3 kHz IF filter, which removes it (−90 dB).
- **Rate independence** needs one rule: noise is specified as a **density**. The complex noise variance per
  sample is P_carrier · fs / (ρ · B_n), with ρ the CNR and B_n the IF filter's two-sided noise bandwidth
  (12.1 kHz for the NBFM filter, 6.0 kHz for the AM one [A]). The same rule serves NBFM.
- **Gaussian noise**: two normal deviates per sample from `lfo::Rng` (a sum of 4 uniforms is enough for
  static; the FM click statistics need true Gaussian tails: Box-Muller or a ziggurat there).

1. **Broadcast processor** (TX):
   - an NRSC-like pre-emphasis (+6 dB at 5 kHz [I]);
   - a 2-band AGC (attack 10 ms, release 300 ms);
   - a clipper that holds negative peaks at 100 % modulation, ADAA hard clip at 2×;
   - a TX low-pass at 4.5 kHz (Region 1);
   - `drive` pushes the modulation index past 100 % instead of the clipper: **overmodulation** (m from 0.85
     to 2.5 at 100 %). The carrier pinches off and splatters, the AM distortion.
2. **Modulation**: e(t) = max(0, 1 + m·x(t)), a real baseband envelope with m = 0.85 nominal. The max() is
   computed at the host rate, with no oversampling: the IF filter removes the splatter and its images
   alike (prototype at m = 2.5, 48 kHz: worst inharmonic product below 4.5 kHz at −92 dB re the fundamental,
   against −97 dB for a 16× reference [A]).
3. **Channel**, a complex signal c(t):
   - **Multipath (selective fading)**: c = e(t) + a(t) · e^{jφ(t)} · e(t − τ(t)), with τ 0.2 – 1 ms, a 0.2 – 0.8,
     and φ and τ seeded random walks at 0.05 – 0.5 Hz. The notch it creates sweeps through the sidebands and
     the carrier: hollow phasing, then **distortion when the carrier sits in the notch** (the envelope
     detector no longer sees a carrier). Its depth is `dropouts` (0 = one path).
   - **Adjacent stations**: two babble programmes, each AM-modulated, rotated to ±9 kHz.
   - **Static**: complex Gaussian noise at the CNR set by `noise`, plus impulsive atmospherics (Poisson
     crackles shaped by a 1 kHz low-pass).
   - **Whistle**: a spur, an unmodulated carrier at +2.5 kHz on the dial [I] at level `whistle` (dBc, decibels
     relative to the carrier).
4. **Tuning**: c is multiplied by e^{−j2πΔt}, with Δ = `tune` × 9 kHz (±100 % = the neighbour station).
5. **IF filter** (intermediate frequency): a real 6th-order Butterworth low-pass on I and on Q (bandwidth ±B,
   B from the receiver: 3 kHz table radio [I]; `band` moves B with the other edges). Detuned, it cuts one
   sideband and puts the carrier on its skirt: the thin, distorted, off-station sound. A rotation alone does
   not change |c|: **all of the detuning sound comes from this filter**. Prototype (1 kHz tone, m = 0.85) [A]:

   | Detuning (`tune`) | Carrier | THD at 1 kHz | THD at 2 kHz |
   |---|---|---|---|
   | 0 | 0 dB | 0.0 % | 0.0 % |
   | 1 kHz (11 %) | 0 dB | 0.4 % | 4.9 % |
   | 2 kHz (22 %) | 0 dB | 3.0 % | 6.5 % |
   | 3 kHz (33 %) | −3 dB | 9.6 % | 11.6 % |
   | 3.5 kHz (39 %) | −9 dB | 17 % | 17 % |
   | 4.5 kHz (50 %) | −22 dB | 6 % | 6.5 % |
   | 9 kHz (100 %) | −63 dB | the neighbour is tuned in | |

   So the dial has a clean centre of about ±10 %, a "slightly off" zone to ±25 %, and breaks up at ±35 – 40 %.
6. **AGC** (receiver, carrier-referenced, 200 ms, gain 0 … **+30 dB**): between stations it raises the gain,
   so the static rises. This is the real "tuning noise". The +30 dB ceiling keeps the mid-dial static
   bounded: at ±50 % the wanted carrier is 22 dB down, beyond ±60 % the AGC is at its ceiling.
7. **Detector**: |c|, followed by the diode's RC: instant attack, release τ_d = 1 / (2π · 3 kHz) × (1 +
   `drive` / 50 %), so a slow RC gives **diagonal clipping** on deep high-frequency modulation. Then a DC
   block (the carrier) and a 4.5 kHz audio low-pass.
   - Diagonal clipping starts above f = √(1 − m²) / (2π · m · τ_d) [I, standard]: 1.86 kHz at `drive` 0 and
     m = 0.85 (in practice nothing: the IF filter has lowered m up there), 620 Hz at `drive` 100 %
     (prototype: THD 3.7 % at 1 kHz, 12 % at 2 kHz [A]).
   - |c| of a signal limited to ±B aliases negligibly at the host rate (prototype with the carrier on the
     skirt: nothing inharmonic above −160 dB at 44.1 kHz [A]).
8. **Whistle pitch**: the spur beats with the station's carrier at |2.5 kHz − Δ|, so **turning `tune` sweeps
   a squeal through zero beat**. This is a "birdie", an internal spur fixed on the dial. Its pitch follows the
   dial as on cheap superhet receivers [I].
   - The real night-time 9 kHz adjacent-channel whistle is also there, at a pitch fixed by the station
     spacing. At `band` 0 it is inaudible, as on a set with a whistle filter: the neighbour's carrier is
     57 dB down the IF skirt, then cut by the 4.5 kHz audio filter [A]. `band` +100 % (IF ±6 kHz, audio
     9 kHz) lets it through at about −25 dB re the carrier, for a neighbour as strong as the station.

**NBFM** (walkie-talkie, complex baseband):

1. **TX**: a pre-emphasis of +6 dB / oct from 300 Hz, then a **deviation limiter** (hard clip with ADAA at 2×),
   then a 3 kHz splatter low-pass.
2. **Modulation**: phase φ += 2π · 2.5 kHz · x / fs (±2.5 kHz peak deviation, 12.5 kHz channel [I, standard:
   11K0F3E]; φ kept in f64 and wrapped). The baseband signal is A · e^{jφ}, with A = 1 when keyed and 0 when
   not, moving over 2 ms (a transmitter's rise time; no step in c).
3. **Channel**:
   - complex Gaussian noise at the CNR (30 dB nominal, moved by `noise`), as a density (the AM rule above);
   - `dropouts` adds **Rayleigh-like fading**: two seeded paths whose difference frequency is 2 – 8 Hz,
     the "picket-fencing" flutter of a moving handset.
4. **RX**:
   - a ±6 kHz IF low-pass (6th-order Butterworth on I and Q; Carson's rule gives ±5.5 kHz), tuned by `tune`
     (±100 % = ±12.5 kHz). There is no neighbour station on this channel: past about ±50 % the carrier leaves
     the IF, the discriminator gives noise and the noise squelch **mutes the pedal** after its tail. The
     playable range is ±40 %; the rest is "out of the channel", stated;
   - a **limiter-discriminator**: arg(c[n] · conj(c[n−1])) · fs / (2π · 2.5 kHz);
   - a de-emphasis, then a 300 – 3,000 Hz audio band.
   - When the CNR falls below about 10 dB, **2π phase slips appear as clicks** by themselves, at Rice's rate
     and at any host rate (§1.2: prototyped). No click is scheduled or synthesised. The discrete phase
     difference is the exact integral of the instantaneous frequency over one sample as long as it turns by
     less than π per sample; near an origin crossing it can pick the other direction, which changes the
     click count by under 10 % between 44.1 and 192 kHz [A]. Detuning clips the deviation in the IF filter,
     which distorts and adds noise.
   - Fading (`dropouts`) lowers the CNR for the length of each fade, so clicks arrive in bursts at the
     bottom of the fades: picket-fencing as it is heard.
5. **Noise squelch**: the discriminator output is high-passed above 4 kHz and followed (attack 15 ms, release
   150 ms × `squelch_tail`); the squelch closes when that noise rises above a fixed ratio. With the carrier
   gone, this gives the tail of the table above **from the physics**. `squelch` sets the VOX keying (the TX
   side); the receiver's squelch threshold is fixed [I].

**Aliasing in the channels** (prototyped, figures above). The baseband runs at the host rate. Its hard
operations are max() in the AM modulator, |c| and arg. All three act on, or are followed by, signals limited
to ±B ≤ 6 kHz by the IF filter, and the audio filter (≤ 4.5 kHz) follows the detectors: the measured
inharmonic residue is below −90 dB. The FM clicks are impulses of area 2π whose width is set by the IF
(about 0.1 ms): band-limited by the physics. Only the TX clippers (AM broadcast clipper, FM deviation
limiter) need the 2× island with ADAA. The bench checks the whole (§5).

### 2.4 Amplifier

The amp is a memoryless chain run oversampled together with the speaker's motor (one up / down pair):
**4× for the hard-rail amps** (Megaphone, Walkie-Talkie, Intercom, Pocket Radio: two cascaded
`Oversampler2x`), **2× for the others** (tube, smart amp, line-driven). The factor belongs to the amp type,
so it never changes while a knob moves; an amp switch crossfades (§2.7).

- **Why** (prototype: 1.2 kHz, +30 dB into a hard clip with first-order ADAA [A]):

  | Oversampling | Worst alias, full band | Behind a 6 kHz 4th-order speaker low-pass | Behind 4.5 kHz |
  |---|---|---|---|
  | 1× | −35 dB | −52 dB | −57 dB |
  | 2× | −56 dB | −74 dB | −80 dB |
  | 4× | −64 dB | −90 dB | −93 dB |

  2× meets the −70 dB bound only because a narrow speaker follows. `speaker` None (the line out for gfx-cab)
  and `band` +100 % remove that cover. At 4× every model passes with 20 dB to spare, and the worst part swap
  (hard rail, `speaker` None, `drive` 100 %) sits at −64 dB, on the contract's −65 dB part-swap bound: the
  bench decides whether that one configuration needs second-order ADAA.

- **Crossover distortion** (class-B): y = x − d · tanh(x / d). d is the model's crossover width (Pocket
  Radio 0.02, Megaphone 0.01 re full scale [I]), scaled up as `battery` falls. It is smooth, so it needs no
  ADAA (prototype: worst alias −73 dB at 2× for d = 0.02, lower at 4× [A]).
- **Rail clip**: a hard clip (new `adaa::Shaper::Hard`, closed-form antiderivative) for transistor amps, and
  tanh (existing) for tube amps.
  - The ceiling is the rail: V_rail · `battery`.
  - **Sag**: the rail drops by up to 20 % with the 50 ms envelope of the output, and more as `battery` falls.
    At `battery` < 30 % the rail collapses on peaks: the **dying-battery sputter**.
- **`drive`**: 0 – 100 % = 0 … +30 dB into the amp. The output is compensated by the model's measured static
  gain curve (a constant table per model, measured on the bench with a −18 dBFS RMS pink reference: nothing
  is rendered at `prepare`), so loudness holds within ±3 LU.
- **Smart amp** (Cell, Laptop): a 5 ms / 150 ms peak limiter at −6 dBFS-equivalent, plus (Laptop) a bass
  enhancer: the 80 – 200 Hz band rectified and band-passed at 2 – 3× its frequency (missing-fundamental
  harmonics) [I].
- **100 V line** (PA Horn): the transformer saturates on lows. Under the 50 ms envelope of a 150 Hz low-pass
  of the signal, a soft clip acts on the < 200 Hz band only [I].

### 2.5 Loudspeaker

`speaker`: Model · None · Handset · Megaphone Horn · Walkie Speaker · Table Radio · PA Horn · Intercom ·
Pocket Radio · Gramophone Horn · Laptop.

1. **Motor** (in the amp's oversampled island, new gfx-dsp `speaker::Motor`). The normalised Klippel lumped
   model has excursion x in units of X_max, velocity v and voltage drive u from the amp:

   ```
   x'' = ω_s² · [ b(x) · u − k(x) · x ] − ω_s · ( 1/Q_ms + b(x)² / Q_es ) · x'
   b(x) = 1 / (1 + β·(x − x_b)²) · (1 + β·x_b²)   (force factor Bl: falls with excursion, never negative;
                                                   x_b = rest-position offset)
   k(x) = 1 + κ1·x + κ·x² + κ₃·x⁴                  (suspension: asymmetry, stiffening, bottoming)
   output p = x'' / ω_s²                           (far-field pressure = cone acceleration)
   ```

   - **Small-signal limit**: b = k = 1. The model is then the Thiele-Small 2nd-order high-pass at ω_s with
     1 / Q_ts = 1 / Q_ms + 1 / Q_es: the electrical damping is the motor's back-EMF, which is why it scales
     with b(x)² and **falls with excursion** (a speaker driven hard rings more at its resonance). The box's
     low section is this motor, not a filter: at low level the motor **is** the response (prototype at
     96 kHz, 2 iterations: within 0.002 dB of the analytic high-pass from 50 Hz to 15 kHz, for f_s 120 –
     700 Hz and Q_ts 0.6 – 1.2 [A]).
   - **Why a rational b(x)**. The draft's polynomial 1 − β·x² + β1·x changes sign at |x| = 1 / √β (1.4 – 1.8
     X_max), which the prototype reaches at `drive` 100 %: the motor would then push backwards. Klippel's
     curves are bell-shaped and stay positive; the rational form has the same first terms.
   - The voice-coil inductance Le(x) is left out: its effect (a rising impedance, reluctance force) lies
     above these speakers' bands and belongs to the body filters.
   - **Per-speaker constants** [I, voiced]: f_s (Handset 400 Hz, 40 mm 650 Hz, 57 mm 350 Hz, 5″ 120 Hz,
     micro 700 Hz), Q_ts 0.6 – 1.2 (Q_ms 3 – 5), β 0.3 – 0.5, κ 0.5 – 3, κ₃ 5 – 50: the x⁴ term is
     negligible below X_max and makes the suspension bottom out above it.
   - **Solver**: trapezoidal, predictor + 2 fixed-point iterations per oversampled sample; p is read from the
     right-hand side (no numerical differentiation). Prototype at 96 kHz, a 20 Hz full-scale square with
     up to +30 dB of gain, f_s 700 Hz, β 0.5, κ 3, κ₃ 0 – 50 [A]: **never unstable**; 2 iterations agree with
     4 within 1 % except at the extreme (κ₃ 50 at +30 dB: 15 % on the peak pressure, where the bottomed
     suspension's own resonance reaches 10 kHz). The amp's rail bounds u long before that. A final clamp
     |x| ≤ 4 guards the state against a pathological automation.
   - **No DC at the output**: p is an acceleration, and a bounded excursion has zero mean acceleration; the
     DC *excursion* of an offset motor (x_b, κ1) is inside the model and modulates the breakup, as it should.
   - **Level reference**: −18 dBFS RMS at `drive` 0 → peak x ≈ 0.3 on the model's own band (light). `drive`
     100 % hits X_max.
   - **Rattle** (`rattle`): when |x| > 0.85, the excess (|x| − 0.85) gates a high-passed (> 2 kHz) noise burst
     and a band-limited impulse at each direction change. This is a rubbing voice coil, the **blown speaker**.
     The gate is the smooth excess, not a comparator, so it adds no step.
2. **Throat** (horn speakers only: Megaphone Horn, PA Horn, Gramophone Horn):
   - y = p + η · HP(d/dt (p²)), with HP a 2nd-order high-pass at 200 Hz;
   - this follows the air-overload shape (§1.2): a 2nd harmonic proportional to frequency and to pressure, and
     falling as η = η₀ · (f_c,ref / f_c) for the horn's cutoff;
   - it runs at the host rate, after the island. p is an acceleration, so the motor does **not** band-limit
     it; a 4th-order low-pass at the horn's upper edge (≤ 6 kHz) comes first, and p² then stays under
     12 kHz: no aliasing at 44.1 kHz and above [A]. `band` moves that low-pass only up to 9 kHz;
   - the derivative is a first difference (its droop at 12 kHz, 1 dB at 48 kHz, is part of the voicing).
3. **Body** (host rate, gfx-dsp `matched` designs in `svf`; 4 – 7 sections per speaker [I]):
   - cone breakup bell(s) and horn resonances (bells);
   - a box or cabinet bump;
   - the high roll-off: a 2nd- or 4th-order resonant low-pass;
   - the horn cutoff, a 2nd-order high-pass on top of the motor's.
   - Each design is an analog prototype mapped by `matched`, so it holds from 44.1 to 192 kHz (the gfx-bassdi
     §2.7 approach).
   - `resonance` scales every bell's dB (0 – 200 %, 100 % = the model's).
   - `band` moves both edges (§2.6).
4. **None** bypasses 1 – 3: a line output (for an IR in gfx-cab after it).

### 2.6 Distance, band, room

- **`distance`** (0 – 100 %): the listener's distance r = r_near · (r_far / r_near)^(d / 100) (an internal
  law; the parameter range stays linear).
  - **Ranges by room**: Handset r_near 2 cm (at the ear), r_far 2 m. Horns 1 m → 40 m. Others 0.3 m → 4 m.
  - **Direct path**: gain r_near / r. The room's diffuse level is constant, so the direct / reverberant ratio
    falls 6 dB per doubling and the critical distance comes from the room.
  - **Air and off-axis**: a high shelf at 4 kHz of −6 · d / 100 dB.
  - **Handset leak**: below 800 Hz the earpiece loses its pressure-chamber bass as it leaves the ear (a
    high-pass rising from 100 to 800 Hz), giving the "phone on the table" sound [I].
  - **Level**: total loudness is compensated so that 0 → 100 % loses at most 6 LU. Farther sounds farther,
    but it does not vanish.
- **`band`** (−100 … +100 %, 0 = the model):
  - ±100 % moves the speaker's and the TX filters' **edges by one octave** outward (+) or inward (−). +100 %
    on Telephone is about 150 Hz – 6.8 kHz.
  - It is the intelligibility and "open up" control (Speakerphone's progressive dry, §1.1).
  - Physical rates do not move: the 8 kHz converters (Digital Line, Cell Codec) still cut at 4 kHz. That is
    stated, not hidden.
- **`room`**: Model · None · Small Room · Hall · Street.
  - **Small Room**: gfx-dsp `reverb::Fdn` (16 lines) at RT60 0.4 s, `Diffuser` before.
  - **Hall**: Fdn at RT60 2.2 s, plus 3 **distant-horn taps** (60, 115 and 180 ms, each low-passed and
    −8 / −12 / −16 dB: other horns of the same PA system) [I].
  - **Street**: 4 discrete facade reflections (45 – 210 ms, `DelayLine`, each with a 1-pole HF loss) and a
    short diffuse tail at RT60 0.8 s.
  - **Seeds**: the delay sets are fixed and seeded, so renders are deterministic.

### 2.7 Output, level, mix, bypass

- **Auto-match**: each model at `drive` 0, `noise` 0 and `distance` 0 carries a static makeup (measured once
  per model on the bench's pink reference, a constant table), so the model plays within ±2 LU of the bypass on
  the bench DIs. `level` (−24 … +12 dB) trims on top.
- **`mix`**: linear crossfade dry ↔ wet (gfx-crusher's law). **The dry is not phase-aligned, and will not
  be** (decided at review).
  - The wet is a band-pass device chain, not a delay: across its own band its phase turns by 2.6 π
    (Telephone), 2.2 π (Megaphone) or 1.5 π (Pocket Radio) [A, prototype of the linear chains]. No all-pass
    on the dry tracks that without becoming the chain itself, and one matched to the low edge fixes one
    notch of several.
  - Measured consequence (deepest notch re the louder of dry and wet) [A]:

    | `mix` | Telephone | Megaphone | Pocket Radio |
    |---|---|---|---|
    | 30 % | −5.5 dB (1.4 kHz) | −3.5 dB | −1.9 dB |
    | 50 % | −22 dB (1.45 kHz) | −12 dB | −4.7 dB |
    | 70 % | −11 dB (430 Hz) | −12 dB | −13 dB |

  - So `mix` is a **layering** control, good to about 35 % (the phone under a clean voice) and at 100 %.
    Between, it sounds like two devices out of phase: the same result as blending a real re-amped phone
    with its source. Presets stay at ≤ 35 % or 100 %.
  - One free improvement: each model carries a fixed **wet polarity**, chosen on the bench as the one whose
    deepest notch at `mix` 30 – 50 % is shallower. It is a constant of the model, inaudible at 100 %.
  - "Opening up" a model is `band`'s job, not `mix`'s (§2.6).
  - `mix` 100 % (the default) is unaffected.
- **`bypass`**: the nih-plug bypass parameter, with **trails**. The input to the wet is ramped to 0 over 10 ms
  while the room tail and an open squelch tail ring out. Hall's 2.2 s tail is a tail the contract's §Trails
  must keep.
- **Model / speaker / channel / mic switch**: the chain **before the room** exists twice. The new one is
  reset, runs on the live input for 10 ms unheard (its filters and AGCs settle on signal; a pre-run on
  silence would warm nothing and would cost a CPU burst in one cycle), then a 30 ms crossfade. The room is
  shared, so its tail survives a model change.
- **Room switch** (`room`, or a model whose room differs): the old room's input is ramped off over 30 ms and
  it rings out while the new one fills (two rooms run until the old tail is below −80 dB, at most 3 s).
- **Smoothing**: every float goes through gfx-dsp `smooth` (20 – 50 ms). `tune`, `distance`, `drive`, `noise`,
  `band` and `mix` also follow the EXP rule (`exp_smooth`).
  - `tune` is smoothed in Hz and drives a phase-continuous rotator (f64 phase).
  - A sweep therefore glides; it never steps at the 128 values of a 7-bit pedal (1.57 % per step = 141 Hz on
    AM [A]).

### 2.8 gfx-dsp reuse and what is new

| Module | Use |
|---|---|
| `svf`, `matched` | Every band, body, bell (including the codec's bell bank: exact identity at 0 dB), IF and audio filter |
| `oversample` (`Oversampler2x`) | The mic island and the TX clippers (2×); the amp + motor island (2×, or 4× for hard-rail amps: two cascaded stages, §2.4) |
| `adaa` | Tanh (existing); **new `Shaper::Hard`** for transistor rails and the TX limiters |
| `envelope` | `SlidingMax` (keying), `PeakFollower` (sag, squelch noise detector, AGCs) |
| `smooth`, `exp_smooth` | Parameters; EXP rule |
| `lfo::Rng` | All seeded noise, babble, Poisson crackle, loss process, fading walks |
| `delay` | PLC history, multipath delay (Hermite), Street / Hall taps |
| `detect` | `PitchDetector` for PLC repeats (75 Hz default range: voice and guitar), fed with the decoder's output |
| `reverb` | `Fdn`, `Diffuser` for Small Room and Hall |
| gfx-crusher `converter` (+ `blep`) | Digital Line and Cell Codec 8 kHz rate and µ-law. **Dependency, settled (§7.8)**: gfx-crusher is designed but not coded; `converter` and `blep` are written **in gfx-dsp from the start**, with the ADC half (sampling → virtual samples) and the DAC half (virtual samples → band-limited output) callable separately, because the Cell Codec works between them |
| gfx-dsp `noise` (planned in its README, not yet in `src/`) | White / pink / band-limited noise; written here if absent |
| **New `speaker`** | `Motor` (§2.5), throat, body voicing tables. gfx-bassdi's IIR cab voicings can share the body part |
| **New `baseband`** (crate first) | Complex AM / FM modulators, multipath, rotator, IF filter pair, envelope and limiter-discriminator detectors, noise squelch. A Shortwave SSB channel would add gfx-freqshift's `ssb` |
| **New `codec`** (crate first) | Frame clock, order-8 LPC (autocorrelation, Levinson), analysis / synthesis lattices, LAR quantiser, pulse grid, band noise suppressor, Gilbert–Elliott losses, PLC; all on virtual samples |

### 2.9 Stereo, latency, CPU

- **Layout**: 1 / 1 first. 2 / 2 runs two identical chains with **linked keying, losses and fading** (one
  device, heard in stereo), and a decorrelated room (the Fdn's two outputs). A speaker is a point source:
  nothing else widens.
- **Latency**: reported 0. The wet carries the half-bands' few samples and the IIR minimum phase (§2.7).
  Nothing looks ahead: the codec gains and the PLC use the past.
- **CPU [A, estimate]**, mono at 48 kHz. The draft's 150 ns forgot the room: gfx-reverb, on the same
  `Fdn` (16 lines) and `Diffuser`, measures 525 ns / sample for its whole algorithm.

  | Configuration | Estimate |
  |---|---|
  | No room (Intercom, Pocket Radio: amp + motor at 4×, body) | ≈ 150 – 250 ns / sample |
  | Typical (Telephone: carbon 2×, phone line, motor 2×, body 6 SVF, small room Fdn) | ≈ 600 ns / sample, about 3 % of a core |
  | Worst (AM or Cell Codec channel, PA horn, Hall Fdn + taps, during a model crossfade and a room change) | ≤ 1,500 ns / sample, about 7 % |
  | AM / NBFM baseband alone | ≈ 60 complex multiply-adds, 2 – 6 Gaussian deviates and 12 biquad sections per sample: 100 – 150 ns |

  All far inside the contract's bound for simulations (20 % of a core). If the Small Room proves too dear for
  a telephone, an 8-line `Fdn` variant is a gfx-dsp matter, not a design change.

## 3. Parameters

New golem block: **type `radio`**, category **`lofi`**, golem-fx only (no original plugin). Roles: voice,
guitar, keys, drums. lofi is already offered to voice and drums; golem-rack adds `radio` to the role blocks.

No catalogue ids to keep. These ids and **names** are join keys from the first ship and never change (descriptor
rules). All ranges are linear. Enums grow only at the end (option order is permanent). None of the defaults
depends on another parameter. Parameters that follow the model are relative (50 % or 100 % = the model's own,
stated) or have `Model` first.

### Front panel

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `model` | Model | e | Telephone · Cell Phone · Megaphone · Walkie-Talkie · AM Radio · PA Horn · Intercom · Pocket Radio · Gramophone · Laptop | Telephone | 30 ms crossfade | | The device bundle (§2.1) |
| `drive` | Drive | f | 0 – 100 % | 0 % | 20 ms | ✓ | Shared id: extra saturation on top of the model's own. +0 … +30 dB into the amp and speaker (and the carbon mic, the AM modulation index, the FM deviation limiter), loudness compensated |
| `noise` | Noise | f | 0 – 100 % | 50 % | 50 ms | ✓ | Channel noise and reception, **relative: 50 % = the model's own**. Offset 0.6 · (n − 50) dB on the noise level, or −0.6 · (n − 50) dB on the CNR for AM / NBFM. Below 10 % the noise amplitude also fades linearly to **exactly 0 at 0 %** (no step at the bottom). NBFM (30 dB at 50 %): first clicks near 85 % (9 dB), a crackle at 90 % (6 dB, ≈ 15 clicks / s), noise over the voice at 100 % (0 dB). Inert on Laptop (no noise modelled) |
| `distance` | Distance | f | 0 – 100 % | 0 % | 50 ms | ✓ | Listener distance: direct / room ratio, HF loss, handset leak (§2.6) |
| `tune` | Tune | f | −100 – +100 % | 0 % | EXP + 30 ms | ✓ | The dial. ±100 % = the next station (AM ±9 kHz, NBFM ±12.5 kHz). Detunes, sweeps the whistle, brings in neighbours and static. **Active on the AM and NBFM channels only** (greyed by golem through `depends` otherwise) |

**Defaults: sound-forward but safe.**
- Telephone at drive 0 is recognisable at once, with no drive added.
- Noise is the model's own, and it stops in true silence (`squelch` 25 %).
- Nothing surprises a live rig.

### Expert

| id | Name | Type | Range | Default | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|
| `squelch` | Squelch | f | 0 – 100 % | 25 % | 30 ms | | Keying threshold: 0 = always open; else −80 + 0.6 · n dBFS (25 % = −65 dBFS). The channel unkeys in silence and closes its own way (§2.3). The default is a fixed value, the same for every model (§7.4) |
| `mic` | Mic | e | Model · Clean · Carbon · Dynamic · Crystal · Electret | Model | crossfade | | Transmitter-side mic (§2.2) |
| `channel` | Channel | e | Model · None · Phone Line · Digital Line · Cell Codec · AM · NBFM | Model | crossfade | | Transmission (§2.3) |
| `speaker` | Speaker | e | Model · None · Handset · Megaphone Horn · Walkie Speaker · Table Radio · PA Horn · Intercom · Pocket Radio · Gramophone Horn · Laptop | Model | crossfade | | The loudspeaker (§2.5). None = a line out for gfx-cab |
| `room` | Room | e | Model · None · Small Room · Hall · Street | Model | crossfade | | Where the listener stands (§2.6) |
| `band` | Bandwidth | f | −100 – +100 % | 0 % | 50 ms | ✓ | Both edges ±1 octave around the model's band; the 8 kHz converters still cut at 4 kHz |
| `resonance` | Resonance | f | 0 – 200 % | 100 % | 50 ms | | Speaker peaks' depth; 100 % = the model's |
| `codec` | Codec | f | 0 – 100 % | 50 % | — (per frame) | | Cell codec coarseness, **relative: 50 % = the model's own** (law below); 0 % = the predictor and the noise suppressor are exact identities |
| `dropouts` | Dropouts | f | 0 – 100 % | 0 % | 50 ms | | Packet loss (Cell: 0.3 % of frames per %, in bursts), multipath fading (AM), Rayleigh fading (NBFM), crackle (Phone Line, Gramophone) |
| `whistle` | Whistle | f | −80 – −10 dB | −80 dB (off) | 30 ms | | AM spur level re the carrier; its pitch follows `tune` |
| `squelch_tail` | Tail | f | 0 – 200 % | 100 % | — | | Hang and squelch-tail time; 100 % = the model's (NBFM 150 ms) |
| `roger` | Roger | b | off / on | off | — | | Roger beep at unkey (NBFM) |
| `battery` | Battery | f | 0 – 100 % | 100 % | 100 ms | ✓ | Rail voltage of battery amps: lower = earlier clip, more crossover, sag, sputter below 30 % |
| `rattle` | Rattle | f | 0 – 100 % | 0 % | 50 ms | | Voice-coil rub at X_max: the blown speaker |
| `mains` | Mains | e | 50 Hz · 60 Hz | 50 Hz | — | | Hum frequency |
| `level` | Level | f | −24 – +12 dB | 0 dB | 20 ms | ✓ | Output trim after the auto-match |
| `mix` | Mix | f | 0 – 100 % | 100 % | 20 ms | ✓ | Dry ↔ wet linear crossfade (§2.7) |
| `bypass` | Bypass | b | off / on | off | 10 ms | | VST3 bypass parameter, with trails |
| `state` | Keyed | f, read-only | 0 – 1 | — | — | | Shared meter id: 1 = channel keyed / squelch open |

**`codec` law** (fixed here, not open), n in %:
- envelope quantiser step q = 0.01 · n in log-area ratio (0.5 at 50 %, 1.0 at 100 %: a very bad connection);
- excitation coarsening α = 0.005 · n (0.25 at 50 %, 0.5 at 100 %);
- noise suppressor's maximum cut 0.3 · n dB (15 dB at 50 %, 30 dB at 100 %).

At 0 % all three are identities; the 8 kHz converter, the band, the AGC and the comfort noise remain: that
is the phone, not the codec.

Inert parameters, all greyed through `depends`, with no hidden re-mapping:
- `tune` and `whistle` off AM / NBFM;
- `codec` off Cell Codec;
- `roger` and `squelch_tail`'s FM part off NBFM;
- `battery` on line-driven and smart-amp models;
- `mains` where no hum is modelled;
- `noise` on Laptop; `dropouts` where the channel is None (except Gramophone's crackle).

Expert groups (`visuals/data/groups.json`):
- **Chain**: `mic`, `channel`, `speaker`, `room`;
- **Tone**: `band`, `resonance`;
- **Channel**: `codec`, `dropouts`, `whistle`, `mains`;
- **Keying**: `squelch`, `squelch_tail`, `roger`;
- **Dirt**: `battery`, `rattle`;
- **Output**: `level`, `mix`.

`noise` is a 0 – 100 % float here, while gfx-tape's `noise` is a toggle. They are different crates and
different blocks, so the ids do not collide in golem, and `noise` is not in the contract's shared-id table.
Decided (§7.10): it stays a float here.

## 4. Applications

Reference settings become block presets (`presets/gfx-radio.json`). Names carry no artist or trademark.
Every preset is level-matched to the bypass (±2 LU on the bench DIs) with `level`.

| Use / preset idea | Settings to start from | Feature that matters |
|---|---|---|
| **Old Phone Call** (verse vocal, film-style call) | Telephone, defaults | Carbon 2nd harmonic and frying, line hum, auto-match |
| **Readable Phone Vocal** (in a dense mix) | Telephone, `band` +40 %, `noise` 20 %, `mix` 100 % | Opens the band without losing the device |
| **Phone Layer** (presence under a clean vocal) | Telephone, `mix` 30 %, `noise` 0 % | Parallel blend: notches ≤ 6 dB at this setting (§2.7); do not raise `mix` toward 50 % |
| **Bad Signal** | Cell Phone, `codec` 80 %, `dropouts` 40 % | Smeared formants, hoarse excitation, bursty PLC repeats, comfort noise |
| **Video Call** | Cell Phone, `channel` Cell Codec, `speaker` Laptop, `codec` 40 % | Matrix: the codec on a laptop speaker |
| **Riot Bullhorn** (vocal hook) | Megaphone, `drive` 50 %, `distance` 30 % | Class-B clip, throat 2nd harmonic, street slap |
| **Bullhorn Lead** (guitar) | Megaphone, `drive` 20 %, `band` +30 %, `room` None | Same horn, less clip, wider band for guitar |
| **Walkie Check** (intro, spoken word) | Walkie-Talkie, `squelch` 40 %, `roger` on | VOX keying, physical squelch tail, key-up click |
| **Out of Range** | Walkie-Talkie, `noise` 80 %, `dropouts` 50 % | FM threshold clicks in bursts at the bottom of each fade, picket-fencing |
| **Tune the Dial** (EXP) | AM Radio, EXP on `tune` −100 → +100 %, `whistle` −30 dB, `noise` 60 % | Neighbour stations, birdie squeal through zero beat, AGC static between stations |
| **Night Fading AM** | AM Radio, `dropouts` 60 %, `noise` 55 % | Selective fading: phasing, then carrier-notch distortion |
| **Station Announcement** | PA Horn, `distance` 60 % | Hall + distant-horn echoes, 100 V line bass saturation |
| **Ticket Window** (the eq-graphic "Hygiaphone" intent) | Intercom, `band` −30 % | Box ring, hum, physical band |
| **Front Door Buzzer** | Intercom, `drive` 40 %, `noise` 70 % | Small IC amp clip, hum |
| **Beach Radio** (guitar or keys) | Pocket Radio, `channel` AM, `drive` 25 % | Crossover distortion, small-speaker bass breakup |
| **Pocket Radio Intro** (full band intro, then off) | Pocket Radio, `drive` 30 % | The eq-graphic "Lo-fi radio" intent, physically |
| **Dying Battery** | Pocket Radio, `battery` 25 %, `drive` 40 % | Rail sag, sputter |
| **Blown Speaker** | Pocket Radio, `rattle` 60 %, `drive` 60 % | Rub and buzz at X_max |
| **78 Horn** (chain gfx-tape Shellac 78 before it, `noise` 0 % here) | Gramophone, `distance` 20 % | Mica soundbox resonance, horn ripples |
| **Laptop Demo** (drum loop "from a laptop") | Laptop, `distance` 20 % | Micro-speaker high-pass, smart-amp pumping |
| **Phone on the Table** | Telephone, `distance` 70 % | Handset leak, small room |
| **IR After** | any model, `speaker` None, then golem's `cab` with a device IR | The convolution route for a specific device |

## 5. Bench suite (on top of gfx-bench's common checks)

**gfx-bench options**:
- **`skip_latency`**: the half-bands and the IIR minimum phase give group delay, not a reported delay
  (gfx-drive's precedent). A row below measures that no sample is looked ahead.
- **`click_bound_db` −30 dB**: key-up clicks, squelch tails, PLC repeats and crackles are content (gfx-crusher's
  precedent); the rows below check the transitions themselves.
- **`silence_floor_db` −70 dB**: Hall's 2.2 s tail is at about −82 dB three seconds after the burst [A], and
  the channel noise has stopped by then (squelch default 25 %: hang 300 ms + fade 200 ms).

| Check | Pass |
|---|---|
| Small signal | At −40 dBFS, `drive` 0, `noise` 0, `dropouts` 0: each speaker's magnitude within ±1 dB of its voicing target from its low edge to its high edge; the motor's response vs the analytic Thiele-Small high-pass (f_s, Q_ts) within 0.05 dB (prototype: 0.002 dB) |
| Telephone band | −3 dB at 300 ± 30 Hz and 3,400 ± 100 Hz; ≤ −25 dB at 100 Hz and 4.5 kHz |
| Motor large signal | 50 Hz – 1 kHz tones stepped −30 → 0 dBFS at `drive` 100 %: THD rises monotonically with level; even harmonics appear only with x_b or κ1 ≠ 0; b(x) > 0 at every sample; no DC at the output; no instability (NaN, overflow, excursion at the clamp) on a full-scale square at 20 Hz with `rattle` 100 %; 2 solver iterations within 1 dB of 8 on the same signal |
| Throat | 1 and 3 kHz tones at equal pressure: the 2nd harmonic rises 9.5 ± 1.5 dB from 1 to 3 kHz (6 dB / oct); 0 at η = 0 |
| Carbon mic | 1 kHz at −18 dBFS RMS: THD 5 % ± 2 %, 2nd harmonic dominant; frying noise level follows the model's table ±1 dB; `drive` 100 % keeps |h·p| ≤ 0.6 |
| Aliasing | 1.2 kHz sine at full scale, `drive` 100 %, every model at its own speaker and `band` 0: worst alias ≤ −70 dB re the fundamental (the contract's circuit-simulation bound, adopted: gfx-radio is not a `circuit_nl` effect, but its amp and motor are a simulation). With `speaker` None or `band` +100 %: ≤ −65 dB (the contract's part-swap bound). Factors as §2.4 (hard-rail amps 4×, others 2×), recorded per model; AM and NBFM models are measured with `noise` 0 at `tune` 0 |
| Codec transparency | Cell Codec at `codec` 0 %, `dropouts` 0 %: the output equals the same path with the predictor and the bell bank removed (13-bit 8 kHz converter, band, AGC, comfort noise kept): ≤ −120 dB residue, ramps included |
| Codec behaviour | `codec` 50 %: coefficients and bell gains change only at frame boundaries (160 virtual samples), with 3 ms ramps; no broadband step > −60 dB re peak at a boundary; frame level within −8 … +8 dB of the `codec` 0 render on the voice DIs and never above the limiter; every k̂ within ±0.98; identical virtual-sample output at 44.1, 48 and 96 kHz for the same virtual input |
| PLC | History = the decoder's output. Forced single loss on a sustained voiced note: a repeat at the detected period ±2 %; level unattenuated for 10 ms, −20 % per 10 ms after, silence at 60 ms; no click > −40 dB re peak at entry or return; no sample later than the current input used (zero lookahead) |
| Loss process | Gilbert–Elliott: measured loss rate and mean burst length within 10 % of the targets over 10 min; seeded strict null |
| AM clean | `tune` 0, `noise` 0, `dropouts` 0, `whistle` off, m = 0.8: response flat ±1 dB 150 Hz – 3 kHz; THD < 1 % at 1 kHz with the detector at minimum release |
| AM physics | Overmodulation (`drive` 100 %): carrier pinch visible as clipped negative envelope; detuning, 1 kHz tone at m = 0.85: THD < 1 % at `tune` ±10 %, 3 ± 1.5 % at ±22 %, 10 ± 4 % at ±33 % (§2.3's table); `tune` ±33 %: the far sideband of a 2 kHz tone ≥ 20 dB below the near one; diagonal clipping at `drive` 100 %: THD at 2 kHz ≥ 5 %; whistle pitch = |2,500 − Δ| ± 1 Hz along a sweep; a neighbour's babble audible above −40 dB at ±100 %; receiver AGC gain never above +30 dB |
| NBFM physics | Unmodulated carrier, 60 s per point, clicks counted as net 2π slips of the received phase: 0 at 15 dB CNR, 0.5 – 2.5 / s at 8 dB, 10 – 20 / s at 6 dB, 50 – 90 / s at 4 dB (Rice: 1.3 / 17 / 87), monotonic; squelch tail at `squelch_tail` 100 %: 150 ms ± 20 %, its loudness ≤ the reference − 6 LU; `squelch` 0 %: never mutes; key-up band-limited (no energy above 4 kHz beyond −50 dB); `tune` ±100 %: muted by the noise squelch |
| Keying | `squelch` 25 %: after the input stops, the output reaches exact 0 within hang + tail + room tail; the noise never hisses in silence; key-up with a note attacked from silence: the note's level within 1 dB after 20 ms (NBFM: after the squelch attack) |
| EXP | `tune`, `distance`, `drive`, `band` stepped at 50 Hz by a 7-bit pedal: control-rate sidebands ≤ −80 dB (with `whistle` off: the whistle's glide is the effect); 90 % of a step within one interval + 5 ms |
| Switching | Each enum switched during a sustained note: no click > −40 dB re peak (30 ms crossfade, warm-started chain) |
| Level | Every model at defaults: within ±2 LU of the bypass on the pink reference at −18 dBFS RMS and on voice, guitar and drum DIs normalised to it; models with an AGC or a limiter (Telephone, Cell Phone, AM Radio, Laptop) level a quieter or louder source toward the reference by design: reported at −30 and −10 dBFS, not bounded; `drive` 0 → 100 %: within ±3 LU; `distance` 0 → 100 %: −6 LU at most |
| Mix | Each model at `mix` 30 %: deepest notch ≤ 6 dB re the louder of dry and wet (linear sweep at −40 dBFS, `noise` 0), with the model's wet polarity chosen for it; 50 % and 70 %: reported only (§2.7) |
| Rate independence | Each model at 44.1 / 48 / 96 kHz: magnitude within 0.5 dB below 15 kHz; whistle pitch within 1 Hz; demodulated noise level per CNR within 0.5 dB; click rate per CNR within 20 % (prototype: 10 %, plus counting error) |
| Determinism | Two renders with fading, losses, babble, crackle: strict null (all seeded, reset at initialize) |
| CPU | No room ≤ 300 ns / sample, typical ≤ 800 ns, worst ≤ 1,500 ns (§2.9), 48 kHz, mono |
| Listening | Voice (male, female), guitar, drum loop through every model: blind and level-matched against golem today (grit "Megaphone" / "Telephone", eq-graphic "Hygiaphone" / "Lo-fi radio"), against the owner's own device measurements (§7.7), and against a reference futz plugin if the owner has one |

## 6. Positioning

| Pedal | Its job | Difference |
|---|---|---|
| gfx-crusher | The exact virtual converter: rates, formats, aliasing | gfx-radio **uses** the same `converter` (in gfx-dsp, §2.8) for the 8 kHz lines; a converter is not a device. "Phone Line" in gfx-crusher's presets (µ-law 8 kHz, 300 – 3,400) is the codec alone; here it is the whole phone |
| gfx-tape | The recorded medium: tape, cassette, vinyl, 78 | **Its deferred "AM Radio" model lives here and is never appended there** (decided, §7.1): AM is a transmission, not a medium, and its artefacts need the baseband this crate builds. Gramophone here is the horn; the record's noise and wow stay in gfx-tape (chain them) |
| gfx-grit | Multiband dirt: its Megaphone / Telephone presets are band-pass plus clip | gfx-grit seasons a band; gfx-radio is the device, with its channel, keying and room. Both are lofi, so voice lanes get both |
| gfx-eq, eq-graphic presets | Static tone curves | Physical band, dynamic behaviour, noise, distortion |
| gfx-drive | Pedal-family clipping | The amp here is a device's power stage (crossover, rail, sag) with the speaker's own breakup |
| gfx-cab | A real miked cab, any IR, in detail | For a specific phone or radio IR, set `speaker` None and put gfx-cab after it. gfx-radio's speakers are voicings of a type, level-dependent |
| gfx-bassdi | IIR cab voicings | Shares the `matched` body-voicing approach and may share gfx-dsp `speaker` |
| gfx-glitch | Spectral damage with frames and latency | The codec here is time-domain, past-only, zero latency |
| gfx-reverb | Rooms as instruments | The rooms here are short, functional places (a room, a hall, a street) on gfx-dsp's Fdn |

**Against the field**:
- **Speakerphone's chain** is covered as a matrix (mic, channel, speaker, room, plus the dial and the carbon
  degrade). Its sample bay, cover objects, Leslie and convolution reverb are not: other golem blocks do those.
- **FutzBox's set** is covered: filters, distortion, parametric speakers, noise that ducks, dropout gating,
  lo-fi via the converter, mix.
- **Beyond both**:
  - level-dependent speaker breakup (the Klippel motor, rattle, the throat);
  - artefacts from physical simulation: AM detuning, fading, overmodulation, FM clicks, the squelch tail,
    bursty PLC;
  - zero latency throughout;
  - loudness matched to the bypass;
  - noise that stops in silence;
  - an EXP dial.
- **Not attempted**:
  - a bit-exact GSM / AMR codec (latency);
  - the LPC "robot" re-synthesis (a possible later `channel`, appended);
  - acoustic feedback howl (§7.11: not in v1);
  - ambience samples.

## Visual

- **Category `lofi`** (with gfx-crusher, gfx-grit and gfx-tape).
- **Crate hue: olive drab `#4b5320`** (`METAL_CRATE_HUE['gfx-radio']`): field radio, signal-corps PA,
  military megaphone. **Accepted at review**, with thrash.
  - Hue 69° (HSL), L* 34. It sits in the gap between utility amber (39°) and glitch lime (90°).
  - ΔE*ab [A, CIELAB, recomputed at review] to the hues in use or proposed: gfx-tuner's deep green `#126e1c`
    33, gfx-acoustic rosewood 37, gapmeter grey 40, dynamics slate 43, gfx-fuzz copper 46, gfx-tape champagne
    49. Everything else is further.
  - gfx-tuner's review rejected ΔE 19 and accepted its own change at 33-class separation. Lighter olives
    come *closer* to the tuner (`#5c6a1c` 26, `#6b7a1e` 27) and are not proposed.
  - **Why a crate hue and not the family's raspberry**: lofi already has two raspberry thrash pedals
    (gfx-crusher, gfx-grit), told apart by motif alone; a third would not be (gfx-tape left the family hue
    for the same reason). Against gfx-tuner the finish differs as well: olive **thrash** with a yellow
    stencil, deep green **anodized** with a lamp-green light.
  - Olive drab is a greyed colour (HSL saturation 44 %), where the rule for the metal hues is "no grey mixed
    into the hue". It is an exception of the same kind as gapmeter grey, rosewood and champagne, and it is
    the point: the colour *is* the object. Raph confirms on the family sheet (§7.12).
- **Finish: thrash** (the lofi family's, as gfx-crusher proposes). Olive gloss with the motif in yellow
  (`#ffd400`) lined in black reads as stencilled equipment; the diamond plate fits.
  **`RUNE_LIGHT['gfx-radio'] = '#c8d86a'`** is set, not a fallback: the default `mix(hue, '#fff', .3)` gives
  `#818763`, a dull khaki that does not read as a light.
- **Motif `grille`** (new case in `motifParts`, drawn with `shape()` and `line()` so both finishes work),
  in the rune zone under the knob grid:
  - **Speaker grille**: either side of the rune, **4** horizontal pill slots (height 2.4 units, gap 2.0),
    lengths 14, 11.5, 9, 6.5 units shrinking toward the rune and vertically centred on it (`pill()` exists).
  - **Tuning dial**: above the rune, a thin baseline from x 8 to W − 8 with **9** ticks alternating 2.5 / 1.4
    units, and a **needle** (a 1.4-wide line, 6 units tall) at 62 % of the strip, between two ticks: "between
    stations". The draft's 13 ticks and 5 slots fall under a pixel at tile size (the gfx-tuner lesson:
    its segment counts were halved for the same reason).
  - Thrash: slots filled yellow lined in black, the dial as a yellow line. Anodized (if forced): engraved
    (dark 1.6 + light 0.7 offset, the `motifSvg` convention).
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-radio": {"name": "Radio", "blocks": ["radio"], "knobs": ["drive", "noise", "distance", "tune"],
                "selectors": ["model"], "category": "lofi", "color": "#4b5320"}
  ```

  Four knobs, as the largest existing faces (no face in `faces.json` has five). `tune` is inert on 8 of 10
  models but is the EXP target and the pedal's name; `squelch` is expert (§7.6).
- **Glyph** (`visuals/glyphs/radio.svg`, new block type): a bullhorn with a handle and a jagged (distorted)
  sound mark. It is distinct from `volume` (fader) and `cab` (box + cone):

  ```svg
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>Radio / megaphone (gfx)</title><path d='M3 10.5v3h2.5L15 18.5V5.5L5.5 10.5z'/><path d='M7.5 14l1 4.5h2l-.7-3.4'/><path d='M18 8.5l2.5 1.2-2.5 1.2 2.5 1.2-2.5 1.2 2.5 1.2' opacity='.6'/></svg>
  ```
- No maker's livery, logo or trademark. Model names are generic ("PA Horn", not a brand; "Ticket Window", not
  the trademarked name of the eq-graphic preset).

## 7. Open questions — answered at review

1. **gfx-tape's deferred "AM Radio"** → **here, never there.** AM is a transmission; its sound (detuning,
   whistle, fading distortion) comes from the baseband this crate builds, and a tape chain with a "broadcast
   limiter + envelope detection" would be a second, poorer AM. gfx-tape's README still carries the slot (its
   §2.1 table and note, the `(· AM Radio)` in its `model` row, "Radio" in its `speed` row): to remove there.
   Not edited from this review.
2. **Appended models and channels** → none in v1. Order when wanted: **Shortwave** first (it is a new sound,
   not a new voicing, and gfx-freqshift's `ssb` will exist), then the **LPC robot** channel (the Cell Codec's
   lattices with a pulse / noise excitation: nearly free once `codec` exists), then Bullet Mic. Smartphone
   Speaker is a Laptop voicing and TV a Table Radio with a 15.6 kHz whistle: presets, unless a listening
   test says otherwise.
3. **Birdie whistle** → **kept, and named for what it is.** A fixed adjacent-carrier whistle does not move
   with the dial, and at the model's bandwidth it is inaudible (§2.3). The sweeping squeal people remember
   is a spur or a regenerative / BFO beat, and it is the playable gesture. `whistle` stays at −80 dB (off)
   by default, so realism is not at stake.
4. **`squelch` default** → **25 %.** It is a fixed default, identical for every model, so it breaks no
   contract rule; the rule it serves is the stronger one, "nothing surprises a live rig". At −65 dBFS it
   only acts in true silence; a source above it behaves as at 0 %. The price (the first 15 ms of a note
   after silence on NBFM) is the walkie's own.
5. **`mix` combing** → **no dry all-pass.** The wet's phase turns by 1.5 – 2.6 π across its band; nothing
   short of the chain itself matches it. `mix` is a layering control to 35 %, with a per-model wet polarity
   (§2.7); the bench bounds 30 % and reports the rest.
6. **Face** → **4 knobs** (`drive`, `noise`, `distance`, `tune`), `squelch` expert. No existing face has
   five, and `squelch`'s default is right without being touched.
7. **Voicings [I]** → still open, and it is the main risk of this pedal (see the Review). Method as proposed:
   the owner measures his own devices with a log sweep at three levels (response and THD vs level) — a
   landline handset, a phone, a cheap walkie pair, a pocket radio, a megaphone if one can be borrowed —
   and compares against references he owns. The codec's q and α are set on recorded voices against a real
   call recorded through a phone. Measuring a device ourselves copies no one's IR.
8. **Converter dependency** → **no placeholder.** A channel that ships on a plain band and later becomes
   the converter changes every preset that uses it. gfx-crusher is not coded yet: `converter` and `blep` go
   to gfx-dsp at once (ADC and DAC halves separate, §2.8), whichever of the two pedals is coded first.
9. **Stereo** → dual chains with linked keying / losses / fading and the Fdn's two outputs: enough. A panned
   source position is golem's pan, after the pedal.
10. **`noise` type** → float here. The contract's shared-id table does not list `noise`, descriptors are per
    block, and gfx-tape's toggle has its own `noise_mode` / `hiss` beside it. golem-rack is told, for the AI
    vocabulary.
11. **Feedback howl** → **not in v1, and not as a loop.** A real acoustic loop around a horn peak is
    level-unbounded by nature and a live-rig hazard. If it is wanted later: a *synthesised* howl (a sine
    at the horn's main peak, keyed by the input level, level-bounded), appended as an expert parameter
    defaulting to off.
12. **Hue** → olive drab `#4b5320` thrash accepted, rune light `#c8d86a` (Visual). Raph, on the family
    sheet, at tile size next to gfx-tuner.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good.** The central idea is right and it is what no futz plugin does: the artefacts come out of a
simulated chain. It was tested where it could fail, with prototypes
(`/tmp/claude-1000/-home-raph-code/e0dc0b09-d78a-4463-a90e-9918eadd7ea8/scratchpad/proto-gfx-radio/`:
`fm.py`, `am.py`, `motor.py`, `alias.py`, `mix.py`, `codec.py`), and the radio half holds without
reservation: FM clicks do emerge from a discrete-time baseband discriminator, at Rice's rate and at any host
rate; AM detuning distortion emerges from the IF filter; the baseband needs no oversampling; the motor ODE
is stable under any drive and is the Thiele-Small high-pass at low level. The draft had one weak mechanism
(the cell codec), one unphysical curve (Bl), three wrong estimates (CPU, the oversampling factor, the
carbon harmonics) and one open question whose proposed fix could not work (the dry all-pass). It is "Good"
and not "Strong" for one reason only: every voicing constant is [I], on ten devices, and this pedal is
judged by recognition. The physics is settled; the sound is not yet.

Changes made, and why:

| # | Change | Why |
|---|---|---|
| 1 | **Cell Codec: the 7-bell frame stepper → a backward-adaptive order-8 LPC pair** (analysis lattice, LAR-quantised synthesis lattice, regular-pulse excitation mix), on the converter's 8 kHz virtual samples; bells kept for the noise suppressor only; `codec` law rewritten (§2.3, §3) | A bank of bells stepping by ± Δ / 2 is a wobbling graphic EQ: it has neither of a speech coder's signatures. The lattice pair is a real coder's short-term stage, costs nothing, needs no lookahead, and is an **exact** identity at `codec` 0 (prototype: −214 dB, ramps included). Prototype also gives the bounds: stable at every setting, frame levels −7 … +8 dB at 50 %, hence α ≤ 0.5 and a limiter |
| 2 | "Transparent at `codec` 0" restated: predictor and suppressor are identities; converter, band, AGC, comfort noise remain; bench row compares like with like | The draft's bench row needed a "noise suppressor off" switch that does not exist, and the AGC made the two paths differ |
| 3 | PLC: history and pitch detector moved to the **decoder's output**; overlap-add stated causal (it reads the quarter period before the repeated one); loss rate mapped (0.3 % per %); lattices run through the loss | A decoder repeats what it played, not the encoder's input; `dropouts` had no law |
| 4 | **FM threshold quantified** (§1.2, §2.3, §3, §5): Rice's rates, prototype rates at four host rates, `noise` positions (85 / 90 / 100 %), bench row in clicks per second | "Below about 10 dB" hid that 10 dB is one click per 30 s; the texture is at 6 – 8 dB. The rate-independence row asked 5 %; the prototype shows 10 % |
| 5 | **Squelch-tail level bounded** (reference − 6 LU) | No-carrier discriminator noise measures −10 dBFS RMS after de-emphasis, 4 dB above a full-deviation tone, peaks over full scale: true to life and unusable live |
| 6 | NBFM: carrier ramps over 2 ms; `tune` beyond ±50 % stated as muting; Gaussian tails required for the noise; IF filter order given | A step in A is a wideband click the IF does not own; a sum of uniforms would under-produce clicks |
| 7 | **AM**: host-rate baseband justified (±22.5 kHz of content, the wrap lands 18 kHz from the IF); noise specified as a density (variance ∝ fs / B_n); detuning table (THD vs offset); receiver AGC capped at +30 dB; `band` moves the IF; diagonal-clipping onset formula; the 9 kHz whistle stated inaudible at `band` 0 (−57 dB) | "Does not depend on the host rate" was asserted, not shown, and is false without the density rule. An uncapped carrier AGC is +63 dB of static at ±100 % |
| 8 | Channel aliasing paragraph rewritten with measured figures; ADAA kept only on the TX clippers | max(), abs and arg behind a ±3 – 6 kHz IF leave nothing above −90 dB at the host rate |
| 9 | **Motor**: Bl polynomial → rational, always positive; damping written as ω_s · (1/Q_ms + b²/Q_es) (the draft's c had the wrong dimension, and its Q_ts formula with it); κ₃ range, state clamp, solver error figures; rattle gate stated smooth; Le(x) stated out | The polynomial changes sign at 1.4 – 1.8 X_max, reached at `drive` 100 %. With the draft's formula taken literally the prototype diverged at once |
| 10 | **Oversampling: 4× for hard-rail amps, 2× elsewhere**, fixed per amp type (§2.4), with the table | 2× + ADAA passes −70 dB only behind a narrow speaker (−74 dB); the matrix (`speaker` None for gfx-cab) and `band` remove that cover (−56 dB). A factor "raised where the bench fails" would have missed the overrides |
| 11 | Throat: explicit 4th-order low-pass before p² (the motor's output is an acceleration, it band-limits nothing); `band` limited to 9 kHz there | The draft's no-aliasing argument rested on a band limit the motor does not provide |
| 12 | Carbon mic: exact harmonic law r^(n−1); `drive` reaches it at 0.4 dB / % only; DC term high-passed | The draft's "9th harmonic −40 dB" was 36 dB pessimistic; +30 dB into the guard only flattens; the rational curve's DC moves with level |
| 13 | **`mix`: no dry all-pass**; notch table; per-model wet polarity; presets ≤ 35 % or 100 %; bench bounds 30 % | Open question 5's fix cannot work (2.6 π of phase across the band). −22 dB at 50 % on Telephone is certain |
| 14 | Switching: chain before the room duplicated, room shared; warm-up on the live input, not a 50 ms pre-run on silence | A reset chain run on silence stays cold and costs 2,400 samples of work in one cycle; crossfading the room cut a 2.2 s tail in 30 ms |
| 15 | **CPU**: 150 / 500 ns → 600 / 1,500 ns | gfx-reverb measures 525 ns on the same Fdn; the 4× island |
| 16 | `squelch` moved to expert (default 25 % kept); face 4 knobs | No face has five; the default needs no hand |
| 17 | `noise`: continuous to exactly 0 at 0 %; inert on Laptop | "0 % = off" was a 30 dB step at the bottom of the knob |
| 18 | Model table: 100 V line moved from Channel to Amp; "—" defined | It was a channel that the `channel` enum does not have |
| 19 | Level row: matched at the reference level; AGC models reported off it | An AGC cannot match the bypass at every input level |
| 20 | Converter dependency: `converter` + `blep` in gfx-dsp from the start, ADC / DAC halves separate; no placeholder channel | gfx-crusher is not coded; the codec works between the halves; a placeholder is a later sound change |
| 21 | Visual: olive thrash accepted and argued, ΔE recomputed, `RUNE_LIGHT` set, motif reduced to 4 slots and 9 ticks | Default glow is `#818763`; 13 ticks are sub-pixel on a tile |
| 22 | Facts: Thuras-Jenkins-O'Neil formula given [U → I]; 9 kHz plan, 11K0F3E; "a few samples" of latency attributed to the all-pass half-bands; drive table "built at `prepare`" → bench constant | Accuracy; no rendering at `prepare` |

Still open:

| Point | Owner |
|---|---|
| Every voicing constant (speakers, bells, carbon h, crossover widths, AGC times, codec q and α, squelch-tail level): device measurements and listening (§7.7). The pedal's rank against Speakerphone depends on this, not on the algorithms | Raph, then the bench |
| Does the LPC coder *sound* like a cell call? Proven stable, exact and bounded; not heard. If listening rejects it, the fallback is the draft's bell stepper (recorded in §2.3's first paragraph) | Listening, at coding |
| The worst part swap for aliasing (hard rail, `speaker` None, `drive` 100 %: −64 dB at 4× in the prototype): second-order ADAA or not | Bench |
| gfx-tape's README: remove the deferred AM Radio slot (§7.1). gfx-crusher's README: `converter` / `blep` start in gfx-dsp with separable halves (§7.8) | Raph (other documents) |
| New block type `radio`, category `lofi`, roles voice / guitar / keys / drums; `depends` greying for the inert parameters; `tune` as an EXP target; `mains` set at insert if golem knows the country; AI vocabulary: `radio` before grit's Megaphone / Telephone and eq-graphic's Hygiaphone / Lo-fi radio for "phone / megaphone / radio" intents; `noise` float here vs gfx-tape's toggle | golem-rack |
| The Oksanen & Välimäki paper's curve and noise values (PDF not readable at survey): to read before voicing the carbon mic | Coding |
| Olive drab next to gfx-tuner at tile size; the greyed-hue exception | Raph, on the family sheet |
