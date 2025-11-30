//! # Grimoire Client
//!
//! Client library for communicating with the DaemonOS Grimoire daemon.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use grimoire_client::GrimoireClient;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Connect to the daemon
//!     let client = GrimoireClient::connect("/run/grimoire/grimoire.sock").await?;
//!
//!     // List all personas
//!     let personas = client.list_personas().await?;
//!     for persona in personas {
//!         println!("Persona: {}", persona.name);
//!     }
//!
//!     // Get a specific persona
//!     let lilith = client.get_persona_by_name("Lilith").await?;
//!
//!     // Add memory
//!     client.add_memory(
//!         lilith.id,
//!         grimoire_core::MemoryEntry::user_message("Hello!".to_string()),
//!     ).await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Mock Mode
//!
//! For development without DaemonOS, enable the `mock` feature:
//!
//! ```toml
//! grimoire-client = { path = "...", features = ["mock"] }
//! ```
//!
//! This provides a built-in mock that doesn't require the daemon.

use std::path::Path;
use std::sync::Arc;

use grimoire_core::{
    GrimoireRequest, GrimoireResponse, ResponseData, ErrorCode,
    Persona, PersonaId, PersonaMemory, MemoryEntry, MemoryQuery,
    Ritual, RitualId, RitualExecution, DaemonStatus, PersonaEvent,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tracing::{debug, warn};

/// Error types for the client
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Daemon error: {0}")]
    DaemonError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result type for client operations
pub type Result<T> = std::result::Result<T, ClientError>;

/// Client for the Grimoire daemon
pub struct GrimoireClient {
    stream: Arc<Mutex<BufReader<UnixStream>>>,
    socket_path: String,
}

impl GrimoireClient {
    /// Connect to the Grimoire daemon
    pub async fn connect(socket_path: impl AsRef<Path>) -> Result<Self> {
        let path = socket_path.as_ref();
        let stream = UnixStream::connect(path).await.map_err(|e| {
            ClientError::ConnectionFailed(format!(
                "Failed to connect to {:?}: {}",
                path, e
            ))
        })?;

        debug!("Connected to Grimoire daemon at {:?}", path);

        Ok(Self {
            stream: Arc::new(Mutex::new(BufReader::new(stream))),
            socket_path: path.to_string_lossy().to_string(),
        })
    }

    /// Connect with default socket path
    pub async fn connect_default() -> Result<Self> {
        Self::connect("/run/grimoire/grimoire.sock").await
    }

    /// Send a request and receive a response
    async fn request(&self, request: GrimoireRequest) -> Result<GrimoireResponse> {
        let mut stream = self.stream.lock().await;

        // Serialize and send request
        let request_json = serde_json::to_string(&request)
            .map_err(|e| ClientError::ParseError(e.to_string()))?;

        stream.get_mut().write_all(request_json.as_bytes()).await?;
        stream.get_mut().write_all(b"\n").await?;
        stream.get_mut().flush().await?;

        // Read response
        let mut line = String::new();
        stream.read_line(&mut line).await?;

        // Parse response
        let response: GrimoireResponse = serde_json::from_str(&line)
            .map_err(|e| ClientError::ParseError(e.to_string()))?;

        Ok(response)
    }

    /// Extract data from a response or return an error
    fn extract_response<T, F>(response: GrimoireResponse, extractor: F) -> Result<T>
    where
        F: FnOnce(ResponseData) -> Option<T>,
    {
        match response {
            GrimoireResponse::Success { data } => {
                extractor(data).ok_or_else(|| {
                    ClientError::DaemonError("Unexpected response type".to_string())
                })
            }
            GrimoireResponse::Error { code, message } => {
                Err(match code {
                    ErrorCode::NotFound => ClientError::NotFound(message),
                    ErrorCode::PermissionDenied => ClientError::PermissionDenied(message),
                    ErrorCode::AlreadyExists => ClientError::AlreadyExists(message),
                    _ => ClientError::DaemonError(message),
                })
            }
            GrimoireResponse::Event { .. } => {
                Err(ClientError::DaemonError("Unexpected event response".to_string()))
            }
        }
    }

    // ========== Persona Operations ==========

    /// List all personas
    pub async fn list_personas(&self) -> Result<Vec<Persona>> {
        let response = self.request(GrimoireRequest::ListPersonas).await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Personas(personas) = data {
                Some(personas)
            } else {
                None
            }
        })
    }

    /// Get a persona by ID
    pub async fn get_persona(&self, id: PersonaId) -> Result<Persona> {
        let response = self.request(GrimoireRequest::GetPersona { id }).await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Persona(persona) = data {
                Some(persona)
            } else {
                None
            }
        })
    }

    /// Get a persona by name
    pub async fn get_persona_by_name(&self, name: &str) -> Result<Persona> {
        let response = self
            .request(GrimoireRequest::GetPersonaByName {
                name: name.to_string(),
            })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Persona(persona) = data {
                Some(persona)
            } else {
                None
            }
        })
    }

    /// Register a new persona
    pub async fn register_persona(&self, persona: Persona) -> Result<PersonaId> {
        let response = self
            .request(GrimoireRequest::RegisterPersona { persona })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::PersonaId(id) = data {
                Some(id)
            } else {
                None
            }
        })
    }

    /// Update an existing persona
    pub async fn update_persona(&self, persona: Persona) -> Result<()> {
        let response = self
            .request(GrimoireRequest::UpdatePersona { persona })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Empty = data {
                Some(())
            } else {
                None
            }
        })
    }

    /// Remove a persona
    pub async fn remove_persona(&self, id: PersonaId) -> Result<()> {
        let response = self.request(GrimoireRequest::RemovePersona { id }).await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Empty = data {
                Some(())
            } else {
                None
            }
        })
    }

    /// Get built-in personas
    pub async fn get_builtin_personas(&self) -> Result<Vec<Persona>> {
        let response = self.request(GrimoireRequest::GetBuiltinPersonas).await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Personas(personas) = data {
                Some(personas)
            } else {
                None
            }
        })
    }

    // ========== Memory Operations ==========

    /// Get memory for a persona
    pub async fn get_memory(&self, persona_id: PersonaId) -> Result<PersonaMemory> {
        let response = self
            .request(GrimoireRequest::GetMemory { persona_id })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Memory(memory) = data {
                Some(memory)
            } else {
                None
            }
        })
    }

    /// Add a memory entry
    pub async fn add_memory(&self, persona_id: PersonaId, entry: MemoryEntry) -> Result<()> {
        let response = self
            .request(GrimoireRequest::AddMemory { persona_id, entry })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Empty = data {
                Some(())
            } else {
                None
            }
        })
    }

    /// Recall memories matching a query
    pub async fn recall_memory(
        &self,
        persona_id: PersonaId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        let response = self
            .request(GrimoireRequest::RecallMemory {
                persona_id,
                query: MemoryQuery {
                    text: query.to_string(),
                    limit,
                    ..Default::default()
                },
            })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::MemoryEntries(entries) = data {
                Some(entries)
            } else {
                None
            }
        })
    }

    /// Clear session memory
    pub async fn clear_session_memory(&self, persona_id: PersonaId) -> Result<()> {
        let response = self
            .request(GrimoireRequest::ClearSessionMemory { persona_id })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Empty = data {
                Some(())
            } else {
                None
            }
        })
    }

    /// Clear all memory
    pub async fn clear_all_memory(&self, persona_id: PersonaId) -> Result<()> {
        let response = self
            .request(GrimoireRequest::ClearAllMemory { persona_id })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Empty = data {
                Some(())
            } else {
                None
            }
        })
    }

    /// Persist memory to disk
    pub async fn persist_memory(&self, persona_id: PersonaId) -> Result<()> {
        let response = self
            .request(GrimoireRequest::PersistMemory { persona_id })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Empty = data {
                Some(())
            } else {
                None
            }
        })
    }

    // ========== Ritual Operations ==========

    /// List all rituals
    pub async fn list_rituals(&self) -> Result<Vec<Ritual>> {
        let response = self.request(GrimoireRequest::ListRituals).await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Rituals(rituals) = data {
                Some(rituals)
            } else {
                None
            }
        })
    }

    /// Get rituals for a persona
    pub async fn list_persona_rituals(&self, persona_id: PersonaId) -> Result<Vec<Ritual>> {
        let response = self
            .request(GrimoireRequest::ListPersonaRituals { persona_id })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Rituals(rituals) = data {
                Some(rituals)
            } else {
                None
            }
        })
    }

    /// Execute a ritual
    pub async fn execute_ritual(
        &self,
        ritual_id: RitualId,
        parameters: std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<RitualExecution> {
        let response = self
            .request(GrimoireRequest::ExecuteRitual {
                ritual_id,
                parameters,
            })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Execution(execution) = data {
                Some(execution)
            } else {
                None
            }
        })
    }

    /// Get ritual execution status
    pub async fn get_ritual_execution(&self, execution_id: uuid::Uuid) -> Result<RitualExecution> {
        let response = self
            .request(GrimoireRequest::GetRitualExecution { execution_id })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Execution(execution) = data {
                Some(execution)
            } else {
                None
            }
        })
    }

    /// Cancel a running ritual
    pub async fn cancel_ritual(&self, execution_id: uuid::Uuid) -> Result<()> {
        let response = self
            .request(GrimoireRequest::CancelRitual { execution_id })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Empty = data {
                Some(())
            } else {
                None
            }
        })
    }

    // ========== Settings Operations ==========

    /// Get a setting value
    pub async fn get_setting(&self, path: &str) -> Result<serde_json::Value> {
        let response = self
            .request(GrimoireRequest::GetSetting {
                path: path.to_string(),
            })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Setting(value) = data {
                Some(value)
            } else {
                None
            }
        })
    }

    /// Set a setting value
    pub async fn set_setting(&self, path: &str, value: serde_json::Value) -> Result<()> {
        let response = self
            .request(GrimoireRequest::SetSetting {
                path: path.to_string(),
                value,
            })
            .await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Empty = data {
                Some(())
            } else {
                None
            }
        })
    }

    // ========== System Operations ==========

    /// Get daemon status
    pub async fn get_status(&self) -> Result<DaemonStatus> {
        let response = self.request(GrimoireRequest::GetStatus).await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Status(status) = data {
                Some(status)
            } else {
                None
            }
        })
    }

    /// Ping the daemon
    pub async fn ping(&self) -> Result<i64> {
        let response = self.request(GrimoireRequest::Ping).await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Pong { timestamp } = data {
                Some(timestamp)
            } else {
                None
            }
        })
    }

    /// Get daemon version
    pub async fn get_version(&self) -> Result<(String, String)> {
        let response = self.request(GrimoireRequest::GetVersion).await?;
        Self::extract_response(response, |data| {
            if let ResponseData::Version { version, build } = data {
                Some((version, build))
            } else {
                None
            }
        })
    }

    /// Check if daemon is healthy
    pub async fn is_healthy(&self) -> bool {
        match self.get_status().await {
            Ok(status) => status.healthy,
            Err(_) => false,
        }
    }
}

