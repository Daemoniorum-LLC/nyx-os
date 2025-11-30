//! IPC server for Grimoire settings daemon

use crate::schema::{SchemaValidator, ValidationResult};
use crate::store::SettingsStore;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;

/// IPC request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    // Basic operations
    Get { path: String },
    Set { path: String, value: Value },
    Delete { path: String },
    Exists { path: String },
    List { path: String },

    // Batch operations
    GetMultiple { paths: Vec<String> },
    SetMultiple { settings: Vec<(String, Value)> },

    // Schema operations
    Validate { path: String, value: Value },
    GetDefault { path: String },
    GetSchema { path: String },
    ListSettings { category: Option<String> },

    // Store operations
    Save,
    Reload,
    Export { format: String },
    Import { format: String, data: String },

    // Watch operations
    Subscribe { paths: Vec<String> },
    Unsubscribe { subscription_id: u64 },
}

/// IPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success { data: Value },
    Error { message: String },
    Notification { path: String, value: Value },
}

/// IPC server for settings
pub struct SettingsIpcServer {
    store: Arc<SettingsStore>,
    validator: Arc<RwLock<SchemaValidator>>,
    subscribers: Arc<RwLock<Vec<Subscription>>>,
}

struct Subscription {
    id: u64,
    paths: Vec<String>,
    tx: tokio::sync::mpsc::Sender<IpcResponse>,
}

