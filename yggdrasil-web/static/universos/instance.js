// Player + editor genérico de instâncias de universo (YG-78 + YG-81).
//
// Renderiza uma UniverseInstance client-side: camadas z-ordenadas com opacity,
// projeção 2d/iso, blocos e conexões. Sem WASM — é dado estático com toggles.
// Em modo edição (dono autenticado) aplica EditOps via PATCH.

'use strict';

const JWT_KEY = 'yggdrasil-jwt';
const API = '/api/v1';

const state = {
  id: location.pathname.split('/').pop(),
  token: localStorage.getItem(JWT_KEY),
  me: null,
  inst: null,
  template: null,
  edit: false,
  selectedType: null,   // block_type da paleta
  selectedBlock: null,  // id do bloco selecionado
  connectFrom: null,    // origem pendente de conexão
  deleteMode: false,
  images: {},           // hash -> HTMLImageElement (object URLs)
  drag: null,           // { blockId, layerId }
  notes: [],            // [{ slug, title, body, links, ... }]
  backlinks: {},        // slug -> [{ source, label }]
  graph: {},            // slug -> [slug-alvo] (wikilinks resolvidos)
  noteQuery: '',        // busca client-side sobre a sidebar Notas
  graphView: false,     // visão grafo (notas-como-nós + arestas de wikilink)
  tl: { off: 0, scale: 1 }, // pan/zoom do eixo X (projection: timeline, YG-123)
  tlPan: null,          // drag-pan em curso na timeline
  // visibilidade por tipo de ligação (legenda da sidebar). `sibling` é
  // derivado: filhos da mesma pasta — desligado por padrão para não poluir.
  linkFilter: { parent: true, ref: true, wikilink: true, sibling: false },
};

const canvas = document.getElementById('canvas');
const ctx = canvas.getContext('2d');

function authHeaders() {
  return state.token ? { Authorization: `Bearer ${state.token}` } : {};
}

function decodeSub(token) {
  try { return JSON.parse(atob(token.split('.')[1])).sub; } catch { return null; }
}

function toast(msg) {
  const t = document.getElementById('toast');
  t.textContent = msg;
  t.classList.add('show');
  setTimeout(() => t.classList.remove('show'), 2200);
}

// ─── Carregamento ────────────────────────────────────────────────────────────

async function load() {
  if (state.token) state.me = decodeSub(state.token);
  const res = await fetch(`${API}/instances/${state.id}`, { headers: authHeaders() });
  if (!res.ok) {
    document.getElementById('title').textContent = res.status === 404
      ? 'Universo não encontrado'
      : 'Acesso negado';
    return;
  }
  state.inst = await res.json();
  document.getElementById('title').textContent = state.inst.title || 'Universo';
  document.getElementById('desc').textContent = state.inst.description || '';

  // dono autenticado vê o botão de edição
  if (state.me && state.me === state.inst.owner) {
    document.getElementById('modeBtn').hidden = false;
    // Universo recém-criado (sem blocos): entra direto em modo edição —
    // senão o primeiro clique na grade não faz nada e parece quebrado.
    const temBlocos = (state.inst.layers || []).some((l) => (l.blocks || []).length);
    if (!temBlocos && !state.edit) toggleEditMode(true);
  }
  if (state.inst.template) {
    try {
      const tr = await fetch(`${API}/templates/${state.inst.template}`);
      if (tr.ok) state.template = await tr.json();
    } catch { /* paleta opcional */ }
  }

  await loadImages();
  await loadNotes();
  sizeCanvas();
  renderLayers();
  renderPalette();
  renderLinkLegend();
  renderNotesList();
  wireNoteSearch();
  wireGraphToggle();
  render();
  wireNoteEditor();
  openFromHash();
}

async function loadNotes() {
  try {
    const r = await fetch(`${API}/instances/${state.id}/notes`, { headers: authHeaders() });
    if (!r.ok) return;
    const d = await r.json();
    state.notes = d.notes || [];
    state.backlinks = d.backlinks || {};
    state.graph = d.graph || {};
  } catch { /* notas opcionais */ }
}

function noteBySlug(slug) { return state.notes.find((n) => n.slug === slug) || null; }

// Bloco-nota (de qualquer camada) cujo props.note_slug bate com o slug.
function noteBlock(slug) {
  for (const { block } of allBlocks()) {
    if (block.props?.note_slug === slug) return block;
  }
  return null;
}

function noteTitle(slug) { return noteBySlug(slug)?.title || slug; }

// ─── Markdown + slug (espelha o slugify do servidor para ASCII/PT) ────────────

function slugifyJs(s) {
  return (s || '').toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '')
    .replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
}

