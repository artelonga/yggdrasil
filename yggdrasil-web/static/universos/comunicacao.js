// Universo "comunicação" — mapa interativo de léxico cross-linguístico.
// Pan/zoom, física e renderização delegados ao co_graph (co.artelonga.com.br/lib/co-graph.js).
// Esta camada mantém: API, salas, inspector, modos de edição e persistência.
'use strict';

const JWT_KEY = 'yggdrasil-jwt';
const API = '/api/v1/comunicacao';

const esc = (s) => String(s == null ? '' : s).replace(/[&<>"]/g, (c) =>
  ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

const state = {
  token: localStorage.getItem(JWT_KEY),
  sub: null,
  canEdit: false,
  roomId: null,
  room: null,
  selectedId: null,
  selectedLinkId: null,
  hoverId: null,
  // modo de ligação (relações)
  linkMode: false,
  linkFrom: null,
  // modo de composição (fractal)
  composeMode: false,
  composeParents: [],
  review: { items: [], idx: 0, revealed: false },
  lexTotal: 0,
};

const ZOOM_MIN = 0.1, ZOOM_MAX = 8;
const GRID = 60;

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

// ─── co_graph setup ──────────────────────────────────────────────────────────
const container = document.getElementById('app');

// Node colors by lexicon state
const LEX_COLOR = { local: '#6b6b78', linked: '#5b8def', contributed: '#4caf72' };
// Base node size (world units; scaled by zoom inside co_graph)
const BASE_NODE_SIZE = 8;

// Popularity rank factor: sala públicas têm id "e<rank>", rank 0 = mais popular.
const RANK_RE = /^e(\d+)$/;
function rankFactor(el) {
  const m = RANK_RE.exec(el.id);
  if (!m) return 1.4;
  const rank = +m[1];
  return 1 + 2.4 / (1 + rank / 35);
}

function elementById(id) {
  return state.room && state.room.elements.find((e) => e.id === id);
}

function toGraphData() {
  if (!state.room) return { nodes: [], edges: [] };
  return {
    nodes: state.room.elements.map((el) => ({
      id: el.id,
      label: el.word,
      sublabel: el.gloss || null,
      color: LEX_COLOR[el.lexicon ? el.lexicon.state : 'local'] || LEX_COLOR.local,
      size: BASE_NODE_SIZE * rankFactor(el),
      selected: el.id === state.selectedId || state.composeParents.includes(el.id),
      x: el.x,
      y: el.y,
    })),
    edges: (state.room.links || []).map((l) => ({
      source: l.from,
      target: l.to,
      kind: l.kind || 'default',
      label: l.label || null,
      directed: l.directed !== false,
      color: l.id === state.selectedLinkId
        ? '#d4af37'
        : l.kind === 'compoe'
          ? 'rgba(76,175,114,0.6)'
          : 'rgba(212,175,55,0.35)',
    })),
  };
}

// Inicializar co_graph no container
let handle = null;
handle = co_graph.render(container, {
  data: { nodes: [], edges: [] },
  layout: 'manual',
  grid: { size: GRID, color: 'rgba(212,175,55,0.06)' },
  onNodeClick: function (node, sx, sy) {
    if (state.composeMode) { toggleComposeParent(node.id); return; }
    if (state.linkMode)    { handleLinkClick(node.id);     return; }
    select(node.id);
  },
  onNodeHover: function (node) {
    const newId = node ? node.id : null;
    if (newId !== state.hoverId) {
      state.hoverId = newId;
      handle.update(toGraphData());
    }
  },
  onNodeMoveEnd: function (node) {
    if (!state.canEdit) return;
    const snappedX = Math.round(node.x / GRID) * GRID;
    const snappedY = Math.round(node.y / GRID) * GRID;
    const el = state.room.elements.find((e) => e.id === node.id);
    if (el) { el.x = snappedX; el.y = snappedY; }
    handle.update(toGraphData());
    cacheRoom();
    api('PATCH', `/salas/${state.roomId}`, { op: 'move_element', id: node.id, x: snappedX, y: snappedY })
      .then((room) => setRoom(room))
      .catch((err) => toast('Erro ao mover: ' + err.message));
  },
});

// Alias compat: alguns lugares usam worldToScreen/screenToWorld como funções livres.
function worldToScreen(wx, wy) { return handle.worldToScreen(wx, wy); }
function screenToWorld(sx, sy) { return handle.screenToWorld(sx, sy); }

// ─── Distância ponto-segmento (para seleção de relações) ─────────────────────
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

// Click em espaço vazio (co_graph já trata pan/zoom; precisamos apenas da seleção de links)
container.addEventListener('click', (e) => {
  if (!state.room) return;
  const rect = container.getBoundingClientRect();
  const sx = e.clientX - rect.left, sy = e.clientY - rect.top;
  // Se co_graph interceptou um clique em nó, callbacks de onNodeClick já rodaram.
  // Aqui tratamos clique em link ou espaço vazio.
  const n = handle.getNodeAt(sx, sy);
  if (n) return; // nó tratado pelo co_graph
  if (state.linkMode) { state.linkFrom = null; handle.update(toGraphData()); return; }
  const lid = linkAt(sx, sy);
  if (lid) { selectLink(lid); } else { select(null); }
});

// Cursor do mouse sobre nós/links
container.addEventListener('mousemove', (e) => {
  const rect = container.getBoundingClientRect();
  const sx = e.clientX - rect.left, sy = e.clientY - rect.top;
  const n = handle.getNodeAt(sx, sy);
  const canvas = container.querySelector('canvas');
  if (!canvas) return;
  canvas.style.cursor = state.composeMode ? 'cell'
    : state.linkMode ? 'crosshair'
    : (n ? (state.canEdit ? 'move' : 'pointer') : 'grab');
});

// ─── Relações (links) ─────────────────────────────────────────────────────────
function handleLinkClick(id) {
  if (!state.linkFrom) {
    state.linkFrom = id;
    handle.update(toGraphData());
    toast('Agora escolha o segundo termo');
    return;
  }
  if (state.linkFrom === id) { state.linkFrom = null; handle.update(toGraphData()); return; }
  const from = state.linkFrom, to = id;
  state.linkFrom = null;
  const label = (prompt('Rótulo da relação (ex.: compõe, funda, relacionado):') || '').trim();
  const lid = 'l' + Math.random().toString(36).slice(2, 9);
  api('PATCH', `/salas/${state.roomId}`, {
    op: 'add_link', link: { id: lid, from, to, label: label || null, directed: true },
  }).then((room) => { setRoom(room); toast('Relação criada'); })
    .catch((err) => { toast('Erro: ' + err.message); handle.update(toGraphData()); });
}

function selectLink(id) {
  state.selectedLinkId = id;
  state.selectedId = null;
  inspector.classList.remove('open');
  handle.update(toGraphData());
  const l = state.room.links.find((x) => x.id === id);
  if (l) toast(`Relação "${l.label || '—'}" — tecle Delete para remover`);
}

function deleteLink(id) {
  api('PATCH', `/salas/${state.roomId}`, { op: 'delete_link', id })
    .then((room) => { state.selectedLinkId = null; setRoom(room); select(state.selectedId); toast('Relação removida'); })
    .catch((err) => toast('Erro: ' + err.message));
}

// ─── Viewport ─────────────────────────────────────────────────────────────────
let vpTimer = null;
function saveViewportSoon() {
  if (!state.canEdit) return;
  clearTimeout(vpTimer);
  vpTimer = setTimeout(() => {
    const cam = handle.getCamera();
    const zoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, cam.s));
    api('PATCH', `/salas/${state.roomId}`, {
      op: 'set_viewport', pan_x: cam.x, pan_y: cam.y, zoom,
    }).catch(() => {});
  }, 600);
}

