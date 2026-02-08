use crate::{
    generate_master_key, load_master_key, sign_agent_key, verify_agent_signature, AgentEntry,
    GlobalConfig,
};
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use uuid::Uuid;

/// Information about an agent identity for the frontend/API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentityInfo {
    pub id: String,
    pub name: String,
    pub directory: String,
    pub birth_complete: bool,
    pub birth_date: Option<String>,
}

/// Thread-safe manager for multi-agent identities.
///
/// Responsibilities:
/// - Load/store GlobalConfig (agent registry)
/// - Load/verify master key
/// - Create new agents (generate keys, sign with master)
/// - List and verify registered agents
pub struct IdentityManager {
    data_root: PathBuf,
    global_config: RwLock<GlobalConfig>,
    master_key: SigningKey,
}

impl IdentityManager {
    /// Create a new IdentityManager, loading GlobalConfig and master key from disk.
    /// If master key doesn't exist, generates one (first-run bootstrap).
    pub fn new(data_root: PathBuf) -> anyhow::Result<Self> {
        // Ensure directories exist
        std::fs::create_dir_all(&data_root)?;
        let identities_dir = data_root.join("identities");
        std::fs::create_dir_all(&identities_dir)?;

        // Load or create master key
        let master_key_path = data_root.join("master.key");
        let master_key = if master_key_path.exists() {
            load_master_key(&master_key_path)?
        } else {
            tracing::info!("No master key found, generating new master key");
            generate_master_key(&data_root)?;
            load_master_key(&master_key_path)?
        };

        // Load or create global config
        let global_config = if GlobalConfig::config_path(&data_root).exists() {
            GlobalConfig::load(&data_root)?
        } else {
            let config = GlobalConfig::new(&data_root);
            config.save(&data_root)?;
            config
        };

        Ok(Self {
            data_root,
            global_config: RwLock::new(global_config),
            master_key,
        })
    }

    /// Get the data root path.
    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    /// Get the identities directory path.
    pub fn identities_dir(&self) -> PathBuf {
        self.data_root.join("identities")
    }

    /// Get the master verifying (public) key.
    pub fn master_pubkey(&self) -> VerifyingKey {
        self.master_key.verifying_key()
    }

    /// List all registered agents with their info.
    pub fn list_agents(&self) -> Result<Vec<AgentIdentityInfo>, String> {
        let gc = self.global_config.read().map_err(|e| e.to_string())?;
        let mut agents = Vec::new();

        for entry in &gc.agents {
            let agent_dir = if entry.directory.is_absolute() {
                entry.directory.clone()
            } else {
                self.data_root.join(&entry.directory)
            };

            // Check for agent config to determine birth status
            let config_path = agent_dir.join("config.json");
            let (birth_complete, birth_date) = if config_path.exists() {
                // Read the config JSON to check birth_complete and birth_timestamp
                match std::fs::read_to_string(&config_path) {
                    Ok(content) => {
                        let val: serde_json::Value =
                            serde_json::from_str(&content).unwrap_or_default();
                        let complete = val
                            .get("birth_complete")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let date = val
                            .get("birth_timestamp")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        (complete, date)
                    }
                    Err(_) => (false, None),
                }
            } else {
                (false, None)
            };

            agents.push(AgentIdentityInfo {
                id: entry.id.clone(),
                name: entry.name.clone(),
                directory: agent_dir.to_string_lossy().to_string(),
                birth_complete,
                birth_date,
            });
        }

        Ok(agents)
    }

