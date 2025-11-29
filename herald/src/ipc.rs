//! IPC server for Herald

use crate::dnd::DndManager;
use crate::history::NotificationHistory;
use crate::notification::{CloseReason, Notification, NotificationQueue, Urgency};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;

/// IPC request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    // Notification operations
    Notify {
        app_name: String,
        summary: String,
        body: Option<String>,
        icon: Option<String>,
        urgency: Option<String>,
        timeout: Option<i32>,
    },
    CloseNotification { id: u32 },
    GetNotifications,
    GetNotification { id: u32 },

    // History operations
    GetHistory { limit: Option<usize> },
    GetHistoryByApp { app_name: String },
    SearchHistory { query: String },
    ClearHistory,
    GetHistoryStats,

    // DND operations
    GetDndStatus,
    EnableDnd,
    DisableDnd,
    EnableDndFor { minutes: u32 },
    ToggleDnd,

    // Action operations
    InvokeAction { id: u32, action_id: String },

    // Status
    GetCapabilities,
    GetServerInfo,
}

/// IPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success { data: serde_json::Value },
    Error { message: String },
}

/// IPC server
pub struct HeraldIpcServer {
    queue: Arc<RwLock<NotificationQueue>>,
    history: Arc<RwLock<NotificationHistory>>,
    dnd: Arc<DndManager>,
    action_tx: tokio::sync::mpsc::Sender<(u32, String)>,
}

impl HeraldIpcServer {
    pub fn new(
        queue: Arc<RwLock<NotificationQueue>>,
        history: Arc<RwLock<NotificationHistory>>,
        dnd: Arc<DndManager>,
        action_tx: tokio::sync::mpsc::Sender<(u32, String)>,
    ) -> Self {
        Self {
            queue,
            history,
            dnd,
            action_tx,
        }
    }

