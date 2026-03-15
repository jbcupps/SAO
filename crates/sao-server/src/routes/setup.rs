use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/setup/status", get(setup_status))
        .route("/api/setup/initialize", post(initialize))
}

async fn setup_status(State(state): State<AppState>) -> Json<Value> {
    let vs = state.inner.vault_state.read().await;
    let initialized = !matches!(*vs, crate::vault_state::VaultState::Uninitialized);

    // Also check if any users exist
    let has_users = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(&state.inner.db)
        .await
        .unwrap_or(0)
        > 0;

    Json(json!({
        "initialized": initialized,
        "has_users": has_users,
        "needs_setup": !initialized || !has_users,
    }))
}

#[derive(Deserialize)]
struct BootstrapModelRequest {
    provider: String,
    model: String,
    api_key: String,
    entity_name: Option<String>,
}

#[derive(Deserialize)]
struct InitializeRequest {
    passphrase: String,
    admin_username: String,
    admin_display_name: Option<String>,
    bootstrap_model: BootstrapModelRequest,
}

struct NormalizedBootstrapModel {
    provider: String,
    model: String,
    api_key: String,
    entity_name: String,
}

async fn initialize(
    State(state): State<AppState>,
    Json(req): Json<InitializeRequest>,
) -> (StatusCode, Json<Value>) {
    // Check if already initialized
    {
        let vs = state.inner.vault_state.read().await;
        if !matches!(*vs, crate::vault_state::VaultState::Uninitialized) {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "Vault already initialized" })),
            );
        }
    }

    if req.passphrase.len() < 8 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Passphrase must be at least 8 characters" })),
        );
    }

    let admin_username = req.admin_username.trim();
    if admin_username.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Admin username is required" })),
        );
    }

    let bootstrap_model = match validate_bootstrap_model(req.bootstrap_model).await {
        Ok(config) => config,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": error })));
        }
    };

    // Generate VMK
    let vmk = sao_core::vault::VaultMasterKey::generate();

    // Derive passphrase key
    let salt = sao_core::vault::generate_salt();
    let passphrase_key = match sao_core::vault::kdf::derive_key_default(&req.passphrase, &salt) {
        Ok(key) => key,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("KDF failed: {}", e) })),
            );
        }
    };

    // Seal the VMK
    let (sealed_ciphertext, sealed_nonce) = match vmk.seal(&passphrase_key) {
        Ok(result) => result,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to seal VMK: {}", e) })),
            );
        }
    };

    // Store sealed VMK: combine ciphertext + nonce for storage
    let mut sealed_envelope = sealed_ciphertext;
    sealed_envelope.extend_from_slice(&sealed_nonce);

    let display_name = req
        .admin_display_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| admin_username.to_string());

    let bootstrap_provider = bootstrap_model.provider.clone();
    let bootstrap_model_name = bootstrap_model.model.clone();
    let admin_entity_name = bootstrap_model.entity_name.clone();
    let admin_entity_secret_label = format!("SAO admin entity {} credential", bootstrap_provider);

    let (identity_agent_id, agent_dir) = match state
        .inner
        .identity_manager
        .create_agent(&admin_entity_name)
    {
        Ok(result) => result,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to create SAO admin entity: {}", e) })),
            );
        }
    };

    if let Err(e) = state.inner.identity_manager.create_birth_documents(
        &identity_agent_id,
        &agent_dir,
        &admin_entity_name,
        Some("sao_admin_entity"),
        Some(&format!(
            "frontier:{}:{}",
            bootstrap_provider, bootstrap_model_name
        )),
    ) {
        cleanup_admin_entity(&state, &identity_agent_id, &agent_dir);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to create SAO admin entity documents: {}", e) })),
        );
    }

    let admin_entity_capabilities = vec![
        "sao_admin_entity".to_string(),
        "setup_orchestrator".to_string(),
        "work_item_tracking".to_string(),
        "frontier_model_connection".to_string(),
        "azure_container_delivery".to_string(),
        "azforge_iac".to_string(),
        format!("provider:{}", bootstrap_provider),
        format!("model:{}", bootstrap_model_name),
    ];
    let capabilities = json!(admin_entity_capabilities);

    let encrypted_bootstrap_secret = match vmk.encrypt(bootstrap_model.api_key.as_bytes()) {
        Ok(result) => result,
        Err(e) => {
            cleanup_admin_entity(&state, &identity_agent_id, &agent_dir);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    json!({ "error": format!("Failed to encrypt SAO admin entity credential: {}", e) }),
                ),
            );
        }
    };

    let mut tx = match state.inner.db.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            cleanup_admin_entity(&state, &identity_agent_id, &agent_dir);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to open setup transaction: {}", e) })),
            );
        }
    };

    if let Err(e) = sqlx::query(
        "INSERT INTO vault_master_key (encrypted_key, kdf_salt, kdf_memory_cost, kdf_time_cost, kdf_parallelism) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&sealed_envelope)
    .bind(&salt)
    .bind(sao_core::vault::kdf::DEFAULT_MEMORY_COST as i32)
    .bind(sao_core::vault::kdf::DEFAULT_TIME_COST as i32)
    .bind(sao_core::vault::kdf::DEFAULT_PARALLELISM as i32)
    .execute(&mut *tx)
    .await
    {
        cleanup_admin_entity(&state, &identity_agent_id, &agent_dir);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to store VMK: {}", e) })),
        );
    }

    let user_id = match sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (username, display_name, role) VALUES ($1, $2, 'admin') RETURNING id",
    )
    .bind(admin_username)
    .bind(&display_name)
    .fetch_one(&mut *tx)
    .await
    {
        Ok(id) => id,
        Err(e) => {
            cleanup_admin_entity(&state, &identity_agent_id, &agent_dir);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to create admin user: {}", e) })),
            );
        }
    };

    let bootstrap_secret_metadata = json!({
        "usage": "sao_admin_frontier_model",
        "entity_name": &admin_entity_name,
        "model": &bootstrap_model_name,
        "provider": &bootstrap_provider,
        "deployment_target": "azure_container_apps",
    });

    let bootstrap_secret_id = match sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO vault_secrets (owner_user_id, secret_type, label, provider, ciphertext, nonce, metadata) \
         VALUES ($1, 'api_key', $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(user_id)
    .bind(&admin_entity_secret_label)
    .bind(&bootstrap_provider)
    .bind(&encrypted_bootstrap_secret.0)
    .bind(&encrypted_bootstrap_secret.1)
    .bind(&bootstrap_secret_metadata)
    .fetch_one(&mut *tx)
    .await
    {
        Ok(id) => id,
        Err(e) => {
            cleanup_admin_entity(&state, &identity_agent_id, &agent_dir);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to store SAO admin entity credential: {}", e) })),
            );
        }
    };

    let admin_entity_id = match sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO agents (owner_user_id, name, capabilities, state) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(user_id)
    .bind(&admin_entity_name)
    .bind(&capabilities)
    .bind("bootstrap_pending")
    .fetch_one(&mut *tx)
    .await
    {
        Ok(id) => id,
        Err(e) => {
            cleanup_admin_entity(&state, &identity_agent_id, &agent_dir);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to persist SAO admin entity: {}", e) })),
            );
        }
    };

    let admin_entity_snapshot = crate::db::admin_entity::AdminEntitySnapshot {
        id: admin_entity_id,
        identity_agent_id: identity_agent_id.clone(),
        name: admin_entity_name.clone(),
        provider: bootstrap_provider.clone(),
        model: bootstrap_model_name.clone(),
        secret_id: bootstrap_secret_id,
        role: "sao_admin_entity".to_string(),
        deployment_target: "azure_container_apps".to_string(),
        iac_strategy: "azd_bicep".to_string(),
        capabilities: admin_entity_capabilities.clone(),
    };

    if let Err(e) =
        crate::db::admin_entity::upsert_admin_entity_snapshot(&mut tx, &admin_entity_snapshot).await
    {
        cleanup_admin_entity(&state, &identity_agent_id, &agent_dir);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to persist SAO admin entity snapshot: {}", e) })),
        );
    }

    let admin_work_items = match seed_admin_work_items(&mut tx, admin_entity_id).await {
        Ok(items) => items,
        Err(e) => {
            cleanup_admin_entity(&state, &identity_agent_id, &agent_dir);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    json!({ "error": format!("Failed to seed SAO admin entity work items: {}", e) }),
                ),
            );
        }
    };

    if let Err(e) = sqlx::query(
        "INSERT INTO audit_log (user_id, action, resource, details) VALUES ($1, $2, $3, $4)",
    )
    .bind(user_id)
    .bind("setup.initialize")
    .bind("vault")
    .bind(json!({
        "admin_username": admin_username,
        "admin_entity": &admin_entity_snapshot,
        "work_item_count": admin_work_items.len(),
    }))
    .execute(&mut *tx)
    .await
    {
        cleanup_admin_entity(&state, &identity_agent_id, &agent_dir);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to write audit log: {}", e) })),
        );
    }

    if let Err(e) = tx.commit().await {
        cleanup_admin_entity(&state, &identity_agent_id, &agent_dir);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to commit setup transaction: {}", e) })),
        );
    }

    {
        let mut vs = state.inner.vault_state.write().await;
        *vs = crate::vault_state::VaultState::Unsealed(vmk);
    }

    (
        StatusCode::CREATED,
        Json(json!({
            "status": "initialized",
            "user_id": user_id,
            "vault_status": "unsealed",
            "admin_entity": {
                "id": admin_entity_id,
                "identity_agent_id": identity_agent_id,
                "name": admin_entity_name,
                "provider": bootstrap_provider,
                "model": bootstrap_model_name,
                "secret_id": bootstrap_secret_id,
            },
            "work_items": admin_work_items,
        })),
    )
}

