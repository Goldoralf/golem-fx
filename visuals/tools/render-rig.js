#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-or-later
// Renders a rig's pictures to files, without a browser: the card (16:9) of a patch or of a preset and, with
// --sections, one picture per section.
//
//   node visuals/tools/render-rig.js patch.json [--out DIR] [--name NAME] [--sections]
//        [--genre "Thrash metal" | --floor plate] [--seed ID] [--outboard rack|board] [--features tab|hide|inline]
//        [--format webp|png|svg[,…]] [--width 640] [--quality 82]
//   node visuals/tools/render-rig.js --batch [--out DIR] [the same options, as defaults]  < jobs.ndjson
//
// Input (a file, or one JSON document per line on stdin with --batch):
//   a lane        {chain, states, name, id}: golem's lane, chain as {pre, split, post}, {chain: [...]} or a list
//   a chain       {pre, split, post} | {chain: [...]} | [blocks]
//   a preset      {lanes: [lane, …], name, id, genre}: the group's card (one cell per lane)
// The document's own name, id and genre (or style / styles[0]) are read. What only concerns the picture goes
// under `render`: {name, genre, floor, seed, state (a lane's state: an object, an id or a name), states ({lane
// id: state} for a preset), outboard, features, sections, format, width, quality, out (a directory)}.
// Output: <name>-card.<ext> (and <name>-<section>.<ext>, per lane for a preset), and one JSON line per
// document on stdout: {ok, name, floor, sections | lanes, files} or {ok: false, error}. A bad document does
// not stop a batch. --batch keeps one process for any number of cards: write a line, read a line.
//
// Rasterising uses @resvg/resvg-js and sharp (`npm install` in visuals/) and the fonts in assets/fonts:
// no Chrome, no network. --format svg alone needs neither.
'use strict';
const fs = require('fs'), path = require('path');
global.window = global;
const V = path.join(__dirname, '..');
for (const f of ['data/visuals-data.js', 'js/golem-pedals.js', 'js/golem-stage.js', 'js/golem-rig.js']) require(path.join(V, f));
const P = window.GolemPedals, R = window.GolemRig;

const argv = process.argv.slice(2), opt = {}, files = [];
const FLAGS = ['sections', 'batch', 'help'];
for (let i = 0; i < argv.length; i++) {
  if (!argv[i].startsWith('--')) { files.push(argv[i]); continue; }
  const k = argv[i].slice(2);
  opt[k] = FLAGS.includes(k) ? true : argv[++i];
}
if (opt.help || (!opt.batch && files.length !== 1)) {
  console.error(fs.readFileSync(__filename, 'utf8').split('\n').slice(2, 24).map(l => l.replace(/^\/\/ ?/, '')).join('\n'));
  process.exit(opt.help ? 0 : 2);
}

