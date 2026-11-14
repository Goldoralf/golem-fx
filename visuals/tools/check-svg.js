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
for (const a of (P.RACK_GENERIC ? Object.keys(P.RACK_GENERIC) : [])) {
  cases.push([`namRack ${a}`, P.namRack({ kind: a })], [`namModule ${a}`, P.namModule({ kind: a })]);
}

const D = window.GOLEM_VISUALS;
for (const crate of Object.keys(D.gfx || {})) for (const style of P.STYLES) { P.setStyle(style); cases.push([`gfxPedal ${crate} ${style}`, P.gfxPedal(crate, { on: true })]); }
P.setStyle('auto');
for (const b of D.blocks || []) cases.push([`pedalFor ${b.type}`, P.pedalFor(b.type)]);
for (const fam of Object.keys(P.AMP_LOOK)) cases.push([`namHead ${fam}`, P.namHead({ family: fam, title: 'Head' })], [`namStack ${fam}`, P.namStack({ family: fam, title: 'Stack' })]);

let bad = 0;
for (const [name, svg] of cases) { const e = check(svg); if (e.length) { bad++; console.log(`✗ ${name}: ${e.slice(0, 3).join('; ')}`); } }
console.log(`${cases.length} drawings, ${bad} invalid`);
process.exit(bad ? 1 : 0);
