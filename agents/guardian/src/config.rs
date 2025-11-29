//! Guardian configuration

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

/// Guardian configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianConfig {
    /// Policy configuration
    #[serde(default)]
    pub policies: PolicyConfig,

    /// Intent analysis configuration
    #[serde(default)]
    pub intent: IntentConfig,

    /// Pattern learning configuration
    #[serde(default)]
    pub patterns: PatternConfig,

    /// Audit configuration
    #[serde(default)]
    pub audit: AuditConfig,
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            policies: PolicyConfig::default(),
            intent: IntentConfig::default(),
            patterns: PatternConfig::default(),
            audit: AuditConfig::default(),
        }
    }
}

/// Policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    /// Default policy for unknown requests
    #[serde(default)]
    pub default_policy: DefaultPolicy,

    /// Trusted applications (auto-approve)
    #[serde(default)]
    pub trusted_apps: Vec<TrustedApp>,

    /// Capability rules
    #[serde(default)]
    pub capability_rules: Vec<CapabilityRule>,

    /// Sandbox configurations
    #[serde(default)]
    pub sandboxes: Vec<SandboxProfile>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            default_policy: DefaultPolicy::Prompt,
            trusted_apps: vec![
                TrustedApp {
                    name: "nyx-init".into(),
                    path_pattern: "/usr/lib/nyx/init".into(),
                    capabilities: vec!["*".into()],
                },
                TrustedApp {
                    name: "guardian".into(),
                    path_pattern: "/usr/lib/nyx/guardian".into(),
                    capabilities: vec!["*".into()],
                },
            ],
            capability_rules: Vec::new(),
            sandboxes: default_sandboxes(),
        }
    }
}

/// Default policy for unknown requests
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DefaultPolicy {
    /// Allow all requests
    Allow,
    /// Deny all requests
    Deny,
    /// Prompt user for decision
    #[default]
    Prompt,
    /// Allow with audit logging
    AllowWithAudit,
}

/// Trusted application definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedApp {
    /// Application name
    pub name: String,
    /// Path pattern (glob)
    pub path_pattern: String,
    /// Allowed capabilities
    pub capabilities: Vec<String>,
}

/// Capability rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRule {
    /// Rule name
    pub name: String,
    /// Capability pattern
    pub capability: String,
    /// Conditions for this rule
    pub conditions: Vec<RuleCondition>,
    /// Action to take
    pub action: RuleAction,
}

/// Rule condition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleCondition {
    /// App matches pattern
    AppPath(String),
    /// User matches
    User(String),
    /// Time window
    TimeWindow { start: String, end: String },
    /// Resource matches pattern
    ResourcePath(String),
    /// Intent matches
    Intent(String),
}

/// Rule action
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    Allow,
    Deny,
    Prompt,
    AllowOnce,
    DenyWithMessage,
    Sandbox,
}

/// Sandbox profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxProfile {
    /// Profile name
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// Allow network access
    #[serde(default)]
    pub network: bool,
    /// Allowed filesystem paths
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    /// Read-only paths
    #[serde(default)]
    pub readonly_paths: Vec<String>,
    /// Allow GPU access
    #[serde(default)]
    pub gpu: bool,
    /// Allow audio access
    #[serde(default)]
    pub audio: bool,
    /// Allow camera access
    #[serde(default)]
    pub camera: bool,
}

