# golem-fx factory presets

One file per crate, `presets/<crate>.json`: the **source of truth** of that pedal's factory presets. They
are exported in `visuals/data/descriptors.json` (golem-rack imports it), seeded by golem into
`library/block-presets/<type>/` as factory block presets, and read by golem.fm's patch AI, which **picks
a preset by its id** (`"preset": "<type>/<id>"`) and never writes values itself (Raph's decision,
2026-11-13). The same preset therefore plays in the rack, in the Designer and in an AI-built patch.

Written in **English** (the product and the model are). Built and checked by
`visuals/tools/build-descriptors.py`: the build fails on any rule below.

## A preset

```json
{
  "id": "edge-dotted-eighth",
  "type": "delay",
  "name": "Dotted-eighth arena",
  "desc": "Dotted-eighth digital repeats, three or four of them, under a clean arpeggio",
  "inst": "guitar",
  "genres": ["rock"],
  "subgenres": ["post-punk"],
  "gain": "clean",
  "use": "rhythm",
  "refs": [
    {"artist": "U2", "song": "Where the Streets Have No Name", "why": "the Edge's dotted-eighth delay over picked eighths",
     "source": "Guitar Player, 1987", "verified": true}
  ],
  "level_db": 0,
  "sync": "1/8.",
  "tags": ["bright"],
  "default": false,
  "params": {"time": "1/8•", "feedback": 35, "mix": 40, "model": "Digital"},
  "notes": "Mix near 40 % keeps the repeats as loud as the pick, which is the sound"
}
```

| Field | Rule |
|---|---|
| `id` | Slug `[a-z0-9-]{2,48}`, unique in the crate, **stable forever** (the AI and saved patches refer to it). The display name may change; the id never does |
| `type` | A golem block type this crate stands for (its `blocks` in descriptors.json) |
| `name` | ≤ 40 characters |
| `desc` | One line, ≤ 120 characters: what it **does to the signal**, not a mood |
| `inst` | `guitar` · `bass` · `voice` · `keys` · `drums` · `any` (the rack's list) |
| `genres` | 1 – 3 family ids from `presets/genres.json` (golem.fm's vocabulary) |
| `subgenres` | Optional, ≤ 3 sub-genre ids of those families (same file) |
| `gain` | The scene gain class it is made for: `clean` · `crunch` · `high` · `any`. For drives and boosts this is **the amp class it is meant to hit** (golem.fm turns drives off in `high` scenes) |
| `use` | `rhythm` · `lead` · `solo` · `ambient` · `texture` · `utility` |
| `refs` | 0 – 3 references `{artist, song?, why, source?, verified}`. `why` ≤ 80 characters, one clause. `verified: true` **only with a `source`** (publication, interview, manual, rig rundown + year). Anything else is `verified: false` and reads as "in the spirit of", never as a fact |
| `level_db` | Output level relative to bypass, roughly (dB): keeps scene levels sane |
| `sync` | Only when a tempo-synced parameter is set: the note value (`1/4`, `1/8.`, `1/8T`…) |
| `tags` | Optional, ≤ 3 of: `vintage` `modern` `subtle` `extreme` `lo-fi` `wide` `dark` `bright` `warm` `aggressive` `dreamy` `tight` |
| `default` | `true` on at most one preset per (`type`, `inst`): the one a hand-placed block gets |
| `params` | The pedal's own ids (descriptors.json) and values: floats within range, enums as their **labels**, bools `true` / `false`. **A parameter left out takes the plugin's default**: write every parameter the sound depends on |
| `notes` | Optional, ≤ 200 characters: why these values (for maintainers and the AI's reasoning) |

## How they are chosen

Each preset answers "who uses this pedal like this, and how": a style, a band, a song, a production
habit. References are researched and sourced; a sound that is widely associated with an artist but not
documented is still useful, marked `verified: false`. A preset never imitates a trademark: names
describe the sound ("Dotted-eighth arena"), the artist stays in `refs`.

About fifteen per pedal (Raph, 2026-11-13): deeper banks for the big pedals (delay, reverb, drive,
chorus, shimmer, pitch…), a handful for utilities (boost, limiter, gapmeter).
