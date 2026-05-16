// api.js — fetch wrapper autenticado contra /api/v1/poker/* e /api/v1/me/*.
//
// Responsabilidades:
//   - injetar `Authorization: Bearer <local-jwt>` em toda chamada
//   - decodificar JWT para extrair `sub` (user_id) no boot
//   - tratar 401 lavando o token + voltando para CTA de login
//
// Não conhece poker — é genérico. Vide docs/POKER-MULTIPLAYER.md#mensageria-através-dos-servidores-co.

import { state } from './state.js';
import { showLoginCta } from './ui.js';

export function decodeJwt(token) {
  try {
    const payload = token.split('.')[1];
    const json = atob(payload.replace(/-/g, '+').replace(/_/g, '/'));
    return JSON.parse(json);
  } catch {
    return null;
  }
}

function authHeaders() {
  return { Authorization: `Bearer ${state.token}` };
}

/// `fetch` wrapper. Lança Error('401') quando o token foi rejeitado para
/// que callers possam diferenciar "rede caiu" de "preciso re-logar".
export async function api(path, options = {}) {
  const res = await fetch(path, {
    ...options,
    headers: {
      ...authHeaders(),
      'Content-Type': 'application/json',
      ...(options.headers || {}),
    },
  });
  if (res.status === 401) {
    showLoginCta();
    throw new Error('401');
  }
  return res;
}
