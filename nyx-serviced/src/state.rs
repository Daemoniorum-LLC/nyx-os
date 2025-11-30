//! Service state management

use chrono::{DateTime, Local, Duration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

/// Service states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
    /// Not started
    Stopped,
    /// Currently starting
    Starting,
    /// Running normally
    Running,
    /// Currently stopping
    Stopping,
    /// Reloading configuration
    Reloading,
    /// Failed to start or crashed
    Failed,
    /// Waiting for activation (socket, etc.)
    Activating,
    /// Deactivating
    Deactivating,
}

impl ServiceState {
    pub fn is_active(&self) -> bool {
        matches!(self, ServiceState::Running | ServiceState::Reloading)
    }

    pub fn can_start(&self) -> bool {
        matches!(self, ServiceState::Stopped | ServiceState::Failed)
    }

    pub fn can_stop(&self) -> bool {
        matches!(self, ServiceState::Running | ServiceState::Starting | ServiceState::Reloading)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ServiceState::Stopped => "stopped",
            ServiceState::Starting => "starting",
            ServiceState::Running => "running",
            ServiceState::Stopping => "stopping",
            ServiceState::Reloading => "reloading",
            ServiceState::Failed => "failed",
            ServiceState::Activating => "activating",
            ServiceState::Deactivating => "deactivating",
        }
    }
}

/// Extended service status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    /// Current state
    pub state: ServiceState,
    /// Process ID (if running)
    pub pid: Option<u32>,
    /// Main process ID (for forking services)
    pub main_pid: Option<u32>,
    /// When the service was started
    pub started_at: Option<DateTime<Local>>,
    /// When the service was stopped
    pub stopped_at: Option<DateTime<Local>>,
    /// Number of restarts
    pub restart_count: u32,
    /// Last exit code
    pub last_exit_code: Option<i32>,
    /// Last exit signal
    pub last_exit_signal: Option<i32>,
    /// Failure reason
    pub failure_reason: Option<String>,
    /// Memory usage in bytes
    pub memory_bytes: Option<u64>,
    /// CPU usage percentage
    pub cpu_percent: Option<f64>,
    /// Last watchdog ping
    pub last_watchdog_ping: Option<DateTime<Local>>,
    /// Whether this was a clean stop
    pub clean_stop: bool,
}

impl Default for ServiceStatus {
    fn default() -> Self {
        Self {
            state: ServiceState::Stopped,
            pid: None,
            main_pid: None,
            started_at: None,
            stopped_at: None,
            restart_count: 0,
            last_exit_code: None,
            last_exit_signal: None,
            failure_reason: None,
            memory_bytes: None,
            cpu_percent: None,
            last_watchdog_ping: None,
            clean_stop: true,
        }
    }
}

impl ServiceStatus {
    /// Calculate uptime
    pub fn uptime(&self) -> Option<Duration> {
        if self.state.is_active() {
            self.started_at.map(|s| Local::now() - s)
        } else {
            None
        }
    }

    /// Format uptime as human-readable string
    pub fn uptime_string(&self) -> Option<String> {
        self.uptime().map(|d| {
            let secs = d.num_seconds();
            if secs < 60 {
                format!("{}s", secs)
            } else if secs < 3600 {
                format!("{}m {}s", secs / 60, secs % 60)
            } else if secs < 86400 {
                format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
            } else {
                format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
            }
        })
    }

    /// Check if service should be considered healthy
    pub fn is_healthy(&self, watchdog_interval: u64) -> bool {
        if !self.state.is_active() {
            return false;
        }

        if watchdog_interval == 0 {
            return true;
        }

        // Check watchdog timeout
        if let Some(last_ping) = self.last_watchdog_ping {
            let elapsed = (Local::now() - last_ping).num_seconds() as u64;
            elapsed < watchdog_interval * 2
        } else {
            // No watchdog pings yet - give it time to start
            if let Some(started) = self.started_at {
                let elapsed = (Local::now() - started).num_seconds() as u64;
                elapsed < watchdog_interval * 2
            } else {
                false
            }
        }
    }

    /// Mark service as started
    pub fn mark_started(&mut self, pid: u32) {
        self.state = ServiceState::Running;
        self.pid = Some(pid);
        self.started_at = Some(Local::now());
        self.stopped_at = None;
        self.failure_reason = None;
        self.clean_stop = false;
    }

    /// Mark service as stopped
    pub fn mark_stopped(&mut self, exit_code: Option<i32>, signal: Option<i32>, clean: bool) {
        self.state = if clean || exit_code == Some(0) {
            ServiceState::Stopped
        } else {
            ServiceState::Failed
        };
        self.pid = None;
        self.stopped_at = Some(Local::now());
        self.last_exit_code = exit_code;
        self.last_exit_signal = signal;
        self.clean_stop = clean;

        if !clean && exit_code != Some(0) {
            self.failure_reason = Some(format!(
                "Exit code: {:?}, Signal: {:?}",
                exit_code, signal
            ));
        }
    }

