/* mundo/themes.js — registry de temas (YG-146 protótipo).
 *
 * Um tema é MAIS que cor: define paleta + tile size + draw-functions
 * procedurais para chão/parede/porta(pasta)/nota/npc/avatar/saída + ambiance.
 * Forma muda por tema (arco de pedra ≠ portão torii ≠ porta de vidro). A mesma
 * interface aceitaria sprites/imagens (drawImage) no futuro — extensível.
 *
 * Interface de cada draw-fn: (ctx, px, py, s, ...extra). px,py = canto do tile.
 * Famílias: medieval (2), garden (2), modern (1). */

// ── helpers comuns ───────────────────────────────────────────────────────────
function shadow(ctx, px, py, s, w) {
  ctx.save();
  ctx.fillStyle = 'rgba(0,0,0,.28)';
  ctx.beginPath();
  ctx.ellipse(px + s / 2, py + s * 0.82, (w || s * 0.32), s * 0.12, 0, 0, Math.PI * 2);
  ctx.fill(); ctx.restore();
}
function glow(ctx, px, py, s, color) {
  ctx.save();
  const g = ctx.createRadialGradient(px + s / 2, py + s / 2, 2, px + s / 2, py + s / 2, s * 0.7);
  g.addColorStop(0, color); g.addColorStop(1, 'rgba(0,0,0,0)');
  ctx.fillStyle = g; ctx.fillRect(px - s * 0.2, py - s * 0.2, s * 1.4, s * 1.4); ctx.restore();
}
function bob(t) { return Math.sin(t * 3) * 1.2; } // leve flutuação idle
// avatar humanoide simples e themeável
function humanoid(ctx, px, py, s, dir, walking, t, col) {
  const cx = px + s / 2, base = py + s * 0.78;
  const stepy = walking ? Math.abs(Math.sin(t * 9)) * 1.6 : bob(t) * 0.4;
  shadow(ctx, px, py, s, s * 0.24);
  // corpo
  ctx.fillStyle = col.body;
  roundedRect(ctx, cx - s * 0.16, base - s * 0.36 - stepy, s * 0.32, s * 0.30, 3);
  // cabeça
  ctx.fillStyle = col.skin;
  ctx.beginPath(); ctx.arc(cx, base - s * 0.42 - stepy, s * 0.12, 0, Math.PI * 2); ctx.fill();
  // cabelo/elmo
  ctx.fillStyle = col.head;
  ctx.beginPath(); ctx.arc(cx, base - s * 0.45 - stepy, s * 0.12, Math.PI, Math.PI * 2); ctx.fill();
  // olhar (direção)
  ctx.fillStyle = '#1a1a1a';
  const ex = dir === 'left' ? -2 : dir === 'right' ? 2 : 0;
  if (dir !== 'up') { ctx.beginPath(); ctx.arc(cx + ex, base - s * 0.42 - stepy, 1.3, 0, 7); ctx.fill(); }
}
function roundedRect(ctx, x, y, w, h, r) {
  ctx.beginPath(); ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r); ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r); ctx.arcTo(x, y, x + w, y, r); ctx.closePath(); ctx.fill();
}
function label(ctx, px, py, s, txt, color) {
  if (!txt) return;
  ctx.save(); ctx.font = '600 9px system-ui,sans-serif'; ctx.textAlign = 'center';
  ctx.fillStyle = color || 'rgba(255,255,255,.85)';
  ctx.fillText(txt.length > 12 ? txt.slice(0, 11) + '…' : txt, px + s / 2, py + s - 2);
  ctx.restore();
}
function noteTint(status) {
  return status === 'done' ? '#34d399' : status === 'doing' ? '#fbbf24' : status === 'todo' ? '#60a5fa' : null;
}