function fitToContent() {
  handle.fit(false);
  saveViewportSoon();
}

function centerOn(el) {
  const cam = handle.getCamera();
  handle.setCamera({ x: el.x, y: el.y, s: Math.max(cam.s, 1.3) }, true);
  select(el.id);
}

// ─── Salas ────────────────────────────────────────────────────────────────────
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

function applyMode() {
  const ro = !state.canEdit;
  const show = (id, on) => { const el = document.getElementById(id); if (el) el.style.display = on ? '' : 'none'; };
  show('btn-add', !ro);
  show('btn-link', !ro);
  show('btn-compor', !ro);
  show('btn-sugerir', ro);
  if (ro && state.linkMode) setLinkMode(false);
  if (ro && state.composeMode) setComposeMode(false);
  const paginated = !!state.room && state.room.template === 'publico';
  show('btn-more', paginated);
  show('lex-count', paginated);
  if (paginated) refreshLexCount();
}

function renderLexCount() {
  const el = document.getElementById('lex-count');
  if (el && state.room) el.textContent = `${state.room.elements.length} / ${state.lexTotal || '…'} termos`;
}

async function refreshLexCount() {
  renderLexCount();
  try {
    const data = await api('GET', `/lexico/lista?lang=${encodeURIComponent(state.room.lang)}&offset=0&limit=0`);
    state.lexTotal = data.total;
    renderLexCount();
  } catch (_) {}
}

