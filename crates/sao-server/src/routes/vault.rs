use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::middleware::{AdminUser, AuthUser};
use crate::state::AppState;

/// Minimum vault passphrase length (NIST SP 800-63B "memorized secret").
///
/// We accept memorized secrets at this length and gate stronger entropy via
/// rejecting a small list of common passwords. This deliberately does not
/// require composition rules, which NIST advises against.
pub const MIN_VAULT_PASSPHRASE_LEN: usize = 12;
pub const MAX_VAULT_PASSPHRASE_LEN: usize = 4096;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/vault/status", get(vault_status))
        .route("/api/vault/configure", post(configure_vault))
        .route("/api/vault/rotate-passphrase", post(rotate_passphrase))
        .route("/api/vault/unseal", post(unseal_vault))
        .route("/api/vault/seal", post(seal_vault))
        .route("/api/vault/secrets", get(list_secrets))
        .route("/api/vault/secrets", post(create_secret))
        .route("/api/vault/secrets/:id", get(get_secret))
        .route("/api/vault/secrets/:id", put(update_secret))
        .route("/api/vault/secrets/:id", delete(delete_secret))
}

async fn vault_status(State(state): State<AppState>) -> Json<Value> {
    let vs = state.inner.vault_state.read().await;
    Json(json!({
        "status": vs.status_str(),
        "auto_unseal_env_present": auto_unseal_env_present(),
    }))
}

fn auto_unseal_env_present() -> bool {
    std::env::var("SAO_VAULT_PASSPHRASE")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

/// Server-side passphrase strength check.
///
/// Returns Err(public_error_message) when the passphrase is unacceptable.
/// Public messages are deliberately neutral so a single failure mode does not
/// leak which other constraint also failed.
fn validate_passphrase_strength(passphrase: &str) -> Result<(), &'static str> {
    let trimmed = passphrase.trim();
    if trimmed.len() < MIN_VAULT_PASSPHRASE_LEN {
        return Err("Passphrase is too short");
    }
    if passphrase.chars().count() > MAX_VAULT_PASSPHRASE_LEN {
        return Err("Passphrase is too long");
    }
    if is_common_passphrase(trimmed) {
        return Err("Passphrase is too common");
    }
    Ok(())
}

/// Tiny denylist of obviously unsafe passphrases. Not a substitute for the
/// length floor; this just catches the most common values that beat the
/// length check (e.g. "password1234", "letmeinplease").
fn is_common_passphrase(passphrase: &str) -> bool {
    const DENYLIST: &[&str] = &[
        "password1234",
        "passwordpassword",
        "letmeinplease",
        "changemechangeme",
        "qwertyqwerty",
        "qwerty123456",
        "administrator",
        "iloveyou1234",
        "trustno1trustno1",
        "welcome12345",
        "monkeymonkey",
        "dragondragon",
        "sunshine1234",
        "starwars1977",
        "footballsuck",
    ];
    let lower = passphrase.to_ascii_lowercase();
    DENYLIST.iter().any(|candidate| lower == *candidate)
}

#[derive(Deserialize)]
struct ConfigureVaultRequest {
    passphrase: String,
    passphrase_confirmation: String,
}

