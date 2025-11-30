//! Ritual definitions - automated multi-step workflows
//!
//! Rituals are sequences of steps that personas can execute
//! to accomplish complex tasks like research, price tracking, etc.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::PersonaId;

/// Unique identifier for a ritual
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RitualId(Uuid);

impl RitualId {
    /// Create a new random ritual ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a deterministic ID from a name
    pub fn from_name(name: &str) -> Self {
        let hash = blake3::hash(name.as_bytes());
        let bytes: [u8; 16] = hash.as_bytes()[..16].try_into().unwrap();
        Self(Uuid::from_bytes(bytes))
    }
}

impl Default for RitualId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RitualId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A ritual definition (multi-step workflow)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ritual {
    /// Unique identifier
    pub id: RitualId,
    /// Human-readable name
    pub name: String,
    /// Description of what this ritual does
    pub description: String,
    /// Persona that executes this ritual
    pub persona_id: PersonaId,
    /// Version
    pub version: semver::Version,
    /// Input parameters required
    pub parameters: Vec<RitualParameter>,
    /// Sequence of steps
    pub steps: Vec<RitualStep>,
    /// Triggers that can start this ritual
    pub triggers: Vec<RitualTrigger>,
    /// Maximum execution time (seconds)
    pub timeout_secs: u64,
    /// Whether this ritual can run in background
    pub background: bool,
}

impl Ritual {
    /// Load ritual from a .ritual file
    pub fn from_file(path: &std::path::Path) -> Result<Self, crate::GrimoireError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
    }

    /// Parse ritual from TOML string
    pub fn from_toml(content: &str) -> Result<Self, crate::GrimoireError> {
        toml::from_str(content).map_err(|e| crate::GrimoireError::ParseError(e.to_string()))
    }

    /// Serialize ritual to TOML
    pub fn to_toml(&self) -> Result<String, crate::GrimoireError> {
        toml::to_string_pretty(self).map_err(|e| crate::GrimoireError::ParseError(e.to_string()))
    }
}

/// Parameter for a ritual
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RitualParameter {
    /// Parameter name
    pub name: String,
    /// Description
    pub description: String,
    /// Parameter type
    pub param_type: ParameterType,
    /// Whether this parameter is required
    pub required: bool,
    /// Default value (if not required)
    pub default: Option<serde_json::Value>,
}

/// Types of ritual parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterType {
    String,
    Number,
    Boolean,
    Url,
    Selector,
    List { item_type: Box<ParameterType> },
}

/// A single step in a ritual
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RitualStep {
    /// Navigate to a URL
    Navigate {
        /// URL (can contain {{variables}})
        url: String,
        /// Wait for page load
        #[serde(default = "default_true")]
        wait_for_load: bool,
    },

    /// Wait for an element to appear
    WaitFor {
        /// CSS selector
        selector: String,
        /// Timeout in milliseconds
        timeout_ms: u64,
        /// Continue even if element not found
        #[serde(default)]
        optional: bool,
    },

    /// Extract content from page
    Extract {
        /// CSS selector
        selector: String,
        /// Variable name to store result
        variable: String,
        /// Extraction mode
        #[serde(default)]
        mode: ExtractionMode,
    },

    /// Click an element
    Click {
        /// CSS selector
        selector: String,
    },

    /// Type text into an input
    Type {
        /// CSS selector
        selector: String,
        /// Text to type (can contain {{variables}})
        text: String,
        /// Clear input first
        #[serde(default)]
        clear_first: bool,
    },

    /// Ask the persona a question
    AskPersona {
        /// Prompt (can contain {{variables}})
        prompt: String,
        /// Variable name to store response
        variable: String,
        /// Maximum tokens in response
        max_tokens: Option<u32>,
    },

    /// Conditional branching
    If {
        /// Condition expression
        condition: String,
        /// Steps if condition is true
        then_steps: Vec<RitualStep>,
        /// Steps if condition is false
        #[serde(default)]
        else_steps: Vec<RitualStep>,
    },

    /// Loop over items
    ForEach {
        /// Variable containing items
        items: String,
        /// Variable name for current item
        variable: String,
        /// Index variable name
        #[serde(default = "default_index_var")]
        index_var: String,
        /// Steps to execute for each item
        steps: Vec<RitualStep>,
        /// Maximum iterations
        max_iterations: Option<usize>,
    },

    /// Wait/delay
    Delay {
        /// Milliseconds to wait
        ms: u64,
    },

    /// Log a message
    Log {
        /// Message (can contain {{variables}})
        message: String,
        /// Log level
        #[serde(default)]
        level: LogLevel,
    },

    /// Notify the user
    Notify {
        /// Notification title
        title: String,
        /// Notification message
        message: String,
        /// Notification type
        #[serde(default)]
        notification_type: NotificationType,
    },

    /// Store a value
    SetVariable {
        /// Variable name
        name: String,
        /// Value (can contain {{variables}})
        value: String,
    },

    /// Execute JavaScript (sandboxed)
    ExecuteScript {
        /// JavaScript code
        script: String,
        /// Variable to store result
        variable: Option<String>,
    },

    /// Take a screenshot
    Screenshot {
        /// Variable to store screenshot path
        variable: String,
        /// Selector to screenshot (or full page if None)
        selector: Option<String>,
    },

    /// Assert a condition (fails ritual if false)
    Assert {
        /// Condition expression
        condition: String,
        /// Error message if assertion fails
        message: String,
    },

    /// Return a value and end the ritual
    Return {
        /// Value to return
        value: String,
    },
}

