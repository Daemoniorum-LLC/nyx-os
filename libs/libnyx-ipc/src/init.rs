//! Init IPC client
//!
//! Client for communicating with nyx-init service manager.

use crate::protocol::{ServiceRegistration, ServiceState, ServiceStatus, ServiceType};
use crate::{paths, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tracing::debug;
use uuid::Uuid;

/// Init client
pub struct InitClient {
    socket_path: PathBuf,
    stream: Option<UnixStream>,
}

impl InitClient {
    /// Create a new client with default socket path
    pub fn new() -> Self {
        Self {
            socket_path: PathBuf::from(paths::INIT_SOCKET),
            stream: None,
        }
    }

    /// Create a client with custom socket path
    pub fn with_socket(path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: path.into(),
            stream: None,
        }
    }

    /// Connect to init using default socket
    pub async fn connect() -> Result<Self> {
        let mut client = Self::new();
        client.connect_internal().await?;
        Ok(client)
    }

    /// Connect to init
    async fn connect_internal(&mut self) -> Result<()> {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Error::ServiceUnavailable
                } else {
                    Error::ConnectionFailed(e.to_string())
                }
            })?;

        self.stream = Some(stream);
        Ok(())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// Register a service with init
    pub async fn register_service(
        &mut self,
        name: impl Into<String>,
        pid: u32,
        service_type: ServiceType,
        capabilities: Vec<String>,
    ) -> Result<()> {
        let registration = ServiceRegistration {
            name: name.into(),
            pid,
            service_type,
            capabilities,
            health_check: None,
        };

        let message = InitRequest::RegisterService { registration };
        let response: InitResponse = self.send_request(&message).await?;

        match response {
            InitResponse::Ok { .. } => Ok(()),
            InitResponse::Error { message } => Err(Error::RequestFailed(message)),
            _ => Err(Error::ProtocolError("Unexpected response".into())),
        }
    }

    /// Unregister a service
    pub async fn unregister_service(&mut self, name: impl Into<String>) -> Result<()> {
        let message = InitRequest::UnregisterService { name: name.into() };
        let response: InitResponse = self.send_request(&message).await?;

        match response {
            InitResponse::Ok { .. } => Ok(()),
            InitResponse::Error { message } => Err(Error::RequestFailed(message)),
            _ => Err(Error::ProtocolError("Unexpected response".into())),
        }
    }

    /// Get status of a service
    pub async fn service_status(&mut self, name: impl Into<String>) -> Result<ServiceStatus> {
        let message = InitRequest::ServiceStatus { name: name.into() };
        let response: InitResponse = self.send_request(&message).await?;

        match response {
            InitResponse::ServiceStatus { status } => Ok(status),
            InitResponse::Error { message } => Err(Error::RequestFailed(message)),
            _ => Err(Error::ProtocolError("Unexpected response".into())),
        }
    }

    /// List all services
    pub async fn list_services(&mut self) -> Result<Vec<ServiceStatus>> {
        let message = InitRequest::ListServices;
        let response: InitResponse = self.send_request(&message).await?;

        match response {
            InitResponse::ServiceList { services } => Ok(services),
            InitResponse::Error { message } => Err(Error::RequestFailed(message)),
            _ => Err(Error::ProtocolError("Unexpected response".into())),
        }
    }

    /// Start a service
    pub async fn start_service(&mut self, name: impl Into<String>) -> Result<()> {
        let message = InitRequest::StartService { name: name.into() };
        let response: InitResponse = self.send_request(&message).await?;

        match response {
            InitResponse::Ok { .. } => Ok(()),
            InitResponse::Error { message } => Err(Error::RequestFailed(message)),
            _ => Err(Error::ProtocolError("Unexpected response".into())),
        }
    }

    /// Stop a service
    pub async fn stop_service(&mut self, name: impl Into<String>) -> Result<()> {
        let message = InitRequest::StopService { name: name.into() };
        let response: InitResponse = self.send_request(&message).await?;

        match response {
            InitResponse::Ok { .. } => Ok(()),
            InitResponse::Error { message } => Err(Error::RequestFailed(message)),
            _ => Err(Error::ProtocolError("Unexpected response".into())),
        }
    }

    /// Restart a service
    pub async fn restart_service(&mut self, name: impl Into<String>) -> Result<()> {
        let message = InitRequest::RestartService { name: name.into() };
        let response: InitResponse = self.send_request(&message).await?;

        match response {
            InitResponse::Ok { .. } => Ok(()),
            InitResponse::Error { message } => Err(Error::RequestFailed(message)),
            _ => Err(Error::ProtocolError("Unexpected response".into())),
        }
    }

    /// Report ready state
    pub async fn notify_ready(&mut self, name: impl Into<String>) -> Result<()> {
        let message = InitRequest::NotifyReady { name: name.into() };
        let response: InitResponse = self.send_request(&message).await?;

        match response {
            InitResponse::Ok { .. } => Ok(()),
            InitResponse::Error { message } => Err(Error::RequestFailed(message)),
            _ => Err(Error::ProtocolError("Unexpected response".into())),
        }
    }

    /// Report health status
    pub async fn notify_health(
        &mut self,
        name: impl Into<String>,
        healthy: bool,
        message: Option<String>,
    ) -> Result<()> {
        let request = InitRequest::NotifyHealth {
            name: name.into(),
            healthy,
            message,
        };
        let response: InitResponse = self.send_request(&request).await?;

        match response {
            InitResponse::Ok { .. } => Ok(()),
            InitResponse::Error { message } => Err(Error::RequestFailed(message)),
            _ => Err(Error::ProtocolError("Unexpected response".into())),
        }
    }

    /// Request shutdown
    pub async fn request_shutdown(&mut self, reason: impl Into<String>) -> Result<()> {
        let message = InitRequest::Shutdown {
            reason: reason.into(),
        };
        let response: InitResponse = self.send_request(&message).await?;

        match response {
            InitResponse::Ok { .. } => Ok(()),
            InitResponse::Error { message } => Err(Error::RequestFailed(message)),
            _ => Err(Error::ProtocolError("Unexpected response".into())),
        }
    }

    /// Get init status
    pub async fn status(&mut self) -> Result<InitStatus> {
        let message = InitRequest::Status;
        let response: InitResponse = self.send_request(&message).await?;

        match response {
            InitResponse::Status {
                version,
                uptime_secs,
                services_running,
                services_total,
            } => Ok(InitStatus {
                version,
                uptime_secs,
                services_running,
                services_total,
            }),
            InitResponse::Error { message } => Err(Error::RequestFailed(message)),
            _ => Err(Error::ProtocolError("Unexpected response".into())),
        }
    }

    async fn send_request<R: for<'de> Deserialize<'de>>(
        &mut self,
        request: &impl Serialize,
    ) -> Result<R> {
        // Ensure connected
        if self.stream.is_none() {
            self.connect_internal().await?;
        }

        let stream = self.stream.as_mut().ok_or(Error::ServiceUnavailable)?;

        // Serialize and send
        let json = serde_json::to_string(request)
            .map_err(|e| Error::ProtocolError(e.to_string()))?;
        let message = json + "\n";

        stream
            .write_all(message.as_bytes())
            .await
            .map_err(|e| Error::Io(e))?;

        // Read response
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .map_err(|e| Error::Io(e))?;

        // Deserialize response
        let response: R = serde_json::from_str(&line)
            .map_err(|e| Error::ProtocolError(e.to_string()))?;

        Ok(response)
    }
}

