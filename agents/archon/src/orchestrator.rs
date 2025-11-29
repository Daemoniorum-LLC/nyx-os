//! Process orchestrator
//!
//! Coordinates process management with Guardian security checks.

use crate::process::{ProcessInfo, ProcessManager, ProcessState, SpawnRequest};
use crate::resource::ResourceManager;
use crate::stats::StatsCollector;
use anyhow::{Context, Result};
use libnyx_ipc::guardian::GuardianClient;
use libnyx_ipc::protocol::{CapabilityRequest, Decision};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Process orchestrator
pub struct Orchestrator {
    /// Process manager
    process_manager: Arc<RwLock<ProcessManager>>,
    /// Resource manager
    resource_manager: Arc<RwLock<ResourceManager>>,
    /// Stats collector
    stats_collector: Arc<StatsCollector>,
    /// Guardian client
    guardian_client: RwLock<Option<GuardianClient>>,
    /// Guardian socket path
    guardian_socket: PathBuf,
    /// Whether Guardian integration is enabled
    guardian_enabled: bool,
}

impl Orchestrator {
    /// Create a new orchestrator
    pub async fn new(
        process_manager: Arc<RwLock<ProcessManager>>,
        resource_manager: Arc<RwLock<ResourceManager>>,
        stats_collector: Arc<StatsCollector>,
        guardian_socket: PathBuf,
    ) -> Result<Self> {
        // Try to connect to Guardian
        let guardian_client = match GuardianClient::with_socket(&guardian_socket).connect_internal().await {
            Ok(client) => {
                info!("Connected to Guardian");
                Some(client)
            }
            Err(e) => {
                warn!("Failed to connect to Guardian: {} - continuing without security checks", e);
                None
            }
        };

        Ok(Self {
            process_manager,
            resource_manager,
            stats_collector,
            guardian_client: RwLock::new(guardian_client),
            guardian_socket,
            guardian_enabled: true,
        })
    }

    /// Spawn a process with Guardian capability check
    pub async fn spawn(&self, request: SpawnRequest) -> Result<ProcessInfo> {
        // Check capabilities with Guardian
        if self.guardian_enabled && !request.capabilities.is_empty() {
            for cap in &request.capabilities {
                let allowed = self.check_capability(
                    &request.executable.display().to_string(),
                    cap,
                    request.cwd.as_ref().map(|p| p.display().to_string()).as_deref(),
                ).await?;

                if !allowed {
                    return Err(anyhow::anyhow!(
                        "Capability '{}' denied for {}",
                        cap,
                        request.executable.display()
                    ));
                }
            }
        }

        // Spawn the process
        let pm = self.process_manager.read().await;
        let info = pm.spawn(request).await?;

        info!("Orchestrator spawned process: {} (pid={})", info.name, info.pid);
        Ok(info)
    }

    /// Check a capability with Guardian
    pub async fn check_capability(
        &self,
        process_path: &str,
        capability: &str,
        resource: Option<&str>,
    ) -> Result<bool> {
        let mut client_guard = self.guardian_client.write().await;

        // Try to reconnect if not connected
        if client_guard.is_none() {
            match GuardianClient::with_socket(&self.guardian_socket).connect_internal().await {
                Ok(client) => {
                    *client_guard = Some(client);
                }
                Err(e) => {
                    warn!("Cannot connect to Guardian: {} - allowing by default", e);
                    return Ok(true); // Allow if Guardian unavailable
                }
            }
        }

        if let Some(ref mut client) = *client_guard {
            let mut cap_request = CapabilityRequest::new(capability);
            cap_request.process_path = process_path.to_string();
            if let Some(res) = resource {
                cap_request = cap_request.with_resource(res);
            }

            match client.check_capability_full(cap_request).await {
                Ok(decision) => {
                    match decision.decision {
                        Decision::Allow | Decision::Sandbox => {
                            debug!("Guardian allowed {} for {}", capability, process_path);
                            Ok(true)
                        }
                        Decision::Deny => {
                            warn!("Guardian denied {} for {}: {}", capability, process_path, decision.reason);
                            Ok(false)
                        }
                        Decision::Prompt => {
                            // For now, treat prompt as allow (would need UI integration)
                            info!("Guardian requires prompt for {} - allowing", capability);
                            Ok(true)
                        }
                    }
                }
                Err(e) => {
                    warn!("Guardian check failed: {} - allowing by default", e);
                    Ok(true)
                }
            }
        } else {
            Ok(true)
        }
    }

