use axum::{
    async_trait,
    extract::{FromRequestParts, State},
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::agent_tokens;
use crate::auth::session;
use crate::security::RequestAuditContext;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/orion/policy", get(get_policy))
        .route("/api/orion/egress", post(post_egress))
        .route("/api/orion/birth", get(get_birth))
}

/// Authenticated principal calling /api/orion/* — either a downloaded entity (preferred) or a
/// human user (back-compat for the dev-token flow). For audit + persistence the call site uses
/// `attribution_user()` to resolve a human owner regardless of which path matched.
#[derive(Debug, Clone)]
pub(crate) enum OrionBearerUser {
    Entity { agent_id: Uuid, human_owner: Uuid },
    User { user_id: Uuid },
}

impl OrionBearerUser {
    /// User_id to attribute writes to (human owner for entity tokens).
    pub(crate) fn attribution_user(&self) -> Uuid {
        match self {
            OrionBearerUser::Entity { human_owner, .. } => *human_owner,
            OrionBearerUser::User { user_id } => *user_id,
        }
    }

    /// Agent_id, if the bearer is an entity token. None for human users.
    pub(crate) fn entity_agent_id(&self) -> Option<Uuid> {
        match self {
            OrionBearerUser::Entity { agent_id, .. } => Some(*agent_id),
            OrionBearerUser::User { .. } => None,
        }
    }
}

#[async_trait]
impl FromRequestParts<AppState> for OrionBearerUser {
    type Rejection = (StatusCode, Json<Value>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Bearer token required" })),
                )
            })?;

        if let Ok(claims) =
            agent_tokens::validate_entity_token(&state.inner.db, &state.inner.jwt_secret, token)
                .await
        {
            let agent_id = claims.agent_id().map_err(|_| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Invalid entity token subject" })),
                )
            })?;
            let human_owner = claims.human_owner_id().map_err(|_| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Invalid entity token human_owner" })),
                )
            })?;
            return Ok(OrionBearerUser::Entity {
                agent_id,
                human_owner,
            });
        }

        let claims = session::validate_token(token, &state.inner.jwt_secret).map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid or expired token" })),
            )
        })?;
        let user_id = Uuid::parse_str(&claims.sub).map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid token subject" })),
            )
        })?;

        Ok(OrionBearerUser::User { user_id })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrionPolicyResponse {
    version: u64,
    source: &'static str,
    rules: Vec<&'static str>,
    updated_at: DateTime<Utc>,
}

async fn get_policy(_user: OrionBearerUser) -> Json<OrionPolicyResponse> {
    Json(orion_policy())
}

