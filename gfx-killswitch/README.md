# gfx-killswitch

> **Status: PROPOSED** — design, no code yet.

golem's rhythmic kill switch / stutter / slicer. Stands for golem's `killswitch` block (today golem's own
`golem_killswitch.jsfx`), keeping its parameter ids and units ([golem contract](../docs/golem-contract.md)).
The aim: the Buckethead / Tom Morello kill switch, a tight trance gate locked to the song, and a tremolo,
in one zero-latency block that never clicks unless asked to.

## 1. What the JSFX does today

From `~/golem/reaper/Effects/golem_killswitch.jsfx`:

- A phase per step, open for `length`, then cut by `depth`; the gain follows a linear, slew-limited ramp
  of `smooth` ms.
- Triggers: **Engage** (inferred from a gap of more than 150 ms in `time_precise()`), **Pick** (peak hold
  35 ms, release 30 ms, compared with an 80 ms average; `sens` sets a rise of 12 … 2 dB and a floor of
  −30 … −60 dBFS; 50 ms refractory), **Beat** (phase from `beat_position`, once per block).
- Traps it respects: no negative minimum, append-only parameters, witness counters.

Its defect: the ramp is slew-limited, so when `smooth` is longer than a segment the gain never reaches its
target. At 1/32 and 200 BPM a step is 9.4 ms: with `smooth` 20 ms the "kill switch" becomes a shallow
wobble.

## 2. State of the art

Survey of 2026-11-12. **[V]** read in the source; **[I]** inference.

