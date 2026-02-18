use argon2::{Argon2, Params, Version};
use rand::RngCore;

/// Default Argon2id parameters.
pub const DEFAULT_MEMORY_COST: u32 = 65536; // 64 MiB
pub const DEFAULT_TIME_COST: u32 = 3;
pub const DEFAULT_PARALLELISM: u32 = 1;

/// Generate a random 32-byte salt.
pub fn generate_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    salt
}

/// Derive a 256-bit key from a passphrase using Argon2id.
pub fn derive_key_from_passphrase(
    passphrase: &str,
    salt: &[u8],
    memory_cost: u32,
    time_cost: u32,
    parallelism: u32,
) -> Result<[u8; 32], String> {
    let params = Params::new(memory_cost, time_cost, parallelism, Some(32))
        .map_err(|e| format!("Invalid Argon2 params: {}", e))?;

    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);

    let mut output = [0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut output)
        .map_err(|e| format!("Argon2 KDF failed: {}", e))?;

    Ok(output)
}

/// Derive key with default parameters.
pub fn derive_key_default(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    derive_key_from_passphrase(
        passphrase,
        salt,
        DEFAULT_MEMORY_COST,
        DEFAULT_TIME_COST,
        DEFAULT_PARALLELISM,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_key_deterministic() {
        let salt = [1u8; 32];
        let key1 = derive_key_default("test passphrase", &salt).unwrap();
        let key2 = derive_key_default("test passphrase", &salt).unwrap();
        assert_eq!(key1, key2);
    }

    #[test]
    fn different_passphrases_different_keys() {
        let salt = [1u8; 32];
        let key1 = derive_key_default("passphrase one", &salt).unwrap();
        let key2 = derive_key_default("passphrase two", &salt).unwrap();
        assert_ne!(key1, key2);
    }

    #[test]
    fn different_salts_different_keys() {
        let salt1 = [1u8; 32];
        let salt2 = [2u8; 32];
        let key1 = derive_key_default("same passphrase", &salt1).unwrap();
        let key2 = derive_key_default("same passphrase", &salt2).unwrap();
        assert_ne!(key1, key2);
    }
}