fn default_sandboxes() -> Vec<SandboxProfile> {
    vec![
        SandboxProfile {
            name: "strict".into(),
            description: "Maximum isolation, no external access".into(),
            network: false,
            allowed_paths: vec!["/tmp".into()],
            readonly_paths: vec!["/usr".into(), "/lib".into()],
            gpu: false,
            audio: false,
            camera: false,
        },
        SandboxProfile {
            name: "browser".into(),
            description: "Web browser sandbox".into(),
            network: true,
            allowed_paths: vec!["~/Downloads".into(), "~/.cache".into()],
            readonly_paths: vec!["/usr".into(), "/lib".into()],
            gpu: true,
            audio: true,
            camera: false, // Prompt for camera
        },
        SandboxProfile {
            name: "gaming".into(),
            description: "Game sandbox".into(),
            network: true,
            allowed_paths: vec!["~/.local/share/Steam".into(), "~/.steam".into()],
            readonly_paths: vec!["/usr".into(), "/lib".into()],
            gpu: true,
            audio: true,
            camera: false,
        },
        SandboxProfile {
            name: "development".into(),
            description: "Development environment".into(),
            network: true,
            allowed_paths: vec!["~/projects".into(), "~/.cargo".into(), "~/.rustup".into()],
            readonly_paths: vec!["/usr".into(), "/lib".into()],
            gpu: true,
            audio: false,
            camera: false,
        },
    ]
}

/// Intent analysis configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntentConfig {
    /// Enable AI-based intent analysis
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Model to use for intent analysis
    #[serde(default = "default_intent_model")]
    pub model: String,

    /// Known intent patterns
    #[serde(default)]
    pub known_intents: Vec<IntentPattern>,
}

fn default_true() -> bool {
    true
}

fn default_intent_model() -> String {
    "guardian-intent".into()
}

/// Known intent pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentPattern {
    /// Intent name
    pub name: String,
    /// Description
    pub description: String,
    /// Capability patterns that indicate this intent
    pub capability_patterns: Vec<String>,
    /// Risk level
    #[serde(default)]
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    #[default]
    Low,
    Medium,
    High,
    Critical,
}

/// Pattern learning configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PatternConfig {
    /// Enable pattern learning
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Pattern database path
    #[serde(default = "default_pattern_db")]
    pub database_path: String,

    /// Anomaly detection threshold (0.0 - 1.0)
    #[serde(default = "default_anomaly_threshold")]
    pub anomaly_threshold: f32,

    /// Learning rate
    #[serde(default = "default_learning_rate")]
    pub learning_rate: f32,
}

fn default_pattern_db() -> String {
    "/var/lib/guardian/patterns.db".into()
}

fn default_anomaly_threshold() -> f32 {
    0.8
}

fn default_learning_rate() -> f32 {
    0.1
}

/// Audit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Enable audit logging
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Audit log path
    #[serde(default = "default_audit_path")]
    pub output_path: PathBuf,

    /// Log retention days
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,

    /// Log rotation size in MB
    #[serde(default = "default_rotate_size")]
    pub rotate_size_mb: u64,

    /// Log security decisions
    #[serde(default = "default_true")]
    pub log_decisions: bool,

    /// Log capability usage
    #[serde(default = "default_true")]
    pub log_capability_usage: bool,

    /// Real-time alerts for critical events
    #[serde(default)]
    pub alerts: AlertConfig,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            output_path: default_audit_path(),
            retention_days: default_retention_days(),
            rotate_size_mb: default_rotate_size(),
            log_decisions: true,
            log_capability_usage: true,
            alerts: AlertConfig::default(),
        }
    }
}

fn default_audit_path() -> PathBuf {
    PathBuf::from("/var/log/guardian/audit.log")
}

fn default_rotate_size() -> u64 {
    100 // 100 MB
}

fn default_retention_days() -> u32 {
    90
}

/// Alert configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AlertConfig {
    /// Enable alerts
    #[serde(default)]
    pub enabled: bool,

    /// Alert on denied requests
    #[serde(default)]
    pub on_deny: bool,

    /// Alert on anomalies
    #[serde(default = "default_true")]
    pub on_anomaly: bool,

    /// Alert on critical capability requests
    #[serde(default = "default_true")]
    pub on_critical_capability: bool,
}

/// Load configuration from file
pub async fn load_config(path: &Path) -> Result<GuardianConfig> {
    if path.exists() {
        let contents = tokio::fs::read_to_string(path).await?;
        let config: GuardianConfig = serde_yaml::from_str(&contents)?;
        info!("Loaded configuration from {}", path.display());
        Ok(config)
    } else {
        info!("No configuration file found, using defaults");
        Ok(GuardianConfig::default())
    }
}
