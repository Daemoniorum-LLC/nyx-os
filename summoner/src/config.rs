//! Summoner configuration

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummonerConfig {
    #[serde(default = "default_app_dirs")]
    pub app_directories: Vec<PathBuf>,
    #[serde(default)]
    pub search: SearchConfig,
    #[serde(default)]
    pub recent: RecentConfig,
    #[serde(default)]
    pub custom_apps: Vec<CustomApp>,
}

impl Default for SummonerConfig {
    fn default() -> Self {
        Self {
            app_directories: default_app_dirs(),
            search: SearchConfig::default(),
            recent: RecentConfig::default(),
            custom_apps: Vec::new(),
        }
    }
}

fn default_app_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        dirs::data_dir().unwrap_or_default().join("applications"),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    #[serde(default = "default_true")]
    pub fuzzy: bool,
    #[serde(default = "default_true")]
    pub search_description: bool,
    #[serde(default = "default_true")]
    pub search_keywords: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            max_results: default_max_results(),
            fuzzy: true,
            search_description: true,
            search_keywords: true,
        }
    }
}

fn default_max_results() -> usize { 20 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_recent_size")]
    pub max_size: usize,
    #[serde(default = "default_true")]
    pub boost_recent: bool,
}

impl Default for RecentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_size: default_recent_size(),
            boost_recent: true,
        }
    }
}

fn default_recent_size() -> usize { 50 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomApp {
    pub name: String,
    pub exec: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

fn default_true() -> bool { true }

pub fn load_config(path: &Path) -> Result<SummonerConfig> {
    if path.exists() {
        let contents = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&contents)?)
    } else {
        Ok(SummonerConfig::default())
    }
}
