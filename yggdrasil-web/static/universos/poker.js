'use strict';

const STORAGE_KEY = 'yggdrasil-jwt';
const POLL_MS = 2000;

const state = {
  token: localStorage.getItem(STORAGE_KEY),
  userId: null,
  lobbies: [],
  activeLobby: null,
  pollTimer: null,
  meSeated: false,
  lastRound: null,
  ws: null,
  wsActive: false,
  atv: null,  // YG-145: presença/atividade do universo
};

const el = {
  ctaLogin:       document.getElementById('cta-login'),
  lobbyList:      document.getElementById('lobby-list'),
  tableView:      document.getElementById('table-view'),
  tableName:      document.getElementById('table-name'),
  seats:          document.getElementById('seats'),
  ctaInvite:      document.getElementById('cta-invite'),
  inviteUrl:      document.getElementById('invite-url'),
  copiedMsg:      document.getElementById('copied-msg'),
  voltar:         document.getElementById('btn-voltar'),
  status:         document.getElementById('status'),
  erroBanner:     document.getElementById('erro-banner'),
  gameArea:       document.getElementById('game-area'),
  suaVezBanner:   document.getElementById('sua-vez-banner'),
  communityCards: document.getElementById('community-cards'),
  roundName:      document.getElementById('round-name'),
  potValue:       document.getElementById('pot-value'),
  betValue:       document.getElementById('bet-value'),
  holeCardsArea:  document.getElementById('hole-cards-area'),
  holeCards:      document.getElementById('hole-cards'),
  actionBar:      document.getElementById('action-bar'),
  btnFold:        document.getElementById('btn-fold'),
  btnCheck:       document.getElementById('btn-check'),
  btnCall:        document.getElementById('btn-call'),
  btnRaise:       document.getElementById('btn-raise'),
  raiseAmount:    document.getElementById('raise-amount'),
  winnerBanner:   document.getElementById('winner-banner'),
  playersList:    document.getElementById('players-list'),
  saldoHeader:    document.getElementById('saldo-header'),
  saldoValue:     document.getElementById('saldo-value'),
};

const BUY_IN_SEMENTES = 1_000;

