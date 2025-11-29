//! IPC server for Guardian
//!
//! Guardian listens for capability requests from the kernel and other processes
//! via a Unix socket. This provides the interface for the security decision flow.

use crate::audit::AuditLogger;
use crate::decision::{DecisionEngine, FinalDecision, SecurityDecision};
use crate::policy::CapabilityRequest;
use crate::sandbox::{SandboxConfig, SandboxEnforcer};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Guardian IPC protocol messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GuardianRequest {
    /// Request capability check
    CheckCapability {
        request_id: Uuid,
        request: CapabilityRequest,
    },
    /// Report user response to prompt
    UserResponse {
        request_id: Uuid,
        approved: bool,
        remember: bool,
    },
    /// Query current status
    Status,
    /// Query policy for a path
    QueryPolicy {
        process_path: String,
        capability: String,
    },
    /// Register a process
    RegisterProcess {
        pid: u32,
        path: String,
        user: String,
    },
    /// Unregister a process
    UnregisterProcess {
        pid: u32,
    },
    /// Request sandbox profile
    GetSandboxProfile {
        level: String,
    },
    /// Reload configuration
    ReloadConfig,
    /// Shutdown Guardian
    Shutdown,
}

/// Guardian IPC protocol responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GuardianResponse {
    /// Capability decision
    Decision {
        request_id: Uuid,
        decision: String,
        reason: String,
        sandbox_config: Option<SandboxConfig>,
        recommended_action: Option<String>,
    },
    /// Prompt needed - waiting for user
    PromptRequired {
        request_id: Uuid,
        message: String,
        details: PromptDetails,
    },
    /// Status response
    Status {
        version: String,
        uptime_secs: u64,
        requests_processed: u64,
        active_processes: u32,
    },
    /// Policy query response
    PolicyResult {
        decision: String,
        applies_to: Vec<String>,
    },
    /// Sandbox profile
    SandboxProfile {
        config: SandboxConfig,
    },
    /// Generic success
    Ok {
        message: String,
    },
    /// Error response
    Error {
        code: ErrorCode,
        message: String,
    },
}

/// Prompt details for user confirmation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptDetails {
    /// Application name
    pub app_name: String,
    /// Requested capability
    pub capability: String,
    /// Resource being accessed
    pub resource: Option<String>,
    /// Risk level
    pub risk_level: String,
    /// Explanation
    pub explanation: String,
    /// Timeout seconds
    pub timeout_secs: u32,
}

/// Error codes
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ErrorCode {
    InvalidRequest,
    NotFound,
    PermissionDenied,
    Timeout,
    InternalError,
}

/// Pending prompt state
struct PendingPrompt {
    request_id: Uuid,
    request: CapabilityRequest,
    decision: SecurityDecision,
    response_tx: mpsc::Sender<(bool, bool)>,
}

/// Guardian IPC server
pub struct GuardianServer {
    /// Socket path
    socket_path: PathBuf,
    /// Decision engine
    decision_engine: Arc<DecisionEngine>,
    /// Audit logger
    audit_logger: Arc<AuditLogger>,
    /// Sandbox enforcer
    sandbox_enforcer: Arc<RwLock<SandboxEnforcer>>,
    /// Pending prompts
    pending_prompts: Arc<RwLock<HashMap<Uuid, PendingPrompt>>>,
    /// Statistics
    stats: Arc<RwLock<ServerStats>>,
    /// Shutdown signal
    shutdown_tx: broadcast::Sender<()>,
    /// Start time
    start_time: std::time::Instant,
}

/// Server statistics
#[derive(Debug, Default)]
struct ServerStats {
    requests_processed: u64,
    decisions_allow: u64,
    decisions_deny: u64,
    decisions_sandbox: u64,
    decisions_prompt: u64,
    active_connections: u32,
}