impl Default for InitClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Init request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum InitRequest {
    RegisterService {
        registration: ServiceRegistration,
    },
    UnregisterService {
        name: String,
    },
    ServiceStatus {
        name: String,
    },
    ListServices,
    StartService {
        name: String,
    },
    StopService {
        name: String,
    },
    RestartService {
        name: String,
    },
    NotifyReady {
        name: String,
    },
    NotifyHealth {
        name: String,
        healthy: bool,
        message: Option<String>,
    },
    Shutdown {
        reason: String,
    },
    Status,
}

/// Init response types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum InitResponse {
    Ok {
        message: String,
    },
    Error {
        message: String,
    },
    ServiceStatus {
        status: ServiceStatus,
    },
    ServiceList {
        services: Vec<ServiceStatus>,
    },
    Status {
        version: String,
        uptime_secs: u64,
        services_running: u32,
        services_total: u32,
    },
}

/// Init status information
#[derive(Debug, Clone)]
pub struct InitStatus {
    pub version: String,
    pub uptime_secs: u64,
    pub services_running: u32,
    pub services_total: u32,
}

/// Convenience function to register current process as a service
pub async fn register_self(
    name: impl Into<String>,
    service_type: ServiceType,
    capabilities: Vec<String>,
) -> Result<()> {
    let mut client = InitClient::connect().await?;
    let pid = std::process::id();
    client.register_service(name, pid, service_type, capabilities).await
}

/// Convenience function to notify ready
pub async fn notify_ready(name: impl Into<String>) -> Result<()> {
    let mut client = InitClient::connect().await?;
    client.notify_ready(name).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_registration() {
        let reg = ServiceRegistration {
            name: "test-service".into(),
            pid: 1234,
            service_type: ServiceType::Agent,
            capabilities: vec!["network:client".into()],
            health_check: Some("http://localhost:8080/health".into()),
        };

        let json = serde_json::to_string(&reg).unwrap();
        assert!(json.contains("test-service"));
        assert!(json.contains("agent"));
    }
}
