use axum::{
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::auth::session;
use crate::security::RequestAuditContext;
use crate::state::AppState;

const ENV_PROVIDER_ID: &str = "entra";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/oidc/providers", get(list_providers))
        .route("/api/auth/oidc/:provider_id/authorize", get(authorize))
        .route("/api/auth/oidc/callback", get(callback))
}

#[derive(Debug, Clone)]
struct ResolvedProvider {
    key: String,
    display_name: String,
    config: crate::auth::oidc::OidcProviderConfig,
}

#[derive(Debug, Clone, Serialize)]
struct PublicProvider {
    id: String,
    name: String,
    enabled: bool,
}

async fn list_providers(State(state): State<AppState>) -> impl IntoResponse {
    let mut providers = Vec::<PublicProvider>::new();
    if let Some(provider) = env_provider() {
        providers.push(PublicProvider {
            id: provider.key,
            name: provider.display_name,
            enabled: true,
        });
    }

    match crate::db::oidc::list_providers_public(&state.inner.db).await {
        Ok(db_providers) => {
            providers.extend(db_providers.into_iter().map(|provider| PublicProvider {
                id: provider.id.to_string(),
                name: provider.name,
                enabled: provider.enabled,
            }));
            (StatusCode::OK, Json(json!({ "providers": providers }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn authorize(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Result<Redirect, (StatusCode, Json<serde_json::Value>)> {
    let provider = resolve_provider(&state, &provider_id).await?;
    let redirect_url = oidc_redirect_url(&state);
    let result = crate::auth::oidc::start_authorization(&provider.config, &redirect_url)
        .await
        .map_err(internal_error)?;

    state
        .inner
        .security
        .challenge_store
        .insert_oidc_state(
            result.csrf_token.secret().to_string(),
            provider.key,
            result.nonce.secret().to_string(),
        )
        .await;

    Ok(Redirect::temporary(result.auth_url.as_str()))
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

async fn callback(
    State(state): State<AppState>,
    Extension(context): Extension<RequestAuditContext>,
    Query(query): Query<CallbackQuery>,
) -> impl IntoResponse {
    let Some((provider_key, nonce)) = state
        .inner
        .security
        .challenge_store
        .consume_oidc_state(&query.state)
        .await
    else {
        return redirect_with_error(&state, "invalid-or-expired-oidc-state");
    };

    let provider = match resolve_provider(&state, &provider_key).await {
        Ok(provider) => provider,
        Err((status, body)) => return (status, body).into_response(),
    };

    let user_info = match crate::auth::oidc::exchange_code(
        &provider.config,
        &oidc_redirect_url(&state),
        &query.code,
        &nonce,
    )
    .await
    {
        Ok(info) => info,
        Err(error) => {
            tracing::warn!(
                request_id = %context.request_id,
                provider = %provider.display_name,
                error = %error,
                "OIDC callback failed"
            );
            return redirect_with_error(&state, "oidc-sign-in-failed");
        }
    };

    let bootstrap_admin = is_bootstrap_admin(&user_info);
    let user_id = match load_or_create_user(&state, &provider, &user_info, bootstrap_admin).await {
        Ok(user_id) => user_id,
        Err(error) => {
            tracing::error!(
                request_id = %context.request_id,
                provider = %provider.display_name,
                error = %error,
                "Failed to load or create OIDC user"
            );
            return redirect_with_error(&state, "oidc-user-provisioning-failed");
        }
    };

    let user = match crate::db::users::get_user_by_id(&state.inner.db, user_id).await {
        Ok(Some(user)) => user,
        _ => return redirect_with_error(&state, "oidc-user-provisioning-failed"),
    };

    let access_token = match session::create_access_token(
        user.id,
        &user.username,
        &user.role,
        &state.inner.jwt_secret,
    ) {
        Ok(token) => token,
        Err(_) => return redirect_with_error(&state, "oidc-session-creation-failed"),
    };
    let refresh_token = session::generate_refresh_token();
    let refresh_hash = session::hash_refresh_token(&refresh_token);
    if crate::db::sessions::store_refresh_token(
        &state.inner.db,
        user.id,
        &refresh_hash,
        session::refresh_token_expires_at(),
    )
    .await
    .is_err()
    {
        return redirect_with_error(&state, "oidc-session-creation-failed");
    }

    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(user.id),
        None,
        "auth.login",
        Some("oidc"),
        Some(json!({
            "provider": provider.display_name,
            "request_id": context.request_id,
        })),
        context.client_ip.as_deref(),
        context.user_agent.as_deref(),
    )
    .await;

    let mut headers = HeaderMap::new();
    session::append_session_cookies(
        &mut headers,
        &state.inner.security.cookie_config,
        &access_token,
        &refresh_token,
    );
    let location = format!("{}/", state.inner.security.frontend_origin());
    if let Ok(value) = axum::http::HeaderValue::from_str(&location) {
        headers.insert(axum::http::header::LOCATION, value);
    }
    (StatusCode::SEE_OTHER, headers).into_response()
}

async fn resolve_provider(
    state: &AppState,
    provider_key: &str,
) -> Result<ResolvedProvider, (StatusCode, Json<serde_json::Value>)> {
    if provider_key == ENV_PROVIDER_ID {
        return env_provider().ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "OIDC provider not configured" })),
            )
        });
    }

    let provider_id = uuid::Uuid::parse_str(provider_key).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid provider identifier" })),
        )
    })?;
    let provider = crate::db::oidc::get_provider(&state.inner.db, provider_id)
        .await
        .map_err(internal_error)?
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

    let client_secret = if let Some(ref encrypted) = provider.client_secret_encrypted {
        let vs = state.inner.vault_state.read().await;
        if let Some(vmk) = vs.vmk() {
            if encrypted.len() > 12 {
                let (ct, nonce) = encrypted.split_at(encrypted.len() - 12);
                vmk.decrypt(ct, nonce)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
            } else {
                None
            }
        } else {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    json!({ "error": "Vault must be unsealed to use database-backed OIDC providers" }),
                ),
            ));
        }
    } else {
        None
    };

    Ok(ResolvedProvider {
        key: provider.id.to_string(),
        display_name: provider.name.clone(),
        config: crate::auth::oidc::OidcProviderConfig {
            id: provider.id,
            name: provider.name,
            issuer_url: provider.issuer_url,
            client_id: provider.client_id,
            client_secret,
            scopes: provider.scopes,
        },
    })
}

