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
function layoutRoom(id, title, parent, doorItems, noteItems) {
  const total = doorItems.length + noteItems.length;
  const cols = Math.max(3, Math.min(6, Math.ceil(Math.sqrt(total || 1))));
  const rows = Math.ceil(Math.max(1, total) / cols);
  const w = cols * 2 + 3;
  const h = rows * 2 + 5;
  const room = { id, title, parent, w, h, tiles: emptyTiles(w, h), doors: [], notes: [], npcs: [] };
  const all = [
    ...doorItems.map((d) => ({ door: d })),
    ...noteItems.map((n) => ({ note: n })),
  ];
  all.forEach((it, i) => {
    const x = 2 + (i % cols) * 2;
    const y = 2 + Math.floor(i / cols) * 2;
    if (it.door) room.doors.push({ x, y, label: it.door.title, target: it.door.id });
    else room.notes.push({ x, y, ...it.note });
  });
  const cx = Math.floor(w / 2);
  room.spawn = { x: cx, y: h - 2 };
  // toda sala não-raiz tem uma saída pisável de volta ao pai (canto-baixo).
  if (parent !== null) room.exit = { x: 2, y: h - 2, target: parent };
  return room;
}

/**
 * Indexa a instância real e expõe as salas com **carregamento preguiçoso**
 * (YG-151): a hierarquia inteira do vault é conhecida de antemão (barata —
 * só conexões `parent`), mas a `Room` de cada sala (grade de tiles + layout)
 * só é construída ao **entrar** nela, e então fica em cache. Assim um vault
 * grande não trava: nunca se lê/layouta o vault todo de uma vez.
 *
 * @returns {{ rootId: string, ids: string[], get: (id:string)=>Object|null }}
 *   `ids` = toda sala navegável (raiz + cada pasta), sem construir nenhuma;
 *   `get(id)` = a `Room` (lazy + cache).
 */
export function buildRooms(inst, notes) {
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

  // toda sala navegável, derivada da hierarquia SEM construir layout:
  // a raiz + cada pasta (= bloco com filhos). Recursivo de fato pois cobre
  // QUALQUER profundidade (o vault inteiro), não um subconjunto.
  const ids = [ROOT_ID, ...Object.keys(blocks).filter((id) => isFolder(id))];

  const cache = {};
  // Constrói UMA sala sob demanda (lazy). A raiz reúne os blocos sem pai;
  // uma pasta reúne seus filhos diretos. Filhos-pasta viram portas; filhos-folha,
  // objetos. Só esta sala é layoutada — as filhas só ao serem entradas.
  function get(id) {
    if (cache[id]) return cache[id];
    let title;
    let parentRoom;
    let childIds;
    if (id === ROOT_ID) {
      title = inst.title || 'Universo';
      parentRoom = null;
      childIds = Object.keys(blocks).filter((bid) => !parentOf[bid]);
    } else {
      const b = blocks[id];
      if (!b || !isFolder(id)) return null;
      title = meta(b).title;
      parentRoom = parentOf[id] || ROOT_ID; // pai-pasta, ou a raiz no topo
      childIds = childrenOf[id] || [];
    }
    const doorItems = [];
    const noteItems = [];
    for (const cid of childIds) {
      const b = blocks[cid];
      if (!b) continue;
      const m = meta(b);
      if (isFolder(cid)) doorItems.push({ id: cid, title: m.title });
      else noteItems.push(m);
    }
    cache[id] = layoutRoom(id, title, parentRoom, doorItems, noteItems);
    return cache[id];
  }

  return { rootId: ROOT_ID, ids, get };
}
