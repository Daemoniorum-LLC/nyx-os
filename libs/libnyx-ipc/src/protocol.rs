//! Common protocol types for Nyx IPC

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Generic IPC message wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message ID for correlation
    pub id: Uuid,
    /// Message type
    #[serde(rename = "type")]
    pub msg_type: String,
    /// Message payload
    pub payload: serde_json::Value,
}

impl Message {
    /// Create a new message
    pub fn new(msg_type: impl Into<String>, payload: impl Serialize) -> Self {
        Self {
            id: Uuid::new_v4(),
            msg_type: msg_type.into(),
            payload: serde_json::to_value(payload).unwrap_or(serde_json::Value::Null),
        }
    }

    /// Get the message ID
    pub fn id(&self) -> Uuid {
        self.id
    }
}

/// Generic response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// Request ID this responds to
    pub request_id: Uuid,
    /// Whether the request succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Response payload
    pub data: serde_json::Value,
}

impl Response {
    /// Create a success response
    pub fn success(request_id: Uuid, data: impl Serialize) -> Self {
        Self {
            request_id,
            success: true,
            error: None,
            data: serde_json::to_value(data).unwrap_or(serde_json::Value::Null),
        }
    }

    /// Create an error response
    pub fn error(request_id: Uuid, message: impl Into<String>) -> Self {
        Self {
            request_id,
            success: false,
            error: Some(message.into()),
            data: serde_json::Value::Null,
        }
    }

    /// Check if the response is successful
    pub fn is_ok(&self) -> bool {
        self.success
    }

    /// Get the error message
    pub fn error_message(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Capability request for Guardian
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRequest {
    /// Process ID making the request
    pub pid: u32,
    /// Process executable path
    pub process_path: String,
    /// User running the process
    pub user: String,
    /// Capability being requested
    pub capability: String,
    /// Resource being accessed (optional)
    pub resource: Option<String>,
    /// Additional context
    #[serde(default)]
    pub context: HashMap<String, String>,
}

impl CapabilityRequest {
    /// Create a new capability request
    pub fn new(capability: impl Into<String>) -> Self {
        let pid = std::process::id();
        let process_path = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".into());
        let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());

        Self {
            pid,
            process_path,
            user,
            capability: capability.into(),
            resource: None,
            context: HashMap::new(),
        }
    }

    /// Set the resource being accessed
    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into());
        self
    }

    /// Add context
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}

/// Capability decision from Guardian
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDecision {
    /// The decision
    pub decision: Decision,
    /// Reason for the decision
    pub reason: String,
    /// Sandbox configuration if sandboxed
    pub sandbox_config: Option<serde_json::Value>,
    /// Recommended action
    pub recommended_action: Option<String>,
}

/// Decision types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// Request allowed
    Allow,
    /// Request denied
    Deny,
    /// Request allowed but sandboxed
    Sandbox,
    /// Need user confirmation (should wait)
    Prompt,
}

impl Decision {
    /// Check if the decision allows the operation
    pub fn is_allowed(&self) -> bool {
        matches!(self, Decision::Allow | Decision::Sandbox)
    }
}

/// Service registration for init
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRegistration {
    /// Service name
    pub name: String,
    /// Process ID
    pub pid: u32,
    /// Service type
    pub service_type: ServiceType,
    /// Capabilities needed
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Health check endpoint (optional)
    pub health_check: Option<String>,
}

/// Service types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ServiceType {
    /// Simple foreground process
    #[default]
    Simple,
    /// Daemon (forks)
    Daemon,
    /// AI agent
    Agent,
    /// One-shot task
    Oneshot,
}

/// Service status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    /// Service name
    pub name: String,
    /// Current state
    pub state: ServiceState,
    /// Process ID (if running)
    pub pid: Option<u32>,
    /// Uptime in seconds
    pub uptime_secs: Option<u64>,
    /// Last health check result
    pub health: Option<HealthStatus>,
}

/// Service states
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    /// Not started
    Stopped,
    /// Starting up
    Starting,
    /// Running
    Running,
    /// Stopping
    Stopping,
    /// Failed
    Failed,
    /// Restarting
    Restarting,
}

/// Health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Is healthy
    pub healthy: bool,
    /// Last check timestamp
    pub last_check: u64,
    /// Message
    pub message: Option<String>,
}
