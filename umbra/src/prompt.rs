//! Prompt rendering and customization

use crate::config::PromptConfig;
use anyhow::Result;
use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Prompt renderer with variable substitution
pub struct Prompt {
    config: PromptConfig,
}

impl Prompt {
    pub fn new(config: PromptConfig) -> Self {
        Self { config }
    }

    /// Render the prompt string
    pub fn render(&self, cwd: &PathBuf, last_exit_code: i32) -> String {
        let mut result = self.config.format.clone();

        // Basic substitutions
        result = result.replace("{user}", &get_username());
        result = result.replace("{host}", &get_hostname());
        result = result.replace("{cwd}", &format_cwd(cwd));
        result = result.replace("{cwd_full}", &cwd.to_string_lossy());
        result = result.replace("{home}", &get_home_display(cwd));

        // Exit code
        if last_exit_code != 0 {
            result = result.replace("{status}", &format!("[{}]", last_exit_code));
            result = result.replace("{status_color}", &colorize("red", &format!("{}", last_exit_code)));
        } else {
            result = result.replace("{status}", "");
            result = result.replace("{status_color}", "");
        }

        // Time
        if self.config.show_time {
            let time = chrono::Local::now().format("%H:%M:%S").to_string();
            result = result.replace("{time}", &time);
        } else {
            result = result.replace("{time}", "");
        }

        // Git info
        if self.config.show_git {
            result = result.replace("{git}", &get_git_info(cwd));
            result = result.replace("{git_branch}", &get_git_branch(cwd).unwrap_or_default());
        } else {
            result = result.replace("{git}", "");
            result = result.replace("{git_branch}", "");
        }

        // Shell indicator
        let shell_char = if is_root() { "#" } else { "$" };
        result = result.replace("{$}", shell_char);

        // Colors (if enabled)
        if self.config.colors {
            result = apply_colors(&result);
        } else {
            result = strip_color_codes(&result);
        }

        result
    }

    /// Render continuation prompt (PS2)
    pub fn render_continuation(&self) -> String {
        if self.config.colors {
            colorize("yellow", "> ")
        } else {
            "> ".to_string()
        }
    }

    /// Render select prompt (PS3)
    pub fn render_select(&self) -> String {
        "#? ".to_string()
    }

    /// Update configuration
    pub fn set_config(&mut self, config: PromptConfig) {
        self.config = config;
    }
}

fn get_username() -> String {
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string())
}

fn get_hostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "localhost".to_string())
}

fn format_cwd(cwd: &PathBuf) -> String {
    let cwd_str = cwd.to_string_lossy();

    // Replace home directory with ~
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        if cwd_str.starts_with(home_str.as_ref()) {
            return cwd_str.replacen(home_str.as_ref(), "~", 1);
        }
    }

    cwd_str.to_string()
}

fn get_home_display(cwd: &PathBuf) -> String {
    if let Some(home) = dirs::home_dir() {
        if cwd.starts_with(&home) {
            return "~".to_string();
        }
    }
    format_cwd(cwd)
}

fn is_root() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::getuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn get_git_info(cwd: &PathBuf) -> String {
    let branch = match get_git_branch(cwd) {
        Some(b) => b,
        None => return String::new(),
    };

    let status = get_git_status(cwd);

    let mut info = format!("({})", branch);

    if !status.is_clean {
        let mut indicators = Vec::new();
        if status.staged > 0 {
            indicators.push(format!("+{}", status.staged));
        }
        if status.modified > 0 {
            indicators.push(format!("~{}", status.modified));
        }
        if status.untracked > 0 {
            indicators.push(format!("?{}", status.untracked));
        }
        if !indicators.is_empty() {
            info.push_str(&format!(" {}", indicators.join(" ")));
        }
    }

    if status.ahead > 0 {
        info.push_str(&format!(" ↑{}", status.ahead));
    }
    if status.behind > 0 {
        info.push_str(&format!(" ↓{}", status.behind));
    }

    format!(" {}", info)
}

fn get_git_branch(cwd: &PathBuf) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

