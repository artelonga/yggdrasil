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
    // YG-159: identidade estável — a hierarquia vem do `parent` (id/slug no
    // frontmatter) quando existe; senão cai no path. Assim tradução (que muda
    // path/título) não reformata a árvore. Ver docs/architecture/i18n-stable-identity.md.
    parent: (e.frontmatter && e.frontmatter.parent) || null,
    // YG-159 i18n: traduções por locale no frontmatter (`title_en`/`body_en` ou
    // um mapa `i18n: {en:{title,body}}`). Identidade (slug/parent) é estável; só
    // title/body variam por locale → trocar idioma não mexe na árvore nem nos links.
    i18n: gatherI18n(e.frontmatter),
  }));
}

// Junta as traduções do frontmatter num mapa { <locale>: {title?, body?} }.
function gatherI18n(fm) {
  const m = {};
  if (!fm) return m;
  for (const k of Object.keys(fm)) {
    const t = /^title_([a-z]{2})$/.exec(k);
    const b = /^body_([a-z]{2})$/.exec(k);
    if (t) (m[t[1]] = m[t[1]] || {}).title = fm[k];
    else if (b) (m[b[1]] = m[b[1]] || {}).body = fm[k];
  }
  if (fm.i18n && typeof fm.i18n === 'object') {
    for (const loc of Object.keys(fm.i18n)) m[loc] = Object.assign(m[loc] || {}, fm.i18n[loc]);
  }
  return m;
}

// Locales disponíveis no conjunto de entries (pro seletor de idioma).
export function localesOf(entries) {
  const s = new Set();
  for (const e of entries) if (e.i18n) for (const l of Object.keys(e.i18n)) s.add(l);
  return [...s];
}

// Aplica um locale: troca title/body pela tradução quando há; mantém slug/parent
// (identidade) intactos. `loc` vazio = fonte. Render relabela; árvore/links não mudam.
export function localize(entries, loc) {
  if (!loc) return entries;
  return entries.map((e) => {
    const tr = e.i18n && e.i18n[loc];
    return tr ? { ...e, title: tr.title || e.title, body: tr.body || e.body } : e;
  });
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
  const byId = {};
  for (const e of entries) byId[e.path] = e;
  // YG-159: sala efetiva de uma entry = `parent` (id/slug estável) se houver;
  // senão o diretório do path (fallback). Id estável > display (path/título).
  const effRoom = (e) => (e.parent ? e.parent : (dirOf(e.path) === '' ? ROOT : dirOf(e.path)));
  // pastas-por-id: entries que aparecem como `parent` de alguém (são salas, não objetos)
  const idFolders = new Set();
  for (const e of entries) if (e.parent) idFolders.add(e.parent);

  const notesByRoom = {};   // roomId -> [{slug,title,status}]
  const roomBySlug = {};    // slug -> roomId efetivo
  for (const e of entries) {
    if (idFolders.has(e.path)) continue; // é pasta (tem filhos por id), não folha
    const r = effRoom(e);
    (notesByRoom[r] = notesByRoom[r] || []).push({ slug: e.path, title: e.title, status: e.status });
    roomBySlug[e.path] = r;
  }

  // conjunto de salas + pai de cada sala (id-folders + cadeia de path-dirs)
  const folders = new Set([ROOT]);
  const parentOfRoom = {};
  for (const fid of idFolders) { folders.add(fid); parentOfRoom[fid] = byId[fid] ? effRoom(byId[fid]) : ROOT; }
  for (const e of entries) {
    if (e.parent) continue; // id-based: sem pasta de path
    let cur = '', par = ROOT;
    for (const seg of (dirOf(e.path) ? dirOf(e.path).split('/') : [])) {
      cur = cur ? cur + '/' + seg : seg;
      folders.add(cur); if (parentOfRoom[cur] == null) parentOfRoom[cur] = par; par = cur;
    }
  }
  const childFolders = {};
  for (const f of folders) { if (f === ROOT) continue; const p = parentOfRoom[f] || ROOT; (childFolders[p] = childFolders[p] || []).push(f); }

  const folderTitle = (fid) => (byId[fid] ? byId[fid].title : lastSeg(fid));
  function get(id) {
    const doors = (childFolders[id] || []).map((cid) => ({ id: cid, title: folderTitle(cid) }));
    const notes = notesByRoom[id] || [];
    return layout(id, id === ROOT ? (title || 'Universo CO') : folderTitle(id),
      id === ROOT ? null : (parentOfRoom[id] || ROOT), doors, notes);
  }
  return { rootId: ROOT, ids: [...folders], get, roomOf: (slug) => roomBySlug[slug] || ROOT };
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
