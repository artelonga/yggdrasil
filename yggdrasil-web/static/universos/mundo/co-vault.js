/* mundo/co-vault.js — federação INBOUND: ler/navegar/editar um universo do CO
 * dentro do Mundo (YG-153, a+b). Cliente puro: chama a API do CO direto, com o
 * cookie compartilhado `.artelonga.com.br` (`credentials:'include'`) — o CO tem
 * CORS `mirror_request` + `allow_credentials`, então não precisa de mudança no CO
 * nem de token server-side. A visibilidade/permissão é a do CO (público p/ anon;
 * membro/privado quando logado; write só onde o CO permite).
 *
 * Produz o MESMO contrato de salas que a engine consome (`{rootId,ids,get,roomOf}`),
 * derivando a hierarquia dos SEGMENTOS do `path` da entry (pasta = prefixo do path,
 * nota = arquivo). Reusa a engine/temas — é só uma fonte de dados alternativa.
 *
 * a (ler/navegar): loadCoVault(key) → entries → buildCoRooms.
 * b (CRUD write-back): saveCoEntry / createCoEntry / deleteCoEntry → API do CO. */

const CO_BASE = 'https://co.artelonga.com.br';
const ROOT = '__root__';

function coFetch(path, opts) {
  return fetch(CO_BASE + path, Object.assign({ credentials: 'include' }, opts || {}));
}

// ── a: leitura ───────────────────────────────────────────────────────────────
// Lista as entries de um universo do CO (path/title/frontmatter/body). Respeita
// visibilidade no servidor; 401/403 → vault não acessível.
export async function loadCoVault(key) {
  const r = await coFetch(`/api/v1/universes/${encodeURIComponent(key)}/entries`);
  if (!r.ok) throw new Error('co-vault ' + r.status);
  const j = await r.json();
  return (j.entries || []).map((e) => ({
    path: e.path,
    title: e.title || lastSeg(e.path),
    body: e.body || '',
    status: (e.frontmatter && (e.frontmatter.status || e.frontmatter.state)) || null,
  }));
}

// Lê uma entry única (corpo fresco) — pro inspetor/editor.
export async function getCoEntry(key, path) {
  const r = await coFetch(`/api/v1/universes/${encodeURIComponent(key)}/entries/${path.split('/').map(encodeURIComponent).join('/')}`);
  if (!r.ok) return null;
  return r.json();
}

// ── b: write-back (CRUD) ─────────────────────────────────────────────────────
// PUT atualiza; o CO rejeita (403/405) onde o usuário não tem permissão — então a
// permissão é a do CO, sem reimplementar. Devolve {ok, status}.
export async function saveCoEntry(key, path, body, frontmatter) {
  const r = await coFetch(
    `/api/v1/universes/${encodeURIComponent(key)}/entries/${path.split('/').map(encodeURIComponent).join('/')}`,
    { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ body, frontmatter: frontmatter || undefined }) },
  );
  return { ok: r.ok, status: r.status };
}
export async function createCoEntry(key, path, body, frontmatter) {
  const r = await coFetch(`/api/v1/universes/${encodeURIComponent(key)}/entries`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path, body: body || '', frontmatter: frontmatter || {} }),
  });
  return { ok: r.ok, status: r.status };
}
export async function deleteCoEntry(key, path) {
  const r = await coFetch(
    `/api/v1/universes/${encodeURIComponent(key)}/entries/${path.split('/').map(encodeURIComponent).join('/')}`,
    { method: 'DELETE' });
  return { ok: r.ok, status: r.status };
}

// ── salas a partir dos paths (pasta = prefixo; nota = arquivo) ────────────────
// Mesmo contrato do loader do Mundo: {rootId, ids, get(id)->Room, roomOf(slug)}.
// `id` de sala = caminho do diretório ('' = raiz → ROOT). `slug` da nota = path.
export function buildCoRooms(entries, title) {
  // dir de cada entry + conjunto de todas as pastas (inclui ancestrais)
  const folders = new Set([ROOT]);
  const notesByRoom = {}; // roomId -> [{slug,title,status}]
  const childFolders = {}; // roomId -> Set(childRoomId)
  for (const e of entries) {
    const dir = dirOf(e.path);
    const rid = dir === '' ? ROOT : dir;
    // registra a cadeia de pastas ancestrais + arestas pai→filho
    let cur = '';
    let parent = ROOT;
    for (const seg of dir ? dir.split('/') : []) {
      cur = cur ? cur + '/' + seg : seg;
      folders.add(cur);
      (childFolders[parent] = childFolders[parent] || new Set()).add(cur);
      parent = cur;
    }
    (notesByRoom[rid] = notesByRoom[rid] || []).push({ slug: e.path, title: e.title, status: e.status });
  }
  const ids = [...folders];
  const roomBySlug = {};
  for (const rid of Object.keys(notesByRoom)) for (const n of notesByRoom[rid]) roomBySlug[n.slug] = rid;

  function get(id) {
    const doors = [...(childFolders[id] || [])].map((cid) => ({ id: cid, title: lastSeg(cid) }));
    const notes = notesByRoom[id] || [];
    return layout(id, id === ROOT ? (title || 'Universo CO') : lastSeg(id), id === ROOT ? null : parentOf(id), doors, notes);
  }
  return { rootId: ROOT, ids, get, roomOf: (slug) => roomBySlug[slug] || ROOT };
}

// auto-layout determinístico (grade) — espelha o layoutRoom do loader do Mundo.
function layout(id, title, parent, doorItems, noteItems) {
  const total = doorItems.length + noteItems.length;
  const cols = Math.max(3, Math.min(6, Math.ceil(Math.sqrt(total || 1))));
  const rows = Math.ceil(Math.max(1, total) / cols);
  const w = cols * 2 + 3;
  const h = rows * 2 + 5;
  const tiles = [];
  for (let y = 0; y < h; y++) { const row = []; for (let x = 0; x < w; x++) row.push(x === 0 || y === 0 || x === w - 1 || y === h - 1 ? 1 : 0); tiles.push(row); }
  const room = { id, title, parent, w, h, tiles, doors: [], notes: [], npcs: [] };
  const all = [...doorItems.map((d) => ({ door: d })), ...noteItems.map((n) => ({ note: n }))];
  all.forEach((it, i) => {
    const x = 2 + (i % cols) * 2;
    const y = 2 + Math.floor(i / cols) * 2;
    if (it.door) room.doors.push({ x, y, label: it.door.title, target: it.door.id });
    else room.notes.push({ x, y, slug: it.note.slug, title: it.note.title, status: it.note.status, kind: 'artigo' });
  });
  room.spawn = { x: Math.floor(w / 2), y: h - 2 };
  if (parent !== null) room.exit = { x: w - 2, y: h - 2, target: parent };
  return room;
}

function dirOf(p) { const i = p.lastIndexOf('/'); return i < 0 ? '' : p.slice(0, i); }
function lastSeg(p) { const b = p.split('/').pop() || p; return b.replace(/\.md$/, ''); }
function parentOf(id) { const i = id.lastIndexOf('/'); return i < 0 ? ROOT : id.slice(0, i); }
