use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use webauthn_rs::prelude::*;

use crate::auth::middleware::AuthUser;
use crate::auth::session;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/webauthn/register/start", post(register_start))
        .route("/api/auth/webauthn/register/finish", post(register_finish))
        .route("/api/auth/webauthn/login/start", post(login_start))
        .route("/api/auth/webauthn/login/finish", post(login_finish))
        .route("/api/auth/refresh", post(refresh_token))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
}

// --- Registration ---

#[derive(Deserialize)]
struct RegisterStartRequest {
    username: String,
}

async fn register_start(
    State(state): State<AppState>,
    Json(req): Json<RegisterStartRequest>,
) -> (StatusCode, Json<Value>) {
    // Look up or fail if user doesn't exist
    let user = match crate::db::users::get_user_by_username(&state.inner.db, &req.username).await {
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

    // Get existing credential IDs for exclusion
    let existing_creds = match crate::db::webauthn::get_credentials_for_user(
        &state.inner.db,
        user.id,
    )
    .await
    {
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
            // Store challenge state in DB
            let challenge_id = uuid::Uuid::new_v4().to_string();
            let reg_state_json = serde_json::to_value(&reg_state).unwrap();

            if let Err(e) = crate::db::webauthn::store_challenge(
                &state.inner.db,
                &challenge_id,
                reg_state_json,
                "registration",
                Some(user.id),
            )
            .await
            {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                );
            }

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

#[derive(Deserialize)]
struct RegisterFinishRequest {
    challenge_id: String,
    credential: RegisterPublicKeyCredential,
}

async fn register_finish(
    State(state): State<AppState>,
    Json(req): Json<RegisterFinishRequest>,
) -> (StatusCode, Json<Value>) {
    // Retrieve challenge state
    let (challenge_json, user_id) = match crate::db::webauthn::consume_challenge(
        &state.inner.db,
        &req.challenge_id,
    )
    .await
    {
        Ok(Some(data)) => data,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid or expired challenge" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    let user_id = match user_id {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Challenge has no associated user" })),
            );
        }
    };

    let reg_state: PasskeyRegistration = match serde_json::from_value(challenge_json) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Invalid challenge state: {}", e) })),
            );
        }
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
            let cred_json = serde_json::to_value(&passkey).unwrap();

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

// --- Authentication ---

#[derive(Deserialize)]
struct LoginStartRequest {
    username: String,
}

async fn login_start(
    State(state): State<AppState>,
    Json(req): Json<LoginStartRequest>,
) -> (StatusCode, Json<Value>) {
    let user = match crate::db::users::get_user_by_username(&state.inner.db, &req.username).await {
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

    // Load user's passkeys
    let cred_rows = match crate::db::webauthn::get_credentials_for_user(
        &state.inner.db,
        user.id,
    )
    .await
    {
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
            let auth_state_json = serde_json::to_value(&auth_state).unwrap();

            if let Err(e) = crate::db::webauthn::store_challenge(
                &state.inner.db,
                &challenge_id,
                auth_state_json,
                "authentication",
                Some(user.id),
            )
            .await
            {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                );
            }

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
    Json(req): Json<LoginFinishRequest>,
) -> (StatusCode, Json<Value>) {
    let (challenge_json, user_id) = match crate::db::webauthn::consume_challenge(
        &state.inner.db,
        &req.challenge_id,
    )
    .await
    {
        Ok(Some(data)) => data,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid or expired challenge" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    let user_id = match user_id {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Challenge has no associated user" })),
            );
        }
    };

    let auth_state: PasskeyAuthentication = match serde_json::from_value(challenge_json) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Invalid challenge state: {}", e) })),
            );
        }
    };

    match crate::auth::webauthn::finish_authentication(
        &state.inner.webauthn,
        &req.credential,
        &auth_state,
    ) {
        Ok(_auth_result) => {
            // Load user for JWT claims
            let user = match crate::db::users::get_user_by_id(&state.inner.db, user_id).await {
                Ok(Some(u)) => u,
                _ => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "User not found" })),
                    );
                }
            };

            // Create JWT
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

            // Create refresh token
            let refresh_token = session::generate_refresh_token();
            let refresh_hash = session::hash_refresh_token(&refresh_token);
            let refresh_expires = Utc::now() + Duration::days(7);

            if let Err(e) = crate::db::sessions::store_refresh_token(
                &state.inner.db,
                user.id,
                &refresh_hash,
                refresh_expires,
            )
            .await
            {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                );
            }

            // Audit log
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(user.id),
                None,
                "auth.login",
                Some("webauthn"),
                None,
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
        Err(e) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": format!("Authentication failed: {}", e) })),
        ),
    }
}

// --- Token refresh ---

#[derive(Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

async fn refresh_token(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> (StatusCode, Json<Value>) {
    let token_hash = session::hash_refresh_token(&req.refresh_token);

    let user_id = match crate::db::sessions::validate_refresh_token(
        &state.inner.db,
        &token_hash,
    )
    .await
    {
        Ok(Some(uid)) => uid,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid or expired refresh token" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    // Revoke old refresh token
    let _ = crate::db::sessions::revoke_refresh_token(&state.inner.db, &token_hash).await;

    // Load user
    let user = match crate::db::users::get_user_by_id(&state.inner.db, user_id).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "User not found" })),
            );
        }
    };

    // Issue new tokens
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

    let new_refresh = session::generate_refresh_token();
    let new_hash = session::hash_refresh_token(&new_refresh);
    let refresh_expires = Utc::now() + Duration::days(7);

    if let Err(e) =
        crate::db::sessions::store_refresh_token(&state.inner.db, user.id, &new_hash, refresh_expires)
            .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        );
    }

    (
        StatusCode::OK,
        Json(json!({
            "access_token": access_token,
            "refresh_token": new_refresh,
            "token_type": "Bearer",
            "expires_in": 1800,
        })),
    )
}

// --- Logout ---

#[derive(Deserialize)]
struct LogoutRequest {
    refresh_token: String,
}

async fn logout(
    State(state): State<AppState>,
    Json(req): Json<LogoutRequest>,
) -> (StatusCode, Json<Value>) {
    let token_hash = session::hash_refresh_token(&req.refresh_token);
    let _ = crate::db::sessions::revoke_refresh_token(&state.inner.db, &token_hash).await;
    (StatusCode::OK, Json(json!({ "status": "logged_out" })))
}

// --- Current user ---

async fn me(user: AuthUser) -> Json<Value> {
    Json(json!({
        "id": user.user_id,
        "username": user.username,
        "role": user.role,
    }))
}