impl GuardianServer {
    /// Create a new Guardian server
    pub fn new(
        socket_path: impl Into<PathBuf>,
        decision_engine: Arc<DecisionEngine>,
        audit_logger: Arc<AuditLogger>,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            socket_path: socket_path.into(),
            decision_engine,
            audit_logger,
            sandbox_enforcer: Arc::new(RwLock::new(SandboxEnforcer::new())),
            pending_prompts: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(ServerStats::default())),
            shutdown_tx,
            start_time: std::time::Instant::now(),
        }
    }

    /// Run the server
    pub async fn run(&self) -> Result<()> {
        // Remove old socket if exists
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        // Create parent directory
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Bind socket
        let listener = UnixListener::bind(&self.socket_path)
            .context("Failed to bind Guardian socket")?;

        // Set socket permissions (only root and guardian group)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o660);
            std::fs::set_permissions(&self.socket_path, perms)?;
        }

        info!("Guardian listening on {}", self.socket_path.display());

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
                            self.handle_connection(stream).await;
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Guardian server shutting down");
                    break;
                }
            }
        }

        // Cleanup
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }

        Ok(())
    }

    async fn handle_connection(&self, stream: UnixStream) {
        let decision_engine = self.decision_engine.clone();
        let audit_logger = self.audit_logger.clone();
        let sandbox_enforcer = self.sandbox_enforcer.clone();
        let pending_prompts = self.pending_prompts.clone();
        let stats = self.stats.clone();
        let start_time = self.start_time;

        // Update connection count
        {
            let mut s = stats.write().await;
            s.active_connections += 1;
        }

        tokio::spawn(async move {
            if let Err(e) = Self::process_connection(
                stream,
                decision_engine,
                audit_logger,
                sandbox_enforcer,
                pending_prompts,
                stats.clone(),
                start_time,
            ).await {
                debug!("Connection closed: {}", e);
            }

            // Update connection count
            let mut s = stats.write().await;
            s.active_connections = s.active_connections.saturating_sub(1);
        });
    }

    async fn process_connection(
        stream: UnixStream,
        decision_engine: Arc<DecisionEngine>,
        audit_logger: Arc<AuditLogger>,
        sandbox_enforcer: Arc<RwLock<SandboxEnforcer>>,
        pending_prompts: Arc<RwLock<HashMap<Uuid, PendingPrompt>>>,
        stats: Arc<RwLock<ServerStats>>,
        start_time: std::time::Instant,
    ) -> Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;
            if bytes_read == 0 {
                break; // Connection closed
            }

            let request: GuardianRequest = match serde_json::from_str(&line) {
                Ok(req) => req,
                Err(e) => {
                    let response = GuardianResponse::Error {
                        code: ErrorCode::InvalidRequest,
                        message: format!("Invalid JSON: {}", e),
                    };
                    let json = serde_json::to_string(&response)? + "\n";
                    writer.write_all(json.as_bytes()).await?;
                    continue;
                }
            };

            let response = Self::handle_request(
                request,
                &decision_engine,
                &audit_logger,
                &sandbox_enforcer,
                &pending_prompts,
                &stats,
                start_time,
            ).await;

            let json = serde_json::to_string(&response)? + "\n";
            writer.write_all(json.as_bytes()).await?;
        }

        Ok(())
    }

    async fn handle_request(
        request: GuardianRequest,
        decision_engine: &Arc<DecisionEngine>,
        audit_logger: &Arc<AuditLogger>,
        sandbox_enforcer: &Arc<RwLock<SandboxEnforcer>>,
        pending_prompts: &Arc<RwLock<HashMap<Uuid, PendingPrompt>>>,
        stats: &Arc<RwLock<ServerStats>>,
        start_time: std::time::Instant,
    ) -> GuardianResponse {
        match request {
            GuardianRequest::CheckCapability { request_id, request } => {
                // Log the request
                audit_logger.log_request(&request);

                // Evaluate
                let decision = decision_engine.evaluate(&request).await;

                // Update stats
                {
                    let mut s = stats.write().await;
                    s.requests_processed += 1;
                    match decision.decision {
                        FinalDecision::Allow => s.decisions_allow += 1,
                        FinalDecision::Deny => s.decisions_deny += 1,
                        FinalDecision::Sandbox(_) => s.decisions_sandbox += 1,
                        FinalDecision::Prompt => s.decisions_prompt += 1,
                    }
                }

                // Handle based on decision
                match decision.decision {
                    FinalDecision::Allow => {
                        decision_engine.record_decision(&request, &decision, false);
                        GuardianResponse::Decision {
                            request_id,
                            decision: "allow".into(),
                            reason: decision.reason.clone(),
                            sandbox_config: None,
                            recommended_action: decision.recommended_action.clone(),
                        }
                    }
                    FinalDecision::Deny => {
                        decision_engine.record_decision(&request, &decision, false);
                        GuardianResponse::Decision {
                            request_id,
                            decision: "deny".into(),
                            reason: decision.reason.clone(),
                            sandbox_config: None,
                            recommended_action: decision.recommended_action.clone(),
                        }
                    }
                    FinalDecision::Sandbox(level) => {
                        let enforcer = sandbox_enforcer.read().await;
                        let profile = enforcer.get_profile(level);
                        let config = enforcer.generate_config(profile);

                        decision_engine.record_decision(&request, &decision, false);

                        GuardianResponse::Decision {
                            request_id,
                            decision: format!("sandbox:{:?}", level).to_lowercase(),
                            reason: decision.reason.clone(),
                            sandbox_config: Some(config),
                            recommended_action: decision.recommended_action.clone(),
                        }
                    }
                    FinalDecision::Prompt => {
                        // Create prompt details
                        let details = PromptDetails {
                            app_name: request.process_path.clone(),
                            capability: request.capability.clone(),
                            resource: request.resource.clone(),
                            risk_level: decision.intent
                                .as_ref()
                                .map(|i| format!("{:?}", i.risk_level))
                                .unwrap_or_else(|| "Unknown".into()),
                            explanation: decision.reason.clone(),
                            timeout_secs: 30,
                        };

                        // Store pending prompt
                        let (tx, mut rx) = mpsc::channel(1);
                        {
                            let mut prompts = pending_prompts.write().await;
                            prompts.insert(request_id, PendingPrompt {
                                request_id,
                                request: request.clone(),
                                decision: decision.clone(),
                                response_tx: tx,
                            });
                        }

                        // Return prompt required (client should wait for user response)
                        GuardianResponse::PromptRequired {
                            request_id,
                            message: format!("{} wants to {}", request.process_path, request.capability),
                            details,
                        }
                    }
                }
            }

            GuardianRequest::UserResponse { request_id, approved, remember } => {
                let mut prompts = pending_prompts.write().await;

                if let Some(pending) = prompts.remove(&request_id) {
                    // Record decision
                    decision_engine.record_decision(&pending.request, &pending.decision, approved);

                    // TODO: If remember is true, update policy

                    // Notify waiting request
                    let _ = pending.response_tx.send((approved, remember)).await;

                    let decision_str = if approved { "allow" } else { "deny" };
                    GuardianResponse::Decision {
                        request_id,
                        decision: decision_str.into(),
                        reason: if approved {
                            "User approved".into()
                        } else {
                            "User denied".into()
                        },
                        sandbox_config: None,
                        recommended_action: None,
                    }
                } else {
                    GuardianResponse::Error {
                        code: ErrorCode::NotFound,
                        message: "No pending prompt with that ID".into(),
                    }
                }
            }

            GuardianRequest::Status => {
                let s = stats.read().await;
                GuardianResponse::Status {
                    version: env!("CARGO_PKG_VERSION").into(),
                    uptime_secs: start_time.elapsed().as_secs(),
                    requests_processed: s.requests_processed,
                    active_processes: s.active_connections,
                }
            }

            GuardianRequest::QueryPolicy { process_path, capability } => {
                // TODO: Implement policy query
                GuardianResponse::PolicyResult {
                    decision: "prompt".into(),
                    applies_to: vec![],
                }
            }

            GuardianRequest::RegisterProcess { pid, path, user } => {
                debug!("Process registered: {} ({}) by {}", path, pid, user);
                GuardianResponse::Ok {
                    message: "Process registered".into(),
                }
            }

            GuardianRequest::UnregisterProcess { pid } => {
                debug!("Process unregistered: {}", pid);
                GuardianResponse::Ok {
                    message: "Process unregistered".into(),
                }
            }

            GuardianRequest::GetSandboxProfile { level } => {
                let level = match level.to_lowercase().as_str() {
                    "light" => crate::decision::SandboxLevel::Light,
                    "medium" => crate::decision::SandboxLevel::Medium,
                    "heavy" => crate::decision::SandboxLevel::Heavy,
                    "maximum" => crate::decision::SandboxLevel::Maximum,
                    _ => {
                        return GuardianResponse::Error {
                            code: ErrorCode::InvalidRequest,
                            message: format!("Unknown sandbox level: {}", level),
                        };
                    }
                };

                let enforcer = sandbox_enforcer.read().await;
                let profile = enforcer.get_profile(level);
                let config = enforcer.generate_config(profile);

                GuardianResponse::SandboxProfile { config }
            }

            GuardianRequest::ReloadConfig => {
                // TODO: Implement config reload
                info!("Configuration reload requested");
                GuardianResponse::Ok {
                    message: "Configuration reloaded".into(),
                }
            }

            GuardianRequest::Shutdown => {
                info!("Shutdown requested via IPC");
                // TODO: Signal main loop
                GuardianResponse::Ok {
                    message: "Shutting down".into(),
                }
            }
        }
    }

    /// Shutdown the server
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }

    /// Get server statistics
    pub async fn get_stats(&self) -> (u64, u64, u64, u64, u64, u32) {
        let s = self.stats.read().await;
        (
            s.requests_processed,
            s.decisions_allow,
            s.decisions_deny,
            s.decisions_sandbox,
            s.decisions_prompt,
            s.active_connections,
        )
    }
}

