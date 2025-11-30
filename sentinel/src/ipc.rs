//! IPC interface for Sentinel

use crate::alerts::{Alert, AlertCounts};
use crate::metrics::SystemSnapshot;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// IPC request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcRequest {
    /// Get current system metrics
    GetMetrics,

    /// Get CPU metrics
    GetCpu,

    /// Get memory metrics
    GetMemory,

    /// Get disk metrics
    GetDisks,

    /// Get network metrics
    GetNetworks,

    /// Get temperature metrics
    GetTemperatures,

    /// Get top processes
    GetProcesses,

    /// Get load average
    GetLoad,

    /// Get uptime
    GetUptime,

    /// Get active alerts
    GetAlerts,

    /// Get alert history
    GetAlertHistory { limit: Option<usize> },

    /// Get metrics history
    GetHistory { limit: Option<usize> },

    /// Get daemon status
    GetStatus,
}

/// IPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success { data: serde_json::Value },
    Error { message: String },
}

/// Daemon status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub version: String,
    pub uptime_secs: u64,
    pub collection_interval: u32,
    pub history_size: usize,
    pub alerts: AlertCounts,
}

/// IPC handler trait
pub trait IpcHandler: Send + Sync {
    fn get_metrics(&self) -> Option<SystemSnapshot>;
    fn get_history(&self, limit: usize) -> Vec<SystemSnapshot>;
    fn get_alerts(&self) -> Vec<Alert>;
    fn get_alert_history(&self, limit: usize) -> Vec<Alert>;
    fn get_status(&self) -> DaemonStatus;
}

/// IPC server
pub struct IpcServer<H: IpcHandler> {
    socket_path: String,
    handler: Arc<H>,
}

impl<H: IpcHandler + 'static> IpcServer<H> {
    pub fn new(socket_path: impl Into<String>, handler: H) -> Self {
        Self {
            socket_path: socket_path.into(),
            handler: Arc::new(handler),
        }
    }

    pub async fn run(&self) -> Result<()> {
        let _ = std::fs::remove_file(&self.socket_path);

        if let Some(parent) = std::path::Path::new(&self.socket_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        tracing::info!("Sentinel IPC listening on {}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let handler = Arc::clone(&self.handler);
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, handler).await {
                            tracing::error!("Client error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
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
            Ok(request) => process_request(request, handler.as_ref()),
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

fn process_request<H: IpcHandler>(request: IpcRequest, handler: &H) -> IpcResponse {
    match request {
        IpcRequest::GetMetrics => {
            match handler.get_metrics() {
                Some(metrics) => IpcResponse::Success {
                    data: serde_json::to_value(metrics).unwrap(),
                },
                None => IpcResponse::Error {
                    message: "No metrics available yet".to_string(),
                },
            }
        }

        IpcRequest::GetCpu => {
            match handler.get_metrics() {
                Some(metrics) => IpcResponse::Success {
                    data: serde_json::to_value(metrics.cpu).unwrap(),
                },
                None => IpcResponse::Error {
                    message: "No metrics available".to_string(),
                },
            }
        }

        IpcRequest::GetMemory => {
            match handler.get_metrics() {
                Some(metrics) => IpcResponse::Success {
                    data: serde_json::to_value(metrics.memory).unwrap(),
                },
                None => IpcResponse::Error {
                    message: "No metrics available".to_string(),
                },
            }
        }

        IpcRequest::GetDisks => {
            match handler.get_metrics() {
                Some(metrics) => IpcResponse::Success {
                    data: serde_json::to_value(metrics.disks).unwrap(),
                },
                None => IpcResponse::Error {
                    message: "No metrics available".to_string(),
                },
            }
        }

        IpcRequest::GetNetworks => {
            match handler.get_metrics() {
                Some(metrics) => IpcResponse::Success {
                    data: serde_json::to_value(metrics.networks).unwrap(),
                },
                None => IpcResponse::Error {
                    message: "No metrics available".to_string(),
                },
            }
        }

        IpcRequest::GetTemperatures => {
            match handler.get_metrics() {
                Some(metrics) => IpcResponse::Success {
                    data: serde_json::to_value(metrics.temperatures).unwrap(),
                },
                None => IpcResponse::Error {
                    message: "No metrics available".to_string(),
                },
            }
        }

        IpcRequest::GetProcesses => {
            match handler.get_metrics() {
                Some(metrics) => IpcResponse::Success {
                    data: serde_json::json!({
                        "top_cpu": metrics.top_cpu_processes,
                        "top_memory": metrics.top_memory_processes,
                    }),
                },
                None => IpcResponse::Error {
                    message: "No metrics available".to_string(),
                },
            }
        }

        IpcRequest::GetLoad => {
            match handler.get_metrics() {
                Some(metrics) => IpcResponse::Success {
                    data: serde_json::to_value(metrics.load).unwrap(),
                },
                None => IpcResponse::Error {
                    message: "No metrics available".to_string(),
                },
            }
        }

        IpcRequest::GetUptime => {
            match handler.get_metrics() {
                Some(metrics) => IpcResponse::Success {
                    data: serde_json::to_value(metrics.uptime).unwrap(),
                },
                None => IpcResponse::Error {
                    message: "No metrics available".to_string(),
                },
            }
        }

        IpcRequest::GetAlerts => {
            let alerts = handler.get_alerts();
            IpcResponse::Success {
                data: serde_json::to_value(alerts).unwrap(),
            }
        }

        IpcRequest::GetAlertHistory { limit } => {
            let alerts = handler.get_alert_history(limit.unwrap_or(50));
            IpcResponse::Success {
                data: serde_json::to_value(alerts).unwrap(),
            }
        }

        IpcRequest::GetHistory { limit } => {
            let history = handler.get_history(limit.unwrap_or(60));
            IpcResponse::Success {
                data: serde_json::to_value(history).unwrap(),
            }
        }

        IpcRequest::GetStatus => {
            let status = handler.get_status();
            IpcResponse::Success {
                data: serde_json::to_value(status).unwrap(),
            }
        }
    }
}

/// IPC client
pub struct IpcClient {
    socket_path: String,
}

impl IpcClient {
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

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

    pub async fn get_metrics(&self) -> Result<SystemSnapshot> {
        match self.send(IpcRequest::GetMetrics).await? {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn get_alerts(&self) -> Result<Vec<Alert>> {
        match self.send(IpcRequest::GetAlerts).await? {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn get_status(&self) -> Result<DaemonStatus> {
        match self.send(IpcRequest::GetStatus).await? {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }
}