function decodeJwt(token) {
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

async function api(path, options = {}) {
  const res = await fetch(path, {
    ...options,
    headers: { ...authHeaders(), 'Content-Type': 'application/json', ...(options.headers || {}) },
  });
  if (res.status === 401) {
    localStorage.removeItem(STORAGE_KEY);
    state.token = null;
    showLoginCta();
    throw new Error('401');
  }
  return res;
}

function setStatus(msg) { el.status.textContent = msg; }

async function refreshSaldo() {
  try {
    const res = await api('/api/v1/me/sementes');
    if (!res.ok) return;
    const data = await res.json();
    el.saldoValue.textContent = data.saldo.toLocaleString('pt-BR');
    el.saldoHeader.style.display = 'flex';
  } catch (_) { /* silent — saldo é informativo */ }
}

function showError(msg) {
  if (!msg) { el.erroBanner.style.display = 'none'; return; }
  el.erroBanner.textContent = msg;
  el.erroBanner.style.display = 'block';
}

function showLoginCta() {
  el.ctaLogin.style.display = 'block';
  el.lobbyList.classList.add('hidden');
  el.tableView.classList.remove('active');
}

async function loadLobbies() {
  setStatus('Carregando mesas…');
  const res = await api('/api/v1/poker/lobbies');
  const data = await res.json();
  state.lobbies = data.lobbies;
  renderLobbyList();
  setStatus('');
}

function renderLobbyList() {
  el.ctaLogin.style.display = 'none';
  el.tableView.classList.remove('active');
  el.lobbyList.classList.remove('hidden');
  el.lobbyList.innerHTML = `
    <div class="lobbies">
      ${state.lobbies.map((l) => {
        const humans = l.seats.filter((s) => s.kind === 'human').length;
        const bots = l.seats.filter((s) => s.kind === 'bot').length;
        return `
          <div class="lobby-card" data-id="${l.id}">
            <h3>${l.name}</h3>
            <p class="meta">
              <strong>${humans}</strong> humano${humans === 1 ? '' : 's'} sentado${humans === 1 ? '' : 's'},
              ${bots > 0 ? `<strong>${bots}</strong> bot` : 'sem bots'},
              ${6 - humans - bots} assento${6 - humans - bots === 1 ? '' : 's'} vago${6 - humans - bots === 1 ? '' : 's'}
            </p>
          </div>
        `;
      }).join('')}
    </div>
  `;
  el.lobbyList.querySelectorAll('.lobby-card').forEach((card) => {
    card.addEventListener('click', () => enterLobby(card.dataset.id));
  });
}

async function enterLobby(id) {
  state.activeLobby = id;
  el.lobbyList.classList.add('hidden');
  el.tableView.classList.add('active');
  await refreshLobby();
  connectWs(id);
}

function leaveLobby() {
  state.activeLobby = null;
  state.meSeated = false;
  state.lastRound = null;
  disconnectWs();
  hideGameArea();
  loadLobbies();
}

async function refreshLobby() {
  if (!state.activeLobby) return;
  showError('');
  try {
    const res = await api(`/api/v1/poker/lobbies/${state.activeLobby}`);
    if (!res.ok) { showError('Erro ao atualizar mesa'); return; }
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

async function refreshHand() {
  if (!state.activeLobby) return;
  try {
    const res = await api(`/api/v1/poker/lobbies/${state.activeLobby}/hand`);
    if (!res.ok) return;
    const hand = await res.json();

    // Fetch hole cards if seated and game is active
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

function renderTable(lobby) {
  el.tableName.textContent = lobby.name;
  const meSeated = lobby.seats.some((s) => s.kind === 'human' && s.user_id === state.userId);
  const hasBot = lobby.seats.some((s) => s.kind === 'bot');

  if (hasBot && meSeated) {
    el.ctaInvite.style.display = 'block';
    el.inviteUrl.textContent = location.href;
    el.inviteUrl.onclick = () => {
      navigator.clipboard.writeText(location.href);
      el.copiedMsg.textContent = 'Copiado!';
      setTimeout(() => (el.copiedMsg.textContent = ''), 2000);
    };
  } else {
    el.ctaInvite.style.display = 'none';
  }

  el.seats.innerHTML = lobby.seats
    .map((s, i) => {
      let cls = 'seat';
      let label;
      if (s.kind === 'empty') {
        cls += ' empty';
        label = meSeated ? '— vago —' : '+ sentar aqui';
      } else if (s.kind === 'bot') {
        cls += ' bot';
        label = '🤖 Bot Carvalho';
      } else {
        cls += ' human';
        if (s.user_id === state.userId) cls += ' self';
        label = s.user_id;
      }
      return `
        <div class="${cls}" data-seat="${i}">
          <span class="seat-num">ASSENTO ${i + 1}</span>
          <span class="seat-label">${label}</span>
        </div>
      `;
    })
    .join('');

  el.seats.querySelectorAll('.seat').forEach((seatEl) => {
    const seatNum = parseInt(seatEl.dataset.seat, 10);
    const seat = lobby.seats[seatNum];
    seatEl.addEventListener('click', () => {
      if (seat.kind === 'human' && seat.user_id === state.userId) {
        stand();
      } else if (seat.kind !== 'human' && !meSeated) {
        sit(seatNum);
      }
    });
  });

  // Stand button
  if (meSeated && !document.getElementById('btn-stand')) {
    const btn = document.createElement('button');
    btn.id = 'btn-stand';
    btn.className = 'voltar';
    btn.style.marginTop = '0.5rem';
    btn.textContent = 'Levantar da mesa';
    btn.onclick = stand;
    el.tableView.insertBefore(btn, el.gameArea);
  } else if (!meSeated) {
    const btn = document.getElementById('btn-stand');
    if (btn) btn.remove();
  }
}

// ── Card rendering ──────────────────────────────────────────────────────────

const SUIT_SYMBOLS = { hearts: '♥', diamonds: '♦', clubs: '♣', spades: '♠' };
const RED_SUITS = new Set(['hearts', 'diamonds']);

function cardEl(card) {
  const div = document.createElement('div');
  div.className = 'playing-card' + (RED_SUITS.has(card.suit) ? ' red' : '');
  div.innerHTML = `<span>${card.rank}</span><span class="card-suit">${SUIT_SYMBOLS[card.suit] || card.suit}</span>`;
  return div;
}

function cardBackEl() {
  const div = document.createElement('div');
  div.className = 'card-back';
  return div;
}

// ── Game rendering ──────────────────────────────────────────────────────────

function hideGameArea() {
  el.gameArea.style.display = 'none';
  el.winnerBanner.style.display = 'none';
  el.actionBar.style.display = 'none';
  el.suaVezBanner.style.display = 'none';
  el.holeCardsArea.style.display = 'none';
}

function renderGame(hand, holeCards) {
  if (!hand || hand.round === 'Aguardando') {
    hideGameArea();
    return;
  }

  el.gameArea.style.display = 'block';

  // Round + pot
  el.roundName.textContent = hand.round;
  el.potValue.textContent = hand.pot;
  el.betValue.textContent = hand.current_bet;

  // Community cards — animate new ones
  const newRound = hand.round !== state.lastRound;
  state.lastRound = hand.round;
  el.communityCards.innerHTML = '';
  const numBack = Math.max(0, 5 - hand.community_cards.length);
  hand.community_cards.forEach((c) => {
    const ce = cardEl(c);
    if (newRound) ce.style.animation = 'card-flip 0.3s ease-out';
    el.communityCards.appendChild(ce);
  });
  for (let i = 0; i < numBack; i++) el.communityCards.appendChild(cardBackEl());

  // Hole cards
  if (holeCards && holeCards.length === 2) {
    el.holeCardsArea.style.display = 'block';
    el.holeCards.innerHTML = '';
    holeCards.forEach((c) => el.holeCards.appendChild(cardEl(c)));
  } else {
    el.holeCardsArea.style.display = 'none';
  }

  // Winner banner
  if (hand.game_over && hand.winner_message) {
    el.winnerBanner.textContent = hand.winner_message;
    el.winnerBanner.style.display = 'block';
    el.actionBar.style.display = 'none';
    el.suaVezBanner.style.display = 'none';
    // Pot foi creditado ao chip stack do vencedor — saldo só muda no cash-out (stand).
    // Mas atualizar saldo aqui é um no-op informativo barato.
    if (!state.handEndedAcked) {
      state.handEndedAcked = true;
      refreshSaldo();
    }
  } else {
    el.winnerBanner.style.display = 'none';
    state.handEndedAcked = false;
  }

  // Action buttons — only on user's turn
  const isMyTurn = !hand.game_over && hand.current_actor === state.userId;
  if (isMyTurn) {
    el.suaVezBanner.style.display = 'block';
    el.actionBar.style.display = 'flex';
    // Auto-scroll to game area
    el.gameArea.scrollIntoView({ behavior: 'smooth', block: 'nearest' });

    // Show/hide check vs call based on whether we owe chips
    const myPlayer = hand.players.find((p) => p.user_id === state.userId);
    const myBet = myPlayer ? myPlayer.current_bet : 0;
    if (myBet < hand.current_bet) {
      el.btnCheck.style.display = 'none';
      el.btnCall.style.display = 'inline-block';
    } else {
      el.btnCheck.style.display = 'inline-block';
      el.btnCall.style.display = 'none';
    }
  } else {
    el.suaVezBanner.style.display = 'none';
    el.actionBar.style.display = 'none';
  }

  // Players list
  el.playersList.innerHTML = hand.players.map((p) => {
    const dealer = p.is_dealer ? '<span class="dealer-chip">★</span>' : '';
    const foldedCls = p.folded ? ' folded' : '';
    const isActor = !hand.game_over && p.user_id === hand.current_actor ? ' →' : '';
    return `<div class="player-row${foldedCls}">${dealer} ${p.user_id}${isActor} — ${p.chips} fichas (bet: ${p.current_bet})</div>`;
  }).join('');
}

// ── Action handlers ─────────────────────────────────────────────────────────

async function sendAction(action, amount) {
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
      if (hcRes.ok) { const d = await hcRes.json(); holeCards = d.cards; }
    }
    renderGame(hand, holeCards);
  } catch (e) {
    if (e.message !== '401') showError(`Erro: ${e.message}`);
  }
}

el.btnFold.addEventListener('click', () => sendAction('fold'));
el.btnCheck.addEventListener('click', () => sendAction('check'));
el.btnCall.addEventListener('click', () => sendAction('call'));
el.btnRaise.addEventListener('click', () => {
  const amt = parseInt(el.raiseAmount.value, 10) || 40;
  sendAction('raise', amt);
});

// ── Seating ─────────────────────────────────────────────────────────────────

async function sit(seat) {
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

async function stand() {
  try {
    const res = await api(`/api/v1/poker/lobbies/${state.activeLobby}/stand`, {
      method: 'POST',
    });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      showError(body.erro || `Erro ${res.status}`);
      return;
    }
    // YG-145: levantar da mesa é o "fim de partida" do poker — sem fim de jogo,
    // só o momento de sair com o saldo. Presença segue até fechar a aba.
    if (state.atv) state.atv.partida({ resultado: 'levantou' });
    refreshSaldo();
    refreshLobby();
  } catch (e) {
    if (e.message !== '401') showError(`Erro: ${e.message}`);
  }
}

function startPolling() {
  stopPolling();
  state.pollTimer = setInterval(refreshLobby, POLL_MS);
}

function stopPolling() {
  if (state.pollTimer) { clearInterval(state.pollTimer); state.pollTimer = null; }
}

function connectWs(lobbyId) {
  disconnectWs();
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  const url = `${proto}//${location.host}/api/v1/poker/lobbies/${lobbyId}/ws`;
  try {
    const ws = new WebSocket(url);
    ws.onopen = () => { state.wsActive = true; stopPolling(); };
    ws.onmessage = () => refreshLobby();
    ws.onerror = () => { state.wsActive = false; startPolling(); };
    ws.onclose = () => { if (state.wsActive) { state.wsActive = false; startPolling(); } };
    state.ws = ws;
  } catch (_) {
    startPolling();
  }
}

function disconnectWs() {
  if (state.ws) {
    state.wsActive = false;
    state.ws.onclose = null;
    state.ws.close();
    state.ws = null;
  }
  stopPolling();
}

el.voltar.addEventListener('click', leaveLobby);

function init() {
  // YG-145: poker é colaborativo e sem fim — presença vale mesmo deslogado/observando.
  // telemetria.js é `defer`, então só está pronto no DOMContentLoaded (init() roda antes).
  window.addEventListener('DOMContentLoaded', function () {
    if (window.yggTelemetria) state.atv = window.yggTelemetria.atividade('poker');
  });
  if (!state.token) { showLoginCta(); return; }
  const claims = decodeJwt(state.token);
  if (!claims || !claims.sub) {
    localStorage.removeItem(STORAGE_KEY);
    state.token = null;
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
