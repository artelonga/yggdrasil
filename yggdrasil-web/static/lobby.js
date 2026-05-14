'use strict';

const TILE = 16;

const COLOR = {
  empty: '#0d0d12',
  wall: '#1a1a2e',
  portal: '#d4af37',
  player: '#d4af37',
};

const SYMBOL = {
  snake: '🐍',
  tetris: '🟦',
  invaders: '👾',
  poker: '🃏',
};

// Mirrors game-core Direction { Up, Down, Left, Right }
const Direction = { Up: 'Up', Down: 'Down', Left: 'Left', Right: 'Right' };

const state = {
  canvas: null,
  playerX: 0,
  playerY: 0,
  tiles: null,
  width: 0,
  height: 0,
  ctx: null,
  animating: false,
};

function isWall(x, y) {
  if (x < 0 || y < 0 || x >= state.width || y >= state.height) return true;
  return state.tiles[y][x] === 'Wall';
}

function setStatus(msg) {
  const el = document.getElementById('rodape');
  if (el) el.textContent = msg;
}

function draw() {
  const { ctx, width, height, tiles, playerX, playerY } = state;

  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const tile = tiles[y][x];
      const px = x * TILE;
      const py = y * TILE;

      if (tile === 'Wall') {
        ctx.fillStyle = COLOR.wall;
        ctx.fillRect(px, py, TILE, TILE);
      } else if (tile !== null && typeof tile === 'object' && tile.Portal) {
        ctx.fillStyle = COLOR.portal;
        ctx.fillRect(px, py, TILE, TILE);
        const sym = SYMBOL[tile.Portal] ?? tile.Portal[0].toUpperCase();
        ctx.font = '10px sans-serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillStyle = '#0d0d12';
        ctx.fillText(sym, px + TILE / 2, py + TILE / 2);
      } else {
        ctx.fillStyle = COLOR.empty;
        ctx.fillRect(px, py, TILE, TILE);
      }
    }
  }

  // Avatar '@' dourado sobre o tile atual
  const px = playerX * TILE;
  const py = playerY * TILE;
  ctx.fillStyle = 'rgba(212, 175, 55, 0.15)';
  ctx.fillRect(px, py, TILE, TILE);
  ctx.font = 'bold 12px monospace';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillStyle = COLOR.player;
  ctx.fillText('@', px + TILE / 2, py + TILE / 2);
}

async function enterPortal() {
  const tile = state.tiles[state.playerY][state.playerX];
  if (!tile || typeof tile !== 'object' || !tile.Portal) return;

  const res = await fetch('/api/v1/lobby/enter', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ x: state.playerX, y: state.playerY }),
  });
  if (!res.ok) return;
  const { slug } = await res.json();
  window.location.assign('/universos/' + slug);
}

function bfs(startX, startY, goalX, goalY) {
  if (startX === goalX && startY === goalY) return [];

  const queue = [[startX, startY]];
  const prev = new Map();
  prev.set(`${startX},${startY}`, null);
  let visited = 0;

  const limit = state.width * state.height;
  while (queue.length > 0) {
    if (visited >= limit) return null;
    const [x, y] = queue.shift();
    visited++;

    for (const [dx, dy] of [[0, -1], [0, 1], [-1, 0], [1, 0]]) {
      const nx = x + dx;
      const ny = y + dy;
      const key = `${nx},${ny}`;
      if (!prev.has(key) && !isWall(nx, ny)) {
        prev.set(key, [x, y]);
        if (nx === goalX && ny === goalY) {
          const path = [];
          let cx = nx, cy = ny;
          while (cx !== startX || cy !== startY) {
            path.unshift([cx, cy]);
            [cx, cy] = prev.get(`${cx},${cy}`);
          }
          return path;
        }
        queue.push([nx, ny]);
      }
    }
  }
  return null;
}

function animatePath(path, onComplete) {
  state.animating = true;
  let i = 0;

  function step() {
    if (i >= path.length) {
      state.animating = false;
      onComplete();
      return;
    }
    [state.playerX, state.playerY] = path[i];
    draw();
    i++;
    setTimeout(step, 50);
  }

  step();
}

function handleClick(e) {
  if (state.animating) return;

  const rect = state.canvas.getBoundingClientRect();
  const scaleX = state.canvas.width / rect.width;
  const scaleY = state.canvas.height / rect.height;
  const tileX = Math.floor((e.clientX - rect.left) * scaleX / TILE);
  const tileY = Math.floor((e.clientY - rect.top) * scaleY / TILE);

  if (tileX < 0 || tileX >= state.width || tileY < 0 || tileY >= state.height) return;

  if (isWall(tileX, tileY)) {
    setStatus('Sem caminho');
    return;
  }

  const path = bfs(state.playerX, state.playerY, tileX, tileY);
  if (path === null) {
    setStatus('Sem caminho');
    return;
  }

  setStatus('');

  const destTile = state.tiles[tileY][tileX];
  const isPortal = destTile && typeof destTile === 'object' && destTile.Portal;
  if (isPortal) {
    state.canvas.setAttribute('aria-label', `movendo para portal ${destTile.Portal}`);
  }

  if (path.length === 0) {
    if (isPortal) enterPortal();
    return;
  }

  animatePath(path, () => {
    if (isPortal) enterPortal();
  });
}

