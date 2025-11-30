//! IPC server and client for service management

use crate::lifecycle::LifecycleManager;
use crate::state::{ServiceState, StateManager};
use crate::unit::UnitRegistry;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{info, error, debug};

/// IPC request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    Start { name: String },
    Stop { name: String },
    Restart { name: String },
    Reload { name: String },
    Status { name: Option<String> },
    Enable { name: String },
    Disable { name: String },
    List { running_only: bool },
    Logs { name: String, lines: usize },
    FollowLogs { name: String },
    WatchdogPing { name: String },
    GetUnit { name: String },
    ReloadDaemon,
}

/// IPC response types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success { message: String },
    Status(ServiceStatus),
    StatusList(Vec<ServiceStatus>),
    List(Vec<ServiceListEntry>),
    Logs(Vec<String>),
    Unit(serde_json::Value),
    Error { message: String },
}

/// Service status for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    pub state: String,
    pub pid: Option<u32>,
    pub started_at: Option<String>,
    pub uptime: Option<String>,
    pub memory_bytes: Option<u64>,
    pub cpu_percent: Option<f64>,
    pub restart_count: Option<u32>,
    pub last_exit_code: Option<i32>,
    pub enabled: bool,
}

/// Service list entry for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceListEntry {
    pub name: String,
    pub state: String,
    pub pid: Option<u32>,
    pub uptime: Option<String>,
    pub enabled: bool,
}

/// IPC server
pub struct ServicedServer {
    socket_path: PathBuf,
    lifecycle: Arc<LifecycleManager>,
    states: Arc<RwLock<StateManager>>,
    units: Arc<RwLock<UnitRegistry>>,
}

impl ServicedServer {
    pub fn new(
        socket_path: PathBuf,
        lifecycle: Arc<LifecycleManager>,
        states: Arc<RwLock<StateManager>>,
        units: Arc<RwLock<UnitRegistry>>,
    ) -> Self {
        Self {
            socket_path,
            lifecycle,
            states,
            units,
        }
    }

    pub async fn run(&self) -> Result<()> {
        // Remove existing socket
        let _ = std::fs::remove_file(&self.socket_path);

        // Ensure parent directory exists
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;

        // Set socket permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.socket_path, std::fs::Permissions::from_mode(0o660))?;
        }

        info!("IPC server listening on {:?}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let lifecycle = self.lifecycle.clone();
                    let states = self.states.clone();
                    let units = self.units.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, lifecycle, states, units).await {
                            error!("Client handler error: {}", e);
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

async fn handle_client(
    stream: UnixStream,
    lifecycle: Arc<LifecycleManager>,
    states: Arc<RwLock<StateManager>>,
    units: Arc<RwLock<UnitRegistry>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        debug!("Received request: {}", line.trim());

        let response = match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => process_request(request, &lifecycle, &states, &units).await,
            Err(e) => IpcResponse::Error {
                message: format!("Invalid request: {}", e),
            },
        };

        let response_json = serde_json::to_string(&response)?;
        writer.write_all(response_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        line.clear();
    }

    Ok(())
}

async fn process_request(
    request: IpcRequest,
    lifecycle: &LifecycleManager,
    states: &RwLock<StateManager>,
    units: &RwLock<UnitRegistry>,
) -> IpcResponse {
    match request {
        IpcRequest::Start { name } => {
            match lifecycle.start(&name).await {
                Ok(()) => IpcResponse::Success {
                    message: format!("Started {}", name),
                },
                Err(e) => IpcResponse::Error {
                    message: e.to_string(),
                },
            }
        }

        IpcRequest::Stop { name } => {
            match lifecycle.stop(&name).await {
                Ok(()) => IpcResponse::Success {
                    message: format!("Stopped {}", name),
                },
                Err(e) => IpcResponse::Error {
                    message: e.to_string(),
                },
            }
        }

        IpcRequest::Restart { name } => {
            match lifecycle.restart(&name).await {
                Ok(()) => IpcResponse::Success {
                    message: format!("Restarted {}", name),
                },
                Err(e) => IpcResponse::Error {
                    message: e.to_string(),
                },
            }
        }

        IpcRequest::Reload { name } => {
            match lifecycle.reload(&name).await {
                Ok(()) => IpcResponse::Success {
                    message: format!("Reloaded {}", name),
                },
                Err(e) => IpcResponse::Error {
                    message: e.to_string(),
                },
            }
        }

        IpcRequest::Status { name } => {
            if let Some(name) = name {
                let state_mgr = states.read().await;
                let unit_reg = units.read().await;

                if let Some(status) = state_mgr.get(&name) {
                    let enabled = unit_reg.is_enabled(&name);
                    IpcResponse::Status(to_ipc_status(&name, status, enabled))
                } else {
                    IpcResponse::Error {
                        message: format!("Service not found: {}", name),
                    }
                }
            } else {
                // Return all statuses
                let state_mgr = states.read().await;
                let unit_reg = units.read().await;

                let statuses: Vec<ServiceStatus> = state_mgr.all()
                    .map(|(n, s)| {
                        let enabled = unit_reg.is_enabled(n);
                        to_ipc_status(n, s, enabled)
                    })
                    .collect();

                IpcResponse::StatusList(statuses)
            }
        }

        IpcRequest::Enable { name } => {
            let mut unit_reg = units.write().await;
            if unit_reg.enable(&name) {
                IpcResponse::Success {
                    message: format!("Enabled {}", name),
                }
            } else {
                IpcResponse::Error {
                    message: format!("Service not found: {}", name),
                }
            }
        }

        IpcRequest::Disable { name } => {
            let mut unit_reg = units.write().await;
            unit_reg.disable(&name);
            IpcResponse::Success {
                message: format!("Disabled {}", name),
            }
        }

        IpcRequest::List { running_only } => {
            let state_mgr = states.read().await;
            let unit_reg = units.read().await;

            let entries: Vec<ServiceListEntry> = if running_only {
                state_mgr.active()
                    .map(|(n, s)| ServiceListEntry {
                        name: n.to_string(),
                        state: s.state.as_str().to_string(),
                        pid: s.pid,
                        uptime: s.uptime_string(),
                        enabled: unit_reg.is_enabled(n),
                    })
                    .collect()
            } else {
                unit_reg.names()
                    .map(|n| {
                        let status = state_mgr.get(n);
                        ServiceListEntry {
                            name: n.to_string(),
                            state: status.map(|s| s.state.as_str()).unwrap_or("stopped").to_string(),
                            pid: status.and_then(|s| s.pid),
                            uptime: status.and_then(|s| s.uptime_string()),
                            enabled: unit_reg.is_enabled(n),
                        }
                    })
                    .collect()
            };

            IpcResponse::List(entries)
        }

        IpcRequest::Logs { name, lines } => {
            // Would read from log file
            let logs = vec![
                format!("[{}] Service logs would be here", name),
                format!("[{}] Showing last {} lines", name, lines),
            ];
            IpcResponse::Logs(logs)
        }

        IpcRequest::FollowLogs { name } => {
            IpcResponse::Success {
                message: format!("Following logs for {} (not implemented)", name),
            }
        }

        IpcRequest::WatchdogPing { name } => {
            // Would notify watchdog
            IpcResponse::Success {
                message: format!("Watchdog ping received for {}", name),
            }
        }

        IpcRequest::GetUnit { name } => {
            let unit_reg = units.read().await;
            if let Some(unit) = unit_reg.get(&name) {
                match serde_json::to_value(unit) {
                    Ok(json) => IpcResponse::Unit(json),
                    Err(e) => IpcResponse::Error {
                        message: format!("Failed to serialize unit: {}", e),
                    },
                }
            } else {
                IpcResponse::Error {
                    message: format!("Unit not found: {}", name),
                }
            }
        }

        IpcRequest::ReloadDaemon => {
            IpcResponse::Success {
                message: "Daemon reload triggered".to_string(),
            }
        }
    }
}

fn to_ipc_status(name: &str, status: &crate::state::ServiceStatus, enabled: bool) -> ServiceStatus {
    ServiceStatus {
        name: name.to_string(),
        state: status.state.as_str().to_string(),
        pid: status.pid,
        started_at: status.started_at.map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string()),
        uptime: status.uptime_string(),
        memory_bytes: status.memory_bytes,
        cpu_percent: status.cpu_percent,
        restart_count: Some(status.restart_count),
        last_exit_code: status.last_exit_code,
        enabled,
    }
}

