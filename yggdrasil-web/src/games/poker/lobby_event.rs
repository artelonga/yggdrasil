//! Wire format dos eventos pushados pelo WebSocket `/stream` (YG-28).
//!
//! O engine (`yggdrasil-core`) emite [`TableEvent`] num `broadcast::Sender`
//! por mesa. O `/stream` mapeia esses eventos para [`LobbyEvent`] — o contrato
//! JSON consumido pelo cliente, com tag `type` e os nomes que o frontend espera
//! (`hand_started`, `actor_changed`, `player_acted`, `hand_ended`).

use serde::Serialize;
use yggdrasil_core::games::poker::events::TableEvent;

use super::state::PokerState;

/// Evento JSON enviado ao cliente. Serializa como `{"type": "...", ...}`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LobbyEvent {
    /// Uma nova mão começou.
    HandStarted {
        lobby_id: String,
        player_count: usize,
    },
    /// A vez passou para outro jogador (ou ninguém, ao fim da mão).
    ActorChanged {
        lobby_id: String,
        actor: Option<String>,
    },
    /// Um jogador agiu (`fold`/`check`/`call`/`raise`/`all_in`).
    PlayerActed {
        lobby_id: String,
        user_id: String,
        action: String,
    },
    /// A mão terminou.
    HandEnded {
        lobby_id: String,
        winner_message: Option<String>,
    },
}

/// Lê o `current_actor` atual de uma mesa (pode ser `None` fora de mão).
fn current_actor(state: &PokerState, lobby_id: &str) -> Option<String> {
    let tables = state.tables.lock().unwrap();
    tables
        .iter()
        .find(|t| t.lobby.id == lobby_id)
        .and_then(|t| t.current_actor.clone())
}

/// Traduz um [`TableEvent`] do engine para zero ou mais [`LobbyEvent`]s de fio.
///
/// `hand_started` e `player_acted` carregam consigo um `actor_changed`
/// derivado do `current_actor` da mesa (que o engine já atualizou antes de
/// emitir o evento), para que o cliente saiba de quem é a vez sem refetch.
/// `seated` não tem correspondente no contrato YG-28 e é ignorado.
pub fn map_event(ev: &TableEvent, state: &PokerState) -> Vec<LobbyEvent> {
    match ev {
        TableEvent::HandStarted {
            table_id,
            player_count,
        } => vec![
            LobbyEvent::HandStarted {
                lobby_id: table_id.clone(),
                player_count: *player_count,
            },
            LobbyEvent::ActorChanged {
                lobby_id: table_id.clone(),
                actor: current_actor(state, table_id),
            },
        ],
        TableEvent::ActionTaken {
            table_id,
            user_id,
            action,
        } => vec![
            LobbyEvent::PlayerActed {
                lobby_id: table_id.clone(),
                user_id: user_id.clone(),
                action: action.clone(),
            },
            LobbyEvent::ActorChanged {
                lobby_id: table_id.clone(),
                actor: current_actor(state, table_id),
            },
        ],
        TableEvent::HandEnded {
            table_id,
            winner_message,
        } => vec![LobbyEvent::HandEnded {
            lobby_id: table_id.clone(),
            winner_message: winner_message.clone(),
        }],
        TableEvent::Seated { .. } => vec![],
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use game_core::storage::Storage;
    use yggdrasil_core::sementes::Sementes;

    use super::*;

    fn state() -> PokerState {
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(Storage::open(&dir.path().join("t.db")).unwrap());
        PokerState::new("s".to_string(), Arc::new(Sementes::new(storage)))
    }

    #[test]
    fn serializa_com_tag_type_snake_case() {
        let json = serde_json::to_value(LobbyEvent::HandStarted {
            lobby_id: "carvalho".into(),
            player_count: 2,
        })
        .unwrap();
        assert_eq!(json["type"], "hand_started");
        assert_eq!(json["lobby_id"], "carvalho");
        assert_eq!(json["player_count"], 2);

        let acted = serde_json::to_value(LobbyEvent::PlayerActed {
            lobby_id: "carvalho".into(),
            user_id: "user-a".into(),
            action: "raise".into(),
        })
        .unwrap();
        assert_eq!(acted["type"], "player_acted");

        let ended = serde_json::to_value(LobbyEvent::HandEnded {
            lobby_id: "carvalho".into(),
            winner_message: None,
        })
        .unwrap();
        assert_eq!(ended["type"], "hand_ended");
        assert!(ended["winner_message"].is_null());

        let actor = serde_json::to_value(LobbyEvent::ActorChanged {
            lobby_id: "carvalho".into(),
            actor: Some("user-a".into()),
        })
        .unwrap();
        assert_eq!(actor["type"], "actor_changed");
        assert_eq!(actor["actor"], "user-a");
    }

    #[test]
    fn hand_started_deriva_actor_changed() {
        let st = state();
        let evs = map_event(
            &TableEvent::HandStarted {
                table_id: "carvalho".into(),
                player_count: 2,
            },
            &st,
        );
        assert_eq!(evs.len(), 2);
        assert!(matches!(evs[0], LobbyEvent::HandStarted { .. }));
        assert!(matches!(evs[1], LobbyEvent::ActorChanged { .. }));
    }

    #[test]
    fn action_taken_vira_player_acted_mais_actor_changed() {
        let st = state();
        let evs = map_event(
            &TableEvent::ActionTaken {
                table_id: "carvalho".into(),
                user_id: "user-a".into(),
                action: "call".into(),
            },
            &st,
        );
        assert_eq!(evs.len(), 2);
        assert!(matches!(
            &evs[0],
            LobbyEvent::PlayerActed { action, .. } if action == "call"
        ));
        assert!(matches!(evs[1], LobbyEvent::ActorChanged { .. }));
    }

    #[test]
    fn hand_ended_e_um_evento_so() {
        let st = state();
        let evs = map_event(
            &TableEvent::HandEnded {
                table_id: "carvalho".into(),
                winner_message: Some("user-a venceu".into()),
            },
            &st,
        );
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], LobbyEvent::HandEnded { .. }));
    }

    #[test]
    fn seated_nao_tem_correspondente() {
        let st = state();
        let evs = map_event(
            &TableEvent::Seated {
                table_id: "carvalho".into(),
                user_id: "user-a".into(),
                seat: 0,
            },
            &st,
        );
        assert!(evs.is_empty());
    }
}
