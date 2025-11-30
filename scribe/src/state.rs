//! Scribe daemon state

use crate::journal::Journal;

/// Daemon state
pub struct ScribeState {
    pub journal: Journal,
    pub config: ScribeConfig,
}

#[derive(Clone)]
pub struct ScribeConfig {
    pub journal_dir: String,
    pub max_file_size: u64,
    pub retention_days: u32,
}