fn cleanup_admin_entity(state: &AppState, agent_id: &str, agent_dir: &std::path::Path) {
    let _ = state.inner.identity_manager.remove_agent(agent_id);
    let _ = std::fs::remove_dir_all(agent_dir);
}

async fn seed_admin_work_items(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    admin_entity_id: Uuid,
) -> Result<Vec<crate::db::admin_entity::AdminWorkItemRow>, sqlx::Error> {
    let seeds = vec![
        crate::db::admin_entity::AdminWorkItemSeed {
            sequence_no: 1,
            slug: "azforge-azure-container-iac",
            title: "Map AZForge into the Azure container IaC flow",
            description: "Define how the requested AZForge workflow maps onto SAO's Azure Container Apps delivery path, with repo-local infrastructure assets under infra/ and azd + Bicep as the default scaffold.",
            area: "azure_delivery",
            status: "pending",
            priority: 10,
            metadata: json!({
                "deployment_target": "azure_container_apps",
                "azforge_requested": true,
                "iac_stack": ["azd", "bicep"],
            }),
        },
        crate::db::admin_entity::AdminWorkItemSeed {
            sequence_no: 2,
            slug: "durable-sao-data",
            title: "Externalize SAO identity state from local container disk",
            description: "Current SAO_DATA_DIR persists identities, global settings, and the master key on the local filesystem. Azure container hosting needs durable storage or a redesign before SAO can operate safely after restarts.",
            area: "platform_foundation",
            status: "pending",
            priority: 20,
            metadata: json!({
                "env_var": "SAO_DATA_DIR",
                "current_value": "/data/sao",
                "risk": "ephemeral_container_storage",
            }),
        },
        crate::db::admin_entity::AdminWorkItemSeed {
            sequence_no: 3,
            slug: "managed-postgres-connectivity",
            title: "Provision Azure-hosted PostgreSQL and connection wiring",
            description: "Replace the local Docker PostgreSQL dependency with a managed Azure database and validate DATABASE_URL, network access, and migrations in the container environment.",
            area: "data",
            status: "pending",
            priority: 30,
            metadata: json!({
                "env_var": "DATABASE_URL",
                "current_local_dependency": "docker-compose-postgres",
            }),
        },
        crate::db::admin_entity::AdminWorkItemSeed {
            sequence_no: 4,
            slug: "runtime-secrets-and-identity",
            title: "Move runtime secrets into Azure-managed identity and vault flows",
            description: "Frontier model credentials and runtime configuration should move from local bootstrap assumptions into managed identity and secure secret distribution for Azure-hosted operation.",
            area: "security",
            status: "pending",
            priority: 40,
            metadata: json!({
                "recommended_services": ["managed_identity", "key_vault"],
            }),
        },
        crate::db::admin_entity::AdminWorkItemSeed {
            sequence_no: 5,
            slug: "webauthn-origin-cutover",
            title: "Cut over WebAuthn relying-party origin for Azure",
            description: "The current setup targets localhost. Before external access works in Azure, SAO_RP_ID and SAO_RP_ORIGIN must match the deployed hostname and TLS endpoint.",
            area: "identity",
            status: "pending",
            priority: 50,
            metadata: json!({
                "env_vars": ["SAO_RP_ID", "SAO_RP_ORIGIN"],
                "current_origin": "http://localhost:3100",
            }),
        },
        crate::db::admin_entity::AdminWorkItemSeed {
            sequence_no: 6,
            slug: "container-release-validation",
            title: "Publish the SAO container release and validate health checks",
            description: "Build the production SAO app image from docker/Dockerfile, publish it to GHCR, and verify the Azure deployment uses port 3100 with a passing /api/health probe. The standalone installer image from installer/Dockerfile is not the Azure runtime image path.",
            area: "release",
            status: "pending",
            priority: 60,
            metadata: json!({
                "image_source": "docker/Dockerfile",
                "image_registry": "ghcr",
                "port": 3100,
                "health_endpoint": "/api/health",
            }),
        },
    ];

    let mut inserted = Vec::with_capacity(seeds.len());
    for seed in &seeds {
        inserted.push(crate::db::admin_entity::insert_work_item(tx, admin_entity_id, seed).await?);
    }

    Ok(inserted)
}

