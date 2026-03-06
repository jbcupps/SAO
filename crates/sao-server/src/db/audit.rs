use sqlx::PgPool;
use uuid::Uuid;

#[allow(clippy::too_many_arguments)]
pub async fn insert_audit_log(
    pool: &PgPool,
    user_id: Option<Uuid>,
    agent_id: Option<Uuid>,
    action: &str,
    resource: Option<&str>,
    details: Option<serde_json::Value>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_log (user_id, agent_id, action, resource, details, ip_address, user_agent) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(user_id)
    .bind(agent_id)
    .bind(action)
    .bind(resource)
    .bind(details)
    .bind(ip_address)
    .bind(user_agent)
    .execute(pool)
    .await?;
    Ok(())
}

pub fn log_birth_event(agent_id: &str) {
    tracing::info!(
        "AUDIT: Agent {} born with immutable soul.md + TriangleEthic preview",
        agent_id
    );
    // Placeholder for future SIEM integration
}
