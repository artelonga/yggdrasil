/* co-mundo.js — navegar/editar um universo do CO dentro do Mundo (YG-153 a+b).
 * Reusa a engine + temas; a fonte de dados é a API do CO (co-vault.js, cliente
 * com cookie compartilhado). `?u=<universe_key>` escolhe o universo (default
 * artelonga). Pasta=sala, nota=objeto; abrir nota lê do CO; editar grava de volta
 * (write-back) — a permissão é a do CO (sem login não-membro → 403 no save). */
import { World } from './mundo/engine.js';
import { THEMES, THEME_BY_ID } from './mundo/themes.js';
import { loadCoVault, buildCoRooms, getCoEntry, saveCoEntry, localize, localesOf } from './mundo/co-vault.js';

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s == null ? '' : s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
const KEY = new URLSearchParams(location.search).get('u') || 'artelonga';

let world, rooms, cur = '__root__';
let raw = [];        // entries cruas (com i18n); a fonte da árvore (ids estáveis)
let locale = '';     // YG-159 i18n: '' = fonte; 'en'/'pt'/… = render localizado
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
  const re = raw.find((x) => x.path === n.slug);
  const tr = re && re.i18n && re.i18n[locale];           // tradução do locale ativo
  const title = (tr && tr.title) || n.title;
  $('panel').innerHTML = `<div class="ph"><span>📝 ${esc(title)}</span><button class="x" id="px">✕</button></div>
    <div class="pb" id="nv"><p class="dim">carregando do CO…</p></div>
    <div class="pf"><button id="ned">✏️ Editar (fonte)</button><span id="nst" class="dim"></span></div>`;
  $('panel').classList.add('open'); $('px').onclick = () => $('panel').classList.remove('open');
  const e = await getCoEntry(KEY, n.slug);
  const srcBody = (e && e.body) || (re && re.body) || '';
  const shown = (tr && tr.body) || srcBody;              // lê localizado; edita a fonte
  $('nv').innerHTML = md(shown) + (locale && tr && tr.body
    ? `<p class="dim" style="margin-top:.6rem">— tradução (${esc(locale.toUpperCase())}); editar altera a fonte —</p>` : '');
  $('ned').onclick = () => {
    $('nv').innerHTML = `<textarea id="nta" rows="12">${esc(srcBody)}</textarea>`;
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

// Hooks determinísticos p/ e2e (sem pixel-math do canvas) — espelha o padrão do
// window.MundoView. Dirigem o MESMO caminho (co-vault) que a UI.
window.CoMundo = {
  key: KEY,
  loadVault: () => loadCoVault(KEY),
  // YG-159: monta as salas de um conjunto de entries e devolve roomOf por slug —
  // hook determinístico p/ provar hierarquia por id estável (frontmatter.parent).
  roomsFrom: (entries) => { const r = buildCoRooms(entries, 'test'); return Object.fromEntries(entries.map((e) => [e.path, r.roomOf(e.path)])); },
  // YG-159 i18n: títulos no locale (sobre as `raw` já carregadas) — prova relabel.
  locales: () => localesOf(raw),
  localized: (loc) => Object.fromEntries(localize(raw, loc).map((e) => [e.path, e.title])),
  read: (slug) => getCoEntry(KEY, slug),
  open: (slug, title) => abrirNota({ slug, title: title || slug }),
  saveNote: (slug, text) => saveCoEntry(KEY, slug, text),
};

// Reconstrói a árvore no locale atual. A identidade (slug/parent) é estável, então
// só os títulos relabelam e a sala atual é preservada — trocar idioma não navega.
function rebuild() {
  const keep = cur;
  rooms = buildCoRooms(localize(raw, locale), KEY);
  enter(rooms.get(keep) ? keep : '__root__');
}

function renderLocales() {
  const locs = localesOf(raw);
  if (!locs.length) { $('locales').innerHTML = ''; return; }
  const opts = [['', 'Fonte']].concat(locs.map((l) => [l, l.toUpperCase()]));
  $('locales').innerHTML = '🌐 ' + opts.map(([v, lbl]) =>
    `<button class="locbtn${v === locale ? ' on' : ''}" data-loc="${v}">${esc(lbl)}</button>`).join('');
  document.querySelectorAll('.locbtn').forEach((b) => (b.onclick = () => {
    locale = b.dataset.loc; rebuild(); renderLocales();
    document.querySelector('#panel').classList.remove('open');
  }));
}

async function boot() {
  world = new World($('cv'), { onInteract });
  renderTemas();
  setTema('modern-office');
  try {
    raw = await loadCoVault(KEY);
    rebuild();
    renderLocales();
    world.start();
    $('hud').textContent = `${KEY} · ${raw.length} entradas do CO`;
  } catch (err) {
    $('hud').textContent = `Não consegui ler o universo "${KEY}" do CO (${err.message}). Público? Logado?`;
  }
}
boot();
