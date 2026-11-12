# gfx-gapmeter

> **Status: PROPOSED** — design, no code yet. **The contract is to be agreed with golem's rack session**
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

## 3. Proposed design (option A)

| id | Name | Type | Range | Default | Role |
|---|---|---|---|---|---|
| `slot` | Slot | e | 0 – 7 | 0 | Identifies the probe in its messages (one per lane / branch) |
| `thresh` | Silence Threshold | f | −90 – 0 dB | −45 dB | Envelope level under which the signal counts as silent |
| `min_gap` | Min Gap | f | 1 – 500 ms | 10 ms | Shorter silences are ignored |
| `port` | Port | f | 1024 – 65535 | to agree | UDP port of golem's bridge |
| `tone` | Test Tone | b | off / on | off | Adds a continuous test tone to the output (replaces REAPER's Tone Generator, absent from Ardour) |
| `tone_freq` | Tone Freq | f | 50 – 2,000 Hz | 440 Hz | |
| `tone_level` | Tone Level | f | −60 – 0 dB | −18 dB | |

Messages (OSC, to agree):

- `/gfx/gap <slot> <start_s> <duration_ms>` when a gap ends;
- `/gfx/gapmeter <slot> <clock_s> <env_db> <silence_ms>` heartbeat every ~50 ms (proves the audio thread
  runs, like the JSFX's clock).

Clock = samples processed since activation / sample rate, as in the JSFX. Same detection as the JSFX
(peak envelope, 3 ms release), so results stay comparable across the switch.

## 4. Questions for the rack session

1. Option A (UDP / OSC from a background thread) acceptable? Which port, which addresses?
2. Is the built-in test tone useful, or does golem place its own generator on the Golem Engine?
3. Anything else `check-scene-gap.py` reads today that the probe must provide?
