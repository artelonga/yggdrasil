//! Multiplayer poker — auth-gated routes split by responsibility.
//!
//! - `state`         — `PokerState` lifecycle (seeding, persistence boot)
//! - `routes`        — HTTP handlers (list, get, sit, stand, hand, hole-cards, action)
//! - `chip_flow`     — sementes ↔ table error mapping
//! - `serialization` — waiting state and other JSON helpers
//! - `tests`         — integration tests (in-process, no network ports)

pub mod chip_flow;
pub mod routes;
pub mod serialization;
pub mod state;

#[cfg(test)]
mod tests;

pub use routes::{get_hand, get_hole_cards, get_lobby, list_lobbies, post_action, sit, stand};
pub use state::PokerState;
