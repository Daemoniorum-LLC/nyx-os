//! Intent analyzer - AI-powered intent understanding
//!
//! This module analyzes capability requests to understand the underlying intent.
//! Instead of just asking "does this app want filesystem access?", we ask
//! "is this app trying to save a file, read system config, or exfiltrate data?"

use crate::config::{IntentConfig, IntentPattern, RiskLevel};
use crate::policy::CapabilityRequest;
use anyhow::Result;
use std::collections::HashMap;
use tracing::debug;

/// Analyzed intent
#[derive(Debug, Clone)]
pub struct AnalyzedIntent {
    /// Primary intent detected
    pub primary_intent: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Risk level
    pub risk_level: RiskLevel,
    /// Explanation
    pub explanation: String,
    /// Secondary intents
    pub secondary_intents: Vec<String>,
    /// Suspicious indicators
    pub suspicious_indicators: Vec<String>,
}

/// Intent analyzer
pub struct IntentAnalyzer {
    /// Whether analysis is enabled
    enabled: bool,
    /// Known intent patterns
    patterns: Vec<IntentMatcher>,
    /// Model name for AI analysis
    model: String,
}

struct IntentMatcher {
    name: String,
    description: String,
    capability_patterns: Vec<regex::Regex>,
    risk_level: RiskLevel,
}

impl IntentAnalyzer {
    /// Create a new intent analyzer
    pub fn new(config: &IntentConfig) -> Result<Self> {
        let patterns = config
            .known_intents
            .iter()
            .filter_map(|p| compile_intent_pattern(p).ok())
            .collect();

        Ok(Self {
            enabled: config.enabled,
            patterns,
            model: config.model.clone(),
        })
    }

    /// Analyze intent from a capability request
    pub async fn analyze(&self, request: &CapabilityRequest) -> AnalyzedIntent {
        if !self.enabled {
            return AnalyzedIntent {
                primary_intent: "unknown".into(),
                confidence: 0.0,
                risk_level: RiskLevel::Low,
                explanation: "Intent analysis disabled".into(),
                secondary_intents: vec![],
                suspicious_indicators: vec![],
            };
        }

        debug!("Analyzing intent for: {:?}", request);

        // First, try pattern matching
        if let Some(matched) = self.match_patterns(request) {
            return matched;
        }

        // Then, try heuristic analysis
        self.heuristic_analysis(request)
    }

    fn match_patterns(&self, request: &CapabilityRequest) -> Option<AnalyzedIntent> {
        for pattern in &self.patterns {
            for cap_pattern in &pattern.capability_patterns {
                if cap_pattern.is_match(&request.capability) {
                    return Some(AnalyzedIntent {
                        primary_intent: pattern.name.clone(),
                        confidence: 0.9,
                        risk_level: pattern.risk_level,
                        explanation: pattern.description.clone(),
                        secondary_intents: vec![],
                        suspicious_indicators: vec![],
                    });
                }
            }
        }
        None
    }

