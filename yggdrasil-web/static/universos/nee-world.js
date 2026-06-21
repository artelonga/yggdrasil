/* nee-world.js — sala caminhável de um LexiconPack (YG-167).
 *
 * Combina:
 *   packToRoom      (nee/pack-room.js)   → Room JSON
 *   World engine    (mundo/engine.js)    → canvas walkable
 *   AudioEngine     (nee/audio-engine.js)→ pisar toca o som
 *   PackLoader      (nee/pack-loader.js) → lazy-load, LRU
 *
 * Pisar/clicar numa entrada → toca entry.audio + abre inspector
 * (term/gloss/role/relations). Clicar numa relação → goTo(entrada-alvo).
 *
 * window.__neeWorld expõe engine + room + stepOn(term) p/ e2e/observabilidade. */

import { World } from './mundo/engine.js';
import { THEMES, THEME_BY_ID } from './mundo/themes.js';
import { AudioEngine } from './nee/audio-engine.js';
import { PackLoader } from './nee/pack-loader.js';
import { packToRoom } from './nee/pack-room.js';

const audioEngine = new AudioEngine();
const packLoader = new PackLoader();
let world = null;
let currentRoom = null;
const noteByTerm = new Map(); // term → note object (p/ navegar via relações)

const $ = (id) => document.getElementById(id);

// Expõe p/ e2e (AudioEngine.stats) e observabilidade — sem side-effects.
window.__neeWorld = {
  audioEngine,
  packLoader,
  get room() { return currentRoom; },
  /** Simula "pisar" numa entrada pelo term — atalho de teste sem depender de pixels. */
  stepOn(term) {
    const note = noteByTerm.get(term);
    if (!note || !world) return false;
    _interact({ type: 'note', _ref: note, ...note });
    return true;
  },
};

// ── interação ────────────────────────────────────────────────────────────────

function _interact(ent) {
  if (!ent || ent.type !== 'note') return;
  if (ent.audio) audioEngine.play(ent.audio);
  _showInspector(ent);
}

function _showInspector(entry) {
  const panel = $('inspector');
  if (!panel) return;

  $('ins-term').textContent = entry.term || '';
  $('ins-gloss').textContent = entry.gloss || '';
  $('ins-role').textContent = entry.role ? `[${entry.role}]` : '';

  const relSection = $('ins-relations');
  const relList = $('ins-rel-list');
  const rels = entry.relations || [];

  if (relSection) relSection.hidden = rels.length === 0;
  if (relList) {
    relList.innerHTML = '';
    for (const rel of rels) {
      const li = document.createElement('li');
      const btn = document.createElement('button');
      btn.className = 'rel-link';
      btn.dataset.to = rel.to;
      btn.textContent = (rel.label ? rel.label + ': ' : '') + rel.to;
      btn.addEventListener('click', () => {
        const target = noteByTerm.get(rel.to);
        if (target && world) world.goTo(target.x, target.y);
        panel.hidden = true;
      });
      li.appendChild(btn);
      relList.appendChild(li);
    }
  }

  panel.hidden = false;
}

// ── carregamento ─────────────────────────────────────────────────────────────

async function _loadPack(packId) {
  const logEl = $('log');
  const titleEl = $('pack-title');
  try {
    if (logEl) logEl.textContent = `carregando ${packId}…`;
    const pack = await packLoader.load(packId);
    if (titleEl) titleEl.textContent = pack.title;

    currentRoom = packToRoom(pack);

    noteByTerm.clear();
    for (const n of currentRoom.notes || []) {
      noteByTerm.set(n.term, n);
    }

    if (world) world.setRoom(currentRoom);

    if (logEl) {
      const resident = packLoader.residentEntries;
      const cap = packLoader.cap;
      logEl.textContent = `${pack.title} — ${currentRoom.notes.length} entradas · residentes: ${resident}/${cap}`;
    }
  } catch (e) {
    if (logEl) logEl.textContent = `erro: ${e.message}`;
    throw e;
  }
}

// ── boot ─────────────────────────────────────────────────────────────────────

async function boot() {
  const canvas = $('cv');
  if (!canvas) return;

  const params = new URLSearchParams(location.search);
  const packId = params.get('pack') || 'musica';

  world = new World(canvas, { onInteract: _interact });
  const theme = THEME_BY_ID['garden-forest'] || THEMES[0];
  world.setTheme(theme);

  const closeBtn = $('ins-close');
  if (closeBtn) closeBtn.addEventListener('click', () => { $('inspector').hidden = true; });

  await _loadPack(packId);
  world.start();
}

boot();
