use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::middleware::AdminUser;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // User management (admin only)
        .route("/api/admin/users", get(list_users))
        .route("/api/admin/users/{id}/role", put(update_user_role))
        .route("/api/admin/users/{id}", delete(delete_user))
        // OIDC provider management (admin only)
        .route("/api/admin/oidc/providers", post(create_oidc_provider))
        .route("/api/admin/oidc/providers", get(list_oidc_providers))
        .route("/api/admin/oidc/providers/{id}", put(update_oidc_provider))
        .route("/api/admin/oidc/providers/{id}", delete(delete_oidc_provider))
        // Audit log (admin only)
        .route("/api/admin/audit", get(query_audit_log))
}

// --- User management ---

async fn list_users(
    AdminUser(_admin): AdminUser,
    State(state): State<AppState>,
) -> (StatusCode, Json<Value>) {
    match crate::db::users::list_users(&state.inner.db).await {
        Ok(users) => (StatusCode::OK, Json(json!({ "users": users }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateRoleRequest {
    role: String,
}

async fn update_user_role(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRoleRequest>,
) -> (StatusCode, Json<Value>) {
    if req.role != "user" && req.role != "admin" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Role must be 'user' or 'admin'" })),
        );
    }

    match crate::db::users::update_user_role(&state.inner.db, id, &req.role).await {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "admin.update_role",
                Some("user"),
                Some(json!({ "target_user_id": id, "new_role": req.role })),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({ "updated": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn delete_user(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    // Prevent self-deletion
    if id == admin.user_id {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Cannot delete your own account" })),
        );
    }

    match crate::db::users::delete_user(&state.inner.db, id).await {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "admin.delete_user",
                Some("user"),
                Some(json!({ "deleted_user_id": id })),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({ "deleted": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

// --- OIDC provider management ---

#[derive(Deserialize)]
struct CreateOidcProviderRequest {
    name: String,
    issuer_url: String,
    client_id: String,
    client_secret: Option<String>,
    scopes: Option<String>,
}

async fn create_oidc_provider(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Json(req): Json<CreateOidcProviderRequest>,
) -> (StatusCode, Json<Value>) {
    // Encrypt client secret if provided
    let encrypted_secret = if let Some(ref secret) = req.client_secret {
        let vs = state.inner.vault_state.read().await;
        match vs.vmk() {
            Some(vmk) => match vmk.encrypt(secret.as_bytes()) {
                Ok((ct, nonce)) => {
                    let mut combined = ct;
                    combined.extend_from_slice(&nonce);
                    Some(combined)
                }
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": format!("Failed to encrypt secret: {}", e) })),
                    );
                }
            },
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "error": "Vault is sealed" })),
                );
            }
        }
    } else {
        None
    };

    let scopes = req.scopes.as_deref().unwrap_or("openid profile email");

    match crate::db::oidc::create_provider(
        &state.inner.db,
        &req.name,
        &req.issuer_url,
        &req.client_id,
        encrypted_secret.as_deref(),
        scopes,
    )
    .await
    {
        Ok(id) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "admin.create_oidc_provider",
                Some("oidc_provider"),
                Some(json!({ "provider_name": req.name, "provider_id": id })),
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

async fn list_oidc_providers(
    AdminUser(_admin): AdminUser,
    State(state): State<AppState>,
) -> (StatusCode, Json<Value>) {
    match crate::db::oidc::list_providers(&state.inner.db).await {
        Ok(providers) => {
            // Strip encrypted secrets from response
            let sanitized: Vec<Value> = providers
                .iter()
                .map(|p| {
                    json!({
                        "id": p.id,
                        "name": p.name,
                        "issuer_url": p.issuer_url,
                        "client_id": p.client_id,
                        "has_client_secret": p.client_secret_encrypted.is_some(),
                        "scopes": p.scopes,
                        "enabled": p.enabled,
                        "created_at": p.created_at,
                        "updated_at": p.updated_at,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "providers": sanitized })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateOidcProviderRequest {
    name: Option<String>,
    issuer_url: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    scopes: Option<String>,
    enabled: Option<bool>,
}

async fn update_oidc_provider(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateOidcProviderRequest>,
) -> (StatusCode, Json<Value>) {
    let encrypted_secret = if let Some(ref secret) = req.client_secret {
        let vs = state.inner.vault_state.read().await;
        match vs.vmk() {
            Some(vmk) => match vmk.encrypt(secret.as_bytes()) {
                Ok((ct, nonce)) => {
                    let mut combined = ct;
                    combined.extend_from_slice(&nonce);
                    Some(combined)
                }
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": format!("Failed to encrypt secret: {}", e) })),
                    );
                }
            },
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "error": "Vault is sealed" })),
                );
            }
        }
    } else {
        None
    };

    match crate::db::oidc::update_provider(
        &state.inner.db,
        id,
        req.name.as_deref(),
        req.issuer_url.as_deref(),
        req.client_id.as_deref(),
        encrypted_secret.as_deref(),
        req.scopes.as_deref(),
        req.enabled,
    )
    .await
    {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "admin.update_oidc_provider",
                Some("oidc_provider"),
                Some(json!({ "provider_id": id })),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({ "updated": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Provider not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn delete_oidc_provider(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    match crate::db::oidc::delete_provider(&state.inner.db, id).await {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "admin.delete_oidc_provider",
                Some("oidc_provider"),
                Some(json!({ "provider_id": id })),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({ "deleted": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Provider not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

// --- Audit log ---

#[derive(Deserialize)]
struct AuditLogQuery {
    user_id: Option<Uuid>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn query_audit_log(
    AdminUser(_admin): AdminUser,
    State(state): State<AppState>,
    Query(params): Query<AuditLogQuery>,
) -> (StatusCode, Json<Value>) {
    let limit = params.limit.unwrap_or(100).min(1000);
    let offset = params.offset.unwrap_or(0);

    match crate::db::admin::query_audit_log(&state.inner.db, params.user_id, limit, offset).await {
        Ok(entries) => (StatusCode::OK, Json(json!({ "audit_log": entries }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}
