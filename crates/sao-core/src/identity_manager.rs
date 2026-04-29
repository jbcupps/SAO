use crate::{
    generate_master_key, load_master_key, verify_agent_signature, AgentEntry, GlobalConfig,
};
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
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
        let agent_dir = self.create_agent_with_id(&uuid, name)?;
        Ok((uuid, agent_dir))
    }

    /// Create a new agent identity using a caller-owned UUID.
    pub fn create_agent_with_id(&self, agent_id: &str, name: &str) -> Result<PathBuf, String> {
        let agent_dir = self.identities_dir().join(agent_id);

        // Create agent directory structure
        std::fs::create_dir_all(&agent_dir).map_err(|e| e.to_string())?;

        // Register in global config
        {
            let mut gc = self.global_config.write().map_err(|e| e.to_string())?;
            gc.register_agent(AgentEntry {
                id: agent_id.to_string(),
                name: name.to_string(),
                directory: PathBuf::from(format!("identities/{}", agent_id)),
            })
            .map_err(|e| e.to_string())?;
            gc.save(&self.data_root).map_err(|e| e.to_string())?;
        }

        tracing::info!("Created new agent entry: {} ({})", name, agent_id);
        Ok(agent_dir)
    }

    /// Create the four signed birth documents for a new agent.
    pub fn create_birth_documents(
        &self,
        agent_id: &str,
        agent_dir: &Path,
        name: &str,
        agent_type: Option<&str>,
        pubkey: Option<&str>,
    ) -> Result<(), String> {
        let created_at = chrono::Utc::now().to_rfc3339();
        let agent_type = agent_type.unwrap_or("personal");
        let pubkey = pubkey.unwrap_or("unspecified");

        let soul = format!(
            "# IMMUTABLE CONSTITUTIONAL ROOT — DO NOT MODIFY\n\n\
<!-- sao-signature: {signature} -->\n\n\
Agent ID: {agent_id}\n\
Name: {name}\n\
Created At: {created_at}\n\
Purpose: foundational identity and continuity.\n",
            signature =
                self.document_signature(format!("soul.md\n{agent_id}\n{name}\n{created_at}")),
            agent_id = agent_id,
            name = name,
            created_at = created_at,
        );

        let ethics_body = format!(
            "# ethics.md\n\n\
TriangleEthic commitments apply to {name}.\n\
Created At: {created_at}\n"
        );
        let ethics = format!(
            "<!-- sao-signature: {} -->\n\n{}",
            self.document_signature(&ethics_body),
            ethics_body
        );

        let org_map_body = format!(
            "# org-map.md\n\n\
agent_id: {agent_id}\n\
name: {name}\n\
type: {agent_type}\n\
pubkey: {pubkey}\n\
reports_to: SAO registry\n"
        );
        let org_map = format!(
            "<!-- sao-signature: {} -->\n\n{}",
            self.document_signature(&org_map_body),
            org_map_body
        );

        let personality_body = format!(
            "# personality.md\n\n\
name: {name}\n\
style: grounded\n\
tone: precise\n\
mutability: evolvable\n"
        );
        let personality = format!(
            "<!-- sao-signature: {} -->\n\n{}",
            self.document_signature(&personality_body),
            personality_body
        );

        self.write_birth_document(agent_dir, "soul.md", &soul, true)?;
        self.write_birth_document(agent_dir, "ethics.md", &ethics, false)?;
        self.write_birth_document(agent_dir, "org-map.md", &org_map, false)?;
        self.write_birth_document(agent_dir, "personality.md", &personality, false)?;
        self.write_birth_config(agent_dir, agent_id, name, agent_type, &created_at)?;

        Ok(())
    }

    pub fn archive_root(&self) -> Result<PathBuf, String> {
        let gc = self.global_config.read().map_err(|e| e.to_string())?;
        let archive_path = &gc.workspace.archive_path;
        Ok(if archive_path.is_absolute() {
            archive_path.clone()
        } else {
            self.data_root.join(archive_path)
        })
    }

    pub fn copy_agent_identity_to(
        &self,
        agent_id: &str,
        destination: &Path,
    ) -> Result<bool, String> {
        let source = self.agent_dir(agent_id)?;
        if !source.exists() {
            return Ok(false);
        }

        copy_dir_recursive(&source, destination).map_err(|e| e.to_string())?;
        Ok(true)
    }

    pub fn copy_agent_identity_for_archive(
        &self,
        agent_id: &str,
        agent_name: &str,
        destination: &Path,
    ) -> Result<Option<String>, String> {
        let (source, identity_agent_id) = {
            let gc = self.global_config.read().map_err(|e| e.to_string())?;
            let Some(entry) = gc
                .find_agent(agent_id)
                .or_else(|| gc.agents.iter().find(|entry| entry.name == agent_name))
            else {
                return Ok(None);
            };
            let agent_dir = if entry.directory.is_absolute() {
                entry.directory.clone()
            } else {
                self.data_root.join(&entry.directory)
            };
            (agent_dir, entry.id.clone())
        };

        if !source.exists() {
            return Ok(None);
        }

        copy_dir_recursive(&source, destination).map_err(|e| e.to_string())?;
        Ok(Some(identity_agent_id))
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

    /// Modify an existing agent document, except for soul.md which is immutable.
    pub fn modify_agent_document(
        &self,
        agent_id: &str,
        file_name: &str,
        contents: &str,
    ) -> Result<(), String> {
        if file_name.contains("soul.md") {
            return Err("soul.md is constitutionally immutable".into());
        }
        let agent_dir = self.agent_dir(agent_id)?;
        self.write_birth_document(&agent_dir, file_name, contents, false)
    }

    fn document_signature(&self, contents: impl AsRef<[u8]>) -> String {
        let signature = self.master_key.sign(contents.as_ref()).to_bytes();
        base64::engine::general_purpose::STANDARD.encode(signature)
    }

    fn write_birth_document(
        &self,
        agent_dir: &Path,
        file_name: &str,
        contents: &str,
        _readonly: bool,
    ) -> Result<(), String> {
        let path = agent_dir.join(file_name);
        std::fs::write(&path, contents).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn write_birth_config(
        &self,
        agent_dir: &Path,
        agent_id: &str,
        name: &str,
        agent_type: &str,
        created_at: &str,
    ) -> Result<(), String> {
        let config = serde_json::json!({
            "agent_id": agent_id,
            "name": name,
            "agent_type": agent_type,
            "birth_complete": true,
            "birth_timestamp": created_at,
            "documents": ["soul.md", "ethics.md", "org-map.md", "personality.md"],
            "personality": {
                "style": "grounded",
                "tone": "precise",
                "mutability": "evolvable"
            }
        });
        let path = agent_dir.join("config.json");
        let contents = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
        std::fs::write(&path, contents).map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(destination)?;

    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn birth_documents_use_caller_owned_agent_id_and_write_config() {
        let data_root = std::env::temp_dir().join(format!("sao-identity-{}", Uuid::new_v4()));
        let manager = IdentityManager::new(data_root.clone()).unwrap();
        let agent_id = Uuid::new_v4().to_string();

        let agent_dir = manager
            .create_agent_with_id(&agent_id, "Archive Test")
            .unwrap();
        manager
            .create_birth_documents(
                &agent_id,
                &agent_dir,
                "Archive Test",
                Some("personal"),
                None,
            )
            .unwrap();

        let config: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(agent_dir.join("config.json")).unwrap())
                .unwrap();

        assert_eq!(config["agent_id"], agent_id);
        assert_eq!(config["birth_complete"], true);
        assert!(agent_dir.join("soul.md").exists());
        assert!(agent_dir.join("personality.md").exists());

        fs::remove_dir_all(data_root).unwrap();
    }

    #[test]
    fn archive_copy_falls_back_to_legacy_identity_name_match() {
        let data_root =
            std::env::temp_dir().join(format!("sao-identity-legacy-{}", Uuid::new_v4()));
        let manager = IdentityManager::new(data_root.clone()).unwrap();
        let legacy_id = Uuid::new_v4().to_string();
        let db_id = Uuid::new_v4().to_string();

        let agent_dir = manager
            .create_agent_with_id(&legacy_id, "Legacy Agent")
            .unwrap();
        manager
            .create_birth_documents(
                &legacy_id,
                &agent_dir,
                "Legacy Agent",
                Some("personal"),
                None,
            )
            .unwrap();

        let archive_dir = data_root.join("archive-test");
        let copied_id = manager
            .copy_agent_identity_for_archive(&db_id, "Legacy Agent", &archive_dir)
            .unwrap();

        assert_eq!(copied_id.as_deref(), Some(legacy_id.as_str()));
        assert!(archive_dir.join("soul.md").exists());
        assert!(archive_dir.join("config.json").exists());

        fs::remove_dir_all(data_root).unwrap();
    }
}
