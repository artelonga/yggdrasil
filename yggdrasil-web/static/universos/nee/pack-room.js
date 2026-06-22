/* nee/pack-room.js — adaptador LexiconPack → Room (YG-171).
 *
 * packToRoom(pack): converte um LexiconPack no Room JSON que o engine.js
 * (YG-148) já consome. pos.x/y do pack → coordenadas de tile (× TILE_SCALE);
 * pos.z IGNORADO no 2D mas PRESERVADO em note._z — não destruir a dimensão p/
 * um cliente 3D futuro trocar apenas o mapeamento de pos (mesmo packToRoom,
 * tile-calc diferente → voxel-calc).
 *
 * Engine-neutro: nenhuma suposição de pixels além da escala de tile.
 * Sem estado global: apenas transformação de dado → dado. */

import { TileKind } from '../mundo/engine.js';

// 1 unidade de pos.{x,y} no pack = TILE_SCALE tiles — garante espaço p/ andar.
const TILE_SCALE = 2;
// Margem de parede ao redor das entradas.
const MARGIN = 2;

/** Converte um LexiconPack em um Room consumível pelo engine.js.
 *
 * Contrato:
 *   - pack.entries[].pos.x/y → tile (escala TILE_SCALE); .z preservado em ._z.
 *   - Room sem `exit` (sala autônoma — não faz parte de uma hierarquia de vault).
 *   - Um cliente 3D usaria o MESMO packToRoom, substituindo apenas o mapeamento
 *     de pos por coordenadas de voxel ou mundo-3D. */
export function packToRoom(pack) {
  const entries = pack.entries || [];
  if (!entries.length) return _emptyRoom(pack);

  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const e of entries) {
    const px = e.pos ? e.pos.x : 0;
    const py = e.pos ? e.pos.y : 0;
    if (px < minX) minX = px;
    if (py < minY) minY = py;
    if (px > maxX) maxX = px;
    if (py > maxY) maxY = py;
  }

  const w = Math.round((maxX - minX) * TILE_SCALE) + MARGIN * 2 + 3;
  const h = Math.round((maxY - minY) * TILE_SCALE) + MARGIN * 2 + 5;
  const room = _base(pack, w, h);

  for (const e of entries) {
    const px = e.pos ? e.pos.x : 0;
    const py = e.pos ? e.pos.y : 0;
    const pz = e.pos ? e.pos.z : 0;
    const tx = MARGIN + Math.round((px - minX) * TILE_SCALE);
    const ty = MARGIN + Math.round((py - minY) * TILE_SCALE);
    room.notes.push({
      x: tx,
      y: ty,
      label: e.term,
      term: e.term,
      gloss: e.gloss || '',
      role: e.role || '',
      audio: e.audio || null,
      relations: e.relations || [],
      _z: pz, // preservado — cliente 3D usa; consumidor 2D ignora
    });
  }

  room.spawn = { x: Math.floor(w / 2), y: h - 2 };
  return room;
}

function _emptyRoom(pack) {
  const w = 7, h = 7;
  const room = _base(pack, w, h);
  room.spawn = { x: 3, y: 5 };
  return room;
}

function _base(pack, w, h) {
  return {
    id: 'pack:' + pack.id,
    title: pack.title || pack.id,
    parent: null,
    w,
    h,
    tiles: _tiles(w, h),
    doors: [],
    notes: [],
    npcs: [],
    portals: [],
    spawn: { x: Math.floor(w / 2), y: h - 2 },
  };
}

function _tiles(w, h) {
  const tiles = [];
  for (let y = 0; y < h; y++) {
    const row = [];
    for (let x = 0; x < w; x++) {
      row.push(x === 0 || y === 0 || x === w - 1 || y === h - 1 ? TileKind.WALL : TileKind.FLOOR);
    }
    tiles.push(row);
  }
  return tiles;
}
