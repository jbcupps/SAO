use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct EgressEventRow {
    pub event_id: Uuid,
    pub user_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub orion_id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub enqueued_at: DateTime<Utc>,
    pub attempts: i32,
    pub created_at: DateTime<Utc>,
}

pub async fn list_for_agent(
    pool: &PgPool,
    agent_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<EgressEventRow>, sqlx::Error> {
    sqlx::query_as::<_, EgressEventRow>(
        "SELECT event_id, user_id, agent_id, orion_id, event_type, payload, enqueued_at, attempts, created_at \
         FROM orion_egress_events \
         WHERE agent_id = $1 \
         ORDER BY created_at DESC \
         LIMIT $2 OFFSET $3",
    )
    .bind(agent_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_egress_event_if_new(
    pool: &PgPool,
    event_id: Uuid,
    user_id: Uuid,
    agent_id: Option<Uuid>,
    orion_id: Uuid,
    event_type: &str,
    payload: serde_json::Value,
    enqueued_at: DateTime<Utc>,
    attempts: u32,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO orion_egress_events \
         (event_id, user_id, agent_id, orion_id, event_type, payload, enqueued_at, attempts) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(event_id)
    .bind(user_id)
    .bind(agent_id)
    .bind(orion_id)
    .bind(event_type)
    .bind(payload)
    .bind(enqueued_at)
    .bind(attempts as i32)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}
