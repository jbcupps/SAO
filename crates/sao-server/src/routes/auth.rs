use axum::{
    extract::{Extension, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use webauthn_rs::prelude::*;

use crate::auth::middleware::AuthUser;
use crate::auth::session;
use crate::security::{cookie_value, RequestAuditContext};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/webauthn/register/start", post(register_start))
        .route("/api/auth/webauthn/register/finish", post(register_finish))
        .route(
            "/api/auth/webauthn/local/register/start",
            post(local_register_start),
        )
        .route(
            "/api/auth/webauthn/local/register/finish",
            post(local_register_finish),
        )
        .route("/api/auth/webauthn/login/start", post(login_start))
        .route("/api/auth/webauthn/login/finish", post(login_finish))
        .route("/api/auth/refresh", post(refresh_token))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
}

#[derive(Deserialize)]
struct RegisterStartRequest {
    username: String,
}

#[derive(Deserialize)]
struct LocalRegisterStartRequest {
    username: Option<String>,
}

async fn local_register_start(
    State(state): State<AppState>,
    Json(req): Json<LocalRegisterStartRequest>,
) -> impl IntoResponse {
    if !local_bootstrap_enabled() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Local Windows Hello registration is disabled" })),
        );
    }

    let local_username = local_bootstrap_username();
    let username = req
        .username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&local_username);

    if username != local_username {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!("Local Windows Hello registration is only available for {local_username}")
            })),
        );
    }

    let user = match crate::db::users::get_user_by_username(&state.inner.db, username).await {
        Ok(Some(user)) if user.role == "admin" => user,
        Ok(Some(_)) => {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Local bootstrap user must be an admin" })),
            );
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "Local bootstrap user not found. Run bootstrap-local first."
                })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    start_registration_for_user(state, user).await
}

async fn local_register_finish(
    State(state): State<AppState>,
    Json(req): Json<RegisterFinishRequest>,
) -> impl IntoResponse {
    if !local_bootstrap_enabled() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Local Windows Hello registration is disabled" })),
        );
    }

    finish_registration_for_challenge(state, req).await
}

async fn register_start(
    State(state): State<AppState>,
    Json(req): Json<RegisterStartRequest>,
) -> impl IntoResponse {
    let username = req.username.trim();
    if username.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Username is required" })),
        );
    }

    let user = match crate::db::users::get_user_by_username(&state.inner.db, username).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "User not found" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    start_registration_for_user(state, user).await
}

#[derive(Deserialize)]
struct RegisterFinishRequest {
    challenge_id: String,
    credential: RegisterPublicKeyCredential,
}

async fn register_finish(
    State(state): State<AppState>,
    Json(req): Json<RegisterFinishRequest>,
) -> impl IntoResponse {
    finish_registration_for_challenge(state, req).await
}

async fn start_registration_for_user(
    state: AppState,
    user: crate::db::users::UserRow,
) -> (StatusCode, Json<serde_json::Value>) {
    let existing_creds =
        match crate::db::webauthn::get_credentials_for_user(&state.inner.db, user.id).await {
            Ok(creds) => creds
                .into_iter()
                .filter_map(|c| {
                    serde_json::from_value::<Passkey>(c.credential_json)
                        .ok()
                        .map(|pk| pk.cred_id().clone())
                })
                .collect::<Vec<_>>(),
            Err(_) => vec![],
        };

    let display_name = user.display_name.as_deref().unwrap_or(&user.username);
    match crate::auth::webauthn::start_registration(
        &state.inner.webauthn,
        user.id,
        &user.username,
        display_name,
        existing_creds,
    ) {
        Ok((challenge, reg_state)) => {
            let challenge_id = uuid::Uuid::new_v4().to_string();
            state
                .inner
                .security
                .challenge_store
                .insert_registration(challenge_id.clone(), user.id, reg_state)
                .await;

            (
                StatusCode::OK,
                Json(json!({
                    "challenge": challenge,
                    "challenge_id": challenge_id,
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("WebAuthn registration failed: {}", e) })),
        ),
    }
}

async fn finish_registration_for_challenge(
    state: AppState,
    req: RegisterFinishRequest,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some((user_id, reg_state)) = state
        .inner
        .security
        .challenge_store
        .consume_registration(&req.challenge_id)
        .await
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid or expired challenge" })),
        );
    };

    match crate::auth::webauthn::finish_registration(
        &state.inner.webauthn,
        &req.credential,
        &reg_state,
    ) {
        Ok(passkey) => {
            let cred_id = base64::Engine::encode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                passkey.cred_id().as_ref(),
            );
            let cred_json = match serde_json::to_value(&passkey) {
                Ok(value) => value,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": format!("Failed to encode credential: {}", e) })),
                    );
                }
            };

            if let Err(e) = crate::db::webauthn::store_credential(
                &state.inner.db,
                user_id,
                &cred_id,
                cred_json,
                None,
            )
            .await
            {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                );
            }

            (
                StatusCode::CREATED,
                Json(json!({ "status": "registered", "credential_id": cred_id })),
            )
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Registration verification failed: {}", e) })),
        ),
    }
}

