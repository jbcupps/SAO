use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::middleware::{AdminUser, AuthUser};
use crate::state::{AppState, WsEvent};

pub fn routes() -> Router<AppState> {
    Router::new()
        // Authenticated user routes
        .route("/api/skills", get(list_skills))
        .route("/api/skills/{id}", get(get_skill))
        .route("/api/skills/{id}/reviews", get(list_skill_reviews))
        .route(
            "/api/skills/bindings/{id}/reviews",
            get(list_binding_reviews),
        )
        .route("/api/agents/{agent_id}/skills", get(list_agent_skills))
        .route(
            "/api/agents/{agent_id}/skills/checkin",
            post(agent_skill_checkin),
        )
        // Admin routes
        .route("/api/admin/skills", post(admin_create_skill))
        .route("/api/admin/skills/{id}", put(admin_update_skill))
        .route("/api/admin/skills/{id}", delete(admin_delete_skill))
        .route("/api/admin/skills/{id}/review", post(admin_review_skill))
        .route("/api/admin/skills/pending", get(admin_list_pending_skills))
        .route(
            "/api/admin/skills/bindings/pending",
            get(admin_list_pending_bindings),
        )
        .route(
            "/api/admin/skills/bindings/{id}/review",
            post(admin_review_binding),
        )
}

// --- Query params ---

#[derive(Deserialize)]
struct SkillListQuery {
    status: Option<String>,
    category: Option<String>,
}

// --- Authenticated user endpoints ---

