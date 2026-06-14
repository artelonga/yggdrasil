/* nee/pack-loader.js — carregador lazy + memory-light de LexiconPacks (YG-155).
 *
 * Pacotes são buscados SOB DEMANDA (lazy-load por sala/entrada) da API
 * `/api/v1/comunicacao/packs/{id}` e mantidos num cache LRU com TETO de entradas
 * residentes (MAX_RESIDENT_ENTRIES) — além disso, descarta o pacote menos
 * recentemente usado. Nada de samples: o áudio chega como parâmetros de síntese
 * (ver audio-engine.js). */

// Espelha `pack::MAX_RESIDENT_ENTRIES` no core — contrato único back↔front.
export const MAX_RESIDENT_ENTRIES = 256;

export class PackLoader {
  constructor(opts = {}) {
    this.base = opts.base || '/api/v1/comunicacao';
    this.cap = opts.cap || MAX_RESIDENT_ENTRIES;
    this._cache = new Map(); // id → pack (Map preserva ordem de inserção p/ LRU)
    this._resident = 0; // soma de entradas residentes
  }

  /** Resumos leves (sem entradas) — p/ listar sem carregar nada pesado. */
  async list() {
    const r = await fetch(`${this.base}/packs`);
    if (!r.ok) throw new Error(`packs: HTTP ${r.status}`);
    return r.json();
  }

  /** Carrega (lazy) um pacote por id, com cache LRU. */
  async load(id) {
    if (this._cache.has(id)) {
      const pack = this._cache.get(id);
      this._cache.delete(id); // toca → vira o mais-recente
      this._cache.set(id, pack);
      return pack;
    }
    const r = await fetch(`${this.base}/packs/${encodeURIComponent(id)}`);
    if (!r.ok) throw new Error(`pack ${id}: HTTP ${r.status}`);
    const pack = await r.json();
    this._admit(pack);
    return pack;
  }

  _admit(pack) {
    const n = (pack.entries || []).length;
    this._cache.set(pack.id, pack);
    this._resident += n;
    // descarta LRU (frente do Map) até caber no teto.
    while (this._resident > this.cap && this._cache.size > 1) {
      const oldestId = this._cache.keys().next().value;
      if (oldestId === pack.id) break; // nunca despeja o recém-admitido
      const old = this._cache.get(oldestId);
      this._cache.delete(oldestId);
      this._resident -= (old.entries || []).length;
    }
  }

  /** Nº de entradas residentes (observabilidade/teste). */
  get residentEntries() {
    return this._resident;
  }
}
