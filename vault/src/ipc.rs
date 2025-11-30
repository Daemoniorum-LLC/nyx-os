//! IPC interface for Vault

use crate::store::{SecretMetadata, SecretType, VaultStats};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// IPC request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcRequest {
    /// Check if vault exists
    Exists,

    /// Initialize new vault
    Initialize { password: String },

    /// Unlock vault
    Unlock { password: String },

    /// Lock vault
    Lock,

    /// Check if unlocked
    IsUnlocked,

    /// Set a secret
    Set {
        name: String,
        value: String,
        #[serde(default)]
        secret_type: Option<SecretType>,
    },

    /// Get a secret
    Get { name: String },

    /// Delete a secret
    Delete { name: String },

    /// List secrets
    List,

    /// Search by tag
    SearchByTag { tag: String },

    /// Add tag to secret
    AddTag { name: String, tag: String },

    /// Set notes for secret
    SetNotes { name: String, notes: Option<String> },

    /// Change master password
    ChangePassword {
        old_password: String,
        new_password: String,
    },

    /// Create backup
    Backup,

    /// Generate password
    GeneratePassword { length: Option<usize> },

    /// Get vault stats
    Stats,

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
    pub vault_exists: bool,
    pub unlocked: bool,
    pub stats: Option<VaultStats>,
}

/// IPC handler trait
pub trait IpcHandler: Send + Sync {
    fn exists(&self) -> bool;
    fn initialize(&self, password: &str) -> Result<()>;
    fn unlock(&self, password: &str) -> Result<()>;
    fn lock(&self);
    fn is_unlocked(&self) -> bool;
    fn set(&self, name: &str, value: &str, secret_type: SecretType) -> Result<()>;
    fn get(&self, name: &str) -> Result<String>;
    fn delete(&self, name: &str) -> Result<()>;
    fn list(&self) -> Result<Vec<SecretMetadata>>;
    fn search_by_tag(&self, tag: &str) -> Result<Vec<SecretMetadata>>;
    fn add_tag(&self, name: &str, tag: &str) -> Result<()>;
    fn set_notes(&self, name: &str, notes: Option<String>) -> Result<()>;
    fn change_password(&self, old: &str, new: &str) -> Result<()>;
    fn backup(&self) -> Result<String>;
    fn generate_password(&self, length: usize) -> Result<String>;
    fn stats(&self) -> Result<VaultStats>;
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

        // Set restrictive permissions on socket
        let listener = UnixListener::bind(&self.socket_path)?;
        tracing::info!("Vault IPC listening on {}", self.socket_path);

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
        IpcRequest::Exists => IpcResponse::Success {
            data: serde_json::json!({"exists": handler.exists()}),
        },

        IpcRequest::Initialize { password } => match handler.initialize(&password) {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"initialized": true}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::Unlock { password } => match handler.unlock(&password) {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"unlocked": true}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::Lock => {
            handler.lock();
            IpcResponse::Success {
                data: serde_json::json!({"locked": true}),
            }
        }

        IpcRequest::IsUnlocked => IpcResponse::Success {
            data: serde_json::json!({"unlocked": handler.is_unlocked()}),
        },

        IpcRequest::Set {
            name,
            value,
            secret_type,
        } => {
            match handler.set(&name, &value, secret_type.unwrap_or(SecretType::Generic)) {
                Ok(()) => IpcResponse::Success {
                    data: serde_json::json!({"saved": name}),
                },
                Err(e) => IpcResponse::Error {
                    message: e.to_string(),
                },
            }
        }

        IpcRequest::Get { name } => match handler.get(&name) {
            Ok(value) => IpcResponse::Success {
                data: serde_json::json!({"name": name, "value": value}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::Delete { name } => match handler.delete(&name) {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"deleted": name}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::List => match handler.list() {
            Ok(secrets) => IpcResponse::Success {
                data: serde_json::to_value(secrets).unwrap(),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::SearchByTag { tag } => match handler.search_by_tag(&tag) {
            Ok(secrets) => IpcResponse::Success {
                data: serde_json::to_value(secrets).unwrap(),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::AddTag { name, tag } => match handler.add_tag(&name, &tag) {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"name": name, "tag": tag}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::SetNotes { name, notes } => match handler.set_notes(&name, notes) {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"name": name}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::ChangePassword {
            old_password,
            new_password,
        } => match handler.change_password(&old_password, &new_password) {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"changed": true}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::Backup => match handler.backup() {
            Ok(path) => IpcResponse::Success {
                data: serde_json::json!({"backup_path": path}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::GeneratePassword { length } => {
            match handler.generate_password(length.unwrap_or(20)) {
                Ok(password) => IpcResponse::Success {
                    data: serde_json::json!({"password": password}),
                },
                Err(e) => IpcResponse::Error {
                    message: e.to_string(),
                },
            }
        }

        IpcRequest::Stats => match handler.stats() {
            Ok(stats) => IpcResponse::Success {
                data: serde_json::to_value(stats).unwrap(),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

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

    pub async fn unlock(&self, password: &str) -> Result<()> {
        match self
            .send(IpcRequest::Unlock {
                password: password.to_string(),
            })
            .await?
        {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn get(&self, name: &str) -> Result<String> {
        match self
            .send(IpcRequest::Get {
                name: name.to_string(),
            })
            .await?
        {
            IpcResponse::Success { data } => {
                Ok(data["value"].as_str().unwrap_or("").to_string())
            }
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn set(&self, name: &str, value: &str) -> Result<()> {
        match self
            .send(IpcRequest::Set {
                name: name.to_string(),
                value: value.to_string(),
                secret_type: None,
            })
            .await?
        {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn list(&self) -> Result<Vec<SecretMetadata>> {
        match self.send(IpcRequest::List).await? {
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
