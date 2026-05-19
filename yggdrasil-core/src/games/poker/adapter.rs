use chrono::Utc;
use game_core::{
    engine::{Input, Universe},
    games::{
        GameAction, PokerGame,
        poker::{GameConfig, Player, SelectedAction, deck::Card},
    },
    storage::{Storage, WalletManager, schema},
};
use serde::Serialize;
use std::sync::Arc;

use crate::games::YggGame;

/// Saldo inicial de sementes para novos jogadores (alinhado com INITIAL_BALANCE em co-web).
pub const INITIAL_SEMENTES: u64 = 10_000;

const DEFAULT_BUY_IN: u32 = 1_000;

#[derive(Serialize)]
pub struct PokerRender {
    pub players: Vec<Player>,
    pub pot: u32,
    pub current_bet: u32,
    pub community_cards: Vec<Card>,
    pub round: &'static str,
    pub game_over: bool,
    pub winner_message: Option<String>,
    pub is_local_turn: bool,
    pub selected_action: &'static str,
    pub raise_amount: u32,
    pub sementes: u64,
}

/// Yggdrasil adapter wrapping [`game_core::PokerGame`] com buy-in e cash-out em sementes.
pub struct YggPoker {
    pub inner: PokerGame,
    storage: Arc<Storage>,
    pub buy_in_sementes: u32,
    user_id: String,
    cashed_out: bool,
    game_over: bool,
}

impl YggPoker {
    /// Cria uma sessão de Poker debitando sementes da carteira via [`WalletManager`].
    ///
    /// - Se a carteira não existir, inicializa com [`INITIAL_SEMENTES`].
    /// - Retorna `Err("Sem sementes para apostar")` se o saldo for zero.
    pub fn try_new(
        universe: Universe,
        storage: Arc<Storage>,
        user_id: &str,
    ) -> Result<Self, String> {
        let balance = {
            let wallet_opt = storage.get_wallet().map_err(|e| e.to_string())?;
            match wallet_opt {
                None => {
                    let new_wallet = schema::Wallet {
                        user_id: user_id.to_string(),
                        balance: INITIAL_SEMENTES,
                        last_updated: Utc::now().timestamp(),
                    };
                    storage
                        .save_wallet(&new_wallet)
                        .map_err(|e| e.to_string())?;
                    INITIAL_SEMENTES
                }
                Some(w) => w.balance,
            }
        };

        if balance == 0 {
            return Err("Sem sementes para apostar".to_string());
        }

        let buy_in_amount = (DEFAULT_BUY_IN as u64).min(balance) as u32;
        let actual_buy_in = WalletManager::new(&storage)
            .buy_in(user_id, buy_in_amount)
            .map_err(|e| e.to_string())?;

        let config = GameConfig {
            starting_chips: actual_buy_in,
            ..Default::default()
        };

        let mut inner = PokerGame::with_config_and_storage(
            universe,
            config,
            "Você".to_string(),
            storage.clone(),
            user_id.to_string(),
            actual_buy_in,
        );

        inner.add_player("Bot Alice".to_string(), actual_buy_in);
        inner.add_player("Bot Bob".to_string(), actual_buy_in);
        inner.start_hand();

        Ok(Self {
            inner,
            storage,
            buy_in_sementes: actual_buy_in,
            user_id: user_id.to_string(),
            cashed_out: false,
            game_over: false,
        })
    }

    /// Credita as fichas restantes de volta à carteira de sementes.
    pub fn cash_out(&mut self) {
        if self.cashed_out {
            return;
        }
        self.cashed_out = true;

        let remaining = self
            .inner
            .players
            .get(self.inner.local_player_index)
            .map(|p| p.chips)
            .unwrap_or(0);

        let _ = WalletManager::new(&self.storage).cash_out(
            &self.user_id,
            remaining,
            self.buy_in_sementes,
        );
    }

    /// Saldo atual de sementes na carteira.
    pub fn sementes_balance(&self) -> u64 {
        WalletManager::new(&self.storage).get_balance().unwrap_or(0)
    }
}

impl YggGame for YggPoker {
    type State = PokerRender;

    fn render(&self) -> PokerRender {
        PokerRender {
            players: self.inner.players.clone(),
            pot: self.inner.table.pot,
            current_bet: self.inner.table.current_bet,
            community_cards: self.inner.table.community_cards.clone(),
            round: self.inner.round.name(),
            game_over: self.inner.game_over,
            winner_message: self.inner.winner_message.clone(),
            is_local_turn: self.inner.is_local_turn(),
            selected_action: action_name(self.inner.selected_action),
            raise_amount: self.inner.raise_amount,
            sementes: self.sementes_balance(),
        }
    }

    fn tick(&mut self, input: Input) -> GameAction {
        if self.game_over {
            return GameAction::Quit;
        }
        let action = self.inner.tick(input);
        if self.inner.game_over || matches!(action, GameAction::Quit) {
            self.game_over = true;
        }
        action
    }

    fn score(&self) -> u32 {
        self.inner
            .players
            .get(self.inner.local_player_index)
            .map(|p| p.chips)
            .unwrap_or(0)
    }

