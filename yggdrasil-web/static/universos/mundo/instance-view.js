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

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s == null ? '' : s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
const ICON = { pasta: '📁', indice: '🗂', artigo: '📝' };

function room(id) { return rooms && rooms.get(id); } // lazy: layouta a sala ao entrar

// ─── ciclo de vida (montado/desmontado ao trocar de view) ────────────────────
export function mount(canvas, opts) {
  ctx = opts;
  rooms = buildRooms(opts.inst, opts.notes);
  if (!world) world = new World(canvas, { onInteract, onEdge });
  world.setTheme(THEME_BY_ID['garden-forest'] || THEMES[0]);
  active = true;
  wireFullscreen();
  navStack.length = 0;
  enterRoom(rooms.rootId);
  world.start();
}

export function unmount() {
  active = false;
  if (isFullscreen()) exitFullscreen();
  if (world) { world.stop(); world.held.clear(); }
  hidePanel();
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

// Ponte com o instance view (script clássico) + hooks determinísticos p/ e2e.
window.MundoView = {
  mount,
  unmount,
  get cur() { return cur; },
  get pos() { return world ? { x: world.ax, y: world.ay } : null; },
  get rooms() { return rooms ? rooms.ids : []; }, // toda sala navegável (lazy)
  get fullscreen() { return isFullscreen(); },
  toggleFullscreen,
};
