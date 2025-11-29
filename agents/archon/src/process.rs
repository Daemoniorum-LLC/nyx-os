//! Process management
//!
//! Handles process lifecycle: spawning, monitoring, and termination.

use crate::config::ProcessConfig;
use crate::resource::ResourceManager;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use nix::sys::signal::{self, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Process state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessState {
    /// Process is being created
    Creating,
    /// Process is running
    Running,
    /// Process is sleeping/waiting
    Sleeping,
    /// Process is stopped (SIGSTOP)
    Stopped,
    /// Process is a zombie (exited but not reaped)
    Zombie,
    /// Process has exited
    Exited,
    /// Process failed to start
    Failed,
}

/// Process information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    /// Unique process ID (Archon-assigned)
    pub id: Uuid,
    /// System PID
    pub pid: u32,
    /// Process name
    pub name: String,
    /// Executable path
    pub executable: PathBuf,
    /// Command line arguments
    pub args: Vec<String>,
    /// Working directory
    pub cwd: Option<PathBuf>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// User running the process
    pub user: String,
    /// Process state
    pub state: ProcessState,
    /// Parent process ID (if managed)
    pub parent_id: Option<Uuid>,
    /// Resource profile applied
    pub resource_profile: Option<String>,
    /// Cgroup path
    pub cgroup_path: Option<String>,
    /// Capabilities granted
    pub capabilities: Vec<String>,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Start time
    pub started_at: Option<DateTime<Utc>>,
    /// Exit time
    pub exited_at: Option<DateTime<Utc>>,
    /// Exit code (if exited)
    pub exit_code: Option<i32>,
    /// Exit signal (if killed)
    pub exit_signal: Option<i32>,
}

/// Process spawn request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnRequest {
    /// Process name
    pub name: String,
    /// Executable path
    pub executable: PathBuf,
    /// Command line arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory
    pub cwd: Option<PathBuf>,
    /// Environment variables (merged with defaults)
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// User to run as (None = current user)
    pub user: Option<String>,
    /// Resource profile to apply
    pub resource_profile: Option<String>,
    /// Capabilities needed
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Sandbox profile (if any)
    pub sandbox: Option<String>,
    /// Parent process ID
    pub parent_id: Option<Uuid>,
    /// Stdin handling
    #[serde(default)]
    pub stdin: StdioConfig,
    /// Stdout handling
    #[serde(default)]
    pub stdout: StdioConfig,
    /// Stderr handling
    #[serde(default)]
    pub stderr: StdioConfig,
}

/// Stdio configuration
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StdioConfig {
    /// Inherit from parent
    #[default]
    Inherit,
    /// Create a pipe
    Pipe,
    /// Redirect to /dev/null
    Null,
}

/// Active process handle
pub struct ActiveProcess {
    /// Process info
    pub info: ProcessInfo,
    /// Tokio child handle
    child: Option<Child>,
}

/// Process manager
pub struct ProcessManager {
    /// Configuration
    config: ProcessConfig,
    /// Resource manager
    resource_manager: Arc<RwLock<ResourceManager>>,
    /// Active processes by ID
    processes: DashMap<Uuid, ActiveProcess>,
    /// PID to ID mapping
    pid_map: DashMap<u32, Uuid>,
    /// Process counter
    process_count: AtomicU64,
    /// Default environment
    default_env: HashMap<String, String>,
}

impl ProcessManager {
    /// Create a new process manager
    pub fn new(
        config: &ProcessConfig,
        resource_manager: Arc<RwLock<ResourceManager>>,
    ) -> Result<Self> {
        let default_env: HashMap<String, String> = config
            .default_env
            .iter()
            .map(|e| (e.key.clone(), e.value.clone()))
            .collect();

        Ok(Self {
            config: config.clone(),
            resource_manager,
            processes: DashMap::new(),
            pid_map: DashMap::new(),
            process_count: AtomicU64::new(0),
            default_env,
        })
    }

