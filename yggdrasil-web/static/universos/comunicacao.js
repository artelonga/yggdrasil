// Universo "comunicação" — mapa interativo de léxico cross-linguístico.
// Pan/zoom contínuo, elementos multilíngues, publicação no léxico e revisão.
'use strict';

const JWT_KEY = 'yggdrasil-jwt';
const API = '/api/v1/comunicacao';

const state = {
  token: localStorage.getItem(JWT_KEY),
  sub: null,        // meu `sub` do JWT (dono)
  canEdit: false,   // dono da sala atual?
  roomId: null,
  room: null,
  selectedId: null,
  selectedLinkId: null,
  view: { panX: 0, panY: 0, zoom: 1 },
  // modo de ligação (relações)
  linkMode: false,
  linkFrom: null,
  // interação por ponteiro
  pointers: new Map(),
  drag: null, // { kind:'pan'|'elem', id?, ox?, oy?, moved?, sx0, sy0 }
  pinch: null,
  review: { items: [], idx: 0, revealed: false },
};

const ZOOM_MIN = 0.1, ZOOM_MAX = 8;
const GRID = 60; // unidades de mundo; moves "encaixam" no grid

// ─── HTTP ───────────────────────────────────────────────────────────────────
function authHeaders(json) {
  const h = state.token ? { Authorization: `Bearer ${state.token}` } : {};
  if (json) h['Content-Type'] = 'application/json';
  return h;
}

