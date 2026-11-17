# Fonts of the pedals, as files

The fonts the drawings name, as TTF, so `tools/render-rig.js` rasterises a rig without a browser or a
network. All under the SIL Open Font License 1.1 (each family's `OFL-*.txt` travels with it), from
[google/fonts](https://github.com/google/fonts) on 2026-11-17.

Saira Semi Condensed, Shrikhand, Pirata One and Share Tech Mono are the upstream files, unmodified. Big
Shoulders Display ships upstream as one variable font: the three files here are its static instances at
600, 700 and 800, cut with `fontTools.varLib.instancer` (its licence reserves no font name).
