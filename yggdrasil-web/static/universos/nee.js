/* nee.js — página do ÑE'Ẽ (YG-155): carrega pacotes de léxico (lazy) e os toca.
 *
 * UM consumidor do contrato engine-neutro `LexiconPack` (canvas/DOM 2D de hoje);
 * o áudio sai pela `AudioEngine` (Web Audio). Um cliente 3D futuro reusaria os
 * MESMOS módulos `pack-loader` + `audio-engine`, só trocando este render. */
import { AudioEngine } from './nee/audio-engine.js';
import { PackLoader } from './nee/pack-loader.js';

const engine = new AudioEngine();
const loader = new PackLoader();
// expõe p/ e2e/observabilidade (asserir nós de áudio sem depender de som real).
window.__nee = { engine, loader };

const logEl = document.getElementById('log');
function log(msg) {
  if (logEl) logEl.textContent = `${msg}\n${logEl.textContent}`.slice(0, 800);
}

function renderPack(pack, gridId, metaId) {
  const grid = document.getElementById(gridId);
  const meta = document.getElementById(metaId);
  if (!grid) return;
  grid.textContent = '';
  if (meta) {
    const bits = pack.entropy_stats ? ` · ${pack.entropy_stats.bits_per_symbol.toFixed(2)} bits/símbolo` : '';
    meta.textContent = `${pack.title} — ${pack.entries.length} entradas${bits}`;
  }
  for (const entry of pack.entries) {
    const btn = document.createElement('button');
    btn.className = `entry ${entry.role || ''}`;
    btn.dataset.term = entry.term;
    btn.dataset.role = entry.role || '';
    const term = document.createElement('span');
    term.className = 'term';
    term.textContent = entry.term;
    btn.appendChild(term);
    if (entry.gloss) {
      const g = document.createElement('span');
      g.className = 'gloss';
      g.textContent = entry.gloss;
      btn.appendChild(g);
    }
    btn.addEventListener('click', () => {
      const ok = engine.play(entry.audio);
      log(ok
        ? `▶ ${entry.term} — vozes: ${engine.stats.voicesStarted}, freqs: [${engine.stats.lastFreqs.slice(-6).join(', ')}]`
        : `· ${entry.term} — sem áudio nesta plataforma (fallback silencioso)`);
    });
    grid.appendChild(btn);
  }
}

async function boot() {
  try {
    const music = await loader.load('musica'); // lazy-load por pacote
    renderPack(music, 'music-grid', 'music-meta');
    const lang = await loader.load('guarani-mbya');
    renderPack(lang, 'language-grid', 'language-meta');
    log(`pacotes residentes: ${loader.residentEntries} entradas (teto ${loader.cap}).`);
  } catch (e) {
    log(`erro ao carregar pacotes: ${e.message}`);
  }
}

boot();