    /// Verify an agent's signature against the master key.
    /// Returns Ok(()) if valid, Err with message if invalid.
    pub fn verify_agent(&self, agent_id: &str) -> Result<(), String> {
        let gc = self.global_config.read().map_err(|e| e.to_string())?;
        let entry = gc
            .find_agent(agent_id)
            .ok_or_else(|| format!("Agent {} not registered", agent_id))?;

        let agent_dir = if entry.directory.is_absolute() {
            entry.directory.clone()
        } else {
            self.data_root.join(&entry.directory)
        };

        // Read the agent's public key
        let pubkey_path = agent_dir.join("external_pubkey.bin");
        if !pubkey_path.exists() {
            return Err(format!(
                "Agent {} has no public key at {}",
                agent_id,
                pubkey_path.display()
            ));
        }
        let pubkey_bytes = std::fs::read(&pubkey_path).map_err(|e| e.to_string())?;
        let pubkey_array: [u8; 32] = pubkey_bytes
            .as_slice()
            .try_into()
            .map_err(|_| "Invalid public key length")?;
        let agent_pubkey = VerifyingKey::from_bytes(&pubkey_array)
            .map_err(|e| format!("Invalid public key: {}", e))?;

        // Read the signature
        let sig_path = agent_dir.join("signature.sig");
        if !sig_path.exists() {
            return Err(format!(
                "Agent {} has no signature at {}",
                agent_id,
                sig_path.display()
            ));
        }
        let sig_bytes = std::fs::read(&sig_path).map_err(|e| e.to_string())?;

        // Verify
        let master_pubkey = self.master_key.verifying_key();
        if !verify_agent_signature(&master_pubkey, &agent_pubkey, &sig_bytes) {
            return Err(format!(
                "SECURITY: Agent {} signature verification FAILED.",
                agent_id
            ));
        }

        Ok(())
    }

    /// Create a new agent identity. Generates UUID, creates directory, returns (uuid, agent_dir).
    /// Note: Unlike in abigail, SAO doesn't generate the agent's keypair—the agent does that
    /// itself and registers via the API. This method creates a placeholder entry.
    pub fn create_agent(&self, name: &str) -> Result<(String, PathBuf), String> {
        let uuid = Uuid::new_v4().to_string();
        let agent_dir = self.identities_dir().join(&uuid);

        // Create agent directory structure
        std::fs::create_dir_all(&agent_dir).map_err(|e| e.to_string())?;

        // Register in global config
        {
            let mut gc = self.global_config.write().map_err(|e| e.to_string())?;
            gc.register_agent(AgentEntry {
                id: uuid.clone(),
                name: name.to_string(),
                directory: PathBuf::from(format!("identities/{}", uuid)),
            })
            .map_err(|e| e.to_string())?;
            gc.save(&self.data_root).map_err(|e| e.to_string())?;
        }

        tracing::info!("Created new agent entry: {} ({})", name, uuid);
        Ok((uuid, agent_dir))
    }

    /// Get the agent directory path for a given UUID.
    pub fn agent_dir(&self, agent_id: &str) -> Result<PathBuf, String> {
        let gc = self.global_config.read().map_err(|e| e.to_string())?;
        let entry = gc
            .find_agent(agent_id)
            .ok_or_else(|| format!("Agent {} not registered", agent_id))?;

        let agent_dir = if entry.directory.is_absolute() {
            entry.directory.clone()
        } else {
            self.data_root.join(&entry.directory)
        };
        Ok(agent_dir)
    }

    /// Update an agent's name in the global config.
    pub fn update_agent_name(&self, agent_id: &str, new_name: &str) -> Result<(), String> {
        let mut gc = self.global_config.write().map_err(|e| e.to_string())?;
        if let Some(entry) = gc.agents.iter_mut().find(|a| a.id == agent_id) {
            entry.name = new_name.to_string();
            gc.save(&self.data_root).map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err(format!("Agent {} not registered", agent_id))
        }
    }

    /// Remove an agent by UUID.
    pub fn remove_agent(&self, agent_id: &str) -> Result<bool, String> {
        let mut gc = self.global_config.write().map_err(|e| e.to_string())?;
        let removed = gc.remove_agent(agent_id);
        if removed {
            gc.save(&self.data_root).map_err(|e| e.to_string())?;
        }
        Ok(removed)
    }

    /// Check if any agents exist.
    pub fn has_agents(&self) -> bool {
        self.global_config
            .read()
            .map(|gc| !gc.agents.is_empty())
            .unwrap_or(false)
    }
}
