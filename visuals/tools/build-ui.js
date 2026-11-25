#!/usr/bin/env node
// Builds gfx-ui's bundles: gfx-ui/bundles/<crate>.gfxui, one per golem-fx pedal (design: gfx-ui/README.md §4).
// A bundle is a deflated JSON document: the pedal cut into a body and one layer per control (each control
// drawn at every position, text as paths), the descriptor's parameters and groups, and the factory presets.
//
//   node visuals/tools/build-ui.js [crate…]      write the bundles (all pedals by default)
//   node visuals/tools/build-ui.js --check       fail if a committed bundle is not current
//   (several pedals, or all: one node process per pedal, so memory stays that of one pedal)
//   --out <dir>   another output directory       --png <dir>   also write full / composed PNGs (to look at)
//
// Every bundle is checked before it is written: body + layers, composed, must give the full drawing back
// at the defaults, at both ends of every control and at random settings. Needs `npm install` in visuals/.
'use strict';
const fs = require('fs'), path = require('path'), zlib = require('zlib');
const V = path.join(__dirname, '..');
global.window = global;
require(path.join(V, 'data/visuals-data.js'));
require(path.join(V, 'js/golem-pedals.js'));
const P = window.GolemPedals;
const { Resvg } = require(path.join(V, 'node_modules/@resvg/resvg-js'));

const VERSION = 1;
const SCALE = 3;                       // px per viewBox unit for the checks and the hit boxes
const STEPS = { knob: 64, fader: 48 }; // intervals: a knob has 65 frames
const TOL = { level: 8, share: 0.0005 };

const args = process.argv.slice(2);
const flag = (name) => { const i = args.indexOf(name); if (i < 0) return null; const v = args[i + 1]; args.splice(i, 2); return v; };
const OUT = flag('--out') || path.join(V, '../gfx-ui/bundles');
const PNG = flag('--png');
const CHECK = args.includes('--check'); if (CHECK) args.splice(args.indexOf('--check'), 1);

const FONT_DIR = path.join(V, 'assets/fonts');
const fontFiles = fs.readdirSync(FONT_DIR).filter(f => /\.(ttf|otf)$/i.test(f)).map(f => path.join(FONT_DIR, f));
const SHARED = /<defs>([\s\S]*)<\/defs>/.exec(P.defs())[1];
const DESC = JSON.parse(fs.readFileSync(path.join(V, 'data/descriptors.json'), 'utf8')).descriptors;
const METERS = JSON.parse(fs.readFileSync(path.join(V, '../gfx-ui/meters.json'), 'utf8'));
const METER_KINDS = ['gr', 'bar', 'lamp', 'state', 'value', 'bands'];

// The meters shown under the pedal (gfx-ui/meters.json): every meter of the descriptor shown once or hidden.
function meters(crate, desc) {
  const have = (desc.meters || []).map(m => m.id), spec = METERS[crate];
  if (!have.length && !spec) return [];
  if (!spec) throw new Error(`${crate}: meters ${have.join(', ')} are not in gfx-ui/meters.json`);
  const rows = spec.rows || [], hide = spec.hide || [];
  const named = rows.flatMap(r => r.kind === 'bands' ? r.ids : [r.id]).concat(hide);
  for (const r of rows) {
    if (!METER_KINDS.includes(r.kind)) throw new Error(`${crate}: meter kind "${r.kind}"`);
    if (r.kind === 'state' && !(r.states || []).length) throw new Error(`${crate}: state meter ${r.id} has no states`);
    if (r.kind === 'bands' && (r.labels || []).length !== r.ids.length) throw new Error(`${crate}: bands ${r.label}: one label per meter`);
    if (['gr', 'bar', 'bands'].includes(r.kind) && !(r.max > (r.min || 0))) throw new Error(`${crate}: meter ${r.id || r.label} has no range`);
  }
  for (const id of named) if (!have.includes(id)) throw new Error(`${crate}: gfx-ui/meters.json names ${id}, not a meter of the plugin`);
  for (const id of have) if (named.filter(x => x === id).length !== 1) throw new Error(`${crate}: meter ${id} must be shown or hidden once in gfx-ui/meters.json`);
  return rows.map(r => ({ kind: r.kind, label: r.label, ids: r.kind === 'bands' ? r.ids : [r.id], labels: r.labels || [],
    min: r.kind === 'gr' ? 0 : r.kind === 'bands' ? -r.max : (r.min || 0), max: r.max || 1, center: !!r.center || r.kind === 'bands',
    states: r.states || [], format: r.format || null }));
}
// A meter row's height in rows (the editor's ROW): the bands take two.
const rowsOf = (ms) => ms.reduce((n, m) => n + (m.kind === 'bands' ? 2 : 1), 0);

