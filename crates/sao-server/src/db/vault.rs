use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct VaultSecretRow {
    pub id: Uuid,
    pub owner_user_id: Option<Uuid>,
    pub secret_type: String,
    pub label: String,
    pub provider: Option<String>,
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub metadata: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct VaultSecretMetadataRow {
    pub id: Uuid,
    pub secret_type: String,
    pub label: String,
    pub provider: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[allow(clippy::too_many_arguments)]
pub async fn create_secret(
    pool: &PgPool,
    owner_user_id: Option<Uuid>,
    secret_type: &str,
    label: &str,
    provider: Option<&str>,
    ciphertext: &[u8],
    nonce: &[u8],
    metadata: Option<serde_json::Value>,
) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO vault_secrets (owner_user_id, secret_type, label, provider, ciphertext, nonce, metadata) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(owner_user_id)
    .bind(secret_type)
    .bind(label)
    .bind(provider)
    .bind(ciphertext)
    .bind(nonce)
    .bind(metadata.unwrap_or(serde_json::json!({})))
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn list_secrets_metadata(
    pool: &PgPool,
    owner_user_id: Option<Uuid>,
) -> Result<Vec<VaultSecretMetadataRow>, sqlx::Error> {
    if let Some(uid) = owner_user_id {
        sqlx::query_as::<_, VaultSecretMetadataRow>(
            "SELECT id, secret_type, label, provider, metadata, created_at, updated_at \
             FROM vault_secrets WHERE owner_user_id = $1 ORDER BY created_at",
        )
        .bind(uid)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, VaultSecretMetadataRow>(
            "SELECT id, secret_type, label, provider, metadata, created_at, updated_at \
             FROM vault_secrets ORDER BY created_at",
        )
        .fetch_all(pool)
        .await
    }
}

pub async fn get_secret(pool: &PgPool, id: Uuid) -> Result<Option<VaultSecretRow>, sqlx::Error> {
    sqlx::query_as::<_, VaultSecretRow>(
        "SELECT id, owner_user_id, secret_type, label, provider, ciphertext, nonce, metadata, created_at, updated_at \
         FROM vault_secrets WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn update_secret(
    pool: &PgPool,
    id: Uuid,
    label: Option<&str>,
    ciphertext: Option<&[u8]>,
    nonce: Option<&[u8]>,
    metadata: Option<serde_json::Value>,
) -> Result<bool, sqlx::Error> {
    // Build update dynamically based on provided fields
    let mut updates = Vec::new();
    let mut param_idx = 1;

    if label.is_some() {
        updates.push(format!("label = ${}", param_idx));
        param_idx += 1;
    }
    if ciphertext.is_some() {
        updates.push(format!("ciphertext = ${}", param_idx));
        param_idx += 1;
        updates.push(format!("nonce = ${}", param_idx));
        param_idx += 1;
    }
    if metadata.is_some() {
        updates.push(format!("metadata = ${}", param_idx));
        param_idx += 1;
    }
    updates.push("updated_at = now()".to_string());

    let sql = format!(
        "UPDATE vault_secrets SET {} WHERE id = ${}",
        updates.join(", "),
        param_idx
    );

    let mut query = sqlx::query(&sql);

    if let Some(l) = label {
        query = query.bind(l);
    }
    if let Some(ct) = ciphertext {
        query = query.bind(ct);
        query = query.bind(nonce.unwrap_or(&[]));
    }
    if let Some(m) = metadata {
        query = query.bind(m);
    }
    query = query.bind(id);

    let result = query.execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_secret(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM vault_secrets WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
