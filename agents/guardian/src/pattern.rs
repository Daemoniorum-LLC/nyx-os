//! Pattern learner - learns normal behavior and detects anomalies
//!
//! Guardian learns the normal patterns of capability usage and flags anomalies.

use crate::config::PatternConfig;
use crate::policy::CapabilityRequest;
use anyhow::Result;
use dashmap::DashMap;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tracing::{debug, warn};

/// Pattern learning result
#[derive(Debug, Clone)]
pub struct PatternAnalysis {
    /// Whether this is a known pattern
    pub is_known: bool,
    /// Anomaly score (0.0 = normal, 1.0 = very anomalous)
    pub anomaly_score: f32,
    /// Similar patterns seen before
    pub similar_patterns: Vec<String>,
    /// Explanation
    pub explanation: String,
}

/// Pattern learner
pub struct PatternLearner {
    /// Whether learning is enabled
    enabled: bool,
    /// Anomaly threshold
    anomaly_threshold: f32,
    /// Learning rate
    learning_rate: f32,
    /// App-to-capability frequency map
    app_capabilities: DashMap<String, CapabilityProfile>,
    /// Time-based patterns
    time_patterns: DashMap<String, TimePattern>,
    /// Resource access patterns
    resource_patterns: DashMap<String, ResourceProfile>,
}

/// Capability usage profile for an app
#[derive(Debug, Clone, Default)]
struct CapabilityProfile {
    /// Capability -> count
    capabilities: HashMap<String, u64>,
    /// Total requests
    total_requests: u64,
    /// First seen
    first_seen: Option<SystemTime>,
    /// Last seen
    last_seen: Option<SystemTime>,
}

/// Time-based access pattern
#[derive(Debug, Clone, Default)]
struct TimePattern {
    /// Hour -> count (0-23)
    hourly_distribution: [u64; 24],
    /// Day of week -> count (0-6, Sunday = 0)
    weekly_distribution: [u64; 7],
}

/// Resource access profile
#[derive(Debug, Clone, Default)]
struct ResourceProfile {
    /// Resource path -> count
    resources: HashMap<String, u64>,
}

impl PatternLearner {
    /// Create a new pattern learner
    pub fn new(config: &PatternConfig) -> Result<Self> {
        Ok(Self {
            enabled: config.enabled,
            anomaly_threshold: config.anomaly_threshold,
            learning_rate: config.learning_rate,
            app_capabilities: DashMap::new(),
            time_patterns: DashMap::new(),
            resource_patterns: DashMap::new(),
        })
    }

    /// Analyze a request against learned patterns
    pub fn analyze(&self, request: &CapabilityRequest) -> PatternAnalysis {
        if !self.enabled {
            return PatternAnalysis {
                is_known: false,
                anomaly_score: 0.0,
                similar_patterns: vec![],
                explanation: "Pattern learning disabled".into(),
            };
        }

        let mut anomaly_score = 0.0;
        let mut explanations = Vec::new();

        // Check app capability pattern
        let app_anomaly = self.check_app_pattern(request);
        if app_anomaly > 0.5 {
            explanations.push(format!(
                "Unusual capability '{}' for this app (score: {:.2})",
                request.capability, app_anomaly
            ));
        }
        anomaly_score = anomaly_score.max(app_anomaly);

        // Check time pattern
        let time_anomaly = self.check_time_pattern(request);
        if time_anomaly > 0.5 {
            explanations.push(format!(
                "Unusual time for this request (score: {:.2})",
                time_anomaly
            ));
        }
        anomaly_score = anomaly_score.max(time_anomaly);

        // Check resource pattern
        if let Some(ref resource) = request.resource {
            let resource_anomaly = self.check_resource_pattern(request, resource);
            if resource_anomaly > 0.5 {
                explanations.push(format!(
                    "Unusual resource access '{}' (score: {:.2})",
                    resource, resource_anomaly
                ));
            }
            anomaly_score = anomaly_score.max(resource_anomaly);
        }

        let is_known = anomaly_score < self.anomaly_threshold;

        PatternAnalysis {
            is_known,
            anomaly_score,
            similar_patterns: self.find_similar_patterns(request),
            explanation: if explanations.is_empty() {
                "Normal pattern".into()
            } else {
                explanations.join("; ")
            },
        }
    }

