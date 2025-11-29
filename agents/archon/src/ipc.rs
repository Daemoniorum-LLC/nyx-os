//! IPC server for Archon
//!
//! Provides an interface for other processes to interact with Archon.

use crate::orchestrator::Orchestrator;
use crate::process::{ProcessInfo, ProcessState, SpawnRequest, StdioConfig};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Archon request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ArchonRequest {
    /// Spawn a new process
    Spawn {
        name: String,
        executable: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        cwd: Option<String>,
        user: Option<String>,
        resource_profile: Option<String>,
        #[serde(default)]
        capabilities: Vec<String>,
        sandbox: Option<String>,
    },
    /// Get process info
    GetProcess {
        id: Uuid,
    },
    /// Get process by PID
    GetProcessByPid {
        pid: u32,
    },
    /// List all processes
    ListProcesses,
    /// List processes by state
    ListByState {
        state: String,
    },
    /// Terminate a process (SIGTERM)
    Terminate {
        id: Uuid,
    },
    /// Kill a process (SIGKILL)
    Kill {
        id: Uuid,
    },
    /// Stop a process (SIGSTOP)
    Stop {
        id: Uuid,
    },
    /// Continue a process (SIGCONT)
    Continue {
        id: Uuid,
    },
    /// Wait for process to exit
    Wait {
        id: Uuid,
    },
    /// Get system stats
    GetStats,
    /// Get stats history
    GetStatsHistory {
        limit: usize,
    },
    /// Get aggregate stats
    GetAggregateStats {
        duration_secs: u64,
    },
    /// Get system resources
    GetSystemResources,
    /// List resource profiles
    ListResourceProfiles,
    /// Get Archon status
    Status,
}

/// Archon response types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ArchonResponse {
    /// Process spawned successfully
    Spawned {
        process: ProcessInfo,
    },
    /// Process info
    Process {
        process: Option<ProcessInfo>,
    },
    /// Process list
    ProcessList {
        processes: Vec<ProcessInfo>,
    },
    /// Process exited
    Exited {
        exit_code: i32,
    },
    /// Statistics snapshot
    Stats {
        snapshot: crate::stats::StatsSnapshot,
    },
    /// Stats history
    StatsHistory {
        snapshots: Vec<crate::stats::StatsSnapshot>,
    },
    /// Aggregate stats
    AggregateStats {
        stats: crate::stats::AggregateStats,
    },
    /// System resources
    SystemResources {
        resources: SystemResourcesDto,
    },
    /// Resource profiles list
    ResourceProfiles {
        profiles: Vec<String>,
    },
    /// Archon status
    Status {
        version: String,
        uptime_secs: u64,
        process_count: u64,
        active_processes: usize,
    },
    /// Success response
    Ok {
        message: String,
    },
    /// Error response
    Error {
        message: String,
    },
}

/// System resources DTO (for serialization)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemResourcesDto {
    pub total_memory: u64,
    pub free_memory: u64,
    pub available_memory: u64,
    pub num_cpus: u32,
    pub cpu_usage_percent: u32,
}

/// Archon IPC server
pub struct ArchonServer {
    /// Socket path
    socket_path: PathBuf,
    /// Orchestrator
    orchestrator: Arc<Orchestrator>,
    /// Shutdown signal
    shutdown_tx: broadcast::Sender<()>,
    /// Start time
    start_time: std::time::Instant,
}

impl ArchonServer {
    /// Create a new Archon server
    pub fn new(socket_path: impl Into<PathBuf>, orchestrator: Arc<Orchestrator>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            socket_path: socket_path.into(),
            orchestrator,
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
            .context("Failed to bind Archon socket")?;

        // Set socket permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o660);
            std::fs::set_permissions(&self.socket_path, perms)?;
        }

        info!("Archon listening on {}", self.socket_path.display());

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
                            let orchestrator = self.orchestrator.clone();
                            let start_time = self.start_time;
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(stream, orchestrator, start_time).await {
                                    debug!("Connection closed: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Archon server shutting down");
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

    async fn handle_connection(
        stream: UnixStream,
        orchestrator: Arc<Orchestrator>,
        start_time: std::time::Instant,
    ) -> Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;
            if bytes_read == 0 {
                break;
            }

            let request: ArchonRequest = match serde_json::from_str(&line) {
                Ok(req) => req,
                Err(e) => {
                    let response = ArchonResponse::Error {
                        message: format!("Invalid JSON: {}", e),
                    };
                    let json = serde_json::to_string(&response)? + "\n";
                    writer.write_all(json.as_bytes()).await?;
                    continue;
                }
            };

            let response = Self::handle_request(request, &orchestrator, start_time).await;
            let json = serde_json::to_string(&response)? + "\n";
            writer.write_all(json.as_bytes()).await?;
        }

