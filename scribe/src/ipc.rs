//! IPC interface for Scribe daemon

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{info, error};

use crate::ScribeState;
use crate::journal::{LogEntry, Priority, Facility, JournalFilter};
use crate::storage;

/// IPC request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    /// Write log entry
    Log {
        priority: u8,
        facility: u8,
        identifier: String,
        message: String,
        pid: Option<u32>,
    },

    /// Query logs
    Query {
        since: Option<String>,
        until: Option<String>,
        priority: Option<u8>,
        identifier: Option<String>,
        grep: Option<String>,
        limit: Option<usize>,
        reverse: bool,
    },

    /// Get disk usage
    DiskUsage,

    /// Rotate journal
    Rotate,

    /// Vacuum (delete old archives)
    Vacuum,

    /// Verify integrity
    Verify,

    /// Flush to disk
    Flush,
}

/// IPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success { message: String },
    Entries(Vec<LogEntryInfo>),
    DiskUsage {
        total_size: u64,
        current_size: u64,
        compressed_size: u64,
        file_count: u64,
    },
    VerifyResult {
        valid_entries: u64,
        valid_archives: u64,
        corrupted_files: u64,
    },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntryInfo {
    pub timestamp: String,
    pub priority: String,
    pub facility: String,
    pub identifier: String,
    pub message: String,
    pub pid: Option<u32>,
}

impl From<&LogEntry> for LogEntryInfo {
    fn from(entry: &LogEntry) -> Self {
        Self {
            timestamp: entry.timestamp.to_rfc3339(),
            priority: entry.priority.as_str().to_string(),
            facility: entry.facility.as_str().to_string(),
            identifier: entry.identifier.clone(),
            message: entry.message.clone(),
            pid: entry.pid,
        }
    }
}

/// IPC server
pub struct ScribeServer {
    socket_path: String,
    state: Arc<RwLock<ScribeState>>,
}

impl ScribeServer {
    pub fn new(socket_path: &str, state: Arc<RwLock<ScribeState>>) -> Self {
        Self {
            socket_path: socket_path.to_string(),
            state,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let _ = std::fs::remove_file(&self.socket_path);

        if let Some(parent) = std::path::Path::new(&self.socket_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;

        info!("Scribe IPC listening on {}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let state = self.state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, state).await {
                            error!("Client error: {}", e);
                        }
                    });
                }
                Err(e) => error!("Accept error: {}", e),
            }
        }
    }
}

async fn handle_client(
    stream: UnixStream,
    state: Arc<RwLock<ScribeState>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => process_request(request, &state).await,
            Err(e) => IpcResponse::Error { message: e.to_string() },
        };

        let json = serde_json::to_string(&response)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        line.clear();
    }

    Ok(())
}

async fn process_request(
    request: IpcRequest,
    state: &RwLock<ScribeState>,
) -> IpcResponse {
    match request {
        IpcRequest::Log { priority, facility, identifier, message, pid } => {
            let entry = LogEntry {
                timestamp: chrono::Utc::now(),
                priority: Priority::from_u8(priority),
                facility: Facility::from_u8(facility),
                identifier,
                message,
                pid,
                uid: None,
                hostname: None,
                fields: std::collections::HashMap::new(),
            };

            let mut state = state.write().await;
            match state.journal.write(&entry) {
                Ok(()) => IpcResponse::Success { message: "Logged".to_string() },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::Query { since, until, priority, identifier, grep, limit, reverse } => {
            use crate::query::{parse_time, parse_priority};

            let filter = JournalFilter {
                since: since.and_then(|s| parse_time(&s)),
                until: until.and_then(|s| parse_time(&s)),
                priority: priority.map(Priority::from_u8),
                facility: None,
                identifier,
                pid: None,
                grep,
                limit,
                reverse,
            };

            let state = state.read().await;
            match state.journal.query(&filter) {
                Ok(entries) => {
                    let infos: Vec<LogEntryInfo> = entries.iter()
                        .map(LogEntryInfo::from)
                        .collect();
                    IpcResponse::Entries(infos)
                }
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::DiskUsage => {
            let state = state.read().await;
            match storage::disk_usage(std::path::Path::new(&state.config.journal_dir)) {
                Ok(usage) => IpcResponse::DiskUsage {
                    total_size: usage.total_size,
                    current_size: usage.current_size,
                    compressed_size: usage.compressed_size,
                    file_count: usage.file_count,
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::Rotate => {
            let mut state = state.write().await;
            match state.journal.rotate() {
                Ok(()) => IpcResponse::Success { message: "Rotated".to_string() },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::Vacuum => {
            let state = state.read().await;
            match storage::vacuum(std::path::Path::new(&state.config.journal_dir)) {
                Ok(freed) => IpcResponse::Success {
                    message: format!("Freed {} bytes", freed),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::Verify => {
            let state = state.read().await;
            match storage::verify(std::path::Path::new(&state.config.journal_dir)) {
                Ok(result) => IpcResponse::VerifyResult {
                    valid_entries: result.valid_entries,
                    valid_archives: result.valid_archives,
                    corrupted_files: result.corrupted_files,
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::Flush => {
            let mut state = state.write().await;
            match state.journal.flush() {
                Ok(()) => IpcResponse::Success { message: "Flushed".to_string() },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }
    }
}
