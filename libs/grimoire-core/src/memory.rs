//! Persona memory types and management
//!
//! Each persona maintains isolated, encrypted memory. Memory is persisted
//! through the Cipher daemon in DaemonOS.

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::PersonaId;

/// Memory entry representing a single piece of information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique entry ID
    pub id: Uuid,
    /// When this was created
    pub timestamp: DateTime<Utc>,
    /// Type of memory entry
    pub entry_type: MemoryEntryType,
    /// Content of the memory
    pub content: String,
    /// Additional metadata
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
    /// Importance score (0.0 - 1.0) for pruning decisions
    pub importance: f32,
    /// Number of times this memory was recalled
    #[serde(default)]
    pub recall_count: u32,
    /// Last time this memory was accessed
    pub last_accessed: Option<DateTime<Utc>>,
}

impl MemoryEntry {
    /// Create a new memory entry
    pub fn new(entry_type: MemoryEntryType, content: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            entry_type,
            content,
            metadata: std::collections::HashMap::new(),
            importance: 0.5,
            recall_count: 0,
            last_accessed: None,
        }
    }

    /// Create a user message entry
    pub fn user_message(content: String) -> Self {
        Self::new(MemoryEntryType::UserMessage, content)
    }

    /// Create a persona response entry
    pub fn persona_response(content: String) -> Self {
        Self::new(MemoryEntryType::PersonaResponse, content)
    }

    /// Create a page content entry
    pub fn page_content(url: String, content: String) -> Self {
        let mut entry = Self::new(MemoryEntryType::PageContent { url: url.clone() }, content);
        entry.metadata.insert("url".to_string(), url);
        entry
    }

    /// Create a fact entry
    pub fn fact(content: String, importance: f32) -> Self {
        let mut entry = Self::new(MemoryEntryType::Fact, content);
        entry.importance = importance;
        entry
    }

    /// Mark this entry as accessed
    pub fn touch(&mut self) {
        self.recall_count += 1;
        self.last_accessed = Some(Utc::now());
    }
}

/// Types of memory entries
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum MemoryEntryType {
    /// User message in conversation
    UserMessage,
    /// Persona's response
    PersonaResponse,
    /// Content from a web page
    PageContent {
        url: String,
    },
    /// Extracted fact or knowledge
    Fact,
    /// User preference
    Preference,
    /// Session summary
    SessionSummary,
    /// Custom entry type
    Custom {
        kind: String,
    },
}

/// Configuration for persona memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Maximum entries in short-term memory
    pub max_short_term_entries: usize,
    /// Maximum entries in long-term memory
    pub max_long_term_entries: usize,
    /// Minimum importance score to persist to long-term
    pub long_term_threshold: f32,
    /// Auto-summarize sessions after this many entries
    pub summarize_after: usize,
    /// TTL for session memories (in seconds)
    pub session_ttl_secs: Option<u64>,
    /// Enable embedding-based retrieval
    pub use_embeddings: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_short_term_entries: 50,
            max_long_term_entries: 1000,
            long_term_threshold: 0.7,
            summarize_after: 100,
            session_ttl_secs: None,
            use_embeddings: false,
        }
    }
}

/// Persona memory container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaMemory {
    /// Persona this memory belongs to
    pub persona_id: PersonaId,
    /// Short-term memory (current session)
    pub short_term: Vec<MemoryEntry>,
    /// Long-term memory (persisted)
    pub long_term: Vec<MemoryEntry>,
    /// Configuration
    pub config: MemoryConfig,
    /// Memory statistics
    pub stats: MemoryStats,
}

impl PersonaMemory {
    /// Create new empty memory for a persona
    pub fn new(persona_id: PersonaId) -> Self {
        Self {
            persona_id,
            short_term: Vec::new(),
            long_term: Vec::new(),
            config: MemoryConfig::default(),
            stats: MemoryStats::default(),
        }
    }

    /// Create memory with custom config
    pub fn with_config(persona_id: PersonaId, config: MemoryConfig) -> Self {
        Self {
            persona_id,
            short_term: Vec::new(),
            long_term: Vec::new(),
            config,
            stats: MemoryStats::default(),
        }
    }

    /// Add a memory entry
    pub fn remember(&mut self, entry: MemoryEntry) {
        self.stats.total_entries += 1;

        // Add to short-term
        self.short_term.push(entry.clone());

        // Prune if too long
        while self.short_term.len() > self.config.max_short_term_entries {
            if let Some(removed) = self.short_term.remove(0) {
                // Consider promoting to long-term
                if removed.importance >= self.config.long_term_threshold {
                    self.promote_to_long_term(removed);
                }
            }
        }

        // Promote high-importance entries immediately
        if entry.importance >= self.config.long_term_threshold {
            self.promote_to_long_term(entry);
        }
    }

    /// Promote an entry to long-term memory
    fn promote_to_long_term(&mut self, entry: MemoryEntry) {
        // Check if already exists (by ID)
        if self.long_term.iter().any(|e| e.id == entry.id) {
            return;
        }

        self.long_term.push(entry);

        // Prune long-term if needed (remove oldest, lowest importance)
        while self.long_term.len() > self.config.max_long_term_entries {
            self.prune_long_term();
        }
    }