fn env_provider() -> Option<ResolvedProvider> {
    let issuer_url = std::env::var("SAO_OIDC_ISSUER_URL").ok()?;
    let client_id = std::env::var("SAO_OIDC_CLIENT_ID").ok()?;
    let client_secret = std::env::var("SAO_OIDC_CLIENT_SECRET")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let name = std::env::var("SAO_OIDC_PROVIDER_NAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Microsoft Entra ID".to_string());
    let scopes = std::env::var("SAO_OIDC_SCOPES")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "openid profile email".to_string());

    Some(ResolvedProvider {
        key: ENV_PROVIDER_ID.to_string(),
        display_name: name.clone(),
        config: crate::auth::oidc::OidcProviderConfig {
            id: uuid::Uuid::nil(),
            name,
            issuer_url,
            client_id,
            client_secret,
            scopes,
        },
    })
}

async fn load_or_create_user(
    state: &AppState,
    provider: &ResolvedProvider,
    user_info: &crate::auth::oidc::OidcUserInfo,
    bootstrap_admin: bool,
) -> Result<uuid::Uuid, String> {
    if provider.key != ENV_PROVIDER_ID {
        if let Ok(provider_id) = uuid::Uuid::parse_str(&provider.key) {
            if let Some(uid) =
                crate::db::oidc::find_user_by_oidc(&state.inner.db, provider_id, &user_info.subject)
                    .await
                    .map_err(|e| e.to_string())?
            {
                if bootstrap_admin {
                    crate::db::users::update_user_role(&state.inner.db, uid, "admin")
                        .await
                        .map_err(|e| e.to_string())?;
                }
                return Ok(uid);
            }
        }
    }

    let username = user_info.email.as_deref().unwrap_or(&user_info.subject);
    let display_name = user_info.name.as_deref().or(user_info.email.as_deref());
    let existing_user = crate::db::users::get_user_by_username(&state.inner.db, username)
        .await
        .map_err(|e| e.to_string())?;

    let user_id = if let Some(existing_user) = existing_user {
        if bootstrap_admin && existing_user.role != "admin" {
            crate::db::users::update_user_role(&state.inner.db, existing_user.id, "admin")
                .await
                .map_err(|e| e.to_string())?;
        }
        existing_user.id
    } else {
        crate::db::users::create_user(
            &state.inner.db,
            username,
            display_name,
            if bootstrap_admin { "admin" } else { "user" },
        )
        .await
        .map_err(|e| e.to_string())?
    };

    if provider.key != ENV_PROVIDER_ID {
        if let Ok(provider_id) = uuid::Uuid::parse_str(&provider.key) {
            let _ = crate::db::oidc::link_user_to_oidc(
                &state.inner.db,
                user_id,
                provider_id,
                &user_info.subject,
                user_info.email.as_deref(),
            )
            .await;
        }
    }

    if bootstrap_admin {
        let _ = crate::db::users::update_user_role(&state.inner.db, user_id, "admin").await;
    }

    Ok(user_id)
}

fn oidc_redirect_url(state: &AppState) -> String {
    format!(
        "{}/api/auth/oidc/callback",
        state.inner.security.frontend_origin()
    )
}

fn redirect_with_error(state: &AppState, error_code: &str) -> axum::response::Response {
    let location = format!(
        "{}/login?error={}",
        state.inner.security.frontend_origin(),
        url::form_urlencoded::byte_serialize(error_code.as_bytes()).collect::<String>()
    );
    Redirect::to(&location).into_response()
}

fn internal_error(error: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": error.to_string() })),
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
    use super::{env_provider, is_bootstrap_admin, ENV_PROVIDER_ID};

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

    #[test]
    fn env_provider_requires_issuer_and_client_id() {
        unsafe {
            std::env::remove_var("SAO_OIDC_ISSUER_URL");
            std::env::remove_var("SAO_OIDC_CLIENT_ID");
        }
        assert!(env_provider().is_none());

        unsafe {
            std::env::set_var("SAO_OIDC_ISSUER_URL", "https://issuer.example.com");
            std::env::set_var("SAO_OIDC_CLIENT_ID", "client-123");
        }
        let provider = env_provider().expect("expected env-backed provider");
        assert_eq!(provider.key, ENV_PROVIDER_ID);
    }
}
