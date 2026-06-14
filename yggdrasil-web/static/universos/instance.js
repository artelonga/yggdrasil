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
  // YG-126: view = lente de runtime sobre o MESMO universo (padrão co):
  // 'mapa' (grade), 'timeline' (eixo do tempo, read-only), 'grafo' (wikilinks)
  view: 'mapa',
  tl: { off: 0, scale: 1 }, // pan/zoom do eixo X (view timeline, YG-123/126)
  tlPan: null,          // drag-pan em curso na timeline
  tlCache: null,        // cena derivada da timeline (invalidada em writes)
  // visibilidade por tipo de ligação (legenda da sidebar). `sibling` é
  // derivado: filhos da mesma pasta — desligado por padrão para não poluir.
  linkFilter: { parent: true, ref: true, wikilink: true, sibling: false },
  // YG-129: escopo das ligações — 'irmas' (mesmo pai, ou do nó selecionado)
  // por padrão; 'todas' expande. Pais colapsados escondem a subárvore.
  linkScope: 'irmas',
  collapsed: {},        // id do pai → true (subárvore escondida)
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

// YG-138: promover este universo autorado a um universo no CO, com convites e
// modo público/subscribe. O CO já expõe a API user-facing (POST /api/v1/universes
// com parent_key/visibility, GET /api/v1/me/universes, members, /subscribe —
// CO-444). Dois lados:
//
//  Passo 1 (criar). A criação no CO é um MODAL no SPA, então fazemos redirect ao
//    CO autenticado (cookie compartilhado .artelonga.com.br / SSO) com prefill via
//    query. Contrato (a alinhar com o handler do CO, em curso):
//      /?criar=1&name=<título>&key=<slug sugerido>&source=yggdrasil&instance=<id>
//    Degrada gracioso: sem o handler, o usuário cai no CO logado e clica "+ Novo".
//
//  Passo 2 (pós-criação: convites + visibilidade). Lemos "meus universos" do CO
//    direto do navegador — `GET {CO}/api/v1/me/universes` com `credentials:
//    'include'` usa o cookie do apex compartilhado, sem token handover server-side
//    (o JWT local do Yggdrasil não é aceito pelo CO). Listamos owned/invited/
//    subscribed e ligamos cada universo à sua página no CO, onde convidar e
//    alternar visibilidade pública são geridos. Degrada gracioso: sem login no CO
//    / CORS bloqueado / offline → cai num único link "abrir meus universos no CO".
const CO_BASE = 'https://co.artelonga.com.br';

function coSuggestedName() {
  return (state.inst && state.inst.title) || 'Universo';
}

// Passo 1 — redirect ao CO com o universo pré-preenchido.
function criarNoCO() {
  const name = coSuggestedName();
  const qs = new URLSearchParams({
    criar: '1',
    name,
    key: (slugifyJs(name) || 'universo').slice(0, 40),
    source: 'yggdrasil',
    instance: state.id,
  });
  window.location.assign(`${CO_BASE}/?${qs.toString()}`);
}

// Abre o painel "Universo no CO" e dispara a carga do passo 2.
function openCoPanel() {
  const panel = document.getElementById('co-panel');
  if (!panel) { criarNoCO(); return; }   // fallback: sem painel, redireciona direto
  const nm = document.getElementById('co-name');
  if (nm) nm.textContent = coSuggestedName();
  panel.classList.add('open');
  loadMeusUniversosCO();
}

function closeCoPanel() {
  const panel = document.getElementById('co-panel');
  if (panel) panel.classList.remove('open');
}

function coFallbackLink(msg) {
  return '<p class="muted">' + escapeHtml(msg) + '</p>' +
    '<p style="margin-top:.4rem"><a href="' + CO_BASE +
    '/" target="_blank" rel="noopener">Abrir meus universos no CO ↗</a></p>';
}