    /// Remove the least important entry from long-term memory
    fn prune_long_term(&mut self) {
        if self.long_term.is_empty() {
            return;
        }

        // Find entry with lowest importance and oldest timestamp
        let mut min_idx = 0;
        let mut min_score = f32::MAX;

        for (i, entry) in self.long_term.iter().enumerate() {
            // Score: importance + recency bonus
            let age_days = (Utc::now() - entry.timestamp).num_days() as f32;
            let recency_bonus = 0.1 / (1.0 + age_days * 0.01);
            let recall_bonus = 0.05 * (entry.recall_count as f32).min(10.0);
            let score = entry.importance + recency_bonus + recall_bonus;

            if score < min_score {
                min_score = score;
                min_idx = i;
            }
        }

        self.long_term.remove(min_idx);
        self.stats.pruned_entries += 1;
    }

    /// Recall memories relevant to a query
    pub fn recall(&mut self, query: &str, limit: usize) -> Vec<&MemoryEntry> {
        let mut results = Vec::new();

        // Simple keyword matching (would be replaced with embeddings)
        let query_lower = query.to_lowercase();
        let keywords: Vec<&str> = query_lower.split_whitespace().collect();

        // Search short-term first (more recent)
        for entry in self.short_term.iter().rev() {
            if self.entry_matches(&entry.content, &keywords) {
                results.push(entry);
                if results.len() >= limit / 2 {
                    break;
                }
            }
        }

        // Then search long-term
        for entry in self.long_term.iter().rev() {
            if self.entry_matches(&entry.content, &keywords) {
                results.push(entry);
                if results.len() >= limit {
                    break;
                }
            }
        }

        self.stats.recalls += 1;
        results
    }

    fn entry_matches(&self, content: &str, keywords: &[&str]) -> bool {
        let content_lower = content.to_lowercase();
        keywords.iter().any(|kw| content_lower.contains(kw))
    }

    /// Clear all short-term memory (session end)
    pub fn clear_session(&mut self) {
        self.short_term.clear();
        self.stats.sessions += 1;
    }

    /// Clear all memory
    pub fn clear_all(&mut self) {
        self.short_term.clear();
        self.long_term.clear();
        self.stats = MemoryStats::default();
    }

    /// Get recent conversation context
    pub fn recent_context(&self, limit: usize) -> Vec<&MemoryEntry> {
        self.short_term
            .iter()
            .rev()
            .filter(|e| matches!(
                e.entry_type,
                MemoryEntryType::UserMessage | MemoryEntryType::PersonaResponse
            ))
            .take(limit)
            .collect()
    }

    /// Serialize memory for persistence (to be encrypted by Cipher)
    pub fn serialize(&self) -> Result<Vec<u8>, crate::GrimoireError> {
        serde_json::to_vec(self).map_err(|e| crate::GrimoireError::ParseError(e.to_string()))
    }

    /// Deserialize memory from bytes
    pub fn deserialize(data: &[u8]) -> Result<Self, crate::GrimoireError> {
        serde_json::from_slice(data).map_err(|e| crate::GrimoireError::ParseError(e.to_string()))
    }
}

/// Memory statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryStats {
    /// Total entries ever added
    pub total_entries: u64,
    /// Entries pruned due to limits
    pub pruned_entries: u64,
    /// Number of recall operations
    pub recalls: u64,
    /// Number of sessions
    pub sessions: u64,
}

/// Memory search query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    /// Text query
    pub text: String,
    /// Filter by entry types
    pub entry_types: Option<Vec<String>>,
    /// Filter by time range (start)
    pub from: Option<DateTime<Utc>>,
    /// Filter by time range (end)
    pub to: Option<DateTime<Utc>>,
    /// Minimum importance
    pub min_importance: Option<f32>,
    /// Maximum results
    pub limit: usize,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            text: String::new(),
            entry_types: None,
            from: None,
            to: None,
            min_importance: None,
            limit: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_entry_creation() {
        let entry = MemoryEntry::user_message("Hello, world!".to_string());
        assert!(matches!(entry.entry_type, MemoryEntryType::UserMessage));
        assert_eq!(entry.content, "Hello, world!");
    }

    #[test]
    fn test_memory_remember_and_recall() {
        let persona_id = crate::PersonaId::from_name("test");
        let mut memory = PersonaMemory::new(persona_id);

        memory.remember(MemoryEntry::user_message("What is Rust?".to_string()));
        memory.remember(MemoryEntry::persona_response("Rust is a systems programming language.".to_string()));

        let results = memory.recall("Rust", 5);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_memory_pruning() {
        let persona_id = crate::PersonaId::from_name("test");
        let mut config = MemoryConfig::default();
        config.max_short_term_entries = 3;

        let mut memory = PersonaMemory::with_config(persona_id, config);

        for i in 0..5 {
            memory.remember(MemoryEntry::user_message(format!("Message {}", i)));
        }

        assert_eq!(memory.short_term.len(), 3);
    }
}
