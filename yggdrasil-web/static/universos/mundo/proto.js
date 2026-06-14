/* mundo/proto.js — cola do protótipo (YG-146).
 * Engine + temas + salas + interações + NPC + seletor de tema + árvore viva. */
import { World } from './engine.js';
import { THEMES, THEME_BY_ID } from './themes.js';
import { ROOMS, PARENT } from './sample.js';

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s == null ? '' : s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));

// ── persistência de layout (YG-149) ──────────────────────────────────────────
// Manipulação direta (drag-drop) deixa de ser efêmera: a posição/reparent de cada
// objeto persiste por universo. Local (localStorage) sempre; e, quando o Mundo
// está ligado a uma instância real (`?inst=<id>` + token), faz write-back ao
// backend (`/layout`) → frontmatter `.md` → CO (federação). Sem instância real,
// fica só no local — base navegável pro YG-148 plugar a fonte real.
const QS = (() => { try { return new URLSearchParams(location.search); } catch (_) { return new URLSearchParams(); } })();
const INST = QS.get('inst');
const UNIV = INST || 'mundo-proto';
const TOKEN = (() => { try { return localStorage.getItem('ygg_token'); } catch (_) { return null; } })();
const LKEY = 'mundo_layout_v1';
function loadLayouts() { try { return JSON.parse(localStorage.getItem(LKEY) || '{}'); } catch (_) { return {}; } }
function saveLayouts(o) { try { localStorage.setItem(LKEY, JSON.stringify(o)); } catch (_) {} }
function layoutFor(u) { return loadLayouts()[u] || {}; }
// grava 1 movimento (commit por solta — o arraste em si não escreve a cada pixel)
function recordMove(slug, room, x, y, parent) {
  if (!slug) return;
  const all = loadLayouts(); const u = (all[UNIV] = all[UNIV] || {});
  const prev = u[slug] || {};
  u[slug] = { room, x, y, parent: parent !== undefined ? parent : prev.parent };
  saveLayouts(all);
  mirrorLayout(slug, room, x, y, parent);
}
function mirrorLayout(slug, room, x, y, parent) {
  if (!INST || !TOKEN) return; // sem instância real ligada → só local
  const move = { slug, pos: { room, x, y } };
  if (parent !== undefined) move.parent = parent;
  fetch(`/api/v1/instances/${encodeURIComponent(INST)}/layout`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + TOKEN },
    body: JSON.stringify({ moves: [move] }),
  }).catch(() => {});
}
// aplica o layout salvo sobre os ROOMS (posição + reparent) — chamado no boot
function applyLayout() {
  const L = layoutFor(UNIV);
  for (const [slug, p] of Object.entries(L)) {
    let found = null, fromRoom = null;
    for (const id of Object.keys(ROOMS)) {
      const n = (ROOMS[id].notes || []).find((x) => x.slug === slug);
      if (n) { found = n; fromRoom = id; break; }
    }
    if (!found) continue;
    if (typeof p.x === 'number') found.x = p.x;
    if (typeof p.y === 'number') found.y = p.y;
    const dest = p.parent || p.room;
    if (dest && dest !== fromRoom && ROOMS[dest]) {
      ROOMS[fromRoom].notes = (ROOMS[fromRoom].notes || []).filter((x) => x !== found);
      (ROOMS[dest].notes = ROOMS[dest].notes || []).push(found);
    }
  }
}

let world, curId = 'raiz';
let temaAtual = 'medieval-castle'; // versão ativa (amarra telemetria + feedback)
const navStack = []; // {fromRoom, doorXY} para voltar ao ponto certo

// ── navegação entre salas ──────────────────────────────────────────────────
function enterRoom(id, spawn) {
  const r = ROOMS[id];
  if (!r) return;
  curId = id;
  world.setRoom(r, spawn || r.spawn);
  renderTrilha();
}
function onInteract(ent) {
  if (!ent) return;
  if (ent.type === 'door') {
    navStack.push({ from: curId, at: { x: ent.x, y: ent.y } });
    const tgt = ROOMS[ent.target];
    enterRoom(ent.target, tgt && tgt.exit ? { x: tgt.exit.x, y: tgt.exit.y - 1 } : null);
  } else if (ent.type === 'exit') {
    const back = navStack.pop();
    enterRoom(ent.target, back && back.from === ent.target ? { x: back.at.x, y: back.at.y + 1 } : null);
  } else if (ent.type === 'note') {
    abrirNota(ent);
  } else if (ent.type === 'npc') {
    abrirNPC(ent);
  }
}

