'use strict';

// Same tile size and color palette as lobby.js
const TILE = 16;

const COLOR = {
  empty:    '#0d0d12',
  wall:     '#1a1a2e',
  head:     '#e0505f',  // snake head — red
  body:     '#34d399',  // snake body — green
  food:     '#d4af37',  // food — gold (portal color from lobby)
};

const state = {
  canvas: null,
  ctx:    null,
  gameId: null,
  direction: 'Right',
  tiles:  null,
  width:  0,
  height: 0,
  score:  0,
  over:   false,
  atv:    null,  // YG-145: presença/atividade do universo
};

function setStatus(msg) {
  const el = document.getElementById('rodape');
  if (el) el.textContent = msg;
}

function setScore(s) {
  const el = document.getElementById('score');
  if (el) el.textContent = s;
}

function draw() {
  const { ctx, width, height, tiles } = state;
  if (!ctx || !tiles) return;

  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const tile = tiles[y][x];
      const px = x * TILE;
      const py = y * TILE;

      if (tile === 'Wall') {
        ctx.fillStyle = COLOR.wall;
        ctx.fillRect(px, py, TILE, TILE);
      } else if (tile !== null && typeof tile === 'object' && tile.Entity) {
        const et = tile.Entity.entity_type;
        if (et === 'Player') {
          ctx.fillStyle = COLOR.head;
          ctx.fillRect(px + 1, py + 1, TILE - 2, TILE - 2);
        } else if (et === 'Obstacle') {
          ctx.fillStyle = COLOR.body;
          ctx.fillRect(px + 1, py + 1, TILE - 2, TILE - 2);
        } else if (et === 'Item') {
          // Food as filled circle
          ctx.fillStyle = COLOR.food;
          ctx.beginPath();
          ctx.arc(px + TILE / 2, py + TILE / 2, TILE / 2 - 2, 0, Math.PI * 2);
          ctx.fill();
        } else {
          ctx.fillStyle = COLOR.empty;
          ctx.fillRect(px, py, TILE, TILE);
        }
      } else {
        ctx.fillStyle = COLOR.empty;
        ctx.fillRect(px, py, TILE, TILE);
      }
    }
  }
}

function drawGameOver() {
  const { ctx, width, height, score } = state;
  const W = width * TILE;
  const H = height * TILE;
  ctx.fillStyle = 'rgba(0,0,0,0.72)';
  ctx.fillRect(0, 0, W, H);
  ctx.textAlign = 'center';
  ctx.fillStyle = '#d4af37';
  ctx.font = 'bold 20px monospace';
  ctx.fillText('FIM DE JOGO', W / 2, H / 2 - 20);
  ctx.font = '14px monospace';
  ctx.fillStyle = '#e8e3d3';
  ctx.fillText(`Pontuação: ${score}`, W / 2, H / 2 + 10);
  ctx.fillText('Voltando ao lobby...', W / 2, H / 2 + 35);
}

async function tick() {
  if (state.over || !state.gameId) return;

  try {
    const res = await fetch(`/api/v1/games/snake/${state.gameId}/input`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ direction: state.direction }),
    });
    if (!res.ok) return;
    const data = await res.json();

    state.tiles = data.state.tiles;
    state.score = data.score;
    setScore(data.score);
    draw();

    if (data.action === 'quit') {
      state.over = true;
      if (state.atv) state.atv.partida({ score: state.score, resultado: 'fim' });
      drawGameOver();
      setStatus('Redirecionando para o lobby...');
      // GameAction::Quit received — redirect to lobby
      setTimeout(() => window.location.assign('/lobby'), 2000);
    }
  } catch (e) {
    console.error('snake tick error', e);
  }
}

function handleKey(e) {
  if (state.over) return;
  switch (e.key) {
    case 'ArrowUp':    case 'w': case 'W': state.direction = 'Up';    e.preventDefault(); break;
    case 'ArrowDown':  case 's': case 'S': state.direction = 'Down';  e.preventDefault(); break;
    case 'ArrowLeft':  case 'a': case 'A': state.direction = 'Left';  e.preventDefault(); break;
    case 'ArrowRight': case 'd': case 'D': state.direction = 'Right'; e.preventDefault(); break;
    case 'Escape': case 'q': case 'Q':
      state.direction = 'Quit';
      break;
    default:
      return;
  }
}

async function initSnake() {
  const res = await fetch('/api/v1/games/snake/start');
  if (!res.ok) throw new Error(`start failed: ${res.status}`);
  const data = await res.json();

  state.gameId    = data.id;
  state.tiles     = data.state.tiles;
  state.width     = data.state.width;
  state.height    = data.state.height;

  const canvas = document.getElementById('canvas');
  canvas.width  = state.width  * TILE;
  canvas.height = state.height * TILE;
  state.canvas = canvas;
  state.ctx    = canvas.getContext('2d');

  draw();
  if (window.yggTelemetria) state.atv = window.yggTelemetria.atividade('snake');
  window.addEventListener('keydown', handleKey);
  setInterval(tick, 120);
  setStatus('');
}

initSnake().catch(err => {
  setStatus('Erro ao iniciar jogo');
  console.error(err);
});
