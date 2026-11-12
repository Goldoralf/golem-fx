# gfx-gapmeter

> **Status: PROPOSED** — design, no code yet. Contract agreed with golem's rack session on 2026-11-12
> (it owns `tools/check-scene-gap.py`).

A measurement probe, not an effect. It replaces golem's `golem_gapmeter.jsfx` so that
`tools/check-scene-gap.py` works on the Golem Engine (Ardour) as well as on REAPER: it detects every
silence longer than a minimum in the audio it sees, sample-accurate, and reports when and how long.

## 1. What the JSFX does today

From `~/golem/reaper/Effects/golem_gapmeter.jsfx`:

- Passes audio through unchanged.
- Peak envelope with a 3 ms release (a 100 Hz tone never dips below the threshold between peaks).
- A silence starts when the envelope falls under `Silence threshold` (−45 dB); it counts as a gap if it
  lasts at least `Min gap` (10 ms).
- Results in `gmem` "golem_gapmeter", one 64-slot block per instance (`Slot` 0 – 7): sample clock, current
  envelope, gap count, longest gap, silence in progress, ring of the last 24 gap durations and end times.
- A separate REAPER Tone Generator, placed by the script, provides the continuous test tone.

## 2. The problem: no `gmem` outside REAPER

A VST3 has no shared memory with golem. Options for getting the results out:

| Option | How | For | Against |
|---|---|---|---|
| **A. UDP to the bridge** (recommended) | The audio thread writes events into a lock-free ring; a background thread sends them as OSC / UDP datagrams to `127.0.0.1:<port>` | Same on REAPER and Ardour; golem's bridge already speaks OSC; events carry sample-accurate times | A port to configure; nothing on the audio thread may block (the background thread does the I/O) |
| B. VST3 output parameters | Read-only parameters (gap count, longest gap…) read by the host | Standard VST3 | nih-plug does not expose output parameters; a ring of 24 events does not fit parameters |
| C. A file | The background thread writes a JSON file | Simple | Polling, stale data, disk I/O during a measurement |

## 3. Design (option A, agreed)

Two roles, chosen per instance:

- **Detector** (default): audio passes through unchanged; gaps are detected on the max of the channels.
  Instances sit at the end of the merge track and of each branch.
- **Generator** (`tone` on): the output is the test tone, **replacing** the input (the plugged-in guitar
  must not mix with it). One instance sits at the end of a lane's input track.

| id | Name | Type | Range | Default | Role |
|---|---|---|---|---|---|
| `slot` | Slot | e | 0 – 7 | 0 | Identifies the probe in its messages |
| `thresh` | Silence Threshold | f | −90 – 0 dB | −45 dB | Envelope level under which the signal counts as silent |
| `min_gap` | Min Gap | f | 1 – 500 ms | 10 ms | Shorter silences are ignored |
| `port` | Port | f | 1024 – 65535 | 9071 | UDP port; `check-scene-gap.py` binds it (never 9000, the bridge's REAPER feedback port, nor 3819, Ardour's surface) |
| `tone` | Test Tone | b | off / on | off | Generator mode: output = tone, input discarded |
| `tone_freq` | Tone Freq | f | 50 – 2,000 Hz | 196 Hz | |
| `tone_level` | Tone Level | f | −60 – 0 dB | −20 dB | |

Detection, as in the JSFX: peak envelope with a 3 ms release, max of the channels; a gap is a silence of
at least `min_gap`. Clock = samples processed since activation / sample rate.

### Messages (OSC over UDP to 127.0.0.1:`port`)

| Address | Arguments | When |
|---|---|---|
| `/gfx/gap` | `slot` `k` `end_s` `duration_ms` | when a gap ends; `k` = the gap's index (0, 1, 2…), so a dropped packet is detectable |
| `/gfx/gapmeter` | `slot` `clock_s` `env_db` `silence_ms` `n` `max_ms` | every ~50 ms |

- The audio thread only publishes: the latest clock, envelope and silence in atomics, gap events in a
  lock-free ring. No allocation, no I/O.
- A background thread sends. The heartbeat keeps coming with the **latest published clock**: if the audio
  thread stalls, the clock freezes while the heartbeats continue. That is how `check-scene-gap.py`
  measures a stall (wall time elapsed − sample clock elapsed).
- Nothing is sent before activation.

### Layouts

1 / 1 and 2 / 2; golem picks at insert time. Do not assume one instance per track: if a host ever runs a
mono plugin as dual mono, two instances would publish the same slot (golem inserts the matching layout
to avoid it).
