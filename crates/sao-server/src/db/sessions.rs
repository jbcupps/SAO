use sqlx::PgPool;
use uuid::Uuid;

/// Store a refresh token hash.
pub async fn store_refresh_token(
    pool: &PgPool,
    user_id: Uuid,
    token_hash: &str,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(())
}

/// Validate a refresh token hash (not expired, not revoked).
pub async fn validate_refresh_token(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM refresh_tokens \
         WHERE token_hash = $1 AND expires_at > now() AND revoked = false",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

/// Revoke a specific refresh token.
pub async fn revoke_refresh_token(pool: &PgPool, token_hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE refresh_tokens SET revoked = true WHERE token_hash = $1")
        .bind(token_hash)
        .execute(pool)
        .await?;
    Ok(())
}

/// Revoke all refresh tokens for a user.
#[allow(dead_code)]
pub async fn revoke_all_user_tokens(pool: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE refresh_tokens SET revoked = true WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}
