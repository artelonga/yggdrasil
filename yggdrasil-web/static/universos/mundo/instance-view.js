/* mundo/instance-view.js — a view "🌍 Mundo" do instance view (YG-148).
 *
 * Cola entre o instance view (YG-126) e a engine 2D walkable do protótipo
 * (`engine.js`, INALTERADA — contrato data-agnóstico). Troca a fonte: em vez do
 * `sample.js` mock, deriva as salas da instância real via `loader.js`.
 *
 * Exposto como `window.MundoView` (o `instance.js` é script clássico; este é um
 * módulo ES, então a ponte é o objeto global). Pisar/clicar numa nota lê o `.md`
 * real do NoteStore (GET .../notes/{slug}); pisar/clicar numa porta entra na
 * sala-filha. CRUD de edição fica pra Fatia 3 (YG-149). */
import { World } from './engine.js';
import { THEMES, THEME_BY_ID } from './themes.js';
import { buildRooms } from './loader.js';

let world = null;
let rooms = null;
let cur = null;
let active = false;
let ctx = null; // { inst, notes, instanceId, api, token, renderMarkdown }
const navStack = [];

// ── manipulação direta (YG-154): drag-drop → coordMap → commit em lote ───────
// O arraste atualiza o objeto em memória (coordMap, chaveado por slug); ao soltar,
// faz-se o commit em LOTE ao backend (`POST /layout`) → grava `pos`/`parent` no
// frontmatter `.md` → write-back ao CO (federação). Não há localStorage: o surface
// real é a instância, e a fonte da verdade é o `.md`/CO — não o navegador.
let coordMap = {}; // slug -> { room, x, y, parent }
const pending = new Set(); // slugs movidos aguardando commit

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s == null ? '' : s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
const ICON = { pasta: '📁', indice: '🗂', artigo: '📝' };

function room(id) { return rooms && rooms.byId[id]; }

// ─── ciclo de vida (montado/desmontado ao trocar de view) ────────────────────
export function mount(canvas, opts) {
  ctx = opts;
  rooms = buildRooms(opts.inst, opts.notes);
  coordMap = {};
  pending.clear();
  if (!world) world = new World(canvas, { onInteract, onEdge, onDragDrop });
  world.setTheme(THEME_BY_ID['garden-forest'] || THEMES[0]);
  active = true;
  navStack.length = 0;
  enterRoom(rooms.rootId);
  world.start();
}

export function unmount() {
  active = false;
  if (world) { world.stop(); world.held.clear(); }
  hidePanel();
}

// ─── navegação entre salas ───────────────────────────────────────────────────
function enterRoom(id, spawn) {
  const r = room(id);
  if (!r) return;
  cur = id;
  world.setRoom(r, spawn || r.spawn);
  hidePanel();
  renderCrumb();
  renderTree();
}

function onInteract(ent) {
  if (!active || !ent) return; // gate: a engine escuta o teclado globalmente
  if (ent.type === 'door') {
    navStack.push({ from: cur, at: { x: ent.x, y: ent.y } });
    const tgt = room(ent.target);
    enterRoom(ent.target, tgt && tgt.exit ? { x: tgt.exit.x, y: tgt.exit.y - 1 } : null);
  } else if (ent.type === 'exit') {
    const back = navStack.pop();
    enterRoom(ent.target, back && back.from === ent.target ? { x: back.at.x, y: back.at.y + 1 } : null);
  } else if (ent.type === 'note') {
    abrirNota(ent);
  }
}

// andar contra a borda de baixo volta pra sala-mãe (mesma semântica do protótipo).
function onEdge(dir) {
  if (!active) return;
  const r = room(cur);
  if (dir === 'down' && r && r.exit) {
    onInteract({ type: 'exit', target: r.exit.target, x: r.exit.x, y: r.exit.y });
  }
}

// ─── drag-drop: reposicionar / reparent + persistir (YG-154) ─────────────────
// Soltar uma nota numa porta (pasta) reparenta-a (move na árvore); soltar numa
// célula livre reposiciona-a na sala atual. Cada solta vira um movimento no
// coordMap e dispara o commit em lote ao backend.
function onDragDrop(ent, tx, ty, target) {
  if (!active || !ent || ent.type !== 'note') return;
  const r = room(cur);
  if (!r) return;
  const obj = ent._ref || r.notes.find((n) => n.slug === ent.slug);
  if (!obj) return;

  // soltar numa porta = reparent: a nota muda de sala (pasta-mãe) na árvore.
  if (target && target.type === 'door') {
    const dest = room(target.target);
    if (!dest) return;
    r.notes = r.notes.filter((n) => n !== obj);
    (dest.notes = dest.notes || []).push(obj);
    recordMove(obj.slug, target.target, obj.x, obj.y, target.target);
    renderTree();
    return;
  }

  // reposicionar na sala atual: respeita parede e tile ocupado.
  if (world.isWall(tx, ty)) return;
  const occ = world.entityAt(tx, ty);
  if (occ && occ._ref !== obj) return;
  obj.x = tx;
  obj.y = ty;
  recordMove(obj.slug, cur, tx, ty);
  renderTree();
}

// Registra um movimento no coordMap (memória) e agenda o commit em lote.
function recordMove(slug, room, x, y, parent) {
  if (!slug) return;
  const prev = coordMap[slug] || {};
  coordMap[slug] = { room, x, y, parent: parent !== undefined ? parent : prev.parent };
  pending.add(slug);
  commitLayout();
}

