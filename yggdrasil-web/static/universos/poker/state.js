// state.js — singleton de estado do cliente + DOM refs + constantes.
//
// Único arquivo que mantém mutação compartilhada. Outros módulos importam
// `state` / `el` e mutam diretamente — princípio do "store global pequeno".
// Vide docs/POKER-MULTIPLAYER.md#state-tracking.

export const STORAGE_KEY = 'yggdrasil-jwt';
export const POLL_MS = 2000;
export const LIST_POLL_MS = 4000;
export const BUY_IN_SEMENTES = 1_000;

export const state = {
  token: localStorage.getItem(STORAGE_KEY),
  userId: null,
  lobbies: [],
  activeLobby: null,
  pollTimer: null,
  listPollTimer: null,
  meSeated: false,
  lastRound: null,
  // Caches anti-flicker: chave = rank+suit concatenado. Quando muda, re-renderiza.
  lastCommunityKey: null,
  lastHoleKey: null,
  // Debounce de refresh de saldo após showdown.
  handEndedAcked: false,
};

export const el = {
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