// ════════════════════ MEDIEVAL A — castelo/masmorra ════════════════════
const medievalCastle = {
  id: 'medieval-castle', label: 'Medieval · Castelo', family: 'medieval', tile: 44,
  pal: { floor: '#3a3530', floorB: '#332e29', wall: '#5b5048', wallTop: '#6e6258', line: '#241f1b', gold: '#d4af37' },
  bg(ctx, v) { ctx.fillStyle = '#15110e'; ctx.fillRect(v.x, v.y, v.w, v.h); },
  floor(ctx, px, py, s, gx, gy) {
    ctx.fillStyle = (gx + gy) % 2 ? this.pal.floor : this.pal.floorB;
    ctx.fillRect(px, py, s, s);
    ctx.strokeStyle = this.pal.line; ctx.lineWidth = 1; ctx.strokeRect(px + .5, py + .5, s - 1, s - 1);
  },
  wall(ctx, px, py, s) {
    ctx.fillStyle = this.pal.wall; ctx.fillRect(px, py, s, s);
    ctx.fillStyle = this.pal.wallTop; ctx.fillRect(px, py, s, s * 0.28);
    ctx.strokeStyle = this.pal.line; ctx.lineWidth = 1.5;
    ctx.strokeRect(px + 2, py + 2, s - 4, s - 4);
    ctx.beginPath(); ctx.moveTo(px, py + s * 0.5); ctx.lineTo(px + s, py + s * 0.5); ctx.stroke();
  },
  door(ctx, px, py, s, d, hover, t) {
    if (hover) glow(ctx, px, py, s, 'rgba(212,175,55,.35)');
    // arco de pedra
    ctx.fillStyle = '#2b2620'; roundedRect(ctx, px + s * 0.18, py + s * 0.16, s * 0.64, s * 0.74, 4);
    ctx.fillStyle = '#1a1512'; roundedRect(ctx, px + s * 0.26, py + s * 0.26, s * 0.48, s * 0.62, 3);
    ctx.fillStyle = this.pal.gold;
    ctx.beginPath(); ctx.arc(px + s * 0.68, py + s * 0.58, 2.2, 0, 7); ctx.fill(); // maçaneta
    label(ctx, px, py, s, d.label, '#e9d9a8');
  },
  note(ctx, px, py, s, n, hover, t) {
    shadow(ctx, px, py, s, s * 0.2);
    if (hover) glow(ctx, px, py, s, 'rgba(230,210,150,.4)');
    const y = py + s * 0.2 + bob(t) * 0.6;
    // pergaminho enrolado
    ctx.fillStyle = '#e8dcae'; roundedRect(ctx, px + s * 0.28, y, s * 0.44, s * 0.5, 3);
    ctx.strokeStyle = '#b9a76e'; ctx.lineWidth = 1;
    for (let i = 1; i < 4; i++) { ctx.beginPath(); ctx.moveTo(px + s * 0.33, y + i * s * 0.11); ctx.lineTo(px + s * 0.67, y + i * s * 0.11); ctx.stroke(); }
    const tint = noteTint(n.status); if (tint) { ctx.fillStyle = tint; ctx.beginPath(); ctx.arc(px + s * 0.7, y + 2, 3, 0, 7); ctx.fill(); }
    label(ctx, px, py, s, n.title, '#e9d9a8');
  },
  npc(ctx, px, py, s, p, hover, t) {
    if (hover) glow(ctx, px, py, s, 'rgba(120,180,255,.4)');
    humanoid(ctx, px, py, s, 'down', false, t, { body: '#6b3f8f', skin: '#e8c39a', head: '#caa86a' });
    // chapéu de mago
    ctx.fillStyle = '#4a2a66'; ctx.beginPath();
    ctx.moveTo(px + s * 0.5, py + s * 0.14); ctx.lineTo(px + s * 0.38, py + s * 0.34); ctx.lineTo(px + s * 0.62, py + s * 0.34); ctx.fill();
    ctx.fillStyle = '#ffd45e'; ctx.font = 'bold 12px serif'; ctx.textAlign = 'center';
    ctx.fillText('?', px + s * 0.5, py + s * 0.06 + bob(t) * 2);
    label(ctx, px, py, s, p.name, '#cfe3ff');
  },
  avatar(ctx, px, py, s, dir, walking, t) {
    humanoid(ctx, px, py, s, dir, walking, t, { body: '#9aa3b2', skin: '#e8c39a', head: '#b0b8c6' });
  },
  exit(ctx, px, py, s, hover) {
    if (hover) glow(ctx, px, py, s, 'rgba(212,175,55,.4)');
    ctx.fillStyle = '#241f1b'; roundedRect(ctx, px + s * 0.2, py + s * 0.2, s * 0.6, s * 0.6, 4);
    ctx.fillStyle = '#d4af37'; ctx.font = 'bold 16px serif'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    ctx.fillText('↑', px + s / 2, py + s / 2); ctx.textBaseline = 'alphabetic';
    label(ctx, px, py, s, 'voltar', '#e9d9a8');
  },
  ambiance(ctx, v, t) { // vinheta de tocha
    const g = ctx.createRadialGradient(v.x + v.w / 2, v.y + v.h / 2, v.h * 0.3, v.x + v.w / 2, v.y + v.h / 2, v.h * 0.8);
    g.addColorStop(0, 'rgba(0,0,0,0)'); g.addColorStop(1, 'rgba(0,0,0,.45)');
    ctx.fillStyle = g; ctx.fillRect(v.x, v.y, v.w, v.h);
  },
};

