//! Audit logging - tamper-evident security audit trail
//!
//! All security decisions are logged for forensics and compliance.

use crate::config::AuditConfig;
use crate::policy::CapabilityRequest;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Audit event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AuditEvent {
    /// Guardian started
    Started {
        version: String,
        config_hash: String,
    },
    /// Guardian stopped
    Stopped {
        reason: String,
        uptime_secs: u64,
    },
    /// Capability request received
    Request {
        request_id: Uuid,
        request: CapabilityRequest,
    },
    /// Security decision made
    Decision {
        request: CapabilityRequest,
        decision: String,
        reason: String,
        user_approved: bool,
    },
    /// Policy violation detected
    Violation {
        request: CapabilityRequest,
        violation_type: String,
        severity: ViolationSeverity,
    },
    /// Anomaly detected
    Anomaly {
        process_path: String,
        anomaly_type: String,
        score: f32,
        explanation: String,
    },
    /// Configuration changed
    ConfigChanged {
        component: String,
        change_type: String,
        old_hash: String,
        new_hash: String,
    },
    /// User override applied
    Override {
        request: CapabilityRequest,
        original_decision: String,
        override_decision: String,
        reason: String,
    },
    /// Pattern learned
    PatternLearned {
        process_path: String,
        capability: String,
        count: u64,
    },
    /// Alert triggered
    Alert {
        alert_type: AlertType,
        message: String,
        context: std::collections::HashMap<String, String>,
    },
}

/// Violation severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ViolationSeverity {
    /// Informational - logged but not blocked
    Info,
    /// Low severity - may be blocked depending on policy
    Low,
    /// Medium severity - typically blocked
    Medium,
    /// High severity - always blocked
    High,
    /// Critical severity - blocked and triggers alert
    Critical,
}

/// Alert types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertType {
    /// Multiple failed requests from same process
    RepeatedDenials,
    /// Suspicious pattern detected
    SuspiciousActivity,
    /// Potential attack detected
    PotentialAttack,
    /// Policy integrity violation
    PolicyTamper,
    /// Resource exhaustion attempt
    ResourceExhaustion,
    /// Privilege escalation attempt
    PrivilegeEscalation,
}

/// Serialized audit entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Entry sequence number
    pub seq: u64,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Machine ID
    pub machine_id: String,
    /// Session ID
    pub session_id: Uuid,
    /// Event data
    pub event: AuditEvent,
    /// Previous entry hash (chain integrity)
    pub prev_hash: String,
    /// Entry hash
    pub hash: String,
}

/// Audit logger
pub struct AuditLogger {
    /// Whether logging is enabled
    enabled: bool,
    /// Output path
    output_path: PathBuf,
    /// Rotation settings
    rotate_size_mb: u64,
    /// Retention days
    retention_days: u32,
    /// Current sequence number
    seq: AtomicU64,
    /// Session ID
    session_id: Uuid,
    /// Machine ID
    machine_id: String,
    /// Last entry hash (for chaining)
    last_hash: Mutex<String>,
    /// Log writer
    writer: Mutex<Option<BufWriter<File>>>,
    /// Async channel for background writing
    tx: Option<mpsc::UnboundedSender<AuditEntry>>,
}

impl AuditLogger {
    /// Create a new audit logger
    pub fn new(config: &AuditConfig) -> Result<Self> {
        let session_id = Uuid::new_v4();
        let machine_id = get_machine_id();

        let writer = if config.enabled {
            // Ensure directory exists
            if let Some(parent) = config.output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&config.output_path)?;

            Some(BufWriter::new(file))
        } else {
            None
        };