/// Guardian IPC client (for other processes to use)
pub struct GuardianClient {
    socket_path: PathBuf,
}

impl GuardianClient {
    /// Create a new client
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    /// Connect to Guardian
    pub async fn connect(&self) -> Result<GuardianConnection> {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .context("Failed to connect to Guardian")?;

        Ok(GuardianConnection { stream })
    }
}

/// Active connection to Guardian
pub struct GuardianConnection {
    stream: UnixStream,
}

impl GuardianConnection {
    /// Send a request and receive response
    pub async fn request(&mut self, request: GuardianRequest) -> Result<GuardianResponse> {
        let json = serde_json::to_string(&request)? + "\n";
        self.stream.write_all(json.as_bytes()).await?;

        let mut reader = BufReader::new(&mut self.stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        let response: GuardianResponse = serde_json::from_str(&line)?;
        Ok(response)
    }

    /// Check a capability
    pub async fn check_capability(&mut self, request: CapabilityRequest) -> Result<GuardianResponse> {
        let request_id = Uuid::new_v4();
        self.request(GuardianRequest::CheckCapability { request_id, request }).await
    }

    /// Get status
    pub async fn status(&mut self) -> Result<GuardianResponse> {
        self.request(GuardianRequest::Status).await
    }

    /// Respond to a prompt
    pub async fn respond_to_prompt(&mut self, request_id: Uuid, approved: bool, remember: bool) -> Result<GuardianResponse> {
        self.request(GuardianRequest::UserResponse { request_id, approved, remember }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_request_serialization() {
        let request = GuardianRequest::CheckCapability {
            request_id: Uuid::new_v4(),
            request: CapabilityRequest {
                pid: 1234,
                process_path: "/usr/bin/test".into(),
                user: "testuser".into(),
                capability: "filesystem:read".into(),
                resource: Some("/home/testuser".into()),
                context: HashMap::new(),
            },
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: GuardianRequest = serde_json::from_str(&json).unwrap();

        match parsed {
            GuardianRequest::CheckCapability { request_id: _, request } => {
                assert_eq!(request.pid, 1234);
                assert_eq!(request.capability, "filesystem:read");
            }
            _ => panic!("Wrong request type"),
        }
    }

    #[tokio::test]
    async fn test_response_serialization() {
        let response = GuardianResponse::Decision {
            request_id: Uuid::new_v4(),
            decision: "allow".into(),
            reason: "Trusted application".into(),
            sandbox_config: None,
            recommended_action: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: GuardianResponse = serde_json::from_str(&json).unwrap();

        match parsed {
            GuardianResponse::Decision { decision, reason, .. } => {
                assert_eq!(decision, "allow");
                assert_eq!(reason, "Trusted application");
            }
            _ => panic!("Wrong response type"),
        }
    }
}
