//! `PokerTable` — estado de partida multiplayer.
//!
//! Composição de `PokerLobby` (seating) com `game_core::PokerGame` (engine).
//! Cada instância gerencia uma mão: deal → pré-flop → flop → turn → river → showdown.

use game_core::{
    PokerGame, create_poker_universe,
    games::poker::{BettingRound, GameConfig, PlayerStatus, PokerAction, deck::Suit},
};
use serde::Serialize;

use crate::games::poker_lobby::{BOT_USER_ID, PokerLobby, SeatOccupant};

#[derive(Debug, thiserror::Error)]
pub enum PokerTableError {
    #[error("Não é sua vez")]
    NaoEhSuaVez,
    #[error("Ação inválida")]
    AcaoInvalida,
    #[error("Mesa sem jogadores suficientes")]
    MesaSemJogadores,
    #[error("Rodada encerrada")]
    RoundEncerrado,
}

#[derive(Serialize, Clone, Debug)]
pub struct CardView {
    pub rank: String,
    pub suit: String,
}

#[derive(Serialize, Clone)]
pub struct PublicPlayer {
    pub user_id: String,
    pub chips: u32,
    pub current_bet: u32,
    pub is_dealer: bool,
    pub folded: bool,
}

#[derive(Serialize)]
pub struct HandState {
    pub community_cards: Vec<CardView>,
    pub pot: u32,
    pub current_bet: u32,
    pub current_actor: Option<String>,
    pub round: String,
    pub game_over: bool,
    pub winner_message: Option<String>,
    pub players: Vec<PublicPlayer>,
}

pub struct PokerTable {
    pub lobby: PokerLobby,
    pub game: Option<PokerGame>,
    pub current_actor: Option<String>,
    player_map: Vec<String>,
}

impl PokerTable {
    pub fn new(lobby: PokerLobby) -> Self {
        Self {
            lobby,
            game: None,
            current_actor: None,
            player_map: vec![],
        }
    }

    fn occupants(&self) -> Vec<String> {
        self.lobby
            .seats
            .iter()
            .filter_map(|s| match s {
                SeatOccupant::Human { user_id, .. } => Some(user_id.clone()),
                SeatOccupant::Bot => Some(BOT_USER_ID.to_string()),
                SeatOccupant::Empty => None,
            })
            .collect()
    }

    /// Inicia uma nova mão. Requer ≥ 2 ocupantes.
    pub fn start_hand(&mut self) -> Result<(), PokerTableError> {
        let occupants = self.occupants();
        if occupants.len() < 2 {
            return Err(PokerTableError::MesaSemJogadores);
        }

        let universe = create_poker_universe();
        let config = GameConfig::default();
        let chips = config.starting_chips;

        let mut game = PokerGame::with_config(universe, config, occupants[0].clone());
        for occ in &occupants[1..] {
            game.add_player(occ.clone(), chips);
        }
        game.start_hand();

        let action_pos = game.table.action_position;
        self.player_map = occupants;
        self.current_actor = self.player_map.get(action_pos).cloned();
        self.game = Some(game);
        Ok(())
    }

    /// Aplica uma ação do jogador `user_id`. Valida vez e consistência da ação.
    pub fn act(&mut self, user_id: &str, action: PokerAction) -> Result<(), PokerTableError> {
        let game = self.game.as_mut().ok_or(PokerTableError::RoundEncerrado)?;
        if game.game_over {
            return Err(PokerTableError::RoundEncerrado);
        }
        if self.current_actor.as_deref() != Some(user_id) {
            return Err(PokerTableError::NaoEhSuaVez);
        }

        let player_idx = game.table.action_position;
        if matches!(action, PokerAction::Check) {
            let player = game
                .players
                .get(player_idx)
                .ok_or(PokerTableError::AcaoInvalida)?;
            if player.current_bet != game.table.current_bet {
                return Err(PokerTableError::AcaoInvalida);
            }
        }

        game.execute_action(action);

        if game.game_over {
            self.current_actor = None;
        } else {
            let action_pos = game.table.action_position;
            self.current_actor = self.player_map.get(action_pos).cloned();
        }
        Ok(())
    }

    /// Estado público da mão: community cards reveladas, pot, current_actor.
    pub fn hand_state(&self) -> Option<HandState> {
        let game = self.game.as_ref()?;

        let num_visible = match game.round {
            BettingRound::PreFlop => 0,
            BettingRound::Flop => 3,
            BettingRound::Turn => 4,
            BettingRound::River | BettingRound::Showdown => 5,
        };

        let community_cards = game
            .table
            .community_cards
            .iter()
            .take(num_visible)
            .map(card_to_view)
            .collect();

        let players = game
            .players
            .iter()
            .enumerate()
            .map(|(i, p)| PublicPlayer {
                user_id: self.player_map.get(i).cloned().unwrap_or_default(),
                chips: p.chips,
                current_bet: p.current_bet,
                is_dealer: p.is_dealer,
                folded: matches!(p.status, PlayerStatus::Folded),
            })
            .collect();

        Some(HandState {
            community_cards,
            pot: game.table.pot,
            current_bet: game.table.current_bet,
            current_actor: self.current_actor.clone(),
            round: game.round.name().to_string(),
            game_over: game.game_over,
            winner_message: game.winner_message.clone(),
            players,
        })
    }

