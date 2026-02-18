//! WebSocket handler for real-time agent communication.
//!
//! Agents connect via `WS /ws/agent/:id` for bidirectional communication.
//! SAO broadcasts events (ethical scores, orchestration commands) to connected agents.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};

use crate::state::AppState;

pub fn ws_routes() -> Router<AppState> {
    Router::new().route("/ws/agent/{agent_id}", get(ws_handler))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(agent_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    tracing::info!("WebSocket connection request from agent: {}", agent_id);
    ws.on_upgrade(move |socket| handle_socket(socket, agent_id, state))
}

async fn handle_socket(socket: WebSocket, agent_id: String, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast channel
    let mut rx = state.inner.ws_tx.subscribe();

    // Spawn task to forward broadcast events to this WebSocket
    let send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let msg = serde_json::to_string(&event).unwrap_or_default();
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Receive messages from the agent
    let agent_id_clone = agent_id.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    tracing::debug!("Agent {} sent: {}", agent_id_clone, text);
                    // Handle agent messages (status updates, etc.)
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(msg_type) = val.get("type").and_then(|v| v.as_str()) {
                            match msg_type {
                                "status" => {
                                    tracing::info!(
                                        "Agent {} status update: {}",
                                        agent_id_clone,
                                        text
                                    );
                                }
                                "heartbeat" => {
                                    // Agent is alive
                                }
                                _ => {
                                    tracing::debug!(
                                        "Agent {} unknown message type: {}",
                                        agent_id_clone,
                                        msg_type
                                    );
                                }
                            }
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    tracing::info!("Agent {} disconnected", agent_id);
}
