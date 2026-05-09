'use strict';

const TILE = 16;

const COLOR = {
  empty: '#0d0d12',
  wall: '#1a1a2e',
  portal: '#d4af37',
};

const SYMBOL = {
  snake: '🐍',
  tetris: '🟦',
  invaders: '👾',
  poker: '🃏',
};

async function renderLobby() {
  const res = await fetch('/api/v1/lobby');
  if (!res.ok) throw new Error(`lobby API ${res.status}`);
  const universe = await res.json();
  const { width, height, tiles } = universe.map;

  const canvas = document.getElementById('canvas');
  canvas.width = width * TILE;
  canvas.height = height * TILE;
  const ctx = canvas.getContext('2d');

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
}

renderLobby().catch(console.error);
