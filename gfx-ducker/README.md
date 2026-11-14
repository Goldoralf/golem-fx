# gfx-ducker

> **Status: DESIGNED** (2026-11-14), **reviewed** the same day (Fable 5.2, see the Review section). Not
> coded. Design by Claude Opus 5.5. New golem block (no catalogue entry): proposed type `ducker`,
> category `dynamics`. Brings a new gfx-dsp module (`duck`) and uses gfx-dsp `onset`, the module
> gfx-swell's design creates (one module for gfx-swell, gfx-killswitch and this crate).

golem's **ducker**: it lowers a signal while another one plays. Three ways to drive the same gain stage:

- **Follow**: the key's level ducks the main signal. The voice ducks the guitar pad, a band mate's lane
  ducks the backing tracks, the talk mic ducks the music between songs, the dry guitar ducks its own
  delay or reverb tail on a wet branch.
- **Trigger**: each onset of the key (a kick) fires one fixed-shape dip, the same size however hard
  the hit. This is the kick ducking the bass.
- **Pump**: the same dip on the host's beat, with no key. This is the sidechain "pump" of dance music
  on a pad, keys or clean chords.

Every mode can duck the whole band (**Wide**), or only part of the spectrum: a bell where the voice
sits, the lows (a kick against the bass), the highs, or **Spectral**, where each of 16 bands is ducked
by how much of the key sits in it. Latency is zero, with optional lookahead that is reported to the host.

## 1. State of the art

Survey of 2026-11-14. **[V]** read on the maker's page, manual or source · **[R]** review or press text
· **[F]** forum opinion · **[U]** not verified · **[A]** our calculation · **[P]** measured on the
review's prototype (numpy, and a Rust timing of gfx-dsp's own code).

### Commercial references

