use aes_gcm_siv::{
    aead::{Aead, KeyInit, OsRng},
    Aes256GcmSiv, Nonce,
};
use rand::RngCore;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// AES-256-GCM-SIV encryption key. Zeroized on drop.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct VaultMasterKey {
    key_bytes: [u8; 32],
}

impl VaultMasterKey {
    /// Create a new random VMK.
    pub fn generate() -> Self {
        let mut key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut key_bytes);
        Self { key_bytes }
    }

    /// Reconstruct a VMK from raw bytes (e.g., after unsealing).
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { key_bytes: bytes }
    }

    /// Export raw key bytes (for sealing with passphrase KDF).
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key_bytes
    }

    /// Encrypt plaintext. Returns (ciphertext, nonce).
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
        let cipher = Aes256GcmSiv::new_from_slice(&self.key_bytes)
            .map_err(|e| format!("Failed to create cipher: {}", e))?;

        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| format!("Encryption failed: {}", e))?;

        Ok((ciphertext, nonce_bytes.to_vec()))
    }

    /// Decrypt ciphertext with the given nonce.
    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, String> {
        let cipher = Aes256GcmSiv::new_from_slice(&self.key_bytes)
            .map_err(|e| format!("Failed to create cipher: {}", e))?;

        let nonce = Nonce::from_slice(nonce);

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| format!("Decryption failed: {}", e))
    }

    /// Seal the VMK with a passphrase-derived key. Returns encrypted VMK bytes.
    pub fn seal(&self, passphrase_key: &[u8; 32]) -> Result<(Vec<u8>, Vec<u8>), String> {
        let cipher = Aes256GcmSiv::new_from_slice(passphrase_key)
            .map_err(|e| format!("Failed to create seal cipher: {}", e))?;

        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let sealed = cipher
            .encrypt(nonce, self.key_bytes.as_slice())
            .map_err(|e| format!("Failed to seal VMK: {}", e))?;

        Ok((sealed, nonce_bytes.to_vec()))
    }

    /// Unseal a VMK from encrypted bytes using a passphrase-derived key.
    pub fn unseal(
        sealed_bytes: &[u8],
        nonce: &[u8],
        passphrase_key: &[u8; 32],
    ) -> Result<Self, String> {
        let cipher = Aes256GcmSiv::new_from_slice(passphrase_key)
            .map_err(|e| format!("Failed to create unseal cipher: {}", e))?;

        let nonce = Nonce::from_slice(nonce);

        let key_bytes_vec = cipher
            .decrypt(nonce, sealed_bytes)
            .map_err(|_| "Failed to unseal VMK: wrong passphrase or corrupted data".to_string())?;

        let key_bytes: [u8; 32] = key_bytes_vec
            .try_into()
            .map_err(|_| "Unsealed VMK has wrong length".to_string())?;

        Ok(Self { key_bytes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let vmk = VaultMasterKey::generate();
        let plaintext = b"secret api key value";

        let (ciphertext, nonce) = vmk.encrypt(plaintext).unwrap();
        assert_ne!(ciphertext.as_slice(), plaintext);

        let decrypted = vmk.decrypt(&ciphertext, &nonce).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn seal_unseal_roundtrip() {
        let vmk = VaultMasterKey::generate();
        let passphrase_key = [42u8; 32];

        let (sealed, nonce) = vmk.seal(&passphrase_key).unwrap();
        let unsealed = VaultMasterKey::unseal(&sealed, &nonce, &passphrase_key).unwrap();

        assert_eq!(vmk.as_bytes(), unsealed.as_bytes());
    }

    #[test]
    fn unseal_wrong_passphrase_fails() {
        let vmk = VaultMasterKey::generate();
        let passphrase_key = [42u8; 32];
        let wrong_key = [99u8; 32];

        let (sealed, nonce) = vmk.seal(&passphrase_key).unwrap();
        let result = VaultMasterKey::unseal(&sealed, &nonce, &wrong_key);
        assert!(result.is_err());
    }
}
