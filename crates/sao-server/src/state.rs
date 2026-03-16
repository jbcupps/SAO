//! Application state for sao-server.

use crate::security::SecurityState;
use anyhow::Context;
use sao_core::IdentityManager;
use sqlx::PgPool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use webauthn_rs::Webauthn;

use crate::vault_state::VaultState;

/// Shared application state for the SAO orchestration server.
#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<AppStateInner>,
}

#[allow(dead_code)]
pub struct AppStateInner {
    pub identity_manager: Arc<IdentityManager>,
    pub active_agent_id: std::sync::RwLock<Option<String>>,
    /// WebSocket broadcast channel for streaming events to connected agents
    pub ws_tx: tokio::sync::broadcast::Sender<WsEvent>,
    /// PostgreSQL connection pool
    pub db: PgPool,
    /// Vault seal state
    pub vault_state: RwLock<VaultState>,
    /// WebAuthn relying party
    pub webauthn: Arc<Webauthn>,
    /// JWT signing secret
    pub jwt_secret: [u8; 32],
    /// Shared request security state
    pub security: Arc<SecurityState>,
}

/// Events sent to WebSocket clients.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WsEvent {
    pub event: String,
    pub payload: serde_json::Value,
}

/// Initialize the SAO application state.
pub fn init_app_state(
    db: PgPool,
    vault_state: VaultState,
    webauthn: Webauthn,
    jwt_secret: [u8; 32],
) -> anyhow::Result<AppState> {
    let data_root = default_data_root();
    init_app_state_with_data_root(db, vault_state, webauthn, jwt_secret, data_root)
}

fn init_app_state_with_data_root(
    db: PgPool,
    vault_state: VaultState,
    webauthn: Webauthn,
    jwt_secret: [u8; 32],
    data_root: PathBuf,
) -> anyhow::Result<AppState> {
    let identity_manager =
        Arc::new(IdentityManager::new(data_root.clone()).with_context(|| {
            format!(
                "Failed to initialize IdentityManager using {}",
                data_root.display()
            )
        })?);
    let security = Arc::new(SecurityState::from_env());

    let (ws_tx, _) = tokio::sync::broadcast::channel::<WsEvent>(256);

    Ok(AppState {
        inner: Arc::new(AppStateInner {
            identity_manager,
            active_agent_id: std::sync::RwLock::new(None),
            ws_tx,
            db,
            vault_state: RwLock::new(vault_state),
            webauthn: Arc::new(webauthn),
            jwt_secret,
            security,
        }),
    })
}

pub fn allowed_origin_values(state: &AppState) -> Vec<axum::http::HeaderValue> {
    state
        .inner
        .security
        .allowed_origins
        .iter()
        .filter_map(|origin| axum::http::HeaderValue::from_str(origin).ok())
        .collect()
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

#[cfg(test)]
mod tests {
    use super::init_app_state_with_data_root;
    use crate::auth::webauthn::create_webauthn_from_config;
    use crate::vault_state::VaultState;
    use sqlx::PgPool;
    use std::fs;
    use uuid::Uuid;

    #[tokio::test]
    async fn init_app_state_returns_ok_with_explicit_data_root() {
        let data_root = std::env::temp_dir().join(format!("sao-state-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&data_root).expect("temp data root should be created");

        let pool = PgPool::connect_lazy("postgresql://tester:secret@localhost/sao_test")
            .expect("lazy pool should be created");
        let webauthn = create_webauthn_from_config("localhost", "http://localhost:3100")
            .expect("default local WebAuthn config should work");

        let state = init_app_state_with_data_root(
            pool,
            VaultState::Uninitialized,
            webauthn,
            [7u8; 32],
            data_root.clone(),
        )
        .expect("app state should initialize without panicking");

        assert_eq!(
            state.inner.identity_manager.data_root(),
            data_root.as_path()
        );

        fs::remove_dir_all(&data_root).expect("temp data root should be removed");
    }
}
