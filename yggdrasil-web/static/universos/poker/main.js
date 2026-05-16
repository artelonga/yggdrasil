// main.js — composition root. Conecta os módulos e faz o boot.
//
// Esta é a ÚNICA arquivo com efeitos de side de boot (init() roda no
// final). Os outros módulos exportam funções puras-ish e estado.
//
// Sequência:
//   1. Decodificar JWT, popular state.userId
//   2. Injetar handlers em views.js (callbacks de clique)
//   3. Carregar saldo e lista de mesas
//   4. Wire dos botões da action bar e do "voltar"

import { state, el, STORAGE_KEY } from './state.js';
import { decodeJwt } from './api.js';
import { setViewHandlers } from './views.js';
import { showLoginCta, showError, setStatus } from './ui.js';
import {
  enterLobby,
  leaveLobby,
  loadLobbies,
  refreshSaldo,
  sendAction,
  sit,
  stand,
  saveFavoriteHand,
} from './actions.js';

function wireHandlers() {
  setViewHandlers({
    onEnterLobby: enterLobby,
    onSit: sit,
    onStand: stand,
    onFavorite: saveFavoriteHand,
    onRefreshSaldo: refreshSaldo,
    onLoadLobbies: loadLobbies,
  });

  el.btnFold.addEventListener('click', () => sendAction('fold'));
  el.btnCheck.addEventListener('click', () => sendAction('check'));
  el.btnCall.addEventListener('click', () => sendAction('call'));
  el.btnRaise.addEventListener('click', () => {
    const amt = parseInt(el.raiseAmount.value, 10) || 40;
    sendAction('raise', amt);
  });

  el.voltar.addEventListener('click', leaveLobby);
}

function init() {
  wireHandlers();

  if (!state.token) {
    showLoginCta();
    return;
  }
  const claims = decodeJwt(state.token);
  if (!claims || !claims.sub) {
    showLoginCta();
    return;
  }
  state.userId = claims.sub;
  refreshSaldo();
  loadLobbies().catch((e) => {
    if (e.message !== '401') {
      showError(`Erro ao carregar: ${e.message}`);
      setStatus('');
    }
  });
}

init();
