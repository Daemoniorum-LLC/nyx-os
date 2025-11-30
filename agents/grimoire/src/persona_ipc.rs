//! Unified IPC server for Grimoire daemon
//!
//! Handles both persona operations and settings operations.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use grimoire_core::{
    GrimoireRequest, GrimoireResponse, ResponseData, ErrorCode, PersonaEvent,
    MemoryQuery,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

use crate::GrimoireDaemon;

/// Unified Grimoire IPC server
pub struct UnifiedGrimoireServer {
    /// Socket path
    socket_path: PathBuf,
    /// Daemon state
    daemon: Arc<GrimoireDaemon>,
    /// Event subscribers
    subscribers: Arc<RwLock<Vec<Subscription>>>,
}

struct Subscription {
    id: u64,
    persona_filter: Option<grimoire_core::PersonaId>,
    tx: tokio::sync::mpsc::Sender<GrimoireResponse>,
}

impl UnifiedGrimoireServer {
    /// Create a new server
    pub fn new(socket_path: PathBuf, daemon: Arc<GrimoireDaemon>) -> Self {
        Self {
            socket_path,
            daemon,
            subscribers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Run the server
    pub async fn run(&self) -> Result<()> {
        // Remove existing socket
        let _ = tokio::fs::remove_file(&self.socket_path).await;

        let listener = UnixListener::bind(&self.socket_path)?;
        info!("Grimoire IPC server listening on {:?}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let daemon = Arc::clone(&self.daemon);
                    let subscribers = Arc::clone(&self.subscribers);

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, daemon, subscribers).await {
                            error!("Client error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }

    /// Broadcast an event to all subscribers
    pub async fn broadcast_event(&self, event: PersonaEvent) {
        let subscribers = self.subscribers.read().await;

        for sub in subscribers.iter() {
            // Check if this subscriber is interested
            let interested = match &event {
                PersonaEvent::PersonaRegistered { persona } |
                PersonaEvent::PersonaUpdated { persona } => {
                    sub.persona_filter.map_or(true, |id| id == persona.id)
                }
                PersonaEvent::PersonaRemoved { id } |
                PersonaEvent::MemoryAdded { persona_id: id, .. } |
                PersonaEvent::MemoryCleared { persona_id: id, .. } => {
                    sub.persona_filter.map_or(true, |filter| filter == *id)
                }
                _ => true, // All other events go to everyone
            };

            if interested {
                let response = GrimoireResponse::Event { event: event.clone() };
                let _ = sub.tx.send(response).await;
            }
        }
    }
}

async fn handle_client(
    stream: UnixStream,
    daemon: Arc<GrimoireDaemon>,
    subscribers: Arc<RwLock<Vec<Subscription>>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Create notification channel for this client
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<GrimoireResponse>(100);

    let mut subscription_id: Option<u64> = None;

    // Spawn notification sender
    tokio::spawn(async move {
        while let Some(notification) = notify_rx.recv().await {
            // Note: In a real implementation, we'd need proper synchronization
            // This is simplified for now
            debug!("Would send notification: {:?}", notification);
        }
    });

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<GrimoireRequest>(&line) {
            Ok(request) => {
                debug!("Received request: {:?}", request);

                // Handle subscription specially
                if let GrimoireRequest::SubscribePersona { persona_id } = &request {
                    let id = rand::random::<u64>();
                    subscription_id = Some(id);

                    subscribers.write().await.push(Subscription {
                        id,
                        persona_filter: Some(*persona_id),
                        tx: notify_tx.clone(),
                    });

                    GrimoireResponse::success(ResponseData::Subscription { id })
                } else if matches!(request, GrimoireRequest::SubscribeAll) {
                    let id = rand::random::<u64>();
                    subscription_id = Some(id);

                    subscribers.write().await.push(Subscription {
                        id,
                        persona_filter: None,
                        tx: notify_tx.clone(),
                    });

                    GrimoireResponse::success(ResponseData::Subscription { id })
                } else {
                    process_request(request, &daemon).await
                }
            }
            Err(e) => {
                warn!("Invalid request: {}", e);
                GrimoireResponse::error(ErrorCode::InvalidRequest, format!("Parse error: {}", e))
            }
        };

        let response_json = serde_json::to_string(&response)?;
        writer.write_all(response_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        line.clear();
    }

    // Clean up subscription on disconnect
    if let Some(id) = subscription_id {
        subscribers.write().await.retain(|s| s.id != id);
    }

    Ok(())
}

async fn process_request(
    request: GrimoireRequest,
    daemon: &GrimoireDaemon,
) -> GrimoireResponse {
    match request {
        // ========== Persona Operations ==========

        GrimoireRequest::ListPersonas => {
            let personas = daemon.persona_store.list_personas().await;
            GrimoireResponse::success(ResponseData::Personas(personas))
        }

        GrimoireRequest::GetPersona { id } => {
            match daemon.persona_store.get_persona(id).await {
                Some(persona) => GrimoireResponse::success(ResponseData::Persona(persona)),
                None => GrimoireResponse::not_found(format!("Persona not found: {}", id)),
            }
        }

        GrimoireRequest::GetPersonaByName { name } => {
            match daemon.persona_store.get_persona_by_name(&name).await {
                Some(persona) => GrimoireResponse::success(ResponseData::Persona(persona)),
                None => GrimoireResponse::not_found(format!("Persona not found: {}", name)),
            }
        }

        GrimoireRequest::RegisterPersona { persona } => {
            match daemon.persona_store.register_persona(persona).await {
                Ok(id) => GrimoireResponse::success(ResponseData::PersonaId(id)),
                Err(e) => GrimoireResponse::error(ErrorCode::AlreadyExists, e.to_string()),
            }
        }

        GrimoireRequest::UpdatePersona { persona } => {
            match daemon.persona_store.update_persona(persona).await {
                Ok(()) => GrimoireResponse::ok(),
                Err(e) => GrimoireResponse::error(ErrorCode::InternalError, e.to_string()),
            }
        }

        GrimoireRequest::RemovePersona { id } => {
            match daemon.persona_store.remove_persona(id).await {
                Ok(()) => GrimoireResponse::ok(),
                Err(e) => GrimoireResponse::error(ErrorCode::InternalError, e.to_string()),
            }
        }

        GrimoireRequest::GetBuiltinPersonas => {
            let personas = daemon.persona_store.get_builtin_personas();
            GrimoireResponse::success(ResponseData::Personas(personas))
        }

        // ========== Memory Operations ==========

        GrimoireRequest::GetMemory { persona_id } => {
            match daemon.persona_store.get_memory(persona_id).await {
                Some(memory) => GrimoireResponse::success(ResponseData::Memory(memory)),
                None => GrimoireResponse::not_found(format!("Memory not found for: {}", persona_id)),
            }
        }

        GrimoireRequest::AddMemory { persona_id, entry } => {
            match daemon.persona_store.add_memory(persona_id, entry).await {
                Ok(()) => GrimoireResponse::ok(),
                Err(e) => GrimoireResponse::error(ErrorCode::InternalError, e.to_string()),
            }
        }

        GrimoireRequest::RecallMemory { persona_id, query } => {
            let entries = daemon.persona_store.recall_memory(
                persona_id,
                &query.text,
                query.limit,
            ).await;
            GrimoireResponse::success(ResponseData::MemoryEntries(entries))
        }

        GrimoireRequest::ClearSessionMemory { persona_id } => {
            match daemon.persona_store.clear_session_memory(persona_id).await {
                Ok(()) => GrimoireResponse::ok(),
                Err(e) => GrimoireResponse::error(ErrorCode::InternalError, e.to_string()),
            }
        }

        GrimoireRequest::ClearAllMemory { persona_id } => {
            match daemon.persona_store.clear_all_memory(persona_id).await {
                Ok(()) => GrimoireResponse::ok(),
                Err(e) => GrimoireResponse::error(ErrorCode::InternalError, e.to_string()),
            }
        }

        GrimoireRequest::PersistMemory { persona_id } => {
            match daemon.persona_store.persist_memory(persona_id).await {
                Ok(()) => GrimoireResponse::ok(),
                Err(e) => GrimoireResponse::error(ErrorCode::InternalError, e.to_string()),
            }
        }

        // ========== Ritual Operations ==========

        GrimoireRequest::ListRituals => {
            let rituals = daemon.ritual_store.read().await.list_rituals();
            GrimoireResponse::success(ResponseData::Rituals(rituals))
        }

        GrimoireRequest::ListPersonaRituals { persona_id } => {
            let rituals = daemon.ritual_store.read().await.list_persona_rituals(persona_id);
            GrimoireResponse::success(ResponseData::Rituals(rituals))
        }

        GrimoireRequest::GetRitual { id } => {
            match daemon.ritual_store.read().await.get_ritual(id) {
                Some(ritual) => GrimoireResponse::success(ResponseData::Ritual(ritual)),
                None => GrimoireResponse::not_found(format!("Ritual not found: {}", id)),
            }
        }

        GrimoireRequest::GetRitualByName { name } => {
            match daemon.ritual_store.read().await.get_ritual_by_name(&name) {
                Some(ritual) => GrimoireResponse::success(ResponseData::Ritual(ritual)),
                None => GrimoireResponse::not_found(format!("Ritual not found: {}", name)),
            }
        }

        GrimoireRequest::RegisterRitual { ritual } => {
            match daemon.ritual_store.write().await.register_ritual(ritual).await {
                Ok(id) => GrimoireResponse::success(ResponseData::RitualId(id)),
                Err(e) => GrimoireResponse::error(ErrorCode::AlreadyExists, e.to_string()),
            }
        }

        GrimoireRequest::RemoveRitual { id } => {
            match daemon.ritual_store.write().await.remove_ritual(id).await {
                Ok(()) => GrimoireResponse::ok(),
                Err(e) => GrimoireResponse::error(ErrorCode::InternalError, e.to_string()),
            }
        }

        GrimoireRequest::ExecuteRitual { ritual_id, parameters } => {
            match daemon.ritual_store.write().await.start_execution(ritual_id, parameters) {
                Ok(execution_id) => {
                    // TODO: Actually execute the ritual steps in background
                    let execution = daemon.ritual_store.read().await.get_execution(execution_id);
                    match execution {
                        Some(exec) => GrimoireResponse::success(ResponseData::Execution(exec)),
                        None => GrimoireResponse::internal_error("Failed to get execution"),
                    }
                }
                Err(e) => GrimoireResponse::error(ErrorCode::InternalError, e.to_string()),
            }
        }

        GrimoireRequest::GetRitualExecution { execution_id } => {
            match daemon.ritual_store.read().await.get_execution(execution_id) {
                Some(execution) => GrimoireResponse::success(ResponseData::Execution(execution)),
                None => GrimoireResponse::not_found(format!("Execution not found: {}", execution_id)),
            }
        }

        GrimoireRequest::CancelRitual { execution_id } => {
            match daemon.ritual_store.write().await.cancel_execution(execution_id) {
                Ok(()) => GrimoireResponse::ok(),
                Err(e) => GrimoireResponse::error(ErrorCode::InternalError, e.to_string()),
            }
        }

        GrimoireRequest::ListActiveRituals => {
            let executions = daemon.ritual_store.read().await.list_active_executions();
            GrimoireResponse::success(ResponseData::Executions(executions))
        }

        // ========== Settings Operations ==========

        GrimoireRequest::GetSetting { path } => {
            match daemon.settings_store.read().await.get(&path).await {
                Some(value) => GrimoireResponse::success(ResponseData::Setting(value)),
                None => GrimoireResponse::not_found(format!("Setting not found: {}", path)),
            }
        }

        GrimoireRequest::SetSetting { path, value } => {
            match daemon.settings_store.write().await.set(&path, value).await {
                Ok(()) => GrimoireResponse::ok(),
                Err(e) => GrimoireResponse::error(ErrorCode::InternalError, e.to_string()),
            }
        }

        GrimoireRequest::GetSettings { paths } => {
            let mut settings = std::collections::HashMap::new();
            let store = daemon.settings_store.read().await;

            for path in paths {
                if let Some(value) = store.get(&path).await {
                    settings.insert(path, value);
                }
            }

            GrimoireResponse::success(ResponseData::Settings(settings))
        }

        GrimoireRequest::ListSettings { category: _ } => {
            let settings = daemon.settings_store.read().await.flatten().await;
            GrimoireResponse::success(ResponseData::Settings(settings))
        }

        // ========== System Operations ==========

        GrimoireRequest::GetStatus => {
            let status = daemon.status().await;
            GrimoireResponse::success(ResponseData::Status(status))
        }

        GrimoireRequest::Reload => {
            // TODO: Implement reload
            GrimoireResponse::ok()
        }

        GrimoireRequest::GetVersion => {
            GrimoireResponse::success(ResponseData::Version {
                version: env!("CARGO_PKG_VERSION").to_string(),
                build: option_env!("GIT_HASH").unwrap_or("dev").to_string(),
            })
        }

        GrimoireRequest::Ping => {
            GrimoireResponse::success(ResponseData::Pong {
                timestamp: chrono::Utc::now().timestamp(),
            })
        }

        // ========== Subscription Operations ==========
        // Handled in handle_client

        GrimoireRequest::SubscribePersona { .. } |
        GrimoireRequest::SubscribeAll |
        GrimoireRequest::Unsubscribe { .. } => {
            GrimoireResponse::error(ErrorCode::InternalError, "Subscription handled elsewhere")
        }
    }
}
