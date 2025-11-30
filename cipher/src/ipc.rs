//! IPC interface for Cipher daemon

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{info, error, debug};

use crate::state::CipherState;
use crate::crypto::Secret;
use crate::keyring::SearchAttributes;

/// IPC request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    /// Initialize keyring with master password
    Initialize { password: String },

    /// Unlock keyring
    Unlock { password: String },

    /// Lock keyring
    Lock,

    /// Get keyring status
    Status,

    /// Create session
    OpenSession,

    /// Close session
    CloseSession { token: String },

    /// List collections
    ListCollections,

    /// Create collection
    CreateCollection { name: String, label: String },

    /// List items in collection
    ListItems { collection: String },

    /// Store secret
    StoreSecret {
        collection: String,
        id: String,
        label: String,
        secret: String,
        attributes: HashMap<String, String>,
    },

    /// Get secret
    GetSecret {
        collection: String,
        id: String,
        session: String,
    },

    /// Delete secret
    DeleteSecret {
        collection: String,
        id: String,
    },

    /// Search secrets
    Search {
        collection: String,
        attributes: HashMap<String, String>,
    },
}

/// IPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success { message: String },
    Session { token: String },
    Secret { value: String },
    Collections(Vec<CollectionInfo>),
    Items(Vec<ItemInfo>),
    SearchResults(Vec<ItemInfo>),
    Status {
        initialized: bool,
        locked: bool,
        collections: usize,
        sessions: usize,
    },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionInfo {
    pub name: String,
    pub label: String,
    pub locked: bool,
    pub item_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemInfo {
    pub id: String,
    pub label: String,
    pub attributes: HashMap<String, String>,
}

/// IPC server
pub struct CipherServer {
    socket_path: String,
    state: Arc<RwLock<CipherState>>,
}

impl CipherServer {
    pub fn new(socket_path: &str, state: Arc<RwLock<CipherState>>) -> Self {
        Self {
            socket_path: socket_path.to_string(),
            state,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let _ = std::fs::remove_file(&self.socket_path);

        if let Some(parent) = std::path::Path::new(&self.socket_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;

        // Restrict socket permissions
        std::fs::set_permissions(
            &self.socket_path,
            std::os::unix::fs::PermissionsExt::from_mode(0o600),
        )?;

        info!("Cipher IPC listening on {}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let state = self.state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, state).await {
                            error!("Client error: {}", e);
                        }
                    });
                }
                Err(e) => error!("Accept error: {}", e),
            }
        }
    }
}

async fn handle_client(
    stream: UnixStream,
    state: Arc<RwLock<CipherState>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => process_request(request, &state).await,
            Err(e) => IpcResponse::Error { message: e.to_string() },
        };

        let json = serde_json::to_string(&response)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        line.clear();
    }

    Ok(())
}

async fn process_request(
    request: IpcRequest,
    state: &RwLock<CipherState>,
) -> IpcResponse {
    match request {
        IpcRequest::Initialize { password } => {
            let mut state = state.write().await;
            match state.keyring.initialize(&password) {
                Ok(()) => IpcResponse::Success {
                    message: "Keyring initialized".to_string(),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::Unlock { password } => {
            let mut state = state.write().await;
            match state.keyring.unlock(&password) {
                Ok(()) => IpcResponse::Success {
                    message: "Keyring unlocked".to_string(),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::Lock => {
            let mut state = state.write().await;
            state.keyring.lock();
            IpcResponse::Success {
                message: "Keyring locked".to_string(),
            }
        }

        IpcRequest::Status => {
            let state = state.read().await;
            IpcResponse::Status {
                initialized: state.keyring.list_collections().len() > 0,
                locked: !state.keyring.is_unlocked(),
                collections: state.keyring.list_collections().len(),
                sessions: state.sessions.active_count(),
            }
        }

        IpcRequest::OpenSession => {
            let mut state = state.write().await;
            let token = state.sessions.create_session(None, None);
            IpcResponse::Session { token: token.to_string() }
        }

        IpcRequest::CloseSession { token } => {
            let mut state = state.write().await;
            state.sessions.close_session(&token);
            IpcResponse::Success {
                message: "Session closed".to_string(),
            }
        }

        IpcRequest::ListCollections => {
            let state = state.read().await;
            let collections: Vec<CollectionInfo> = state.keyring.list_collections()
                .iter()
                .map(|c| CollectionInfo {
                    name: c.name.clone(),
                    label: c.label.clone(),
                    locked: c.locked,
                    item_count: 0, // Would need to count items
                })
                .collect();
            IpcResponse::Collections(collections)
        }

        IpcRequest::CreateCollection { name, label } => {
            let mut state = state.write().await;
            match state.keyring.create_collection(&name, &label) {
                Ok(()) => IpcResponse::Success {
                    message: format!("Collection '{}' created", name),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::ListItems { collection } => {
            let state = state.read().await;
            match state.keyring.list_items(&collection) {
                Ok(items) => {
                    let infos: Vec<ItemInfo> = items.iter()
                        .map(|i| ItemInfo {
                            id: i.id.clone(),
                            label: i.label.clone(),
                            attributes: i.attributes.clone(),
                        })
                        .collect();
                    IpcResponse::Items(infos)
                }
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::StoreSecret { collection, id, label, secret, attributes } => {
            let mut state = state.write().await;
            let secret = Secret::from_str(&secret);
            match state.keyring.store_secret(&collection, &id, &label, &secret, attributes) {
                Ok(()) => IpcResponse::Success {
                    message: "Secret stored".to_string(),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::GetSecret { collection, id, session } => {
            let mut state = state.write().await;

            // Validate session
            if let Err(e) = state.sessions.validate(&session) {
                return IpcResponse::Error { message: e.to_string() };
            }

            match state.keyring.get_secret(&collection, &id) {
                Ok(secret) => {
                    match secret.as_str() {
                        Ok(s) => IpcResponse::Secret { value: s.to_string() },
                        Err(e) => IpcResponse::Error { message: e.to_string() },
                    }
                }
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::DeleteSecret { collection, id } => {
            let mut state = state.write().await;
            match state.keyring.delete_secret(&collection, &id) {
                Ok(()) => IpcResponse::Success {
                    message: "Secret deleted".to_string(),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::Search { collection, attributes } => {
            let state = state.read().await;
            let attrs = SearchAttributes { attributes };
            match state.keyring.search(&collection, &attrs) {
                Ok(items) => {
                    let infos: Vec<ItemInfo> = items.iter()
                        .map(|i| ItemInfo {
                            id: i.id.clone(),
                            label: i.label.clone(),
                            attributes: i.attributes.clone(),
                        })
                        .collect();
                    IpcResponse::SearchResults(infos)
                }
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }
    }
}
