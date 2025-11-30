//! IPC message types for Grimoire daemon communication
//!
//! These types define the protocol between clients (like Sitra) and
//! the DaemonOS Grimoire daemon.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    Persona, PersonaId, PersonaMemory, MemoryEntry, MemoryQuery,
    Ritual, RitualId, RitualExecution,
};

/// Request types for Grimoire IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum GrimoireRequest {
    // ========== Persona Operations ==========

    /// List all available personas
    ListPersonas,

    /// Get a specific persona by ID
    GetPersona { id: PersonaId },

    /// Get a persona by name
    GetPersonaByName { name: String },

    /// Register a new persona
    RegisterPersona { persona: Persona },

    /// Update an existing persona
    UpdatePersona { persona: Persona },

    /// Remove a persona
    RemovePersona { id: PersonaId },

    /// Get all built-in personas
    GetBuiltinPersonas,

    // ========== Memory Operations ==========

    /// Get memory for a persona
    GetMemory { persona_id: PersonaId },

    /// Add a memory entry
    AddMemory {
        persona_id: PersonaId,
        entry: MemoryEntry,
    },

    /// Recall memories matching a query
    RecallMemory {
        persona_id: PersonaId,
        query: MemoryQuery,
    },

    /// Clear session memory for a persona
    ClearSessionMemory { persona_id: PersonaId },

    /// Clear all memory for a persona
    ClearAllMemory { persona_id: PersonaId },

    /// Persist memory to Cipher-encrypted storage
    PersistMemory { persona_id: PersonaId },

    // ========== Ritual Operations ==========

    /// List all rituals
    ListRituals,

    /// List rituals for a specific persona
    ListPersonaRituals { persona_id: PersonaId },

    /// Get a ritual by ID
    GetRitual { id: RitualId },

    /// Get a ritual by name
    GetRitualByName { name: String },

    /// Register a new ritual
    RegisterRitual { ritual: Ritual },

    /// Remove a ritual
    RemoveRitual { id: RitualId },

    /// Execute a ritual
    ExecuteRitual {
        ritual_id: RitualId,
        parameters: std::collections::HashMap<String, Value>,
    },

    /// Get ritual execution status
    GetRitualExecution { execution_id: uuid::Uuid },

    /// Cancel a running ritual
    CancelRitual { execution_id: uuid::Uuid },

    /// List active ritual executions
    ListActiveRituals,

    // ========== Settings Operations ==========

    /// Get a setting value
    GetSetting { path: String },

    /// Set a setting value
    SetSetting { path: String, value: Value },

    /// Get multiple settings
    GetSettings { paths: Vec<String> },

    /// List all settings
    ListSettings { category: Option<String> },

    // ========== Subscription Operations ==========

    /// Subscribe to persona events
    SubscribePersona { persona_id: PersonaId },

    /// Subscribe to all grimoire events
    SubscribeAll,

    /// Unsubscribe from events
    Unsubscribe { subscription_id: u64 },

    // ========== System Operations ==========

    /// Get daemon status
    GetStatus,

    /// Reload all personas and rituals from disk
    Reload,

    /// Get daemon version
    GetVersion,

    /// Health check
    Ping,
}

/// Response types for Grimoire IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GrimoireResponse {
    /// Successful response with data
    Success { data: ResponseData },

    /// Error response
    Error { code: ErrorCode, message: String },

    /// Event notification (for subscriptions)
    Event { event: PersonaEvent },
}

/// Response data types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ResponseData {
    /// Empty success (for operations like delete)
    Empty,

    /// Single persona
    Persona(Persona),

    /// List of personas
    Personas(Vec<Persona>),

    /// Persona ID (for registration)
    PersonaId(PersonaId),

    /// Persona memory
    Memory(PersonaMemory),

    /// List of memory entries
    MemoryEntries(Vec<MemoryEntry>),

    /// Single ritual
    Ritual(Ritual),

    /// List of rituals
    Rituals(Vec<Ritual>),

    /// Ritual ID (for registration)
    RitualId(RitualId),

    /// Ritual execution status
    Execution(RitualExecution),

    /// List of executions
    Executions(Vec<RitualExecution>),

    /// Setting value
    Setting(Value),

    /// Multiple settings
    Settings(std::collections::HashMap<String, Value>),

    /// Subscription ID
    Subscription { id: u64 },

    /// Status info
    Status(DaemonStatus),

    /// Version info
    Version { version: String, build: String },

    /// Pong response
    Pong { timestamp: i64 },
}

/// Error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Resource not found
    NotFound,
    /// Invalid request
    InvalidRequest,
    /// Permission denied
    PermissionDenied,
    /// Resource already exists
    AlreadyExists,
    /// Validation failed
    ValidationError,
    /// Internal error
    InternalError,
    /// Operation timed out
    Timeout,
    /// Service unavailable
    Unavailable,
    /// Rate limited
    RateLimited,
}

/// Events that can be subscribed to
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PersonaEvent {
    /// Persona was registered
    PersonaRegistered { persona: Persona },

    /// Persona was updated
    PersonaUpdated { persona: Persona },

    /// Persona was removed
    PersonaRemoved { id: PersonaId },

    /// Memory was added
    MemoryAdded {
        persona_id: PersonaId,
        entry: MemoryEntry,
    },

    /// Memory was cleared
    MemoryCleared {
        persona_id: PersonaId,
        scope: String,  // "session" or "all"
    },

    /// Ritual started
    RitualStarted {
        execution_id: uuid::Uuid,
        ritual_id: RitualId,
    },

    /// Ritual step completed
    RitualProgress {
        execution_id: uuid::Uuid,
        step: usize,
        total_steps: usize,
    },

    /// Ritual completed
    RitualCompleted {
        execution_id: uuid::Uuid,
        result: Option<Value>,
    },

    /// Ritual failed
    RitualFailed {
        execution_id: uuid::Uuid,
        error: String,
    },

    /// Setting changed
    SettingChanged {
        path: String,
        old_value: Option<Value>,
        new_value: Value,
    },
}

/// Daemon status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    /// Whether the daemon is healthy
    pub healthy: bool,
    /// Number of registered personas
    pub persona_count: usize,
    /// Number of registered rituals
    pub ritual_count: usize,
    /// Number of active ritual executions
    pub active_executions: usize,
    /// Uptime in seconds
    pub uptime_secs: u64,
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// Whether Cipher integration is available
    pub cipher_available: bool,
}

impl GrimoireResponse {
    /// Create a success response with data
    pub fn success(data: ResponseData) -> Self {
        Self::Success { data }
    }

    /// Create an empty success response
    pub fn ok() -> Self {
        Self::Success { data: ResponseData::Empty }
    }

    /// Create an error response
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::Error {
            code,
            message: message.into(),
        }
    }

    /// Create a not found error
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::error(ErrorCode::NotFound, message)
    }

    /// Create an internal error
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::error(ErrorCode::InternalError, message)
    }

    /// Check if this is a success response
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }

    /// Check if this is an error response
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    /// Get the error message if this is an error
    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Error { message, .. } => Some(message),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let request = GrimoireRequest::ListPersonas;
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("list_personas"));

        let parsed: GrimoireRequest = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, GrimoireRequest::ListPersonas));
    }

    #[test]
    fn test_response_serialization() {
        let response = GrimoireResponse::ok();
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("success"));

        let error = GrimoireResponse::not_found("Persona not found");
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("not_found"));
    }
}
