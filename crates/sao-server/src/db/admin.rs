use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct AuditLogRow {
    pub id: i64,
    pub user_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub action: String,
    pub resource: Option<String>,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn query_audit_log(
    pool: &PgPool,
    user_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<AuditLogRow>, sqlx::Error> {
    if let Some(uid) = user_id {
        sqlx::query_as::<_, AuditLogRow>(
            "SELECT id, user_id, agent_id, action, resource, details, ip_address, user_agent, created_at \
             FROM audit_log WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(uid)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, AuditLogRow>(
            "SELECT id, user_id, agent_id, action, resource, details, ip_address, user_agent, created_at \
             FROM audit_log ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }
}