// ── navegação por parede (andar contra a borda) ─────────────────────────────
// ↑ topo = entra (mais fundo) · ↓ baixo = Home (raiz/índice, NPC) · ←→ = pasta irmã
function navHome() { navStack.length = 0; enterRoom('raiz'); toast('🏠 Início (índice)'); }
function forward() {
  const d = (ROOMS[curId].doors || [])[0];
  if (!d) { toast('Sem subpasta aqui'); return; }
  navStack.push({ from: curId, at: { x: d.x, y: d.y } });
  const t = ROOMS[d.target];
  enterRoom(d.target, t && t.exit ? { x: t.exit.x, y: t.exit.y - 1 } : null);
  toast(`↪ ${ROOMS[d.target].title}`);
}
function siblingsOf(id) {
  const parent = PARENT[id] ? ROOMS[PARENT[id]] : ROOMS['raiz'];
  return (parent.doors || []).map((d) => d.target);
}
function sibling(delta) {
  const sibs = siblingsOf(curId);
  if (!sibs.length) { toast('Sem pastas irmãs'); return; }
  let i = sibs.indexOf(curId);
  i = i < 0 ? (delta > 0 ? 0 : sibs.length - 1) : (i + delta + sibs.length) % sibs.length;
  const id = sibs[i], t = ROOMS[id];
  navStack.length = 0; if (PARENT[id]) navStack.push({ from: PARENT[id], at: null });
  enterRoom(id, t && t.exit ? { x: t.exit.x, y: t.exit.y - 1 } : null);
  toast(`📁 ${t.title}`);
}
function onEdge(dir) {
  if (dir === 'down') navHome();
  else if (dir === 'up') forward();
  else if (dir === 'left') sibling(-1);
  else if (dir === 'right') sibling(1);
}

// ── ferramentas do lobby (4 ícones à esquerda) + overlays ────────────────────
let npcTop = false; // A/B: NPC sempre no topo (B) vs ícone lateral (A)
function renderFerramentas() {
  const tools = [
    { ic: '🏠', t: 'Início', fn: navHome },
    { ic: '📋', t: 'Quadro (Kanban)', fn: abrirKanban },
    { ic: '🕐', t: 'Linha do tempo', fn: abrirTimeline },
  ];
  if (!npcTop) tools.push({ ic: '🧙', t: 'Guia (NPC)', fn: () => abrirNPC({ name: 'Guia' }) });
  $('ferramentas').innerHTML = tools.map((x, i) => `<button class="tool" data-i="${i}" title="${x.t}">${x.ic}</button>`).join('');
  document.querySelectorAll('#ferramentas .tool').forEach((b, i) => (b.onclick = tools[i].fn));
  $('npc-top').style.display = npcTop ? '' : 'none';
}
function abrirOverlay(title, html) {
  $('overlay').innerHTML = `<div class="ov-card"><div class="ph"><span>${title}</span><button class="x" id="ov-x">✕</button></div><div class="ov-body">${html}</div></div>`;
  $('overlay').classList.add('open');
  $('ov-x').onclick = fecharOverlay;
  $('overlay').onclick = (e) => { if (e.target === $('overlay')) fecharOverlay(); };
}
function fecharOverlay() { $('overlay').classList.remove('open'); }
function irNaTarefa(room, slug) {
  navStack.length = 0; if (room !== curId) enterRoom(room);
  const n = (ROOMS[room].notes || []).find((x) => x.slug === slug); if (n) abrirNota(n);
}
function tarefasTodas() {
  const out = [];
  for (const id of Object.keys(ROOMS)) for (const n of (ROOMS[id].notes || [])) if (n.status) out.push({ room: id, n });
  return out;
}
function abrirKanban() {
  const cols = [['todo', 'A fazer'], ['doing', 'Fazendo'], ['done', 'Feita']];
  const all = tarefasTodas();
  const colHtml = cols.map(([s, label]) => {
    const items = all.filter((t) => t.n.status === s);
    const cards = items.map((t) => `<div class="kcard" data-room="${t.room}" data-slug="${esc(t.n.slug)}">${esc(t.n.title)}<span class="kroom">${esc(ROOMS[t.room].title)}</span></div>`).join('') || '<div class="kempty">—</div>';
    return `<div class="kcol"><div class="khead st ${s}">${label} <span>${items.length}</span></div>${cards}</div>`;
  }).join('');
  abrirOverlay('📋 Quadro — tarefas do universo', `<div class="kanban">${colHtml}</div><p class="ohint">Centralizado: agrega tarefas (notas com status) de <b>todas as salas</b>. No produto, exporta/espelha o quadro (kanban) do CO.</p>`);
  document.querySelectorAll('.kcard').forEach((c) => (c.onclick = () => { fecharOverlay(); irNaTarefa(c.dataset.room, c.dataset.slug); }));
}
function abrirTimeline() {
  const rows = [];
  for (const id of Object.keys(ROOMS)) for (const n of (ROOMS[id].notes || [])) rows.push(`<li data-room="${id}" data-slug="${esc(n.slug)}"><b>${esc(n.title)}</b> <span class="kroom">${esc(ROOMS[id].title)}</span>${n.status ? ' <span class="st ' + n.status + '">' + n.status + '</span>' : ''}</li>`);
  abrirOverlay('🕐 Linha do tempo', `<ul class="tl">${rows.join('')}</ul><p class="ohint">Protótipo: lista por sala. No produto, eixo temporal real (view timeline do instance).</p>`);
  document.querySelectorAll('.tl li').forEach((li) => (li.onclick = () => { fecharOverlay(); irNaTarefa(li.dataset.room, li.dataset.slug); }));
}

