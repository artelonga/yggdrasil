//! Multiplayer poker — auth-gated routes split by responsibility.
//!
//! - `state`         — `PokerState` lifecycle (seeding, persistence boot)
//! - `routes`        — HTTP handlers (list, get, sit, stand, hand, hole-cards, action)
//! - `chip_flow`     — sementes ↔ table error mapping
//! - `serialization` — waiting state and other JSON helpers
//! - `lobby_event`   — wire format dos eventos pushados pelo `/stream` (YG-28)
//! - `ws`            — handler do WebSocket `/stream` (live updates, YG-28)
//! - `tests`         — integration tests (in-process, no network ports)

pub mod chip_flow;
pub mod lobby_event;
pub mod routes;
pub mod serialization;
pub mod state;
pub mod ws;

#[cfg(test)]
mod tests;

pub use routes::{get_hand, get_hole_cards, get_lobby, list_lobbies, post_action, sit, stand};
pub use state::PokerState;
pub use ws::stream_handler;