// ════════════════════ MEDIEVAL B — taverna/madeira ════════════════════
const medievalTavern = {
  id: 'medieval-tavern', label: 'Medieval · Taverna', family: 'medieval', tile: 44,
  pal: { plank: '#7a4f2c', plankB: '#6c4525', wall: '#caa472', beam: '#5a3a1f', amber: '#ffb347' },
  bg(ctx, v) { ctx.fillStyle = '#2a1c10'; ctx.fillRect(v.x, v.y, v.w, v.h); },
  floor(ctx, px, py, s, gx, gy) {
    ctx.fillStyle = gy % 2 ? this.pal.plank : this.pal.plankB; ctx.fillRect(px, py, s, s);
    ctx.strokeStyle = 'rgba(0,0,0,.25)'; ctx.lineWidth = 1;
    ctx.beginPath(); ctx.moveTo(px, py + .5); ctx.lineTo(px + s, py + .5); ctx.stroke();
    if (gx % 3 === 0) { ctx.beginPath(); ctx.moveTo(px + .5, py); ctx.lineTo(px + .5, py + s); ctx.stroke(); }
  },
  wall(ctx, px, py, s) {
    ctx.fillStyle = this.pal.wall; ctx.fillRect(px, py, s, s);
    ctx.fillStyle = this.pal.beam; // vigas (timber framing)
    ctx.fillRect(px, py, s, 4); ctx.fillRect(px, py + s - 4, s, 4); ctx.fillRect(px + s * 0.45, py, 5, s);
  },
  door(ctx, px, py, s, d, hover, t) {
    if (hover) glow(ctx, px, py, s, 'rgba(255,179,71,.4)');
    ctx.fillStyle = '#3a2412'; roundedRect(ctx, px + s * 0.2, py + s * 0.14, s * 0.6, s * 0.78, 3);
    ctx.fillStyle = '#5a3a1f'; roundedRect(ctx, px + s * 0.26, py + s * 0.2, s * 0.48, s * 0.66, 2);
    ctx.strokeStyle = '#2a1808'; ctx.lineWidth = 1.2;
    ctx.beginPath(); ctx.moveTo(px + s * 0.5, py + s * 0.2); ctx.lineTo(px + s * 0.5, py + s * 0.86); ctx.stroke();
    ctx.fillStyle = '#d9b27a'; ctx.beginPath(); ctx.arc(px + s * 0.42, py + s * 0.56, 2, 0, 7); ctx.fill();
    label(ctx, px, py, s, d.label, '#ffe2b0');
  },
  note(ctx, px, py, s, n, hover, t) {
    shadow(ctx, px, py, s, s * 0.2);
    if (hover) glow(ctx, px, py, s, 'rgba(255,220,160,.45)');
    const y = py + s * 0.22 + bob(t) * 0.5;
    ctx.save(); ctx.translate(px + s / 2, y + s * 0.22); ctx.rotate(-0.06); ctx.translate(-(px + s / 2), -(y + s * 0.22));
    ctx.fillStyle = '#f3e6c4'; roundedRect(ctx, px + s * 0.27, y, s * 0.46, s * 0.46, 2);
    ctx.strokeStyle = '#caa86a'; ctx.lineWidth = 1; ctx.strokeRect(px + s * 0.27, y, s * 0.46, s * 0.46);
    ctx.restore();
    const tint = noteTint(n.status); if (tint) { ctx.fillStyle = tint; ctx.fillRect(px + s * 0.27, y - 2, s * 0.46, 3); }
    label(ctx, px, py, s, n.title, '#ffe2b0');
  },
  npc(ctx, px, py, s, p, hover, t) {
    if (hover) glow(ctx, px, py, s, 'rgba(255,200,120,.45)');
    humanoid(ctx, px, py, s, 'down', false, t, { body: '#a8632e', skin: '#e8c39a', head: '#7a4a22' });
    // avental
    ctx.fillStyle = '#e8dcae'; roundedRect(ctx, px + s * 0.38, py + s * 0.5, s * 0.24, s * 0.2, 2);
    ctx.fillStyle = '#ffb347'; ctx.font = 'bold 12px serif'; ctx.textAlign = 'center';
    ctx.fillText('?', px + s * 0.5, py + s * 0.08 + bob(t) * 2);
    label(ctx, px, py, s, p.name, '#ffe2b0');
  },
  avatar(ctx, px, py, s, dir, walking, t) {
    humanoid(ctx, px, py, s, dir, walking, t, { body: '#3f6b8f', skin: '#e8c39a', head: '#5a4632' });
  },
  exit(ctx, px, py, s, hover) {
    if (hover) glow(ctx, px, py, s, 'rgba(255,179,71,.4)');
    ctx.fillStyle = '#3a2412'; roundedRect(ctx, px + s * 0.2, py + s * 0.2, s * 0.6, s * 0.6, 3);
    ctx.fillStyle = '#ffb347'; ctx.font = 'bold 16px serif'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    ctx.fillText('↑', px + s / 2, py + s / 2); ctx.textBaseline = 'alphabetic';
    label(ctx, px, py, s, 'voltar', '#ffe2b0');
  },
};