/// Mock client for testing without the daemon
#[cfg(feature = "mock")]
pub mod mock {
    use super::*;
    use std::sync::RwLock;

    /// Mock Grimoire client that stores everything in memory
    pub struct MockGrimoireClient {
        personas: RwLock<Vec<Persona>>,
        memories: RwLock<std::collections::HashMap<PersonaId, PersonaMemory>>,
    }

    impl MockGrimoireClient {
        /// Create a new mock client with built-in personas
        pub fn new() -> Self {
            let client = Self {
                personas: RwLock::new(grimoire_core::builtin::all()),
                memories: RwLock::new(std::collections::HashMap::new()),
            };

            // Initialize memories for built-in personas
            for persona in grimoire_core::builtin::all() {
                client.memories.write().unwrap().insert(
                    persona.id,
                    PersonaMemory::new(persona.id),
                );
            }

            client
        }

        pub fn list_personas(&self) -> Vec<Persona> {
            self.personas.read().unwrap().clone()
        }

        pub fn get_persona(&self, id: PersonaId) -> Option<Persona> {
            self.personas.read().unwrap().iter().find(|p| p.id == id).cloned()
        }

        pub fn get_persona_by_name(&self, name: &str) -> Option<Persona> {
            let name_lower = name.to_lowercase();
            self.personas
                .read()
                .unwrap()
                .iter()
                .find(|p| p.name.to_lowercase() == name_lower)
                .cloned()
        }

        pub fn add_memory(&self, persona_id: PersonaId, entry: MemoryEntry) {
            if let Some(memory) = self.memories.write().unwrap().get_mut(&persona_id) {
                memory.remember(entry);
            }
        }

        pub fn get_memory(&self, persona_id: PersonaId) -> Option<PersonaMemory> {
            self.memories.read().unwrap().get(&persona_id).cloned()
        }
    }

    impl Default for MockGrimoireClient {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "mock")]
    #[test]
    fn test_mock_client() {
        let client = mock::MockGrimoireClient::new();

        let personas = client.list_personas();
        assert!(personas.len() >= 3);

        let lilith = client.get_persona_by_name("Lilith");
        assert!(lilith.is_some());
    }
}
