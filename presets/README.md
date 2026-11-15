# golem-fx factory presets

One file per crate, `presets/<crate>.json`: the **source of truth** of that pedal's factory presets. They
are exported in `visuals/data/descriptors.json` (golem-rack imports it), seeded by golem into
`library/block-presets/<type>/` as factory block presets, and read by golem.fm's patch AI, which **picks
a preset by its id** (`"preset": "<type>/<id>"`) and never writes values itself (the maintainer's decision,
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
| `params` | The pedal's own ids (descriptors.json) and values: floats within range, enums as their **labels**, bools `true` / `false`. Write the parameters the sound depends on; **the export completes every other parameter with the plugin's default**, so an exported preset is always the whole sound |
| `notes` | Optional, ≤ 200 characters: why these values (for maintainers and the AI's reasoning) |

## How golem loads them

A preset sets **every** parameter of the block (the export is complete): on load golem resets every setting
to its default, then applies the preset. It **wins over any value golem pins at insert** for that block
type (e.g. the generic `reverb`: a Spring preset plays Spring). The user then changes whatever they like
from there (the maintainer, 2026-11-13).

One exception, the type's **identity** (golem's engine table, field `identity`): a block type that exists to
be one voicing keeps it, hidden from the sheet and never written by a preset. Today: `reverb-plate` = algo
Plate, `reverb-room` = algo Room, `wah` = model GCB-95 (the other wah models are `wah-color`). golem's seed
refuses a preset whose identity setting disagrees: a preset of those types must set it to that value (or
leave it out).

golem seeds the presets into `library/block-presets/<type>/<id>.json` (`tools/sync-golem-fx-presets.py`
there; `--check` tells whether its copy is current). gfx-gapmeter's are not seeded: it is not a rack block.

## How they are chosen

Each preset answers "who uses this pedal like this, and how": a style, a band, a song, a production
habit. References are researched and sourced; a sound that is widely associated with an artist but not
documented is still useful, marked `verified: false`. A preset never imitates a trademark: names
describe the sound ("Dotted-eighth arena"), the artist stays in `refs`.

About fifteen per pedal (the maintainer, 2026-11-13): deeper banks for the big pedals (delay, reverb, drive,
chorus, shimmer, pitch…), a handful for utilities (boost, limiter, gapmeter).

## Presets in a DAW

`tools/export-vstpresets.py` turns these files into standard VST3 presets (`.vstpreset`, Steinberg's
format), one per preset, so the factory presets show in a DAW's preset browser next to the plugin:

```sh
python3 tools/export-vstpresets.py                    # all pedals -> dist/vstpresets/GoleM/<Plugin>/
python3 tools/export-vstpresets.py --crate gfx-delay  # one pedal (repeatable)
python3 tools/export-vstpresets.py --install          # also copy into this OS's user preset folder
```

Each file is named after the preset's display name and carries the whole sound: every parameter, the
preset's value or the plugin's default (loading one resets the pedal, as in golem), and the `desc` as the
preset's comment. The build fails, writing nothing, on an unknown parameter id, an enum label that is not an
option or a value out of range. Releases ship them as `golem-fx-vstpresets.zip`: unzip it into the folder
below (it holds the `GoleM` folder).

| OS | User preset folder (the VST3 SDK's "Preset Locations") |
|---|---|
| Linux | `~/.vst3/presets/GoleM/<Plugin>/` |
| Windows | `%USERPROFILE%\Documents\VST3 Presets\GoleM\<Plugin>\` |
| macOS | `~/Library/Audio/Presets/GoleM/<Plugin>/` |

`<Plugin>` is the plugin's name (`GoleM FX Delay`). Hosts that list `.vstpreset` files from these folders:
Cubase / Nuendo, Studio One, Bitwig Studio and REAPER (the FX window's preset menu). **Ableton Live does
not**: its manual documents loading only VST2 presets and banks (`.fxp` / `.fxb`, "VST Presets and Banks")
and Ableton's help notes that VST3 plug-ins show no preset list in Live. In Live, load a `.vstpreset` in
another host or in golem, or save the sound as a Live device preset (plugin in a Rack).

`tools/vstpreset-check` verifies the files against the real plugins without a GUI: it loads the built
VST3 binaries as a host does (GetPluginFactory, IComponent::setState with the `Comp` chunk) and checks that
every parameter reads back at the preset's value (its `src/main.rs` says how to run it).
