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

/// Store the initial sealed VMK envelope.
pub async fn insert_vmk(
    pool: &PgPool,
    encrypted_key: &[u8],
    kdf_salt: &[u8],
    kdf_memory_cost: i32,
    kdf_time_cost: i32,
    kdf_parallelism: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO vault_master_key \
         (encrypted_key, kdf_salt, kdf_memory_cost, kdf_time_cost, kdf_parallelism) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(encrypted_key)
    .bind(kdf_salt)
    .bind(kdf_memory_cost)
    .bind(kdf_time_cost)
    .bind(kdf_parallelism)
    .execute(pool)
    .await?;
    Ok(())
}

/// Rotate the passphrase envelope for an existing VMK row.
///
/// The underlying VaultMasterKey bytes are *not* changed; only the
/// passphrase-derived envelope (sealed ciphertext, KDF salt, AEAD nonce) is
/// replaced. Existing secret ciphertexts therefore stay valid.
///
/// This intentionally updates the existing row in place (and stamps
/// `rotated_at`) rather than inserting a new row, so `get_vmk()` keeps
/// returning a single source of truth for the current envelope and we never
/// race a stale row.
pub async fn rotate_vmk_envelope(
    pool: &PgPool,
    id: i32,
    encrypted_key: &[u8],
    kdf_salt: &[u8],
    kdf_memory_cost: i32,
    kdf_time_cost: i32,
    kdf_parallelism: i32,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE vault_master_key \
         SET encrypted_key = $2, \
             kdf_salt = $3, \
             kdf_memory_cost = $4, \
             kdf_time_cost = $5, \
             kdf_parallelism = $6, \
             rotated_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(encrypted_key)
    .bind(kdf_salt)
    .bind(kdf_memory_cost)
    .bind(kdf_time_cost)
    .bind(kdf_parallelism)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}