        Ok(())
    }

    async fn handle_request(
        request: ArchonRequest,
        orchestrator: &Orchestrator,
        start_time: std::time::Instant,
    ) -> ArchonResponse {
        match request {
            ArchonRequest::Spawn {
                name,
                executable,
                args,
                env,
                cwd,
                user,
                resource_profile,
                capabilities,
                sandbox,
            } => {
                let spawn_request = SpawnRequest {
                    name,
                    executable: PathBuf::from(executable),
                    args,
                    env,
                    cwd: cwd.map(PathBuf::from),
                    user,
                    resource_profile,
                    capabilities,
                    sandbox,
                    parent_id: None,
                    stdin: StdioConfig::Null,
                    stdout: StdioConfig::Null,
                    stderr: StdioConfig::Null,
                };

                match orchestrator.spawn(spawn_request).await {
                    Ok(process) => ArchonResponse::Spawned { process },
                    Err(e) => ArchonResponse::Error {
                        message: e.to_string(),
                    },
                }
            }

            ArchonRequest::GetProcess { id } => {
                let process = orchestrator.get_process(&id).await;
                ArchonResponse::Process { process }
            }

            ArchonRequest::GetProcessByPid { pid } => {
                let process = orchestrator.get_process_by_pid(pid).await;
                ArchonResponse::Process { process }
            }

            ArchonRequest::ListProcesses => {
                let processes = orchestrator.list_processes().await;
                ArchonResponse::ProcessList { processes }
            }

            ArchonRequest::ListByState { state } => {
                let state = match state.to_lowercase().as_str() {
                    "running" => ProcessState::Running,
                    "stopped" => ProcessState::Stopped,
                    "exited" => ProcessState::Exited,
                    "failed" => ProcessState::Failed,
                    "zombie" => ProcessState::Zombie,
                    _ => {
                        return ArchonResponse::Error {
                            message: format!("Unknown state: {}", state),
                        };
                    }
                };
                let processes = orchestrator.list_processes_by_state(state).await;
                ArchonResponse::ProcessList { processes }
            }

            ArchonRequest::Terminate { id } => {
                match orchestrator.terminate(&id).await {
                    Ok(()) => ArchonResponse::Ok {
                        message: "Process terminated".into(),
                    },
                    Err(e) => ArchonResponse::Error {
                        message: e.to_string(),
                    },
                }
            }

            ArchonRequest::Kill { id } => {
                match orchestrator.kill(&id).await {
                    Ok(()) => ArchonResponse::Ok {
                        message: "Process killed".into(),
                    },
                    Err(e) => ArchonResponse::Error {
                        message: e.to_string(),
                    },
                }
            }

            ArchonRequest::Stop { id } => {
                match orchestrator.stop(&id).await {
                    Ok(()) => ArchonResponse::Ok {
                        message: "Process stopped".into(),
                    },
                    Err(e) => ArchonResponse::Error {
                        message: e.to_string(),
                    },
                }
            }

            ArchonRequest::Continue { id } => {
                match orchestrator.cont(&id).await {
                    Ok(()) => ArchonResponse::Ok {
                        message: "Process continued".into(),
                    },
                    Err(e) => ArchonResponse::Error {
                        message: e.to_string(),
                    },
                }
            }

            ArchonRequest::Wait { id } => {
                match orchestrator.wait(&id).await {
                    Ok(exit_code) => ArchonResponse::Exited { exit_code },
                    Err(e) => ArchonResponse::Error {
                        message: e.to_string(),
                    },
                }
            }

            ArchonRequest::GetStats => {
                let snapshot = orchestrator.collect_stats().await;
                ArchonResponse::Stats { snapshot }
            }

            ArchonRequest::GetStatsHistory { limit } => {
                let snapshots = orchestrator.stats_history(limit).await;
                ArchonResponse::StatsHistory { snapshots }
            }

            ArchonRequest::GetAggregateStats { duration_secs } => {
                let stats = orchestrator.aggregate_stats(duration_secs).await;
                ArchonResponse::AggregateStats { stats }
            }

            ArchonRequest::GetSystemResources => {
                match orchestrator.system_resources().await {
                    Ok(resources) => ArchonResponse::SystemResources {
                        resources: SystemResourcesDto {
                            total_memory: resources.total_memory,
                            free_memory: resources.free_memory,
                            available_memory: resources.available_memory,
                            num_cpus: resources.num_cpus,
                            cpu_usage_percent: resources.cpu_usage_percent,
                        },
                    },
                    Err(e) => ArchonResponse::Error {
                        message: e.to_string(),
                    },
                }
            }

            ArchonRequest::ListResourceProfiles => {
                let profiles = orchestrator.list_resource_profiles().await;
                ArchonResponse::ResourceProfiles { profiles }
            }

            ArchonRequest::Status => {
                ArchonResponse::Status {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    uptime_secs: start_time.elapsed().as_secs(),
                    process_count: orchestrator.process_count().await,
                    active_processes: orchestrator.active_process_count().await,
                }
            }
        }
    }

    /// Shutdown the server
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let request = ArchonRequest::Spawn {
            name: "test".into(),
            executable: "/bin/echo".into(),
            args: vec!["hello".into()],
            env: HashMap::new(),
            cwd: None,
            user: None,
            resource_profile: Some("standard".into()),
            capabilities: vec!["filesystem:read".into()],
            sandbox: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("Spawn"));
        assert!(json.contains("/bin/echo"));
    }
}