async fn list_skills(
    _user: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<SkillListQuery>,
) -> (StatusCode, Json<Value>) {
    match crate::db::skills::list_skills(
        &state.inner.db,
        params.status.as_deref(),
        params.category.as_deref(),
    )
    .await
    {
        Ok(skills) => (StatusCode::OK, Json(json!({ "skills": skills }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn get_skill(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    match crate::db::skills::get_skill(&state.inner.db, id).await {
        Ok(Some(skill)) => (StatusCode::OK, Json(json!(skill))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Skill not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn list_skill_reviews(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    match crate::db::skills::list_reviews_for_target(&state.inner.db, "catalog", id).await {
        Ok(reviews) => (StatusCode::OK, Json(json!({ "reviews": reviews }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn list_binding_reviews(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    match crate::db::skills::list_reviews_for_target(&state.inner.db, "binding", id).await {
        Ok(reviews) => (StatusCode::OK, Json(json!({ "reviews": reviews }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn list_agent_skills(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    match crate::db::skills::list_bindings_for_agent(&state.inner.db, agent_id).await {
        Ok(bindings) => (StatusCode::OK, Json(json!({ "bindings": bindings }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

// --- Agent skill check-in ---

#[derive(Deserialize)]
struct SkillCheckinRequest {
    skills: Vec<sao_core::skills::SkillDeclaration>,
}

#[derive(serde::Serialize)]
struct SkillCheckinResultEntry {
    name: String,
    version: String,
    skill_id: Uuid,
    binding_id: Uuid,
    skill_status: String,
    binding_status: String,
    policy_score: Option<u32>,
    auto_approved: bool,
}

async fn agent_skill_checkin(
    user: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(req): Json<SkillCheckinRequest>,
) -> (StatusCode, Json<Value>) {
    // Verify agent exists
    match crate::db::agents::get_agent(&state.inner.db, agent_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Agent not found" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    }

    let mut results = Vec::new();
    let mut has_pending = false;

    for decl in &req.skills {
        // Look up existing skill by name+version
        let existing = match crate::db::skills::find_skill_by_name_version(
            &state.inner.db,
            &decl.name,
            &decl.version,
        )
        .await
        {
            Ok(skill) => skill,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                );
            }
        };

        let (skill_row, policy_result, was_new) = if let Some(existing_skill) = existing {
            (existing_skill, None, false)
        } else {
            // New skill: run policy engine
            let policy = sao_core::skills::evaluate_skill_policy(decl);
            let (skill_status, review_action) = if policy.auto_approve {
                ("approved", "auto_approve")
            } else {
                ("pending_review", "auto_flag")
            };

            let skill = match crate::db::skills::create_skill(
                &state.inner.db,
                &decl.name,
                &decl.version,
                decl.description.as_deref(),
                decl.author.as_deref(),
                decl.category.as_deref(),
                &decl.tags,
                &decl.permissions,
                &decl.api_endpoints,
                decl.input_schema.clone(),
                decl.output_schema.clone(),
                policy.risk_level.as_str(),
                skill_status,
                Some(policy.score as i32),
                Some(serde_json::to_value(&policy.checks).unwrap_or(json!([]))),
                None,
                Some(agent_id),
            )
            .await
            {
                Ok(s) => s,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": format!("Failed to create skill: {}", e) })),
                    );
                }
            };

            // Insert review record
            let _ = crate::db::skills::insert_review(
                &state.inner.db,
                "catalog",
                skill.id,
                review_action,
                None,
                Some(policy.score as i32),
                Some(serde_json::to_value(&policy.checks).unwrap_or(json!([]))),
                None,
            )
            .await;

            (skill, Some(policy), true)
        };

        // Determine binding status
        let binding_status = if skill_row.status == "approved" {
            "approved"
        } else {
            "pending_review"
        };

        let binding = match crate::db::skills::create_binding(
            &state.inner.db,
            agent_id,
            skill_row.id,
            binding_status,
            None,
        )
        .await
        {
            Ok(b) => b,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("Failed to create binding: {}", e) })),
                );
            }
        };

        // Insert binding review record if it was a new skill
        if was_new {
            let binding_action = if binding_status == "approved" {
                "auto_approve"
            } else {
                "auto_flag"
            };
            let _ = crate::db::skills::insert_review(
                &state.inner.db,
                "binding",
                binding.id,
                binding_action,
                None,
                policy_result.as_ref().map(|p| p.score as i32),
                policy_result
                    .as_ref()
                    .map(|p| serde_json::to_value(&p.checks).unwrap_or(json!([]))),
                None,
            )
            .await;
        }

        if binding_status == "pending_review" {
            has_pending = true;
        }

        results.push(SkillCheckinResultEntry {
            name: decl.name.clone(),
            version: decl.version.clone(),
            skill_id: skill_row.id,
            binding_id: binding.id,
            skill_status: skill_row.status.clone(),
            binding_status: binding_status.to_string(),
            policy_score: policy_result.as_ref().map(|p| p.score),
            auto_approved: binding_status == "approved",
        });
    }

    // Audit log
    let _ = crate::db::audit::insert_audit_log(
        &state.inner.db,
        Some(user.user_id),
        Some(agent_id),
        "skills.checkin",
        Some("agent_skills"),
        Some(json!({
            "agent_id": agent_id,
            "skill_count": results.len(),
            "pending_count": results.iter().filter(|r| !r.auto_approved).count(),
        })),
        None,
        None,
    )
    .await;

    // Broadcast WebSocket event if any items need review
    if has_pending {
        let _ = state.inner.ws_tx.send(WsEvent {
            event: "skill_checkin_pending".to_string(),
            payload: json!({
                "agent_id": agent_id,
                "pending_count": results.iter().filter(|r| !r.auto_approved).count(),
            }),
        });
    }

    (StatusCode::OK, Json(json!({ "results": results })))
}

// --- Admin endpoints ---

#[derive(Deserialize)]
struct AdminCreateSkillRequest {
    name: String,
    version: Option<String>,
    description: Option<String>,
    author: Option<String>,
    category: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    api_endpoints: Vec<String>,
    input_schema: Option<serde_json::Value>,
    output_schema: Option<serde_json::Value>,
}

async fn admin_create_skill(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Json(req): Json<AdminCreateSkillRequest>,
) -> (StatusCode, Json<Value>) {
    let version = req.version.as_deref().unwrap_or("1.0.0");

    // Run policy engine for risk assessment
    let decl = sao_core::skills::SkillDeclaration {
        name: req.name.clone(),
        version: version.to_string(),
        description: req.description.clone(),
        author: req.author.clone(),
        category: req.category.clone(),
        tags: req.tags.clone(),
        permissions: req.permissions.clone(),
        api_endpoints: req.api_endpoints.clone(),
        input_schema: req.input_schema.clone(),
        output_schema: req.output_schema.clone(),
    };
    let policy = sao_core::skills::evaluate_skill_policy(&decl);

    // Admin-created skills are auto-approved
    match crate::db::skills::create_skill(
        &state.inner.db,
        &req.name,
        version,
        req.description.as_deref(),
        req.author.as_deref(),
        req.category.as_deref(),
        &req.tags,
        &req.permissions,
        &req.api_endpoints,
        req.input_schema,
        req.output_schema,
        policy.risk_level.as_str(),
        "approved",
        Some(policy.score as i32),
        Some(serde_json::to_value(&policy.checks).unwrap_or(json!([]))),
        Some(admin.user_id),
        None,
    )
    .await
    {
        Ok(skill) => {
            let _ = crate::db::skills::insert_review(
                &state.inner.db,
                "catalog",
                skill.id,
                "manual_approve",
                Some(admin.user_id),
                Some(policy.score as i32),
                Some(serde_json::to_value(&policy.checks).unwrap_or(json!([]))),
                Some("Admin-created skill, auto-approved"),
            )
            .await;

            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "skills.admin_create",
                Some("skill_catalog"),
                Some(json!({ "skill_id": skill.id, "name": skill.name })),
                None,
                None,
            )
            .await;

            (StatusCode::CREATED, Json(json!(skill)))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

#[derive(Deserialize)]
struct AdminUpdateSkillRequest {
    description: Option<String>,
    author: Option<String>,
    category: Option<String>,
    tags: Option<Vec<String>>,
    permissions: Option<Vec<String>>,
    api_endpoints: Option<Vec<String>>,
    input_schema: Option<serde_json::Value>,
    output_schema: Option<serde_json::Value>,
    risk_level: Option<String>,
}

async fn admin_update_skill(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<AdminUpdateSkillRequest>,
) -> (StatusCode, Json<Value>) {
    match crate::db::skills::update_skill_metadata(
        &state.inner.db,
        id,
        req.description.as_deref(),
        req.author.as_deref(),
        req.category.as_deref(),
        req.tags.as_deref(),
        req.permissions.as_deref(),
        req.api_endpoints.as_deref(),
        req.input_schema,
        req.output_schema,
        req.risk_level.as_deref(),
    )
    .await
    {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "skills.admin_update",
                Some("skill_catalog"),
                Some(json!({ "skill_id": id })),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({ "updated": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Skill not found or no changes" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn admin_delete_skill(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<Value>) {
    match crate::db::skills::delete_skill(&state.inner.db, id).await {
        Ok(true) => {
            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                "skills.admin_delete",
                Some("skill_catalog"),
                Some(json!({ "skill_id": id })),
                None,
                None,
            )
            .await;
            (StatusCode::OK, Json(json!({ "deleted": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Skill not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

#[derive(Deserialize)]
struct ReviewRequest {
    action: String,
    notes: Option<String>,
}

async fn admin_review_skill(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ReviewRequest>,
) -> (StatusCode, Json<Value>) {
    let new_status = match req.action.as_str() {
        "approve" | "manual_approve" => "approved",
        "reject" | "manual_reject" => "rejected",
        "request_changes" => "pending_review",
        "deprecate" => "deprecated",
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    json!({ "error": "Invalid action. Use: approve, reject, request_changes, deprecate" }),
                ),
            );
        }
    };

    let review_action = match req.action.as_str() {
        "approve" => "manual_approve",
        "reject" => "manual_reject",
        _ => &req.action,
    };

    match crate::db::skills::update_skill_status(
        &state.inner.db,
        id,
        new_status,
        Some(admin.user_id),
        req.notes.as_deref(),
    )
    .await
    {
        Ok(true) => {
            let _ = crate::db::skills::insert_review(
                &state.inner.db,
                "catalog",
                id,
                review_action,
                Some(admin.user_id),
                None,
                None,
                req.notes.as_deref(),
            )
            .await;

            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                &format!("skills.review_{}", review_action),
                Some("skill_catalog"),
                Some(json!({ "skill_id": id, "action": review_action, "new_status": new_status })),
                None,
                None,
            )
            .await;

            // Broadcast WebSocket event
            let _ = state.inner.ws_tx.send(WsEvent {
                event: "skill_review".to_string(),
                payload: json!({
                    "skill_id": id,
                    "action": review_action,
                    "new_status": new_status,
                }),
            });

            (StatusCode::OK, Json(json!({ "status": new_status })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Skill not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn admin_list_pending_skills(
    AdminUser(_admin): AdminUser,
    State(state): State<AppState>,
) -> (StatusCode, Json<Value>) {
    match crate::db::skills::list_skills(&state.inner.db, Some("pending_review"), None).await {
        Ok(skills) => (StatusCode::OK, Json(json!({ "skills": skills }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn admin_list_pending_bindings(
    AdminUser(_admin): AdminUser,
    State(state): State<AppState>,
) -> (StatusCode, Json<Value>) {
    match crate::db::skills::list_pending_bindings(&state.inner.db).await {
        Ok(bindings) => (StatusCode::OK, Json(json!({ "bindings": bindings }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

async fn admin_review_binding(
    AdminUser(admin): AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ReviewRequest>,
) -> (StatusCode, Json<Value>) {
    let new_status = match req.action.as_str() {
        "approve" | "manual_approve" => "approved",
        "reject" | "manual_reject" => "rejected",
        "revoke" => "revoked",
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid action. Use: approve, reject, revoke" })),
            );
        }
    };

    let review_action = match req.action.as_str() {
        "approve" => "manual_approve",
        "reject" => "manual_reject",
        _ => &req.action,
    };

    match crate::db::skills::update_binding_status(
        &state.inner.db,
        id,
        new_status,
        Some(admin.user_id),
        req.notes.as_deref(),
    )
    .await
    {
        Ok(true) => {
            let _ = crate::db::skills::insert_review(
                &state.inner.db,
                "binding",
                id,
                review_action,
                Some(admin.user_id),
                None,
                None,
                req.notes.as_deref(),
            )
            .await;

            let _ = crate::db::audit::insert_audit_log(
                &state.inner.db,
                Some(admin.user_id),
                None,
                &format!("skills.binding_review_{}", review_action),
                Some("agent_skill_binding"),
                Some(
                    json!({ "binding_id": id, "action": review_action, "new_status": new_status }),
                ),
                None,
                None,
            )
            .await;

            let _ = state.inner.ws_tx.send(WsEvent {
                event: "skill_binding_review".to_string(),
                payload: json!({
                    "binding_id": id,
                    "action": review_action,
                    "new_status": new_status,
                }),
            });

            (StatusCode::OK, Json(json!({ "status": new_status })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Binding not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}