    fn is_over(&self) -> bool {
        self.game_over
    }
}

fn action_name(action: SelectedAction) -> &'static str {
    match action {
        SelectedAction::Fold => "Fold",
        SelectedAction::Check => "Check",
        SelectedAction::Call => "Call",
        SelectedAction::Raise => "Raise",
        SelectedAction::AllIn => "AllIn",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::{create_poker_universe, storage::schema};
    use std::sync::Arc;
    use tempfile::tempdir;

    fn make_storage(initial_balance: Option<u64>) -> (Arc<Storage>, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let storage = Arc::new(Storage::open(&path).unwrap());
        if let Some(balance) = initial_balance {
            let wallet = schema::Wallet {
                user_id: "test".to_string(),
                balance,
                last_updated: 0,
            };
            storage.save_wallet(&wallet).unwrap();
        }
        (storage, dir)
    }

    fn make_game(balance: u64) -> (YggPoker, tempfile::TempDir) {
        let (storage, dir) = make_storage(Some(balance));
        let game = YggPoker::try_new(create_poker_universe(), storage, "test").unwrap();
        (game, dir)
    }

    #[test]
    fn new_game_with_sufficient_balance() {
        let (game, _dir) = make_game(INITIAL_SEMENTES);
        assert!(!game.is_over());
        assert!(game.score() > 0);
        assert!(game.buy_in_sementes > 0);
    }

    #[test]
    fn refuses_entry_with_zero_balance() {
        let (storage, _dir) = make_storage(Some(0));
        let result = YggPoker::try_new(create_poker_universe(), storage, "test");
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), "Sem sementes para apostar");
    }

    #[test]
    fn auto_initializes_wallet_when_none_exists() {
        let (storage, _dir) = make_storage(None);
        let game = YggPoker::try_new(create_poker_universe(), storage, "test").unwrap();
        assert!(!game.is_over());
        assert_eq!(
            game.sementes_balance(),
            INITIAL_SEMENTES - game.buy_in_sementes as u64
        );
    }

    #[test]
    fn buy_in_debits_from_wallet() {
        let (storage, _dir) = make_storage(Some(INITIAL_SEMENTES));
        let game = YggPoker::try_new(create_poker_universe(), storage.clone(), "test").unwrap();
        let balance = WalletManager::new(&*storage).get_balance().unwrap();
        assert_eq!(balance, INITIAL_SEMENTES - game.buy_in_sementes as u64);
    }

    #[test]
    fn cash_out_credits_chips_back() {
        let (storage, _dir) = make_storage(Some(INITIAL_SEMENTES));
        let mut game = YggPoker::try_new(create_poker_universe(), storage.clone(), "test").unwrap();
        let buy_in = game.buy_in_sementes;
        let remaining = game.score();
        game.cash_out();
        let balance = WalletManager::new(&*storage).get_balance().unwrap();
        assert_eq!(balance, INITIAL_SEMENTES - buy_in as u64 + remaining as u64);
    }

    #[test]
    fn double_cash_out_is_idempotent() {
        let (storage, _dir) = make_storage(Some(INITIAL_SEMENTES));
        let mut game = YggPoker::try_new(create_poker_universe(), storage.clone(), "test").unwrap();
        game.cash_out();
        let balance_after_first = WalletManager::new(&*storage).get_balance().unwrap();
        game.cash_out();
        let balance_after_second = WalletManager::new(&*storage).get_balance().unwrap();
        assert_eq!(balance_after_first, balance_after_second);
    }

    #[test]
    fn render_valid() {
        let (game, _dir) = make_game(INITIAL_SEMENTES);
        let v = serde_json::to_value(game.render()).unwrap();
        assert!(v["players"].is_array());
        assert!(v["pot"].is_number());
        assert!(v["community_cards"].is_array());
        assert!(v["sementes"].is_number());
        assert!(!v["game_over"].as_bool().unwrap());
        assert!(v["selected_action"].is_string());
    }

    #[test]
    fn render_has_three_players() {
        let (game, _dir) = make_game(INITIAL_SEMENTES);
        let v = serde_json::to_value(game.render()).unwrap();
        assert_eq!(v["players"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn quit_ends_game() {
        let (mut game, _dir) = make_game(INITIAL_SEMENTES);
        let action = game.tick(Input::Quit);
        assert_eq!(action, GameAction::Quit);
        assert!(game.is_over());
    }

    #[test]
    fn tick_after_game_over_returns_quit() {
        let (mut game, _dir) = make_game(INITIAL_SEMENTES);
        game.tick(Input::Quit);
        assert_eq!(game.tick(Input::None), GameAction::Quit);
    }

    #[test]
    fn small_balance_buys_in_with_available_amount() {
        let (storage, _dir) = make_storage(Some(500));
        let game = YggPoker::try_new(create_poker_universe(), storage, "test").unwrap();
        assert_eq!(game.buy_in_sementes, 500);
        assert!(game.score() > 0);
    }

    #[test]
    fn initial_sementes_constant_is_ten_thousand() {
        assert_eq!(INITIAL_SEMENTES, 10_000);
    }
}
