//! IPC interface for Slumber

use crate::battery::PowerStatus;
use crate::profiles::ProfileStatus;
use crate::sleep::SleepStatus;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// IPC request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcRequest {
    /// Get power status (battery, AC)
    GetPowerStatus,

    /// Get current profile
    GetProfile,

    /// Set power profile
    SetProfile { name: String },

    /// List available profiles
    ListProfiles,

    /// Get sleep status
    GetSleepStatus,

    /// Suspend to RAM
    Suspend,

    /// Hibernate to disk
    Hibernate,

    /// Hybrid sleep
    HybridSleep,

    /// Get full daemon status
    GetStatus,
}

/// IPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success { data: serde_json::Value },
    Error { message: String },
}

/// Full daemon status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub version: String,
    pub power: PowerStatus,
    pub profile: ProfileStatus,
    pub sleep: SleepStatus,
}

/// IPC handler trait
pub trait IpcHandler: Send + Sync {
    fn get_power_status(&self) -> Result<PowerStatus>;
    fn get_profile(&self) -> ProfileStatus;
    fn set_profile(&self, name: &str) -> Result<()>;
    fn list_profiles(&self) -> Vec<String>;
    fn get_sleep_status(&self) -> SleepStatus;
    fn suspend(&self) -> Result<()>;
    fn hibernate(&self) -> Result<()>;
    fn hybrid_sleep(&self) -> Result<()>;
    fn get_daemon_status(&self) -> Result<DaemonStatus>;
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
        tracing::info!("Slumber IPC listening on {}", self.socket_path);

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
        IpcRequest::GetPowerStatus => match handler.get_power_status() {
            Ok(status) => IpcResponse::Success {
                data: serde_json::to_value(status).unwrap(),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::GetProfile => IpcResponse::Success {
            data: serde_json::to_value(handler.get_profile()).unwrap(),
        },

        IpcRequest::SetProfile { name } => match handler.set_profile(&name) {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"profile": name}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::ListProfiles => IpcResponse::Success {
            data: serde_json::json!({"profiles": handler.list_profiles()}),
        },

        IpcRequest::GetSleepStatus => IpcResponse::Success {
            data: serde_json::to_value(handler.get_sleep_status()).unwrap(),
        },

        IpcRequest::Suspend => match handler.suspend() {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"action": "suspended"}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::Hibernate => match handler.hibernate() {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"action": "hibernated"}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::HybridSleep => match handler.hybrid_sleep() {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"action": "hybrid_sleep"}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::GetStatus => match handler.get_daemon_status() {
            Ok(status) => IpcResponse::Success {
                data: serde_json::to_value(status).unwrap(),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },
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

    pub async fn get_power_status(&self) -> Result<PowerStatus> {
        match self.send(IpcRequest::GetPowerStatus).await? {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn get_profile(&self) -> Result<ProfileStatus> {
        match self.send(IpcRequest::GetProfile).await? {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn set_profile(&self, name: &str) -> Result<()> {
        match self.send(IpcRequest::SetProfile { name: name.to_string() }).await? {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn suspend(&self) -> Result<()> {
        match self.send(IpcRequest::Suspend).await? {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn hibernate(&self) -> Result<()> {
        match self.send(IpcRequest::Hibernate).await? {
            IpcResponse::Success { .. } => Ok(()),
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
