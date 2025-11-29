//! Tab completion engine with AI assistance

use crate::config::UmbraConfig;
use crate::history::History;
use anyhow::Result;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Completion result
#[derive(Debug, Clone)]
pub struct Completion {
    pub text: String,
    pub display: String,
    pub kind: CompletionKind,
    pub score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompletionKind {
    Command,
    File,
    Directory,
    History,
    Alias,
    Variable,
    Builtin,
    AiSuggestion,
}

/// Completion engine
pub struct Completer {
    command_cache: Vec<String>,
    cache_valid: bool,
    builtins: Vec<&'static str>,
}

impl Completer {
    pub fn new() -> Self {
        Self {
            command_cache: Vec::new(),
            cache_valid: false,
            builtins: vec![
                "cd", "pwd", "export", "unset", "exit", "jobs",
                "history", "alias", "source", ".", "type", "echo",
                "fg", "bg", "wait", "read", "test", "[", "true", "false",
            ],
        }
    }

    /// Complete input at cursor position
    pub fn complete(
        &mut self,
        input: &str,
        cursor: usize,
        config: &UmbraConfig,
        history: &History,
    ) -> Result<Vec<Completion>> {
        let (prefix, word, is_first_word) = self.parse_input(input, cursor);

        let mut completions = Vec::new();

        if is_first_word {
            // Complete commands
            completions.extend(self.complete_commands(&word, config)?);
            completions.extend(self.complete_builtins(&word));
            completions.extend(self.complete_aliases(&word, config));
        } else {
            // Check for variable completion
            if word.starts_with('$') {
                completions.extend(self.complete_variables(&word[1..]));
            } else {
                // Complete files/directories
                completions.extend(self.complete_paths(&word)?);
            }
        }

        // Add history-based completions
        completions.extend(self.complete_from_history(input, history));

        // Sort by score
        completions.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        // Deduplicate
        let mut seen = std::collections::HashSet::new();
        completions.retain(|c| seen.insert(c.text.clone()));

        // Limit results
        completions.truncate(50);

        Ok(completions)
    }

    fn parse_input(&self, input: &str, cursor: usize) -> (String, String, bool) {
        let relevant = &input[..cursor.min(input.len())];
        let parts: Vec<&str> = relevant.split_whitespace().collect();

        let is_first = parts.len() <= 1 && !relevant.ends_with(' ');
        let current_word = if relevant.ends_with(' ') {
            String::new()
        } else {
            parts.last().unwrap_or(&"").to_string()
        };

        let prefix = if let Some(idx) = relevant.rfind(char::is_whitespace) {
            relevant[..idx + 1].to_string()
        } else {
            String::new()
        };

        (prefix, current_word, is_first)
    }

    fn complete_commands(
        &mut self,
        prefix: &str,
        _config: &UmbraConfig,
    ) -> Result<Vec<Completion>> {
        // Refresh command cache if needed
        if !self.cache_valid {
            self.refresh_command_cache();
        }

        let completions: Vec<Completion> = self.command_cache
            .iter()
            .filter(|cmd| cmd.starts_with(prefix))
            .map(|cmd| Completion {
                text: cmd.clone(),
                display: cmd.clone(),
                kind: CompletionKind::Command,
                score: 1.0 - (cmd.len() as f64 - prefix.len() as f64) / 100.0,
            })
            .collect();

        Ok(completions)
    }

