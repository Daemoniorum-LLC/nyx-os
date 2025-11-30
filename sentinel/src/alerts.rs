//! Alert management

use crate::config::AlertConfig;
use crate::metrics::SystemSnapshot;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, warn};

/// Alert severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

/// Alert type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertType {
    HighCpu,
    HighMemory,
    HighDisk,
    HighTemperature,
    HighLoad,
}

/// Alert instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// Alert type
    pub alert_type: AlertType,
    /// Severity
    pub severity: AlertSeverity,
    /// Message
    pub message: String,
    /// Current value
    pub value: f32,
    /// Threshold
    pub threshold: f32,
    /// When the alert was triggered
    pub timestamp: DateTime<Utc>,
    /// Resource name (e.g., disk mount point)
    pub resource: Option<String>,
}

/// Alert manager
pub struct AlertManager {
    config: AlertConfig,
    active_alerts: HashMap<(AlertType, Option<String>), Alert>,
    last_alert_time: HashMap<(AlertType, Option<String>), DateTime<Utc>>,
    alert_history: Vec<Alert>,
}

impl AlertManager {
    /// Create new alert manager
    pub fn new(config: AlertConfig) -> Self {
        Self {
            config,
            active_alerts: HashMap::new(),
            last_alert_time: HashMap::new(),
            alert_history: Vec::new(),
        }
    }

    /// Check snapshot for alerts
    pub fn check(&mut self, snapshot: &SystemSnapshot) -> Vec<Alert> {
        if !self.config.enabled {
            return Vec::new();
        }

        let mut new_alerts = Vec::new();

        // Check CPU
        if let Some(ref cpu) = snapshot.cpu {
            if cpu.usage >= self.config.cpu_threshold {
                if let Some(alert) = self.create_alert(
                    AlertType::HighCpu,
                    None,
                    cpu.usage,
                    self.config.cpu_threshold,
                    format!("CPU usage at {:.1}%", cpu.usage),
                ) {
                    new_alerts.push(alert);
                }
            } else {
                self.clear_alert(AlertType::HighCpu, None);
            }
        }

        // Check memory
        if let Some(ref memory) = snapshot.memory {
            if memory.usage_percent >= self.config.memory_threshold {
                if let Some(alert) = self.create_alert(
                    AlertType::HighMemory,
                    None,
                    memory.usage_percent,
                    self.config.memory_threshold,
                    format!("Memory usage at {:.1}%", memory.usage_percent),
                ) {
                    new_alerts.push(alert);
                }
            } else {
                self.clear_alert(AlertType::HighMemory, None);
            }
        }

        // Check disks
        for disk in &snapshot.disks {
            let key = Some(disk.mount_point.clone());
            if disk.usage_percent >= self.config.disk_threshold {
                if let Some(alert) = self.create_alert(
                    AlertType::HighDisk,
                    key.clone(),
                    disk.usage_percent,
                    self.config.disk_threshold,
                    format!(
                        "Disk {} usage at {:.1}%",
                        disk.mount_point, disk.usage_percent
                    ),
                ) {
                    new_alerts.push(alert);
                }
            } else {
                self.clear_alert(AlertType::HighDisk, key);
            }
        }

        // Check temperatures
        for temp in &snapshot.temperatures {
            let key = Some(temp.label.clone());
            if temp.temperature >= self.config.temp_threshold {
                if let Some(alert) = self.create_alert(
                    AlertType::HighTemperature,
                    key.clone(),
                    temp.temperature,
                    self.config.temp_threshold,
                    format!(
                        "Temperature {} at {:.1}°C",
                        temp.label, temp.temperature
                    ),
                ) {
                    new_alerts.push(alert);
                }
            } else {
                self.clear_alert(AlertType::HighTemperature, key);
            }
        }

        // Check load average (per core)
        let num_cores = snapshot
            .cpu
            .as_ref()
            .map(|c| c.logical_cores)
            .unwrap_or(1) as f64;
        let load_per_core = (snapshot.load.one / num_cores) as f32;

        if load_per_core >= self.config.load_threshold {
            if let Some(alert) = self.create_alert(
                AlertType::HighLoad,
                None,
                load_per_core,
                self.config.load_threshold,
                format!(
                    "Load average {:.2} ({:.2} per core)",
                    snapshot.load.one, load_per_core
                ),
            ) {
                new_alerts.push(alert);
            }
        } else {
            self.clear_alert(AlertType::HighLoad, None);
        }

        new_alerts
    }

    /// Create an alert if not in cooldown
    fn create_alert(
        &mut self,
        alert_type: AlertType,
        resource: Option<String>,
        value: f32,
        threshold: f32,
        message: String,
    ) -> Option<Alert> {
        let key = (alert_type, resource.clone());
        let now = Utc::now();

        // Check cooldown
        if let Some(last) = self.last_alert_time.get(&key) {
            let elapsed = (now - *last).num_seconds();
            if elapsed < self.config.cooldown_secs as i64 {
                debug!("Alert {:?} in cooldown ({} seconds remaining)", alert_type, self.config.cooldown_secs as i64 - elapsed);
                return None;
            }
        }

        let severity = self.determine_severity(alert_type, value, threshold);

        let alert = Alert {
            alert_type,
            severity,
            message,
            value,
            threshold,
            timestamp: now,
            resource,
        };

        warn!("Alert: {} ({:?})", alert.message, severity);

        self.active_alerts.insert(key.clone(), alert.clone());
        self.last_alert_time.insert(key, now);
        self.alert_history.push(alert.clone());

        Some(alert)
    }

    /// Clear an alert
    fn clear_alert(&mut self, alert_type: AlertType, resource: Option<String>) {
        let key = (alert_type, resource);
        if self.active_alerts.remove(&key).is_some() {
            debug!("Alert {:?} cleared", alert_type);
        }
    }

    /// Determine alert severity based on how far over threshold
    fn determine_severity(&self, alert_type: AlertType, value: f32, threshold: f32) -> AlertSeverity {
        let ratio = value / threshold;

        // For temperature, be more aggressive
        if matches!(alert_type, AlertType::HighTemperature) {
            if ratio >= 1.15 {
                AlertSeverity::Critical
            } else if ratio >= 1.05 {
                AlertSeverity::Warning
            } else {
                AlertSeverity::Info
            }
        } else {
            if ratio >= 1.2 {
                AlertSeverity::Critical
            } else if ratio >= 1.1 {
                AlertSeverity::Warning
            } else {
                AlertSeverity::Info
            }
        }
    }

    /// Get active alerts
    pub fn get_active_alerts(&self) -> Vec<&Alert> {
        self.active_alerts.values().collect()
    }

    /// Get alert history
    pub fn get_history(&self, limit: usize) -> Vec<&Alert> {
        self.alert_history
            .iter()
            .rev()
            .take(limit)
            .collect()
    }

    /// Get alert count by severity
    pub fn get_counts(&self) -> AlertCounts {
        let mut counts = AlertCounts::default();

        for alert in self.active_alerts.values() {
            match alert.severity {
                AlertSeverity::Critical => counts.critical += 1,
                AlertSeverity::Warning => counts.warning += 1,
                AlertSeverity::Info => counts.info += 1,
            }
        }

        counts
    }
}

/// Alert counts by severity
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlertCounts {
    pub critical: usize,
    pub warning: usize,
    pub info: usize,
}
