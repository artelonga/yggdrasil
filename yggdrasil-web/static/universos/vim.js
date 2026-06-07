'use strict';

// ── Constantes de renderização ───────────────────────────────────────────────

const FONT_SIZE  = 15;       // px
const LINE_HEIGHT = 20;      // px por linha
const CHAR_WIDTH  = 9;       // px por caractere (monospace)
const COLS        = 60;      // colunas visíveis
const PADDING_LEFT  = 8;     // px
const PADDING_TOP   = 6;     // px

const COLOR = {
  bg:           '#0a0a10',
  text:         '#e8e3d3',
  cursorNormal: 'rgba(212,175,55,0.7)',    // bloco dourado em NORMAL
  cursorInsert: 'rgba(96,165,250,0.5)',    // barra azul em INSERT
  cursorVisual: 'rgba(192,132,252,0.4)',   // bloco roxo em VISUAL
  lineHighlight:'rgba(255,255,255,0.04)',
  selection:    'rgba(192,132,252,0.2)',
  lineNum:      '#3a3a5a',
};

// ── Estado da aplicação ──────────────────────────────────────────────────────

const app = {
  sessionId: null,
  state: null,
  canvas: null,
  ctx: null,
  rows: 12,          // linhas visíveis no canvas (ajusta conforme nível)
  pendingKey: null,  // teclas aguardando composição (ex: 'g' → 'gg', 'd' → 'dw')
  lastGPress: false, // para detectar 'gg'
  gTimer: null,      // timeout do 'gg'
  queue: [],         // fila serial de teclas (nunca descarta — corrige tecla perdida)
  processing: false, // há um POST em andamento?
};

// ── Funções de UI ────────────────────────────────────────────────────────────

function setStatus(msg) {
  const el = document.getElementById('rodape');
  if (el) el.textContent = msg;
}

function updateHeader(state) {
  const nivel = document.getElementById('nivel');
  const score = document.getElementById('score');
  const titulo = document.getElementById('nivel-titulo');
  if (nivel) nivel.textContent = state.level;
  if (score) score.textContent = state.score;
  if (titulo) titulo.textContent = '';
}

function updateMode(mode) {
  const el = document.getElementById('mode-label');
  if (!el) return;
  el.textContent = mode;
  el.className = 'status-mode mode-' + mode;
}

function updateStatusText(state) {
  const el = document.getElementById('status-text');
  if (el) el.textContent = state.status_line || '';

  const pos = document.getElementById('status-pos');
  if (pos) pos.textContent = `${state.cursor[0]+1},${state.cursor[1]+1}`;
}

function updateHint(hint) {
  const box = document.getElementById('hint-box');
  const txt = document.getElementById('hint-text');
  if (!hint) {
    if (box) box.style.display = 'none';
    return;
  }
  if (box) box.style.display = '';
  if (txt) txt.textContent = hint;
}

function showNivelCompleto(level) {
  const div = document.getElementById('nivel-completo');
  const msg = document.getElementById('nc-msg');
  if (!div) return;
  div.classList.add('ativo');
  if (msg) {
    if (level >= 10) {
      msg.textContent = 'Você completou todos os níveis! Parabéns!';
    } else {
      msg.textContent = `Avançando para o nível ${level + 1}…`;
    }
  }
  setTimeout(() => div.classList.remove('ativo'), 2200);
}

// ── Renderização do canvas ───────────────────────────────────────────────────

function resizeCanvas(lineCount) {
  const rows = Math.max(lineCount, 4);
  const w = PADDING_LEFT * 2 + CHAR_WIDTH * COLS;
  const h = PADDING_TOP * 2 + LINE_HEIGHT * rows;
  app.canvas.width  = w;
  app.canvas.height = h;
  app.rows = rows;
}

function draw(state) {
  if (!app.ctx || !state) return;

  const { ctx } = app;
  const lines   = state.lines;
  const cursor  = state.cursor; // [row, col]
  const mode    = state.mode;
  const sel     = state.selection; // null ou [[r0,c0],[r1,c1]]

  resizeCanvas(lines.length);

  const W = app.canvas.width;
  const H = app.canvas.height;

  // Fundo
  ctx.fillStyle = COLOR.bg;
  ctx.fillRect(0, 0, W, H);

  ctx.font = `${FONT_SIZE}px 'Courier New', Courier, monospace`;

  for (let row = 0; row < lines.length; row++) {
    const line = lines[row];
    const y    = PADDING_TOP + row * LINE_HEIGHT;

    // Destaque da linha atual
    if (row === cursor[0]) {
      ctx.fillStyle = COLOR.lineHighlight;
      ctx.fillRect(0, y, W, LINE_HEIGHT);
    }

    // Destaque de seleção visual
    if (sel) {
      const [s, e] = sel;
      if (row >= s[0] && row <= e[0]) {
        const startCol = row === s[0] ? s[1] : 0;
        const endCol   = row === e[0] ? e[1] + 1 : line.length;
        const sx = PADDING_LEFT + startCol * CHAR_WIDTH;
        const ex = PADDING_LEFT + endCol   * CHAR_WIDTH;
        ctx.fillStyle = COLOR.selection;
        ctx.fillRect(sx, y, ex - sx, LINE_HEIGHT);
      }
    }

    // Texto da linha
    ctx.fillStyle = COLOR.text;
    ctx.fillText(line, PADDING_LEFT, y + FONT_SIZE);
  }

  // Cursor
  const cy = PADDING_TOP + cursor[0] * LINE_HEIGHT;
  const cx = PADDING_LEFT + cursor[1] * CHAR_WIDTH;

  if (mode === 'INSERT') {
    // Cursor barra vertical
    ctx.fillStyle = COLOR.cursorInsert;
    ctx.fillRect(cx, cy + 2, 2, LINE_HEIGHT - 4);
  } else if (mode === 'VISUAL') {
    ctx.fillStyle = COLOR.cursorVisual;
    ctx.fillRect(cx, cy, CHAR_WIDTH, LINE_HEIGHT);
  } else {
    // NORMAL / COMMAND / SEARCH — bloco
    ctx.fillStyle = COLOR.cursorNormal;
    ctx.fillRect(cx, cy, CHAR_WIDTH, LINE_HEIGHT);
    // Inverte o caractere sob o cursor
    const ch = lines[cursor[0]]?.[cursor[1]];
    if (ch) {
      ctx.fillStyle = COLOR.bg;
      ctx.fillText(ch, cx, cy + FONT_SIZE);
    }
  }
}

