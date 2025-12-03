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
    fn test_calculator() {
        let engine = SearchEngine::new();
        let results = engine.search("2+2");

        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.kind == CommandKind::Calculator));
    }

    #[test]
    fn test_empty_query() {
        let engine = SearchEngine::new();
        let results = engine.search("");

        assert!(!results.is_empty()); // Should return suggestions
    }
}
