//! Log collectors

use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixListener;
use tracing::{debug, warn};

use crate::state::ScribeState;
use crate::journal::{LogEntry, Priority, Facility};

/// Kernel log collector (reads from /dev/kmsg)
pub struct KernelCollector;

impl KernelCollector {
    pub fn new() -> Self {
        Self
    }

    pub async fn run(&self, state: Arc<RwLock<ScribeState>>) -> Result<()> {
        let file = tokio::fs::File::open("/dev/kmsg").await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await? {
            if let Some(entry) = self.parse_kmsg(&line) {
                let mut state = state.write().await;
                if let Err(e) = state.journal.write(&entry) {
                    warn!("Failed to write kernel log: {}", e);
                }
            }
        }

        Ok(())
    }

    fn parse_kmsg(&self, line: &str) -> Option<LogEntry> {
        // kmsg format: <priority>,<sequence>,<timestamp>,-;<message>
        let parts: Vec<&str> = line.splitn(2, ';').collect();
        if parts.len() != 2 {
            return None;
        }

        let header = parts[0];
        let message = parts[1].trim();

        let header_parts: Vec<&str> = header.split(',').collect();
        if header_parts.is_empty() {
            return None;
        }

        let priority_raw: u8 = header_parts[0].parse().ok()?;
        let priority = Priority::from_u8(priority_raw & 0x7);

        Some(LogEntry {
            timestamp: chrono::Utc::now(),
            priority,
            facility: Facility::Kernel,
            identifier: "kernel".to_string(),
            message: message.to_string(),
            pid: None,
            uid: None,
            hostname: None,
            fields: std::collections::HashMap::new(),
        })
    }
}

/// Syslog collector (listens on /dev/log)
pub struct SyslogCollector {
    socket_path: String,
}

impl SyslogCollector {
    pub fn new(socket_path: &str) -> Self {
        Self {
            socket_path: socket_path.to_string(),
        }
    }

    pub async fn run(&self, state: Arc<RwLock<ScribeState>>) -> Result<()> {
        // Remove existing socket
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)?;

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let state = state.clone();
                    tokio::spawn(async move {
                        let reader = BufReader::new(stream);
                        let mut lines = reader.lines();

                        while let Ok(Some(line)) = lines.next_line().await {
                            if let Some(entry) = Self::parse_syslog(&line) {
                                let mut state = state.write().await;
                                if let Err(e) = state.journal.write(&entry) {
                                    warn!("Failed to write syslog: {}", e);
                                }
                            }
                        }
                    });
                }
                Err(e) => warn!("Syslog accept error: {}", e),
            }
        }
    }

    fn parse_syslog(line: &str) -> Option<LogEntry> {
        // BSD syslog format: <priority>timestamp hostname identifier[pid]: message
        // Or simplified: <priority>message

        let line = line.trim();

        if !line.starts_with('<') {
            return None;
        }

        let end_pri = line.find('>')?;
        let pri_str = &line[1..end_pri];
        let pri: u8 = pri_str.parse().ok()?;

        let facility = Facility::from_u8(pri >> 3);
        let priority = Priority::from_u8(pri & 0x7);

        let rest = &line[end_pri + 1..];

        // Try to parse structured format
        let (identifier, message, pid) = Self::parse_message(rest);

        Some(LogEntry {
            timestamp: chrono::Utc::now(),
            priority,
            facility,
            identifier,
            message,
            pid,
            uid: None,
            hostname: None,
            fields: std::collections::HashMap::new(),
        })
    }

    fn parse_message(msg: &str) -> (String, String, Option<u32>) {
        // Try to extract identifier[pid]: message
        if let Some(colon_pos) = msg.find(": ") {
            let prefix = &msg[..colon_pos];
            let message = &msg[colon_pos + 2..];

            if let Some(bracket_start) = prefix.find('[') {
                if let Some(bracket_end) = prefix.find(']') {
                    let identifier = prefix[..bracket_start].trim().to_string();
                    let pid: Option<u32> = prefix[bracket_start + 1..bracket_end].parse().ok();
                    return (identifier, message.to_string(), pid);
                }
            }

            return (prefix.trim().to_string(), message.to_string(), None);
        }

        ("unknown".to_string(), msg.to_string(), None)
    }
}

/// Stdout/stderr collector for services
pub struct StdoutCollector {
    identifier: String,
    pid: u32,
}

impl StdoutCollector {
    pub fn new(identifier: &str, pid: u32) -> Self {
        Self {
            identifier: identifier.to_string(),
            pid,
        }
    }

    pub async fn collect_stream<R: tokio::io::AsyncRead + Unpin>(
        &self,
        stream: R,
        priority: Priority,
        state: Arc<RwLock<ScribeState>>,
    ) -> Result<()> {
        let reader = BufReader::new(stream);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await? {
            let entry = LogEntry {
                timestamp: chrono::Utc::now(),
                priority,
                facility: Facility::Daemon,
                identifier: self.identifier.clone(),
                message: line,
                pid: Some(self.pid),
                uid: None,
                hostname: None,
                fields: std::collections::HashMap::new(),
            };

            let mut state = state.write().await;
            state.journal.write(&entry)?;
        }

        Ok(())
    }
}
