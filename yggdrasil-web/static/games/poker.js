'use strict';

const STORAGE_KEY = 'yggdrasil-jwt';
const POLL_MS = 2000;

const state = {
  token: localStorage.getItem(STORAGE_KEY),
  userId: null,
  lobbies: [],
  activeLobby: null,
  pollTimer: null,
};

const el = {
  ctaLogin: document.getElementById('cta-login'),
  lobbyList: document.getElementById('lobby-list'),
  tableView: document.getElementById('table-view'),
  tableName: document.getElementById('table-name'),
  seats: document.getElementById('seats'),
  ctaInvite: document.getElementById('cta-invite'),
  inviteUrl: document.getElementById('invite-url'),
  copiedMsg: document.getElementById('copied-msg'),
  voltar: document.getElementById('btn-voltar'),
  status: document.getElementById('status'),
  erroBanner: document.getElementById('erro-banner'),
};

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

function setStatus(msg) {
  el.status.textContent = msg;
}

function showError(msg) {
  if (!msg) {
    el.erroBanner.style.display = 'none';
    return;
  }
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
  startPolling();
}

function leaveLobby() {
  state.activeLobby = null;
  stopPolling();
  loadLobbies();
}

async function refreshLobby() {
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
  } catch (e) {
    if (e.message !== '401') showError(`Erro: ${e.message}`);
  }
}

function renderTable(lobby) {
  el.tableName.textContent = lobby.name;
  const meSeated = lobby.seats.some(
    (s) => s.kind === 'human' && s.user_id === state.userId,
  );
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

  // Action bar — show "stand" if seated
  if (meSeated && !document.getElementById('btn-stand')) {
    const btn = document.createElement('button');
    btn.id = 'btn-stand';
    btn.className = 'voltar';
    btn.style.marginTop = '0.5rem';
    btn.textContent = 'Levantar da mesa';
    btn.onclick = stand;
    el.tableView.appendChild(btn);
  } else if (!meSeated) {
    const btn = document.getElementById('btn-stand');
    if (btn) btn.remove();
  }
}

async function sit(seat) {
  try {
    const res = await api(`/api/v1/poker/lobbies/${state.activeLobby}/sit`, {
      method: 'POST',
      body: JSON.stringify({ seat }),
    });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      showError(body.erro || `Erro ${res.status}`);
      return;
    }
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
  if (state.pollTimer) {
    clearInterval(state.pollTimer);
    state.pollTimer = null;
  }
}

el.voltar.addEventListener('click', leaveLobby);

function init() {
  if (!state.token) {
    showLoginCta();
    return;
  }
  const claims = decodeJwt(state.token);
  if (!claims || !claims.sub) {
    localStorage.removeItem(STORAGE_KEY);
    state.token = null;
    showLoginCta();
    return;
  }
  state.userId = claims.sub;
  loadLobbies().catch((e) => {
    if (e.message !== '401') {
      showError(`Erro ao carregar: ${e.message}`);
      setStatus('');
    }
  });
}

init();