    pub async fn start(&self, socket_path: &Path) -> Result<()> {
        let _ = std::fs::remove_file(socket_path);

        let listener = UnixListener::bind(socket_path)?;
        tracing::info!("Herald IPC server listening on {:?}", socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let queue = Arc::clone(&self.queue);
                    let history = Arc::clone(&self.history);
                    let dnd = Arc::clone(&self.dnd);
                    let action_tx = self.action_tx.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, queue, history, dnd, action_tx).await {
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
}

async fn handle_client(
    stream: UnixStream,
    queue: Arc<RwLock<NotificationQueue>>,
    history: Arc<RwLock<NotificationHistory>>,
    dnd: Arc<DndManager>,
    action_tx: tokio::sync::mpsc::Sender<(u32, String)>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => process_request(request, &queue, &history, &dnd, &action_tx).await,
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

    Ok(())
}

async fn process_request(
    request: IpcRequest,
    queue: &RwLock<NotificationQueue>,
    history: &RwLock<NotificationHistory>,
    dnd: &DndManager,
    action_tx: &tokio::sync::mpsc::Sender<(u32, String)>,
) -> IpcResponse {
    match request {
        IpcRequest::Notify { app_name, summary, body, icon, urgency, timeout } => {
            let urgency = urgency.map(|u| match u.to_lowercase().as_str() {
                "low" => Urgency::Low,
                "critical" => Urgency::Critical,
                _ => Urgency::Normal,
            }).unwrap_or(Urgency::Normal);

            let mut notification = Notification::new(0, &app_name, &summary);
            notification.body = body;
            notification.app_icon = icon;
            notification.urgency = urgency;
            notification.timeout = timeout.unwrap_or(-1);

            // Check DND
            if !dnd.should_show(&notification).await {
                return IpcResponse::Success {
                    data: serde_json::json!({ "id": 0, "suppressed": true }),
                };
            }

            let id = queue.write().await.add(notification.clone());

            // Add to history
            history.write().await.add(notification);

            IpcResponse::Success {
                data: serde_json::json!({ "id": id }),
            }
        }

        IpcRequest::CloseNotification { id } => {
            queue.write().await.remove(id);
            history.write().await.record_close(id, CloseReason::Closed, None);

            IpcResponse::Success {
                data: serde_json::json!({ "closed": id }),
            }
        }

        IpcRequest::GetNotifications => {
            let queue_guard = queue.read().await;
            let notifications: Vec<_> = queue_guard.all().iter().map(|n| {
                serde_json::json!({
                    "id": n.id,
                    "app_name": n.app_name,
                    "summary": n.summary,
                    "body": n.body,
                    "icon": n.app_icon,
                    "urgency": format!("{:?}", n.urgency),
                })
            }).collect();

            IpcResponse::Success {
                data: serde_json::json!({ "notifications": notifications }),
            }
        }

        IpcRequest::GetNotification { id } => {
            let queue_guard = queue.read().await;
            if let Some(n) = queue_guard.get(id) {
                IpcResponse::Success {
                    data: serde_json::json!({
                        "id": n.id,
                        "app_name": n.app_name,
                        "summary": n.summary,
                        "body": n.body,
                        "icon": n.app_icon,
                        "urgency": format!("{:?}", n.urgency),
                        "actions": n.actions.iter().map(|a| {
                            serde_json::json!({ "id": a.id, "label": a.label })
                        }).collect::<Vec<_>>(),
                    }),
                }
            } else {
                IpcResponse::Error {
                    message: format!("Notification not found: {}", id),
                }
            }
        }

        IpcRequest::GetHistory { limit } => {
            let history_guard = history.read().await;
            let entries: Vec<_> = history_guard.all()
                .into_iter()
                .take(limit.unwrap_or(100))
                .map(|e| {
                    serde_json::json!({
                        "id": e.notification.id,
                        "app_name": e.notification.app_name,
                        "summary": e.notification.summary,
                        "body": e.notification.body,
                        "timestamp": e.displayed_at,
                        "closed_at": e.closed_at,
                    })
                })
                .collect();

            IpcResponse::Success {
                data: serde_json::json!({ "history": entries }),
            }
        }

        IpcRequest::GetHistoryByApp { app_name } => {
            let history_guard = history.read().await;
            let entries: Vec<_> = history_guard.by_app(&app_name)
                .into_iter()
                .map(|e| {
                    serde_json::json!({
                        "id": e.notification.id,
                        "summary": e.notification.summary,
                        "timestamp": e.displayed_at,
                    })
                })
                .collect();

            IpcResponse::Success {
                data: serde_json::json!({ "history": entries }),
            }
        }

        IpcRequest::SearchHistory { query } => {
            let history_guard = history.read().await;
            let entries: Vec<_> = history_guard.search(&query)
                .into_iter()
                .map(|e| {
                    serde_json::json!({
                        "id": e.notification.id,
                        "app_name": e.notification.app_name,
                        "summary": e.notification.summary,
                        "body": e.notification.body,
                    })
                })
                .collect();

            IpcResponse::Success {
                data: serde_json::json!({ "results": entries }),
            }
        }

        IpcRequest::ClearHistory => {
            history.write().await.clear();
            IpcResponse::Success {
                data: serde_json::json!({ "cleared": true }),
            }
        }

        IpcRequest::GetHistoryStats => {
            let stats = history.read().await.stats();
            IpcResponse::Success {
                data: serde_json::json!({
                    "total": stats.total,
                    "today": stats.today,
                    "critical": stats.critical,
                    "unread": stats.unread,
                    "by_app": stats.by_app,
                }),
            }
        }

        IpcRequest::GetDndStatus => {
            let status = dnd.status().await;
            IpcResponse::Success {
                data: serde_json::json!({
                    "active": status.active,
                    "reason": format!("{:?}", status.reason),
                    "until": status.until.map(|u| u.to_rfc3339()),
                    "allow_critical": status.allow_critical,
                }),
            }
        }

        IpcRequest::EnableDnd => {
            dnd.enable().await;
            IpcResponse::Success {
                data: serde_json::json!({ "enabled": true }),
            }
        }

        IpcRequest::DisableDnd => {
            dnd.disable().await;
            IpcResponse::Success {
                data: serde_json::json!({ "disabled": true }),
            }
        }

        IpcRequest::EnableDndFor { minutes } => {
            dnd.enable_for(minutes).await;
            IpcResponse::Success {
                data: serde_json::json!({ "enabled_for": minutes }),
            }
        }

        IpcRequest::ToggleDnd => {
            let enabled = dnd.toggle().await;
            IpcResponse::Success {
                data: serde_json::json!({ "enabled": enabled }),
            }
        }

        IpcRequest::InvokeAction { id, action_id } => {
            let _ = action_tx.send((id, action_id.clone())).await;
            history.write().await.record_close(id, CloseReason::ActionInvoked, Some(action_id));

            IpcResponse::Success {
                data: serde_json::json!({ "invoked": true }),
            }
        }

        IpcRequest::GetCapabilities => {
            IpcResponse::Success {
                data: serde_json::json!({
                    "capabilities": [
                        "actions", "body", "body-hyperlinks", "body-markup",
                        "icon-static", "persistence", "sound"
                    ]
                }),
            }
        }

        IpcRequest::GetServerInfo => {
            IpcResponse::Success {
                data: serde_json::json!({
                    "name": "Herald",
                    "vendor": "Nyx",
                    "version": "0.1.0",
                    "spec_version": "1.2"
                }),
            }
        }
    }
}

/// IPC client
pub struct HeraldClient {
    socket_path: std::path::PathBuf,
}

impl HeraldClient {
    pub fn new(socket_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    pub async fn notify(&self, app_name: &str, summary: &str, body: Option<&str>) -> Result<u32> {
        let response = self.send(IpcRequest::Notify {
            app_name: app_name.to_string(),
            summary: summary.to_string(),
            body: body.map(|s| s.to_string()),
            icon: None,
            urgency: None,
            timeout: None,
        }).await?;

        match response {
            IpcResponse::Success { data } => {
                data.get("id")
                    .and_then(|v| v.as_u64())
                    .map(|id| id as u32)
                    .ok_or_else(|| anyhow::anyhow!("No ID in response"))
            }
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn close(&self, id: u32) -> Result<()> {
        let response = self.send(IpcRequest::CloseNotification { id }).await?;

        match response {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn toggle_dnd(&self) -> Result<bool> {
        let response = self.send(IpcRequest::ToggleDnd).await?;

        match response {
            IpcResponse::Success { data } => {
                Ok(data.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false))
            }
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
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
