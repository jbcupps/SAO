use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

const ADMIN_ENTITY_CONFIG_KEY: &str = "sao.admin_entity";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminEntitySnapshot {
    pub id: Uuid,
    pub identity_agent_id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub secret_id: Uuid,
    pub role: String,
    pub deployment_target: String,
    pub iac_strategy: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AdminWorkItemRow {
    pub id: Uuid,
    pub admin_agent_id: Uuid,
    pub sequence_no: i32,
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub area: String,
    pub status: String,
    pub priority: i32,
    pub metadata: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminEntityOverview {
    pub admin_entity: AdminEntitySnapshot,
    pub work_items: Vec<AdminWorkItemRow>,
}

pub async fn get_admin_entity_snapshot(pool: &PgPool) -> Result<Option<AdminEntitySnapshot>> {
    let value = sqlx::query_scalar::<_, Value>("SELECT value FROM system_config WHERE key = $1")
        .bind(ADMIN_ENTITY_CONFIG_KEY)
        .fetch_optional(pool)
        .await
        .context("failed to read SAO admin entity snapshot")?;

    value
        .map(|value| {
            serde_json::from_value::<AdminEntitySnapshot>(value)
                .context("failed to decode SAO admin entity snapshot")
        })
        .transpose()
}

pub async fn list_work_items(
    pool: &PgPool,
    admin_agent_id: Uuid,
) -> Result<Vec<AdminWorkItemRow>, sqlx::Error> {
    sqlx::query_as::<_, AdminWorkItemRow>(
        "SELECT *
         FROM admin_work_items
         WHERE admin_agent_id = $1
         ORDER BY priority, sequence_no, created_at",
    )
    .bind(admin_agent_id)
    .fetch_all(pool)
    .await
}

pub async fn get_admin_entity_overview(pool: &PgPool) -> Result<Option<AdminEntityOverview>> {
    let snapshot = match get_admin_entity_snapshot(pool).await? {
        Some(snapshot) => snapshot,
        None => return Ok(None),
    };

    let work_items = list_work_items(pool, snapshot.id)
        .await
        .context("failed to load SAO admin entity work items")?;

    Ok(Some(AdminEntityOverview {
        admin_entity: snapshot,
        work_items,
    }))
}
