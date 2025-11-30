//! Service lifecycle management

use crate::dependency::{check_dependencies, get_start_before, DependencyCheck};
use crate::state::{ServiceState, ServiceStatus, StateManager};
use crate::unit::{RestartPolicy, ServiceType, Unit, UnitRegistry};
use anyhow::{Result, Context, anyhow};
use libnyx_platform::PlatformCapabilities;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::collections::{HashMap, HashSet};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{timeout, Duration};
use tracing::{info, warn, error, debug};

/// Lifecycle manager for services
pub struct LifecycleManager {
    units: Arc<RwLock<UnitRegistry>>,
    states: Arc<RwLock<StateManager>>,
    capabilities: PlatformCapabilities,
    processes: RwLock<HashMap<String, Child>>,
    log_dir: PathBuf,
}

impl LifecycleManager {
    pub fn new(
        units: Arc<RwLock<UnitRegistry>>,
        states: Arc<RwLock<StateManager>>,
        capabilities: PlatformCapabilities,
    ) -> Self {
        Self {
            units,
            states,
            capabilities,
            processes: RwLock::new(HashMap::new()),
            log_dir: PathBuf::from("/var/log/nyx"),
        }
    }

    /// Start all enabled services in dependency order
    pub async fn start_enabled(&self) -> Result<usize> {
        let units = self.units.read().await;
        let enabled: Vec<_> = units.enabled().cloned().collect();
        drop(units);

        let mut started = 0;

        // Build dependency order
        let unit_refs: Vec<&Unit> = enabled.iter().collect();
        let order = crate::dependency::resolve_order(&unit_refs)?;

        for unit in order {
            if let Err(e) = self.start(&unit.name).await {
                error!("Failed to start {}: {}", unit.name, e);
            } else {
                started += 1;
            }
        }

        Ok(started)
    }

    /// Start a service
    pub async fn start(&self, name: &str) -> Result<()> {
        let units = self.units.read().await;
        let unit = units.get(name)
            .ok_or_else(|| anyhow!("Service not found: {}", name))?
            .clone();
        drop(units);

        // Check current state
        {
            let states = self.states.read().await;
            if let Some(status) = states.get(name) {
                if status.state.is_active() {
                    info!("Service {} is already running", name);
                    return Ok(());
                }
                if status.state == ServiceState::Starting {
                    return Err(anyhow!("Service {} is already starting", name));
                }
            }
        }

        // Check dependencies
        let running = self.get_running_services().await;
        let available = self.get_available_services().await;
        let dep_check = check_dependencies(&unit, &running, &available);

        match dep_check {
            DependencyCheck::Missing(missing) => {
                return Err(anyhow!(
                    "Missing dependencies for {}: {:?}",
                    name, missing
                ));
            }
            DependencyCheck::NotRunning(not_running) => {
                // Start dependencies first
                for dep in not_running {
                    info!("Starting dependency {} for {}", dep, name);
                    if let Err(e) = self.start(&dep).await {
                        return Err(anyhow!(
                            "Failed to start dependency {} for {}: {}",
                            dep, name, e
                        ));
                    }
                }
            }
            DependencyCheck::Satisfied => {}
        }

        // Set starting state
        self.states.write().await.set_state(name, ServiceState::Starting);

        info!("Starting service: {}", name);

        // Execute start command
        match self.execute_start(&unit).await {
            Ok(pid) => {
                info!("Service {} started with PID {}", name, pid);
                self.states.write().await.get_or_create(name).mark_started(pid);
                Ok(())
            }
            Err(e) => {
                error!("Failed to start {}: {}", name, e);
                self.states.write().await.get_or_create(name).mark_failed(&e.to_string());
                Err(e)
            }
        }
    }

    /// Stop a service
    pub async fn stop(&self, name: &str) -> Result<()> {
        let states = self.states.read().await;
        let status = states.get(name);

        if status.is_none() || !status.unwrap().state.can_stop() {
            return Ok(());
        }

        let pid = status.and_then(|s| s.pid);
        drop(states);

        // Set stopping state
        self.states.write().await.set_state(name, ServiceState::Stopping);

        info!("Stopping service: {}", name);

        // Get unit config for timeouts
        let units = self.units.read().await;
        let unit = units.get(name).cloned();
        drop(units);

        let timeout_sec = unit.as_ref()
            .map(|u| u.service.timeout_stop_sec)
            .unwrap_or(90);

        // Try exec_stop first
        if let Some(unit) = &unit {
            if let Some(stop_cmd) = &unit.service.exec_stop {
                if let Err(e) = self.execute_command(stop_cmd, &unit.service).await {
                    warn!("exec_stop failed for {}: {}", name, e);
                }
            }
        }

        // Send SIGTERM
        if let Some(pid) = pid {
            let nix_pid = Pid::from_raw(pid as i32);

            debug!("Sending SIGTERM to PID {}", pid);
            let _ = signal::kill(nix_pid, Signal::SIGTERM);

            // Wait for graceful shutdown
            let stopped = self.wait_for_exit(name, timeout_sec).await;

            if !stopped {
                // Force kill
                warn!("Service {} did not stop gracefully, sending SIGKILL", name);
                let _ = signal::kill(nix_pid, Signal::SIGKILL);
                self.wait_for_exit(name, 5).await;
            }
        }

        // Remove from process map
        self.processes.write().await.remove(name);

        // Mark as stopped
        self.states.write().await.get_or_create(name)
            .mark_stopped(Some(0), None, true);

        info!("Service {} stopped", name);
        Ok(())
    }

