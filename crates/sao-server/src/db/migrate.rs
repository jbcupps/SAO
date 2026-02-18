use sqlx::PgPool;

/// Run all pending database migrations.
pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    tracing::info!("Running database migrations...");
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await?;
    tracing::info!("Database migrations complete");
    Ok(())
}