    /// Spawn a new process
    pub async fn spawn(&self, request: SpawnRequest) -> Result<ProcessInfo> {
        // Check process limit
        let current_count = self.process_count.load(Ordering::SeqCst);
        if current_count >= self.config.max_processes as u64 {
            anyhow::bail!("Process limit reached ({}/{})", current_count, self.config.max_processes);
        }

        let id = Uuid::new_v4();
        let user = request.user.clone().unwrap_or_else(|| {
            std::env::var("USER").unwrap_or_else(|_| "unknown".into())
        });

        // Merge environment
        let mut env = self.default_env.clone();
        env.extend(request.env.clone());

        // Create process info
        let mut info = ProcessInfo {
            id,
            pid: 0, // Will be set after spawn
            name: request.name.clone(),
            executable: request.executable.clone(),
            args: request.args.clone(),
            cwd: request.cwd.clone(),
            env: env.clone(),
            user: user.clone(),
            state: ProcessState::Creating,
            parent_id: request.parent_id,
            resource_profile: request.resource_profile.clone(),
            cgroup_path: None,
            capabilities: request.capabilities.clone(),
            created_at: Utc::now(),
            started_at: None,
            exited_at: None,
            exit_code: None,
            exit_signal: None,
        };

        // Build command
        let mut cmd = Command::new(&request.executable);
        cmd.args(&request.args);

        // Set environment
        cmd.env_clear();
        for (key, value) in &env {
            cmd.env(key, value);
        }

        // Set working directory
        if let Some(ref cwd) = request.cwd {
            cmd.current_dir(cwd);
        }

        // Configure stdio
        cmd.stdin(match request.stdin {
            StdioConfig::Inherit => Stdio::inherit(),
            StdioConfig::Pipe => Stdio::piped(),
            StdioConfig::Null => Stdio::null(),
        });
        cmd.stdout(match request.stdout {
            StdioConfig::Inherit => Stdio::inherit(),
            StdioConfig::Pipe => Stdio::piped(),
            StdioConfig::Null => Stdio::null(),
        });
        cmd.stderr(match request.stderr {
            StdioConfig::Inherit => Stdio::inherit(),
            StdioConfig::Pipe => Stdio::piped(),
            StdioConfig::Null => Stdio::null(),
        });

        // Spawn process
        let child = cmd.spawn().context("Failed to spawn process")?;

        let pid = child.id().ok_or_else(|| anyhow::anyhow!("Failed to get PID"))?;
        info.pid = pid;
        info.state = ProcessState::Running;
        info.started_at = Some(Utc::now());

        // Apply resource limits
        if let Some(ref profile) = request.resource_profile {
            let resource_manager = self.resource_manager.read().await;
            if let Err(e) = resource_manager.apply_profile(pid, profile).await {
                warn!("Failed to apply resource profile {}: {}", profile, e);
            } else {
                info.cgroup_path = Some(format!("nyx/{}", id));
            }
        }

        info!(
            "Spawned process {} (pid={}, name={})",
            id, pid, request.name
        );

        let process_info = info.clone();

        // Store process
        self.processes.insert(id, ActiveProcess {
            info,
            child: Some(child),
        });
        self.pid_map.insert(pid, id);
        self.process_count.fetch_add(1, Ordering::SeqCst);

        Ok(process_info)
    }

    /// Get process by ID
    pub fn get(&self, id: &Uuid) -> Option<ProcessInfo> {
        self.processes.get(id).map(|p| p.info.clone())
    }

    /// Get process by PID
    pub fn get_by_pid(&self, pid: u32) -> Option<ProcessInfo> {
        self.pid_map
            .get(&pid)
            .and_then(|id| self.processes.get(&id).map(|p| p.info.clone()))
    }

    /// List all processes
    pub fn list(&self) -> Vec<ProcessInfo> {
        self.processes
            .iter()
            .map(|entry| entry.info.clone())
            .collect()
    }

    /// List processes by state
    pub fn list_by_state(&self, state: ProcessState) -> Vec<ProcessInfo> {
        self.processes
            .iter()
            .filter(|entry| entry.info.state == state)
            .map(|entry| entry.info.clone())
            .collect()
    }

    /// Send signal to process
    pub fn signal(&self, id: &Uuid, sig: Signal) -> Result<()> {
        let process = self.processes.get(id)
            .ok_or_else(|| anyhow::anyhow!("Process not found"))?;

        let pid = Pid::from_raw(process.info.pid as i32);
        signal::kill(pid, sig).context("Failed to send signal")?;

        debug!("Sent {:?} to process {} (pid={})", sig, id, process.info.pid);
        Ok(())
    }

    /// Terminate process gracefully (SIGTERM)
    pub fn terminate(&self, id: &Uuid) -> Result<()> {
        self.signal(id, Signal::SIGTERM)
    }

    /// Kill process forcefully (SIGKILL)
    pub fn kill(&self, id: &Uuid) -> Result<()> {
        self.signal(id, Signal::SIGKILL)
    }