    /// Restart a service
    pub async fn restart(&self, name: &str) -> Result<()> {
        self.stop(name).await?;
        self.start(name).await
    }

    /// Reload service configuration
    pub async fn reload(&self, name: &str) -> Result<()> {
        let units = self.units.read().await;
        let unit = units.get(name)
            .ok_or_else(|| anyhow!("Service not found: {}", name))?;

        if let Some(reload_cmd) = &unit.service.exec_reload {
            info!("Reloading service: {}", name);
            self.states.write().await.set_state(name, ServiceState::Reloading);

            let result = self.execute_command(reload_cmd, &unit.service).await;

            self.states.write().await.set_state(name, ServiceState::Running);
            result?;

            info!("Service {} reloaded", name);
            Ok(())
        } else {
            // Fall back to restart
            warn!("No reload command for {}, restarting instead", name);
            drop(units);
            self.restart(name).await
        }
    }

    /// Handle service exit
    pub async fn handle_exit(&self, name: &str, exit_code: i32, signal: Option<i32>) {
        info!(
            "Service {} exited with code {} (signal: {:?})",
            name, exit_code, signal
        );

        let units = self.units.read().await;
        let unit = units.get(name).cloned();
        drop(units);

        let should_restart = unit.as_ref().map(|u| {
            match u.service.restart {
                RestartPolicy::No => false,
                RestartPolicy::Always => true,
                RestartPolicy::OnFailure => exit_code != 0 || signal.is_some(),
                RestartPolicy::OnAbnormal => signal.is_some(),
                RestartPolicy::OnAbort => signal == Some(6), // SIGABRT
                RestartPolicy::OnWatchdog => false, // Handled by watchdog
                RestartPolicy::UnlessStopped => {
                    let states = futures::executor::block_on(self.states.read());
                    !states.get(name).map(|s| s.clean_stop).unwrap_or(false)
                }
            }
        }).unwrap_or(false);

        // Update state
        {
            let mut states = self.states.write().await;
            states.get_or_create(name).mark_stopped(Some(exit_code), signal, false);
        }

        // Check restart limits
        if should_restart {
            let restart_ok = {
                let states = self.states.read().await;
                let status = states.get(name);
                if let (Some(status), Some(unit)) = (status, &unit) {
                    unit.service.restart_max == 0 ||
                    status.restart_count < unit.service.restart_max
                } else {
                    true
                }
            };

            if restart_ok {
                let restart_sec = unit.map(|u| u.service.restart_sec).unwrap_or(1);
                let name = name.to_string();
                let lifecycle = self.clone_arc();

                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(restart_sec)).await;
                    lifecycle.states.write().await.get_or_create(&name).increment_restart();
                    if let Err(e) = lifecycle.start(&name).await {
                        error!("Failed to restart {}: {}", name, e);
                    }
                });
            } else {
                error!(
                    "Service {} exceeded restart limit, not restarting",
                    name
                );
            }
        }
    }

    /// Execute service start command
    async fn execute_start(&self, unit: &Unit) -> Result<u32> {
        let exec_start = unit.service.exec_start.as_ref()
            .ok_or_else(|| anyhow!("No ExecStart defined for {}", unit.name))?;

        // Run pre-start commands
        for pre in &unit.service.exec_start_pre {
            self.execute_command(pre, &unit.service).await
                .with_context(|| format!("ExecStartPre failed: {}", pre))?;
        }

        // Parse command
        let parts: Vec<&str> = exec_start.split_whitespace().collect();
        if parts.is_empty() {
            return Err(anyhow!("Empty ExecStart command"));
        }

        let mut cmd = Command::new(parts[0]);

        if parts.len() > 1 {
            cmd.args(&parts[1..]);
        }

        // Set working directory
        if let Some(wd) = &unit.service.working_directory {
            cmd.current_dir(wd);
        }

        // Set environment
        for (key, value) in &unit.service.environment {
            cmd.env(key, value);
        }

        // Load environment files
        for env_file in &unit.service.environment_file {
            if env_file.exists() {
                if let Ok(content) = std::fs::read_to_string(env_file) {
                    for line in content.lines() {
                        if let Some((key, value)) = line.split_once('=') {
                            cmd.env(key.trim(), value.trim().trim_matches('"'));
                        }
                    }
                }
            }
        }

        // Set up stdio
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Set up user/group (requires privileges)
        if let Some(user) = &unit.service.user {
            if let Ok(uid) = user.parse::<u32>() {
                unsafe {
                    cmd.pre_exec(move || {
                        nix::unistd::setuid(nix::unistd::Uid::from_raw(uid))?;
                        Ok(())
                    });
                }
            }
        }

        // Spawn process
        let mut child = cmd.spawn()
            .with_context(|| format!("Failed to spawn {}", parts[0]))?;

        let pid = child.id().ok_or_else(|| anyhow!("No PID for child"))?;

        // Spawn log handlers
        let name = unit.name.clone();
        let log_dir = self.log_dir.clone();

        if let Some(stdout) = child.stdout.take() {
            let name = name.clone();
            let log_dir = log_dir.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    debug!("[{}] {}", name, line);
                    // Could write to log file here
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            let name = name.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    warn!("[{}] {}", name, line);
                }
            });
        }

        // Store child handle
        self.processes.write().await.insert(unit.name.clone(), child);

        // For simple services, we're ready immediately
        // For notify services, we'd wait for notification
        match unit.service.service_type {
            ServiceType::Simple | ServiceType::Idle => {
                // Ready immediately
            }
            ServiceType::Forking => {
                // Wait for parent to exit, track child PID
                let timeout_sec = unit.service.timeout_start_sec;
                let stopped = self.wait_for_exit(&unit.name, timeout_sec).await;
                if !stopped {
                    return Err(anyhow!("Forking service did not complete in time"));
                }
                // Would need to read PID file here
            }
            ServiceType::Oneshot => {
                // Wait for completion
                let timeout_sec = unit.service.timeout_start_sec;
                let stopped = self.wait_for_exit(&unit.name, timeout_sec).await;
                if !stopped {
                    return Err(anyhow!("Oneshot service did not complete in time"));
                }
            }
            ServiceType::Notify | ServiceType::Dbus => {
                // Would wait for notification
            }
        }

        // Run post-start commands
        for post in &unit.service.exec_start_post {
            if let Err(e) = self.execute_command(post, &unit.service).await {
                warn!("ExecStartPost failed: {}", e);
            }
        }

        Ok(pid)
    }

    /// Execute a command
    async fn execute_command(&self, cmd: &str, config: &crate::unit::ServiceConfig) -> Result<()> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        let mut command = Command::new(parts[0]);

        if parts.len() > 1 {
            command.args(&parts[1..]);
        }

        if let Some(wd) = &config.working_directory {
            command.current_dir(wd);
        }

        for (key, value) in &config.environment {
            command.env(key, value);
        }

        let output = command.output().await
            .with_context(|| format!("Failed to execute: {}", cmd))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow!(
                "Command failed with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// Wait for a service to exit
    async fn wait_for_exit(&self, name: &str, timeout_sec: u64) -> bool {
        let mut processes = self.processes.write().await;

        if let Some(child) = processes.get_mut(name) {
            match timeout(Duration::from_secs(timeout_sec), child.wait()).await {
                Ok(Ok(_)) => true,
                _ => false,
            }
        } else {
            true
        }
    }

    /// Get set of running services
    async fn get_running_services(&self) -> HashSet<String> {
        self.states.read().await
            .active()
            .map(|(n, _)| n.to_string())
            .collect()
    }

    /// Get set of available (registered) services
    async fn get_available_services(&self) -> HashSet<String> {
        self.units.read().await
            .names()
            .map(|s| s.to_string())
            .collect()
    }

    /// Clone as Arc (for spawning)
    fn clone_arc(&self) -> Arc<Self> {
        // This is a workaround - in real code we'd use Arc<Self>
        Arc::new(Self {
            units: self.units.clone(),
            states: self.states.clone(),
            capabilities: self.capabilities.clone(),
            processes: RwLock::new(HashMap::new()),
            log_dir: self.log_dir.clone(),
        })
    }

    /// Get status of a service
    pub async fn status(&self, name: &str) -> Option<ServiceStatus> {
        self.states.read().await.get(name).cloned()
    }

    /// Get all service statuses
    pub async fn all_status(&self) -> Vec<(String, ServiceStatus)> {
        self.states.read().await
            .all()
            .map(|(n, s)| (n.to_string(), s.clone()))
            .collect()
    }
}
