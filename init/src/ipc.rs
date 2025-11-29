//! IPC integration for init system

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info};

/// IPC message types for init control
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InitMessage {
    /// Start a service
    StartService { name: String },

    /// Stop a service
    StopService { name: String },

    /// Restart a service
    RestartService { name: String },

    /// Get service status
    GetStatus { name: Option<String> },

    /// Reload configuration
    ReloadConfig,

    /// Shutdown system
    Shutdown { reboot: bool },

    /// List all services
    ListServices,

    /// Response message
    Response(InitResponse),
}

/// Response from init daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitResponse {
    /// Success status
    pub success: bool,
    /// Optional message
    pub message: Option<String>,
    /// Optional data
    pub data: Option<serde_json::Value>,
}

impl InitResponse {
    pub fn ok() -> Self {
        Self {
            success: true,
            message: None,
            data: None,
        }
    }

    pub fn ok_with_message(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: Some(message.into()),
            data: None,
        }
    }

    pub fn ok_with_data(data: serde_json::Value) -> Self {
        Self {
            success: true,
            message: None,
            data: Some(data),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: Some(message.into()),
            data: None,
        }
    }
}

/// IPC server for init daemon
pub struct InitIpcServer {
    socket_path: String,
}

impl InitIpcServer {
    /// Create a new IPC server
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    /// Start listening for connections
    #[cfg(unix)]
    pub async fn listen<F, Fut>(&self, handler: F) -> Result<()>
    where
        F: Fn(InitMessage) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = InitResponse> + Send,
    {
        use tokio::net::UnixListener;

        // Remove old socket if it exists
        let _ = std::fs::remove_file(&self.socket_path);

        // Create parent directory if needed
        if let Some(parent) = Path::new(&self.socket_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        info!("Init IPC server listening on {}", self.socket_path);

        loop {
            let (mut socket, _) = listener.accept().await?;
            let handler = handler.clone();

            tokio::spawn(async move {
                let mut buf = vec![0u8; 65536];

                loop {
                    match socket.read(&mut buf).await {
                        Ok(0) => break, // Connection closed
                        Ok(n) => {
                            // Parse message
                            match serde_json::from_slice::<InitMessage>(&buf[..n]) {
                                Ok(msg) => {
                                    debug!("Received IPC message: {:?}", msg);
                                    let response = handler(msg).await;
                                    let response_bytes = serde_json::to_vec(&response).unwrap();
                                    let _ = socket.write_all(&response_bytes).await;
                                }
                                Err(e) => {
                                    let response = InitResponse::error(format!(
                                        "Invalid message: {}",
                                        e
                                    ));
                                    let response_bytes = serde_json::to_vec(&response).unwrap();
                                    let _ = socket.write_all(&response_bytes).await;
                                }
                            }
                        }
                        Err(e) => {
                            debug!("IPC read error: {}", e);
                            break;
                        }
                    }
                }
            });
        }
    }

    #[cfg(not(unix))]
    pub async fn listen<F, Fut>(&self, _handler: F) -> Result<()>
    where
        F: Fn(InitMessage) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = InitResponse> + Send,
    {
        Err(anyhow::anyhow!("Unix sockets not supported on this platform"))
    }
}

/// IPC client for controlling init daemon
pub struct InitIpcClient {
    socket_path: String,
}

impl InitIpcClient {
    /// Create a new IPC client
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    /// Send a message and get response
    #[cfg(unix)]
    pub async fn send(&self, msg: InitMessage) -> Result<InitResponse> {
        use tokio::net::UnixStream;

        let mut socket = UnixStream::connect(&self.socket_path).await?;

        // Send message
        let msg_bytes = serde_json::to_vec(&msg)?;
        socket.write_all(&msg_bytes).await?;

        // Read response
        let mut buf = vec![0u8; 65536];
        let n = socket.read(&mut buf).await?;

        let response: InitResponse = serde_json::from_slice(&buf[..n])?;
        Ok(response)
    }

    #[cfg(not(unix))]
    pub async fn send(&self, _msg: InitMessage) -> Result<InitResponse> {
        Err(anyhow::anyhow!("Unix sockets not supported on this platform"))
    }

    // Convenience methods

    pub async fn start_service(&self, name: &str) -> Result<InitResponse> {
        self.send(InitMessage::StartService {
            name: name.to_string(),
        })
        .await
    }

    pub async fn stop_service(&self, name: &str) -> Result<InitResponse> {
        self.send(InitMessage::StopService {
            name: name.to_string(),
        })
        .await
    }

    pub async fn restart_service(&self, name: &str) -> Result<InitResponse> {
        self.send(InitMessage::RestartService {
            name: name.to_string(),
        })
        .await
    }

    pub async fn get_status(&self, name: Option<&str>) -> Result<InitResponse> {
        self.send(InitMessage::GetStatus {
            name: name.map(String::from),
        })
        .await
    }

    pub async fn list_services(&self) -> Result<InitResponse> {
        self.send(InitMessage::ListServices).await
    }

    pub async fn shutdown(&self, reboot: bool) -> Result<InitResponse> {
        self.send(InitMessage::Shutdown { reboot }).await
    }
}
