// SPDX-License-Identifier: GPL-3.0-or-later
// MOCK-UP (2026-11-17): a rig cut into sections, each in its natural view (the way amp-sim plugins do).
//   GolemSections.split(chain)                      -> [{key: 'pre' | 'outboard' | 'amp' | 'post', label, items}]
//   GolemSections.render(section, {floor, W, H})    -> one section: pedals from above, amps and rack from the front
//   GolemSections.card(sections, {floor})           -> the fixed image for a card: the sections as slanted bands
// Needs golem-pedals.js, golem-stage.js and board.js (its floors, board and cables).
(function (root) {
  'use strict';
  const P = () => root.GolemPedals, B = () => root.GolemBoard._parts;
  let count = 0;
  const f = n => n.toFixed(1);
  const isOutboard = it => /^(preamp|comp)-nam$/.test(it.type);

  function split(chain) {
    const { familyOf, isAmp } = B();
    const items = (chain || []).map(c => (typeof c === 'string' ? { type: c } : c));
    const stackOf = list => {
      const out = [];
      for (let i = 0; i < list.length; i++) {
        const it = list[i], nx = list[i + 1];
        if (familyOf(it.type) === 'nam-head' && nx && (nx.type === 'cab' || familyOf(nx.type) === 'nam-cab')) { out.push(Object.assign({}, it, { type: 'amp', family: it.family || nx.family, size: nx.size || nx.title })); i++; }
        else out.push(it);
      }
      return out;
    };
    const flat = stackOf(items), k = flat.findIndex(it => it.split || isAmp(it));
    const before = k < 0 ? flat : flat.slice(0, k), after = k < 0 ? [] : flat.slice(k + 1), stage = k < 0 ? null : flat[k];
    const amps = !stage ? [] : stage.split
      ? stage.split.branches.map((br, b) => { const a = stackOf(Array.isArray(br) ? br : br.items).find(isAmp); return a && Object.assign({}, a, { branch: String.fromCharCode(65 + b) }, br.mute ? { on: false } : {}); }).filter(Boolean)
      : [stage];
    const ob = before.concat(after).filter(isOutboard), obAfter = after.some(isOutboard);
    const pre = before.filter(it => !isOutboard(it)), post = after.filter(it => !isOutboard(it));
    const sec = [];
    if (pre.length) sec.push({ key: 'pre', label: amps.length ? 'Pedals · pre' : 'Pedals', items: pre });
    if (ob.length && !obAfter) sec.push({ key: 'outboard', label: 'Outboard', items: ob });
    if (amps.length) sec.push({ key: 'amp', label: amps.length > 1 ? 'Heads / Cabs' : 'Head / Cab', items: amps });
    if (ob.length && obAfter) sec.push({ key: 'outboard', label: 'Outboard', items: ob });
    if (post.length) sec.push({ key: 'post', label: 'Pedals · post', items: post });
    return sec;
  }

  // ---------- pedals, from above: as large as the frame allows, wrapped into rows ----------
  function pedals(id, sec, W, H, o) {
    const { place, board, wire, plug, PRE, POST } = B(), col = sec.key === 'post' ? POST : PRE;
    const MX = Math.min(150, W * .13), MY = Math.min(110, H * .12), PADX = Math.min(70, W * .07), PADY = 22, RG = 30;
    // every unit at the same scale: PW is the px width of a compact stomp, the others follow their real footprint
    const all = sec.items.map(it => { const svg = P().pedalFor(it), fp = P().footprint(svg); return { it, svg, uw: fp.w / 124, uh: fp.h / 124 }; });
    const lim = W - MX * 2 - PADX * 2, N = all.length, total = all.reduce((t, n) => t + n.uw, 0);
    let rows, PW = 0;
    for (let rn = 1; rn <= Math.min(4, N); rn++) {    // rows of even width; keep the count that gives the largest pedals
      const chunks = [[]];
      let acc = 0;
      all.forEach(n => { if (chunks[chunks.length - 1].length && chunks.length < rn && acc + n.uw / 2 > total * chunks.length / rn) chunks.push([]); chunks[chunks.length - 1].push(n); acc += n.uw; });
      const wide = Math.max(...chunks.map(c => c.reduce((t, n) => t + n.uw, 0) + .36 * (c.length - 1)));
      const tall = chunks.reduce((t, c) => t + Math.max(...c.map(n => n.uh)), 0);
      const pw = Math.min(300, lim / wide, (H - MY * 2 - PADY * 2 - (chunks.length - 1) * RG) / tall);
      if (pw > PW * 1.04) { PW = pw; rows = chunks; }
    }
    const GAP = PW * .36;
    rows.forEach(r => r.forEach(n => { n.w = PW * n.uw; n.h = PW * n.uh; }));
    const rowHs = rows.map(r => Math.max(...r.map(n => n.h))), rowW = r => r.reduce((s, n) => s + n.w, 0) + GAP * (r.length - 1);
    const bw = Math.max(...rows.map(rowW)) + PADX * 2, bh = rowHs.reduce((t, h) => t + h, 0) + (rows.length - 1) * RG + PADY * 2;
    const bx = (W - bw) / 2, by = (H - bh) / 2;
    let draw = '', under = '', plugs = '', prev = null;
    const sw = Math.max(4, PW * .05);
    rows.forEach((r, ri) => {
      let x = bx + (bw - rowW(r)) / 2;
      const rowH = rowHs[ri], y0 = by + PADY + rowHs.slice(0, ri).reduce((t, h) => t + h + RG, 0);
      r.forEach(n => {
        const pl = place(n.svg, x, y0 + rowH - n.h, n.w), sc = pl.h / pl.vb[3];
        const jy = pl.y + (/gp-hw/.test(n.svg) ? pl.h * .62 : (40 - pl.vb[1]) * sc);
        n.in = [x + 2, jy]; n.out = [x + n.w - 2, jy];
        draw += n.it.on === false ? `<g opacity=".78">${pl.svg}</g>` : pl.svg;
        if (prev) {
          const a = prev.out, b = n.in;
          under += b[0] > a[0] ? wire(id, [[a[0] + 16, a[1]], [(a[0] + b[0]) / 2, Math.max(a[1], b[1]) + PW * .26], [b[0] - 16, b[1]]], col, sw)
            : wire(id, [[a[0] + 16, a[1]], [a[0] + 60, y0 - RG * .6], [(a[0] + b[0]) / 2, y0 - RG / 2], [b[0] - 60, y0 - RG * .4], [b[0] - 16, b[1]]], col, sw);
          plugs += plug(a[0], a[1], -1) + plug(b[0], b[1], 1);
        }
        prev = n; x += n.w + GAP;
      });
    });
    const first = rows[0][0], p = first.in, q = prev.out;
    under += wire(id, [[-20, p[1] + H * .2], [bx - MX * .7, p[1] + H * .16], [bx - 30, p[1] + 50], [p[0] - 16, p[1]]], col, sw + 1)
      + wire(id, [[q[0] + 16, q[1]], [bx + bw + 30, q[1] + 40], [bx + bw + MX * .7, q[1] + H * .14], [W + 20, q[1] + H * .18]], col, sw + 1);
    plugs += plug(p[0], p[1], 1) + plug(q[0], q[1], -1);
    return `${o.floor === 'concrete' ? B().concrete(id, W, H, o.R) : B().planks(id, W, H, o.R)}${board(id, bx, by, bw, bh)}${under}${draw}${plugs}`;
  }

  // ---------- the back wall and the floor's front edge, for what stands ----------
  function room(id, W, H, o, floorLine) {
    return `${B().wall(id, W, floorLine, o.floor)}<rect y="${f(floorLine)}" width="${W}" height="${f(H - floorLine)}" fill="${o.floor === 'concrete' ? '#1b1c1e' : '#140f0c'}"/>
      <rect y="${f(floorLine)}" width="${W}" height="3" fill="#fff" opacity=".1"/>`;
  }
  function amps(id, sec, W, H, o) {
    const { place, tape } = B(), n = sec.items.length, FL = H * .86, AG = Math.min(70, W * .06);
    const svgs = sec.items.map(a => P().pedalFor(a)), ratio = Math.max(...svgs.map(s => { const pl = place(s, 0, 0, 100); return pl.h / 100; }));
    const AW = Math.min((W - W * .16 - AG * (n - 1)) / n, (FL - H * .07) / ratio), tot = n * AW + (n - 1) * AG;
    let s = room(id, W, H, o, FL);
    sec.items.forEach((a, i) => {
      const x = (W - tot) / 2 + i * (AW + AG), pl = place(svgs[i], x, 0, AW), y = FL + AW * .06 - pl.h;
      s += `<ellipse cx="${f(x + AW / 2)}" cy="${f(FL + AW * .05)}" rx="${f(AW * .58)}" ry="${f(AW * .07)}" fill="#000" opacity=".75" filter="url(#${id}-ash)"/>
        ${a.on === false ? `<g opacity=".45">${place(svgs[i], x, y, AW).svg}</g>` : place(svgs[i], x, y, AW).svg}`;
      if (a.branch && n > 1) s += `<g transform="translate(${f(x + AW / 2)} ${f(FL + (H - FL) * .6)}) scale(${f(Math.max(1.2, H / 420))})">${tape(0, 0, a.branch + (a.on === false ? ' · MUTED' : ''), -2)}</g>`;
    });
    return s;
  }
  function outboard(id, sec, W, H, o) {
    const { place } = B(), FL = H * .86;
    const mk = it => P().namRack({ label: it.label, title: it.title, on: it.on !== false });
    let UW = Math.min(W - W * .2, 860);
    const hs = w => sec.items.reduce((t, it) => t + place(mk(it), 0, 0, w).h - 2, 0);
    if (hs(UW) > FL - H * .2) UW *= (FL - H * .2) / hs(UW);
    const pad = UW * .045, ch = hs(UW) + pad * 2.4, cw = UW + pad * 2, x = (W - cw) / 2, y = FL + 10 - ch;
    let yy = y + pad * 1.2, units = '';
    sec.items.forEach(it => { const pl = place(mk(it), x + pad, yy, UW); units += pl.svg; yy += pl.h - 2; });
    return `${room(id, W, H, o, FL)}<ellipse cx="${f(W / 2)}" cy="${f(FL + 10)}" rx="${f(cw * .56)}" ry="${f(cw * .035)}" fill="#000" opacity=".75" filter="url(#${id}-ash)"/>
      <rect x="${f(x)}" y="${f(y)}" width="${f(cw)}" height="${f(ch)}" rx="8" fill="#17181a"/><rect x="${f(x)}" y="${f(y)}" width="${f(cw)}" height="${f(ch)}" rx="8" fill="url(#gp-tolex)" opacity=".7"/>
      <rect x="${f(x + 3)}" y="${f(y + 3)}" width="${f(cw - 6)}" height="${f(ch - 6)}" rx="6" fill="none" stroke="url(#gp-chrome)" stroke-width="${f(pad * .22)}"/>
      <rect x="${f(x + pad * .8)}" y="${f(y + pad)}" width="${f(cw - pad * 1.6)}" height="${f(ch - pad * 2)}" fill="#050505"/>${units}
      ${[[0, 0], [1, 0], [0, 1], [1, 1]].map(([cx, cy]) => `<circle cx="${f(x + pad * .5 + cx * (cw - pad))}" cy="${f(y + pad * .5 + cy * (ch - pad))}" r="${f(pad * .42)}" fill="url(#gp-chrome)"/>`).join('')}`;
  }

  function inner(id, sec, W, H, o) {
    return sec.key === 'amp' ? amps(id, sec, W, H, o) : sec.key === 'outboard' ? outboard(id, sec, W, H, o) : pedals(id, sec, W, H, o);
  }
  const defs = (id, cy) => `<defs><filter id="${id}-blur" x="-10%" y="-30%" width="120%" height="160%"><feGaussianBlur stdDeviation="2.4"/></filter>
    <filter id="${id}-ash" x="-30%" y="-60%" width="160%" height="220%"><feGaussianBlur stdDeviation="9"/></filter>
    <radialGradient id="${id}-spot" cx=".5" cy="${cy}" r=".75"><stop offset="0" stop-color="#ffe9c2" stop-opacity=".14"/><stop offset=".6" stop-color="#ffe9c2" stop-opacity=".03"/><stop offset="1" stop-color="#000" stop-opacity=".6"/></radialGradient></defs>`;

  function render(sec, opts) {
    opts = opts || {};
    const id = 'gsx' + (++count), W = opts.W || 1600, H = opts.H || 900, o = { floor: opts.floor, R: B().rng(opts.seed || 20261117) };
    return `<svg class="gb-scene" viewBox="0 0 ${W} ${H}" role="img" aria-label="${sec.label}">${defs(id, .55)}${inner(id, sec, W, H, o)}
      <rect width="${W}" height="${H}" fill="url(#${id}-spot)" pointer-events="none"/></svg>`;
  }

  // the card: every section as a slanted band of one 16:9 image; the amp band is the widest
  function card(sections, opts) {
    opts = opts || {};
    const W = 1600, H = 900, SL = sections.length > 1 ? 46 : 0, cid = 'gsc' + (++count);
    const wt = s => (s.key === 'amp' ? 1.35 : s.key === 'outboard' ? .95 : Math.min(1.25, .7 + s.items.length * .11));
    const tw = sections.reduce((t, s) => t + wt(s), 0);
    let x = 0, out = '', lines = '';
    sections.forEach((s, i) => {
      const w = W * wt(s) / tw, id = 'gsx' + (++count), bwd = w + SL * 2, o = { floor: opts.floor, R: B().rng(20261117 + i) };
      const x0 = i ? x + SL : 0, x1 = i < sections.length - 1 ? x + w + SL : W, x2 = i < sections.length - 1 ? x + w - SL : W, x3 = i ? x - SL : 0;
      out += `<clipPath id="${cid}-c${i}"><polygon points="${f(x0)},0 ${f(x1)},0 ${f(x2)},${H} ${f(x3)},${H}"/></clipPath>
        <g clip-path="url(#${cid}-c${i})"><svg x="${f(x - SL)}" y="0" width="${f(bwd)}" height="${H}" viewBox="0 0 ${f(bwd)} ${H}">${defs(id, .55)}${inner(id, s, bwd, H, o)}
          <rect width="${f(bwd)}" height="${H}" fill="url(#${id}-spot)"/></svg></g>`;
      if (i) lines += `<path d="M${f(x + SL)} 0L${f(x - SL)} ${H}" stroke="#000" stroke-opacity=".7" stroke-width="16"/><path d="M${f(x + SL)} 0L${f(x - SL)} ${H}" stroke="#ffd23c" stroke-width="5"/>`;
      x += w;
    });
    return `<svg class="gb-scene" viewBox="0 0 ${W} ${H}" role="img" aria-label="rig">${out}${lines}</svg>`;
  }

  root.GolemSections = { split, render, card };
})(typeof window !== 'undefined' ? window : globalThis);