// A drawing as a file of its own: namespace and the shared defs.
const standalone = (svg) => svg.replace(/^<svg /, '<svg xmlns="http://www.w3.org/2000/svg" ').replace(/>/, `><defs>${SHARED}</defs>`);
const resvg = (svg, width) => new Resvg(svg, { fitTo: { mode: 'width', value: width },
  font: { fontFiles, loadSystemFonts: false, defaultFontFamily: 'Saira SemiCondensed' } });
// Text as paths, `use` resolved, unused defs dropped: what the plugin's resvg reads without a font.
const flatten = (svg) => resvg(standalone(svg), 100).toString();

// Cuts a pedal: {head, body, controls: [{kind, id, svg}]}, each control's svg a drawing of its own
// (the root, the pedal's defs, the group inside its ancestors' opening tags).
function cut(svg) {
  const head = /^<svg [^>]*>/.exec(svg)[0];
  const defs = (/^<svg [^>]*>\s*(<defs>[\s\S]*?<\/defs>)/.exec(svg) || [])[1] || '';
  const tag = /<(\/?)([a-zA-Z][\w:-]*)((?:\s+[\w:-]+=(?:"[^"]*"|'[^']*'))*)\s*(\/?)>/g;
  const stack = [], controls = [];
  let m, body = '', last = 0;
  while ((m = tag.exec(svg))) {
    const [open, close, name, attrs, self] = m;
    if (close) { const top = stack.pop(); if (!top || top.name !== name) throw new Error(`</${name}> closes <${top && top.name}>`); continue; }
    const c = name === 'g' && /class="(?:[^"]* )?gp-(knob|selector|toggle|fader)(?: [^"]*)?"/.exec(attrs);
    const id = c && (/data-param="([^"]*)"/.exec(attrs) || [])[1];
    if (!c || !id || self) { if (!self) stack.push({ name, open }); continue; }
    let depth = 1, end = tag.lastIndex, k;
    const inner = /<(\/?)([a-zA-Z][\w:-]*)(?:\s+[\w:-]+=(?:"[^"]*"|'[^']*'))*\s*(\/?)>/g; inner.lastIndex = end;
    while (depth > 0 && (k = inner.exec(svg))) { if (k[3]) continue; depth += k[1] ? -1 : 1; end = inner.lastIndex; }
    if (depth) throw new Error(`control ${id}: unbalanced group`);
    const anc = stack.slice(1);          // without the root
    controls.push({ kind: c[1], id, svg: head + defs + anc.map(a => a.open).join('') + svg.slice(m.index, end)
      + anc.slice().reverse().map(a => `</${a.name}>`).join('') + '</svg>' });
    body += svg.slice(last, m.index); last = end; tag.lastIndex = end;
  }
  // Nothing the regular expression missed: every control group of the drawing was cut, once per parameter.
  const drawn = (svg.match(/\bgp-(knob|selector|toggle|fader)\b[^>]*data-param=/g) || []).length;
  if (drawn !== controls.length) throw new Error(`${drawn} control groups drawn, ${controls.length} cut`);
  if (new Set(controls.map(c => c.id)).size !== controls.length) throw new Error('two controls for one parameter');
  return { head, body: body + svg.slice(last), controls };
}

// Source over, inside the rectangle [x, y, w, h] of pixels only: the editor draws a frame cut to its box.
const over = (dst, src, width, box) => { const height = dst.length / 4 / width;
  for (let y = Math.max(0, box[1]); y < Math.min(height, box[1] + box[3]); y++)
    for (let x = Math.max(0, box[0]); x < Math.min(width, box[0] + box[2]); x++) { const i = (y * width + x) * 4, a = src[i + 3] / 255;
      for (let c = 0; c < 3; c++) dst[i + c] = Math.round(src[i + c] + dst[i + c] * (1 - a));
      dst[i + 3] = Math.round(src[i + 3] + dst[i + 3] * (1 - a)); } };
// A box in viewBox units as pixels of the face (gfx-ui's raster::pixel_box).
const pixelBox = (vb, b) => { const x0 = Math.floor((b[0] - vb[0]) * SCALE), y0 = Math.floor((b[1] - vb[1]) * SCALE);
  return [x0, y0, Math.ceil((b[0] + b[2] - vb[0]) * SCALE) - x0, Math.ceil((b[1] + b[3] - vb[1]) * SCALE) - y0]; };

// The values a control is drawn at, one per frame.
function positions(kind, p) {
  if (p.kind === 'bool') return [false, true];
  if (p.kind === 'enum') return p.options.map((_, i) => i);
  const n = STEPS[kind] || STEPS.knob;
  return Array.from({ length: n + 1 }, (_, i) => p.min + (p.max - p.min) * i / n);
}
// The frame that draws a value (the editor's rule, here for the check).
function frameOf(ctl, p, v) {
  if (p.kind === 'bool') return v ? 1 : 0;
  if (p.kind === 'enum') return typeof v === 'number' ? v : Math.max(0, p.options.findIndex(o => o.toLowerCase() === String(v).toLowerCase()));
  return Math.round((v - p.min) / (p.max - p.min) * (ctl.frames.length - 1));
}

function face(crate, desc, fixed) {
  // The renderer numbers a pedal's own gradients with a counter (gpm82-b): numbered here as the only pedal
  // of its file, so a bundle does not depend on what was drawn before it.
  const draw = (values) => P.gfxPedal(crate, { on: true, values: Object.assign({}, fixed, values) }).replace(/\bgpm\d+(?=-)/g, 'gpm');
  const base = draw({});
  const width = Math.round(parseFloat(/ width="([\d.]+)"/.exec(base)[1]) * SCALE);
  const vb = / viewBox="([^"]*)"/.exec(base)[1].split(/[\s,]+/).map(Number);
  // The editor takes the viewBox as the drawing's size: one unit is one px of the file.
  if (Math.abs(parseFloat(/ width="([\d.]+)"/.exec(base)[1]) - vb[2]) > 1e-6) throw new Error(`${crate}: the drawing's width is not its viewBox's`);
  const parts = cut(base);
  const controls = parts.controls.map((c) => {
    const p = desc.params[c.id];
    if (!p) throw new Error(`${crate}: control "${c.id}" is not a parameter of the descriptor`);
    const frames = [], box = [Infinity, Infinity, -Infinity, -Infinity];
    for (const v of positions(c.kind, p)) {
      const at = cut(draw({ [c.id]: v })).controls.find(x => x.id === c.id);
      const flat = flatten(at.svg);
      const r = resvg(flat, width).render(), px = r.pixels;
      for (let y = 0; y < r.height; y++) for (let x = 0; x < r.width; x++) if (px[(y * r.width + x) * 4 + 3] > 8) {
        if (x < box[0]) box[0] = x; if (y < box[1]) box[1] = y; if (x > box[2]) box[2] = x; if (y > box[3]) box[3] = y; }
      frames.push(flat);
    }
    if (!isFinite(box[0])) throw new Error(`${crate}: control "${c.id}" draws nothing`);
    const u = (n) => Math.round(n / SCALE * 100) / 100, pad = 1.5;
    const bbox = [u(box[0]) + vb[0] - pad, u(box[1]) + vb[1] - pad, u(box[2] - box[0] + 1) + 2 * pad, u(box[3] - box[1] + 1) + 2 * pad];
    return { param: c.id, kind: c.kind, bbox, frames };
  });
  // The chrome's accent: the body's own colour (the middle stop of the pedal's body gradient).
  const stops = [...((/<linearGradient id="gpm-b"[^>]*>([\s\S]*?)<\/linearGradient>/.exec(base) || [])[1] || '').matchAll(/stop-color="(#[0-9a-fA-F]{6})"/g)].map(s => s[1]);
  const out = { view_box: vb, accent: stops[1] || stops[0] || '#c8a24a', body: flatten(parts.body), controls };

  // The check: body + frames against the full drawing.
  const rnd = (() => { let s = 0x9e3779b9 ^ crate.length; for (const ch of crate) s = Math.imul(s ^ ch.charCodeAt(0), 0x85ebca6b);
    return () => { s ^= s << 13; s ^= s >>> 17; s ^= s << 5; return (s >>> 0) / 4294967296; }; })();
  const ends = (p) => p.kind === 'bool' ? [false, true] : p.kind === 'enum' ? [0, p.options.length - 1] : [p.min, p.max];
  const pick = (p) => p.kind === 'bool' ? rnd() < .5 : p.kind === 'enum' ? Math.floor(rnd() * p.options.length) : p.min + (p.max - p.min) * Math.round(rnd() * 64) / 64;
  const settings = [{}];
  for (const c of controls) for (const v of ends(desc.params[c.param])) settings.push({ [c.param]: v });
  for (let i = 0; i < 8; i++) settings.push(Object.fromEntries(controls.map(c => [c.param, pick(desc.params[c.param])])));
  const bodyPx = resvg(out.body, width).render();
  // The value a frame was drawn at: the reference is drawn there, so the check measures the cut alone,
  // not the distance from a value to its nearest frame.
  const onFrame = (c, p, v) => p.kind === 'float' ? p.min + (p.max - p.min) * frameOf(c, p, v) / (c.frames.length - 1) : v;
  let worst = { level: 0, share: 0 };
  settings.forEach((given, n) => {
    const values = Object.fromEntries(controls.map(c => { const p = desc.params[c.param];
      return [c.param, onFrame(c, p, c.param in given ? given[c.param] : p.default)]; }));
    const full = resvg(standalone(draw(values)), width).render();
    const comp = Buffer.from(bodyPx.pixels);
    for (const c of controls) { const p = desc.params[c.param];
      over(comp, resvg(c.frames[frameOf(c, p, values[c.param])], width).render().pixels, bodyPx.width, pixelBox(vb, c.bbox)); }
    let level = 0, count = 0; const a = full.pixels;
    for (let i = 0; i < comp.length; i++) { const d = Math.abs(comp[i] - a[i]); if (d > level) level = d; if (d > TOL.level) count++; }
    const share = count / (comp.length / 4);
    if (share > worst.share) worst = { level, share, values };
    if (PNG && n === 0) { fs.mkdirSync(PNG, { recursive: true });
      fs.writeFileSync(path.join(PNG, `${crate}-full.png`), full.asPng());
      fs.writeFileSync(path.join(PNG, `${crate}-body.png`), bodyPx.asPng()); }
  });
  if (worst.share > TOL.share)
    throw new Error(`${crate}: body + layers differ from the full drawing on ${(worst.share * 100).toFixed(2)} % of the pixels (max ${worst.level}) at ${JSON.stringify(worst.values)}`);
  out.check = worst.share;
  return out;
}

