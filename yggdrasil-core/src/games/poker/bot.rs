//! Bot AI para pôquer multiplayer — escolha aleatória entre ações legais.
//!
//! Pesos (do task YG-26):
//! - fold = 15%
//! - check (se permitido) = 35%
//! - call = 35% (com `current_bet` zerado, equivale a check)
//! - raise (small fixed amount = big_blind * 2) = 15%
//!
//! Determinismo: `pick_action` recebe um `Rng` injetado, então testes podem
//! fixar a semente. Em produção usar `rand::thread_rng()`.

use game_core::games::poker::PokerAction;
use rand::Rng;

use super::game::PokerTable;
use super::lobby::BOT_USER_ID;

const FOLD_WEIGHT: u32 = 15;
const CHECK_WEIGHT: u32 = 35;
const CALL_WEIGHT: u32 = 35;
const RAISE_WEIGHT: u32 = 15;

/// Quanto vai num raise (em chips). `big_blind * 2` por padrão para um bet
/// simples acima do big blind atual.
fn raise_step(table: &PokerTable) -> u32 {
    table
        .game
        .as_ref()
        .map(|g| g.config.big_blind.saturating_mul(2))
        .unwrap_or(40)
}

/// Escolhe uma ação aleatória legal para o jogador na vez (que se espera ser o bot).
///
/// Se o jogo não está em curso ou o jogador da vez não existe, retorna `Fold`
/// como fallback inerte — o caller (`auto_step_bots`) deve detectar isso e parar.
pub fn pick_action<R: Rng + ?Sized>(table: &PokerTable, rng: &mut R) -> PokerAction {
    let Some(game) = table.game.as_ref() else {
        return PokerAction::Fold;
    };
    let player_idx = game.table.action_position;
    let Some(player) = game.players.get(player_idx) else {
        return PokerAction::Fold;
    };

    let can_check = player.current_bet == game.table.current_bet;
    let raise_amount = raise_step(table);
    let call_to_match = game.table.current_bet.saturating_sub(player.current_bet);
    let can_raise = player.chips > call_to_match.saturating_add(raise_amount);

    // Constrói weighted choices: (action, weight)
    let mut choices: Vec<(PokerAction, u32)> = Vec::with_capacity(4);
    choices.push((PokerAction::Fold, FOLD_WEIGHT));
    if can_check {
        choices.push((PokerAction::Check, CHECK_WEIGHT));
    } else {
        choices.push((PokerAction::Call, CALL_WEIGHT));
    }
    if can_raise {
        choices.push((PokerAction::Raise(raise_amount), RAISE_WEIGHT));
    }

    let total: u32 = choices.iter().map(|(_, w)| *w).sum();
    let mut roll = rng.gen_range(0..total);
    for (action, weight) in &choices {
        if roll < *weight {
            return action.clone();
        }
        roll -= *weight;
    }
    PokerAction::Fold
}

/// Avança bots enquanto o ator atual for o bot. Limite de iterações evita
/// loop infinito caso o engine falhe em avançar o turno.
pub fn auto_step_bots(table: &mut PokerTable) {
    let mut rng = rand::thread_rng();
    auto_step_bots_with_rng(table, &mut rng);
}

/// Variante com RNG injetável para testes determinísticos.
pub fn auto_step_bots_with_rng<R: Rng + ?Sized>(table: &mut PokerTable, rng: &mut R) {
    for _ in 0..32 {
        if table.current_actor.as_deref() != Some(BOT_USER_ID) {
            return;
        }
        let action = pick_action(table, rng);
        if table.act(BOT_USER_ID, action).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::poker::lobby::PokerLobby;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn make_human_vs_bot_table() -> PokerTable {
        let mut table = PokerTable::new(PokerLobby::new("t1", "Mesa Teste"));
        table.lobby.sit(0, "user-a").unwrap();
        // After 1 human, lobby.rebalance_bots adds a bot.
        table.start_hand().unwrap();
        table
    }

    #[test]
    fn pick_action_retorna_acao_valida_em_1000_iteracoes() {
        let table = make_human_vs_bot_table();
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..1000 {
            let action = pick_action(&table, &mut rng);
            match action {
                PokerAction::Fold | PokerAction::Check | PokerAction::Call => {}
                PokerAction::Raise(amount) => assert!(amount > 0),
                PokerAction::AllIn => panic!("AllIn não está no weight table"),
            }
        }
    }

    #[test]
    fn pick_action_sem_jogo_retorna_fold() {
        let table = PokerTable::new(PokerLobby::new("t1", "Mesa Teste"));
        let mut rng = StdRng::seed_from_u64(1);
        assert!(matches!(pick_action(&table, &mut rng), PokerAction::Fold));
    }

    #[test]
    fn auto_step_bots_para_quando_humano_vez_ou_jogo_acaba() {
        let mut table = make_human_vs_bot_table();
        let mut rng = StdRng::seed_from_u64(99);
        auto_step_bots_with_rng(&mut table, &mut rng);
        // Após auto-step, o ator deve ser humano OU o jogo deve ter terminado.
        let state = table.hand_state().unwrap();
        if !state.game_over {
            assert_eq!(state.current_actor.as_deref(), Some("user-a"));
        }
    }

    #[test]
    fn humano_vs_bot_completa_mao_sem_travar() {
        use game_core::games::poker::PokerAction;
        let mut table = make_human_vs_bot_table();
        let mut rng = StdRng::seed_from_u64(7);

        // Loop: avança bot, depois humano joga `call` ou `check`.
        for _ in 0..50 {
            auto_step_bots_with_rng(&mut table, &mut rng);
            let state = match table.hand_state() {
                Some(s) if !s.game_over => s,
                _ => return, // mão terminou — sucesso
            };
            let actor = state.current_actor.clone();
            if actor.as_deref() != Some("user-a") {
                continue;
            }
            let player = state
                .players
                .iter()
                .find(|p| p.user_id == "user-a")
                .unwrap();
            let action = if player.current_bet < state.current_bet {
                PokerAction::Call
            } else {
                PokerAction::Check
            };
            if table.act("user-a", action).is_err() {
                // Pode falhar se o bot encerrou a mão entre auto_step e nosso act
                break;
            }
        }
        let final_state = table.hand_state().unwrap();
        assert!(
            final_state.game_over,
            "mão deveria terminar em até 50 iterações"
        );
    }

    #[test]
    fn pick_action_pode_escolher_raise_quando_ha_chips() {
        // Verifica que raise aparece na distribuição (não é zerado).
        let table = make_human_vs_bot_table();
        let mut rng = StdRng::seed_from_u64(123);
        let mut saw_raise = false;
        for _ in 0..200 {
            if matches!(pick_action(&table, &mut rng), PokerAction::Raise(_)) {
                saw_raise = true;
                break;
            }
        }
        assert!(
            saw_raise,
            "raise deveria aparecer em 200 amostras com bot tendo chips"
        );
    }
}