// ════════════════════ GARDEN A — floresta/jardim ════════════════════
const gardenForest = {
  id: 'garden-forest', label: 'Jardim · Floresta', family: 'garden', tile: 44,
  pal: { grass: '#3f7d3a', grassB: '#377033', hedge: '#2c5a2a', soil: '#5a4326', bloom: '#f78fb3' },
  bg(ctx, v) { ctx.fillStyle = '#22421f'; ctx.fillRect(v.x, v.y, v.w, v.h); },
  floor(ctx, px, py, s, gx, gy) {
    ctx.fillStyle = (gx + gy) % 2 ? this.pal.grass : this.pal.grassB; ctx.fillRect(px, py, s, s);
    // tufos de grama
    ctx.strokeStyle = 'rgba(255,255,255,.10)'; ctx.lineWidth = 1;
    const seed = (gx * 7 + gy * 13) % 5;
    for (let i = 0; i < 3; i++) {
      const tx = px + s * (0.2 + ((seed + i) % 3) * 0.3), ty = py + s * (0.7 + (i % 2) * 0.12);
      ctx.beginPath(); ctx.moveTo(tx, ty); ctx.lineTo(tx - 2, ty - 5); ctx.moveTo(tx, ty); ctx.lineTo(tx + 2, ty - 5); ctx.stroke();
    }
  },
  wall(ctx, px, py, s) { // arbusto/hedge
    ctx.fillStyle = this.pal.hedge; roundedRect(ctx, px, py, s, s, 6);
    ctx.fillStyle = 'rgba(255,255,255,.08)';
    for (let i = 0; i < 5; i++) ctx.fillRect(px + (i * 9 % s), py + ((i * 13) % s), 4, 4);
    ctx.strokeStyle = 'rgba(0,0,0,.18)'; ctx.lineWidth = 1; ctx.strokeRect(px + 1, py + 1, s - 2, s - 2);
  },
  door(ctx, px, py, s, d, hover, t) { // arco de treliça com flores
    if (hover) glow(ctx, px, py, s, 'rgba(247,143,179,.45)');
    ctx.strokeStyle = '#caa86a'; ctx.lineWidth = 3;
    ctx.beginPath(); ctx.moveTo(px + s * 0.22, py + s * 0.9); ctx.lineTo(px + s * 0.22, py + s * 0.32);
    ctx.arc(px + s * 0.5, py + s * 0.32, s * 0.28, Math.PI, 0); ctx.lineTo(px + s * 0.78, py + s * 0.9); ctx.stroke();
    ctx.fillStyle = '#173a14'; // vão escuro
    ctx.beginPath(); ctx.moveTo(px + s * 0.28, py + s * 0.9); ctx.lineTo(px + s * 0.28, py + s * 0.36);
    ctx.arc(px + s * 0.5, py + s * 0.36, s * 0.22, Math.PI, 0); ctx.lineTo(px + s * 0.72, py + s * 0.9); ctx.fill();
    for (let i = 0; i < 5; i++) { ctx.fillStyle = i % 2 ? this.pal.bloom : '#ffd45e'; ctx.beginPath(); ctx.arc(px + s * (0.24 + i * 0.13), py + s * (0.3 + (i % 2) * 0.1), 2.4, 0, 7); ctx.fill(); }
    label(ctx, px, py, s, d.label, '#dff5d0');
  },
  note(ctx, px, py, s, n, hover, t) { // placa de semente / folha
    shadow(ctx, px, py, s, s * 0.2);
    if (hover) glow(ctx, px, py, s, 'rgba(180,255,160,.5)');
    const y = py + s * 0.24 + bob(t) * 0.7;
    ctx.fillStyle = '#caa86a'; ctx.fillRect(px + s * 0.49, y, 2, s * 0.5); // haste
    ctx.fillStyle = noteTint(n.status) || '#7ec850'; // folha
    ctx.save(); ctx.translate(px + s * 0.5, y); ctx.rotate(0.3);
    ctx.beginPath(); ctx.ellipse(0, 0, s * 0.2, s * 0.11, 0, 0, 7); ctx.fill(); ctx.restore();
    ctx.strokeStyle = 'rgba(0,0,0,.2)'; ctx.beginPath(); ctx.moveTo(px + s * 0.4, y); ctx.lineTo(px + s * 0.6, y); ctx.stroke();
    label(ctx, px, py, s, n.title, '#dff5d0');
  },
  npc(ctx, px, py, s, p, hover, t) { // jardineiro/duende
    if (hover) glow(ctx, px, py, s, 'rgba(255,240,150,.5)');
    humanoid(ctx, px, py, s, 'down', false, t, { body: '#5a8f3f', skin: '#e8c39a', head: '#caa86a' });
    ctx.fillStyle = '#d94a4a'; ctx.beginPath(); // chapéu pontudo vermelho
    ctx.moveTo(px + s * 0.5, py + s * 0.14); ctx.lineTo(px + s * 0.36, py + s * 0.34); ctx.lineTo(px + s * 0.64, py + s * 0.34); ctx.fill();
    ctx.fillStyle = '#fff7b0'; ctx.font = 'bold 12px serif'; ctx.textAlign = 'center';
    ctx.fillText('?', px + s * 0.5, py + s * 0.08 + bob(t) * 2);
    label(ctx, px, py, s, p.name, '#dff5d0');
  },
  avatar(ctx, px, py, s, dir, walking, t) {
    humanoid(ctx, px, py, s, dir, walking, t, { body: '#c0673a', skin: '#e8c39a', head: '#5a4632' });
  },
  exit(ctx, px, py, s, hover) {
    if (hover) glow(ctx, px, py, s, 'rgba(247,143,179,.45)');
    ctx.fillStyle = '#173a14'; roundedRect(ctx, px + s * 0.22, py + s * 0.22, s * 0.56, s * 0.56, 8);
    ctx.fillStyle = '#dff5d0'; ctx.font = 'bold 16px serif'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    ctx.fillText('↑', px + s / 2, py + s / 2); ctx.textBaseline = 'alphabetic';
    label(ctx, px, py, s, 'voltar', '#dff5d0');
  },
};

