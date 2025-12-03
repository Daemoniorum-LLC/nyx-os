//! Command types and execution for Nyx Assistant

use serde::{Deserialize, Serialize};

/// A command result from search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    /// Unique identifier
    pub id: String,
    /// Display title
    pub title: String,
    /// Subtitle/description
    pub subtitle: Option<String>,
    /// Icon (text/emoji or icon name)
    pub icon: String,
    /// Command type
    pub kind: CommandKind,
    /// Keywords for search
    pub keywords: Vec<String>,
    /// Score from fuzzy matching
    #[serde(skip)]
    pub score: i64,
}

/// Types of commands
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandKind {
    /// Launch an application
    Application,
    /// Open a file
    File,
    /// Open a folder
    Folder,
    /// System command
    System,
    /// Calculator result
    Calculator,
    /// Web search
    WebSearch,
    /// AI query
    AiQuery,
    /// Recent file/app
    Recent,
    /// Settings page
    Settings,
}

impl CommandResult {
    /// Create a new application command
    pub fn app(id: impl Into<String>, name: impl Into<String>, icon: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: name.into(),
            subtitle: Some("Application".to_string()),
            icon: icon.into(),
            kind: CommandKind::Application,
            keywords: vec![],
            score: 0,
        }
    }

    /// Create a system command
    pub fn system(
        id: impl Into<String>,
        title: impl Into<String>,
        subtitle: impl Into<String>,
        icon: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: Some(subtitle.into()),
            icon: icon.into(),
            kind: CommandKind::System,
            keywords: vec![],
            score: 0,
        }
    }

    /// Create a calculator result
    pub fn calculator(expression: impl Into<String>, result: impl Into<String>) -> Self {
        Self {
            id: "calc".to_string(),
            title: result.into(),
            subtitle: Some(expression.into()),
            icon: "󰃬".to_string(),
            kind: CommandKind::Calculator,
            keywords: vec![],
            score: 1000,
        }
    }

    /// Create a web search suggestion
    pub fn web_search(query: impl Into<String>) -> Self {
        let q = query.into();
        Self {
            id: format!("search:{}", q),
            title: format!("Search for \"{}\"", q),
            subtitle: Some("Web Search".to_string()),
            icon: "󰍉".to_string(),
            kind: CommandKind::WebSearch,
            keywords: vec![],
            score: -100,
        }
    }

    /// Create an AI query suggestion
    pub fn ai_query(query: impl Into<String>) -> Self {
        let q = query.into();
        Self {
            id: format!("ai:{}", q),
            title: format!("Ask: \"{}\"", q),
            subtitle: Some("AI Assistant".to_string()),
            icon: "󰚩".to_string(),
            kind: CommandKind::AiQuery,
            keywords: vec![],
            score: -50,
        }
    }

    /// Create a settings page command
    pub fn settings(
        id: impl Into<String>,
        title: impl Into<String>,
        subtitle: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: Some(subtitle.into()),
            icon: "󰒓".to_string(),
            kind: CommandKind::Settings,
            keywords: vec![],
            score: 0,
        }
    }

    /// Add keywords for search
    pub fn with_keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords;
        self
    }

    /// Set the match score
    pub fn with_score(mut self, score: i64) -> Self {
        self.score = score;
        self
    }
}

/// Get built-in system commands
pub fn system_commands() -> Vec<CommandResult> {
    vec![
        CommandResult::system("lock", "Lock Screen", "Lock your computer", "󰌾"),
        CommandResult::system("sleep", "Sleep", "Put computer to sleep", "󰤄"),
        CommandResult::system("restart", "Restart", "Restart your computer", "󰜉"),
        CommandResult::system("shutdown", "Shut Down", "Turn off your computer", "󰐥"),
        CommandResult::system("logout", "Log Out", "Log out of your session", "󰍃"),
        CommandResult::system("settings", "System Settings", "Open system settings", "󰒓"),
        CommandResult::system("terminal", "Terminal", "Open a terminal", "󰆍"),
        CommandResult::system("files", "Files", "Open file manager", "󰉋"),
        CommandResult::system("wifi", "WiFi Settings", "Manage WiFi connections", "󰤨"),
        CommandResult::system("bluetooth", "Bluetooth Settings", "Manage Bluetooth devices", "󰂯"),
        CommandResult::system("display", "Display Settings", "Configure displays", "󰍹"),
        CommandResult::system("sound", "Sound Settings", "Configure audio", "󰕾"),
        CommandResult::system("updates", "Software Updates", "Check for updates", "󰚰"),
    ]
}

