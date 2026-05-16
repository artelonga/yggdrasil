// cards.js — renderização pura de cartas. Sem estado, sem IO.
//
// Reusável para qualquer universo de cartas no futuro. Vide
// docs/POKER-MULTIPLAYER.md#mapa-de-módulos-do-cliente.

const SUIT_SYMBOLS = { hearts: '♥', diamonds: '♦', clubs: '♣', spades: '♠' };
const RED_SUITS = new Set(['hearts', 'diamonds']);

export function cardEl(card) {
  const div = document.createElement('div');
  div.className = 'playing-card' + (RED_SUITS.has(card.suit) ? ' red' : '');
  div.innerHTML = `<span>${card.rank}</span><span class="card-suit">${SUIT_SYMBOLS[card.suit] || card.suit}</span>`;
  return div;
}

export function cardBackEl() {
  const div = document.createElement('div');
  div.className = 'card-back';
  return div;
}