// ════════════════════ GARDEN B — zen/japonês ════════════════════
const gardenZen = {
  id: 'garden-zen', label: 'Jardim · Zen', family: 'garden', tile: 44,
  pal: { sand: '#e6dcc3', sandB: '#ddd2b6', stone: '#8a8275', moss: '#6f8f5a', torii: '#c0392b' },
  bg(ctx, v) { ctx.fillStyle = '#cdbf9f'; ctx.fillRect(v.x, v.y, v.w, v.h); },
  floor(ctx, px, py, s, gx, gy) {
    ctx.fillStyle = (gx + gy) % 2 ? this.pal.sand : this.pal.sandB; ctx.fillRect(px, py, s, s);
    ctx.strokeStyle = 'rgba(120,100,70,.25)'; ctx.lineWidth = 1; // areia raked
    for (let i = 0; i < 3; i++) { ctx.beginPath(); ctx.moveTo(px, py + s * (0.25 + i * 0.28)); ctx.lineTo(px + s, py + s * (0.25 + i * 0.28)); ctx.stroke(); }
  },
  wall(ctx, px, py, s) { // bambu
    ctx.fillStyle = '#7a8a4a'; ctx.fillRect(px, py, s, s);
    ctx.fillStyle = '#94a85a';
    for (let i = 0; i < 3; i++) ctx.fillRect(px + 3 + i * (s / 3), py, s / 4, s);
    ctx.strokeStyle = '#5a6a36'; ctx.lineWidth = 1;
    for (let i = 0; i < 3; i++) { ctx.beginPath(); ctx.moveTo(px, py + s * (0.3 + i * 0.3)); ctx.lineTo(px + s, py + s * (0.3 + i * 0.3)); ctx.stroke(); }
  },
  door(ctx, px, py, s, d, hover, t) { // portão torii
    if (hover) glow(ctx, px, py, s, 'rgba(192,57,43,.4)');
    ctx.fillStyle = this.pal.torii;
    ctx.fillRect(px + s * 0.16, py + s * 0.2, s * 0.68, s * 0.1); // topo
    ctx.fillRect(px + s * 0.2, py + s * 0.34, s * 0.6, s * 0.06); // trave
    ctx.fillRect(px + s * 0.26, py + s * 0.2, s * 0.07, s * 0.7); // perna
    ctx.fillRect(px + s * 0.67, py + s * 0.2, s * 0.07, s * 0.7);
    label(ctx, px, py, s, d.label, '#7a3020');
  },
  note(ctx, px, py, s, n, hover, t) { // lanterna de pedra
    shadow(ctx, px, py, s, s * 0.2);
    if (hover) glow(ctx, px, py, s, 'rgba(255,230,150,.5)');
    const x = px + s * 0.5, y = py + s * 0.28;
    ctx.fillStyle = this.pal.stone;
    ctx.fillRect(x - s * 0.06, y + s * 0.28, s * 0.12, s * 0.22); // base
    ctx.fillRect(x - s * 0.15, y + s * 0.1, s * 0.3, s * 0.12);   // caixa de luz
    ctx.fillStyle = noteTint(n.status) || '#ffe08a';
    ctx.fillRect(x - s * 0.08, y + s * 0.12, s * 0.16, s * 0.08); // luz
    ctx.fillStyle = this.pal.stone;
    ctx.beginPath(); ctx.moveTo(x - s * 0.18, y + s * 0.1); ctx.lineTo(x, y); ctx.lineTo(x + s * 0.18, y + s * 0.1); ctx.fill(); // telhado
    label(ctx, px, py, s, n.title, '#5a4a2a');
  },
  npc(ctx, px, py, s, p, hover, t) { // grou/monge
    if (hover) glow(ctx, px, py, s, 'rgba(255,255,255,.5)');
    humanoid(ctx, px, py, s, 'down', false, t, { body: '#d8d2c4', skin: '#e8c39a', head: '#3a3a3a' });
    ctx.fillStyle = '#c0392b'; ctx.font = 'bold 12px serif'; ctx.textAlign = 'center';
    ctx.fillText('?', px + s * 0.5, py + s * 0.1 + bob(t) * 2);
    label(ctx, px, py, s, p.name, '#5a4a2a');
  },
  avatar(ctx, px, py, s, dir, walking, t) {
    humanoid(ctx, px, py, s, dir, walking, t, { body: '#4a6f8f', skin: '#e8c39a', head: '#2a2a2a' });
  },
  exit(ctx, px, py, s, hover) {
    if (hover) glow(ctx, px, py, s, 'rgba(192,57,43,.4)');
    ctx.fillStyle = this.pal.torii; ctx.fillRect(px + s * 0.2, py + s * 0.2, s * 0.6, s * 0.1);
    ctx.fillStyle = '#5a4a2a'; ctx.font = 'bold 15px serif'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    ctx.fillText('↑', px + s / 2, py + s * 0.6); ctx.textBaseline = 'alphabetic';
    label(ctx, px, py, s, 'voltar', '#5a4a2a');
  },
};