function escapeHtml(s) {
  return (s || '').replace(/&/g, '&amp;').replace(/</g, '&lt;')
    .replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

// Renderizador Markdown mínimo. Escapa HTML PRIMEIRO (anti-XSS), depois aplica
// wikilinks, inline e blocos. Sem dependência externa (canvas/vanilla, CLAUDE.md).
function renderMarkdown(src) {
  let s = escapeHtml(src);
  // wikilinks [[alvo|alias]] / [[alvo]] — marca .missing se a nota não existe
  s = s.replace(/\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_m, target, alias) => {
    const slug = slugifyJs(target.trim());
    const text = escapeHtml((alias || target).trim());
    const missing = noteBySlug(slug) ? '' : ' missing';
    return `<a class="wikilink${missing}" data-slug="${slug}">${text}</a>`;
  });
  s = s.replace(/`([^`]+)`/g, '<code>$1</code>');
  s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  s = s.replace(/(^|[^*])\*([^*]+)\*/g, '$1<em>$2</em>');
  s = s.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');
  // blocos linha-a-linha
  const out = [];
  let inUl = false;
  const closeUl = () => { if (inUl) { out.push('</ul>'); inUl = false; } };
  for (const line of s.split(/\r?\n/)) {
    const h = line.match(/^(#{1,3})\s+(.*)$/);
    if (h) { closeUl(); out.push(`<h${h[1].length + 2}>${h[2]}</h${h[1].length + 2}>`); continue; }
    const li = line.match(/^[-*]\s+(.*)$/);
    if (li) { if (!inUl) { out.push('<ul>'); inUl = true; } out.push(`<li>${li[1]}</li>`); continue; }
    if (line.trim() === '') { closeUl(); continue; }
    out.push(`<p>${line}</p>`);
  }
  closeUl();
  return out.join('');
}

async function loadImages() {
  const hashes = new Set();
  for (const l of state.inst.layers) {
    if (l.background) hashes.add(l.background.hash);
  }
  await Promise.all([...hashes].map(async (hash) => {
    try {
      const r = await fetch(`${API}/instances/${state.id}/attachments/${hash}`, { headers: authHeaders() });
      if (!r.ok) return;
      const blob = await r.blob();
      const img = new Image();
      img.src = URL.createObjectURL(blob);
      await img.decode().catch(() => {});
      state.images[hash] = img;
    } catch { /* ignora imagem faltante */ }
  }));
}

// ─── Geometria ───────────────────────────────────────────────────────────────

function gridSpec() { return state.inst.grid; }

function isIso() { return state.inst.projection === 'isometric'; }
// YG-123: mundo-timeline — eixo X = tempo, pan/zoom só no X.
function isTimeline() { return state.inst.projection === 'timeline'; }
const TL_AXIS_H = 28; // faixa do eixo de tempo no rodapé

function cellToScreen(x, y) {
  const g = gridSpec();
  const c = g.cell_size;
  if (isIso()) {
    const originX = g.height * c / 2;
    return { sx: (x - y) * (c / 2) + originX, sy: (x + y) * (c / 4) + c };
  }
  if (isTimeline()) {
    // pan/zoom horizontal: o pitch das colunas escala, a célula não distorce
    return { sx: x * c * state.tl.scale + state.tl.off, sy: y * c };
  }
  return { sx: x * c, sy: y * c };
}

function cellCenter(x, y) {
  const c = gridSpec().cell_size;
  const p = cellToScreen(x, y);
  return isIso() ? { cx: p.sx, cy: p.sy + c / 4 } : { cx: p.sx + c / 2, cy: p.sy + c / 2 };
}

function screenToCell(mx, my) {
  const g = gridSpec();
  const c = g.cell_size;
  if (isIso()) {
    const originX = g.height * c / 2;
    const ix = (mx - originX) / (c / 2);
    const iy = (my - c) / (c / 4);
    const x = Math.round((ix + iy) / 2);
    const y = Math.round((iy - ix) / 2);
    return { x, y };
  }
  if (isTimeline()) {
    return { x: Math.floor((mx - state.tl.off) / (c * state.tl.scale)), y: Math.floor(my / c) };
  }
  return { x: Math.floor(mx / c), y: Math.floor(my / c) };
}

function sizeCanvas() {
  const g = gridSpec();
  const c = g.cell_size;
  if (isIso()) {
    canvas.width = (g.width + g.height) * (c / 2);
    canvas.height = (g.width + g.height) * (c / 4) + 2 * c;
  } else if (isTimeline()) {
    // viewport fixo (pan/zoom em vez de canvas gigante) + faixa do eixo
    canvas.width = Math.min(g.width * c, 1100);
    canvas.height = g.height * c + TL_AXIS_H;
  } else {
    canvas.width = g.width * c;
    canvas.height = g.height * c;
  }
}

// ─── Render ──────────────────────────────────────────────────────────────────

function allBlocks() {
  const out = [];
  for (const l of state.inst.layers) {
    if (l.kind === 'background') continue;
    for (const b of l.blocks) out.push({ block: b, layer: l });
  }
  return out;
}

function findBlock(id) {
  for (const l of state.inst.layers) {
    const b = l.blocks.find((b) => b.id === id);
    if (b) return { block: b, layer: l };
  }
  return null;
}

function render() {
  if (state.graphView) { renderGraph(); return; }
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  const g = gridSpec();
  const c = g.cell_size;

  // camadas em z-order (índice 0 = base)
  for (const l of state.inst.layers) {
    if (!l.visible) continue;
    ctx.globalAlpha = l.opacity;
    if (l.kind === 'background' && l.background && state.images[l.background.hash]) {
      const img = state.images[l.background.hash];
      // cobre a área da grade preservando proporção
      drawCover(img, 0, 0, g.width * c, g.height * c);
    }
  }
  ctx.globalAlpha = 1;

  if (!isIso() && (!state.template || state.template.render_hints?.grid_lines)) drawGridLines();

  // conexões (sob os blocos), separadas por tipo via legenda (state.linkFilter)
  for (const conn of state.inst.connections) {
    const kind = connKind(conn);
    if (state.linkFilter[kind] === false) continue;
    const a = findBlock(conn.from), b = findBlock(conn.to);
    if (!a || !b) continue;
    const p = cellCenter(a.block.pos.x, a.block.pos.y);
    const q = cellCenter(b.block.pos.x, b.block.pos.y);
    drawEdge(p, q, conn.directed, conn.label, kind);
  }

  // arestas derivadas: irmãos (filhos da mesma pasta) — "ser filho de um pai
  // comum" é um tipo de ligação por si só; desligado por padrão na legenda.
  if (state.linkFilter.sibling) {
    for (const grupo of Object.values(childrenByParent())) {
      for (let i = 0; i < grupo.length; i++) {
        for (let j = i + 1; j < grupo.length; j++) {
          drawSiblingEdge(
            cellCenter(grupo[i].pos.x, grupo[i].pos.y),
            cellCenter(grupo[j].pos.x, grupo[j].pos.y));
        }
      }
    }
  }

  // arestas de wikilink entre blocos-nota (tracejadas, derivadas do grafo)
  if (state.linkFilter.wikilink) {
    for (const [from, targets] of Object.entries(state.graph || {})) {
      const a = noteBlock(from);
      if (!a) continue;
      for (const to of targets) {
        const b = noteBlock(to);
        if (!b || b.id === a.id) continue;
        drawWikiEdge(cellCenter(a.pos.x, a.pos.y), cellCenter(b.pos.x, b.pos.y));
      }
    }
  }

  // blocos
  for (const { block } of allBlocks()) drawBlock(block);

  if (isTimeline()) drawTimeAxis();
}

// ─── Timeline (YG-123): eixo de tempo + zoom ─────────────────────────────────

// Intervalo [min, max] dos `props.at_iso` dos blocos visíveis.
function tlRange() {
  let min = Infinity, max = -Infinity;
  for (const { block } of allBlocks()) {
    const t = Date.parse(block.props?.at_iso || '');
    if (!Number.isFinite(t)) continue;
    if (t < min) min = t;
    if (t > max) max = t;
  }
  return min <= max ? { min, max } : null;
}

// Eixo de tempo no rodapé: linha + ~6 ticks com datas interpoladas entre o
// primeiro e o último evento (mesma interpolação linear do gerador x_for).
function drawTimeAxis() {
  const g = gridSpec();
  const c = g.cell_size;
  const yAxis = g.height * c + 4;
  const range = tlRange();

  ctx.save();
  ctx.fillStyle = '#14141b';
  ctx.fillRect(0, g.height * c, canvas.width, TL_AXIS_H);
  ctx.strokeStyle = '#45474c';
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(0, yAxis);
  ctx.lineTo(canvas.width, yAxis);
  ctx.stroke();
  if (!range) { ctx.restore(); return; }

  const span = range.max - range.min;
  const fmt = new Intl.DateTimeFormat('pt-BR', span > 90 * 86400e3
    ? { month: 'short', year: 'numeric' }
    : { day: '2-digit', month: 'short' });
  ctx.fillStyle = '#c5c6cc';
  ctx.font = '10px system-ui';
  ctx.textAlign = 'center';
  const ticks = 6;
  for (let i = 0; i < ticks; i++) {
    const frac = ticks === 1 ? 0 : i / (ticks - 1);
    // mesma régua do gerador: coluna 0..width-1 ↔ min..max
    const col = frac * (g.width - 1);
    const px = (col + 0.5) * c * state.tl.scale + state.tl.off;
    if (px < -40 || px > canvas.width + 40) continue;
    ctx.strokeStyle = '#45474c';
    ctx.beginPath();
    ctx.moveTo(px, yAxis);
    ctx.lineTo(px, yAxis + 5);
    ctx.stroke();
    ctx.fillText(fmt.format(new Date(range.min + frac * span)), px, yAxis + 17);
  }
  ctx.restore();
}

// Zoom no eixo X ancorado no cursor (scroll/trackpad).
canvas.addEventListener('wheel', (e) => {
  if (!state.inst || !isTimeline() || state.graphView) return;
  e.preventDefault();
  const { mx } = mouseXY(e);
  const f = e.deltaY < 0 ? 1.15 : 1 / 1.15;
  const novo = Math.min(8, Math.max(0.3, state.tl.scale * f));
  state.tl.off = mx - (mx - state.tl.off) * (novo / state.tl.scale);
  state.tl.scale = novo;
  render();
}, { passive: false });

function drawCover(img, dx, dy, dw, dh) {
  const ir = img.width / img.height, dr = dw / dh;
  let w = dw, h = dh, ox = dx, oy = dy;
  if (ir > dr) { h = dh; w = dh * ir; ox = dx + (dw - w) / 2; }
  else { w = dw; h = dw / ir; oy = dy + (dh - h) / 2; }
  ctx.drawImage(img, ox, oy, w, h);
}

function drawGridLines() {
  const g = gridSpec(), c = g.cell_size;
  ctx.strokeStyle = '#15151f';
  ctx.lineWidth = 1;
  for (let x = 0; x <= g.width; x++) {
    ctx.beginPath(); ctx.moveTo(x * c, 0); ctx.lineTo(x * c, g.height * c); ctx.stroke();
  }
  for (let y = 0; y <= g.height; y++) {
    ctx.beginPath(); ctx.moveTo(0, y * c); ctx.lineTo(g.width * c, y * c); ctx.stroke();
  }
}

function drawBlock(b) {
  const c = gridSpec().cell_size;
  const { cx, cy } = cellCenter(b.pos.x, b.pos.y);
  const r = c * 0.45;
  const color = b.props?.color || '#d4af37';
  const icon = b.props?.icon || '■';
  ctx.beginPath();
  ctx.arc(cx, cy, r, 0, Math.PI * 2);
  ctx.fillStyle = color + '33';
  ctx.fill();
  ctx.lineWidth = b.id === state.selectedBlock ? 3 : 1.5;
  ctx.strokeStyle = b.id === state.selectedBlock ? '#fff' : color;
  ctx.stroke();
  ctx.fillStyle = '#fff';
  ctx.font = `${Math.round(c * 0.7)}px system-ui`;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText(icon, cx, cy);
  if (b.label) {
    ctx.font = `${Math.round(c * 0.42)}px system-ui`;
    ctx.fillStyle = '#e8e3d3';
    ctx.fillText(b.label, cx, cy + r + c * 0.35);
  }
}

// Estilo por tipo de ligação (mesma chave da legenda/state.linkFilter).
const EDGE_STYLE = {
  parent: { stroke: '#e9c349cc', width: 2.5, text: '#e9c349' },
  ref:    { stroke: '#7ec8e3aa', width: 2,   text: '#7ec8e3' },
};

function connKind(conn) {
  return (conn.props && conn.props.kind) || 'ref';
}

// Filhos agrupados por pasta-pai (conexões `parent`): pai-id -> [blocos-filho].
function childrenByParent() {
  const grupos = {};
  for (const conn of state.inst.connections) {
    if (connKind(conn) !== 'parent') continue;
    const filho = findBlock(conn.from), pai = findBlock(conn.to);
    if (!filho || !pai) continue;
    (grupos[pai.block.id] = grupos[pai.block.id] || []).push(filho.block);
  }
  return grupos;
}

function drawEdge(p, q, directed, label, kind) {
  const st = EDGE_STYLE[kind] || EDGE_STYLE.ref;
  ctx.strokeStyle = st.stroke;
  ctx.lineWidth = st.width;
  ctx.beginPath();
  ctx.moveTo(p.cx, p.cy);
  ctx.lineTo(q.cx, q.cy);
  ctx.stroke();
  if (directed) {
    const ang = Math.atan2(q.cy - p.cy, q.cx - p.cx);
    const ah = 8;
    ctx.beginPath();
    ctx.moveTo(q.cx, q.cy);
    ctx.lineTo(q.cx - ah * Math.cos(ang - 0.4), q.cy - ah * Math.sin(ang - 0.4));
    ctx.lineTo(q.cx - ah * Math.cos(ang + 0.4), q.cy - ah * Math.sin(ang + 0.4));
    ctx.closePath();
    ctx.fillStyle = st.stroke;
    ctx.fill();
  }
  if (label) {
    ctx.fillStyle = st.text;
    ctx.font = '10px system-ui';
    ctx.textAlign = 'center';
    ctx.fillText(label, (p.cx + q.cx) / 2, (p.cy + q.cy) / 2 - 4);
  }
}

// Aresta derivada entre irmãos (mesma pasta): pontilhada curta, bem sutil.
function drawSiblingEdge(p, q) {
  ctx.save();
  ctx.strokeStyle = '#e9c34955';
  ctx.lineWidth = 1;
  ctx.setLineDash([2, 5]);
  ctx.beginPath();
  ctx.moveTo(p.cx, p.cy);
  ctx.lineTo(q.cx, q.cy);
  ctx.stroke();
  ctx.restore();
}

// Aresta de wikilink: tracejada, cor distinta das conexões manuais.
function drawWikiEdge(p, q) {
  ctx.save();
  ctx.strokeStyle = '#b48eadaa';
  ctx.lineWidth = 1.5;
  ctx.setLineDash([4, 4]);
  ctx.beginPath();
  ctx.moveTo(p.cx, p.cy);
  ctx.lineTo(q.cx, q.cy);
  ctx.stroke();
  ctx.restore();
}

// ─── Visão grafo (notas-como-nós + arestas de wikilink) ──────────────────────

// Layout circular determinístico: cada nota vira um nó disposto num círculo.
// Reusa `state.graph` (wikilinks resolvidos) para as arestas. Sem física — é
// estável e suficiente para um jardim de notas pequeno/médio.
function graphNodes() {
  const notes = state.notes;
  const n = notes.length;
  if (!n) return [];
  const cx = canvas.width / 2;
  const cy = canvas.height / 2;
  const radius = Math.max(60, Math.min(canvas.width, canvas.height) / 2 - 60);
  return notes.map((note, i) => {
    if (n === 1) return { note, cx, cy };
    const ang = (i / n) * Math.PI * 2 - Math.PI / 2;
    return { note, cx: cx + radius * Math.cos(ang), cy: cy + radius * Math.sin(ang) };
  });
}

function renderGraph() {
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  const nodes = graphNodes();
  if (!nodes.length) {
    ctx.fillStyle = '#8a7a8a';
    ctx.font = '14px system-ui';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText('Sem notas ainda — crie uma para vê-la no grafo.', canvas.width / 2, canvas.height / 2);
    return;
  }
  const pos = {};
  for (const nd of nodes) pos[nd.note.slug] = nd;

  // arestas de wikilink (reusa o grafo resolvido pelo servidor)
  for (const [from, targets] of Object.entries(state.graph || {})) {
    const a = pos[from];
    if (!a) continue;
    for (const to of targets) {
      const b = pos[to];
      if (!b || b === a) continue;
      drawWikiEdge({ cx: a.cx, cy: a.cy }, { cx: b.cx, cy: b.cy });
    }
  }

  // nós (notas)
  const r = 18;
  for (const nd of nodes) {
    const sel = noteBlock(nd.note.slug)?.id === state.selectedBlock
      && state.selectedBlock != null;
    ctx.beginPath();
    ctx.arc(nd.cx, nd.cy, r, 0, Math.PI * 2);
    ctx.fillStyle = '#b48ead33';
    ctx.fill();
    ctx.lineWidth = sel ? 3 : 1.5;
    ctx.strokeStyle = sel ? '#fff' : '#b48ead';
    ctx.stroke();
    ctx.fillStyle = '#fff';
    ctx.font = '14px system-ui';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText('📝', nd.cx, nd.cy);
    ctx.font = '11px system-ui';
    ctx.fillStyle = '#e8e3d3';
    ctx.fillText(nd.note.title || nd.note.slug, nd.cx, nd.cy + r + 10);
  }
}

// Nó do grafo sob (mx,my), se houver.
function graphNodeAt(mx, my) {
  for (const nd of graphNodes()) {
    if (Math.hypot(mx - nd.cx, my - nd.cy) <= 20) return nd.note;
  }
  return null;
}

// Liga o toggle da visão grafo.
function wireGraphToggle() {
  const btn = document.getElementById('graphBtn');
  if (!btn) return;
  btn.addEventListener('click', () => {
    state.graphView = !state.graphView;
    btn.classList.toggle('active', state.graphView);
    btn.textContent = state.graphView ? '🗺️ Mapa' : '🕸️ Grafo';
    render();
  });
}

// ─── Sidebar ─────────────────────────────────────────────────────────────────

function renderLayers() {
  const el = document.getElementById('layers');
  el.innerHTML = '';
  // mostra topo→base na UI
  [...state.inst.layers].reverse().forEach((l) => {
    const row = document.createElement('div');
    row.className = 'layer-row';
    const vis = document.createElement('input');
    vis.type = 'checkbox'; vis.checked = l.visible;
    vis.onchange = () => editLayer(l.id, { visible: vis.checked });
    const name = document.createElement('label');
    name.textContent = l.name;
    const op = document.createElement('input');
    op.type = 'range'; op.min = 0; op.max = 1; op.step = 0.05; op.value = l.opacity;
    op.title = 'Transparência';
    op.oninput = () => { l.opacity = parseFloat(op.value); render(); };
    op.onchange = () => editLayer(l.id, { opacity: parseFloat(op.value) });
    row.append(vis, name, op);
    el.append(row);
  });
}

function renderPalette() {
  const el = document.getElementById('palette');
  el.innerHTML = '';
  const items = state.template?.palette || [{ block_type: 'note', label: 'Nota', default_props: { icon: '📝' } }];
  items.forEach((it) => {
    const d = document.createElement('div');
    d.className = 'palette-item' + (state.selectedType === it.block_type ? ' sel' : '');
    d.innerHTML = `<span class="ico">${it.default_props?.icon || '■'}</span> ${it.label}`;
    d.onclick = () => {
      state.selectedType = state.selectedType === it.block_type ? null : it.block_type;
      state.deleteMode = false;
      renderPalette();
    };
    el.append(d);
  });
}

function showInspector(b) {
  const el = document.getElementById('inspector');
  if (!b) { el.innerHTML = '<p class="hint">Clique num bloco para ver seu conteúdo.</p>'; return; }
  if (b.props?.note_slug) { showNoteInspector(el, b, b.props.note_slug); return; }
  // Pasta: lista o "conteúdo do diretório" (blocos ligados por `parent`).
  if (b.block_type === 'pasta') {
    const filhos = childrenByParent()[b.id] || [];
    el.innerHTML = `<strong>📁 ${escapeHtml(b.label || 'Pasta')}</strong>`;
    if (!filhos.length) {
      el.innerHTML += '<p class="hint">Pasta vazia — arraste uma nota para cima dela.</p>';
      return;
    }
    filhos.forEach((f) => {
      const d = document.createElement('div');
      d.className = 'palette-item';
      d.innerHTML = `<span class="ico">${f.props?.icon || '📝'}</span> ${escapeHtml(f.label || f.id)}`;
      d.onclick = () => {
        state.selectedBlock = f.id;
        if (f.props?.note_slug) openNote(f.props.note_slug); else showInspector(f);
        render();
      };
      el.append(d);
    });
    return;
  }
  el.innerHTML = `<strong>${escapeHtml(b.label || b.block_type)}</strong>`;
  // Bloco-evento (timeline): instante + kind canônicos
  if (b.props?.at_iso) {
    const t = Date.parse(b.props.at_iso);
    const quando = Number.isFinite(t)
      ? new Intl.DateTimeFormat('pt-BR', { dateStyle: 'long', timeStyle: 'short', timeZone: 'UTC' }).format(new Date(t)) + ' UTC'
      : b.props.at_iso;
    const linha = document.createElement('div');
    linha.className = 'att';
    linha.innerHTML = `🕐 ${escapeHtml(quando)}${b.props.kind ? ` · <code>${escapeHtml(b.props.kind)}</code>` : ''}`;
    el.append(linha);
  }
  for (const a of (b.attachments || [])) {
    const div = document.createElement('div');
    div.className = 'att';
    const url = `${API}/instances/${state.id}/attachments/${a.hash}`;
    if (a.kind === 'image') {
      div.innerHTML = `<img alt="${a.filename}" src="${url}">`;
    } else if (a.kind === 'pdf') {
      div.innerHTML = `<a href="${url}" target="_blank">📄 ${a.filename}</a>`;
    } else if (a.kind === 'sound') {
      div.innerHTML = `<audio controls src="${url}"></audio>`;
    } else {
      div.innerHTML = `<a href="${url}" target="_blank">${a.filename}</a>`;
    }
    if (a.attribution || b.props?.attribution) {
      const credit = document.createElement('div');
      credit.className = 'credit';
      credit.textContent = 'Fonte: ' + (a.attribution || b.props.attribution);
      div.append(credit);
    }
    el.append(div);
  }
  if (state.edit) {
    const up = document.createElement('button');
    up.textContent = '📎 Anexar arquivo';
    up.style.marginTop = '0.6rem';
    up.onclick = () => triggerUpload(b.id);
    el.append(up);
  }
}

// Inspetor de uma nota: Markdown renderizado, wikilinks clicáveis, backlinks e
// (em edição) editor de corpo.
function showNoteInspector(el, b, slug) {
  const note = noteBySlug(slug);
  el.innerHTML = `<strong>${escapeHtml(note?.title || b.label || slug)}</strong>`;

  const view = document.createElement('div');
  view.className = 'att note-view';
  view.innerHTML = renderMarkdown(note?.body || '_(nota vazia)_');
  view.querySelectorAll('a.wikilink').forEach((a) => {
    a.addEventListener('click', (e) => { e.preventDefault(); openNote(a.dataset.slug); });
  });
  el.append(view);

  const back = state.backlinks?.[slug] || [];
  if (back.length) {
    const bl = document.createElement('div');
    bl.className = 'backlinks';
    bl.innerHTML = 'Mencionado em: ' + back.map((x) =>
      `<a class="wikilink" data-slug="${x.source}">${escapeHtml(noteTitle(x.source))}</a>`).join(', ');
    bl.querySelectorAll('a').forEach((a) =>
      a.addEventListener('click', (e) => { e.preventDefault(); openNote(a.dataset.slug); }));
    el.append(bl);
  }

  // Editar abre o popup (independe do modo do canvas) — só para o dono.
  if (state.me && state.inst && state.me === state.inst.owner) {
    const edit = document.createElement('button');
    edit.textContent = '✎ Editar nota';
    edit.style.marginTop = '0.4rem';
    edit.onclick = () => openNoteEditor(slug, note?.title || b.label || slug);
    el.append(edit);
  }
}

// ─── Editor de nota em popup (rascunho-como-branch; salvar = commit) ─────────
//
// O rascunho vive em localStorage por instância+nota+usuário — nunca toca a
// nota canônica até 💾 Salvar (PUT → persiste no servidor e federa ao CO).
// Fechar/Esc preserva o rascunho; reabrir oferece continuar ou descartar.

const editor = { slug: null, title: null, baseline: '', timer: null };

function draftKey(slug) {
  return `ygg-draft:${state.id}:${slug}:${state.me || 'anon'}`;
}

function noteHashLink(slug) {
  // Hash fragment: não vai a logs de servidor nem Referer; sem credencial —
  // editar continua exigindo o JWT do dono (PUT owner-only no servidor).
  return `${location.origin}/universos/instance/${encodeURIComponent(state.id)}#nota=${encodeURIComponent(slug)}&editar=1`;
}

