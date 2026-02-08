use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterKeyResult {
    pub master_key_path: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct MasterKeyStored {
    secret: Vec<u8>,
}

/// Generate a new Ed25519 master key and save it to `{data_dir}/master.key`.
pub fn generate_master_key(data_dir: &Path) -> anyhow::Result<MasterKeyResult> {
    let signing_key = SigningKey::generate(&mut OsRng);
    std::fs::create_dir_all(data_dir)?;
    let master_key_path = data_dir.join("master.key");
    let stored = MasterKeyStored {
        secret: signing_key.to_bytes().to_vec(),
    };
    std::fs::write(&master_key_path, serde_json::to_vec(&stored)?)?;
    Ok(MasterKeyResult { master_key_path })
}

/// Save a master key to a specific path.
pub fn save_master_key(key: &SigningKey, path: &Path) -> anyhow::Result<()> {
    let stored = MasterKeyStored {
        secret: key.to_bytes().to_vec(),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec(&stored)?)?;
    Ok(())
}

/// Load a master key from disk.
pub fn load_master_key(path: &Path) -> anyhow::Result<SigningKey> {
    let data = std::fs::read(path)?;
    let stored: MasterKeyStored = serde_json::from_slice(&data)?;
    let key_bytes: [u8; 32] = stored
        .secret
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid master key length"))?;
    Ok(SigningKey::from_bytes(&key_bytes))
}

/// Sign an agent's public key with the master key, producing a signature.
pub fn sign_agent_key(master_key: &SigningKey, agent_pubkey: &VerifyingKey) -> Vec<u8> {
    master_key.sign(agent_pubkey.as_bytes()).to_bytes().to_vec()
}

/// Verify that an agent's public key was signed by the master key.
pub fn verify_agent_signature(
    master_pubkey: &VerifyingKey,
    agent_pubkey: &VerifyingKey,
    signature_bytes: &[u8],
) -> bool {
    let Ok(sig) = Signature::from_slice(signature_bytes) else {
        return false;
    };
    master_pubkey.verify(agent_pubkey.as_bytes(), &sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_master_key_roundtrip() {
        let tmp = std::env::temp_dir().join("sao_master_key_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let result = generate_master_key(&tmp).unwrap();
        let _loaded = load_master_key(&result.master_key_path).unwrap();

        // Save and reload a specific key
        let original_key = SigningKey::generate(&mut OsRng);
        save_master_key(&original_key, &result.master_key_path).unwrap();
        let reloaded = load_master_key(&result.master_key_path).unwrap();
        assert_eq!(original_key.to_bytes(), reloaded.to_bytes());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_agent_key_signing_and_verification() {
        let master = SigningKey::generate(&mut OsRng);
        let agent = SigningKey::generate(&mut OsRng);
        let agent_pubkey = agent.verifying_key();
        let master_pubkey = master.verifying_key();

        let signature = sign_agent_key(&master, &agent_pubkey);
        assert!(verify_agent_signature(
            &master_pubkey,
            &agent_pubkey,
            &signature
        ));

        // Verify with wrong master key fails
        let wrong_master = SigningKey::generate(&mut OsRng);
        assert!(!verify_agent_signature(
            &wrong_master.verifying_key(),
            &agent_pubkey,
            &signature
        ));
    }
}