async function loadMore() {
  if (!state.room) return;
  const loaded = state.room.elements.length;
  if (state.lexTotal && loaded >= state.lexTotal) { toast('Todos os termos já carregados'); return; }
  try {
    const data = await api('GET', `/lexico/lista?lang=${encodeURIComponent(state.room.lang)}&offset=${loaded}&limit=100`);
    const have = new Set(state.room.elements.map((e) => e.id));
    for (const en of data.entries) {
      const id = 'e' + en.index;
      if (have.has(id)) continue;
      state.room.elements.push({ id, word: en.word, lang: en.lang, x: en.x, y: en.y, gloss: en.gloss || null, pronunciation: en.pron || null, decomp: en.decomp || null });
    }
    state.lexTotal = data.total;
    cacheRoom();
    renderLexCount();
    handle.update(toGraphData());
  } catch (err) { toast('Erro: ' + err.message); }
}

function setRoom(room) {
  state.room = room;
  state.roomId = room.id;
  state.canEdit = !!room.owner && room.owner === mySub();
  if (room.viewport) {
    const vp = room.viewport;
    handle.setCamera({ x: vp.pan_x, y: vp.pan_y, s: Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, vp.zoom || 1)) }, false);
  }
  const ro = state.canEdit ? '' : ' · público (somente leitura)';
  document.getElementById('room-title').textContent = (room.title || 'Comunicação') + ro;
  applyMode();
  cacheRoom();
  handle.update(toGraphData());
  renderPopulares();
}

// YG-141: painel "mais populares" — rank + barra de popularidade relativa. O
// léxico em espiral só mostrava pontos; aqui a popularidade fica legível e
// clicável (centra o nó). Aparece só em salas de léxico (elementos com `pop`).
function renderPopulares() {
  const host = document.getElementById('populares');
  if (!host) return;
  const els = (state.room && state.room.elements) || [];
  const comPop = els.filter((e) => typeof e.pop === 'number');
  if (comPop.length < 2) { host.innerHTML = ''; host.style.display = 'none'; return; }
  host.style.display = '';
  const ranked = comPop.slice().sort((a, b) => b.pop - a.pop).slice(0, 30);
  const max = ranked[0].pop || 1;
  host.innerHTML = '<div class="pop-h">★ mais populares</div>' +
    ranked.map((e, i) => {
      const w = Math.max(3, Math.round((e.pop / max) * 100));
      return '<div class="pop-row" data-id="' + esc(e.id) + '" title="' + esc(e.gloss || '') + '">' +
        '<span class="pop-rank">' + (i + 1) + '</span>' +
        '<span class="pop-word">' + esc(e.word) + '</span>' +
        '<span class="pop-bar"><i style="width:' + w + '%"></i></span>' +
        '<span class="pop-n">' + e.pop + '</span></div>';
    }).join('');
  host.querySelectorAll('.pop-row').forEach((r) => {
    r.addEventListener('click', () => {
      const el = elementById(r.dataset.id);
      if (el) { handle.setCamera({ x: el.x, y: el.y, s: 1.6 }, true); select(el.id); }
    });
  });
}

async function loadRoom(id) {
  try {
    const cached = localStorage.getItem(ROOM_CACHE(id));
    if (cached) setRoom(JSON.parse(cached));
  } catch (_) {}
  const room = await api('GET', `/salas/${id}`);
  setRoom(room);
  renderLexChips();
}

