mod auth;
mod db;
mod routes;
mod runtime;
mod state;
mod vault_state;
mod ws;

use anyhow::{bail, Context};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    if let Err(error) = run_server().await {
        tracing::error!("SAO server startup failed: {error:#}");
        return Err(error);
    }

    Ok(())
}

async fn run_server() -> anyhow::Result<()> {
    tracing::info!("Starting SAO - Secure Agent Orchestrator");

    // Initialize database pool (required)
    let pool = db::pool::init_pool()
        .await
        .context("Failed to initialize the PostgreSQL connection pool")?;
    log_startup_checkpoint("db_pool_initialized");

    // Run database migrations
    db::migrate::run_migrations(&pool)
        .await
        .context("Failed while applying database migrations")?;
    log_startup_checkpoint("migrations_complete");

    // Determine initial vault state
    let vault_state = determine_vault_state(&pool)
        .await
        .context("Failed to determine the initial vault state")?;
    log_startup_checkpoint("vault_state_determined");

    // Initialize WebAuthn
    let webauthn = auth::webauthn::create_webauthn().context("Failed to initialize WebAuthn")?;
    log_startup_checkpoint("webauthn_initialized");

    // Initialize JWT secret
    let jwt_secret = auth::session::jwt_secret();
    log_startup_checkpoint("jwt_initialized");

    // Initialize application state
    let app_state = state::init_app_state(pool, vault_state, webauthn, jwt_secret)
        .context("Failed to initialize identity and runtime state")?;
    log_startup_checkpoint("identity_runtime_initialized");

    // Start background runtime scheduler
    let runtime = app_state.inner.runtime.clone();
    tokio::spawn(async move {
        runtime.scheduler_loop().await;
    });

    // Build CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Static file serving for the React SPA
    let static_dir =
        std::env::var("SAO_STATIC_DIR").unwrap_or_else(|_| "frontend/dist".to_string());
    let spa_fallback = ServeDir::new(&static_dir)
        .not_found_service(ServeFile::new(format!("{}/index.html", static_dir)));

    // Build the router
    let app = Router::new()
        .merge(routes::routes())
        .merge(ws::ws_routes())
        .fallback_service(spa_fallback)
        .layer(cors)
        .with_state(app_state);

    // Bind and serve
    let bind_addr = std::env::var("SAO_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3100".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("Failed to bind the SAO listener on {bind_addr}"))?;
    log_startup_checkpoint("listener_bound");

    tracing::info!("SAO server listening on {}", bind_addr);
    axum::serve(listener, app)
        .await
        .context("SAO HTTP server exited unexpectedly")?;

    Ok(())
}

fn log_startup_checkpoint(stage: &str) {
    tracing::info!("SAO startup checkpoint: {stage}");
}

/// Determine initial vault state: Uninitialized, auto-unseal, or Sealed.
async fn determine_vault_state(pool: &sqlx::PgPool) -> anyhow::Result<vault_state::VaultState> {
    tracing::info!("Inspecting vault state during startup");
    let vmk_exists = db::vault_key::vmk_exists(pool)
        .await
        .context("Failed to check whether a vault master key already exists")?;

    if !vmk_exists {
        tracing::info!("No vault master key found - vault is uninitialized");
        return Ok(vault_state::VaultState::Uninitialized);
    }

    // Try auto-unseal from environment variable
    if let Ok(passphrase) = std::env::var("SAO_VAULT_PASSPHRASE") {
        tracing::info!("Attempting auto-unseal from SAO_VAULT_PASSPHRASE...");
        match auto_unseal(pool, &passphrase).await {
            Ok(vmk) => {
                tracing::info!("Vault auto-unsealed successfully");
                return Ok(vault_state::VaultState::Unsealed(vmk));
            }
            Err(e) => {
                tracing::error!("Auto-unseal failed: {e:#}");
            }
        }
    }

    tracing::info!("Vault is sealed - unseal via API to enable encryption");
    Ok(vault_state::VaultState::Sealed)
}

async fn auto_unseal(
    pool: &sqlx::PgPool,
    passphrase: &str,
) -> anyhow::Result<sao_core::vault::VaultMasterKey> {
    let vmk_row = db::vault_key::get_vmk(pool)
        .await
        .context("Failed to load the stored vault master key envelope")?
        .ok_or_else(|| anyhow::anyhow!("No vault master key row was found"))?;

    let passphrase_key = sao_core::vault::kdf::derive_key_from_passphrase(
        passphrase,
        &vmk_row.kdf_salt,
        vmk_row.kdf_memory_cost as u32,
        vmk_row.kdf_time_cost as u32,
        vmk_row.kdf_parallelism as u32,
    )
    .map_err(|error| anyhow::anyhow!("Failed to derive the vault passphrase key: {error}"))?;

    let encrypted = &vmk_row.encrypted_key;
    if encrypted.len() < 12 {
        bail!("Corrupted VMK envelope");
    }
    let (ciphertext, nonce) = encrypted.split_at(encrypted.len() - 12);

    sao_core::vault::VaultMasterKey::unseal(ciphertext, nonce, &passphrase_key)
        .map_err(|error| anyhow::anyhow!("Failed to unseal the vault master key: {error}"))
}
