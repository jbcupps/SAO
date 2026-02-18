use sao_core::vault::VaultMasterKey;

/// Vault seal state.
pub enum VaultState {
    /// No VMK has been initialized yet (first-run).
    Uninitialized,
    /// VMK exists in DB but is not loaded into memory.
    Sealed,
    /// VMK is loaded and ready for encrypt/decrypt operations.
    Unsealed(VaultMasterKey),
}

impl VaultState {
    pub fn is_unsealed(&self) -> bool {
        matches!(self, VaultState::Unsealed(_))
    }

    pub fn status_str(&self) -> &'static str {
        match self {
            VaultState::Uninitialized => "uninitialized",
            VaultState::Sealed => "sealed",
            VaultState::Unsealed(_) => "unsealed",
        }
    }

    /// Get a reference to the VMK if unsealed.
    pub fn vmk(&self) -> Option<&VaultMasterKey> {
        match self {
            VaultState::Unsealed(vmk) => Some(vmk),
            _ => None,
        }
    }
}