/// First-time vault passphrase configuration.
///
/// Generates a fresh VaultMasterKey, derives a passphrase key with Argon2id,
/// seals the VMK, and stores the envelope. Refuses (409) once the vault has
/// already been configured, because overwriting the VMK would orphan every
/// existing encrypted secret.
async fn configure_vault(
    user: AdminUser,
    State(state): State<AppState>,
    Json(req): Json<ConfigureVaultRequest>,
) -> (StatusCode, Json<Value>) {
    let admin = user.0;

    if req.passphrase != req.passphrase_confirmation {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Passphrase and confirmation do not match",
                "code": "confirmation_mismatch",
            })),
        );
    }

    if let Err(reason) = validate_passphrase_strength(&req.passphrase) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": reason,
                "code": "weak_passphrase",
                "min_length": MIN_VAULT_PASSPHRASE_LEN,
            })),
        );
    }

    match crate::db::vault_key::vmk_exists(&state.inner.db).await {
        Ok(true) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "Vault is already configured. Use /api/vault/rotate-passphrase to change the passphrase.",
                    "code": "vault_already_initialized",
                })),
            );
        }
        Ok(false) => {}
        Err(e) => {
            tracing::error!(error = %e, "Failed to check vault initialization state");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Vault state lookup failed" })),
            );
        }
    }

    let vmk = sao_core::vault::VaultMasterKey::generate();
    let salt = sao_core::vault::generate_salt();
    let passphrase_key = match sao_core::vault::kdf::derive_key_from_passphrase(
        &req.passphrase,
        &salt,
        sao_core::vault::kdf::DEFAULT_MEMORY_COST,
        sao_core::vault::kdf::DEFAULT_TIME_COST,
        sao_core::vault::kdf::DEFAULT_PARALLELISM,
    ) {
        Ok(key) => key,
        Err(e) => {
            tracing::error!(error = %e, "Argon2 KDF failed during vault configure");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to derive passphrase key" })),
            );
        }
    };

    let (sealed, nonce) = match vmk.seal(&passphrase_key) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!(error = %e, "Failed to seal new VMK during vault configure");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to seal new vault master key" })),
            );
        }
    };
    let mut envelope = sealed;
    envelope.extend_from_slice(&nonce);

    if let Err(e) = crate::db::vault_key::insert_vmk(
        &state.inner.db,
        &envelope,
        &salt,
        sao_core::vault::kdf::DEFAULT_MEMORY_COST as i32,
        sao_core::vault::kdf::DEFAULT_TIME_COST as i32,
        sao_core::vault::kdf::DEFAULT_PARALLELISM as i32,
    )
    .await
    {
        tracing::error!(error = %e, "Failed to persist vault master key envelope");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to persist vault master key" })),
        );
    }

    {
        let mut vs = state.inner.vault_state.write().await;
        *vs = crate::vault_state::VaultState::Unsealed(vmk);
    }

    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(admin.user_id),
        None,
        "vault.configure",
        Some("vault_master_key"),
        Some(json!({
            "auto_unseal_env_present": auto_unseal_env_present(),
        })),
        None,
        None,
    )
    .await;

    (
        StatusCode::OK,
        Json(json!({
            "status": "unsealed",
            "auto_unseal_env_present": auto_unseal_env_present(),
        })),
    )
}

#[derive(Deserialize)]
struct RotatePassphraseRequest {
    current_passphrase: String,
    new_passphrase: String,
    new_passphrase_confirmation: String,
}

