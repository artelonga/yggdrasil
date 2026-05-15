//! `PokerTable` — estado de partida multiplayer.
//!
//! Composição de `PokerLobby` (seating) com `game_core::PokerGame` (engine).
//! Cada instância gerencia uma mão: deal → pré-flop → flop → turn → river → showdown.

use game_core::{
    PokerGame, create_poker_universe,
    games::poker::{BettingRound, GameConfig, PlayerStatus, PokerAction, deck::Suit},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::games::poker_lobby::{BOT_USER_ID, LobbyError, PokerLobby, SeatOccupant};
use crate::sementes::{Sementes, SementesError};

/// Buy-in fixo por sentar à mesa (YG-27). Em chips 1:1 com sementes.
pub const BUY_IN_SEMENTES: u64 = 1_000;

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

/// Erros ao sentar com buy-in em sementes (YG-27).
#[derive(Debug, thiserror::Error)]
pub enum PokerSitError {
    #[error(transparent)]
    Lobby(#[from] LobbyError),
    #[error(transparent)]
    Sementes(#[from] SementesError),
}

/// Erros ao levantar e cashar out (YG-27).
#[derive(Debug, thiserror::Error)]
pub enum PokerStandError {
    #[error(transparent)]
    Lobby(#[from] LobbyError),
    #[error(transparent)]
    Sementes(#[from] SementesError),
}

#[derive(Serialize, Clone, Debug)]
pub struct CardView {
    pub rank: String,
    pub suit: String,
}

#[derive(Serialize, Clone)]
pub struct PublicPlayer {
    pub user_id: String,
    /// Username resolvido pelo route handler via `user_profiles`. Igual a
    /// `user_id` quando o perfil não existe (anonymous, bot, ou usuário sem
    /// login recente). Frontend faz `p.username || p.user_id`.
    #[serde(default)]
    pub username: String,
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
    pub player_map: Vec<String>,
    /// Chip stack persistido entre mãos. Chave = user_id (humano ou `BOT_USER_ID`).
    /// Buy-in entra aqui no `sit_with_sementes` (YG-27), atualizado após cada mão.
    pub stacks: HashMap<String, u32>,
    /// Quando a mão entrou em `game_over = true`. Usado pelo route handler
    /// para auto-restart após delay — sem isso, o showdown ficava preso
    /// indefinidamente porque `start_hand` só era chamado quando `game.is_none()`.
    pub hand_ended_at: Option<chrono::DateTime<chrono::Utc>>,
    /// ID único da mão atual — `{table_id}-{millis}`. Usado por favoritos para
    /// indexar snapshots de showdown. Regenerado em `start_hand`.
    pub current_hand_id: String,
}

/// Segundos entre o fim de uma mão e o auto-start da próxima. Curto o
/// suficiente para o jogo fluir, longo o suficiente para o vencedor ser
/// visível.
pub const HAND_END_RESTART_DELAY_SECS: i64 = 5;

/// Snapshot serializável de uma `PokerTable` para persistência em SQLite
/// (YG-29). Persiste apenas `lobby` (seating) + `stacks` (chips entre mãos).
/// Mãos em curso (`PokerGame`) NÃO são persistidas — um restart no meio de
/// uma mão é forfeit, mas seats e chip-stacks sobrevivem. Esta é uma escolha
/// pragmática: o engine `PokerGame` do `co/game-core` não é serde-friendly,
/// e o custo de uma mão perdida (≤ algumas dezenas de sementes) é muito
/// menor que o custo de perder buy-ins (1k+ sementes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PokerTableSnapshot {
    pub lobby: PokerLobby,
    pub stacks: HashMap<String, u32>,
}

impl PokerTable {
    pub fn new(lobby: PokerLobby) -> Self {
        Self {
            lobby,
            game: None,
            current_actor: None,
            player_map: vec![],
            stacks: HashMap::new(),
            hand_ended_at: None,
            current_hand_id: String::new(),
        }
    }

    /// `true` se a mão atual terminou há ≥ `HAND_END_RESTART_DELAY_SECS`.
    /// Route handlers usam isso para decidir quando substituir o `game` por uma
    /// nova mão automaticamente.
    pub fn should_auto_restart(&self) -> bool {
        let Some(t) = self.hand_ended_at else {
            return false;
        };
        (chrono::Utc::now() - t).num_seconds() >= HAND_END_RESTART_DELAY_SECS
    }

    /// Serializa a mesa para persistência (YG-29). Captura `lobby` + `stacks`;
    /// `game` em curso é descartado (vide doc de [`PokerTableSnapshot`]).
    /// Faz snapshot dos chips do engine para `stacks` antes — assim mesmo que
    /// uma mão esteja a meio caminho, os chips refletem o estado atual.
    pub fn to_snapshot(&mut self) -> PokerTableSnapshot {
        self.snapshot_stacks_from_game();
        PokerTableSnapshot {
            lobby: self.lobby.clone(),
            stacks: self.stacks.clone(),
        }
    }

    /// Restaura uma mesa a partir de um snapshot persistido (YG-29).
    /// `game` fica `None` — qualquer mão em curso no momento do snapshot é
    /// considerada forfeit; a próxima chamada a `start_hand` (acionada por
    /// `GET /hand`) reabre o jogo.
    pub fn from_snapshot(snap: PokerTableSnapshot) -> Self {
        Self {
            lobby: snap.lobby,
            game: None,
            current_actor: None,
            player_map: vec![],
            stacks: snap.stacks,
            hand_ended_at: None,
            current_hand_id: String::new(),
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

    /// Saldo de chips de um usuário (humano ou bot). Zero se nunca sentou.
    pub fn stack_of(&self, user_id: &str) -> u32 {
        self.stacks.get(user_id).copied().unwrap_or(0)
    }

    /// Inicia uma nova mão. Requer ≥ 2 ocupantes com stack > 0.
    ///
    /// Para jogadores ainda não tracked em `stacks` (ex.: testes legados que
    /// usam `lobby.sit` direto, sem buy-in), o stack é auto-inicializado com
    /// `GameConfig::default().starting_chips`. Callers do YG-27
    /// (`sit_with_sementes`) populam `stacks` explicitamente com o buy-in.
    pub fn start_hand(&mut self) -> Result<(), PokerTableError> {
        let occupants = self.occupants();
        let default_stack = GameConfig::default().starting_chips;
        for uid in &occupants {
            self.stacks.entry(uid.clone()).or_insert(default_stack);
        }
        let active: Vec<(String, u32)> = occupants
            .into_iter()
            .filter_map(|uid| {
                let stack = self.stacks.get(&uid).copied().unwrap_or(0);
                if stack > 0 { Some((uid, stack)) } else { None }
            })
            .collect();
        if active.len() < 2 {
            return Err(PokerTableError::MesaSemJogadores);
        }

        let universe = create_poker_universe();
        let config = GameConfig {
            starting_chips: active[0].1,
            ..Default::default()
        };

        let mut game = PokerGame::with_config(universe, config, active[0].0.clone());
        for (uid, stack) in &active[1..] {
            game.add_player(uid.clone(), *stack);
        }
        game.start_hand();

        let action_pos = game.table.action_position;
        self.player_map = active.into_iter().map(|(uid, _)| uid).collect();
        self.current_actor = self.player_map.get(action_pos).cloned();
        self.game = Some(game);
        self.hand_ended_at = None;
        // ID único da mão para indexar snapshots de favorito.
        self.current_hand_id = format!(
            "{}-{}",
            self.lobby.id,
            chrono::Utc::now().timestamp_millis()
        );
        Ok(())
    }

    /// Senta com buy-in em sementes (YG-27). Debita BUY_IN_SEMENTES da carteira
    /// antes de ocupar o assento; se a debitação falhar, o assento não é tocado.
    /// Se o lobby recusar (assento ocupado, já sentado, etc.), a debitação é
    /// revertida via `creditar`.
    pub fn sit_with_sementes(
        &mut self,
        seat: usize,
        user_id: &str,
        sementes: &Sementes,
    ) -> Result<(), PokerSitError> {
        sementes.debitar(user_id, BUY_IN_SEMENTES)?;
        if let Err(e) = self.lobby.sit(seat, user_id) {
            // Refund — sit não chegou a acontecer.
            let _ = sementes.creditar(user_id, BUY_IN_SEMENTES);
            return Err(e.into());
        }
        self.stacks
            .insert(user_id.to_string(), BUY_IN_SEMENTES as u32);
        self.sync_bot_stack();
        Ok(())
    }

    /// Levanta da mesa e credita o stack remanescente de volta em sementes (YG-27).
    /// Se está no meio de uma mão, faz fold automático antes.
    pub fn stand_with_sementes(
        &mut self,
        user_id: &str,
        sementes: &Sementes,
    ) -> Result<(), PokerStandError> {
        // Fold se for a vez do usuário e o jogo está em curso.
        if self.current_actor.as_deref() == Some(user_id) {
            let _ = self.act(user_id, PokerAction::Fold);
        }
        // Captura chips atuais do engine antes de levantar.
        self.snapshot_stacks_from_game();
        let remaining = self.stacks.remove(user_id).unwrap_or(0);
        self.lobby.stand(user_id)?;
        if remaining > 0 {
            sementes.creditar(user_id, remaining as u64)?;
        }
        self.sync_bot_stack();
        Ok(())
    }

    /// Garante que o bot tem stack inicializado quando está sentado, e que é
    /// removido de `stacks` quando sai. Idempotente.
    fn sync_bot_stack(&mut self) {
        let bot_seated = self
            .lobby
            .seats
            .iter()
            .any(|s| matches!(s, SeatOccupant::Bot));
        if bot_seated {
            self.stacks
                .entry(BOT_USER_ID.to_string())
                .or_insert(BUY_IN_SEMENTES as u32);
        } else {
            self.stacks.remove(BOT_USER_ID);
        }
    }

    /// Copia chips do `PokerGame` para `stacks`. Chamado após cada mão
    /// (ou antes de cash-out) para que o saldo persista entre mãos.
    fn snapshot_stacks_from_game(&mut self) {
        if let Some(game) = self.game.as_ref() {
            for (i, p) in game.players.iter().enumerate() {
                if let Some(uid) = self.player_map.get(i) {
                    self.stacks.insert(uid.clone(), p.chips);
                }
            }
        }
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
            // Marca o instante do fim da mão se ainda não foi marcado. O route
            // handler usa isso para auto-restart após delay.
            if self.hand_ended_at.is_none() {
                self.hand_ended_at = Some(chrono::Utc::now());
            }
        } else {
            let action_pos = game.table.action_position;
            self.current_actor = self.player_map.get(action_pos).cloned();
        }
        // Snapshot stacks após cada ação para que o saldo flutue corretamente
        // — necessário para YG-27 (cash-out a qualquer momento).
        self.snapshot_stacks_from_game();
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
            .map(|(i, p)| {
                let uid = self.player_map.get(i).cloned().unwrap_or_default();
                PublicPlayer {
                    username: uid.clone(), // default; route handler sobrescreve com lookup
                    user_id: uid,
                    chips: p.chips,
                    current_bet: p.current_bet,
                    is_dealer: p.is_dealer,
                    folded: matches!(p.status, PlayerStatus::Folded),
                }
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

    // ── YG-27: buy-in / cash-out em sementes ───────────────────────────────

    use crate::sementes::Sementes;
    use game_core::storage::Storage;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn make_sementes_com_saldo(user: &str, saldo: u64) -> (Sementes, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let storage = Arc::new(Storage::open(&path).unwrap());
        let s = Sementes::new(storage);
        if saldo > 0 {
            s.creditar(user, saldo).unwrap();
        }
        (s, dir)
    }

    #[test]
    fn sit_with_sementes_debita_buy_in_e_inicializa_stack() {
        let mut table = PokerTable::new(PokerLobby::new("t1", "Mesa Teste"));
        let (sementes, _dir) = make_sementes_com_saldo("alice", 5_000);
        table.sit_with_sementes(0, "alice", &sementes).unwrap();
        assert_eq!(sementes.saldo("alice").unwrap(), 5_000 - BUY_IN_SEMENTES);
        assert_eq!(table.stack_of("alice"), BUY_IN_SEMENTES as u32);
    }

    #[test]
    fn sit_with_sementes_saldo_insuficiente_recusa() {
        let mut table = PokerTable::new(PokerLobby::new("t1", "Mesa Teste"));
        let (sementes, _dir) = make_sementes_com_saldo("alice", 100);
        let err = table.sit_with_sementes(0, "alice", &sementes).unwrap_err();
        assert!(matches!(
            err,
            PokerSitError::Sementes(SementesError::SaldoInsuficiente)
        ));
        // Não deve ter debitado nem sentado.
        assert_eq!(sementes.saldo("alice").unwrap(), 100);
        assert_eq!(table.stack_of("alice"), 0);
    }

    #[test]
    fn sit_with_sementes_assento_ocupado_devolve_buy_in() {
        let mut table = PokerTable::new(PokerLobby::new("t1", "Mesa Teste"));
        let (sementes, _dir) = make_sementes_com_saldo("alice", 5_000);
        sementes.creditar("bob", 5_000).unwrap();
        table.sit_with_sementes(0, "alice", &sementes).unwrap();
        // Bob tenta sentar no mesmo assento.
        let saldo_bob_antes = sementes.saldo("bob").unwrap();
        let err = table.sit_with_sementes(0, "bob", &sementes).unwrap_err();
        assert!(matches!(err, PokerSitError::Lobby(LobbyError::SeatTaken)));
        // Bob não foi debitado (refund executado).
        assert_eq!(sementes.saldo("bob").unwrap(), saldo_bob_antes);
    }

    #[test]
    fn stand_with_sementes_credita_stack_remanescente() {
        let mut table = PokerTable::new(PokerLobby::new("t1", "Mesa Teste"));
        let (sementes, _dir) = make_sementes_com_saldo("alice", 5_000);
        table.sit_with_sementes(0, "alice", &sementes).unwrap();
        let saldo_apos_sit = sementes.saldo("alice").unwrap();
        table.stand_with_sementes("alice", &sementes).unwrap();
        // Stand devolveu o buy-in (alice nunca jogou uma mão).
        assert_eq!(
            sementes.saldo("alice").unwrap(),
            saldo_apos_sit + BUY_IN_SEMENTES
        );
    }

    #[test]
    fn ciclo_sit_stand_preserva_soma_de_sementes() {
        let mut table = PokerTable::new(PokerLobby::new("t1", "Mesa Teste"));
        let (sementes, _dir) = make_sementes_com_saldo("alice", 5_000);
        let inicial = sementes.saldo("alice").unwrap();
        table.sit_with_sementes(0, "alice", &sementes).unwrap();
        table.stand_with_sementes("alice", &sementes).unwrap();
        assert_eq!(sementes.saldo("alice").unwrap(), inicial);
    }

    #[test]
    fn bot_recebe_stack_quando_humano_solo_senta() {
        let mut table = PokerTable::new(PokerLobby::new("t1", "Mesa Teste"));
        let (sementes, _dir) = make_sementes_com_saldo("alice", 5_000);
        table.sit_with_sementes(0, "alice", &sementes).unwrap();
        // Lobby auto-adicionou o bot — PokerTable deve ter dado stack a ele.
        assert_eq!(table.stack_of(BOT_USER_ID), BUY_IN_SEMENTES as u32);
    }

    // ── YG-29: snapshot/restore para persistência em SQLite ───────────────

    #[test]
    fn snapshot_round_trip_preserva_lobby_e_stacks() {
        let mut table = PokerTable::new(PokerLobby::new("t1", "Mesa Teste"));
        let (sementes, _dir) = make_sementes_com_saldo("alice", 5_000);
        sementes.creditar("bob", 5_000).unwrap();
        table.sit_with_sementes(0, "alice", &sementes).unwrap();
        table.sit_with_sementes(3, "bob", &sementes).unwrap();

        let snap = table.to_snapshot();
        let json = serde_json::to_string(&snap).unwrap();
        let restored_snap: PokerTableSnapshot = serde_json::from_str(&json).unwrap();
        let restored = PokerTable::from_snapshot(restored_snap);

        assert_eq!(restored.lobby.id, "t1");
        assert_eq!(restored.lobby.name, "Mesa Teste");
        assert_eq!(restored.lobby.human_count(), 2);
        assert_eq!(restored.stack_of("alice"), BUY_IN_SEMENTES as u32);
        assert_eq!(restored.stack_of("bob"), BUY_IN_SEMENTES as u32);
        // Mãos em curso são forfeit — game sempre None após restore.
        assert!(restored.game.is_none());
    }

    #[test]
    fn snapshot_preserva_seats_por_indice() {
        // Heads-up table com alice no seat 0 e bot no seat 1.
        let mut table = PokerTable::new(PokerLobby::with_max_seats("hu", "Heads-Up", 2));
        let (sementes, _dir) = make_sementes_com_saldo("alice", 5_000);
        table.sit_with_sementes(0, "alice", &sementes).unwrap();

        let snap = table.to_snapshot();
        let restored = PokerTable::from_snapshot(snap);

        assert_eq!(restored.lobby.max_seats, 2);
        assert_eq!(restored.lobby.seats.len(), 2);
        // Alice no seat 0
        assert!(matches!(
            &restored.lobby.seats[0],
            crate::games::poker_lobby::SeatOccupant::Human { user_id, .. } if user_id == "alice"
        ));
        // Bot no seat 1 (auto-spawn quando há 1 humano).
        assert!(matches!(
            &restored.lobby.seats[1],
            crate::games::poker_lobby::SeatOccupant::Bot
        ));
        assert_eq!(restored.stack_of(BOT_USER_ID), BUY_IN_SEMENTES as u32);
    }

    #[test]
    fn bot_perde_stack_quando_segundo_humano_senta() {
        let mut table = PokerTable::new(PokerLobby::new("t1", "Mesa Teste"));
        let (sementes, _dir) = make_sementes_com_saldo("alice", 5_000);
        sementes.creditar("bob", 5_000).unwrap();
        table.sit_with_sementes(0, "alice", &sementes).unwrap();
        assert_eq!(table.stack_of(BOT_USER_ID), BUY_IN_SEMENTES as u32);
        table.sit_with_sementes(1, "bob", &sementes).unwrap();
        // Bot foi deslocado pelo bob.
        assert_eq!(table.stack_of(BOT_USER_ID), 0);
    }
}
