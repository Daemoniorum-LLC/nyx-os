//! Grimoire Settings configuration

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GrimoireConfig {
    #[serde(default)]
    pub watch: WatchConfig,
    #[serde(default)]
    pub validation: ValidationConfig,
    #[serde(default)]
    pub migration: MigrationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_debounce")]
    pub debounce_ms: u64,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self { enabled: true, debounce_ms: default_debounce() }
    }
}

fn default_debounce() -> u64 { 100 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub strict: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self { enabled: true, strict: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    #[serde(default = "default_true")]
    pub auto_migrate: bool,
    #[serde(default = "default_true")]
    pub backup_before: bool,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self { auto_migrate: true, backup_before: true }
    }
}

fn default_true() -> bool { true }