        Ok(Self {
            enabled: config.enabled,
            output_path: config.output_path.clone(),
            rotate_size_mb: config.rotate_size_mb,
            retention_days: config.retention_days,
            seq: AtomicU64::new(0),
            session_id,
            machine_id,
            last_hash: Mutex::new(String::from("genesis")),
            writer: Mutex::new(writer),
            tx: None,
        })
    }

    /// Log an event
    pub fn log(&self, event: AuditEvent) {
        if !self.enabled {
            return;
        }

        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let timestamp = Utc::now();

        // Get previous hash for chaining
        let prev_hash = {
            let guard = self.last_hash.lock().unwrap();
            guard.clone()
        };

        // Create entry
        let mut entry = AuditEntry {
            seq,
            timestamp,
            machine_id: self.machine_id.clone(),
            session_id: self.session_id,
            event,
            prev_hash,
            hash: String::new(),
        };

        // Compute hash
        entry.hash = compute_entry_hash(&entry);

        // Update last hash
        {
            let mut guard = self.last_hash.lock().unwrap();
            *guard = entry.hash.clone();
        }

        // Write entry
        self.write_entry(&entry);

        // Check for rotation
        self.maybe_rotate();
    }

    /// Log a capability request
    pub fn log_request(&self, request: &CapabilityRequest) {
        let request_id = Uuid::new_v4();
        self.log(AuditEvent::Request {
            request_id,
            request: request.clone(),
        });
    }

    /// Log a decision
    pub fn log_decision(
        &self,
        request: &CapabilityRequest,
        decision: &str,
        reason: &str,
        user_approved: bool,
    ) {
        self.log(AuditEvent::Decision {
            request: request.clone(),
            decision: decision.to_string(),
            reason: reason.to_string(),
            user_approved,
        });
    }

    /// Log a violation
    pub fn log_violation(
        &self,
        request: &CapabilityRequest,
        violation_type: &str,
        severity: ViolationSeverity,
    ) {
        self.log(AuditEvent::Violation {
            request: request.clone(),
            violation_type: violation_type.to_string(),
            severity,
        });

        // Critical violations trigger immediate alert
        if severity == ViolationSeverity::Critical {
            self.log(AuditEvent::Alert {
                alert_type: AlertType::SuspiciousActivity,
                message: format!("Critical violation: {}", violation_type),
                context: std::collections::HashMap::new(),
            });
        }
    }

    /// Log an anomaly
    pub fn log_anomaly(&self, process_path: &str, anomaly_type: &str, score: f32, explanation: &str) {
        self.log(AuditEvent::Anomaly {
            process_path: process_path.to_string(),
            anomaly_type: anomaly_type.to_string(),
            score,
            explanation: explanation.to_string(),
        });
    }

    /// Log Guardian startup
    pub fn log_started(&self, version: &str, config: &crate::config::GuardianConfig) {
        let config_hash = compute_config_hash(config);
        self.log(AuditEvent::Started {
            version: version.to_string(),
            config_hash,
        });
    }

    /// Log Guardian shutdown
    pub fn log_stopped(&self, reason: &str, uptime_secs: u64) {
        self.log(AuditEvent::Stopped {
            reason: reason.to_string(),
            uptime_secs,
        });
    }

    fn write_entry(&self, entry: &AuditEntry) {
        let mut guard = self.writer.lock().unwrap();
        if let Some(ref mut writer) = *guard {
            match serde_json::to_string(entry) {
                Ok(json) => {
                    if let Err(e) = writeln!(writer, "{}", json) {
                        error!("Failed to write audit entry: {}", e);
                    }
                    if let Err(e) = writer.flush() {
                        error!("Failed to flush audit log: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to serialize audit entry: {}", e);
                }
            }
        }
    }

    fn maybe_rotate(&self) {
        // Check file size
        if let Ok(metadata) = std::fs::metadata(&self.output_path) {
            let size_mb = metadata.len() / (1024 * 1024);
            if size_mb >= self.rotate_size_mb {
                self.rotate();
            }
        }
    }

    fn rotate(&self) {
        let mut guard = self.writer.lock().unwrap();

        // Close current writer
        *guard = None;

        // Rename current file
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let rotated_name = format!(
            "{}.{}",
            self.output_path.display(),
            timestamp
        );

        if let Err(e) = std::fs::rename(&self.output_path, &rotated_name) {
            error!("Failed to rotate audit log: {}", e);
            return;
        }

        info!("Rotated audit log to {}", rotated_name);

        // Open new file
        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.output_path)
        {
            Ok(file) => {
                *guard = Some(BufWriter::new(file));
            }
            Err(e) => {
                error!("Failed to create new audit log: {}", e);
            }
        }

        // Clean up old logs
        self.cleanup_old_logs();
    }

    fn cleanup_old_logs(&self) {
        let retention_secs = self.retention_days as i64 * 24 * 60 * 60;
        let cutoff = Utc::now().timestamp() - retention_secs;

        if let Some(parent) = self.output_path.parent() {
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |e| e == "log") {
                        if let Ok(metadata) = entry.metadata() {
                            if let Ok(modified) = metadata.modified() {
                                if let Ok(duration) = modified.elapsed() {
                                    if duration.as_secs() > retention_secs as u64 {
                                        if let Err(e) = std::fs::remove_file(&path) {
                                            warn!("Failed to remove old log {}: {}", path.display(), e);
                                        } else {
                                            info!("Removed old audit log: {}", path.display());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Verify audit log integrity
    pub fn verify_integrity(&self) -> Result<IntegrityReport> {
        let file = File::open(&self.output_path)?;
        let reader = std::io::BufReader::new(file);

        let mut entries_checked = 0u64;
        let mut errors = Vec::new();
        let mut prev_hash = String::from("genesis");

        use std::io::BufRead;
        for (line_num, line) in reader.lines().enumerate() {
            let line = line?;
            if line.is_empty() {
                continue;
            }

            match serde_json::from_str::<AuditEntry>(&line) {
                Ok(entry) => {
                    // Verify chain
                    if entry.prev_hash != prev_hash {
                        errors.push(IntegrityError::ChainBroken {
                            line: line_num,
                            expected: prev_hash.clone(),
                            found: entry.prev_hash.clone(),
                        });
                    }

                    // Verify hash
                    let computed_hash = compute_entry_hash(&entry);
                    if entry.hash != computed_hash {
                        errors.push(IntegrityError::HashMismatch {
                            line: line_num,
                            expected: computed_hash,
                            found: entry.hash.clone(),
                        });
                    }

                    prev_hash = entry.hash;
                    entries_checked += 1;
                }
                Err(e) => {
                    errors.push(IntegrityError::ParseError {
                        line: line_num,
                        error: e.to_string(),
                    });
                }
            }
        }

        Ok(IntegrityReport {
            entries_checked,
            errors,
        })
    }
}

/// Integrity verification report
#[derive(Debug)]
pub struct IntegrityReport {
    pub entries_checked: u64,
    pub errors: Vec<IntegrityError>,
}

impl IntegrityReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Integrity errors
#[derive(Debug)]
pub enum IntegrityError {
    ChainBroken {
        line: usize,
        expected: String,
        found: String,
    },
    HashMismatch {
        line: usize,
        expected: String,
        found: String,
    },
    ParseError {
        line: usize,
        error: String,
    },
}

fn get_machine_id() -> String {
    // Try to read machine-id
    if let Ok(id) = std::fs::read_to_string("/etc/machine-id") {
        return id.trim().to_string();
    }

    // Fall back to hostname
    if let Ok(hostname) = std::fs::read_to_string("/etc/hostname") {
        return hostname.trim().to_string();
    }

    // Last resort
    "unknown".to_string()
}

fn compute_entry_hash(entry: &AuditEntry) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();

    // Hash relevant fields (excluding the hash itself)
    entry.seq.hash(&mut hasher);
    entry.timestamp.to_rfc3339().hash(&mut hasher);
    entry.machine_id.hash(&mut hasher);
    entry.session_id.to_string().hash(&mut hasher);
    format!("{:?}", entry.event).hash(&mut hasher);
    entry.prev_hash.hash(&mut hasher);

    format!("{:016x}", hasher.finish())
}

fn compute_config_hash(config: &crate::config::GuardianConfig) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    format!("{:?}", config).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_audit_logging() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("audit.log");

        let config = AuditConfig {
            enabled: true,
            output_path: log_path.clone(),
            rotate_size_mb: 100,
            retention_days: 7,
        };

        let logger = AuditLogger::new(&config).unwrap();

        // Log some events
        logger.log(AuditEvent::Started {
            version: "0.1.0".into(),
            config_hash: "abc123".into(),
        });

        let request = CapabilityRequest {
            pid: 1234,
            process_path: "/usr/bin/test".into(),
            user: "testuser".into(),
            capability: "filesystem:read".into(),
            resource: Some("/home/testuser".into()),
            context: std::collections::HashMap::new(),
        };

        logger.log_request(&request);
        logger.log_decision(&request, "Allow", "Trusted app", false);

        // Verify file exists and has content
        assert!(log_path.exists());
        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("Started"));
        assert!(content.contains("Request"));
        assert!(content.contains("Decision"));
    }

    #[test]
    fn test_integrity_verification() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("audit.log");

        let config = AuditConfig {
            enabled: true,
            output_path: log_path.clone(),
            rotate_size_mb: 100,
            retention_days: 7,
        };

        let logger = AuditLogger::new(&config).unwrap();

        // Log events
        for i in 0..10 {
            logger.log(AuditEvent::Alert {
                alert_type: AlertType::SuspiciousActivity,
                message: format!("Test alert {}", i),
                context: std::collections::HashMap::new(),
            });
        }

        // Verify integrity
        let report = logger.verify_integrity().unwrap();
        assert!(report.is_valid());
        assert_eq!(report.entries_checked, 10);
    }
}
