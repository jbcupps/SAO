//! PostgreSQL connection pool and health checking for AO Manager.
//!
//! Enabled via the `postgres` Cargo feature. When `DATABASE_URL` is set,
//! the server creates a connection pool at startup and exposes it via AppState.
//! The `/health` endpoint includes database connectivity status.

#[cfg(feature = "postgres")]
use sqlx::postgres::{PgPool, PgPoolOptions};

/// Database pool wrapper. When the `postgres` feature is disabled,
/// this is a zero-cost no-op.
#[derive(Clone)]
pub struct DbPool {
    #[cfg(feature = "postgres")]
    pool: Option<PgPool>,
    #[cfg(not(feature = "postgres"))]
    _phantom: (),
}

impl DbPool {
    /// Create a no-op pool (postgres feature disabled or no DATABASE_URL).
    pub fn none() -> Self {
        DbPool {
            #[cfg(feature = "postgres")]
            pool: None,
            #[cfg(not(feature = "postgres"))]
            _phantom: (),
        }
    }

    /// Initialize from DATABASE_URL environment variable.
    /// Returns `DbPool::none()` if the variable is unset or the feature is disabled.
    pub async fn from_env() -> Self {
        #[cfg(feature = "postgres")]
        {
            match std::env::var("DATABASE_URL") {
                Ok(url) if !url.is_empty() => match Self::connect(&url).await {
                    Ok(pool) => pool,
                    Err(e) => {
                        tracing::error!("Failed to connect to PostgreSQL: {}", e);
                        DbPool::none()
                    }
                },
                _ => {
                    tracing::info!("DATABASE_URL not set, PostgreSQL disabled");
                    DbPool::none()
                }
            }
        }
        #[cfg(not(feature = "postgres"))]
        {
            tracing::info!("PostgreSQL feature not enabled");
            DbPool::none()
        }
    }

    /// Connect to PostgreSQL with the given connection string.
    #[cfg(feature = "postgres")]
    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let ssl_mode = std::env::var("AO_DB_SSL")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        let url = if ssl_mode && !database_url.contains("sslmode=") {
            let separator = if database_url.contains('?') { "&" } else { "?" };
            format!("{}{}sslmode=require", database_url, separator)
        } else {
            database_url.to_string()
        };

        let pool = PgPoolOptions::new()
            .max_connections(10)
            .min_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .idle_timeout(std::time::Duration::from_secs(600))
            .connect(&url)
            .await?;

        tracing::info!("PostgreSQL connection pool established");
        Ok(DbPool { pool: Some(pool) })
    }

    /// Check if the database is connected and responsive.
    pub async fn is_healthy(&self) -> bool {
        #[cfg(feature = "postgres")]
        {
            if let Some(ref pool) = self.pool {
                sqlx::query_scalar::<_, i32>("SELECT 1")
                    .fetch_one(pool)
                    .await
                    .is_ok()
            } else {
                false
            }
        }
        #[cfg(not(feature = "postgres"))]
        {
            false
        }
    }

    /// Whether a pool is present (feature enabled + DATABASE_URL was set + connected).
    pub fn is_connected(&self) -> bool {
        #[cfg(feature = "postgres")]
        {
            self.pool.is_some()
        }
        #[cfg(not(feature = "postgres"))]
        {
            false
        }
    }

    /// Get a reference to the underlying pool, if available.
    #[cfg(feature = "postgres")]
    pub fn pool(&self) -> Option<&PgPool> {
        self.pool.as_ref()
    }
}

/// Health status for the `/health` endpoint.
#[derive(serde::Serialize)]
pub struct HealthStatus {
    pub status: String,
    pub service: String,
    pub database: DatabaseHealth,
}

#[derive(serde::Serialize)]
pub struct DatabaseHealth {
    pub configured: bool,
    pub connected: bool,
    pub healthy: bool,
}

impl DbPool {
    pub async fn health_status(&self) -> HealthStatus {
        let configured = self.is_connected();
        let healthy = if configured {
            self.is_healthy().await
        } else {
            false
        };

        let overall = if !configured || healthy { "ok" } else { "degraded" };

        HealthStatus {
            status: overall.to_string(),
            service: "sao".to_string(),
            database: DatabaseHealth {
                configured,
                connected: configured,
                healthy,
            },
        }
    }
}
