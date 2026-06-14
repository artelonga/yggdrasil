/* mundo/loader.js — deriva salas walkable de uma instância REAL (YG-148).
 *
 * Substitui o `sample.js` mock: a fonte é a `UniverseInstance` (InstanceStore)
 * + as notas (NoteStore), já carregadas pelo instance view (YG-126). Sem mock
 * no caminho real.
 *
 * Modelo (unificado, YG-131): cada nó é um bloco; um bloco vira pasta quando tem
 * filhos (conexões `parent`). Daí:
 *   pasta (nó com filhos) → sala  ·  porta = pasta-filha
 *   nota (nó-folha)       → objeto pisável na grade (abre o `.md` real)
 * A sala raiz é a própria instância (blocos sem pai). Posições são auto-layout
 * (a engine é data-agnóstica; só consome a `Room`). */
import { TileKind } from './engine.js';

export const ROOT_ID = '__root__';

// Grade retangular com borda de parede.
function emptyTiles(w, h) {
  const t = [];
  for (let y = 0; y < h; y++) {
    const row = [];
    for (let x = 0; x < w; x++) {
      row.push(x === 0 || y === 0 || x === w - 1 || y === h - 1 ? TileKind.WALL : TileKind.FLOOR);
    }
    t.push(row);
  }
  return t;
}

// Ícone-render do nó (espelha iconeDoNo do instance view): a forma emerge do
// conteúdo — nunca é verdade gravada.
function kindOf(body, temFilhos) {
  if (!String(body || '').trim()) return 'pasta';
  return temFilhos ? 'indice' : 'artigo';
}

// Auto-layout determinístico: portas e notas numa grade (passo 2 p/ respiro);
// spawn no centro-baixo; saída reservada no canto-baixo (não colide com notas).
// `portalItems` (YG-152): portas para OUTROS universos (vaults) — só na raiz;
// viram entidades `portal` próprias (engine: `room.portals`), com `universe` =
// id da instância destino.
function layoutRoom(id, title, parent, doorItems, noteItems, portalItems) {
  portalItems = portalItems || [];
  const total = doorItems.length + noteItems.length + portalItems.length;
  const cols = Math.max(3, Math.min(6, Math.ceil(Math.sqrt(total || 1))));
  const rows = Math.ceil(Math.max(1, total) / cols);
  const w = cols * 2 + 3;
  const h = rows * 2 + 5;
  const room = { id, title, parent, w, h, tiles: emptyTiles(w, h), doors: [], notes: [], npcs: [], portals: [] };
  const all = [
    ...doorItems.map((d) => ({ door: d })),
    ...noteItems.map((n) => ({ note: n })),
    ...portalItems.map((p) => ({ portal: p })),
  ];
  all.forEach((it, i) => {
    const x = 2 + (i % cols) * 2;
    const y = 2 + Math.floor(i / cols) * 2;
    if (it.door) room.doors.push({ x, y, label: it.door.title, target: it.door.id });
    // portal cross-universe (YG-152): entidade própria (engine: `room.portals`),
    // distinta de porta-de-pasta; `universe` = id da instância destino.
    else if (it.portal) room.portals.push({ x, y, label: it.portal.title, universe: it.portal.id, back: !!it.portal.back });
    else room.notes.push({ x, y, ...it.note });
  });
  const cx = Math.floor(w / 2);
  room.spawn = { x: cx, y: h - 2 };
  // toda sala não-raiz tem uma saída pisável de volta ao pai (canto-baixo).
  if (parent !== null) room.exit = { x: 2, y: h - 2, target: parent };
  return room;
}

/**
 * Constrói o conjunto de salas a partir da instância real e suas notas.
 * @param {Array<{id:string,title:string}>} [portals] portas p/ outros universos
 *   (YG-152) — colocadas só na sala raiz (a fronteira do vault).
 * @returns {{ byId: Object, rootId: string }} salas indexadas por id.
 */
export function buildRooms(inst, notes, portals = []) {
  const noteBySlug = {};
  for (const n of notes || []) noteBySlug[n.slug] = n;

  // blocos de todas as camadas (ignora fundo) indexados por id.
  const blocks = {};
  for (const layer of inst.layers || []) {
    if (layer.kind === 'background') continue;
    for (const b of layer.blocks || []) blocks[b.id] = b;
  }

  // hierarquia via conexões `parent` (from=filho, to=pai).
  const childrenOf = {};
  const parentOf = {};
  for (const c of inst.connections || []) {
    if (!c.props || c.props.kind !== 'parent') continue;
    if (!blocks[c.from] || !blocks[c.to]) continue;
    (childrenOf[c.to] = childrenOf[c.to] || []).push(c.from);
    parentOf[c.from] = c.to;
  }
  const isFolder = (id) => (childrenOf[id] || []).length > 0;

  function meta(b) {
    const slug = (b.props && b.props.note_slug) || null;
    const note = slug ? noteBySlug[slug] : null;
    const body = note ? note.body || '' : '';
    return {
      blockId: b.id,
      slug,
      title: (note && note.title) || b.label || slug || b.id,
      body,
      status: note ? note.status || null : null,
      kind: kindOf(body, isFolder(b.id)),
    };
  }

  const byId = {};
  const built = new Set();

  function build(roomId, title, parentRoomId, childIds, portalItems) {
    if (built.has(roomId)) return; // defesa contra ciclos
    built.add(roomId);
    const doorItems = [];
    const noteItems = [];
    for (const id of childIds) {
      const b = blocks[id];
      if (!b) continue;
      const m = meta(b);
      if (isFolder(id)) doorItems.push({ id, title: m.title });
      else noteItems.push(m);
    }
    byId[roomId] = layoutRoom(roomId, title, parentRoomId, doorItems, noteItems, portalItems);
    for (const d of doorItems) {
      build(d.id, meta(blocks[d.id]).title, roomId, childrenOf[d.id] || [], []);
    }
  }

  const rootChildren = Object.keys(blocks).filter((id) => !parentOf[id]);
  // portais p/ outros universos vivem na fronteira do vault = a sala raiz.
  build(ROOT_ID, inst.title || 'Universo', null, rootChildren, portals || []);

  return { byId, rootId: ROOT_ID };
}