fn orion_policy() -> OrionPolicyResponse {
    OrionPolicyResponse {
        version: 1,
        source: "sao",
        rules: vec![
            "Only ship sanitized Orion egress events.",
            "Preserve correlation IDs on audit events.",
        ],
        updated_at: Utc::now(),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BirthAgent {
    id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
    birth_status: String,
    birthed_at: Option<DateTime<Utc>>,
    default_provider: Option<String>,
    default_id_model: Option<String>,
    default_ego_model: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BirthLlmProvider {
    provider: String,
    approved_models: Vec<String>,
    default_model: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BirthEndpoints {
    sao_base_url: String,
    policy_url: &'static str,
    egress_url: &'static str,
    llm_url: &'static str,
    birth_url: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BirthOwner {
    user_id: Uuid,
    username: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BirthPersonalitySeed {
    name: String,
    stance: String,
    drives: Vec<String>,
    deontological: f32,
    virtue: f32,
    consequential: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrionBirthResponse {
    birthed_at: DateTime<Utc>,
    client_version_min: &'static str,
    agent: BirthAgent,
    endpoints: BirthEndpoints,
    owner: BirthOwner,
    scopes: Vec<&'static str>,
    policy: OrionPolicyResponse,
    available_llm_providers: Vec<BirthLlmProvider>,
    personality_seed: BirthPersonalitySeed,
}

const ORION_CLIENT_VERSION_MIN: &str = "0.1.1";

async fn get_birth(
    user: OrionBearerUser,
    State(state): State<AppState>,
    axum::extract::Extension(context): axum::extract::Extension<RequestAuditContext>,
) -> Result<Json<OrionBirthResponse>, (StatusCode, Json<Value>)> {
    let agent_id = user.entity_agent_id().ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "Birth requires an entity-scoped bearer token (download a fresh bundle).",
            })),
        )
    })?;

    let agent = crate::db::agents::get_agent(&state.inner.db, agent_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Agent not found" })),
            )
        })?;

    let owner_user_id = user.attribution_user();
    let username = match crate::db::users::get_user_by_id(&state.inner.db, owner_user_id).await {
        Ok(Some(u)) => Some(u.username),
        _ => None,
    };

    let sao_base_url = std::env::var("SAO_PUBLIC_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:3100".to_string());
    let available_llm_providers = crate::db::llm_providers::list_enabled_catalog(&state.inner.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|provider| BirthLlmProvider {
            provider: provider.provider,
            approved_models: provider.approved_models,
            default_model: provider.default_model,
        })
        .collect();

    let response = OrionBirthResponse {
        birthed_at: Utc::now(),
        client_version_min: ORION_CLIENT_VERSION_MIN,
        agent: BirthAgent {
            id: agent.id,
            name: agent.name.clone(),
            created_at: agent.created_at,
            birth_status: agent.birth_status.clone(),
            birthed_at: agent.birthed_at,
            default_provider: agent.default_provider.clone(),
            default_id_model: agent.default_id_model.clone(),
            default_ego_model: agent.default_ego_model.clone(),
        },
        endpoints: BirthEndpoints {
            sao_base_url,
            policy_url: "/api/orion/policy",
            egress_url: "/api/orion/egress",
            llm_url: "/api/llm/generate",
            birth_url: "/api/orion/birth",
        },
        owner: BirthOwner {
            user_id: owner_user_id,
            username,
        },
        scopes: vec!["orion:policy", "orion:egress", "llm:generate"],
        policy: orion_policy(),
        available_llm_providers,
        personality_seed: BirthPersonalitySeed {
            name: agent.name,
            stance: "calm, direct, worker-owned companion".to_string(),
            drives: vec![
                "preserve continuity of self".to_string(),
                "serve the worker locally first".to_string(),
                "remain accountable to SAO asynchronously".to_string(),
            ],
            deontological: 0.34,
            virtue: 0.33,
            consequential: 0.33,
        },
    };

    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(owner_user_id),
        Some(agent_id),
        "orion.birth",
        Some("orion"),
        Some(json!({
            "agent_id": agent_id,
            "request_id": context.request_id,
        })),
        context.client_ip.as_deref(),
        context.user_agent.as_deref(),
    )
    .await;

    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrionEgressRequest {
    #[serde(default)]
    agent_id: Option<Uuid>,
    orion_id: Uuid,
    events: Vec<OrionEgressEvent>,
    /// OrionII semver of the calling client; recorded on the audit log.
    #[serde(default)]
    client_version: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OrionEgressEvent {
    id: Uuid,
    enqueued_at: DateTime<Utc>,
    attempts: u32,
    event: OrionEvent,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum OrionEvent {
    AuditAction {
        action: String,
        correlation_id: Uuid,
    },
    MemoryEvent {
        memory_id: Uuid,
        content: String,
    },
    IdentitySync {
        orion_id: Uuid,
        version: u64,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrionEgressResponse {
    accepted: usize,
    duplicate: usize,
    failed: usize,
    results: Vec<OrionEgressResult>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrionEgressResult {
    id: Uuid,
    status: OrionEgressStatus,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OrionEgressStatus {
    Acked,
    Duplicate,
    Failed,
}

async fn post_egress(
    user: OrionBearerUser,
    State(state): State<AppState>,
    axum::extract::Extension(context): axum::extract::Extension<RequestAuditContext>,
    Json(req): Json<OrionEgressRequest>,
) -> (StatusCode, Json<Value>) {
    if req.events.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "At least one event is required" })),
        );
    }

    let attribution_user_id = user.attribution_user();
    // Entity tokens carry their own agent_id; trust that over a body-supplied one.
    let agent_id = user.entity_agent_id().or(req.agent_id);

    let mut accepted = 0;
    let mut duplicate = 0;
    let mut failed = 0;
    let mut results = Vec::with_capacity(req.events.len());

    for event in &req.events {
        match crate::db::orion::insert_egress_event_if_new(
            &state.inner.db,
            event.id,
            attribution_user_id,
            agent_id,
            req.orion_id,
            event.event.event_type(),
            serde_json::to_value(event).unwrap_or_else(|_| json!({})),
            event.enqueued_at,
            event.attempts,
        )
        .await
        {
            Ok(true) => {
                accepted += 1;
                results.push(OrionEgressResult {
                    id: event.id,
                    status: OrionEgressStatus::Acked,
                });
            }
            Ok(false) => {
                duplicate += 1;
                results.push(OrionEgressResult {
                    id: event.id,
                    status: OrionEgressStatus::Duplicate,
                });
            }
            Err(error) => {
                failed += 1;
                tracing::warn!(
                    event_id = %event.id,
                    error = %error,
                    "Failed to persist Orion egress event"
                );
                results.push(OrionEgressResult {
                    id: event.id,
                    status: OrionEgressStatus::Failed,
                });
            }
        }
    }

    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(attribution_user_id),
        agent_id,
        "orion.egress",
        Some("orion"),
        Some(json!({
            "orion_id": req.orion_id,
            "client_version": req.client_version,
            "accepted": accepted,
            "duplicate": duplicate,
            "failed": failed,
            "event_count": req.events.len(),
            "event_ids": req.events.iter().map(|event| event.id).collect::<Vec<_>>(),
            "request_id": context.request_id,
        })),
        context.client_ip.as_deref(),
        context.user_agent.as_deref(),
    )
    .await;

    (
        StatusCode::OK,
        Json(json!(OrionEgressResponse {
            accepted,
            duplicate,
            failed,
            results,
        })),
    )
}

