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
// YG-152: stack de universos (vaults) já visitados — espelha o `sala_stack` do
// CO-400 na camada `universe-as-node`. Cada item guarda o universo de onde se veio
// p/ poder voltar (breadcrumb de universo). NÃO inclui o universo atual.
const universeStack = [];

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s == null ? '' : s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
const ICON = { pasta: '📁', indice: '🗂', artigo: '📝' };

function room(id) { return rooms && rooms.byId[id]; }

// ─── ciclo de vida (montado/desmontado ao trocar de view) ────────────────────
export async function mount(canvas, opts) {
  ctx = opts;
  if (!world) world = new World(canvas, { onInteract, onEdge });
  world.setTheme(THEME_BY_ID['garden-forest'] || THEMES[0]);
  active = true;
  navStack.length = 0;
  universeStack.length = 0;
  // descobre universos vizinhos visíveis (lazy: só metadados, não os mundos).
  const portals = await discoverPortals(ctx);
  if (!active) return; // trocou de view enquanto buscava
  rooms = buildRooms(opts.inst, opts.notes, portals);
  enterRoom(rooms.rootId);
  world.start();
}

export function unmount() {
  active = false;
  if (world) { world.stop(); world.held.clear(); }
  universeStack.length = 0;
  hidePanel();
}

// ─── travessia entre universos (vault→vault, YG-152) ─────────────────────────
const authHdr = () => (ctx.token ? { headers: { Authorization: `Bearer ${ctx.token}` } } : {});

// Universos a que o usuário tem acesso (dele + públicos), exceto o atual. Só
// metadados (id/título) — os mundos só se carregam ao atravessar. A visibilidade
// é a do servidor: `GET /instances/{id}/portals` já filtra (públicos + os do
// caller, exclui a origem) e devolve só o resumo (lazy: o mundo do destino só
// é buscado ao cruzar).
async function discoverPortals(c) {
  try {
    const r = await fetch(`${c.api}/instances/${c.instanceId}/portals`, authHdr());
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

// Atravessa para `universeId`: carrega a instância destino (lazy), empilha o
// universo atual e spawna na sala-raiz do destino. Visibilidade é respeitada
// pelo servidor — 403/404 aborta a travessia (fica-se onde está).
async function crossTo(universeId) {
  if (!active || !universeId) return;
  let inst, notes;
  try {
    const ri = await fetch(`${ctx.api}/instances/${universeId}`, authHdr());
    if (!ri.ok) return;
    inst = await ri.json();
    const rn = await fetch(`${ctx.api}/instances/${universeId}/notes`, authHdr());
    notes = rn.ok ? ((await rn.json()).notes || []) : [];
  } catch { return; }
  if (!active) return;
  // guarda o universo de origem com a sala/posição atuais → ao voltar, o
  // avatar/câmera são preservados (reaparece onde estava, não no spawn).
  universeStack.push({
    ctx,
    rooms,
    cur,
    pos: world ? { x: world.ax, y: world.ay } : null,
    title: (ctx.inst && ctx.inst.title) || 'Universo',
  });
  ctx = { ...ctx, inst, notes, instanceId: universeId };
  const portals = markBack(await discoverPortals(ctx));
  if (!active) return;
  rooms = buildRooms(inst, notes, portals);
  navStack.length = 0;
  enterRoom(rooms.rootId);
}

// Volta na pilha de universos até o índice `idx` (que passa a ser o atual),
// restaurando a sala e a posição do avatar de onde se partiu.
function popToUniverse(idx) {
  const target = universeStack[idx];
  if (!target) return;
  universeStack.length = idx; // descarta `idx` e tudo acima
  ctx = target.ctx;
  rooms = target.rooms;
  navStack.length = 0;
  enterRoom(room(target.cur) ? target.cur : rooms.rootId, target.pos);
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
  if (ent.type === 'portal') { // YG-152: pisar num portal atravessa pra outro vault
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

// ─── trilha de UNIVERSOS (YG-152) + trilha de salas + árvore da sala atual ────
// Breadcrumb de universos: os já visitados (stack) + o atual. Clicar num anterior
// volta pra ele (pop da pilha). Escondido enquanto há um só universo na cena.
function renderUniCrumb() {
  const el = $('mundo-uni');
  if (!el) return;
  const curTitle = (ctx && ctx.inst && ctx.inst.title) || 'Universo';
  if (!universeStack.length) { el.hidden = true; el.innerHTML = ''; return; }
  const chain = universeStack
    .map((u, i) => `<button class="mu-crumb" data-idx="${i}">🌐 ${esc(u.title)}</button>`)
    .concat(`<button class="mu-crumb on">🌐 ${esc(curTitle)}</button>`);
  el.hidden = false;
  el.innerHTML = chain.join('<span class="mc-sep">›</span>');
  el.querySelectorAll('.mu-crumb[data-idx]').forEach((b) =>
    (b.onclick = () => popToUniverse(+b.dataset.idx)));
}

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
  // portais p/ outros universos (YG-152): listados à parte, com glifo 🌐.
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

// Ponte com o instance view (script clássico) + hooks determinísticos p/ e2e.
window.MundoView = {
  mount,
  unmount,
  get cur() { return cur; },
  get pos() { return world ? { x: world.ax, y: world.ay } : null; },
  get rooms() { return rooms ? Object.keys(rooms.byId) : []; },
  // YG-152: universo (vault) atual + profundidade da pilha de universos.
  get universe() { return ctx ? ctx.instanceId : null; },
  get universeDepth() { return universeStack.length; },
};