// ── painel de nota (CRUD ao pisar OU ao clicar no menu) ──────────────────────
// Variantes A/B da dica de edição (← → no rodapé do editor pra alternar/testar).
const HINT_VARIANTS = [
  { hint: 'Enter salva · Shift+Enter quebra linha', saveOnEnter: true },
  { hint: 'Shift+Enter salva · Enter quebra linha', saveOnEnter: false },
];
let hintVar = 0;

function abrirNota(input) {
  const n = input._ref || input; // aceita entidade da engine ou objeto direto
  const kindIco = { pasta: '📁', indice: '🗂', artigo: '📝' }[n.kind] || '📝';
  $('panel').innerHTML = `
    <div class="ph"><span>${kindIco} ${esc(n.title)}</span><button class="x" id="p-x" title="Fechar (Esc)">✕</button></div>
    <div class="pb" id="nota-view">${renderMd(n.body || '')}</div>
    <div class="pf">
      <button id="nota-edit">✏️ Editar</button>
      <button id="nota-del" class="danger">🗑 Excluir</button>
    </div>`;
  showPanel();
  $('p-x').onclick = hidePanel;
  $('nota-edit').onclick = () => editarNota(n);
  $('nota-del').onclick = () => {
    const r = ROOMS[curId]; r.notes = (r.notes || []).filter((x) => x !== n);
    hidePanel(); renderTrilha(); toast('Nota excluída (mock)');
  };
}
function editarNota(n) {
  function render() {
    const v = HINT_VARIANTS[hintVar];
    $('nota-view').innerHTML = `
      <textarea id="nota-ta" rows="7">${esc(n.body || '')}</textarea>
      <div class="hintbar">
        <button class="hb-nav" id="hb-prev" title="ver outra variante (A/B)">‹</button>
        <span class="hint">⌨️ ${v.hint}</span>
        <button class="hb-nav" id="hb-next" title="ver outra variante (A/B)">›</button>
      </div>`;
    const ta = $('nota-ta'); ta.focus(); ta.setSelectionRange(ta.value.length, ta.value.length);
    ta.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        const save = v.saveOnEnter ? !e.shiftKey : e.shiftKey;
        if (save) { e.preventDefault(); salvar(); } // senão: textarea insere \n naturalmente
      }
    });
    $('hb-prev').onclick = $('hb-next').onclick = () => { hintVar = (hintVar + 1) % HINT_VARIANTS.length; render(); };
  }
  function salvar() { n.body = $('nota-ta').value; toast('Nota salva (mock)'); abrirNota(n); }
  render();
  $('nota-edit').textContent = '💾 Salvar';
  $('nota-edit').onclick = salvar; // botão de salvar sempre disponível
}

