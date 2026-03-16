use crate::db::startup::StartupRetryConfig;
use anyhow::Context;
use std::time::Instant;
use tokio::time::sleep;

use sqlx::PgPool;

/// Run all pending database migrations.
pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    let retry_config = StartupRetryConfig::default();
    let started_at = Instant::now();
    let mut attempt = 1_u32;

    loop {
        tracing::info!(attempt, "Running database migrations...");
        match sqlx::migrate!("../../migrations").run(pool).await {
            Ok(()) => {
                tracing::info!(
                    attempt,
                    elapsed_seconds = started_at.elapsed().as_secs(),
                    "Database migrations complete"
                );
                return Ok(());
            }
            Err(error) => {
                let elapsed = started_at.elapsed();
                let next_delay = retry_config.retry_delay_for_attempt(attempt);
                if elapsed + next_delay >= retry_config.max_wait {
                    return Err(anyhow::Error::from(error)).context(format!(
                        "Database migrations did not complete within {} seconds after {} attempts",
                        retry_config.max_wait.as_secs(),
                        attempt
                    ));
                }

                tracing::warn!(
                    attempt,
                    elapsed_seconds = elapsed.as_secs(),
                    retry_delay_seconds = next_delay.as_secs(),
                    error = %error,
                    "Database migrations failed during startup; retrying"
                );
                sleep(next_delay).await;
                attempt += 1;
            }
        }
    }
}
