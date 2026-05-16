//! `poker_engine` — fronteira única de importação do engine `game_core::poker`.
//!
//! Todo módulo do subuniverso poker (lobby, table, bot, routes) **importa
//! daqui**, nunca direto de `game_core`. Isso mantém a fronteira do
//! subuniverso explícita num só lugar: se o engine quebrar um nome ou
//! mudar o caminho, só este arquivo precisa atualizar.
//!
//! Convenção:
//! - **Tipos do engine de pôquer** → re-exportados aqui via `pub use`.
//! - **Storage, Wallet, Universe genérico** → seguem importados direto de
//!   `game_core` nos call-sites (não são parte do "vocabulário de pôquer").
//!
//! Vide [`docs/POKER-MULTIPLAYER.md`](../../../../docs/POKER-MULTIPLAYER.md#onde-está-a-lógica-do-jogo)
//! para o mapa completo de camadas.

pub use game_core::PokerGame;
pub use game_core::create_poker_universe;
pub use game_core::games::poker::{
    BettingRound, GameConfig, Player, PlayerStatus, PokerAction, SelectedAction,
    deck::{Card, Suit},
};
