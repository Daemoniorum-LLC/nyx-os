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

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════════
    // COMMAND KIND TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_command_kind_equality() {
        assert_eq!(CommandKind::Application, CommandKind::Application);
        assert_ne!(CommandKind::Application, CommandKind::File);
        assert_ne!(CommandKind::Calculator, CommandKind::WebSearch);
    }

    #[test]
    fn test_command_kind_copy() {
        let kind = CommandKind::System;
        let copy = kind;
        assert_eq!(kind, copy);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // COMMAND RESULT CONSTRUCTOR TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_app_constructor() {
        let app = CommandResult::app("test-app", "Test App", "󰀄");
        assert_eq!(app.id, "test-app");
        assert_eq!(app.title, "Test App");
        assert_eq!(app.subtitle, Some("Application".to_string()));
        assert_eq!(app.icon, "󰀄");
        assert_eq!(app.kind, CommandKind::Application);
        assert_eq!(app.score, 0);
        assert!(app.keywords.is_empty());
    }

    #[test]
    fn test_system_constructor() {
        let cmd = CommandResult::system("lock", "Lock Screen", "Lock your computer", "󰌾");
        assert_eq!(cmd.id, "lock");
        assert_eq!(cmd.title, "Lock Screen");
        assert_eq!(cmd.subtitle, Some("Lock your computer".to_string()));
        assert_eq!(cmd.icon, "󰌾");
        assert_eq!(cmd.kind, CommandKind::System);
    }

    #[test]
    fn test_calculator_constructor() {
        let calc = CommandResult::calculator("2+2", "4");
        assert_eq!(calc.id, "calc");
        assert_eq!(calc.title, "4");
        assert_eq!(calc.subtitle, Some("2+2".to_string()));
        assert_eq!(calc.icon, "󰃬");
        assert_eq!(calc.kind, CommandKind::Calculator);
        assert_eq!(calc.score, 1000); // High priority for calculator results
    }

    #[test]
    fn test_web_search_constructor() {
        let search = CommandResult::web_search("rust programming");
        assert_eq!(search.id, "search:rust programming");
        assert_eq!(search.title, "Search for \"rust programming\"");
        assert_eq!(search.subtitle, Some("Web Search".to_string()));
        assert_eq!(search.icon, "󰍉");
        assert_eq!(search.kind, CommandKind::WebSearch);
        assert_eq!(search.score, -100); // Low priority fallback
    }

    #[test]
    fn test_ai_query_constructor() {
        let query = CommandResult::ai_query("what is rust?");
        assert_eq!(query.id, "ai:what is rust?");
        assert_eq!(query.title, "Ask: \"what is rust?\"");
        assert_eq!(query.subtitle, Some("AI Assistant".to_string()));
        assert_eq!(query.icon, "󰚩");
        assert_eq!(query.kind, CommandKind::AiQuery);
        assert_eq!(query.score, -50); // Higher than web search but still fallback
    }

    #[test]
    fn test_settings_constructor() {
        let settings = CommandResult::settings("display", "Display", "Configure monitor settings");
        assert_eq!(settings.id, "display");
        assert_eq!(settings.title, "Display");
        assert_eq!(settings.subtitle, Some("Configure monitor settings".to_string()));
        assert_eq!(settings.icon, "󰒓");
        assert_eq!(settings.kind, CommandKind::Settings);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // BUILDER PATTERN TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_with_keywords() {
        let app = CommandResult::app("test", "Test", "󰀄")
            .with_keywords(vec!["keyword1".into(), "keyword2".into()]);
        assert_eq!(app.keywords.len(), 2);
        assert!(app.keywords.contains(&"keyword1".to_string()));
        assert!(app.keywords.contains(&"keyword2".to_string()));
    }

    #[test]
    fn test_with_score() {
        let app = CommandResult::app("test", "Test", "󰀄").with_score(500);
        assert_eq!(app.score, 500);
    }

    #[test]
    fn test_chained_builders() {
        let app = CommandResult::app("test", "Test", "󰀄")
            .with_keywords(vec!["key".into()])
            .with_score(100);
        assert_eq!(app.keywords, vec!["key".to_string()]);
        assert_eq!(app.score, 100);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // SYSTEM COMMANDS TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_system_commands_not_empty() {
        let commands = system_commands();
        assert!(!commands.is_empty());
    }

    #[test]
    fn test_system_commands_all_have_system_kind() {
        let commands = system_commands();
        for cmd in &commands {
            assert_eq!(cmd.kind, CommandKind::System);
        }
    }

    #[test]
    fn test_system_commands_contain_expected() {
        let commands = system_commands();
        let ids: Vec<&str> = commands.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"lock"));
        assert!(ids.contains(&"shutdown"));
        assert!(ids.contains(&"restart"));
        assert!(ids.contains(&"settings"));
        assert!(ids.contains(&"terminal"));
    }

    #[test]
    fn test_system_commands_have_subtitles() {
        let commands = system_commands();
        for cmd in &commands {
            assert!(cmd.subtitle.is_some());
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // SAMPLE APPLICATIONS TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_sample_applications_not_empty() {
        let apps = sample_applications();
        assert!(!apps.is_empty());
    }

    #[test]
    fn test_sample_applications_all_have_application_kind() {
        let apps = sample_applications();
        for app in &apps {
            assert_eq!(app.kind, CommandKind::Application);
        }
    }

    #[test]
    fn test_sample_applications_have_keywords() {
        let apps = sample_applications();
        // Most applications should have keywords for better search
        let with_keywords = apps.iter().filter(|a| !a.keywords.is_empty()).count();
        assert!(with_keywords >= apps.len() / 2); // At least half have keywords
    }

    #[test]
    fn test_sample_applications_contain_core_apps() {
        let apps = sample_applications();
        let ids: Vec<&str> = apps.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"umbra")); // Terminal
        assert!(ids.contains(&"nyx-files")); // File manager
        assert!(ids.contains(&"nyx-settings")); // Settings
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // EXPRESSION EVALUATOR TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_evaluate_addition() {
        assert_eq!(evaluate_expression("2+2"), Some(4.0));
        assert_eq!(evaluate_expression("10+5"), Some(15.0));
        assert_eq!(evaluate_expression("0+100"), Some(100.0));
    }

    #[test]
    fn test_evaluate_subtraction() {
        assert_eq!(evaluate_expression("10-3"), Some(7.0));
        assert_eq!(evaluate_expression("100-50"), Some(50.0));
        assert_eq!(evaluate_expression("5-10"), Some(-5.0));
    }

    #[test]
    fn test_evaluate_multiplication() {
        assert_eq!(evaluate_expression("3*4"), Some(12.0));
        assert_eq!(evaluate_expression("10*10"), Some(100.0));
        assert_eq!(evaluate_expression("0*999"), Some(0.0));
    }

    #[test]
    fn test_evaluate_division() {
        assert_eq!(evaluate_expression("10/2"), Some(5.0));
        assert_eq!(evaluate_expression("9/3"), Some(3.0));
        assert_eq!(evaluate_expression("1/4"), Some(0.25));
    }

    #[test]
    fn test_evaluate_division_by_zero() {
        assert_eq!(evaluate_expression("10/0"), None);
    }

    #[test]
    fn test_evaluate_power() {
        assert_eq!(evaluate_expression("2^3"), Some(8.0));
        assert_eq!(evaluate_expression("10^2"), Some(100.0));
        assert_eq!(evaluate_expression("5^0"), Some(1.0));
    }

    #[test]
    fn test_evaluate_modulo() {
        assert_eq!(evaluate_expression("10%3"), Some(1.0));
        assert_eq!(evaluate_expression("15%5"), Some(0.0));
        assert_eq!(evaluate_expression("7%4"), Some(3.0));
    }

    #[test]
    fn test_evaluate_sqrt() {
        assert_eq!(evaluate_expression("sqrt(16)"), Some(4.0));
        assert_eq!(evaluate_expression("sqrt(25)"), Some(5.0));
        assert_eq!(evaluate_expression("sqrt(2)"), Some(2.0_f64.sqrt()));
    }

    #[test]
    fn test_evaluate_plain_number() {
        assert_eq!(evaluate_expression("42"), Some(42.0));
        assert_eq!(evaluate_expression("3.14"), Some(3.14));
        assert_eq!(evaluate_expression("0"), Some(0.0));
    }

    #[test]
    fn test_evaluate_with_spaces() {
        assert_eq!(evaluate_expression("2 + 2"), Some(4.0));
        assert_eq!(evaluate_expression(" 10 * 5 "), Some(50.0));
    }

    #[test]
    fn test_evaluate_invalid_expression() {
        assert_eq!(evaluate_expression("hello"), None);
        assert_eq!(evaluate_expression("abc+def"), None);
        assert_eq!(evaluate_expression(""), None);
    }

    #[test]
    fn test_evaluate_decimal_operations() {
        assert_eq!(evaluate_expression("1.5+1.5"), Some(3.0));
        assert_eq!(evaluate_expression("2.5*2"), Some(5.0));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // SERIALIZATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_command_result_serialization() {
        let cmd = CommandResult::app("test", "Test App", "󰀄");
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("Test App"));
    }

    #[test]
    fn test_command_result_deserialization() {
        let json = r#"{"id":"test","title":"Test","subtitle":null,"icon":"X","kind":"Application","keywords":[]}"#;
        let cmd: CommandResult = serde_json::from_str(json).unwrap();
        assert_eq!(cmd.id, "test");
        assert_eq!(cmd.title, "Test");
        assert_eq!(cmd.kind, CommandKind::Application);
    }

    #[test]
    fn test_command_kind_serialization() {
        let kind = CommandKind::Calculator;
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.contains("Calculator"));
    }

    #[test]
    fn test_score_not_serialized() {
        let cmd = CommandResult::app("test", "Test", "X").with_score(999);
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(!json.contains("999")); // Score is marked #[serde(skip)]
    }
}