// ---------- a drawing as a file ----------
const FONT_DIR = path.join(V, 'assets/fonts');
const fontFiles = fs.existsSync(FONT_DIR) ? fs.readdirSync(FONT_DIR).filter(f => /\.(ttf|otf)$/i.test(f)).map(f => path.join(FONT_DIR, f)) : [];
const kitCache = {};
let Resvg, sharp;
const needSharp = why => { if (!sharp) { try { sharp = require('sharp'); } catch (e) { throw new Error(`${why} needs sharp: run \`npm install\` in visuals/`); } } return sharp; };
// The kits' photographs become data URIs, and the families are named as the TTF files name themselves.
// forRaster: resvg does not decode WebP, so a photograph goes in as JPEG, the small one for a small picture.
async function selfContained(svg, forRaster, width) {
  const hrefs = [...new Set([...svg.matchAll(/href="([^"]*kits\/([\w-]+)\.webp)"/g)].map(m => m[2]))];
  for (const kit of hrefs) {
    const key = `${kit}|${forRaster ? (width <= 800 ? 'jpeg-sm' : 'jpeg') : 'webp'}`;
    if (key in kitCache) continue;
    const small = path.join(V, 'assets/kits', `${kit}-sm.webp`), full = path.join(V, 'assets/kits', `${kit}.webp`);
    const file = forRaster && width <= 800 && fs.existsSync(small) ? small : full;
    if (!fs.existsSync(file)) { kitCache[key] = null; continue; }
    kitCache[key] = forRaster ? 'data:image/jpeg;base64,' + (await needSharp('a drum kit\'s photograph')(file).jpeg({ quality: 88 }).toBuffer()).toString('base64')
      : 'data:image/webp;base64,' + fs.readFileSync(file).toString('base64');
  }
  const mode = forRaster ? (width <= 800 ? 'jpeg-sm' : 'jpeg') : 'webp';
  return R.standalone(svg).replace(/href="([^"]*kits\/([\w-]+)\.webp)"/g, (m, href, kit) => `href="${kitCache[`${kit}|${mode}`] || href}"`)
    .replace(/'Saira Semi Condensed'/g, "'Saira SemiCondensed','Saira Semi Condensed'");
}

function raster(svg, width) {
  if (!Resvg) {
    try { ({ Resvg } = require('@resvg/resvg-js')); } catch (e) { throw new Error('rasterising needs @resvg/resvg-js: run `npm install` in visuals/ (or ask for --format svg)'); }
  }
  return new Resvg(svg, { fitTo: { mode: 'width', value: width }, background: '#0d0b09',
    font: { fontFiles, loadSystemFonts: false, defaultFontFamily: 'Saira SemiCondensed' } }).render().asPng();
}
async function webp(png, quality) {
  return needSharp('WebP')(png).webp({ quality }).toBuffer();
}

// ---------- one document ----------
const slugOf = s => String(s || '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'rig';
async function job(doc, fallbackName) {
  const d = doc && !Array.isArray(doc) ? doc : {}, rd = d.render || {}, pick = k => (rd[k] != null ? rd[k] : opt[k]);
  const isPreset = Array.isArray(d.lanes), isLane = !isPreset && d.chain != null;
  const name = rd.name || opt.name || d.name || d.id || fallbackName || 'rig', slug = slugOf(name);
  const floor = pick('floor') || R.floorFor(rd.genre || d.genre || d.style || (d.styles || [])[0] || opt.genre);
  if (!R.FLOORS.includes(floor)) throw new Error(`unknown floor "${floor}": ${R.FLOORS.join(', ')}`);
  const cut = { outboard: pick('outboard') === 'board' ? 'board' : 'rack', features: pick('features') || 'tab' };
  const o = { floor, seed: rd.seed || d.id || opt.seed || name, outboard: cut.outboard };
  const formats = String(pick('format') || 'webp').split(',').map(s => s.trim()).filter(Boolean);
  for (const f of formats) if (!['webp', 'png', 'svg'].includes(f)) throw new Error(`unknown format "${f}": webp, png, svg`);
  const width = Math.max(160, Math.min(3200, +pick('width') || 640)), quality = Math.max(1, Math.min(100, +pick('quality') || 82));
  const wantSections = !!pick('sections'), out = path.resolve(pick('out') || '.');
  const drawings = [], report = { ok: true, name, floor };
  if (isPreset) {
    const lanes = R.preset(d.lanes, Object.assign({ states: rd.states }, cut));
    report.lanes = lanes.map(l => ({ id: l.id, name: l.name, sections: l.sections.map(s => ({ key: s.key, label: s.label, blocks: s.items.length })) }));
    drawings.push([`${slug}-card`, R.presetCard(lanes, o)]);
    if (wantSections) lanes.forEach((l, i) => l.sections.forEach(s => drawings.push([`${slug}-${slugOf(l.id || l.name || i + 1)}-${s.key}`, R.render(s, o)])));
  } else {
    const secs = isLane ? R.fromLane(d, rd.state, cut) : R.sections(d.items || window.GolemStage.fromGolem(Array.isArray(doc) ? doc : d), cut);
    report.sections = secs.map(s => ({ key: s.key, label: s.label, blocks: s.items.length }));
    drawings.push([`${slug}-card`, R.card(secs, o)]);
    if (wantSections) secs.forEach(s => drawings.push([`${slug}-${s.key}`, R.render(s, o)]));
  }
  if (!drawings[0][1]) throw new Error('nothing to draw: the chain is empty');
  fs.mkdirSync(out, { recursive: true });
  report.files = [];
  for (const [base, raw] of drawings) {
    const png = formats.some(f => f !== 'svg') ? raster(await selfContained(raw, true, width), width) : null;
    for (const f of formats) {
      const file = path.join(out, `${base}.${f}`);
      fs.writeFileSync(file, f === 'svg' ? (await selfContained(raw, false)) + '\n' : f === 'png' ? png : await webp(png, quality));
      report.files.push(file);
    }
  }
  return report;
}

(async () => {
  if (!opt.batch) {
    try { console.log(JSON.stringify(await job(JSON.parse(fs.readFileSync(files[0], 'utf8')), path.basename(files[0], path.extname(files[0]))), null, 2)); }
    catch (e) { console.error(e.message); process.exit(1); }
    return;
  }
  let n = 0, bad = 0;
  for await (const line of require('readline').createInterface({ input: process.stdin })) {
    if (!line.trim()) continue;
    n++;
    let rep;
    try { rep = await job(JSON.parse(line), `rig-${n}`); } catch (e) { bad++; rep = { ok: false, error: e.message }; }
    process.stdout.write(JSON.stringify(rep) + '\n');
  }
  process.exitCode = bad && bad === n ? 1 : 0;
})();