async function api(method, path, body) {
  const res = await fetch(API + path, {
    method,
    headers: authHeaders(body !== undefined),
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (res.status === 401) {
    location.assign('/login?next=' + encodeURIComponent(location.pathname + location.search));
    throw new Error('nao_autenticado');
  }
  if (!res.ok) {
    let msg = res.statusText;
    try { msg = (await res.json()).erro || msg; } catch (_) {}
    throw new Error(msg);
  }
  if (res.status === 204) return null;
  return res.json();
}

// ─── Canvas + render ──────────────────────────────────────────────────────────
const canvas = document.getElementById('canvas');
const ctx = canvas.getContext('2d');
let cw = 0, ch = 0, dpr = 1;

function resize() {
  dpr = window.devicePixelRatio || 1;
  cw = canvas.clientWidth;
  ch = canvas.clientHeight;
  canvas.width = cw * dpr;
  canvas.height = ch * dpr;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  draw();
}
window.addEventListener('resize', resize);

function worldToScreen(wx, wy) {
  return [cw / 2 + (wx - state.view.panX) * state.view.zoom,
          ch / 2 + (wy - state.view.panY) * state.view.zoom];
}
function screenToWorld(sx, sy) {
  return [state.view.panX + (sx - cw / 2) / state.view.zoom,
          state.view.panY + (sy - ch / 2) / state.view.zoom];
}

const LEX_COLOR = { local: '#6b6b78', linked: '#5b8def', contributed: '#4caf72' };

function elementById(id) {
  return state.room && state.room.elements.find((e) => e.id === id);
}
function nodeRadius() {
  // nós bem maiores e legíveis; não encolhem demais ao dar zoom out
  return 40 * Math.min(1.5, Math.max(0.85, state.view.zoom));
}
function snapToGrid(v) {
  return Math.round(v / GRID) * GRID;
}

function drawArrow(ax, ay, bx, by, nodeR) {
  const dx = bx - ax, dy = by - ay;
  const len = Math.hypot(dx, dy) || 1;
  const ux = dx / len, uy = dy / len;
  const tx = bx - ux * nodeR, ty = by - uy * nodeR; // ponta encosta na borda do nó
  const s = 9, w = 0.55;
  ctx.beginPath();
  ctx.moveTo(tx, ty);
  ctx.lineTo(tx - ux * s - uy * s * w, ty - uy * s + ux * s * w);
  ctx.lineTo(tx - ux * s + uy * s * w, ty - uy * s - ux * s * w);
  ctx.closePath();
  ctx.fill();
}

function draw() {
  ctx.clearRect(0, 0, cw, ch);
  if (!state.room) return;
  const z = state.view.zoom;

  // grade de fundo — alinhada ao grid de mundo (reforça "mapa editável")
  ctx.strokeStyle = 'rgba(212,175,55,0.07)';
  ctx.lineWidth = 1;
  const step = GRID * z;
  if (step > 10) {
    const [ox, oy] = worldToScreen(0, 0);
    for (let x = ((ox % step) + step) % step; x < cw; x += step) { ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, ch); ctx.stroke(); }
    for (let y = ((oy % step) + step) % step; y < ch; y += step) { ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(cw, y); ctx.stroke(); }
  }

  // relações (links) — desenhadas sob os nós
  const r = nodeRadius();
  ctx.textAlign = 'center';
  for (const l of (state.room.links || [])) {
    const a = elementById(l.from), b = elementById(l.to);
    if (!a || !b) continue;
    const [ax, ay] = worldToScreen(a.x, a.y);
    const [bx, by] = worldToScreen(b.x, b.y);
    const sel = l.id === state.selectedLinkId;
    ctx.strokeStyle = sel ? '#d4af37' : 'rgba(212,175,55,0.38)';
    ctx.fillStyle = ctx.strokeStyle;
    ctx.lineWidth = sel ? 3 : 1.5;
    ctx.beginPath(); ctx.moveTo(ax, ay); ctx.lineTo(bx, by); ctx.stroke();
    if (l.directed !== false) drawArrow(ax, ay, bx, by, r);
    if (l.label && z > 0.45) {
      ctx.fillStyle = sel ? '#d4af37' : '#8c8674';
      ctx.font = `${Math.max(9, 10 * z)}px system-ui`;
      ctx.fillText(l.label, (ax + bx) / 2, (ay + by) / 2 - 5);
    }
  }

  // nós (só a frase — sem ícones)
  const wordSize = Math.max(15, 20 * z);
  for (const el of state.room.elements) {
    const [sx, sy] = worldToScreen(el.x, el.y);
    if (sx < -160 || sx > cw + 160 || sy < -160 || sy > ch + 160) continue;
    const sel = el.id === state.selectedId;
    const linking = state.linkFrom === el.id;
    const color = LEX_COLOR[el.lexicon ? el.lexicon.state : 'local'] || LEX_COLOR.local;

    ctx.beginPath();
    ctx.arc(sx, sy, r, 0, Math.PI * 2);
    ctx.fillStyle = sel || linking ? 'rgba(212,175,55,0.20)' : 'rgba(22,22,30,0.95)';
    ctx.fill();
    ctx.lineWidth = sel || linking ? 3.5 : 2;
    ctx.strokeStyle = linking ? '#4caf72' : (sel ? '#d4af37' : color);
    ctx.stroke();

    ctx.fillStyle = '#f3eee0';
    ctx.font = `600 ${wordSize}px system-ui, sans-serif`;
    ctx.fillText(el.word, sx, sy + wordSize * 0.34);
    if (z > 0.7 && el.gloss) {
      ctx.fillStyle = '#a59d86';
      ctx.font = `${Math.max(11, 13 * z)}px system-ui`;
      const g = el.gloss.length > 32 ? el.gloss.slice(0, 30) + '…' : el.gloss;
      ctx.fillText(g, sx, sy + r + 16 * Math.min(1.4, z));
    }
  }
}

function elementAt(sx, sy) {
  if (!state.room) return null;
  const hit = nodeRadius() + 6;
  let best = null, bestD = hit * hit;
  for (const el of state.room.elements) {
    const [ex, ey] = worldToScreen(el.x, el.y);
    const d = (ex - sx) ** 2 + (ey - sy) ** 2;
    if (d < bestD) { bestD = d; best = el; }
  }
  return best;
}

// ─── Pointer (pan / drag / pinch) ─────────────────────────────────────────────
canvas.addEventListener('pointerdown', (e) => {
  canvas.setPointerCapture(e.pointerId);
  state.pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
  if (state.pointers.size === 2) { startPinch(); return; }
  const rect = canvas.getBoundingClientRect();
  const sx = e.clientX - rect.left, sy = e.clientY - rect.top;
  const el = elementAt(sx, sy);
  if (el && (state.canEdit || state.linkMode)) {
    // arrastar/mover só faz sentido se a sala é editável; em read-only,
    // o pointerdown vira só seleção (drag não inicia)
    const [wx, wy] = screenToWorld(sx, sy);
    state.drag = { kind: 'elem', id: el.id, ox: wx - el.x, oy: wy - el.y, moved: false, ro: !state.canEdit };
  } else if (el) {
    select(el.id); // read-only: clique seleciona e mostra a ficha
  } else {
    state.drag = { kind: 'pan', sx0: sx, sy0: sy, px0: state.view.panX, py0: state.view.panY, moved: false };
    canvas.classList.add('grabbing');
  }
});

canvas.addEventListener('pointermove', (e) => {
  if (!state.pointers.has(e.pointerId)) return;
  state.pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
  if (state.pinch) { updatePinch(); return; }
  if (!state.drag) return;
  const rect = canvas.getBoundingClientRect();
  const sx = e.clientX - rect.left, sy = e.clientY - rect.top;
  if (state.drag.kind === 'elem') {
    const [wx, wy] = screenToWorld(sx, sy);
    const el = state.room.elements.find((x) => x.id === state.drag.id);
    if (el) { el.x = wx - state.drag.ox; el.y = wy - state.drag.oy; state.drag.moved = true; draw(); }
  } else {
    const dx = sx - state.drag.sx0, dy = sy - state.drag.sy0;
    if (Math.abs(dx) + Math.abs(dy) > 3) state.drag.moved = true;
    state.view.panX = state.drag.px0 - dx / state.view.zoom;
    state.view.panY = state.drag.py0 - dy / state.view.zoom;
    draw();
  }
});

function endPointer(e) {
  state.pointers.delete(e.pointerId);
  if (state.pointers.size < 2) state.pinch = null;
  canvas.classList.remove('grabbing');
  const d = state.drag;
  state.drag = null;
  if (!d) return;
  if (d.kind === 'elem') {
    const el = state.room.elements.find((x) => x.id === d.id);
    if (d.moved && el) {
      // encaixa no grid e persiste (cada move é salvo)
      el.x = snapToGrid(el.x);
      el.y = snapToGrid(el.y);
      draw();
      cacheRoom();
      api('PATCH', `/salas/${state.roomId}`, { op: 'move_element', id: el.id, x: el.x, y: el.y })
        .then((room) => setRoom(room))
        .catch((err) => toast('Erro ao mover: ' + err.message));
    } else if (el) {
      if (state.linkMode) handleLinkClick(el.id);
      else select(el.id);
    }
  } else if (!d.moved) {
    if (state.linkMode) { state.linkFrom = null; draw(); }
    else {
      const lid = linkAt(d.sx0, d.sy0);
      if (lid) selectLink(lid); else select(null);
    }
  } else {
    saveViewportSoon();
  }
}

// Cursor de "mover" ao passar sobre um termo — deixa claro que o mapa é editável.
canvas.addEventListener('mousemove', (e) => {
  if (state.drag || state.pinch) return;
  const rect = canvas.getBoundingClientRect();
  const over = elementAt(e.clientX - rect.left, e.clientY - rect.top);
  canvas.style.cursor = state.linkMode ? 'crosshair' : (over ? 'move' : 'grab');
});

// ─── Relações (links) ──────────────────────────────────────────────────────────
function handleLinkClick(id) {
  if (!state.linkFrom) { state.linkFrom = id; draw(); toast('Agora escolha o segundo termo'); return; }
  if (state.linkFrom === id) { state.linkFrom = null; draw(); return; }
  const from = state.linkFrom, to = id;
  state.linkFrom = null;
  const label = (prompt('Rótulo da relação (ex.: compõe, funda, relacionado):') || '').trim();
  const lid = 'l' + Math.random().toString(36).slice(2, 9);
  api('PATCH', `/salas/${state.roomId}`, {
    op: 'add_link', link: { id: lid, from, to, label: label || null, directed: true },
  }).then((room) => { setRoom(room); toast('Relação criada'); })
    .catch((err) => { toast('Erro: ' + err.message); draw(); });
}

function distToSeg(px, py, ax, ay, bx, by) {
  const dx = bx - ax, dy = by - ay, len2 = dx * dx + dy * dy || 1;
  let t = ((px - ax) * dx + (py - ay) * dy) / len2;
  t = Math.max(0, Math.min(1, t));
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}
function linkAt(sx, sy) {
  if (!state.room) return null;
  for (const l of (state.room.links || [])) {
    const a = elementById(l.from), b = elementById(l.to);
    if (!a || !b) continue;
    const [ax, ay] = worldToScreen(a.x, a.y);
    const [bx, by] = worldToScreen(b.x, b.y);
    if (distToSeg(sx, sy, ax, ay, bx, by) < 8) return l.id;
  }
  return null;
}
function selectLink(id) {
  state.selectedLinkId = id;
  state.selectedId = null;
  inspector.classList.remove('open');
  draw();
  const l = state.room.links.find((x) => x.id === id);
  if (l) toast(`Relação "${l.label || '—'}" — tecle Delete para remover`);
}
function deleteLink(id) {
  api('PATCH', `/salas/${state.roomId}`, { op: 'delete_link', id })
    .then((room) => { state.selectedLinkId = null; setRoom(room); select(state.selectedId); toast('Relação removida'); })
    .catch((err) => toast('Erro: ' + err.message));
}
canvas.addEventListener('pointerup', endPointer);
canvas.addEventListener('pointercancel', endPointer);

canvas.addEventListener('wheel', (e) => {
  e.preventDefault();
  const rect = canvas.getBoundingClientRect();
  const sx = e.clientX - rect.left, sy = e.clientY - rect.top;
  const [wx, wy] = screenToWorld(sx, sy);
  const factor = Math.exp(-e.deltaY * 0.0015);
  zoomTo(state.view.zoom * factor, wx, wy, sx, sy);
}, { passive: false });

function zoomTo(newZoom, wx, wy, sx, sy) {
  newZoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, newZoom));
  // mantém o ponto de mundo (wx,wy) sob o cursor (sx,sy)
  state.view.zoom = newZoom;
  state.view.panX = wx - (sx - cw / 2) / newZoom;
  state.view.panY = wy - (sy - ch / 2) / newZoom;
  draw();
  saveViewportSoon();
}

