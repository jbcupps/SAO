use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[allow(dead_code)] // created_by + get() are read on review-style queries we'll add later
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct InstallerSourceRow {
    pub id: Uuid,
    pub kind: String,
    pub url: String,
    pub filename: String,
    pub version: String,
    pub expected_sha256: String,
    pub is_default: bool,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub created_by: Option<Uuid>,
}

pub async fn list(pool: &PgPool) -> Result<Vec<InstallerSourceRow>, sqlx::Error> {
    sqlx::query_as::<_, InstallerSourceRow>(
        "SELECT id, kind, url, filename, version, expected_sha256, is_default, enabled, created_at, created_by \
         FROM installer_sources ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
}

#[allow(dead_code)]
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<InstallerSourceRow>, sqlx::Error> {
    sqlx::query_as::<_, InstallerSourceRow>(
        "SELECT id, kind, url, filename, version, expected_sha256, is_default, enabled, created_at, created_by \
         FROM installer_sources WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn get_default(
    pool: &PgPool,
    kind: &str,
) -> Result<Option<InstallerSourceRow>, sqlx::Error> {
    sqlx::query_as::<_, InstallerSourceRow>(
        "SELECT id, kind, url, filename, version, expected_sha256, is_default, enabled, created_at, created_by \
         FROM installer_sources WHERE kind = $1 AND is_default = true AND enabled = true",
    )
    .bind(kind)
    .fetch_optional(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    pool: &PgPool,
    kind: &str,
    url: &str,
    filename: &str,
    version: &str,
    expected_sha256: &str,
    is_default: bool,
    created_by: Uuid,
) -> Result<InstallerSourceRow, sqlx::Error> {
    let mut tx = pool.begin().await?;
    if is_default {
        sqlx::query("UPDATE installer_sources SET is_default = false WHERE kind = $1")
            .bind(kind)
            .execute(&mut *tx)
            .await?;
    }
    let row = sqlx::query_as::<_, InstallerSourceRow>(
        "INSERT INTO installer_sources (kind, url, filename, version, expected_sha256, is_default, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         RETURNING id, kind, url, filename, version, expected_sha256, is_default, enabled, created_at, created_by",
    )
    .bind(kind)
    .bind(url)
    .bind(filename)
    .bind(version)
    .bind(expected_sha256)
    .bind(is_default)
    .bind(created_by)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row)
}

pub async fn set_default(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<InstallerSourceRow>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let kind: Option<(String,)> = sqlx::query_as("SELECT kind FROM installer_sources WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some((kind,)) = kind else {
        return Ok(None);
    };
    sqlx::query("UPDATE installer_sources SET is_default = false WHERE kind = $1")
        .bind(&kind)
        .execute(&mut *tx)
        .await?;
    let row = sqlx::query_as::<_, InstallerSourceRow>(
        "UPDATE installer_sources SET is_default = true WHERE id = $1 \
         RETURNING id, kind, url, filename, version, expected_sha256, is_default, enabled, created_at, created_by",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(row))
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM installer_sources WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
