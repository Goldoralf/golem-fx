# golem pedals (visuals)

> **Status: PROTOTYPE** (2026-11-13). A gallery and the SVG renderers that draw every golem block as a pedal,
> plus golem's glyph set. Nothing is wired into golem yet.

Open [index.html](index.html) in a browser (it works from disk, no server needed). It shows:

- **a lane drawn for real**: the pedals on a stage floor or a rug, patch cables, the guitar cable coming
  in, and the amp (or a stage box when the lane has no amp) at the end. Under it, the same lane in block
  mode (golem's tiles). Clicking a pedal or a tile switches the block on or off in both views;
- **the golem-fx pedals** (29), **the NAM line** (effect captures, heads, cabs, head + cab), **the generic
  pedal** for every other block golem hosts, and **the glyph set** (79 glyphs).

## The idea: the glyph is the link

A golem is brought to life by a word on its forehead. Each block has one **glyph**, and the glyph is what
you recognise in both modes: on the block tile, and on the pedal (cast into golem-fx pedals as a rune that
lights with the pedal, on an enamel badge on generic pedals, etched on the NAM pedals' name plate).

## Three families, never confused

| Family | For | Look |
|---|---|---|
| **Metal** (`gfxPedal`) | golem-fx effects | Two finishes. By default (`'auto'`) the finish follows the family (`GolemPedals.finishFor`): **Thrash** for drive, dynamics and utility plus the killswitch, **Anodized** for the rest plus the gap meter; `GolemPedals.setStyle('anodized' \| 'thrash')` forces one for the page, `opts.style` for one pedal. **Anodized** is machined aluminium in the family's saturated colour, the motif engraved in the metal, aluminium knobs; **Thrash** is gloss lacquer with a diamond-plate panel round the switch, the motif in yellow lined in black, black knobs with a yellow pointer. Each effect carries a motif that says what it does (`GolemPedals.MOTIF`: flames for drive, echoes for delay, rays for reverb, clamp for the dynamics, barbed wire for the gate, lightning for the killswitch, fractured spectrum bins for the glitch…), and its glyph lights as a rune when on. `'clay'`, the first prototype, is still available. The high-gain companions gfx-met, gfx-emet, gfx-shem and the vocal doubler gfx-golem wear **Blackwork** (`'blackwork'`): black lacquer, white engraving, an engraved skull whose eyes light in the pedal's colour when it is on (the only colour: green, red, amber, violet), one motif each (barbed wire, flame crown, rays, voice arcs), names in Pirata One |
| **Specimen** (`namPedal`, `namHead`, `namCab`, `namStack`) | NAM captures, heads, cabs, head + cab | Brushed steel and glass for the effect captures: the captured circuit (a lattice, a valve for preamps, a needle for compressors) glows behind a window, coloured by the capture's family (overdrive, distortion, fuzz, boost, bass…). Amps in tolex, voiced by their family: clean (cream tolex, wheat cloth), crunch (oxblood and gold), high gain (black and steel), bass (navy). Cabs 1x12 · 2x12 · 4x12 · 2x10 · 8x10 from the cab's name; head + cab as a stack or a combo |
| **Enamel** (`genericPedal`) | every other golem block | A hammertone die-cast box with screws, an aged colour from the block's family, cream border, jewel lamp, chrome stomp switch; the glyph on a cream enamel badge, the name in a 70s script |

No family copies a maker's livery: amps take a period and a material, not a brand. Archetype names
(Twin, JCM800, TS…) come from golem's catalogue, where they name the captures.

## Files

| Path | What |
|---|---|
| `data/glyphs.json` | **The glyph set**: one per catalogue type (+ `gapmeter`, `glitch`: golem-fx pedals golem's catalogue does not list yet), inner SVG on a 24 × 24 grid, stroked in `currentColor` like golem's `app/icons.js`; `uni` = the former Unicode glyph, kept as a text fallback |
| `glyphs/<type>.svg` | Each glyph as a standalone SVG (generated) |
| `data/faces.json` | Each golem-fx pedal's face: knobs, selectors, toggles, the blocks it stands for |
| `data/manifest.json` | Every effect's parameters, generated from the code: `cargo run -p gfx-bench -- manifest` |
| `data/descriptors.json` | **Every pedal's complete descriptor** (generated, do not edit): plugin name and VST3 class id, the front panel (the pedal face), the expert groups with their Custom switches, and every parameter (kind, range, default, unit, display taper, step, decimals, enum labels, EXP, structural, `depends`, one-line help), plus reference presets. golem builds its block sheet from it |
| `data/groups.json` | Hand-written: each pedal's expert groups, plus the help lines and `depends` the docs leave out |
| `tools/build-descriptors.py` | Builds `descriptors.json` from the code (manifest, `plugin.rs`), `faces.json`, `groups.json` and the crates' parameter tables; fails if a parameter is missing, doubled, unknown or misnamed. `--check` verifies the committed file is current |
| `data/golem-blocks.json` | Snapshot of golem's catalogue (types, colours, first parameters, pedal / amp / cab archetypes) |
| `data/visuals-data.js` | Everything above in one script (generated), read by the gallery and by golem |
| `js/golem-pedals.js` | The renderers: pedals of the three families, amps, block tiles, glyphs. `pedalFor(type)` picks the right one |
| `js/golem-stage.js` | `GolemStage.render(chain, {floor})`: the realistic lane |
| `tools/build-data.py` | Builds `visuals-data.js` and `glyphs/`, checks that every catalogue type has a glyph and every face's parameters exist |

Rebuild after a parameter change or a catalogue change:

```sh
python3 visuals/tools/build-data.py --manifest --golem ~/golem
python3 visuals/tools/build-descriptors.py
```

## Integration in golem (later)

- **Glyphs**: golem's catalogue keeps `glyph` as a Unicode character today, with duplicates (`∿` for eq,
  tremolo and vibrato; `≈` for chorus and stretch; `△` for both boosts). The SVG set gives each type its
  own glyph. golem can load `data/glyphs.json` (or `visuals-data.js`) and draw `GolemPedals.glyph(type)`
  wherever it prints `glyph` today, keeping the Unicode character as the fallback.
- **Pedals**: the renderers are plain scripts returning SVG strings. Call `GolemPedals.defs()` once per
  page (shared gradients, textures and filters, ids prefixed `gp-`), then `GolemPedals.pedalFor({type, on,
  archetype, family, title})` for a block, or `GolemStage.render(chain, {floor})` for a lane.
- **Splits**: `GolemStage.render` takes `{split: {mode, gain, branches}}` as a chain item (each branch an
  array of items or `{id, mute, items}`). It draws a split box (one LED per branch), each branch in
  series on its own lettered strip of gaffer tape (two amps stand side by side), and a merge box with
  its gain. A muted branch is drawn off and dimmed, `B · MUTED` on its tape. A drawn block carries its
  chain path in `data-idx` (`"3"`, or `"3.1.0"` inside a branch); a tape carries `data-split` /
  `data-branch` / `data-b`, so the host can mute the branch from the scene.
- **From golem's lane**: `GolemStage.fromGolem(lane.chain, laneState)` converts golem's `{pre, split,
  post}` chain (or a flat block list) and a lane state (`blocks` on/off, `branches` mute) into
  render items. It resolves capture labels to catalogue archetypes by their `match` / `aliases`
  (`GolemPedals.archetypeFor`), so no maker's name reaches the scene. The gallery's *Metal* lane is
  golem's factory "Modern Metal" in its "Chug" state.
- **Every block appears**: a block golem drives over MIDI (`engine: "midi"`: `whammy-dt`, `midi-pedal`)
  is an enamel pedal with a 5-pin DIN socket on top and a MIDI stamp. `whammy-dt` (pitch) is an
  enamel treadle. Blocks with no catalogue entry fall back to a grey enamel pedal.
- **Fonts**: the gallery loads Saira Semi Condensed, Shrikhand, Big Shoulders Display and Pirata One
  (Blackwork names) from Google Fonts. The rig can run offline: golem should vendor them (all three are under the OFL).
- **Choices left to golem**: which archetype a `pedal-nam` / `amp` block shows (from the capture picked),
  the amp's family (from the archetype), the stack/combo form.