function startPinch() {
  const pts = [...state.pointers.values()];
  const dist = Math.hypot(pts[0].x - pts[1].x, pts[0].y - pts[1].y);
  const rect = canvas.getBoundingClientRect();
  const cxs = (pts[0].x + pts[1].x) / 2 - rect.left, cys = (pts[0].y + pts[1].y) / 2 - rect.top;
  const [wx, wy] = screenToWorld(cxs, cys);
  state.pinch = { dist0: dist, zoom0: state.view.zoom, wx, wy };
  state.drag = null;
}
function updatePinch() {
  const pts = [...state.pointers.values()];
  if (pts.length < 2) return;
  const dist = Math.hypot(pts[0].x - pts[1].x, pts[0].y - pts[1].y);
  const rect = canvas.getBoundingClientRect();
  const cxs = (pts[0].x + pts[1].x) / 2 - rect.left, cys = (pts[0].y + pts[1].y) / 2 - rect.top;
  zoomTo(state.pinch.zoom0 * (dist / state.pinch.dist0), state.pinch.wx, state.pinch.wy, cxs, cys);
}

let vpTimer = null;
function saveViewportSoon() {
  if (!state.canEdit) return; // salas públicas não persistem câmera
  clearTimeout(vpTimer);
  vpTimer = setTimeout(() => {
    const v = state.view;
    api('PATCH', `/salas/${state.roomId}`, {
      op: 'set_viewport', pan_x: v.panX, pan_y: v.panY,
      zoom: Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, v.zoom)),
    }).catch(() => {});
  }, 600);
}