function bundle(crate) {
  const desc = DESC[crate];
  if (!desc) throw new Error(`${crate}: no descriptor`);
  const f = face(crate, desc, {});
  const check = f.check; delete f.check;
  const keep = (p) => ({ name: p.name, kind: p.kind, min: p.min, max: p.max, default: p.default, unit: p.unit,
    display: p.display, step: p.step, decimals: p.decimals, options: p.options, depends: p.depends, help: p.help });
  const doc = { version: VERSION, crate, name: desc.name, class_id: desc.class_id, face: f,
    front: desc.front, groups: desc.groups,
    params: Object.fromEntries(Object.entries(desc.params).map(([id, p]) => [id, keep(p)])),
    meters: meters(crate, desc),
    presets: (desc.presets || []).map(p => ({ id: p.id, type: p.type, name: p.name, desc: p.desc, default: !!p.default, params: p.params })) };
  const json = JSON.stringify(doc);
  // Header, read without inflating (the window's size is needed when the plugin loads): "GFXU", version,
  // flags (bit 0: an expert panel; bits 1-4: the meter rows under the pedal), the face's width and height
  // in viewBox units (u16 LE).
  const onFace = new Set(f.controls.map(c => c.param));
  const panel = Object.keys(doc.params).some(id => !onFace.has(id));
  const head = Buffer.alloc(10); head.write('GFXU', 0, 'latin1'); head[4] = VERSION; const rows = rowsOf(doc.meters); if (rows > 15) throw new Error(`${crate}: ${rows} meter rows, 15 at most`);
  head[5] = (panel ? 1 : 0) | rows << 1;
  head.writeUInt16LE(Math.ceil(f.view_box[2]), 6); head.writeUInt16LE(Math.ceil(f.view_box[3]), 8);
  return { data: Buffer.concat([head, zlib.deflateSync(Buffer.from(json, 'utf8'), { level: 9 })]), head, json, raw: json.length, check, controls: f.controls.length };
}