// Rascunho server-side (YG-125): a mesma branch, cross-device. Endpoints
// owner-only; falha de rede degrada para o localStorage em silêncio.
function draftUrl(slug) {
  return `${API}/instances/${state.id}/notes/${encodeURIComponent(slug)}/draft`;
}
async function fetchServerDraft(slug) {
  if (!state.token) return null;
  try {
    const r = await fetch(draftUrl(slug), { headers: authHeaders() });
    if (!r.ok) return null;
    const d = await r.json();
    return { text: d.markdown, ts: Date.parse(d.updated) || 0 };
  } catch { return null; }
}
function pushServerDraft(slug, text) {
  if (!state.token) return;
  fetch(draftUrl(slug), {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify({ markdown: text }),
  }).catch(() => { /* offline → LS cobre */ });
}
function discardDraft(slug) {
  localStorage.removeItem(draftKey(slug));
  if (state.token) fetch(draftUrl(slug), { method: 'DELETE', headers: authHeaders() }).catch(() => {});
}

function offerDraft(ta, banner, texto, origem, quando, slug) {
  banner.hidden = false;
  $id('ed-draft-txt').textContent = `Há um rascunho não salvo${origem}${quando ? ` (${quando})` : ''}.`;
  $id('ed-draft-keep').onclick = () => { ta.value = texto; banner.hidden = true; edPreview(); };
  $id('ed-draft-drop').onclick = () => { discardDraft(slug); banner.hidden = true; };
}

