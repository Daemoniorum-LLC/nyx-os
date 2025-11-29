//! Umbra configuration

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UmbraConfig {
    #[serde(default)]
    pub prompt: PromptConfig,
    #[serde(default)]
    pub history: HistoryConfig,
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub aliases: Vec<Alias>,
    #[serde(default)]
    pub environment: Vec<EnvVar>,
}

impl Default for UmbraConfig {
    fn default() -> Self {
        Self {
            prompt: PromptConfig::default(),
            history: HistoryConfig::default(),
            ai: AiConfig::default(),
            aliases: default_aliases(),
            environment: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptConfig {
    #[serde(default = "default_prompt")]
    pub format: String,
    #[serde(default)]
    pub show_git: bool,
    #[serde(default)]
    pub show_time: bool,
    #[serde(default = "default_true")]
    pub colors: bool,
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            format: default_prompt(),
            show_git: true,
            show_time: false,
            colors: true,
        }
    }
}

fn default_prompt() -> String {
    "{user}@{host}:{cwd}$ ".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryConfig {
    #[serde(default = "default_history_size")]
    pub max_size: usize,
    #[serde(default = "default_history_file")]
    pub file: String,
    #[serde(default = "default_true")]
    pub ignore_duplicates: bool,
    #[serde(default = "default_true")]
    pub ignore_space: bool,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_size: default_history_size(),
            file: default_history_file(),
            ignore_duplicates: true,
            ignore_space: true,
        }
    }
}

fn default_history_size() -> usize { 10000 }
fn default_history_file() -> String { "~/.umbra_history".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_persona")]
    pub default_persona: String,
    #[serde(default)]
    pub auto_suggest: bool,
    #[serde(default = "default_true")]
    pub explain_errors: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_persona: default_persona(),
            auto_suggest: false,
            explain_errors: true,
        }
    }
}

fn default_persona() -> String { "shell-assistant".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alias {
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

fn default_true() -> bool { true }

fn default_aliases() -> Vec<Alias> {
    vec![
        Alias { name: "ll".into(), command: "ls -la".into() },
        Alias { name: "la".into(), command: "ls -a".into() },
        Alias { name: "..".into(), command: "cd ..".into() },
        Alias { name: "...".into(), command: "cd ../..".into() },
    ]
}

pub fn load_config(path: Option<&Path>) -> Result<UmbraConfig> {
    let config_path = path.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("~/.config"))
            .join("umbra/config.yaml")
    });

    if config_path.exists() {
        let contents = std::fs::read_to_string(&config_path)?;
        Ok(serde_yaml::from_str(&contents)?)
    } else {
        Ok(UmbraConfig::default())
    }
}
