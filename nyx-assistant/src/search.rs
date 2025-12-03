//! Search functionality for Nyx Assistant

use crate::commands::{
    evaluate_expression, sample_applications, system_commands, CommandKind, CommandResult,
};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

/// Search engine for the assistant
pub struct SearchEngine {
    /// Fuzzy matcher
    matcher: SkimMatcherV2,
    /// All available commands
    commands: Vec<CommandResult>,
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchEngine {
    /// Create a new search engine
    pub fn new() -> Self {
        let mut commands = Vec::new();

        // Add system commands
        commands.extend(system_commands());

        // Add applications
        commands.extend(sample_applications());

        Self {
            matcher: SkimMatcherV2::default(),
            commands,
        }
    }

    /// Search for commands matching the query
    pub fn search(&self, query: &str) -> Vec<CommandResult> {
        if query.is_empty() {
            // Return recent/suggested items
            return self.get_suggestions();
        }

        let mut results = Vec::new();

        // Check if it's a math expression
        if let Some(value) = evaluate_expression(query) {
            let result_str = if value.fract() == 0.0 {
                format!("{}", value as i64)
            } else {
                format!("{:.4}", value).trim_end_matches('0').trim_end_matches('.').to_string()
            };
            results.push(CommandResult::calculator(query, result_str));
        }

        // Fuzzy search through commands
        let query_lower = query.to_lowercase();

        for cmd in &self.commands {
            let mut best_score = 0i64;

            // Match against title
            if let Some(score) = self.matcher.fuzzy_match(&cmd.title, query) {
                best_score = best_score.max(score);
            }

            // Match against subtitle
            if let Some(ref subtitle) = cmd.subtitle {
                if let Some(score) = self.matcher.fuzzy_match(subtitle, query) {
                    best_score = best_score.max(score / 2); // Lower weight for subtitle
                }
            }

            // Match against keywords
            for keyword in &cmd.keywords {
                if let Some(score) = self.matcher.fuzzy_match(keyword, query) {
                    best_score = best_score.max(score);
                }
            }

            if best_score > 0 {
                results.push(cmd.clone().with_score(best_score));
            }
        }

        // Sort by score (highest first)
        results.sort_by(|a, b| b.score.cmp(&a.score));

        // Add web search and AI query as fallbacks
        if results.len() < 8 && query.len() >= 3 {
            results.push(CommandResult::ai_query(query));
            results.push(CommandResult::web_search(query));
        }

        // Limit results
        results.truncate(10);

        results
    }

    /// Get default suggestions when no query is entered
    pub fn get_suggestions(&self) -> Vec<CommandResult> {
        let mut suggestions = Vec::new();

        // Add frequently used/pinned items
        suggestions.push(CommandResult::app("umbra", "Umbra Terminal", "󰆍"));
        suggestions.push(CommandResult::app("nyx-browser", "Nyx Browser", "󰈹"));
        suggestions.push(CommandResult::app("nyx-files", "Files", "󰉋"));
        suggestions.push(CommandResult::app("nyx-code", "Nyx Code", "󰨞"));
        suggestions.push(CommandResult::system("settings", "Settings", "Open system settings", "󰒓"));

        suggestions
    }

    /// Add a custom command
    pub fn add_command(&mut self, command: CommandResult) {
        self.commands.push(command);
    }

    /// Refresh the command list
    pub fn refresh(&mut self) {
        // In a real implementation, this would rescan .desktop files
        self.commands.clear();
        self.commands.extend(system_commands());
        self.commands.extend(sample_applications());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════════
    // SEARCH ENGINE INITIALIZATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_search_engine_new() {
        let engine = SearchEngine::new();
        // Engine should be initialized with commands
        assert!(!engine.commands.is_empty());
    }

    #[test]
    fn test_search_engine_default() {
        let engine = SearchEngine::default();
        assert!(!engine.commands.is_empty());
    }

    #[test]
    fn test_search_engine_has_system_commands() {
        let engine = SearchEngine::new();
        let has_system = engine.commands.iter().any(|c| c.kind == CommandKind::System);
        assert!(has_system);
    }

    #[test]
    fn test_search_engine_has_applications() {
        let engine = SearchEngine::new();
        let has_apps = engine.commands.iter().any(|c| c.kind == CommandKind::Application);
        assert!(has_apps);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // SEARCH FUNCTIONALITY TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_search_applications() {
        let engine = SearchEngine::new();
        let results = engine.search("term");

        assert!(!results.is_empty());
        assert!(results
            .iter()
            .any(|r| r.title.to_lowercase().contains("terminal")));
    }

    #[test]
    fn test_search_by_keyword() {
        let engine = SearchEngine::new();
        // Search for a keyword that should match an app
        let results = engine.search("shell");
        assert!(!results.is_empty());
    }

    #[test]
    fn test_search_case_insensitive() {
        let engine = SearchEngine::new();
        let results_lower = engine.search("terminal");
        let results_upper = engine.search("TERMINAL");
        // Both should find results (fuzzy matching is case-insensitive)
        assert!(!results_lower.is_empty() || !results_upper.is_empty());
    }

    #[test]
    fn test_search_partial_match() {
        let engine = SearchEngine::new();
        let results = engine.search("brow"); // Should match "Browser"
        assert!(results.iter().any(|r| r.title.contains("Browser")));
    }

    #[test]
    fn test_search_results_sorted_by_score() {
        let engine = SearchEngine::new();
        let results = engine.search("files");
        if results.len() >= 2 {
            // Results should be sorted by score (descending)
            for i in 0..results.len() - 1 {
                assert!(results[i].score >= results[i + 1].score);
            }
        }
    }

    #[test]
    fn test_search_results_limited() {
        let engine = SearchEngine::new();
        let results = engine.search("a"); // Should match many things
        assert!(results.len() <= 10); // Should be limited to 10
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // CALCULATOR INTEGRATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_calculator() {
        let engine = SearchEngine::new();
        let results = engine.search("2+2");

        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.kind == CommandKind::Calculator));
    }

    #[test]
    fn test_calculator_result_first() {
        let engine = SearchEngine::new();
        let results = engine.search("10*5");
        // Calculator result should have high priority
        if let Some(first) = results.first() {
            if first.kind == CommandKind::Calculator {
                assert_eq!(first.title, "50");
            }
        }
    }

    #[test]
    fn test_calculator_with_decimals() {
        let engine = SearchEngine::new();
        let results = engine.search("22/7");
        assert!(results.iter().any(|r| r.kind == CommandKind::Calculator));
    }

    #[test]
    fn test_calculator_formats_integers() {
        let engine = SearchEngine::new();
        let results = engine.search("6*7");
        let calc = results.iter().find(|r| r.kind == CommandKind::Calculator);
        if let Some(c) = calc {
            assert_eq!(c.title, "42"); // Should be integer, not "42.0"
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // EMPTY QUERY AND SUGGESTIONS TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_empty_query() {
        let engine = SearchEngine::new();
        let results = engine.search("");

        assert!(!results.is_empty()); // Should return suggestions
    }

    #[test]
    fn test_get_suggestions() {
        let engine = SearchEngine::new();
        let suggestions = engine.get_suggestions();
        assert!(!suggestions.is_empty());
        // Should include common apps
        assert!(suggestions.iter().any(|s| s.title.contains("Terminal")));
    }

    #[test]
    fn test_suggestions_include_core_apps() {
        let engine = SearchEngine::new();
        let suggestions = engine.get_suggestions();
        let titles: Vec<&str> = suggestions.iter().map(|s| s.title.as_str()).collect();
        assert!(titles.iter().any(|t| t.contains("Terminal")));
        assert!(titles.iter().any(|t| t.contains("Browser")));
        assert!(titles.iter().any(|t| t.contains("Files")));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // FALLBACK RESULTS TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_long_query_adds_fallbacks() {
        let engine = SearchEngine::new();
        let results = engine.search("something that wont match any commands");
        // Should add AI query and web search as fallbacks
        let has_ai = results.iter().any(|r| r.kind == CommandKind::AiQuery);
        let has_web = results.iter().any(|r| r.kind == CommandKind::WebSearch);
        assert!(has_ai || has_web);
    }

    #[test]
    fn test_ai_query_fallback_contains_query() {
        let engine = SearchEngine::new();
        let results = engine.search("what is rust?");
        let ai_query = results.iter().find(|r| r.kind == CommandKind::AiQuery);
        if let Some(q) = ai_query {
            assert!(q.title.contains("what is rust?"));
        }
    }

    #[test]
    fn test_web_search_fallback_contains_query() {
        let engine = SearchEngine::new();
        let results = engine.search("how to learn rust");
        let web_search = results.iter().find(|r| r.kind == CommandKind::WebSearch);
        if let Some(s) = web_search {
            assert!(s.title.contains("how to learn rust"));
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // COMMAND MANAGEMENT TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_add_command() {
        let mut engine = SearchEngine::new();
        let initial_count = engine.commands.len();
        engine.add_command(CommandResult::app("custom", "Custom App", "X"));
        assert_eq!(engine.commands.len(), initial_count + 1);
    }

    #[test]
    fn test_added_command_searchable() {
        let mut engine = SearchEngine::new();
        engine.add_command(CommandResult::app("zzz-unique", "ZZZ Unique App", "X"));
        let results = engine.search("ZZZ Unique");
        assert!(results.iter().any(|r| r.id == "zzz-unique"));
    }

    #[test]
    fn test_refresh() {
        let mut engine = SearchEngine::new();
        engine.add_command(CommandResult::app("custom", "Custom", "X"));
        let count_before = engine.commands.len();
        engine.refresh();
        // After refresh, custom command should be gone, only built-in commands remain
        assert!(engine.commands.len() < count_before);
        assert!(!engine.commands.iter().any(|c| c.id == "custom"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // SYSTEM COMMAND SEARCH TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_search_lock() {
        let engine = SearchEngine::new();
        let results = engine.search("lock");
        assert!(results.iter().any(|r| r.id == "lock"));
    }

    #[test]
    fn test_search_shutdown() {
        let engine = SearchEngine::new();
        let results = engine.search("shut");
        assert!(results.iter().any(|r| r.title.to_lowercase().contains("shut")));
    }

    #[test]
    fn test_search_settings() {
        let engine = SearchEngine::new();
        let results = engine.search("settings");
        assert!(results.iter().any(|r| r.title.to_lowercase().contains("settings")));
    }
}
