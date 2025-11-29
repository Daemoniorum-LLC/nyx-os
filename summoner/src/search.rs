//! Fuzzy search implementation

use crate::config::SearchConfig;
use crate::index::{AppIndex, IndexedApp};
use anyhow::Result;

/// Search engine with fuzzy matching
pub struct SearchEngine {
    config: SearchConfig,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub app: IndexedApp,
    pub score: f64,
    pub match_type: MatchType,
    pub highlights: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MatchType {
    Exact,
    Prefix,
    Substring,
    Fuzzy,
    Keyword,
}

impl SearchEngine {
    pub fn new(config: SearchConfig) -> Self {
        Self { config }
    }

    /// Search for applications
    pub async fn search(&self, query: &str, index: &AppIndex) -> Vec<SearchResult> {
        if query.is_empty() {
            return Vec::new();
        }

        let query_lower = query.to_lowercase();
        let apps = index.all().await;
        let mut results: Vec<SearchResult> = Vec::new();

        for app in apps {
            if let Some(result) = self.match_app(&query_lower, &app) {
                results.push(result);
            }
        }

        // Sort by score
        results.sort_by(|a, b| {
            b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply result limit
        results.truncate(self.config.max_results);

        results
    }

    fn match_app(&self, query: &str, app: &IndexedApp) -> Option<SearchResult> {
        let name_lower = app.entry.name.to_lowercase();

        // Exact match
        if name_lower == query {
            return Some(SearchResult {
                app: app.clone(),
                score: 100.0 * app.score,
                match_type: MatchType::Exact,
                highlights: vec![(0, query.len())],
            });
        }

        // Prefix match
        if name_lower.starts_with(query) {
            return Some(SearchResult {
                app: app.clone(),
                score: 90.0 * app.score,
                match_type: MatchType::Prefix,
                highlights: vec![(0, query.len())],
            });
        }

        // Substring match
        if let Some(pos) = name_lower.find(query) {
            return Some(SearchResult {
                app: app.clone(),
                score: 70.0 * app.score,
                match_type: MatchType::Substring,
                highlights: vec![(pos, pos + query.len())],
            });
        }

        // Keyword match
        if self.config.search_keywords {
            for keyword in &app.entry.keywords {
                let kw_lower = keyword.to_lowercase();
                if kw_lower.starts_with(query) || kw_lower.contains(query) {
                    return Some(SearchResult {
                        app: app.clone(),
                        score: 60.0 * app.score,
                        match_type: MatchType::Keyword,
                        highlights: Vec::new(),
                    });
                }
            }
        }

        // Description match
        if self.config.search_description {
            if let Some(ref desc) = app.entry.comment {
                if desc.to_lowercase().contains(query) {
                    return Some(SearchResult {
                        app: app.clone(),
                        score: 40.0 * app.score,
                        match_type: MatchType::Substring,
                        highlights: Vec::new(),
                    });
                }
            }
        }

        // Fuzzy match
        if self.config.fuzzy {
            if let Some(score) = self.fuzzy_match(query, &name_lower) {
                if score > 0.5 {
                    return Some(SearchResult {
                        app: app.clone(),
                        score: score * 50.0 * app.score,
                        match_type: MatchType::Fuzzy,
                        highlights: self.get_fuzzy_highlights(query, &name_lower),
                    });
                }
            }
        }

        None
    }

    /// Fuzzy string matching using Smith-Waterman inspired algorithm
    fn fuzzy_match(&self, pattern: &str, text: &str) -> Option<f64> {
        if pattern.is_empty() {
            return Some(1.0);
        }
        if text.is_empty() {
            return None;
        }

        let pattern_chars: Vec<char> = pattern.chars().collect();
        let text_chars: Vec<char> = text.chars().collect();

        let mut pattern_idx = 0;
        let mut consecutive_bonus = 0.0;
        let mut total_score = 0.0;
        let mut last_match_idx: Option<usize> = None;

        for (text_idx, &text_char) in text_chars.iter().enumerate() {
            if pattern_idx < pattern_chars.len() && text_char == pattern_chars[pattern_idx] {
                // Base score for match
                let mut score = 1.0;

                // Consecutive match bonus
                if let Some(last_idx) = last_match_idx {
                    if text_idx == last_idx + 1 {
                        consecutive_bonus += 0.5;
                        score += consecutive_bonus;
                    } else {
                        consecutive_bonus = 0.0;
                    }
                }

                // Word boundary bonus
                if text_idx == 0 || text_chars[text_idx - 1] == ' ' || text_chars[text_idx - 1] == '-' {
                    score += 1.0;
                }

                // Capital letter bonus
                if text_char.is_uppercase() {
                    score += 0.5;
                }

                total_score += score;
                last_match_idx = Some(text_idx);
                pattern_idx += 1;
            }
        }

        // All pattern characters must match
        if pattern_idx == pattern_chars.len() {
            let max_score = (pattern_chars.len() as f64) * 3.0; // Max possible score
            Some(total_score / max_score)
        } else {
            None
        }
    }

    fn get_fuzzy_highlights(&self, pattern: &str, text: &str) -> Vec<(usize, usize)> {
        let mut highlights = Vec::new();
        let pattern_chars: Vec<char> = pattern.chars().collect();
        let text_chars: Vec<char> = text.chars().collect();

        let mut pattern_idx = 0;
        let mut byte_offset = 0;

        for (text_idx, &text_char) in text_chars.iter().enumerate() {
            if pattern_idx < pattern_chars.len() && text_char == pattern_chars[pattern_idx] {
                let char_len = text_char.len_utf8();
                highlights.push((byte_offset, byte_offset + char_len));
                pattern_idx += 1;
            }
            byte_offset += text_char.len_utf8();
        }

        // Merge consecutive highlights
        let mut merged = Vec::new();
        for (start, end) in highlights {
            if let Some((_, last_end)) = merged.last_mut() {
                if *last_end == start {
                    *last_end = end;
                    continue;
                }
            }
            merged.push((start, end));
        }

        merged
    }
}

/// Quick filter for initial candidate selection
pub fn quick_filter(apps: &[IndexedApp], query: &str) -> Vec<&IndexedApp> {
    let query_lower = query.to_lowercase();
    let first_char = query_lower.chars().next();

    apps.iter()
        .filter(|app| {
            // Quick first character check
            if let Some(fc) = first_char {
                let name_lower = app.entry.name.to_lowercase();
                if !name_lower.chars().any(|c| c == fc) {
                    return false;
                }
            }
            true
        })
        .collect()
}

/// Trigram-based similarity for spell correction
pub fn trigram_similarity(a: &str, b: &str) -> f64 {
    let trigrams_a = get_trigrams(a);
    let trigrams_b = get_trigrams(b);

    if trigrams_a.is_empty() || trigrams_b.is_empty() {
        return 0.0;
    }

    let common: usize = trigrams_a.iter()
        .filter(|t| trigrams_b.contains(t))
        .count();

    let total = trigrams_a.len() + trigrams_b.len() - common;

    if total == 0 {
        0.0
    } else {
        common as f64 / total as f64
    }
}

fn get_trigrams(s: &str) -> Vec<String> {
    let padded = format!("  {}  ", s.to_lowercase());
    let chars: Vec<char> = padded.chars().collect();

    if chars.len() < 3 {
        return Vec::new();
    }

    (0..chars.len() - 2)
        .map(|i| chars[i..i + 3].iter().collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigram_similarity() {
        assert!(trigram_similarity("firefox", "firefoxx") > 0.8);
        assert!(trigram_similarity("firefox", "chrome") < 0.3);
    }
}
