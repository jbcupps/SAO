use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts, HeaderMap, StatusCode},
    Json,
};
use serde_json::json;
use uuid::Uuid;

use crate::auth::session;
use crate::security::{cookie_value, RequestAuditContext};
use crate::state::AppState;

/// Authenticated user info extracted from JWT.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub role: String,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

/// Axum extractor that requires a valid JWT Bearer token.
#[axum::async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        let token = extract_bearer_or_cookie(auth_header, &parts.headers).ok_or_else(|| {
            log_auth_failure(parts, "Missing session token");
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Authentication required" })),
            )
        })?;

        let claims = session::validate_token(&token, &state.inner.jwt_secret).map_err(|_| {
            log_auth_failure(parts, "Invalid or expired session token");
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid or expired token" })),
            )
        })?;

        let user_id = Uuid::parse_str(&claims.sub).map_err(|_| {
            log_auth_failure(parts, "Invalid token subject");
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid token subject" })),
            )
        })?;

        let user = crate::db::users::get_user_by_id(&state.inner.db, user_id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    user_id = %user_id,
                    error = %error,
                    "Failed to load current user role"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Failed to load current user" })),
                )
            })?
            .ok_or_else(|| {
                log_auth_failure(parts, "Session user no longer exists");
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Session user no longer exists" })),
                )
            })?;

        Ok(AuthUser {
            user_id,
            role: user.role,
        })
    }
}

/// Axum extractor that requires admin role.
#[derive(Debug, Clone)]
pub struct AdminUser(pub AuthUser);

#[axum::async_trait]
impl FromRequestParts<AppState> for AdminUser {
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if !user.is_admin() {
            log_auth_failure(parts, "Administrator access required");
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Administrator access required" })),
            ));
        }
        Ok(AdminUser(user))
    }
}

fn extract_bearer_or_cookie<'a>(
    auth_header: Option<&'a str>,
    headers: &'a HeaderMap,
) -> Option<String> {
    if let Some(auth_header) = auth_header {
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            return Some(token.to_string());
        }
    }

    cookie_value(headers, session::ACCESS_COOKIE_NAME)
}

fn log_auth_failure(parts: &Parts, reason: &str) {
    let context = parts.extensions.get::<RequestAuditContext>();
    tracing::warn!(
        request_id = context.map(|ctx| ctx.request_id.as_str()).unwrap_or("unknown"),
        client_ip = ?context.and_then(|ctx| ctx.client_ip.as_deref()),
        user_agent = ?context.and_then(|ctx| ctx.user_agent.as_deref()),
        reason = reason,
        "Authentication or authorization failure"
    );
}
