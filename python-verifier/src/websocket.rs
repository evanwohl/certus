use axum::{
    extract::{State, WebSocketUpgrade, ws::WebSocket},
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobUpdate {
    pub job_id: String,
    pub status: String,
    pub timestamp: u64,
    pub data: serde_json::Value,
}

#[derive(Clone)]
pub struct WsState {
    pub tx: broadcast::Sender<JobUpdate>,
}

impl WsState {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self { tx }
    }
}

/// WebSocket handler for real-time job updates
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WsState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<WsState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    // spawn task to forward updates to client
    let mut send_task = tokio::spawn(async move {
        while let Ok(update) = rx.recv().await {
            let msg = serde_json::to_string(&update).unwrap();
            if sender.send(axum::extract::ws::Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // spawn task to handle incoming messages
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                axum::extract::ws::Message::Text(text) => {
                    // handle subscription requests
                    if let Ok(sub) = serde_json::from_str::<SubscribeRequest>(&text) {
                        println!("Client subscribed to job: {}", sub._job_id);
                    }
                }
                axum::extract::ws::Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // wait for either task to finish
    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    }
}

#[derive(Debug, Deserialize)]
struct SubscribeRequest {
    _job_id: String,
}

/// Broadcast job update to all connected clients
pub fn broadcast_update(state: &WsState, update: JobUpdate) {
    let _ = state.tx.send(update);
}