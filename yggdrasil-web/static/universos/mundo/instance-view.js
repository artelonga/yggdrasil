/* mundo/instance-view.js — a view "🌍 Mundo" do instance view (YG-148).
 *
 * Cola entre o instance view (YG-126) e a engine 2D walkable do protótipo
 * (`engine.js`, INALTERADA — contrato data-agnóstico). Troca a fonte: em vez do
 * `sample.js` mock, deriva as salas da instância real via `loader.js`.
 *
 * Exposto como `window.MundoView` (o `instance.js` é script clássico; este é um
 * módulo ES, então a ponte é o objeto global). Pisar/clicar numa nota lê o `.md`
 * real do NoteStore (GET .../notes/{slug}); pisar/clicar numa porta entra na
 * sala-filha.
 *
 * YG-156: une o loader **lazy** (vault inteiro navegável + tela cheia, YG-151)
 * com o **drag-drop durável** (reposicionar/reparent → `.md`/CO, YG-154). O
 * `findNote` resolve a sala efetiva pelo `loader.roomOf(slug)` (lazy), nunca por
 * um `byId` eager — que não existe mais. */
import { World, isTyping } from './engine.js';
import { THEMES, THEME_BY_ID } from './themes.js';
import { buildRooms } from './loader.js';

let world = null;
let rooms = null;
let cur = null;
let active = false;
let wired = false;
let ctx = null; // { inst, notes, instanceId, api, token, renderMarkdown }
const navStack = [];
// YG-157: stack de universos (vaults) já visitados — espelha o `sala_stack` do
// CO-400 na camada `universe-as-node`. Cada item guarda o universo de onde se veio
// (ctx + loader lazy + sala/posição + coordMap) p/ voltar restaurando avatar/câmera.
// NÃO inclui o universo atual.
const universeStack = [];

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

function room(id) { return rooms && rooms.get(id); } // lazy: layouta a sala ao entrar

// ─── ciclo de vida (montado/desmontado ao trocar de view) ────────────────────
export async function mount(canvas, opts) {
  ctx = opts;
  coordMap = {};
  pending.clear();
  if (!world) world = new World(canvas, { onInteract, onEdge, onDragDrop });
  world.setTheme(THEME_BY_ID['garden-forest'] || THEMES[0]);
  active = true;
  wireFullscreen();
  navStack.length = 0;
  universeStack.length = 0;
  // Constrói o mundo SÍNCRONO (sem portais) → MundoView usável de imediato.
  // Sem isto, `await discoverPortals` deixava uma janela onde `#mundo-ui` está
  // visível mas `rooms` ainda é null → `findNote`/`drag` estouravam (regressão
  // dos e2e YG-154/156). Os portais (cross-universe, YG-157) são descobertos em
  // BACKGROUND e injetados depois — só se a sessão ainda não foi mexida, pra não
  // descartar um drag/reparent em curso.
  rooms = buildRooms(opts.inst, opts.notes, []);
  enterRoom(rooms.rootId);
  world.start();
  discoverPortals(ctx).then((portals) => {
    if (!active || !portals.length) return;
    if (pending.size || Object.keys(coordMap).length) return; // sessão mexida → próximo load mostra
    rooms = buildRooms(opts.inst, opts.notes, portals);
    enterRoom(cur || rooms.rootId);
  });
}

export function unmount() {
  active = false;
  if (isFullscreen()) exitFullscreen();
  if (world) { world.stop(); world.held.clear(); }
  universeStack.length = 0;
  hidePanel();
}

// ─── travessia entre universos (vault→vault, YG-157) ─────────────────────────
const authHdr = () => (ctx.token ? { headers: { Authorization: `Bearer ${ctx.token}` } } : {});

// Universos a que o usuário tem acesso (dele + públicos), exceto o atual. Só
// metadados (id/título) — os mundos só se carregam ao atravessar. A visibilidade
// é a do servidor: `GET /instances/{id}/portals` já filtra (públicos + os do
// caller, exclui a origem) e devolve só o resumo (lazy: o mundo do destino só
// é buscado ao cruzar).
async function discoverPortals(c) {
  try {
    const r = await fetch(`${c.api}/instances/${c.instanceId}/portals`, c.token ? { headers: { Authorization: `Bearer ${c.token}` } } : {});
    if (r.ok) {
      const j = await r.json();
      return (j.portals || [])
        .filter((p) => p && p.id)
        .map((p) => ({ id: p.id, title: p.title || p.id }));
    }
  } catch { /* offline → sem portais (o mundo atual segue navegável) */ }
  return [];
}

