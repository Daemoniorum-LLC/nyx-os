//! Service definition and management

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Service specification (from YAML)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSpec {
    /// Service name (unique identifier)
    pub name: String,

    /// Human-readable description
    #[serde(default)]
    pub description: String,

    /// Executable path
    pub exec: String,

    /// Command-line arguments
    #[serde(default)]
    pub args: Vec<String>,

    /// Service type
    #[serde(default)]
    pub service_type: ServiceType,

    /// Restart policy
    #[serde(default)]
    pub restart: RestartPolicy,

    /// Maximum restart attempts
    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,

    /// Restart delay in seconds
    #[serde(default = "default_restart_delay")]
    pub restart_delay_sec: u32,

    /// Service dependencies (other service names)
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Required capabilities
    #[serde(default)]
    pub capabilities: Vec<String>,

    /// Environment variables
    #[serde(default)]
    pub environment: HashMap<String, String>,

    /// Working directory
    pub working_dir: Option<PathBuf>,

    /// User to run as
    pub user: Option<String>,

    /// Group to run as
    pub group: Option<String>,

    /// Health check configuration
    #[serde(default)]
    pub health_check: Option<HealthCheck>,

    /// Sandbox configuration
    #[serde(default)]
    pub sandbox: SandboxConfig,

    /// Ready notification type
    #[serde(default)]
    pub ready_notify: ReadyNotify,

    /// Timeout for service to become ready
    #[serde(default = "default_ready_timeout")]
    pub ready_timeout_sec: u32,
}

impl Default for ServiceSpec {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            exec: String::new(),
            args: Vec::new(),
            service_type: ServiceType::default(),
            restart: RestartPolicy::default(),
            max_restarts: default_max_restarts(),
            restart_delay_sec: default_restart_delay(),
            dependencies: Vec::new(),
            capabilities: Vec::new(),
            environment: HashMap::new(),
            working_dir: None,
            user: None,
            group: None,
            health_check: None,
            sandbox: SandboxConfig::default(),
            ready_notify: ReadyNotify::default(),
            ready_timeout_sec: default_ready_timeout(),
        }
    }
}

fn default_max_restarts() -> u32 {
    5
}

fn default_restart_delay() -> u32 {
    1
}

fn default_ready_timeout() -> u32 {
    30
}

impl ServiceSpec {
    /// Validate the service specification
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            bail!("Service name cannot be empty");
        }

        if self.exec.is_empty() {
            bail!("Service {} has no executable", self.name);
        }

        // Validate dependencies don't include self
        if self.dependencies.contains(&self.name) {
            bail!("Service {} cannot depend on itself", self.name);
        }

        Ok(())
    }
}

/// Type of service
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceType {
    /// Simple one-shot service
    Simple,
    /// Long-running daemon
    #[default]
    Daemon,
    /// AI agent (has special IPC integration)
    Agent,
    /// One-shot task
    Oneshot,
    /// Service that forks
    Forking,
}

/// Restart policy
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RestartPolicy {
    /// Never restart
    Never,
    /// Always restart
    #[default]
    Always,
    /// Restart on failure only
    OnFailure,
    /// Restart on abnormal exit
    OnAbnormal,
}

/// How service signals readiness
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReadyNotify {
    /// Ready immediately after exec
    #[default]
    Immediate,
    /// Ready after forking (for forking services)
    Forked,
    /// Ready when socket is created
    Socket { path: String },
    /// Ready when IPC endpoint is registered
    Ipc { endpoint: String },
    /// Ready when health check passes
    HealthCheck,
    /// Ready via systemd-style notify
    Notify,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Check type
    #[serde(rename = "type")]
    pub check_type: HealthCheckType,

    /// Interval between checks
    #[serde(default = "default_health_interval")]
    pub interval_sec: u32,

    /// Timeout for each check
    #[serde(default = "default_health_timeout")]
    pub timeout_sec: u32,

    /// Retries before marking unhealthy
    #[serde(default = "default_health_retries")]
    pub retries: u32,
}

fn default_health_interval() -> u32 {
    30
}

fn default_health_timeout() -> u32 {
    10
}

fn default_health_retries() -> u32 {
    3
}

/// Health check types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthCheckType {
    /// HTTP GET request
    Http { url: String, expected_status: Option<u16> },
    /// TCP connection
    Tcp { host: String, port: u16 },
    /// Unix socket connection
    Socket { path: String },
    /// Execute command
    Command { cmd: String, args: Vec<String> },
    /// IPC ping
    Ipc { endpoint: String },
}

/// Sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SandboxConfig {
    /// Enable sandboxing
    #[serde(default)]
    pub enabled: bool,

    /// Allow network access
    #[serde(default)]
    pub network: bool,

    /// Allowed filesystem paths
    #[serde(default)]
    pub allowed_paths: Vec<PathBuf>,

    /// Read-only filesystem paths
    #[serde(default)]
    pub readonly_paths: Vec<PathBuf>,

    /// Enable GPU access
    #[serde(default)]
    pub gpu: bool,

    /// Custom seccomp profile
    pub seccomp_profile: Option<PathBuf>,
}

