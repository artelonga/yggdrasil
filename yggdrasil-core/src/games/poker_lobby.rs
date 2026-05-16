//! Multiplayer poker lobby state.
//!
//! Two-phase rollout: this module owns *seating* (who is at which table). Card
//! play and betting actions will plug in later by composing this with
//! `game_core::PokerGame`. For now a "lobby" is just a table with 6 seats and
//! a bot-presence rule:
//!
//! - 0 humans → 0 bots (lobby is dormant)
//! - exactly 1 human → 1 bot fills the first empty seat (always have an opponent)
//! - 2+ humans → no bots (multiplayer encouraged)
//!
//! **Camada**: seating. Não conhece cartas, apostas ou HTTP. Vide
//! [`docs/POKER-MULTIPLAYER.md`](../../../../docs/POKER-MULTIPLAYER.md#camadas).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const SEATS_PER_LOBBY: usize = 6;
pub const BOT_USER_ID: &str = "bot:carvalho";
pub const BOT_DISPLAY_NAME: &str = "Bot Carvalho";

/// Variantes de tamanho de mesa (YG-37). 6 = cash game padrão; 2 = heads-up.
/// Outros valores são aceitos para futuras configurações.

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SeatOccupant {
    Empty,
    Human {
        user_id: String,
        sat_at: DateTime<Utc>,
    },
    Bot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PokerLobby {
    pub id: String,
    pub name: String,
    pub seats: Vec<SeatOccupant>,
    /// Capacidade máxima — `Vec::len(seats)`. Mantida pública para serialização
    /// no JSON do lobby. YG-37: variantes podem reduzir (heads-up = 2).
    pub max_seats: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum LobbyError {
    #[error("Assento inválido (índice {0})")]
    InvalidSeat(usize),
    #[error("Assento ocupado")]
    SeatTaken,
    #[error("Você já está em outro assento desta mesa")]
    AlreadySeated,
    #[error("Você não está sentado nesta mesa")]
    NotSeated,
}

impl PokerLobby {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self::with_max_seats(id, name, SEATS_PER_LOBBY)
    }

    /// Construtor com tamanho de mesa customizado (YG-37). Usado para
    /// variantes como `poker/heads-up` (max_seats = 2).
    pub fn with_max_seats(
        id: impl Into<String>,
        name: impl Into<String>,
        max_seats: usize,
    ) -> Self {
        let max = max_seats.max(2); // mínimo 2 — pôquer precisa de oponente
        Self {
            id: id.into(),
            name: name.into(),
            seats: vec![SeatOccupant::Empty; max],
            max_seats: max,
        }
    }

    pub fn human_count(&self) -> usize {
        self.seats
            .iter()
            .filter(|s| matches!(s, SeatOccupant::Human { .. }))
            .count()
    }

    pub fn bot_count(&self) -> usize {
        self.seats
            .iter()
            .filter(|s| matches!(s, SeatOccupant::Bot))
            .count()
    }

    fn human_seat(&self, user_id: &str) -> Option<usize> {
        self.seats.iter().position(|s| match s {
            SeatOccupant::Human { user_id: id, .. } => id == user_id,
            _ => false,
        })
    }

    pub fn sit(&mut self, seat: usize, user_id: &str) -> Result<(), LobbyError> {
        if seat >= self.max_seats {
            return Err(LobbyError::InvalidSeat(seat));
        }
        if self.human_seat(user_id).is_some() {
            return Err(LobbyError::AlreadySeated);
        }
        match self.seats[seat] {
            SeatOccupant::Human { .. } => return Err(LobbyError::SeatTaken),
            SeatOccupant::Bot | SeatOccupant::Empty => {}
        }
        self.seats[seat] = SeatOccupant::Human {
            user_id: user_id.to_string(),
            sat_at: Utc::now(),
        };
        self.rebalance_bots();
        Ok(())
    }

    pub fn stand(&mut self, user_id: &str) -> Result<(), LobbyError> {
        let idx = self.human_seat(user_id).ok_or(LobbyError::NotSeated)?;
        self.seats[idx] = SeatOccupant::Empty;
        self.rebalance_bots();
        Ok(())
    }

    /// Enforce the bot-presence rule. Called after every sit/stand.
    fn rebalance_bots(&mut self) {
        let target_bots = match self.human_count() {
            0 => 0,
            1 => 1,
            _ => 0,
        };
        let current_bots = self.bot_count();
        if current_bots == target_bots {
            return;
        }
        if current_bots > target_bots {
            // Remove bots starting from the highest seat.
            let mut to_remove = current_bots - target_bots;
            for seat in self.seats.iter_mut().rev() {
                if to_remove == 0 {
                    break;
                }
                if matches!(seat, SeatOccupant::Bot) {
                    *seat = SeatOccupant::Empty;
                    to_remove -= 1;
                }
            }
        } else {
            // Add bots to the first empty seats.
            let mut to_add = target_bots - current_bots;
            for seat in self.seats.iter_mut() {
                if to_add == 0 {
                    break;
                }
                if matches!(seat, SeatOccupant::Empty) {
                    *seat = SeatOccupant::Bot;
                    to_add -= 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_lobby_has_six_empty_seats_and_no_bot() {
        let l = PokerLobby::new("t1", "Mesa Carvalho");
        assert_eq!(l.seats.len(), SEATS_PER_LOBBY);
        assert_eq!(l.human_count(), 0);
        assert_eq!(l.bot_count(), 0);
    }

    #[test]
    fn one_human_seats_spawns_one_bot() {
        let mut l = PokerLobby::new("t1", "Mesa Carvalho");
        l.sit(2, "user-a").unwrap();
        assert_eq!(l.human_count(), 1);
        assert_eq!(l.bot_count(), 1);
    }

    #[test]
    fn second_human_removes_bot() {
        let mut l = PokerLobby::new("t1", "Mesa Carvalho");
        l.sit(0, "user-a").unwrap();
        l.sit(3, "user-b").unwrap();
        assert_eq!(l.human_count(), 2);
        assert_eq!(l.bot_count(), 0);
    }

    #[test]
    fn standing_back_to_one_human_respawns_bot() {
        let mut l = PokerLobby::new("t1", "Mesa Carvalho");
        l.sit(0, "user-a").unwrap();
        l.sit(3, "user-b").unwrap();
        assert_eq!(l.bot_count(), 0);
        l.stand("user-b").unwrap();
        assert_eq!(l.human_count(), 1);
        assert_eq!(l.bot_count(), 1);
    }

    #[test]
    fn last_human_leaving_clears_bot() {
        let mut l = PokerLobby::new("t1", "Mesa Carvalho");
        l.sit(0, "user-a").unwrap();
        l.stand("user-a").unwrap();
        assert_eq!(l.human_count(), 0);
        assert_eq!(l.bot_count(), 0);
    }

    #[test]
    fn cannot_sit_in_occupied_human_seat() {
        let mut l = PokerLobby::new("t1", "Mesa Carvalho");
        l.sit(0, "user-a").unwrap();
        let err = l.sit(0, "user-b").unwrap_err();
        assert!(matches!(err, LobbyError::SeatTaken));
    }

    #[test]
    fn sitting_in_bot_seat_displaces_bot() {
        let mut l = PokerLobby::new("t1", "Mesa Carvalho");
        l.sit(0, "user-a").unwrap();
        let bot_seat = l
            .seats
            .iter()
            .position(|s| matches!(s, SeatOccupant::Bot))
            .unwrap();
        l.sit(bot_seat, "user-b").unwrap();
        assert_eq!(l.human_count(), 2);
        assert_eq!(l.bot_count(), 0);
    }

    #[test]
    fn cannot_sit_twice() {
        let mut l = PokerLobby::new("t1", "Mesa Carvalho");
        l.sit(0, "user-a").unwrap();
        let err = l.sit(2, "user-a").unwrap_err();
        assert!(matches!(err, LobbyError::AlreadySeated));
    }

    #[test]
    fn out_of_range_seat_errors() {
        let mut l = PokerLobby::new("t1", "Mesa Carvalho");
        let err = l.sit(99, "user-a").unwrap_err();
        assert!(matches!(err, LobbyError::InvalidSeat(_)));
    }

    #[test]
    fn heads_up_lobby_recusa_assento_3() {
        // YG-37: variante poker/heads-up usa with_max_seats(2). Assento 2+ inválido.
        let mut l = PokerLobby::with_max_seats("hu", "Heads-Up", 2);
        assert_eq!(l.seats.len(), 2);
        assert_eq!(l.max_seats, 2);
        l.sit(0, "alice").unwrap();
        l.sit(1, "bob").unwrap();
        let err = l.sit(2, "clara").unwrap_err();
        assert!(matches!(err, LobbyError::InvalidSeat(2)));
    }

    #[test]
    fn with_max_seats_clamps_to_minimum_2() {
        // Não faz sentido mesa de pôquer com 0 ou 1 assento.
        let l = PokerLobby::with_max_seats("hu", "Heads-Up", 1);
        assert_eq!(l.seats.len(), 2);
        assert_eq!(l.max_seats, 2);
    }
}
