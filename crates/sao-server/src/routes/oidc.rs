use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Redirect,
    routing::get,
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::session;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/oidc/providers", get(list_providers))
        .route("/api/auth/oidc/{provider_id}/authorize", get(authorize))
        .route("/api/auth/oidc/callback", get(callback))
}

/// List enabled OIDC providers (public, for login page).
async fn list_providers(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    match crate::db::oidc::list_providers_public(&state.inner.db).await {
        Ok(providers) => (StatusCode::OK, Json(json!({ "providers": providers }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

/// Redirect to OIDC provider's authorization endpoint.
async fn authorize(
    State(state): State<AppState>,
    Path(provider_id): Path<Uuid>,
) -> Result<Redirect, (StatusCode, Json<Value>)> {
    let provider = crate::db::oidc::get_provider(&state.inner.db, provider_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Provider not found" })),
        ))?;

    if !provider.enabled {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Provider is disabled" })),
        ));
    }

    // Decrypt client secret if present
    let client_secret = if let Some(ref encrypted) = provider.client_secret_encrypted {
        let vs = state.inner.vault_state.read().await;
        if let Some(vmk) = vs.vmk() {
            if encrypted.len() > 12 {
                let (ct, nonce) = encrypted.split_at(encrypted.len() - 12);
                vmk.decrypt(ct, nonce)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let rp_origin =
        std::env::var("SAO_RP_ORIGIN").unwrap_or_else(|_| "http://localhost:3100".to_string());
    let redirect_url = format!("{}/api/auth/oidc/callback", rp_origin);

    let config = crate::auth::oidc::OidcProviderConfig {
        id: provider.id,
        name: provider.name,
        issuer_url: provider.issuer_url,
        client_id: provider.client_id,
        client_secret,
        scopes: provider.scopes,
    };

    let result = crate::auth::oidc::start_authorization(&config, &redirect_url)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e })),
            )
        })?;

    // Store CSRF state in DB as a challenge
    let state_json = json!({
        "provider_id": provider_id,
        "csrf": result.csrf_token.secret(),
        "nonce": result.nonce.secret(),
    });

    crate::db::webauthn::store_challenge(
        &state.inner.db,
        result.csrf_token.secret(),
        state_json,
        "oidc",
        None,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to persist OIDC state: {}", e) })),
        )
    })?;

    Ok(Redirect::temporary(result.auth_url.as_str()))
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