// Commit em LOTE: envia todos os movimentos pendentes numa só requisição ao
// backend, que grava o `.md` e federa o patch (pos/parent) ao CO.
async function commitLayout() {
  if (!ctx || !ctx.instanceId || !pending.size) return;
  const moves = [...pending].map((slug) => {
    const c = coordMap[slug];
    const m = { slug, pos: { room: c.room, x: c.x, y: c.y } };
    if (c.parent !== undefined) m.parent = c.parent;
    return m;
  });
  pending.clear();
  try {
    await fetch(`${ctx.api}/instances/${encodeURIComponent(ctx.instanceId)}/layout`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(ctx.token ? { Authorization: `Bearer ${ctx.token}` } : {}),
      },
      body: JSON.stringify({ moves }),
    });
  } catch {
    /* offline → o coordMap em memória mantém o estado da sessão */
  }
}

// ─── nota: abre o `.md` REAL do NoteStore (read; edição = YG-149) ────────────
async function abrirNota(ent) {
  const panel = $('mundo-panel');
  if (!panel) return;
  const slug = ent.slug;
  let title = ent.title || slug || 'Nota';
  let body = ent.body || '';
  if (slug) {
    try {
      const r = await fetch(
        `${ctx.api}/instances/${ctx.instanceId}/notes/${encodeURIComponent(slug)}`,
        { headers: ctx.token ? { Authorization: `Bearer ${ctx.token}` } : {} },
      );
      if (r.ok) {
        const n = await r.json();
        body = n.body || '';
        title = n.title || title;
      }
    } catch { /* offline → usa o corpo já carregado pelo instance view */ }
  }
  const md = ctx.renderMarkdown ? ctx.renderMarkdown(body) : esc(body).replace(/\n/g, '<br>');
  panel.innerHTML =
    `<div class="mp-head"><span>${esc(title)}</span><button class="mp-x" aria-label="Fechar">✕</button></div>`
    + `<div class="mp-body" data-slug="${esc(slug || '')}">${md || '<p class="hint">(nota vazia)</p>'}</div>`;
  panel.hidden = false;
  panel.querySelector('.mp-x').onclick = hidePanel;
}
function hidePanel() { const p = $('mundo-panel'); if (p) p.hidden = true; }

// ─── trilha (breadcrumb) + árvore da sala atual ──────────────────────────────
function pathTo(id) {
  const c = [id];
  let p = room(id) && room(id).parent;
  while (p) { c.unshift(p); p = room(p) && room(p).parent; }
  return c;
}
function renderCrumb() {
  const el = $('mundo-crumb');
  if (!el) return;
  el.innerHTML = pathTo(cur)
    .map((id) => `<button class="mc-crumb${id === cur ? ' on' : ''}" data-id="${esc(id)}">${esc(room(id).title)}</button>`)
    .join('<span class="mc-sep">›</span>');
  el.querySelectorAll('.mc-crumb').forEach((b) => (b.onclick = () => { navStack.length = 0; enterRoom(b.dataset.id); }));
}
function renderTree() {
  const el = $('mundo-tree');
  if (!el) return;
  const r = room(cur);
  if (!r) return;
  const doors = r.doors
    .map((d) => `<li class="mt-folder" data-id="${esc(d.target)}">📁 ${esc(d.label)}</li>`)
    .join('');
  const notes = r.notes
    .map((n, i) => `<li class="mt-note" data-i="${i}">${ICON[n.kind] || '📝'} ${esc(n.title)}</li>`)
    .join('');
  el.innerHTML = `<div class="mt-here">📍 ${esc(r.title)}</div><ul>${doors}${notes}</ul>`;
  el.querySelectorAll('.mt-folder').forEach((li) => (li.onclick = () => {
    navStack.push({ from: cur, at: null });
    const tgt = room(li.dataset.id);
    enterRoom(li.dataset.id, tgt && tgt.exit ? { x: tgt.exit.x, y: tgt.exit.y - 1 } : null);
  }));
  el.querySelectorAll('.mt-note').forEach((li) => (li.onclick = () => {
    const n = room(cur).notes[+li.dataset.i];
    if (n) abrirNota({ type: 'note', ...n });
  }));
}

// localiza um objeto (nota) pelo slug em qualquer sala montada.
function findNote(slug) {
  if (!rooms) return null;
  for (const id of Object.keys(rooms.byId)) {
    const n = (rooms.byId[id].notes || []).find((x) => x.slug === slug);
    if (n) return { room: id, n };
  }
  return null;
}

// Ponte com o instance view (script clássico) + hooks determinísticos p/ e2e.
// Os hooks de drag/reparent dirigem o MESMO caminho de persistência que o mouse,
// sem depender da matemática de pixels do canvas (YG-154 e2e no instance view).
window.MundoView = {
  mount,
  unmount,
  get cur() { return cur; },
  get pos() { return world ? { x: world.ax, y: world.ay } : null; },
  get rooms() { return rooms ? Object.keys(rooms.byId) : []; },
  enter: (id) => enterRoom(id),
  posOf: (slug) => { const f = findNote(slug); return f ? { room: f.room, x: f.n.x, y: f.n.y } : null; },
  drag: (slug, tx, ty) => {
    const f = findNote(slug);
    if (!f) return false;
    if (f.room !== cur) enterRoom(f.room);
    onDragDrop({ type: 'note', _ref: f.n, x: f.n.x, y: f.n.y, slug }, tx, ty, null);
    return true;
  },
  reparent: (slug, destRoom) => {
    const f = findNote(slug);
    if (!f || !room(destRoom)) return false;
    if (f.room !== cur) enterRoom(f.room);
    const door = { type: 'door', target: destRoom, label: room(destRoom).title, x: f.n.x, y: f.n.y };
    onDragDrop({ type: 'note', _ref: f.n, x: f.n.x, y: f.n.y, slug }, f.n.x, f.n.y, door);
    return true;
  },
};
