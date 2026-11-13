// SPDX-License-Identifier: GPL-3.0-or-later
// golem stage: one audio lane drawn "for real": the pedals on a stage floor or a rug, patch cables
// between them, the guitar cable coming in, and the amp (or a stage box) at the end.
//
//   GolemStage.render(chain, {floor: 'stage' | 'rug', guitar: true})
//
// chain: golem block types in lane order, or {type, on, archetype, family, title, size, form} objects.
// An `amp-nam` directly followed by a `cab` stands as one stack. Needs golem-pedals.js and its defs().
(function (root) {
  'use strict';
  const P = () => root.GolemPedals;
  let sceneCount = 0;

  // seeded random, so a lane always draws the same floor
  const rng = seed => () => (seed = (seed * 16807) % 2147483647) / 2147483647;

  // nest a pedal's <svg> at (x, y) with width w; returns {svg, h}
  function place(svg, x, y, w, idx) {
    const vb = /viewBox="([-\d.]+) ([-\d.]+) ([\d.]+) ([\d.]+)"/.exec(svg);
    const h = vb ? w * (+vb[4]) / (+vb[3]) : w;
    const out = svg.replace(/^<svg /, `<svg x="${x.toFixed(1)}" y="${y.toFixed(1)}" height="${h.toFixed(1)}" ${idx != null ? `data-idx="${idx}" ` : ''}`)
      .replace(/ width="[\d.]+"/, ` width="${w.toFixed(1)}"`);
    return { svg: out, h, vb: vb ? vb.slice(1).map(Number) : [0, 0, w, w] };
  }
  // a point given in the pedal's own viewBox -> scene coordinates
  const toScene = (pl, x, y, px, py) => [x + (px - pl.vb[0]) * (pl.h / pl.vb[3]), y + (py - pl.vb[1]) * (pl.h / pl.vb[3])];

  function familyOf(type) {
    const b = P().blockByType(type);
    return b ? b.family : 'generic';
  }

  // ---------- floors ----------
  function floorStage(id, W, H, R) {
    let planks = '';
    const ph = 34;
    for (let y = -ph / 2, row = 0; y < H; y += ph, row++) {
      let x = -((row * 137) % 260);
      while (x < W) {
        const len = 220 + R() * 260, t = R();
        const c = t < .33 ? '#2b221b' : t < .66 ? '#251d17' : '#30261e';
        planks += `<rect x="${x.toFixed(0)}" y="${y.toFixed(0)}" width="${len.toFixed(0)}" height="${ph - 1.5}" fill="${c}"/>`;
        x += len + 1.5;
      }
    }
    // tape marks where the player stands, and a set list taped to the floor
    const tapes = [[W * .18, H * .2, -8], [W * .71, H * .12, 6]].map(([x, y, r]) =>
      `<g transform="translate(${x} ${y}) rotate(${r})" opacity=".85"><rect x="-26" y="-4" width="52" height="8" fill="#e8e2c8" opacity=".75"/><rect x="-4" y="-26" width="8" height="52" fill="#e8e2c8" opacity=".75"/></g>`).join('');
    return `<defs>
  <filter id="${id}-grain" x="0" y="0" width="100%" height="100%"><feTurbulence type="fractalNoise" baseFrequency=".003 .28" numOctaves="2" seed="9"/>
    <feColorMatrix type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 .9 -.25"/></filter>
  <radialGradient id="${id}-spot" cx=".5" cy=".42" r=".62"><stop offset="0" stop-color="#ffe9c2" stop-opacity=".20"/><stop offset=".55" stop-color="#ffe9c2" stop-opacity=".05"/><stop offset="1" stop-color="#000" stop-opacity=".55"/></radialGradient>
</defs>
<rect width="${W}" height="${H}" fill="#120e0b"/>${planks}
<rect width="${W}" height="${H}" filter="url(#${id}-grain)"/>
${tapes}
<rect width="${W}" height="${H}" fill="url(#${id}-spot)"/>`;
  }

  function floorRug(id, W, H, R) {
    const rx = 26, ry = 22, rw = W - 52, rh = H - 44;
    let fringe = '';
    for (let y = ry + 4; y < ry + rh - 2; y += 5) {
      fringe += `<path d="M${rx} ${y}h-${8 + R() * 6}M${rx + rw} ${y}h${8 + R() * 6}"/>`;
    }
    return `<defs>
  <pattern id="${id}-field" width="56" height="56" patternUnits="userSpaceOnUse">
    <rect width="56" height="56" fill="#6e1e1a"/>
    <path d="M28 6 50 28 28 50 6 28z" fill="#8b2a22"/><path d="M28 14 42 28 28 42 14 28z" fill="#1f2a44"/>
    <path d="M28 21 35 28 28 35 21 28z" fill="#d9b779"/><circle cx="0" cy="0" r="5" fill="#d9b779" opacity=".8"/><circle cx="56" cy="0" r="5" fill="#d9b779" opacity=".8"/>
    <circle cx="0" cy="56" r="5" fill="#d9b779" opacity=".8"/><circle cx="56" cy="56" r="5" fill="#d9b779" opacity=".8"/></pattern>
  <pattern id="${id}-border" width="22" height="22" patternUnits="userSpaceOnUse">
    <rect width="22" height="22" fill="#1f2a44"/><path d="M0 11 11 0 22 11 11 22z" fill="none" stroke="#d9b779" stroke-width="2"/><circle cx="11" cy="11" r="2.5" fill="#b8402f"/></pattern>
  <filter id="${id}-weave" x="0" y="0" width="100%" height="100%"><feTurbulence type="fractalNoise" baseFrequency="1.3" numOctaves="2" seed="4"/>
    <feColorMatrix type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 1.1 -.42"/></filter>
  <radialGradient id="${id}-lamp" cx=".5" cy=".45" r=".7"><stop offset="0" stop-color="#ffd9a0" stop-opacity=".16"/><stop offset="1" stop-color="#000" stop-opacity=".55"/></radialGradient>
</defs>
<rect width="${W}" height="${H}" fill="#2a2420"/>
<g stroke="#d8cbb0" stroke-width="1.6" opacity=".8">${fringe}</g>
<rect x="${rx}" y="${ry}" width="${rw}" height="${rh}" fill="url(#${id}-border)"/>
<rect x="${rx + 22}" y="${ry + 22}" width="${rw - 44}" height="${rh - 44}" fill="#d9b779"/>
<rect x="${rx + 26}" y="${ry + 26}" width="${rw - 52}" height="${rh - 52}" fill="url(#${id}-field)"/>
<rect x="${rx}" y="${ry}" width="${rw}" height="${rh}" filter="url(#${id}-weave)"/>
<rect width="${W}" height="${H}" fill="url(#${id}-lamp)"/>`;
  }

  // ---------- cables ----------
  function plug(x, y, dir) {
    // a right-angle jack: barrel into the socket, boot toward the cable
    const s = dir; // +1: plug enters to the right (into a left-side input)
    return `<g><rect x="${s > 0 ? x - 11 : x + 1}" y="${y - 3.4}" width="10" height="6.8" rx="1.2" fill="url(#gp-chrome)"/>
      <rect x="${s > 0 ? x - 19 : x + 9}" y="${y - 5}" width="10" height="10" rx="2.5" fill="#1b1b1b"/>
      <rect x="${s > 0 ? x - 19 : x + 9}" y="${y - 5}" width="10" height="3" rx="1.5" fill="#fff" opacity=".1"/></g>`;
  }
  // returns [cable, plugs]: the plugs are drawn over the pedals' sides, the cable under the pedals
  function cable(id, x1, y1, x2, y2, sag, color) {
    // leaves the output to the right, drops to the floor, comes back up into the input
    const a = x1 + 18, b = x2 - 18, span = Math.abs(b - a);
    const k = Math.min(34, span * .25);
    const d = `M${a} ${y1} C${a + k} ${y1 + sag}, ${b - k} ${y2 + sag}, ${b} ${y2}`;
    return [`<g class="gs-cable">
  <path d="${d}" fill="none" stroke="#000" stroke-opacity=".55" stroke-width="7" transform="translate(3 6)" filter="url(#${id}-blur)"/>
  <path d="${d}" fill="none" stroke="${color || '#141414'}" stroke-width="5.5" stroke-linecap="round"/>
  <path d="${d}" fill="none" stroke="#fff" stroke-opacity=".16" stroke-width="1.3" transform="translate(-.8 -1.4)"/></g>`, plug(x1, y1, -1) + plug(x2, y2, 1)];
  }

  // ---------- the stage box (a lane with no amp ends in the snake) ----------
  function stageBox() {
    return `<svg viewBox="-4 -4 96 76" width="96" role="img" aria-label="stage box"><g filter="url(#gp-shadow)">
  <rect x="0" y="0" width="88" height="64" rx="5" fill="#2b2f33"/><rect x="0" y="0" width="88" height="64" rx="5" fill="url(#gp-brush-lines)"/>
  <rect x="0" y="0" width="88" height="64" rx="5" fill="url(#gp-sheen)"/></g>
  ${[0, 1, 2, 3].map(i => `<circle cx="${16 + i * 19}" cy="26" r="7" fill="#111"/><circle cx="${16 + i * 19}" cy="26" r="4.4" fill="#2a2a2a"/>
    <circle cx="${14.5 + i * 19}" cy="24.5" r=".8" fill="#888"/><circle cx="${17.5 + i * 19}" cy="24.5" r=".8" fill="#888"/><circle cx="${16 + i * 19}" cy="28" r=".8" fill="#888"/>`).join('')}
  <text x="44" y="52" text-anchor="middle" font-family="${P().FONT.nam}" font-size="9" font-weight="700" letter-spacing="1.5" fill="#c9ced3">FOH</text></svg>`;
  }

  // ---------- render ----------
  function render(chain, opts) {
    opts = opts || {};
    const id = 'gs' + (++sceneCount);
    const R = rng(opts.seed || 20261113);
    const items = (chain || []).map(c => (typeof c === 'string' ? { type: c } : Object.assign({}, c)));
    // group head + cab into one stack
    const groups = [];
    for (let i = 0; i < items.length; i++) {
      const it = items[i], fam = familyOf(it.type);
      if (fam === 'nam-head' && items[i + 1] && familyOf(items[i + 1].type) === 'nam-cab') {
        groups.push({ idx: i, kind: 'amp', item: Object.assign({}, it, { type: 'amp', size: items[i + 1].size || items[i + 1].title, cabTitle: items[i + 1].title }) });
        i++;
      } else {
        groups.push({ idx: i, kind: ['nam-head', 'nam-cab', 'nam-stack'].includes(fam) ? 'amp' : 'pedal', item: it });
      }
    }
    const hasAmp = groups.some(g => g.kind === 'amp');

    // the scene is as tall as its tallest amp needs (amps stand 70 px behind the pedal line)
    let ampH = 0;
    groups.filter(g => g.kind === 'amp').forEach(g => {
      const fam = familyOf(g.item.type);
      ampH = Math.max(ampH, place(P().pedalFor(g.item), 0, 0, fam === 'nam-cab' ? 200 : 250).h);
    });
    const H = opts.height || Math.max(330, ampH + 70 + 46 + 24), floorY = H - 46; // pedals stand on floorY
    const pedalW = 104, gap = 46, ampW = 250, margin = 84;
    let x = margin;
    const placed = [];
    groups.forEach(g => {
      const fam = familyOf(g.item.type);
      if (g.kind === 'amp') {
        const svg = P().pedalFor(g.item);
        const w = fam === 'nam-head' ? ampW : fam === 'nam-cab' ? ampW * .8 : ampW;
        const pl = place(svg, x, 0, w);
        const y = floorY - 70 - pl.h; // stands behind the pedal line
        const p2 = place(svg, x, y, w, g.idx);
        // the head's input jack: left of its panel (headBody puts it at x+22, panel y)
        const inPt = [x + w * .09, y + p2.h * (fam === 'nam-cab' ? .5 : (fam === 'nam-head' ? .72 : .36))];
        const outPt = [x + w * .96, inPt[1]];
        placed.push({ svg: p2.svg, inPt, outPt, x, w, amp: true });
        x += w + gap;
      } else {
        const svg = P().pedalFor(g.item);
        const isTreadle = /gp-treadle/.test(svg);
        const w = isTreadle ? pedalW * 1.08 : /viewBox="-6 -4 104/.test(svg) ? pedalW * .84 : pedalW;
        const pl = place(svg, x, 0, w);
        const y = floorY - pl.h;
        const p2 = place(svg, x, y, w, g.idx);
        const jy = isTreadle ? 150 : 40;
        const inPt = toScene(p2, x, y, -4, jy), outPt = toScene(p2, x, y, p2.vb[0] + p2.vb[2] - 2, jy);
        placed.push({ svg: p2.svg, inPt, outPt, x, w });
        x += w + gap;
      }
    });
    // the end: a stage box when the lane has no amp
    if (!hasAmp) {
      const pl = place(stageBox(), x + 10, floorY - 80, 96);
      placed.push({ svg: pl.svg, inPt: [x + 12, floorY - 80 + pl.h * .4], outPt: null, x, w: 96, end: true });
      x += 120;
    }
    const W = Math.max(opts.minWidth || 0, x + margin - gap);

    let cables = '', plugs = '';
    // the guitar cable, from off stage
    if (placed.length && opts.guitar !== false) {
      const [ix, iy] = placed[0].inPt;
      const d = `M-10 ${floorY + 30} C${ix * .3} ${floorY + 40}, ${ix - 60} ${iy + 70}, ${ix - 18} ${iy}`;
      cables += `<g class="gs-cable"><path d="${d}" fill="none" stroke="#000" stroke-opacity=".5" stroke-width="8" transform="translate(3 6)" filter="url(#${id}-blur)"/>
        <path d="${d}" fill="none" stroke="#1d1d1d" stroke-width="6.5"/><path d="${d}" fill="none" stroke="#fff" stroke-opacity=".14" stroke-width="1.4" transform="translate(-.8 -1.4)"/></g>`;
      plugs += plug(ix, iy, 1);
    }
    for (let i = 0; i < placed.length - 1; i++) {
      const a = placed[i], b = placed[i + 1];
      if (!a.outPt) continue;
      const toAmp = b.amp || a.amp;
      const [c, pl] = cable(id, a.outPt[0], a.outPt[1], b.inPt[0], b.inPt[1], toAmp ? 120 : 26 + R() * 14, toAmp ? '#191919' : null);
      cables += c; plugs += pl;
    }
    // the last effect after an amp goes off to front of house
    const last = placed[placed.length - 1];
    if (hasAmp && last && last.outPt) {
      const [ox, oy] = last.outPt;
      cables += `<g class="gs-cable"><path d="M${ox + 18} ${oy} C${ox + 70} ${oy + 40}, ${W - 40} ${floorY + 20}, ${W + 10} ${floorY + 34}" fill="none" stroke="#1d1d1d" stroke-width="6"/></g>`;
      plugs += plug(ox, oy, -1);
    }

    const floor = opts.floor === 'rug' ? floorRug(id, W, H, R) : floorStage(id, W, H, R);
    return `<svg class="gs-scene gs-${opts.floor === 'rug' ? 'rug' : 'stage'}" viewBox="0 0 ${W.toFixed(0)} ${H}" width="${opts.width || W.toFixed(0)}" role="img" aria-label="${(chain || []).length} blocks on a ${opts.floor === 'rug' ? 'rug' : 'stage floor'}">
<defs><filter id="${id}-blur" x="-10%" y="-30%" width="120%" height="160%"><feGaussianBlur stdDeviation="2.4"/></filter></defs>
${floor}
${placed.filter(p => p.amp).map(p => p.svg).join('\n')}
${cables}
${placed.filter(p => !p.amp).map(p => p.svg).join('\n')}
${plugs}
</svg>`;
  }

  root.GolemStage = { render, stageBox };
})(typeof window !== 'undefined' ? window : globalThis);
