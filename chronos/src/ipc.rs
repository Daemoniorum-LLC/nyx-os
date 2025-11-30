//! IPC interface for Chronos daemon

use crate::clock::ClockStatus;
use crate::ntp::SyncState;
use crate::timezone::TimezoneInfo;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// IPC request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    /// Get current time status
    GetStatus,

    /// Get NTP sync status
    GetSyncStatus,

    /// Force NTP synchronization
    ForceSync,

    /// Get timezone information
    GetTimezone,

    /// Set timezone
    SetTimezone { timezone: String },

    /// List available timezones
    ListTimezones { region: Option<String> },

    /// Get clock status
    GetClockStatus,

    /// Sync RTC from system clock
    SyncRtc,

    /// Get full daemon status
    GetDaemonStatus,
}

/// IPC response types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    /// Successful response with data
    Success { data: serde_json::Value },

    /// Error response
    Error { message: String },
}

impl IpcResponse {
    /// Create success response
    pub fn success<T: Serialize>(data: T) -> Self {
        Self::Success {
            data: serde_json::to_value(data).unwrap_or(serde_json::Value::Null),
        }
    }

    /// Create error response
    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }
}

/// Time status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeStatus {
    /// UTC time (ISO 8601)
    pub utc: String,
    /// Local time (ISO 8601)
    pub local: String,
    /// Unix timestamp
    pub unix_timestamp: f64,
    /// Timezone name
    pub timezone: String,
    /// UTC offset string
    pub utc_offset: String,
    /// Whether NTP is synchronized
    pub ntp_synchronized: bool,
}

/// Full daemon status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    /// Version
    pub version: String,
    /// Time status
    pub time: TimeStatus,
    /// NTP sync state
    pub ntp: NtpStatus,
    /// Clock status
    pub clock: ClockStatus,
    /// Timezone info
    pub timezone: TimezoneInfo,
}

/// NTP status for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NtpStatus {
    /// Is synchronized
    pub synchronized: bool,
    /// Last sync time (ISO 8601)
    pub last_sync: Option<String>,
    /// Last offset (seconds)
    pub last_offset: f64,
    /// Last delay (seconds)
    pub last_delay: f64,
    /// Current stratum
    pub stratum: u8,
    /// Reference server
    pub ref_server: Option<String>,
    /// Sync count
    pub sync_count: u64,
    /// Fail count
    pub fail_count: u64,
}

impl From<&SyncState> for NtpStatus {
    fn from(state: &SyncState) -> Self {
        Self {
            synchronized: state.synchronized,
            last_sync: state.last_sync.map(|t| {
                chrono::DateTime::<chrono::Utc>::from(t)
                    .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                    .to_string()
            }),
            last_offset: state.last_offset,
            last_delay: state.last_delay,
            stratum: state.stratum,
            ref_server: state.ref_server.clone(),
            sync_count: state.sync_count,
            fail_count: state.fail_count,
        }
    }
}

/// Handler for processing IPC requests
pub trait IpcHandler: Send + Sync {
    /// Handle an IPC request
    fn handle(&self, request: IpcRequest) -> impl std::future::Future<Output = IpcResponse> + Send;
}

/// IPC server
pub struct IpcServer<H: IpcHandler> {
    socket_path: String,
    handler: Arc<H>,
}

impl<H: IpcHandler + 'static> IpcServer<H> {
    /// Create new IPC server
    pub fn new(socket_path: impl Into<String>, handler: H) -> Self {
        Self {
            socket_path: socket_path.into(),
            handler: Arc::new(handler),
        }
    }

    /// Start the IPC server
    pub async fn run(&self) -> Result<()> {
        let path = Path::new(&self.socket_path);

        // Create parent directory
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Remove existing socket
        let _ = std::fs::remove_file(path);

        let listener = UnixListener::bind(path)?;
        info!("Chronos IPC listening on {}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let handler = self.handler.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, handler).await {
                            error!("Client error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }
}

async fn handle_client<H: IpcHandler>(stream: UnixStream, handler: Arc<H>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => {
                debug!("IPC request: {:?}", request);
                handler.handle(request).await
            }
            Err(e) => IpcResponse::error(format!("Invalid request: {}", e)),
        };

        let response_json = serde_json::to_string(&response)?;
        writer.write_all(response_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        line.clear();
    }

    Ok(())
}

/// IPC client for connecting to chronosd
pub struct IpcClient {
    socket_path: String,
}

impl IpcClient {
    /// Create new IPC client
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    /// Send request and receive response
    pub async fn send(&self, request: IpcRequest) -> Result<IpcResponse> {
        let mut stream = UnixStream::connect(&self.socket_path).await?;

        let request_json = serde_json::to_string(&request)?;
        stream.write_all(request_json.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        Ok(serde_json::from_str(&line)?)
    }

    /// Get time status
    pub async fn get_status(&self) -> Result<TimeStatus> {
        match self.send(IpcRequest::GetStatus).await? {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    /// Get NTP sync status
    pub async fn get_sync_status(&self) -> Result<NtpStatus> {
        match self.send(IpcRequest::GetSyncStatus).await? {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    /// Force NTP synchronization
    pub async fn force_sync(&self) -> Result<NtpStatus> {
        match self.send(IpcRequest::ForceSync).await? {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    /// Get timezone info
    pub async fn get_timezone(&self) -> Result<TimezoneInfo> {
        match self.send(IpcRequest::GetTimezone).await? {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    /// Set timezone
    pub async fn set_timezone(&self, timezone: &str) -> Result<TimezoneInfo> {
        match self
            .send(IpcRequest::SetTimezone {
                timezone: timezone.to_string(),
            })
            .await?
        {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    /// List available timezones
    pub async fn list_timezones(&self, region: Option<&str>) -> Result<Vec<String>> {
        match self
            .send(IpcRequest::ListTimezones {
                region: region.map(|s| s.to_string()),
            })
            .await?
        {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    /// Sync RTC
    pub async fn sync_rtc(&self) -> Result<()> {
        match self.send(IpcRequest::SyncRtc).await? {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    /// Get full daemon status
    pub async fn get_daemon_status(&self) -> Result<DaemonStatus> {
        match self.send(IpcRequest::GetDaemonStatus).await? {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }
}
