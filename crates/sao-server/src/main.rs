mod auth;
mod db;
mod routes;
mod runtime;
mod state;
mod vault_state;
mod ws;

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

    tracing::info!("Starting SAO - Secure Agent Orchestrator");

    // Initialize database pool (required)
    let pool = db::pool::init_pool().await?;

    // Run database migrations
    db::migrate::run_migrations(&pool).await?;

    // Determine initial vault state
    let vault_state = determine_vault_state(&pool).await;

    // Initialize WebAuthn
    let webauthn = auth::webauthn::create_webauthn();

    // Initialize JWT secret
    let jwt_secret = auth::session::jwt_secret();

    // Initialize application state
    let app_state = state::init_app_state(pool, vault_state, webauthn, jwt_secret);

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
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    tracing::info!("SAO server listening on {}", bind_addr);
    axum::serve(listener, app).await?;

    Ok(())
}

/// Determine initial vault state: Uninitialized, auto-unseal, or Sealed.
async fn determine_vault_state(pool: &sqlx::PgPool) -> vault_state::VaultState {
    let vmk_exists = match db::vault_key::vmk_exists(pool).await {
        Ok(exists) => exists,
        Err(e) => {
            tracing::error!("Failed to check VMK status: {}", e);
            return vault_state::VaultState::Uninitialized;
        }
    };

    if !vmk_exists {
        tracing::info!("No vault master key found - vault is uninitialized");
        return vault_state::VaultState::Uninitialized;
    }

    // Try auto-unseal from environment variable
    if let Ok(passphrase) = std::env::var("SAO_VAULT_PASSPHRASE") {
        tracing::info!("Attempting auto-unseal from SAO_VAULT_PASSPHRASE...");
        match auto_unseal(pool, &passphrase).await {
            Ok(vmk) => {
                tracing::info!("Vault auto-unsealed successfully");
                return vault_state::VaultState::Unsealed(vmk);
            }
            Err(e) => {
                tracing::error!("Auto-unseal failed: {}", e);
            }
        }
    }

    tracing::info!("Vault is sealed - unseal via API to enable encryption");
    vault_state::VaultState::Sealed
}

async fn auto_unseal(
    pool: &sqlx::PgPool,
    passphrase: &str,
) -> Result<sao_core::vault::VaultMasterKey, String> {
    let vmk_row = db::vault_key::get_vmk(pool)
        .await
        .map_err(|e| format!("DB error: {}", e))?
        .ok_or("No VMK found")?;

    let passphrase_key = sao_core::vault::kdf::derive_key_from_passphrase(
        passphrase,
        &vmk_row.kdf_salt,
        vmk_row.kdf_memory_cost as u32,
        vmk_row.kdf_time_cost as u32,
        vmk_row.kdf_parallelism as u32,
    )?;

    let encrypted = &vmk_row.encrypted_key;
    if encrypted.len() < 12 {
        return Err("Corrupted VMK envelope".to_string());
    }
    let (ciphertext, nonce) = encrypted.split_at(encrypted.len() - 12);

    sao_core::vault::VaultMasterKey::unseal(ciphertext, nonce, &passphrase_key)
}
