use anyhow::anyhow;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use sao_core::{GlobalConfig, WorkspaceConfig};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::{sleep, timeout};
use url::Url;
use uuid::Uuid;

const DEFAULT_SCHEDULER_INTERVAL_SECS: u64 = 30;

struct MemoryMigration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MEMORY_MIGRATIONS: &[MemoryMigration] = &[
    MemoryMigration {
        version: 1,
        name: "initial_memory_schema",
        sql: r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS conversation_turns (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                raw_json TEXT,
                metadata_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_conversation_turns_session_created
                ON conversation_turns(session_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS session_snapshots (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                summary TEXT NOT NULL,
                raw_json TEXT,
                metadata_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_session_snapshots_session_created
                ON session_snapshots(session_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS memory_entries (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL CHECK(kind IN ('ephemeral', 'distilled', 'crystallized')),
                title TEXT,
                content TEXT NOT NULL,
                source_session TEXT,
                confidence REAL,
                tags_json TEXT NOT NULL DEFAULT '[]',
                active INTEGER NOT NULL DEFAULT 1,
                superseded_by TEXT,
                metadata_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_memory_entries_kind_created
                ON memory_entries(kind, created_at DESC);

            CREATE TABLE IF NOT EXISTS archive_entries (
                id TEXT PRIMARY KEY,
                archive_kind TEXT NOT NULL,
                title TEXT,
                content TEXT NOT NULL,
                source_session TEXT,
                source_turn_id TEXT,
                metadata_json TEXT NOT NULL DEFAULT '{}',
                file_path TEXT,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_archive_entries_kind_created
                ON archive_entries(archive_kind, created_at DESC);

            CREATE TABLE IF NOT EXISTS import_runs (
                id TEXT PRIMARY KEY,
                import_type TEXT NOT NULL,
                source_path TEXT NOT NULL,
                checksum TEXT,
                status TEXT NOT NULL,
                preview_json TEXT,
                metadata_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                applied_at TEXT
            );
        "#,
    },
    MemoryMigration {
        version: 2,
        name: "jobs_and_profiles",
        sql: r#"
            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                kind TEXT NOT NULL,
                schedule_seconds INTEGER NOT NULL,
                payload_json TEXT NOT NULL DEFAULT '{}',
                enabled INTEGER NOT NULL DEFAULT 1,
                last_run_at TEXT,
                next_run_at TEXT,
                last_status TEXT,
                last_error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS connection_profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                driver TEXT NOT NULL,
                redacted_connection_string TEXT NOT NULL,
                ownership_confirmed INTEGER NOT NULL DEFAULT 0,
                notes TEXT,
                metadata_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
        "#,
    },
];

#[derive(Debug)]
pub enum RuntimeError {
    InvalidRequest(String),
    PathDenied {
        denied_path: PathBuf,
        reason: String,
        nearest_allowed_root: Option<PathBuf>,
        suggested_remediation: String,
    },
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    Other(anyhow::Error),
}

impl RuntimeError {
    pub fn status_code(&self) -> u16 {
        match self {
            Self::InvalidRequest(_) => 400,
            Self::PathDenied { .. } => 403,
            Self::Io(_) | Self::Sqlite(_) | Self::Json(_) | Self::Other(_) => 500,
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            Self::InvalidRequest(message) => json!({
                "error": message,
                "error_type": "invalid_request",
            }),
            Self::PathDenied {
                denied_path,
                reason,
                nearest_allowed_root,
                suggested_remediation,
            } => json!({
                "error": "Path access denied",
                "error_type": "path_denied",
                "denied_path": denied_path,
                "reason": reason,
                "nearest_allowed_root": nearest_allowed_root,
                "suggested_remediation": suggested_remediation,
            }),
            Self::Io(err) => json!({ "error": err.to_string(), "error_type": "io_error" }),
            Self::Sqlite(err) => json!({ "error": err.to_string(), "error_type": "sqlite_error" }),
            Self::Json(err) => json!({ "error": err.to_string(), "error_type": "json_error" }),
            Self::Other(err) => json!({ "error": err.to_string(), "error_type": "runtime_error" }),
        }
    }
}

impl From<std::io::Error> for RuntimeError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rusqlite::Error> for RuntimeError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

impl From<serde_json::Error> for RuntimeError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<anyhow::Error> for RuntimeError {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(value)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeWorkspace {
    pub data_root: PathBuf,
    pub current_working_directory: PathBuf,
    pub workspace_root: PathBuf,
    pub docs_inbox: PathBuf,
    pub memory_store_path: PathBuf,
    pub backup_path: PathBuf,
    pub archive_path: PathBuf,
    pub temp_path: PathBuf,
    pub allowed_read_roots: Vec<PathBuf>,
    pub allowed_write_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct RuntimeManager {
    workspace: RuntimeWorkspace,
    started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirectoryOperationResult {
    pub path: PathBuf,
    pub existed_before: bool,
    pub created: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteTestResult {
    pub target_directory: PathBuf,
    pub temporary_file: PathBuf,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryMigrationRecord {
    pub version: i64,
    pub name: String,
    pub applied_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryStatus {
    pub database_path: PathBuf,
    pub exists: bool,
    pub schema_valid: bool,
    pub table_count: usize,
    pub tables: Vec<String>,
    pub migrations: Vec<MemoryMigrationRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryBootstrapResult {
    pub database_path: PathBuf,
    pub created: bool,
    pub applied_versions: Vec<i64>,
    pub status: MemoryStatus,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConversationTurnInput {
    pub session_id: String,
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub raw: Option<Value>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationTurnRecord {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub raw: Option<Value>,
    pub metadata: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationAppendResult {
    pub turn: ConversationTurnRecord,
    pub archive_entry_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionSummaryRequest {
    pub session_id: String,
    #[serde(default = "default_summary_limit")]
    pub limit: usize,
    #[serde(default)]
    pub persist_memory: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummaryResult {
    pub snapshot_id: String,
    pub archive_entry_id: String,
    pub session_id: String,
    pub summary: String,
    pub persisted_memory_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionState {
    pub session_id: String,
    pub recent_turns: Vec<ConversationTurnRecord>,
    pub latest_summary: Option<ArchiveEntryRecord>,
    pub memories: Vec<MemoryEntryRecord>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryEntryInput {
    pub kind: String,
    #[serde(default)]
    pub title: Option<String>,
    pub content: String,
    #[serde(default)]
    pub source_session: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_true")]
    pub active: bool,
    #[serde(default)]
    pub superseded_by: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryEntryRecord {
    pub id: String,
    pub kind: String,
    pub title: Option<String>,
    pub content: String,
    pub source_session: Option<String>,
    pub confidence: Option<f64>,
    pub tags: Vec<String>,
    pub active: bool,
    pub superseded_by: Option<String>,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveEntryRecord {
    pub id: String,
    pub archive_kind: String,
    pub title: Option<String>,
    pub content: String,
    pub source_session: Option<String>,
    pub source_turn_id: Option<String>,
    pub metadata: Value,
    pub file_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArchiveQuery {
    #[serde(default)]
    pub archive_kind: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default = "default_archive_limit")]
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveStorageReport {
    pub memory_database: PathBuf,
    pub archive_root: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveSliceExport {
    pub file_path: PathBuf,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportManifestFile {
    pub file_name: String,
    pub sha256: String,
    pub lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportManifest {
    pub export_id: String,
    pub created_at: String,
    pub database_path: String,
    pub files: Vec<ExportManifestFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportBundleResult {
    pub export_id: String,
    pub bundle_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub sqlite_snapshot_path: PathBuf,
    pub markdown_summary_path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportRequest {
    pub bundle_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportPreviewResult {
    pub manifest: ExportManifest,
    pub bundle_dir: PathBuf,
    pub valid: bool,
    pub files_checked: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportRestoreResult {
    pub import_id: String,
    pub preview: ImportPreviewResult,
    pub rebuilt_indexes: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct IngestedFileRecord {
    pub path: PathBuf,
    pub classification: String,
    pub headings: Vec<String>,
    pub summary: String,
    pub schema_implications: Vec<String>,
    pub archive_entry_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IngestRequest {
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default = "default_true")]
    pub recursive: bool,
    #[serde(default = "default_true")]
    pub persist_archive: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct IngestReport {
    pub folder: PathBuf,
    pub scanned_files: usize,
    pub supported_files: usize,
    pub entries: Vec<IngestedFileRecord>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JobRequest {
    pub name: String,
    pub kind: String,
    pub schedule_seconds: i64,
    #[serde(default)]
    pub payload: Value,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JobUpdateRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub schedule_seconds: Option<i64>,
    #[serde(default)]
    pub payload: Option<Value>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobRecord {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub schedule_seconds: i64,
    pub payload: Value,
    pub enabled: bool,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobRunResult {
    pub job: JobRecord,
    pub outcome: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellExecutionMode {
    PowerShell,
    Argv,
    Script,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShellExecutionRequest {
    pub mode: ShellExecutionMode,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShellExecutionResponse {
    pub mode: ShellExecutionMode,
    pub program: String,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub script_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseProbeRequest {
    pub connection_string: String,
    #[serde(default)]
    pub ownership_confirmed: bool,
    #[serde(default)]
    pub create_disposable_schema: Option<String>,
    #[serde(default)]
    pub save_profile_name: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseProbeResponse {
    pub driver: String,
    pub redacted_connection_string: String,
    pub ownership_confirmed: bool,
    pub writes_permitted: bool,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityReport {
    pub started_at: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub network_available: bool,
    pub shell_modes: Vec<ShellExecutionMode>,
    pub workspace: RuntimeWorkspace,
    pub memory: MemoryStatus,
}

impl RuntimeManager {
    pub fn new(data_root: PathBuf) -> Result<Self, RuntimeError> {
        fs::create_dir_all(&data_root)?;
        let config_path = GlobalConfig::config_path(&data_root);
        let config = if config_path.exists() {
            GlobalConfig::load(&data_root)?
        } else {
            let config = GlobalConfig::new(&data_root);
            config.save(&data_root)?;
            config
        };

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let workspace = Self::resolve_workspace(&data_root, &cwd, &config.workspace);
        let manager = Self {
            workspace,
            started_at: Utc::now(),
        };
        manager.ensure_workspace_dirs()?;
        manager.bootstrap_memory_store()?;
        Ok(manager)
    }

    pub fn workspace(&self) -> &RuntimeWorkspace {
        &self.workspace
    }

    pub fn capabilities(&self) -> Result<CapabilityReport, RuntimeError> {
        Ok(CapabilityReport {
            started_at: self.started_at.to_rfc3339(),
            provider: std::env::var("SAO_PROVIDER")
                .ok()
                .or_else(|| std::env::var("OPENAI_API_PROVIDER").ok()),
            model: std::env::var("SAO_MODEL")
                .ok()
                .or_else(|| std::env::var("OPENAI_MODEL").ok()),
            network_available: true,
            shell_modes: vec![
                ShellExecutionMode::PowerShell,
                ShellExecutionMode::Argv,
                ShellExecutionMode::Script,
            ],
            workspace: self.workspace.clone(),
            memory: self.memory_status()?,
        })
    }

    pub fn create_directory(
        &self,
        raw_path: &str,
        recursive: bool,
    ) -> Result<DirectoryOperationResult, RuntimeError> {
        let target = self.resolve_user_path(raw_path);
        self.validate_write_path(&target)?;
        let existed_before = target.exists();
        if recursive {
            fs::create_dir_all(&target)?;
        } else {
            fs::create_dir(&target)?;
        }
        Ok(DirectoryOperationResult {
            path: target,
            existed_before,
            created: !existed_before,
        })
    }

    pub fn write_test(&self, raw_path: &str) -> Result<WriteTestResult, RuntimeError> {
        let target = self.resolve_user_path(raw_path);
        self.validate_write_path(&target)?;
        fs::create_dir_all(&target)?;
        let temporary_file = target.join(format!(".sao-write-test-{}", Uuid::new_v4()));
        fs::write(&temporary_file, b"ok")?;
        fs::remove_file(&temporary_file)?;
        Ok(WriteTestResult {
            target_directory: target,
            temporary_file,
            success: true,
        })
    }

    pub fn bootstrap_memory_store(&self) -> Result<MemoryBootstrapResult, RuntimeError> {
        let existed = self.workspace.memory_store_path.exists();
        let mut conn = self.open_memory_db()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL
            )",
        )?;

        let mut applied_versions = Vec::new();
        for migration in MEMORY_MIGRATIONS {
            let already_applied: Option<i64> = conn
                .query_row(
                    "SELECT version FROM schema_migrations WHERE version = ?1",
                    [migration.version],
                    |row| row.get(0),
                )
                .optional()?;
            if already_applied.is_some() {
                continue;
            }
            let tx = conn.transaction()?;
            tx.execute_batch(migration.sql)?;
            tx.execute(
                "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
                params![migration.version, migration.name, Utc::now().to_rfc3339()],
            )?;
            tx.commit()?;
            applied_versions.push(migration.version);
        }

        Ok(MemoryBootstrapResult {
            database_path: self.workspace.memory_store_path.clone(),
            created: !existed,
            applied_versions,
            status: self.memory_status()?,
        })
    }

    pub fn memory_status(&self) -> Result<MemoryStatus, RuntimeError> {
        let exists = self.workspace.memory_store_path.exists();
        if !exists {
            return Ok(MemoryStatus {
                database_path: self.workspace.memory_store_path.clone(),
                exists: false,
                schema_valid: false,
                table_count: 0,
                tables: Vec::new(),
                migrations: Vec::new(),
            });
        }

        let conn = self.open_memory_db()?;
        let tables = self.list_tables(&conn)?;
        let migrations = self.list_migrations_with_conn(&conn)?;
        let required_tables = [
            "schema_migrations",
            "conversation_turns",
            "session_snapshots",
            "memory_entries",
            "archive_entries",
            "import_runs",
            "jobs",
            "connection_profiles",
        ];
        let schema_valid = required_tables
            .iter()
            .all(|table| tables.iter().any(|entry| entry == table));

        Ok(MemoryStatus {
            database_path: self.workspace.memory_store_path.clone(),
            exists,
            schema_valid,
            table_count: tables.len(),
            tables,
            migrations,
        })
    }

    pub fn append_conversation_turn(
        &self,
        input: ConversationTurnInput,
    ) -> Result<ConversationAppendResult, RuntimeError> {
        if input.session_id.trim().is_empty() {
            return Err(RuntimeError::InvalidRequest(
                "session_id is required".to_string(),
            ));
        }
        if input.role.trim().is_empty() {
            return Err(RuntimeError::InvalidRequest("role is required".to_string()));
        }
        if input.content.trim().is_empty() {
            return Err(RuntimeError::InvalidRequest(
                "content is required".to_string(),
            ));
        }

        let mut conn = self.open_memory_db()?;
        let now = Utc::now().to_rfc3339();
        let turn_id = Uuid::new_v4().to_string();
        let archive_id = Uuid::new_v4().to_string();
        let metadata = input.metadata.unwrap_or_else(|| json!({}));
        let raw_text = match input.raw {
            Some(value) => Some(serde_json::to_string(&value)?),
            None => None,
        };

        conn.execute(
            "INSERT INTO conversation_turns (id, session_id, role, content, raw_json, metadata_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                turn_id,
                input.session_id,
                input.role,
                input.content,
                raw_text,
                serde_json::to_string(&metadata)?,
                now,
            ],
        )?;

        conn.execute(
            "INSERT INTO archive_entries (id, archive_kind, title, content, source_session, source_turn_id, metadata_json, file_path, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)",
            params![
                archive_id,
                "raw_turn",
                format!("{} turn", input.role),
                input.content,
                input.session_id,
                turn_id,
                serde_json::to_string(&json!({
                    "role": input.role,
                    "raw_present": raw_text.is_some(),
                }))?,
                now,
            ],
        )?;

        Ok(ConversationAppendResult {
            turn: ConversationTurnRecord {
                id: turn_id,
                session_id: input.session_id,
                role: input.role,
                content: input.content,
                raw: raw_text
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()
                    .unwrap_or(None),
                metadata,
                created_at: now,
            },
            archive_entry_id: archive_id,
        })
    }

    pub fn summarize_session(
        &self,
        request: SessionSummaryRequest,
    ) -> Result<SessionSummaryResult, RuntimeError> {
        let turns = self.list_conversation_turns(&request.session_id, request.limit)?;
        if turns.is_empty() {
            return Err(RuntimeError::InvalidRequest(format!(
                "No conversation turns found for session {}",
                request.session_id
            )));
        }

        let summary = build_summary(&request.session_id, &turns);
        let snapshot_id = Uuid::new_v4().to_string();
        let archive_entry_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let metadata = request.metadata.unwrap_or_else(|| json!({}));
        let mut conn = self.open_memory_db()?;
        conn.execute(
            "INSERT INTO session_snapshots (id, session_id, summary, raw_json, metadata_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                snapshot_id,
                request.session_id,
                summary,
                serde_json::to_string(&json!({ "turn_count": turns.len() }))?,
                serde_json::to_string(&metadata)?,
                now,
            ],
        )?;
        conn.execute(
            "INSERT INTO archive_entries (id, archive_kind, title, content, source_session, source_turn_id, metadata_json, file_path, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, NULL, ?7)",
            params![
                archive_entry_id,
                "session_summary",
                request
                    .title
                    .clone()
                    .unwrap_or_else(|| format!("Summary for {}", request.session_id)),
                summary,
                request.session_id,
                serde_json::to_string(&metadata)?,
                now,
            ],
        )?;

        let persisted_memory_id = if request.persist_memory {
            Some(
                self.store_memory(MemoryEntryInput {
                    kind: "distilled".to_string(),
                    title: request.title.clone(),
                    content: summary.clone(),
                    source_session: Some(request.session_id.clone()),
                    confidence: Some(0.75),
                    tags: vec!["session_summary".to_string()],
                    active: true,
                    superseded_by: None,
                    metadata: Some(metadata),
                })?
                .id,
            )
        } else {
            None
        };

        Ok(SessionSummaryResult {
            snapshot_id,
            archive_entry_id,
            session_id: request.session_id,
            summary,
            persisted_memory_id,
        })
    }

    pub fn recent_session_state(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<SessionState, RuntimeError> {
        let recent_turns = self.list_conversation_turns(session_id, limit)?;
        let latest_summary = self.latest_session_summary(session_id)?;
        let memories = self.list_memories(Some(session_id), None, limit, false)?;
        Ok(SessionState {
            session_id: session_id.to_string(),
            recent_turns,
            latest_summary,
            memories,
        })
    }

    pub fn store_memory(&self, input: MemoryEntryInput) -> Result<MemoryEntryRecord, RuntimeError> {
        validate_memory_kind(&input.kind)?;
        if input.content.trim().is_empty() {
            return Err(RuntimeError::InvalidRequest(
                "memory content is required".to_string(),
            ));
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let metadata = input.metadata.unwrap_or_else(|| json!({}));
        let tags_json = serde_json::to_string(&input.tags)?;
        let mut conn = self.open_memory_db()?;
        conn.execute(
            "INSERT INTO memory_entries
                (id, kind, title, content, source_session, confidence, tags_json, active, superseded_by, metadata_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id,
                input.kind,
                input.title,
                input.content,
                input.source_session,
                input.confidence,
                tags_json,
                i64::from(input.active),
                input.superseded_by,
                serde_json::to_string(&metadata)?,
                now,
                now,
            ],
        )?;
        self.get_memory(&id)?.ok_or_else(|| {
            RuntimeError::Other(anyhow!("memory entry {} was inserted but not readable", id))
        })
    }

    pub fn get_memory(&self, id: &str) -> Result<Option<MemoryEntryRecord>, RuntimeError> {
        let conn = self.open_memory_db()?;
        conn.query_row(
            "SELECT id, kind, title, content, source_session, confidence, tags_json, active, superseded_by, metadata_json, created_at, updated_at
             FROM memory_entries WHERE id = ?1",
            [id],
            row_to_memory,
        )
        .optional()
        .map_err(RuntimeError::from)
    }

    pub fn list_memories(
        &self,
        source_session: Option<&str>,
        kind: Option<&str>,
        limit: usize,
        active_only: bool,
    ) -> Result<Vec<MemoryEntryRecord>, RuntimeError> {
        let conn = self.open_memory_db()?;
        let mut sql = String::from(
            "SELECT id, kind, title, content, source_session, confidence, tags_json, active, superseded_by, metadata_json, created_at, updated_at
             FROM memory_entries WHERE 1=1",
        );
        let mut params_vec: Vec<String> = Vec::new();
        if let Some(value) = source_session {
            sql.push_str(" AND source_session = ?");
            params_vec.push(value.to_string());
        }
        if let Some(value) = kind {
            sql.push_str(" AND kind = ?");
            params_vec.push(value.to_string());
        }
        if active_only {
            sql.push_str(" AND active = 1");
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ?");
        params_vec.push(limit.to_string());

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params_vec.iter()), row_to_memory)?;
        collect_rows(rows)
    }

    pub fn list_archive(
        &self,
        query: ArchiveQuery,
    ) -> Result<Vec<ArchiveEntryRecord>, RuntimeError> {
        let conn = self.open_memory_db()?;
        let mut sql = String::from(
            "SELECT id, archive_kind, title, content, source_session, source_turn_id, metadata_json, file_path, created_at
             FROM archive_entries WHERE 1=1",
        );
        let mut params_vec: Vec<String> = Vec::new();
        if let Some(kind) = query.archive_kind {
            sql.push_str(" AND archive_kind = ?");
            params_vec.push(kind);
        }
        if let Some(session_id) = query.session_id {
            sql.push_str(" AND source_session = ?");
            params_vec.push(session_id);
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ?");
        params_vec.push(query.limit.to_string());
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(params_vec.iter()),
            row_to_archive_entry,
        )?;
        collect_rows(rows)
    }

    pub fn get_archive_entry(&self, id: &str) -> Result<Option<ArchiveEntryRecord>, RuntimeError> {
        let conn = self.open_memory_db()?;
        conn.query_row(
            "SELECT id, archive_kind, title, content, source_session, source_turn_id, metadata_json, file_path, created_at
             FROM archive_entries WHERE id = ?1",
            [id],
            row_to_archive_entry,
        )
        .optional()
        .map_err(RuntimeError::from)
    }

    pub fn search_archive(
        &self,
        search_term: &str,
        limit: usize,
    ) -> Result<Vec<ArchiveEntryRecord>, RuntimeError> {
        if search_term.trim().is_empty() {
            return Err(RuntimeError::InvalidRequest(
                "search query is required".to_string(),
            ));
        }
        let conn = self.open_memory_db()?;
        let pattern = format!("%{}%", search_term);
        let mut stmt = conn.prepare(
            "SELECT id, archive_kind, title, content, source_session, source_turn_id, metadata_json, file_path, created_at
             FROM archive_entries
             WHERE title LIKE ?1 OR content LIKE ?1 OR metadata_json LIKE ?1
             ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit as i64], row_to_archive_entry)?;
        collect_rows(rows)
    }

    pub fn archive_storage_report(&self) -> ArchiveStorageReport {
        ArchiveStorageReport {
            memory_database: self.workspace.memory_store_path.clone(),
            archive_root: self.workspace.archive_path.clone(),
        }
    }

    pub fn export_archive_slice(
        &self,
        search_term: &str,
        limit: usize,
    ) -> Result<ArchiveSliceExport, RuntimeError> {
        let entries = self.search_archive(search_term, limit)?;
        let export_dir = self.workspace.archive_path.join("exports");
        fs::create_dir_all(&export_dir)?;
        let file_path = export_dir.join(format!(
            "archive-slice-{}-{}.jsonl",
            Utc::now().format("%Y%m%d%H%M%S"),
            Uuid::new_v4()
        ));
        write_jsonl(
            &file_path,
            &entries
                .iter()
                .map(serde_json::to_value)
                .collect::<Result<Vec<_>, _>>()?,
        )?;
        Ok(ArchiveSliceExport {
            file_path,
            count: entries.len(),
        })
    }

    pub fn export_bundle(&self) -> Result<ExportBundleResult, RuntimeError> {
        let export_id = Uuid::new_v4().to_string();
        let bundle_dir = self
            .workspace
            .backup_path
            .join(format!("memory-export-{}", export_id));
        fs::create_dir_all(&bundle_dir)?;

        let conn = self.open_memory_db()?;
        let files = vec![
            (
                "conversation_turns.jsonl",
                query_values(
                    &conn,
                    "SELECT id, session_id, role, content, raw_json, metadata_json, created_at FROM conversation_turns ORDER BY created_at",
                    |row| {
                        Ok(json!({
                            "id": row.get::<_, String>(0)?,
                            "session_id": row.get::<_, String>(1)?,
                            "role": row.get::<_, String>(2)?,
                            "content": row.get::<_, String>(3)?,
                            "raw_json": row.get::<_, Option<String>>(4)?,
                            "metadata_json": row.get::<_, String>(5)?,
                            "created_at": row.get::<_, String>(6)?,
                        }))
                    },
                )?,
            ),
            (
                "session_snapshots.jsonl",
                query_values(
                    &conn,
                    "SELECT id, session_id, summary, raw_json, metadata_json, created_at FROM session_snapshots ORDER BY created_at",
                    |row| {
                        Ok(json!({
                            "id": row.get::<_, String>(0)?,
                            "session_id": row.get::<_, String>(1)?,
                            "summary": row.get::<_, String>(2)?,
                            "raw_json": row.get::<_, Option<String>>(3)?,
                            "metadata_json": row.get::<_, String>(4)?,
                            "created_at": row.get::<_, String>(5)?,
                        }))
                    },
                )?,
            ),
            (
                "memory_entries.jsonl",
                query_values(
                    &conn,
                    "SELECT id, kind, title, content, source_session, confidence, tags_json, active, superseded_by, metadata_json, created_at, updated_at FROM memory_entries ORDER BY created_at",
                    |row| {
                        Ok(json!({
                            "id": row.get::<_, String>(0)?,
                            "kind": row.get::<_, String>(1)?,
                            "title": row.get::<_, Option<String>>(2)?,
                            "content": row.get::<_, String>(3)?,
                            "source_session": row.get::<_, Option<String>>(4)?,
                            "confidence": row.get::<_, Option<f64>>(5)?,
                            "tags_json": row.get::<_, String>(6)?,
                            "active": row.get::<_, i64>(7)?,
                            "superseded_by": row.get::<_, Option<String>>(8)?,
                            "metadata_json": row.get::<_, String>(9)?,
                            "created_at": row.get::<_, String>(10)?,
                            "updated_at": row.get::<_, String>(11)?,
                        }))
                    },
                )?,
            ),
            (
                "archive_entries.jsonl",
                query_values(
                    &conn,
                    "SELECT id, archive_kind, title, content, source_session, source_turn_id, metadata_json, file_path, created_at FROM archive_entries ORDER BY created_at",
                    |row| {
                        Ok(json!({
                            "id": row.get::<_, String>(0)?,
                            "archive_kind": row.get::<_, String>(1)?,
                            "title": row.get::<_, Option<String>>(2)?,
                            "content": row.get::<_, String>(3)?,
                            "source_session": row.get::<_, Option<String>>(4)?,
                            "source_turn_id": row.get::<_, Option<String>>(5)?,
                            "metadata_json": row.get::<_, String>(6)?,
                            "file_path": row.get::<_, Option<String>>(7)?,
                            "created_at": row.get::<_, String>(8)?,
                        }))
                    },
                )?,
            ),
            (
                "jobs.jsonl",
                query_values(
                    &conn,
                    "SELECT id, name, kind, schedule_seconds, payload_json, enabled, last_run_at, next_run_at, last_status, last_error, created_at, updated_at FROM jobs ORDER BY created_at",
                    |row| {
                        Ok(json!({
                            "id": row.get::<_, String>(0)?,
                            "name": row.get::<_, String>(1)?,
                            "kind": row.get::<_, String>(2)?,
                            "schedule_seconds": row.get::<_, i64>(3)?,
                            "payload_json": row.get::<_, String>(4)?,
                            "enabled": row.get::<_, i64>(5)?,
                            "last_run_at": row.get::<_, Option<String>>(6)?,
                            "next_run_at": row.get::<_, Option<String>>(7)?,
                            "last_status": row.get::<_, Option<String>>(8)?,
                            "last_error": row.get::<_, Option<String>>(9)?,
                            "created_at": row.get::<_, String>(10)?,
                            "updated_at": row.get::<_, String>(11)?,
                        }))
                    },
                )?,
            ),
        ];

        let mut manifest_files = Vec::new();
        for (file_name, rows) in files {
            let path = bundle_dir.join(file_name);
            write_jsonl(&path, &rows)?;
            manifest_files.push(ExportManifestFile {
                file_name: file_name.to_string(),
                sha256: sha256_file(&path)?,
                lines: rows.len(),
            });
        }

        let sqlite_snapshot_path = bundle_dir.join("memory.sqlite3");
        fs::copy(&self.workspace.memory_store_path, &sqlite_snapshot_path)?;

        let markdown_summary_path = bundle_dir.join("bundle.md");
        let mut markdown = File::create(&markdown_summary_path)?;
        writeln!(markdown, "# Memory Export {}", export_id)?;
        writeln!(markdown, "- Created At: {}", Utc::now().to_rfc3339())?;
        writeln!(
            markdown,
            "- Memory DB: {}",
            self.workspace.memory_store_path.display()
        )?;
        for file in &manifest_files {
            writeln!(
                markdown,
                "- {}: {} record(s), sha256 `{}`",
                file.file_name, file.lines, file.sha256
            )?;
        }

        let manifest = ExportManifest {
            export_id: export_id.clone(),
            created_at: Utc::now().to_rfc3339(),
            database_path: self.workspace.memory_store_path.display().to_string(),
            files: manifest_files,
        };
        let manifest_path = bundle_dir.join("manifest.json");
        fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;

        Ok(ExportBundleResult {
            export_id,
            bundle_dir,
            manifest_path,
            sqlite_snapshot_path,
            markdown_summary_path,
        })
    }

    pub fn preview_import(
        &self,
        request: ImportRequest,
    ) -> Result<ImportPreviewResult, RuntimeError> {
        let bundle_dir = self.resolve_user_path(&request.bundle_path);
        self.validate_read_path(&bundle_dir)?;
        let manifest_path = bundle_dir.join("manifest.json");
        let manifest: ExportManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
        let mut valid = true;
        for file in &manifest.files {
            let path = bundle_dir.join(&file.file_name);
            if !path.exists() || sha256_file(&path)? != file.sha256 {
                valid = false;
            }
        }
        let files_checked = manifest.files.len();
        Ok(ImportPreviewResult {
            manifest,
            bundle_dir,
            valid,
            files_checked,
        })
    }

    pub fn restore_import(
        &self,
        request: ImportRequest,
    ) -> Result<ImportRestoreResult, RuntimeError> {
        let preview = self.preview_import(request)?;
        if !preview.valid {
            return Err(RuntimeError::InvalidRequest(
                "Import bundle failed integrity validation".to_string(),
            ));
        }
        self.bootstrap_memory_store()?;
        let mut conn = self.open_memory_db()?;
        let tx = conn.transaction()?;
        let import_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO import_runs (id, import_type, source_path, checksum, status, preview_json, metadata_json, created_at, applied_at)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5, '{}', ?6, ?7)",
            params![
                import_id,
                "bundle_restore",
                preview.bundle_dir.display().to_string(),
                "restored",
                serde_json::to_string(&preview.manifest)?,
                Utc::now().to_rfc3339(),
                Utc::now().to_rfc3339(),
            ],
        )?;

        for file in &preview.manifest.files {
            let path = preview.bundle_dir.join(&file.file_name);
            import_jsonl(&tx, &file.file_name, &path)?;
        }
        tx.execute_batch("REINDEX; ANALYZE;")?;
        tx.commit()?;

        Ok(ImportRestoreResult {
            import_id,
            preview,
            rebuilt_indexes: true,
        })
    }

    pub fn ingest_folder(&self, request: IngestRequest) -> Result<IngestReport, RuntimeError> {
        let folder = request
            .folder
            .as_deref()
            .map(|value| self.resolve_user_path(value))
            .unwrap_or_else(|| self.workspace.docs_inbox.clone());
        self.validate_read_path(&folder)?;

        let files = collect_files(&folder, request.recursive)?;
        let mut entries = Vec::new();
        let mut supported_files = 0;
        for file in files {
            let extension = file
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !matches!(extension.as_str(), "md" | "txt" | "json") {
                continue;
            }
            supported_files += 1;
            let content = fs::read_to_string(&file)?;
            let (classification, headings) = classify_file(&file, &content);
            let summary = summarize_text(&content);
            let schema_implications = infer_schema_implications(&content, &classification);
            let archive_entry_id = if request.persist_archive {
                Some(
                    self.persist_archive_entry(
                        "ingested_file",
                        Some(
                            file.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string(),
                        ),
                        content.clone(),
                        None,
                        None,
                        json!({
                            "path": file.display().to_string(),
                            "classification": classification,
                            "headings": headings,
                            "schema_implications": schema_implications,
                        }),
                    )?,
                )
            } else {
                None
            };
            entries.push(IngestedFileRecord {
                path: file,
                classification,
                headings,
                summary,
                schema_implications,
                archive_entry_id,
            });
        }

        Ok(IngestReport {
            folder,
            scanned_files: entries.len(),
            supported_files,
            entries,
        })
    }

    pub fn list_jobs(&self) -> Result<Vec<JobRecord>, RuntimeError> {
        let conn = self.open_memory_db()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, kind, schedule_seconds, payload_json, enabled, last_run_at, next_run_at, last_status, last_error, created_at, updated_at
             FROM jobs ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_job)?;
        collect_rows(rows)
    }

    pub fn get_job(&self, id: &str) -> Result<Option<JobRecord>, RuntimeError> {
        let conn = self.open_memory_db()?;
        conn.query_row(
            "SELECT id, name, kind, schedule_seconds, payload_json, enabled, last_run_at, next_run_at, last_status, last_error, created_at, updated_at
             FROM jobs WHERE id = ?1",
            [id],
            row_to_job,
        )
        .optional()
        .map_err(RuntimeError::from)
    }

    pub fn create_job(&self, request: JobRequest) -> Result<JobRecord, RuntimeError> {
        validate_job_kind(&request.kind)?;
        if request.schedule_seconds <= 0 {
            return Err(RuntimeError::InvalidRequest(
                "schedule_seconds must be greater than zero".to_string(),
            ));
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let next_run =
            (Utc::now() + chrono::Duration::seconds(request.schedule_seconds)).to_rfc3339();
        let mut conn = self.open_memory_db()?;
        conn.execute(
            "INSERT INTO jobs (id, name, kind, schedule_seconds, payload_json, enabled, last_run_at, next_run_at, last_status, last_error, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, NULL, NULL, ?8, ?9)",
            params![
                id,
                request.name,
                request.kind,
                request.schedule_seconds,
                serde_json::to_string(&request.payload)?,
                i64::from(request.enabled),
                next_run,
                now,
                now,
            ],
        )?;
        self.get_job(&id)?
            .ok_or_else(|| RuntimeError::Other(anyhow!("job creation failed")))
    }

    pub fn update_job(
        &self,
        id: &str,
        request: JobUpdateRequest,
    ) -> Result<Option<JobRecord>, RuntimeError> {
        let existing = match self.get_job(id)? {
            Some(job) => job,
            None => return Ok(None),
        };
        if let Some(value) = request.schedule_seconds {
            if value <= 0 {
                return Err(RuntimeError::InvalidRequest(
                    "schedule_seconds must be greater than zero".to_string(),
                ));
            }
        }
        let schedule_seconds = request
            .schedule_seconds
            .unwrap_or(existing.schedule_seconds);
        let enabled = request.enabled.unwrap_or(existing.enabled);
        let next_run = if enabled {
            Some((Utc::now() + chrono::Duration::seconds(schedule_seconds)).to_rfc3339())
        } else {
            None
        };
        let mut conn = self.open_memory_db()?;
        conn.execute(
            "UPDATE jobs
             SET name = ?2,
                 schedule_seconds = ?3,
                 payload_json = ?4,
                 enabled = ?5,
                 next_run_at = ?6,
                 updated_at = ?7
             WHERE id = ?1",
            params![
                id,
                request.name.unwrap_or(existing.name),
                schedule_seconds,
                serde_json::to_string(&request.payload.unwrap_or(existing.payload))?,
                i64::from(enabled),
                next_run,
                Utc::now().to_rfc3339(),
            ],
        )?;
        self.get_job(id)
    }

    pub fn delete_job(&self, id: &str) -> Result<bool, RuntimeError> {
        let mut conn = self.open_memory_db()?;
        Ok(conn.execute("DELETE FROM jobs WHERE id = ?1", [id])? > 0)
    }

    pub async fn run_job_now(&self, id: &str) -> Result<Option<JobRunResult>, RuntimeError> {
        let job = match self.get_job(id)? {
            Some(job) => job,
            None => return Ok(None),
        };
        let outcome = self.execute_job(&job).await?;
        let updated = self
            .get_job(id)?
            .ok_or_else(|| RuntimeError::Other(anyhow!("job disappeared after execution")))?;
        Ok(Some(JobRunResult {
            job: updated,
            outcome,
        }))
    }

    pub async fn run_due_jobs(&self) -> Result<Vec<JobRunResult>, RuntimeError> {
        let now = Utc::now().to_rfc3339();
        let due_jobs = {
            let conn = self.open_memory_db()?;
            let mut stmt = conn.prepare(
                "SELECT id, name, kind, schedule_seconds, payload_json, enabled, last_run_at, next_run_at, last_status, last_error, created_at, updated_at
                 FROM jobs
                 WHERE enabled = 1 AND next_run_at IS NOT NULL AND next_run_at <= ?1
                 ORDER BY next_run_at ASC",
            )?;
            let due_rows = stmt.query_map([now], row_to_job)?;
            collect_rows(due_rows)?
        };
        let mut results = Vec::new();
        for job in due_jobs {
            let outcome = self.execute_job(&job).await?;
            let updated = self
                .get_job(&job.id)?
                .ok_or_else(|| RuntimeError::Other(anyhow!("job disappeared after execution")))?;
            results.push(JobRunResult {
                job: updated,
                outcome,
            });
        }
        Ok(results)
    }

    pub async fn execute_shell(
        &self,
        request: ShellExecutionRequest,
    ) -> Result<ShellExecutionResponse, RuntimeError> {
        let working_directory = request
            .working_directory
            .as_deref()
            .map(|value| self.resolve_user_path(value))
            .unwrap_or_else(|| self.workspace.workspace_root.clone());
        self.validate_read_path(&working_directory)?;

        let timeout_secs = request.timeout_seconds.unwrap_or(60);
        let (program, args, script_path) = match request.mode {
            ShellExecutionMode::PowerShell => {
                let command = request.command.ok_or_else(|| {
                    RuntimeError::InvalidRequest("command is required for powershell mode".into())
                })?;
                (
                    powershell_binary(),
                    vec![
                        "-NoProfile".to_string(),
                        "-NonInteractive".to_string(),
                        "-ExecutionPolicy".to_string(),
                        "Bypass".to_string(),
                        "-Command".to_string(),
                        command,
                    ],
                    None,
                )
            }
            ShellExecutionMode::Argv => (
                request.program.ok_or_else(|| {
                    RuntimeError::InvalidRequest("program is required for argv mode".into())
                })?,
                request.args,
                None,
            ),
            ShellExecutionMode::Script => {
                let script_body = request.script.ok_or_else(|| {
                    RuntimeError::InvalidRequest("script is required for script mode".into())
                })?;
                let script_path = self
                    .workspace
                    .temp_path
                    .join(format!("sao-script-{}.ps1", Uuid::new_v4()));
                self.validate_write_path(&script_path)?;
                fs::create_dir_all(&self.workspace.temp_path)?;
                fs::write(&script_path, script_body)?;
                (
                    powershell_binary(),
                    vec![
                        "-NoProfile".to_string(),
                        "-NonInteractive".to_string(),
                        "-ExecutionPolicy".to_string(),
                        "Bypass".to_string(),
                        "-File".to_string(),
                        script_path.display().to_string(),
                    ],
                    Some(script_path),
                )
            }
        };

        let mut command = Command::new(&program);
        command
            .args(&args)
            .current_dir(&working_directory)
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output = match timeout(Duration::from_secs(timeout_secs), command.output()).await {
            Ok(result) => result.map_err(|err| RuntimeError::Other(anyhow!(err)))?,
            Err(_) => {
                return Ok(ShellExecutionResponse {
                    mode: request.mode,
                    program,
                    args,
                    working_directory,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("Command timed out after {} second(s)", timeout_secs),
                    timed_out: true,
                    script_path,
                });
            }
        };

        Ok(ShellExecutionResponse {
            mode: request.mode,
            program,
            args,
            working_directory,
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            timed_out: false,
            script_path,
        })
    }

    pub async fn probe_database(
        &self,
        request: DatabaseProbeRequest,
    ) -> Result<DatabaseProbeResponse, RuntimeError> {
        let redacted_connection_string = redact_connection_string(&request.connection_string);
        if let Some(ref name) = request.save_profile_name {
            self.upsert_connection_profile(
                name,
                &request.connection_string,
                request.ownership_confirmed,
                request.notes.as_deref(),
            )?;
        }

        if request.connection_string.starts_with("postgres://")
            || request.connection_string.starts_with("postgresql://")
        {
            if request.create_disposable_schema.is_some() && !request.ownership_confirmed {
                return Err(RuntimeError::InvalidRequest(
                    "ownership_confirmed=true is required before creating a disposable schema"
                        .to_string(),
                ));
            }
            let pool = PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(Duration::from_secs(3))
                .connect(&request.connection_string)
                .await
                .map_err(|err| RuntimeError::Other(anyhow!(err)))?;
            let version: (String,) = sqlx::query_as("SELECT version()")
                .fetch_one(&pool)
                .await
                .map_err(|err| RuntimeError::Other(anyhow!(err)))?;
            let identity: (String, String) =
                sqlx::query_as("SELECT current_database(), current_user")
                    .fetch_one(&pool)
                    .await
                    .map_err(|err| RuntimeError::Other(anyhow!(err)))?;
            if let Some(ref schema_name) = request.create_disposable_schema {
                validate_identifier(&schema_name)?;
                sqlx::query(&format!(
                    "CREATE SCHEMA IF NOT EXISTS {}",
                    quote_identifier(&schema_name)
                ))
                .execute(&pool)
                .await
                .map_err(|err| RuntimeError::Other(anyhow!(err)))?;
            }
            return Ok(DatabaseProbeResponse {
                driver: "postgres".to_string(),
                redacted_connection_string,
                ownership_confirmed: request.ownership_confirmed,
                writes_permitted: request.ownership_confirmed,
                metadata: json!({
                    "database": identity.0,
                    "user": identity.1,
                    "version": version.0,
                    "disposable_schema_created": request.create_disposable_schema.clone(),
                }),
            });
        }

        if request.connection_string.starts_with("sqlite:") {
            let path = sqlite_path_from_dsn(&request.connection_string)?;
            let resolved = if path.is_absolute() {
                path
            } else {
                self.resolve_user_path(path.to_string_lossy().as_ref())
            };
            self.validate_read_path(&resolved)?;
            let conn = Connection::open(&resolved)?;
            let version: String =
                conn.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
            let tables = self.list_tables(&conn)?;
            return Ok(DatabaseProbeResponse {
                driver: "sqlite".to_string(),
                redacted_connection_string,
                ownership_confirmed: request.ownership_confirmed,
                writes_permitted: request.ownership_confirmed,
                metadata: json!({
                    "database_path": resolved,
                    "version": version,
                    "tables": tables,
                }),
            });
        }

        Err(RuntimeError::InvalidRequest(
            "Only postgres://, postgresql://, and sqlite: connection strings are supported"
                .to_string(),
        ))
    }

    pub async fn scheduler_loop(self: Arc<Self>) {
        loop {
            if let Err(err) = self.run_due_jobs().await {
                tracing::error!("runtime scheduler failed: {}", err.to_value());
            }
            sleep(Duration::from_secs(DEFAULT_SCHEDULER_INTERVAL_SECS)).await;
        }
    }

    fn resolve_workspace(
        data_root: &Path,
        cwd: &Path,
        config: &WorkspaceConfig,
    ) -> RuntimeWorkspace {
        let workspace_root = resolve_config_path(data_root, &config.workspace_root);
        let docs_inbox = resolve_config_path(data_root, &config.docs_inbox);
        let memory_store_path = resolve_config_path(data_root, &config.memory_store_path);
        let backup_path = resolve_config_path(data_root, &config.backup_path);
        let archive_path = resolve_config_path(data_root, &config.archive_path);
        let temp_path = resolve_config_path(data_root, &config.temp_path);

        let mut read_roots: Vec<PathBuf> = config
            .allowed_read_roots
            .iter()
            .map(|path| resolve_config_path(data_root, path))
            .collect();
        read_roots.extend(env_roots("SAO_ALLOWED_READ_ROOTS"));
        read_roots.push(data_root.to_path_buf());
        read_roots.push(workspace_root.clone());
        read_roots.push(docs_inbox.clone());
        read_roots.push(cwd.to_path_buf());

        let mut write_roots: Vec<PathBuf> = config
            .allowed_write_roots
            .iter()
            .map(|path| resolve_config_path(data_root, path))
            .collect();
        write_roots.extend(env_roots("SAO_ALLOWED_WRITE_ROOTS"));
        write_roots.push(workspace_root.clone());
        write_roots.push(backup_path.clone());
        write_roots.push(archive_path.clone());
        write_roots.push(temp_path.clone());
        write_roots.push(
            memory_store_path
                .parent()
                .unwrap_or(&workspace_root)
                .to_path_buf(),
        );

        RuntimeWorkspace {
            data_root: data_root.to_path_buf(),
            current_working_directory: cwd.to_path_buf(),
            workspace_root,
            docs_inbox,
            memory_store_path,
            backup_path,
            archive_path,
            temp_path,
            allowed_read_roots: dedupe_roots(read_roots),
            allowed_write_roots: dedupe_roots(write_roots),
        }
    }

    fn ensure_workspace_dirs(&self) -> Result<(), RuntimeError> {
        for dir in [
            &self.workspace.workspace_root,
            &self.workspace.docs_inbox,
            &self.workspace.backup_path,
            &self.workspace.archive_path,
            &self.workspace.temp_path,
        ] {
            fs::create_dir_all(dir)?;
        }
        if let Some(parent) = self.workspace.memory_store_path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    fn open_memory_db(&self) -> Result<Connection, RuntimeError> {
        if let Some(parent) = self.workspace.memory_store_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&self.workspace.memory_store_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(conn)
    }

    fn list_tables(&self, conn: &Connection) -> Result<Vec<String>, RuntimeError> {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        collect_rows(rows)
    }

    fn list_migrations_with_conn(
        &self,
        conn: &Connection,
    ) -> Result<Vec<MemoryMigrationRecord>, RuntimeError> {
        let mut stmt = conn
            .prepare("SELECT version, name, applied_at FROM schema_migrations ORDER BY version")?;
        let rows = stmt.query_map([], |row| {
            Ok(MemoryMigrationRecord {
                version: row.get(0)?,
                name: row.get(1)?,
                applied_at: row.get(2)?,
            })
        })?;
        collect_rows(rows)
    }

    fn resolve_user_path(&self, raw: &str) -> PathBuf {
        let path = PathBuf::from(raw);
        let absolute = if path.is_absolute() {
            path
        } else {
            self.workspace.workspace_root.join(path)
        };
        normalize_path(&absolute)
    }

    fn validate_read_path(&self, path: &Path) -> Result<(), RuntimeError> {
        self.validate_path(path, &self.workspace.allowed_read_roots, "read")
    }

    fn validate_write_path(&self, path: &Path) -> Result<(), RuntimeError> {
        self.validate_path(path, &self.workspace.allowed_write_roots, "write")
    }

    fn validate_path(
        &self,
        path: &Path,
        allowed_roots: &[PathBuf],
        access: &str,
    ) -> Result<(), RuntimeError> {
        let normalized = normalize_path(path);
        if allowed_roots
            .iter()
            .map(|root| normalize_path(root))
            .any(|root| normalized.starts_with(&root))
        {
            return Ok(());
        }
        Err(RuntimeError::PathDenied {
            denied_path: normalized.clone(),
            reason: format!("{} access outside configured roots", access),
            nearest_allowed_root: nearest_allowed_root(&normalized, allowed_roots),
            suggested_remediation: format!(
                "Use a path inside {}",
                allowed_roots
                    .iter()
                    .map(|root| root.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })
    }

    fn list_conversation_turns(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<ConversationTurnRecord>, RuntimeError> {
        let conn = self.open_memory_db()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, raw_json, metadata_json, created_at
             FROM conversation_turns WHERE session_id = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_id, limit as i64], row_to_turn)?;
        collect_rows(rows)
    }

    fn latest_session_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<ArchiveEntryRecord>, RuntimeError> {
        let conn = self.open_memory_db()?;
        conn.query_row(
            "SELECT id, archive_kind, title, content, source_session, source_turn_id, metadata_json, file_path, created_at
             FROM archive_entries
             WHERE source_session = ?1 AND archive_kind = 'session_summary'
             ORDER BY created_at DESC LIMIT 1",
            [session_id],
            row_to_archive_entry,
        )
        .optional()
        .map_err(RuntimeError::from)
    }

    fn persist_archive_entry(
        &self,
        archive_kind: &str,
        title: Option<String>,
        content: String,
        source_session: Option<String>,
        source_turn_id: Option<String>,
        metadata: Value,
    ) -> Result<String, RuntimeError> {
        let id = Uuid::new_v4().to_string();
        let mut conn = self.open_memory_db()?;
        conn.execute(
            "INSERT INTO archive_entries (id, archive_kind, title, content, source_session, source_turn_id, metadata_json, file_path, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)",
            params![
                id,
                archive_kind,
                title,
                content,
                source_session,
                source_turn_id,
                serde_json::to_string(&metadata)?,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(id)
    }

    fn upsert_connection_profile(
        &self,
        name: &str,
        connection_string: &str,
        ownership_confirmed: bool,
        notes: Option<&str>,
    ) -> Result<(), RuntimeError> {
        let mut conn = self.open_memory_db()?;
        let driver = if connection_string.starts_with("postgres://")
            || connection_string.starts_with("postgresql://")
        {
            "postgres"
        } else if connection_string.starts_with("sqlite:") {
            "sqlite"
        } else {
            "unknown"
        };
        conn.execute(
            "INSERT INTO connection_profiles
                (id, name, driver, redacted_connection_string, ownership_confirmed, notes, metadata_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, '{}', ?7, ?8)
             ON CONFLICT(name) DO UPDATE SET
                driver = excluded.driver,
                redacted_connection_string = excluded.redacted_connection_string,
                ownership_confirmed = excluded.ownership_confirmed,
                notes = excluded.notes,
                updated_at = excluded.updated_at",
            params![
                Uuid::new_v4().to_string(),
                name,
                driver,
                redact_connection_string(connection_string),
                i64::from(ownership_confirmed),
                notes,
                Utc::now().to_rfc3339(),
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn execute_job(&self, job: &JobRecord) -> Result<Value, RuntimeError> {
        let outcome = match job.kind.as_str() {
            "sqlite_backup" => serde_json::to_value(self.export_bundle()?)?,
            "archive_snapshot" => {
                let session_id = job
                    .payload
                    .get("session_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        RuntimeError::InvalidRequest(
                            "archive_snapshot jobs require payload.session_id".to_string(),
                        )
                    })?;
                serde_json::to_value(
                    self.summarize_session(SessionSummaryRequest {
                        session_id: session_id.to_string(),
                        limit: job
                            .payload
                            .get("limit")
                            .and_then(Value::as_u64)
                            .unwrap_or(default_summary_limit() as u64)
                            as usize,
                        persist_memory: false,
                        title: job
                            .payload
                            .get("title")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        metadata: Some(json!({ "job_id": job.id, "kind": job.kind })),
                    })?,
                )?
            }
            "summary_generation" => {
                let session_id = job
                    .payload
                    .get("session_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        RuntimeError::InvalidRequest(
                            "summary_generation jobs require payload.session_id".to_string(),
                        )
                    })?;
                serde_json::to_value(
                    self.summarize_session(SessionSummaryRequest {
                        session_id: session_id.to_string(),
                        limit: job
                            .payload
                            .get("limit")
                            .and_then(Value::as_u64)
                            .unwrap_or(default_summary_limit() as u64)
                            as usize,
                        persist_memory: job
                            .payload
                            .get("persist_memory")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                        title: job
                            .payload
                            .get("title")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        metadata: Some(json!({ "job_id": job.id, "kind": job.kind })),
                    })?,
                )?
            }
            "script" => {
                let request: ShellExecutionRequest =
                    serde_json::from_value(job.payload.clone()).map_err(RuntimeError::from)?;
                serde_json::to_value(self.execute_shell(request).await?)?
            }
            _ => {
                return Err(RuntimeError::InvalidRequest(format!(
                    "Unsupported job kind {}",
                    job.kind
                )))
            }
        };

        let mut conn = self.open_memory_db()?;
        let now = Utc::now().to_rfc3339();
        let next_run = (Utc::now() + chrono::Duration::seconds(job.schedule_seconds)).to_rfc3339();
        conn.execute(
            "UPDATE jobs
             SET last_run_at = ?2,
                 next_run_at = ?3,
                 last_status = ?4,
                 last_error = NULL,
                 updated_at = ?5
             WHERE id = ?1",
            params![job.id, now, next_run, "ok", Utc::now().to_rfc3339()],
        )?;
        let _ = self.persist_archive_entry(
            "job_run",
            Some(format!("Job {} execution", job.name)),
            serde_json::to_string_pretty(&outcome)?,
            None,
            None,
            json!({ "job_id": job.id, "kind": job.kind }),
        );
        Ok(outcome)
    }
}

pub fn default_summary_limit() -> usize {
    20
}

fn default_archive_limit() -> usize {
    50
}

fn default_true() -> bool {
    true
}

fn validate_memory_kind(kind: &str) -> Result<(), RuntimeError> {
    if matches!(kind, "ephemeral" | "distilled" | "crystallized") {
        Ok(())
    } else {
        Err(RuntimeError::InvalidRequest(
            "kind must be one of: ephemeral, distilled, crystallized".to_string(),
        ))
    }
}

fn validate_job_kind(kind: &str) -> Result<(), RuntimeError> {
    if matches!(
        kind,
        "sqlite_backup" | "archive_snapshot" | "summary_generation" | "script"
    ) {
        Ok(())
    } else {
        Err(RuntimeError::InvalidRequest(
            "kind must be one of: sqlite_backup, archive_snapshot, summary_generation, script"
                .to_string(),
        ))
    }
}

fn resolve_config_path(data_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        data_root.join(path)
    }
}

fn env_roots(var_name: &str) -> Vec<PathBuf> {
    std::env::var(var_name)
        .ok()
        .map(|value| {
            value
                .split(if cfg!(windows) { ';' } else { ':' })
                .filter(|entry| !entry.trim().is_empty())
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn dedupe_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for root in roots {
        let normalized = normalize_path(&root);
        let key = normalized.to_string_lossy().to_string();
        if seen.insert(key) {
            out.push(normalized);
        }
    }
    out
}

fn normalize_path(path: &Path) -> PathBuf {
    if path.exists() {
        return fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    }
    if let Some(parent) = path.parent() {
        let canonical_parent = if parent.exists() {
            fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf())
        } else {
            normalize_path(parent)
        };
        if let Some(name) = path.file_name() {
            return canonical_parent.join(name);
        }
    }
    path.to_path_buf()
}

fn nearest_allowed_root(path: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    roots
        .iter()
        .max_by_key(|root| common_prefix_len(path, root))
        .cloned()
}

fn common_prefix_len(a: &Path, b: &Path) -> usize {
    a.components()
        .zip(b.components())
        .take_while(|(left, right)| left == right)
        .count()
}

fn row_to_turn(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConversationTurnRecord> {
    Ok(ConversationTurnRecord {
        id: row.get(0)?,
        session_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        raw: row
            .get::<_, Option<String>>(4)?
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .unwrap_or(None),
        metadata: parse_json_column(row.get::<_, String>(5)?),
        created_at: row.get(6)?,
    })
}

fn row_to_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEntryRecord> {
    Ok(MemoryEntryRecord {
        id: row.get(0)?,
        kind: row.get(1)?,
        title: row.get(2)?,
        content: row.get(3)?,
        source_session: row.get(4)?,
        confidence: row.get(5)?,
        tags: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
        active: row.get::<_, i64>(7)? != 0,
        superseded_by: row.get(8)?,
        metadata: parse_json_column(row.get::<_, String>(9)?),
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn row_to_archive_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArchiveEntryRecord> {
    Ok(ArchiveEntryRecord {
        id: row.get(0)?,
        archive_kind: row.get(1)?,
        title: row.get(2)?,
        content: row.get(3)?,
        source_session: row.get(4)?,
        source_turn_id: row.get(5)?,
        metadata: parse_json_column(row.get::<_, String>(6)?),
        file_path: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn row_to_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobRecord> {
    Ok(JobRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        schedule_seconds: row.get(3)?,
        payload: parse_json_column(row.get::<_, String>(4)?),
        enabled: row.get::<_, i64>(5)? != 0,
        last_run_at: row.get(6)?,
        next_run_at: row.get(7)?,
        last_status: row.get(8)?,
        last_error: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn parse_json_column(value: String) -> Value {
    serde_json::from_str(&value).unwrap_or_else(|_| json!({}))
}

fn collect_rows<T>(
    rows: impl IntoIterator<Item = rusqlite::Result<T>>,
) -> Result<Vec<T>, RuntimeError> {
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn build_summary(session_id: &str, turns: &[ConversationTurnRecord]) -> String {
    let roles = turns
        .iter()
        .map(|turn| turn.role.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let focus = turns
        .iter()
        .take(3)
        .map(|turn| {
            let snippet = turn
                .content
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or_default()
                .trim();
            truncate(snippet, 120)
        })
        .filter(|snippet| !snippet.is_empty())
        .collect::<Vec<_>>();
    format!(
        "Session {} captured {} turn(s). Roles present: {}. Recent focus: {}.",
        session_id,
        turns.len(),
        if roles.is_empty() {
            "unknown".to_string()
        } else {
            roles.join(", ")
        },
        if focus.is_empty() {
            "no non-empty content".to_string()
        } else {
            focus.join(" | ")
        }
    )
}

fn truncate(value: &str, max_len: usize) -> String {
    if value.chars().count() <= max_len {
        value.to_string()
    } else {
        value.chars().take(max_len).collect::<String>() + "..."
    }
}

fn query_values<F>(conn: &Connection, sql: &str, mut mapper: F) -> Result<Vec<Value>, RuntimeError>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<Value>,
{
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| mapper(row))?;
    collect_rows(rows)
}

fn write_jsonl(path: &Path, rows: &[Value]) -> Result<(), RuntimeError> {
    let mut file = File::create(path)?;
    for row in rows {
        writeln!(file, "{}", serde_json::to_string(row)?)?;
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, RuntimeError> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    Ok(digest.iter().map(|byte| format!("{:02x}", byte)).collect())
}

fn import_jsonl(
    tx: &rusqlite::Transaction<'_>,
    file_name: &str,
    path: &Path,
) -> Result<(), RuntimeError> {
    let file = File::open(path)?;
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(&line)?;
        match file_name {
            "conversation_turns.jsonl" => {
                tx.execute(
                    "INSERT OR IGNORE INTO conversation_turns (id, session_id, role, content, raw_json, metadata_json, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        value["id"].as_str(),
                        value["session_id"].as_str(),
                        value["role"].as_str(),
                        value["content"].as_str(),
                        value["raw_json"].as_str(),
                        value["metadata_json"].as_str(),
                        value["created_at"].as_str(),
                    ],
                )?;
            }
            "session_snapshots.jsonl" => {
                tx.execute(
                    "INSERT OR IGNORE INTO session_snapshots (id, session_id, summary, raw_json, metadata_json, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        value["id"].as_str(),
                        value["session_id"].as_str(),
                        value["summary"].as_str(),
                        value["raw_json"].as_str(),
                        value["metadata_json"].as_str(),
                        value["created_at"].as_str(),
                    ],
                )?;
            }
            "memory_entries.jsonl" => {
                tx.execute(
                    "INSERT OR IGNORE INTO memory_entries
                        (id, kind, title, content, source_session, confidence, tags_json, active, superseded_by, metadata_json, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        value["id"].as_str(),
                        value["kind"].as_str(),
                        value["title"].as_str(),
                        value["content"].as_str(),
                        value["source_session"].as_str(),
                        value["confidence"].as_f64(),
                        value["tags_json"].as_str(),
                        value["active"].as_i64(),
                        value["superseded_by"].as_str(),
                        value["metadata_json"].as_str(),
                        value["created_at"].as_str(),
                        value["updated_at"].as_str(),
                    ],
                )?;
            }
            "archive_entries.jsonl" => {
                tx.execute(
                    "INSERT OR IGNORE INTO archive_entries
                        (id, archive_kind, title, content, source_session, source_turn_id, metadata_json, file_path, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        value["id"].as_str(),
                        value["archive_kind"].as_str(),
                        value["title"].as_str(),
                        value["content"].as_str(),
                        value["source_session"].as_str(),
                        value["source_turn_id"].as_str(),
                        value["metadata_json"].as_str(),
                        value["file_path"].as_str(),
                        value["created_at"].as_str(),
                    ],
                )?;
            }
            "jobs.jsonl" => {
                tx.execute(
                    "INSERT OR IGNORE INTO jobs
                        (id, name, kind, schedule_seconds, payload_json, enabled, last_run_at, next_run_at, last_status, last_error, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        value["id"].as_str(),
                        value["name"].as_str(),
                        value["kind"].as_str(),
                        value["schedule_seconds"].as_i64(),
                        value["payload_json"].as_str(),
                        value["enabled"].as_i64(),
                        value["last_run_at"].as_str(),
                        value["next_run_at"].as_str(),
                        value["last_status"].as_str(),
                        value["last_error"].as_str(),
                        value["created_at"].as_str(),
                        value["updated_at"].as_str(),
                    ],
                )?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn collect_files(folder: &Path, recursive: bool) -> Result<Vec<PathBuf>, RuntimeError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && recursive {
            files.extend(collect_files(&path, recursive)?);
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(files)
}

fn classify_file(path: &Path, content: &str) -> (String, Vec<String>) {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "md" => (
            "markdown".to_string(),
            content
                .lines()
                .filter_map(|line| line.trim().strip_prefix('#').map(str::trim))
                .map(ToString::to_string)
                .take(10)
                .collect(),
        ),
        "json" => {
            let keys = serde_json::from_str::<Value>(content)
                .ok()
                .and_then(|value| value.as_object().cloned())
                .map(|object| object.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            ("json".to_string(), keys)
        }
        _ => (
            "text".to_string(),
            content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .take(5)
                .map(ToString::to_string)
                .collect(),
        ),
    }
}

fn summarize_text(content: &str) -> String {
    let lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(4)
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        "Empty or whitespace-only file".to_string()
    } else {
        truncate(&lines.join(" | "), 240)
    }
}

fn infer_schema_implications(content: &str, classification: &str) -> Vec<String> {
    let lowered = content.to_ascii_lowercase();
    let mut implications = Vec::new();
    if lowered.contains("migration") || lowered.contains("schema version") {
        implications.push("schema_versioning".to_string());
    }
    if lowered.contains("memory") || lowered.contains("archive") {
        implications.push("memory_classification".to_string());
    }
    if lowered.contains("backup") || lowered.contains("restore") || lowered.contains("import") {
        implications.push("recovery_pipeline".to_string());
    }
    if lowered.contains("job") || lowered.contains("scheduler") || lowered.contains("cron") {
        implications.push("scheduled_operations".to_string());
    }
    if lowered.contains("workspace") || lowered.contains("path") {
        implications.push("workspace_configuration".to_string());
    }
    if implications.is_empty() {
        implications.push(format!("{}_review", classification));
    }
    implications
}

fn powershell_binary() -> String {
    if cfg!(windows) {
        "powershell".to_string()
    } else {
        "pwsh".to_string()
    }
}

fn redact_connection_string(connection_string: &str) -> String {
    if let Ok(mut url) = Url::parse(connection_string) {
        if url.password().is_some() {
            let _ = url.set_password(Some("******"));
        }
        return url.to_string();
    }
    connection_string.to_string()
}

fn sqlite_path_from_dsn(dsn: &str) -> Result<PathBuf, RuntimeError> {
    let trimmed = dsn
        .trim_start_matches("sqlite://")
        .trim_start_matches("sqlite:");
    if trimmed.is_empty() {
        return Err(RuntimeError::InvalidRequest(
            "sqlite connection string must include a path".to_string(),
        ));
    }
    Ok(PathBuf::from(trimmed))
}

fn validate_identifier(value: &str) -> Result<(), RuntimeError> {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Ok(())
    } else {
        Err(RuntimeError::InvalidRequest(
            "identifier must contain only ASCII letters, digits, or underscores".to_string(),
        ))
    }
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_builder_mentions_session_and_roles() {
        let turns = vec![
            ConversationTurnRecord {
                id: "1".to_string(),
                session_id: "abc".to_string(),
                role: "user".to_string(),
                content: "Need a scheduler and recovery flow".to_string(),
                raw: None,
                metadata: json!({}),
                created_at: Utc::now().to_rfc3339(),
            },
            ConversationTurnRecord {
                id: "2".to_string(),
                session_id: "abc".to_string(),
                role: "assistant".to_string(),
                content: "Implement workspace and SQLite bootstrap".to_string(),
                raw: None,
                metadata: json!({}),
                created_at: Utc::now().to_rfc3339(),
            },
        ];

        let summary = build_summary("abc", &turns);
        assert!(summary.contains("abc"));
        assert!(summary.contains("user"));
        assert!(summary.contains("assistant"));
    }

    #[test]
    fn schema_implications_detect_backup_and_scheduler_keywords() {
        let implications =
            infer_schema_implications("Need backup restore workflow and a scheduler job", "text");
        assert!(implications.contains(&"recovery_pipeline".to_string()));
        assert!(implications.contains(&"scheduled_operations".to_string()));
    }
}
