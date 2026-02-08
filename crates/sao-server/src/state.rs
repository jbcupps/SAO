//! Application state for sao-server.

use sao_core::IdentityManager;
use std::path::PathBuf;
use std::sync::Arc;

/// Shared application state for the SAO orchestration server.
#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<AppStateInner>,
}

pub struct AppStateInner {
    pub identity_manager: Arc<IdentityManager>,
    pub active_agent_id: std::sync::RwLock<Option<String>>,
    /// WebSocket broadcast channel for streaming events to connected agents
    pub ws_tx: tokio::sync::broadcast::Sender<WsEvent>,
    /// PostgreSQL connection pool (optional)
    pub db: crate::db::DbPool,
}

/// Events sent to WebSocket clients.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WsEvent {
    pub event: String,
    pub payload: serde_json::Value,
}

/// Initialize the SAO application state.
pub fn init_app_state(db: crate::db::DbPool) -> AppState {
    let data_root = default_data_root();

    let identity_manager = Arc::new(
        IdentityManager::new(data_root.clone()).unwrap_or_else(|e| {
            tracing::error!("Failed to initialize IdentityManager: {}", e);
            panic!("IdentityManager initialization failed: {}", e);
        }),
    );

    let (ws_tx, _) = tokio::sync::broadcast::channel::<WsEvent>(256);

    AppState {
        inner: Arc::new(AppStateInner {
            identity_manager,
            active_agent_id: std::sync::RwLock::new(None),
            ws_tx,
            db,
        }),
    }
}

/// Default data directory for SAO.
fn default_data_root() -> PathBuf {
    if let Ok(dir) = std::env::var("SAO_DATA_DIR") {
        return PathBuf::from(dir);
    }

    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sao")
}