| Reference | What it does best | What we take |
|---|---|---|
| Hardware kill switch [I] | A hard cut, the "clack" | `smooth` 0 = a true hard cut, always available |
| [Boss SL-2 Slicer](https://in.boss.info/products/sl-2/) [V] | 8 pattern types × 11 variations, stereo ping-pong | Step patterns |
| [Soundtoys Tremolator](https://www.soundtoys.com/wp-content/uploads/Tremolator-Manual.pdf) [V] | Groove (swing / shuffle), Accent, custom rhythms where each event triggers one cycle, editable shapes | Swing, accents, offset |
| [Cableguys ShaperBox / VolumeShaper](https://www.cableguys.com/support/audio-automation-midi-and-timing) [V] | **Audio trigger** restarts the pattern on each transient; anti-click smoothing | Pick restart, clean edges |
| [Strymon Flint](https://www.strymon.net/amplifier-tremolo-technology-white-paper/) [V] | Tremolo shapes from real circuits (harmonic, power tube, photocell) | Shapes beyond the gate |
| [Bello et al., *A Tutorial on Onset Detection in Music Signals*, 2005](https://www.cs.bu.edu/fac/snyder/cs583/Literature%20and%20Resources/bello_onset_tutorial.pdf) [V], [aubio](https://aubio.org/manpages/latest/aubioonset.1.html) [V] | Onset = a detection function + peak picking; high-frequency content is efficient for percussive onsets; minimum inter-onset interval, silence gate | A zero-latency, high-passed pick detector |

### What we conclude

1. **Edges as fixed-length raised-cosine crossfades**, clamped to 45 % of the shortest segment: the gain
   always reaches its target. Fast open (≤ 1 ms, the next pick survives), close over `smooth`.
2. **Sync to the host's position**, not only its tempo: phase from `pos_beats` per sample (f64), anchored
   on the bar, so step 1 falls on the downbeat and loops / seeks stay in place.
3. **Patterns**: 16 steps, accents, swing, offset.
4. **Pick restart** on a high-passed fast / slow envelope difference: removes low-string ripple, ~1 ms
   detection.
5. **Foot control**: a momentary kill (hold to mute) and a restart, driven by golem's footswitches.

## 3. Design

- **Phase**: Beat with the transport playing → from `pos_beats` per sample, anchored on
  `bar_start_pos_beats`. Engage / Pick → a free accumulator at the host tempo (fallback 120 BPM; `rate`
  when `sync` = Free), reset on the trigger. Beat with the transport stopped → behaves as Engage.
- **Step gain**: `pattern` bit on → open for `length`; accent bit → full `depth`, otherwise `depth` × 0.6;
  bit off → closed for the whole step.
- **Swing**: each pair of steps lasts (1 + s)·T and (1 − s)·T.
- **Shapes**: Gate (square + crossfade edges), Sine, Triangle, Ramp up, Ramp down (tremolo / swells);
  depth applied in dB for the non-gate shapes, so a tremolo sounds even.
- **Pick detector**: 2nd-order high-pass (~1.5 kHz) → fast envelope (0.2 ms / 5 ms) and slow envelope
  (150 ms) → fires when fast − slow > rise(`sens`), the level is above floor(`sens`) and the detector is
  re-armed (hysteresis); 40 ms refractory. The JSFX's `sens` mapping is kept.
- **Engage**: the block switches on / off through its bypass parameter (`make_bypass`), so it sees a clean
  edge and restarts the pattern; `reset()` restarts it too.
- **Stereo**: `width` > 0 turns it into an autopan (right channel at phase + ½).
- **Zero latency.**

## 4. Parameters

Catalogue ids kept; new ones appended; no negative minimum.

| id | Name | Type | Range | Catalogue | Default | EXP | Role |
|---|---|---|---|---|---|---|---|
| `depth` | Depth | f | 0 – 100 % | same | 100 % | ✓ | |
| `length` | Length | f | 5 – 95 % | same | 50 % | ✓ | Open part of each step |
| `sync` | Sync | e | Free · 1/4 · 1/8 · 1/8T · 1/16 · 1/16T · 1/32 | same | 1/16 | | |
| `rate` | Rate | f | 1 – 30 Hz | same | 8 Hz | ✓ | When `sync` = Free |
| `trigger` | Trigger | e | Engage · Pick · Beat | same | Engage | | |
| `sens` | Sensitivity | f | 0 – 100 % | same | 50 % | | Pick detector |
| `smooth` | Smooth | f | 0 – 20 ms | same | 1 ms | | Edge crossfade, clamped to the segment; 0 = hard cut |
| `shape` | Shape | e | Gate · Sine · Triangle · Ramp Up · Ramp Down | new | Gate | | |
| `pattern` | Pattern | f (int) | 1 – 65,535 | new | 65,535 (all steps) | | 16-step bitmask, bit 0 = step 1 |
| `steps` | Steps | f (int) | 1 – 16 | new | 16 | | Pattern length |
| `accent` | Accents | f (int) | 0 – 65,535 | new | 0 | | Accent bitmask |
| `swing` | Swing | f | 0 – 75 % | new | 0 % | | |
| `offset` | Offset | f | 0 – 100 % | new | 0 % | | Grid shift, in fractions of a step |
| `kill` | Kill | b | off / on | new | off | | Momentary: hold to mute (with the edge crossfade) |
| `restart` | Restart | b | off / on | new | off | | Each off → on edge restarts the pattern |
| `width` | Width | f | 0 – 100 % | new | 0 % | | Autopan (stereo layout only) |

Read-only (diagnostics, replacing the JSFX's witness sliders): `steps_count`, `picks_count`.

**To agree with golem**: `pattern` and `accent` as integers carried by float parameters (exact up to
65,535); whether golem's UI edits a step grid.

## 5. Bench suite (on top of gfx-bench's common checks)

| Check | Pass |
|---|---|
| Edges reach their target | 1/32 at 200 BPM, `smooth` 20 ms: the gain still reaches 1 and 1 − depth on every step |
| Hard cut | `smooth` 0: a single-sample edge |
| Sync | host at 120 BPM, playing from bar 3.2: step 1 on every downbeat ± 1 sample; loop and seek stay aligned |
| Swing | step pair lengths (1 ± s)·T |
| Pick restart | funk.wav and crunch.wav: one restart per pick, none on low-string ripple, ≤ 2 ms after the onset |
| Engage | bypass → on: the pattern starts at step 1 |
| Kill | momentary mute with crossfade edges, no click |