// Marca como `back` o portal que volta pro universo de onde se veio (topo da
// pilha) — a engine pinta esse de âmbar (drawPortal) p/ sinalizar o retorno.
function markBack(portals) {
  const top = universeStack[universeStack.length - 1];
  const backId = top && top.ctx && top.ctx.instanceId;
  return portals.map((p) => ({ ...p, back: p.id === backId }));
}

// Atravessa para `universeId`: carrega a instância destino (lazy, só ao cruzar),
// empilha o universo atual (ctx + loader + sala/posição + coordMap) e spawna na
// sala-raiz do destino. Visibilidade é respeitada pelo servidor — 403/404 aborta
// a travessia (fica-se onde está). NÃO regride o lazy: o destino é um loader
// `{rootId,ids,get,roomOf}` novo, navegável por inteiro como o de origem.
async function crossTo(universeId) {
  if (!active || !universeId) return;
  let inst;
  let notes;
  try {
    const ri = await fetch(`${ctx.api}/instances/${universeId}`, authHdr());
    if (!ri.ok) return;
    inst = await ri.json();
    const rn = await fetch(`${ctx.api}/instances/${universeId}/notes`, authHdr());
    notes = rn.ok ? ((await rn.json()).notes || []) : [];
  } catch { return; }
  if (!active) return;
  // guarda o universo de origem com a sala/posição + o coordMap atuais → ao
  // voltar, o avatar/câmera e o layout da sessão são preservados.
  universeStack.push({
    ctx,
    rooms,
    cur,
    pos: world ? { x: world.ax, y: world.ay } : null,
    coordMap,
    title: (ctx.inst && ctx.inst.title) || 'Universo',
  });
  ctx = { ...ctx, inst, notes, instanceId: universeId };
  coordMap = {}; // o drag-drop do destino é seu (commit usa ctx.instanceId)
  pending.clear();
  const portals = markBack(await discoverPortals(ctx));
  if (!active) return;
  rooms = buildRooms(inst, notes, portals);
  navStack.length = 0;
  enterRoom(rooms.rootId);
}

// Volta na pilha de universos até o índice `idx` (que passa a ser o atual),
// restaurando o loader, a sala e a posição do avatar de onde se partiu — sem
// refetch (o loader lazy do origem está em cache na pilha).
function popToUniverse(idx) {
  const target = universeStack[idx];
  if (!target) return;
  universeStack.length = idx; // descarta `idx` e tudo acima
  ctx = target.ctx;
  rooms = target.rooms;
  coordMap = target.coordMap || {};
  pending.clear();
  navStack.length = 0;
  enterRoom(room(target.cur) ? target.cur : rooms.rootId, target.pos);
}

// ─── tela cheia (YG-151): Fullscreen API no container do palco ───────────────
// O #canvas é width/height:100% em `body.mundo`, então pôr o palco (.stage, que
// contém canvas + HUD #mundo-ui) em fullscreen faz o mundo cobrir a tela inteira
// e mantém a HUD por cima. Botão na HUD + tecla `F`; sai com `Esc` (nativo).
function fsTarget() { return world && world.canvas ? world.canvas.parentElement : null; }
function isFullscreen() { return !!document.fullscreenElement; }
function exitFullscreen() { if (document.exitFullscreen) document.exitFullscreen().catch(() => {}); }
function toggleFullscreen() {
  const el = fsTarget();
  if (!el) return;
  if (isFullscreen()) exitFullscreen();
  else if (el.requestFullscreen) el.requestFullscreen().catch(() => {});
}
function onFsChange() {
  if (world) world._resize(); // re-dimensiona o canvas ao entrar E ao sair
  const btn = $('mundo-fs');
  if (btn) btn.textContent = isFullscreen() ? '⛶ Sair (Esc)' : '⛶ Tela cheia';
}
function onKey(e) {
  if (!active || isTyping()) return; // não sequestra digitação (mesma guarda da engine)
  if (e.key === 'f' || e.key === 'F') { e.preventDefault(); toggleFullscreen(); }
}
// Listeners globais ligados uma única vez; gateados por `active`.
function wireFullscreen() {
  const btn = $('mundo-fs');
  if (btn) btn.onclick = toggleFullscreen;
  if (wired) return;
  wired = true;
  window.addEventListener('keydown', onKey);
  document.addEventListener('fullscreenchange', onFsChange);
}

