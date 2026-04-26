use sqlx::PgPool;
use uuid::Uuid;

// --- Row types ---

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct SkillCatalogRow {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub permissions: Vec<String>,
    pub api_endpoints: Vec<String>,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
    pub risk_level: String,
    pub status: String,
    pub policy_score: Option<i32>,
    pub policy_details: Option<serde_json::Value>,
    pub created_by_user_id: Option<Uuid>,
    pub created_by_agent_id: Option<Uuid>,
    pub reviewed_by_user_id: Option<Uuid>,
    pub review_notes: Option<String>,
    pub reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct AgentSkillBindingRow {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub skill_id: Uuid,
    pub status: String,
    pub config: Option<serde_json::Value>,
    pub declared_at: chrono::DateTime<chrono::Utc>,
    pub reviewed_by_user_id: Option<Uuid>,
    pub review_notes: Option<String>,
    pub reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct SkillReviewRow {
    pub id: i64,
    pub target_type: String,
    pub target_id: Uuid,
    pub action: String,
    pub reviewer_user_id: Option<Uuid>,
    pub policy_score: Option<i32>,
    pub policy_details: Option<serde_json::Value>,
    pub notes: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// --- Catalog CRUD ---

#[allow(clippy::too_many_arguments)]
pub async fn create_skill(
    pool: &PgPool,
    name: &str,
    version: &str,
    description: Option<&str>,
    author: Option<&str>,
    category: Option<&str>,
    tags: &[String],
    permissions: &[String],
    api_endpoints: &[String],
    input_schema: Option<serde_json::Value>,
    output_schema: Option<serde_json::Value>,
    risk_level: &str,
    status: &str,
    policy_score: Option<i32>,
    policy_details: Option<serde_json::Value>,
    created_by_user_id: Option<Uuid>,
    created_by_agent_id: Option<Uuid>,
) -> Result<SkillCatalogRow, sqlx::Error> {
    sqlx::query_as::<_, SkillCatalogRow>(
        "INSERT INTO skill_catalog \
         (name, version, description, author, category, tags, permissions, api_endpoints, \
          input_schema, output_schema, risk_level, status, policy_score, policy_details, \
          created_by_user_id, created_by_agent_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16) \
         RETURNING *",
    )
    .bind(name)
    .bind(version)
    .bind(description)
    .bind(author)
    .bind(category)
    .bind(tags)
    .bind(permissions)
    .bind(api_endpoints)
    .bind(input_schema)
    .bind(output_schema)
    .bind(risk_level)
    .bind(status)
    .bind(policy_score)
    .bind(policy_details)
    .bind(created_by_user_id)
    .bind(created_by_agent_id)
    .fetch_one(pool)
    .await
}

pub async fn list_skills(
    pool: &PgPool,
    status_filter: Option<&str>,
    category_filter: Option<&str>,
) -> Result<Vec<SkillCatalogRow>, sqlx::Error> {
    match (status_filter, category_filter) {
        (Some(s), Some(c)) => {
            sqlx::query_as::<_, SkillCatalogRow>(
                "SELECT * FROM skill_catalog WHERE status = $1 AND category = $2 ORDER BY created_at DESC",
            )
            .bind(s)
            .bind(c)
            .fetch_all(pool)
            .await
        }
        (Some(s), None) => {
            sqlx::query_as::<_, SkillCatalogRow>(
                "SELECT * FROM skill_catalog WHERE status = $1 ORDER BY created_at DESC",
            )
            .bind(s)
            .fetch_all(pool)
            .await
        }
        (None, Some(c)) => {
            sqlx::query_as::<_, SkillCatalogRow>(
                "SELECT * FROM skill_catalog WHERE category = $1 ORDER BY created_at DESC",
            )
            .bind(c)
            .fetch_all(pool)
            .await
        }
        (None, None) => {
            sqlx::query_as::<_, SkillCatalogRow>(
                "SELECT * FROM skill_catalog ORDER BY created_at DESC",
            )
            .fetch_all(pool)
            .await
        }
    }
}

pub async fn get_skill(pool: &PgPool, id: Uuid) -> Result<Option<SkillCatalogRow>, sqlx::Error> {
    sqlx::query_as::<_, SkillCatalogRow>("SELECT * FROM skill_catalog WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn find_skill_by_name_version(
    pool: &PgPool,
    name: &str,
    version: &str,
) -> Result<Option<SkillCatalogRow>, sqlx::Error> {
    sqlx::query_as::<_, SkillCatalogRow>(
        "SELECT * FROM skill_catalog WHERE name = $1 AND version = $2",
    )
    .bind(name)
    .bind(version)
    .fetch_optional(pool)
    .await
}

pub async fn update_skill_status(
    pool: &PgPool,
    id: Uuid,
    status: &str,
    reviewed_by_user_id: Option<Uuid>,
    review_notes: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE skill_catalog SET status = $1, reviewed_by_user_id = $2, review_notes = $3, \
         reviewed_at = now(), updated_at = now() WHERE id = $4",
    )
    .bind(status)
    .bind(reviewed_by_user_id)
    .bind(review_notes)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_skill_metadata(
    pool: &PgPool,
    id: Uuid,
    description: Option<&str>,
    author: Option<&str>,
    category: Option<&str>,
    tags: Option<&[String]>,
    permissions: Option<&[String]>,
    api_endpoints: Option<&[String]>,
    input_schema: Option<serde_json::Value>,
    output_schema: Option<serde_json::Value>,
    risk_level: Option<&str>,
) -> Result<bool, sqlx::Error> {
    // Dynamic update: build SET clause based on provided fields
    let mut sets = Vec::new();
    let mut param_idx = 1u32;

    macro_rules! add_field {
        ($field:expr, $val:expr) => {
            if $val.is_some() {
                sets.push(format!("{} = ${}", $field, param_idx));
                param_idx += 1;
            }
        };
    }

    add_field!("description", description);
    add_field!("author", author);
    add_field!("category", category);
    add_field!("tags", tags);
    add_field!("permissions", permissions);
    add_field!("api_endpoints", api_endpoints);
    add_field!("input_schema", input_schema);
    add_field!("output_schema", output_schema);
    add_field!("risk_level", risk_level);

    if sets.is_empty() {
        return Ok(false);
    }

    sets.push("updated_at = now()".to_string());

    let sql = format!(
        "UPDATE skill_catalog SET {} WHERE id = ${}",
        sets.join(", "),
        param_idx
    );

    let mut query = sqlx::query(&sql);

    if let Some(v) = description {
        query = query.bind(v);
    }
    if let Some(v) = author {
        query = query.bind(v);
    }
    if let Some(v) = category {
        query = query.bind(v);
    }
    if let Some(v) = tags {
        query = query.bind(v);
    }
    if let Some(v) = permissions {
        query = query.bind(v);
    }
    if let Some(v) = api_endpoints {
        query = query.bind(v);
    }
    if let Some(v) = input_schema {
        query = query.bind(v);
    }
    if let Some(v) = output_schema {
        query = query.bind(v);
    }
    if let Some(v) = risk_level {
        query = query.bind(v);
    }

    query = query.bind(id);
    let result = query.execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_skill(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM skill_catalog WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

// --- Binding CRUD ---

pub async fn create_binding(
    pool: &PgPool,
    agent_id: Uuid,
    skill_id: Uuid,
    status: &str,
    config: Option<serde_json::Value>,
) -> Result<AgentSkillBindingRow, sqlx::Error> {
    sqlx::query_as::<_, AgentSkillBindingRow>(
        "INSERT INTO agent_skill_bindings (agent_id, skill_id, status, config) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (agent_id, skill_id) \
         DO UPDATE SET status = $3, config = COALESCE($4, agent_skill_bindings.config), \
         declared_at = now(), updated_at = now() \
         RETURNING *",
    )
    .bind(agent_id)
    .bind(skill_id)
    .bind(status)
    .bind(config.unwrap_or(serde_json::json!({})))
    .fetch_one(pool)
    .await
}

pub async fn list_bindings_for_agent(
    pool: &PgPool,
    agent_id: Uuid,
) -> Result<Vec<AgentSkillBindingRow>, sqlx::Error> {
    sqlx::query_as::<_, AgentSkillBindingRow>(
        "SELECT * FROM agent_skill_bindings WHERE agent_id = $1 ORDER BY declared_at DESC",
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await
}

pub async fn list_pending_bindings(
    pool: &PgPool,
) -> Result<Vec<AgentSkillBindingRow>, sqlx::Error> {
    sqlx::query_as::<_, AgentSkillBindingRow>(
        "SELECT * FROM agent_skill_bindings WHERE status = 'pending_review' ORDER BY declared_at DESC",
    )
    .fetch_all(pool)
    .await
}

pub async fn update_binding_status(
    pool: &PgPool,
    id: Uuid,
    status: &str,
    reviewed_by_user_id: Option<Uuid>,
    review_notes: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE agent_skill_bindings SET status = $1, reviewed_by_user_id = $2, \
         review_notes = $3, reviewed_at = now(), updated_at = now() WHERE id = $4",
    )
    .bind(status)
    .bind(reviewed_by_user_id)
    .bind(review_notes)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

// --- Review CRUD ---

pub struct NewSkillReview {
    pub target_type: String,
    pub target_id: Uuid,
    pub action: String,
    pub reviewer_user_id: Option<Uuid>,
    pub policy_score: Option<i32>,
    pub policy_details: Option<serde_json::Value>,
    pub notes: Option<String>,
}

pub async fn insert_review(pool: &PgPool, review: NewSkillReview) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO skill_reviews \
         (target_type, target_id, action, reviewer_user_id, policy_score, policy_details, notes) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(review.target_type)
    .bind(review.target_id)
    .bind(review.action)
    .bind(review.reviewer_user_id)
    .bind(review.policy_score)
    .bind(review.policy_details)
    .bind(review.notes)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn list_reviews_for_target(
    pool: &PgPool,
    target_type: &str,
    target_id: Uuid,
) -> Result<Vec<SkillReviewRow>, sqlx::Error> {
    sqlx::query_as::<_, SkillReviewRow>(
        "SELECT * FROM skill_reviews WHERE target_type = $1 AND target_id = $2 ORDER BY created_at DESC",
    )
    .bind(target_type)
    .bind(target_id)
    .fetch_all(pool)
    .await
}
