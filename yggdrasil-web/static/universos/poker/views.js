// views.js — renderização dependente de estado: lista de mesas, mesa+assentos,
// estado da mão (community, hole, banner, players row). Sem IO direto.
//
// As ações disparadas por clique vão via *handlers* injetados — views.js
// NÃO importa actions.js (evita ciclo). Vide docs/POKER-MULTIPLAYER.md#mapa-de-módulos-do-cliente.

import { state, el, LIST_POLL_MS } from './state.js';
import { cardEl, cardBackEl } from './cards.js';
import { hideGameArea } from './ui.js';

let handlers = {
  onEnterLobby: () => {},
  onSit: () => {},
  onStand: () => {},
  onFavorite: () => {},
  onRefreshSaldo: () => {},
  onLoadLobbies: () => {},
};

/// Boot wires up callbacks. Chamado uma vez em `main.js`.
export function setViewHandlers(h) {
  Object.assign(handlers, h);
}

/// Polling do seletor de mesas: 4s. Ativo enquanto a lista está visível.
export function startListPolling() {
  stopListPolling();
  state.listPollTimer = setInterval(() => handlers.onLoadLobbies(), LIST_POLL_MS);
}

export function stopListPolling() {
  if (state.listPollTimer) {
    clearInterval(state.listPollTimer);
    state.listPollTimer = null;
  }
}

export function renderLobbyList() {
  el.ctaLogin.style.display = 'none';
  el.tableView.classList.remove('active');
  el.lobbyList.classList.remove('hidden');
  startListPolling();
  el.lobbyList.innerHTML = `
    <div class="lobbies">
      ${state.lobbies.map((l) => {
        const humans = l.seats.filter((s) => s.kind === 'human').length;
        const bots = l.seats.filter((s) => s.kind === 'bot').length;
        const total = l.seats.length; // honra max_seats variável (2 em heads-up, 6 em cash)
        const vagas = total - humans - bots;
        return `
          <div class="lobby-card" data-id="${l.id}">
            <h3>${l.name}</h3>
            <p class="meta">
              <strong>${humans}</strong> humano${humans === 1 ? '' : 's'} sentado${humans === 1 ? '' : 's'},
              ${bots > 0 ? `<strong>${bots}</strong> bot` : 'sem bots'},
              ${vagas} assento${vagas === 1 ? '' : 's'} vago${vagas === 1 ? '' : 's'}
              <span style="opacity:0.4">(${total} max)</span>
            </p>
          </div>
        `;
      }).join('')}
    </div>
  `;
  el.lobbyList.querySelectorAll('.lobby-card').forEach((card) => {
    card.addEventListener('click', () => handlers.onEnterLobby(card.dataset.id));
  });
}

export function renderTable(lobby) {
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
        // lobby.usernames[s.user_id] vem do server (LEFT JOIN user_profiles).
        // Fallback para o próprio user_id se não houver perfil.
        label = (lobby.usernames && lobby.usernames[s.user_id]) || s.user_id;
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
        handlers.onStand();
      } else if (seat.kind !== 'human' && !meSeated) {
        handlers.onSit(seatNum);
      }
    });
  });

  // Botão "Levantar da mesa" (criado/removido conforme `meSeated`).
  if (meSeated && !document.getElementById('btn-stand')) {
    const btn = document.createElement('button');
    btn.id = 'btn-stand';
    btn.className = 'voltar';
    btn.style.marginTop = '0.5rem';
    btn.textContent = 'Levantar da mesa';
    btn.onclick = () => handlers.onStand();
    el.tableView.insertBefore(btn, el.gameArea);
  } else if (!meSeated) {
    const btn = document.getElementById('btn-stand');
    if (btn) btn.remove();
  }
}

export function renderGame(hand, holeCards) {
  if (!hand || hand.round === 'Aguardando') {
    hideGameArea();
    return;
  }

  el.gameArea.style.display = 'block';

  // Round + pot
  el.roundName.textContent = hand.round;
  el.potValue.textContent = hand.pot;
  el.betValue.textContent = hand.current_bet;

  // Community cards — só rebuild quando o conteúdo muda (anti-flicker no
  // polling de 2s). A "chave" das cartas é rank+suit concatenado.
  const communityKey = hand.community_cards.map((c) => `${c.rank}${c.suit}`).join(',');
  if (communityKey !== state.lastCommunityKey) {
    const newRound = hand.round !== state.lastRound;
    state.lastRound = hand.round;
    state.lastCommunityKey = communityKey;
    el.communityCards.innerHTML = '';
    const numBack = Math.max(0, 5 - hand.community_cards.length);
    hand.community_cards.forEach((c) => {
      const ce = cardEl(c);
      if (newRound) ce.style.animation = 'card-flip 0.3s ease-out';
      el.communityCards.appendChild(ce);
    });
    for (let i = 0; i < numBack; i++) el.communityCards.appendChild(cardBackEl());
  }

  // Hole cards — mesma regra anti-flicker.
  if (holeCards && holeCards.length === 2) {
    el.holeCardsArea.style.display = 'block';
    const holeKey = holeCards.map((c) => `${c.rank}${c.suit}`).join(',');
    if (holeKey !== state.lastHoleKey) {
      state.lastHoleKey = holeKey;
      el.holeCards.innerHTML = '';
      holeCards.forEach((c) => el.holeCards.appendChild(cardEl(c)));
    }
  } else {
    el.holeCardsArea.style.display = 'none';
    state.lastHoleKey = null;
  }

  // Winner banner com botão "★ Salvar mão" — só na primeira renderização
  // pós-showdown (debounce via state.handEndedAcked).
  if (hand.game_over && hand.winner_message) {
    el.winnerBanner.innerHTML = `
      <div>${hand.winner_message}</div>
      <button id="btn-favorite-hand" class="btn-favorite">★ Salvar esta mão</button>
    `;
    el.winnerBanner.style.display = 'block';
    el.actionBar.style.display = 'none';
    el.suaVezBanner.style.display = 'none';
    document.getElementById('btn-favorite-hand').onclick = () => handlers.onFavorite();
    if (!state.handEndedAcked) {
      state.handEndedAcked = true;
      handlers.onRefreshSaldo();
    }
  } else {
    el.winnerBanner.style.display = 'none';
    state.handEndedAcked = false;
  }

  // Action bar — só na vez do usuário.
  const isMyTurn = !hand.game_over && hand.current_actor === state.userId;
  if (isMyTurn) {
    el.suaVezBanner.style.display = 'block';
    el.actionBar.style.display = 'flex';
    el.gameArea.scrollIntoView({ behavior: 'smooth', block: 'nearest' });

    // Check vs call: só pode dar check quando o usuário já está coberto.
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

  // Players list (com indicador → de current actor e marca de dealer).
  el.playersList.innerHTML = hand.players.map((p) => {
    const dealer = p.is_dealer ? '<span class="dealer-chip">★</span>' : '';
    const foldedCls = p.folded ? ' folded' : '';
    const isActor = !hand.game_over && p.user_id === hand.current_actor ? ' →' : '';
    const name = p.username || p.user_id;
    return `<div class="player-row${foldedCls}">${dealer} ${name}${isActor} — ${p.chips} fichas (bet: ${p.current_bet})</div>`;
  }).join('');
}
