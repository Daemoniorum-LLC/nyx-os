//! Notification history management

use crate::notification::{CloseReason, Notification, Urgency};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;

/// Historical notification entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub notification: Notification,
    pub displayed_at: u64,
    pub closed_at: Option<u64>,
    pub close_reason: Option<CloseReason>,
    pub action_invoked: Option<String>,
}

/// Notification history manager
pub struct NotificationHistory {
    entries: VecDeque<HistoryEntry>,
    max_size: usize,
    retention_days: u32,
    file_path: PathBuf,
}

impl NotificationHistory {
    pub fn new(max_size: usize, retention_days: u32, file_path: PathBuf) -> Self {
        Self {
            entries: VecDeque::new(),
            max_size,
            retention_days,
            file_path,
        }
    }

    /// Load history from file
    pub async fn load(&mut self) -> Result<()> {
        if self.file_path.exists() {
            let content = tokio::fs::read_to_string(&self.file_path).await?;
            let entries: Vec<HistoryEntry> = serde_json::from_str(&content)?;
            self.entries = entries.into_iter().collect();
            self.cleanup_old();
        }
        Ok(())
    }

    /// Save history to file
    pub async fn save(&self) -> Result<()> {
        if let Some(parent) = self.file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let entries: Vec<&HistoryEntry> = self.entries.iter().collect();
        let content = serde_json::to_string_pretty(&entries)?;
        tokio::fs::write(&self.file_path, content).await?;
        Ok(())
    }

    /// Add a notification to history
    pub fn add(&mut self, notification: Notification) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let entry = HistoryEntry {
            notification,
            displayed_at: now,
            closed_at: None,
            close_reason: None,
            action_invoked: None,
        };

        self.entries.push_front(entry);

        // Trim to max size
        while self.entries.len() > self.max_size {
            self.entries.pop_back();
        }
    }

    /// Record notification closure
    pub fn record_close(&mut self, id: u32, reason: CloseReason, action: Option<String>) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if let Some(entry) = self.entries.iter_mut()
            .find(|e| e.notification.id == id && e.closed_at.is_none())
        {
            entry.closed_at = Some(now);
            entry.close_reason = Some(reason);
            entry.action_invoked = action;
        }
    }

    /// Get all history entries
    pub fn all(&self) -> Vec<&HistoryEntry> {
        self.entries.iter().collect()
    }

    /// Get entries from a specific app
    pub fn by_app(&self, app_name: &str) -> Vec<&HistoryEntry> {
        self.entries.iter()
            .filter(|e| e.notification.app_name == app_name)
            .collect()
    }

    /// Get entries by urgency
    pub fn by_urgency(&self, urgency: Urgency) -> Vec<&HistoryEntry> {
        self.entries.iter()
            .filter(|e| e.notification.urgency == urgency)
            .collect()
    }

    /// Get entries from last N hours
    pub fn last_hours(&self, hours: u64) -> Vec<&HistoryEntry> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let cutoff = now.saturating_sub(hours * 3600);

        self.entries.iter()
            .filter(|e| e.displayed_at >= cutoff)
            .collect()
    }

    /// Search history by summary/body
    pub fn search(&self, query: &str) -> Vec<&HistoryEntry> {
        let query_lower = query.to_lowercase();

        self.entries.iter()
            .filter(|e| {
                e.notification.summary.to_lowercase().contains(&query_lower) ||
                e.notification.body.as_ref()
                    .map(|b| b.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
            })
            .collect()
    }

    /// Get unread notifications (never closed by user)
    pub fn unread(&self) -> Vec<&HistoryEntry> {
        self.entries.iter()
            .filter(|e| {
                e.close_reason != Some(CloseReason::Dismissed) &&
                e.close_reason != Some(CloseReason::ActionInvoked)
            })
            .collect()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Clear history for specific app
    pub fn clear_app(&mut self, app_name: &str) {
        self.entries.retain(|e| e.notification.app_name != app_name);
    }

    /// Remove old entries based on retention policy
    fn cleanup_old(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let cutoff = now.saturating_sub(self.retention_days as u64 * 24 * 3600);

        self.entries.retain(|e| e.displayed_at >= cutoff);
    }

    /// Get statistics
    pub fn stats(&self) -> HistoryStats {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let today_cutoff = now.saturating_sub(24 * 3600);

        HistoryStats {
            total: self.entries.len(),
            today: self.entries.iter().filter(|e| e.displayed_at >= today_cutoff).count(),
            critical: self.entries.iter().filter(|e| e.notification.urgency == Urgency::Critical).count(),
            unread: self.unread().len(),
            by_app: self.count_by_app(),
        }
    }

    fn count_by_app(&self) -> Vec<(String, usize)> {
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        for entry in &self.entries {
            *counts.entry(entry.notification.app_name.clone()).or_default() += 1;
        }

        let mut sorted: Vec<_> = counts.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted
    }

    /// Get entry count
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryStats {
    pub total: usize,
    pub today: usize,
    pub critical: usize,
    pub unread: usize,
    pub by_app: Vec<(String, usize)>,
}

/// Serializable close reason for JSON
impl Serialize for CloseReason {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            CloseReason::Expired => "expired",
            CloseReason::Dismissed => "dismissed",
            CloseReason::ActionInvoked => "action_invoked",
            CloseReason::Closed => "closed",
        };
        serializer.serialize_str(s)
    }
}

impl<'de> Deserialize<'de> for CloseReason {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "expired" => Ok(CloseReason::Expired),
            "dismissed" => Ok(CloseReason::Dismissed),
            "action_invoked" => Ok(CloseReason::ActionInvoked),
            "closed" => Ok(CloseReason::Closed),
            _ => Err(serde::de::Error::custom("unknown close reason")),
        }
    }
}