// ── Comunicação com o servidor ───────────────────────────────────────────────

async function createSession() {
  const res = await fetch('/api/v1/universos/vim/sessoes', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: '{}',
  });
  if (!res.ok) throw new Error(`Erro ao criar sessão: ${res.status}`);
  return res.json();
}

async function postKey(key) {
  const res = await fetch(`/api/v1/universos/vim/sessoes/${app.sessionId}/tecla`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ key }),
  });
  if (!res.ok) {
    console.warn('send_key error', res.status);
    return null;
  }
  return res.json();
}

// Fila serial: cada tecla é enviada na ordem, sem descartar nada enquanto um
// POST está em andamento (corrige "a segunda tecla some" / $ e G ignorados).
function enqueueKey(key) {
  app.queue.push(key);
  if (!app.processing) processQueue();
}

async function processQueue() {
  app.processing = true;
  try {
    while (app.queue.length) {
      const key = app.queue.shift();
      try {
        const data = await postKey(key);
        if (data) applyState(data.state);
      } catch (err) {
        console.error('vim key error', err);
        setStatus('Erro de comunicação com o servidor.');
      }
    }
  } finally {
    app.processing = false;
  }
}

function applyState(state) {
  app.state = state;
  draw(state);
  updateMode(state.mode);
  updateStatusText(state);
  updateHeader(state);
  updateHint(state.hint);

  if (state.completed_level != null) {
    showNivelCompleto(state.completed_level);
  }

  if (state.game_completed) {
    setStatus('Jogo concluído! Pontuação final: ' + state.score + ' sementes. Voltando ao lobby…');
    setTimeout(() => window.location.assign('/lobby'), 3000);
  }
}

// ── Mapeamento de teclas do browser para strings Vim ────────────────────────

/**
 * Converte um evento KeyboardEvent para a string de tecla Vim.
 * Retorna null se a tecla não deve ser enviada.
 */
function mapKey(e) {
  switch (e.key) {
    case 'Escape':    return 'Esc';
    case 'Enter':     return 'Enter';
    case 'Backspace': return 'Backspace';
    case 'Tab':       return null; // ignorar
    // Setas → hjkl
    case 'ArrowLeft':  return 'h';
    case 'ArrowDown':  return 'j';
    case 'ArrowUp':    return 'k';
    case 'ArrowRight': return 'l';
    default:
      // Caracteres imprimíveis (len=1)
      if (e.key.length === 1) return e.key;
      return null;
  }
}

// ── Tratamento de teclas com composição ─────────────────────────────────────

function handleVimKey(key) {
  // Detecta 'gg': dois 'g' consecutivos em NORMAL → enfileira "gg"
  if (key === 'g' && app.state && app.state.mode === 'NORMAL') {
    if (app.lastGPress) {
      app.lastGPress = false;
      clearTimeout(app.gTimer);
      enqueueKey('gg');
      return;
    }
    app.lastGPress = true;
    clearTimeout(app.gTimer);
    app.gTimer = setTimeout(() => { app.lastGPress = false; }, 800);
    return;
  }
  app.lastGPress = false;
  enqueueKey(key);
}

// ── Event listener de teclado ────────────────────────────────────────────────

function handleKeydown(e) {
  // Não intercepta atalhos do browser (Ctrl+R, Ctrl+C, F5…)
  if (e.ctrlKey || e.metaKey || e.altKey) return;

  // Bloqueia scroll via setas em modo de jogo
  if (['ArrowUp','ArrowDown','ArrowLeft','ArrowRight',' '].includes(e.key)) {
    e.preventDefault();
  }

  const key = mapKey(e);
  if (!key) return;

  e.preventDefault();
  handleVimKey(key);
}

// ── Inicialização ────────────────────────────────────────────────────────────

async function initVim() {
  app.canvas = document.getElementById('canvas');
  app.ctx    = app.canvas.getContext('2d');

  const data = await createSession();
  app.sessionId = data.session_id;
  applyState(data.state);

  window.addEventListener('keydown', handleKeydown);
  app.canvas.focus();
  setStatus('');
}

initVim().catch(err => {
  setStatus('Erro ao iniciar o universo Vim.');
  console.error(err);
});