/// Get sample applications (in real implementation, scan .desktop files)
pub fn sample_applications() -> Vec<CommandResult> {
    vec![
        CommandResult::app("umbra", "Umbra Terminal", "󰆍")
            .with_keywords(vec!["terminal".into(), "shell".into(), "console".into()]),
        CommandResult::app("nyx-browser", "Nyx Browser", "󰈹")
            .with_keywords(vec!["web".into(), "internet".into(), "chrome".into()]),
        CommandResult::app("nyx-files", "Files", "󰉋")
            .with_keywords(vec!["folder".into(), "nautilus".into(), "explorer".into()]),
        CommandResult::app("nyx-code", "Nyx Code", "󰨞")
            .with_keywords(vec!["editor".into(), "vscode".into(), "ide".into()]),
        CommandResult::app("nyx-settings", "Settings", "󰒓")
            .with_keywords(vec!["preferences".into(), "config".into()]),
        CommandResult::app("nyx-mail", "Mail", "󰇮")
            .with_keywords(vec!["email".into(), "thunderbird".into()]),
        CommandResult::app("nyx-calendar", "Calendar", "󰃭")
            .with_keywords(vec!["schedule".into(), "events".into()]),
        CommandResult::app("nyx-music", "Music", "󰝚")
            .with_keywords(vec!["audio".into(), "spotify".into(), "player".into()]),
        CommandResult::app("nyx-photos", "Photos", "󰋩")
            .with_keywords(vec!["images".into(), "gallery".into(), "pictures".into()]),
        CommandResult::app("nyx-notes", "Notes", "󱞎")
            .with_keywords(vec!["text".into(), "memo".into()]),
    ]
}

/// Simple math expression evaluator
pub fn evaluate_expression(expr: &str) -> Option<f64> {
    // Simple tokenizer and parser for basic math
    let expr = expr.trim().replace(" ", "");

    // Handle simple expressions like "2+2", "10*5", etc.
    if let Some(pos) = expr.rfind('+') {
        let (left, right) = expr.split_at(pos);
        let left: f64 = left.parse().ok()?;
        let right: f64 = right[1..].parse().ok()?;
        return Some(left + right);
    }

    if let Some(pos) = expr.rfind('-') {
        if pos > 0 {
            let (left, right) = expr.split_at(pos);
            let left: f64 = left.parse().ok()?;
            let right: f64 = right[1..].parse().ok()?;
            return Some(left - right);
        }
    }

    if let Some(pos) = expr.find('*') {
        let (left, right) = expr.split_at(pos);
        let left: f64 = left.parse().ok()?;
        let right: f64 = right[1..].parse().ok()?;
        return Some(left * right);
    }

    if let Some(pos) = expr.find('/') {
        let (left, right) = expr.split_at(pos);
        let left: f64 = left.parse().ok()?;
        let right: f64 = right[1..].parse().ok()?;
        if right != 0.0 {
            return Some(left / right);
        }
    }

    if let Some(pos) = expr.find('^') {
        let (left, right) = expr.split_at(pos);
        let left: f64 = left.parse().ok()?;
        let right: f64 = right[1..].parse().ok()?;
        return Some(left.powf(right));
    }

    if let Some(pos) = expr.find('%') {
        let (left, right) = expr.split_at(pos);
        let left: f64 = left.parse().ok()?;
        let right: f64 = right[1..].parse().ok()?;
        return Some(left % right);
    }

    // Special functions
    if expr.starts_with("sqrt(") && expr.ends_with(')') {
        let inner = &expr[5..expr.len() - 1];
        let val: f64 = inner.parse().ok()?;
        return Some(val.sqrt());
    }

    // Just a number
    expr.parse().ok()
}
