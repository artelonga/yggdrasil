'use strict';

const BLOCK = 28;
const COLS = 10;
const ROWS = 20;

// Piece colors indexed by type (1-7); index 0 unused
const COLORS = [
  null,
  '#e0505f',  // 1 I — red
  '#d4af37',  // 2 O — gold
  '#60a5fa',  // 3 T — blue
  '#34d399',  // 4 S — green
  '#f97316',  // 5 Z — orange
  '#a78bfa',  // 6 J — violet
  '#fb7185',  // 7 L — pink
];

const state = {
  gameId:    null,
  board:     null,
  piece:     null,
  score:     0,
  level:     1,
  over:      false,
  canvas:    null,
  ctx:       null,
  dropTimer: null,
  busy:      false,
  atv:       null,  // YG-145: presença/atividade do universo
};

function setStatus(msg) {
  const el = document.getElementById('rodape');
  if (el) el.textContent = msg;
}

function setScore(s) {
  const el = document.getElementById('score');
  if (el) el.textContent = s;
}

function setLevel(l) {
  const el = document.getElementById('level');
  if (el) el.textContent = l;
}

function drawBlock(x, y, color) {
  const { ctx } = state;
  ctx.fillStyle = color;
  ctx.fillRect(x * BLOCK + 1, y * BLOCK + 1, BLOCK - 2, BLOCK - 2);
  ctx.fillStyle = 'rgba(255,255,255,0.15)';
  ctx.fillRect(x * BLOCK + 1, y * BLOCK + 1, BLOCK - 2, 4);
}

function draw() {
  const { ctx, board, piece } = state;
  if (!ctx || !board) return;

  ctx.fillStyle = '#0d0d12';
  ctx.fillRect(0, 0, COLS * BLOCK, ROWS * BLOCK);

  // Draw grid lines
  ctx.strokeStyle = 'rgba(255,255,255,0.04)';
  ctx.lineWidth = 1;
  for (let c = 0; c <= COLS; c++) {
    ctx.beginPath();
    ctx.moveTo(c * BLOCK, 0);
    ctx.lineTo(c * BLOCK, ROWS * BLOCK);
    ctx.stroke();
  }
  for (let r = 0; r <= ROWS; r++) {
    ctx.beginPath();
    ctx.moveTo(0, r * BLOCK);
    ctx.lineTo(COLS * BLOCK, r * BLOCK);
    ctx.stroke();
  }

  // Draw locked board cells
  for (let r = 0; r < ROWS; r++) {
    for (let c = 0; c < COLS; c++) {
      const cell = board[r][c];
      if (cell) {
        drawBlock(c, r, COLORS[cell]);
      }
    }
  }

  // Draw active piece
  if (piece) {
    const { x, y, shape, piece_type } = piece;
    for (let r = 0; r < shape.length; r++) {
      for (let c = 0; c < shape[r].length; c++) {
        if (shape[r][c]) {
          const px = x + c;
          const py = y + r;
          if (py >= 0) {
            drawBlock(px, py, COLORS[piece_type]);
          }
        }
      }
    }
  }

  if (state.over) {
    const W = COLS * BLOCK;
    const H = ROWS * BLOCK;
    ctx.fillStyle = 'rgba(0,0,0,0.72)';
    ctx.fillRect(0, 0, W, H);
    ctx.textAlign = 'center';
    ctx.fillStyle = '#d4af37';
    ctx.font = 'bold 20px monospace';
    ctx.fillText('FIM DE JOGO', W / 2, H / 2 - 20);
    ctx.font = '14px monospace';
    ctx.fillStyle = '#e8e3d3';
    ctx.fillText(`Pontuação: ${state.score}`, W / 2, H / 2 + 10);
    ctx.fillText('Voltando ao lobby...', W / 2, H / 2 + 35);
  }
}

function dropSpeed() {
  return Math.max(100, 800 - (state.level - 1) * 70);
}

function startDropTimer() {
  clearInterval(state.dropTimer);
  state.dropTimer = setInterval(() => sendInput('Drop'), dropSpeed());
}

async function sendInput(direction) {
  if (state.over || state.busy || !state.gameId) return;
  state.busy = true;

  try {
    const res = await fetch(`/api/v1/games/tetris/${state.gameId}/input`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ direction }),
    });
    if (!res.ok) return;
    const data = await res.json();

    const prevLevel = state.level;
    state.board  = data.state.board;
    state.piece  = data.state.piece;
    state.score  = data.score;
    state.level  = data.state.level;

    setScore(data.score);
    setLevel(data.state.level);
    draw();

    if (state.level !== prevLevel) {
      startDropTimer();
    }

    if (data.action === 'quit') {
      state.over = true;
      if (state.atv) state.atv.partida({ score: state.score, resultado: 'fim' });
      clearInterval(state.dropTimer);
      draw();
      setStatus('Redirecionando para o lobby...');
      setTimeout(() => window.location.assign('/lobby'), 2000);
    }
  } catch (e) {
    console.error('tetris input error', e);
  } finally {
    state.busy = false;
  }
}

function handleKey(e) {
  if (state.over) return;

  switch (e.key) {
    case 'ArrowLeft':
      e.preventDefault();
      sendInput('Left');
      break;
    case 'ArrowRight':
      e.preventDefault();
      sendInput('Right');
      break;
    case 'ArrowDown':
      e.preventDefault();
      sendInput('Down');
      break;
    case 'ArrowUp':
      e.preventDefault();
      sendInput('Rotate');
      break;
    case ' ':
      e.preventDefault();
      sendInput('HardDrop');
      break;
    case 'Escape':
    case 'q':
    case 'Q':
      sendInput('Quit');
      break;
    default:
      return;
  }
}

async function initTetris() {
  const res = await fetch('/api/v1/games/tetris/start');
  if (!res.ok) throw new Error(`start failed: ${res.status}`);
  const data = await res.json();

  state.gameId = data.id;
  state.board  = data.state.board;
  state.piece  = data.state.piece;
  state.score  = data.score;
  state.level  = data.state.level;

  const canvas = document.getElementById('canvas');
  canvas.width  = COLS * BLOCK;
  canvas.height = ROWS * BLOCK;
  state.canvas = canvas;
  state.ctx    = canvas.getContext('2d');

  draw();
  if (window.yggTelemetria) state.atv = window.yggTelemetria.atividade('tetris');
  window.addEventListener('keydown', handleKey);
  startDropTimer();
  setStatus('');
}

initTetris().catch(err => {
  setStatus('Erro ao iniciar jogo');
  console.error(err);
});
