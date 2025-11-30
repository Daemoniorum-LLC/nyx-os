//! Timezone management
//!
//! Handles timezone configuration and queries.

use crate::config::TimezoneConfig;
use anyhow::{anyhow, Result};
use chrono::{DateTime, FixedOffset, Offset, TimeZone, Utc};
use chrono_tz::Tz;
use std::fs;
use std::path::Path;
use tracing::{debug, info};

/// Timezone manager
pub struct TimezoneManager {
    config: TimezoneConfig,
    current_tz: Tz,
}

impl TimezoneManager {
    /// Create new timezone manager
    pub fn new(config: TimezoneConfig) -> Result<Self> {
        let current_tz: Tz = config.timezone.parse().map_err(|_| {
            anyhow!("Invalid timezone: {}", config.timezone)
        })?;

        Ok(Self { config, current_tz })
    }

    /// Get current timezone
    pub fn current(&self) -> &Tz {
        &self.current_tz
    }

    /// Get current timezone name
    pub fn current_name(&self) -> &str {
        &self.config.timezone
    }

    /// Set timezone
    pub fn set_timezone(&mut self, tz_name: &str) -> Result<()> {
        let new_tz: Tz = tz_name.parse().map_err(|_| {
            anyhow!("Invalid timezone: {}", tz_name)
        })?;

        // Update system timezone symlink if running as root
        if let Err(e) = self.update_system_timezone(tz_name) {
            debug!("Could not update system timezone: {}", e);
        }

        self.current_tz = new_tz;
        self.config.timezone = tz_name.to_string();

        info!("Timezone set to {}", tz_name);
        Ok(())
    }

    /// Update system timezone symlink
    fn update_system_timezone(&self, tz_name: &str) -> Result<()> {
        let localtime_path = Path::new("/etc/localtime");
        let tz_file = Path::new(&self.config.tzdata_path).join(tz_name);

        if !tz_file.exists() {
            return Err(anyhow!("Timezone file not found: {:?}", tz_file));
        }

        // Remove existing symlink
        if localtime_path.exists() || localtime_path.is_symlink() {
            fs::remove_file(localtime_path)?;
        }

        // Create new symlink
        std::os::unix::fs::symlink(&tz_file, localtime_path)?;

        // Also update /etc/timezone if it exists
        let timezone_file = Path::new("/etc/timezone");
        if timezone_file.exists() || timezone_file.parent().map(|p| p.exists()).unwrap_or(false) {
            let _ = fs::write(timezone_file, format!("{}\n", tz_name));
        }

        Ok(())
    }

    /// Get UTC offset for current timezone
    pub fn utc_offset(&self) -> FixedOffset {
        let now = Utc::now();
        let local = now.with_timezone(&self.current_tz);
        local.offset().fix()
    }

    /// Get UTC offset in seconds
    pub fn utc_offset_seconds(&self) -> i32 {
        self.utc_offset().local_minus_utc()
    }

    /// Get UTC offset as string (e.g., "+05:30", "-08:00")
    pub fn utc_offset_string(&self) -> String {
        let offset_secs = self.utc_offset_seconds();
        let hours = offset_secs / 3600;
        let minutes = (offset_secs.abs() % 3600) / 60;

        if offset_secs >= 0 {
            format!("+{:02}:{:02}", hours, minutes)
        } else {
            format!("{:03}:{:02}", hours, minutes)
        }
    }

    /// Convert UTC time to local time
    pub fn utc_to_local(&self, utc: DateTime<Utc>) -> DateTime<Tz> {
        utc.with_timezone(&self.current_tz)
    }

    /// Convert local time to UTC
    pub fn local_to_utc(&self, local: DateTime<Tz>) -> DateTime<Utc> {
        local.with_timezone(&Utc)
    }

    /// Get current time in configured timezone
    pub fn now(&self) -> DateTime<Tz> {
        Utc::now().with_timezone(&self.current_tz)
    }

    /// Check if timezone observes DST
    pub fn has_dst(&self) -> bool {
        // Check if offset differs between summer and winter
        let winter = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let summer = Utc.with_ymd_and_hms(2024, 7, 15, 12, 0, 0).unwrap();

        let winter_offset = winter.with_timezone(&self.current_tz).offset().fix().local_minus_utc();
        let summer_offset = summer.with_timezone(&self.current_tz).offset().fix().local_minus_utc();

        winter_offset != summer_offset
    }

    /// Check if currently in DST
    pub fn is_dst(&self) -> bool {
        if !self.has_dst() {
            return false;
        }

        // Compare current offset to January offset
        let winter = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let winter_offset = winter.with_timezone(&self.current_tz).offset().fix().local_minus_utc();
        let current_offset = self.utc_offset_seconds();

        current_offset != winter_offset
    }

    /// List available timezones
    pub fn list_timezones(&self) -> Vec<String> {
        // Return common timezones - chrono-tz has them all
        chrono_tz::TZ_VARIANTS
            .iter()
            .map(|tz| tz.name().to_string())
            .collect()
    }

    /// List timezones by region
    pub fn list_timezones_by_region(&self, region: &str) -> Vec<String> {
        chrono_tz::TZ_VARIANTS
            .iter()
            .filter(|tz| tz.name().starts_with(region))
            .map(|tz| tz.name().to_string())
            .collect()
    }

    /// Get timezone info
    pub fn get_info(&self) -> TimezoneInfo {
        TimezoneInfo {
            name: self.config.timezone.clone(),
            offset: self.utc_offset_string(),
            offset_seconds: self.utc_offset_seconds(),
            has_dst: self.has_dst(),
            is_dst: self.is_dst(),
        }
    }
}

/// Timezone information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimezoneInfo {
    /// Timezone name (IANA)
    pub name: String,
    /// UTC offset string (e.g., "+05:30")
    pub offset: String,
    /// UTC offset in seconds
    pub offset_seconds: i32,
    /// Whether timezone observes DST
    pub has_dst: bool,
    /// Whether currently in DST
    pub is_dst: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timezone_manager() {
        let config = TimezoneConfig {
            timezone: "America/New_York".to_string(),
            tzdata_path: "/usr/share/zoneinfo".to_string(),
            auto_detect: false,
        };

        let manager = TimezoneManager::new(config).unwrap();
        assert_eq!(manager.current_name(), "America/New_York");

        // New York has DST
        assert!(manager.has_dst());
    }

    #[test]
    fn test_utc_timezone() {
        let config = TimezoneConfig {
            timezone: "UTC".to_string(),
            tzdata_path: "/usr/share/zoneinfo".to_string(),
            auto_detect: false,
        };

        let manager = TimezoneManager::new(config).unwrap();
        assert_eq!(manager.utc_offset_seconds(), 0);
        assert!(!manager.has_dst());
    }
}