// ─── Sala (com cache local por sala) ───────────────────────────────────────────
const ROOM_CACHE = (id) => `comunicacao-room-${id}`;
const LAST_ROOM = 'comunicacao-last-room';

function cacheRoom() {
  if (!state.room) return;
  try {
    localStorage.setItem(ROOM_CACHE(state.room.id), JSON.stringify(state.room));
    localStorage.setItem(LAST_ROOM, state.room.id);
  } catch (_) {}
}

function mySub() {
  if (state.sub !== null) return state.sub;
  state.sub = '';
  if (state.token) {
    try {
      const p = JSON.parse(atob(state.token.split('.')[1].replace(/-/g, '+').replace(/_/g, '/')));
      state.sub = p.sub || '';
    } catch (_) {}
  }
  return state.sub;
}

// Mostra/oculta as ações conforme a sala é editável (dono) ou pública (read-only).
function applyMode() {
  const ro = !state.canEdit;
  const show = (id, on) => { const el = document.getElementById(id); if (el) el.style.display = on ? '' : 'none'; };
  show('btn-add', !ro);
  show('btn-link', !ro);
  show('btn-sugerir', ro);
  if (ro && state.linkMode) setLinkMode(false);
}

function centerOn(el) {
  state.view.panX = el.x;
  state.view.panY = el.y;
  if (state.view.zoom < 1) state.view.zoom = 1.3;
  select(el.id);
  draw();
}