fn local_bootstrap_enabled() -> bool {
    std::env::var("SAO_LOCAL_BOOTSTRAP")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn local_bootstrap_username() -> String {
    std::env::var("SAO_LOCAL_ADMIN_USERNAME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "local-admin".to_string())
}

#[derive(Deserialize)]
struct LoginStartRequest {
    username: Option<String>,
}

async fn login_start(
    State(state): State<AppState>,
    Json(req): Json<LoginStartRequest>,
) -> impl IntoResponse {
    let requested_username = req
        .username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let user = if let Some(username) = requested_username {
        match crate::db::users::get_user_by_username(&state.inner.db, username).await {
            Ok(Some(user)) => user,
            Ok(None) => {
                match crate::db::users::get_single_user_with_credentials(&state.inner.db).await {
                    Ok(Some(user)) => user,
                    Ok(None) => {
                        return (
                            StatusCode::NOT_FOUND,
                            Json(json!({ "error": "User not found" })),
                        );
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "error": e.to_string() })),
                        );
                    }
                }
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                );
            }
        }
    } else {
        match crate::db::users::get_single_user_with_credentials(&state.inner.db).await {
            Ok(Some(user)) => user,
            Ok(None) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": "Username is only optional when exactly one local Windows Hello account is registered"
                    })),
                );
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                );
            }
        }
    };

    let cred_rows =
        match crate::db::webauthn::get_credentials_for_user(&state.inner.db, user.id).await {
            Ok(rows) => rows,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                );
            }
        };

    if cred_rows.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "No credentials registered for this user" })),
        );
    }

    let passkeys: Vec<Passkey> = cred_rows
        .into_iter()
        .filter_map(|row| serde_json::from_value(row.credential_json).ok())
        .collect();

    match crate::auth::webauthn::start_authentication(&state.inner.webauthn, passkeys) {
        Ok((challenge, auth_state)) => {
            let challenge_id = uuid::Uuid::new_v4().to_string();
            state
                .inner
                .security
                .challenge_store
                .insert_authentication(challenge_id.clone(), user.id, auth_state)
                .await;

            (
                StatusCode::OK,
                Json(json!({
                    "challenge": challenge,
                    "challenge_id": challenge_id,
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("WebAuthn authentication failed: {}", e) })),
        ),
    }
}

#[derive(Deserialize)]
struct LoginFinishRequest {
    challenge_id: String,
    credential: PublicKeyCredential,
}