function openNoteEditor(slug, title) {
  const note = noteBySlug(slug);
  editor.slug = slug;
  editor.title = title || note?.title || slug;
  editor.baseline = note?.body || '';
  $id('ed-title').textContent = `✎ ${editor.title}`;
  const ta = $id('ed-text');
  ta.value = editor.baseline;

  // rascunho pendente (a "branch"): local primeiro, depois o do servidor se
  // for mais novo (feito noutro dispositivo)
  let draft = null;
  try { draft = JSON.parse(localStorage.getItem(draftKey(slug))); } catch { /* corrompido → ignora */ }
  const banner = $id('ed-draft-banner');
  banner.hidden = true;
  if (draft && typeof draft.text === 'string' && draft.text !== editor.baseline) {
    offerDraft(ta, banner, draft.text, '',
      draft.ts ? new Date(draft.ts).toLocaleTimeString('pt-BR') : '', slug);
  }
  fetchServerDraft(slug).then((sd) => {
    if (!sd || editor.slug !== slug) return;          // editor já fechou/trocou
    if (sd.text === editor.baseline) return;          // igual à nota → nada a oferecer
    if (draft && (draft.ts || 0) >= sd.ts) return;    // a branch local é mais nova
    if (ta.value !== editor.baseline) return;         // usuário já está digitando
    offerDraft(ta, banner, sd.text, ' de outro dispositivo',
      sd.ts ? new Date(sd.ts).toLocaleTimeString('pt-BR') : '', slug);
  });

  $id('note-editor').classList.add('open');
  history.replaceState(null, '', `#nota=${encodeURIComponent(slug)}&editar=1`);
  edPreview();
  ta.focus();
}