/// Runtime service state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    /// Not started
    Stopped,
    /// Starting up
    Starting,
    /// Running and healthy
    Running,
    /// Failed health check
    Unhealthy,
    /// Stopping
    Stopping,
    /// Failed to start
    Failed,
    /// Restarting
    Restarting,
}

/// Runtime service instance
pub struct Service {
    /// Service specification
    pub spec: ServiceSpec,
    /// Current state
    pub state: ServiceState,
    /// Child process handle
    process: Option<Child>,
    /// Process ID
    pub pid: Option<u32>,
    /// Start time
    pub started_at: Option<Instant>,
    /// Restart count
    pub restart_count: u32,
    /// Last error
    pub last_error: Option<String>,
}

impl Service {
    /// Create a new service from spec
    pub fn new(spec: ServiceSpec) -> Self {
        Self {
            spec,
            state: ServiceState::Stopped,
            process: None,
            pid: None,
            started_at: None,
            restart_count: 0,
            last_error: None,
        }
    }

    /// Start the service
    pub async fn start(&mut self) -> Result<()> {
        if self.state == ServiceState::Running {
            debug!("Service {} already running", self.spec.name);
            return Ok(());
        }

        info!("Starting service: {}", self.spec.name);
        self.state = ServiceState::Starting;

        // Build command
        let mut cmd = Command::new(&self.spec.exec);
        cmd.args(&self.spec.args);

        // Set environment
        for (key, value) in &self.spec.environment {
            cmd.env(key, value);
        }

        // Set working directory
        if let Some(ref dir) = self.spec.working_dir {
            cmd.current_dir(dir);
        }

        // Set up stdio
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Spawn process
        match cmd.spawn() {
            Ok(child) => {
                self.pid = child.id();
                self.process = Some(child);
                self.started_at = Some(Instant::now());
                self.state = ServiceState::Running;
                info!(
                    "Service {} started with PID {:?}",
                    self.spec.name, self.pid
                );
                Ok(())
            }
            Err(e) => {
                self.state = ServiceState::Failed;
                self.last_error = Some(e.to_string());
                error!("Failed to start service {}: {}", self.spec.name, e);
                Err(e.into())
            }
        }
    }

    /// Stop the service
    pub async fn stop(&mut self) -> Result<()> {
        if self.state == ServiceState::Stopped {
            return Ok(());
        }

        info!("Stopping service: {}", self.spec.name);
        self.state = ServiceState::Stopping;

        if let Some(ref mut child) = self.process {
            // Send SIGTERM first
            #[cfg(unix)]
            {
                if let Some(pid) = self.pid {
                    let _ = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(pid as i32),
                        nix::sys::signal::Signal::SIGTERM,
                    );
                }
            }

            // Wait for graceful shutdown (with timeout)
            let timeout = tokio::time::timeout(Duration::from_secs(10), child.wait()).await;

            match timeout {
                Ok(Ok(status)) => {
                    info!(
                        "Service {} exited with status: {}",
                        self.spec.name, status
                    );
                }
                Ok(Err(e)) => {
                    warn!("Error waiting for service {}: {}", self.spec.name, e);
                }
                Err(_) => {
                    // Timeout - send SIGKILL
                    warn!(
                        "Service {} did not stop gracefully, sending SIGKILL",
                        self.spec.name
                    );
                    let _ = child.kill().await;
                }
            }
        }

        self.process = None;
        self.pid = None;
        self.state = ServiceState::Stopped;

        Ok(())
    }

    /// Restart the service
    pub async fn restart(&mut self) -> Result<()> {
        self.state = ServiceState::Restarting;
        self.stop().await?;

        // Delay before restart
        tokio::time::sleep(Duration::from_secs(self.spec.restart_delay_sec as u64)).await;

        self.restart_count += 1;
        self.start().await
    }

    /// Check if process is still running
    pub async fn check_alive(&mut self) -> bool {
        if let Some(ref mut child) = self.process {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Process exited
                    info!(
                        "Service {} exited with status: {}",
                        self.spec.name, status
                    );
                    self.state = if status.success() {
                        ServiceState::Stopped
                    } else {
                        ServiceState::Failed
                    };
                    self.process = None;
                    self.pid = None;
                    false
                }
                Ok(None) => {
                    // Still running
                    true
                }
                Err(e) => {
                    error!("Error checking service {}: {}", self.spec.name, e);
                    false
                }
            }
        } else {
            false
        }
    }

    /// Should this service be restarted?
    pub fn should_restart(&self) -> bool {
        if self.restart_count >= self.spec.max_restarts {
            return false;
        }

        match self.spec.restart {
            RestartPolicy::Always => true,
            RestartPolicy::OnFailure => self.state == ServiceState::Failed,
            RestartPolicy::OnAbnormal => {
                self.state == ServiceState::Failed || self.state == ServiceState::Unhealthy
            }
            RestartPolicy::Never => false,
        }
    }
}