    fn heuristic_analysis(&self, request: &CapabilityRequest) -> AnalyzedIntent {
        let mut suspicious_indicators = Vec::new();
        let mut risk_level = RiskLevel::Low;
        let mut intent = "general_operation".to_string();
        let mut explanation = String::new();

        // Analyze capability type
        let cap = &request.capability;

        // File system intents
        if cap.contains("filesystem") || cap.contains("file") {
            if let Some(ref resource) = request.resource {
                if resource.contains("/etc") || resource.contains("/sys") {
                    intent = "system_configuration".into();
                    explanation = "Accessing system configuration files".into();
                    risk_level = RiskLevel::Medium;
                } else if resource.contains("/home") || resource.contains("~") {
                    intent = "user_data_access".into();
                    explanation = "Accessing user home directory".into();
                } else if resource.contains("/tmp") {
                    intent = "temporary_storage".into();
                    explanation = "Using temporary storage".into();
                } else if resource.contains("..") {
                    intent = "path_traversal".into();
                    explanation = "Potential path traversal detected".into();
                    suspicious_indicators.push("Path contains '..'".into());
                    risk_level = RiskLevel::High;
                }
            }
        }

        // Network intents
        if cap.contains("network") {
            intent = "network_communication".into();
            explanation = "Requesting network access".into();
            risk_level = RiskLevel::Medium;

            // Check for suspicious patterns
            if let Some(ref resource) = request.resource {
                if resource.contains(":22") || resource.contains(":23") {
                    suspicious_indicators.push("SSH/Telnet port access".into());
                }
                if resource.contains("0.0.0.0") || resource.contains("*") {
                    suspicious_indicators.push("Wildcard network binding".into());
                    risk_level = RiskLevel::High;
                }
            }
        }

        // Process intents
        if cap.contains("process") || cap.contains("exec") {
            intent = "process_execution".into();
            explanation = "Executing or managing processes".into();
            risk_level = RiskLevel::Medium;

            if cap.contains("kill") {
                intent = "process_termination".into();
                risk_level = RiskLevel::High;
            }
        }

        // Hardware intents
        if cap.contains("gpu") || cap.contains("tensor") {
            intent = "ai_computation".into();
            explanation = "AI/ML workload".into();
        }

        if cap.contains("camera") || cap.contains("microphone") {
            intent = "media_capture".into();
            explanation = "Accessing camera or microphone".into();
            risk_level = RiskLevel::High;
            suspicious_indicators.push("Sensor access requested".into());
        }

        // Check for suspicious process paths
        if request.process_path.contains("/tmp")
            || request.process_path.contains("/var/tmp")
            || request.process_path.contains("/dev/shm")
        {
            suspicious_indicators.push("Process running from temporary directory".into());
            risk_level = RiskLevel::High;
        }

        AnalyzedIntent {
            primary_intent: intent,
            confidence: 0.7,
            risk_level,
            explanation,
            secondary_intents: vec![],
            suspicious_indicators,
        }
    }

    /// Check if AI-based analysis should be used
    pub fn should_use_ai(&self, heuristic_result: &AnalyzedIntent) -> bool {
        // Use AI for low-confidence or high-risk cases
        heuristic_result.confidence < 0.5 || matches!(heuristic_result.risk_level, RiskLevel::High | RiskLevel::Critical)
    }

    /// Perform AI-based intent analysis (requires Infernum)
    #[cfg(feature = "infernum")]
    pub async fn ai_analyze(&self, request: &CapabilityRequest) -> Result<AnalyzedIntent> {
        // This would call Malphas/Abaddon for inference
        // For now, just return heuristic result
        Ok(self.heuristic_analysis(request))
    }
}

fn compile_intent_pattern(pattern: &IntentPattern) -> Result<IntentMatcher> {
    let capability_patterns = pattern
        .capability_patterns
        .iter()
        .map(|p| {
            let escaped = regex::escape(p);
            let regex_pattern = escaped.replace(r"\*", ".*");
            regex::Regex::new(&format!("^{}$", regex_pattern))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(IntentMatcher {
        name: pattern.name.clone(),
        description: pattern.description.clone(),
        capability_patterns,
        risk_level: pattern.risk_level,
    })
}

/// Common intent categories
pub mod intents {
    pub const FILE_SAVE: &str = "file_save";
    pub const FILE_READ: &str = "file_read";
    pub const SYSTEM_CONFIG: &str = "system_configuration";
    pub const NETWORK_CLIENT: &str = "network_client";
    pub const NETWORK_SERVER: &str = "network_server";
    pub const PROCESS_SPAWN: &str = "process_spawn";
    pub const AI_INFERENCE: &str = "ai_inference";
    pub const MEDIA_CAPTURE: &str = "media_capture";
    pub const DATA_EXFILTRATION: &str = "data_exfiltration";
    pub const PRIVILEGE_ESCALATION: &str = "privilege_escalation";
}