/// IPC client
pub struct ServicedClient {
    socket_path: PathBuf,
}

impl ServicedClient {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    async fn send(&self, request: IpcRequest) -> Result<IpcResponse> {
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

    pub async fn start(&self, name: &str) -> Result<String> {
        match self.send(IpcRequest::Start { name: name.to_string() }).await? {
            IpcResponse::Success { message } => Ok(message),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn stop(&self, name: &str) -> Result<String> {
        match self.send(IpcRequest::Stop { name: name.to_string() }).await? {
            IpcResponse::Success { message } => Ok(message),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn restart(&self, name: &str) -> Result<String> {
        match self.send(IpcRequest::Restart { name: name.to_string() }).await? {
            IpcResponse::Success { message } => Ok(message),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn reload(&self, name: &str) -> Result<String> {
        match self.send(IpcRequest::Reload { name: name.to_string() }).await? {
            IpcResponse::Success { message } => Ok(message),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn status(&self, name: Option<&str>) -> Result<ServiceStatus> {
        match self.send(IpcRequest::Status { name: name.map(String::from) }).await? {
            IpcResponse::Status(status) => Ok(status),
            IpcResponse::StatusList(list) if !list.is_empty() => Ok(list[0].clone()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn enable(&self, name: &str) -> Result<String> {
        match self.send(IpcRequest::Enable { name: name.to_string() }).await? {
            IpcResponse::Success { message } => Ok(message),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn disable(&self, name: &str) -> Result<String> {
        match self.send(IpcRequest::Disable { name: name.to_string() }).await? {
            IpcResponse::Success { message } => Ok(message),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn list(&self, running_only: bool) -> Result<Vec<ServiceListEntry>> {
        match self.send(IpcRequest::List { running_only }).await? {
            IpcResponse::List(list) => Ok(list),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn logs(&self, name: &str, lines: usize) -> Result<Vec<String>> {
        match self.send(IpcRequest::Logs { name: name.to_string(), lines }).await? {
            IpcResponse::Logs(logs) => Ok(logs),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn follow_logs(&self, name: &str) -> Result<()> {
        // Would keep connection open and stream logs
        println!("Following logs for {} (not implemented)", name);
        Ok(())
    }
}

/// Convenience function to create a client
pub fn client(socket_path: impl Into<PathBuf>) -> ServicedClient {
    ServicedClient::new(socket_path.into())
}