const crates = args.length ? args : Object.keys(DESC);
let failed = 0;
// Several pedals: one process each. The rendered frames of a pedal are only freed when its process ends
// (about 165 MB per control), and all the pedals in one process need more memory than a desktop has.
if (crates.length > 1) {
  const pass = ['--out', OUT].concat(PNG ? ['--png', PNG] : [], CHECK ? ['--check'] : []);
  for (const crate of crates) {
    const r = require('child_process').spawnSync(process.execPath, [__filename, ...pass, crate], { stdio: 'inherit' });
    if (r.status !== 0) { if (r.status === null) console.error(`${crate}: stopped (${r.signal || r.error})`); failed++; }
  }
  process.exit(failed ? 1 : 0);
}
for (const crate of crates) {
  try {
    const b = bundle(crate), file = path.join(OUT, `${crate}.gfxui`);
    if (CHECK) {
      const cur = fs.existsSync(file) ? fs.readFileSync(file) : null;
      // The document is compared, not its deflated bytes: those depend on node's zlib.
      const same = cur && cur.subarray(0, 10).equals(b.head) && zlib.inflateSync(cur.subarray(10)).toString('utf8') === b.json;
      if (!same) { console.error(`${crate}: bundle is not current (run node visuals/tools/build-ui.js)`); failed++; }
      continue;
    }
    fs.mkdirSync(OUT, { recursive: true });
    fs.writeFileSync(file, b.data);
    console.log(`${crate.padEnd(18)} ${b.controls} controls  ${(b.raw / 1024).toFixed(0)} KB -> ${(b.data.length / 1024).toFixed(1)} KB  cut ${(b.check * 100).toFixed(3)} %`);
  } catch (e) { console.error(String(e.message || e)); failed++; }
}
if (failed) process.exit(1);