function handleKey(e) {
  if (state.animating) return;
  let dx = 0;
  let dy = 0;

  switch (e.key) {
    case 'ArrowUp':    case 'w': case 'W': dy = -1; break;
    case 'ArrowDown':  case 's': case 'S': dy =  1; break;
    case 'ArrowLeft':  case 'a': case 'A': dx = -1; break;
    case 'ArrowRight': case 'd': case 'D': dx =  1; break;
    case 'Enter':
      enterPortal();
      return;
    default:
      return;
  }

  e.preventDefault();
  const nx = state.playerX + dx;
  const ny = state.playerY + dy;
  if (!isWall(nx, ny)) {
    state.playerX = nx;
    state.playerY = ny;
    draw();
  }
}

async function initLobby() {
  const res = await fetch('/api/v1/lobby');
  if (!res.ok) throw new Error(`lobby API ${res.status}`);
  const universe = await res.json();
  const { width, height, tiles } = universe.map;

  const canvas = document.getElementById('canvas');
  canvas.width = width * TILE;
  canvas.height = height * TILE;
  state.canvas = canvas;
  state.ctx = canvas.getContext('2d');
  state.tiles = tiles;
  state.width = width;
  state.height = height;
  state.playerX = Math.floor(width / 2);
  state.playerY = Math.floor(height / 2);

  draw();
  window.addEventListener('keydown', handleKey);
  canvas.addEventListener('click', handleClick);
}

// ── Auth area (top-right) ────────────────────────────────────────────────────

const JWT_KEY = 'yggdrasil-jwt';

function decodeJwt(token) {
  try {
    const payload = token.split('.')[1];
    const json = atob(payload.replace(/-/g, '+').replace(/_/g, '/'));
    return JSON.parse(json);
  } catch { return null; }
}

function renderAuthArea() {
  const area = document.getElementById('auth-area');
  const token = localStorage.getItem(JWT_KEY);
  if (!token) {
    area.innerHTML = `<a href="/login?next=/lobby">Entrar</a>`;
    return;
  }
  const claims = decodeJwt(token);
  const email = claims && claims.email ? claims.email : 'logado';
  area.innerHTML = `
    <span class="auth-user">${email}</span>
    <button id="btn-logout">Sair</button>
  `;
  document.getElementById('btn-logout').addEventListener('click', () => {
    localStorage.removeItem(JWT_KEY);
    location.reload();
  });
}

// ── Scores + activity sidebar ────────────────────────────────────────────────

const GAME_LABEL = {
  snake: '🐍 Snake',
  tetris: '🟦 Tetris',
  invaders: '👾 Invaders',
  poker: '🃏 Poker',
};

async function loadScores() {
  try {
    const res = await fetch('/api/v1/scores/top?limit=3');
    if (!res.ok) return;
    const { scores } = await res.json();
    renderScores(scores);
  } catch (_) { /* silent */ }
}

function renderScores(scores) {
  const root = document.getElementById('scores-content');
  if (!scores.length) {
    root.innerHTML = '<div class="empty">Ainda não há pontuações registradas. Jogue para entrar no quadro.</div>';
    return;
  }
  // Group by game
  const byGame = {};
  for (const s of scores) {
    if (!byGame[s.game]) byGame[s.game] = [];
    byGame[s.game].push(s);
  }
  const parts = [];
  for (const game of ['snake', 'tetris', 'invaders']) {
    if (!byGame[game]) continue;
    parts.push(`<div class="group-label">${GAME_LABEL[game] || game}</div>`);
    for (const s of byGame[game]) {
      parts.push(`
        <div class="score-row">
          <span class="user">${s.username || s.user_id}</span>
          <span class="score">${s.score}</span>
        </div>
      `);
    }
  }
  root.classList.remove('empty');
  root.innerHTML = parts.join('') || '<div class="empty">Sem dados.</div>';
}

async function loadActivity() {
  try {
    const res = await fetch('/api/v1/scores/recent');
    if (!res.ok) return;
    const { scores } = await res.json();
    renderActivity(scores);
  } catch (_) { /* silent */ }
}

function shortDate(iso) {
  try {
    const d = new Date(iso);
    const now = new Date();
    const diff = (now - d) / 1000;
    if (diff < 60) return 'agora';
    if (diff < 3600) return `${Math.floor(diff / 60)}min`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h`;
    return d.toLocaleDateString('pt-BR');
  } catch { return ''; }
}

function renderActivity(scores) {
  const root = document.getElementById('activity-content');
  if (!scores.length) {
    root.innerHTML = '<div class="empty">Nenhuma atividade ainda.</div>';
    return;
  }
  root.classList.remove('empty');
  root.innerHTML = scores.map((s) => `
    <div class="score-row">
      <span class="game">${GAME_LABEL[s.game] || s.game}</span>
      <span class="user">${s.username || s.user_id}</span>
      <span class="score">${s.score}</span>
      <span class="ts">${shortDate(s.ts)}</span>
    </div>
  `).join('');
}

renderAuthArea();
loadScores();
loadActivity();
initLobby().catch(console.error);