async fn login_finish(
    State(state): State<AppState>,
    Extension(context): Extension<RequestAuditContext>,
    Json(req): Json<LoginFinishRequest>,
) -> Response {
    let Some((user_id, auth_state)) = state
        .inner
        .security
        .challenge_store
        .consume_authentication(&req.challenge_id)
        .await
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid or expired challenge" })),
        )
            .into_response();
    };

    match crate::auth::webauthn::finish_authentication(
        &state.inner.webauthn,
        &req.credential,
        &auth_state,
    ) {
        Ok(_) => {
            let user = match crate::db::users::get_user_by_id(&state.inner.db, user_id).await {
                Ok(Some(u)) => u,
                _ => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "User not found" })),
                    )
                        .into_response();
                }
            };

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
                    )
                        .into_response();
                }
            };

            let refresh_token = session::generate_refresh_token();
            let refresh_hash = session::hash_refresh_token(&refresh_token);
            if let Err(e) = crate::db::sessions::store_refresh_token(
                &state.inner.db,
                user.id,
                &refresh_hash,
                session::refresh_token_expires_at(),
            )
            .await
            {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response();
            }

            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(user.id),
                None,
                "auth.login",
                Some("webauthn"),
                Some(json!({ "request_id": context.request_id })),
                context.client_ip.as_deref(),
                context.user_agent.as_deref(),
            )
            .await;

            authenticated_response(&state, &user, access_token, refresh_token, StatusCode::OK)
        }
        Err(e) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": format!("Authentication failed: {}", e) })),
        )
            .into_response(),
    }
}

async fn refresh_token(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let Some(refresh_token) = cookie_value(&headers, session::REFRESH_COOKIE_NAME) else {
        let mut response = (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Refresh session is missing" })),
        )
            .into_response();
        session::append_cleared_session_cookies(
            response.headers_mut(),
            &state.inner.security.cookie_config,
        );
        return response;
    };

    let token_hash = session::hash_refresh_token(&refresh_token);
    let user_id =
        match crate::db::sessions::validate_refresh_token(&state.inner.db, &token_hash).await {
            Ok(Some(uid)) => uid,
            Ok(None) => {
                let mut response = (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Invalid or expired refresh token" })),
                )
                    .into_response();
                session::append_cleared_session_cookies(
                    response.headers_mut(),
                    &state.inner.security.cookie_config,
                );
                return response;
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response();
            }
        };

    let _ = crate::db::sessions::revoke_refresh_token(&state.inner.db, &token_hash).await;

    let user = match crate::db::users::get_user_by_id(&state.inner.db, user_id).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "User not found" })),
            )
                .into_response();
        }
    };

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
            )
                .into_response();
        }
    };

    let new_refresh = session::generate_refresh_token();
    let new_hash = session::hash_refresh_token(&new_refresh);
    if let Err(e) = crate::db::sessions::store_refresh_token(
        &state.inner.db,
        user.id,
        &new_hash,
        session::refresh_token_expires_at(),
    )
    .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    let mut headers = HeaderMap::new();
    session::append_session_cookies(
        &mut headers,
        &state.inner.security.cookie_config,
        &access_token,
        &new_refresh,
    );

    (StatusCode::OK, headers, Json(json!({ "refreshed": true }))).into_response()
}

async fn logout(
    State(state): State<AppState>,
    Extension(context): Extension<RequestAuditContext>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(refresh_token) = cookie_value(&headers, session::REFRESH_COOKIE_NAME) {
        let token_hash = session::hash_refresh_token(&refresh_token);
        let _ = crate::db::sessions::revoke_refresh_token(&state.inner.db, &token_hash).await;
    }

    let mut response = (
        StatusCode::OK,
        Json(json!({
            "status": "logged_out",
            "request_id": context.request_id,
        })),
    )
        .into_response();
    session::append_cleared_session_cookies(
        response.headers_mut(),
        &state.inner.security.cookie_config,
    );
    response
}

async fn me(user: AuthUser, State(state): State<AppState>) -> impl IntoResponse {
    match crate::db::users::get_user_by_id(&state.inner.db, user.user_id).await {
        Ok(Some(row)) => Json(json!(row)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

fn authenticated_response(
    state: &AppState,
    user: &crate::db::users::UserRow,
    access_token: String,
    refresh_token: String,
    status: StatusCode,
) -> axum::response::Response {
    let mut headers = HeaderMap::new();
    session::append_session_cookies(
        &mut headers,
        &state.inner.security.cookie_config,
        &access_token,
        &refresh_token,
    );
    (
        status,
        headers,
        Json(json!({
            "authenticated": true,
            "user": {
                "id": user.id,
                "username": user.username,
                "display_name": user.display_name,
                "role": user.role,
            }
        })),
    )
        .into_response()
}