// ════════════════════ MODERN — escritório (Gather-like) ════════════════════
const modern = {
  id: 'modern-office', label: 'Moderno · Office', family: 'modern', tile: 42,
  pal: { floor: '#eef1f5', floorB: '#e6eaf0', wall: '#cfd6e0', glass: 'rgba(120,170,255,.18)', accent: '#6366f1' },
  bg(ctx, v) { ctx.fillStyle = '#f3f5f9'; ctx.fillRect(v.x, v.y, v.w, v.h); },
  floor(ctx, px, py, s, gx, gy) {
    ctx.fillStyle = (gx + gy) % 2 ? this.pal.floor : this.pal.floorB; ctx.fillRect(px, py, s, s);
    ctx.strokeStyle = 'rgba(120,140,170,.18)'; ctx.lineWidth = 1; ctx.strokeRect(px + .5, py + .5, s - 1, s - 1);
  },
  wall(ctx, px, py, s) { // partição de vidro
    ctx.fillStyle = this.pal.wall; ctx.fillRect(px, py, s, s);
    ctx.fillStyle = this.pal.glass; ctx.fillRect(px + 3, py + 3, s - 6, s - 6);
    ctx.strokeStyle = '#aeb8c6'; ctx.lineWidth = 2; ctx.strokeRect(px + 3, py + 3, s - 6, s - 6);
    ctx.strokeStyle = 'rgba(255,255,255,.5)'; ctx.beginPath(); ctx.moveTo(px + 6, py + 6); ctx.lineTo(px + s - 8, py + s - 8); ctx.stroke();
  },
  door(ctx, px, py, s, d, hover, t) { // porta de vidro deslizante
    if (hover) glow(ctx, px, py, s, 'rgba(99,102,241,.3)');
    ctx.fillStyle = '#fff'; roundedRect(ctx, px + s * 0.16, py + s * 0.16, s * 0.68, s * 0.74, 4);
    ctx.fillStyle = this.pal.glass; ctx.fillRect(px + s * 0.2, py + s * 0.2, s * 0.6, s * 0.66);
    ctx.strokeStyle = this.pal.accent; ctx.lineWidth = 2;
    ctx.beginPath(); ctx.moveTo(px + s * 0.5, py + s * 0.2); ctx.lineTo(px + s * 0.5, py + s * 0.86); ctx.stroke();
    ctx.fillStyle = this.pal.accent; roundedRect(ctx, px + s * 0.43, py + s * 0.48, s * 0.05, s * 0.12, 2);
    label(ctx, px, py, s, d.label, '#3b4252');
  },
  note(ctx, px, py, s, n, hover, t) { // file card / sticky
    shadow(ctx, px, py, s, s * 0.18);
    if (hover) glow(ctx, px, py, s, 'rgba(99,102,241,.3)');
    const y = py + s * 0.24;
    ctx.fillStyle = '#fff'; roundedRect(ctx, px + s * 0.26, y, s * 0.48, s * 0.5, 3);
    ctx.fillStyle = noteTint(n.status) || this.pal.accent; ctx.fillRect(px + s * 0.26, y, s * 0.48, 5);
    ctx.strokeStyle = '#dde2ea'; ctx.lineWidth = 1;
    for (let i = 1; i < 4; i++) { ctx.beginPath(); ctx.moveTo(px + s * 0.31, y + 6 + i * s * 0.09); ctx.lineTo(px + s * 0.69, y + 6 + i * s * 0.09); ctx.stroke(); }
    label(ctx, px, py, s, n.title, '#3b4252');
  },
  npc(ctx, px, py, s, p, hover, t) { // assistente/robô
    if (hover) glow(ctx, px, py, s, 'rgba(99,102,241,.35)');
    shadow(ctx, px, py, s, s * 0.22);
    const yb = bob(t);
    ctx.fillStyle = '#c7cdd8'; roundedRect(ctx, px + s * 0.3, py + s * 0.34 - yb, s * 0.4, s * 0.36, 6); // corpo
    ctx.fillStyle = '#3b4252'; roundedRect(ctx, px + s * 0.32, py + s * 0.2 - yb, s * 0.36, s * 0.2, 6); // cabeça
    ctx.fillStyle = this.pal.accent; ctx.beginPath(); ctx.arc(px + s * 0.43, py + s * 0.3 - yb, 2.4, 0, 7); ctx.arc(px + s * 0.57, py + s * 0.3 - yb, 2.4, 0, 7); ctx.fill();
    ctx.fillStyle = '#fff'; ctx.font = 'bold 11px system-ui'; ctx.textAlign = 'center';
    ctx.fillText('?', px + s * 0.5, py + s * 0.12 - yb);
    label(ctx, px, py, s, p.name, '#3b4252');
  },
  avatar(ctx, px, py, s, dir, walking, t) {
    humanoid(ctx, px, py, s, dir, walking, t, { body: '#6366f1', skin: '#e8c39a', head: '#3b4252' });
  },
  exit(ctx, px, py, s, hover) {
    if (hover) glow(ctx, px, py, s, 'rgba(99,102,241,.3)');
    ctx.fillStyle = '#fff'; roundedRect(ctx, px + s * 0.2, py + s * 0.2, s * 0.6, s * 0.6, 6);
    ctx.fillStyle = this.pal.accent; ctx.font = 'bold 16px system-ui'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    ctx.fillText('↑', px + s / 2, py + s / 2); ctx.textBaseline = 'alphabetic';
    label(ctx, px, py, s, 'voltar', '#3b4252');
  },
};

export const THEMES = [medievalCastle, medievalTavern, gardenForest, gardenZen, modern];
export const THEME_BY_ID = Object.fromEntries(THEMES.map((t) => [t.id, t]));
