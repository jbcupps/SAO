use sqlx::PgPool;

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
pub struct VaultMasterKeyRow {
    pub id: i32,
    pub encrypted_key: Vec<u8>,
    pub kdf_salt: Vec<u8>,
    pub kdf_memory_cost: i32,
    pub kdf_time_cost: i32,
    pub kdf_parallelism: i32,
}

/// Check if a VMK has been initialized.
pub async fn vmk_exists(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM vault_master_key")
        .fetch_one(pool)
        .await?;
    Ok(count.0 > 0)
}

/// Retrieve the most recent VMK envelope.
pub async fn get_vmk(pool: &PgPool) -> Result<Option<VaultMasterKeyRow>, sqlx::Error> {
    sqlx::query_as::<_, VaultMasterKeyRow>(
        "SELECT id, encrypted_key, kdf_salt, kdf_memory_cost, kdf_time_cost, kdf_parallelism \
         FROM vault_master_key ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
}
