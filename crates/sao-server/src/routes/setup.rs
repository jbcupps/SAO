use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/setup/status", get(setup_status))
        .route("/api/setup/initialize", post(initialize))
}

async fn setup_status(State(state): State<AppState>) -> Json<Value> {
    let vs = state.inner.vault_state.read().await;
    let initialized = !matches!(*vs, crate::vault_state::VaultState::Uninitialized);

    // Also check if any users exist
    let has_users = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(&state.inner.db)
        .await
        .unwrap_or(0)
        > 0;

    Json(json!({
        "initialized": initialized,
        "has_users": has_users,
        "needs_setup": !initialized || !has_users,
    }))
}

#[derive(Deserialize)]
struct InitializeRequest {
    passphrase: String,
    admin_username: String,
    admin_display_name: Option<String>,
}

async fn initialize(
    State(state): State<AppState>,
    Json(req): Json<InitializeRequest>,
) -> (StatusCode, Json<Value>) {
    // Check if already initialized
    {
        let vs = state.inner.vault_state.read().await;
        if !matches!(*vs, crate::vault_state::VaultState::Uninitialized) {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "Vault already initialized" })),
            );
        }
    }

    if req.passphrase.len() < 8 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Passphrase must be at least 8 characters" })),
        );
    }

    // Generate VMK
    let vmk = sao_core::vault::VaultMasterKey::generate();

    // Derive passphrase key
    let salt = sao_core::vault::generate_salt();
    let passphrase_key = match sao_core::vault::kdf::derive_key_default(&req.passphrase, &salt) {
        Ok(key) => key,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("KDF failed: {}", e) })),
            );
        }
    };

    // Seal the VMK
    let (sealed_ciphertext, sealed_nonce) = match vmk.seal(&passphrase_key) {
        Ok(result) => result,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to seal VMK: {}", e) })),
            );
        }
    };

    // Store sealed VMK: combine ciphertext + nonce for storage
    let mut sealed_envelope = sealed_ciphertext;
    sealed_envelope.extend_from_slice(&sealed_nonce);

    if let Err(e) = crate::db::vault_key::store_vmk(
        &state.inner.db,
        &sealed_envelope,
        &salt,
        sao_core::vault::kdf::DEFAULT_MEMORY_COST as i32,
        sao_core::vault::kdf::DEFAULT_TIME_COST as i32,
        sao_core::vault::kdf::DEFAULT_PARALLELISM as i32,
    )
    .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to store VMK: {}", e) })),
        );
    }

    // Create admin user
    let display_name = req
        .admin_display_name
        .unwrap_or_else(|| req.admin_username.clone());

    let user_id = match crate::db::users::create_user(
        &state.inner.db,
        &req.admin_username,
        Some(&display_name),
        "admin",
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to create admin user: {}", e) })),
            );
        }
    };

    // Transition vault state to Unsealed
    {
        let mut vs = state.inner.vault_state.write().await;
        *vs = crate::vault_state::VaultState::Unsealed(vmk);
    }

    // Audit log
    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(user_id),
        None,
        "setup.initialize",
        Some("vault"),
        Some(json!({ "admin_username": req.admin_username })),
        None,
        None,
    )
    .await;

    (
        StatusCode::CREATED,
        Json(json!({
            "status": "initialized",
            "user_id": user_id,
            "vault_status": "unsealed",
        })),
    )
}