    /// Stop process (SIGSTOP)
    pub fn stop(&self, id: &Uuid) -> Result<()> {
        let result = self.signal(id, Signal::SIGSTOP)?;

        // Update state
        if let Some(mut process) = self.processes.get_mut(id) {
            process.info.state = ProcessState::Stopped;
        }

        Ok(result)
    }

    /// Continue process (SIGCONT)
    pub fn cont(&self, id: &Uuid) -> Result<()> {
        let result = self.signal(id, Signal::SIGCONT)?;

        // Update state
        if let Some(mut process) = self.processes.get_mut(id) {
            process.info.state = ProcessState::Running;
        }

        Ok(result)
    }

    /// Wait for process to exit
    pub async fn wait(&self, id: &Uuid) -> Result<i32> {
        let mut process = self.processes.get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Process not found"))?;

        if let Some(ref mut child) = process.child {
            let status = child.wait().await?;
            let exit_code = status.code().unwrap_or(-1);

            process.info.state = ProcessState::Exited;
            process.info.exited_at = Some(Utc::now());
            process.info.exit_code = Some(exit_code);

            Ok(exit_code)
        } else {
            Err(anyhow::anyhow!("Process already waited"))
        }
    }

    /// Reap zombie processes
    pub fn reap_zombies(&self) -> Vec<(Uuid, i32)> {
        let mut reaped = Vec::new();

        for mut entry in self.processes.iter_mut() {
            if entry.info.state == ProcessState::Running {
                let pid = Pid::from_raw(entry.info.pid as i32);

                match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
                    Ok(WaitStatus::Exited(_, code)) => {
                        entry.info.state = ProcessState::Exited;
                        entry.info.exited_at = Some(Utc::now());
                        entry.info.exit_code = Some(code);
                        reaped.push((entry.info.id, code));
                        debug!("Reaped process {} (pid={}, exit={})", entry.info.id, entry.info.pid, code);
                    }
                    Ok(WaitStatus::Signaled(_, sig, _)) => {
                        entry.info.state = ProcessState::Exited;
                        entry.info.exited_at = Some(Utc::now());
                        entry.info.exit_signal = Some(sig as i32);
                        reaped.push((entry.info.id, -1));
                        debug!("Process {} killed by signal {:?}", entry.info.id, sig);
                    }
                    Ok(WaitStatus::Stopped(_, _)) => {
                        entry.info.state = ProcessState::Stopped;
                    }
                    Ok(WaitStatus::Continued(_)) => {
                        entry.info.state = ProcessState::Running;
                    }
                    Ok(_) => {}
                    Err(nix::errno::Errno::ECHILD) => {
                        // No child - process might have been reaped elsewhere
                        entry.info.state = ProcessState::Exited;
                        entry.info.exited_at = Some(Utc::now());
                    }
                    Err(e) => {
                        warn!("waitpid error for {}: {}", entry.info.id, e);
                    }
                }
            }
        }

        reaped
    }

    /// Cleanup exited processes
    pub fn cleanup(&self, max_age_secs: u64) -> Vec<Uuid> {
        let now = Utc::now();
        let mut removed = Vec::new();

        self.processes.retain(|id, process| {
            if process.info.state == ProcessState::Exited {
                if let Some(exited_at) = process.info.exited_at {
                    let age = (now - exited_at).num_seconds() as u64;
                    if age > max_age_secs {
                        self.pid_map.remove(&process.info.pid);
                        self.process_count.fetch_sub(1, Ordering::SeqCst);
                        removed.push(*id);
                        return false;
                    }
                }
            }
            true
        });

        if !removed.is_empty() {
            debug!("Cleaned up {} exited processes", removed.len());
        }

        removed
    }

    /// Get process count
    pub fn count(&self) -> u64 {
        self.process_count.load(Ordering::SeqCst)
    }

    /// Get active (running) process count
    pub fn active_count(&self) -> usize {
        self.processes
            .iter()
            .filter(|p| p.info.state == ProcessState::Running)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ResourceConfig;
    use crate::cgroup::CgroupManager;
    use crate::config::CgroupConfig;

    #[tokio::test]
    async fn test_spawn_request() {
        let request = SpawnRequest {
            name: "test".into(),
            executable: PathBuf::from("/bin/echo"),
            args: vec!["hello".into()],
            cwd: None,
            env: HashMap::new(),
            user: None,
            resource_profile: None,
            capabilities: vec![],
            sandbox: None,
            parent_id: None,
            stdin: StdioConfig::Null,
            stdout: StdioConfig::Null,
            stderr: StdioConfig::Null,
        };

        assert_eq!(request.name, "test");
        assert_eq!(request.args, vec!["hello"]);
    }
}