function setRoom(room) {
  state.room = room;
  state.roomId = room.id;
  state.canEdit = !!room.owner && room.owner === mySub();
  if (room.viewport) {
    state.view = { panX: room.viewport.pan_x, panY: room.viewport.pan_y, zoom: room.viewport.zoom || 1 };
  }
  const ro = state.canEdit ? '' : ' · público (somente leitura)';
  document.getElementById('room-title').textContent = (room.title || 'Comunicação') + ro;
  applyMode();
  cacheRoom(); // salva cada estado da sala no cache local
  draw();
}

async function loadRoom(id) {
  // pinta o cache local imediatamente (instantâneo), depois sincroniza com o servidor
  try {
    const cached = localStorage.getItem(ROOM_CACHE(id));
    if (cached) setRoom(JSON.parse(cached));
  } catch (_) {}
  const room = await api('GET', `/salas/${id}`);
  setRoom(room);
}

async function createRoom(template) {
  const room = await api('POST', `/salas?template=${encodeURIComponent(template)}`);
  history.replaceState(null, '', `${location.pathname}?id=${room.id}`);
  setRoom(room);
  closeModal('salas-modal');
  refreshReviewCount();
}

function fitToContent() {
  if (!state.room || !state.room.elements.length) {
    state.view = { panX: 0, panY: 0, zoom: 1 }; draw(); saveViewportSoon(); return;
  }
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const el of state.room.elements) {
    minX = Math.min(minX, el.x); maxX = Math.max(maxX, el.x);
    minY = Math.min(minY, el.y); maxY = Math.max(maxY, el.y);
  }
  const w = Math.max(1, maxX - minX), h = Math.max(1, maxY - minY);
  const z = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.min(cw / (w + 200), ch / (h + 200))));
  state.view = { panX: (minX + maxX) / 2, panY: (minY + maxY) / 2, zoom: z };
  draw(); saveViewportSoon();
}