function closeNoteEditor() {
  edSaveDraft(); // fechar nunca perde nada — o rascunho fica
  $id('note-editor').classList.remove('open');
  history.replaceState(null, '', location.pathname);
  editor.slug = null;
}

function edPreview() {
  const view = $id('ed-preview');
  view.innerHTML = renderMarkdown($id('ed-text').value || '_(nota vazia)_');
  view.querySelectorAll('a.wikilink').forEach((a) =>
    a.addEventListener('click', (e) => e.preventDefault()));
}

function edSaveDraft() {
  if (!editor.slug) return;
  const text = $id('ed-text').value;
  if (text === editor.baseline) { discardDraft(editor.slug); return; }
  try {
    localStorage.setItem(draftKey(editor.slug), JSON.stringify({ text, ts: Date.now() }));
  } catch { /* LS cheio — o servidor cobre */ }
  pushServerDraft(editor.slug, text); // branch cross-device (YG-125)
  $id('ed-status').textContent = `rascunho guardado ${new Date().toLocaleTimeString('pt-BR')} · nada publica até salvar`;
}

async function edCommit() {
  if (!editor.slug) return;
  const slug = editor.slug;
  const ok = await saveNote(slug, editor.title, $id('ed-text').value);
  if (ok === false) return; // toast de erro já apareceu; rascunho intacto
  // baseline = texto commitado, senão o edSaveDraft do close recriaria a branch
  editor.baseline = $id('ed-text').value;
  discardDraft(slug); // commit feito → a branch (local + servidor) se dissolve
  closeNoteEditor();
  toast('💾 Nota publicada');
}

