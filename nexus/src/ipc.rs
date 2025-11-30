//! IPC interface for Nexus daemon

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{info, error, debug};

use crate::package::PackageSpec;

/// IPC request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    Install {
        specs: Vec<PackageSpec>,
        dry_run: bool,
    },
    Remove {
        packages: Vec<String>,
        autoremove: bool,
    },
    Upgrade {
        packages: Vec<String>,
    },
    Sync,
    Rollback {
        generation: Option<u32>,
    },
    Status,
}

/// IPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success {
        message: String,
    },
    Plan {
        install: Vec<String>,
        remove: Vec<String>,
        download_size: u64,
        install_size: u64,
    },
    Status {
        installed_count: usize,
        current_generation: u32,
        cache_size: u64,
    },
    Error {
        message: String,
    },
}

// Custom serialization for PackageSpec
impl Serialize for PackageSpec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.name)
    }
}

impl<'de> Deserialize<'de> for PackageSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// IPC server
pub struct NexusServer<S> {
    socket_path: PathBuf,
    state: Arc<RwLock<S>>,
}

impl<S: Send + Sync + 'static> NexusServer<S> {
    pub fn new(socket_path: &str, state: Arc<RwLock<S>>) -> Self {
        Self {
            socket_path: PathBuf::from(socket_path),
            state,
        }
    }

    pub async fn run<F, Fut>(&self, handler: F) -> Result<()>
    where
        F: Fn(IpcRequest, Arc<RwLock<S>>) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = IpcResponse> + Send,
    {
        // Remove existing socket
        let _ = std::fs::remove_file(&self.socket_path);

        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;

        // Set socket permissions
        std::fs::set_permissions(
            &self.socket_path,
            std::os::unix::fs::PermissionsExt::from_mode(0o660),
        )?;

        info!("Nexus IPC listening on {:?}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let state = self.state.clone();
                    let handler = handler.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, state, handler).await {
                            error!("Client error: {}", e);
                        }
                    });
                }
                Err(e) => error!("Accept error: {}", e),
            }
        }
    }
}

async fn handle_client<S, F, Fut>(
    stream: UnixStream,
    state: Arc<RwLock<S>>,
    handler: F,
) -> Result<()>
where
    F: Fn(IpcRequest, Arc<RwLock<S>>) -> Fut,
    Fut: std::future::Future<Output = IpcResponse>,
{
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => {
                debug!("Request: {:?}", request);
                handler(request, state.clone()).await
            }
            Err(e) => IpcResponse::Error {
                message: format!("Invalid request: {}", e),
            },
        };

        let json = serde_json::to_string(&response)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        line.clear();
    }

    Ok(())
}

/// IPC client
pub struct NexusClient {
    socket_path: PathBuf,
}

impl NexusClient {
    pub async fn connect() -> Result<Self> {
        let socket_path = PathBuf::from("/run/nexus/nexus.sock");

        // Test connection
        let _ = UnixStream::connect(&socket_path).await?;

        Ok(Self { socket_path })
    }

    async fn send(&self, request: IpcRequest) -> Result<IpcResponse> {
        let mut stream = UnixStream::connect(&self.socket_path).await?;

        let json = serde_json::to_string(&request)?;
        stream.write_all(json.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        Ok(serde_json::from_str(&line)?)
    }

    pub async fn install(&self, specs: &[PackageSpec], dry_run: bool) -> Result<()> {
        let response = self.send(IpcRequest::Install {
            specs: specs.to_vec(),
            dry_run,
        }).await?;

        match response {
            IpcResponse::Success { message } => {
                println!("{}", message);
                Ok(())
            }
            IpcResponse::Plan { install, remove, download_size, install_size } => {
                println!("Packages to install:");
                for pkg in install {
                    println!("  {}", pkg);
                }
                println!("Download: {} bytes", download_size);
                println!("Install:  {} bytes", install_size);
                Ok(())
            }
            IpcResponse::Error { message } => {
                Err(anyhow::anyhow!(message))
            }
            _ => Ok(()),
        }
    }

    pub async fn remove(&self, packages: &[String], autoremove: bool) -> Result<()> {
        let response = self.send(IpcRequest::Remove {
            packages: packages.to_vec(),
            autoremove,
        }).await?;

        match response {
            IpcResponse::Success { message } => {
                println!("{}", message);
                Ok(())
            }
            IpcResponse::Error { message } => {
                Err(anyhow::anyhow!(message))
            }
            _ => Ok(()),
        }
    }

    pub async fn upgrade(&self, packages: &[String]) -> Result<()> {
        let response = self.send(IpcRequest::Upgrade {
            packages: packages.to_vec(),
        }).await?;

        match response {
            IpcResponse::Success { message } => {
                println!("{}", message);
                Ok(())
            }
            IpcResponse::Error { message } => {
                Err(anyhow::anyhow!(message))
            }
            _ => Ok(()),
        }
    }

    pub async fn sync(&self) -> Result<()> {
        let response = self.send(IpcRequest::Sync).await?;

        match response {
            IpcResponse::Success { message } => {
                println!("{}", message);
                Ok(())
            }
            IpcResponse::Error { message } => {
                Err(anyhow::anyhow!(message))
            }
            _ => Ok(()),
        }
    }

    pub async fn rollback(&self, generation: Option<u32>) -> Result<()> {
        let response = self.send(IpcRequest::Rollback { generation }).await?;

        match response {
            IpcResponse::Success { message } => {
                println!("{}", message);
                Ok(())
            }
            IpcResponse::Error { message } => {
                Err(anyhow::anyhow!(message))
            }
            _ => Ok(()),
        }
    }
}