    /// Get process by ID
    pub async fn get_process(&self, id: &Uuid) -> Option<ProcessInfo> {
        let pm = self.process_manager.read().await;
        pm.get(id)
    }

    /// Get process by PID
    pub async fn get_process_by_pid(&self, pid: u32) -> Option<ProcessInfo> {
        let pm = self.process_manager.read().await;
        pm.get_by_pid(pid)
    }

    /// List all processes
    pub async fn list_processes(&self) -> Vec<ProcessInfo> {
        let pm = self.process_manager.read().await;
        pm.list()
    }

    /// List processes by state
    pub async fn list_processes_by_state(&self, state: ProcessState) -> Vec<ProcessInfo> {
        let pm = self.process_manager.read().await;
        pm.list_by_state(state)
    }

    /// Terminate a process
    pub async fn terminate(&self, id: &Uuid) -> Result<()> {
        let pm = self.process_manager.read().await;
        pm.terminate(id)
    }

    /// Kill a process
    pub async fn kill(&self, id: &Uuid) -> Result<()> {
        let pm = self.process_manager.read().await;
        pm.kill(id)
    }

    /// Stop a process
    pub async fn stop(&self, id: &Uuid) -> Result<()> {
        let pm = self.process_manager.read().await;
        pm.stop(id)
    }

    /// Continue a stopped process
    pub async fn cont(&self, id: &Uuid) -> Result<()> {
        let pm = self.process_manager.read().await;
        pm.cont(id)
    }

    /// Wait for a process
    pub async fn wait(&self, id: &Uuid) -> Result<i32> {
        let pm = self.process_manager.read().await;
        pm.wait(id).await
    }

    /// Get resource profiles
    pub async fn list_resource_profiles(&self) -> Vec<String> {
        let rm = self.resource_manager.read().await;
        rm.list_profiles().iter().map(|p| p.name.clone()).collect()
    }

    /// Get system resources
    pub async fn system_resources(&self) -> Result<crate::resource::SystemResources> {
        let rm = self.resource_manager.read().await;
        rm.system_resources()
    }

    /// Collect statistics
    pub async fn collect_stats(&self) -> crate::stats::StatsSnapshot {
        self.stats_collector.collect().await
    }

    /// Get latest stats
    pub async fn latest_stats(&self) -> Option<crate::stats::StatsSnapshot> {
        self.stats_collector.latest().await
    }

    /// Get stats history
    pub async fn stats_history(&self, limit: usize) -> Vec<crate::stats::StatsSnapshot> {
        self.stats_collector.history(limit).await
    }

    /// Get aggregate stats
    pub async fn aggregate_stats(&self, duration_secs: u64) -> crate::stats::AggregateStats {
        self.stats_collector.aggregate(duration_secs).await
    }

    /// Run background tasks
    pub async fn run_background_tasks(&self) -> Result<()> {
        let pm = self.process_manager.clone();
        let stats = self.stats_collector.clone();

        // Zombie reaper task
        let pm_reaper = pm.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                let pm = pm_reaper.read().await;
                pm.reap_zombies();
            }
        });

        // Cleanup task
        let pm_cleanup = pm.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let pm = pm_cleanup.read().await;
                pm.cleanup(300); // Clean up processes exited more than 5 minutes ago
            }
        });

        // Stats collection task
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                stats.collect().await;
            }
        });

        info!("Background tasks started");
        Ok(())
    }

    /// Get process count
    pub async fn process_count(&self) -> u64 {
        let pm = self.process_manager.read().await;
        pm.count()
    }

    /// Get active process count
    pub async fn active_process_count(&self) -> usize {
        let pm = self.process_manager.read().await;
        pm.active_count()
    }
}

/// Process group management
#[derive(Debug, Clone)]
pub struct ProcessGroup {
    /// Group ID
    pub id: Uuid,
    /// Group name
    pub name: String,
    /// Member process IDs
    pub members: Vec<Uuid>,
    /// Resource profile for the group
    pub resource_profile: Option<String>,
    /// Cgroup path
    pub cgroup_path: Option<String>,
}

impl ProcessGroup {
    /// Create a new process group
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            members: Vec::new(),
            resource_profile: None,
            cgroup_path: None,
        }
    }

    /// Add a process to the group
    pub fn add(&mut self, process_id: Uuid) {
        if !self.members.contains(&process_id) {
            self.members.push(process_id);
        }
    }

    /// Remove a process from the group
    pub fn remove(&mut self, process_id: &Uuid) {
        self.members.retain(|id| id != process_id);
    }
}
