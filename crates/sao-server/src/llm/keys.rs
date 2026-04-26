//! Vault lookups for LLM provider API keys.
//!
//! Keys are stored in `vault_secrets` with `provider = '<openai|anthropic>'`, `label = 'api_key'`,
//! `secret_type = 'api_key'`. The vault must be unsealed for reads.

use crate::state::AppState;

use super::LlmError;

pub async fn get_api_key(state: &AppState, provider: &str) -> Result<Option<String>, LlmError> {
    let row = sqlx::query_as::<_, (Vec<u8>, Vec<u8>)>(
        "SELECT ciphertext, nonce FROM vault_secrets \
         WHERE provider = $1 AND label = 'api_key' AND secret_type = 'api_key' \
         ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(provider)
    .fetch_optional(&state.inner.db)
    .await?;

    let Some((ciphertext, nonce)) = row else {
        return Ok(None);
    };

    let vs = state.inner.vault_state.read().await;
    let vmk = vs.vmk().ok_or(LlmError::VaultSealed)?;
    let plaintext = vmk
        .decrypt(&ciphertext, &nonce)
        .map_err(|e| LlmError::BadResponse(format!("vault decrypt: {e}")))?;

    Ok(Some(String::from_utf8_lossy(&plaintext).to_string()))
}