struct GitStatus {
    is_clean: bool,
    staged: usize,
    modified: usize,
    untracked: usize,
    ahead: usize,
    behind: usize,
}

fn get_git_status(cwd: &PathBuf) -> GitStatus {
    let mut status = GitStatus {
        is_clean: true,
        staged: 0,
        modified: 0,
        untracked: 0,
        ahead: 0,
        behind: 0,
    };

    // Get porcelain status
    if let Ok(output) = Command::new("git")
        .args(["status", "--porcelain", "-b"])
        .current_dir(cwd)
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.starts_with("##") {
                    // Branch line with ahead/behind info
                    if let Some(ahead_behind) = line.split('[').nth(1) {
                        if ahead_behind.contains("ahead") {
                            if let Some(num) = ahead_behind
                                .split("ahead ")
                                .nth(1)
                                .and_then(|s| s.split(|c| c == ',' || c == ']').next())
                                .and_then(|s| s.trim().parse().ok())
                            {
                                status.ahead = num;
                            }
                        }
                        if ahead_behind.contains("behind") {
                            if let Some(num) = ahead_behind
                                .split("behind ")
                                .nth(1)
                                .and_then(|s| s.split(']').next())
                                .and_then(|s| s.trim().parse().ok())
                            {
                                status.behind = num;
                            }
                        }
                    }
                } else if !line.is_empty() {
                    status.is_clean = false;
                    let chars: Vec<char> = line.chars().collect();
                    if chars.len() >= 2 {
                        match chars[0] {
                            'A' | 'M' | 'D' | 'R' | 'C' => status.staged += 1,
                            '?' => status.untracked += 1,
                            _ => {}
                        }
                        if chars[1] == 'M' || chars[1] == 'D' {
                            status.modified += 1;
                        }
                    }
                }
            }
        }
    }

    status
}

/// Apply ANSI color codes
fn apply_colors(text: &str) -> String {
    let mut result = text.to_string();

    // Color tags: {color:text} or {color}text{/color}
    let colors = [
        ("red", "\x1b[31m"),
        ("green", "\x1b[32m"),
        ("yellow", "\x1b[33m"),
        ("blue", "\x1b[34m"),
        ("magenta", "\x1b[35m"),
        ("cyan", "\x1b[36m"),
        ("white", "\x1b[37m"),
        ("bold", "\x1b[1m"),
        ("dim", "\x1b[2m"),
        ("reset", "\x1b[0m"),
    ];

    for (name, code) in colors {
        result = result.replace(&format!("{{{}}}", name), code);
        result = result.replace(&format!("{{/{}}}", name), "\x1b[0m");
    }

    result
}

fn strip_color_codes(text: &str) -> String {
    let mut result = text.to_string();

    let tags = ["red", "green", "yellow", "blue", "magenta", "cyan", "white", "bold", "dim", "reset"];
    for tag in tags {
        result = result.replace(&format!("{{{}}}", tag), "");
        result = result.replace(&format!("{{/{}}}", tag), "");
    }

    result
}

fn colorize(color: &str, text: &str) -> String {
    let code = match color {
        "red" => "\x1b[31m",
        "green" => "\x1b[32m",
        "yellow" => "\x1b[33m",
        "blue" => "\x1b[34m",
        "magenta" => "\x1b[35m",
        "cyan" => "\x1b[36m",
        "white" => "\x1b[37m",
        "bold" => "\x1b[1m",
        _ => "",
    };

    format!("{}{}\x1b[0m", code, text)
}

/// Predefined prompt themes
pub enum Theme {
    Minimal,
    Default,
    Powerline,
    TwoLine,
}

impl Theme {
    pub fn format(&self) -> String {
        match self {
            Theme::Minimal => "{cwd}{$} ".to_string(),
            Theme::Default => "{user}@{host}:{cwd}{git}{$} ".to_string(),
            Theme::Powerline => "{bold}{blue}{cwd}{/bold} {green}{git}{/green}{status_color}\n{cyan}❯{/cyan} ".to_string(),
            Theme::TwoLine => "{bold}{user}{/bold}@{host} {blue}{cwd}{/blue}{git}\n{time} {$} ".to_string(),
        }
    }
}
