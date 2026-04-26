use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use fetorrent_core::Engine;
use futures::{sink::SinkExt, stream::StreamExt};
use std::sync::Arc;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(engine): State<Arc<Engine>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, engine))
}

async fn handle_socket(socket: WebSocket, engine: Arc<Engine>) {
    let (mut sender, mut _receiver) = socket.split();
    let mut rx = engine.subscribe();

    // Initial state dump
    let initial = fetorrent_core::models::TorrentEvent::StatsUpdate {
        torrents: engine.get_torrents(),
        global: engine.get_global_stats(),
    };
    if let Ok(msg) = serde_json::to_string(&initial) {
        if sender.send(Message::Text(msg)).await.is_err() {
            return;
        }
    }

    // Subscribe to updates loop
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let Ok(msg) = serde_json::to_string(&event) {
                if sender.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Optionally handle incoming messages from client here
}
