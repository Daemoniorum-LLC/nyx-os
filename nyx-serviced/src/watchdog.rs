//! Service health monitoring and watchdog

use crate::lifecycle::LifecycleManager;
use crate::state::{ServiceState, StateManager};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

/// Watchdog for monitoring service health
pub struct Watchdog {
    lifecycle: Arc<LifecycleManager>,
    states: Arc<RwLock<StateManager>>,
    check_interval: Duration,
    last_pings: RwLock<HashMap<String, Instant>>,
}

impl Watchdog {
    pub fn new(
        lifecycle: Arc<LifecycleManager>,
        states: Arc<RwLock<StateManager>>,
    ) -> Self {
        Self {
            lifecycle,
            states,
            check_interval: Duration::from_secs(5),
            last_pings: RwLock::new(HashMap::new()),
        }
    }

    /// Run the watchdog loop
    pub async fn run(&self) {
        info!("Watchdog started with {}s check interval", self.check_interval.as_secs());

        let mut interval = tokio::time::interval(self.check_interval);

        loop {
            interval.tick().await;

            if let Err(e) = self.check_services().await {
                error!("Watchdog check failed: {}", e);
            }
        }
    }

    /// Check all running services
    async fn check_services(&self) -> anyhow::Result<()> {
        let states = self.states.read().await;
        let running: Vec<_> = states
            .by_state(ServiceState::Running)
            .map(|(n, s)| (n.to_string(), s.clone()))
            .collect();
        drop(states);

        for (name, status) in running {
            // Check if service has a watchdog configured
            if let Some(watchdog_sec) = self.get_watchdog_interval(&name).await {
                if watchdog_sec > 0 {
                    self.check_watchdog(&name, watchdog_sec, &status).await;
                }
            }

            // Check if process is still alive
            if let Some(pid) = status.pid {
                if !is_process_alive(pid) {
                    warn!("Service {} (PID {}) is no longer running", name, pid);
                    // The lifecycle manager should handle this via wait()
                    // but we can trigger it here as a backup
                    self.lifecycle.handle_exit(&name, -1, Some(9)).await;
                }
            }
        }

        Ok(())
    }

    /// Check watchdog for a specific service
    async fn check_watchdog(&self, name: &str, interval_sec: u64, status: &crate::state::ServiceStatus) {
        let now = Instant::now();
        let timeout = Duration::from_secs(interval_sec * 2);

        let last_ping = {
            let pings = self.last_pings.read().await;
            pings.get(name).copied()
        };

        let should_restart = if let Some(last) = last_ping {
            now.duration_since(last) > timeout
        } else {
            // No ping yet - check if service has been running long enough
            if let Some(started) = status.started_at {
                let running_time = chrono::Local::now() - started;
                running_time.num_seconds() as u64 > interval_sec * 3
            } else {
                false
            }
        };

        if should_restart {
            error!("Watchdog timeout for {} - restarting", name);
            if let Err(e) = self.lifecycle.restart(name).await {
                error!("Failed to restart {} after watchdog timeout: {}", name, e);
            }
        }
    }

    /// Record a watchdog ping from a service
    pub async fn ping(&self, name: &str) {
        debug!("Watchdog ping from {}", name);

        self.last_pings.write().await.insert(name.to_string(), Instant::now());

        // Also update the state manager
        self.states.write().await.get_or_create(name).watchdog_ping();
    }

    /// Get watchdog interval for a service
    async fn get_watchdog_interval(&self, _name: &str) -> Option<u64> {
        // This would look up the unit configuration
        // For now, return None (no watchdog)
        None
    }

    /// Reset watchdog tracking for a service
    pub async fn reset(&self, name: &str) {
        self.last_pings.write().await.remove(name);
    }
}

