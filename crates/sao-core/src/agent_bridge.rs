//! Agent bridge - REST/WebSocket interface for connecting agents to SAO.
//!
//! Agents register via POST /api/agents/register with their Ed25519 public key.
//! SAO verifies the signature chain (master key -> agent key) before accepting.
use serde::{Deserialize, Serialize};
/// Registration request from an agent.
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentRegistration {
    pub agent_id: String,
    pub public_key: String, // base64-encoded Ed25519 public key
    pub signature: String,  // base64-encoded signature from master key
    pub name: String,
    pub capabilities: Vec<String>,
}
/// Status report from a connected agent.
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentStatus {
    pub agent_id: String,
    pub state: AgentState,
    pub uptime_seconds: u64,
    pub last_activity: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentState {
    Online,
    Busy,
    Idle,
    Offline,
}