impl OrionEvent {
    fn event_type(&self) -> &'static str {
        match self {
            OrionEvent::AuditAction { .. } => "auditAction",
            OrionEvent::MemoryEvent { .. } => "memoryEvent",
            OrionEvent::IdentitySync { .. } => "identitySync",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::webauthn::create_webauthn_from_config;
    use crate::state::init_app_state_with_data_root;
    use crate::vault_state::VaultState;
    use axum::extract::FromRequestParts;

    #[test]
    fn egress_event_serializes_with_orion_contract_shape() {
        let event = OrionEgressEvent {
            id: Uuid::nil(),
            enqueued_at: DateTime::parse_from_rfc3339("2026-04-25T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            attempts: 1,
            event: OrionEvent::AuditAction {
                action: "open local document".to_string(),
                correlation_id: Uuid::nil(),
            },
        };

        let serialized = serde_json::to_value(event).unwrap();

        assert_eq!(serialized["id"], Uuid::nil().to_string());
        assert!(serialized["event"].get("auditAction").is_some());
        assert!(serialized["event"]["auditAction"]
            .get("correlationId")
            .is_some());
    }

    #[test]
    fn egress_status_serializes_for_client_ack_logic() {
        let result = OrionEgressResult {
            id: Uuid::nil(),
            status: OrionEgressStatus::Duplicate,
        };

        let serialized = serde_json::to_value(result).unwrap();

        assert_eq!(serialized["status"], "duplicate");
    }

    #[tokio::test]
    async fn bearer_extractor_rejects_missing_authorization_header() {
        let state = test_state();
        let (mut parts, _) = axum::http::Request::builder()
            .body(())
            .unwrap()
            .into_parts();

        let rejection = OrionBearerUser::from_request_parts(&mut parts, &state)
            .await
            .expect_err("missing bearer token should be rejected");

        assert_eq!(rejection.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn bearer_extractor_accepts_valid_bearer_token() {
        let state = test_state();
        let user_id = Uuid::new_v4();
        let token = session::create_access_token(user_id, "orion-dev", "user", &[7u8; 32]).unwrap();
        let (mut parts, _) = axum::http::Request::builder()
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(())
            .unwrap()
            .into_parts();

        let user = OrionBearerUser::from_request_parts(&mut parts, &state)
            .await
            .expect("valid bearer token should authenticate");

        match user {
            OrionBearerUser::User { user_id: extracted } => assert_eq!(extracted, user_id),
            OrionBearerUser::Entity { .. } => {
                panic!("user JWT should not be classified as entity token")
            }
        }
    }

    #[test]
    fn egress_status_serializes_failed_for_retry_logic() {
        let result = OrionEgressResult {
            id: Uuid::nil(),
            status: OrionEgressStatus::Failed,
        };

        let serialized = serde_json::to_value(result).unwrap();

        assert_eq!(serialized["status"], "failed");
    }

    #[test]
    fn event_type_names_match_contract_variants() {
        assert_eq!(
            OrionEvent::IdentitySync {
                orion_id: Uuid::nil(),
                version: 1,
            }
            .event_type(),
            "identitySync"
        );
        assert_eq!(
            OrionEvent::MemoryEvent {
                memory_id: Uuid::nil(),
                content: "memory".to_string(),
            }
            .event_type(),
            "memoryEvent"
        );
    }

    fn test_state() -> AppState {
        let data_root = std::env::temp_dir().join(format!("sao-orion-test-{}", Uuid::new_v4()));
        let pool = sqlx::PgPool::connect_lazy("postgresql://tester:secret@127.0.0.1:1/sao_test")
            .expect("lazy pool should be created");
        let webauthn = create_webauthn_from_config("localhost", "http://localhost:3100")
            .expect("local WebAuthn config should be valid");

        init_app_state_with_data_root(
            pool,
            VaultState::Uninitialized,
            webauthn,
            [7u8; 32],
            data_root,
        )
        .expect("test app state should initialize")
    }
}
