// Caderno do Ayvu Rapyta — favoritos (★), notas (✎), sugestões e progresso.
//
// YG-112: passou de local-first (db.* no localStorage) para persistência por
// usuário no servidor. Quando há sessão (JWT), o Caderno é durável e cross-device
// via /api/v1/comunicacao/caderno/...; sem sessão (anônimo), cai no localStorage
// como antes. No primeiro login, o blob local é migrado pro servidor (idempotente,
// sem perda) e depois limpo localmente.
//
// As notas, quando logado, federam de graça: o servidor as grava via NoteStore
// sob a instância canônica do Ayvu (YG-114) — nada a fazer no cliente.

const API = '/api/v1/comunicacao';
const JWT_KEY = 'yggdrasil-jwt';
// Chave do blob local (shape histórico db.fav/db.notes/db.sugg/db.progress).
const LOCAL_KEY = 'corpus-caderno';
const MIGRATED_KEY = 'corpus-caderno-migrado';

function token() {
  return localStorage.getItem(JWT_KEY);
}

function logado() {
  return !!token();
}

function authHeaders() {
  const t = token();
  return t ? { Authorization: `Bearer ${t}` } : {};
}

// ─── Blob local (fallback anônimo + fonte da migração) ───────────────────────

function blocoVazio() {
  return { fav: {}, notes: {}, sugg: {}, progress: {} };
}

function lerLocal() {
  try {
    const raw = localStorage.getItem(LOCAL_KEY);
    if (!raw) return blocoVazio();
    return { ...blocoVazio(), ...JSON.parse(raw) };
  } catch (_) {
    return blocoVazio();
  }
}

function gravarLocal(db) {
  try {
    localStorage.setItem(LOCAL_KEY, JSON.stringify(db));
  } catch (_) {
    /* quota/Safari privado — silencioso */
  }
}

// ─── HTTP helpers ────────────────────────────────────────────────────────────

async function req(method, path, body) {
  const opts = { method, headers: { ...authHeaders() } };
  if (body !== undefined) {
    opts.headers['Content-Type'] = 'application/json';
    opts.body = JSON.stringify(body);
  }
  const res = await fetch(`${API}${path}`, opts);
  if (!res.ok) throw new Error(`caderno ${method} ${path}: ${res.status}`);
  // 204/sem corpo → null
  const txt = await res.text();
  return txt ? JSON.parse(txt) : null;
}

// ─── API pública do Caderno ──────────────────────────────────────────────────

// Carrega o Caderno: do servidor se logado, senão do localStorage.
async function carregar() {
  if (!logado()) return lerLocal();
  return req('GET', '/caderno');
}

// Favorita um verso (★).
async function favoritar(key) {
  if (logado()) return req('PUT', `/caderno/favoritos/${encodeURIComponent(key)}`);
  const db = lerLocal();
  db.fav[key] = new Date().toISOString();
  gravarLocal(db);
  return { chave: key, favoritado: true };
}

// Desfavorita um verso.
async function desfavoritar(key) {
  if (logado()) return req('DELETE', `/caderno/favoritos/${encodeURIComponent(key)}`);
  const db = lerLocal();
  delete db.fav[key];
  gravarLocal(db);
  return { chave: key, favoritado: false };
}

// Cria/atualiza uma nota (✎). Quando logado, federa pelo Ayvu (YG-114).
async function salvarNota(key, title, markdown) {
  if (logado()) {
    return req('PUT', `/caderno/notas/${encodeURIComponent(key)}`, { title, markdown });
  }
  const db = lerLocal();
  db.notes[key] = {
    key,
    title: title || key,
    markdown: markdown || '',
    updated_at: new Date().toISOString(),
  };
  gravarLocal(db);
  return { chave: key, slug: key, title: db.notes[key].title };
}

// Remove uma nota.
async function removerNota(key) {
  if (logado()) return req('DELETE', `/caderno/notas/${encodeURIComponent(key)}`);
  const db = lerLocal();
  delete db.notes[key];
  gravarLocal(db);
  return { chave: key, removido: true };
}

// Registra progresso de leitura (último verso lido de um capítulo/seção).
async function marcarProgresso(key, verse) {
  if (logado()) {
    return req('PUT', `/caderno/progresso/${encodeURIComponent(key)}`, { verse });
  }
  const db = lerLocal();
  db.progress[key] = verse;
  gravarLocal(db);
  return { chave: key, verse };
}

// Migra o blob local pro servidor (idempotente, sem perda) e limpa o local.
// Chamar uma vez após o login. No-op se anônimo, se já migrado, ou se não há
// nada local a migrar.
async function migrarSeNecessario() {
  if (!logado()) return null;
  if (localStorage.getItem(MIGRATED_KEY)) return null;
  const db = lerLocal();
  const temAlgo =
    Object.keys(db.fav).length || Object.keys(db.notes).length || Object.keys(db.progress).length;
  if (!temAlgo) {
    localStorage.setItem(MIGRATED_KEY, '1');
    return null;
  }
  // O servidor aceita o shape { fav, notes, progress }; `sugg` (YG-113) fica
  // local por ora — a curadoria de sugestões é outro fluxo.
  const consolidado = await req('POST', '/caderno/migrar', {
    fav: db.fav,
    notes: db.notes,
    progress: db.progress,
  });
  localStorage.setItem(MIGRATED_KEY, '1');
  localStorage.removeItem(LOCAL_KEY);
  return consolidado;
}

window.Caderno = {
  logado,
  carregar,
  favoritar,
  desfavoritar,
  salvarNota,
  removerNota,
  marcarProgresso,
  migrarSeNecessario,
};
