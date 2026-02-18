use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct OidcProviderRow {
    pub id: Uuid,
    pub name: String,
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret_encrypted: Option<Vec<u8>>,
    pub scopes: String,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct OidcProviderPublicRow {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
}

pub async fn list_providers_public(
    pool: &PgPool,
) -> Result<Vec<OidcProviderPublicRow>, sqlx::Error> {
    sqlx::query_as::<_, OidcProviderPublicRow>(
        "SELECT id, name, enabled FROM oidc_providers WHERE enabled = true ORDER BY name",
    )
    .fetch_all(pool)
    .await
}

pub async fn list_providers(pool: &PgPool) -> Result<Vec<OidcProviderRow>, sqlx::Error> {
    sqlx::query_as::<_, OidcProviderRow>(
        "SELECT id, name, issuer_url, client_id, client_secret_encrypted, scopes, enabled, created_at, updated_at \
         FROM oidc_providers ORDER BY name",
    )
    .fetch_all(pool)
    .await
}

pub async fn get_provider(pool: &PgPool, id: Uuid) -> Result<Option<OidcProviderRow>, sqlx::Error> {
    sqlx::query_as::<_, OidcProviderRow>(
        "SELECT id, name, issuer_url, client_id, client_secret_encrypted, scopes, enabled, created_at, updated_at \
         FROM oidc_providers WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn create_provider(
    pool: &PgPool,
    name: &str,
    issuer_url: &str,
    client_id: &str,
    client_secret_encrypted: Option<&[u8]>,
    scopes: &str,
) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO oidc_providers (name, issuer_url, client_id, client_secret_encrypted, scopes) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(name)
    .bind(issuer_url)
    .bind(client_id)
    .bind(client_secret_encrypted)
    .bind(scopes)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_provider(
    pool: &PgPool,
    id: Uuid,
    name: Option<&str>,
    issuer_url: Option<&str>,
    client_id: Option<&str>,
    client_secret_encrypted: Option<&[u8]>,
    scopes: Option<&str>,
    enabled: Option<bool>,
) -> Result<bool, sqlx::Error> {
    // Simple full update (all fields)
    let result = sqlx::query(
        "UPDATE oidc_providers SET \
         name = COALESCE($1, name), \
         issuer_url = COALESCE($2, issuer_url), \
         client_id = COALESCE($3, client_id), \
         client_secret_encrypted = COALESCE($4, client_secret_encrypted), \
         scopes = COALESCE($5, scopes), \
         enabled = COALESCE($6, enabled), \
         updated_at = now() \
         WHERE id = $7",
    )
    .bind(name)
    .bind(issuer_url)
    .bind(client_id)
    .bind(client_secret_encrypted)
    .bind(scopes)
    .bind(enabled)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_provider(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM oidc_providers WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

// --- User links ---

pub async fn find_user_by_oidc(
    pool: &PgPool,
    provider_id: Uuid,
    subject: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM oidc_user_links WHERE provider_id = $1 AND subject = $2",
    )
    .bind(provider_id)
    .bind(subject)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

pub async fn link_user_to_oidc(
    pool: &PgPool,
    user_id: Uuid,
    provider_id: Uuid,
    subject: &str,
    email: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO oidc_user_links (user_id, provider_id, subject, email) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (provider_id, subject) DO NOTHING",
    )
    .bind(user_id)
    .bind(provider_id)
    .bind(subject)
    .bind(email)
    .execute(pool)
    .await?;
    Ok(())
}
