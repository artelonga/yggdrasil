// actions.js — orquestra IO (HTTP) + estado do cliente + polling da mesa ativa.
//
// Cada export aqui é um "comando" que muda estado tanto no servidor (via api.js)
// quanto no cliente (via state.js) e re-renderiza (via views.js).
// Vide docs/POKER-MULTIPLAYER.md#processamento-em-tempo-real.

import { state, el, POLL_MS, BUY_IN_SEMENTES } from './state.js';
import { api } from './api.js';
import { setStatus, showError, hideGameArea } from './ui.js';
import { renderTable, renderGame, renderLobbyList, stopListPolling } from './views.js';

/// Polling da mesa ativa: 2s. Ativo somente enquanto o usuário está em
/// uma mesa aberta. Stops quando voltar para o seletor.
export function startTablePolling() {
  stopTablePolling();
  state.pollTimer = setInterval(refreshLobby, POLL_MS);
}

export function stopTablePolling() {
  if (state.pollTimer) {
    clearInterval(state.pollTimer);
    state.pollTimer = null;
  }
}

export async function refreshSaldo() {
  try {
    const res = await api('/api/v1/me/sementes');
    if (!res.ok) return;
    const data = await res.json();
    el.saldoValue.textContent = data.saldo.toLocaleString('pt-BR');
    el.saldoHeader.style.display = 'flex';
  } catch (_) { /* silent — saldo é informativo */ }
}

export async function loadLobbies() {
  setStatus('Carregando mesas…');
  const res = await api('/api/v1/poker/lobbies');
  const data = await res.json();
  state.lobbies = data.lobbies;
  renderLobbyList();
  setStatus('');
}

export async function enterLobby(id) {
  state.activeLobby = id;
  el.lobbyList.classList.add('hidden');
  el.tableView.classList.add('active');
  stopListPolling(); // troca para polling da mesa
  await refreshLobby();
  startTablePolling();
}

export function leaveLobby() {
  state.activeLobby = null;
  state.meSeated = false;
  state.lastRound = null;
  state.lastCommunityKey = null;
  state.lastHoleKey = null;
  stopTablePolling();
  hideGameArea();
  loadLobbies();
}

/// Refresca estado da mesa ativa (seats + hand). Chamado pelo polling
/// de 2s e por cada ação do usuário.
export async function refreshLobby() {
  if (!state.activeLobby) return;
  showError('');
  try {
    const res = await api(`/api/v1/poker/lobbies/${state.activeLobby}`);
    if (!res.ok) {
      showError('Erro ao atualizar mesa');
      return;
    }
    const lobby = await res.json();
    renderTable(lobby);
    state.meSeated = lobby.seats.some((s) => s.kind === 'human' && s.user_id === state.userId);
    if (state.meSeated) {
      await refreshHand();
    } else {
      hideGameArea();
    }
  } catch (e) {
    if (e.message !== '401') showError(`Erro: ${e.message}`);
  }
}

export async function refreshHand() {
  if (!state.activeLobby) return;
  try {
    const res = await api(`/api/v1/poker/lobbies/${state.activeLobby}/hand`);
    if (!res.ok) return;
    const hand = await res.json();

    // Hole cards: só se sentado e mão em andamento.
    let holeCards = null;
    if (!hand.game_over && hand.round !== 'Aguardando') {
      const hcRes = await api(`/api/v1/poker/lobbies/${state.activeLobby}/hole-cards`);
      if (hcRes.ok) {
        const hcData = await hcRes.json();
        holeCards = hcData.cards;
      }
    }
    renderGame(hand, holeCards);
  } catch (e) {
    if (e.message !== '401') setStatus('Erro ao carregar partida');
  }
}

export async function sendAction(action, amount) {
  const body = { action };
  if (amount !== undefined) body.amount = amount;
  try {
    const res = await api(`/api/v1/poker/lobbies/${state.activeLobby}/action`, {
      method: 'POST',
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      const data = await res.json().catch(() => ({}));
      showError(data.erro || `Erro ${res.status}`);
      return;
    }
    const hand = await res.json();
    let holeCards = null;
    if (!hand.game_over && hand.round !== 'Aguardando') {
      const hcRes = await api(`/api/v1/poker/lobbies/${state.activeLobby}/hole-cards`);
      if (hcRes.ok) {
        const d = await hcRes.json();
        holeCards = d.cards;
      }
    }
    renderGame(hand, holeCards);
  } catch (e) {
    if (e.message !== '401') showError(`Erro: ${e.message}`);
  }
}

export async function sit(seat) {
  try {
    const res = await api(`/api/v1/poker/lobbies/${state.activeLobby}/sit`, {
      method: 'POST',
      body: JSON.stringify({ seat }),
    });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      if (res.status === 402) {
        showError(`${body.erro || 'Saldo insuficiente'} — buy-in é ${BUY_IN_SEMENTES} sementes.`);
      } else {
        showError(body.erro || `Erro ${res.status}`);
      }
      return;
    }
    refreshSaldo();
    refreshLobby();
  } catch (e) {
    if (e.message !== '401') showError(`Erro: ${e.message}`);
  }
}

export async function stand() {
  try {
    const res = await api(`/api/v1/poker/lobbies/${state.activeLobby}/stand`, {
      method: 'POST',
    });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      showError(body.erro || `Erro ${res.status}`);
      return;
    }
    refreshSaldo();
    refreshLobby();
  } catch (e) {
    if (e.message !== '401') showError(`Erro: ${e.message}`);
  }
}

export async function saveFavoriteHand() {
  if (!state.activeLobby) return;
  const btn = document.getElementById('btn-favorite-hand');
  if (btn) {
    btn.disabled = true;
    btn.textContent = 'Salvando...';
  }
  try {
    const res = await api(`/api/v1/me/favorites/hands/${state.activeLobby}`, {
      method: 'POST',
    });
    if (res.ok) {
      if (btn) btn.textContent = '✓ Salva! Veja em /favoritos';
    } else {
      const data = await res.json().catch(() => ({}));
      if (btn) {
        btn.disabled = false;
        btn.textContent = `Erro: ${data.erro || res.status}`;
      }
    }
  } catch (_) {
    if (btn) {
      btn.disabled = false;
      btn.textContent = 'Tentar novamente';
    }
  }
}