/// Check if a process is still alive
fn is_process_alive(pid: u32) -> bool {
    // Try to send signal 0 (no signal, just check if process exists)
    let result = nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid as i32),
        None,
    );

    result.is_ok()
}

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthCheck {
    /// Type of health check
    pub check_type: HealthCheckType,
    /// Check interval
    pub interval: Duration,
    /// Timeout for each check
    pub timeout: Duration,
    /// Number of failures before marking unhealthy
    pub retries: u32,
    /// Command to run (for exec type)
    pub command: Option<String>,
    /// HTTP endpoint (for http type)
    pub http_endpoint: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum HealthCheckType {
    /// Process is running
    ProcessAlive,
    /// Execute a command
    Exec,
    /// HTTP endpoint check
    Http,
    /// TCP port check
    Tcp,
    /// Custom script
    Script,
}

impl Default for HealthCheck {
    fn default() -> Self {
        Self {
            check_type: HealthCheckType::ProcessAlive,
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
            retries: 3,
            command: None,
            http_endpoint: None,
        }
    }
}

/// Health check runner
pub struct HealthChecker {
    checks: HashMap<String, HealthCheck>,
    failures: RwLock<HashMap<String, u32>>,
}

impl HealthChecker {
    pub fn new() -> Self {
        Self {
            checks: HashMap::new(),
            failures: RwLock::new(HashMap::new()),
        }
    }

    /// Register a health check for a service
    pub fn register(&mut self, name: &str, check: HealthCheck) {
        self.checks.insert(name.to_string(), check);
    }

    /// Run health check for a service
    pub async fn check(&self, name: &str, pid: Option<u32>) -> HealthCheckResult {
        let check = match self.checks.get(name) {
            Some(c) => c,
            None => return HealthCheckResult::Healthy,
        };

        let result = match check.check_type {
            HealthCheckType::ProcessAlive => {
                if let Some(pid) = pid {
                    if is_process_alive(pid) {
                        HealthCheckResult::Healthy
                    } else {
                        HealthCheckResult::Unhealthy("Process not running".into())
                    }
                } else {
                    HealthCheckResult::Unhealthy("No PID".into())
                }
            }
            HealthCheckType::Exec => {
                if let Some(cmd) = &check.command {
                    self.run_exec_check(cmd, check.timeout).await
                } else {
                    HealthCheckResult::Unhealthy("No command configured".into())
                }
            }
            HealthCheckType::Http => {
                if let Some(endpoint) = &check.http_endpoint {
                    self.run_http_check(endpoint, check.timeout).await
                } else {
                    HealthCheckResult::Unhealthy("No endpoint configured".into())
                }
            }
            HealthCheckType::Tcp => {
                // Would check TCP port
                HealthCheckResult::Healthy
            }
            HealthCheckType::Script => {
                if let Some(cmd) = &check.command {
                    self.run_exec_check(cmd, check.timeout).await
                } else {
                    HealthCheckResult::Unhealthy("No script configured".into())
                }
            }
        };

        // Track failures
        match &result {
            HealthCheckResult::Healthy => {
                self.failures.write().await.remove(name);
            }
            HealthCheckResult::Unhealthy(_) => {
                let mut failures = self.failures.write().await;
                let count = failures.entry(name.to_string()).or_insert(0);
                *count += 1;
            }
        }

        result
    }

    async fn run_exec_check(&self, cmd: &str, timeout: Duration) -> HealthCheckResult {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return HealthCheckResult::Unhealthy("Empty command".into());
        }

        let result = tokio::time::timeout(
            timeout,
            tokio::process::Command::new(parts[0])
                .args(&parts[1..])
                .output()
        ).await;

        match result {
            Ok(Ok(output)) if output.status.success() => HealthCheckResult::Healthy,
            Ok(Ok(output)) => HealthCheckResult::Unhealthy(
                format!("Exit code: {}", output.status)
            ),
            Ok(Err(e)) => HealthCheckResult::Unhealthy(format!("Failed to run: {}", e)),
            Err(_) => HealthCheckResult::Unhealthy("Timeout".into()),
        }
    }

    async fn run_http_check(&self, _endpoint: &str, _timeout: Duration) -> HealthCheckResult {
        // Would make HTTP request here
        // For now, just return healthy
        HealthCheckResult::Healthy
    }

    /// Check if a service has exceeded failure threshold
    pub async fn is_unhealthy(&self, name: &str) -> bool {
        let check = match self.checks.get(name) {
            Some(c) => c,
            None => return false,
        };

        let failures = self.failures.read().await;
        failures.get(name).copied().unwrap_or(0) >= check.retries
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum HealthCheckResult {
    Healthy,
    Unhealthy(String),
}

impl HealthCheckResult {
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthCheckResult::Healthy)
    }
}
