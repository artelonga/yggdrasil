//! Stream público de analytics ao vivo (YG-128) — WebSocket com ring buffer,
//! padrão do `/api/v1/analytics/stream` do CO (snapshot no connect + broadcast).
//!
//! **Só agregados/efêmeros anônimos** saem por aqui: nada de slug, título,
//! usuário ou path de nota — eventos são `{ev, universo?, ts}` e frames de
//! stats `{ev:"stats", jogando_agora, sessoes_24h}`. Rascunhos nunca passam
//! (o producer de notas não os vê — privacidade por construção, YG-125).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use tokio::sync::broadcast;

const RING: usize = 200;
const CANAL: usize = 256;

#[derive(Clone)]
pub struct Pulso {
    ring: Arc<Mutex<VecDeque<serde_json::Value>>>,
    tx: broadcast::Sender<serde_json::Value>,
}

impl Default for Pulso {
    fn default() -> Self {
        Self::new()
    }
}

impl Pulso {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(CANAL);
        Self {
            ring: Arc::new(Mutex::new(VecDeque::with_capacity(RING))),
            tx,
        }
    }

    /// Publica um evento anônimo no stream (e no snapshot dos próximos).
    pub fn push(&self, ev: serde_json::Value) {
        if let Ok(mut r) = self.ring.lock() {
            if r.len() >= RING {
                r.pop_front();
            }
            r.push_back(ev.clone());
        }
        let _ = self.tx.send(ev); // sem assinantes = no-op
    }

    fn snapshot(&self) -> Vec<serde_json::Value> {
        self.ring
            .lock()
            .map(|r| r.iter().cloned().collect())
            .unwrap_or_default()
    }
}

/// `GET /api/v1/analytics/stream` — WS público: snapshot do ring + broadcast.
/// Assinante lento é derrubado (lagged), espelhando o hub do CO.
pub async fn ws_stream(State(pulso): State<Pulso>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| stream_socket(socket, pulso))
}

async fn stream_socket(mut socket: WebSocket, pulso: Pulso) {
    let snap = serde_json::json!({ "ev": "snapshot", "eventos": pulso.snapshot() });
    if socket
        .send(Message::Text(snap.to_string().into()))
        .await
        .is_err()
    {
        return;
    }
    let mut rx = pulso.tx.subscribe();
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(v) => {
                    if socket.send(Message::Text(v.to_string().into())).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Ping(p))) => { let _ = socket.send(Message::Pong(p)).await; }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                _ => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_guarda_ate_o_limite_e_descarta_o_mais_velho() {
        let p = Pulso::new();
        for i in 0..(RING + 10) {
            p.push(serde_json::json!({ "ev": "x", "i": i }));
        }
        let snap = p.snapshot();
        assert_eq!(snap.len(), RING);
        assert_eq!(snap[0]["i"], 10, "FIFO: os 10 primeiros caíram");
    }
}
