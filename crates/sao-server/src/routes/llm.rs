//! POST /api/llm/generate — entity-token-authenticated proxy to a configured LLM provider.

use axum::{
    async_trait,
    extract::{FromRequestParts, State},
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::agent_tokens;
use crate::llm::{self, GenerateRequest};
use crate::security::RequestAuditContext;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/llm/generate", post(generate_handler))
}

#[derive(Debug, Clone)]
struct EntityCaller {
    agent_id: Uuid,
    human_owner: Uuid,
}

#[async_trait]
impl FromRequestParts<AppState> for EntityCaller {
    type Rejection = (StatusCode, Json<Value>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Bearer entity token required" })),
                )
            })?;

        let claims =
            agent_tokens::validate_entity_token(&state.inner.db, &state.inner.jwt_secret, token)
                .await
                .map_err(|_| {
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(json!({ "error": "Invalid or revoked entity token" })),
                    )
                })?;

        let agent_id = claims.agent_id().map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid agent_id in token" })),
            )
        })?;
        let human_owner = claims.human_owner_id().map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid human_owner in token" })),
            )
        })?;

        Ok(Self {
            agent_id,
            human_owner,
        })
    }
}

async fn generate_handler(
    caller: EntityCaller,
    State(state): State<AppState>,
    axum::extract::Extension(context): axum::extract::Extension<RequestAuditContext>,
    Json(req): Json<GenerateRequest>,
) -> (StatusCode, Json<Value>) {
    if req.prompt.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "prompt is required" })),
        );
    }

    match llm::dispatch(&state, &req).await {
        Ok(resp) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(caller.human_owner),
                Some(caller.agent_id),
                "llm.generate",
                Some("llm"),
                Some(json!({
                    "provider": req.provider,
                    "model": req.model,
                    "role": req.role,
                    "latency_ms": resp.latency_ms,
                    "request_id": context.request_id,
                })),
                context.client_ip.as_deref(),
                context.user_agent.as_deref(),
            )
            .await;
            (StatusCode::OK, Json(json!(resp)))
        }
        Err(e) => {
            let status = match &e {
                llm::LlmError::ProviderDisabled(_)
                | llm::LlmError::ProviderUnconfigured(_)
                | llm::LlmError::ModelNotApproved { .. } => StatusCode::BAD_REQUEST,
                llm::LlmError::VaultSealed => StatusCode::SERVICE_UNAVAILABLE,
                llm::LlmError::ProviderError { .. }
                | llm::LlmError::Http(_)
                | llm::LlmError::BadResponse(_) => StatusCode::BAD_GATEWAY,
                llm::LlmError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(caller.human_owner),
                Some(caller.agent_id),
                "llm.generate.failed",
                Some("llm"),
                Some(json!({
                    "provider": req.provider,
                    "model": req.model,
                    "error": e.to_string(),
                    "request_id": context.request_id,
                })),
                context.client_ip.as_deref(),
                context.user_agent.as_deref(),
            )
            .await;
            (status, Json(json!({ "error": e.to_string() })))
        }
    }
}
