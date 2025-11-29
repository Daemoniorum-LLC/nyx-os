//! Recently used applications tracking

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::SystemTime;

/// Recent applications manager
pub struct RecentApps {
    entries: VecDeque<RecentEntry>,
    max_size: usize,
    file_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentEntry {
    pub app_id: String,
    pub timestamp: u64,
    pub count: u64,
}

impl RecentApps {
    pub fn new(max_size: usize, file_path: PathBuf) -> Self {
        Self {
            entries: VecDeque::new(),
            max_size,
            file_path,
        }
    }

    /// Load recent apps from file
    pub async fn load(&mut self) -> Result<()> {
        if self.file_path.exists() {
            let content = tokio::fs::read_to_string(&self.file_path).await?;
            let entries: Vec<RecentEntry> = serde_json::from_str(&content)?;
            self.entries = entries.into_iter().collect();
        }
        Ok(())
    }

    /// Save recent apps to file
    pub async fn save(&self) -> Result<()> {
        if let Some(parent) = self.file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let entries: Vec<&RecentEntry> = self.entries.iter().collect();
        let content = serde_json::to_string_pretty(&entries)?;
        tokio::fs::write(&self.file_path, content).await?;
        Ok(())
    }

    /// Record an application use
    pub fn record(&mut self, app_id: &str) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Check if already in recent list
        if let Some(pos) = self.entries.iter().position(|e| e.app_id == app_id) {
            // Move to front and update
            if let Some(mut entry) = self.entries.remove(pos) {
                entry.timestamp = now;
                entry.count += 1;
                self.entries.push_front(entry);
            }
        } else {
            // Add new entry
            self.entries.push_front(RecentEntry {
                app_id: app_id.to_string(),
                timestamp: now,
                count: 1,
            });

            // Trim to max size
            while self.entries.len() > self.max_size {
                self.entries.pop_back();
            }
        }
    }

    /// Get recent app IDs in order
    pub fn get_recent(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.app_id.as_str()).collect()
    }

    /// Get recent app IDs with usage count
    pub fn get_recent_with_count(&self) -> Vec<(&str, u64)> {
        self.entries.iter().map(|e| (e.app_id.as_str(), e.count)).collect()
    }

    /// Get most frequently used
    pub fn get_frequent(&self, limit: usize) -> Vec<&str> {
        let mut sorted: Vec<_> = self.entries.iter().collect();
        sorted.sort_by(|a, b| b.count.cmp(&a.count));
        sorted.into_iter().take(limit).map(|e| e.app_id.as_str()).collect()
    }

    /// Check if app is in recent list
    pub fn contains(&self, app_id: &str) -> bool {
        self.entries.iter().any(|e| e.app_id == app_id)
    }

    /// Get usage count for an app
    pub fn get_count(&self, app_id: &str) -> u64 {
        self.entries
            .iter()
            .find(|e| e.app_id == app_id)
            .map(|e| e.count)
            .unwrap_or(0)
    }

    /// Clear all recent entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Remove a specific app from recent
    pub fn remove(&mut self, app_id: &str) {
        self.entries.retain(|e| e.app_id != app_id);
    }

    /// Get entry count
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Frecency scoring (frequency + recency)
pub struct FrecencyScorer {
    decay_factor: f64,
    frequency_weight: f64,
    recency_weight: f64,
}

impl FrecencyScorer {
    pub fn new() -> Self {
        Self {
            decay_factor: 0.95,
            frequency_weight: 0.3,
            recency_weight: 0.7,
        }
    }

    pub fn with_weights(frequency: f64, recency: f64) -> Self {
        Self {
            decay_factor: 0.95,
            frequency_weight: frequency,
            recency_weight: recency,
        }
    }

    /// Calculate frecency score
    pub fn score(&self, entry: &RecentEntry, max_count: u64, now: u64) -> f64 {
        // Frequency component (normalized)
        let freq_score = if max_count > 0 {
            entry.count as f64 / max_count as f64
        } else {
            0.0
        };

        // Recency component (exponential decay)
        let age_hours = (now.saturating_sub(entry.timestamp)) as f64 / 3600.0;
        let recency_score = self.decay_factor.powf(age_hours / 24.0);

        // Combined score
        self.frequency_weight * freq_score + self.recency_weight * recency_score
    }

    /// Rank entries by frecency
    pub fn rank<'a>(&self, entries: &'a [RecentEntry]) -> Vec<(&'a RecentEntry, f64)> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let max_count = entries.iter().map(|e| e.count).max().unwrap_or(1);

        let mut scored: Vec<_> = entries
            .iter()
            .map(|e| (e, self.score(e, max_count, now)))
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored
    }
}

impl Default for FrecencyScorer {
    fn default() -> Self {
        Self::new()
    }
}

/// Usage pattern analyzer
pub struct UsagePatterns {
    hourly_usage: [u64; 24],
    daily_usage: [u64; 7],
    app_times: std::collections::HashMap<String, Vec<u64>>,
}

impl UsagePatterns {
    pub fn new() -> Self {
        Self {
            hourly_usage: [0; 24],
            daily_usage: [0; 7],
            app_times: std::collections::HashMap::new(),
        }
    }

    /// Record a usage event
    pub fn record(&mut self, app_id: &str, timestamp: u64) {
        use chrono::{DateTime, Datelike, Timelike, Utc};

        let dt = DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
            .unwrap_or_else(Utc::now);

        let hour = dt.hour() as usize;
        let day = dt.weekday().num_days_from_monday() as usize;

        self.hourly_usage[hour] += 1;
        self.daily_usage[day] += 1;

        self.app_times
            .entry(app_id.to_string())
            .or_default()
            .push(timestamp);
    }

    /// Get peak usage hours
    pub fn peak_hours(&self) -> Vec<usize> {
        let max = *self.hourly_usage.iter().max().unwrap_or(&0);
        if max == 0 {
            return Vec::new();
        }

        let threshold = max * 80 / 100; // Top 20%

        self.hourly_usage
            .iter()
            .enumerate()
            .filter(|(_, &count)| count >= threshold)
            .map(|(hour, _)| hour)
            .collect()
    }

    /// Get apps commonly used at a specific hour
    pub fn apps_for_hour(&self, hour: usize) -> Vec<String> {
        use chrono::{DateTime, Timelike, Utc};

        let mut app_scores: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

        for (app_id, times) in &self.app_times {
            let count = times.iter().filter(|&&t| {
                DateTime::<Utc>::from_timestamp(t as i64, 0)
                    .map(|dt| dt.hour() as usize == hour)
                    .unwrap_or(false)
            }).count();

            if count > 0 {
                app_scores.insert(app_id.clone(), count as u64);
            }
        }

        let mut sorted: Vec<_> = app_scores.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.into_iter().map(|(id, _)| id).collect()
    }
}

impl Default for UsagePatterns {
    fn default() -> Self {
        Self::new()
    }
}
