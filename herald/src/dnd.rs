//! Do Not Disturb functionality

use crate::config::{DndConfig, DndSchedule};
use crate::notification::{Notification, Urgency};
use chrono::{Datelike, Local, NaiveTime, Timelike};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Do Not Disturb manager
pub struct DndManager {
    config: Arc<RwLock<DndConfig>>,
    manual_enabled: Arc<RwLock<bool>>,
    manual_until: Arc<RwLock<Option<chrono::DateTime<Local>>>>,
}

impl DndManager {
    pub fn new(config: DndConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            manual_enabled: Arc::new(RwLock::new(false)),
            manual_until: Arc::new(RwLock::new(None)),
        }
    }

    /// Check if DND is currently active
    pub async fn is_active(&self) -> bool {
        // Check manual override first
        if *self.manual_enabled.read().await {
            // Check if timed override has expired
            if let Some(until) = *self.manual_until.read().await {
                if Local::now() >= until {
                    *self.manual_enabled.write().await = false;
                    *self.manual_until.write().await = None;
                    return self.check_schedule().await;
                }
            }
            return true;
        }

        self.check_schedule().await
    }

    async fn check_schedule(&self) -> bool {
        let config = self.config.read().await;
        let now = Local::now();

        for schedule in &config.schedule {
            if self.matches_schedule(&schedule, &now) {
                return true;
            }
        }

        false
    }

    fn matches_schedule(&self, schedule: &DndSchedule, now: &chrono::DateTime<Local>) -> bool {
        // Check day
        let day_name = match now.weekday() {
            chrono::Weekday::Mon => "monday",
            chrono::Weekday::Tue => "tuesday",
            chrono::Weekday::Wed => "wednesday",
            chrono::Weekday::Thu => "thursday",
            chrono::Weekday::Fri => "friday",
            chrono::Weekday::Sat => "saturday",
            chrono::Weekday::Sun => "sunday",
        };

        let day_matches = schedule.days.iter().any(|d| {
            d.to_lowercase() == day_name ||
            d.to_lowercase() == "all" ||
            (d.to_lowercase() == "weekdays" && matches!(now.weekday(), chrono::Weekday::Mon | chrono::Weekday::Tue | chrono::Weekday::Wed | chrono::Weekday::Thu | chrono::Weekday::Fri)) ||
            (d.to_lowercase() == "weekends" && matches!(now.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun))
        });

        if !day_matches {
            return false;
        }

        // Check time
        let current_time = now.time();

        let start = NaiveTime::parse_from_str(&schedule.start, "%H:%M").ok();
        let end = NaiveTime::parse_from_str(&schedule.end, "%H:%M").ok();

        match (start, end) {
            (Some(start_time), Some(end_time)) => {
                if start_time <= end_time {
                    // Normal range (e.g., 22:00 to 23:00)
                    current_time >= start_time && current_time < end_time
                } else {
                    // Overnight range (e.g., 22:00 to 07:00)
                    current_time >= start_time || current_time < end_time
                }
            }
            _ => false,
        }
    }

    /// Check if a notification should be shown during DND
    pub async fn should_show(&self, notification: &Notification) -> bool {
        if !self.is_active().await {
            return true;
        }

        let config = self.config.read().await;

        // Always show critical if configured
        if config.allow_critical && notification.is_critical() {
            return true;
        }

        false
    }

    /// Enable DND manually
    pub async fn enable(&self) {
        *self.manual_enabled.write().await = true;
        *self.manual_until.write().await = None;
        tracing::info!("DND enabled manually");
    }

    /// Enable DND for a duration
    pub async fn enable_for(&self, minutes: u32) {
        *self.manual_enabled.write().await = true;
        *self.manual_until.write().await = Some(
            Local::now() + chrono::Duration::minutes(minutes as i64)
        );
        tracing::info!("DND enabled for {} minutes", minutes);
    }

    /// Disable DND manually
    pub async fn disable(&self) {
        *self.manual_enabled.write().await = false;
        *self.manual_until.write().await = None;
        tracing::info!("DND disabled");
    }

    /// Toggle DND state
    pub async fn toggle(&self) -> bool {
        let currently_enabled = *self.manual_enabled.read().await;
        if currently_enabled {
            self.disable().await;
            false
        } else {
            self.enable().await;
            true
        }
    }

    /// Get time until DND ends (if timed)
    pub async fn remaining_time(&self) -> Option<chrono::Duration> {
        let until = *self.manual_until.read().await;
        until.map(|u| u - Local::now())
    }

    /// Update schedule configuration
    pub async fn update_config(&self, config: DndConfig) {
        *self.config.write().await = config;
    }

    /// Add a schedule entry
    pub async fn add_schedule(&self, schedule: DndSchedule) {
        self.config.write().await.schedule.push(schedule);
    }

    /// Remove a schedule entry by index
    pub async fn remove_schedule(&self, index: usize) {
        let mut config = self.config.write().await;
        if index < config.schedule.len() {
            config.schedule.remove(index);
        }
    }

    /// Get current DND status
    pub async fn status(&self) -> DndStatus {
        let is_active = self.is_active().await;
        let manual = *self.manual_enabled.read().await;
        let until = *self.manual_until.read().await;

        let reason = if !is_active {
            DndReason::Inactive
        } else if manual {
            if until.is_some() {
                DndReason::ManualTimed
            } else {
                DndReason::Manual
            }
        } else {
            DndReason::Scheduled
        };

        DndStatus {
            active: is_active,
            reason,
            until,
            allow_critical: self.config.read().await.allow_critical,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DndStatus {
    pub active: bool,
    pub reason: DndReason,
    pub until: Option<chrono::DateTime<Local>>,
    pub allow_critical: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DndReason {
    Inactive,
    Manual,
    ManualTimed,
    Scheduled,
}

/// Quick DND presets
pub enum DndPreset {
    OneHour,
    TwoHours,
    FourHours,
    UntilTomorrow,
    ThisMeeting,
}

impl DndPreset {
    pub fn duration_minutes(&self) -> u32 {
        match self {
            DndPreset::OneHour => 60,
            DndPreset::TwoHours => 120,
            DndPreset::FourHours => 240,
            DndPreset::UntilTomorrow => {
                let now = Local::now();
                let tomorrow_8am = (now + chrono::Duration::days(1))
                    .date_naive()
                    .and_hms_opt(8, 0, 0)
                    .unwrap();
                let tomorrow = tomorrow_8am.and_local_timezone(Local).unwrap();
                (tomorrow - now).num_minutes() as u32
            }
            DndPreset::ThisMeeting => 30,
        }
    }
}