async function createRoom(template) {
  const room = await api('POST', `/salas?template=${encodeURIComponent(template)}`);
  history.replaceState(null, '', `${location.pathname}?id=${room.id}`);
  setRoom(room);
  closeModal('salas-modal');
  refreshReviewCount();
}

// ─── Inspector ────────────────────────────────────────────────────────────────
const inspector = document.getElementById('inspector');
function select(id) {
  state.selectedId = id;
  state.selectedLinkId = null;
  handle.update(toGraphData());
  if (!id) { inspector.classList.remove('open'); return; }
  const el = state.room.elements.find((x) => x.id === id);
  if (!el) return;
  const lex = el.lexicon ? el.lexicon.state : 'local';
  const badge = document.getElementById('ins-badge');
  badge.className = 'badge ' + lex;
  badge.textContent = { local: 'só na sala', linked: 'ligado ao léxico', contributed: 'contribuído' }[lex];
  document.getElementById('ins-word').textContent = el.word;
  document.getElementById('ins-pron').textContent = el.pronunciation || '';
  // Decomposição morfológica em partículas (estudo Ayvu Rapyta / NOTAS Cadogan).
  const decompEl = document.getElementById('ins-decomp');
  if (decompEl) {
    decompEl.textContent = el.decomp ? ('partículas: ' + el.decomp) : '';
    decompEl.style.display = el.decomp ? 'block' : 'none';
  }
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

// ─── Adicionar palavra ────────────────────────────────────────────────────────
document.getElementById('btn-add').addEventListener('click', async () => {
  const word = prompt('Palavra na língua nativa:');
  if (!word || !word.trim()) return;
  const gloss = prompt('Glosa / tradução (opcional):') || '';
  const containerRect = container.getBoundingClientRect();
  const [wx, wy] = handle.screenToWorld(containerRect.width / 2, containerRect.height / 2);
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

// ─── Modo de ligação ──────────────────────────────────────────────────────────
const btnLink = document.getElementById('btn-link');
function setLinkMode(on) {
  state.linkMode = on;
  state.linkFrom = null;
  btnLink.classList.toggle('primary', on);
  const canvas = container.querySelector('canvas');
  if (canvas) canvas.style.cursor = on ? 'crosshair' : 'grab';
  toast(on ? 'Modo ligação: clique dois termos para relacioná-los' : 'Modo ligação desligado');
  handle.update(toGraphData());
}
btnLink.addEventListener('click', () => setLinkMode(!state.linkMode));

// ─── Modo de composição ───────────────────────────────────────────────────────
const btnCompor = document.getElementById('btn-compor');
function setComposeMode(on) {
  state.composeMode = on;
  state.composeParents = [];
  if (on && state.linkMode) setLinkMode(false);
  btnCompor.classList.toggle('primary', on);
  btnCompor.textContent = on ? 'Compor (0)' : 'Compor';
  const canvas = container.querySelector('canvas');
  if (canvas) canvas.style.cursor = on ? 'cell' : 'grab';
  toast(on ? 'Escolha os termos-parente; depois clique Compor' : 'Composição cancelada');
  handle.update(toGraphData());
}
function toggleComposeParent(id) {
  const i = state.composeParents.indexOf(id);
  if (i >= 0) state.composeParents.splice(i, 1);
  else state.composeParents.push(id);
  btnCompor.textContent = `Compor (${state.composeParents.length})`;
  handle.update(toGraphData());
}
async function finalizeCompose() {
  const parents = state.composeParents.slice();
  if (!parents.length) { setComposeMode(false); return; }
  const words = parents.map((pid) => (elementById(pid) || {}).word).filter(Boolean);
  const word = prompt('Novo termo composto (a partir de: ' + words.join(' + ') + '):', words.join(' '));
  if (!word || !word.trim()) return;
  let cx = 0, cy = 0, n = 0;
  for (const pid of parents) { const e = elementById(pid); if (e) { cx += e.x; cy += e.y; n++; } }
  cx = n ? cx / n : 0; cy = (n ? cy / n : 0) - 120;
  const id = 'c' + Math.random().toString(36).slice(2, 9);
  try {
    const room = await api('PATCH', `/salas/${state.roomId}`, {
      op: 'compose', id, word: word.trim(), lang: state.room.lang, x: cx, y: cy, parents,
    });
    setComposeMode(false);
    setRoom(room);
    select(id);
    toast('Termo composto criado (ligado aos parents)');
  } catch (err) { toast('Erro: ' + err.message); }
}
btnCompor.addEventListener('click', () => {
  if (state.composeMode) finalizeCompose(); else setComposeMode(true);
});

window.addEventListener('keydown', (e) => {
  if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;
  if ((e.key === 'Delete' || e.key === 'Backspace') && state.selectedLinkId) {
    e.preventDefault(); deleteLink(state.selectedLinkId);
  } else if (e.key === 'Escape') {
    if (state.composeMode) setComposeMode(false);
    else if (state.linkMode) setLinkMode(false);
    else { state.selectedLinkId = null; select(null); }
  }
});

// ─── Salas (launcher) ─────────────────────────────────────────────────────────
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
    const pub = (await api('GET', '/salas?published=true')).sort((a, b) => (a.id < b.id ? -1 : 1));
    if (pub.length) {
      list.appendChild(hintEl('<b>Léxicos públicos</b>'));
      pub.forEach((s) => list.appendChild(salaItem(s, ' · público')));
    }
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

// ─── Sugerir (fork) + busca ───────────────────────────────────────────────────
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

document.getElementById('btn-more').addEventListener('click', loadMore);

// ─── Revisão ─────────────────────────────────────────────────────────────────
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

// ─── Modais + toast ───────────────────────────────────────────────────────────
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

// ─── Léxicos sempre à mão (YG-132) ───────────────────────────────────────────
// O modal "Salas" não bastava: quem caía numa sala (ou era devolvido à última
// pelo localStorage) ficava preso nela. Chips fixos na toolbar: um por léxico
// público + o corpus do Ayvu Rapyta.
let _publicas = [];
async function renderLexChips() {
  const el = document.getElementById('lexicos');
  if (!el) return;
  try {
    if (!_publicas.length) _publicas = await api('GET', '/salas?published=true');
  } catch (_) { /* sem rede → sem chips */ }
  const NOME = { 'gn-mbya': 'Mbyá', yo: 'Iorubá', 'pt-br': 'Português', pt: 'Português' };
  const chip = (rotulo, ativo, onclick, href) => {
    const a = document.createElement('a');
    a.textContent = rotulo;
    a.style.cssText = 'cursor:pointer;padding:.2rem .6rem;border-radius:999px;font-size:.72rem;' +
      (ativo ? 'background:#d4af37;color:#1a1408;font-weight:700'
             : 'border:1px solid #3a3a4a;color:#c9c9d6');
    if (href) a.href = href; else a.onclick = onclick;
    return a;
  };
  el.innerHTML = '';
  _publicas.forEach((sala) => {
    el.appendChild(chip(
      NOME[sala.lang] || sala.lang,
      state.room && state.room.id === sala.id,
      () => { history.replaceState(null, '', `${location.pathname}?id=${sala.id}`); loadRoom(sala.id); },
    ));
  });
  el.appendChild(chip('📜 Ayvu Rapyta', false, null, '/universos/corpus'));
}

// ─── Boot ─────────────────────────────────────────────────────────────────────
async function boot() {
  const params = new URLSearchParams(location.search);
  const id = params.get('id');
  const template = params.get('template');
  const last = state.token ? localStorage.getItem(LAST_ROOM) : null;
  try {
    if (id) await loadRoom(id);
    else if (template && state.token) await createRoom(template);
    else if (last) { history.replaceState(null, '', `${location.pathname}?id=${last}`); await loadRoom(last); }
    else {
      // YG-133: sem sala na URL/histórico, cai direto no primeiro léxico
      // público (Mbyá ordena primeiro) — nada de modal cobrindo a página;
      // os chips (YG-132) deixam a troca sempre visível.
      _publicas = await api('GET', '/salas?published=true');
      const primeira = _publicas.slice().sort((a, b) => (a.id < b.id ? -1 : 1))[0];
      if (primeira) {
        history.replaceState(null, '', `${location.pathname}?id=${primeira.id}`);
        await loadRoom(primeira.id);
      } else await openSalas();
    }
  } catch (err) {
    if (err.message !== 'nao_autenticado') toast('Erro: ' + err.message);
    await openSalas();
  }
  renderLexChips();
  refreshReviewCount();
}
boot();
