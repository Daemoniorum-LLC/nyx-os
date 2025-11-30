//! Journal storage and management

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use tracing::{info, debug};

/// Log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub priority: Priority,
    pub facility: Facility,
    pub identifier: String,
    pub message: String,
    pub pid: Option<u32>,
    pub uid: Option<u32>,
    pub hostname: Option<String>,
    pub fields: HashMap<String, String>,
}

/// Log priority (syslog compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum Priority {
    Emergency = 0,
    Alert = 1,
    Critical = 2,
    Error = 3,
    Warning = 4,
    Notice = 5,
    Info = 6,
    Debug = 7,
}

impl Priority {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Priority::Emergency,
            1 => Priority::Alert,
            2 => Priority::Critical,
            3 => Priority::Error,
            4 => Priority::Warning,
            5 => Priority::Notice,
            6 => Priority::Info,
            _ => Priority::Debug,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::Emergency => "emerg",
            Priority::Alert => "alert",
            Priority::Critical => "crit",
            Priority::Error => "err",
            Priority::Warning => "warning",
            Priority::Notice => "notice",
            Priority::Info => "info",
            Priority::Debug => "debug",
        }
    }
}

/// Log facility (syslog compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Facility {
    Kernel = 0,
    User = 1,
    Mail = 2,
    Daemon = 3,
    Auth = 4,
    Syslog = 5,
    Lpr = 6,
    News = 7,
    Uucp = 8,
    Cron = 9,
    AuthPriv = 10,
    Ftp = 11,
    Local0 = 16,
    Local1 = 17,
    Local2 = 18,
    Local3 = 19,
    Local4 = 20,
    Local5 = 21,
    Local6 = 22,
    Local7 = 23,
}

impl Facility {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Facility::Kernel,
            1 => Facility::User,
            2 => Facility::Mail,
            3 => Facility::Daemon,
            4 => Facility::Auth,
            5 => Facility::Syslog,
            6 => Facility::Lpr,
            7 => Facility::News,
            8 => Facility::Uucp,
            9 => Facility::Cron,
            10 => Facility::AuthPriv,
            11 => Facility::Ftp,
            16 => Facility::Local0,
            17 => Facility::Local1,
            18 => Facility::Local2,
            19 => Facility::Local3,
            20 => Facility::Local4,
            21 => Facility::Local5,
            22 => Facility::Local6,
            23 => Facility::Local7,
            _ => Facility::User,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Facility::Kernel => "kern",
            Facility::User => "user",
            Facility::Mail => "mail",
            Facility::Daemon => "daemon",
            Facility::Auth => "auth",
            Facility::Syslog => "syslog",
            Facility::Lpr => "lpr",
            Facility::News => "news",
            Facility::Uucp => "uucp",
            Facility::Cron => "cron",
            Facility::AuthPriv => "authpriv",
            Facility::Ftp => "ftp",
            _ => "local",
        }
    }
}

/// Journal storage
pub struct Journal {
    dir: PathBuf,
    current_file: PathBuf,
    writer: BufWriter<File>,
    entry_count: u64,
    current_size: u64,
    max_file_size: u64,
}

impl Journal {
    /// Open or create journal
    pub fn open(dir: &str) -> Result<Self> {
        let dir = PathBuf::from(dir);
        fs::create_dir_all(&dir)?;

        let current_file = dir.join("current.journal");

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&current_file)?;

        let current_size = file.metadata()?.len();