// ─── Inspector ─────────────────────────────────────────────────────────────────
const inspector = document.getElementById('inspector');
function select(id) {
  state.selectedId = id;
  draw();
  if (!id) { inspector.classList.remove('open'); return; }
  const el = state.room.elements.find((x) => x.id === id);
  if (!el) return;
  const lex = el.lexicon ? el.lexicon.state : 'local';
  const badge = document.getElementById('ins-badge');
  badge.className = 'badge ' + lex;
  badge.textContent = { local: 'só na sala', linked: 'ligado ao léxico', contributed: 'contribuído' }[lex];
  document.getElementById('ins-word').textContent = el.word;
  document.getElementById('ins-pron').textContent = el.pronunciation || '';
  const ro = !state.canEdit;
  for (const fid of ['f-word', 'f-pron', 'f-gloss', 'f-concept']) {
    document.getElementById(fid).disabled = ro;
  }
  document.getElementById('f-word').value = el.word || '';
  document.getElementById('f-pron').value = el.pronunciation || '';
  document.getElementById('f-gloss').value = el.gloss || '';
  document.getElementById('f-concept').value = el.concept || '';
  const refs = document.getElementById('ins-refs');
  refs.innerHTML = (el.refs || []).map((r) => `<a href="${r.url}" target="_blank" rel="noopener">↗ ${r.source}</a>`).join('');

  // relações deste elemento (✕ remover só quando editável)
  state.selectedLinkId = null;
  const linksEl = document.getElementById('ins-links');
  const mine = (state.room.links || []).filter((l) => l.from === id || l.to === id);
  linksEl.innerHTML = mine.length
    ? '<label>Relações</label>' + mine.map((l) => {
        const other = l.from === id ? l.to : l.from;
        const oe = elementById(other);
        const dir = l.directed === false ? '↔' : (l.from === id ? '→' : '←');
        const lbl = l.label ? ` · ${l.label}` : '';
        const del = ro ? '' : `<button class="link-del" data-link="${l.id}">✕</button>`;
        return `<div class="link-row"><span>${dir} ${oe ? oe.word : other}${lbl}</span>${del}</div>`;
      }).join('')
    : '';
  linksEl.querySelectorAll('.link-del').forEach((b) =>
    b.addEventListener('click', () => deleteLink(b.dataset.link)));

  const path = el.lexicon && el.lexicon.path;
  document.getElementById('ins-lexpath').textContent = path ? `léxico: ${path}` : '';
  // ações de edição só aparecem quando o usuário é dono da sala
  document.getElementById('row-edit').style.display = ro ? 'none' : '';
  document.getElementById('row-publish').style.display = ro ? 'none' : '';
  const pubBtn = document.getElementById('btn-publish');
  pubBtn.textContent = lex === 'local' ? 'Publicar no léxico →' : 'Republicar / revisar';
  inspector.classList.add('open');
}

document.getElementById('btn-save').addEventListener('click', async () => {
  if (!state.selectedId) return;
  try {
    const room = await api('PATCH', `/salas/${state.roomId}`, {
      op: 'edit_element', id: state.selectedId,
      word: document.getElementById('f-word').value.trim(),
      pronunciation: document.getElementById('f-pron').value.trim(),
      gloss: document.getElementById('f-gloss').value.trim(),
      concept: document.getElementById('f-concept').value.trim() || null,
    });
    setRoom(room); select(state.selectedId); toast('Salvo');
  } catch (err) { toast('Erro: ' + err.message); }
});

document.getElementById('btn-delete').addEventListener('click', async () => {
  if (!state.selectedId) return;
  try {
    const room = await api('PATCH', `/salas/${state.roomId}`, { op: 'delete_element', id: state.selectedId });
    setRoom(room); select(null); toast('Removido');
  } catch (err) { toast('Erro: ' + err.message); }
});

document.getElementById('btn-publish').addEventListener('click', async () => {
  if (!state.selectedId) return;
  try {
    const r = await api('POST', `/salas/${state.roomId}/elementos/${state.selectedId}/publicar`);
    const el = state.room.elements.find((x) => x.id === state.selectedId);
    if (el && r.elemento) el.lexicon = r.elemento.lexicon;
    select(state.selectedId);
    toast(r.criado ? `Criado no léxico: ${r.caminho}` : `Ligado a: ${r.caminho}`);
    refreshReviewCount();
  } catch (err) { toast('Erro: ' + err.message); }
});

// ─── Adicionar palavra ──────────────────────────────────────────────────────────
document.getElementById('btn-add').addEventListener('click', async () => {
  const word = prompt('Palavra na língua nativa:');
  if (!word || !word.trim()) return;
  const gloss = prompt('Glosa / tradução (opcional):') || '';
  const [wx, wy] = screenToWorld(cw / 2, ch / 2);
  const id = 'e' + Math.random().toString(36).slice(2, 9);
  const lang = (state.room && state.room.lang) || 'pt';
  try {
    const room = await api('PATCH', `/salas/${state.roomId}`, {
      op: 'add_element',
      element: { id, word: word.trim(), lang, x: wx, y: wy, gloss: gloss.trim() || null },
    });
    setRoom(room); select(id);
  } catch (err) { toast('Erro: ' + err.message); }
});