// ── drag-drop: reposicionar (state) ou soltar em pasta (reparent) ────────────
function onDragDrop(ent, tx, ty, target) {
  const r = ROOMS[curId];
  const obj = ent._ref;
  if (!obj) return;
  if (target && target.type === 'door' && ent.type === 'note') {
    r.notes = (r.notes || []).filter((x) => x !== obj);
    const dest = ROOMS[target.target]; (dest.notes = dest.notes || []).push(obj);
    recordMove(obj.slug, target.target, obj.x, obj.y, target.target); // reparent persiste
    renderTrilha(); toast(`"${obj.title}" movida → ${target.label} (persistido)`); return;
  }
  if (world.isWall(tx, ty)) { toast('Não dá pra soltar aí'); return; }
  const occ = world.entityAt(tx, ty);
  if (occ && !(occ.x === obj.x && occ.y === obj.y)) { toast('Tile ocupado'); return; }
  obj.x = tx; obj.y = ty;
  recordMove(obj.slug, curId, tx, ty); // posição persiste (.md/CO quando ligado)
  renderTrilha(); toast('Reposicionado ✓ (persistido)');
}

// ── NPC: tutoriais determinísticos (tutorial.md) + LLM local (Ollama) ────────
// Decisão YG-146: LLM local via Ollama no servidor real (POST /api/v1/npc),
// com FALLBACK determinístico pro tutorial.md (funciona sem backend nenhum).
let TUT = []; // [{title, body}] parseado do tutorial.md
const tutLoaded = fetch('/static/universos/mundo/tutorial.md')
  .then((r) => (r.ok ? r.text() : '')).then(parseTut).catch(() => {});
function parseTut(md) {
  TUT = []; let cur = null;
  for (const ln of String(md || '').split('\n')) {
    const m = /^##\s+(.*)$/.exec(ln);
    if (m) { cur = { title: m[1].trim(), body: '' }; TUT.push(cur); }
    else if (cur && !/^#\s/.test(ln)) cur.body += ln + '\n';
  }
  TUT.forEach((t) => { t.body = t.body.trim(); });
  return TUT;
}
function fold(s) { return String(s || '').toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, ''); }
function respostaDeterministica(q) {
  const qw = fold(q).split(/[^a-z0-9]+/).filter((w) => w.length > 2);
  let best = null, score = 0;
  for (const t of TUT) {
    const hay = fold(t.title + ' ' + t.body);
    let s = 0; for (const w of qw) if (hay.includes(w)) s++;
    if (s > score) { score = s; best = t; }
  }
  return best && score > 0 ? best.body
    : 'Não sei isso ainda — tente um dos tópicos acima. Com o **LLM local (Ollama)** ligado eu respondo perguntas livres.';
}
async function perguntarNPC(q) {
  try { // tenta o endpoint LLM (Ollama) no servidor real; senão, determinístico
    const r = await fetch('/api/v1/npc', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ q, universo: curId }) });
    if (r.ok) { const d = await r.json(); if (d && d.answer) return d.answer; }
  } catch (_) { /* sem backend → fallback */ }
  return respostaDeterministica(q);
}
async function abrirNPC(p) {
  await tutLoaded;
  const topicos = TUT.map((t, i) => `<button class="topic" data-i="${i}">${esc(t.title)}</button>`).join('');
  $('panel').innerHTML = `
    <div class="ph"><span>🧙 ${esc(p.name)}</span><button class="x" id="p-x" title="Fechar (Esc)">✕</button></div>
    <div class="pb">
      <div id="npc-msg" class="npc-msg">Olá! Sou o <b>Guia</b>. Escolha um tópico ou me pergunte algo.</div>
      <div class="topics">${topicos}</div>
      <div class="ask">
        <input id="npc-q" type="text" placeholder="Pergunte qualquer coisa…" />
        <button id="npc-send">Perguntar</button>
      </div>
    </div>`;
  showPanel();
  $('p-x').onclick = hidePanel;
  document.querySelectorAll('.topic').forEach((b) => b.onclick = () => { $('npc-msg').innerHTML = renderMd(TUT[+b.dataset.i].body); });
  async function send() {
    const q = $('npc-q').value.trim(); if (!q) return;
    $('npc-msg').innerHTML = '<i>pensando…</i>'; $('npc-q').value = '';
    $('npc-msg').innerHTML = renderMd(await perguntarNPC(q));
  }
  $('npc-send').onclick = send;
  $('npc-q').addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); send(); } });
}

