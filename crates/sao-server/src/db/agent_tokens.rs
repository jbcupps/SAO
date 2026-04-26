use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

// Several fields aren't read today — they're surfaced through the audit-log JSON when relevant
// rather than every callsite reading them off the row.
#[allow(dead_code)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AgentTokenRow {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub issued_by: Uuid,
    pub issued_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub scope: String,
}

pub async fn insert_token(
    pool: &PgPool,
    agent_id: Uuid,
    issued_by: Uuid,
    expires_at: Option<DateTime<Utc>>,
    scope: &str,
) -> Result<AgentTokenRow, sqlx::Error> {
    sqlx::query_as::<_, AgentTokenRow>(
        "INSERT INTO agent_tokens (agent_id, issued_by, expires_at, scope) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id, agent_id, issued_by, issued_at, expires_at, revoked_at, last_used_at, scope",
    )
    .bind(agent_id)
    .bind(issued_by)
    .bind(expires_at)
    .bind(scope)
    .fetch_one(pool)
    .await
}

pub async fn get_active(pool: &PgPool, jti: Uuid) -> Result<Option<AgentTokenRow>, sqlx::Error> {
    sqlx::query_as::<_, AgentTokenRow>(
        "SELECT id, agent_id, issued_by, issued_at, expires_at, revoked_at, last_used_at, scope \
         FROM agent_tokens \
         WHERE id = $1 AND revoked_at IS NULL \
           AND (expires_at IS NULL OR expires_at > now())",
    )
    .bind(jti)
    .fetch_optional(pool)
    .await
}

pub async fn touch_last_used(pool: &PgPool, jti: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE agent_tokens SET last_used_at = now() WHERE id = $1")
        .bind(jti)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn revoke_for_agent(pool: &PgPool, agent_id: Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE agent_tokens SET revoked_at = now() \
         WHERE agent_id = $1 AND revoked_at IS NULL",
    )
    .bind(agent_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
