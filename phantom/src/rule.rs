//! Device rule matching and processing

use crate::device::Device;
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::path::Path;
use tracing::{info, warn, debug};

/// A device rule
#[derive(Debug, Clone)]
pub struct Rule {
    /// Rule name/comment
    pub name: Option<String>,
    /// Match conditions
    pub conditions: Vec<RuleCondition>,
    /// Actions to perform
    pub actions: Vec<RuleAction>,
    /// Priority (lower runs first)
    pub priority: i32,
}

/// Rule match condition
#[derive(Debug, Clone)]
pub enum RuleCondition {
    /// Match subsystem
    Subsystem(String),
    /// Match kernel name
    Kernel(String),
    /// Match driver
    Driver(String),
    /// Match device type
    DevType(String),
    /// Match attribute
    Attr(String, String),
    /// Match property (from uevent)
    Property(String, String),
    /// Match parent attribute
    ParentAttr(String, String),
    /// Match tag
    Tag(String),
    /// Match action (add, remove, change)
    Action(String),
}

/// Rule action
#[derive(Debug, Clone)]
pub enum RuleAction {
    /// Set device node name
    Name(String),
    /// Create symlink
    Symlink(String),
    /// Set permissions (octal)
    Mode(u32),
    /// Set owner
    Owner(String),
    /// Set group
    Group(String),
    /// Run program
    Run(String),
    /// Add tag
    Tag(String),
    /// Set environment variable
    Env(String, String),
}

impl Rule {
    /// Check if rule matches device
    pub fn matches(&self, device: &Device) -> bool {
        for condition in &self.conditions {
            if !condition.matches(device) {
                return false;
            }
        }
        true
    }
}

impl RuleCondition {
    fn matches(&self, device: &Device) -> bool {
        match self {
            RuleCondition::Subsystem(pattern) => {
                device.subsystem.as_ref()
                    .map(|s| pattern_match(s, pattern))
                    .unwrap_or(false)
            }
            RuleCondition::Kernel(pattern) => {
                pattern_match(&device.sysname, pattern)
            }
            RuleCondition::Driver(pattern) => {
                device.driver.as_ref()
                    .map(|d| pattern_match(d, pattern))
                    .unwrap_or(false)
            }
            RuleCondition::DevType(pattern) => {
                device.devtype.as_ref()
                    .map(|t| pattern_match(t, pattern))
                    .unwrap_or(false)
            }
            RuleCondition::Attr(key, pattern) => {
                device.attributes.get(key)
                    .map(|v| pattern_match(v, pattern))
                    .unwrap_or(false)
            }
            RuleCondition::Property(key, pattern) => {
                device.properties.get(key)
                    .map(|v| pattern_match(v, pattern))
                    .unwrap_or(false)
            }
            RuleCondition::ParentAttr(_, _) => {
                // Would need to look up parent device
                false
            }
            RuleCondition::Tag(tag) => {
                device.has_tag(tag)
            }
            RuleCondition::Action(_) => {
                // Action is checked separately at event time
                true
            }
        }
    }
}

/// Collection of rules
pub struct RuleSet {
    rules: Vec<Rule>,
}

impl RuleSet {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Load rules from a directory
    pub fn load_directory(&mut self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }

        let mut files: Vec<_> = std::fs::read_dir(path)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext == "rules")
                    .unwrap_or(false)
            })
            .collect();

        // Sort by filename for predictable order
        files.sort_by_key(|e| e.file_name());

        for entry in files {
            if let Err(e) = self.load_file(&entry.path()) {
                warn!("Failed to load {:?}: {}", entry.path(), e);
            }
        }

        // Sort by priority
        self.rules.sort_by_key(|r| r.priority);

        Ok(())
    }

    /// Load rules from a file
    pub fn load_file(&mut self, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        // Extract priority from filename (e.g., "10-storage.rules" -> 10)
        let priority: i32 = filename
            .split('-')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50);

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            match parse_rule(line, priority) {
                Ok(rule) => {
                    debug!("Loaded rule from {}:{}", filename, line_num + 1);
                    self.rules.push(rule);
                }
                Err(e) => {
                    warn!("Parse error in {}:{}: {}", filename, line_num + 1, e);
                }
            }
        }

        info!("Loaded rules from {:?}", path);
        Ok(())
    }

    /// Find rules matching a device
    pub fn find_matches(&self, device: &Device) -> Vec<&Rule> {
        self.rules.iter()
            .filter(|r| r.matches(device))
            .collect()
    }

    /// Add a rule
    pub fn add(&mut self, rule: Rule) {
        self.rules.push(rule);
        self.rules.sort_by_key(|r| r.priority);
    }

    /// Get rule count
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

impl Default for RuleSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a udev-style rule line
fn parse_rule(line: &str, priority: i32) -> Result<Rule> {
    let mut conditions = Vec::new();
    let mut actions = Vec::new();

    // Simple parser for key==value, key=value pairs
    for part in line.split(',') {
        let part = part.trim();

        if let Some((key, value)) = part.split_once("==") {
            // Match condition
            let condition = parse_condition(key.trim(), value.trim())?;
            conditions.push(condition);
        } else if let Some((key, value)) = part.split_once("=") {
            // Check for != (not equal)
            if key.ends_with('!') {
                // Would handle != conditions
                continue;
            }

            // Assignment action
            let action = parse_action(key.trim(), value.trim())?;
            actions.push(action);
        }
    }

    if conditions.is_empty() && actions.is_empty() {
        return Err(anyhow!("Empty rule"));
    }

    Ok(Rule {
        name: None,
        conditions,
        actions,
        priority,
    })
}

