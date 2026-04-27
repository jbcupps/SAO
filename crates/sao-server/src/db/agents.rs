use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct AgentRow {
    #[serde(skip_serializing)]
    pub owner_user_id: Option<Uuid>,
    pub id: Uuid,
    pub name: String,
    pub public_key: Option<Vec<u8>>,
    pub state: String,
    pub capabilities: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub default_provider: Option<String>,
    pub default_id_model: Option<String>,
    pub default_ego_model: Option<String>,
    pub installer_sha256: Option<String>,
    pub installer_filename: Option<String>,
    pub installer_version: Option<String>,
}

const SELECT_COLS: &str = "owner_user_id, id, name, public_key, state, capabilities, created_at, updated_at, default_provider, default_id_model, default_ego_model, installer_sha256, installer_filename, installer_version";

pub async fn set_installer_pin(
    pool: &PgPool,
    id: Uuid,
    sha256: &str,
    filename: &str,
    version: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE agents SET installer_sha256 = $2, installer_filename = $3, installer_version = $4, updated_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(sha256)
    .bind(filename)
    .bind(version)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_agents(
    pool: &PgPool,
    owner_filter: Option<Uuid>,
) -> Result<Vec<AgentRow>, sqlx::Error> {
    if let Some(owner_user_id) = owner_filter {
        sqlx::query_as::<_, AgentRow>(&format!(
            "SELECT {SELECT_COLS} FROM agents WHERE owner_user_id = $1 ORDER BY created_at"
        ))
        .bind(owner_user_id)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, AgentRow>(&format!(
            "SELECT {SELECT_COLS} FROM agents ORDER BY created_at"
        ))
        .fetch_all(pool)
        .await
    }
}

pub async fn create_agent(
    pool: &PgPool,
    owner_user_id: Uuid,
    name: &str,
    default_provider: Option<&str>,
    default_id_model: Option<&str>,
    default_ego_model: Option<&str>,
) -> Result<AgentRow, sqlx::Error> {
    sqlx::query_as::<_, AgentRow>(&format!(
        "INSERT INTO agents (owner_user_id, name, default_provider, default_id_model, default_ego_model) \
         VALUES ($1, $2, $3, $4, $5) RETURNING {SELECT_COLS}"
    ))
    .bind(owner_user_id)
    .bind(name)
    .bind(default_provider)
    .bind(default_id_model)
    .bind(default_ego_model)
    .fetch_one(pool)
    .await
}

pub async fn get_agent(pool: &PgPool, id: Uuid) -> Result<Option<AgentRow>, sqlx::Error> {
    sqlx::query_as::<_, AgentRow>(&format!("SELECT {SELECT_COLS} FROM agents WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn last_egress_at(
    pool: &PgPool,
    agent_id: Uuid,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, sqlx::Error> {
    let row: Option<(Option<chrono::DateTime<chrono::Utc>>,)> =
        sqlx::query_as("SELECT MAX(created_at) FROM orion_egress_events WHERE agent_id = $1")
            .bind(agent_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|r| r.0))
}

pub async fn delete_agent(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