// ─── navegação entre salas ───────────────────────────────────────────────────
function enterRoom(id, spawn) {
  const r = room(id);
  if (!r) return;
  cur = id;
  world.setRoom(r, spawn || r.spawn);
  hidePanel();
  renderUniCrumb();
  renderCrumb();
  renderTree();
}

function onInteract(ent) {
  if (!active || !ent) return; // gate: a engine escuta o teclado globalmente
  if (ent.type === 'portal') { // YG-157: pisar num portal atravessa pra outro vault
    crossTo(ent.universe);
  } else if (ent.type === 'door') {
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

// Registra um movimento no coordMap (memória) e agenda o commit em lote. O
// `room` aqui é a sala efetiva atual da nota nesta sessão — `findNote` o consulta
// antes do `roomOf` do loader, pra achar a nota mesmo após um reparent em sessão.
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

// ─── trilha de UNIVERSOS (YG-157) + trilha de salas + árvore da sala atual ────
// Breadcrumb de universos: os já visitados (stack) + o atual. Clicar num anterior
// volta pra ele (pop da pilha). Escondido enquanto há um só universo na cena.
function renderUniCrumb() {
  const el = $('mundo-uni');
  if (!el) return;
  if (!universeStack.length) { el.hidden = true; el.innerHTML = ''; return; }
  const curTitle = (ctx && ctx.inst && ctx.inst.title) || 'Universo';
  const chain = universeStack
    .map((u, i) => `<button class="mu-crumb" data-idx="${i}">🌐 ${esc(u.title)}</button>`)
    .concat(`<button class="mu-crumb on">🌐 ${esc(curTitle)}</button>`);
  el.hidden = false;
  el.innerHTML = chain.join('<span class="mc-sep">›</span>');
  el.querySelectorAll('.mu-crumb[data-idx]').forEach((b) =>
    (b.onclick = () => popToUniverse(+b.dataset.idx)));
}

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
  // portais p/ outros universos (YG-157): listados à parte, com glifo 🌐.
  const portals = (r.portals || [])
    .map((p) => `<li class="mt-portal" data-uni="${esc(p.universe)}">🌐 ${esc(p.label)}</li>`)
    .join('');
  const notes = r.notes
    .map((n, i) => `<li class="mt-note" data-i="${i}">${ICON[n.kind] || '📝'} ${esc(n.title)}</li>`)
    .join('');
  el.innerHTML = `<div class="mt-here">📍 ${esc(r.title)}</div><ul>${doors}${notes}${portals}</ul>`;
  el.querySelectorAll('.mt-folder').forEach((li) => (li.onclick = () => {
    navStack.push({ from: cur, at: null });
    const tgt = room(li.dataset.id);
    enterRoom(li.dataset.id, tgt && tgt.exit ? { x: tgt.exit.x, y: tgt.exit.y - 1 } : null);
  }));
  el.querySelectorAll('.mt-portal').forEach((li) => (li.onclick = () => crossTo(li.dataset.uni)));
  el.querySelectorAll('.mt-note').forEach((li) => (li.onclick = () => {
    const n = room(cur).notes[+li.dataset.i];
    if (n) abrirNota({ type: 'note', ...n });
  }));
}

// localiza um objeto (nota) pelo slug SEM varrer o vault (não há `byId` no lazy):
// a sala efetiva vem do `coordMap` (movimentos da sessão) ou, na carga fresca, do
// `loader.roomOf(slug)` (override do `.md` + membership estrutural). Só essa sala
// é construída (lazy) pra achar o objeto.
function findNote(slug) {
  if (!rooms) return null;
  const rid = (coordMap[slug] && coordMap[slug].room) || rooms.roomOf(slug);
  if (!rid) return null;
  const r = room(rid);
  if (!r) return null;
  const n = (r.notes || []).find((x) => x.slug === slug);
  if (!n) return null;
  return { room: rid, n };
}

// Ponte com o instance view (script clássico) + hooks determinísticos p/ e2e.
// Os hooks de drag/reparent dirigem o MESMO caminho de persistência que o mouse,
// sem depender da matemática de pixels do canvas (YG-154 e2e no instance view).
window.MundoView = {
  mount,
  unmount,
  get cur() { return cur; },
  get pos() { return world ? { x: world.ax, y: world.ay } : null; },
  get rooms() { return rooms ? rooms.ids : []; }, // toda sala navegável (lazy)
  // YG-157: universo (vault) atual + profundidade da pilha de universos.
  get universe() { return ctx ? ctx.instanceId : null; },
  get universeDepth() { return universeStack.length; },
  get fullscreen() { return isFullscreen(); },
  toggleFullscreen,
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
