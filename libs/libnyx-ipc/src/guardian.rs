//! Guardian IPC client
//!
//! Client for communicating with the Guardian security agent.

use crate::protocol::{CapabilityDecision, CapabilityRequest, Decision};
use crate::{paths, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tracing::{debug, warn};
use uuid::Uuid;

/// Guardian client
pub struct GuardianClient {
    socket_path: PathBuf,
    stream: Option<UnixStream>,
}

impl GuardianClient {
    /// Create a new client with default socket path
    pub fn new() -> Self {
        Self {
            socket_path: PathBuf::from(paths::GUARDIAN_SOCKET),
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

    /// Connect to Guardian using default socket
    pub async fn connect() -> Result<Self> {
        let mut client = Self::new();
        client.connect_internal().await?;
        Ok(client)
    }

    /// Connect to Guardian
    pub async fn connect_internal(&mut self) -> Result<()> {
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

    /// Check a capability request
    pub async fn check_capability(
        &mut self,
        capability: impl Into<String>,
        resource: Option<&str>,
    ) -> Result<CapabilityDecision> {
        let mut request = CapabilityRequest::new(capability);
        if let Some(res) = resource {
            request = request.with_resource(res);
        }

        self.check_capability_full(request).await
    }

    /// Check a full capability request
    pub async fn check_capability_full(
        &mut self,
        request: CapabilityRequest,
    ) -> Result<CapabilityDecision> {
        let request_id = Uuid::new_v4();

        let message = GuardianRequest::CheckCapability {
            request_id,
            request,
        };

        let response: GuardianResponse = self.send_request(&message).await?;

        match response {
            GuardianResponse::Decision {
                decision,
                reason,
                sandbox_config,
                recommended_action,
                ..
            } => {
                let decision = match decision.as_str() {
                    "allow" => Decision::Allow,
                    "deny" => Decision::Deny,
                    s if s.starts_with("sandbox") => Decision::Sandbox,
                    "prompt" => Decision::Prompt,
                    _ => Decision::Deny,
                };

                Ok(CapabilityDecision {
                    decision,
                    reason,
                    sandbox_config,
                    recommended_action,
                })
            }
            GuardianResponse::PromptRequired { request_id, message, .. } => {
                // Return prompt decision - caller should handle user interaction
                Ok(CapabilityDecision {
                    decision: Decision::Prompt,
                    reason: message,
                    sandbox_config: None,
                    recommended_action: Some("User confirmation required".into()),
                })
            }
            GuardianResponse::Error { code: _, message } => {
                Err(Error::RequestFailed(message))
            }
            _ => Err(Error::ProtocolError("Unexpected response type".into())),
        }
    }

    /// Respond to a prompt
    pub async fn respond_to_prompt(
        &mut self,
        request_id: Uuid,
        approved: bool,
        remember: bool,
    ) -> Result<CapabilityDecision> {
        let message = GuardianRequest::UserResponse {
            request_id,
            approved,
            remember,
        };

        let response: GuardianResponse = self.send_request(&message).await?;

        match response {
            GuardianResponse::Decision {
                decision,
                reason,
                sandbox_config,
                recommended_action,
                ..
            } => {
                let decision = match decision.as_str() {
                    "allow" => Decision::Allow,
                    "deny" => Decision::Deny,
                    _ => Decision::Deny,
                };

                Ok(CapabilityDecision {
                    decision,
                    reason,
                    sandbox_config,
                    recommended_action,
                })
            }
            GuardianResponse::Error { message, .. } => Err(Error::RequestFailed(message)),
            _ => Err(Error::ProtocolError("Unexpected response type".into())),
        }
    }

    /// Get Guardian status
    pub async fn status(&mut self) -> Result<GuardianStatus> {
        let response: GuardianResponse = self.send_request(&GuardianRequest::Status).await?;

        match response {
            GuardianResponse::Status {
                version,
                uptime_secs,
                requests_processed,
                active_processes,
            } => Ok(GuardianStatus {
                version,
                uptime_secs,
                requests_processed,
                active_processes,
            }),
            GuardianResponse::Error { message, .. } => Err(Error::RequestFailed(message)),
            _ => Err(Error::ProtocolError("Unexpected response type".into())),
        }
    }

    /// Request a sandbox profile
    pub async fn get_sandbox_profile(&mut self, level: &str) -> Result<serde_json::Value> {
        let message = GuardianRequest::GetSandboxProfile {
            level: level.into(),
        };

        let response: GuardianResponse = self.send_request(&message).await?;

        match response {
            GuardianResponse::SandboxProfile { config } => Ok(config),
            GuardianResponse::Error { message, .. } => Err(Error::RequestFailed(message)),
            _ => Err(Error::ProtocolError("Unexpected response type".into())),
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

impl Default for GuardianClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Guardian request types (mirroring guardian::ipc)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum GuardianRequest {
    CheckCapability {
        request_id: Uuid,
        request: CapabilityRequest,
    },
    UserResponse {
        request_id: Uuid,
        approved: bool,
        remember: bool,
    },
    Status,
    QueryPolicy {
        process_path: String,
        capability: String,
    },
    RegisterProcess {
        pid: u32,
        path: String,
        user: String,
    },
    UnregisterProcess {
        pid: u32,
    },
    GetSandboxProfile {
        level: String,
    },
    ReloadConfig,
    Shutdown,
}

/// Guardian response types (mirroring guardian::ipc)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum GuardianResponse {
    Decision {
        request_id: Uuid,
        decision: String,
        reason: String,
        sandbox_config: Option<serde_json::Value>,
        recommended_action: Option<String>,
    },
    PromptRequired {
        request_id: Uuid,
        message: String,
        details: serde_json::Value,
    },
    Status {
        version: String,
        uptime_secs: u64,
        requests_processed: u64,
        active_processes: u32,
    },
    PolicyResult {
        decision: String,
        applies_to: Vec<String>,
    },
    SandboxProfile {
        config: serde_json::Value,
    },
    Ok {
        message: String,
    },
    Error {
        code: String,
        message: String,
    },
}

/// Guardian status information
#[derive(Debug, Clone)]
pub struct GuardianStatus {
    pub version: String,
    pub uptime_secs: u64,
    pub requests_processed: u64,
    pub active_processes: u32,
}

/// Convenience function to check a capability
pub async fn check_capability(
    capability: impl Into<String>,
    resource: Option<&str>,
) -> Result<CapabilityDecision> {
    let mut client = GuardianClient::connect().await?;
    client.check_capability(capability, resource).await
}

/// Convenience function to check if a capability is allowed
pub async fn is_allowed(capability: impl Into<String>, resource: Option<&str>) -> bool {
    match check_capability(capability, resource).await {
        Ok(decision) => decision.decision.is_allowed(),
        Err(e) => {
            warn!("Capability check failed: {}", e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_request() {
        let req = CapabilityRequest::new("filesystem:read")
            .with_resource("/etc/passwd")
            .with_context("reason", "testing");

        assert_eq!(req.capability, "filesystem:read");
        assert_eq!(req.resource, Some("/etc/passwd".into()));
        assert_eq!(req.context.get("reason"), Some(&"testing".into()));
    }
}
