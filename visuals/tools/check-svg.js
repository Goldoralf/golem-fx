#!/usr/bin/env node
// Renders every drawing variant and checks each SVG is well-formed XML: no attribute twice on an element,
// tags balanced. Run: node visuals/tools/check-svg.js (exit code 1 on any error).
'use strict';
const path = require('path');
global.window = global;
require(path.join(__dirname, '../data/visuals-data.js'));
require(path.join(__dirname, '../js/golem-pedals.js'));
const P = window.GolemPedals;

function check(svg) {
  const errs = [], stack = [];
  const re = /<(\/?)([a-zA-Z][\w:-]*)((?:\s+[\w:-]+=(?:"[^"]*"|'[^']*'))*)\s*(\/?)>/g;
  let m, last = 0;
  while ((m = re.exec(svg))) {
    if (/[<>]/.test(svg.slice(last, m.index).replace(/&[a-z#0-9]+;/g, ''))) errs.push(`stray markup before <${m[2]}`);
    last = re.lastIndex;
    const [, close, tag, attrs, self] = m;
    if (close) { const top = stack.pop(); if (top !== tag) errs.push(`</${tag}> closes <${top}>`); continue; }
    const names = [...attrs.matchAll(/([\w:-]+)=["']/g)].map(a => a[1]);
    const dup = names.filter((n, i) => names.indexOf(n) !== i);
    if (dup.length) errs.push(`<${tag}> repeats ${[...new Set(dup)].join(', ')}`);
    if (!self) stack.push(tag);
  }
  if (stack.length) errs.push(`unclosed ${stack.join(' > ')}`);
  return errs;
}

const cases = [];
for (const type of P.KIT_TYPES) for (const compact of [false, true]) {
  for (const lit of [[], ...P.KIT_PIECES.map(p => [p]), P.KIT_PIECES]) for (const on of [true, false])
    cases.push([`drumKit ${type}${compact ? ' compact' : ''} lit=${lit.join('+') || '-'} on=${on}`, P.drumKit({ type, title: 'Black Pearl & Co', lit, on, compact })]);
}
for (const compact of [false, true]) for (const lit of [[], [0], [0, 5, 15]])
  cases.push([`padKit lit=${lit}`, P.padKit({ title: 'House', compact, lit, pads: [{ label: 'kick', hue: '#ff5a3c' }] })]);
for (const kind of ['piano', 'epiano', 'organ', 'synth']) for (const compact of [false, true]) for (const extra of [{}, { family: 'mallet' }, { family: 'harpsichord' }, { style: 'percussion' }])
  cases.push([`keysUnit ${kind} ${JSON.stringify(extra)}`, P.keysUnit(Object.assign({ kind, title: 'Tine <E.Piano>', compact, macros: [{ name: 'a', value: .5 }] }, extra))]);
for (const kind of P.HARDWARE_KINDS.concat('unknown')) for (const on of [true, false])
  cases.push([`hardwareUnit ${kind} on=${on}`, P.hardwareUnit(kind, { on, label: 'Pitch <A&B> "x"' })]);
for (const type of P.FEATURE_TYPES) for (const on of [true, false]) for (const title of [undefined, 'Rec <A&B> "x"'])
  cases.push([`featureUnit ${type} on=${on} title=${title || '-'}`, P.featureUnit(type, { on, title })]);
for (const a of (P.RACK_GENERIC ? Object.keys(P.RACK_GENERIC) : [])) {
  cases.push([`namRack ${a}`, P.namRack({ kind: a })], [`namModule ${a}`, P.namModule({ kind: a })]);
}
// every outboard unit (archetypes and generics) in both formats, on and off, rack and module
{
  const units = (window.GOLEM_VISUALS.archetypes.outboard || []).map(a => ({ archetype: a }))
    .concat(Object.keys(P.RACK_GENERIC).map(kind => ({ kind })));
  for (const u of units) for (const format of ['nam', 'ir']) for (const on of [true, false]) for (const label of ['', 'Tank <A&B> Spring Reverb 3', 'delay 12 - B']) {
    const o = Object.assign({ on, format, label }, u), n = `${u.archetype ? u.archetype.id : u.kind} ${format} on=${on} label=${label || '-'}`;
    cases.push([`namRack ${n}`, P.namRack(o)], [`namModule ${n}`, P.namModule(o)]);
  }
  cases.push(['namRack ir program', P.namRack({ format: 'ir', label: 'Studio unit', program: 'Hall <3>' })]);
}

const D = window.GOLEM_VISUALS;
for (const crate of Object.keys(D.gfx || {})) for (const style of P.STYLES) { P.setStyle(style); cases.push([`gfxPedal ${crate} ${style}`, P.gfxPedal(crate, { on: true })]); }
P.setStyle('auto');
// slider faces: every fader set (by the enum's rank), on and off
for (const crate of Object.keys(D.gfx || {})) {
  const fs = D.gfx[crate].fader_sets;
  if (fs && fs.param) fs.sets.forEach((_, rank) => [true, false].forEach(on => cases.push([`gfxPedal ${crate} set ${rank} on=${on}`, P.gfxPedal(crate, { on, values: { [fs.param.id]: rank, level: 6 } })])));
}
for (const b of D.blocks || []) cases.push([`pedalFor ${b.type}`, P.pedalFor(b.type)]);
for (const fam of Object.keys(P.AMP_LOOK)) cases.push([`namHead ${fam}`, P.namHead({ family: fam, title: 'Head' })], [`namStack ${fam}`, P.namStack({ family: fam, title: 'Stack' })]);

// the rig by sections: every section and the card, on every floor, outboard as rack and on the board
{
  require(path.join(__dirname, '../js/golem-stage.js'));
  require(path.join(__dirname, '../js/golem-rig.js'));
  const R = window.GolemRig, S = window.GolemStage;
  const rigs = {
    split: S.fromGolem({ pre: [{ id: 'w', type: 'whammy-dt' }, { id: 't', type: 'tuner' }, { id: 'g', type: 'gate' }],
      split: { mode: 'merge', gain: -3, branches: [
        { id: 'a', blocks: [{ id: 'd', type: 'guitar-drive' }, { id: 'h', type: 'amp-nam', label: 'EVH 5150 III Stealth' }, { id: 'c', type: 'cab', label: 'Mesa 4x12 <V30>' }] },
        { id: 'b', blocks: [{ id: 'h2', type: 'amp-nam', label: 'Peavey 5150 II' }, { id: 'c2', type: 'cab', label: '4x12' }, { id: 'e', type: 'eq' }] }] },
      post: [{ id: 'o', type: 'comp-nam', label: 'UA 1176 <A&B>' }, { id: 'i', type: 'ir-fx', label: 'Tank Spring Reverb 3' }, { id: 'dl', type: 'delay' }, { id: 'r', type: 'rec' }] },
    { blocks: { dl: false }, branches: { b: { mute: true } } }),
    voice: ['deesser', 'comp', { type: 'preamp-nam', label: 'Neve 1073' }, 'golem-double', 'delay', 'reverb-shimmer', 'click', 'looper'],
    combo: ['wah', 'eq-graphic', { type: 'midi-pedal', kind: 'floor-multi' }, { type: 'amp', family: 'clean', title: 'Twin', form: 'combo' }, 'reverb'],
    keys: S.fromGolem({ chain: [{ id: 'v', type: 'vital', params: { preset: 'Sparkly_Dreamy <Pad>' } }, { id: 'o', type: 'organ', params: { db16: 8, db8: 8 } }, { id: 'c', type: 'chorus' }] }),
    drums: S.fromLane({ id: 'd', chain: { chain: [{ id: 'k', type: 'drumkit', params: { kit: 'drums-black-pearl' } }, { id: 'p', type: 'pads', params: { kit: 'pads-lofi' } }, { id: 'c', type: 'comp' }] }, states: [{ id: 's', name: 'A', blocks: { c: false } }] }, 'A'),
    kit: [{ type: 'drumkit', title: 'Jazz <kit>', kitType: 'jazz' }], piano: ['piano', { type: 'ep', title: 'Clav & Co' }],
    empty: [],
  };
  const lanes = [{ id: 'g<1>', name: 'Guitar <1>', chain: { pre: [{ id: 'a', type: 'amp', label: 'Marshall JCM800' }, { id: 'c', type: 'cab', label: '4x12' }] } },
    { id: 'k', name: 'Keys', chain: { chain: [{ id: 'p', type: 'piano' }] } }, { id: 'v', name: 'Voice', chain: [{ id: 'd', type: 'deesser' }] },
    { id: 'e', name: 'Empty', chain: {} }, { id: 'd', name: 'Drums', chain: { chain: [{ id: 'k', type: 'drumkit' }] } }];
  for (let n = 1; n <= lanes.length; n++) for (const floor of ['stage', 'neon']) cases.push([`rig presetCard ${n} ${floor}`, R.presetCard(lanes.slice(0, n), { floor, seed: 'p' })]);
  cases.push(['rig presetCard 6', R.presetCard(lanes.concat(lanes[0], lanes[1]), {})]);
  for (const [name, chain] of Object.entries(rigs)) for (const outboard of ['rack', 'board']) for (const features of ['tab', 'hide', 'inline']) {
    const secs = R.sections(chain, { outboard, features });
    for (const floor of R.FLOORS) {
      for (const s of secs) cases.push([`rig ${name} ${outboard} ${features} ${floor} ${s.key}`, R.render(s, { floor, outboard, seed: name })]);
      if (secs.length) cases.push([`rig card ${name} ${outboard} ${features} ${floor}`, R.card(secs, { floor, outboard, seed: 7 })]);
      if (secs.length && floor === 'stage') cases.push([`rig standalone ${name} ${outboard} ${features}`, R.standalone(R.card(secs, { floor, outboard, detail: 'full' }), { fontsCss: 'https://example.org/f.css?a=1&b=2' })]);
    }
  }
}

let bad = 0;
for (const [name, svg] of cases) { const e = check(svg); if (e.length) { bad++; console.log(`✗ ${name}: ${e.slice(0, 3).join('; ')}`); } }
console.log(`${cases.length} drawings, ${bad} invalid`);
process.exit(bad ? 1 : 0);
