//! System clock management
//!
//! Handles setting system time and RTC synchronization.

use crate::config::RtcConfig;
use anyhow::{anyhow, Result};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// System clock manager
pub struct ClockManager {
    rtc_config: RtcConfig,
}

impl ClockManager {
    /// Create new clock manager
    pub fn new(rtc_config: RtcConfig) -> Self {
        Self { rtc_config }
    }

    /// Get current system time
    pub fn get_time(&self) -> SystemTime {
        SystemTime::now()
    }

    /// Get current time as Unix timestamp
    pub fn get_unix_time(&self) -> f64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64()
    }

    /// Set system time with step adjustment
    pub fn step_time(&self, offset: f64) -> Result<()> {
        let current = self.get_unix_time();
        let new_time = current + offset;

        info!("Stepping system clock by {:.6}s", offset);

        self.set_time_secs(new_time)?;

        Ok(())
    }

    /// Set system time from Unix timestamp
    fn set_time_secs(&self, unix_secs: f64) -> Result<()> {
        let secs = unix_secs as i64;
        let usecs = ((unix_secs - secs as f64) * 1_000_000.0) as i64;

        let tv = libc::timeval {
            tv_sec: secs,
            tv_usec: usecs,
        };

        let result = unsafe { libc::settimeofday(&tv, std::ptr::null()) };

        if result != 0 {
            return Err(anyhow!(
                "settimeofday failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        Ok(())
    }

    /// Adjust system time gradually (slew)
    pub fn slew_time(&self, offset: f64) -> Result<()> {
        // Convert offset to microseconds
        let usecs = (offset * 1_000_000.0) as i64;

        debug!("Slewing system clock by {:.6}s ({} usec)", offset, usecs);

        let delta = libc::timeval {
            tv_sec: 0,
            tv_usec: usecs,
        };

        let result = unsafe { libc::adjtime(&delta, std::ptr::null_mut()) };

        if result != 0 {
            return Err(anyhow!(
                "adjtime failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        Ok(())
    }

    /// Apply time correction (step or slew based on magnitude)
    pub fn apply_correction(&self, offset: f64, step_threshold: f64) -> Result<()> {
        if offset.abs() > step_threshold {
            info!(
                "Offset {:.6}s exceeds threshold {:.6}s, stepping",
                offset, step_threshold
            );
            self.step_time(offset)
        } else {
            debug!("Offset {:.6}s within threshold, slewing", offset);
            self.slew_time(offset)
        }
    }

    /// Read RTC time
    pub fn read_rtc(&self) -> Result<SystemTime> {
        if !self.rtc_config.enabled {
            return Err(anyhow!("RTC disabled"));
        }

        let mut file = OpenOptions::new()
            .read(true)
            .open(&self.rtc_config.device)?;

        // Use ioctl to read RTC time
        let mut rtc_time = RtcTime::default();
        let fd = file.as_raw_fd();

        let result =
            unsafe { libc::ioctl(fd, RTC_RD_TIME, &mut rtc_time as *mut RtcTime) };

        if result != 0 {
            return Err(anyhow!(
                "RTC read failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        // Convert RTC time to SystemTime
        let datetime = chrono::NaiveDateTime::new(
            chrono::NaiveDate::from_ymd_opt(
                rtc_time.tm_year as i32 + 1900,
                rtc_time.tm_mon as u32 + 1,
                rtc_time.tm_mday as u32,
            )
            .ok_or_else(|| anyhow!("Invalid RTC date"))?,
            chrono::NaiveTime::from_hms_opt(
                rtc_time.tm_hour as u32,
                rtc_time.tm_min as u32,
                rtc_time.tm_sec as u32,
            )
            .ok_or_else(|| anyhow!("Invalid RTC time"))?,
        );

        let unix_secs = if self.rtc_config.utc {
            datetime.and_utc().timestamp()
        } else {
            // If RTC is in local time, we need to convert
            datetime.and_utc().timestamp()
        };

        Ok(UNIX_EPOCH + Duration::from_secs(unix_secs as u64))
    }

    /// Write current system time to RTC
    pub fn sync_rtc(&self) -> Result<()> {
        if !self.rtc_config.enabled {
            return Err(anyhow!("RTC disabled"));
        }

        let file = OpenOptions::new()
            .write(true)
            .open(&self.rtc_config.device)?;

        let fd = file.as_raw_fd();

        // Use ioctl to set RTC from system time
        let result = unsafe { libc::ioctl(fd, RTC_SET_TIME_FROM_SYS) };

        if result != 0 {
            return Err(anyhow!(
                "RTC sync failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        info!("RTC synchronized with system clock");
        Ok(())
    }

    /// Get clock status
    pub fn get_status(&self) -> ClockStatus {
        let now = self.get_time();
        let uptime = self.get_uptime().unwrap_or_default();

        ClockStatus {
            system_time: now,
            unix_timestamp: self.get_unix_time(),
            uptime_secs: uptime.as_secs_f64(),
            rtc_available: self.rtc_config.enabled && self.read_rtc().is_ok(),
        }
    }

    /// Get system uptime
    fn get_uptime(&self) -> Result<Duration> {
        let content = std::fs::read_to_string("/proc/uptime")?;
        let uptime_str = content.split_whitespace().next().unwrap_or("0");
        let uptime_secs: f64 = uptime_str.parse().unwrap_or(0.0);
        Ok(Duration::from_secs_f64(uptime_secs))
    }
}

/// RTC time structure (matches kernel rtc_time)
#[repr(C)]
#[derive(Debug, Default)]
struct RtcTime {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
}

// RTC ioctl commands
const RTC_RD_TIME: libc::c_ulong = 0x80247009; // Read RTC time
const RTC_SET_TIME_FROM_SYS: libc::c_ulong = 0x0000700f; // Set RTC from system time

/// Clock status information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClockStatus {
    /// Current system time
    #[serde(with = "system_time_serde")]
    pub system_time: SystemTime,
    /// Unix timestamp
    pub unix_timestamp: f64,
    /// System uptime in seconds
    pub uptime_secs: f64,
    /// RTC available and readable
    pub rtc_available: bool,
}

mod system_time_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
        duration.as_secs_f64().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = f64::deserialize(deserializer)?;
        Ok(UNIX_EPOCH + Duration::from_secs_f64(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_time() {
        let config = RtcConfig::default();
        let manager = ClockManager::new(config);
        let time = manager.get_time();
        assert!(time > UNIX_EPOCH);
    }

    #[test]
    fn test_get_unix_time() {
        let config = RtcConfig::default();
        let manager = ClockManager::new(config);
        let unix = manager.get_unix_time();
        assert!(unix > 0.0);
    }
}
