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
  playerX: 0,
  playerY: 0,
  tiles: null,
  width: 0,
  height: 0,
  ctx: null,
};

function isWall(x, y) {
  if (x < 0 || y < 0 || x >= state.width || y >= state.height) return true;
  return state.tiles[y][x] === 'Wall';
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
  window.location.assign('/games/' + slug);
}

function handleKey(e) {
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
  state.ctx = canvas.getContext('2d');
  state.tiles = tiles;
  state.width = width;
  state.height = height;
  state.playerX = Math.floor(width / 2);
  state.playerY = Math.floor(height / 2);

  draw();
  window.addEventListener('keydown', handleKey);
}

initLobby().catch(console.error);