    /// Cartas privadas do jogador autenticado (hole cards).
    pub fn hole_cards_for(&self, user_id: &str) -> Option<[CardView; 2]> {
        let game = self.game.as_ref()?;
        let player_idx = self.player_map.iter().position(|uid| uid == user_id)?;
        let player = game.players.get(player_idx)?;
        let [c0, c1] = player.hole_cards?;
        Some([card_to_view(&c0), card_to_view(&c1)])
    }
}

fn card_to_view(card: &game_core::games::poker::deck::Card) -> CardView {
    CardView {
        rank: card.rank.symbol().to_string(),
        suit: match card.suit {
            Suit::Hearts => "hearts",
            Suit::Diamonds => "diamonds",
            Suit::Clubs => "clubs",
            Suit::Spades => "spades",
        }
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::poker_lobby::PokerLobby;

    fn make_two_player_table() -> PokerTable {
        let mut table = PokerTable::new(PokerLobby::new("t1", "Mesa Teste"));
        table.lobby.sit(0, "user-a").unwrap();
        // Sitting user-b removes the bot added for user-a
        table.lobby.sit(1, "user-b").unwrap();
        table
    }

    #[test]
    fn start_hand_requires_two_players() {
        let mut table = PokerTable::new(PokerLobby::new("t1", "Mesa Teste"));
        let err = table.start_hand().unwrap_err();
        assert!(matches!(err, PokerTableError::MesaSemJogadores));
    }

    #[test]
    fn start_hand_com_dois_humanos() {
        let mut table = make_two_player_table();
        table.start_hand().unwrap();
        let state = table.hand_state().unwrap();
        assert!(!state.game_over);
        assert_eq!(state.players.len(), 2);
        assert!(state.current_actor.is_some());
    }

    #[test]
    fn acao_fora_da_vez_retorna_nao_eh_sua_vez() {
        let mut table = make_two_player_table();
        table.start_hand().unwrap();
        let state = table.hand_state().unwrap();
        let actor = state.current_actor.unwrap();
        let nao_actor = if actor == "user-a" {
            "user-b"
        } else {
            "user-a"
        };
        let err = table.act(nao_actor, PokerAction::Check).unwrap_err();
        assert!(matches!(err, PokerTableError::NaoEhSuaVez));
    }

    #[test]
    fn fold_encerra_mao_imediatamente() {
        let mut table = make_two_player_table();
        table.start_hand().unwrap();
        let actor = table.current_actor.clone().unwrap();
        table.act(&actor, PokerAction::Fold).unwrap();
        let state = table.hand_state().unwrap();
        assert!(state.game_over);
        assert!(state.winner_message.is_some());
    }

    #[test]
    fn act_sem_jogo_retorna_round_encerrado() {
        let mut table = make_two_player_table();
        let err = table.act("user-a", PokerAction::Check).unwrap_err();
        assert!(matches!(err, PokerTableError::RoundEncerrado));
    }

    #[test]
    fn check_invalido_quando_ha_aposta_retorna_acao_invalida() {
        let mut table = make_two_player_table();
        table.start_hand().unwrap();
        let state = table.hand_state().unwrap();
        // Pre-flop has blinds, so current_bet > 0 for at least one player.
        // Find a player whose current_bet != table.current_bet.
        let pot_before = state.pot;
        let actor = state.current_actor.unwrap();
        let player = state.players.iter().find(|p| p.user_id == actor).unwrap();
        if player.current_bet < state.current_bet {
            // This player must call or fold, not check
            let err = table.act(&actor, PokerAction::Check).unwrap_err();
            assert!(matches!(err, PokerTableError::AcaoInvalida));
        } else {
            // Player can check — not testing invalid check here
            let _ = pot_before; // suppress unused warning
        }
    }

    #[test]
    fn dois_humanos_completam_mao_ate_showdown() {
        let mut table = make_two_player_table();
        table.start_hand().unwrap();

        // Play to completion: call when owed, check when even
        for _ in 0..20 {
            let state = match table.hand_state() {
                Some(s) if !s.game_over => s,
                _ => break,
            };
            let actor = state.current_actor.clone().unwrap();
            let player = state.players.iter().find(|p| p.user_id == actor).unwrap();
            let action = if player.current_bet < state.current_bet {
                PokerAction::Call
            } else {
                PokerAction::Check
            };
            table.act(&actor, action).unwrap();
        }

        let state = table.hand_state().unwrap();
        assert!(state.game_over, "mão deveria terminar em showdown");
        assert!(
            state.winner_message.is_some(),
            "deve haver vencedor declarado"
        );
        assert_eq!(state.round, "Showdown");
    }

    #[test]
    fn hole_cards_retorna_duas_cartas_para_jogador_sentado() {
        let mut table = make_two_player_table();
        table.start_hand().unwrap();
        let cards = table.hole_cards_for("user-a").unwrap();
        assert_eq!(cards.len(), 2);
        assert!(!cards[0].rank.is_empty());
    }

    #[test]
    fn hole_cards_none_para_jogador_ausente() {
        let mut table = make_two_player_table();
        table.start_hand().unwrap();
        assert!(table.hole_cards_for("user-x").is_none());
    }
}
