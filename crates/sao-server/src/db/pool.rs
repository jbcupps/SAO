use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Initialize the PostgreSQL connection pool from DATABASE_URL.
/// Panics if DATABASE_URL is not set or connection fails.
pub async fn init_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL environment variable must be set");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .idle_timeout(std::time::Duration::from_secs(600))
        .connect(&database_url)
        .await
        .expect("Failed to connect to PostgreSQL");

    tracing::info!("PostgreSQL connection pool established");
    pool
}
