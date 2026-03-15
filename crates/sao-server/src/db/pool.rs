use anyhow::{Context, Result};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::time::Instant;
use tokio::time::sleep;

use crate::db::startup::StartupRetryConfig;

/// Initialize the PostgreSQL connection pool from DATABASE_URL.
pub async fn init_pool() -> Result<PgPool> {
    let database_url =
        std::env::var("DATABASE_URL").context("DATABASE_URL environment variable must be set")?;
    let connect_options = database_url
        .parse::<PgConnectOptions>()
        .context("DATABASE_URL could not be parsed as PostgreSQL connection settings")?;
    let retry_config = StartupRetryConfig::default();
    let started_at = Instant::now();
    let mut attempt = 1_u32;

    loop {
        let pool_result = PgPoolOptions::new()
            .max_connections(10)
            .min_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .idle_timeout(std::time::Duration::from_secs(600))
            .connect_with(connect_options.clone())
            .await;

        match pool_result {
            Ok(pool) => {
                tracing::info!(
                    attempt,
                    elapsed_seconds = started_at.elapsed().as_secs(),
                    "PostgreSQL connection pool established"
                );
                return Ok(pool);
            }
            Err(error) => {
                let elapsed = started_at.elapsed();
                let next_delay = retry_config.retry_delay_for_attempt(attempt);
                if elapsed + next_delay >= retry_config.max_wait {
                    return Err(error).with_context(|| {
                        format!(
                            "Failed to connect to PostgreSQL within {} seconds after {} attempts",
                            retry_config.max_wait.as_secs(),
                            attempt
                        )
                    });
                }

                tracing::warn!(
                    attempt,
                    elapsed_seconds = elapsed.as_secs(),
                    retry_delay_seconds = next_delay.as_secs(),
                    error = %error,
                    "PostgreSQL connection attempt failed; retrying during startup"
                );
                sleep(next_delay).await;
                attempt += 1;
            }
        }
    }
}
