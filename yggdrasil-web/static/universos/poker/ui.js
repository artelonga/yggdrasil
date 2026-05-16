// ui.js — manipulação de elementos de UI que não dependem do estado do jogo:
// banner de erro, status footer, CTA de login. Sem fetch, sem polling.

import { el, state, STORAGE_KEY } from './state.js';

export function setStatus(msg) {
  el.status.textContent = msg;
}

export function showError(msg) {
  if (!msg) {
    el.erroBanner.style.display = 'none';
    return;
  }
  el.erroBanner.textContent = msg;
  el.erroBanner.style.display = 'block';
}

/// Esconde a área de jogo + winner banner + action bar. Chamado quando o
/// usuário não está sentado ou quando levanta da mesa.
export function hideGameArea() {
  el.gameArea.style.display = 'none';
  el.winnerBanner.style.display = 'none';
  el.actionBar.style.display = 'none';
  el.suaVezBanner.style.display = 'none';
  el.holeCardsArea.style.display = 'none';
}

/// Exibe o CTA de login. Limpa o token local quando chamado (vide api.js
/// no caminho de 401 → re-login).
export function showLoginCta() {
  localStorage.removeItem(STORAGE_KEY);
  state.token = null;
  el.ctaLogin.style.display = 'block';
  el.lobbyList.classList.add('hidden');
  el.tableView.classList.remove('active');
}
