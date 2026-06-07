'use strict';

const TILE = 22;

const COLOR = {
  empty: '#0d0d12',
  wall: '#1a1a2e',
  portal: '#d4af37',
  player: '#d4af37',
};

// Ícones SVG (não emojis). Markup interno usa `currentColor` para recolorir:
// inline nos chips (herda a cor do texto) e como imagem dourada/escura no canvas.
const ICONS = {
  snake: '<path fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" d="M4 16c2.6 0 2.6-3.2 5.2-3.2S11.8 16 14.4 16s3.1-1.6 3.1-4.1a4.1 4.1 0 0 0-8.2 0"/><circle cx="4" cy="16" r="1.2" fill="currentColor"/>',
  tetris: '<g fill="currentColor"><rect x="4" y="3" width="5" height="5" rx="1"/><rect x="4" y="9.5" width="5" height="5" rx="1"/><rect x="4" y="16" width="5" height="5" rx="1"/><rect x="10.5" y="16" width="5" height="5" rx="1"/></g>',
  invaders: '<g fill="currentColor"><rect x="8" y="4" width="2" height="2"/><rect x="14" y="4" width="2" height="2"/><rect x="7" y="7" width="10" height="3"/><rect x="5" y="10" width="14" height="3"/><rect x="5" y="13" width="3" height="2"/><rect x="16" y="13" width="3" height="2"/><rect x="9" y="13" width="6" height="2"/><rect x="6" y="17" width="3" height="2"/><rect x="15" y="17" width="3" height="2"/></g>',
  poker: '<path fill="currentColor" d="M12 3C9.2 7 4 9 4 13a3.6 3.6 0 0 0 6.2 2.6c-.2 1.7-1 2.9-2.2 3.4h8c-1.2-.5-2-1.7-2.2-3.4A3.6 3.6 0 0 0 20 13c0-4-5.2-6-8-10z"/>',
  vim: '<path fill="currentColor" opacity="0.22" d="M12 2 22 12 12 22 2 12z"/><path fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" d="M8 9l4 7 4-7"/>',
  neuro: '<path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" d="M12 5.5a3 3 0 0 0-3 3 3 3 0 0 0-2 5.2c0 2 1.6 3.3 3.5 3.3H12m0-11.5a3 3 0 0 1 3 3 3 3 0 0 1 2 5.2c0 2-1.6 3.3-3.5 3.3H12m0-11.5v11.5"/>',
  comunicacao: '<path fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" d="M5 5h14a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1h-9l-4 4v-4H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z"/>',
};

/** SVG inline para os chips (herda a cor do texto). */
function svgIcon(slug, size) {
  const inner = ICONS[slug] || '';
  return `<svg class="ico" viewBox="0 0 24 24" width="${size}" height="${size}" aria-hidden="true">${inner}</svg>`;
}

// Imagens (data-URL) dos ícones recoloridos p/ os portais do canvas (escuro
// sobre o tile dourado). Pré-carregadas; redesenha quando prontas.
const PORTAL_IMG = {};
function loadPortalIcons(onReady) {
  let pending = 0;
  for (const slug in ICONS) {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">${ICONS[slug].replaceAll('currentColor', '#0d0d12')}</svg>`;
    const img = new Image();
    pending++;
    const done = () => { if (--pending === 0 && onReady) onReady(); };
    img.onload = () => { PORTAL_IMG[slug] = img; done(); };
    img.onerror = done;
    img.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
  }
}

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
        const img = PORTAL_IMG[tile.Portal];
        if (img) {
          ctx.drawImage(img, px + 2, py + 2, TILE - 4, TILE - 4);
        } else {
          ctx.font = 'bold 9px sans-serif';
          ctx.textAlign = 'center';
          ctx.textBaseline = 'middle';
          ctx.fillStyle = '#0d0d12';
          ctx.fillText(tile.Portal[0].toUpperCase(), px + TILE / 2, py + TILE / 2);
        }
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
  ctx.font = 'bold 15px monospace';
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
  snake: 'Snake',
  tetris: 'Tetris',
  invaders: 'Invaders',
  poker: 'Poker',
  vim: 'Vim',
  neuro: 'Neuro',
  comunicacao: 'Comunicação',
};

// Universos acessíveis a partir do lobby (chips clicáveis sob o mapa).
const UNIVERSOS = [
  { slug: 'snake', name: 'Snake' },
  { slug: 'tetris', name: 'Tetris' },
  { slug: 'invaders', name: 'Invaders' },
  { slug: 'poker', name: 'Poker' },
  { slug: 'vim', name: 'Vim' },
  { slug: 'neuro', name: 'Neuro — Atlas 3D' },
  { slug: 'comunicacao', name: 'Comunicação' },
];

function renderUniversos() {
  const root = document.getElementById('universos');
  if (!root) return;
  root.innerHTML = UNIVERSOS.map((u) => `
    <a class="universo-chip" href="/universos/${u.slug}" title="Entrar em ${u.name}">
      ${svgIcon(u.slug, 26)}<span>${u.name}</span>
    </a>
  `).join('');
}

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
          <span class="user">${s.user_id}</span>
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
      <span class="user">${s.user_id}</span>
      <span class="score">${s.score}</span>
      <span class="ts">${shortDate(s.ts)}</span>
    </div>
  `).join('');
}

renderAuthArea();
renderUniversos();
loadScores();
loadActivity();
initLobby().then(() => loadPortalIcons(draw)).catch(console.error);
