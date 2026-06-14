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
 * (a engine é data-agnóstica; só consome a `Room`).
 *
 * YG-156: o loader é **lazy** (YG-151) — toda a hierarquia é conhecida de antemão
 * (barato), mas cada `Room` só é construída ao entrar nela (com cache) → o vault
 * INTEIRO é navegável sem estourar memória. O **layout persistido** do drag-drop
 * (YG-154: `pos{room,x,y}`/`parent` no frontmatter `.md`) é **folado** no lazy:
 * pré-computado numa passada barata e aplicado sala-a-sala em `get(id)`, sem
 * jamais materializar `byId` do vault inteiro. */
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
 * (YG-151) + o **layout persistido folado** (YG-154/156). A hierarquia inteira
 * do vault é conhecida de antemão (barata — só conexões `parent` + uma passada
 * pelos frontmatters), mas a `Room` de cada sala (grade + layout) só é construída
 * ao **entrar** nela, e então fica em cache. O `.md` de cada nota pode mover a
 * nota de sala (reparent) e/ou reposicioná-la — isso é refletido na membership
 * efetiva e no overlay de `x/y`, sem construir nenhuma outra sala.
 *
 * @returns {{ rootId: string, ids: string[], get: (id:string)=>Object|null,
 *             roomOf: (slug:string)=>string|null }}
 *   `ids` = toda sala navegável (raiz + cada pasta), sem construir nenhuma;
 *   `get(id)` = a `Room` (lazy + cache, com overrides do `.md` aplicados);
 *   `roomOf(slug)` = a sala efetiva da nota (pro `findNote`/`posOf` sem `byId`).
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
  const validRoom = new Set(ids);

  // YG-156: pré-computa os overrides do `.md` numa passada barata (sem layout).
  // `pos.room`/`parent` são ids de sala (id do bloco-pasta ou `__root__`),
  // iguais aos que o drag-drop grava (YG-154/149). `parent` ganha de `pos.room`.
  const layoutOverride = {}; // slug -> { room?, x?, y? }
  for (const n of notes || []) {
    if (!n || !n.slug) continue;
    const pos = n.pos || null;
    const room = n.parent || (pos && pos.room) || undefined;
    const x = pos && typeof pos.x === 'number' ? pos.x : undefined;
    const y = pos && typeof pos.y === 'number' ? pos.y : undefined;
    if (room === undefined && x === undefined && y === undefined) continue;
    layoutOverride[n.slug] = { room, x, y };
  }

  // YG-156: membership EFETIVA das notas (folhas) por sala, já com reparent.
  // Para cada folha, a sala estrutural é `parentOf[block] || ROOT_ID`; o override
  // a substitui se apontar pra uma sala válida. Assim `get(id)` inclui as notas
  // reparenteadas PRA `id` e exclui as movidas pra FORA — sem construir as outras
  // salas. `roomOf(slug)` é o índice reverso desse mesmo cálculo.
  const notesInRoom = {}; // roomId -> [blockId]
  const roomBySlug = {}; // slug -> roomId efetivo
  for (const id of Object.keys(blocks)) {
    if (isFolder(id)) continue; // pastas viram salas/portas, não objetos
    const b = blocks[id];
    const slug = (b.props && b.props.note_slug) || null;
    const ov = slug ? layoutOverride[slug] : null;
    const structural = parentOf[id] || ROOT_ID;
    let r = ov && ov.room ? ov.room : structural;
    if (!validRoom.has(r)) r = structural; // override órfão → cai pro estrutural
    (notesInRoom[r] = notesInRoom[r] || []).push(id);
    if (slug) roomBySlug[slug] = r;
  }

  const cache = {};
  // Constrói UMA sala sob demanda (lazy). A raiz reúne os blocos sem pai;
  // uma pasta reúne seus filhos diretos. Filhos-pasta viram portas; as notas
  // (folhas) vêm da membership EFETIVA (`notesInRoom`, já reparenteada). Só esta
  // sala é layoutada — as filhas só ao serem entradas. Após o layout, o `x/y`
  // persistido é sobreposto em cada objeto.
  function get(id) {
    if (cache[id]) return cache[id];
    let title;
    let parentRoom;
    let folderChildren;
    if (id === ROOT_ID) {
      title = inst.title || 'Universo';
      parentRoom = null;
      folderChildren = Object.keys(blocks).filter((bid) => !parentOf[bid] && isFolder(bid));
    } else {
      const b = blocks[id];
      if (!b || !isFolder(id)) return null;
      title = meta(b).title;
      parentRoom = parentOf[id] || ROOT_ID; // pai-pasta, ou a raiz no topo
      folderChildren = (childrenOf[id] || []).filter((cid) => isFolder(cid));
    }
    const doorItems = folderChildren.map((cid) => ({ id: cid, title: meta(blocks[cid]).title }));
    const noteItems = (notesInRoom[id] || []).map((cid) => meta(blocks[cid]));
    const r = layoutRoom(id, title, parentRoom, doorItems, noteItems);
    // overlay do `x/y` persistido (YG-154): reabrir mantém pra onde se arrastou.
    for (const obj of r.notes) {
      const ov = obj.slug ? layoutOverride[obj.slug] : null;
      if (!ov) continue;
      if (typeof ov.x === 'number') obj.x = ov.x;
      if (typeof ov.y === 'number') obj.y = ov.y;
    }
    cache[id] = r;
    return r;
  }

  // sala efetiva de uma nota (lookup direto — não constrói nenhuma sala).
  function roomOf(slug) { return roomBySlug[slug] || null; }

  return { rootId: ROOT_ID, ids, get, roomOf };
}
