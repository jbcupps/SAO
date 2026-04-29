use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct EntityArchiveRow {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub agent_name: String,
    pub owner_user_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub reason: Option<String>,
    pub archive_path: String,
    pub manifest: serde_json::Value,
    pub egress_event_count: i32,
    pub memory_event_count: i32,
    pub created_at: DateTime<Utc>,
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_archive(
    pool: &PgPool,
    id: Uuid,
    agent_id: Uuid,
    agent_name: &str,
    owner_user_id: Option<Uuid>,
    created_by: Option<Uuid>,
    reason: Option<&str>,
    archive_path: &str,
    manifest: serde_json::Value,
    egress_event_count: i32,
    memory_event_count: i32,
) -> Result<EntityArchiveRow, sqlx::Error> {
    sqlx::query_as::<_, EntityArchiveRow>(
        "INSERT INTO entity_archives \
         (id, agent_id, agent_name, owner_user_id, created_by, reason, archive_path, manifest, egress_event_count, memory_event_count) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         RETURNING id, agent_id, agent_name, owner_user_id, created_by, reason, archive_path, manifest, egress_event_count, memory_event_count, created_at",
    )
    .bind(id)
    .bind(agent_id)
    .bind(agent_name)
    .bind(owner_user_id)
    .bind(created_by)
    .bind(reason)
    .bind(archive_path)
    .bind(manifest)
    .bind(egress_event_count)
    .bind(memory_event_count)
    .fetch_one(pool)
    .await
}

pub async fn list_archives(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<EntityArchiveRow>, sqlx::Error> {
    sqlx::query_as::<_, EntityArchiveRow>(
        "SELECT id, agent_id, agent_name, owner_user_id, created_by, reason, archive_path, manifest, egress_event_count, memory_event_count, created_at \
         FROM entity_archives \
         ORDER BY created_at DESC \
         LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}