fn default_true() -> bool {
    true
}

fn default_index_var() -> String {
    "_index".to_string()
}

/// How to extract content from an element
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionMode {
    /// Get text content
    #[default]
    Text,
    /// Get inner HTML
    Html,
    /// Get an attribute value
    Attribute { name: String },
    /// Get all matching elements
    All,
}

/// Log levels
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

/// Notification types
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

/// Triggers that can start a ritual
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RitualTrigger {
    /// Manual invocation only
    Manual,

    /// Scheduled execution
    Schedule {
        /// Cron expression
        cron: String,
    },

    /// When visiting a matching page
    PageMatch {
        /// URL pattern (glob or regex)
        url_pattern: String,
        /// Use regex instead of glob
        #[serde(default)]
        regex: bool,
    },

    /// On keyword command
    Keyword {
        /// Trigger keyword
        keyword: String,
    },

    /// On receiving a system event
    Event {
        /// Event name
        event: String,
    },
}

/// State of a ritual execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RitualExecution {
    /// Execution ID
    pub id: Uuid,
    /// Ritual being executed
    pub ritual_id: RitualId,
    /// Current status
    pub status: ExecutionStatus,
    /// Current step index
    pub current_step: usize,
    /// Variables
    pub variables: std::collections::HashMap<String, serde_json::Value>,
    /// Start time
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// End time
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Error message if failed
    pub error: Option<String>,
    /// Return value if completed
    pub result: Option<serde_json::Value>,
}

impl RitualExecution {
    /// Create a new execution for a ritual
    pub fn new(ritual_id: RitualId) -> Self {
        Self {
            id: Uuid::new_v4(),
            ritual_id,
            status: ExecutionStatus::Pending,
            current_step: 0,
            variables: std::collections::HashMap::new(),
            started_at: chrono::Utc::now(),
            ended_at: None,
            error: None,
            result: None,
        }
    }
}

/// Status of a ritual execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ritual_id_deterministic() {
        let id1 = RitualId::from_name("deep_research");
        let id2 = RitualId::from_name("deep_research");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_ritual_step_serialization() {
        let step = RitualStep::Navigate {
            url: "https://example.com".to_string(),
            wait_for_load: true,
        };

        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("navigate"));

        let parsed: RitualStep = serde_json::from_str(&json).unwrap();
        if let RitualStep::Navigate { url, .. } = parsed {
            assert_eq!(url, "https://example.com");
        } else {
            panic!("Wrong variant");
        }
    }
}
