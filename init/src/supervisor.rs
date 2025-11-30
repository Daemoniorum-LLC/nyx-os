//! Service supervisor - manages service lifecycle

use crate::config::InitConfig;
use crate::dependency::DependencyGraph;
use crate::service::{Service, ServiceState};
use anyhow::Result;
use dashmap::DashMap;
use libnyx_ipc::guardian::GuardianClient;
use libnyx_ipc::protocol::{CapabilityRequest, Decision};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};
use tracing::{debug, error, info, warn};

/// Events emitted by the supervisor
#[derive(Debug, Clone)]
pub enum SupervisorEvent {
    /// Service started
    ServiceStarted { name: String, pid: u32 },
    /// Service stopped
    ServiceStopped { name: String },
    /// Service failed
    ServiceFailed { name: String, error: String },
    /// Service restarting
    ServiceRestarting { name: String, attempt: u32 },
    /// All services ready
    AllServicesReady,
    /// Shutdown initiated
    ShutdownInitiated,
    /// Shutdown complete
    ShutdownComplete,
}

/// Service supervisor
pub struct Supervisor {
    /// Configuration
    config: InitConfig,
    /// Dependency graph
    dep_graph: DependencyGraph,
    /// Running services
    services: DashMap<String, Service>,
    /// Event broadcaster
    events: broadcast::Sender<SupervisorEvent>,
    /// Shutdown flag
    shutdown: Arc<tokio::sync::Notify>,
    /// Guardian client (if enabled)
    guardian: Option<Arc<Mutex<GuardianClient>>>,
}

impl Supervisor {
    /// Create a new supervisor
    pub fn new(config: InitConfig, dep_graph: DependencyGraph) -> Self {
        let (events, _) = broadcast::channel(256);

        Self {
            config,
            dep_graph,
            services: DashMap::new(),
            events,
            shutdown: Arc::new(tokio::sync::Notify::new()),
            guardian: None,
        }
    }

    /// Initialize Guardian client connection (call before start_all)
    pub async fn init_guardian(&mut self) -> Result<()> {
        if !self.config.system.guardian.enabled {
            info!("Guardian integration disabled");
            return Ok(());
        }

        info!("Connecting to Guardian security agent...");

        match GuardianClient::connect().await {
            Ok(client) => {
                // Get Guardian status to verify connection
                let mut client_lock = client;
                match client_lock.status().await {
                    Ok(status) => {
                        info!(
                            "Connected to Guardian v{} (uptime: {}s, {} requests processed)",
                            status.version,
                            status.uptime_secs,
                            status.requests_processed
                        );
                        self.guardian = Some(Arc::new(Mutex::new(client_lock)));
                    }
                    Err(e) => {
                        warn!("Guardian connected but status check failed: {}", e);
                        self.guardian = Some(Arc::new(Mutex::new(client_lock)));
                    }
                }
                Ok(())
            }
            Err(e) => {
                if self.config.system.guardian.required {
                    error!("Guardian connection failed (required): {}", e);
                    Err(anyhow::anyhow!("Guardian connection required but failed: {}", e))
                } else {
                    warn!("Guardian connection failed (not required), continuing without: {}", e);
                    Ok(())
                }
            }
        }
    }

    /// Subscribe to supervisor events
    pub fn subscribe(&self) -> broadcast::Receiver<SupervisorEvent> {
        self.events.subscribe()
    }

    /// Start all services in dependency order
    pub async fn start_all(&mut self) -> Result<()> {
        info!("Starting all services");

        // Get services in topological order
        let order = self.dep_graph.topological_order()?;

        for service_name in order {
            // Find the service spec
            let spec = self
                .config
                .services
                .iter()
                .find(|s| s.name == service_name)
                .cloned();

            if let Some(spec) = spec {
                self.start_service(&spec).await?;
            }
        }

        info!("All services started");
        let _ = self.events.send(SupervisorEvent::AllServicesReady);

        Ok(())
    }

    /// Start a single service
    pub async fn start_service(&mut self, spec: &crate::service::ServiceSpec) -> Result<()> {
        let name = spec.name.clone();

        // Check dependencies are running
        for dep in &spec.dependencies {
            let dep_running = self
                .services
                .get(dep)
                .map(|s| s.state == ServiceState::Running)
                .unwrap_or(false);

            if !dep_running {
                error!(
                    "Cannot start {}: dependency {} is not running",
                    name, dep
                );
                return Err(anyhow::anyhow!(
                    "Dependency {} not running",
                    dep
                ));
            }
        }

        // Request capabilities from Guardian (if enabled)
        if self.config.system.guardian.enabled {
            self.request_capabilities(&spec.capabilities, &name).await?;
        }

        // Create and start service
        let mut service = Service::new(spec.clone());
        service.start().await?;

        if let Some(pid) = service.pid {
            let _ = self.events.send(SupervisorEvent::ServiceStarted {
                name: name.clone(),
                pid,
            });
        }

        // Store service
        self.services.insert(name, service);

        Ok(())
    }

