use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct WebauthnCredentialRow {
    pub id: i32,
    pub user_id: Uuid,
    pub credential_id: String,
    pub credential_json: serde_json::Value,
    pub label: Option<String>,
}

/// Store a WebAuthn credential for a user.
pub async fn store_credential(
    pool: &PgPool,
    user_id: Uuid,
    credential_id: &str,
    credential_json: serde_json::Value,
    label: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO webauthn_credentials (user_id, credential_id, credential_json, label) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(user_id)
    .bind(credential_id)
    .bind(credential_json)
    .bind(label)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get all credentials for a user.
pub async fn get_credentials_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<WebauthnCredentialRow>, sqlx::Error> {
    sqlx::query_as::<_, WebauthnCredentialRow>(
        "SELECT id, user_id, credential_id, credential_json, label \
         FROM webauthn_credentials WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// Update credential's last_used_at timestamp.
pub async fn touch_credential(pool: &PgPool, credential_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE webauthn_credentials SET last_used_at = now() WHERE credential_id = $1")
        .bind(credential_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Store a WebAuthn challenge state (ephemeral).
pub async fn store_challenge(
    pool: &PgPool,
    challenge_id: &str,
    challenge_json: serde_json::Value,
    challenge_type: &str,
    user_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    // Clean up expired challenges first
    sqlx::query("DELETE FROM webauthn_challenges WHERE expires_at < now()")
        .execute(pool)
        .await?;

    sqlx::query(
        "INSERT INTO webauthn_challenges (id, challenge_json, challenge_type, user_id) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (id) DO UPDATE SET challenge_json = $2, challenge_type = $3",
    )
    .bind(challenge_id)
    .bind(challenge_json)
    .bind(challenge_type)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Retrieve and delete a challenge state (one-time use).
pub async fn consume_challenge(
    pool: &PgPool,
    challenge_id: &str,
) -> Result<Option<(serde_json::Value, Option<Uuid>)>, sqlx::Error> {
    let row: Option<(serde_json::Value, Option<Uuid>)> = sqlx::query_as(
        "DELETE FROM webauthn_challenges WHERE id = $1 AND expires_at > now() \
         RETURNING challenge_json, user_id",
    )
    .bind(challenge_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