/// Rotate the vault passphrase.
///
/// Verifies the caller knows the current passphrase by re-deriving the KDF
/// key and unsealing a *throwaway* copy of the VMK from the stored envelope.
/// On success, generates a fresh KDF salt and re-seals the same VMK with the
/// new passphrase-derived key. Existing secret ciphertexts stay valid because
/// the VMK bytes never change.
async fn rotate_passphrase(
    user: AdminUser,
    State(state): State<AppState>,
    Json(req): Json<RotatePassphraseRequest>,
) -> (StatusCode, Json<Value>) {
    let admin = user.0;

    if req.new_passphrase != req.new_passphrase_confirmation {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "New passphrase and confirmation do not match",
                "code": "confirmation_mismatch",
            })),
        );
    }

    if req.new_passphrase == req.current_passphrase {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "New passphrase must be different from the current passphrase",
                "code": "same_as_current",
            })),
        );
    }

    if let Err(reason) = validate_passphrase_strength(&req.new_passphrase) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": reason,
                "code": "weak_passphrase",
                "min_length": MIN_VAULT_PASSPHRASE_LEN,
            })),
        );
    }

    let vmk_row = match crate::db::vault_key::get_vmk(&state.inner.db).await {
        Ok(Some(row)) => row,
        Ok(None) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "Vault is not configured yet. Use /api/vault/configure first.",
                    "code": "vault_uninitialized",
                })),
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to load vault master key envelope");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to load vault master key envelope" })),
            );
        }
    };

    let current_key = match sao_core::vault::kdf::derive_key_from_passphrase(
        &req.current_passphrase,
        &vmk_row.kdf_salt,
        vmk_row.kdf_memory_cost as u32,
        vmk_row.kdf_time_cost as u32,
        vmk_row.kdf_parallelism as u32,
    ) {
        Ok(key) => key,
        Err(e) => {
            tracing::error!(error = %e, "Argon2 KDF failed during passphrase rotation");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to derive current passphrase key" })),
            );
        }
    };

    if vmk_row.encrypted_key.len() < 12 {
        tracing::error!("Stored VMK envelope is too short to contain a nonce");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Corrupted vault master key envelope" })),
        );
    }
    let (current_ciphertext, current_nonce) = vmk_row
        .encrypted_key
        .split_at(vmk_row.encrypted_key.len() - 12);

    let vmk = match sao_core::vault::VaultMasterKey::unseal(
        current_ciphertext,
        current_nonce,
        &current_key,
    ) {
        Ok(key) => key,
        Err(_) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "vault.rotate_passphrase.failed",
                Some("vault_master_key"),
                Some(json!({ "reason": "invalid_current_passphrase" })),
                None,
                None,
            )
            .await;
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "Current passphrase is incorrect",
                    "code": "invalid_credentials",
                })),
            );
        }
    };

    let new_salt = sao_core::vault::generate_salt();
    let new_key = match sao_core::vault::kdf::derive_key_from_passphrase(
        &req.new_passphrase,
        &new_salt,
        sao_core::vault::kdf::DEFAULT_MEMORY_COST,
        sao_core::vault::kdf::DEFAULT_TIME_COST,
        sao_core::vault::kdf::DEFAULT_PARALLELISM,
    ) {
        Ok(key) => key,
        Err(e) => {
            tracing::error!(error = %e, "Argon2 KDF failed deriving new passphrase key");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to derive new passphrase key" })),
            );
        }
    };

    let (new_sealed, new_nonce) = match vmk.seal(&new_key) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!(error = %e, "Failed to seal VMK with new passphrase key");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to seal vault master key with new passphrase" })),
            );
        }
    };
    let mut new_envelope = new_sealed;
    new_envelope.extend_from_slice(&new_nonce);

    match crate::db::vault_key::rotate_vmk_envelope(
        &state.inner.db,
        vmk_row.id,
        &new_envelope,
        &new_salt,
        sao_core::vault::kdf::DEFAULT_MEMORY_COST as i32,
        sao_core::vault::kdf::DEFAULT_TIME_COST as i32,
        sao_core::vault::kdf::DEFAULT_PARALLELISM as i32,
    )
    .await
    {
        Ok(true) => {}
        Ok(false) => {
            tracing::error!("Vault rotation update affected zero rows; envelope unchanged");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Vault rotation did not persist" })),
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to persist rotated vault master key envelope");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to persist rotated vault master key envelope" })),
            );
        }
    }

    {
        let mut vs = state.inner.vault_state.write().await;
        *vs = crate::vault_state::VaultState::Unsealed(vmk);
    }

    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(admin.user_id),
        None,
        "vault.rotate_passphrase",
        Some("vault_master_key"),
        Some(json!({
            "auto_unseal_env_present": auto_unseal_env_present(),
        })),
        None,
        None,
    )
    .await;

    (
        StatusCode::OK,
        Json(json!({
            "status": "unsealed",
            "auto_unseal_env_stale": auto_unseal_env_present(),
        })),
    )
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

#[cfg(test)]
mod tests {
    use super::{
        is_common_passphrase, validate_passphrase_strength, MAX_VAULT_PASSPHRASE_LEN,
        MIN_VAULT_PASSPHRASE_LEN,
    };

    #[test]
    fn passphrase_below_minimum_length_is_rejected() {
        let short = "Ab1!".repeat(2);
        assert!(short.len() < MIN_VAULT_PASSPHRASE_LEN);
        let err =
            validate_passphrase_strength(&short).expect_err("short passphrase must be rejected");
        assert!(err.to_lowercase().contains("short"));
    }

    #[test]
    fn passphrase_meeting_minimum_is_accepted() {
        let strong = "correct horse battery staple alpaca";
        assert!(strong.len() >= MIN_VAULT_PASSPHRASE_LEN);
        assert!(validate_passphrase_strength(strong).is_ok());
    }

    #[test]
    fn passphrase_in_denylist_is_rejected_even_if_long_enough() {
        assert!(is_common_passphrase("password1234"));
        let err = validate_passphrase_strength("password1234")
            .expect_err("denylisted passphrase must be rejected");
        assert!(err.to_lowercase().contains("common"));
    }

    #[test]
    fn passphrase_above_max_length_is_rejected_to_bound_kdf_cost() {
        let absurd = "a".repeat(MAX_VAULT_PASSPHRASE_LEN + 1);
        let err = validate_passphrase_strength(&absurd)
            .expect_err("oversized passphrase must be rejected");
        assert!(err.to_lowercase().contains("long"));
    }
}
