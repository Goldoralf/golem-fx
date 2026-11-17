#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-or-later
// Renders a rig's pictures to files: the card (16:9) and, with --sections, one file per section.
//
//   node visuals/tools/render-rig.js rig.json [--out DIR] [--name NAME] [--sections]
//        [--genre "Thrash metal" | --floor plate] [--seed ID] [--outboard rack|board] [--features tab|hide|inline]
//        [--png] [--webp] [--width 1280] [--fonts URL-or-file.css] [--chrome /path/to/chrome]
//
// rig.json: golem's lane chain ({pre, split, post} or a flat block list), or {chain, state, genre, id, name}
// with `state` a lane state ({blocks, branches}), or GolemStage.render()'s items under {items}.
// Always writes standalone SVG files. --png rasterises them with a headless Chrome / Chromium (found on the
// PATH, or --chrome, or $CHROME); --webp converts the PNG with `cwebp`. The fonts come from Google Fonts unless
// --fonts gives another stylesheet (golem vendors one: app/vendor/golem-visuals/fonts.css).
'use strict';
const fs = require('fs'), os = require('os'), path = require('path');
const { execFileSync } = require('child_process');
global.window = global;
for (const f of ['../data/visuals-data.js', '../js/golem-pedals.js', '../js/golem-stage.js', '../js/golem-rig.js']) require(path.join(__dirname, f));
const R = window.GolemRig, S = window.GolemStage;

const argv = process.argv.slice(2), opt = {}, files = [];
const FLAGS = ['sections', 'png', 'webp', 'help'];
for (let i = 0; i < argv.length; i++) {
  if (!argv[i].startsWith('--')) { files.push(argv[i]); continue; }
  const k = argv[i].slice(2);
  opt[k] = FLAGS.includes(k) ? true : argv[++i];
}
if (opt.help || files.length !== 1) {
  console.error(fs.readFileSync(__filename, 'utf8').split('\n').slice(2, 14).map(l => l.replace(/^\/\/ ?/, '')).join('\n'));
  process.exit(opt.help ? 0 : 2);
}
const FONTS = 'https://fonts.googleapis.com/css2?family=Big+Shoulders+Display:wght@600;700;800&family=Saira+Semi+Condensed:wght@400;600;700&family=Shrikhand&family=Pirata+One&family=Share+Tech+Mono&display=swap';
const doc = JSON.parse(fs.readFileSync(files[0], 'utf8'));
const wrapped = doc && !Array.isArray(doc) && (doc.chain || doc.items);
const items = wrapped && doc.items ? doc.items : S.fromGolem(wrapped ? doc.chain : doc, wrapped ? doc.state : undefined);
const name = opt.name || (wrapped && (doc.name || doc.id)) || path.basename(files[0], path.extname(files[0]));
const slug = String(name).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'rig';
const floor = opt.floor || R.floorFor(opt.genre || (wrapped && doc.genre));
if (!R.FLOORS.includes(floor)) { console.error(`unknown floor "${floor}": ${R.FLOORS.join(', ')}`); process.exit(2); }
const o = { floor, seed: opt.seed || (wrapped && doc.id) || name, outboard: opt.outboard === 'board' ? 'board' : 'rack' };
const secs = R.sections(items, { outboard: o.outboard, features: opt.features || 'tab' });
if (!secs.length) { console.error('nothing to draw: the chain is empty'); process.exit(1); }
const out = path.resolve(opt.out || '.'), width = Math.max(160, +opt.width || 1280), height = Math.round(width * 9 / 16);
fs.mkdirSync(out, { recursive: true });
const fontsCss = opt.fonts ? (/^https?:/.test(opt.fonts) ? opt.fonts : 'file://' + path.resolve(opt.fonts)) : FONTS;

const jobs = [[`${slug}-card`, R.card(secs, o)]];
if (opt.sections) secs.forEach(s => jobs.push([`${slug}-${s.key}`, R.render(s, o)]));

function chrome() {
  const cands = [opt.chrome, process.env.CHROME, 'google-chrome', 'chromium', 'chromium-browser', 'google-chrome-stable'].filter(Boolean);
  for (const c of cands) { try { execFileSync(c, ['--version'], { stdio: 'ignore' }); return c; } catch (e) { /* next */ } }
  throw new Error('no Chrome / Chromium found: give --chrome or set $CHROME');
}
const written = [];
for (const [base, svg] of jobs) {
  const file = path.join(out, base + '.svg');
  fs.writeFileSync(file, R.standalone(svg, { fontsCss }) + '\n');
  written.push(file);
  if (!opt.png && !opt.webp) continue;
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'golem-rig-')), page = path.join(tmp, 'p.html'), png = path.join(out, base + '.png');
  fs.writeFileSync(page, `<!doctype html><meta charset="utf-8"><link rel="stylesheet" href="${fontsCss}"><style>html,body{margin:0;background:#000}svg{display:block;width:${width}px;height:${height}px}</style>${window.GolemPedals.defs()}${svg}`);
  execFileSync(chrome(), ['--headless=new', '--disable-gpu', '--hide-scrollbars', `--window-size=${width},${height}`, '--virtual-time-budget=6000', `--screenshot=${png}`, 'file://' + page], { stdio: 'ignore' });
  fs.rmSync(tmp, { recursive: true, force: true });
  if (opt.webp) {
    const webp = path.join(out, base + '.webp');
    execFileSync('cwebp', ['-quiet', '-q', '82', png, '-o', webp]);
    written.push(webp);
    if (!opt.png) fs.rmSync(png); else written.push(png);
  } else written.push(png);
}
console.log(JSON.stringify({ name, floor, sections: secs.map(s => ({ key: s.key, label: s.label, blocks: s.items.length })), files: written }, null, 2));