    /// Request capabilities from Guardian
    async fn request_capabilities(&self, capabilities: &[String], service_name: &str) -> Result<()> {
        if capabilities.is_empty() {
            return Ok(());
        }

        debug!(
            "Requesting capabilities for {}: {:?}",
            service_name, capabilities
        );

        // If Guardian is not connected, auto-approve (based on config)
        let guardian = match &self.guardian {
            Some(g) => g.clone(),
            None => {
                if self.config.system.guardian.auto_approve_without_guardian {
                    debug!("Guardian not connected, auto-approving capabilities for {}", service_name);
                    return Ok(());
                } else {
                    return Err(anyhow::anyhow!(
                        "Guardian required but not connected, cannot approve capabilities for {}",
                        service_name
                    ));
                }
            }
        };

        // Request each capability from Guardian
        let mut client = guardian.lock().await;
        let mut denied_caps = Vec::new();

        for cap in capabilities {
            let request = CapabilityRequest::new(cap.clone())
                .with_context("service", service_name)
                .with_context("action", "start");

            match client.check_capability_full(request).await {
                Ok(decision) => {
                    match decision.decision {
                        Decision::Allow => {
                            debug!("Guardian approved capability '{}' for {}", cap, service_name);
                        }
                        Decision::Sandbox => {
                            info!(
                                "Guardian approved '{}' for {} with sandbox: {:?}",
                                cap, service_name, decision.sandbox_config
                            );
                            // TODO: Apply sandbox configuration
                        }
                        Decision::Deny => {
                            warn!(
                                "Guardian denied capability '{}' for {}: {}",
                                cap, service_name, decision.reason
                            );
                            denied_caps.push(cap.clone());
                        }
                        Decision::Prompt => {
                            // For now, treat prompt as deny in automated context
                            warn!(
                                "Guardian requires user prompt for '{}' (treating as deny): {}",
                                cap, decision.reason
                            );
                            denied_caps.push(cap.clone());
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Guardian capability check failed for '{}' on {}: {}",
                        cap, service_name, e
                    );
                    // Treat Guardian errors as denied if strict mode
                    if self.config.system.guardian.strict_mode {
                        denied_caps.push(cap.clone());
                    }
                }
            }
        }

        // If any capabilities were denied, fail the service start
        if !denied_caps.is_empty() {
            return Err(anyhow::anyhow!(
                "Guardian denied capabilities for {}: {:?}",
                service_name,
                denied_caps
            ));
        }

        info!("All capabilities approved for {} by Guardian", service_name);
        Ok(())
    }

    /// Stop a service
    pub async fn stop_service(&self, name: &str) -> Result<()> {
        if let Some(mut service) = self.services.get_mut(name) {
            service.stop().await?;
            let _ = self.events.send(SupervisorEvent::ServiceStopped {
                name: name.to_string(),
            });
        }
        Ok(())
    }

    /// Stop all services in reverse dependency order
    pub async fn stop_all(&self) -> Result<()> {
        info!("Stopping all services");
        let _ = self.events.send(SupervisorEvent::ShutdownInitiated);

        // Get reverse topological order
        let order = self.dep_graph.topological_order()?;

        for service_name in order.into_iter().rev() {
            self.stop_service(&service_name).await?;
        }

        let _ = self.events.send(SupervisorEvent::ShutdownComplete);
        info!("All services stopped");

        Ok(())
    }

    /// Main supervisor loop
    pub async fn run(&mut self) -> Result<()> {
        info!("Supervisor entering main loop");

        let mut check_interval = tokio::time::interval(Duration::from_secs(1));
        let mut health_interval = tokio::time::interval(Duration::from_secs(30));

        // Set up signal handlers
        #[cfg(unix)]
        let mut sigterm = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate()
        )?;
        #[cfg(unix)]
        let mut sigint = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::interrupt()
        )?;

        loop {
            tokio::select! {
                // Check service health
                _ = check_interval.tick() => {
                    self.check_services().await;
                }

                // Periodic health checks
                _ = health_interval.tick() => {
                    self.run_health_checks().await;
                }

                // Handle SIGTERM
                _ = sigterm.recv() => {
                    info!("Received SIGTERM, initiating shutdown");
                    break;
                }

                // Handle SIGINT
                _ = sigint.recv() => {
                    info!("Received SIGINT, initiating shutdown");
                    break;
                }

                // Handle shutdown notification
                _ = self.shutdown.notified() => {
                    info!("Shutdown requested");
                    break;
                }
            }
        }

        // Graceful shutdown
        self.stop_all().await?;

        Ok(())
    }

    /// Check service states and restart if needed
    async fn check_services(&mut self) {
        let mut to_restart = Vec::new();

        for mut entry in self.services.iter_mut() {
            let name = entry.key().clone();
            let service = entry.value_mut();

            // Check if process is still alive
            if !service.check_alive().await {
                if service.should_restart() {
                    to_restart.push(name);
                }
            }
        }

        // Restart services that need it
        for name in to_restart {
            if let Some(mut service) = self.services.get_mut(&name) {
                let attempt = service.restart_count + 1;
                let _ = self.events.send(SupervisorEvent::ServiceRestarting {
                    name: name.clone(),
                    attempt,
                });

                if let Err(e) = service.restart().await {
                    error!("Failed to restart service {}: {}", name, e);
                    let _ = self.events.send(SupervisorEvent::ServiceFailed {
                        name,
                        error: e.to_string(),
                    });
                }
            }
        }
    }

    /// Run health checks for services that have them
    async fn run_health_checks(&self) {
        for entry in self.services.iter() {
            let service = entry.value();

            if service.state != ServiceState::Running {
                continue;
            }

            if let Some(ref health_check) = service.spec.health_check {
                // TODO: Implement actual health check execution
                debug!("Running health check for {}", service.spec.name);
            }
        }
    }

    /// Request shutdown
    pub fn request_shutdown(&self) {
        self.shutdown.notify_one();
    }

    /// Get service status
    pub fn get_status(&self, name: &str) -> Option<ServiceState> {
        self.services.get(name).map(|s| s.state)
    }

    /// Get all service statuses
    pub fn get_all_status(&self) -> Vec<(String, ServiceState)> {
        self.services
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().state))
            .collect()
    }
}
