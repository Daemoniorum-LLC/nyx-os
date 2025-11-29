//! Notification types and management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Notification urgency level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

/// Notification data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub app_icon: Option<String>,
    pub summary: String,
    pub body: Option<String>,
    pub urgency: Urgency,
    pub timeout: i32,  // -1 = server default, 0 = never expire
    pub actions: Vec<NotificationAction>,
    pub hints: HashMap<String, HintValue>,
    pub timestamp: u64,
    pub replaces_id: Option<u32>,
    pub resident: bool,
    pub transient: bool,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationAction {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HintValue {
    String(String),
    Int(i64),
    Uint(u64),
    Bool(bool),
    Byte(u8),
    ByteArray(Vec<u8>),
}

impl Notification {
    pub fn new(id: u32, app_name: &str, summary: &str) -> Self {
        Self {
            id,
            app_name: app_name.to_string(),
            app_icon: None,
            summary: summary.to_string(),
            body: None,
            urgency: Urgency::Normal,
            timeout: -1,
            actions: Vec::new(),
            hints: HashMap::new(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            replaces_id: None,
            resident: false,
            transient: false,
            category: None,
        }
    }

    pub fn with_body(mut self, body: &str) -> Self {
        self.body = Some(body.to_string());
        self
    }

    pub fn with_icon(mut self, icon: &str) -> Self {
        self.app_icon = Some(icon.to_string());
        self
    }

    pub fn with_urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
        self
    }

    pub fn with_timeout(mut self, timeout: i32) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_action(mut self, id: &str, label: &str) -> Self {
        self.actions.push(NotificationAction {
            id: id.to_string(),
            label: label.to_string(),
        });
        self
    }

    pub fn with_category(mut self, category: &str) -> Self {
        self.category = Some(category.to_string());
        self
    }

    pub fn is_critical(&self) -> bool {
        self.urgency == Urgency::Critical
    }

    /// Get effective timeout in milliseconds
    pub fn effective_timeout(&self, default_ms: u64) -> Option<u64> {
        match self.timeout {
            -1 => Some(default_ms),
            0 => None,  // Never expire
            t if t > 0 => Some(t as u64),
            _ => Some(default_ms),
        }
    }

    /// Extract image data from hints if present
    pub fn get_image_data(&self) -> Option<ImageData> {
        // Check for image-data hint (icon_data in older spec)
        if let Some(HintValue::ByteArray(data)) = self.hints.get("image-data") {
            // Format: width, height, rowstride, has_alpha, bits_per_sample, channels, data
            // This is a simplified extraction
            return Some(ImageData {
                data: data.clone(),
                width: 0,
                height: 0,
                has_alpha: true,
            });
        }

        // Check for image-path hint
        if let Some(HintValue::String(path)) = self.hints.get("image-path") {
            return Some(ImageData {
                data: Vec::new(),
                width: 0,
                height: 0,
                has_alpha: false,
            });
        }

        None
    }

    /// Get desktop entry hint
    pub fn get_desktop_entry(&self) -> Option<&str> {
        if let Some(HintValue::String(entry)) = self.hints.get("desktop-entry") {
            Some(entry)
        } else {
            None
        }
    }

    /// Check if notification should be suppressed in DND
    pub fn suppress_in_dnd(&self) -> bool {
        !self.is_critical()
    }
}

#[derive(Debug, Clone)]
pub struct ImageData {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
}

/// Notification display state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NotificationState {
    Pending,
    Displayed,
    Closed(CloseReason),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CloseReason {
    Expired,
    Dismissed,
    ActionInvoked,
    Closed,  // Closed by CloseNotification call
}

impl CloseReason {
    pub fn to_code(&self) -> u32 {
        match self {
            CloseReason::Expired => 1,
            CloseReason::Dismissed => 2,
            CloseReason::ActionInvoked => 3,
            CloseReason::Closed => 4,
        }
    }
}

/// Notification queue manager
pub struct NotificationQueue {
    notifications: Vec<(Notification, NotificationState)>,
    next_id: u32,
    max_visible: usize,
}

impl NotificationQueue {
    pub fn new(max_visible: usize) -> Self {
        Self {
            notifications: Vec::new(),
            next_id: 1,
            max_visible,
        }
    }

    /// Add notification to queue
    pub fn add(&mut self, mut notification: Notification) -> u32 {
        // Handle replaces_id
        if let Some(replaces_id) = notification.replaces_id {
            if replaces_id > 0 {
                if let Some(pos) = self.notifications.iter().position(|(n, _)| n.id == replaces_id) {
                    notification.id = replaces_id;
                    self.notifications[pos] = (notification, NotificationState::Pending);
                    return replaces_id;
                }
            }
        }

        // Assign new ID
        let id = self.next_id;
        self.next_id += 1;
        notification.id = id;

        self.notifications.push((notification, NotificationState::Pending));
        id
    }

    /// Remove notification
    pub fn remove(&mut self, id: u32) -> Option<Notification> {
        if let Some(pos) = self.notifications.iter().position(|(n, _)| n.id == id) {
            Some(self.notifications.remove(pos).0)
        } else {
            None
        }
    }

    /// Get notification by ID
    pub fn get(&self, id: u32) -> Option<&Notification> {
        self.notifications.iter()
            .find(|(n, _)| n.id == id)
            .map(|(n, _)| n)
    }

    /// Get mutable notification by ID
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Notification> {
        self.notifications.iter_mut()
            .find(|(n, _)| n.id == id)
            .map(|(n, _)| n)
    }

    /// Update notification state
    pub fn set_state(&mut self, id: u32, state: NotificationState) {
        if let Some((_, s)) = self.notifications.iter_mut().find(|(n, _)| n.id == id) {
            *s = state;
        }
    }

    /// Get notifications ready to display
    pub fn get_pending(&self) -> Vec<&Notification> {
        self.notifications.iter()
            .filter(|(_, state)| *state == NotificationState::Pending)
            .take(self.max_visible)
            .map(|(n, _)| n)
            .collect()
    }

    /// Get currently displayed notifications
    pub fn get_displayed(&self) -> Vec<&Notification> {
        self.notifications.iter()
            .filter(|(_, state)| *state == NotificationState::Displayed)
            .map(|(n, _)| n)
            .collect()
    }

    /// Get all notifications
    pub fn all(&self) -> Vec<&Notification> {
        self.notifications.iter().map(|(n, _)| n).collect()
    }

    /// Count by urgency
    pub fn count_by_urgency(&self, urgency: Urgency) -> usize {
        self.notifications.iter()
            .filter(|(n, _)| n.urgency == urgency)
            .count()
    }

    /// Clear all closed notifications
    pub fn cleanup(&mut self) {
        self.notifications.retain(|(_, state)| {
            !matches!(state, NotificationState::Closed(_))
        });
    }
}

impl Default for NotificationQueue {
    fn default() -> Self {
        Self::new(5)
    }
}