// Passo 2 — lê /api/v1/me/universes do CO no navegador e lista buckets com
// links para convidar / alternar visibilidade (geridos no CO).
async function loadMeusUniversosCO() {
  const host = document.getElementById('co-mine');
  if (!host) return;
  host.innerHTML = '<p class="muted">Carregando…</p>';
  let data;
  try {
    const r = await fetch(`${CO_BASE}/api/v1/me/universes`, {
      credentials: 'include',
      headers: { Accept: 'application/json' },
    });
    if (r.status === 401 || r.status === 403) {
      host.innerHTML = coFallbackLink('Entre no CO para ver e gerir seus universos.');
      return;
    }
    if (!r.ok) throw new Error('status ' + r.status);
    data = await r.json();
  } catch {
    host.innerHTML = coFallbackLink('Não deu pra falar com o CO agora (login/rede). Você pode abrir direto:');
    return;
  }
  host.innerHTML = renderMeusUniversosCO(data);
}

// Renderiza os buckets do /me/universes. Tolerante ao formato exato do CO:
// aceita { owned, subscribed, invited } como arrays de universos.
function renderMeusUniversosCO(data) {
  data = data || {};
  const buckets = [
    { key: 'owned', label: 'Sou dono', items: data.owned },
    { key: 'invited', label: 'Convidado', items: data.invited },
    { key: 'subscribed', label: 'Subscrito', items: data.subscribed },
  ];
  let any = false;
  let html = '';
  buckets.forEach(function (b) {
    const items = Array.isArray(b.items) ? b.items : [];
    if (!items.length) return;
    any = true;
    html += '<h3 style="margin-top:.6rem">' + escapeHtml(b.label) + '</h3>';
    items.forEach(function (u) {
      const key = u.key || u.slug || u.id || '';
      const name = u.title || u.name || key || 'universo';
      const vis = u.visibility || (u.public ? 'public' : null);
      const visTxt = vis === 'public' ? '🌐 público' : (vis ? '🔒 ' + escapeHtml(vis) : '');
      const manage = b.key === 'owned'
        ? '<a href="' + CO_BASE + '/' + encodeURIComponent(key) +
          '" target="_blank" rel="noopener" title="Convidar pessoas ou alternar visibilidade no CO">✉️ convites · visibilidade ↗</a>'
        : '<a href="' + CO_BASE + '/' + encodeURIComponent(key) +
          '" target="_blank" rel="noopener">abrir ↗</a>';
      html += '<div class="uni"><span class="nm">' + escapeHtml(name) +
        '</span><span class="vis">' + visTxt + '</span>' + manage + '</div>';
    });
  });
  if (!any) {
    return coFallbackLink('Você ainda não tem universos no CO. Crie um no passo 1 acima.');
  }
  return html +
    '<p class="muted" style="margin-top:.6rem">Convites e o modo público (pra subscribe) ' +
    'são geridos na página do universo no CO.</p>';
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
    // YG-129: manipulação direta — dono edita SEMPRE (sem toggle). O botão
    // ✏️ some; clique cria, arrasto move, arrastar-sobre liga.
    toggleEditMode(true);
    // YG-138: dono pode promover este universo autorado a um universo no CO
    // (vira dele lá; pode convidar ou deixar público pra subscribe). O botão
    // abre o painel (criar + gerir convites/visibilidade pós-criação).
    const coBtn = document.getElementById('co-create');
    if (coBtn) {
      coBtn.hidden = false;
      coBtn.addEventListener('click', openCoPanel);
    }
    const coDo = document.getElementById('co-do-create');
    if (coDo) coDo.addEventListener('click', criarNoCO);
    const coClose = document.getElementById('co-close');
    if (coClose) coClose.addEventListener('click', closeCoPanel);
    const coPanel = document.getElementById('co-panel');
    if (coPanel) {
      coPanel.addEventListener('click', (e) => { if (e.target === coPanel) closeCoPanel(); });
    }
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
  wireInline();
  // instâncias geradas com projection=timeline abrem direto na lente timeline
  if (state.inst.projection === 'timeline') setView('timeline');
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
    state.tlCache = null; // notas mudaram → eventos derivados renascem
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

// ─── Tarefa por composição (YG-130): nota + status ───────────────────────────
const STATUS_GLIFO = { todo: '☐', doing: '◐', done: '☑' };
const STATUS_TUI = { todo: '[ ]', doing: '[~]', done: '[x]' };
const STATUS_NOME = { todo: 'a fazer', doing: 'fazendo', done: 'feita' };

// YG-131: a forma emerge do conteúdo — ícone é RENDER, nunca verdade gravada.
// nota sem corpo = 📁 pasta · corpo + filhos = 🗂 índice (index.html) ·
// corpo sem filhos = 📝 artigo · blocos legados mantêm o próprio ícone.
function iconeDoNo(b) {
  const slug = b?.props?.note_slug;
  if (!slug) return b?.props?.icon || (b?.block_type === 'pasta' ? '📁' : '■');
  const note = noteBySlug(slug);
  const temCorpo = !!(note?.body || '').trim();
  const temFilhos = (childrenByParent()[b.id] || []).length > 0;
  if (!temCorpo) return '📁';
  return temFilhos ? '🗂' : '📝';
}

function statusDoBloco(b) {
  const slug = b?.props?.note_slug;
  return slug ? (noteBySlug(slug)?.status || null) : null;
}

// todo → doing → done → (limpa: volta a nota) → todo …
async function ciclarStatus(slug) {
  const n = noteBySlug(slug);
  if (!n) return;
  const ordem = { todo: 'doing', doing: 'done', done: '' };
  const novo = n.status == null ? 'todo' : (ordem[n.status] ?? 'todo');
  const res = await fetch(`${API}/instances/${state.id}/notes/${encodeURIComponent(slug)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify({ title: n.title, markdown: n.body, status: novo }),
  });
  if (!res.ok) { toast('⚠ Falha ao mudar status'); return; }
  await loadNotes();
  renderNotesList();
  render();
  const blk = noteBlock(slug);
  if (blk && state.selectedBlock === blk.id) showInspector(blk);
}

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

function gridSpec() {
  // na view timeline a grade é a da cena derivada (48 colunas × faixas)
  if (state.inst && isTimeline()) return tlScene().grid;
  return state.inst.grid;
}

// ─── Cena derivada da timeline (YG-126) ──────────────────────────────────────
//
// Espelho JS do contrato puro do gerador (x_for/lane_rows — mesma régua; ver
// docs/architecture/co-387-time-lens-spec-draft.md). Eventos de QUALQUER
// universo: blocos com `props.at_iso` (evento explícito), criação de notas
// (padrão co: created_at como fallback de data) e a criação do universo.
// Futuro: eventos de sistema via bridge (scores etc.) aterrissam aqui.
function tlScene() {
  if (state.tlCache) return state.tlCache;
  const eventos = [];
  for (const l of state.inst.layers) {
    for (const b of l.blocks) {
      const t = Date.parse(b.props?.at_iso || '');
      if (!Number.isFinite(t)) continue;
      eventos.push({ at: t, src: b, kind: b.props?.kind || b.block_type });
    }
  }
  for (const n of state.notes) {
    const t = Date.parse(n.created || '');
    if (!Number.isFinite(t)) continue;
    eventos.push({
      at: t,
      kind: 'nota.criada',
      virt: {
        id: `evt-nota-${n.slug}`,
        block_type: 'evento',
        label: n.title,
        props: { icon: '📝', kind: 'nota.criada', at_iso: n.created, note_slug: n.slug },
      },
    });
  }
  const criado = Date.parse(state.inst.created_at || '');
  if (Number.isFinite(criado)) {
    eventos.push({
      at: criado,
      kind: 'universo.criado',
      virt: {
        id: 'evt-universo-criado',
        block_type: 'evento',
        label: `${state.inst.title || 'Universo'} criado`,
        props: { icon: '✦', kind: 'universo.criado', at_iso: state.inst.created_at },
      },
    });
  }

  eventos.sort((a, b) => a.at - b.at);
  const WIDTH = 48, LANE_H = 3;
  const min = eventos.length ? eventos[0].at : 0;
  const max = eventos.length ? eventos[eventos.length - 1].at : 0;
  // lanes por família (prefixo até o 1º '.'), ordem determinística
  const fams = [...new Set(eventos.map((e) => e.kind.split('.')[0]))].sort();
  const lane = Object.fromEntries(fams.map((f, i) => [f, i]));
  const height = Math.max(fams.length * LANE_H, LANE_H);

  const stack = {};
  const items = eventos.map((e) => {
    const span = max - min;
    const x = span <= 0 ? Math.floor(WIDTH / 2)
      : Math.round(((e.at - min) / span) * (WIDTH - 1));
    const fam = e.kind.split('.')[0];
    const key = `${fam}:${x}`;
    const s = (stack[key] = (stack[key] || 0) + 1) - 1;
    const y = Math.min(lane[fam] * LANE_H + Math.min(s, LANE_H - 1), height - 1);
    const base = e.src || e.virt;
    // clone com posição derivada; mantém id/props (inspetor/nota funcionam)
    return { ...base, pos: { x, y } };
  });

  state.tlCache = {
    grid: { width: WIDTH, height, cell_size: 28 },
    items,
  };
  return state.tlCache;
}

function isIso() { return state.inst.projection === 'isometric'; }
// YG-126: timeline é uma VIEW de runtime (qualquer universo), não projeção fixa.
function isTimeline() { return state.view === 'timeline'; }
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
  if (state.view === 'mundo') return; // MundoView dimensiona o canvas (engine 2D)
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
  // view timeline: cena derivada (eventos virtuais com posição no tempo)
  if (isTimeline()) return tlScene().items.map((b) => ({ block: b, layer: null }));
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
  if (state.view === 'mundo') return; // a engine 2D (MundoView) é dona do canvas
  if (state.view === 'grafo') { renderGraph(); renderTree(); return; }
  const _pais = parentMap();
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
    // YG-129: escopo — irmãs/seleção primeiro; pais (estrutura) sempre passam
    if (kind !== 'parent' && !arestaVisivel(conn.from, conn.to, _pais)) continue;
    if (escondidoPorColapso(conn.from, _pais) || escondidoPorColapso(conn.to, _pais)) continue;
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
          if (escondidoPorColapso(grupo[i].id, _pais)) continue;
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
        if (!arestaVisivel(a.id, b.id, _pais)) continue;
        drawWikiEdge(cellCenter(a.pos.x, a.pos.y), cellCenter(b.pos.x, b.pos.y));
      }
    }
  }

  // blocos (subárvores colapsadas ficam de fora; pastas fechadas mostram ▸)
  for (const { block } of allBlocks()) {
    if (!isTimeline() && escondidoPorColapso(block.id, _pais)) continue;
    drawBlock(block);
  }

  renderTree();
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
  if (!state.inst || !isTimeline()) return;
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
  const icon = iconeDoNo(b);
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
    // pasta fechada mostra quantos filhos guarda (▸N); tarefa mostra ☐/◐/☑
    const kids = state.collapsed[b.id] ? (childrenByParent()[b.id] || []).length : 0;
    const st = statusDoBloco(b);
    const rotulo = (st ? STATUS_GLIFO[st] + ' ' : '') + b.label + (kids ? ` ▸${kids}` : '');
    ctx.fillText(rotulo, cx, cy + r + c * 0.35);
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

// id do filho → id do pai (ligações `parent`). Raízes ficam de fora.
function parentMap() {
  const pais = {};
  for (const conn of state.inst.connections) {
    if (connKind(conn) === 'parent') pais[conn.from] = conn.to;
  }
  return pais;
}

// Irmãs = mesmo pai; duas raízes também são irmãs (o "pai" é a raiz comum).
function saoIrmas(aId, bId, pais) {
  return (pais[aId] || null) === (pais[bId] || null);
}

// Escondido por algum ancestral colapsado (pasta "fechada").
function escondidoPorColapso(blockId, pais) {
  let p = pais[blockId];
  let guard = 0;
  while (p && guard++ < 64) {
    if (state.collapsed[p]) return true;
    p = pais[p];
  }
  return false;
}

// A aresta aparece? Escopo 'irmas': endpoints irmãos OU encostando no nó
// selecionado. 'todas': sempre (respeitando os toggles por tipo da legenda).
function arestaVisivel(fromId, toId, pais) {
  if (escondidoPorColapso(fromId, pais) || escondidoPorColapso(toId, pais)) return false;
  if (state.linkScope === 'todas') return true;
  if (state.selectedBlock && (fromId === state.selectedBlock || toId === state.selectedBlock)) return true;
  return saoIrmas(fromId, toId, pais);
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

// Seletor de views (YG-126): Mapa | Timeline | Grafo | Mundo — lentes do mesmo
// universo. "🌍 Mundo" (YG-148) entrega o #canvas à engine 2D walkable.
function setView(v) {
  state.view = v;
  state.tlCache = null; // cena derivada renasce com os dados atuais
  state.tl = { off: 0, scale: 1 };
  document.querySelectorAll('#views button').forEach((b) =>
    b.classList.toggle('active', b.dataset.view === v));
  const mundo = v === 'mundo';
  document.body.classList.toggle('mundo', mundo);
  const ui = document.getElementById('mundo-ui');
  if (ui) ui.hidden = !mundo;
  if (mundo) {
    if (window.MundoView) {
      window.MundoView.mount(canvas, {
        inst: state.inst,
        notes: state.notes,
        instanceId: state.id,
        api: API,
        token: state.token,
        renderMarkdown,
      });
    }
    return;
  }
  if (window.MundoView) window.MundoView.unmount();
  sizeCanvas();
  render();
}
function wireGraphToggle() {
  document.querySelectorAll('#views button').forEach((b) =>
    b.addEventListener('click', () => setView(b.dataset.view)));
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

  // tarefa por composição: chips de status (dono) — nota · ☐ · ◐ · ☑
  if (state.me && state.inst && state.me === state.inst.owner) {
    const chips = document.createElement('div');
    chips.className = 'layer-row';
    [[null, '· nota'], ['todo', '☐ a fazer'], ['doing', '◐ fazendo'], ['done', '☑ feita']]
      .forEach(([st, rotulo]) => {
        const btn = document.createElement('button');
        btn.textContent = rotulo;
        btn.style.fontSize = '0.7rem';
        if ((note?.status || null) === st) btn.classList.add('active');
        btn.onclick = async () => {
          const res = await fetch(`${API}/instances/${state.id}/notes/${encodeURIComponent(slug)}`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json', ...authHeaders() },
            body: JSON.stringify({ title: note?.title || slug, markdown: note?.body || '', status: st || '' }),
          });
          if (!res.ok) { toast('⚠ Falha ao mudar status'); return; }
          await loadNotes();
          renderNotesList();
          render();
          showInspector(b);
        };
        chips.append(btn);
      });
    el.append(chips);
  }

  // YG-131: índice (index.html) — corpo renderizado (se houver)…
  const temCorpo = !!(note?.body || '').trim();
  if (temCorpo) {
    const view = document.createElement('div');
    view.className = 'att note-view';
    view.innerHTML = renderMarkdown(note.body);
    view.querySelectorAll('a.wikilink').forEach((a) => {
      a.addEventListener('click', (e) => { e.preventDefault(); openNote(a.dataset.slug); });
    });
    el.append(view);
  }
  // …+ conteúdo da pasta (filhos clicáveis, se houver) — o mesmo painel para
  // pasta (sem corpo), artigo (sem filhos) e índice (ambos).
  const filhos = childrenByParent()[b.id] || [];
  if (filhos.length) {
    const cab = document.createElement('p');
    cab.className = 'hint';
    cab.textContent = `📁 ${filhos.length} dentro:`;
    el.append(cab);
    filhos.forEach((f) => {
      const d = document.createElement('div');
      d.className = 'palette-item';
      d.innerHTML = `<span class="ico">${iconeDoNo(f)}</span> ${escapeHtml(f.label || f.id)}`;
      d.onclick = () => {
        state.selectedBlock = f.id;
        if (f.props?.note_slug) openNote(f.props.note_slug); else showInspector(f);
        render();
      };
      el.append(d);
    });
  }
  if (!temCorpo && !filhos.length) {
    const vazio = document.createElement('p');
    vazio.className = 'hint';
    vazio.textContent = 'Vazia — escreva um corpo (vira artigo) ou arraste nós para dentro (vira pasta).';
    el.append(vazio);
  }

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

// ─── Árvore TUI (YG-129): a hierarquia da raiz, estilo terminal ──────────────
//
// ~/titulo
// ├── 📁 pasta            (clique no pai alterna ▸/▾ — esconde no canvas tb.)
// │   └── 📝 nota
// └── solto
function renderTree() {
  const el = document.getElementById('tree');
  if (!el) return;
  const pais = parentMap();
  const filhos = childrenByParent();
  const todos = [];
  for (const l of state.inst.layers) {
    if (l.kind === 'background') continue;
    for (const b of l.blocks) todos.push(b);
  }
  if (!todos.length) {
    el.innerHTML = '<span class="hint">clique numa célula vazia e digite</span>';
    return;
  }
  const raizes = todos.filter((b) => !pais[b.id])
    .sort((a, b) => String(a.label || a.id).localeCompare(String(b.label || b.id), 'pt-BR'));

  const linhas = [`<span style="opacity:.55">~/</span>${escapeHtml(state.inst.title || 'universo')}`];
  function no(b, prefixo, ultimo) {
    const kids = (filhos[b.id] || [])
      .sort((x, y) => String(x.label || x.id).localeCompare(String(y.label || y.id), 'pt-BR'));
    const ramo = ultimo ? '└── ' : '├── ';
    const fechado = !!state.collapsed[b.id];
    const seta = kids.length ? (fechado ? '▸ ' : '▾ ') : '';
    const icone = iconeDoNo(b);
    const sel = b.id === state.selectedBlock ? ';color:#e9c349' : '';
    const st = statusDoBloco(b);
    const chk = st
      ? `<a data-ciclar="${escapeHtml(b.props.note_slug)}" style="cursor:pointer;color:#6dbf8b" title="clique p/ avançar">${STATUS_TUI[st]}</a> `
      : '';
    linhas.push(
      `<span style="opacity:.4">${prefixo}${ramo}</span>` + chk +
      `<a data-tree="${escapeHtml(b.id)}" style="cursor:pointer${sel}">` +
      `${seta}${icone} ${escapeHtml(b.label || b.id)}</a>`);
    if (fechado) return;
    const sub = prefixo + (ultimo ? '    ' : '│   ');
    kids.forEach((k, i) => no(k, sub, i === kids.length - 1));
  }
  raizes.forEach((r, i) => no(r, '', i === raizes.length - 1));
  el.innerHTML = linhas.join('\n');

  el.querySelectorAll('[data-tree]').forEach((a) => {
    a.onclick = () => {
      const b = findBlock(a.dataset.tree)?.block;
      if (!b) return;
      toggleNo(b);
    };
  });
  el.querySelectorAll('[data-ciclar]').forEach((a) => {
    a.onclick = (e) => { e.stopPropagation(); ciclarStatus(a.dataset.ciclar); };
  });
}

// Clique num nó (árvore OU canvas): pai alterna mostrar/esconder filhos;
// qualquer nó seleciona (revela as ligações dele) e abre no inspetor.
function toggleNo(b) {
  const kids = childrenByParent()[b.id] || [];
  if (kids.length) state.collapsed[b.id] = !state.collapsed[b.id];
  state.selectedBlock = b.id;
  if (b.props?.note_slug) openNote(b.props.note_slug); else showInspector(b);
  render();
}

// ─── Composer inline (YG-129): clique numa célula vazia e digite ─────────────
//
// Idioma de terminal, zero popups:
//   Enter cria · Shift+Enter quebra linha · Esc cancela
//   `nome/`  → 📁 pasta (a barra é o gesto universal de diretório)
//   resto    → 📝 nota: 1ª linha = título (renderiza inline na grade),
//              linhas seguintes = corpo. Nota só-título = rótulo inline.
let inlineCell = null;

function abrirInline(cell) {
  const input = document.getElementById('inline-input');
  if (!input) return;
  inlineCell = cell;
  const { cx, cy } = cellCenter(cell.x, cell.y);
  const r = canvas.getBoundingClientRect();
  const stage = canvas.parentElement.getBoundingClientRect();
  const sx = r.left - stage.left + (cx / canvas.width) * r.width;
  const sy = r.top - stage.top + (cy / canvas.height) * r.height;
  input.style.left = Math.max(0, sx - 8) + 'px';
  input.style.top = Math.max(0, sy - 14) + 'px';
  input.value = '';
  input.hidden = false;
  input.focus();
}

function fecharInline() {
  const input = document.getElementById('inline-input');
  if (input) { input.hidden = true; input.value = ''; }
  inlineCell = null;
}

async function confirmarInline() {
  const input = document.getElementById('inline-input');
  const texto = (input?.value || '').replace(/\s+$/, '');
  const cell = inlineCell;
  fecharInline();
  if (!texto.trim() || !cell) return;
  const layer = targetBlocksLayer();
  const id = `n-${Date.now().toString(36)}`;
  const linhas = texto.split('\n');
  const primeira = linhas[0].trim();

  if (linhas.length === 1 && /\/$/.test(primeira)) {
    // `nome/` → NOTA de corpo vazio (YG-131): pasta é render, não tipo.
    // Ganhar corpo depois vira índice; os filhos ficam.
    const nome = primeira.replace(/\/+$/, '').trim();
    if (!nome) return;
    let slug = slugifyJs(nome) || id;
    const res = await fetch(`${API}/instances/${state.id}/notes/${encodeURIComponent(slug)}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
      body: JSON.stringify({ title: nome, markdown: '' }),
    });
    if (!res.ok) { toast('⚠ Falha ao criar pasta'); return; }
    slug = (await res.json()).slug;
    await patch({
      op: 'place_block',
      layer,
      block: {
        id, block_type: 'note', pos: { x: cell.x, y: cell.y },
        label: nome, props: { note_slug: slug },
      },
    });
    await loadNotes();
  } else {
    // nota: 1ª linha = título (inline na grade); resto = corpo.
    // `[] …`/`[x] …`/`[~] …` = tarefa por composição (YG-130): nota + status.
    const chk = primeira.match(/^\[( |x|~)?\]\s*/);
    const status = chk ? ({ x: 'done', '~': 'doing' }[chk[1]] || 'todo') : undefined;
    const semChk = chk ? primeira.slice(chk[0].length).trim() : primeira;
    const titulo = semChk.slice(0, 80) || 'nota';
    const corpo = linhas.slice(1).join('\n').trim();
    let slug = slugifyJs(titulo) || id;
    const res = await fetch(`${API}/instances/${state.id}/notes/${encodeURIComponent(slug)}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
      body: JSON.stringify(status ? { title: titulo, markdown: corpo, status } : { title: titulo, markdown: corpo }),
    });
    if (!res.ok) { toast('⚠ Falha ao criar nota'); return; }
    slug = (await res.json()).slug;
    await patch({
      op: 'place_block',
      layer,
      block: {
        id, block_type: 'note', pos: { x: cell.x, y: cell.y },
        label: titulo, props: { icon: '📝', note_slug: slug },
      },
    });
    await loadNotes();
  }
  renderNotesList();
  render();
}

function wireInline() {
  const input = document.getElementById('inline-input');
  if (!input) return;
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); confirmarInline(); }
    if (e.key === 'Escape') { e.preventDefault(); fecharInline(); }
  });
  input.addEventListener('input', () => {
    // cresce com o texto (Shift+Enter abre o corpo da nota ali mesmo)
    input.rows = Math.min(8, (input.value.match(/\n/g) || []).length + 1);
  });
  input.addEventListener('blur', () => {
    // clicar fora: confirma se há texto, senão só fecha
    if ((input.value || '').trim()) confirmarInline(); else fecharInline();
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
  // YG-129: escopo — começa só entre irmãs (mesmo pai) + as do nó selecionado;
  // "todas" expande para o emaranhado completo.
  const scope = document.createElement('div');
  scope.className = 'palette-item';
  scope.innerHTML = state.linkScope === 'irmas'
    ? '<span class="ico">🧍</span> só entre irmãs <small style="opacity:.5">(clique p/ todas)</small>'
    : '<span class="ico">🌐</span> todas as ligações <small style="opacity:.5">(clique p/ irmãs)</small>';
  scope.onclick = () => {
    state.linkScope = state.linkScope === 'irmas' ? 'todas' : 'irmas';
    renderLinkLegend();
    render();
  };
  el.append(scope);
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
  state.tlCache = null; // blocos mudaram → eventos derivados renascem
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
  const pais = parentMap();
  for (const { block } of allBlocks()) {
    if (!isTimeline() && escondidoPorColapso(block.id, pais)) continue; // pasta fechada
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
  if (state.view === 'grafo') {
    const note = graphNodeAt(mx, my);
    if (note) openNote(note.slug);
    return;
  }

  const hit = blockAt(mx, my);

  // View timeline é uma LENTE read-only: pan no vazio, clique abre/inspeciona.
  if (isTimeline()) {
    if (!hit) {
      state.tlPan = { startMx: mx, startOff: state.tl.off, moved: false };
      return;
    }
    state.selectedBlock = hit.id;
    if (hit.props?.note_slug) openNote(hit.props.note_slug);
    else showInspector(hit);
    render();
    return;
  }

  if (!state.edit) {
    // não-dono: leitura — clique seleciona/inspeciona, nada edita
    state.selectedBlock = hit ? hit.id : null;
    showInspector(hit);
    render();
    return;
  }

  // ── manipulação direta (YG-129): sem modo — o grid É o editor ────────────
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
    // seleciona (revela as ligações do nó) + inspetor; arrasto move/liga;
    // soltar sem mover alterna o colapso de pastas (ver mouseup)
    state.selectedBlock = hit.id;
    state.drag = { blockId: hit.id, layerId: findBlock(hit.id).layer.id, moved: false };
    showInspector(hit);
    render();
    return;
  }
  // célula vazia → composer inline (digite; Enter cria). Paleta só para o
  // tipo explícito `evento` (que pede data); nota/pasta nascem do texto.
  const cell = screenToCell(mx, my);
  const g = gridSpec();
  if (cell.x < 0 || cell.y < 0 || cell.x >= g.width || cell.y >= g.height) return;
  if (state.selectedType === 'evento') { placeBlock(cell); return; }
  abrirInline(cell);
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
        // YG-131: soltar sobre qualquer nó ANINHA (filesystem mental model).
        // Referência é gesto textual: wikilink [[...]] (ou o botão 🔗).
        addConnection(state.drag.blockId, alvo.id, 'parent').then(() => {
          toast(`📁 Movido para dentro de "${alvo.label || 'nó'}"`);
        });
      } else {
        moveBlock(state.drag.blockId, state.drag.layerId, cell);
      }
    }
  } else if (state.drag && !state.drag.moved) {
    // clique simples num PAI (sem arrastar): mostra/esconde os filhos
    const b = findBlock(state.drag.blockId)?.block;
    if (b && (childrenByParent()[b.id] || []).length) {
      state.collapsed[b.id] = !state.collapsed[b.id];
      render();
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
