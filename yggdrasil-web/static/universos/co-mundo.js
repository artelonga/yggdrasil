/* co-mundo.js — navegar/editar um universo do CO dentro do Mundo (YG-153 a+b).
 * Reusa a engine + temas; a fonte de dados é a API do CO (co-vault.js, cliente
 * com cookie compartilhado). `?u=<universe_key>` escolhe o universo (default
 * artelonga). Pasta=sala, nota=objeto; abrir nota lê do CO; editar grava de volta
 * (write-back) — a permissão é a do CO (sem login não-membro → 403 no save). */
import { World } from './mundo/engine.js';
import { THEMES, THEME_BY_ID } from './mundo/themes.js';
import { loadCoVault, buildCoRooms, getCoEntry, saveCoEntry } from './mundo/co-vault.js';

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s == null ? '' : s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
const KEY = new URLSearchParams(location.search).get('u') || 'artelonga';

let world, rooms, cur = '__root__';
const navStack = [];

function room(id) { return rooms.get(id); }
function enter(id, spawn) { cur = id; const r = room(id); world.setRoom(r, spawn || r.spawn); renderTrilha(); }

function onInteract(ent) {
  if (!ent) return;
  if (ent.type === 'door') { navStack.push(cur); const t = room(ent.target); enter(ent.target, t.exit ? { x: t.exit.x, y: t.exit.y - 1 } : null); }
  else if (ent.type === 'exit') { enter(ent.target); navStack.pop(); }
  else if (ent.type === 'note') abrirNota(ent);
}

async function abrirNota(n) {
  $('panel').innerHTML = `<div class="ph"><span>📝 ${esc(n.title)}</span><button class="x" id="px">✕</button></div>
    <div class="pb" id="nv"><p class="dim">carregando do CO…</p></div>
    <div class="pf"><button id="ned">✏️ Editar</button><span id="nst" class="dim"></span></div>`;
  $('panel').classList.add('open'); $('px').onclick = () => $('panel').classList.remove('open');
  const e = await getCoEntry(KEY, n.slug);
  const body = (e && e.body) || '';
  $('nv').innerHTML = md(body);
  $('ned').onclick = () => {
    $('nv').innerHTML = `<textarea id="nta" rows="12">${esc(body)}</textarea>`;
    $('ned').textContent = '💾 Salvar no CO';
    $('ned').onclick = async () => {
      $('nst').textContent = 'salvando…';
      const r = await saveCoEntry(KEY, n.slug, $('nta').value, e && e.frontmatter);
      $('nst').textContent = r.ok ? '✓ salvo no CO' : (r.status === 401 || r.status === 403 ? '⚠ sem permissão (entre no CO)' : '⚠ falhou ' + r.status);
    };
  };
}

function md(s) { return esc(s).replace(/^#+ (.*)$/gm, '<h3>$1</h3>').replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>').replace(/\n/g, '<br>'); }

function renderTrilha() {
  const chain = []; let id = cur; while (id) { chain.unshift(id); id = id === '__root__' ? null : (id.includes('/') ? id.slice(0, id.lastIndexOf('/')) : '__root__'); }
  $('trilha').innerHTML = chain.map((id) => `<button class="crumb${id === cur ? ' on' : ''}" data-id="${esc(id)}">${esc(id === '__root__' ? KEY : id.split('/').pop())}</button>`).join(' › ');
  document.querySelectorAll('.crumb').forEach((b) => b.onclick = () => { navStack.length = 0; enter(b.dataset.id); });
}

function renderTemas() {
  $('temas').innerHTML = THEMES.map((t) => `<button class="tbtn" data-id="${t.id}">${esc(t.label.split('·')[1] || t.label)}</button>`).join('');
  document.querySelectorAll('.tbtn').forEach((b) => b.onclick = () => setTema(b.dataset.id));
}
function setTema(id) { const t = THEME_BY_ID[id]; if (!t) return; world.setTheme(t); document.querySelectorAll('.tbtn').forEach((b) => b.classList.toggle('on', b.dataset.id === id)); }

async function boot() {
  world = new World($('cv'), { onInteract });
  renderTemas();
  setTema('modern-office');
  try {
    const entries = await loadCoVault(KEY);
    rooms = buildCoRooms(entries, KEY);
    enter('__root__');
    world.start();
    $('hud').textContent = `${KEY} · ${entries.length} entradas do CO`;
  } catch (err) {
    $('hud').textContent = `Não consegui ler o universo "${KEY}" do CO (${err.message}). Público? Logado?`;
  }
}
boot();
