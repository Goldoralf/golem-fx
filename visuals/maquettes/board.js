// SPDX-License-Identifier: GPL-3.0-or-later
// MOCK-UP (2026-11-17): a rig as a pedalboard seen from above, at a fixed 16:9 ratio.
//   GolemBoard.render(chain, {amp: 'backline' | 'top', floor: 'planks' | 'concrete'})
// chain: GolemStage.render()'s items. Front row = before the amp, back row = after the amp.
// 'backline': the amps stand against the back wall, seen from the front. 'top': everything from above.
// Limits of the mock-up: a split's branches hold amps only; rows do not wrap.
(function (root) {
  'use strict';
  const P = () => root.GolemPedals, S = () => root.GolemStage;
  let count = 0;
  const rng = seed => () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  const f = n => n.toFixed(1);
  const PRE = '#c9a24b', POST = '#3d8f96';           // cable colour = which side of the amp
  const shade = (hex, k) => '#' + [1, 3, 5].map(i => Math.max(0, Math.min(255, Math.round(parseInt(hex.slice(i, i + 2), 16) * k))).toString(16).padStart(2, '0')).join('');

  function place(svg, x, y, w) {
    const vb = /viewBox="([-\d.]+) ([-\d.]+) ([\d.]+) ([\d.]+)"/.exec(svg).slice(1).map(Number);
    const h = w * vb[3] / vb[2];
    return { svg: svg.replace(/^<svg /, `<svg x="${f(x)}" y="${f(y)}" height="${f(h)}" `).replace(/ width="[\d.]+"/, ` width="${f(w)}"`), h, vb, x, y, w };
  }
  const familyOf = t => (P().blockByType(t) || {}).family || 'generic';
  const isAmp = it => ['nam-head', 'nam-cab', 'nam-stack'].includes(familyOf(it.type));

  // ---------- floors and walls ----------
  function planks(id, W, H, R) {
    let s = '';
    const ph = 46;
    for (let y = -ph / 2, row = 0; y < H; y += ph, row++) {
      let x = -((row * 137) % 260);
      while (x < W) {
        const len = 260 + R() * 320, t = R();
        s += `<rect x="${x.toFixed(0)}" y="${y.toFixed(0)}" width="${len.toFixed(0)}" height="${ph - 1.5}" fill="${t < .33 ? '#2b221b' : t < .66 ? '#251d17' : '#30261e'}"/>`;
        x += len + 1.5;
      }
    }
    return `<defs><filter id="${id}-grain" x="0" y="0" width="100%" height="100%"><feTurbulence type="fractalNoise" baseFrequency=".003 .22" numOctaves="2" seed="9"/>
      <feColorMatrix type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 .9 -.25"/></filter></defs>
    <rect width="${W}" height="${H}" fill="#120e0b"/>${s}<rect width="${W}" height="${H}" filter="url(#${id}-grain)"/>`;
  }
  function concrete(id, W, H, R) {
    let s = '';
    for (let i = 0; i < 7; i++) {              // hairline cracks and a few stains
      let x = R() * W, y = R() * H, d = `M${f(x)} ${f(y)}`;
      for (let k = 0; k < 6; k++) { x += (R() - .5) * 150; y += (R() - .3) * 70; d += `L${f(x)} ${f(y)}`; }
      s += `<path d="${d}" fill="none" stroke="#000" stroke-opacity=".28" stroke-width="1.2"/>`;
    }
    for (let i = 0; i < 5; i++) s += `<ellipse cx="${f(R() * W)}" cy="${f(R() * H)}" rx="${f(60 + R() * 120)}" ry="${f(30 + R() * 70)}" fill="#000" opacity=".10"/>`;
    return `<defs><filter id="${id}-grain" x="0" y="0" width="100%" height="100%"><feTurbulence type="fractalNoise" baseFrequency=".55" numOctaves="3" seed="3"/>
      <feColorMatrix type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 .7 -.22"/></filter>
      <filter id="${id}-cloud" x="0" y="0" width="100%" height="100%"><feTurbulence type="fractalNoise" baseFrequency=".006" numOctaves="2" seed="11"/>
      <feColorMatrix type="matrix" values="0 0 0 0 1  0 0 0 0 1  0 0 0 0 1  0 0 0 .5 -.2"/></filter></defs>
    <rect width="${W}" height="${H}" fill="#3b3d3f"/><rect width="${W}" height="${H}" filter="url(#${id}-cloud)" opacity=".5"/>
    ${s}<rect width="${W}" height="${H}" filter="url(#${id}-grain)"/>
    <path d="M0 ${H * .63}H${W}M${W * .37} 0V${H}" stroke="#000" stroke-opacity=".35" stroke-width="3"/>`;
  }
  // the back wall, seen from the front (backline variant): a stage curtain, or painted blocks
  function wall(id, W, wy, kind) {
    let s;
    if (kind === 'concrete') {
      let b = '';
      for (let y = 0, r = 0; y < wy; y += 44, r++) for (let x = -(r % 2) * 60; x < W; x += 120) b += `<rect x="${x + 1}" y="${y + 1}" width="118" height="42" fill="#2a2c2f"/>`;
      s = `<rect width="${W}" height="${wy}" fill="#17181a"/>${b}`;
    } else {
      s = `<defs><linearGradient id="${id}-fold" x1="0" x2="1" y1="0" y2="0"><stop offset="0" stop-color="#1c0d0f"/><stop offset=".35" stop-color="#4a1a1e"/><stop offset=".6" stop-color="#2a1114"/><stop offset="1" stop-color="#14090a"/></linearGradient>
        <pattern id="${id}-curtain" width="74" height="10" patternUnits="userSpaceOnUse"><rect width="74" height="10" fill="url(#${id}-fold)"/></pattern></defs>
        <rect width="${W}" height="${wy}" fill="url(#${id}-curtain)"/>`;
    }
    return `${s}<defs><linearGradient id="${id}-wsh" x1="0" x2="0" y1="0" y2="1"><stop offset="0" stop-color="#000" stop-opacity=".72"/><stop offset=".7" stop-color="#000" stop-opacity=".25"/><stop offset="1" stop-color="#000" stop-opacity=".5"/></linearGradient>
      <linearGradient id="${id}-fsh" x1="0" x2="0" y1="0" y2="1"><stop offset="0" stop-color="#000" stop-opacity=".6"/><stop offset="1" stop-color="#000" stop-opacity="0"/></linearGradient></defs>
      <rect width="${W}" height="${wy}" fill="url(#${id}-wsh)"/>
      <rect y="${wy - 9}" width="${W}" height="9" fill="#0c0b0a"/><rect y="${wy - 9}" width="${W}" height="1.5" fill="#fff" opacity=".08"/>
      <rect y="${wy}" width="${W}" height="70" fill="url(#${id}-fsh)"/>`;
  }

  // ---------- the board, from above: an aluminium frame of rails ----------
  function board(id, x, y, w, h) {
    const n = 6, gap = 13, rh = (h - gap * (n - 1)) / n;
    let rails = '';
    for (let i = 0; i < n; i++) {
      const ry = y + i * (rh + gap);
      rails += `<rect x="${f(x)}" y="${f(ry)}" width="${f(w)}" height="${f(rh)}" rx="5" fill="url(#${id}-rail)"/>
        <rect x="${f(x)}" y="${f(ry)}" width="${f(w)}" height="${f(rh)}" rx="5" fill="url(#gp-brush-lines)" opacity=".5"/>
        <rect x="${f(x + 4)}" y="${f(ry + 1)}" width="${f(w - 8)}" height="1.4" fill="#fff" opacity=".13"/>`;
    }
    const cap = cx => `<rect x="${f(cx)}" y="${f(y - 5)}" width="26" height="${f(h + 10)}" rx="7" fill="#1b1d20"/><rect x="${f(cx + 3)}" y="${f(y - 2)}" width="2" height="${f(h + 4)}" fill="#fff" opacity=".08"/>`;
    return `<defs><linearGradient id="${id}-rail" x1="0" x2="0" y1="0" y2="1"><stop offset="0" stop-color="#3d4146"/><stop offset="1" stop-color="#25282c"/></linearGradient>
      <filter id="${id}-bsh" x="-10%" y="-10%" width="120%" height="130%"><feGaussianBlur stdDeviation="12"/></filter></defs>
      <rect x="${f(x - 6)}" y="${f(y + 10)}" width="${f(w + 12)}" height="${f(h + 14)}" rx="14" fill="#000" opacity=".62" filter="url(#${id}-bsh)"/>
      <rect x="${f(x)}" y="${f(y)}" width="${f(w)}" height="${f(h)}" rx="8" fill="#060606" opacity=".8"/>
      ${rails}${cap(x - 8)}${cap(x + w - 18)}`;
  }
  const tape = (x, y, txt, rot) => `<g transform="translate(${f(x)} ${f(y)}) rotate(${rot})"><rect x="${-txt.length * 4.6 - 9}" y="-10" width="${txt.length * 9.2 + 18}" height="20" rx="1.5" fill="#e8e2c8" opacity=".9"/>
    <text y="5" text-anchor="middle" font-family="${P().FONT.nam}" font-size="14" font-weight="800" letter-spacing="1.4" fill="#2a2018">${txt}</text></g>`;

  // ---------- cables ----------
  function plug(x, y, dir) {
    return `<g><rect x="${dir > 0 ? x - 11 : x + 1}" y="${y - 3.4}" width="10" height="6.8" rx="1.2" fill="url(#gp-chrome)"/>
      <rect x="${dir > 0 ? x - 19 : x + 9}" y="${y - 5}" width="10" height="10" rx="2.5" fill="#1b1b1b"/></g>`;
  }
  // a smooth cable through way points (Catmull-Rom as cubic Béziers)
  function wire(id, pts, color, wdt) {
    let d = `M${f(pts[0][0])} ${f(pts[0][1])}`;
    for (let i = 0; i < pts.length - 1; i++) {
      const p0 = pts[i - 1] || pts[i], p1 = pts[i], p2 = pts[i + 1], p3 = pts[i + 2] || p2, t = .2;
      d += `C${f(p1[0] + (p2[0] - p0[0]) * t)} ${f(p1[1] + (p2[1] - p0[1]) * t)},${f(p2[0] - (p3[0] - p1[0]) * t)} ${f(p2[1] - (p3[1] - p1[1]) * t)},${f(p2[0])} ${f(p2[1])}`;
    }
    wdt = wdt || 5.5;
    return `<g fill="none" stroke-linecap="round"><path d="${d}" stroke="#000" stroke-opacity=".5" stroke-width="${wdt + 2}" transform="translate(3 6)" filter="url(#${id}-blur)"/>
      <path d="${d}" stroke="${shade(color, .55)}" stroke-width="${wdt}"/><path d="${d}" stroke="${color}" stroke-width="${wdt - 2.2}" stroke-dasharray="5 3"/>
      <path d="${d}" stroke="#fff" stroke-opacity=".16" stroke-width="1.2" transform="translate(-.8 -1.4)"/></g>`;
  }

  // ---------- an amp from above (variant 'top'): the head on its cab, a strip of tape with its name ----------
  const TOLEX = { clean: '#cfc3a4', crunch: '#4a1a18', 'high-gain': '#1b1b1e', bass: '#1c2740' };
  const PANEL = { clean: '#b9bcc0', crunch: '#c9a54e', 'high-gain': '#8d949b', bass: '#aab0b6' };
  const CLOTH = { clean: '#b99b62', crunch: '#3a342c', 'high-gain': '#2a2a2c', bass: '#2c3240' };
  const LAMP = { clean: '#ff5340', crunch: '#ff5340', 'high-gain': '#ff3a2a', bass: '#ffb340' };
  function ampTop(id, it, x, y, w) {
    const fam = TOLEX[it.family] ? it.family : 'high-gain', on = it.on !== false;
    const d = w * .47, hw = w * .9, hd = d * .62, hx = x + (w - hw) / 2, hy = y + d - hd - 8;
    const tol = TOLEX[fam], dark = fam !== 'clean';
    const box = (bx, by, bw, bh, r) => `<rect x="${f(bx)}" y="${f(by)}" width="${f(bw)}" height="${f(bh)}" rx="${r}" fill="${tol}"/>
      <rect x="${f(bx)}" y="${f(by)}" width="${f(bw)}" height="${f(bh)}" rx="${r}" fill="url(#gp-tolex)" opacity=".8"/>
      <rect x="${f(bx + 3)}" y="${f(by + 3)}" width="${f(bw - 6)}" height="${f(bh - 6)}" rx="${r - 2}" fill="none" stroke="${dark ? '#fff' : '#000'}" stroke-opacity=".14" stroke-width="1.5"/>
      ${[[0, 0], [1, 0], [0, 1], [1, 1]].map(([cx, cy]) => `<rect x="${f(bx + cx * (bw - 15))}" y="${f(by + cy * (bh - 15))}" width="15" height="15" rx="4" fill="url(#gp-chrome)" opacity=".85"/>`).join('')}`;
    let knobs = '';
    for (let i = 0; i < 8; i++) knobs += `<rect x="${f(hx + hw * .2 + i * hw * .075)}" y="${f(hy + hd + 6)}" width="${f(hw * .04)}" height="6" rx="1.5" fill="#111"/>`;
    return `<g class="gp-amp" opacity="${on ? 1 : .55}">
      <rect x="${f(x - 4)}" y="${f(y + 6)}" width="${f(w + 8)}" height="${f(d + 12)}" rx="12" fill="#000" opacity=".6" filter="url(#${id}-ash)"/>
      ${on ? `<ellipse cx="${f(hx + hw * .88)}" cy="${f(y + d + 22)}" rx="46" ry="20" fill="${LAMP[fam]}" opacity=".3" filter="url(#${id}-ash)"/>` : ''}
      <rect x="${f(x + 8)}" y="${f(y + d - 3)}" width="${f(w - 16)}" height="8" rx="2" fill="${CLOTH[fam]}"/>
      ${box(x, y, w, d, 9)}
      <rect x="${f(hx - 2)}" y="${f(hy + 4)}" width="${f(hw + 4)}" height="${f(hd + 6)}" rx="9" fill="#000" opacity=".45" filter="url(#${id}-blur)"/>
      <rect x="${f(hx + 10)}" y="${f(hy + hd - 2)}" width="${f(hw - 20)}" height="9" rx="2" fill="${PANEL[fam]}"/>${knobs}
      <circle cx="${f(hx + hw * .88)}" cy="${f(hy + hd + 8)}" r="3.4" fill="${on ? LAMP[fam] : '#3a1512'}"${on ? ' filter="url(#gp-glow-soft)"' : ''}/>
      ${box(hx, hy, hw, hd, 8)}
      <rect x="${f(hx + hw * .2)}" y="${f(hy + 9)}" width="${f(hw * .6)}" height="13" rx="2" fill="#0b0b0b"/>
      ${Array.from({ length: 22 }, (_, i) => `<rect x="${f(hx + hw * .2 + 4 + i * (hw * .6 - 8) / 22)}" y="${f(hy + 11)}" width="${f((hw * .6 - 8) / 44)}" height="9" fill="#3a3a3a"/>`).join('')}
      <rect x="${f(hx + hw * .33)}" y="${f(hy + hd * .52)}" width="${f(hw * .34)}" height="13" rx="6" fill="#0d0d0d"/>
      <rect x="${f(hx + hw * .33 - 5)}" y="${f(hy + hd * .52 - 2)}" width="13" height="17" rx="3" fill="url(#gp-chrome)"/><rect x="${f(hx + hw * .67 - 8)}" y="${f(hy + hd * .52 - 2)}" width="13" height="17" rx="3" fill="url(#gp-chrome)"/>
      ${it.title ? tape(hx + hw * .5, hy + hd * .52 + 34, String(it.title).toUpperCase(), -2) : ''}</g>`;
  }

  // ---------- render ----------
  // modes: 'backline' (A), 'top' (B), 'tilt' (C: one high-angle view, ground squashed by SY, upright faces
  // by SZ), 'diptych' (D: the board from above on the left, a bolt, amps and rack from the front on the right)
  function render(chain, opts) {
    opts = opts || {};
    const id = 'gb' + (++count), W = 1600, H = 900, R = rng(opts.seed || 20261117);
    const mode = ['top', 'tilt', 'diptych'].includes(opts.amp) ? opts.amp : 'backline';
    const back = mode === 'backline', tilt = mode === 'tilt', dip = mode === 'diptych';
    const SY = tilt ? .77 : 1, SZ = .64, HP = H / SY;
    const floorKind = opts.floor === 'concrete' ? 'concrete' : 'planks';
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
    const flat = stackOf(items);
    const k = flat.findIndex(it => it.split || isAmp(it));
    let pre = k < 0 ? flat : flat.slice(0, k), post = k < 0 ? [] : flat.slice(k + 1);
    const stage = k < 0 ? null : flat[k];
    const amps = !stage ? [] : stage.split
      ? stage.split.branches.map(br => { const a = stackOf(Array.isArray(br) ? br : br.items).find(isAmp); return a && Object.assign({}, a, br.mute ? { on: false } : {}); }).filter(Boolean)
      : [Object.assign({}, stage)];
    const isSplit = !!(stage && stage.split) && amps.length > 1;
    // diptych: outboard captures leave the board for the rack
    const isOutboard = it => /^(preamp|comp)-nam$/.test(it.type);
    const rack = dip ? pre.concat(post).filter(isOutboard) : [];
    if (dip) { pre = pre.filter(it => !isOutboard(it)); post = post.filter(it => !isOutboard(it)); }

    const CX = dip ? 480 : W / 2;
    const wallY = back ? 352 : 0, ampBand = !amps.length || dip ? 0 : back ? wallY : tilt ? 470 : 250;
    const nRows = post.length ? 2 : 1, PADX = 74, PADY = 20, ROWGAP = 26;
    const PW = Math.max(90, Math.min(dip ? 104 : 128, ((HP - ampBand - 74 - PADY * 2 - ROWGAP * (nRows - 1)) / nRows) / 1.8)), GAP = PW * .38;
    const measure = it => {
      const svg = P().pedalFor(it);
      let w = /gp-hw-pitch-treadle|gp-wide/.test(svg) ? PW * 1.5 : /gp-treadle/.test(svg) ? PW * 1.08 : PW, h = place(svg, 0, 0, w).h;
      if (h > PW * 1.8) { w *= PW * 1.8 / h; h = PW * 1.8; }
      return { it, svg, w, h };
    };
    const box = (kind, o) => { const b = S().splitBox(kind, amps.length, o); const vb = /viewBox="([-\d.]+) ([-\d.]+) ([\d.]+) ([\d.]+)"/.exec(b.svg).slice(1).map(Number); return { box: b, svg: b.svg, w: vb[2], h: vb[3], vb }; };
    const front = pre.map(measure), rear = post.map(measure);
    if (isSplit) { front.push(box('split', { muted: amps.map(a => a.on === false) })); if (rear.length) rear.unshift(box('merge', { gain: stage.split.gain })); }
    const rowW = r => r.reduce((s, n) => s + n.w, 0) + GAP * Math.max(0, r.length - 1);
    const rowH = Math.max(PW * 1.75, ...front.concat(rear).map(n => n.h));
    const rows = rear.length ? 2 : 1;
    const bw = Math.max(rowW(front), rowW(rear), 520) + PADX * 2, bh = rows * rowH + (rows - 1) * ROWGAP + PADY * 2;
    const bx = CX - bw / 2, by = Math.max(ampBand + 44, ampBand + (HP - ampBand - bh) / 2 + (back ? 6 : 10));
    const rearY = by + PADY, frontY = by + PADY + (rows - 1) * (rowH + ROWGAP);

    const nodes = [];
    const layRow = (row, y0, rev) => {
      let x = bx + (bw - rowW(row)) / 2;
      row.forEach(n => {
        const pl = place(n.svg, x, y0 + rowH - n.h, n.w), sc = pl.h / pl.vb[3];
        if (n.box) {
          const pt = ([px, py]) => [x + (px - n.vb[0]) * sc, pl.y + (py - n.vb[1]) * sc];
          n.ins = n.box.ins.map(pt); n.outs = n.box.outs.map(pt);
        } else {
          const jy = pl.y + (/gp-hw/.test(n.svg) ? pl.h * .62 : (40 - pl.vb[1]) * sc);
          n.ins = [[x + 2, jy]]; n.outs = [[x + n.w - 2, jy]];
        }
        if (rev) { const mir = pt => [2 * x + n.w - pt[0], pt[1]]; n.ins = n.ins.map(mir); n.outs = n.outs.map(mir); }   // in on the right, out on the left
        n.draw = n.it && n.it.on === false ? `<g opacity=".78">${pl.svg}</g>` : pl.svg;
        if (tilt) {                                   // the enclosure's front side, under its face
          const mx = -pl.vb[0] * sc, mb = (/gp-pedal/.test(n.svg) ? 12 : pl.vb[3] * .05) * sc, yb = pl.y + pl.h - mb;
          n.draw = `<rect x="${f(x + mx)}" y="${f(yb - 30)}" width="${f(n.w - mx * 2)}" height="48" rx="9" fill="#0b0b0d"/>
            <rect x="${f(x + mx + 6)}" y="${f(yb + 13)}" width="${f(n.w - mx * 2 - 12)}" height="1.6" fill="#fff" opacity=".12"/>${n.draw}`;
        }
        nodes.push(n); x += n.w + GAP;
      });
    };
    layRow(front, frontY); if (rear.length) layRow(dip ? rear.slice().reverse() : rear, rearY, dip);   // diptych: the back row comes back right to left

    // the amps (and, in the diptych, the rack)
    const FLOORLINE = 800, CUT = [[1040, 0], [950, 420], [1032, 452], [925, 900]];
    const AW = back ? 200 : tilt ? 236 : dip ? 250 : 300, AGAP = back ? 70 : tilt ? 80 : dip ? 34 : 90;
    const RKW = 230, rackSvgs = rack.map(it => place(P().namRack({ label: it.label, title: it.title, on: it.on !== false }), 0, 0, RKW));
    const rackH = rackSvgs.reduce((s, r) => s + r.h - 2, 0) + 44;
    const ampsW = amps.length * AW + (amps.length - 1) * AGAP + (rack.length ? RKW + 56 : 0);
    const ax0 = dip ? 1320 - ampsW / 2 + (rack.length ? RKW + 56 : 0) : (W - ampsW) / 2;
    let under = '', over = '', cross = '', plugs = '', ampSvg = '';
    if (rack.length) {                                // a flight case of 19" units, on the floor
      const rx = ax0 - RKW - 56, ry = FLOORLINE + 14 - rackH;
      let y = ry + 20, units = '';
      rackSvgs.forEach((r, i) => { units += place(P().namRack({ label: rack[i].label, title: rack[i].title, on: rack[i].on !== false }), rx + 16, y, RKW).svg; y += r.h - 2; });
      ampSvg += `<ellipse cx="${f(rx + RKW / 2 + 16)}" cy="${FLOORLINE + 12}" rx="${f(RKW * .62)}" ry="14" fill="#000" opacity=".7" filter="url(#${id}-ash)"/>
        <rect x="${f(rx)}" y="${f(ry)}" width="${RKW + 32}" height="${f(rackH)}" rx="7" fill="#17181a"/><rect x="${f(rx)}" y="${f(ry)}" width="${RKW + 32}" height="${f(rackH)}" rx="7" fill="url(#gp-tolex)" opacity=".7"/>
        <rect x="${f(rx + 3)}" y="${f(ry + 3)}" width="${RKW + 26}" height="${f(rackH - 6)}" rx="5" fill="none" stroke="url(#gp-chrome)" stroke-width="5"/>
        <rect x="${f(rx + 14)}" y="${f(ry + 18)}" width="${RKW + 4}" height="${f(rackH - 40)}" fill="#050505"/>${units}
        ${[[0, 0], [1, 0], [0, 1], [1, 1]].map(([cx, cy]) => `<circle cx="${f(rx + 10 + cx * (RKW + 12))}" cy="${f(ry + 10 + cy * (rackH - 20))}" r="8" fill="url(#gp-chrome)"/>`).join('')}`;
    }
    amps.forEach((a, i) => {
      const x = ax0 + i * (AW + AGAP), svg = P().pedalFor(a), dim = s => (a.on === false ? `<g opacity=".5">${s}</g>` : s);
      if (back || dip) {
        const base = dip ? FLOORLINE : wallY, pl = place(svg, x, 0, AW), y = base + 16 - pl.h;
        ampSvg += `<ellipse cx="${f(x + AW / 2)}" cy="${f(base + 12)}" rx="${f(AW * .56)}" ry="16" fill="#000" opacity=".7" filter="url(#${id}-ash)"/>${dim(place(svg, x, y, AW).svg)}`;
        a.inPt = [x + AW * .84, y + pl.h * .215]; a.retPt = [x + 16, base - 6];
      } else if (tilt) {
        // front face squashed by SZ, standing on the plane at yF; the top face (depth) above it
        const pl = place(svg, x, 0, AW), fh = pl.h * SZ, yF = ampBand - 26, yb = yF * SY, yt = yb - fh;
        const mx = -pl.vb[0] * AW / pl.vb[2], top = yt - pl.vb[1] * fh / pl.vb[3], bwd = AW - mx * 2, dp = bwd * .46 * SY;
        const fam = TOLEX[a.family] ? a.family : 'high-gain', dark = fam !== 'clean';
        const frontSvg = svg.replace(/^<svg /, `<svg x="${f(x)}" y="${f(yt)}" height="${f(fh)}" preserveAspectRatio="none" `).replace(/ width="[\d.]+"/, ` width="${f(AW)}"`);
        ampSvg += dim(`<ellipse cx="${f(x + AW / 2)}" cy="${f(yb + 6)}" rx="${f(AW * .6)}" ry="20" fill="#000" opacity=".75" filter="url(#${id}-ash)"/>${frontSvg}
          <rect x="${f(x + mx)}" y="${f(top - dp)}" width="${f(bwd)}" height="${f(dp + 3)}" rx="7" fill="${TOLEX[fam]}"/>
          <rect x="${f(x + mx)}" y="${f(top - dp)}" width="${f(bwd)}" height="${f(dp + 3)}" rx="7" fill="url(#gp-tolex)" opacity=".8"/>
          <rect x="${f(x + mx)}" y="${f(top - dp)}" width="${f(bwd)}" height="${f(dp + 3)}" rx="7" fill="#fff" opacity="${dark ? .17 : .05}"/>
          <rect x="${f(x + mx + 1)}" y="${f(top - dp + 1)}" width="${f(bwd - 2)}" height="${f(dp + 1)}" rx="6" fill="none" stroke="#fff" stroke-opacity=".22" stroke-width="1.5"/>
          <rect x="${f(x + mx + 2)}" y="${f(top)}" width="${f(bwd - 4)}" height="2" fill="#000" opacity=".5"/>
          ${[0, 1].map(c => [0, 1].map(r => `<rect x="${f(x + mx + c * (bwd - 14))}" y="${f(top - dp + r * (dp - 9))}" width="14" height="11" rx="3.5" fill="url(#gp-chrome)" opacity=".85"/>`).join('')).join('')}
          <rect x="${f(x + mx + bwd * .33)}" y="${f(top - dp * .56)}" width="${f(bwd * .34)}" height="10" rx="5" fill="#0d0d0d"/>
          <rect x="${f(x + mx + bwd * .33 - 5)}" y="${f(top - dp * .56 - 2)}" width="12" height="14" rx="3" fill="url(#gp-chrome)"/><rect x="${f(x + mx + bwd * .67 - 7)}" y="${f(top - dp * .56 - 2)}" width="12" height="14" rx="3" fill="url(#gp-chrome)"/>`);
        a.inPt = [x + AW * .93, yF - 8]; a.retPt = [x + AW * .07, yF - 8];
      } else {
        const d = AW * .47, y = 150 - d / 2 + 16;
        ampSvg += ampTop(id, a, x, y, AW);
        a.inPt = [x + AW * .2, y + d + 9]; a.retPt = [x + AW * .16, y + 4];
      }
    });

    // cables. PRE colour before the amp, POST colour after; a split's second branch a shade darker
    const lastFront = front[front.length - 1], firstRear = rear[0], lastRear = rear[rear.length - 1];
    const patch = (row, col) => row.slice(0, -1).forEach((n, i) => {
      const a = n.outs[0], b = row[i + 1].ins[0], m = [(a[0] + b[0]) / 2, Math.abs(a[1] - b[1]) > 40 ? (a[1] + b[1]) / 2 : Math.max(a[1], b[1]) + 24 + R() * 10];
      const dir = b[0] < a[0] ? -1 : 1;
      under += wire(id, [[a[0] + 16 * dir, a[1]], m, [b[0] - 16 * dir, b[1]]], col, 5);
      plugs += plug(a[0], a[1], -dir) + plug(b[0], b[1], dir);
    });
    patch(front, PRE); patch(rear, POST);
    if (front.length) {                               // the guitar comes in from the left
      const p = front[0].ins[0];
      under += wire(id, [[-20, HP - 70], [Math.max(40, bx - 150), HP - 46], [bx - 44, p[1] + 60], [p[0] - 16, p[1]]], PRE, 6);
      plugs += plug(p[0], p[1], 1);
    }
    const runR = bx + bw + 58, runL = bx - 58, lane = by - (back ? 26 : 34);
    const jack = a => `<rect x="${f(a.inPt[0] - 3.4)}" y="${f(a.inPt[1] - 1)}" width="6.8" height="11" rx="1.2" fill="url(#gp-chrome)"/><rect x="${f(a.inPt[0] - 5)}" y="${f(a.inPt[1] + 8)}" width="10" height="10" rx="2.5" fill="#1b1b1b"/>`;
    amps.forEach((a, i) => {
      const colA = shade(PRE, 1 - i * .22), colB = shade(POST, 1 - i * .22), off = i * 20;
      if (lastFront) {                                // board -> amp input
        const o = lastFront.outs[Math.min(i, lastFront.outs.length - 1)];
        plugs += plug(o[0], o[1], -1);
        if (dip) {                                    // straight across the bolt
          cross += wire(id, [[o[0] + 16, o[1]], [bx + bw + 40, o[1] - 6 - off], [985, o[1] - 24 - off], [a.inPt[0] - 70, a.inPt[1] + 170 + off * 2], [a.inPt[0] + 14, a.inPt[1] + 80], a.inPt], colA) + jack(a);
        } else {                                      // round the right of the board
          const pts = [[o[0] + 16, o[1]], [runR + off, o[1] - 50], [runR + off - 6, lane + 40 - off]];
          if (back) pts.push([a.inPt[0] + 70, lane - off * .6], [a.inPt[0] + 8, a.inPt[1] + 70], a.inPt);
          else if (tilt) pts.push([a.inPt[0] + 120, lane - 20 - off * .5], a.inPt);
          else pts.push([a.inPt[0] + 110, lane - off * .5], [a.inPt[0] + 6, a.inPt[1] + 26], a.inPt);
          over += wire(id, pts, colA) + (tilt ? '' : jack(a));
        }
      }
      if (firstRear) {                                // amp (from behind) -> back row
        const p = firstRear.ins[Math.min(i, firstRear.ins.length - 1)];
        if (dip) {                                    // back across the bolt, into the right end of the back row
          cross += wire(id, [[a.retPt[0] + 20, a.retPt[1] - 8], [1006 - off, a.retPt[1] - 70], [990 - off, p[1] + 90], [p[0] + 90, p[1] + 14 - off], [p[0] + 16, p[1]]], colB);
          plugs += plug(p[0], p[1], -1);
          return;
        }
        const wx = Math.min(a.retPt[0] - 70, p[0] - 90) - off;
        const tail = wx < bx ? [[runL - off, lane + 44], [runL - off + 4, p[1] - 46]] : [[wx + 26, lane + 10], [wx, (lane + p[1]) / 2 + 10]];
        const start = back ? [[a.retPt[0] + 20, a.retPt[1] - 8], [Math.max(a.retPt[0] - 30, wx + 60), lane - 6 - off * .5]]
          : tilt ? [[a.retPt[0], a.retPt[1]], [a.retPt[0] - 60 - off, lane - 24]]
          : [[a.retPt[0], a.retPt[1] + 10], [a.retPt[0] - 6, a.retPt[1] - 26], [a.retPt[0] - 90 - off, a.retPt[1] + 20], [a.retPt[0] - 110 - off, lane - 30]];
        under = wire(id, start.concat(tail, [[p[0] - 16, p[1]]]), colB) + under;
        plugs += plug(p[0], p[1], 1);
      }
    });
    const end = lastRear || (amps.length ? null : lastFront);
    if (end) {                                        // out to front of house
      const o = end.outs[0];
      under += dip ? wire(id, [[o[0] - 16, o[1]], [bx - 30, o[1] - 20], [bx - 70, o[1] - 120], [-30, o[1] - 170]], POST, 6)
        : wire(id, [[o[0] + 16, o[1]], [bx + bw + 30, o[1] + 34], [W - 120, o[1] + 110], [W + 20, o[1] + 130]], rear.length ? POST : PRE, 6);
      plugs += plug(o[0], o[1], dip ? 1 : -1);
    }

    const rowTape = (y, txt) => tape(bx + 30, y + rowH / 2, txt, -90);
    const floor = floorKind === 'concrete' ? concrete(id, W, HP, R) : planks(id, W, HP, R);
    const plane = `${floor}
${back && amps.length ? wall(id, W, wallY, floorKind) : ''}
${tilt ? `<rect x="${f(bx - 8)}" y="${f(by + bh - 12)}" width="${f(bw + 16)}" height="34" rx="8" fill="#08080a"/>` : ''}
${board(id, bx, by, bw, bh)}
${rear.length ? rowTape(rearY, 'AFTER AMP') : ''}${amps.length ? rowTape(frontY, 'BEFORE AMP') : ''}
${under}
${tilt || dip ? '' : ampSvg}
${nodes.map(n => n.draw).join('\n')}
${over}
${plugs}`;
    const cutPts = CUT.map(p => p.join(',')).join(' ');
    const right = !dip ? '' : `<defs><clipPath id="${id}-cut"><polygon points="${cutPts} ${W},${H} ${W},0"/></clipPath></defs>
<polyline points="${cutPts}" fill="none" stroke="#000" stroke-opacity=".6" stroke-width="26" filter="url(#${id}-ash)"/>
<g clip-path="url(#${id}-cut)">${wall(id, W, FLOORLINE, floorKind)}
  <rect y="${FLOORLINE}" width="${W}" height="${H - FLOORLINE}" fill="#0b0908"/><rect y="${FLOORLINE}" width="${W}" height="3" fill="#fff" opacity=".1"/>
  ${ampSvg}</g>
<polyline points="${cutPts}" fill="none" stroke="#ffd23c" stroke-width="7" stroke-linejoin="miter" filter="url(#gp-glow-soft)"/>
<polyline points="${cutPts}" fill="none" stroke="#fff7d6" stroke-width="2.4" stroke-linejoin="miter"/>`;
    return `<svg class="gb-scene" viewBox="0 0 ${W} ${H}" role="img" aria-label="rig on a pedalboard">
<defs><filter id="${id}-blur" x="-10%" y="-30%" width="120%" height="160%"><feGaussianBlur stdDeviation="2.4"/></filter>
  <filter id="${id}-ash" x="-30%" y="-60%" width="160%" height="220%"><feGaussianBlur stdDeviation="9"/></filter>
  <radialGradient id="${id}-spot" cx=".5" cy="${back ? .62 : .55}" r=".72"><stop offset="0" stop-color="#ffe9c2" stop-opacity=".16"/><stop offset=".6" stop-color="#ffe9c2" stop-opacity=".03"/><stop offset="1" stop-color="#000" stop-opacity=".62"/></radialGradient></defs>
${tilt ? `<g transform="scale(1 ${SY})">${plane}</g>${ampSvg}` : plane}
${right}
${cross}
<rect width="${W}" height="${H}" fill="url(#${id}-spot)" pointer-events="none"/>
</svg>`;
  }

  root.GolemBoard = { render, _parts: { place, planks, concrete, wall, board, tape, plug, wire, rng, familyOf, isAmp, PRE, POST } };
})(typeof window !== 'undefined' ? window : globalThis);
