use std::sync::Arc;

use axum::{
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::IntoResponse,
};
use tokio::sync::broadcast;
use yggdrasil_core::games::poker::events::TableEvent;

use super::state::PokerState;

pub async fn ws_handler(
    State(state): State<Arc<PokerState>>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
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
            .on_upgrade(move |socket| stream_events(socket, rx))
            .into_response(),
    }
}

async fn stream_events(mut socket: WebSocket, mut rx: broadcast::Receiver<TableEvent>) {
    loop {
        match rx.recv().await {
            Ok(event) => {
                let Ok(json) = serde_json::to_string(&event) else {
                    break;
                };
                if socket.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}
