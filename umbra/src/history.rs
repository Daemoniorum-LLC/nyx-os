//! Command history management

use crate::config::HistoryConfig;
use anyhow::Result;
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// History manager with persistence
pub struct History {
    entries: VecDeque<String>,
    config: HistoryConfig,
    file_path: PathBuf,
    position: usize,
    search_mode: bool,
    search_pattern: String,
    modified: bool,
}

impl History {
    pub fn new(config: &HistoryConfig) -> Result<Self> {
        let file_path = expand_path(&config.file);

        let mut history = Self {
            entries: VecDeque::new(),
            config: config.clone(),
            file_path,
            position: 0,
            search_mode: false,
            search_pattern: String::new(),
            modified: false,
        };

        history.load()?;
        Ok(history)
    }

    /// Load history from file
    fn load(&mut self) -> Result<()> {
        if self.file_path.exists() {
            let file = File::open(&self.file_path)?;
            let reader = BufReader::new(file);

            for line in reader.lines() {
                if let Ok(entry) = line {
                    if !entry.is_empty() {
                        self.entries.push_back(entry);
                    }
                }
            }

            // Trim to max size
            while self.entries.len() > self.config.max_size {
                self.entries.pop_front();
            }
        }

        self.position = self.entries.len();
        Ok(())
    }

    /// Save history to file
    pub fn save(&mut self) -> Result<()> {
        if !self.modified {
            return Ok(());
        }

        // Ensure directory exists
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.file_path)?;

        for entry in &self.entries {
            writeln!(file, "{}", entry)?;
        }

        self.modified = false;
        Ok(())
    }

    /// Add a command to history
    pub fn add(&mut self, command: &str) -> Result<()> {
        let command = command.trim().to_string();

        // Skip empty commands
        if command.is_empty() {
            return Ok(());
        }

        // Skip commands starting with space if configured
        if self.config.ignore_space && command.starts_with(' ') {
            return Ok(());
        }

        // Skip duplicates if configured
        if self.config.ignore_duplicates {
            if let Some(last) = self.entries.back() {
                if *last == command {
                    return Ok(());
                }
            }
        }

        self.entries.push_back(command);

        // Trim to max size
        while self.entries.len() > self.config.max_size {
            self.entries.pop_front();
        }

        self.position = self.entries.len();
        self.modified = true;

        Ok(())
    }

    /// Get the previous entry (up arrow)
    pub fn previous(&mut self) -> Option<&str> {
        if self.search_mode {
            return self.search_previous();
        }

        if self.position > 0 {
            self.position -= 1;
            self.entries.get(self.position).map(|s| s.as_str())
        } else {
            None
        }
    }

    /// Get the next entry (down arrow)
    pub fn next(&mut self) -> Option<&str> {
        if self.search_mode {
            return self.search_next();
        }

        if self.position < self.entries.len() {
            self.position += 1;
            if self.position == self.entries.len() {
                None // Return to current input
            } else {
                self.entries.get(self.position).map(|s| s.as_str())
            }
        } else {
            None
        }
    }

    /// Reset position to end
    pub fn reset_position(&mut self) {
        self.position = self.entries.len();
        self.search_mode = false;
        self.search_pattern.clear();
    }

    /// Start reverse search (Ctrl+R)
    pub fn start_search(&mut self, pattern: &str) {
        self.search_mode = true;
        self.search_pattern = pattern.to_string();
        self.position = self.entries.len();
    }

    /// Update search pattern
    pub fn update_search(&mut self, pattern: &str) -> Option<&str> {
        self.search_pattern = pattern.to_string();
        self.search_previous()
    }

    /// Search previous match
    fn search_previous(&mut self) -> Option<&str> {
        let start = if self.position > 0 { self.position - 1 } else { return None };

        for i in (0..=start).rev() {
            if let Some(entry) = self.entries.get(i) {
                if entry.contains(&self.search_pattern) {
                    self.position = i;
                    return Some(entry.as_str());
                }
            }
        }

        None
    }

    /// Search next match
    fn search_next(&mut self) -> Option<&str> {
        for i in (self.position + 1)..self.entries.len() {
            if let Some(entry) = self.entries.get(i) {
                if entry.contains(&self.search_pattern) {
                    self.position = i;
                    return Some(entry.as_str());
                }
            }
        }

        None
    }

    /// Stop search mode
    pub fn stop_search(&mut self) -> Option<&str> {
        self.search_mode = false;
        self.search_pattern.clear();
        self.entries.get(self.position).map(|s| s.as_str())
    }

    /// Get all entries
    pub fn entries(&self) -> Vec<String> {
        self.entries.iter().cloned().collect()
    }

    /// Get entry at index
    pub fn get(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(|s| s.as_str())
    }

    /// Search entries containing pattern
    pub fn search(&self, pattern: &str) -> Vec<(usize, &str)> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.contains(pattern))
            .map(|(i, e)| (i, e.as_str()))
            .collect()
    }

    /// Get last n entries
    pub fn last_n(&self, n: usize) -> Vec<&str> {
        self.entries
            .iter()
            .rev()
            .take(n)
            .map(|s| s.as_str())
            .collect()
    }

    /// Clear history
    pub fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        self.position = 0;
        self.modified = true;
        self.save()
    }

    /// Get total entry count
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Is currently in search mode
    pub fn is_searching(&self) -> bool {
        self.search_mode
    }

    /// Current search pattern
    pub fn search_pattern(&self) -> &str {
        &self.search_pattern
    }
}

impl Drop for History {
    fn drop(&mut self) {
        let _ = self.save();
    }
}

/// Expand ~ and environment variables in path
fn expand_path(path: &str) -> PathBuf {
    let expanded = if path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            path.replacen('~', home.to_string_lossy().as_ref(), 1)
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    };

    PathBuf::from(expanded)
}

/// Session history for undo/redo support
pub struct SessionHistory {
    states: Vec<String>,
    position: usize,
}

impl SessionHistory {
    pub fn new() -> Self {
        Self {
            states: vec![String::new()],
            position: 0,
        }
    }

    pub fn push(&mut self, state: &str) {
        // Remove any redo states
        self.states.truncate(self.position + 1);

        // Add new state if different from current
        if self.states.last() != Some(&state.to_string()) {
            self.states.push(state.to_string());
            self.position = self.states.len() - 1;
        }
    }

    pub fn undo(&mut self) -> Option<&str> {
        if self.position > 0 {
            self.position -= 1;
            Some(&self.states[self.position])
        } else {
            None
        }
    }

    pub fn redo(&mut self) -> Option<&str> {
        if self.position < self.states.len() - 1 {
            self.position += 1;
            Some(&self.states[self.position])
        } else {
            None
        }
    }

    pub fn current(&self) -> &str {
        &self.states[self.position]
    }
}

impl Default for SessionHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_navigation() {
        let config = HistoryConfig {
            max_size: 100,
            file: "/tmp/test_history".to_string(),
            ignore_duplicates: true,
            ignore_space: true,
        };

        let mut history = History::new(&config).unwrap();
        history.add("first").unwrap();
        history.add("second").unwrap();
        history.add("third").unwrap();

        assert_eq!(history.previous(), Some("third"));
        assert_eq!(history.previous(), Some("second"));
        assert_eq!(history.previous(), Some("first"));
        assert_eq!(history.next(), Some("second"));
    }
}
