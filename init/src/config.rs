//! Configuration loading from Grimoire

use crate::service::ServiceSpec;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info};

/// Init system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitConfig {
    /// System configuration
    #[serde(default)]
    pub system: SystemConfig,

    /// Services to manage
    #[serde(default)]
    pub services: Vec<ServiceSpec>,

    /// Boot targets (like systemd targets)
    #[serde(default)]
    pub targets: Vec<BootTarget>,
}

/// System-wide configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// Hostname
    #[serde(default = "default_hostname")]
    pub hostname: String,

    /// Default target to boot into
    #[serde(default = "default_target")]
    pub default_target: String,

    /// Enable watchdog
    #[serde(default)]
    pub watchdog_enabled: bool,

    /// Watchdog timeout in seconds
    #[serde(default = "default_watchdog_timeout")]
    pub watchdog_timeout_sec: u32,

    /// Guardian integration
    #[serde(default)]
    pub guardian: GuardianConfig,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            hostname: default_hostname(),
            default_target: default_target(),
            watchdog_enabled: false,
            watchdog_timeout_sec: default_watchdog_timeout(),
            guardian: GuardianConfig::default(),
        }
    }
}

/// Guardian security integration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GuardianConfig {
    /// Enable Guardian for capability approval
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Guardian socket path
    #[serde(default = "default_guardian_socket")]
    pub socket: String,

    /// Default policy for unapproved requests
    #[serde(default)]
    pub default_policy: SecurityPolicy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SecurityPolicy {
    /// Deny by default
    #[default]
    Deny,
    /// Allow with audit
    AllowWithAudit,
    /// Prompt user
    Prompt,
}

/// Boot target (group of services)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootTarget {
    /// Target name
    pub name: String,

    /// Description
    #[serde(default)]
    pub description: String,

    /// Services in this target
    pub services: Vec<String>,

    /// Targets this depends on
    #[serde(default)]
    pub requires: Vec<String>,
}

fn default_hostname() -> String {
    "daemon".into()
}

fn default_target() -> String {
    "multi-user".into()
}

fn default_watchdog_timeout() -> u32 {
    30
}

fn default_true() -> bool {
    true
}

fn default_guardian_socket() -> String {
    "/run/guardian/guardian.sock".into()
}

/// Load configuration from Grimoire directory
pub async fn load_config(config_dir: &Path) -> Result<InitConfig> {
    info!("Loading configuration from {}", config_dir.display());

    let mut config = InitConfig {
        system: SystemConfig::default(),
        services: Vec::new(),
        targets: Vec::new(),
    };

    // Load system config
    let system_config_path = config_dir.join("init.yaml");
    if system_config_path.exists() {
        let contents = tokio::fs::read_to_string(&system_config_path)
            .await
            .context("Failed to read init.yaml")?;
        config.system = serde_yaml::from_str(&contents)
            .context("Failed to parse init.yaml")?;
        debug!("Loaded system config from init.yaml");
    }

    // Load service definitions
    let services_dir = config_dir.join("services");
    if services_dir.exists() {
        let mut entries = tokio::fs::read_dir(&services_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "yaml" || e == "yml") {
                let contents = tokio::fs::read_to_string(&path)
                    .await
                    .with_context(|| format!("Failed to read {}", path.display()))?;

                let service: ServiceSpec = serde_yaml::from_str(&contents)
                    .with_context(|| format!("Failed to parse {}", path.display()))?;

                debug!("Loaded service: {}", service.name);
                config.services.push(service);
            }
        }
    }

    // Load boot targets
    let targets_path = config_dir.join("targets.yaml");
    if targets_path.exists() {
        let contents = tokio::fs::read_to_string(&targets_path)
            .await
            .context("Failed to read targets.yaml")?;
        config.targets = serde_yaml::from_str(&contents)
            .context("Failed to parse targets.yaml")?;
        debug!("Loaded {} boot targets", config.targets.len());
    }

    // Add default services if none specified
    if config.services.is_empty() {
        info!("No services configured, adding defaults");
        config.services = default_services();
    }

    info!(
        "Configuration loaded: {} services, {} targets",
        config.services.len(),
        config.targets.len()
    );

    Ok(config)
}

/// Default services for a minimal boot
fn default_services() -> Vec<ServiceSpec> {
    vec![
        ServiceSpec {
            name: "guardian".into(),
            description: "Security agent - AI-powered capability approval".into(),
            exec: "/usr/lib/nyx/guardian".into(),
            service_type: crate::service::ServiceType::Agent,
            restart: crate::service::RestartPolicy::Always,
            dependencies: vec![],
            capabilities: vec!["cap:full".into()],
            environment: Default::default(),
            ..Default::default()
        },
        ServiceSpec {
            name: "malphas".into(),
            description: "Model orchestration and routing".into(),
            exec: "/usr/lib/infernum/malphas".into(),
            service_type: crate::service::ServiceType::Agent,
            restart: crate::service::RestartPolicy::Always,
            dependencies: vec!["guardian".into()],
            capabilities: vec!["cap:inference".into(), "cap:gpu".into()],
            environment: Default::default(),
            ..Default::default()
        },
        ServiceSpec {
            name: "abaddon".into(),
            description: "Local inference engine".into(),
            exec: "/usr/lib/infernum/abaddon".into(),
            service_type: crate::service::ServiceType::Agent,
            restart: crate::service::RestartPolicy::Always,
            dependencies: vec!["guardian".into(), "malphas".into()],
            capabilities: vec!["cap:inference".into(), "cap:gpu".into(), "cap:tensor".into()],
            environment: Default::default(),
            ..Default::default()
        },
        ServiceSpec {
            name: "archon".into(),
            description: "File management agent".into(),
            exec: "/usr/lib/nyx/archon".into(),
            service_type: crate::service::ServiceType::Agent,
            restart: crate::service::RestartPolicy::Always,
            dependencies: vec!["guardian".into()],
            capabilities: vec!["cap:filesystem".into()],
            environment: Default::default(),
            ..Default::default()
        },
        ServiceSpec {
            name: "arachne".into(),
            description: "Network management agent".into(),
            exec: "/usr/lib/nyx/arachne".into(),
            service_type: crate::service::ServiceType::Agent,
            restart: crate::service::RestartPolicy::Always,
            dependencies: vec!["guardian".into()],
            capabilities: vec!["cap:network".into()],
            environment: Default::default(),
            ..Default::default()
        },
    ]
}