async fn validate_bootstrap_model(
    req: BootstrapModelRequest,
) -> Result<NormalizedBootstrapModel, String> {
    let provider = req.provider.trim().to_lowercase();
    let model = req.model.trim().to_string();
    let api_key = req.api_key.trim().to_string();
    let entity_name = req
        .entity_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("sao-admin-entity")
        .to_string();

    if !matches!(provider.as_str(), "openai" | "anthropic" | "google") {
        return Err("Bootstrap provider must be one of: openai, anthropic, google".into());
    }

    if model.is_empty() {
        return Err("Bootstrap model is required".into());
    }

    if api_key.is_empty() {
        return Err("Bootstrap API key is required".into());
    }

    validate_frontier_connection(&provider, &model, &api_key).await?;

    Ok(NormalizedBootstrapModel {
        provider,
        model,
        api_key,
        entity_name,
    })
}

async fn validate_frontier_connection(
    provider: &str,
    model: &str,
    api_key: &str,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to construct validation client: {}", e))?;

    let response = match provider {
        "openai" => client
            .get(format!("https://api.openai.com/v1/models/{}", model))
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(|e| format!("OpenAI validation failed: {}", e))?,
        "anthropic" => client
            .get(format!("https://api.anthropic.com/v1/models/{}", model))
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
            .map_err(|e| format!("Anthropic validation failed: {}", e))?,
        "google" => {
            let model_path = if model.starts_with("models/") {
                model.to_string()
            } else {
                format!("models/{}", model)
            };

            client
                .get(format!(
                    "https://generativelanguage.googleapis.com/v1beta/{}",
                    model_path
                ))
                .query(&[("key", api_key)])
                .send()
                .await
                .map_err(|e| format!("Google validation failed: {}", e))?
        }
        _ => return Err("Unsupported bootstrap provider".into()),
    };

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(format!(
        "{} rejected the bootstrap connection for model `{}` ({}): {}",
        provider_label(provider),
        model,
        status,
        summarize_upstream_error(&body)
    ))
}

fn provider_label(provider: &str) -> &'static str {
    match provider {
        "openai" => "OpenAI",
        "anthropic" => "Anthropic",
        "google" => "Google",
        _ => "Provider",
    }
}

fn summarize_upstream_error(body: &str) -> String {
    if body.trim().is_empty() {
        return "no response body returned".into();
    }

    let parsed = serde_json::from_str::<Value>(body).ok();
    if let Some(message) = parsed
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(|error| match error {
            Value::String(message) => Some(message.to_string()),
            Value::Object(map) => map
                .get("message")
                .and_then(|value| value.as_str())
                .map(ToString::to_string),
            _ => None,
        })
    {
        return message;
    }

    let first_line = body
        .lines()
        .next()
        .unwrap_or("unexpected upstream response");
    first_line.chars().take(200).collect()
}
