/* nee.js — página do ÑE'Ẽ (YG-155 + YG-168): carrega pacotes de léxico, toca,
 * e implementa o loop de bits/score (Shannon economy).
 *
 * A/B de timing do quiz: variante atribuída aleatoriamente por usuário (localStorage).
 *   A = "imediato" — quiz dispara logo após a 1ª reprodução da entrada.
 *   B = "sob_demanda" — o botão "testar" aparece na entrada; quiz só quando clicado.
 */
import { AudioEngine } from './nee/audio-engine.js';
import { PackLoader } from './nee/pack-loader.js';

const engine = new AudioEngine();
const loader = new PackLoader();
window.__nee = { engine, loader };

// ── A/B de timing do quiz ────────────────────────────────────────────────────
const QUIZ_VARIANT_KEY = 'nee_quiz_variant';
function getQuizVariant() {
  let v = localStorage.getItem(QUIZ_VARIANT_KEY);
  if (!v) {
    v = Math.random() < 0.5 ? 'imediato' : 'sob_demanda';
    localStorage.setItem(QUIZ_VARIANT_KEY, v);
  }
  return v;
}
const QUIZ_VARIANT = getQuizVariant();

// ── Auth (JWT do localStorage — mesmo padrão das outras páginas) ─────────────
function getToken() {
  return localStorage.getItem('yggdrasil-jwt') || null;
}

const logEl = document.getElementById('log');
function log(msg) {
  if (logEl) logEl.textContent = `${msg}\n${logEl.textContent}`.slice(0, 800);
}

// ── HUD ──────────────────────────────────────────────────────────────────────
const hudEl = document.getElementById('bits-hud');
const hudBits = document.getElementById('hud-bits');
const hudDiscovered = document.getElementById('hud-discovered');

let currentScore = { total_bits: 0, descobertos: 0 };

async function fetchScore() {
  const token = getToken();
  if (!token) return;
  try {
    const r = await fetch('/api/v1/comunicacao/score', {
      headers: { Authorization: `Bearer ${token}` },
    });
    if (r.ok) {
      currentScore = await r.json();
      renderHud();
    }
  } catch (_) { /* silencioso */ }
}

function renderHud() {
  const token = getToken();
  if (!token) {
    hudEl.classList.add('guest');
    hudBits.textContent = '–';
    hudDiscovered.textContent = '';
    return;
  }
  hudEl.classList.remove('guest');
  hudEl.title = 'Bits Shannon acumulados ao descobrir e identificar símbolos';
  hudBits.textContent = Math.floor(currentScore.total_bits);
  hudDiscovered.textContent = `(${currentScore.descobertos} descobertos)`;
}

// ── API helpers ──────────────────────────────────────────────────────────────
async function apiPost(path, body) {
  const token = getToken();
  if (!token) return null;
  const r = await fetch(path, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify(body),
  });
  const data = await r.json().catch(() => ({}));
  return { status: r.status, data };
}

async function descobrir(pack, term) {
  const res = await apiPost('/api/v1/comunicacao/score/descobrir', { pack, term });
  if (!res) return;
  currentScore.total_bits = res.data.total_bits ?? currentScore.total_bits;
  if (res.data.creditado > 0) currentScore.descobertos += 1;
  renderHud();
  if (res.data.creditado > 0) log(`✦ Descoberta: +${res.data.creditado} bits (${term})`);
}

async function identificar(pack, term, answer) {
  const res = await apiPost('/api/v1/comunicacao/score/identificar', {
    pack,
    term,
    answer,
    quiz_variant: QUIZ_VARIANT,
  });
  if (!res) return null;
  currentScore.total_bits = res.data.total_bits ?? currentScore.total_bits;
  renderHud();
  return res.data;
}

async function revelar(pack, term, btnEl) {
  const res = await apiPost('/api/v1/comunicacao/score/revelar', { pack, term });
  if (!res) return;
  if (res.status === 402) {
    btnEl.classList.add('insufficient');
    setTimeout(() => btnEl.classList.remove('insufficient'), 600);
    log(`✗ Saldo insuficiente para revelar "${term}" (precisa ${res.data.custo ?? '?'} bits)`);
    return;
  }
  currentScore.total_bits = res.data.total_bits ?? currentScore.total_bits;
  renderHud();
  // Marca a glosa como revelada no botão pai
  const card = btnEl.closest('button.entry');
  if (card) {
    const glossEl = card.querySelector('.gloss');
    if (glossEl) {
      glossEl.classList.remove('hidden');
      glossEl.dataset.revealed = '1';
    }
    btnEl.remove();
  }
  log(`🔓 Revelado: "${term}" (−${res.data.custo} bits)`);
}

// ── Quiz modal ──────────────────────────────────────────────────────────────
const quizOverlay = document.getElementById('quiz-overlay');
const quizGloss = document.getElementById('quiz-gloss');
const quizOptions = document.getElementById('quiz-options');
const quizFeedback = document.getElementById('quiz-feedback');
const quizClose = document.getElementById('quiz-close');

quizClose.addEventListener('click', () => quizOverlay.classList.remove('open'));
quizOverlay.addEventListener('click', (e) => {
  if (e.target === quizOverlay) quizOverlay.classList.remove('open');
});

/** Escolhe N distratores aleatórios do pack para o quiz (excluindo o gabarito). */
function pickDistractors(pack, correctTerm, n) {
  const others = pack.entries
    .filter((e) => e.term !== correctTerm && e.gloss)
    .map((e) => e.term);
  // Fisher-Yates shuffle
  for (let i = others.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [others[i], others[j]] = [others[j], others[i]];
  }
  return others.slice(0, n);
}