// ─── Modo de ligação ────────────────────────────────────────────────────────────
const btnLink = document.getElementById('btn-link');
function setLinkMode(on) {
  state.linkMode = on;
  state.linkFrom = null;
  btnLink.classList.toggle('primary', on);
  canvas.style.cursor = on ? 'crosshair' : 'grab';
  toast(on ? 'Modo ligação: clique dois termos para relacioná-los' : 'Modo ligação desligado');
  draw();
}
btnLink.addEventListener('click', () => setLinkMode(!state.linkMode));

// Teclas: Delete remove relação selecionada; Esc sai do modo ligação.
window.addEventListener('keydown', (e) => {
  if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;
  if ((e.key === 'Delete' || e.key === 'Backspace') && state.selectedLinkId) {
    e.preventDefault(); deleteLink(state.selectedLinkId);
  } else if (e.key === 'Escape') {
    if (state.linkMode) setLinkMode(false);
    else { state.selectedLinkId = null; select(null); }
  }
});

// ─── Salas (launcher) ────────────────────────────────────────────────────────────
document.getElementById('btn-salas').addEventListener('click', openSalas);
function salaItem(s, tag) {
  const div = document.createElement('div');
  div.className = 'sala-item';
  div.innerHTML = `<span>${s.title}</span><span class="hint">${s.lang} · ${s.elements.length} termos${tag || ''}</span>`;
  div.addEventListener('click', () => {
    history.replaceState(null, '', `${location.pathname}?id=${s.id}`);
    loadRoom(s.id); closeModal('salas-modal');
  });
  return div;
}
function hintEl(html) {
  const p = document.createElement('p'); p.className = 'hint'; p.innerHTML = html; return p;
}
async function openSalas() {
  openModal('salas-modal');
  const list = document.getElementById('salas-list');
  list.innerHTML = '<p class="hint">carregando…</p>';
  try {
    list.innerHTML = '';
    // léxicos públicos (visíveis sem login)
    const pub = (await api('GET', '/salas?published=true')).sort((a, b) => (a.id < b.id ? -1 : 1));
    if (pub.length) {
      list.appendChild(hintEl('<b>Léxicos públicos</b>'));
      pub.forEach((s) => list.appendChild(salaItem(s, ' · público')));
    }
    // minhas salas (se logado)
    if (state.token) {
      const own = (await api('GET', '/salas')).filter((s) => !pub.some((p) => p.id === s.id));
      list.appendChild(hintEl('<b>Suas salas</b>'));
      if (own.length) own.forEach((s) => list.appendChild(salaItem(s)));
      else list.appendChild(hintEl('Nenhuma ainda — abra um léxico público e clique <b>Sugerir</b>.'));
    } else {
      list.appendChild(hintEl(`<a href="/login?next=${encodeURIComponent(location.pathname)}">Entrar</a> para criar salas e sugerir termos.`));
    }
  } catch (err) { list.innerHTML = `<p class="hint">Erro: ${err.message}</p>`; }
}
document.querySelectorAll('#salas-modal [data-tpl]').forEach((b) =>
  b.addEventListener('click', () => createRoom(b.dataset.tpl)));

document.getElementById('btn-fit').addEventListener('click', fitToContent);

// ─── Sugerir (fork) + busca ─────────────────────────────────────────────────────
document.getElementById('btn-sugerir').addEventListener('click', async () => {
  if (!state.token) {
    location.assign('/login?next=' + encodeURIComponent(location.pathname + location.search));
    return;
  }
  try {
    const room = await api('POST', `/salas/${state.roomId}/fork`);
    history.replaceState(null, '', `${location.pathname}?id=${room.id}`);
    setRoom(room);
    toast('Cópia criada — agora você pode editar e sugerir termos');
  } catch (err) { toast('Erro: ' + err.message); }
});

