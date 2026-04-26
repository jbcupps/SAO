mod auth;
mod db;
mod installers;
mod llm;
mod routes;
mod security;
mod state;
mod vault_state;

use anyhow::{bail, Context};
use axum::middleware;
use axum::Router;
use serde_json::json;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    match std::env::args().nth(1).as_deref() {
        Some("bootstrap-local") => return run_local_bootstrap().await,
        Some("mint-dev-token") => return mint_dev_token().await,
        _ => {}
    }

    if let Err(error) = run_server().await {
        tracing::error!("SAO server startup failed: {error:#}");
        return Err(error);
    }

    Ok(())
}

async fn mint_dev_token() -> anyhow::Result<()> {
    require_local_bootstrap_enabled()?;
    let username = std::env::var("SAO_LOCAL_ADMIN_USERNAME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "local-admin".to_string());
    let pool = db::pool::init_pool()
        .await
        .context("Failed to initialize the PostgreSQL connection pool")?;
    let user = db::users::get_user_by_username(&pool, &username)
        .await
        .context("Failed to query local admin user")?
        .with_context(|| {
            format!("Local user {username} was not found; run bootstrap-local first")
        })?;
    let jwt_secret = auth::session::jwt_secret();
    let token =
        auth::session::create_access_token(user.id, &user.username, &user.role, &jwt_secret)
            .context("Failed to create local development bearer token")?;
    println!("{token}");
    Ok(())
}

async fn run_local_bootstrap() -> anyhow::Result<()> {
    require_local_bootstrap_enabled()?;
    tracing::info!("Starting SAO local development bootstrap");

    let pool = db::pool::init_pool()
        .await
        .context("Failed to initialize the PostgreSQL connection pool")?;
    db::migrate::run_migrations(&pool)
        .await
        .context("Failed while applying database migrations")?;

    let passphrase = required_env("SAO_LOCAL_VAULT_PASSPHRASE")?;
    let username = std::env::var("SAO_LOCAL_ADMIN_USERNAME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "local-admin".to_string());
    let display_name = std::env::var("SAO_LOCAL_ADMIN_DISPLAY_NAME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Local SAO Admin".to_string());

    let vault_initialized = db::vault_key::vmk_exists(&pool)
        .await
        .context("Failed to inspect vault master key state")?;
    if !vault_initialized {
        initialize_local_vault(&pool, &passphrase).await?;
        tracing::info!("Initialized local development vault");
    } else {
        tracing::info!("Local development vault already initialized");
    }

    let user_id = match db::users::get_user_by_username(&pool, &username)
        .await
        .context("Failed to inspect local admin user")?
    {
        Some(user) => {
            if user.role != "admin" {
                db::users::update_user_role(&pool, user.id, "admin")
                    .await
                    .context("Failed to promote local bootstrap user to admin")?;
            }
            user.id
        }
        None => db::users::create_user(&pool, &username, Some(&display_name), "admin")
            .await
            .context("Failed to create local bootstrap admin user")?,
    };

    let _ = db::audit::insert_audit_log(
        &pool,
        Some(user_id),
        None,
        "local.bootstrap",
        Some("setup"),
        Some(json!({
            "username": username,
            "vault_initialized": !vault_initialized,
            "mode": "local_development",
        })),
        None,
        Some("sao-server bootstrap-local"),
    )
    .await;

    println!("SAO local bootstrap complete.");
    println!("Admin user: {username}");
    println!("Vault: initialized");
    println!("Next: open http://localhost:3100 or register a WebAuthn credential for {username}.");
    Ok(())
}

async fn initialize_local_vault(pool: &sqlx::PgPool, passphrase: &str) -> anyhow::Result<()> {
    let vmk = sao_core::vault::VaultMasterKey::generate();
    let salt = sao_core::vault::generate_salt();
    let passphrase_key = sao_core::vault::derive_key_from_passphrase(
        passphrase,
        &salt,
        sao_core::vault::kdf::DEFAULT_MEMORY_COST,
        sao_core::vault::kdf::DEFAULT_TIME_COST,
        sao_core::vault::kdf::DEFAULT_PARALLELISM,
    )
    .map_err(|error| anyhow::anyhow!("Failed to derive local vault passphrase key: {error}"))?;
    let (sealed, nonce) = vmk
        .seal(&passphrase_key)
        .map_err(|error| anyhow::anyhow!("Failed to seal local vault master key: {error}"))?;
    let mut envelope = sealed;
    envelope.extend_from_slice(&nonce);

    db::vault_key::insert_vmk(
        pool,
        &envelope,
        &salt,
        sao_core::vault::kdf::DEFAULT_MEMORY_COST as i32,
        sao_core::vault::kdf::DEFAULT_TIME_COST as i32,
        sao_core::vault::kdf::DEFAULT_PARALLELISM as i32,
    )
    .await
    .context("Failed to store local vault master key envelope")
}

fn require_local_bootstrap_enabled() -> anyhow::Result<()> {
    let enabled = std::env::var("SAO_LOCAL_BOOTSTRAP")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false);
    if !enabled {
        bail!("Refusing local bootstrap unless SAO_LOCAL_BOOTSTRAP=true is set");
    }
    Ok(())
}

fn required_env(name: &str) -> anyhow::Result<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .with_context(|| format!("{name} must be set for local bootstrap"))
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

    // Build CORS layer
    let allowed_origins = state::allowed_origin_values(&app_state);
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed_origins))
        .allow_methods(AllowMethods::list([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ]))
        .allow_headers(AllowHeaders::list([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
            axum::http::header::AUTHORIZATION,
            axum::http::header::COOKIE,
            axum::http::header::SET_COOKIE,
            axum::http::HeaderName::from_static(security::CSRF_HEADER),
            axum::http::HeaderName::from_static(security::REQUEST_ID_HEADER),
        ]))
        .allow_credentials(true);

    // Static file serving for the React SPA
    let static_dir =
        std::env::var("SAO_STATIC_DIR").unwrap_or_else(|_| "frontend/dist".to_string());
    let spa_fallback = ServeDir::new(&static_dir)
        .not_found_service(ServeFile::new(format!("{}/index.html", static_dir)));

    // Build the router
    let app = Router::new()
        .merge(routes::routes())
        .fallback_service(spa_fallback)
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            security::enforce_request_security,
        ))
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
