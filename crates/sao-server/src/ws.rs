//! WebSocket handler for real-time agent communication.
//!
//! Agents connect via `WS /ws/agent/{agent_id}` for bidirectional communication.
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
use tokio::sync::Mutex;

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
    let (sender, mut receiver) = socket.split();
    let sender = std::sync::Arc::new(Mutex::new(sender));

    // Subscribe to broadcast channel
    let mut rx = state.inner.ws_tx.subscribe();

    // Spawn task to forward broadcast events to this WebSocket
    let send_handle = sender.clone();
    let send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let msg = serde_json::to_string(&event).unwrap_or_default();
            let mut sender = send_handle.lock().await;
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Receive messages from the agent
    let agent_id_clone = agent_id.clone();
    let recv_handle = sender.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    tracing::debug!("Agent {} sent: {}", agent_id_clone, text);
                    let msg_type = match serde_json::from_str::<serde_json::Value>(&text) {
                        Ok(val) => val.get("type").and_then(|v| v.as_str()).map(str::to_owned),
                        Err(_) => Some(text.to_string()),
                    };

                    match msg_type.as_deref() {
                        Some("status") => {
                            tracing::info!("Agent {} status update: {}", agent_id_clone, text);
                        }
                        Some("heartbeat") => {
                            let last_heartbeat = chrono::Utc::now().to_rfc3339();
                            println!("Agent {} heartbeat received", agent_id_clone);
                            let rollup = sao_core::ethical_bridge::propose_periodic_superego_rollup(
                                &agent_id_clone,
                            );
                            tracing::info!("Superego roll-up: {}", rollup);
                            let mut sender = recv_handle.lock().await;
                            if sender
                                .send(Message::Text(
                                    serde_json::json!({
                                        "status": "ACTIVE",
                                        "last_heartbeat": last_heartbeat,
                                        "tweak_proposal": rollup,
                                    })
                                    .to_string()
                                    .into(),
                                ))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        Some(other) => {
                            tracing::debug!(
                                "Agent {} unknown message type: {}",
                                agent_id_clone,
                                other
                            );
                        }
                        None => {}
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