document.getElementById('search').addEventListener('input', (e) => {
  const q = e.target.value.trim().toLowerCase();
  if (!q || !state.room) return;
  const el = state.room.elements.find((x) => x.word.toLowerCase().includes(q));
  if (el) centerOn(el);
});

// ─── Revisão ──────────────────────────────────────────────────────────────────
document.getElementById('btn-review').addEventListener('click', openReview);
async function openReview() {
  try {
    const data = await api('GET', '/revisao');
    state.review = { items: data.vencidos_agora || [], idx: 0, revealed: false };
    openModal('review-modal');
    showReviewCard();
  } catch (err) { toast('Erro: ' + err.message); }
}
function showReviewCard() {
  const { items, idx } = state.review;
  const wordEl = document.getElementById('rev-word');
  const glossEl = document.getElementById('rev-gloss');
  const progress = document.getElementById('rev-progress');
  if (idx >= items.length) {
    wordEl.textContent = items.length ? '✓ fim' : 'Nada vencido agora 🎉';
    glossEl.style.display = 'none';
    document.getElementById('rev-reveal-row').style.display = 'none';
    document.getElementById('rev-grade-row').style.display = 'none';
    progress.textContent = '';
    refreshReviewCount();
    return;
  }
  const it = items[idx];
  wordEl.textContent = it.word;
  glossEl.textContent = it.gloss || '(sem glosa)';
  glossEl.style.display = 'none';
  document.getElementById('rev-reveal-row').style.display = 'flex';
  document.getElementById('rev-grade-row').style.display = 'none';
  progress.textContent = `${idx + 1} / ${items.length} · ${it.lang}`;
  state.review.revealed = false;
}
document.getElementById('rev-reveal').addEventListener('click', () => {
  document.getElementById('rev-gloss').style.display = 'block';
  document.getElementById('rev-reveal-row').style.display = 'none';
  document.getElementById('rev-grade-row').style.display = 'flex';
});
async function grade(correct) {
  const it = state.review.items[state.review.idx];
  if (!it) return;
  try { await api('POST', '/revisao/nota', { term_path: it.term_path, correct }); }
  catch (err) { toast('Erro: ' + err.message); }
  state.review.idx += 1;
  showReviewCard();
}
document.getElementById('rev-right').addEventListener('click', () => grade(true));
document.getElementById('rev-wrong').addEventListener('click', () => grade(false));
document.getElementById('rev-close').addEventListener('click', () => closeModal('review-modal'));

async function refreshReviewCount() {
  if (!state.token) { document.getElementById('btn-review').style.display = 'none'; return; }
  try {
    const data = await api('GET', '/revisao');
    const n = data.vencidos || 0;
    document.getElementById('review-count').textContent = n ? `(${n})` : '';
  } catch (_) {}
}

// ─── Modais + toast ────────────────────────────────────────────────────────────
function openModal(id) { document.getElementById(id).classList.add('open'); }
function closeModal(id) { document.getElementById(id).classList.remove('open'); }
document.querySelectorAll('.modal').forEach((m) =>
  m.addEventListener('click', (e) => { if (e.target === m) m.classList.remove('open'); }));

let toastTimer = null;
function toast(msg) {
  const t = document.getElementById('toast');
  t.textContent = msg; t.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove('show'), 2600);
}

// ─── Boot ───────────────────────────────────────────────────────────────────────
async function boot() {
  // Salas públicas são visíveis SEM login; só sugerir/editar exige entrar.
  resize();
  const params = new URLSearchParams(location.search);
  const id = params.get('id');
  const template = params.get('template');
  const last = state.token ? localStorage.getItem(LAST_ROOM) : null;
  try {
    if (id) await loadRoom(id);
    else if (template && state.token) await createRoom(template);
    else if (last) { history.replaceState(null, '', `${location.pathname}?id=${last}`); await loadRoom(last); }
    else await openSalas();
  } catch (err) {
    if (err.message !== 'nao_autenticado') toast('Erro: ' + err.message);
    await openSalas();
  }
  refreshReviewCount();
}
boot();
