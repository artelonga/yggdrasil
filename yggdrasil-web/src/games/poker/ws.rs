use std::sync::Arc;

use axum::{
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use tokio::sync::broadcast;
use yggdrasil_core::games::poker::events::TableEvent;

use super::lobby_event::map_event;
use super::state::PokerState;
use crate::auth::verify_jwt;

/// JWT chega como query param (`?token=…`) porque o construtor `WebSocket` do
/// browser não permite setar headers — não há como mandar `Authorization`.
#[derive(Debug, Deserialize)]
pub struct StreamAuth {
    pub token: String,
}

/// `GET /api/v1/poker/lobbies/{id}/stream` — faz upgrade para WebSocket e pusha
/// [`LobbyEvent`](super::lobby_event::LobbyEvent)s da mesa em tempo real (YG-28).
/// Autentica via JWT no query param antes do upgrade.
pub async fn stream_handler(
    State(state): State<Arc<PokerState>>,
    Path(id): Path<String>,
    Query(auth): Query<StreamAuth>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let user_id = match verify_jwt(&auth.token, &state.jwt_secret) {
        Ok(uid) => uid,
        Err(_) => return (StatusCode::UNAUTHORIZED, "token inválido").into_response(),
    };

    let rx = {
        let tables = state.tables.lock().unwrap();
        tables
            .iter()
            .find(|t| t.lobby.id == id)
            .map(|t| t.events.subscribe())
    };

    match rx {
        None => (StatusCode::NOT_FOUND, "mesa não encontrada").into_response(),
        Some(rx) => ws
            .on_upgrade(move |socket| stream_events(socket, rx, state, id, user_id))
            .into_response(),
    }
}

async fn stream_events(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<TableEvent>,
    state: Arc<PokerState>,
    lobby_id: String,
    user_id: String,
) {
    tracing::info!(user_id = %user_id, lobby_id = %lobby_id, "poker WS conectado");
    'conn: loop {
        match rx.recv().await {
            Ok(event) => {
                for le in map_event(&event, &state) {
                    let Ok(json) = serde_json::to_string(&le) else {
                        continue;
                    };
                    if socket.send(Message::Text(json.into())).await.is_err() {
                        break 'conn;
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
    tracing::info!(user_id = %user_id, lobby_id = %lobby_id, "poker WS desconectado");
}
