use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/vault/status", get(vault_status))
        .route("/api/vault/unseal", post(unseal_vault))
        .route("/api/vault/seal", post(seal_vault))
        .route("/api/vault/secrets", get(list_secrets))
        .route("/api/vault/secrets", post(create_secret))
        .route("/api/vault/secrets/{id}", get(get_secret))
        .route("/api/vault/secrets/{id}", put(update_secret))
        .route("/api/vault/secrets/{id}", delete(delete_secret))
}

async fn vault_status(State(state): State<AppState>) -> Json<Value> {
    let vs = state.inner.vault_state.read().await;
    Json(json!({
        "status": vs.status_str(),
    }))
}

#[derive(Deserialize)]
struct UnsealRequest {
    passphrase: String,
}

async fn unseal_vault(
    State(state): State<AppState>,
    Json(req): Json<UnsealRequest>,
) -> (StatusCode, Json<Value>) {
    let vmk_row = match crate::db::vault_key::get_vmk(&state.inner.db).await {
        Ok(Some(row)) => row,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Vault not initialized. Run setup first." })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    let passphrase_key = match sao_core::vault::kdf::derive_key_from_passphrase(
        &req.passphrase,
        &vmk_row.kdf_salt,
        vmk_row.kdf_memory_cost as u32,
        vmk_row.kdf_time_cost as u32,
        vmk_row.kdf_parallelism as u32,
    ) {
        Ok(key) => key,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("KDF failed: {}", e) })),
            );
        }
    };

    let encrypted = &vmk_row.encrypted_key;
    if encrypted.len() < 12 {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Corrupted VMK envelope" })),
        );
    }
    let (ciphertext, nonce) = encrypted.split_at(encrypted.len() - 12);

    match sao_core::vault::VaultMasterKey::unseal(ciphertext, nonce, &passphrase_key) {
        Ok(vmk) => {
            let mut vs = state.inner.vault_state.write().await;
            *vs = crate::vault_state::VaultState::Unsealed(vmk);
            (StatusCode::OK, Json(json!({ "status": "unsealed" })))
        }
        Err(e) => (StatusCode::UNAUTHORIZED, Json(json!({ "error": e }))),
    }
}

async fn seal_vault(_user: AuthUser, State(state): State<AppState>) -> Json<Value> {
    let mut vs = state.inner.vault_state.write().await;
    if vs.is_unsealed() {
        *vs = crate::vault_state::VaultState::Sealed;
        Json(json!({ "status": "sealed" }))
    } else {
        Json(json!({ "status": vs.status_str() }))
    }
}

#[derive(Deserialize)]
struct CreateSecretRequest {
    secret_type: String,
    label: String,
    provider: Option<String>,
    value: String,
    metadata: Option<serde_json::Value>,
}

async fn create_secret(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateSecretRequest>,
) -> (StatusCode, Json<Value>) {
    let vs = state.inner.vault_state.read().await;
    let vmk = match vs.vmk() {
        Some(vmk) => vmk,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "Vault is sealed" })),
            );
        }
    };

    let (ciphertext, nonce) = match vmk.encrypt(req.value.as_bytes()) {
        Ok(result) => result,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Encryption failed: {}", e) })),
            );
        }
    };

    match crate::db::vault::create_secret(
        &state.inner.db,
        Some(user.user_id),
        &req.secret_type,
        &req.label,
        req.provider.as_deref(),
        &ciphertext,
        &nonce,
        req.metadata,
    )
    .await
    {
        Ok(id) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(user.user_id),
                None,
                "vault.create_secret",
                Some("vault_secret"),
                Some(json!({ "secret_id": id, "label": req.label })),
                None,
                None,
            )
            .await;
            (StatusCode::CREATED, Json(json!({ "id": id })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn list_secrets(user: AuthUser, State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    // Users see only their own secrets; admins see all
    let owner_filter = if user.is_admin() {
        None
    } else {
        Some(user.user_id)
    };

    match crate::db::vault::list_secrets_metadata(&state.inner.db, owner_filter).await {
        Ok(secrets) => (StatusCode::OK, Json(json!({ "secrets": secrets }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn get_secret(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    let vs = state.inner.vault_state.read().await;
    let vmk = match vs.vmk() {
        Some(vmk) => vmk,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "Vault is sealed" })),
            );
        }
    };

    let secret = match crate::db::vault::get_secret(&state.inner.db, id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Secret not found" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    // Ownership check: non-admins can only access their own secrets
    if !user.is_admin() && secret.owner_user_id != Some(user.user_id) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Access denied" })),
        );
    }

    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(user.user_id),
        None,
        "vault.read_secret",
        Some("vault_secret"),
        Some(json!({ "secret_id": id })),
        None,
        None,
    )
    .await;

    match vmk.decrypt(&secret.ciphertext, &secret.nonce) {
        Ok(plaintext) => {
            let value = String::from_utf8_lossy(&plaintext).to_string();
            (
                StatusCode::OK,
                Json(json!({
                    "id": secret.id,
                    "secret_type": secret.secret_type,
                    "label": secret.label,
                    "provider": secret.provider,
                    "value": value,
                    "metadata": secret.metadata,
                    "created_at": secret.created_at,
                    "updated_at": secret.updated_at,
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Decryption failed: {}", e) })),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateSecretRequest {
    label: Option<String>,
    value: Option<String>,
    metadata: Option<serde_json::Value>,
}

async fn update_secret(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateSecretRequest>,
) -> (StatusCode, Json<Value>) {
    // Check ownership
    if !user.is_admin() {
        if let Ok(Some(secret)) = crate::db::vault::get_secret(&state.inner.db, id).await {
            if secret.owner_user_id != Some(user.user_id) {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": "Access denied" })),
                );
            }
        }
    }

    let (new_ct, new_nonce) = if let Some(ref value) = req.value {
        let vs = state.inner.vault_state.read().await;
        let vmk = match vs.vmk() {
            Some(vmk) => vmk,
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "error": "Vault is sealed" })),
                );
            }
        };
        match vmk.encrypt(value.as_bytes()) {
            Ok((ct, nonce)) => (Some(ct), Some(nonce)),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("Encryption failed: {}", e) })),
                );
            }
        }
    } else {
        (None, None)
    };

    match crate::db::vault::update_secret(
        &state.inner.db,
        id,
        req.label.as_deref(),
        new_ct.as_deref(),
        new_nonce.as_deref(),
        req.metadata,
    )
    .await
    {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(user.user_id),
                None,
                "vault.update_secret",
                Some("vault_secret"),
                Some(json!({ "secret_id": id })),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({ "updated": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Secret not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn delete_secret(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    // Check ownership
    if !user.is_admin() {
        if let Ok(Some(secret)) = crate::db::vault::get_secret(&state.inner.db, id).await {
            if secret.owner_user_id != Some(user.user_id) {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": "Access denied" })),
                );
            }
        }
    }

    match crate::db::vault::delete_secret(&state.inner.db, id).await {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(user.user_id),
                None,
                "vault.delete_secret",
                Some("vault_secret"),
                Some(json!({ "secret_id": id })),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({ "deleted": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Secret not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}