/// Handle OIDC callback after provider authentication.
async fn callback(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> (StatusCode, Json<Value>) {
    // Retrieve and validate CSRF state
    let (state_json, _) =
        match crate::db::webauthn::consume_challenge(&state.inner.db, &query.state).await {
            Ok(Some(data)) => data,
            Ok(None) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": "Invalid or expired OIDC state" })),
                );
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                );
            }
        };

    let provider_id: Uuid = match state_json.get("provider_id").and_then(|v| v.as_str()) {
        Some(id) => match Uuid::parse_str(id) {
            Ok(u) => u,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": "Invalid provider ID in state" })),
                );
            }
        },
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Missing provider_id in state" })),
            );
        }
    };

    // Load provider config
    let provider = match crate::db::oidc::get_provider(&state.inner.db, provider_id).await {
        Ok(Some(p)) => p,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Provider not found" })),
            );
        }
    };

    // Decrypt client secret
    let client_secret = if let Some(ref encrypted) = provider.client_secret_encrypted {
        let vs = state.inner.vault_state.read().await;
        if let Some(vmk) = vs.vmk() {
            if encrypted.len() > 12 {
                let (ct, nonce) = encrypted.split_at(encrypted.len() - 12);
                vmk.decrypt(ct, nonce)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let rp_origin =
        std::env::var("SAO_RP_ORIGIN").unwrap_or_else(|_| "http://localhost:3100".to_string());
    let redirect_url = format!("{}/api/auth/oidc/callback", rp_origin);

    let config = crate::auth::oidc::OidcProviderConfig {
        id: provider.id,
        name: provider.name.clone(),
        issuer_url: provider.issuer_url,
        client_id: provider.client_id,
        client_secret,
        scopes: provider.scopes,
    };

    // Exchange code for tokens
    let user_info =
        match crate::auth::oidc::exchange_code(&config, &redirect_url, &query.code).await {
            Ok(info) => info,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e })),
                );
            }
        };

    // Find or create user
    let bootstrap_admin = is_bootstrap_admin(&user_info);

    let user_id = match crate::db::oidc::find_user_by_oidc(
        &state.inner.db,
        provider_id,
        &user_info.subject,
    )
    .await
    {
        Ok(Some(uid)) => uid,
        Ok(None) => {
            let username = user_info.email.as_deref().unwrap_or(&user_info.subject);
            let display_name = user_info.name.as_deref().or(user_info.email.as_deref());
            let role = if bootstrap_admin { "admin" } else { "user" };

            let existing_user =
                match crate::db::users::get_user_by_username(&state.inner.db, username).await {
                    Ok(user) => user,
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "error": format!("Failed to look up user: {}", e) })),
                        );
                    }
                };

            let uid = if let Some(existing_user) = existing_user {
                if bootstrap_admin && existing_user.role != "admin" {
                    if let Err(e) = crate::db::users::update_user_role(
                        &state.inner.db,
                        existing_user.id,
                        "admin",
                    )
                    .await
                    {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                json!({ "error": format!("Failed to promote bootstrap admin: {}", e) }),
                            ),
                        );
                    }
                }

                existing_user.id
            } else {
                match crate::db::users::create_user(&state.inner.db, username, display_name, role)
                    .await
                {
                    Ok(id) => id,
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "error": format!("Failed to create user: {}", e) })),
                        );
                    }
                }
            };

            // Link OIDC identity
            if let Err(e) = crate::db::oidc::link_user_to_oidc(
                &state.inner.db,
                uid,
                provider_id,
                &user_info.subject,
                user_info.email.as_deref(),
            )
            .await
            {
                tracing::error!("Failed to link OIDC identity: {}", e);
            }

            uid
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    // Load user for JWT
    if bootstrap_admin {
        if let Err(e) = crate::db::users::update_user_role(&state.inner.db, user_id, "admin").await
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to promote bootstrap admin: {}", e) })),
            );
        }
    }

    let user = match crate::db::users::get_user_by_id(&state.inner.db, user_id).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "User not found after OIDC link" })),
            );
        }
    };

    // Issue JWT
    let access_token = match session::create_access_token(
        user.id,
        &user.username,
        &user.role,
        &state.inner.jwt_secret,
    ) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to create token: {}", e) })),
            );
        }
    };

    let refresh_token = session::generate_refresh_token();
    let refresh_hash = session::hash_refresh_token(&refresh_token);
    let refresh_expires = Utc::now() + Duration::days(7);

    let _ = crate::db::sessions::store_refresh_token(
        &state.inner.db,
        user.id,
        &refresh_hash,
        refresh_expires,
    )
    .await;

    // Audit log
    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(user.id),
        None,
        "auth.login",
        Some("oidc"),
        Some(json!({ "provider": provider.name })),
        None,
        None,
    )
    .await;

    (
        StatusCode::OK,
        Json(json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "token_type": "Bearer",
            "expires_in": 1800,
            "user": {
                "id": user.id,
                "username": user.username,
                "display_name": user.display_name,
                "role": user.role,
            },
        })),
    )
}

fn is_bootstrap_admin(user_info: &crate::auth::oidc::OidcUserInfo) -> bool {
    let bootstrap_admin_oid = std::env::var("SAO_BOOTSTRAP_ADMIN_OID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let Some(bootstrap_admin_oid) = bootstrap_admin_oid else {
        return false;
    };

    user_info.oid.as_deref() == Some(bootstrap_admin_oid.as_str())
        || user_info.subject == bootstrap_admin_oid
}

#[cfg(test)]
mod tests {
    use super::is_bootstrap_admin;

    #[test]
    fn bootstrap_admin_matches_entra_oid_claim() {
        unsafe {
            std::env::set_var("SAO_BOOTSTRAP_ADMIN_OID", "admin-oid-123");
        }

        let user = crate::auth::oidc::OidcUserInfo {
            subject: "subject-1".to_string(),
            email: Some("user@example.com".to_string()),
            name: Some("User".to_string()),
            oid: Some("admin-oid-123".to_string()),
        };

        assert!(is_bootstrap_admin(&user));
    }
}
