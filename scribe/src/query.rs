//! Journal query utilities

use crate::journal::{LogEntry, Priority, Facility, JournalFilter};
use chrono::{DateTime, Utc, Duration, TimeZone};

/// Parse time specification
pub fn parse_time(spec: &str) -> Option<DateTime<Utc>> {
    // Try ISO 8601 format
    if let Ok(dt) = spec.parse::<DateTime<Utc>>() {
        return Some(dt);
    }

    // Try relative time (e.g., "1h ago", "30m ago")
    if spec.ends_with(" ago") {
        let amount = &spec[..spec.len() - 4];
        return parse_relative_time(amount);
    }

    // Try relative time without "ago" (e.g., "-1h", "-30m")
    if spec.starts_with('-') {
        return parse_relative_time(&spec[1..]);
    }

    // Try keywords
    match spec.to_lowercase().as_str() {
        "today" => {
            let now = Utc::now();
            Some(Utc.with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0).unwrap())
        }
        "yesterday" => {
            let yesterday = Utc::now() - Duration::days(1);
            Some(Utc.with_ymd_and_hms(yesterday.year(), yesterday.month(), yesterday.day(), 0, 0, 0).unwrap())
        }
        "now" => Some(Utc::now()),
        _ => None,
    }
}

fn parse_relative_time(spec: &str) -> Option<DateTime<Utc>> {
    let spec = spec.trim();

    // Parse number and unit
    let (num_str, unit) = if spec.ends_with("min") || spec.ends_with("m") {
        let num = spec.trim_end_matches("min").trim_end_matches('m');
        (num, "m")
    } else if spec.ends_with("hour") || spec.ends_with("h") {
        let num = spec.trim_end_matches("hour").trim_end_matches('h');
        (num, "h")
    } else if spec.ends_with("day") || spec.ends_with("d") {
        let num = spec.trim_end_matches("day").trim_end_matches('d');
        (num, "d")
    } else if spec.ends_with("week") || spec.ends_with("w") {
        let num = spec.trim_end_matches("week").trim_end_matches('w');
        (num, "w")
    } else {
        return None;
    };

    let num: i64 = num_str.trim().parse().ok()?;

    let duration = match unit {
        "m" => Duration::minutes(num),
        "h" => Duration::hours(num),
        "d" => Duration::days(num),
        "w" => Duration::weeks(num),
        _ => return None,
    };

    Some(Utc::now() - duration)
}

use chrono::Datelike;

/// Parse priority specification
pub fn parse_priority(spec: &str) -> Option<Priority> {
    match spec.to_lowercase().as_str() {
        "emerg" | "emergency" | "0" => Some(Priority::Emergency),
        "alert" | "1" => Some(Priority::Alert),
        "crit" | "critical" | "2" => Some(Priority::Critical),
        "err" | "error" | "3" => Some(Priority::Error),
        "warn" | "warning" | "4" => Some(Priority::Warning),
        "notice" | "5" => Some(Priority::Notice),
        "info" | "6" => Some(Priority::Info),
        "debug" | "7" => Some(Priority::Debug),
        _ => None,
    }
}

/// Parse facility specification
pub fn parse_facility(spec: &str) -> Option<Facility> {
    match spec.to_lowercase().as_str() {
        "kern" | "kernel" | "0" => Some(Facility::Kernel),
        "user" | "1" => Some(Facility::User),
        "mail" | "2" => Some(Facility::Mail),
        "daemon" | "3" => Some(Facility::Daemon),
        "auth" | "4" => Some(Facility::Auth),
        "syslog" | "5" => Some(Facility::Syslog),
        "lpr" | "6" => Some(Facility::Lpr),
        "news" | "7" => Some(Facility::News),
        "uucp" | "8" => Some(Facility::Uucp),
        "cron" | "9" => Some(Facility::Cron),
        "authpriv" | "10" => Some(Facility::AuthPriv),
        "ftp" | "11" => Some(Facility::Ftp),
        "local0" | "16" => Some(Facility::Local0),
        "local1" | "17" => Some(Facility::Local1),
        "local2" | "18" => Some(Facility::Local2),
        "local3" | "19" => Some(Facility::Local3),
        "local4" | "20" => Some(Facility::Local4),
        "local5" | "21" => Some(Facility::Local5),
        "local6" | "22" => Some(Facility::Local6),
        "local7" | "23" => Some(Facility::Local7),
        _ => None,
    }
}

/// Format log entry for display
pub fn format_entry(entry: &LogEntry, format: OutputFormat) -> String {
    match format {
        OutputFormat::Short => format!(
            "{} {} {}[{}]: {}",
            entry.timestamp.format("%b %d %H:%M:%S"),
            entry.hostname.as_deref().unwrap_or("localhost"),
            entry.identifier,
            entry.pid.map(|p| p.to_string()).unwrap_or_default(),
            entry.message
        ),
        OutputFormat::Verbose => format!(
            "{} [{}] {}.{} {}[{}]: {}",
            entry.timestamp.format("%Y-%m-%d %H:%M:%S%.3f"),
            entry.priority.as_str(),
            entry.facility.as_str(),
            entry.priority.as_str(),
            entry.identifier,
            entry.pid.map(|p| p.to_string()).unwrap_or_default(),
            entry.message
        ),
        OutputFormat::Json => serde_json::to_string(entry).unwrap_or_default(),
        OutputFormat::Cat => entry.message.clone(),
    }
}

#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Short,
    Verbose,
    Json,
    Cat,
}

impl Default for OutputFormat {
    fn default() -> Self {
        OutputFormat::Short
    }
}