    fn check_app_pattern(&self, request: &CapabilityRequest) -> f32 {
        let profile = self.app_capabilities.get(&request.process_path);

        match profile {
            Some(profile) => {
                // Check if this capability has been used before
                let cap_count = profile.capabilities.get(&request.capability).copied().unwrap_or(0);

                if profile.total_requests == 0 {
                    return 0.5; // No history, neutral
                }

                // Calculate how unusual this capability is
                let cap_frequency = cap_count as f32 / profile.total_requests as f32;

                // New capability is more anomalous
                if cap_count == 0 {
                    0.8
                } else if cap_frequency < 0.01 {
                    0.6
                } else if cap_frequency < 0.1 {
                    0.3
                } else {
                    0.1
                }
            }
            None => {
                // First time seeing this app
                0.5
            }
        }
    }

    fn check_time_pattern(&self, request: &CapabilityRequest) -> f32 {
        let now = chrono::Local::now();
        let hour = now.hour() as usize;
        let day = now.weekday().num_days_from_sunday() as usize;

        let pattern = self.time_patterns.get(&request.process_path);

        match pattern {
            Some(pattern) => {
                let total_hourly: u64 = pattern.hourly_distribution.iter().sum();
                let total_weekly: u64 = pattern.weekly_distribution.iter().sum();

                if total_hourly == 0 {
                    return 0.3;
                }

                let hour_frequency = pattern.hourly_distribution[hour] as f32 / total_hourly as f32;
                let day_frequency = pattern.weekly_distribution[day] as f32 / total_weekly.max(1) as f32;

                // Check for unusual time
                let hour_anomaly = if hour_frequency < 0.01 { 0.7 } else { hour_frequency.recip().min(1.0) * 0.3 };
                let day_anomaly = if day_frequency < 0.01 { 0.5 } else { 0.1 };

                (hour_anomaly + day_anomaly) / 2.0
            }
            None => 0.3,
        }
    }

    fn check_resource_pattern(&self, request: &CapabilityRequest, resource: &str) -> f32 {
        let profile = self.resource_patterns.get(&request.process_path);

        match profile {
            Some(profile) => {
                // Check if this resource has been accessed before
                let resource_count = profile.resources.get(resource).copied().unwrap_or(0);
                let total: u64 = profile.resources.values().sum();

                if total == 0 {
                    return 0.5;
                }

                if resource_count == 0 {
                    // New resource - check if similar resources accessed
                    let similar = profile.resources.keys()
                        .filter(|r| has_common_prefix(r, resource))
                        .count();

                    if similar > 0 { 0.4 } else { 0.7 }
                } else {
                    0.1
                }
            }
            None => 0.5,
        }
    }

    fn find_similar_patterns(&self, request: &CapabilityRequest) -> Vec<String> {
        let mut similar = Vec::new();

        // Find apps that use similar capabilities
        for entry in self.app_capabilities.iter() {
            if entry.key() != &request.process_path {
                if entry.value().capabilities.contains_key(&request.capability) {
                    similar.push(entry.key().clone());
                }
            }
        }

        similar.truncate(5);
        similar
    }

    /// Learn from an approved request
    pub fn learn(&self, request: &CapabilityRequest) {
        if !self.enabled {
            return;
        }

        debug!("Learning pattern from: {:?}", request);

        // Update app capability profile
        let mut profile = self.app_capabilities
            .entry(request.process_path.clone())
            .or_default();
        *profile.capabilities.entry(request.capability.clone()).or_insert(0) += 1;
        profile.total_requests += 1;
        profile.last_seen = Some(SystemTime::now());
        if profile.first_seen.is_none() {
            profile.first_seen = Some(SystemTime::now());
        }

        // Update time pattern
        let now = chrono::Local::now();
        let hour = now.hour() as usize;
        let day = now.weekday().num_days_from_sunday() as usize;

        let mut time_pattern = self.time_patterns
            .entry(request.process_path.clone())
            .or_default();
        time_pattern.hourly_distribution[hour] += 1;
        time_pattern.weekly_distribution[day] += 1;

        // Update resource pattern
        if let Some(ref resource) = request.resource {
            let mut resource_profile = self.resource_patterns
                .entry(request.process_path.clone())
                .or_default();
            *resource_profile.resources.entry(resource.clone()).or_insert(0) += 1;
        }
    }

    /// Get anomaly threshold
    pub fn threshold(&self) -> f32 {
        self.anomaly_threshold
    }
}

fn has_common_prefix(a: &str, b: &str) -> bool {
    // Check if paths share at least 2 directory components
    let a_parts: Vec<_> = a.split('/').collect();
    let b_parts: Vec<_> = b.split('/').collect();

    let common = a_parts.iter().zip(b_parts.iter())
        .take_while(|(x, y)| x == y)
        .count();

    common >= 2
}

use chrono::{Datelike, Timelike};