| Product | What it does best | What we take |
|---|---|---|
| [Wavesfactory Trackspacer](https://www.wavesfactory.com/audio-plugins/trackspacer/) [V] | "An internal **32 band EQ** that reacts to the incoming sidechain signal". One Amount knob, attack / release in ms, low / high cut, processing of the stereo, mid or side signal "or anything in between", a display of the key's spectrum and of the reduction curve, and an internal sidechain when none is connected (v2.5.8; current 2.5.10). The filter type and the latency are not published; shop listings call it zero or low latency [R] | **Spectral** shape; Mid-only ducking of a stereo backing |
| [oeksound soothe2](https://oeksound.com/manuals/soothe2/), external sidechain [V] | Analyses the key's spectrum and attenuates the same frequencies in the main. FFT-based. A user measured 2,048 samples of latency at 48 kHz ([gearspace](https://gearspace.com/threads/make-soothe2-attack-faster.1433670/)) [F]. oeksound states that the DAW must align the key ([support](https://oeksound.com/support/sidechain-latency-compensation)) [V] | The idea. Not the latency: our Spectral mode is an IIR filter bank |
| [sonible smart:comp 2](https://www.sonible.com/blog/smartcomp-working-with-sidechain-ducking/) [V] | **Spectral ducking**: compresses only the frequency regions where the main clashes with the key | Ducking limited to the key's bands |
| [FabFilter Pro-C 2](https://www.fabfilter.com/news/1440576000/fabfilter-releases-fabfilter-pro-c-2-compressor-plug-in) [R] | External sidechain with sidechain EQ, a **Pumping** style, Hold up to 500 ms, smooth lookahead up to 20 ms that can be switched off for zero latency | Hold; optional lookahead; zero latency by default |
| [Nicky Romero Kickstart 2 (Cableguys)](https://www.musictech.com/reviews/plug-ins/cableguys-kickstart-2-review/) [R] | Host sync, MIDI trigger or **audio trigger**; 16 dip curves; **Band Split** pumps only the lows; a view of the kick against the curve | **Trigger** mode; Low shape |
| [Cableguys VolumeShaper](https://www.cableguys.com/volumeshaper) [V] | 3 bands with a curve each; beat sync, MIDI or **audio trigger** (restart on each transient); external sidechain; **lookahead pre-smoothing** before a trigger; smoothed bypass | Trigger restarts the curve; a deep dip needs a little lookahead to stay click-free |
| [Xfer LFOTool](https://www.kvraudio.com/news/xfer-records-updates-lfotool-to-v1-1-win-and-mac-vst-and-au-19838) [R] | Drawn volume and filter graphs, synced or retriggered by MIDI (Envelope mode plays once). No audio trigger [R] | The pump curve as a shape with tension, not an LFO |
| Devious Machines Duck 2 ([KVR](https://www.kvraudio.com/news/devious-machines-releases-duck-2-66374), [rekkerd](https://rekkerd.org/devious-machines-releases-duck-2-sidechaining-volumeshaping-plugin/)) [R] | Drawn curves, synced or triggered by audio or MIDI (the envelope can hold while the trigger lasts); 2 or 3 bands with an envelope each, crossovers of 12 / 24 / 48 dB/oct or linear phase; one offset in ms or samples that replaces the old lookahead | Confirms Trigger, band-limited dips and a time offset. Not taken: crossovers (0 dB would no longer be exact), MIDI trigger (golem sends effects no MIDI) |
| Kilohearts (Multipass + Volume / Trance Gate snap-ins) [U] | Band-split volume shaping in a modular host | Confirms band-limited pumping as a standard need |
| Pioneer DJM "Talk Over" ([DJM-750 manual](https://office.manualsonline.com/manuals/mfg/pioneer/djm750.html?p=21), [DDJ-SZ](https://www.manualslib.com/manual/878738/Pioneer-Ddj-Sz.html?page=26)) [V] | **Normal** lowers every non-mic channel across the full band. **Advanced** (the factory default) lowers **only the midrange**. Levels −6 / −12 / −18 (default) / −24 dB | Talkover = a Bell duck by default; −18 dB as the reference depth |
| [Audacity Auto Duck](https://manual.audacityteam.org/man/auto_duck.html) [V] | Broadcast-style voice-over ducking: −12 dB, 0.5 s fades, a **1 s maximum pause** bridges the gaps between words, threshold −30 dB RMS. Offline, two passes | Hold sized for gaps between words; fixed depth rather than a ratio |
| Broadcast duckers / auto-mixers [U] | Fixed-depth "talkover" with a hold time; threshold above the mic's ambient level | Depth in dB, hold, a threshold set relative to the mic's floor |

### Open source

| Project | Idea |
|---|---|
| [Calf Sidechain Compressor](https://calf-studio-gear.org/doc/Sidechain%20Compressor.html) [V] | Two filters on the detection signal; a **split** mode that reduces one frequency range only |
| [LSP Compressor](https://lsp-plug.in/?page=manuals&section=compressor_mono) [V] | Peak / RMS / LPF / SMA detectors; feed-forward, feed-back or a **shared-memory link** as the sidechain source; reactivity; lookahead; sidechain HPF / LPF with slope; listen |
| x42 / Ardour a-comp [U] | Ardour's own a-comp has a sidechain input. We found no x42 ducker | — |
| gfx-delay / gfx-reverb / gfx-shimmer (ours, read in `src/lib.rs`) [V] | Self-ducking of the wet: peak follower (5 ms attack; release `duck_release` 50 – 2,000 ms, fixed 400 ms in shimmer) on the dry; activation linear over **6 dB above the threshold**; depth `duck` 0 – 24 dB (shimmer: 0 – 100 % of 18 dB); linear gain smoothed by a one-pole of 0.999 **per sample**. Delay sets the threshold in dB (`duck_thresh`); reverb sets it from `duck_sens` (threshold = −60 · sens / 100 dB, so 50 % gives −30 dB); shimmer fixes it at −30 dBFS | Our Follow law covers theirs: with `knee` 6 dB the static curve is the same within 10 % of the depth (an S across the knee where theirs is a straight line) |
| gfx-gate, gfx-met (ours) | Key on its own auxiliary bus; sliding-maximum close detector sized from the key's high-pass (no ripple on low notes); `state` / `gr` meters | Reused as is |
| gfx-deesser (ours) | Dynamic matched bell / shelf redesigned every 32 samples, coefficients ramped; filter left out of the path at 0 dB, so it is bit-transparent; `detect` Absolute · Relative | The band shapes; the `detect` id |
| gfx-killswitch (ours) | Zero-latency pick detector (fixed 1.5 kHz high-pass, then fast / slow envelope difference, `sens`), phase from the host's position | The Pump phase. Its detector is **not** taken as it is: without the high-pass it fires twice on one kick (§2.4) |
| gfx-swell (design, reviewed) | gfx-dsp `onset`: level, pick (A), flux and pitch detectors; reported `lookahead` enum, setup only, crossfaded on change | The `onset` module (one module, §2.10); the lookahead rules |

### Forum feedback: what people love and hate

- Trackspacer is loved for its one-knob speed. Some report "scratchy artefacts" with fast attacks, and
  ask for lookahead and oversampling ([gearspace](https://gearspace.com/threads/plugin-for-dynamic-eq-ducking.1194318/),
  [KVR](https://www.kvraudio.com/forum/viewtopic.php?p=8648734)) [F]. Others hear no difference from a
  well-set dynamic EQ on vocals ([motunation](https://www.motunation.com/forum/viewtopic.php?p=599938)) [F].
  Lesson: a fast gain change on a low band clicks. That calls for a minimum attack, S-shaped edges, and
  lookahead as an option.
- Live, the vocal mic is never silent: it picks up the drums and the backline. A threshold set at
  soundcheck drifts during the gig [U, common live-sound experience]. Lesson: a **relative** detector
  that measures how far the voice rises above the mic's own floor.
- A ducker that releases between words sounds like "pumping" on speech. Broadcast tools bridge the gaps
  with a hold of about 0.5 – 1 s (Audacity's maximum pause). Lesson: Hold, sized for speech.
- Sidechain pumping should follow the tempo. A recovery set in ms must be redone when the song changes
  (the reason drawn curves won). Lesson: in Trigger and Pump, the dip's length is a fraction of a note
  value.

### What we conclude

1. **One gain stage, three drivers** (Follow, Trigger, Pump). They share depth, shapes, stereo,
   lookahead and the meters. Pump lives here, not in gfx-killswitch or gfx-tremolo (§2.6).
2. **Fixed depth, not a ratio.** Above the threshold plus a `knee` window, the duck reaches `depth`
   whatever the key's level, as the broadcast and DJ talkovers do. A wide `knee` gives the proportional
   feel of a sidechain compressor.
3. **Relative detection** against the key's own floor, for live mics with bleed. The absolute threshold
   is the default, so presets keep a plain meaning. Relative is the hard part of this block and is at
   risk until it is proven on a recorded stage mic (§2.2).
4. **Frequency-selective ducking** with zero latency: a dynamic matched bell or shelf (gfx-deesser's
   mechanism), and an IIR 16-band **Spectral** mode. soothe2 is FFT-based and late; Trackspacer does not
   publish its method. Spectral is a "make room" tool at half-octave resolution, not a resonance
   suppressor.
5. **No steps, no zipper**: the gain moves per sample, in dB, along raised-cosine edges. An edge is as
   clean as its length allows and no cleaner: 0.5 ms on a bass note is audible as a tick whatever its
   shape (§2.3, measured). The default attack is 10 ms; optional lookahead is reported to the host and
   is a setup setting.
6. Not taken: FFT spectral ducking (it adds latency), a voice-activity classifier on the key (guitar
   bleed is harmonic too, so pitch clarity cannot separate it from singing), and a ratio-based sidechain
   compressor (that is gfx-comp's job, once it gets a key bus).

## 2. Design

### 2.1 Signal path

```
main in ─▶ [lookahead delay, if on] ─▶ M/S (if `stereo` Mid / Side) ─▶ gain stage ─▶ M/S decode ─▶ out
                                                                            ▲  Wide: × g
                                                                            │  Bell / Low / High: dynamic matched SVF
                                                                            │  Spectral: 16 dynamic bells in series (solver)
key in (aux bus) ─▶ HPF `hpf` ─▶ LPF `lpf` ─┬▶ level detector ─▶ threshold (absolute or floor + margin) ─┐
                                            ├▶ onset detector (Trigger) ──────────────────────────────┤
                                            └▶ 16-band analysis (Spectral) ───▶ band weights ─────────┤
host transport ─▶ beat phase (Pump) ──────────────────────────────────────────────────────────────────┤
`force` (footswitch) ───────────────────────────────────────────────────────────────────────────────────┘
                                   ▶ activation a ∈ [0, 1] (attack / hold / release or curve) ▶ duck dB = depth · S(a)
```

### 2.2 Key detector (Follow and Trigger)

- **Filters**: `hpf` and `lpf` are Butterworth 2-pole TPT SVFs (gfx-gate's ids and laws). They set the
  band the detector listens to. Voice: 150 Hz – 4 kHz. Kick: 0 – 150 Hz.
- **Level L**: the **sliding maximum** of |x| of the filtered key over one period of the lowest content,
  as in gfx-gate: window = clamp(1.2 / max(`hpf`, 30 Hz), 8 ms, 40 ms), read in dB. It rises with the
  key, sample by sample, and falls one window late. The level never ripples on a bass or kick key, so the
  gain cannot flutter.
- **Threshold T** (peak, like gfx-gate and the time effects' ducking):
  - `detect` Absolute: T = `thresh`.
  - `detect` Relative: T = max(`thresh`, F + `margin`). F is the **key floor**: the level of what the mic
    picks up when nobody sings into it. **At risk: a candidate rule, proven on synthetic signals only.**
    - *Start*: F = `thresh` − `margin`, so T = `thresh`.
    - *Under T, quiet for a while* (no sample above T in the last 150 ms): F rises toward L at 40 dB/s.
      This is bleed, by definition, and F follows its peaks.
    - *Under T, any time*: F falls toward L at 6 dB/s (slow enough to bridge the gaps between drum hits).
    - *Short excursion above T* (it ends within 80 ms): a drum hit. When it ends, F rises by up to 3 dB
      toward the excursion's peak.
    - *Long excursion above T*: singing. F rises at 0.5 dB/s, so a long note does not pull the threshold
      up to itself (10 dB in 20 s).

    Why each line is there [P]:
    - Bleed on a stage is impulsive: drum hits stand 20 dB or more above the gaps between them. The
      first draft's floor "falls at once to any lower level", so it sat in those gaps and every snare
      hit ducked the pad: 29 % of the time on a synthetic drum bleed, all of it with `hold` 250 ms.
    - Following every level under T fails the other way: the tails of sung words pass under T and pull
      F up, word after word, until the threshold reaches the voice (phrases ducked 33 % of the time).
      Hence the 150 ms guard after anything above T.
    - With the guard alone a louder backline is never learned (its hits are above T): hence the
      short-excursion rule.
    - Result on the synthetic test (hits every 250 ms, voice 20 dB above them, `thresh` −40 or −50,
      `margin` 10, `hold` 250): bleed alone never ducks once learned (0 – 36 % during the first 4 s); the
      phrases duck 100 % of the time; after the bleed jumps by 8 dB, 19 % of the next 4 s, then 0 %.

    **Honest limits.** A level detector cannot tell bleed from a voice that is less than `margin` above
    it: that voice is lost, and Absolute is the right mode. A sustained bleed that jumps (a cymbal wash,
    a wedge turned up) is "singing" and is caught at 0.5 dB/s only. `thresh` must sit above the room's
    quiet level: at the room's own level the hits stay above T too long to be classed as hits (41 % of
    ducking on bleed in the test). The five constants are starting values; a recorded stage vocal mic
    decides them, and whether Relative ships in v1 (open question 2). Absolute is the default and the
    presets' base, so nothing else waits for this.
- **Activation target**: a* = clamp((L − T_eff) / `knee`, 0, 1), or a step when `knee` = 0.
  **Hysteresis** shifts the whole curve on the way down: T_eff = T while the detector is closed (a* = 0),
  and T + `hyst` (`hyst` ≤ 0) from the moment a* leaves 0 until L falls below T + `hyst`. With `knee` 0
  this is gfx-gate's open / close pair.
- **No key = no duck.** A key bus that is absent or silent reads L = −∞, which is under any threshold:
  a* = 0 and the output is the input, bit for bit. No detection of "connected or not" is needed.
  gfx-gate and Met read their main input instead; a ducker keyed on its own main would be a fixed-depth
  compressor, never what the player meant. Pump needs no key.

### 2.3 Activation envelope and gain (new gfx-dsp module `duck`)

- The activation a moves **linearly** toward a* per sample:
  - up at 1 / `attack` per second (`attack` ≥ 0.5 ms);
  - down at 1 / `release` per second, but only after a* has stayed below a for `hold` ms. A new rise
    during hold or release resumes from the current a, so the gain never jumps.
- **Gain**: duck_dB = `depth` · S(a), with S(a) = (1 − cos πa) / 2 (raised cosine). The gain is
  10^(−duck_dB / 20), computed **per sample** (6 ns [P]). There are no block steps or corners, so there
  is no zipper noise. A full duck takes exactly `attack` ms with S-shaped ends, and the release is a
  raised-cosine fade over `release` ms.
- **What a fast edge costs** (Wide, main = 80 Hz sine at its worst phase, energy the edge adds above
  2 kHz / above 10 kHz, dB under the sine's peak [P]):

  | `attack` | `depth` 12 dB | `depth` 18 dB | `depth` 60 dB |
  |---|---|---|---|
  | 0.5 ms | −26 / −58 | −23 / −55 | −15 / −43 |
  | 1 ms | −38 / −71 | −34 / −68 | −23 / −57 |
  | 2 ms | −52 / −83 | −48 / −80 | −36 / −69 |
  | 5 ms | −68 / −99 | −65 / −96 | −54 / −85 |
  | 10 ms | −81 / −111 | −77 / −108 | −67 / −97 |

  Under 2 ms a duck on a low, sustained sound is a tick. That is right when a kick lands on it (Trigger
  on a bass: the kick masks it) and wrong on a pad under a voice. A deep duck is faster than its
  `attack` says: in dB the first quarter of the edge already removes most of the amplitude. Use 5 ms or
  more, or `lookahead`, wherever nothing masks the edge.
- **Band shapes** move at the control tick (§2.7). Their fastest edge is two ticks (1.3 ms): a shorter
  `attack` is clamped there for Bell, Low, High and Spectral.
- `depth` and `freq` are EXP-capable (the pedal is a talkover amount, or sweeps the bell). They run
  through gfx-dsp `exp_smooth`; the other continuous parameters run through `smooth::OnePole`, 20 ms.
- `force` (momentary footswitch "Duck now"): sets a* = 1 while held, and runs through the same attack,
  hold and release.
- **Self-ducking compatibility**: Follow with `knee` 6 dB, `attack` 5 ms, `hold` 0 gives the static
  curve of gfx-delay / gfx-reverb within 10 % of the depth. The motion differs, on purpose: theirs
  smooths the linear gain with a one-pole of 0.999 per sample, a time constant of 999.5 samples, so
  22.7 ms at 44.1 kHz, 20.8 ms at 48 kHz, 10.4 ms at 96 kHz and 5.2 ms at 192 kHz (verified in the
  shipped code, open question 9).

### 2.4 Trigger mode

- **Onset detector**: gfx-dsp `onset`'s pick detector (gfx-swell's detector A), fed with the level L of
  §2.2 instead of its own high-passed fast envelope:
  - slow reference = the 150 ms average of L in dB;
  - it fires when L − slow > R, with R = 12 − 9 · `sens` / 100 dB (12 → 3 dB), L rose by more than R / 2
    within max(3 ms, 0.5 / `lpf`), L > T, and the detector is armed;
  - it re-arms when L − slow < R / 2; refractory 40 ms.

  **Why not gfx-killswitch's detector on the user's filters** (the first draft): its fast envelope
  releases in 5 ms, which is right above its fixed 1.5 kHz high-pass and wrong on a kick. A 55 Hz body
  ripples the envelope by more than 10 dB per half-cycle, so the detector re-arms and fires again in the tail: 7
  triggers for 4 kicks, 2 for one long 808, with or without `lpf` 150 Hz [P]. On the sliding maximum the
  same signals give 4 and 1, 1 – 2 ms after the hit, and three bass notes bleeding 14 dB under the
  kicks add none [P].
- **Kick mic with bass bleed**: `thresh` does the separation (the kick is far louder in its own mic),
  and each dip has the same depth whatever fired it. Two key settings work: the body (`hpf` 0, `lpf`
  150 Hz), or the beater's click (`hpf` 2,000 Hz), which the bass hardly reaches and which answers
  faster.
- Each onset starts the **dip curve** from the current gain:
  - a fall to full depth over `attack` (raised cosine in dB, as §2.3);
  - a stay at full depth for `hold`;
  - a recovery to 0 dB over the rest of the curve, shaped by `tension`.
- **Curve length**: with `sync` Off, attack + hold + `release` ms. With `sync` on, `length` % of the note
  value at the host tempo (1/4 at 120 BPM, `length` 75 % → 375 ms). If attack + hold exceed **half** the
  length, both are scaled down proportionally: the recovery always has at least half the dip.
- **Recovery shape**, drawn in linear gain as the curve plug-ins draw it. With u ∈ [0, 1] the recovery
  phase, g_min the gain at full depth and p = 4^(|`tension`| / 100):
  - `tension` ≥ 0: g(u) = g_min + (1 − g_min) · u^p. The gain stays low and returns late (the club
    "pump"). The join with the hold is smooth (zero slope); the corner is at the top, where the gain
    reaches 0 dB.
  - `tension` < 0: g(u) = g_min + (1 − g_min) · (1 − (1 − u)^p). It snaps back early and lands smoothly
    on 0 dB; the corner is at the start, with a finite slope of p / recovery time.
  - 0 is linear in amplitude, with a small corner at each end.

  The first draft used u^p for both signs. For p < 1 its slope at u = 0 is infinite: at `tension` −100
  the gain crossed 40 dB in its first sample, a click at −27 dB above 10 kHz on an 80 Hz sine. The mirrored
  form measures −65 dB or lower (recovery 30 ms, `depth` 60) [P]. The remaining corners are slope
  breaks, not steps, and need no rounding.

### 2.5 Pump mode

- The same dip curve, restarted every `sync` note value.
- The phase comes from the host's **position** (gfx-dsp `transport::phase_at`, f64 per sample),
  anchored on beat 0 like gfx-killswitch, plus `offset` (% of the period) to sit on the kick.
- With the transport stopped, the curve free-runs at the host tempo from engage (fallback 120 BPM).
  With `sync` Off, it free-runs with a period of attack + hold + `release` ms.
- **Lookahead brings nothing to Pump** (the dip is known in advance), but if it is on, the main is late
  by L and the dip must still land on the beat. The gain applied to an output sample belongs to the
  input L samples earlier, so the phase is read at **position − L**, on the assumption that the host
  reports the position of the audio it hands in. Hosts differ on that [U]: the Pump sync check (§5)
  runs with `lookahead` on in REAPER and in the Golem Engine, and the sign is fixed there.

### 2.6 Positioning against gfx-killswitch and gfx-tremolo

| | gfx-killswitch | gfx-tremolo | gfx-ducker Pump / Trigger |
|---|---|---|---|
| Purpose | Rhythmic gating, stutter, kill | Amp tremolo voicing | Sidechain-compressor dip |
| Shape | Steps (16, accents, swing), Gate / Sine / Triangle / Ramps | Symmetric LFO into a circuit | One asymmetric dip per beat: fast fall, hold, tension-shaped recovery |
| Trigger | Engage, own pick, beat | Free / sync | Beat, **another instrument's** onsets, a key level |
| Band | Full | Full (Harmonic: two antiphase bands) | Wide, Bell, Low, High, Spectral |

Killswitch's Ramp Up approximates a pump on a full band. It stays as it is: no parameter moves between
the pedals. The ducker owns the dip because the dip is a ducking envelope: the same gain stage, depth
and band shapes as Follow, and Trigger is that dip fired by a key.

### 2.7 Gain stage and shapes (`shape`)

| `shape` | Implementation | 0 dB of duck |
|---|---|---|
| **Wide** | x · g, g per sample | Identity, bit-exact |
| **Bell** | Matched bell (gfx-dsp `matched::design`, `Analog::bell`) at `freq`, Q from `width` in octaves: Q = √(2^N) / (2^N − 1) (2 oct → 0.67, 1 oct → 1.41). Gain −duck_dB. Redesigned every control tick, coefficients ramped linearly (`Svf::set_biquad_ramped`), as gfx-deesser does | Filter out of the path (bit-exact). It re-enters from reset state with a near-0 dB gain, so there is no click |
| **Low** | Matched low shelf at `freq`, Q 0.7: only the lows duck. This is Kickstart's band split, done without a crossover, so 0 dB is still exact | Same |
| **High** | Matched high shelf at `freq`, Q 0.7 | Same |
| **Spectral** | 16 matched bells in series at f_k = 56.6 · 2^(k/2) Hz (56.6 Hz – 10.2 kHz, half-octave spacing, **Q 2.0**). Band k gets the cut the solver gives it (below) | Each bell is out of the path while its gain is 0 |

- **Control tick**: 1 / 1,500 s (32 samples at 48 kHz, 128 at 192 kHz), so the motion is the same at
  every sample rate (gfx-deesser counts 32 samples at any rate: noted, not changed).
- **Shelves**: `freq` is where half the duck (in dB) applies. A second-order shelf slopes at 12 dB per
  octave at most, so a deep duck reaches further: at `depth` 24 dB the Low shape takes 12 dB at `freq`
  and still 3 dB one octave above it; at 60 dB, 7 dB two octaves above [P].
- **Stability**: the TPT SVF stays stable while its coefficients move, and gfx-deesser ships it up to 24 dB. Up
  to 60 dB is new: the bench checks for overshoot (§5).

A wider alternative was rejected: a fixed-pole parallel bell (x + (G − 1) · BP(x)) costs less and needs
no redesign, but its notch narrows as the cut deepens, so a −18 dB talkover would hollow a thin slice
instead of the vocal band [A].

**Spectral weights.** The filtered key feeds 16 band-pass filters at the same f_k. Each is **two
cascaded SVF band-passes** (Q 2.0 each, 4th order), followed by a peak follower (2 ms / 60 ms) read in
dB, giving K_k.
- Why 4th order: a 2-pole band-pass falls by 6 dB per octave only. A pure tone on a centre then reads
  −7 dB in the next band and −13 dB in the one after (Q 2.87), so one note ducked four or five bands at
  the default `spread`, and the valleys between a voice's formants were filled in. Cascaded, the same
  tone reads −9.5 and −20 dB, and −3.4 dB in both bands when it sits between two centres [P].
- w_k = clamp(1 − (K_max − K_k) / `spread`, 0, 1): the key's loudest band takes the full duck, and bands
  `spread` dB below it take none.
- The overall activation a still comes from the broadband detector, so threshold, relative floor, hold
  and release behave as in Wide. The spectrum decides only **where** the duck goes.
- w_k is smoothed with the same activation ramp rates, so the cuts do not flicker with the key's
  formants.
- Cutting a band where the main has no energy is inaudible, so no masking test on the main is needed.

**Spectral solver (bells in series add up).** The target at each centre is t_k = duck_dB · w_k. Giving
each bell its own target, as first written, does not produce that curve: neighbours overlap. With all 16
bells at a nominal 12 dB the sum is **18.7 – 20.1 dB** at the centres with 4.4 dB of ripple, and
30 – 32 dB for 18 [P]. More bands do not help (32 bells a quarter-octave apart give the same excess
[P]); this is gfx-geq's finding, in a cut-only form.
- **Interaction matrix** B (16 × 16, fixed): B_jk = the response of bell k at centre f_j, per dB of its
  own cut, taken at 12 dB (0.35 for a neighbour, 0.12 for the next). Computed once in `prepare`.
- **Projected Gauss-Seidel**, cut-only: for each band in turn, g_k = max(0, t_k − Σ_{j≠k} B_kj · g_j).
  Two sweeps per tick, warm-started from the previous tick (256 multiply-adds a sweep). No bell ever
  boosts, so a band whose target is 0 stays out of the path, and g_k ≤ t_k ≤ `depth`.
- **Result** [P], four sweeps from cold, Q 2.0: the deepest point of the sum is within 0.2 dB of `depth`
  for uniform, single-band, triangular and block targets at 6, 12 and 18 dB; the active centres are
  within 0.8 dB; the ripple between centres of a uniform cut is 0.7 dB at 6, 1.3 dB at 12 and 1.8 dB at
  18. At Q 2.87 (the first draft: bandwidth = spacing) the ripple is 2.5 dB at 12, so the bells are
  widened to Q 2.0.
- What stays, by nature: a single ducked band takes its neighbours with it (a 12 dB cut gives 4.2 dB
  half an octave away, 1.5 dB at one octave, 0.3 dB at two).
- **Coefficient tables instead of designs.** A matched design costs **925 ns** [P] (its 25-point fit),
  so 16 designs per tick are 460 ns per sample: six times the first estimate, and more than the bells
  themselves. The centres and Q are fixed, so `prepare` fills a table per band (SVF coefficients for
  0 – 60 dB in 0.5 dB steps: 1,936 designs, about 2 ms, off the audio thread), and a tick is a linear
  interpolation in it, then `set_biquad_ramped`. Bell, Low and High keep the live design: one per tick
  is 29 ns per sample.
- **The band count is internal**, not a parameter. If listening against Trackspacer asks for more, 24
  bands a third of an octave apart is a change of constants and tables (bench decides, §Review).
- Bench limits (§5): depth of the sum, ripple, and a single-tone key.

### 2.8 Stereo, layouts, meters, bypass

- **Layouts**: main **1 / 1 first**, 2 / 2 second (a ducker is not stereo by nature). Each layout has a
  **key auxiliary bus** of 1 or 2 channels, exactly as gfx-gate and gfx-met (contract: the key is a
  separate auxiliary bus).
- Key detection reads max(|kL|, |kR|). The gain is linked: the same on both channels.
- `stereo` (2 / 2 only): **Both** · **Mid** · **Side**. Mid ducks M = (L + R) / 2 and leaves S alone
  (Trackspacer's Mid). A stereo backing makes room for a centred voice and keeps its width.
  - Written as a difference, so nothing is encoded and decoded: with M' the ducked mid,
    L' = L − (M − M') and R' = R − (M − M'). At 0 dB of duck M' = M and the output is the input, bit for
    bit; the side signal is untouched at any depth. Side: the same with S = (L − R) / 2 and opposite
    signs.
- **Meters** (read-only, golem's nih-plug fork): `gr` (current duck, dB, positive; in band shapes the
  deepest target) and `state` (1 while `gr` > 1 dB).
- **Bypass**: `bypass` (VST3 `kIsBypass`), 10 ms gain crossfade, as the coded effects ship. With
  `lookahead` on, the bypassed signal is the **delayed** main: the reported latency never changes with
  bypass.
- `listen`: outputs the filtered key, for setting `hpf` / `lpf`. Crossfaded over 5 ms. Silence when no
  key is connected.

### 2.9 Latency, CPU

- **Zero latency** with `lookahead` Off, the default. Off is exactly 0 samples: no oversampling, and the
  filters are only in the path while ducking.
- `lookahead` 2 / 5 / 10 ms delays the main (and only the main) by round(ms · fs) samples, which is
  reported to the host through `set_latency_samples`, called only when the value changes. It is an
  **enum and a setup setting**, the rule of gfx-limiter (Off · 1 · 2 · 5 ms, shipped) and gfx-swell
  (Off · 3 · 10 · 25 ms): set at insert or by a preset, never automated, never a scene value, never
  switched inside a song. The audio is crossfaded over 20 ms across a change. The values differ per
  effect because the job does: here 5 ms covers the default-size fall, and 10 ms a slow, inaudible one.
- With 5 ms, an attack of 5 ms finishes as the key's onset reaches the output, provided the host hands
  in the key aligned with the main (§6, item 4).
- **CPU** at 48 kHz per mono channel, against Met's measured 96 ns/sample. Parts marked [P] were timed
  on gfx-dsp's own code (release build, this machine); the rest are estimates [A]:

  | Shape | Cost |
  |---|---|
  | Wide | ≈ 30 ns (10^x per sample: 6 ns [P]) |
  | Bell / Low / High | ≈ 60 ns (one SVF + one matched design per tick: 29 ns [P]) |
  | Trigger | + 15 ns |
  | Spectral | ≈ 220 ns mono, ≈ 320 ns stereo: 16 bells per channel (≈ 90 ns when all are in [P]), 32 analysis band-passes with followers (≈ 100 ns; 16 measured at 50 ns [P]), solver ≈ 16 ns, tables ≈ 1 ns. As first written (16 live designs per tick) it measured ≈ 610 ns mono [P] |

  Spectral is under 2 % of a core in stereo. Bells whose target is 0 are skipped, so a typical voice key
  costs less.

### 2.10 gfx-dsp reuse and what is new

- **Reused**:
  - `envelope::{PeakFollower, SlidingMax}`;
  - `svf::Svf` (filters, `set_biquad_ramped`);
  - `matched::{design, Analog::bell / low_shelf / high_shelf}`;
  - `transport::{Sync, TransportInfo::phase_at, note_samples}`;
  - `smooth::{OnePole, LinearRamp}`, `exp_smooth`;
  - `delay::DelayLine` (integer read, lookahead);
  - gfx-gate's aux-bus wrapper.
- **New `duck` module**: the activation envelope (linear ramps, hold, raised-cosine map, knee,
  hysteresis), the floor tracker, the dip-curve generator (attack / hold / tension recovery, sync length)
  and the Spectral weights, solver and tables. gfx-delay, gfx-reverb and gfx-shimmer can adopt it later
  (open question 9).
- **`onset`: one module, gfx-swell's.** gfx-swell's design already creates gfx-dsp `onset.rs` (level
  gate L, pick detector A, flux B, pitch jump C, strum logic). This crate must not create a second one
  by moving gfx-killswitch's detector. What the ducker needs from it:
  - detector A as a **standalone type** (`onset::Pick`), usable without L, B, C or the strum logic;
  - its envelope source selectable: A's own high-passed fast envelope (gfx-swell, gfx-killswitch), or an
    **external level in dB** (the ducker's sliding maximum, §2.4);
  - R, the slope window, the floor and the refractory time as arguments.

  Whichever of gfx-swell and gfx-ducker is coded first writes `Pick` to this interface. gfx-killswitch
  moves to it afterwards, as a separate change with its own listening: its `sens` mapping (rise 12 → 2
  dB, floor −30 → −60 dBFS) stays in the crate, and A's dB slow reference changes when it fires.
  gfx-transient does not use `onset`, by design (a shaper does not decide "onset or not").
- **Key input in the bench.** gfx-gate has its own `process_with_key`; neither gfx-dsp's `Effect` trait
  nor gfx-bench knows a key today, so the gate's common checks run unkeyed. To add:
  - gfx-dsp `effect::Effect` gains `process_with_key(buffers, key: Option<&[&[f32]]>)`, which defaults
    to `process`. gfx-gate, gfx-met, gfx-ducker (and later gfx-swell, gfx-transient) implement it;
  - gfx-bench `Options` gains a `key` signal, so the common checks (clicks, silence, determinism,
    latency, CPU) run **while ducking**. Without a key the ducker is the identity and every check would
    pass trivially;
  - `param_steps` skips `lookahead` (it changes the reported latency), as gfx-transient asks.

## 3. Parameters

A new block, so there are no catalogue ids to keep. gfx-gate's ids are used wherever the meaning is the
same (`thresh`, `attack`, `hold`, `release`, `hpf`, `lpf`, `hyst`, `knee`, `listen`, `state`, `gr`), plus
the shared `sync`, gfx-killswitch's `sens`, `length` and `offset`, gfx-deesser's `detect` and `shape`,
and the `lookahead` enum of gfx-limiter and gfx-swell. All ranges are linear (contract); "Curve" is
display only. No default depends on another parameter. Enums only grow at the end.

### Front panel

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `mode` | Mode | e | Follow · Trigger · Pump | Follow | — | crossfade 20 ms | | What drives the duck: the key's level, the key's onsets, or the beat |
| `thresh` | Threshold | f | −90 – 0 dB | −40 dB | lin | 20 | | Peak level of the filtered key where ducking starts. In Relative, the lower bound under floor + `margin` |
| `depth` | Depth | f | 0 – 60 dB | 12 dB | lin | EXP rule | ✓ | Full duck, a positive amount in dB, whatever the key's level above threshold + knee. The talkover reference is 18 dB |
| `attack` | Attack | f | 0.5 – 500 ms | 10 ms | log | — | | Time to reach full depth (raised-cosine edge). In Trigger and Pump, the dip's fall. Under 2 ms it ticks on low, sustained sounds (§2.3); band shapes clamp it to 1.3 ms |
| `hold` | Hold | f | 0 – 2,000 ms | 150 ms | log | — | | Stays ducked this long after the key drops: bridges the gaps between words. In Trigger and Pump, the stay at full depth (at most half the dip with `attack`) |
| `release` | Release | f | 10 – 5,000 ms | 400 ms | log | — | | Return from full depth to 0 dB. In Trigger and Pump with `sync` Off, the dip's recovery |
| `shape` | Band | e | Wide · Bell · Low · High · Spectral | Wide | — | crossfade 20 ms | | Where the duck acts (§2.7) |

The defaults are sound-forward: an inserted ducker on a lane with a key ducks audibly. They are not
neutral, because a ducker at 0 dB depth looks broken. The same holds for the other modes without any
mode-dependent default: `sync` defaults to **1/4**, which Follow ignores, so Trigger and Pump give a
musical dip as soon as they are selected (at 120 BPM: 375 ms, of which 10 ms fall, 150 ms hold and
215 ms recovery). With `sync` Off the default times would make a 560 ms dip, longer than a beat.

### Expert

| id | Name | Type | Range | Default | Curve | Smooth | EXP | Role |
|---|---|---|---|---|---|---|---|---|
| `hpf` | Key Low Cut | f | 0 – 2,000 Hz | 0 (off) | log | 20 | | Butterworth high-pass on the key. Also sizes the sliding-max window |
| `lpf` | Key High Cut | f | 500 – 22,000 Hz | 22,000 Hz (off) | log | 20 | | Butterworth low-pass on the key |
| `knee` | Knee | f | 0 – 24 dB | 6 dB | lin | 20 | | Key range over which the duck grows from 0 to `depth`. 6 dB = gfx-delay / gfx-reverb; 24 dB feels like a sidechain compressor; 0 = a switch |
| `hyst` | Hysteresis | f | −24 – 0 dB | −3 dB | lin | 20 | | Release threshold = T + `hyst` |
| `detect` | Detection | e | Absolute · Relative | Absolute | — | — | | Relative = threshold measured above the key's own floor (live mics with bleed) |
| `margin` | Margin | f | 0 – 40 dB | 10 dB | lin | 20 | | Relative only: how far above its floor the key must rise |
| `freq` | Band Frequency | f | 40 – 12,000 Hz | 1,000 Hz | log | EXP rule (in log f) | ✓ | Bell centre; Low / High: where half the duck applies |
| `width` | Band Width | f | 0.3 – 4 oct | 2 oct | lin | 20 | | Bell bandwidth |
| `spread` | Spectral Spread | f | 3 – 30 dB | 12 dB | lin | 20 | | Spectral: how far below the key's loudest band a band still ducks |
| `sync` | Sync | e | shared list: Off · 4/1 · 2/1 · 1/1 · 1/2 · 1/2• · 1/2T · 1/4 · 1/4• · 1/4T · 1/8 · 1/8• · 1/8T · 1/16 · 1/16• · 1/16T · 1/32 | 1/4 | — | — | | Trigger: the dip's length as a note value. Pump: the period, and the dip's length inside it. Off = the ms times (Pump then free-runs, off the beat). Ignored in Follow |
| `length` | Dip Length | f | 10 – 100 % | 75 % | lin | 20 | | Share of the note value the dip lasts (`sync` on) |
| `tension` | Recovery Tension | f | −100 – +100 % | 0 % | lin | 20 | | Recovery shape (§2.4): + returns late (club pump), − snaps back. Named `tension`, not `curve`: gfx-swell's `curve` is an enum of fade laws |
| `offset` | Beat Offset | f | 0 – 100 % | 0 % | lin | — | | Pump: shifts the dip within the period to sit on the kick |
| `sens` | Trigger Sensitivity | f | 0 – 100 % | 50 % | lin | — | | Trigger: the rise an onset needs, 12 → 3 dB (gfx-swell's detector A; same direction as gfx-killswitch) |
| `stereo` | Stereo | e | Both · Mid · Side | Both | — | crossfade 20 ms | | Stereo layout only |
| `lookahead` | Lookahead | e | Off · 2 ms · 5 ms · 10 ms | Off | — | 20 (crossfade) | | Delays the main, reported to the host. Setup setting: never a scene value, never automated |
| `force` | Duck Now | b | off / on | off | — | — | | Momentary footswitch: ducks fully while held (talkback, breakdown) |
| `listen` | Listen | b | off / on | off | — | 5 (crossfade) | | Outputs the filtered key |
| `bypass` | Bypass | b | off / on | off | — | 10 (crossfade) | | VST3 bypass |

Read-only: `gr` (Duck, 0 – 60 dB) and `state` (Ducking, 0 / 1).

Expert groups (`visuals/data/groups.json`):
- **Key**: `hpf`, `lpf`, `knee`, `hyst`, `detect`, `margin`, `listen`;
- **Band**: `freq`, `width`, `spread`, `stereo`;
- **Rhythm**: `sync`, `length`, `tension`, `offset`, `sens`;
- **Setup**: `lookahead`, `force`.

**To check on the bench (contract):** `thresh` and `hyst` have negative minimums; `tension` has a bipolar
range. Check the plain ↔ normalized writes in REAPER and Ardour.

**Setup and momentary parameters** (for golem's catalogue): `lookahead` is set at insert or by a preset
only; `force` is a momentary foot action and is never stored on in a preset or a scene; `listen` is a
setting aid, off in every preset.

## 4. Applications

Here "key" means the routing golem gives the block (§6). Only the lane's own DI exists today: of the
uses below, the tail duck, the talkback button and the three Pump rows work now; every other row waits
for §6, items 2 and 3.

| Use | Settings to start from | Feature that matters |
|---|---|---|
| Guitar pad / ambient lane under the singer | key = voice lane DI; Follow, `hpf` 150, `lpf` 4,000, `detect` Relative (Absolute with `thresh` set at soundcheck until Relative is proven), `margin` 10 dB, `depth` 9 dB, `attack` 15, `hold` 250, `release` 600 | Relative floor: the mic's drum bleed does not duck the pad; hold bridges the words |
| Same, keeping the pad's body | as above, `shape` Bell, `freq` 1,500 Hz, `width` 2 oct, `depth` 8 dB | Only the vocal band moves: the pad stays as loud, and the voice cuts through |
| Keys / synth bed under a lead vocal | `shape` Spectral, `depth` 8 dB, `spread` 12 dB | Ducks where this voice's formants are, moment by moment |
| Backing tracks under a band mate's solo | key = soloist's lane output; Spectral, `stereo` Mid, `depth` 6 dB, `release` 800 | Room for the solo in the centre; the backing keeps its width |
| Talkover between songs (DJ "Advanced") | key = talk mic; Follow, `shape` Bell, `freq` 1,200 Hz, `width` 3 oct, `depth` 18 dB, `hold` 600, `release` 1,000 | Mid-only talkover, Pioneer's default behaviour |
| Talkover, full band ("Normal") | as above, `shape` Wide | |
| Talkback button | `force` on a momentary footswitch, `depth` 18 dB, `release` 800 | Duck now, without a key |
| Delay / reverb / looper / glitch tail out of the playing | on a **wet-only branch** (effect at kill-dry), after the effect; key = lane DI (golem's default, works today); `thresh` −35, `knee` 6, `attack` 5, `hold` 0, `release` 300, `depth` 12 | Self-ducking for any effect that has no ducking of its own (NAM, third-party, glitch, granular, looper) |
| Kick ducks the bass | key = drums / kick source; Trigger, `sync` Off, `hpf` 0, `lpf` 150, `sens` 50, `attack` 2, `hold` 20, `release` 120, `shape` Low, `freq` 120 Hz, `depth` 12 dB | Level-independent dips; mostly the sub ducks (6 dB at 120 Hz, 1 dB at 240 Hz), the bass's growl stays |
| Same, from a kick mic with bass bleed | as above, `hpf` 2,000, `lpf` 22,000 (the beater's click), `thresh` just under the kick's level | The bass hardly reaches the detector |
| Same, tempo-locked recovery | Trigger, `sync` 1/8, `length` 70 %, `tension` +30 | Recovery follows the song's tempo |
| Club pump on a pad / clean chords | Pump, `sync` 1/4, `attack` 5, `hold` 0, `length` 80 %, `tension` +40, `depth` 18 dB | Sidechain feel without a kick |
| Low-only pump (keeps the top bright) | Pump, `shape` Low, `freq` 200 Hz, `depth` 24 dB | Kickstart-style band split |
| Half-time breathing | Pump, `sync` 1/2, `length` 90 %, `tension` −30, `depth` 6 dB | |
| Deep, click-free dips in a studio pass | any mode, `lookahead` 5 ms, `attack` 5 | The fall finishes as the onset arrives |
| Setting the key filters | `listen` on | Hear what the detector hears |

**Preset ideas** (`presets/gfx-ducker.json`, ids permanent once shipped): `pad-under-voice`,
`voice-band-carve`, `spectral-vocal-space`, `solo-spotlight`, `talkover-mid`, `talkover-full`,
`talkback`, `tail-duck-delay`, `tail-duck-reverb`, `kick-bass-sub`, `kick-bass-synced`, `club-pump-quarter`,
`low-pump`, `halftime-breath`, `studio-lookahead`. No artist or trademark names.

## 5. Bench suite (on top of gfx-bench's common checks, run with a key)

| Check | Signal | Pass |
|---|---|---|
| Transparency | any shape and `stereo`, key silent or absent; and after a duck has fully released | bit-identical output (delayed by L with `lookahead`) |
| Depth | key a sine 20 dB above T, `knee` 6 | steady duck = `depth` ± 0.1 dB, Wide |
| Static curve | slow key ramp, `knee` 0 / 6 / 24 | duck_dB = depth · S(clamp((L − T)/knee)) ± 0.2 dB |
| Times | key step on / off | full depth after `attack` ± 1 sample; no rise during `hold`; back to 0 dB after `release` ± 1 sample |
| Hysteresis | key ramp up then down, `knee` 0 and 6 | starts at T; on the way down the whole curve sits `hyst` lower; closes below T + `hyst` ± 0.1 dB |
| No flutter | key = 41 Hz sine at T + 3 dB, `hpf` 0 | gain ripple ≤ 0.1 dB (sliding max) |
| Edges | main = 80 Hz sine at 0 dBFS, key bursts at every phase, Wide: `attack` 0.5 / 2 / 10 ms × `depth` 12 / 60 dB | energy added above 10 kHz and above 2 kHz within 3 dB of §2.3's table (the edge is the designed one, no extra step); the common −50 dB bound above 10 kHz holds from 1 ms at 60 dB and from 0.5 ms at 18 dB |
| Recovery corners | same sine, Trigger, `depth` 60, recovery 30 ms, `tension` −100 / 0 / +100 | ≤ −60 dB above 10 kHz (prototype: −65 / −77 / −65) |
| Band-shape stability | Bell, Low, High, Spectral at `depth` 60, `attack` 0.5 ms, pink noise and an 80 Hz sine | output peak ≤ input peak + 0.5 dB during the edges; no NaN, no ringing after the release |
| Relative floor | key = **drum-like bleed** (decaying noise bursts every 250 ms peaking −38 dBFS over a −62 dBFS room), sung words with 120 ms tails at −18 dBFS, then the bleed raised by 8 dB; `thresh` −45, `margin` 10, `hold` 250 | bleed alone: 0 % ducked once learned (after 4 s), ≤ 25 % in the 4 s after the rise, then 0 %; 100 % during the phrases; F never within `margin` of the voice; the same test with key and `thresh` moved by ±15 dB ducks the same within 10 % of the times. Then the same on a **recorded stage vocal mic** (constants tuned there; Relative does not ship before) |
| Words | voice/male.wav as key, `hold` 250 | no release between words of a phrase |
| Bell / shelves | at full duck: response vs the matched analog design | ± 0.5 dB to 0.45 fs |
| Spectral, sum | static targets through the solver: uniform, one band, a triangle over 9 bands, two blocks; `depth` 6 / 12 / 18 | deepest point of the measured response = `depth` ± 0.5 dB; active centres ± 1 dB; uniform ripple ≤ 1 / 1.5 / 2 dB |
| Spectral, key | key = sine at 905 Hz (a centre) and at 1,076 Hz (between two), `spread` 12, `depth` 12 | deepest cut at the key's frequency ± a quarter octave; ≤ 2 dB at one octave, ≤ 0.5 dB at two; with a pink-noise key the response is the uniform one |
| Spectral, motion | key = voice/female.wav, main = pink noise, `depth` 18 | no band's cut moves faster than the activation ramps allow; no click by the common bound |
| Stereo Mid | main L ≠ R, `stereo` Mid | S unchanged (null ≤ −100 dB) |
| Trigger | kick loop as key, 120 BPM; one long 808; kicks with three bass notes bleeding 14 dB under them; with `lpf` 150 and with no key filter | exactly one dip per kick (the prototype's counts: 4, 1, 4), starting ≤ 3 ms after the onset; equal depth for hits ±12 dB; 16th-note kicks at 180 BPM (83 ms apart) all fire |
| Pump sync | host 120 BPM, playing from bar 3.2; loop; seek | dip on every beat ± 1 sample; aligned after loop and seek. With `lookahead` 5 ms: measured **in REAPER and in the Golem Engine** after their compensation (it fixes the sign of §2.5's offset) |
| Latency | `lookahead` Off / 2 / 5 / 10 | reported = measured; Off = 0 |
| EXP | `depth` driven by 50 Hz-stepped 7-bit values | sidebands ≤ −80 dB (contract) |
| Determinism | two renders | strict null |
| CPU | per shape, mono / stereo, 48 kHz, while ducking (keyed) | Wide ≤ 50 ns, Bell ≤ 90 ns, Spectral ≤ 300 ns mono and ≤ 400 ns stereo per sample |
| Listening | clean-pad.wav ducked by voice/female.wav (to record, with drum bleed); bass DI ducked by a kick loop; pad in Pump; delay wet branch keyed from the DI | renders kept for Raph |

## 6. What golem-rack must provide

**What exists** (read in `~/golem/docs/golem-fx-sheet.md`, "The key of a keyed gate is the lane's DI"):
golem routes one key, **the lane's own DI**, to a block flagged `key: true` in the engine table, anywhere
in the lane, on both engines.
- REAPER (`golem_rig.lua`): the track gets 4 channels, 3/4 being the key. `golem_keytap.jsfx` copies the
  input onto 3/4 on the lane's input track; on a branch or the merge, a **pre-FX send** from the input
  track lands on 3/4.
- Golem Engine (`golem_watch.lua`): the plan gives the block `key_input` (the card input of its path);
  the watcher adds the insert's sidechain (`Route:add_sidechain`) and connects it.
- Nothing else: no key from another lane, none from a service track, and no effect chain on the service
  tracks (Backing, Drums, Sample, Looper, Click are buses with a volume in `docs/mixer.md`).

That covers self-ducking on a wet branch now. The rest is to build, in this order of value:

1. **`key: true` for `ducker`** in the engine table, with the block's key defaulting to the lane's DI.
   Enough for the tail duck. On Windows and macOS, where golem routes no key, the block passes the
   signal through: golem should say so on the block instead of showing a ducker that does nothing.
2. **A key source per block**: a `key_from` field in the preset's block entry, next to its values.
   - `self` (default): the lane's DI, as today;
   - `lane:<id>` + `tap` `di` | `out`: another lane's input, or its output after its chain;
   - `service:<name>`: `backing`, `drums`, `sample`, `looper` (the bus's output). `click` is not offered:
     it must never leak into a room mix through a key by mistake, and Pump does that job.

   REAPER: a send from the source track onto 3/4 of the block's track, pre-FX for `di`, post-FX
   (pre-fader) for `out`, the mechanism already used for branches; the send is part of the FX signature.
   Golem Engine: connect the insert's sidechain to the source route's input port (`di`) or output port
   (`out`, a port-to-port connection). A source that does not exist in the current preset = no key =
   no duck.
3. **Effect slots on service buses**, at least one `ducker` slot on Backing and on Drums / Sample.
   Without it "the singer ducks the backing" and "the kick ducks the bass from the drum machine's other
   side" cannot be placed: today a block lives in an instrument lane only. The Backing bus is stereo:
   the block's 2 / 2 layout with `stereo` Mid is the intended use.
4. **Key timing.** The key must not reach the plugin later than the main. Per source, golem-rack states
   the offset between the key and the main at the block's input:
   - a `di` key against a block placed after latent blocks (NAM, cab, a `lookahead`) arrives **early**
     by the lane's latency when the host does not compensate sends; that is harmless lookahead up to a
     few ms, and wrong beyond (the duck starts before the word);
   - an `out` key from a lane with more latency than the ducked one arrives **late**;
   - REAPER compensates sends between tracks; the Golem Engine's sidechain port connection is to be
     measured. Target: |offset| ≤ 1 ms, checked with a click through both paths.
5. **Transport.** With a backing, REAPER's transport plays and its position is the song time
   (`docs/arrangement.md`); the Golem Engine keeps its transport running (`docs/ardour-engine.md`). To
   confirm: that the plugin sees tempo and position on both, and what happens **without** a backing
   (a song on the click alone, or free playing). If the transport is stopped there, Pump free-runs at
   the tempo from engage and the downbeat is not locked; a "restart Pump on this press" foot action
   would then be the only way to place it.
6. **Latency changes at preset level** (`lookahead`): the same question as gfx-limiter, gfx-swell and
   gfx-glitch `align`: both engines must re-sync cleanly when a preset load changes it.
7. Catalogue entry `ducker` (dynamics, offered to every lane role and to the service slots); a
   **momentary** foot action for `force` (press = on, release = off, not a toggle; golem's `foot_mode`
   knows `transport`, `sample` and `drums` gestures today); EXP on `depth` and `freq`.

### Visual

- **Category `dynamics`**, family slate `#4a5a6e`, **Thrash** finish (`FAMILY_FINISH.dynamics`). No crate
  hue: gate, comp, limiter and de-esser all share the slate, and the motif tells them apart. Keeping the
  family hue says "dynamics" on the board.
- **Motif `dips`** (new case in `motifParts`), on the geometry of gfx-swell's `swells` and gfx-transient's
  `spikes`, so the three envelope motifs of the family sit in the same band, below the knob grid and
  the cartouche, and differ by their figure: swells rise from the baseline, spikes stand on it, **dips
  hang from a top line**.
  - A faint baseline at y0 = H − 40 across the width (`line`, opacity .4).
  - A level line at yTop = cy + ch / 2 + 8. In each of four slots of width W / 4 (four beats), with
    w = W / 4 − 4: the line runs flat for 0.12 w, drops almost vertically (1.5 px wide) to y0 − 10, then
    climbs back along a quadratic `Q x0+.45w y0−10 x0+.9w yTop` and runs flat to the slot's end. This is
    the fast duck and the curved recovery of a pump.
  - Under each drop, on the baseline, three short ticks (heights 3, 6, 4; opacity .35, clear of the dip): the key's burst
    that caused it, the same language as the swell's removed pick.
  - The dips get shallower from left to right by 2 px each, so the row is not a repeated stamp.
  - All `line`: yellow lined in black on thrash, engraved on anodized (if forced).
- **Thrash is kept**: no `CRATE_FINISH` entry. gfx-swell left thrash because it removes attacks; a ducker
  pushes a signal down on each hit, which the family's finish says well enough, and gate, comp,
  limiter, de-esser, gfx-transient and gfx-mbcomp (`tiers`) all stay thrash on the slate.
- **Face** (`visuals/data/faces.json`):

  ```json
  "gfx-ducker": {"name": "Duck", "blocks": ["ducker"], "knobs": ["thresh", "depth", "hold", "release"],
                 "selectors": ["mode", "shape"], "toggles": ["force"], "category": "dynamics"}
  ```
- **Glyph** `visuals/glyphs/ducker.svg`: a level line that drops at once and recovers along a curve, over
  three faint key ticks:

  ```svg
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><title>Ducker</title><path d='M2.5 6.5H8.5V14.5C12.5 14.5 15 6.5 21.5 6.5'/><path d='M7 21.5V20M9.5 21.5V18.5M12 21.5V19.5' opacity='.5'/></svg>
  ```
  The ticks were shortened and the dip raised by half a pixel: at stroke 1.8 the middle tick touched the
  curve. It reads as "a level pushed down by something below, then let back up", distinct from `gate`
  (square step), `swell` (rise to a plateau) and `tremolo` (wave under an envelope).
- No maker's livery, logo or trademark on the pedal.

## Open questions for review

All settled at the review of 2026-11-14, except what needs ears, a recording or golem-rack (listed at
the end of the Review section).

1. **`depth` as a positive dB amount.** **Yes, `depth`, 0 – 60 dB, positive.** A reduction is a positive
   amount everywhere in the suite except one place: `duck` in gfx-delay and gfx-reverb, `range` in
   gfx-comp (0 – 60 dB) and gfx-deesser (0 – 24 dB), and the `gr` meters. gfx-gate's negative `range` is
   the catalogue's floor gain, kept for its presets. `range` was not taken here because in gfx-comp and
   gfx-deesser it is a **limit** on a computed reduction, and here the amount is the setting itself.
   `depth` is not a contract shared id (it is ms in gfx-chorus, % in gfx-killswitch), so dB is free.
2. **Floor tracker.** **Re-specified (§2.2), not exposed, and at risk.** The draft's rule fails on
   impulsive bleed, and so does the obvious repair; the candidate now written holds on synthetic
   signals. Its five constants are tuned on a recorded stage vocal mic, and Relative ships only if it
   passes there. One more expert time would only move the problem to the player.
3. **Spectral resolution.** **16 bands stay for v1, re-specified**: Q 2.0, the solver, 4th-order
   analysis, tables (§2.7). The ripple bound of 1 dB was not reachable as written (4.4 dB, and a sum
   60 % too deep); it is 1.3 dB at 12 dB with the solver. The range moves down one band to
   **56.6 Hz – 10.2 kHz**: a kick's fundamental (45 – 60 Hz) had no band, and above 10 kHz there is
   little of a voice or an instrument to make room for. Against Trackspacer's 32 bands: see the Review.
4. **Pump here, not in gfx-killswitch.** **Accepted.** gfx-killswitch's Ramp Up stays (its presets use
   it), and nothing moves between the pedals. Pump is the Trigger dip with the beat as its key: one
   generator, and the band shapes make it more than a tremolo (gfx-tremolo's waves are symmetric and
   full-band).
5. **`lookahead` as an enum.** **Yes**: Off · 2 · 5 · 10 ms, the rule of gfx-limiter and gfx-swell.
   gfx-deesser ships a continuous `lookahead` (0 – 10 ms, a float): each knob move changes the reported
   latency. Its id and type are permanent, so it is not turned into an enum; the proposal for the owner
   is to quantise it inside the plugin (0 / 1 / 2 / 5 / 10 ms, nearest) and to mark it setup-only in
   golem. Out of scope here.
6. **No key = no duck.** **Yes**, and simpler than written: a silent key is under any threshold, so no
   "connected or not" logic exists (§2.2). golem shows the block as keyless where it routes no key.
7. **`sync` Off in Trigger / Pump.** **Accepted, with `sync` defaulting to 1/4** (§3): Off is the
   special case (kick ducking in ms; a free-running Pump), and Trigger and Pump are musical by default.
   No `rate`.
8. **Recovery curve join.** **No rounding; the family is changed instead** (§2.4). The draft had it the
   wrong way round: the join was smooth for positive values, a corner at 0, and an infinite slope (a
   click) for negative ones. The parameter is renamed `tension`.
9. **gfx-delay / gfx-reverb / gfx-shimmer ducking.** **Confirmed in the shipped code**:
   `gfx-delay/src/lib.rs:694`, `gfx-reverb/src/lib.rs:579`, `gfx-shimmer/src/lib.rs:444` all smooth the
   linear ducking gain with `g = target + (g − target) · 0.999` once per sample, at the host rate. The
   time constant is 999.5 samples: 22.7 ms at 44.1 kHz, 20.8 ms at 48 kHz, 10.4 ms at 96 kHz, 5.2 ms at
   192 kHz. Their detector (`PeakFollower`, times in ms) is sample-rate independent; only this smoother
   is not. The effect: at 192 kHz the wet ducks and returns four times faster than at 48 kHz, so a
   preset tuned at one rate pumps differently at another, and the engines' parity null only holds when
   both run at the same rate. **Recommendation**: replace the constant with
   gfx-dsp's `one_pole_coef(0.0208 · sr)`, computed with the other per-block values (48 kHz behaviour
   kept, a two-line change per crate), now; moving them
   to `duck` is a later, audible change with its own listening. Seen on the way, same class: gfx-drive's
   Distortion slew envelope (`gfx-drive/src/lib.rs:444`, `slew_env * 0.999` per processed sample) and
   the 0.9995 DC blockers of gfx-delay (`:598`) and gfx-shimmer (`:534`). No code was changed.
10. **Trigger detector without the 1.5 kHz high-pass.** **Not robust as written; replaced** (§2.4): on a
    kick it fired 7 times for 4 hits. The detector now reads the sliding-maximum level, and a kick mic
    with bass bleed has two documented key settings.
11. **golem-side work.** **Specified in §6**, in order of value. Until items 2 and 3 exist the block is
    a tail ducker, a talkback button and a pump; that is worth shipping, and the catalogue text should
    say what the key is.

## Review (Fable 5.2, 2026-11-14)

**Verdict: Good.** The concept is right and contract-clean: one gain stage with three drivers, fixed
depth, band shapes that are bit-exact at rest, the key on its own bus, zero latency. Follow, Trigger
and Pump in Wide, Bell, Low and High with Absolute detection are **Strong** after the fixes below. Two
features are **at risk until benched**:
- **Spectral**: as written it cut 60 % too deep and cost twice its estimate (the designs alone six
  times theirs); the amended engine meets its targets on the prototype, but whether half-octave bells
  sound as good as Trackspacer is a listening question.
- **Relative detection**: the block's best idea for a stage, and the hardest. The draft's floor rule
  does not survive drum bleed; the candidate that replaces it is proven on synthetic signals only.

Prototype: `proto-gfx-ducker/` in the session scratchpad (`spectral.py`, `comp.py`, `pgs.py`,
`trig.py`, `floor.py`, `floor2.py`, `floor3.py`, `clicks.py`, and `rs/`, a Rust timing of gfx-dsp's `matched` and `svf`).

### Changes made, and why

| # | Change | Why |
|---|---|---|
| 1 | **Spectral solver** (interaction matrix, projected Gauss-Seidel, cut-only) and bells widened to **Q 2.0** | Bells in series add up: 16 bells at a nominal 12 dB gave 18.7 – 20.1 dB with 4.4 dB of ripple, 30 – 32 dB for 18 [P]. `depth` was not the depth, and the bench's 1 dB ripple could not pass. Same finding as gfx-geq's review; here a cut-only solve is enough (max of the sum within 0.2 dB of `depth`, ripple 1.3 dB at 12) |
| 2 | **Coefficient tables** for the 16 fixed bells; CPU table redone from measurements | A matched design measures 925 ns, not ≈ 150: 16 per tick = 460 ns/sample, Spectral ≈ 610 ns mono against "≈ 300". With tables ≈ 220 ns |
| 3 | **4th-order analysis bands** | With 6 dB/oct skirts a pure tone read −7 and −13 dB in the next two bands: the weights could not resolve what the 16 bells could cut |
| 4 | Bank moved to **56.6 Hz – 10.2 kHz**; control tick defined in time (1 / 1,500 s); band count declared internal | A kick key had no band under 80 Hz. 32 samples is 0.67 ms at 48 kHz and 0.17 ms at 192 kHz (gfx-geq's review lesson) |
| 5 | **Relative floor re-specified** as a candidate: peaks of the quiet-time level, a 150 ms guard after anything above T, short excursions learned as hits, 0.5 dB/s during singing; start at `thresh` − `margin`; marked at risk | "Falls at once to any lower level" put the floor in the gaps between drum hits: the threshold sat 14 dB under the snare bleed and every hit ducked (29 % of the time on the prototype, all of it with hold). Following every level under T fails too (word tails ratchet the floor up to the voice). The "catches up within 3 s" claim was false for a rise above `margin`; the limits are now stated |
| 6 | **Trigger detector** reads the sliding-maximum level (slow reference in dB, slope test) | gfx-killswitch's 5 ms fast release is made for its 1.5 kHz high-pass. On a kick body it re-armed inside the tail: 7 triggers for 4 kicks, 2 for one 808 [P]; the new one gives 4 and 1, and ignores bass bleed 14 dB down |
| 7 | **Recovery family** mirrored for negative values; attack + hold limited to half the dip; `curve` renamed **`tension`** | u^p with p < 1 has an infinite slope at the join: −27 dB of click above 10 kHz on an 80 Hz sine at −100 [P], against −65 dB mirrored. The text had the smooth and broken cases reversed. gfx-swell's `curve` is an enum of fade laws: one id, two types, in one family |
| 8 | "Click-free by construction" replaced by a **measured edge table**; band shapes clamp the attack to two ticks; bench bounds taken from the table | A 0.5 ms, 60 dB edge on an 80 Hz sine adds −15 dB above 2 kHz and −43 dB above 10 kHz: the bench's own click bound (−50 dB) failed on the design's flagship test. Coefficients move per tick, so a band shape cannot make a 0.5 ms raised cosine |
| 9 | **Hysteresis with a knee** defined (the curve shifts by `hyst` on the way down) | "Keeps a* from dropping back to 0" said nothing about a* between T + `hyst` and T + `knee` |
| 10 | **No-key logic removed**: silence is under the threshold | The "silent since reset" detector was not needed, and gfx-gate's 1 s rule would have been copied for nothing |
| 11 | **`sync` defaults to 1/4**; `hold` and `sync` roles per mode written in the table | With `sync` Off the defaults made a 560 ms dip, longer than a beat at 120 BPM: Trigger and Pump never recovered out of the box. A mode-dependent default is forbidden; a default Follow ignores is not |
| 12 | **Mid / Side as a difference** (L' = L − (M − M')); bypass keeps the lookahead delay; lookahead change crossfaded; `freq` on the EXP smoother | Encode / decode is not bit-exact at 0 dB, and the transparency check would have failed in Mid. A bypass that drops the delay changes the latency. `freq` was marked EXP with a plain 20 ms smoother |
| 13 | **Pump phase with lookahead**: position − L, flagged [U], measured on both engines | "Position + latency" has the wrong sign under the usual host convention, and the convention itself differs between hosts |
| 14 | **`onset`: one module** (gfx-swell's), with the interface the ducker needs; key support placed in gfx-dsp's `Effect` trait | The draft moved gfx-killswitch's detector into a module of the same name as the one gfx-swell's reviewed design creates. gfx-transient does not use `onset` at all. The `Effect` trait lives in gfx-dsp, not gfx-bench |
| 15 | **§6 rewritten** from golem's documents: what exists, a `key_from` field, service-bus slots, timing target, transport facts, momentary `force` | The draft's list was right in spirit; it lacked what exists today (only `key: true` → lane DI), the order of value, and testable targets |
| 16 | **Visual**: `dips` redrawn on the `swells` / `spikes` geometry; thrash kept and argued; glyph ticks shortened | Three envelope motifs are arriving in one family (swells, spikes, dips): the same band and slots, a different figure, so they read as a set. The first `dips` used its own geometry (a line at cy − 0.6 ch, 18-unit periods either side of the rune). At stroke 1.8 the glyph's middle tick touched the curve |
| 17 | **Facts**: Trackspacer's row limited to what its page says (no key listen there; method and latency unpublished, "FFT or long filters" removed); Devious Machines Duck 2 resolved; gfx-shimmer's ducking added; the "reproduces gfx-delay's law" claim reduced to the static curve within 10 %; shelf `freq` defined; time constants at four sample rates | Errors or gaps against the makers' pages and our own code |
| 18 | **Bench**: Edges, Recovery corners, Band-shape stability, three Spectral checks, a drum-bleed Relative check, Trigger negatives, CPU bounds from measurements | The old checks tested the claims, not the failure modes found here |

Checked and left as written: fixed depth instead of a ratio; the raised-cosine map in dB and linear
activation ramps; hold; the sliding-maximum level (gfx-gate's window rule); Bell Q from `width`
(0.67 at 2 octaves, 1.41 at 1); the matched bell rather than a fixed-pole parallel one; mono first with
the key on an auxiliary bus; `force`; `listen`; linear ranges; no mode-dependent default; the survey's
other rows (Pro-C 2, Kickstart 2, VolumeShaper, LFOTool, Pioneer talkover levels, Audacity Auto Duck's
defaults).

### Is Spectral competitive with Trackspacer?

- **Where it is ahead**: latency is zero and stated (Trackspacer publishes none); `depth` is a real
  number of dB that the sum respects; the band is bit-transparent at rest; hold comes with it, so it
  does not pump between words; Mid-only is there.
- **Where it is behind**: half-octave resolution against about a third of an octave (32 bands over the
  audio range [A]; Trackspacer's filters are not published), and second-order bells whose skirts take
  4 dB from the next band. It follows a voice's formant regions, not its harmonics. That is the right
  grain for "make room" and the wrong tool for surgical unmasking; soothe-like behaviour stays out of
  scope at zero latency.
- **Decision rule**: bench listening on a pad under a voice and a bass under a kick, level-matched
  against a third-octave dynamic EQ. If 16 bands sound coarse, go to 24 bands a third of an octave
  apart (same code, ≈ +50 % CPU, about 330 ns mono), before any shipped preset uses Spectral.

### Still open

| Point | Owner |
|---|---|
| Fix the 0.999-per-sample ducking smoother in gfx-delay, gfx-reverb, gfx-shimmer (question 9): two lines each, 48 kHz behaviour kept; and look at gfx-drive's slew envelope | Raph decides; golem-fx |
| Relative: the candidate floor rule and its five constants on a **recorded stage vocal mic** with drum bleed (none exists in the bench's DIs); ship Relative in v1 or after | Raph (recording, decision), golem-fx |
| Spectral: 16 or 24 bands, by listening; `spread` default; solver sweeps per tick | golem-fx bench, Raph's ears |
| `tension` (renamed by the review) and `sync` default 1/4: keep or revert | Raph |
| gfx-deesser's continuous `lookahead`: quantise inside the plugin, setup-only in golem | Raph, golem-rack |
| Who writes `onset::Pick` first (gfx-swell or gfx-ducker), and gfx-killswitch's move to it | golem-fx |
| `key_from` and its taps, service-bus slots, key timing per engine, transport without a backing, momentary foot action, preset-level latency change, the keyless display on Windows / macOS (§6) | golem-rack |
| Sign of the Pump phase offset with `lookahead` on each engine | golem-fx bench with golem-rack |
| Catalogue entry `ducker`, glyph, face, `dips` in `motifParts`, block presets | golem-rack / visuals, once coded |

### Decisions after the review (Raph, 2026-11-14)

- **`detect` Relative ships only after it passes the bench on a recorded stage vocal mic** (drums and
  backline bleed, sung-word tails). Until then the option stays out of v1's enum; Absolute is the only
  detector shipped. The enum is append-only, so adding Relative later costs nothing.
- **The ducking smoothers of gfx-delay, gfx-reverb and gfx-shimmer are fixed** (0.999 per sample at 48 kHz,
  now the same time constant at every rate), with gfx-drive's slew envelope and the 0.9995 loop DC blockers.
- **Sidechain window under `hpf`**: the gfx-transient review halved the window rule (W = 1.2 / max(hpf / 2,
  30 Hz)) because a note under a 12 dB/oct corner is only 7–10 dB down. gfx-gate was probed on a decaying
  82 / 41 / 62 Hz note under `hpf` 100–200 Hz and does not chatter (new test
  `no_chatter_on_low_notes_under_the_sidechain_highpass`); for this block, whose gain follows the level
  continuously, use gfx-transient's halved rule for the Follow detector and bench its ripple with a key note
  under `hpf`.