// ── seletor de tema ──────────────────────────────────────────────────────────
function renderTemas() {
  const fam = { medieval: '🏰 Medieval', garden: '🌿 Jardim', modern: '🏢 Moderno' };
  const byFam = {};
  THEMES.forEach((t) => { (byFam[t.family] = byFam[t.family] || []).push(t); });
  $('temas').innerHTML = Object.keys(byFam).map((f) =>
    `<div class="fam"><span class="fl">${fam[f] || f}</span>` +
    byFam[f].map((t) => `<button class="tbtn" data-id="${t.id}">${esc(t.label.split('·')[1] || t.label)}</button>`).join('') +
    `</div>`).join('');
  document.querySelectorAll('.tbtn').forEach((b) => b.onclick = () => setTema(b.dataset.id));
}
function setTema(id) {
  const t = THEME_BY_ID[id]; if (!t) return;
  temaAtual = id;
  world.setTheme(t);
  try { localStorage.setItem('mundo_tema', id); } catch (_) {}
  document.querySelectorAll('.tbtn').forEach((b) => b.classList.toggle('on', b.dataset.id === id));
  $('tema-nome').textContent = t.label;
  // rastreia qual versão o usuário está vendo (feedback amarra a isto)
  if (window.yggTelemetria) window.yggTelemetria.track('mundo_tema', { tema: id });
}

// feedback amarrado à versão (tema) — vai pro mural do CO via /api/v1/feedback
function abrirFeedback() {
  const inp = 'width:100%;padding:.5rem;background:#0b0b0f;border:1px solid #33334a;border-radius:6px;color:#e8e3d3;font:inherit';
  abrirOverlay('💬 Feedback', `
    <p class="ohint">Você está no tema <b>${esc(THEME_BY_ID[temaAtual].label)}</b> — seu feedback será marcado com ele.</p>
    <input id="fb-nome" type="text" placeholder="Nome (opcional)" style="${inp};margin-bottom:.5rem">
    <textarea id="fb-msg" rows="5" placeholder="O que achou? (movimento, visual, navegação, NPC…)" style="${inp}"></textarea>
    <div style="margin-top:.6rem"><button id="fb-send" style="padding:.45rem 1rem;border:none;background:#6366f1;color:#fff;border-radius:7px;cursor:pointer">Enviar</button>
    <span id="fb-stat" style="margin-left:.6rem;font-size:.82rem;color:#8a8a9e"></span></div>`);
  $('fb-send').onclick = async () => {
    const msg = $('fb-msg').value.trim();
    if (!msg) { $('fb-stat').textContent = 'escreva algo'; return; }
    $('fb-send').disabled = true; $('fb-stat').textContent = 'enviando…';
    try {
      const r = await fetch('/api/v1/feedback', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ universe: 'mundo', kind: 'tema:' + temaAtual, message: msg, name: $('fb-nome').value.trim() || null }),
      });
      $('fb-stat').textContent = r.ok ? '✓ obrigado!' : '⚠ falhou (' + r.status + ')';
      if (window.yggTelemetria) window.yggTelemetria.track('mundo_feedback', { tema: temaAtual });
      if (r.ok) setTimeout(fecharOverlay, 900);
    } catch (_) { $('fb-stat').textContent = '⚠ rede'; }
    $('fb-send').disabled = false;
  };
}

// ── trilha / árvore viva ─────────────────────────────────────────────────────
function caminho(id) { const c = [id]; let p = PARENT[id]; while (p) { c.unshift(p); p = PARENT[p]; } return c; }
function renderTrilha() {
  $('trilha').innerHTML = caminho(curId).map((id, i, a) =>
    `<button class="crumb${id === curId ? ' on' : ''}" data-id="${id}">${esc(ROOMS[id].title)}</button>` +
    (i < a.length - 1 ? '<span class="sep">›</span>' : '')).join('');
  document.querySelectorAll('.crumb').forEach((b) => b.onclick = () => {
    // navega direto via trilha (recompõe stack simples)
    navStack.length = 0; enterRoom(b.dataset.id);
  });
  // árvore: sala atual + filhos (portas) + notas
  const r = ROOMS[curId];
  const filhos = (r.doors || []).map((d) => `<li class="t-folder" data-t="${d.target}">📁 ${esc(d.label)}</li>`).join('');
  const notas = (r.notes || []).map((n, i) => `<li class="t-note" data-i="${i}">${({ pasta: '📁', indice: '🗂', artigo: '📝' }[n.kind] || '📝')} ${esc(n.title)}${n.status ? ' <span class="st ' + n.status + '">' + n.status + '</span>' : ''}</li>`).join('');
  $('arvore').innerHTML = `<div class="t-here">📍 ${esc(r.title)}</div><ul>${filhos}${notas}</ul>`;
  document.querySelectorAll('.t-folder').forEach((li) => li.onclick = () => { navStack.push({ from: curId, at: null }); enterRoom(li.dataset.t); });
  // clicar no item do menu abre a nota pra editar (sem precisar caminhar até ela)
  document.querySelectorAll('.t-note').forEach((li) => li.onclick = () => { const n = (ROOMS[curId].notes || [])[+li.dataset.i]; if (n) abrirNota(n); });
}