function $id(x) { return document.getElementById(x); }

function wireNoteEditor() {
  $id('ed-close').onclick = closeNoteEditor;
  $id('ed-save').onclick = edCommit;
  $id('ed-link').onclick = () => {
    const url = noteHashLink(editor.slug);
    (navigator.clipboard?.writeText(url) || Promise.reject())
      .then(() => toast('🔗 Link copiado'))
      .catch(() => prompt('Copie o link:', url));
  };
  const ta = $id('ed-text');
  ta.addEventListener('input', () => {
    edPreview();
    clearTimeout(editor.timer);
    editor.timer = setTimeout(edSaveDraft, 800);
  });
  document.addEventListener('keydown', (e) => {
    if (!$id('note-editor').classList.contains('open')) return;
    if (e.key === 'Escape') { e.preventDefault(); closeNoteEditor(); }
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') { e.preventDefault(); edCommit(); }
  });
  $id('note-editor').addEventListener('mousedown', (e) => {
    if (e.target === $id('note-editor')) closeNoteEditor();
  });
}

// Deep-link: #nota=<slug>[&editar=1] abre a nota (e o editor, se dono).
function openFromHash() {
  const m = location.hash.match(/#nota=([^&]+)(?:&editar=(1))?/);
  if (!m) return;
  const slug = decodeURIComponent(m[1]);
  const editar = m[2] === '1';
  const dono = state.me && state.inst && state.me === state.inst.owner;
  if (editar && dono) {
    const note = noteBySlug(slug);
    openNoteEditor(slug, note?.title || slug);
  } else {
    openNote(slug);
  }
}

// Abre a nota: seleciona o bloco que a referencia (se houver) e mostra no inspetor.
function openNote(slug) {
  const blk = noteBlock(slug);
  if (blk) {
    state.selectedBlock = blk.id;
    showInspector(blk);
    render();
  } else {
    toast(`Nota "${slug}" ainda não tem bloco no mapa`);
  }
}

async function saveNote(slug, title, markdown) {
  const res = await fetch(`${API}/instances/${state.id}/notes/${encodeURIComponent(slug)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify({ title, markdown }),
  });
  if (!res.ok) {
    let msg = 'Falha ao salvar nota';
    try { msg = (await res.json()).erro || msg; } catch {}
    toast('⚠ ' + msg);
    return false;
  }
  await loadNotes();
  renderNotesList();
  const blk = noteBlock(slug);
  if (blk) showInspector(blk);
  render();
  return true;
}

// Notas que casam com a busca (título OU corpo), client-side, sem API.
function filteredNotes() {
  const q = state.noteQuery.trim().toLowerCase();
  if (!q) return state.notes;
  return state.notes.filter((n) =>
    (n.title || '').toLowerCase().includes(q) ||
    (n.body || '').toLowerCase().includes(q));
}

// Lista de notas na sidebar; clique abre a nota. Respeita a busca.
function renderNotesList() {
  const el = document.getElementById('notes-list');
  if (!el) return;
  el.innerHTML = '';
  if (!state.notes.length) { el.innerHTML = '<p class="hint">Sem notas ainda.</p>'; return; }
  const matches = filteredNotes();
  if (!matches.length) { el.innerHTML = '<p class="hint">Nenhuma nota encontrada.</p>'; return; }
  matches.forEach((n) => {
    const d = document.createElement('div');
    d.className = 'palette-item';
    d.innerHTML = `<span class="ico">📝</span> ${escapeHtml(n.title)}`;
    d.onclick = () => openNote(n.slug);
    el.append(d);
  });
}

// ─── Legenda de ligações (separa por tipo; clique alterna visibilidade) ──────

const LINK_KINDS = [
  { key: 'parent',   label: 'Pasta (pai/filho)', swatch: '#e9c349' },
  { key: 'ref',      label: 'Referência',        swatch: '#7ec8e3' },
  { key: 'wikilink', label: 'Wikilink',          swatch: '#b48ead' },
  { key: 'sibling',  label: 'Irmãos (mesma pasta)', swatch: '#e9c34955' },
];

function renderLinkLegend() {
  const el = document.getElementById('link-legend');
  if (!el) return;
  el.innerHTML = '';
  LINK_KINDS.forEach((k) => {
    const d = document.createElement('div');
    d.className = 'palette-item' + (state.linkFilter[k.key] ? ' sel' : '');
    d.setAttribute('role', 'checkbox');
    d.setAttribute('aria-checked', String(!!state.linkFilter[k.key]));
    d.innerHTML = `<span class="ico" style="color:${k.swatch}">${state.linkFilter[k.key] ? '◉' : '◯'}</span> ${k.label}`;
    d.onclick = () => {
      state.linkFilter[k.key] = !state.linkFilter[k.key];
      renderLinkLegend();
      render();
    };
    el.append(d);
  });
}

// Liga o campo de busca (filtra a sidebar Notas sem chamar a API).
function wireNoteSearch() {
  const input = document.getElementById('note-search');
  if (!input) return;
  input.addEventListener('input', () => {
    state.noteQuery = input.value;
    renderNotesList();
  });
}

// ─── Edição (EditOps via PATCH) ──────────────────────────────────────────────

async function patch(op) {
  const res = await fetch(`${API}/instances/${state.id}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify(op),
  });
  if (!res.ok) {
    let msg = 'Operação inválida';
    try { msg = (await res.json()).erro || msg; } catch {}
    toast('⚠ ' + msg);
    await load(); // reconcilia estado em erro
    return null;
  }
  state.inst = await res.json();
  renderLayers();
  render();
  return state.inst;
}

