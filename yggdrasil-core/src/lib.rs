//! Yggdrasil core — modelos de universo, lobby, e adaptadores para `game-core`.
//!
//! Esse crate é o núcleo de domínio do Yggdrasil. Ele depende de
//! [`game_core`] (do projeto `co`) para reaproveitar engine 2D, sessões,
//! plugin loader e wallet.
//!
//! # Estrutura planejada
//!
//! - `lobby` — universo central com portais para os jogos (YG-2, YG-4, YG-5).
//! - `games` — adaptadores que wrapeiam `SnakeGame`, `TetrisGame`,
//!   `InvadersGame`, `PokerGame` (YG-6 .. YG-9).
//! - `currency` — sementes (renomeação semântica do `WalletManager`) (YG-10).
//!
//! Nenhum módulo está implementado ainda. Esse crate compila vazio em
//! `v0.0.1` — cada YG-<n> preenche uma parte.

/// Re-exports convenientes do engine. Usar `yggdrasil_core::engine::Universe`
/// em vez de `game_core::engine::Universe` em código de aplicação Yggdrasil.
pub mod engine {
    pub use game_core::engine::*;
}

/// Re-exports dos jogos do `game-core`. Adaptadores Yggdrasil-específicos
/// virão em `yggdrasil_core::games` (YG-6+).
pub mod upstream_games {
    pub use game_core::games::*;
}

pub mod games;
pub mod lobby;