        Ok(Self {
            dir,
            current_file,
            writer: BufWriter::new(file),
            entry_count: 0,
            current_size,
            max_file_size: 50 * 1024 * 1024, // 50MB default
        })
    }

    /// Write log entry
    pub fn write(&mut self, entry: &LogEntry) -> Result<()> {
        let json = serde_json::to_string(entry)?;
        let line = format!("{}\n", json);
        let bytes = line.as_bytes();

        self.writer.write_all(bytes)?;
        self.current_size += bytes.len() as u64;
        self.entry_count += 1;

        // Flush periodically
        if self.entry_count % 100 == 0 {
            self.writer.flush()?;
        }

        // Check if rotation needed
        if self.current_size >= self.max_file_size {
            self.rotate()?;
        }

        Ok(())
    }

    /// Flush to disk
    pub fn flush(&mut self) -> Result<()> {
        self.writer.flush()?;
        Ok(())
    }

    /// Rotate journal
    pub fn rotate(&mut self) -> Result<()> {
        self.flush()?;

        // Generate archive name with timestamp
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let archive_name = format!("journal-{}.gz", timestamp);
        let archive_path = self.dir.join(&archive_name);

        // Compress current journal
        info!("Rotating journal to {}", archive_name);

        let input = File::open(&self.current_file)?;
        let output = File::create(&archive_path)?;

        let mut encoder = flate2::write::GzEncoder::new(output, flate2::Compression::default());
        let mut reader = BufReader::new(input);
        let mut buffer = Vec::new();

        reader.read_to_end(&mut buffer)?;
        encoder.write_all(&buffer)?;
        encoder.finish()?;

        // Truncate current file
        let file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&self.current_file)?;

        self.writer = BufWriter::new(file);
        self.current_size = 0;

        // Clean old archives
        self.cleanup_old_archives()?;

        Ok(())
    }

    fn cleanup_old_archives(&self) -> Result<()> {
        let retention = chrono::Duration::days(30);
        let cutoff = Utc::now() - retention;

        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) == Some("gz") {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        let modified: DateTime<Utc> = modified.into();
                        if modified < cutoff {
                            info!("Removing old journal: {:?}", path);
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Query journal entries
    pub fn query(&self, filter: &JournalFilter) -> Result<Vec<LogEntry>> {
        let mut entries = Vec::new();

        // Read current journal
        let file = File::open(&self.current_file)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            if let Ok(entry) = serde_json::from_str::<LogEntry>(&line) {
                if filter.matches(&entry) {
                    entries.push(entry);
                }
            }
        }

        // Read archived journals if time range requires
        if filter.since.is_some() {
            for entry in fs::read_dir(&self.dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension().and_then(|e| e.to_str()) == Some("gz") {
                    let file = File::open(&path)?;
                    let decoder = flate2::read::GzDecoder::new(file);
                    let reader = BufReader::new(decoder);

                    for line in reader.lines() {
                        let line = line?;
                        if let Ok(entry) = serde_json::from_str::<LogEntry>(&line) {
                            if filter.matches(&entry) {
                                entries.push(entry);
                            }
                        }
                    }
                }
            }
        }

        // Sort by timestamp
        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        // Apply limit
        if let Some(limit) = filter.limit {
            if filter.reverse {
                entries = entries.into_iter().rev().take(limit).collect();
            } else {
                entries.truncate(limit);
            }
        }

        Ok(entries)
    }
}

/// Journal query filter
#[derive(Debug, Clone, Default)]
pub struct JournalFilter {
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub priority: Option<Priority>,
    pub facility: Option<Facility>,
    pub identifier: Option<String>,
    pub pid: Option<u32>,
    pub grep: Option<String>,
    pub limit: Option<usize>,
    pub reverse: bool,
}

impl JournalFilter {
    pub fn matches(&self, entry: &LogEntry) -> bool {
        if let Some(since) = &self.since {
            if entry.timestamp < *since {
                return false;
            }
        }

        if let Some(until) = &self.until {
            if entry.timestamp > *until {
                return false;
            }
        }

        if let Some(priority) = &self.priority {
            if entry.priority > *priority {
                return false;
            }
        }

        if let Some(facility) = &self.facility {
            if entry.facility != *facility {
                return false;
            }
        }

        if let Some(identifier) = &self.identifier {
            if !entry.identifier.contains(identifier) {
                return false;
            }
        }

        if let Some(pid) = &self.pid {
            if entry.pid != Some(*pid) {
                return false;
            }
        }

        if let Some(grep) = &self.grep {
            if !entry.message.contains(grep) {
                return false;
            }
        }

        true
    }
}