    fn refresh_command_cache(&mut self) {
        self.command_cache.clear();

        if let Ok(path) = env::var("PATH") {
            for dir in path.split(':') {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        if let Ok(metadata) = entry.metadata() {
                            if metadata.is_file() && is_executable(&entry.path()) {
                                if let Some(name) = entry.file_name().to_str() {
                                    self.command_cache.push(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        self.command_cache.sort();
        self.command_cache.dedup();
        self.cache_valid = true;
    }

    fn complete_builtins(&self, prefix: &str) -> Vec<Completion> {
        self.builtins
            .iter()
            .filter(|b| b.starts_with(prefix))
            .map(|b| Completion {
                text: b.to_string(),
                display: format!("{} (builtin)", b),
                kind: CompletionKind::Builtin,
                score: 1.5, // Prefer builtins
            })
            .collect()
    }

    fn complete_aliases(&self, prefix: &str, config: &UmbraConfig) -> Vec<Completion> {
        config.aliases
            .iter()
            .filter(|a| a.name.starts_with(prefix))
            .map(|a| Completion {
                text: a.name.clone(),
                display: format!("{} -> {}", a.name, a.command),
                kind: CompletionKind::Alias,
                score: 1.3, // Prefer aliases over commands
            })
            .collect()
    }

    fn complete_paths(&self, prefix: &str) -> Result<Vec<Completion>> {
        let mut completions = Vec::new();

        // Expand tilde
        let expanded = if prefix.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                prefix.replacen('~', home.to_string_lossy().as_ref(), 1)
            } else {
                prefix.to_string()
            }
        } else {
            prefix.to_string()
        };

        let (dir, file_prefix) = if expanded.contains('/') {
            let path = PathBuf::from(&expanded);
            if expanded.ends_with('/') {
                (path, String::new())
            } else {
                (
                    path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from(".")),
                    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                )
            }
        } else {
            (PathBuf::from("."), expanded.clone())
        };

        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(&file_prefix) || file_prefix.is_empty() {
                        // Skip hidden files unless prefix starts with .
                        if name.starts_with('.') && !file_prefix.starts_with('.') {
                            continue;
                        }

                        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        let completion_text = if prefix.contains('/') {
                            format!("{}/{}{}",
                                prefix.rsplit_once('/').map(|(p, _)| p).unwrap_or(""),
                                name,
                                if is_dir { "/" } else { "" }
                            )
                        } else {
                            format!("{}{}", name, if is_dir { "/" } else { "" })
                        };

                        completions.push(Completion {
                            text: completion_text,
                            display: if is_dir {
                                format!("{}/", name)
                            } else {
                                name.to_string()
                            },
                            kind: if is_dir { CompletionKind::Directory } else { CompletionKind::File },
                            score: if is_dir { 1.1 } else { 1.0 },
                        });
                    }
                }
            }
        }

        Ok(completions)
    }

    fn complete_variables(&self, prefix: &str) -> Vec<Completion> {
        let mut completions = Vec::new();

        // Environment variables
        for (key, _) in env::vars() {
            if key.starts_with(prefix) {
                completions.push(Completion {
                    text: format!("${}", key),
                    display: format!("${} (env)", key),
                    kind: CompletionKind::Variable,
                    score: 1.0,
                });
            }
        }

        // Special variables
        let special = ["?", "$", "!", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9",
                      "@", "*", "#", "PWD", "HOME", "USER", "SHELL"];
        for var in special {
            if var.starts_with(prefix) {
                completions.push(Completion {
                    text: format!("${}", var),
                    display: format!("${} (special)", var),
                    kind: CompletionKind::Variable,
                    score: 1.2,
                });
            }
        }

        completions
    }

    fn complete_from_history(&self, input: &str, history: &History) -> Vec<Completion> {
        history.entries()
            .iter()
            .filter(|h| h.starts_with(input) && *h != input)
            .take(5)
            .map(|h| Completion {
                text: h.clone(),
                display: format!("(history) {}", h),
                kind: CompletionKind::History,
                score: 0.8, // Lower priority than direct completions
            })
            .collect()
    }

    /// Invalidate the command cache (e.g., after PATH change)
    pub fn invalidate_cache(&mut self) {
        self.cache_valid = false;
    }
}

impl Default for Completer {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a file is executable
fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = path.metadata() {
            let permissions = metadata.permissions();
            return permissions.mode() & 0o111 != 0;
        }
    }
    false
}

/// AI-powered completion suggestions
pub struct AiCompleter {
    enabled: bool,
    context_window: Vec<String>,
}

impl AiCompleter {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            context_window: Vec::new(),
        }
    }

    pub fn add_context(&mut self, command: &str) {
        self.context_window.push(command.to_string());
        if self.context_window.len() > 20 {
            self.context_window.remove(0);
        }
    }

    /// Get AI-powered suggestions based on context
    pub async fn suggest(&self, input: &str) -> Vec<Completion> {
        if !self.enabled || input.len() < 3 {
            return Vec::new();
        }

        // In a real implementation, this would call a persona
        // For now, return pattern-based suggestions
        let mut suggestions = Vec::new();

        // Common command patterns
        let patterns: HashMap<&str, Vec<&str>> = [
            ("git ", vec!["git status", "git add .", "git commit -m \"", "git push", "git pull"]),
            ("docker ", vec!["docker ps", "docker images", "docker build .", "docker compose up"]),
            ("npm ", vec!["npm install", "npm run dev", "npm run build", "npm test"]),
            ("cargo ", vec!["cargo build", "cargo test", "cargo run", "cargo check"]),
        ].into_iter().collect();

        for (prefix, cmds) in patterns {
            if input.starts_with(prefix) {
                for cmd in cmds {
                    if cmd.starts_with(input) {
                        suggestions.push(Completion {
                            text: cmd.to_string(),
                            display: format!("✨ {}", cmd),
                            kind: CompletionKind::AiSuggestion,
                            score: 0.7,
                        });
                    }
                }
            }
        }

        suggestions
    }
}
