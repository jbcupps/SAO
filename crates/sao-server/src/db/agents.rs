use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct AgentRow {
    pub id: Uuid,
    pub name: String,
    pub public_key: Option<Vec<u8>>,
    pub state: String,
    pub capabilities: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_agents(pool: &PgPool) -> Result<Vec<AgentRow>, sqlx::Error> {
    sqlx::query_as::<_, AgentRow>(
        "SELECT id, name, public_key, state, capabilities, created_at, updated_at \
         FROM agents ORDER BY created_at",
    )
    .fetch_all(pool)
    .await
}

pub async fn create_agent(pool: &PgPool, name: &str) -> Result<AgentRow, sqlx::Error> {
    sqlx::query_as::<_, AgentRow>(
        "INSERT INTO agents (name) VALUES ($1) \
         RETURNING id, name, public_key, state, capabilities, created_at, updated_at",
    )
    .bind(name)
    .fetch_one(pool)
    .await
}

pub async fn get_agent(pool: &PgPool, id: Uuid) -> Result<Option<AgentRow>, sqlx::Error> {
    sqlx::query_as::<_, AgentRow>(
        "SELECT id, name, public_key, state, capabilities, created_at, updated_at \
         FROM agents WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn delete_agent(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