    /// Mark service as failed
    pub fn mark_failed(&mut self, reason: &str) {
        self.state = ServiceState::Failed;
        self.pid = None;
        self.stopped_at = Some(Local::now());
        self.failure_reason = Some(reason.to_string());
        self.clean_stop = false;
    }

    /// Increment restart count
    pub fn increment_restart(&mut self) {
        self.restart_count += 1;
    }

    /// Record watchdog ping
    pub fn watchdog_ping(&mut self) {
        self.last_watchdog_ping = Some(Local::now());
    }

    /// Update resource metrics
    pub fn update_metrics(&mut self, memory: Option<u64>, cpu: Option<f64>) {
        self.memory_bytes = memory;
        self.cpu_percent = cpu;
    }
}

/// State manager for all services
pub struct StateManager {
    states: HashMap<String, ServiceStatus>,
    next_instance_id: AtomicU32,
}

impl StateManager {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            next_instance_id: AtomicU32::new(1),
        }
    }

    /// Get or create state for a service
    pub fn get_or_create(&mut self, name: &str) -> &mut ServiceStatus {
        self.states.entry(name.to_string()).or_default()
    }

    /// Get state for a service
    pub fn get(&self, name: &str) -> Option<&ServiceStatus> {
        self.states.get(name)
    }

    /// Get mutable state for a service
    pub fn get_mut(&mut self, name: &str) -> Option<&mut ServiceStatus> {
        self.states.get_mut(name)
    }

    /// Set state for a service
    pub fn set_state(&mut self, name: &str, state: ServiceState) {
        self.get_or_create(name).state = state;
    }

    /// Get all services in a specific state
    pub fn by_state(&self, state: ServiceState) -> impl Iterator<Item = (&str, &ServiceStatus)> {
        self.states.iter()
            .filter(move |(_, s)| s.state == state)
            .map(|(n, s)| (n.as_str(), s))
    }

    /// Get all active services
    pub fn active(&self) -> impl Iterator<Item = (&str, &ServiceStatus)> {
        self.states.iter()
            .filter(|(_, s)| s.state.is_active())
            .map(|(n, s)| (n.as_str(), s))
    }

    /// Get all failed services
    pub fn failed(&self) -> impl Iterator<Item = (&str, &ServiceStatus)> {
        self.states.iter()
            .filter(|(_, s)| s.state == ServiceState::Failed)
            .map(|(n, s)| (n.as_str(), s))
    }

    /// Generate a unique instance ID
    pub fn next_instance_id(&self) -> u32 {
        self.next_instance_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Get all states
    pub fn all(&self) -> impl Iterator<Item = (&str, &ServiceStatus)> {
        self.states.iter().map(|(n, s)| (n.as_str(), s))
    }

    /// Count services by state
    pub fn count_by_state(&self) -> HashMap<ServiceState, usize> {
        let mut counts = HashMap::new();
        for status in self.states.values() {
            *counts.entry(status.state).or_insert(0) += 1;
        }
        counts
    }

    /// Reset all restart counts (e.g., after burst window)
    pub fn reset_restart_counts(&mut self) {
        for status in self.states.values_mut() {
            status.restart_count = 0;
        }
    }

    /// Remove state for a service
    pub fn remove(&mut self, name: &str) -> Option<ServiceStatus> {
        self.states.remove(name)
    }

    /// Clear all states
    pub fn clear(&mut self) {
        self.states.clear();
    }
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Notification for state changes
#[derive(Debug, Clone)]
pub struct StateChange {
    pub service: String,
    pub from: ServiceState,
    pub to: ServiceState,
    pub timestamp: DateTime<Local>,
    pub reason: Option<String>,
}

impl StateChange {
    pub fn new(service: &str, from: ServiceState, to: ServiceState) -> Self {
        Self {
            service: service.to_string(),
            from,
            to,
            timestamp: Local::now(),
            reason: None,
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_state_transitions() {
        let mut status = ServiceStatus::default();
        assert_eq!(status.state, ServiceState::Stopped);
        assert!(status.state.can_start());

        status.mark_started(1234);
        assert_eq!(status.state, ServiceState::Running);
        assert!(status.state.is_active());
        assert!(status.state.can_stop());

        status.mark_stopped(Some(0), None, true);
        assert_eq!(status.state, ServiceState::Stopped);
        assert!(status.clean_stop);
    }

    #[test]
    fn test_uptime_string() {
        let mut status = ServiceStatus::default();
        status.mark_started(1);

        // Just check it produces something reasonable
        let uptime = status.uptime_string();
        assert!(uptime.is_some());
    }
}
