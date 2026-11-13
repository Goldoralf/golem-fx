# golem pedals (visuals)

> **Status: PROTOTYPE** (2026-11-13). A gallery and the SVG renderers that draw every golem block as a pedal,
> plus golem's glyph set. Nothing is wired into golem yet.

Open [index.html](index.html) in a browser (it works from disk, no server needed). It shows:

- **a lane drawn for real**: the pedals on a stage floor or a rug, patch cables, the guitar cable coming
  in, and the amp (or a stage box when the lane has no amp) at the end. Under it, the same lane in block
  mode (golem's tiles). Clicking a pedal or a tile switches the block on or off in both views;
- **the golem-fx pedals** (24), **the NAM line** (effect captures, heads, cabs, head + cab), **the generic
  pedal** for every other block golem hosts, and **the glyph set** (74 glyphs).

## The idea: the glyph is the link

A golem is brought to life by a word on its forehead. Each block has one **glyph**, and the glyph is what
you recognise in both modes: on the block tile, and on the pedal (cast into golem-fx pedals as a rune that
lights with the pedal, on an enamel badge on generic pedals, etched on the NAM pedals' name plate).

## Three families, never confused

| Family | For | Look |
|---|---|---|
| **Clay** (`gfxPedal`) | golem-fx effects | Matte cast body tinted by the block's family, rounded like a casting; the glyph carved in a cartouche, lit when on; knurled low-profile knobs with a pointer in the rune's colour |
| **Specimen** (`namPedal`, `namHead`, `namCab`, `namStack`) | NAM captures, heads, cabs, head + cab | Brushed steel and glass for the effect captures: the captured circuit (a lattice, a valve for preamps, a needle for compressors) glows behind a window, coloured by the capture's family (overdrive, distortion, fuzz, boost, bass…). Amps in tolex, voiced by their family: clean (cream tolex, wheat cloth), crunch (oxblood and gold), high gain (black and steel), bass (navy). Cabs 1x12 · 2x12 · 4x12 · 2x10 · 8x10 from the cab's name; head + cab as a stack or a combo |
| **Enamel** (`genericPedal`) | every other golem block | A hammertone die-cast box with screws, an aged colour from the block's family, cream border, jewel lamp, chrome stomp switch; the glyph on a cream enamel badge, the name in a 70s script |

No family copies a maker's livery: amps take a period and a material, not a brand. Archetype names
(Twin, JCM800, TS…) come from golem's catalogue, where they name the captures.

## Files

| Path | What |
|---|---|
| `data/glyphs.json` | **The glyph set**: one per catalogue type (+ `gapmeter`), inner SVG on a 24 × 24 grid, stroked in `currentColor` like golem's `app/icons.js`; `uni` = the former Unicode glyph, kept as a text fallback |
| `glyphs/<type>.svg` | Each glyph as a standalone SVG (generated) |
| `data/faces.json` | Each golem-fx pedal's face: knobs, selectors, toggles, the blocks it stands for |
| `data/manifest.json` | Every effect's parameters, generated from the code: `cargo run -p gfx-bench -- manifest` |
| `data/golem-blocks.json` | Snapshot of golem's catalogue (types, colours, first parameters, pedal / amp / cab archetypes) |
| `data/visuals-data.js` | Everything above in one script (generated), read by the gallery and by golem |
| `js/golem-pedals.js` | The renderers: pedals of the three families, amps, block tiles, glyphs. `pedalFor(type)` picks the right one |
| `js/golem-stage.js` | `GolemStage.render(chain, {floor})`: the realistic lane |
| `tools/build-data.py` | Builds `visuals-data.js` and `glyphs/`, checks that every catalogue type has a glyph and every face's parameters exist |

Rebuild after a parameter change or a catalogue change:

```sh
python3 visuals/tools/build-data.py --manifest --golem ~/golem
```

## Integration in golem (later)

- **Glyphs**: golem's catalogue keeps `glyph` as a Unicode character today, with duplicates (`∿` for eq,
  tremolo and vibrato; `≈` for chorus and stretch; `△` for both boosts). The SVG set gives each type its
  own glyph. golem can load `data/glyphs.json` (or `visuals-data.js`) and draw `GolemPedals.glyph(type)`
  wherever it prints `glyph` today, keeping the Unicode character as the fallback.
- **Pedals**: the renderers are plain scripts returning SVG strings. Call `GolemPedals.defs()` once per
  page (shared gradients, textures and filters, ids prefixed `gp-`), then `GolemPedals.pedalFor({type, on,
  archetype, family, title})` for a block, or `GolemStage.render(chain, {floor})` for a lane.
- **Fonts**: the gallery loads Saira Semi Condensed, Shrikhand and Big Shoulders Display from Google
  Fonts. The rig can run offline: golem should vendor them (all three are under the OFL).
- **Choices left to golem**: which archetype a `pedal-nam` / `amp` block shows (from the capture picked),
  the amp's family (from the archetype), the stack/combo form.