function editLayer(id, fields) {
  return patch({ op: 'edit_layer', layer: id, ...fields });
}

function targetBlocksLayer() {
  const l = state.inst.layers.find((l) => l.kind === 'blocks');
  return l ? l.id : (state.inst.layers[0] && state.inst.layers[0].id);
}

async function placeBlock(cell) {
  const layer = targetBlocksLayer();
  const item = (state.template?.palette || []).find((p) => p.block_type === state.selectedType);
  const id = `${state.selectedType}-${Date.now().toString(36)}`;
  const props = { ...(item?.default_props || {}) };
  let label;

  // Bloco-nota: cria o arquivo Markdown canônico e amarra via props.note_slug.
  if (state.selectedType === 'note') {
    const title = prompt('Título da nota:');
    if (title === null || title.trim() === '') return; // cancelou
    label = title.trim();
    let slug = slugifyJs(label) || id;
    const res = await fetch(`${API}/instances/${state.id}/notes/${encodeURIComponent(slug)}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
      body: JSON.stringify({ title: label, markdown: '' }),
    });
    if (!res.ok) { toast('⚠ Falha ao criar nota'); return; }
    slug = (await res.json()).slug; // slug canônico do servidor
    props.note_slug = slug;
    if (!props.icon) props.icon = '📝';
  }

  // Pasta: só um rótulo — simula diretório; notas viram filhas por drag-and-drop.
  if (state.selectedType === 'pasta') {
    const nome = prompt('Nome da pasta:');
    if (nome === null || nome.trim() === '') return; // cancelou
    label = nome.trim();
    if (!props.icon) props.icon = '📁';
  }

  const block = { id, block_type: state.selectedType, pos: { x: cell.x, y: cell.y }, props };
  if (label) block.label = label;
  await patch({ op: 'place_block', layer, block });
  await loadNotes();
  renderNotesList();
  render();
}

async function moveBlock(blockId, layerId, cell) {
  await patch({ op: 'move_block', layer: layerId, block_id: blockId, to: { x: cell.x, y: cell.y } });
}

async function deleteBlock(blockId, layerId) {
  await patch({ op: 'delete_block', layer: layerId, block_id: blockId });
  state.selectedBlock = null;
  showInspector(null);
}

// `kind` viaja em `props.kind` (o schema já tem `props` livre — sem migração):
// 'parent' = filho→pasta (hierarquia), 'ref' = referência manual (default).
async function addConnection(from, to, kind) {
  await patch({
    op: 'add_connection',
    connection: {
      id: `c-${Date.now().toString(36)}`,
      from,
      to,
      directed: true,
      props: { kind: kind || 'ref' },
    },
  });
}

// ─── Upload de anexos ────────────────────────────────────────────────────────

let uploadTargetBlock = null;
function triggerUpload(blockId) {
  uploadTargetBlock = blockId;
  document.getElementById('upload').click();
}

document.getElementById('upload').addEventListener('change', async (e) => {
  const file = e.target.files[0];
  e.target.value = '';
  if (!file || !uploadTargetBlock) return;
  const fd = new FormData();
  fd.append('file', file);
  const res = await fetch(`${API}/instances/${state.id}/attachments`, {
    method: 'POST', headers: authHeaders(), body: fd,
  });
  if (!res.ok) {
    let msg = 'Upload falhou';
    try { msg = (await res.json()).erro || msg; } catch {}
    toast('⚠ ' + msg); return;
  }
  const cref = await res.json();
  const hit = findBlock(uploadTargetBlock);
  await patch({ op: 'attach_content', layer: hit.layer.id, block_id: uploadTargetBlock, content: cref });
  await loadImages();
  const again = findBlock(uploadTargetBlock);
  showInspector(again.block);
  render();
  toast('Anexo adicionado');
});

// ─── Interação no canvas ─────────────────────────────────────────────────────

function blockAt(mx, my) {
  const c = gridSpec().cell_size;
  for (const { block } of allBlocks()) {
    const { cx, cy } = cellCenter(block.pos.x, block.pos.y);
    if (Math.hypot(mx - cx, my - cy) <= c * 0.5) return block;
  }
  return null;
}

function mouseXY(e) {
  const r = canvas.getBoundingClientRect();
  return { mx: (e.clientX - r.left) * (canvas.width / r.width), my: (e.clientY - r.top) * (canvas.height / r.height) };
}

canvas.addEventListener('mousedown', (e) => {
  const { mx, my } = mouseXY(e);

  // Visão grafo: clicar num nó seleciona/abre a nota (sem edição no grafo).
  if (state.graphView) {
    const note = graphNodeAt(mx, my);
    if (note) openNote(note.slug);
    return;
  }

  const hit = blockAt(mx, my);

  // Timeline: arrastar o vazio = pan no eixo do tempo (qualquer modo).
  if (isTimeline() && !hit) {
    state.tlPan = { startMx: mx, startOff: state.tl.off, moved: false };
    return;
  }

  if (!state.edit) {
    state.selectedBlock = hit ? hit.id : null;
    showInspector(hit);
    render();
    // Dono clicou em célula vazia no modo visualizar: dica em vez de silêncio.
    if (!hit && state.me && state.inst && state.me === state.inst.owner) {
      toast('Ative ✏️ Editar para colocar notas e pastas');
    }
    return;
  }

  // modo edição
  if (state.deleteMode && hit) {
    const layer = findBlock(hit.id).layer.id;
    deleteBlock(hit.id, layer);
    return;
  }
  if (state.connectFrom) {
    if (hit && hit.id !== state.connectFrom) addConnection(state.connectFrom, hit.id);
    state.connectFrom = null;
    document.getElementById('connBtn').classList.remove('active');
    return;
  }
  if (hit) {
    state.selectedBlock = hit.id;
    state.drag = { blockId: hit.id, layerId: findBlock(hit.id).layer.id, moved: false };
    showInspector(hit);
    render();
    return;
  }
  // célula vazia + tipo selecionado → coloca
  if (state.selectedType) {
    const cell = screenToCell(mx, my);
    const g = gridSpec();
    if (cell.x >= 0 && cell.y >= 0 && cell.x < g.width && cell.y < g.height) placeBlock(cell);
  }
});

canvas.addEventListener('mousemove', (e) => {
  if (state.tlPan) {
    const { mx } = mouseXY(e);
    state.tl.off = state.tlPan.startOff + (mx - state.tlPan.startMx);
    if (Math.abs(mx - state.tlPan.startMx) > 3) state.tlPan.moved = true;
    render();
    return;
  }
  if (!state.drag) return;
  state.drag.moved = true;
});

canvas.addEventListener('mouseup', (e) => {
  if (state.tlPan) {
    if (!state.tlPan.moved) { state.selectedBlock = null; showInspector(null); render(); }
    state.tlPan = null;
    return;
  }
  if (state.drag && state.drag.moved) {
    const { mx, my } = mouseXY(e);
    const cell = screenToCell(mx, my);
    const g = gridSpec();
    if (cell.x >= 0 && cell.y >= 0 && cell.x < g.width && cell.y < g.height) {
      // Soltar sobre OUTRO bloco = ligar (não mover): em cima de uma pasta o
      // bloco vira filho (`parent`); em cima de nota/landmark vira `ref`.
      const alvo = blockAt(mx, my);
      if (alvo && alvo.id !== state.drag.blockId) {
        const kind = alvo.block_type === 'pasta' ? 'parent' : 'ref';
        addConnection(state.drag.blockId, alvo.id, kind).then(() => {
          toast(kind === 'parent'
            ? `📁 Movido para dentro de "${alvo.label || 'pasta'}"`
            : '🔗 Ligação criada');
        });
      } else {
        moveBlock(state.drag.blockId, state.drag.layerId, cell);
      }
    }
  }
  state.drag = null;
});

// ─── Controles de modo ───────────────────────────────────────────────────────

function toggleEditMode(force) {
  state.edit = force === undefined ? !state.edit : !!force;
  document.body.classList.toggle('edit', state.edit);
  document.getElementById('modeBtn').classList.toggle('active', state.edit);
  document.getElementById('modeBtn').textContent = state.edit ? '👁️ Visualizar' : '✏️ Editar';
  // O inspetor muda com o modo (editor de nota aparece/some) — re-renderiza.
  const sel = state.selectedBlock && findBlock(state.selectedBlock);
  if (sel) showInspector(sel.block);
}
document.getElementById('modeBtn').addEventListener('click', () => toggleEditMode());

document.getElementById('connBtn').addEventListener('click', () => {
  if (!state.selectedBlock) { toast('Selecione um bloco de origem'); return; }
  state.connectFrom = state.selectedBlock;
  state.deleteMode = false;
  document.getElementById('connBtn').classList.add('active');
  document.getElementById('delBtn').classList.remove('active');
  toast('Clique no bloco de destino');
});

document.getElementById('delBtn').addEventListener('click', () => {
  state.deleteMode = !state.deleteMode;
  state.connectFrom = null;
  state.selectedType = null;
  document.getElementById('delBtn').classList.toggle('active', state.deleteMode);
  document.getElementById('connBtn').classList.remove('active');
  renderPalette();
});

load();