/** Abre o quiz para um termo do pack. */
function openQuiz(pack, entry) {
  const distractors = pickDistractors(pack, entry.term, 3);
  const choices = [entry.term, ...distractors];
  // Shuffle choices
  for (let i = choices.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [choices[i], choices[j]] = [choices[j], choices[i]];
  }

  const gloss = entry.gloss || entry.term;
  quizGloss.textContent = `"${gloss}"`;
  quizFeedback.textContent = '';
  quizOptions.innerHTML = '';

  choices.forEach((choice) => {
    const btn = document.createElement('button');
    btn.className = 'quiz-option';
    btn.textContent = choice;
    btn.dataset.choice = choice;
    btn.addEventListener('click', async () => {
      // Desabilita todas as opções
      quizOptions.querySelectorAll('.quiz-option').forEach((b) => (b.disabled = true));
      const result = await identificar(pack.id, entry.term, choice);
      if (!result) {
        quizFeedback.textContent = 'Faça login para pontuar.';
        return;
      }
      if (result.correto) {
        btn.classList.add('correct');
        quizFeedback.textContent =
          result.creditado > 0
            ? `✓ Correto! +${result.creditado} bits`
            : `✓ Correto! (bônus esgotado)`;
        log(`✓ Quiz: "${entry.term}" → correto (+${result.creditado} bits)`);
      } else {
        btn.classList.add('wrong');
        // Destaca a resposta certa
        quizOptions.querySelectorAll('.quiz-option').forEach((b) => {
          if (b.dataset.choice === entry.term) b.classList.add('correct');
        });
        quizFeedback.textContent = `✗ Errado. Era "${entry.term}".`;
        log(`✗ Quiz: "${entry.term}" → errado`);
      }
    });
    quizOptions.appendChild(btn);
  });

  quizOverlay.classList.add('open');
}

// ── Render ───────────────────────────────────────────────────────────────────
function renderPack(pack, gridId, metaId) {
  const grid = document.getElementById(gridId);
  const meta = document.getElementById(metaId);
  if (!grid) return;
  grid.textContent = '';

  const bps = pack.entropy_stats
    ? pack.entropy_stats.bits_per_symbol
    : Math.log2(Math.max(pack.entries.length, 2));
  const cost = Math.ceil(bps);

  if (meta) {
    const bpsTxt = pack.entropy_stats ? ` · ${bps.toFixed(2)} bits/símbolo` : '';
    meta.textContent = `${pack.title} — ${pack.entries.length} entradas${bpsTxt}`;
  }

  for (const entry of pack.entries) {
    const btn = document.createElement('button');
    btn.className = `entry ${entry.role || ''}`;
    btn.dataset.term = entry.term;
    btn.dataset.role = entry.role || '';
    btn.dataset.pack = pack.id;

    const term = document.createElement('span');
    term.className = 'term';
    term.textContent = entry.term;
    btn.appendChild(term);

    // Glosa — oculta por padrão (revelada via bits ou descoberta)
    const glossEl = document.createElement('span');
    glossEl.className = 'gloss hidden';
    glossEl.textContent = entry.gloss || '';
    btn.appendChild(glossEl);

    // Botão "revelar (−N bits)"
    const revealBtn = document.createElement('button');
    revealBtn.className = 'btn-reveal';
    revealBtn.textContent = `revelar (−${cost} bits)`;
    revealBtn.dataset.revealTerm = entry.term;
    revealBtn.dataset.revealPack = pack.id;
    revealBtn.addEventListener('click', async (e) => {
      e.stopPropagation();
      await revelar(pack.id, entry.term, revealBtn);
    });
    btn.appendChild(revealBtn);

    // Click principal: toca o áudio + descoberta + quiz (se variante imediata)
    let played = false;
    btn.addEventListener('click', async (e) => {
      if (e.target === revealBtn || revealBtn.contains(e.target)) return;
      const ok = engine.play(entry.audio);
      log(
        ok
          ? `▶ ${entry.term} — vozes: ${engine.stats.voicesStarted}, freqs: [${engine.stats.lastFreqs.slice(-6).join(', ')}]`
          : `· ${entry.term} — sem áudio nesta plataforma (fallback silencioso)`
      );
      // Registra descoberta (idempotente no servidor)
      await descobrir(pack.id, entry.term);
      // Quiz: A=imediato (só na 1ª vez), B=sob demanda (botão separado)
      if (QUIZ_VARIANT === 'imediato' && !played && entry.gloss) {
        played = true;
        // Pequeno delay p/ o usuário ouvir antes de ver o quiz
        setTimeout(() => openQuiz(pack, entry), 600);
      }
    });

    // Botão "testar" para variante B (sob demanda) — só aparece se houver glosa
    if (QUIZ_VARIANT === 'sob_demanda' && entry.gloss) {
      const testBtn = document.createElement('button');
      testBtn.className = 'btn-reveal';
      testBtn.textContent = '📝 testar';
      testBtn.style.borderStyle = 'solid';
      testBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        openQuiz(pack, entry);
      });
      btn.appendChild(testBtn);
    }

    grid.appendChild(btn);
  }
}

// ── Boot ─────────────────────────────────────────────────────────────────────
async function boot() {
  await fetchScore();
  try {
    const music = await loader.load('musica');
    renderPack(music, 'music-grid', 'music-meta');
    const lang = await loader.load('guarani-mbya');
    renderPack(lang, 'language-grid', 'language-meta');
    log(`pacotes residentes: ${loader.residentEntries} entradas (teto ${loader.cap}).`);
    log(`quiz variant: ${QUIZ_VARIANT}`);
  } catch (e) {
    log(`erro ao carregar pacotes: ${e.message}`);
  }
}

boot();