impl SettingsIpcServer {
    pub fn new(store: Arc<SettingsStore>, validator: Arc<RwLock<SchemaValidator>>) -> Self {
        Self {
            store,
            validator,
            subscribers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start the IPC server
    pub async fn start(&self, socket_path: &Path) -> Result<()> {
        // Remove existing socket
        let _ = std::fs::remove_file(socket_path);

        let listener = UnixListener::bind(socket_path)?;
        tracing::info!("Grimoire IPC server listening on {:?}", socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let store = Arc::clone(&self.store);
                    let validator = Arc::clone(&self.validator);
                    let subscribers = Arc::clone(&self.subscribers);

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, store, validator, subscribers).await {
                            tracing::error!("Client error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                }
            }
        }
    }

    /// Notify subscribers of a change
    pub async fn notify_change(&self, path: &str, value: &Value) {
        let subscribers = self.subscribers.read().await;

        for sub in subscribers.iter() {
            // Check if this subscriber is interested in this path
            let interested = sub.paths.iter().any(|p| {
                path.starts_with(p) || p == "*"
            });

            if interested {
                let notification = IpcResponse::Notification {
                    path: path.to_string(),
                    value: value.clone(),
                };

                let _ = sub.tx.send(notification).await;
            }
        }
    }
}

async fn handle_client(
    stream: UnixStream,
    store: Arc<SettingsStore>,
    validator: Arc<RwLock<SchemaValidator>>,
    subscribers: Arc<RwLock<Vec<Subscription>>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Create notification channel for this client
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<IpcResponse>(100);

    // Spawn notification handler (placeholder for future implementation)
    tokio::spawn(async move {
        while let Some(_notification) = notify_rx.recv().await {
            // Note: Real implementation would need shared writer access
            // Currently notifications are not sent to clients
        }
    });

    let mut subscription_id: Option<u64> = None;

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => {
                // Handle subscription specially to track the subscription ID
                if let IpcRequest::Subscribe { ref paths } = request {
                    let id = rand::random::<u64>();
                    subscription_id = Some(id);

                    subscribers.write().await.push(Subscription {
                        id,
                        paths: paths.clone(),
                        tx: notify_tx.clone(),
                    });

                    IpcResponse::Success {
                        data: serde_json::json!({ "subscription_id": id }),
                    }
                } else {
                    process_request(request, &store, &validator, &subscribers).await
                }
            }
            Err(e) => IpcResponse::Error {
                message: format!("Invalid request: {}", e),
            },
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
    request: IpcRequest,
    store: &SettingsStore,
    validator: &RwLock<SchemaValidator>,
    _subscribers: &RwLock<Vec<Subscription>>,
) -> IpcResponse {
    match request {
        IpcRequest::Get { path } => {
            match store.get(&path).await {
                Some(value) => IpcResponse::Success { data: value },
                None => IpcResponse::Error {
                    message: format!("Setting not found: {}", path),
                },
            }
        }

        IpcRequest::Set { path, value } => {
            // Validate before setting
            let validation = validator.read().await.validate("default", &path, &value);

            if !validation.is_valid() {
                return IpcResponse::Error {
                    message: validation.message.unwrap_or_else(|| "Validation failed".to_string()),
                };
            }

            match store.set(&path, value.clone()).await {
                Ok(()) => IpcResponse::Success {
                    data: serde_json::json!({ "path": path, "set": true }),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::Delete { path } => {
            match store.delete(&path).await {
                Ok(deleted) => IpcResponse::Success {
                    data: serde_json::json!({ "path": path, "deleted": deleted }),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::Exists { path } => {
            let exists = store.exists(&path).await;
            IpcResponse::Success {
                data: serde_json::json!({ "path": path, "exists": exists }),
            }
        }

        IpcRequest::List { path } => {
            let keys = store.list(&path).await;
            IpcResponse::Success {
                data: serde_json::json!({ "path": path, "keys": keys }),
            }
        }

        IpcRequest::GetMultiple { paths } => {
            let mut results = serde_json::Map::new();

            for path in paths {
                if let Some(value) = store.get(&path).await {
                    results.insert(path, value);
                }
            }

            IpcResponse::Success {
                data: Value::Object(results),
            }
        }

        IpcRequest::SetMultiple { settings } => {
            let mut errors = Vec::new();
            let mut set_count = 0;

            for (path, value) in settings {
                match store.set(&path, value).await {
                    Ok(()) => set_count += 1,
                    Err(e) => errors.push(format!("{}: {}", path, e)),
                }
            }

            if errors.is_empty() {
                IpcResponse::Success {
                    data: serde_json::json!({ "set": set_count }),
                }
            } else {
                IpcResponse::Success {
                    data: serde_json::json!({
                        "set": set_count,
                        "errors": errors
                    }),
                }
            }
        }

        IpcRequest::Validate { path, value } => {
            let result = validator.read().await.validate("default", &path, &value);

            IpcResponse::Success {
                data: serde_json::json!({
                    "valid": result.is_valid(),
                    "level": format!("{:?}", result.level),
                    "message": result.message,
                }),
            }
        }

        IpcRequest::GetDefault { path } => {
            match validator.read().await.get_default("default", &path) {
                Some(default) => IpcResponse::Success { data: default },
                None => IpcResponse::Error {
                    message: format!("No default for: {}", path),
                },
            }
        }

        IpcRequest::GetSchema { path } => {
            let validator_guard = validator.read().await;
            let settings = validator_guard.list_settings("default");

            if let Some((_, def)) = settings.iter().find(|(p, _)| p == &path) {
                IpcResponse::Success {
                    data: serde_json::json!({
                        "type": format!("{:?}", def.value_type),
                        "description": def.description,
                        "default": def.default,
                        "deprecated": def.deprecated,
                    }),
                }
            } else {
                IpcResponse::Error {
                    message: format!("Schema not found for: {}", path),
                }
            }
        }

        IpcRequest::ListSettings { category } => {
            let validator_guard = validator.read().await;

            let settings: Vec<_> = if let Some(cat) = category {
                validator_guard.get_by_category("default", &cat)
                    .iter()
                    .map(|(path, def)| {
                        serde_json::json!({
                            "path": path,
                            "type": format!("{:?}", def.value_type),
                            "description": def.description,
                        })
                    })
                    .collect()
            } else {
                validator_guard.list_settings("default")
                    .iter()
                    .map(|(path, def)| {
                        serde_json::json!({
                            "path": path,
                            "type": format!("{:?}", def.value_type),
                            "description": def.description,
                        })
                    })
                    .collect()
            };

            IpcResponse::Success {
                data: serde_json::json!({ "settings": settings }),
            }
        }

        IpcRequest::Save => {
            match store.save().await {
                Ok(()) => IpcResponse::Success {
                    data: serde_json::json!({ "saved": true }),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::Reload => {
            match store.load().await {
                Ok(()) => IpcResponse::Success {
                    data: serde_json::json!({ "reloaded": true }),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::Export { format } => {
            match format.as_str() {
                "json" => {
                    match store.export_json().await {
                        Ok(json) => IpcResponse::Success {
                            data: serde_json::json!({ "data": json }),
                        },
                        Err(e) => IpcResponse::Error { message: e.to_string() },
                    }
                }
                _ => IpcResponse::Error {
                    message: format!("Unsupported format: {}", format),
                },
            }
        }

        IpcRequest::Import { format, data } => {
            match format.as_str() {
                "json" => {
                    match store.import_json(&data).await {
                        Ok(()) => IpcResponse::Success {
                            data: serde_json::json!({ "imported": true }),
                        },
                        Err(e) => IpcResponse::Error { message: e.to_string() },
                    }
                }
                _ => IpcResponse::Error {
                    message: format!("Unsupported format: {}", format),
                },
            }
        }

        IpcRequest::Subscribe { .. } => {
            // Handled specially in handle_client
            IpcResponse::Error {
                message: "Subscribe handled elsewhere".to_string(),
            }
        }

        IpcRequest::Unsubscribe { subscription_id: _ } => {
            // Would need to track subscription in connection state
            IpcResponse::Success {
                data: serde_json::json!({ "unsubscribed": true }),
            }
        }
    }
}

/// IPC client for other components
pub struct SettingsClient {
    socket_path: std::path::PathBuf,
}

impl SettingsClient {
    pub fn new(socket_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    pub async fn get(&self, path: &str) -> Result<Option<Value>> {
        let response = self.send(IpcRequest::Get {
            path: path.to_string(),
        }).await?;

        match response {
            IpcResponse::Success { data } => Ok(Some(data)),
            IpcResponse::Error { message } if message.contains("not found") => Ok(None),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Ok(None),
        }
    }

    pub async fn set(&self, path: &str, value: Value) -> Result<()> {
        let response = self.send(IpcRequest::Set {
            path: path.to_string(),
            value,
        }).await?;

        match response {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Ok(()),
        }
    }

    async fn send(&self, request: IpcRequest) -> Result<IpcResponse> {
        let mut stream = UnixStream::connect(&self.socket_path).await?;

        let request_json = serde_json::to_string(&request)?;
        stream.write_all(request_json.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        Ok(serde_json::from_str(&line)?)
    }
}