fn parse_condition(key: &str, value: &str) -> Result<RuleCondition> {
    // Remove quotes from value
    let value = value.trim_matches('"');

    let condition = match key.to_uppercase().as_str() {
        "SUBSYSTEM" => RuleCondition::Subsystem(value.to_string()),
        "KERNEL" => RuleCondition::Kernel(value.to_string()),
        "DRIVER" => RuleCondition::Driver(value.to_string()),
        "DEVTYPE" => RuleCondition::DevType(value.to_string()),
        "ACTION" => RuleCondition::Action(value.to_string()),
        "TAG" => RuleCondition::Tag(value.to_string()),
        key if key.starts_with("ATTR{") => {
            let attr = key.strip_prefix("ATTR{")
                .and_then(|s| s.strip_suffix('}'))
                .ok_or_else(|| anyhow!("Invalid ATTR syntax"))?;
            RuleCondition::Attr(attr.to_string(), value.to_string())
        }
        key if key.starts_with("ENV{") => {
            let env = key.strip_prefix("ENV{")
                .and_then(|s| s.strip_suffix('}'))
                .ok_or_else(|| anyhow!("Invalid ENV syntax"))?;
            RuleCondition::Property(env.to_string(), value.to_string())
        }
        _ => return Err(anyhow!("Unknown condition: {}", key)),
    };

    Ok(condition)
}

fn parse_action(key: &str, value: &str) -> Result<RuleAction> {
    // Remove quotes from value
    let value = value.trim_matches('"');

    let action = match key.to_uppercase().as_str() {
        "NAME" => RuleAction::Name(value.to_string()),
        "SYMLINK" => RuleAction::Symlink(value.to_string()),
        "MODE" => {
            let mode = u32::from_str_radix(value, 8)
                .map_err(|_| anyhow!("Invalid mode: {}", value))?;
            RuleAction::Mode(mode)
        }
        "OWNER" => RuleAction::Owner(value.to_string()),
        "GROUP" => RuleAction::Group(value.to_string()),
        "RUN" => RuleAction::Run(value.to_string()),
        "TAG" => RuleAction::Tag(value.to_string()),
        key if key.starts_with("ENV{") => {
            let env = key.strip_prefix("ENV{")
                .and_then(|s| s.strip_suffix('}'))
                .ok_or_else(|| anyhow!("Invalid ENV syntax"))?;
            RuleAction::Env(env.to_string(), value.to_string())
        }
        _ => return Err(anyhow!("Unknown action: {}", key)),
    };

    Ok(action)
}

/// Pattern matching (supports * and ?)
fn pattern_match(value: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let mut pattern_chars = pattern.chars().peekable();
    let mut value_chars = value.chars().peekable();

    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                // Match zero or more characters
                if pattern_chars.peek().is_none() {
                    return true;
                }
                // Try matching rest of pattern at each position
                while value_chars.peek().is_some() {
                    let remaining_pattern: String = pattern_chars.clone().collect();
                    let remaining_value: String = value_chars.clone().collect();
                    if pattern_match(&remaining_value, &remaining_pattern) {
                        return true;
                    }
                    value_chars.next();
                }
                let remaining_pattern: String = pattern_chars.collect();
                return pattern_match("", &remaining_pattern);
            }
            '?' => {
                // Match exactly one character
                if value_chars.next().is_none() {
                    return false;
                }
            }
            c => {
                if value_chars.next() != Some(c) {
                    return false;
                }
            }
        }
    }

    value_chars.peek().is_none()
}

/// Run a program with device environment
pub async fn run_program(command: &str, device: &Device) -> Result<()> {
    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c");
    cmd.arg(command);

    // Set device environment variables
    cmd.env("DEVPATH", &device.devpath);
    cmd.env("SUBSYSTEM", device.subsystem.as_deref().unwrap_or(""));
    cmd.env("DEVNAME", device.devnode.as_deref().unwrap_or(""));

    for (key, value) in &device.properties {
        cmd.env(key, value);
    }

    let status = cmd.status().await?;

    if !status.success() {
        warn!("Program failed with {}: {}", status, command);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_match() {
        assert!(pattern_match("sda", "*"));
        assert!(pattern_match("sda1", "sda*"));
        assert!(pattern_match("sda", "sda"));
        assert!(pattern_match("sda1", "sd?1"));
        assert!(!pattern_match("sdb", "sda*"));
    }

    #[test]
    fn test_parse_rule() {
        let rule = parse_rule(
            r#"SUBSYSTEM=="block", KERNEL=="sd*", MODE="0660", GROUP="disk""#,
            50,
        ).unwrap();

        assert_eq!(rule.conditions.len(), 2);
        assert_eq!(rule.actions.len(), 2);
    }
}