// ── nova pasta (demonstra hierarquia em tempo real) ──────────────────────────
function novaPasta() {
  const nome = prompt('Nome da nova sala/pasta:'); if (!nome) return;
  const r = ROOMS[curId];
  // acha um tile livre na parede de cima
  let x = 2; while ((r.doors || []).some((d) => d.x === x && d.y === 2) && x < r.w - 2) x += 2;
  const id = 'sala-' + Math.random().toString(36).slice(2, 7);
  ROOMS[id] = { id, title: nome, w: 9, h: 7, tiles: salaVazia(9, 7), spawn: { x: 4, y: 4 }, exit: { x: 4, y: 5, target: curId }, notes: [] };
  PARENT[id] = curId;
  (r.doors = r.doors || []).push({ x, y: 2, label: nome, target: id });
  renderTrilha(); toast('Sala criada — a árvore atualizou na hora');
}
function salaVazia(w, h) {
  const t = []; for (let y = 0; y < h; y++) { const row = []; for (let x = 0; x < w; x++) row.push(x === 0 || y === 0 || x === w - 1 || y === h - 1 ? 1 : 0); t.push(row); } return t;
}

// ── util de UI ───────────────────────────────────────────────────────────────
function renderMd(src) {
  return esc(src).replace(/^# (.*)$/gm, '<h3>$1</h3>').replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>').replace(/\*([^*]+)\*/g, '<i>$1</i>').replace(/\n/g, '<br>');
}
function showPanel() { $('panel').classList.add('open'); }
function hidePanel() { $('panel').classList.remove('open'); }
function toast(m) { const t = $('toast'); t.textContent = m; t.classList.add('show'); setTimeout(() => t.classList.remove('show'), 1800); }

// ── boot ─────────────────────────────────────────────────────────────────────
applyLayout(); // YG-149: reabrir o Mundo mantém o layout persistido
world = new World($('cv'), { onInteract, onDragDrop, onEdge });
renderTemas();
renderFerramentas();
// ?tema=<id> (deep-link de versão) tem prioridade; senão localStorage; senão default
let inicial = 'medieval-castle';
try {
  const q = new URLSearchParams(location.search).get('tema');
  inicial = (q && THEME_BY_ID[q] && q) || localStorage.getItem('mundo_tema') || inicial;
} catch (_) {}
setTema(THEME_BY_ID[inicial] ? inicial : 'medieval-castle');
enterRoom('raiz');
world.start();
$('nova-pasta').onclick = novaPasta;
$('fb-btn').onclick = abrirFeedback;
$('npc-top').onclick = () => abrirNPC({ name: 'Guia' });
$('npc-ab').onclick = () => { npcTop = !npcTop; $('npc-ab').textContent = npcTop ? 'NPC: topo (B)' : 'NPC: lateral (A)'; renderFerramentas(); };
window.addEventListener('keydown', (e) => { if (e.key === 'Escape') { hidePanel(); fecharOverlay(); } });

// hook de teste (YG-149 e2e): dirige o MESMO caminho de drag-drop/persistência
// que o mouse — sem depender da matemática de pixels do canvas.
function findNote(slug) {
  for (const id of Object.keys(ROOMS)) {
    const n = (ROOMS[id].notes || []).find((x) => x.slug === slug);
    if (n) return { room: id, n };
  }
  return null;
}
window.__mundo = {
  ready: true,
  cur: () => curId,
  enter: (id) => enterRoom(id),
  posOf: (slug) => { const f = findNote(slug); return f ? { room: f.room, x: f.n.x, y: f.n.y } : null; },
  drag: (slug, tx, ty) => {
    const f = findNote(slug); if (!f) return false;
    if (f.room !== curId) enterRoom(f.room);
    onDragDrop({ type: 'note', _ref: f.n, x: f.n.x, y: f.n.y }, tx, ty, null);
    return true;
  },
  reparent: (slug, destRoom) => {
    const f = findNote(slug); if (!f || !ROOMS[destRoom]) return false;
    if (f.room !== curId) enterRoom(f.room);
    const door = { type: 'door', target: destRoom, label: ROOMS[destRoom].title, x: f.n.x, y: f.n.y };
    onDragDrop({ type: 'note', _ref: f.n, x: f.n.x, y: f.n.y }, f.n.x, f.n.y, door);
    return true;
  },
};
